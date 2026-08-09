//! **The bench** (`docs/plan_crafting_and_materials.md` §3/§5) — what a turn at it costs, what it
//! delivers, and the four things that must not drift.
//!
//! The world these tests want is small: a band with a store, a bench and an equipment ledger, and
//! the four config tables `advance_crafting` reads. So they stand one up by hand rather than through
//! worldgen — everything here is about the bench, and a generated map would only add ways for the
//! test to be about something else.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::Entity;
use bevy::MinimalPlugins;

use core_sim::{
    advance_crafting, scalar_from_f32, scalar_one, scalar_zero, BandBench, BandEquipment, BandId,
    DiscoveryProgressLedger, EquipmentConfig, EquipmentConfigHandle, FactionId, GenerationId,
    LadderConfigHandle, LocalStore, MaterialsConfig, MaterialsConfigHandle, MoraleCause,
    PopulationCohort, RecipesConfig, RecipesConfigHandle, Scalar,
};
use std::collections::BTreeMap;

const FACTION: FactionId = FactionId(0);
const BAND_POP: u32 = 30;
/// A crew big enough that the bench finishes in a small number of turns at the bare-handed rate, so
/// a test can measure a completion without simulating a season.
const CREW: u32 = 8;

const HIDE: &str = "hide";
const TOUGHNESS: &str = "toughness";
const SUPPLENESS: &str = "suppleness";
/// The shipped grade names — spelled here because a test asserting *which* grade a draw selected has
/// to name one, and nowhere else in the sim does.
const COARSE: &str = "coarse";
const STANDARD: &str = "standard";
const FINE: &str = "fine";
const TANNING_FRAME: &str = "tanning_frame";
const SLED: &str = "sled";
/// The tier every shipped item's one quality rung carries — the flint age.
const FLINT_TIER: &str = "flint";

fn readings(pairs: &[(&str, f32)]) -> BTreeMap<String, f32> {
    pairs
        .iter()
        .map(|(axis, value)| ((*axis).to_string(), *value))
        .collect()
}

fn cohort(working: f32, stores: LocalStore) -> PopulationCohort {
    PopulationCohort {
        home: Entity::PLACEHOLDER,
        current_tile: Entity::PLACEHOLDER,
        size: BAND_POP,
        children: scalar_zero(),
        working: scalar_from_f32(working),
        elders: scalar_zero(),
        stores,
        morale: scalar_one(),
        last_food_consumption: 0.0,
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
        faction: FACTION,
        knowledge: Vec::new(),
        migration: None,
    }
}

/// A world holding exactly what `advance_crafting` reads, plus one band.
struct Bench {
    app: App,
    band: Entity,
}

impl Bench {
    fn new(materials: MaterialsConfig, recipes: RecipesConfig, equipment: EquipmentConfig) -> Self {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.world
            .insert_resource(MaterialsConfigHandle::new(std::sync::Arc::new(materials)));
        app.world
            .insert_resource(RecipesConfigHandle::new(std::sync::Arc::new(recipes)));
        let equipment = std::sync::Arc::new(equipment);
        let equipment_for_stock = std::sync::Arc::clone(&equipment);
        app.world
            .insert_resource(EquipmentConfigHandle::new(equipment));
        app.world.insert_resource(LadderConfigHandle::default());
        app.world
            .insert_resource(DiscoveryProgressLedger::default());
        let band = app
            .world
            .spawn((
                cohort(BAND_POP as f32, LocalStore::new()),
                BandBench::default(),
                // A spawned band is start-stocked; `Default` means *owns nothing* since the count
                // slice, and a bench fixture that started bare would have no kit to replenish.
                BandEquipment::start_stocked(&equipment_for_stock),
                BandId(1),
            ))
            .id();
        Self { app, band }
    }

    /// The shipped tables — the honest fixture for anything about the game as authored.
    fn shipped() -> Self {
        Self::new(
            MaterialsConfig::from_json_str(core_sim::BUILTIN_MATERIALS_CONFIG).expect("materials"),
            RecipesConfig::from_json_str(core_sim::BUILTIN_RECIPES_CONFIG).expect("recipes"),
            EquipmentConfig::from_json_str(core_sim::BUILTIN_EQUIPMENT_CONFIG).expect("equipment"),
        )
    }

    fn stock(&mut self, material: &str, amount: f32, axes: &[(&str, f32)]) -> &mut Self {
        let materials = self.app.world.resource::<MaterialsConfigHandle>().get();
        let readings = readings(axes);
        let key = materials
            .band_key(material, &readings)
            .expect("the fixture's material is in the table");
        let mut cohort = self
            .app
            .world
            .get_mut::<PopulationCohort>(self.band)
            .expect("the band has a cohort");
        cohort
            .stores
            .deposit_material(material, key, scalar_from_f32(amount), &readings);
        self
    }

    fn give_tool(&mut self, item: &str) -> &mut Self {
        self.app
            .world
            .get_mut::<BandEquipment>(self.band)
            .expect("the band has a ledger")
            // A crafted tool arrives as its own batch of one, at the tier that ships known.
            .stock(item, 1, FLINT_TIER, None);
        self
    }

    fn start(&mut self, recipe: &str, workers: u32) -> &mut Self {
        self.app
            .world
            .get_mut::<BandBench>(self.band)
            .expect("the band has a bench")
            .set_job(recipe, workers);
        self
    }

    fn turns(&mut self, count: u32) -> &mut Self {
        for _ in 0..count {
            self.app.world.run_system_once(advance_crafting);
        }
        self
    }

    fn bench(&self) -> &BandBench {
        self.app
            .world
            .get::<BandBench>(self.band)
            .expect("the band has a bench")
    }

    fn wear_of(&self, item: &str) -> f32 {
        self.app
            .world
            .get::<BandEquipment>(self.band)
            .expect("the band has a ledger")
            .wear_of(item)
    }

    /// Whole units of an item the band holds, across every batch.
    fn count_of(&self, item: &str) -> u32 {
        self.app
            .world
            .get::<BandEquipment>(self.band)
            .expect("the band has a ledger")
            .count_of(item)
    }

    /// Condition left on the unit in hand.
    fn remaining(&self, item: &str) -> f32 {
        let equipment = self.app.world.resource::<EquipmentConfigHandle>().get();
        self.app
            .world
            .get::<BandEquipment>(self.band)
            .expect("the band has a ledger")
            .remaining(item, &equipment)
    }

    fn stock_of(&self, material: &str) -> Scalar {
        self.app
            .world
            .get::<PopulationCohort>(self.band)
            .expect("the band has a cohort")
            .stores
            .material_total(material)
    }

    fn knows(&self, craft: &str) -> f32 {
        let id = core_sim::crafting::craft_discovery_id(craft).expect("a shipped craft");
        self.app
            .world
            .resource::<DiscoveryProgressLedger>()
            .get_progress(FACTION, id)
            .to_f32()
    }
}

/// **THE REFUSAL, AND IT IS A ZERO RATE — NOT A BRANCH.**
///
/// A material with no `hand_working` cannot be worked bare-handed at all, so the bench's rate is `0`
/// and progress never moves. The tool supplies the rate, and then it does.
///
/// **Both halves, or "nothing happened" passes.** A test that only asserted the toolless bench makes
/// no progress would pass just as well against a bench that was never wired up at all.
#[test]
fn a_material_that_cannot_be_worked_by_hand_makes_no_progress_without_its_tool() {
    let build = |with_tool: bool| {
        let mut bench = Bench::new(
            MaterialsConfig::from_json_str(FIXTURE_MATERIALS).expect("fixture materials"),
            RecipesConfig::from_json_str(FIXTURE_RECIPES).expect("fixture recipes"),
            EquipmentConfig::from_json_str(FIXTURE_EQUIPMENT).expect("fixture equipment"),
        );
        bench.stock("ore", 40.0, &[("hardness", 0.9), ("working_temp", 0.5)]);
        if with_tool {
            bench.give_tool("crucible");
        }
        bench.start("ingot", CREW).turns(4);
        (bench.bench().progress, bench.bench().items_completed)
    };

    assert_eq!(
        build(false),
        (scalar_zero(), 0),
        "a material with no bare-handed rate must make ZERO progress with no tool - the whole \
         refusal is that zero, and there is no 'you cannot craft that' branch to find"
    );
    assert!(
        build(true).1 > 0,
        "the same recipe with the crucible must finish something, or the pairing proves nothing"
    );
}

/// **THE GRADE IS FIXED AT DRAW TIME AND NEVER MOVES.** It is not a taper.
///
/// The band draws against a pile of excellent hide, banking a `fine`-reading draw; the rest of the
/// pile is then replaced with poor stock. The pass in flight keeps the grade it drew.
#[test]
fn the_grade_a_draw_selects_does_not_move_when_the_bands_stock_does() {
    let mut bench = Bench::shipped();
    // **A ONE-HAND crew, deliberately.** The pass has to still be in flight when the stock changes
    // — a bench that finished and re-drew would be answering a different question.
    bench
        .stock(HIDE, 6.0, &[(TOUGHNESS, 0.95), (SUPPLENESS, 0.4)])
        .stock("fibre", 4.0, &[("fineness", 0.5), ("strength", 0.5)])
        .give_tool(TANNING_FRAME)
        .start(SLED, 1)
        .turns(1);

    let drawn = bench.bench().drawn.clone().expect("the draw succeeded");
    assert_eq!(
        drawn.grade.as_deref(),
        Some(FINE),
        "excellent toughness under a 0.90 tool ceiling is a fine sled"
    );

    // The excellent hide is gone; only poor hide is left. The pass in flight must not notice.
    bench.stock(HIDE, 40.0, &[(TOUGHNESS, 0.05), (SUPPLENESS, 0.9)]);
    bench.turns(1);
    assert_eq!(
        bench.bench().drawn.as_ref().and_then(|d| d.grade.as_deref()),
        Some(FINE),
        "the grade was fixed when the materials left the store - it is not a taper and it does not \
         re-read the pile"
    );
}

/// **THE TOOL CEILING CAPS THE OUTPUT: `min(material reading, ceiling)`.**
///
/// Fine flax with no loom still makes a standard basket. Here: excellent hide with no tanning frame
/// still makes a `standard` sled, because the material's own bare-handed ceiling (`0.60`) sits below
/// the `fine` seam (`0.75`) — and the same hide with the frame reaches `fine`.
#[test]
fn the_bare_handed_ceiling_caps_an_excellent_hide_to_a_standard_sled() {
    let output_grade = |with_tool: bool| {
        let mut bench = Bench::shipped();
        bench
            .stock(HIDE, 6.0, &[(TOUGHNESS, 0.95), (SUPPLENESS, 0.4)])
            .stock("fibre", 4.0, &[("fineness", 0.5), ("strength", 0.5)]);
        if with_tool {
            bench.give_tool(TANNING_FRAME);
        }
        bench.start(SLED, CREW).turns(4);
        assert_eq!(
            bench.bench().items_completed,
            1,
            "the fixture must finish exactly one sled, or the grade being read is not the one the \
             excellent hide bought"
        );
        bench
            .bench()
            .last_output_grade
            .clone()
            .expect("a graded recipe finished")
    };

    assert_eq!(
        output_grade(false),
        STANDARD,
        "the ceiling belongs to the MATERIAL, not to the absent tool, and 0.60 is below the fine seam"
    );
    assert_eq!(
        output_grade(true),
        FINE,
        "the frame's 0.90 ceiling is what lets an excellent hide read as excellent"
    );
}

/// **WEAR AND THE LESSON ARE CHARGED ON THE SAME QUANTUM — per item completed.**
///
/// Split across two sites they drift: a tool that lasts 25 items ends up teaching a craft in 30.
/// So this counts the items the bench finished and asserts *both* charges are exactly that many.
#[test]
fn the_lesson_and_the_tools_wear_are_charged_the_same_number_of_times() {
    let mut bench = Bench::shipped();
    // Enough hide and fibre for several passes, all at one rating so nothing about the draw order
    // can vary between them.
    bench
        .stock(HIDE, 60.0, &[(TOUGHNESS, 0.6), (SUPPLENESS, 0.5)])
        .stock("fibre", 40.0, &[("fineness", 0.5), ("strength", 0.5)])
        .give_tool(TANNING_FRAME)
        .start(SLED, CREW)
        .turns(12);

    let completed = bench.bench().items_completed;
    assert!(
        completed >= 2,
        "the fixture must finish more than one item, or 'the same number of times' is vacuous \
         (finished {completed})"
    );

    let equipment =
        EquipmentConfig::from_json_str(core_sim::BUILTIN_EQUIPMENT_CONFIG).expect("equipment");
    let per_item = equipment
        .item(TANNING_FRAME)
        .expect("the frame is shipped")
        .wear
        .amount;
    assert_eq!(
        bench.wear_of(TANNING_FRAME),
        per_item * completed as f32,
        "the frame must be worn exactly once per item finished"
    );

    let lesson = LadderConfigHandle::default()
        .get()
        .knowledge
        .lesson_per_crafted_item;
    let expected = (lesson * completed as f32).min(1.0);
    assert!(
        (bench.knows("tanning") - expected).abs() < 1e-3,
        "tanning must be taught exactly once per item finished: expected {expected}, got {}",
        bench.knows("tanning")
    );
}

/// **A SHORT DRAW WITHDRAWS NOTHING.** Not "withdraws until it runs out" — the availability test
/// runs over every input before a single unit moves, so a band one hide short still has all of them.
#[test]
fn a_short_draw_withdraws_nothing_at_all() {
    let mut bench = Bench::shipped();
    // The sled wants 6 hide and 2 fibre. Plenty of hide, not enough fibre.
    bench
        .stock(HIDE, 20.0, &[(TOUGHNESS, 0.6), (SUPPLENESS, 0.5)])
        .stock("fibre", 1.0, &[("fineness", 0.5), ("strength", 0.5)])
        .start(SLED, CREW)
        .turns(3);

    assert_eq!(
        bench.stock_of(HIDE),
        scalar_from_f32(20.0),
        "the hide must be untouched - a short draw is a no-op, never a half-spent pile"
    );
    assert_eq!(bench.stock_of("fibre"), scalar_from_f32(1.0));
    assert_eq!(bench.bench().progress, scalar_zero());
    assert_eq!(bench.bench().items_completed, 0);
    assert!(bench.bench().drawn.is_none());
}

/// The bench delivers: a finished sled lands in the band's ledger as **its own batch**, unworn.
///
/// **A second sled is a second sled now.** The ledger counts, so the band that started this test
/// with one nearly-spent unit ends it holding that unit *and* a fresh one — the replenishment loop
/// the whole arc exists for.
#[test]
fn a_finished_item_lands_in_the_bands_equipment_ledger_unworn() {
    let mut bench = Bench::shipped();
    bench
        .stock(HIDE, 12.0, &[(TOUGHNESS, 0.6), (SUPPLENESS, 0.5)])
        .stock("fibre", 8.0, &[("fineness", 0.5), ("strength", 0.5)]);
    // Wear the band's own sled most of the way out first, so a fresh batch is a visible change
    // rather than the reading a start-stocked band already gives.
    let equipment =
        EquipmentConfig::from_json_str(core_sim::BUILTIN_EQUIPMENT_CONFIG).expect("equipment");
    let sled = equipment.item(SLED).expect("the sled is shipped");
    let uses = (sled.default_tier().starting_durability - 1.0) / sled.wear.amount;
    bench
        .app
        .world
        .get_mut::<BandEquipment>(bench.band)
        .expect("ledger")
        .wear_item(&equipment, SLED, uses);
    assert_eq!(bench.count_of(SLED), 1, "the band starts with one sled");

    bench.start(SLED, CREW).turns(6);
    assert!(
        bench.bench().items_completed >= 1,
        "the bench must finish one"
    );
    assert_eq!(
        bench.count_of(SLED),
        1 + bench.bench().items_completed,
        "every delivered sled is a NEW one, not a repair of the first"
    );
    assert_eq!(
        bench.remaining(SLED),
        1.0,
        "the band keeps SERVING its nearly-spent sled - the fresh batch is stock, worst-first is \
         what makes the stock run out one batch at a time, and idle stock does not rot"
    );
    assert_eq!(bench.bench().last_output_grade.as_deref(), Some(STANDARD));
}

/// **The material is drawn WORST-FIRST on the axis the recipe reads** — you spend the poor hide
/// before the excellent one. Pinned through the *grade*, which is what the ordering is for: a draw
/// that took the best stock first would come out `fine` instead of `coarse`.
#[test]
fn the_draw_spends_the_poor_stock_before_the_good() {
    let mut bench = Bench::shipped();
    bench
        .stock(HIDE, 6.0, &[(TOUGHNESS, 0.05), (SUPPLENESS, 0.9)])
        .stock(HIDE, 60.0, &[(TOUGHNESS, 0.95), (SUPPLENESS, 0.4)])
        .stock("fibre", 8.0, &[("fineness", 0.5), ("strength", 0.5)])
        .start(SLED, CREW)
        .turns(1);

    assert_eq!(
        bench
            .bench()
            .drawn
            .as_ref()
            .and_then(|d| d.grade.as_deref()),
        Some(COARSE),
        "the poor hide must be spent first, or the player's best stock is silently burned on the \
         first thing they make"
    );
}

// --- validate's rejections on the equipment side -------------------------------------------------

/// A craft stat on an item that bounds no material has nothing to be the equipped side **of** — its
/// unequipped side is a property of the material, and there is no material.
#[test]
fn validate_rejects_a_craft_stat_on_an_item_that_bounds_no_material() {
    let err = mutate_shipped_equipment(|json| {
        json["items"]["spears"]["effects"]
            .as_array_mut()
            .expect("spears has effects")
            .push(serde_json::json!({ "stat": "craft_speed", "equipped": 2.0 }));
    });
    assert!(err.contains("bounds no material"), "got {err}");
}

/// A craft stat declaring an `unequipped` side is a second home for the material's own
/// `hand_working`, read by nothing.
#[test]
fn validate_rejects_a_craft_stat_declaring_an_unequipped_side() {
    let err = mutate_shipped_equipment(|json| {
        json["items"][TANNING_FRAME]["tiers"][0]["effects"][0] =
            serde_json::json!({ "stat": "craft_speed", "unequipped": 0.5 });
    });
    assert!(err.contains("unequipped"), "got {err}");
}

/// Two tools bounding one material would be picked by name order.
#[test]
fn validate_rejects_two_tools_bounding_the_same_material() {
    let err = mutate_shipped_equipment(|json| {
        json["items"]["loom"]["bounds_material"] = serde_json::json!(HIDE);
    });
    assert!(err.contains("bound"), "got {err}");
}

/// A tool serves the bench, not a party: a kit naming one would carry it onto the range, where it
/// grants nothing and is never worn.
#[test]
fn validate_rejects_a_kit_that_names_a_bench_tool() {
    let err = mutate_shipped_equipment(|json| {
        json["kits"][0]["uses"]
            .as_array_mut()
            .expect("a kit uses items")
            .push(serde_json::json!(TANNING_FRAME));
    });
    assert!(err.contains("bench tool"), "got {err}");
}

/// The bench is the only site that charges `item_crafted`, so a tool on any other quantum is
/// immortal and anything else on this one never wears at all.
#[test]
fn validate_rejects_a_tool_that_does_not_wear_per_item_crafted() {
    let err = mutate_shipped_equipment(|json| {
        json["items"][TANNING_FRAME]["wear"]["per"] = serde_json::json!("kill");
    });
    assert!(err.contains("item_crafted"), "got {err}");
}

/// A `bounds_material` naming something the materials table does not carry would make the item the
/// bench tool for nothing — the `UnknownItem` debt, paid at the composition seam.
#[test]
fn validate_against_materials_rejects_a_tool_bounding_an_unknown_material() {
    let mut json: serde_json::Value =
        serde_json::from_str(core_sim::BUILTIN_EQUIPMENT_CONFIG).expect("json");
    json["items"][TANNING_FRAME]["bounds_material"] = serde_json::json!("hyde");
    let config = EquipmentConfig::from_json_str(&json.to_string())
        .expect("the item table is still self-consistent");
    let err = config
        .validate_against_materials(
            &MaterialsConfig::from_json_str(core_sim::BUILTIN_MATERIALS_CONFIG).expect("materials"),
        )
        .expect_err("a tool bounding nothing must be rejected")
        .to_string();
    assert!(err.contains("not a material"), "got {err}");
}

fn mutate_shipped_equipment(mutate: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut json: serde_json::Value =
        serde_json::from_str(core_sim::BUILTIN_EQUIPMENT_CONFIG).expect("json");
    mutate(&mut json);
    EquipmentConfig::from_json_str(&json.to_string())
        .expect_err("the mutated table must be rejected")
        .to_string()
}

// --- the "cannot be worked by hand" fixture ------------------------------------------------------
//
// The shipped roster is all organic, so nothing in it refuses bare hands — the state metal will be
// in the day the minerals arc gives it a producer. Exercised by a fixture instead, for the same
// reason the bronze equipment tier and the variety presets are.

const FIXTURE_MATERIALS: &str = r#"{
  "characteristic_bands": [
    { "name": "poor", "from": 0.0 }, { "name": "good", "from": 0.5 }
  ],
  "materials": {
    "ore": { "craft": "tanning", "characteristics": ["hardness", "working_temp"] }
  }
}"#;

const FIXTURE_RECIPES: &str = r#"{
  "crafting": { "progress_per_worker_turn": 1.0 },
  "recipes": {
    "ingot": {
      "display_name": "Ingot",
      "craft": "tanning",
      "work": 4.0,
      "inputs": [{ "material": "ore", "amount": 4.0, "reads": "hardness" }],
      "outputs": [{ "equipment": "spears", "amount": 1.0 }],
      "grades": { "rough": { "when": 0.0 } }
    }
  }
}"#;

const FIXTURE_EQUIPMENT: &str = r#"{
  "items": {
    "spears": {
      "wear": { "per": "kill", "amount": 0.4 },
      "tiers": [
        {
          "id": "flint",
          "starting_durability": 100.0,
          "effects": [{ "stat": "attack", "equipped": 20.0 }]
        }
      ]
    },
    "crucible": {
      "wear": { "per": "item_crafted", "amount": 4.0 },
      "bounds_material": "ore",
      "tiers": [
        {
          "id": "flint",
          "starting_durability": 100.0,
          "effects": [
            { "stat": "craft_speed", "equipped": 1.0 },
            { "stat": "craft_quality_ceiling", "equipped": 1.0 }
          ]
        }
      ]
    }
  },
  "kits": [
    { "id": "big_game", "display_name": "Stalking kit", "jobs": ["hunt"], "uses": ["spears"] },
    { "id": "none", "display_name": "No kit", "jobs": ["hunt", "forage", "scout", "warrior"], "uses": [] }
  ],
  "default_kits": { "hunt": "big_game", "forage": "none", "scout": "none", "warrior": "none" },
  "quarry_default_kit_margin": 0.25,
  "life_readout": { "warn_fraction": 0.34, "danger_fraction": 0.10 }
}"#;

/// **A PASS THAT MAKES THREE MAKES THREE.** `RecipeOutput::amount` used to be read by nothing on
/// the equipment side: one completion delivered one item's worth of *condition* however many the
/// row named, which a ledger that counts things has no excuse for.
///
/// Exercised by a fixture rather than a shipped recipe, because every shipped equipment recipe
/// makes exactly one — the defect was invisible for that reason and would stay invisible.
#[test]
fn a_completion_delivers_the_recipes_whole_output_amount() {
    let mut book: serde_json::Value =
        serde_json::from_str(core_sim::BUILTIN_RECIPES_CONFIG).expect("the book is json");
    const BATCH: u32 = 3;
    book["recipes"][SLED]["outputs"][0]["amount"] = serde_json::json!(BATCH);
    let mut bench = Bench::new(
        MaterialsConfig::from_json_str(core_sim::BUILTIN_MATERIALS_CONFIG).expect("materials"),
        RecipesConfig::from_json_str(&book.to_string())
            .expect("a batch of three is a valid recipe"),
        EquipmentConfig::from_json_str(core_sim::BUILTIN_EQUIPMENT_CONFIG).expect("equipment"),
    );
    bench
        .stock(HIDE, 12.0, &[(TOUGHNESS, 0.6), (SUPPLENESS, 0.5)])
        .stock("fibre", 8.0, &[("fineness", 0.5), ("strength", 0.5)])
        .start(SLED, CREW)
        .turns(6);

    let completed = bench.bench().items_completed;
    assert!(completed >= 1, "the bench must finish at least one pass");
    assert_eq!(
        bench.count_of(SLED),
        1 + completed * BATCH,
        "each completed pass delivers the row's whole `amount`, on top of the one the band spawned \
         with"
    );
}

/// **THE GRADE THE DRAW SELECTED RIDES THE BATCH, AND ITS ABSOLUTES WITH IT.**
///
/// A grade declares what a stat *takes* on the thing that was made, so the value has to reach the
/// item — and it is copied at craft time rather than looked up later, which is what makes *"fixed at
/// craft time and never moves"* structural: swapping the bench's recipe afterwards cannot re-grade a
/// sled already in the band's hands.
///
/// Both directions: a **fine** sled hauls more than the shipped rate, and a **standard** one hauls
/// exactly it — the rule that makes a standard-grade craft reproduce today's game.
#[test]
fn a_crafted_items_grade_decides_what_it_grants() {
    let equipment =
        EquipmentConfig::from_json_str(core_sim::BUILTIN_EQUIPMENT_CONFIG).expect("equipment");
    let recipes = RecipesConfig::from_json_str(core_sim::BUILTIN_RECIPES_CONFIG).expect("recipes");
    let labor = core_sim::LaborConfig::builtin();
    let big_game = equipment
        .kit("big_game")
        .expect("the roster ships big_game");
    let shipped = equipment
        .item(SLED)
        .expect("the sled is shipped")
        .default_tier()
        .effects
        .iter()
        .find(|effect| effect.stat == core_sim::EquipmentStat::HuntCarry)
        .map(|effect| effect.tier.value())
        .expect("the sled's tier declares a haul rate");

    let haul_at = |grade_id: &str| {
        let grade = recipes
            .recipe(SLED)
            .expect("the sled recipe is shipped")
            .grades
            .get(grade_id)
            .unwrap_or_else(|| panic!("the sled recipe ships a '{grade_id}' grade"));
        let mut ledger = BandEquipment::default();
        ledger.stock(
            SLED,
            1,
            FLINT_TIER,
            Some(core_sim::BatchGrade {
                id: grade_id.to_string(),
                effects: grade.effects.clone(),
            }),
        );
        equipment.hunt_per_worker_biomass_capacity(
            labor.hunt.per_worker_biomass_capacity,
            &big_game,
            &ledger,
        )
    };

    assert_eq!(
        haul_at(STANDARD),
        shipped,
        "a STANDARD craft reproduces the shipped sled exactly"
    );
    assert!(
        haul_at(FINE) > shipped,
        "a fine sled hauls more: {} vs {shipped}",
        haul_at(FINE)
    );
    assert!(
        haul_at("coarse") < shipped,
        "and a coarse one less: {} vs {shipped}",
        haul_at("coarse")
    );
}

/// **A CRAFT COMES OUT AT THE BEST TIER THE FACTION KNOWS**, which on the shipped roster is the one
/// that ships known — so the shipped opening makes exactly what it always made.
#[test]
fn a_delivered_item_carries_the_tier_that_ships_known() {
    let mut bench = Bench::shipped();
    bench
        .stock(HIDE, 12.0, &[(TOUGHNESS, 0.6), (SUPPLENESS, 0.5)])
        .stock("fibre", 8.0, &[("fineness", 0.5), ("strength", 0.5)])
        .start(SLED, CREW)
        .turns(6);
    assert!(bench.bench().items_completed >= 1, "the bench finishes one");

    let ledger = bench
        .app
        .world
        .get::<BandEquipment>(bench.band)
        .expect("the band has a ledger");
    let batches: Vec<_> = ledger
        .batches()
        .find(|(item, _)| *item == SLED)
        .map(|(_, batches)| batches.to_vec())
        .expect("the band holds sleds");
    assert!(
        batches.iter().all(|batch| batch.tier == FLINT_TIER),
        "every batch is at the tier that ships known: {batches:?}"
    );
    assert!(
        batches
            .iter()
            .any(|batch| batch.grade.as_ref().is_some_and(|g| g.id == STANDARD)),
        "and the crafted one carries the grade its draw selected: {batches:?}"
    );
}
