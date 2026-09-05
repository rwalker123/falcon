//! **THE FOOD ECONOMY, BOTH WEBS, ONE SET OF INPUTS** — a *printing* harness, not a balance test.
//!
//! ```text
//! cargo test -p core_sim --test food_economy_table -- --nocapture
//! ```
//!
//! Its job is to be **read**. It prices every source the game can put a worker on — the plant
//! ladder's three rungs on one pinned reference basket, and every roster species at every rung its
//! `husbandry_ceiling` allows — through the *same two seams the game itself quotes a player with*
//! ([`forage_source_yield_preview`] / [`hunt_source_yield_preview`]), and lays the results beside
//! each other. Nothing here re-derives a yield: if the number a column wants is not reachable
//! through those two functions, it is fetched from the sim's own take path or the column says `n/a`
//! (see "What the two seams cannot answer").
//!
//! # Why it exists
//!
//! Three observations from play — *harvesting beats hunting, it beats it at every rung, and a sown
//! Field is a mega-producer* — are all comparative claims, and none of them can be checked against a
//! per-arc fixture that quotes one web at a time. This prints the table the comparison needs before
//! any dial is touched.
//!
//! # ⛔ IT PRICES A KITTED BAND, BECAUSE THAT IS THE ONLY BAND THE GAME EVER HANDS A PLAYER
//!
//! The first draft priced both webs **bare-handed** off `labor_config`'s
//! `per_worker_biomass_capacity`, which is the pre-gear baseline and describes a band that does not
//! exist in play: every spawn inserts a start-stocked ledger and every job resolves a default kit.
//! Since the plant web is **carry-bound**, that understated flora by the whole basket tier while
//! barely moving fauna — i.e. it got the one axis the entire comparison turns on wrong.
//!
//! Every headline column is now resolved through the **same seam the server's assign-time seed
//! uses** (`bin/server.rs`, the `LaborTarget::Forage` / `LaborTarget::Hunt` arms): the job's
//! `default_kit` (or, on the animal side, [`herd_default_hunt_kit`] — the *quarry's* own default,
//! which is what the wire publishes as `defaultKitId`), against a `BandEquipment::start_stocked_for`
//! ledger, divided by the same [`EquipmentConfig::coverage`], and the party composed by
//! [`PartyResolution::party_against`]. The **bare-handed figure rides beside it as a control**, so
//! how much of any row is gear stays visible.
//!
//! # The fairness rule
//!
//! Every row is priced with the same `output_multiplier`, the same seasonal weight, the same harvest
//! floor, the same forecast band width, the same crew sizes, and — crucially — the same *operating
//! point*: each source is seated at `floor · K`, the stock the shipped floor settles it on, so no row
//! is quoting a windfall drawdown from a full stand that the next row has already spent.
//!
//! The carry rate is deliberately **per-web** (a basket and a sled are two different model claims,
//! not two spellings of one dial), and the bare column is what makes that difference measurable
//! rather than assumed.
//!
//! # What the two seams cannot answer
//!
//! - **Materials.** [`core_sim::SourceYield::materials`] is always empty from a preview — a
//!   pre-commit row quotes no material, by design, because `credit_material_yield` is paid off a take
//!   in *biomass* and the preview path resolves the take in currency space. So the materials column
//!   is built the way the shipped herd/patch rows build theirs: the source's own material rows through
//!   [`core_sim::material_yield_totals`] (the one expression every material quote in the sim goes
//!   through), off the biomass **the food column was itself paid from** —
//!   `food ÷ provisions_per_biomass`, a division of two published numbers rather than a second yield
//!   derivation. A species with no food axis (`wolf`) cannot have that division inverted, and its row
//!   says `n/a` instead of `0`.
//! - **Which stage capped the take.** The preview's `realized` is the *unquantised* projection and
//!   never reaches [`core_sim::hunt_take_bound`]. So the binding stage is taken from the **live take
//!   path**: the harness drives `systems::hunt_take` forward over the same horizon on a private
//!   clone — Logistics regrowth then Population take, the shipped order — and reads
//!   `HuntOutcome::bound` off each turn. The sim produces the bound; the harness only tallies it.
//!   The plant web has no such enum (it has no engagement, no retreat and no fight), so its rows
//!   print `n/a` there and answer with carry utilisation alone.
//!
//! # Liveness
//!
//! A table of zeros that exits `0` is worse than no table, so the printing is followed by assertions
//! that every printed rate is finite, that both webs actually paid something, and that no fauna row
//! left its binding-stage column empty.

use std::collections::BTreeMap;
use std::sync::Arc;

use core_sim::{
    animals_engaged, crop_field_cost_multiplier, forage_provisions, forage_source_yield_preview,
    herd_capacity, herd_default_hunt_kit, herd_density_gain, herd_ecology, herd_engage_rate,
    herd_hunt_yield, herd_space_capacity, herd_take_room, herd_upkeep_demand,
    hunt_source_yield_preview, hunt_take, material_yield_totals, patch_carrying_capacity,
    patch_composition, patch_ecology, patch_field_cost_multiplier, patch_material_yields_taking,
    patch_provisions_per_biomass_taking, patch_upkeep_demand, plant_rung_span, regrow_biomass,
    selected_biomass_share, sustainable_yield, BandEquipment, CombatConfig, CreaturesConfig,
    EquipmentConfig, FactionId, FaunaConfig, FloraConfig, ForagePatch, Herd, HuntDraw,
    HuntTakeBound, HuntingParty, HusbandryCeiling, KitChoice, KitCoverage, KitJob, LaborConfig,
    LadderConfig, MaterialPayoff, PartyResolution, Quarry, RungKey, SourceYield, SpeciesDef,
    TakeSelection, DEFAULT_ESCAPEMENT_FLOOR,
};
use sim_runtime::TerrainType;

mod common;
/// **The seed, tile, ground and crop are the SHARED pin** — `field_reference_basket.rs` asserts rung
/// 3's payoff on the same realization, and two copies of it would drift.
use common::reference_basket as basket;

// ---------------------------------------------------------------------------------------------
// Harness parameters — every one of them a *measurement* choice, never a gameplay lever. Anything
// the game decides is read from the shipped JSON below, through the ordinary load path.
// ---------------------------------------------------------------------------------------------

/// Neutral band productivity, so every row is the source's own figure rather than a band bonus's.
const UNIT_OUTPUT_MULTIPLIER: f32 = 1.0;
/// A full growing season, so a plant row is the ground's own figure rather than a calendar's.
const FULL_SEASONAL_WEIGHT: f32 = 1.0;
/// The crews every row is priced at: a lone worker, the ladder's reference crew of three, and the
/// five the live readings in the validation block were taken at.
const CREWS: [u32; 3] = [1, 3, 5];
/// The crew Section C and the materials column quote — the reference crew, so one column is not a
/// lone worker's and its neighbour a crew's.
const REFERENCE_CREW: u32 = CREWS[1];
/// Where [`REFERENCE_CREW`] sits in [`CREWS`], so the two can never drift apart.
const REFERENCE_SLOT: usize = 1;
/// The faction that owns every improvement this harness seats.
const TABLE_FACTION: FactionId = FactionId(0);
/// A herd fixture's roaming route: one tile, because nothing here moves.
const FIXTURE_TILES: [bevy::math::UVec2; 1] = [bevy::math::UVec2::new(1, 1)];
/// **The synthetic map the fixture's footprint disk is counted on** — big enough that no footprint
/// clips an edge, so a species' tile count is its full disk. `hex_range_tiles` is the sim's own
/// counter; only the map it is asked about is the harness's.
const FIXTURE_MAP: u32 = 64;
/// The fixture map does not wrap; at [`FIXTURE_MAP`] nothing reaches an edge to care.
const FIXTURE_WRAP: bool = false;
/// A fixture herd is anchored well inside [`FIXTURE_MAP`], so its disk is whole.
const FIXTURE_ANCHOR: bevy::math::UVec2 = bevy::math::UVec2::new(32, 32);
/// A rate the harness renders as "no denominator" rather than dividing by.
const NO_UPKEEP: f32 = 0.0;
/// A resident band banks its whole take — `systems::hunt_take`'s own contract for the band arm.
const NO_CARRY_LIMIT: f32 = f32::INFINITY;
/// The crews the mammoth diagnosis sweeps. It reaches far past the table's own range on purpose:
/// the question is *what does adding hands do*, and the answer changes shape past the point where
/// the party can put a whole 800-unit body on the ground.
const MAMMOTH_CREWS: [u32; 4] = [1, 3, 5, 20];
/// The species that diagnosis is about.
const MAMMOTH: &str = "mammoth";
/// A row disagreeing with a live reading by more than this is called out rather than glossed.
const VALIDATION_TOLERANCE: f32 = 0.25;

// ---------------------------------------------------------------------------------------------
// Ground truth — readings taken off a RUNNING GAME, not computed here
// ---------------------------------------------------------------------------------------------

/// **What the live game showed, at one worker with the default kits.** These are observations, not
/// levers: they exist to be disagreed with. The harness is never tuned to fit them — a fixture bent
/// to match its own acceptance data measures nothing.
///
/// The five-worker readings were `food/turn` for the whole crew and are divided by the crew here, so
/// every entry below is in the one unit the table reports: **provisions per worker per turn**.
const LIVE_READINGS: &[LiveReading] = &[
    LiveReading {
        label: "harvest tile, whole basket",
        food_per_worker: 0.21,
        crew: 1,
    },
    LiveReading {
        label: "harvest tile, food only",
        food_per_worker: 0.33,
        crew: 1,
    },
    LiveReading {
        label: "harvest tile, whole basket",
        food_per_worker: 1.04 / 5.0,
        crew: 5,
    },
    LiveReading {
        label: "Silt Catfish",
        food_per_worker: 0.13,
        crew: 1,
    },
    LiveReading {
        label: "Red Deer",
        food_per_worker: 0.11,
        crew: 1,
    },
    LiveReading {
        label: "Red Deer",
        food_per_worker: 0.53 / 5.0,
        crew: 5,
    },
    LiveReading {
        label: "Wild Sheep",
        food_per_worker: 0.08,
        crew: 1,
    },
    LiveReading {
        label: "Wild Boar",
        food_per_worker: 0.06,
        crew: 1,
    },
    LiveReading {
        label: "Wild Boar",
        food_per_worker: 0.30 / 5.0,
        crew: 5,
    },
    LiveReading {
        label: "Wild Fowl",
        food_per_worker: 0.03,
        crew: 1,
    },
];

/// One reading off the running game.
struct LiveReading {
    /// Matches a printed row's source label, or the harvest-tile fixture below.
    label: &'static str,
    /// Provisions per worker per turn.
    food_per_worker: f32,
    /// The crew it was read at.
    crew: u32,
}

/// **The ground the live harvest readings were taken on.** A river delta, which is a different
/// biome from the pinned reference basket Section A quotes — so the validation block asks the same
/// machinery about *this* biome rather than comparing two different tiles and calling the difference
/// an error.
const LIVE_HARVEST_TERRAIN: TerrainType = TerrainType::RiverDelta;

// ---------------------------------------------------------------------------------------------
// The shared inputs, resolved once
// ---------------------------------------------------------------------------------------------

/// Everything read from the shipped JSON, loaded through the ordinary builtin path.
struct Shipped {
    labor: Arc<LaborConfig>,
    flora: Arc<FloraConfig>,
    fauna: Arc<FaunaConfig>,
    ladder: Arc<LadderConfig>,
    combat: Arc<CombatConfig>,
    equipment: Arc<EquipmentConfig>,
    creatures: Arc<CreaturesConfig>,
}

impl Shipped {
    fn load() -> Self {
        Self {
            labor: LaborConfig::builtin(),
            flora: FloraConfig::builtin(),
            fauna: FaunaConfig::builtin(),
            ladder: LadderConfig::builtin(),
            combat: CombatConfig::builtin(),
            equipment: EquipmentConfig::builtin(),
            creatures: CreaturesConfig::builtin(),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The gear — resolved exactly as the server's assign-time seed resolves it
// ---------------------------------------------------------------------------------------------

/// **What a fresh band carries on this job**, and how that gear divides its people — the three terms
/// `bin/server.rs`'s seed arms build before they call either preview.
struct Outfit {
    kit: KitChoice,
    wear: BandEquipment,
    coverage: KitCoverage,
}

impl Outfit {
    /// The kit's own id, for the header.
    fn id(&self) -> &str {
        self.kit.id()
    }
}

/// Assemble an outfit around `kit` for a party of `workers`, against the ledger **every spawn path
/// inserts**: `start_stocked_for`, one party's worth of every item a kit carries, unworn.
fn outfit(kit: KitChoice, workers: u32, shipped: &Shipped) -> Outfit {
    let wear = BandEquipment::start_stocked_for(&shipped.equipment, workers as f32);
    let coverage = shipped.equipment.coverage(&kit, workers as f32, &wear);
    Outfit {
        kit,
        wear,
        coverage,
    }
}

/// The **forage** job's shipped default kit, outfitted for `workers`.
fn forage_outfit(workers: u32, shipped: &Shipped) -> Outfit {
    outfit(
        shipped.equipment.default_kit(KitJob::Forage),
        workers,
        shipped,
    )
}

/// **The kit the game gives a band sent after THIS quarry** — `herd_default_hunt_kit`, the same seam
/// the wire's `defaultKitId` and `assign_labor … hunt` resolve, so a trapping quarry is priced with
/// traps and a mammoth with spears.
fn hunt_outfit(def: &SpeciesDef, corralled: bool, workers: u32, shipped: &Shipped) -> Outfit {
    outfit(
        herd_default_hunt_kit(
            &shipped.equipment,
            shipped.creatures.person(),
            def,
            corralled,
        ),
        workers,
        shipped,
    )
}

/// **One gatherer's biomass throughput** at this outfit — through the same coverage-weighted rate the
/// server's seed uses, so a partly-armed crew is priced partly-armed.
fn forage_carry(outfit: &Outfit, shipped: &Shipped) -> f32 {
    outfit.coverage.weighted_rate(|kit| {
        shipped.equipment.forage_per_worker_biomass_capacity(
            shipped.labor.forage.per_worker_biomass_capacity,
            kit,
            &outfit.wear,
        )
    })
}

/// The hunt twin of [`forage_carry`].
fn hunt_carry(outfit: &Outfit, shipped: &Shipped) -> f32 {
    outfit.coverage.weighted_rate(|kit| {
        shipped.equipment.hunt_per_worker_biomass_capacity(
            shipped.labor.hunt.per_worker_biomass_capacity,
            kit,
            &outfit.wear,
        )
    })
}

/// **The party as it fights THIS quarry** — `PartyResolution::party_against(Quarry::Mass(..))`, the
/// server's own composition, so a mass-bounded weapon is scored on quarry it can actually hold.
fn hunt_party(outfit: &Outfit, body_mass: f32, shipped: &Shipped) -> HuntingParty {
    PartyResolution {
        equipment: &shipped.equipment,
        coverage: &outfit.coverage,
        wear: &outfit.wear,
        intrinsic: shipped.creatures.person(),
        tuning: shipped.combat.tuning(),
        hunt_injury_damage_per_animal: shipped.combat.hunt_injury_damage_per_animal,
    }
    .party_against(Quarry::Mass(body_mass))
}

/// **The wholly unequipped party** — no kit, no weapon, the `person` row's intrinsic `attack 1`.
/// Printed once in the header as the size of the gear cliff, not per row: against every roster
/// `defense` it brings down nothing at all, so a column of it would be a column of zeros and would
/// say nothing about *carry*, which is the axis the control exists to isolate.
fn bare_party() -> HuntingParty {
    HuntingParty::builtin_unequipped()
}

// ---------------------------------------------------------------------------------------------
// The row
// ---------------------------------------------------------------------------------------------

/// **One row of either table**, in the units the headers state.
struct Row {
    web: &'static str,
    source: String,
    rung: &'static str,
    /// Food (provisions) per worker per turn at each of [`CREWS`], **kitted**.
    food_per_worker: [f32; CREWS.len()],
    /// **The carry control**: the same row, same crew, same kit, same party — priced at
    /// `labor_config`'s **pre-gear** `per_worker_biomass_capacity` instead of the kit's. One variable
    /// moves, so the gap against the kitted column is *exactly* what the basket or the sled bought,
    /// and a row that does not move is a row carry was never binding on.
    food_per_worker_bare: f32,
    /// Biomass actually brought home per worker ÷ that worker's carry capacity, at
    /// [`REFERENCE_CREW`]. `1.0` means the basket/sled is the binding constraint.
    carry_utilisation: f32,
    /// Which stage the **live take path** hit most turns of the drive, at each of [`CREWS`].
    /// `None` on the plant web, which has no such stage.
    bound: [Option<HuntTakeBound>; CREWS.len()],
    /// The whole live-take drive at [`REFERENCE_CREW`], for the distribution appendix — its bound
    /// tally **and** its carried/wasted pair, which is what says whether a source's kill is being
    /// thrown away for want of somewhere to put it.
    drive: Option<Drive>,
    /// The preview's `sustainable`. **On a rung-3 MANAGED source (a Field, a pen) this is not an
    /// escapement MSY line** — the preview reports the source's own production there.
    sustainable: f32,
    managed: bool,
    /// Materials per worker per turn at [`REFERENCE_CREW`], `None` where the food axis cannot be
    /// inverted to a biomass (an inedible quarry).
    materials: Option<Vec<MaterialPayoff>>,
    build_work: f32,
    upkeep_work: f32,
    /// The kit id each food column was priced at.
    kit: String,
}

impl Row {
    fn reference_food(&self) -> f32 {
        self.food_per_worker[REFERENCE_SLOT]
    }
}

/// Render a binding stage, or the plant web's honest absence of one.
fn render_bound(bound: Option<HuntTakeBound>) -> &'static str {
    match bound {
        Some(bound) => bound.as_str(),
        None => "n/a",
    }
}

// ---------------------------------------------------------------------------------------------
// SECTION A — the plant web
// ---------------------------------------------------------------------------------------------

/// **A patch seated on `rung`, the way the sim would seat it.** The position moves through
/// `complete_cultivation`/`complete_field` (the shipped fixture mutators, which take the ladder and
/// write the standing with the position), the capacity is re-struck through the sim's own
/// [`patch_carrying_capacity`] exactly as `advance_forage_regrowth` re-strikes it every turn, and the
/// stock is then put on the operating point the shipped floor settles a stand at.
fn seated_patch(terrain: TerrainType, rung: Option<RungKey>, shipped: &Shipped) -> ForagePatch {
    let tile_capacity = basket::capacity_of(&shipped.labor, terrain);
    let mut patch = ForagePatch::new(basket::TILE, tile_capacity);
    if let Some(rung) = rung {
        // The commitment is what reweights the basket and what the conversion gain lands on; the
        // real Cultivate/Sow arm stamps it on the first turn a crew works the patch.
        patch.species = Some(basket::CROP.to_string());
        let seated = match rung {
            RungKey::PlantField => patch.complete_field(TABLE_FACTION, &shipped.ladder),
            _ => patch.complete_cultivation(TABLE_FACTION, &shipped.ladder),
        };
        assert!(
            seated,
            "the reference basket must be able to climb {rung:?}"
        );
    }
    // The one write `advance_forage_regrowth` makes: the tile's `K` times the rung's capacity gain.
    patch.carrying_capacity = patch_carrying_capacity(tile_capacity, &patch, &shipped.labor.forage);
    // **The operating point, not a full stand** — `floor · K` is where the shipped floor leaves a
    // source, so every row is quoting a steady rate rather than one row's drawdown windfall.
    patch.biomass = patch.carrying_capacity * DEFAULT_ESCAPEMENT_FLOOR;
    patch.biomass_before_regrowth = patch.biomass;
    patch
}

/// The plants in `composition` that carry a food axis at all — the selection a player makes when they
/// drop the cash crops to raise their food rate.
fn food_only_selection(composition: &[core_sim::FloraShare], shipped: &Shipped) -> TakeSelection {
    TakeSelection::from_keys(composition.iter().filter_map(|share| {
        shipped
            .flora
            .species
            .get(&share.species)
            .filter(|def| def.yield_.provisions_per_biomass > 0.0)
            .map(|_| share.species.as_str())
    }))
}

/// One plant preview, at `workers` and `carry`.
fn forage_preview(
    patch: &ForagePatch,
    composition: &[core_sim::FloraShare],
    take: &TakeSelection,
    workers: u32,
    carry: f32,
    shipped: &Shipped,
) -> SourceYield {
    forage_source_yield_preview(
        patch,
        composition,
        &shipped.labor.forage,
        &shipped.flora,
        carry,
        FULL_SEASONAL_WEIGHT,
        UNIT_OUTPUT_MULTIPLIER,
        workers,
        DEFAULT_ESCAPEMENT_FLOOR,
        take,
        shipped.labor.yield_average_horizon_turns,
        shipped.labor.arrivals_horizon_turns,
        shipped.combat.forecast_range_sigmas,
    )
}

/// Food per worker per turn off a plant patch, kitted at `workers`.
fn forage_food_per_worker(
    patch: &ForagePatch,
    composition: &[core_sim::FloraShare],
    take: &TakeSelection,
    workers: u32,
    carry: f32,
    shipped: &Shipped,
) -> f32 {
    forage_preview(patch, composition, take, workers, carry, shipped).realized / workers as f32
}

/// **Cumulative work to stand on `rung`, at THIS patch's own price.** `plant:field` is priced by how
/// much of the tile the crop has to replace (`docs/plan_standing_upkeep.md` §4.15), so the Field's
/// figure is the declared `work_cost` times the multiplier this ground quotes.
fn plant_build_work(rung: Option<RungKey>, field_multiplier: f32, shipped: &Shipped) -> f32 {
    match rung {
        None => 0.0,
        Some(RungKey::PlantField) => {
            let (base, width) = plant_rung_span(RungKey::PlantField, &shipped.ladder);
            base + width * field_multiplier
        }
        Some(rung) => {
            let (base, width) = plant_rung_span(rung, &shipped.ladder);
            base + width
        }
    }
}

/// The Field price this ground quotes, live off the tended patch beneath it — which is where a
/// two-leg Sow re-quotes its Field leg from.
fn field_cost_multiplier(shipped: &Shipped) -> f32 {
    let tended = seated_patch(basket::TERRAIN, Some(RungKey::PlantTended), shipped);
    patch_field_cost_multiplier(
        &tended,
        &basket::composition(&shipped.flora),
        &shipped.flora,
        &shipped.labor.forage,
        &shipped.ladder,
    )
}

fn flora_rows(shipped: &Shipped) -> Vec<Row> {
    let multiplier = field_cost_multiplier(shipped);
    let composition = basket::composition(&shipped.flora);
    let food_only = food_only_selection(&composition, shipped);
    let tile_capacity = basket::capacity(&shipped.labor);
    let mut rows = Vec::new();
    for (label, rung) in [
        ("wild", None),
        ("tended", Some(RungKey::PlantTended)),
        ("field", Some(RungKey::PlantField)),
    ] {
        let patch = seated_patch(basket::TERRAIN, rung, shipped);
        for (selection_label, take) in [
            ("whole basket", &TakeSelection::EVERYTHING),
            ("food only", &food_only),
        ] {
            let mut food_per_worker = [0.0_f32; CREWS.len()];
            let mut kit = String::new();
            for (slot, workers) in CREWS.iter().enumerate() {
                let gear = forage_outfit(*workers, shipped);
                kit = gear.id().to_string();
                food_per_worker[slot] = forage_food_per_worker(
                    &patch,
                    &composition,
                    take,
                    *workers,
                    forage_carry(&gear, shipped),
                    shipped,
                );
            }
            let gear = forage_outfit(REFERENCE_CREW, shipped);
            let carry = forage_carry(&gear, shipped);
            let reference =
                forage_preview(&patch, &composition, take, REFERENCE_CREW, carry, shipped);
            let per_biomass = patch_provisions_per_biomass_taking(
                &patch,
                &composition,
                &shipped.flora,
                &shipped.labor.forage,
                take,
            );
            let biomass_per_worker =
                biomass_behind(food_per_worker[REFERENCE_SLOT], per_biomass).unwrap_or(0.0);
            rows.push(Row {
                web: "flora",
                source: format!("reference basket ({selection_label})"),
                rung: label,
                food_per_worker,
                food_per_worker_bare: forage_food_per_worker(
                    &patch,
                    &composition,
                    take,
                    REFERENCE_CREW,
                    shipped.labor.forage.per_worker_biomass_capacity,
                    shipped,
                ),
                carry_utilisation: utilisation(biomass_per_worker, carry),
                bound: [None; CREWS.len()],
                drive: None,
                sustainable: reference.sustainable,
                managed: patch.is_field(),
                materials: materials_off_food(
                    &patch_material_yields_taking(
                        &patch,
                        &composition,
                        &shipped.flora,
                        &shipped.labor.forage,
                        take,
                    ),
                    food_per_worker[REFERENCE_SLOT],
                    per_biomass,
                ),
                build_work: plant_build_work(rung, multiplier, shipped),
                upkeep_work: patch_upkeep_demand(
                    &patch,
                    &shipped.ladder,
                    tile_capacity,
                    &shipped.labor.forage,
                ),
                kit,
            });
        }
    }
    rows
}

// ---------------------------------------------------------------------------------------------
// SECTION B — the animal web
// ---------------------------------------------------------------------------------------------

/// **A herd seated on `rung`, the way the sim would seat it.** The position moves through the shipped
/// fixture mutators (`tame_outright`, `corral_at`), both of which self-guard on the species'
/// `husbandry_ceiling`, so a state the sim cannot reach cannot be staged here either. `K` is the
/// roster's own full-group band times the density gain the rung buys — the same [`herd_density_gain`]
/// the one `K` seam applies — and the stock is then put on the operating point the shipped floor
/// settles a herd at.
fn seated_herd(key: &str, def: &SpeciesDef, rung: RungKey, shipped: &Shipped) -> Option<Herd> {
    let range_capacity = full_group_capacity(def);
    let mut herd = Herd::new(
        format!("table_{key}"),
        def.display_name.clone(),
        // **The species' OWN size class**, not a fixture constant: it is what `graze_range_radius`
        // reads, and that radius is the herd's physical footprint — the term
        // `husbandry.hex_space_budget` is measured against. It changes nothing while the space
        // dial is unset (this harness writes `carrying_capacity` itself), which is exactly why it was
        // safe to be wrong before and is not now.
        def.size_class,
        FIXTURE_TILES.to_vec(),
        range_capacity,
        range_capacity,
        def.fodder_per_biomass,
        def.regrowth_rate
            .unwrap_or(shipped.fauna.ecology.regrowth_rate),
        def.body_mass,
    );
    herd.husbandry_ceiling = def.husbandry_ceiling;
    herd.taming_cost_multiplier = shipped.fauna.taming_cost_multiplier_for(&herd.species);
    match rung {
        RungKey::AnimalWild => {}
        RungKey::AnimalPastoral => {
            if !herd.tame_outright(TABLE_FACTION, &shipped.ladder) {
                return None;
            }
        }
        RungKey::AnimalPen => {
            if !herd.tame_outright(TABLE_FACTION, &shipped.ladder)
                || !herd.corral_at(FIXTURE_TILES[0], &shipped.ladder)
            {
                return None;
            }
        }
        _ => return None,
    }
    // The one write `advance_herds` makes: the range's `K` times the density gain the standing buys,
    // **`min`'d against what the footprint physically holds** — the same `min(feed_K, space_K)`
    // `ecological_carrying_capacity` applies, with the density gain on the feed side only.
    herd.carrying_capacity = (range_capacity
        * herd_density_gain(&herd.standing(), &herd, &shipped.fauna))
    .min(herd_space_capacity(
        fixture_footprint_tiles(&herd, def),
        herd.body_mass,
        &shipped.fauna,
    ));
    herd.biomass = herd.carrying_capacity * DEFAULT_ESCAPEMENT_FLOOR;
    herd.biomass_before_regrowth = herd.biomass;
    // A managed herd whose keeping went unmet would be measuring neglect, not the rung.
    herd.upkeep_supplied = herd_upkeep_demand(&herd, &shipped.fauna, &shipped.ladder);
    herd.refresh_ecology_phase(&shipped.fauna);
    Some(herd)
}

/// **HOW MANY TILES THIS FIXTURE HERD'S FOOTPRINT COVERS** — the fenced disk once it is penned, the
/// roam disk otherwise, counted by the sim's own `hex_range_tiles` on a map big enough that nothing
/// clips. The radii are the sim's: `Herd::pen_radius` (`0` until `ExtendPen` works a ring) and
/// `Herd::graze_range_radius` (`0` small, `1` big, `loiter_radius` migratory).
fn fixture_footprint_tiles(herd: &Herd, def: &SpeciesDef) -> usize {
    let radius = if herd.is_corralled() {
        herd.pen_radius
    } else {
        herd.graze_range_radius(Some(def))
    };
    core_sim::grid_utils::hex_range_tiles(
        FIXTURE_ANCHOR,
        radius,
        FIXTURE_MAP,
        FIXTURE_MAP,
        FIXTURE_WRAP,
    )
    .len()
}

/// **`K` for a full group of this species** — the top of the roster's own biomass band, which is what
/// `spawn_initial_herds` seeds a full group at. The live sim re-strikes `K` off the graze layer under
/// the herd's footprint; with no world here, the roster's own band is the honest stand-in and the
/// header says so.
fn full_group_capacity(def: &SpeciesDef) -> f32 {
    def.biomass[1]
}

/// One animal preview, at `workers`, `carry` and `party`.
fn hunt_preview(
    herd: &Herd,
    party: &HuntingParty,
    workers: u32,
    carry: f32,
    shipped: &Shipped,
) -> SourceYield {
    hunt_source_yield_preview(
        herd,
        &shipped.fauna,
        carry,
        party,
        UNIT_OUTPUT_MULTIPLIER,
        workers,
        DEFAULT_ESCAPEMENT_FLOOR,
        shipped.labor.yield_average_horizon_turns,
        shipped.labor.arrivals_horizon_turns,
        shipped.combat.forecast_range_sigmas,
    )
}

fn hunt_food_per_worker(
    herd: &Herd,
    party: &HuntingParty,
    workers: u32,
    carry: f32,
    shipped: &Shipped,
) -> f32 {
    hunt_preview(herd, party, workers, carry, shipped).realized / workers as f32
}

/// **WHAT THE LIVE TAKE PATH ACTUALLY DID, turn by turn.**
///
/// The preview's `realized` is the unquantised projection and never reaches `hunt_take_bound`, so
/// this drives `systems::hunt_take` forward over the same horizon on a private clone — Logistics
/// regrowth, then the Population take, the shipped order — and records what the sim reports.
/// Nothing is re-derived: the sim resolves the take and hands back its own bound.
struct Drive {
    /// `HuntOutcome::bound` tallied, in descending count order, so the head is the modal bound.
    tally: Vec<(HuntTakeBound, u32)>,
    /// Whole animals put on the ground over the whole drive — the number that says whether a
    /// sub-threshold party ever lands one at all (`docs/plan_hunt_through_combat.md` §4.2).
    kills: u32,
    /// Biomass brought home over the whole drive.
    carried: f32,
    /// Biomass KILLED and left standing — an animal the pack could not seat. On a big-bodied quarry
    /// this dwarfs `carried`, which is the whole reason it is a column.
    wasted: f32,
}

impl Drive {
    fn modal(&self) -> Option<HuntTakeBound> {
        self.tally.first().map(|(bound, _)| *bound)
    }

    fn rendered_tally(&self) -> String {
        if self.tally.is_empty() {
            return "(no turns — the herd was already spent)".to_string();
        }
        self.tally
            .iter()
            .map(|(bound, count)| format!("{} {count}", bound.as_str()))
            .collect::<Vec<_>>()
            .join("   ")
    }
}

fn drive_take(
    herd: &Herd,
    party: &HuntingParty,
    workers: u32,
    carry: f32,
    shipped: &Shipped,
) -> Drive {
    let mut quarry = herd.clone();
    let ecology = herd_ecology(&quarry, &shipped.fauna);
    let capacity = quarry.carrying_capacity;
    let mut tally: BTreeMap<&'static str, (HuntTakeBound, u32)> = BTreeMap::new();
    let mut kills = 0u32;
    let mut carried = 0.0_f32;
    let mut wasted = 0.0_f32;
    for _ in 0..shipped.labor.yield_average_horizon_turns {
        regrow_biomass(&mut quarry, &shipped.fauna);
        if quarry.biomass <= ecology.extinction_floor * capacity {
            break; // `advance_herds` would despawn it here — the herd is gone.
        }
        let outcome = hunt_take(
            &mut quarry,
            workers,
            DEFAULT_ESCAPEMENT_FLOOR,
            carry,
            party,
            &shipped.fauna,
            NO_CARRY_LIMIT,
            HuntDraw::EXPECTED,
        );
        let entry = tally
            .entry(outcome.bound.as_str())
            .or_insert((outcome.bound, 0));
        entry.1 += 1;
        kills += outcome.take.killed;
        carried += outcome.take.carried;
        wasted += outcome.take.wasted;
    }
    let mut rows: Vec<(HuntTakeBound, u32)> = tally.into_values().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.as_str().cmp(b.0.as_str())));
    Drive {
        tally: rows,
        kills,
        carried,
        wasted,
    }
}

/// The rungs `ceiling` allows, in climb order.
fn rungs_for(ceiling: HusbandryCeiling) -> Vec<(&'static str, RungKey)> {
    let mut rungs = vec![("wild hunt", RungKey::AnimalWild)];
    if ceiling != HusbandryCeiling::Wild {
        rungs.push(("pastoral", RungKey::AnimalPastoral));
    }
    if ceiling == HusbandryCeiling::Pen {
        rungs.push(("pen", RungKey::AnimalPen));
    }
    rungs
}

/// **The regrowth facts behind a fauna row's food column**, and whether the managed rate is being
/// clipped by `husbandry.husbandry_regrowth_cap`.
struct RegrowthFacts {
    wild: f32,
    effective: f32,
    /// What the rung's gain *would* have paid before the cap took it.
    uncapped: f32,
}

impl RegrowthFacts {
    fn clipped(&self) -> bool {
        self.uncapped > self.effective + f32::EPSILON
    }

    /// **The share of the rate itself the cap takes** — `(uncapped - effective) / uncapped`, the
    /// reading *"this pen breeds 29% slower than its gain says"*.
    fn clipped_rate_fraction(&self) -> f32 {
        if self.uncapped <= 0.0 {
            return 0.0;
        }
        (self.uncapped - self.effective) / self.uncapped
    }

    /// **The share of what the RUNG BUYS that the cap takes** — the same shortfall measured against
    /// the bonus over the wild rate rather than against the rate.
    fn clipped_bonus_fraction(&self) -> f32 {
        if self.uncapped <= self.wild {
            return 0.0;
        }
        (self.uncapped - self.effective) / (self.uncapped - self.wild)
    }
}

fn regrowth_facts(herd: &Herd, rung: RungKey, shipped: &Shipped) -> RegrowthFacts {
    let gain = match rung {
        RungKey::AnimalPastoral => shipped.fauna.husbandry.pastoral_gain,
        RungKey::AnimalPen => shipped.fauna.husbandry.pen_gain,
        _ => 1.0,
    };
    RegrowthFacts {
        wild: herd.regrowth_rate,
        // The one seam every consumer reads the rung's rate through.
        effective: herd_ecology(herd, &shipped.fauna).regrowth_rate,
        uncapped: herd.regrowth_rate * gain,
    }
}

/// Everything a Section B row prints beyond what [`Row`] carries.
struct FaunaExtras {
    engage_rate: f32,
    body_mass: f32,
    regrowth: RegrowthFacts,
}

fn fauna_rows(shipped: &Shipped) -> Vec<(Row, FaunaExtras)> {
    let mut rows = Vec::new();
    let mut keys: Vec<&String> = shipped.fauna.species.keys().collect();
    keys.sort();
    for key in keys {
        let def = &shipped.fauna.species[key];
        for (label, rung) in rungs_for(def.husbandry_ceiling) {
            let Some(herd) = seated_herd(key, def, rung, shipped) else {
                continue;
            };
            let corralled = herd.is_corralled();
            let mut food_per_worker = [0.0_f32; CREWS.len()];
            let mut bound = [None; CREWS.len()];
            let mut kit = String::new();
            for (slot, workers) in CREWS.iter().enumerate() {
                let gear = hunt_outfit(def, corralled, *workers, shipped);
                let carry = hunt_carry(&gear, shipped);
                let party = hunt_party(&gear, herd.body_mass, shipped);
                kit = gear.id().to_string();
                food_per_worker[slot] =
                    hunt_food_per_worker(&herd, &party, *workers, carry, shipped);
                bound[slot] = drive_take(&herd, &party, *workers, carry, shipped).modal();
            }
            let gear = hunt_outfit(def, corralled, REFERENCE_CREW, shipped);
            let carry = hunt_carry(&gear, shipped);
            let party = hunt_party(&gear, herd.body_mass, shipped);
            let party_at_reference = hunt_party(&gear, herd.body_mass, shipped);
            let reference = hunt_preview(&herd, &party, REFERENCE_CREW, carry, shipped);
            let per_biomass = shipped
                .fauna
                .hunt_yield_for(&herd.species)
                .provisions_per_biomass;
            let biomass_per_worker =
                biomass_behind(food_per_worker[REFERENCE_SLOT], per_biomass).unwrap_or(0.0);
            rows.push((
                Row {
                    web: "fauna",
                    source: def.display_name.clone(),
                    rung: label,
                    food_per_worker,
                    // **One variable** — the same party and kit as the row above it, at the
                    // pre-gear carry baseline. Swapping the party too would confound the sled with
                    // the spear and answer neither question.
                    food_per_worker_bare: hunt_food_per_worker(
                        &herd,
                        &party_at_reference,
                        REFERENCE_CREW,
                        shipped.labor.hunt.per_worker_biomass_capacity,
                        shipped,
                    ),
                    carry_utilisation: utilisation(biomass_per_worker, carry),
                    bound,
                    drive: Some(drive_take(&herd, &party, REFERENCE_CREW, carry, shipped)),
                    sustainable: reference.sustainable,
                    managed: corralled,
                    materials: materials_off_food(
                        shipped.fauna.hunt_materials_for(&herd.species),
                        food_per_worker[REFERENCE_SLOT],
                        per_biomass,
                    ),
                    build_work: animal_build_work(&herd, rung, shipped),
                    upkeep_work: herd_upkeep_demand(&herd, &shipped.fauna, &shipped.ladder),
                    kit,
                },
                FaunaExtras {
                    engage_rate: herd_engage_rate(&herd, &shipped.fauna),
                    body_mass: herd.body_mass,
                    regrowth: regrowth_facts(&herd, rung, shipped),
                },
            ));
        }
    }
    rows
}

/// Cumulative work to stand on `rung`, at **this herd's** own price list (`taming_cost_multiplier`
/// scales the pastoral leg per species).
fn animal_build_work(herd: &Herd, rung: RungKey, shipped: &Shipped) -> f32 {
    match rung {
        RungKey::AnimalPastoral => herd.rung_cost(RungKey::AnimalPastoral, &shipped.ladder),
        RungKey::AnimalPen => {
            herd.rung_cost(RungKey::AnimalPastoral, &shipped.ladder)
                + herd.rung_cost(RungKey::AnimalPen, &shipped.ladder)
        }
        _ => 0.0,
    }
}

// ---------------------------------------------------------------------------------------------
// The derived columns
// ---------------------------------------------------------------------------------------------

/// **The biomass a food figure was paid from** — `food ÷ provisions_per_biomass`, the inverse of the
/// source's own published conversion rate. `None` where that rate is zero: an inedible quarry's food
/// column is honestly `0` and inverting it would be `0/0`.
fn biomass_behind(food: f32, provisions_per_biomass: f32) -> Option<f32> {
    (provisions_per_biomass > 0.0).then(|| food / provisions_per_biomass)
}

/// **How much of a worker's carry the take actually filled** — the cheapest test of whether the
/// basket/sled is what binds. `0.0` where there is no carry to fill.
fn utilisation(biomass_per_worker: f32, carry: f32) -> f32 {
    if carry <= 0.0 {
        return 0.0;
    }
    biomass_per_worker / carry
}

/// **What the take the food column was paid from is MADE OF** — the source's own material rows
/// through the sim's one material-quote expression, off the biomass that food implies.
fn materials_off_food(
    rows: &[core_sim::MaterialYieldDef],
    food: f32,
    provisions_per_biomass: f32,
) -> Option<Vec<MaterialPayoff>> {
    let biomass = biomass_behind(food, provisions_per_biomass)?;
    Some(material_yield_totals(rows, biomass, UNIT_OUTPUT_MULTIPLIER))
}

fn render_materials(materials: &Option<Vec<MaterialPayoff>>) -> String {
    match materials {
        None => "n/a (no food axis to invert)".to_string(),
        Some(rows) if rows.is_empty() => "—".to_string(),
        Some(rows) => rows
            .iter()
            .map(|row| format!("{} {:.4}", row.material, row.amount))
            .collect::<Vec<_>>()
            .join("  "),
    }
}

// ---------------------------------------------------------------------------------------------
// The printing
// ---------------------------------------------------------------------------------------------

const RULE: &str = "------------------------------------------------------------------------------------------------------------------------------------------------------------------";

fn print_inputs(shipped: &Shipped) {
    println!("\n{}", "=".repeat(RULE.len()));
    println!("FOOD ECONOMY TABLE — every source in both webs, priced through the game's own two preview seams, at the SHIPPED DEFAULT KITS");
    println!("{}", "=".repeat(RULE.len()));
    println!("\nSHARED INPUTS — every row below is priced with these");
    println!(
        "  output_multiplier            {UNIT_OUTPUT_MULTIPLIER:.3}   (neutral band productivity)"
    );
    println!("  seasonal weight              {FULL_SEASONAL_WEIGHT:.3}   (a full growing season)");
    println!(
        "  harvest floor                {DEFAULT_ESCAPEMENT_FLOOR:.3}   (DEFAULT_ESCAPEMENT_FLOOR = MSY_BIOMASS_FRACTION, the shipped default)"
    );
    println!(
        "  forecast_range_sigmas        {:.3}   (combat_config.forecast_range_sigmas)",
        shipped.combat.forecast_range_sigmas
    );
    println!(
        "  realized horizon             {} turns (labor_config.yield_average_horizon_turns) — the 'steady headline' every food column reports,",
        shipped.labor.yield_average_horizon_turns
    );
    println!("                               and the length of the live-take drive the binding-stage column is read off");
    println!(
        "  crews priced                 {CREWS:?} workers   ({REFERENCE_CREW} = the reference crew; materials, carry% and Section C quote it)"
    );
    println!("  operating point              biomass = floor x K on BOTH webs — where the shipped floor settles a source, so no row quotes a drawdown windfall");

    println!("\n  ⛔ THE GEAR — resolved through the SAME seam bin/server.rs's assign-time seed uses, not hand-picked");
    let forage_gear = forage_outfit(REFERENCE_CREW, shipped);
    println!(
        "  forage default kit           \"{}\", carry {:.3} biomass/gatherer/turn (bare baseline {:.3})",
        forage_gear.id(),
        forage_carry(&forage_gear, shipped),
        shipped.labor.forage.per_worker_biomass_capacity,
    );
    println!(
        "  hunt default kit             per QUARRY, via herd_default_hunt_kit (the seam the wire publishes as defaultKitId); the job default is \"{}\"",
        shipped.equipment.default_kit_id(KitJob::Hunt)
    );
    println!("                               Every fauna row's `kit` column names the one it was actually priced at.");
    println!("  ledger                       BandEquipment::start_stocked_for(equipment, workers) — what every spawn path inserts, unworn");
    println!("  party                        PartyResolution::party_against(Quarry::Mass(body_mass)) — so a mass-bounded weapon is scored on quarry it can hold");
    println!(
        "\n  BARE-CARRY control           every row re-priced at labor_config's PRE-GEAR carry only — forage {:.3}, hunt {:.3} — with the same",
        shipped.labor.forage.per_worker_biomass_capacity,
        shipped.labor.hunt.per_worker_biomass_capacity,
    );
    println!("                               kit and the same party otherwise. ONE variable moves, so the gap is exactly what the basket/sled bought,");
    println!("                               and a row that does not move is a row carry was never binding on. It is a CONTROL, not a play state.");
    let cliff = bare_party();
    println!(
        "  the gear cliff, for scale     a WHOLLY unequipped party is attack {:.2} (the `person` row's intrinsic) against roster defenses of 1-12:",
        cliff.crews.first().map_or(0.0, |crew| crew.hunter.attack),
    );
    println!("                               it brings down nothing at all, at any crew size. That is why the control moves carry and not the weapon.");

    println!(
        "\n  flora reference tile         {:?} {:?} under seed {:#018X}, tile K = {:.1}",
        basket::TERRAIN,
        (basket::TILE.x, basket::TILE.y),
        basket::SEED,
        basket::capacity(&shipped.labor),
    );
    let composition = basket::composition(&shipped.flora);
    let shares: Vec<String> = composition
        .iter()
        .map(|share| format!("{} {:.3}", share.species, share.share))
        .collect();
    println!("    realized basket            {}", shares.join("  "));
    println!("    crop committed at rungs 2+ {}", basket::CROP);
    println!(
        "    'food only' selection      {}",
        food_only_selection(&composition, shipped)
            .keys()
            .collect::<Vec<_>>()
            .join("  ")
    );
    println!(
        "\n  fauna herd fixture           K = the roster's own full-group biomass band x herd_density_gain(rung); the live sim re-strikes K"
    );
    println!("                               off the graze layer under the herd's footprint, which needs a world — the roster band is the stand-in.");
    println!(
        "  husbandry_regrowth_cap       {:.3}   (pastoral_gain {:.2}, pen_gain {:.2})",
        shipped.fauna.husbandry.husbandry_regrowth_cap,
        shipped.fauna.husbandry.pastoral_gain,
        shipped.fauna.husbandry.pen_gain,
    );
}

fn print_section_a(rows: &[Row], shipped: &Shipped) {
    let multiplier = field_cost_multiplier(shipped);
    println!("\n{RULE}");
    println!("SECTION A — FLORA, the reference basket. Food is provisions per WORKER per turn (the preview's steady 'realized', divided by the crew).");
    println!("{RULE}");
    println!(
        "  {:<8} {:<14} {:>9} {:>9} {:>9} {:>9} {:>7} {:<10} {:>12} {:>12} {:>10}  materials/worker/turn ({REFERENCE_CREW} wkr)",
        "rung",
        "selection",
        "food/w/t",
        "food/w/t",
        "food/w/t",
        "food/w/t",
        "carry",
        "binds",
        "sustainable",
        "build work",
        "upkeep",
    );
    println!(
        "  {:<8} {:<14} {:>9} {:>9} {:>9} {:>9} {:>7} {:<10} {:>12} {:>12} {:>10}",
        "",
        "",
        format!("{} wkr", CREWS[0]),
        format!("{} wkr", CREWS[1]),
        format!("{} wkr", CREWS[2]),
        "BARE-carry",
        "used",
        "",
        "food/turn",
        "cumulative",
        "work/turn",
    );
    for row in rows {
        let selection = row
            .source
            .trim_start_matches("reference basket (")
            .trim_end_matches(')');
        println!(
            "  {:<8} {:<14} {:>9.4} {:>9.4} {:>9.4} {:>9.4} {:>6.0}% {:<10} {:>11.3}{} {:>12.1} {:>10.2}  {}",
            row.rung,
            selection,
            row.food_per_worker[0],
            row.food_per_worker[1],
            row.food_per_worker[2],
            row.food_per_worker_bare,
            row.carry_utilisation * 100.0,
            render_bound(row.bound[REFERENCE_SLOT]),
            row.sustainable,
            if row.managed { "*" } else { " " },
            row.build_work,
            row.upkeep_work,
            render_materials(&row.materials),
        );
    }
    println!(
        "\n  'binds' is n/a on the plant web ON PURPOSE — HuntTakeBound names the animal take's stages (engagement, retreat, fight), and a"
    );
    println!("  gather has none of them. 'carry used' answers for it: at 100% the basket is what binds, which is why every plant row above is");
    println!("  flat across 1/3/5 workers.");
    println!(
        "\n  * MANAGED SOURCE: a rung-3 source (a Field, a pen) has no wild stock to stop short of, so the preview reports its own"
    );
    println!("    production in `sustainable` rather than an escapement MSY line — and that production is THIS TURN's take at the");
    println!("    operating point, not the horizon average the food columns carry. Compare it against food/turn, not against the rungs below.");

    let (base, width) = plant_rung_span(RungKey::PlantField, &shipped.ladder);
    println!(
        "\n  ⛔ THE FIELD'S BUILD WORK IS SCALED: plant:field declares {width:.1} work units, and this ground quotes it at x{multiplier:.4}"
    );
    println!(
        "     = {:.1} for the Field leg, on top of the {base:.1} the tended leg beneath it costs (docs/plan_standing_upkeep.md §4.15).",
        width * multiplier
    );
    println!(
        "     It lands on x1.0000 because this crop's WEEDED share here is exactly cultivation.field_reference_crop_share ({:.4}) —",
        shipped.labor.forage.cultivation.field_reference_crop_share
    );
    println!("     i.e. the reference basket is literally §4.15's reference ground. Every crop this basket could be sown to, priced:");
    let composition = basket::composition(&shipped.flora);
    for share in &composition {
        let quoted = crop_field_cost_multiplier(
            &composition,
            &share.species,
            &shipped.flora,
            &shipped.labor.forage,
        );
        println!(
            "       {:<14} wild share {:.3}   sow price {}",
            share.species,
            share.share,
            match quoted {
                Some(value) => format!("x{value:.4}  = {:.1} work", width * value),
                None => "cannot climb to a Field here (no row, never a zero)".to_string(),
            },
        );
    }
}

fn print_section_b(rows: &[(Row, FaunaExtras)]) {
    println!("\n{RULE}");
    println!("SECTION B — FAUNA, every roster species at every rung its husbandry_ceiling allows. Food is provisions per WORKER per turn.");
    println!("{RULE}");
    println!(
        "  {:<16} {:<10} {:<9} {:>8} {:>8} {:>8} {:>8} {:>6} {:<11} {:<11} {:<11} {:>7} {:>6} {:>6} {:>6} {:>4} {:>9} {:>8}",
        "species",
        "rung",
        "kit",
        "food/w/t",
        "food/w/t",
        "food/w/t",
        "food/w/t",
        "carry",
        "binds@1",
        "binds@3",
        "binds@5",
        "engage",
        "body",
        "r wild",
        "r rung",
        "clip",
        "build",
        "upkeep",
    );
    println!(
        "  {:<16} {:<10} {:<9} {:>8} {:>8} {:>8} {:>8} {:>6} {:<11} {:<11} {:<11} {:>7} {:>6} {:>6} {:>6} {:>4} {:>9} {:>8}",
        "",
        "",
        "",
        format!("{} wkr", CREWS[0]),
        format!("{} wkr", CREWS[1]),
        format!("{} wkr", CREWS[2]),
        "BARE-carry",
        "used",
        "",
        "",
        "",
        "rate",
        "mass",
        "",
        "",
        "",
        "work",
        "work/turn",
    );
    let mut last = "";
    for (row, extras) in rows {
        if row.source != last {
            if !last.is_empty() {
                println!();
            }
            last = row.source.as_str();
        }
        println!(
            "  {:<16} {:<10} {:<9} {:>8.4} {:>8.4} {:>8.4} {:>8.4} {:>5.0}% {:<11} {:<11} {:<11} {:>7.2} {:>6.2} {:>6.3} {:>6.3} {:>4} {:>9.1} {:>8.2}",
            row.source,
            row.rung,
            row.kit,
            row.food_per_worker[0],
            row.food_per_worker[1],
            row.food_per_worker[2],
            row.food_per_worker_bare,
            row.carry_utilisation * 100.0,
            render_bound(row.bound[0]),
            render_bound(row.bound[1]),
            render_bound(row.bound[2]),
            extras.engage_rate,
            extras.body_mass,
            extras.regrowth.wild,
            extras.regrowth.effective,
            if extras.regrowth.clipped() { "CLIP" } else { "" },
            row.build_work,
            row.upkeep_work,
        );
    }
    println!(
        "\n  BINDING-STAGE DISTRIBUTION over the live-take drive at {REFERENCE_CREW} hunters — `systems::hunt_take` run turn by turn on a clone,"
    );
    println!(
        "  reading back HuntOutcome::bound. 'floor' on most turns of a slow breeder is the honest answer: the herd has nothing to spare yet."
    );
    println!(
        "  The killed/carried/wasted triple beside it is the same drive's, in biomass: WASTED is meat the take put on the ground and could"
    );
    println!("  not bring home, which is what a carry bound produces.");
    for (row, _) in rows {
        let Some(drive) = row.drive.as_ref() else {
            continue;
        };
        println!(
            "    {:<16} {:<10} {:<44}  killed {:>7.0}  carried {:>7.0}  wasted {:>7.0}",
            row.source,
            row.rung,
            drive.rendered_tally(),
            drive.carried + drive.wasted,
            drive.carried,
            drive.wasted,
        );
    }
    println!(
        "\n  * A quarry with no food axis (Grey Wolf Pack) prints n/a for materials in Section C rather than 0: its food column is honestly 0,"
    );
    println!("  and the biomass its materials ride on cannot be recovered by dividing 0 food by a 0 rate. Its hides and bone are real.");
    println!("\n  CLIP = husbandry_regrowth_cap is taking part of this rung's regrowth bonus:");
    for (row, extras) in rows {
        if extras.regrowth.clipped() {
            println!(
                "    {:<16} {:<10} wild r {:.3} x gain = {:.3}, capped to {:.3} — {:.0}% of the uncapped rate lost ({:.0}% of the bonus over wild)",
                row.source,
                row.rung,
                extras.regrowth.wild,
                extras.regrowth.uncapped,
                extras.regrowth.effective,
                extras.regrowth.clipped_rate_fraction() * 100.0,
                extras.regrowth.clipped_bonus_fraction() * 100.0,
            );
        }
    }
}

fn print_section_c(rows: &[&Row]) {
    println!("\n{RULE}");
    println!("SECTION C — THE COMPARISON. Every source/rung from both webs, at the {REFERENCE_CREW}-worker reference crew, sorted by food/worker/turn.");
    println!("{RULE}");
    println!(
        "  {:>3} {:<5} {:<38} {:<11} {:>9} {:>10} {:>9} {:>6} {:<11} {:>9} {:>9}  materials/worker/turn",
        "#",
        "web",
        "source",
        "rung",
        "food/w/t",
        "food/turn",
        "food/w/t",
        "carry",
        "binds",
        "upkeep",
        "food/w/t",
    );
    println!(
        "  {:>3} {:<5} {:<38} {:<11} {:>9} {:>10} {:>9} {:>6} {:<11} {:>9} {:>9}",
        "", "", "", "", "kitted", "kitted", "BARE", "used", "", "work/turn", "per work",
    );
    for (index, row) in rows.iter().enumerate() {
        let food = row.reference_food();
        let per_work = if row.upkeep_work > NO_UPKEEP {
            format!("{:.3}", food / row.upkeep_work)
        } else {
            "free".to_string()
        };
        println!(
            "  {:>3} {:<5} {:<38} {:<11} {:>9.4} {:>10.4} {:>9.4} {:>5.0}% {:<11} {:>9.2} {:>9}  {}",
            index + 1,
            row.web,
            row.source,
            row.rung,
            food,
            food * REFERENCE_CREW as f32,
            row.food_per_worker_bare,
            row.carry_utilisation * 100.0,
            render_bound(row.bound[REFERENCE_SLOT]),
            row.upkeep_work,
            per_work,
            render_materials(&row.materials),
        );
    }
    println!("\n  'food/w/t per work' = food per worker per turn ÷ upkeep work per turn — what a unit of standing effort buys.");
    println!("  'free' = the rung declares no standing upkeep at all (both webs' wild rung), so there is no denominator to divide by.");
    println!(
        "  'BARE' is the same row, same kit, same party, at the PRE-GEAR carry rate — the control. The gap between the two columns is"
    );
    println!("  exactly what the basket or the sled bought, and it is NOT the same size on the two webs: a basket multiplies a carry-bound");
    println!("  gather outright, while a sled adds nothing to a hunt that reach or the floor had already stopped.");
}

// ---------------------------------------------------------------------------------------------
// THE RUNG-PAIR COMPARISON — the headline
// ---------------------------------------------------------------------------------------------

/// **THE THREE PAIRS THE LADDERS ARE SUPPOSED TO KEEP LEVEL.** Each plant rung has an animal rung
/// opposite it — same position on the same climb, same knowledge gate, same order of build work —
/// so the honest question is not *"is hunting worse than gathering"* but *"by how much, at each
/// step of the ladder"*.
///
/// The plant partner is the **whole-basket** row, which is what a band that has named no selection
/// gathers; the food-only row is printed beside it because dropping the cash crops is a real move a
/// player makes, and it moves the ratio.
const RUNG_PAIRS: [(&str, &str, RungKey); 3] = [
    ("wild hunt", "wild", RungKey::AnimalWild),
    ("pastoral", "tended", RungKey::AnimalPastoral),
    ("pen", "field", RungKey::AnimalPen),
];

/// The median of a set of rates. Even counts take the lower of the two middles — the pessimistic
/// half, which is the one a ladder comparison should not be able to hide behind.
fn median(mut values: Vec<f32>) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f32::total_cmp);
    values[(values.len() - 1) / 2]
}

/// **THE CREW SIZES SECTION F SWEEPS**, both webs, to find where each source stops rising. It has to
/// reach past the plant curve's own range because a Field keeps climbing to 8 gatherers, and past the
/// pens' because the answer *"3"* is only meaningful if larger crews were asked and paid no more.
const MAX_SWEEP_CREWS: [u32; 9] = [1, 2, 3, 4, 5, 6, 8, 10, 13];

/// **WHERE A SOURCE STOPS PAYING FOR ANOTHER WORKER** — the first crew size in [`MAX_SWEEP_CREWS`]
/// that reaches (within a rounding of) the best figure any crew in the sweep reaches.
///
/// It is the **first** such crew and not the largest, because the question is *how many people does
/// this source want*: a pen that pays 8.75 at three keepers and 8.75 at thirteen wants three.
fn crew_for_max(by_crew: &[(u32, f32)]) -> (u32, f32) {
    let best = by_crew
        .iter()
        .map(|(_, food)| *food)
        .fold(0.0_f32, f32::max);
    let (crew, food) = by_crew
        .iter()
        .find(|(_, food)| *food >= best - MAX_SWEEP_EPSILON)
        .copied()
        .unwrap_or((0, 0.0));
    (crew, food)
}

/// How close to the sweep's best still counts as *at* it — a food/turn figure is a product of two
/// f32 rates, and a quantised take wobbles a fraction of a body between crew sizes.
const MAX_SWEEP_EPSILON: f32 = 1e-3;

/// **ONE FIXED CREW, AND EACH SOURCE'S OWN BEST** — the two readings side by side, because a table
/// that mixes crew sizes without saying so compares nothing.
fn print_standardised(shipped: &Shipped) {
    println!("\n{RULE}");
    println!("SECTION F — EVERY SOURCE, STANDARDISED. Left: all at a fixed {STANDARD_CREW} workers. Right: each at its own maximum.");
    println!("{RULE}");

    let composition = basket::composition(&shipped.flora);
    let food_only = food_only_selection(&composition, shipped);
    let mut rows: Vec<(String, String, f32, u32, f32)> = Vec::new();

    // ---- the plant web ----
    for (label, rung) in [
        ("wild", None),
        ("tended", Some(RungKey::PlantTended)),
        ("field", Some(RungKey::PlantField)),
    ] {
        let patch = seated_patch(basket::TERRAIN, rung, shipped);
        for (selection, take) in [
            ("whole basket", &TakeSelection::EVERYTHING),
            ("food only", &food_only),
        ] {
            let at = |crew: u32| {
                let gear = forage_outfit(crew, shipped);
                forage_preview(
                    &patch,
                    &composition,
                    take,
                    crew,
                    forage_carry(&gear, shipped),
                    shipped,
                )
                .realized
            };
            let sweep: Vec<(u32, f32)> = MAX_SWEEP_CREWS.iter().map(|c| (*c, at(*c))).collect();
            let (crew, best) = crew_for_max(&sweep);
            rows.push((
                format!("{label} ({selection})"),
                "plant".to_string(),
                at(STANDARD_CREW),
                crew,
                best,
            ));
        }
    }

    // ---- the animal web ----
    let mut keys: Vec<&String> = shipped.fauna.species.keys().collect();
    keys.sort();
    for key in keys {
        let def = &shipped.fauna.species[key];
        for (label, rung) in rungs_for(def.husbandry_ceiling) {
            let Some(herd) = seated_herd(key, def, rung, shipped) else {
                continue;
            };
            let corralled = herd.is_corralled();
            let at = |crew: u32| {
                let gear = hunt_outfit(def, corralled, crew, shipped);
                hunt_preview(
                    &herd,
                    &hunt_party(&gear, herd.body_mass, shipped),
                    crew,
                    hunt_carry(&gear, shipped),
                    shipped,
                )
                .realized
            };
            let sweep: Vec<(u32, f32)> = MAX_SWEEP_CREWS.iter().map(|c| (*c, at(*c))).collect();
            let (crew, best) = crew_for_max(&sweep);
            rows.push((
                def.display_name.clone(),
                label.to_string(),
                at(STANDARD_CREW),
                crew,
                best,
            ));
        }
    }

    // **Sorted by food per WORKER on both tables.** A source's total says what a place is worth; its
    // per-worker rate says what a person is worth standing there, and that is the axis a band with a
    // fixed head count actually chooses on.
    println!("\n  AT A FIXED {STANDARD_CREW} WORKERS — sorted by food per worker per turn");
    println!(
        "  {:>3} {:<38} {:<11} {:>12} {:>14}",
        "#", "source", "rung", "food/turn", "food/worker"
    );
    let mut standard = rows.clone();
    standard.sort_by(|a, b| (b.2 / STANDARD_CREW as f32).total_cmp(&(a.2 / STANDARD_CREW as f32)));
    for (index, (source, rung, food, _, _)) in standard.iter().enumerate() {
        println!(
            "  {:>3} {:<38} {:<11} {:>12.4} {:>14.4}",
            index + 1,
            source,
            rung,
            food,
            food / STANDARD_CREW as f32
        );
    }

    println!("\n  AT EACH SOURCE'S OWN MAXIMUM — sorted by food per worker per turn");
    println!(
        "  {:>3} {:<38} {:<11} {:>12} {:>14} {:>8}",
        "#", "source", "rung", "food/turn", "food/worker", "workers"
    );
    let mut best = rows;
    best.sort_by(|a, b| {
        let rate = |r: &(String, String, f32, u32, f32)| {
            if r.3 > 0 {
                r.4 / r.3 as f32
            } else {
                0.0
            }
        };
        rate(b).total_cmp(&rate(a))
    });
    for (index, (source, rung, _, crew, food)) in best.iter().enumerate() {
        println!(
            "  {:>3} {:<38} {:<11} {:>12.4} {:>14.4} {:>8}",
            index + 1,
            source,
            rung,
            food,
            if *crew > 0 { food / *crew as f32 } else { 0.0 },
            crew
        );
    }
    println!(
        "\n  'workers' is the FIRST crew in {MAX_SWEEP_CREWS:?} that reaches the best figure any crew in that sweep reaches —"
    );
    println!("  i.e. how many people the source actually wants, not the largest crew that fits.");
}

/// **The crew the standardised table fixes every source at.** Five: that is where the pens are tuned
/// to reach their own line and where a Field is still climbing, so both halves are live there.
const STANDARD_CREW: u32 = 5;

/// **THE CREW COUNTS THE PLANT CURVE IS WALKED AT** — far past the table's own range, because the
/// question is *where does a stand stop being carry-bound and start drawing itself down*, and on the
/// shipped Field that happens well beyond three gatherers.
const PLANT_CREW_CURVE: [u32; 7] = [1, 3, 5, 8, 10, 13, 20];

/// **WHAT STOPS A PLANT TAKE** — the plant web's answer to `HuntTakeBound`, which it has no version
/// of because it has no engagement, retreat or fight. There are only two things it can be.
fn plant_bound(biomass_taken: f32, crew_carry: f32) -> &'static str {
    // The take filled the baskets: another gatherer would carry another basketful.
    if biomass_taken >= crew_carry - PLANT_BOUND_EPSILON {
        "carry"
    } else {
        // The stand ran out first — the escapement ceiling above the floor is the whole offer, and
        // more hands take the same amount.
        "stand"
    }
}

/// Float slack for *"the take filled the baskets"* — a ratio of two f32 products.
const PLANT_BOUND_EPSILON: f32 = 1e-3;

/// **DOES A FIELD RUN OUT?** — the plant rungs walked up the crew curve, against each stand's own
/// sustainable line, so the plant web's "flattens like a herd / does not" can be read directly.
fn print_plant_crew_curve(shipped: &Shipped) {
    println!("\n{RULE}");
    println!("SECTION D — THE PLANT CREW CURVE. Where each plant rung stops being carry-bound and starts drawing itself down.");
    println!("{RULE}");
    let composition = basket::composition(&shipped.flora);
    let food_only = food_only_selection(&composition, shipped);
    for (label, rung) in [
        ("wild", None),
        ("tended", Some(RungKey::PlantTended)),
        ("field", Some(RungKey::PlantField)),
    ] {
        let patch = seated_patch(basket::TERRAIN, rung, shipped);
        for (selection_label, take) in [
            ("whole basket", &TakeSelection::EVERYTHING),
            ("food only", &food_only),
        ] {
            let per_biomass = patch_provisions_per_biomass_taking(
                &patch,
                &composition,
                &shipped.flora,
                &shipped.labor.forage,
                take,
            );
            // **The stand's OWN sustainable line** — the same expression `forage_source_yield_preview`
            // computes and then *discards* on a Field (`forecast_source_yield` overwrites a managed
            // source's `sustainable` with the turn's `actual`). Resolved here so rung 3 can be read on
            // the same footing as the two beneath it.
            let selected = selected_biomass_share(
                &patch_composition(&patch, &composition, &shipped.flora, &shipped.labor.forage),
                take,
            );
            let sustainable = forage_provisions(
                sustainable_yield(
                    patch.biomass * selected,
                    patch.carrying_capacity * selected,
                    &patch_ecology(&patch, &shipped.labor.forage),
                ),
                per_biomass,
                UNIT_OUTPUT_MULTIPLIER,
            );
            println!(
                "\n  {label} / {selection_label}   —   K {:.1} biomass, standing {:.1}, sustainable {sustainable:.4} food/turn",
                patch.carrying_capacity, patch.biomass,
            );
            println!(
                "  {:>7} {:>12} {:>14} {:>14} {:>12} {:<8}  vs sustainable",
                "workers", "food/turn", "food/wkr/turn", "biomass taken", "crew carry", "binds",
            );
            for crew in PLANT_CREW_CURVE {
                let gear = forage_outfit(crew, shipped);
                let carry = forage_carry(&gear, shipped);
                let food =
                    forage_preview(&patch, &composition, take, crew, carry, shipped).realized;
                let biomass = biomass_behind(food, per_biomass).unwrap_or(0.0);
                let crew_carry = crew as f32 * carry;
                println!(
                    "  {crew:>7} {food:>12.4} {:>14.4} {biomass:>14.2} {crew_carry:>12.2} {:<8}  {}",
                    food / crew as f32,
                    plant_bound(biomass, crew_carry),
                    steady_verdict(food, sustainable),
                );
            }
        }
    }
    println!("\n  'binds: carry' means the take filled the baskets and another gatherer would carry another basketful.");
    println!("  'binds: stand' means the escapement ceiling above the harvest floor was the whole offer, and more hands take the same amount.");
}

/// **AT, UNDER, OR OVER THE SUSTAINABLE LINE** — the one verdict that says whether a figure in these
/// tables is a rate the source can pay forever or a stock being spent.
fn steady_verdict(taken: f32, sustainable: f32) -> String {
    if sustainable <= 0.0 {
        return "no sustainable line (source spent or below its Allee threshold)".to_string();
    }
    let share = taken / sustainable;
    if share > 1.0 + STEADY_BAND {
        format!("OVER by {:.0}% - DRAWDOWN", (share - 1.0) * 100.0)
    } else if share < 1.0 - STEADY_BAND {
        format!(
            "under by {:.0}% - steady, with headroom",
            (1.0 - share) * 100.0
        )
    } else {
        "AT the line - steady".to_string()
    }
}

/// How far from the sustainable line still counts as *on* it. The take is quantised to whole animals
/// on one web and to nothing on the other, so an exact equality would never be reported.
const STEADY_BAND: f32 = 0.05;

/// **IS THE NUMBER WE HAVE BEEN READING A STEADY RATE OR A DRAWDOWN?** — every source at every rung,
/// its own reproduction against what the crew actually takes.
///
/// # ⛔ WHY THE PUBLISHED `sustainable` CANNOT ANSWER THIS
///
/// `forecast_source_yield` overwrites a **managed** source's `sustainable` with the turn's own
/// `actual` (a rung-3 source has no wild stock to stop short of), so at the top of *both* ladders —
/// a Field and a pen — the wire's own steady line is the take. This section resolves
/// [`sustainable_yield`] directly, which is the expression both previews compute before discarding
/// it, so rung 3 is read on the same footing as the rungs beneath it.
fn print_steady_rates(shipped: &Shipped, fauna: &[(Row, FaunaExtras)]) {
    println!("\n{RULE}");
    println!("SECTION E — THE STEADY RATE. What each source reproduces per turn, against what is actually taken.");
    println!("{RULE}");
    println!("\n  ANIMAL WEB — every species at every rung it can stand on");
    println!(
        "  {:<16} {:<10} {:>7} {:>10} {:>13} {:>13} {:>13}  verdict at 5 workers",
        "species", "rung", "r", "K biomass", "sustainable", "taken 1 wkr", "taken 5 wkr",
    );
    println!(
        "  {:<16} {:<10} {:>7} {:>10} {:>13} {:>13} {:>13}",
        "", "", "per turn", "", "food/turn", "food/turn", "food/turn",
    );
    let mut keys: Vec<&String> = shipped.fauna.species.keys().collect();
    keys.sort();
    for key in keys {
        let def = &shipped.fauna.species[key];
        for (label, rung) in rungs_for(def.husbandry_ceiling) {
            let Some(herd) = seated_herd(key, def, rung, shipped) else {
                continue;
            };
            // The same expression `hunt_source_yield_preview` computes and then discards at a pen.
            let sustainable = herd_hunt_yield(&herd, &shipped.fauna)
                .apply(
                    sustainable_yield(
                        herd.biomass,
                        herd_capacity(&herd, &shipped.fauna),
                        &herd_ecology(&herd, &shipped.fauna),
                    ),
                    UNIT_OUTPUT_MULTIPLIER,
                )
                .provisions;
            let row = fauna
                .iter()
                .find(|(row, _)| row.source == def.display_name && row.rung == label);
            let (one, five) = row.map_or((0.0, 0.0), |(row, _)| {
                (
                    row.food_per_worker[0],
                    row.food_per_worker[2] * CREWS[2] as f32,
                )
            });
            println!(
                "  {:<16} {:<10} {:>7.3} {:>10.0} {sustainable:>13.4} {one:>13.4} {five:>13.4}  {}",
                def.display_name,
                label,
                herd_ecology(&herd, &shipped.fauna).regrowth_rate,
                herd_capacity(&herd, &shipped.fauna),
                steady_verdict(five, sustainable),
            );
        }
    }
    println!("\n  PLANT WEB — the same columns, on the reference basket");
    println!(
        "  {:<8} {:<14} {:>7} {:>10} {:>13} {:>13} {:>13}  verdict at 5 workers",
        "rung", "selection", "r", "K biomass", "sustainable", "taken 1 wkr", "taken 5 wkr",
    );
    let composition = basket::composition(&shipped.flora);
    let food_only = food_only_selection(&composition, shipped);
    for (label, rung) in [
        ("wild", None),
        ("tended", Some(RungKey::PlantTended)),
        ("field", Some(RungKey::PlantField)),
    ] {
        let patch = seated_patch(basket::TERRAIN, rung, shipped);
        for (selection_label, take) in [
            ("whole basket", &TakeSelection::EVERYTHING),
            ("food only", &food_only),
        ] {
            let per_biomass = patch_provisions_per_biomass_taking(
                &patch,
                &composition,
                &shipped.flora,
                &shipped.labor.forage,
                take,
            );
            let selected = selected_biomass_share(
                &patch_composition(&patch, &composition, &shipped.flora, &shipped.labor.forage),
                take,
            );
            let ecology = patch_ecology(&patch, &shipped.labor.forage);
            let sustainable = forage_provisions(
                sustainable_yield(
                    patch.biomass * selected,
                    patch.carrying_capacity * selected,
                    &ecology,
                ),
                per_biomass,
                UNIT_OUTPUT_MULTIPLIER,
            );
            let food_at = |crew: u32| {
                let gear = forage_outfit(crew, shipped);
                forage_preview(
                    &patch,
                    &composition,
                    take,
                    crew,
                    forage_carry(&gear, shipped),
                    shipped,
                )
                .realized
            };
            let (one, five) = (food_at(CREWS[0]), food_at(CREWS[2]));
            println!(
                "  {label:<8} {selection_label:<14} {:>7.3} {:>10.0} {sustainable:>13.4} {one:>13.4} {five:>13.4}  {}",
                ecology.regrowth_rate,
                patch.carrying_capacity,
                steady_verdict(five, sustainable),
            );
        }
    }
    println!("\n  'sustainable' is the source's OWN reproduction at its current stock, in food/turn - `sustainable_yield` at the rung's own");
    println!("  ecology, converted at the rung's own rate. It is NOT the `sustainable` the wire publishes for a Field or a pen: that one is");
    println!("  overwritten with the turn's take, because a rung-3 source has no wild stock to stop short of.");
}

fn print_rung_pairs(flora: &[Row], fauna: &[(Row, FaunaExtras)]) {
    println!("\n{RULE}");
    println!("THE RUNG-PAIR COMPARISON — each plant rung against the animal rung opposite it, at the {REFERENCE_CREW}-worker reference crew.");
    println!("{RULE}");
    println!(
        "  {:<10} {:<8} {:>10} {:>10} {:>10} | {:>10} {:>10} | {:>8} {:>8}  best animal row",
        "animal rung", "plant", "plant", "plant", "animal", "animal", "rows", "gap", "gap",
    );
    println!(
        "  {:<10} {:<8} {:>10} {:>10} {:>10} | {:>10} {:>10} | {:>8} {:>8}",
        "", "rung", "basket", "food only", "BEST", "MEDIAN", "counted", "vs best", "vs med",
    );
    for (animal_rung, plant_rung, _) in RUNG_PAIRS {
        let plant_basket = flora
            .iter()
            .find(|row| row.rung == plant_rung && row.source.contains("whole basket"))
            .map_or(0.0, Row::reference_food);
        let plant_food_only = flora
            .iter()
            .find(|row| row.rung == plant_rung && row.source.contains("food only"))
            .map_or(0.0, Row::reference_food);
        let animals: Vec<f32> = fauna
            .iter()
            .filter(|(row, _)| row.rung == animal_rung)
            .map(|(row, _)| row.reference_food())
            .collect();
        let best = animals.iter().copied().fold(0.0_f32, f32::max);
        let best_source = fauna
            .iter()
            .filter(|(row, _)| row.rung == animal_rung)
            .max_by(|a, b| a.0.reference_food().total_cmp(&b.0.reference_food()))
            .map_or(String::new(), |(row, _)| row.source.clone());
        let mid = median(animals.clone());
        println!(
            "  {:<10} {:<8} {:>10.4} {:>10.4} {:>10.4} | {:>10.4} {:>10} | {:>7.2}x {:>7.2}x  {best_source}",
            animal_rung,
            plant_rung,
            plant_basket,
            plant_food_only,
            best,
            mid,
            animals.len(),
            if best > 0.0 { plant_basket / best } else { f32::INFINITY },
            if mid > 0.0 { plant_basket / mid } else { f32::INFINITY },
        );
    }
    println!("\n  'gap' = the plant WHOLE-BASKET row divided by the animal row — how many times better the plant half of that rung pays.");
    println!("  1.00x is parity. Against the food-only plant row every gap is larger still, because dropping the cash crops raises the");
    println!("  plant half and does nothing for the animal half.");
}

// ---------------------------------------------------------------------------------------------
// Validation against the running game
// ---------------------------------------------------------------------------------------------

/// **THE HARNESS AGAINST THE GAME.** Nothing here is allowed to move a fixture: a reading that
/// disagrees is printed as a disagreement, with the term most likely responsible named.
fn print_validation(shipped: &Shipped, fauna: &[(Row, FaunaExtras)]) {
    println!("\n{RULE}");
    println!("VALIDATION — the harness against readings taken off a RUNNING GAME (1 and 5 workers, default kits). Nothing below is tuned to fit.");
    println!("{RULE}");

    // The plant side is quoted on the biome the live reading was taken on, not on Section A's tile.
    let composition = basket::composition_of(&shipped.flora, LIVE_HARVEST_TERRAIN);
    let food_only = food_only_selection(&composition, shipped);
    let patch = seated_patch(LIVE_HARVEST_TERRAIN, None, shipped);
    println!(
        "\n  harvest tile fixture: {:?}, tile K = {:.1}, wild rung, realized basket:",
        LIVE_HARVEST_TERRAIN, patch.carrying_capacity
    );
    println!(
        "    {}",
        composition
            .iter()
            .map(|share| format!("{} {:.3}", share.species, share.share))
            .collect::<Vec<_>>()
            .join("  ")
    );
    println!(
        "  ⛔ A BASKET IS A PER-TILE REALIZATION OF THE MAP SEED. The live reading came off a different map, so the harness cannot"
    );
    println!("     reproduce the exact mix that was gathered — only this biome's own. Treat the two flora rows as an order-of-magnitude check.");

    println!(
        "\n  {:<34} {:>4} {:>10} {:>10} {:>9}  verdict",
        "source", "crew", "live", "harness", "delta"
    );
    for reading in LIVE_READINGS {
        let slot = CREWS.iter().position(|crew| *crew == reading.crew);
        let Some(slot) = slot else {
            continue;
        };
        let measured = match reading.label {
            "harvest tile, whole basket" => Some(forage_measured(
                &patch,
                &composition,
                &TakeSelection::EVERYTHING,
                CREWS[slot],
                shipped,
            )),
            "harvest tile, food only" => Some(forage_measured(
                &patch,
                &composition,
                &food_only,
                CREWS[slot],
                shipped,
            )),
            species => fauna
                .iter()
                .find(|(row, _)| row.source == species && row.rung == "wild hunt")
                .map(|(row, _)| row.food_per_worker[slot]),
        };
        let Some(measured) = measured else {
            println!(
                "  {:<34} {:>4} {:>10.4} {:>10} {:>9}  NO ROW — the harness never printed this source",
                reading.label, reading.crew, reading.food_per_worker, "-", "-",
            );
            continue;
        };
        let delta = if reading.food_per_worker > 0.0 {
            (measured - reading.food_per_worker) / reading.food_per_worker
        } else {
            f32::INFINITY
        };
        let verdict = if delta.abs() <= VALIDATION_TOLERANCE {
            "agrees".to_string()
        } else {
            format!("DISAGREES by {:.0}%", delta.abs() * 100.0)
        };
        println!(
            "  {:<34} {:>4} {:>10.4} {:>10.4} {:>8.0}%  {verdict}",
            reading.label,
            reading.crew,
            reading.food_per_worker,
            measured,
            delta * 100.0,
        );
    }
    // --- The flora gap, MEASURED rather than listed. Two terms can produce it and they are
    // --- separable: the basket's conversion rate (visible while carry binds) and the stand's
    // --- standing stock (visible in whether the per-worker figure is flat across crew sizes).
    let carry_at_one = forage_carry(&forage_outfit(1, shipped), shipped);
    let harness_rate = patch_provisions_per_biomass_taking(
        &patch,
        &composition,
        &shipped.flora,
        &shipped.labor.forage,
        &TakeSelection::EVERYTHING,
    );
    let live_at_one = LIVE_READINGS
        .iter()
        .find(|reading| reading.label == "harvest tile, whole basket" && reading.crew == 1)
        .map_or(0.0, |reading| reading.food_per_worker);
    println!("\n  THE FLORA GAP, SEPARATED INTO ITS TWO TERMS — neither is a harness error, and the table says which is which:");
    println!(
        "\n  (a) THE BASKET CONVERTS DIFFERENTLY. While carry binds, food/worker = carry x provisions_per_biomass, so a live reading"
    );
    println!("      inverts to the basket it was taken on:");
    println!(
        "        harness basket here      {harness_rate:.5} provisions per unit biomass  (this biome's realization under the pinned seed)"
    );
    println!(
        "        the live reading implies {:.5}   ( {live_at_one:.2} food/worker / {carry_at_one:.1} carry )",
        live_at_one / carry_at_one,
    );
    println!("      Different tile, different realized mix. Nothing here is fixable without the map the reading came off.");
    println!("\n  (b) THE STAND WAS FULLER THAN THE OPERATING POINT. The live figures are FLAT per worker — 0.21 at one worker and 1.04/5 =");
    println!("      0.21 at five — which means the ESCAPEMENT CEILING never bound and carry did, at both crew sizes. This harness seats every");
    println!("      source at floor x K, where the only room is one turn's growth, so its ceiling binds from about two workers on. Re-seating");
    println!("      the same tile at a FULL stand (B = K), which is what an untouched patch a band has just walked up to looks like:");
    let mut full = seated_patch(LIVE_HARVEST_TERRAIN, None, shipped);
    full.biomass = full.carrying_capacity;
    full.biomass_before_regrowth = full.biomass;
    println!(
        "        {:>5} {:>16} {:>16} {:>16}",
        "crew", "at floor x K", "at a FULL stand", "live"
    );
    for (slot, crew) in CREWS.iter().enumerate() {
        let live = LIVE_READINGS
            .iter()
            .find(|reading| reading.label == "harvest tile, whole basket" && reading.crew == *crew)
            .map(|reading| format!("{:.4}", reading.food_per_worker))
            .unwrap_or_else(|| "-".to_string());
        let _ = slot;
        println!(
            "        {:>5} {:>16.4} {:>16.4} {:>16}",
            crew,
            forage_measured(
                &patch,
                &composition,
                &TakeSelection::EVERYTHING,
                *crew,
                shipped
            ),
            forage_measured(
                &full,
                &composition,
                &TakeSelection::EVERYTHING,
                *crew,
                shipped
            ),
            live,
        );
    }
    println!("      The full-stand column is flat across the crew sizes exactly as the live readings are. THE SHAPE MATCHES; the level does");
    println!("      not, and (a) is why. The harness is not wrong to seat at the operating point — a steady rate is what Sections A-C are for —");
    println!("      but a live reading of a fresh tile is quoting accumulated stock, and the two answer different questions.");

    println!("\n  WHERE A DISAGREEMENT COMES FROM, in the order to suspect it:");
    println!("   1. THE HERD-K FIXTURE. The harness has no world, so a herd's K is the roster's full-group band, not the graze flow under its");
    println!("      footprint. K sets the escapement room, so any row whose binding stage is 'floor' inherits that fixture's error directly.");
    println!("      A row bound by 'engagement' or 'carry' does NOT — those terms are crew x species and have no K in them, which is why the");
    println!("      engagement-bound rows are the trustworthy half of this comparison. EVERY fauna row above agreed, so this fixture is not");
    println!("      currently costing anything measurable at these crew sizes.");
    println!("   2. THE FLORA BASKET AND THE STAND'S STOCK — both measured directly above, both properties of the reading rather than faults.");
    println!("   3. THE SEASON. Every row here is priced at seasonal weight 1.0; a live reading is taken in whatever season it was taken in.");
}

/// The plant food/worker/turn the validation block compares — the same call Section A makes, at the
/// live reading's own crew and this biome's own ground.
fn forage_measured(
    patch: &ForagePatch,
    composition: &[core_sim::FloraShare],
    take: &TakeSelection,
    workers: u32,
    shipped: &Shipped,
) -> f32 {
    let gear = forage_outfit(workers, shipped);
    forage_food_per_worker(
        patch,
        composition,
        take,
        workers,
        forage_carry(&gear, shipped),
        shipped,
    )
}

// ---------------------------------------------------------------------------------------------
// The mammoth
// ---------------------------------------------------------------------------------------------

/// **THE STAGE TERMS ON A TURN THE HERD CAN ACTUALLY SPARE AN ANIMAL.**
///
/// Turn one of a source seated at `floor · K` has only one turn's growth above the floor, so on a
/// big-bodied quarry the escapement room is under one body and it zeroes the engagement before reach
/// is ever tested. Reading only that turn would report the floor and hide the reach failure behind
/// it. This walks the live path forward until the room clears one body, then resolves one more turn
/// and hands back that turn's terms — which is the picture that repeats for the rest of the run.
///
/// `None` if the room never clears a body inside the horizon.
struct SteadyTerms {
    turn: u32,
    room_bodies: f32,
    reach: f32,
    stayed: f32,
    brought_down: f32,
    bound: HuntTakeBound,
}

fn steady_terms(
    herd: &Herd,
    party: &HuntingParty,
    workers: u32,
    carry: f32,
    shipped: &Shipped,
) -> Option<SteadyTerms> {
    let mut quarry = herd.clone();
    for turn in 1..=shipped.labor.yield_average_horizon_turns {
        regrow_biomass(&mut quarry, &shipped.fauna);
        let room = herd_take_room(&quarry, DEFAULT_ESCAPEMENT_FLOOR, &shipped.fauna);
        let reach = animals_engaged(workers, herd_engage_rate(&quarry, &shipped.fauna));
        let outcome = hunt_take(
            &mut quarry,
            workers,
            DEFAULT_ESCAPEMENT_FLOOR,
            carry,
            party,
            &shipped.fauna,
            NO_CARRY_LIMIT,
            HuntDraw::EXPECTED,
        );
        if room >= quarry.body_mass {
            return Some(SteadyTerms {
                turn,
                room_bodies: room / quarry.body_mass,
                reach,
                stayed: outcome.engaged - outcome.fled,
                brought_down: outcome.fight.brought_down,
                bound: outcome.bound,
            });
        }
    }
    None
}

/// **WHY THE HIGHEST `engage_rate x body_mass` CEILING IN THE GAME PAYS LAST.**
///
/// `docs/plan_hunt_through_combat.md` §2.1 authors `engage_rate` as a spatial constraint and pins the
/// roster against `ceiling = engage_rate x body_mass`; the mammoth's is the largest on the roster. §2
/// also predicts the failure mode this block exists to test for: *an `engage_rate` set too low
/// silently becomes a second floor.* Every number below is the sim's own — the stage terms come off
/// `HuntOutcome`, the bound off `hunt_take_bound` through it.
fn print_mammoth(shipped: &Shipped) {
    println!("\n{RULE}");
    println!("MAMMOTH DIAGNOSIS — the largest engage_rate x body_mass ceiling on the roster, and the last-paying row in Section C.");
    println!("{RULE}");
    let def = &shipped.fauna.species[MAMMOTH];
    let Some(herd) = seated_herd(MAMMOTH, def, RungKey::AnimalWild, shipped) else {
        println!("  the mammoth could not be seated — nothing to diagnose");
        return;
    };
    println!(
        "\n  species terms: body {:.0}, engage_rate {:.2}  ->  authored ceiling {:.1} biomass/hunter/turn (the roster's highest)",
        def.body_mass,
        herd_engage_rate(&herd, &shipped.fauna),
        herd_engage_rate(&herd, &shipped.fauna) * def.body_mass,
    );
    println!(
        "  herd terms:    r {:.3}, K {:.0} (roster band), seated at floor x K = {:.0}, one turn's growth {:.1} biomass = {:.2} bodies",
        herd.regrowth_rate,
        herd.carrying_capacity,
        herd.biomass,
        herd.carrying_capacity * herd.regrowth_rate * DEFAULT_ESCAPEMENT_FLOOR
            * (1.0 - DEFAULT_ESCAPEMENT_FLOOR),
        herd.carrying_capacity * herd.regrowth_rate * DEFAULT_ESCAPEMENT_FLOOR
            * (1.0 - DEFAULT_ESCAPEMENT_FLOOR)
            / def.body_mass,
    );
    println!("\n  Every stage term is in BODIES, because a mammoth take is quantised to whole animals. The three stage columns are read on the");
    println!("  first turn the herd can spare a WHOLE body — turn one has only one turn's growth above the floor, so the escapement room");
    println!("  zeroes the engagement there and would hide the reach failure behind it.");
    println!(
        "  {:>7} {:<9} {:>7} {:>7} | {:>5} {:>7} {:>7} {:>9} {:<12} | {:>5} {:>8} {:>8} {:>8} {:<12}",
        "hunters",
        "kit",
        "carry",
        "reach",
        "turn",
        "room",
        "stayed",
        "broughtDn",
        "binds",
        "kills",
        "carried",
        "wasted",
        "food/w/t",
        "modal",
    );
    println!(
        "  {:>7} {:<9} {:>7} {:>7} | {:>5} {:>7} {:>7} {:>9} {:<12} | {:>5} {:>8} {:>8} {:>8} {:<12}",
        "",
        "",
        "bodies",
        "bodies",
        "",
        "bodies",
        "bodies",
        "bodies",
        "on that turn",
        "/run",
        "biomass",
        "biomass",
        "",
        "over the run",
    );
    for crew in MAMMOTH_CREWS {
        let gear = hunt_outfit(def, false, crew, shipped);
        let carry = hunt_carry(&gear, shipped);
        let party = hunt_party(&gear, herd.body_mass, shipped);
        let drive = drive_take(&herd, &party, crew, carry, shipped);
        let steady = steady_terms(&herd, &party, crew, carry, shipped);
        println!(
            "  {:>7} {:<9} {:>7.3} {:>7.3} | {:>5} {:>7.3} {:>7.3} {:>9.3} {:<12} | {:>5} {:>8.0} {:>8.0} {:>8.4} {:<12}  {}",
            crew,
            gear.id(),
            crew as f32 * carry / def.body_mass,
            steady.as_ref().map_or(0.0, |terms| terms.reach),
            steady
                .as_ref()
                .map_or("never".to_string(), |terms| terms.turn.to_string()),
            steady.as_ref().map_or(0.0, |terms| terms.room_bodies),
            steady.as_ref().map_or(0.0, |terms| terms.stayed),
            steady.as_ref().map_or(0.0, |terms| terms.brought_down),
            steady
                .as_ref()
                .map_or("-", |terms| terms.bound.as_str()),
            drive.kills,
            drive.carried,
            drive.wasted,
            hunt_food_per_worker(&herd, &party, crew, carry, shipped),
            drive.modal().map_or("-", |bound| bound.as_str()),
            drive.rendered_tally(),
        );
    }
    println!(
        "\n  THE HEADLINE: the mammoth's authored ceiling is the roster's highest and NOTHING IN THE TAKE EVER SEES IT. Reach is a fraction"
    );
    println!("  of an animal until 20 hunters; below that the party puts nothing on the ground for turns at a time, and when it finally does,");
    println!("  its sleds carry home a fraction of the carcass. What binds, stage by stage:");
    println!(
        "   • REACH IS UNDER ONE ANIMAL until 20 hunters. `animals_engaged = workers x engage_rate`, and at {:.2} that is {:.2} bodies per",
        herd_engage_rate(&herd, &shipped.fauna),
        herd_engage_rate(&herd, &shipped.fauna),
    );
    let rate = herd_engage_rate(&herd, &shipped.fauna);
    println!(
        "     hunter — so it takes {:.0} hunters to reach exactly ONE whole mammoth per turn. The retreat then shaves that fraction",
        if rate > 0.0 { 1.0 / rate } else { f32::INFINITY },
    );
    println!("     further, and `resolve_hunt_fight` floors what it is handed to whole animals, so the party puts NOTHING on the ground on");
    println!("     almost every turn. That is precisely the failure mode §2 of docs/plan_hunt_through_combat.md predicts:");
    println!("     **an engage_rate set too low silently becomes a second floor.** It is a REACH failure.");
    println!(
        "   • IT IS REPORTED AS `fight`, NOT AS `engagement`, AND THAT NAME IS MISLEADING HERE. `hunt_take_bound`'s last two arms split"
    );
    let lone_gear = hunt_outfit(def, false, 1, shipped);
    let lone_party = hunt_party(&lone_gear, herd.body_mass, shipped);
    let lone = steady_terms(
        &herd,
        &lone_party,
        1,
        hunt_carry(&lone_gear, shipped),
        shipped,
    );
    let lone_attack = lone_party
        .crews
        .first()
        .map_or(0.0, |crew| crew.hunter.attack);
    println!("     one min: `brought_down < stayed` reads Fight, `brought_down == stayed` reads Engagement. `stayed` is a FRACTION of an");
    println!(
        "     animal ({:.3} at one hunter, on a turn the herd CAN spare a body) while `brought_down` has already been floored to {:.0}, so",
        lone.as_ref().map_or(0.0, |terms| terms.stayed),
        lone.as_ref().map_or(0.0, |terms| terms.brought_down),
    );
    println!("     the strict inequality holds and the bound is named Fight — for a party that never reached a whole animal to fight. The");
    println!(
        "     party is NOT losing the fight: its attack {lone_attack:.0} beats the mammoth's defense {:.0}. Fixing that naming is a sim change, and it",
        def.combat.defense,
    );
    println!("     is NOT part of this measurement.");
    println!(
        "   • THE ESCAPEMENT FLOOR IS THE SECOND WALL, and it is the one that binds at 20 hunters. 'room' is `hunt_take_room(floor, B, K,"
    );
    println!(
        "     growth)` at the shipped harvest floor of {DEFAULT_ESCAPEMENT_FLOOR:.2} — the stock above floor x K plus this turn's growth share. Seated at the"
    );
    println!("     operating point that is one turn's growth, 0.15 of a body, so the herd cannot spare a whole mammoth for ~7 turns whatever");
    println!("     the party does. Once a kill lands it resets, which is why `floor` dominates the 20-hunter drive.");
    println!(
        "   • CARRY BARELY EVER *STOPS* A KILL, AND IT THROWS MOST OF IT AWAY. One sled hauls {:.3} of a body, and the quantiser takes a",
        hunt_carry(&hunt_outfit(def, false, 1, shipped), shipped) / def.body_mass
    );
    println!("     whole animal whether or not the pack can seat it — so `carry` shows up in the tally as a straggler while the waste is");
    println!("     enormous. Measured over the run:");
    for crew in MAMMOTH_CREWS {
        let gear = hunt_outfit(def, false, crew, shipped);
        let carry = hunt_carry(&gear, shipped);
        let party = hunt_party(&gear, herd.body_mass, shipped);
        let drive = drive_take(&herd, &party, crew, carry, shipped);
        let dropped = drive.carried + drive.wasted;
        println!(
            "       {crew:>2} hunter(s): killed {:>5.0} biomass, carried {:>5.0}, LEFT {:>5.0} to rot ({}) ",
            dropped,
            drive.carried,
            drive.wasted,
            if dropped > 0.0 {
                format!("{:.0}% of the kill wasted", drive.wasted / dropped * 100.0)
            } else {
                "nothing was ever brought down".to_string()
            },
        );
    }
    println!("   • WOUNDS ACCUMULATE ACROSS TURNS (§4.2), which is why food/w/t is not zero above one hunter: a sub-threshold party grinds");
    println!("     for several turns and then lands a whole animal. The 'kills /run' column is that pulse counted.");
}

// ---------------------------------------------------------------------------------------------
// The one test
// ---------------------------------------------------------------------------------------------

/// **PRINTS THE TABLE.** Run it with `--nocapture`; the assertions at the end exist only so a table
/// of zeros cannot exit `0` and be mistaken for a reading.
#[test]
fn the_food_economy_table() {
    let shipped = Shipped::load();

    print_inputs(&shipped);

    let flora = flora_rows(&shipped);
    print_section_a(&flora, &shipped);

    let fauna = fauna_rows(&shipped);
    print_section_b(&fauna);

    let mut all: Vec<&Row> = flora
        .iter()
        .chain(fauna.iter().map(|(row, _)| row))
        .collect();
    all.sort_by(|a, b| b.reference_food().total_cmp(&a.reference_food()));
    print_section_c(&all);

    print_rung_pairs(&flora, &fauna);
    print_plant_crew_curve(&shipped);
    print_standardised(&shipped);
    print_steady_rates(&shipped, &fauna);
    print_validation(&shipped, &fauna);
    print_mammoth(&shipped);
    print_species_coverage(&shipped, &fauna);

    // ---- Liveness. A diff-based reading of a table improves when the table breaks. ----
    assert!(
        !flora.is_empty() && !fauna.is_empty(),
        "both webs must produce rows"
    );
    for row in &all {
        for food in row.food_per_worker {
            assert!(
                food.is_finite() && food >= 0.0,
                "{} {} printed a non-finite food rate: {food}",
                row.source,
                row.rung
            );
        }
        assert!(
            row.sustainable.is_finite()
                && row.build_work.is_finite()
                && row.upkeep_work.is_finite()
                && row.food_per_worker_bare.is_finite()
                && row.carry_utilisation.is_finite(),
            "{} {} printed a non-finite work, MSY, control or utilisation figure",
            row.source,
            row.rung
        );
    }
    assert!(
        flora.iter().any(|row| row.reference_food() > 0.0),
        "at least one FLORA row must pay food — a table of plant zeros is a broken harness, not a reading"
    );
    assert!(
        fauna.iter().any(|(row, _)| row.reference_food() > 0.0),
        "at least one FAUNA row must pay food — a table of animal zeros is a broken harness, not a reading"
    );
    assert!(
        all.iter()
            .any(|row| row.materials.as_ref().is_some_and(|rows| !rows.is_empty())),
        "at least one row must pay a material, or the materials column is wired to nothing"
    );
    // **The binding-stage column is the slice's whole point** — an empty one is a silent hole, so it
    // fails instead. Every crew size, every fauna row.
    for (row, _) in &fauna {
        for (slot, workers) in CREWS.iter().enumerate() {
            assert!(
                row.bound[slot].is_some(),
                "{} {} left its binding stage empty at {workers} worker(s) — the live-take drive \
                 produced no turn to read a bound off",
                row.source,
                row.rung
            );
        }
        assert!(
            row.drive
                .as_ref()
                .is_some_and(|drive| !drive.tally.is_empty()),
            "{} {} produced an empty binding-stage tally",
            row.source,
            row.rung
        );
    }
    // **The gear PATH is wired, which is not the same claim as the band OWNING gear.** This used to
    // assert that some row out-earns its bare-carry control — true only while `start_stock_fraction`
    // was positive, so it was reading a *config value* through a behavioural guard and it fired the
    // moment a spawn stopped owning equipment (which is a shipped state, not a break). What has to
    // stay true is that the outfit resolves through the real seams at all: a kit id, a coverage, a
    // party. If `forage_outfit` stopped reaching `EquipmentConfig` the id would be empty.
    assert!(
        all.iter()
            .all(|row| !row.kit.is_empty() || row.web == "flora"),
        "a fauna row resolved no kit id — the equipment path is not wired in"
    );
}

/// Every roster species has to appear, or the table is silently narrower than it claims.
fn print_species_coverage(shipped: &Shipped, rows: &[(Row, FaunaExtras)]) {
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for (row, _) in rows {
        *seen.entry(row.source.as_str()).or_default() += 1;
    }
    let missing: Vec<&str> = shipped
        .fauna
        .species
        .values()
        .filter(|def| !seen.contains_key(def.display_name.as_str()))
        .map(|def| def.display_name.as_str())
        .collect();
    println!(
        "\n  Roster coverage: {} of {} species printed, {} rows.",
        seen.len(),
        shipped.fauna.species.len(),
        rows.len()
    );
    assert!(
        missing.is_empty(),
        "every roster species must appear in Section B; missing: {missing:?}"
    );
}
