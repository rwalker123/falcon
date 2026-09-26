//! **FLORA VS FAUNA, FOOD PER WORKER, RUNG BY RUNG** — a printing probe, not a balance test.
//!
//! ```text
//! cargo test -p core_sim --test food_rate_survey -- --ignored --nocapture
//! ```
//!
//! It answers one comparative question: where do the two food webs' per-worker curves sit today, as
//! **rung pairs** — wild hunt vs forage, tame vs tended, pen vs field — for a band that owns its kit?
//! Every cell is a **settled** rate: the source is seated at the operating point the shipped floor
//! leaves it at (`floor · K`), driven through the sim's own take path for [`WARMUP_TURNS`], and then
//! averaged over [`MEASURED_TURNS`] more, so neither the whole-animal quantum nor the wound ledger's
//! carry shows as a lump.
//!
//! - **The animal web** is driven through `hunt_take` itself, Logistics regrowth first, the shipped
//!   order — the function the turn pays through.
//! - **The plant web** is driven through `project_realized_forage`, whose loop body is the one
//!   `forage_take` the turn pays through; a gather is continuous, so the projection *is* the take.
//!
//! **Every animal cell is printed twice: the shipped config, and "was" — the pre-trial baseline**
//! ([`was_fauna`]), rebuilt in-process by overriding the trial's levers on a clone, so the comparison
//! survives the JSON moving. The plant web is untouched by the trial, so its rows carry one reading.
//!
//! **The band is STOCKED** — `EquipmentConfig::for_a_stocked_fixture`, one party's worth of every item
//! plus the half-again reserve — because a shipped spawn owns nothing (`start_stock_fraction 0.0`).
//! **Every worker is local** (inside the apron): no caravan, no walk.

use std::sync::Arc;

use bevy::math::UVec2;
use core_sim::{
    build_work_per_worker_turn, forage_provisions, herd_default_hunt_kit, herd_density_gain,
    herd_ecology, herd_space_capacity, herd_standing_provisions, herd_upkeep_demand, hunt_take,
    patch_carrying_capacity, patch_composition, patch_ecology, patch_provisions_per_biomass_taking,
    patch_upkeep_demand, project_realized_forage, regrow_biomass, selected_biomass_share,
    sustainable_yield, BandEquipment, CombatConfig, CreaturesConfig, CultivationCeiling,
    DemographicsConfig, EquipmentConfig, FactionId, FaunaConfig, FloraConfig, FloraShare,
    ForagePatch, Herd, HuntDraw, HuntTakeBound, HuntingParty, HusbandryCeiling, KitChoice,
    KitCoverage, LaborConfig, LadderConfig, PartyResolution, Quarry, RungKey, SpeciesDef,
    TakeSelection, DEFAULT_ESCAPEMENT_FLOOR, NO_BUILD_GEAR,
};
use sim_runtime::TerrainType;

mod common;
/// The pinned realization seed and tile — the same pin every plant harness quotes against.
use common::reference_basket as basket;

// ---------------------------------------------------------------------------------------------
// Measurement choices — none of these is a gameplay lever.
// ---------------------------------------------------------------------------------------------

/// Turns driven before anything is counted, so the seated stock and the wound ledger settle.
const WARMUP_TURNS: u32 = 100;
/// Turns averaged after the warm-up.
const MEASURED_TURNS: u32 = 300;
/// The crews the wild rung is priced at.
const CREWS: [u32; 3] = [1, 3, 5];
/// The crews the improved rungs are priced at.
const RUNG_CREWS: [u32; 2] = [3, 5];
/// The crew whose binding stage a row names.
const BOUND_CREW: u32 = 3;
/// Neutral band productivity, so every row is the source's own figure.
const UNIT_OUTPUT_MULTIPLIER: f32 = 1.0;
/// A full growing season.
const FULL_SEASONAL_WEIGHT: f32 = 1.0;
/// The faction that owns every improvement this probe seats.
const SURVEY_FACTION: FactionId = FactionId(0);
/// A herd fixture's route: one tile, because nothing here moves.
const FIXTURE_TILES: [UVec2; 1] = [UVec2::new(1, 1)];
/// The synthetic map a footprint disk is counted on — big enough that nothing clips.
const FIXTURE_MAP: u32 = 64;
/// The fixture map does not wrap.
const FIXTURE_WRAP: bool = false;
/// A footprint is anchored well inside [`FIXTURE_MAP`].
const FIXTURE_ANCHOR: UVec2 = UVec2::new(32, 32);
/// A plant patch is one tile — the unit `capacity_by_biome` is authored in.
const PATCH_TILES: usize = 1;
/// A resident band banks its whole take — `hunt_take`'s own contract for the band arm.
const NO_CARRY_LIMIT: f32 = f32::INFINITY;
/// The Stalking kit's roster id.
const STALKING_KIT: &str = "big_game";
/// The Trapping kit's roster id.
const TRAPPING_KIT: &str = "trapping";
/// The Harvesting kit's roster id — the gatherers' baskets.
const HARVESTING_KIT: &str = "gathering";
/// A single worker, for the uncapped per-worker rate.
const ONE_WORKER: u32 = 1;
/// How far a "fat" patch's `K` is inflated to read one gatherer's UNCAPPED rate.
const FAT_SOURCE_SCALE: f32 = 1_000.0;
/// A take "reaches the line" once its total is within this share of the source's sustainable line.
const LINE_SHARE: f32 = 0.95;
/// The largest crew the line sweep tries.
const MAX_SWEEP_CREW: u32 = 200;
/// Every crew up to here is swept one at a time; above it, in [`SWEEP_COARSE_STEP`]s.
const SWEEP_FINE_UNTIL: u32 = 40;
/// The step the sweep takes above [`SWEEP_FINE_UNTIL`].
const SWEEP_COARSE_STEP: usize = 10;
/// A kept herd committing its whole standing stock to milk / eggs / wool.
const FULL_STANDING_COMMITMENT: f32 = 1.0;
/// How many forage terrains the wild table prints in full; the rest is one tail line.
const FORAGE_ROWS_SHOWN: usize = 12;
/// A carry-bound row delivers this share of one worker's uncapped rate or more; below it, the stand
/// is what binds.
const CARRY_BOUND_SHARE: f32 = 0.99;

// ---------------------------------------------------------------------------------------------
// The pre-trial baseline ("was")
// ---------------------------------------------------------------------------------------------

/// `hunt.provisions_per_biomass` before the trial.
const WAS_MEAT_RATE: f32 = 0.02;
/// `husbandry.pastoral_gain` before the trial.
const WAS_PASTORAL_GAIN: f32 = 2.0;
/// `husbandry.pen_gain` before the trial.
const WAS_PEN_GAIN: f32 = 4.0;
/// `animals_per_herder` before the trial, for every species the trial moves.
const WAS_ANIMALS_PER_HERDER: [(&str, f32); 8] = [
    ("aurochs", 12.0),
    ("steppe_runner", 15.0),
    ("marsh_grazer", 15.0),
    ("wild_horse", 25.0),
    ("reindeer", 100.0),
    ("boar", 15.0),
    ("wild_sheep", 50.0),
    ("crag_goat", 80.0),
];

/// **The fauna config as it stood before the trial** — the shipped config with the trial's levers put
/// back, so "was" stays the baseline whatever the JSON says today.
fn was_fauna(now: &FaunaConfig) -> FaunaConfig {
    let mut was = now.clone();
    was.hunt.provisions_per_biomass = WAS_MEAT_RATE;
    was.husbandry.pastoral_gain = WAS_PASTORAL_GAIN;
    was.husbandry.pen_gain = WAS_PEN_GAIN;
    for (key, herders) in WAS_ANIMALS_PER_HERDER {
        was.species
            .get_mut(key)
            .unwrap_or_else(|| panic!("the shipped roster carries `{key}`"))
            .animals_per_herder = herders;
    }
    was
}

// ---------------------------------------------------------------------------------------------
// The shipped configs, and the stocked band
// ---------------------------------------------------------------------------------------------

struct Shipped {
    labor: Arc<LaborConfig>,
    flora: Arc<FloraConfig>,
    now: Arc<FaunaConfig>,
    was: FaunaConfig,
    ladder: Arc<LadderConfig>,
    combat: Arc<CombatConfig>,
    /// The stocked fixture roster, so an armed column reads armed.
    equipment: Arc<EquipmentConfig>,
    creatures: Arc<CreaturesConfig>,
    /// `demographics.consumption.per_capita_draw` — the "× upkeep" denominator.
    per_capita_draw: f32,
}

impl Shipped {
    fn load() -> Self {
        let now = FaunaConfig::builtin();
        let was = was_fauna(&now);
        Self {
            labor: LaborConfig::builtin(),
            flora: FloraConfig::builtin(),
            now,
            was,
            ladder: LadderConfig::builtin(),
            combat: CombatConfig::builtin(),
            equipment: Arc::new(EquipmentConfig::for_a_stocked_fixture()),
            creatures: CreaturesConfig::builtin(),
            per_capita_draw: DemographicsConfig::builtin().consumption.per_capita_draw,
        }
    }

    /// `food (× upkeep)`.
    fn cell(&self, food: f32) -> String {
        format!("{food:.3} ({:.2}x)", food / self.per_capita_draw)
    }

    /// `food (× upkeep) ← was`.
    fn pair(&self, now: f32, was: f32) -> String {
        format!("{} ← {was:.3}", self.cell(now))
    }

    fn kit(&self, id: &str) -> KitChoice {
        self.equipment
            .kit(id)
            .unwrap_or_else(|| panic!("the shipped roster carries a `{id}` kit"))
    }
}

/// What a crew carries, and how that gear divides its people.
struct Gear {
    wear: BandEquipment,
    coverage: KitCoverage,
}

fn gear(kit: KitChoice, workers: u32, s: &Shipped) -> Gear {
    let wear = BandEquipment::start_stocked_for(&s.equipment, workers as f32);
    let coverage = s.equipment.coverage(&kit, workers as f32, &wear);
    Gear { wear, coverage }
}

fn forage_carry(g: &Gear, s: &Shipped) -> f32 {
    g.coverage.weighted_rate(|kit| {
        s.equipment.forage_per_worker_biomass_capacity(
            s.labor.forage.per_worker_biomass_capacity,
            kit,
            &g.wear,
        )
    })
}

fn hunt_carry(g: &Gear, s: &Shipped) -> f32 {
    g.coverage.weighted_rate(|kit| {
        s.equipment.hunt_per_worker_biomass_capacity(
            s.labor.hunt.per_worker_biomass_capacity,
            kit,
            &g.wear,
        )
    })
}

fn hunt_party(g: &Gear, body_mass: f32, s: &Shipped) -> HuntingParty {
    PartyResolution {
        equipment: &s.equipment,
        coverage: &g.coverage,
        wear: &g.wear,
        intrinsic: s.creatures.person(),
        tuning: s.combat.tuning(),
        hunt_injury_damage_per_animal: s.combat.hunt_injury_damage_per_animal,
    }
    .party_against(Quarry::Mass(body_mass))
}

/// Keeper-turns a rung's standing upkeep owes, at bare hands — workers too, and NOT in the take crew
/// the food column divides by.
fn keepers_owed(upkeep_work_per_turn: f32) -> f32 {
    upkeep_work_per_turn / build_work_per_worker_turn(NO_BUILD_GEAR)
}

/// Food per worker once the keepers are counted in the denominator.
fn with_keepers(food_per_worker: f32, crew: u32, keepers: f32) -> f32 {
    food_per_worker * crew as f32 / (crew as f32 + keepers)
}

/// The crews the line sweep settles.
fn sweep_crews() -> Vec<u32> {
    (1..=SWEEP_FINE_UNTIL)
        .chain(
            (SWEEP_FINE_UNTIL + SWEEP_COARSE_STEP as u32..=MAX_SWEEP_CREW)
                .step_by(SWEEP_COARSE_STEP),
        )
        .collect()
}

/// **The first crew whose settled total reaches [`LINE_SHARE`] of `line`**, or `never (best N%)`.
fn crew_to_line(line: f32, total_at: impl Fn(u32) -> f32) -> String {
    if line <= 0.0 {
        return "—".to_string();
    }
    let mut best = 0.0_f32;
    for crew in sweep_crews() {
        let total = total_at(crew);
        best = best.max(total);
        if total >= line * LINE_SHARE {
            return crew.to_string();
        }
    }
    format!("never (best {:.0}%)", best / line * 100.0)
}

// ---------------------------------------------------------------------------------------------
// The plant web
// ---------------------------------------------------------------------------------------------

/// A patch on `terrain`, committed to `crop` and seated on `rung` when one is named, at `floor · K`.
fn seated_patch(
    terrain: TerrainType,
    crop: Option<(&str, RungKey)>,
    capacity_scale: f32,
    s: &Shipped,
) -> ForagePatch {
    let tile_capacity = s.labor.forage.capacity_for(terrain) * capacity_scale;
    let mut patch = ForagePatch::new(basket::TILE, tile_capacity);
    if let Some((species, rung)) = crop {
        patch.species = Some(species.to_string());
        // **The precondition is FORCED** — the ladder position moves to the top of the rung through
        // the fixture mutator; no knowledge and no build is run.
        let seated = match rung {
            RungKey::PlantField => patch.complete_field(SURVEY_FACTION, &s.ladder),
            _ => patch.complete_cultivation(SURVEY_FACTION, &s.ladder),
        };
        assert!(
            seated,
            "{terrain:?} must be able to seat {species} on {rung:?}"
        );
    }
    patch.carrying_capacity = patch_carrying_capacity(tile_capacity, &patch, &s.labor.forage);
    patch.biomass = patch.carrying_capacity * DEFAULT_ESCAPEMENT_FLOOR;
    patch.biomass_before_regrowth = patch.biomass;
    patch
}

/// Settled food per worker per turn off a patch — turns `WARMUP..WARMUP + MEASURED`, as the
/// difference of two deterministic projections from the same start.
fn settle_forage(
    patch: &ForagePatch,
    composition: &[FloraShare],
    workers: u32,
    carry: f32,
    s: &Shipped,
) -> f32 {
    let project = |horizon: u32| {
        project_realized_forage(
            patch,
            composition,
            &s.labor.forage,
            &s.flora,
            carry,
            FULL_SEASONAL_WEIGHT,
            UNIT_OUTPUT_MULTIPLIER,
            workers,
            DEFAULT_ESCAPEMENT_FLOOR,
            &TakeSelection::EVERYTHING,
            horizon,
        ) * horizon as f32
    };
    let total = project(WARMUP_TURNS + MEASURED_TURNS) - project(WARMUP_TURNS);
    total / MEASURED_TURNS as f32 / workers as f32
}

/// The stand's own sustainable line, in food/turn.
fn patch_line(patch: &ForagePatch, composition: &[FloraShare], s: &Shipped) -> f32 {
    let take = TakeSelection::EVERYTHING;
    let per_biomass =
        patch_provisions_per_biomass_taking(patch, composition, &s.flora, &s.labor.forage, &take);
    let selected = selected_biomass_share(
        &patch_composition(patch, composition, &s.flora, &s.labor.forage),
        &take,
    );
    forage_provisions(
        sustainable_yield(
            patch.biomass * selected,
            patch.carrying_capacity * selected,
            &patch_ecology(patch, &s.labor.forage),
        ),
        per_biomass,
        UNIT_OUTPUT_MULTIPLIER,
    )
}

fn forage_terrains(s: &Shipped) -> Vec<TerrainType> {
    let mut terrains: Vec<TerrainType> = s
        .labor
        .forage
        .capacity_by_biome
        .iter()
        .filter(|(_, capacity)| **capacity > 0.0)
        .map(|(terrain, _)| *terrain)
        .collect();
    terrains.sort_by_key(|terrain| format!("{terrain:?}"));
    terrains
}

fn basket_on(terrain: TerrainType, s: &Shipped) -> Vec<FloraShare> {
    s.flora
        .realized_composition(terrain, basket::TILE, basket::SEED)
}

/// The best food crop in this basket that `rung` admits — the crop a player chasing food commits.
fn best_crop(composition: &[FloraShare], rung: RungKey, s: &Shipped) -> Option<String> {
    composition
        .iter()
        .filter_map(|share| {
            let def = s.flora.species.get(&share.species)?;
            let admits = match rung {
                RungKey::PlantField => def.cultivation_ceiling == CultivationCeiling::Field,
                _ => def.cultivation_ceiling != CultivationCeiling::Wild,
            };
            (admits && def.yield_.provisions_per_biomass > 0.0)
                .then_some((share.species.clone(), def.yield_.provisions_per_biomass))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
        .map(|(species, _)| species)
}

/// Everything one plant row prints.
struct PlantRow {
    per_worker: Vec<f32>,
    line: f32,
    crew_to_line: String,
    binds: &'static str,
}

/// Measure one patch at `crews` with the Harvesting kit, plus its line and binding term.
fn plant_row(
    terrain: TerrainType,
    crop: Option<(&str, RungKey)>,
    crews: &[u32],
    s: &Shipped,
) -> PlantRow {
    let composition = basket_on(terrain, s);
    let patch = seated_patch(terrain, crop, 1.0, s);
    let fat = seated_patch(terrain, crop, FAT_SOURCE_SCALE, s);
    let basket_kit = s.kit(HARVESTING_KIT);
    let carry_at = |workers: u32| forage_carry(&gear(basket_kit.clone(), workers, s), s);
    let per_worker: Vec<f32> = crews
        .iter()
        .map(|workers| settle_forage(&patch, &composition, *workers, carry_at(*workers), s))
        .collect();
    let line = patch_line(&patch, &composition, s);
    let uncapped = settle_forage(&fat, &composition, ONE_WORKER, carry_at(ONE_WORKER), s);
    let at_bound = settle_forage(&patch, &composition, BOUND_CREW, carry_at(BOUND_CREW), s);
    let binds = if at_bound >= uncapped * CARRY_BOUND_SHARE {
        "carry (basket)"
    } else {
        "stand (K × r)"
    };
    PlantRow {
        per_worker,
        line,
        crew_to_line: crew_to_line(line, |crew| {
            settle_forage(&patch, &composition, crew, carry_at(crew), s) * crew as f32
        }),
        binds,
    }
}

// ---------------------------------------------------------------------------------------------
// The animal web
// ---------------------------------------------------------------------------------------------

/// A herd seated on `rung` the way the sim seats it, under `fauna` — the `food_economy_table` fixture.
fn seated_herd(
    key: &str,
    def: &SpeciesDef,
    rung: RungKey,
    fauna: &FaunaConfig,
    s: &Shipped,
) -> Option<Herd> {
    let allowed = match rung {
        RungKey::AnimalWild => true,
        RungKey::AnimalPastoral => def.husbandry_ceiling != HusbandryCeiling::Wild,
        RungKey::AnimalPen => def.husbandry_ceiling == HusbandryCeiling::Pen,
        _ => false,
    };
    if !allowed {
        return None;
    }
    let range_capacity = def.biomass[1];
    let mut herd = Herd::new(
        format!("survey_{key}"),
        def.display_name.clone(),
        def.size_class,
        FIXTURE_TILES.to_vec(),
        range_capacity,
        range_capacity,
        def.fodder_per_biomass,
        def.regrowth_rate.unwrap_or(fauna.ecology.regrowth_rate),
        def.body_mass,
    );
    herd.husbandry_ceiling = def.husbandry_ceiling;
    herd.taming_cost_multiplier = fauna.taming_cost_multiplier_for(&herd.species);
    let seated = match rung {
        RungKey::AnimalWild => true,
        RungKey::AnimalPastoral => herd.tame_outright(SURVEY_FACTION, &s.ladder),
        _ => {
            herd.tame_outright(SURVEY_FACTION, &s.ladder)
                && herd.corral_at(FIXTURE_TILES[0], &s.ladder)
        }
    };
    assert!(seated, "{key} must be able to stand on {rung:?}");
    herd.carrying_capacity =
        (range_capacity * herd_density_gain(&herd.standing(), &herd, fauna)).min(
            herd_space_capacity(footprint_tiles(&herd, def), herd.body_mass, fauna),
        );
    herd.biomass = herd.carrying_capacity * DEFAULT_ESCAPEMENT_FLOOR;
    herd.biomass_before_regrowth = herd.biomass;
    // A managed herd whose keeping went unmet would be measuring neglect, not the rung.
    herd.upkeep_supplied = herd_upkeep_demand(&herd, fauna, &s.ladder);
    herd.refresh_ecology_phase(fauna);
    Some(herd)
}

/// **The tiles a herd's footprint covers** — the fenced disk once penned, the roam disk otherwise,
/// counted by the sim's own `hex_range_tiles`.
fn footprint_tiles(herd: &Herd, def: &SpeciesDef) -> usize {
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

/// A settled hunt.
struct SettledHunt {
    food_per_worker: f32,
    modal_bound: Option<HuntTakeBound>,
    /// The herd's average stock over the measured window, as a fraction of its `K`.
    stock_over_k: f32,
}

/// Drive `hunt_take` for `WARMUP + MEASURED` turns under `fauna` and average the measured window.
fn settle_hunt(
    herd: &Herd,
    party: &HuntingParty,
    workers: u32,
    carry: f32,
    fauna: &FaunaConfig,
) -> SettledHunt {
    let mut quarry = herd.clone();
    let ecology = herd_ecology(&quarry, fauna);
    let capacity = quarry.carrying_capacity;
    let provisions_per_biomass = fauna.hunt_yield_for(&quarry.species).provisions_per_biomass;
    let mut carried = 0.0_f32;
    let mut stock = 0.0_f32;
    let mut tally: Vec<(HuntTakeBound, u32)> = Vec::new();
    for turn in 0..WARMUP_TURNS + MEASURED_TURNS {
        regrow_biomass(&mut quarry, fauna);
        if quarry.biomass <= ecology.extinction_floor * capacity {
            break;
        }
        let outcome = hunt_take(
            &mut quarry,
            workers,
            DEFAULT_ESCAPEMENT_FLOOR,
            carry,
            party,
            fauna,
            NO_CARRY_LIMIT,
            HuntDraw::EXPECTED,
        );
        if turn >= WARMUP_TURNS {
            carried += outcome.take.carried;
            stock += quarry.biomass;
            match tally.iter_mut().find(|(bound, _)| *bound == outcome.bound) {
                Some((_, count)) => *count += 1,
                None => tally.push((outcome.bound, 1)),
            }
        }
    }
    tally.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    SettledHunt {
        food_per_worker: carried * provisions_per_biomass * UNIT_OUTPUT_MULTIPLIER
            / MEASURED_TURNS as f32
            / workers as f32,
        modal_bound: tally.first().map(|(bound, _)| *bound),
        stock_over_k: stock / MEASURED_TURNS as f32 / capacity,
    }
}

fn hunt_with(
    herd: &Herd,
    kit: &KitChoice,
    workers: u32,
    fauna: &FaunaConfig,
    s: &Shipped,
) -> SettledHunt {
    let g = gear(kit.clone(), workers, s);
    let carry = hunt_carry(&g, s);
    let party = hunt_party(&g, herd.body_mass, s);
    settle_hunt(herd, &party, workers, carry, fauna)
}

/// The herd's own sustainable line, in food/turn, under `fauna`.
fn herd_line(herd: &Herd, fauna: &FaunaConfig) -> f32 {
    sustainable_yield(
        herd.biomass,
        herd.carrying_capacity,
        &herd_ecology(herd, fauna),
    ) * fauna.hunt_yield_for(&herd.species).provisions_per_biomass
}

/// **Milk / eggs at a full standing commitment**, food/turn averaged over the measured window: the
/// herd is taken from not at all (`f = 1`), so it rides up to `K` and its standing yield is what
/// comes in.
fn standing_at_full(herd: &Herd, fauna: &FaunaConfig) -> f32 {
    let mut kept = herd.clone();
    kept.standing_output_fraction = FULL_STANDING_COMMITMENT;
    let mut total = 0.0_f32;
    for turn in 0..WARMUP_TURNS + MEASURED_TURNS {
        regrow_biomass(&mut kept, fauna);
        if turn >= WARMUP_TURNS {
            total += herd_standing_provisions(&kept, fauna);
        }
    }
    total / MEASURED_TURNS as f32
}

fn sorted_species(s: &Shipped) -> Vec<(&String, &SpeciesDef)> {
    let mut rows: Vec<(&String, &SpeciesDef)> = s.now.species.iter().collect();
    rows.sort_by(|a, b| a.0.cmp(b.0));
    rows
}

fn bound_name(bound: Option<HuntTakeBound>) -> &'static str {
    bound.map_or("-", |bound| bound.as_str())
}

fn upkeep_work_per_turn(rung: RungKey, s: &Shipped) -> f32 {
    s.ladder
        .rung(rung)
        .upkeep
        .as_ref()
        .map_or(0.0, |upkeep| upkeep.work_per_turn)
}

/// One managed-herd row: the numbers under `now` and under `was`.
struct HerdRow {
    per_worker: Vec<(f32, f32)>,
    keepers: (f32, f32),
    with_keepers: Vec<(f32, f32)>,
    standing: (f32, f32),
    line: (f32, f32),
    crew_to_line: (String, String),
    tiles: usize,
    binds: &'static str,
}

fn managed_row(
    key: &str,
    def: &SpeciesDef,
    rung: RungKey,
    corralled: bool,
    s: &Shipped,
) -> Option<HerdRow> {
    let now_herd = seated_herd(key, def, rung, &s.now, s)?;
    let was_herd = seated_herd(key, def, rung, &s.was, s)?;
    let kit = herd_default_hunt_kit(&s.equipment, s.creatures.person(), def, corralled);
    let keepers_now = keepers_owed(herd_upkeep_demand(&now_herd, &s.now, &s.ladder));
    let keepers_was = keepers_owed(herd_upkeep_demand(&was_herd, &s.was, &s.ladder));
    let mut per_worker = Vec::new();
    let mut with = Vec::new();
    for workers in RUNG_CREWS {
        let now = hunt_with(&now_herd, &kit, workers, &s.now, s).food_per_worker;
        let was = hunt_with(&was_herd, &kit, workers, &s.was, s).food_per_worker;
        per_worker.push((now, was));
        with.push((
            with_keepers(now, workers, keepers_now),
            with_keepers(was, workers, keepers_was),
        ));
    }
    let line_now = herd_line(&now_herd, &s.now);
    let line_was = herd_line(&was_herd, &s.was);
    Some(HerdRow {
        per_worker,
        keepers: (keepers_now, keepers_was),
        with_keepers: with,
        standing: (
            standing_at_full(&now_herd, &s.now),
            standing_at_full(&was_herd, &s.was),
        ),
        line: (line_now, line_was),
        crew_to_line: (
            crew_to_line(line_now, |crew| {
                hunt_with(&now_herd, &kit, crew, &s.now, s).food_per_worker * crew as f32
            }),
            crew_to_line(line_was, |crew| {
                hunt_with(&was_herd, &kit, crew, &s.was, s).food_per_worker * crew as f32
            }),
        ),
        tiles: footprint_tiles(&now_herd, def),
        binds: bound_name(hunt_with(&now_herd, &kit, BOUND_CREW, &s.now, s).modal_bound),
    })
}

// ---------------------------------------------------------------------------------------------
// The survey
// ---------------------------------------------------------------------------------------------

#[test]
#[ignore = "a printing probe: run with --ignored --nocapture"]
fn food_rate_survey() {
    let s = Shipped::load();
    let mut every_cell: Vec<f32> = Vec::new();
    println!(
        "\nFOOD RATE SURVEY — rung pairs, settled over turns {WARMUP_TURNS}..{} ({MEASURED_TURNS} averaged); \
         cell = food/worker-turn (x upkeep, upkeep = per_capita_draw {:.2}) ← was; stocked band; floor {:.2}",
        WARMUP_TURNS + MEASURED_TURNS,
        s.per_capita_draw,
        DEFAULT_ESCAPEMENT_FLOOR,
    );

    // ================================ RUNG 1 — WILD ================================
    let stalking = s.kit(STALKING_KIT);
    let trapping = s.kit(TRAPPING_KIT);
    let bare = s.equipment.no_kit();
    println!("\n## RUNG 1 — WILD: hunt (every species, Stalking and Trapping on every one)\n");
    println!("| species | Stalking 1 | Stalking 3 | Stalking 5 | Trapping 1 | Trapping 3 | Trapping 5 | bare 3 (info) |");
    println!("|---|---|---|---|---|---|---|---|");
    for (key, def) in sorted_species(&s) {
        let now = seated_herd(key, def, RungKey::AnimalWild, &s.now, &s).expect("wild rung");
        let was = seated_herd(key, def, RungKey::AnimalWild, &s.was, &s).expect("wild rung");
        let mut cells = Vec::new();
        for kit in [&stalking, &trapping] {
            for workers in CREWS {
                let n = hunt_with(&now, kit, workers, &s.now, &s).food_per_worker;
                let w = hunt_with(&was, kit, workers, &s.was, &s).food_per_worker;
                every_cell.push(n);
                cells.push(s.pair(n, w));
            }
        }
        let bare_now = hunt_with(&now, &bare, BOUND_CREW, &s.now, &s).food_per_worker;
        println!(
            "| {} | {} | {} |",
            def.display_name,
            cells.join(" | "),
            s.cell(bare_now)
        );
    }
    println!(
        "\n### Rung 1 hunt — scaling limits (one typical herd, K = roster full-group biomass)\n"
    );
    println!("| species | range tiles | line food/turn | line per tile | Stalking crew → 95% of line | Trapping crew → 95% of line | binds @3 (Stalking) | settled B/K @ best Stalking crew |");
    println!("|---|---|---|---|---|---|---|---|");
    for (key, def) in sorted_species(&s) {
        let now = seated_herd(key, def, RungKey::AnimalWild, &s.now, &s).expect("wild rung");
        let was = seated_herd(key, def, RungKey::AnimalWild, &s.was, &s).expect("wild rung");
        let line = herd_line(&now, &s.now);
        let line_was = herd_line(&was, &s.was);
        let tiles = footprint_tiles(&now, def);
        let crew_for = |kit: &KitChoice| {
            crew_to_line(line, |crew| {
                hunt_with(&now, kit, crew, &s.now, &s).food_per_worker * crew as f32
            })
        };
        let best_stock = sweep_crews()
            .into_iter()
            .map(|crew| hunt_with(&now, &stalking, crew, &s.now, &s))
            .max_by(|a, b| a.food_per_worker.total_cmp(&b.food_per_worker))
            .map_or(0.0, |settled| settled.stock_over_k);
        println!(
            "| {} | {tiles} | {line:.3} ← {line_was:.3} | {:.3} | {} | {} | {} | {best_stock:.2} |",
            def.display_name,
            line / tiles as f32,
            crew_for(&stalking),
            crew_for(&trapping),
            bound_name(hunt_with(&now, &stalking, BOUND_CREW, &s.now, &s).modal_bound),
        );
    }

    println!("\n## RUNG 1 — WILD: forage (whole realized basket; flora is untouched by the trial, so no \"was\")\n");
    println!("| terrain | K | bare 1 | bare 3 | bare 5 | basket 1 | basket 3 | basket 5 | line food/turn (1 tile) | basket crew → 95% of line | binds @3 |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|");
    let mut forage_rows: Vec<(TerrainType, f32, Vec<f32>, PlantRow)> = forage_terrains(&s)
        .into_iter()
        .map(|terrain| {
            let composition = basket_on(terrain, &s);
            let patch = seated_patch(terrain, None, 1.0, &s);
            let bare_food: Vec<f32> = CREWS
                .iter()
                .map(|workers| {
                    let carry = forage_carry(&gear(bare.clone(), *workers, &s), &s);
                    settle_forage(&patch, &composition, *workers, carry, &s)
                })
                .collect();
            (
                terrain,
                patch.carrying_capacity,
                bare_food,
                plant_row(terrain, None, &CREWS, &s),
            )
        })
        .collect();
    forage_rows.sort_by(|a, b| b.3.per_worker[0].total_cmp(&a.3.per_worker[0]));
    for (terrain, k, bare_food, row) in forage_rows.iter().take(FORAGE_ROWS_SHOWN) {
        every_cell.extend(row.per_worker.iter().copied());
        let bare_cells: Vec<String> = bare_food.iter().map(|food| s.cell(*food)).collect();
        let basket_cells: Vec<String> = row.per_worker.iter().map(|food| s.cell(*food)).collect();
        println!(
            "| {terrain:?} | {k:.0} | {} | {} | {:.3} | {} | {} |",
            bare_cells.join(" | "),
            basket_cells.join(" | "),
            row.line,
            row.crew_to_line,
            row.binds,
        );
    }
    let tail = &forage_rows[FORAGE_ROWS_SHOWN.min(forage_rows.len())..];
    if !tail.is_empty() {
        let low = tail
            .iter()
            .map(|row| row.3.per_worker[0])
            .fold(f32::INFINITY, f32::min);
        let high = tail
            .iter()
            .map(|row| row.3.per_worker[0])
            .fold(0.0, f32::max);
        let k_low = tail.iter().map(|row| row.1).fold(f32::INFINITY, f32::min);
        let k_high = tail.iter().map(|row| row.1).fold(0.0, f32::max);
        println!(
            "\nTail — {} more terrains, K {k_low:.0}–{k_high:.0}: basket 1 worker {low:.3}–{high:.3} ({:.2}x–{:.2}x); all stand-bound past 1 worker.",
            tail.len(),
            low / s.per_capita_draw,
            high / s.per_capita_draw,
        );
    }

    // ================================ RUNG 2 — TAME vs TENDED ================================
    let pastoral_upkeep = upkeep_work_per_turn(RungKey::AnimalPastoral, &s);
    println!("\n## RUNG 2 — TAME (pastoral, the herd's own default kit, keeping fully supplied)\n");
    println!("| species | 3 workers | 5 workers | keepers owed | 3 + keepers | 5 + keepers | milk @ f=1 food/turn (per keeper) | line food/turn | crew → 95% of line | range tiles | line per tile | binds @3 | multipliers over wild |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    for (key, def) in sorted_species(&s) {
        let Some(row) = managed_row(key, def, RungKey::AnimalPastoral, false, &s) else {
            continue;
        };
        print_herd_row(def, &row, &s, &mut every_cell, |display| {
            format!(
                "r×{:.2} ← {:.2} (`husbandry.pastoral_gain`) · K×{:.2} (`pastoral_density`) · reach×{:.2} (`pastoral_engage_gain`) · retreat×{:.2} (`pastoral_wariness`) · fight×{:.2} (`pastoral_resistance`) · aph {:.0} ← {:.0} · upkeep {:.2}/load",
                s.now.husbandry.pastoral_gain,
                s.was.husbandry.pastoral_gain,
                s.now.pastoral_density_for(display),
                s.now.pastoral_engage_gain_for(display),
                s.now.husbandry.pastoral_wariness,
                s.now.pastoral_resistance_for(display),
                s.now.animals_per_herder_for(display),
                s.was.animals_per_herder_for(display),
                pastoral_upkeep,
            )
        });
    }
    let tended_upkeep = upkeep_work_per_turn(RungKey::PlantTended, &s);
    let cultivation = &s.labor.forage.cultivation;
    println!("\n## RUNG 2 — TENDED (Harvesting kit, rung FORCED, best food crop in the basket committed)\n");
    println!("| terrain | crop | K | 3 workers | 5 workers | keepers owed | 3 + keepers | 5 + keepers | line food/turn (1 tile) | crew → 95% of line | binds @3 |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|");
    for terrain in forage_terrains(&s) {
        print_plant_rung(terrain, RungKey::PlantTended, &s, &mut every_cell);
    }
    println!(
        "\nTended multipliers over wild: r×{:.2} (`cultivation.tended_regrowth_gain`) · committed-crop share ×{:.2} (`tended_weeding_gain`) · conversion ×{:.2} (`tended_conversion_gain`) · K unchanged · upkeep {:.2} work/turn per `capacity_per_tender` {:.0} (`plant:tended` upkeep)",
        cultivation.tended_regrowth_gain,
        cultivation.tended_weeding_gain,
        cultivation.tended_conversion_gain,
        tended_upkeep,
        cultivation.capacity_per_tender,
    );

    // ================================ RUNG 3 — PEN vs FIELD ================================
    let pen_upkeep = upkeep_work_per_turn(RungKey::AnimalPen, &s);
    println!("\n## RUNG 3 — PEN (the herd's own default kit, keeping supplied, pen assumed fed)\n");
    println!("| species | 3 workers | 5 workers | keepers owed | 3 + keepers | 5 + keepers | milk/eggs @ f=1 food/turn (per keeper) | line food/turn | crew → 95% of line | pen tiles | line per tile | binds @3 | multipliers over pastoral |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|---|---|");
    for (key, def) in sorted_species(&s) {
        let Some(row) = managed_row(key, def, RungKey::AnimalPen, true, &s) else {
            continue;
        };
        print_herd_row(def, &row, &s, &mut every_cell, |display| {
            format!(
                "r×{:.2} ← {:.2} (`husbandry.pen_gain`) · K×{:.2} (`pen_density`) · reach×{:.2} (`pen_engage_gain`) · retreat×{:.2} (`pen_wariness`) · no fight · carry unbounded (`pen_is_a_larder` {}) · aph {:.0} ← {:.0} · upkeep {:.2}/load",
                s.now.husbandry.pen_gain,
                s.was.husbandry.pen_gain,
                s.now.pen_density_for(display),
                s.now.pen_engage_gain_for(display),
                s.now.husbandry.pen_wariness,
                s.now.husbandry.pen_is_a_larder,
                s.now.animals_per_herder_for(display),
                s.was.animals_per_herder_for(display),
                pen_upkeep,
            )
        });
    }
    let field_upkeep = upkeep_work_per_turn(RungKey::PlantField, &s);
    println!("\n## RUNG 3 — FIELD (Harvesting kit, rung FORCED, best field crop in the basket committed)\n");
    println!("| terrain | crop | K | 3 workers | 5 workers | keepers owed | 3 + keepers | 5 + keepers | line food/turn (1 tile) | crew → 95% of line | binds @3 |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|");
    for terrain in forage_terrains(&s) {
        print_plant_rung(terrain, RungKey::PlantField, &s, &mut every_cell);
    }
    println!(
        "\nField multipliers over tended: K×{:.2} (`field_capacity_gain`) · r×{:.2} (`field_regrowth_gain`) · conversion ×{:.2} (`field_conversion_gain`) · upkeep {:.2} work/turn per `capacity_per_tender` (`plant:field` upkeep)",
        cultivation.field_capacity_gain,
        cultivation.field_regrowth_gain,
        cultivation.field_conversion_gain,
        field_upkeep,
    );
    println!(
        "\nMeat conversion (every animal row): `hunt.provisions_per_biomass` {:.3} ← {:.3}. Regrowth cap `husbandry_regrowth_cap` {:.2}.",
        s.now.hunt.provisions_per_biomass,
        s.was.hunt.provisions_per_biomass,
        s.now.husbandry.husbandry_regrowth_cap,
    );

    // Liveness: a table of zeros that exits 0 would say nothing.
    assert!(
        every_cell.iter().all(|food| food.is_finite()),
        "every cell must be finite"
    );
    assert!(
        every_cell.iter().any(|food| *food > 0.0),
        "the survey must measure something positive"
    );
}

/// Print one managed-herd row.
fn print_herd_row(
    def: &SpeciesDef,
    row: &HerdRow,
    s: &Shipped,
    every_cell: &mut Vec<f32>,
    multipliers: impl Fn(&str) -> String,
) {
    let food: Vec<String> = row
        .per_worker
        .iter()
        .map(|(now, was)| {
            every_cell.push(*now);
            s.pair(*now, *was)
        })
        .collect();
    let keeping: Vec<String> = row
        .with_keepers
        .iter()
        .map(|(now, was)| s.pair(*now, *was))
        .collect();
    let per_keeper = |standing: f32, keepers: f32| {
        if keepers > 0.0 {
            standing / keepers
        } else {
            0.0
        }
    };
    println!(
        "| {} | {} | {:.2} ← {:.2} | {} | {:.3} ({:.3}) ← {:.3} ({:.3}) | {:.3} ← {:.3} | {} ← {} | {} | {:.3} | {} | {} |",
        def.display_name,
        food.join(" | "),
        row.keepers.0,
        row.keepers.1,
        keeping.join(" | "),
        row.standing.0,
        per_keeper(row.standing.0, row.keepers.0),
        row.standing.1,
        per_keeper(row.standing.1, row.keepers.1),
        row.line.0,
        row.line.1,
        row.crew_to_line.0,
        row.crew_to_line.1,
        row.tiles,
        row.line.0 / row.tiles as f32,
        row.binds,
        multipliers(&def.display_name),
    );
}

/// Print one plant rung row for `terrain`, when its basket carries a crop the rung admits.
fn print_plant_rung(terrain: TerrainType, rung: RungKey, s: &Shipped, every_cell: &mut Vec<f32>) {
    let composition = basket_on(terrain, s);
    let Some(crop) = best_crop(&composition, rung, s) else {
        return;
    };
    let patch = seated_patch(terrain, Some((&crop, rung)), 1.0, s);
    let keepers = keepers_owed(patch_upkeep_demand(
        &patch,
        &s.ladder,
        s.labor.forage.capacity_for(terrain),
        &s.labor.forage,
    ));
    let row = plant_row(terrain, Some((&crop, rung)), &RUNG_CREWS, s);
    every_cell.extend(row.per_worker.iter().copied());
    let food: Vec<String> = row.per_worker.iter().map(|food| s.cell(*food)).collect();
    let with: Vec<String> = row
        .per_worker
        .iter()
        .zip(RUNG_CREWS)
        .map(|(food, crew)| s.cell(with_keepers(*food, crew, keepers)))
        .collect();
    println!(
        "| {terrain:?} | {crop} | {:.0} | {} | {keepers:.2} | {} | {:.3} ({:.3}/tile) | {} | {} |",
        patch.carrying_capacity,
        food.join(" | "),
        with.join(" | "),
        row.line,
        row.line / PATCH_TILES as f32,
        row.crew_to_line,
        row.binds,
    );
}
