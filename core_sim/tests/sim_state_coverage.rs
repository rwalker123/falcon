//! Every piece of simulation state is **classified**, and adding a new one fails this test.
//!
//! The defect this exists to prevent is not a wrong classification — it is a *missing* one. A
//! rollback that silently drops a resource produces a world that looks fine and diverges later,
//! and that is precisely how `core_sim` arrived at thirteen resources with no representation in
//! any checkpoint: nothing failed when they were left out, because nothing was looking.
//!
//! **Scope: the library's resources and components.** The app walked is `build_headless_app`, so
//! anything the `server` binary inserts — `ResolvedPortBase`, `ConfigWatcherRegistry`,
//! `CommandSenderResource`, `CommandLog` — is invisible here, and naming one below fails the stale
//! check rather than classifying it. Those are covered by the seams that produce them, not by a
//! table; see `.claude/rules/core_sim/checkpoints.md`.
//!
//! So the enumeration side is taken from a **built app at runtime** — `world.storages().resources`
//! and `world.components()` — never from a hand-maintained list or a grep. Adding a `Resource` or a
//! `Component` puts a name in front of this test that is in none of the tables below, and the test
//! fails until somebody decides which bucket it belongs in. The tables carry the *decision*; the
//! runtime carries the *inventory*. A hand-written inventory would reproduce the original bug.
//!
//! The buckets:
//!
//! | Table | Meaning |
//! |---|---|
//! | `SIM_STATE_*` | Mutated across turns and steers later ones. Must be in the checkpoint. |
//! | `DERIVED_*` | Fully rebuilt every turn by a named system, so a checkpoint can omit it. |
//! | `WORLD_STATIC_*` | Fixed at worldgen and never written again. |
//! | `NOT_SIM_STATE_*` | Infrastructure, session-scoped, or honestly not understood. |
//! | `CONFIG_RESOURCES` | Immutable balance data loaded from `src/data/*.json`. |
//!
//! **A name belongs to exactly one of them**, enforced by `classification_tables_are_disjoint`.
//! The union above is a `BTreeSet`, and a set cannot report that it absorbed the same name twice —
//! so a name in two buckets is "classified" by whichever a reader opens first, and deleting it from
//! the other leaves every assertion in this file still passing.
//!
//! Being in a table is not a claim that the checkpoint *currently* handles it — the behavioural
//! oracles in `integration_tests/tests/replay_determinism.rs` are what prove that. This test proves
//! only that nothing is unaccounted for, which is the half that was missing.

use bevy::prelude::*;
use core_sim::{build_headless_app, run_turn};
use std::collections::{BTreeMap, BTreeSet};

/// Mutated across turns, and a later turn reads it. A checkpoint that omits any of these produces
/// a world that diverges from the one it claims to restore.
const SIM_STATE_RESOURCES: [&str; 37] = [
    "ActiveCrisisLedger",
    // The band-id counter. Restoring the bands without it re-issues a live id after a rollback.
    "BandIdAllocator",
    "BeatLedger",
    "CapabilityFlags",
    // Append-only. A restore must TRUNCATE this to the checkpoint's length, not replace it —
    // replacing leaves post-checkpoint events in a rolled-back world.
    "CommandEventLog",
    "CorruptionLedgers",
    // Splits: `exposures_this_turn` is cleared per turn (derived), `exposures_total` is a
    // cumulative counter (state). Carried whole because the cheap half is small.
    "CorruptionTelemetry",
    "CounterIntelBudgets",
    // Gauge history / EMA / trend accumulate across turns, so this is not derived despite its name.
    "CrisisTelemetry",
    "CultureManager",
    "DiscoveredSites",
    "DiscoveryProgressLedger",
    "EspionageMissionState",
    "EspionageRoster",
    "FactionInventory",
    // Mutated only by command handlers, which is still state a rollback has to put back.
    "FactionSecurityPolicies",
    "ForageRegistry",
    "GrazeRegistry",
    "GreatDiscoveryLedger",
    "GreatDiscoveryReadiness",
    "GreatDiscoveryTelemetry",
    "HerdRegistry",
    "InfluencerImpacts",
    "InfluentialRoster",
    "KnowledgeLedger",
    // Enqueued in the `GreatDiscovery` stage and drained in `Crisis`, so these are empty at the
    // end of a turn — EXCEPT on the mid-turn `recapture_and_broadcast` path, where a
    // command-queued spawn can still be pending. "Empty in practice" is not a property a test can
    // check, so they are carried.
    "PendingCrisisSeeds",
    "PendingCrisisSpawns",
    "SedentarizationScore",
    "SentimentAxisBias",
    "SimulationTick",
    "TradeTelemetry",
    "VictoryState",
    // Permanent-memory fog: the ledger IS the record of what has ever been seen.
    "VisibilityLedger",
    // The three below look derived and are not, because **"derived" is only safe if nothing
    // publishes the value before the system that rebuilds it next runs.** `capture_snapshot` reads
    // `SimulationMetrics.crisis` for the published crisis telemetry, `PowerGridState` for
    // `power_metrics`, and `HerdTelemetry` for the display herd list — all in the same turn, all
    // written by systems that will not have run again when a restored world is next captured.
    // `HerdTelemetry` is the sharpest: it is a mid-system snapshot of herd biomass rather than a
    // pure function of `HerdRegistry`, so rebuilding it from the registry yields a *different*
    // number, not a stale one. The restore-fidelity oracle is what distinguished these from the
    // genuinely derived four.
    "SimulationMetrics",
    "PowerGridState",
    "HerdTelemetry",
    // Previous-turn positions, so `calculate_visibility` can sweep the corridor a band crossed.
    "VisibilitySweepTracker",
];

/// Rebuilt from scratch every turn by the named system, **and read by nothing in between**.
///
/// The second half is the load-bearing one, and it is why `HerdTelemetry`, `PowerGridState` and
/// `SimulationMetrics` are not here despite each having a system that rebuilds it: `capture_snapshot`
/// publishes all three within the same turn. See the comment on them in `SIM_STATE_RESOURCES`.
const DERIVED_RESOURCES: [(&str, &str); 4] = [
    (
        "CrisisOverlayCache",
        "advance_crisis_system -> rebuild_overlay",
    ),
    ("CultureEffectsCache", "reconcile_culture_layers"),
    ("HerdDensityMap", "advance_herds"),
    ("SupplyNetworkMembership", "balance_supply_networks"),
];

/// Written once at worldgen and never again.
///
/// **This reason carries an expiry.** These survive a rollback only because a rollback restores
/// into the same live `World`, which still holds the map worldgen built. That stops being true the
/// day a checkpoint becomes a **save file** loaded into a fresh process — at which point every
/// entry here has to be either serialized or regenerated from `WorldGenSeed`. Do not read this
/// table as "these can never matter".
const WORLD_STATIC_RESOURCES: [&str; 17] = [
    "ActiveStartProfile",
    "BiomePalette",
    "CampaignLabel",
    "ElevationField",
    "FactionRegistry",
    "FoodSiteRegistry",
    "FoodSiteWaterBiasReport",
    "GenerationRegistry",
    "GreatDiscoveryRegistry",
    "HydrologyState",
    "MoistureRaster",
    "PowerTopology",
    "ProvinceMap",
    "StartLocation",
    "StartProfileLookup",
    "TileRegistry",
    "WorldGenSeed",
];

/// Infrastructure, session-scoped, or not understood. The last three are the honest ones.
const NOT_SIM_STATE_RESOURCES: [(&str, &str); 10] = [
    (
        "FloraQuoteCache",
        "a memo of a pure function of ground + config; it re-derives on demand and its own \
         per-tile / per-config identity check catches anything that moved (#410)",
    ),
    (
        "Replaying",
        "a flag held only for the duration of one rollback; a checkpoint is never taken while set",
    ),
    (
        "SnapshotHistory",
        "the publication ring itself; restoring it into a restore would be circular",
    ),
    (
        "SnapshotCaptureMode",
        "a one-frame flag set around `recapture_snapshot_in_place`",
    ),
    (
        "WorldEpoch",
        "counts world rebuilds for the client's stale-world check; not simulated",
    ),
    (
        "ViewerFaction",
        "which faction the client renders — a session concern, not sim state",
    ),
    (
        "TurnQueue",
        "server-side order intake, owned by `bin/server.rs` outside the turn",
    ),
    // The three below are classified honestly rather than plausibly. Each is a candidate for
    // deletion, and saying so here is more useful than filing it somewhere it looks understood.
    (
        "DiplomacyLeverage",
        "written by 3 systems, read by none: candidate dead code",
    ),
    (
        "PowerDiscoveryEffects",
        "written by `resolve_great_discovery`, read by none: candidate dead code",
    ),
    (
        "ObservationLedger",
        "never mutated outside tests; inert in a real run",
    ),
];

/// Immutable balance data parsed from `src/data/*.json` at boot. `*Metadata` is the path/provenance
/// record beside each handle. `SimulationConfig` sits here because it is config the operator edits,
/// not state the turn evolves — note the hot-reload path in `bin/server.rs` means a replay is only
/// reproducible against the config it originally ran with.
const CONFIG_RESOURCES: [&str; 42] = [
    "BeatCatalogHandle",
    "BeatCatalogMetadata",
    "BeatConfigHandle",
    "BeatConfigMetadata",
    "CombatConfigHandle",
    "CombatConfigMetadata",
    "CreaturesConfigHandle",
    "CreaturesConfigMetadata",
    "CrisisArchetypeCatalogHandle",
    "CrisisArchetypeCatalogMetadata",
    "CrisisModifierCatalogHandle",
    "CrisisModifierCatalogMetadata",
    "CrisisTelemetryConfigHandle",
    "CrisisTelemetryConfigMetadata",
    "CultureCorruptionConfigHandle",
    "DemographicsConfigHandle",
    "DemographicsConfigMetadata",
    "EspionageCatalog",
    "ExpeditionConfigHandle",
    "ExpeditionConfigMetadata",
    "FaunaConfigHandle",
    "FaunaConfigMetadata",
    "FloraConfigHandle",
    "FloraConfigMetadata",
    "InfluencerConfigHandle",
    "KnowledgeLedgerConfigHandle",
    "LaborConfigHandle",
    "LaborConfigMetadata",
    "LadderConfigHandle",
    "LadderConfigMetadata",
    "MapPresetsHandle",
    "MapPresetsMetadata",
    "SedentarizationConfigHandle",
    "SedentarizationConfigMetadata",
    "SettlementStageConfigHandle",
    "SettlementStageConfigMetadata",
    "SimulationConfig",
    "SimulationConfigMetadata",
    "SitesConfigHandle",
    "SitesConfigMetadata",
    "SnapshotOverlaysConfigHandle",
    "SnapshotOverlaysConfigMetadata",
];

/// The remaining config handles, split out only because Rust array consts need a fixed length and
/// one 50-entry literal reads worse than two.
const CONFIG_RESOURCES_CONT: [&str; 17] = [
    "EquipmentConfigHandle",
    "EquipmentConfigMetadata",
    "MaterialsConfigHandle",
    "MaterialsConfigMetadata",
    "StartProfileKnowledgeTagsHandle",
    "StartProfileKnowledgeTagsMetadata",
    "StartProfilesHandle",
    "StartProfilesMetadata",
    "SupplyNetworkConfigHandle",
    "SupplyNetworkConfigMetadata",
    "TurnPipelineConfigHandle",
    "TurnPipelineConfigMetadata",
    "VictoryConfigHandle",
    "VisibilityConfigHandle",
    "VisibilityConfigMetadata",
    "WellbeingConfigHandle",
    "WellbeingConfigMetadata",
];

/// Component state on entities. Omitting one of these is exactly the failure `PowerNode`'s missing
/// `base_generation` / `base_demand` already is, which is why this table exists alongside the
/// resource one: a resource-only guard would have missed the bug that motivated the guard.
const SIM_STATE_COMPONENTS: [&str; 17] = [
    "Tile",
    // A band's durable identity — the thing `Entity` could not be across a restore.
    "BandId",
    // Carried by the checkpoint. A checkpoint is lossless; "rollback cancels an in-flight move" is
    // a gameplay rule, and it is applied as an explicit step in the rollback command path rather
    // than by leaving the state out. Implemented as an omission it would not be a rule, it would be
    // a hole with a comment on it.
    "BandTravel",
    "PopulationCohort",
    // The fractional carry behind a band's birth, death and age-transition events. Carried for the same
    // reason as `BandTravel`: a band two-thirds of the way to a birth was two-thirds of the way
    // there, and dropping the remainder re-times every demographic event after a restore.
    "DemographicFlowAccumulator",
    "LaborAllocation",
    // How worn each of a band's two consumable kits is (the minimal TOE). Carried for the same
    // reason as `DemographicFlowAccumulator`: a checkpoint that forgot how worn your spears were
    // would silently re-stock them on rollback, and there is no replenishment path that could
    // legitimately do that.
    "BandEquipment",
    "Expedition",
    "LogisticsLink",
    "TradeLink",
    // Carries `base_generation` / `base_demand`, which no checkpoint has ever recorded.
    "PowerNode",
    "ResidentBand",
    // How `resolve_starting_unit_entity` finds a band for a client command.
    "StartingUnit",
    "Settlement",
    "TownCenter",
    // Worldgen tile tags. Restore does not respawn them today, which is why a restored world
    // reports `forage_patches[].per_worker_yield == 0`.
    "FoodModuleTag",
    "SiteTag",
];

const NOT_SIM_STATE_COMPONENTS: [(&str, &str); 0] = [];

fn leaf(type_path: &str) -> &str {
    type_path.rsplit("::").next().unwrap_or(type_path)
}

/// Every resource table paired with its name, so a duplicate can report *which two* tables hold it.
/// One list feeds both the union below and the disjointness check, so neither can go out of date.
fn resource_tables() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("SIM_STATE_RESOURCES", SIM_STATE_RESOURCES.to_vec()),
        (
            "DERIVED_RESOURCES",
            DERIVED_RESOURCES.iter().map(|(name, _)| *name).collect(),
        ),
        ("WORLD_STATIC_RESOURCES", WORLD_STATIC_RESOURCES.to_vec()),
        (
            "NOT_SIM_STATE_RESOURCES",
            NOT_SIM_STATE_RESOURCES
                .iter()
                .map(|(name, _)| *name)
                .collect(),
        ),
        ("CONFIG_RESOURCES", CONFIG_RESOURCES.to_vec()),
        ("CONFIG_RESOURCES_CONT", CONFIG_RESOURCES_CONT.to_vec()),
    ]
}

fn component_tables() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("SIM_STATE_COMPONENTS", SIM_STATE_COMPONENTS.to_vec()),
        (
            "NOT_SIM_STATE_COMPONENTS",
            NOT_SIM_STATE_COMPONENTS
                .iter()
                .map(|(name, _)| *name)
                .collect(),
        ),
    ]
}

fn classified_resources() -> BTreeSet<&'static str> {
    resource_tables()
        .into_iter()
        .flat_map(|(_, names)| names)
        .collect()
}

/// Fails naming the symbol and both tables holding it.
fn assert_tables_disjoint(kind: &str, tables: &[(&'static str, Vec<&'static str>)]) {
    let mut home: BTreeMap<&'static str, &'static str> = BTreeMap::new();
    let mut clashes: Vec<String> = Vec::new();
    for (table, names) in tables {
        for name in names {
            if let Some(first) = home.insert(name, table) {
                clashes.push(format!(
                    "`{name}` is classified in BOTH `{first}` and `{table}`"
                ));
            }
        }
    }
    assert!(
        clashes.is_empty(),
        "a {kind} is classified twice:\n  {}\n\n\
         The buckets are mutually exclusive claims about one symbol, so a name in two of them is \
         two contradictory decisions — and because the tables are unioned into a set before they \
         are checked against the runtime, the duplicate ALSO makes the surviving copy optional: \
         delete it from either table and every other assertion in this file still passes. Pick the \
         bucket that is true and delete the other entry.",
        clashes.join("\n  ")
    );
}

/// Resource type paths a built app actually holds, restricted to this crate's own types.
fn runtime_resources(world: &World) -> Vec<String> {
    world
        .storages()
        .resources
        .iter()
        .filter(|(_, data)| data.is_present())
        .filter_map(|(id, _)| world.components().get_info(id))
        .map(|info| info.name().to_string())
        .collect()
}

/// No symbol is classified twice — the guard the other tests in this file cannot supply.
///
/// They compare a `BTreeSet` union of the tables against the runtime, and a set silently absorbs a
/// repeat. So a name listed in two buckets is covered by neither: remove it from one and the union
/// is unchanged, which is a coverage hole shaped exactly like the omission this file exists to
/// catch. `HerdTelemetry`, `PowerGridState` and `SimulationMetrics` were in both
/// `SIM_STATE_RESOURCES` and `DERIVED_RESOURCES` — the two halves of the one distinction
/// `.claude/rules/core_sim/checkpoints.md` names as the tempting shortcut, and the three it uses as
/// its worked examples of getting it wrong.
#[test]
fn classification_tables_are_disjoint() {
    assert_tables_disjoint("resource", &resource_tables());
    assert_tables_disjoint("component", &component_tables());
}

#[test]
fn every_runtime_resource_is_classified() {
    let mut app = build_headless_app();
    run_turn(&mut app);
    let world = &app.world;

    let classified = classified_resources();
    let mut unclassified: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();

    for path in runtime_resources(world) {
        if !path.starts_with("core_sim::") {
            // bevy's own resources are not ours to classify. `Events<core_sim::T>` is the one
            // wrapper that mentions our types; bevy double-buffers and clears events every frame,
            // so no event survives a turn boundary and none can be checkpoint state. Anything else
            // that wraps a `core_sim` type is a real hole and fails below.
            assert!(
                !path.contains("core_sim::") || path.starts_with("bevy_ecs::event::Events<"),
                "a non-bevy container is holding a `core_sim` type and is unclassified: {path}"
            );
            continue;
        }
        let name = leaf(&path);
        seen.insert(name.to_string());
        if !classified.contains(name) {
            unclassified.push(path);
        }
    }

    assert!(
        unclassified.is_empty(),
        "these resources exist in a built app but are in none of the tables in this file:\n  {}\n\n\
         Classify each one: is it checkpoint state (`SIM_STATE_RESOURCES`), rebuilt every turn \
         (`DERIVED_RESOURCES`, name the system), fixed at worldgen (`WORLD_STATIC_RESOURCES`), \
         infrastructure (`NOT_SIM_STATE_RESOURCES`, give a reason), or balance data \
         (`CONFIG_RESOURCES`)? An honest \"written but never read\" beats a plausible guess.",
        unclassified.join("\n  ")
    );

    // The tables rot in the other direction too: a resource that was deleted or renamed leaves a
    // stale entry that silently excuses nothing.
    let stale: Vec<&str> = classified
        .iter()
        .copied()
        .filter(|name| !seen.contains(*name))
        .collect();
    assert!(
        stale.is_empty(),
        "these names are classified in this file but no longer exist in a built app — delete \
         them, or the table is claiming to cover state that is gone:\n  {}",
        stale.join("\n  ")
    );
}

#[test]
fn every_registered_component_is_classified() {
    let mut app = build_headless_app();
    run_turn(&mut app);
    let world = &app.world;

    // `Components` holds resources too (bevy stores them in the same registry), so the resource
    // ids are subtracted. Registered rather than *live* components on purpose: `Expedition` and
    // `TradeLink` have no instances in a freshly-generated world, and a walk over archetypes would
    // quietly miss exactly the state a rollback is most likely to drop.
    let resource_ids: BTreeSet<_> = world
        .storages()
        .resources
        .iter()
        .map(|(id, _)| id)
        .collect();

    let classified: BTreeSet<&str> = component_tables()
        .into_iter()
        .flat_map(|(_, names)| names)
        .collect();

    let mut unclassified: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for info in world.components().iter() {
        if resource_ids.contains(&info.id()) {
            continue;
        }
        let path = info.name();
        if !path.starts_with("core_sim::") {
            continue;
        }
        let name = leaf(path);
        seen.insert(name.to_string());
        if !classified.contains(name) {
            unclassified.push(path.to_string());
        }
    }

    assert!(
        unclassified.is_empty(),
        "these components are registered but unclassified:\n  {}\n\n\
         A component on a live entity is simulation state exactly as much as a resource is. \
         Decide whether the checkpoint has to carry it (`SIM_STATE_COMPONENTS`) or why it does not \
         (`NOT_SIM_STATE_COMPONENTS`, with a reason).",
        unclassified.join("\n  ")
    );

    let stale: Vec<&str> = classified
        .iter()
        .copied()
        .filter(|name| !seen.contains(*name))
        .collect();
    assert!(
        stale.is_empty(),
        "these component names are classified here but no longer registered — delete them:\n  {}",
        stale.join("\n  ")
    );
}

/// Every band carries a unique [`BandId`], and the allocator agrees with what it handed out.
///
/// The uniqueness half is the one that matters: a duplicate id is worse than the entity churn
/// `BandId` replaces, because two bands that alias resolve to *each other* silently instead of
/// failing to resolve at all.
#[test]
fn every_band_has_a_unique_durable_id() {
    use core_sim::{BandId, BandIdAllocator, PopulationCohort};

    let mut app = build_headless_app();
    run_turn(&mut app);

    let mut query = app.world.query::<(&PopulationCohort, Option<&BandId>)>();
    let bands: Vec<(bool, Option<BandId>)> = query
        .iter(&app.world)
        .map(|(_, id)| (true, id.copied()))
        .collect();
    assert!(!bands.is_empty(), "worldgen produced no bands to check");

    let missing = bands.iter().filter(|(_, id)| id.is_none()).count();
    assert_eq!(
        missing,
        0,
        "{missing} of {} cohorts have no `BandId` — every spawn site must attach one, or a \
         checkpoint cannot name the band it is restoring",
        bands.len()
    );

    let ids: Vec<u64> = bands
        .iter()
        .filter_map(|(_, id)| id.map(|id| id.0))
        .collect();
    let unique: BTreeSet<u64> = ids.iter().copied().collect();
    assert_eq!(
        unique.len(),
        ids.len(),
        "duplicate `BandId` handed out: {ids:?}"
    );
    assert!(
        !unique.contains(&0),
        "`BandId(0)` is reserved for \"unset\""
    );

    // The allocator must be ahead of every id it issued, or the next band collides with a live one.
    let next = app.world.resource::<BandIdAllocator>().peek();
    let highest = unique.iter().copied().max().unwrap_or(0);
    assert!(
        next > highest,
        "allocator is at {next} but band {highest} already exists — the next spawn would alias it"
    );
}

/// `capture_sim_state` sees every entity the tables above call simulation state, and leaks no
/// `Entity` into the checkpoint.
///
/// The counts are the point. A checkpoint that quietly captured *some* of the tiles would restore a
/// world that looked plausible and diverged later, which is the failure this arc exists to remove;
/// comparing against a direct query is the cheapest oracle for "did the walk find everything".
#[test]
fn capture_sees_every_sim_state_entity() {
    use core_sim::sim_state::capture_sim_state;
    use core_sim::{BandId, LogisticsLink, PopulationCohort, PowerNode, Settlement, Tile};

    let mut app = build_headless_app();
    run_turn(&mut app);

    let tiles = app.world.query::<&Tile>().iter(&app.world).count();
    let links = app.world.query::<&LogisticsLink>().iter(&app.world).count();
    let powered = app.world.query::<&PowerNode>().iter(&app.world).count();
    let bands = app
        .world
        .query::<(&PopulationCohort, &BandId)>()
        .iter(&app.world)
        .count();
    let settlements = app.world.query::<&Settlement>().iter(&app.world).count();

    let state = capture_sim_state(&app.world);

    assert_eq!(state.tiles.len(), tiles, "captured tile count");
    assert_eq!(state.links.len(), links, "captured logistics link count");
    assert_eq!(state.bands.len(), bands, "captured band count");
    assert_eq!(
        state.settlements.len(),
        settlements,
        "captured settlement count"
    );
    assert_eq!(
        state.tiles.iter().filter(|t| t.power.is_some()).count(),
        powered,
        "captured power node count"
    );
    assert!(tiles > 0 && links > 0 && bands > 0, "world was not built");

    // Every reference must be a stable sim id. A live `Entity` in here would resolve to a different
    // thing after a restore, silently.
    for band in &state.bands {
        assert_eq!(
            band.cohort.home,
            Entity::PLACEHOLDER,
            "band {:?} leaked a live `home` entity into the checkpoint",
            band.id
        );
        assert_eq!(
            band.cohort.current_tile,
            Entity::PLACEHOLDER,
            "band {:?} leaked a live `current_tile` entity",
            band.id
        );
        if let Some(expedition) = &band.expedition {
            assert_eq!(
                expedition.expedition.home_band,
                Entity::PLACEHOLDER,
                "expedition {:?} leaked a live `home_band` entity",
                band.id
            );
        }
    }
    for link in &state.links {
        assert_eq!(link.link.from, Entity::PLACEHOLDER, "link leaked `from`");
        assert_eq!(link.link.to, Entity::PLACEHOLDER, "link leaked `to`");
    }

    // Bands are keyed by id, so the ids must be unique within a checkpoint.
    let unique: BTreeSet<u64> = state.bands.iter().map(|band| band.id.0).collect();
    assert_eq!(unique.len(), state.bands.len(), "duplicate BandId captured");
}
