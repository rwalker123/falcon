//! **Band fission — a band splits in two where it stands** (`docs/plan_band_fission.md`, issue #511).
//!
//! The whole verb is *form a band, then move it*. The new band is a [`ResidentBand`] from the moment
//! the command resolves: it eats, ages, forages, births and starves on the same systems as every
//! other band, on its first turn. There is no party, no walk, no arrival and no second decision.
//!
//! **The player's one input is a worker count**, and every other quantity divides on the share that
//! implies (`docs/plan_band_fission.md` §Q3). That is the model, not an economy on top of it — see
//! [`split_band_from_parent`] for why a per-bracket choice is the thing being avoided.

use bevy::prelude::*;

use crate::components::{
    available_workers, BandEquipment, BandId, DemographicFlowAccumulator, LaborAllocation,
    LocalStore, MoraleCause, MoraleContributions, PopulationCohort, ResidentBand, StartingUnit,
    Tile,
};
use crate::culture::CultureManager;
use crate::expedition_config::SettleConfig;
use crate::provinces::ProvinceMap;
use crate::resources::BandIdAllocator;
use crate::scalar::{scalar_from_f32, scalar_zero, Scalar};

/// **Why a split was refused.**
///
/// **A refusal refuses the SPLIT, never the band.** Every arm leaves the parent exactly as it stood,
/// so nothing the player has invested is lost by asking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitRefusal {
    /// The named entity is not a resident band — an expedition, or nothing at all.
    NotAResidentBand,
    /// Nobody was asked to go. A band of no workers is not a band.
    EmptySplit,
    /// The parent does not have that many workers to give.
    NotEnoughWorkers { asked: u32, available: u32 },
    /// The **new** band would open below [`SettleConfig::min_founding_workers`].
    NewBandTooSmall { workers: u32, required: u32 },
    /// The **parent** would be left below [`SettleConfig::parent_min_workers`].
    ParentTooSmall { remaining: u32, required: u32 },
}

impl SplitRefusal {
    /// The stable log token (`.claude/rules/core_sim/event-feed.md` — tokens are matched, prose is
    /// read).
    pub fn token(&self) -> &'static str {
        match self {
            SplitRefusal::NotAResidentBand => "not_a_resident_band",
            SplitRefusal::EmptySplit => "empty_split",
            SplitRefusal::NotEnoughWorkers { .. } => "not_enough_workers",
            SplitRefusal::NewBandTooSmall { .. } => "new_band_too_small",
            SplitRefusal::ParentTooSmall { .. } => "parent_too_small",
        }
    }

    /// A lowercase sentence fragment, so refusals join into one line without a capital mid-sentence.
    pub fn explanation(&self) -> String {
        match self {
            SplitRefusal::NotAResidentBand => "that is not a band that can split.".to_string(),
            SplitRefusal::EmptySplit => "a new band needs workers to be a band.".to_string(),
            SplitRefusal::NotEnoughWorkers { asked, available } => {
                format!("it has {available} workers to give and {asked} were asked for.")
            }
            SplitRefusal::NewBandTooSmall { workers, required } => format!(
                "a new band starts with {required} workers and this one would have {workers}."
            ),
            SplitRefusal::ParentTooSmall {
                remaining,
                required,
            } => format!(
                "the home band would keep {remaining} workers, below its floor of {required}."
            ),
        }
    }
}

/// Every applicable refusal, in a stable order.
///
/// **All of them, never the first one.** A split that is both too small and leaves the parent short
/// has two things to fix, and reporting one at a time teaches the rules one refusal at a time — the
/// player fixes it, presses again, and discovers the next.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SplitRefusals(Vec<SplitRefusal>);

impl SplitRefusals {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Comma-joined tokens for the structured log line — one field, so the whole set survives a
    /// whitespace-split log reader. (Its sibling [`explanation`](Self::explanation) joins with
    /// spaces because that one is a sentence.)
    pub fn tokens(&self) -> String {
        self.0
            .iter()
            .map(|refusal| refusal.token())
            .collect::<Vec<_>>()
            .join(",")
    }

    /// The whole set as one sentence for the feed.
    pub fn explanation(&self) -> String {
        self.0
            .iter()
            .map(|refusal| refusal.explanation())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl From<Vec<SplitRefusal>> for SplitRefusals {
    fn from(refusals: Vec<SplitRefusal>) -> Self {
        Self(refusals)
    }
}

impl std::ops::Deref for SplitRefusals {
    type Target = [SplitRefusal];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// What a split produced — the facts the feed line needs and nothing else.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SplitBand {
    /// The id the new band was allocated.
    pub band: BandId,
    /// Where both halves now stand. A split is co-located until the player moves one.
    pub at: UVec2,
    /// Workers that went with it — the player's one input, echoed back as resolved.
    pub workers: u32,
    /// The fraction every other quantity divided on.
    pub share: f32,
    /// Provisions the new band opened with.
    pub provisions: Scalar,
}

/// **The gate, evaluated against numbers rather than a world** — so the command and any forecast
/// that wants to answer the same question ahead of time run one rule set rather than two that agree
/// by habit.
///
/// `parent_workers` is the parent's **assignable** worker count ([`available_workers`]), which is
/// what the player is choosing from; the fractional remainder of the cohort is not on the table.
pub fn split_refusals(asked: u32, parent_workers: u32, settle: &SettleConfig) -> Vec<SplitRefusal> {
    let mut refusals = Vec::new();
    if asked == 0 {
        refusals.push(SplitRefusal::EmptySplit);
        // Every floor below is a statement about a band that would exist. With nobody going there is
        // no such band, so they would all fire at once and say the same thing five ways.
        return refusals;
    }
    if asked > parent_workers {
        refusals.push(SplitRefusal::NotEnoughWorkers {
            asked,
            available: parent_workers,
        });
        // Same reasoning: the floors below are evaluated against a split that cannot be made.
        return refusals;
    }
    if asked < settle.min_founding_workers {
        refusals.push(SplitRefusal::NewBandTooSmall {
            workers: asked,
            required: settle.min_founding_workers,
        });
    }
    let remaining = parent_workers - asked;
    if remaining < settle.parent_min_workers {
        refusals.push(SplitRefusal::ParentTooSmall {
            remaining,
            required: settle.parent_min_workers,
        });
    }
    refusals
}

/// **Split `parent` in two, and return the new band.**
///
/// The player names a worker count; `share = asked ÷ parent_workers` divides everything else —
/// children, elders and every store the band holds. **The new band is a smaller copy of the one it
/// came from, not a party with a composition of its own**, and that is the design rather than a
/// simplification of it (`docs/plan_band_fission.md` §Q3): per-bracket allocation would let a band
/// that cannot feed itself shed the people who cannot feed it, splitting off the elders and keeping
/// the workers. A proportional split takes the same share of mouths as of hands, so the parent's
/// dependency ratio comes out exactly where it went in and that move does not exist. The exploit is
/// closed by the shape of the model, which is why no ratio ceiling appears anywhere in this file.
///
/// **The exact fraction moves.** `3.27` children is an ordinary [`Scalar`] and transferring it
/// exactly is what keeps the two cohorts conserved with no rounding rule and no leftover to
/// reconcile. Whole bodies are a *display* concern, settled by `HudFormat.apportion_people` on the
/// client, where the same apportionment already renders every band's PEOPLE block.
///
/// Everything is checked before anything is written, so a refusal leaves the parent untouched.
pub fn split_band_from_parent(
    world: &mut World,
    parent: Entity,
    asked: u32,
    settle: &SettleConfig,
) -> Result<SplitBand, SplitRefusals> {
    if world.get::<ResidentBand>(parent).is_none() {
        return Err(SplitRefusals::from(vec![SplitRefusal::NotAResidentBand]));
    }
    let Some(cohort) = world.get::<PopulationCohort>(parent) else {
        return Err(SplitRefusals::from(vec![SplitRefusal::NotAResidentBand]));
    };
    let parent_workers = available_workers(cohort.working);
    let refusals = split_refusals(asked, parent_workers, settle);
    if !refusals.is_empty() {
        return Err(SplitRefusals::from(refusals));
    }

    // The share is taken against the cohort's **whole** working value, not the assignable count it
    // was chosen from: the fractional remainder is real people who eat, and dividing by the rounded
    // figure would hand the new band a slightly different slice of children than of workers.
    let mut child = cohort.clone();
    let share = if cohort.working > scalar_zero() {
        scalar_from_f32(asked as f32) / cohort.working
    } else {
        return Err(SplitRefusals::from(vec![SplitRefusal::NotEnoughWorkers {
            asked,
            available: 0,
        }]));
    };

    let site_tile = cohort.current_tile;
    let Some(site) = world.get::<Tile>(site_tile).map(|tile| tile.position) else {
        return Err(SplitRefusals::from(vec![SplitRefusal::NotAResidentBand]));
    };

    // ---- Divide the people ----
    // Workers are the number the player chose, exactly. Dependants are the share of what the parent
    // held, which is what makes the two halves the same band in miniature.
    let taken_working = scalar_from_f32(asked as f32);
    let taken_children = child.children * share;
    let taken_elders = child.elders * share;

    child.working = taken_working;
    child.children = taken_children;
    child.elders = taken_elders;

    // ---- Divide the stores ----
    // Every good, on the same share. The new band starts stocked because its people were already
    // sitting on that food — there is no reserve calculation and no second consumption rate.
    let mut child_stores = LocalStore::new();
    let taken: Vec<(String, Scalar)> = cohort
        .stores
        .iter()
        .map(|(item, amount)| (item.to_string(), amount * share))
        .collect();
    for (item, amount) in &taken {
        if *amount > scalar_zero() {
            child_stores.add(item, *amount);
        }
    }
    let provisions = child_stores.get(crate::components::FOOD);
    child.stores = child_stores;

    // ---- What this band's own life starts as ----
    // `age_turns` gates `migration_min_settled_turns`; inheriting the parent's settled duration would
    // let a band formed this turn bleed people out on its first.
    child.age_turns = 0;
    child.home = site_tile;
    child.current_tile = site_tile;
    child.migration = None;
    // **Grievance is INHERITED, not zeroed.** These are the same people who were unhappy a moment
    // ago, and a split that reset it would make forming a band a way to launder discontent — the
    // same class of move the proportional share exists to close.
    // Derived per-turn readings are the *parent's*, copied by the clone. They are all recomputed next
    // turn, but a split publishes a frame before then, and the new band would open by narrating
    // somebody else's morale swing, meal and migration.
    child.last_food_consumption = 0.0;
    // The dowry is booked on the *allocation's* accumulator below, which is what the next turn
    // capture copies here; carrying the parent's published pair over would have the new band open by
    // reporting a transfer it was not party to.
    child.last_turn_transfer_received = 0.0;
    child.last_turn_transfer_sent = 0.0;
    child.last_morale_delta = scalar_zero();
    child.last_morale_cause = MoraleCause::default();
    child.last_morale_contributions = MoraleContributions::default();
    child.last_fertility_factors = crate::components::FertilityFactors::default();
    child.discontent_fraction = scalar_zero();
    child.last_emigrated = 0;
    child.last_immigrated = 0;
    child.sync_size();

    // ---- Take it off the parent ----
    // Subtraction rather than a second multiply, so the two halves sum to what the band held however
    // the share rounds in fixed point.
    {
        let Some(mut parent_cohort) = world.get_mut::<PopulationCohort>(parent) else {
            return Err(SplitRefusals::from(vec![SplitRefusal::NotAResidentBand]));
        };
        parent_cohort.working -= taken_working;
        parent_cohort.children -= taken_children;
        parent_cohort.elders -= taken_elders;
        for (item, amount) in &taken {
            parent_cohort.stores.take(item, *amount);
        }
        parent_cohort.sync_size();
    }

    // **The food that walked out with them is booked as a transfer, on both ends.** A split is a
    // command applied *between* two captures, so the parent publishes a frame whose larder fell by
    // the share it handed over — food that passed through neither `food_income` nor
    // `food_consumption`. Without this the published identity
    // (`LaborAllocation::last_transfer_received`) is simply false on the turn a band splits, short
    // by exactly the provisions.
    //
    // **FOOD only**, and the same pair of terms `balance_supply_networks` and a trade shipment use:
    // the identity is the food one, and materials deliberately have no identity of their own.
    if provisions > scalar_zero() {
        if let Some(mut allocation) = world.get_mut::<LaborAllocation>(parent) {
            // Added, never assigned — a band can split, ship and balance inside one snapshot window.
            allocation.last_transfer_sent += provisions.to_f32();
        }
    }

    // **The kit is inherited WORN** (`docs/plan_band_fission.md` §Q4). `BandEquipment` is a wear
    // ledger, so handing the new band a `default()` would mint a fresh kit out of nothing every time
    // a band splits, permanently — which trivially defeats the pull into the crafting economy that
    // running your kit dry is supposed to be. The splinter is exactly as worn out as the people it
    // came from.
    let equipment = world
        .get::<BandEquipment>(parent)
        .cloned()
        .unwrap_or_default();
    let unit = world
        .get::<StartingUnit>(parent)
        .cloned()
        .unwrap_or_else(|| StartingUnit::new("band".to_string(), Vec::new()));

    let band = world.resource_mut::<BandIdAllocator>().allocate();
    let child_entity = world
        .spawn((
            child,
            band,
            ResidentBand,
            // **Every resident band carries a flow accumulator** or its births and deaths are
            // unreportable (`demographic_events::every_resident_band_carries_a_flow_accumulator`).
            DemographicFlowAccumulator::default(),
            // Labor assignments are the parent's intent about the parent's sources; the new band
            // starts idle and the player staffs it. The **receiving** half of the food ledger's
            // transfer pair opens on it, though: the dowry is food that crossed between larders, and
            // the new band's first published frame is the one that has to account for it.
            LaborAllocation {
                last_transfer_received: provisions.to_f32(),
                ..LaborAllocation::default()
            },
            equipment,
            unit,
        ))
        .id();

    // **The culture the splinter walked out with is part of the dowry too.** Attached here rather
    // than left to `reconcile_band_culture_layers`, whose "resident band with no layer" case seeds
    // from the province the band stands in — which for a split is the parent's own province, so the
    // difference is invisible today and would become wrong the moment the new band moves before the
    // reconcile runs. Seeding from the parent is the rule a *walking* band already gets from
    // `set_band_parent`. Attaching now also means the reconcile finds a layer next turn and its
    // attach branch never fires, so there is one attach path and not two.
    if let Some(parent_band) = world.get::<BandId>(parent).copied() {
        let region_id =
            crate::culture::culture_region_at(world.get_resource::<ProvinceMap>(), site);
        let mut culture = world.resource_mut::<CultureManager>();
        let parent_region = culture.upsert_regional(region_id);
        culture.attach_band_from_source(band, parent_region, parent_band);
    }
    debug_assert!(world.get::<PopulationCohort>(child_entity).is_some());

    Ok(SplitBand {
        band,
        at: site,
        workers: asked,
        share: share.to_f32(),
        provisions,
    })
}
