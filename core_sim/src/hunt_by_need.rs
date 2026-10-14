//! **HUNTING BY NEED** (`docs/plan_roaming_bands.md` §Hunting by need, #798): a band in migration
//! mode hunts when it needs meat, not at a fixed rate.
//!
//! A hunt row with `move_with_herd` is a *need row*. It holds no standing workers; each turn the
//! labor pass decides whether the band sends a crew to it NEXT turn ([`assess_need`]), mustered from
//! the band's own hands for that one turn ([`crate::LaborAllocation::muster_donors`]) and handed
//! back at the end of the pass. The same assessment is what the snapshot capture publishes from, so
//! the board and the turn cannot disagree.

use crate::components::LaborTarget;
use crate::components::{BandEquipment, LaborAllocation, MusterKit, PopulationCohort};
use crate::demographics_config::DemographicsConfig;
use crate::equipment_config::{EquipmentConfig, KitChoice};
use crate::fauna::{
    herd_food_per_animal, hunt_crew_for_animals, hunt_crew_take_curve, hunt_crew_turns_to_kill,
    hunt_useful_crew, HuntCrewCurveInputs, NO_USEFUL_CREW,
};
use crate::fauna_config::FaunaConfig;
use crate::snapshot::{kill_food_terms, kill_turns_of_food, BandFoodTerms, NOT_FOOD_LIMITED_TURNS};

/// The wire's `turnsUntilHunt` when a crew will not be sent however long the band waits: the larder
/// is not food-limited, or no crew can be raised. `u32::MAX` rather than `0` because `0` already
/// means *a crew goes out now*.
pub const NO_HUNT_NEEDED: u32 = u32::MAX;

/// The least `turnsUntilHunt` a waiting row publishes: it is waiting, so the crew is at least a turn
/// away.
const MIN_TURNS_UNTIL_HUNT: u32 = 1;

/// **IS THIS SOURCE WITHIN THE BAND'S WORK RANGE?** — the geometric test a donor row must pass, on
/// the one threshold ([`crate::work_party::party_begins_past`]) `post_a_party` posts a party past. A
/// source the band cannot place (a role row, a herd gone, no band position) is not local.
/// `grid` is `(width, wrap_horizontal)`.
pub(crate) fn source_is_local(
    target: &crate::LaborTarget,
    herds: &crate::HerdRegistry,
    band_pos: Option<bevy::math::UVec2>,
    labor: &crate::LaborConfig,
    grid: (u32, bool),
) -> bool {
    let (Some(band), Some(source)) = (
        band_pos,
        crate::systems::party_source_position(target, herds),
    ) else {
        return false;
    };
    crate::grid_utils::hex_distance_wrapped(band, source, grid.0, grid.1)
        <= crate::work_party::party_begins_past(labor)
}

/// The fewest animals a hunt can bring down: an animal is indivisible, so even a meal of nothing is
/// one whole animal.
const MIN_ANIMALS: f32 = 1.0;

/// **THE MEAL A HUNT IS SIZED TO** — what the band can eat from one kill before the meat spoils,
/// less what its larder already holds, and what one animal of the herd is worth.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NeedMeal {
    /// Food the band cannot cover otherwise before this kill's meat spoils ([`meal_food`]).
    pub food: f32,
    /// Food in one whole animal of the herd (`body_mass × provisions_per_biomass`).
    pub food_per_animal: f32,
}

impl NeedMeal {
    /// **Whole animals the meal calls for** — `max(1, ceil(food / food_per_animal))`. A herd that
    /// pays no food (or a meal that never ends because the class never spoils) calls for more than
    /// any crew lands, which leaves the crew at the useful one.
    pub fn animals(&self) -> f32 {
        if self.food_per_animal <= 0.0 || !self.food.is_finite() {
            return f32::INFINITY;
        }
        (self.food / self.food_per_animal).ceil().max(MIN_ANIMALS)
    }
}

/// **THE MEAL, IN FOOD** — `max(0, net_eat × keeps − usable_larder)`: what the band eats beyond its
/// own income over the `keeps_turns` the meat lasts, less the larder it can already count on.
/// `keeps_turns` is [`crate::spoilage::band_shelf_life`] of the class the kill lands in, so the
/// meal and the larder's rot are one rule. `None` (a class that never rots) is an unending meal.
pub(crate) fn meal_food(terms: &BandFoodTerms, keeps_turns: Option<f32>) -> f32 {
    let Some(keeps) = keeps_turns else {
        return f32::INFINITY;
    };
    let net_eat = (terms.demand.to_f32() - terms.steady_income).max(0.0);
    (net_eat * keeps - terms.usable_larder).max(0.0)
}

/// The meal for each configured keeping class, in the table's order — what the wire publishes as
/// `PopulationCohortState.huntMeals`, so a compose sheet sizing a crew before the row exists reads
/// the very figure the turn will. Each class's meal is struck over the food that class's kill
/// replaces ([`kill_food_terms`]), the same larder the turn sizes the crew against.
pub(crate) fn meals_by_class(
    cohort: &PopulationCohort,
    allocation: Option<&LaborAllocation>,
    demographics: &DemographicsConfig,
    is_expedition: bool,
) -> Vec<(String, f32)> {
    demographics
        .keeping
        .classes
        .iter()
        .map(|class| {
            let terms = kill_food_terms(cohort, allocation, demographics, is_expedition, &class.id);
            (
                class.id.clone(),
                meal_food(
                    &terms,
                    crate::spoilage::band_shelf_life(&demographics.keeping, &class.id),
                ),
            )
        })
        .collect()
}

/// **THE NEED CLOCK FOR ONE HERD** — the meal its kill calls for and the runway the trigger reads.
/// Both are struck over **the food that keeps no longer than this herd's kill does**
/// ([`crate::spoilage::keeps_no_longer_than`]): the band's dried reserve is not what a kill
/// replaces, so a band arriving with plenty of `dry` and no meat hunts at once, and after a kill it
/// hunts again as the meat runs low rather than burning the reserve to its last turn. When drying
/// (#708) lengthens the kill's shelf life the longer classes join the count with no new lever. Net
/// eating — demand less steady income — is unchanged. The turn and the snapshot both call this.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct NeedClock {
    pub meal: NeedMeal,
    /// Turns the meat on hand lasts ([`crate::snapshot::kill_turns_of_food`]); the sentinel
    /// [`NOT_FOOD_LIMITED_TURNS`] when income covers the people.
    pub runway: f32,
}

pub(crate) fn need_clock(
    cohort: &PopulationCohort,
    allocation: Option<&LaborAllocation>,
    demographics: &DemographicsConfig,
    is_expedition: bool,
    herd: &crate::fauna::Herd,
    fauna: &FaunaConfig,
) -> NeedClock {
    let keeping = &demographics.keeping;
    let class = fauna
        .keeping_for(&herd.species)
        .unwrap_or(&keeping.kill_fallback_class);
    let terms = kill_food_terms(cohort, allocation, demographics, is_expedition, class);
    NeedClock {
        meal: NeedMeal {
            food: meal_food(&terms, crate::spoilage::band_shelf_life(keeping, class)),
            food_per_animal: herd_food_per_animal(herd, fauna),
        },
        runway: kill_turns_of_food(cohort, allocation, demographics, is_expedition, class),
    }
}

/// The `need_idx` of a hunt no row stands on yet: the Assign hunters sheet pricing a crew before the
/// row exists. No donor is excluded as "the need row", and no claim is zeroed for it.
pub const NO_NEED_ROW: usize = usize::MAX;

/// **THE HUNT'S TAKE KIT AGAINST THE BAND'S GEAR, FOR A CREW OF `crew`** - whether the kit is short
/// and which donor rows are holding it, struck from the same item budget the turn settles the
/// band's gear with ([`LaborAllocation::item_budget`], on `claims`, the rows' standing claims) so
/// there is one count of who holds what.
///
/// An item of `hunt_kit` is **short** when the units no row claims
/// ([`crate::equipment_config::BandItemBudget::unclaimed_units`]) arm fewer than `crew` hands. The
/// shortfall is the largest gap over the short items; a donor row is *holding* the kit when it
/// carries a short item and was settled units of it - and the hands it holds are what lending it
/// frees. A kit nothing is short of returns the default, and the donor order stays Priority alone.
pub(crate) fn muster_kit(
    allocation: &LaborAllocation,
    need_idx: usize,
    crew: u32,
    hunt_kit: &KitChoice,
    claims: &[f32],
    equipment: &EquipmentConfig,
    wear: &BandEquipment,
) -> MusterKit {
    let budget = allocation.item_budget(equipment, claims);
    let per_unit = |item: &str| equipment.item(item).map_or(1, |def| def.workers_per_unit) as f32;
    let short_items: Vec<(&str, u32)> = hunt_kit
        .uses()
        .filter_map(|item| {
            let free_hands =
                (budget.unclaimed_units(item, wear, equipment) * per_unit(item)).floor();
            let gap = crew as f32 - free_hands;
            (gap > 0.0).then_some((item, gap as u32))
        })
        .collect();
    let Some(shortfall) = short_items.iter().map(|(_, gap)| *gap).max() else {
        return MusterKit::default();
    };
    let carried = allocation
        .assignments
        .iter()
        .enumerate()
        .map(|(idx, row)| {
            if idx == need_idx || row.is_need_row() {
                return 0;
            }
            let row_kit = row.kit_choice(equipment);
            let at = budget.row_of(&row.target);
            short_items
                .iter()
                .filter(|(item, _)| row_kit.uses().any(|used| used == *item))
                .map(|(item, _)| {
                    (budget.units_for_row(item, at, wear, equipment) * per_unit(item)).floor()
                        as u32
                })
                .max()
                .unwrap_or(0)
                .min(allocation.hands_present_on(row))
        })
        .collect();
    MusterKit { shortfall, carried }
}

/// **THE GEAR THE MUSTER BRINGS AT A CREW OF `c`** - the one function the crew-take curve, the
/// need assessment (useful crew, turns to kill, meal-sized crew), the hunt-turn craft pass and the
/// Assign hunters sheet price a need crew against, so they cannot disagree about it.
///
/// A need row holds no hands, so the rows beside it still hold their kit at the standing claims: a
/// curve priced over the **free** units alone arms nobody on a band whose stalking kits are all
/// claimed by its boar-hunt rows, and a quarry bare hands cannot wound (Marsh Grazers, defense 3)
/// prices at zero - the hunt would never go out. A boar hunter lent to the hunt brings his kit, so
/// the crew is priced over the free units **plus the units the lent donors hold**, in the order
/// [`LaborAllocation::muster_donors`] takes them. There is no fixed point: the plan is a function of
/// the crew size and the *standing* claims alone, and the donors' reduced claims are derived from
/// it, not fed back into it.
pub struct MusterPricing<'a> {
    /// The band's allocation as it stands (donors at full strength, the need row at nothing).
    pub allocation: &'a LaborAllocation,
    /// The hunt source being priced - `rows_excluding_source` drops any row already on it.
    pub target: &'a LaborTarget,
    /// The need row's index, or [`NO_NEED_ROW`] before one exists.
    pub need_idx: usize,
    /// Idle hands, mustered before any donor.
    pub idle: u32,
    /// Whether a source is within the band's work range (donors must be).
    pub is_local: &'a dyn Fn(&LaborTarget) -> bool,
    /// The crafting bench as a donor beside the rows (`None` = no bench, or nobody at it).
    pub bench: Option<crate::components::MusterBench>,
    /// Every row's standing take-kit claim, index-aligned with the allocation.
    pub claims: &'a [f32],
}

impl MusterPricing<'_> {
    /// The standing claims with the hunt source's own row (if any) at nothing: it is the row being
    /// priced, not a competitor for its own gear.
    fn standing_claims(&self) -> Vec<f32> {
        self.allocation
            .assignments
            .iter()
            .zip(self.claims)
            .map(|(row, claim)| {
                if row.target.same_source(self.target) {
                    0.0
                } else {
                    *claim
                }
            })
            .collect()
    }

    /// Who a crew of `crew` is made of, kit-aware.
    pub fn plan_at(
        &self,
        crew: u32,
        hunt_kit: &KitChoice,
        equipment: &EquipmentConfig,
        wear: &BandEquipment,
    ) -> crate::components::MusterPlan {
        let standing = self.standing_claims();
        let kit = muster_kit(
            self.allocation,
            self.need_idx,
            crew,
            hunt_kit,
            &standing,
            equipment,
            wear,
        );
        self.allocation.muster_donors(
            self.idle,
            self.need_idx,
            crew,
            self.is_local,
            self.bench,
            &kit,
        )
    }

    /// **The rows a crew of `crew` competes with for the band's gear** once the donors it musters
    /// have given their hands up: each lending row's claim falls in proportion to the hands lent, so
    /// the kit they held is free for the crew.
    pub fn other_rows_at(
        &self,
        crew: u32,
        hunt_kit: &KitChoice,
        equipment: &EquipmentConfig,
        wear: &BandEquipment,
    ) -> Vec<crate::equipment_config::KittedRow> {
        let plan = self.plan_at(crew, hunt_kit, equipment, wear);
        let mut claims = self.standing_claims();
        for (row, lent) in &plan.from_rows {
            let held = self.allocation.hands_present(*row);
            if held > 0 {
                claims[*row] *= (held - lent) as f32 / held as f32;
            }
        }
        self.allocation
            .rows_excluding_source(equipment, self.target, &claims)
    }
}

/// What the band decides about one need row for next turn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NeedAssessment {
    /// The smallest crew of the curve that brings down the meal's animals, never above
    /// [`hunt_useful_crew`] (the crew itself where no crew lands that many); priced over the
    /// musterable pool; `0` when none can be raised.
    pub crew: u32,
    /// Turns until that crew brings an animal down, counting the herd's current wounds; `0` when no
    /// crew can be raised.
    pub turns_to_kill: u32,
    /// Whether the band sends the crew next turn.
    pub send: bool,
    /// On a waiting row, turns until a crew goes out if nothing changes ([`NO_HUNT_NEEDED`] when it
    /// never will); `0` on a row that is sending.
    pub turns_until_hunt: u32,
}

impl NeedAssessment {
    /// No crew can be raised: nothing is sent and nothing is waited for.
    const NO_CREW: Self = Self {
        crew: NO_USEFUL_CREW,
        turns_to_kill: 0,
        send: false,
        turns_until_hunt: NO_HUNT_NEEDED,
    };
}

/// **THE NEED DECISION FOR ONE ROW** — `inputs` is the hunt crew curve's own input bundle with
/// `max_workers` set to the musterable pool; `runway` is the band's food runway, the very value the
/// snapshot publishes as `turnsOfFood`; `margin` is `follow.need_margin_turns`.
///
/// * the crew that would go out is sized to the meal ([`hunt_crew_for_animals`]): the smallest that
///   lands the animals the band can eat before the meat spoils, at least one, never above
///   [`hunt_useful_crew`]; none → nothing is sent;
/// * an animal **already partly down** is finished (`finishing`, below);
/// * otherwise a crew goes out iff `runway <= turns_to_kill + margin`, and a runway at the
///   [`NOT_FOOD_LIMITED_TURNS`] sentinel never triggers it.
///
/// # ⛔ `finishing` IS "THE CREW WAS OUT THIS TURN, THE HERD HAS WOUNDS PENDING, AND NO KILL LANDED"
///
/// It is not simply `wounds.pending() > 0`. A crew that kills a mammoth deals more damage than the
/// body holds, and the excess banks toward the NEXT animal (`DamageLedger::strike`), so a herd is
/// left with a small `pending` after almost every kill. Read as "the animal is partly down" that
/// remainder would send a full crew after the next mammoth the turn after each kill, whatever the
/// larder said, for ever. The rule that means what it says is *a hunt in progress*: the band sent a
/// crew this turn, the animal took damage, and it is not down yet. The caller states that fact; this
/// function only obeys it.
pub fn assess_need(
    inputs: &HuntCrewCurveInputs<'_>,
    horizon: u32,
    runway: f32,
    margin: f32,
    finishing: bool,
    meal: &NeedMeal,
) -> NeedAssessment {
    let curve = hunt_crew_take_curve(inputs);
    let useful = hunt_useful_crew(&curve);
    if useful == NO_USEFUL_CREW {
        return NeedAssessment::NO_CREW;
    }
    let crew = hunt_crew_for_animals(&curve, meal.animals(), useful);
    let turns_to_kill = hunt_crew_turns_to_kill(inputs, crew, horizon);
    let reported_turns = turns_to_kill.unwrap_or(0);
    if finishing {
        return NeedAssessment {
            crew,
            turns_to_kill: reported_turns,
            send: true,
            turns_until_hunt: 0,
        };
    }
    let food_limited = runway < NOT_FOOD_LIMITED_TURNS;
    let Some(turns) = turns_to_kill.filter(|_| food_limited) else {
        return NeedAssessment {
            crew,
            turns_to_kill: reported_turns,
            send: false,
            turns_until_hunt: NO_HUNT_NEEDED,
        };
    };
    let slack = runway - turns as f32 - margin;
    if slack <= 0.0 {
        return NeedAssessment {
            crew,
            turns_to_kill: turns,
            send: true,
            turns_until_hunt: 0,
        };
    }
    NeedAssessment {
        crew,
        turns_to_kill: turns,
        send: false,
        turns_until_hunt: (slack.ceil() as u32).max(MIN_TURNS_UNTIL_HUNT),
    }
}
