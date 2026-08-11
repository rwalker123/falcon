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
    /// The per-hunter **hunt** haul rate. Declared **equipped, on the item's TIER** — the sledded
    /// `40.0` is what a flint-age sled buys; `labor_config.json`'s
    /// `hunt.per_worker_biomass_capacity` is the **no-equipment baseline** a sledless party drags at.
    HuntCarry,
    /// The per-gatherer throughput before the tile's seasonal weight. Declared **equipped, on the
    /// item's TIER**; `labor_config.json`'s `forage.per_worker_biomass_capacity` is the
    /// **no-equipment baseline** a bare-handed gatherer carries.
    ForageCarry,
    /// **Multiplies the quarry's own `wariness`** — `effective_wariness = clamp(wariness × dispersion,
    /// 0, 1)`. Neutral at `1.0`; a trap ships `0.0`. A multiplier rather than a subtraction so the
    /// *species* decides how much a noisy approach costs, which is what lets one spear line scatter a
    /// warren and contain a mammoth with no per-target authoring.
    Dispersion,
    /// **Multiplies the hunt's baseline injury hazard** (`fauna::hunt_injuries`). Neutral at `1.0`; a
    /// stand-off instrument ships `0.0` and wears out instead of its users getting hurt.
    Exposure,
    /// **The per-keeper rate a PEN is collected at.** Declared **unequipped**; the equipped side is
    /// **shared with [`Self::HuntCarry`]** ([`Self::shares_equipped_rate_with`]) — the number the pen
    /// harvest has always run on, so a keeper carrying husbandry gear collects exactly what it
    /// always did, and that number keeps its single home on the sled's tier.
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
    /// **Multiplies the rate a rung's per-source build meter fills at** — the factor
    /// [`crate::intensification::RungDef::build_accrual`] applies beside the floor, the species
    /// timescale and the crew scale. Neutral at `1.0`; the handling gear ships `1.5`.
    ///
    /// **A multiplier rather than a two-tier rate, and that is what lets a SECOND item declare it.**
    /// The [`Self::TWO_TIER`] stats find their other side by searching the whole item table and
    /// taking the first match, so each of them may be declared by exactly one item or the answer
    /// resolves alphabetically. A build tool for the plant web (issue #539) is a second declarer by
    /// construction, so the stat has to be the kind a kit resolves as the **max of what its live
    /// items declare** — the shape `dispersion` and `exposure` already run on.
    ///
    /// **It is deliberately NOT named for husbandry.** Both food webs' rungs read the one build
    /// seam, so a stat keyed to the animal branch would have to be renamed the day a hoe ships. What
    /// is animal-only today is the *content* — `husbandry_gear` is the only item declaring it, so
    /// both plant rungs resolve the neutral `1.0` until an item names them.
    BuildRate,
    /// **The rate a bench works at with this tool in hand** — the value
    /// `workers × progress_per_worker_turn ×` is multiplied by. Declared **equipped only**; the
    /// unequipped side is the MATERIAL's own [`crate::materials_config::HandWorking::rate`], which
    /// is `0` for a material that cannot be worked bare-handed. See [`Self::CRAFT_ONLY`].
    CraftSpeed,
    /// **The best reading a craft on this tool can realize** — the output grade is selected by
    /// `min(material reading, ceiling)`, so excellent flax with no loom still makes a fair basket.
    /// Declared **equipped only**; the bare-handed ceiling is the material's
    /// [`crate::materials_config::HandWorking::quality_ceiling`].
    CraftQualityCeiling,
    /// **The fraction of a recipe's stated input amounts a draw on this tool actually consumes.**
    /// Declared **equipped only**; the bare-handed side is the identity
    /// ([`crate::crafting::HAND_WORKING_MATERIAL_EFFICIENCY`]) — a bench with nothing on it saves
    /// nothing.
    CraftMaterialEfficiency,
}

impl EquipmentStat {
    /// The neutral value — what the stat reads when **no** item declares it. Only the multiplier
    /// stats have one; the tiered stats resolve against a rate the caller already holds, so asking
    /// for their neutral value is a category error the type refuses to answer.
    pub fn neutral(self) -> Option<f32> {
        match self {
            EquipmentStat::Dispersion | EquipmentStat::Exposure | EquipmentStat::BuildRate => {
                Some(1.0)
            }
            EquipmentStat::Attack
            | EquipmentStat::HuntCarry
            | EquipmentStat::ForageCarry
            | EquipmentStat::PenCarry
            | EquipmentStat::ScoutVantageRange
            | EquipmentStat::CraftSpeed
            | EquipmentStat::CraftQualityCeiling
            | EquipmentStat::CraftMaterialEfficiency => None,
        }
    }

    /// **The stats resolved through [`EquipmentConfig::rate_tier`]** — the two-sided rates, whose
    /// *other* side is found by searching the **whole item table** and taking the first match. Each
    /// of these may therefore be declared by at most one item (on the item or on any of its tiers)
    /// or the answer would resolve by `BTreeMap` order, i.e. alphabetically. Named once, here, so
    /// `validate` cannot fall behind a new stat.
    ///
    /// **They do not all declare the same SIDE, and that is one-home-per-fact rather than an
    /// inconsistency.** The two carries declare the **equipped** side on the item's tier (that is
    /// what the material buys) and fall back to `labor_config.json`'s no-equipment baseline; the pen
    /// and the vantage declare the **unequipped** side on the item, because their equipped value
    /// already has a home elsewhere — the hunt haul's tier for the pen
    /// ([`Self::shares_equipped_rate_with`]), `labor_config.scout.vantage_range` for the vantage.
    pub const TWO_TIER: [EquipmentStat; 4] = [
        EquipmentStat::HuntCarry,
        EquipmentStat::ForageCarry,
        EquipmentStat::PenCarry,
        EquipmentStat::ScoutVantageRange,
    ];

    /// **The stat whose EQUIPPED rate this one borrows**, when its equipped side is deliberately not
    /// its own number.
    ///
    /// A pen has always been collected at the hunt haul's equipped rate, and it keeps sharing it:
    /// the number lives once, on the sled's tier, and both the range and the camp read it there. It
    /// is a link rather than a copy because a copy is a second home to drift from — which is the
    /// whole reason this stat pair was authored the way it was
    /// (`.claude/rules/core_sim/equipment.md` → "A pen is collected on `pen_carry`").
    pub fn shares_equipped_rate_with(self) -> Option<EquipmentStat> {
        match self {
            EquipmentStat::PenCarry => Some(EquipmentStat::HuntCarry),
            _ => None,
        }
    }

    /// **The stats only a bench TOOL may declare** — a tool bounds one material and grants nothing
    /// outside it, the shape `max_body_mass` already runs on.
    ///
    /// **Deliberately NOT in [`Self::TWO_TIER`]**, though each has an unequipped reading: that
    /// fallback searches the whole item table and takes the first match, so it would answer the
    /// *loom's* speed for a band scraping a hide bare-handed. Every one of these three falls back to
    /// a property of the **material** instead ([`crate::materials_config::HandWorking`]), which is
    /// the only thing that knows which material is being worked. One home per fact, and the home is
    /// the material.
    pub const CRAFT_ONLY: [EquipmentStat; 3] = [
        EquipmentStat::CraftSpeed,
        EquipmentStat::CraftQualityCeiling,
        EquipmentStat::CraftMaterialEfficiency,
    ];

    /// Whether this stat is one of [`Self::CRAFT_ONLY`].
    pub fn is_craft_stat(self) -> bool {
        Self::CRAFT_ONLY.contains(&self)
    }
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
    /// Per **landed strike that went into a body**. Spears, traps and clubs.
    ///
    /// # It replaced `Kill` AND `Fight`, and both for the same reason
    ///
    /// A kill charged the whole party for a body one part of it brought down, and a fight charged
    /// the whole warrior line for an engagement most of it may never have swung in. Both pretended
    /// every piece of gear present did the work. **A strike is what a weapon actually does**, so it
    /// is the honest quantum for anything swung — and it is attributed to the *crew that threw it*
    /// ([`crate::combat::ContingentResult::strikes_landed`]), which is what lets a bare-handed run
    /// inside a speared party pay nothing.
    ///
    /// **It is scaled by what the bodies could absorb.** Ten hunters deal enough damage for five
    /// deer with two standing, so two-fifths of the swing did work and `10 × 0.4 = 4` spears are
    /// charged (`crate::combat::damage_absorbed`). Overkill against a thin herd is not free, but
    /// neither is it billed as if every blow found a body.
    ///
    /// **Still not a clock.** A party that marches all turn, waits out a herd it cannot afford to
    /// touch, or is never raided lands no strikes and pays nothing —
    /// `docs/plan_denial_raid.md` §1.2 intact.
    Strike,
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
    /// Per **item completed on the bench**. Bench tools.
    ///
    /// **The lesson the craft teaches is charged on this SAME quantum**
    /// ([`crate::systems::advance_crafting`]) — one wear and one lesson per item — so the thing that
    /// consumes the tool and the thing that teaches the craft cannot drift apart. It is a use count
    /// like the six above it, not a clock: a bench standing idle wears nothing, and one that
    /// finished nothing this turn pays nothing.
    ItemCrafted,
    /// Per **unit of build progress accrued** on a rung's per-source meter
    /// ([`crate::intensification::RungDef::build_accrual`]). Handling gear on a `Tame` or a
    /// `Corral`.
    ///
    /// **A build has no discrete event, so the quantum is the AMOUNT.** A kill, a hauled unit and a
    /// finished craft are all things that either happened or did not; clearing ground and gentling a
    /// herd is continuous work that shows up only as the meter moving. The meter's own increment is
    /// therefore what a use *is* — the same treatment the two biomass quanta give a take.
    ///
    /// **Every build totals [`crate::intensification::RUNG_COMPLETE`] of progress, so a build costs
    /// a FIXED amount of gear** whatever else is true of it. A Steppe Runner's 125-turn `Tame` and a
    /// rabbit's 25-turn one burn identical hurdles, and a crew building at a shallow floor halves
    /// its accrual and doubles its turns to arrive at the same total. That invariance is the reason
    /// to prefer it over a per-worker-turn charge, which would be a clock in a per-use costume and
    /// would additionally make the gear's cost track a species' `taming_rate` — the same
    /// species-dependence `equipment.md` rejected for `body_mass` on the learning rate.
    ///
    /// **Still not a clock.** A stalled build accrues nothing and pays nothing: a crew below its
    /// rung's knowledge gate, on a source it is not working, or holding no build verb at all moves
    /// the meter by zero and the charge is zero with it — `docs/plan_denial_raid.md` §1.2 intact.
    BuildProgress,
}

impl WearQuantum {
    /// **The plural noun a life readout counts this quantum in** — `48 raids left`, `120 kills left`.
    ///
    /// **Resolved sim-side, deliberately.** The life meter is a fuel gauge and reads in the item's
    /// own use quanta, never in percent, so *something* has to turn the enum into English — and a
    /// client that did it would be a second, silent copy of this table that a new quantum would not
    /// update. It is `wear.per` that decides the word: a club that wears per fight reads **raids**,
    /// a sled per biomass hauled reads **biomass hauled**.
    ///
    /// A *count* quantum gets a count noun; a *continuous* one keeps its own unit, because a
    /// "biomass" is not a countable event and inventing a per-turn conversion here would need a
    /// forecast of what the band is about to do.
    ///
    /// **[`Self::BuildProgress`] reads as a count and is not an exception to that.** It is a
    /// continuous quantum like the biomasses, but its unit is *already* the whole event: a rung
    /// completes at [`crate::intensification::RUNG_COMPLETE`] `== 1.0`, so one unit of progress is
    /// one finished build and "builds" is the unit rather than a conversion of it.
    ///
    /// **An item wearing on SEVERAL quanta is quoted on its FIRST** — see
    /// [`ItemDefinition::headline_wear`], which is where that choice is argued.
    pub fn noun(self) -> &'static str {
        match self {
            Self::Strike => "blows",
            Self::BiomassHauled => "biomass hauled",
            Self::BiomassGathered => "biomass gathered",
            Self::BiomassCollected => "biomass butchered",
            Self::TileRevealed => "new tiles",
            Self::ItemCrafted => "crafts",
            Self::BuildProgress => "builds",
        }
    }

    /// The singular of [`Self::noun`], for the `~1 raid left` rung — the one place a readout says
    /// *one*. A quantum whose noun is a mass term (`biomass hauled`) has no singular and reads the
    /// same either way.
    pub fn singular_noun(self) -> &'static str {
        match self {
            Self::Strike => "blow",
            Self::BiomassHauled => "biomass hauled",
            Self::BiomassGathered => "biomass gathered",
            Self::BiomassCollected => "biomass butchered",
            Self::TileRevealed => "new tile",
            Self::ItemCrafted => "craft",
            Self::BuildProgress => "build",
        }
    }
}

/// **When a life readout stops being green** — the two seams of the fuel gauge's colour, as
/// fractions of **one fresh unit's** worth of quanta.
///
/// A *fraction* rather than an absolute count because the quanta are not comparable across items: a
/// spear's life is 250 kills and a sled's is 5000 biomass, so one number would colour one of them
/// permanently red. It is only the **colour**; the wording itself is always the count
/// ([`WearQuantum::noun`]), because a percentage bar would draw a taper this model does not have.
///
/// A band holding several units reads above `1.0` and is therefore healthy, which is right: stock is
/// life.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifeReadoutConfig {
    /// Below this fraction of a fresh unit the row is `warn`.
    pub warn_fraction: f32,
    /// Below this fraction it is `danger`. Must be `< warn_fraction`.
    pub danger_fraction: f32,
}

/// An item's use quantum and what one use costs it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WearConfig {
    /// What counts as one use.
    pub per: WearQuantum,
    /// Condition spent per use, on the shared 0–100 scale.
    pub amount: f32,
}

/// **One QUALITY TIER of an item** — a flint spear against a bronze one.
///
/// **A tier is an AGE, and the vocabulary is shared across items**: every shipped item's one tier is
/// `flint`, and the day metal lands each gains a `bronze` beside it. That is what makes the upgrade
/// axis legible and gates it once (*"bronze needs Smithing"*) rather than per item.
///
/// **What the MATERIAL buys sits here; what is SHARED stays on the item.** A spear is a thrown
/// weapon whatever it is tipped with (`dispersion` and `exposure` on [`ItemDefinition::effects`]),
/// while `attack`, `starting_durability` and the carry rates are what the material changes.
///
/// **A mass bound rides with the effect it bounds**, so `traps`' `max_body_mass` sits on the tier's
/// `attack` rather than on the item: the bound is a property of *that effect*, and an effect with a
/// bound but no value is not representable — an effect names the value a stat takes. A second tier
/// of the passive device therefore restates its bound, which `validate_mass_bounds` still checks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EquipmentTier {
    /// Stable id, unique within the item — the age this tier belongs to (`flint`).
    pub id: String,
    /// Condition a fresh unit of this tier carries, on the shared 0–100 scale. A batch is equipped
    /// while its accumulated wear is **strictly below** this.
    pub starting_durability: f32,
    /// **The craft a faction must know before a bench can make this tier.** Absent on the first tier
    /// of every item — that one ships known, so nothing is locked at the start and the gate has a
    /// real job the day bronze exists. Validated against the crafts the materials table declares.
    #[serde(default)]
    pub requires_knowledge: Option<String>,
    /// What a unit of this tier sets while it is intact — overriding anything the item declares for
    /// the same stat.
    #[serde(default)]
    pub effects: Vec<EquipmentEffect>,
}

/// **One piece of equipment.** It owns what it does (`effects`), what wears it (`wear`) and the
/// **tiers** it can be made at — how long it lasts and what the material buys sit on those.
///
/// **`PartialEq` is what lets the designer catalogue's round-trip test compare the whole item table
/// at once** rather than field by field — a hand-listed comparison goes stale the moment an item
/// gains an axis, which is the failure `equipmentConfigJson` exists to make impossible.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemDefinition {
    /// **Every quantum this item is worn by, and what one use of each costs it.** Non-empty, and no
    /// quantum may appear twice. **Shared across tiers** — how fast a thing is used up is a property
    /// of the job, not of what it is made from; how long it survives that use is the tier's
    /// `starting_durability`.
    ///
    /// # A LIST, because an item may do more than one job
    ///
    /// It was a single [`WearConfig`], on the reasoning that one item does one job. That claim is
    /// about the *stats* an item lifts — a sled drags and a basket contains, and neither reaches the
    /// other's rate — and it survives. What it never covered is an item lifting **one** stat that
    /// two different sites read.
    ///
    /// `husbandry_gear` is that item. Hurdles, halters and a butchering stone are worked on the
    /// beast at a slaughter ([`WearQuantum::BiomassCollected`]) *and* on the animals being gentled
    /// during a `Tame` or fenced during a `Corral` ([`WearQuantum::BuildProgress`], issue #515) —
    /// the same physical bundle, two kinds of work. With one slot the second was unbillable, and
    /// leaving it uncharged would have let a band tame every herd on the map for free, which is the
    /// whole thing *wear follows the work actually done* exists to prevent.
    ///
    /// **The alternative was a second roster item**, which would have obeyed *one item, one job*
    /// literally while splitting one physical bundle in two — and the roster is already the surface
    /// issue #519 says does not scale. The list keeps the bundle whole and generalises: a bench tool
    /// that also serves a party would want exactly this.
    ///
    /// **A charge site names its quantum and gets that entry or nothing**
    /// ([`crate::components::BandEquipment::wear_kit`]), so two items on different quanta still
    /// cannot cross-charge and an item is still never billed for work it did not do.
    pub wear: Vec<WearConfig>,
    /// **The effects every tier of this item SHARES** — the multipliers, and the *unequipped* side
    /// of a rate whose equipped side lives elsewhere. Empty on an item whose whole payload is what
    /// the material buys.
    #[serde(default)]
    pub effects: Vec<EquipmentEffect>,
    /// **The quality tiers this item can be made at, worst first.** Non-empty; `tiers[0]` is the
    /// **default tier** — what a spawn stocks, what every reference rate resolves through, and the
    /// one tier that may not be knowledge-gated.
    ///
    /// A `Vec` rather than a map because the **order is the model**: a bench makes the best tier the
    /// faction knows, and a map has no order to ask.
    pub tiers: Vec<EquipmentTier>,
    /// **The ONE material this item is a bench tool for.** Absent on everything a party carries.
    ///
    /// A tool bounds one material and grants nothing outside it — the shape `max_body_mass` already
    /// runs on. It is what makes a loom useless on a hide with no *"this tool does not apply"*
    /// branch: the bench asks for the tool bounding the material it is working, and a loom is not
    /// that tool.
    ///
    /// **A tool serves the BENCH, not a party**: `validate` rejects a kit that names one, and its
    /// live predicate is ownership + condition rather than a kit mask.
    #[serde(default)]
    pub bounds_material: Option<String>,
    /// **How many workers one unit of this item takes to use.** A spear is `1` — one worker holds
    /// one spear, so ten hunters need ten spears and a party holding five sends five of them out
    /// bare-handed.
    ///
    /// **This is the item's OWN fact and may not be inferred from what it does.** The crew and the
    /// scope are independent axes: a four-worker net still raises carry *per worker crewing it*.
    /// Every effect shipped today is per-assigned-worker, which is why no scope field rides beside
    /// this one yet — a per-unit-scoped effect is the first thing to add the axis for, and adding it
    /// before an item wants it would be a dead axis.
    ///
    /// **A unit needs its FULL crew or it is not used** ([`EquipmentConfig::coverage`]): ten workers
    /// and three four-worker nets crew two nets with eight people and leave two unequipped, never a
    /// third of a net.
    ///
    /// Defaults to `1` so the shipped roster — every item of which is held by one person — says
    /// nothing, and `validate` rejects a zero.
    #[serde(default = "one_worker")]
    pub workers_per_unit: u32,
}

/// The `workers_per_unit` default — see [`ItemDefinition::workers_per_unit`]. A free function
/// because `serde(default = …)` names a path, not a literal.
fn one_worker() -> u32 {
    1
}

impl ItemDefinition {
    /// **What this item pays for one use of `quantum`**, or `None` if that quantum does not wear it
    /// at all.
    ///
    /// **`None` is the whole cross-charging guarantee**, and it is why every charge site names its
    /// quantum rather than handing over an amount: a hunt landing blows finds no entry on the
    /// baskets, so a gather kit cannot be blunted by a fight it was not in.
    pub fn wear_for(&self, quantum: WearQuantum) -> Option<&WearConfig> {
        self.wear.iter().find(|wear| wear.per == quantum)
    }

    /// **Is this item worn by `quantum` at all?** The predicate
    /// [`crate::components::BandEquipment::wear_kit`] filters its charge on.
    pub fn wears_on(&self, quantum: WearQuantum) -> bool {
        self.wear_for(quantum).is_some()
    }

    /// **The entry a LIFE READOUT quotes — the first, and the order is the model.**
    ///
    /// A fuel gauge reads in one unit ([`WearQuantum::noun`]), so an item worn by several quanta has
    /// to pick one: `≈12 builds` and `≈2500 biomass butchered` are the same condition counted two
    /// ways, and stating both would need the readout to know the usage mix it is precisely there to
    /// let the player choose. So the item DECLARES its headline by writing that quantum first, the
    /// same way `tiers[0]` declares the default tier.
    ///
    /// **The gauge is therefore accurate under one usage assumption, not unconditionally** — a band
    /// splitting its handling gear between a pen and a `Tame` runs out sooner than the pen-only
    /// count says. That is the same limit every rate-to-range conversion has, and the alternative —
    /// a per-quantum row — is a wire and readout change this slice deliberately does not make.
    ///
    /// `validate` guarantees the list is non-empty, so there is always an answer.
    pub fn headline_wear(&self) -> &WearConfig {
        self.wear
            .first()
            .expect("validate guarantees every item declares at least one wear quantum")
    }

    /// **The tier a spawn stocks and every reference rate resolves through** — the first, which
    /// `validate` guarantees exists and requires no knowledge.
    pub fn default_tier(&self) -> &EquipmentTier {
        self.tiers
            .first()
            .expect("validate guarantees every item declares at least one tier")
    }

    /// The tier with this id, or `None`.
    pub fn tier(&self, id: &str) -> Option<&EquipmentTier> {
        self.tiers.iter().find(|tier| tier.id == id)
    }

    /// **The tier this item is made at, or the default** — a batch naming a tier the config has
    /// since dropped falls back rather than vanishing, the same reading `has_condition` gives an
    /// item the table no longer carries.
    pub fn tier_or_default(&self, id: &str) -> &EquipmentTier {
        self.tier(id).unwrap_or_else(|| self.default_tier())
    }

    /// **The best tier this item can be made at by a faction that knows `known`** — the last in file
    /// order whose gate is satisfied, so the order is the upgrade ladder. The default tier requires
    /// nothing, so there is always an answer.
    pub fn craftable_tier(&self, known: impl Fn(&str) -> bool) -> &EquipmentTier {
        self.tiers
            .iter()
            .rev()
            .find(|tier| tier.requires_knowledge.as_deref().is_none_or(&known))
            .unwrap_or_else(|| self.default_tier())
    }

    /// **What one TIER of this item declares for a craft stat** — the tier's own entry if it has
    /// one, else the item's shared one, resolved the same way [`LiveItem::effect_entry`] does minus
    /// the grade layer.
    ///
    /// It exists for the caller that has **no serving batch to resolve against**: a readout about a
    /// bench tool the band does not own yet cannot ask what its live unit says, and quoting a
    /// different tool's number — or the top of the quality ladder — would advertise a ceiling this
    /// one does not reach.
    pub fn tier_craft_stat(&self, tier: &EquipmentTier, stat: EquipmentStat) -> Option<f32> {
        let find = |effects: &[EquipmentEffect]| {
            effects
                .iter()
                .find(|effect| effect.stat == stat)
                .map(|effect| effect.tier)
        };
        match find(&tier.effects).or_else(|| find(&self.effects)) {
            Some(EffectTier::Equipped(value)) => Some(value),
            _ => None,
        }
    }

    /// The material this item is a bench tool for, or `None` for ordinary party gear.
    pub fn bounds_material(&self) -> Option<&str> {
        self.bounds_material.as_deref()
    }

    /// Every effect this item can declare anywhere — its shared list plus every tier's. For
    /// `validate` and for the whole-table searches, neither of which has a batch to resolve against.
    fn every_effect(&self) -> impl Iterator<Item = &EquipmentEffect> {
        self.effects
            .iter()
            .chain(self.tiers.iter().flat_map(|tier| tier.effects.iter()))
    }

    /// The **shared** tier this item declares for `stat` — the unequipped side of a rate whose
    /// equipped side lives elsewhere. Tier-declared stats are not here; see [`LiveItem::effect`].
    fn shared_effect(&self, stat: EquipmentStat) -> Option<EffectTier> {
        self.effects
            .iter()
            .find(|effect| effect.stat == stat)
            .map(|effect| effect.tier)
    }
}

/// **What one live unit of an item grants** — the item's shared effects with the serving batch's
/// **tier** and **grade** layered over them, in that order.
///
/// **Three layers, one answer per stat, and the precedence is the specificity.** The item states
/// what is true of every one of these ever made; the tier states what the material bought; the grade
/// states what *this batch's* craft came out at. A grade may only declare a stat its tiers declare
/// (`validate_against`), so a grade always **replaces** a number rather than adding one.
#[derive(Debug, Clone, Copy)]
pub struct LiveItem<'a> {
    /// The item's own definition — its wear quantum, its shared effects.
    pub item: &'a ItemDefinition,
    /// The tier the serving batch was made at.
    pub tier: &'a EquipmentTier,
    /// The absolutes the serving batch's craft grade declares. **A start-stocked batch carries the
    /// anchor grade's NAME with an empty payload**
    /// ([`crate::components::BandEquipment::start_stocked_owned`]), which resolves here exactly as
    /// `None` does — its stats come from the tier, and that is what keeps the shipped opening
    /// unchanged by the stamp.
    pub grade: Option<&'a [EquipmentEffect]>,
}

impl<'a> LiveItem<'a> {
    /// **The one effect entry that answers for `stat`** — the grade's if it declares one, else the
    /// tier's, else the item's shared one. `validate` rejects a stat declared twice within a layer,
    /// so each layer has at most one entry to find.
    pub fn effect_entry(&self, stat: EquipmentStat) -> Option<&'a EquipmentEffect> {
        let find =
            |effects: &'a [EquipmentEffect]| effects.iter().find(|effect| effect.stat == stat);
        self.grade
            .and_then(find)
            .or_else(|| find(&self.tier.effects))
            .or_else(|| find(&self.item.effects))
    }

    /// The tier `stat` takes for this unit, or `None` if nothing in the three layers touches it.
    pub fn effect(&self, stat: EquipmentStat) -> Option<EffectTier> {
        self.effect_entry(stat).map(|effect| effect.tier)
    }

    /// **The equipped value this unit declares for a craft stat.** `None` when it says nothing about
    /// it — *present effects apply, absent ones do not*, the same "only declared values participate"
    /// clause [`KitChoice::multiplier`] runs on. A speed-only tool therefore leaves the ceiling and
    /// the efficiency exactly where the bare hand had them.
    pub fn craft_stat(&self, stat: EquipmentStat) -> Option<f32> {
        match self.effect(stat) {
            Some(EffectTier::Equipped(value)) => Some(value),
            _ => None,
        }
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

/// **One run of workers holding the SAME items** — a crew, and the thing a
/// [`crate::combat::Contingent`] is built from.
///
/// Its `kit` is the party's own [`KitChoice`] narrowed to the items *these* workers actually hold,
/// which is what lets every existing per-band resolver — `hunter_profile_against`,
/// `hunt_per_worker_biomass_capacity`, `multiplier` — answer for a crew with no new arm. **A crew is
/// not a new kind of kit**; it is the same kit with fewer things in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Crew {
    /// How many of the job's workers are in this run. Fractional, because a forecast's headcounts
    /// are (`hunt_engage_workers`), and a crew that rounded would make the projection disagree with
    /// the take it is projecting.
    pub workers: f32,
    /// The party's kit, narrowed to what this run is holding.
    pub kit: KitChoice,
}

/// **How a party's gear divides its people** — the output of [`EquipmentConfig::coverage`], and the
/// one answer every consumer of an unevenly-equipped party reads.
///
/// **Ordered best-equipped first.** The crews partition the job's whole headcount, so
/// `Σ crews.workers == workers` and a party with gear for nobody is one crew holding nothing rather
/// than an empty list — a caller must never have to distinguish *"no crews"* from *"one bare crew"*.
#[derive(Debug, Clone, PartialEq)]
pub struct KitCoverage {
    crews: Vec<Crew>,
    /// The kit the party was **sent out with**, unnarrowed — what a coverage with no crews in it
    /// still has to be able to answer for. A party of nobody has no crews to average over, and
    /// every rate below it would otherwise be a division by zero; the chosen kit's own rate is the
    /// answer every one of those sites gave before coverage existed.
    kit: KitChoice,
    /// **The head count this coverage was resolved for** — the party the crews partition.
    ///
    /// Stored rather than summed back out of [`Self::crews`], because it is the **denominator**
    /// every share and every weighted rate divides by, and a published *"10 of 17"* pair whose two
    /// halves came from different arithmetic is exactly the disagreement a reader cannot see. The
    /// crews telescope to it, but a float sum of a partition is not the number that went in.
    workers: f32,
}

impl KitCoverage {
    /// The crews, best-equipped first.
    pub fn crews(&self) -> &[Crew] {
        &self.crews
    }

    /// **The head count this coverage divides** — `0` for a job nobody is staffed on.
    ///
    /// The one authority for the denominator: [`Self::workers_holding`] over this is the whole
    /// *"10 of 17 armed"* sentence, and `Σ crews().workers` is the same number by construction.
    pub fn workers(&self) -> f32 {
        self.workers
    }

    /// The kit the party was sent out with, before any crew narrowed it.
    pub fn kit(&self) -> &KitChoice {
        &self.kit
    }

    /// **A per-worker rate for an unevenly-equipped party** — `Σ share × rate(crew's kit)`.
    ///
    /// Every consumer of a per-worker rate multiplies it by the head count, so the weighted mean is
    /// exactly the party's total divided by its people: five sledded hunters and five sledless
    /// haul `5 × 40 + 5 × 12`, quoted as a rate of `26`. **One scalar rather than a per-crew haul**
    /// because the take paths bound biomass against `workers × rate` and the inversions
    /// (`hunt_haul_workers`) invert that same product — a per-crew carry would need both to grow a
    /// crew loop for an answer identical to this one.
    ///
    /// A party with **no crews** (nobody assigned) answers at the chosen kit's own rate, which is
    /// what every one of these sites answered before coverage existed and keeps the inversions from
    /// dividing by zero.
    pub fn weighted_rate(&self, rate: impl Fn(&KitChoice) -> f32) -> f32 {
        // **The stored head count, not a sum of the crews** — one authority for the denominator
        // ([`Self::workers`]). `<=` rather than `!(> 0)` so a NaN takes the fallback too: a rate
        // divided by one would poison every consumer downstream.
        let total = self.workers;
        if !total.is_finite() || total <= 0.0 {
            return rate(&self.kit);
        }
        self.crews
            .iter()
            .map(|crew| (crew.workers / total) * rate(&crew.kit))
            .sum()
    }

    /// **Is every worker holding the same thing?** True for a party fully equipped *and* for one
    /// fully bare — both are one crew, and neither needs a mixed-force readout.
    pub fn is_uniform(&self) -> bool {
        self.crews.len() <= 1
    }

    /// Workers holding `item`, summed across crews — what a *"10 of 16 armed"* readout counts.
    pub fn workers_holding(&self, item: &str) -> f32 {
        self.crews
            .iter()
            .filter(|crew| crew.kit.uses().any(|used| used == item))
            .map(|crew| crew.workers)
            .sum()
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

    /// **The same kit with only `items` in it** — one crew's share of an unevenly-equipped party
    /// ([`EquipmentConfig::coverage`]).
    ///
    /// **The id is kept.** A crew short of spears is still out on the big-game kit — that is what
    /// the player chose and what `LaborAssignment.kitId` publishes — and minting a synthetic id per
    /// crew would put strings on the wire no roster carries.
    fn restricted_to(&self, items: Vec<Arc<str>>) -> KitChoice {
        KitChoice {
            id: Arc::clone(&self.id),
            uses: Arc::from(items),
        }
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

    /// The kit's items that are **still serving**, resolved against the batch actually in hand — the
    /// one iteration every stat resolution runs over.
    ///
    /// **A party resolves UNIFORMLY**, so one batch answers for the whole crew: the **serving**
    /// batch, which is the most-worn live one — the same batch [`crate::components::BandEquipment::wear_item`]
    /// charges, so what the party is priced at is what the party is spending. The partly-equipped
    /// party is issue #520 and is deliberately not this.
    fn live_items<'a>(
        &'a self,
        wear: &'a crate::components::BandEquipment,
        config: &'a EquipmentConfig,
    ) -> impl Iterator<Item = LiveItem<'a>> {
        self.uses
            .iter()
            .filter_map(move |item| config.live_item(item, wear))
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
                item.effect_entry(stat)
                    .filter(|effect| quarry.within(effect))
                    .and_then(|effect| match effect.tier {
                        EffectTier::Equipped(value) => Some(value),
                        EffectTier::Unequipped(_) => None,
                    })
            })
            .fold(None::<f32>, |best, value| {
                Some(best.map_or(value, |best| best.max(value)))
            })
    }

    /// **Is a live item in this kit supplying `stat` at all?** — the predicate the two-sided rates
    /// resolve on, whichever side the item happens to declare.
    fn supplies(
        &self,
        stat: EquipmentStat,
        wear: &crate::components::BandEquipment,
        config: &EquipmentConfig,
    ) -> bool {
        self.live_items(wear, config)
            .any(|item| item.effect(stat).is_some())
    }

    /// **The tier a live item in this kit declares for `stat`** — the best of them, so a kit
    /// carrying two things that both lift a rate is priced at the better one. `None` when nothing
    /// live touches the stat at all, which is the *"only declared values participate"* clause again.
    fn declared_by_live_item(
        &self,
        stat: EquipmentStat,
        wear: &crate::components::BandEquipment,
        config: &EquipmentConfig,
    ) -> Option<EffectTier> {
        self.live_items(wear, config)
            .filter_map(|item| item.effect(stat))
            .fold(None::<EffectTier>, |best, tier| match best {
                Some(best) if best.value() >= tier.value() => Some(best),
                _ => Some(tier),
            })
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
    /// **How decisively a kit must beat the hunt job's default before it replaces it on a quarry** —
    /// the one lever on [`crate::fauna::quarry_default_hunt_kit`], as a *fraction* of the default's
    /// own score.
    ///
    /// A near-tie keeps [`DefaultKitsConfig::hunt`], because a published default that flips on a
    /// trivial retune moves under the player for reasons they cannot see. `0.25` ships; the shipped
    /// roster's closest genuine win is the Silt Catfish's `1.67×`, so nothing is near the line.
    ///
    /// **Required, like every other key in this file** — no `serde` default. `Default::default()`'s
    /// `0.0` would read as *"any win at all, however small, republishes the default"*, which is
    /// exactly the flapping the lever exists to prevent, and a silently-defaulted lever is
    /// `config-loading.md`'s "looks live but isn't".
    pub quarry_default_kit_margin: f32,
    /// **THE OPENING RESERVE** — how many of each item a spawn stocks, as a multiple of the party's
    /// own head count ([`crate::components::BandEquipment::start_stocked_owned`]).
    ///
    /// A band needs `workers / workers_per_unit` units to arm everybody, so `1.0` would stock
    /// *exactly* enough and the **first break would disarm someone**: coverage counts units in
    /// usable condition, so the turn a spear retires the party goes out one hunter short. The
    /// half-again is what buys the band the turns between the first break and the bench.
    ///
    /// **Required, like every other key in this file** — no `serde` default, for the reason
    /// [`Self::quarry_default_kit_margin`] has none: `Default::default()`'s `0.0` would stock the
    /// floor of one unit per item and send a shipped band out with sixteen bare hands and one spear,
    /// which is `config-loading.md`'s "looks live but isn't" at its most expensive.
    pub start_stock_fraction: f32,
    /// **The life readout's two colour seams.** See [`LifeReadoutConfig`] — presentation tuning for
    /// the published `lifeSeverity`, and the only thing in this file the sim itself never reads.
    pub life_readout: LifeReadoutConfig,
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

    /// **Every kit this verb may be sent on, resolved, in file order.** The candidate set a
    /// per-quarry default is scored over, and the same `jobs` test
    /// [`Self::resolve_kit_for_job`] refuses a bad selection with — so a kit the command boundary
    /// would reject can never be published as a default.
    ///
    /// File order is the tie-break: a scorer folding on *strictly* greater keeps the earliest
    /// entry, so two kits that price identically resolve to the one the roster lists first rather
    /// than to whichever the iterator happened to reach last.
    pub fn kits_for_job(&self, job: KitJob) -> impl Iterator<Item = KitChoice> + '_ {
        self.kits
            .iter()
            .filter(move |kit| kit.jobs.contains(&job))
            .map(Self::choice_from)
    }

    /// **The kit this verb is sent on to work a source collected at `stat`** — the earliest entry,
    /// in file order, that supplies the stat at the **fresh** tier. `None` when the roster carries
    /// no such kit, which is a real answer: nothing on this roster can do that job properly, so the
    /// caller keeps whatever default it already had.
    ///
    /// **The source-axis question, asked of the roster rather than answered by an id.** A pen is
    /// collected on [`EquipmentStat::PenCarry`] and only handling gear supplies it, so *"which kit
    /// does a penned herd want"* is a lookup, not a score — and asking it here keeps the husbandry
    /// kit's id out of the sim, exactly as `hunter_profile` keeps the spear's out (see
    /// `.claude/rules/core_sim/equipment.md` → *"Nothing resolves a stat by naming an item"*).
    ///
    /// **Fresh, like every other default resolution**: a kit's *identity* as the one that supplies
    /// a stat is a property of quarry × roster and must not move as a band wears its gear down.
    pub fn kit_supplying(&self, job: KitJob, stat: EquipmentStat) -> Option<KitChoice> {
        let fresh = crate::components::BandEquipment::start_stocked(self);
        self.kits_for_job(job)
            .find(|kit| kit.supplies(stat, &fresh, self))
    }

    /// The roster entry with this id, or `None`.
    pub fn kit_definition(&self, id: &str) -> Option<&KitDefinition> {
        self.kits.iter().find(|kit| kit.id == id)
    }

    /// Resolve a roster id into the mask it stands for. `None` for an id the roster does not carry.
    pub fn kit(&self, id: &str) -> Option<KitChoice> {
        self.kit_definition(id).map(Self::choice_from)
    }

    /// **How this kit's gear divides `workers` into crews** — the seam every unevenly-equipped party
    /// resolves through, whatever it is doing.
    ///
    /// A party needs `workers / workers_per_unit` units of each item it carries. Own fewer and the
    /// shortfall goes out without that item — **per item**, so a party can be short of spears and
    /// long on sleds and the two shortfalls are different people only insofar as the counts differ.
    ///
    /// **The partition is by ITEM SET.** Each item covers a prefix of the party (the same people
    /// hold the same things — nothing here models *which* individual got the last spear, because
    /// nothing downstream can tell), so sorting the coverages and cutting at each distinct boundary
    /// yields runs of workers holding identical gear. Ten hunters with five spears and ten sleds is
    /// two crews: five on `{spears, sled}` and five on `{sled}`.
    ///
    /// **A unit needs its FULL crew.** For a multi-worker item only whole crews count, so ten
    /// workers and three four-worker nets crew two nets with eight people and leave two unequipped.
    /// **A one-worker item is exact rather than floored**, which is not an inconsistency: a forecast
    /// counts hunters in fractions (`hunt_engage_workers`), and flooring the common case would make
    /// the projection disagree with the take by up to a whole person.
    ///
    /// **A dead item covers nobody** — coverage counts units in usable condition
    /// ([`crate::components::BandEquipment::live_units`]), so the durability cliff arrives one
    /// person at a time instead of all at once. That graded disarmament is this model falling out,
    /// not a feature laid on top of it.
    pub fn coverage(
        &self,
        kit: &KitChoice,
        workers: f32,
        wear: &crate::components::BandEquipment,
    ) -> KitCoverage {
        // A non-finite head count takes the empty arm too — a NaN would otherwise flow into every
        // crew's `share`.
        if !workers.is_finite() || workers <= 0.0 {
            return KitCoverage {
                crews: Vec::new(),
                kit: kit.clone(),
                workers: 0.0,
            };
        }
        // What each of the kit's items covers, clamped to the party — an item the band has more of
        // than it can crew still only arms the people who are there.
        let covered: Vec<(&Arc<str>, f32)> = kit
            .uses
            .iter()
            .filter_map(|item| {
                let def = self.item(item)?;
                let per_unit = def.workers_per_unit as f32;
                let units = wear.live_units(item, self) as f32;
                // Two independent caps: the gear you hold, and the people you brought. The second is
                // whole crews only for a multi-worker unit — see the doc comment.
                let from_units = units * per_unit;
                let from_people = if def.workers_per_unit == 1 {
                    workers
                } else {
                    (workers / per_unit).floor() * per_unit
                };
                let covered = from_units.min(from_people);
                (covered > 0.0).then_some((item, covered))
            })
            .collect();

        // The cut points: every distinct coverage strictly inside the party, then the party's edge.
        let mut cuts: Vec<f32> = covered
            .iter()
            .map(|(_, covered)| *covered)
            .filter(|covered| *covered < workers)
            .collect();
        cuts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        cuts.dedup();
        cuts.push(workers);

        let mut crews = Vec::with_capacity(cuts.len());
        let mut floor = 0.0_f32;
        for cut in cuts {
            // A worker at position `p` holds an item iff `p < covered`, so a run ending at `cut`
            // holds exactly the items covering at least that far. The comparison is exact: every
            // `cut` is either one of these very `covered` values or the clamp they were capped to.
            let held: Vec<Arc<str>> = covered
                .iter()
                .filter(|(_, covered)| *covered >= cut)
                .map(|(item, _)| Arc::clone(item))
                .collect();
            crews.push(Crew {
                workers: cut - floor,
                kit: kit.restricted_to(held),
            });
            floor = cut;
        }
        KitCoverage {
            crews,
            kit: kit.clone(),
            workers,
        }
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

    /// **The bench tool for a material, whether or not this band has one** — the item whose
    /// `bounds_material` names it. Unique by `validate`, so there is one answer.
    ///
    /// Used for the *refusal* readout (*"No loom"*) and for nothing that resolves a rate; a rate
    /// goes through [`Self::live_bench_tool`], which also asks whether the band owns it.
    pub fn bench_tool_for(&self, material: &str) -> Option<(&str, &ItemDefinition)> {
        self.items()
            .find(|(_, item)| item.bounds_material() == Some(material))
    }

    /// **The tool a band actually has at the bench for `material`** — owned *and* with condition
    /// left. `None` is the ordinary opening state and it is not an error: the band works the
    /// material bare-handed, at the rate and ceiling the **material** declares.
    ///
    /// **Ownership is the ordinary question now.** It was the tool's alone while an absent ledger
    /// entry still read as a full item for everything a spawn stocks; the count slice flipped that
    /// invariant for every item, so *"does the band have one"* and *"has it any condition left"* are
    /// one reading — [`crate::components::BandEquipment::has_condition`] — and this joins it to the
    /// material lookup rather than to a second ownership test.
    ///
    /// **Nothing resolves a stat by naming an item**: the caller passes the *material*, so a roster
    /// that renames the loom moves the bench with it and the id is spelled only in config.
    pub fn live_bench_tool<'a>(
        &'a self,
        material: &str,
        wear: &'a crate::components::BandEquipment,
    ) -> Option<LiveItem<'a>> {
        let (id, _) = self.bench_tool_for(material)?;
        self.live_item(id, wear)
    }

    /// **The item that declares `stat` as a SHARED effect, whatever kit is in play** — how a
    /// two-sided rate finds its *unequipped* value even when the kit carrying it is absent or spent.
    /// Deliberately searches the whole table rather than the kit: a party with no handling gear
    /// still needs to know what a bare-handed pen collects, and that number lives on the gear.
    ///
    /// **Shared effects only, never a tier's.** An unequipped side is true of every tier of an item
    /// by construction — it is what you get when the item is *not there* — so a tier declaring one
    /// is rejected at validate rather than reachable here.
    fn declared_tier(&self, stat: EquipmentStat) -> Option<EffectTier> {
        self.items
            .values()
            .find_map(|item| item.shared_effect(stat))
    }

    /// **The batch of `item` this band is actually using, resolved to what it grants** — `None` when
    /// the band owns none with condition left, which is the same reading a band that never had one
    /// gets. See [`crate::components::BandEquipment`]: **an absent entry is NOT OWNED**.
    pub fn live_item<'a>(
        &'a self,
        item: &str,
        wear: &'a crate::components::BandEquipment,
    ) -> Option<LiveItem<'a>> {
        let def = self.item(item)?;
        let batch = wear.serving_batch(item, self)?;
        Some(LiveItem {
            item: def,
            tier: def.tier_or_default(&batch.tier),
            grade: batch.grade.as_ref().map(|grade| grade.effects.as_slice()),
        })
    }

    /// **Every item a spawn stocks** — the ids some kit `uses`, in id order.
    ///
    /// *"The band's start kits"* stated as the roster states it: an item no kit carries is not
    /// something a party was ever sent out with, and a **bench tool** can never appear here because
    /// `validate` rejects a kit that names one — which is what keeps *"tools are earned, never a
    /// prerequisite"* true without a second rule.
    pub fn start_stocked_items(&self) -> impl Iterator<Item = (&str, &ItemDefinition)> {
        self.items().filter(|(id, _)| {
            self.kits
                .iter()
                .any(|kit| kit.uses.iter().any(|used| used == id))
        })
    }

    /// **How many units of `item` a spawn stocks for a party of `workers`** —
    /// `ceil(workers × start_stock_fraction / workers_per_unit)`, never below one unit.
    ///
    /// **A whole unit, because a unit is what a person holds.** The fraction is an *opening
    /// reserve*, not a supply of half-spears: rounding up is what makes the last hunter armed rather
    /// than nearly armed. The floor of one is for the degenerate party — a fixture with no workers
    /// still owns the item, which is what keeps *"an absent entry is not owned"* readable.
    pub fn start_stock_units(&self, item: &ItemDefinition, workers: f32) -> u32 {
        let workers = if workers.is_finite() {
            workers.max(0.0)
        } else {
            0.0
        };
        let wanted = (workers * self.start_stock_fraction) / item.workers_per_unit.max(1) as f32;
        (wanted.ceil().max(1.0) as u32).max(1)
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
        self.resolve_kit_or(id, job, self.default_kit(job))
    }

    /// **The same resolution, with the ABSENT case answered by the caller.** A Hunt row that names
    /// no kit resolves the *herd's* default ([`crate::fauna::quarry_default_hunt_kit`]) rather than
    /// the job's, because that is what the wire published and what the compose sheet opened on —
    /// resolving the job default there would run Stalking on a warren whose sheet said Trapping,
    /// which is the silent substitution [`KitSelectionError`] exists to prevent, arriving through
    /// the absent-token door.
    ///
    /// **The NAMED path is untouched and is the only validated one.** `absent` is a `KitChoice` the
    /// caller has already resolved off this roster, so it cannot smuggle in an unknown id or a kit
    /// that does not cover the job.
    pub fn resolve_kit_or(
        &self,
        id: Option<&str>,
        job: KitJob,
        absent: KitChoice,
    ) -> Result<KitChoice, KitSelectionError> {
        let Some(id) = id else {
            return Ok(absent);
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

    /// **A band's per-worker HUNT haul rate** — resolved against the **no-equipment baseline** the
    /// caller already holds (`labor_config.hunt.per_worker_biomass_capacity`), with the sled's own
    /// tier supplying the sledded rate. The single seam every hunt-take, crew-size and hunt-forecast
    /// site reads, so the assign-time seed and the resolved row can never disagree about which tier
    /// a band is on.
    ///
    /// **Baskets cannot reach this by construction** — they declare [`EquipmentStat::ForageCarry`],
    /// a different stat, so dragging a carcass stays unrelated to how much you can hold (§4.8).
    pub fn hunt_per_worker_biomass_capacity(
        &self,
        baseline_rate: f32,
        kit: &KitChoice,
        wear: &crate::components::BandEquipment,
    ) -> f32 {
        self.rate_tier(EquipmentStat::HuntCarry, baseline_rate, kit, wear)
    }

    /// **A band's per-worker GATHER throughput** — resolved against the **no-equipment baseline** the
    /// caller already holds (`labor_config.forage.per_worker_biomass_capacity`), before the tile's
    /// seasonal weight is folded in ([`crate::forage::forage_per_worker_biomass`]).
    ///
    /// **The sled cannot reach this by construction** — it declares [`EquipmentStat::HuntCarry`].
    pub fn forage_per_worker_biomass_capacity(
        &self,
        baseline_rate: f32,
        kit: &KitChoice,
        wear: &crate::components::BandEquipment,
    ) -> f32 {
        self.rate_tier(EquipmentStat::ForageCarry, baseline_rate, kit, wear)
    }

    /// **A band's per-keeper PEN collection rate** — resolved against the **no-equipment baseline**
    /// the caller already holds. Its equipped side is the **hunt haul's**, resolved through
    /// [`Self::equipped_reference`] rather than declared again here, so a keeper carrying husbandry
    /// gear collects **exactly what a pen always collected** and that number keeps its one home.
    ///
    /// **The sled cannot reach this by construction**, and that is the deliberate consequence: a
    /// hunting party that has corralled a herd and left its assignment on the big-game kit is
    /// working the pen with a drag harness and no handling gear, and collects at the bare rate. That
    /// is the same shape as bringing baskets to a deer — see [`EquipmentStat::PenCarry`].
    pub fn pen_per_worker_biomass_capacity(
        &self,
        baseline_rate: f32,
        kit: &KitChoice,
        wear: &crate::components::BandEquipment,
    ) -> f32 {
        self.rate_tier(EquipmentStat::PenCarry, baseline_rate, kit, wear)
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
        self.rate_tier(EquipmentStat::ScoutVantageRange, equipped_range, kit, wear)
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

    /// **The four two-sided rates' shared resolution.** `baseline` is the **no-equipment** rate the
    /// caller already holds (`labor_config.json`'s), and the gear's own declaration is what lifts it.
    ///
    /// Three arms, and which one runs is one-home-per-fact rather than free choice:
    ///
    /// - a live item declaring the **equipped** side (the two carries, on their tier) *is* the
    ///   answer — that is what the material bought;
    /// - a live item declaring the **unequipped** side (the pen, the vantage) means the *equipped*
    ///   rate applies, and that rate is looked up through [`Self::equipped_reference`] because it
    ///   lives somewhere else;
    /// - nothing live ⇒ [`Self::declared_tier`] over the **whole item table**, because a party with
    ///   no handling gear still has to know what a bare-handed pen collects and that number lives on
    ///   the gear it is not carrying. With nothing declaring an unequipped side either, the
    ///   `baseline` stands.
    fn rate_tier(
        &self,
        stat: EquipmentStat,
        baseline: f32,
        kit: &KitChoice,
        wear: &crate::components::BandEquipment,
    ) -> f32 {
        match kit.declared_by_live_item(stat, wear, self) {
            Some(EffectTier::Equipped(value)) => value,
            Some(EffectTier::Unequipped(_)) => self.equipped_reference(stat, baseline),
            None => match self.declared_tier(stat) {
                Some(EffectTier::Unequipped(value)) => value,
                _ => baseline,
            },
        }
    }

    /// **The shipped EQUIPPED rate for a stat — what a fully-stocked band resolves**, and the one
    /// seam every readout with no band to resolve against reads.
    ///
    /// It is the item table's **default tier** declaration, because that is where the equipped side
    /// of a carry now lives; `baseline` (the caller's `labor_config.json` no-equipment rate) is the
    /// honest answer when nothing in the table lifts the stat at all, since then there is no
    /// equipped tier to be at.
    ///
    /// **A stat may BORROW another's equipped rate rather than own one**
    /// ([`EquipmentStat::shares_equipped_rate_with`]): a pen is collected at the hunt haul's rate and
    /// keeps being, so the number stays on the sled's tier and both readers resolve it here.
    pub fn equipped_reference(&self, stat: EquipmentStat, baseline: f32) -> f32 {
        let source = stat.shares_equipped_rate_with().unwrap_or(stat);
        self.items
            .values()
            .find_map(|item| {
                item.default_tier()
                    .effects
                    .iter()
                    .find(|effect| {
                        effect.stat == source && matches!(effect.tier, EffectTier::Equipped(_))
                    })
                    .map(|effect| effect.tier.value())
            })
            .unwrap_or(baseline)
    }

    /// **How much the quarry's own `wariness` is multiplied by** for a party carrying this kit — see
    /// [`KitChoice::multiplier`] for why it is the maximum of what the live items declare.
    pub fn dispersion(&self, kit: &KitChoice, wear: &crate::components::BandEquipment) -> f32 {
        kit.multiplier(EquipmentStat::Dispersion, wear, self)
    }

    /// **How much faster a rung's build meter fills** for a crew carrying this kit — the factor
    /// [`crate::intensification::RungDef::build_accrual`] applies, `1.0` for a kit carrying nothing
    /// that helps. The maximum of what the live items declare, for
    /// [`KitChoice::multiplier`]'s reason: two tools that both speed the work do not compound, you
    /// simply use the better one.
    ///
    /// **It is resolved off the CREW'S kit, not the band's ownership**, like every other stat here —
    /// so gear that could have sped a `Tame` and was left behind on another kit speeds nothing, and
    /// choosing the handling kit for the climb costs the hunt job whatever that kit does not carry.
    /// That trade is the decision the stat exists to create.
    pub fn build_rate(&self, kit: &KitChoice, wear: &crate::components::BandEquipment) -> f32 {
        kit.multiplier(EquipmentStat::BuildRate, wear, self)
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
            if let Some(effect) = item.effect_entry(EquipmentStat::Attack) {
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

    /// **Every tier a kit grants, resolved once, for one `(kit, wear)` pair.**
    ///
    /// The nine numbers a consumer needs to describe what sending *this* kit buys, each through the
    /// same seam the take path reads it through — so a readout cannot drift from what the raid
    /// actually pays.
    ///
    /// # It exists because the same arithmetic had two callers and was about to have three
    ///
    /// `snapshot::kit_roster_states` resolves it per kit over a **fresh** ledger (the picker's
    /// reference), and `snapshot::population_state` resolves it per band over that band's **live**
    /// ledger. Those differ only in the `wear` argument, and the per-band-per-kit readout would have
    /// been a third copy of the same nine calls. One function, three call sites, no drift.
    ///
    /// **Every axis a kit can lift is here, and adding one here is what keeps the two readings in
    /// step.** The pen's collection rate and the scout vantage's reach were resolved *beside* this
    /// call at the roster site and not at all at the per-band site, so the per-kit rows went to the
    /// wire without them and a client fell back to the FRESH tier for exactly those two — a pen sheet
    /// quoting 40 per keeper against a sim collecting 12, and a Scout card quoting 2 tiles against a
    /// reveal at 1.
    ///
    /// **`attack` resolves UNBOUNDED**, because this is a statement about the *kit* and there is no
    /// quarry in scope — the mass window rides beside it
    /// ([`ResolvedKitTiers::attack_min_body_mass`] / `attack_max_body_mass`) so a consumer can gate
    /// against the animal in front of it. A path that *has* a quarry must resolve
    /// [`Self::hunter_profile_against`] instead; see "Two named resolvers".
    ///
    /// **`warrior_attack` is deliberately NOT here.** It is the same `attack` this already resolves,
    /// read through a different *kit* rather than a different stat — so a band's warrior tier is the
    /// warrior kit's own row, not a tenth number on every row.
    pub fn resolve_kit_tiers(
        &self,
        hunter_intrinsic: CombatStats,
        baseline_haul_rate: f32,
        baseline_gather_rate: f32,
        equipped_vantage_range: f32,
        kit: &KitChoice,
        wear: &crate::components::BandEquipment,
    ) -> ResolvedKitTiers {
        let (attack_min_body_mass, attack_max_body_mass) = self.attack_mass_bounds(kit, wear);
        ResolvedKitTiers {
            attack: self
                .hunter_profile_unbounded(hunter_intrinsic, kit, wear)
                .attack,
            hunt_carry_per_worker_biomass: self.hunt_per_worker_biomass_capacity(
                baseline_haul_rate,
                kit,
                wear,
            ),
            forage_carry_per_worker_biomass: self.forage_per_worker_biomass_capacity(
                baseline_gather_rate,
                kit,
                wear,
            ),
            // **The pen shares the HUNT haul's equipped rate**, resolved off the sled's own tier
            // through `EquipmentStat::shares_equipped_rate_with` so that number keeps its one home —
            // but it resolves through `EquipmentStat::PenCarry`, so a kit with a sled and no handling
            // gear reads the bare rate here beside the sledded rate above. Both sides fall back to
            // the same `labor_config.hunt.per_worker_biomass_capacity` baseline.
            pen_carry_per_worker_biomass: self.pen_per_worker_biomass_capacity(
                baseline_haul_rate,
                kit,
                wear,
            ),
            scout_vantage_range: self.scout_vantage_range(equipped_vantage_range, kit, wear),
            // `0` is the *sentinel* for "unbounded" on both ends — the schema's own default, and what
            // every weapon but the passive device ships.
            attack_min_body_mass: attack_min_body_mass.unwrap_or(UNBOUNDED_BODY_MASS),
            attack_max_body_mass: attack_max_body_mass.unwrap_or(UNBOUNDED_BODY_MASS),
            dispersion: self.dispersion(kit, wear),
            exposure: self.exposure(kit, wear),
            build_rate: self.build_rate(kit, wear),
        }
    }

    /// Invariants a TOE config must satisfy. **An item with no wear rate is not consumable** and one
    /// with no durability is born dry, so both are rejected rather than shipped as a silently eternal
    /// (or silently absent) item.
    ///
    /// `Invalid` names a **dynamic** field path now (`items.spears.headline_wear().amount`) rather than one of a
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
            Self::validate_wear(id, item)?;
            Self::validate_effect_layer(&format!("items.{id}.effects"), id, &item.effects)?;
            Self::validate_tiers(id, item)?;
            if item.every_effect().next().is_none() {
                return Err(EquipmentConfigError::InvalidRoster {
                    reason: format!(
                        "item '{id}' declares no effects on itself or any of its tiers - it would \
                         wear out doing nothing"
                    ),
                });
            }
            // **A zero-crew unit would cover an unbounded number of workers.** `coverage` divides
            // the workers on the job by this, so `0` is not "free gear for everybody" by design —
            // it is a division that has to be given a meaning somewhere, and the honest place to
            // refuse it is here. An item genuinely serving a whole party regardless of size is a
            // per-unit-scoped *effect*, which is a different axis and not yet shipped.
            if item.workers_per_unit == 0 {
                return Err(EquipmentConfigError::Invalid {
                    field: format!("items.{id}.workers_per_unit"),
                    constraint: "be at least 1 - a unit no worker has to hold covers everyone"
                        .to_string(),
                    value: "0".to_string(),
                });
            }
        }
        // **The two-sided rates must be declared by at most ONE item each**, because `declared_tier`
        // and `equipped_reference` both search the whole table and take the FIRST match: two items
        // disagreeing about the sledless haul rate would resolve by `BTreeMap` order, which is
        // alphabetical and therefore arbitrary. Counted over the item's **whole** declaration
        // surface — shared effects and every tier's — because either home answers that search.
        for stat in EquipmentStat::TWO_TIER {
            let declared: Vec<&str> = self
                .items
                .iter()
                .filter(|(_, item)| item.every_effect().any(|effect| effect.stat == stat))
                .map(|(id, _)| id.as_str())
                .collect();
            if declared.len() > 1 {
                return Err(EquipmentConfigError::InvalidRoster {
                    reason: format!(
                        "items {} all declare the same two-sided rate {stat:?} - the whole-table fallback would resolve by name order",
                        declared.join(", ")
                    ),
                });
            }
        }
        // **The quarry-default margin is a FRACTION, so `0` is legal and negative is not.** `0`
        // means "any strict win republishes the default" — flappy, but a coherent choice a tuner may
        // want while measuring; a negative margin would let a kit that scores *worse* than the job
        // default take its place, which is not a weaker rule but an inverted one.
        if !self.quarry_default_kit_margin.is_finite() || self.quarry_default_kit_margin < 0.0 {
            return Err(EquipmentConfigError::Invalid {
                field: "quarry_default_kit_margin".to_string(),
                constraint: "be finite and not negative".to_string(),
                value: self.quarry_default_kit_margin.to_string(),
            });
        }
        // **The opening reserve is a MULTIPLE of the head count, so it must be strictly positive.**
        // `0` would stock the one-unit floor whatever the band's size — the pre-coverage behaviour,
        // which now means one armed hunter and the rest bare — and a negative multiple has no
        // reading at all.
        if !self.start_stock_fraction.is_finite() || self.start_stock_fraction <= 0.0 {
            return Err(EquipmentConfigError::Invalid {
                field: "start_stock_fraction".to_string(),
                constraint: "be finite and greater than zero".to_string(),
                value: self.start_stock_fraction.to_string(),
            });
        }
        // **The life readout's two seams**: both fractions of one fresh unit, both in `0..=1`, and
        // `danger` strictly below `warn` — a danger seam at or above the warn seam would make the
        // warn band unreachable, so one colour would simply never appear.
        let life = self.life_readout;
        for (field, value) in [
            ("life_readout.warn_fraction", life.warn_fraction),
            ("life_readout.danger_fraction", life.danger_fraction),
        ] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(EquipmentConfigError::Invalid {
                    field: field.to_string(),
                    constraint: "be finite and within 0..=1".to_string(),
                    value: value.to_string(),
                });
            }
        }
        if life.danger_fraction >= life.warn_fraction {
            return Err(EquipmentConfigError::Invalid {
                field: "life_readout.danger_fraction".to_string(),
                constraint: "be strictly below warn_fraction".to_string(),
                value: format!("{} vs warn {}", life.danger_fraction, life.warn_fraction),
            });
        }
        self.validate_bench_tools()?;
        self.validate_roster()?;
        self.validate_default_hunt_kit_is_quarry_blind()?;
        self.validate_warrior_kits_have_no_quarry()
    }

    /// **One layer of an item's effects** — its shared list, or one tier's. Each layer is validated
    /// on its own because each is searched on its own: [`LiveItem::effect_entry`] takes the first
    /// match *within* a layer, so a stat declared twice in one is a silently dead line and two
    /// entries disagreeing about a number the fight reads.
    fn validate_effect_layer(
        field: &str,
        id: &str,
        effects: &[EquipmentEffect],
    ) -> Result<(), EquipmentConfigError> {
        for (index, effect) in effects.iter().enumerate() {
            let value = effect.tier.value();
            if !value.is_finite() || value < 0.0 {
                return Err(EquipmentConfigError::Invalid {
                    field: format!("{field}[{index}]"),
                    constraint: "be finite and not negative".to_string(),
                    value: value.to_string(),
                });
            }
            if effects[..index]
                .iter()
                .any(|prior| prior.stat == effect.stat)
            {
                return Err(EquipmentConfigError::InvalidRoster {
                    reason: format!("{field} declares the same stat twice"),
                });
            }
            Self::validate_mass_bounds(id, index, effect)?;
        }
        Ok(())
    }

    /// **AN ITEM'S WEAR ENTRIES.** Three checks, each closing a way for a charge to go missing:
    ///
    /// - **at least one entry** — an item nothing wears is not consumable, so its role never steps
    ///   down and the whole durability cliff is absent for it;
    /// - **a positive amount on every entry** — a `0` is an entry that looks live at the dials and
    ///   bills nothing, the same "looks live but isn't" this file rejects everywhere else. It is
    ///   checked per entry rather than once, because a second entry silently costing nothing is
    ///   exactly the new failure the list makes possible;
    /// - **no quantum twice** — [`ItemDefinition::wear_for`] answers the *first* match, so a
    ///   duplicate is dead config whose amount a reader would reasonably assume was in play. It is
    ///   `tiers`' unique-id rule, one field over.
    fn validate_wear(id: &str, item: &ItemDefinition) -> Result<(), EquipmentConfigError> {
        if item.wear.is_empty() {
            return Err(EquipmentConfigError::Invalid {
                field: format!("items.{id}.wear"),
                constraint: "name at least one quantum - an item nothing wears is never consumed, \
                             so its role could never step down"
                    .to_string(),
                value: "[]".to_string(),
            });
        }
        let mut seen: Vec<WearQuantum> = Vec::with_capacity(item.wear.len());
        for (index, wear) in item.wear.iter().enumerate() {
            Self::require_positive(format!("items.{id}.wear[{index}].amount"), wear.amount)?;
            if seen.contains(&wear.per) {
                return Err(EquipmentConfigError::Invalid {
                    field: format!("items.{id}.wear[{index}].per"),
                    constraint: "name a quantum no earlier entry already names - `wear_for` \
                                 answers the first match, so a duplicate is dead config"
                        .to_string(),
                    value: format!("{:?}", wear.per),
                });
            }
            seen.push(wear.per);
        }
        Ok(())
    }

    /// **AN ITEM'S QUALITY TIERS.** A tier is what the material bought, and every check here closes
    /// a way for that to be silently untrue:
    ///
    /// - **at least one tier**, or the item has no durability and is born dry;
    /// - **unique ids**, or [`ItemDefinition::tier`] answers the first of two and a batch's recorded
    ///   tier picks one by file order;
    /// - **the first tier is knowledge-free** — it is what a spawn stocks and what every reference
    ///   rate resolves through, so a gate on it would lock the shipped opening behind a craft nobody
    ///   knows on turn 1;
    /// - **a tier may not restate a stat the item shares**, because the tier would silently win
    ///   ([`LiveItem::effect_entry`]) and the shared line would be dead config — the "declares the
    ///   same stat twice" rule one level up;
    /// - **a tier may not declare an `unequipped` side.** An unequipped value is what you get when
    ///   the item is *not there*, which is true of every tier at once, so it belongs on the item.
    fn validate_tiers(id: &str, item: &ItemDefinition) -> Result<(), EquipmentConfigError> {
        if item.tiers.is_empty() {
            return Err(EquipmentConfigError::InvalidRoster {
                reason: format!(
                    "item '{id}' declares no tiers - it would have no durability and be born dry"
                ),
            });
        }
        for (index, tier) in item.tiers.iter().enumerate() {
            Self::require_positive(
                format!("items.{id}.tiers.{}.starting_durability", tier.id),
                tier.starting_durability,
            )?;
            if item.tiers[..index].iter().any(|prior| prior.id == tier.id) {
                return Err(EquipmentConfigError::InvalidRoster {
                    reason: format!("item '{id}' declares the tier '{}' twice", tier.id),
                });
            }
            Self::validate_effect_layer(
                &format!("items.{id}.tiers.{}.effects", tier.id),
                id,
                &tier.effects,
            )?;
            for effect in &tier.effects {
                if item.effects.iter().any(|shared| shared.stat == effect.stat) {
                    return Err(EquipmentConfigError::InvalidRoster {
                        reason: format!(
                            "item '{id}' declares {:?} on both itself and its tier '{}' - the tier \
                             wins, so the shared line would be dead config",
                            effect.stat, tier.id
                        ),
                    });
                }
                if let EffectTier::Unequipped(_) = effect.tier {
                    return Err(EquipmentConfigError::InvalidRoster {
                        reason: format!(
                            "item '{id}' declares {:?} `unequipped` on its tier '{}' - an unequipped \
                             value is what you get when the item is NOT there, which is true of every \
                             tier at once, so it belongs on the item",
                            effect.stat, tier.id
                        ),
                    });
                }
            }
        }
        if item.default_tier().requires_knowledge.is_some() {
            return Err(EquipmentConfigError::InvalidRoster {
                reason: format!(
                    "item '{id}' gates its first tier '{}' on knowledge - that tier is what a spawn \
                     stocks and what every reference rate resolves through, so it must ship known",
                    item.default_tier().id
                ),
            });
        }
        Ok(())
    }

    /// **A BENCH TOOL'S OWN INVARIANTS.** Each failure below is silent at runtime — the item parses,
    /// validates, and then either stretches nothing or is never worn — which is exactly
    /// `config-loading.md`'s "looks live but isn't".
    ///
    /// - **A craft stat needs a `bounds_material`.** The three craft stats fall back to a property
    ///   of the *material*, so a craft stat on an item that names no material has nothing to be the
    ///   equipped side *of*.
    /// - **A craft stat is EQUIPPED-only.** Its unequipped side is the material's `hand_working`,
    ///   and an `unequipped` here would be a second, wrong home for it — read by nothing, since the
    ///   bench falls back to the material rather than to the table.
    /// - **One tool per material.** [`Self::bench_tool_for`] answers the first match, so two would
    ///   resolve by `BTreeMap` order, i.e. alphabetically.
    /// - **A tool wears on `item_crafted`, and only a tool does.** The bench is the only site that
    ///   charges that quantum, so a tool on any other quantum would be immortal and a spear on this
    ///   one would never wear at all.
    /// - **A tool declares at least one craft stat**, or it is gear that costs material, wears out
    ///   at a bench, and buys nothing.
    fn validate_bench_tools(&self) -> Result<(), EquipmentConfigError> {
        for (id, item) in &self.items {
            let is_tool = item.bounds_material().is_some();
            for effect in item.every_effect() {
                if !effect.stat.is_craft_stat() {
                    continue;
                }
                if !is_tool {
                    return Err(EquipmentConfigError::InvalidRoster {
                        reason: format!(
                            "item '{id}' declares the craft stat {:?} but bounds no material - a \
                             craft stat's unequipped side is a property of the MATERIAL, so there \
                             is nothing for this to be the equipped side of",
                            effect.stat
                        ),
                    });
                }
                if let EffectTier::Unequipped(_) = effect.tier {
                    return Err(EquipmentConfigError::InvalidRoster {
                        reason: format!(
                            "item '{id}' declares {:?} `unequipped` - the unequipped side of every \
                             craft stat is the material's own `hand_working`, and this would be a \
                             second home read by nothing",
                            effect.stat
                        ),
                    });
                }
            }
            // **A tool's bench quantum is its ONLY one, and the equality is what says so.** With
            // `wear` a list the honest reading is per-quantum: a tool must wear on `item_crafted`
            // (or it is immortal at the bench) and nothing else may (or it never wears at all,
            // since the bench is the only site charging it) — and a tool carrying a *second*
            // quantum would be a party item in a tool's clothing, which the kit ban below already
            // says it is not.
            let charges_bench_wear = item.wears_on(WearQuantum::ItemCrafted);
            if is_tool != charges_bench_wear {
                return Err(EquipmentConfigError::InvalidRoster {
                    reason: format!(
                        "item '{id}' bounds a material: {is_tool}, but wears per {:?} - the bench \
                         is the only site that charges `item_crafted`, so a tool on another \
                         quantum is immortal and anything else on this one never wears at all",
                        item.wear.iter().map(|wear| wear.per).collect::<Vec<_>>()
                    ),
                });
            }
            if is_tool && item.wear.len() > 1 {
                return Err(EquipmentConfigError::InvalidRoster {
                    reason: format!(
                        "item '{id}' is a bench tool but wears on {} quanta - a tool serves the \
                         bench alone, and no other charge site can reach one",
                        item.wear.len()
                    ),
                });
            }
            if is_tool
                && !item
                    .every_effect()
                    .any(|effect| effect.stat.is_craft_stat())
            {
                return Err(EquipmentConfigError::InvalidRoster {
                    reason: format!(
                        "item '{id}' is a bench tool but declares no craft stat - it would cost \
                         material, wear out, and buy nothing"
                    ),
                });
            }
            if let Some(material) = item.bounds_material() {
                let others: Vec<&str> = self
                    .items
                    .iter()
                    .filter(|(other, def)| {
                        other.as_str() != id.as_str() && def.bounds_material() == Some(material)
                    })
                    .map(|(other, _)| other.as_str())
                    .collect();
                if !others.is_empty() {
                    return Err(EquipmentConfigError::InvalidRoster {
                        reason: format!(
                            "items {id}, {} all bound '{material}' - the bench resolves the first \
                             match, so the tool would be picked by name order",
                            others.join(", ")
                        ),
                    });
                }
            }
        }
        // **A TOOL SERVES THE BENCH, NOT A PARTY.** A kit naming one would carry it onto the range,
        // where it grants nothing (no take path reads a craft stat) and is never worn (no take site
        // charges `item_crafted`) — a kit slot spent on nothing.
        for kit in &self.kits {
            for item in &kit.uses {
                if self
                    .item(item)
                    .and_then(ItemDefinition::bounds_material)
                    .is_some()
                {
                    return Err(EquipmentConfigError::InvalidRoster {
                        reason: format!(
                            "kit '{}' uses '{item}', which is a bench tool - a tool serves the \
                             bench and grants nothing to a party",
                            kit.id
                        ),
                    });
                }
            }
        }
        Ok(())
    }

    /// **Every `bounds_material` reconciled against the materials table** — the cross-config half,
    /// run at the composition seam in `build_headless_app` where both configs are in scope. Same
    /// `UnknownItem` debt the two food webs' yield edges pay: a tool bounding `hyde` would parse,
    /// validate, and then be the tool for nothing.
    pub fn validate_against_materials(
        &self,
        materials: &crate::materials_config::MaterialsConfig,
    ) -> Result<(), EquipmentConfigError> {
        let crafts = crate::crafting::crafts_declared_by(materials);
        for (id, item) in &self.items {
            // **A TIER'S GATE MUST NAME A REAL CRAFT.** A craft id is a `String`, so a tier gated on
            // `smithng` would parse, validate, and then be unreachable forever — the `UnknownItem`
            // debt again, and in its most expensive direction: the content is authored and cannot be
            // earned. Asked of the materials table rather than of a coded list, so a craft arrives
            // with the material that teaches it.
            for tier in &item.tiers {
                let Some(craft) = tier.requires_knowledge.as_deref() else {
                    continue;
                };
                if !crafts.contains(&craft) {
                    return Err(EquipmentConfigError::InvalidRoster {
                        reason: format!(
                            "item '{id}' gates its tier '{}' on '{craft}', which no material \
                             declares as a craft - the tier could never be reached",
                            tier.id
                        ),
                    });
                }
            }
            let Some(material) = item.bounds_material() else {
                continue;
            };
            if materials.material(material).is_none() {
                return Err(EquipmentConfigError::InvalidRoster {
                    reason: format!(
                        "item '{id}' bounds '{material}', which is not a material - it would be the \
                         bench tool for nothing"
                    ),
                });
            }
        }
        Ok(())
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
                for effect in def.every_effect() {
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

    /// **THE HUNT JOB'S DEFAULT KIT MUST CARRY NO MASS-BOUNDED ATTACK**, because it is the answer
    /// this file gives wherever there is **no quarry to test a bound against**.
    ///
    /// Since the herd table resolves its own default per quarry
    /// ([`crate::fauna::quarry_default_hunt_kit`]), `default_kits.hunt` is no longer what prices the
    /// estimate tables — it is the **fallback**: a band's own `hunterAttack` row and
    /// [`crate::fauna::HuntingParty::builtin_equipped`] resolve it through
    /// [`Self::hunter_profile_unbounded`], and a herd whose species the roster cannot resolve falls
    /// back to it too. Every one of those surfaces has nothing to ask about the bound, so a bounded
    /// weapon here would be counted **everywhere** — the twin of
    /// [`Self::validate_warrior_kits_have_no_quarry`], and `config-loading.md`'s "looks live but
    /// isn't" in its most reassuring direction.
    ///
    /// **A bounded kit may still be a QUARRY's default** — that resolution passes the animal's own
    /// `body_mass` through [`Self::hunter_profile_against`], which is what makes the trapping kit
    /// legal there and illegal here.
    fn validate_default_hunt_kit_is_quarry_blind(&self) -> Result<(), EquipmentConfigError> {
        let default = self.default_kit(KitJob::Hunt);
        for item in default.uses() {
            let Some(def) = self.item(item) else { continue };
            for effect in def.every_effect() {
                if effect.stat == EquipmentStat::Attack && effect.is_mass_bounded() {
                    return Err(EquipmentConfigError::InvalidRoster {
                        reason: format!(
                            "the hunt job's default kit '{}' uses '{item}', whose attack is bounded by body mass — the job default is what every surface WITH NO QUARRY resolves, so the bound would be silently ignored there",
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

/// **What one kit grants a party at one state of wear** — the output of
/// [`EquipmentConfig::resolve_kit_tiers`], and the shape both the world-level kit roster and the
/// per-band kit readout are built from.
///
/// A plain value with no config in it: everything here has already been resolved, so a consumer
/// renders these numbers rather than re-deriving them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedKitTiers {
    /// A hunter's combat `attack` under this kit — what the gate `max(0, attack − defense)` compares
    /// against a herd's `defense`. **Unbounded**: see [`Self::attack_min_body_mass`].
    pub attack: f32,
    /// Per-hunter HUNT haul rate (biomass/turn) — the sled's tier.
    pub hunt_carry_per_worker_biomass: f32,
    /// Per-gatherer throughput (biomass/turn, **before** the tile's seasonal weight) — the baskets'.
    pub forage_carry_per_worker_biomass: f32,
    /// Per-keeper PEN collection rate (biomass/turn) — the handling gear's, and **not**
    /// [`Self::hunt_carry_per_worker_biomass`]. A sled drags a carcass in off the range and a pen
    /// stands at the camp, so a kit with a sled and no handling gear reads the bare rate here.
    pub pen_carry_per_worker_biomass: f32,
    /// The sight range each posted scout vantage reveals at — the wayfinding gear's. A distance in
    /// tiles, carried as `f32` because the effects axis is continuous; the reveal path rounds.
    pub scout_vantage_range: f32,
    /// **The range of quarry [`Self::attack`] applies to**, by body mass.
    /// [`UNBOUNDED_BODY_MASS`] on either end means no bound there. Outside the range the kit grants
    /// no attack at all and the party falls back to the bare hand's.
    pub attack_min_body_mass: f32,
    /// See [`Self::attack_min_body_mass`].
    pub attack_max_body_mass: f32,
    /// What this kit multiplies the quarry's own `wariness` by at the retreat. `1.0` is neutral.
    pub dispersion: f32,
    /// What this kit multiplies the hunt's baseline injury hazard by. `1.0` is neutral.
    pub exposure: f32,
    /// What this kit multiplies a rung's build accrual by. `1.0` is neutral — the reading of every
    /// kit but `husbandry` on the shipped roster.
    pub build_rate: f32,
}

/// **"This end of the attack's mass window is not bounded"** — `0`, which is both the FlatBuffers
/// default and what every weapon but the passive device ships. Named because a `0` beside a body
/// mass otherwise reads as *"applies to animals of zero mass"*, which is the opposite of unbounded.
pub const UNBOUNDED_BODY_MASS: f32 = 0.0;

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

    /// The **equipped** value an item's DEFAULT TIER declares for a stat — panics if it declares
    /// the other side, so a test cannot silently assert against the wrong side of a cliff.
    fn equipped_of(config: &EquipmentConfig, id: &str, stat: EquipmentStat) -> f32 {
        let tier = item(config, id).default_tier();
        match tier.effects.iter().find(|e| e.stat == stat).map(|e| e.tier) {
            Some(EffectTier::Equipped(value)) => value,
            other => panic!("'{id}' declares {other:?} for {stat:?}, not an equipped tier"),
        }
    }

    /// The **unequipped** value an item declares for a stat, on its SHARED effects. Same strictness,
    /// other side — and the other home, because an unequipped value is true of every tier at once.
    #[allow(dead_code)] // the shipped roster's only unequipped sides are read by the pen/vantage tests
    fn unequipped_of(config: &EquipmentConfig, id: &str, stat: EquipmentStat) -> f32 {
        match item(config, id).shared_effect(stat) {
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

    /// A band that owns one unworn unit of everything a kit carries — the reference ledger, and
    /// **not** `Default`, which since the count slice means *owns nothing*.
    fn fresh() -> BandEquipment {
        BandEquipment::start_stocked(&EquipmentConfig::builtin())
    }

    /// A band owning exactly `count` units of each named item, unworn, at the default tier.
    fn owning(config: &EquipmentConfig, stock: &[(&str, u32)]) -> BandEquipment {
        let mut wear = BandEquipment::default();
        for (item, count) in stock {
            let tier = config
                .item(item)
                .expect("test names a real item")
                .default_tier()
                .id
                .clone();
            wear.stock(item, *count, &tier, None);
        }
        wear
    }

    /// **The partition is by ITEM SET, and the shortfall is PER ITEM.**
    ///
    /// Ten hunters, five spears, ten sleds: five people hold both and five hold only the sled. The
    /// two shortfalls are independent, which is the whole reason coverage is resolved per item
    /// rather than as one "how equipped is this party" scalar.
    #[test]
    fn gear_partitions_a_party_into_crews_by_what_each_run_holds() {
        let config = EquipmentConfig::builtin();
        let kit = kit_of(&[SPEARS, SLED]);
        let wear = owning(&config, &[(SPEARS, 5), (SLED, 10)]);

        let coverage = config.coverage(&kit, 10.0, &wear);
        let crews = coverage.crews();

        assert_eq!(crews.len(), 2, "two distinct loadouts, so two crews");
        assert_eq!(crews[0].workers, 5.0);
        assert_eq!(
            crews[0].kit.uses().collect::<Vec<_>>(),
            vec![SPEARS, SLED],
            "the best-equipped crew comes first and holds everything"
        );
        assert_eq!(crews[1].workers, 5.0);
        assert_eq!(
            crews[1].kit.uses().collect::<Vec<_>>(),
            vec![SLED],
            "the shortfall is in spears alone - the sleds reach everybody"
        );
        assert_eq!(coverage.workers_holding(SPEARS), 5.0);
        assert_eq!(coverage.workers_holding(SLED), 10.0);
        assert!(!coverage.is_uniform());
    }

    /// **A fully-equipped party and a fully-bare one are BOTH one crew**, and a caller must never
    /// have to tell "no crews" from "one crew holding nothing".
    ///
    /// The bare arm is the one that matters: it is what a band whose spears have all broken sends
    /// out, and it has to be a crew with people in it or the hunt resolves against an empty force.
    #[test]
    fn a_uniform_party_is_one_crew_whether_it_is_armed_or_bare() {
        let config = EquipmentConfig::builtin();
        let kit = kit_of(&[SPEARS]);

        let armed = config.coverage(&kit, 8.0, &owning(&config, &[(SPEARS, 8)]));
        assert!(armed.is_uniform());
        assert_eq!(armed.crews()[0].workers, 8.0);
        assert_eq!(
            armed.crews()[0].kit.uses().collect::<Vec<_>>(),
            vec![SPEARS]
        );

        let bare = config.coverage(&kit, 8.0, &owning(&config, &[]));
        assert!(bare.is_uniform());
        assert_eq!(bare.crews()[0].workers, 8.0, "the unarmed still go");
        assert!(
            bare.crews()[0].kit.uses().next().is_none(),
            "holding nothing, but holding it as a crew"
        );
        assert_eq!(
            bare.crews()[0].kit.id(),
            kit.id(),
            "still the kit they chose"
        );
    }

    /// **Surplus arms nobody extra.** Twenty spears and eight hunters is eight armed hunters and
    /// twelve spears in reserve — the reserve is what the *next* break spends, not more attack now.
    #[test]
    fn surplus_units_are_reserve_rather_than_extra_coverage() {
        let config = EquipmentConfig::builtin();
        let kit = kit_of(&[SPEARS]);
        let coverage = config.coverage(&kit, 8.0, &owning(&config, &[(SPEARS, 20)]));

        assert!(coverage.is_uniform());
        assert_eq!(coverage.workers_holding(SPEARS), 8.0);
    }

    /// **A spent unit arms nobody, so the cliff arrives ONE PERSON AT A TIME.**
    ///
    /// This is the graded disarmament falling out of the counts rather than being built: the band
    /// below owns three spears and has worn one out, so the next hunt goes out two armed and one
    /// bare-handed instead of flipping the whole party at once.
    #[test]
    fn a_worn_out_unit_stops_covering_anyone() {
        let config = EquipmentConfig::builtin();
        let kit = kit_of(&[SPEARS]);
        let mut wear = owning(&config, &[(SPEARS, 3)]);
        let spears = config.item(SPEARS).expect("spears ship");

        // Exactly one whole unit's worth of USES — the charge is `uses × wear.amount`, so a unit's
        // life is its durability measured in the item's own quantum, not in condition.
        wear.wear_item(
            &config,
            SPEARS,
            WearQuantum::Strike,
            spears.default_tier().starting_durability / spears.headline_wear().amount,
        );

        assert_eq!(wear.live_units(SPEARS, &config), 2, "one unit retired");
        let coverage = config.coverage(&kit, 3.0, &wear);
        assert_eq!(coverage.crews().len(), 2);
        assert_eq!(coverage.workers_holding(SPEARS), 2.0);
        assert_eq!(
            coverage.crews()[1].workers,
            1.0,
            "one hunter dropped to bare hands, not the whole party"
        );
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

    /// The shipped **no-equipment** haul baseline — `labor_config.json`'s
    /// `hunt.per_worker_biomass_capacity`, named here so this module's unit tests do not restate a
    /// number that lives elsewhere.
    fn baseline_haul_rate() -> f32 {
        crate::labor_config::LaborConfig::builtin()
            .hunt
            .per_worker_biomass_capacity
    }

    /// The shipped **no-equipment** gather baseline — `labor_config.json`'s
    /// `forage.per_worker_biomass_capacity`, for the same reason.
    fn baseline_gather_rate() -> f32 {
        crate::labor_config::LaborConfig::builtin()
            .forage
            .per_worker_biomass_capacity
    }

    #[test]
    fn builtin_config_ships_all_three_kits() {
        let config = EquipmentConfig::builtin();
        assert_eq!(equipped_of(&config, SPEARS, EquipmentStat::Attack), 20.0);
        assert!(item(&config, SPEARS).default_tier().starting_durability > 0.0);
        assert!(item(&config, SPEARS).headline_wear().amount > 0.0);
        assert!(equipped_of(&config, SLED, EquipmentStat::HuntCarry) > 0.0);
        assert!(item(&config, SLED).headline_wear().amount > 0.0);
        assert!(equipped_of(&config, BASKETS, EquipmentStat::ForageCarry) > 0.0);
        assert!(item(&config, BASKETS).headline_wear().amount > 0.0);
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
            equipped_of(&equipment, SLED, EquipmentStat::HuntCarry) > baseline_haul_rate(),
            "the sled must raise the hunt's haul rate above the bare-armed tier"
        );
        assert!(
            equipped_of(&equipment, BASKETS, EquipmentStat::ForageCarry) > baseline_gather_rate(),
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
            equipped_of(&equipment, SLED, EquipmentStat::HuntCarry) / baseline_haul_rate();
        let basket_ratio =
            equipped_of(&equipment, BASKETS, EquipmentStat::ForageCarry) / baseline_gather_rate();
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
        let hunt = baseline_haul_rate();
        let gather = baseline_gather_rate();
        assert_eq!(
            equipment.hunt_per_worker_biomass_capacity(hunt, &kit_of(&[SLED]), &fresh()),
            equipped_of(&equipment, SLED, EquipmentStat::HuntCarry)
        );
        assert_eq!(
            equipment.hunt_per_worker_biomass_capacity(hunt, &kit_of(&[]), &fresh()),
            hunt
        );
        assert_eq!(
            equipment.forage_per_worker_biomass_capacity(gather, &kit_of(&[BASKETS]), &fresh()),
            equipped_of(&equipment, BASKETS, EquipmentStat::ForageCarry)
        );
        assert_eq!(
            equipment.forage_per_worker_biomass_capacity(gather, &kit_of(&[]), &fresh()),
            gather
        );
    }

    /// **ONE KIT, ONE JOB, at the resolver seam** (§4.8) — the cross-check that would have caught the
    /// original defect, where the "carry kit" called baskets raised the *hunt's* haul and foraging
    /// got nothing. A dry basket must leave the hunt's tier untouched and a dry sled must leave the
    /// gather's untouched, so each resolver is swept across the other kit's state.
    #[test]
    fn a_dry_basket_does_not_touch_the_hunt_and_a_dry_sled_does_not_touch_the_gather() {
        let equipment = EquipmentConfig::builtin();
        let hunt = baseline_haul_rate();
        let gather = baseline_gather_rate();
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
            equipped_of(&equipment, SLED, EquipmentStat::HuntCarry),
            "an equipped sled hauls at the shipped rate whatever the baskets are doing"
        );
        assert_eq!(
            equipment.forage_per_worker_biomass_capacity(gather, &kit_of(&[BASKETS]), &fresh()),
            equipped_of(&equipment, BASKETS, EquipmentStat::ForageCarry),
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
            matches!(&err, EquipmentConfigError::Invalid { field, .. } if field == "items.spears.wear[0].amount"),
            "unexpected error: {err}"
        );
    }

    /// **AN ITEM MAY WEAR ON SEVERAL QUANTA, and each is charged over its own number** (issue #515).
    /// The handling gear is the case: it is worked on the beast at a slaughter and on the animals
    /// being gentled during a `Tame`, and with one slot the second was unbillable.
    #[test]
    fn an_item_may_wear_on_more_than_one_quantum() {
        let config = EquipmentConfig::builtin();
        let gear = item(&config, "husbandry_gear");
        assert!(
            gear.wears_on(WearQuantum::BiomassCollected)
                && gear.wears_on(WearQuantum::BuildProgress),
            "the shipped handling gear must be charged for both the jobs it does"
        );
        // **The two rates are independent**, which is what lets either life be retuned without
        // moving the other — the same reason the pen's two quanta were split in the first place.
        assert!(
            gear.wear_for(WearQuantum::BiomassCollected)
                .expect("declared above")
                .amount
                > 0.0
                && gear
                    .wear_for(WearQuantum::BuildProgress)
                    .expect("declared above")
                    .amount
                    > 0.0,
            "every declared quantum must cost something, or it is a dial that bills nothing"
        );
        // **A quantum the item does NOT declare answers `None`, and that is the cross-charging
        // guarantee**: a hunt landing blows finds no entry here, so a slaughter cannot blunt a spear.
        assert!(
            gear.wear_for(WearQuantum::Strike).is_none(),
            "an item must not be billable on a quantum it does not declare"
        );
    }

    /// **A QUANTUM DECLARED TWICE IS DEAD CONFIG** — `wear_for` answers the first match, so the
    /// second entry's amount is a dial a reader would reasonably believe is in play. `tiers`' own
    /// unique-id rule, one field over.
    #[test]
    fn validate_rejects_an_item_wearing_on_the_same_quantum_twice() {
        let mut json: serde_json::Value =
            serde_json::from_str(BUILTIN_EQUIPMENT_CONFIG).expect("the builtin parses");
        json["items"]["husbandry_gear"]["wear"] = serde_json::json!([
            { "per": "biomass_collected", "amount": 0.04 },
            { "per": "biomass_collected", "amount": 9.0 },
        ]);
        let err = EquipmentConfig::from_json_str(&json.to_string())
            .expect_err("a duplicate quantum is invalid");
        assert!(
            matches!(&err, EquipmentConfigError::Invalid { field, .. }
                if field == "items.husbandry_gear.wear[1].per"),
            "unexpected error: {err}"
        );
    }

    /// **AN ITEM NOTHING WEARS IS NOT CONSUMABLE**, so its role never steps down and the whole
    /// durability cliff is absent for it. The list made the empty case newly representable, so it is
    /// newly rejectable.
    #[test]
    fn validate_rejects_an_item_with_no_wear_entry_at_all() {
        let mut json: serde_json::Value =
            serde_json::from_str(BUILTIN_EQUIPMENT_CONFIG).expect("the builtin parses");
        json["items"]["spears"]["wear"] = serde_json::json!([]);
        let err = EquipmentConfig::from_json_str(&json.to_string())
            .expect_err("an item with no wear entry is invalid");
        assert!(
            matches!(&err, EquipmentConfigError::Invalid { field, .. }
                if field == "items.spears.wear"),
            "unexpected error: {err}"
        );
    }

    /// **THE BUILD AXIS IS A MULTIPLIER RESOLVED LIKE THE OTHER TWO** — the max of what a kit's live
    /// items declare, neutral when nothing does. Pinned here rather than only through a turn,
    /// because *"the husbandry kit builds faster"* also passes for a resolver that answers the
    /// husbandry kit's number for every kit.
    #[test]
    fn the_build_rate_is_neutral_unless_a_live_item_declares_it() {
        let config = EquipmentConfig::builtin();
        let wear = crate::components::BandEquipment::start_stocked(&config);
        let husbandry = config
            .kit("husbandry")
            .expect("the shipped roster carries the husbandry kit");
        let big_game = config
            .kit("big_game")
            .expect("the shipped roster carries the stalking kit");
        assert!(
            config.build_rate(&husbandry, &wear) > 1.0,
            "the handling kit must declare a build rate above neutral"
        );
        assert_eq!(
            config.build_rate(&big_game, &wear),
            1.0,
            "a kit carrying nothing that helps a build must be exactly neutral"
        );
        // **A SPENT ITEM STOPS CONTRIBUTING IT**, the rule every other axis follows: the cliff is
        // flat-then-step-down, and for a multiplier the step down IS the neutral.
        let mut dry = wear.clone();
        dry.wear_item(
            &config,
            "husbandry_gear",
            WearQuantum::BuildProgress,
            f32::MAX / 2.0,
        );
        assert_eq!(
            config.build_rate(&husbandry, &dry),
            1.0,
            "handling gear that has run dry must build no faster than bare hands"
        );
    }

    #[test]
    fn validate_rejects_a_kit_born_dry() {
        let err = EquipmentConfig::from_json_str(&kit_json("2.0", "0.02", "0.0"))
            .expect_err("zero durability is invalid");
        assert!(
            matches!(&err, EquipmentConfigError::Invalid { field, .. } if field == "items.baskets.tiers.flint.starting_durability"),
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
                "spears": {{ "wear": [{{ "per": "strike", "amount": 0.4 }}], "effects": [], "tiers": [{{"id": "flint", "starting_durability": 100.0, "effects": [{{ "stat": "attack", "equipped": 20.0 }}]}}] }},
                "sled": {{ "wear": [{{ "per": "biomass_hauled", "amount": 0.02 }}], "effects": [{{ "stat": "hunt_carry", "unequipped": 12.0 }}], "tiers": [{{"id": "flint", "starting_durability": 100.0, "effects": []}}] }}
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
                "spears": {{ "wear": [{{ "per": "strike", "amount": {spear_wear} }}], "tiers": [{{ "id": "flint", "starting_durability": 100.0, "effects": [{{ "stat": "attack", "equipped": 20.0 }}] }}] }},
                "sled": {{ "wear": [{{ "per": "biomass_hauled", "amount": {sled_wear} }}], "effects": [{{ "stat": "hunt_carry", "unequipped": 12.0 }}], "tiers": [{{ "id": "flint", "starting_durability": 100.0, "effects": [] }}] }},
                "baskets": {{ "wear": [{{ "per": "biomass_gathered", "amount": 0.04 }}], "effects": [{{ "stat": "forage_carry", "unequipped": 1.6 }}], "tiers": [{{"id": "flint", "starting_durability": {basket_durability}, "effects": []}}] }}
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
        let fresh = fresh();
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
            equipped_of(&equipment, SLED, EquipmentStat::HuntCarry),
            "the stalking kit hauls at the sled's own tier"
        );
        assert_eq!(
            equipment.forage_per_worker_biomass_capacity(
                labor.forage.per_worker_biomass_capacity,
                &gathering,
                &fresh
            ),
            equipped_of(&equipment, BASKETS, EquipmentStat::ForageCarry),
            "the gathering kit carries at the baskets' own tier"
        );
    }

    /// **A kit that uses nothing reads false everywhere, however fresh the band's gear is** — and
    /// the tiers it resolves to are the three unequipped ones. `none` is an ordinary roster member,
    /// so this is a statement about an empty `uses` list rather than about a sentinel id.
    #[test]
    fn a_kit_that_uses_nothing_runs_at_every_unequipped_tier() {
        let equipment = EquipmentConfig::builtin();
        let fresh = fresh();
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
            labor.hunt.per_worker_biomass_capacity,
            "a party with no sled drags at the no-equipment baseline"
        );
        assert_eq!(
            equipment.forage_per_worker_biomass_capacity(
                labor.forage.per_worker_biomass_capacity,
                &none,
                &fresh
            ),
            labor.forage.per_worker_biomass_capacity,
            "a party with no baskets gathers at the no-equipment baseline"
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
                "default_kits": { "hunt": "big_game", "forage": "big_game", "scout": "big_game", "warrior": "big_game" },
                "quarry_default_kit_margin": 0.25,
                "start_stock_fraction": 1.5,
            "life_readout": { "warn_fraction": 0.34, "danger_fraction": 0.10 }"#,
            ),
            (
                "a kit that can be sent on nothing",
                r#""kits": [
                    { "id": "big_game", "display_name": "A", "jobs": [], "uses": [] }
                ],
                "default_kits": { "hunt": "big_game", "forage": "big_game", "scout": "big_game", "warrior": "big_game" },
                "quarry_default_kit_margin": 0.25,
                "start_stock_fraction": 1.5,
            "life_readout": { "warn_fraction": 0.34, "danger_fraction": 0.10 }"#,
            ),
            (
                "a default naming no roster entry",
                r#""kits": [
                    { "id": "big_game", "display_name": "A", "jobs": ["hunt", "forage"], "uses": [] }
                ],
                "default_kits": { "hunt": "ghost", "forage": "big_game", "scout": "big_game", "warrior": "big_game" },
                "quarry_default_kit_margin": 0.25,
                "start_stock_fraction": 1.5,
            "life_readout": { "warn_fraction": 0.34, "danger_fraction": 0.10 }"#,
            ),
            (
                "a default whose jobs do not cover its own job",
                r#""kits": [
                    { "id": "big_game", "display_name": "A", "jobs": ["hunt"], "uses": [] },
                    { "id": "gathering", "display_name": "B", "jobs": ["forage"], "uses": [] }
                ],
                "default_kits": { "hunt": "gathering", "forage": "gathering", "scout": "gathering", "warrior": "gathering" },
                "quarry_default_kit_margin": 0.25,
                "start_stock_fraction": 1.5,
            "life_readout": { "warn_fraction": 0.34, "danger_fraction": 0.10 }"#,
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
            "default_kits": { "hunt": "big_game", "forage": "big_game", "scout": "big_game", "warrior": "big_game" },
                "quarry_default_kit_margin": 0.25,
                "start_stock_fraction": 1.5,
            "life_readout": { "warn_fraction": 0.34, "danger_fraction": 0.10 }"#,
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
                "spears": {{ "wear": [{{ "per": "strike", "amount": 0.4 }}], "effects": [], "tiers": [{{"id": "flint", "starting_durability": 100.0, "effects": [{{ "stat": "attack", "equipped": 20.0 }}]}}] }},
                "sled": {{ "wear": [{{ "per": "biomass_hauled", "amount": 0.02 }}], "effects": [{{ "stat": "hunt_carry", "unequipped": 12.0 }}], "tiers": [{{"id": "flint", "starting_durability": 100.0, "effects": []}}] }},
                "baskets": {{ "wear": [{{ "per": "biomass_gathered", "amount": 0.04 }}], "effects": [{{ "stat": "forage_carry", "unequipped": 1.6 }}], "tiers": [{{"id": "flint", "starting_durability": 100.0, "effects": []}}] }}
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
                "spears": { "wear": [{ "per": "strike", "amount": 0.4 }], "effects": [], "tiers": [{"id": "flint", "starting_durability": 100.0, "effects": [{ "stat": "attack", "equipped": 20.0 }]}] },
                "snares": { "wear": [{ "per": "strike", "amount": 0.2 }], "effects": [], "tiers": [{"id": "flint", "starting_durability": 100.0, "effects": [{ "stat": "attack", "equipped": 20.0, "max_body_mass": 1.0 }]}] }
            },
            "kits": [
                { "id": "big_game", "display_name": "A", "jobs": ["hunt", "forage"], "uses": ["spears"] },
                { "id": "warrior", "display_name": "W", "jobs": ["warrior", "scout"], "uses": ["snares"] }
            ],
            "default_kits": { "hunt": "big_game", "forage": "big_game", "scout": "warrior", "warrior": "warrior" },
            "quarry_default_kit_margin": 0.25,
                "start_stock_fraction": 1.5,
            "life_readout": { "warn_fraction": 0.34, "danger_fraction": 0.10 }
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
            "default_kits": { "hunt": "big_game", "forage": "gathering", "scout": "none", "warrior": "none" },
            "quarry_default_kit_margin": 0.25,
                "start_stock_fraction": 1.5,
            "life_readout": { "warn_fraction": 0.34, "danger_fraction": 0.10 }"#;
}
