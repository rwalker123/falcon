use std::borrow::Cow;
use std::io::{self, BufReader, Read};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

use bevy::{
    ecs::system::Resource,
    math::UVec2,
    prelude::{Entity, With},
};
use crossbeam_channel::{unbounded, Receiver, Sender};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tracing::{info, warn};
use tracing_subscriber::prelude::*;

use core_sim::log_stream::start_log_stream_server;
use core_sim::port_alloc;

use core_sim::grid_utils::hex_distance_wrapped;
use core_sim::metrics::SimulationMetrics;
use core_sim::network::{start_snapshot_server, SnapshotServer};
use core_sim::port_base_override;
use core_sim::sim_state::{restore_sim_state, Replaying};
use core_sim::turn_profile;
use core_sim::{
    apply_port_base, available_workers, floor_is_valid, forage_source_yield_preview,
    hunt_source_yield_preview, knows, load_simulation_config_for_new_world, output_multiplier,
    resolve_active_profile, resolve_committed_species, rung_site_refusal, tile_flora_composition,
    tile_is_fresh_watered, ActiveStartProfile, BandTravel, BeatCatalogHandle, BeatConfigHandle,
    BeatLedger, CampaignLabel, Expedition, ExpeditionConfigHandle, ExpeditionMission,
    ExpeditionPhase, FloraConfigHandle, FoodModuleTag, ForkAnswerError, LaborAllocation,
    LaborTarget, LadderConfigHandle, LocalStore, ResidentBand, RungKey, SiteRefusal,
    SpeciesRefusal, StartProfile, StartProfileOverrides, WellbeingConfigHandle,
    DEFAULT_ESCAPEMENT_FLOOR, NO_FORAGE_SEASON,
};
use core_sim::{
    build_headless_app, clear_config_overrides, hunt_trip_forecast, install_config_override,
    recapture_snapshot_in_place, run_turn, scalar_from_f32, AgentAssignment, BandId,
    BandIdAllocator, CommandEventEntry, CommandEventKind, CommandEventLog, CorruptionLedgers,
    CounterIntelBudgets, CrisisArchetypeCatalog, CrisisArchetypeCatalogHandle,
    CrisisArchetypeCatalogMetadata, CrisisModifierCatalog, CrisisModifierCatalogHandle,
    CrisisModifierCatalogMetadata, CrisisTelemetry, CrisisTelemetryConfig,
    CrisisTelemetryConfigHandle, CrisisTelemetryConfigMetadata, DiscoveryProgressLedger,
    EspionageAgentHandle, EspionageCatalog, EspionageMissionId, EspionageMissionKind,
    EspionageMissionState, EspionageMissionTemplate, EspionageRoster, FactionId, FactionOrders,
    FactionRegistry, FactionSecurityPolicies, FaunaConfigHandle, FoodSiteRegistry, ForageRegistry,
    FrameSink, GenerationId, GenerationRegistry, HerdRegistry, Improvement, InfluencerImpacts,
    InfluentialRoster, LaborConfigHandle, MapPresetsHandle, PendingCrisisSpawns, PopulationCohort,
    QueueMissionError, QueueMissionParams, Scalar, SecurityPolicy, SentimentAxisBias, Settlement,
    SimulationConfig, SimulationConfigMetadata, SimulationTick, SnapshotHistory,
    SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, SnapshotOverlaysConfigMetadata,
    StartLocation, StartProfileLookup, StartProfilesHandle, StartingUnit, StoredSnapshot,
    SubmitError, SubmitOutcome, SupportChannel, Tile, TileRegistry, TownCenter, TurnPipelineConfig,
    TurnPipelineConfigHandle, TurnPipelineConfigMetadata, TurnQueue, WorldEpoch, FOOD,
};
use sim_runtime::{
    commands::{
        ConfigOverrideKind, EspionageGeneratorUpdate as CommandGeneratorUpdate, ReloadConfigKind,
    },
    AxisBiasState, CancelScope, CommandEnvelope as ProtoCommandEnvelope,
    CommandPayload as ProtoCommandPayload, CorruptionEntry, CorruptionSubsystem,
    InfluenceScopeKind, OrdersDirective as ProtoOrdersDirective, SecurityPolicyKind,
    SupportChannel as ProtoSupportChannel, TerrainTags,
};
use sim_schema::{encode_map_export_json, MapExport};

/// Gitignored scratch directory that `export_map` writes into when the command
/// is invoked without an explicit path.
const DEFAULT_EXPORT_DIR: &str = "exports";

/// Gitignored scratch directory holding the merged configs that `set_config_override` stages,
/// one `<kind>.json` per config. Alongside `exports/` and for the same reason: it is a per-run
/// artifact of a dev tool, not a source file, but it is worth being able to open and read while
/// explaining a playtest.
const DEFAULT_CONFIG_OVERRIDE_DIR: &str = "config_overrides";

const SETTLEMENT_PROVISION_COST: i64 = 80;
const SETTLEMENT_CONSTRUCTION_RADIUS: u32 = 3;
const SETTLEMENT_LOGISTICS_RADIUS: u32 = 4;

/// Exit code when no port block could be bound. Distinct from a panic: this is
/// an operator-actionable configuration/environment problem, not a bug.
const PORT_ALLOC_EXIT_CODE: i32 = 2;

/// The port base the process actually bound (which may differ from the
/// configured one after an auto-bump). Config hot-reload re-applies *this*
/// rather than the configured base, so a reload can't leave the in-world config
/// claiming ports the server does not hold.
#[derive(Resource, Clone, Copy)]
struct ResolvedPortBase(u16);

fn main() {
    let mut app = build_headless_app();
    app.insert_resource(SimulationMetrics::default());

    let config = app.world.resource::<SimulationConfig>().clone();

    // Bind the whole port block up front, all-or-nothing, before any subsystem
    // starts. A busy port either bumps the block to the next free slot or aborts
    // startup outright; the server never runs with a socket silently disabled.
    // Slot 0 of the block is reserved and never bound (#388) — the base is where
    // the block starts, not a listener.
    let configured_base = config.port_base_bind.port();
    let base_is_explicit = port_base_override().is_some();
    let bound_ports = match port_alloc::allocate(
        config.port_base_bind.ip(),
        configured_base,
        base_is_explicit,
    ) {
        Ok(bound) => bound,
        Err(err) => {
            eprintln!("Shadow-Scale server cannot start: {err}");
            std::process::exit(PORT_ALLOC_EXIT_CODE);
        }
    };
    let port_base_bumped = bound_ports.base != configured_base;
    let resolved_base = bound_ports.base;
    let (command_port, snapshot_flat_port, log_port) = (
        bound_ports.command_port(),
        bound_ports.snapshot_flat_port(),
        bound_ports.log_port(),
    );

    // Publish the resolved ports for client auto-discovery. Failure is never
    // fatal: the server still runs, only discovery is lost. The guard removes
    // the file when `main` returns normally.
    let ports_file = port_alloc::write_ports_file(&bound_ports);

    // Keep the in-world config honest about the ports actually bound, so the
    // config-reload path and anything reading the binds report the truth.
    if port_base_bumped {
        let mut config_res = app.world.resource_mut::<SimulationConfig>();
        apply_port_base(&mut config_res, resolved_base);
    }
    app.world.insert_resource(ResolvedPortBase(resolved_base));

    let log_stream = start_log_stream_server(bound_ports.log);
    let log_stream_enabled = log_stream.is_some();
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    if let Some(handle) = &log_stream {
        tracing_subscriber::registry()
            .with(env_filter.clone())
            .with(tracing_subscriber::fmt::layer())
            .with(handle.layer())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    if !log_stream_enabled {
        warn!(target: "shadow_scale::server", "log_stream.start_failed");
    }

    // Shared, because the publisher thread of every world holds a handle to it as its
    // `FrameSink` while the command loop keeps writing rollback / resync / feed frames to it.
    let snapshot_flat_server = Arc::new(start_snapshot_server(bound_ports.snapshot_flat));

    let config_watch_path = app
        .world
        .resource::<SimulationConfigMetadata>()
        .path()
        .cloned();
    let turn_pipeline_watch_path = app
        .world
        .resource::<TurnPipelineConfigMetadata>()
        .path()
        .cloned();
    let snapshot_overlays_watch_path = app
        .world
        .resource::<SnapshotOverlaysConfigMetadata>()
        .path()
        .cloned();
    let crisis_archetypes_watch_path = app
        .world
        .resource::<CrisisArchetypeCatalogMetadata>()
        .path()
        .cloned();
    let crisis_modifiers_watch_path = app
        .world
        .resource::<CrisisModifierCatalogMetadata>()
        .path()
        .cloned();
    let crisis_telemetry_watch_path = app
        .world
        .resource::<CrisisTelemetryConfigMetadata>()
        .path()
        .cloned();

    let (command_rx, command_tx) = spawn_command_listener(bound_ports.command);
    app.world
        .insert_resource(CommandSenderResource(command_tx.clone()));
    app.world.insert_resource(ConfigWatcherRegistry::default());

    if let Some(path) = config_watch_path {
        app.world
            .resource_mut::<ConfigWatcherRegistry>()
            .restart_simulation(Some(path), command_tx.clone());
    }
    if let Some(path) = turn_pipeline_watch_path {
        app.world
            .resource_mut::<ConfigWatcherRegistry>()
            .restart_turn_pipeline(Some(path), command_tx.clone());
    }
    if let Some(path) = snapshot_overlays_watch_path {
        app.world
            .resource_mut::<ConfigWatcherRegistry>()
            .restart_snapshot_overlays(Some(path), command_tx.clone());
    }
    if let Some(path) = crisis_archetypes_watch_path {
        app.world
            .resource_mut::<ConfigWatcherRegistry>()
            .restart_crisis_archetypes(Some(path), command_tx.clone());
    }
    if let Some(path) = crisis_modifiers_watch_path {
        app.world
            .resource_mut::<ConfigWatcherRegistry>()
            .restart_crisis_modifiers(Some(path), command_tx.clone());
    }
    if let Some(path) = crisis_telemetry_watch_path {
        app.world
            .resource_mut::<ConfigWatcherRegistry>()
            .restart_crisis_telemetry(Some(path), command_tx.clone());
    }

    // Boot IDLE: do NOT run the Startup worldgen or broadcast a first snapshot. Bevy's `Startup`
    // schedule only runs on the first `app.update()`, so not calling `run_turn` here leaves the world
    // ungenerated (and `ElevationField` uninserted, so the Snapshot stage must never run — see the
    // `world_active` gate below). A world is generated on demand by `new_game` (or `map_size`/ResetMap).
    let mut world_active = false;
    // The rollback authority. Built on the first `new_game`; until then there is no world to log.
    let mut command_log: Option<CommandLog> = None;
    // The monotonic world-build counter (lives outside the app, like `world_active`, because every
    // rebuild constructs a brand-new `App`). `rebuild_world_from_config` increments it and inserts a
    // `WorldEpoch` into each fresh app before its first capture; the idle boot app carries 0.
    let mut world_epoch: u32 = 0;

    let bind_host = config.port_base_bind.ip();
    if port_base_bumped {
        warn!(
            target: "shadow_scale::server",
            configured_base,
            resolved_base,
            "port_block.bumped=configured base was in use"
        );
    }
    info!(
        host = %bind_host,
        port_base = resolved_base,
        command_port,
        snapshot_flat_port,
        log_port,
        port_base_bumped,
        ports_file = ports_file
            .as_ref()
            .map(|guard| guard.path().display().to_string())
            .unwrap_or_else(|| "unavailable".to_string()),
        log_stream_enabled,
        // WHICH BUILD THIS IS, on the line every operator already reads. A debug server is ~15x
        // slower per turn and the player feels it as ~1.9s of click-to-updated-map versus ~0.1s
        // (docs/plan_delta_streaming.md §8.6) — but nothing in the running game SAYS so, which is
        // how a whole optimisation arc got spent on the other 6% while an unoptimised build
        // supplied the latency. `run_stack.sh` defaults to --release; this is the confirmation
        // that it took, and the first thing to check when "the game feels slow" comes back.
        build_profile = if cfg!(debug_assertions) { "debug" } else { "release" },
        "Shadow-Scale headless server ready (idle — send new_game to generate a world)"
    );

    while let Ok(command) = command_rx.recv() {
        let flat_server: &SnapshotServer = &snapshot_flat_server;
        match command {
            Command::Turn(turns) => {
                if !world_active {
                    warn!(
                        target: "shadow_scale::server",
                        "turn.rejected=no active game — send new_game first"
                    );
                    continue;
                }
                for _ in 0..turns {
                    resolve_turn_with_auto_orders(&mut app);
                    if let Some(log) = command_log.as_mut() {
                        log.push(LogEntry::Turn);
                    }
                }
            }
            Command::ResetMap { width, height } => {
                if width == 0 || height == 0 {
                    warn!(
                        target: "shadow_scale::server",
                        width,
                        height,
                        "map.reset.rejected=invalid_dimensions"
                    );
                    continue;
                }
                let command_sender = {
                    let res = app.world.resource::<CommandSenderResource>();
                    res.0.clone()
                };
                let current_config = app.world.resource::<SimulationConfig>().clone();
                let seed_random_requested = {
                    let metadata = app.world.resource::<SimulationConfigMetadata>();
                    metadata.seed_random()
                };
                let preset_seed = {
                    let presets = app.world.resource::<MapPresetsHandle>();
                    presets
                        .get()
                        .get(&current_config.map_preset_id)
                        .and_then(|preset| preset.map_seed)
                };
                let should_randomize_seed = seed_random_requested && preset_seed.is_none();
                let same_dimensions =
                    current_config.grid_size.x == width && current_config.grid_size.y == height;
                let watch_paths = collect_watch_paths(&app);
                info!(
                    target: "shadow_scale::server",
                    width,
                    height,
                    same_dimensions,
                    "map.reset.begin"
                );
                let mut new_config = current_config.clone();
                new_config.grid_size = UVec2::new(width, height);
                if should_randomize_seed {
                    new_config.map_seed = 0;
                }

                retire_publisher(&mut app);
                app = rebuild_world_from_config(
                    new_config,
                    seed_random_requested,
                    command_sender,
                    &watch_paths,
                    &snapshot_flat_server,
                    &mut world_epoch,
                    |_| {},
                );
                world_active = true;
                // A new world: nothing before this point is reachable.
                command_log = Some(CommandLog::new(&app));
                info!(
                    target: "shadow_scale::server",
                    width,
                    height,
                    same_dimensions,
                    "map.reset.completed"
                );
            }
            Command::NewGame {
                preset_id,
                width,
                height,
                seed,
                profile_id,
            } => {
                handle_new_game(
                    &mut app,
                    &mut world_active,
                    &mut world_epoch,
                    preset_id,
                    width,
                    height,
                    seed,
                    profile_id,
                    &snapshot_flat_server,
                );
            }
            other => {
                // Logged BEFORE it runs, and on the same uniform seam every command already passes
                // through: a new command variant is logged whether or not anyone remembers it
                // exists. `Rollback` is excluded because it is not part of the timeline — it moves
                // through it, and logging it would make a rollback replay itself.
                // A config reload is not replayable — a `SimState` carries no config by design, so
                // replaying across one would run turns under whatever tuning is live rather than
                // that tick's. It re-bases the origin instead of being logged.
                let rebases_origin = matches!(other, Command::ReloadConfig { .. });
                if let Some(log) = command_log.as_mut() {
                    log_dispatched_command(log, &other);
                }
                if let Command::Rollback { tick } = other {
                    if let Some(log) = command_log.as_mut() {
                        handle_rollback(&mut app, tick, flat_server, log);
                    }
                } else {
                    apply_command(&mut app, other, flat_server);
                    if rebases_origin {
                        if let Some(log) = command_log.as_mut() {
                            log.rebase(&app, "config_reload");
                        }
                    }
                }
            }
        }

        // Re-capture + broadcast the fresh world (incl. the feed) so an immediate, synchronous
        // command mutation (expedition launch, move_band, assign_labor, …) reaches the client now,
        // not only at the next turn (replaces the feed-only splice that reused last turn's world).
        // Gated on `world_active`: on the idle (pre-`new_game`) world there is no `ElevationField`,
        // so recapture would panic in the Snapshot stage.
        if world_active {
            recapture_and_broadcast(&mut app);
        }
    }
}

#[derive(Debug, Clone)]
enum Command {
    Turn(u32),
    ResetMap {
        width: u32,
        height: u32,
    },
    Heat {
        target_x: u32,
        target_y: u32,
        delta: i64,
    },
    Orders {
        faction: FactionId,
        orders: FactionOrders,
    },
    Rollback {
        tick: u64,
    },
    AxisBias {
        axis: usize,
        value: f32,
    },
    SupportInfluencer {
        id: u32,
        magnitude: f32,
    },
    SuppressInfluencer {
        id: u32,
        magnitude: f32,
    },
    SupportInfluencerChannel {
        id: u32,
        channel: SupportChannel,
        magnitude: f32,
    },
    SpawnInfluencer {
        scope: Option<InfluenceScopeKind>,
        generation: Option<GenerationId>,
    },
    InjectCorruption {
        subsystem: CorruptionSubsystem,
        intensity: f32,
        exposure_timer: u16,
    },
    UpdateEspionageGenerators {
        updates: Vec<CommandGeneratorUpdate>,
    },
    QueueEspionageMission {
        params: QueueMissionParams,
    },
    UpdateEspionageQueueDefaults {
        scheduled_tick_offset: Option<u64>,
        target_tier: Option<u8>,
    },
    UpdateCounterIntelPolicy {
        faction: FactionId,
        policy: SecurityPolicy,
    },
    AdjustCounterIntelBudget {
        faction: FactionId,
        reserve: Option<Scalar>,
        delta: Option<Scalar>,
    },
    ReloadConfig {
        kind: ReloadConfigKind,
        path: Option<String>,
    },
    SetCrisisAutoSeed {
        enabled: bool,
    },
    SetFogEnabled {
        enabled: bool,
    },
    SpawnCrisis {
        faction: FactionId,
        archetype_id: String,
    },
    SetStartProfile {
        profile_id: String,
    },
    AssignLabor {
        faction: FactionId,
        band_id: Option<u64>,
        role: String,
        workers: u32,
        target_x: Option<u32>,
        target_y: Option<u32>,
        fauna_id: Option<String>,
        /// Which named plant a forage `Cultivate`/`Sow` should commit the patch to (a
        /// `flora_config.json` species key); `None` = auto-pick the tile's dominant legal plant.
        species: Option<String>,
        /// **Where the crew stops, as a fraction of the source's `K`.** `None` = the sim's default
        /// ([`core_sim::DEFAULT_ESCAPEMENT_FLOOR`]); an out-of-range value is **rejected**, never
        /// clamped.
        floor: Option<f32>,
    },
    MoveBand {
        faction: FactionId,
        band_id: Option<u64>,
        target_x: u32,
        target_y: u32,
    },
    SendExpedition {
        faction: FactionId,
        band_id: Option<u64>,
        party_workers: u32,
        target_x: u32,
        target_y: u32,
    },
    RecallExpedition {
        faction: FactionId,
        expedition_band_id: u64,
    },
    SendHuntExpedition {
        faction: FactionId,
        band_id: Option<u64>,
        party_workers: u32,
        fauna_id: String,
        floor: Option<f32>,
    },
    FoundSettlement {
        faction: FactionId,
        target_x: u32,
        target_y: u32,
    },
    Tame {
        faction: FactionId,
        herd_id: String,
    },
    /// The Telling: answer a pending narrative fork with one of its authored choices.
    AnswerFork {
        faction: FactionId,
        beat_id: String,
        choice_id: String,
    },
    Cultivate {
        faction: FactionId,
        target_x: u32,
        target_y: u32,
    },
    Sow {
        faction: FactionId,
        target_x: u32,
        target_y: u32,
    },
    Corral {
        faction: FactionId,
        target_x: u32,
        target_y: u32,
    },
    /// Abandon whatever improvement is in flight on one source (issue #442) — `kind` picks the web,
    /// `target_*` names a patch, `fauna_id` a herd. See `handle_abandon_improvement`.
    AbandonImprovement {
        faction: FactionId,
        kind: String,
        target_x: u32,
        target_y: u32,
        fauna_id: String,
    },
    ExtendPen {
        faction: FactionId,
        target_x: u32,
        target_y: u32,
    },
    CancelOrder {
        faction: FactionId,
        band_id: Option<u64>,
        /// What the cancel clears: everything (+ travel), the worked sources, or the standing roles.
        scope: CancelScope,
    },
    ExportMap {
        path: Option<String>,
    },
    /// Republish the world as a FULL snapshot — delta-streaming recovery (see `ResyncCommand`).
    Resync,
    /// Boot-idle new game: generate a world on demand (the server boots with none). `seed == 0`
    /// randomizes the map seed (mirrors `ResetMap`); an unknown `profile_id` is rejected. Field 43.
    NewGame {
        preset_id: String,
        width: u32,
        height: u32,
        seed: u64,
        profile_id: String,
    },
    /// Stage a sparse config patch for the **next** `new_game`. Validated and installed by
    /// `core_sim::install_config_override`; the running world is never touched.
    SetConfigOverride {
        kind: ConfigOverrideKind,
        patch_json: String,
    },
    /// Drop every staged override, so the next `new_game` boots on the shipped configs.
    ClearConfigOverrides,
}

enum InfluencerAction {
    Support,
    Suppress,
}

#[derive(Resource, Clone)]
struct CommandSenderResource(Sender<Command>);

#[derive(Resource, Default)]
struct ConfigWatcherRegistry {
    simulation: Option<FileWatcherHandle>,
    turn_pipeline: Option<FileWatcherHandle>,
    snapshot_overlays: Option<FileWatcherHandle>,
    crisis_archetypes: Option<FileWatcherHandle>,
    crisis_modifiers: Option<FileWatcherHandle>,
    crisis_telemetry: Option<FileWatcherHandle>,
}

impl ConfigWatcherRegistry {
    fn restart_simulation(&mut self, path: Option<PathBuf>, sender: Sender<Command>) {
        if let Some(existing) = self.simulation.take() {
            existing.stop();
        }

        if let Some(path) = path {
            match start_file_watcher(path.clone(), sender, ReloadConfigKind::Simulation) {
                Ok(watcher) => {
                    info!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        "simulation_config.watch_started"
                    );
                    self.simulation = Some(watcher);
                }
                Err(err) => {
                    warn!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        error = %err,
                        "simulation_config.watch_failed"
                    );
                }
            }
        } else {
            info!(
                target: "shadow_scale::config",
                "simulation_config.watch_disabled"
            );
        }
    }

    fn restart_turn_pipeline(&mut self, path: Option<PathBuf>, sender: Sender<Command>) {
        if let Some(existing) = self.turn_pipeline.take() {
            existing.stop();
        }

        if let Some(path) = path {
            match start_file_watcher(path.clone(), sender, ReloadConfigKind::TurnPipeline) {
                Ok(watcher) => {
                    info!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        "turn_pipeline_config.watch_started"
                    );
                    self.turn_pipeline = Some(watcher);
                }
                Err(err) => {
                    warn!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        error = %err,
                        "turn_pipeline_config.watch_failed"
                    );
                }
            }
        } else {
            info!(
                target: "shadow_scale::config",
                "turn_pipeline_config.watch_disabled"
            );
        }
    }

    fn restart_snapshot_overlays(&mut self, path: Option<PathBuf>, sender: Sender<Command>) {
        if let Some(existing) = self.snapshot_overlays.take() {
            existing.stop();
        }

        if let Some(path) = path {
            match start_file_watcher(path.clone(), sender, ReloadConfigKind::SnapshotOverlays) {
                Ok(watcher) => {
                    info!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        "snapshot_overlays_config.watch_started"
                    );
                    self.snapshot_overlays = Some(watcher);
                }
                Err(err) => {
                    warn!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        error = %err,
                        "snapshot_overlays_config.watch_failed"
                    );
                }
            }
        } else {
            info!(
                target: "shadow_scale::config",
                "snapshot_overlays_config.watch_disabled"
            );
        }
    }

    fn restart_crisis_archetypes(&mut self, path: Option<PathBuf>, sender: Sender<Command>) {
        if let Some(existing) = self.crisis_archetypes.take() {
            existing.stop();
        }

        if let Some(path) = path {
            match start_file_watcher(path.clone(), sender, ReloadConfigKind::CrisisArchetypes) {
                Ok(watcher) => {
                    info!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        "crisis_archetypes.watch_started"
                    );
                    self.crisis_archetypes = Some(watcher);
                }
                Err(err) => {
                    warn!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        error = %err,
                        "crisis_archetypes.watch_failed"
                    );
                }
            }
        } else {
            info!(
                target: "shadow_scale::config",
                "crisis_archetypes.watch_disabled"
            );
        }
    }

    fn restart_crisis_modifiers(&mut self, path: Option<PathBuf>, sender: Sender<Command>) {
        if let Some(existing) = self.crisis_modifiers.take() {
            existing.stop();
        }

        if let Some(path) = path {
            match start_file_watcher(path.clone(), sender, ReloadConfigKind::CrisisModifiers) {
                Ok(watcher) => {
                    info!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        "crisis_modifiers.watch_started"
                    );
                    self.crisis_modifiers = Some(watcher);
                }
                Err(err) => {
                    warn!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        error = %err,
                        "crisis_modifiers.watch_failed"
                    );
                }
            }
        } else {
            info!(
                target: "shadow_scale::config",
                "crisis_modifiers.watch_disabled"
            );
        }
    }

    fn restart_crisis_telemetry(&mut self, path: Option<PathBuf>, sender: Sender<Command>) {
        if let Some(existing) = self.crisis_telemetry.take() {
            existing.stop();
        }

        if let Some(path) = path {
            match start_file_watcher(path.clone(), sender, ReloadConfigKind::CrisisTelemetry) {
                Ok(watcher) => {
                    info!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        "crisis_telemetry_config.watch_started"
                    );
                    self.crisis_telemetry = Some(watcher);
                }
                Err(err) => {
                    warn!(
                        target: "shadow_scale::config",
                        path = %path.display(),
                        error = %err,
                        "crisis_telemetry_config.watch_failed"
                    );
                }
            }
        } else {
            info!(
                target: "shadow_scale::config",
                "crisis_telemetry_config.watch_disabled"
            );
        }
    }
}

struct FileWatcherHandle {
    stop_tx: mpsc::Sender<()>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl FileWatcherHandle {
    fn stop(mut self) {
        let _ = self.stop_tx.send(());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for FileWatcherHandle {
    fn drop(&mut self) {
        let _ = self.stop_tx.send(());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

const MAX_PROTO_FRAME: usize = 64 * 1024;

/// Starts the command listener on an already-bound listener. Binding happens
/// up front in `port_alloc::allocate`, so this can no longer panic on a port
/// conflict.
fn spawn_command_listener(listener: TcpListener) -> (Receiver<Command>, Sender<Command>) {
    if let Err(err) = listener.set_nonblocking(true) {
        warn!("Failed to set nonblocking on command listener: {}", err);
    }

    let (sender, receiver) = unbounded::<Command>();
    let sender_for_thread = sender.clone();
    thread::spawn(move || loop {
        match listener.accept() {
            Ok((stream, addr)) => {
                info!("Command client connected: {}", addr);
                let sender = sender_for_thread.clone();
                thread::spawn(move || handle_proto_client(stream, sender));
            }
            Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(err) => {
                warn!("Error accepting command client: {}", err);
                thread::sleep(std::time::Duration::from_millis(200));
            }
        }
    });

    (receiver, sender)
}

fn handle_proto_client(stream: TcpStream, sender: Sender<Command>) {
    let mut reader = BufReader::new(stream);
    loop {
        let mut len_buf = [0u8; 4];
        match reader.read_exact(&mut len_buf) {
            Ok(_) => {}
            Err(err) => {
                if err.kind() != io::ErrorKind::UnexpectedEof {
                    warn!("Proto command length read error: {}", err);
                }
                break;
            }
        }
        let frame_len = u32::from_le_bytes(len_buf) as usize;
        if frame_len == 0 {
            warn!("Proto command received empty frame");
            continue;
        }
        if frame_len > MAX_PROTO_FRAME {
            warn!(
                "Proto command frame too large ({} bytes), dropping connection",
                frame_len
            );
            break;
        }
        let mut payload = vec![0u8; frame_len];
        if let Err(err) = reader.read_exact(&mut payload) {
            if err.kind() != io::ErrorKind::UnexpectedEof {
                warn!("Proto command payload read error: {}", err);
            }
            break;
        }
        match ProtoCommandEnvelope::decode(&payload) {
            Ok(envelope) => {
                if let Some(cmd) = command_from_payload(envelope.payload) {
                    if sender.send(cmd).is_err() {
                        break;
                    }
                }
            }
            Err(err) => {
                warn!("Proto command decode error: {}", err);
            }
        }
    }
}

fn start_file_watcher(
    path: PathBuf,
    sender: Sender<Command>,
    kind: ReloadConfigKind,
) -> notify::Result<FileWatcherHandle> {
    let (ready_tx, ready_rx) = mpsc::channel();
    let (stop_tx, stop_rx) = mpsc::channel();
    let watcher_path = path.clone();

    let handle = thread::spawn(move || {
        let (event_tx, event_rx) = mpsc::channel();
        match notify::recommended_watcher(move |res| {
            let _ = event_tx.send(res);
        }) {
            Ok(mut watcher) => {
                if let Err(err) = watcher.watch(&watcher_path, RecursiveMode::NonRecursive) {
                    let _ = ready_tx.send(Err(err));
                    return;
                }
                let _ = ready_tx.send(Ok(()));
                watch_config(watcher_path, watcher, event_rx, stop_rx, sender, kind);
            }
            Err(err) => {
                let _ = ready_tx.send(Err(err));
            }
        }
    });

    match ready_rx.recv() {
        Ok(Ok(())) => Ok(FileWatcherHandle {
            stop_tx,
            handle: Some(handle),
        }),
        Ok(Err(err)) => {
            let _ = stop_tx.send(());
            let _ = handle.join();
            Err(err)
        }
        Err(_) => {
            let _ = stop_tx.send(());
            let _ = handle.join();
            Err(notify::Error::generic(
                "config watcher initialization channel closed",
            ))
        }
    }
}

fn watch_config(
    path: PathBuf,
    mut watcher: RecommendedWatcher,
    event_rx: mpsc::Receiver<notify::Result<notify::Event>>,
    stop_rx: mpsc::Receiver<()>,
    sender: Sender<Command>,
    kind: ReloadConfigKind,
) {
    let debounce = Duration::from_millis(250);
    let mut last_emit = Instant::now() - debounce;

    loop {
        if stop_rx.try_recv().is_ok() {
            break;
        }

        match event_rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Ok(event)) => match event.kind {
                EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                    if last_emit.elapsed() >= debounce =>
                {
                    if sender
                        .send(Command::ReloadConfig { kind, path: None })
                        .is_err()
                    {
                        break;
                    }
                    last_emit = Instant::now();
                }
                _ => {}
            },
            Ok(Err(err)) => {
                warn!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "simulation_config.watch_event_error"
                );
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let _ = watcher.unwatch(&path);
}

fn handle_reload_config(
    app: &mut bevy::prelude::App,
    kind: ReloadConfigKind,
    path: Option<String>,
) {
    match kind {
        ReloadConfigKind::Simulation => handle_reload_simulation_config(app, path),
        ReloadConfigKind::TurnPipeline => handle_reload_turn_pipeline_config(app, path),
        ReloadConfigKind::SnapshotOverlays => handle_reload_snapshot_overlays_config(app, path),
        ReloadConfigKind::CrisisArchetypes => handle_reload_crisis_archetypes_config(app, path),
        ReloadConfigKind::CrisisModifiers => handle_reload_crisis_modifiers_config(app, path),
        ReloadConfigKind::CrisisTelemetry => handle_reload_crisis_telemetry_config(app, path),
    }
}

/// Write the current world map (terrain snapshot + resolved seed/preset) to disk
/// as JSON for offline inspection and as a test fixture. Never panics: on any
/// failure it logs a warning and returns, leaving the simulation untouched.
fn write_map_export(app: &bevy::prelude::App, requested_path: Option<String>) {
    let snapshot = {
        let history = app.world.resource::<SnapshotHistory>();
        match history.last_snapshot().clone() {
            Some(snapshot) => snapshot,
            None => {
                warn!(
                    target: "shadow_scale::server",
                    "map.export.rejected=no_snapshot"
                );
                return;
            }
        }
    };

    // `spawn_initial_world` resolves the (possibly random) seed and writes it
    // back into `SimulationConfig.map_seed`, so the config is the seed's source
    // of truth by the time any command is handled.
    let (seed, preset) = {
        let config = app.world.resource::<SimulationConfig>();
        (config.map_seed, config.map_preset_id.clone())
    };
    let tick = snapshot.header.tick;

    let export = MapExport::from_snapshot(seed, preset, (*snapshot).clone());

    let path = match requested_path {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(DEFAULT_EXPORT_DIR).join(format!("map-tick{tick}-seed{seed}.json")),
    };

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(err) = std::fs::create_dir_all(parent) {
                warn!(
                    target: "shadow_scale::server",
                    error = %err,
                    path = %path.display(),
                    "map.export.failed=create_dir"
                );
                return;
            }
        }
    }

    let json = match encode_map_export_json(&export) {
        Ok(json) => json,
        Err(err) => {
            warn!(
                target: "shadow_scale::server",
                error = %err,
                "map.export.failed=encode"
            );
            return;
        }
    };

    match std::fs::write(&path, json) {
        Ok(()) => info!(
            target: "shadow_scale::server",
            path = %path.display(),
            seed,
            tick,
            width = export.width,
            height = export.height,
            "map.export.completed"
        ),
        Err(err) => warn!(
            target: "shadow_scale::server",
            error = %err,
            path = %path.display(),
            "map.export.failed=write"
        ),
    }
}

/// Stage a config-tuning override for the next `new_game`.
///
/// **Rejection is the interesting case, and it is not defensive coding.** `load_config_from_env`
/// panics on a present-but-broken file *by design* (`.claude/rules/core_sim/config-loading.md`), so
/// an override installed without validating it would not fail here — it would kill the server at
/// the next New Game, arbitrarily far from the edit that caused it. `install_config_override`
/// therefore parses the merged config through the kind's own `from_json_str` (where `validate` runs)
/// **before** anything is written or registered, and a failure changes nothing at all: no file, no
/// registry entry, and — since this handler never touches `app` — no effect on the running world.
///
/// Hence `warn!` rather than a panic or an error!: the operator is sitting at a UI that will show
/// them the refusal, and the world they are playing is fine.
fn handle_set_config_override(kind: ConfigOverrideKind, patch_json: &str) {
    match install_config_override(kind, patch_json, Path::new(DEFAULT_CONFIG_OVERRIDE_DIR)) {
        Ok(installed) => info!(
            target: "shadow_scale::server",
            kind = kind.as_str(),
            path = %installed.path.display(),
            "config_override.installed"
        ),
        Err(err) => warn!(
            target: "shadow_scale::server",
            kind = kind.as_str(),
            error = %err,
            "config_override.rejected"
        ),
    }
}

/// The config-file watch paths carried across a world rebuild, so the fresh app keeps watching the
/// same files the old one did. Gathered once from the live app by [`collect_watch_paths`].
struct WatchPaths {
    simulation: Option<PathBuf>,
    turn_pipeline: Option<PathBuf>,
    snapshot_overlays: Option<PathBuf>,
    crisis_archetypes: Option<PathBuf>,
    crisis_modifiers: Option<PathBuf>,
    crisis_telemetry: Option<PathBuf>,
}

fn collect_watch_paths(app: &bevy::prelude::App) -> WatchPaths {
    WatchPaths {
        simulation: app
            .world
            .resource::<SimulationConfigMetadata>()
            .path()
            .cloned(),
        turn_pipeline: app
            .world
            .resource::<TurnPipelineConfigMetadata>()
            .path()
            .cloned(),
        snapshot_overlays: app
            .world
            .resource::<SnapshotOverlaysConfigMetadata>()
            .path()
            .cloned(),
        crisis_archetypes: app
            .world
            .resource::<CrisisArchetypeCatalogMetadata>()
            .path()
            .cloned(),
        crisis_modifiers: app
            .world
            .resource::<CrisisModifierCatalogMetadata>()
            .path()
            .cloned(),
        crisis_telemetry: app
            .world
            .resource::<CrisisTelemetryConfigMetadata>()
            .path()
            .cloned(),
    }
}

/// Drain and finish the outgoing world's snapshot publisher before the next world's is started.
///
/// Both rebuild paths (`ResetMap`, `NewGame`) construct the new `App` — first turn and baseline
/// frame included — *before* the old one is dropped, so without this the two publishers overlap and
/// a previous-world frame can reach the socket after the new world's baseline. The client would
/// survive it (a stale-epoch delta names a `base_frame_seq` it does not hold, so it is dropped), but
/// "the previous world's frames arrive after the new world's" is the exact failure
/// `.claude/rules/core_sim/world-handoff.md` exists to prevent, and it should not rest on the client
/// noticing.
fn retire_publisher(app: &mut bevy::prelude::App) {
    app.world.resource_mut::<SnapshotHistory>().shutdown();
}

/// The shared world-build path used by BOTH `ResetMap` and `NewGame`: build a fresh headless app on
/// `config`, re-attach the runtime resources and file watchers, run one Startup pass (worldgen), and
/// broadcast the first snapshot. `configure` runs after the config/metadata/watchers are in place but
/// BEFORE Startup, so a caller (e.g. `new_game`) can apply a start profile that worldgen must see.
/// Returns the new app for the caller to swap in.
#[allow(clippy::too_many_arguments)]
fn rebuild_world_from_config(
    config: SimulationConfig,
    seed_random: bool,
    command_sender: Sender<Command>,
    watch_paths: &WatchPaths,
    snapshot_server_flat: &Arc<SnapshotServer>,
    world_epoch: &mut u32,
    configure: impl FnOnce(&mut bevy::prelude::App),
) -> bevy::prelude::App {
    let mut new_app = build_headless_app();
    // The base this process actually bound travels with the config — `new_game` carries the binds
    // over from the outgoing world and `ResetMap` clones them — but the resource does not:
    // `build_headless_app` never inserts it, so without this line the first rebuild dropped it and a
    // later `reload_config` re-applied the *file's* base, leaving the in-world config naming sockets
    // nothing is listening on (the exact thing `ResolvedPortBase` exists to prevent).
    let resolved_port_base = ResolvedPortBase(config.port_base_bind.port());
    {
        let mut config_res = new_app.world.resource_mut::<SimulationConfig>();
        *config_res = config;
    }
    new_app.insert_resource(resolved_port_base);
    new_app.insert_resource(SimulationMetrics::default());
    new_app.insert_resource(CommandSenderResource(command_sender.clone()));
    new_app.insert_resource(ConfigWatcherRegistry::default());
    {
        let mut metadata = new_app.world.resource_mut::<SimulationConfigMetadata>();
        metadata.set_path(watch_paths.simulation.clone());
        metadata.set_seed_random(seed_random);
    }
    {
        let mut metadata = new_app.world.resource_mut::<TurnPipelineConfigMetadata>();
        metadata.set_path(watch_paths.turn_pipeline.clone());
    }
    {
        let mut metadata = new_app
            .world
            .resource_mut::<SnapshotOverlaysConfigMetadata>();
        metadata.set_path(watch_paths.snapshot_overlays.clone());
    }
    {
        let mut metadata = new_app
            .world
            .resource_mut::<CrisisArchetypeCatalogMetadata>();
        metadata.set_path(watch_paths.crisis_archetypes.clone());
    }
    {
        let mut metadata = new_app
            .world
            .resource_mut::<CrisisModifierCatalogMetadata>();
        metadata.set_path(watch_paths.crisis_modifiers.clone());
    }
    {
        let mut metadata = new_app
            .world
            .resource_mut::<CrisisTelemetryConfigMetadata>();
        metadata.set_path(watch_paths.crisis_telemetry.clone());
    }
    {
        let mut watcher_registry = new_app.world.resource_mut::<ConfigWatcherRegistry>();
        watcher_registry.restart_simulation(watch_paths.simulation.clone(), command_sender.clone());
        watcher_registry
            .restart_turn_pipeline(watch_paths.turn_pipeline.clone(), command_sender.clone());
        watcher_registry.restart_snapshot_overlays(
            watch_paths.snapshot_overlays.clone(),
            command_sender.clone(),
        );
        watcher_registry.restart_crisis_archetypes(
            watch_paths.crisis_archetypes.clone(),
            command_sender.clone(),
        );
        watcher_registry
            .restart_crisis_modifiers(watch_paths.crisis_modifiers.clone(), command_sender.clone());
        watcher_registry
            .restart_crisis_telemetry(watch_paths.crisis_telemetry.clone(), command_sender.clone());
    }

    // Advance the world epoch for this fresh world and stamp it onto the app BEFORE the first
    // `run_turn` capture, so every snapshot in this world carries the same epoch (first real world →
    // 1, next rebuild → 2, …). The shared path for BOTH `NewGame` and `ResetMap`.
    *world_epoch += 1;
    new_app.insert_resource(WorldEpoch(*world_epoch));

    // Attach the socket BEFORE the first capture, so this world's baseline frame is broadcast by
    // the publisher like every frame after it. A world whose publisher has no sink still publishes
    // — it simply has no audience — which is exactly the state the idle boot app and every test
    // run in.
    new_app
        .world
        .resource::<SnapshotHistory>()
        .attach_sink(Arc::clone(snapshot_server_flat) as Arc<dyn FrameSink>);

    // Apply any caller-supplied configuration (e.g. the start profile) before Startup worldgen runs.
    configure(&mut new_app);

    run_turn(&mut new_app);

    new_app
}

/// Generate a world on demand from the `new_game` wire command (the server boots idle). Validates
/// dimensions and the start profile, then rebuilds the world through the shared
/// [`rebuild_world_from_config`] path and flips `world_active` so turns are accepted.
#[allow(clippy::too_many_arguments)]
fn handle_new_game(
    app: &mut bevy::prelude::App,
    world_active: &mut bool,
    world_epoch: &mut u32,
    preset_id: String,
    width: u32,
    height: u32,
    seed: u64,
    profile_id: String,
    snapshot_server_flat: &Arc<SnapshotServer>,
) {
    if width == 0 || height == 0 {
        warn!(
            target: "shadow_scale::server",
            width,
            height,
            preset = %preset_id,
            "new_game.rejected=invalid_dimensions"
        );
        return;
    }

    // Resolve the requested start profile. An unknown `profile_id` is a hard reject — we do NOT build
    // a world with an arbitrary fallback profile. (An unknown `preset_id`, by contrast, falls through
    // to the worldgen default, mirroring ResetMap.)
    let handle = app.world.resource::<StartProfilesHandle>().clone();
    let (profile, used_fallback) = resolve_active_profile(&handle, &profile_id);
    if used_fallback {
        warn!(
            target: "shadow_scale::server",
            requested = %profile_id,
            "new_game.rejected=unknown_profile"
        );
        return;
    }

    let command_sender = {
        let res = app.world.resource::<CommandSenderResource>();
        res.0.clone()
    };
    let watch_paths = collect_watch_paths(app);

    // Start from the config that would load RIGHT NOW, not from a clone of the outgoing world's:
    // a `simulation` override staged by the tuning panel reaches a world only because New Game
    // re-reads every config, and cloning skipped that read (see
    // `load_simulation_config_for_new_world`, which also carries back the narrow set of fields the
    // file cannot know).
    // The *watched* path is deliberately left alone — `rebuild_world_from_config` keeps this
    // server's `SimulationConfigMetadata` path, so a staged override never becomes the file the
    // watcher hot-reloads into a running world.
    let mut new_config =
        load_simulation_config_for_new_world(app.world.resource::<SimulationConfig>());
    new_config.grid_size = UVec2::new(width, height);
    new_config.map_preset_id = preset_id.clone();
    // `seed == 0` randomizes: worldgen resolves a `map_seed` of 0 to a fresh entropy seed, exactly the
    // mechanism ResetMap uses (map_seed 0 + seed_random true).
    new_config.map_seed = seed;

    info!(
        target: "shadow_scale::server",
        preset = %preset_id,
        width,
        height,
        seed,
        profile = %profile.id,
        "new_game.begin"
    );

    retire_publisher(app);
    *app = rebuild_world_from_config(
        new_config,
        seed == 0,
        command_sender,
        &watch_paths,
        snapshot_server_flat,
        world_epoch,
        move |new_app| apply_start_profile(new_app, &profile),
    );
    *world_active = true;

    info!(
        target: "shadow_scale::server",
        preset = %preset_id,
        "new_game.completed"
    );
}

/// Apply a resolved start profile to the app's campaign resources (config overrides,
/// `StartProfileLookup`, `ActiveStartProfile`, `CampaignLabel`). Shared by `handle_set_start_profile`
/// and the `new_game` rebuild — it does NOT regenerate the world; the caller runs Startup afterward.
fn apply_start_profile(app: &mut bevy::prelude::App, profile: &StartProfile) {
    {
        let mut config = app.world.resource_mut::<SimulationConfig>();
        config.start_profile_id = profile.id.clone();
        config.start_profile_overrides = StartProfileOverrides::from_profile(profile);
    }
    {
        let mut lookup = app.world.resource_mut::<StartProfileLookup>();
        lookup.id = profile.id.clone();
    }
    {
        let mut active = app.world.resource_mut::<ActiveStartProfile>();
        *active = ActiveStartProfile::new(profile.clone());
    }
    {
        let mut label = app.world.resource_mut::<CampaignLabel>();
        *label = CampaignLabel::from_profile(profile);
    }
}

fn handle_set_start_profile(app: &mut bevy::prelude::App, profile_id: String) {
    let handle = app.world.resource::<StartProfilesHandle>().clone();
    let (profile, used_fallback) = resolve_active_profile(&handle, &profile_id);

    apply_start_profile(app, &profile);

    info!(
        target: "shadow_scale::campaign",
        requested = %profile_id,
        applied = %profile.id,
        fallback = used_fallback,
        "start_profile.updated"
    );

    if used_fallback {
        warn!(
            target: "shadow_scale::campaign",
            requested = %profile_id,
            applied = %profile.id,
            "start_profile.fallback_applied"
        );
    }
}

// **RETIRED: `parse_follow_policy(Option<&str>) -> FollowPolicy`** — the assign-labor path's stance
// parse, which warned and defaulted to Sustain on an unparseable token.
//
// Its last caller was `handle_assign_labor`, and the harvest floor arc replaced the token it parsed
// with a `floor: Option<f32>` that **fails closed** (`floor_is_valid`, rejected with a command
// failure, never clamped). That is the opposite discipline: this function's whole behaviour was to
// keep going on a typo, which is defensible for a four-value picker and is not for the one number
// the harvest model turns on.
//
// It is **not** what `send_hunt_expedition` uses — that path has always parsed its own token with
// its own parse and *rejects* an unusable one, so routing it here would have loosened a
// gate rather than shared one.

fn handle_found_settlement(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    target_x: u32,
    target_y: u32,
) {
    let target = UVec2::new(target_x, target_y);
    let Some(_tile_entity) = ensure_land_tile(
        app,
        faction,
        target,
        "found_settlement",
        Some(CommandEventKind::FoundSettlement),
    ) else {
        return;
    };

    // Reject if a settlement already exists on this tile.
    {
        let mut query = app.world.query::<&Settlement>();
        if query
            .iter(&app.world)
            .any(|settlement| settlement.position == target)
        {
            emit_command_failure(
                app,
                CommandEventKind::FoundSettlement,
                faction,
                "A settlement already exists at that location.",
            );
            return;
        }
    }

    // Require a Founders band to be present and consume it.
    let Some(founders) = select_founder_band(app, faction, CommandEventKind::FoundSettlement)
    else {
        return;
    };

    if !consume_faction_provisions(
        app,
        faction,
        SETTLEMENT_PROVISION_COST,
        "found_settlement",
        CommandEventKind::FoundSettlement,
    ) {
        return;
    }

    let removed = app.world.despawn(founders.entity);
    if !removed {
        warn!(
            target: "shadow_scale::command",
            command = "found_settlement",
            faction = %faction.0,
            entity_bits = founders.entity.to_bits(),
            "command.found_settlement.rejected=despawn_failed"
        );
        emit_command_failure(
            app,
            CommandEventKind::FoundSettlement,
            faction,
            "Failed to consume the Founders unit.",
        );
        return;
    }

    let construction_radius = SETTLEMENT_CONSTRUCTION_RADIUS;
    let logistics_radius = SETTLEMENT_LOGISTICS_RADIUS;

    let settlement_entity = app.world.spawn((
        Settlement {
            faction,
            position: target,
        },
        TownCenter {
            construction_radius,
            logistics_radius,
        },
    ));
    let settlement_id = settlement_entity.id();

    // Re-home the campaign start marker on the new hub.
    let tick = app.world.resource::<SimulationTick>().0;
    let Some(mut start_location) = app.world.get_resource_mut::<StartLocation>() else {
        warn!(
            target: "shadow_scale::command",
            command = "found_settlement",
            faction = %faction.0,
            "command.found_settlement.rejected=start_location_missing"
        );
        return;
    };
    start_location.relocate(target);

    push_command_event(
        app,
        tick,
        CommandEventKind::CampaignFounded,
        faction,
        format!("Settlement -> ({}, {})", target_x, target_y),
        Some(format!(
            "construction_radius={} logistics_radius={} cost={} provisions founders_entity={}",
            construction_radius,
            logistics_radius,
            SETTLEMENT_PROVISION_COST,
            settlement_id.to_bits()
        )),
    );
}

/// Fetch (or insert a default) mutable [`LaborAllocation`] on a band entity.
fn band_allocation_mut(
    app: &mut bevy::prelude::App,
    band: Entity,
) -> bevy::prelude::Mut<'_, LaborAllocation> {
    if app.world.get::<LaborAllocation>(band).is_none() {
        app.world
            .entity_mut(band)
            .insert(LaborAllocation::default());
    }
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("labor allocation inserted above")
}

/// Seed the touched source's **yield telemetry** (`LaborAllocation.last_yields`) from its
/// **pre-commit forecast**, right after the allocation is mutated.
///
/// Without this, telemetry is only ever written during turn resolution (`advance_labor_allocation`),
/// so between "player assigns workers" and "player advances the turn" a brand-new source has no row
/// and the display snapshot serializes `actual_yield = 0.0` — the client cannot tell "0 because not
/// computed yet" from "0 because the source is barren", and every fresh assignment reads `+0.00`.
///
/// The seeded row is exactly what the turn will pay under unchanged conditions: it reuses the same
/// forecast helpers the take path reads (`forecast == actual` — see "Pre-commit Yield Forecast" in
/// `core_sim/CLAUDE.md`), and it is the same number the client's compose-time "Expected yield" row
/// promises — so there is no jump when the turn lands, and it is overwritten by the resolved take.
///
/// Only the **one source the command touched** is seeded (other sources keep their real actuals), and
/// only where the resolution path would actually pay: a source the turn would skip (out of the band's
/// work range / past the hunt leash, an unseeded patch, a vanished herd) keeps its zero row, and a
/// genuinely barren source seeds `0.0` — `+0.00` stays reachable, and correct, there.
fn seed_source_yield(
    app: &mut bevy::prelude::App,
    band: Entity,
    target: &LaborTarget,
    improvement: Option<Improvement>,
    workers: u32,
) {
    // Unassigned (`workers == 0`): `set_assignment` already dropped the source's row with its
    // assignment. Scout/Warrior are band-wide roles with no food yield — the resolution path leaves
    // them at zero, so seeding must too.
    if workers == 0
        || !matches!(
            target,
            LaborTarget::Forage { .. } | LaborTarget::Hunt { .. }
        )
    {
        return;
    }
    let Some(cohort) = app.world.get::<PopulationCohort>(band) else {
        return;
    };
    let current_tile = cohort.current_tile;
    // The band's productivity multiplier is applied at payout in the resolution path, so the forecast
    // must fold it in too (the snapshot's per-source forecast is captured at 1.0 and scaled client-side
    // by this same multiplier).
    let wellbeing = app.world.resource::<WellbeingConfigHandle>().get();
    let output_mult = output_multiplier(cohort, &wellbeing).to_f32();
    let Some(band_pos) = app
        .world
        .get::<Tile>(current_tile)
        .map(|tile| tile.position)
    else {
        return;
    };
    let grid_width = app.world.resource::<TileRegistry>().width;
    let wrap_horizontal = app
        .world
        .resource::<SimulationConfig>()
        .map_topology
        .wrap_horizontal;
    let labor = app.world.resource::<LaborConfigHandle>().get();

    let seeded = match target {
        LaborTarget::Forage { tile, floor, .. } => {
            // Out of the band's work range → the turn pays 0 (assignment kept). Keep the zero row.
            if hex_distance_wrapped(band_pos, *tile, grid_width, wrap_horizontal)
                > labor.band_work_range
            {
                return;
            }
            let Some(tile_entity) = app.world.resource::<TileRegistry>().index(tile.x, tile.y)
            else {
                return;
            };
            // No food module → no wild **gather** season (`NO_FORAGE_SEASON`), exactly as the labor
            // arm reads it. Not an early return: since slice 5 a sown Field may stand on a
            // module-less tile, and its managed harvest is biomass-based and seasonless — returning
            // here would seed that Field a `0` row and reintroduce the `+0.00`-then-jump bug the seed
            // exists to kill.
            let seasonal = app
                .world
                .get::<FoodModuleTag>(tile_entity)
                .map_or(NO_FORAGE_SEASON, |module| module.seasonal_weight.max(0.0));
            let Some(patch) = app.world.resource::<ForageRegistry>().patch(*tile) else {
                return; // unseeded patch → the turn pays 0 (a bare-ground sow's honest opening row).
            };
            let ladder = app.world.resource::<LadderConfigHandle>().get();
            let flora = app.world.resource::<FloraConfigHandle>().get();
            // **What is growing on this tile** — the same realized basket the labor arm and the
            // snapshot read, so the assign-time seed is priced off the identical composition the
            // turn will pay from (#433). Absent ground names no plants.
            let map_seed = app.world.resource::<SimulationConfig>().map_seed;
            let tile_composition = app.world.get::<Tile>(tile_entity).map_or_else(
                || Cow::Owned(Vec::new()),
                |ground| tile_flora_composition(&flora, &labor.forage, ground, map_seed),
            );
            forage_source_yield_preview(
                patch,
                &tile_composition,
                &labor.forage,
                &flora,
                &ladder,
                seasonal,
                output_mult,
                workers,
                *floor,
                improvement,
                labor.yield_average_horizon_turns,
                labor.arrivals_horizon_turns,
            )
        }
        LaborTarget::Hunt { fauna_id, floor } => {
            let Some(herd) = app.world.resource::<HerdRegistry>().find(fauna_id) else {
                return; // herd gone → the assignment lapses next turn.
            };
            // Past the leash → the assignment lapses next turn; keep the zero row.
            if hex_distance_wrapped(band_pos, herd.position(), grid_width, wrap_horizontal)
                > labor.hunt_reach()
            {
                return;
            }
            let fauna = app.world.resource::<FaunaConfigHandle>().get();
            let ladder = app.world.resource::<LadderConfigHandle>().get();
            hunt_source_yield_preview(
                herd,
                &fauna,
                &ladder,
                labor.hunt.per_worker_biomass_capacity,
                output_mult,
                workers,
                *floor,
                improvement,
                labor.yield_average_horizon_turns,
                labor.arrivals_horizon_turns,
            )
        }
        LaborTarget::Scout | LaborTarget::Warrior => return,
    };
    band_allocation_mut(app, band).set_source_yield(target, seeded);
}

/// Validate a labor target's **stance** against the source it names, returning a player-facing
/// rejection reason (`Err`) or `Ok`.
///
/// **Since issue #442 the stance axis has two rules, and neither is about a policy.**
/// - **Hunt:** `fauna::hunt_policies_for` prunes a [`fauna::HuntYield::yields_nothing`] quarry —
///   worth neither meat nor pelt — down to `Eradicate` alone, because every other rung would be a
///   rate at which to collect nothing.
/// - **Forage:** the **crop the player named**, if they named one. `assign_labor` is the only
///   command that can *set* `LaborTarget::Forage::species`, so it is the only place a bad selection
///   can be caught at the moment it is made — and the only place that sees a re-selection dropped
///   onto a build already in flight.
///
/// Everything else this function used to check was about the **build verbs**, which are no longer
/// stances: the kind-exclusivity check (`Cultivate` on a herd) and the four per-rung gates moved to
/// [`validate_improvement`], where they belong to the axis that carries them. That split is what
/// makes a crew change a *stance-side* edit which never re-asserts a paused build's start gate
/// (`docs/plan_investment_rung_toggle.md` §6).
fn validate_labor_policy(
    app: &bevy::prelude::App,
    faction: FactionId,
    target: &LaborTarget,
) -> Result<(), String> {
    let _ = faction;
    match target {
        LaborTarget::Hunt { fauna_id, floor } => {
            // **A species that pays NOTHING may only be worked at floor `0`.** "Harvest it
            // sustainably" is meaningless for a quarry with no product: the only coherent reason to
            // put hunters on it is to remove it, so every floor that would leave some standing is
            // refused. The predicate is the species' own yield vector — the ONE seam this validator
            // shares with the snapshot's exported ladder (`fauna::hunt_policies_for`), so the picker
            // the client draws and the picker the sim accepts cannot drift into two lists.
            let fauna = app.world.resource::<FaunaConfigHandle>().get();
            let species = app
                .world
                .resource::<HerdRegistry>()
                .find(fauna_id)
                .map(|herd| herd.species.clone());
            if let Some(species) = species {
                let denial_only = core_sim::species_requires_denial(fauna.hunt_yield_for(&species));
                if denial_only && *floor > core_sim::STRIP_IT_BARE {
                    return Err(format!(
                        "The {} yields neither food nor trade goods — the only thing to do with \
                         it is eradicate it.",
                        species
                    ));
                }
            }
            Ok(())
        }
        LaborTarget::Forage { tile, species, .. } => {
            // **CAN THEY GATHER HERE AT ALL?** The plant branch's rung 1 carries a
            // `site_requirement` of its own — the ground must be a **gathering site** — and this is
            // where it is enforced. It is the whole of the early game's scarcity: a `FoodModuleTag`
            // (and therefore a forage patch, and therefore a stand of named plants) sits on ~every
            // land tile, so without this rule a band gathers anywhere it stands and *which* ground
            // it can reach never matters.
            //
            // It was a CLIENT-SIDE rule until this arc — the tile card simply declined to offer the
            // compose sheet off-site — which meant the sim accepted a command no player could send,
            // the card advertised a stand nobody could work, and the rule lived nowhere the sim
            // could see it (issue #464).
            if let Some(refusal) = plant_rung_site_refusal(app, RungKey::PlantWild, *tile) {
                return Err(site_refusal_message(refusal, *tile, "gather"));
            }
            // **Judge the crop the player NAMED, and only then.** Absent means "pick the tile's
            // dominant legal plant for me", which cannot be wrong and — crucially — must not drag
            // an ordinary wild gather into the ladder's refusals: ground whose whole basket is
            // `wild` is perfectly gatherable, and rejecting `assign_labor` there would make the
            // open-water fisheries and the alpine peaks unworkable.
            //
            // **At `PlantTended`, the ladder's ENTRY rung.** `cultivation_ceiling` is a ladder
            // (`allows_sow` implies `allows_cultivate`), so tended is the weaker of the two gates:
            // judging at `PlantField` here would refuse a crop the player may legitimately intend
            // to `cultivate`, and the stance command does not yet know which verb will follow.
            // `handle_sow` re-judges the crew's crop at its own rung.
            let Some(named) = species.as_deref() else {
                return Ok(());
            };
            validate_species_selection(app, *tile, Some(named), RungKey::PlantTended)
        }
        LaborTarget::Scout | LaborTarget::Warrior => Ok(()),
    }
}

/// Validate an **[`Improvement`]** against the source it names — the gate half of the second axis,
/// and the home of every check `validate_labor_policy` used to carry for a build verb.
///
/// 1. **The web must match the source kind.** `Cultivate`/`Sow` are plant-only, `Tame`/`Corral`
///    animal-only ([`Improvement::valid_for_forage`] / [`Improvement::valid_for_hunt`]). Rejected
///    outright rather than silently coerced.
/// 2. **Gates — one knowledge per rung-transition** (§4.3). Each verb resolves its gate off its OWN
///    rung record, never a hard-coded id: `Cultivate` needs **Cultivation** + a **Thriving** patch
///    (not already tended, not someone else's); `Sow` needs **Seed Selection** + ground the
///    `plant:field` rung's `site_requirement` accepts; `Tame` needs **Herding**; and `Corral` needs
///    **Penning** — *not* Herding, which gates `tame` alone — plus an owned, **domesticated**,
///    not-yet-penned herd of a `pen`-ceiling species. Each is unchanged by the axis split.
fn validate_improvement(
    app: &bevy::prelude::App,
    faction: FactionId,
    target: &LaborTarget,
    improvement: Improvement,
) -> Result<(), String> {
    match target {
        LaborTarget::Forage { tile, species, .. } => {
            if !improvement.valid_for_forage() {
                return Err(format!(
                    "'{}' is not something you build on the land — it applies to herds.",
                    improvement.as_str()
                ));
            }
            match improvement {
                Improvement::Sow => validate_sow(app, faction, *tile, species.as_deref()),
                _ => validate_cultivate(app, faction, *tile, species.as_deref()),
            }
        }
        LaborTarget::Hunt { fauna_id, .. } => {
            if !improvement.valid_for_hunt() {
                return Err(format!(
                    "'{}' is not something you build on a herd — it applies to forage patches.",
                    improvement.as_str()
                ));
            }
            match improvement {
                Improvement::Tame => validate_tame(app, faction, fauna_id),
                _ => validate_corral(app, faction, fauna_id),
            }
        }
        LaborTarget::Scout | LaborTarget::Warrior => Err(format!(
            "There is nothing to {} on a standing role.",
            improvement.as_str()
        )),
    }
}

/// **Does the land admit `rung` here?** — the ONE place the server resolves a plant rung's
/// `site_requirement`, so the gather gate, `cultivate` and `sow` cannot drift into disagreeing about
/// which ground may be worked. It gathers the three readings [`rung_site_refusal`] judges (is this a
/// gathering site, what is the tile's own forage capacity, is it fresh-watered) and hands them to
/// that one seam — the same seam the labor arm's placement gate and the wire's `sowSiteRefusal` use.
///
/// `None` on a tile that is off the map: the caller's own "there is no tile there" error is the
/// better message, and inventing a site refusal for a coordinate that does not exist would hide it.
///
/// **A world with no `TileRegistry` answers `None`, and that must stay a `get_resource`** — the idle
/// boot and the command-unit harnesses run an `App` that has never generated a map, and
/// `Command::AssignLabor` is dispatched with no `world_active` gate, so a `assign_labor … forage`
/// arriving before `new_game` reaches this function. Panicking here unwinds out of the command loop
/// and kills the server. The permissive answer is the same one [`validate_species_selection`] gives
/// for exactly this case: with no tiles there is no ground to judge, and the labor arm — which
/// always has the real tiles — remains the authority.
fn plant_rung_site_refusal(
    app: &bevy::prelude::App,
    rung_key: RungKey,
    tile: UVec2,
) -> Option<SiteRefusal> {
    let registry = app.world.get_resource::<TileRegistry>()?;
    let tile_entity = registry.index(tile.x, tile.y)?;
    let ground = app.world.get::<Tile>(tile_entity)?;
    let (grid_width, grid_height) = (registry.width, registry.height);
    let wrap_horizontal = app
        .world
        .resource::<SimulationConfig>()
        .map_topology
        .wrap_horizontal;
    let fresh_water =
        tile_is_fresh_watered(ground, grid_width, grid_height, wrap_horizontal, |coord| {
            registry
                .index(coord.x, coord.y)
                .and_then(|entity| app.world.get::<Tile>(entity))
                .map(|neighbor| neighbor.terrain_tags)
        });
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    rung_site_refusal(
        ladder.rung(rung_key),
        ground,
        &labor.forage,
        app.world.resource::<FoodSiteRegistry>().is_site(tile),
        fresh_water,
    )
}

/// **The land's refusal, phrased for the player** — one wording per fault, shared by every plant
/// gate, so the same ground never gets two different explanations depending on which verb asked.
///
/// Each names the thing the player can actually *do* about it. `NotGatheringSite` is the one that is
/// not a "yet": no rung below Farm relaxes it, so the answer is to work ground your people already
/// gather rather than to wait.
fn site_refusal_message(refusal: SiteRefusal, tile: UVec2, verb: &str) -> String {
    match refusal {
        SiteRefusal::NotGatheringSite => format!(
            "Nobody gathers at ({}, {}) — your people cannot {} ground they do not already work. \
             Choose a gathering site, or move to one.",
            tile.x, tile.y, verb
        ),
        SiteRefusal::TooPoor => format!(
            "Nothing will grow at ({}, {}) — that ground is too thin to take a crop. Your people \
             cannot yet feed the land.",
            tile.x, tile.y
        ),
        SiteRefusal::TooDry => format!(
            "Nothing will grow at ({}, {}) — that ground is too dry to take a crop. Your people can \
             carry seed, but not yet water: sow the well-watered ground along the rivers.",
            tile.x, tile.y
        ),
        SiteRefusal::TooPoorAndTooDry => format!(
            "Nothing will grow at ({}, {}) — that ground is too thin and too dry to take a crop. \
             Your people can carry seed, but not yet water or feed the land.",
            tile.x, tile.y
        ),
    }
}

/// The **`Cultivate`** verb's gates — the plant rung-2 twin of [`validate_tame`], split out of the
/// old inline chain when the improvement became its own axis.
fn validate_cultivate(
    app: &bevy::prelude::App,
    faction: FactionId,
    tile: UVec2,
    species: Option<&str>,
) -> Result<(), String> {
    // The rung's own gate, resolved off the ladder — the `validate_tame` pattern: the record
    // says which knowledge opens `cultivate`, and the ladder says when a knowledge is known.
    let (knowledge_threshold, tended_unlock) = {
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        (
            ladder.knowledge.completion_threshold,
            ladder.rung(RungKey::PlantTended).unlock_discovery_id(),
        )
    };
    let knows_cultivation = tended_unlock.is_none_or(|knowledge| {
        knows(
            app.world.resource::<DiscoveryProgressLedger>(),
            faction,
            knowledge,
            knowledge_threshold,
        )
    });
    if !knows_cultivation {
        return Err("Your people have not learned Cultivation yet. Sustain-forage thriving patches to learn it.".to_string());
    }
    // **Rung 2 asks exactly what rung 1 asks** — Cultivate improves the output of ground you already
    // gather, so it can only stand on a gathering site. Resolved through the same seam rather than
    // leaning on "the Forage assignment must already have passed", because an improvement can be
    // named on the same command that first staffs the tile.
    if let Some(refusal) = plant_rung_site_refusal(app, RungKey::PlantTended, tile) {
        return Err(site_refusal_message(refusal, tile, "tend"));
    }
    let Some(patch) = app.world.resource::<ForageRegistry>().patch(tile) else {
        return Err(format!("No forage patch at ({}, {}).", tile.x, tile.y));
    };
    if patch.is_cultivated() {
        return Err(format!(
            "The patch at ({}, {}) is already cultivated — forage it to tend it.",
            tile.x, tile.y
        ));
    }
    // **There is no health gate here** (`docs/plan_harvest_floor.md` §3.2). `Cultivate` used to
    // demand `EcologyPhase::Thriving`, as a **start** gate with an exemption for a build already
    // underway (`ForagePatch::cultivation_underway`) — a whole start-vs-continue ruling that existed
    // to make the mid-build lapse survivable. The harvest floor replaced the cliff with a rate: a
    // crew pulling hard on the ground they are clearing builds *slowly*
    // (`intensification::learn_multiplier`), never *not at all*. With nothing left to lapse, the
    // exemption has nothing to exempt and the gate has nothing to gate.
    if patch.owner.is_some_and(|owner| owner != faction) {
        return Err(format!(
            "Another people are cultivating the patch at ({}, {}).",
            tile.x, tile.y
        ));
    }
    // **Which plant would this commit the ground to?** (Flora Roster S1.) The last gate, because it
    // is the most specific: the land and the knowledge decide whether the verb is available at all,
    // and this decides whether the *selection* is one this ground can grow.
    validate_species_selection(app, tile, species, RungKey::PlantTended)
}

/// The **`Corral`** verb's gates — the animal rung-3 twin of [`validate_sow`].
///
/// **The §4.3 gate reshuffle**: rung 3 is gated on **Penning**, the knowledge rung 2 teaches — not on
/// Herding, which now gates `tame` alone. Read off the rung record (the [`validate_tame`] pattern)
/// rather than a hard-coded id, so the gate cannot drift from the ladder the labor arm accrues
/// against.
fn validate_corral(
    app: &bevy::prelude::App,
    faction: FactionId,
    fauna_id: &str,
) -> Result<(), String> {
    let (knowledge_threshold, pen_unlock) = {
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        (
            ladder.knowledge.completion_threshold,
            ladder.rung(RungKey::AnimalPen).unlock_discovery_id(),
        )
    };
    let knows_penning = pen_unlock.is_none_or(|knowledge| {
        knows(
            app.world.resource::<DiscoveryProgressLedger>(),
            faction,
            knowledge,
            knowledge_threshold,
        )
    });
    if !knows_penning {
        return Err(
            "Your people have not learned Penning yet. Tame and keep herds to learn it."
                .to_string(),
        );
    }
    let Some(herd) = app.world.resource::<HerdRegistry>().find(fauna_id) else {
        return Err(format!("No herd '{}' to corral.", fauna_id));
    };
    // Grazing 2d-δ: only a `Pen`-ceiling species may be penned (nomadic herders don't fence).
    if !herd.can_pen() {
        return Err(format!("{} cannot be penned.", herd.species));
    }
    if herd.is_corralled() {
        return Err(format!("{} is already corralled.", fauna_id));
    }
    if !herd.is_domesticated() {
        return Err(format!(
            "{} is not domesticated. Tame it before building a pen.",
            fauna_id
        ));
    }
    if herd.owner != Some(faction) {
        return Err(format!("You do not own {}.", fauna_id));
    }
    Ok(())
}

/// The **`Sow`** policy's gates — the plant **rung-3** verb (`docs/plan_intensification_ladder.md`
/// §2), split out for `validate_tame`'s reason: the Forage arm now validates two investment rungs.
///
/// Each rejection is distinct, and the order is deliberate:
/// 1. **The tile exists.** A coordinate off the map names no ground at all.
/// 2. **The LAND will take seed** — the rung's own `site_requirement` (`RungSiteRequirement`), read
///    off the ladder record and judged by the *rung*, not restated here: the ground must be a
///    **gathering site** *and* **near fresh water**. Rung 3 knows how to move seed but not how to
///    carry water, and it does not yet work unfamiliar ground — so it sows the watered ground its
///    people already gather. The failures are **distinct** and phrased distinctly through the shared
///    [`site_refusal_message`]. Checked *before* knowledge, because it is a property of the *place*,
///    not of the player (the `validate_tame` rule: the animal's own nature outranks who is hunting it).
/// 3. **Seed Selection** — the rung's own `unlock_knowledge`, read off the ladder rather than
///    hard-coded, naming both the knowledge and how it is learned.
/// 4. **Not already a Field** — this rung is already climbed; work it, don't re-sow it.
/// 5. **Not another faction's ground** — mirrors the Cultivate arm's "another people are cultivating
///    it".
///
/// **There is deliberately no health gate**: freshly sown ground starts at the reseed floor, i.e.
/// Collapsing, so requiring Thriving would forbid exactly the case this rung exists for.
///
/// **`Sow` USED TO NEED NO SITE AT ALL, AND THAT WAS THE REVERSAL THIS ARC MADE.** §2 read "where the
/// two webs legitimately differ: `Corral` needs a herd you already tamed, `Sow` needs nothing" —
/// seed travels, so any qualifying ground was a legal, indeed the interesting, target. What that
/// missed is that a player could never *reach* such ground: gathering itself is site-bound, so the
/// only tiles a band works are gathering sites, and a rung that could leap off them existed on paper
/// only. Moving "seed travels" up to **rung 4 (Farm)** is what gives that rung its identity — it is
/// the first rung to drop `requires_gathering_site` — and leaves rung 3 as what it always played as:
/// *commit one of the plants you already gather here to a single crop*.
fn validate_sow(
    app: &bevy::prelude::App,
    faction: FactionId,
    tile: UVec2,
    species: Option<&str>,
) -> Result<(), String> {
    // `get_resource`, not `resource`: a world that has never been generated carries no
    // `TileRegistry`, and this arm is reachable from an ungated `assign_labor` before `new_game`.
    // A map-less world has no tile to name as missing either, so it falls through to the same
    // permissive stance `plant_rung_site_refusal` and `validate_species_selection` take.
    if let Some(registry) = app.world.get_resource::<TileRegistry>() {
        if registry.index(tile.x, tile.y).is_none() {
            return Err(format!("There is no tile at ({}, {}).", tile.x, tile.y));
        }
    }
    let (knowledge_threshold, field_unlock) = {
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        (
            ladder.knowledge.completion_threshold,
            ladder.rung(RungKey::PlantField).unlock_discovery_id(),
        )
    };
    // The land's own answer, phrased through the shared message table so a tile the gather gate
    // already refused does not get a second, differently-worded "no" from this verb.
    if let Some(refusal) = plant_rung_site_refusal(app, RungKey::PlantField, tile) {
        return Err(site_refusal_message(refusal, tile, "sow"));
    }
    let knows_seed_selection = field_unlock.is_none_or(|knowledge| {
        knows(
            app.world.resource::<DiscoveryProgressLedger>(),
            faction,
            knowledge,
            knowledge_threshold,
        )
    });
    if !knows_seed_selection {
        return Err(
            "Your people have not learned Seed Selection yet. Work tended patches to learn \
                    it."
            .to_string(),
        );
    }
    // A tile with no patch at all is a LEGAL target — the create-from-nothing case. Only an existing
    // patch can be in a state that refuses the seed.
    if let Some(patch) = app.world.resource::<ForageRegistry>().patch(tile) {
        if patch.is_field() {
            return Err(format!(
                "The field at ({}, {}) is already sown — forage it to work it.",
                tile.x, tile.y
            ));
        }
        if patch.owner.is_some_and(|owner| owner != faction) {
            return Err(format!(
                "Another people are working the ground at ({}, {}).",
                tile.x, tile.y
            ));
        }
    }
    // **Which crop?** — the species half of "will this ground take seed", after the land half above.
    validate_species_selection(app, tile, species, RungKey::PlantField)
}

/// **May a `Cultivate`/`Sow` on this tile commit to this plant?** — the species-side gate
/// (`docs/plan_flora_roster.md` §4.3), phrased for the player.
///
/// It resolves through the *same* `forage::resolve_committed_species` seam the labor arm commits
/// with, so a selection this accepts can never be one the turn then refuses (the `rung_site_refusal`
/// discipline). The composition comes from `forage::tile_flora_composition` — the one seam — so a
/// navigable hex is judged on the two-term basket it actually has.
fn validate_species_selection(
    app: &bevy::prelude::App,
    tile: UVec2,
    species: Option<&str>,
    rung: RungKey,
) -> Result<(), String> {
    // **No map, nothing to judge.** A world that has not been generated yet (the idle boot, and the
    // command-unit harnesses) carries no `TileRegistry` at all, so there is no basket to read; the
    // labor arm, which always has the real tiles, remains the authority and simply accrues nothing
    // if the ground grows nothing that climbs.
    let Some(registry) = app.world.get_resource::<TileRegistry>() else {
        return Ok(());
    };
    let Some(ground) = registry
        .index(tile.x, tile.y)
        .and_then(|entity| app.world.get::<Tile>(entity))
    else {
        return Err(format!("There is no tile at ({}, {}).", tile.x, tile.y));
    };
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let flora = app.world.resource::<FloraConfigHandle>().get();
    let map_seed = app.world.resource::<SimulationConfig>().map_seed;
    let composition = tile_flora_composition(&flora, &labor.forage, ground, map_seed);
    let verb = match rung {
        RungKey::PlantField => "sown",
        _ => "tended",
    };
    match resolve_committed_species(species, &composition, &flora, rung) {
        Ok(_) => Ok(()),
        Err(SpeciesRefusal::Unknown) => Err(format!(
            "Your people know no plant called '{}'.",
            species.unwrap_or_default()
        )),
        Err(SpeciesRefusal::CeilingTooLow) => Err(format!(
            "{} cannot be {verb} — it is a wild harvest, gathered where it grows.",
            flora.species.get(species.unwrap_or_default()).map_or_else(
                || species.unwrap_or_default().to_string(),
                |def| def.display_name.clone()
            )
        )),
        Err(SpeciesRefusal::NotHere) => Err(format!(
            "{} does not grow at ({}, {}).",
            flora.species.get(species.unwrap_or_default()).map_or_else(
                || species.unwrap_or_default().to_string(),
                |def| def.display_name.clone()
            ),
            tile.x,
            tile.y
        )),
        Err(SpeciesRefusal::NothingClimbsHere) => Err(format!(
            "Nothing that grows at ({}, {}) can be {verb} — what the ground offers there is a wild \
             harvest.",
            tile.x, tile.y
        )),
    }
}

/// The **`Tame`** policy's gates — the animal rung-2 twin of the `Cultivate` arm above, in the same
/// order and with the same shape. Split out because the Hunt arm now validates two investment rungs
/// and one inline `if` chain for both would read as a maze.
///
/// Each rejection is distinct, and the order is deliberate:
/// 1. **Herding** — the rung's own `unlock_knowledge`, read off the ladder rather than hard-coded, so
///    a config edit to the gate can't leave a stale check here. (§4.3 will move rung 3 to `penning`;
///    this arm keeps naming its own rung's knowledge whatever that becomes.)
/// 2. **The herd exists.**
/// 3. **The species' `husbandry_ceiling` allows domestication** (Grazing 2d-δ) — checked *before*
///    ownership, because it is a property of the *animal*, not of who is hunting it (the rule the
///    retired `domesticate` handler established).
/// 4. **Not already domesticated** — this rung is already climbed; `corral` is the next verb.
/// 5. **Not another faction's** — mirrors the plant side's "another people are cultivating it".
///
/// Deliberately **not** gated on the herd being Thriving, unlike the patch: a herd's phase swings as
/// it is hunted, and the labor arm already handles a lapsed phase gracefully (accrual pauses, the
/// meter holds, work resumes on recovery). Rejecting the *policy* for a transient dip would be a
/// worse experience than letting the player commit and wait.
fn validate_tame(
    app: &bevy::prelude::App,
    faction: FactionId,
    fauna_id: &str,
) -> Result<(), String> {
    let (knowledge_threshold, pastoral_unlock) = {
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        (
            ladder.knowledge.completion_threshold,
            ladder.rung(RungKey::AnimalPastoral).unlock_discovery_id(),
        )
    };
    let knows_unlock = pastoral_unlock.is_none_or(|knowledge| {
        knows(
            app.world.resource::<DiscoveryProgressLedger>(),
            faction,
            knowledge,
            knowledge_threshold,
        )
    });
    if !knows_unlock {
        return Err(
            "Your people have not learned Herding yet. Sustain-hunt thriving herds to learn it."
                .to_string(),
        );
    }
    let Some(herd) = app.world.resource::<HerdRegistry>().find(fauna_id) else {
        return Err(format!("No herd '{}' to tame.", fauna_id));
    };
    // Grazing 2d-δ: a `Wild`-ceiling species can never be tamed — a property of the animal.
    if !herd.can_domesticate() {
        return Err(format!(
            "{} is wild game — hunt-only, it cannot be tamed.",
            herd.species
        ));
    }
    if herd.is_domesticated() {
        return Err(format!(
            "{} is already domesticated — corral it to pen it.",
            fauna_id
        ));
    }
    if herd.owner.is_some_and(|owner| owner != faction) {
        return Err(format!("Another people are taming {}.", fauna_id));
    }
    Ok(())
}

/// Set the worker count for one labor target on a band (idempotent; `0` unassigns; clamps to the
/// band's free working-age headroom). Text forms:
///   `assign_labor <faction> <band> forage <x> <y> [policy] <workers>`
///   `assign_labor <faction> <band> hunt <herd_id> [policy] <workers>`
///   `assign_labor <faction> <band> scout <workers>`
///   `assign_labor <faction> <band> warrior <workers>`
///
/// `policy` is one of the **four harvest stances** (`sustain`/`surplus`/`deplete`/`eradicate`). It
/// no longer accepts a build verb: an improvement is set by its own command
/// (`cultivate`/`sow`/`tame`/`corral`) and **is never touched by this one**
/// (`docs/plan_investment_rung_toggle.md` §5), which is what makes a paused build's crew editable.
#[allow(clippy::too_many_arguments)]
/// **The floor a `LaborTarget` built to NAME A SOURCE carries** — the improvement commands
/// (`cultivate`/`sow`/`tame`/`corral`), the abandon path and the pen-keeper lookup all construct a
/// target to identify a tile or a herd, never to state an assignment. [`LaborTarget::same_source`]
/// keys on the tile/herd id alone, so the floor here is matched by nothing and read by nothing.
///
/// It is [`DEFAULT_ESCAPEMENT_FLOOR`] rather than an arbitrary number so that a future reader who
/// *does* look at it sees the sustainable value, not a strip order.
const SOURCE_NAMED_NOT_ASSIGNED: f32 = DEFAULT_ESCAPEMENT_FLOOR;

/// The feed channel a labor command reports on, resolved from the **role token** rather than from a
/// built `LaborTarget` — the floor is validated before a target exists, and a rejection has to land
/// on the channel the player was looking at.
fn labor_event_kind(role: &str) -> CommandEventKind {
    match role.to_ascii_lowercase().as_str() {
        "forage" => CommandEventKind::Forage,
        "hunt" => CommandEventKind::Hunt,
        "scout" => CommandEventKind::Scout,
        _ => CommandEventKind::CancelOrder,
    }
}

// One labor command's worth of context: the band, the role, the crew, the source's coordinates or
// herd id, and the assignment's two mutable properties (the crop selection and the floor). Bundling
// them would just move the noise.
#[allow(clippy::too_many_arguments)]
fn handle_assign_labor(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_id: Option<u64>,
    role: String,
    workers: u32,
    target_x: Option<u32>,
    target_y: Option<u32>,
    fauna_id: Option<String>,
    species: Option<String>,
    floor: Option<f32>,
) {
    // **The floor FAILS CLOSED** (`docs/plan_harvest_floor.md` §4): absent means the default, but a
    // value outside `0.0..=1.0` is rejected with its own failure event rather than clamped. A clamp
    // would turn a typo into a quiet policy change on the one number the whole harvest model turns
    // on — the `cancel_order` scope precedent.
    let floor = match floor {
        None => DEFAULT_ESCAPEMENT_FLOOR,
        Some(value) if floor_is_valid(value) => value,
        Some(value) => {
            emit_command_failure(
                app,
                labor_event_kind(&role),
                faction,
                format!(
                    "assign_labor floor must be a fraction of carrying capacity in 0.0..=1.0; got {value}."
                ),
            );
            return;
        }
    };
    let target = match role.to_ascii_lowercase().as_str() {
        "forage" => match (target_x, target_y) {
            (Some(x), Some(y)) => LaborTarget::Forage {
                tile: UVec2::new(x, y),
                floor,
                // The optional species selection (Flora Roster S1): which named plant a
                // `Cultivate`/`Sow` here should commit the patch to. Absent/blank = "pick the tile's
                // dominant legal plant for me", the same absent-means-default convention the floor has.
                species: species
                    .as_deref()
                    .map(str::trim)
                    .filter(|key| !key.is_empty())
                    .map(str::to_string),
            },
            _ => {
                emit_command_failure(
                    app,
                    CommandEventKind::Forage,
                    faction,
                    "assign_labor forage requires <x> <y>.".to_string(),
                );
                return;
            }
        },
        "hunt" => match fauna_id {
            Some(id) if !id.trim().is_empty() => LaborTarget::Hunt {
                fauna_id: id,
                floor,
            },
            _ => {
                emit_command_failure(
                    app,
                    CommandEventKind::Hunt,
                    faction,
                    "assign_labor hunt requires <herd_id>.".to_string(),
                );
                return;
            }
        },
        "scout" => LaborTarget::Scout,
        "warrior" => LaborTarget::Warrior,
        other => {
            emit_command_failure(
                app,
                CommandEventKind::CancelOrder,
                faction,
                format!("Unknown labor role '{}'.", other),
            );
            return;
        }
    };

    let event_kind = match &target {
        LaborTarget::Forage { .. } => CommandEventKind::Forage,
        LaborTarget::Hunt { .. } => CommandEventKind::Hunt,
        LaborTarget::Scout => CommandEventKind::Scout,
        LaborTarget::Warrior => CommandEventKind::CancelOrder,
    };

    // Stance validation. Unassigning (`workers == 0`) is always allowed — a player must be able to
    // abandon an investment even if its gates have since lapsed. **The improvement's gates are NOT
    // re-run here** (issue #442): this command does not set an improvement, so re-asserting one the
    // band already carries would refuse a crew change on a paused build — the trap §6 removes.
    if workers > 0 {
        if let Err(reason) = validate_labor_policy(app, faction, &target) {
            emit_command_failure(app, event_kind, faction, reason);
            return;
        }
    }

    let Some(band) = select_starting_band(app, faction, band_id, "assign_labor", event_kind) else {
        return;
    };

    let available = app
        .world
        .get::<PopulationCohort>(band.entity)
        .map(|cohort| available_workers(cohort.working))
        .unwrap_or(0);

    let kind_label = target.kind();
    let (applied, assigned_total, improvement) = {
        let mut allocation = band_allocation_mut(app, band.entity);
        let applied = allocation.set_assignment(target.clone(), workers, available);
        // `set_assignment` carries any running improvement across, so the seed must price the dip
        // that is still in flight rather than the undipped stance.
        let improvement = allocation
            .assignments
            .iter()
            .find(|assignment| assignment.target.same_source(&target))
            .and_then(|assignment| assignment.improvement);
        (applied, allocation.assigned_total(), improvement)
    };
    // Show the source's expected yield immediately (workers added/removed OR stance changed — every
    // shape of this command that moves the number): without the seed the row reads `+0.00` until the
    // player advances a turn.
    seed_source_yield(app, band.entity, &target, improvement, applied);

    let tick = app.world.resource::<SimulationTick>().0;
    let clamp_note = if applied < workers {
        format!(" (clamped from {} — only {} idle)", workers, available)
    } else {
        String::new()
    };
    push_command_event(
        app,
        tick,
        event_kind,
        faction,
        format!("{} {} x{}{}", band.label, kind_label, applied, clamp_note),
        Some(format!(
            "status=applied role={} workers={} assigned_total={} available={}",
            kind_label, applied, assigned_total, available
        )),
    );
}

/// Order a band to travel toward a target tile at `band_move_tiles_per_turn`/turn (Early-Game
/// Labor). In-range sources update as the band moves; a Forage assignment the move carries out of
/// `band_work_range` is abandoned that same turn (workers back to the pool, feed entry naming the
/// tile). Text form: `move_band <faction> <band> <x> <y>`.
fn handle_move_band(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_id: Option<u64>,
    target_x: u32,
    target_y: u32,
) {
    let target = UVec2::new(target_x, target_y);
    if ensure_land_tile(
        app,
        faction,
        target,
        "move_band",
        Some(CommandEventKind::CancelOrder),
    )
    .is_none()
    {
        return;
    }
    let Some(band) = select_starting_band(
        app,
        faction,
        band_id,
        "move_band",
        CommandEventKind::CancelOrder,
    ) else {
        return;
    };
    app.world
        .entity_mut(band.entity)
        .insert(BandTravel { target });

    // If the moved entity is an expedition, a fresh `move_band` un-latches AwaitingOrders (or
    // redirects a Returning party back out to explore): re-arm it Outbound and re-open the
    // arrival announcement so reaching the new waypoint fires the feed line again.
    if let Some(mut expedition) = app.world.get_mut::<Expedition>(band.entity) {
        expedition.phase = ExpeditionPhase::Outbound;
        expedition.announced = false;
    }

    let tick = app.world.resource::<SimulationTick>().0;
    push_command_event(
        app,
        tick,
        CommandEventKind::CancelOrder,
        faction,
        format!("{} moving -> ({}, {})", band.label, target_x, target_y),
        Some(format!(
            "status=queued action=move_band band={}",
            band.label
        )),
    );
}

/// Outfit and launch a scouting expedition: draw `party_workers` off the resolved home band's
/// working pool and larder-drawn provisions, then spawn a detached `StartingUnit` band tagged
/// `Expedition` (deliberately no `ResidentBand`) traveling toward the target. v1 is deterministic
/// success. Text form: `send_expedition <faction> <band> <party_workers> <x> <y>`.
fn handle_send_expedition(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_id: Option<u64>,
    party_workers: u32,
    target_x: u32,
    target_y: u32,
) {
    let target = UVec2::new(target_x, target_y);
    if ensure_land_tile(
        app,
        faction,
        target,
        "send_expedition",
        Some(CommandEventKind::ExpeditionSent),
    )
    .is_none()
    {
        return;
    }
    let Some(band) = select_starting_band(
        app,
        faction,
        band_id,
        "send_expedition",
        CommandEventKind::ExpeditionSent,
    ) else {
        return;
    };
    // `select_starting_band` only filters `With<ResidentBand>` on the None-bits fallback; an
    // explicit `band_id` resolves on `StartingUnit` alone, which an expedition also carries
    // (kept so `move_band` can retarget it). A party can only be outfitted *from* a resident band —
    // reject anything else so `send_expedition` can't spawn a party off another expedition.
    if app.world.get::<ResidentBand>(band.entity).is_none() {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            "send_expedition: band is not a resident band.",
        );
        return;
    }

    let grid_width = app.world.resource::<TileRegistry>().width;
    let wrap_horizontal = app
        .world
        .resource::<SimulationConfig>()
        .map_topology
        .wrap_horizontal;
    let cfg = app.world.resource::<ExpeditionConfigHandle>().get();

    // Snapshot the home band: its position, worker pool, and a clone we retask into the party.
    let Some(band_cohort) = app.world.get::<PopulationCohort>(band.entity) else {
        return;
    };
    let current_tile = band_cohort.current_tile;
    let band_working = band_cohort.working;
    let mut expedition_cohort = band_cohort.clone();
    let Some(band_tile) = app.world.get::<Tile>(current_tile) else {
        return;
    };
    let band_pos = band_tile.position;
    let (unit_kind, unit_tags) = app
        .world
        .get::<StartingUnit>(band.entity)
        .map(|unit| (unit.kind.clone(), unit.tags.clone()))
        .unwrap_or_else(|| ("expedition".to_string(), Vec::new()));

    let distance = hex_distance_wrapped(band_pos, target, grid_width, wrap_horizontal);
    let available = available_workers(band_working);
    let max_party = available.min(cfg.max_party_size);
    if party_workers < 1 || party_workers > max_party {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            format!(
                "Party of {} workers invalid — {} can outfit 1..{} workers.",
                party_workers, band.label, max_party
            ),
        );
        return;
    }

    // Draw provisions (partial OK — non-fatal in v1) and remove the party from the band's pool.
    let requested = scalar_from_f32(
        party_workers as f32 * distance as f32 * cfg.provision_draw_per_worker_per_tile,
    );
    let party_scalar = Scalar::from_u32(party_workers);
    let drawn = {
        // The `get`-guard above already confirmed the component; a synchronous handler can't
        // despawn it mid-call, so this re-fetch is unreachable-None. Match the sibling guards'
        // let-else style (no `expect` on a server path) and early-return if it somehow fails.
        let Some(mut band_cohort) = app.world.get_mut::<PopulationCohort>(band.entity) else {
            return;
        };
        let drawn = band_cohort.stores.take(FOOD, requested);
        band_cohort.working -= party_scalar;
        band_cohort.sync_size();
        drawn
    };

    // Retask the cloned cohort into a detached party co-located with the band.
    expedition_cohort.children = Scalar::from_i64(0);
    expedition_cohort.working = party_scalar;
    expedition_cohort.elders = Scalar::from_i64(0);
    expedition_cohort.stores = LocalStore::new();
    if drawn > Scalar::from_i64(0) {
        expedition_cohort.stores.add(FOOD, drawn);
    }
    expedition_cohort.age_turns = 0;
    expedition_cohort.migration = None;
    expedition_cohort.grievance = Scalar::from_i64(0);
    expedition_cohort.sync_size();

    // A detached party is a band in its own right, so it takes its own durable id.
    let expedition_band_id = app.world.resource_mut::<BandIdAllocator>().allocate();
    let expedition_entity = app
        .world
        .spawn((
            expedition_cohort,
            expedition_band_id,
            LaborAllocation::default(),
            StartingUnit::new(unit_kind, unit_tags),
            Expedition {
                home_band: band.entity,
                mission: ExpeditionMission::Scout,
                phase: ExpeditionPhase::Outbound,
                announced: false,
                pending_reveal: Vec::new(),
                // An outfitted party leaves with an empty trade pack — it earns its pelts in the
                // field (`advance_expeditions`).
                carried_trade: 0.0,
            },
            BandTravel { target },
        ))
        .id();

    let tick = app.world.resource::<SimulationTick>().0;
    push_command_event(
        app,
        tick,
        CommandEventKind::ExpeditionSent,
        faction,
        format!("{} expedition -> ({}, {})", band.label, target_x, target_y),
        Some(format!(
            "status=applied workers={} provisions_drawn={} distance={} expedition={}",
            party_workers,
            drawn.to_i64_whole(),
            distance,
            expedition_entity.to_bits()
        )),
    );
}

/// Outfit and launch a hunting expedition (PR 2): draw `party_workers` off the resolved home band
/// and send a detached party to follow the herd `fauna_id` under `policy` (Sustain when omitted).
/// Unlike the scouting expedition it draws **no** provisions (it lives off its kills) and starts in
/// the `Hunting` phase heading for the herd's live tile. Text form:
/// `send_hunt_expedition <faction> <band> <party_workers> <fauna_id> [policy]`.
fn handle_send_hunt_expedition(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_id: Option<u64>,
    party_workers: u32,
    fauna_id: String,
    floor: Option<f32>,
) {
    // **The raid's floor FAILS CLOSED**, exactly as `assign_labor`'s does: absent means the default
    // (the food peak, the conservative reading), and a value outside `0.0..=1.0` is refused with its
    // own failure event rather than clamped. Where a party stops is the whole of what its orders say
    // about pressure, so a typo must not silently flip a herd's fate.
    let floor = match floor {
        None => DEFAULT_ESCAPEMENT_FLOOR,
        Some(value) if floor_is_valid(value) => value,
        Some(value) => {
            emit_command_failure(
                app,
                CommandEventKind::ExpeditionSent,
                faction,
                format!(
                    "send_hunt_expedition: floor must be a fraction of carrying capacity in \
                     0.0..=1.0; got {value}."
                ),
            );
            return;
        }
    };
    let Some(band) = select_starting_band(
        app,
        faction,
        band_id,
        "send_hunt_expedition",
        CommandEventKind::ExpeditionSent,
    ) else {
        return;
    };
    // Same resident-band gate as `send_expedition`: a party can only be outfitted from a real band.
    if app.world.get::<ResidentBand>(band.entity).is_none() {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            "send_hunt_expedition: band is not a resident band.",
        );
        return;
    }

    // The target must resolve to a live herd; capture its current tile as the initial travel target.
    let herd_pos = {
        let registry = app.world.resource::<HerdRegistry>();
        registry.find(&fauna_id).map(|herd| herd.position())
    };
    let Some(herd_pos) = herd_pos else {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            format!("send_hunt_expedition: no live herd '{}'.", fauna_id),
        );
        return;
    };

    let cfg = app.world.resource::<ExpeditionConfigHandle>().get();
    let Some(band_cohort) = app.world.get::<PopulationCohort>(band.entity) else {
        return;
    };
    let band_working = band_cohort.working;
    let mut expedition_cohort = band_cohort.clone();
    let (unit_kind, unit_tags) = app
        .world
        .get::<StartingUnit>(band.entity)
        .map(|unit| (unit.kind.clone(), unit.tags.clone()))
        .unwrap_or_else(|| ("expedition".to_string(), Vec::new()));

    let available = available_workers(band_working);
    let max_party = available.min(cfg.max_party_size);
    if party_workers < 1 || party_workers > max_party {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            format!(
                "Party of {} workers invalid — {} can outfit 1..{} workers.",
                party_workers, band.label, max_party
            ),
        );
        return;
    }

    // Launch-time viability forecast — a bounded forward SIMULATION of the trip (`hunt_trip_forecast`),
    // not a division. A Sustain party skims the herd's Maximum Sustainable Yield (a *flow*), and a
    // Surplus/Deplete party eats *stock* headroom and then falls back to the regrowth trickle once it
    // is gone, so filling a carry cap off a small herd can genuinely take dozens of turns. That is
    // ecologically true, not a bug; the player must be told at launch rather than silently trapped,
    // so the forecast rides the `ExpeditionSent` feed entry (it still launches either way).
    let forecast = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .find(&fauna_id)
            .map(|herd| hunt_trip_forecast(party_workers, herd, floor, &fauna, &labor, &cfg))
    };
    // Round-trip TRAVEL is part of the honest trip length — the party walks out to the herd and back.
    // `hunt_trip_forecast` counts only the HUNTING turns (once in reach), so add the walk here, where
    // the launching band's tile is known. (The per-herd `huntTripEstimates` snapshot table is
    // band-agnostic — one row serves every band — so the CLIENT adds this same travel to the pre-launch
    // readout from the SELECTED band's tile + the exported `bandMoveTilesPerTurn`.)
    let travel_turns: u32 = {
        let grid_width = app.world.resource::<TileRegistry>().width;
        let wrap_horizontal = app
            .world
            .resource::<SimulationConfig>()
            .map_topology
            .wrap_horizontal;
        let move_rate = app
            .world
            .resource::<LaborConfigHandle>()
            .get()
            .band_move_tiles_per_turn
            .max(1);
        app.world
            .get::<PopulationCohort>(band.entity)
            .map(|c| c.current_tile)
            .and_then(|t| app.world.get::<Tile>(t))
            .map(|tile| {
                let one_way =
                    hex_distance_wrapped(tile.position, herd_pos, grid_width, wrap_horizontal);
                (2 * one_way).div_ceil(move_rate)
            })
            .unwrap_or(0)
    };
    // The raid always completes in bounded turns (grab the surplus, come home), so the only genuine
    // non-viable case is "no surplus to take" — the herd is at/below the policy's floor and delivers
    // NO animals. Otherwise headline the payload the raid actually lands, including the round trip.
    let (viability_note, viability_detail) = match &forecast {
        // An INEDIBLE quarry brings no food home — say what it *does* bring, no food ETA. This arm
        // used to fire for a denial *mission* (Eradicate); since #337 the policy is pure intensity
        // and the species decides the product, so an Eradicate raid on a deer reports its windfall
        // like any other rung, and only a wolf lands here.
        Some(f) if !f.delivers_food => (
            if f.delivers_trade {
                " — no food from this quarry: the party brings back trade goods, not meat"
                    .to_string()
            } else {
                " — this quarry yields nothing: the party delivers neither food nor trade goods"
                    .to_string()
            },
            format!(
                " eta_turns=none viability=inedible delivers_trade={}",
                f.delivers_trade
            ),
        ),
        // The herd has no surplus above the policy's floor — the honest non-viable case. "Too lean"
        // now means the raid lands NO food at all (a small party on a big animal still delivers a
        // partial with waste, so the signal is `delivered_food == 0`, not "the party is too small").
        Some(f) if f.delivered_food <= 0.0 => (
            format!(
                " — the {} is too lean to raid: at its {} floor it has no surplus, the party would \
                 return empty",
                fauna_id,
                floor
            ),
            " eta_turns=none viability=no_surplus".to_string(),
        ),
        // A completed raid: headline the food landed, with the kill count + waste below. A pack too
        // small to seat a whole animal delivers a partial and wastes the rest, so food (not the animal
        // count) is the payload. `turns_to_fill == None` means it ran the whole horizon still delivering
        // (a slow breeder a big party can neither fill nor exhaust).
        Some(f) => {
            let animals = f.animals_taken;
            let food = f.delivered_food;
            let wasted = f.wasted_food;
            match f.turns_to_fill {
                Some(hunt_turns) => {
                    let total = hunt_turns + travel_turns;
                    (
                        format!(
                            " — est. ~{:.1} food ({} animals, {:.1} wasted) over ~{} turns ({} hunting \
                             + {} travel)",
                            food, animals, wasted, total, hunt_turns, travel_turns
                        ),
                        format!(
                            " eta_turns={} hunt_turns={} travel_turns={} animals={} food={:.2} \
                             wasted={:.2}",
                            total, hunt_turns, travel_turns, animals, food, wasted
                        ),
                    )
                }
                None => (
                    format!(
                        " — a long raid: ~{:.1} food ({} animals, {:.1} wasted) over {}+ hunting turns \
                         (+{} travel)",
                        food, animals, wasted, cfg.hunt.forecast_horizon_turns, travel_turns
                    ),
                    format!(
                        " eta_turns=none travel_turns={} animals={} food={:.2} wasted={:.2}",
                        travel_turns, animals, food, wasted
                    ),
                ),
            }
        }
        None => (String::new(), String::new()),
    };

    // Remove the party from the band's pool — but draw NO provisions (it lives off its kills).
    let party_scalar = Scalar::from_u32(party_workers);
    {
        let Some(mut band_cohort) = app.world.get_mut::<PopulationCohort>(band.entity) else {
            return;
        };
        band_cohort.working -= party_scalar;
        band_cohort.sync_size();
    }

    // Retask the cloned cohort into a detached party co-located with the band, empty larder.
    expedition_cohort.children = Scalar::from_i64(0);
    expedition_cohort.working = party_scalar;
    expedition_cohort.elders = Scalar::from_i64(0);
    expedition_cohort.stores = LocalStore::new();
    expedition_cohort.age_turns = 0;
    expedition_cohort.migration = None;
    expedition_cohort.grievance = Scalar::from_i64(0);
    expedition_cohort.sync_size();

    // A detached party is a band in its own right, so it takes its own durable id.
    let expedition_band_id = app.world.resource_mut::<BandIdAllocator>().allocate();
    let expedition_entity = app
        .world
        .spawn((
            expedition_cohort,
            expedition_band_id,
            LaborAllocation::default(),
            StartingUnit::new(unit_kind, unit_tags),
            Expedition {
                home_band: band.entity,
                mission: ExpeditionMission::Hunt {
                    fauna_id: fauna_id.clone(),
                    floor,
                },
                phase: ExpeditionPhase::Hunting,
                announced: false,
                pending_reveal: Vec::new(),
                carried_trade: 0.0,
            },
            BandTravel { target: herd_pos },
        ))
        .id();

    let tick = app.world.resource::<SimulationTick>().0;
    push_command_event(
        app,
        tick,
        CommandEventKind::ExpeditionSent,
        faction,
        format!(
            "{} hunting expedition (floor {:.2}·K) -> herd {}{}",
            band.label, floor, fauna_id, viability_note
        ),
        Some(format!(
            "status=applied mission=hunt floor={} workers={} herd={} expedition={}{}",
            floor,
            party_workers,
            fauna_id,
            expedition_entity.to_bits(),
            viability_detail
        )),
    );
}

/// Order an expedition home: set its phase to `Returning` (it chases the home band's live tile and
/// folds its workers + leftover provisions back on arrival). Text form:
/// `recall_expedition <faction> <expedition_band_id>`.
fn handle_recall_expedition(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    expedition_band_id: u64,
) {
    let Some(entity) = resolve_expedition_entity(
        app,
        faction,
        expedition_band_id,
        "recall_expedition",
        CommandEventKind::ExpeditionRecalled,
    ) else {
        return;
    };
    let label = starting_unit_label(app, entity);
    if let Some(mut expedition) = app.world.get_mut::<Expedition>(entity) {
        expedition.phase = ExpeditionPhase::Returning;
    }
    let tick = app.world.resource::<SimulationTick>().0;
    push_command_event(
        app,
        tick,
        CommandEventKind::ExpeditionRecalled,
        faction,
        format!("{} recalled — returning home", label),
        Some(format!("status=returning expedition={}", entity.to_bits())),
    );
}

/// Resolve a [`BandId`] to a faction's own [`Expedition`] (mirrors [`resolve_starting_unit_entity`]
/// but gates on the `Expedition` component + faction match rather than merely `StartingUnit`).
///
/// **A detached party is a band and carries a `BandId` like any other** — both expedition spawns
/// allocate one. This function resolved *entity bits* until the protocol cutover converted its
/// sibling and missed it, which made `recall_expedition` silently no-op: the client sent a small
/// counter value, `Entity::from_bits` read it as an index/generation pair, and the lookup either
/// found nothing or found something unrelated. Nothing failed loudly, because every rejection path
/// below is a `warn!` and a `return None`.
fn resolve_expedition_entity(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_id: u64,
    command_label: &str,
    event_kind: CommandEventKind,
) -> Option<Entity> {
    let entity_bits = band_id;
    let wanted = BandId(band_id);
    let found = {
        let mut query = app.world.query::<(Entity, &BandId)>();
        query
            .iter(&app.world)
            .find(|(_, id)| **id == wanted)
            .map(|(entity, _)| entity)
    };
    let Some(entity) = found else {
        tracing::error!(
            target: "shadow_scale::command",
            command = command_label,
            faction = %faction.0,
            band_id,
            "command.expedition.rejected=no_such_band"
        );
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!("Expedition {} does not exist in the simulation.", band_id),
        );
        return None;
    };
    if !app.world.entities().contains(entity) {
        warn!(
            target: "shadow_scale::command",
            command = command_label,
            faction = %faction.0,
            entity_bits,
            "command.expedition.rejected=entity_not_found"
        );
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!(
                "Expedition id {} does not exist in the simulation.",
                entity_bits
            ),
        );
        return None;
    }
    if app.world.get::<Expedition>(entity).is_none() {
        warn!(
            target: "shadow_scale::command",
            command = command_label,
            faction = %faction.0,
            entity_bits,
            "command.expedition.rejected=entity_not_expedition"
        );
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!("Unit id {} is not an expedition.", entity_bits),
        );
        return None;
    }
    let faction_ok = app
        .world
        .get::<PopulationCohort>(entity)
        .map(|cohort| cohort.faction == faction)
        .unwrap_or(false);
    if !faction_ok {
        warn!(
            target: "shadow_scale::command",
            command = command_label,
            faction = %faction.0,
            entity_bits,
            "command.expedition.rejected=wrong_faction"
        );
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!("Expedition id {} belongs to another faction.", entity_bits),
        );
        return None;
    }
    Some(entity)
}

/// Whether `scope` clears an assignment on `target`. [`CancelScope::All`] takes everything;
/// `Work` takes only the worked food sources and `Roles` only the band-wide standing roles, which
/// is what lets the Band panel's Work and Roles sections clear independently.
fn cancel_scope_clears(scope: CancelScope, target: &LaborTarget) -> bool {
    match scope {
        CancelScope::All => true,
        CancelScope::Work => matches!(
            target,
            LaborTarget::Forage { .. } | LaborTarget::Hunt { .. }
        ),
        CancelScope::Roles => matches!(target, LaborTarget::Scout | LaborTarget::Warrior),
    }
}

/// What the rejection says when the requested scope has nothing to clear — the band may be busy
/// with work the *other* scope owns, so the message has to name the scope rather than claim the
/// band is idle.
fn cancel_scope_nothing_to_clear(scope: CancelScope, band_label: &str) -> String {
    match scope {
        CancelScope::All => format!("{band_label} has no active order to cancel."),
        CancelScope::Work => format!("{band_label} has no worked sources to unassign."),
        CancelScope::Roles => format!("{band_label} has no standing roles to clear."),
    }
}

/// The feed line a successful cancel pushes. Only `All` stands the band down; the narrow scopes
/// leave the band working (and possibly travelling), so they must not claim otherwise.
fn cancel_scope_applied_message(scope: CancelScope, band_label: &str) -> String {
    match scope {
        CancelScope::All => format!("{band_label} stood down"),
        CancelScope::Work => format!("{band_label} unassigned its worked sources"),
        CancelScope::Roles => format!("{band_label} cleared its standing roles"),
    }
}

/// Clear the labor assignments `scope` names on a band — every one plus any in-progress move under
/// [`CancelScope::All`] (the band goes fully idle), the worked Forage/Hunt sources under `Work`, or
/// the Scout/Warrior roles under `Roles`. The narrow scopes deliberately leave [`BandTravel`] alone:
/// moving is not working. Rejects when *the requested scope* has nothing to clear, so a stray
/// invocation reports a failure rather than a misleading "stood down".
fn handle_cancel_order(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_id: Option<u64>,
    scope: CancelScope,
) {
    let Some(band) = select_starting_band(
        app,
        faction,
        band_id,
        "cancel_order",
        CommandEventKind::CancelOrder,
    ) else {
        return;
    };

    let clears_travel = matches!(scope, CancelScope::All);
    let has_task = {
        let entity = app.world.entity(band.entity);
        (clears_travel && entity.contains::<BandTravel>())
            || app
                .world
                .get::<LaborAllocation>(band.entity)
                .map(|allocation| {
                    allocation
                        .assignments
                        .iter()
                        .any(|assignment| cancel_scope_clears(scope, &assignment.target))
                })
                .unwrap_or(false)
    };
    if !has_task {
        emit_command_failure(
            app,
            CommandEventKind::CancelOrder,
            faction,
            cancel_scope_nothing_to_clear(scope, &band.label),
        );
        return;
    }

    {
        let mut entity = app.world.entity_mut(band.entity);
        if clears_travel {
            entity.remove::<BandTravel>();
        }
        if let Some(mut allocation) = entity.get_mut::<LaborAllocation>() {
            allocation.clear_kinds(|target| !cancel_scope_clears(scope, target));
        }
    }

    let tick = app.world.resource::<SimulationTick>().0;
    let detail = format!(
        "status=cancelled scope={} band={}",
        scope.as_str(),
        band.label
    );
    info!(
        target: "shadow_scale::command",
        command = "cancel_order",
        faction = %faction.0,
        band = %band.label,
        scope = %scope.as_str(),
        "command.cancel_order.applied"
    );
    push_command_event(
        app,
        tick,
        CommandEventKind::CancelOrder,
        faction,
        cancel_scope_applied_message(scope, &band.label),
        Some(detail),
    );
}

/// **Set the `Tame` policy** on the herd `herd_id` for the band(s) already hunting it — the animal
/// rung-2 verb, and the exact twin of `handle_cultivate`. This is the command form of what the
/// client's improvement checkbox does; it **tames nothing outright**.
///
/// It **replaces the retired `domesticate` early-claim**, which snapped `domestication_progress` to
/// `1.0` once past a `claim_threshold`. That claim existed to *skip the investment*, which is the
/// entire decision — the same reason the plant side removed its own claim first. Taming now costs a
/// real yield dip (the `animal:pastoral` rung's `yield_fraction_while_building × the herd's Sustain
/// (MSY) ceiling`) and takes `1 / progress_per_turn` turns of sustained work.
///
/// Targets a **herd id** (as `domesticate` did) rather than a tile: taming is the verb you reach for
/// on a *roaming* wild herd, which is identified by who is following it, not by where it stands this
/// turn. (`corral`, by contrast, keys off a tile — a pen is a place.)
///
/// Gates (via the shared `validate_labor_policy`): the faction must know **Herding**, the species'
/// `husbandry_ceiling` must allow domestication, and the herd must not already be domesticated or
/// another faction's — plus the rejection when **no band is hunting it**.
fn handle_tame(app: &mut bevy::prelude::App, faction: FactionId, herd_id: String) {
    // The source, named by the herd id. Its *stance* is irrelevant here: `same_source` matches on the
    // id alone, and this command sets only the improvement — whatever stance each band holds is left
    // exactly as the player set it (issue #442).
    let target = LaborTarget::Hunt {
        fauna_id: herd_id.clone(),
        floor: SOURCE_NAMED_NOT_ASSIGNED,
    };
    if let Err(reason) = validate_improvement(app, faction, &target, Improvement::Tame) {
        warn!(
            target: "shadow_scale::command",
            command = "tame",
            faction = %faction.0,
            herd = %herd_id,
            reason = %reason,
            "command.tame.rejected"
        );
        emit_command_failure(app, CommandEventKind::Tame, faction, reason);
        return;
    }

    let switched = set_improvement_on_working_bands(app, faction, &target, Some(Improvement::Tame));
    if switched == 0 {
        emit_command_failure(
            app,
            CommandEventKind::Tame,
            faction,
            format!(
                "No band is hunting {} — assign herders to it first, then tame it.",
                herd_id
            ),
        );
        return;
    }

    let tick = app.world.resource::<SimulationTick>().0;
    info!(
        target: "shadow_scale::command",
        command = "tame",
        faction = %faction.0,
        herd = %herd_id,
        bands = switched,
        "command.tame.taming"
    );
    push_command_event(
        app,
        tick,
        CommandEventKind::Tame,
        faction,
        format!("Taming {}", herd_id),
        Some(format!(
            "status=taming action=tame herd={} bands={}",
            herd_id, switched
        )),
    );
}

/// **Answer a pending narrative fork** (The Telling's fork tier).
///
/// The choice's writes land in the `BeatLedger` — declared stance offsets and consequence flags —
/// the beat is marked fired *now* (a fork is fired when answered, not when posted), the answer is
/// remembered, a deferring choice re-arms the beat, and the choice's echo joins the command feed
/// under `NarrativeFork` so the decision is part of the story record rather than a silent state
/// change.
///
/// **This is a pure ledger mutation — nothing here gates a turn.** The turn gate for an unanswered
/// fork is client-side; the server's counterpart is the expiry valve in `telling_tick`, which
/// auto-resolves a stale fork to its defer choice. Do not add a block to the turn queue or
/// `run_turn`: forks post for AI and unattended factions too.
fn handle_answer_fork(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    beat_id: String,
    choice_id: String,
) {
    let catalog = app.world.resource::<BeatCatalogHandle>().get();
    let default_register = app
        .world
        .resource::<BeatConfigHandle>()
        .get()
        .voice
        .default_register
        .clone();
    let tick = app.world.resource::<SimulationTick>().0;

    let outcome = {
        let mut ledger = app.world.resource_mut::<BeatLedger>();
        ledger.answer_fork(&catalog, faction, &beat_id, &choice_id, tick)
    };

    match outcome {
        Err(ForkAnswerError::UnknownBeat) => emit_command_failure(
            app,
            CommandEventKind::NarrativeFork,
            faction,
            format!("There is no beat '{beat_id}'."),
        ),
        Err(ForkAnswerError::NoPendingFork) => emit_command_failure(
            app,
            CommandEventKind::NarrativeFork,
            faction,
            format!("'{beat_id}' has no question waiting on you."),
        ),
        Err(ForkAnswerError::UnknownChoice) => emit_command_failure(
            app,
            CommandEventKind::NarrativeFork,
            faction,
            format!("'{beat_id}' offers no answer called '{choice_id}'."),
        ),
        Ok(resolution) => {
            info!(
                target: "shadow_scale::analytics",
                event = "telling_fork_answered",
                faction = faction.0,
                beat = %resolution.beat_id,
                choice = %resolution.choice_id,
                wardrobe = %resolution.wardrobe_id,
            );
            push_command_event(
                app,
                tick,
                CommandEventKind::NarrativeFork,
                faction,
                resolution.echo_line(&default_register),
                Some(format!("{} resolved=answered", resolution.detail())),
            );
        }
    }
}

/// **The forage source at `tile`, as the improvement commands name it.** `same_source` matches on
/// the tile alone, so the stance and crop slots are placeholders — this exists so the two build
/// verbs cannot accidentally *carry* a stance or a crop into `set_improvement_on_working_bands`, and
/// so "the command names a source, not an assignment" is stated once.
fn forage_source(tile: UVec2) -> LaborTarget {
    LaborTarget::Forage {
        tile,
        floor: SOURCE_NAMED_NOT_ASSIGNED,
        species: None,
    }
}

/// **Gate a plant build verb against the crops the CREWS actually hold**, not against the auto-pick.
///
/// `cultivate`/`sow` name a tile and nothing else, so they used to hand [`validate_improvement`] a
/// `species: None` target — which judges what `resolve_committed_species` would pick *for* the
/// player. The crop that will really be committed is the one riding each band's
/// `LaborTarget::Forage::species`, and if this rung refuses it the labor arm silently declines to
/// commit: `patch.species` stays `None`, the rung's `eligible` gate is false, and **the build meter
/// never advances, with no player-facing feedback**. So every distinct crop held on the source is
/// judged, and the first refusal is the command's.
///
/// **Every crop, not just the first band's** — unlike `abandon_improvement`'s "the first band answers
/// for all of them", which is sound because at most one *improvement* is ever in flight on a source.
/// Crops are per band and can genuinely differ, and a second band's illegal pick stalls exactly the
/// same way the first's would.
///
/// With **no band working the source** the list is `[None]`, so the auto-pick is judged exactly as
/// before and the gates above the species check (knowledge, phase, ownership, the site rule) keep
/// their precedence over the command's own "no band is foraging" rejection.
fn validate_forage_improvement_for_crews(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    tile: UVec2,
    improvement: Improvement,
) -> Result<(), String> {
    // The crop list is collected first (a `World::query` needs unique access), so the validation
    // below sees the same immutable world every other gate does.
    for species in crops_named_on_forage_source(app, faction, tile) {
        let target = LaborTarget::Forage {
            tile,
            floor: SOURCE_NAMED_NOT_ASSIGNED,
            species,
        };
        validate_improvement(app, faction, &target, improvement)?;
    }
    Ok(())
}

/// The distinct crop selections this faction's bands carry on the forage source at `tile`, in a
/// stable order. `[None]` when nobody works it — see [`validate_forage_improvement_for_crews`] for
/// why that is the *no-crew* answer rather than an empty list.
fn crops_named_on_forage_source(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    tile: UVec2,
) -> Vec<Option<String>> {
    let source = forage_source(tile);
    let mut crops: Vec<Option<String>> = Vec::new();
    for (_, allocation) in app
        .world
        .query::<(&PopulationCohort, &LaborAllocation)>()
        .iter(&app.world)
        .filter(|(cohort, _)| cohort.faction == faction)
    {
        for assignment in &allocation.assignments {
            if !assignment.target.same_source(&source) || assignment.workers == 0 {
                continue;
            }
            let LaborTarget::Forage { species, .. } = &assignment.target else {
                continue;
            };
            if !crops.contains(species) {
                crops.push(species.clone());
            }
        }
    }
    if crops.is_empty() {
        crops.push(None);
    }
    crops
}

/// **Set the Cultivate improvement** on the forage patch at `tile` for the band(s) already working it
/// (Intensification — "Cultivate & Corral as explicit policies"). This is the command form of what
/// the client's policy picker does; it does **not** claim or complete anything.
///
/// The old early-claim (snap `cultivation_progress` to `1.0` once past a `claim_threshold`) is
/// **gone**: it would let the player skip the investment, which is the entire decision. Cultivating
/// now costs a real yield dip — while preparing, the patch pays only
/// `cultivation.cultivating_yield_fraction × its Sustain (MSY) ceiling` — and takes
/// `1 / progress_per_turn` turns of sustained work.
///
/// Gates (via the shared `validate_labor_policy`): the faction must know **Cultivation**, and the
/// patch must be **Thriving**, not already cultivated, and not another faction's.
fn handle_cultivate(app: &mut bevy::prelude::App, faction: FactionId, tile: UVec2) {
    // The source, named by the tile. Its stance is the band's, not the command's: this sets only the
    // improvement, and `same_source` matches on the tile alone. The **crop** is the band's too, and
    // is therefore what the gate has to judge — see `crops_named_on_forage_source`.
    if let Err(reason) =
        validate_forage_improvement_for_crews(app, faction, tile, Improvement::Cultivate)
    {
        warn!(
            target: "shadow_scale::command",
            command = "cultivate",
            faction = %faction.0,
            x = tile.x,
            y = tile.y,
            reason = %reason,
            "command.cultivate.rejected"
        );
        emit_command_failure(app, CommandEventKind::Cultivate, faction, reason);
        return;
    }

    let switched = set_improvement_on_working_bands(
        app,
        faction,
        &forage_source(tile),
        Some(Improvement::Cultivate),
    );
    if switched == 0 {
        emit_command_failure(
            app,
            CommandEventKind::Cultivate,
            faction,
            format!(
                "No band is foraging ({}, {}). Assign foragers to the patch first, then cultivate it.",
                tile.x, tile.y
            ),
        );
        return;
    }

    let tick = app.world.resource::<SimulationTick>().0;
    info!(
        target: "shadow_scale::command",
        command = "cultivate",
        faction = %faction.0,
        x = tile.x,
        y = tile.y,
        bands = switched,
        "command.cultivate.preparing"
    );
    push_command_event(
        app,
        tick,
        CommandEventKind::Cultivate,
        faction,
        format!(
            "Preparing patch at ({}, {}) for cultivation",
            tile.x, tile.y
        ),
        Some(format!(
            "status=preparing action=cultivate x={} y={} bands={}",
            tile.x, tile.y, switched
        )),
    );
}

/// **Set the `Sow` policy** on the tile at `tile` for the band(s) already foraging it — the plant
/// **rung-3** verb, and the exact twin of `handle_cultivate` one rung up. It is the command form of
/// what the client's improvement checkbox does; it **sows nothing outright**.
///
/// What makes it the interesting verb: `Sow` **places** a food source where the wild put none — a
/// crew commits the ground they already gather to a single crop instead of taking what the stand
/// offers. The seed itself goes into the ground in the labor arm, on the first turn a crew actually
/// works the tile under this policy — so `assign_labor … sow` and this command place a Field on
/// exactly the same terms.
///
/// **`Sow` used to place a source on ground carrying no forage site at all, and this arc reversed
/// that** — see [`validate_sow`] for the autopsy. Rung 3 is now bound to the ground its people
/// already work: the `plant:field` rung's `site_requirement` demands a **gathering site** that is
/// also **near fresh water**, because rung 3 knows how to move seed but not how to carry water, and
/// does not yet work unfamiliar ground. "Seed travels" moved up to **rung 4 (Farm)**, the first rung
/// to drop `requires_gathering_site`.
///
/// **And the ground is scarce, which is the point**: watered gathering sites are a small slice of the
/// sites a people hold, so *which* tile a band can farm is a real decision, and a band may have to
/// **move** to farm at all — that is the sedentarization pull.
///
/// Gates (via the shared `validate_labor_policy` → `validate_sow`): the tile must be ground the
/// people already gather ("Nobody gathers at (x, y)…") and be fresh-watered, the faction must know
/// **Seed Selection**, and the tile must not already be a Field or another people's — plus the
/// rejection when **no band is foraging** it.
fn handle_sow(app: &mut bevy::prelude::App, faction: FactionId, tile: UVec2) {
    // As in `handle_cultivate`: the command sets the *improvement* and leaves the crew's stance and
    // crop alone — which is exactly why the crop it must gate on is the crew's, judged at **this**
    // verb's rung. A `tended`-ceiling crop is legal to cultivate and illegal to sow, so this is the
    // only place that distinction can be drawn.
    if let Err(reason) = validate_forage_improvement_for_crews(app, faction, tile, Improvement::Sow)
    {
        warn!(
            target: "shadow_scale::command",
            command = "sow",
            faction = %faction.0,
            x = tile.x,
            y = tile.y,
            reason = %reason,
            "command.sow.rejected"
        );
        emit_command_failure(app, CommandEventKind::Sow, faction, reason);
        return;
    }

    let switched = set_improvement_on_working_bands(
        app,
        faction,
        &forage_source(tile),
        Some(Improvement::Sow),
    );
    if switched == 0 {
        emit_command_failure(
            app,
            CommandEventKind::Sow,
            faction,
            format!(
                "No band is foraging ({}, {}). Assign foragers to the ground first, then sow it.",
                tile.x, tile.y
            ),
        );
        return;
    }

    let tick = app.world.resource::<SimulationTick>().0;
    info!(
        target: "shadow_scale::command",
        command = "sow",
        faction = %faction.0,
        x = tile.x,
        y = tile.y,
        bands = switched,
        "command.sow.sowing"
    );
    push_command_event(
        app,
        tick,
        CommandEventKind::Sow,
        faction,
        format!("Sowing a field at ({}, {})", tile.x, tile.y),
        Some(format!(
            "status=sowing action=sow x={} y={} bands={}",
            tile.x, tile.y, switched
        )),
    );
}

/// **Set the Corral improvement** on the domesticated herd standing at `tile` for the band(s) already
/// hunting it — the animal mirror of `handle_cultivate`, and the command form of the client's policy
/// picker. While the pen is built the keeper takes only
/// `husbandry.corralling_yield_fraction × the herd's Sustain (MSY) ceiling`; at
/// `corral_progress == 1.0` the herd is penned (`Herd::corral_at`), stops roaming, and pays the
/// higher place-local corral yield. There is no early claim.
///
/// Gates (via the shared `validate_labor_policy`): the faction must know **Herding** and own the
/// **domesticated**, not-yet-penned herd.
fn handle_corral(app: &mut bevy::prelude::App, faction: FactionId, tile: UVec2) {
    let Some(fauna_id) = app
        .world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .find(|herd| herd.position() == tile)
        .map(|herd| herd.id.clone())
    else {
        warn!(
            target: "shadow_scale::command",
            command = "corral",
            faction = %faction.0,
            x = tile.x,
            y = tile.y,
            "command.corral.rejected=unknown_herd"
        );
        emit_command_failure(
            app,
            CommandEventKind::Corral,
            faction,
            format!("No herd at ({}, {}) to corral.", tile.x, tile.y),
        );
        return;
    };

    let target = LaborTarget::Hunt {
        fauna_id: fauna_id.clone(),
        floor: SOURCE_NAMED_NOT_ASSIGNED,
    };
    if let Err(reason) = validate_improvement(app, faction, &target, Improvement::Corral) {
        warn!(
            target: "shadow_scale::command",
            command = "corral",
            faction = %faction.0,
            herd = %fauna_id,
            reason = %reason,
            "command.corral.rejected"
        );
        emit_command_failure(app, CommandEventKind::Corral, faction, reason);
        return;
    }

    let switched =
        set_improvement_on_working_bands(app, faction, &target, Some(Improvement::Corral));
    if switched == 0 {
        emit_command_failure(
            app,
            CommandEventKind::Corral,
            faction,
            format!(
                "No band is hunting {}. Assign herders to it first, then corral it.",
                fauna_id
            ),
        );
        return;
    }

    let tick = app.world.resource::<SimulationTick>().0;
    info!(
        target: "shadow_scale::command",
        command = "corral",
        faction = %faction.0,
        herd = %fauna_id,
        x = tile.x,
        y = tile.y,
        bands = switched,
        "command.corral.building"
    );
    push_command_event(
        app,
        tick,
        CommandEventKind::Corral,
        faction,
        format!(
            "Building a corral for {} at ({}, {})",
            fauna_id, tile.x, tile.y
        ),
        Some(format!(
            "status=building action=corral herd={} x={} y={} bands={}",
            fauna_id, tile.x, tile.y, switched
        )),
    );
}

/// **Abandon the improvement running on one source** — the command that passes `None` where
/// `cultivate` / `sow` / `tame` / `corral` pass a verb (issue #442,
/// `docs/plan_investment_rung_toggle.md`). Text forms:
///   `abandon_improvement <faction> forage <x> <y>`
///   `abandon_improvement <faction> hunt <herd_id>`
///
/// **It exists because the split otherwise removed a capability the old model had by accident.** When
/// the build verb *was* the policy, changing your mind meant picking another policy, so a 25-turn
/// commitment could always be walked away from. Splitting the axes gave the stance its own control
/// and left the improvement with a set-only one; without this the player could start a Corral and
/// never stop it.
///
/// **Ungated, and that is the design.** Abandonment is not a rung transition: it asks for no
/// knowledge, no species ceiling, no site and — pointedly — no `Thriving` check. A **stalled** build
/// on unhealthy ground is exactly when a player reaches for this, and gating it on the conditions
/// that *started* the build would make the remedy unreachable in precisely that case. The only
/// rejections are "that is not a source kind" and "nobody is building anything there".
///
/// **It does not zero the meter, deliberately.** Each web already has a rule for a source nobody is
/// improving, and this simply hands the source back to it — the same state an assignment reaches when
/// it lapses out of range:
/// - **plant**: `advance_cultivation` bleeds `cultivation_progress` / `field_progress` at the rung's
///   `decay_per_turn` on every turn the patch is not worked as an improvement, so an abandoned
///   part-prepared patch reverts toward `0`;
/// - **animal**: `domestication_progress` is monotone-up (the tameness bleed was retired with the
///   neglect-escape arc) and the `animal:pen` rung's `decay_per_turn` is `0`, so the meter is kept.
///
/// Inventing a forfeit here would have made *this* path differ from walking the band away, which is
/// the same decision expressed with more steps.
///
/// The **stance and the crew are untouched**, as with every improvement-side command: the band goes
/// on working the source, undipped, under the policy the player chose.
fn handle_abandon_improvement(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    kind: String,
    tile: UVec2,
    fauna_id: String,
) {
    // The source, named the way its web names it. The stance is irrelevant — `same_source` matches on
    // the tile / herd id alone — so the default one stands in.
    let target = match kind.trim().to_ascii_lowercase().as_str() {
        "forage" => LaborTarget::Forage {
            tile,
            floor: SOURCE_NAMED_NOT_ASSIGNED,
            species: None,
        },
        "hunt" if !fauna_id.trim().is_empty() => LaborTarget::Hunt {
            fauna_id: fauna_id.clone(),
            floor: SOURCE_NAMED_NOT_ASSIGNED,
        },
        "hunt" => {
            emit_command_failure(
                app,
                CommandEventKind::CancelOrder,
                faction,
                "abandon_improvement hunt requires <herd_id>.".to_string(),
            );
            return;
        }
        other => {
            emit_command_failure(
                app,
                CommandEventKind::CancelOrder,
                faction,
                format!("Unknown source kind '{}' — expected forage or hunt.", other),
            );
            return;
        }
    };

    // **Which verb is actually running**, resolved before the clear so the feed line can name it (and
    // so "nothing to abandon" is a distinct, honest rejection rather than a silent no-op). At most one
    // improvement is ever in flight on a source, so the first band working it answers for all of them.
    let running = app
        .world
        .query::<(&PopulationCohort, &LaborAllocation)>()
        .iter(&app.world)
        .filter(|(cohort, _)| cohort.faction == faction)
        .find_map(|(_, allocation)| {
            allocation
                .assignments
                .iter()
                .find(|assignment| assignment.target.same_source(&target))
                .and_then(|assignment| assignment.improvement)
        });
    let Some(improvement) = running else {
        emit_command_failure(
            app,
            CommandEventKind::CancelOrder,
            faction,
            format!("Nothing is being built on {}.", describe_source(&target)),
        );
        return;
    };
    // The improvement's own feed channel, so abandoning a Corral reads on the same line the Corral
    // was announced on.
    let event_kind = improvement_event_kind(improvement);

    let cleared = set_improvement_on_working_bands(app, faction, &target, None);
    let tick = app.world.resource::<SimulationTick>().0;
    info!(
        target: "shadow_scale::command",
        command = "abandon_improvement",
        faction = %faction.0,
        improvement = improvement.as_str(),
        bands = cleared,
        "command.abandon_improvement.abandoned"
    );
    push_command_event(
        app,
        tick,
        event_kind,
        faction,
        format!(
            "Abandoned {} on {}",
            improvement.as_str(),
            describe_source(&target)
        ),
        Some(format!(
            "status=abandoned action=abandon_improvement improvement={} bands={}",
            improvement.as_str(),
            cleared
        )),
    );
}

/// A source named for a player-facing line: a patch by its coordinates, a herd by its id.
fn describe_source(target: &LaborTarget) -> String {
    match target {
        LaborTarget::Forage { tile, .. } => format!("({}, {})", tile.x, tile.y),
        LaborTarget::Hunt { fauna_id, .. } => fauna_id.clone(),
        LaborTarget::Scout => "scouting".to_string(),
        LaborTarget::Warrior => "the watch".to_string(),
    }
}

/// The command-feed channel an improvement's events ride — the same one its *setting* verb used, so a
/// rung's whole life (start → complete, or start → abandon) reads on one line.
fn improvement_event_kind(improvement: Improvement) -> CommandEventKind {
    match improvement {
        Improvement::Cultivate => CommandEventKind::Cultivate,
        Improvement::Sow => CommandEventKind::Sow,
        Improvement::Tame => CommandEventKind::Tame,
        Improvement::Corral => CommandEventKind::Corral,
    }
}

/// Grazing 2d-β — the `ExtendPen` command. Put an owned, **built** pen at `tile` into the "extending"
/// state so its keeper band works off the next fenced ring (`pen_radius += 1` at completion, ~25 turns
/// at the corral build rate, with the harvest dipped to `corralling_yield_fraction` while it fences).
/// The pen's whole life rides `CommandEventKind::Corral`, so the extend feed lines reuse it.
///
/// Validates: a herd penned **exactly at `tile`** (`corralled_at`, the fixed pen anchor — not the
/// roaming `position()` `corral` keys off), owned by `faction`, the faction knows **Penning** (a ring
/// rides the same `animal:pen` rung as the initial build, so it takes that rung's gate — not Herding),
/// `pen_radius` below `husbandry.pen_radius_max`, **no extension already in flight**, and a band is
/// keeping it (or the ring never accrues, and an untended pen escapes anyway).
fn handle_extend_pen(app: &mut bevy::prelude::App, faction: FactionId, tile: UVec2) {
    let Some(fauna_id) = app
        .world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .find(|herd| herd.corralled_at == Some(tile))
        .map(|herd| herd.id.clone())
    else {
        warn!(
            target: "shadow_scale::command",
            command = "extend_pen",
            faction = %faction.0,
            x = tile.x,
            y = tile.y,
            "command.extend_pen.rejected=no_pen"
        );
        emit_command_failure(
            app,
            CommandEventKind::Corral,
            faction,
            format!("No pen at ({}, {}) to extend.", tile.x, tile.y),
        );
        return;
    };

    let (owns, knows_penning, can_pen, species, at_max, already_extending, pen_radius_max) = {
        let pen_radius_max = app
            .world
            .resource::<FaunaConfigHandle>()
            .get()
            .husbandry
            .pen_radius_max;
        // A fence ring rides the **same `animal:pen` rung** as the initial build (2d-β: same labor,
        // same dip, same dials), so it takes that rung's gate too — **Penning** since the §4.3
        // reshuffle. Read off the rung, so a ring can never be gated differently from the pen it
        // extends.
        let (knowledge_threshold, pen_unlock) = {
            let ladder = app.world.resource::<LadderConfigHandle>().get();
            (
                ladder.knowledge.completion_threshold,
                ladder.rung(RungKey::AnimalPen).unlock_discovery_id(),
            )
        };
        let knows_penning = pen_unlock.is_none_or(|knowledge| {
            knows(
                app.world.resource::<DiscoveryProgressLedger>(),
                faction,
                knowledge,
                knowledge_threshold,
            )
        });
        let herd = app
            .world
            .resource::<HerdRegistry>()
            .find(&fauna_id)
            .expect("herd resolved above");
        (
            herd.owner == Some(faction),
            knows_penning,
            herd.can_pen(),
            herd.species.clone(),
            herd.pen_radius >= pen_radius_max,
            herd.pen_extending,
            pen_radius_max,
        )
    };
    let reason = if !can_pen {
        // Grazing 2d-δ: belt-and-braces — a non-`Pen` species can never be penned, so this is
        // unreachable via the gated corral path, but the extend command states the rule explicitly.
        Some(format!("{species} cannot be penned."))
    } else if !knows_penning {
        Some(
            "Your people have not learned Penning yet. Tame and keep herds to learn it."
                .to_string(),
        )
    } else if !owns {
        Some(format!("You do not own the pen for {}.", fauna_id))
    } else if already_extending {
        Some(format!(
            "The pen for {} is already being extended.",
            fauna_id
        ))
    } else if at_max {
        Some(format!(
            "The pen for {} is already at its maximum size.",
            fauna_id
        ))
    } else {
        None
    };
    if let Some(reason) = reason {
        warn!(
            target: "shadow_scale::command",
            command = "extend_pen",
            faction = %faction.0,
            herd = %fauna_id,
            reason = %reason,
            "command.extend_pen.rejected"
        );
        emit_command_failure(app, CommandEventKind::Corral, faction, reason);
        return;
    }

    // A band must be keeping the pen (a Hunt assignment on it, any policy) or the ring never accrues.
    let keeper_target = LaborTarget::Hunt {
        fauna_id: fauna_id.clone(),
        floor: SOURCE_NAMED_NOT_ASSIGNED, // matched by `same_source` (herd id) — the floor is irrelevant
    };
    let keepers = app
        .world
        .query::<(&PopulationCohort, &LaborAllocation)>()
        .iter(&app.world)
        .filter(|(cohort, _)| cohort.faction == faction)
        .filter(|(_, allocation)| allocation.workers_on(&keeper_target) > 0)
        .count();
    if keepers == 0 {
        emit_command_failure(
            app,
            CommandEventKind::Corral,
            faction,
            format!(
                "No band is keeping {}. Assign herders to it first, then extend the pen.",
                fauna_id
            ),
        );
        return;
    }

    // Enter the extending state — `begin_pen_extension` re-checks is_corralled / not-extending /
    // below-max, so the guard and the validation above can never disagree.
    let began = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|h| h.id == fauna_id)
            .expect("herd resolved above");
        herd.begin_pen_extension(pen_radius_max)
    };
    if !began {
        emit_command_failure(
            app,
            CommandEventKind::Corral,
            faction,
            format!("Cannot extend the pen for {} right now.", fauna_id),
        );
        return;
    }

    let tick = app.world.resource::<SimulationTick>().0;
    info!(
        target: "shadow_scale::command",
        command = "extend_pen",
        faction = %faction.0,
        herd = %fauna_id,
        x = tile.x,
        y = tile.y,
        "command.extend_pen.extending"
    );
    push_command_event(
        app,
        tick,
        CommandEventKind::Corral,
        faction,
        format!(
            "Extending the pen for {} at ({}, {})",
            fauna_id, tile.x, tile.y
        ),
        Some(format!(
            "status=extending action=extend_pen herd={} x={} y={}",
            fauna_id, tile.x, tile.y
        )),
    );
}

/// Set (or clear) the **improvement** on every band of `faction` **already working** `target`'s
/// source (matched by `LaborTarget::same_source`, so the tile / herd id). Returns how many bands were
/// updated (`0` = nobody is working that source, which the callers report as *"staff it first"*).
/// The shared body of the four improvement commands (`cultivate` / `sow` / `tame` / `corral`).
///
/// **It touches nothing but the improvement slot** (issue #442). Its predecessor
/// `set_policy_on_working_bands` had to rebuild each band's whole `LaborTarget` — and therefore had to
/// carefully merge back the crop the band had selected, or a policy switch silently cleared it. With
/// the verb in its own slot the target is never rewritten, so the stance, the crew and the committed
/// species all survive by construction and the `merge_target` helper that guarded the species is gone.
fn set_improvement_on_working_bands(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    target: &LaborTarget,
    improvement: Option<Improvement>,
) -> usize {
    // Each band's own assignment target, because the re-seed below must price the source under the
    // stance and crop that band actually holds — the command names neither.
    let bands: Vec<(Entity, u32, LaborTarget)> = app
        .world
        .query::<(Entity, &PopulationCohort, &LaborAllocation)>()
        .iter(&app.world)
        .filter(|(_, cohort, _)| cohort.faction == faction)
        .filter_map(|(entity, _, allocation)| {
            allocation
                .assignments
                .iter()
                .find(|assignment| assignment.target.same_source(target) && assignment.workers > 0)
                .map(|assignment| (entity, assignment.workers, assignment.target.clone()))
        })
        .collect();
    for (entity, workers, band_target) in &bands {
        {
            let mut allocation = band_allocation_mut(app, *entity);
            allocation.set_improvement(band_target, improvement);
        }
        // Starting (or finishing) a build changes the expected yield — the take drops to
        // `stance ceiling × yield_fraction_while_building` — so re-seed the source's telemetry from
        // the new forecast, the same reason `handle_assign_labor` seeds after a stance edit.
        seed_source_yield(app, *entity, band_target, improvement, *workers);
    }
    bands.len()
}

fn handle_reload_simulation_config(app: &mut bevy::prelude::App, path: Option<String>) {
    let command_sender = {
        let res = app.world.resource::<CommandSenderResource>();
        res.0.clone()
    };

    let current_config = app.world.resource::<SimulationConfig>().clone();

    let requested_path = path
        .and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(PathBuf::from(trimmed))
            }
        })
        .or_else(|| {
            app.world
                .resource::<SimulationConfigMetadata>()
                .path()
                .cloned()
        });

    let (mut new_config, applied_path) = match requested_path {
        Some(path) => match SimulationConfig::from_file(&path) {
            Ok(cfg) => (cfg, Some(path)),
            Err(err) => {
                warn!(
                    target: "shadow_scale::config",
                    error = %err,
                    "simulation_config.reload_failed"
                );
                return;
            }
        },
        None => (SimulationConfig::builtin(), None),
    };

    // Reapply the port base the process ACTUALLY bound (post auto-bump), so a
    // reload of an unchanged file keeps the live binds and doesn't spuriously
    // trip the socket_changed=restart_required warning below. Rebinding live
    // sockets is out of scope: the reloaded config must describe the ports the
    // server holds, not the ones the file asks for.
    if let Some(resolved) = app.world.get_resource::<ResolvedPortBase>().copied() {
        apply_port_base(&mut new_config, resolved.0);
    }

    {
        let mut metadata = app.world.resource_mut::<SimulationConfigMetadata>();
        metadata.set_path(applied_path.clone());
        metadata.set_seed_random(new_config.map_seed == 0);
    }

    {
        let mut config_res = app.world.resource_mut::<SimulationConfig>();
        *config_res = new_config.clone();
    }

    // The publication ring's depth is a constant now (`snapshot::PUBLICATION_RING_DEPTH`), so a
    // config reload no longer resizes it; `checkpoint_history_turns` is read where it is used.

    let watch_path = app
        .world
        .resource::<SimulationConfigMetadata>()
        .path()
        .cloned();

    {
        let mut watcher_state = app.world.resource_mut::<ConfigWatcherRegistry>();
        watcher_state.restart_simulation(watch_path, command_sender);
    }

    info!(
        target: "shadow_scale::config",
        path = applied_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "builtin".to_string()),
        grid_width = new_config.grid_size.x,
        grid_height = new_config.grid_size.y,
        "simulation_config.reloaded"
    );

    if new_config.grid_size != current_config.grid_size {
        warn!(
            target: "shadow_scale::config",
            old = ?current_config.grid_size,
            new = ?new_config.grid_size,
            "simulation_config.grid_size_changed=map_reset_recommended"
        );
    }

    if new_config.command_bind != current_config.command_bind
        || new_config.port_base_bind != current_config.port_base_bind
        || new_config.snapshot_flat_bind != current_config.snapshot_flat_bind
        || new_config.log_bind != current_config.log_bind
    {
        warn!(
            target: "shadow_scale::config",
            "simulation_config.socket_changed=restart_required"
        );
    }
}

fn handle_reload_turn_pipeline_config(app: &mut bevy::prelude::App, path: Option<String>) {
    let command_sender = {
        let res = app.world.resource::<CommandSenderResource>();
        res.0.clone()
    };

    let requested_path = path
        .and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(PathBuf::from(trimmed))
            }
        })
        .or_else(|| {
            app.world
                .resource::<TurnPipelineConfigMetadata>()
                .path()
                .cloned()
        });

    let (new_config, applied_path) = match requested_path {
        Some(path) => match TurnPipelineConfig::from_file(&path) {
            Ok(cfg) => (Arc::new(cfg), Some(path)),
            Err(err) => {
                warn!(
                    target: "shadow_scale::config",
                    error = %err,
                    "turn_pipeline_config.reload_failed"
                );
                return;
            }
        },
        None => (TurnPipelineConfig::builtin(), None),
    };

    {
        let mut metadata = app.world.resource_mut::<TurnPipelineConfigMetadata>();
        metadata.set_path(applied_path.clone());
    }

    {
        let mut handle = app.world.resource_mut::<TurnPipelineConfigHandle>();
        handle.replace(Arc::clone(&new_config));
    }

    let watch_path = app
        .world
        .resource::<TurnPipelineConfigMetadata>()
        .path()
        .cloned();

    {
        let mut watcher_state = app.world.resource_mut::<ConfigWatcherRegistry>();
        watcher_state.restart_turn_pipeline(watch_path, command_sender);
    }

    let logistics = new_config.logistics();
    info!(
        target: "shadow_scale::config",
        path = applied_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "builtin".to_string()),
        flow_gain_min = logistics.flow_gain_min().to_f32(),
        flow_gain_max = logistics.flow_gain_max().to_f32(),
        "turn_pipeline_config.reloaded"
    );
}

fn handle_reload_snapshot_overlays_config(app: &mut bevy::prelude::App, path: Option<String>) {
    let command_sender = {
        let res = app.world.resource::<CommandSenderResource>();
        res.0.clone()
    };

    let requested_path = path
        .and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(PathBuf::from(trimmed))
            }
        })
        .or_else(|| {
            app.world
                .resource::<SnapshotOverlaysConfigMetadata>()
                .path()
                .cloned()
        });

    let (new_config, applied_path) = match requested_path {
        Some(path) => match SnapshotOverlaysConfig::from_file(&path) {
            Ok(cfg) => (Arc::new(cfg), Some(path)),
            Err(err) => {
                warn!(
                    target: "shadow_scale::config",
                    error = %err,
                    "snapshot_overlays_config.reload_failed"
                );
                return;
            }
        },
        None => (SnapshotOverlaysConfig::builtin(), None),
    };

    {
        let mut metadata = app.world.resource_mut::<SnapshotOverlaysConfigMetadata>();
        metadata.set_path(applied_path.clone());
    }

    {
        let mut handle = app.world.resource_mut::<SnapshotOverlaysConfigHandle>();
        handle.replace(Arc::clone(&new_config));
    }

    let watch_path = app
        .world
        .resource::<SnapshotOverlaysConfigMetadata>()
        .path()
        .cloned();

    {
        let mut watcher_state = app.world.resource_mut::<ConfigWatcherRegistry>();
        watcher_state.restart_snapshot_overlays(watch_path, command_sender);
    }

    let corruption = new_config.corruption();
    let military = new_config.military();

    info!(
        target: "shadow_scale::config",
        path = applied_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "builtin".to_string()),
        corruption_logistics_weight = corruption.logistics_weight().to_f32(),
        corruption_trade_weight = corruption.trade_weight().to_f32(),
        corruption_military_weight = corruption.military_weight().to_f32(),
        corruption_governance_weight = corruption.governance_weight().to_f32(),
        military_presence_weight = military.presence_weight().to_f32(),
        military_support_weight = military.support_weight().to_f32(),
        "snapshot_overlays_config.reloaded"
    );
}

fn handle_reload_crisis_archetypes_config(app: &mut bevy::prelude::App, path: Option<String>) {
    let command_sender = {
        let res = app.world.resource::<CommandSenderResource>();
        res.0.clone()
    };

    let requested_path = path
        .and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(PathBuf::from(trimmed))
            }
        })
        .or_else(|| {
            app.world
                .resource::<CrisisArchetypeCatalogMetadata>()
                .path()
                .cloned()
        });

    let (new_catalog, applied_path) = match requested_path {
        Some(path) => match CrisisArchetypeCatalog::from_file(&path) {
            Ok(cfg) => (Arc::new(cfg), Some(path)),
            Err(err) => {
                warn!(
                    target: "shadow_scale::config",
                    error = %err,
                    "crisis_archetypes.reload_failed"
                );
                return;
            }
        },
        None => (CrisisArchetypeCatalog::builtin(), None),
    };

    {
        let mut metadata = app.world.resource_mut::<CrisisArchetypeCatalogMetadata>();
        metadata.set_path(applied_path.clone());
    }

    {
        let mut handle = app.world.resource_mut::<CrisisArchetypeCatalogHandle>();
        handle.replace(Arc::clone(&new_catalog));
    }

    let watch_path = app
        .world
        .resource::<CrisisArchetypeCatalogMetadata>()
        .path()
        .cloned();

    {
        let mut watcher_state = app.world.resource_mut::<ConfigWatcherRegistry>();
        watcher_state.restart_crisis_archetypes(watch_path, command_sender);
    }

    info!(
        target: "shadow_scale::config",
        path = applied_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "builtin".to_string()),
        archetype_count = new_catalog.archetypes.len(),
        "crisis_archetypes.reloaded"
    );
}

fn handle_reload_crisis_modifiers_config(app: &mut bevy::prelude::App, path: Option<String>) {
    let command_sender = {
        let res = app.world.resource::<CommandSenderResource>();
        res.0.clone()
    };

    let requested_path = path
        .and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(PathBuf::from(trimmed))
            }
        })
        .or_else(|| {
            app.world
                .resource::<CrisisModifierCatalogMetadata>()
                .path()
                .cloned()
        });

    let (new_catalog, applied_path) = match requested_path {
        Some(path) => match CrisisModifierCatalog::from_file(&path) {
            Ok(cfg) => (Arc::new(cfg), Some(path)),
            Err(err) => {
                warn!(
                    target: "shadow_scale::config",
                    error = %err,
                    "crisis_modifiers.reload_failed"
                );
                return;
            }
        },
        None => (CrisisModifierCatalog::builtin(), None),
    };

    {
        let mut metadata = app.world.resource_mut::<CrisisModifierCatalogMetadata>();
        metadata.set_path(applied_path.clone());
    }

    {
        let mut handle = app.world.resource_mut::<CrisisModifierCatalogHandle>();
        handle.replace(Arc::clone(&new_catalog));
    }

    let watch_path = app
        .world
        .resource::<CrisisModifierCatalogMetadata>()
        .path()
        .cloned();

    {
        let mut watcher_state = app.world.resource_mut::<ConfigWatcherRegistry>();
        watcher_state.restart_crisis_modifiers(watch_path, command_sender);
    }

    info!(
        target: "shadow_scale::config",
        path = applied_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "builtin".to_string()),
        modifier_count = new_catalog.modifiers.len(),
        "crisis_modifiers.reloaded"
    );
}

fn handle_reload_crisis_telemetry_config(app: &mut bevy::prelude::App, path: Option<String>) {
    let command_sender = {
        let res = app.world.resource::<CommandSenderResource>();
        res.0.clone()
    };

    let requested_path = path
        .and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(PathBuf::from(trimmed))
            }
        })
        .or_else(|| {
            app.world
                .resource::<CrisisTelemetryConfigMetadata>()
                .path()
                .cloned()
        });

    let (new_config, applied_path) = match requested_path {
        Some(path) => match CrisisTelemetryConfig::from_file(&path) {
            Ok(cfg) => (Arc::new(cfg), Some(path)),
            Err(err) => {
                warn!(
                    target: "shadow_scale::config",
                    error = %err,
                    "crisis_telemetry_config.reload_failed"
                );
                return;
            }
        },
        None => (CrisisTelemetryConfig::builtin(), None),
    };

    {
        let mut metadata = app.world.resource_mut::<CrisisTelemetryConfigMetadata>();
        metadata.set_path(applied_path.clone());
    }

    {
        let mut handle = app.world.resource_mut::<CrisisTelemetryConfigHandle>();
        handle.replace(Arc::clone(&new_config));
    }

    {
        let mut telemetry = app.world.resource_mut::<CrisisTelemetry>();
        telemetry.apply_config(new_config.as_ref());
    }

    let watch_path = app
        .world
        .resource::<CrisisTelemetryConfigMetadata>()
        .path()
        .cloned();

    {
        let mut watcher_state = app.world.resource_mut::<ConfigWatcherRegistry>();
        watcher_state.restart_crisis_telemetry(watch_path, command_sender);
    }

    info!(
        target: "shadow_scale::config",
        path = applied_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "builtin".to_string()),
        ema_alpha = new_config.ema_alpha,
        gauge_count = new_config.gauges.len(),
        "crisis_telemetry_config.reloaded"
    );
}

fn command_from_payload(payload: ProtoCommandPayload) -> Option<Command> {
    match payload {
        ProtoCommandPayload::Turn { steps } => Some(Command::Turn(steps)),
        ProtoCommandPayload::ResetMap { width, height } => {
            Some(Command::ResetMap { width, height })
        }
        ProtoCommandPayload::Heat {
            target_x,
            target_y,
            delta,
        } => Some(Command::Heat {
            target_x,
            target_y,
            delta,
        }),
        ProtoCommandPayload::Orders {
            faction_id,
            directive,
        } => match directive {
            ProtoOrdersDirective::Ready => Some(Command::Orders {
                faction: FactionId(faction_id),
                orders: FactionOrders::end_turn(),
            }),
        },
        ProtoCommandPayload::Rollback { tick } => Some(Command::Rollback { tick }),
        ProtoCommandPayload::AxisBias { axis, value } => Some(Command::AxisBias {
            axis: axis as usize,
            value,
        }),
        ProtoCommandPayload::SupportInfluencer { id, magnitude } => {
            Some(Command::SupportInfluencer { id, magnitude })
        }
        ProtoCommandPayload::SuppressInfluencer { id, magnitude } => {
            Some(Command::SuppressInfluencer { id, magnitude })
        }
        ProtoCommandPayload::SupportInfluencerChannel {
            id,
            channel,
            magnitude,
        } => {
            let mapped = map_support_channel(channel)?;
            Some(Command::SupportInfluencerChannel {
                id,
                channel: mapped,
                magnitude,
            })
        }
        ProtoCommandPayload::SpawnInfluencer { scope, generation } => {
            let generation = generation.map(|value| value as GenerationId);
            Some(Command::SpawnInfluencer { scope, generation })
        }
        ProtoCommandPayload::InjectCorruption {
            subsystem,
            intensity,
            exposure_timer,
        } => {
            let exposure = if exposure_timer > u16::MAX as u32 {
                warn!(
                    "Proto command exposure_timer {} exceeds u16::MAX; clamping",
                    exposure_timer
                );
                u16::MAX
            } else {
                exposure_timer as u16
            };
            Some(Command::InjectCorruption {
                subsystem,
                intensity,
                exposure_timer: exposure,
            })
        }
        ProtoCommandPayload::UpdateEspionageGenerators { updates } => {
            Some(Command::UpdateEspionageGenerators { updates })
        }
        ProtoCommandPayload::QueueEspionageMission {
            mission_id,
            owner_faction,
            target_owner_faction,
            discovery_id,
            agent_handle,
            target_tier,
            scheduled_tick,
        } => {
            let params = QueueMissionParams {
                mission_id: EspionageMissionId::new(mission_id),
                owner: FactionId(owner_faction),
                target_owner: FactionId(target_owner_faction),
                discovery_id,
                agent: EspionageAgentHandle(agent_handle),
                target_tier,
                scheduled_tick: scheduled_tick.unwrap_or(0),
            };
            Some(Command::QueueEspionageMission { params })
        }
        ProtoCommandPayload::UpdateEspionageQueueDefaults {
            scheduled_tick_offset,
            target_tier,
        } => Some(Command::UpdateEspionageQueueDefaults {
            scheduled_tick_offset: scheduled_tick_offset.map(|value| value as u64),
            target_tier,
        }),
        ProtoCommandPayload::UpdateCounterIntelPolicy { faction, policy } => {
            match map_security_policy(policy) {
                Some(mapped) => Some(Command::UpdateCounterIntelPolicy {
                    faction: FactionId(faction),
                    policy: mapped,
                }),
                None => {
                    warn!(
                        target: "shadow_scale::server",
                        faction,
                        policy = ?policy,
                        "counter_intel_policy.update.invalid"
                    );
                    None
                }
            }
        }
        ProtoCommandPayload::AdjustCounterIntelBudget {
            faction,
            reserve,
            delta,
        } => {
            if reserve.is_none() && delta.is_none() {
                warn!(
                    target: "shadow_scale::server",
                    faction,
                    "counter_intel_budget.adjust.ignore_empty"
                );
                None
            } else {
                Some(Command::AdjustCounterIntelBudget {
                    faction: FactionId(faction),
                    reserve: reserve.map(scalar_from_f32),
                    delta: delta.map(scalar_from_f32),
                })
            }
        }
        ProtoCommandPayload::ReloadConfig { kind, path } => {
            Some(Command::ReloadConfig { kind, path })
        }
        ProtoCommandPayload::SetCrisisAutoSeed { enabled } => {
            Some(Command::SetCrisisAutoSeed { enabled })
        }
        ProtoCommandPayload::SetFogEnabled { enabled } => Some(Command::SetFogEnabled { enabled }),
        ProtoCommandPayload::SpawnCrisis {
            faction_id,
            archetype_id,
        } => Some(Command::SpawnCrisis {
            faction: FactionId(faction_id),
            archetype_id,
        }),
        ProtoCommandPayload::SetStartProfile { profile_id } => {
            Some(Command::SetStartProfile { profile_id })
        }
        ProtoCommandPayload::AssignLabor {
            faction_id,
            band_id,
            role,
            workers,
            target_x,
            target_y,
            fauna_id,
            // The stance token is retired by the harvest floor arc; the proto field survives only
            // because a shipped field number is immutable, and nothing reads it.
            policy: _,
            species,
            floor,
        } => Some(Command::AssignLabor {
            faction: FactionId(faction_id),
            band_id,
            role,
            workers,
            target_x,
            target_y,
            fauna_id,
            species,
            floor,
        }),
        ProtoCommandPayload::MoveBand {
            faction_id,
            band_id,
            target_x,
            target_y,
        } => Some(Command::MoveBand {
            faction: FactionId(faction_id),
            band_id,
            target_x,
            target_y,
        }),
        ProtoCommandPayload::SendExpedition {
            faction_id,
            band_id,
            party_workers,
            target_x,
            target_y,
        } => Some(Command::SendExpedition {
            faction: FactionId(faction_id),
            band_id,
            party_workers,
            target_x,
            target_y,
        }),
        ProtoCommandPayload::RecallExpedition {
            faction_id,
            expedition_band_id,
        } => Some(Command::RecallExpedition {
            faction: FactionId(faction_id),
            expedition_band_id,
        }),
        ProtoCommandPayload::SendHuntExpedition {
            faction_id,
            band_id,
            party_workers,
            fauna_id,
            floor,
        } => Some(Command::SendHuntExpedition {
            faction: FactionId(faction_id),
            band_id,
            party_workers,
            fauna_id,
            floor,
        }),
        ProtoCommandPayload::FoundSettlement {
            faction_id,
            target_x,
            target_y,
        } => Some(Command::FoundSettlement {
            faction: FactionId(faction_id),
            target_x,
            target_y,
        }),
        // Retired single-task band orders (Early-Game Labor slice 3a): the source-centric
        // `assign_labor` / `move_band` replace them. Ignored if a stale client still sends one.
        ProtoCommandPayload::ScoutArea { .. }
        | ProtoCommandPayload::FollowHerd { .. }
        | ProtoCommandPayload::ForageTile { .. }
        | ProtoCommandPayload::HuntGame { .. }
        | ProtoCommandPayload::HuntFauna { .. } => {
            warn!(
                target: "shadow_scale::server",
                "command.retired=ignored (replaced by assign_labor/move_band)"
            );
            None
        }
        ProtoCommandPayload::AnswerFork {
            faction_id,
            beat_id,
            choice_id,
        } => Some(Command::AnswerFork {
            faction: FactionId(faction_id),
            beat_id,
            choice_id,
        }),
        ProtoCommandPayload::Tame {
            faction_id,
            herd_id,
        } => Some(Command::Tame {
            faction: FactionId(faction_id),
            herd_id,
        }),
        ProtoCommandPayload::Cultivate {
            faction_id,
            target_x,
            target_y,
        } => Some(Command::Cultivate {
            faction: FactionId(faction_id),
            target_x,
            target_y,
        }),
        ProtoCommandPayload::Sow {
            faction_id,
            target_x,
            target_y,
        } => Some(Command::Sow {
            faction: FactionId(faction_id),
            target_x,
            target_y,
        }),
        ProtoCommandPayload::Corral {
            faction_id,
            target_x,
            target_y,
        } => Some(Command::Corral {
            faction: FactionId(faction_id),
            target_x,
            target_y,
        }),
        ProtoCommandPayload::AbandonImprovement {
            faction_id,
            kind,
            target_x,
            target_y,
            fauna_id,
        } => Some(Command::AbandonImprovement {
            faction: FactionId(faction_id),
            kind,
            target_x,
            target_y,
            fauna_id,
        }),
        ProtoCommandPayload::ExtendPen {
            faction_id,
            target_x,
            target_y,
        } => Some(Command::ExtendPen {
            faction: FactionId(faction_id),
            target_x,
            target_y,
        }),
        ProtoCommandPayload::CancelOrder {
            faction_id,
            band_id,
            scope,
        } => Some(Command::CancelOrder {
            faction: FactionId(faction_id),
            band_id,
            scope,
        }),
        ProtoCommandPayload::ExportMap { path } => Some(Command::ExportMap { path }),
        ProtoCommandPayload::SetConfigOverride { kind, patch_json } => {
            Some(Command::SetConfigOverride { kind, patch_json })
        }
        ProtoCommandPayload::ClearConfigOverrides => Some(Command::ClearConfigOverrides),
        ProtoCommandPayload::Resync => Some(Command::Resync),
        ProtoCommandPayload::NewGame {
            preset_id,
            width,
            height,
            seed,
            profile_id,
        } => Some(Command::NewGame {
            preset_id,
            width,
            height,
            seed,
            profile_id,
        }),
    }
}

fn map_support_channel(channel: ProtoSupportChannel) -> Option<SupportChannel> {
    match channel {
        ProtoSupportChannel::Popular => Some(SupportChannel::Popular),
        ProtoSupportChannel::Peer => Some(SupportChannel::Peer),
        ProtoSupportChannel::Institutional => Some(SupportChannel::Institutional),
        ProtoSupportChannel::Humanitarian => Some(SupportChannel::Humanitarian),
    }
}

fn map_security_policy(kind: SecurityPolicyKind) -> Option<SecurityPolicy> {
    Some(match kind {
        SecurityPolicyKind::Lenient => SecurityPolicy::Lenient,
        SecurityPolicyKind::Standard => SecurityPolicy::Standard,
        SecurityPolicyKind::Hardened => SecurityPolicy::Hardened,
        SecurityPolicyKind::Crisis => SecurityPolicy::Crisis,
    })
}
/// Heat a tile, addressed by **position**.
///
/// It used to take raw `Entity` bits off the wire. A rollback rebuilds the world and renumbers every
/// entity, so a logged `Heat` naming one would resolve to nothing when replayed — the same reason no
/// `Entity` crosses any other persistence boundary in this arc.
fn apply_heat(app: &mut bevy::prelude::App, position: UVec2, delta_raw: i64) {
    let Some(entity) = app
        .world
        .resource::<TileRegistry>()
        .index(position.x, position.y)
    else {
        warn!(
            target: "shadow_scale::server",
            x = position.x,
            y = position.y,
            "command.heat.rejected=no_such_tile"
        );
        return;
    };
    if let Some(mut tile) = app.world.get_mut::<Tile>(entity) {
        tile.temperature += Scalar::from_raw(delta_raw);
    } else {
        warn!(
            target: "shadow_scale::server",
            x = position.x,
            y = position.y,
            "command.heat.rejected=tile_missing"
        );
    }
}

fn ensure_land_tile(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    coords: UVec2,
    command_label: &str,
    event_kind: Option<CommandEventKind>,
) -> Option<Entity> {
    let tile_entity = {
        let registry = app.world.resource::<TileRegistry>();
        registry.index(coords.x, coords.y)
    };
    let Some(tile_entity) = tile_entity else {
        log_tile_rejection(
            app,
            faction,
            coords,
            command_label,
            "out_of_bounds",
            event_kind,
        );
        return None;
    };
    let Some(tile) = app.world.get::<Tile>(tile_entity) else {
        log_tile_rejection(
            app,
            faction,
            coords,
            command_label,
            "tile_missing",
            event_kind,
        );
        return None;
    };
    if tile.terrain_tags.contains(TerrainTags::WATER) {
        log_tile_rejection(
            app,
            faction,
            coords,
            command_label,
            "water_tile",
            event_kind,
        );
        return None;
    }
    Some(tile_entity)
}

/// Resolve a [`BandId`] to the entity that holds it, gated on faction ownership.
///
/// **This replaces a raw-`Entity`-bits path; there is deliberately no fallback to it.** The wire
/// used to carry `entity.to_bits()`, which made the client→server protocol traffic in ECS handles —
/// and a handle is not an identity: a rollback rebuilds the world and renumbers every entity, so a
/// logged command naming one resolved to nothing when replayed. `BandId` is the identity the sim
/// already had. There are no shipped clients, so accepting both forms would be permanent complexity
/// bought for nobody.
fn resolve_starting_unit_entity(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_id: u64,
    command_label: &str,
    event_kind: CommandEventKind,
) -> Option<Entity> {
    let wanted = BandId(band_id);
    let found = {
        let mut query = app.world.query::<(Entity, &BandId)>();
        query
            .iter(&app.world)
            .find(|(_, id)| **id == wanted)
            .map(|(entity, _)| entity)
    };
    let Some(entity) = found else {
        // Loud, never a silent no-op — this now covers live commands as well as replayed ones.
        tracing::error!(
            target: "shadow_scale::command",
            command = command_label,
            faction = %faction.0,
            band_id,
            "command.band.rejected=no_such_band"
        );
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!("Band {band_id} does not exist in the simulation."),
        );
        return None;
    };
    if app.world.get::<StartingUnit>(entity).is_none() {
        warn!(
            target: "shadow_scale::command",
            command = command_label,
            faction = %faction.0,
            band_id,
            "command.band.rejected=not_a_starting_unit"
        );
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!("Band {band_id} is not a commandable unit."),
        );
        return None;
    }
    let owner = app.world.get::<PopulationCohort>(entity).map(|c| c.faction);
    if owner != Some(faction) {
        warn!(
            target: "shadow_scale::command",
            command = command_label,
            faction = %faction.0,
            band_id,
            "command.band.rejected=wrong_faction"
        );
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!("Band {band_id} belongs to another faction."),
        );
        return None;
    }
    Some(entity)
}

struct SelectedBand {
    entity: Entity,
    label: String,
}

fn select_starting_band(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_id: Option<u64>,
    command_label: &str,
    event_kind: CommandEventKind,
) -> Option<SelectedBand> {
    if let Some(bits) = band_id {
        let entity = resolve_starting_unit_entity(app, faction, bits, command_label, event_kind)?;
        return Some(SelectedBand {
            entity,
            label: starting_unit_label(app, entity),
        });
    }

    // Default-band picker: only ever auto-grab a real band (`With<ResidentBand>`) so a band-less
    // command never silently commandeers a detached expedition (which keeps `StartingUnit`).
    let mut query = app
        .world
        .query_filtered::<(Entity, &PopulationCohort, &StartingUnit), With<ResidentBand>>();
    for (entity, cohort, unit) in query.iter(&app.world) {
        if cohort.faction == faction {
            return Some(SelectedBand {
                entity,
                label: unit.kind.clone(),
            });
        }
    }

    warn!(
        target: "shadow_scale::command",
        command = command_label,
        faction = %faction.0,
        "command.starting_unit.rejected=none_available"
    );
    emit_command_failure(
        app,
        event_kind,
        faction,
        "No available bands can accept this order right now.",
    );
    None
}

fn select_founder_band(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    event_kind: CommandEventKind,
) -> Option<SelectedBand> {
    // Founders picker: real bands only (`With<ResidentBand>`) — an expedition can never found.
    let mut query = app
        .world
        .query_filtered::<(Entity, &PopulationCohort, &StartingUnit), With<ResidentBand>>();
    for (entity, cohort, unit) in query.iter(&app.world) {
        if cohort.faction == faction && unit.kind.eq_ignore_ascii_case("founders") {
            return Some(SelectedBand {
                entity,
                label: unit.kind.clone(),
            });
        }
    }

    emit_command_failure(
        app,
        event_kind,
        faction,
        "No Founders unit is available to found a settlement.",
    );
    None
}

fn starting_unit_label(app: &bevy::prelude::App, entity: Entity) -> String {
    app.world
        .get::<StartingUnit>(entity)
        .map(|unit| unit.kind.clone())
        .unwrap_or_else(|| format!("starting_unit:{}", entity.index()))
}

/// Charge a provisions cost from the faction's bands' local larders (food is band-local now — the
/// supply network keeps networked bands topped up). Sums the faction's carried food; on shortfall
/// rejects with a command-feed failure, otherwise draws the cost greedily across bands in a
/// deterministic order.
fn consume_faction_provisions(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    amount: i64,
    command_label: &str,
    event_kind: CommandEventKind,
) -> bool {
    if amount <= 0 {
        return true;
    }
    let mut bands: Vec<(Entity, Scalar)> = Vec::new();
    {
        let mut query = app.world.query::<(Entity, &PopulationCohort)>();
        for (entity, cohort) in query.iter(&app.world) {
            if cohort.faction == faction {
                bands.push((entity, cohort.stores.get(FOOD)));
            }
        }
    }
    bands.sort_by_key(|(entity, _)| entity.to_bits());
    let available = bands
        .iter()
        .fold(Scalar::from_i64(0), |acc, (_, food)| acc + *food);
    let cost = Scalar::from_i64(amount);
    if available < cost {
        warn!(
            target: "shadow_scale::command",
            command = command_label,
            faction = %faction.0,
            item = "provisions",
            required = amount,
            available = available.to_i64_whole(),
            "command.inventory.rejected=insufficient"
        );
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!(
                "{} provisions required but only {} available.",
                amount,
                available.to_i64_whole()
            ),
        );
        return false;
    }
    let mut remaining = cost;
    for (entity, _) in bands {
        if remaining <= Scalar::from_i64(0) {
            break;
        }
        if let Some(mut cohort) = app.world.get_mut::<PopulationCohort>(entity) {
            remaining -= cohort.stores.take(FOOD, remaining);
        }
    }
    true
}

fn push_command_event(
    app: &mut bevy::prelude::App,
    tick: u64,
    kind: CommandEventKind,
    faction: FactionId,
    label: String,
    detail: Option<String>,
) {
    if let Some(mut log) = app.world.get_resource_mut::<CommandEventLog>() {
        log.push(CommandEventEntry::new(tick, kind, faction, label, detail));
    }
}

fn emit_command_failure(
    app: &mut bevy::prelude::App,
    kind: CommandEventKind,
    faction: FactionId,
    detail: impl Into<String>,
) {
    let tick = app.world.resource::<SimulationTick>().0;
    let summary = format!("{} failed", command_kind_display(kind));
    push_command_event(app, tick, kind, faction, summary, Some(detail.into()));
}

fn command_kind_display(kind: CommandEventKind) -> &'static str {
    match kind {
        CommandEventKind::Scout => "Scout",
        CommandEventKind::FollowHerd => "Hunt (ongoing)",
        CommandEventKind::FoundSettlement => "Found settlement",
        CommandEventKind::CampaignFounded => "Campaign founded",
        CommandEventKind::CampaignMilestone => "Campaign milestone",
        CommandEventKind::CampaignVictory => "Campaign victory",
        CommandEventKind::Forage => "Harvest",
        CommandEventKind::Hunt => "Hunt",
        CommandEventKind::Tame => "Tame",
        CommandEventKind::Cultivate => "Cultivate",
        CommandEventKind::Sow => "Sow",
        CommandEventKind::Corral => "Corral",
        CommandEventKind::HuntDanger => "Dangerous hunt",
        CommandEventKind::PredatorRaid => "Predator raid",
        CommandEventKind::CancelOrder => "Cancel order",
        CommandEventKind::SedentarizationPrompt => "Sedentarization",
        CommandEventKind::SiteDiscovered => "Site discovered",
        CommandEventKind::NarrativeBeat => "The Telling",
        CommandEventKind::NarrativeFork => "The Telling",
        CommandEventKind::ExpeditionSent => "Expedition sent",
        CommandEventKind::ExpeditionArrived => "Expedition arrived",
        CommandEventKind::ExpeditionRecalled => "Expedition recalled",
        CommandEventKind::ExpeditionReturned => "Expedition returned",
        CommandEventKind::HerdUnderHerded => "Under-herded",
    }
}

// `entity_from_bits` lived here. It is deleted rather than left unused, and the deletion is the
// guard: **no command may turn a wire value into an `Entity`.** Every band handle on the wire is a
// `BandId` now, and the one function that could reinterpret a wire `u64` as an ECS index/generation
// pair was the last place `recall_expedition` silently no-op'd from. Its absence is what stops that
// coming back — a reintroduced caller has to reintroduce the function first, which is a visible act.

fn log_tile_rejection(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    coords: UVec2,
    command_label: &str,
    reason: &str,
    event_kind: Option<CommandEventKind>,
) {
    warn!(
        target: "shadow_scale::command",
        command = command_label,
        faction = %faction.0,
        x = coords.x,
        y = coords.y,
        reason,
        "command.tile_validation.failed"
    );
    if let Some(kind) = event_kind {
        let human_reason = describe_tile_rejection(reason);
        let detail = format!(
            "Tile ({}, {}): {} ({})",
            coords.x, coords.y, human_reason, reason
        );
        emit_command_failure(app, kind, faction, detail);
    }
}

fn describe_tile_rejection(reason: &str) -> &'static str {
    match reason {
        "out_of_bounds" => "Destination is outside the playable area",
        "tile_missing" => "Tile data is unavailable",
        "water_tile" => "Cannot perform this action on a water tile",
        "no_food_module" => "Tile lacks a harvestable food source",
        "module_mismatch" => "Food source does not match the requested type",
        "no_yield" => "This site has no remaining seasonal yield",
        _ => "Tile is not valid for this command",
    }
}

/// What a rollback replays: the origin world, and every event since.
///
/// **A checkpoint is a cache; this is the thing it caches.** The world at tick N is a pure function
/// of `(origin) + (the ordered commands and turn boundaries since)`, so the log *is* the authority
/// and a materialized world is only ever an optimisation over it. The previous design had the cache
/// without the authority, which produced three defects in a row: a replay that skipped commands, a
/// per-command checkpoint bolted on to paper over it, and a ring whose 16 slots an active player
/// could evict in 16 ticks — silently cutting a 256-turn rollback window down to "the last few
/// things you touched".
///
/// Rollback is undo/redo, and undo/redo is a log. A command is a few hundred bytes against a full
/// world clone, and commands are human-paced.
///
/// **Bit-exact forward replay is the precondition**, and it is the thing this arc established and
/// proved: `a_restored_world_simulates_forward_identically` and
/// `checkpoint_restore_is_lossless` are what make replaying the log land on the world that happened
/// rather than one that merely looks like it.
struct CommandLog {
    /// The world this log replays from, captured once when the world was created or re-based.
    origin: std::sync::Arc<core_sim::sim_state::SimState>,
    origin_tick: u64,
    entries: Vec<LogEntry>,
}

/// One replayable event. A turn carries no data — `run_turn` is a pure function of the world.
#[derive(Debug, Clone)]
enum LogEntry {
    Turn,
    Command(Command),
}

impl CommandLog {
    fn new(app: &bevy::prelude::App) -> Self {
        Self {
            origin: std::sync::Arc::new(core_sim::sim_state::capture_sim_state(&app.world)),
            origin_tick: app.world.resource::<SimulationTick>().0,
            entries: Vec::new(),
        }
    }

    /// Re-base: this world is a new starting point and nothing before it is reachable.
    ///
    /// `new_game`, `reset_map` and **every config reload** land here. The reload is the interesting
    /// one and it is the deliberate answer to a hole this arc flagged early: a `SimState` carries no
    /// config *by design*, so replaying across a reload would run turns under whatever tuning is
    /// live rather than the tuning of that tick. Re-basing is consistent with that decision and
    /// needs no config serialization at all.
    fn rebase(&mut self, app: &bevy::prelude::App, reason: &str) {
        *self = Self::new(app);
        warn!(
            target: "shadow_scale::server",
            reason,
            origin_tick = self.origin_tick,
            "rollback.origin_rebased -- rollback can no longer reach before this point"
        );
    }

    fn push(&mut self, entry: LogEntry) {
        self.entries.push(entry);
    }

    /// How many entries reproduce the world at `tick`, or `None` if it is out of reach.
    ///
    /// **"The world at tick N" means immediately after the Nth turn resolved** — before any command
    /// issued while sitting at that tick. That has to be pinned down rather than left to fall out of
    /// the loop: a command lands *between* turns, so "at tick N" is otherwise ambiguous about
    /// whether the commands issued at N are in or out, and the two readings give different worlds.
    fn prefix_len_for(&self, tick: u64) -> Option<usize> {
        if tick < self.origin_tick {
            return None;
        }
        let mut turns = self.origin_tick;
        if turns == tick {
            return Some(0);
        }
        for (index, entry) in self.entries.iter().enumerate() {
            if matches!(entry, LogEntry::Turn) {
                turns += 1;
                if turns == tick {
                    return Some(index + 1);
                }
            }
        }
        None
    }
}

/// Apply one **replayable** command.
///
/// Every arm here is a pure function of `(world, command)` plus publication — which is what lets a
/// rollback re-apply them from the log and land on the world that actually happened.
///
/// The four commands that are *not* here stay in the dispatch loop because they are not replayable:
/// `Turn` is a turn (the log records it as `LogEntry::Turn`, not as a command), and `ResetMap` /
/// `NewGame` / a config reload **re-base the origin** — they replace the world or the tuning it runs
/// under, so there is nothing before them to replay from. `ResetMap` additionally reassigns the
/// `App` itself, which a `&mut App` parameter could not do; that the un-extractable arms are exactly
/// the un-replayable ones is the cut falling out of the design rather than being imposed on it.
/// Record a dispatched command in the log, unless it is one of the two that must not be.
///
/// **This is the one place the exclusion policy lives**, and the dispatch loop and the tests both go
/// through it. They used to disagree: the loop applied the policy inline while the tests pushed
/// `LogEntry::Command` by hand, so the tests exercised the replay mechanism while the wiring that
/// feeds it in a running server was covered by nothing — delete the loop's push and every test still
/// passed. That is the same shape as an oracle that issues no commands, one layer out.
///
/// `Rollback` is excluded because logging it would make a replay re-enter the rollback it is
/// serving. A config reload is excluded because it is not replayable at all — a `SimState` carries
/// no config by design, so replaying across one would run turns under whatever tuning is live rather
/// than that tick's; it re-bases the origin instead. The staged config overrides are excluded for
/// the *same* reason from the other end: they only ever change what the **next** `new_game` boots
/// on, so a replay of this timeline that re-installed them would re-do file writes to change
/// nothing about the world being replayed.
fn log_dispatched_command(log: &mut CommandLog, command: &Command) {
    let unreplayable = matches!(
        command,
        Command::ReloadConfig { .. }
            | Command::Rollback { .. }
            | Command::SetConfigOverride { .. }
            | Command::ClearConfigOverrides
    );
    if !unreplayable {
        log.push(LogEntry::Command(command.clone()));
    }
}

fn apply_command(app: &mut bevy::prelude::App, command: Command, flat_server: &SnapshotServer) {
    match command {
        Command::ExportMap { path } => {
            write_map_export(app, path);
        }
        // Staged config tuning. Takes NO `app`: it changes what the next `new_game` boots on and
        // nothing about the world running now — which is also why the client states the
        // next-New-Game contract on the panel itself.
        Command::SetConfigOverride { kind, patch_json } => {
            handle_set_config_override(kind, &patch_json);
        }
        Command::ClearConfigOverrides => {
            clear_config_overrides();
            info!(
                target: "shadow_scale::server",
                "config_override.cleared"
            );
        }
        // Republish the world as a FULL frame. The client asks for this when it cannot apply a
        // delta (`docs/plan_delta_streaming.md` §3.3), so the answer must be a complete world
        // rather than another delta — a delta is what it just failed to use.
        //
        // Client-INITIATED, which is what makes this safe against the world-handoff rule: the
        // server never volunteers a frame to a connecting client (it might belong to a world
        // that client did not ask for), but answering a request cannot surprise anyone, and the
        // `worldEpoch` on the frame still lets the client reject a world it did not want.
        //
        // It republishes through `publish_full_frame` rather than encoding the ring entry as
        // stored, because the answer must carry a LIVE sequence number: resync is the recovery
        // path, so a stale number here reopens the sequence gap the client asked us to close.
        Command::Resync => {
            let mut history = app.world.resource_mut::<SnapshotHistory>();
            match history.latest_entry() {
                Some(entry) => {
                    let bytes = history.publish_full_frame(&entry);
                    flat_server.broadcast(&bytes);
                    info!(
                        target: "shadow_scale::server",
                        tick = entry.tick,
                        bytes = bytes.len(),
                        "resync.published"
                    );
                }
                None => {
                    // No world yet (the server boots idle). Nothing to republish; the client's
                    // `new_game` retry is what recovers this case.
                    info!(target: "shadow_scale::server", "resync.no_world");
                }
            }
        }
        Command::Heat {
            target_x,
            target_y,
            delta,
        } => {
            apply_heat(app, UVec2::new(target_x, target_y), delta);
            info!(
                target: "shadow_scale::server",
                target_x,
                target_y,
                delta,
                "command.applied=heat"
            );
        }
        Command::Orders { faction, orders } => {
            handle_order_submission(app, faction, orders);
        }
        Command::AxisBias { axis, value } => {
            handle_axis_bias(app, axis, value, flat_server);
        }
        Command::SupportInfluencer { id, magnitude } => {
            handle_influencer_command(app, id, magnitude, InfluencerAction::Support, flat_server);
        }
        Command::SuppressInfluencer { id, magnitude } => {
            handle_influencer_command(app, id, magnitude, InfluencerAction::Suppress, flat_server);
        }
        Command::SupportInfluencerChannel {
            id,
            channel,
            magnitude,
        } => {
            handle_influencer_channel_support(app, id, channel, magnitude, flat_server);
        }
        Command::SpawnInfluencer { scope, generation } => {
            handle_influencer_spawn(app, scope, generation, flat_server);
        }
        Command::InjectCorruption {
            subsystem,
            intensity,
            exposure_timer,
        } => {
            handle_inject_corruption(app, subsystem, intensity, exposure_timer, flat_server);
        }
        Command::UpdateEspionageGenerators { updates } => {
            handle_update_espionage_generators(app, updates);
        }
        Command::QueueEspionageMission { params } => {
            handle_queue_espionage_mission(app, params);
        }
        Command::UpdateEspionageQueueDefaults {
            scheduled_tick_offset,
            target_tier,
        } => {
            handle_update_queue_defaults(app, scheduled_tick_offset, target_tier);
        }
        Command::UpdateCounterIntelPolicy { faction, policy } => {
            handle_update_counter_intel_policy(app, faction, policy);
        }
        Command::AdjustCounterIntelBudget {
            faction,
            reserve,
            delta,
        } => {
            handle_adjust_counter_intel_budget(app, faction, reserve, delta);
        }
        Command::ReloadConfig { kind, path } => {
            handle_reload_config(app, kind, path);
        }
        Command::SetCrisisAutoSeed { enabled } => {
            {
                let mut config_res = app.world.resource_mut::<SimulationConfig>();
                config_res.crisis_auto_seed = enabled;
            }
            info!(
                target: "shadow_scale::server",
                enabled,
                "crisis.autoseed.updated"
            );
        }
        Command::SetFogEnabled { enabled } => {
            {
                let mut config_res = app.world.resource_mut::<SimulationConfig>();
                config_res.fog_enabled = enabled;
            }
            info!(
                target: "shadow_scale::server",
                enabled,
                "fog.of_war.updated"
            );
        }
        Command::SpawnCrisis {
            faction,
            archetype_id,
        } => {
            let archetype = archetype_id.clone();
            {
                let mut spawns = app.world.resource_mut::<PendingCrisisSpawns>();
                spawns.push(faction, archetype);
            }
            info!(
                target: "shadow_scale::server",
                faction = %faction.0,
                archetype = %archetype_id,
                "crisis.spawn.enqueued"
            );
        }
        Command::SetStartProfile { profile_id } => {
            handle_set_start_profile(app, profile_id);
        }
        Command::AssignLabor {
            faction,
            band_id,
            role,
            workers,
            target_x,
            target_y,
            fauna_id,
            species,
            floor,
        } => {
            handle_assign_labor(
                app, faction, band_id, role, workers, target_x, target_y, fauna_id, species, floor,
            );
        }
        Command::MoveBand {
            faction,
            band_id,
            target_x,
            target_y,
        } => {
            handle_move_band(app, faction, band_id, target_x, target_y);
        }
        Command::SendExpedition {
            faction,
            band_id,
            party_workers,
            target_x,
            target_y,
        } => {
            handle_send_expedition(app, faction, band_id, party_workers, target_x, target_y);
        }
        Command::RecallExpedition {
            faction,
            expedition_band_id,
        } => {
            handle_recall_expedition(app, faction, expedition_band_id);
        }
        Command::SendHuntExpedition {
            faction,
            band_id,
            party_workers,
            fauna_id,
            floor,
        } => {
            handle_send_hunt_expedition(app, faction, band_id, party_workers, fauna_id, floor);
        }
        Command::FoundSettlement {
            faction,
            target_x,
            target_y,
        } => {
            handle_found_settlement(app, faction, target_x, target_y);
        }
        Command::Tame { faction, herd_id } => {
            handle_tame(app, faction, herd_id);
        }
        Command::AnswerFork {
            faction,
            beat_id,
            choice_id,
        } => {
            handle_answer_fork(app, faction, beat_id, choice_id);
        }
        Command::Cultivate {
            faction,
            target_x,
            target_y,
        } => {
            handle_cultivate(app, faction, UVec2::new(target_x, target_y));
        }
        Command::Sow {
            faction,
            target_x,
            target_y,
        } => {
            handle_sow(app, faction, UVec2::new(target_x, target_y));
        }
        Command::Corral {
            faction,
            target_x,
            target_y,
        } => {
            handle_corral(app, faction, UVec2::new(target_x, target_y));
        }
        Command::AbandonImprovement {
            faction,
            kind,
            target_x,
            target_y,
            fauna_id,
        } => {
            handle_abandon_improvement(
                app,
                faction,
                kind,
                UVec2::new(target_x, target_y),
                fauna_id,
            );
        }
        Command::ExtendPen {
            faction,
            target_x,
            target_y,
        } => {
            handle_extend_pen(app, faction, UVec2::new(target_x, target_y));
        }
        Command::CancelOrder {
            faction,
            band_id,
            scope,
        } => {
            handle_cancel_order(app, faction, band_id, scope);
        }
        // The four non-replayable commands never reach here; the dispatch loop handles them.
        Command::Turn(_)
        | Command::ResetMap { .. }
        | Command::NewGame { .. }
        | Command::Rollback { .. } => {
            unreachable!("turn, world-rebuilding and rollback commands are handled in the loop")
        }
    }
}

/// Re-capture a fresh world snapshot (current ECS state, **including** the command-event feed) and
/// broadcast it — WITHOUT advancing the turn or pushing a rollback ring entry. Runs after every
/// dispatched command so a world-mutating command (expedition launch, `move_band`, `assign_labor`,
/// …) is reflected in the client's snapshot immediately, not only after the next turn resolves.
/// Toggles `SnapshotCaptureMode::refresh_in_place` so `capture_snapshot` refreshes the latest
/// broadcast + back ring entry in place instead of recording a new ring entry. Re-capturing on a
/// genuinely non-mutating command is merely slightly wasteful (commands are human-issued, low
/// frequency) — the robust uniform path, no hand-curated "which commands mutate" list.
fn recapture_and_broadcast(app: &mut bevy::prelude::App) {
    // The recapture rides the same publisher queue as a turn does, so it publishes exactly what a
    // turn publishes — the flat delta, or the flat full frame when this world has one pending.
    // There is no second, serial path: `capture_snapshot` hands the world over and returns, here as
    // on the turn (#393).
    recapture_snapshot_in_place(&mut app.world);
}

fn handle_order_submission(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    orders: FactionOrders,
) {
    let order_count = orders.orders.len();
    let result = {
        let mut queue = app.world.resource_mut::<TurnQueue>();
        queue.submit_orders(faction, orders)
    };

    match result {
        Ok(SubmitOutcome::Accepted { remaining }) => info!(
            target: "shadow_scale::server",
            %faction,
            order_count,
            remaining,
            "orders.accepted"
        ),
        Ok(SubmitOutcome::ReadyToResolve) => {
            info!(
                target: "shadow_scale::server",
                %faction,
                order_count,
                "orders.ready_to_resolve"
            );
            resolve_ready_turn(app);
        }
        Err(SubmitError::UnknownFaction(f)) => warn!(
            target: "shadow_scale::server",
            %f,
            "orders.rejected=unknown_faction"
        ),
        Err(SubmitError::DuplicateSubmission(f)) => warn!(
            target: "shadow_scale::server",
            %f,
            "orders.rejected=duplicate_submission"
        ),
    }
}

fn handle_axis_bias(
    app: &mut bevy::prelude::App,
    axis: usize,
    value: f32,
    snapshot_server_flat: &SnapshotServer,
) {
    if axis >= 4 {
        warn!(
            target: "shadow_scale::server",
            axis,
            "axis_bias.rejected=invalid_axis"
        );
        return;
    }

    let clamped = value.clamp(-1.0, 1.0);
    {
        let mut bias_res = app.world.resource_mut::<SentimentAxisBias>();
        bias_res.set_policy_axis(axis, Scalar::from_f32(clamped));
    }

    let bias_state = {
        let bias_res = app.world.resource::<SentimentAxisBias>();
        let raw = bias_res.as_raw();
        AxisBiasState {
            knowledge: raw[0],
            trust: raw[1],
            equity: raw[2],
            agency: raw[3],
        }
    };

    let broadcast_payload = {
        let mut history = app.world.resource_mut::<SnapshotHistory>();
        history.update_axis_bias(bias_state)
    };

    if let Some(flat) = broadcast_payload {
        snapshot_server_flat.broadcast(&flat);
    }

    info!(
        target: "shadow_scale::server",
        axis,
        value = clamped,
        "axis_bias.updated"
    );
}

fn handle_influencer_channel_support(
    app: &mut bevy::prelude::App,
    id: u32,
    channel: SupportChannel,
    magnitude: f32,
    snapshot_server_flat: &SnapshotServer,
) {
    let clamped = magnitude.clamp(0.1, 5.0);
    let scalar_amount = Scalar::from_f32(clamped);
    let applied = {
        let mut roster = app.world.resource_mut::<InfluentialRoster>();
        roster.apply_channel_support(id, channel, scalar_amount)
    };

    if !applied {
        warn!(
            target: "shadow_scale::server",
            id,
            channel = channel.as_str(),
            magnitude = clamped,
            "influencer.channel_support.rejected=unknown_id"
        );
        return;
    }

    broadcast_influencer_update(app, snapshot_server_flat);

    info!(
        target: "shadow_scale::server",
        id,
        channel = channel.as_str(),
        magnitude = clamped,
        "influencer.channel_support.applied"
    );
}

fn handle_influencer_spawn(
    app: &mut bevy::prelude::App,
    scope: Option<InfluenceScopeKind>,
    generation: Option<GenerationId>,
    snapshot_server_flat: &SnapshotServer,
) {
    let registry_snapshot = app.world.resource::<GenerationRegistry>().clone();
    // The spawn's draw is derived from `(seed, tick, id)`, so the command needs the tick it lands
    // on — a manual spawn is reproducible on replay exactly like an organic one.
    let tick = app.world.resource::<SimulationTick>().0;
    let spawned = {
        let mut roster = app.world.resource_mut::<InfluentialRoster>();
        roster.force_spawn(scope, generation, &registry_snapshot, tick)
    };

    let Some(new_id) = spawned else {
        warn!(
            target: "shadow_scale::server",
            scope = ?scope,
            generation = ?generation,
            "influencer.spawn.rejected"
        );
        return;
    };

    broadcast_influencer_update(app, snapshot_server_flat);

    let label = {
        let roster = app.world.resource::<InfluentialRoster>();
        roster
            .states()
            .into_iter()
            .find(|state| state.id == new_id)
            .map(|state| state.name)
            .unwrap_or_else(|| "unknown".to_string())
    };

    info!(
        target: "shadow_scale::server",
        id = new_id,
        scope = ?scope,
        generation = ?generation,
        name = label.as_str(),
        "influencer.spawn.manual"
    );
}

fn broadcast_influencer_update(
    app: &mut bevy::prelude::App,
    snapshot_server_flat: &SnapshotServer,
) {
    let (states, sentiment_totals, logistics_total, morale_total, power_total) = {
        let roster = app.world.resource::<InfluentialRoster>();
        (
            roster.states(),
            roster.sentiment_totals(),
            roster.logistics_total(),
            roster.morale_total(),
            roster.power_total(),
        )
    };

    {
        let mut impacts = app.world.resource_mut::<InfluencerImpacts>();
        impacts.set_from_totals(logistics_total, morale_total, power_total);
    }

    {
        let mut bias_res = app.world.resource_mut::<SentimentAxisBias>();
        bias_res.set_influencer(sentiment_totals);
    }

    let bias_state = {
        let bias_res = app.world.resource::<SentimentAxisBias>();
        let raw = bias_res.as_raw();
        AxisBiasState {
            knowledge: raw[0],
            trust: raw[1],
            equity: raw[2],
            agency: raw[3],
        }
    };

    let (influencer_delta, bias_delta) = {
        let mut history = app.world.resource_mut::<SnapshotHistory>();
        let influencer_delta = history.update_influencers(states);
        let bias_delta = history.update_axis_bias(bias_state);
        (influencer_delta, bias_delta)
    };

    if let Some(flat) = influencer_delta {
        snapshot_server_flat.broadcast(&flat);
    }
    if let Some(flat) = bias_delta {
        snapshot_server_flat.broadcast(&flat);
    }
}

fn handle_influencer_command(
    app: &mut bevy::prelude::App,
    id: u32,
    magnitude: f32,
    action: InfluencerAction,
    snapshot_server_flat: &SnapshotServer,
) {
    let clamped = magnitude.clamp(0.1, 5.0);
    let scalar_amount = Scalar::from_f32(clamped);

    let applied = {
        let mut roster = app.world.resource_mut::<InfluentialRoster>();
        match action {
            InfluencerAction::Support => roster.apply_support(id, scalar_amount),
            InfluencerAction::Suppress => roster.apply_suppress(id, scalar_amount),
        }
    };

    if !applied {
        warn!(
            target: "shadow_scale::server",
            id,
            magnitude = clamped,
            "influencer.command.rejected=unknown_id"
        );
        return;
    }

    broadcast_influencer_update(app, snapshot_server_flat);

    match action {
        InfluencerAction::Support => info!(
            target: "shadow_scale::server",
            id,
            magnitude = clamped,
            "influencer.support.applied"
        ),
        InfluencerAction::Suppress => info!(
            target: "shadow_scale::server",
            id,
            magnitude = clamped,
            "influencer.suppress.applied"
        ),
    }
}

fn handle_inject_corruption(
    app: &mut bevy::prelude::App,
    subsystem: CorruptionSubsystem,
    intensity: f32,
    exposure_timer: u16,
    snapshot_server_flat: &SnapshotServer,
) {
    let clamped_intensity = intensity.clamp(-5.0, 5.0);
    let timer = exposure_timer.max(1);
    let restitution = timer.saturating_add(4);
    let tick = app.world.resource::<SimulationTick>().0;

    let (ledger_clone, incident_id) = {
        let mut ledgers = app.world.resource_mut::<CorruptionLedgers>();
        let ledger = ledgers.ledger_mut();
        let incident_id = (tick << 32) | (((ledger.entry_count() as u64) + 1) & 0xFFFF_FFFF);
        let entry = CorruptionEntry {
            subsystem,
            intensity: Scalar::from_f32(clamped_intensity).raw(),
            incident_id,
            exposure_timer: timer,
            restitution_window: restitution,
            last_update_tick: tick,
        };
        ledger.register_incident(entry);
        (ledger.clone(), incident_id)
    };

    let delta_payload = {
        let mut history = app.world.resource_mut::<SnapshotHistory>();
        history.update_corruption(ledger_clone)
    };

    if let Some(flat) = delta_payload {
        snapshot_server_flat.broadcast(&flat);
    }

    info!(
        target: "shadow_scale::server",
        ?subsystem,
        intensity = clamped_intensity,
        exposure_timer = timer,
        incident_id,
        "corruption.injected"
    );
}

fn handle_update_espionage_generators(
    app: &mut bevy::prelude::App,
    updates: Vec<CommandGeneratorUpdate>,
) {
    if updates.is_empty() {
        info!(
            target: "shadow_scale::espionage",
            "espionage.generator.update_skipped=no_updates"
        );
        return;
    }

    let factions: Vec<FactionId> = {
        let registry = app.world.resource::<FactionRegistry>();
        registry.factions.clone()
    };

    let mut catalog = app.world.resource_mut::<EspionageCatalog>();
    let mut changed = false;

    for update in updates {
        let template_id = update.template_id;
        let enabled = update.enabled;
        let per_faction = update.per_faction;
        let applied = catalog.update_agent_generator(template_id.as_str(), enabled, per_faction);
        if applied {
            changed = true;
            info!(
                target: "shadow_scale::espionage",
                template_id,
                enabled = ?enabled,
                per_faction = ?per_faction,
                "espionage.generator.updated"
            );
        } else {
            warn!(
                target: "shadow_scale::espionage",
                template_id,
                "espionage.generator.update_failed=unknown_template"
            );
        }
    }
    if !changed {
        return;
    }

    app.world
        .resource_scope(|world, mut roster: bevy::prelude::Mut<EspionageRoster>| {
            let catalog = world.resource::<EspionageCatalog>();
            roster.refresh_generated_agents(catalog, &factions);
        });

    info!(
        target: "shadow_scale::espionage",
        factions = factions.len(),
        "espionage.generators.reseeded"
    );
}

fn handle_update_queue_defaults(
    app: &mut bevy::prelude::App,
    scheduled_tick_offset: Option<u64>,
    target_tier: Option<u8>,
) {
    if scheduled_tick_offset.is_none() && target_tier.is_none() {
        info!(
            target: "shadow_scale::espionage",
            "espionage.queue_defaults.update_skipped=no_fields"
        );
        return;
    }

    let mut catalog = app.world.resource_mut::<EspionageCatalog>();
    let mut defaults = catalog.config().queue_defaults().clone();

    if let Some(offset) = scheduled_tick_offset {
        defaults.scheduled_tick_offset = offset;
    }

    if target_tier.is_some() {
        defaults.target_tier = target_tier;
    }

    catalog.update_queue_defaults(defaults.clone());

    info!(
        target: "shadow_scale::espionage",
        scheduled_tick_offset = defaults.scheduled_tick_offset,
        target_tier = ?defaults.target_tier,
        "espionage.queue_defaults.updated"
    );
}

fn handle_update_counter_intel_policy(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    policy: SecurityPolicy,
) {
    let mut policies = app.world.resource_mut::<FactionSecurityPolicies>();
    policies.set_policy(faction, policy);
    info!(
        target: "shadow_scale::espionage",
        %faction,
        ?policy,
        "counter_intel.policy.updated"
    );
}

fn handle_adjust_counter_intel_budget(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    reserve: Option<Scalar>,
    delta: Option<Scalar>,
) {
    if reserve.is_none() && delta.is_none() {
        warn!(
            target: "shadow_scale::espionage",
            %faction,
            "counter_intel_budget.adjust.noop"
        );
        return;
    }

    let budget_config = {
        let catalog = app.world.resource::<EspionageCatalog>();
        catalog.config().counter_intel_budget().clone()
    };

    let mut budgets = app.world.resource_mut::<CounterIntelBudgets>();
    let mut updated = budgets.available(faction);

    if let Some(value) = reserve {
        updated = budgets.set_reserve(faction, value, &budget_config);
    }
    if let Some(value) = delta {
        updated = budgets.adjust_reserve(faction, value, &budget_config);
    }

    info!(
        target: "shadow_scale::espionage",
        %faction,
        reserve = reserve.map(|v| v.to_f32()),
        delta = delta.map(|v| v.to_f32()),
        available = updated.to_f32(),
        "counter_intel_budget.adjusted"
    );
}

const AUTO_AGENT_HANDLE: u32 = u32::MAX;

fn handle_queue_espionage_mission(app: &mut bevy::prelude::App, mut params: QueueMissionParams) {
    let current_tick = app.world.resource::<SimulationTick>().0;
    let defaults = {
        let catalog = app.world.resource::<EspionageCatalog>();
        catalog.config().queue_defaults().clone()
    };

    if params.scheduled_tick == 0 {
        params.scheduled_tick = current_tick.saturating_add(defaults.scheduled_tick_offset);
    }

    if params.target_tier.is_none() {
        params.target_tier = defaults.target_tier;
    }

    let mission_id = params.mission_id.0.clone();
    let owner = params.owner.0;
    let target_owner = params.target_owner.0;
    let auto_agent_requested = params.agent.0 == AUTO_AGENT_HANDLE;
    let mut selected_agent = params.agent;

    let queue_result = app.world.resource_scope(
        |world, mut missions: bevy::prelude::Mut<EspionageMissionState>| {
            let mut queued_params = params.clone();
            world.resource_scope(|world, mut roster: bevy::prelude::Mut<EspionageRoster>| {
                let catalog = world.resource::<EspionageCatalog>();

                if queued_params.agent.0 == AUTO_AGENT_HANDLE {
                    let template = match catalog.mission(&queued_params.mission_id) {
                        Some(template) => template,
                        None => {
                            return Err(QueueMissionError::UnknownMission(
                                queued_params.mission_id.0.clone(),
                            ));
                        }
                    };

                    let Some(handle) =
                        pick_best_agent_for_mission(&roster, queued_params.owner, template)
                    else {
                        return Err(QueueMissionError::NoAgentAvailable {
                            faction: queued_params.owner,
                        });
                    };

                    queued_params.agent = handle;
                }

                selected_agent = queued_params.agent;
                missions.queue_mission(catalog, &mut roster, queued_params)
            })
        },
    );

    match queue_result {
        Ok(instance_id) => {
            info!(
                target: "shadow_scale::espionage",
                mission_id,
                owner_faction = owner,
                target_owner,
                discovery_id = params.discovery_id,
                agent_handle = selected_agent.0,
                target_tier = ?params.target_tier,
                scheduled_tick = params.scheduled_tick,
                instance = instance_id.0,
                auto_agent = auto_agent_requested,
                "espionage.mission.queued"
            );
        }
        Err(error) => {
            warn!(
                target: "shadow_scale::espionage",
                mission_id,
                owner_faction = owner,
                target_owner,
                discovery_id = params.discovery_id,
                agent_handle = selected_agent.0,
                target_tier = ?params.target_tier,
                scheduled_tick = params.scheduled_tick,
                %error,
                "espionage.mission.queue_failed"
            );
        }
    }
}

fn pick_best_agent_for_mission(
    roster: &EspionageRoster,
    faction: FactionId,
    template: &EspionageMissionTemplate,
) -> Option<EspionageAgentHandle> {
    let mut best: Option<(EspionageAgentHandle, f32)> = None;

    for agent in roster.agents_for(faction) {
        if agent.assignment != AgentAssignment::Available {
            continue;
        }

        let score = match template.kind {
            EspionageMissionKind::Probe => {
                agent.stealth.to_f32() * template.stealth_weight.to_f32()
                    + agent.recon.to_f32() * template.recon_weight.to_f32()
            }
            EspionageMissionKind::CounterIntel => {
                agent.counter_intel.to_f32() * template.counter_intel_weight.to_f32()
            }
        };

        let is_better = match &best {
            Some((_, best_score)) => score > *best_score,
            None => true,
        };

        if is_better {
            best = Some((agent.handle, score));
        }
    }

    best.map(|(handle, _)| handle)
}

/// Resolve one turn the way the `Turn` command does — auto-submitting for any faction still
/// awaited, then resolving.
///
/// **Both the live path and replay go through here**, because a `LogEntry::Turn` has to reproduce
/// what the turn actually did. `resolve_ready_turn` *skips* when `TurnQueue` is not ready, so a
/// replay that omitted the auto-submit would silently resolve fewer turns than the original and
/// land on the wrong tick.
fn resolve_turn_with_auto_orders(app: &mut bevy::prelude::App) {
    {
        let mut queue = app.world.resource_mut::<TurnQueue>();
        let awaiting = queue.awaiting();
        for faction in &awaiting {
            info!(
                target: "shadow_scale::server",
                %faction,
                "orders.auto_generated=end_turn"
            );
        }
        queue.force_submit_all(|_| FactionOrders::end_turn());
    }
    resolve_ready_turn(app);
}

fn resolve_ready_turn(app: &mut bevy::prelude::App) {
    let turn_start = std::time::Instant::now();
    // Open the turn's profile here rather than from a stage marker: order application and snapshot
    // broadcast happen outside `app.update()` and belong to the same turn's breakdown.
    turn_profile::begin_turn();
    let ready_orders = {
        let mut queue = app.world.resource_mut::<TurnQueue>();
        if !queue.is_ready() {
            warn!(
                target: "shadow_scale::server",
                awaiting = ?queue.awaiting(),
                "turn.resolve_skipped=awaiting_orders"
            );
            return;
        }
        queue.drain_ready_orders()
    };

    {
        let _s = turn_profile::scope("orders.apply");
        apply_orders(&ready_orders);
    }
    run_turn(app);

    {
        let mut queue = app.world.resource_mut::<TurnQueue>();
        queue.advance_turn();
    }

    // No broadcast step: `capture_snapshot` handed the world to the publisher inside `run_turn`,
    // and the publisher hashes, diffs, encodes and writes it on its own thread (#393). The turn is
    // over here whether or not that frame has reached the socket yet.

    let metrics = app.world.resource::<SimulationMetrics>();
    let duration_ms = turn_start.elapsed().as_secs_f64() * 1000.0;
    info!(
        target: "shadow_scale::server",
        turn = metrics.turn,
        grid_width = metrics.grid_size.0,
        grid_height = metrics.grid_size.1,
        total_mass = metrics.total_mass,
        avg_temp = metrics.avg_temperature,
        duration_ms,
        "turn.completed"
    );
    // The per-phase breakdown of the `duration_ms` just reported. One string field because `tracing`
    // fields must be primitives; see `turn_profile::render` for the format (and note the labels nest
    // flat, so a parent's figure INCLUDES its `parent.child` entries).
    let phases = turn_profile::take();
    info!(
        target: "shadow_scale::server",
        turn = metrics.turn,
        duration_ms,
        phases = %turn_profile::render(&phases),
        "turn.profile"
    );
}

fn apply_orders(submissions: &[(FactionId, FactionOrders)]) {
    for (faction, orders) in submissions {
        info!(
            target: "shadow_scale::server",
            %faction,
            directives = orders.orders.len(),
            "orders.applied"
        );
    }
}

/// Roll the world back to `tick`.
///
/// The world is rebuilt from [`CheckpointHistory`]'s `SimState` — the save state, which carries
/// everything a turn reads — and the client's frame is then **derived from that restored world** by
/// recapturing it, not fetched from a parallel archive. There is one history of worlds, so there is
/// nothing for a second one to disagree with; `a_rollback_produces_the_world_that_tick_had` asserts
/// the result end to end.
fn handle_rollback(
    app: &mut bevy::prelude::App,
    tick: u64,
    snapshot_server_flat: &SnapshotServer,
    log: &mut CommandLog,
) {
    let Some(prefix) = log.prefix_len_for(tick) else {
        warn!(
            target: "shadow_scale::server",
            tick,
            origin_tick = log.origin_tick,
            "rollback.failed=tick_out_of_reach"
        );
        return;
    };

    // Restore the origin, then re-apply the timeline up to `tick`. `Replaying` suppresses both
    // publication and logging: a rollback is ONE publication, and re-logging what it replays would
    // make the log grow every time it was read.
    restore_sim_state(&mut app.world, log.origin.as_ref());
    // `TurnQueue` is server-side order intake and deliberately not checkpoint state, so restoring
    // the origin leaves whatever the *discarded* future put in it. The log's `Orders` entries are
    // what refill it, so it starts empty exactly as it was at the origin — without this a replayed
    // turn can see orders that had not been submitted yet.
    let factions = app.world.resource::<FactionRegistry>().factions.clone();
    app.world.insert_resource(TurnQueue::new(factions));
    app.world.resource_mut::<Replaying>().0 = true;
    let entries: Vec<LogEntry> = log.entries[..prefix].to_vec();
    for entry in entries {
        match entry {
            LogEntry::Turn => resolve_turn_with_auto_orders(app),
            // The log stores the command it was given. Commands address bands by `BandId`, which
            // a world rebuild does not renumber, so there is nothing to translate on the way out.
            LogEntry::Command(command) => apply_command(app, command, snapshot_server_flat),
        }
    }
    app.world.resource_mut::<Replaying>().0 = false;

    // The futures after this point did not happen.
    log.entries.truncate(prefix);

    info!(
        target: "shadow_scale::server",
        tick,
        from = log.origin_tick,
        replayed = prefix,
        "rollback.replayed_from_origin"
    );

    // The client's frame is derived from the world just rebuilt, not fetched from an archive.
    recapture_snapshot_in_place(&mut app.world);
    let entry: Option<StoredSnapshot> = app.world.resource::<SnapshotHistory>().latest_entry();
    let Some(entry) = entry else {
        warn!(
            target: "shadow_scale::server",
            tick,
            "rollback.failed=recapture_produced_no_frame"
        );
        return;
    };

    let flat_frame = {
        let mut history = app.world.resource_mut::<SnapshotHistory>();
        history.reset_to_entry(&entry);
        history.publish_full_frame(&entry)
    };

    warn!(
        target: "shadow_scale::server",
        tick,
        "rollback.completed -- clients should reconnect to receive fresh state"
    );

    snapshot_server_flat.broadcast(&flat_frame);
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::UVec2;
    // The ladder's knowledge ids are named only by the tests now: the handlers resolve their gate
    // off the rung record (`unlock_discovery_id`), never a hard-coded id.
    use core_sim::{
        build_headless_app, default_species_for_rung, EcologyPhase, FoodModule, FoodSiteEntry,
        ForagePatch, CULTIVATION_DISCOVERY_ID, HERDING_DISCOVERY_ID, NO_IMPROVEMENT_UNDERWAY,
        PENNING_DISCOVERY_ID, RUNG_COMPLETE, SEED_SELECTION_DISCOVERY_ID, SITE_ACCEPTED,
    };

    /// Insert a **Thriving, wild** patch — a valid Cultivate target (there is no early claim any
    /// more; progress must be earned with the Cultivate improvement in flight).
    fn seed_thriving_patch(app: &mut bevy::prelude::App, coord: UVec2) {
        seed_gathering_site(app, coord);
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = ForagePatch::new(coord, 100.0);
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);
        registry.patches.insert(coord, patch);
    }

    /// **Put ground under `coord` and make it a GATHERING SITE.**
    ///
    /// Every plant rung carries `requires_gathering_site`, so a fixture that seeds a patch on bare
    /// nothing describes a world the sim cannot produce — `spawn_initial_forage` only ever seeds a
    /// patch on a tile, and worldgen only ever curates a site onto one. Without this, every Forage /
    /// Cultivate / Sow gate in these tests refuses ground the test meant to be workable, and the
    /// failure reads as a broken gate rather than a fixture that never had a map.
    ///
    /// Idempotent in both halves: a fixture that already laid down its own grid keeps it, and a coord
    /// already curated is not curated twice.
    fn seed_gathering_site(app: &mut bevy::prelude::App, coord: UVec2) {
        if !app.world.contains_resource::<TileRegistry>() {
            seed_grid_with_baskets(app, coord.x.max(coord.y).max(PHASE_GATE_GRID - 1) + 1);
        }
        let mut sites = app.world.resource_mut::<FoodSiteRegistry>();
        if sites.is_site(coord) {
            return;
        }
        let module = FoodModule::SavannaGrassland;
        let mut entries = sites.sites().to_vec();
        entries.push(FoodSiteEntry {
            position: coord,
            module,
            kind: module.site_kind(),
            seasonal_weight: 1.0,
        });
        sites.set_sites(entries);
    }

    /// A band of `faction` sitting on tile entity `home` with one labor assignment (the band the
    /// repurposed `cultivate` / `corral` commands check the improvement box on).
    fn spawn_working_band(
        app: &mut bevy::prelude::App,
        faction: FactionId,
        target: LaborTarget,
    ) -> Entity {
        let home = app.world.spawn_empty().id();
        app.world
            .spawn((
                PopulationCohort {
                    home,
                    current_tile: home,
                    size: 30,
                    children: core_sim::scalar_zero(),
                    working: scalar_from_f32(30.0),
                    elders: core_sim::scalar_zero(),
                    stores: LocalStore::new(),
                    morale: core_sim::scalar_one(),
                    last_food_consumption: 0.0,
                    last_morale_delta: core_sim::scalar_zero(),
                    last_morale_cause: Default::default(),
                    last_morale_contributions: Default::default(),
                    last_fertility_factors: Default::default(),
                    discontent_fraction: core_sim::scalar_zero(),
                    grievance: core_sim::scalar_zero(),
                    last_emigrated: 0,
                    last_immigrated: 0,
                    age_turns: 0,
                    generation: 0,
                    faction,
                    knowledge: Vec::new(),
                    migration: None,
                },
                LaborAllocation {
                    assignments: vec![core_sim::LaborAssignment {
                        target,
                        workers: BAND_WORKERS,
                        improvement: None,
                    }],
                    ..Default::default()
                },
            ))
            .id()
    }

    /// [`spawn_working_band`] plus the two markers the **command path** resolves a band through:
    /// `StartingUnit` (the addressable unit) and `ResidentBand` (what `select_starting_band`'s
    /// default picker filters on, so a band-less command never commandeers an expedition). Use this
    /// when the test drives a real `handle_*` command rather than a validator directly.
    fn spawn_resident_working_band(
        app: &mut bevy::prelude::App,
        faction: FactionId,
        target: LaborTarget,
    ) -> Entity {
        let band = spawn_working_band(app, faction, target);
        app.world.entity_mut(band).insert((
            StartingUnit {
                kind: "BandForager".to_string(),
                tags: Vec::new(),
            },
            ResidentBand,
        ));
        band
    }

    /// Workers each test band staffs on its source.
    const BAND_WORKERS: u32 = 5;

    /// The biomass a **stocked patch** fixture is seeded at, as a fraction of `K` — deliberately
    /// **above** Sustain's escapement floor (`fauna::MSY_BIOMASS_FRACTION`, `0.5`), so a Sustain
    /// gather has standing stock to take. `0.5` is the one biomass at which a Sustain row honestly
    /// reads `+0.00` (`docs/plan_harvest_floor.md` §1), which is exactly what these fixtures must not
    /// measure.
    const STOCKED_PATCH_FRACTION: f32 = 0.8;

    /// A snapshot broadcaster bound to an ephemeral loopback port — enough to satisfy the
    /// world-build path's broadcast without a real client.
    fn loopback_snapshot_server() -> Arc<SnapshotServer> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        Arc::new(start_snapshot_server(listener))
    }

    /// **The plant site gates must survive a world that has no tiles.** `Command::AssignLabor` is
    /// dispatched with no `world_active` gate, so an `assign_labor … forage` arriving before
    /// `new_game` reaches `validate_labor_policy` on an `App` that carries no `TileRegistry`. A
    /// panicking accessor there unwinds out of the command loop and takes the server down, so both
    /// plant arms answer permissively instead and leave the labor arm as the authority.
    #[test]
    fn plant_site_gates_do_not_panic_before_a_world_exists() {
        let app = build_headless_app();
        assert!(
            app.world.get_resource::<TileRegistry>().is_none(),
            "idle boot carries no world"
        );
        let faction = FactionId(1);
        let tile = UVec2::new(3, 4);

        assert!(
            plant_rung_site_refusal(&app, RungKey::PlantWild, tile).is_none(),
            "no tiles means no ground to refuse"
        );
        assert!(
            validate_labor_policy(
                &app,
                faction,
                &LaborTarget::Forage {
                    tile,
                    floor: DEFAULT_ESCAPEMENT_FLOOR,
                    species: None,
                },
            )
            .is_ok(),
            "the Forage gate must not reject — or panic — on a map-less world"
        );
        // `Sow` still answers from its *knowledge* gate — that one needs no tiles — but it must
        // reach that gate rather than panicking, and must not invent a verdict about ground that
        // does not exist.
        let sow = validate_sow(&app, faction, tile, None);
        assert!(
            sow.as_ref()
                .err()
                .is_none_or(|reason| !reason.contains("There is no tile at")
                    && !reason.contains("Nobody gathers at")),
            "a map-less world has no ground to judge, so Sow may only fail on knowledge: {:?}",
            sow
        );
    }

    /// Boot-idle + `new_game`: the server boots with no world (Startup never ran), `new_game` builds
    /// one on demand, an unknown profile is rejected without building, and zero dimensions are rejected.
    #[test]
    fn new_game_builds_a_world_and_rejects_bad_input() {
        let mut app = build_headless_app();
        app.world
            .insert_resource(CommandSenderResource(unbounded::<Command>().0));
        // Idle boot: Startup never ran, so the worldgen-inserted `TileRegistry` does not exist yet.
        assert!(
            app.world.get_resource::<TileRegistry>().is_none(),
            "server boots idle — no world generated"
        );

        let flat = loopback_snapshot_server();
        let mut world_active = false;
        let mut world_epoch: u32 = 0;

        // Unknown profile → rejected, no world built.
        handle_new_game(
            &mut app,
            &mut world_active,
            &mut world_epoch,
            "earthlike".to_string(),
            48,
            32,
            7,
            "no_such_profile".to_string(),
            &flat,
        );
        assert!(!world_active, "an unknown profile must not build a world");
        assert!(
            app.world.get_resource::<TileRegistry>().is_none(),
            "no world after a rejected new_game"
        );
        assert_eq!(
            world_epoch, 0,
            "a rejected new_game does not advance the epoch"
        );

        // Zero dimensions → rejected.
        handle_new_game(
            &mut app,
            &mut world_active,
            &mut world_epoch,
            "earthlike".to_string(),
            0,
            32,
            7,
            "late_forager_tribe".to_string(),
            &flat,
        );
        assert!(!world_active, "zero width must be rejected");
        assert!(app.world.get_resource::<TileRegistry>().is_none());
        assert_eq!(
            world_epoch, 0,
            "a rejected new_game does not advance the epoch"
        );

        // Valid new_game → world built, turns now accepted.
        handle_new_game(
            &mut app,
            &mut world_active,
            &mut world_epoch,
            "earthlike".to_string(),
            48,
            32,
            7,
            "late_forager_tribe".to_string(),
            &flat,
        );
        assert!(world_active, "a valid new_game activates the world");
        assert_eq!(
            app.world.resource::<TileRegistry>().width,
            48,
            "the generated grid matches the new_game width"
        );
        // The first real world is epoch 1, and every snapshot it captured carries it on the header.
        assert_eq!(world_epoch, 1, "the first real world is epoch 1");
        assert_eq!(
            app.world.resource::<WorldEpoch>().0,
            1,
            "the fresh app carries the live epoch resource"
        );
        assert_eq!(
            app.world
                .resource::<SnapshotHistory>()
                .last_snapshot()
                .expect("the built world captured a snapshot")
                .header
                .world_epoch,
            1,
            "the captured snapshot header carries the world epoch"
        );

        // A second build (e.g. NewGame or ResetMap) strictly increases the epoch, and the newly
        // captured header reflects it.
        handle_new_game(
            &mut app,
            &mut world_active,
            &mut world_epoch,
            "earthlike".to_string(),
            48,
            32,
            7,
            "late_forager_tribe".to_string(),
            &flat,
        );
        assert_eq!(world_epoch, 2, "the next world build increments the epoch");
        assert_eq!(
            app.world
                .resource::<SnapshotHistory>()
                .last_snapshot()
                .expect("the rebuilt world captured a snapshot")
                .header
                .world_epoch,
            2,
            "the rebuilt world's snapshot header carries the incremented epoch"
        );
    }

    /// The **improvement** the band's single assignment currently carries — what the four build
    /// commands set, and what a command they reject must leave alone.
    fn band_improvement(app: &bevy::prelude::App, band: Entity) -> Option<Improvement> {
        app.world
            .get::<LaborAllocation>(band)
            .expect("band has an allocation")
            .assignments[0]
            .improvement
    }

    /// The **floor** the band's single assignment currently carries.
    fn band_floor(app: &bevy::prelude::App, band: Entity) -> f32 {
        match &app
            .world
            .get::<LaborAllocation>(band)
            .expect("band has an allocation")
            .assignments[0]
            .target
        {
            LaborTarget::Forage { floor, .. } | LaborTarget::Hunt { floor, .. } => *floor,
            other => panic!("unexpected labor target {other:?}"),
        }
    }

    fn cultivate_rejected_for_unknown(app: &bevy::prelude::App) -> bool {
        app.world.resource::<CommandEventLog>().iter().any(|entry| {
            matches!(entry.kind, CommandEventKind::Cultivate)
                && entry
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("learned Cultivation"))
        })
    }

    /// Rung 1b gate: `cultivate` is rejected when the faction has not learned Cultivation, and the
    /// band's Forage policy is left untouched.
    #[test]
    fn cultivate_rejected_when_cultivation_unknown() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        seed_thriving_patch(&mut app, coord);
        let band = spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
            },
        );

        handle_cultivate(&mut app, faction, coord);

        assert!(
            cultivate_rejected_for_unknown(&app),
            "cultivate must emit a NotKnown failure when Cultivation is unknown"
        );
        assert_eq!(
            band_improvement(&app, band),
            None,
            "a rejected cultivate must not start a build"
        );
        assert_eq!(
            band_floor(&app, band),
            0.5,
            "and it must never touch the band's floor"
        );
    }

    /// **`cultivate` is ACCEPTED on a non-Thriving patch** — the positive pin on the gate
    /// `docs/plan_harvest_floor.md` §3.2 deleted. It replaced
    /// `cultivate_rejected_on_a_stressed_patch`, whose subject is gone: the floor turned the health
    /// cliff into a rate, so pulling hard on ground you are clearing *slows* the meter instead of
    /// refusing the verb, and there is no lapse state left to be exempt from. Stated as a test
    /// rather than deleted, because a re-added phase check would be silent otherwise.
    #[test]
    fn cultivate_is_accepted_on_a_stressed_patch() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        seed_thriving_patch(&mut app, coord);
        {
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(coord).unwrap();
            patch.ecology_phase = EcologyPhase::Stressed;
        }
        grant_cultivation(&mut app, faction);
        let band = spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
            },
        );

        handle_cultivate(&mut app, faction, coord);

        assert!(
            !cultivate_failure_detail_contains(&app, "not thriving"),
            "no health gate survives on the Cultivate verb"
        );
        assert_eq!(
            band_improvement(&app, band),
            Some(Improvement::Cultivate),
            "the crew starts clearing unhealthy ground — the floor prices the pressure, not a gate"
        );
    }

    /// The repurposed `cultivate`: with Cultivation known and a Thriving patch, it **sets the
    /// Cultivate improvement** on the band already foraging the tile (it claims nothing — the investment
    /// must still be worked off).
    #[test]
    fn cultivate_sets_the_cultivate_policy_on_the_working_band() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        seed_thriving_patch(&mut app, coord);
        grant_cultivation(&mut app, faction);
        let band = spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
            },
        );

        handle_cultivate(&mut app, faction, coord);

        assert_eq!(
            band_improvement(&app, band),
            Some(Improvement::Cultivate),
            "cultivate checks the improvement box on the working band"
        );
        assert_eq!(
            band_floor(&app, band),
            0.5,
            "and leaves the floor exactly as the player set it (issue #442)"
        );
        assert!(
            !app.world
                .resource::<ForageRegistry>()
                .patch(coord)
                .unwrap()
                .is_cultivated(),
            "there is no early claim — the patch must still be prepared"
        );
    }

    /// With nobody foraging the tile there is no assignment to re-point: `cultivate` is rejected and
    /// tells the player to staff the patch first.
    #[test]
    fn cultivate_rejected_when_no_band_is_foraging_the_patch() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        seed_thriving_patch(&mut app, coord);
        grant_cultivation(&mut app, faction);

        handle_cultivate(&mut app, faction, coord);

        assert!(cultivate_failure_detail_contains(
            &app,
            "No band is foraging"
        ));
    }

    // --- A build is never refused for the state of the ground under it -------------------------
    //
    // The `Cultivate` verb used to demand `EcologyPhase::Thriving` as a **start** gate, with an
    // exemption for a build already underway (`ForagePatch::cultivation_underway`) — a whole
    // start-vs-continue ruling that existed only to make the mid-build lapse survivable.
    // `docs/plan_harvest_floor.md` §3.2 deleted the lot: the floor replaced the cliff with a rate
    // (`intensification::learn_multiplier`), so pulling hard on ground you are clearing slows the
    // meter instead of stopping it, and nothing lapses. The tests below pin what survives — the
    // knowledge gate, the owner rule, the species gate, and the re-crew path — plus the *absence* of
    // the phase check, which would otherwise regress silently.

    /// Seat the source patch as a **part-built, unhealthy** patch: progress banked and owned, but no
    /// longer Thriving — exactly the state a patch reaches when another band overdraws it
    /// mid-cultivation, and the state the retired phase gate used to refuse.
    fn seed_paused_build(app: &mut bevy::prelude::App, coord: UVec2, owner: Option<FactionId>) {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry
            .patch_mut(coord)
            .expect("the fixture seeded a patch");
        patch.cultivation_progress = PART_PREPARED_PROGRESS;
        patch.owner = owner;
        patch.ecology_phase = EcologyPhase::Stressed;
    }

    /// Progress a paused build has banked — any value strictly inside `(RUNG_UNSTARTED,
    /// RUNG_COMPLETE)` works; a mid-build figure reads as the state it represents.
    const PART_PREPARED_PROGRESS: f32 = 0.5;

    /// **The re-crew case.** A build this faction has underway on a patch that has dropped out of
    /// Thriving still accepts a `Cultivate` assignment — which is what lets the player *ease workers
    /// off* and let the patch regrow. Doubly true since `docs/plan_harvest_floor.md` §3.2: easing
    /// off is now also how you *speed the build up*, because a shallower draw is a faster meter.
    #[test]
    fn a_paused_cultivation_can_still_be_re_crewed() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        seed_thriving_patch(&mut app, coord);
        seed_paused_build(&mut app, coord, Some(faction));
        grant_cultivation(&mut app, faction);

        // Re-checking the box on a paused build this faction owns is still permitted (the exemption).
        let verdict = validate_improvement(
            &app,
            faction,
            &LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
            },
            Improvement::Cultivate,
        );
        assert!(
            verdict.is_ok(),
            "a build already underway must accept a re-check on a paused patch: {verdict:?}"
        );

        // **THE issue-#442 fix, end to end.** A crew change on a paused build is a *stance-side*
        // edit: `assign_labor` names no verb, so the phase gate is never consulted and the build
        // survives at the new head-count. Before the split this command re-issued `Cultivate` and
        // the gate refused it, leaving `workers == 0` (abandon it) as the only executable answer.
        let band = spawn_resident_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
            },
        );
        {
            let mut allocation = app
                .world
                .get_mut::<LaborAllocation>(band)
                .expect("the band works the patch");
            allocation.assignments[0].improvement = Some(Improvement::Cultivate);
        }
        const EASED_OFF_WORKERS: u32 = BAND_WORKERS - 1;
        handle_assign_labor(
            &mut app,
            faction,
            None,
            "forage".to_string(),
            EASED_OFF_WORKERS,
            Some(coord.x),
            Some(coord.y),
            None,
            None,
            None,
        );
        let allocation = app
            .world
            .get::<LaborAllocation>(band)
            .expect("the band keeps its allocation");
        assert_eq!(
            allocation.assignments.len(),
            1,
            "the re-crew replaces the row on the same source, it does not add one"
        );
        assert_eq!(
            allocation.assignments[0].workers, EASED_OFF_WORKERS,
            "the eased-off crew is what the command must be able to apply"
        );
        assert_eq!(
            band_improvement(&app, band),
            Some(Improvement::Cultivate),
            "the build survives a crew change — a stance-side edit never re-asserts the verb, and \
             never drops it"
        );
        assert!(
            !cultivate_failure_detail_contains(&app, "not thriving")
                && !forage_failure_detail_contains(&app, "not thriving"),
            "re-crewing a paused build must emit no phase rejection"
        );
    }

    // `a_fresh_cultivate_on_a_stressed_patch_is_still_refused` was deleted with its subject: it
    // existed to prove that exempting a build underway did not weaken the phase gate, and there is
    // no phase gate left to weaken (`docs/plan_harvest_floor.md` §3.2). The positive replacement is
    // `cultivate_is_accepted_on_a_stressed_patch`.

    /// **A rival's part-built patch is refused by the OWNER rule.** Retargeted from
    /// `another_factions_cultivation_is_still_refused_paused_or_not`, whose other half asserted that
    /// a rival's *paused* build fell through to the phase check — a check that no longer exists
    /// (`docs/plan_harvest_floor.md` §3.2). The owner rule is what was really load-bearing there,
    /// and it survives unchanged.
    #[test]
    fn another_factions_cultivation_is_refused_by_the_owner_rule() {
        let faction = FactionId(0);
        let rival = FactionId(1);
        let coord = UVec2::new(1, 1);
        let patch = LaborTarget::Forage {
            tile: coord,
            floor: 0.5,
            species: None,
        };

        let mut thriving = build_headless_app();
        seed_thriving_patch(&mut thriving, coord);
        {
            let mut registry = thriving.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(coord).unwrap();
            patch.cultivation_progress = PART_PREPARED_PROGRESS;
            patch.owner = Some(rival);
        }
        grant_cultivation(&mut thriving, faction);
        let verdict = validate_improvement(&thriving, faction, &patch, Improvement::Cultivate);
        assert!(
            verdict
                .as_ref()
                .is_err_and(|reason| reason.contains("Another people")),
            "another faction's ground is not yours to clear: {verdict:?}"
        );

        // …and the same rule fires whatever the ground's health, now that health gates nothing.
        let mut stressed = build_headless_app();
        seed_thriving_patch(&mut stressed, coord);
        seed_paused_build(&mut stressed, coord, Some(rival));
        grant_cultivation(&mut stressed, faction);
        let verdict = validate_improvement(&stressed, faction, &patch, Improvement::Cultivate);
        assert!(
            verdict
                .as_ref()
                .is_err_and(|reason| reason.contains("Another people")),
            "the owner rule is the only thing refusing a rival's stressed ground: {verdict:?}"
        );
    }

    /// **The SPECIES gate runs on unhealthy ground too** — the surviving half of the retired
    /// `a_paused_build_is_exempt_from_the_phase_check_and_nothing_else`. That test pinned an
    /// exemption (a build underway skipped the phase check and *only* the phase check); with the
    /// phase check gone (`docs/plan_harvest_floor.md` §3.2) what is left worth pinning is that the
    /// gate *below* it still fires, on exactly the patch state that used to be exempted. This seats
    /// a real tile — `validate_species_selection` needs a `TileRegistry` to have a basket to judge —
    /// and asks for a plant that does not exist.
    ///
    /// **It is driven through `handle_cultivate`, not `validate_improvement` directly** (PR #448
    /// review). It used to hand the validator `species: Some("not_a_plant")` by hand — an input **no
    /// command path could supply**, because the `cultivate` command names no crop and passed `None`.
    /// So the test went on passing while the behaviour it was written to protect was, for a while,
    /// gone entirely. The crop now rides the *band's* assignment, which is where a player's
    /// selection genuinely lives, and the assertion is on the **rejection the feed carries**.
    #[test]
    fn the_species_gate_runs_on_a_part_built_unhealthy_patch_too() {
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);

        // The control: with the band naming no crop, the paused build re-checks (the phase check is
        // the only thing that was refusing it, and it is now exempt).
        let mut control = paused_build_worked_by_a_band(faction, coord, None);
        handle_cultivate(&mut control, faction, coord);
        assert!(
            !cultivate_failure_detail_contains(&control, "not thriving")
                && !cultivate_failure_detail_contains(&control, "know no plant"),
            "control: a paused build whose crew named no crop is re-checkable"
        );

        let mut named = paused_build_worked_by_a_band(faction, coord, Some("not_a_plant"));
        handle_cultivate(&mut named, faction, coord);
        assert!(
            cultivate_failure_detail_contains(&named, "know no plant"),
            "the species gate runs on a part-built, unhealthy patch — nothing above it exempts it"
        );
    }

    /// A paused (part-prepared, non-Thriving) build on `coord` this faction owns, with one band
    /// foraging it under `crop`. The fixture the phase-exemption tests drive the real `cultivate`
    /// command against.
    fn paused_build_worked_by_a_band(
        faction: FactionId,
        coord: UVec2,
        crop: Option<&str>,
    ) -> bevy::prelude::App {
        let mut app = build_headless_app();
        seed_grid_with_baskets(&mut app, PHASE_GATE_GRID);
        seed_thriving_patch(&mut app, coord);
        seed_paused_build(&mut app, coord, Some(faction));
        grant_cultivation(&mut app, faction);
        spawn_resident_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: crop.map(str::to_string),
            },
        );
        app
    }

    /// **A crop the player NAMED is judged by the command path that carries it** (PR #448 review).
    ///
    /// `assign_labor` is the only command that can set `LaborTarget::Forage::species`, and after the
    /// stance/improvement split its validator stopped looking at the Forage arm at all — so
    /// `assign_labor … forage <x> <y> sustain not_a_plant 5` was **accepted**. The bad crop then rode
    /// the assignment into the labor arm, where `resolve_committed_species` refused it, the patch
    /// never committed, the Cultivate rung's `eligible` gate stayed false, and the build meter sat at
    /// zero **forever with nothing said**. All three of the pre-split refusals are asserted, because
    /// they are three different mistakes with three different fixes.
    #[test]
    fn assign_labor_rejects_a_crop_this_ground_cannot_grow() {
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);

        // The control first: the same command with no crop named is accepted, so the rejections
        // below cannot be the verb being broken outright.
        let mut app = forage_ground_with_baskets(faction, coord);
        assign_forage_crop(&mut app, faction, coord, None);
        assert!(
            !forage_failure_detail_contains(&app, "know no plant"),
            "control: naming no crop is the auto-pick, which is always legal"
        );

        // 1. A plant that does not exist at all.
        let mut app = forage_ground_with_baskets(faction, coord);
        assign_forage_crop(&mut app, faction, coord, Some("not_a_plant"));
        assert!(
            forage_failure_detail_contains(&app, "know no plant"),
            "an unknown crop is refused where it is named"
        );

        // 2. A real plant that cannot be tended — a wild harvest, gathered where it grows.
        let mut app = forage_ground_with_baskets(faction, coord);
        assign_forage_crop(&mut app, faction, coord, Some(WILD_CEILING_SPECIES));
        assert!(
            forage_failure_detail_contains(&app, "wild harvest"),
            "a `wild`-ceiling plant can never be committed, so naming it is refused"
        );

        // 3. A real, tendable plant that does not grow on this ground.
        let mut app = forage_ground_with_baskets(faction, coord);
        let elsewhere = a_tendable_species_absent_from(&app, coord);
        assign_forage_crop(&mut app, faction, coord, Some(&elsewhere));
        assert!(
            forage_failure_detail_contains(&app, "does not grow at"),
            "a crop this tile's basket does not carry is refused, naming the tile"
        );
    }

    /// A `wild`-ceiling species from the shipped roster — one that reaches neither plant rung, so
    /// naming it as a crop is always the `CeilingTooLow` refusal. Named by key rather than resolved
    /// at runtime because the *point* of the assertion is that this particular kind of plant is
    /// refused; if the roster ever retires it, this test should fail loudly rather than silently
    /// stop testing the case.
    const WILD_CEILING_SPECIES: &str = "oak_mast";

    /// A tendable species the tile at `coord` does **not** grow — the `NotHere` refusal's input,
    /// resolved against the live roster + the tile's own realized basket so it cannot go stale.
    fn a_tendable_species_absent_from(app: &bevy::prelude::App, coord: UVec2) -> String {
        let flora = app.world.resource::<FloraConfigHandle>().get();
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let map_seed = app.world.resource::<SimulationConfig>().map_seed;
        let ground = app
            .world
            .resource::<TileRegistry>()
            .index(coord.x, coord.y)
            .and_then(|entity| app.world.get::<Tile>(entity))
            .expect("the fixture seeded this tile");
        let here = tile_flora_composition(&flora, &labor.forage, ground, map_seed);
        flora
            .species
            .iter()
            .find(|(key, def)| {
                def.cultivation_ceiling.allows_cultivate()
                    && !here.iter().any(|share| &&share.species == key)
            })
            .map(|(key, _)| key.clone())
            .expect("the roster carries a tendable plant this tile does not grow")
    }

    /// Ground with a real basket and a band already foraging it — the state `assign_labor` edits.
    fn forage_ground_with_baskets(faction: FactionId, coord: UVec2) -> bevy::prelude::App {
        let mut app = build_headless_app();
        seed_grid_with_baskets(&mut app, PHASE_GATE_GRID);
        seed_thriving_patch(&mut app, coord);
        spawn_resident_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
            },
        );
        app
    }

    /// Re-issue the band's forage assignment naming `crop` — the real `assign_labor` command path,
    /// which is the only one that can carry a species.
    fn assign_forage_crop(
        app: &mut bevy::prelude::App,
        faction: FactionId,
        coord: UVec2,
        crop: Option<&str>,
    ) {
        handle_assign_labor(
            app,
            faction,
            None,
            "forage".to_string(),
            BAND_WORKERS,
            Some(coord.x),
            Some(coord.y),
            None,
            crop.map(str::to_string),
            None,
        );
    }

    /// Side of the square tile grid [`seed_grid_with_baskets`] builds. Small, but it has to contain
    /// the `(1, 1)` these tests site their patch on.
    const PHASE_GATE_GRID: u32 = 2;

    /// A square grid of real `Tile`s on a food-bearing biome, plus the `TileRegistry` that indexes
    /// them. `build_headless_app` runs no Startup, so it has no map at all — and the species gate
    /// short-circuits to `Ok` without one, which would quietly make it untestable.
    fn seed_grid_with_baskets(app: &mut bevy::prelude::App, side: u32) {
        let tiles: Vec<Entity> = (0..side * side)
            .map(|i| {
                app.world
                    .spawn(Tile {
                        position: UVec2::new(i % side, i / side),
                        terrain: SOURCE_BIOME,
                        ..Default::default()
                    })
                    .id()
            })
            .collect();
        app.world.insert_resource(TileRegistry {
            tiles,
            width: side,
            height: side,
        });
    }

    // --- `sow` (the plant rung-3 verb, slice 5). The rejections are the contract: each one names a
    // different thing the player must fix, and the barren-ground one has to point at *why*.

    fn grant_seed_selection(app: &mut bevy::prelude::App, faction: FactionId) {
        app.world
            .resource_mut::<DiscoveryProgressLedger>()
            .add_progress(faction, SEED_SELECTION_DISCOVERY_ID, scalar_from_f32(1.0));
    }

    fn sow_failure_detail_contains(app: &bevy::prelude::App, needle: &str) -> bool {
        app.world.resource::<CommandEventLog>().iter().any(|entry| {
            matches!(entry.kind, CommandEventKind::Sow)
                && entry
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains(needle))
        })
    }

    /// The map every `sow` test stands on. The shipped `map_seed` is **0 = entropy**, so a test that
    /// wants a reproducible map must pin one — otherwise "is there hospitable ground here?" is a
    /// coin flip per run.
    const SOW_TEST_MAP_SEED: u64 = 119304647;

    /// A **real world** — `build_headless_app` builds the app, `update` runs the Startup chain, so
    /// the map, its `Tile`s and its seeded forage patches all exist. `sow` needs them: its defining
    /// gate is a property of the *ground*.
    /// The latest published world, with everything that is not simulation state normalized away.
    ///
    /// Two things legitimately differ between the frame published when a tick first happened and the
    /// one recaptured after rolling back to it, and neither is the world:
    ///
    /// - **Publication bookkeeping.** `frame_seq` counts publications, not ticks.
    /// - **Entity ids.** A restore despawns and respawns everything, so bevy hands back fresh
    ///   generations. That is the whole reason the checkpoint keys on stable sim ids, and it is what
    ///   `replay_determinism.rs` canonicalizes before comparing.
    fn normalize_for_compare(app: &bevy::prelude::App) -> sim_schema::world::WorldSnapshot {
        let mut snapshot = (*app
            .world
            .resource::<SnapshotHistory>()
            .last_snapshot()
            .expect("a snapshot was captured"))
        .clone();
        snapshot.header.frame_seq = 0;
        snapshot.header.base_frame_seq = 0;
        snapshot.header.world_epoch = 0;

        snapshot.tiles.sort_by_key(|tile| (tile.y, tile.x));
        for tile in snapshot.tiles.iter_mut() {
            tile.entity = 0;
        }
        snapshot.power.sort_by_key(|node| node.node_id);
        for node in snapshot.power.iter_mut() {
            node.entity = 0;
        }
        snapshot
            .populations
            .sort_by_key(|cohort| (cohort.current_x, cohort.current_y, cohort.size));
        for cohort in snapshot.populations.iter_mut() {
            cohort.entity = 0;
            cohort.home = 0;
            cohort.home_band_entity = 0;
        }
        for layer in snapshot.culture_layers.iter_mut() {
            layer.owner = 0;
        }
        snapshot
    }

    /// **A band handle on the wire resolves to the band it names — including an expedition's.**
    ///
    /// The protocol cutover converted `resolve_starting_unit_entity` to look up a `BandId` and
    /// missed its twin, `resolve_expedition_entity`, which kept calling `Entity::from_bits` on the
    /// incoming value. The client correctly sent a `BandId`; the server read a small counter as an
    /// ECS index/generation pair, resolved nothing, and **returned silently** — `recall_expedition`
    /// did nothing at all and the confirmation dialog reappeared every turn. A human found it.
    ///
    /// `command-guard` could not catch this: it proves the client *emits* the right handle, and the
    /// client was already correct. The missing assertion was that the server *resolves* what it is
    /// sent, which is what this test makes. Asserting the observable effect — the phase actually
    /// flips to `Returning` — rather than that the resolver returns `Some`, because "resolved
    /// something" was never the symptom.
    #[test]
    fn recalling_an_expedition_by_band_id_actually_recalls_it() {
        let mut app = build_headless_app();
        app.world
            .insert_resource(CommandSenderResource(unbounded::<Command>().0));
        let faction = FactionId(0);

        // A detached party is a band and carries a `BandId` like any other — both real expedition
        // spawns allocate one, which is exactly why addressing it by that id must work.
        let expedition_entity = spawn_working_band(&mut app, faction, LaborTarget::Scout);
        let band_id = app.world.resource_mut::<BandIdAllocator>().allocate();
        let home_band = app.world.spawn_empty().id();
        app.world.entity_mut(expedition_entity).insert((
            band_id,
            Expedition {
                home_band,
                mission: ExpeditionMission::Scout,
                phase: ExpeditionPhase::Outbound,
                announced: false,
                pending_reveal: Vec::new(),
                carried_trade: 0.0,
            },
        ));

        handle_recall_expedition(&mut app, faction, band_id.0);

        let phase = app
            .world
            .get::<Expedition>(expedition_entity)
            .expect("the expedition still exists")
            .phase;
        assert_eq!(
            phase,
            ExpeditionPhase::Returning,
            "recall_expedition must resolve the BandId the client sends and flip the phase; \
             reading it as entity bits resolves nothing and returns silently"
        );
    }

    /// **A rollback reaches an early tick however many commands have happened since.**
    ///
    /// This guards a specific past failure, and it is written against the *reach* rather than the
    /// mechanism on purpose. The checkpoint design bounded the rollback ring at
    /// `history_turns / interval` = 16 slots, then added a second producer — one checkpoint per
    /// command — into those same 16. An active player issuing commands across 16 ticks evicted the
    /// entire history, and a 256-turn rollback window silently became "the last few things you
    /// touched". No test caught it because the tests issued no commands.
    ///
    /// A test written against the log's internals would pass again the moment someone reintroduced
    /// a bound. This one asserts what was actually lost: that the early tick is still reachable, and
    /// that rolling back to it reproduces it.
    #[test]
    fn a_rollback_still_reaches_an_early_tick_after_many_commands() {
        const COMMANDS: usize = 40;

        let mut app = build_world_app();
        let mut log = CommandLog::new(&app);
        let band = {
            let mut query = app
                .world
                .query_filtered::<Entity, (With<PopulationCohort>, With<ResidentBand>)>();
            query
                .iter(&app.world)
                .next()
                .expect("worldgen spawned a resident band")
        };

        // An early tick, remembered.
        resolve_turn_with_auto_orders(&mut app);
        log.push(LogEntry::Turn);
        recapture_snapshot_in_place(&mut app.world);
        let early_tick = app.world.resource::<SimulationTick>().0;
        let expected = normalize_for_compare(&app);

        // A long, command-heavy timeline — far more commands than the old ring had slots.
        for index in 0..COMMANDS {
            let command = Command::AssignLabor {
                faction: FactionId(0),
                band_id: Some(band.to_bits()),
                role: "scout".to_string(),
                workers: (index % 4) as u32 + 1,
                target_x: None,
                target_y: None,
                fauna_id: None,
                floor: None,
                species: None,
            };
            log_dispatched_command(&mut log, &command);
            apply_command(&mut app, command, &loopback_snapshot_server());
            resolve_turn_with_auto_orders(&mut app);
            log.push(LogEntry::Turn);
        }

        handle_rollback(&mut app, early_tick, &loopback_snapshot_server(), &mut log);

        assert_eq!(
            app.world.resource::<SimulationTick>().0,
            early_tick,
            "the early tick was no longer reachable after {COMMANDS} commands — the rollback window \
             collapsed, which is the eviction defect this test exists for"
        );
        assert_eq!(
            sim_runtime::hash_snapshot(&expected),
            sim_runtime::hash_snapshot(&normalize_for_compare(&app)),
            "the early tick was reachable but did not reproduce"
        );
    }

    /// **A rollback across a world-mutating command reproduces the world that tick had.**
    ///
    /// This is *the* test of the rollback design, and the one whose absence let the previous one
    /// ship broken. Every oracle in `replay_determinism.rs` drives the world with `app.update()` and
    /// issues **zero commands** — precisely the case where replay is trivially correct, because
    /// every step between the origin and the target is a turn. A command mutates the world *between*
    /// turns, and `run_turn` cannot reproduce it.
    ///
    /// It passes because the log records the command as an entry of its own and replays it in order.
    /// It uses the real handler through the real dispatch, because the seam being tested is the
    /// logging seam, not the mutation.
    #[test]
    fn a_rollback_across_a_command_reproduces_the_world_that_tick_had() {
        let mut app = build_world_app();
        let mut log = CommandLog::new(&app);

        let band = {
            let mut query = app
                .world
                .query_filtered::<Entity, (With<PopulationCohort>, With<ResidentBand>)>();
            query
                .iter(&app.world)
                .next()
                .expect("worldgen spawned a resident band")
        };

        for _ in 0..4 {
            resolve_turn_with_auto_orders(&mut app);
            log.push(LogEntry::Turn);
        }

        // A real command, landing between turns.
        let command = Command::AssignLabor {
            faction: FactionId(0),
            band_id: Some(band.to_bits()),
            role: "scout".to_string(),
            workers: 3,
            target_x: None,
            target_y: None,
            fauna_id: None,
            floor: None,
            species: None,
        };
        log_dispatched_command(&mut log, &command);
        apply_command(&mut app, command, &loopback_snapshot_server());

        resolve_turn_with_auto_orders(&mut app);
        log.push(LogEntry::Turn);

        recapture_snapshot_in_place(&mut app.world);
        let target_tick = app.world.resource::<SimulationTick>().0;
        let expected = normalize_for_compare(&app);

        for _ in 0..4 {
            resolve_turn_with_auto_orders(&mut app);
            log.push(LogEntry::Turn);
        }

        handle_rollback(&mut app, target_tick, &loopback_snapshot_server(), &mut log);
        let actual = normalize_for_compare(&app);

        assert_eq!(
            sim_runtime::hash_snapshot(&expected),
            sim_runtime::hash_snapshot(&actual),
            "the rolled-back world differs from the world tick {target_tick} originally had — the \
             command issued before it was not reproduced by the replayed log"
        );
    }

    fn build_world_app() -> bevy::prelude::App {
        let mut app = build_headless_app();
        app.world.resource_mut::<SimulationConfig>().map_seed = SOW_TEST_MAP_SEED;
        app.update();
        app
    }

    /// **The land's own verdict on every tile**, resolved through the *real* seam the sim uses
    /// (`rung_site_refusal` + `tile_is_fresh_watered` against the `plant:field` rung's own
    /// `site_requirement`) — never a restatement of the rule. `None` = the ground will take seed.
    fn site_verdict(app: &bevy::prelude::App, coord: UVec2) -> Option<Option<SiteRefusal>> {
        let entity = app
            .world
            .resource::<TileRegistry>()
            .index(coord.x, coord.y)?;
        let ground = app.world.get::<Tile>(entity)?;
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let (width, height) = {
            let registry = app.world.resource::<TileRegistry>();
            (registry.width, registry.height)
        };
        let wrap = app
            .world
            .resource::<SimulationConfig>()
            .map_topology
            .wrap_horizontal;
        let fresh_water = tile_is_fresh_watered(ground, width, height, wrap, |neighbor| {
            app.world
                .resource::<TileRegistry>()
                .index(neighbor.x, neighbor.y)
                .and_then(|entity| app.world.get::<Tile>(entity))
                .map(|tile| tile.terrain_tags)
        });
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        Some(rung_site_refusal(
            ladder.rung(RungKey::PlantField),
            ground,
            &labor.forage,
            app.world.resource::<FoodSiteRegistry>().is_site(coord),
            fresh_water,
        ))
    }

    /// **The same tile, judged by a rung that asks only about WATER** — the control the
    /// short-circuit test needs. `site_verdict` can never report a water fault on ground that is off
    /// every gathering site (the site test supersedes), so proving the suppressed fault was real
    /// takes a rung with the site term switched off. Everything else about the reading is identical.
    fn water_only_verdict(app: &bevy::prelude::App, coord: UVec2) -> Option<SiteRefusal> {
        let entity = app
            .world
            .resource::<TileRegistry>()
            .index(coord.x, coord.y)?;
        let ground = app.world.get::<Tile>(entity)?;
        let (width, height) = {
            let registry = app.world.resource::<TileRegistry>();
            (registry.width, registry.height)
        };
        let wrap = app
            .world
            .resource::<SimulationConfig>()
            .map_topology
            .wrap_horizontal;
        let fresh_water = tile_is_fresh_watered(ground, width, height, wrap, |neighbor| {
            app.world
                .resource::<TileRegistry>()
                .index(neighbor.x, neighbor.y)
                .and_then(|entity| app.world.get::<Tile>(entity))
                .map(|tile| tile.terrain_tags)
        });
        core_sim::RungSiteRequirement {
            requires_gathering_site: false,
            min_forage_capacity: 0.0,
            requires_fresh_water: true,
        }
        .refusal(false, 0.0, fresh_water)
    }

    /// The first tile matching `accept`, scanned in a **totally ordered** `(y, x)` sweep — never map
    /// iteration order, so no seed/hash flake (the lesson of `7c09c7e`).
    fn find_tile(
        app: &bevy::prelude::App,
        accept: impl Fn(Option<SiteRefusal>, Option<&ForagePatch>) -> bool,
    ) -> Option<UVec2> {
        let (width, height) = {
            let registry = app.world.resource::<TileRegistry>();
            (registry.width, registry.height)
        };
        for y in 0..height {
            for x in 0..width {
                let coord = UVec2::new(x, y);
                let Some(verdict) = site_verdict(app, coord) else {
                    continue;
                };
                if accept(verdict, app.world.resource::<ForageRegistry>().patch(coord)) {
                    return Some(coord);
                }
            }
        }
        None
    }

    /// **Ground the ladder will take seed on** — rich *and* watered. On the pinned map this is the
    /// river-valley set (~46 tiles of 4160), which is the scarcity the rung is made of.
    fn find_sowable_tile(app: &bevy::prelude::App) -> UVec2 {
        find_tile(app, |verdict, _| verdict.is_none())
            .expect("the pinned map must carry sowable river-valley ground")
    }

    /// Ground the land refuses, in the *specific* way named — the two failures are different problems
    /// and the messages must say which.
    fn find_refused_tile(app: &bevy::prelude::App, refusal: SiteRefusal) -> UVec2 {
        find_tile(app, |verdict, _| verdict == Some(refusal))
            .unwrap_or_else(|| panic!("the pinned map must carry ground that is {refusal:?}"))
    }

    /// **The gate the slice-4 knowledge finally spends.** Without Seed Selection there is no `sow`,
    /// and the refusal must *name* the knowledge and say how it is learned — a gate the player cannot
    /// see is indistinguishable from a bug.
    #[test]
    fn sow_rejected_when_seed_selection_unknown() {
        let mut app = build_world_app();
        let faction = FactionId(0);
        let coord = find_sowable_tile(&app);
        let band = spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
            },
        );

        handle_sow(&mut app, faction, coord);

        assert!(
            sow_failure_detail_contains(&app, "Seed Selection"),
            "the refusal must name the knowledge that gates sowing"
        );
        assert!(
            sow_failure_detail_contains(&app, "tended patches"),
            "...and say how it is learned"
        );
        assert_eq!(
            band_floor(&app, band),
            0.5,
            "a rejected sow must not touch the band's floor"
        );
    }

    /// **Sow stands on ground the people already work.** Rung 3 knows how to move seed, not how to
    /// break unfamiliar country — so ground nobody gathers is refused however rich and however wet,
    /// and the refusal says so rather than blaming the soil or the water. That is rung 4's ground,
    /// and this refusal is the shape of what rung 4 will unlock.
    #[test]
    fn sow_rejected_on_ground_nobody_gathers() {
        let mut app = build_world_app();
        let faction = FactionId(0);
        let coord = find_refused_tile(&app, SiteRefusal::NotGatheringSite);
        grant_seed_selection(&mut app, faction);
        spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
            },
        );

        handle_sow(&mut app, faction, coord);

        assert!(
            sow_failure_detail_contains(&app, "Nobody gathers at"),
            "ground off every gathering site must be refused, naming that fault"
        );
        assert!(
            !sow_failure_detail_contains(&app, "too dry")
                && !sow_failure_detail_contains(&app, "too thin"),
            "...and must NOT teach the player a soil or water fault they cannot act on"
        );
        assert!(
            sow_failure_detail_contains(&app, "move to one"),
            "...pointing at what the player can actually do about it"
        );
    }

    /// **The water rule, and it is not redundant.** Ground rich enough to farm but **dry** is refused:
    /// rung 3 can carry seed, not water. This is what pulls the first fields into the river valleys.
    #[test]
    fn sow_rejected_on_ground_that_is_too_dry() {
        let mut app = build_world_app();
        let faction = FactionId(0);
        let coord = find_refused_tile(&app, SiteRefusal::TooDry);
        grant_seed_selection(&mut app, faction);
        spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
            },
        );

        handle_sow(&mut app, faction, coord);

        assert!(
            sow_failure_detail_contains(&app, "too dry to take a crop"),
            "dry ground must be refused, and the message must name the fault"
        );
        assert!(
            !sow_failure_detail_contains(&app, "too thin"),
            "...and must NOT blame the soil on rich ground"
        );
        assert!(
            app.world
                .resource::<ForageRegistry>()
                .patch(coord)
                .is_none()
                || !app
                    .world
                    .resource::<ForageRegistry>()
                    .patch(coord)
                    .unwrap()
                    .is_field(),
            "a refused sow must not build a field"
        );
    }

    /// **A SITE WHOSE BASKET CANNOT CLIMB IS REFUSED, and this became reachable in this arc.**
    ///
    /// Rung 3's fertility floor of 195 used to admit only the river-deposit class, whose baskets are
    /// full of `field`-ceiling staples — so "the ground takes seed" implied "something here can be
    /// sown", and the two questions never came apart. Dropping the floor for the gathering-site rule
    /// separates them: an open-water fishery or an alpine shelf is a perfectly good gathering site
    /// whose whole basket is `wild`-ceiling, and `flora_config.json` calls that "the ruling working,
    /// not a gap".
    ///
    /// The refusal already existed (`SpeciesRefusal::NothingClimbsHere`, reached because `sow` with no
    /// named species asks `resolve_committed_species` for the rung's default). Nothing pinned it, and
    /// it now guards a case the shipped map actually offers. The client withholds the rung outright
    /// rather than gating it — `RungGates._any_crop_allows`, asserted in `ui_preview` — so this is the
    /// server-side half of one rule.
    #[test]
    fn sow_rejected_where_nothing_in_the_basket_can_climb() {
        let mut app = build_world_app();
        let faction = FactionId(0);
        let coord = find_unsowable_basket_site(&app)
            .expect("the pinned map must carry a gathering site whose basket is all wild-ceiling");
        grant_seed_selection(&mut app, faction);
        spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
            },
        );

        handle_sow(&mut app, faction, coord);

        assert!(
            sow_failure_detail_contains(&app, "can be sown"),
            "a site whose basket cannot climb must be refused on the CROP, naming that fault"
        );
        assert!(
            !sow_failure_detail_contains(&app, "Nobody gathers at"),
            "...and not on the site, which this ground satisfies"
        );
    }

    /// The first tile that clears the **whole site rule** — a curated gathering site, on fresh water
    /// — and whose realized basket still holds nothing that can climb to `field`. Scanned in a
    /// totally-ordered `(y, x)` sweep, and resolved through the same `plant_rung_site_refusal` /
    /// `tile_flora_composition` / `default_species_for_rung` seams the command judges with, so the
    /// fixture cannot select a tile the rule would accept for a different reason.
    ///
    /// **The site rule has to pass, or the test asserts the wrong refusal.** `validate_sow` answers
    /// the LAND before the CROP, so a site that merely failed the water rule would be refused with
    /// "too dry" and the crop check would never be reached — which is exactly how this fixture broke
    /// when #466's water-biased curation moved the marker list under it.
    fn find_unsowable_basket_site(app: &bevy::prelude::App) -> Option<UVec2> {
        let (width, height) = {
            let registry = app.world.resource::<TileRegistry>();
            (registry.width, registry.height)
        };
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let flora = app.world.resource::<FloraConfigHandle>().get();
        let map_seed = app.world.resource::<SimulationConfig>().map_seed;
        for y in 0..height {
            for x in 0..width {
                let coord = UVec2::new(x, y);
                if plant_rung_site_refusal(app, RungKey::PlantField, coord).is_some() {
                    continue;
                }
                let Some(ground) = app
                    .world
                    .resource::<TileRegistry>()
                    .index(x, y)
                    .and_then(|entity| app.world.get::<Tile>(entity))
                else {
                    continue;
                };
                let composition = tile_flora_composition(&flora, &labor.forage, ground, map_seed);
                if default_species_for_rung(&composition, &flora, RungKey::PlantField).is_none() {
                    return Some(coord);
                }
            }
        }
        None
    }

    /// **The gathering-site fault SUPERSEDES the ground readings, and the map proves it is not a
    /// hypothetical.** A dry tile that is also off every gathering site is refused for the site alone
    /// — telling the player "and it is dry" would hand them a second fault they cannot act on, and
    /// the one they *can* act on is "work a site instead".
    ///
    /// Asserted on a tile that would fail the water rule too, so it distinguishes "the site test
    /// short-circuits" from "this tile only had one fault anyway".
    #[test]
    fn the_site_fault_supersedes_the_ground_readings() {
        let mut app = build_world_app();
        let faction = FactionId(0);
        let coord = find_tile(&app, |verdict, _| {
            verdict == Some(SiteRefusal::NotGatheringSite)
        })
        .expect("the pinned map must carry ground off every gathering site");
        // The same ground, judged by a rung that asks only about water: it is genuinely dry, so the
        // refusal above suppressed a real second fault rather than a vacuous one.
        assert_eq!(
            water_only_verdict(&app, coord),
            Some(SiteRefusal::TooDry),
            "the fixture tile must ALSO fail the water rule, or this asserts nothing"
        );
        grant_seed_selection(&mut app, faction);
        spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
            },
        );

        handle_sow(&mut app, faction, coord);

        assert!(sow_failure_detail_contains(&app, "Nobody gathers at"));
        assert!(
            !sow_failure_detail_contains(&app, "too dry"),
            "the site fault must be the whole of the message"
        );
    }

    #[test]
    fn sow_rejected_on_a_tile_off_the_map() {
        let mut app = build_world_app();
        let faction = FactionId(0);
        grant_seed_selection(&mut app, faction);
        let (width, height) = {
            let registry = app.world.resource::<TileRegistry>();
            (registry.width, registry.height)
        };

        handle_sow(&mut app, faction, UVec2::new(width + 5, height + 5));

        assert!(sow_failure_detail_contains(&app, "There is no tile at"));
    }

    /// A Field is already sown — there is nothing left to build, so re-sowing it is refused (the
    /// twin of "the patch is already cultivated").
    #[test]
    fn sow_rejected_on_a_patch_that_is_already_a_field() {
        let mut app = build_world_app();
        let faction = FactionId(0);
        let coord = find_sowable_tile(&app);
        seed_thriving_patch(&mut app, coord);
        {
            // Set the meter straight (the accrual is `pub(crate)`): this test is about the command's
            // gate, not about how a Field gets built.
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(coord).unwrap();
            patch.field_progress = RUNG_COMPLETE;
            patch.owner = Some(faction);
        }
        grant_seed_selection(&mut app, faction);
        spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
            },
        );

        handle_sow(&mut app, faction, coord);

        assert!(sow_failure_detail_contains(&app, "already sown"));
    }

    /// The ownership gate, mirroring Cultivate's: you cannot sow ground another people are working.
    #[test]
    fn sow_rejected_on_another_factions_ground() {
        let mut app = build_world_app();
        let faction = FactionId(0);
        let coord = find_sowable_tile(&app);
        seed_thriving_patch(&mut app, coord);
        {
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(coord).unwrap();
            patch.cultivation_progress = 0.5;
            patch.owner = Some(FactionId(1));
        }
        grant_seed_selection(&mut app, faction);
        spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
            },
        );

        handle_sow(&mut app, faction, coord);

        assert!(sow_failure_detail_contains(&app, "Another people"));
    }

    /// With nobody foraging the ground there is no assignment to re-point: `sow` is rejected and
    /// tells the player to staff it first (the `cultivate` rule — the command sets a policy, it does
    /// not conjure labor).
    #[test]
    fn sow_rejected_when_no_band_is_foraging_the_tile() {
        let mut app = build_world_app();
        let faction = FactionId(0);
        let coord = find_sowable_tile(&app);
        grant_seed_selection(&mut app, faction);

        handle_sow(&mut app, faction, coord);

        assert!(sow_failure_detail_contains(&app, "No band is foraging"));
    }

    /// **The happy path.** On ground the land accepts — rich *and* watered, the river-valley set —
    /// `sow` is accepted: it checks the improvement box and claims nothing, exactly like `cultivate`. The seed
    /// itself goes in when the crew works the ground.
    ///
    /// (`Sow`'s *create-from-nothing* half — hospitable ground carrying no forage site at all — cannot
    /// be reached on a generated map, since worldgen seeds a patch on every food-bearing tile; it is
    /// exercised against constructed bare ground in `forage_field.rs`.)
    #[test]
    fn sow_sets_the_sow_policy_on_qualifying_ground() {
        let mut app = build_world_app();
        let faction = FactionId(0);
        let coord = find_sowable_tile(&app);
        grant_seed_selection(&mut app, faction);
        let band = spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
            },
        );

        handle_sow(&mut app, faction, coord);

        assert_eq!(
            band_improvement(&app, band),
            Some(Improvement::Sow),
            "sow checks the improvement box on the working band"
        );
        assert!(
            !app.world
                .resource::<ForageRegistry>()
                .patch(coord)
                .is_some_and(|patch| patch.is_field()),
            "the command claims nothing — the field must still be worked off"
        );
    }

    /// **THE WIRE CANNOT DISAGREE WITH THE GATE.** `ForagePatchState.sowSiteRefusal` is the answer to
    /// *"why can't I sow here?"* — the question players will actually ask, since only ~1% of tiles are
    /// sowable — so it has to be the **same** verdict `handle_sow` acts on. Both resolve through
    /// `RungSiteRequirement::refusal`; this asserts they agree on a qualifying tile, a too-poor tile
    /// and a too-dry tile, by driving the *real* command and reading the *real* capture.
    #[test]
    fn the_exported_sow_site_refusal_is_the_verdict_the_command_acts_on() {
        for (expected_wire, expected_command_fault) in [
            (SITE_ACCEPTED, None),
            (
                SiteRefusal::NotGatheringSite.as_str(),
                Some("Nobody gathers at"),
            ),
            (SiteRefusal::TooDry.as_str(), Some("too dry to take a crop")),
        ] {
            let mut app = build_world_app();
            let faction = FactionId(0);
            let coord = match expected_command_fault {
                None => find_sowable_tile(&app),
                // **With a patch on it** — this test reads the tile off the WIRE, and the wire
                // carries one entry per patch. Ground off every gathering site is mostly ordinary
                // land that has one, but the first such tile in the sweep can as easily be a glacier
                // that does not, and then the assertion fails on the fixture rather than the rule.
                Some(_) if expected_wire == SiteRefusal::NotGatheringSite.as_str() => {
                    find_tile(&app, |verdict, patch| {
                        verdict == Some(SiteRefusal::NotGatheringSite) && patch.is_some()
                    })
                    .expect("the pinned map must carry a patch off every gathering site")
                }
                Some(_) => find_refused_tile(&app, SiteRefusal::TooDry),
            };
            grant_seed_selection(&mut app, faction);
            spawn_working_band(
                &mut app,
                faction,
                LaborTarget::Forage {
                    tile: coord,
                    floor: 0.5,
                    species: None,
                },
            );

            // What the WIRE says about this ground.
            recapture_snapshot_in_place(&mut app.world);
            let snapshot = app
                .world
                .resource::<SnapshotHistory>()
                .last_snapshot()
                .clone()
                .expect("a snapshot was captured");
            let patch = snapshot
                .forage_patches
                .iter()
                .find(|patch| patch.x == coord.x && patch.y == coord.y)
                .expect(
                    "every food-bearing tile carries a patch, so the wire must describe this one",
                );
            assert_eq!(
                patch.sow_site_refusal, expected_wire,
                "the wire's verdict at {coord:?}"
            );

            // What the COMMAND does about this ground.
            handle_sow(&mut app, faction, coord);
            match expected_command_fault {
                None => {
                    assert!(
                        !sow_failure_detail_contains(&app, "Nothing will grow"),
                        "the wire says this ground takes seed ({expected_wire:?}) — the command must \
                         not refuse it"
                    );
                }
                Some(fault) => {
                    assert!(
                        sow_failure_detail_contains(&app, fault),
                        "the wire says {expected_wire:?} — the command must refuse it for the SAME \
                         reason"
                    );
                }
            }
        }
    }

    /// The rung-3 meters and the Sow forecast pair reach the wire, and read as the rung the patch
    /// actually stands on. `fieldYield` is the payoff the client shows against `ceilingSow`'s dip.
    #[test]
    fn the_wire_carries_both_plant_meters_and_the_sow_forecast_pair() {
        let mut app = build_world_app();
        let coord = find_sowable_tile(&app);
        {
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry
                .patch_mut(coord)
                .expect("sowable ground has a patch");
            // **Above Sustain's escapement floor** — at `K/2` exactly a Sustain row is
            // honestly `+0.00`, and a dip on nothing is nothing.
            patch.biomass = patch.carrying_capacity * STOCKED_PATCH_FRACTION;
            patch.cultivation_progress = 1.0;
            patch.field_progress = 0.4;
            patch.owner = Some(FactionId(0));
        }
        recapture_snapshot_in_place(&mut app.world);
        let snapshot = app
            .world
            .resource::<SnapshotHistory>()
            .last_snapshot()
            .clone()
            .expect("a snapshot was captured");
        let patch = snapshot
            .forage_patches
            .iter()
            .find(|patch| patch.x == coord.x && patch.y == coord.y)
            .expect("the patch is on the wire");

        // BOTH plant meters ship, independently — the two-meter split the client needs.
        assert!((patch.cultivation_progress - 1.0).abs() < 1e-6);
        assert!(patch.is_cultivated);
        assert!((patch.field_progress - 0.4).abs() < 1e-6);
        assert!(!patch.is_field, "0.4 is a half-sown field, not a Field");

        // Sow's pre-commit pair: the dip now, the payoff once sown. On a TENDED patch the dip bites
        // the tended harvest (the rung above is still unbuilt), and the payoff is the Field's rate.
        // **The dip is the FRACTION times the ceiling the client composes** — the wire carries
        // `sow_build_fraction` beside the per-biomass rate, `biomass` and `carryingCapacity`, so
        // this composes the two exactly as the client must (`docs/plan_harvest_floor.md` §5).
        assert!(patch.tended_yield > 0.0);
        let sustain_ceiling =
            (patch.biomass - core_sim::MSY_BIOMASS_FRACTION * patch.carrying_capacity).max(0.0)
                * patch.provisions_per_biomass;
        assert!(
            sustain_ceiling > 0.0,
            "the fixture patch must stand above the food peak, or the dip below is a fraction of 0"
        );
        assert!(
            patch.sow_build_fraction > 0.0 && patch.sow_build_fraction < 1.0,
            "the Sow dip crosses as its fraction: {}",
            patch.sow_build_fraction
        );
        let sow_dip = sustain_ceiling * patch.sow_build_fraction;
        assert!(
            sow_dip > 0.0 && sow_dip < sustain_ceiling,
            "sowing pays a FRACTION of the stance the crew is holding while it builds: {sow_dip} vs \
             {sustain_ceiling}"
        );
        // **Deliberately not compared against `tended_yield`.** Since the harvest floor a stance row
        // is constant escapement — a *stock* — while `tendedYield`/`fieldYield` are long-run rates;
        // ordering a stock against a rate is not a statement about anything
        // (`docs/plan_harvest_floor.md` §1). The payoff comparison that still means something is
        // rate-against-rate, below.
        assert!(
            patch.field_yield > patch.tended_yield,
            "the Field out-yields the patch it replaces — that IS the reason to sow: {} vs {}",
            patch.field_yield,
            patch.tended_yield
        );
    }

    fn grant_cultivation(app: &mut bevy::prelude::App, faction: FactionId) {
        app.world
            .resource_mut::<DiscoveryProgressLedger>()
            .add_progress(faction, CULTIVATION_DISCOVERY_ID, scalar_from_f32(1.0));
    }

    /// `assign_labor forage` reports under `CommandEventKind::Forage`, not `Cultivate` — so a test
    /// that asserts the *absence* of a rejection has to watch the kind the command path emits.
    fn forage_failure_detail_contains(app: &bevy::prelude::App, needle: &str) -> bool {
        app.world.resource::<CommandEventLog>().iter().any(|entry| {
            matches!(entry.kind, CommandEventKind::Forage)
                && entry
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains(needle))
        })
    }

    fn cultivate_failure_detail_contains(app: &bevy::prelude::App, needle: &str) -> bool {
        app.world.resource::<CommandEventLog>().iter().any(|entry| {
            matches!(entry.kind, CommandEventKind::Cultivate)
                && entry
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains(needle))
        })
    }

    /// Seed a herd standing on `coord`, optionally domesticated + owned by `owner`. Returns its id.
    /// One test deer (slice 8). These are **command-validation** tests — they assert which verbs a
    /// herd accepts and which rejections it names, never what a take pays — so the quantum is kept
    /// small enough that it can never be the reason a fixture herd yields nothing.
    const CORRAL_TEST_BODY_MASS: f32 = 1.0;

    fn seed_herd(app: &mut bevy::prelude::App, coord: UVec2, owner: Option<FactionId>) -> String {
        use core_sim::{Herd, SizeClass};
        let mut herd = Herd::new(
            "game_corral_test".to_string(),
            "Test Deer".to_string(),
            SizeClass::Small,
            vec![coord],
            60.0,
            100.0,
            0.0,
            0.05,
            CORRAL_TEST_BODY_MASS,
        );
        if let Some(faction) = owner {
            herd.accrue_domestication(faction, RUNG_COMPLETE);
        }
        let id = herd.id.clone();
        app.world.resource_mut::<HerdRegistry>().herds.push(herd);
        id
    }

    /// Rung 2's gate — what `tame` needs. Since the §4.3 reshuffle that is **all** Herding opens:
    /// corralling needs [`grant_penning`].
    fn grant_herding(app: &mut bevy::prelude::App, faction: FactionId) {
        app.world
            .resource_mut::<DiscoveryProgressLedger>()
            .add_progress(faction, HERDING_DISCOVERY_ID, scalar_from_f32(1.0));
    }

    /// Rung 3's gate — what `corral` and `extend_pen` need. Deliberately grants **only** Penning, so
    /// these tests also prove the gates read the right knowledge rather than any ladder progress.
    fn grant_penning(app: &mut bevy::prelude::App, faction: FactionId) {
        app.world
            .resource_mut::<DiscoveryProgressLedger>()
            .add_progress(faction, PENNING_DISCOVERY_ID, scalar_from_f32(1.0));
    }

    fn herd_is_corralled(app: &bevy::prelude::App, id: &str) -> bool {
        app.world
            .resource::<HerdRegistry>()
            .find(id)
            .is_some_and(|herd| herd.is_corralled())
    }

    fn corral_failure_detail_contains(app: &bevy::prelude::App, needle: &str) -> bool {
        app.world.resource::<CommandEventLog>().iter().any(|entry| {
            matches!(entry.kind, CommandEventKind::Corral)
                && entry
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains(needle))
        })
    }

    /// **The §4.3 gate reshuffle, asserted where it bites:** rung 3 is gated on **Penning**, and
    /// **Herding is no longer enough**. The faction owns a domesticated herd and knows Herding — the
    /// exact state that used to permit corralling — and `corral` is still refused, naming Penning.
    /// The herd stays mobile.
    ///
    /// Granting Herding is the load-bearing half: a test that granted *nothing* would pass just as
    /// happily against the old Herding gate, and so would not pin the reshuffle at all.
    #[test]
    fn corral_rejected_when_penning_unknown_even_knowing_herding() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let id = seed_herd(&mut app, coord, Some(faction));
        grant_herding(&mut app, faction);

        handle_corral(&mut app, faction, coord);

        assert!(
            corral_failure_detail_contains(&app, "learned Penning"),
            "corral must emit a NotKnown failure naming PENNING — Herding gates `tame` only now"
        );
        assert!(
            !herd_is_corralled(&app, &id),
            "a rejected corral leaves the herd mobile"
        );
    }

    /// `corral` is rejected on a herd that isn't domesticated (needs husbandry first), even when the
    /// faction knows Herding.
    #[test]
    fn corral_rejected_when_not_domesticated() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let id = seed_herd(&mut app, coord, None);
        grant_penning(&mut app, faction);

        handle_corral(&mut app, faction, coord);

        assert!(
            corral_failure_detail_contains(&app, "not domesticated"),
            "corral must reject a wild herd as NotDomesticated"
        );
        assert!(!herd_is_corralled(&app, &id));
    }

    /// `corral` is rejected for a faction that doesn't own the domesticated herd.
    #[test]
    fn corral_rejected_for_non_owner() {
        let mut app = build_headless_app();
        let owner = FactionId(0);
        let intruder = FactionId(1);
        let coord = UVec2::new(1, 1);
        let id = seed_herd(&mut app, coord, Some(owner));
        grant_penning(&mut app, intruder);

        handle_corral(&mut app, intruder, coord);

        assert!(
            corral_failure_detail_contains(&app, "do not own"),
            "corral must reject a non-owner"
        );
        assert!(!herd_is_corralled(&app, &id));
    }

    /// The repurposed `corral`: a faction that knows Penning and owns the domesticated herd on the
    /// tile **sets the Corral improvement** on the band already hunting it. The pen is not built yet — that
    /// costs `1 / corral_build_progress_per_turn` turns of the reduced Corral take.
    #[test]
    fn corral_sets_the_corral_policy_on_the_working_band() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let id = seed_herd(&mut app, coord, Some(faction));
        grant_penning(&mut app, faction);
        let band = spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Hunt {
                fauna_id: id.clone(),
                floor: 0.5,
            },
        );

        handle_corral(&mut app, faction, coord);

        assert_eq!(
            band_improvement(&app, band),
            Some(Improvement::Corral),
            "corral checks the improvement box on the working band"
        );
        assert!(
            !herd_is_corralled(&app, &id),
            "there is no early claim — the pen must still be built"
        );
    }

    /// With nobody hunting the herd there is no assignment to re-point: `corral` is rejected.
    #[test]
    fn corral_rejected_when_no_band_is_hunting_the_herd() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        seed_herd(&mut app, coord, Some(faction));
        grant_penning(&mut app, faction);

        handle_corral(&mut app, faction, coord);

        assert!(corral_failure_detail_contains(&app, "No band is hunting"));
    }

    // --- Tame (the intensification ladder's animal rung-2 verb) ----------------------------------

    /// Rung-2 gate (§4.3): `tame` is refused until the faction has learned **Herding**. Taming used
    /// to be ungated (a free side effect of Sustain); it is now paced by practice.
    #[test]
    fn tame_rejected_when_herding_unknown() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        // Owner `None` — a wild, untamed herd, which is what `tame` targets.
        let id = seed_herd(&mut app, coord, None);
        let band = spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Hunt {
                fauna_id: id.clone(),
                floor: 0.5,
            },
        );

        handle_tame(&mut app, faction, id.clone());

        assert!(tame_failure_detail_contains(
            &app,
            "have not learned Herding"
        ));
        assert_eq!(
            band_floor(&app, band),
            0.5,
            "a refused tame must not switch the band's floor"
        );
    }

    /// The happy path: with Herding known and herders already on the herd, `tame` **sets the Tame
    /// policy** on them. It tames nothing outright — the investment must still be worked off (this is
    /// exactly what the retired `domesticate` early-claim let the player skip).
    #[test]
    fn tame_sets_the_tame_policy_on_the_working_band() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let id = seed_herd(&mut app, coord, None);
        grant_herding(&mut app, faction);
        let band = spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Hunt {
                fauna_id: id.clone(),
                floor: 0.5,
            },
        );

        handle_tame(&mut app, faction, id.clone());

        assert_eq!(
            band_improvement(&app, band),
            Some(Improvement::Tame),
            "tame checks the improvement box on the working band"
        );
        assert!(
            !app.world
                .resource::<HerdRegistry>()
                .find(&id)
                .unwrap()
                .is_domesticated(),
            "tame claims nothing — there is no early claim any more"
        );
    }

    /// An already-domesticated herd has climbed this rung — `corral` is the next verb, not `tame`.
    #[test]
    fn tame_rejected_when_already_domesticated() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let id = seed_herd(&mut app, coord, Some(faction));
        grant_herding(&mut app, faction);
        spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Hunt {
                fauna_id: id.clone(),
                floor: 0.5,
            },
        );

        handle_tame(&mut app, faction, id.clone());

        assert!(tame_failure_detail_contains(&app, "already domesticated"));
    }

    /// You cannot tame a herd another people are already taming.
    #[test]
    fn tame_rejected_for_another_factions_herd() {
        let mut app = build_headless_app();
        let owner = FactionId(0);
        let intruder = FactionId(1);
        let coord = UVec2::new(1, 1);
        let id = seed_herd(&mut app, coord, None);
        // Part-tamed by faction 0 — enough to own it, not enough to be domesticated.
        app.world
            .resource_mut::<HerdRegistry>()
            .herds
            .iter_mut()
            .find(|h| h.id == id)
            .unwrap()
            .accrue_domestication(owner, 0.2);
        grant_herding(&mut app, intruder);
        spawn_working_band(
            &mut app,
            intruder,
            LaborTarget::Hunt {
                fauna_id: id.clone(),
                floor: 0.5,
            },
        );

        handle_tame(&mut app, intruder, id.clone());

        assert!(tame_failure_detail_contains(
            &app,
            "Another people are taming"
        ));
    }

    /// `tame` is a policy switch, so it needs someone to switch: staff the herd first.
    #[test]
    fn tame_rejected_when_no_band_is_hunting_the_herd() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let id = seed_herd(&mut app, UVec2::new(1, 1), None);
        grant_herding(&mut app, faction);

        handle_tame(&mut app, faction, id.clone());

        assert!(tame_failure_detail_contains(&app, "No band is hunting"));
    }

    /// An unknown herd id is rejected by name.
    #[test]
    fn tame_rejected_for_an_unknown_herd() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        grant_herding(&mut app, faction);

        handle_tame(&mut app, faction, "game_nonexistent".to_string());

        assert!(tame_failure_detail_contains(&app, "No herd"));
    }

    // --- ExtendPen (Grazing 2d-β) — the command form of growing a built pen's fenced footprint. ------

    /// Seed a herd already **penned** at `coord` (`corral_at`), optionally owned by `owner`.
    fn seed_penned_herd(
        app: &mut bevy::prelude::App,
        coord: UVec2,
        owner: Option<FactionId>,
    ) -> String {
        let id = seed_herd(app, coord, owner);
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
        assert!(
            herd.corral_at(coord),
            "the fixture species must be pennable"
        );
        id
    }

    fn herd_pen_state(app: &bevy::prelude::App, id: &str) -> (u32, bool) {
        let herd = app.world.resource::<HerdRegistry>().find(id).unwrap();
        (herd.pen_radius, herd.pen_extending)
    }

    /// `extend_pen` rides the same `animal:pen` rung as the initial build, so it takes the same
    /// gate: **Penning**, not Herding (which is granted here to prove it is not sufficient).
    #[test]
    fn extend_pen_rejected_when_penning_unknown_even_knowing_herding() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let id = seed_penned_herd(&mut app, coord, Some(faction));
        grant_herding(&mut app, faction);

        handle_extend_pen(&mut app, faction, coord);

        assert!(corral_failure_detail_contains(&app, "learned Penning"));
        assert_eq!(herd_pen_state(&app, &id), (0, false), "no ring started");
    }

    /// `extend_pen` targets the fixed pen anchor: an unpenned (mobile) herd at the tile is "no pen".
    #[test]
    fn extend_pen_rejected_when_no_pen_at_tile() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        // A domesticated but NOT-penned herd standing on the tile.
        let id = seed_herd(&mut app, coord, Some(faction));
        grant_penning(&mut app, faction);

        handle_extend_pen(&mut app, faction, coord);

        assert!(corral_failure_detail_contains(&app, "No pen at"));
        assert_eq!(herd_pen_state(&app, &id), (0, false));
    }

    /// `extend_pen` is rejected for a faction that doesn't own the pen.
    #[test]
    fn extend_pen_rejected_for_non_owner() {
        let mut app = build_headless_app();
        let owner = FactionId(0);
        let intruder = FactionId(1);
        let coord = UVec2::new(1, 1);
        let id = seed_penned_herd(&mut app, coord, Some(owner));
        grant_penning(&mut app, intruder);

        handle_extend_pen(&mut app, intruder, coord);

        assert!(corral_failure_detail_contains(&app, "do not own the pen"));
        assert_eq!(herd_pen_state(&app, &id), (0, false));
    }

    /// A pen already at `pen_radius_max` refuses to extend further.
    #[test]
    fn extend_pen_rejected_at_max_radius() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let radius_max = app
            .world
            .resource::<FaunaConfigHandle>()
            .get()
            .husbandry
            .pen_radius_max;
        let id = seed_penned_herd(&mut app, coord, Some(faction));
        app.world
            .resource_mut::<HerdRegistry>()
            .herds
            .iter_mut()
            .find(|h| h.id == id)
            .unwrap()
            .pen_radius = radius_max;
        grant_penning(&mut app, faction);
        spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Hunt {
                fauna_id: id.clone(),
                floor: 0.5,
            },
        );

        handle_extend_pen(&mut app, faction, coord);

        assert!(corral_failure_detail_contains(&app, "maximum size"));
        assert_eq!(herd_pen_state(&app, &id), (radius_max, false));
    }

    /// With nobody keeping the pen the ring could never accrue: `extend_pen` says to staff it first.
    #[test]
    fn extend_pen_rejected_when_no_band_is_keeping_it() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let id = seed_penned_herd(&mut app, coord, Some(faction));
        grant_penning(&mut app, faction);

        handle_extend_pen(&mut app, faction, coord);

        assert!(corral_failure_detail_contains(&app, "No band is keeping"));
        assert_eq!(herd_pen_state(&app, &id), (0, false));
    }

    /// The happy path: an owned, kept, Penning-known pen below the max enters the extending state.
    #[test]
    fn extend_pen_sets_the_extending_state() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let id = seed_penned_herd(&mut app, coord, Some(faction));
        grant_penning(&mut app, faction);
        spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Hunt {
                fauna_id: id.clone(),
                floor: 0.5,
            },
        );

        handle_extend_pen(&mut app, faction, coord);

        assert_eq!(
            herd_pen_state(&app, &id),
            (0, true),
            "the pen enters the extending state (radius unchanged until the ring completes)"
        );
        assert!(corral_failure_detail_contains(&app, "status=extending"));
    }

    // --- Husbandry ceiling gates (Grazing 2d-δ) -------------------------------------------------------

    /// Set a seeded herd's husbandry ceiling (`wild` | `pastoral` | `pen`) for the gate tests.
    fn set_ceiling(app: &mut bevy::prelude::App, id: &str, ceiling: core_sim::HusbandryCeiling) {
        app.world
            .resource_mut::<HerdRegistry>()
            .herds
            .iter_mut()
            .find(|h| h.id == id)
            .unwrap()
            .husbandry_ceiling = ceiling;
    }

    /// A `wild`-ceiling species (deer, mammoth) is hunt-only — **`tame` rejects it**, and it is
    /// refused for being the wrong *animal*, not for anything about the hunter: the faction here
    /// knows Herding and has herders on the herd, so the ceiling is the only thing left to fail.
    ///
    /// Retargeted from the retired `domesticate_rejects_a_wild_species`: the guarantee ("a wild
    /// species can never be tamed") is unchanged — only the verb that must enforce it moved.
    #[test]
    fn tame_rejects_a_wild_species() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        // Owner `None` so `seed_herd` doesn't auto-domesticate it (the ceiling check is what matters).
        let id = seed_herd(&mut app, UVec2::new(1, 1), None);
        set_ceiling(&mut app, &id, core_sim::HusbandryCeiling::Wild);
        grant_herding(&mut app, faction);
        spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Hunt {
                fauna_id: id.clone(),
                floor: 0.5,
            },
        );

        handle_tame(&mut app, faction, id.clone());

        assert!(tame_failure_detail_contains(&app, "wild game"));
        assert!(
            !app.world
                .resource::<HerdRegistry>()
                .find(&id)
                .unwrap()
                .is_domesticated(),
            "a wild herd is never domesticated"
        );
    }

    fn tame_failure_detail_contains(app: &bevy::prelude::App, needle: &str) -> bool {
        app.world.resource::<CommandEventLog>().iter().any(|entry| {
            matches!(entry.kind, CommandEventKind::Tame)
                && entry
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains(needle))
        })
    }

    /// A non-`pen` species (wild or pastoral) is refused by `corral` — nomadic herders don't fence.
    #[test]
    fn corral_rejects_a_non_pennable_species() {
        for ceiling in [
            core_sim::HusbandryCeiling::Wild,
            core_sim::HusbandryCeiling::Pastoral,
        ] {
            let mut app = build_headless_app();
            let faction = FactionId(0);
            let coord = UVec2::new(1, 1);
            let id = seed_herd(&mut app, coord, Some(faction));
            set_ceiling(&mut app, &id, ceiling);
            grant_penning(&mut app, faction);
            spawn_working_band(
                &mut app,
                faction,
                LaborTarget::Hunt {
                    fauna_id: id.clone(),
                    floor: 0.5,
                },
            );

            handle_corral(&mut app, faction, coord);

            assert!(
                corral_failure_detail_contains(&app, "cannot be penned"),
                "{ceiling:?} must be refused by corral"
            );
            assert!(!herd_is_corralled(&app, &id));
        }
    }

    /// `extend_pen`'s belt-and-braces ceiling check: a (hypothetically) penned non-`pen` species is
    /// refused before it can grow a ring.
    #[test]
    fn extend_pen_rejects_a_non_pennable_species() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let id = seed_penned_herd(&mut app, coord, Some(faction));
        set_ceiling(&mut app, &id, core_sim::HusbandryCeiling::Pastoral);
        grant_penning(&mut app, faction);
        spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Hunt {
                fauna_id: id.clone(),
                floor: 0.5,
            },
        );

        handle_extend_pen(&mut app, faction, coord);

        assert!(corral_failure_detail_contains(&app, "cannot be penned"));
        assert_eq!(herd_pen_state(&app, &id), (0, false), "no ring started");
    }

    /// **An improvement verb never reaches an expedition.** Preparing ground, gentling a herd or
    /// building a pen is place-bound work a *resident* band does — a detached party cannot pen a herd
    /// and walk home — so `send_hunt_expedition` refuses all four at launch, alongside any other
    /// unparseable token. No party may be spawned, and the failure must name the four stances that
    /// ARE valid.
    ///
    /// **The guarantee is structural, and this is its behavioural echo**: a raid's orders are a
    /// **floor**, so a build verb cannot be typed there at all — nor can any other word, since the
    /// launch token is parsed as a number (`sim_runtime`'s `parse_f32`, which also carries the
    /// retired-stance guard). The two hand-written verb lists this replaces had both rotted (the gate
    /// silently accepted `tame`), which is what makes the sweep worth keeping.
    #[test]
    fn send_hunt_expedition_rejects_a_floor_outside_the_dial() {
        for bad in [-0.5_f32, 1.5, f32::NAN] {
            let mut app = build_headless_app();
            let faction = FactionId(0);
            let herd_id = seed_herd(&mut app, UVec2::new(1, 1), Some(faction));

            handle_send_hunt_expedition(&mut app, faction, None, 1, herd_id, Some(bad));

            let rejected = app.world.resource::<CommandEventLog>().iter().any(|entry| {
                matches!(entry.kind, CommandEventKind::ExpeditionSent)
                    && entry
                        .detail
                        .as_deref()
                        .is_some_and(|detail| detail.contains("floor must be"))
            });
            assert!(
                rejected,
                "a floor of {bad} is not on the dial — the launch must be refused with a reason"
            );
            let parties = app
                .world
                .query::<&Expedition>()
                .iter(&app.world)
                .peekable()
                .peek()
                .is_some();
            assert!(!parties, "floor {bad}: no expedition may be spawned");
        }
    }

    /// **A deep floor beside a running build is LEGAL, and nothing gates it** (issue #442,
    /// `docs/plan_investment_rung_toggle.md`). The command layer is where a gate would have had to
    /// live, so this is where its absence is pinned: a deep-floor forage assignment is accepted,
    /// checking `Cultivate` on top of it is accepted, and the two survive together on the band's row.
    ///
    /// The design refuses the gate deliberately — the **rate** is what prices over-drawing while
    /// building (`intensification::learn_multiplier`, `docs/plan_harvest_floor.md` §3), and a gate
    /// would re-create in the UI the very coupling this arc removes from the model. The price itself
    /// is measured on the animal web by
    /// `fauna_husbandry::a_deep_floor_beside_a_tame_build_takes_more_now_and_finishes_later`.
    #[test]
    fn a_deep_floor_accepts_a_cultivate_improvement_beside_it() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        seed_thriving_patch(&mut app, coord);
        grant_cultivation(&mut app, faction);
        let band = spawn_resident_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: 0.15,
                species: None,
            },
        );

        // The stance validator has nothing to say about a plant source, and the improvement's own
        // gates ask about knowledge, health and ownership — never about the stance beside them.
        let target = LaborTarget::Forage {
            tile: coord,
            floor: 0.15,
            species: None,
        };
        assert!(validate_labor_policy(&app, faction, &target).is_ok());
        assert!(
            validate_improvement(&app, faction, &target, Improvement::Cultivate).is_ok(),
            "no gate may refuse a build because of the stance held beside it (§2.1)"
        );

        handle_cultivate(&mut app, faction, coord);

        assert_eq!(
            band_improvement(&app, band),
            Some(Improvement::Cultivate),
            "the build starts under a Deplete stance — it is sayable"
        );
        assert_eq!(
            band_floor(&app, band),
            0.15,
            "and the floor is untouched: the two axes are independent"
        );
    }

    /// **A running improvement can be ABANDONED, and the stance survives** (issue #442). The
    /// two-axis split gave the stance its own control and left the improvement with a set-only one,
    /// which silently removed a capability the old model had by accident: when the build verb *was*
    /// the policy, changing your mind meant picking another policy. `abandon_improvement` is the
    /// missing half — the one path that passes `None`.
    ///
    /// **Phase 1 pins the "ungated" claim against a state where a gate really would bite**: the box
    /// is checked on a patch whose faction has **not learned Cultivation**, so the setting verb is
    /// refused outright and any gate copied onto the abandon path would refuse it too.
    ///
    /// It used to use the `EcologyPhase::Thriving` gate for that control. That gate is gone
    /// (`docs/plan_harvest_floor.md` §3.2 — a build now *slows* under pressure rather than stalling),
    /// so the knowledge gate is the surviving refusal, and it is the better control anyway: it
    /// cannot lapse under the build the player is trying to call off.
    ///
    /// **Phase 2 pins that abandoning does not forfeit the meter**, which needs a build with progress
    /// banked.
    #[test]
    fn a_running_improvement_can_be_abandoned_and_the_stance_survives() {
        // --- Phase 1: the stuck state — box checked, nothing banked, ground gone Stressed. ---
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        seed_thriving_patch(&mut app, coord);
        // Deliberately **not** `grant_cultivation` — the faction cannot set this verb, which is what
        // makes "abandon is ungated" testable at all.
        let band = spawn_resident_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: 0.3,
                species: None,
            },
        );
        {
            let mut allocation = app
                .world
                .get_mut::<LaborAllocation>(band)
                .expect("the band works the patch");
            allocation.assignments[0].improvement = Some(Improvement::Cultivate);
        }

        // The control: *setting* this verb here is refused, so any gate copied onto the abandon path
        // would refuse it too. Without this line the phase below could pass against a gated abandon.
        let would_be_refused = validate_improvement(
            &app,
            faction,
            &LaborTarget::Forage {
                tile: coord,
                floor: 0.3,
                species: None,
            },
            Improvement::Cultivate,
        );
        assert!(
            would_be_refused
                .as_ref()
                .is_err_and(|reason| reason.contains("not learned Cultivation")),
            "fixture: the SETTING verb must be refused here, or 'ungated' is untested: \
             {would_be_refused:?}"
        );

        handle_abandon_improvement(
            &mut app,
            faction,
            "forage".to_string(),
            coord,
            String::new(),
        );

        assert_eq!(
            band_improvement(&app, band),
            None,
            "abandoning is not a rung transition — it takes no gate, least of all the knowledge \
             gate that refuses to *start* the build the player is trying to call off"
        );
        assert_eq!(
            band_floor(&app, band),
            0.3,
            "the floor the player chose is untouched, as with every improvement-side command"
        );
        assert_eq!(
            app.world
                .get::<LaborAllocation>(band)
                .expect("the band keeps its allocation")
                .assignments[0]
                .workers,
            BAND_WORKERS,
            "the crew stays on the source — you abandon the BUILD, not the patch"
        );
        assert!(
            !cultivate_failure_detail_contains(&app, "not thriving"),
            "abandoning must emit no phase rejection"
        );

        // --- Phase 2: the meter is not forfeited. ---
        //
        // Abandoning hands the source back to its web's own unworked-source rule (here
        // `advance_cultivation`'s per-turn bleed), which is exactly the state walking the band out of
        // range already reaches. Inventing a forfeit here would make this path differ from that one.
        let mut app = build_headless_app();
        seed_thriving_patch(&mut app, coord);
        seed_paused_build(&mut app, coord, Some(faction));
        grant_cultivation(&mut app, faction);
        let band = spawn_resident_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
            },
        );
        {
            let mut allocation = app
                .world
                .get_mut::<LaborAllocation>(band)
                .expect("the band works the patch");
            allocation.assignments[0].improvement = Some(Improvement::Cultivate);
        }
        let banked = app
            .world
            .resource::<ForageRegistry>()
            .patch(coord)
            .expect("seeded patch")
            .cultivation_progress;
        assert!(
            banked > 0.0,
            "fixture: there must be progress to walk away from"
        );

        handle_abandon_improvement(
            &mut app,
            faction,
            "forage".to_string(),
            coord,
            String::new(),
        );

        assert_eq!(band_improvement(&app, band), None);
        assert_eq!(
            app.world
                .resource::<ForageRegistry>()
                .patch(coord)
                .expect("seeded patch")
                .cultivation_progress,
            banked,
            "the command does not forfeit progress — the web's decay rule owns that"
        );
    }

    /// Abandoning something nobody is building is a **named rejection**, not a silent no-op — the
    /// same discipline the four setting verbs use for "no band is working that source".
    #[test]
    fn abandoning_nothing_is_rejected_by_name() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        seed_thriving_patch(&mut app, coord);
        spawn_resident_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
            },
        );

        handle_abandon_improvement(
            &mut app,
            faction,
            "forage".to_string(),
            coord,
            String::new(),
        );
        assert!(
            app.world.resource::<CommandEventLog>().iter().any(|entry| {
                matches!(entry.kind, CommandEventKind::CancelOrder)
                    && entry
                        .detail
                        .as_deref()
                        .is_some_and(|detail| detail.contains("Nothing is being built"))
            }),
            "a harvesting band has nothing to abandon, and the feed must say so"
        );

        // An unknown source kind is refused before anything is looked up.
        handle_abandon_improvement(
            &mut app,
            faction,
            "quarry".to_string(),
            coord,
            String::new(),
        );
        assert!(
            app.world.resource::<CommandEventLog>().iter().any(|entry| {
                entry
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("Unknown source kind"))
            }),
            "only the two food webs name a source"
        );
    }

    /// The kind gates: `Corral` on a forage patch and `Cultivate` on a herd are both rejected
    /// outright by `validate_improvement` (the guard each build command routes through).
    #[test]
    fn cross_web_improvements_are_rejected() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        seed_thriving_patch(&mut app, coord);
        let id = seed_herd(&mut app, coord, Some(faction));

        let corral_on_forage = validate_improvement(
            &app,
            faction,
            &LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
            },
            Improvement::Corral,
        );
        assert!(
            corral_on_forage
                .as_ref()
                .is_err_and(|reason| reason.contains("applies to herds")),
            "Corral is not a plant improvement: {corral_on_forage:?}"
        );

        let cultivate_on_hunt = validate_improvement(
            &app,
            faction,
            &LaborTarget::Hunt {
                fauna_id: id,
                floor: 0.5,
            },
            Improvement::Cultivate,
        );
        assert!(
            cultivate_on_hunt
                .as_ref()
                .is_err_and(|reason| reason.contains("applies to forage patches")),
            "Cultivate is not an animal improvement: {cultivate_on_hunt:?}"
        );
    }

    // --- Assign-time yield seeding (the `+0.00` fix) ----------------------------------------------
    //
    // `LaborAllocation.last_yields` used to be written ONLY during turn resolution, so between
    // "player assigns workers" and "player advances the turn" a brand-new source had no telemetry row
    // and the display snapshot serialized `actual_yield = 0.0` — every fresh assignment read `+0.00`.
    // `handle_assign_labor` now seeds the touched source's row from its pre-commit forecast, which
    // (by the forecast == actual invariant) is exactly what the turn then pays: no jump.

    /// f32 slack between the seeded forecast (provisions, direct f32 math) and the resolved take
    /// (biomass → fixed-point provisions): different multiplication order + a 1e-6 fixed-point grid.
    const SEED_EPSILON: f32 = 1e-4;
    /// Side of the square tile grid the seeding tests build.
    const GRID: u32 = 3;
    /// The biome the harness grid stands on — grassland, matching the `FoodModule::SavannaGrassland`
    /// tag its source tile carries. A forage patch's cap is the **tile's**
    /// (`forage.capacity_by_biome`), so the harness names a biome rather than reading a constant.
    const SOURCE_BIOME: sim_runtime::TerrainType = sim_runtime::TerrainType::PrairieSteppe;

    /// A `GRID`×`GRID` tile world + its `TileRegistry` (labor commands resolve band/source positions
    /// through it), with a full-weight `FoodModuleTag` on `source` so a Forage assignment there
    /// resolves. Returns the tile entity at `source`.
    fn seed_tile_grid(app: &mut bevy::prelude::App, source: UVec2) -> Entity {
        use core_sim::{FoodModule, FoodSiteKind};
        let tiles: Vec<Entity> = (0..GRID)
            .flat_map(|y| (0..GRID).map(move |x| UVec2::new(x, y)))
            .map(|position| {
                app.world
                    .spawn(Tile {
                        position,
                        terrain: SOURCE_BIOME,
                        ..Default::default()
                    })
                    .id()
            })
            .collect();
        let source_tile = tiles[(source.y * GRID + source.x) as usize];
        app.world.entity_mut(source_tile).insert(FoodModuleTag {
            module: FoodModule::SavannaGrassland,
            seasonal_weight: 1.0,
            kind: FoodSiteKind::SavannaTrack,
        });
        app.world.insert_resource(TileRegistry {
            tiles,
            width: GRID,
            height: GRID,
        });
        source_tile
    }

    /// A resident band standing on `tile` with **no** assignments — the state `assign_labor` acts on.
    fn spawn_idle_band(app: &mut bevy::prelude::App, faction: FactionId, tile: Entity) -> Entity {
        let band = spawn_working_band(app, faction, LaborTarget::Scout);
        app.world
            .entity_mut(band)
            .insert((
                StartingUnit::new("test_band".to_string(), Vec::new()),
                ResidentBand,
            ))
            .insert(LaborAllocation::default());
        let mut cohort = app.world.get_mut::<PopulationCohort>(band).unwrap();
        cohort.home = tile;
        cohort.current_tile = tile;
        band
    }

    /// Insert a **wild** patch at `coord` with the given biomass (`0.0` = barren) and ecology phase.
    fn seed_patch_with_biomass(
        app: &mut bevy::prelude::App,
        coord: UVec2,
        biomass: f32,
        phase: EcologyPhase,
    ) {
        seed_gathering_site(app, coord);
        let cap = forage_carrying_capacity(app);
        let mut patch = ForagePatch::new(coord, cap);
        patch.biomass = biomass;
        patch.ecology_phase = phase;
        app.world
            .resource_mut::<ForageRegistry>()
            .patches
            .insert(coord, patch);
    }

    /// The harness grid's forage carrying capacity: **the tile's**, from
    /// `forage.capacity_by_biome[SOURCE_BIOME]` (the human food web's per-biome table — no longer a
    /// global constant). The tests stock patches as a fraction of it rather than hard-coding biomass.
    fn forage_carrying_capacity(app: &bevy::prelude::App) -> f32 {
        app.world
            .resource::<LaborConfigHandle>()
            .get()
            .forage
            .capacity_for(SOURCE_BIOME)
    }

    /// **The food peak** — the floor a fresh assignment gets, and the one these seeding fixtures
    /// use whenever the *floor* is not what they are varying.
    const SUSTAIN_FLOOR: f32 = DEFAULT_ESCAPEMENT_FLOOR;

    /// *"Take everything"* — the floor-`0` end of the dial.
    const STRIP_FLOOR: f32 = 0.0;

    /// Drive the real command handler (band resolved by the default resident-band picker).
    fn assign_forage(
        app: &mut bevy::prelude::App,
        faction: FactionId,
        coord: UVec2,
        floor: f32,
        workers: u32,
    ) {
        handle_assign_labor(
            app,
            faction,
            None,
            "forage".to_string(),
            workers,
            Some(coord.x),
            Some(coord.y),
            None,
            None,
            Some(floor),
        );
    }

    fn assign_hunt(
        app: &mut bevy::prelude::App,
        faction: FactionId,
        fauna_id: &str,
        floor: f32,
        workers: u32,
    ) {
        handle_assign_labor(
            app,
            faction,
            None,
            "hunt".to_string(),
            workers,
            None,
            None,
            Some(fauna_id.to_string()),
            None,
            Some(floor),
        );
    }

    /// The single source's seeded/resolved `actual` yield.
    fn source_actual(app: &bevy::prelude::App, band: Entity) -> f32 {
        app.world
            .get::<LaborAllocation>(band)
            .expect("band has an allocation")
            .last_yields
            .first()
            .expect("the staffed source has a telemetry row")
            .actual
    }

    /// The first source's **steady** realized rate (the honest average of the lumpy `actual`).
    fn source_realized(app: &bevy::prelude::App, band: Entity) -> f32 {
        app.world
            .get::<LaborAllocation>(band)
            .expect("band has an allocation")
            .last_yields
            .first()
            .expect("the staffed source has a telemetry row")
            .realized
    }

    /// Resolve one turn of labor (the only system that used to write yield telemetry).
    fn resolve_labor(app: &mut bevy::prelude::App) {
        use bevy_ecs::system::RunSystemOnce;
        app.world
            .run_system_once(core_sim::advance_labor_allocation);
    }

    /// **Forage.** A brand-new assignment reports its expected yield immediately — BEFORE any turn is
    /// advanced — and that seed is exactly what the pre-commit forecast promises.
    #[test]
    fn assigning_forage_workers_seeds_the_expected_yield_before_the_turn() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let tile = seed_tile_grid(&mut app, coord);
        // Half cap → a clear positive MSY skim; Thriving is the phase that biomass implies.
        let stocked = forage_carrying_capacity(&app) * STOCKED_PATCH_FRACTION;
        seed_patch_with_biomass(&mut app, coord, stocked, EcologyPhase::Thriving);
        let band = spawn_idle_band(&mut app, faction, tile);

        assign_forage(&mut app, faction, coord, SUSTAIN_FLOOR, BAND_WORKERS);

        let seeded = source_actual(&app, band);
        assert!(
            seeded > 0.0,
            "a staffed, stocked forage patch must not read +0.00 before the turn: {seeded}"
        );
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let patch = app.world.resource::<ForageRegistry>().patch(coord).unwrap();
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        let flora = app.world.resource::<FloraConfigHandle>().get();
        // The same realized basket the seed path reads — every forage rate is its share-weighted
        // average now (#433), so the expectation has to be priced off it too.
        let map_seed = app.world.resource::<SimulationConfig>().map_seed;
        let ground = app.world.get::<Tile>(tile).expect("the source tile");
        let composition = tile_flora_composition(&flora, &labor.forage, ground, map_seed);
        let expected = forage_source_yield_preview(
            patch,
            &composition,
            &labor.forage,
            &flora,
            &ladder,
            1.0,
            1.0,
            BAND_WORKERS,
            0.5,
            NO_IMPROVEMENT_UNDERWAY,
            labor.yield_average_horizon_turns,
            labor.arrivals_horizon_turns,
        );
        assert!(
            (seeded - expected.actual).abs() < SEED_EPSILON,
            "seed {seeded} must equal the forecast {}",
            expected.actual
        );
    }

    /// **Forage, no jump.** Advancing the turn pays exactly the seeded number (the forecast == actual
    /// invariant): the displayed yield does not move when the turn lands.
    #[test]
    fn resolved_forage_yield_equals_the_seeded_yield() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let tile = seed_tile_grid(&mut app, coord);
        let stocked = forage_carrying_capacity(&app) * STOCKED_PATCH_FRACTION;
        seed_patch_with_biomass(&mut app, coord, stocked, EcologyPhase::Thriving);
        let band = spawn_idle_band(&mut app, faction, tile);

        assign_forage(&mut app, faction, coord, SUSTAIN_FLOOR, BAND_WORKERS);
        let seeded = source_actual(&app, band);
        resolve_labor(&mut app);
        let resolved = source_actual(&app, band);

        assert!(
            (resolved - seeded).abs() < SEED_EPSILON,
            "the turn must pay the seeded yield (seed {seeded}, resolved {resolved})"
        );
    }

    /// **Hunt.** Same seed-before-the-turn guarantee on the animal side. The seed is the herd's
    /// **steady** sustainable rate (`hunt_forecast` drops the transient `hunt_credit` term), so it is
    /// exactly `hunt_source_yield_preview` — the two are the same forecast object, and this pins that
    /// the command-path seed matches it.
    #[test]
    fn assigning_hunt_workers_seeds_the_expected_yield_before_the_turn() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let tile = seed_tile_grid(&mut app, coord);
        let id = seed_herd(&mut app, coord, None);
        let band = spawn_idle_band(&mut app, faction, tile);

        assign_hunt(&mut app, faction, &id, SUSTAIN_FLOOR, BAND_WORKERS);

        let seeded = source_actual(&app, band);
        assert!(
            seeded > 0.0,
            "a staffed, thriving herd must not read +0.00 before the turn: {seeded}"
        );
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let herd = app.world.resource::<HerdRegistry>().find(&id).unwrap();
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        let expected = hunt_source_yield_preview(
            herd,
            &fauna,
            &ladder,
            labor.hunt.per_worker_biomass_capacity,
            1.0,
            BAND_WORKERS,
            0.5,
            NO_IMPROVEMENT_UNDERWAY,
            labor.yield_average_horizon_turns,
            labor.arrivals_horizon_turns,
        );
        assert!(
            (seeded - expected.actual).abs() < SEED_EPSILON,
            "seed {seeded} must equal the forecast {}",
            expected.actual
        );
    }

    /// **Hunt, no jump — on a fresh (empty-bank) herd.** The resolved take equals the seed.
    ///
    /// The seed is now the herd's **steady** sustainable rate (`hunt_forecast` no longer folds in the
    /// banked `hunt_credit`). On a **fresh** herd (`hunt_credit == 0`) the take path's
    /// `min(0 + rate, biomass)` IS that steady rate, so the first resolved turn pays exactly the seed —
    /// no jump. A herd already carrying banked credit would cash it and take *more* this one turn than
    /// the steady display promised; that is the lumpy TAKE, not a forecast error, so this no-jump
    /// invariant is asserted on the empty-bank herd `seed_herd` produces (the precondition below is
    /// load-bearing).
    #[test]
    fn resolved_hunt_yield_equals_the_seeded_yield() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let tile = seed_tile_grid(&mut app, coord);
        let id = seed_herd(&mut app, coord, None);
        let band = spawn_idle_band(&mut app, faction, tile);
        assert_eq!(
            app.world.resource::<HerdRegistry>().find(&id).unwrap().hunt_credit,
            0.0,
            "no-jump is the empty-bank invariant: the steady seed equals the take only when no banked \
             credit is waiting to be cashed"
        );

        assign_hunt(&mut app, faction, &id, SUSTAIN_FLOOR, BAND_WORKERS);
        let seeded = source_actual(&app, band);
        resolve_labor(&mut app);
        let resolved = source_actual(&app, band);

        assert!(
            (resolved - seeded).abs() < SEED_EPSILON,
            "the turn must pay the seeded yield (seed {seeded}, resolved {resolved})"
        );
    }

    /// **Hunt, no jump — the STEADY `realized` projection is a pure function of state.** The
    /// assign-time seeded `realized` is the forward projection off `hunt_forecast`'s herd, and the
    /// first resolved turn recomputes the identical projection from the identical (unchanged) herd
    /// state — so the headline "Food /turn" does not move at all between compose-time and the first
    /// resolved turn, even though `actual` (the lumpy kill) may. Asserted as exact equality, the true
    /// no-jump restored by the forward-projection definition.
    #[test]
    fn resolved_hunt_realized_equals_the_seeded_realized() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let tile = seed_tile_grid(&mut app, coord);
        let id = seed_herd(&mut app, coord, None);
        let band = spawn_idle_band(&mut app, faction, tile);

        assign_hunt(&mut app, faction, &id, SUSTAIN_FLOOR, BAND_WORKERS);
        let seeded = source_realized(&app, band);
        assert!(
            seeded > 0.0,
            "a staffed, thriving herd must seed a positive steady average, not 0: {seeded}"
        );
        resolve_labor(&mut app);
        let resolved = source_realized(&app, band);

        assert!(
            (resolved - seeded).abs() < SEED_EPSILON,
            "the forward-projected realized is a pure function of state, so seed == first resolved \
             (seed {seeded}, resolved {resolved})"
        );
    }

    /// **A floor change re-seeds.** Dragging an existing assignment from the food peak down to `0`
    /// raises the displayed expectation immediately — the seed tracks every shape of the command
    /// that moves the number, not just a fresh staffing.
    ///
    /// Swept across the dial rather than asserted at two points, because the floor is continuous
    /// now: **every** step down must re-seed at least as much as the step above it, so a re-seed
    /// path that only fired at the four values the retired stances named would fail here.
    #[test]
    fn changing_the_floor_reseeds_the_expected_yield() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let tile = seed_tile_grid(&mut app, coord);
        let stocked = forage_carrying_capacity(&app) * STOCKED_PATCH_FRACTION;
        seed_patch_with_biomass(&mut app, coord, stocked, EcologyPhase::Thriving);
        let band = spawn_idle_band(&mut app, faction, tile);

        // Shallow → deep. Each step must re-seed at least as much as the last, and the ends must
        // differ, or the "it re-seeds" claim is satisfied by a row that never moves.
        let mut seeded = Vec::new();
        for floor in [SUSTAIN_FLOOR, 0.42, 0.3, 0.15, STRIP_FLOOR] {
            assign_forage(&mut app, faction, coord, floor, BAND_WORKERS);
            seeded.push((floor, source_actual(&app, band)));
        }
        for pair in seeded.windows(2) {
            assert!(
                pair[1].1 >= pair[0].1 - SEED_EPSILON,
                "dragging the floor from {} to {} must not LOWER the seeded expectation: {seeded:?}",
                pair[0].0,
                pair[1].0
            );
        }
        assert!(
            seeded[seeded.len() - 1].1 > seeded[0].1,
            "stripping the patch must re-seed a higher expectation than holding it at the food              peak: {seeded:?}"
        );
    }

    /// **A barren source still reads `+0.00`.** The seed is a forecast, not a fiction: a patch with no
    /// biomass yields nothing, so `+0.00` stays reachable — and correct — there.
    #[test]
    fn a_barren_source_seeds_zero() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let tile = seed_tile_grid(&mut app, coord);
        seed_patch_with_biomass(&mut app, coord, 0.0, EcologyPhase::Collapsing);
        let band = spawn_idle_band(&mut app, faction, tile);

        assign_forage(&mut app, faction, coord, SUSTAIN_FLOOR, BAND_WORKERS);

        assert_eq!(
            source_actual(&app, band),
            0.0,
            "a barren patch must still seed a zero yield"
        );
    }

    /// **An out-of-range floor is REJECTED, not clamped** (`docs/plan_harvest_floor.md` §4). A floor
    /// is a fraction of `K`; anything outside `0.0..=1.0` names a stock the source cannot have, and
    /// silently clamping it would turn a typo into a quiet policy change on the one number the whole
    /// harvest model turns on. Fail closed, the `cancel_order` scope precedent.
    ///
    /// The **assignment must be untouched** too — a rejected command changes nothing, so a band that
    /// was already working the patch keeps the floor it had rather than being left half-edited.
    #[test]
    fn an_out_of_range_floor_is_rejected_and_leaves_the_assignment_alone() {
        for bad in [-0.01_f32, 1.5, f32::NAN, f32::INFINITY] {
            let mut app = build_headless_app();
            let faction = FactionId(0);
            let coord = UVec2::new(1, 1);
            let tile = seed_tile_grid(&mut app, coord);
            let stocked = forage_carrying_capacity(&app) * STOCKED_PATCH_FRACTION;
            seed_patch_with_biomass(&mut app, coord, stocked, EcologyPhase::Thriving);
            let band = spawn_idle_band(&mut app, faction, tile);

            // A good assignment first, so the rejection has something it could have damaged.
            assign_forage(&mut app, faction, coord, SUSTAIN_FLOOR, BAND_WORKERS);
            let before = source_actual(&app, band);

            assign_forage(&mut app, faction, coord, bad, BAND_WORKERS);

            assert_eq!(
                band_floor(&app, band),
                SUSTAIN_FLOOR,
                "a floor of {bad} must be refused outright, leaving the assignment as it was"
            );
            assert_eq!(
                source_actual(&app, band),
                before,
                "…and the seeded expectation with it"
            );
            assert!(
                app.world.resource::<CommandEventLog>().iter().any(|entry| {
                    matches!(entry.kind, CommandEventKind::Forage)
                        && entry
                            .detail
                            .as_deref()
                            .is_some_and(|detail| detail.contains("floor must be"))
                }),
                "the refusal must say so on the feed rather than failing silently: {bad}"
            );
        }
    }

    /// **Unassigning drops the row.** Setting a source to zero workers removes its assignment *and* its
    /// telemetry row, so the derived `last_yields` stays index-aligned with `assignments` (the snapshot
    /// zips the two by index — a stale row would be attributed to another source).
    #[test]
    fn unassigning_a_source_drops_its_yield_row() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let tile = seed_tile_grid(&mut app, coord);
        let stocked = forage_carrying_capacity(&app) * STOCKED_PATCH_FRACTION;
        seed_patch_with_biomass(&mut app, coord, stocked, EcologyPhase::Thriving);
        let band = spawn_idle_band(&mut app, faction, tile);

        assign_forage(&mut app, faction, coord, SUSTAIN_FLOOR, BAND_WORKERS);
        assign_forage(&mut app, faction, coord, SUSTAIN_FLOOR, 0);

        let allocation = app.world.get::<LaborAllocation>(band).unwrap();
        assert!(allocation.assignments.is_empty(), "the source is unstaffed");
        assert!(
            allocation.last_yields.is_empty(),
            "its telemetry row must go with it"
        );
    }

    // --- `cancel_order` scopes --------------------------------------------------------------------
    //
    // The Band panel splits the single "cancel" button into per-section clears, so the verb names
    // what it clears. `work` takes the worked sources, `roles` the standing roles, `all` both plus
    // the band's travel — and only `all` may touch `BandTravel` (moving is not working).

    /// Workers each staffed source/role carries in the cancel-scope harness. Distinct per target so
    /// a mis-scoped clear shows up in the freed-worker count instead of cancelling out.
    const CANCEL_FORAGE_WORKERS: u32 = 3;
    const CANCEL_HUNT_WORKERS: u32 = 4;
    const CANCEL_SCOUT_WORKERS: u32 = 2;
    const CANCEL_WARRIOR_WORKERS: u32 = 1;
    /// The herd the harness band hunts. It need not exist — `cancel_order` only reads assignments.
    const CANCEL_HERD_ID: &str = "game_deer_01";

    /// A band staffing all four labor targets: two worked sources and both standing roles.
    fn spawn_band_working_every_target(
        app: &mut bevy::prelude::App,
        faction: FactionId,
    ) -> (Entity, UVec2) {
        let coord = UVec2::new(1, 1);
        let tile = seed_tile_grid(app, coord);
        let band = spawn_idle_band(app, faction, tile);
        let available = available_workers(
            app.world
                .get::<PopulationCohort>(band)
                .expect("band has a cohort")
                .working,
        );
        let mut allocation = LaborAllocation::default();
        allocation.set_assignment(
            LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
            },
            CANCEL_FORAGE_WORKERS,
            available,
        );
        allocation.set_assignment(
            LaborTarget::Hunt {
                fauna_id: CANCEL_HERD_ID.to_string(),
                floor: 0.5,
            },
            CANCEL_HUNT_WORKERS,
            available,
        );
        allocation.set_assignment(LaborTarget::Scout, CANCEL_SCOUT_WORKERS, available);
        allocation.set_assignment(LaborTarget::Warrior, CANCEL_WARRIOR_WORKERS, available);
        app.world.entity_mut(band).insert(allocation);
        (band, coord)
    }

    /// Unassigned workers, exactly as the snapshot derives them.
    fn idle_workers(app: &bevy::prelude::App, band: Entity) -> u32 {
        let working = app
            .world
            .get::<PopulationCohort>(band)
            .expect("band has a cohort")
            .working;
        let assigned = app
            .world
            .get::<LaborAllocation>(band)
            .map(|allocation| allocation.assigned_total())
            .unwrap_or(0);
        available_workers(working).saturating_sub(assigned)
    }

    fn staffed_kinds(app: &bevy::prelude::App, band: Entity) -> Vec<&'static str> {
        app.world
            .get::<LaborAllocation>(band)
            .expect("band has an allocation")
            .assignments
            .iter()
            .map(|assignment| assignment.target.kind())
            .collect()
    }

    /// `work` unassigns the worked sources and leaves the standing roles staffed, freeing exactly
    /// the source workers.
    #[test]
    fn cancel_order_work_clears_the_sources_and_keeps_the_roles() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let (band, _) = spawn_band_working_every_target(&mut app, faction);
        let idle_before = idle_workers(&app, band);

        handle_cancel_order(&mut app, faction, None, CancelScope::Work);

        assert_eq!(
            staffed_kinds(&app, band),
            vec!["scout", "warrior"],
            "only the worked sources are unassigned"
        );
        assert_eq!(
            idle_workers(&app, band),
            idle_before + CANCEL_FORAGE_WORKERS + CANCEL_HUNT_WORKERS,
            "the freed source workers go idle"
        );
        let allocation = app.world.get::<LaborAllocation>(band).unwrap();
        assert_eq!(
            allocation.last_yields.len(),
            allocation.assignments.len(),
            "the telemetry rows stay index-aligned with the assignments"
        );
    }

    /// `roles` is the mirror: the standing roles go, the worked sources stay.
    #[test]
    fn cancel_order_roles_clears_the_roles_and_keeps_the_sources() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let (band, _) = spawn_band_working_every_target(&mut app, faction);
        let idle_before = idle_workers(&app, band);

        handle_cancel_order(&mut app, faction, None, CancelScope::Roles);

        assert_eq!(
            staffed_kinds(&app, band),
            vec!["forage", "hunt"],
            "only the standing roles are cleared"
        );
        assert_eq!(
            idle_workers(&app, band),
            idle_before + CANCEL_SCOUT_WORKERS + CANCEL_WARRIOR_WORKERS,
            "the freed role workers go idle"
        );
        let allocation = app.world.get::<LaborAllocation>(band).unwrap();
        assert_eq!(
            allocation.last_yields.len(),
            allocation.assignments.len(),
            "the telemetry rows stay index-aligned with the assignments"
        );
    }

    /// `all` is the historical behaviour: everything goes, travel included.
    #[test]
    fn cancel_order_all_clears_everything_and_stops_the_move() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let (band, _) = spawn_band_working_every_target(&mut app, faction);
        handle_move_band(&mut app, faction, None, 2, 2);
        assert!(
            app.world.entity(band).contains::<BandTravel>(),
            "the band is travelling before the cancel"
        );

        handle_cancel_order(&mut app, faction, None, CancelScope::All);

        assert!(
            staffed_kinds(&app, band).is_empty(),
            "every assignment is cleared"
        );
        assert!(
            !app.world.entity(band).contains::<BandTravel>(),
            "`all` stops the band's move"
        );
    }

    /// Moving is not working: a `work` clear must leave an in-progress `move_band` running.
    #[test]
    fn cancel_order_work_leaves_an_in_progress_move_alone() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let (band, _) = spawn_band_working_every_target(&mut app, faction);
        handle_move_band(&mut app, faction, None, 2, 2);

        handle_cancel_order(&mut app, faction, None, CancelScope::Work);

        assert!(
            app.world.entity(band).contains::<BandTravel>(),
            "unassigning the sources must not strand the band mid-journey"
        );
    }

    /// The rejection is scope-aware: a band with sources but no roles accepts `work` and refuses
    /// `roles`, rather than reporting itself idle.
    #[test]
    fn cancel_order_rejects_only_the_scope_that_has_nothing_to_clear() {
        let mut app = build_headless_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let tile = seed_tile_grid(&mut app, coord);
        let band = spawn_idle_band(&mut app, faction, tile);
        let available = available_workers(
            app.world
                .get::<PopulationCohort>(band)
                .expect("band has a cohort")
                .working,
        );
        let mut allocation = LaborAllocation::default();
        allocation.set_assignment(
            LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
            },
            CANCEL_FORAGE_WORKERS,
            available,
        );
        app.world.entity_mut(band).insert(allocation);

        handle_cancel_order(&mut app, faction, None, CancelScope::Roles);
        assert_eq!(
            staffed_kinds(&app, band),
            vec!["forage"],
            "a rejected `roles` clear touches nothing"
        );

        handle_cancel_order(&mut app, faction, None, CancelScope::Work);
        assert!(
            staffed_kinds(&app, band).is_empty(),
            "`work` is accepted on the same band"
        );
    }
}
