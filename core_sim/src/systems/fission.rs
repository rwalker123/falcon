//! **Band fission — a band splits in two where it stands** (`docs/plan_band_fission.md`, issue #511).
//!
//! The whole verb is *form a band, then move it*. The new band is a [`ResidentBand`] from the moment
//! the command resolves: it eats, ages, forages, births and starves on the same systems as every
//! other band, on its first turn. There is no party, no walk, no arrival and no second decision.
//!
//! **The player's one input is a worker count**, and every other quantity divides on the share that
//! implies (`docs/plan_band_fission.md` §Q3). That is the model, not an economy on top of it — see
//! [`split_band_from_parent`] for why a per-bracket choice is the thing being avoided.

use std::collections::BTreeMap;

use bevy::prelude::*;

use crate::band_names::BandNameCatalogHandle;
use crate::components::{
    available_workers, BandEquipment, BandId, DemographicFlowAccumulator, LaborAllocation,
    LocalStore, MaterialDraw, MoraleCause, MoraleContributions, PopulationCohort, ResidentBand,
    StartingUnit, Tile, TransferLedger, TransferLink,
};
use crate::culture::CultureManager;
use crate::equipment_config::{EquipmentConfig, EquipmentConfigHandle};
use crate::expedition_config::SettleConfig;
use crate::orders::FactionId;
use crate::provinces::ProvinceMap;
use crate::resources::{BandIdAllocator, BandNameAllocator, SimulationConfig};
use crate::scalar::{scalar_from_f32, scalar_zero, Scalar};
use crate::starting_loadout::{
    KitAllocation, LoadoutSupply, LoadoutWindow, MaterialAllocation, StartingLoadout,
};

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
    // Kept beside the share because the **whole-unit** manifest divides on the ratio rather than on
    // the rounded quotient — see [`whole_share`].
    let cohort_working = cohort.working;
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
    child.last_turn_food_transfers = TransferLedger::default();
    child.last_turn_fodder_transfers = TransferLedger::default();
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

    // ---- WHICH WAY THIS SPLIT PAYS FOR THE SPLINTER ----
    //
    // ⛔ **A GRANT SPLIT PARTITIONS THE GRANT AND MOVES NOTHING; A TAKE SPLIT MOVES GOODS.** They are
    // two ways of paying for one splinter and doing **both** charges the parent twice — which is what
    // shipped: the manifest walked out of the parent's ledger *and* the splinter's slots and points
    // came off the parent's budget. A player who had spent 28 of 30 points and split 5 hands off 17
    // watched their card read **`-6 / 22 left`**, and the negative meter was only the visible edge:
    // the parent's standing allocation was never re-fitted, so its next revision — an apply is a
    // replacement built from empty — **re-minted all 28 units** while the 8 that had walked stayed
    // with the splinter. Material out of nothing, on every turn-one split.
    //
    // The rule is decided **before anything moves**, because it decides whether anything moves at
    // all. `grants()` is *open, and still holding an unspent grant*; see
    // [`open_splinter_loadout_window`] for what each arm then does.
    let parent_grants = world
        .get::<BandId>(parent)
        .copied()
        .and_then(|id| {
            world
                .get_resource::<StartingLoadout>()
                .and_then(|loadout| loadout.window(id).map(LoadoutWindow::grants))
        })
        .unwrap_or(false);

    // ---- Divide the MATERIALS ----
    // **A second account, and it was never divided at all until now.** `LocalStore::iter` walks the
    // commodity bag only, so a splinter of a band sitting on twenty hides used to open with none of
    // them. The batches move through [`LocalStore::take_material_batches`] — the store's-own-order
    // twin of `take_material`, deliberately **not** the axis-sorted one: a split names no axis, and
    // each draw carries its source batch's exact reading, so nothing is re-graded on the way across.
    //
    // ⛔ **WHOLE UNITS, because the take is PUBLISHED as an allocation.** The default take opens the
    // splinter's outfitting card, and a card states `units:u32` — so a fractional `share × total`
    // could not be shown, and re-sending what the card showed would hand the remainder back. A whole
    // floored share is what makes an untouched *"Set out"* an exact no-op.
    //
    // **Skipped entirely when the parent still grants** — see the callout above.
    let default_materials = if parent_grants {
        Vec::new()
    } else {
        default_take_materials(
            world
                .get::<PopulationCohort>(parent)
                .map(|cohort| &cohort.stores),
            asked,
            cohort_working,
        )
    };
    let mut moved_materials: Vec<(String, Vec<MaterialDraw>)> = Vec::new();
    {
        let Some(mut parent_cohort) = world.get_mut::<PopulationCohort>(parent) else {
            return Err(SplitRefusals::from(vec![SplitRefusal::NotAResidentBand]));
        };
        for allocation in &default_materials {
            let drawn = parent_cohort.stores.take_material_batches(
                &allocation.material_id,
                scalar_from_f32(allocation.units as f32),
            );
            if !drawn.is_empty() {
                moved_materials.push((allocation.material_id.clone(), drawn));
            }
        }
    }
    let mut taken_materials: BTreeMap<String, Scalar> = BTreeMap::new();
    for (material, draws) in &moved_materials {
        let mut moved = scalar_zero();
        for draw in draws {
            child.stores.deposit_material(
                material,
                draw.band.clone(),
                draw.amount,
                &draw.characteristics,
            );
            moved += draw.amount;
        }
        taken_materials.insert(material.clone(), moved);
    }

    // **The food that walked out with them is booked as a transfer, on both ends.** A split is a
    // command applied *between* two captures, so the parent publishes a frame whose larder fell by
    // the share it handed over — food that passed through neither `food_income` nor
    // `food_consumption`. Without this the published identity
    // (`LaborAllocation::last_food_transfers`) is simply false on the turn a band splits, short
    // by exactly the provisions.
    //
    // **FOOD only**, and the same ledger `balance_supply_networks` and a trade shipment write:
    // the identity is the food one, and materials deliberately have no identity of their own.
    //
    // **The link is [`TransferLink::Local`]**: a splinter is camped where its parent is and nothing
    // carried the dowry anywhere — the same *standing together* crossing supply-network pooling is,
    // and not the [`TransferLink::Route`] a party's pack takes.
    if provisions > scalar_zero() {
        if let Some(mut allocation) = world.get_mut::<LaborAllocation>(parent) {
            // Added, never assigned — a band can split, ship and balance inside one snapshot window.
            allocation
                .last_food_transfers
                .debit(TransferLink::Local, provisions.to_f32());
        }
    }

    // ---- Divide the KIT ----
    //
    // **The kit is inherited WORN** (`docs/plan_band_fission.md` §Q4). `BandEquipment` is a wear
    // ledger, so handing the new band a `default()` would mint a fresh kit out of nothing every time
    // a band splits, permanently — which trivially defeats the pull into the crafting economy that
    // running your kit dry is supposed to be.
    //
    // ⛔ **AND IT IS MOVED, NOT COPIED.** This used to `clone()` the whole ledger onto the splinter
    // and never debit the parent, which minted a second full kit on every split — strictly better
    // than the `default()` the comment above rules out, and duplication all the same. Moved with
    // [`BandEquipment::take_units`]: **the freshest units leave** and the parent keeps the worn
    // stock, because a new venture is outfitted properly.
    //
    // ⛔ **AND IT IS DENOMINATED IN KITS** — see [`default_take_kits`]. The manifest was a bare
    // per-item `floor(share × count_of(item))`, which no kit allocation could express, so the
    // splinter's outfitting card opened **empty** while the band held the gear: an untouched
    // *"Set out"* then ordered *take nothing* and handed the whole dowry straight back. What moves
    // here is `expand_kits(default_kits)` — the **one** expansion rule, shared with every take a
    // player composes — so what the card shows and what the band holds are the same object.
    //
    // ⛔ **AND NOTHING MOVES AT ALL WHEN THE PARENT STILL GRANTS** — the splinter mints from slots
    // carved out of the parent's budget instead, and moving gear on top of that charged the parent
    // twice. See the callout above the material divide.
    let equipment_config = world
        .get_resource::<EquipmentConfigHandle>()
        .map(|handle| handle.get())
        .unwrap_or_else(EquipmentConfig::builtin);
    let default_kits = if parent_grants {
        Vec::new()
    } else {
        default_take_kits(
            &equipment_config,
            world.get::<BandEquipment>(parent),
            asked,
            cohort_working,
        )
    };
    let mut equipment = BandEquipment::default();
    let mut taken_items: BTreeMap<String, u32> = BTreeMap::new();
    if let Some(mut parent_equipment) = world.get_mut::<BandEquipment>(parent) {
        for (item, units) in crate::starting_loadout::expand_kits(&equipment_config, &default_kits)
        {
            if units == 0 {
                continue;
            }
            let batches = parent_equipment.take_units(&item, units);
            let moved: u32 = batches.iter().map(|batch| batch.count).sum();
            equipment.place_batches(&item, batches);
            if moved > 0 {
                taken_items.insert(item, moved);
            }
        }
    }
    // **`StartingUnit` rides onto the splinter and MUST**, however little the outfitting window now
    // needs it: it is the *commandable unit* marker `resolve_starting_unit_entity` gates every
    // band-addressed order on, and one half of the vision-source query
    // (`visibility_systems::VisionCohorts`). A splinter without it is a band the player cannot order
    // and that sees nothing.
    let unit = world
        .get::<StartingUnit>(parent)
        .cloned()
        .unwrap_or_else(|| StartingUnit::new("band".to_string(), Vec::new()));

    // The receiving half of the dowry, on the same [`TransferLink::Local`] arm the parent's debit
    // took — the two ends of one crossing, so a reader summing the faction's arms sees them cancel.
    let mut dowry_received = TransferLedger::default();
    dowry_received.credit(TransferLink::Local, provisions.to_f32());

    let band = world.resource_mut::<BandIdAllocator>().allocate();
    // **A splinter is a NEW band, so it mints a fresh name rather than inheriting the parent's.**
    // It walks out with the parent's food, kit and culture, but its identity is its own from the
    // first turn — the parent is still standing, and two living bands may not answer to one name.
    // Resolved before the spawn because minting borrows the world mutably.
    let child_faction = child.faction;
    let map_seed = world
        .get_resource::<SimulationConfig>()
        .map(|config| config.map_seed)
        .unwrap_or_default();
    // A hand-rolled test `World` may install neither resource; the builtin pool is the very list
    // `include_str!` baked in, so falling back to it substitutes nothing.
    let catalog = world
        .get_resource::<BandNameCatalogHandle>()
        .map(BandNameCatalogHandle::get)
        .unwrap_or_else(crate::band_names::BandNameCatalog::builtin);
    let name = world
        .get_resource_mut::<BandNameAllocator>()
        .map(|mut names| names.mint(child_faction, map_seed, catalog.as_ref()))
        .unwrap_or_else(|| crate::components::BandName(String::new()));
    let child_entity = world
        .spawn((
            child,
            band,
            name,
            ResidentBand,
            // **Every resident band carries a flow accumulator** or its births and deaths are
            // unreportable (`demographic_events::every_resident_band_carries_a_flow_accumulator`).
            DemographicFlowAccumulator::default(),
            // Labor assignments are the parent's intent about the parent's sources; the new band
            // starts idle and the player staffs it. The **receiving** half of the food ledger's
            // transfer terms opens on it, though: the dowry is food that crossed between larders, and
            // the new band's first published frame is the one that has to account for it.
            LaborAllocation {
                last_food_transfers: dowry_received,
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
    let partitioned_a_grant = open_splinter_loadout_window(
        world,
        parent,
        band,
        asked,
        share,
        SplinterTake {
            kits: default_kits,
            materials: default_materials,
            items: taken_items,
            material_amounts: taken_materials,
        },
    );
    if partitioned_a_grant {
        if let Some(parent_band) = world.get::<BandId>(parent).copied() {
            rebalance_partitioned_grant(world, child_faction, parent_band, band);
        }
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

/// **The whole units a proportional share of `held` comes to** — `floor(held × asked ÷ workers)`.
///
/// Computed from the **ratio** rather than from the already-rounded `share`, and that is not a
/// micro-optimisation: `share` is a fixed-point quotient, so a third stores as `0.333333` and
/// `0.333333 × 3` floors to **0** — the third of three spears the player asked for, gone.
///
/// A zero worker count divides nothing: the caller has already refused that split, and answering `0`
/// here keeps the helper total.
fn whole_share(held: u32, asked: u32, workers: Scalar) -> u32 {
    let workers = workers.to_f32() as f64;
    if held == 0 || asked == 0 || workers <= 0.0 {
        return 0;
    }
    let share = (held as f64) * (asked as f64) / workers;
    share.floor().max(0.0).min(held as f64) as u32
}

/// **Open the splinter's own outfitting window** (`crate::starting_loadout`).
///
/// Every band gets one, and what bounds it is a fact about the **parent's** state rather than about
/// the turn:
///
/// | the parent's window | the splinter's window |
/// |---|---|
/// | still holds an unspent **grant** (turn one) | a grant of its own: `min(asked, the parent's remaining kit budget)` kit slots and `floor(share × the parent's remaining material points)`, **both deducted from the parent's** — so no slot and no point is minted twice or lost. Its picks MINT, and **nothing physical moved**: [`rebalance_partitioned_grant`] re-fits the parent to its reduced budget and hands what that takes off to the splinter. |
/// | holds no grant (every later turn) | a **take** on the parent: the cap is what the parent can supply, and the kit allocation just moved is the window's **accepted allocation**, so the card opens on it. Its picks MOVE. |
///
/// Nothing here is a literal: both caps are the numbers the split itself just resolved.
///
/// Returns **whether it partitioned a grant**, which is what the caller needs to know to run the
/// re-fit — the budgets have moved by then, so *"did the parent grant"* is no longer answerable.
fn open_splinter_loadout_window(
    world: &mut World,
    parent: Entity,
    band: BandId,
    asked: u32,
    share: Scalar,
    take: SplinterTake,
) -> bool {
    let Some(parent_band) = world.get::<BandId>(parent).copied() else {
        return false;
    };
    let Some(mut loadout) = world.get_resource_mut::<StartingLoadout>() else {
        return false;
    };
    let parent_grant = loadout
        .window(parent_band)
        .filter(|window| window.grants())
        .map(|window| (window.supply.kit_budget(), window.supply.material_budget()));
    let supply = match parent_grant {
        Some((parent_kits, parent_points)) => {
            let kit_budget = asked.min(parent_kits);
            let material_budget = (scalar_from_f32(parent_points as f32) * share)
                .to_f32()
                .floor()
                .max(0.0) as u32;
            let material_budget = material_budget.min(parent_points);
            if let Some(window) = loadout.window_mut(parent_band) {
                window.supply = LoadoutSupply::Grant {
                    kit_budget: parent_kits - kit_budget,
                    material_budget: parent_points - material_budget,
                };
            }
            LoadoutSupply::Grant {
                kit_budget,
                material_budget,
            }
        }
        None => LoadoutSupply::Parent {
            parent: parent_band,
            items: take.items,
            materials: take.material_amounts,
        },
    };
    // ⛔ **THE WINDOW OPENS AT THE ALLOCATION, NOT AT ZERO.** The rows are the accepted allocation
    // this band's card draws itself from, and they were empty on a splinter for the whole first cut
    // of this arc: the card showed nothing while the band held its dowry, so an untouched
    // *"Set out"* re-sent an empty order — which is a **real** order (*take nothing*, the same as it
    // is on a grant) and handed the lot back. The card is no longer empty when the take is not, and
    // an empty tail keeps its meaning.
    let partitioned_a_grant = matches!(supply, LoadoutSupply::Grant { .. });
    let mut window = LoadoutWindow::opened(supply);
    window.kits = take.kits;
    window.materials = take.materials;
    loadout.open(band, window);
    partitioned_a_grant
}

/// **Re-fit the parent to the budget the split just took off it, and hand the splinter what that
/// takes away.**
///
/// # ⛔ THE METER MUST NEVER BE ABLE TO READ NEGATIVE
///
/// A grant split reduces the parent's two budgets. Its **standing allocation** is not automatically
/// smaller, so a parent that had spent 28 of 30 points sat at 28 against a budget of 22 — the card
/// read `-6 / 22 left`, and its next revision would have re-minted all 28. This closes both: the
/// allocation is re-fitted to the reduced budget by
/// [`crate::starting_loadout::clamp_allocation`]'s proportional-floored rule, and the parent is
/// **re-materialized from it** so its ledger, its store and its meter state one thing.
///
/// **What the clamp takes off the parent is not deleted — it is OFFERED TO THE SPLINTER**, bounded by
/// the splinter's own budget by the same rule. That is the model: those units are taken away from the
/// main band and given to the new one.
///
/// **A parent that still fits its reduced budget gives up nothing**, and this returns without
/// touching either band. That is not merely an optimisation: re-materializing rebuilds a ledger from
/// **empty** (an apply is a replacement), so running it on a parent with no standing allocation would
/// destroy gear that never came from one.
///
/// Both halves are re-materialized through **`apply_starting_loadout`**, the same path a player's own
/// commit takes, so there is one materialization rule and the refusals it enforces are the ones that
/// apply here too. A refusal is structurally impossible — a clamped allocation fits by construction
/// and its ids came from an order that was already accepted — so one is logged rather than handled.
fn rebalance_partitioned_grant(
    world: &mut World,
    faction: FactionId,
    parent_band: BandId,
    child_band: BandId,
) {
    let Some(loadout) = world.get_resource::<StartingLoadout>() else {
        return;
    };
    let Some(parent_window) = loadout.window(parent_band) else {
        return;
    };
    let parent_kits: BTreeMap<String, u32> = parent_window
        .kits
        .iter()
        .map(|row| (row.kit_id.clone(), row.count))
        .collect();
    let parent_materials: BTreeMap<String, u32> = parent_window
        .materials
        .iter()
        .map(|row| (row.material_id.clone(), row.units))
        .collect();
    let (parent_kit_budget, parent_material_budget) = (
        parent_window.supply.kit_budget(),
        parent_window.supply.material_budget(),
    );
    let (child_kit_budget, child_material_budget) = loadout
        .window(child_band)
        .map(|window| (window.supply.kit_budget(), window.supply.material_budget()))
        .unwrap_or_default();

    let (kept_kits, kits_bound) =
        crate::starting_loadout::clamp_allocation(&parent_kits, parent_kit_budget);
    let (kept_materials, materials_bound) =
        crate::starting_loadout::clamp_allocation(&parent_materials, parent_material_budget);
    if !kits_bound && !materials_bound {
        return;
    }

    // What the clamp took off the parent, fitted to the splinter's own budget by the same rule.
    let shed_kits = shed(&parent_kits, &kept_kits);
    let shed_materials = shed(&parent_materials, &kept_materials);
    let child_kits = crate::starting_loadout::clamp_allocation(&shed_kits, child_kit_budget).0;
    let child_materials =
        crate::starting_loadout::clamp_allocation(&shed_materials, child_material_budget).0;

    for (band, kits, materials) in [
        (parent_band, kept_kits, kept_materials),
        (child_band, child_kits, child_materials),
    ] {
        let kits: Vec<KitAllocation> = kits
            .into_iter()
            .map(|(kit_id, count)| KitAllocation { kit_id, count })
            .collect();
        let materials: Vec<MaterialAllocation> = materials
            .into_iter()
            .map(|(material_id, units)| MaterialAllocation { material_id, units })
            .collect();
        if band == child_band && kits.is_empty() && materials.is_empty() {
            continue;
        }
        if let Err(reason) =
            crate::starting_loadout::apply_starting_loadout(world, faction, band, &kits, &materials)
        {
            warn!(
                target: "shadow_scale::campaign",
                band = band.0,
                %reason,
                "starting_loadout.grant_partition.refused=a clamped allocation must always fit"
            );
        }
    }
}

/// What a clamp took away: `before − kept`, per row, dropping the rows it left alone.
fn shed(before: &BTreeMap<String, u32>, kept: &[(String, u32)]) -> BTreeMap<String, u32> {
    let kept: BTreeMap<&str, u32> = kept.iter().map(|(id, n)| (id.as_str(), *n)).collect();
    before
        .iter()
        .filter_map(|(id, count)| {
            let left = count.saturating_sub(kept.get(id.as_str()).copied().unwrap_or(0));
            (left > 0).then(|| (id.clone(), left))
        })
        .collect()
}

/// **The dowry a split hands its splinter, in both denominations at once.**
///
/// The `kits` / `materials` halves are what the window PUBLISHES — the accepted allocation the
/// player's card opens on and re-sends unchanged; `items` / `material_amounts` are what actually
/// crossed, which is the bookkeeping a later revision is priced against. They are two readings of one
/// move (`items == expand_kits(kits)` by construction), carried together so no call site can pass one
/// without the other.
struct SplinterTake {
    kits: Vec<KitAllocation>,
    materials: Vec<MaterialAllocation>,
    items: BTreeMap<String, u32>,
    material_amounts: BTreeMap<String, Scalar>,
}

/// **The splinter's default take, DENOMINATED IN KITS** — the proportional share of the parent's
/// gear, expressed in the same currency the player's own take is composed in.
///
/// # Why kits and not items
///
/// The picker is kit-denominated by design: a player composes kits, never bare items. So a default
/// take expressed per item is one the card **cannot show and cannot adjust** — which is exactly what
/// shipped, and what made an untouched *"Set out"* forfeit the whole dowry.
///
/// # ⛔ A BENCH TOOL DOES NOT WALK OUT, AND THAT FALLS OUT OF THE DENOMINATION
///
/// `bone_awl`, `loom` and `tanning_frame` are the only three items no kit `uses`
/// ([`EquipmentConfig::item_is_kit_carried`]), and they are the knowledge-gated bench tools. Because
/// the take is composed of kits they can never appear in one — **shop equipment stays with the
/// workshop that built it**, rather than moving invisibly with a band that cannot see it on the card,
/// adjust it, or choose to keep it.
///
/// # The rule: PROPORTIONAL, FLOORED, and the remainder is left unspent
///
/// Two clamps, because `sled` is used by **both** `big_game` and `trapping` and so no kit's count can
/// be resolved on its own:
///
/// 1. **The kit's own ceiling** — `t_k = min over the items it uses of floor(share × parent holds)`,
///    the complete kits' worth of `k` the share affords.
/// 2. **The shared-item clamp** — where the kits' combined demand for an item exceeds that item's
///    share, every kit that uses it is scaled by `budget ÷ demand` and floored.
///
/// Proportional rather than first-come, on [`crate::starting_loadout::clamped_kit_defaults`]' stated
/// reasoning: the roster has no author's order to consume in, so "declaration order" would really be
/// *id* order and make `big_game` beat `trapping` because `b` sorts first — an arbitrary winner
/// dressed as a rule. The floor's remainder is left with the parent, which the player can then take
/// deliberately.
fn default_take_kits(
    equipment: &EquipmentConfig,
    parent: Option<&BandEquipment>,
    asked: u32,
    workers: Scalar,
) -> Vec<KitAllocation> {
    let Some(parent) = parent else {
        return Vec::new();
    };
    let carried: Vec<&crate::equipment_config::KitDefinition> = equipment
        .kits()
        .iter()
        .filter(|kit| !kit.uses.is_empty())
        .collect();
    // The share of each item the parent holds — the budget every clamp below is measured against.
    let mut budget: BTreeMap<&str, u32> = BTreeMap::new();
    for kit in &carried {
        for item in &kit.uses {
            budget
                .entry(item.as_str())
                .or_insert_with(|| whole_share(parent.count_of(item), asked, workers));
        }
    }
    // 1. Each kit's own ceiling: the complete kits' worth of it the share affords.
    let wanted: BTreeMap<&str, u32> = carried
        .iter()
        .map(|kit| {
            let ceiling = kit
                .uses
                .iter()
                .map(|item| budget.get(item.as_str()).copied().unwrap_or(0))
                .min()
                .unwrap_or(0);
            (kit.id.as_str(), ceiling)
        })
        .collect();
    // 2. The shared-item clamp: an item two kits both want is scaled proportionally.
    let mut demand: BTreeMap<&str, u32> = BTreeMap::new();
    for kit in &carried {
        let count = wanted.get(kit.id.as_str()).copied().unwrap_or(0);
        for item in &kit.uses {
            *demand.entry(item.as_str()).or_default() += count;
        }
    }
    carried
        .iter()
        .filter_map(|kit| {
            let count = wanted.get(kit.id.as_str()).copied().unwrap_or(0);
            if count == 0 {
                return None;
            }
            let scale = kit
                .uses
                .iter()
                .filter_map(|item| {
                    let wants = demand.get(item.as_str()).copied().unwrap_or(0);
                    let has = budget.get(item.as_str()).copied().unwrap_or(0);
                    (wants > has).then(|| f64::from(has) / f64::from(wants))
                })
                .fold(1.0_f64, f64::min);
            let scaled = (f64::from(count) * scale).floor().max(0.0) as u32;
            (scaled > 0).then(|| KitAllocation {
                kit_id: kit.id.clone(),
                count: scaled,
            })
        })
        .collect()
}

/// **The material half of the default take, in WHOLE UNITS** — `floor(share × total)` per material
/// the parent holds.
///
/// Materials are one-to-one with the currency the command spends, so this publishes exactly and needs
/// none of [`default_take_kits`]' clamping. It is **floored to whole units** for the same reason the
/// kit half is denominated in kits: the card states `units:u32`, so a fractional take is one it
/// cannot show and re-sending what it showed would hand the remainder back.
fn default_take_materials(
    parent: Option<&LocalStore>,
    asked: u32,
    workers: Scalar,
) -> Vec<MaterialAllocation> {
    let Some(parent) = parent else {
        return Vec::new();
    };
    parent
        .materials()
        .filter_map(|(material, _)| {
            let held = parent.material_total(material).to_f32();
            let workers = workers.to_f32();
            if held <= 0.0 || asked == 0 || workers <= 0.0 {
                return None;
            }
            let units = ((held as f64) * f64::from(asked) / f64::from(workers))
                .floor()
                .max(0.0) as u32;
            (units > 0).then(|| MaterialAllocation {
                material_id: material.to_string(),
                units,
            })
        })
        .collect()
}
