//! Every piece of simulation state is **classified**, and adding a new one fails this test.
//!
//! The defect this exists to prevent is not a wrong classification — it is a *missing* one. A
//! rollback that silently drops a resource produces a world that looks fine and diverges later,
//! and that is precisely how `core_sim` arrived at thirteen resources with no representation in
//! any checkpoint: nothing failed when they were left out, because nothing was looking.
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
//! Being in a table is not a claim that the checkpoint *currently* handles it — the behavioural
//! oracles in `integration_tests/tests/replay_determinism.rs` are what prove that. This test proves
//! only that nothing is unaccounted for, which is the half that was missing.

use bevy::prelude::*;
use core_sim::{build_headless_app, run_turn};
use std::collections::BTreeSet;

/// Mutated across turns, and a later turn reads it. A checkpoint that omits any of these produces
/// a world that diverges from the one it claims to restore.
const SIM_STATE_RESOURCES: [&str; 33] = [
    "ActiveCrisisLedger",
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
    // Previous-turn positions, so `calculate_visibility` can sweep the corridor a band crossed.
    "VisibilitySweepTracker",
];

/// Rebuilt from scratch every turn by the named system, before anything reads it.
const DERIVED_RESOURCES: [(&str, &str); 7] = [
    (
        "CrisisOverlayCache",
        "advance_crisis_system -> rebuild_overlay",
    ),
    ("CultureEffectsCache", "reconcile_culture_layers"),
    ("HerdDensityMap", "advance_herds"),
    ("HerdTelemetry", "advance_herds"),
    ("PowerGridState", "simulate_power"),
    ("SimulationMetrics", "collect_metrics"),
    ("SupplyNetworkMembership", "balance_supply_networks"),
];

/// Written once at worldgen and never again.
///
/// **This reason carries an expiry.** These survive a rollback only because a rollback restores
/// into the same live `World`, which still holds the map worldgen built. That stops being true the
/// day a checkpoint becomes a **save file** loaded into a fresh process — at which point every
/// entry here has to be either serialized or regenerated from `WorldGenSeed`. Do not read this
/// table as "these can never matter".
const WORLD_STATIC_RESOURCES: [&str; 16] = [
    "ActiveStartProfile",
    "BiomePalette",
    "CampaignLabel",
    "ElevationField",
    "FactionRegistry",
    "FoodSiteRegistry",
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
const NOT_SIM_STATE_RESOURCES: [(&str, &str); 8] = [
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
const CONFIG_RESOURCES_CONT: [&str; 13] = [
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
const SIM_STATE_COMPONENTS: [&str; 13] = [
    "Tile",
    "PopulationCohort",
    "LaborAllocation",
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

const NOT_SIM_STATE_COMPONENTS: [(&str, &str); 1] = [(
    // Documented at its definition as deliberately non-persistent: "a rollback mid-move cancels
    // the travel". That is a real decision, and it is also a divergence a bit-exact replay oracle
    // will report — the two cannot both hold. Revisit when the oracles are switched on.
    "BandTravel",
    "documented non-persistent: a rollback mid-move cancels the travel",
)];

fn leaf(type_path: &str) -> &str {
    type_path.rsplit("::").next().unwrap_or(type_path)
}

fn classified_resources() -> BTreeSet<&'static str> {
    SIM_STATE_RESOURCES
        .iter()
        .copied()
        .chain(DERIVED_RESOURCES.iter().map(|(name, _)| *name))
        .chain(WORLD_STATIC_RESOURCES.iter().copied())
        .chain(NOT_SIM_STATE_RESOURCES.iter().map(|(name, _)| *name))
        .chain(CONFIG_RESOURCES.iter().copied())
        .chain(CONFIG_RESOURCES_CONT.iter().copied())
        .collect()
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

    let classified: BTreeSet<&str> = SIM_STATE_COMPONENTS
        .iter()
        .copied()
        .chain(NOT_SIM_STATE_COMPONENTS.iter().map(|(name, _)| *name))
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
