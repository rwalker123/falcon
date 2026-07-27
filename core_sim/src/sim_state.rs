//! `SimState` — the simulation's **checkpoint**, and a different thing from `WorldSnapshot`.
//!
//! `WorldSnapshot` is the **client view**: fog-filtered herds, derived rasters, display-only
//! forecast fields, everything shaped by what a player needs to see. It was also, until this
//! module, the thing rollback restored from — and that is the root defect behind every divergence
//! this arc measured. Thirteen mutable resources had no representation in it at all, because no
//! client ever needed them, and nothing failed when they were left out.
//!
//! So the two are now separate. `WorldSnapshot` stays exactly what it is and stops pretending;
//! `SimState` carries what a turn actually reads.
//!
//! ## Three rules this format holds
//!
//! **1. No `Entity` crosses a checkpoint.** Restoring despawns and respawns everything, so bevy
//! hands back fresh generations. Every reference here is a *stable sim id*: tiles and settlements
//! by `(x, y)`, logistics links by their endpoint pair, power nodes by the tile they ride on, bands
//! by [`BandId`]. The `Entity` fields inside cloned components are overwritten on restore and are
//! set to [`Entity::PLACEHOLDER`] at capture so nothing can read a stale one by accident.
//!
//! **2. No config.** Three sim-state types hold configuration —
//! [`InfluentialRoster::checkpoint`], [`KnowledgeLedger::checkpoint`] and
//! [`CultureManager::checkpoint`] exist precisely to leave it behind. Cloning them whole would
//! capture the tuning that was live when the checkpoint was taken, so a rollback would silently
//! reinstall it: hot-reload a config, roll back, and the reload is undone with nothing logged.
//! Restore re-attaches whatever config is live now.
//!
//! **3. Capture is a pure function of the world.** [`capture_sim_state`] takes `&World` and
//! reads nothing else — no change detection, no retained deltas, no assumption that it ran last
//! turn. That is what keeps "materialize a checkpoint every Nth turn" a scheduling change rather
//! than a rewrite.
//!
//! ## Serialization is deliberately absent
//!
//! Nothing here derives `Serialize`. The checkpoint is an in-memory `Clone`, which is all an
//! in-process rollback needs, and it is what let this land without a 59-type serde migration whose
//! only consumer would be a save-file feature that does not exist yet. Two facts about that
//! migration, established by measurement and worth not rediscovering:
//!
//! - **17 sim-state maps are keyed by `FactionId`, `UVec2` or tuples**, which `serde_json` cannot
//!   represent — it admits only string keys. The repo's only serde codec today is JSON, in
//!   `sim_schema`.
//! - The sim-state closure is **119 types** and contains **no trait objects, function pointers,
//!   closures, raw pointers, interior mutability, lock types or manual `Drop` impls**. The only
//!   constructs serde could not derive through were a `SmallRng` (deleted — the roster draws from
//!   derived seeds now) and `Entity` (rule 1 above).

use bevy::prelude::*;
use bevy::utils::HashMap;

use crate::{
    components::{
        BandId, BandTravel, Expedition, LaborAllocation, LogisticsLink, PopulationCohort,
        PowerNode, ResidentBand, Settlement, StartingUnit, Tile, TownCenter, TradeLink,
    },
    crisis::{ActiveCrisisLedger, CrisisTelemetry},
    culture::{CultureManager, CultureManagerCheckpoint},
    espionage::{
        CounterIntelBudgets, EspionageMissionState, EspionageRoster, FactionSecurityPolicies,
    },
    fauna::HerdRegistry,
    food::FoodModuleTag,
    forage::ForageRegistry,
    graze::GrazeRegistry,
    great_discovery::{GreatDiscoveryLedger, GreatDiscoveryReadiness, GreatDiscoveryTelemetry},
    influencers::{InfluencerImpacts, InfluentialRoster, InfluentialRosterCheckpoint},
    knowledge_ledger::{KnowledgeLedger, KnowledgeLedgerCheckpoint},
    resources::{
        BandIdAllocator, CapabilityFlags, CommandEventLog, CorruptionLedgers, CorruptionTelemetry,
        DiscoveryProgressLedger, FactionInventory, PendingCrisisSeeds, PendingCrisisSpawns,
        SentimentAxisBias, SimulationTick, TradeTelemetry,
    },
    sedentarization::SedentarizationScore,
    sites::{DiscoveredSites, SiteTag},
    telling::BeatLedger,
    victory::VictoryState,
    visibility::{VisibilityLedger, VisibilitySweepTracker},
};

/// One tile, keyed by its position.
#[derive(Debug, Clone)]
pub struct TileRecord {
    pub tile: Tile,
    /// The tile's power node, if it has one. Carries `base_generation` / `base_demand`, which no
    /// `WorldSnapshot` ever did — restore used to set `base = current`, so the next turn re-applied
    /// modifiers to an already-modified base and every node drifted immediately.
    pub power: Option<PowerNode>,
    /// Worldgen tile tags. Restore never rebuilt these, which is why a restored world reported
    /// `forage_patches[].per_worker_yield == 0`.
    pub food_module: Option<FoodModuleTag>,
    pub site: Option<SiteTag>,
}

/// One logistics link, keyed by its endpoint tiles.
#[derive(Debug, Clone)]
pub struct LinkRecord {
    pub from: UVec2,
    pub to: UVec2,
    pub link: LogisticsLink,
    pub trade: TradeLink,
}

/// An in-flight expedition, with its home band named by id rather than by entity.
#[derive(Debug, Clone)]
pub struct ExpeditionRecord {
    pub home_band: BandId,
    pub expedition: Expedition,
}

/// One band, keyed by [`BandId`].
#[derive(Debug, Clone)]
pub struct BandRecord {
    pub id: BandId,
    /// The cohort. Its `home` / `current_tile` are [`Entity::PLACEHOLDER`]; the real positions are
    /// the two fields below.
    pub cohort: PopulationCohort,
    pub home: UVec2,
    pub current: UVec2,
    pub labor: LaborAllocation,
    pub resident: bool,
    pub starting_unit: Option<StartingUnit>,
    /// A pending `move_band` order. **Carried**, because a checkpoint is lossless: a band that was
    /// mid-move at tick T was mid-move at tick T, and restoring it is what reproduces that world.
    /// The component's own doc once said a rollback cancels the travel — that described the
    /// consequence of it not being persisted, not an intent.
    pub travel: Option<BandTravel>,
    pub expedition: Option<ExpeditionRecord>,
}

/// A settlement and its town centre.
#[derive(Debug, Clone)]
pub struct SettlementRecord {
    pub settlement: Settlement,
    pub town_center: Option<TownCenter>,
}

/// The simulation's state at one tick.
#[derive(Debug, Clone)]
pub struct SimState {
    pub tick: SimulationTick,
    pub tiles: Vec<TileRecord>,
    pub links: Vec<LinkRecord>,
    pub bands: Vec<BandRecord>,
    pub settlements: Vec<SettlementRecord>,

    // --- resources, cloned whole ---
    pub band_ids: BandIdAllocator,
    pub active_crises: ActiveCrisisLedger,
    pub beat_ledger: BeatLedger,
    pub capability_flags: CapabilityFlags,
    pub command_events: CommandEventLog,
    pub corruption: CorruptionLedgers,
    pub corruption_telemetry: CorruptionTelemetry,
    pub counter_intel: CounterIntelBudgets,
    pub crisis_telemetry: CrisisTelemetry,
    pub discovered_sites: DiscoveredSites,
    pub discovery_progress: DiscoveryProgressLedger,
    pub espionage_missions: EspionageMissionState,
    pub espionage_roster: EspionageRoster,
    pub faction_inventory: FactionInventory,
    pub security_policies: FactionSecurityPolicies,
    pub forage: ForageRegistry,
    pub graze: GrazeRegistry,
    pub great_discoveries: GreatDiscoveryLedger,
    pub great_discovery_readiness: GreatDiscoveryReadiness,
    pub great_discovery_telemetry: GreatDiscoveryTelemetry,
    pub herds: HerdRegistry,
    pub influencer_impacts: InfluencerImpacts,
    pub pending_crisis_seeds: PendingCrisisSeeds,
    pub pending_crisis_spawns: PendingCrisisSpawns,
    pub sedentarization: SedentarizationScore,
    pub sentiment_bias: SentimentAxisBias,
    pub trade_telemetry: TradeTelemetry,
    pub victory: VictoryState,
    pub visibility: VisibilityLedger,

    // --- resources whose config is deliberately left behind ---
    pub culture: CultureManagerCheckpoint,
    pub influencers: InfluentialRosterCheckpoint,
    pub knowledge: KnowledgeLedgerCheckpoint,

    /// Previous-turn positions for the visibility corridor sweep, re-keyed from `Entity` to
    /// [`BandId`] so they survive the respawn.
    pub sweep_positions: Vec<(BandId, UVec2)>,
}

/// Read a checkpoint out of the world.
///
/// Pure: it takes `&World` and consults nothing else, so it is correct whether it runs every turn
/// or every hundredth.
pub fn capture_sim_state(world: &World) -> SimState {
    let mut tiles: Vec<TileRecord> = world
        .iter_entities()
        .filter_map(|entity| {
            let tile = entity.get::<Tile>()?;
            Some(TileRecord {
                tile: tile.clone(),
                power: entity.get::<PowerNode>().cloned(),
                food_module: entity.get::<FoodModuleTag>().cloned(),
                site: entity.get::<SiteTag>().cloned(),
            })
        })
        .collect();
    tiles.sort_by_key(|record| (record.tile.position.y, record.tile.position.x));

    // Tile entity -> position, so links can name their endpoints by coordinate.
    let tile_positions: HashMap<Entity, UVec2> = world
        .iter_entities()
        .filter_map(|entity| Some((entity.id(), entity.get::<Tile>()?.position)))
        .collect();

    let mut links: Vec<LinkRecord> = world
        .iter_entities()
        .filter_map(|entity| {
            let link = entity.get::<LogisticsLink>()?;
            let from = *tile_positions.get(&link.from)?;
            let to = *tile_positions.get(&link.to)?;
            let mut stored = link.clone();
            stored.from = Entity::PLACEHOLDER;
            stored.to = Entity::PLACEHOLDER;
            Some(LinkRecord {
                from,
                to,
                link: stored,
                trade: entity.get::<TradeLink>().cloned().unwrap_or_default(),
            })
        })
        .collect();
    links.sort_by_key(|record| (record.from.y, record.from.x, record.to.y, record.to.x));

    // Band entity -> id, so an expedition's `home_band` and the sweep tracker can name bands.
    let band_ids_by_entity: HashMap<Entity, BandId> = world
        .iter_entities()
        .filter_map(|entity| Some((entity.id(), *entity.get::<BandId>()?)))
        .collect();

    let mut bands: Vec<BandRecord> = world
        .iter_entities()
        .filter_map(|entity| {
            let cohort = entity.get::<PopulationCohort>()?;
            let id = *entity.get::<BandId>()?;
            let home = tile_positions
                .get(&cohort.home)
                .copied()
                .unwrap_or_default();
            let current = tile_positions
                .get(&cohort.current_tile)
                .copied()
                .unwrap_or(home);
            let mut stored = cohort.clone();
            stored.home = Entity::PLACEHOLDER;
            stored.current_tile = Entity::PLACEHOLDER;
            let expedition = entity.get::<Expedition>().map(|expedition| {
                let mut stored = expedition.clone();
                let home_band = band_ids_by_entity
                    .get(&expedition.home_band)
                    .copied()
                    .unwrap_or(BandId(0));
                stored.home_band = Entity::PLACEHOLDER;
                ExpeditionRecord {
                    home_band,
                    expedition: stored,
                }
            });
            Some(BandRecord {
                id,
                cohort: stored,
                home,
                current,
                labor: entity.get::<LaborAllocation>().cloned().unwrap_or_default(),
                resident: entity.contains::<ResidentBand>(),
                starting_unit: entity.get::<StartingUnit>().cloned(),
                travel: entity.get::<BandTravel>().copied(),
                expedition,
            })
        })
        .collect();
    bands.sort_by_key(|record| record.id);

    let mut settlements: Vec<SettlementRecord> = world
        .iter_entities()
        .filter_map(|entity| {
            let settlement = entity.get::<Settlement>()?;
            Some(SettlementRecord {
                settlement: settlement.clone(),
                town_center: entity.get::<TownCenter>().cloned(),
            })
        })
        .collect();
    settlements.sort_by_key(|record| {
        (
            record.settlement.position.y,
            record.settlement.position.x,
            record.settlement.faction.0,
        )
    });

    let sweep = world.resource::<VisibilitySweepTracker>();
    let mut sweep_positions: Vec<(BandId, UVec2)> = band_ids_by_entity
        .iter()
        .filter_map(|(entity, id)| sweep.previous(*entity).map(|position| (*id, position)))
        .collect();
    sweep_positions.sort_by_key(|(id, _)| *id);

    SimState {
        tick: *world.resource::<SimulationTick>(),
        tiles,
        links,
        bands,
        settlements,
        band_ids: *world.resource::<BandIdAllocator>(),
        active_crises: world.resource::<ActiveCrisisLedger>().clone(),
        beat_ledger: world.resource::<BeatLedger>().clone(),
        capability_flags: *world.resource::<CapabilityFlags>(),
        command_events: world.resource::<CommandEventLog>().clone(),
        corruption: world.resource::<CorruptionLedgers>().clone(),
        corruption_telemetry: world.resource::<CorruptionTelemetry>().clone(),
        counter_intel: world.resource::<CounterIntelBudgets>().clone(),
        crisis_telemetry: world.resource::<CrisisTelemetry>().clone(),
        discovered_sites: world.resource::<DiscoveredSites>().clone(),
        discovery_progress: world.resource::<DiscoveryProgressLedger>().clone(),
        espionage_missions: world.resource::<EspionageMissionState>().clone(),
        espionage_roster: world.resource::<EspionageRoster>().clone(),
        faction_inventory: world.resource::<FactionInventory>().clone(),
        security_policies: world.resource::<FactionSecurityPolicies>().clone(),
        forage: world.resource::<ForageRegistry>().clone(),
        graze: world.resource::<GrazeRegistry>().clone(),
        great_discoveries: world.resource::<GreatDiscoveryLedger>().clone(),
        great_discovery_readiness: world.resource::<GreatDiscoveryReadiness>().clone(),
        great_discovery_telemetry: world.resource::<GreatDiscoveryTelemetry>().clone(),
        herds: world.resource::<HerdRegistry>().clone(),
        influencer_impacts: world.resource::<InfluencerImpacts>().clone(),
        pending_crisis_seeds: world.resource::<PendingCrisisSeeds>().clone(),
        pending_crisis_spawns: world.resource::<PendingCrisisSpawns>().clone(),
        sedentarization: world.resource::<SedentarizationScore>().clone(),
        sentiment_bias: world.resource::<SentimentAxisBias>().clone(),
        trade_telemetry: world.resource::<TradeTelemetry>().clone(),
        victory: world.resource::<VictoryState>().clone(),
        visibility: world.resource::<VisibilityLedger>().clone(),
        culture: world.resource::<CultureManager>().checkpoint(),
        influencers: world.resource::<InfluentialRoster>().checkpoint(),
        knowledge: world.resource::<KnowledgeLedger>().checkpoint(),
        sweep_positions,
    }
}
