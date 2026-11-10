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
    advance_crafting, deliver_bench_output, scalar_from_f32, scalar_one, scalar_zero, BandBench,
    BandEquipment, BandId, BenchOrder, DiscoveryProgressLedger, EquipmentConfig,
    EquipmentConfigHandle, FactionId, GenerationId, LadderConfigHandle, LocalStore,
    MaterialsConfig, MaterialsConfigHandle, MoraleCause, PopulationCohort, RecipesConfig,
    RecipesConfigHandle, Scalar, WearQuantum, HEAD_ORDER,
};
use std::collections::BTreeMap;

const FACTION: FactionId = FactionId(0);
/// **An order no fixture finishes** — what the retired repeat-until-cleared job was, as a count.
const NEVER_FINISHED: u32 = u32::MAX;

/// **The bench works `recipe` and nothing else, with `workers` on it** — the queue holding one
/// [`NEVER_FINISHED`] order.
fn put_on_bench(bench: &mut BandBench, recipe: &str, workers: u32) {
    bench.orders = vec![BenchOrder::new(recipe, NEVER_FINISHED)];
    bench.orders[0].crew = core_sim::BenchCrew {
        at_tool: 0,
        by_hand: workers,
    };
}
const BAND_POP: u32 = 30;
/// A crew big enough that the bench finishes in a small number of turns at the bare-handed rate, so
/// a test can measure a completion without simulating a season.
const CREW: u32 = 8;
/// **One hand at the bench** - too few to finish a hurdle pass in a single turn at the bare-handed
/// rate, so a test that reads the pile one pass drew sees exactly one pass drawn (a bigger crew now
/// completes an item inside the turn and draws the next pass off the other pile).
const ONE_HAND_AT_THE_BENCH: u32 = 1;

const HIDE: &str = "hide";
const TOUGHNESS: &str = "toughness";
const SUPPLENESS: &str = "suppleness";
/// The grade names — which are the `characteristic_bands` rungs, because there is **one** quality
/// ladder for the whole game. Spelled here because a test asserting *which* grade a draw came out at
/// has to name one.
const POOR: &str = "poor";
/// **The ANCHOR band on the shipped tables** — the rung all three organics' bare-handed
/// `hand_working.quality_ceiling` of `0.60` falls in, and therefore the one pinned to the shipped
/// item.
const GOOD: &str = "good";
const EXCELLENT: &str = "excellent";
const TANNING_FRAME: &str = "tanning_frame";
/// The sled's **second** input and its two axes. A draw is all-or-nothing, so a fixture that stocks
/// only the hide withdraws nothing whatever the code does — which is how a draw-gate assertion comes
/// to pass against the very defect it is for.
const FIBRE: &str = "fibre";
const FINENESS: &str = "fineness";
const STRENGTH: &str = "strength";
const SLED: &str = "sled";
/// **The game's only material-only recipe** — 4 wood + 0.5 hide, reading hide's `suppleness`.
const HURDLES: &str = "hurdles";
/// [`HURDLES`]' other input and its two axes.
const WOOD: &str = "wood";
const HARDNESS: &str = "hardness";
const PLIANCY: &str = "pliancy";
/// **One pass of [`HURDLES`]' hide**, per `recipes.json` — stocked as a whole pile so *"that pile is
/// gone"* is the observation, rather than a partial draw off two.
const ONE_HURDLES_PASS_OF_HIDE: f32 = 0.5;
/// Wood enough for several passes; the wood row names no `reads`, so its own spend order is the
/// fallback and nothing here is about it.
const WOOD_TO_SPARE: f32 = 40.0;
/// **The pile that must be left alone**, large enough that a draw off it would be unmistakable.
const THE_UNTOUCHED_PILE: f32 = 60.0;
/// The OPENING tier every shipped item carries — the bone, hide and fibre gear a band starts with.
const PLAIN_TIER: &str = "plain";

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
        last_food_need: 0.0,
        fed_party_workers: Default::default(),
        last_food_spoiled: 0.0,
        last_food_into_store: 0.0,
        last_food_from_store: 0.0,
        last_food_kept: 0.0,
        last_turn_food_transfers: Default::default(),
        last_turn_fodder_transfers: Default::default(),
        last_turn_transfer_crossings: Vec::new(),
        last_morale_delta: scalar_zero(),
        last_morale_cause: MoraleCause::None,
        last_morale_contributions: Default::default(),
        last_fertility_factors: Default::default(),
        last_breeding: Default::default(),
        discontent_fraction: scalar_zero(),
        grievance: scalar_zero(),
        last_emigrated: 0,
        last_immigrated: 0,
        age_turns: 0,
        generation: 0 as GenerationId,
        faction: FACTION,
        knowledge: Vec::new(),
        founding_lines: core_sim::FoundingLines::founded(
            core_sim::BandId(0),
            core_sim::MIN_BAND_LINES,
        ),
        belief_anchor: None,
        last_belief_relay_hops: 0,
        predators_kept_off: 0,
        last_cohesion: Default::default(),
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

    /// **Put `recipe` on the bench as its only order, with `workers` on it** — an order long enough
    /// ([`NEVER_FINISHED`]) that no fixture here pops it, so a test about one recipe's passes is
    /// never about the queue moving on.
    fn start(&mut self, recipe: &str, workers: u32) -> &mut Self {
        put_on_bench(
            &mut self
                .app
                .world
                .get_mut::<BandBench>(self.band)
                .expect("the band has a bench"),
            recipe,
            workers,
        );
        self
    }

    /// The head order — the one every fixture here works.
    fn head(&self) -> &BenchOrder {
        self.bench()
            .orders
            .first()
            .expect("the fixture's order is still on the bench")
    }

    /// **`count` turns of the bench, in the schedule's order** — each opens with
    /// [`deliver_bench_output`] (what the bench finished last turn reaches the store, before the turn's
    /// first stage) and then works the bench.
    fn turns(&mut self, count: u32) -> &mut Self {
        for _ in 0..count {
            self.app.world.run_system_once(deliver_bench_output);
            self.app.world.run_system_once(advance_crafting);
        }
        self
    }

    /// **The top of the next turn, and nothing else of it** — what the last turn's bench finished
    /// reaches the store. A finished item is parked on the bench until then, so a test about what
    /// the store holds after a completion has to cross this line first.
    fn deliver(&mut self) -> &mut Self {
        self.app.world.run_system_once(deliver_bench_output);
        self
    }

    fn bench(&self) -> &BandBench {
        self.app
            .world
            .get::<BandBench>(self.band)
            .expect("the band has a bench")
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

    /// **Every batch of `material` as `(its reading on `axis`, its amount)`.** [`Self::stock_of`]
    /// sums the batches and therefore cannot tell two piles apart, which is the entire distinction a
    /// claim about *which pile was spent* rests on.
    fn batches_by(&self, material: &str, axis: &str) -> Vec<(f32, f32)> {
        self.app
            .world
            .get::<PopulationCohort>(self.band)
            .expect("the band has a cohort")
            .stores
            .material_batches(material)
            .map(|(_, batch)| {
                (
                    batch.characteristics.get(axis).copied().unwrap_or_default(),
                    batch.amount.to_f32(),
                )
            })
            .collect()
    }

    fn stock_of(&self, material: &str) -> Scalar {
        self.app
            .world
            .get::<PopulationCohort>(self.band)
            .expect("the band has a cohort")
            .stores
            .material_total(material)
    }
}

/// **A SHORT DRAW WITHDRAWS NOTHING.** Not "withdraws until it runs out" — the availability test
/// runs over every input before a single unit moves, so a band one hide short still has all of them.
#[test]
fn a_short_draw_withdraws_nothing_at_all() {
    let mut bench = Bench::shipped();
    // The sled wants 2 hide and 2 fibre. Plenty of hide, not enough fibre.
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
    assert_eq!(bench.head().progress_total(), scalar_zero());
    assert_eq!(bench.head().made, 0);
    assert!(bench.head().items.is_empty());
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
    let uses = (sled.default_tier().starting_durability - 1.0) / sled.headline_wear().amount;
    bench
        .app
        .world
        .get_mut::<BandEquipment>(bench.band)
        .expect("ledger")
        .wear_item(&equipment, SLED, WearQuantum::BiomassHauled, uses);
    assert_eq!(bench.count_of(SLED), 1, "the band starts with one sled");

    bench.start(SLED, CREW).turns(6).deliver();
    assert!(bench.head().made >= 1, "the bench must finish one");
    assert_eq!(
        bench.count_of(SLED),
        1 + bench.head().made,
        "every delivered sled is a NEW one, not a repair of the first"
    );
    assert_eq!(
        bench.remaining(SLED),
        1.0,
        "the band keeps SERVING its nearly-spent sled - the fresh batch is stock, worst-first is \
         what makes the stock run out one batch at a time, and idle stock does not rot"
    );
    assert_eq!(bench.bench().last_output_grade.as_deref(), Some(GOOD));
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
            .head()
            .items
            .first()
            .and_then(|item| item.drawn.grade.as_deref()),
        Some(POOR),
        "the poor hide must be spent first, or the player's best stock is silently burned on the \
         first thing they make"
    );
}

/// **THE SPEND ORDER DID NOT MOVE WHEN THE GRADE STOPPED BEING PUBLISHED.**
///
/// `reads` says **two** things: the axis the output's grade is read from, and the axis the input is
/// spent worst-first on (`systems::crafting::spend_axis`, which otherwise falls back to the
/// material's **first declared axis**). `hurdles` makes only a material and so is stamped with no
/// grade — but it still declares `reads: "suppleness"`, and that half is load-bearing. Deleting the
/// word to silence the grade would move the bench off hide's `suppleness` and onto its first
/// declared axis, `toughness`, and quietly eat a different pile.
///
/// This is asserted directly rather than through the grade, because the grade the sibling test pins
/// through is exactly what a material-only recipe no longer has. Two piles that are opposites on the
/// two axes: **suppleness decides which one goes**, and toughness would pick the other.
#[test]
fn the_hurdles_draw_still_spends_hide_by_suppleness_and_not_by_its_first_axis() {
    let mut bench = Bench::shipped();
    bench
        // Worst on SUPPLENESS, best on toughness — what `reads` picks, and exactly one pass of it.
        .stock(
            HIDE,
            ONE_HURDLES_PASS_OF_HIDE,
            &[(TOUGHNESS, 0.95), (SUPPLENESS, 0.05)],
        )
        // Worst on TOUGHNESS — what the fallback would pick.
        .stock(
            HIDE,
            THE_UNTOUCHED_PILE,
            &[(TOUGHNESS, 0.05), (SUPPLENESS, 0.95)],
        )
        .stock(WOOD, WOOD_TO_SPARE, &[(HARDNESS, 0.5), (PLIANCY, 0.6)])
        .start(HURDLES, ONE_HAND_AT_THE_BENCH)
        .turns(1);

    assert!(
        !bench.head().items.is_empty(),
        "fixture: the pass must have drawn its pile, or nothing has been spent to have an order"
    );
    assert_eq!(
        bench.head().items.first().and_then(|item| item.drawn.grade.as_deref()),
        None,
        "a material-only recipe stamps no grade - which is why this test observes the piles instead"
    );
    let remaining = bench.batches_by(HIDE, SUPPLENESS);
    assert_eq!(
        remaining,
        vec![(0.95, THE_UNTOUCHED_PILE)],
        "the suppleness-poor pile is spent WHOLE and the suppleness-good one is untouched - a draw \
         ordered by hide's first declared axis (toughness) would have taken the other one"
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
        json["items"][TANNING_FRAME]["wear"][0]["per"] = serde_json::json!("strike");
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
        .turns(6)
        .deliver();

    let completed = bench.head().made;
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
/// Both directions: an **excellent** sled hauls more than the shipped rate, and a **good** one — the
/// anchor band — hauls exactly it, which is the rule that makes a bare-handed craft reproduce today's
/// game.
#[test]
fn a_crafted_items_grade_decides_what_it_grants() {
    let equipment =
        EquipmentConfig::from_json_str(core_sim::BUILTIN_EQUIPMENT_CONFIG).expect("equipment");
    let recipes = RecipesConfig::from_json_str(core_sim::BUILTIN_RECIPES_CONFIG).expect("recipes");
    let materials =
        MaterialsConfig::from_json_str(core_sim::BUILTIN_MATERIALS_CONFIG).expect("materials");
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
        // **Resolved through the inheritance rule**, exactly as the bench resolves it — a band the
        // recipe does not declare takes the one below it.
        let effects = recipes
            .recipe(SLED)
            .expect("the sled recipe is shipped")
            .grade_effects_for(grade_id, &materials);
        let mut ledger = BandEquipment::default();
        ledger.stock(
            SLED,
            1,
            PLAIN_TIER,
            Some(core_sim::BatchGrade {
                id: grade_id.to_string(),
                effects: effects.to_vec(),
            }),
        );
        equipment.hunt_per_worker_biomass_capacity(
            labor.hunt.per_worker_biomass_capacity,
            &big_game,
            &ledger,
        )
    };

    assert_eq!(
        haul_at(GOOD),
        shipped,
        "a craft at the ANCHOR band reproduces the shipped sled exactly"
    );
    assert!(
        haul_at(EXCELLENT) > shipped,
        "an excellent sled hauls more: {} vs {shipped}",
        haul_at(EXCELLENT)
    );
    assert!(
        haul_at(POOR) < shipped,
        "and a poor one less: {} vs {shipped}",
        haul_at(POOR)
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
        .turns(6)
        .deliver();
    assert!(bench.head().made >= 1, "the bench finishes one");

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
        batches.iter().all(|batch| batch.tier == PLAIN_TIER),
        "every batch is at the tier that ships known: {batches:?}"
    );
    assert!(
        batches
            .iter()
            .any(|batch| batch.grade.as_ref().is_some_and(|g| g.id == GOOD)),
        "and the crafted one carries the grade its draw selected: {batches:?}"
    );
}

/// ⛔ **A FINISHED ITEM IS PARKED ON THE BENCH, AND A JOB CHANGE BEFORE THE NEXT TURN DOES NOT LOSE
/// IT** (issue #720).
///
/// The bench runs after the labour pass that settles the band's tools, so a tool it finishes cannot
/// be issued that turn — and it no longer sits in the store pretending it was. It waits on
/// `BandBench::finished` until the top of the next turn stocks it.
///
/// **Both job changes the player can make between turns** — a clear and a re-task — must carry the
/// parked batch across: the item is already made, and only the next one is being cancelled.
#[test]
fn a_finished_item_waits_on_the_bench_and_survives_a_job_change() {
    /// One pass of the sled's own inputs (`recipes.json`: 2 hide + 2 fibre), so the bench finishes
    /// exactly one and cannot re-draw.
    const ONE_PASS_OF_HIDE: f32 = 2.0;
    const ONE_PASS_OF_FIBRE: f32 = 2.0;
    /// Enough turns for [`CREW`] bare-handed to finish one `work: 8` pass, with room to spare.
    const TURN_LIMIT: u32 = 8;
    /// The sled a start-stocked band already holds.
    const STARTING_SLEDS: u32 = 1;
    const ONE_PARKED_BATCH: usize = 1;
    /// What the re-task puts on the bench instead — any other recipe will do.
    const RETASKED_RECIPE: &str = "baskets";

    type JobChange = fn(&mut BandBench);
    let changes: [(&str, JobChange); 2] = [
        ("cleared", |bench| {
            bench
                .remove_order(HEAD_ORDER)
                .expect("the fixture's order is on the bench");
        }),
        ("re-tasked", |bench| {
            put_on_bench(bench, RETASKED_RECIPE, CREW)
        }),
    ];
    for (label, change) in changes {
        let mut bench = Bench::shipped();
        bench
            .stock(
                HIDE,
                ONE_PASS_OF_HIDE,
                &[(TOUGHNESS, 0.6), (SUPPLENESS, 0.5)],
            )
            .stock(
                FIBRE,
                ONE_PASS_OF_FIBRE,
                &[(FINENESS, 0.5), (STRENGTH, 0.5)],
            )
            .start(SLED, CREW);
        let mut turns = 0;
        while bench.head().made == 0 && turns < TURN_LIMIT {
            bench.turns(1);
            turns += 1;
        }
        assert_eq!(
            bench.head().made,
            1,
            "{label}: fixture — the bench must finish the sled"
        );
        assert_eq!(
            bench.count_of(SLED),
            STARTING_SLEDS,
            "{label}: the turn that finished the sled does NOT put it in the store"
        );
        assert_eq!(
            bench.bench().finished.len(),
            ONE_PARKED_BATCH,
            "{label}: …it is parked on the bench instead"
        );

        change(
            &mut bench
                .app
                .world
                .get_mut::<BandBench>(bench.band)
                .expect("the band has a bench"),
        );
        assert_eq!(
            bench.bench().finished.len(),
            ONE_PARKED_BATCH,
            "{label}: the job change leaves the finished sled where it was"
        );

        bench.deliver();
        assert_eq!(
            bench.count_of(SLED),
            STARTING_SLEDS + 1,
            "{label}: the top of the next turn stocks the parked sled"
        );
        assert!(
            bench.bench().finished.is_empty(),
            "{label}: …and empties the bench's parking, so it is delivered once"
        );
    }
}
