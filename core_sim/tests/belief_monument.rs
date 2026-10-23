//! **THE BELIEF BRANCH** (#692, `.claude/rules/core_sim/belief.md`, `intensification.md` -> "The
//! tile kind"): a gathering ground becomes a monument. The monument is an ordinary tile rung - its
//! build, crew keeping, upkeep, neglect, grace and decay are the storage pit's machinery - and the
//! only new mechanisms are the four under test here:
//!
//! - a tile rung may carry a BUILD pile (20 stone, drawn as the meter climbs);
//! - `site_requirement.min_belief`, judged through the one `rung_site_refusal` seam;
//! - gatherings teach `ritual`, which gates the rung;
//! - a held monument adds belief to its own tile (`belief_config.json` -> `improvement_belief`).
//!
//! Everything runs through the real systems against the SHIPPED config.

mod faction_support;

use std::collections::BTreeMap;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};

use core_sim::{
    advance_connections, advance_improvement_belief, build_test_app, knows, rung_site_refusal,
    scalar_from_f32, scalar_one, tile_rung_span, BandId, BeliefConfigHandle, BeliefRegistry,
    BranchId, BuildJob, BuildSource, ConnectionKey, ConnectionLedger, ConnectionsConfig,
    ContactsThisTurn, DiscoveryProgressLedger, FactionId, LaborAllocation, LaborConfigHandle,
    LaborTarget, LadderConfigHandle, PopulationCohort, ResidentBand, RungId, Sighting,
    SimulationConfig, SimulationTick, SiteRefusal, Tile, TileImprovementRegistry, TileRegistry,
    ViewerFaction, NO_BELIEF, NO_DEPOSIT_FLOOR, RITUAL_DISCOVERY_ID,
};
use faction_support::{two_faction_world, HOME, RIVAL};

const MAP_SEED: u64 = 119_304_647;
const BUILDERS: u32 = 5;
const CREW_HANDS: u32 = 2;
const ALL_HANDS: u32 = BUILDERS + CREW_HANDS;
const TURN_BUDGET: usize = 80;
/// Stone in the store when the monument is raised: well over the rung's pile.
const STONE_STOCK: f32 = 30.0;
const STONE: &str = "stone";
/// The keeping class and size of the fixture band's larder.
const LARDER_CLASS: &str = "dry";
const LARDER: f32 = 100_000.0;
const PILE_EPSILON: f32 = 1.0e-2;
/// Quiet turns that drain a fresh tie to parked (`gathering.rs`'s figure).
const QUIET_TURNS: u64 = 20;
const ESTABLISHED_AGE: u32 = 5;
const FIRST_TURN: u64 = 5;
/// The gatherings that teach ritual at the shipped costs (`ritual_lesson_per_gathering` 4 / cost 20).
const GATHERINGS_TO_LEARN: usize = 5;
/// Turns the payoff is read over.
const PAYOFF_TURNS: usize = 7;
const TOLERANCE: f32 = 1.0e-4;

fn belief_branch() -> BranchId {
    BranchId::new("belief")
}
fn gathering_ground() -> RungId {
    RungId::new(belief_branch(), "gathering_ground")
}
fn monument() -> RungId {
    RungId::new(belief_branch(), "monument")
}
fn monument_row(tile: UVec2) -> LaborTarget {
    LaborTarget::Tile {
        tile,
        branch: belief_branch(),
    }
}

fn campaign_world() -> App {
    let mut app = build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = MAP_SEED;
    app.world.insert_resource(config);
    app.update();
    app
}

fn first_band(app: &mut App) -> (Entity, FactionId, UVec2) {
    let (entity, faction, tile) = {
        let mut query = app
            .world
            .query_filtered::<(Entity, &PopulationCohort), With<ResidentBand>>();
        let (entity, cohort) = query.iter(&app.world).next().expect("a resident band");
        (entity, cohort.faction, cohort.current_tile)
    };
    let position = app.world.get::<Tile>(tile).unwrap().position;
    app.world.insert_resource(ViewerFaction(faction));
    (entity, faction, position)
}

fn learn_ritual(app: &mut App, faction: FactionId) {
    app.world
        .resource_mut::<DiscoveryProgressLedger>()
        .add_progress(faction, RITUAL_DISCOVERY_ID, scalar_one());
}

fn stock_stone(app: &mut App, band: Entity, units: f32) {
    let materials = core_sim::MaterialsConfig::builtin();
    let characteristics: BTreeMap<String, f32> = materials
        .material(STONE)
        .expect("the shipped roster carries stone")
        .characteristics
        .iter()
        .map(|axis| (axis.clone(), core_sim::OPENING_MATERIAL_READING))
        .collect();
    let key = materials
        .band_key(STONE, &characteristics)
        .expect("the shipped roster rates stone");
    app.world
        .get_mut::<PopulationCohort>(band)
        .unwrap()
        .stores
        .deposit_material(
            STONE,
            key,
            core_sim::scalar_from_f32(units),
            &characteristics,
        );
}

fn stone_held(app: &App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .unwrap()
        .stores
        .material_total(STONE)
        .to_f32()
}

fn belief_at(app: &App, tile: UVec2) -> f32 {
    app.world.resource::<BeliefRegistry>().get(tile)
}

/// Place the gathering ground, stand the builders on the queue head and crew the tile.
fn begin_the_monument(app: &mut App, entity: Entity, faction: FactionId, tile: UVec2) {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    app.world.resource_mut::<TileImprovementRegistry>().place(
        tile,
        belief_branch(),
        faction,
        &ladder,
    );
    let mut allocation = LaborAllocation::default();
    allocation.set_assignment(LaborTarget::Builders, BUILDERS, ALL_HANDS, None);
    allocation.set_assignment(monument_row(tile), CREW_HANDS, ALL_HANDS, None);
    allocation.enqueue_build(
        BuildSource::Tile {
            tile,
            branch: belief_branch(),
        },
        BuildJob::Climb(monument()),
    );
    app.world.entity_mut(entity).insert(allocation);
}

fn held(app: &App, tile: UVec2) -> RungId {
    app.world
        .resource::<TileImprovementRegistry>()
        .get(tile, belief_branch())
        .expect("the ground was placed")
        .held_rung()
}

fn position(app: &App, tile: UVec2) -> f32 {
    app.world
        .resource::<TileImprovementRegistry>()
        .get(tile, belief_branch())
        .expect("the ground was placed")
        .position()
}

fn run_until(app: &mut App, what: &str, done: impl Fn(&App) -> bool) {
    for _ in 0..TURN_BUDGET {
        if done(app) {
            return;
        }
        app.update();
    }
    panic!("{what} did not happen within {TURN_BUDGET} turns");
}

/// A campaign band that has learned ritual, stands on `tile` holding `belief`, with the ground placed.
fn monument_scene(belief: f32) -> (App, Entity, FactionId, UVec2) {
    scene(belief, true)
}

fn scene(belief: f32, ritual: bool) -> (App, Entity, FactionId, UVec2) {
    let mut app = campaign_world();
    let (entity, faction, tile) = first_band(&mut app);
    // A larder far past what the scenario eats, so the crews are not shed to hunger mid-build.
    app.world
        .get_mut::<PopulationCohort>(entity)
        .unwrap()
        .stores
        .add_food_aged(LARDER_CLASS, scalar_from_f32(LARDER), 0);
    *app.world.resource_mut::<BeliefRegistry>() = BeliefRegistry::default();
    app.world.resource_mut::<BeliefRegistry>().add(tile, belief);
    if ritual {
        learn_ritual(&mut app, faction);
    }
    begin_the_monument(&mut app, entity, faction, tile);
    (app, entity, faction, tile)
}

/// **A MONUMENT BUILD STALLS WITH NO STONE AND COMPLETES WITH IT, DRAWING THE PILE.**
#[test]
fn a_monument_stalls_without_stone_and_completes_drawing_its_pile() {
    let (mut app, entity, _, tile) = monument_scene(1.0);
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let pile = *ladder
        .rung(monument())
        .build
        .as_ref()
        .and_then(|build| build.materials.get(STONE))
        .expect("the monument declares a pile of stone");

    for _ in 0..6 {
        app.update();
    }
    assert_eq!(held(&app, tile), gathering_ground());
    assert_eq!(
        position(&app, tile),
        0.0,
        "a dry store banks nothing toward the monument"
    );

    stock_stone(&mut app, entity, STONE_STOCK);
    run_until(&mut app, "the monument going up", |app| {
        held(app, tile) == monument()
    });
    assert!(
        (stone_held(&app, entity) - (STONE_STOCK - pile)).abs() < PILE_EPSILON,
        "the climb drew {pile} stone and no more: {} left of {STONE_STOCK}",
        stone_held(&app, entity)
    );
}

/// **THE MONUMENT IS REFUSED ON A TILE HOLDING TOO LITTLE BELIEF AND RAISED ON ONE HOLDING ENOUGH**
/// - through the labor arm's gate, and on the one `rung_site_refusal` seam the command also reads.
#[test]
fn a_monument_needs_a_tile_that_holds_belief() {
    const SHORT_OF_ONE: f32 = 0.5;
    let (mut app, entity, _, tile) = monument_scene(SHORT_OF_ONE);
    stock_stone(&mut app, entity, STONE_STOCK);

    // The seam, asked directly.
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let ground = {
        let entity = app
            .world
            .resource::<TileRegistry>()
            .index(tile.x, tile.y)
            .unwrap();
        app.world.get::<Tile>(entity).unwrap().clone()
    };
    let ask = |belief: f32| {
        rung_site_refusal(
            ladder.rung(monument()),
            &ground,
            &labor.forage,
            true,
            true,
            NO_DEPOSIT_FLOOR,
            belief,
            &|_| false,
        )
    };
    assert_eq!(ask(SHORT_OF_ONE), Some(SiteRefusal::NoBelief));
    assert_eq!(ask(1.0), None);

    // The arm. Deaths on the band's tile would credit it belief, so the stock is held at the short
    // reading each turn.
    for _ in 0..6 {
        *app.world.resource_mut::<BeliefRegistry>() = BeliefRegistry::default();
        app.world
            .resource_mut::<BeliefRegistry>()
            .add(tile, SHORT_OF_ONE);
        app.update();
    }
    *app.world.resource_mut::<BeliefRegistry>() = BeliefRegistry::default();
    app.world
        .resource_mut::<BeliefRegistry>()
        .add(tile, SHORT_OF_ONE);
    assert_eq!(
        position(&app, tile),
        0.0,
        "half a dead-equivalent is not a gathering ground"
    );
    app.world
        .resource_mut::<BeliefRegistry>()
        .add(tile, 1.0 - SHORT_OF_ONE);
    run_until(&mut app, "the monument going up", |app| {
        held(app, tile) == monument()
    });
}

/// **WITHOUT RITUAL THE MONUMENT IS NOT RAISED.**
#[test]
fn a_monument_waits_on_ritual() {
    let (mut app, entity, faction, tile) = scene(1.0, false);
    stock_stone(&mut app, entity, STONE_STOCK);
    for _ in 0..6 {
        app.update();
    }
    assert_eq!(
        position(&app, tile),
        0.0,
        "an unlearned lesson banks nothing"
    );
    learn_ritual(&mut app, faction);
    run_until(&mut app, "the monument going up", |app| {
        held(app, tile) == monument()
    });
}

/// **A HELD MONUMENT ADDS `per_turn` TO ITS TILE EACH TURN; A HALF-BUILT ONE AND A BARE GROUND ADD
/// NOTHING.** Driven through the system alone, so no death or gathering moves the stock.
#[test]
fn a_held_monument_adds_belief_and_nothing_else_does() {
    let mut app = build_test_app();
    app.update();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let per_turn = app
        .world
        .resource::<BeliefConfigHandle>()
        .get()
        .improvement_belief
        .iter()
        .find(|entry| entry.rung == monument())
        .expect("the shipped config names the monument")
        .per_turn;
    *app.world.resource_mut::<BeliefRegistry>() = BeliefRegistry::default();

    let (built, half, bare) = (UVec2::new(1, 1), UVec2::new(2, 1), UVec2::new(3, 1));
    {
        let (base, width) = tile_rung_span(monument(), &ladder);
        let mut registry = app.world.resource_mut::<TileImprovementRegistry>();
        registry
            .place(built, belief_branch(), HOME, &ladder)
            .set_position(base + width, &ladder);
        registry
            .place(half, belief_branch(), HOME, &ladder)
            .set_position(base + width * 0.5, &ladder);
        registry.place(bare, belief_branch(), HOME, &ladder);
    }
    for _ in 0..PAYOFF_TURNS {
        app.world.run_system_once(advance_improvement_belief);
    }
    let expected = per_turn * PAYOFF_TURNS as f32;
    assert!(
        (belief_at(&app, built) - expected).abs() < TOLERANCE,
        "{} vs {expected}",
        belief_at(&app, built)
    );
    assert_eq!(
        belief_at(&app, half),
        NO_BELIEF,
        "on_completion: no half credit"
    );
    assert_eq!(
        belief_at(&app, bare),
        NO_BELIEF,
        "the ground itself pays nothing"
    );
}

/// **AN UNKEPT MONUMENT DECAYS TO THE GROUND AFTER ITS GRACE, AND THE TILE'S BELIEF DOES NOT DROP.**
#[test]
fn an_unkept_monument_decays_and_the_belief_it_added_stays() {
    let (mut app, entity, _, tile) = monument_scene(1.0);
    stock_stone(&mut app, entity, STONE_STOCK);
    run_until(&mut app, "the monument going up", |app| {
        held(app, tile) == monument()
    });
    for _ in 0..4 {
        app.update();
    }
    assert_eq!(held(&app, tile), monument(), "a kept monument holds");

    let before = belief_at(&app, tile);
    app.world
        .get_mut::<LaborAllocation>(entity)
        .unwrap()
        .set_assignment(monument_row(tile), 0, ALL_HANDS, None);
    run_until(&mut app, "the monument decaying", |app| {
        held(app, tile) == gathering_ground()
    });
    assert!(
        belief_at(&app, tile) >= before,
        "belief is monotone: {} after, {before} before",
        belief_at(&app, tile)
    );
}

// ---------------------------------------------------------------------------------------------
// Ritual: learned by gatherings
// ---------------------------------------------------------------------------------------------

struct Band {
    entity: Entity,
    id: BandId,
    tile: UVec2,
}

fn pair(app: &mut App) -> (Band, Band) {
    let mut bands: Vec<(Entity, BandId, FactionId)> = app
        .world
        .query_filtered::<(Entity, &BandId, &PopulationCohort), With<ResidentBand>>()
        .iter(&app.world)
        .map(|(entity, id, cohort)| (entity, *id, cohort.faction))
        .collect();
    bands.sort_by_key(|(_, id, _)| *id);
    let pick = |faction: FactionId| *bands.iter().find(|(_, _, f)| *f == faction).unwrap();
    let mut out = Vec::new();
    for (entity, id, _) in [pick(HOME), pick(RIVAL)] {
        let tile_entity = app
            .world
            .get::<PopulationCohort>(entity)
            .unwrap()
            .current_tile;
        let tile = app.world.get::<Tile>(tile_entity).unwrap().position;
        app.world
            .get_mut::<PopulationCohort>(entity)
            .unwrap()
            .age_turns = ESTABLISHED_AGE;
        out.push(Band { entity, id, tile });
    }
    let b = out.pop().unwrap();
    let a = out.pop().unwrap();
    let _ = (a.entity, b.entity);
    (a, b)
}

fn gather(app: &mut App, turn: u64, a: &Band, b: &Band) {
    app.world.resource_mut::<SimulationTick>().0 = turn;
    let faction_of = |band: &Band, app: &App| {
        app.world
            .get::<PopulationCohort>(band.entity)
            .unwrap()
            .faction
    };
    let (fa, fb) = (faction_of(a, app), faction_of(b, app));
    {
        let mut contacts = app.world.resource_mut::<ContactsThisTurn>();
        for (observer, subject, at, faction) in [(a, b, b.tile, fb), (b, a, a.tile, fa)] {
            contacts.record(
                observer.id,
                subject.id,
                Sighting::new(at, turn, "seen", faction),
            );
        }
    }
    app.world.run_system_once(advance_connections);
    // Let the tie fade to parked so the next meeting is a new gathering.
    let cfg = ConnectionsConfig::default();
    for later in turn + 1..=turn + QUIET_TURNS {
        app.world
            .resource_mut::<ConnectionLedger>()
            .decay_all(later, &cfg);
    }
    assert!(!app
        .world
        .resource::<ConnectionLedger>()
        .tie_is_live(a.id, b.id));
    let _ = ConnectionKey::new(a.id, b.id);
}

fn knows_ritual(app: &App, faction: FactionId) -> bool {
    let threshold = app
        .world
        .resource::<LadderConfigHandle>()
        .get()
        .knowledge
        .completion_threshold;
    knows(
        app.world.resource::<DiscoveryProgressLedger>(),
        faction,
        RITUAL_DISCOVERY_ID,
        threshold,
    )
}

/// **FIVE GATHERINGS TEACH RITUAL TO BOTH PEOPLES; FOUR DO NOT.**
#[test]
fn five_gatherings_teach_ritual_and_four_do_not() {
    let mut app = two_faction_world();
    let (a, b) = pair(&mut app);
    let mut turn = FIRST_TURN;
    for held in 1..=GATHERINGS_TO_LEARN {
        assert!(
            !knows_ritual(&app, HOME) && !knows_ritual(&app, RIVAL),
            "{} gatherings in, ritual is not yet learned",
            held - 1
        );
        gather(&mut app, turn, &a, &b);
        turn += QUIET_TURNS + 1;
        if held < GATHERINGS_TO_LEARN {
            assert!(
                !knows_ritual(&app, HOME) && !knows_ritual(&app, RIVAL),
                "{held} gatherings are not enough"
            );
        }
    }
    assert!(
        knows_ritual(&app, HOME),
        "five gatherings teach the first people"
    );
    assert!(knows_ritual(&app, RIVAL), "and the second");
}
