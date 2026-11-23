//! **The scenario loader, slice 2b: the `rungs` section** (`docs/plan_scenarios.md`, issue #923).
//!
//! Driven through the library path the server's `load_scenario` handler runs: pass 1
//! ([`parse_scenario`]), then [`build_scenario_world`]. Real sources are found by building the
//! scenario's world once with an empty `rungs` (the seed is fixed, so the ground is deterministic),
//! then the scenario that names them is loaded; each positive claim has its control.

use bevy::prelude::*;
use serde_json::{json, Value};

use core_sim::build_gates::validate_improvement;
use core_sim::command_seam::{apply_build_rung, BuildRungRequest, CommandRefusal, DeclaredKind};
use core_sim::scenario::{build_scenario_world, parse_scenario, ScenarioContext, ScenarioRefusal};
use core_sim::{
    build_test_app, default_species_for_rung, pen_site_in_world, publish_baseline_snapshot,
    tile_flora_composition, BandId, BuildSource, FactionId, ForageRegistry, HerdRegistry,
    LaborAllocation, LaborTarget, LadderConfigHandle, PopulationCohort, SimulationConfig,
    SnapshotHistory, TakeSelection, Tile, TileRegistry, VerbId,
};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

const SHIPPED: &str = include_str!("../scenarios/breeding_ceiling_150.json");
const HOME: FactionId = FactionId(0);
const CULTIVATION: &str = "cultivation";
const SEED_SELECTION: &str = "seed_selection";
const HERDING: &str = "herding";
const PENNING: &str = "penning";
const WOODCRAFT: &str = "woodcraft";
const WORKERS_ON_THE_ROW: u32 = 3;
const IDLE_HANDS_AVAILABLE: u32 = 30;

fn shipped() -> Value {
    serde_json::from_str(SHIPPED).expect("the shipped scenario is JSON")
}

/// The shipped scenario with `knowledge` and `rungs` replaced.
fn scenario(knowledge: &[&str], rungs: Value) -> String {
    let mut file = shipped();
    // `messengers` stays so the file differs from the shipped one only in what is under test.
    let mut tags = vec![json!("messengers")];
    tags.extend(knowledge.iter().map(|tag| json!(tag)));
    file["factions"][0]["knowledge"] = Value::Array(tags);
    file["factions"][0]["rungs"] = rungs;
    file.to_string()
}

fn load(text: &str) -> Result<App, Vec<ScenarioRefusal>> {
    let mut app = build_test_app();
    let plan = parse_scenario(text, &ScenarioContext::from_world(&app.world))?;
    build_scenario_world(&mut app, &plan)?;
    publish_baseline_snapshot(&mut app.world);
    Ok(app)
}

fn pass_one(text: &str) -> Result<(), Vec<ScenarioRefusal>> {
    let app = build_test_app();
    parse_scenario(text, &ScenarioContext::from_world(&app.world)).map(|_| ())
}

/// The world the empty scenario builds, with every lesson the tests climb on known.
fn base_world() -> App {
    load(&scenario(
        &[CULTIVATION, SEED_SELECTION, HERDING, PENNING, WOODCRAFT],
        json!([]),
    ))
    .expect("the base scenario loads")
}

/// The standing tile of the home faction's lowest-id band in `app`.
fn home_band_tile(app: &mut App) -> UVec2 {
    let mut query = app.world.query::<(&BandId, &PopulationCohort)>();
    let (_, cohort) = query
        .iter(&app.world)
        .filter(|(_, cohort)| cohort.faction == HOME)
        .min_by_key(|(id, _)| **id)
        .expect("the home band");
    let tile = cohort.current_tile;
    app.world.get::<Tile>(tile).expect("its tile").position
}

fn forage_target(tile: UVec2, species: Option<&str>) -> LaborTarget {
    LaborTarget::Forage {
        tile,
        floor: 0.0,
        species: species.map(str::to_string),
        take_species: TakeSelection::EVERYTHING,
    }
}

fn verb(name: &str) -> VerbId {
    VerbId::new(name)
}

/// Patches (sorted by tile) the cultivate gate passes on, in the base world.
fn tended_tiles(app: &App) -> Vec<UVec2> {
    let mut tiles: Vec<UVec2> = app
        .world
        .resource::<ForageRegistry>()
        .patches
        .keys()
        .copied()
        .collect();
    tiles.sort_by_key(|tile| (tile.y, tile.x));
    tiles
        .into_iter()
        .filter(|tile| {
            validate_improvement(
                &app.world,
                HOME,
                &forage_target(*tile, None),
                verb("cultivate"),
            )
            .is_ok()
        })
        .collect()
}

/// The default crop a Sow on `tile` would commit, for tiles the Farm can be raised on.
fn default_farm_crop(app: &App, tile: UVec2) -> Option<String> {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
    let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
    let seed = app.world.resource::<SimulationConfig>().map_seed;
    let entity = app.world.resource::<TileRegistry>().index(tile.x, tile.y)?;
    let ground = app.world.get::<Tile>(entity)?;
    let composition = tile_flora_composition(&flora, &labor.forage, ground, seed);
    let farm = ladder.find("plant", "farm")?.rung_id();
    default_species_for_rung(&composition, &flora, &ladder, farm)
}

/// A tile where both Cultivate and (after it) Sow are legal for the default crop.
fn farmable_tile(app: &App) -> (UVec2, String) {
    for tile in tended_tiles(app) {
        let Some(crop) = default_farm_crop(app, tile) else {
            continue;
        };
        // The Farm's own gate needs the tended rung below it, so judge it on a cloned world's twin:
        // the scenario itself is the test, so here only the site rule is asked.
        let ladder = app.world.resource::<LadderConfigHandle>().get();
        let farm = ladder.find("plant", "farm").expect("the farm rung");
        let ground_ok =
            core_sim::build_gates::plant_rung_site_refusal(&app.world, HOME, farm.rung_id(), tile)
                .is_none();
        if ground_ok {
            return (tile, crop);
        }
    }
    panic!("the base world has a tile the Farm can be raised on");
}

fn published_tended(app: &App, tile: UVec2) -> bool {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let rows = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .subsistence()
        .and_then(|section| section.foragePatches())
        .expect("forage patches");
    let row = rows
        .iter()
        .find(|row| row.x() == tile.x && row.y() == tile.y)
        .expect("the patch row is published");
    sim_schema::wire_rung_quote(row.rungQuotes(), "plant:tended")
        .is_some_and(|quote| quote.reached())
}

fn held(rung: &str, extra: Value) -> Value {
    let mut body = json!({ "rung": rung });
    for (key, value) in extra.as_object().expect("an object").iter() {
        body[key] = value.clone();
    }
    json!({ "held": body })
}

fn at(tile: UVec2) -> Value {
    json!([tile.x, tile.y])
}

fn refusal_at<'a>(refusals: &'a [ScenarioRefusal], path: &str) -> &'a ScenarioRefusal {
    refusals
        .iter()
        .find(|refusal| refusal.path == path)
        .unwrap_or_else(|| panic!("no refusal at {path}: {refusals:?}"))
}

#[test]
fn a_held_tended_rung_is_cultivated_owned_and_published() {
    let base = base_world();
    let tile = *tended_tiles(&base).first().expect("a tended-able patch");
    let rungs = json!([held("plant:tended", json!({ "at": at(tile) }))]);

    let app = load(&scenario(&[CULTIVATION], rungs.clone())).expect("the held rung loads");
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(tile)
        .expect("the patch")
        .clone();
    assert!(patch.is_cultivated(), "the patch holds the tended rung");
    assert!(!patch.is_field(), "and only the tended rung");
    assert_eq!(patch.owner, Some(HOME), "owned as an accrual owns it");
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let tended = ladder.find("plant", "tended").expect("the tended rung");
    let (span_base, width) = core_sim::plant_rung_span(tended.rung_id(), &ladder);
    assert_eq!(
        patch.ladder_position(),
        span_base + width,
        "the position stands exactly at the top of the tended rung, with no spill into the farm"
    );
    assert!(
        published_tended(&app, tile),
        "the encoded frame's row reads the tended rung reached"
    );

    // Control: without the lesson the same file is refused with the gate's own text.
    let refusals = load(&scenario(&[], rungs)).expect_err("refused");
    let wanted = validate_improvement(
        &base.world,
        HOME,
        &forage_target(tile, None),
        verb("cultivate"),
    );
    // The base world knows the lesson, so state the gate's text from a world that does not.
    assert!(wanted.is_ok(), "the base world has the lesson");
    let refusal = refusal_at(&refusals, "factions[0].rungs[0]");
    assert!(
        refusal.reason.starts_with("plant:tended: "),
        "the refusal names the rung: {}",
        refusal.reason
    );
    assert!(
        refusal.reason.to_lowercase().contains("cultivation"),
        "and the gate's own knowledge text: {}",
        refusal.reason
    );
}

#[test]
fn a_held_farm_climbs_through_tended_and_commits_the_crop() {
    let base = base_world();
    let (tile, default_crop) = farmable_tile(&base);
    let both = [CULTIVATION, SEED_SELECTION];

    // Default pick when `species` is absent.
    let rungs = json!([held("plant:farm", json!({ "at": at(tile) }))]);
    let app = load(&scenario(&both, rungs)).expect("the farm loads");
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(tile)
        .expect("the patch")
        .clone();
    assert!(patch.is_field(), "the farm is held");
    assert!(patch.is_cultivated(), "and the tended rung beneath it");
    assert_eq!(patch.owner, Some(HOME));
    assert_eq!(patch.species.as_deref(), Some(default_crop.as_str()));
    assert!(
        patch.quoted_field_cost_multiplier().is_some(),
        "the Field leg's price was stamped as a real Sow stamps it"
    );

    // Named species.
    let rungs = json!([held(
        "plant:farm",
        json!({ "at": at(tile), "species": default_crop })
    )]);
    let app = load(&scenario(&both, rungs)).expect("the named crop loads");
    let patch = app
        .world
        .resource::<ForageRegistry>()
        .patch(tile)
        .expect("the patch")
        .clone();
    assert_eq!(patch.species.as_deref(), Some(default_crop.as_str()));

    // Control: tended-only knowledge cannot raise the Farm, and the gate names the farm rung.
    let rungs = json!([held("plant:farm", json!({ "at": at(tile) }))]);
    let refusals = load(&scenario(&[CULTIVATION], rungs)).expect_err("seed selection is missing");
    let refusal = refusal_at(&refusals, "factions[0].rungs[0]");
    assert!(
        refusal.reason.starts_with("plant:farm: "),
        "the climb got past tended and the refusal names the farm rung: {}",
        refusal.reason
    );
}

#[test]
fn a_held_pen_corrals_and_owns_the_herd_and_a_short_ceiling_is_refused() {
    let base = base_world();
    let ladder = base.world.resource::<LadderConfigHandle>().get();
    let herds = base.world.resource::<HerdRegistry>();
    let penable = herds
        .herds
        .iter()
        .find(|herd| herd.can_pen(&ladder))
        .expect("a herd whose ceiling reaches the pen")
        .id
        .clone();
    let short = herds
        .herds
        .iter()
        .find(|herd| herd.can_domesticate(&ladder) && !herd.can_pen(&ladder))
        .expect("the seed has a herd that can be tamed but not penned")
        .id
        .clone();
    let knowing = [HERDING, PENNING];

    let rungs = json!([held("animal:pen", json!({ "herd": penable }))]);
    let app = load(&scenario(&knowing, rungs)).expect("the pen loads");
    let herd = app
        .world
        .resource::<HerdRegistry>()
        .find(&penable)
        .expect("the herd")
        .clone();
    assert!(herd.is_corralled(), "the herd is penned");
    assert!(herd.is_domesticated(), "through tamed");
    assert_eq!(herd.owner, Some(HOME));

    // The pen stands where a Corral queued by the home band would fence it, not under the herd.
    let mut base = base_world();
    let band_tile = home_band_tile(&mut base);
    let unpenned = base
        .world
        .resource::<HerdRegistry>()
        .find(&penable)
        .expect("the herd in the base world")
        .clone();
    let expected = pen_site_in_world(&base.world, &unpenned, band_tile).expect("a site in range");
    assert_eq!(herd.corralled_at, Some(expected), "the game's own pick");
    assert_ne!(
        expected,
        unpenned.position(),
        "control: the pick is not the herd's own tile, so the old fallback would have differed"
    );

    // Control: a species that can be tamed but not penned passes the tame rung and is refused at
    // the pen with the gate's own ceiling text.
    let tame_only = scenario(
        &knowing,
        json!([held("animal:pastoral", json!({ "herd": short }))]),
    );
    let tamed = load(&tame_only).expect("the tame rung alone loads");
    let ceiling_text = validate_improvement(
        &tamed.world,
        HOME,
        &LaborTarget::Hunt {
            fauna_id: short.clone(),
            floor: 0.0,
            move_with_herd: false,
        },
        verb("corral"),
    )
    .expect_err("the tamed herd cannot be penned");
    let rungs = json!([held("animal:pen", json!({ "herd": short }))]);
    let refusals = load(&scenario(&knowing, rungs)).expect_err("refused");
    let refusal = refusal_at(&refusals, "factions[0].rungs[0]");
    assert_eq!(refusal.reason, format!("animal:pen: {ceiling_text}"));
}

#[test]
fn two_held_pens_are_never_fenced_on_one_tile() {
    let mut base = base_world();
    let band_tile = home_band_tile(&mut base);
    let ladder = base.world.resource::<LadderConfigHandle>().get();
    let mut penable: Vec<_> = base
        .world
        .resource::<HerdRegistry>()
        .herds
        .iter()
        .filter(|herd| herd.can_pen(&ladder))
        .cloned()
        .collect();
    penable.sort_by_key(|herd| {
        (
            pen_site_in_world(&base.world, herd, band_tile).is_none(),
            herd.position().x.abs_diff(band_tile.x) + herd.position().y.abs_diff(band_tile.y),
            herd.id.clone(),
        )
    });
    let (first, second) = (&penable[0], &penable[1]);
    // Liveness: with nothing else penned both herds would take the same tile.
    assert_eq!(
        pen_site_in_world(&base.world, first, band_tile),
        pen_site_in_world(&base.world, second, band_tile),
        "the control: the unqueued picks collide"
    );

    let rungs = json!([
        held("animal:pen", json!({ "herd": first.id })),
        held("animal:pen", json!({ "herd": second.id })),
    ]);
    let app = load(&scenario(&[HERDING, PENNING], rungs)).expect("both pens load");
    let herds = app.world.resource::<HerdRegistry>();
    let a = herds.find(&first.id).expect("first").corralled_at;
    let b = herds.find(&second.id).expect("second").corralled_at;
    assert!(a.is_some() && b.is_some(), "both are penned");
    assert_ne!(a, b, "two pens never share a tile");
}

#[test]
fn a_held_working_rung_holds_when_its_lesson_is_known() {
    let base = base_world();
    let ladder = base.world.resource::<LadderConfigHandle>().get();
    let felling = ladder.find("forestry", "felling").expect("felling");
    let (width, height) = {
        let registry = base.world.resource::<TileRegistry>();
        (registry.width, registry.height)
    };
    let tile = (0..height)
        .flat_map(|y| (0..width).map(move |x| UVec2::new(x, y)))
        .find(|tile| {
            validate_improvement(
                &base.world,
                HOME,
                &LaborTarget::Extract {
                    tile: *tile,
                    material: "wood".to_string(),
                    floor: 0.0,
                },
                felling.verb_id().expect("a verb"),
            )
            .is_ok()
        })
        .expect("a tile the wood can be felled on");
    let rungs = json!([held(
        "forestry:felling",
        json!({ "at": at(tile), "material": "wood" })
    )]);

    let app = load(&scenario(&[WOODCRAFT], rungs.clone())).expect("the working loads");
    let working = app
        .world
        .resource::<core_sim::DepositRegistry>()
        .source(tile, "wood")
        .expect("the working was opened")
        .clone();
    assert_eq!(working.rung(), felling.rung_id());

    // Control: the lesson unknown refuses.
    let refusals = load(&scenario(&[], rungs)).expect_err("refused");
    let refusal = refusal_at(&refusals, "factions[0].rungs[0]");
    assert!(refusal.reason.starts_with("forestry:felling: "));
}

#[test]
fn pass_one_collects_every_refusal_in_one_load() {
    let rungs = json!([
        held("plant:nothing", json!({ "at": [1, 1] })),
        held("plant:wild", json!({ "at": [1, 1] })),
        held("route:dirt_road", json!({ "at": [1, 1] })),
        held("plant:tended", json!({})),
        held("plant:tended", json!({ "at": [1, 1], "herd": "h1" })),
        held("animal:pen", json!({ "herd": "h1", "at": [2, 2] })),
        held(
            "forestry:felling",
            json!({ "at": [3, 3], "material": "unobtainium" })
        ),
        held("forestry:felling", json!({ "at": [3, 3], "material": "stone" })),
        held("plant:farm", json!({ "at": [4, 4], "species": "no_such_crop" })),
        held("animal:pen", json!({ "herd": "dup" })),
        held("animal:pastoral", json!({ "herd": "dup" })),
        { "build_rung": { "band": "nobody", "verb": "no_such_verb",
                          "target_x": null, "target_y": null, "herd_id": null,
                          "material": null, "branch": null } },
    ]);
    let refusals = pass_one(&scenario(&[], rungs)).expect_err("refused");
    let path = |j: usize, field: &str| format!("factions[0].rungs[{j}].held.{field}");
    assert!(refusal_at(&refusals, &path(0, "rung"))
        .reason
        .contains("not a rung on the ladder"));
    assert!(refusal_at(&refusals, &path(1, "rung"))
        .reason
        .contains("root"));
    assert!(refusal_at(&refusals, &path(2, "rung"))
        .reason
        .contains("not a `held` source"));
    assert!(refusal_at(&refusals, &path(3, "at"))
        .reason
        .contains("needs 'at'"));
    assert!(refusal_at(&refusals, &path(4, "herd"))
        .reason
        .contains("takes no 'herd'"));
    assert!(refusal_at(&refusals, &path(5, "at"))
        .reason
        .contains("takes no 'at'"));
    assert!(refusal_at(&refusals, &path(6, "material"))
        .reason
        .contains("not a deposit"));
    assert!(refusal_at(&refusals, &path(7, "material"))
        .reason
        .contains("branch"));
    assert!(refusal_at(&refusals, &path(8, "species"))
        .reason
        .contains("not a species"));
    assert!(refusal_at(&refusals, "factions[0].rungs[10]")
        .reason
        .contains("listed twice"));
    assert!(
        refusal_at(&refusals, "factions[0].rungs[11].build_rung.band")
            .reason
            .contains("not a band id declared")
    );
    assert!(
        refusal_at(&refusals, "factions[0].rungs[11].build_rung.verb")
            .reason
            .contains("not a build verb")
    );
}

#[test]
fn a_refused_entry_beside_a_good_one_builds_no_world() {
    let base = base_world();
    let tile = *tended_tiles(&base).first().expect("a tended-able patch");
    let rungs = json!([
        held("plant:tended", json!({ "at": at(tile) })),
        held("plant:tended", json!({ "at": [0, 0] })),
    ]);
    let refusals =
        load(&scenario(&[CULTIVATION], rungs)).expect_err("one refused entry fails the whole load");
    assert!(
        refusals
            .iter()
            .any(|refusal| refusal.path == "factions[0].rungs[1]"),
        "{refusals:?}"
    );
    // The good entry alone loads, so the refusal above is the bad entry's and not the file's.
    let rungs = json!([held("plant:tended", json!({ "at": at(tile) }))]);
    load(&scenario(&[CULTIVATION], rungs)).expect("the good entry loads on its own");
}

#[test]
fn a_build_rung_on_a_patch_no_band_works_is_refused_with_the_handlers_own_text() {
    let base = base_world();
    let tile = *tended_tiles(&base).first().expect("a tended-able patch");
    let rungs = json!([{ "build_rung": {
        "band": null, "verb": "cultivate",
        "target_x": tile.x, "target_y": tile.y,
        "herd_id": null, "material": null, "branch": null } }]);
    let refusals = load(&scenario(&[CULTIVATION], rungs)).expect_err("no band works the patch");
    let refusal = refusal_at(&refusals, "factions[0].rungs[0]");
    assert!(
        refusal
            .reason
            .contains(&format!("No band of yours works ({}, {})", tile.x, tile.y)),
        "{}",
        refusal.reason
    );
}

#[test]
fn the_seam_declares_a_build_on_a_worked_patch_and_refuses_one_already_built() {
    let mut app = base_world();
    let tile = *tended_tiles(&app).first().expect("a tended-able patch");
    let (band, entity) = {
        let mut query = app.world.query::<(Entity, &BandId, &PopulationCohort)>();
        let (entity, band, _) = query
            .iter(&app.world)
            .find(|(_, _, cohort)| cohort.faction == HOME)
            .expect("the home band");
        (*band, entity)
    };
    let _ = band;
    app.world
        .get_mut::<LaborAllocation>(entity)
        .map(|mut allocation| {
            allocation.set_assignment(
                forage_target(tile, None),
                WORKERS_ON_THE_ROW,
                IDLE_HANDS_AVAILABLE,
                None,
            )
        })
        .or_else(|| {
            let mut allocation = LaborAllocation::default();
            allocation.set_assignment(
                forage_target(tile, None),
                WORKERS_ON_THE_ROW,
                IDLE_HANDS_AVAILABLE,
                None,
            );
            app.world.entity_mut(entity).insert(allocation);
            Some(0)
        });
    let request = BuildRungRequest {
        verb: "cultivate".to_string(),
        target_x: Some(tile.x),
        target_y: Some(tile.y),
        ..BuildRungRequest::default()
    };
    let declared = apply_build_rung(&mut app.world, HOME, &request).expect("the build is declared");
    assert!(matches!(
        declared.kind,
        DeclaredKind::Source { bands: 1, .. }
    ));
    assert!(
        app.world
            .get::<LaborAllocation>(entity)
            .expect("the allocation")
            .has_build(&BuildSource::Patch(tile)),
        "the band holds the declared build"
    );

    // Control: a patch already holding the rung is refused with the gate's own text.
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    app.world
        .resource_mut::<ForageRegistry>()
        .patch_mut(tile)
        .expect("the patch")
        .complete_cultivation(HOME, &ladder);
    let gate = validate_improvement(
        &app.world,
        HOME,
        &forage_target(tile, None),
        verb("cultivate"),
    )
    .expect_err("the gate refuses a rung already held");
    let Err(CommandRefusal::Build { reason, .. }) =
        apply_build_rung(&mut app.world, HOME, &request)
    else {
        panic!("a rung already held is refused");
    };
    assert_eq!(reason, gate);
    assert!(gate.to_lowercase().contains("already"), "{gate}");
}

#[test]
fn the_held_rungs_playtest_scenario_loads_and_fences_its_pen_off_the_herds_tile() {
    let text = include_str!("../scenarios/held_rungs_playtest.json");
    let app = load(text).expect("the playtest scenario loads");
    let pen = app
        .world
        .resource::<HerdRegistry>()
        .find("game_wild_sheep_00")
        .expect("the herd")
        .corralled_at
        .expect("the sheep are penned");

    // The same world with the pen row dropped: where the herd stood, and what the game would pick.
    let mut file: Value = serde_json::from_str(text).expect("JSON");
    let rungs = file["factions"][0]["rungs"].as_array_mut().expect("rungs");
    rungs.retain(|entry| entry["held"]["rung"] != "animal:pen");
    let mut base = load(&file.to_string()).expect("the world without the pen loads");
    let band_tile = home_band_tile(&mut base);
    let unpenned = base
        .world
        .resource::<HerdRegistry>()
        .find("game_wild_sheep_00")
        .expect("the herd")
        .clone();
    println!("pen at {pen:?}, herd stood at {:?}", unpenned.position());
    assert_eq!(
        Some(pen),
        pen_site_in_world(&base.world, &unpenned, band_tile)
    );
    assert_ne!(
        pen,
        unpenned.position(),
        "the pen is off the herd's own tile"
    );
}
