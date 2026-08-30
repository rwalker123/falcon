use std::borrow::Cow;
use std::io::{self, BufReader, Read, Write};
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
    herd_build_verb, hunt_source_yield_preview, knows, load_simulation_config_for_new_world,
    output_multiplier, patch_build_verb, patch_composition, resolve_active_profile,
    resolve_committed_species, resolve_take_selection, rung_site_refusal, species_stands_in,
    tile_flora_composition, tile_is_fresh_watered, ActiveStartProfile, BandBench, BandEquipment,
    BandTravel, BandWorkforce, BeatCatalogHandle, BeatConfigHandle, BeatLedger, BuildJob,
    BuildSource, CampaignLabel, CombatConfigHandle, CreaturesConfigHandle, Expedition,
    ExpeditionConfigHandle, ExpeditionMission, ExpeditionPhase, FloraConfigHandle, FoodModuleTag,
    ForkAnswerError, HuntingParty, KitChoice, KitJob, LaborAllocation, LaborTarget,
    LadderConfigHandle, LocalStore, MaterialsConfigHandle, RecipesConfigHandle, ResidentBand,
    RungKey, SiteRefusal, SourcePriority, SpeciesRefusal, StartProfile, StartProfileOverrides,
    TakeSelection, UpkeepFundMode, WellbeingConfigHandle, DEFAULT_ESCAPEMENT_FLOOR,
    NO_FORAGE_SEASON,
};
use core_sim::{
    build_headless_app, clear_config_overrides, denial_forecast, expedition_returned_event,
    fold_party_into_band, hunt_trip_forecast, install_config_override, party_owes_a_report,
    recapture_snapshot_in_place, run_turn, scalar_from_f32, split_band_from_parent,
    AgentAssignment, BandId, BandIdAllocator, CommandEventEntry, CommandEventKind, CommandEventLog,
    CounterIntelBudgets, CrisisArchetypeCatalog, CrisisArchetypeCatalogHandle,
    CrisisArchetypeCatalogMetadata, CrisisModifierCatalog, CrisisModifierCatalogHandle,
    CrisisModifierCatalogMetadata, CrisisTelemetry, CrisisTelemetryConfig,
    CrisisTelemetryConfigHandle, CrisisTelemetryConfigMetadata, DiscoveryProgressLedger,
    EquipmentConfigHandle, EspionageAgentHandle, EspionageCatalog, EspionageMissionId,
    EspionageMissionKind, EspionageMissionState, EspionageMissionTemplate, EspionageRoster,
    FactionId, FactionOrders, FactionRegistry, FactionSecurityPolicies, FaunaConfigHandle,
    FoodSiteRegistry, ForageRegistry, FrameSink, HerdRegistry, Improvement, LaborConfigHandle,
    MapPresetsHandle, PendingCrisisSpawns, PopulationCohort, QueueMissionError, QueueMissionParams,
    Scalar, SecurityPolicy, Settlement, SimulationConfig, SimulationConfigMetadata, SimulationTick,
    SnapshotHistory, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    SnapshotOverlaysConfigMetadata, StartLocation, StartProfileLookup, StartProfilesHandle,
    StartingUnit, StoredSnapshot, SubmitError, SubmitOutcome, Tile, TileRegistry, TownCenter,
    TurnPipelineConfig, TurnPipelineConfigHandle, TurnPipelineConfigMetadata, TurnQueue,
    WorldEpoch, FOOD,
};
use sim_runtime::{
    commands::{
        query_error, ConfigOverrideKind, EspionageGeneratorUpdate as CommandGeneratorUpdate,
        QueryPayload, QueryReply, QueryReplyEnvelope, ReloadConfigKind, BENCH_CREW_UNSPECIFIED,
        MAX_PROTO_FRAME,
    },
    CancelScope, CommandEnvelope as ProtoCommandEnvelope, CommandPayload as ProtoCommandPayload,
    OrdersDirective as ProtoOrdersDirective, SecurityPolicyKind, TerrainTags, TradeCargoItem,
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
            // **A QUERY IS ANSWERED, NOT APPLIED — and that is why it is matched here rather than
            // falling into the arm below.** Two things below it must not happen to a query:
            //
            //  - `log_dispatched_command` would put it in the replay log. A query mutates nothing,
            //    so replaying one would re-answer a question nobody asked, into a reply channel from
            //    a connection that no longer exists — a log entry that cannot reproduce anything and
            //    can only fail.
            //  - `apply_command` has nothing to apply. There is no world change to make.
            //
            // It also `continue`s past the re-capture at the bottom of the loop: that republishes
            // the entire world, which is the expensive half of a turn, and a question that changed
            // nothing has nothing to republish.
            Command::Query {
                request_id,
                query,
                reply,
            } => {
                let answer = answer_query(world_active, &mut app.world, &query);
                // A send failure means the asking connection is gone. Nothing to recover: the
                // question died with it.
                if reply
                    .send(QueryReplyEnvelope {
                        request_id,
                        reply: answer,
                    })
                    .is_err()
                {
                    warn!(
                        target: "shadow_scale::server",
                        request_id,
                        "query.reply.dropped=asking connection closed"
                    );
                }
                continue;
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

/// **HOW THE FOUR QUEUE VERBS NAME A SOURCE** — a tile, or a herd id
/// (`docs/plan_standing_upkeep.md` §2.5).
///
/// One type rather than four copies of the same optional triple, because `abandon`, `unqueue`,
/// `build_order` and `build_kit` address a source identically and a per-verb spelling is how one of
/// them comes to accept a shape the others reject. It resolves to a [`LaborTarget`] once, in
/// [`BuildSourceRef::target`].
#[derive(Debug, Clone)]
struct BuildSourceRef {
    target_x: Option<u32>,
    target_y: Option<u32>,
    herd_id: Option<String>,
}

impl BuildSourceRef {
    /// The labor target this names, or `None` when the wire carried neither shape. A herd id wins a
    /// malformed both-shapes message only because the tile pair is tested first and requires both
    /// halves — the text grammar cannot produce one at all.
    fn target(&self) -> Option<LaborTarget> {
        match (self.target_x, self.target_y, self.herd_id.as_ref()) {
            (Some(x), Some(y), _) => Some(forage_source(UVec2::new(x, y))),
            (_, _, Some(herd_id)) => Some(LaborTarget::Hunt {
                fauna_id: herd_id.clone(),
                floor: SOURCE_NAMED_NOT_ASSIGNED,
            }),
            _ => None,
        }
    }

    /// What the feed line calls it.
    fn label(&self) -> String {
        match (self.target_x, self.target_y, self.herd_id.as_ref()) {
            (Some(x), Some(y), _) => format!("({x}, {y})"),
            (_, _, Some(herd_id)) => herd_id.clone(),
            _ => "that source".to_string(),
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
    Orders {
        faction: FactionId,
        orders: FactionOrders,
    },
    Rollback {
        tick: u64,
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
        /// **The kit this crew works under** — an `equipment.json` roster id, or `None` for the
        /// job's default. Rejected with a reason if unknown or wrong-job; ignored by the band-wide
        /// roles.
        kit_id: Option<String>,
        /// **Which plants a forage crew carries home** (the selective gather) — `flora_config.json`
        /// species keys, **empty = the whole basket**. Rejected with a reason if the roster does not
        /// know a key or it does not grow on this tile; ignored by every other role.
        take_species: Vec<String>,
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
    /// **Form a new band** — a resident band splits in two where it stands
    /// (`docs/plan_band_fission.md`). `workers` is the player's ONE input; every other quantity
    /// divides on the share it implies.
    SplitBand {
        faction: FactionId,
        band_id: Option<u64>,
        workers: u32,
    },
    SendHuntExpedition {
        faction: FactionId,
        band_id: Option<u64>,
        party_workers: u32,
        fauna_id: String,
        floor: Option<f32>,
        /// **The kit the party is sent out with**, resolved once at launch. `None` = the hunt job's
        /// default; unknown or wrong-job is a command failure.
        kit_id: Option<String>,
    },
    /// **The denial raid** (`docs/plan_denial_raid.md`) — no floor, no fill target, and no target
    /// faction. It names a herd, a party size and the kit that party carries, and nothing else.
    SendDenialRaid {
        faction: FactionId,
        band_id: Option<u64>,
        party_workers: u32,
        fauna_id: String,
        /// See [`Command::SendHuntExpedition::kit_id`]. The one order this mission still takes.
        kit_id: Option<String>,
    },
    /// **The trade expedition** (`docs/plan_contact_and_logistics.md` §Q5) — a party that walks a
    /// shipment to another band. Gated on a live **connection** between the two bands and on
    /// nothing about their factions.
    SendTradeExpedition {
        faction: FactionId,
        band_id: Option<u64>,
        party_workers: u32,
        destination_band_id: u64,
        cargo: Vec<TradeCargoItem>,
        /// See [`Command::SendHuntExpedition::kit_id`].
        kit_id: Option<String>,
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
    /// **The route branch's two TILE verbs**, one variant apiece so the dispatch names the rung it
    /// raises rather than carrying an `Improvement` a caller could get wrong.
    Grade {
        faction: FactionId,
        band_id: u64,
        target_x: u32,
        target_y: u32,
    },
    Pave {
        faction: FactionId,
        band_id: u64,
        target_x: u32,
        target_y: u32,
    },
    // **RETIRED: `AbandonImprovement`.** The build verb is derived from the meter
    // (`forage::patch_build_verb`, `docs/plan_standing_upkeep.md` §2.4), so there was no stored
    // authority for it to clear. What came back in its place is **disposal**, not arbitration:
    // [`Command::Abandon`] puts the whole source down, and [`Command::Unqueue`] withdraws a
    // declaration. Proto field 46 stays reserved.
    /// See [`BuildSourceRef`].
    /// **Put a source down** — drop the band's *holding* of it, the row and its queue entry, on
    /// every band of the faction working it. The meters are untouched; the ground rots back down at
    /// the rung's own rate with nobody keeping it (`docs/plan_standing_upkeep.md` §2.5).
    Abandon {
        faction: FactionId,
        source: BuildSourceRef,
    },
    /// **Withdraw a declaration** — drop the source's build-queue entry only, leaving the row, its
    /// take crew, its kit and the meter alone. The undo a declaration never had.
    Unqueue {
        faction: FactionId,
        source: BuildSourceRef,
    },
    /// **Re-order one band's build queue** — move its entry for this source to a 0-based position.
    /// The order *is* the funding decision, because the whole pool goes on the head.
    BuildOrder {
        faction: FactionId,
        band_id: u64,
        source: BuildSourceRef,
        position: u32,
    },
    /// **Name the kit ONE queued build is raised with** — set it on the source's queue entry, on
    /// every band of the faction that has it queued. `kit_id` absent **clears** the override back to
    /// the entry's own web derivation (`docs/plan_standing_upkeep.md` §4.7a ②).
    BuildKit {
        faction: FactionId,
        source: BuildSourceRef,
        kit_id: Option<String>,
    },
    /// **Name the kit one work site is kept with** — on every band of the faction that works the
    /// source (`docs/plan_standing_upkeep.md` §2.7). See `handle_upkeep_kit`.
    UpkeepKit {
        faction: FactionId,
        source: BuildSourceRef,
        kit_id: Option<String>,
    },
    /// **Mark one worked row with the player's own rank** — `high`, `normal` or `low`, on the named
    /// band's assignment for this source (`docs/plan_standing_upkeep.md` §4.9 item 9b). The band's
    /// scarcity handlers read it as the outermost level of their ordering. See
    /// `handle_work_priority`.
    WorkPriority {
        faction: FactionId,
        band_id: u64,
        source: BuildSourceRef,
        level: String,
    },
    /// **Mark one band's crafting bench with the player's own rank** — `high`, `normal` or `low`,
    /// the same mark a worked row carries and read by the same shedding order. See
    /// `handle_bench_priority`.
    BenchPriority {
        faction: FactionId,
        band_id: u64,
        level: String,
    },
    /// Say how one band splits a maintenance pool it cannot stretch — `spread` or `priority`
    /// (`docs/plan_standing_upkeep.md` §2.5). See `handle_upkeep_mode`.
    UpkeepMode {
        faction: FactionId,
        band_id: u64,
        mode: String,
    },
    ExtendPen {
        faction: FactionId,
        target_x: u32,
        target_y: u32,
    },
    /// Put a recipe on a band's crafting bench and draw idle workers onto it. See
    /// `handle_set_bench` — **make IS the assignment**, so there is no Crafter role card and no
    /// `LaborTarget` variant.
    SetBench {
        faction: FactionId,
        band_id: Option<u64>,
        recipe_id: String,
        workers: u32,
    },
    /// Take the job off a band's bench and hand its crew back.
    ClearBench {
        faction: FactionId,
        band_id: Option<u64>,
    },
    /// Re-crew a band's running bench, leaving the job and its progress alone.
    BenchCrew {
        faction: FactionId,
        band_id: Option<u64>,
        workers: u32,
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
    /// **A question, not an order** — the only variant in this enum the loop *answers* instead of
    /// applying. It mutates nothing, so it is dispatched ahead of the catch-all arm and never
    /// reaches the replay log (see the dispatch site for why that matters).
    ///
    /// `reply` is the asking **connection's** channel, cloned at decode time, so an answer computed
    /// later on the turn thread goes back to the client that asked rather than to whoever is
    /// currently connected.
    Query {
        request_id: u64,
        query: QueryPayload,
        reply: Sender<QueryReplyEnvelope>,
    },
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

/// **The command socket is bidirectional, and this is where the second direction lives.**
///
/// Every payload but one is an *order*: the client sends it, the world changes, and the client
/// learns what happened from the next snapshot. `QueryCommand` is the exception — it asks a question
/// and is *answered* on the same TCP stream. Nothing about the transport had to change to allow
/// that; "one-way" was a protocol choice, not a limitation of the socket.
///
/// So each connection gets a **writer thread** over a `try_clone`d handle and its own unbounded
/// reply channel. The read loop hands a clone of the sender to every `Command::Query` it decodes, so
/// an answer computed on the turn thread — arbitrarily later, and possibly interleaved with other
/// connections' — lands back on *this* client's socket, correlated by `request_id`.
///
/// **The channel is unbounded and the writer never blocks the sim.** A snapshot broadcast can afford
/// to drop a slow client because the next frame supersedes it (`snapshot-socket.md`); a query reply
/// has no successor, so the answer is queued rather than discarded. Bounding queries is the client's
/// job — it asks one question per sheet interaction.
///
/// **Both threads end together.** The writer exits when the read loop drops its sender (the client
/// disconnected) or when a write fails; the reader exits on EOF or a framing violation. Neither can
/// leave the other spinning on a dead socket.
fn handle_proto_client(stream: TcpStream, sender: Sender<Command>) {
    // The write half. A failure to clone is not fatal to the *command* direction — orders still
    // work; only queries go unanswered — so it degrades to a read-only connection rather than
    // dropping a client that may never ask a question.
    let (reply_tx, reply_rx) = unbounded::<QueryReplyEnvelope>();
    match stream.try_clone() {
        Ok(write_half) => {
            thread::spawn(move || write_query_replies(write_half, reply_rx));
        }
        Err(err) => {
            warn!(
                target: "shadow_scale::server",
                error = %err,
                "query.reply_channel.unavailable=stream could not be cloned for writing"
            );
        }
    }

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
                if let Some(cmd) = command_from_payload(envelope.payload, &reply_tx) {
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

/// **The idle-boot gate on a query.**
///
/// The server boots with **no world** (`world_active == false` until `new_game` or `ResetMap`), and
/// that world has no herds, no bands and no `ElevationField`. Answering there would resolve against
/// an empty registry and report `unknown_herd` — true, and useless: it sends the player looking at
/// the map for a problem that is "there is no map". The distinct token is what lets a client say
/// *"start a game first"*.
///
/// Split out of the dispatch arm so the gate is testable without a main loop; it is the one part of
/// answering a query that depends on server state rather than world state.
fn answer_query(
    world_active: bool,
    world: &mut bevy::prelude::World,
    query: &QueryPayload,
) -> QueryReply {
    if !world_active {
        return QueryReply::Error(query_error::NO_ACTIVE_WORLD.to_string());
    }
    core_sim::forecast_query::answer_forecast_query(world, query)
}

/// One connection's **reply writer**: drain answered queries and frame them back onto the socket.
///
/// The framing is the read path's, inverted — a 4-byte little-endian length followed by the encoded
/// protobuf — and it is bounded by the **same** [`MAX_PROTO_FRAME`], deliberately: a reply the reader
/// on the other end would refuse as oversized must not be put on the wire in the first place, because
/// the client's framing loop drops the connection on one. A reply that would exceed it is logged and
/// skipped, which costs the client one unanswered question instead of the whole socket.
///
/// Exits when the sender drops (the connection's read loop ended) or when a write fails — a broken
/// pipe is the ordinary way a client goes away, so it warns at most once and returns.
fn write_query_replies(mut stream: TcpStream, replies: Receiver<QueryReplyEnvelope>) {
    while let Ok(reply) = replies.recv() {
        let request_id = reply.request_id;
        let encoded = match reply.encode_to_vec() {
            Ok(encoded) => encoded,
            Err(err) => {
                warn!(
                    target: "shadow_scale::server",
                    request_id,
                    error = %err,
                    "query.reply.encode_failed"
                );
                continue;
            }
        };
        if encoded.len() > MAX_PROTO_FRAME {
            warn!(
                target: "shadow_scale::server",
                request_id,
                bytes = encoded.len(),
                limit = MAX_PROTO_FRAME,
                "query.reply.too_large=dropped rather than sent as a frame the client must refuse"
            );
            continue;
        }
        let header = (encoded.len() as u32).to_le_bytes();
        if let Err(err) = stream
            .write_all(&header)
            .and_then(|()| stream.write_all(&encoded))
            .and_then(|()| stream.flush())
        {
            warn!(
                target: "shadow_scale::server",
                request_id,
                error = %err,
                "query.reply.write_failed=connection closed"
            );
            return;
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
    _improvement: Option<Improvement>,
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
    // **The kit this crew was assigned with, read off the assignment itself** — not off the band's
    // stock. `set_assignment` has already stored it, so the seed and the turn resolve the identical
    // tier through the identical seam, which is what `forecast == actual` rests on.
    let crew_kit = {
        let equipment_cfg = app.world.resource::<EquipmentConfigHandle>().get();
        app.world
            .get::<LaborAllocation>(band)
            .and_then(|allocation| {
                allocation
                    .assignments
                    .iter()
                    .find(|assignment| assignment.target.same_source(target))
                    .map(|assignment| assignment.kit_choice(&equipment_cfg))
            })
    };
    let Some(crew_kit) = crew_kit else {
        return;
    };
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
    // **The reported band's width** (`combat_config.forecast_range_sigmas`) — a readout lever, not a
    // model term (`docs/plan_hunt_through_combat.md` §6.4). Read on both webs so the one
    // `forecast_source_yield` seeds every row's range the same way.
    let range_sigmas = app
        .world
        .resource::<CombatConfigHandle>()
        .get()
        .forecast_range_sigmas;

    let seeded = match target {
        // The three band-wide roles work no source, so there is nothing to price — see the arm at
        // the end of this match.
        LaborTarget::Forage {
            tile,
            floor,
            take_species,
            ..
        } => {
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
            let flora = app.world.resource::<FloraConfigHandle>().get();
            // **What is growing on this tile** — the same realized basket the labor arm and the
            // snapshot read, so the assign-time seed is priced off the identical composition the
            // turn will pay from (#433). Absent ground names no plants.
            let map_seed = app.world.resource::<SimulationConfig>().map_seed;
            let tile_composition = app.world.get::<Tile>(tile_entity).map_or_else(
                || Cow::Owned(Vec::new()),
                |ground| tile_flora_composition(&flora, &labor.forage, ground, map_seed),
            );
            // **The seed must be priced at THIS band's BASKET tier**, for the same reason the hunt
            // arm below prices its haul at the band's sled tier: `advance_labor_allocation` resolves
            // the same tier through the same seam, so a band-agnostic equipped rate here would
            // promise a bare-handed band a basketful (`yield-forecast.md`).
            let equipment_cfg = app.world.resource::<EquipmentConfigHandle>().get();
            let band_wear = app
                .world
                .get::<BandEquipment>(band)
                .cloned()
                .unwrap_or_default();
            // **Through the same coverage the turn resolves** (`equipment.md` → "the
            // partly-equipped party"): baskets cover gatherers one unit at a time, so a seed priced
            // at the whole crew's best tier would promise a basketful to people holding nothing.
            let crew_coverage = equipment_cfg.coverage(&crew_kit, workers as f32, &band_wear);
            let per_worker_biomass = crew_coverage.weighted_rate(|kit| {
                equipment_cfg.forage_per_worker_biomass_capacity(
                    labor.forage.per_worker_biomass_capacity,
                    kit,
                    &band_wear,
                )
            });
            forage_source_yield_preview(
                patch,
                &tile_composition,
                &labor.forage,
                &flora,
                per_worker_biomass,
                seasonal,
                output_mult,
                workers,
                *floor,
                // **The crew's own take selection** — a seed priced on the whole basket would
                // promise a narrowed crew a stand it will not touch, which is exactly the
                // forecast-vs-actual split the seed exists to close.
                take_species,
                labor.yield_average_horizon_turns,
                labor.arrivals_horizon_turns,
                range_sigmas,
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
            // **The seed must be priced at THIS band's SLED tier** (the minimal TOE), or the
            // exact-forecast-equals-actual invariant breaks the moment a band's baskets run dry:
            // `advance_labor_allocation` resolves the same tier through the same seam, so a
            // band-agnostic equipped rate here would promise a dry band a kitted haul.
            let equipment_cfg = app.world.resource::<EquipmentConfigHandle>().get();
            let band_wear = app
                .world
                .get::<BandEquipment>(band)
                .cloned()
                .unwrap_or_default();
            // **ONE CARRY RATE, PENNED OR WILD** (issue #543) — the same single rate
            // `advance_labor_allocation` resolves, because the seed has to reach `hunt_forecast`
            // holding the rate the turn will pay it at. A seed and a resolved row that priced a herd
            // differently is `yield-forecast.md`'s invariant broken on the one surface the player
            // commits from, so this must stay exactly-equal to the sim's arm.
            // **The same coverage the turn resolves** (`equipment.md` → "the partly-equipped
            // party"): `advance_labor_allocation` divides this crew by the gear the band actually
            // owns, so a seed priced at the whole party's best tier would promise a haul only the
            // armed half can make.
            let hunt_coverage = equipment_cfg.coverage(&crew_kit, workers as f32, &band_wear);
            let per_worker_biomass = hunt_coverage.weighted_rate(|kit| {
                equipment_cfg.hunt_per_worker_biomass_capacity(
                    labor.hunt.per_worker_biomass_capacity,
                    kit,
                    &band_wear,
                )
            });
            // **And at THIS band's FIGHTING tier**, for the same reason and through the same seam
            // (`docs/plan_hunt_through_combat.md` §4): the take now resolves through the combat
            // system, so a band whose spears are gone brings down less — or, past a quarry's
            // `defense`, nothing at all — and the seed has to say so.
            let combat_cfg = app.world.resource::<CombatConfigHandle>().get();
            let hunting_party = core_sim::PartyResolution {
                equipment: &equipment_cfg,
                coverage: &hunt_coverage,
                wear: &band_wear,
                intrinsic: app.world.resource::<CreaturesConfigHandle>().get().person(),
                tuning: combat_cfg.tuning(),
                hunt_injury_damage_per_animal: combat_cfg.hunt_injury_damage_per_animal,
            }
            .party_against(core_sim::Quarry::Mass(herd.body_mass));
            hunt_source_yield_preview(
                herd,
                &fauna,
                per_worker_biomass,
                &hunting_party,
                output_mult,
                workers,
                *floor,
                labor.yield_average_horizon_turns,
                labor.arrivals_horizon_turns,
                range_sigmas,
            )
        }
        // A band-wide role produces no per-source yield, so there is no row to seed.
        LaborTarget::Scout
        | LaborTarget::Warrior
        | LaborTarget::Agriculture
        | LaborTarget::Husbandry
        | LaborTarget::Roadwork
        | LaborTarget::Builders => return,
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
            // **WHICH PLANTS ARE THEY HERE FOR? — NOT ASKED HERE.** The take selection is
            // *resolved* rather than gated, because most of what is wrong with one is the ground
            // having moved under it rather than the player having mistyped it — see
            // [`resolve_take_for_ground`], which `handle_assign_labor` runs immediately after this
            // and which prunes the stale names instead of refusing the command that carries them.
            //
            // It lives outside this gate because this function only ever answers *may they*, and a
            // repair is not a verdict: threading a rewritten selection back out of a
            // `Result<(), String>` would make every other arm carry a value it has no opinion about.
            let Some(named) = species.as_deref() else {
                return Ok(());
            };
            validate_species_selection(app, *tile, Some(named), RungKey::PlantTended)
        }
        LaborTarget::Scout
        | LaborTarget::Warrior
        | LaborTarget::Agriculture
        | LaborTarget::Husbandry
        | LaborTarget::Roadwork
        | LaborTarget::Builders => Ok(()),
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
        LaborTarget::Scout
        | LaborTarget::Warrior
        | LaborTarget::Agriculture
        | LaborTarget::Husbandry
        | LaborTarget::Roadwork
        | LaborTarget::Builders => Err(format!(
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
    // **THE REFUSAL IS THE METER'S FULLNESS, NOT THE ACHIEVED RUNG** — the same question
    // `forage::patch_rung_already_built` and `forage::patch_build_verb` ask, so the command and the
    // queue can never disagree about whether there is work left on this ground.
    //
    // **A tended patch eroded below its cost is a REPAIR, and this used to forbid it.**
    // `is_cultivated()` used to compare against the *retention bar*, which sat well below the cost,
    // so a patch at 99% answered *"already cultivated"* — while completion had already retired its
    // queue entry, and `build_workers` aims the pool only at a head that declares. The three
    // composed into a rung that could never be repaired: no entry, no builders, and no command that
    // could make one (`docs/plan_standing_upkeep.md` §2.4).
    //
    // **THE GAP IS GONE RATHER THAN BRIDGED** (§2.8): the retention bar is deleted, so *achieved*
    // and *its meter is full* are one fact and there is no band between them for a patch to be
    // stranded in. The predicate is `is_cultivated()` again — which now means exactly what the
    // retired `cultivation_meter_full()` meant — and a rung that dips is simply not achieved, so
    // re-queueing it is an ordinary build rather than a repair the command had to be taught to allow.
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
    // **The meter's fullness, not the fence flag** — [`validate_cultivate`]'s rule on the animal
    // web, so this refusal asks exactly what `fauna::herd_rung_already_built` asks. The two agree on
    // every herd the sim can reach today (`corral_at` sets the meter to its own cost, and nothing
    // bleeds it), so this is the *shape* being made uniform rather than a behaviour change: it is
    // what keeps a pen meter that ever learns to erode repairable by the same one-line rule the
    // plant web already needed.
    if herd.corral_meter_full() {
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
        // [`validate_cultivate`]'s rule one rung up, and the same history: `is_field()` used to read
        // a *retention bar* below the cost, so a Field eroded to 99% refused the very `sow` that
        // would repair it. The bar is deleted (§2.8), so the achieved rung and a full meter are one
        // fact and this asks it once.
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

/// **THE TAKE SELECTION THIS COMMAND WILL STORE, and the names the ground has taken off it** — the
/// selection resolved against the mix the take will actually narrow.
///
/// `selection` is what the row must carry; `dropped` is the display names pruned out of it, empty
/// when the player's selection stood as given. The caller stores the one and announces the other.
struct ResolvedTake {
    selection: TakeSelection,
    dropped: Vec<String>,
}

/// **What may this crew gather HERE, given what it asked for?** — the take selection's command-side
/// resolution, phrased for the player.
///
/// It resolves through the *same* `forage::resolve_take_selection` seam the take path narrows with,
/// against **the mix that take will narrow** — `forage::patch_composition`, the patch's live
/// rung-reweighted basket, falling back to the tile's own realization
/// (`forage::tile_flora_composition`, never `FloraConfig::composition` on a raw terrain) where no
/// patch stands yet. **There is no rung *asked* in it**: a take selection says what the crew carries
/// home from the stand that is standing, so a `wild`-ceiling plant is a perfectly good answer — but
/// the stand that is standing on a tended or sown patch is the reweighted one, not the wild
/// realization it grew out of.
///
/// **Judging the wild basket was a defect, not a simplification.** The two baskets differ on every
/// committed patch, so a selection naming a plant weeding or sowing had already displaced was
/// accepted at the command boundary — freshly typed, no staleness involved — and then valued at
/// exactly zero by the very next turn's take.
///
/// > #### ⛔ AN ABSENT PLANT IS PRUNED. ONLY A PLANT NO ROSTER CARRIES IS REFUSED.
/// >
/// > Judging the patch's own mix is right; **hard-refusing on it was not.** The mix moves under a
/// > stored selection — that is what a `Cultivate`/`Sow` *is* — so the names this finds absent are
/// > typically ones that were legal when the player made them and that the player's own crop then
/// > weeded out.
/// >
/// > Refusing them refused the **whole `assign_labor`**, worker count included: reported from play on
/// > a Field at `Wild Emmer 100%` whose row still named Wild Pulses, where raising the tenders did
/// > nothing at all and the feed said only *"Harvest failed — Wild Pulses does not grow at (13,
/// > 10)"*. There was no way out of it from the panel either — a chip is only drawn for a plant the
/// > **current** mix carries, so the stale key had no control to clear it with.
/// >
/// > So the absent case **prunes** ([`TakeSelection::pruned_to`], the same narrowing the turn's own
/// > commitment repair runs) and the command lands. An **unknown** key is still refused by name: that
/// > is a typo, nothing can be inferred from it, and the player should be told.
fn resolve_take_for_ground(
    app: &bevy::prelude::App,
    tile: UVec2,
    take: &TakeSelection,
) -> Result<ResolvedTake, String> {
    let unchanged = || ResolvedTake {
        selection: take.clone(),
        dropped: Vec::new(),
    };
    if take.is_everything() {
        return Ok(unchanged());
    }
    // **No map, nothing to judge** — the same carve-out `validate_species_selection` makes, and for
    // the same reason: the command-unit harnesses and the idle boot carry no `TileRegistry`, and the
    // labor arm (which always has the real tiles) remains the authority.
    let Some(registry) = app.world.get_resource::<TileRegistry>() else {
        return Ok(unchanged());
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
    let tile_composition = tile_flora_composition(&flora, &labor.forage, ground, map_seed);
    // The patch's own mix where one stands — the same seam `forage_take` narrows against.
    let registry = app.world.resource::<ForageRegistry>();
    let composition = registry.patch(tile).map_or_else(
        || Cow::Borrowed(tile_composition.as_ref()),
        |patch| patch_composition(patch, &tile_composition, &flora, &labor.forage),
    );
    let absent = match resolve_take_selection(take, &composition, &flora) {
        Ok(absent) => absent,
        Err(species) => return Err(format!("Your people know no plant called '{species}'.")),
    };
    if absent.is_empty() {
        return Ok(unchanged());
    }
    let dropped = absent
        .iter()
        .map(|species| {
            flora
                .species
                .get(*species)
                .map_or_else(|| (*species).to_string(), |def| def.display_name.clone())
        })
        .collect();
    Ok(ResolvedTake {
        selection: take.pruned_to(|species| species_stands_in(&composition, species)),
        dropped,
    })
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
/// 4. **Not already domesticated** — this rung is already climbed; `corral` is the next verb. This
///    one already reads the **meter** (`Herd::is_domesticated` *is* `progress >= cost`; the
///    pastoral rung has no separate retention bar), so it needed nothing when the plant verbs' gates
///    were moved off the retention bar — and `fauna::herd_rung_already_built` asks the same
///    expression, which is why a `Tame` was never caught in the repair deadlock.
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

/// **What this row runs on when the player names no kit** — the herd's own default for a Hunt on a
/// resolvable quarry, the job's default for everything else.
///
/// It is the **same** id the wire published for that herd (`HerdTelemetryState::default_kit_id`,
/// via `fauna::herd_default_hunt_kit`, resolved through the same seams at the same fresh tier and
/// on the same source axis), so the command and the compose sheet cannot disagree about what "no
/// kit named" means.
///
/// **Every no-kit-named hunt surface resolves through here** — `assign_labor` and both raiding
/// verbs (`resolve_raid_kit`) — because a second resolution is a second answer, and the one the
/// launch sheet quoted is the one the launch has to run.
///
/// Falls through to `default_kits.<job>` on every path with no quarry to score: a Forage, Scout or
/// Warrior row, a herd id the registry does not carry, and a herd whose species the roster cannot
/// resolve.
fn default_kit_for_target(
    app: &bevy::prelude::App,
    equipment: &core_sim::EquipmentConfig,
    target: &LaborTarget,
) -> KitChoice {
    let job = target.kit_job();
    let LaborTarget::Hunt { fauna_id, .. } = target else {
        return equipment.default_kit(job);
    };
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let resolved = app
        .world
        .resource::<HerdRegistry>()
        .find(fauna_id)
        .and_then(|herd| {
            fauna
                .species_by_display(&herd.species)
                .map(|species| (species, herd.is_corralled()))
        });
    let Some((species, corralled)) = resolved else {
        return equipment.default_kit(job);
    };
    core_sim::herd_default_hunt_kit(
        equipment,
        app.world.resource::<CreaturesConfigHandle>().get().person(),
        species,
        corralled,
    )
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
    kit_id: Option<String>,
    take_species: Vec<String>,
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
    let mut target = match role.to_ascii_lowercase().as_str() {
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
                // **WHICH PLANTS THIS CREW CARRIES HOME** (the selective gather) — sorted and
                // deduplicated by the constructor, so the order the player typed cannot reach the
                // snapshot. Empty = the whole basket, the same absent-means-default convention the
                // floor and the commit species have. Its legality is judged below, against this
                // tile's own basket, and **fails closed**.
                take_species: TakeSelection::from_keys(&take_species),
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
        // **The two keeping roles** (`docs/plan_standing_upkeep.md` §2.5) — staffed like any other
        // band-wide role, because that is what they are: their hands are a pool against the summed
        // upkeep of everything this band holds on that web, and `0` stops maintaining the whole web.
        "agriculture" => LaborTarget::Agriculture,
        "husbandry" => LaborTarget::Husbandry,
        // **The third keeping role** (`docs/plan_standing_upkeep.md` §4.13) — the route branch's,
        // staffed exactly like the two above it. What its hands hold is not a source row but the road
        // TILES THIS BAND IS THE KEEPER OF — the ones it graded or paved, wherever the band has since
        // walked (`routes` rule 2; the catchment is the keeper, never the band's own position) — and
        // `0` stops keeping roads at all.
        "roadwork" => LaborTarget::Roadwork,
        // **The builders** (`docs/plan_standing_upkeep.md` §2.5) — one pool for both webs, whose
        // whole output goes on the head of this band's build queue. A verb declares what to raise;
        // this is what raises it, and `0` stops building altogether.
        "builders" => LaborTarget::Builders,
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
        // The two keeping roles ride their web's own verb channel, so a player watching a rung's
        // line sees the hands that hold it move.
        LaborTarget::Agriculture => CommandEventKind::Cultivate,
        LaborTarget::Husbandry => CommandEventKind::Corral,
        // **The road keepers have no web's channel to ride**, because the route branch declares no
        // verb at all — traffic is the crew — so they report on the generic one, as the builders and
        // the warriors do.
        LaborTarget::Roadwork => CommandEventKind::CancelOrder,
        // The builders serve both webs, so they have no web's channel to ride and report on the
        // generic one, as the warriors do.
        LaborTarget::Builders => CommandEventKind::CancelOrder,
    };

    // Stance validation. Unassigning (`workers == 0`) is always allowed — a player must be able to
    // abandon an investment even if its gates have since lapsed. **The improvement's gates are NOT
    // re-run here** (issue #442): this command does not set an improvement, so re-asserting one the
    // band already carries would refuse a crew change on a paused build — the trap §6 removes.
    //
    // **THE TAKE SELECTION IS REPAIRED HERE, NOT REFUSED** — see [`resolve_take_for_ground`]. It
    // rides the same `workers > 0` guard as the gate above and for the same reason: an unassign must
    // never be refused, and a row going to zero gathers nothing for a selection to be wrong about.
    // The names it drops are announced once, beside the applied event at the foot of this function,
    // because a pruned selection is a change the player did not ask for.
    let mut pruned_take: Vec<String> = Vec::new();
    if workers > 0 {
        if let Err(reason) = validate_labor_policy(app, faction, &target) {
            emit_command_failure(app, event_kind, faction, reason);
            return;
        }
        if let LaborTarget::Forage {
            tile, take_species, ..
        } = &target
        {
            match resolve_take_for_ground(app, *tile, take_species) {
                Err(reason) => {
                    emit_command_failure(app, event_kind, faction, reason);
                    return;
                }
                Ok(resolved) => {
                    if !resolved.dropped.is_empty() {
                        pruned_take = resolved.dropped;
                        if let LaborTarget::Forage { take_species, .. } = &mut target {
                            *take_species = resolved.selection;
                        }
                    }
                }
            }
        }
    }

    // **The kit this crew works under, resolved at the command boundary and FAILING CLOSED.** An
    // unknown id, or one whose `jobs` does not cover this role, is refused with a reason rather than
    // quietly becoming the default: naming a kit is how the player compares tiers, so a silent
    // substitution answers a different question than the one asked. **The band-wide roles resolve
    // one too** — `kit_job()` answers for all four roles now, so `assign_labor … scout 3 kit none`
    // is a real selection rather than a token ignored the way `species` and `floor` are on those
    // rows.
    //
    // **Unassigning (`workers == 0`) resolves NO kit**, the same rule the policy validation above
    // follows and for the same reason: a player must be able to abandon an investment even if what
    // it was staffed with has since lapsed. `LaborAllocation::set_assignment` *drops* the assignment
    // at zero workers and never reads the kit, so refusing here refused a command whose kit could
    // not be used either way — and a roster edit that removed an id left every crew still holding it
    // unclearable, locked in by a kit that no longer exists.
    //
    // **A HUNT row with no kit named resolves the HERD's default, not the job's.** The wire
    // publishes that per-herd id (`HerdTelemetryState::default_kit_id`) and the compose sheet opens
    // on it, so resolving the job default here would run Stalking on a warren whose sheet said
    // Trapping — the silent substitution the refusal above exists to prevent, arriving through the
    // absent-token door instead. `default_kits.hunt` stays the answer wherever there is no quarry
    // to score: every other role, and a Hunt row whose herd or species will not resolve.
    //
    // ⛔ **A BUILDERS ROW CARRIES NO KIT AT ALL, AND NAMING ONE IS REFUSED**
    // (`docs/plan_standing_upkeep.md` §4.7a ②). The builders' kit is a property of the **queue
    // entry** — a hoe for a Cultivate, hurdles for a `Tame` — so a single stored id per *band* is
    // the one thing the derivation cannot express. It was an override that won permanently: one
    // pick pinned the animal web's tool onto every later plant build with no way back, and `none`
    // (bare-handed) is a different statement, not an undo. `build_kit <faction> <source…> kit <id>`
    // is where the override lives, one job at a time.
    //
    // **Refused rather than ignored.** A token the command silently drops is the same class of
    // defect as the one this replaces — the player names a tool and the sim does something else —
    // so the row says so by name.
    //
    // The fork is **here and not in `default_kit_for_target`**, because the question it answers is
    // *"what does this command STORE"*, not *"which kit is the absent one"*: that helper returns a
    // resolved `KitChoice` for the raid path too, and widening it to an `Option` would push the
    // absent-means-derive case into two call sites that have no derivation to defer to. Only the
    // builders arm is touched — the other six roles' stored default is load-bearing (the wire and
    // the turn both read the row's kit for them, and there is nothing per-entry to derive).
    //
    // **AND THE TWO KEEPING ROLES ARE THE SAME RULE, ONE ACCOUNT OVER**
    // (`docs/plan_standing_upkeep.md` §2.7). A keeping kit is a property of the **work site**, not of
    // the band: the `agriculture` / `husbandry` rows say how many keepers a web gets, and
    // `upkeep_kit <faction> <source…> kit <id>` says what the keepers of one site carry. A kit stored
    // here reached the split through `LaborAllocation::named_kit_on` until §2.7 and reaches nothing
    // now, so accepting the token would be the worst version of the defect the builders row had —
    // the player names a tool, the sim stores it, and no keeper anywhere picks it up.
    //
    // **Refused rather than ignored**, for the builders row's reason: a token the command silently
    // drops is the same class of defect as the one this replaces.
    //
    // The rules below are separate and reach the same store, so they are named separately and OR'd
    // rather than written as two arms of one `if` — which is the same block twice, and reads as an
    // accident.
    let unstaffing = workers == 0;
    let staffing_a_standing_pool = matches!(
        target,
        LaborTarget::Builders
            | LaborTarget::Agriculture
            | LaborTarget::Husbandry
            | LaborTarget::Roadwork
    );
    if staffing_a_standing_pool && kit_id.is_some() {
        // **AND THE ROAD KEEPERS ARE THE SAME RULE WITH NO OVERRIDE TO POINT AT.** A road is not a
        // work site the player holds — it is owned by nobody — so there is no `upkeep_kit <source…>`
        // that could name one, and the kit is derived from the roster alone
        // (`EquipmentConfig::keeping_kit_for` at `RungBranch::Route`, today the bare `none`).
        // **Refused rather than ignored**, for the builders row's reason: a token the command
        // silently drops is the same class of defect as the one this rule replaces.
        let refusal = match target {
            LaborTarget::Builders => "the builders kit is set per queue entry — use `build_kit                                       <faction> <source…> kit <id>`"
                .to_string(),
            LaborTarget::Roadwork => "a road has no keeper's kit to name — the roadwork kit is                                       derived from the roster"
                .to_string(),
            _ => "the keeping kit is set per work site — use `upkeep_kit <faction> <source…> kit                   <id>`"
                .to_string(),
        };
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!("assign_labor: {refusal}."),
        );
        return;
    }
    let crew_kit = if unstaffing || staffing_a_standing_pool {
        None
    } else {
        let equipment_cfg = app.world.resource::<EquipmentConfigHandle>().get();
        let absent = default_kit_for_target(app, &equipment_cfg, &target);
        match equipment_cfg.resolve_kit_or(kit_id.as_deref(), target.kit_job(), absent) {
            Ok(kit) => Some(kit),
            Err(reason) => {
                emit_command_failure(app, event_kind, faction, format!("assign_labor: {reason}."));
                return;
            }
        }
    };

    let Some(band) = select_starting_band(app, faction, band_id, "assign_labor", event_kind) else {
        return;
    };

    // **The bench's crew is off the table.** `Make` draws idle workers onto a recipe, so a band with
    // four hands at the bench has four fewer to send anywhere — see [`BandWorkforce::assignable`],
    // which nets the bench out for the command path and the published `idleWorkers` alike. Not
    // modelled as a `LaborTarget` because a bench is not an in-range source, and giving it one would
    // put a fictitious row on every yield readout in the game.
    let available = band_workforce(app, band.entity).assignable();

    // **"IS THERE STILL ANYTHING OF OURS HERE?"**, asked before the allocation is borrowed
    // (`docs/plan_standing_upkeep.md` §2.2). A source row survives losing its take crew — it is the
    // band's *holding*, and the keeping pool funds what a band holds, not what it happens to be
    // gathering — but a wild stand nobody has built anything on is not a holding, so unstaffing one
    // clears its row here rather than leaving a `+0.00` row for the turn to sweep up. The same
    // predicate retires a holding whose meter has finally rotted away, inside `advance_labor_allocation`.
    let source_holds_something = {
        let forage_registry = app.world.resource::<ForageRegistry>();
        let herds = app.world.resource::<HerdRegistry>();
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        core_sim::source_has_a_meter_at_risk(&target, forage_registry, herds, &ladder)
    };

    let kind_label = target.kind();
    let (applied, assigned_total) = {
        let mut allocation = band_allocation_mut(app, band.entity);
        let applied = allocation.set_assignment(target.clone(), workers, available, crew_kit);
        // **Nothing built, nothing DECLARED, nobody on it — the band's business here is over.**
        // The **queue entry** is part of the test: a `Cultivate` declared this turn has no progress
        // on its meter yet, so dropping the row on the ground's answer alone would abandon a build
        // the player had just ordered (`docs/plan_standing_upkeep.md` §2.5 — an entry requires a
        // row, so dropping the row would drop the entry with it).
        let queued = BuildSource::of(&target)
            .is_some_and(|source| allocation.build_queue_position(&source).is_some());
        if applied == 0 && !source_holds_something && !queued {
            allocation.drop_source_row(&target);
        }
        (applied, allocation.assigned_total())
    };
    // The seed must price the dip of whatever this band has **queued** here, rather than the
    // undipped stance — a build in flight is the queue's, not the row's. Read after the allocation
    // borrow is released, because resolving the declaration against the ground needs both webs'
    // registries.
    let improvement = build_verb_on_source_at(app, band.entity, &target);
    // Show the source's expected yield immediately (workers added/removed OR stance changed — every
    // shape of this command that moves the number): without the seed the row reads `+0.00` until the
    // player advances a turn.
    seed_source_yield(app, band.entity, &target, improvement, applied);

    let tick = app.world.resource::<SimulationTick>().0;
    // **ONE LINE FOR THE NAMES THE GROUND TOOK OFF THE SELECTION**, ahead of the applied row it
    // explains. It is stated because the player did not ask for it; it is stated *briefly* because
    // the row underneath already says what the crew now carries.
    if !pruned_take.is_empty() {
        let names = pruned_take.join(", ");
        // **`band=` NAMES THE WORK BOARD THIS NARROWING HAPPENED ON** — the token the event dock's
        // per-row *"Work tab"* link reads (`systems::labor::band_detail_token` says why it is the
        // durable `BandId` and never the entity). Read off the selected band rather than the
        // command's own argument, because that argument is absent whenever the player let the
        // default-band picker choose.
        //
        // **It sits BEFORE `dropped=`, and it has to.** Detail tokens are space-delimited
        // `key=value`, so the comma-joined display names can only be the trailing remainder
        // (`.claude/rules/core_sim/event-feed.md`) — anything appended after them is swallowed by
        // that value.
        let band_token = app
            .world
            .get::<BandId>(band.entity)
            .map(|id| format!(" band={}", id.0))
            .unwrap_or_default();
        push_command_event(
            app,
            tick,
            event_kind,
            faction,
            format!("{} no longer stands here — dropped from the take", names),
            Some(format!(
                "status=pruned reason=not_here role={}{} dropped={}",
                kind_label, band_token, names
            )),
        );
    }
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
    // **The band is the bound, and the only one** (`docs/plan_denial_raid.md` §3.1). The config's
    // party lever is a *sampling* bound on the pre-launch estimate tables, never a rule about what
    // may be sent: you cannot detach workers you do not have, and you may detach all the ones you do.
    let max_party = available_workers(band_working);
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
    // The scout's launch larder is food leaving the band with the party — the same food-ledger
    // transfer term a shipment's cargo takes, and it comes back on the fold-back.
    if let Some(mut allocation) = app.world.get_mut::<LaborAllocation>(band.entity) {
        allocation.last_transfer_sent += drawn.to_f32();
    }

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
            // **The party leaves OUTFITTED** — a detached party is a band in its own right, so it
            // carries the same full kit a band spawns with, and `advance_expeditions` then **wears**
            // it: the hunting kit per animal killed and the sled per unit hauled, charged on a
            // scout's opportunistic roadside kill exactly as on a raid's take
            // (`.claude/rules/core_sim/equipment.md`). Without the component there would be nothing
            // for that wear to land on, and the party would publish a dry kit beside an equipped
            // haul rate — a contradiction on the wire. **Stated rather than defaulted**: an absent
            // ledger entry means NOT OWNED since the count slice, so `Default` would send the party
            // out bare-handed.
            outfitted_party_equipment(app, party_workers),
            StartingUnit::new(unit_kind, unit_tags),
            Expedition {
                home_band: band.entity,
                mission: ExpeditionMission::Scout,
                phase: ExpeditionPhase::Outbound,
                announced: false,
                pending_reveal: Vec::new(),
                pending_contacts: Default::default(),
                // An outfitted party leaves with an empty trade pack — it earns its pelts in the
                // field (`advance_expeditions`).
                // **A scout carries the HUNT job's default kit.** `send_expedition` names no kit —
                // scouting is not a kit job — but a scout's opportunistic roadside kill resolves
                // through the very same hunt seams, so it needs a real mask rather than a hole.
                kit: app
                    .world
                    .resource::<EquipmentConfigHandle>()
                    .get()
                    .default_kit(KitJob::Hunt),
                // A scout carries no shipment — the cargo store is the trade verb's.
                cargo: LocalStore::new(),
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

/// **Everything outfitting a raiding party needs, once its orders are known to be legal** — the band
/// it comes off, the herd it is aimed at, and the template it is cloned from.
///
/// Shared by the two raiding verbs, [`handle_send_hunt_expedition`] and [`handle_send_denial_raid`],
/// which differ only in the mission they name, the numbers they validate and the verdict they quote
/// — never in how a party is drawn off a band. Keeping that half in one place is what stops the third
/// verb (`docs/plan_denial_raid.md` §3) from acquiring its own copy of the resident-band gate, the
/// party-size bound and the herd lookup.
/// **The half of outfitting that has nothing to do with the mission** — a real *resident* band, a
/// legal party size, and the template the detached party is cloned from.
///
/// Split out of [`OutfittedParty`] so a **fourth** verb (the trade expedition, which names a band
/// rather than a herd) could reuse the resident-band gate and the party bound without acquiring its
/// own copy of them — the exact reason the raid seam was extracted for the third.
struct OutfittedBand {
    band: SelectedBand,
    /// The home band's cohort, cloned as the detached party's template.
    cohort: PopulationCohort,
    unit_kind: String,
    unit_tags: Vec<String>,
}

/// Validate the band half of a launch order: a real **resident** band and a **legal party size**.
/// Emits its own `ExpeditionSent` failure event and answers `None` on any refusal, so a caller
/// holding a `Some` has nothing left to check about the band.
///
/// `verb` names the command in the refusal text, so every verb reads as itself.
fn outfit_detached_party(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_id: Option<u64>,
    party_workers: u32,
    verb: &str,
) -> Option<OutfittedBand> {
    let band = select_starting_band(
        app,
        faction,
        band_id,
        verb,
        CommandEventKind::ExpeditionSent,
    )?;
    // Same resident-band gate as `send_expedition`: a party can only be outfitted from a real band.
    if app.world.get::<ResidentBand>(band.entity).is_none() {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            format!("{verb}: band is not a resident band."),
        );
        return None;
    }

    // **A band with no cohort REFUSES LOUDLY.** It is unreachable through the gate above —
    // `select_starting_band` resolves a band by its cohort — but a bare `?` here answered `None`
    // with no feed entry at all, so the command would vanish while every other refusal in this
    // function published a reason. A command log that can drop an order silently is worse than one
    // that reports an impossible state.
    let Some(cohort) = app.world.get::<PopulationCohort>(band.entity).cloned() else {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            format!("{verb}: {} has no population to outfit from.", band.label),
        );
        return None;
    };
    // **The band is the bound, and the only one** — see `handle_send_expedition`, and
    // `ExpeditionConfig::estimate_party_sizes` for the lever that used to also live here.
    let max_party = available_workers(cohort.working);
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
        return None;
    }

    let (unit_kind, unit_tags) = app
        .world
        .get::<StartingUnit>(band.entity)
        .map(|unit| (unit.kind.clone(), unit.tags.clone()))
        .unwrap_or_else(|| ("expedition".to_string(), Vec::new()));
    Some(OutfittedBand {
        band,
        cohort,
        unit_kind,
        unit_tags,
    })
}

struct OutfittedParty {
    band: SelectedBand,
    /// The herd's live tile, captured as the party's initial travel target.
    herd_pos: UVec2,
    /// **The herd's species display name**, captured in the same lookup that proved the herd live and
    /// carried onto the mission (`ExpeditionMission::target_species`). This is the only moment the
    /// name is guaranteed available — the gate below refuses a launch the registry cannot resolve, and
    /// the herd may be gone from both the registry and the published telemetry long before the party
    /// stops being bound to it (issue #378).
    herd_species: String,
    /// The home band's cohort, cloned as the detached party's template.
    cohort: PopulationCohort,
    unit_kind: String,
    unit_tags: Vec<String>,
}

/// Validate a raiding order and gather what launching it needs: a real **resident** band, a **live**
/// herd, and a **legal party size**. Emits its own `ExpeditionSent` failure event and answers `None`
/// on any refusal, so a caller holding a `Some` has nothing left to check.
///
/// `verb` names the command in the refusal text, so the two verbs read as themselves.
fn outfit_raiding_party(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_id: Option<u64>,
    party_workers: u32,
    fauna_id: &str,
    verb: &str,
) -> Option<OutfittedParty> {
    // The band half — the resident-band gate, the party bound and the cohort template — is
    // `outfit_detached_party`'s, shared with the trade verb. What is left here is the herd.
    let OutfittedBand {
        band,
        cohort,
        unit_kind,
        unit_tags,
    } = outfit_detached_party(app, faction, band_id, party_workers, verb)?;

    // The target must resolve to a live herd; capture its current tile as the initial travel target
    // and its species as the name the party will be known by for the rest of its life.
    let target = {
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .find(fauna_id)
            .map(|herd| (herd.position(), herd.species.clone()))
    };
    let Some((herd_pos, herd_species)) = target else {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            format!("{verb}: no live herd '{fauna_id}'."),
        );
        return None;
    };

    Some(OutfittedParty {
        band,
        herd_pos,
        herd_species,
        cohort,
        unit_kind,
        unit_tags,
    })
}

/// **What a detached party leaves outfitted with** — one unworn unit of every item some kit
/// carries, exactly as a band spawns.
///
/// One helper for both outfitting paths, because *"a party leaves outfitted"* is one fact: two
/// call sites reaching for the ledger separately is how one of them ends up sending a bare-handed
/// raid out under a kitted forecast.
///
/// **Sized to the party that leaves**, because a unit arms one person: a raid of ten sent out with
/// one spear is nine bare hands, which is neither what the launch line quotes nor what "outfitted"
/// means. Every worker in the party is a hunter, so the head count *is* the worker count here.
fn outfitted_party_equipment(app: &bevy::prelude::App, party_workers: u32) -> BandEquipment {
    BandEquipment::start_stocked_owned(
        &app.world.resource::<EquipmentConfigHandle>().get(),
        &app.world.resource::<RecipesConfigHandle>().get(),
        &app.world.resource::<MaterialsConfigHandle>().get(),
        party_workers as f32,
    )
}

/// **The party a launch forecast is quoted for** — the kit the player is sending it with, over a
/// **fresh** set of components ([`BandEquipment::default`] is zero wear), because the party leaves
/// outfitted and that is the tier it will fight its first turns at. Wear is what moves it later, and
/// the in-flight readouts re-quote against the party's live kit each turn.
///
/// **Quoted at the CHOSEN kit, not at "equipped"** — a raid sent out bare-handed must be quoted
/// bare-handed, or the launch line promises a slaughter the party cannot perform.
///
/// **It takes the QUARRY'S MASS** because a mass-bounded weapon is only a weapon against animals it
/// can hold: a raid sent with traps after a mammoth must be quoted at the bare hand's attack, which
/// is the gate refusing the raid — the same answer the take will give.
fn launch_forecast_party(
    app: &bevy::prelude::App,
    kit: &KitChoice,
    quarry_body_mass: f32,
) -> HuntingParty {
    let equipment_cfg = app.world.resource::<EquipmentConfigHandle>().get();
    let combat = app.world.resource::<CombatConfigHandle>().get();
    // A fresh ledger: the launch line quotes the KIT the party is being sent with, before it has worn
    // any of it. The party's own wear then moves its tiers turn by turn once it is in flight.
    let fresh = BandEquipment::start_stocked(&equipment_cfg);
    // **UNIFORM**: the party leaves *outfitted* — `outfitted_party_equipment` stocks a party's
    // worth of each item, sized to the head count being sent — so every hunter is holding the kit
    // the player named. Quoting coverage against the one-unit reference ledger would price a raid
    // of ten at one armed hunter and nine bare hands, which is not the party that will leave.
    HuntingParty::uniform(
        equipment_cfg.hunter_profile_against(
            app.world.resource::<CreaturesConfigHandle>().get().person(),
            kit,
            &fresh,
            quarry_body_mass,
        ),
        combat.expedition_tuning(),
        combat.hunt_injury_damage_per_animal * equipment_cfg.exposure(kit, &fresh),
        equipment_cfg.dispersion(kit, &fresh),
    )
}

/// **The per-hunter haul rate the same launch forecast is quoted at** — the chosen kit's *sled*
/// tier over a fresh set of components, the twin of [`launch_forecast_party`]'s attack tier. Both
/// halves have to move together: quoting a bare-handed fight against a kitted haul would promise a
/// party that kills nothing and drags it home fast.
fn launch_forecast_haul(app: &bevy::prelude::App, kit: &KitChoice) -> f32 {
    let equipment_cfg = app.world.resource::<EquipmentConfigHandle>().get();
    let baseline_rate = app
        .world
        .resource::<LaborConfigHandle>()
        .get()
        .hunt
        .per_worker_biomass_capacity;
    let fresh = BandEquipment::start_stocked(&equipment_cfg);
    equipment_cfg.hunt_per_worker_biomass_capacity(baseline_rate, kit, &fresh)
}

/// Resolve the kit a raiding verb was given, or refuse the launch with a reason.
///
/// **Fails closed, exactly as the floor does.** An unknown id, or one whose `jobs` does not cover
/// `hunt`, is a command failure — never a silent fall back to the default, because a party quietly
/// re-armed is the opposite of the comparison the player asked for.
///
/// **Absent = the TARGET HERD's default, not the job's** — the same `default_kit_for_target` seam
/// `handle_assign_labor` resolves through, keyed on the herd this raid names. Both verbs are quoted
/// against tables the wire priced at that herd's own kit (`huntTripEstimatesKitId` /
/// `denialEstimatesKitId`, which are `defaultKitId` by construction), and the client's launch sheet
/// reads `defaultKitId`; resolving `default_kits.hunt` here would launch a party on a different kit
/// than the forecast the player committed from — the silent substitution the refusal above exists to
/// prevent, arriving through the absent-token door.
fn resolve_raid_kit(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    verb: &str,
    kit_id: Option<&str>,
    fauna_id: &str,
) -> Option<KitChoice> {
    let equipment_cfg = app.world.resource::<EquipmentConfigHandle>().get();
    // A target built to NAME the herd, never to state an assignment — hence the floor constant.
    let absent = default_kit_for_target(
        app,
        &equipment_cfg,
        &LaborTarget::Hunt {
            fauna_id: fauna_id.to_string(),
            floor: SOURCE_NAMED_NOT_ASSIGNED,
        },
    );
    let resolved = equipment_cfg.resolve_kit_or(kit_id, KitJob::Hunt, absent);
    match resolved {
        Ok(kit) => Some(kit),
        Err(reason) => {
            emit_command_failure(
                app,
                CommandEventKind::ExpeditionSent,
                faction,
                format!("{verb}: {reason}."),
            );
            None
        }
    }
}

/// **The round-trip walk**, in turns, from the launching band's tile out to the herd and back — the
/// half of a trip's length the band-agnostic forecasts cannot see. `hunt_trip_forecast` /
/// `denial_forecast` count only the turns spent working the herd once in reach, so the walk is added
/// here, where the launching band's tile is known. (The per-herd snapshot tables are band-agnostic —
/// one row serves every band — so the **client** adds this same travel from the selected band's tile.)
/// Decimal places the denial launch line prints its payload and waste to — one, because the sheet is
/// quoting an approximation ("~") of a whole raid and a second digit would read as precision the
/// projection does not have. The `detail` twin keeps the finer `{:.2}` a machine reader wants.
const DENIAL_LEDGER_DECIMALS: usize = 1;

/// Decimal places the same line prints a **material** haul to — finer than the food figure beside
/// it, for the reason `systems::expeditions::HAUL_MATERIAL_DECIMALS` is finer than a whole count: a
/// raid can honestly come home with a *fraction* of a hide, and a one-digit readout would print
/// `~0.0` over a pack that really did bank pelts — the exact `~0.0` the omission rule below exists
/// to prevent.
const DENIAL_LEDGER_MATERIAL_DECIMALS: usize = 2;

/// **What a denial raid brings home and what it leaves — food AND materials.**
///
/// A Grey Wolf Pack pays `provisions_per_biomass == 0`, so a food-only line reads *"~0.0 food home,
/// ~0.0 left on the range"* on exactly the raid whose waste is total, and says nothing at all about
/// the hides that raid really banks. [`core_sim::DenialForecast::delivered_material`] states that
/// haul per material, so this line states it too — **the client's own take line
/// (`SourceForecast.denial_take_bbcode`) reads that same field off that same forecast**, and the
/// server's sentence and the client's must not disagree about one raid. The material id is printed
/// verbatim; `materials.json` authors no display name, and the client resolves the same key.
///
/// **A component with nothing on either side of it is omitted rather than printed as `~0.0`**, the
/// `describe_haul` rule: a zero there is a fact about the species, not about this raid — and an
/// empty `delivered_material` is *"no row"*, never a zero, so nothing has to special-case it.
///
/// **The waste stays food-only, deliberately.** `DenialForecast` carries no `wasted_material` and is
/// not to grow one: Ray ruled the per-material waste out of scope (the waste is already legible as a
/// percentage, so a second reading buys nothing), and a flat "wasted materials" scalar would be the
/// retired trade axis under a new name. See `DenialForecast::delivered_material`'s own comment.
fn describe_denial_ledger(forecast: &core_sim::DenialForecast) -> String {
    let mut ledger = Vec::new();
    if forecast.delivered_food > 0.0 || forecast.wasted_food > 0.0 {
        ledger.push(format!(
            "~{:.*} food home, ~{:.*} left on the range",
            DENIAL_LEDGER_DECIMALS,
            forecast.delivered_food,
            DENIAL_LEDGER_DECIMALS,
            forecast.wasted_food
        ));
    }
    // One clause per material, never a sum: a total of hide and bone is the retired trade axis under
    // a new name. Same grammar as the food clause above, so the two read as one ledger.
    for payoff in &forecast.delivered_material {
        ledger.push(format!(
            "~{:.*} {} home",
            DENIAL_LEDGER_MATERIAL_DECIMALS, payoff.amount, payoff.material
        ));
    }
    if ledger.is_empty() {
        // Neither meat nor material: the raid destroys animals and genuinely brings nothing back.
        return "nothing worth hauling from this quarry".to_string();
    }
    ledger.join("; ")
}

fn round_trip_travel_turns(
    app: &bevy::prelude::App,
    band: bevy::prelude::Entity,
    herd_pos: UVec2,
) -> u32 {
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
        .get::<PopulationCohort>(band)
        .map(|c| c.current_tile)
        .and_then(|t| app.world.get::<Tile>(t))
        .map(|tile| {
            let one_way =
                hex_distance_wrapped(tile.position, herd_pos, grid_width, wrap_horizontal);
            (2 * one_way).div_ceil(move_rate)
        })
        .unwrap_or(0)
}

/// Draw `party_workers` off the home band and spawn the detached party on `mission`, heading for the
/// herd's live tile. Draws **no** provisions — a raiding party lives off its kills — and leaves
/// **outfitted**, because a detached party is a band in its own right and carries the same full kit
/// one spawns with (which `advance_expeditions` then wears per animal killed and per unit hauled).
///
/// **`None` when the home band's cohort is gone, and the caller must publish a FAILURE.** It used to
/// answer `Entity::PLACEHOLDER`, which the callers then stamped into a
/// `status=applied mission=… expedition=<placeholder bits>` feed line — *"the order worked"* for a
/// party that was never spawned. That is the one shape a command log must never produce, so the
/// impossible state is now a refusal rather than a sentinel entity.
fn launch_detached_party(
    app: &mut bevy::prelude::App,
    outfit: OutfittedParty,
    party_workers: u32,
    mission: ExpeditionMission,
    // **The kit the party is sent out with, resolved ONCE here.** It rides the `Expedition` for the
    // party's whole life and is never re-resolved against the home band's later stock — a party sent
    // out bare would otherwise silently re-arm the moment the band's spears were counted again.
    kit: KitChoice,
) -> Option<bevy::prelude::Entity> {
    let OutfittedParty {
        band,
        herd_pos,
        // Not read here: the caller has already spent it composing the `mission` argument, which is
        // where the name lives for the party's life.
        herd_species: _,
        cohort,
        unit_kind,
        unit_tags,
    } = outfit;
    launch_party_from_band(
        app,
        OutfittedBand {
            band,
            cohort,
            unit_kind,
            unit_tags,
        },
        party_workers,
        mission,
        // A raid leaves hunting, aimed at its herd, with an empty pack and no provisions — it lives
        // off its kills.
        LaunchOrders {
            phase: ExpeditionPhase::Hunting,
            target: herd_pos,
            provisions: Scalar::from_i64(0),
            cargo: LocalStore::new(),
        },
        kit,
    )
}

/// **What the launch below needs that the band and the mission do not say** — where the party is
/// pointed, what phase it starts in, and what it leaves loaded with.
///
/// Bundled rather than passed as four parameters because they are one decision per verb: a raid
/// leaves hunting with nothing, a shipment leaves outbound with a drawn larder and a cargo.
struct LaunchOrders {
    phase: ExpeditionPhase,
    target: UVec2,
    /// Provisions **already drawn** from the home band and handed to the party's own pack. A raid
    /// draws none.
    provisions: Scalar,
    /// The shipment the party is carrying, a store of its own — never merged into the pack above,
    /// or a hungry party would eat what it was sent to deliver.
    cargo: LocalStore,
}

/// **THE detached-party spawn**, shared by every verb that draws a party off a band: it removes the
/// workers from the band's pool, retasks the cloned cohort, allocates the party's own `BandId` and
/// spawns it outfitted.
///
/// **`None` when the home band's cohort is gone, and the caller must publish a FAILURE** — see
/// [`launch_detached_party`]'s note on the placeholder entity that used to be answered instead.
fn launch_party_from_band(
    app: &mut bevy::prelude::App,
    outfit: OutfittedBand,
    party_workers: u32,
    mission: ExpeditionMission,
    orders: LaunchOrders,
    kit: KitChoice,
) -> Option<bevy::prelude::Entity> {
    let OutfittedBand {
        band,
        mut cohort,
        unit_kind,
        unit_tags,
    } = outfit;
    let party_scalar = Scalar::from_u32(party_workers);
    {
        // Nothing is spawned and nothing is drawn — the caller refuses instead. Bail BEFORE the
        // spawn below, so a refusal cannot leave a party standing with no home band.
        let mut band_cohort = app.world.get_mut::<PopulationCohort>(band.entity)?;
        band_cohort.working -= party_scalar;
        band_cohort.sync_size();
    }

    // Retask the cloned cohort into a detached party co-located with the band, carrying only the
    // provisions its verb drew for it.
    cohort.children = Scalar::from_i64(0);
    cohort.working = party_scalar;
    cohort.elders = Scalar::from_i64(0);
    cohort.stores = LocalStore::new();
    if orders.provisions > Scalar::from_i64(0) {
        cohort.stores.add(FOOD, orders.provisions);
    }
    cohort.age_turns = 0;
    cohort.migration = None;
    cohort.grievance = Scalar::from_i64(0);
    cohort.sync_size();

    // A detached party is a band in its own right, so it takes its own durable id.
    let expedition_band_id = app.world.resource_mut::<BandIdAllocator>().allocate();
    let expedition_entity = app
        .world
        .spawn((
            cohort,
            expedition_band_id,
            LaborAllocation::default(),
            // **Outfitted, stated rather than defaulted** — see the scout's spawn above.
            outfitted_party_equipment(app, party_workers),
            StartingUnit::new(unit_kind, unit_tags),
            Expedition {
                home_band: band.entity,
                mission,
                phase: orders.phase,
                announced: false,
                pending_reveal: Vec::new(),
                pending_contacts: Default::default(),
                kit,
                cargo: orders.cargo,
            },
            BandTravel {
                target: orders.target,
            },
        ))
        .id();
    Some(expedition_entity)
}

/// Outfit and launch a hunting expedition (PR 2): draw `party_workers` off the resolved home band
/// and send a detached party to follow the herd `fauna_id` at the escapement `floor` it names. Text
/// form:
/// `send_hunt_expedition <faction> <band> <party_workers> <fauna_id> [floor] [kit <id>]`.
#[allow(clippy::too_many_arguments)] // every launch order the verb accepts is a parameter
fn handle_send_hunt_expedition(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_id: Option<u64>,
    party_workers: u32,
    fauna_id: String,
    floor: Option<f32>,
    kit_id: Option<String>,
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
    // **The kit fails closed too** — resolved before the party is drawn off the band, so a bad kit
    // id refuses the launch outright rather than sending a party at a tier nobody named.
    let Some(kit) = resolve_raid_kit(
        app,
        faction,
        "send_hunt_expedition",
        kit_id.as_deref(),
        &fauna_id,
    ) else {
        return;
    };
    let Some(outfit) = outfit_raiding_party(
        app,
        faction,
        band_id,
        party_workers,
        &fauna_id,
        "send_hunt_expedition",
    ) else {
        return;
    };

    let cfg = app.world.resource::<ExpeditionConfigHandle>().get();
    // Launch-time viability forecast — a bounded forward SIMULATION of the trip (`hunt_trip_forecast`),
    // not a division. A party at the food peak skims the herd's Maximum Sustainable Yield (a *flow*),
    // and a deeper floor eats *stock* headroom and then falls back to the regrowth trickle once it is
    // gone, so filling a carry cap off a small herd can genuinely take dozens of turns. That is
    // ecologically true, not a bug; the player must be told at launch rather than silently trapped,
    // so the forecast rides the `ExpeditionSent` feed entry (it still launches either way).
    let forecast = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        // **Quoted at the kit the party is being sent with**, both halves: the fight through
        // `party` and the haul through `per_worker_haul`.
        let per_worker_haul = launch_forecast_haul(app, &kit);
        let registry = app.world.resource::<HerdRegistry>();
        registry.find(&fauna_id).map(|herd| {
            // Resolved INSIDE the herd lookup: the attack tier is a fact about this party against
            // THIS animal, not about the party alone.
            let party = launch_forecast_party(app, &kit, herd.body_mass);
            hunt_trip_forecast(
                party_workers,
                herd,
                floor,
                &fauna,
                per_worker_haul,
                &cfg,
                &party,
            )
        })
    };
    let travel_turns = round_trip_travel_turns(app, outfit.band.entity, outfit.herd_pos);
    // The raid always completes in bounded turns (grab the surplus, come home), so the only genuine
    // non-viable case is "no surplus to take" — the herd is at/below the policy's floor and delivers
    // NO animals. Otherwise headline the payload the raid actually lands, including the round trip.
    let (viability_note, viability_detail) = match &forecast {
        // An INEDIBLE quarry brings no food home — say what it *does* bring, no food ETA. This arm
        // used to fire for a denial *mission* (Eradicate); since #337 the policy is pure intensity
        // and the species decides the product, so a floor-`0` raid on a deer reports its windfall
        // like any other rung, and only a wolf lands here.
        Some(_f) if !_f.delivers_food => (
            " — no food from this quarry: the party brings back hides and bone, not meat"
                .to_string(),
            " eta_turns=none viability=inedible".to_string(),
        ),
        // The herd has no surplus above the policy's floor — the honest non-viable case. "Too lean"
        // now means the raid lands NO food at all (a small party on a big animal still delivers a
        // partial with waste, so the signal is `delivered_food == 0`, not "the party is too small").
        Some(f) if f.delivered_food <= 0.0 => (
            format!(
                " — the {} is too lean to raid: at its {} floor it has no surplus, the party would \
                 return empty",
                fauna_id, floor
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
                             wasted={:.2} bound={}",
                            total,
                            hunt_turns,
                            travel_turns,
                            animals,
                            food,
                            wasted,
                            f.bound.as_str()
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
                        " eta_turns=none travel_turns={} animals={} food={:.2} wasted={:.2} bound={}",
                        travel_turns,
                        animals,
                        food,
                        wasted,
                        f.bound.as_str()
                    ),
                ),
            }
        }
        None => (String::new(), String::new()),
    };

    let band_label = outfit.band.label.clone();
    // Read off the outfit BEFORE it is moved into the launch, for the same reason `band_label` is.
    let target_species = outfit.herd_species.clone();
    let mission = ExpeditionMission::Hunt {
        fauna_id: fauna_id.clone(),
        target_species,
        floor,
    };
    // **The event line names the species; the `herd=` token below keeps the id.** Read off the
    // mission rather than off `outfit.herd_species` so this line and every later line about the
    // same party resolve the name one way (`ExpeditionMission::target_display`).
    let target_display = mission.target_display().to_string();
    // **A launch that did not happen publishes a FAILURE, never an `applied` line** — see
    // `launch_detached_party`, which answers `None` rather than a placeholder entity.
    let Some(expedition_entity) =
        launch_detached_party(app, outfit, party_workers, mission, kit.clone())
    else {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            format!("send_hunt_expedition: {band_label} has no population to outfit from."),
        );
        return;
    };

    let tick = app.world.resource::<SimulationTick>().0;
    push_command_event(
        app,
        tick,
        CommandEventKind::ExpeditionSent,
        faction,
        format!(
            "{} hunting expedition (floor {:.2}·K) -> {}{}",
            band_label, floor, target_display, viability_note
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

/// Outfit and launch a **denial raid** (`docs/plan_denial_raid.md`) — the third expedition verb,
/// beside Scout and Hunt. Text form:
/// `send_denial_raid <faction> <band> <party_workers> <fauna_id>`.
///
/// **There is no floor to validate and no fill target to default**, which is why this handler is
/// shorter than its hunting sibling rather than a copy of it: the mission carries no numbers, so the
/// order is *"this herd, this many people"* and the only refusals are the shared ones
/// ([`outfit_raiding_party`]). `floor` appears nowhere in the command, the feed line or the detail.
///
/// **The verdict is `turns_to_collapse`, not a food total** (§1.1): a raid succeeds by pushing the
/// herd under `ecology.collapse_fraction` — the point of no return — and walking away, and what it
/// hauls home is a rounding error against what it killed. When the party cannot get there at all,
/// the line **says so** rather than showing a blank (§3).
fn handle_send_denial_raid(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_id: Option<u64>,
    party_workers: u32,
    fauna_id: String,
    kit_id: Option<String>,
) {
    // **The one order this mission still takes, and it fails closed like the hunt's.** A denial raid
    // carries no floor and no fill target, but the party still has to be sent with *something*.
    let Some(kit) = resolve_raid_kit(
        app,
        faction,
        "send_denial_raid",
        kit_id.as_deref(),
        &fauna_id,
    ) else {
        return;
    };
    let Some(outfit) = outfit_raiding_party(
        app,
        faction,
        band_id,
        party_workers,
        &fauna_id,
        "send_denial_raid",
    ) else {
        return;
    };

    let cfg = app.world.resource::<ExpeditionConfigHandle>().get();
    let forecast = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        // The reported band's width (`combat_config.forecast_range_sigmas`) — a readout lever, so
        // widening it cannot move an animal.
        let range_sigmas = app
            .world
            .resource::<CombatConfigHandle>()
            .get()
            .forecast_range_sigmas;
        // Quoted at the kit the raid is being sent with — the verdict rests on kills, which the
        // fight owns, so a bare-handed raid is told it cannot do the job rather than promised it can.
        let per_worker_haul = launch_forecast_haul(app, &kit);
        let registry = app.world.resource::<HerdRegistry>();
        registry.find(&fauna_id).map(|herd| {
            // Resolved INSIDE the herd lookup: the attack tier is a fact about this party against
            // THIS animal, not about the party alone.
            let party = launch_forecast_party(app, &kit, herd.body_mass);
            denial_forecast(
                party_workers,
                herd,
                &fauna,
                per_worker_haul,
                &cfg,
                &party,
                range_sigmas,
            )
        })
    };
    let travel_turns = round_trip_travel_turns(app, outfit.band.entity, outfit.herd_pos);
    let (verdict, verdict_detail) = match &forecast {
        Some(f) => {
            let waste = describe_denial_ledger(f);
            match f.turns_to_collapse {
                Some(turns) => (
                    // **The range, not a promise** (`docs/plan_hunt_through_combat.md` §6.4) — the
                    // retreat is stochastic, so the verdict is a band. It collapses to one number
                    // when the distribution is degenerate, which the client reads by comparing the
                    // three; here the prose does the same.
                    format!(
                        " — past recovery in {} ({} raiding + {} travel); {} animals killed, {}",
                        describe_collapse_window(
                            f.turns_to_collapse_low,
                            turns,
                            f.turns_to_collapse_high
                        ),
                        turns,
                        travel_turns,
                        f.animals_killed,
                        waste
                    ),
                    format!(
                        " outcome={} turns_to_collapse={} low={} high={} travel_turns={} \
                         animals={} food={:.2} wasted={:.2}",
                        f.outcome.as_str(),
                        turns,
                        f.turns_to_collapse_low.unwrap_or(0),
                        f.turns_to_collapse_high.unwrap_or(0),
                        travel_turns,
                        f.animals_killed,
                        f.delivered_food,
                        f.wasted_food
                    ),
                ),
                // **Never a blank** (§3). A party whose kills cannot outpace the herd's regrowth is
                // told exactly that; one that is merely slow is told the horizon ran out.
                None => (
                    match f.outcome {
                        core_sim::DenialOutcome::Repelled => format!(
                            " — this party CANNOT drive the {} past recovery: its kills do not \
                             outpace the herd's regrowth",
                            fauna_id
                        ),
                        _ => format!(
                            " — a long raid: the {} is not past recovery within {} turns",
                            fauna_id, cfg.hunt.forecast_horizon_turns
                        ),
                    },
                    format!(
                        " outcome={} turns_to_collapse=none travel_turns={} animals={} \
                         food={:.2} wasted={:.2}",
                        f.outcome.as_str(),
                        travel_turns,
                        f.animals_killed,
                        f.delivered_food,
                        f.wasted_food
                    ),
                ),
            }
        }
        None => (String::new(), String::new()),
    };

    let band_label = outfit.band.label.clone();
    // Read off the outfit BEFORE it is moved into the launch, for the same reason `band_label` is.
    let target_species = outfit.herd_species.clone();
    let mission = ExpeditionMission::Deny {
        fauna_id: fauna_id.clone(),
        target_species,
    };
    // The species on the line, the id in the `herd=` token — as the hunt launch does.
    let target_display = mission.target_display().to_string();
    // **A launch that did not happen publishes a FAILURE, never an `applied` line** — see
    // `launch_detached_party`, which answers `None` rather than a placeholder entity.
    let Some(expedition_entity) =
        launch_detached_party(app, outfit, party_workers, mission, kit.clone())
    else {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            format!("send_denial_raid: {band_label} has no population to outfit from."),
        );
        return;
    };

    let tick = app.world.resource::<SimulationTick>().0;
    push_command_event(
        app,
        tick,
        CommandEventKind::ExpeditionSent,
        faction,
        format!(
            "{} denial raid -> {}{}",
            band_label, target_display, verdict
        ),
        Some(format!(
            "status=applied mission=deny workers={} herd={} expedition={}{}",
            party_workers,
            fauna_id,
            expedition_entity.to_bits(),
            verdict_detail
        )),
    );
}

/// **Everything a shipment needs resolved before a single unit is drawn** — the destination it is
/// bound for and the goods it will hold, each already checked against the sending band's store, the
/// party's pack space and the tie between the two peoples.
///
/// It exists so the draw is **one step at the end**: every refusal below happens before anything is
/// debited, so a refused `send_trade_expedition` leaves the band exactly as it stood.
struct ResolvedShipment {
    destination_band: BandId,
    destination_name: String,
    destination_pos: UVec2,
    /// The FOOD the shipment will carry, summed over the order's food lines.
    food: Scalar,
    /// One `(material id, amount)` per material the order names, summed over its lines and in the
    /// order the player named them.
    materials: Vec<(String, Scalar)>,
}

/// **How much pack space this shipment takes** — `food + material_carry_weight × Σ material
/// amounts`, the one expression the cap is checked against.
///
/// A material's bulk is a v1 simplification (`expedition_config.trade.material_carry_weight`): every
/// material weighs the same per unit relative to food, because `materials.json` authors no density
/// axis to read instead.
fn shipment_mass(food: Scalar, materials: &[(String, Scalar)], material_carry_weight: f32) -> f32 {
    let material_units: f32 = materials
        .iter()
        .map(|(_, amount)| amount.to_f32())
        .sum::<f32>();
    food.to_f32() + material_carry_weight * material_units
}

/// Resolve and validate a shipment's destination and its cargo. **Fails closed on every axis** — an
/// empty order, an unknown commodity or material, a non-positive or non-finite amount, cargo the
/// band does not hold, cargo over the party's carry cap, a destination that is not a resident band,
/// and a destination this band holds no tie to are each a command failure with a reason. None of
/// them clamps, and none of them silently drops a line.
fn resolve_shipment(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    home_entity: Entity,
    party_workers: u32,
    destination_band_id: u64,
    cargo: &[TradeCargoItem],
) -> Option<ResolvedShipment> {
    const VERB: &str = "send_trade_expedition";
    // --- the destination is a real, resident band -------------------------------------------
    let wanted = BandId(destination_band_id);
    // The entity is deliberately not carried out of here: nothing downstream needs it. It *was*,
    // to resolve a display name off `StartingUnit` — see `ResolvedShipment::destination_name` for
    // why that is gone.
    let destination = {
        let mut query = app
            .world
            .query_filtered::<(&BandId, &PopulationCohort), With<ResidentBand>>();
        query
            .iter(&app.world)
            .find(|(id, _)| **id == wanted)
            .map(|(id, cohort)| (*id, cohort.current_tile))
    };
    let Some((destination_band, destination_tile)) = destination else {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            format!("{VERB}: band {destination_band_id} is not a band to deliver to."),
        );
        return None;
    };
    let Some(destination_pos) = app.world.get::<Tile>(destination_tile).map(|t| t.position) else {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            format!("{VERB}: band {destination_band_id} is not standing anywhere."),
        );
        return None;
    };

    // --- the tie is the gate, and FACTION IS NEVER ASKED --------------------------------------
    // *"At zero, nothing flows"* (`docs/plan_contact_and_logistics.md` §Q6): a shipment needs a live
    // connection from the sending band to the destination, and a **parked** tie — an edge at zero,
    // meaning *"we know such a people exist and have no current dealings"* — refuses exactly as a
    // missing one does.
    //
    // **There is deliberately no same-faction branch here or anywhere downstream.** Faction is a
    // property of the endpoint (`.claude/rules/core_sim/connections.md`), so a destination in
    // another faction works by construction rather than by a clause someone has to remember to add.
    let Some(&home_band) = app.world.get::<BandId>(home_entity) else {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            format!("{VERB}: the sending band has no id to trade under."),
        );
        return None;
    };
    let tie = app
        .world
        .resource::<core_sim::connections::ConnectionLedger>()
        .get(&core_sim::connections::ConnectionKey::new(
            home_band,
            destination_band,
        ))
        .map(|connection| connection.strength)
        .unwrap_or(core_sim::connections::NO_TIE);
    if tie <= core_sim::connections::NO_TIE {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            format!(
                "{VERB}: no dealings with band {destination_band_id} — meet them before you ship \
                 to them."
            ),
        );
        return None;
    }

    // --- the cargo lines, summed per key ------------------------------------------------------
    if cargo.is_empty() {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            format!("{VERB}: a shipment with nothing in it is not a shipment."),
        );
        return None;
    }
    let materials_cfg = app.world.resource::<MaterialsConfigHandle>().get();
    let mut food = Scalar::from_i64(0);
    // A `Vec` rather than a map: the order the player named the lines in is the order they are drawn
    // and published in, and two lines naming one material sum rather than the second winning.
    let mut materials: Vec<(String, Scalar)> = Vec::new();
    for item in cargo {
        if !item.amount.is_finite() || item.amount <= 0.0 {
            emit_command_failure(
                app,
                CommandEventKind::ExpeditionSent,
                faction,
                format!(
                    "{VERB}: '{}' must be a positive amount; got {}.",
                    item.id, item.amount
                ),
            );
            return None;
        }
        let amount = scalar_from_f32(item.amount);
        if item.is_material {
            if materials_cfg.material(&item.id).is_none() {
                emit_command_failure(
                    app,
                    CommandEventKind::ExpeditionSent,
                    faction,
                    format!("{VERB}: unknown material '{}'.", item.id),
                );
                return None;
            }
            match materials.iter_mut().find(|(id, _)| id == &item.id) {
                Some((_, held)) => *held += amount,
                None => materials.push((item.id.clone(), amount)),
            }
        } else {
            // **The commodity key is checked, not assumed.** `sim_runtime::FOOD_CARGO_KEY` is a
            // restatement of this crate's `FOOD`, and this is what makes the duplication safe: a
            // drift refuses the shipment rather than loading the wrong good.
            if item.id != FOOD {
                emit_command_failure(
                    app,
                    CommandEventKind::ExpeditionSent,
                    faction,
                    format!("{VERB}: unknown commodity '{}'.", item.id),
                );
                return None;
            }
            food += amount;
        }
    }

    // --- it fits in the packs of the people being sent ----------------------------------------
    let cfg = app.world.resource::<ExpeditionConfigHandle>().get();
    let cap = party_workers as f32 * cfg.trade.per_worker_carry;
    let mass = shipment_mass(food, &materials, cfg.trade.material_carry_weight);
    if mass > cap {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            format!(
                "{VERB}: {party_workers} workers can carry {cap:.2}; this shipment weighs \
                 {mass:.2}."
            ),
        );
        return None;
    }

    // --- and the band actually holds it -------------------------------------------------------
    let Some(store) = app
        .world
        .get::<PopulationCohort>(home_entity)
        .map(|cohort| cohort.stores.clone())
    else {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            format!("{VERB}: the sending band has no store to load from."),
        );
        return None;
    };
    if store.get(FOOD) < food {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            format!(
                "{VERB}: the band holds {:.2} {FOOD}, not {:.2}.",
                store.get(FOOD).to_f32(),
                food.to_f32()
            ),
        );
        return None;
    }
    for (material, amount) in &materials {
        let held = store.material_total(material);
        if held < *amount {
            emit_command_failure(
                app,
                CommandEventKind::ExpeditionSent,
                faction,
                format!(
                    "{VERB}: the band holds {:.2} {material}, not {:.2}.",
                    held.to_f32(),
                    amount.to_f32()
                ),
            );
            return None;
        }
    }

    Some(ResolvedShipment {
        destination_band,
        // **EMPTY, because bands have no names in this game.** This briefly resolved through
        // `starting_unit_label`, which answers `StartingUnit.kind` — the unit *archetype*
        // (`"BandForager"`), the same string for every seeded band — so an in-flight party's row
        // rendered *"Bound for BandForager"* for every destination in the game, and disagreed with
        // the positional label ("Band 2") the rest of the HUD gives the same band.
        //
        // The right fix is not a better guess: it is to say **nothing**, which is what `""` means on
        // this field (the *"empty is no row, never a zero"* contract this arc's material readouts
        // use). The client then falls back to the label it already uses everywhere else. The field
        // stays because a naming scheme is a separate piece of design and because #513 makes it
        // load-bearing: a foreign band's name has to come from the sim, the client having no roster
        // to resolve one from.
        destination_name: String::new(),
        destination_pos,
        food,
        materials,
    })
}

/// Outfit and launch a **trade expedition**: draw `party_workers` off the resolved home band, load
/// them with cargo out of that band's own store, and send them to deliver it to another band. Text
/// form: `send_trade_expedition <faction> <band> <party_workers> <destination_band_id>
/// [food <amount>] [material <material_id> <amount>]... [kit <id>]`.
///
/// **The first rider on the connection primitive** (`docs/plan_contact_and_logistics.md` §Q5, arc
/// #527). A shipment is a party that walks it: there is no persistent link component in this slice,
/// because what maintains a link is a route and the route ladder is what will hold that state.
///
/// **A trade party is provisioned like a SCOUT** — a launch larder of `party × distance ×
/// provision_draw_per_worker_per_tile`, drained per turn on the road. That is where the trip's cost
/// lives, which is why there is no separate friction lever: a longer haul eats more.
#[allow(clippy::too_many_arguments)] // every launch order the verb accepts is a parameter
fn handle_send_trade_expedition(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_id: Option<u64>,
    party_workers: u32,
    destination_band_id: u64,
    cargo: Vec<TradeCargoItem>,
    kit_id: Option<String>,
) {
    const VERB: &str = "send_trade_expedition";
    // **The kit fails closed**, resolved before anything is drawn — the rule both raiding verbs
    // follow. A trade party is quoted at the **hunt** job for the reason a scout is: it carries the
    // sled that decides what it can haul, and its opportunistic roadside kill resolves through the
    // same hunt seams.
    let kit = {
        let equipment_cfg = app.world.resource::<EquipmentConfigHandle>().get();
        let absent = equipment_cfg.default_kit(KitJob::Hunt);
        match equipment_cfg.resolve_kit_or(kit_id.as_deref(), KitJob::Hunt, absent) {
            Ok(kit) => kit,
            Err(reason) => {
                emit_command_failure(
                    app,
                    CommandEventKind::ExpeditionSent,
                    faction,
                    format!("{VERB}: {reason}."),
                );
                return;
            }
        }
    };
    // The resident-band gate, the party bound and the cohort template — the same seam both raiding
    // verbs outfit through, so this verb could not acquire its own copy of them.
    let Some(outfit) = outfit_detached_party(app, faction, band_id, party_workers, VERB) else {
        return;
    };
    let Some(shipment) = resolve_shipment(
        app,
        faction,
        outfit.band.entity,
        party_workers,
        destination_band_id,
        &cargo,
    ) else {
        return;
    };

    // --- everything above refused without drawing; from here the band is debited ---------------
    let band_pos = app
        .world
        .get::<PopulationCohort>(outfit.band.entity)
        .and_then(|cohort| app.world.get::<Tile>(cohort.current_tile))
        .map(|tile| tile.position);
    let distance = {
        let grid_width = app.world.resource::<TileRegistry>().width;
        let wrap_horizontal = app
            .world
            .resource::<SimulationConfig>()
            .map_topology
            .wrap_horizontal;
        band_pos
            .map(|from| {
                hex_distance_wrapped(from, shipment.destination_pos, grid_width, wrap_horizontal)
            })
            .unwrap_or(0)
    };
    let cfg = app.world.resource::<ExpeditionConfigHandle>().get();
    // The walk's larder — the scout's draw, and partial is fine (non-fatal at zero in v1). The cargo
    // is drawn EXACTLY, because a shipment short of what was ordered is a different shipment.
    let requested_provisions = scalar_from_f32(
        party_workers as f32 * distance as f32 * cfg.provision_draw_per_worker_per_tile,
    );
    let (provisions, shipment_store) = {
        let Some(mut band_cohort) = app.world.get_mut::<PopulationCohort>(outfit.band.entity)
        else {
            emit_command_failure(
                app,
                CommandEventKind::ExpeditionSent,
                faction,
                format!(
                    "{VERB}: {} has no population to outfit from.",
                    outfit.band.label
                ),
            );
            return;
        };
        let mut loaded = LocalStore::new();
        let food = band_cohort.stores.take(FOOD, shipment.food);
        if food > Scalar::from_i64(0) {
            loaded.add(FOOD, food);
        }
        // **Peeled batch by batch, splitting only the last** — a split preserves the batch's rating
        // and its readings, because an amount is a quantity of one identical material. Two ratings
        // of one material therefore leave as two batches and arrive as two batches.
        for (material, amount) in &shipment.materials {
            for draw in band_cohort.stores.take_material_batches(material, *amount) {
                loaded.deposit_material(material, draw.band, draw.amount, &draw.characteristics);
            }
        }
        let provisions = band_cohort.stores.take(FOOD, requested_provisions);
        (provisions, loaded)
    };
    // **The sending half of the food ledger's transfer pair** — the cargo the shipment carries AND
    // the larder the party walks on, because both are food that left this band's store through
    // neither consumption nor a pen. The receiving half is booked when the shipment lands, and the
    // rest comes home on the fold-back if it never does.
    if let Some(mut allocation) = app.world.get_mut::<LaborAllocation>(outfit.band.entity) {
        allocation.last_transfer_sent += shipment_store.get(FOOD).to_f32() + provisions.to_f32();
    }

    let band_label = outfit.band.label.clone();
    let mission = ExpeditionMission::Trade {
        destination_band: shipment.destination_band,
        destination_name: shipment.destination_name,
    };
    // **The launch line names the destination through `destination_display`**, which falls back to
    // the band's id — the sim has to be able to write this sentence on its own, and today there is
    // no name to write. The `destination=<id>` detail token beside it is the key a client uses if it
    // would rather print its own label.
    let destination_label = mission.destination_display();
    let carried_food = shipment_store.get(FOOD).to_f32();
    let carried_materials: Vec<String> = shipment
        .materials
        .iter()
        .map(|(material, amount)| format!("{:.2} {material}", amount.to_f32()))
        .collect();
    // **A launch that did not happen publishes a FAILURE, never an `applied` line** — see
    // `launch_party_from_band`, which answers `None` rather than a placeholder entity.
    let Some(expedition_entity) = launch_party_from_band(
        app,
        outfit,
        party_workers,
        mission,
        LaunchOrders {
            // A shipment reuses the scout's `Outbound`; there is no trade phase, because the party
            // does exactly two things and both already have one.
            phase: ExpeditionPhase::Outbound,
            target: shipment.destination_pos,
            provisions,
            cargo: shipment_store,
        },
        kit,
    ) else {
        emit_command_failure(
            app,
            CommandEventKind::ExpeditionSent,
            faction,
            format!("{VERB}: {band_label} has no population to outfit from."),
        );
        return;
    };

    // The manifest reads as a list, never as a total: a sum of food and hide is the retired trade
    // axis under a new name.
    let mut manifest: Vec<String> = Vec::new();
    if carried_food > 0.0 {
        manifest.push(format!("{carried_food:.2} {FOOD}"));
    }
    manifest.extend(carried_materials);
    let tick = app.world.resource::<SimulationTick>().0;
    push_command_event(
        app,
        tick,
        CommandEventKind::ExpeditionSent,
        faction,
        format!(
            "{band_label} shipment -> {destination_label} ({})",
            manifest.join(", ")
        ),
        Some(format!(
            "status=applied mission=trade workers={} destination={} distance={} expedition={}",
            party_workers,
            destination_band_id,
            distance,
            expedition_entity.to_bits()
        )),
    );
}

/// The collapse verdict's turn window as prose — *"4 turns"* when the distribution is degenerate,
/// *"3–5 turns"* when it is not, *"3+ turns"* when the pessimistic end never gets there at all, and
/// *"up to 5 turns"* when only the pessimistic end did.
///
/// **A range that is a point must READ as a point** — the rule every range on this wire follows: the
/// sim publishes three numbers and *"say 4, not 4–4"* is the reader's comparison, not a stored flag.
///
/// # The prose may not contradict any of the three numbers beside it
///
/// `likely` is a **separate projection** ([`core_sim::denial_forecast`] runs one per quantile), so it
/// is not bound to sit between `low` and `high`: the take is quantised to whole animals, and a
/// lumpier schedule can put the expected run outside the two extremes it was not derived from. The
/// degenerate arm used to test only `low == high` and then print `likely`, so `low = high = 3` beside
/// `likely = 4` rendered *"4 turns"* against a published band of 3–3 — prose disagreeing with the
/// `low=`/`high=` tokens in the very same feed entry.
///
/// So the window is the span of **all three** published numbers, and it collapses to a point only
/// when all three agree.
fn describe_collapse_window(low: Option<u32>, likely: u32, high: Option<u32>) -> String {
    match (low, high) {
        (Some(low), Some(high)) if low == likely && likely == high => format!("{likely} turns"),
        (Some(low), Some(high)) => format!("{}–{} turns", low.min(likely), high.max(likely)),
        // The pessimistic draw never gets there inside the horizon — an honest open-ended window
        // rather than a second number the projection did not produce.
        (Some(low), None) => format!("{}+ turns", low.min(likely)),
        // The mirror case: only the pessimistic end reached the line. Stating it as a *ceiling*
        // keeps the number the projection did produce, where the default below discarded it.
        (None, Some(high)) => format!("up to {} turns", high.max(likely)),
        (None, None) => format!("{likely} turns"),
    }
}

/// Order an expedition home: set its phase to `Returning` (it chases the home band's live tile and
/// folds its workers + leftover provisions back on arrival). Text form:
/// `recall_expedition <faction> <expedition_band_id>`.
///
/// **A party standing in its home band's camp is CANCELLED, not sent on a round trip.** Recalling one
/// that has not left used to publish `Returning` and then make the player wait a turn for
/// `advance_expeditions` to fold back a party that had gone nowhere — the order read as a no-op right
/// when the player was most sure it should be instant. The condition is positional and state-based,
/// never "turn 0": the party is on the band's own tile and owes it no report
/// ([`party_owes_a_report`]), which also covers a party recalled the moment it walks back into camp.
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
    if let Some(at) = cancel_party_standing_in_camp(app, entity) {
        push_command_event(
            app,
            tick,
            CommandEventKind::ExpeditionRecalled,
            faction,
            format!("{} cancelled — the party never left camp", label),
            Some(format!("status=cancelled expedition={}", entity.to_bits())),
        );
        // The world event beside the command ack: the fold-back is what actually happened, and it is
        // the same line the `Returning` arm publishes when a party walks home.
        let event =
            expedition_returned_event(tick, faction, at.position, at.banked_materials, entity);
        app.world.resource_mut::<CommandEventLog>().push(event);
        app.world.despawn(entity);
        return;
    }
    push_command_event(
        app,
        tick,
        CommandEventKind::ExpeditionRecalled,
        faction,
        format!("{} recalled — returning home", label),
        Some(format!("status=returning expedition={}", entity.to_bits())),
    );
}

/// **Form a new band** — a resident band splits in two on the tile it is standing on
/// (`docs/plan_band_fission.md`, issue #511).
///
/// The gate, the division and their order all live in [`split_band_from_parent`], so the refusal the
/// compose sheet forecasts and the refusal this command produces are one rule set rather than two
/// that agree by habit. A refusal leaves the parent exactly as it stood.
fn handle_split_band(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_id: Option<u64>,
    workers: u32,
) {
    let Some(band) = select_starting_band(
        app,
        faction,
        band_id,
        "split_band",
        CommandEventKind::BandFounded,
    ) else {
        return;
    };
    let label = starting_unit_label(app, band.entity);
    let settle = app
        .world
        .resource::<ExpeditionConfigHandle>()
        .get()
        .settle
        .clone();
    let split = match split_band_from_parent(&mut app.world, band.entity, workers, &settle) {
        Ok(split) => split,
        // **Every applicable reason, not the first one.** A split that is both too small and leaves
        // the parent short has two things to fix, and reporting one at a time teaches the rules one
        // refusal at a time — so the token list and the feed line both carry the whole set.
        Err(refusals) => {
            warn!(
                target: "shadow_scale::command",
                command = "split_band",
                faction = %faction.0,
                workers,
                "command.split.rejected={}",
                refusals.tokens()
            );
            emit_command_failure(
                app,
                CommandEventKind::BandFounded,
                faction,
                format!("{} cannot split — {}", label, refusals.explanation()),
            );
            return;
        }
    };
    let tick = app.world.resource::<SimulationTick>().0;
    // Every token is numeric, so none of them has to be last (`.claude/rules/core_sim/event-feed.md`
    // — a multi-word value can only be the trailing remainder).
    let parent_id = app
        .world
        .get::<BandId>(band.entity)
        .map(|id| id.0)
        .unwrap_or_default();
    let detail = format!(
        "status=split band={} parent={} x={} y={} workers={} share={:.3} provisions={:.2}",
        split.band.0,
        parent_id,
        split.at.x,
        split.at.y,
        split.workers,
        split.share,
        split.provisions.to_f32()
    );
    push_command_event(
        app,
        tick,
        CommandEventKind::BandFounded,
        faction,
        format!(
            "{} split off a new band of {} workers at ({}, {})",
            label, split.workers, split.at.x, split.at.y
        ),
        Some(detail),
    );
}

/// Where a cancelled party folded back, and what its pack was worth — the two things the feed line
/// needs from a fold-back that happened outside the turn loop.
struct CancelledInCamp {
    position: UVec2,
    banked_materials: f32,
}

/// Settle `entity` into its home band **now** if it is standing on that band's tile with nothing left
/// to report, returning where it stood. `None` = it is in the field (or owes a report) and must take
/// the ordinary `Returning` walk home.
///
/// **"At home" is exact co-location, not the comm range** the `Returning` arm folds back within: a
/// party two tiles out is genuinely away, and folding it back from there would teleport its workers
/// home rather than cancel an order that had not taken effect.
fn cancel_party_standing_in_camp(
    app: &mut bevy::prelude::App,
    entity: Entity,
) -> Option<CancelledInCamp> {
    let expedition = app.world.get::<Expedition>(entity)?.clone();
    if party_owes_a_report(&expedition) {
        return None;
    }
    let mut party = app.world.get::<PopulationCohort>(entity)?.clone();
    let position = app.world.get::<Tile>(party.current_tile)?.position;
    let home_tile = app
        .world
        .get::<PopulationCohort>(expedition.home_band)?
        .current_tile;
    let home_position = app.world.get::<Tile>(home_tile)?.position;
    if position != home_position {
        return None;
    }
    // The clone's cargo is what folds back: an undelivered shipment lands in the band that sent it,
    // exactly as a party turned home mid-flight would deliver it. The caller despawns the party
    // immediately after, so the live component is never read again.
    let mut cargo = expedition.cargo.clone();
    let fold = {
        let mut home = app
            .world
            .get_mut::<PopulationCohort>(expedition.home_band)?;
        fold_party_into_band(&mut party, &mut cargo, &mut home)
    };
    // The pack and any undelivered cargo landing back in the band's larder is a transfer, exactly as
    // the `Returning` arm's fold-back is — a cancel differs only in *when* it fires.
    if let Some(mut allocation) = app.world.get_mut::<LaborAllocation>(expedition.home_band) {
        allocation.last_transfer_received += fold.food.to_f32();
    }
    Some(CancelledInCamp {
        position,
        banked_materials: fold.materials,
    })
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
        CancelScope::Roles => matches!(
            target,
            LaborTarget::Scout
                | LaborTarget::Warrior
                | LaborTarget::Agriculture
                | LaborTarget::Husbandry
                | LaborTarget::Builders
        ),
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
/// (MSY) ceiling`) and takes `work_cost / the crew's output` turns of sustained work.
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
    let switched =
        queue_build_on_working_bands(app, faction, &target, BuildJob::Rung(Improvement::Tame));
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
/// the tile alone, so the stance, crop and take-selection slots are placeholders — this exists so
/// the two build verbs cannot accidentally *carry* a stance, a crop or a selection into
/// `queue_build_on_working_bands`, and so "the command names a source, not an assignment" is stated
/// once.
fn forage_source(tile: UVec2) -> LaborTarget {
    LaborTarget::Forage {
        tile,
        floor: SOURCE_NAMED_NOT_ASSIGNED,
        species: None,
        take_species: TakeSelection::EVERYTHING,
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
/// **Every crop, not just the first band's** — unlike the retired `abandon_improvement`'s "the first band answers
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
            take_species: TakeSelection::EVERYTHING,
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

/// **THE RUNG DIRECTLY BENEATH `rung` ON ITS OWN BRANCH** — the inverse walk of [`RungKey::above`],
/// which the ladder does not carry because nothing else has needed it.
///
/// It is what the two road verbs' *"not yet a trail / not yet a dirt road"* refusal asks, and it is
/// derived from the coded climb rather than named per verb so a re-ordered branch cannot leave the
/// refusal asking for the wrong rung. `None` at the root of a branch, which is the honest answer:
/// nothing stands beneath a path.
fn rung_beneath(rung: RungKey) -> Option<RungKey> {
    let mut cursor = rung.branch().root_rung();
    loop {
        let next = cursor.above()?;
        if next == rung {
            return Some(cursor);
        }
        cursor = next;
    }
}

/// **A band by its durable id, and the tile it is standing on** — the pair the road verbs need, so
/// the lookup and the position cannot come from two different frames.
fn band_entity_and_tile(app: &mut bevy::prelude::App, band: BandId) -> Option<(Entity, UVec2)> {
    let mut query = app.world.query::<(Entity, &PopulationCohort, &BandId)>();
    let (entity, current_tile) = query
        .iter(&app.world)
        .find(|(_, _, id)| **id == band)
        .map(|(entity, cohort, _)| (entity, cohort.current_tile))?;
    let position = app.world.get::<Tile>(current_tile)?.position;
    Some((entity, position))
}

/// ⛔ **`grade <faction> <band> <x> <y>` / `pave …` — THE ROUTE BRANCH'S TWO TILE VERBS.**
///
/// `cultivate`/`sow`'s grammar with a **band** token in it, because a road is a per-tile improvement
/// with a **keeper** and no labor row (`docs/plan_standing_upkeep.md` §4.13b). Issuing one does two
/// things at once, and they are the same act: it **declares the job** (a `BuildQueueEntry` on that
/// band's queue, raised by that band's `builders` pool at the head) and it **names the keeper** —
/// exactly what `cultivate` does to a patch's owner.
///
/// **ONE KEEPER PER TILE, NO SHARES.** A tile another band already keeps is refused by name, which
/// is what makes *"several bands each pay a part"* unrepresentable rather than merely discouraged —
/// and what makes *"one band keeps half the tiles between two camps and another keeps the rest"* the
/// state the model is built around.
///
/// **RE-ISSUING IT ON A ROAD NOBODY KEEPS IS ADOPTION**, and deliberately not a second verb. That is
/// why the *"already at that rung"* refusal is scoped to a road **this band already keeps**: a
/// keeperless dirt road is a road to pick up, not a job already done.
///
/// # THE REFUSALS, EACH NAMED
///
/// | | |
/// |---|---|
/// | no band by that id, or not yours | you cannot commit somebody else's people |
/// | no road on that tile at all | there is nothing to grade; walk it first |
/// | not yet a trail / not yet a dirt road | the rung beneath has to stand |
/// | the knowledge is not learned | `roadbuilding` gates `grade`, `paving` gates `pave` |
/// | another band keeps it | one keeper per tile |
/// | you already keep it at that rung or above | there is nothing left to raise |
///
/// # ⛔ DISTANCE IS PRICED HERE AND REFUSED NOWHERE
///
/// The band's hex distance to the tile is quoted through [`core_sim::remoteness_multiplier`] — which
/// asks [`core_sim::road_keeping_range`], the one seam — and **stamped on the road**, where it prices
/// both the build pile and the standing upkeep for the life of the job. There is deliberately **no
/// range refusal**: Ray, *"already forage and hunting have different work ranges, expeditions are even
/// farther. I don't think it makes sense to restrict it."*
fn handle_road_verb(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_id: u64,
    tile: UVec2,
    improvement: Improvement,
) {
    let verb = improvement.as_str();
    let destination = RungKey::built_by(improvement);
    let required = rung_beneath(destination)
        .expect("every built route rung stands on the rung below it — the floor is two rungs down");

    let refusal = road_verb_refusal(app, faction, band_id, tile, improvement, required);
    if let Err(reason) = refusal {
        warn!(
            target: "shadow_scale::command",
            command = verb,
            faction = %faction.0,
            band_id,
            x = tile.x,
            y = tile.y,
            reason = %reason,
            "command.road.rejected"
        );
        emit_command_failure(app, CommandEventKind::Road, faction, reason);
        return;
    }

    // **The keeper and the remoteness quote, written together**, because the price is a fact about
    // the moment the band took the road on — `ForagePatch::field_cost_multiplier`'s discipline.
    let band = BandId(band_id);
    let Some((entity, band_tile)) = band_entity_and_tile(app, band) else {
        emit_command_failure(
            app,
            CommandEventKind::Road,
            faction,
            format!("Band {band_id} is not standing anywhere the simulation can see."),
        );
        return;
    };
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let (width, wrap) = {
        let registry = app.world.resource::<TileRegistry>();
        let config = app.world.resource::<SimulationConfig>();
        (registry.width, config.map_topology.wrap_horizontal)
    };
    let distance = hex_distance_wrapped(band_tile, tile, width, wrap);
    let remoteness = core_sim::remoteness_multiplier(distance, &ladder);
    {
        let mut roads = app.world.resource_mut::<core_sim::RoadRegistry>();
        let Some(road) = roads.road_mut(tile) else {
            return;
        };
        road.take_keeper(core_sim::RoadKeeper { faction, band }, remoteness, &ladder);
    }

    // **The declaration**, on that band's own queue. A road names no labor row, so this is the
    // band-addressed twin of `queue_build_on_working_bands` — and there is no yield seed to re-strike
    // beside it, because a road pays into no take.
    {
        let mut allocation = band_allocation_mut(app, entity);
        allocation.enqueue_build(BuildSource::Road(tile), BuildJob::Rung(improvement));
    }

    let tick = app.world.resource::<SimulationTick>().0;
    info!(
        target: "shadow_scale::command",
        command = verb,
        faction = %faction.0,
        band_id,
        x = tile.x,
        y = tile.y,
        distance,
        remoteness,
        "command.road.declared"
    );
    push_command_event(
        app,
        tick,
        CommandEventKind::Road,
        faction,
        format!(
            "Band {band_id} takes on the road at ({}, {}) — {verb}",
            tile.x, tile.y
        ),
        Some(format!(
            "status=declared action={verb} x={} y={} band={band_id} distance={distance}",
            tile.x, tile.y
        )),
    );
}

/// **Every refusal `grade` / `pave` can produce, in the order they are asked.** Split out so the
/// command and its tests read one list — the `validate_cultivate` convention one branch over.
fn road_verb_refusal(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_id: u64,
    tile: UVec2,
    improvement: Improvement,
    required: core_sim::RungKey,
) -> Result<(), String> {
    let band = BandId(band_id);
    let holds_band = {
        let mut query = app.world.query::<(&PopulationCohort, &BandId)>();
        query
            .iter(&app.world)
            .any(|(cohort, id)| *id == band && cohort.faction == faction)
    };
    if !holds_band {
        return Err(format!(
            "Band {band_id} is not one of your people, so it cannot take on a road."
        ));
    }
    let destination = RungKey::built_by(improvement);
    let (threshold, knowledge) = {
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        (
            ladder.knowledge.completion_threshold,
            ladder.rung(destination).unlock_discovery_id(),
        )
    };
    let knows_rung = knowledge.is_none_or(|discovery_id| {
        knows(
            app.world.resource::<DiscoveryProgressLedger>(),
            faction,
            discovery_id,
            threshold,
        )
    });
    if !knows_rung {
        return Err(format!(
            "Your people have not learned how to {} a road yet.",
            improvement.as_str()
        ));
    }
    let roads = app.world.resource::<core_sim::RoadRegistry>();
    let Some(road) = roads.road(tile) else {
        return Err(format!(
            "There is no road at ({}, {}) — walk it into a trail before you {} it.",
            tile.x,
            tile.y,
            improvement.as_str()
        ));
    };
    if !road.held_rung().is_at_or_above(required) {
        return Err(format!(
            "The road at ({}, {}) is only a {} — it must be a {} before you can {} it.",
            tile.x,
            tile.y,
            road.held_rung().id().replace('_', " "),
            required.id().replace('_', " "),
            improvement.as_str()
        ));
    }
    match road.keeper {
        // **One keeper per tile.** A second band cannot become a co-payer of a road another band
        // keeps — the refusal is what makes that unrepresentable rather than merely discouraged.
        Some(keeper) if keeper.band != band => {
            return Err(format!(
                "Band {} already keeps the road at ({}, {}). One band keeps a road tile, never two \
                 — have them abandon it first.",
                keeper.band.0, tile.x, tile.y
            ));
        }
        // **Already there, and it is YOURS** — nothing left to raise. A road nobody keeps falls
        // through instead, which is how adoption works.
        Some(_) if road.held_rung().is_at_or_above(destination) => {
            return Err(format!(
                "The road at ({}, {}) is already a {}.",
                tile.x,
                tile.y,
                road.held_rung().id().replace('_', " ")
            ));
        }
        _ => {}
    }
    Ok(())
}

/// **Set the Cultivate improvement** on the forage patch at `tile` for the band(s) already working it
/// (Intensification — "Cultivate & Corral as explicit policies"). This is the command form of what
/// the client's policy picker does; it does **not** claim or complete anything.
///
/// The old early-claim (snap `cultivation_progress` to `1.0` once past a `claim_threshold`) is
/// **gone**: it would let the player skip the investment, which is the entire decision. Cultivating
/// now costs a real yield dip — while preparing, the patch pays only
/// `cultivation.cultivating_yield_fraction × its Sustain (MSY) ceiling` — and takes
/// `work_cost / the crew's output` turns of sustained work.
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
    let switched = queue_build_on_working_bands(
        app,
        faction,
        &forage_source(tile),
        BuildJob::Rung(Improvement::Cultivate),
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
    let switched = queue_build_on_working_bands(
        app,
        faction,
        &forage_source(tile),
        BuildJob::Rung(Improvement::Sow),
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
        queue_build_on_working_bands(app, faction, &target, BuildJob::Rung(Improvement::Corral));
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

// **RETIRED: `handle_abandon_improvement`.** The build verb is derived from the meter
// (`forage::patch_build_verb` / `fauna::herd_build_verb`, `docs/plan_standing_upkeep.md` §2.4), so
// the stored verb is only a declaration for a meter at zero and there is nothing for this command to
// clear. **The undo is its own verb now**: `unqueue <faction> <source…>` withdraws the declaration
// and leaves the row, the take crew, the kit and the meter alone, and `abandon` puts the whole
// holding down (`docs/plan_standing_upkeep.md` §2.5).

/// **SAY HOW A BAND SPLITS A MAINTENANCE POOL IT CANNOT STRETCH** —
/// `upkeep_mode <faction> <band> spread|priority` (`docs/plan_standing_upkeep.md` §2.5).
///
/// It is what is left of the retired `maintain` once **maintenance left the tile**. The keeping is a
/// band-level standing role now (`assign_labor … agriculture|husbandry <workers>`), so *where the
/// hands go* is no longer a decision — the pool covers the whole web. What remains is what happens
/// when the pool falls short of the summed demand, and both answers are defensible:
///
/// - **spread** — proportional to demand, so everything degrades a little.
/// - **priority** — fund sources completely until the pool runs out, most-invested first, so the
///   biggest investments stay whole and the marginal ones rot.
///
/// **An unknown mode is refused by name**, never defaulted: silently reading a typo as `spread`
/// would leave the player believing they had protected their Field.
///
/// **It writes the BAND's `LaborAllocation`**, so it rides the checkpoint with the assignments and a
/// restore reproduces the same split.
fn handle_upkeep_mode(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_id: u64,
    mode: String,
) {
    let Some(chosen) = UpkeepFundMode::from_token(mode.trim().to_ascii_lowercase().as_str()) else {
        emit_command_failure(
            app,
            CommandEventKind::CancelOrder,
            faction,
            format!(
                "Unknown upkeep mode '{}' — expected {} or {}.",
                mode.trim(),
                UpkeepFundMode::Spread.as_str(),
                UpkeepFundMode::Priority.as_str()
            ),
        );
        return;
    };
    let mode = chosen;
    let Some(band) = select_starting_band(
        app,
        faction,
        Some(band_id),
        "upkeep_mode",
        CommandEventKind::CancelOrder,
    ) else {
        return;
    };
    {
        let mut allocation = band_allocation_mut(app, band.entity);
        allocation.upkeep_fund_mode = mode;
    }
    let tick = app.world.resource::<SimulationTick>().0;
    info!(
        target: "shadow_scale::command",
        command = "upkeep_mode",
        faction = %faction.0,
        band = band_id,
        mode = mode.as_str(),
        "command.upkeep_mode.applied"
    );
    let label = match mode {
        UpkeepFundMode::Spread => format!(
            "{}: short of keepers, everything it holds degrades a little",
            band.label
        ),
        UpkeepFundMode::Priority => format!(
            "{}: short of keepers, it holds its biggest investments and lets the rest go",
            band.label
        ),
    };
    push_command_event(
        app,
        tick,
        CommandEventKind::CancelOrder,
        faction,
        label,
        Some(format!(
            "status=applied action=upkeep_mode mode={} band={band_id}",
            mode.as_str()
        )),
    );
}

// **RETIRED: `crew_is_affordable` / `emit_crew_unaffordable`** — the pool gate the four improvement
// verbs and `extend_pen` passed through, and the one sentence they all refused with.
//
// **They existed only for those five gates**, and the gates went with the crew they named
// (`docs/plan_standing_upkeep.md` §2.5): a verb states *what* to raise and never *who* raises it, so
// there is no number left for it to refuse. The hands stand on the band-level `builders` role, whose
// stepper clamps against the band's idle pool exactly as scout's and warrior's do — `assign_labor`
// is the one enforcement of `Σ ≤ available`, and `LaborAllocation::normalize` still answers the
// other question, a band that **shrank**.

/// **A band's head-count, read off the world for a command to clamp against.**
///
/// The arithmetic lives on [`BandWorkforce`] — the one authority, shared with the snapshot that
/// publishes `idleWorkers` — so a command and the readout the player sized it against can never
/// disagree about who is free. This is only the ECS lookup.
fn band_workforce(app: &bevy::prelude::App, band: Entity) -> BandWorkforce {
    BandWorkforce::resolve(
        app.world.get::<PopulationCohort>(band),
        app.world.get::<LaborAllocation>(band),
        app.world.get::<BandBench>(band),
    )
}

/// The band's bench, inserted on demand so a band spawned before the component existed still has
/// one — the same shape `band_allocation_mut` uses for the labor allocation.
fn band_bench_mut(app: &mut bevy::prelude::App, band: Entity) -> bevy::prelude::Mut<'_, BandBench> {
    if app.world.get::<BandBench>(band).is_none() {
        app.world.entity_mut(band).insert(BandBench::default());
    }
    app.world
        .get_mut::<BandBench>(band)
        .expect("bench inserted above")
}

/// **Put a recipe on a band's bench and draw idle workers onto it** — `set_bench <faction> <band>
/// recipe <id> [workers <n>]`.
///
/// Two refusals, both **command failures with a reason** rather than silent no-ops, for the same
/// reason an unknown kit id is one: the player is choosing between recipes, so a quiet substitution
/// or a quiet nothing answers a different question than the one asked.
///
/// - an id the book does not carry, and
/// - a recipe whose `requires_knowledge` this faction has not learned — which is only ever a **tool**
///   (see `recipes.json`), so *"you cannot build a loom yet"* is a sentence the player is told, not
///   a bench that sits there doing nothing.
///
/// **There is no third refusal for material.** A band that is short simply makes no progress — the
/// draw takes nothing and the turn is a no-op — which is `docs/plan_crafting_and_materials.md` §5's
/// *"no 'you cannot craft that' branch in the sim"*: the panel names the shortfall, the sim just
/// does not move.
///
/// **The crew is the player's to name, never the sim's to guess** — see [`BENCH_CREW_UNSPECIFIED`]
/// at the clamp below. Naming no crew leaves the crew where it is; `bench_crew` is what takes a
/// number.
fn handle_set_bench(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_id: Option<u64>,
    recipe_id: &str,
    workers: u32,
) {
    let event_kind = CommandEventKind::Craft;
    let recipes = app.world.resource::<RecipesConfigHandle>().get();
    let Some(recipe) = recipes.recipe(recipe_id) else {
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!(
                "set_bench: unknown recipe '{recipe_id}' — the book offers {}.",
                recipes.recipe_ids_for_message()
            ),
        );
        return;
    };
    let threshold = app
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .knowledge
        .completion_threshold;
    let unknown_craft = {
        let ledger = app.world.resource::<DiscoveryProgressLedger>();
        recipe
            .requires_knowledge
            .iter()
            .find(|craft| {
                core_sim::crafting::craft_discovery_id(craft)
                    .is_none_or(|id| !knows(ledger, faction, id, threshold))
            })
            .cloned()
    };
    if let Some(craft) = unknown_craft {
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!(
                "set_bench: {} needs {craft}, which this people has not learned — a craft is \
                 learned by practising it bare-handed.",
                recipe.display_name
            ),
        );
        return;
    }
    let display_name = recipe.display_name.clone();

    let Some(band) = select_starting_band(app, faction, band_id, "set_bench", event_kind) else {
        return;
    };
    // The band's OWN crew stays on the bench while the job is swapped, so the pool this is clamped
    // against is the free hands PLUS the crew already standing there — [`BandWorkforce::benchable`],
    // which is the one place that decides not to count them twice.
    let benchable = band_workforce(app, band.entity).benchable();
    // **A crew of zero changes nothing about the crew.** The command arrives over a proto3 scalar,
    // which cannot tell an absent `workers` from an explicit `0`, and the client sends neither —
    // so this verb keeps whoever is already standing at the bench and recruits nobody. An idle
    // bench therefore stages at zero and the player staffs it, which is the point: labor is the
    // scarce currency and dividing the band is the decision the game is made of, so the one number
    // the sim must not pick is how many hands stop hunting. `bench_crew <n>` sets the crew, zero
    // included, so no reachable intent is lost.
    let standing = band_bench_mut(app, band.entity).workers;
    let applied = if workers == BENCH_CREW_UNSPECIFIED {
        standing.min(benchable)
    } else {
        workers.min(benchable)
    };
    {
        let mut bench = band_bench_mut(app, band.entity);
        bench.set_job(recipe_id, applied);
    }
    let tick = app.world.resource::<SimulationTick>().0;
    let clamp_note = if applied < workers {
        format!(" (clamped from {workers} — the band has only {benchable} hands off other work)")
    } else {
        String::new()
    };
    push_command_event(
        app,
        tick,
        event_kind,
        faction,
        format!(
            "{} is making {display_name} x{applied}{clamp_note}",
            band.label
        ),
        Some(format!(
            "status=applied action=set_bench recipe={recipe_id} workers={applied} \
             benchable={benchable}"
        )),
    );
}

/// **Take the job off a band's bench** — `clear_bench <faction> <band>`. The crew returns to the
/// idle pool.
///
/// **Materials already drawn for the pass in flight are spent.** They were cut for the thing the
/// player has just stopped making, and the store has no representation for a half-worked pile; the
/// command's help text says so rather than the sim pretending otherwise.
fn handle_clear_bench(app: &mut bevy::prelude::App, faction: FactionId, band_id: Option<u64>) {
    let event_kind = CommandEventKind::Craft;
    let Some(band) = select_starting_band(app, faction, band_id, "clear_bench", event_kind) else {
        return;
    };
    let running = {
        let mut bench = band_bench_mut(app, band.entity);
        let running = bench.recipe_id.clone();
        bench.clear_job();
        running
    };
    let Some(recipe_id) = running else {
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!("clear_bench: {} has nothing on its bench.", band.label),
        );
        return;
    };
    let tick = app.world.resource::<SimulationTick>().0;
    push_command_event(
        app,
        tick,
        event_kind,
        faction,
        format!("{} stopped making {recipe_id}", band.label),
        Some(format!(
            "status=cleared action=clear_bench recipe={recipe_id}"
        )),
    );
}

/// **MARK A BAND'S CRAFTING BENCH WITH THE PLAYER'S OWN RANK** — `bench_priority <faction> <band>
/// high|normal|low` (`docs/plan_standing_upkeep.md` §4.9 item 9b).
///
/// The same [`core_sim::SourcePriority`] a worked row carries, read by the same shedding order. It
/// touches nothing else on the bench: the recipe, the crew, the progress and the drawn pile are all
/// separate statements the player made.
///
/// # IT APPLIES TO AN IDLE BENCH TOO
///
/// Unlike `bench_crew`, which needs a running job to re-crew, a rank is a standing preference about
/// *the bench* — so it is settable before a recipe is put on it and survives the job being swapped,
/// exactly as a source row's mark survives an edit to its crew. Refusing it on an idle bench would
/// mean the player could only rank the bench in the window where they were least likely to want to.
///
/// # AN UNKNOWN LEVEL IS REFUSED BY NAME
///
/// `upkeep_mode`'s rule, and `work_priority`'s: a rank the player mistyped must fail loudly rather
/// than silently landing on the default, which is the one value that would look like it worked.
fn handle_bench_priority(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_id: u64,
    level: String,
) {
    let event_kind = CommandEventKind::Craft;
    let Some(priority) = SourcePriority::from_token(level.trim().to_ascii_lowercase().as_str())
    else {
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!(
                "Unknown bench priority '{}' — expected {}, {} or {}.",
                level.trim(),
                SourcePriority::High.as_str(),
                SourcePriority::Normal.as_str(),
                SourcePriority::Low.as_str()
            ),
        );
        return;
    };
    let Some(band) =
        select_starting_band(app, faction, Some(band_id), "bench_priority", event_kind)
    else {
        return;
    };
    if app.world.get::<BandBench>(band.entity).is_none() {
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!("bench_priority: {} has no crafting bench.", band.label),
        );
        return;
    }
    band_bench_mut(app, band.entity).priority = priority;
    let tick = app.world.resource::<SimulationTick>().0;
    info!(
        target: "shadow_scale::command",
        command = "bench_priority",
        faction = %faction.0,
        band = band_id,
        level = priority.as_str(),
        "command.bench_priority.applied"
    );
    let sentence = match priority {
        SourcePriority::High => format!("{}: the bench is held before anything else", band.label),
        SourcePriority::Normal => format!("{}: the bench takes its turn like the rest", band.label),
        SourcePriority::Low => format!("{}: the bench is the first thing given up", band.label),
    };
    push_command_event(
        app,
        tick,
        event_kind,
        faction,
        sentence,
        Some(format!(
            "status=applied action=bench_priority level={} band={band_id}",
            priority.as_str()
        )),
    );
}

/// **Re-crew a band's running bench** — `bench_crew <faction> <band> workers <n>`. The job and its
/// progress are untouched, exactly as `assign_labor` leaves an improvement in flight alone: editing
/// the crew is a crew-side edit and must not restart a build the player committed to.
fn handle_bench_crew(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_id: Option<u64>,
    workers: u32,
) {
    let event_kind = CommandEventKind::Craft;
    let Some(band) = select_starting_band(app, faction, band_id, "bench_crew", event_kind) else {
        return;
    };
    if app
        .world
        .get::<BandBench>(band.entity)
        .is_none_or(|bench| !bench.is_running())
    {
        emit_command_failure(
            app,
            event_kind,
            faction,
            format!("bench_crew: {} has nothing on its bench.", band.label),
        );
        return;
    }
    // Same ceiling `set_bench` clamps against, and for the same reason: the crew already on the
    // bench is being re-set, not added to.
    let benchable = band_workforce(app, band.entity).benchable();
    // **Zero is an order here, not a question.** This verb exists to name a crew, so it is the one
    // way to stand the bench down without taking the job off it — the opposite reading from
    // `set_bench`'s [`BENCH_CREW_UNSPECIFIED`].
    let applied = workers.min(benchable);
    let recipe_id = {
        let mut bench = band_bench_mut(app, band.entity);
        bench.workers = applied;
        bench.recipe_id.clone().unwrap_or_default()
    };
    let tick = app.world.resource::<SimulationTick>().0;
    let clamp_note = if applied < workers {
        format!(" (clamped from {workers} — the band has only {benchable} hands off other work)")
    } else {
        String::new()
    };
    push_command_event(
        app,
        tick,
        event_kind,
        faction,
        format!("{} bench crew x{applied}{clamp_note}", band.label),
        Some(format!(
            "status=applied action=bench_crew recipe={recipe_id} workers={applied} \
             benchable={benchable}"
        )),
    );
}

// **RETIRED with `abandon_improvement`**: `describe_source` and `improvement_event_kind`, its two
// helpers. Every other command names its own source inline, and the labor system keeps its own copy
// of the verb→channel map (`systems::labor::improvement_feed_channel`) for the events it pushes.

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

    // **A RING IS A QUEUE ENTRY LIKE EVERY OTHER BUILD** (`docs/plan_standing_upkeep.md` §2.5) — it
    // is fencing work on the same `animal:pen` rung as the pen it widens, so it waits its turn in
    // the same line and is funded from the same `builders` pool. It is the one entry kind that names
    // no rung verb: a built pen carries no meter for the derived verb to name, which is exactly the
    // gap `BuildJob::ExtendPen` fills.
    queue_build_on_working_bands(app, faction, &keeper_target, BuildJob::ExtendPen);

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

/// **PUT A SOURCE DOWN** — `abandon <faction> <x> <y>` / `abandon <faction> <herd_id>`
/// (`docs/plan_standing_upkeep.md` §2.5).
///
/// Drops the band's **holding**: the assignment row *and* its build-queue entry, on every band of
/// the faction working it. `drop_source_row` prunes the entry on the same edge, so the two cannot
/// come apart.
///
/// # THE METERS ARE UNTOUCHED, AND THAT IS WHY IT NEEDS NO CONFIRMATION
///
/// The ground keeps whatever is on it and, with nobody holding it, rots back down at the rung's own
/// rate over the following turns exactly as an unkept improvement already does. Nothing is destroyed
/// on the spot, so there is no second destruction path and nothing to confirm — the player stops
/// paying and the land goes back to what it was.
///
/// **One bit per source, never a number.** It is disposal rather than a smaller share; the
/// per-source *funding* lever stays deleted.
fn handle_abandon(app: &mut bevy::prelude::App, faction: FactionId, source: BuildSourceRef) {
    let label = source.label();
    let Some(target) = source.target() else {
        emit_command_failure(
            app,
            CommandEventKind::CancelOrder,
            faction,
            "abandon needs a source: two numbers name a tile, one token names a herd.".to_string(),
        );
        return;
    };
    // ⛔ **A TILE MAY CARRY A ROAD AS WELL AS A PATCH, AND `abandon` PUTS DOWN BOTH.**
    //
    // `abandon <faction> <x> <y>` names a *place*, and since roads became per-tile improvements a
    // place can hold two holdings at once. Putting one down without the other would leave `abandon`
    // silently partial on exactly the tiles where a band both farms and keeps a road — so this drops
    // the faction's keeping of the road on that tile too, and its queue entry with it.
    //
    // **It is the per-road choice the `Roadwork` POOL needs.** The pool covers every road the band
    // keeps, so *"pay for this road and not that one"* has to be expressible somewhere; this is that
    // somewhere, and it needs no verb of its own (`docs/plan_standing_upkeep.md` §4.13b).
    //
    // **The meter is untouched**, exactly as it is for a patch: the ground keeps whatever is on it
    // and, with nobody keeping it, rots back down at the rung's own rate over the following turns.
    let roads_put_down = release_roads_at(app, faction, &source);
    let bands = bands_working_source(app, faction, &target);
    if bands.is_empty() && roads_put_down == 0 {
        emit_command_failure(
            app,
            CommandEventKind::CancelOrder,
            faction,
            format!("No band of yours holds {label}, so there is nothing to put down."),
        );
        return;
    }
    for band in &bands {
        // **A ring the dropped entry was funding stops with it** — putting the pen down while a
        // fence ring was in flight otherwise left `pen_extending` set with nothing left to raise
        // it, and every later `extend_pen` on that pen refused.
        core_sim::drop_holding_and_cancel_ring(&mut app.world, *band, &target);
    }
    let tick = app.world.resource::<SimulationTick>().0;
    push_command_event(
        app,
        tick,
        CommandEventKind::CancelOrder,
        faction,
        format!("Put down {label} — whatever is built there is left to go back to the wild"),
        Some(format!(
            "status=applied action=abandon source={label} bands={} roads={roads_put_down}",
            bands.len()
        )),
    );
}

/// **PUT DOWN THE ROAD ON THIS TILE, if this faction keeps one** — [`handle_abandon`]'s route half,
/// returning how many were released (0 or 1; a tile carries at most one road).
///
/// It clears the keeper **and** that band's queue entry together, because the two are one statement:
/// a `grade` declares the job and names the keeper in the same act, so undoing it has to undo both.
/// Leaving the entry behind would have the band's builders raising a road it no longer keeps, which
/// the road arm refuses anyway — silently, which is worse than not happening.
///
/// A `BuildSourceRef` naming a herd resolves no tile and releases nothing.
fn release_roads_at(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    source: &BuildSourceRef,
) -> usize {
    let (Some(x), Some(y)) = (source.target_x, source.target_y) else {
        return 0;
    };
    let tile = UVec2::new(x, y);
    let keeper = {
        let mut roads = app.world.resource_mut::<core_sim::RoadRegistry>();
        let Some(road) = roads.road_mut(tile) else {
            return 0;
        };
        match road.keeper {
            Some(keeper) if keeper.faction == faction => {
                road.release_keeper();
                keeper
            }
            _ => return 0,
        }
    };
    if let Some((entity, _)) = band_entity_and_tile(app, keeper.band) {
        let mut allocation = band_allocation_mut(app, entity);
        allocation.unqueue_build(&BuildSource::Road(tile));
    }
    1
}

/// **WITHDRAW A DECLARATION** — `unqueue <faction> <x> <y>` / `unqueue <faction> <herd_id>`.
///
/// Drops the build-queue entry only, on every band of the faction working the source. The row, its
/// take crew, its kit and the meter are untouched.
///
/// **It is the undo a declaration never had.** `cultivate <f> <x> <y> 0` *set* the improvement with
/// zero builders rather than clearing it, so an unwanted verb was stuck on the row for the life of
/// the band. A declaration carries no crew at all now, so this is the only undo — and
/// [`handle_abandon`] is how a source with work already banked on it is put down.
fn handle_unqueue(app: &mut bevy::prelude::App, faction: FactionId, source: BuildSourceRef) {
    let label = source.label();
    let Some(target) = source.target() else {
        emit_command_failure(
            app,
            CommandEventKind::CancelOrder,
            faction,
            "unqueue needs a source: two numbers name a tile, one token names a herd.".to_string(),
        );
        return;
    };
    let Some(build_source) = BuildSource::of(&target) else {
        return;
    };
    let mut dropped = 0usize;
    for band in bands_working_source(app, faction, &target) {
        // **A withdrawn ring is cancelled, not paused** — the flag `extend_pen` set before it
        // queued is cleared here, so the pen can be extended again (see
        // [`core_sim::cancel_dropped_rings`]).
        if core_sim::unqueue_build_and_cancel_ring(&mut app.world, band, &build_source) {
            dropped += 1;
        }
    }
    if dropped == 0 {
        emit_command_failure(
            app,
            CommandEventKind::CancelOrder,
            faction,
            format!("Nothing of yours is queued to be built at {label}."),
        );
        return;
    }
    let tick = app.world.resource::<SimulationTick>().0;
    push_command_event(
        app,
        tick,
        CommandEventKind::CancelOrder,
        faction,
        format!("Took {label} out of the build queue — the ground is left as it stands"),
        Some(format!(
            "status=applied action=unqueue source={label} bands={dropped}"
        )),
    );
}

/// **RE-ORDER ONE BAND'S BUILD QUEUE** — `build_order <faction> <band> <source…> <position>`.
///
/// **The queue's defining input.** The whole `builders` pool goes on the head entry until its meter
/// fills, so where an entry sits *is* what it is funded at — and re-ordering is the one input a list
/// can carry that a stepper cannot (`docs/plan_standing_upkeep.md` §2.5).
///
/// `position` is 0-based and **clamped** to the queue's length rather than refused: naming a place
/// past the end is unambiguously *"put it last"*, which is a coherent version of the same order.
/// A source the band has not queued **is** refused — inventing an entry here would enrol a build the
/// player never declared.
fn handle_build_order(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_id: u64,
    source: BuildSourceRef,
    position: u32,
) {
    let label = source.label();
    let Some(target) = source.target() else {
        emit_command_failure(
            app,
            CommandEventKind::CancelOrder,
            faction,
            "build_order needs a source: two numbers name a tile, one token names a herd."
                .to_string(),
        );
        return;
    };
    let Some(build_source) = BuildSource::of(&target) else {
        return;
    };
    let Some(band) = select_starting_band(
        app,
        faction,
        Some(band_id),
        "build_order",
        CommandEventKind::CancelOrder,
    ) else {
        return;
    };
    let moved =
        band_allocation_mut(app, band.entity).move_build_entry(&build_source, position as usize);
    if !moved {
        emit_command_failure(
            app,
            CommandEventKind::CancelOrder,
            faction,
            format!("{} has nothing queued at {label} to re-order.", band.label),
        );
        return;
    }
    let landed = band_allocation_mut(app, band.entity)
        .build_queue_position(&build_source)
        .unwrap_or_default();
    let tick = app.world.resource::<SimulationTick>().0;
    push_command_event(
        app,
        tick,
        CommandEventKind::CancelOrder,
        faction,
        format!(
            "{}: {label} is now #{} in the build queue",
            band.label,
            landed + 1
        ),
        Some(format!(
            "status=applied action=build_order source={label} position={landed} band={band_id}"
        )),
    );
}

/// **MARK ONE WORKED ROW WITH THE PLAYER'S OWN RANK** — `work_priority <faction> <band> <x> <y>
/// <level>` / `work_priority <faction> <band> <herd_id> <level>` (`docs/plan_standing_upkeep.md`
/// §4.9 item 9b).
///
/// Sets [`core_sim::SourcePriority`] on the named band's assignment for that source. The take crew,
/// the floor, the kit and the queue entry are untouched — this says only *where this row stands when
/// the band runs short*.
///
/// # IT IS ONE BAND'S STATEMENT, WHICH IS WHY IT NAMES ONE
///
/// The ordering it feeds is a **band's**: the shedding walk partitions that band's own rows, and the
/// pen-feed split serves that band's own stores. `build_order` names a band for exactly the same
/// reason. The source-addressed verbs that reach *every* band working a source (`unqueue`,
/// `build_kit`) are the ones whose subject is the ground rather than the holding.
///
/// # AN UNKNOWN LEVEL IS REFUSED BY NAME
///
/// `upkeep_mode`'s rule. A rank the player mistyped must fail loudly rather than silently landing on
/// the default, which is the one value that would look like it worked.
fn handle_work_priority(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    band_id: u64,
    source: BuildSourceRef,
    level: String,
) {
    let label = source.label();
    let Some(target) = source.target() else {
        emit_command_failure(
            app,
            CommandEventKind::CancelOrder,
            faction,
            "work_priority needs a source: two numbers name a tile, one token names a herd."
                .to_string(),
        );
        return;
    };
    let Some(priority) = SourcePriority::from_token(level.trim().to_ascii_lowercase().as_str())
    else {
        emit_command_failure(
            app,
            CommandEventKind::CancelOrder,
            faction,
            format!(
                "Unknown work priority '{}' — expected {}, {} or {}.",
                level.trim(),
                SourcePriority::High.as_str(),
                SourcePriority::Normal.as_str(),
                SourcePriority::Low.as_str()
            ),
        );
        return;
    };
    let Some(band) = select_starting_band(
        app,
        faction,
        Some(band_id),
        "work_priority",
        CommandEventKind::CancelOrder,
    ) else {
        return;
    };
    let marked = band_allocation_mut(app, band.entity).set_source_priority(&target, priority);
    if !marked {
        emit_command_failure(
            app,
            CommandEventKind::CancelOrder,
            faction,
            format!("{} works nothing at {label} to rank.", band.label),
        );
        return;
    }
    let tick = app.world.resource::<SimulationTick>().0;
    info!(
        target: "shadow_scale::command",
        command = "work_priority",
        faction = %faction.0,
        band = band_id,
        source = %label,
        level = priority.as_str(),
        "command.work_priority.applied"
    );
    let sentence = match priority {
        SourcePriority::High => format!("{}: {label} is held before anything else", band.label),
        SourcePriority::Normal => format!("{}: {label} takes its turn like the rest", band.label),
        SourcePriority::Low => format!("{}: {label} is the first thing given up", band.label),
    };
    push_command_event(
        app,
        tick,
        CommandEventKind::CancelOrder,
        faction,
        sentence,
        Some(format!(
            "status=applied action=work_priority source={label} level={} band={band_id}",
            priority.as_str()
        )),
    );
}

/// **NAME THE KIT ONE QUEUED BUILD IS RAISED WITH** — `build_kit <faction> <x> <y> [kit <id>]` /
/// `build_kit <faction> <herd_id> [kit <id>]` (`docs/plan_standing_upkeep.md` §4.7a ②).
///
/// Sets [`core_sim::BuildQueueEntry::kit`] on every band of the faction that has the source queued —
/// the same `bands_working_source` reach [`handle_unqueue`] has, narrowed to the bands that actually
/// carry an entry. The row, its take crew and the meter are untouched.
///
/// # THE BUILDERS' KIT IS PER ENTRY, AND THIS IS THE ONLY OVERRIDE
///
/// A build's default kit is derived from that entry's own food web — a hoe for a Cultivate, hurdles
/// for a `Tame` — so a kit stored on the band's `builders` **row** is the one thing that derivation
/// cannot express: one pick pinned the animal web's tool onto every later plant build with no way
/// back. `handle_assign_labor` refuses a `kit` token on that role, and the override lives here.
///
/// # AN ABSENT `kit` TOKEN CLEARS IT
///
/// Back to the derivation, on the existing *"an absent `kitId` means the job's default"* rule — which
/// is what lets a client express *"back to default"* with no new vocabulary, since it already omits
/// the token whenever the selection equals the default. **`kit none` is bare-handed and is a real
/// selection**, which is how a player conserves gear on one job.
fn handle_build_kit(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    source: BuildSourceRef,
    kit_id: Option<String>,
) {
    let label = source.label();
    let Some(target) = source.target() else {
        emit_command_failure(
            app,
            CommandEventKind::CancelOrder,
            faction,
            "build_kit needs a source: two numbers name a tile, one token names a herd."
                .to_string(),
        );
        return;
    };
    let Some(build_source) = BuildSource::of(&target) else {
        return;
    };
    // **The kit resolves at the command boundary and FAILS CLOSED**, exactly as every other role's
    // does: an unknown id, or one whose `jobs` does not cover `builders`, is refused by name rather
    // than quietly becoming the derivation — naming a kit is how the player compares tools, so a
    // silent substitution answers a different question than the one asked.
    //
    // **The `absent` arm is never taken**: an absent token is handled above this call as *"clear the
    // override"*, so `resolve_kit_or` is only reached with a real id. The job default is passed
    // because the signature needs one, and it is the same fall-back `builders_kit_for` ends on.
    let kit = match kit_id {
        None => None,
        Some(id) => {
            let equipment_cfg = app.world.resource::<EquipmentConfigHandle>().get();
            let absent = equipment_cfg.default_kit(KitJob::Builders);
            match equipment_cfg.resolve_kit_or(Some(id.as_str()), KitJob::Builders, absent) {
                Ok(kit) => Some(kit),
                Err(reason) => {
                    emit_command_failure(
                        app,
                        CommandEventKind::CancelOrder,
                        faction,
                        format!("build_kit: {reason}."),
                    );
                    return;
                }
            }
        }
    };
    let mut set = 0usize;
    for band in bands_working_source(app, faction, &target) {
        if band_allocation_mut(app, band).set_build_entry_kit(&build_source, kit.clone()) {
            set += 1;
        }
    }
    if set == 0 {
        emit_command_failure(
            app,
            CommandEventKind::CancelOrder,
            faction,
            format!("Nothing of yours is queued to be built at {label}."),
        );
        return;
    }
    let tick = app.world.resource::<SimulationTick>().0;
    // **The feed says which kit, or that the job is back on its own default** — a player who cleared
    // an override must be able to see that they did, and `none` is a kit id rather than a clearing.
    let (sentence, action) = match kit.as_ref() {
        Some(kit) => (
            format!("{label} will be built with {}", kit.id()),
            format!("kit={}", kit.id()),
        ),
        None => (
            format!("{label} is back on the kit its own web wants"),
            "kit=default".to_string(),
        ),
    };
    push_command_event(
        app,
        tick,
        CommandEventKind::CancelOrder,
        faction,
        sentence,
        Some(format!(
            "status=applied action=build_kit source={label} {action} bands={set}"
        )),
    );
}

/// **NAME THE KIT ONE WORK SITE IS KEPT WITH** — `upkeep_kit <faction> <x> <y> [kit <id>]` /
/// `upkeep_kit <faction> <herd_id> [kit <id>]` (`docs/plan_standing_upkeep.md` §2.7).
///
/// Sets [`core_sim::LaborAssignment::upkeep_kit`] on every band of the faction that works the
/// source — the same `bands_working_source` reach [`handle_build_kit`] has, and the wider one of the
/// two: a keeping bill is owed by every band holding the ground, not only by whoever queued a build
/// on it. The take crew, its own kit, the queue entry and the meter are untouched.
///
/// # THE KEEPING KIT IS PER WORK SITE, AND THIS IS THE ONLY OVERRIDE
///
/// The band is the pool of workers and goods to draw from; it does not decide which tool a given
/// site is worked with. A kit stored on the band's `agriculture` / `husbandry` **role row** — where
/// this lived until §2.7 — is the one thing a per-site derivation cannot express: one pick put the
/// same tool on every site that band kept, with no way back. `handle_assign_labor` names no kit on
/// those roles, and the override lives here.
///
/// # AN ABSENT `kit` TOKEN CLEARS IT
///
/// Back to the site's own web derivation, on the existing *"an absent `kitId` means the job's
/// default"* rule. **`kit none` is bare-handed and is a real selection**, which is how a player
/// conserves the tool on one site while its neighbour goes on using it.
///
/// # ⛔ AND THE KIT MUST SERVE THIS SITE'S WEB
///
/// A patch is kept on the `agriculture` job and a herd on `husbandry`
/// ([`core_sim::EquipmentConfig::keeping_job`]), so naming a plant keeping kit on a herd is a
/// **command failure** rather than a silent fall back to the derivation — `build_kit`'s rule, and
/// for its reason: naming a kit is how a player compares tools, so a silent substitution answers a
/// different question than the one asked.
fn handle_upkeep_kit(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    source: BuildSourceRef,
    kit_id: Option<String>,
) {
    let label = source.label();
    let Some(target) = source.target() else {
        emit_command_failure(
            app,
            CommandEventKind::CancelOrder,
            faction,
            "upkeep_kit needs a source: two numbers name a tile, one token names a herd."
                .to_string(),
        );
        return;
    };
    let Some(build_source) = BuildSource::of(&target) else {
        return;
    };
    let branch = match build_source {
        BuildSource::Patch(_) => core_sim::RungBranch::Plant,
        BuildSource::Herd(_) => core_sim::RungBranch::Animal,
        // Unreachable through this path — `BuildSource::of` maps a *labor target*, and no target
        // names a road (the keeping is the band-wide `Roadwork` row, which names no tile). The arm
        // is stated rather than wildcarded so a future road labor row fails to compile here.
        BuildSource::Road(_) => core_sim::RungBranch::Route,
    };
    // **The kit resolves at the command boundary and FAILS CLOSED** — see the doc above. The
    // `absent` arm is never taken: an absent token is handled here as *"clear the override"*, so
    // `resolve_kit_or` is only reached with a real id, and the job default it is passed is the same
    // fall-back `keeping_kit_for` ends on.
    let job = core_sim::EquipmentConfig::keeping_job(branch);
    let kit = match kit_id {
        None => None,
        Some(id) => {
            let equipment_cfg = app.world.resource::<EquipmentConfigHandle>().get();
            let absent = equipment_cfg.default_kit(job);
            match equipment_cfg.resolve_kit_or(Some(id.as_str()), job, absent) {
                Ok(kit) => Some(kit),
                Err(reason) => {
                    emit_command_failure(
                        app,
                        CommandEventKind::CancelOrder,
                        faction,
                        format!("upkeep_kit: {reason}."),
                    );
                    return;
                }
            }
        }
    };
    let mut set = 0usize;
    for band in bands_working_source(app, faction, &target) {
        if band_allocation_mut(app, band).set_upkeep_kit(&target, kit.clone()) {
            set += 1;
        }
    }
    if set == 0 {
        emit_command_failure(
            app,
            CommandEventKind::CancelOrder,
            faction,
            format!("Nothing of yours works {label} to keep."),
        );
        return;
    }
    let tick = app.world.resource::<SimulationTick>().0;
    // **The feed says which kit, or that the site is back on its own default** — a player who
    // cleared an override must be able to see that they did, and `none` is a kit id rather than a
    // clearing.
    let (sentence, action) = match kit.as_ref() {
        Some(kit) => (
            format!("{label} will be kept with {}", kit.id()),
            format!("kit={}", kit.id()),
        ),
        None => (
            format!("{label} is back on the keeping kit its own web wants"),
            "kit=default".to_string(),
        ),
    };
    push_command_event(
        app,
        tick,
        CommandEventKind::CancelOrder,
        faction,
        sentence,
        Some(format!(
            "status=applied action=upkeep_kit source={label} {action} bands={set}"
        )),
    );
}

/// Every band of `faction` with a row on this source — the set all four queue verbs act over, and
/// the same "a verb reaches only bands that already work the source" rule
/// [`queue_build_on_working_bands`] applies.
///
/// **A row at zero take counts**, unlike the queueing path's: a holding the player is putting down
/// or withdrawing is exactly the row that may have no gatherers left on it.
fn bands_working_source(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    target: &LaborTarget,
) -> Vec<Entity> {
    app.world
        .query::<(Entity, &PopulationCohort, &LaborAllocation)>()
        .iter(&app.world)
        .filter(|(_, cohort, _)| cohort.faction == faction)
        .filter(|(_, _, allocation)| {
            allocation
                .assignments
                .iter()
                .any(|assignment| assignment.target.same_source(target))
        })
        .map(|(entity, _, _)| entity)
        .collect()
}

/// **QUEUE A BUILD ON EVERY BAND OF `faction` ALREADY WORKING `target`'s SOURCE** (matched by
/// `LaborTarget::same_source`, so the tile / herd id). Returns how many bands were updated (`0` =
/// nobody is working that source, which the callers report as *"staff it first"*). The shared body
/// of the four improvement verbs and of `extend_pen`.
///
/// # A VERB DECLARES; IT DOES NOT STAFF
///
/// It appends a [`BuildQueueEntry`] and nothing else (`docs/plan_standing_upkeep.md` §2.5). The
/// hands are the band's `builders` role, which the player staffs separately and which funds only the
/// **head** of the queue — so this command names no crew, refuses nothing on affordability, and
/// cannot disband the gathering it was meant to improve.
///
/// # RE-ISSUING KEEPS THE ENTRY'S PLACE IN THE LINE
///
/// `LaborAllocation::enqueue_build` replaces `declared` in place, so correcting `cultivate` → `sow`
/// on a queued source does not cost the player their position. Withdrawing one is `unqueue`.
///
/// **A verb reaches only bands already WORKING the source** — the rule this function has always
/// carried, and now the rule that keeps an entry paired with a row.
fn queue_build_on_working_bands(
    app: &mut bevy::prelude::App,
    faction: FactionId,
    target: &LaborTarget,
    declared: BuildJob,
) -> usize {
    let Some(source) = BuildSource::of(target) else {
        return 0;
    };
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
            allocation.enqueue_build(source.clone(), declared);
        }
        // Queueing a build does not change the take at all, but the source's dip does depend on
        // what is being raised on it, so re-seed the telemetry from the new forecast — the same
        // reason `handle_assign_labor` seeds after a stance edit.
        let improvement = build_verb_on_source_at(app, *entity, band_target);
        seed_source_yield(app, *entity, band_target, improvement, *workers);
    }
    bands.len()
}

/// **WHAT ONE BAND IS ACTUALLY RAISING ON A SOURCE** — its queue entry's declaration resolved
/// against the ground (`forage::patch_build_verb` / `fauna::herd_build_verb`), which is what the
/// yield seed has to price: a declaration answers only while the meter it names is at zero.
///
/// `None` for a source this band has not queued, for a ring (which raises no rung and pays no dip),
/// and for an entry whose ground has moved on — a **dead** entry.
fn build_verb_on_source(
    allocation: &LaborAllocation,
    target: &LaborTarget,
    app: &bevy::prelude::App,
) -> Option<Improvement> {
    let source = BuildSource::of(target)?;
    let declared = match allocation.build_queue_entry(&source)?.declared {
        BuildJob::Rung(improvement) => improvement,
        BuildJob::ExtendPen => return None,
    };
    match source {
        BuildSource::Patch(tile) => app
            .world
            .resource::<ForageRegistry>()
            .patch(tile)
            .and_then(|patch| patch_build_verb(patch, Some(declared))),
        BuildSource::Herd(id) => app
            .world
            .resource::<HerdRegistry>()
            .find(&id)
            .and_then(|herd| herd_build_verb(herd, Some(declared))),
        // **A road's declaration answers for itself**, with no meter-derived twin behind it: `grade`
        // and `pave` are the only things that ever raise a road, so there is no second statement for
        // the ground to override. Unreachable through this path in any case — the source comes from a
        // *labor target*, and no target names a road.
        BuildSource::Road(_) => Some(declared),
    }
}

/// [`build_verb_on_source`] for a band named by entity — the shape the queueing loop needs, where
/// the allocation borrow has already been released.
fn build_verb_on_source_at(
    app: &bevy::prelude::App,
    band: Entity,
    target: &LaborTarget,
) -> Option<Improvement> {
    let allocation = app.world.get::<LaborAllocation>(band)?.clone();
    build_verb_on_source(&allocation, target, app)
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

    // The event log's turn window is a live tunable: re-window (and prune) the running log so the
    // reloaded number is the one the next snapshot publishes.
    if let Some(mut log) = app.world.get_resource_mut::<CommandEventLog>() {
        log.set_retention_turns(new_config.command_events_retention_turns);
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

    let population = new_config.population();
    info!(
        target: "shadow_scale::config",
        path = applied_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "builtin".to_string()),
        attrition_penalty_scale = population.attrition_penalty_scale().to_f32(),
        hardness_penalty_scale = population.hardness_penalty_scale().to_f32(),
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

/// Map a decoded wire payload onto the loop's [`Command`].
///
/// `reply` is the **connection's** reply channel, threaded through so the one payload that expects
/// an answer can carry a way back to the client that sent it. Every other arm ignores it.
fn command_from_payload(
    payload: ProtoCommandPayload,
    reply: &Sender<QueryReplyEnvelope>,
) -> Option<Command> {
    match payload {
        ProtoCommandPayload::Turn { steps } => Some(Command::Turn(steps)),
        ProtoCommandPayload::ResetMap { width, height } => {
            Some(Command::ResetMap { width, height })
        }
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
            kit_id,
            take_species,
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
            kit_id,
            take_species,
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
        ProtoCommandPayload::SplitBand {
            faction_id,
            band_id,
            workers,
        } => Some(Command::SplitBand {
            faction: FactionId(faction_id),
            band_id,
            workers,
        }),
        ProtoCommandPayload::SendHuntExpedition {
            faction_id,
            band_id,
            party_workers,
            fauna_id,
            floor,
            kit_id,
        } => Some(Command::SendHuntExpedition {
            faction: FactionId(faction_id),
            band_id,
            party_workers,
            fauna_id,
            floor,
            kit_id,
        }),
        ProtoCommandPayload::SendDenialRaid {
            faction_id,
            band_id,
            party_workers,
            fauna_id,
            kit_id,
        } => Some(Command::SendDenialRaid {
            faction: FactionId(faction_id),
            band_id,
            party_workers,
            fauna_id,
            kit_id,
        }),
        ProtoCommandPayload::SendTradeExpedition {
            faction_id,
            band_id,
            party_workers,
            destination_band_id,
            cargo,
            kit_id,
        } => Some(Command::SendTradeExpedition {
            faction: FactionId(faction_id),
            band_id,
            party_workers,
            destination_band_id,
            cargo,
            kit_id,
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
        ProtoCommandPayload::Grade {
            faction_id,
            band_id,
            target_x,
            target_y,
        } => Some(Command::Grade {
            faction: FactionId(faction_id),
            band_id,
            target_x,
            target_y,
        }),
        ProtoCommandPayload::Pave {
            faction_id,
            band_id,
            target_x,
            target_y,
        } => Some(Command::Pave {
            faction: FactionId(faction_id),
            band_id,
            target_x,
            target_y,
        }),
        ProtoCommandPayload::Abandon {
            faction_id,
            target_x,
            target_y,
            herd_id,
        } => Some(Command::Abandon {
            faction: FactionId(faction_id),
            source: BuildSourceRef {
                target_x,
                target_y,
                herd_id,
            },
        }),
        ProtoCommandPayload::Unqueue {
            faction_id,
            target_x,
            target_y,
            herd_id,
        } => Some(Command::Unqueue {
            faction: FactionId(faction_id),
            source: BuildSourceRef {
                target_x,
                target_y,
                herd_id,
            },
        }),
        ProtoCommandPayload::BuildOrder {
            faction_id,
            band_id,
            target_x,
            target_y,
            herd_id,
            position,
        } => Some(Command::BuildOrder {
            faction: FactionId(faction_id),
            band_id,
            source: BuildSourceRef {
                target_x,
                target_y,
                herd_id,
            },
            position,
        }),
        ProtoCommandPayload::BuildKit {
            faction_id,
            target_x,
            target_y,
            herd_id,
            kit_id,
        } => Some(Command::BuildKit {
            faction: FactionId(faction_id),
            source: BuildSourceRef {
                target_x,
                target_y,
                herd_id,
            },
            kit_id,
        }),
        ProtoCommandPayload::UpkeepKit {
            faction_id,
            target_x,
            target_y,
            herd_id,
            kit_id,
        } => Some(Command::UpkeepKit {
            faction: FactionId(faction_id),
            source: BuildSourceRef {
                target_x,
                target_y,
                herd_id,
            },
            kit_id,
        }),
        ProtoCommandPayload::WorkPriority {
            faction_id,
            band_id,
            target_x,
            target_y,
            herd_id,
            level,
        } => Some(Command::WorkPriority {
            faction: FactionId(faction_id),
            band_id,
            source: BuildSourceRef {
                target_x,
                target_y,
                herd_id,
            },
            level,
        }),
        ProtoCommandPayload::BenchPriority {
            faction_id,
            band_id,
            level,
        } => Some(Command::BenchPriority {
            faction: FactionId(faction_id),
            band_id,
            level,
        }),
        ProtoCommandPayload::UpkeepMode {
            faction_id,
            band_id,
            mode,
        } => Some(Command::UpkeepMode {
            faction: FactionId(faction_id),
            band_id,
            mode,
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
        ProtoCommandPayload::SetBench {
            faction_id,
            band_id,
            recipe_id,
            workers,
        } => Some(Command::SetBench {
            faction: FactionId(faction_id),
            band_id: Some(band_id),
            recipe_id,
            workers,
        }),
        ProtoCommandPayload::ClearBench {
            faction_id,
            band_id,
        } => Some(Command::ClearBench {
            faction: FactionId(faction_id),
            band_id: Some(band_id),
        }),
        ProtoCommandPayload::BenchCrew {
            faction_id,
            band_id,
            workers,
        } => Some(Command::BenchCrew {
            faction: FactionId(faction_id),
            band_id: Some(band_id),
            workers,
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
        // The one payload that carries a way BACK. `reply` is this connection's writer channel, so
        // the answer reaches the client that asked even if it is computed several commands later.
        ProtoCommandPayload::Query { request_id, query } => Some(Command::Query {
            request_id,
            query,
            reply: reply.clone(),
        }),
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
        CommandEventKind::Road => "Road",
        CommandEventKind::Craft => "Craft",
        CommandEventKind::KitLife => "Kit life",
        CommandEventKind::MaterialShortfall => "Material shortfall",
        CommandEventKind::HuntDanger => "Dangerous hunt",
        CommandEventKind::HuntReport => "Hunt report",
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
        CommandEventKind::TradeDelivered => "Trade delivered",
        CommandEventKind::BandFounded => "Band founded",
        CommandEventKind::HerdUnderHerded => "Under-herded",
        // The demographic kinds are world events, not commands — they never reach
        // `emit_command_failure`. Named anyway so the display map stays total.
        CommandEventKind::Born => "Birth",
        CommandEventKind::Died => "Death",
        CommandEventKind::CameOfAge => "Came of age",
        CommandEventKind::Aged => "Joined the elders",
        CommandEventKind::Migrated => "Migration",
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
    if is_replayable(command) {
        log.push(LogEntry::Command(command.clone()));
    }
}

/// **Does replaying this command reproduce anything?** The exclusion list of
/// [`log_dispatched_command`], as its own predicate so it can be asserted without standing a world
/// up to hold a [`CommandLog`].
///
/// The reasons differ per variant and are given in that function's docs. `Query` is the newest and
/// the plainest: it mutates nothing, so there is nothing to reproduce, and it carries a reply
/// channel belonging to a connection a replay does not have.
///
/// **A query is dispatched before `log_dispatched_command` is ever reached, so it cannot arrive
/// there today.** It is named here anyway, because the exclusion is a property of the *command* —
/// not of where the dispatcher happens to match it — and a refactor that routed it through the
/// generic arm must not silently start logging questions.
fn is_replayable(command: &Command) -> bool {
    !matches!(
        command,
        Command::ReloadConfig { .. }
            | Command::Rollback { .. }
            | Command::SetConfigOverride { .. }
            | Command::ClearConfigOverrides
            | Command::Query { .. }
    )
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
        // **Unreachable by construction** — the main loop answers a query and `continue`s, so one
        // never gets here. It is a loud no-op rather than an `unreachable!` because the cost of
        // being wrong differs by orders of magnitude: a panic here would take the whole server down
        // over a question, while a warning leaves one query unanswered and names the routing bug in
        // the log.
        Command::Query { request_id, .. } => {
            warn!(
                target: "shadow_scale::server",
                request_id,
                "query.misrouted=a query reached apply_command; it is answered by the dispatcher"
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
        Command::Orders { faction, orders } => {
            handle_order_submission(app, faction, orders);
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
            kit_id,
            take_species,
        } => {
            handle_assign_labor(
                app,
                faction,
                band_id,
                role,
                workers,
                target_x,
                target_y,
                fauna_id,
                species,
                floor,
                kit_id,
                take_species,
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
        Command::SplitBand {
            faction,
            band_id,
            workers,
        } => {
            handle_split_band(app, faction, band_id, workers);
        }
        Command::SendHuntExpedition {
            faction,
            band_id,
            party_workers,
            fauna_id,
            floor,
            kit_id,
        } => {
            handle_send_hunt_expedition(
                app,
                faction,
                band_id,
                party_workers,
                fauna_id,
                floor,
                kit_id,
            );
        }
        Command::SendDenialRaid {
            faction,
            band_id,
            party_workers,
            fauna_id,
            kit_id,
        } => {
            handle_send_denial_raid(app, faction, band_id, party_workers, fauna_id, kit_id);
        }
        Command::SendTradeExpedition {
            faction,
            band_id,
            party_workers,
            destination_band_id,
            cargo,
            kit_id,
        } => {
            handle_send_trade_expedition(
                app,
                faction,
                band_id,
                party_workers,
                destination_band_id,
                cargo,
                kit_id,
            );
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
        Command::UpkeepMode {
            faction,
            band_id,
            mode,
        } => {
            handle_upkeep_mode(app, faction, band_id, mode);
        }
        Command::Grade {
            faction,
            band_id,
            target_x,
            target_y,
        } => {
            handle_road_verb(
                app,
                faction,
                band_id,
                UVec2::new(target_x, target_y),
                Improvement::Grade,
            );
        }
        Command::Pave {
            faction,
            band_id,
            target_x,
            target_y,
        } => {
            handle_road_verb(
                app,
                faction,
                band_id,
                UVec2::new(target_x, target_y),
                Improvement::Pave,
            );
        }
        Command::Abandon { faction, source } => {
            handle_abandon(app, faction, source);
        }
        Command::Unqueue { faction, source } => {
            handle_unqueue(app, faction, source);
        }
        Command::BuildKit {
            faction,
            source,
            kit_id,
        } => {
            handle_build_kit(app, faction, source, kit_id);
        }
        Command::BuildOrder {
            faction,
            band_id,
            source,
            position,
        } => {
            handle_build_order(app, faction, band_id, source, position);
        }
        Command::UpkeepKit {
            faction,
            source,
            kit_id,
        } => {
            handle_upkeep_kit(app, faction, source, kit_id);
        }
        Command::WorkPriority {
            faction,
            band_id,
            source,
            level,
        } => {
            handle_work_priority(app, faction, band_id, source, level);
        }
        Command::ExtendPen {
            faction,
            target_x,
            target_y,
        } => {
            handle_extend_pen(app, faction, UVec2::new(target_x, target_y));
        }
        Command::SetBench {
            faction,
            band_id,
            recipe_id,
            workers,
        } => {
            handle_set_bench(app, faction, band_id, &recipe_id, workers);
        }
        Command::ClearBench { faction, band_id } => {
            handle_clear_bench(app, faction, band_id);
        }
        Command::BenchCrew {
            faction,
            band_id,
            workers,
        } => {
            handle_bench_crew(app, faction, band_id, workers);
        }
        Command::BenchPriority {
            faction,
            band_id,
            level,
        } => {
            handle_bench_priority(app, faction, band_id, level);
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
        avg_temp = metrics.avg_temperature,
        // The connection subsystem's only observer outside tests until a client reads the
        // `connections` section (#517/#232): a tie forming or being reaped is otherwise invisible in
        // a running game, which makes the primitive impossible to play-test.
        connections_live = metrics.connections_live,
        connections_formed = metrics.connections_formed,
        connections_reaped = metrics.connections_reaped,
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
    use core_sim::NO_CREW_ON_THIS_ACTIVITY;

    /// **The shipped EQUIPPED haul rate** — off the sled's own tier. `labor_config`'s
    /// `hunt.per_worker_biomass_capacity` is the *bare-handed* baseline since quality tiers landed.
    fn equipped_haul_reference() -> f32 {
        core_sim::EquipmentConfig::builtin().equipped_reference(
            core_sim::EquipmentStat::HuntCarry,
            core_sim::LaborConfig::builtin()
                .hunt
                .per_worker_biomass_capacity,
        )
    }

    /// The gather twin of [`equipped_haul_reference`] — the baskets' own tier.
    fn equipped_gather_reference() -> f32 {
        core_sim::EquipmentConfig::builtin().equipped_reference(
            core_sim::EquipmentStat::ForageCarry,
            core_sim::LaborConfig::builtin()
                .forage
                .per_worker_biomass_capacity,
        )
    }

    use super::*;
    use bevy::math::UVec2;
    // The ladder's knowledge ids are named only by the tests now: the handlers resolve their gate
    // off the rung record (`unlock_discovery_id`), never a hard-coded id.
    use core_sim::{
        build_test_app, default_species_for_rung, EcologyPhase, FoodModule, FoodSiteEntry,
        ForagePatch, SourcePriority, CULTIVATION_DISCOVERY_ID, FABRICATED_BUILD_COST,
        HERDING_DISCOVERY_ID, PENNING_DISCOVERY_ID, SEED_SELECTION_DISCOVERY_ID, SITE_ACCEPTED,
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
                    size: BAND_WORKING_AGE,
                    children: core_sim::scalar_zero(),
                    working: scalar_from_f32(BAND_WORKING_AGE as f32),
                    elders: core_sim::scalar_zero(),
                    stores: LocalStore::new(),
                    morale: core_sim::scalar_one(),
                    last_food_consumption: 0.0,
                    last_turn_transfer_received: 0.0,
                    last_turn_transfer_sent: 0.0,
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
                        kit: None,
                        priority: SourcePriority::default(),
                        upkeep_kit: None,
                    }],
                    ..Default::default()
                },
                // A spawned band is KITTED, exactly as `spawn_profile_population` spawns one —
                // **stated**, because an absent ledger entry means NOT OWNED since the count slice,
                // and **sized to the band's workers**, because a spawn stocks a party's worth: one
                // unit each would arm one of these thirty and leave the rest bare-handed
                // (`equipment.md` → "the partly-equipped party").
                BandEquipment::start_stocked_for(
                    &core_sim::EquipmentConfig::builtin(),
                    BAND_WORKING_AGE as f32,
                ),
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
    /// Every fixture band's working-age head count — also what its start stock is sized against, so
    /// the two cannot drift and leave a fixture band partly equipped for reasons no test is about.
    const BAND_WORKING_AGE: u32 = 30;

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
        let app = build_test_app();
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
                    take_species: TakeSelection::EVERYTHING,
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
        let mut app = build_test_app();
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

    /// **What the band has DECLARED on its single worked source** — what the four build verbs
    /// append to the band's queue, and what a command they reject must leave alone
    /// (`docs/plan_standing_upkeep.md` §2.5).
    ///
    /// It reads the *declaration*, not the derived rung: what a rejected command must not have done
    /// is change the player's stated intent.
    fn band_improvement(app: &bevy::prelude::App, band: Entity) -> Option<Improvement> {
        let allocation = app
            .world
            .get::<LaborAllocation>(band)
            .expect("band has an allocation");
        let source = BuildSource::of(&allocation.assignments[0].target)?;
        match allocation.build_queue_entry(&source)?.declared {
            BuildJob::Rung(improvement) => Some(improvement),
            BuildJob::ExtendPen => None,
        }
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
        let mut app = build_test_app();
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
                take_species: TakeSelection::EVERYTHING,
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
    // **RETIRED: `a_verb_asking_for_more_hands_than_the_band_has_is_refused_outright` and
    // `extend_pen_claims_the_rings_hands_and_refuses_a_ring_the_band_cannot_staff`** — the two tests
    // that pinned the improvement verbs' affordability refusal.
    //
    // **A VERB NAMES NO CREW ANY MORE** (`docs/plan_standing_upkeep.md` §2.5), so there is no number
    // for it to refuse: the five build verbs append a queue entry, and the hands stand on the band's
    // `builders` role, whose stepper clamps against idle exactly as scout's and warrior's do. What
    // survives of the invariant is `assign_labor`'s own clamp, pinned where it lives, and the
    // enrolment rule below — a verb still reaches only bands that already work the source.

    /// **A VERB DECLARES; IT DOES NOT STAFF** (`docs/plan_standing_upkeep.md` §2.5) — and it reaches
    /// only bands that already work the source, which is the rule that keeps a queue entry paired
    /// with a row.
    ///
    /// **The ring is the interesting half.** `extend_pen` rides the same `animal:pen` rung as the
    /// pen it widens, so it waits in the same queue and is funded by the same pool — but it is the
    /// one entry kind that names no rung verb, because a built pen carries no meter for the derived
    /// verb to name. That is exactly the gap `BuildJob::ExtendPen` fills, and it is why the ring is
    /// command-driven in the first place.
    #[test]
    fn a_build_verb_queues_the_source_and_stands_nobody_on_it() {
        let mut app = build_test_app();
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
                take_species: TakeSelection::EVERYTHING,
            },
        );

        handle_cultivate(&mut app, faction, coord);

        assert_eq!(
            band_improvement(&app, band),
            Some(Improvement::Cultivate),
            "the verb's whole effect is the declaration"
        );
        assert_eq!(
            band_build_crew(&app, band),
            NO_CREW_ON_THIS_ACTIVITY,
            "…and it staffs nobody: the builders are the player's own `assign_labor … builders <n>`"
        );
        assert_eq!(
            band_queue_position(&app, band, BuildSource::Patch(coord)),
            Some(0),
            "a first declaration goes to the head of an empty queue"
        );
    }

    /// **A RING IS A QUEUE ENTRY LIKE EVERY OTHER BUILD**, under its own kind.
    #[test]
    fn extend_pen_queues_the_ring_under_its_own_kind() {
        let mut app = build_test_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let id = seed_penned_herd(&mut app, coord, Some(faction));
        grant_penning(&mut app, faction);
        let band = spawn_working_band(
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
            "the ring is under way"
        );
        assert_eq!(
            band_queue_job(&app, band, BuildSource::Herd(id.clone())),
            Some(BuildJob::ExtendPen),
            "a ring waits in the same queue, under the kind that names no rung"
        );
        assert_eq!(
            band_improvement(&app, band),
            None,
            "…and it declares no rung verb — a built pen has no meter left for one to name"
        );
        assert_eq!(
            band_build_crew(&app, band),
            NO_CREW_ON_THIS_ACTIVITY,
            "the ring names no crew either; the `builders` pool raises it when it reaches the head"
        );
    }

    /// Where a source sits in one band's build queue.
    fn band_queue_position(
        app: &bevy::prelude::App,
        band: Entity,
        source: BuildSource,
    ) -> Option<usize> {
        app.world
            .get::<LaborAllocation>(band)
            .expect("band has an allocation")
            .build_queue_position(&source)
    }

    /// What one band has declared on a source.
    fn band_queue_job(
        app: &bevy::prelude::App,
        band: Entity,
        source: BuildSource,
    ) -> Option<BuildJob> {
        app.world
            .get::<LaborAllocation>(band)
            .expect("band has an allocation")
            .build_queue_entry(&source)
            .map(|entry| entry.declared)
    }

    /// **The band's BUILDERS POOL** — one head count for the whole band since
    /// `docs/plan_standing_upkeep.md` §2.5, where it used to be one crew per source.
    fn band_build_crew(app: &bevy::prelude::App, band: Entity) -> u32 {
        app.world
            .get::<LaborAllocation>(band)
            .expect("band has an allocation")
            .workers_on(&LaborTarget::Builders)
    }

    /// **`upkeep_mode` sets the band's fund mode, both ways, and refuses a mode it does not know**
    /// (`docs/plan_standing_upkeep.md` §2.5).
    ///
    /// It is what is left of the retired `maintain` once maintenance left the tile: the keeping's
    /// *crew* is `assign_labor … agriculture|husbandry <workers>` now, and what remains to state is
    /// what happens when that pool cannot cover the band's whole web.
    ///
    /// **Both directions are asserted**, because a one-way command would be a trap — and the
    /// refusal is, because silently reading a typo as the default would leave the player believing
    /// they had protected their Field.
    #[test]
    fn upkeep_mode_sets_the_bands_fund_mode_and_refuses_an_unknown_one() {
        let mut app = build_test_app();
        app.update();
        let faction = FactionId(0);
        let band = starting_band_id(&mut app, faction);
        assert_eq!(
            band_fund_mode(&mut app, faction),
            UpkeepFundMode::Spread,
            "an unstated policy is `spread`: nobody is singled out"
        );

        handle_upkeep_mode(&mut app, faction, band, "priority".to_string());
        assert_eq!(band_fund_mode(&mut app, faction), UpkeepFundMode::Priority);

        handle_upkeep_mode(&mut app, faction, band, "spread".to_string());
        assert_eq!(
            band_fund_mode(&mut app, faction),
            UpkeepFundMode::Spread,
            "…and it goes back, so the choice is a dial rather than a ratchet"
        );

        handle_upkeep_mode(&mut app, faction, band, "sideways".to_string());
        assert!(
            cancel_failure_detail_contains(&app, "Unknown upkeep mode"),
            "an unknown mode is named, not guessed at"
        );
        assert_eq!(
            band_fund_mode(&mut app, faction),
            UpkeepFundMode::Spread,
            "and a refused mode changes nothing"
        );
    }

    /// **The keeping is staffed like any other band-wide role**, through `assign_labor` — which is
    /// the whole of what "maintenance left the tile" means at the command boundary. `0` stops
    /// maintaining that web, exactly as `0` unassigns any other row.
    #[test]
    fn assign_labor_staffs_the_two_keeping_roles() {
        let mut app = build_test_app();
        app.update();
        let faction = FactionId(0);
        let band = starting_band_id(&mut app, faction);
        const KEEPERS: u32 = 3;

        handle_assign_labor(
            &mut app,
            faction,
            Some(band),
            "agriculture".to_string(),
            KEEPERS,
            None,
            None,
            None,
            None,
            None,
            None,
            Vec::new(),
        );
        assert_eq!(
            role_crew(&mut app, faction, &LaborTarget::Agriculture),
            KEEPERS
        );
        assert_eq!(
            role_crew(&mut app, faction, &LaborTarget::Husbandry),
            NO_CREW_ON_THIS_ACTIVITY,
            "the two webs are separate pools — staffing one never staffs the other"
        );

        handle_assign_labor(
            &mut app,
            faction,
            Some(band),
            "agriculture".to_string(),
            NO_CREW_ON_THIS_ACTIVITY,
            None,
            None,
            None,
            None,
            None,
            None,
            Vec::new(),
        );
        assert_eq!(
            role_crew(&mut app, faction, &LaborTarget::Agriculture),
            NO_CREW_ON_THIS_ACTIVITY,
            "…and zero takes them off again — that is the whole of 'stop maintaining this web'"
        );
    }

    /// The `band_id` of this faction's first band — what a per-band command names. It is the band's
    /// own [`BandId`], not its entity bits: `resolve_starting_unit_entity` matches on the component,
    /// because an entity id does not survive a checkpoint restore.
    fn starting_band_id(app: &mut bevy::prelude::App, faction: FactionId) -> u64 {
        app.world
            .query::<(&PopulationCohort, &BandId)>()
            .iter(&app.world)
            .find(|(cohort, _)| cohort.faction == faction)
            .map(|(_, id)| id.0)
            .expect("the start profile seeded a band")
    }

    /// This faction's band's maintenance fund mode. **Queried rather than read off a stored
    /// `Entity`**, because `restore_sim_state` respawns bands and the id does not survive it.
    fn band_fund_mode(app: &mut bevy::prelude::App, faction: FactionId) -> UpkeepFundMode {
        app.world
            .query::<(&PopulationCohort, &LaborAllocation)>()
            .iter(&app.world)
            .find(|(cohort, _)| cohort.faction == faction)
            .map(|(_, allocation)| allocation.upkeep_fund_mode)
            .expect("the faction has a band")
    }

    /// The hands this faction has on one band-wide role, summed across its bands.
    fn role_crew(app: &mut bevy::prelude::App, faction: FactionId, role: &LaborTarget) -> u32 {
        app.world
            .query::<(&PopulationCohort, &LaborAllocation)>()
            .iter(&app.world)
            .filter(|(cohort, _)| cohort.faction == faction)
            .map(|(_, allocation)| allocation.workers_on(role))
            .sum()
    }

    /// `upkeep_mode`'s refusals ride the `CancelOrder` feed channel, exactly as
    /// the retired `abandon_improvement`'s did — the two were siblings and read on one line.
    fn cancel_failure_detail_contains(app: &bevy::prelude::App, needle: &str) -> bool {
        app.world.resource::<CommandEventLog>().iter().any(|entry| {
            matches!(entry.kind, CommandEventKind::CancelOrder)
                && entry
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains(needle))
        })
    }

    /// **`cultivate` is ACCEPTED on a non-Thriving patch** — the positive pin on the gate
    /// `docs/plan_harvest_floor.md` §3.2 deleted. It replaced
    /// `cultivate_rejected_on_a_stressed_patch`, whose subject is gone: the floor turned the health
    /// cliff into a rate, so pulling hard on ground you are clearing *slows* the meter instead of
    /// refusing the verb, and there is no lapse state left to be exempt from. Stated as a test
    /// rather than deleted, because a re-added phase check would be silent otherwise.
    #[test]
    fn cultivate_is_accepted_on_a_stressed_patch() {
        let mut app = build_test_app();
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
                take_species: TakeSelection::EVERYTHING,
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
        let mut app = build_test_app();
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
                take_species: TakeSelection::EVERYTHING,
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
        let mut app = build_test_app();
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

    // --- AN ACHIEVED RUNG SHORT OF ITS COST IS REPAIRABLE; A FULL ONE IS REFUSED ----------------
    //
    // `docs/plan_standing_upkeep.md` §2.4: *"repairing it is a fresh decision the player makes by
    // putting it back in the queue"*. Three gates composed to make that impossible — completion
    // retires the queue entry, no entry means no builders, and the verb's own refusal read the
    // **retention bar** (37.5 of 50) rather than the **cost**, so a patch eroded to 99% answered
    // *"already cultivated"* and could not be re-queued at all. The escape was to stop paying
    // keeping, bleed 12 units, lose the rung and re-buy it.
    //
    // The refusal now asks the meter, which is what the accrual guard, `patch_rung_already_built`
    // and `herd_rung_already_built` already asked. **The pair is what these tests pin**: an eroded
    // meter accepts, a FULL meter still refuses with the message that exists for it.

    /// **Where a rung's span ends**, read off the shipped ladder so a retune moves the fixture with
    /// the game rather than leaving a literal behind. The retention bar that used to ride beside it
    /// is deleted (`docs/plan_standing_upkeep.md` §2.8) — a rung is achieved exactly here.
    fn rung_top(app: &bevy::prelude::App, key: RungKey) -> f32 {
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        let (base, width) = core_sim::plant_rung_span(key, &ladder);
        base + width
    }

    /// **The fraction of its span an ERODED position is left standing at** — a hair below the top,
    /// so the only thing distinguishing it from a finished rung is that one bleed.
    const ERODED_BUT_STILL_HELD: f32 = 0.99;

    /// Put `coord`'s patch in the state a finished Cultivate decays into: **below the top of the
    /// tended rung**, and therefore no longer tended.
    ///
    /// **The old fixture claimed a third state — tended AND building — and it no longer exists.**
    /// That gap was the retention bar's, and deleting the bar collapses *achieved* and *its meter is
    /// full* back into one fact. What the §4.7 fix bought survives: `cultivate` is accepted here,
    /// because the position is below the rung's top and there is genuinely work to do.
    fn erode_a_tended_patch(app: &mut bevy::prelude::App, coord: UVec2, faction: FactionId) {
        let top = rung_top(app, RungKey::PlantTended);
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry
            .patch_mut(coord)
            .expect("the fixture patch is there");
        patch.set_ladder_position(top * ERODED_BUT_STILL_HELD, &ladder);
        patch.owner = Some(faction);
        assert!(
            !patch.is_cultivated(),
            "the fixture must be BELOW the rung's top, or the command has nothing to accept"
        );
    }

    /// A patch worn below its cost is ground the builders may legitimately repair, so `cultivate`
    /// takes it — and a patch whose meter is genuinely full is still refused, which is the case the
    /// *"already cultivated"* message exists for.
    #[test]
    fn an_eroded_tended_patch_is_re_tendable_and_a_full_one_is_not() {
        let mut app = build_test_app();
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
                take_species: TakeSelection::EVERYTHING,
            },
        );
        erode_a_tended_patch(&mut app, coord, faction);

        handle_cultivate(&mut app, faction, coord);

        assert!(
            !cultivate_failure_detail_contains(&app, "already cultivated"),
            "a tended patch eroded below its cost is a repair, not a finished job"
        );
        assert_eq!(
            band_improvement(&app, band),
            Some(Improvement::Cultivate),
            "and re-queueing it is what puts the builders back on it — the entry IS the declaration"
        );
        // **The old pair here — "and the ground stays TENDED throughout" — is GONE with the
        // retention bar** (`docs/plan_standing_upkeep.md` §2.8). A position below the rung's top is
        // not that rung, so a patch being repaired is honestly untended while the repair runs. What
        // made that a *cliff* is gone too: the payout and the keeping both interpolate, so a patch a
        // hair below the top is worth a hair under a whole tended patch rather than dropping to a
        // wild stand's rate.
        assert!(
            !app.world
                .resource::<ForageRegistry>()
                .patch(coord)
                .unwrap()
                .is_cultivated(),
            "a patch below the rung's top is not that rung — which is what makes it repairable"
        );

        // The other half, in its own world so the first half's (absent) failure cannot be read for
        // this one's.
        let mut full = build_test_app();
        seed_thriving_patch(&mut full, coord);
        grant_cultivation(&mut full, faction);
        spawn_working_band(
            &mut full,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
                take_species: TakeSelection::EVERYTHING,
            },
        );
        {
            let top = rung_top(&full, RungKey::PlantTended);
            let ladder = full.world.resource::<LadderConfigHandle>().get();
            let mut registry = full.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(coord).unwrap();
            patch.set_ladder_position(top, &ladder);
            patch.owner = Some(faction);
        }

        handle_cultivate(&mut full, faction, coord);

        assert!(
            cultivate_failure_detail_contains(&full, "already cultivated"),
            "a FULL meter has nothing left to build and must still be refused"
        );
    }

    /// The rung-3 twin, on the same pair: a Field worn below its cost is re-sowable, a full one is
    /// not.
    #[test]
    fn an_eroded_field_is_re_sowable_and_a_full_one_is_not() {
        let mut app = build_world_app();
        let faction = FactionId(0);
        let coord = find_sowable_tile(&app);
        seed_thriving_patch(&mut app, coord);
        grant_seed_selection(&mut app, faction);
        let band = spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
                take_species: TakeSelection::EVERYTHING,
            },
        );
        let top = rung_top(&app, RungKey::PlantField);
        {
            let ladder = app.world.resource::<LadderConfigHandle>().get();
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(coord).unwrap();
            patch.set_ladder_position(top * ERODED_BUT_STILL_HELD, &ladder);
            patch.owner = Some(faction);
            assert!(
                !patch.is_field(),
                "below the Field's top is no longer a Field"
            );
        }

        handle_sow(&mut app, faction, coord);

        assert!(
            !sow_failure_detail_contains(&app, "already sown"),
            "a Field eroded below its cost is a repair"
        );
        assert_eq!(band_improvement(&app, band), Some(Improvement::Sow));

        let mut full = build_world_app();
        let coord = find_sowable_tile(&full);
        seed_thriving_patch(&mut full, coord);
        grant_seed_selection(&mut full, faction);
        spawn_working_band(
            &mut full,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
                take_species: TakeSelection::EVERYTHING,
            },
        );
        {
            let mut registry = full.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(coord).unwrap();
            patch.complete_field(faction, &core_sim::LadderConfig::builtin());
            patch.owner = Some(faction);
        }

        handle_sow(&mut full, faction, coord);

        assert!(
            sow_failure_detail_contains(&full, "already sown"),
            "a FULL Field meter is still refused"
        );
    }

    /// **The animal web's half of the same sweep.** `tame`'s gate always read the meter
    /// (`is_domesticated()` *is* `progress >= cost`), and `corral`'s read the **fence flag**; the
    /// pen's meter never bleeds today, so the two answer alike — this pins that they do, and that
    /// the refusal survives, so the shape can be made uniform without moving play.
    #[test]
    fn a_full_pen_and_a_tamed_herd_are_both_still_refused() {
        let mut app = build_test_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let id = seed_herd(&mut app, coord, Some(faction));
        grant_penning(&mut app, faction);
        // Both rungs' knowledge, because both verbs are asked below and a missing gate would refuse
        // for the wrong reason.
        grant_herding(&mut app, faction);
        spawn_working_band(
            &mut app,
            faction,
            LaborTarget::Hunt {
                fauna_id: id.clone(),
                floor: 0.5,
            },
        );
        {
            let mut herds = app.world.resource_mut::<HerdRegistry>();
            let herd = herds
                .herds
                .iter_mut()
                .find(|herd| herd.id == id)
                .expect("the fixture herd is there");
            assert!(
                herd.corral_at(coord, &core_sim::LadderConfig::builtin()),
                "the fixture herd can be penned"
            );
            assert!(
                herd.is_corralled() && herd.corral_meter_full(),
                "the fence flag and the meter agree on every herd the sim can reach"
            );
        }

        handle_corral(&mut app, faction, coord);
        assert!(corral_failure_detail_contains(&app, "already corralled"));

        handle_tame(&mut app, faction, id);
        assert!(tame_failure_detail_contains(&app, "already domesticated"));
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
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry
            .patch_mut(coord)
            .expect("the fixture seeded a patch");
        patch.set_ladder_position(PART_PREPARED_WORK, &ladder);
        patch.owner = owner;
        patch.ecology_phase = EcologyPhase::Stressed;
    }

    /// **A paused build's banked work** — half of a nominal one-worker job, so a position at it
    /// reads unambiguously as mid-build without pretending to the ladder's shipped price, which
    /// these command-gate tests are not about.
    ///
    /// **The PLANT web needs no `PART_PREPARED_JOB` beside it any more** — its rung boundaries come
    /// from live config, so a plant fixture states only where the source stands and whether that is
    /// mid-rung is the *ladder's* answer. [`PART_PREPARED_JOB`] survives for the **animal** web,
    /// whose two meters still carry their own stamped costs.
    const PART_PREPARED_WORK: f32 = FABRICATED_BUILD_COST / 2.0;
    /// **The re-crew case.** A build this faction has underway on a patch that has dropped out of
    /// Thriving still accepts a `Cultivate` assignment — which is what lets the player *ease workers
    /// off* and let the patch regrow. Doubly true since `docs/plan_harvest_floor.md` §3.2: easing
    /// off is now also how you *speed the build up*, because a shallower draw is a faster meter.
    #[test]
    fn a_paused_cultivation_can_still_be_re_crewed() {
        let mut app = build_test_app();
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
                take_species: TakeSelection::EVERYTHING,
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
                take_species: TakeSelection::EVERYTHING,
            },
        );
        {
            let mut allocation = app
                .world
                .get_mut::<LaborAllocation>(band)
                .expect("the band works the patch");
            let source = BuildSource::of(&allocation.assignments[0].target)
                .expect("the fixture row names a source");
            assert!(allocation.enqueue_build(source, BuildJob::Rung(Improvement::Cultivate)));
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
            None,
            Vec::new(),
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
            take_species: TakeSelection::EVERYTHING,
        };

        let mut thriving = build_test_app();
        seed_thriving_patch(&mut thriving, coord);
        {
            let ladder = thriving.world.resource::<LadderConfigHandle>().get();
            let mut registry = thriving.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(coord).unwrap();
            patch.set_ladder_position(PART_PREPARED_WORK, &ladder);
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
        let mut stressed = build_test_app();
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
        let mut app = build_test_app();
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
                take_species: TakeSelection::EVERYTHING,
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

    /// **A TAKE SELECTION IS JUDGED BY THE COMMAND THAT CARRIES IT** (the selective gather) — and
    /// the two ways it can be wrong get two different answers.
    ///
    /// `assign_labor` is the only command that can set `LaborTarget::Forage::take_species`. An
    /// **unknown** key fails closed there for the reason the floor does: nothing can be inferred from
    /// a typo, and a silently dropped one produces exactly the numbers *"take everything"* produces —
    /// the same take, the same crew count, the same row — so the mistake would be undiagnosable. A
    /// plant that merely **does not stand here** is pruned instead; that half is
    /// `a_take_selection_the_ground_no_longer_offers_is_pruned_not_refused`.
    ///
    /// The refusal is asserted **through the command**, never through the validator it calls:
    /// `cultivation.md` records a guard that went on passing while no command path validated
    /// anything, because it fed the validator an input no command could supply.
    ///
    /// The control comes first, so a rejection cannot be the verb being broken outright, and the
    /// legal case is a plant the tile really grows — the selective gather has **no rung gate**, so a
    /// `wild`-ceiling plant nobody can ever commit to is a perfectly good thing to carry home.
    #[test]
    fn assign_labor_rejects_a_take_selection_naming_a_plant_that_does_not_exist() {
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);

        // The control: naming nothing is the whole basket, which cannot be wrong.
        let mut app = forage_ground_with_baskets(faction, coord);
        assign_forage_take(&mut app, faction, coord, &[]);
        assert!(
            !forage_failure_detail_contains(&app, "know no plant"),
            "control: naming no plants is the whole basket, which is always legal"
        );

        // …and so is a plant that actually grows here, whatever rung it can climb.
        let mut app = forage_ground_with_baskets(faction, coord);
        let growing = a_species_growing_at(&app, coord);
        assign_forage_take(&mut app, faction, coord, &[growing.as_str()]);
        assert!(
            !forage_failure_detail_contains(&app, "know no plant"),
            "control: a plant this tile's basket carries is a legal thing to gather"
        );

        // 1. A plant that does not exist at all.
        let mut app = forage_ground_with_baskets(faction, coord);
        assign_forage_take(&mut app, faction, coord, &["not_a_plant"]);
        assert!(
            forage_failure_detail_contains(&app, "know no plant"),
            "an unknown plant is refused where it is named, never quietly dropped"
        );

        // 2. …and one unknown key spoils the whole selection, rather than being quietly filtered out
        //    of it. A typo is not a narrowing, so half a selection is a different order than the one
        //    the player gave — and the refusal leaves the standing selection exactly as it was.
        let mut app = forage_ground_with_baskets(faction, coord);
        let growing = a_species_growing_at(&app, coord);
        assign_forage_take(&mut app, faction, coord, &[growing.as_str()]);
        assert_eq!(
            band_take_selection(&app, coord),
            vec![growing.clone()],
            "fixture: the band is standing on a narrowed selection, or nothing is being preserved"
        );
        assign_forage_take(&mut app, faction, coord, &[growing.as_str(), "not_a_plant"]);
        assert!(
            forage_failure_detail_contains(&app, "know no plant"),
            "one unknown plant refuses the whole selection; it is not silently narrowed"
        );
        assert_eq!(
            band_take_selection(&app, coord),
            vec![growing],
            "and a refused command changes nothing — not even the half of it that was legal"
        );
    }

    /// **A PLANT THE GROUND NO LONGER OFFERS IS PRUNED OUT OF THE SELECTION, NOT REFUSED WITH IT** —
    /// the reported defect, end to end.
    ///
    /// Judging the take against the patch's own rung-reweighted mix is right (see
    /// `assign_labor_judges_a_take_selection_against_the_patchs_own_mix`). **Hard-refusing on it was
    /// not.** A `Sow` reweights the ground out from under a selection the player made before it, so
    /// the stored names go stale through the player's own investment — and the refusal then rejected
    /// the *whole* `assign_labor`, worker count included. Reported from play at T120 on a Field
    /// standing at `Wild Emmer 100%` whose row still named Wild Pulses: raising the tenders did
    /// nothing at all, turn after turn, and the only thing said was *"Harvest failed — Wild Pulses
    /// does not grow at (13, 10)"*. The panel offered no way out either, because a chip is drawn only
    /// for a plant the **current** mix carries.
    ///
    /// So the crew count is what this asserts, **off the published wire row** rather than the
    /// in-process allocation: what the player is looking at when they say it did not take is the
    /// snapshot, and an assertion on the component would pass on a frame that never shipped.
    #[test]
    fn a_take_selection_the_ground_no_longer_offers_is_pruned_not_refused() {
        let faction = FactionId(0);
        // **A REAL world, because the assertion is on the WIRE** — the baskets fixture never
        // resolves a turn, so it publishes no frame at all.
        let (mut app, coord) = sowable_ground_with_a_resident_band(faction);
        let (crop, displaced) = a_crop_and_what_it_displaces(&app, coord);

        // The band is gathering the plant the Sow is about to weed out — a selection that was
        // perfectly legal when it was made.
        handle_assign_labor(
            &mut app,
            faction,
            Some(FIXTURE_BAND_ID),
            "forage".to_string(),
            BAND_WORKERS,
            Some(coord.x),
            Some(coord.y),
            None,
            None,
            None,
            None,
            vec![displaced.clone()],
        );
        assert_eq!(
            band_take_selection(&app, coord),
            vec![displaced.clone()],
            "fixture: the stale selection has to be standing before the ground moves under it"
        );

        // …and now the ground is a Field of `crop`, which `planted` weeded `displaced` out of.
        sow_the_patch_to(&mut app, faction, coord, &crop);

        // The player raises the crew. The selection is stale and the command must still land.
        let raised = BAND_WORKERS + 1;
        handle_assign_labor(
            &mut app,
            faction,
            Some(FIXTURE_BAND_ID),
            "forage".to_string(),
            raised,
            Some(coord.x),
            Some(coord.y),
            None,
            None,
            None,
            None,
            vec![displaced.clone()],
        );

        assert!(
            !forage_failure_detail_contains(&app, "does not grow at"),
            "a plant the player's own Sow displaced must not refuse the command that names it"
        );
        assert_eq!(
            band_take_selection(&app, coord),
            Vec::<String>::new(),
            "nothing survived the prune, so the crew is back on the whole basket"
        );
        assert!(
            forage_event_detail_contains(&app, "status=pruned"),
            "a selection the sim narrowed on the player's behalf is said once in the feed"
        );
        // **AND IT NAMES THE BAND**, because the event dock offers a *"Work tab"* jump on this row
        // and the `band=` token is its only channel. Asserted as the whole token, ahead of
        // `dropped=`: a bare `contains("90001")` would also pass on a tile coordinate, and a token
        // appended *after* the comma-joined display names would be swallowed by that trailing value.
        assert!(
            forage_event_detail_contains(&app, &format!("band={FIXTURE_BAND_ID} dropped=")),
            "the narrowed row names the band whose work board it is, by its durable BandId"
        );

        // …and the crew the player asked for is still there on the frame they are looking at.
        app.update();
        assert_eq!(
            published_forage_workers(&app, coord),
            Some(raised),
            "the crew the player asked for must reach the wire — this is the reported defect"
        );
    }

    /// **Sowable ground in a REAL world, with a resident band standing on it and gathering it** —
    /// the fixture for anything that has to read the published frame, which `build_world_app` is the
    /// only way to get (the baskets fixture runs no turn, so it publishes nothing).
    ///
    /// The band is planted on the tile's own entity because `spawn_working_band` gives it a bare
    /// `spawn_empty` home: the labor pass reads `current_tile` for the in-range test, and a band that
    /// is nowhere lapses its row off the wire before any assertion can see it.
    fn sowable_ground_with_a_resident_band(faction: FactionId) -> (bevy::prelude::App, UVec2) {
        let mut app = build_world_app();
        let coord = find_sowable_tile(&app);
        seed_thriving_patch(&mut app, coord);
        let band = spawn_resident_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: DEFAULT_ESCAPEMENT_FLOOR,
                species: None,
                take_species: TakeSelection::EVERYTHING,
            },
        );
        // **ADDRESSABLE, because a real world already has a resident band of this faction** and an
        // unaddressed `assign_labor` picks whichever one the query hands back first.
        app.world.entity_mut(band).insert(BandId(FIXTURE_BAND_ID));
        let ground = app
            .world
            .resource::<TileRegistry>()
            .index(coord.x, coord.y)
            .expect("the pinned map carries this tile");
        let mut cohort = app
            .world
            .get_mut::<PopulationCohort>(band)
            .expect("the fixture band has a cohort");
        cohort.home = ground;
        cohort.current_tile = ground;
        (app, coord)
    }

    /// **A PARTIALLY STALE SELECTION KEEPS WHAT STILL STANDS** — the prune narrows, it does not
    /// reset. Blanket-resetting to the whole basket would start carrying home the very plants the
    /// player had deliberately unticked, which is overriding a stated preference in the other
    /// direction — the same rule `TakeSelection::pruned_for_commitment` already holds on the turn's
    /// side of the repair.
    #[test]
    fn a_partially_stale_take_selection_keeps_the_names_that_still_stand() {
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let mut app = forage_ground_with_baskets(faction, coord);
        let (crop, displaced) = a_crop_and_what_it_displaces(&app, coord);
        sow_the_patch_to(&mut app, faction, coord, &crop);

        // Half of this selection is what the ground is made of; half is what it weeded out.
        handle_assign_labor(
            &mut app,
            faction,
            None,
            "forage".to_string(),
            BAND_WORKERS,
            Some(coord.x),
            Some(coord.y),
            None,
            None,
            None,
            None,
            vec![crop.clone(), displaced],
        );

        assert_eq!(
            band_take_selection(&app, coord),
            vec![crop],
            "the surviving name is kept; only the one the ground no longer grows is dropped"
        );
    }

    /// The **crop** a Sow would commit this ground to, and the clearable plant that Sow displaces —
    /// resolved through the same `tile_flora_composition` seam the command judges with, so neither
    /// can go stale against the shipped roster.
    fn a_crop_and_what_it_displaces(app: &bevy::prelude::App, coord: UVec2) -> (String, String) {
        let displaced = {
            let labor = app.world.resource::<LaborConfigHandle>().get();
            let flora = app.world.resource::<FloraConfigHandle>().get();
            let map_seed = app.world.resource::<SimulationConfig>().map_seed;
            let ground = app
                .world
                .resource::<TileRegistry>()
                .index(coord.x, coord.y)
                .and_then(|entity| app.world.get::<Tile>(entity))
                .expect("the fixture seeded this tile");
            let composition = tile_flora_composition(&flora, &labor.forage, ground, map_seed);
            // The least abundant clearable member — the first thing a Sow displaces, and still a
            // legal selection against the *wild* basket, which is the whole point.
            composition
                .iter()
                .filter(|entry| flora.stands_in_worked_ground(&entry.species))
                .min_by(|a, b| a.share.total_cmp(&b.share))
                .expect("the fixture tile grows something clearable")
                .species
                .clone()
        };
        let crop = a_species_growing_at(app, coord);
        assert_ne!(
            crop, displaced,
            "fixture: the crop must displace a different plant, or nothing is being judged"
        );
        (crop, displaced)
    }

    /// Put a finished Field of `crop` under `coord`. `planted` takes the whole basket less whatever
    /// stands outside the worked ground, so every clearable plant but the crop stops standing here.
    fn sow_the_patch_to(
        app: &mut bevy::prelude::App,
        faction: FactionId,
        coord: UVec2,
        crop: &str,
    ) {
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry
            .patch_mut(coord)
            .expect("the fixture seeded a patch here");
        patch.species = Some(crop.to_string());
        patch.complete_field(faction, &ladder);
    }

    /// The band's standing take selection on `coord`, off the labor allocation — empty is the whole
    /// basket.
    fn band_take_selection(app: &bevy::prelude::App, coord: UVec2) -> Vec<String> {
        app.world
            .iter_entities()
            .filter_map(|entity| entity.get::<LaborAllocation>())
            .flat_map(|allocation| allocation.assignments.iter())
            .find_map(|assignment| match &assignment.target {
                LaborTarget::Forage {
                    tile, take_species, ..
                } if *tile == coord => {
                    Some(take_species.keys().map(str::to_string).collect::<Vec<_>>())
                }
                _ => None,
            })
            .unwrap_or_default()
    }

    /// **The crew on `coord` AS PUBLISHED** — the wire row, which is what the player is actually
    /// looking at, rather than the allocation the capture reads.
    fn published_forage_workers(app: &bevy::prelude::App, coord: UVec2) -> Option<u32> {
        app.world
            .resource::<SnapshotHistory>()
            .last_snapshot()?
            .populations
            .iter()
            .flat_map(|cohort| cohort.labor_assignments.iter())
            .find(|row| {
                // The wire's `kind` is `LaborTarget::kind()` verbatim, so the row is matched
                // through that one spelling rather than a literal repeated here.
                row.kind
                    == LaborTarget::Forage {
                        tile: coord,
                        floor: DEFAULT_ESCAPEMENT_FLOOR,
                        species: None,
                        take_species: TakeSelection::EVERYTHING,
                    }
                    .kind()
                    && row.target_x == coord.x
                    && row.target_y == coord.y
            })
            .map(|row| row.workers)
    }

    /// Did any `Forage` event carry `needle` in its detail? The success twin of
    /// [`forage_failure_detail_contains`], for the lines a *landed* command pushes.
    fn forage_event_detail_contains(app: &bevy::prelude::App, needle: &str) -> bool {
        app.world.resource::<CommandEventLog>().iter().any(|entry| {
            matches!(entry.kind, CommandEventKind::Forage)
                && entry
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains(needle))
        })
    }

    /// **A TAKE SELECTION IS JUDGED AGAINST THE MIX THE TAKE WILL NARROW**, which on a sown patch is
    /// not the tile's wild realization.
    ///
    /// The command used to judge `tile_flora_composition` while the take path narrows against
    /// `forage::patch_composition` — the rung-reweighted mix. On any tended or sown patch those
    /// differ, so the boundary **stored** a selection the very next turn's take valued at exactly
    /// zero: a `+0.00 /turn` row with nothing said.
    ///
    /// **What the mix decides is what is PRUNED, not what is refused** — the refusal that first
    /// carried this reading broke every crew edit on a committed Field
    /// (`a_take_selection_the_ground_no_longer_offers_is_pruned_not_refused`). So the assertion is on
    /// the selection the row ends up carrying, which is the thing the zero take was ever about.
    ///
    /// The control is asserted from the same fixture: the crop the ground was sown to is still a
    /// perfectly good thing to gather, so the prune cannot be the sown patch dropping everything.
    #[test]
    fn assign_labor_judges_a_take_selection_against_the_patchs_own_mix() {
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);

        let mut app = forage_ground_with_baskets(faction, coord);
        let (crop, displaced) = a_crop_and_what_it_displaces(&app, coord);

        // The wild basket still carries `displaced`, so judging THAT would keep it.
        assign_forage_take(&mut app, faction, coord, &[displaced.as_str()]);
        assert_eq!(
            band_take_selection(&app, coord),
            vec![displaced.clone()],
            "control: on wild ground the plant is standing, so the selection is stored verbatim"
        );

        // Sow the ground to `crop`: `planted` takes the whole basket less whatever stands outside
        // the worked ground, so `displaced` is no longer standing here.
        let mut app = forage_ground_with_baskets(faction, coord);
        sow_the_patch_to(&mut app, faction, coord, &crop);

        assign_forage_take(&mut app, faction, coord, &[displaced.as_str()]);
        assert_eq!(
            band_take_selection(&app, coord),
            Vec::<String>::new(),
            "a plant the Sow displaced is pruned out — the take would value it at zero"
        );

        let mut app = forage_ground_with_baskets(faction, coord);
        sow_the_patch_to(&mut app, faction, coord, &crop);
        assign_forage_take(&mut app, faction, coord, &[crop.as_str()]);
        assert_eq!(
            band_take_selection(&app, coord),
            vec![crop],
            "control: the sown crop is what the patch is made of, so gathering it survives"
        );
    }

    /// Re-issue the band's forage assignment naming a take selection — the real `assign_labor`
    /// command path, which is the only one that can carry one.
    fn assign_forage_take(
        app: &mut bevy::prelude::App,
        faction: FactionId,
        coord: UVec2,
        take: &[&str],
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
            None,
            None,
            None,
            take.iter().map(|key| (*key).to_string()).collect(),
        );
    }

    /// A plant the tile at `coord` really grows, through the one `tile_flora_composition` seam the
    /// command judges with — the legal input the refusals above are measured against.
    fn a_species_growing_at(app: &bevy::prelude::App, coord: UVec2) -> String {
        let labor = app.world.resource::<LaborConfigHandle>().get();
        let flora = app.world.resource::<FloraConfigHandle>().get();
        let map_seed = app.world.resource::<SimulationConfig>().map_seed;
        let ground = app
            .world
            .resource::<TileRegistry>()
            .index(coord.x, coord.y)
            .and_then(|entity| app.world.get::<Tile>(entity))
            .expect("the fixture seeded this tile");
        tile_flora_composition(&flora, &labor.forage, ground, map_seed)
            .first()
            .expect("the fixture tile grows something")
            .species
            .clone()
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
        let mut app = build_test_app();
        seed_grid_with_baskets(&mut app, PHASE_GATE_GRID);
        seed_thriving_patch(&mut app, coord);
        spawn_resident_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: coord,
                floor: 0.5,
                species: None,
                take_species: TakeSelection::EVERYTHING,
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
            None,
            Vec::new(),
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
    const SOW_TEST_MAP_SEED: u64 = core_sim::HARNESS_MAP_SEED;

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
        let mut app = build_test_app();
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
                pending_contacts: Default::default(),
                kit: core_sim::EquipmentConfig::builtin().default_kit(KitJob::Hunt),
                cargo: LocalStore::new(),
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

    /// Launch a hunting party of `PARTY_ON_A_RECALLED_RAID` off the world's first resident band at
    /// the first live herd, returning `(band, party, the band's working count before the launch)` —
    /// the shared opening of both recall fixtures below, so the "cancelled in camp" and "walked home"
    /// cases cannot diverge in how the party was raised.
    fn launch_a_hunting_party(
        app: &mut bevy::prelude::App,
        faction: FactionId,
    ) -> (Entity, Entity, Scalar) {
        let band = {
            let mut query = app
                .world
                .query_filtered::<Entity, (With<PopulationCohort>, With<ResidentBand>)>();
            query
                .iter(&app.world)
                .next()
                .expect("worldgen spawned a resident band")
        };
        let working_before = app
            .world
            .get::<PopulationCohort>(band)
            .expect("the band has a cohort")
            .working;
        let herd_id = app
            .world
            .resource::<HerdRegistry>()
            .herds
            .first()
            .expect("worldgen seeded a herd")
            .id
            .clone();
        handle_send_hunt_expedition(
            app,
            faction,
            None,
            PARTY_ON_A_RECALLED_RAID,
            herd_id,
            None,
            None,
        );
        let party = {
            let mut query = app.world.query_filtered::<Entity, With<Expedition>>();
            query
                .iter(&app.world)
                .next()
                .expect("the launch spawned a detached party")
        };
        (band, party, working_before)
    }

    /// A party small enough for any starting band to outfit, and large enough that its workers going
    /// missing is visible in the band's `working` count.
    const PARTY_ON_A_RECALLED_RAID: u32 = 2;

    /// **Cancelling a party that has not left is a CANCEL, not a round trip.**
    ///
    /// The playtest report: launch a hunting expedition, press the party row's ✕ before advancing a
    /// single turn, confirm — and nothing observable happens, because the recall only set
    /// `Returning` and the fold-back waited on the next turn's `advance_expeditions`. A party
    /// standing in its home band's own camp has gone nowhere, so the order that cancels it has
    /// nothing to wait for.
    ///
    /// Asserted **without advancing a turn**, which is the whole claim: the party is gone and its
    /// workers are back in the band the instant the command lands.
    #[test]
    fn a_party_recalled_in_camp_folds_back_without_waiting_a_turn() {
        let mut app = build_world_app();
        app.world
            .insert_resource(CommandSenderResource(unbounded::<Command>().0));
        let faction = FactionId(0);
        let (band, party, working_before) = launch_a_hunting_party(&mut app, faction);
        let party_band_id = *app
            .world
            .get::<BandId>(party)
            .expect("a detached party is a band and carries a BandId");

        handle_recall_expedition(&mut app, faction, party_band_id.0);

        assert!(
            !app.world.entities().contains(party),
            "a party recalled where it stands must fold back at once, not next turn"
        );
        assert_eq!(
            app.world
                .get::<PopulationCohort>(band)
                .expect("the band survives")
                .working,
            working_before,
            "the cancelled party's workers must be exactly the ones drawn off at launch"
        );
        assert!(
            app.world
                .resource::<CommandEventLog>()
                .iter()
                .any(|entry| entry.kind == CommandEventKind::ExpeditionReturned),
            "the fold-back publishes the same ExpeditionReturned line a party walking home does"
        );
    }

    /// **A party recalled in the FIELD still walks home and folds back** — the cancel above must not
    /// have become the only way a recall ever completes. Its workers rejoin the band on arrival, so
    /// the two paths differ in when they settle and in nothing else.
    #[test]
    fn a_party_recalled_in_the_field_walks_home_and_folds_back() {
        const TILES_FROM_CAMP: u32 = 5;
        const TURNS_TO_WALK_HOME: u32 = 20;

        let mut app = build_world_app();
        app.world
            .insert_resource(CommandSenderResource(unbounded::<Command>().0));
        let faction = FactionId(0);
        let (band, party, working_before) = launch_a_hunting_party(&mut app, faction);

        // Put the party out on the map, past the comm range its fold-back needs.
        let camp = app
            .world
            .get::<PopulationCohort>(party)
            .expect("the party has a cohort")
            .current_tile;
        let camp = app
            .world
            .get::<Tile>(camp)
            .expect("camp is a tile")
            .position;
        let out_there = app
            .world
            .resource::<TileRegistry>()
            .index(camp.x + TILES_FROM_CAMP, camp.y)
            .expect("a tile that many steps east of camp");
        app.world
            .get_mut::<PopulationCohort>(party)
            .expect("the party has a cohort")
            .current_tile = out_there;

        let party_band_id = *app.world.get::<BandId>(party).expect("party BandId");
        handle_recall_expedition(&mut app, faction, party_band_id.0);
        assert!(
            app.world.entities().contains(party),
            "a party out in the field has a walk home to make first"
        );

        for _ in 0..TURNS_TO_WALK_HOME {
            resolve_turn_with_auto_orders(&mut app);
            if !app.world.entities().contains(party) {
                break;
            }
        }
        assert!(
            !app.world.entities().contains(party),
            "a recalled party must reach its band and fold back"
        );
        assert!(
            app.world
                .get::<PopulationCohort>(band)
                .expect("the band survives")
                .working
                >= working_before,
            "the returned party's workers rejoin the band's pool"
        );
    }

    /// **The party's `band_id` AS THE CLIENT READS IT** — decoded off an encoded frame, the way the
    /// Godot client decodes it (envelope → snapshot payload → population section), never read off
    /// the `BandId` component.
    ///
    /// The whole failure mode a recall round trip can have is the published id and the resolvable id
    /// disagreeing, so a fixture that read the component would assert the two halves agree by
    /// construction and prove nothing.
    ///
    /// **It publishes through `publish_full_frame`, not `StoredSnapshot::encode_flat`.** `encode_flat`
    /// is a read of stored bytes, so its header carries the sequence number the entry was published
    /// under — stale the moment anything else publishes, and a recapture publishes on every
    /// world-mutating command. `publish_full_frame` is the seam a `Resync` answers through: it claims
    /// a live number and re-encodes, which is the frame a client would actually be handed here.
    fn published_party_band_id(app: &mut bevy::prelude::App) -> u64 {
        use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

        recapture_snapshot_in_place(&mut app.world);
        let entry = app
            .world
            .resource::<SnapshotHistory>()
            .latest_entry()
            .expect("a snapshot was captured");
        let bytes = app
            .world
            .resource_mut::<SnapshotHistory>()
            .publish_full_frame(&entry);
        let envelope =
            fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
        assert_eq!(
            envelope.payload_type(),
            fb::SnapshotPayload::snapshot,
            "a full frame carries a snapshot payload"
        );
        let parties: Vec<u64> = envelope
            .payload_as_snapshot()
            .expect("the envelope carries a snapshot")
            .population()
            .and_then(|section| section.populations())
            .expect("the snapshot carries a population section")
            .iter()
            .filter(|cohort| cohort.isExpedition())
            .map(|cohort| cohort.bandId())
            .collect();
        assert_eq!(
            parties.len(),
            1,
            "exactly one detached party is on the wire; this fixture launches one"
        );
        parties[0]
    }

    /// The population rows and removals of the **delta the recapture just broadcast** — the frame a
    /// live client actually receives after a world-mutating command (a full snapshot goes out only
    /// on a world's first publication). Rows as `(entity, band_id, is_expedition)`.
    fn recaptured_population_delta(
        app: &mut bevy::prelude::App,
    ) -> (Vec<(u64, u64, bool)>, Vec<u64>) {
        use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

        recapture_snapshot_in_place(&mut app.world);
        let bytes = app
            .world
            .resource::<SnapshotHistory>()
            .encoded_delta_flat()
            .expect("the recapture broadcast a delta");
        let envelope =
            fb::root_as_envelope(bytes.as_ref()).expect("the delta encodes to a valid envelope");
        assert_eq!(
            envelope.payload_type(),
            fb::SnapshotPayload::delta,
            "a recapture publishes a delta"
        );
        let section = envelope
            .payload_as_delta()
            .expect("the envelope carries a delta")
            .population()
            .expect("the delta carries a population section");
        let rows = section
            .populations()
            .map(|rows| {
                rows.iter()
                    .map(|row| (row.entity(), row.bandId(), row.isExpedition()))
                    .collect()
            })
            .unwrap_or_default();
        let removed = section
            .removedPopulations()
            .map(|ids| ids.iter().collect())
            .unwrap_or_default();
        (rows, removed)
    }

    /// **A party launched and cancelled inside ONE tick must be published as REMOVED.**
    ///
    /// This is the ghost the playtest found. A mid-tick recapture diffs with `Baseline::Hold`, so
    /// the launch frame publishes the party *without storing it*; the cancel frame then found the
    /// party in no baseline, had nothing to report as vanished, and every later turn's diff had
    /// nothing to report either — the baseline never learned the row existed. The client kept the
    /// party row on its Band panel indefinitely, its ✕ kept sending the `BandId` it still held, and
    /// the sim kept answering `Expedition N does not exist in the simulation`, turn after turn.
    ///
    /// Asserted on the **encoded delta**, because the whole defect is a row the sim knows is gone
    /// and the wire never says so — an in-process check of the world would pass throughout.
    #[test]
    fn a_party_cancelled_in_the_tick_it_launched_is_published_as_removed() {
        let mut app = build_world_app();
        app.world
            .insert_resource(CommandSenderResource(unbounded::<Command>().0));
        let faction = FactionId(0);
        // Resolve a turn first, so the baseline the recaptures below hold is a committed one.
        resolve_turn_with_auto_orders(&mut app);

        let (_band, party, _working_before) = launch_a_hunting_party(&mut app, faction);
        let party_entity = party.to_bits();
        let party_band_id = *app
            .world
            .get::<BandId>(party)
            .expect("a detached party is a band and carries a BandId");

        let (launched, _) = recaptured_population_delta(&mut app);
        assert!(
            launched
                .iter()
                .any(|(entity, _, is_expedition)| *entity == party_entity && *is_expedition),
            "the launch frame must put the party on the client's roster, or the removal below              proves nothing"
        );

        handle_recall_expedition(&mut app, faction, party_band_id.0);
        assert!(
            !app.world.entities().contains(party),
            "an in-camp recall folds the party back at once — that is what makes this one tick"
        );

        let (_, removed) = recaptured_population_delta(&mut app);
        assert!(
            removed.contains(&party_entity),
            "the frame that follows the cancel must tell the client the party is gone; without it              the row is a permanent ghost whose recall the sim can only refuse"
        );
    }

    /// **The id the snapshot publishes is the id the sim resolves** — the recall round trip, end to
    /// end, through the real handlers.
    ///
    /// A playtest reported the party row's ✕ doing nothing on turn after turn, with the feed saying
    /// `Expedition 2 does not exist in the simulation` — `resolve_expedition_entity`'s `no_such_band`
    /// arm. The client echoes back the `band_id` off the party's snapshot row, so the claim under
    /// test is a *round trip*: launch through `handle_send_hunt_expedition`, read the id off the
    /// **encoded** frame, and recall with exactly that value.
    ///
    /// Asserted across the states the report implicates — the id is read again after a turn has
    /// resolved and after the party has moved off camp, because "it failed on T2 and again on T3" is
    /// a claim about a party that has been alive for a while, not about the instant of launch.
    #[test]
    fn a_party_is_recalled_by_the_band_id_its_snapshot_row_published() {
        const TILES_FROM_CAMP: u32 = 5;

        let mut app = build_world_app();
        app.world
            .insert_resource(CommandSenderResource(unbounded::<Command>().0));
        let faction = FactionId(0);
        let (_band, party, _working_before) = launch_a_hunting_party(&mut app, faction);

        let at_launch = published_party_band_id(&mut app);
        assert_ne!(
            at_launch, 0,
            "a detached party publishes its own durable id, never the `unwrap_or_default` sentinel \
             that means it carries no `BandId` at all"
        );

        // Cross a turn boundary and put the party out in the field, so the recall is a walk home
        // rather than an in-camp cancel — the two states the report's repeated failures cover.
        resolve_turn_with_auto_orders(&mut app);
        let camp = app
            .world
            .get::<PopulationCohort>(party)
            .expect("the party has a cohort")
            .current_tile;
        let camp = app
            .world
            .get::<Tile>(camp)
            .expect("camp is a tile")
            .position;
        let out_there = app
            .world
            .resource::<TileRegistry>()
            .index(camp.x + TILES_FROM_CAMP, camp.y)
            .expect("a tile that many steps east of camp");
        app.world
            .get_mut::<PopulationCohort>(party)
            .expect("the party has a cohort")
            .current_tile = out_there;

        let published = published_party_band_id(&mut app);
        assert_eq!(
            published, at_launch,
            "a party's published id must be the same number a turn later — the client holds the one \
             it was last shown"
        );

        handle_recall_expedition(&mut app, faction, published);

        assert_eq!(
            app.world
                .get::<Expedition>(party)
                .expect("the party still exists")
                .phase,
            ExpeditionPhase::Returning,
            "the id the snapshot published must resolve to the party it names and turn it for home"
        );
    }

    /// **A denial party is addressable by its published id too** — the second spawn site allocates a
    /// `BandId` exactly as the hunt's does, and the recall verb makes no distinction between the
    /// three missions, so neither may the round trip.
    #[test]
    fn a_denial_party_is_recalled_by_the_band_id_its_snapshot_row_published() {
        let mut app = build_world_app();
        app.world
            .insert_resource(CommandSenderResource(unbounded::<Command>().0));
        let faction = FactionId(0);
        let herd_id = app
            .world
            .resource::<HerdRegistry>()
            .herds
            .first()
            .expect("worldgen seeded a herd")
            .id
            .clone();
        handle_send_denial_raid(
            &mut app,
            faction,
            None,
            PARTY_ON_A_RECALLED_RAID,
            herd_id,
            None,
        );
        let party = {
            let mut query = app.world.query_filtered::<Entity, With<Expedition>>();
            query
                .iter(&app.world)
                .next()
                .expect("the launch spawned a detached party")
        };

        let published = published_party_band_id(&mut app);
        assert_ne!(published, 0, "a denial party carries its own durable id");

        handle_recall_expedition(&mut app, faction, published);

        // The party never left camp, so the recall is the in-camp cancel — which is still the proof
        // the id resolved: the `no_such_band` arm despawns nothing and folds nothing back.
        assert!(
            !app.world.entities().contains(party),
            "a denial party recalled where it stands folds back at once, which it cannot do unless \
             its published id resolved"
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
                kit_id: None,
                take_species: Vec::new(),
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
            kit_id: None,
            take_species: Vec::new(),
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
        let mut app = build_test_app();
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
                take_species: TakeSelection::EVERYTHING,
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
                take_species: TakeSelection::EVERYTHING,
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
                take_species: TakeSelection::EVERYTHING,
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
                take_species: TakeSelection::EVERYTHING,
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
                take_species: TakeSelection::EVERYTHING,
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
            patch.complete_field(FactionId(0), &core_sim::LadderConfig::builtin());
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
                take_species: TakeSelection::EVERYTHING,
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
            let ladder = app.world.resource::<LadderConfigHandle>().get();
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(coord).unwrap();
            patch.set_ladder_position(PART_PREPARED_WORK, &ladder);
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
                take_species: TakeSelection::EVERYTHING,
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
                take_species: TakeSelection::EVERYTHING,
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
                    take_species: TakeSelection::EVERYTHING,
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
        /// How far up the Field's own span the fixture seats the position — the fraction the wire's
        /// `fieldProgress` must then read back.
        const HALF_SOWN: f32 = 0.5;
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
            // A Sow half-way up on top of it, so the row publishes a live rung-3 build fraction
            // beside a completed rung-2 one — which is the two-meter READOUT still riding one
            // position (`docs/plan_standing_upkeep.md` §2.8).
            let ladder = core_sim::LadderConfig::builtin();
            let (field_base, field_width) =
                core_sim::plant_rung_span(core_sim::RungKey::PlantField, &ladder);
            // Half-way up the Field's own span, on top of a completed tended rung.
            patch.owner = Some(FactionId(0));
            patch.set_ladder_position(field_base + field_width * HALF_SOWN, &ladder);
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

        // BOTH plant meters ship, independently — the two-meter split the client needs. **Each is
        // still a 0..1 FRACTION**, divided at capture against that patch's own job, even though the
        // meter behind it now stores absolute work units.
        assert!((patch.cultivation_progress - 1.0).abs() < 1e-6);
        assert!(patch.is_cultivated);
        assert!((patch.field_progress - HALF_SOWN).abs() < 1e-6);
        assert!(!patch.is_field, "a half-sown field is not a Field");

        // **And the WORK pair beside it** — the absolute meter and the job's live price, which is
        // what lets the UI say "18 of 50 work" and quote a rung before the player commits. The cost
        // is the LADDER's, not the patch's stamped one: an unstarted rung must still have a price.
        let cultivate_cost = core_sim::LadderConfig::builtin()
            .rung(RungKey::PlantTended)
            .build_cost(core_sim::RUNG_COST_UNSCALED)
            .expect("the tended rung builds");
        assert!((patch.cultivation_work_cost - cultivate_cost).abs() < 1e-6);
        assert!(
            (patch.field_work_done - patch.field_work_cost * HALF_SOWN).abs() < 1e-3,
            "the Field's work-done readout is the position clamped into the Field's own span"
        );
        assert!(
            patch.field_work_cost > 0.0,
            "a rung nobody has started still quotes its price"
        );
        assert_eq!(
            patch.build_turns_remaining,
            sim_schema::NO_BUILD_TURNS_ESTIMATE,
            "no crew is on this fixture patch, so there is no finish date to quote"
        );

        // Sow's pre-commit payoff, and the STANDING-UPKEEP trio beside it
        // (`docs/plan_standing_upkeep.md` §2). The `sow_build_fraction` this block used to compose a
        // "preparing" row from is retired with the dip: a crew building takes nothing, so there is
        // no factor left to publish and the client reads the zero from the model.
        assert!(patch.tended_yield > 0.0);
        let sustain_ceiling =
            (patch.biomass - core_sim::MSY_BIOMASS_FRACTION * patch.carrying_capacity).max(0.0)
                * patch.provisions_per_biomass;
        assert!(
            sustain_ceiling > 0.0,
            "the fixture patch must stand above the food peak, or its rows describe an empty patch"
        );
        // **The trio describes ONE rung and ONE turn.** This fixture patch carries a part-prepared
        // Sow, so the rung at risk is `plant:field` — and it owes the **same** maintenance rate a
        // finished Field owes, because the rate never lapses; only who supplies it moves
        // (`docs/plan_standing_upkeep.md` §2.4). Nobody is building it and nobody is keeping it, so
        // the whole of that is unmet and the shortfall is the bleed `advance_cultivation` will
        // apply.
        // **AND IT INTERPOLATES**, so a half-sown Field owes a whole tended patch plus half of what
        // a Field adds — not the Field's own rate, which is what it owed while the demand stepped at
        // the rung boundary (`docs/plan_standing_upkeep.md` §2.8). The cost moves with the benefit or
        // not at all.
        // **AND IT IS QUOTED PER TENDER-LOAD OF THIS GROUND**, because both plant rungs declare
        // `scaled_by: source_load`: the fixture sits on whatever tile `find_sowable_tile` picked, so
        // the load is resolved off that tile's own `K` rather than assumed to be the reference one.
        let ladder = core_sim::LadderConfig::builtin();
        let tender_loads = {
            let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
            let registry = app.world.resource::<core_sim::TileRegistry>();
            let tile_entity = registry
                .index(coord.x, coord.y)
                .expect("the fixture tile is on the map");
            let ground = app
                .world
                .get::<core_sim::Tile>(tile_entity)
                .expect("the fixture tile carries a Tile");
            core_sim::patch_tender_loads(
                core_sim::tile_forage_capacity(&labor.forage, ground),
                &labor.forage,
            )
        };
        assert!(
            tender_loads > 0.0,
            "fixture: sowable ground presents land to tend, or every upkeep figure below is zero"
        );
        let tended_demand = ladder
            .rung(RungKey::PlantTended)
            .upkeep_demand(tender_loads);
        let field_demand = ladder.rung(RungKey::PlantField).upkeep_demand(tender_loads);
        assert!(
            field_demand > tended_demand && tended_demand > 0.0,
            "the ladder's demands climb, or this block asserts nothing about interpolation"
        );
        let expected = tended_demand + HALF_SOWN * (field_demand - tended_demand);
        assert!(
            (patch.upkeep_demand - expected).abs() < 1e-3,
            "a half-sown Field owes {expected}, not {}",
            patch.upkeep_demand
        );
        assert_eq!(
            patch.upkeep_supplied, 0.0,
            "no band is keeping this fixture patch"
        );
        assert!(
            (patch.upkeep_shortfall - expected).abs() < 1e-3,
            "…so the whole demand went unmet — a row reading `0` short beside `0` supplied would \
             say nothing is wrong on a patch the sim is reverting"
        );
        // **And what it would take to stop that — the rate in whole hands, published on BOTH sides
        // of completion.** The keeping pool owes the rate for a meter carrying work at any fullness
        // (`docs/plan_standing_upkeep.md` §4.6a), so over a part-sown Field this number means what it
        // means over a finished one: the hands that hold the ground. It is **not** a minimum viable
        // build crew — a build crew supplies none of the rate.
        assert_eq!(
            patch.upkeep_workers_needed,
            expected.ceil() as u32,
            "hands to meet the rate the position actually owes, whoever is supplying it"
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
            herd.tame_outright(faction, &core_sim::LadderConfig::builtin());
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
        let mut app = build_test_app();
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
        let mut app = build_test_app();
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
        let mut app = build_test_app();
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
    /// costs `work_cost / the keeper crew's output` turns of the reduced Corral take.
    #[test]
    fn corral_sets_the_corral_policy_on_the_working_band() {
        let mut app = build_test_app();
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
        let mut app = build_test_app();
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
        let mut app = build_test_app();
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
        let mut app = build_test_app();
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
        let mut app = build_test_app();
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
        let mut app = build_test_app();
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
            .accrue_domestication(
                owner,
                PART_PREPARED_WORK,
                core_sim::RUNG_COST_UNSCALED,
                &core_sim::LadderConfig::builtin(),
            );
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
        let mut app = build_test_app();
        let faction = FactionId(0);
        let id = seed_herd(&mut app, UVec2::new(1, 1), None);
        grant_herding(&mut app, faction);

        handle_tame(&mut app, faction, id.clone());

        assert!(tame_failure_detail_contains(&app, "No band is hunting"));
    }

    /// An unknown herd id is rejected by name.
    #[test]
    fn tame_rejected_for_an_unknown_herd() {
        let mut app = build_test_app();
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
            herd.corral_at(coord, &core_sim::LadderConfig::builtin()),
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
        let mut app = build_test_app();
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
        let mut app = build_test_app();
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
        let mut app = build_test_app();
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
        let mut app = build_test_app();
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
        let mut app = build_test_app();
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
        let mut app = build_test_app();
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
        let mut app = build_test_app();
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
            let mut app = build_test_app();
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
        let mut app = build_test_app();
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
            let mut app = build_test_app();
            let faction = FactionId(0);
            let herd_id = seed_herd(&mut app, UVec2::new(1, 1), Some(faction));

            handle_send_hunt_expedition(&mut app, faction, None, 1, herd_id, Some(bad), None);

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

    /// **A kit the roster does not carry, or one the verb's job is not on, REFUSES the launch** —
    /// with a reason, and with no party spawned.
    ///
    /// The alternative — quietly sending the job's default — is the defect this whole arc exists to
    /// prevent, and it is worse than a typo: naming a kit is how the player *compares* tiers, so a
    /// silent substitution answers a different question than the one asked and looks exactly like an
    /// answer. Swept over both raiding verbs, because the outfit half is shared and the refusal is
    /// not.
    #[test]
    fn a_raiding_verb_refuses_an_unknown_or_wrong_job_kit_rather_than_defaulting() {
        for bad_kit in ["spear_of_destiny", "gathering"] {
            for verb in [RaidVerb::Hunt, RaidVerb::Deny] {
                let mut app = build_test_app();
                let faction = FactionId(0);
                let herd_id = seed_herd(&mut app, UVec2::new(1, 1), Some(faction));
                match verb {
                    RaidVerb::Hunt => handle_send_hunt_expedition(
                        &mut app,
                        faction,
                        None,
                        1,
                        herd_id,
                        None,
                        Some(bad_kit.to_string()),
                    ),
                    RaidVerb::Deny => handle_send_denial_raid(
                        &mut app,
                        faction,
                        None,
                        1,
                        herd_id,
                        Some(bad_kit.to_string()),
                    ),
                }

                let rejected = app.world.resource::<CommandEventLog>().iter().any(|entry| {
                    matches!(entry.kind, CommandEventKind::ExpeditionSent)
                        && entry.detail.as_deref().is_some_and(|detail| {
                            detail.contains("unknown kit") || detail.contains("cannot be sent on")
                        })
                });
                assert!(
                    rejected,
                    "{verb:?} with kit '{bad_kit}' must be refused with a reason naming the problem"
                );
                let parties = app
                    .world
                    .query::<&Expedition>()
                    .iter(&app.world)
                    .peekable()
                    .peek()
                    .is_some();
                assert!(
                    !parties,
                    "{verb:?} with kit '{bad_kit}': no party may leave"
                );
            }
        }
    }

    /// The launch half of the same rule for `assign_labor`, whose refusal path is its own.
    #[test]
    fn assign_labor_refuses_an_unknown_or_wrong_job_kit_rather_than_defaulting() {
        for (role, bad_kit) in [("hunt", "gathering"), ("forage", "big_game")] {
            let mut app = build_test_app();
            let faction = FactionId(0);
            let herd_id = seed_herd(&mut app, UVec2::new(1, 1), Some(faction));
            handle_assign_labor(
                &mut app,
                faction,
                None,
                role.to_string(),
                1,
                Some(1),
                Some(1),
                Some(herd_id),
                None,
                None,
                Some(bad_kit.to_string()),
                Vec::new(),
            );
            let rejected = app.world.resource::<CommandEventLog>().iter().any(|entry| {
                entry
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains("cannot be sent on"))
            });
            assert!(
                rejected,
                "a {role} crew named the {bad_kit} kit — the command must fail with a reason"
            );
            let staffed = app
                .world
                .query::<&LaborAllocation>()
                .iter(&app.world)
                .any(|allocation| !allocation.assignments.is_empty());
            assert!(!staffed, "a refused {role} assignment staffs nobody");
        }
    }

    /// **A crew can always be cleared, even by a kit id that no longer resolves** — the same rule
    /// the policy validation thirteen lines above already follows (*"a player must be able to abandon
    /// an investment even if its gates have since lapsed"*).
    ///
    /// The kit was resolved **before** the worker count was consulted, so `assign_labor … 0 kit
    /// <id-since-removed>` was refused outright and the band was locked into an assignment by a kit
    /// that had been edited out of `equipment.json`. Nothing downstream even wanted the answer:
    /// `LaborAllocation::set_assignment` never reads the kit at zero workers.
    ///
    /// **What "cleared" means is the TAKE CREW, not the row.** This fixture's herd is *owned*, so the
    /// band still holds it and its row survives at zero hunters to keep drawing from the `husbandry`
    /// pool (`docs/plan_standing_upkeep.md` §2.2). The defect this pins is the refusal, which is
    /// visible either way: a refused command leaves the crew exactly where it was.
    ///
    /// The band is staffed by the fixture rather than by a second command, so the precondition
    /// cannot fail for a reason that has nothing to do with kits — and it is asserted, because an
    /// unassign that found nothing there would pass this test with the defect intact.
    #[test]
    fn an_unassign_is_not_blocked_by_a_kit_id_that_no_longer_exists() {
        /// A roster id `equipment.json` does not carry — what a kit removed by a config edit looks
        /// like to a command composed against the old roster.
        const RETIRED_KIT: &str = "obsidian_spears";

        let mut app = build_test_app();
        let faction = FactionId(0);
        let herd_id = seed_herd(&mut app, UVec2::new(1, 1), Some(faction));
        spawn_addressable_band(&mut app, faction, &herd_id);
        let staffed = |app: &mut bevy::prelude::App| {
            app.world
                .query::<&LaborAllocation>()
                .iter(&app.world)
                .any(|allocation| {
                    allocation
                        .assignments
                        .iter()
                        .any(|assignment| assignment.workers > 0)
                })
        };

        assert!(
            staffed(&mut app),
            "the fixture must actually staff a hunt crew first"
        );
        handle_assign_labor(
            &mut app,
            faction,
            Some(FIXTURE_BAND_ID),
            "hunt".to_string(),
            0,
            Some(1),
            Some(1),
            Some(herd_id.clone()),
            None,
            None,
            Some(RETIRED_KIT.to_string()),
            Vec::new(),
        );
        assert!(
            !staffed(&mut app),
            "unassigning names a kit the roster no longer carries — the crew must still clear, or \
             the player is locked into an assignment by a kit that does not exist"
        );
    }

    /// **`build_kit` SETS ONE JOB'S TOOL, AND AN ABSENT TOKEN CLEARS IT BACK TO THE DERIVATION**
    /// (`docs/plan_standing_upkeep.md` §4.7a ②).
    ///
    /// The three states are three different statements and the command has to keep them apart:
    /// **a named kit** is an override, **an absent token** is *"whatever this entry's web wants"*,
    /// and **`kit none`** is a real selection — send this job's builders out bare-handed. The absent
    /// case is what lets the client express *"back to default"* with no new vocabulary, since
    /// `Main._kit_token` already omits the token whenever the selection equals the default.
    #[test]
    fn build_kit_sets_the_entrys_kit_and_an_absent_token_clears_it() {
        /// The plant web's own builders kit — what the roster derives for a Cultivate.
        const PLANT_BUILD_KIT: &str = "tillage";
        /// The bare-handed roster entry, which is a selection and not an absence.
        const BARE_KIT: &str = "none";

        let (mut app, band, patch) = a_band_with_a_queued_cultivate();
        let named = |app: &bevy::prelude::App| -> Option<String> {
            app.world
                .get::<LaborAllocation>(band)
                .expect("the fixture band carries an allocation")
                .build_queue_entry(&BuildSource::Patch(patch))
                .expect("the fixture entry survives")
                .kit
                .as_ref()
                .map(|kit| kit.id().to_string())
        };
        assert_eq!(
            named(&app),
            None,
            "fixture: a fresh entry names no kit, so its own web answers"
        );

        handle_build_kit(
            &mut app,
            FactionId(0),
            patch_source(patch),
            Some(PLANT_BUILD_KIT.to_string()),
        );
        assert_eq!(
            named(&app),
            Some(PLANT_BUILD_KIT.to_string()),
            "a named kit is stored on the ENTRY — the band's `builders` row carries none at all"
        );

        handle_build_kit(&mut app, FactionId(0), patch_source(patch), None);
        assert_eq!(
            named(&app),
            None,
            "an ABSENT `kit` token clears the override back to the entry's own derivation — the \
             existing 'an absent kitId means the job's default' rule, and the client's only way to \
             say 'back to default'"
        );

        handle_build_kit(
            &mut app,
            FactionId(0),
            patch_source(patch),
            Some(BARE_KIT.to_string()),
        );
        assert_eq!(
            named(&app),
            Some(BARE_KIT.to_string()),
            "…and `kit none` is a REAL selection that survives the round trip: bare-handed is a \
             different statement from 'derive', and collapsing the two makes conserving gear on \
             one job unexpressible"
        );
    }

    /// **`build_kit` FAILS CLOSED** — on a source nothing of the faction's has queued, on an id the
    /// roster does not carry, and on a kit that cannot do the `builders` job.
    ///
    /// The same failing-closed resolution every other role uses: naming a kit is how the player
    /// compares tools, so a silent substitution answers a different question than the one asked.
    #[test]
    fn build_kit_refuses_an_unqueued_source_and_a_kit_that_cannot_build() {
        /// A roster kit that lists no `builders` job — gathering is all it can do.
        const NOT_A_BUILD_KIT: &str = "gathering";
        /// An id no roster entry carries.
        const NO_SUCH_KIT: &str = "adamantine_trowel";

        // A refusal is a `CancelOrder` event whose DETAIL carries the reason —
        // `emit_command_failure` puts the sentence there and leaves the label generic.
        let refused = |app: &bevy::prelude::App, needle: &str| -> bool {
            app.world.resource::<CommandEventLog>().iter().any(|entry| {
                matches!(entry.kind, CommandEventKind::CancelOrder)
                    && entry
                        .detail
                        .as_deref()
                        .is_some_and(|detail| detail.contains(needle))
            })
        };

        // **A source in nobody's queue** — mirrors `handle_unqueue`'s own refusal, by name.
        let (mut app, _, patch) = a_band_with_a_queued_cultivate();
        let elsewhere = UVec2::new(patch.x + 7, patch.y + 7);
        handle_build_kit(&mut app, FactionId(0), patch_source(elsewhere), None);
        assert!(
            refused(&app, "queued to be built"),
            "a source nothing of yours has queued has no job to re-kit, and inventing one would \
             enrol a build the player never declared"
        );

        // **An unknown id**, and **a kit that does not list the job** — both through `resolve_kit_or`.
        for bad in [NO_SUCH_KIT, NOT_A_BUILD_KIT] {
            let (mut app, band, patch) = a_band_with_a_queued_cultivate();
            handle_build_kit(
                &mut app,
                FactionId(0),
                patch_source(patch),
                Some(bad.to_string()),
            );
            assert!(
                refused(&app, "build_kit"),
                "'{bad}' cannot raise a build and must be refused by name"
            );
            assert!(
                app.world
                    .get::<LaborAllocation>(band)
                    .expect("the fixture band carries an allocation")
                    .build_queue_entry(&BuildSource::Patch(patch))
                    .expect("the fixture entry survives")
                    .kit
                    .is_none(),
                "…and a refused command stores nothing: the entry stays on its own derivation"
            );
        }
    }

    /// **`upkeep_kit` SETS ONE WORK SITE'S TOOL, AND AN ABSENT TOKEN CLEARS IT BACK TO THE
    /// DERIVATION** (`docs/plan_standing_upkeep.md` §2.7).
    ///
    /// The same three statements `build_kit` keeps apart one account over: **a named kit** is an
    /// override, **an absent token** is *"whatever this site's web wants"*, and **`kit none`** is a
    /// real selection — work this one site bare-handed while its neighbour keeps the tool.
    ///
    /// **It lands on the ROW and on nothing else.** The row's take kit is a separate statement, and
    /// a command that quietly overwrote it would make the two selections one.
    #[test]
    fn upkeep_kit_sets_the_sites_kit_and_an_absent_token_clears_it() {
        /// The plant web's own keeping kit — what the roster derives for a patch.
        const PLANT_KEEPING_KIT: &str = "tillage";
        /// The bare-handed roster entry, which is a selection and not an absence.
        const BARE_KIT: &str = "none";

        let (mut app, band, patch) = a_band_with_a_queued_cultivate();
        let row = |app: &bevy::prelude::App| -> (Option<String>, Option<String>) {
            let assignment = app
                .world
                .get::<LaborAllocation>(band)
                .expect("the fixture band carries an allocation")
                .assignments
                .iter()
                .find(|assignment| matches!(assignment.target, LaborTarget::Forage { tile, .. } if tile == patch))
                .cloned()
                .expect("the fixture row survives");
            (
                assignment
                    .upkeep_kit
                    .as_ref()
                    .map(|kit| kit.id().to_string()),
                assignment.kit.as_ref().map(|kit| kit.id().to_string()),
            )
        };
        let (kept_with, take_kit) = row(&app);
        assert_eq!(
            kept_with, None,
            "fixture: a fresh row names no keeping kit, so its own web answers"
        );

        handle_upkeep_kit(
            &mut app,
            FactionId(0),
            patch_source(patch),
            Some(PLANT_KEEPING_KIT.to_string()),
        );
        assert_eq!(
            row(&app).0,
            Some(PLANT_KEEPING_KIT.to_string()),
            "a named kit is stored on the SITE's row — the band's `agriculture` role carries none"
        );
        assert_eq!(
            row(&app).1,
            take_kit,
            "…and the row's TAKE kit is untouched: what the gatherers carry and what the keepers \
             carry are two statements the player makes separately"
        );

        handle_upkeep_kit(&mut app, FactionId(0), patch_source(patch), None);
        assert_eq!(
            row(&app).0,
            None,
            "an ABSENT `kit` token clears the override back to the site's own derivation — the \
             existing 'an absent kitId means the job's default' rule, and the client's only way to \
             say 'back to default'"
        );

        handle_upkeep_kit(
            &mut app,
            FactionId(0),
            patch_source(patch),
            Some(BARE_KIT.to_string()),
        );
        assert_eq!(
            row(&app).0,
            Some(BARE_KIT.to_string()),
            "…and `kit none` is a REAL selection that survives the round trip: keeping ONE site \
             bare-handed to conserve the tool is a different statement from 'derive'"
        );
    }

    /// **`upkeep_kit` FAILS CLOSED** — on a source nothing of the faction's works, on an id the
    /// roster does not carry, and ⛔ **on a kit that does not serve THIS SITE'S WEB**.
    ///
    /// The last is the one the per-site scope makes reachable: a patch is kept on the `agriculture`
    /// job and a herd on `husbandry`, so a plant keeping kit named on a herd is refused by name
    /// rather than falling silently back to the animal derivation. `build_kit`'s rule and its
    /// reason — naming a kit is how a player compares tools, so a silent substitution answers a
    /// different question than the one asked.
    #[test]
    fn upkeep_kit_refuses_an_unworked_source_and_a_kit_that_cannot_keep_this_web() {
        /// The PLANT keeping kit, which is exactly what must not be accepted on a herd.
        const PLANT_KEEPING_KIT: &str = "tillage";
        /// An id no roster entry carries.
        const NO_SUCH_KIT: &str = "adamantine_trowel";

        // A refusal is a `CancelOrder` event whose DETAIL carries the reason. ⛔ The needle has to be
        // one an APPLIED command cannot also carry: a success writes `action=upkeep_kit` into the
        // same field, so bare `"upkeep_kit"` reads true for the very outcome this is testing against.
        let refused = |app: &bevy::prelude::App, needle: &str| -> bool {
            app.world.resource::<CommandEventLog>().iter().any(|entry| {
                matches!(entry.kind, CommandEventKind::CancelOrder)
                    && entry
                        .detail
                        .as_deref()
                        .is_some_and(|detail| detail.contains(needle))
            })
        };

        // **A source nothing of yours works** — there is no row for a keeping kit to live on, and
        // minting one would enrol a holding the player never took.
        let (mut app, _, patch) = a_band_with_a_queued_cultivate();
        let elsewhere = UVec2::new(patch.x + 7, patch.y + 7);
        handle_upkeep_kit(&mut app, FactionId(0), patch_source(elsewhere), None);
        assert!(
            refused(&app, "to keep"),
            "a source nothing of yours works has no row to re-kit"
        );

        // **An unknown id** on a patch.
        let (mut app, band, patch) = a_band_with_a_queued_cultivate();
        handle_upkeep_kit(
            &mut app,
            FactionId(0),
            patch_source(patch),
            Some(NO_SUCH_KIT.to_string()),
        );
        assert!(
            refused(&app, "upkeep_kit: "),
            "'{NO_SUCH_KIT}' is on no roster and must be refused by name"
        );
        assert!(
            app.world
                .get::<LaborAllocation>(band)
                .expect("the fixture band carries an allocation")
                .assignments
                .iter()
                .all(|assignment| assignment.upkeep_kit.is_none()),
            "…and a refused command stores nothing: the site stays on its own derivation"
        );

        // ⛔ **THE WRONG WEB** — the plant keeping kit named on a herd.
        let mut app = build_test_app();
        let faction = FactionId(0);
        let herd_id = seed_herd(&mut app, UVec2::new(1, 1), Some(faction));
        let band = spawn_addressable_band(&mut app, faction, &herd_id);
        handle_upkeep_kit(
            &mut app,
            faction,
            BuildSourceRef {
                target_x: None,
                target_y: None,
                herd_id: Some(herd_id.clone()),
            },
            Some(PLANT_KEEPING_KIT.to_string()),
        );
        assert!(
            refused(&app, "upkeep_kit: "),
            "a PLANT keeping kit on a herd serves nothing there and is a command failure, never a \
             silent fall back to the animal derivation"
        );
        assert!(
            app.world
                .get::<LaborAllocation>(band)
                .expect("the fixture band carries an allocation")
                .assignments
                .iter()
                .all(|assignment| assignment.upkeep_kit.is_none()),
            "…and nothing was stored: the herd stays on its own web's tool"
        );
    }

    /// The `build_kit` fixture: one band foraging one patch, with a `Cultivate` declared on it.
    fn a_band_with_a_queued_cultivate() -> (bevy::prelude::App, Entity, UVec2) {
        let mut app = build_test_app();
        let faction = FactionId(0);
        let patch = UVec2::new(2, 2);
        let band = spawn_resident_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: patch,
                floor: DEFAULT_ESCAPEMENT_FLOOR,
                species: None,
                take_species: TakeSelection::EVERYTHING,
            },
        );
        app.world.entity_mut(band).insert(BandId(FIXTURE_BAND_ID));
        assert!(
            app.world
                .get_mut::<LaborAllocation>(band)
                .expect("the fixture band carries an allocation")
                .enqueue_build(
                    BuildSource::Patch(patch),
                    BuildJob::Rung(Improvement::Cultivate),
                ),
            "fixture: the band works this patch, so a declaration on it is accepted"
        );
        (app, band, patch)
    }

    /// **A KEEPING ROLE TAKES NO KIT EITHER, AND THE TOKEN IS REFUSED BY NAME**
    /// (`docs/plan_standing_upkeep.md` §2.7) — the `builders` row's rule one account over.
    ///
    /// The `agriculture` / `husbandry` rows say **how many** keepers a web gets; what the keepers of
    /// one site carry is that site's own statement (`upkeep_kit`). A kit stored here reached the
    /// split through `LaborAllocation::named_kit_on` until §2.7 and reaches nothing now, so
    /// swallowing the token would be the worst version of the defect the builders row had: the
    /// player names a tool, the sim stores it, and no keeper anywhere picks it up.
    ///
    /// **The pair is the test.** The token is refused, *and* a keeper count with no token still
    /// staffs the role — a fix that refused everything would satisfy the first half alone.
    #[test]
    fn a_keeping_role_takes_no_kit_and_the_token_is_refused_rather_than_swallowed() {
        /// The plant keeping kit — a perfectly valid `agriculture` kit, which is the point: it is
        /// refused for being on the wrong *seam*, not for being the wrong tool.
        const PLANT_KEEPING_KIT: &str = "tillage";
        /// Hands on the keeping role — any positive count.
        const KEEPERS: u32 = 2;

        let staffed_with = |role: &str, named: Option<&str>| -> u32 {
            let mut app = build_test_app();
            let faction = FactionId(0);
            let patch = UVec2::new(2, 2);
            let band = spawn_resident_working_band(
                &mut app,
                faction,
                LaborTarget::Forage {
                    tile: patch,
                    floor: DEFAULT_ESCAPEMENT_FLOOR,
                    species: None,
                    take_species: TakeSelection::EVERYTHING,
                },
            );
            app.world.entity_mut(band).insert(BandId(FIXTURE_BAND_ID));
            handle_assign_labor(
                &mut app,
                faction,
                Some(FIXTURE_BAND_ID),
                role.to_string(),
                KEEPERS,
                None,
                None,
                None,
                None,
                None,
                named.map(str::to_string),
                Vec::new(),
            );
            let target = if role == "agriculture" {
                LaborTarget::Agriculture
            } else {
                LaborTarget::Husbandry
            };
            app.world
                .get::<LaborAllocation>(band)
                .expect("the fixture band carries an allocation")
                .workers_on(&target)
        };

        for role in ["agriculture", "husbandry"] {
            assert_eq!(
                staffed_with(role, None),
                KEEPERS,
                "`assign_labor … {role} <n>` still staffs the role — the refusal below must be \
                 about the KIT and not about the command"
            );
            // A refused command returns before it touches the allocation, which is the observable
            // difference between refusing and silently swallowing the token.
            assert_eq!(
                staffed_with(role, Some(PLANT_KEEPING_KIT)),
                0,
                "naming a kit on the `{role}` row must be refused by name: the keeping kit is per \
                 WORK SITE, and a token stored here would be picked up by nobody"
            );
        }
    }

    /// A tile named the way the queue family names one.
    fn patch_source(tile: UVec2) -> BuildSourceRef {
        BuildSourceRef {
            target_x: Some(tile.x),
            target_y: Some(tile.y),
            herd_id: None,
        }
    }

    /// **A `builders` row CANNOT NAME A KIT, and the head entry's own web answers instead**
    /// (`docs/plan_standing_upkeep.md` §4.7a ②).
    ///
    /// The command boundary used to resolve *every* row's absent kit into a stored id, so a builders
    /// row carried `default_kits.builders` = `none` and the pool built bare-handed. That was fixed by
    /// storing nothing — but the row could still carry an **explicit** kit, and that override won
    /// permanently: measured in play, one pick pinned `hurdling` onto every later builders command,
    /// locking a band raising a *plant* Cultivate to the animal web's tool with no way back. So the
    /// row's kit is gone entirely and `build_kit` sets it per queue entry.
    ///
    /// **The refusal is asserted with the two derived cases and is what makes them mean something**:
    /// a fix that merely stopped *storing* the token would satisfy the first two while silently
    /// swallowing a kit the player named.
    #[test]
    fn a_builders_row_takes_no_kit_and_derives_from_the_head_entry() {
        /// The roster kit the plant web's builds want (`equipment.json` → `build_work` on `hoes`).
        const PLANT_BUILD_KIT: &str = "tillage";
        /// …and the animal web's (`hurdles`).
        const ANIMAL_BUILD_KIT: &str = "hurdling";
        /// The bare-handed roster entry: `default_kits.builders`, and what an explicit `kit none`
        /// names. Both readings must be distinguishable, which is the point of the third case.
        const BARE_KIT: &str = "none";
        /// Hands on the builders row — any positive count; the fork only asks `workers > 0`.
        const BUILDERS: u32 = 2;

        /// One trip through the real command path: a band whose build queue head is on `animal_head`'s
        /// web takes an `assign_labor … builders` carrying `named`, and the kit the pool ends up
        /// working with is read back through the one seam the turn and the wire both resolve through.
        fn resolved_builders_kit(animal_head: bool, named: Option<&str>) -> String {
            let mut app = build_test_app();
            let faction = FactionId(0);
            let herd_id = seed_herd(&mut app, UVec2::new(1, 1), Some(faction));
            let patch = UVec2::new(2, 2);
            let (worked, head) = if animal_head {
                (
                    LaborTarget::Hunt {
                        fauna_id: herd_id.clone(),
                        floor: DEFAULT_ESCAPEMENT_FLOOR,
                    },
                    core_sim::BuildQueueEntry {
                        source: BuildSource::Herd(herd_id.clone()),
                        declared: BuildJob::Rung(Improvement::Tame),
                        kit: None,
                    },
                )
            } else {
                (
                    LaborTarget::Forage {
                        tile: patch,
                        floor: DEFAULT_ESCAPEMENT_FLOOR,
                        species: None,
                        take_species: TakeSelection::EVERYTHING,
                    },
                    core_sim::BuildQueueEntry {
                        source: BuildSource::Patch(patch),
                        declared: BuildJob::Rung(Improvement::Cultivate),
                        kit: None,
                    },
                )
            };
            let band = spawn_resident_working_band(&mut app, faction, worked);
            app.world.entity_mut(band).insert(BandId(FIXTURE_BAND_ID));
            app.world
                .get_mut::<LaborAllocation>(band)
                .expect("the fixture band carries an allocation")
                .build_queue
                .push(head);

            handle_assign_labor(
                &mut app,
                faction,
                Some(FIXTURE_BAND_ID),
                "builders".to_string(),
                BUILDERS,
                None,
                None,
                None,
                None,
                None,
                named.map(str::to_string),
                Vec::new(),
            );

            let equipment = app.world.resource::<EquipmentConfigHandle>().get();
            let allocation = app
                .world
                .get::<LaborAllocation>(band)
                .expect("the fixture band carries an allocation");
            assert_eq!(
                allocation.workers_on(&LaborTarget::Builders),
                BUILDERS,
                "the command must actually staff the builders row, or this measures nothing"
            );
            allocation.builders_kit(&equipment).id().to_string()
        }

        /// **Does `assign_labor … builders <n> kit <id>` refuse?** Measured by the row staying
        /// unstaffed: the handler returns before it touches the allocation.
        fn builders_kit_token_is_refused(named: &str) -> bool {
            let mut app = build_test_app();
            let faction = FactionId(0);
            let patch = UVec2::new(2, 2);
            let band = spawn_resident_working_band(
                &mut app,
                faction,
                LaborTarget::Forage {
                    tile: patch,
                    floor: DEFAULT_ESCAPEMENT_FLOOR,
                    species: None,
                    take_species: TakeSelection::EVERYTHING,
                },
            );
            app.world.entity_mut(band).insert(BandId(FIXTURE_BAND_ID));
            handle_assign_labor(
                &mut app,
                faction,
                Some(FIXTURE_BAND_ID),
                "builders".to_string(),
                BUILDERS,
                None,
                None,
                None,
                None,
                None,
                Some(named.to_string()),
                Vec::new(),
            );
            app.world
                .get::<LaborAllocation>(band)
                .expect("the fixture band carries an allocation")
                .workers_on(&LaborTarget::Builders)
                == 0
        }

        assert_eq!(
            resolved_builders_kit(false, None),
            PLANT_BUILD_KIT,
            "a builders pool on a plant-web head with no kit named must work the roster's plant \
             build kit — storing the job default here makes §4.6b's derivation unreachable and \
             sends the pool out bare-handed"
        );
        assert_eq!(
            resolved_builders_kit(true, None),
            ANIMAL_BUILD_KIT,
            "…and the same row on an animal-web head must work the roster's animal build kit"
        );
        // **A kit token on the row is REFUSED, not swallowed.** The command leaves the row
        // unstaffed, so the fixture's own liveness assertion inside `resolved_builders_kit` would
        // trip — which is exactly the observable difference between refusing and ignoring, and is
        // why this arm reads the refusal directly rather than through that helper.
        assert!(
            builders_kit_token_is_refused(BARE_KIT),
            "naming a kit on the builders row must be refused by name: the builders kit is per \
             queue entry, and a silently-dropped token is the same defect as the pinning override \
             it replaced"
        );
    }

    /// **The collapse window's prose may not contradict the three numbers published beside it.**
    ///
    /// The `low == high` arm printed `likely`, so a band of 3–3 beside a likely of 4 rendered
    /// *"4 turns"* — prose disagreeing with the `low=`/`high=` tokens in the same feed entry. And
    /// `(None, Some(high))` fell to the same default, throwing away a bound the projection *did*
    /// produce.
    ///
    /// The three quantiles are separate projections over a whole-animal take, so `likely` outside
    /// `low..=high` is a reachable state rather than a hypothetical — which is why the window is the
    /// span of all three and collapses to a point only when all three agree.
    #[test]
    fn the_collapse_window_never_prints_a_count_its_own_range_contradicts() {
        // The degenerate reading survives: three numbers that agree read as one number.
        assert_eq!(
            describe_collapse_window(Some(4), 4, Some(4)),
            "4 turns",
            "a range that is a point must read as a point"
        );
        // The defect: a point band beside a likely outside it.
        assert_eq!(
            describe_collapse_window(Some(3), 4, Some(3)),
            "3–4 turns",
            "the prose must contain every number published beside it"
        );
        assert_eq!(describe_collapse_window(Some(3), 4, Some(5)), "3–5 turns");
        // Only the optimistic end reached the line: an open-ended window, not a second number.
        assert_eq!(describe_collapse_window(Some(3), 4, None), "3+ turns");
        // …and its mirror, which used to discard the one bound it had.
        assert_eq!(describe_collapse_window(None, 4, Some(6)), "up to 6 turns");
        // Neither end reached it: the point estimate is all there is to say.
        assert_eq!(describe_collapse_window(None, 4, None), "4 turns");
    }

    /// **A raiding party is bounded by the BAND, and by nothing else** (`docs/plan_denial_raid.md`
    /// §3.1).
    ///
    /// The reported defect was a party of **9** refused from a band holding **16** idle workers,
    /// because the config's party lever read `8` and a Red Deer herd at 51 of 119 head needs nine
    /// hunters to out-kill its regrowth. Two unrelated eights, and the config one won.
    ///
    /// That lever was doing two jobs under one name. The **sampling** job — how far the per-*herd*
    /// estimate tables are quoted, on a table that cannot know which band is asking — is real and
    /// survives as `estimate_party_sizes`. The **rules cap** on what a player may send had no design
    /// behind it, and the honest bound is the one the band panel already displays.
    ///
    /// Since the sampling axis became an explicit **ladder**, "past the bound" is restated as *"a
    /// party size the ladder does not quote"* — the honest form of the same claim, because the
    /// estimate tables now stop being able to answer for a party at the first gap between rungs
    /// rather than at a flat ceiling.
    ///
    /// Three assertions, and the pairing is what makes them mean something:
    /// 1. a denial raid the tables cannot quote **launches**, with the party it asked for;
    /// 2. so does a **hunt** — pinned deliberately, because a hunt's party sizing IS changed by this
    ///    split and *"unchanged unless deliberate"* has to be recorded either way;
    /// 3. a party past the **band** is still refused, on both verbs, so the bound moved rather than
    ///    vanished.
    #[test]
    fn a_raiding_party_is_bounded_by_the_band_and_not_by_the_sampling_lever() {
        // 1 + 2. Both raiding verbs launch whatever party the band can field.
        for verb in [RaidVerb::Deny, RaidVerb::Hunt] {
            let mut app = build_test_app();
            // Startup, so the world carries the tile registry every launch path resolves against.
            app.update();
            let faction = FactionId(0);
            let herd_id = seed_herd(&mut app, UVec2::new(1, 1), None);
            let band = spawn_addressable_band(&mut app, faction, &herd_id);
            let pool = available_workers(
                app.world
                    .get::<PopulationCohort>(band)
                    .expect("the fixture band exists")
                    .working,
            );
            // **The largest party the band can field.** This used to be *"the largest party the
            // sampling ladder does not carry a row for"*, because a launch had to be shown to work
            // for a party the estimate tables could not quote. The ladder and the tables are gone —
            // a forecast is asked for by party size now, so every party is quotable — and what is
            // left to assert is the bound itself: the band, and nothing else.
            let unquoted_party = pool;
            assert!(
                unquoted_party > 1,
                "{verb:?}: the fixture only means something while the band can spare a real party \
                 ({pool} workers)"
            );

            verb.launch(&mut app, faction, unquoted_party, herd_id);
            let launched: Vec<u32> = app
                .world
                .query::<(&Expedition, &PopulationCohort)>()
                .iter(&app.world)
                .map(|(_, cohort)| available_workers(cohort.working))
                .collect();
            assert_eq!(
                launched,
                vec![unquoted_party],
                "{verb:?}: a party of {unquoted_party} must launch from a band of {pool} — the \
                 band's own workers are the only rule about what may be sent"
            );
        }

        // 3. The bound MOVED, it did not vanish: a party past the band is still refused, both verbs.
        for verb in [RaidVerb::Deny, RaidVerb::Hunt] {
            let mut app = build_test_app();
            app.update();
            let faction = FactionId(0);
            let herd_id = seed_herd(&mut app, UVec2::new(1, 1), None);
            let band = spawn_addressable_band(&mut app, faction, &herd_id);
            let pool = available_workers(
                app.world
                    .get::<PopulationCohort>(band)
                    .expect("the fixture band exists")
                    .working,
            );
            verb.launch(&mut app, faction, pool + 1, herd_id);
            assert!(
                expedition_failure_detail_contains(&app, "workers invalid"),
                "{verb:?}: a party larger than the band itself must still be refused"
            );
            assert_eq!(
                app.world.query::<&Expedition>().iter(&app.world).count(),
                0,
                "{verb:?}: …and no party may be spawned"
            );
        }
    }

    /// The two raiding verbs, so the party-bound fixture states its claim about **both** rather than
    /// about whichever one it happened to call.
    #[derive(Debug, Clone, Copy)]
    enum RaidVerb {
        Deny,
        Hunt,
    }

    impl RaidVerb {
        fn launch(
            self,
            app: &mut bevy::prelude::App,
            faction: FactionId,
            party_workers: u32,
            fauna_id: String,
        ) {
            match self {
                RaidVerb::Deny => {
                    handle_send_denial_raid(
                        app,
                        faction,
                        Some(FIXTURE_BAND_ID),
                        party_workers,
                        fauna_id,
                        None,
                    );
                }
                RaidVerb::Hunt => handle_send_hunt_expedition(
                    app,
                    faction,
                    Some(FIXTURE_BAND_ID),
                    party_workers,
                    fauna_id,
                    None,
                    None,
                ),
            }
        }
    }

    /// The [`BandId`] the party-size fixture addresses its band by. A launch command names a band by
    /// its **`BandId`**, not by entity bits (a restore renumbers entities), so a fixture that wants a
    /// band of its own choosing has to give it one — the worldgen bands the default picker would
    /// otherwise grab carry a pool this test does not control.
    const FIXTURE_BAND_ID: u64 = 90_001;

    /// A resident band with a `BandId` this fixture can address, staffed on `herd_id` so it is a
    /// realistic working band rather than an idle one.
    fn spawn_addressable_band(
        app: &mut bevy::prelude::App,
        faction: FactionId,
        herd_id: &str,
    ) -> Entity {
        let band = spawn_resident_working_band(
            app,
            faction,
            LaborTarget::Hunt {
                fauna_id: herd_id.to_string(),
                floor: 0.5,
            },
        );
        app.world.entity_mut(band).insert(BandId(FIXTURE_BAND_ID));
        band
    }

    /// Did any `ExpeditionSent` event carry a failure naming `needle`? The launch handlers report
    /// their refusals as feed entries, so this is how a command-level test reads a rejection.
    fn expedition_failure_detail_contains(app: &bevy::prelude::App, needle: &str) -> bool {
        app.world.resource::<CommandEventLog>().iter().any(|entry| {
            matches!(entry.kind, CommandEventKind::ExpeditionSent)
                && entry
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains(needle))
        })
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
        let mut app = build_test_app();
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
                take_species: TakeSelection::EVERYTHING,
            },
        );

        // The stance validator has nothing to say about a plant source, and the improvement's own
        // gates ask about knowledge, health and ownership — never about the stance beside them.
        let target = LaborTarget::Forage {
            tile: coord,
            floor: 0.15,
            species: None,
            take_species: TakeSelection::EVERYTHING,
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

    // **RETIRED with `abandon_improvement`**: `a_running_improvement_can_be_abandoned_and_the_stance_survives`
    // and `abandoning_nothing_is_rejected_by_name`. There is no command left to test — the build verb
    // is derived from the meter (`docs/plan_standing_upkeep.md` §2.4) and a player walks away by
    // unstaffing the builders, which `handle_cultivate`'s own zero-crew path already covers.

    /// The kind gates: `Corral` on a forage patch and `Cultivate` on a herd are both rejected
    /// outright by `validate_improvement` (the guard each build command routes through).
    #[test]
    fn cross_web_improvements_are_rejected() {
        let mut app = build_test_app();
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
                take_species: TakeSelection::EVERYTHING,
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
    /// **HOW FAR THE STEADY HEADLINE MAY MOVE ACROSS THE TURN IT PROJECTED.** `realized` is a window
    /// average over a **quantised** take, so resolving the turn slides the window and re-phases the
    /// pulse inside it — a few percent, on a herd whose whole steady rate is a couple of bodies a
    /// window. The defect the no-jump tests exist for is an order-of-magnitude **lurch** (the arc
    /// opened on an 8× disagreement between the headline and the compose sheet), which this refuses
    /// with two digits to spare.
    const REALIZED_NO_JUMP_FRACTION: f32 = 0.10;
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
            None,
            Vec::new(),
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
            None,
            Vec::new(),
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
    /// **One turn of Logistics, then the Population labor arm** — the two stages a seeded row spans.
    ///
    /// The seed is a **pre-commit** forecast: it prices the source as the *next* take will find it,
    /// one Logistics regrowth on (`forage::next_turns_stand` / `fauna::next_turns_quarry`), because
    /// every production reader of it sits after the Population take. A harness that ran the labor arm
    /// alone would compare the quote against a turn whose regrowth never happened, and the difference
    /// would be exactly the growth — so *"no jump"* would be asserting the wrong thing.
    fn resolve_labor(app: &mut bevy::prelude::App) {
        use bevy_ecs::system::RunSystemOnce;
        app.world.run_system_once(core_sim::advance_forage_regrowth);
        {
            let fauna = app.world.resource::<FaunaConfigHandle>().get();
            for herd in app.world.resource_mut::<HerdRegistry>().herds.iter_mut() {
                *herd = core_sim::next_turns_quarry(herd, &fauna);
            }
        }
        app.world
            .run_system_once(core_sim::advance_labor_allocation);
    }

    /// **Forage.** A brand-new assignment reports its expected yield immediately — BEFORE any turn is
    /// advanced — and that seed is exactly what the pre-commit forecast promises.
    #[test]
    fn assigning_forage_workers_seeds_the_expected_yield_before_the_turn() {
        let mut app = build_test_app();
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
            // The band under test is freshly spawned, so its baskets are whole — the seed path
            // resolves the same equipped tier.
            equipped_gather_reference(),
            1.0,
            1.0,
            BAND_WORKERS,
            0.5,
            &TakeSelection::EVERYTHING,
            labor.yield_average_horizon_turns,
            labor.arrivals_horizon_turns,
            app.world
                .resource::<CombatConfigHandle>()
                .get()
                .forecast_range_sigmas,
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
        let mut app = build_test_app();
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
        let mut app = build_test_app();
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
        let expected = hunt_source_yield_preview(
            herd,
            &fauna,
            equipped_haul_reference(),
            &HuntingParty::builtin_equipped(),
            1.0,
            BAND_WORKERS,
            0.5,
            labor.yield_average_horizon_turns,
            labor.arrivals_horizon_turns,
            app.world
                .resource::<CombatConfigHandle>()
                .get()
                .forecast_range_sigmas,
        );
        assert!(
            (seeded - expected.actual).abs() < SEED_EPSILON,
            "seed {seeded} must equal the forecast {}",
            expected.actual
        );
    }

    /// **A wary, light-bodied quarry** — the row where the trap's `dispersion 0` genuinely beats the
    /// job default, so "the command matched the wire" is a claim about a kit that MOVED.
    const WARREN: &str = "Rabbit Warren";

    /// The id every per-herd-default fixture below re-badges its herd to.
    const QUARRY_DEFAULT_FIXTURE_HERD: &str = "quarry_default_fixture_herd";

    /// The crew a per-herd-default launch fixture sends out — small, because the claim is about
    /// which kit the party carries, not about what it brings home.
    const PARTY_ON_A_DEFAULT_KIT_RAID: u32 = 2;

    /// Re-badge a stationary game group as a [`WARREN`] under [`QUARRY_DEFAULT_FIXTURE_HERD`] and
    /// refresh the display telemetry the capture reads.
    ///
    /// Re-badging rather than spawning a herd on invented ground: it is already on a real tile,
    /// already in the registry, and already reachable by the starting band the command resolves.
    fn pin_a_warren(app: &mut bevy::prelude::App) -> String {
        let body_mass = app
            .world
            .resource::<FaunaConfigHandle>()
            .get()
            .species_by_display(WARREN)
            .expect("the roster ships the warren")
            .body_mass;
        let id = QUARRY_DEFAULT_FIXTURE_HERD.to_string();
        {
            let mut registry = app.world.resource_mut::<HerdRegistry>();
            let herd = registry
                .herds
                .iter_mut()
                .find(|herd| herd.id.starts_with("game_") && herd.route_length() == 1)
                .expect("the campaign map seeds a stationary game group");
            herd.id = id.clone();
            herd.species = WARREN.to_string();
            herd.body_mass = body_mass;
        }
        refresh_herd_telemetry(app);
        id
    }

    /// Rebuild `HerdTelemetry` off the authoritative registry — the capture reads the display list,
    /// so a fixture that edits a `Herd` must republish it or the wire describes the old herd.
    fn refresh_herd_telemetry(app: &mut bevy::prelude::App) {
        let entries = app.world.resource::<HerdRegistry>().snapshot_entries();
        app.world.resource_mut::<core_sim::HerdTelemetry>().entries = entries;
    }

    /// `HerdTelemetryState.defaultKitId` for one herd, read off the **encoded** envelope through the
    /// client's own accessor chain — a field that never reached the codec still passes an in-process
    /// assertion.
    fn published_default_kit_for(app: &mut bevy::prelude::App, herd_id: &str) -> String {
        use core_sim::{recapture_snapshot_in_place, SnapshotHistory};
        use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

        recapture_snapshot_in_place(&mut app.world);
        let snapshot = app
            .world
            .resource::<SnapshotHistory>()
            .latest_entry()
            .expect("a snapshot was captured")
            .snapshot;
        let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
        let envelope = fb::root_as_envelope(&bytes).expect("the snapshot encodes");
        envelope
            .payload_as_snapshot()
            .expect("the envelope carries a snapshot")
            .subsistence()
            .and_then(|section| section.herds())
            .expect("the subsistence section carries the herd list")
            .iter()
            .find(|herd| herd.id() == Some(herd_id))
            .and_then(|herd| herd.defaultKitId())
            .expect("the herd publishes the kit its sheet opens on")
            .to_string()
    }

    /// **A hunt row with no `kit` token stores the kit the WIRE published for that herd.**
    ///
    /// The compose sheet opens on `HerdTelemetryState.defaultKitId`, so if the command boundary
    /// still resolved `default_kits.hunt` the sheet would say Trapping and the command would run
    /// Stalking — the silent substitution `equipment.md`'s "an unknown id is a command failure,
    /// never a silent fall back" exists to prevent, arriving through the *absent*-token door.
    ///
    /// Asserted against the **published id**, decoded off the encoded snapshot, rather than against
    /// a literal or a second call to the scorer: the claim is that the two surfaces agree, and a
    /// re-derivation would agree with itself no matter what the wire said.
    #[test]
    fn a_hunt_row_with_no_kit_named_stores_the_kit_the_wire_published_for_that_herd() {
        // The REAL campaign world, because the assertion reads a captured snapshot and the capture
        // wants every worldgen resource. `fog_enabled = false` so the pinned herd reaches the wire
        // wherever it stands rather than the test asserting about visibility.
        let mut app = build_test_app();
        app.world.resource_mut::<SimulationConfig>().fog_enabled = false;
        app.update();
        let faction = FactionId(0);
        let id = pin_a_warren(&mut app);
        let published = published_default_kit_for(&mut app, &id);

        // **No `kit` token** — the absent-token path, which is the one under test.
        assign_hunt(&mut app, faction, &id, SUSTAIN_FLOOR, BAND_WORKERS);

        let stored = app
            .world
            .iter_entities()
            .filter_map(|entity| entity.get::<LaborAllocation>())
            .flat_map(|allocation| allocation.assignments.iter())
            .find_map(|assignment| assignment.kit.as_ref().map(|kit| kit.id().to_string()))
            .expect("the hunt row stored a resolved kit");
        assert_eq!(
            stored, published,
            "the command resolves the same per-herd default the sheet opened on"
        );
        assert_ne!(
            published,
            app.world
                .resource::<EquipmentConfigHandle>()
                .get()
                .default_kit_id(KitJob::Hunt),
            "LIVENESS: on this quarry the herd default DIFFERS from the job default, so the \
             equality above is a real agreement rather than both surfaces reading the same fallback"
        );
    }

    /// **A raid with no `kit` token launches on the kit the WIRE published for that herd** — the
    /// expedition twin of the assign-labor agreement above, and the same defect class.
    ///
    /// The client's launch sheet reads `HerdTelemetryState.defaultKitId` and quotes both estimate
    /// tables (`huntTripEstimatesKitId` / `denialEstimatesKitId`, which are that same id by
    /// construction). While `resolve_raid_kit` resolved `default_kits.hunt`, the sheet said Trapping
    /// and the party went out Stalking, so the forecast the player committed from was **not** the
    /// one they got.
    ///
    /// The `assert_ne!` is what makes the equality a real agreement: on a quarry whose default IS
    /// the job default, a verb that ignored the herd entirely would still pass the first assertion.
    #[test]
    fn a_raid_with_no_kit_named_launches_on_the_kit_the_wire_published_for_that_herd() {
        let mut app = build_test_app();
        app.world.resource_mut::<SimulationConfig>().fog_enabled = false;
        app.update();
        let faction = FactionId(0);
        let id = pin_a_warren(&mut app);
        let published = published_default_kit_for(&mut app, &id);

        // **No `kit` token** — the absent-token path, which is the one under test.
        handle_send_hunt_expedition(
            &mut app,
            faction,
            None,
            PARTY_ON_A_DEFAULT_KIT_RAID,
            id.clone(),
            None,
            None,
        );

        let launched = {
            let mut query = app.world.query::<&Expedition>();
            query
                .iter(&app.world)
                .next()
                .expect("the launch spawned a detached party")
                .kit
                .id()
                .to_string()
        };
        assert_eq!(
            launched, published,
            "the raid goes out on the same per-herd default the launch sheet opened on"
        );
        assert_ne!(
            published,
            app.world
                .resource::<EquipmentConfigHandle>()
                .get()
                .default_kit_id(KitJob::Hunt),
            "LIVENESS: on this quarry the herd default DIFFERS from the job default, so the \
             equality above is a real agreement rather than both surfaces reading the same fallback"
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
        let mut app = build_test_app();
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
    /// first resolved turn recomputes the same projection from the herd the turn left behind — so
    /// the headline "Food /turn" is steady between compose-time and the first resolved turn, even
    /// though `actual` (the lumpy kill) may pulse.
    ///
    /// # ⛔ IT IS A SLIDING WINDOW OVER A QUANTISED TAKE, SO "STEADY" IS NOT BIT-FOR-BIT
    ///
    /// `project_realized_hunt` averages `yield_average_horizon_turns` simulated turns, each of them
    /// `regrow → take`, and each take lands in **whole animals**. The seed's window opens on the turn
    /// about to be resolved; the resolved row's opens on the one after it, because the turn in
    /// between actually happened — so the window slides, the pulse inside it re-phases, and the herd
    /// the second window starts from is the one the turn left. What must not happen is a **lurch**,
    /// which is what [`REALIZED_NO_JUMP_FRACTION`] bounds.
    ///
    /// **The exact equality it replaced was an artifact of a frozen harness** — see [`resolve_labor`]
    /// — where the standing room quantised to zero whole animals, nothing was taken, and the herd the
    /// second projection started from was byte-identical to the first. A harness that resolves the
    /// turn the row is about cannot reproduce that, and should not.
    #[test]
    fn resolved_hunt_realized_equals_the_seeded_realized() {
        let mut app = build_test_app();
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
            (resolved - seeded).abs() <= seeded * REALIZED_NO_JUMP_FRACTION + SEED_EPSILON,
            "the forward-projected realized is steady across the turn it projected — it may re-phase \
             by a body, it may not lurch: seed {seeded}, resolved {resolved}"
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
        let mut app = build_test_app();
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

    /// **A barren source still reads `+0.00`.** The seed is a forecast, not a fiction: ground that
    /// grows nothing yields nothing, so `+0.00` stays reachable — and correct — there.
    ///
    /// # ⛔ A STRIPPED STAND IS NOT BARREN GROUND, AND THE FIXTURE HAS TO SAY WHICH IT MEANS
    ///
    /// The fixture used to be a patch at **zero biomass on ordinary ground**, which is *not* a source
    /// that yields nothing: `forage::regrow_patch` lifts a depleted stand to its reseed floor and
    /// regrows it, so the very next gather takes a real harvest — and the seeded row prices the turn
    /// the take runs on (`forage::next_turns_stand`), which is that one. Quoting `0` there would be
    /// the fiction this test exists to forbid, and the `realized` headline on the same row has always
    /// said so (`project_realized_forage` regrows first too).
    ///
    /// So the barren case is stated as barren **ground** — no carrying capacity, nothing to reseed
    /// from — which is the state `capacity_by_biome`'s `NO_FORAGE_CAPACITY` names and the one where
    /// `+0.00` is the honest answer at every horizon.
    #[test]
    fn a_barren_source_seeds_zero() {
        let mut app = build_test_app();
        let faction = FactionId(0);
        let coord = UVec2::new(1, 1);
        let tile = seed_tile_grid(&mut app, coord);
        seed_patch_with_biomass(&mut app, coord, 0.0, EcologyPhase::Collapsing);
        {
            let mut registry = app.world.resource_mut::<ForageRegistry>();
            let patch = registry
                .patch_mut(coord)
                .expect("the fixture seeded a patch");
            patch.carrying_capacity = core_sim::NO_FORAGE_CAPACITY;
        }
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
            let mut app = build_test_app();
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
        let mut app = build_test_app();
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
                take_species: TakeSelection::EVERYTHING,
            },
            CANCEL_FORAGE_WORKERS,
            available,
            None,
        );
        allocation.set_assignment(
            LaborTarget::Hunt {
                fauna_id: CANCEL_HERD_ID.to_string(),
                floor: 0.5,
            },
            CANCEL_HUNT_WORKERS,
            available,
            None,
        );
        allocation.set_assignment(LaborTarget::Scout, CANCEL_SCOUT_WORKERS, available, None);
        allocation.set_assignment(
            LaborTarget::Warrior,
            CANCEL_WARRIOR_WORKERS,
            available,
            None,
        );
        app.world.entity_mut(band).insert(allocation);
        (band, coord)
    }

    /// Unassigned workers, through the same seam the snapshot publishes and the commands clamp
    /// against — so a fixture cannot drift from either.
    fn idle_workers(app: &bevy::prelude::App, band: Entity) -> u32 {
        band_workforce(app, band).idle()
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
        let mut app = build_test_app();
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
        let mut app = build_test_app();
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
        let mut app = build_test_app();
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
        let mut app = build_test_app();
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
        let mut app = build_test_app();
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
                take_species: TakeSelection::EVERYTHING,
            },
            CANCEL_FORAGE_WORKERS,
            available,
            None,
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

    // --- the query channel ------------------------------------------------------------------

    /// A hunt query, framed as the client sends it. The values are irrelevant to the transport —
    /// what is exercised is the framing and the correlation.
    fn a_query_envelope(request_id: u64) -> ProtoCommandEnvelope {
        ProtoCommandEnvelope {
            payload: ProtoCommandPayload::Query {
                request_id,
                query: a_hunt_query(),
            },
            correlation_id: None,
        }
    }

    /// The one query shape these transport tests send. Its values never reach a world.
    fn a_hunt_query() -> QueryPayload {
        QueryPayload::HuntTripForecast(sim_runtime::commands::HuntTripForecastQuery {
            faction_id: 0,
            band_id: 1,
            herd_id: "game_transport".to_string(),
            kit_id: "big_game".to_string(),
            party_workers: 3,
            floor: 0.25,
            preset_floors: vec![0.0, 0.5],
            // A plateau scan the transport tests never read; the reply is injected, not computed.
            max_party_workers: 0,
        })
    }

    /// Write one length-prefixed frame, exactly as the client's emit path does.
    fn write_frame(stream: &mut TcpStream, bytes: &[u8]) {
        stream
            .write_all(&(bytes.len() as u32).to_le_bytes())
            .expect("frame header");
        stream.write_all(bytes).expect("frame body");
        stream.flush().expect("frame flush");
    }

    /// Read one length-prefixed frame back off the socket.
    fn read_frame(stream: &mut TcpStream) -> Vec<u8> {
        let mut header = [0u8; 4];
        stream.read_exact(&mut header).expect("reply header");
        let len = u32::from_le_bytes(header) as usize;
        assert!(
            len <= MAX_PROTO_FRAME,
            "a reply frame respects the same bound the read path enforces"
        );
        let mut body = vec![0u8; len];
        stream.read_exact(&mut body).expect("reply body");
        body
    }

    /// **THE ROUND TRIP, over a real socket.** A query frame in, a reply frame out, on the *same*
    /// connection, correlated by `request_id`.
    ///
    /// It drives the actual `handle_proto_client` over a real `TcpStream` rather than exercising the
    /// codec, because what is being proved is the part a codec round trip cannot see: that the
    /// command socket carries a second direction at all. The reply path is a `try_clone`d handle and
    /// a writer thread, and neither exists in a codec test.
    ///
    /// The answer is **injected rather than computed** — the sim's half is tested in
    /// `core_sim::forecast_query`, and binding this test to a world would make a transport failure
    /// and a forecast failure look the same.
    #[test]
    fn a_query_is_answered_on_the_same_socket() {
        const REQUEST_ID: u64 = 4242;

        let listener = TcpListener::bind("127.0.0.1:0").expect("an ephemeral port");
        let addr = listener.local_addr().expect("the bound address");
        let (command_tx, command_rx) = unbounded::<Command>();

        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("the test client connects");
            handle_proto_client(stream, command_tx);
        });

        let mut client = TcpStream::connect(addr).expect("connect to the listener");
        write_frame(
            &mut client,
            &a_query_envelope(REQUEST_ID)
                .encode_to_vec()
                .expect("the query envelope encodes"),
        );

        // The loop's side: the decoded command carries the request id AND a way back.
        let command = command_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the query reaches the command channel");
        let Command::Query {
            request_id,
            query,
            reply,
        } = command
        else {
            panic!("a query envelope must decode to Command::Query");
        };
        assert_eq!(request_id, REQUEST_ID);
        assert!(
            matches!(query, QueryPayload::HuntTripForecast(_)),
            "the oneof survives the wire"
        );

        reply
            .send(QueryReplyEnvelope {
                request_id,
                reply: QueryReply::Error(query_error::NO_ACTIVE_WORLD.to_string()),
            })
            .expect("the writer thread is listening");

        let answer =
            QueryReplyEnvelope::decode(&read_frame(&mut client)).expect("the reply frame decodes");
        assert_eq!(
            answer.request_id, REQUEST_ID,
            "the reply is correlated to the query that asked for it"
        );
        assert_eq!(
            answer.reply,
            QueryReply::Error(query_error::NO_ACTIVE_WORLD.to_string())
        );

        // Closing the client ends the read loop, which drops the reply sender and ends the writer.
        drop(client);
        server.join().expect("the connection handler exits cleanly");
    }

    /// **Two queries on one connection stay distinguishable, whatever order they are answered in.**
    /// The reply channel is per *connection* and shared by every question asked on it, so a client
    /// with two sheets open must be able to tell the answers apart — which is the whole job of
    /// `request_id`. Answering the second one first is what makes that a real assertion rather than
    /// an accident of ordering.
    #[test]
    fn replies_stay_correlated_when_queries_are_pipelined() {
        const FIRST: u64 = 1;
        const SECOND: u64 = 2;

        let listener = TcpListener::bind("127.0.0.1:0").expect("an ephemeral port");
        let addr = listener.local_addr().expect("the bound address");
        let (command_tx, command_rx) = unbounded::<Command>();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("the test client connects");
            handle_proto_client(stream, command_tx);
        });

        let mut client = TcpStream::connect(addr).expect("connect to the listener");
        for id in [FIRST, SECOND] {
            write_frame(
                &mut client,
                &a_query_envelope(id).encode_to_vec().expect("encodes"),
            );
        }

        let mut pending = Vec::new();
        for _ in 0..2 {
            let command = command_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("both queries arrive");
            let Command::Query {
                request_id, reply, ..
            } = command
            else {
                panic!("expected a query");
            };
            pending.push((request_id, reply));
        }
        assert_eq!(pending[0].0, FIRST);
        assert_eq!(pending[1].0, SECOND);

        for (request_id, reply) in pending.into_iter().rev() {
            reply
                .send(QueryReplyEnvelope {
                    request_id,
                    reply: QueryReply::Error(format!("token_{request_id}")),
                })
                .expect("the writer thread is listening");
        }

        let first_out = QueryReplyEnvelope::decode(&read_frame(&mut client)).expect("decodes");
        let second_out = QueryReplyEnvelope::decode(&read_frame(&mut client)).expect("decodes");
        assert_eq!(
            first_out.request_id, SECOND,
            "the answers come back in the order they were ANSWERED, not asked"
        );
        assert_eq!(second_out.request_id, FIRST);
        assert_eq!(
            first_out.reply,
            QueryReply::Error(format!("token_{SECOND}")),
            "each reply carries its OWN answer, not the other's"
        );

        drop(client);
        server.join().expect("the connection handler exits cleanly");
    }

    /// **The idle server refuses a query with its own token.** `no_active_world` is unreachable
    /// through `answer_forecast_query` — there is no world to ask — so it is asserted on the gate
    /// that produces it.
    ///
    /// The `World::new()` is deliberately **empty**: it carries none of the config handles an answer
    /// reads, so a gate that fell through to the answering path would panic on a missing resource
    /// rather than quietly return some other token.
    #[test]
    fn an_idle_server_refuses_a_query_without_touching_the_world() {
        let mut world = bevy::prelude::World::new();
        let reply = answer_query(false, &mut world, &a_hunt_query());
        assert_eq!(
            reply,
            QueryReply::Error(query_error::NO_ACTIVE_WORLD.to_string())
        );
    }

    /// **A query is never written to the replay log.** It mutates nothing, so a logged one would
    /// make a replay re-answer a question into a reply channel whose connection is long gone.
    ///
    /// Asserted against the log's own exclusion predicate rather than against the dispatch arm: the
    /// arm is the *current* reason a query never reaches the log, and the predicate is the durable
    /// one. A refactor that routed queries through the generic arm must still not log them.
    #[test]
    fn a_query_is_not_replayable() {
        let (reply, _reply_rx) = unbounded::<QueryReplyEnvelope>();
        let query = Command::Query {
            request_id: 1,
            query: a_hunt_query(),
            reply,
        };

        assert!(
            !is_replayable(&query),
            "a query must not enter the replay log — replaying one cannot reproduce anything"
        );
        assert!(
            is_replayable(&Command::Turn(1)),
            "the predicate must still admit an ordinary command, or it proves nothing"
        );
    }

    // --- the bench crew is not idle -------------------------------------------------------------

    /// An **ungated** recipe — nothing in `requires_knowledge` a fresh faction lacks — so the bench
    /// fixtures below are about the crew, not about a refusal.
    const BENCH_IDLE_RECIPE: &str = "sled";
    /// The crew the fixtures stand at the bench. Small enough that any campaign band has the hands,
    /// large enough that a lost subtraction is unmistakable in the assertion.
    const BENCH_IDLE_CREW: u32 = 3;

    /// `PopulationCohortState.idleWorkers` for one band, read off the **encoded** envelope. The claim
    /// is about what the client is told, and a field that never reached the codec still satisfies an
    /// in-process check.
    fn published_idle_workers(app: &mut bevy::prelude::App, band: Entity) -> u32 {
        use core_sim::{recapture_snapshot_in_place, SnapshotHistory};
        use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

        recapture_snapshot_in_place(&mut app.world);
        let snapshot = app
            .world
            .resource::<SnapshotHistory>()
            .latest_entry()
            .expect("a snapshot was captured")
            .snapshot;
        let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
        let envelope = fb::root_as_envelope(&bytes).expect("the snapshot encodes");
        envelope
            .payload_as_snapshot()
            .expect("the envelope carries a snapshot")
            .population()
            .and_then(|section| section.populations())
            .expect("the population section carries the cohort list")
            .iter()
            .find(|cohort| cohort.entity() == band.to_bits())
            .expect("the band is on the wire")
            .idleWorkers()
    }

    /// The world's first resident band, the one a band-less command picks — see
    /// `select_starting_band`'s default picker.
    fn first_resident_band(app: &mut bevy::prelude::App) -> Entity {
        let mut query = app
            .world
            .query_filtered::<Entity, (With<PopulationCohort>, With<ResidentBand>)>();
        query
            .iter(&app.world)
            .next()
            .expect("worldgen spawned a resident band")
    }

    /// **A crew at the bench is published as BUSY** — the same hands `assign_labor` refuses to send
    /// anywhere.
    ///
    /// The published field was `working_age − assigned`, and a bench crew is a number on
    /// [`BandBench`] rather than a `LaborTarget`, so it was never in `assigned`: every "n idle of m"
    /// readout in the game counted the bench's hands as free, in the *reassuring* direction.
    #[test]
    fn a_bench_crew_is_missing_from_the_published_idle_count() {
        let mut app = build_test_app();
        app.update();
        let faction = FactionId(0);
        let band = first_resident_band(&mut app);
        let idle_before = published_idle_workers(&mut app, band);
        assert!(
            idle_before >= BENCH_IDLE_CREW,
            "the fixture band must have the hands to staff the bench at all"
        );

        handle_set_bench(&mut app, faction, None, BENCH_IDLE_RECIPE, BENCH_IDLE_CREW);
        // Liveness: the crew really is standing there. Without this the assertion below passes on a
        // bench that silently refused the job — and on a sim that stopped publishing idle at all.
        assert_eq!(
            app.world
                .get::<BandBench>(band)
                .map(|bench| bench.workers)
                .unwrap_or(0),
            BENCH_IDLE_CREW,
            "the command must have put the crew on the bench"
        );

        assert_eq!(
            published_idle_workers(&mut app, band),
            idle_before - BENCH_IDLE_CREW,
            "the bench's crew must leave the published idle count"
        );

        // …and handing them back restores it, so the subtraction is a live one rather than a band
        // that simply lost workers.
        handle_clear_bench(&mut app, faction, None);
        assert_eq!(
            published_idle_workers(&mut app, band),
            idle_before,
            "clearing the job returns the crew to the idle pool"
        );
    }

    /// **The published idle count is exactly what the command path will let the player assign.**
    ///
    /// The liveness half of the pair above: a sim that published `0` idle forever would satisfy
    /// "fewer with a bench crew" and fail here. Asserted by asking `assign_labor` for *more* hands
    /// than exist and reading what it applied — the clamp is `BandWorkforce::assignable()` minus the
    /// other assignments, which is the same arithmetic `idle()` reports.
    #[test]
    fn the_published_idle_count_is_what_assign_labor_will_staff() {
        let mut app = build_test_app();
        app.update();
        let faction = FactionId(0);
        let band = first_resident_band(&mut app);
        handle_set_bench(&mut app, faction, None, BENCH_IDLE_RECIPE, BENCH_IDLE_CREW);
        let published = published_idle_workers(&mut app, band);
        assert!(
            published > 0,
            "the fixture must leave hands free, or the equality below proves nothing"
        );

        // A band-wide role, so the answer is about the head-count and not about a source's range.
        handle_assign_labor(
            &mut app,
            faction,
            None,
            "scout".to_string(),
            published + BENCH_IDLE_CREW,
            None,
            None,
            None,
            None,
            None,
            None,
            Vec::new(),
        );

        assert_eq!(
            app.world
                .get::<LaborAllocation>(band)
                .map(|allocation| allocation.assigned_total())
                .unwrap_or(0),
            published,
            "the command staffs exactly the workers the wire called idle — no more, and not fewer"
        );
        assert_eq!(
            published_idle_workers(&mut app, band),
            0,
            "and with them staffed the band publishes nobody idle"
        );
    }

    /// A **second** ungated recipe, so a swap is a real change of job rather than a re-set of the
    /// same one.
    const BENCH_SWAP_RECIPE: &str = "baskets";

    /// What is actually standing at a band's bench.
    fn bench_crew(app: &bevy::prelude::App, band: Entity) -> u32 {
        app.world
            .get::<BandBench>(band)
            .map(|bench| bench.workers)
            .unwrap_or(0)
    }

    /// **Naming no crew recruits nobody; naming one is obeyed to the head.**
    ///
    /// A pairing, because every other bench fixture passes an explicit crew — which is the one call
    /// shape the client never makes. **The player staffs the bench and the sim never does**: labor
    /// is the scarce currency and splitting the band is the game's turn-to-turn decision, so an
    /// idle bench stages at zero and waits for the stepper. The second half is what keeps that from
    /// meaning *"a named crew is ignored too"*.
    #[test]
    fn a_set_bench_with_no_crew_named_recruits_nobody() {
        let mut app = build_test_app();
        app.update();
        let faction = FactionId(0);
        let band = first_resident_band(&mut app);
        let idle_before = published_idle_workers(&mut app, band);
        assert!(
            idle_before > BENCH_IDLE_CREW,
            "the fixture band must have more hands than the explicit crew, or the two halves \
             below cannot be told apart"
        );

        handle_set_bench(
            &mut app,
            faction,
            None,
            BENCH_IDLE_RECIPE,
            BENCH_CREW_UNSPECIFIED,
        );
        assert_eq!(
            bench_crew(&app, band),
            0,
            "a set_bench that names no crew stages the recipe with nobody on it — the sim does not \
             pick how many hands stop hunting"
        );
        assert_eq!(
            published_idle_workers(&mut app, band),
            idle_before,
            "…so the band's idle count is untouched: every hand is still free to be spent elsewhere"
        );

        handle_set_bench(&mut app, faction, None, BENCH_IDLE_RECIPE, BENCH_IDLE_CREW);
        assert_eq!(
            bench_crew(&app, band),
            BENCH_IDLE_CREW,
            "a set_bench that DOES name a crew applies exactly that crew — an absent number means \
             leave the crew alone, not ignore the one that is there"
        );
    }

    /// **Swapping the job on a running bench keeps the crew standing there.**
    ///
    /// `BandBench::set_job` overwrites `workers`, so a swap that applied the proto's `0` dismissed a
    /// crew the player never asked to send home — the exact case `BandWorkforce::benchable()` (pool
    /// − assigned, deliberately *not* netting the bench) exists to preserve.
    #[test]
    fn swapping_the_job_on_a_running_bench_keeps_its_crew() {
        let mut app = build_test_app();
        app.update();
        let faction = FactionId(0);
        let band = first_resident_band(&mut app);

        handle_set_bench(&mut app, faction, None, BENCH_IDLE_RECIPE, BENCH_IDLE_CREW);
        assert_eq!(
            bench_crew(&app, band),
            BENCH_IDLE_CREW,
            "the fixture must have a crew at the bench, or the swap below proves nothing"
        );

        handle_set_bench(
            &mut app,
            faction,
            None,
            BENCH_SWAP_RECIPE,
            BENCH_CREW_UNSPECIFIED,
        );
        assert_eq!(
            app.world
                .get::<BandBench>(band)
                .and_then(|bench| bench.recipe_id.clone())
                .unwrap_or_default(),
            BENCH_SWAP_RECIPE,
            "the swap must actually have changed the job"
        );
        assert_eq!(
            bench_crew(&app, band),
            BENCH_IDLE_CREW,
            "the crew already at the bench stays put across the swap — an absent number is not an \
             order to send them home"
        );
    }

    /// A projection whose food half is honestly zero and whose whole payload is material — the wolf
    /// shape, built directly so the assertion is about the *sentence* rather than about a roster.
    fn inedible_denial_forecast(
        delivered_material: Vec<core_sim::MaterialPayoff>,
    ) -> core_sim::DenialForecast {
        core_sim::DenialForecast {
            turns_to_collapse: Some(4),
            turns_to_collapse_low: Some(3),
            turns_to_collapse_high: Some(6),
            outcome: core_sim::DenialOutcome::PastRecovery,
            animals_killed: 9,
            delivered_food: 0.0,
            wasted_food: 0.0,
            delivered_material,
        }
    }

    /// **The launch ack states the materials the same forecast promises.** The client's take line
    /// reads `DenialForecast::delivered_material` off this very forecast, so a server sentence that
    /// said *"nothing worth hauling"* beside it would contradict the client about one raid.
    ///
    /// Paired with the genuinely-empty case, because *"always name the hides"* would otherwise be
    /// satisfiable by deleting the fallback — and a raid that brings back neither meat nor material
    /// must still say so rather than print a `~0.0`.
    #[test]
    fn an_inedible_raids_ack_names_the_materials_its_forecast_promises() {
        let hauling = inedible_denial_forecast(vec![
            core_sim::MaterialPayoff {
                material: "hide".to_string(),
                amount: 3.2,
            },
            core_sim::MaterialPayoff {
                material: "bone".to_string(),
                amount: 0.44,
            },
        ]);
        let line = describe_denial_ledger(&hauling);
        assert!(
            line.contains("hide") && line.contains("bone"),
            "both promised materials belong on the line, never summed — got {line}"
        );
        assert!(
            line.contains("3.20") && line.contains("0.44"),
            "the amounts print finely enough to show a sub-unit pack — got {line}"
        );
        assert!(
            !line.contains("food"),
            "an inedible quarry states no food clause at all rather than a fabricated ~0.0 — got \
             {line}"
        );

        let barren = inedible_denial_forecast(Vec::new());
        assert_eq!(
            describe_denial_ledger(&barren),
            "nothing worth hauling from this quarry",
            "a raid that really does bring nothing back still says so"
        );
    }

    // ------------------------------------------------------------------------------------------
    // The trade expedition — the connection primitive's first rider (arc #527, issue #517)
    // ------------------------------------------------------------------------------------------

    /// The exported floats are `f32` sums of `Scalar`-quantized takes; a few ULPs of slack, no more.
    const TRADE_EPSILON: f32 = 0.01;
    /// Working-age people the fixture band is stocked with, so a split leaves two real bands and
    /// there is a comfortable party to draw off either.
    const TRADE_FIXTURE_WORKERS: f32 = 20.0;
    /// Workers the shipment party is sent with. With the shipped `trade.per_worker_carry` of 6.0
    /// this is a 12-unit pack — big enough to hold `TRADE_CARGO_FOOD` and small enough that
    /// `OVER_CAP_FOOD` genuinely does not fit.
    const TRADE_PARTY: u32 = 2;
    /// Workers the fixture hands the second band. Over `min_founding_workers` on any seed.
    const TRADE_SPLIT_WORKERS: u32 = 5;
    /// Food the fixture band is stocked with — far more than any shipment below asks for, so a
    /// refusal is never a refusal about availability unless it says so.
    const TRADE_FIXTURE_LARDER: f32 = 400.0;
    /// A shipment that fits: under `TRADE_PARTY × trade.per_worker_carry`.
    const TRADE_CARGO_FOOD: f32 = 10.0;
    /// A shipment that does not: over the same cap, and comfortably inside the larder, so the only
    /// thing that can refuse it is the pack.
    const OVER_CAP_FOOD: f32 = 99.0;

    /// Two co-located resident bands that have **found each other** — one real turn of the sight
    /// sweep forms the tie the trade verb is gated on, rather than a hand-written ledger entry.
    /// Returns `(sender entity, destination id, faction)`.
    fn two_bands_that_know_each_other(
        app: &mut bevy::prelude::App,
    ) -> (Entity, core_sim::BandId, FactionId) {
        let (parent, faction) = {
            let mut query = app
                .world
                .query_filtered::<(Entity, &PopulationCohort), With<ResidentBand>>();
            let (entity, cohort) = query
                .iter(&app.world)
                .next()
                .expect("the campaign spawns a resident band");
            (entity, cohort.faction)
        };
        stock_trade_band(app, parent);
        let settle = core_sim::SettleConfig {
            min_founding_workers: 1,
            parent_min_workers: 0,
        };
        let split =
            core_sim::split_band_from_parent(&mut app.world, parent, TRADE_SPLIT_WORKERS, &settle)
                .expect("a stocked parent can split");
        // A real turn: the sight sweep finds the band standing beside this one and records the tie.
        app.update();
        // Stocked AFTER the turn, so the people's own meal cannot eat into what the shipment needs.
        stock_trade_band(app, parent);
        (parent, split.band, faction)
    }

    fn stock_trade_band(app: &mut bevy::prelude::App, band: Entity) {
        let mut cohort = app
            .world
            .get_mut::<PopulationCohort>(band)
            .expect("the band exists");
        cohort.working = Scalar::from_f32(TRADE_FIXTURE_WORKERS);
        cohort.sync_size();
        cohort
            .stores
            .set(FOOD, scalar_from_f32(TRADE_FIXTURE_LARDER));
    }

    fn food_cargo(amount: f32) -> Vec<TradeCargoItem> {
        vec![TradeCargoItem {
            id: sim_runtime::FOOD_CARGO_KEY.to_string(),
            is_material: false,
            amount,
        }]
    }

    fn launched_party(app: &mut bevy::prelude::App) -> Option<Entity> {
        let mut query = app.world.query_filtered::<Entity, With<Expedition>>();
        query.iter(&app.world).next()
    }

    fn last_expedition_detail(app: &bevy::prelude::App) -> String {
        app.world
            .resource::<CommandEventLog>()
            .iter()
            .filter(|entry| matches!(entry.kind, CommandEventKind::ExpeditionSent))
            .filter_map(|entry| entry.detail.clone())
            .last()
            .unwrap_or_default()
    }

    fn band_food(app: &bevy::prelude::App, band: Entity) -> f32 {
        app.world
            .get::<PopulationCohort>(band)
            .expect("the band exists")
            .stores
            .get(FOOD)
            .to_f32()
    }

    /// **The arc's gate, both ways.** A shipment to a band this one has never met is refused; the
    /// same order to a band it *has* met launches. Paired deliberately — a gate asserted only in the
    /// refusing direction passes on a verb that refuses everything.
    #[test]
    fn a_shipment_needs_a_live_tie_and_launches_once_there_is_one() {
        let mut app = build_world_app();
        let (sender, destination, faction) = two_bands_that_know_each_other(&mut app);
        let sender_id = app.world.get::<core_sim::BandId>(sender).expect("an id").0;

        // A band nobody has met: an id the ledger holds no edge for at all.
        let stranger = app.world.resource_mut::<BandIdAllocator>().allocate().0;
        handle_send_trade_expedition(
            &mut app,
            faction,
            Some(sender_id),
            TRADE_PARTY,
            stranger,
            food_cargo(TRADE_CARGO_FOOD),
            None,
        );
        assert!(
            launched_party(&mut app).is_none(),
            "a shipment to a band this one has never met must not leave"
        );

        handle_send_trade_expedition(
            &mut app,
            faction,
            Some(sender_id),
            TRADE_PARTY,
            destination.0,
            food_cargo(TRADE_CARGO_FOOD),
            None,
        );
        let party = launched_party(&mut app).expect("a live tie lets the shipment leave");
        let mission = app
            .world
            .get::<Expedition>(party)
            .expect("the party carries its mission")
            .mission
            .clone();
        assert_eq!(
            mission.destination_band(),
            Some(destination),
            "the party is bound for the band the order named"
        );
    }

    /// **The debit is exactly the manifest plus the walk's larder.** The sending band loses what it
    /// handed over — and the party is holding the cargo in a store of its own, not in its pack.
    #[test]
    fn the_sending_band_is_debited_by_exactly_what_the_party_carries() {
        let mut app = build_world_app();
        let (sender, destination, faction) = two_bands_that_know_each_other(&mut app);
        let sender_id = app.world.get::<core_sim::BandId>(sender).expect("an id").0;
        let before = band_food(&app, sender);

        handle_send_trade_expedition(
            &mut app,
            faction,
            Some(sender_id),
            TRADE_PARTY,
            destination.0,
            food_cargo(TRADE_CARGO_FOOD),
            None,
        );
        let party = launched_party(&mut app).expect("the shipment left");
        let (cargo, pack) = {
            let expedition = app.world.get::<Expedition>(party).expect("the party");
            let cohort = app.world.get::<PopulationCohort>(party).expect("the party");
            (
                expedition.cargo.get(FOOD).to_f32(),
                cohort.stores.get(FOOD).to_f32(),
            )
        };
        let after = band_food(&app, sender);

        assert!(
            (cargo - TRADE_CARGO_FOOD).abs() < TRADE_EPSILON,
            "the party carries the manifest exactly: {cargo} vs {TRADE_CARGO_FOOD}"
        );
        assert!(
            cargo > 0.0,
            "the liveness half — a shipment that carried nothing would pass every check above"
        );
        assert!(
            (before - after - cargo - pack).abs() < TRADE_EPSILON,
            "the band's debit is the cargo plus the walk's larder: before={before} after={after} \
             cargo={cargo} pack={pack}"
        );
    }

    /// **Over the pack, refused — never clamped.** A player who asked for a shipment the party
    /// cannot carry gets a refusal and an untouched larder, not a quietly smaller shipment.
    #[test]
    fn a_shipment_over_the_carry_cap_is_refused_rather_than_clamped() {
        let mut app = build_world_app();
        let (sender, destination, faction) = two_bands_that_know_each_other(&mut app);
        let sender_id = app.world.get::<core_sim::BandId>(sender).expect("an id").0;
        let before = band_food(&app, sender);

        handle_send_trade_expedition(
            &mut app,
            faction,
            Some(sender_id),
            TRADE_PARTY,
            destination.0,
            food_cargo(OVER_CAP_FOOD),
            None,
        );

        assert!(
            launched_party(&mut app).is_none(),
            "a shipment heavier than the party's packs must not leave"
        );
        let after = band_food(&app, sender);
        assert!(
            (before - after).abs() < TRADE_EPSILON,
            "a refused shipment leaves the band exactly as it stood: {before} -> {after}"
        );
        let detail = last_expedition_detail(&app);
        assert!(
            detail.contains("carry"),
            "the refusal says WHY the pack refused it — got {detail}"
        );
    }

    /// **Empty, unknown and not-held all fail closed**, on the same untouched-larder rule: a
    /// shipment is refused with a reason, never trimmed to what the band happens to have.
    #[test]
    fn an_empty_unknown_or_unheld_shipment_is_refused() {
        for cargo in [
            Vec::new(),
            // A material id `materials.json` does not author at all.
            vec![TradeCargoItem {
                id: "unobtanium".to_string(),
                is_material: true,
                amount: 1.0,
            }],
            // A real material the fixture band holds none of.
            vec![TradeCargoItem {
                id: "hide".to_string(),
                is_material: true,
                amount: 1.0,
            }],
            // A commodity key that is not the larder's.
            vec![TradeCargoItem {
                id: "moonlight".to_string(),
                is_material: false,
                amount: 1.0,
            }],
        ] {
            let mut app = build_world_app();
            let (sender, destination, faction) = two_bands_that_know_each_other(&mut app);
            let sender_id = app.world.get::<core_sim::BandId>(sender).expect("an id").0;
            let before = band_food(&app, sender);
            handle_send_trade_expedition(
                &mut app,
                faction,
                Some(sender_id),
                TRADE_PARTY,
                destination.0,
                cargo.clone(),
                None,
            );
            assert!(
                launched_party(&mut app).is_none(),
                "{cargo:?} must be refused — empty, unknown and not-held all fail closed"
            );
            assert!(
                (before - band_food(&app, sender)).abs() < TRADE_EPSILON,
                "{cargo:?} was refused, so the band must stand exactly as it did"
            );
        }
    }

    /// **A destination that is not a band is refused**, and so is one that is an *expedition* — a
    /// detached party is not a people you can deliver to.
    #[test]
    fn a_destination_that_is_not_a_resident_band_is_refused() {
        let mut app = build_world_app();
        let (sender, destination, faction) = two_bands_that_know_each_other(&mut app);
        let sender_id = app.world.get::<core_sim::BandId>(sender).expect("an id").0;

        // Launch one shipment so a detached party (which carries a `BandId` of its own) exists.
        handle_send_trade_expedition(
            &mut app,
            faction,
            Some(sender_id),
            TRADE_PARTY,
            destination.0,
            food_cargo(TRADE_CARGO_FOOD),
            None,
        );
        let party = launched_party(&mut app).expect("the shipment left");
        let party_band = app
            .world
            .get::<core_sim::BandId>(party)
            .expect("a detached party is a band in its own right")
            .0;

        handle_send_trade_expedition(
            &mut app,
            faction,
            Some(sender_id),
            TRADE_PARTY,
            party_band,
            food_cargo(TRADE_CARGO_FOOD),
            None,
        );
        let parties = {
            let mut query = app.world.query_filtered::<Entity, With<Expedition>>();
            query.iter(&app.world).count()
        };
        assert_eq!(
            parties, 1,
            "a shipment addressed to another party must be refused, not launched"
        );
    }

    /// **A real launch publishes NO destination name, and that is the fix rather than the gap.**
    ///
    /// Bands have no names in this game, so the sim has nothing to put on
    /// `expeditionDestinationName` and declines to guess. This first shipped resolving it through
    /// `starting_unit_label` -> `StartingUnit.kind`, which is the unit **archetype** — the same
    /// string for every seeded band — so every in-flight party's row read *"Bound for
    /// BandForager"*, and disagreed with the positional label ("Band 2") the rest of the HUD gives
    /// that same band. A wrong name is worse than none: none has a fallback.
    ///
    /// **The unit kind is asserted absent by NAME**, not merely "the string is empty" — that is what
    /// makes this a regression test for the specific wrong answer rather than a restatement of the
    /// field's default. The band's id is asserted present beside it, because a client renders its
    /// own label by joining on that key and the row is useless without it.
    #[test]
    fn a_real_launch_publishes_no_destination_name_rather_than_a_unit_kind() {
        let mut app = build_world_app();
        let (sender, destination, faction) = two_bands_that_know_each_other(&mut app);
        let sender_id = app.world.get::<core_sim::BandId>(sender).expect("an id").0;
        // The archetype the broken version published — read off the world rather than spelled out,
        // so the assertion tracks the shipped start profile instead of a copy of it.
        let unit_kind = starting_unit_label(&app, sender);
        assert!(
            !unit_kind.is_empty(),
            "the fixture rests on the bands carrying a `StartingUnit.kind` at all"
        );

        handle_send_trade_expedition(
            &mut app,
            faction,
            Some(sender_id),
            TRADE_PARTY,
            destination.0,
            food_cargo(TRADE_CARGO_FOOD),
            None,
        );
        let party = launched_party(&mut app).expect("the shipment left");
        let mission = app
            .world
            .get::<Expedition>(party)
            .expect("the party carries its mission")
            .mission
            .clone();

        assert_eq!(
            mission.destination_name(),
            "",
            "a real launch has no name to give, and empty means NO NAME — the client falls back to \
             the label it uses for that band everywhere else"
        );
        assert_ne!(
            mission.destination_name(),
            unit_kind,
            "and it must never be the unit ARCHETYPE ('{unit_kind}'), which is the same string for \
             every band in the game"
        );
        assert_eq!(
            mission.destination_band(),
            Some(destination),
            "the KEY is still there — it is what a client joins its own label on"
        );
        // The sim's own feed prose still names something: `destination_display` falls back to the
        // band's id, which is the honest floor for a line the sim has to write on its own. It is
        // deliberately NOT what the wire carries.
        assert!(
            mission
                .destination_display()
                .contains(&destination.0.to_string()),
            "the feed line names the band by id when it has no name: {}",
            mission.destination_display()
        );
    }

    /// **One band's published ranks, read off the ENCODED envelope** — `(kind, priority)` per labor
    /// row, through the client's own accessor chain, because a field that never reached the codec
    /// still passes an in-process assertion.
    fn published_ranks(
        app: &mut bevy::prelude::App,
        band_id: u64,
    ) -> Vec<(
        String,
        shadow_scale_flatbuffers::generated::shadow_scale::sim::SourcePriority,
        u32,
    )> {
        use core_sim::{recapture_snapshot_in_place, SnapshotHistory};
        use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

        recapture_snapshot_in_place(&mut app.world);
        let snapshot = app
            .world
            .resource::<SnapshotHistory>()
            .latest_entry()
            .expect("a snapshot was captured")
            .snapshot;
        let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
        let envelope = fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes");
        envelope
            .payload_as_snapshot()
            .expect("the envelope carries a snapshot")
            .population()
            .and_then(|section| section.populations())
            .expect("the population section carries the band list")
            .iter()
            .find(|cohort| cohort.bandId() == band_id)
            .expect("the fixture band is published")
            .laborAssignments()
            .into_iter()
            .flatten()
            .map(|row| {
                (
                    row.kind().unwrap_or_default().to_string(),
                    row.priority(),
                    row.workers(),
                )
            })
            .collect()
    }

    /// **`work_priority` MARKS THE NAMED BAND'S ROW, AND THE MARK IS LIVE ON THE WIRE**
    /// (`docs/plan_standing_upkeep.md` §4.9 item 9b).
    ///
    /// Four things in one run, because each fails silently on its own: the default is published, the
    /// command changes it on its **own recapture** (no turn advanced — the rule `build_order` already
    /// ships on), the mark **survives a re-assignment** of the row's crew, and an unknown level is
    /// refused **by name** rather than landing on the default.
    #[test]
    fn work_priority_marks_a_row_lives_on_the_wire_and_survives_an_edit() {
        use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

        let mut app = build_world_app();
        let faction = FactionId(0);
        let tile = UVec2::new(3, 3);
        let band = spawn_resident_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile,
                floor: DEFAULT_ESCAPEMENT_FLOOR,
                species: None,
                take_species: TakeSelection::EVERYTHING,
            },
        );
        app.world.entity_mut(band).insert(BandId(FIXTURE_BAND_ID));
        // The ground has to admit a worked stand, or the `assign_labor` below is refused and the
        // survives-an-edit assertion never sees an edit at all.
        {
            let entity = app
                .world
                .resource::<TileRegistry>()
                .index(tile.x, tile.y)
                .expect("the generated world carries this tile");
            app.world
                .get_mut::<Tile>(entity)
                .expect("a registered tile carries its ground")
                .terrain = SOURCE_BIOME;
        }
        seed_thriving_patch(&mut app, tile);

        // **The rank AND the crew, together.** The crew is what makes the survives-an-edit
        // assertion an assertion at all: an `assign_labor` the world refused would leave the mark
        // standing for the wrong reason, and a rank-only reading cannot tell the two apart.
        let forage_row = |app: &mut bevy::prelude::App| {
            published_ranks(app, FIXTURE_BAND_ID)
                .into_iter()
                .find(|(kind, _, _)| kind == "forage")
                .map(|(_, rank, workers)| (rank, workers))
        };

        let (_, crew_before) = forage_row(&mut app).expect("the fixture band publishes its row");
        assert_eq!(
            forage_row(&mut app).map(|(rank, _)| rank),
            Some(fb::SourcePriority::Normal),
            "an unmarked row publishes the default"
        );

        handle_work_priority(
            &mut app,
            faction,
            FIXTURE_BAND_ID,
            patch_source(tile),
            "LOW".to_string(),
        );
        assert_eq!(
            forage_row(&mut app).map(|(rank, _)| rank),
            Some(fb::SourcePriority::Low),
            "the mark arrives on the command's own recapture, case-folded, with no turn advanced"
        );

        // The `−`/`+` the player presses next. `set_assignment` re-pushes the row to the END of the
        // vector, which is the whole reason the rank is a stated value and not a list position.
        handle_assign_labor(
            &mut app,
            faction,
            Some(FIXTURE_BAND_ID),
            "forage".to_string(),
            crew_before + 1,
            Some(tile.x),
            Some(tile.y),
            None,
            None,
            None,
            None,
            Vec::new(),
        );
        assert_eq!(
            forage_row(&mut app),
            Some((fb::SourcePriority::Low, crew_before + 1)),
            "the edit LANDED and the mark survived it"
        );

        // An unknown level is refused by name — it must NOT silently land on the default, which is
        // the one value that would look like it worked.
        handle_work_priority(
            &mut app,
            faction,
            FIXTURE_BAND_ID,
            patch_source(tile),
            "sideways".to_string(),
        );
        assert_eq!(
            forage_row(&mut app).map(|(rank, _)| rank),
            Some(fb::SourcePriority::Low),
            "a level the sim does not know changes nothing"
        );
    }

    /// **One band's published `buildQueue`, read off the ENCODED envelope** — as `(x, y)` pairs,
    /// since these fixtures queue patches. Read through the client's own accessor chain because a
    /// field that never reached the codec still passes an in-process assertion.
    fn published_build_queue(app: &mut bevy::prelude::App, band_id: u64) -> Vec<(u32, u32)> {
        use core_sim::{recapture_snapshot_in_place, SnapshotHistory};
        use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

        recapture_snapshot_in_place(&mut app.world);
        let snapshot = app
            .world
            .resource::<SnapshotHistory>()
            .latest_entry()
            .expect("a snapshot was captured")
            .snapshot;
        let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
        let envelope = fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes");
        envelope
            .payload_as_snapshot()
            .expect("the envelope carries a snapshot")
            .population()
            .and_then(|section| section.populations())
            .expect("the population section carries the band list")
            .iter()
            .find(|cohort| cohort.bandId() == band_id)
            .expect("the fixture band is published")
            .buildQueue()
            .map(|queue| {
                queue
                    .iter()
                    .map(|entry| (entry.targetX(), entry.targetY()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// **THE PUBLISHED QUEUE IS CAPTURED LIVE, so a re-order lands on the COMMAND'S OWN FRAME**
    /// (`docs/plan_standing_upkeep.md` §4.9 item 9a).
    ///
    /// `build_order` mutates the allocation at command time and the server re-captures after every
    /// dispatched command, so the wire carries the new order **without a turn being advanced**. That
    /// is what retires the client's optimistic ordering overlay — an overlay is a second ordering
    /// beside the wire's, which is exactly the drift the field's "the rank is the index" rule
    /// forbids — so it is pinned here rather than assumed.
    ///
    /// The declaration and the withdrawal ride the same recapture and are asserted in the same run:
    /// all three writers of the queue must be live, or the overlay has to come back for the one that
    /// is not.
    #[test]
    fn the_published_build_queue_follows_a_command_without_a_turn() {
        let mut app = build_world_app();
        let faction = FactionId(0);
        grant_cultivation(&mut app, faction);

        // Three patches, declared in this order — deliberately NOT in coordinate order, so an
        // ordering derived from anything but the band's own queue is visible.
        let first = UVec2::new(3, 3);
        let second = UVec2::new(1, 1);
        let third = UVec2::new(2, 2);
        let band = spawn_resident_working_band(
            &mut app,
            faction,
            LaborTarget::Forage {
                tile: first,
                floor: DEFAULT_ESCAPEMENT_FLOOR,
                species: None,
                take_species: TakeSelection::EVERYTHING,
            },
        );
        app.world.entity_mut(band).insert(BandId(FIXTURE_BAND_ID));
        for tile in [first, second, third] {
            // The ground has to admit a tended crop, so pin the terrain the plant-web fixtures use
            // before seeding the stand on it — worldgen's own tile at these coordinates offers
            // whatever the map rolled, which is routinely a wild-harvest-only basket.
            let entity = app
                .world
                .resource::<TileRegistry>()
                .index(tile.x, tile.y)
                .expect("the generated world carries this tile");
            app.world
                .get_mut::<Tile>(entity)
                .expect("a registered tile carries its ground")
                .terrain = SOURCE_BIOME;
            seed_thriving_patch(&mut app, tile);
        }
        // The other two patches need take crews before a declaration can land on them: an entry
        // requires a row.
        for tile in [second, third] {
            handle_assign_labor(
                &mut app,
                faction,
                Some(FIXTURE_BAND_ID),
                "forage".to_string(),
                1,
                Some(tile.x),
                Some(tile.y),
                None,
                None,
                None,
                None,
                Vec::new(),
            );
        }

        // ① A DECLARATION is live: each `cultivate` appends, and the wire says so on its own frame.
        for (index, tile) in [first, second, third].iter().enumerate() {
            handle_cultivate(&mut app, faction, *tile);
            assert_eq!(
                published_build_queue(&mut app, FIXTURE_BAND_ID).len(),
                index + 1,
                "declaring {tile:?} must reach the wire without a turn being advanced"
            );
        }
        assert_eq!(
            published_build_queue(&mut app, FIXTURE_BAND_ID),
            vec![(first.x, first.y), (second.x, second.y), (third.x, third.y)],
            "the published order is the order the player declared them in"
        );

        // ② A RE-ORDER is live: send the tail to the head, and read the wire with no turn between.
        handle_build_order(&mut app, faction, FIXTURE_BAND_ID, patch_source(third), 0);
        assert_eq!(
            published_build_queue(&mut app, FIXTURE_BAND_ID),
            vec![(third.x, third.y), (first.x, first.y), (second.x, second.y)],
            "the re-order arrives on the command's own recapture — the client needs no optimistic \
             ordering overlay"
        );

        // ③ A WITHDRAWAL is live: `unqueue` drops the entry and the rest close up, same frame.
        handle_unqueue(&mut app, faction, patch_source(first));
        assert_eq!(
            published_build_queue(&mut app, FIXTURE_BAND_ID),
            vec![(third.x, third.y), (second.x, second.y)],
            "the withdrawal arrives on the command's own recapture too"
        );
    }
    // ---------------------------------------------------------------------------------------
    // ⛔ THE ROUTE BRANCH'S TWO TILE VERBS — `grade` and `pave`
    // ---------------------------------------------------------------------------------------

    /// The band id every road fixture below addresses. Far above anything worldgen allocates.
    const ROAD_BAND_ID: u64 = 9_811;

    /// A world with one band standing on `coord`, and a road registry to grade tiles in.
    fn road_world(coord: UVec2) -> (bevy::prelude::App, FactionId, Entity) {
        let mut app = build_test_app();
        let faction = FactionId(0);
        let tile = seed_tile_grid(&mut app, coord);
        let band = spawn_idle_band(&mut app, faction, tile);
        app.world.entity_mut(band).insert(BandId(ROAD_BAND_ID));
        (app, faction, band)
    }

    /// Seat a road on `coord` at the top of `rung`, with nobody keeping it — the ground a `grade`
    /// or a `pave` is issued against.
    fn seat_unkept_road(app: &mut bevy::prelude::App, coord: UVec2, rung: core_sim::RungKey) {
        let ladder = core_sim::LadderConfig::builtin();
        let (base, width) = core_sim::road_rung_span(rung, &ladder, core_sim::NEAR_ENOUGH_TO_KEEP);
        let mut roads = app.world.resource_mut::<core_sim::RoadRegistry>();
        roads
            .road_or_trail(coord, &ladder)
            .set_position(base + width, &ladder);
    }

    fn grant_roadbuilding(app: &mut bevy::prelude::App, faction: FactionId) {
        app.world
            .resource_mut::<DiscoveryProgressLedger>()
            .add_progress(
                faction,
                core_sim::ROADBUILDING_DISCOVERY_ID,
                scalar_from_f32(1.0),
            );
    }

    fn grant_paving(app: &mut bevy::prelude::App, faction: FactionId) {
        app.world
            .resource_mut::<DiscoveryProgressLedger>()
            .add_progress(faction, core_sim::PAVING_DISCOVERY_ID, scalar_from_f32(1.0));
    }

    fn road_failure_detail_contains(app: &bevy::prelude::App, needle: &str) -> bool {
        app.world.resource::<CommandEventLog>().iter().any(|entry| {
            matches!(entry.kind, CommandEventKind::Road)
                && entry
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.contains(needle))
        })
    }

    fn keeper_of(app: &bevy::prelude::App, coord: UVec2) -> Option<BandId> {
        app.world
            .resource::<core_sim::RoadRegistry>()
            .road(coord)
            .and_then(|road| road.keeper)
            .map(|keeper| keeper.band)
    }

    fn queued_road_tiles(app: &bevy::prelude::App, band: Entity) -> Vec<UVec2> {
        app.world
            .get::<LaborAllocation>(band)
            .expect("the band has an allocation")
            .build_queue
            .iter()
            .filter_map(|entry| match entry.source {
                BuildSource::Road(tile) => Some(tile),
                _ => None,
            })
            .collect()
    }

    /// ⛔ **`grade` IS REFUSED WITHOUT `roadbuilding` AND ACCEPTED WITH IT** — the knowledge gate,
    /// with its liveness half, so the refusal is not passing on a verb that never works.
    #[test]
    fn grade_is_refused_without_roadbuilding_and_accepted_with_it() {
        const COORD: UVec2 = UVec2::new(1, 1);

        let (mut ignorant, faction, band) = road_world(COORD);
        seat_unkept_road(&mut ignorant, COORD, core_sim::RungKey::RouteTrail);
        handle_road_verb(
            &mut ignorant,
            faction,
            ROAD_BAND_ID,
            COORD,
            Improvement::Grade,
        );
        assert!(
            road_failure_detail_contains(&ignorant, "not learned"),
            "a people who have not learned roadbuilding are told so by name"
        );
        assert_eq!(
            keeper_of(&ignorant, COORD),
            None,
            "and the tile is still nobody's job"
        );
        assert!(queued_road_tiles(&ignorant, band).is_empty());

        let (mut taught, faction, band) = road_world(COORD);
        seat_unkept_road(&mut taught, COORD, core_sim::RungKey::RouteTrail);
        grant_roadbuilding(&mut taught, faction);
        handle_road_verb(
            &mut taught,
            faction,
            ROAD_BAND_ID,
            COORD,
            Improvement::Grade,
        );
        assert_eq!(
            keeper_of(&taught, COORD),
            Some(BandId(ROAD_BAND_ID)),
            "with the lesson learned, `grade` makes the tile that band's job"
        );
        assert_eq!(
            queued_road_tiles(&taught, band),
            vec![COORD],
            "and appends the entry its builders will raise"
        );
    }

    /// ⛔ **`grade` IS REFUSED ON GROUND THAT IS NOT YET A TRAIL, AND `pave` ON ONE THAT IS NOT YET A
    /// DIRT ROAD** — the rung beneath has to stand, and each refusal names a different thing to fix.
    #[test]
    fn a_road_verb_is_refused_until_the_rung_beneath_it_stands() {
        const COORD: UVec2 = UVec2::new(1, 1);

        // Bare ground: there is no road at all.
        let (mut bare, faction, _) = road_world(COORD);
        grant_roadbuilding(&mut bare, faction);
        handle_road_verb(&mut bare, faction, ROAD_BAND_ID, COORD, Improvement::Grade);
        assert!(
            road_failure_detail_contains(&bare, "There is no road"),
            "bare ground is told there is nothing there to grade"
        );

        // A trail: `pave` cannot skip the dirt road.
        let (mut trail, faction, _) = road_world(COORD);
        seat_unkept_road(&mut trail, COORD, core_sim::RungKey::RouteTrail);
        grant_roadbuilding(&mut trail, faction);
        grant_paving(&mut trail, faction);
        handle_road_verb(&mut trail, faction, ROAD_BAND_ID, COORD, Improvement::Pave);
        assert!(
            road_failure_detail_contains(&trail, "must be a dirt road"),
            "a trail is told which rung is missing — not merely that the command failed"
        );
        assert_eq!(
            keeper_of(&trail, COORD),
            None,
            "and a refused verb takes on nothing"
        );

        // The liveness half: the same tile at a dirt road accepts the same `pave`.
        let (mut ready, faction, _) = road_world(COORD);
        seat_unkept_road(&mut ready, COORD, core_sim::RungKey::RouteDirtRoad);
        grant_paving(&mut ready, faction);
        handle_road_verb(&mut ready, faction, ROAD_BAND_ID, COORD, Improvement::Pave);
        assert_eq!(
            keeper_of(&ready, COORD),
            Some(BandId(ROAD_BAND_ID)),
            "a dirt road really can be paved — otherwise the refusals above pass on a dead verb"
        );
    }

    /// ⛔ **ONE KEEPER PER TILE: A SECOND BAND CANNOT BECOME A CO-PAYER OF A ROAD ANOTHER BAND
    /// KEEPS.** This is what makes *"several bands each pay a share"* unrepresentable rather than
    /// merely discouraged.
    #[test]
    fn a_second_band_cannot_take_on_a_tile_another_band_keeps() {
        const COORD: UVec2 = UVec2::new(1, 1);
        const OTHER_BAND_ID: u64 = 9_812;

        let (mut app, faction, _) = road_world(COORD);
        seat_unkept_road(&mut app, COORD, core_sim::RungKey::RouteTrail);
        grant_roadbuilding(&mut app, faction);
        handle_road_verb(&mut app, faction, ROAD_BAND_ID, COORD, Improvement::Grade);
        assert_eq!(keeper_of(&app, COORD), Some(BandId(ROAD_BAND_ID)));

        // A second band of the same people, standing on the same ground.
        let tile = app
            .world
            .resource::<TileRegistry>()
            .index(COORD.x, COORD.y)
            .expect("the fixture tile is on the map");
        let other = spawn_idle_band(&mut app, faction, tile);
        app.world.entity_mut(other).insert(BandId(OTHER_BAND_ID));

        handle_road_verb(&mut app, faction, OTHER_BAND_ID, COORD, Improvement::Grade);
        assert!(
            road_failure_detail_contains(&app, "already keeps the road"),
            "the second band is refused BY NAME — a road tile has exactly one keeper"
        );
        assert_eq!(
            keeper_of(&app, COORD),
            Some(BandId(ROAD_BAND_ID)),
            "and the first band's claim is untouched"
        );
        assert!(
            queued_road_tiles(&app, other).is_empty(),
            "the refused band queues nothing, so its builders never touch that tile"
        );
    }

    /// ⛔ **RE-ISSUING THE VERB ON A KEEPERLESS ROAD ADOPTS IT**, and that is deliberately not a
    /// second verb: adoption is the same act as building.
    ///
    /// The *"already at that rung"* refusal is therefore scoped to a road **this band already
    /// keeps** — a keeperless dirt road is a road to pick up, not a job already done. Both halves
    /// are here, because the adoption alone would pass on a build with no such refusal at all.
    #[test]
    fn re_issuing_grade_on_a_keeperless_road_adopts_it() {
        const COORD: UVec2 = UVec2::new(1, 1);

        let (mut app, faction, band) = road_world(COORD);
        // A whole dirt road that nobody keeps — what a band walking away from the game leaves.
        seat_unkept_road(&mut app, COORD, core_sim::RungKey::RouteDirtRoad);
        grant_roadbuilding(&mut app, faction);

        handle_road_verb(&mut app, faction, ROAD_BAND_ID, COORD, Improvement::Grade);
        assert_eq!(
            keeper_of(&app, COORD),
            Some(BandId(ROAD_BAND_ID)),
            "re-issuing `grade` on a road nobody keeps ADOPTS it — no new verb, no new mechanism"
        );

        // And now that it IS ours and already at that rung, the same command is refused.
        handle_road_verb(&mut app, faction, ROAD_BAND_ID, COORD, Improvement::Grade);
        assert!(
            road_failure_detail_contains(&app, "already a dirt road"),
            "a rung this band already holds has nothing left to raise"
        );
        assert_eq!(
            queued_road_tiles(&app, band),
            vec![COORD],
            "and the refusal queued nothing a second time"
        );
    }

    /// ⛔ **`abandon` PUTS A ROAD DOWN**, which is the per-road choice the band-wide `Roadwork` pool
    /// needs: the pool covers every road the band keeps, so *"pay for this one and not that one"* has
    /// to be expressible, and this is where.
    #[test]
    fn abandon_releases_a_road_and_drops_its_queue_entry() {
        const COORD: UVec2 = UVec2::new(1, 1);

        let (mut app, faction, band) = road_world(COORD);
        seat_unkept_road(&mut app, COORD, core_sim::RungKey::RouteTrail);
        grant_roadbuilding(&mut app, faction);
        handle_road_verb(&mut app, faction, ROAD_BAND_ID, COORD, Improvement::Grade);
        assert_eq!(keeper_of(&app, COORD), Some(BandId(ROAD_BAND_ID)));

        handle_abandon(&mut app, faction, patch_source(COORD));
        assert_eq!(
            keeper_of(&app, COORD),
            None,
            "the road is nobody's job again — and its meter is untouched, so it rots back down"
        );
        assert!(
            queued_road_tiles(&app, band).is_empty(),
            "and the declaration goes with it: the two are one statement"
        );
        assert!(
            app.world
                .resource::<core_sim::RoadRegistry>()
                .road(COORD)
                .is_some(),
            "abandoning destroys nothing on the spot — the ground keeps what is on it"
        );
    }

    /// ⛔ **A GRADED TILE IS RAISED BY THE BAND'S BUILDERS**, exactly as a Field or a pen is — and it
    /// is the *builders* pool, not the keepers and not traffic.
    #[test]
    fn the_builders_pool_raises_a_graded_tile() {
        const COORD: UVec2 = UVec2::new(1, 1);

        let (mut app, faction, band) = road_world(COORD);
        seat_unkept_road(&mut app, COORD, core_sim::RungKey::RouteTrail);
        grant_roadbuilding(&mut app, faction);
        handle_road_verb(&mut app, faction, ROAD_BAND_ID, COORD, Improvement::Grade);
        let before = app
            .world
            .resource::<core_sim::RoadRegistry>()
            .road(COORD)
            .expect("the road exists")
            .position();

        // Nobody on `builders`: the entry stands and nothing is banked.
        resolve_labor(&mut app);
        let unstaffed = app
            .world
            .resource::<core_sim::RoadRegistry>()
            .road(COORD)
            .expect("the road exists")
            .position();
        assert_eq!(
            unstaffed, before,
            "a declaration with no hands behind it puts nothing on the ground"
        );

        // Staff the pool and the meter moves.
        {
            let mut allocation = app
                .world
                .get_mut::<LaborAllocation>(band)
                .expect("allocation");
            allocation.set_assignment(LaborTarget::Builders, BAND_WORKERS, BAND_WORKERS, None);
        }
        resolve_labor(&mut app);
        let staffed = app
            .world
            .resource::<core_sim::RoadRegistry>()
            .road(COORD)
            .expect("the road exists")
            .position();
        assert!(
            staffed > before,
            "the band's BUILDERS raise a graded tile: {staffed} against {before}"
        );
    }
}
