//! **STANDING YIELD — a kept herd chooses its output** (`docs/plan_pen_standing_yield.md`,
//! issue #630). A kept herd produced only meat; milk, eggs and wool are the renewable half, harvested
//! without killing, so the standing stock keeps compounding and the animal pays over its whole life
//! rather than once.
//!
//! The model under test is one line: a herd commits `f ∈ [0,1]` of itself, the **meat take** becomes
//! the existing take `× (1 − f)`, and the **standing yield** is the species' per-head rates
//! `× head count × f × rung_fraction` (`1.0` penned, `husbandry.pastoral_standing_fraction` roaming,
//! nothing wild). What this file pins:
//!
//! - **`f = 0` is byte-identical to the behaviour that shipped** — the whole point of the default.
//! - **`f = 1` pays no meat and wastes nothing**, and pays the derived standing rates instead.
//! - **The pastoral share applies at `animal:pastoral` and `1.0` at `animal:pen`**, which is what
//!   makes the two never-pennable migratory species worth keeping at all.
//! - **A species with no `standing_yield` block yields nothing renewable at any `f`** — and still
//!   loses its meat to the split, which is the honest reading of an order the player gave.
//! - **The material half is not rounded** — a sub-unit fleece draw lands in the store as itself.
//! - **The split rides the WIRE**, on the published assignment row, so a client itemizes it without
//!   arithmetic.
//! - **The config validator refuses each bad case.**
//!
//! Deterministic (a pinned map seed, no rand), modelled on `grazing_2d_pen.rs`.

mod pen_materials_support;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::Entity;
use bevy::MinimalPlugins;

use core_sim::grid_utils::hex_range_tiles;
use core_sim::{
    advance_graze_regrowth, advance_herd_grazing, advance_herds, advance_husbandry,
    advance_labor_allocation, scalar_from_f32, scalar_one, scalar_zero, spawn_initial_graze,
    spawn_initial_herds, spawn_initial_world, CommandEventLog, CultureManager,
    DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfig, FaunaConfigHandle,
    ForageRegistry, GenerationId, GenerationRegistry, GrazePatch, GrazeRegistry, Herd,
    HerdDensityMap, HerdRegistry, HerdTelemetry, LaborAllocation, LaborAssignment,
    LaborConfigHandle, LaborTarget, LadderConfig, LadderConfigHandle, MapPresets, MapPresetsHandle,
    MaterialsConfig, MoraleCause, PopulationCohort, SimulationConfig, SimulationTick, SizeClass,
    SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, SourcePriority, StartLocation,
    StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, StartingUnit, TileRegistry,
    WellbeingConfigHandle, FOOD,
};
use core_sim::{recapture_snapshot_in_place, SnapshotHistory};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

/// A pinned earthlike map — `map_seed` is otherwise entropy.
const MAP_SEED: u64 = core_sim::HARNESS_MAP_SEED;
/// A crew big enough that neither the keeping bill nor the take is ever hand-limited: this file
/// measures the **split**, never a staffing.
const KEEPER_WORKERS: u32 = 500;
/// **A DELIBERATELY SMALL BUILD CREW.** A commitment costs ~25 work at the pen, so a keeping-sized
/// pool would bank it in a single turn and there would be no *"the fraction has not moved yet"* state
/// left to assert — the whole point of §4 being that the change is worked off over turns.
const BUILDERS: u32 = 2;
/// Re-stocked into the keeper each turn so the pen's feed is always payable.
const RESTOCK: f32 = 1_000_000.0;
/// Capacity every tile of the levelled footprint carries — a fixture constant, since every assertion
/// reads a ratio or a rate rather than an absolute.
const LEVELLED_PASTURE_CAPACITY: f32 = 200.0;
/// The fence these fixtures seat: one ring, so the pen has real pasture under it.
const PEN_RADIUS: u32 = 1;
/// The stance every fixture holds — the food peak, the floor a fresh assignment gets.
const SUSTAIN_FLOOR: f32 = 0.5;
/// **THE RUNG THE SEED/RESOLVED COMPARISON IS MADE AT, AND WHY IT IS THE HALTER.**
///
/// `forecast == actual` needs the herd the seed priced and the herd the turn takes from to be one
/// herd. A **pastoral** herd is exactly that: `next_turns_quarry` applies the one Logistics
/// regrowth, and one Logistics regrowth is the whole of what happens to a roaming herd between the
/// seed and the take. A **pen** additionally grazes its own fenced footprint and draws its keeper's
/// hay in the same window, neither of which a pre-commit quote simulates — a pre-existing pen-only
/// approximation this arc did not introduce and does not touch. The split is rung-agnostic
/// (`fauna::herd_standing_rung_share` is the only thing that differs), so measuring it at the halter
/// tests the same arithmetic on ground where the invariant is exact.
const SEED_COMPARISON_RUNG: Rung = Rung::Pastoral;

/// Turns a fixture settles for before the seed/resolved comparison, so the two readings are taken
/// on a herd that has stopped moving rather than across its opening transient.
const SETTLE_TURNS: u32 = 5;
/// Turns each fixture runs. Long enough for a penned herd to settle onto its own line and for a
/// sub-unit material draw to cross a whole unit.
const TURNS: u32 = 40;

/// **The roster's archetypal dairy animal** — `standing_yield.provisions_per_head 0.0351`, and no
/// standing material, so its food half is readable on its own.
const DAIRY_SPECIES: &str = "Wild Aurochs";
/// **The roster's fleece animal** — milk *and* fibre, which is what makes it the row that proves one
/// fraction pays both accounts.
const FLEECE_SPECIES: &str = "Wild Sheep";
/// **A pennable species with NO `standing_yield` block at all** — pigs give neither milk nor fleece.
const NO_STANDING_SPECIES: &str = "Wild Boar";
/// **A species that can never be penned** (`husbandry_ceiling: "pastoral"`), and therefore the one
/// the pastoral share exists for.
const PASTORAL_ONLY_SPECIES: &str = "Steppe Runners";

/// The material a fleece is made of — the same generic `fibre` a carcass' sinew is.
const FLEECE_MATERIAL: &str = "fibre";

/// Floating-point slack for a figure both sides of an assertion compute the same way, on **one**
/// turn's row.
const EXACT: f32 = 1e-6;

/// Slack for a figure **summed over the whole run**: `f32` addition over [`TURNS`] terms accumulates
/// its own error, so a total is compared as a fraction of itself rather than to the ulp.
const SUMMED: f32 = 1e-5;

/// Two run totals agree — see [`SUMMED`].
fn agrees(a: f32, b: f32) -> bool {
    (a - b).abs() <= SUMMED * a.abs().max(b.abs()).max(1.0)
}

/// Turns the wire fixture runs. A handful is plenty — the assertion is that both halves are *on the
/// row*, not that either has settled — and every one of them is a whole headless turn.
const PUBLISHED_TURNS: u32 = 4;

/// **A HERD HALF-COMMITTED** — the one fraction that puts *both* lines on a row at once, which is
/// what the wire test needs: at `0` or `1` one of the two published halves is zero and the sum
/// assertion would hold trivially.
const COMMITTED_HALF: f32 = 0.5;

fn base_world() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = MAP_SEED;
    app.world.insert_resource(config);

    app.world
        .insert_resource(MapPresetsHandle::new(MapPresets::builtin()));
    app.world
        .insert_resource(GenerationRegistry::with_seed(42, 8));
    app.world.insert_resource(SimulationTick::default());
    app.world.insert_resource(CultureManager::new());
    app.world.insert_resource(StartLocation::default());
    app.world
        .insert_resource(DiscoveryProgressLedger::default());
    app.world.insert_resource(FactionInventory::default());
    app.world
        .insert_resource(StartProfileKnowledgeTagsHandle::new(
            StartProfileKnowledgeTags::builtin(),
        ));
    app.world.insert_resource(SnapshotOverlaysConfigHandle::new(
        SnapshotOverlaysConfig::builtin(),
    ));

    app.add_systems(bevy::app::Startup, spawn_initial_world);
    app.update();

    app.world.insert_resource(HerdRegistry::default());
    app.world.insert_resource(HerdTelemetry::default());
    app.world.insert_resource(HerdDensityMap::default());
    app.world.insert_resource(GrazeRegistry::default());
    app.world.insert_resource(ForageRegistry::default());
    app.world.insert_resource(FaunaConfigHandle::default());
    app.world.insert_resource(LaborConfigHandle::default());
    app.world
        .insert_resource(core_sim::FloraConfigHandle::default());
    app.world.insert_resource(LadderConfigHandle::default());
    app.world.insert_resource(core_sim::RoadRegistry::default());
    app.world.insert_resource(WellbeingConfigHandle::default());
    app.world
        .insert_resource(core_sim::CombatConfigHandle::default());
    app.world
        .insert_resource(core_sim::CreaturesConfigHandle::default());
    app.world
        .insert_resource(core_sim::EquipmentConfigHandle::for_a_stocked_fixture());
    app.world
        .insert_resource(core_sim::MaterialsConfigHandle::default());
    app.world
        .insert_resource(core_sim::RecipesConfigHandle::default());
    app.world
        .insert_resource(core_sim::ExtractionConfigHandle::default());
    // An empty deposit registry is the shipped turn-1 state: a working is opened the
    // first turn a crew stands on it, so a harness with no `extract` row has none.
    app.world
        .insert_resource(core_sim::extraction::DepositRegistry::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.run_system_once(spawn_initial_herds);
    app.world.run_system_once(spawn_initial_graze);
    app
}

fn richest_pasture(app: &App) -> (UVec2, f32) {
    app.world
        .resource::<GrazeRegistry>()
        .richest_patch()
        .expect("the earthlike map seeds graze patches")
}

/// Level the footprint so the fence's `K` is a function of the tiles it encloses and nothing else.
fn level_footprint_pasture(app: &mut App, center: UVec2, radius: u32) {
    let (width, height, wrap) = {
        let registry = app.world.resource::<TileRegistry>();
        let wrap = app
            .world
            .resource::<SimulationConfig>()
            .map_topology
            .wrap_horizontal;
        (registry.width, registry.height, wrap)
    };
    for tile in hex_range_tiles(center, radius, width, height, wrap) {
        app.world
            .resource_mut::<GrazeRegistry>()
            .patches
            .insert(tile, GrazePatch::new(tile, LEVELLED_PASTURE_CAPACITY));
    }
}

/// **How far up the ladder a fixture herd stands.** The two managed rungs pay standing yield at
/// different shares, and the wild rung pays none — which is exactly the thing under test, so a
/// fixture states its rung rather than inferring one.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Rung {
    Wild,
    Pastoral,
    Penned,
}

/// Seat one herd of a **real roster species** at `tile`, at `rung`, committed `fraction` of itself
/// to standing output. A roster name is load-bearing here (unlike `grazing_2d_pen`'s deliberately
/// synthetic one): the standing rates are resolved live off the species table by display name.
fn seat_herd(app: &mut App, tile: UVec2, species: &str, rung: Rung, fraction: f32) -> String {
    let ladder = LadderConfig::builtin();
    let (body_mass, capacity, wild_r, fodder) = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let def = fauna
            .species
            .values()
            .find(|def| def.display_name == species)
            .expect("the fixture names a roster species");
        (
            def.body_mass,
            def.biomass[1],
            def.regrowth_rate.expect("a roster species breeds"),
            def.fodder_per_biomass,
        )
    };
    let mut herd = Herd::new(
        "standing_0".to_string(),
        species.to_string(),
        SizeClass::Small,
        vec![tile],
        capacity,
        capacity,
        fodder,
        wild_r,
        body_mass,
    );
    match rung {
        Rung::Wild => {}
        Rung::Pastoral => {
            herd.tame_outright(FactionId(0), &ladder);
        }
        Rung::Penned => {
            herd.tame_outright(FactionId(0), &ladder);
            assert!(
                herd.corral_at(tile, &ladder),
                "{species} must be pennable for this fixture"
            );
            herd.pen_radius = PEN_RADIUS;
        }
    }
    herd.standing_output_fraction = fraction;
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    registry.herds.clear();
    registry.herds.push(herd);
    "standing_0".to_string()
}

/// A keeper band on the herd's tile with one Hunt assignment — the row that both tends and takes.
fn spawn_keeper(app: &mut App, herd_id: &str, tile: UVec2) -> Entity {
    let tile_entity = app
        .world
        .resource::<TileRegistry>()
        .index(tile.x, tile.y)
        .expect("the fixture tile resolves");
    app.world
        .spawn((
            PopulationCohort {
                home: tile_entity,
                current_tile: tile_entity,
                size: 30,
                children: scalar_zero(),
                working: scalar_from_f32((KEEPER_WORKERS * 2) as f32),
                elders: scalar_zero(),
                stores: pen_materials_support::stocked_with_pen_materials(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_turn_food_transfers: Default::default(),
                last_turn_fodder_transfers: Default::default(),
                last_morale_delta: scalar_zero(),
                last_morale_cause: MoraleCause::None,
                last_morale_contributions: Default::default(),
                last_fertility_factors: Default::default(),
                discontent_fraction: scalar_zero(),
                grievance: scalar_zero(),
                last_emigrated: 0,
                last_immigrated: 0,
                age_turns: 0,
                generation: 0 as GenerationId,
                faction: FactionId(0),
                knowledge: Vec::new(),
                migration: None,
            },
            StartingUnit {
                kind: "BandKeeper".to_string(),
                tags: Vec::new(),
            },
            // **A KITTED BAND**, sized to its crew — the seed prices a take at the band's own tier
            // and so does the turn, so a bare-handed fixture would make the two disagree for a
            // reason that has nothing to do with the standing split
            // (`.claude/rules/core_sim/yield-forecast.md`).
            core_sim::BandEquipment::start_stocked_for(
                &core_sim::EquipmentConfig::for_a_stocked_fixture(),
                (KEEPER_WORKERS * 2) as f32,
            ),
            LaborAllocation {
                assignments: vec![
                    LaborAssignment {
                        target: LaborTarget::Hunt {
                            fauna_id: herd_id.to_string(),
                            floor: SUSTAIN_FLOOR,
                        },
                        workers: KEEPER_WORKERS,
                        kit: None,
                        priority: SourcePriority::default(),
                        upkeep_kit: None,
                    },
                    // **The builders' pool, staffed** — a `set_herd_output` commitment is an
                    // ordinary build and is raised from this pool at the head of the band's queue,
                    // exactly as a fence ring is.
                    LaborAssignment {
                        target: LaborTarget::Builders,
                        workers: BUILDERS,
                        kit: None,
                        priority: SourcePriority::default(),
                        upkeep_kit: None,
                    },
                    // **The keeping role, staffed** — the fixture's keeper really is keeping the
                    // herd, and an unstaffed `husbandry` pool reads as total neglect (see
                    // `grazing_2d_pen`'s own note): an accelerating shed would terminate the herd
                    // and there would be nothing left to milk.
                    LaborAssignment {
                        target: LaborTarget::Husbandry,
                        workers: KEEPER_WORKERS,
                        kit: None,
                        priority: SourcePriority::default(),
                        upkeep_kit: None,
                    },
                ],
                ..Default::default()
            },
        ))
        .id()
}

/// One turn in live stage order — Logistics then Population — with the keeper re-stocked first so
/// the pen's feed is always payable and the fixture never measures a starvation.
fn run_turn(app: &mut App, keeper: Entity) {
    app.world
        .get_mut::<PopulationCohort>(keeper)
        .expect("the fixture keeper exists")
        .stores
        .set(FOOD, scalar_from_f32(RESTOCK));
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(advance_herd_grazing);
    app.world.run_system_once(advance_graze_regrowth);
    app.world.run_system_once(advance_husbandry);
    app.world.run_system_once(advance_labor_allocation);
}

/// **A WORLD THAT CAN PUBLISH** — the whole headless app rather than this file's stage-by-stage
/// harness, because a snapshot capture reads two dozen resources the harness does not seat. Every
/// test that asserts on the **wire** builds its fixture here.
fn wire_world(species: &str, rung: Rung, fraction: f32) -> (App, String, Entity) {
    let mut app = core_sim::build_test_app();
    app.update();
    let tile = richest_pasture(&app).0;
    level_footprint_pasture(&mut app, tile, PEN_RADIUS);
    let id = seat_herd(&mut app, tile, species, rung, fraction);
    let keeper = spawn_keeper(&mut app, &id, tile);
    (app, id, keeper)
}

/// One whole headless turn, with the keeper re-stocked first — [`run_turn`]'s twin on [`wire_world`].
fn publishing_turn(app: &mut App, keeper: Entity) {
    app.world
        .get_mut::<PopulationCohort>(keeper)
        .expect("the fixture keeper exists")
        .stores
        .set(FOOD, scalar_from_f32(RESTOCK));
    core_sim::run_turn(app);
}

/// **WHAT THE TURN PAID, off the band's own telemetry row** — the totals every assertion in this
/// file reads, summed over the run so a lumpy per-turn take is compared as a rate rather than as a
/// sample.
#[derive(Default, Debug)]
struct Ledger {
    actual: f32,
    meat: f32,
    standing: f32,
    wasted: f32,
    fibre: f32,
    /// The herd's biomass when the run finished — the *"is it being drawn down"* half.
    final_biomass: f32,
}

/// Run one fixture forward and report what it paid.
fn run_fixture(species: &str, rung: Rung, fraction: f32) -> Ledger {
    let mut app = base_world();
    let (tile, _) = richest_pasture(&app);
    level_footprint_pasture(&mut app, tile, PEN_RADIUS);
    let id = seat_herd(&mut app, tile, species, rung, fraction);
    let keeper = spawn_keeper(&mut app, &id, tile);

    let mut ledger = Ledger::default();
    for _ in 0..TURNS {
        run_turn(&mut app, keeper);
        let allocation = app
            .world
            .get::<LaborAllocation>(keeper)
            .expect("the fixture keeper keeps its allocation");
        let row = allocation
            .last_yields
            .first()
            .expect("the Hunt row is resolved every turn");
        ledger.actual += row.actual;
        ledger.meat += row.meat;
        ledger.standing += row.standing;
        ledger.wasted += row.wasted;
        ledger.fibre += row
            .materials
            .iter()
            .filter(|payoff| payoff.material == FLEECE_MATERIAL)
            .map(|payoff| payoff.amount)
            .sum::<f32>();
    }
    ledger.final_biomass = app
        .world
        .resource::<HerdRegistry>()
        .find(&id)
        .map(|herd| herd.biomass)
        .unwrap_or_default();
    ledger
}

/// **THE DEFAULT CHANGES NOTHING.** `f = 0` is the reading every herd on every map carries until a
/// player pays to change it, so it has to be the behaviour that shipped — and the split has to
/// report the whole row as meat rather than inventing a second line reading `+0.00`.
#[test]
fn an_uncommitted_herd_is_the_behaviour_that_shipped() {
    let pen = run_fixture(DAIRY_SPECIES, Rung::Penned, 0.0);
    assert!(
        pen.actual > 0.0,
        "the fixture pen must actually pay something, else this proves nothing (got {pen:?})"
    );
    assert!(
        agrees(pen.meat, pen.actual),
        "an uncommitted herd's whole row is meat: {pen:?}"
    );
    assert_eq!(
        pen.standing, 0.0,
        "an uncommitted herd gives no standing yield at all: {pen:?}"
    );
    // **The fibre on an uncommitted row is the CARCASS' own sinew**, which is what a hunt has always
    // paid — so the assertion is that the *sheared* half added nothing, not that the row is fibre-free.
    let sheared = run_fixture(FLEECE_SPECIES, Rung::Penned, 0.0);
    assert_eq!(
        sheared.standing, 0.0,
        "an uncommitted fleece herd is shorn of nothing: {sheared:?}"
    );
}

/// **AT `f = 1` THE HERD IS TAKEN FROM NOT AT ALL** — no meat, and (the contract that motivated
/// scaling the take's *ceiling* rather than its result) **no waste**: carry waste is a property of
/// hauling meat home, and there is none on milk.
#[test]
fn a_fully_committed_herd_pays_milk_and_wastes_nothing() {
    let committed = run_fixture(DAIRY_SPECIES, Rung::Penned, 1.0);
    assert_eq!(
        committed.meat, 0.0,
        "a fully committed herd is never slaughtered: {committed:?}"
    );
    assert_eq!(
        committed.wasted, 0.0,
        "there is no carry waste on milk: {committed:?}"
    );
    assert!(
        committed.standing > 0.0,
        "…and it pays the standing rates instead: {committed:?}"
    );
    assert!(
        agrees(committed.actual, committed.standing),
        "the row's total is its standing half alone: {committed:?}"
    );

    // **The herd rides at `K`.** Nothing is drawn off it, so the surplus births are self-limiting —
    // which is the whole reason the arc needs no separate culling mechanism.
    let drawn = run_fixture(DAIRY_SPECIES, Rung::Penned, 0.0);
    assert!(
        committed.final_biomass > drawn.final_biomass,
        "an un-culled herd must stand higher than a harvested one \
         ({} vs {})",
        committed.final_biomass,
        drawn.final_biomass
    );
}

/// **THE STANDING YIELD IS THE DERIVED RATE, NOT A NUMBER THIS TEST INVENTS** — re-struck here from
/// the species' own row and the herd's own head count, so a config retune moves both sides together
/// and only a *model* change can fail it.
#[test]
fn the_standing_payout_is_the_species_rate_times_the_head_count() {
    let mut app = base_world();
    let (tile, _) = richest_pasture(&app);
    level_footprint_pasture(&mut app, tile, PEN_RADIUS);
    let id = seat_herd(&mut app, tile, DAIRY_SPECIES, Rung::Penned, 1.0);
    let keeper = spawn_keeper(&mut app, &id, tile);

    // The head count the turn opens with — the basis the payout is struck on, before the take.
    let (heads, per_head) = {
        let herd = app
            .world
            .resource::<HerdRegistry>()
            .find(&id)
            .expect("the fixture herd exists")
            .clone();
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        (
            core_sim::herd_head_count(herd.biomass, herd.body_mass),
            fauna.standing_yield_for(DAIRY_SPECIES).provisions_per_head,
        )
    };
    assert!(
        per_head > 0.0,
        "{DAIRY_SPECIES} must carry a standing food rate for this fixture to mean anything"
    );

    run_turn(&mut app, keeper);
    let paid = app
        .world
        .get::<LaborAllocation>(keeper)
        .expect("the fixture keeper keeps its allocation")
        .last_yields[0]
        .standing;
    // The band's own output multiplier rides both halves of the yield, so the comparison is a
    // ratio rather than an equality against a bare rate.
    let expected_before_multiplier = per_head * heads;
    assert!(
        paid > 0.0 && (paid / expected_before_multiplier - 1.0).abs() < 0.5,
        "the standing payout must be the per-head rate times the head count, \
         up to the band's output multiplier: paid {paid}, rate x heads {expected_before_multiplier}"
    );
}

/// **A ROAMING HERD IS MILKED OPPORTUNISTICALLY, NOT TWICE DAILY** — the pastoral share, and the
/// reason it is load-bearing rather than a nicety: the two migratory species can never be penned, so
/// a pen-only gate would hand them nothing at all.
#[test]
fn the_pastoral_rung_pays_its_share_and_the_wild_rung_pays_nothing() {
    let share = FaunaConfig::builtin().husbandry.pastoral_standing_fraction;
    assert!(
        (0.0..1.0).contains(&share),
        "this test measures a share BELOW the pen's, and the shipped dial is {share}"
    );

    // The pastoral-only species, at its own top rung. It is the row the share exists for.
    let roaming = run_fixture(PASTORAL_ONLY_SPECIES, Rung::Pastoral, 1.0);
    assert!(
        roaming.standing > 0.0,
        "a species that can never be penned must still be milkable: {roaming:?}"
    );

    // A wild herd of the same species yields nothing renewable however the field is set — you do not
    // milk an animal that runs from you.
    let wild = run_fixture(PASTORAL_ONLY_SPECIES, Rung::Wild, 1.0);
    assert_eq!(
        wild.standing, 0.0,
        "a wild herd gives no standing yield: {wild:?}"
    );

    // And the pen pays MORE per head than the halter, by exactly the dial: same species, same
    // fraction, one rung apart. Measured on a pennable species so both rungs are reachable.
    let penned = run_fixture(DAIRY_SPECIES, Rung::Penned, 1.0);
    let haltered = run_fixture(DAIRY_SPECIES, Rung::Pastoral, 1.0);
    assert!(
        haltered.standing > 0.0 && haltered.standing < penned.standing,
        "a roaming herd yields less than a penned one, and more than nothing: \
         pastoral {} vs pen {}",
        haltered.standing,
        penned.standing
    );
}

/// **AN ABSENT BLOCK NEEDS NO "THIS SPECIES CAN'T" BRANCH.** A pig gives neither milk nor fleece, so
/// committing it produces nothing renewable — and it still gives up the meat the player ordered it
/// to keep, which is the honest consequence of the order rather than a hidden refusal.
#[test]
fn a_species_with_no_standing_block_yields_nothing_renewable_at_any_fraction() {
    for fraction in [0.0, 0.5, 1.0] {
        let ledger = run_fixture(NO_STANDING_SPECIES, Rung::Penned, fraction);
        assert_eq!(
            ledger.standing, 0.0,
            "{NO_STANDING_SPECIES} has no standing_yield block, so it pays none at f={fraction}: \
             {ledger:?}"
        );
    }
    let all_meat = run_fixture(NO_STANDING_SPECIES, Rung::Penned, 0.0);
    let committed = run_fixture(NO_STANDING_SPECIES, Rung::Penned, 1.0);
    assert!(
        all_meat.meat > 0.0 && committed.meat == 0.0,
        "the meat half still obeys the split — the order is real, it just buys nothing here: \
         {all_meat:?} vs {committed:?}"
    );
}

/// **THE FLEECE IS THE SAME GENERIC `fibre` A CARCASS GIVES**, credited through the one seam and
/// **never rounded per turn**: `wild_sheep`'s `0.00588` per head is a sub-unit draw at any plausible
/// flock size for the first turns, and the store crosses whole units by itself.
#[test]
fn a_committed_fleece_herd_is_credited_fibre_without_rounding() {
    let sheared = run_fixture(FLEECE_SPECIES, Rung::Penned, 1.0);
    let unsheared = run_fixture(FLEECE_SPECIES, Rung::Penned, 0.0);
    assert!(
        sheared.fibre > unsheared.fibre,
        "committing a fleece herd must credit more fibre than working it for meat: \
         {} vs {}",
        sheared.fibre,
        unsheared.fibre
    );
    assert_eq!(
        unsheared.fibre.max(0.0),
        unsheared.fibre,
        "the meat row's fibre is the carcass' own sinew and is not negative"
    );
    // The shipped sheep pays milk AND fleece at ONE fraction — only meat trades off, and a shorn
    // sheep is still milked.
    assert!(
        sheared.standing > 0.0 && sheared.fibre > 0.0,
        "one fraction pays both accounts: {sheared:?}"
    );
}

/// **THE SPLIT RIDES THE WIRE.** A client itemizes the row without arithmetic, so the two terms have
/// to be on the published assignment — and they have to still sum to the `actualYield` the larder
/// identity is taken on.
#[test]
fn the_published_row_carries_the_split_and_still_sums_to_the_total() {
    let (mut app, id, keeper) = wire_world(DAIRY_SPECIES, Rung::Penned, COMMITTED_HALF);
    for _ in 0..PUBLISHED_TURNS {
        publishing_turn(&mut app, keeper);
    }

    // **ASSERTED ON THE ENCODED BUFFER, not the in-process row** — a field can be right in
    // `SourceYield` and never reach a client, and the split exists solely so a client can render it.
    recapture_snapshot_in_place(&mut app.world);
    let bytes = {
        let snapshot = app
            .world
            .resource::<SnapshotHistory>()
            .latest_entry()
            .expect("a snapshot was captured")
            .snapshot;
        sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref())
    };
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let snapshot = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot");

    let row = snapshot
        .population()
        .expect("the snapshot carries a population section")
        .populations()
        .expect("the population section carries cohorts")
        .iter()
        .flat_map(|cohort| {
            cohort
                .laborAssignments()
                .into_iter()
                .flat_map(|rows| rows.iter())
        })
        .find(|row| row.kind().unwrap_or_default() == "hunt")
        .expect("the keeper's Hunt row is published");
    assert!(
        row.standingYield() > 0.0,
        "a half-committed dairy pen publishes a standing line: {}",
        row.standingYield()
    );
    assert!(
        row.meatYield() > 0.0,
        "…and still publishes a meat line beside it: {}",
        row.meatYield()
    );
    assert!(
        (row.meatYield() + row.standingYield() - row.actualYield()).abs() < EXACT,
        "the two published halves must sum to the published total: {} + {} != {}",
        row.meatYield(),
        row.standingYield(),
        row.actualYield()
    );

    let herd = snapshot
        .subsistence()
        .and_then(|section| section.herds())
        .expect("the subsistence section carries the herd list")
        .iter()
        .find(|herd| herd.id().unwrap_or_default() == id)
        .expect("the fixture herd is on the wire");
    assert!(
        (herd.standingOutputFraction() - COMMITTED_HALF).abs() < EXACT,
        "the herd's committed fraction rides the wire: {}",
        herd.standingOutputFraction()
    );
    assert_eq!(
        herd.standingOutputTarget(),
        sim_schema::NO_OUTPUT_COMMITMENT_IN_FLIGHT,
        "nothing is in flight, and that is a NEGATIVE sentinel rather than a 0 that means an order"
    );
}

// ---------------------------------------------------------------------------------------------
// Config validation — each bad case refused by name.
// ---------------------------------------------------------------------------------------------

/// Parse the shipped `fauna_config.json` into a mutable value so a test can break exactly one field.
fn shipped_fauna_json() -> serde_json::Value {
    serde_json::from_str(include_str!("../src/data/fauna_config.json"))
        .expect("the shipped fauna config parses")
}

fn validate_broken(mutate: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut json = shipped_fauna_json();
    mutate(&mut json);
    let config: FaunaConfig =
        serde_json::from_value(json).expect("the mutation must stay parseable to reach validate()");
    config
        .validate()
        .expect_err("the broken config must be refused")
        .to_string()
}

#[test]
fn validate_rejects_a_negative_standing_provisions_rate() {
    let message = validate_broken(|json| {
        json["species"]["aurochs"]["standing_yield"]["provisions_per_head"] =
            serde_json::json!(-0.01);
    });
    assert!(
        message.contains("species.aurochs.standing_yield.provisions_per_head"),
        "the refusal must name the field: {message}"
    );
}

#[test]
fn validate_rejects_a_non_finite_standing_material_rate() {
    let message = validate_broken(|json| {
        json["species"]["wild_sheep"]["standing_yield"]["materials"][0]["per_head"] =
            serde_json::json!(-1.0);
    });
    assert!(
        message.contains("species.wild_sheep.standing_yield.materials.per_head"),
        "the refusal must name the field: {message}"
    );
}

#[test]
fn validate_rejects_a_pastoral_standing_fraction_outside_the_unit_range() {
    let message = validate_broken(|json| {
        json["husbandry"]["pastoral_standing_fraction"] = serde_json::json!(1.5);
    });
    assert!(
        message.contains("husbandry.pastoral_standing_fraction"),
        "the refusal must name the dial: {message}"
    );
}

#[test]
fn validate_rejects_a_free_output_recommit() {
    let message = validate_broken(|json| {
        json["husbandry"]["output_recommit_work_fraction"] = serde_json::json!(0.0);
    });
    assert!(
        message.contains("husbandry.output_recommit_work_fraction"),
        "a commitment that costs nothing is the one thing this dial exists to prevent: {message}"
    );
}

/// **A STANDING ROW NAMING A MATERIAL THE TABLE DOES NOT CARRY IS REFUSED** — by the same
/// cross-config check `hunt_yield` already takes, so the two yield edges cannot come to disagree
/// about what a material is.
#[test]
fn validate_rejects_a_standing_row_naming_an_unknown_material() {
    let mut json = shipped_fauna_json();
    json["species"]["wild_sheep"]["standing_yield"]["materials"][0]["material"] =
        serde_json::json!("unobtanium");
    let config: FaunaConfig = serde_json::from_value(json).expect("still parseable");
    let message = config
        .validate_against_materials(&MaterialsConfig::builtin())
        .expect_err("an unknown material must be refused")
        .to_string();
    assert!(
        message.contains("species.wild_sheep.standing_yield") && message.contains("unobtanium"),
        "the refusal must name the row and the material: {message}"
    );
}

/// **THE SHIPPED ROSTER IS COHERENT** — every standing row it carries names a material the table
/// declares, at exactly the axes that material declares. The positive control for the four refusals
/// above, which would all pass against a config that was already broken.
#[test]
fn the_shipped_roster_reconciles_against_the_materials_table() {
    FaunaConfig::builtin()
        .validate_against_materials(&MaterialsConfig::builtin())
        .expect("the shipped roster's yield rows resolve");
    FaunaConfig::builtin()
        .validate()
        .expect("the shipped roster validates");
}

// ---------------------------------------------------------------------------------------------
// The commitment costs work — `SetHerdOutput` through the labor pass.
// ---------------------------------------------------------------------------------------------

/// **A COMMITMENT IS NOT FREE, AND IT IS PRICED OFF THE RUNG** — the ladder's own `work_cost` scaled
/// by `husbandry.output_recommit_work_fraction`, so a rung retune carries it. The expected figure is
/// **re-derived from the shipped config** rather than written down, which makes this a test of the
/// derivation instead of a copy of one number.
///
/// And it lands only when the meter completes: *every commitment costs, including the first*.
#[test]
fn a_commitment_is_priced_off_the_rung_and_lands_when_its_meter_completes() {
    let mut app = base_world();
    let (tile, _) = richest_pasture(&app);
    level_footprint_pasture(&mut app, tile, PEN_RADIUS);
    let id = seat_herd(&mut app, tile, DAIRY_SPECIES, Rung::Penned, 0.0);
    let keeper = spawn_keeper(&mut app, &id, tile);

    let expected_cost = {
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        let share = app
            .world
            .resource::<FaunaConfigHandle>()
            .get()
            .husbandry
            .output_recommit_work_fraction;
        ladder
            .rung(core_sim::RungKey::AnimalPen)
            .build_cost(core_sim::RUNG_COST_UNSCALED)
            .expect("the pen rung has a build meter")
            * share
    };
    assert!(
        expected_cost > 0.0,
        "the shipped ladder must price a commitment at something"
    );

    // Order the change, and queue it on the keeping band exactly as the command handler does.
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|herd| herd.id == id)
            .expect("the fixture herd exists");
        assert!(
            herd.begin_output_recommit(COMMITTED_HALF),
            "a managed herd accepts a real change"
        );
    }
    app.world
        .get_mut::<LaborAllocation>(keeper)
        .expect("the fixture keeper keeps its allocation")
        .enqueue_build(
            core_sim::BuildSource::Herd(id.clone()),
            core_sim::BuildJob::SetHerdOutput(core_sim::RungKey::AnimalPen),
        );

    // One turn stamps the meter with its denominator; the fraction has NOT moved yet.
    run_turn(&mut app, keeper);
    {
        let herd = app
            .world
            .resource::<HerdRegistry>()
            .find(&id)
            .expect("the fixture herd survives");
        assert!(
            (herd.output_recommit_cost - expected_cost).abs() < EXACT * expected_cost.max(1.0),
            "the commitment is priced at the rung's own work_cost x the recommit share: \
             stamped {}, derived {expected_cost}",
            herd.output_recommit_cost
        );
        assert_eq!(
            herd.standing_output_fraction, 0.0,
            "the herd goes on producing what it was until the meter completes"
        );
    }

    // …and it does complete, at which point the ordered fraction becomes the live one.
    for _ in 0..TURNS {
        run_turn(&mut app, keeper);
        let herd = app
            .world
            .resource::<HerdRegistry>()
            .find(&id)
            .expect("the fixture herd survives");
        if herd.standing_output_target.is_none() {
            assert!(
                (herd.standing_output_fraction - COMMITTED_HALF).abs() < EXACT,
                "the order the player paid for is the one that lands: {}",
                herd.standing_output_fraction
            );
            return;
        }
    }
    panic!("the commitment never completed within {TURNS} turns of a fully staffed band");
}

// ---------------------------------------------------------------------------------------------
// The pre-commit row and the resolved row are ONE number.
// ---------------------------------------------------------------------------------------------

/// **THE SEED IS WHAT THE PLAYER READS WHILE DECIDING**, so a committed herd whose pre-commit row
/// quoted meat alone would say *"this produces nothing"* about the very thing it is asking them to
/// staff — and the seed would then jump when the turn resolved.
/// `.claude/rules/core_sim/yield-forecast.md`'s `forecast == actual` is the invariant that forbids
/// it, and this is that invariant re-asserted across the split.
///
/// **On the ENCODED buffer at both ends**: the seeded row has to *reach* the client for any of this
/// to matter, so both readings come off the wire rather than out of `SourceYield`.
fn published_hunt_row(app: &mut App) -> PublishedRow {
    recapture_snapshot_in_place(&mut app.world);
    let bytes = {
        let snapshot = app
            .world
            .resource::<SnapshotHistory>()
            .latest_entry()
            .expect("a snapshot was captured")
            .snapshot;
        sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref())
    };
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let row = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .expect("the snapshot carries a population section")
        .populations()
        .expect("the population section carries cohorts")
        .iter()
        .flat_map(|cohort| {
            cohort
                .laborAssignments()
                .into_iter()
                .flat_map(|rows| rows.iter())
        })
        .find(|row| row.kind().unwrap_or_default() == "hunt")
        .expect("the keeper's Hunt row is published");
    PublishedRow {
        actual: row.actualYield(),
        workers_needed: row.workersNeeded(),
        meat: row.meatYield(),
        standing: row.standingYield(),
        low: row.actualYieldLow(),
        high: row.actualYieldHigh(),
    }
}

/// One assignment row **as the client receives it** — the five figures the seed/resolved comparison
/// is made on, read off the encoded buffer rather than out of `SourceYield`.
#[derive(Debug, Clone, Copy)]
struct PublishedRow {
    actual: f32,
    workers_needed: u32,
    meat: f32,
    standing: f32,
    low: f32,
    high: f32,
}

/// Stamp the assign-time seed onto the keeper's Hunt row — the very seam `handle_assign_labor` uses
/// (`fauna::hunt_source_yield_preview` → `LaborAllocation::set_source_yield`).
///
/// # ⛔ EVERY TERM IS RESOLVED THE WAY THE LABOR ARM RESOLVES IT
///
/// The haul tier through this crew's own **coverage**, the fighting party through
/// [`core_sim::PartyResolution`] against **this quarry's mass**, and the productivity through
/// [`core_sim::output_multiplier`] on this band's own cohort. A seed priced at a reference tier
/// instead would disagree with the turn for a reason that has nothing to do with the standing split
/// — and the disagreement lands as a whole animal, which is exactly the kind of near-miss that would
/// make this test look like a model defect.
fn seed_the_hunt_row(app: &mut App, keeper: Entity, herd_id: &str) {
    let labor = app.world.resource::<LaborConfigHandle>().get().clone();
    let equipment = app
        .world
        .resource::<core_sim::EquipmentConfigHandle>()
        .get()
        .clone();
    let combat = app
        .world
        .resource::<core_sim::CombatConfigHandle>()
        .get()
        .clone();
    let creatures = app
        .world
        .resource::<core_sim::CreaturesConfigHandle>()
        .get()
        .clone();
    let wellbeing = app.world.resource::<WellbeingConfigHandle>().get().clone();
    let fauna = app.world.resource::<FaunaConfigHandle>().get().clone();
    let sigmas = combat.forecast_range_sigmas;

    let herd = app
        .world
        .resource::<HerdRegistry>()
        .find(herd_id)
        .expect("the fixture herd survives")
        .clone();
    let band_kit = app
        .world
        .get::<core_sim::BandEquipment>(keeper)
        .cloned()
        .unwrap_or_default();
    let cohort = app
        .world
        .get::<PopulationCohort>(keeper)
        .expect("the fixture keeper exists");
    let output_mult = core_sim::output_multiplier(cohort, &wellbeing).to_f32();
    let allocation = app
        .world
        .get::<LaborAllocation>(keeper)
        .expect("the fixture keeper keeps its allocation");
    let assignment = allocation
        .assignments
        .iter()
        .find(|assignment| matches!(assignment.target, LaborTarget::Hunt { .. }))
        .expect("the fixture keeper hunts");
    let LaborTarget::Hunt { floor, .. } = assignment.target else {
        unreachable!("filtered above")
    };
    let workers = assignment.workers;
    let crew_kit = assignment.kit_choice(&equipment);
    let crew_coverage = equipment.coverage(&crew_kit, workers as f32, &band_kit);
    let per_worker = crew_coverage.weighted_rate(|kit| {
        equipment.hunt_per_worker_biomass_capacity(
            labor.hunt.per_worker_biomass_capacity,
            kit,
            &band_kit,
        )
    });
    let party = core_sim::PartyResolution {
        equipment: &equipment,
        coverage: &crew_coverage,
        wear: &band_kit,
        intrinsic: creatures.person(),
        tuning: combat.tuning(),
        hunt_injury_damage_per_animal: combat.hunt_injury_damage_per_animal,
    }
    .party_against(core_sim::Quarry::Mass(herd.body_mass));

    let seeded = core_sim::hunt_source_yield_preview(
        &herd,
        &fauna,
        per_worker,
        &party,
        output_mult,
        workers,
        floor,
        labor.yield_average_horizon_turns,
        labor.arrivals_horizon_turns,
        sigmas,
    );
    let target = LaborTarget::Hunt {
        fauna_id: herd_id.to_string(),
        floor,
    };
    app.world
        .get_mut::<LaborAllocation>(keeper)
        .expect("the fixture keeper keeps its allocation")
        .set_source_yield(&target, seeded);
}

/// **THE SEEDED ROW AND THE FIRST RESOLVED ROW ARE ONE READING, AT EVERY COMMITMENT** — the property
/// the forecast widening exists to restore, swept at the two endpoints and the midpoint because each
/// exercises a different side: at `0` the row is all meat (and must stay byte-identical), at `1` it
/// is all milk, and at `0.5` both terms are live at once.
///
/// # What "one reading" means on each half, and why they differ
///
/// `.claude/rules/core_sim/yield-forecast.md` restates `forecast == actual` as a **distribution**:
/// a hunt has two stochastic stages, the quarry's retreat and the fight's per-unit rolls, and a
/// forecast physically cannot draw the seed a future tick will. So the meat half is pinned as
/// *"the resolved take lies inside the band the seed published"*, which is the promise the wire's
/// `actualYieldLow`/`High` make.
///
/// **The standing half has no such excuse and is pinned EXACTLY.** Nothing about milk is stochastic:
/// it is a per-head rate times a head count, so seed and resolved must agree to the float. That is
/// the assertion this test exists for — and at `f = 1` there is no meat and therefore no stochastic
/// stage left at all, so the whole row is pinned exactly there.
#[test]
fn the_seeded_row_equals_the_first_resolved_row_at_every_commitment() {
    for fraction in [0.0, COMMITTED_HALF, 1.0] {
        let (mut app, id, keeper) = wire_world(FLEECE_SPECIES, SEED_COMPARISON_RUNG, fraction);
        // Settle first, so the comparison is not made across the fixture's opening transient.
        for _ in 0..SETTLE_TURNS {
            publishing_turn(&mut app, keeper);
        }

        // **The seed, struck exactly where `assign_labor` strikes it**: after a turn resolved,
        // pricing the turn to come.
        seed_the_hunt_row(&mut app, keeper, &id);
        let seeded = published_hunt_row(&mut app);

        // **THE TWO STAGES THE FORECAST MODELS, AND ONLY THOSE** — Logistics (the keeping bill, then
        // `fauna::next_turns_quarry`, which is literally the transformation `hunt_forecast` applies
        // to its private clone) then the Population take. This is the shape `forecast == actual` is
        // *defined* over and the shape `labor`'s own sweep asserts it on; a whole `run_turn` would
        // also roam the herd and re-derive its range's `K`, neither of which a pre-commit quote
        // simulates or claims to.
        app.world.run_system_once(advance_husbandry);
        {
            let fauna = app.world.resource::<FaunaConfigHandle>().get().clone();
            for herd in app.world.resource_mut::<HerdRegistry>().herds.iter_mut() {
                *herd = core_sim::next_turns_quarry(herd, &fauna);
            }
        }
        app.world
            .get_mut::<PopulationCohort>(keeper)
            .expect("the fixture keeper exists")
            .stores
            .set(FOOD, scalar_from_f32(RESTOCK));
        app.world.run_system_once(advance_labor_allocation);
        let resolved = published_hunt_row(&mut app);

        assert!(
            (seeded.standing - resolved.standing).abs() < EXACT,
            "the standing half is not stochastic, so seed and resolved must agree exactly \
             at f={fraction}: seeded {seeded:?}, resolved {resolved:?}"
        );
        assert!(
            seeded.low <= resolved.actual + EXACT && resolved.actual <= seeded.high + EXACT,
            "the resolved take must land inside the band the seed published at f={fraction}: \
             seeded {seeded:?}, resolved {resolved:?}"
        );
        assert!(
            (seeded.meat + seeded.standing - seeded.actual).abs() < EXACT,
            "the seeded row's two halves must sum to its total: {seeded:?}"
        );
        if fraction == 0.0 {
            assert!(
                seeded.actual > 0.0 && seeded.standing == 0.0,
                "the f=0 case has to be a real, positive, all-meat take or it proves nothing: \
                 {seeded:?}"
            );
        }
        if fraction == 1.0 {
            // **No meat, therefore no stochastic stage** — the row is milk alone, so the whole of it
            // is pinned to the float. This is the case that published `0 ± 0` before the widening.
            assert!(
                seeded.standing > 0.0,
                "a fully committed herd must quote its milk before the turn, not after: {seeded:?}"
            );
            assert!(
                (seeded.actual - resolved.actual).abs() < EXACT
                    && seeded.low == seeded.actual
                    && seeded.high == seeded.actual,
                "with nothing left to be uncertain about, seed and resolved are one number: \
                 seeded {seeded:?}, resolved {resolved:?}"
            );
        }
    }
}

/// **THE TAKE CREW'S STRUCTURAL MINIMUM** — `fauna::peak_animal_drop` is `floor(room / body) + 1`,
/// and the `+ 1` is the partial body a turn's regrowth could tip over, so even an empty room asks
/// for one hauler. It is what a fully committed herd's row reports, and it is a fact about that
/// seam rather than about the standing split.
const PARTIAL_BODY_CREW: u32 = 1;

/// **NO NUMBER OF EXTRA HANDS INCREASES MILK**, so the staffing signals answer for the meat side
/// alone — the crew question is *"would another pair of hands bring more home"*, and a standing
/// yield is not brought home by anybody.
///
/// At `f = 1` there is no cull at all, so the row asks for **no take crew** while still paying every
/// turn. That is the honest reading: the herd wants *keepers* (its own `upkeepWorkersNeeded`, a
/// different field in a different unit), not haulers.
#[test]
fn the_take_crew_answers_for_the_meat_side_alone() {
    let (mut app, _, keeper) = wire_world(FLEECE_SPECIES, SEED_COMPARISON_RUNG, 0.0);
    for _ in 0..SETTLE_TURNS {
        publishing_turn(&mut app, keeper);
    }
    let all_meat = published_hunt_row(&mut app);

    let (mut app, _, keeper) = wire_world(FLEECE_SPECIES, SEED_COMPARISON_RUNG, 1.0);
    for _ in 0..SETTLE_TURNS {
        publishing_turn(&mut app, keeper);
    }
    let all_milk = published_hunt_row(&mut app);

    assert!(
        all_meat.workers_needed > 0,
        "the uncommitted control must want a real take crew, or the comparison proves nothing: \
         {all_meat:?}"
    );
    assert_eq!(
        all_milk.workers_needed, PARTIAL_BODY_CREW,
        "a herd nothing is ever taken from asks for the take crew's structural MINIMUM, however \
         much milk it gives — `fauna::peak_animal_drop`'s `+ 1` is the partial body a turn's \
         regrowth could tip over, and it is the whole of what is left here: {all_milk:?}"
    );
    assert!(
        all_milk.workers_needed <= all_meat.workers_needed,
        "committing a herd may only ever SHRINK its take crew — the milk is not hauled, so it can \
         never add a hand: {all_meat:?} vs {all_milk:?}"
    );
    assert!(
        all_milk.standing > 0.0 && all_milk.meat == 0.0,
        "…and it is genuinely paying, which is what makes the zero crew a finding rather than an \
         empty row: {all_milk:?}"
    );
}
