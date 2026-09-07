//! **The outfitting window — one per band, open until the turn is finalized.**
//!
//! A spawning band owns **nothing**: `equipment.json` ships `start_stock_fraction: 0.0`, and no
//! material declares a start stock (the mechanism that let one is deleted). Instead the player
//! composes a loadout *after* seeing the generated map — a budget of one kit per working-age hand
//! spread across the kit roster, and a separate budget of material points spread across the
//! profile's pick list.
//!
//! **Every band gets a window, not just the one that spawned.** A band that splits off hands its
//! splinter a window of its own, pre-filled with the proportional share the split already gives the
//! new band's people and food ([`crate::systems::split_band_from_parent`]). **Turn one is not
//! special** — only the *parent's state* differs:
//!
//! - The spawned band's window carries a **GRANT**, two budgets it may mint against, and a splinter
//!   of a band whose grant is still unspent takes a slice of that grant rather than of a ledger.
//!   Those windows [`LoadoutSupply::Grant`] and their picks **mint**.
//! - From turn two nobody holds a grant, so a splinter's window is a
//!   [`LoadoutSupply::Parent`] one: its picks **move** gear and material out of the parent's own
//!   ledger, and the cap is what the parent can supply.
//!
//! **Windows close on the turn advance and on nothing else** — committing a loadout does not close
//! one, so the whole of a turn is a working surface: the player tries a pick and revises it as often
//! as they like. An allocation is therefore a **replacement**, not a purchase (see
//! [`apply_starting_loadout`]). A `SetStartingLoadout` naming a band with no open window is rejected
//! whole rather than partially honoured, and nothing re-opens a window.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    components::{BandEquipment, BandId, PopulationCohort, StartingUnit},
    equipment_config::EquipmentConfigHandle,
    materials_config::MaterialsConfigHandle,
    orders::FactionId,
    recipes_config::RecipesConfigHandle,
    scalar::{scalar_from_f32, scalar_zero, Scalar},
    start_profile::ActiveStartProfile,
};

/// **The reading every material picked in the opening loadout arrives at, on every axis that
/// material declares** — the middle of the range: what the band scavenged before setting out is
/// unremarkable.
///
/// One number rather than a per-material table, because a spread would be a claim nothing makes: the
/// player picks a *quantity* of a generic material, not a provenance. It is faithful to what the
/// retired per-material start stocks read (`stone` 0.5/0.5, `wood` 0.5/0.6).
pub const OPENING_MATERIAL_READING: f32 = 0.5;

/// **Where a window's gear comes from** — the one fact that decides whether a pick *mints* or
/// *moves*, and therefore what caps it.
///
/// It is an enum rather than a pair of "meaningful only when…" fields because the two cases cap on
/// different currencies: a grant is bounded by two integers the world handed out, a parent take is
/// bounded by what another band is standing on right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoadoutSupply {
    /// **MINTED against the world's opening grant.** The spawned band's window, and a splinter of a
    /// band whose own grant is still unspent — that splinter takes a slice of the grant, deducted
    /// from the parent's, so no point is minted twice or lost.
    Grant {
        /// One per working-age hand, stamped at spawn or carved off the parent at a split.
        /// **Derived, never configured** — see `start_profile::OpeningLoadoutConfig`.
        kit_budget: u32,
        /// `start_profiles.json` `opening_loadout.material_points`, or the splinter's share of what
        /// the parent had left.
        material_budget: u32,
    },
    /// **MOVED out of `parent`'s ledger** — a splinter of a band with no grant left. The cap is what
    /// the parent can supply, and the standing take is the record of what has already crossed.
    Parent {
        /// The band this take is drawn from.
        parent: BandId,
        /// **The expanded ITEM take standing against the parent**, `item → whole units`.
        ///
        /// Recorded rather than re-derived, and that is what makes a revision exact: the band's own
        /// ledger is *the take minus whatever its own splinters have since taken off it*, so
        /// "reduce this band's take from 5 spears to 3" cannot be read off the ledger once the band
        /// has itself split.
        items: BTreeMap<String, u32>,
        /// **The material take standing against the parent**, `material → amount`, in the same
        /// fixed point the store holds. Fractional, because a split's default take is
        /// `share × total` and the store is continuous.
        materials: BTreeMap<String, Scalar>,
    },
}

impl LoadoutSupply {
    /// The kit slots this window may mint against — `0` for a take, which mints nothing.
    pub fn kit_budget(&self) -> u32 {
        match self {
            LoadoutSupply::Grant { kit_budget, .. } => *kit_budget,
            LoadoutSupply::Parent { .. } => 0,
        }
    }

    /// The material points this window may mint against — `0` for a take.
    pub fn material_budget(&self) -> u32 {
        match self {
            LoadoutSupply::Grant {
                material_budget, ..
            } => *material_budget,
            LoadoutSupply::Parent { .. } => 0,
        }
    }

    /// The band a take is drawn from, or `None` for a grant.
    pub fn parent(&self) -> Option<BandId> {
        match self {
            LoadoutSupply::Grant { .. } => None,
            LoadoutSupply::Parent { parent, .. } => Some(*parent),
        }
    }
}

/// **One band's outfitting window.**
///
/// `open` is set when the window is opened — by [`stamp_starting_loadout`] at world build, or by
/// [`crate::systems::split_band_from_parent`] at a split — and cleared by
/// [`close_opening_window`] and by nothing else. In particular **not** by
/// [`apply_starting_loadout`], which is what lets the player revise a pick for the whole turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadoutWindow {
    pub open: bool,
    /// Where this window's gear comes from, and what caps it. See [`LoadoutSupply`].
    pub supply: LoadoutSupply,
    /// **The kit rows the last accepted order named** — the accepted allocation, which is what the
    /// picker re-draws and what a revision replaces. Empty only for a window nobody has ordered
    /// against yet: a **splinter's opens at its default take**, denominated in kits by
    /// [`crate::systems::split_band_from_parent`] so that re-sending it unchanged is a no-op rather
    /// than an order to take nothing.
    pub kits: Vec<KitAllocation>,
    /// The material rows the last accepted order named, the twin of [`Self::kits`].
    pub materials: Vec<MaterialAllocation>,
}

impl LoadoutWindow {
    /// An open window with no order standing against it.
    pub fn opened(supply: LoadoutSupply) -> Self {
        Self {
            open: true,
            supply,
            kits: Vec::new(),
            materials: Vec::new(),
        }
    }

    /// Whether this window still has a grant to mint against — the test the split reads to decide
    /// whether a splinter takes a slice of the grant or a slice of the ledger.
    pub fn grants(&self) -> bool {
        self.open && matches!(self.supply, LoadoutSupply::Grant { .. })
    }

    /// This window's standing item take against its parent, `0` for a grant window.
    pub fn taken_units(&self, item: &str) -> u32 {
        match &self.supply {
            LoadoutSupply::Grant { .. } => 0,
            LoadoutSupply::Parent { items, .. } => items.get(item).copied().unwrap_or(0),
        }
    }

    /// This window's standing material take against its parent, zero for a grant window.
    pub fn taken_material(&self, material: &str) -> Scalar {
        match &self.supply {
            LoadoutSupply::Grant { .. } => scalar_zero(),
            LoadoutSupply::Parent { materials, .. } => {
                materials.get(material).copied().unwrap_or_else(scalar_zero)
            }
        }
    }
}

/// **Every band's outfitting window, keyed by band.**
///
/// Checkpoint state (`SimState`), because it is a fact about *this* world that nothing rebuilds — a
/// rollback into a turn whose windows were open must land back in a world whose windows are still
/// open, and a save must come back with the budgets and the takes it had.
///
/// [`Default`] is **no windows at all**, which is what a world that never ran worldgen (a load, a
/// bare test `App`) correctly reads as: there is nothing to outfit. A closed window is *removed*
/// rather than kept with `open: false` — a closed window can never be revised, so keeping one would
/// grow the checkpoint by a row per band ever split, forever, to record a state that is already what
/// an absent key says.
#[derive(Resource, Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StartingLoadout {
    windows: BTreeMap<BandId, LoadoutWindow>,
}

impl StartingLoadout {
    /// This band's window, or `None` when it has none — which reads as *closed*.
    pub fn window(&self, band: BandId) -> Option<&LoadoutWindow> {
        self.windows.get(&band)
    }

    /// Whether this band has an open window right now.
    pub fn is_open(&self, band: BandId) -> bool {
        self.windows
            .get(&band)
            .map(|window| window.open)
            .unwrap_or(false)
    }

    /// This band's window for editing — the split's seam for debiting a parent's grant.
    pub fn window_mut(&mut self, band: BandId) -> Option<&mut LoadoutWindow> {
        self.windows.get_mut(&band)
    }

    /// Open (or replace) this band's window.
    pub fn open(&mut self, band: BandId, window: LoadoutWindow) {
        self.windows.insert(band, window);
    }

    /// Every window, in band order.
    pub fn iter(&self) -> impl Iterator<Item = (BandId, &LoadoutWindow)> {
        self.windows.iter().map(|(band, window)| (*band, window))
    }

    /// How many windows stand open — the closing log line's number, and a fixture's liveness check.
    pub fn open_count(&self) -> usize {
        self.windows.values().filter(|window| window.open).count()
    }

    /// **What `band`'s own splinters have already taken off it**, `item → whole units`, summed over
    /// every open window drawing on it.
    ///
    /// It is the floor a revision of `band`'s own take must clear: a band that has itself split
    /// inside this turn has handed part of its stock onward, and lowering its take below what it
    /// passed on would strand that downstream take.
    pub fn onward_items(&self, band: BandId) -> BTreeMap<String, u32> {
        let mut onward: BTreeMap<String, u32> = BTreeMap::new();
        for window in self.windows.values() {
            let LoadoutSupply::Parent { parent, items, .. } = &window.supply else {
                continue;
            };
            if *parent != band {
                continue;
            }
            for (item, units) in items {
                *onward.entry(item.clone()).or_default() += *units;
            }
        }
        onward
    }

    /// The material twin of [`Self::onward_items`].
    pub fn onward_materials(&self, band: BandId) -> BTreeMap<String, Scalar> {
        let mut onward: BTreeMap<String, Scalar> = BTreeMap::new();
        for window in self.windows.values() {
            let LoadoutSupply::Parent {
                parent, materials, ..
            } = &window.supply
            else {
                continue;
            };
            if *parent != band {
                continue;
            }
            for (material, amount) in materials {
                let entry = onward.entry(material.clone()).or_insert_with(scalar_zero);
                *entry += *amount;
            }
        }
        onward
    }
}

/// One line of the kit half of a loadout: `count` of `kit_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KitAllocation {
    pub kit_id: String,
    pub count: u32,
}

/// One line of the material half: `units` of `material_id`, where one budget point buys one unit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialAllocation {
    pub material_id: String,
    pub units: u32,
}

/// **Why a `SetStartingLoadout` was refused.** Every variant refuses the **whole** command: a
/// loadout is one composition against one supply, so honouring the lines that happened to be legal
/// would spend the player's points on something they did not choose.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoadoutRejection {
    #[error("this band has no open outfitting window - a window shuts on the turn advance")]
    WindowClosed,
    #[error("'{0}' is not a kit on the equipment roster")]
    UnknownKit(String),
    #[error("'{0}' carries no items, so allocating it would buy nothing")]
    KitBuysNothing(String),
    #[error("'{0}' is not one of this start profile's pickable materials")]
    UnpickableMaterial(String),
    #[error("{kits} kits allocated against a budget of {budget}")]
    OverKitBudget { kits: u32, budget: u32 },
    #[error("{units} material units allocated against a budget of {budget}")]
    OverMaterialBudget { units: u32, budget: u32 },
    #[error("'{0}' is allocated twice - one line per kit, one line per material")]
    DuplicateAllocation(String),
    #[error("there is no such band to outfit")]
    NoStartingBand,
    /// The parent cannot cover a line of the take. Names the item (or material) and the shortfall,
    /// because *which* pick is unaffordable is the only thing the player can act on.
    #[error("the home band can supply {available} '{id}' and {asked} were asked for")]
    ParentCannotSupply {
        id: String,
        asked: u32,
        available: u32,
    },
    /// Lowering this band's take would leave a band that split off it holding a take its own stock
    /// can no longer justify. **Refused, never clamped**: a silent clamp would move a number the
    /// player did not name.
    #[error(
        "band {band} already took {onward} '{id}' off this one, so its take cannot fall to {asked}"
    )]
    OnwardTakeStranded {
        band: u64,
        id: String,
        asked: u32,
        onward: u32,
    },
}

/// **The tick the world is BUILT on**, and the one turn that does not shut a window.
///
/// `SimulationTick` opens at `0`, and building a world runs exactly one `app.update()` — Startup
/// worldgen plus one pass of the turn schedule — which is what produces the baseline frame the
/// client first draws. The player has not advanced anything yet at that point, so shutting the
/// window there would close it before the map it is composed against had ever been seen. Every
/// later update is a turn the player asked for.
const WORLD_BUILD_TICK: u64 = 0;

/// **Close every open window.** Registered before the turn's first stage, so a turn advance always
/// finds them shut — a budget is spent before the first turn resolves or it is not spent at all.
/// Idempotent: a world with nothing open closes nothing.
///
/// Skips [`WORLD_BUILD_TICK`], which is the pass that *opened* the spawned band's window.
pub fn close_opening_window(
    tick: Res<crate::resources::SimulationTick>,
    mut loadout: ResMut<StartingLoadout>,
) {
    if tick.0 == WORLD_BUILD_TICK {
        return;
    }
    let closed = loadout.open_count();
    if closed == 0 && loadout.windows.is_empty() {
        return;
    }
    // A closed window is an ABSENT one — see [`StartingLoadout`]. Dropping the rows is what keeps
    // the checkpoint from growing a permanent entry per band ever split.
    loadout.windows.clear();
    if closed > 0 {
        info!(
            target: "shadow_scale::campaign",
            windows = closed,
            "starting_loadout.window.closed=first_turn_advance"
        );
    }
}

/// **Stamp the spawned band's window open with the budgets the world just built.** A Startup system,
/// chained after the spawn, because the kit budget is the *spawned band's* worker count rather than
/// anything a config states.
///
/// A world with no starting band opens nothing: there is nobody to outfit.
pub fn stamp_starting_loadout(
    mut loadout: ResMut<StartingLoadout>,
    profile: Option<Res<ActiveStartProfile>>,
    bands: Query<(&BandId, &PopulationCohort), With<StartingUnit>>,
    demographics: Option<Res<crate::demographics_config::DemographicsConfigHandle>>,
) {
    let Some(profile) = profile else {
        return;
    };
    // The globally lowest `BandId` carrying `StartingUnit`, with **no faction filter**: the shipped
    // world is single-faction — `spawn_population_entity` hard-codes every band's cohort to
    // `FactionId(0)` — and worldgen is the one moment at which exactly the spawned bands exist, so
    // the lowest id is the campaign's opening band by construction.
    let Some((band, cohort)) = bands.iter().min_by_key(|(id, _)| **id) else {
        return;
    };
    let working_fraction = demographics
        .map(|handle| handle.get().initial_distribution.working)
        .unwrap_or_else(|| {
            crate::demographics_config::DemographicsConfig::builtin()
                .initial_distribution
                .working
        });
    let workers = crate::systems::party_workers(cohort.size, working_fraction);
    let kit_budget = workers as u32;
    let material_budget = profile
        .profile()
        .overrides()
        .opening_loadout
        .material_points;
    loadout.open(
        *band,
        LoadoutWindow::opened(LoadoutSupply::Grant {
            kit_budget,
            material_budget,
        }),
    );
    info!(
        target: "shadow_scale::campaign",
        band = band.0,
        kit_budget,
        material_budget,
        "starting_loadout.window.opened"
    );
    // **The kit pre-fill's only sanity check, and it happens HERE rather than at the publish site**
    // — this is the first and only moment a config fault of that shape can be observed (the budget
    // does not exist until the band does), and it happens once per world rather than once per
    // captured frame. `snapshot_opening_loadout` re-runs the same pure helper for the value.
    let (_, clamped) = clamped_kit_defaults(
        &profile.profile().overrides().opening_loadout.kit_defaults,
        kit_budget,
    );
    if clamped {
        warn!(
            target: "shadow_scale::campaign",
            kit_budget,
            declared = profile
                .profile()
                .overrides()
                .opening_loadout
                .kit_defaults
                .values()
                .sum::<u32>(),
            "starting_loadout.kit_defaults.clamped=the profile pre-fills more kits than this band \
             has hands"
        );
    }
}

/// **The kit column's pre-fill, scaled to fit the budget this band actually turned out to have.**
///
/// Returns the rows to publish, in id order, and whether the clamp bound.
///
/// # The rule: PROPORTIONAL, FLOORED, and the remainder is left unspent
///
/// `start_profiles.json`'s `kit_defaults` cannot be sum-checked at load, because the kit budget is
/// the spawned band's working-age head count rather than a number in that file
/// (`start_profile::OpeningLoadoutConfig::kit_defaults`). So an over-allocating pre-fill is scaled
/// here: each row becomes `floor(count × budget / declared_total)`, and a row that floors to zero is
/// **dropped** — a pre-fill of nothing is what an absent row already says.
///
/// **Proportional rather than first-come, and the floor's remainder goes nowhere.** The config is a
/// `BTreeMap`, so there is no author's order to consume in — "declaration order" would really be
/// *id* order, making `gathering` beat `trapping` because `g` sorts first, which is an arbitrary
/// winner dressed as a rule. Scaling preserves the shape the designer expressed, and handing the
/// leftover point or two to whichever id sorts first would put that arbitrary tiebreak back. Leaving
/// it unspent is strictly better: a pre-fill is a **suggestion**, and a couple of unallocated hands
/// is exactly the state the player is being invited to resolve.
///
/// **A zero budget publishes nothing**, which needs no special case: every scaled row floors to 0.
pub fn clamped_kit_defaults(
    declared: &BTreeMap<String, u32>,
    kit_budget: u32,
) -> (Vec<(String, u32)>, bool) {
    clamp_allocation(declared, kit_budget)
}

/// **PROPORTIONAL, FLOORED, remainder unspent — the one implementation of that rule.**
///
/// Returns the rows to keep, in id order, and whether the clamp bound. A row that floors to zero is
/// **dropped**: an allocation of nothing is what an absent row already says.
///
/// Three callers, all asking the same question — *"this allocation is bigger than the budget it is
/// measured against; which of it survives?"*:
///
/// - [`clamped_kit_defaults`], fitting a profile's kit pre-fill to the spawned band's head count;
/// - the **parent's** standing allocation, re-fitted to the budget a grant split just reduced;
/// - the **remainder** that clamp took off the parent, fitted to the splinter's own budget.
///
/// **Proportional rather than first-come, and the floor's remainder goes nowhere.** These are
/// `BTreeMap`s, so there is no author's order to consume in — "declaration order" would really be
/// *id* order, making `gathering` beat `trapping` because `g` sorts first, which is an arbitrary
/// winner dressed as a rule. Scaling preserves the shape the allocation expressed, and handing the
/// leftover point or two to whichever id sorts first would put that arbitrary tiebreak back. Leaving
/// it unspent is strictly better: the budget is still there and the player can spend it deliberately.
///
/// **A zero budget keeps nothing**, which needs no special case: every scaled row floors to 0.
pub(crate) fn clamp_allocation(
    declared: &BTreeMap<String, u32>,
    budget: u32,
) -> (Vec<(String, u32)>, bool) {
    let total: u32 = declared.values().copied().sum();
    if total <= budget {
        return (
            declared
                .iter()
                .filter(|(_, count)| **count > 0)
                .map(|(id, count)| (id.clone(), *count))
                .collect(),
            false,
        );
    }
    let rows = declared
        .iter()
        .filter_map(|(id, count)| {
            // `u64` because `count × budget` overflows a `u32` for counts a config could plausibly
            // typo (65_536 × 65_536), and a wrapped product would clamp *upward*.
            let scaled = (u64::from(*count) * u64::from(budget) / u64::from(total)) as u32;
            (scaled > 0).then(|| (id.clone(), scaled))
        })
        .collect();
    (rows, true)
}

/// The item list one kit allocation buys, expanded and summed across the whole order.
///
/// **Two kits that share an item ADD.** 3 `big_game` + 3 `trapping` is 3 spears, 3 traps and **6**
/// sleds: an allocation buys a kit's worth of gear per hand, and two hands carrying a sled are two
/// sleds however they were bought. That is also why a kit ROW cannot be capped on its own — the
/// roster maps kits to items almost one-to-one, but `sled` is used by two of them, so the thing that
/// has to fit is the **expanded item list**, whole.
pub(crate) fn expand_kits(
    equipment: &crate::equipment_config::EquipmentConfig,
    kits: &[KitAllocation],
) -> BTreeMap<String, u32> {
    let mut items: BTreeMap<String, u32> = BTreeMap::new();
    for allocation in kits {
        let Some(definition) = equipment.kit_definition(&allocation.kit_id) else {
            continue;
        };
        for item_id in &definition.uses {
            *items.entry(item_id.clone()).or_default() += allocation.count;
        }
    }
    items
}

/// **Apply a composed loadout to one band.**
///
/// Every check runs before anything is written, so a refusal leaves the world byte-identical.
///
/// # ⛔ AN ALLOCATION IS A REPLACEMENT, NOT A PURCHASE
///
/// **This does NOT shut the window** — [`close_opening_window`] is the only thing that ever clears
/// `open`, and it does so on the turn advance. The whole turn is a working surface: the player looks
/// around the map they were just given, tries a pick, and revises it. So a `SetStartingLoadout` is
/// accepted as many times as it arrives while the window is open.
///
/// That makes **idempotence the contract**: after applying allocation `A`, the band holds exactly
/// what `A` describes, however many drafts preceded it. `6 big_game` revised to `4 big_game` must
/// leave four kits' worth of gear, not ten.
///
/// # The two supplies
///
/// - [`LoadoutSupply::Grant`] — both halves are built from **empty** and minted. The material reset
///   is account-aware and the store is what makes it so: a band's `LocalStore` holds its food beside
///   its material batches, so the reset is [`crate::components::LocalStore::clear_materials`] rather
///   than a `LocalStore::new()` that would starve it.
/// - [`LoadoutSupply::Parent`] — the order is priced against **the parent's holdings plus the take
///   already standing**, because the units already moved are this band's to keep or give back;
///   pricing them as unavailable would refuse a raise from 3 to 5 for the 3 that are already here.
///   Only the **delta** crosses, in whichever direction it points, which is what makes a refusal
///   byte-identical without a rollback.
pub fn apply_starting_loadout(
    world: &mut World,
    faction: FactionId,
    band: BandId,
    kits: &[KitAllocation],
    materials: &[MaterialAllocation],
) -> Result<(), LoadoutRejection> {
    let Some(window) = world
        .resource::<StartingLoadout>()
        .window(band)
        .filter(|window| window.open)
        .cloned()
    else {
        return Err(LoadoutRejection::WindowClosed);
    };

    let equipment = world.resource::<EquipmentConfigHandle>().get();
    let recipes = world.resource::<RecipesConfigHandle>().get();
    let materials_table = world.resource::<MaterialsConfigHandle>().get();

    let mut named = BTreeSet::new();
    let mut kit_total: u32 = 0;
    for allocation in kits {
        if !named.insert(allocation.kit_id.as_str()) {
            return Err(LoadoutRejection::DuplicateAllocation(
                allocation.kit_id.clone(),
            ));
        }
        let Some(definition) = equipment.kit_definition(&allocation.kit_id) else {
            return Err(LoadoutRejection::UnknownKit(allocation.kit_id.clone()));
        };
        // **The `none` kit, refused by what makes it `none`.** A kit that puts nothing in anybody's
        // hands cannot be bought, and testing `uses` rather than the id keeps the rule true of any
        // future empty roster entry without minting a magic string.
        if definition.uses.is_empty() {
            return Err(LoadoutRejection::KitBuysNothing(allocation.kit_id.clone()));
        }
        kit_total = kit_total.saturating_add(allocation.count);
    }

    let mut named = BTreeSet::new();
    let mut material_total: u32 = 0;
    for allocation in materials {
        if !named.insert(allocation.material_id.as_str()) {
            return Err(LoadoutRejection::DuplicateAllocation(
                allocation.material_id.clone(),
            ));
        }
        material_total = material_total.saturating_add(allocation.units);
    }

    let Some(entity) = band_entity(world, faction, band) else {
        return Err(LoadoutRejection::NoStartingBand);
    };

    match &window.supply {
        LoadoutSupply::Grant {
            kit_budget,
            material_budget,
        } => {
            if kit_total > *kit_budget {
                return Err(LoadoutRejection::OverKitBudget {
                    kits: kit_total,
                    budget: *kit_budget,
                });
            }
            if material_total > *material_budget {
                return Err(LoadoutRejection::OverMaterialBudget {
                    units: material_total,
                    budget: *material_budget,
                });
            }
            // **The pick list is the GRANT's rule.** It says which materials the world hands out at
            // the start, so it binds exactly the window that mints and deliberately not a take,
            // whose only question is whether the parent has the stuff.
            let pickable = &world
                .resource::<ActiveStartProfile>()
                .profile()
                .overrides()
                .opening_loadout
                .pickable_materials;
            for allocation in materials {
                if !pickable.iter().any(|id| id == &allocation.material_id) {
                    return Err(LoadoutRejection::UnpickableMaterial(
                        allocation.material_id.clone(),
                    ));
                }
            }

            // --- nothing above this line writes; nothing below it can fail --------------------
            mint_loadout(
                world,
                entity,
                kits,
                materials,
                &equipment,
                &recipes,
                &materials_table,
            );
        }
        LoadoutSupply::Parent { parent, .. } => {
            let parent = *parent;
            let Some(parent_entity) = band_entity(world, faction, parent) else {
                return Err(LoadoutRejection::NoStartingBand);
            };
            let wanted_items = expand_kits(&equipment, kits);
            let plan = plan_take(
                world,
                band,
                parent_entity,
                &window,
                &wanted_items,
                materials,
            )?;

            // --- nothing above this line writes; nothing below it can fail --------------------
            move_take(world, entity, parent_entity, &plan);
            let mut loadout = world.resource_mut::<StartingLoadout>();
            if let Some(open) = loadout.windows.get_mut(&band) {
                open.supply = LoadoutSupply::Parent {
                    parent,
                    items: plan.items,
                    materials: plan.materials,
                };
            }
        }
    }

    {
        let mut loadout = world.resource_mut::<StartingLoadout>();
        if let Some(open) = loadout.windows.get_mut(&band) {
            open.kits = kits.to_vec();
            open.materials = materials.to_vec();
        }
    }

    // **The window is deliberately left OPEN.** The turn advance closes it and nothing else does.
    info!(
        target: "shadow_scale::campaign",
        faction = faction.0,
        band = band.0,
        kits = kit_total,
        material_units = material_total,
        "starting_loadout.applied"
    );
    Ok(())
}

/// The take a validated order resolves to: the new standing take, and the deltas that carry the
/// world from the old one to it. Resolved **before** anything is written so a refusal never has to
/// be undone.
struct TakePlan {
    /// The new standing item take, `item → whole units`.
    items: BTreeMap<String, u32>,
    /// The new standing material take, `material → amount`.
    materials: BTreeMap<String, Scalar>,
    /// `item → signed units`: positive moves from the parent to the band, negative moves back.
    item_delta: BTreeMap<String, i64>,
    /// The material twin of [`Self::item_delta`].
    material_delta: BTreeMap<String, Scalar>,
}

/// **Price a take against the parent, and against what this band has already passed onward.**
///
/// Two bounds, both refusing the whole order:
/// - **the parent's holdings plus the take already standing** — the units already here are this
///   band's to keep or hand back, so pricing them as unavailable would refuse a raise from 3 to 5
///   for the 3 that already moved;
/// - **what this band's own splinters have taken off it** — a band that split inside this turn has
///   already handed part of its stock onward, and lowering its take under that would strand it.
fn plan_take(
    world: &World,
    band: BandId,
    parent_entity: Entity,
    window: &LoadoutWindow,
    wanted_items: &BTreeMap<String, u32>,
    materials: &[MaterialAllocation],
) -> Result<TakePlan, LoadoutRejection> {
    let loadout = world.resource::<StartingLoadout>();
    let onward_items = loadout.onward_items(band);
    let onward_materials = loadout.onward_materials(band);
    let parent_ledger = world.get::<BandEquipment>(parent_entity);
    let parent_store = world
        .get::<PopulationCohort>(parent_entity)
        .map(|cohort| &cohort.stores);
    let (standing_items, standing_materials) = match &window.supply {
        LoadoutSupply::Parent {
            items, materials, ..
        } => (items.clone(), materials.clone()),
        LoadoutSupply::Grant { .. } => (BTreeMap::new(), BTreeMap::new()),
    };

    let mut item_delta: BTreeMap<String, i64> = BTreeMap::new();
    // Every item either side names, so a line dropped from the order still books its return.
    let item_ids: BTreeSet<&str> = wanted_items
        .keys()
        .chain(standing_items.keys())
        .map(String::as_str)
        .collect();
    for item in item_ids {
        let asked = wanted_items.get(item).copied().unwrap_or(0);
        let standing = standing_items.get(item).copied().unwrap_or(0);
        let held = parent_ledger
            .map(|ledger| ledger.count_of(item))
            .unwrap_or(0);
        let available = held.saturating_add(standing);
        if asked > available {
            return Err(LoadoutRejection::ParentCannotSupply {
                id: item.to_string(),
                asked,
                available,
            });
        }
        let onward = onward_items.get(item).copied().unwrap_or(0);
        if asked < onward {
            return Err(LoadoutRejection::OnwardTakeStranded {
                band: onward_band(loadout, band, item),
                id: item.to_string(),
                asked,
                onward,
            });
        }
        item_delta.insert(item.to_string(), i64::from(asked) - i64::from(standing));
    }

    let mut material_delta: BTreeMap<String, Scalar> = BTreeMap::new();
    let wanted_materials: BTreeMap<String, Scalar> = materials
        .iter()
        .map(|allocation| {
            (
                allocation.material_id.clone(),
                scalar_from_f32(allocation.units as f32),
            )
        })
        .collect();
    let material_ids: BTreeSet<&str> = wanted_materials
        .keys()
        .chain(standing_materials.keys())
        .map(String::as_str)
        .collect();
    for material in material_ids {
        let asked = wanted_materials
            .get(material)
            .copied()
            .unwrap_or_else(scalar_zero);
        let standing = standing_materials
            .get(material)
            .copied()
            .unwrap_or_else(scalar_zero);
        let held = parent_store
            .map(|store| store.material_total(material))
            .unwrap_or_else(scalar_zero);
        let available = held + standing;
        if asked > available {
            return Err(LoadoutRejection::ParentCannotSupply {
                id: material.to_string(),
                asked: asked.to_f32() as u32,
                available: available.to_f32() as u32,
            });
        }
        let onward = onward_materials
            .get(material)
            .copied()
            .unwrap_or_else(scalar_zero);
        if asked < onward {
            return Err(LoadoutRejection::OnwardTakeStranded {
                band: onward_band(loadout, band, material),
                id: material.to_string(),
                asked: asked.to_f32() as u32,
                onward: onward.to_f32().ceil() as u32,
            });
        }
        material_delta.insert(material.to_string(), asked - standing);
    }

    Ok(TakePlan {
        items: wanted_items
            .iter()
            .filter(|(_, units)| **units > 0)
            .map(|(item, units)| (item.clone(), *units))
            .collect(),
        materials: wanted_materials
            .into_iter()
            .filter(|(_, amount)| *amount > scalar_zero())
            .collect(),
        item_delta,
        material_delta,
    })
}

/// The lowest band whose take off `parent` names `id` — the band a stranding refusal points at, so
/// the sentence names somebody the player can go and revise.
fn onward_band(loadout: &StartingLoadout, parent: BandId, id: &str) -> u64 {
    loadout
        .iter()
        .filter(|(_, window)| window.supply.parent() == Some(parent))
        .find(|(_, window)| window.taken_units(id) > 0 || window.taken_material(id) > scalar_zero())
        .map(|(band, _)| band.0)
        .unwrap_or_default()
}

/// **Mint a grant window's allocation, both halves built from EMPTY.**
///
/// Cloning the held ledger would make a revised pick add to the draft it revises. Within one
/// allocation two kits that share an item still add — see [`expand_kits`] — and each kit's items are
/// stocked as their own batch, which is what keeps a fresh mint from averaging into a half-spent
/// pile.
fn mint_loadout(
    world: &mut World,
    entity: Entity,
    kits: &[KitAllocation],
    materials: &[MaterialAllocation],
    equipment: &crate::equipment_config::EquipmentConfig,
    recipes: &crate::recipes_config::RecipesConfig,
    materials_table: &crate::materials_config::MaterialsConfig,
) {
    let mut ledger = BandEquipment::default();
    for allocation in kits {
        let Some(definition) = equipment.kit_definition(&allocation.kit_id) else {
            continue;
        };
        for item_id in &definition.uses {
            let Some(item) = equipment.item(item_id) else {
                continue;
            };
            ledger.stock(
                item_id,
                allocation.count,
                &item.default_tier().id,
                BandEquipment::anchor_grade(recipes, materials_table, item_id),
            );
        }
    }
    world.entity_mut(entity).insert(ledger);

    if let Some(mut cohort) = world.get_mut::<PopulationCohort>(entity) {
        // **The material account only.** The band's opening food reserve shares this store, and a
        // wholesale `LocalStore::new()` here would starve it (`LocalStore::clear_materials`).
        cohort.stores.clear_materials();
        for allocation in materials {
            let Some(def) = materials_table.material(&allocation.material_id) else {
                continue;
            };
            let readings: BTreeMap<String, f32> = def
                .characteristics
                .iter()
                .map(|axis| (axis.clone(), OPENING_MATERIAL_READING))
                .collect();
            // The merge key comes from the table's own lookup, exactly as a yield edge's does — the
            // store stores, it does not interpret.
            let Some(key) = materials_table.band_key(&allocation.material_id, &readings) else {
                continue;
            };
            cohort.stores.deposit_material(
                &allocation.material_id,
                key,
                scalar_from_f32(allocation.units as f32),
                &readings,
            );
        }
    }
}

/// **Move a validated take's delta between the parent's ledger and the band's**, in whichever
/// direction each line points.
///
/// Only the delta crosses, which is the whole reason a revision does not have to be expressed as
/// *"hand everything back, then take the new order"*: the two are algebraically the same move, and
/// this one never puts the world through a transient state a refusal would have to undo.
///
/// Both directions go through [`BandEquipment::take_units`], so the **freshest units move** whichever
/// way they are going — one rule about which unit is being handed over, not a second one for the
/// return leg.
fn move_take(world: &mut World, entity: Entity, parent_entity: Entity, plan: &TakePlan) {
    for (item, delta) in &plan.item_delta {
        if *delta == 0 {
            continue;
        }
        let (from, to, units) = if *delta > 0 {
            (parent_entity, entity, *delta as u32)
        } else {
            (entity, parent_entity, delta.unsigned_abs() as u32)
        };
        let taken = world
            .get_mut::<BandEquipment>(from)
            .map(|mut ledger| ledger.take_units(item, units))
            .unwrap_or_default();
        if taken.is_empty() {
            continue;
        }
        if let Some(mut ledger) = world.get_mut::<BandEquipment>(to) {
            ledger.place_batches(item, taken);
        } else {
            let mut ledger = BandEquipment::default();
            ledger.place_batches(item, taken);
            world.entity_mut(to).insert(ledger);
        }
    }

    for (material, delta) in &plan.material_delta {
        if *delta == scalar_zero() {
            continue;
        }
        let (from, to, amount) = if *delta > scalar_zero() {
            (parent_entity, entity, *delta)
        } else {
            (entity, parent_entity, scalar_zero() - *delta)
        };
        let drawn = world
            .get_mut::<PopulationCohort>(from)
            .map(|mut cohort| cohort.stores.take_material_batches(material, amount))
            .unwrap_or_default();
        if drawn.is_empty() {
            continue;
        }
        if let Some(mut cohort) = world.get_mut::<PopulationCohort>(to) {
            for draw in drawn {
                cohort.stores.deposit_material(
                    material,
                    draw.band,
                    draw.amount,
                    &draw.characteristics,
                );
            }
        }
    }
}

/// **The entity carrying `band` for `faction`** — the band a loadout outfits, addressed by its own
/// durable id rather than by a marker.
fn band_entity(world: &mut World, faction: FactionId, band: BandId) -> Option<Entity> {
    let mut query = world.query::<(Entity, &BandId, &PopulationCohort)>();
    query
        .iter(world)
        .find(|(_, id, cohort)| **id == band && cohort.faction == faction)
        .map(|(entity, _, _)| entity)
}
