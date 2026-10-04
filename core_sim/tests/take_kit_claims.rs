//! **A TAKE-KIT CLAIM COUNTS ONLY THE HANDS THAT WOULD TAKE SOMETHING WITH IT**
//! (`docs/plan_site_crews.md` §2.3) — seen where the player sees it, on the encoded `kitToe` lines.
//!
//! The reported scene: a band with four baskets and two Normal forage rows — a rich wild stand
//! worked by two, and a thin kept stand worked by three. The player raised the thin row to four, a
//! basket moved off the rich row, and its shortage mark lit; dropping back to three put it back. The
//! settlement was splitting the baskets by **head count**, so a hand the thin stand could not use
//! still pulled a basket off a row that could. The claim is now planned as if equipped — the take
//! hands that reach the site's yield at the row's floor — so the useless hand claims nothing and the
//! split does not move when the player steps the crew.
//!
//! **Asserted on the ENCODED envelope**, because the mark the player watched is read off the wire.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::Entity;

use core_sim::{
    advance_labor_allocation, build_test_app, recapture_snapshot_in_place, scalar_from_f32,
    scalar_one, scalar_zero, BandEquipment, EquipmentConfig, FactionId, ForageRegistry,
    GenerationId, LaborAllocation, LaborConfigHandle, LaborTarget, LocalStore, MoraleCause,
    PopulationCohort, ResidentBand, SnapshotHistory, StartingUnit, TakeSelection, TileRegistry,
    DEFAULT_ESCAPEMENT_FLOOR,
};

/// The take kit's one item on the default gathering kit.
const BASKETS: &str = "baskets";

/// The scene's stock: four baskets.
const SCENE_BASKETS: u32 = 4;

/// The rich wild stand's crew — every hand on it takes something with a basket.
const RICH_CREW: u32 = 2;

/// The thin stand's crew before and after the player's raise.
const THIN_CREW: u32 = 3;
const RAISED_THIN_CREW: u32 = 4;

/// **The thin stand's floor** — high, so the room a crew may take there is a sliver of the stand and
/// a hand or so with a basket reaches all of it.
const THIN_FLOOR: f32 = 0.9;

/// Hands the band has — enough to staff both rows at the raised crew and leave some idle.
const BAND_WORKERS: u32 = 10;

// **The scene's premise**: the raised head counts outrun the stock, so a head-count split is short.
const _: () = assert!(RICH_CREW + RAISED_THIN_CREW > SCENE_BASKETS);

/// One basket for a band whose rich row alone claims two — a genuine shortage.
const SHORT_BASKETS: u32 = 1;

/// A forage target on `tile` at `floor`.
fn forage_on(tile: UVec2, floor: f32) -> LaborTarget {
    LaborTarget::Forage {
        tile,
        floor,
        species: None,
        take_species: TakeSelection::EVERYTHING,
    }
}

/// **Two patches one band can work at once** — the home patch and the nearest other inside
/// `labor_config.band_work_range`, so neither row posts a work party.
fn two_worked_patches(app: &App) -> (UVec2, UVec2) {
    let range = app
        .world
        .resource::<LaborConfigHandle>()
        .get()
        .band_work_range;
    let config = app.world.resource::<core_sim::SimulationConfig>();
    let width = config.grid_size.x;
    let wrap = config.map_topology.wrap_horizontal;
    let mut patches: Vec<UVec2> = app
        .world
        .resource::<ForageRegistry>()
        .patches
        .keys()
        .copied()
        .collect();
    // The registry iterates in hash order; sort so the pair is the same on every run.
    patches.sort_by_key(|tile| (tile.y, tile.x));
    for home in &patches {
        if let Some(other) = patches.iter().find(|tile| {
            *tile != home
                && core_sim::grid_utils::hex_distance_wrapped(*home, **tile, width, wrap) <= range
        }) {
            return (*home, *other);
        }
    }
    panic!("worldgen seeded no two forage patches within one band's work range");
}

/// **The scene**: a band on `rich` holding `baskets`, two on the full stand at `rich` and three on
/// the thin stand at `thin`, which stands exactly on its high floor.
fn the_scene(baskets: u32) -> (App, Entity, UVec2, UVec2) {
    let mut app = build_test_app();
    // One `update()` runs the whole Startup worldgen chain, which seeds the patches and the registry.
    app.update();
    let (rich, thin) = two_worked_patches(&app);
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        let patch = registry
            .patches
            .get_mut(&rich)
            .expect("the rich patch exists");
        patch.biomass = patch.carrying_capacity;
        let patch = registry
            .patches
            .get_mut(&thin)
            .expect("the thin patch exists");
        patch.biomass = patch.carrying_capacity * THIN_FLOOR;
    }
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(rich.x, rich.y)
        .expect("the rich patch resolves to a tile");
    let mut allocation = LaborAllocation::default();
    allocation.set_assignment(
        forage_on(rich, DEFAULT_ESCAPEMENT_FLOOR),
        RICH_CREW,
        BAND_WORKERS,
        None,
    );
    allocation.set_assignment(forage_on(thin, THIN_FLOOR), THIN_CREW, BAND_WORKERS, None);
    let tier = EquipmentConfig::builtin()
        .item(BASKETS)
        .expect("the roster ships baskets")
        .default_tier()
        .id
        .clone();
    let mut wear = BandEquipment::default();
    wear.stock(BASKETS, baskets, &tier, None);
    let band = app
        .world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 30,
                children: scalar_zero(),
                working: scalar_from_f32(BAND_WORKERS as f32),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_food_need: 0.0,
                last_food_spoiled: 0.0,
                last_turn_food_transfers: Default::default(),
                last_turn_fodder_transfers: Default::default(),
                last_turn_transfer_crossings: Vec::new(),
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
                founding_lines: core_sim::FoundingLines::founded(
                    core_sim::BandId(0),
                    core_sim::MIN_BAND_LINES,
                ),
                belief_anchor: None,
            },
            StartingUnit {
                kind: "BandForager".to_string(),
                tags: Vec::new(),
            },
            ResidentBand,
            wear,
            allocation,
        ))
        .id();
    (app, band, rich, thin)
}

/// One turn of the labor pass, then the frame the player reads.
fn resolve_and_publish(app: &mut App) {
    app.world.run_system_once(advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);
}

/// **The player's `−`/`+` on the thin row**, through the command's own seam, then the recapture the
/// server makes after every command — no turn in between.
fn step_thin_crew(app: &mut App, band: Entity, thin: UVec2, crew: u32) {
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the band has an allocation")
        .set_assignment(forage_on(thin, THIN_FLOOR), crew, BAND_WORKERS, None);
    recapture_snapshot_in_place(&mut app.world);
}

/// One `kitToe` line as the client reads it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ToeLine {
    required: f32,
    filled: f32,
}

/// **The forage row working `tile`**, read off the encoded envelope: its baskets line (`None` where
/// it claims no basket) and its `workersNeeded` — the `+` cap.
fn published_row(app: &App, tile: UVec2) -> (Option<ToeLine>, u32) {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let row = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .flat_map(|cohort| cohort.laborAssignments().into_iter().flatten())
        .find(|row| {
            row.kind().unwrap_or_default() == "forage"
                && row.targetX() == tile.x
                && row.targetY() == tile.y
        })
        .expect("the forage row is on the wire");
    let baskets = row
        .kitToe()
        .into_iter()
        .flatten()
        .find(|line| line.itemId() == Some(BASKETS))
        .map(|line| ToeLine {
            required: line.required(),
            filled: line.filled(),
        });
    (baskets, row.workersNeeded())
}

/// The baskets line alone ([`published_row`]).
fn published_baskets(app: &App, tile: UVec2) -> Option<ToeLine> {
    published_row(app, tile).0
}

/// # ⛔ A HAND THE THIN STAND CANNOT USE PULLS NO BASKET OFF THE RICH ROW — AND IS NOT INVITED
///
/// Four baskets, five heads at crew 3 and six at crew 4 — so a head-count split is short at both
/// sizes and moves a basket when the thin row is raised (`4 × 2 ÷ 6` floors the rich row to one).
/// On claims the thin stand asks for the sliver of a basket its room needs at any crew, the rich row
/// for its two, and the split is the same at 3, at 4 and back at 3 — before a turn and after one.
///
/// **The `+` cap does not move either**: the thin row's `workersNeeded` is the same after a turn at
/// three, at four and at three again, and it is below three — so the cap never invited the fourth
/// hand, which adds nothing.
#[test]
fn raising_a_row_past_its_sites_yield_moves_no_kit_off_the_row_beside_it() {
    let (mut app, band, rich, thin) = the_scene(SCENE_BASKETS);
    // **A brand-new row is settled on its real claim before any turn has run** — the claim is a
    // function of the world, struck by the capture exactly as the turn strikes it.
    recapture_snapshot_in_place(&mut app.world);
    let rich_line = published_baskets(&app, rich).expect("the rich row claims baskets");
    let thin_line = published_baskets(&app, thin);
    assert_eq!(
        rich_line,
        ToeLine {
            required: RICH_CREW as f32,
            filled: RICH_CREW as f32,
        },
        "every hand on the rich stand takes something with a basket, and gets one"
    );
    // **The fixture's premise**: the thin stand claims fewer baskets than it has hands, and the two
    // claims fit the stock while the two head counts do not.
    let thin_claim = thin_line.map_or(0.0, |line| line.required);
    assert!(
        thin_claim < THIN_CREW as f32 && RICH_CREW as f32 + thin_claim <= SCENE_BASKETS as f32,
        "fixture: the thin stand must claim fewer baskets than it has hands ({thin_claim})"
    );

    // A turn at three: the split it settles is the one the capture quoted.
    resolve_and_publish(&mut app);
    assert_eq!(published_baskets(&app, rich), Some(rich_line));
    let (thin_after_three, cap_at_three) = published_row(&app, thin);
    assert!(
        cap_at_three < THIN_CREW,
        "fixture: the thin stand needs fewer hands than it has ({cap_at_three})"
    );

    // The `+`: the thin row goes to four, and the frame is recaptured with no turn between.
    step_thin_crew(&mut app, band, thin, RAISED_THIN_CREW);
    assert_eq!(
        published_baskets(&app, rich),
        Some(rich_line),
        "the raise must not pull a basket off the rich row"
    );
    assert_eq!(
        published_baskets(&app, thin),
        thin_after_three,
        "the fourth hand on the thin stand claims nothing"
    );

    // A turn at four: the rich row still holds its two, and the cap reads what it read at three.
    resolve_and_publish(&mut app);
    assert_eq!(
        published_baskets(&app, rich),
        Some(rich_line),
        "after a turn at four the rich row still holds its baskets"
    );
    let (thin_at_four, cap_at_four) = published_row(&app, thin);
    assert_eq!(
        cap_at_four, cap_at_three,
        "the `+` cap does not move with the player's own crew"
    );

    // The `−`: back to three, and the split is the one the row had at four.
    step_thin_crew(&mut app, band, thin, THIN_CREW);
    assert_eq!(published_baskets(&app, rich), Some(rich_line));
    assert_eq!(
        published_baskets(&app, thin),
        thin_at_four,
        "stepping the thin row back to three moves nothing"
    );
    resolve_and_publish(&mut app);
    assert_eq!(published_baskets(&app, rich), Some(rich_line));
    assert_eq!(
        published_row(&app, thin).1,
        cap_at_three,
        "the cap reads the same after the turn back at three"
    );
}

/// # ⛔ A GENUINE SHORTAGE STILL NAMES ITS ITEM
///
/// One basket for a rich row that claims two: the line reads `filled < required`, which is the
/// shortage the row's mark names by item.
#[test]
fn a_row_short_of_its_claim_publishes_the_item_it_is_short_of() {
    let (mut app, _band, rich, _thin) = the_scene(SHORT_BASKETS);
    resolve_and_publish(&mut app);
    let line = published_baskets(&app, rich).expect("the rich row claims baskets");
    assert_eq!(
        line.required, RICH_CREW as f32,
        "the rich row claims a basket per hand"
    );
    assert!(
        line.filled < line.required,
        "one basket for a claim of two is short: {line:?}"
    );
}
