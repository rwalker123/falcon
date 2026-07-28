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
    /// `None` when the link carries no [`TradeLink`].
    ///
    /// **Presence is state.** Worldgen spawns links bare, and `capture_snapshot`'s query asks for
    /// `(&LogisticsLink, &TradeLink)` — so a link without one is invisible to the published
    /// `logistics` section entirely. A restore that helpfully inserted a default `TradeLink` would
    /// make 728 links appear on the wire that the original world never published.
    pub trade: Option<TradeLink>,
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
    /// `None` when the band carries no [`LaborAllocation`]. Presence is state here too, for the
    /// same reason as [`LinkRecord::trade`].
    pub labor: Option<LaborAllocation>,
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
    /// Three resources the classification tables call *derived*, carried anyway.
    ///
    /// "Derived" is only safe if nothing **publishes** the value before the system that rebuilds it
    /// next runs. These three fail that test: `capture_snapshot` reads `SimulationMetrics.crisis`
    /// for the published crisis telemetry, `PowerGridState` for `power_metrics`, and
    /// `HerdTelemetry` for the display herd list — all in the same turn, and all written by systems
    /// that will not have run again by the time a restored world is next captured. `HerdTelemetry`
    /// is the sharpest case: it is a mid-system snapshot of herd biomass, not a pure function of
    /// `HerdRegistry`, so rebuilding it from the registry produces a *different* number rather than
    /// a stale one.
    pub metrics: crate::metrics::SimulationMetrics,
    pub power_grid: crate::power::PowerGridState,
    pub herd_telemetry: crate::fauna::HerdTelemetry,

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
                trade: entity.get::<TradeLink>().cloned(),
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
            // A band's `home` is always a tile in production — worldgen sets it, and an expedition
            // clones it from the band it left. If it ever is not, the checkpoint would record
            // `(0, 0)` and a rollback would put the band on a corner of the map with nothing said,
            // which is the quietly-plausible-and-wrong failure this whole format exists to remove.
            // Loud instead: a debug build stops, a release build says so and carries on.
            let home = tile_positions.get(&cohort.home).copied().unwrap_or_else(|| {
                log::error!(
                    "checkpoint.capture.band_home_is_not_a_tile band={} — recording (0, 0)",
                    id.0
                );
                debug_assert!(
                    false,
                    "band {} has a `home` that is not a tile; a checkpoint cannot key it by position",
                    id.0
                );
                UVec2::ZERO
            });
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
                labor: entity.get::<LaborAllocation>().cloned(),
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
        metrics: world
            .resource::<crate::metrics::SimulationMetrics>()
            .clone(),
        power_grid: world.resource::<crate::power::PowerGridState>().clone(),
        herd_telemetry: world.resource::<crate::fauna::HerdTelemetry>().clone(),
        culture: world.resource::<CultureManager>().checkpoint(),
        influencers: world.resource::<InfluentialRoster>().checkpoint(),
        knowledge: world.resource::<KnowledgeLedger>().checkpoint(),
        sweep_positions,
    }
}

/// Rebuild the world from a checkpoint.
///
/// ## Ordering
///
/// The passes below are ordered by *reference*, not by convenience, and the order is forced:
///
/// 1. tiles — everything else names a tile by position,
/// 2. links and bands — both resolve tile positions to the entities pass 1 created,
/// 3. expeditions and the visibility sweep — both name a **band** by [`BandId`], so they need the
///    map pass 2 built,
/// 4. resources, then the derived structures a system would otherwise rebuild a turn late.
///
/// There is no cycle: every reference points from a later pass to an earlier one. If a future
/// record needs something from a pass after it, that is a design problem to solve at the record,
/// not with a second resolution pass.
pub fn restore_sim_state(world: &mut World, state: &SimState) {
    // --- clear what the checkpoint owns -------------------------------------------------------
    for entity in owned_entities(world) {
        world.despawn(entity);
    }

    // --- pass 1: tiles ------------------------------------------------------------------------
    let mut tile_entities: HashMap<UVec2, Entity> = HashMap::with_capacity(state.tiles.len());
    for record in &state.tiles {
        let mut entity = world.spawn(record.tile.clone());
        if let Some(power) = &record.power {
            entity.insert(power.clone());
        }
        if let Some(tag) = &record.food_module {
            entity.insert(tag.clone());
        }
        if let Some(tag) = &record.site {
            entity.insert(tag.clone());
        }
        tile_entities.insert(record.tile.position, entity.id());
    }

    // --- pass 2a: logistics links -------------------------------------------------------------
    for record in &state.links {
        let (Some(&from), Some(&to)) = (
            tile_entities.get(&record.from),
            tile_entities.get(&record.to),
        ) else {
            warn!(
                target: "shadow_scale::sim_state",
                from = ?record.from,
                to = ?record.to,
                "checkpoint.restore.link_endpoint_missing"
            );
            continue;
        };
        let mut link = record.link.clone();
        link.from = from;
        link.to = to;
        let mut entity = world.spawn(link);
        if let Some(trade) = &record.trade {
            entity.insert(trade.clone());
        }
    }

    // --- pass 2b: bands -----------------------------------------------------------------------
    let mut band_entities: HashMap<BandId, Entity> = HashMap::with_capacity(state.bands.len());
    for record in &state.bands {
        let Some(&home) = tile_entities.get(&record.home) else {
            warn!(
                target: "shadow_scale::sim_state",
                band = record.id.0,
                "checkpoint.restore.band_home_missing"
            );
            continue;
        };
        let current = tile_entities.get(&record.current).copied().unwrap_or(home);
        let mut cohort = record.cohort.clone();
        cohort.home = home;
        cohort.current_tile = current;

        let mut entity = world.spawn((cohort, record.id));
        if let Some(labor) = &record.labor {
            entity.insert(labor.clone());
        }
        if record.resident {
            entity.insert(ResidentBand);
        }
        if let Some(marker) = &record.starting_unit {
            entity.insert(marker.clone());
        }
        if let Some(travel) = record.travel {
            entity.insert(travel);
        }
        band_entities.insert(record.id, entity.id());
    }

    // --- pass 3a: expeditions -----------------------------------------------------------------
    for record in &state.bands {
        let (Some(expedition), Some(&entity)) = (&record.expedition, band_entities.get(&record.id))
        else {
            continue;
        };
        let Some(&home_band) = band_entities.get(&expedition.home_band) else {
            warn!(
                target: "shadow_scale::sim_state",
                band = record.id.0,
                home_band = expedition.home_band.0,
                "checkpoint.restore.expedition_home_missing"
            );
            continue;
        };
        let mut restored = expedition.expedition.clone();
        restored.home_band = home_band;
        world.entity_mut(entity).insert(restored);
    }

    // --- pass 3b: settlements -----------------------------------------------------------------
    for record in &state.settlements {
        let mut entity = world.spawn(record.settlement.clone());
        if let Some(town_center) = &record.town_center {
            entity.insert(town_center.clone());
        }
    }

    // --- pass 3c: the visibility corridor sweep -----------------------------------------------
    let mut sweep = VisibilitySweepTracker::default();
    for (band, position) in &state.sweep_positions {
        if let Some(&entity) = band_entities.get(band) {
            sweep.record(entity, *position);
        }
    }
    world.insert_resource(sweep);

    // --- pass 4a: the tile registry -----------------------------------------------------------
    let grid_size = world
        .get_resource::<crate::resources::SimulationConfig>()
        .map(|config| config.grid_size)
        .unwrap_or_default();
    let registry_tiles: Vec<Entity> = state
        .tiles
        .iter()
        .filter_map(|record| tile_entities.get(&record.tile.position).copied())
        .collect();
    world.insert_resource(crate::resources::TileRegistry {
        tiles: registry_tiles,
        width: grid_size.x,
        height: grid_size.y,
    });

    // --- pass 4b: resources -------------------------------------------------------------------
    world.insert_resource(state.tick);
    world.insert_resource(state.band_ids);
    world.insert_resource(state.active_crises.clone());
    world.insert_resource(state.beat_ledger.clone());
    world.insert_resource(state.capability_flags);
    // Installing the checkpoint's copy IS the truncation: the log is append-only, so the captured
    // copy is a prefix of the live one and everything appended after the checkpoint goes away.
    world.insert_resource(state.command_events.clone());
    world.insert_resource(state.corruption.clone());
    world.insert_resource(state.corruption_telemetry.clone());
    world.insert_resource(state.counter_intel.clone());
    world.insert_resource(state.crisis_telemetry.clone());
    world.insert_resource(state.discovered_sites.clone());
    world.insert_resource(state.discovery_progress.clone());
    world.insert_resource(state.espionage_missions.clone());
    world.insert_resource(state.espionage_roster.clone());
    world.insert_resource(state.faction_inventory.clone());
    world.insert_resource(state.security_policies.clone());
    world.insert_resource(state.forage.clone());
    world.insert_resource(state.graze.clone());
    world.insert_resource(state.great_discoveries.clone());
    world.insert_resource(state.great_discovery_readiness.clone());
    world.insert_resource(state.great_discovery_telemetry.clone());
    world.insert_resource(state.herds.clone());
    world.insert_resource(state.influencer_impacts.clone());
    world.insert_resource(state.pending_crisis_seeds.clone());
    world.insert_resource(state.pending_crisis_spawns.clone());
    world.insert_resource(state.sedentarization.clone());
    world.insert_resource(state.sentiment_bias.clone());
    world.insert_resource(state.trade_telemetry.clone());
    world.insert_resource(state.victory.clone());
    // The fog ledger is restored, not wiped. Wiping it used to be the only way to stop a rollback
    // leaking tiles discovered *after* the restore point, because `WorldSnapshot` did not carry the
    // ledger — but it also destroyed everything discovered *before* the checkpoint, permanently,
    // since fog memory is never re-derived. A checkpoint's ledger contains exactly what was known
    // then, which prevents the leak properly.
    world.insert_resource(state.visibility.clone());
    world.insert_resource(state.metrics.clone());
    world.insert_resource(state.power_grid.clone());
    world.insert_resource(state.herd_telemetry.clone());

    // --- pass 4c: resources whose config must NOT come from the checkpoint --------------------
    // `restore_checkpoint` writes state and leaves the config field alone, so each of these keeps
    // whatever config is live now. That is the whole reason these three are not plain clones.
    world
        .resource_mut::<CultureManager>()
        .restore_checkpoint(&state.culture);
    world
        .resource_mut::<InfluentialRoster>()
        .restore_checkpoint(&state.influencers);
    world
        .resource_mut::<KnowledgeLedger>()
        .restore_checkpoint(&state.knowledge);

    // --- pass 4d: derived structures a system would otherwise rebuild a turn late -------------
    let herds = world.resource::<HerdRegistry>().clone();
    if let Some(mut density) = world.get_resource_mut::<crate::fauna::HerdDensityMap>() {
        density.rebuild(grid_size, &herds);
    }
    let effects = world.resource::<CultureManager>().compute_effects();
    world.insert_resource(effects);
}

/// The entities a checkpoint owns, and therefore the ones a restore replaces.
fn owned_entities(world: &mut World) -> Vec<Entity> {
    let mut owned: Vec<Entity> = Vec::new();
    let mut tiles = world.query_filtered::<Entity, With<Tile>>();
    owned.extend(tiles.iter(world));
    let mut links = world.query_filtered::<Entity, With<LogisticsLink>>();
    owned.extend(links.iter(world));
    let mut cohorts = world.query_filtered::<Entity, With<PopulationCohort>>();
    owned.extend(cohorts.iter(world));
    let mut settlements = world.query_filtered::<Entity, With<Settlement>>();
    owned.extend(settlements.iter(world));
    owned
}

/// Set while a rollback is replaying turns forward from a checkpoint.
///
/// **A rollback is one publication and one history.** Replayed turns re-run the simulation, so they
/// must not also re-publish frames the client already applied, and must not push new entries into
/// the very rings the rollback is rewinding — a rollback that grew its own history could not
/// terminate. `capture_snapshot` and `record_checkpoint` are gated off this; `collect_metrics` and
/// `advance_tick`, which share their stage, are **not**, because the tick has to advance and
/// `SimulationMetrics` is checkpoint state the next turn reads.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Replaying(pub bool);

/// Run condition: true on a normal turn, false while replaying forward.
pub fn not_replaying(replaying: Option<Res<Replaying>>) -> bool {
    !replaying.is_some_and(|replaying| replaying.0)
}
