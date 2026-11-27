//! **The scenario exporter** (`docs/plan_scenarios.md` section 2.10, issue #928): the live world
//! written out as a scenario file, and the round trip export -> load -> export.
//!
//! Driven through the library path the server's `export_scenario` runs
//! ([`export_scenario`]) and the loader path `tests/scenario_loader.rs` runs
//! ([`parse_scenario`] + [`build_scenario_world`]).

use bevy::prelude::*;
use serde_json::Value;

use core_sim::scenario::{
    build_scenario_world, export_scenario, parse_scenario, ScenarioContext, ScenarioExport,
};
use core_sim::{build_test_app, run_turn};

const BREEDING: &str = include_str!("../scenarios/breeding_ceiling_150.json");
const BENCH: &str = include_str!("../scenarios/bench_queue.json");
const DEPOSITS: &str = include_str!("../scenarios/deposits_playtest.json");
const HELD_RUNGS: &str = include_str!("../scenarios/held_rungs_playtest.json");
const HERDS_PENS: &str = include_str!("../scenarios/herds_pens_playtest.json");
const ROADS: &str = include_str!("../scenarios/kept_roads.json");

/// Turns run on the loaded world before it is exported, so the state has moved.
const TURNS_BEFORE_EXPORT: usize = 3;
/// The name the exports are given.
const EXPORT_NAME: &str = "round_trip";

/// Load `text` into a fresh world, run `turns`, and return the world.
fn loaded(text: &str, turns: usize) -> App {
    let mut app = build_test_app();
    let plan = parse_scenario(text, &ScenarioContext::from_world(&app.world))
        .unwrap_or_else(|refusals| panic!("the file is refused: {refusals:?}"));
    build_scenario_world(&mut app, &plan)
        .unwrap_or_else(|refusals| panic!("the file does not load: {refusals:?}"));
    for _ in 0..turns {
        run_turn(&mut app);
    }
    app
}

fn export_of(app: &mut App) -> (ScenarioExport, Value, String) {
    let export = export_scenario(&mut app.world, EXPORT_NAME);
    let text = serde_json::to_string_pretty(&export.file).expect("the export serializes");
    let value: Value = serde_json::from_str(&text).expect("the export is JSON");
    (export, value, text)
}

/// The keys of `value` whose values differ from `other`'s, for a failure that names the field.
fn drifting_keys(a: &Value, b: &Value) -> Vec<String> {
    let (Some(a), Some(b)) = (a.as_object(), b.as_object()) else {
        return vec!["<not objects>".to_string()];
    };
    a.keys()
        .chain(b.keys())
        .filter(|key| a.get(*key) != b.get(*key))
        .cloned()
        .collect()
}

/// Export A, rebuild from it, export B, and assert they are the same file. Returns A.
///
/// **`note` is not compared**: it describes the live world the file was exported from (a herd that
/// wandered, a party out), so a reload, which has not lived, has less to say. Every other key is.
fn round_trips(text: &str, turns: usize) -> Value {
    let without_note = |file: &Value| {
        let mut file = file.clone();
        if let Some(object) = file.as_object_mut() {
            object.remove("note");
        }
        file
    };
    let mut live = loaded(text, turns);
    let (_, a, a_text) = export_of(&mut live);
    let mut reloaded = loaded(&a_text, 0);
    let (_, b, _) = export_of(&mut reloaded);
    let (left, right) = (without_note(&a), without_note(&b));
    assert_eq!(
        left,
        right,
        "export -> load -> export drifted in {:?}",
        drifting_keys(&left, &right)
    );
    a
}

fn len_of(file: &Value, pointer: &str) -> usize {
    file.pointer(pointer)
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

#[test]
fn the_exported_file_reparses_through_pass_one_for_every_checked_in_scenario() {
    for (name, text) in [
        ("breeding", BREEDING),
        ("bench", BENCH),
        ("deposits", DEPOSITS),
        ("held_rungs", HELD_RUNGS),
        ("herds_pens", HERDS_PENS),
        ("roads", ROADS),
    ] {
        let mut live = loaded(text, TURNS_BEFORE_EXPORT);
        let (_, _, exported) = export_of(&mut live);
        let ctx = ScenarioContext::from_world(&live.world);
        let refused = parse_scenario(&exported, &ctx).err();
        assert!(
            refused.is_none(),
            "{name}: its own export is refused by pass 1: {refused:?}"
        );
    }
}

#[test]
fn a_world_with_a_band_exports_its_band_larder_and_outfit_and_round_trips() {
    let a = round_trips(BREEDING, TURNS_BEFORE_EXPORT);
    assert_eq!(a["name"], EXPORT_NAME);
    assert!(
        len_of(&a, "/factions/0/bands") >= 1,
        "liveness: the export holds the band"
    );
    let band = &a["factions"][0]["bands"][0];
    assert_eq!(band["id"], "f0_band0", "ids are positional");
    assert!(band["size"].as_u64().unwrap_or(0) > 0);
    assert!(
        !band["larder"].as_array().expect("a larder").is_empty(),
        "liveness: the larder is written"
    );
    assert!(
        band.get("outfit").is_some(),
        "the first band's outfit is written"
    );
    assert!(
        a["note"]
            .as_str()
            .unwrap_or("")
            .contains("tile overrides are not exported"),
        "the tiles section says what it cannot"
    );
    assert_eq!(a["tiles"], serde_json::json!([]));
}

#[test]
fn herds_pens_and_labor_round_trip() {
    let a = round_trips(HERDS_PENS, TURNS_BEFORE_EXPORT);
    assert!(len_of(&a, "/herds") > 0, "liveness: herds are written");
    assert!(
        len_of(&a, "/factions/0/labor") > 0,
        "liveness: standing labor rows are written"
    );
}

#[test]
fn held_rungs_round_trip() {
    let a = round_trips(HELD_RUNGS, TURNS_BEFORE_EXPORT);
    assert!(
        len_of(&a, "/factions/0/rungs") > 0,
        "liveness: held rungs or builds are written"
    );
}

#[test]
fn deposits_round_trip() {
    let a = round_trips(DEPOSITS, TURNS_BEFORE_EXPORT);
    assert!(
        len_of(&a, "/deposits") > 0,
        "liveness: workings are written"
    );
}

#[test]
fn roads_round_trip() {
    let a = round_trips(ROADS, TURNS_BEFORE_EXPORT);
    assert!(
        len_of(&a, "/factions/0/roads") > 0,
        "liveness: held roads are written"
    );
}

#[test]
fn every_checked_in_scenario_round_trips_whole() {
    for text in [BREEDING, BENCH, DEPOSITS, HELD_RUNGS, HERDS_PENS, ROADS] {
        round_trips(text, TURNS_BEFORE_EXPORT);
    }
}

#[test]
fn the_bench_queue_round_trips() {
    let a = round_trips(BENCH, TURNS_BEFORE_EXPORT);
    assert!(
        len_of(&a, "/factions/0/bench") > 0,
        "liveness: the bench queue is written"
    );
}

/// The breeding scenario with a second band beside the first, each holding gear.
fn two_outfitted_bands(second_size: u32, second_gathering_kits: u32) -> String {
    two_outfitted_bands_with(second_size, second_gathering_kits, false)
}

/// [`two_outfitted_bands`], the second band optionally holding a leader.
fn two_outfitted_bands_with(
    second_size: u32,
    second_gathering_kits: u32,
    second_leader: bool,
) -> String {
    let mut file: Value = serde_json::from_str(BREEDING).expect("the fixture is JSON");
    let first = file["factions"][0]["bands"][0].clone();
    let at = first["at"].as_array().expect("at").clone();
    let mut live = loaded(BREEDING, 0);
    let width = live.world.resource::<core_sim::TileRegistry>().width;
    let mut tiles = live.world.query::<&core_sim::Tile>();
    let start = UVec2::new(
        at[0].as_u64().unwrap_or(0) as u32,
        at[1].as_u64().unwrap_or(0) as u32,
    );
    let beside = tiles
        .iter(&live.world)
        .filter(|tile| {
            !tile.terrain_tags.contains(sim_runtime::TerrainTags::WATER)
                && tile.position != start
                && core_sim::grid_utils::hex_distance_wrapped(start, tile.position, width, false)
                    <= 2
        })
        .map(|tile| tile.position)
        .min_by_key(|pos| (pos.y, pos.x))
        .expect("land beside the first band");
    file["factions"][0]["bands"]
        .as_array_mut()
        .expect("bands")
        .push(serde_json::json!({
            "id": "second",
            "at": [beside.x, beside.y],
            "size": second_size,
            "ages": { "children": 0.30, "working": 0.60, "elders": 0.10 },
            "founding_lines": 4,
            "larder": [],
            "outfit": {
                "kits": { "gathering": second_gathering_kits },
                "materials": { "fibre": 2 }
            }
        }));
    if second_leader {
        file["factions"][0]["bands"][1]["leader"] = serde_json::json!(true);
    }
    file.to_string()
}

#[test]
fn a_second_bands_outfit_lands_on_that_band_and_round_trips() {
    let text = two_outfitted_bands(30, 3);
    let a = round_trips(&text, TURNS_BEFORE_EXPORT);
    let bands = a["factions"][0]["bands"].as_array().expect("bands");
    assert_eq!(bands.len(), 2, "liveness: both bands are exported");
    assert_eq!(
        bands[1]["outfit"]["kits"]["gathering"], 3,
        "the second band's own gear survives"
    );
    assert!(
        bands[0]["outfit"]["kits"]
            .as_object()
            .is_some_and(|k| !k.is_empty()),
        "the first band's gear survives"
    );
    let note = a["note"].as_str().unwrap_or("");
    assert!(
        !note.contains("only a faction's first band"),
        "no band is left out for not being first: {note}"
    );
}

#[test]
fn a_second_bands_outfit_over_its_carry_is_refused() {
    // Two people carry nothing like fifty kits.
    let text = two_outfitted_bands(2, 50);
    let mut app = build_test_app();
    let plan = parse_scenario(&text, &ScenarioContext::from_world(&app.world))
        .expect("the file passes pass 1");
    let refusals = build_scenario_world(&mut app, &plan).expect_err("over carry is refused");
    assert!(
        refusals
            .iter()
            .any(|r| r.path == "factions[0].bands[1].outfit" && r.reason.contains("carry")),
        "OverCarry at the band's outfit path: {refusals:?}"
    );
}

#[test]
fn a_band_the_file_gives_a_leader_holds_one_made_the_way_emergence_makes_it() {
    use core_sim::{
        BandId, BandLeadership, CommandEventKind, CommandEventLog, CultureManager, CultureOwner,
        CultureTraitAxis, LeaderKind, LeaderNameCatalogHandle, SimulationConfig,
    };
    // The control: the same file without the key leaves the band leaderless, so the setter is what
    // made the leader below.
    let mut control = loaded(&two_outfitted_bands_with(30, 3, false), 0);
    let led_in_control = control
        .world
        .query::<&BandLeadership>()
        .iter(&control.world)
        .filter(|leadership| leadership.leader.is_some())
        .count();
    let text = two_outfitted_bands_with(30, 3, true);
    let mut app = build_test_app();
    let plan = parse_scenario(&text, &ScenarioContext::from_world(&app.world)).expect("pass 1");
    let applied = build_scenario_world(&mut app, &plan).expect("loads");
    let led_with_key = app
        .world
        .query::<&BandLeadership>()
        .iter(&app.world)
        .filter(|leadership| leadership.leader.is_some())
        .count();
    assert_eq!(
        led_with_key,
        led_in_control + 1,
        "the key adds exactly one leader"
    );
    let band = applied.bands["second"];
    let leader = app
        .world
        .query::<(&BandId, &BandLeadership)>()
        .iter(&app.world)
        .find(|(id, _)| **id == band)
        .and_then(|(_, leadership)| leadership.leader.clone())
        .expect("the band holds a leader");
    let axis = app
        .world
        .resource::<CultureManager>()
        .band_layer_by_owner(CultureOwner::from_band(band))
        .expect("a culture layer")
        .traits
        .values()[CultureTraitAxis::HierarchicalEgalitarian.index()]
    .to_f32();
    assert_eq!(
        leader.kind,
        LeaderKind::for_axis_value(axis),
        "kind per the axis"
    );
    let seed = app.world.resource::<SimulationConfig>().map_seed;
    let name = app
        .world
        .resource::<LeaderNameCatalogHandle>()
        .get()
        .name_for_band(seed, band);
    assert_eq!(leader.name, name, "name per the catalog");
    let emerged = app
        .world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| entry.kind == CommandEventKind::LeaderEmerged)
        .any(|entry| {
            entry
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains(&format!("name={name}")))
        });
    assert!(emerged, "a LeaderEmerged feed line names the new leader");
}

#[test]
fn a_band_with_a_leader_exports_leader_true_and_the_reload_has_it() {
    let text = two_outfitted_bands_with(30, 3, true);
    let a = round_trips(&text, TURNS_BEFORE_EXPORT);
    assert_eq!(
        a["factions"][0]["bands"][1]["leader"], true,
        "the led band exports leader: true"
    );
    let a_text = serde_json::to_string(&a).expect("serializes");
    let mut reloaded = loaded(&a_text, 0);
    let held = reloaded
        .world
        .query::<&core_sim::BandLeadership>()
        .iter(&reloaded.world)
        .filter(|leadership| leadership.leader.is_some())
        .count();
    assert!(held >= 1, "the reload holds the leader");
}

/// A led band whose chief would list the herds another band of its people hunts: `home` holds a
/// leader and an empty labor list, `other` (a neighbour with no leader) hunts every herd.
fn led_band_beside_a_hunter() -> String {
    let mut file: Value = serde_json::from_str(HERDS_PENS).expect("the fixture is JSON");
    let faction = &mut file["factions"][0];
    faction["pens"] = serde_json::json!([]);
    faction["rungs"] = serde_json::json!([]);
    faction["bands"][0]["leader"] = serde_json::json!(true);
    faction["bands"]
        .as_array_mut()
        .expect("bands")
        .push(serde_json::json!({
            "id": "other", "at": [14, 11], "size": 60, "outfit": { "kits": { "gathering": 1 }, "materials": {} },
            "ages": { "children": 0.3, "working": 0.6, "elders": 0.1 },
            "founding_lines": 4, "larder": []
        }));
    let hunts: Vec<Value> = ["sheep", "new_aurochs", "thin_boar", "flock"]
        .iter()
        .map(|herd| {
            serde_json::json!({ "assign_labor": {
                "band": "other", "role": "hunt", "workers": 1, "fauna_id": herd,
                "take_species": [], "move_with_herd": false
            }})
        })
        .collect();
    faction["labor"] = Value::Array(hunts);
    file.to_string()
}

#[test]
fn a_chief_is_replayed_after_the_other_bands_rows_it_would_otherwise_claim() {
    let text = led_band_beside_a_hunter();
    let a = round_trips(&text, TURNS_BEFORE_EXPORT);
    let labor = a["factions"][0]["labor"].as_array().expect("labor");
    let band_of = |entry: &Value| {
        entry
            .as_object()
            .expect("entry")
            .values()
            .next()
            .expect("body")["band"]
            .clone()
    };
    let first_order = labor
        .iter()
        .position(|entry| entry.get("band_order").is_some())
        .expect("the led band exports its orders");
    assert!(first_order > 0, "the hunter's rows are written first");
    assert!(
        labor[..first_order]
            .iter()
            .all(|entry| band_of(entry) == "f0_band1"),
        "every unled band's rows come before the first chief order"
    );
    // The file's own rows are reserved while the chiefs are replayed, so the same entries written
    // chief-first load too: the ordering the export writes is no longer the only thing that holds.
    let mut reversed = a.clone();
    reversed["factions"][0]["labor"]
        .as_array_mut()
        .expect("labor")
        .reverse();
    let mut app = build_test_app();
    let plan = parse_scenario(
        &reversed.to_string(),
        &ScenarioContext::from_world(&app.world),
    )
    .expect("pass 1");
    build_scenario_world(&mut app, &plan)
        .unwrap_or_else(|refusals| panic!("chief-first no longer refuses: {refusals:?}"));
}

/// Every `null` in `value`, as a JSON pointer.
fn nulls_in(value: &Value, at: &str, found: &mut Vec<String>) {
    match value {
        Value::Null => found.push(at.to_string()),
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                nulls_in(item, &format!("{at}/{i}"), found);
            }
        }
        Value::Object(map) => {
            for (key, item) in map {
                nulls_in(item, &format!("{at}/{key}"), found);
            }
        }
        _ => {}
    }
}

#[test]
fn an_export_of_every_checked_in_scenario_holds_no_null() {
    for (name, text) in [
        ("breeding", BREEDING.to_string()),
        ("bench", BENCH.to_string()),
        ("deposits", DEPOSITS.to_string()),
        ("held_rungs", HELD_RUNGS.to_string()),
        ("herds_pens", HERDS_PENS.to_string()),
        ("roads", ROADS.to_string()),
        ("led_band_beside_a_hunter", led_band_beside_a_hunter()),
    ] {
        let mut live = loaded(&text, TURNS_BEFORE_EXPORT);
        let (_, value, _) = export_of(&mut live);
        let mut found = Vec::new();
        nulls_in(&value, "", &mut found);
        assert!(
            found.is_empty(),
            "{name}: the export writes null at {found:?}"
        );
    }
}

#[test]
fn a_worldgen_herd_that_died_out_stays_gone_across_export_and_reload() {
    use core_sim::{HerdRegistry, WorldgenHerdRoster};
    let mut live = loaded(BREEDING, 0);
    let standing: Vec<_> = {
        let registry = live.world.resource::<HerdRegistry>();
        live.world
            .resource::<WorldgenHerdRoster>()
            .herds
            .iter()
            .filter(|record| registry.find(&record.id).is_some())
            .cloned()
            .collect()
    };
    let record = standing
        .iter()
        .min_by(|a, b| a.id.cmp(&b.id))
        .cloned()
        .expect("worldgen made a herd that is still standing");
    let before_gone = export_of(&mut live).1["herds"]
        .as_array()
        .expect("herds")
        .iter()
        .filter(|h| h.get("gone").is_some())
        .count();
    live.world
        .resource_mut::<HerdRegistry>()
        .herds
        .retain(|herd| herd.id != record.id);
    let (_, a, a_text) = export_of(&mut live);
    let herds = a["herds"].as_array().expect("herds");
    let gone: Vec<&Value> = herds
        .iter()
        .filter(|h| {
            h.get("gone").is_some()
                && h["at"] == serde_json::json!([record.spawn.x, record.spawn.y])
        })
        .collect();
    assert_eq!(
        gone.len(),
        1,
        "the dead herd is written as gone at its spawn tile"
    );
    assert_eq!(gone[0]["gone"], true);
    assert_eq!(gone[0]["origin"], "worldgen");
    let all_gone = herds.iter().filter(|h| h.get("gone").is_some()).count();
    assert_eq!(
        all_gone,
        before_gone + 1,
        "only the killed herd is added to the build turn's own casualties"
    );
    let mut reloaded = loaded(&a_text, 0);
    assert!(
        reloaded
            .world
            .resource::<HerdRegistry>()
            .find(&record.id)
            .is_none(),
        "the reload does not spawn the dead herd again"
    );
    let (_, b, _) = export_of(&mut reloaded);
    let without_note = |file: &Value| {
        let mut file = file.clone();
        file.as_object_mut().expect("object").remove("note");
        file
    };
    assert_eq!(
        without_note(&a),
        without_note(&b),
        "export -> load -> export drifted"
    );
}

#[test]
fn a_gone_entry_on_a_placed_herd_or_with_a_stock_is_refused() {
    let live = loaded(BREEDING, 0);
    let mut file: Value = serde_json::from_str(BREEDING).expect("JSON");
    let herds = file["herds"].as_array_mut().expect("herds");
    herds.push(serde_json::json!({
        "id": "x", "species": "aurochs", "at": [0, 0], "stock": 1.0,
        "origin": "placed", "gone": true
    }));
    herds.push(serde_json::json!({
        "id": "y", "species": "aurochs", "at": [1, 0], "stock": 0.5,
        "origin": "worldgen", "gone": true
    }));
    let refused = parse_scenario(&file.to_string(), &ScenarioContext::from_world(&live.world))
        .expect_err("both entries are refused");
    let text = format!("{refused:?}");
    assert!(text.contains("only a worldgen herd can be gone"), "{text}");
    assert!(text.contains("a gone herd has no stock to set"), "{text}");
}

/// Two led bands of one people: `home` (band 0) holds a leader and no rows, `other` holds a leader and
/// a hunt row on every herd. The lower band's chief would list those herds if nothing reserved them.
fn two_led_bands_one_hunting() -> String {
    let mut file: Value = serde_json::from_str(&led_band_beside_a_hunter()).expect("JSON");
    file["factions"][0]["bands"][1]["leader"] = serde_json::json!(true);
    // Big enough to open the leader's orders, as the first band is.
    file["factions"][0]["bands"][1]["size"] = serde_json::json!(150);
    // The led band holds the food order, then hands the whole kind to the player, as the exporter
    // writes a band whose player crews the kind; its rows follow.
    let rows = file["factions"][0]["labor"].take();
    let mut labor = vec![
        // The lower band's chief is told to hold food on hand, so it lists every source nobody holds.
        serde_json::json!({ "band_order": {
            "band": "home", "kind": "food", "action": "set", "target": 8, "priority": "high"
        }}),
        serde_json::json!({ "band_order": {
            "band": "other", "kind": "food", "action": "set", "target": 1, "priority": "normal"
        }}),
        serde_json::json!({ "band_order": {
            "band": "other", "kind": "food", "action": "release"
        }}),
    ];
    labor.extend(rows.as_array().expect("labor").iter().cloned());
    file["factions"][0]["labor"] = Value::Array(labor);
    file.to_string()
}

#[test]
fn a_led_bands_chief_leaves_a_later_led_bands_row_alone() {
    use core_sim::{BandId, LaborAllocation, LaborTarget};
    let mut app = build_test_app();
    let plan = parse_scenario(
        &two_led_bands_one_hunting(),
        &ScenarioContext::from_world(&app.world),
    )
    .expect("pass 1");
    let applied = build_scenario_world(&mut app, &plan)
        .unwrap_or_else(|refusals| panic!("the file does not load: {refusals:?}"));
    let other = applied.bands["other"];
    let hunted = app
        .world
        .query::<(&BandId, &LaborAllocation)>()
        .iter(&app.world)
        .find(|(id, _)| **id == other)
        .map(|(_, allocation)| {
            allocation
                .assignments
                .iter()
                .filter(|row| matches!(row.target, LaborTarget::Hunt { .. }))
                .count()
        })
        .expect("the band exists");
    assert!(
        hunted >= 4,
        "the higher led band holds its own rows ({hunted})"
    );
    assert!(
        app.world
            .get_resource::<core_sim::source_holders::ScenarioLaborReservations>()
            .is_none(),
        "the reservation ends with the replay"
    );
}
