//! **The TOE** (`data/equipment.json`) — the consumable equipment that lifts a band's hunting,
//! hauling and gathering roles from their *unequipped* to their *equipped* tier.
//!
//! Design: `docs/plan_early_game_labor.md` → "Equipment / TOE" (the authoritative arc) and
//! `docs/plan_hunt_through_combat.md` §4.8 (why a minimal TOE has to land before the hunt resolves
//! through combat: a bare-handed hunter's `attack` is `1`, below every megafauna's `defense`, so
//! without a spear the gate is the entire game).
//!
//! # Two nouns, and they are not the same
//!
//! An **item** ([`ItemDefinition`]) is a piece of equipment that owns what it does ([`EquipmentEffect`]),
//! how long it lasts (`starting_durability`) and what wears it ([`WearQuantum`]). A **kit**
//! ([`KitDefinition`]) is a roster entry that *lists* the items a party is sent out with. The items
//! used to be called `hunting_kit` / `sled_kit` / `basket_kit`, which gave both concepts the same word
//! and made a kit look like a leaf; "kit" now means only the roster entry.
//!
//! | item | declares |
//! |---|---|
//! | **spears** | `attack` (equipped), plus `dispersion`/`exposure` at their neutral `1.0` — see [`KitChoice::multiplier`] for why declaring the neutral is load-bearing |
//! | **sled** | `hunt_carry` (unequipped) — a carcass is one lumpy object you *drag* out whole |
//! | **baskets** | `forage_carry` (unequipped) — berries are loose, divisible, bounded by what you can hold |
//! | **traps** — the passive device | `attack` bounded by `max_body_mass`, and `dispersion`/`exposure` at `0` — set down and walked away from, so nothing bolts and nobody is hurt |
//!
//! # Four rules this module exists to keep
//!
//! 1. **Two tiers, never a taper.** An item's performance is *flat* until it expires and then the role
//!    **steps down**. Durability and performance are deliberately orthogonal axes — coupling them (an
//!    item that gets worse as it wears) would be the modeling mistake the arc calls out, and would let
//!    a future crafting economy tune only one thing. [`EquipmentEffect`] has no representation for a
//!    taper, which is what makes this structural rather than a convention.
//! 2. **Wear is charged for USE, never for turns elapsed** (`docs/plan_denial_raid.md` §1.2). A
//!    turn-based clock charges an idle march the same as a slaughter, which would make denial free.
//!    [`WearQuantum`] has no `Turn` variant, and each item's quantum is charged at its own site, so
//!    two items cannot cross-charge.
//! 3. **One home per fact.** Which *tier* an effect declares is this rule showing through: the other
//!    tier already had a home and keeps it. A bare hand's attack is [`crate::creatures_config`]'s
//!    `person.combat.attack` (`1.0`), so `spears` declares the **equipped** side; a hunt's per-hunter
//!    haul rate is `labor_config.json`'s `hunt.per_worker_biomass_capacity` (`40.0`) and a gatherer's
//!    its `forage.per_worker_biomass_capacity` (`8.0`) — both of which **are** the equipped tiers,
//!    because the shipped game has always run kitted — so the two carry items declare the
//!    **unequipped** side. No shipped number gets a second home to drift from.
//! 4. **A kit is a MASK, and the condition test cannot be read without it.**
//!    `effective(item) = kit uses it AND the band still has condition in it`, which is
//!    [`KitChoice::item_live`] and nothing else. [`crate::components::BandEquipment`]'s own condition
//!    test is crate-private precisely so a caller cannot read the condition alone and silently re-arm
//!    a party sent out bare.
//!
//! **Start-stocked and NOT craftable.** There is no replenishment path in this slice; running dry is
//! the intended pressure. The band's state is *wear*, not *stock*, so a freshly spawned
//! [`crate::components::BandEquipment`] is a full kit by construction (`Default` = zero wear) and no
//! spawn site needs to read this config. **Quality tiers** (flint against bronze spears) are
//! deliberately absent for the same reason inverted: nothing can craft one, so the structure would
//! ship with no way to exercise it. Both ride the crafting slice.
//!
//! Loader mirrors [`crate::creatures_config`]: baked-in builtin + `EQUIPMENT_CONFIG_PATH` override +
//! [`EquipmentConfig::validate`] inside `from_json_str`, so **every** load path is validated and a
//! present-but-broken file is a boot panic rather than a silent fallback
//! (`.claude/rules/core_sim/config-loading.md`).

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::combat::CombatStats;
use crate::config_load::{load_config_from_env, ConfigLoadError};

pub const BUILTIN_EQUIPMENT_CONFIG: &str = include_str!("data/equipment.json");

/// **A stat a piece of equipment can set.** The variants are the JSON `stat` keys.
///
/// **Which tier an effect declares is not free choice — it is one-home-per-fact showing through.**
/// For [`EquipmentStat::Attack`] the *unequipped* value already had a home (`creatures.json`'s
/// `person.combat.attack`), so an item declares the **equipped** side; for the two carries the
/// *equipped* rates are `labor_config.json`'s, so an item declares the **unequipped** side. The three
/// stats introduced with the effects model are **neutral at `1.0`**, so an item that never mentions
/// one changes nothing about it — which is what makes the whole generalization a no-op for the
/// shipped roster.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentStat {
    /// A hunter's per-unit `attack` — the left side of §4.2's gate `max(0, attack − defense)`.
    /// Declared **equipped**; the bare hand's `1.0` is the `person` roster row.
    Attack,
    /// The per-hunter **hunt** haul rate. Declared **unequipped**; the sledded `40.0` is
    /// `labor_config.json`'s `hunt.per_worker_biomass_capacity`.
    HuntCarry,
    /// The per-gatherer throughput before the tile's seasonal weight. Declared **unequipped**; the
    /// basketed `8.0` is `labor_config.json`'s `forage.per_worker_biomass_capacity`.
    ForageCarry,
    /// **Multiplies the quarry's own `wariness`** — `effective_wariness = clamp(wariness × dispersion,
    /// 0, 1)`. Neutral at `1.0`; a trap ships `0.0`. A multiplier rather than a subtraction so the
    /// *species* decides how much a noisy approach costs, which is what lets one spear line scatter a
    /// warren and contain a mammoth with no per-target authoring.
    Dispersion,
    /// **Multiplies the hunt's baseline injury hazard** (`fauna::hunt_injuries`). Neutral at `1.0`; a
    /// stand-off instrument ships `0.0` and wears out instead of its users getting hurt.
    Exposure,
    /// **The per-keeper rate a PEN is collected at.** Declared **unequipped**; the equipped `40.0` is
    /// `labor_config.json`'s `hunt.per_worker_biomass_capacity` — the number the pen harvest has
    /// always run on, so a keeper carrying husbandry gear collects exactly what it always did.
    ///
    /// **A separate stat from [`Self::HuntCarry`], and that is the physical claim `one item, one
    /// job` already makes twice.** A sled drags a carcass in off the range; a pen stands at the
    /// camp, and what bounds a slaughter there is the handling gear — hurdles to work the beast into,
    /// something to butcher onto, vessels to carry it in. A party that brought a sled to a pen has
    /// the wrong tool, exactly as a party that brought baskets to a deer does.
    PenCarry,
    /// **The sight range each posted scout vantage reveals at.** Declared **unequipped**; the
    /// equipped `2` is `labor_config.json`'s `scout.vantage_range`.
    ///
    /// It lifts the vantage's *range* rather than how far out it is posted, because the posting
    /// distance is three dials (`vantage_distance_base` / `_per_scout` / `_max`) and a kit that moved
    /// one of them would be a fourth authority over the same line. What wayfinding gear buys is what
    /// an observer can make out once they are there.
    ScoutVantageRange,
}

impl EquipmentStat {
    /// The neutral value — what the stat reads when **no** item declares it. Only the multiplier
    /// stats have one; the tiered stats resolve against a rate the caller already holds, so asking
    /// for their neutral value is a category error the type refuses to answer.
    pub fn neutral(self) -> Option<f32> {
        match self {
            EquipmentStat::Dispersion | EquipmentStat::Exposure => Some(1.0),
            EquipmentStat::Attack
            | EquipmentStat::HuntCarry
            | EquipmentStat::ForageCarry
            | EquipmentStat::PenCarry
            | EquipmentStat::ScoutVantageRange => None,
        }
    }

    /// **The stats resolved through [`EquipmentConfig::two_tier`]** — the ones whose *unequipped*
    /// side is declared here and whose equipped side lives in `labor_config.json`. `two_tier`'s
    /// fallback searches the **whole item table** and takes the first match, so each of these may be
    /// declared by at most one item or the answer would resolve by `BTreeMap` order (i.e.
    /// alphabetically). Named once, here, so `validate` cannot fall behind a new stat.
    pub const TWO_TIER: [EquipmentStat; 4] = [
        EquipmentStat::HuntCarry,
        EquipmentStat::ForageCarry,
        EquipmentStat::PenCarry,
        EquipmentStat::ScoutVantageRange,
    ];
}

/// **Which tier of a stat an effect declares** — exactly one, never both, because the other tier
/// already has a home elsewhere and copying it here would give a shipped number a second home to
/// drift from. Flattened into the effect, so the JSON reads `{ "stat": "attack", "equipped": 20.0 }`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectTier {
    /// The value the stat takes **while the item is intact**.
    Equipped(f32),
    /// The value the stat falls back to **once the item is spent**.
    Unequipped(f32),
}

impl EffectTier {
    /// The declared number, whichever side it describes — for validation, which cares only that it
    /// is a real quantity.
    pub fn value(self) -> f32 {
        match self {
            EffectTier::Equipped(value) | EffectTier::Unequipped(value) => value,
        }
    }
}

/// One entry in an item's `effects` list: a stat, the value it takes, and optionally **the size of
/// quarry it applies to**.
///
/// **An effect names a VALUE, never a delta or a multiplier stacking on something else.** That is
/// what keeps *flat until expiry, then a step down* structurally true rather than a rule a future
/// author has to remember — there is no representation for "a bit worse as it wears".
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EquipmentEffect {
    pub stat: EquipmentStat,
    #[serde(flatten)]
    pub tier: EffectTier,
    /// **The lightest quarry this effect reaches** — a bow is poor against something small and fast.
    /// Absent = no lower bound. Reserved for the ranged work on #501; nothing ships one yet.
    #[serde(default)]
    pub min_body_mass: Option<f32>,
    /// **The heaviest quarry this effect reaches.** A snare holds a hare and not a deer, so `traps`
    /// ships `1.0`.
    ///
    /// **This is what stops a flat `attack` from making a trap universal**, and it is the correction
    /// to a real mistake: `dispersion` answers *does the animal bolt before you reach it*, which is a
    /// different question from *what can this thing physically hold*. Collapsing the two made traps
    /// take Red Deer, because `attack 8` clears a deer's `defense 1` like everything else's.
    ///
    /// **It reads `body_mass`, which the roster already authors — not a size CATEGORY.** A
    /// `size_class` here would be a second authority to drift from the masses, exactly as
    /// `dispersion` reads `wariness` rather than a "jumpy" flag. The shipped roster separates
    /// cleanly: every `defense 0` row is `0.13..=0.67` and the next species up is a Desert Gazelle at
    /// `3.3`, so any ceiling in that gap behaves identically and `1.0` is the round number in it.
    #[serde(default)]
    pub max_body_mass: Option<f32>,
}

impl EquipmentEffect {
    /// **Does this effect reach quarry of this mass?** Unbounded effects reach everything, which is
    /// what keeps every shipped item but `traps` byte-identical.
    pub fn reaches(&self, body_mass: f32) -> bool {
        self.min_body_mass.is_none_or(|min| body_mass >= min)
            && self.max_body_mass.is_none_or(|max| body_mass <= max)
    }

    /// Whether this effect names a size of quarry at all.
    pub fn is_mass_bounded(&self) -> bool {
        self.min_body_mass.is_some() || self.max_body_mass.is_some()
    }
}

/// **What one use of an item IS** — the quantum wear is charged against.
///
/// `docs/plan_denial_raid.md` §1.2 depends on there being no turn variant: a turn clock charges an
/// idle march the same as a slaughter, which makes denial free. Each quantum is charged at its own
/// site, so two items on different quanta cannot cross-charge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WearQuantum {
    /// Per animal brought down. Spears and traps.
    Kill,
    /// Per unit of biomass hauled home from a hunt. The sled.
    BiomassHauled,
    /// Per unit of biomass gathered. Baskets.
    BiomassGathered,
    /// Per unit of biomass **butchered** at a pen — what the slaughter put on the ground, not what
    /// the keeper got home ([`crate::fauna::AnimalTake::killed_biomass`]). Husbandry gear.
    ///
    /// **Its own quantum rather than [`Self::BiomassHauled`], and over a different number.** A pen
    /// harvest charges both, because the sled is being *dragged* and the handling gear is being
    /// *worked* — but the gear is worked on the **whole beast** brought out of the pen and killed,
    /// while the sled only ever drags home the fraction the crew could carry. The two coincide on
    /// every pen a keeper can seat whole and part on the ones it cannot, which is where charging
    /// this over `carried` under-priced exactly the animal the gear did the most work on.
    ///
    /// Keeping the quanta apart is *separately* what stops a band that only keeps pens from blunting
    /// a sled it never took onto the range, and what lets either life be retuned without moving the
    /// other.
    BiomassCollected,
    /// Per **tile revealed for the first time**. Wayfinding gear.
    ///
    /// **First time, not tile-seen** — a band parked in explored ground re-sees the same ring every
    /// turn, so charging per tile *seen* would be a turn clock wearing a per-use costume, which is
    /// exactly what `docs/plan_denial_raid.md` §1.2 forbids. What wears wayfinding gear is going
    /// somewhere new.
    TileRevealed,
    /// Per **fight resolved**. Warrior weapons.
    ///
    /// **Per engagement, not per casualty inflicted.** A defence that killed nothing was still
    /// fought, and pricing the kit on its results would make a band that is losing pay less. A band
    /// nobody raided pays nothing, and a band three packs turned on pays three — a use count, not a
    /// clock.
    Fight,
}

/// An item's use quantum and what one use costs it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WearConfig {
    /// What counts as one use.
    pub per: WearQuantum,
    /// Condition spent per use, on the shared 0–100 scale.
    pub amount: f32,
}

/// **One piece of equipment.** It owns what it does (`effects`), how long it lasts
/// (`starting_durability`) and what wears it (`wear`) — the three axes the design keeps orthogonal.
///
/// Quality tiers (flint vs bronze spears) are deliberately **absent**: nothing can craft one, so a
/// tier here would be a data model with no gameplay behind it. They ride the crafting slice.
///
/// **`PartialEq` is what lets the designer catalogue's round-trip test compare the whole item table
/// at once** rather than field by field — a hand-listed comparison goes stale the moment an item
/// gains an axis, which is the failure `equipmentConfigJson` exists to make impossible.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemDefinition {
    /// Condition a fresh item carries, on the shared 0–100 scale. A band is equipped while its
    /// accumulated wear is **strictly below** this.
    pub starting_durability: f32,
    /// The use quantum and its cost.
    pub wear: WearConfig,
    /// What this item sets while it is intact.
    pub effects: Vec<EquipmentEffect>,
}

impl ItemDefinition {
    /// The tier this item declares for `stat`, or `None` if it does not touch it.
    pub fn effect(&self, stat: EquipmentStat) -> Option<EffectTier> {
        self.effects
            .iter()
            .find(|effect| effect.stat == stat)
            .map(|effect| effect.tier)
    }
}

// **The retired per-item blocks.** `HuntingKitConfig` / `SledKitConfig` / `BasketKitConfig` each
// held one hand-named field (`equipped_attack`, `unequipped_per_worker_biomass_capacity`, …), which
// is why no item could ever touch two stats — the *shape* said one item, one number, not an
// oversight. [`ItemDefinition`] replaces all three, and `KitComponent` — whose three variants *were*
// the JSON block keys — is replaced by the item id, resolved against the table.
//
// **That trade loses a parse-time guarantee and validate must pay it back.** A `uses` entry naming a
// component that had no block used to fail to *deserialize*, for free, because the enum could not
// represent it. An item id is a string, so nothing stops the file naming `spearz`; the check moved
// to [`EquipmentConfig::validate`] (`UnknownItem`), which every load path runs.

/// **A job a kit may be sent out on** — the four labor roles that resolve a tier off the TOE.
///
/// **The two band-wide roles are here now, and that is this slice's whole shape change.** They used
/// to be absent on the grounds that they consumed no component: `LaborTarget::kit_job` answered
/// `None` for Scout and Warrior, so `LaborAssignment.kitId` published `""` and neither role had a
/// tier to step down from. Both have live consumers — scouts post forward-observer vantages in
/// `calculate_visibility`, warriors are the band's defending contingent in
/// `advance_predator_raids` — so "consumes no component" was a statement about the roster, not about
/// the sim, and it stopped being true the moment the roster carried gear for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KitJob {
    Hunt,
    Forage,
    Scout,
    Warrior,
}

impl KitJob {
    /// Every job, for the validations and the wire — one list, so a new job cannot be validated in
    /// three places and forgotten in a fourth.
    pub const ALL: [KitJob; 4] = [KitJob::Hunt, KitJob::Forage, KitJob::Scout, KitJob::Warrior];

    /// The wire/command token for this job — the same string `assign_labor`'s role token uses (and
    /// the same string [`crate::components::LaborTarget::kind`] answers), so a kit's `jobs` list and
    /// a labor role are compared in one language.
    pub fn as_str(self) -> &'static str {
        match self {
            KitJob::Hunt => "hunt",
            KitJob::Forage => "forage",
            KitJob::Scout => "scout",
            KitJob::Warrior => "warrior",
        }
    }
}

/// One roster entry: a **named mask** over the item table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KitDefinition {
    /// Stable id — what a command names and what the wire carries. Unique across the roster.
    pub id: String,
    /// Player-facing label. The sim never branches on it.
    pub display_name: String,
    /// Which verbs this kit may be sent on. A kit named for a job outside this list is a **command
    /// failure**, never a silent fall back to the job's default.
    pub jobs: Vec<KitJob>,
    /// The items this kit actually puts in the party's hands, by id. **Empty is an ordinary roster
    /// entry** (the shipped `none`), not a sentinel: it grants nothing, so every predicate reads
    /// false and — because wear rides the same predicate — nothing is spent either.
    ///
    /// Validated to name real items ([`EquipmentConfigError::UnknownItem`]); the retired
    /// `KitComponent` enum got that for free at parse time and an id cannot.
    pub uses: Vec<String>,
}

/// The kit each verb reaches for when the player names none. Validated to name a real roster entry
/// whose `jobs` covers that verb, so [`EquipmentConfig::default_kit`] cannot fail after load.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultKitsConfig {
    pub hunt: String,
    pub forage: String,
    pub scout: String,
    pub warrior: String,
}

/// **What the kit is being resolved AGAINST** — the argument a mass-bounded effect is tested on.
///
/// It is an enum rather than an `Option<f32>` so the two readings cannot be confused at a call site.
/// [`Quarry::Any`] is *"nothing specific is in view — give me the best this kit can do"*, which is
/// the honest answer for a **display** surface with no target (the published kit roster, a band's own
/// `hunterAttack` row). A **take** path must never pass it: it would hand a trapping party its
/// small-game attack against a mammoth, which is precisely the bug the bound exists to prevent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Quarry {
    /// A specific animal, by `body_mass`.
    Mass(f32),
    /// No quarry in view. Every bounded effect counts.
    Any,
}

impl Quarry {
    /// Does this quarry fall inside the effect's size bounds?
    fn within(self, effect: &EquipmentEffect) -> bool {
        match self {
            Quarry::Mass(mass) => effect.reaches(mass),
            Quarry::Any => true,
        }
    }
}

/// **A chosen kit, resolved once against the roster** — an id plus the set of items it stands for.
///
/// It is the **only** way anything asks "is this gear serving?": [`Self::item_live`] is
/// `kit uses the item AND the band still has condition in it`, and
/// [`crate::components::BandEquipment`]'s own condition test is crate-private so a second reading
/// cannot appear beside it. A caller that read the condition alone would silently re-arm a party
/// sent out bare.
///
/// **Resolved once, then carried.** A detached party stores its choice at launch and prices its
/// whole life from it — re-resolving against the band's current stock would silently re-arm a party
/// sent out bare the moment the band's spears were counted again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KitChoice {
    id: Arc<str>,
    uses: Arc<[Arc<str>]>,
}

impl KitChoice {
    /// The roster id this choice was resolved from — what the command named and what the wire carries.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The item ids this kit puts in the party's hands, whatever condition they are in.
    pub fn uses(&self) -> impl Iterator<Item = &str> {
        self.uses.iter().map(|item| item.as_ref())
    }

    /// **Is this item serving this party?** The mask, and then the band's condition — a kit that does
    /// not carry spears is bare-handed however fresh the band's are, and a kit that does still steps
    /// down when they wear out.
    pub fn item_live(
        &self,
        item: &str,
        wear: &crate::components::BandEquipment,
        config: &EquipmentConfig,
    ) -> bool {
        self.uses.iter().any(|used| used.as_ref() == item) && wear.has_condition(item, config)
    }

    /// The kit's items that are **still serving**, paired with their definitions — the one iteration
    /// every stat resolution runs over.
    fn live_items<'a>(
        &'a self,
        wear: &'a crate::components::BandEquipment,
        config: &'a EquipmentConfig,
    ) -> impl Iterator<Item = &'a ItemDefinition> {
        self.uses.iter().filter_map(move |item| {
            if wear.has_condition(item, config) {
                config.item(item)
            } else {
                None
            }
        })
    }

    /// **Resolve a MULTIPLIER stat across the kit — the maximum of what its live items DECLARE.**
    ///
    /// Two clauses, both load-bearing:
    ///
    /// - **Only declared values participate.** An item that says nothing about a stat contributes
    ///   nothing, rather than contributing the neutral `1.0`. Without that a sled — carry gear nobody
    ///   approaches an animal with — would drag a trapping party's `dispersion` back up to `1.0`
    ///   simply by being in the kit, and traps would never work.
    /// - **The MAXIMUM, not the minimum**, for the stats that describe *how the party hunts*. If you
    ///   are also running up and throwing spears, you are scaring the herd and you are in reach of it
    ///   however many traps you also set. This is why `spears` declares `dispersion 1.0` and
    ///   `exposure 1.0` explicitly even though both are the neutral value: the declaration is what
    ///   makes a hypothetical spears-and-traps kit resolve to *loud and exposed* instead of
    ///   inheriting the trap's stand-off for free.
    ///
    /// Neutral when nothing declares it, so a kit of pure carry gear leaves the shipped fight alone.
    pub fn multiplier(
        &self,
        stat: EquipmentStat,
        wear: &crate::components::BandEquipment,
        config: &EquipmentConfig,
    ) -> f32 {
        let neutral = stat
            .neutral()
            .expect("multiplier() is for the neutral-at-1.0 stats");
        self.live_items(wear, config)
            .filter_map(|item| item.effect(stat))
            .map(EffectTier::value)
            .fold(None::<f32>, |best, value| {
                Some(best.map_or(value, |best| best.max(value)))
            })
            .unwrap_or(neutral)
    }

    /// **Is any live item in this kit declaring an EQUIPPED tier for `stat`?** — the predicate the
    /// two-tier stats resolve on. `attack` reads it directly; the carries read its inverse, because
    /// they declare the *unequipped* side (see [`EquipmentStat`]).
    fn declares_equipped(
        &self,
        stat: EquipmentStat,
        wear: &crate::components::BandEquipment,
        config: &EquipmentConfig,
        quarry: Quarry,
    ) -> Option<f32> {
        self.live_items(wear, config)
            .filter_map(|item| {
                item.effects
                    .iter()
                    .filter(|effect| effect.stat == stat && quarry.within(effect))
                    .find_map(|effect| match effect.tier {
                        EffectTier::Equipped(value) => Some(value),
                        EffectTier::Unequipped(_) => None,
                    })
            })
            .fold(None::<f32>, |best, value| {
                Some(best.map_or(value, |best| best.max(value)))
            })
    }

    /// **Is a live item in this kit supplying `stat` at all?** — for the two carries, whose items
    /// declare the *unequipped* side, so "supplied" means the equipped rate applies.
    fn supplies(
        &self,
        stat: EquipmentStat,
        wear: &crate::components::BandEquipment,
        config: &EquipmentConfig,
    ) -> bool {
        self.live_items(wear, config)
            .any(|item| item.effect(stat).is_some())
    }
}

/// Why a named kit cannot be sent on a verb. Both readings are **command failures with a reason** —
/// a bad selection must never quietly become the default, because the whole point of naming a kit is
/// that the player is comparing tiers.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum KitSelectionError {
    #[error("unknown kit '{id}' — the roster offers {available}")]
    Unknown { id: String, available: String },
    #[error("the {display_name} cannot be sent on a {job} — it is a {jobs} kit")]
    WrongJob {
        display_name: String,
        job: &'static str,
        jobs: String,
    },
}

/// Root TOE configuration: one block per shipped **component**, plus the **roster** of named kits
/// that mask over them. **One kit, one job** — nothing here composes two roles onto one block, which
/// is what makes the cross-checks in this module's tests (baskets do not touch the hunt; the sled
/// does not touch foraging) statements about the *type*, not about a convention someone has to
/// remember.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquipmentConfig {
    /// **Every piece of equipment, by id.** A `BTreeMap` rather than a `HashMap` so `items()` has a
    /// stable order — the wire's item list and any refusal message that enumerates ids must not
    /// reshuffle between runs.
    pub items: std::collections::BTreeMap<String, ItemDefinition>,
    /// The named kits a party may be sent out with. See [`KitDefinition`].
    pub kits: Vec<KitDefinition>,
    /// What each verb reaches for when the player names none.
    pub default_kits: DefaultKitsConfig,
}

impl EquipmentConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            Self::from_json_str(BUILTIN_EQUIPMENT_CONFIG)
                .expect("builtin equipment config should parse and validate"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, EquipmentConfigError> {
        let config: EquipmentConfig = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

    pub fn from_file(path: &Path) -> Result<Self, EquipmentConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| EquipmentConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        EquipmentConfig::from_json_str(&contents)
    }

    /// Every roster entry, in file order — the client's picker list.
    pub fn kits(&self) -> &[KitDefinition] {
        &self.kits
    }

    /// The roster entry with this id, or `None`.
    pub fn kit_definition(&self, id: &str) -> Option<&KitDefinition> {
        self.kits.iter().find(|kit| kit.id == id)
    }

    /// Resolve a roster id into the mask it stands for. `None` for an id the roster does not carry.
    pub fn kit(&self, id: &str) -> Option<KitChoice> {
        self.kit_definition(id).map(Self::choice_from)
    }

    fn choice_from(definition: &KitDefinition) -> KitChoice {
        KitChoice {
            id: Arc::from(definition.id.as_str()),
            uses: definition
                .uses
                .iter()
                .map(|item| Arc::from(item.as_str()))
                .collect(),
        }
    }

    /// **The empty kit** — carries nothing, so every predicate reads false and a party runs at every
    /// unequipped tier, spending no durability on anything.
    ///
    /// **Synthetic rather than a lookup of the roster's `none`**, deliberately: the roster is config
    /// and a file is free to drop that entry, but "this crew carries no kit" is a state the sim
    /// reaches on its own — a band-wide role like Scout or Warrior has no kit axis at all. Resolving
    /// it through the roster would make a config edit able to panic the labor loop. Its id is empty,
    /// which is exactly what `LaborAssignment.kitId` already publishes for a row with no kit axis.
    pub fn no_kit(&self) -> KitChoice {
        KitChoice {
            id: Arc::from(""),
            uses: Arc::from(Vec::new()),
        }
    }

    /// Every item, in id order — the stable iteration the wire and the refusal messages ride.
    pub fn items(&self) -> impl Iterator<Item = (&str, &ItemDefinition)> {
        self.items.iter().map(|(id, item)| (id.as_str(), item))
    }

    /// The item with this id, or `None`.
    pub fn item(&self, id: &str) -> Option<&ItemDefinition> {
        self.items.get(id)
    }

    /// **The item that declares `stat`, whatever kit is in play** — how a two-tier stat finds its
    /// *unequipped* value even when the kit carrying it is absent or spent. Deliberately searches the
    /// whole table rather than the kit: a party with no sled still needs to know what a sledless haul
    /// rate is, and that number lives on the sled.
    fn declared_tier(&self, stat: EquipmentStat) -> Option<EffectTier> {
        self.items.values().find_map(|item| item.effect(stat))
    }

    /// The id this verb's default kit carries.
    pub fn default_kit_id(&self, job: KitJob) -> &str {
        match job {
            KitJob::Hunt => &self.default_kits.hunt,
            KitJob::Forage => &self.default_kits.forage,
            KitJob::Scout => &self.default_kits.scout,
            KitJob::Warrior => &self.default_kits.warrior,
        }
    }

    /// **What a verb runs on when the player names no kit.** Infallible after load: `validate`
    /// rejects a default that is not a real roster entry covering its own job, so a broken roster is
    /// a boot panic rather than a resolution that has to be handled at every call site.
    pub fn default_kit(&self, job: KitJob) -> KitChoice {
        self.kit(self.default_kit_id(job))
            .expect("validate guarantees every default kit names a roster entry")
    }

    /// **The command boundary's one resolution.** `None` = the player named no kit, which is the
    /// job's default; a named kit must exist *and* list this job, and anything else is an error the
    /// caller reports rather than a quiet fall back to the default.
    pub fn resolve_kit_for_job(
        &self,
        id: Option<&str>,
        job: KitJob,
    ) -> Result<KitChoice, KitSelectionError> {
        let Some(id) = id else {
            return Ok(self.default_kit(job));
        };
        let Some(definition) = self.kit_definition(id) else {
            return Err(KitSelectionError::Unknown {
                id: id.to_string(),
                available: self.kit_ids_for_message(),
            });
        };
        if !definition.jobs.contains(&job) {
            return Err(KitSelectionError::WrongJob {
                display_name: definition.display_name.clone(),
                job: job.as_str(),
                jobs: definition
                    .jobs
                    .iter()
                    .map(|job| job.as_str())
                    .collect::<Vec<_>>()
                    .join("/"),
            });
        }
        Ok(Self::choice_from(definition))
    }

    /// The roster's ids, for a refusal message — a player who mistypes a kit is told what there is.
    fn kit_ids_for_message(&self) -> String {
        self.kits
            .iter()
            .map(|kit| kit.id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// **A hunter's per-unit combat profile, kit composed in** — `intrinsic ⊕ loadout`, the
    /// composition `docs/plan_predators.md` names and the *one* seam any consumer resolves a hunter's
    /// `attack` through.
    ///
    /// The kit swaps the **whole attack tier** (never adds to it): the unequipped number is the
    /// `person` row's own `attack`, so returning `intrinsic` unchanged is exactly what "dropped to the
    /// unequipped tier" means, and the step between the two is the cliff.
    ///
    /// **It takes the kit and the wear rather than a resolved `bool`**, because the equipped tier is
    /// no longer one config field: it is whatever the kit's *live* items declare, and the best of them
    /// wins. A caller cannot pre-compute that without knowing which items carry `attack`, which is
    /// exactly the knowledge the effects table exists to keep out of call sites.
    ///
    /// **Defense, range and wariness are untouched here** — a weapon is a weapon. Armour is the
    /// Warrior role's kit, which this slice deliberately does not ship, and the retreat's `wariness` is
    /// the *quarry's*, moved by [`EquipmentStat::Dispersion`] at the retreat rather than here.
    /// **Against a SPECIFIC animal** — the only form a take or a forecast may use, because a
    /// mass-bounded weapon is only a weapon against quarry it reaches. A trapping party handed a
    /// mammoth resolves to the bare hand's `attack`, and `max(0, 1 − 12)` is the gate refusing it.
    pub fn hunter_profile_against(
        &self,
        intrinsic: CombatStats,
        kit: &KitChoice,
        wear: &crate::components::BandEquipment,
        body_mass: f32,
    ) -> CombatStats {
        self.hunter_profile_for(intrinsic, kit, wear, Quarry::Mass(body_mass))
    }

    /// **With no quarry in view** — the best this kit can do against *something*. For DISPLAY only
    /// (the published kit roster, a band's own `hunterAttack` row): both are facts about the kit, not
    /// about a hunt, and neither has an animal to ask about. Named apart from
    /// [`Self::hunter_profile_against`] so a take path cannot reach it by leaving an argument off.
    pub fn hunter_profile_unbounded(
        &self,
        intrinsic: CombatStats,
        kit: &KitChoice,
        wear: &crate::components::BandEquipment,
    ) -> CombatStats {
        self.hunter_profile_for(intrinsic, kit, wear, Quarry::Any)
    }

    fn hunter_profile_for(
        &self,
        intrinsic: CombatStats,
        kit: &KitChoice,
        wear: &crate::components::BandEquipment,
        quarry: Quarry,
    ) -> CombatStats {
        match kit.declares_equipped(EquipmentStat::Attack, wear, self, quarry) {
            Some(attack) => CombatStats {
                attack,
                ..intrinsic
            },
            None => intrinsic,
        }
    }

    /// **A band's per-worker HUNT haul rate** — resolved against the equipped rate the caller already
    /// holds (`labor_config.hunt.per_worker_biomass_capacity`). The single seam every hunt-take,
    /// crew-size and hunt-forecast site reads, so the assign-time seed and the resolved row can never
    /// disagree about which tier a band is on.
    ///
    /// **Baskets cannot reach this by construction** — they declare [`EquipmentStat::ForageCarry`],
    /// a different stat, so dragging a carcass stays unrelated to how much you can hold (§4.8).
    pub fn hunt_per_worker_biomass_capacity(
        &self,
        equipped_rate: f32,
        kit: &KitChoice,
        wear: &crate::components::BandEquipment,
    ) -> f32 {
        self.two_tier(EquipmentStat::HuntCarry, equipped_rate, kit, wear)
    }

    /// **A band's per-worker GATHER throughput** — resolved against the equipped rate the caller
    /// already holds (`labor_config.forage.per_worker_biomass_capacity`), before the tile's seasonal
    /// weight is folded in ([`crate::forage::forage_per_worker_biomass`]).
    ///
    /// **The sled cannot reach this by construction** — it declares [`EquipmentStat::HuntCarry`].
    pub fn forage_per_worker_biomass_capacity(
        &self,
        equipped_rate: f32,
        kit: &KitChoice,
        wear: &crate::components::BandEquipment,
    ) -> f32 {
        self.two_tier(EquipmentStat::ForageCarry, equipped_rate, kit, wear)
    }

    /// **A band's per-keeper PEN collection rate** — resolved against the equipped rate the caller
    /// already holds, which is the same `labor_config.hunt.per_worker_biomass_capacity` the pen
    /// harvest has always been capped by. A keeper carrying husbandry gear therefore collects
    /// **exactly what a pen always collected**; what is new is the state below the cliff.
    ///
    /// **The sled cannot reach this by construction**, and that is the deliberate consequence: a
    /// hunting party that has corralled a herd and left its assignment on the big-game kit is
    /// working the pen with a drag harness and no handling gear, and collects at the bare rate. That
    /// is the same shape as bringing baskets to a deer — see [`EquipmentStat::PenCarry`].
    pub fn pen_per_worker_biomass_capacity(
        &self,
        equipped_rate: f32,
        kit: &KitChoice,
        wear: &crate::components::BandEquipment,
    ) -> f32 {
        self.two_tier(EquipmentStat::PenCarry, equipped_rate, kit, wear)
    }

    /// **The sight range a band's posted scout vantages reveal at** — resolved against the equipped
    /// range the caller already holds (`labor_config.scout.vantage_range`).
    ///
    /// Returned as `f32` like every other tier and rounded by the caller, because the *config* axis
    /// is a distance in tiles and the *effects* axis is a quantity: giving this one stat an integer
    /// type would make it the only effect a designer could not tune continuously.
    pub fn scout_vantage_range(
        &self,
        equipped_range: f32,
        kit: &KitChoice,
        wear: &crate::components::BandEquipment,
    ) -> f32 {
        self.two_tier(EquipmentStat::ScoutVantageRange, equipped_range, kit, wear)
    }

    /// **A warrior's per-head combat profile, kit composed in** — the defending contingent's side of
    /// `advance_predator_raids`, resolved through the *same* seam and the same `attack` stat a
    /// hunter's is.
    ///
    /// **One stat for both roles rather than a `warrior_attack` of its own**, because `attack` is
    /// already "what this person hits with" and a second stat would be a second authority over the
    /// one number the resolver reads. What keeps a spear out of a raid and a club out of a hunt is
    /// the kit's `jobs` list, not the stat.
    ///
    /// **No [`Quarry`], and `validate` enforces that there is nothing to ask about.** A warrior
    /// fights people, who have no `body_mass` on this roster, so a mass-bounded weapon in a warrior
    /// kit is rejected at load rather than silently resolving as unbounded here.
    pub fn warrior_profile(
        &self,
        intrinsic: CombatStats,
        kit: &KitChoice,
        wear: &crate::components::BandEquipment,
    ) -> CombatStats {
        self.hunter_profile_for(intrinsic, kit, wear, Quarry::Any)
    }

    /// **The two carries' shared resolution**, and the asymmetry with `attack` is one-home-per-fact,
    /// not an inconsistency: a carry item declares the **unequipped** side (the equipped rate is
    /// `labor_config`'s and stays there), so a live item means *the caller's equipped rate applies*
    /// and its absence means *fall back to what the item declared*.
    ///
    /// The fallback reads [`Self::declared_tier`] over the **whole item table**, not the kit — a party
    /// with no sled still has to know what a sledless haul rate is, and that number lives on the sled
    /// it is not carrying. With nothing in the table declaring the stat at all there is no second tier
    /// to step down to, so the equipped rate stands.
    fn two_tier(
        &self,
        stat: EquipmentStat,
        equipped_rate: f32,
        kit: &KitChoice,
        wear: &crate::components::BandEquipment,
    ) -> f32 {
        if kit.supplies(stat, wear, self) {
            return equipped_rate;
        }
        match self.declared_tier(stat) {
            Some(EffectTier::Unequipped(value)) => value,
            _ => equipped_rate,
        }
    }

    /// **How much the quarry's own `wariness` is multiplied by** for a party carrying this kit — see
    /// [`KitChoice::multiplier`] for why it is the maximum of what the live items declare.
    pub fn dispersion(&self, kit: &KitChoice, wear: &crate::components::BandEquipment) -> f32 {
        kit.multiplier(EquipmentStat::Dispersion, wear, self)
    }

    /// **The size window this kit's `attack` applies within**, as `(min, max)` body mass — the
    /// **widest** window its live weapons cover, because the kit can reach whatever its best weapon
    /// reaches. `None` on an end means unbounded there.
    ///
    /// Published so a **client** can resolve the pre-launch gate against the quarry in front of it.
    /// Without it a picker asks *"can this kit hurt a Red Deer"*, gets the passive device's
    /// `attack 20`, and answers **yes** about a hunt that would take nothing.
    ///
    /// A kit with no live weapon at all returns `(None, None)`: there is no attack to bound, and the
    /// party is bare-handed everywhere rather than nowhere.
    pub fn attack_mass_bounds(
        &self,
        kit: &KitChoice,
        wear: &crate::components::BandEquipment,
    ) -> (Option<f32>, Option<f32>) {
        let mut window: Option<(Option<f32>, Option<f32>)> = None;
        for item in kit.live_items(wear, self) {
            for effect in item
                .effects
                .iter()
                .filter(|e| e.stat == EquipmentStat::Attack)
            {
                // **An unbounded weapon widens the window to everything and ends the search** — a
                // kit carrying one bounded and one unbounded attack reaches every quarry.
                if !effect.is_mass_bounded() {
                    return (None, None);
                }
                window = Some(match window {
                    None => (effect.min_body_mass, effect.max_body_mass),
                    Some((min, max)) => (
                        // `None` is "no bound", which is WIDER than any number — so a union with it
                        // stays `None` rather than collapsing to the other side's value.
                        match (min, effect.min_body_mass) {
                            (Some(a), Some(b)) => Some(a.min(b)),
                            _ => None,
                        },
                        match (max, effect.max_body_mass) {
                            (Some(a), Some(b)) => Some(a.max(b)),
                            _ => None,
                        },
                    ),
                });
            }
        }
        window.unwrap_or((None, None))
    }

    /// **How much the hunt's baseline injury hazard is multiplied by** — `0` for a party whose whole
    /// kit keeps it out of reach of the animal.
    pub fn exposure(&self, kit: &KitChoice, wear: &crate::components::BandEquipment) -> f32 {
        kit.multiplier(EquipmentStat::Exposure, wear, self)
    }

    /// Invariants a TOE config must satisfy. **An item with no wear rate is not consumable** and one
    /// with no durability is born dry, so both are rejected rather than shipped as a silently eternal
    /// (or silently absent) item.
    ///
    /// `Invalid` names a **dynamic** field path now (`items.spears.wear.amount`) rather than one of a
    /// fixed nine, because the item table is open — a config that adds a bow gets the same checks with
    /// no edit here.
    pub fn validate(&self) -> Result<(), EquipmentConfigError> {
        if self.items.is_empty() {
            return Err(EquipmentConfigError::InvalidRoster {
                reason: "the item table is empty - no kit could put anything in a party's hands"
                    .to_string(),
            });
        }
        for (id, item) in &self.items {
            Self::require_positive(
                format!("items.{id}.starting_durability"),
                item.starting_durability,
            )?;
            Self::require_positive(format!("items.{id}.wear.amount"), item.wear.amount)?;
            if item.effects.is_empty() {
                return Err(EquipmentConfigError::InvalidRoster {
                    reason: format!(
                        "item '{id}' declares no effects - it would wear out doing nothing"
                    ),
                });
            }
            for (index, effect) in item.effects.iter().enumerate() {
                let value = effect.tier.value();
                if !value.is_finite() || value < 0.0 {
                    return Err(EquipmentConfigError::Invalid {
                        field: format!("items.{id}.effects[{index}]"),
                        constraint: "be finite and not negative".to_string(),
                        value: value.to_string(),
                    });
                }
                // **A second declaration of the same stat is rejected, not merged.** `effect()` takes
                // the first match, so a duplicate would be a silently dead line - and the two entries
                // would disagree about a number the fight reads.
                if item.effects[..index]
                    .iter()
                    .any(|prior| prior.stat == effect.stat)
                {
                    return Err(EquipmentConfigError::InvalidRoster {
                        reason: format!("item '{id}' declares the same stat twice"),
                    });
                }
                Self::validate_mass_bounds(id, index, effect)?;
            }
        }
        // **The two-tier stats must be declared by at most ONE item each**, because `declared_tier`
        // searches the whole table for the unequipped fallback and takes the FIRST match: two items
        // disagreeing about the sledless haul rate would resolve by `BTreeMap` order, which is
        // alphabetical and therefore arbitrary.
        for stat in EquipmentStat::TWO_TIER {
            let declared: Vec<&str> = self
                .items
                .iter()
                .filter(|(_, item)| item.effect(stat).is_some())
                .map(|(id, _)| id.as_str())
                .collect();
            if declared.len() > 1 {
                return Err(EquipmentConfigError::InvalidRoster {
                    reason: format!(
                        "items {} all declare the same two-tier stat {stat:?} - the unequipped fallback would resolve by name order",
                        declared.join(", ")
                    ),
                });
            }
        }
        self.validate_roster()?;
        self.validate_default_hunt_kit_is_quarry_blind()?;
        self.validate_warrior_kits_have_no_quarry()
    }

    /// **A WARRIOR KIT'S ATTACK MUST NOT BE BOUNDED BY BODY MASS**, because there is nothing on the
    /// other side of that fight with a `body_mass` to test.
    ///
    /// [`Self::warrior_profile`] resolves at [`Quarry::Any`], so a bounded weapon in a warrior kit
    /// would count *everywhere* — a snare rated to hold a hare would arm the camp against a wolf
    /// pack. That is `config-loading.md`'s "looks live but isn't" in its worst direction: the bound
    /// is written, parses, validates, and is then ignored by the one resolver that reads the item.
    /// Rejecting it says so at load instead.
    fn validate_warrior_kits_have_no_quarry(&self) -> Result<(), EquipmentConfigError> {
        for kit in self.kits.iter().filter(|kit| {
            kit.jobs
                .iter()
                .any(|job| matches!(job, KitJob::Warrior | KitJob::Scout))
        }) {
            for item in &kit.uses {
                let Some(def) = self.item(item) else { continue };
                for effect in &def.effects {
                    if effect.stat == EquipmentStat::Attack && effect.is_mass_bounded() {
                        return Err(EquipmentConfigError::InvalidRoster {
                            reason: format!(
                                "kit '{}' can be sent on a band-wide role but uses '{item}', whose attack is bounded by body mass — a raid has no quarry to test the bound against, so it would be silently ignored",
                                kit.id
                            ),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// **THE HUNT JOB'S DEFAULT KIT MUST CARRY NO MASS-BOUNDED ATTACK**, and this is a structural
    /// debt of the snapshot rather than a design preference.
    ///
    /// `snapshot/capture.rs` builds **one** party to price **every** herd's estimate tables — they
    /// are ~95% of snapshot capture and a per-herd kit resolution multiplies them (see
    /// `.claude/rules/core_sim/equipment.md` → "The two estimate tables are NOT repriced per kit").
    /// One party cannot carry a per-quarry attack, so it resolves *unbounded*. If the default kit's
    /// weapon were mass-bounded, every table would quote a kitted take against animals that weapon
    /// cannot touch — a lie in the reassuring direction, on the surface a player commits from.
    ///
    /// Rejecting the config is the loud form of that limit; silently quoting the wrong number is the
    /// failure `.claude/rules/core_sim/config-loading.md` exists to close. The fix, when a bounded
    /// weapon does become a default, is to resolve the profile per herd inside
    /// `herd_snapshot_entries` — and this check is what makes that a deliberate decision rather than
    /// a bug found in play.
    fn validate_default_hunt_kit_is_quarry_blind(&self) -> Result<(), EquipmentConfigError> {
        let default = self.default_kit(KitJob::Hunt);
        for item in default.uses() {
            let Some(def) = self.item(item) else { continue };
            for effect in &def.effects {
                if effect.stat == EquipmentStat::Attack && effect.is_mass_bounded() {
                    return Err(EquipmentConfigError::InvalidRoster {
                        reason: format!(
                            "the hunt job's default kit '{}' uses '{item}', whose attack is bounded by body mass — the per-herd estimate tables resolve ONE party for every herd and cannot express that",
                            default.id()
                        ),
                    });
                }
            }
        }
        Ok(())
    }

    /// **A body-mass bound must be usable, and must be HONOURED where it is written.**
    ///
    /// Only [`EquipmentStat::Attack`] is resolved through a [`Quarry`] today, so a bound on any other
    /// stat would parse, validate, and then do **nothing** — the "looks live but isn't" failure
    /// `.claude/rules/core_sim/config-loading.md` exists to close. It is rejected loudly instead, and
    /// the message says which stats do honour one so the author knows what to do about it. Widening
    /// this is the natural follow-on when the ranged work (#501) gives `hit_chance` a quarry.
    fn validate_mass_bounds(
        id: &str,
        index: usize,
        effect: &EquipmentEffect,
    ) -> Result<(), EquipmentConfigError> {
        if !effect.is_mass_bounded() {
            return Ok(());
        }
        if effect.stat != EquipmentStat::Attack {
            return Err(EquipmentConfigError::InvalidRoster {
                reason: format!(
                    "item '{id}' bounds effect[{index}] ({:?}) by body mass, but only `attack` is \
                     resolved against a quarry — the bound would be silently ignored",
                    effect.stat
                ),
            });
        }
        for (label, bound) in [
            ("min_body_mass", effect.min_body_mass),
            ("max_body_mass", effect.max_body_mass),
        ] {
            if let Some(value) = bound {
                if !value.is_finite() || value < 0.0 {
                    return Err(EquipmentConfigError::Invalid {
                        field: format!("items.{id}.effects[{index}].{label}"),
                        constraint: "be finite and not negative".to_string(),
                        value: value.to_string(),
                    });
                }
            }
        }
        // An inverted window reaches NOTHING, which is an item that silently does not work.
        if let (Some(min), Some(max)) = (effect.min_body_mass, effect.max_body_mass) {
            if min > max {
                return Err(EquipmentConfigError::InvalidRoster {
                    reason: format!(
                        "item '{id}' bounds effect[{index}] to body mass {min}..={max}, which reaches \
                         no quarry at all"
                    ),
                });
            }
        }
        Ok(())
    }

    fn require_positive(field: String, value: f32) -> Result<(), EquipmentConfigError> {
        if !value.is_finite() || value <= 0.0 {
            return Err(EquipmentConfigError::Invalid {
                field,
                constraint: "be finite and greater than 0".to_string(),
                value: value.to_string(),
            });
        }
        Ok(())
    }

    /// The item table's ids, for a refusal message.
    fn item_ids_for_message(&self) -> String {
        self.items
            .keys()
            .map(|id| id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// **The roster's own invariants.** A broken roster is a boot panic
    /// (`.claude/rules/core_sim/config-loading.md`), because every one of these breakages ends with
    /// a party priced at a tier nobody chose:
    ///
    /// - **ids are unique** — otherwise `kit_definition` silently picks the first of two, and which
    ///   one a command got would depend on file order.
    /// - **a kit lists at least one job** — a kit that can be sent on nothing is dead config, and
    ///   the roster is the client's picker list.
    /// - **each default names a real kit** — [`Self::default_kit`] is infallible by construction, so
    ///   this is what makes that `expect` a statement rather than a hope.
    /// - **each default covers its own job** — a `forage` default the forage verb would itself
    ///   refuse would fail every gather assignment at the command boundary.
    ///
    /// - **every `uses` entry names a real item** — this used to be free: the retired `KitComponent`
    ///   enum's variants *were* the JSON block keys, so a bad name could not deserialize. An item id
    ///   is a string, so the guarantee has to be bought back here or a typo would silently ship a kit
    ///   that grants nothing and wears nothing.
    fn validate_roster(&self) -> Result<(), EquipmentConfigError> {
        for (index, kit) in self.kits.iter().enumerate() {
            if self.kits[..index].iter().any(|prior| prior.id == kit.id) {
                return Err(EquipmentConfigError::InvalidRoster {
                    reason: format!("duplicate kit id '{}'", kit.id),
                });
            }
            if kit.jobs.is_empty() {
                return Err(EquipmentConfigError::InvalidRoster {
                    reason: format!("kit '{}' lists no jobs — it can be sent on nothing", kit.id),
                });
            }
            for item in &kit.uses {
                if !self.items.contains_key(item) {
                    return Err(EquipmentConfigError::UnknownItem {
                        kit: kit.id.clone(),
                        item: item.clone(),
                        available: self.item_ids_for_message(),
                    });
                }
            }
        }
        for job in KitJob::ALL {
            let id = self.default_kit_id(job);
            let Some(definition) = self.kit_definition(id) else {
                return Err(EquipmentConfigError::InvalidRoster {
                    reason: format!(
                        "default_kits.{} names '{}', which is not in the roster",
                        job.as_str(),
                        id
                    ),
                });
            };
            if !definition.jobs.contains(&job) {
                return Err(EquipmentConfigError::InvalidRoster {
                    reason: format!(
                        "default_kits.{} names '{}', whose jobs do not include {}",
                        job.as_str(),
                        id,
                        job.as_str()
                    ),
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum EquipmentConfigError {
    #[error("failed to read equipment config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse equipment config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid equipment config: `{field}` must {constraint}, got {value}")]
    Invalid {
        field: String,
        constraint: String,
        value: String,
    },
    /// A roster the sim cannot resolve a kit through — see [`EquipmentConfig::validate_roster`].
    /// Carries the same weight as [`Self::Invalid`]: a file that is there and wrong.
    #[error("invalid equipment kit roster: {reason}")]
    InvalidRoster { reason: String },
    /// **A kit naming an item the table does not carry.** Its own variant rather than an
    /// `InvalidRoster` string because it is the one check that *replaces* a guarantee the retired
    /// `KitComponent` enum gave for free at parse time — naming it makes the debt visible, and lets
    /// the message list what there actually is for the typo that caused it.
    #[error("kit '{kit}' uses '{item}', which is not an item — the table carries {available}")]
    UnknownItem {
        kit: String,
        item: String,
        available: String,
    },
}

impl ConfigLoadError for EquipmentConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// Handle for accessing the TOE configuration.
#[derive(Resource, Debug, Clone)]
pub struct EquipmentConfigHandle(pub Arc<EquipmentConfig>);

impl EquipmentConfigHandle {
    pub fn new(config: Arc<EquipmentConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<EquipmentConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<EquipmentConfig>) {
        self.0 = config;
    }
}

impl Default for EquipmentConfigHandle {
    fn default() -> Self {
        Self(EquipmentConfig::builtin())
    }
}

/// Metadata about the TOE configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct EquipmentConfigMetadata {
    path: Option<PathBuf>,
}

impl EquipmentConfigMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    pub fn set_path(&mut self, path: Option<PathBuf>) {
        self.path = path;
    }
}

/// Load TOE configuration from environment (`EQUIPMENT_CONFIG_PATH`) or the default data path. The
/// file is **validated** before it can reach the sim, and a broken invariant is as fatal as a parse
/// error. Only an absent *default* path falls back to the builtin; a present-but-broken file, or an
/// `EQUIPMENT_CONFIG_PATH` that names a missing or broken file, is a boot panic — see
/// [`crate::config_load::resolve_config`].
pub fn load_equipment_config_from_env() -> (Arc<EquipmentConfig>, EquipmentConfigMetadata) {
    let (config, source) = load_config_from_env(
        "EQUIPMENT_CONFIG_PATH",
        "equipment_config",
        "src/data/equipment.json",
        EquipmentConfig::builtin,
        EquipmentConfig::from_file,
    );
    (config, EquipmentConfigMetadata::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::RangeBand;
    use crate::components::BandEquipment;
    use crate::creatures_config::CreaturesConfig;

    /// The shipped item ids. Named here rather than inlined so a rename shows up as a compile-adjacent
    /// diff in one place — the *sim* never spells an item, it resolves stats and quanta.
    const SPEARS: &str = "spears";
    const SLED: &str = "sled";
    const BASKETS: &str = "baskets";
    const TRAPS: &str = "traps";

    fn item<'a>(config: &'a EquipmentConfig, id: &str) -> &'a ItemDefinition {
        config
            .item(id)
            .unwrap_or_else(|| panic!("the shipped roster must carry '{id}'"))
    }

    /// The **equipped** value an item declares for a stat — panics if it declares the other tier, so
    /// a test cannot silently assert against the wrong side of a cliff.
    fn equipped_of(config: &EquipmentConfig, id: &str, stat: EquipmentStat) -> f32 {
        match item(config, id).effect(stat) {
            Some(EffectTier::Equipped(value)) => value,
            other => panic!("'{id}' declares {other:?} for {stat:?}, not an equipped tier"),
        }
    }

    /// The **unequipped** value an item declares for a stat. Same strictness, other side.
    fn unequipped_of(config: &EquipmentConfig, id: &str, stat: EquipmentStat) -> f32 {
        match item(config, id).effect(stat) {
            Some(EffectTier::Unequipped(value)) => value,
            other => panic!("'{id}' declares {other:?} for {stat:?}, not an unequipped tier"),
        }
    }

    /// A synthetic kit over exactly these items — lets a test name a loadout the shipped roster does
    /// not offer (spears without a sled, spears *with* traps) without editing `equipment.json`.
    fn kit_of(items: &[&str]) -> KitChoice {
        KitChoice {
            id: Arc::from("test"),
            uses: items.iter().map(|id| Arc::from(*id)).collect(),
        }
    }

    /// A band that has used nothing.
    fn fresh() -> BandEquipment {
        BandEquipment::default()
    }

    /// **THE TRAPPING KIT'S WHOLE CLAIM, IN ONE PLACE.**
    ///
    /// The passive device — snares, nets, weirs, one item, because at this game's abstraction they
    /// are one thing: something you set down, walk away from, and come back to. Its advantage is
    /// **not being seen**: `dispersion 0` means nothing bolts, so the party keeps everything it
    /// reaches, and against a `wariness 0.75` warren that is the whole 4× gap over a spear party.
    ///
    /// **Its `attack` is the spear's own number, deliberately.** At 0.13–0.67 kg the weapon is not
    /// what is scarce, so the device must not win by hitting harder — it wins by not scaring
    /// anything, and `max_body_mass` is what keeps it off everything else.
    #[test]
    fn the_trapping_kit_wins_by_not_being_seen_on_quarry_it_can_hold() {
        let equipment = EquipmentConfig::builtin();
        let trapping = equipment
            .kit("trapping")
            .expect("the roster ships trapping");
        let intrinsic = CreaturesConfig::builtin().person();
        let fresh = fresh();

        // **THE ATTACK IS BOUNDED BY THE QUARRY'S MASS, and that is the whole of what keeps a trap
        // line from being a universal upgrade.** A flat attack shipped once and cleared a Red Deer's
        // `defense 1` exactly as it cleared a rabbit's `0`.
        const A_HARE: f32 = 0.6; // inside the bound
        const A_DEER: f32 = 15.0; // far outside it
        assert!(
            equipment
                .hunter_profile_against(intrinsic, &trapping, &fresh, A_HARE)
                .attack
                > intrinsic.attack,
            "traps must actually kill what they can hold, or the kit is a decoy"
        );
        assert_eq!(
            equipment
                .hunter_profile_against(intrinsic, &trapping, &fresh, A_DEER)
                .attack,
            intrinsic.attack,
            "above the bound the item grants NOTHING — the party is bare-handed and the fight's own \
             gate refuses the hunt, with no 'cannot trap that' branch anywhere"
        );
        assert_eq!(
            equipment.dispersion(&trapping, &fresh),
            0.0,
            "nothing runs up to the animal, so nothing scatters"
        );
        assert_eq!(
            equipment.exposure(&trapping, &fresh),
            0.0,
            "a stand-off instrument wears out INSTEAD of its user getting hurt"
        );

        // **The sled it also carries must not undo any of that.** Carry gear declares none of the
        // three multipliers, and `multiplier()` folds only what its live items DECLARE — the clause
        // that would otherwise drag dispersion back to the neutral 1.0 and make traps inert.
        assert!(
            trapping.uses().any(|item| item == SLED),
            "the fixture is vacuous unless the trapping kit really carries a second item"
        );

        // ...whereas a kit that DOES put hands in reach resolves loud and exposed, because the
        // multipliers take the MAX of what is declared. Synthetic: nothing shipped pairs them.
        let both = kit_of(&[SPEARS, TRAPS]);
        assert_eq!(
            equipment.dispersion(&both, &fresh),
            1.0,
            "throwing spears scares the herd however many traps you also set"
        );
        assert_eq!(
            equipment.exposure(&both, &fresh),
            1.0,
            "and puts you in reach of it — the trap's stand-off is not inherited for free"
        );
    }

    /// The shipped equipped **hunt** haul rate — `labor_config.json`'s
    /// `hunt.per_worker_biomass_capacity`, named here so this module's unit tests do not restate a
    /// number that lives elsewhere.
    fn equipped_haul_rate() -> f32 {
        crate::labor_config::LaborConfig::builtin()
            .hunt
            .per_worker_biomass_capacity
    }

    /// The shipped equipped **gather** throughput — `labor_config.json`'s
    /// `forage.per_worker_biomass_capacity`, for the same reason.
    fn equipped_gather_rate() -> f32 {
        crate::labor_config::LaborConfig::builtin()
            .forage
            .per_worker_biomass_capacity
    }

    #[test]
    fn builtin_config_ships_all_three_kits() {
        let config = EquipmentConfig::builtin();
        assert_eq!(equipped_of(&config, SPEARS, EquipmentStat::Attack), 20.0);
        assert!(item(&config, SPEARS).starting_durability > 0.0);
        assert!(item(&config, SPEARS).wear.amount > 0.0);
        assert!(unequipped_of(&config, SLED, EquipmentStat::HuntCarry) > 0.0);
        assert!(item(&config, SLED).wear.amount > 0.0);
        assert!(unequipped_of(&config, BASKETS, EquipmentStat::ForageCarry) > 0.0);
        assert!(item(&config, BASKETS).wear.amount > 0.0);
    }

    /// **The equipped tier must beat the unequipped one on ALL THREE axes** — a "kit" that made you
    /// worse is incoherent. The three unequipped tiers live in the configs that own them, so this is
    /// the one place the files are compared.
    #[test]
    fn every_equipped_tier_beats_its_unequipped_tier() {
        let equipment = EquipmentConfig::builtin();
        let bare_attack = CreaturesConfig::builtin().person().attack;
        assert!(
            equipped_of(&equipment, SPEARS, EquipmentStat::Attack) > bare_attack,
            "the hunting kit must raise attack above the bare-handed {bare_attack}"
        );
        assert!(
            equipped_haul_rate() > unequipped_of(&equipment, SLED, EquipmentStat::HuntCarry),
            "the sled must raise the hunt's haul rate above the bare-armed tier"
        );
        assert!(
            equipped_gather_rate() > unequipped_of(&equipment, BASKETS, EquipmentStat::ForageCarry),
            "baskets must raise the gather rate above the bare-handed tier"
        );
    }

    /// **The two carries want different SHAPES, not merely different numbers** (§4.8). Forage is
    /// *containment*-bound — a handful against a basketful — so its drop is the harsher one; the hunt
    /// is *transport*-bound, and a sledless party can always drag something. Asserted as the ordering
    /// between the two ratios, with a liveness assertion on each ratio so a kit whose tiers collapsed
    /// to equal cannot satisfy it vacuously.
    #[test]
    fn losing_your_baskets_costs_proportionally_more_than_losing_your_sled() {
        let equipment = EquipmentConfig::builtin();
        let sled_ratio =
            equipped_haul_rate() / unequipped_of(&equipment, SLED, EquipmentStat::HuntCarry);
        let basket_ratio =
            equipped_gather_rate() / unequipped_of(&equipment, BASKETS, EquipmentStat::ForageCarry);
        assert!(
            sled_ratio > 1.0,
            "the sled must be live at all: ratio {sled_ratio}"
        );
        assert!(
            basket_ratio > 1.0,
            "baskets must be live at all: ratio {basket_ratio}"
        );
        assert!(
            basket_ratio > sled_ratio,
            "containment must bite harder than transport: baskets ×{basket_ratio} vs sled ×{sled_ratio}"
        );
    }

    #[test]
    fn the_hunter_profile_swaps_only_the_attack_tier() {
        let equipment = EquipmentConfig::builtin();
        let bare = CreaturesConfig::builtin().person();
        let kitted = equipment.hunter_profile_unbounded(bare, &kit_of(&[SPEARS]), &fresh());
        assert_eq!(
            kitted.attack,
            equipped_of(&equipment, SPEARS, EquipmentStat::Attack)
        );
        // Defense/range/wariness are the Warrior kit's business, not the spear's.
        assert_eq!(kitted.defense, bare.defense);
        assert_eq!(kitted.range, RangeBand::Melee);
        assert_eq!(kitted.wariness, bare.wariness);
        // Unequipped is the intrinsic row itself — the tier it drops back to.
        assert_eq!(
            equipment.hunter_profile_unbounded(bare, &kit_of(&[]), &fresh()),
            bare
        );
    }

    #[test]
    fn each_carry_tier_resolves_to_one_of_exactly_two_rates() {
        let equipment = EquipmentConfig::builtin();
        let hunt = equipped_haul_rate();
        let gather = equipped_gather_rate();
        assert_eq!(
            equipment.hunt_per_worker_biomass_capacity(hunt, &kit_of(&[SLED]), &fresh()),
            hunt
        );
        assert_eq!(
            equipment.hunt_per_worker_biomass_capacity(hunt, &kit_of(&[]), &fresh()),
            unequipped_of(&equipment, SLED, EquipmentStat::HuntCarry)
        );
        assert_eq!(
            equipment.forage_per_worker_biomass_capacity(gather, &kit_of(&[BASKETS]), &fresh()),
            gather
        );
        assert_eq!(
            equipment.forage_per_worker_biomass_capacity(gather, &kit_of(&[]), &fresh()),
            unequipped_of(&equipment, BASKETS, EquipmentStat::ForageCarry)
        );
    }

    /// **ONE KIT, ONE JOB, at the resolver seam** (§4.8) — the cross-check that would have caught the
    /// original defect, where the "carry kit" called baskets raised the *hunt's* haul and foraging
    /// got nothing. A dry basket must leave the hunt's tier untouched and a dry sled must leave the
    /// gather's untouched, so each resolver is swept across the other kit's state.
    #[test]
    fn a_dry_basket_does_not_touch_the_hunt_and_a_dry_sled_does_not_touch_the_gather() {
        let equipment = EquipmentConfig::builtin();
        let hunt = equipped_haul_rate();
        let gather = equipped_gather_rate();
        // Liveness on both sides: each kit really does move its own number...
        assert_ne!(
            equipment.hunt_per_worker_biomass_capacity(hunt, &kit_of(&[SLED]), &fresh()),
            equipment.hunt_per_worker_biomass_capacity(hunt, &kit_of(&[]), &fresh()),
            "the sled tier must be live, or the cross-check below is vacuous"
        );
        assert_ne!(
            equipment.forage_per_worker_biomass_capacity(gather, &kit_of(&[BASKETS]), &fresh()),
            equipment.forage_per_worker_biomass_capacity(gather, &kit_of(&[]), &fresh()),
            "the basket tier must be live, or the cross-check below is vacuous"
        );
        // ...and neither resolver can even be *asked* about the other item, because the two
        // declare DIFFERENT STATS (`hunt_carry` against `forage_carry`) and each resolver names the
        // one it wants. A kit holding only the other item supplies nothing for that stat, so the
        // answer falls to its declared unequipped tier — never to the other web's number.
        assert_eq!(
            equipment.hunt_per_worker_biomass_capacity(hunt, &kit_of(&[SLED]), &fresh()),
            hunt,
            "an equipped sled hauls at the shipped rate whatever the baskets are doing"
        );
        assert_eq!(
            equipment.forage_per_worker_biomass_capacity(gather, &kit_of(&[BASKETS]), &fresh()),
            gather,
            "equipped baskets gather at the shipped rate whatever the sled is doing"
        );
    }

    #[test]
    fn validate_rejects_a_kit_that_never_wears_out() {
        // `wear_per_kill = 0` would make a "consumable" kit eternal — the one value that silently
        // deletes the whole pressure this slice exists to create.
        let err = EquipmentConfig::from_json_str(&kit_json("0.0", "0.02", "100.0"))
            .expect_err("a zero wear rate is invalid");
        assert!(
            matches!(&err, EquipmentConfigError::Invalid { field, .. } if field == "items.spears.wear.amount"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn validate_rejects_a_kit_born_dry() {
        let err = EquipmentConfig::from_json_str(&kit_json("2.0", "0.02", "0.0"))
            .expect_err("zero durability is invalid");
        assert!(
            matches!(&err, EquipmentConfigError::Invalid { field, .. } if field == "items.baskets.starting_durability"),
            "unexpected error: {err}"
        );
    }

    /// **A kit naming an item the table does not carry is REJECTED** — and this is the check that
    /// pays back a guarantee the model used to get for free.
    ///
    /// The retired `KitComponent` enum's variants *were* the JSON block keys, so a roster naming a
    /// component with no block could not deserialize. Item ids are strings, so nothing stops a file
    /// naming `baskets` in a `uses` list and never defining it — which would leave the forage web
    /// silently unkitted again, the exact defect §4.8 corrects. Only `validate` stands between that
    /// file and a running sim now.
    #[test]
    fn a_kit_naming_an_item_the_table_does_not_carry_is_rejected() {
        let json = format!(
            r#"{{
            "items": {{
                "spears": {{ "starting_durability": 100.0, "wear": {{ "per": "kill", "amount": 0.4 }}, "effects": [{{ "stat": "attack", "equipped": 20.0 }}] }},
                "sled": {{ "starting_durability": 100.0, "wear": {{ "per": "biomass_hauled", "amount": 0.02 }}, "effects": [{{ "stat": "hunt_carry", "unequipped": 12.0 }}] }}
            }},
            {ROSTER_JSON}
        }}"#
        );
        let err = EquipmentConfig::from_json_str(&json)
            .expect_err("a kit naming an undefined item is invalid");
        assert!(
            matches!(&err, EquipmentConfigError::UnknownItem { item, .. } if item == "baskets"),
            "unexpected error: {err}"
        );
    }

    /// A three-item fixture with one dial per item under the test's control — so a validate test says
    /// which dial it is breaking instead of restating the whole file.
    fn kit_json(spear_wear: &str, sled_wear: &str, basket_durability: &str) -> String {
        format!(
            r#"{{
            "items": {{
                "spears": {{ "starting_durability": 100.0, "wear": {{ "per": "kill", "amount": {spear_wear} }}, "effects": [{{ "stat": "attack", "equipped": 20.0 }}] }},
                "sled": {{ "starting_durability": 100.0, "wear": {{ "per": "biomass_hauled", "amount": {sled_wear} }}, "effects": [{{ "stat": "hunt_carry", "unequipped": 12.0 }}] }},
                "baskets": {{ "starting_durability": {basket_durability}, "wear": {{ "per": "biomass_gathered", "amount": 0.04 }}, "effects": [{{ "stat": "forage_carry", "unequipped": 1.6 }}] }}
            }},
            {ROSTER_JSON}
        }}"#
        )
    }

    // -----------------------------------------------------------------------------------------
    // THE KIT ROSTER
    // -----------------------------------------------------------------------------------------

    /// **The two working kits mask in exactly what every call site used to consult
    /// unconditionally** — which is the whole load-bearing claim of the roster arc. If this fails,
    /// choosing a shipped kit is not the no-op the design rests on and every number in the game has
    /// moved.
    #[test]
    fn the_two_shipped_kits_reproduce_the_pre_roster_predicates() {
        let equipment = EquipmentConfig::builtin();
        let fresh = crate::components::BandEquipment::default();
        let big_game = equipment
            .kit("big_game")
            .expect("the roster ships big_game");
        let gathering = equipment
            .kit("gathering")
            .expect("the roster ships gathering");

        // The hunt job's kit reaches for the hunt's two components and nothing else.
        assert!(big_game.item_live(SPEARS, &fresh, &equipment));
        assert!(big_game.item_live(SLED, &fresh, &equipment));
        assert!(
            !big_game.item_live(BASKETS, &fresh, &equipment),
            "one kit, one job — a spear-and-sled party carries no baskets"
        );
        // And the forage job's reaches for the basket and nothing else.
        assert!(gathering.item_live(BASKETS, &fresh, &equipment));
        assert!(!gathering.item_live(SPEARS, &fresh, &equipment));
        assert!(!gathering.item_live(SLED, &fresh, &equipment));

        // **The tiers those masks resolve to are the shipped equipped ones, bit for bit.**
        let labor = crate::labor_config::LaborConfig::builtin();
        let intrinsic = CreaturesConfig::builtin().person();
        assert_eq!(
            equipment
                .hunter_profile_unbounded(intrinsic, &big_game, &fresh)
                .attack,
            equipped_of(&equipment, SPEARS, EquipmentStat::Attack)
        );
        assert_eq!(
            equipment.hunt_per_worker_biomass_capacity(
                labor.hunt.per_worker_biomass_capacity,
                &big_game,
                &fresh
            ),
            labor.hunt.per_worker_biomass_capacity
        );
        assert_eq!(
            equipment.forage_per_worker_biomass_capacity(
                labor.forage.per_worker_biomass_capacity,
                &gathering,
                &fresh
            ),
            labor.forage.per_worker_biomass_capacity
        );
    }

    /// **A kit that uses nothing reads false everywhere, however fresh the band's gear is** — and
    /// the tiers it resolves to are the three unequipped ones. `none` is an ordinary roster member,
    /// so this is a statement about an empty `uses` list rather than about a sentinel id.
    #[test]
    fn a_kit_that_uses_nothing_runs_at_every_unequipped_tier() {
        let equipment = EquipmentConfig::builtin();
        let fresh = crate::components::BandEquipment::default();
        let none = equipment.kit("none").expect("the roster ships none");
        assert!(!none.item_live(SPEARS, &fresh, &equipment));
        assert!(!none.item_live(SLED, &fresh, &equipment));
        assert!(!none.item_live(BASKETS, &fresh, &equipment));

        let labor = crate::labor_config::LaborConfig::builtin();
        let intrinsic = CreaturesConfig::builtin().person();
        assert_eq!(
            equipment
                .hunter_profile_unbounded(intrinsic, &none, &fresh)
                .attack,
            intrinsic.attack,
            "a party carrying no spears fights bare-handed"
        );
        assert_eq!(
            equipment.hunt_per_worker_biomass_capacity(
                labor.hunt.per_worker_biomass_capacity,
                &none,
                &fresh
            ),
            unequipped_of(&equipment, SLED, EquipmentStat::HuntCarry)
        );
        assert_eq!(
            equipment.forage_per_worker_biomass_capacity(
                labor.forage.per_worker_biomass_capacity,
                &none,
                &fresh
            ),
            unequipped_of(&equipment, BASKETS, EquipmentStat::ForageCarry)
        );
    }

    /// **A wrong-job kit is refused, and an unknown one too** — never a quiet fall back to the
    /// default. Both readings are the same defect from the player's side: they asked to compare
    /// tiers and were answered about a different one.
    #[test]
    fn a_kit_is_refused_for_a_job_it_does_not_list_and_for_an_id_that_is_not_there() {
        let equipment = EquipmentConfig::builtin();
        assert!(matches!(
            equipment.resolve_kit_for_job(Some("gathering"), KitJob::Hunt),
            Err(KitSelectionError::WrongJob { .. })
        ));
        assert!(matches!(
            equipment.resolve_kit_for_job(Some("big_game"), KitJob::Forage),
            Err(KitSelectionError::WrongJob { .. })
        ));
        assert!(matches!(
            equipment.resolve_kit_for_job(Some("spear_of_destiny"), KitJob::Hunt),
            Err(KitSelectionError::Unknown { .. })
        ));
        // `none` covers both jobs, so it resolves on either.
        assert!(equipment
            .resolve_kit_for_job(Some("none"), KitJob::Hunt)
            .is_ok());
        assert!(equipment
            .resolve_kit_for_job(Some("none"), KitJob::Forage)
            .is_ok());
        // Naming none at all is the job's default, which is not an error.
        assert_eq!(
            equipment
                .resolve_kit_for_job(None, KitJob::Hunt)
                .expect("no selection resolves to the default")
                .id(),
            equipment.default_kit_id(KitJob::Hunt)
        );
    }

    /// **Every broken roster shape is rejected at load**, which under
    /// `.claude/rules/core_sim/config-loading.md` makes it a boot panic rather than a sim quietly
    /// running a kit table nobody authored. Swept rather than asserted one at a time so a new
    /// invariant has an obvious place to join.
    #[test]
    fn validate_rejects_every_broken_roster_shape() {
        let cases: [(&str, &str); 4] = [
            (
                "duplicate ids",
                r#""kits": [
                    { "id": "big_game", "display_name": "A", "jobs": ["hunt"], "uses": [] },
                    { "id": "big_game", "display_name": "B", "jobs": ["hunt"], "uses": [] }
                ],
                "default_kits": { "hunt": "big_game", "forage": "big_game", "scout": "big_game", "warrior": "big_game" }"#,
            ),
            (
                "a kit that can be sent on nothing",
                r#""kits": [
                    { "id": "big_game", "display_name": "A", "jobs": [], "uses": [] }
                ],
                "default_kits": { "hunt": "big_game", "forage": "big_game", "scout": "big_game", "warrior": "big_game" }"#,
            ),
            (
                "a default naming no roster entry",
                r#""kits": [
                    { "id": "big_game", "display_name": "A", "jobs": ["hunt", "forage"], "uses": [] }
                ],
                "default_kits": { "hunt": "ghost", "forage": "big_game", "scout": "big_game", "warrior": "big_game" }"#,
            ),
            (
                "a default whose jobs do not cover its own job",
                r#""kits": [
                    { "id": "big_game", "display_name": "A", "jobs": ["hunt"], "uses": [] },
                    { "id": "gathering", "display_name": "B", "jobs": ["forage"], "uses": [] }
                ],
                "default_kits": { "hunt": "gathering", "forage": "gathering", "scout": "gathering", "warrior": "gathering" }"#,
            ),
        ];
        for (what, roster) in cases {
            let err = EquipmentConfig::from_json_str(&component_json(roster))
                .expect_err(&format!("{what} must be rejected"));
            assert!(
                matches!(err, EquipmentConfigError::InvalidRoster { .. }),
                "{what}: unexpected error: {err}"
            );
        }
    }

    /// **A `uses` entry naming an item the table does not carry is rejected at VALIDATE** — and this
    /// test is the debt the effects model took on, so it must not be deleted.
    ///
    /// The retired `KitComponent` enum's variants *were* the JSON block keys, so this file used to
    /// get the invariant for free: an unknown name could not deserialize, and the guarantee was
    /// carried by the type. An item id is a `String`, so nothing stops a config naming `net_kit`; the
    /// same boot panic now depends entirely on [`EquipmentConfig::validate`] running on every load
    /// path. A kit that silently granted and wore nothing is exactly the failure §4.8 corrects.
    #[test]
    fn a_kit_using_an_item_that_does_not_exist_is_rejected() {
        let err = EquipmentConfig::from_json_str(&component_json(
            r#""kits": [
                { "id": "big_game", "display_name": "A", "jobs": ["hunt", "forage"], "uses": ["net_kit"] }
            ],
            "default_kits": { "hunt": "big_game", "forage": "big_game", "scout": "big_game", "warrior": "big_game" }"#,
        ))
        .expect_err("an item that does not exist is invalid");
        assert!(
            matches!(&err, EquipmentConfigError::UnknownItem { item, kit, .. }
                if item == "net_kit" && kit == "big_game"),
            "unexpected error: {err}"
        );
    }

    /// The three shipped **component** blocks at their shipped values, with the roster left to the
    /// caller — the mirror of [`kit_json`], for fixtures that break the roster instead of a dial.
    fn component_json(roster: &str) -> String {
        format!(
            r#"{{
            "items": {{
                "spears": {{ "starting_durability": 100.0, "wear": {{ "per": "kill", "amount": 0.4 }}, "effects": [{{ "stat": "attack", "equipped": 20.0 }}] }},
                "sled": {{ "starting_durability": 100.0, "wear": {{ "per": "biomass_hauled", "amount": 0.02 }}, "effects": [{{ "stat": "hunt_carry", "unequipped": 12.0 }}] }},
                "baskets": {{ "starting_durability": 100.0, "wear": {{ "per": "biomass_gathered", "amount": 0.04 }}, "effects": [{{ "stat": "forage_carry", "unequipped": 1.6 }}] }}
            }},
            {roster}
        }}"#
        )
    }

    /// A minimal **valid** roster, so a fixture testing one of the three *component* blocks does not
    /// have to restate the shipped kit list to get past the roster's own validation.
    /// **A BAND-WIDE role's weapon may not be bounded by body mass, because there is nothing on the
    /// other side of that fight to test the bound against.**
    ///
    /// [`EquipmentConfig::warrior_profile`] resolves at [`Quarry::Any`], so a bounded weapon in a
    /// warrior kit counts **everywhere** — a snare rated to hold a hare would arm the camp against a
    /// wolf pack. That is `config-loading.md`'s "looks live but isn't" in its worst direction: the
    /// bound parses, validates, and is then silently ignored by the one resolver that reads it. The
    /// twin of `validate_default_hunt_kit_is_quarry_blind`, and rejected for the same reason.
    #[test]
    fn a_band_wide_kit_may_not_carry_a_mass_bounded_weapon() {
        // Its own item table rather than `component_json`'s, because the bounded weapon has to
        // EXIST — a roster naming an item the table does not carry is rejected by `UnknownItem`
        // first, which is a different check.
        let json = r#"{
            "items": {
                "spears": { "starting_durability": 100.0, "wear": { "per": "kill", "amount": 0.4 }, "effects": [{ "stat": "attack", "equipped": 20.0 }] },
                "snares": { "starting_durability": 100.0, "wear": { "per": "kill", "amount": 0.2 }, "effects": [{ "stat": "attack", "equipped": 20.0, "max_body_mass": 1.0 }] }
            },
            "kits": [
                { "id": "big_game", "display_name": "A", "jobs": ["hunt", "forage"], "uses": ["spears"] },
                { "id": "warrior", "display_name": "W", "jobs": ["warrior", "scout"], "uses": ["snares"] }
            ],
            "default_kits": { "hunt": "big_game", "forage": "big_game", "scout": "warrior", "warrior": "warrior" }
        }"#;
        let err = EquipmentConfig::from_json_str(json)
            .expect_err("a warrior kit carrying a mass-bounded weapon is invalid");
        assert!(
            matches!(&err, EquipmentConfigError::InvalidRoster { reason } if reason.contains("snares")),
            "unexpected error: {err}"
        );
    }

    /// **A minimal roster that satisfies validate on all four jobs.** The two band-wide roles need a
    /// default like the other two, and `none` is what these fixtures give them: they are testing the
    /// item table, not the roster, and a kit-carrying entry per role would put three more items in
    /// every one of them.
    const ROSTER_JSON: &str = r#""kits": [
                { "id": "big_game", "display_name": "Stalking kit", "jobs": ["hunt"], "uses": ["spears", "sled"] },
                { "id": "gathering", "display_name": "Gathering kit", "jobs": ["forage"], "uses": ["baskets"] },
                { "id": "none", "display_name": "No kit", "jobs": ["hunt", "forage", "scout", "warrior"], "uses": [] }
            ],
            "default_kits": { "hunt": "big_game", "forage": "gathering", "scout": "none", "warrior": "none" }"#;
}
