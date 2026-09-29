//! **A BAND THAT STANDS STILL FOR A HUNDRED TURNS** — a printing probe, not a balance test.
//!
//! ```text
//! cargo test -p core_sim --test stationary_push_probe -- --ignored --nocapture
//! ```
//!
//! Issue #705, for `docs/plan_civilization_steps.md` §"The push is depletion, and we already own
//! it": a band that never moves — how do its food source rows (the intensification arc's per-source
//! *actual vs sustainable* breakdown), its larder and its runway move, turn by turn? Two readers:
//!
//! 1. **Is depletion felt?** Is the thinning of the patch too slow, or too uniform across biomes,
//!    for a player to see the ground push them on?
//! 2. **What surplus does a well-placed band run?** It sets the storage lesson's pace — the steady
//!    larder is `surplus/turn ÷ spoilage rate`.
//!
//! **Shipped path only.** Every world is `build_test_app()` — the production `build_headless_app`
//! with a map seed written before the first update — advanced by `run_turn`, so the real Startup
//! chain and every turn stage run. Shipped config and the shipped start profile
//! (`simulation_config.json` → `start_profile_id: "late_forager_tribe"`, `map_preset_id:
//! "earthlike"`). The one thing `build_test_app` changes beyond the seed — it stocks every band's
//! gear for fixtures — is put **back** to the shipped `EquipmentConfig` before Startup, so the band
//! owns what the outfitting window's committed default gives it and nothing more.
//!
//! **Every figure is read off the ENCODED snapshot** — the FlatBuffers envelope a client parses:
//! `PopulationCohortState` (`size`, `foodIncome`, `foodConsumption`, `raidForfeit`, `turnsOfFood`,
//! the `provisions` store) and its `laborAssignments` rows (`actualYield`, `realizedYield`,
//! `sustainableYield`, `overdraws`), plus the source's own stock from `SubsistenceSection`
//! (`foragePatches` / `herds`: `biomass` and `carryingCapacity`). Nothing is recomputed in-process.
//!
//! **The labour.** A spawned band carries **no** labour assignments (every working-age hand is
//! idle), so this probe staffs it the way a new player would, through the same steps the server's
//! `handle_assign_labor` takes (the handler lives in `bin/server.rs`, so its body is followed here
//! rather than called): a `LaborTarget` at the shipped `DEFAULT_ESCAPEMENT_FLOOR`, the whole basket
//! (no take selection), no crop named, the kit the command resolves when none is named, and
//! `LaborAllocation::set_assignment` against the band's assignable hands. What the player picks is
//! read off the wire — see [`choose_sources`] — and is run twice per map ([`Staffing`]): the
//! first thing that looks good, and everything the apron offers, so the answer brackets player
//! skill instead of resting on one guess about it.
//!
//! **The band never moves**: no move order, no split. A source outside the apron posts a work party
//! (a caravan) — that is the shipped behaviour of a standing band, not a move.

use bevy::math::UVec2;
use bevy::prelude::Entity;
use core_sim::grid_utils::hex_distance_wrapped;
use core_sim::{
    build_test_app, herd_default_hunt_kit, run_turn, BandBench, BandId, BandWorkforce,
    CreaturesConfigHandle, EquipmentConfigHandle, FaunaConfigHandle, LaborAllocation, LaborTarget,
    PopulationCohort, SimulationConfig, SnapshotHistory, TakeSelection, DEFAULT_ESCAPEMENT_FLOOR,
};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

// ---------------------------------------------------------------------------------------------
// Measurement choices — none of these is a gameplay lever.
// ---------------------------------------------------------------------------------------------

/// Turns the band stands still for — the issue's ~100, long enough for a patch to reach its floor.
const TURNS: u32 = 100;
/// The pinned maps, each chosen so the player's start lands in a different biome (found by
/// surveying seeds 1–40 of the shipped `earthlike` preset; the start search never lands a band in
/// desert, scrub or tundra there, so these five are the palette's start biomes).
const RUNS: [(u64, &str); 5] = [
    (1, "grassland (PrairieSteppe)"),
    (10, "forest (MixedWoodland)"),
    (2, "coast/wetland (RiverDelta)"),
    (5, "river valley (Floodplain)"),
    (20, "dry (OasisBasin)"),
];
/// The player faction — faction 0 is always the human seat.
const PLAYER_FACTION: u32 = 0;
/// The share of the band's hands a new player sends to gather; the rest hunt. An even split — the
/// neutral first move when the compose sheet offers both webs.
const FORAGE_SHARE: f32 = 0.5;
/// **The two ways a new player staffs the opening**, run on every map so the answer is a bracket
/// rather than one guess about player skill.
#[derive(Clone, Copy, Debug)]
enum Staffing {
    /// The first thing that looks good: the best-rated gathering site in the apron and the nearest
    /// food-paying herd.
    Naive,
    /// Everything the Work board offers nearby: every gathering site in the apron (foragers spread
    /// evenly) and the food-paying herd in hunt reach with the most standing stock.
    Thorough,
}

impl Staffing {
    fn label(self) -> &'static str {
        match self {
            Staffing::Naive => "naive",
            Staffing::Thorough => "thorough",
        }
    }
}

/// Both staffings, in print order.
const STAFFINGS: [Staffing; 2] = [Staffing::Naive, Staffing::Thorough];
/// Every turn up to here is printed; after it, every [`PRINT_EVERY`]th.
const PRINT_ALL_UNTIL: u32 = 10;
/// The table's stride past [`PRINT_ALL_UNTIL`].
const PRINT_EVERY: u32 = 5;
/// The turns the summary quotes larder and runway at.
const CHECKPOINTS: [u32; 3] = [10, 50, 100];
/// The first turn of the "settled" half the second surplus mean is taken over.
const SECOND_HALF_FROM: u32 = 51;
/// `turnsOfFood`'s published no-drain sentinel (`larder_runway_turns`) — read as infinity.
const RUNWAY_SENTINEL: f32 = 999.0;
/// The larder item the Food line reads.
const PROVISIONS: &str = "provisions";
/// Fixed-point scale of a `CohortStore.quantity` (`sim_runtime::FIXED_POINT_SCALE`).
const STORE_SCALE: f32 = sim_runtime::FIXED_POINT_SCALE as f32;
/// A source's stock "is at its floor" once within this share above `floor · K` — the take has
/// drawn it down and only regrowth is left to cut.
const FLOOR_TOLERANCE: f32 = 0.02;

// ---------------------------------------------------------------------------------------------
// Reading the wire
// ---------------------------------------------------------------------------------------------

/// The latest published frame, encoded exactly as it crosses the socket.
fn encoded_frame(app: &bevy::app::App) -> Vec<u8> {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref()).to_vec()
}

fn world_of(bytes: &[u8]) -> fb::WorldSnapshot<'_> {
    fb::root_as_envelope(bytes)
        .expect("the snapshot encodes to a valid envelope")
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
}

/// The player's resident bands (expedition parties excluded).
fn player_bands<'a>(world: &fb::WorldSnapshot<'a>) -> Vec<fb::PopulationCohortState<'a>> {
    world
        .population()
        .and_then(|section| section.populations())
        .map(|list| {
            list.iter()
                .filter(|c| c.faction() == PLAYER_FACTION && !c.isExpedition())
                .collect()
        })
        .unwrap_or_default()
}

fn larder_of(cohort: &fb::PopulationCohortState<'_>) -> f32 {
    cohort
        .stores()
        .into_iter()
        .flatten()
        .find(|store| store.item() == Some(PROVISIONS))
        .map(|store| store.quantity() as f32 / STORE_SCALE)
        .unwrap_or(0.0)
}

/// Which source a row is: the same key the wire row carries.
#[derive(Clone, Debug, PartialEq)]
enum SourceKey {
    Forage(UVec2),
    Hunt(String),
}

impl SourceKey {
    fn label(&self) -> String {
        match self {
            SourceKey::Forage(tile) => format!("forage({},{})", tile.x, tile.y),
            SourceKey::Hunt(id) => format!("hunt {id}"),
        }
    }
}

/// One source row's published figures on one turn.
#[derive(Clone, Copy, Default)]
struct RowReading {
    workers: u32,
    actual: f32,
    realized: f32,
    sustainable: f32,
    overdraws: bool,
    /// The source's stock and capacity, from its `SubsistenceSection` entry; `None` when the entry
    /// is not in this frame (a herd out of sight).
    stock: Option<(f32, f32)>,
}

fn read_row(
    world: &fb::WorldSnapshot<'_>,
    band: &fb::PopulationCohortState<'_>,
    key: &SourceKey,
) -> Option<RowReading> {
    let row = band
        .laborAssignments()
        .into_iter()
        .flatten()
        .find(|row| match key {
            SourceKey::Forage(tile) => {
                row.kind() == Some("forage") && row.targetX() == tile.x && row.targetY() == tile.y
            }
            SourceKey::Hunt(id) => row.kind() == Some("hunt") && row.faunaId() == Some(id.as_str()),
        })?;
    let subsistence = world.subsistence();
    let stock = match key {
        SourceKey::Forage(tile) => {
            subsistence
                .and_then(|s| s.foragePatches())
                .and_then(|patches| {
                    patches
                        .iter()
                        .find(|p| p.x() == tile.x && p.y() == tile.y)
                        .map(|p| (p.biomass(), p.carryingCapacity()))
                })
        }
        SourceKey::Hunt(id) => subsistence.and_then(|s| s.herds()).and_then(|herds| {
            herds
                .iter()
                .find(|h| h.id() == Some(id.as_str()))
                .map(|h| (h.biomass(), h.carryingCapacity()))
        }),
    };
    Some(RowReading {
        workers: row.workers(),
        actual: row.actualYield(),
        realized: row.realizedYield(),
        sustainable: row.sustainableYield(),
        overdraws: row.overdraws(),
        stock,
    })
}

// ---------------------------------------------------------------------------------------------
// The new player's first move
// ---------------------------------------------------------------------------------------------

/// A candidate gathering site: where, how far, the published per-worker rate, and its capacity.
struct SiteCandidate {
    at: UVec2,
    distance: u32,
    per_worker: f32,
    capacity: f32,
}

/// A candidate quarry.
struct HerdCandidate {
    id: String,
    species: String,
    distance: u32,
    biomass: f32,
    capacity: f32,
}

/// **What a new player staffs, read off the frame they would be looking at.**
///
/// - **Gather:** gathering sites (published `foodModules` entries — the plant ladder's rung-1 site
///   rule) inside the band's apron (`workRange`), ranked by published `perWorkerYield`, nearest
///   first on a tie. [`Staffing::Naive`] takes the top one, [`Staffing::Thorough`] all of them. If
///   the apron holds none, the nearest site anywhere.
/// - **Hunt:** a herd that is `huntable` and pays food (`provisionsPerBiomass > 0`) within the band's
///   published `huntReach` — the nearest for [`Staffing::Naive`], the largest standing stock for
///   [`Staffing::Thorough`].
///
/// Returns the chosen sources, and a line saying what was chosen and why for the output header.
fn choose_sources(
    bytes: &[u8],
    width: u32,
    wrap: bool,
    staffing: Staffing,
) -> (Vec<SourceKey>, String) {
    let world = world_of(bytes);
    let band = player_bands(&world)
        .into_iter()
        .next()
        .expect("the player has a band");
    let home = UVec2::new(band.currentX(), band.currentY());
    let subsistence = world.subsistence().expect("the frame carries subsistence");
    let sites: Vec<UVec2> = subsistence
        .foodModules()
        .into_iter()
        .flatten()
        .map(|m| UVec2::new(m.x(), m.y()))
        .collect();
    let distance = |at: UVec2| hex_distance_wrapped(home, at, width, wrap);

    let mut patches: Vec<SiteCandidate> = subsistence
        .foragePatches()
        .into_iter()
        .flatten()
        .map(|p| {
            let at = UVec2::new(p.x(), p.y());
            SiteCandidate {
                at,
                distance: distance(at),
                per_worker: p.perWorkerYield(),
                capacity: p.carryingCapacity(),
            }
        })
        .filter(|p| sites.contains(&p.at))
        .collect();
    let in_apron = patches.iter().any(|p| p.distance <= band.workRange());
    if in_apron {
        patches.retain(|p| p.distance <= band.workRange());
        patches.sort_by(|a, b| {
            b.per_worker
                .total_cmp(&a.per_worker)
                .then(a.distance.cmp(&b.distance))
        });
    } else {
        patches.sort_by(|a, b| {
            a.distance
                .cmp(&b.distance)
                .then(b.per_worker.total_cmp(&a.per_worker))
        });
    }

    let mut herds: Vec<HerdCandidate> = subsistence
        .herds()
        .into_iter()
        .flatten()
        .filter(|h| h.huntable() && h.provisionsPerBiomass() > 0.0)
        .map(|h| HerdCandidate {
            id: h.id().unwrap_or_default().to_string(),
            species: h.species().unwrap_or_default().to_string(),
            distance: distance(UVec2::new(h.x(), h.y())),
            biomass: h.biomass(),
            capacity: h.carryingCapacity(),
        })
        .filter(|h| h.distance <= band.huntReach())
        .collect();
    match staffing {
        Staffing::Naive => herds.sort_by(|a, b| {
            a.distance
                .cmp(&b.distance)
                .then(b.biomass.total_cmp(&a.biomass))
        }),
        Staffing::Thorough => herds.sort_by(|a, b| {
            b.biomass
                .total_cmp(&a.biomass)
                .then(a.distance.cmp(&b.distance))
        }),
    }
    let sites_taken = match staffing {
        Staffing::Naive => 1,
        Staffing::Thorough if in_apron => patches.len(),
        Staffing::Thorough => 1,
    };

    let mut chosen = Vec::new();
    let mut note = format!(
        "home ({},{}), apron {} hexes, hunt reach {}; ",
        home.x,
        home.y,
        band.workRange(),
        band.huntReach()
    );
    for site in patches.iter().take(sites_taken) {
        chosen.push(SourceKey::Forage(site.at));
        note += &format!(
            "gather site ({},{}) at {} hexes ({}, {:.2}/worker, K {:.0}); ",
            site.at.x,
            site.at.y,
            site.distance,
            if in_apron {
                "best in apron"
            } else {
                "nearest — none in apron"
            },
            site.per_worker,
            site.capacity
        );
    }
    if patches.is_empty() {
        note += "no gathering site; ";
    }
    if let Some(herd) = herds.first() {
        chosen.push(SourceKey::Hunt(herd.id.clone()));
        note += &format!(
            "hunt {} [{}] at {} hexes (B {:.0} / K {:.0})",
            herd.species, herd.id, herd.distance, herd.biomass, herd.capacity
        );
    } else {
        note += "no food-paying herd in hunt reach";
    }
    (chosen, note)
}

/// The band's entity, by the durable id the wire publishes.
fn band_entity(app: &mut bevy::app::App, band_id: u64) -> Entity {
    app.world
        .query::<(Entity, &BandId)>()
        .iter(&app.world)
        .find(|(_, id)| id.0 == band_id)
        .map(|(entity, _)| entity)
        .expect("the wire's band exists in the world")
}

/// **`handle_assign_labor`'s steps, with no kit, crop or take named and the default floor.**
fn assign(app: &mut bevy::app::App, band: Entity, key: &SourceKey, workers: u32) -> u32 {
    let target = match key {
        SourceKey::Forage(tile) => LaborTarget::Forage {
            tile: *tile,
            floor: DEFAULT_ESCAPEMENT_FLOOR,
            species: None,
            take_species: TakeSelection::from_keys(Vec::<String>::new()),
        },
        SourceKey::Hunt(id) => LaborTarget::Hunt {
            fauna_id: id.clone(),
            floor: DEFAULT_ESCAPEMENT_FLOOR,
        },
    };
    // `default_kit_for_target`: the herd's own default on a hunt, the job's default otherwise.
    let equipment = app.world.resource::<EquipmentConfigHandle>().get();
    let absent = match key {
        SourceKey::Forage(_) => equipment.default_kit(target.kit_job()),
        SourceKey::Hunt(id) => {
            let fauna = app.world.resource::<FaunaConfigHandle>().get();
            let (species, corralled) = app
                .world
                .resource::<core_sim::HerdRegistry>()
                .find(id)
                .map(|herd| (herd.species.clone(), herd.is_corralled()))
                .expect("the chosen herd is in the registry");
            let species = fauna
                .species_by_display(&species)
                .expect("the chosen herd's species resolves");
            herd_default_hunt_kit(
                &equipment,
                app.world.resource::<CreaturesConfigHandle>().get().person(),
                species,
                corralled,
            )
        }
    };
    let kit = equipment
        .resolve_kit_or(None, target.kit_job(), absent)
        .expect("an unnamed kit resolves to the default");
    let available = BandWorkforce::resolve(
        app.world.get::<PopulationCohort>(band),
        app.world.get::<LaborAllocation>(band),
        app.world.get::<BandBench>(band),
    )
    .assignable();
    if app.world.get::<LaborAllocation>(band).is_none() {
        app.world
            .entity_mut(band)
            .insert(LaborAllocation::default());
    }
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("allocation inserted above")
        .set_assignment(target, workers, available, Some(kit))
}

// ---------------------------------------------------------------------------------------------
// One run
// ---------------------------------------------------------------------------------------------

/// One turn's band-level reading.
#[derive(Clone, Default)]
struct TurnReading {
    tick: u64,
    alive: bool,
    bands: usize,
    moved: bool,
    population: u32,
    income: f32,
    consumption: f32,
    raid_forfeit: f32,
    larder: f32,
    runway: f32,
    rows: Vec<Option<RowReading>>,
}

impl TurnReading {
    /// The published ledger's own identity: `larder_delta == foodIncome − foodConsumption −
    /// raidForfeit`.
    fn net(&self) -> f32 {
        self.income - self.consumption - self.raid_forfeit
    }
}

struct RunSummary {
    biome: &'static str,
    staffing: Staffing,
    seed: u64,
    labor: String,
    pop_start: u32,
    pop_end: u32,
    first_below: Option<u32>,
    first_overdraw: Option<u32>,
    first_floor: Option<u32>,
    larder_out: Option<u32>,
    mean_net_all: f32,
    mean_net_late: f32,
    larders: Vec<f32>,
    runways: Vec<f32>,
    fate: String,
}

fn runway_text(runway: f32) -> String {
    if runway >= RUNWAY_SENTINEL {
        "inf".to_string()
    } else {
        format!("{runway:.1}")
    }
}

fn fraction_text(end: f32, start: f32) -> String {
    if start > 0.0 {
        format!("{:.2}", end / start)
    } else if end > 0.0 {
        "n/a (t1=0)".to_string()
    } else {
        "0/0".to_string()
    }
}

fn stock_text(stock: Option<(f32, f32)>) -> String {
    stock.map_or("unseen".to_string(), |(b, k)| format!("{b:.0}/{k:.0}"))
}

fn run(seed: u64, biome: &'static str, staffing: Staffing, out: &mut String) -> RunSummary {
    use std::fmt::Write;

    let mut app = build_test_app();
    app.world.resource_mut::<SimulationConfig>().map_seed = seed;
    // Undo the fixture stocking `build_test_app` installs: this probe's subject IS the shipped
    // opening.
    app.world.insert_resource(EquipmentConfigHandle::default());
    // The Startup chain (worldgen, spawn, the outfitting window's committed default) and the first
    // turn — the frame a new player first looks at.
    run_turn(&mut app);
    let (width, wrap) = {
        let config = app.world.resource::<SimulationConfig>();
        (config.grid_size.x, config.map_topology.wrap_horizontal)
    };
    let opening = encoded_frame(&app);
    let (band_id, home, idle, default_rows, terrain) = {
        let world = world_of(&opening);
        let band = player_bands(&world)
            .into_iter()
            .next()
            .expect("the player has a band");
        let home = UVec2::new(band.currentX(), band.currentY());
        let terrain = world
            .map()
            .and_then(|m| m.tiles())
            .and_then(|tiles| tiles.iter().find(|t| t.x() == home.x && t.y() == home.y))
            .map(|t| format!("{:?}", t.terrain()))
            .unwrap_or_else(|| "?".to_string());
        (
            band.bandId(),
            home,
            band.idleWorkers(),
            band.laborAssignments().map(|rows| rows.len()).unwrap_or(0),
            terrain,
        )
    };

    let (sources, choice) = choose_sources(&opening, width, wrap, staffing);
    let entity = band_entity(&mut app, band_id);
    let labor = if default_rows > 0 {
        format!("DEFAULT — the spawned band carried {default_rows} labour row(s); left untouched")
    } else {
        let sites = sources
            .iter()
            .filter(|k| matches!(k, SourceKey::Forage(_)))
            .count() as u32;
        let hunts = sources.len() as u32 - sites;
        let forage_hands = match (sites, hunts) {
            (0, _) => 0,
            (_, 0) => idle,
            _ => (idle as f32 * FORAGE_SHARE).ceil() as u32,
        };
        let hunt_hands = idle - forage_hands;
        let mut applied = Vec::new();
        let mut site_index = 0u32;
        for key in &sources {
            let hands = match key {
                SourceKey::Forage(_) => {
                    // Spread evenly; the remainder goes to the best-rated sites first.
                    let share = forage_hands / sites + u32::from(site_index < forage_hands % sites);
                    site_index += 1;
                    share
                }
                SourceKey::Hunt(_) => hunt_hands,
            };
            let got = assign(&mut app, entity, key, hands);
            applied.push(format!("{} x{got}", key.label()));
        }
        format!(
            "ASSIGNED — the spawned band had no labour rows and {idle} idle hands; {}",
            applied.join(", ")
        )
    };

    let rule = "=".repeat(100);
    let _ = writeln!(out, "\n{rule}");
    let _ = writeln!(
        out,
        "RUN seed {seed}, {} staffing — start biome {biome}; home tile terrain {terrain}",
        staffing.label()
    );
    let _ = writeln!(out, "  choice: {choice}");
    let _ = writeln!(out, "  labour: {labor}");
    let _ = writeln!(
        out,
        "  source: every figure below is off the ENCODED snapshot (wire), none in-process"
    );
    let _ = writeln!(
        out,
        "  per source: w=workers act=actualYield rlz=realizedYield sus=sustainableYield \
         !=overdraws B/K=stock/capacity"
    );

    let mut turns: Vec<TurnReading> = Vec::new();
    for _ in 0..TURNS {
        run_turn(&mut app);
        let bytes = encoded_frame(&app);
        let world = world_of(&bytes);
        let tick = world.header().map(|h| h.tick()).unwrap_or_default();
        let bands = player_bands(&world);
        let reading = match bands.iter().find(|b| b.bandId() == band_id) {
            Some(band) => TurnReading {
                tick,
                alive: true,
                bands: bands.len(),
                moved: band.currentX() != home.x || band.currentY() != home.y,
                population: band.size(),
                income: band.foodIncome(),
                consumption: band.foodConsumption(),
                raid_forfeit: band.raidForfeit(),
                larder: larder_of(band),
                runway: band.turnsOfFood(),
                rows: sources
                    .iter()
                    .map(|key| read_row(&world, band, key))
                    .collect(),
            },
            None => TurnReading {
                tick,
                bands: bands.len(),
                rows: vec![None; sources.len()],
                ..Default::default()
            },
        };
        turns.push(reading);
    }

    // The per-turn table.
    let mut header = format!("{:>4} {:>5} {:>4}", "turn", "tick", "pop");
    for key in &sources {
        header += &format!(" | {:<46}", key.label());
    }
    header += &format!(
        " | {:>6} {:>6} {:>7} {:>8} {:>7}",
        "income", "cons", "net", "larder", "runway"
    );
    let _ = writeln!(out, "{header}");
    for (index, t) in turns.iter().enumerate() {
        let turn = index as u32 + 1;
        if turn > PRINT_ALL_UNTIL && !turn.is_multiple_of(PRINT_EVERY) {
            continue;
        }
        if !t.alive {
            let _ = writeln!(out, "{turn:>4} {:>5}  band gone", t.tick);
            continue;
        }
        let mut line = format!("{turn:>4} {:>5} {:>4}", t.tick, t.population);
        for row in &t.rows {
            line += &match row {
                Some(r) => format!(
                    " | w{:<2} act{:>6.2} rlz{:>6.2} sus{:>6.2}{} {:<11}",
                    r.workers,
                    r.actual,
                    r.realized,
                    r.sustainable,
                    if r.overdraws { "!" } else { " " },
                    stock_text(r.stock)
                ),
                None => format!(" | {:<46}", "(no row)"),
            };
        }
        line += &format!(
            " | {:>6.2} {:>6.2} {:>+7.2} {:>8.1} {:>7}",
            t.income,
            t.consumption,
            t.net(),
            t.larder,
            runway_text(t.runway)
        );
        if t.bands > 1 {
            line += &format!("  [{} player bands]", t.bands);
        }
        if t.moved {
            line += "  [MOVED]";
        }
        let _ = writeln!(out, "{line}");
    }

    // The per-run summary.
    let first_turn_where = |test: &dyn Fn(&RowReading) -> bool| {
        turns
            .iter()
            .position(|t| t.rows.iter().flatten().any(test))
            .map(|i| i as u32 + 1)
    };
    let first_below = first_turn_where(&|r| r.actual < r.sustainable);
    let first_overdraw = first_turn_where(&|r| r.overdraws);
    let at_floor = |r: &RowReading| {
        r.stock.is_some_and(|(biomass, capacity)| {
            biomass <= DEFAULT_ESCAPEMENT_FLOOR * capacity * (1.0 + FLOOR_TOLERANCE)
        })
    };
    let floor_turns: Vec<Option<u32>> = (0..sources.len())
        .map(|index| {
            turns
                .iter()
                .position(|t| t.rows[index].as_ref().is_some_and(at_floor))
                .map(|i| i as u32 + 1)
        })
        .collect();
    let first_floor = floor_turns.iter().flatten().min().copied();
    let larder_out = turns
        .iter()
        .position(|t| t.alive && t.larder < t.consumption)
        .map(|i| i as u32 + 1);
    let mean_from = |from: u32| {
        let window: Vec<f32> = turns
            .iter()
            .enumerate()
            .filter(|(i, t)| t.alive && *i as u32 + 1 >= from)
            .map(|(_, t)| t.net())
            .collect();
        if window.is_empty() {
            0.0
        } else {
            window.iter().sum::<f32>() / window.len() as f32
        }
    };
    let at = |turn: u32| &turns[(turn - 1) as usize];
    let alive: Vec<&TurnReading> = turns.iter().filter(|t| t.alive).collect();
    let pop_start = at(1).population;
    let pop_end = at(TURNS).population;
    let pop_min = alive.iter().map(|t| t.population).min().unwrap_or(0);
    let (peak_index, pop_peak) = turns
        .iter()
        .enumerate()
        .map(|(i, t)| (i, t.population))
        .max_by_key(|(i, population)| (*population, std::cmp::Reverse(*i)))
        .unwrap_or_default();
    let larder_emptied = alive.iter().any(|t| t.larder <= 0.0);
    let fate = if !at(TURNS).alive {
        "DIED".to_string()
    } else {
        let trend = match pop_end.cmp(&pop_start) {
            std::cmp::Ordering::Greater => "grew",
            std::cmp::Ordering::Less => "shrank",
            std::cmp::Ordering::Equal => "held",
        };
        format!(
            "{trend} {pop_start}->{pop_end} (peak {pop_peak} @t{}, min {pop_min}){}",
            peak_index + 1,
            if larder_emptied { ", larder hit 0" } else { "" }
        )
    };

    let _ = writeln!(
        out,
        "  -- summary, seed {seed} ({biome}), {} staffing --",
        staffing.label()
    );
    let never = || "never".to_string();
    let _ = writeln!(
        out,
        "  first turn any source's actual < sustainable: {}",
        first_below.map_or_else(never, |t| t.to_string())
    );
    let _ = writeln!(
        out,
        "  first turn any source flags overdraws: {}",
        first_overdraw.map_or_else(never, |t| t.to_string())
    );
    for (index, key) in sources.iter().enumerate() {
        let first = at(1).rows[index].unwrap_or_default();
        let last = at(TURNS).rows[index].unwrap_or_default();
        let _ = writeln!(
            out,
            "  {}: actual t{TURNS}/t1 = {} ({:.2} / {:.2}); realized t{TURNS}/t1 = {} ({:.2} / {:.2}); \
             stock t1 {} -> t{TURNS} {}",
            key.label(),
            fraction_text(last.actual, first.actual),
            last.actual,
            first.actual,
            fraction_text(last.realized, first.realized),
            last.realized,
            first.realized,
            stock_text(first.stock),
            stock_text(last.stock),
        );
        let _ = writeln!(
            out,
            "      at its floor (B <= {DEFAULT_ESCAPEMENT_FLOOR}·K) from turn: {}",
            floor_turns[index].map_or("never".to_string(), |t| t.to_string())
        );
    }
    let mean_net_all = mean_from(1);
    let mean_net_late = mean_from(SECOND_HALF_FROM);
    let _ = writeln!(
        out,
        "  mean net surplus/turn: turns 1-{TURNS} {mean_net_all:+.2}; turns \
         {SECOND_HALF_FROM}-{TURNS} {mean_net_late:+.2}"
    );
    let larders: Vec<f32> = CHECKPOINTS.iter().map(|&t| at(t).larder).collect();
    let runways: Vec<f32> = CHECKPOINTS.iter().map(|&t| at(t).runway).collect();
    let _ = writeln!(
        out,
        "  larder at {CHECKPOINTS:?}: {}",
        larders
            .iter()
            .map(|l| format!("{l:.1}"))
            .collect::<Vec<_>>()
            .join(" / ")
    );
    let _ = writeln!(
        out,
        "  runway at {CHECKPOINTS:?}: {}",
        runways
            .iter()
            .map(|r| runway_text(*r))
            .collect::<Vec<_>>()
            .join(" / ")
    );
    let _ = writeln!(
        out,
        "  larder first below one turn's eating: {}",
        larder_out.map_or("never".to_string(), |t| format!("turn {t}"))
    );
    let _ = writeln!(out, "  band: {fate}");

    RunSummary {
        biome,
        staffing,
        seed,
        labor,
        pop_start,
        pop_end,
        first_below,
        first_overdraw,
        first_floor,
        larder_out,
        mean_net_all,
        mean_net_late,
        larders,
        runways,
        fate,
    }
}

#[test]
#[ignore = "printing probe — run with --ignored --nocapture"]
fn a_band_that_stands_still_for_a_hundred_turns() {
    use std::fmt::Write;

    let mut out = String::new();
    let summaries: Vec<RunSummary> = RUNS
        .iter()
        .flat_map(|&(seed, biome)| STAFFINGS.map(|staffing| (seed, biome, staffing)))
        .map(|(seed, biome, staffing)| run(seed, biome, staffing, &mut out))
        .collect();

    let rule = "=".repeat(100);
    let _ = writeln!(out, "\n{rule}");
    let _ = writeln!(
        out,
        "CROSS-RUN SUMMARY ({TURNS} turns, band never moves; net = income - consumption - raid)"
    );
    let _ = writeln!(
        out,
        "  1st<sus = first turn any row's actual < sustainable; 1st! = first overdraws flag; \
         floor@ = first turn a source's stock reaches floor·K; larder0 = first turn larder < one \
         turn's eating"
    );
    let _ = writeln!(
        out,
        "{:<27} {:>4} {:<8} {:>7} {:>7} {:>5} {:>6} {:>7} {:>8} {:>9} {:>17} {:>17}  fate",
        "biome",
        "seed",
        "staffing",
        "pop",
        "1st<sus",
        "1st!",
        "floor@",
        "larder0",
        "net1-100",
        "net51-100",
        "larder t10/50/100",
        "runway t10/50/100"
    );
    for s in &summaries {
        let join = |values: &[f32], text: &dyn Fn(f32) -> String| {
            values
                .iter()
                .map(|v| text(*v))
                .collect::<Vec<_>>()
                .join("/")
        };
        let _ = writeln!(
            out,
            "{:<27} {:>4} {:<8} {:>7} {:>7} {:>5} {:>6} {:>7} {:>+8.2} {:>+9.2} {:>17} {:>17}  {}",
            s.biome,
            s.seed,
            s.staffing.label(),
            format!("{}->{}", s.pop_start, s.pop_end),
            s.first_below.map_or("never".to_string(), |t| t.to_string()),
            s.first_overdraw
                .map_or("never".to_string(), |t| t.to_string()),
            s.first_floor.map_or("never".to_string(), |t| t.to_string()),
            s.larder_out.map_or("never".to_string(), |t| t.to_string()),
            s.mean_net_all,
            s.mean_net_late,
            join(&s.larders, &|l| format!("{l:.0}")),
            join(&s.runways, &runway_text),
            s.fate,
        );
    }
    for s in &summaries {
        let _ = writeln!(out, "  seed {} {}: {}", s.seed, s.staffing.label(), s.labor);
    }
    println!("{out}");
}
