//! Data-driven fauna species table + spawn abundance.
//!
//! Loaded from `data/fauna_config.json`. Turns the former hard-coded `HerdSpecies`
//! enum into a table: each species carries a display name, size class, migratory
//! flag, roaming range (route length), group biomass, and the food-module "biomes"
//! it hosts in. `abundance` drives how densely short-range game spawns per biome.
//! Mirrors the `visibility_config.rs` loader pattern (baked-in builtin + optional
//! file/env override).

use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use rand::{rngs::SmallRng, Rng};
use serde::Deserialize;
use sim_runtime::TerrainType;

use thiserror::Error;

use crate::combat::CombatStats;
use crate::config_load::{load_config_from_env, ConfigLoadError};

pub const BUILTIN_FAUNA_CONFIG: &str = include_str!("data/fauna_config.json");

/// **What an animal eats** — the trophic knob (`docs/plan_predators.md`). A `Herbivore` grazes the
/// land (the graze layer); a `Carnivore` eats prey biomass. The **only** knob that changes the
/// food/carrying-capacity layer — and it does so in **Phase 1**, not here. `#[serde(default)]` =
/// `Herbivore`, persisted-enum convention (`as_str` / `from_key`), so every existing species is
/// byte-identical and this field is **inert this phase**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Diet {
    /// Grazes the land. The default.
    #[default]
    Herbivore,
    /// Eats prey biomass — a predator.
    Carnivore,
}

impl Diet {
    /// Stable string key (the JSON spelling and any future snapshot field).
    pub fn as_str(&self) -> &'static str {
        match self {
            Diet::Herbivore => "herbivore",
            Diet::Carnivore => "carnivore",
        }
    }

    /// Parse the stable key back (inverse of [`Diet::as_str`]); unknown/empty → [`Diet::Herbivore`].
    pub fn from_key(key: &str) -> Self {
        match key {
            "carnivore" => Diet::Carnivore,
            _ => Diet::Herbivore,
        }
    }
}

/// Coarse size band. Drives roaming range + group size; also lets Phase B/C offer
/// the right verbs (big/small game are huntable one-shot; migratory herds follow).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SizeClass {
    #[default]
    Small,
    Big,
    Migratory,
}

impl SizeClass {
    /// Stable string key (also the snapshot `size_class` field).
    pub fn as_str(&self) -> &'static str {
        match self {
            SizeClass::Small => "small",
            SizeClass::Big => "big",
            SizeClass::Migratory => "migratory",
        }
    }

    /// Parse the stable string key back into a size class (inverse of `as_str`; the rollback
    /// restore path). Unknown/empty strings resolve to the `Default` (`Small`).
    pub fn from_key(key: &str) -> Self {
        match key {
            "big" => SizeClass::Big,
            "migratory" => SizeClass::Migratory,
            _ => SizeClass::Small,
        }
    }
}

/// **How far up the husbandry ladder a species can climb** (Grazing 2d-δ, `docs/plan_grazing_2d.md`
/// §4a). The ladder is a *sequence* (wild → pastoral → pen), so a species' reach is a single ceiling,
/// not two independent flags — which makes the incoherent "pennable but not tameable" state
/// unrepresentable (no `validate()` combination guard needed). `Wild` is hunt-only (domestication never
/// accrues, `tame`/`corral`/`extend_pen` reject); `Pastoral` tames + roams but never pens
/// (`corral`/`extend_pen` reject); `Pen` is the full ladder. **Default `Pen`** preserves the pre-δ
/// universal-full-ladder behaviour for any untagged/future species.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HusbandryCeiling {
    /// Hunt-only. Domestication never accrues.
    Wild,
    /// Reaches the mobile-tamed rung but never the pen.
    Pastoral,
    /// The full ladder — the default.
    #[default]
    Pen,
}

impl HusbandryCeiling {
    /// Stable string key (also the snapshot `husbandry_ceiling` field / the wire `husbandryCeiling`).
    pub fn as_str(&self) -> &'static str {
        match self {
            HusbandryCeiling::Wild => "wild",
            HusbandryCeiling::Pastoral => "pastoral",
            HusbandryCeiling::Pen => "pen",
        }
    }

    /// Parse the stable string key back (inverse of `as_str`; the rollback restore path). Unknown/empty
    /// strings resolve to the `Default` (`Pen`), preserving the full ladder.
    pub fn from_key(key: &str) -> Self {
        match key {
            "wild" => HusbandryCeiling::Wild,
            "pastoral" => HusbandryCeiling::Pastoral,
            _ => HusbandryCeiling::Pen,
        }
    }

    /// Can this species be **tamed** (mobile domestication)? True for `Pastoral` and `Pen`.
    pub fn allows_domestication(&self) -> bool {
        !matches!(self, HusbandryCeiling::Wild)
    }

    /// Can this species be **penned** (corralled)? True only for `Pen`.
    pub fn allows_pen(&self) -> bool {
        matches!(self, HusbandryCeiling::Pen)
    }
}

/// **What kind of open water a species' spawn site must border** — the shore predicate's four states.
///
/// `None` is "no rule at all" (the default, so every species that omits it is byte-identical);
/// `Any` is the historical "any `WATER` tag will do". The salt/fresh split exists because the two
/// species carrying a shore rule want opposite things: a seal is a **marine** forager, so a
/// landlocked freshwater lake (or a navigable river) is not a coast it can haul out on, while the
/// Silt Catfish is freshwater and is happy beside anything wet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ShoreRequirement {
    /// No site rule — the species may spawn anywhere its `host_biomes` admit. The default.
    #[default]
    None,
    /// Any open water on one of the six hex sides (`WATER`), fresh or salt.
    Any,
    /// **Salt water only** — `WATER` without `FRESHWATER`, the ocean (the same rule `hydrology`'s
    /// `TileWorld::is_ocean` states, in the same tag vocabulary).
    Salt,
    /// **Fresh water only** — `WATER` *with* `FRESHWATER`: lakes, inland seas, navigable rivers.
    Fresh,
}

impl ShoreRequirement {
    /// Stable string key, matching the JSON spelling (also the `validate` rejection's `value`).
    pub fn as_str(&self) -> &'static str {
        match self {
            ShoreRequirement::None => "none",
            ShoreRequirement::Any => "any",
            ShoreRequirement::Salt => "salt",
            ShoreRequirement::Fresh => "fresh",
        }
    }

    /// Does this rule ask anything of the site at all?
    pub fn is_required(&self) -> bool {
        !matches!(self, ShoreRequirement::None)
    }

    /// Is this requirement satisfied by a site that borders `has_salt` / `has_fresh` water?
    pub fn satisfied_by(&self, has_salt: bool, has_fresh: bool) -> bool {
        match self {
            ShoreRequirement::None => true,
            ShoreRequirement::Any => has_salt || has_fresh,
            ShoreRequirement::Salt => has_salt,
            ShoreRequirement::Fresh => has_fresh,
        }
    }
}

/// One species row in the table.
#[derive(Debug, Clone, Deserialize)]
pub struct SpeciesDef {
    /// Player-facing name; also the snapshot `species` string. Must embed the
    /// client icon keyword (e.g. "deer", "boar") so `FoodIcons.for_herd` resolves.
    pub display_name: String,
    #[serde(default)]
    pub size_class: SizeClass,
    #[serde(default)]
    pub migratory: bool,
    /// Inclusive `[min, max]` route length in tiles = roaming range.
    pub route_len: [u32; 2],
    /// Inclusive `[min, max]` group biomass.
    pub biomass: [f32; 2],
    /// **Biomass of ONE animal** — the quantum a hunt take is rounded down to (intensification
    /// ladder slice 8). A herd's animal count is `biomass / body_mass`, **derived, never stored**:
    /// biomass stays the authoritative stock and the count is a reading of it.
    ///
    /// **This is what makes a herd a herd and not a fluid.** Every hunt take is
    /// [`crate::fauna::quantise_animal_take`]: you kill `floor(escapement / body_mass)` whole
    /// animals, and a party that cannot carry a whole one still takes it and **wastes** the rest.
    /// Two consequences fall straight out of the ratio against the herd's MSY (`r × K / 4`):
    /// - **Rhythm** — `body_mass / MSY` turns per animal at the operating point. Small game
    ///   (fowl 1 / rabbit 2) is a near-continuous trickle; a mammoth is one kill every ~7 turns and
    ///   then you eat for a week. When the herd cannot yet spare a whole animal the hunt **pauses**
    ///   and the herd regrows — the discretised form of constant escapement.
    /// - **Party size = how much of the kill you keep** — `hunt.per_worker_biomass_capacity` (40)
    ///   against this: one hunter keeps 80% of a boar, 33% of a steppe runner, 5% of a mammoth.
    ///   ~20 hunters are needed to bring a whole mammoth home.
    ///
    /// **Playtest dials.** Validated finite & `> 0` — at `0` a herd would hold infinitely many
    /// animals and `floor(x / 0)` would take the whole stock in one turn.
    pub body_mass: f32,
    /// **How many of these one hunter can bring into contact per turn** — the engagement stage of
    /// `docs/plan_hunt_through_combat.md` §2, and a purely **spatial** constraint. Twenty hunters can
    /// surround one mammoth (`0.05`); one hunter can work a line of snares (`10`).
    ///
    /// **It says nothing about how fast they die.** That is the fight's business — durability against
    /// `hunters × max(0, attack − defense)`. Folding lethality in here would be a kill model living
    /// outside the resolver, which is the duplication that arc exists to delete, and it is what let a
    /// hand-authored "turns per kill" table look plausible during design.
    ///
    /// **It scales linearly with party size and is throughput, not a threshold.** Forty hunters
    /// engage two mammoths a turn; five still engage one (contact rounds up — a small band can walk
    /// up to a mammoth, it just cannot hurt it quickly) and grind it down over many turns. The gate
    /// is attack-vs-defense, never headcount.
    ///
    /// **Authored against `engage_rate × body_mass`** — the most biomass one hunter can ever take
    /// from this species per turn, at any weapon tier. That ceiling is what orders the roster: a
    /// mammoth's `40` is an outlier rather than the top of a smooth curve, the tameable species sit
    /// at 20–26.5 (you hunt them until you can tame them), pen small game is at the bottom, and
    /// dangerous-for-their-size (boar `4`, wolf `1.75`) are the worst deals in the game.
    ///
    /// **Playtest dials.** Validated finite & `> 0`.
    pub engage_rate: f32,
    /// Food-module keys (see `FoodModule::as_str`) this species hosts in.
    #[serde(default)]
    pub host_biomes: Vec<String>,
    /// **The shore predicate** — the kind of open water a spawn site must border on one of its six
    /// hex sides (`fauna`'s `adjacent_water_kinds`). The site rule a *marine forager* must
    /// satisfy: a seal colony hauls out on a shoreline, never on inland tundra.
    ///
    /// **The kind matters.** `Salt` is `WATER` without `FRESHWATER` — the ocean; `Fresh` is `WATER`
    /// *with* it — a lake, an inland sea, a navigable river. Seals are marine, so they ask for
    /// `salt`: a one-hex freshwater lake is not a coast. The Silt Catfish (`river_fish`) is
    /// freshwater game and asks for `any`, which is exactly the historical behaviour — it wants a
    /// shore, and does not care which.
    ///
    /// It is deliberately a **site** rule and nothing else. The *cold* half of "cold coast" comes
    /// from [`SpeciesDef::host_biomes`] (`boreal_arctic` = BorealTaiga/Tundra/PeriglacialSteppe/
    /// SeasonalSnowfield) — **not** from a second climate gate, because
    /// `climate::climate_band_for_temperature` is the single climate authority and a parallel one
    /// would drift from it. And it **reads** the coastline geometry rather than editing terrain, so
    /// worldgen stays the sole authority on where the water is.
    ///
    /// Defaults to [`ShoreRequirement::None`], so every species that omits it is byte-identical.
    /// **Any non-`None` value is rejected in combination with [`SpeciesDef::migratory`]** — see
    /// [`FaunaConfig::validate`].
    #[serde(default)]
    pub adjacent_water: ShoreRequirement,
    /// Turns the group grazes its current tile before stepping ≤1 hex (the graze-wander cadence,
    /// `advance_herds`). `~1` → effectively half speed, so an equal-speed party can catch it during
    /// a graze turn. Game rows use this; migratory rows use it for the pause between loiter wanders.
    #[serde(default = "default_dwell_turns")]
    pub dwell_turns: u32,
    /// Migratory only: inclusive `[min, max]` turns to loiter (graze-wander near an anchor) before
    /// committing to the next directed migration leg.
    #[serde(default = "default_loiter_turns")]
    pub loiter_turns: [u32; 2],
    /// Migratory only: hex radius of the local graze-wander around a loiter anchor.
    #[serde(default = "default_loiter_radius")]
    pub loiter_radius: u32,
    /// **Fodder one unit of animal biomass demands per turn** (Grazing Phase 2b-i). A herd of
    /// `biomass` draws `fodder_per_biomass × biomass` graze from the tiles in its range each turn
    /// (`fauna::advance_herd_grazing`), the metabolic denominator that turns the land's *grass flow*
    /// into *animals*. Smaller animals run hotter per unit mass, so small game carries the largest
    /// value and migratory megafauna the smallest. Cached onto `Herd` at spawn (mirroring
    /// `carrying_capacity`). Defaults to `0.0` (a non-grazing species) for a config that omits it —
    /// harmless while Phase 2b-i is inert on carrying capacity.
    #[serde(default)]
    pub fodder_per_biomass: f32,
    /// **Per-species logistic regrowth rate** for a *wild* herd (Grazing Phase 2b-ii). Replaces the
    /// single global `fauna.ecology.regrowth_rate` (0.05) that every animal used to breed at — the
    /// artifact that made "small game can't provision an expedition" (PR #117): a rabbit bred at a
    /// mammoth's rate. Fast small game breeds hot (~0.35), slow megafauna cold (~0.04). Cached onto
    /// `Herd` at spawn (mirroring `fodder_per_biomass` / `carrying_capacity`) and folded into the
    /// herd's *wild* ecology by [`crate::fauna::herd_ecology`]; the **pastoral/pen** rungs keep their
    /// own faster `r` (0.25 / 0.90), and the phase bands stay shared. `None` (omitted) falls back to
    /// `fauna.ecology.regrowth_rate`, so an older config stays non-breaking. Validated finite & `> 0`
    /// when present.
    #[serde(default)]
    pub regrowth_rate: Option<f32>,
    /// **HOW MUCH MORE WORK THIS SPECIES IS TO TAME, as a multiple of the `animal:pastoral` rung's
    /// declared `work_cost`.** The rung owns the *mechanic*; the species prices it — exactly the split
    /// [`SpeciesDef::regrowth_rate`] already uses against `pastoral_gain`/`pen_gain`. A single cost on
    /// the rung would make a rabbit and a Steppe Runner the same job; taming a small, quick,
    /// forgiving animal should be light work, and binding a large migratory herd should be
    /// generational. Roster: rabbit/fowl/crag_goat `1.0` (50 units) · boar `1.25` (62.5) · aurochs
    /// `2.0` (100) · steppe_runner/marsh_grazer `5.0` (250); a `wild`-ceiling species (deer, mammoth)
    /// never tames, so it carries none.
    ///
    /// **IT IS A COST, NOT A RATE, AND THE INVERSION IS THE HONEST STATEMENT**
    /// (`docs/plan_unit_costed_work.md` §3.1). It was `taming_rate` 0.2 on a Steppe Runner, which said
    /// *your people are five times worse at their job on this animal*; `taming_cost_multiplier` 5.0
    /// says *the animal is five times the work*, which is what anyone would have meant. Same pacing,
    /// truer sentence — and it composes with a cost spread, where a rate could not.
    ///
    /// **It scales the rung's DECAY as well as its cost**, because [`RungDef::build_decay`] reads
    /// `decay_fraction_per_turn` off the *scaled* cost — so the rung's build:decay ratio is invariant
    /// per species for free: **slow to tame, slow to forget**. Moot today (`animal:pastoral` declares
    /// no decay at all) but it is the rule that keeps a future decaying rung correct.
    ///
    /// Defaults to `1.0` (the rung's own price) when omitted, so an untagged or future species keeps
    /// today's behaviour. **Playtest dial.** Validated finite & `> 0` (at `0` the species would tame
    /// the instant any crew touched it; negative is meaningless).
    #[serde(default = "default_taming_cost_multiplier")]
    pub taming_cost_multiplier: f32,
    /// **How many ANIMALS one herder can mind** — the standing maintenance a managed (pastoral or
    /// penned) herd demands every turn: `herders_needed = ceil((biomass / body_mass) /
    /// animals_per_herder)` ([`crate::fauna::herders_needed`]). *Just because you aren't killing an
    /// animal doesn't mean you aren't tending them, making sure they don't run off, repairing fences.*
    /// Before this a pen of 2 and a pen of 200 needed the same single keeper; only the **feed** scaled.
    ///
    /// # Herding is HEADS, not tonnes — the denominator is load-bearing
    ///
    /// A shepherd minds ~300 sheep; a cowherd ~80 cattle. You watch **individuals** — chase strays,
    /// check each animal — and a heavier beast is not proportionally more work. An earlier cut of this
    /// dial was `biomass_per_herder` (one global "biomass one herder minds"), which is the same claim
    /// as *one herder per 100 fowl but one herder per 2 boar*. It also invented a **45-herder steppe
    /// megaherd** that was a pure artifact of the unit: 4,560 biomass of Steppe Runner is only **38
    /// animals**, i.e. ~3 herders. Per-species, per-**animal**, is the only unit that reads true.
    ///
    /// Per-species for the same reason [`SpeciesDef::body_mass`] / [`SpeciesDef::taming_cost_multiplier`] /
    /// [`SpeciesDef::husbandry_ceiling`] are: a herder minds far more birds than aurochs. Roster:
    /// fowl/rabbit 50, crag_goat 25, boar 15, steppe_runner/marsh_grazer 15, aurochs 12. Deer and
    /// mammoth omit it — a `wild` [`HusbandryCeiling`] is never herded at all.
    ///
    /// Resolved **live** by display name ([`FaunaConfig::animals_per_herder_for`]), never cached on the
    /// `Herd` — the `taming_cost_multiplier_for` path, so retuning reaches herds already on the map (and it needs no
    /// snapshot field). Defaults to [`DEFAULT_ANIMALS_PER_HERDER`] when omitted. **Playtest dial.**
    /// Validated finite & `> 0` (at `0` any herd would need infinitely many herders and could never be
    /// fully staffed).
    #[serde(default = "default_animals_per_herder")]
    pub animals_per_herder: f32,
    /// **How far up the husbandry ladder this species climbs** (Grazing 2d-δ) — `wild` | `pastoral` |
    /// `pen`. Cached onto `Herd` at spawn (mirroring `fodder_per_biomass` / `regrowth_rate`) and gates
    /// domestication accrual + the `tame` / `corral` / `extend_pen` paths. Defaults to `pen`
    /// (the full ladder) when omitted. See [`HusbandryCeiling`].
    #[serde(default)]
    pub husbandry_ceiling: HusbandryCeiling,
    /// **The K (carrying-capacity) multiplier at the mobile-tamed (pastoral) rung** — domestication
    /// makes the *land* hold more animals, non-linearly by species. Distinct from the global r-gains
    /// (`husbandry.pastoral_gain` / `pen_gain`), which scale a herd's *breeding rate*: this scales its
    /// *ceiling*. Without it a species on marginal range (a goat at `K≈24`) stays tiny even tamed while
    /// a fast wild breeder out-yields it, because taming touched only `r`. Folded into the herd's `K` at
    /// the one seam that writes it (`fauna::ecological_carrying_capacity`, via [`fauna::herd_density_gain`]),
    /// so a wild herd's `×1.0` leaves its `K` byte-identical. Resolved **live** by display name
    /// ([`FaunaConfig::pastoral_density_for`]), never cached on the `Herd` — the `taming_cost_multiplier_for` path, so a
    /// retune reaches herds already on the map. Defaults to [`DEFAULT_HUSBANDRY_DENSITY`] (1.0, neutral).
    /// **Playtest dial.** Validated finite & `>= 1.0` (a gain below 1 would make domestication *reduce*
    /// capacity).
    #[serde(default = "default_husbandry_density")]
    pub pastoral_density: f32,
    /// **The K (carrying-capacity) multiplier at the penned rung** — the top of the density ladder, big
    /// for the prime domesticates (goat/aurochs `5.0`). The pen twin of [`SpeciesDef::pastoral_density`];
    /// see it for the full rationale. Resolved live ([`FaunaConfig::pen_density_for`]), defaults to
    /// [`DEFAULT_HUSBANDRY_DENSITY`], validated finite & `>= 1.0`.
    #[serde(default = "default_husbandry_density")]
    pub pen_density: f32,
    /// **Plural form** of the species, lowercase, reading naturally mid-sentence ("the *deer* did
    /// not run"). Consumed by The Telling's fauna noun resolvers (`core_sim/src/telling/nouns.rs`).
    ///
    /// Optional, defaulting to `display_name` — deliberately **data, not a heuristic**: many of
    /// these names are already collective ("aurochs", "deer", "fowl") and a naive `+s` would
    /// produce "deers".
    #[serde(default)]
    pub plural: Option<String>,
    /// **Adjectival form** of the species, lowercase ("*deer* bones"). Optional, defaulting to
    /// `display_name`. Same rationale as [`SpeciesDef::plural`].
    #[serde(default)]
    pub adjective: Option<String>,
    /// **The species' intrinsic combat body** — the neutral [`CombatStats`] the combat subsystem
    /// reads (`docs/plan_predators.md`). The *same* `attack` predation will one day read for "who can
    /// it eat" and the hunt path reads for "how dangerous is this to hunt" — intrinsic combat stats
    /// and predation stats are one thing. `#[serde(default)]` = `{ attack: 0, defense: 1, range:
    /// Melee }`, so every species that omits it is a **harmless** hunt (attack 0 → zero casualties)
    /// and byte-identical. Validated: `attack >= 0` finite, `defense > 0` finite (it is a denominator
    /// in the kill/wound split).
    #[serde(default)]
    pub combat: CombatStats,
    /// **What this species eats** — herbivore (grazes) vs carnivore (eats prey). `#[serde(default)]` =
    /// `Herbivore`. Consumed in **Phase 1a**: `ecological_carrying_capacity` sums prey flow for
    /// carnivores, and `advance_predation` draws prey herds down.
    #[serde(default)]
    pub diet: Diet,
    /// **Prey biomass one unit of predator biomass demands per turn** (Predators Phase 1a,
    /// `docs/plan_predators.md`) — the carnivore analog of [`SpeciesDef::fodder_per_biomass`], and the
    /// denominator of a carnivore's prey-limited carrying capacity:
    /// `K_pred = Σ_prey prey_sustainable_flow / prey_per_biomass` over the prey herds inside the
    /// predator's prey-sensing disk (`fauna::ecological_carrying_capacity`). It is also the per-turn
    /// predation demand `prey_per_biomass × biomass` that `fauna::advance_predation` draws from those
    /// prey herds.
    ///
    /// `#[serde(default)]` = `0.0` for every herbivore (inert — an herbivore's `K` never reads it).
    /// **A carnivore requires it `> 0`** ([`FaunaConfig::validate`]): it is a denominator, so a `0`
    /// would make `K_pred` infinite.
    #[serde(default)]
    pub prey_per_biomass: f32,
    /// **Does it initiate?** `0..1` — the probability it raids unguarded foragers *unprovoked* (`> 0`),
    /// vs only ever reacting to being hunted (`0`). This is **behaviour**, orthogonal to strength
    /// ([`SpeciesDef::combat`]): a mammoth is immensely strong but `aggression 0` (it never comes for
    /// your camp), while a wolf is `aggression`-high. `#[serde(default)]` = `0.0`. **Inert this phase**
    /// — Phase 1's predator-raid trigger consumes it (camp-threat ≈ `attack × aggression`). Validated
    /// finite & in `[0, 1]`.
    #[serde(default)]
    pub aggression: f32,
    /// **Does it fight back when attacked?** `0..1` — the probability it turns and fights a hunting
    /// party rather than *fleeing*. The other half of the behaviour split (with [`SpeciesDef::aggression`]):
    /// `aggression` is "does it start it", `ferocity` is "does it finish it". **Danger is DERIVED, never
    /// stored** — hunt-danger ≈ `attack × ferocity` (a strong animal that flees costs you almost
    /// nothing), so the hunt-casualty adapters scale the animal's effective attack by this. A fleeing
    /// deer (`ferocity ~0.15`) barely scratches a party; a cornered boar (`0.6`) draws blood; a mammoth
    /// (`0.9`) is deadly. `#[serde(default)]` = `0.0` (flees — harmless to hunt). Validated finite & in
    /// `[0, 1]`.
    #[serde(default)]
    pub ferocity: f32,
    /// **A carnivore's target population as a fraction of its prey base** (Predators Phase 1a,
    /// `docs/plan_predators.md`) — *"wolves are 10% of their prey groups"*. The dedicated
    /// [`fauna::spawn_predators`] pass derives its per-species pack count from this instead of a fixed
    /// cap: `target = round(eligible_prey_herds × prey_ratio)`, where the prey herds are every
    /// herbivore herd this predator's `attack` clears (the map-wide count, no sensing-disk filter). A
    /// predator population is *defined by* its prey base, so the count is **derived, not an absolute** —
    /// and it lives on the predator's own row because each predator has its own prey set and its own
    /// ratio (a future big cat might be `0.05`).
    ///
    /// `#[serde(default)]` = `0.0` for every herbivore (inert — only [`fauna::spawn_predators`] reads
    /// it, and only for carnivores). **A carnivore requires it finite `> 0`** ([`FaunaConfig::validate`]):
    /// a `0` (or negative/non-finite) ratio would seat no packs at all, an incoherent predator.
    #[serde(default)]
    pub prey_ratio: f32,
    /// **What a hunt of this species PAYS, per unit of biomass taken** (`docs/plan_hunt_yield_model.md`
    /// §3) — the product half of *yield = product × intensity*. `#[serde(default)]` is both components
    /// `None`, i.e. *"the `hunt.*` globals"*, so every species that omits the block is **byte-identical**
    /// on food and trade. Resolved through the single seam [`FaunaConfig::hunt_yield_for`].
    ///
    /// Roster today: only `wolf` declares it (inedible, pelt-bearing). Validated per *present*
    /// component: finite & `>= 0.0` — **zero is legal**, it is the whole point (see [`HuntYieldDef`]).
    #[serde(default)]
    pub hunt_yield: HuntYieldDef,
}

/// Default graze pause: one turn of grazing between hex steps (≈ half movement speed).
fn default_dwell_turns() -> u32 {
    1
}

/// Default migratory loiter window (turns) at an anchor before the next migration leg.
fn default_loiter_turns() -> [u32; 2] {
    [12, 24]
}

/// **A species that costs exactly what the `animal:pastoral` rung declares** — the neutral
/// multiplier, so an untagged (or future) species behaves exactly as it did before the dial existed.
/// Also what an unresolvable species name reads as
/// (`FaunaConfig::taming_cost_multiplier_for`).
pub const DEFAULT_TAMING_COST_MULTIPLIER: f32 = 1.0;

fn default_taming_cost_multiplier() -> f32 {
    DEFAULT_TAMING_COST_MULTIPLIER
}

/// **Animals one herder minds for a species that does not declare a rate** — mid-roster (between the
/// aurochs' 12 and the fowl's 50), so an untagged or future species lands on a plausible crew size
/// rather than a free or an impossible one. Also what an unresolvable species name reads as
/// ([`FaunaConfig::animals_per_herder_for`]).
pub const DEFAULT_ANIMALS_PER_HERDER: f32 = 25.0;

fn default_animals_per_herder() -> f32 {
    DEFAULT_ANIMALS_PER_HERDER
}

/// **A species whose husbandry does not raise its carrying capacity** — the neutral density gain
/// ([`SpeciesDef::pastoral_density`] / [`SpeciesDef::pen_density`]), so an untagged (or wild) species'
/// `K` is unchanged (`×1.0`). Also what an unresolvable species name reads as
/// ([`FaunaConfig::pastoral_density_for`] / [`FaunaConfig::pen_density_for`]).
pub const DEFAULT_HUSBANDRY_DENSITY: f32 = 1.0;

fn default_husbandry_density() -> f32 {
    DEFAULT_HUSBANDRY_DENSITY
}

/// Default migratory loiter wander radius (hexes) around an anchor. Also the fallback grazing-range
/// radius for a migratory herd whose species row can't be resolved (`Herd::graze_range_radius`).
pub(crate) fn default_loiter_radius() -> u32 {
    2
}

impl SpeciesDef {
    /// The species' plural form, falling back to its display name when the table omits one.
    pub fn plural_or_name(&self) -> &str {
        self.plural.as_deref().unwrap_or(&self.display_name)
    }

    /// The species' adjectival form, falling back to its display name when the table omits one.
    pub fn adjective_or_name(&self) -> &str {
        self.adjective.as_deref().unwrap_or(&self.display_name)
    }

    /// Sample a route length within the configured inclusive range (>= 1).
    pub fn sample_route_len(&self, rng: &mut SmallRng) -> u32 {
        let lo = self.route_len[0].max(1);
        let hi = self.route_len[1].max(lo);
        rng.gen_range(lo..=hi)
    }

    /// Sample a migratory loiter window (turns) within the configured inclusive range (>= 1).
    pub fn sample_loiter_turns(&self, rng: &mut SmallRng) -> u32 {
        let lo = self.loiter_turns[0].max(1);
        let hi = self.loiter_turns[1].max(lo);
        rng.gen_range(lo..=hi)
    }

    /// Sample a group biomass within the configured inclusive range.
    pub fn sample_biomass(&self, rng: &mut SmallRng) -> f32 {
        let lo = self.biomass[0].max(0.0);
        let hi = self.biomass[1].max(lo);
        if hi <= lo {
            lo
        } else {
            rng.gen_range(lo..=hi)
        }
    }

    pub fn hosts_biome(&self, module_key: &str) -> bool {
        self.host_biomes.iter().any(|b| b == module_key)
    }

    /// Per-species carrying capacity biomass regrows toward (= the table max).
    pub fn carrying_capacity(&self) -> f32 {
        self.biomass[1].max(self.biomass[0]).max(0.0)
    }

    /// The **low end** of the species' `biomass` range — a pack's smallest viable size (Predators
    /// Phase 1a). The prey-gated spawn (`fauna::spawn_predator_group_at`) requires a tile's prey-derived
    /// `K` to reach at least this, so a pack only lands where the local prey base can sustain even its
    /// smallest form rather than being stillborn on prey-sparse ground.
    pub fn min_spawn_biomass(&self) -> f32 {
        self.biomass[0].min(self.biomass[1]).max(0.0)
    }

    /// The **wild** per-species logistic regrowth rate to cache on a spawned `Herd`, falling back to
    /// the global `fauna.ecology.regrowth_rate` when the row omits its own (Grazing Phase 2b-ii). The
    /// pastoral/pen rungs never read this — they keep their own faster `r` (see
    /// [`crate::fauna::herd_ecology`]).
    pub fn regrowth_rate_or(&self, wild_default: f32) -> f32 {
        self.regrowth_rate.unwrap_or(wild_default)
    }
}

/// Spawn-density tuning. `per_biome` is the per-tile probability of placing a game
/// group, keyed by the tile's food module; abundance is high to start by design.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct AbundanceConfig {
    pub per_biome: HashMap<String, f32>,
    pub max_total_game: usize,
    pub min_spacing: u32,
    /// **How many migratory herds a map holds** — the long-route pass's own budget, separate from
    /// `max_total_game` (see [`MigratoryAbundanceConfig`]).
    pub migratory: MigratoryAbundanceConfig,
}

impl AbundanceConfig {
    pub fn probability_for(&self, module_key: &str) -> f32 {
        self.per_biome
            .get(module_key)
            .copied()
            .unwrap_or(0.0)
            .clamp(0.0, 1.0)
    }
}

/// **The migratory herd budget** (issue #290) — `tiles_per_herd` sets the density, `min_herds` /
/// `max_herds` clamp it. These were three bare literals inside `fauna::determine_herd_count`
/// (`area / 3000`, clamped `[2, 6]`), which made the single number that decides whether a migratory
/// species appears on a map untunable without a rebuild.
///
/// **It is a per-map budget shared by the WHOLE migratory roster, drawn with replacement** — so
/// presence per species is `1 − ((n−1)/n)^herds` for `n` migratory rows, *not* linear in the herd
/// count, and the marginal slot buys less each time:
///
/// | herds | presence per species (5 rows) |
/// |---|---|
/// | 2 | 36% |
/// | 3 | 49% |
/// | 5 | **67%** |
/// | 8 | 83% |
/// | 12 | 93% |
///
/// The shipped `tiles_per_herd: 800` puts the standard 80×52 map (area 4160) at **5** — one slot per
/// migratory row, so each row's *expected* herd count is exactly 1 and two thirds of maps carry any
/// given species. It replaces `3000`, under which the standard map computed **1** and was
/// clamp-floored to 2: the density was inert at the shipped size and `min_herds` silently decided
/// everything. Measured before/after in `core_sim/tests/fauna_migratory_representation.rs`.
///
/// **Raising this raises migratory BIOMASS steeply** — a migratory herd carries 4,000–12,000 biomass
/// against a deer's 600–1,200 — so it is a food-economy dial, not just a variety dial. Playtest dial.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MigratoryAbundanceConfig {
    /// Map tiles (full grid, water included) per migratory herd. Validated `> 0` — a `0` would divide
    /// by zero and, before the clamp, is the one value with no sensible reading.
    pub tiles_per_herd: u32,
    /// Floor on the per-map count, so a small map still gets a migration to follow. Validated `>= 1`
    /// (at `0` a small map has no migratory game at all, which no preset wants).
    pub min_herds: u32,
    /// Ceiling on the per-map count. Validated `>= min_herds` — an inverted clamp would make
    /// `min_herds` unreachable and silently win, the exact failure this block exists to remove.
    pub max_herds: u32,
}

impl Default for MigratoryAbundanceConfig {
    /// Mirrors the shipped JSON, so a config that omits the block behaves like the shipped one rather
    /// than deriving all-zeros (`AbundanceConfig` is `#[serde(default)]`, so an omitted block is
    /// reachable) — a `tiles_per_herd: 0` is the divisor in [`Self::herds_for_map`].
    fn default() -> Self {
        Self {
            tiles_per_herd: 800,
            min_herds: 2,
            max_herds: 12,
        }
    }
}

impl MigratoryAbundanceConfig {
    /// How many migratory herds a `width × height` map holds — the density, clamped. `area` is the
    /// full grid (water included), which is what the retired literal formula measured too, so the
    /// promote is a pure re-parameterization of the same shape.
    pub fn herds_for_map(&self, width: u32, height: u32) -> u32 {
        let area = width.saturating_mul(height).max(1);
        let density = area / self.tiles_per_herd.max(1);
        density.clamp(self.min_herds, self.max_herds.max(self.min_herds))
    }
}

/// **The dedicated predator pass tuning** (Predators Phase 1a, `docs/plan_predators.md`). Predators
/// are seeded by their **own** pass (`fauna::spawn_predators`), not from the herbivore short-range
/// pool, so they stay rare and never consume the `abundance.max_total_game` budget; and their
/// prey-limited ecology needs a couple of levers the grazer model has no analog for.
///
/// **There is no `max_packs` cap** — a predator population is *defined by* its prey base, so the pack
/// count is **derived per carnivore species** as `round(eligible_prey_herds × SpeciesDef::prey_ratio)`
/// (the pack count belongs to the predator's own row, not to a single map-wide absolute).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PredatorConfig {
    /// Per-tile probability of seeding a predator pack, keyed by the tile's food module — the
    /// carnivore twin of `abundance.per_biome`.
    pub per_biome: HashMap<String, f32>,
    /// Minimum Chebyshev spacing between two seeded packs.
    pub min_spacing: u32,
    /// **The functional-response taper** — predation may draw a prey herd down to this fraction of the
    /// prey's carrying capacity but no lower in one turn (`fauna::advance_predation`), the predator
    /// twin of `graze.overgraze_escapement_fraction`. So the pack takes less as prey thins and stops
    /// before zero, damping the crash. Validated `0 < f < 0.5` (above the prey's own floors, below its
    /// MSY point).
    pub predation_escapement_fraction: f32,
    /// **The prey-sensing disk radius** — how far (odd-r hex distance) a predator senses prey when
    /// sizing its `K` and drawing prey down. Deliberately **wider** than a herbivore's
    /// `graze_range_radius` (0–1): prey are sparse points, so a graze-sized footprint would contain
    /// zero prey most turns and snap `K→0`. A single clearly-named dial for the whole predator model
    /// (chosen as a global lever over a per-species `SpeciesDef` field). Validated `>= 1`.
    pub prey_sense_radius: u32,
    /// **The pursue-acquisition radius** (Predators Phase 2, `docs/plan_predators.md`) — the odd-r hex
    /// radius within which a **wild carnivore** acquires and steps toward the nearest prey it can eat
    /// (`fauna::advance_herds`' `pursue` dispatch). Deliberately **wider** than the feeding
    /// `prey_sense_radius` (shipped 4, code default 3): a pack *tracks* prey over a larger territory than the disk it *feeds*
    /// from, and a wider acquisition range is the real fix for the transient-zero-prey stranding that
    /// widening `prey_sense_radius` 3→4 only band-aided. Validated `>= 1` (a hard bound, so it stays a
    /// free dial); conceptually it should be `>= prey_sense_radius`, but that intent is left to the
    /// playtest rather than enforced. A playtest dial.
    #[serde(default = "default_pursuit_radius")]
    pub pursuit_radius: u32,
    /// **The raid trigger reach** (Predators Phase 1b, `docs/plan_predators.md`) — how close (odd-r hex
    /// distance) a carnivore must be to a band to raid its camp (`systems::advance_predator_raids`). Its
    /// **own** lever: a raid is the pack reaching the camp, distinct from — and deliberately **tighter
    /// than** — the `prey_sense_radius` disk the pack senses game across. Validated `>= 1`. A playtest
    /// dial.
    #[serde(default = "default_raid_radius")]
    pub raid_radius: u32,
    /// **How many of a band's working-age people are exposed to a raid** (Predators Phase 1b) — the
    /// defender-side populace that can be killed. Bounds a raid so it is a *skirmish*, not a massacre:
    /// only this many folk (beyond the warriors) stand in the pack's path each raid turn. Validated
    /// finite `> 0`. A playtest dial.
    #[serde(default = "default_raid_exposure")]
    pub raid_exposure: f32,
    /// **The share of a band's food income a casualty-causing raid forfeits** (Predators Phase 3,
    /// `docs/plan_predators.md`) — the band's people were defending or fleeing, not gathering, so a raid
    /// that costs lives also costs a fraction of **that turn's** food income, debited from the larder
    /// (capped at what it holds). `0.0` = a raid costs only people; `1.0` = it forfeits the whole turn's
    /// income. Validated finite and in `[0, 1]`. A playtest dial.
    #[serde(default = "default_raid_yield_forfeit_fraction")]
    pub raid_yield_forfeit_fraction: f32,
}

impl Default for PredatorConfig {
    fn default() -> Self {
        Self {
            per_biome: HashMap::new(),
            min_spacing: DEFAULT_PREDATOR_MIN_SPACING,
            predation_escapement_fraction: DEFAULT_PREDATION_ESCAPEMENT_FRACTION,
            prey_sense_radius: DEFAULT_PREY_SENSE_RADIUS,
            pursuit_radius: default_pursuit_radius(),
            raid_radius: default_raid_radius(),
            raid_exposure: default_raid_exposure(),
            raid_yield_forfeit_fraction: default_raid_yield_forfeit_fraction(),
        }
    }
}

impl PredatorConfig {
    /// Per-tile predator-pack spawn probability for a food module (`0.0` for an unlisted module —
    /// predators do not seat there), clamped to `[0, 1]`.
    pub fn probability_for(&self, module_key: &str) -> f32 {
        self.per_biome
            .get(module_key)
            .copied()
            .unwrap_or(0.0)
            .clamp(0.0, 1.0)
    }
}

/// Default predator-pack Chebyshev spacing. See [`PredatorConfig::min_spacing`].
const DEFAULT_PREDATOR_MIN_SPACING: u32 = 6;
/// Default functional-response taper. See [`PredatorConfig::predation_escapement_fraction`].
const DEFAULT_PREDATION_ESCAPEMENT_FRACTION: f32 = 0.15;
/// Default prey-sensing disk radius (wider than a graze footprint). See
/// [`PredatorConfig::prey_sense_radius`].
const DEFAULT_PREY_SENSE_RADIUS: u32 = 3;
/// Default pursue-acquisition radius (wider than the prey-sensing disk). See
/// [`PredatorConfig::pursuit_radius`].
fn default_pursuit_radius() -> u32 {
    8
}
/// Default raid trigger reach (tighter than the prey-sensing disk). See
/// [`PredatorConfig::raid_radius`].
fn default_raid_radius() -> u32 {
    2
}
/// Default number of a band's working-age folk exposed to a raid. See
/// [`PredatorConfig::raid_exposure`].
fn default_raid_exposure() -> f32 {
    4.0
}
/// Default share of a band's food income a casualty-causing raid forfeits. See
/// [`PredatorConfig::raid_yield_forfeit_fraction`].
fn default_raid_yield_forfeit_fraction() -> f32 {
    0.25
}

/// Hunt tuning: how a take converts to resources, the per-policy take multiples, and the pursuit
/// geometry (band closes to `pursuit_radius` tiles).
///
/// **The hunt axis is ONE CONTINUOUS ESCAPEMENT FLOOR** (`docs/plan_harvest_floor.md`,
/// [`crate::fauna::hunt_escapement_ceiling`]): the assignment carries a floor, the take is the stock
/// standing above it, and a deeper floor takes more. There is no per-stance multiplier left to tune,
/// which is why this block carries none.
///
/// **`surplus_multiplier` / `deplete_multiplier` / `surplus_escapement_fraction` are DELETED**, not
/// merely unread. They were kept as validated keys through slice 1 so an inverted pair could not read
/// as a live ladder; once the stances went, keeping them meant `FaunaConfig::validate` could still
/// *reject a boot* over an ordering with no reader — a `FAUNA_CONFIG_PATH` file setting
/// `hunt.surplus_escapement_fraction: 0.6` panicked the server for a lever that did nothing.
///
/// **`take_fraction` / `min_take` / `take_from` stay RETIRED** — floor `0` takes the whole standing
/// stock (clamped by carry + quantise), which is what "eradicate" meant and needs no dial.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct HuntConfig {
    pub provisions_per_biomass: f32,
    pub pursuit_radius: u32,
    pub pursuit_tiles_per_turn: u32,
    pub max_pursuit_turns: u32,
}

impl Default for HuntConfig {
    fn default() -> Self {
        Self {
            provisions_per_biomass: 0.02,
            pursuit_radius: 1,
            pursuit_tiles_per_turn: 3,
            max_pursuit_turns: 12,
        }
    }
}

// NB: `DEFAULT_SURPLUS_MULTIPLIER` (1.5), `DEFAULT_DEPLETE_MULTIPLIER` (2.5) and
// `DEFAULT_SURPLUS_ESCAPEMENT_FRACTION` (0.30) went with the three `HuntConfig` fields they defaulted
// — see that struct's doc for why a defaulted, validated lever with no reader is worse than no lever.

/// **The per-species hunt-yield vector, as CONFIGURED** — *what* a hunt of this species yields, per
/// unit of biomass taken (`docs/plan_hunt_yield_model.md`, issue #337). *How much* biomass is the
/// **stance's** job ([`crate::fauna::hunt_escapement_ceiling`]); the two axes are orthogonal, which is the
/// whole point of the arc: yield is **product × intensity**, never one routed by the other.
///
/// Mirrors the flora roster's per-species vector (`FloraSpecies::yield`), so the two food webs stay
/// the same shape.
///
/// The rate component is `None` ⇒ *"use the global default"*
/// ([`HuntConfig::provisions_per_biomass`]), which is what keeps every species that omits the block
/// byte-identical on its FOOD component. An explicit **`0.0` is a real, meaningful value** — it is
/// how a wolf says *"you do not eat me"*. That distinction is why the field is `Option`, not a bare
/// float with a `0` sentinel.
///
/// **`trade_goods_per_biomass` is RETIRED** (arc #527) — a wolf's pelt is `materials`, and already
/// was, on the row below.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HuntYieldDef {
    pub provisions_per_biomass: Option<f32>,
    /// **What the carcass is MADE OF** — hide, bone, sinew — per unit of biomass carried home
    /// (`docs/plan_crafting_and_materials.md` §2). Authored per row and tunable, and **the same
    /// shape the flora roster's `yield.materials` carries**: nothing in the materials model is
    /// fauna-shaped, so a plant and a deposit state their yield the same way.
    ///
    /// An empty list is the ordinary case for a species nothing is made out of. Unlike the rate
    /// component above there is **no global to fall back to**: a material is a *thing*, and there
    /// is no species-blind statement of which thing an animal gives.
    ///
    /// Validated against the materials table at load
    /// ([`crate::materials_config::MaterialsConfig::validate_yield`]) — the material must exist and
    /// the reading must name **exactly** the axes it declares.
    pub materials: Vec<crate::materials_config::MaterialYieldDef>,
}

/// **The per-species hunt-yield vector, RESOLVED** — the configured [`HuntYieldDef`] with its `None`
/// components filled from the `hunt.*` globals. Produced by exactly one seam,
/// [`FaunaConfig::hunt_yield_for`], so no call site re-derives the fallback.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HuntYield {
    pub provisions_per_biomass: f32,
    /// **Does a carcass of this species give any MATERIAL at all** — the one thing the roster's
    /// `materials` list can say that a per-biomass *rate* cannot. It is carried here because this
    /// type is the resolved vector every take and every forecast reads, and it has to be able to
    /// answer [`HuntYield::yields_nothing`] without holding the roster.
    ///
    /// A `bool` rather than the list itself for the same reason the rate beside it is a bare float:
    /// this vector is `Copy` and is threaded through every take path, and *"is there anything here
    /// at all"* is the only question asked of it. Resolved once, at [`FaunaConfig::hunt_yield_for`]
    /// — derived from the roster row, never authored beside it.
    pub yields_materials: bool,
}

impl HuntYield {
    /// **Is this species food?** `edible` is **DERIVED, never stored** — one source of truth (the
    /// vector), and the flag is a comparison against it. A stored `edible: bool` beside a
    /// `provisions_per_biomass` is two statements of one fact that can drift.
    pub fn edible(self) -> bool {
        self.provisions_per_biomass > 0.0
    }

    /// **The degenerate species: a pure pest, worth neither meat nor hide.** No shipped species is
    /// this today — a wolf is inedible and is still a pelt and a bone — but the picker rule
    /// ([`crate::fauna::species_requires_denial`]) is stated in terms of it so it derives correctly
    /// the day one arrives, instead of being retro-fitted then.
    ///
    /// **The materials half is what keeps the wolf huntable.** Until arc #527 this read *"neither
    /// edible nor tradeable"* and the wolf's whole payload was the retired trade scalar, so testing
    /// food alone would prune every rung but denial off a species a band genuinely hunts for hides.
    pub fn yields_nothing(self) -> bool {
        !self.edible() && !self.yields_materials
    }

    /// **THE conversion — one call for the whole take.** Converting one account and forgetting
    /// another is the precise failure mode this model risks across ~20 readout sites, so the
    /// components are never available separately from a take: you get the vector or nothing.
    ///
    /// `output_multiplier` is the acting band's productivity (a linear factor), applied exactly as
    /// the retired `hunt_provisions` applied it to food.
    ///
    /// **The MATERIALS a carcass gives are NOT here**, and cannot be: they are per-material batches
    /// with a characteristic vector each, credited off `take.carried` at the take site
    /// (`systems::labor`'s `credit_material_yield`). This type is the flat, addable half.
    ///
    /// **Do NOT invert this to count animals.** Whole-animal quantisation stays in *biomass* space —
    /// see [`crate::fauna::quantise_animal_take`], which spells out why (`provisions_per_biomass ==
    /// 0` makes `floor(food_ceiling / food_per_animal)` a `0/0`).
    pub fn apply(self, biomass_take: f32, output_multiplier: f32) -> YieldAccounts {
        YieldAccounts {
            provisions: biomass_take * self.provisions_per_biomass * output_multiplier,
            // **An animal pays no fodder.** The second account is the PLANT web's (`hay_grass` pays
            // fodder and nothing else); no species' `HuntYield` has a fodder rate to apply, so this is
            // a structural zero rather than an unprojected gap.
            fodder: 0.0,
        }
    }
}

/// **What one take actually pays, in every SCALAR account.** The return of [`HuntYield::apply`] —
/// food (fully fractional, banked on the larder's `Scalar` grid) and fodder (the plant web's animal
/// feed, `FODDER` on a band's `LocalStore`). Both land on the producing band's `LocalStore`.
///
/// **Two accounts, not three, since arc #527.** The trade-goods component was written by every take
/// site and read by none — there was no `take(TRADE_GOODS)` anywhere in the workspace — while the
/// **materials** credited beside it named the same take's actual stuff. Materials cannot live here:
/// they are batches keyed by a characteristic vector, and this type is the part that adds, scales
/// and `min`s componentwise.
///
/// **Not to be confused with [`crate::flora_config::YieldVector`]**, which is the per-*biomass* RATE
/// pair (`provisions_per_biomass` / `fodder_per_biomass`). This type is a per-turn AMOUNT — the rate
/// times a biomass take. The field names are the tell.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct YieldAccounts {
    pub provisions: f32,
    pub fodder: f32,
}

/// **Which component of a [`YieldAccounts`] a RATIO is counted on.**
///
/// Every whole-animal count in the yield model is a *ratio* — `floor(ceiling / one animal)` — and a
/// ratio is unit-free: taken on any component whose per-biomass rate is **positive**, it gives the
/// same animal count, because that component is a positive linear image of biomass. Taken on a
/// component whose rate is `0` it is a `0/0`. So a ratio never picks a component by convention; it
/// asks [`YieldAccounts::ratio_axis`] which one is legal.
///
/// This is the operational form of `quantise_animal_take`'s warning: **never divide by a food number
/// you have not established is positive.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YieldAxis {
    Provisions,
    /// **Plant-only in practice** — no animal pays fodder, so this is never selected on the hunt
    /// path. It exists because [`YieldAccounts::is_zero`] is defined through [`YieldAccounts::ratio_axis`],
    /// and a hay-only patch (food `0`, positive fodder) must not read as paying nothing.
    Fodder,
}

impl YieldAccounts {
    /// A source that pays nothing in any account.
    pub const ZERO: Self = Self {
        provisions: 0.0,
        fodder: 0.0,
    };

    /// Every component scaled by the same factor — worker counts, output multipliers, and the
    /// carried/killed share of a quantised take are all scalar, so they never touch the *mix*.
    pub fn scale(self, factor: f32) -> Self {
        Self {
            provisions: self.provisions * factor,
            fodder: self.fodder * factor,
        }
    }

    /// Component-wise sum — accumulating a projection turn by turn. (Named `plus`, not `add`, so it
    /// cannot be confused with `std::ops::Add::add`, which this type deliberately does not implement:
    /// these are *amounts in different currencies*, and blanket arithmetic on them invites summing
    /// bread into hay.)
    pub fn plus(self, other: Self) -> Self {
        Self {
            provisions: self.provisions + other.provisions,
            fodder: self.fodder + other.fodder,
        }
    }

    /// Component-wise `min` — the continuous take's `min(collection, ceiling)`. Sound because both
    /// operands are the same biomass put through the same rates, so every component agrees on
    /// which side binds.
    pub fn min(self, other: Self) -> Self {
        Self {
            provisions: self.provisions.min(other.provisions),
            fodder: self.fodder.min(other.fodder),
        }
    }

    /// Read one component.
    pub fn component(self, axis: YieldAxis) -> f32 {
        match axis {
            YieldAxis::Provisions => self.provisions,
            YieldAxis::Fodder => self.fodder,
        }
    }

    /// **The axis a ratio against these accounts may be counted on** — the first component with a
    /// strictly positive value, preferring `Provisions` so every edible species keeps *exactly* the
    /// arithmetic it had before the vector (bit-identical, not merely equivalent). `None` when every
    /// account is empty: nothing to count, and nothing may divide by it.
    ///
    /// **`Fodder` is tested LAST, and that ordering is load-bearing** — it preserves every answer
    /// this function gave before #426, exactly as preferring `Provisions` preserved every answer
    /// from before #337.
    pub fn ratio_axis(self) -> Option<YieldAxis> {
        if self.provisions > 0.0 {
            Some(YieldAxis::Provisions)
        } else if self.fodder > 0.0 {
            Some(YieldAxis::Fodder)
        } else {
            None
        }
    }

    /// Does this pay nothing at all?
    pub fn is_zero(self) -> bool {
        self.ratio_axis().is_none()
    }

    /// **Rebuild the whole vector from a value measured on ONE axis**, using `self` as the reference
    /// mix: the result reads exactly `value` on `axis` (bit-identical — no divide-then-multiply
    /// round trip on the axis that was actually computed) and carries the other component at the
    /// same proportion.
    ///
    /// This is how a quantised take crosses back from the single axis it was counted on: the animal
    /// count is one number, and it values the same in every currency.
    pub fn rescaled_to(self, axis: YieldAxis, value: f32) -> Self {
        let reference = self.component(axis);
        if reference <= 0.0 {
            // Nothing to scale against — the caller already established a positive axis, so this is
            // the degenerate "yields nothing" source.
            return Self::ZERO;
        }
        let share = value / reference;
        match axis {
            YieldAxis::Provisions => Self {
                provisions: value,
                fodder: self.fodder * share,
            },
            YieldAxis::Fodder => Self {
                provisions: self.provisions * share,
                fodder: value,
            },
        }
    }
}

/// Ecology tuning: per-turn **critical-depensation** biomass dynamics toward each
/// species' carrying cap. Above the Allee threshold (`collapse_fraction * cap`) the
/// group regrows logistically at `regrowth_rate`; below it the group is non-viable and
/// declines by `collapse_rate` of its biomass each turn — an irreversible crash to
/// local extinction even without further hunting (the overhunting point-of-no-return).
/// A collapsing remnant below `extinction_floor * cap` disperses (despawns).
/// `stressed_fraction` is the softer band used only to classify a herd's `EcologyPhase`
/// for the client; it does not affect the growth curve.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default)]
pub struct EcologyConfig {
    pub regrowth_rate: f32,
    /// Allee threshold as a fraction of carrying capacity. Below `collapse_fraction *
    /// cap` the group collapses (depensation) instead of regrowing — the overhunting point of no
    /// return that turns Surplus/Deplete's steady overdraw into an irreversible crash. (It **used** to
    /// double as Deplete's escapement floor; slice 8b made the hunt policies multiples of MSY, so this
    /// is once again only the depensation threshold.)
    pub collapse_fraction: f32,
    /// Per-turn fractional decline of a collapsing (sub-threshold) group.
    pub collapse_rate: f32,
    /// Upper edge of the "stressed" (depleted-but-recovering) band, as a fraction of
    /// carrying capacity. Classification only.
    pub stressed_fraction: f32,
    /// Viability floor: a group below `extinction_floor * cap` disperses (local
    /// extinction) so a collapse reaches zero in finite turns.
    pub extinction_floor: f32,
}

impl Default for EcologyConfig {
    fn default() -> Self {
        Self {
            regrowth_rate: 0.05,
            collapse_fraction: 0.15,
            collapse_rate: 0.20,
            stressed_fraction: 0.40,
            extinction_floor: 0.02,
        }
    }
}

/// Immigration tuning: a low per-turn chance to respawn a wild-game group up to the
/// abundance cap so an overhunted map slowly replenishes (early forager play stays
/// game-rich). `max_attempts` bounds the per-turn random tile sampling.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ImmigrationConfig {
    pub chance_per_turn: f32,
    pub max_attempts: u32,
}

impl Default for ImmigrationConfig {
    fn default() -> Self {
        Self {
            chance_per_turn: 0.15,
            max_attempts: 12,
        }
    }
}

/// Follow tuning: the small per-turn non-food tracking benefit (fog reveal pulse + morale).
///
/// Follow tuning: the small per-turn non-food benefit a tracking band gets (fog-reveal pulse +
/// morale). A `follow.surplus_multiplier` field is **retired** (it was briefly a `1.6 × MSY` *flow*,
/// which a whole-animal take cannot survive: a constant-in-`B` ceiling never accumulates one body),
/// and there is no take multiple anywhere else either — the take axis is a floor.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct FollowConfig {
    pub reveal_radius: u32,
    pub reveal_duration_turns: u64,
    pub morale_gain: f32,
}

impl Default for FollowConfig {
    fn default() -> Self {
        Self {
            reveal_radius: 2,
            reveal_duration_turns: 3,
            morale_gain: 0.01,
        }
    }
}

/// Husbandry tuning — **the animal web's own economy**. Taming's own dials are *not* here: the
/// **`Tame` policy**'s build meter (`work_cost` / `decay_fraction_per_turn` /
/// `yield_fraction_while_building`) lives on `intensification_ladder.json`'s `animal:pastoral` rung,
/// alongside the pen's on `animal:pen`, so both food webs climb on the same numbers
/// (`crate::intensification`). The retired `claim_threshold` — the `domesticate` command's
/// early-claim — is **gone with the command**: it existed to skip the taming investment, which is
/// the entire decision.
///
/// **The husbandry yield ladder is FLOW-BASED — every rung pays MSY**
/// (`docs/plan_corral_managed_population.md`). Management does not buy a licence to eat the standing
/// stock; it buys a **higher growth rate**, because a managed herd is protected from predation,
/// disease and winter kill. The rungs differ *only* in the ecology their MSY is computed against, and
/// in what that ecology costs you:
///
/// | Rung | Ecology | `r` | Costs |
/// |---|---|---|---|
/// | Wild | `fauna.ecology` | per-species `wild_r` | a worker |
/// | Mobile domesticated (**pastoral**) | [`PastoralConfig::ecology`] | `min(cap, wild_r × pastoral_gain)` | none — passive |
/// | Penned (**pen**) | [`PenConfig::ecology`] | `min(cap, wild_r × pen_gain)` | a worker + **food upkeep** + pinned |
///
/// Since Grazing 2d the managed rungs are **per-species** (`wild_r × gain`, capped) rather than the
/// retired flat `0.25 / 0.90` — a penned rabbit and a penned mammoth are different economies. A penned
/// herd's carrying capacity is its **fenced footprint's** graze flow (`hex_range_tiles(corralled_at,
/// pen_radius)`), so it grazes its own land and the larder only pays what the pasture cannot cover
/// (`pen_upkeep × biomass × (1 − pasture_fraction)`) — `capacity_fraction` is retired.
///
/// The managed harvest **draws the herd down**, which is what makes it sustainable: the herd
/// converges on `K/2` and holds there, paying `r·K/4` forever. Both husbandry rungs take it through
/// the shared helper `fauna::managed_yield_biomass`, which is **constant-*escapement* MSY** —
/// `take = min(peak_regrowth(K), max(0, B − K/2))` — **not** the constant-*catch* `sustainable_yield`
/// a wild `Sustain` hunt takes. The sim regrows in Logistics and harvests in Population, so a
/// constant-catch take is evaluated at the **post**-regrowth biomass; above `K/2` both forms cap at
/// MSY and converge on `K/2`, but **below `K/2`** constant-catch removes `g(B + g(B)) > g(B)` — more
/// than the herd grew — which at the pen's `r` = 0.90 spirals a fully-fed herd to zero. Escapement
/// never takes a herd below `K/2`, so a depleted managed herd **rebuilds** (yielding less while it
/// does) and then pays `r·K/4` forever — stable from both sides. The retired flat
/// `provisions_per_biomass` / `corral_provisions_per_biomass` rates, by contrast, paid a share of
/// standing **stock** and never drew the herd down at all — a penned herd parked at capacity and
/// printed food forever (~48× the Sustain baseline).
///
/// **Corral (Rung 1c) levers.** Corralling is an **explicit `Corral` policy with an investment
/// cost**, the animal twin of Cultivate. Its **build dials moved to the shared ladder**,
/// `data/intensification_ladder.json` → the `animal:pen` rung's `build` block
/// (`crate::intensification`), so both food webs climb on the same numbers: while the pen is being
/// built (`Herd::corral_progress` < 1.0) the crew takes only that rung's
/// `yield_fraction_while_building × the herd's Sustain (MSY) ceiling` — a sustainable draw, so the
/// herd stays healthy — accruing its crew's work output each turn; at the job's cost the herd is penned
/// (`corralled_at`) and its keeper harvests the pen's MSY, paying `pen.upkeep_per_biomass` per unit
/// of biomass in feed. What stays here is the animal web's own economy.
///
/// **The earned-knowledge levers are GONE from here** (slice 4): `knowledge_progress_per_turn` /
/// `knowledge_completion_threshold` moved to `intensification_ladder.json`'s ladder-level `knowledge`
/// block, which `labor_config` had duplicated verbatim — once the earn path became one rung-driven
/// seam (`RungDef::knowledge_earned`), a number that paces *both* food webs belonged to the ladder,
/// exactly like the build dials. **And the gate they pace reshuffled with them:** Herding gates `Tame`
/// (rung 2) and **only** `Tame`; `Corral` (rung 3) is gated on **Penning**, which is earned by working
/// an already-tamed herd — one knowledge per rung-transition. **The cultivation asymmetry is gone:**
/// taming is no longer ungated, and a Sustain hunt no longer tames anything — it only *teaches*,
/// exactly as a Sustain forage only teaches Cultivation.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct HusbandryConfig {
    /// The **mobile domesticated** (pastoral) rung: the ecology a tamed, roaming herd lives under.
    pub pastoral: PastoralConfig,
    /// The **penned** rung: the ecology a corralled herd lives under, plus what the pen costs to run.
    pub pen: PenConfig,
    /// **Per-species husbandry growth (Grazing 2d §3).** The mobile-domesticated (pastoral) rung grows
    /// at `min(husbandry_regrowth_cap, wild_r × pastoral_gain)` — a MULTIPLE of the herd's own wild
    /// breeding rate, not a flat rate, so a tamed rabbit and a tamed mammoth are different economies.
    /// `> 1` (management must beat wild growth); `< pen_gain` (the ladder is monotone). Folded into the
    /// pastoral ecology by [`crate::fauna::herd_ecology`]; retires the flat `pastoral.ecology.regrowth_rate`.
    pub pastoral_gain: f32,
    /// The penned rung's growth multiplier: `min(husbandry_regrowth_cap, wild_r × pen_gain)` — the top
    /// of the ladder (`> pastoral_gain`). Retires the flat `pen.ecology.regrowth_rate`.
    pub pen_gain: f32,
    /// The stable-band ceiling on any managed `r`: `pastoral`/`pen` growth is capped here so a fast
    /// breeder (rabbit wild 0.35 × pen_gain 3.0 = 1.05) is held to a logistic rate that does not
    /// overshoot/oscillate. `0.75` keeps the discrete logistic monotone.
    pub husbandry_regrowth_cap: f32,
    /// **The largest fenced-footprint radius a pen may reach** (Grazing 2d-β, the `ExtendPen` command).
    /// Each worked-off ring grows `Herd::pen_radius` by 1; the command refuses once `pen_radius` reaches
    /// this. `2` → up to a 19-tile footprint (`hex_range_tiles` disk `1, 7, 19`). Validated `>= 1`
    /// (a `0` cap would forbid every extension).
    pub pen_radius_max: u32,
    /// **The herder-requirement hysteresis deadband, as a fraction of `animals_per_herder`.** The raw
    /// `herders_needed = ceil(animals / animals_per_herder)` flickers ±1 when a Sustain-hunted herd's
    /// biomass breathes across an `animals_per_herder` multiple (the lumpy whole-animal kill), trapping
    /// the player in a "staff all 1 / staff all 2" churn. [`crate::fauna::Herd::stabilize_herders_needed`]
    /// uses `animals_per_herder × this` as the down-step deadband, so a bumped-up requirement holds
    /// until the herd falls *well* below the lower rung's ceiling — enough to absorb the ±1-animal
    /// oscillation. `0.25` ≈ a quarter of a herder's flock. Validated finite & `>= 0` (`0` disables the
    /// deadband, restoring the raw stateless flicker). A **playtest dial**.
    pub herders_hysteresis_fraction: f32,
    /// **The shed rate for an under-contained PASTORAL (unfenced) herd** — the fraction of the herd's
    /// labor-capacity *overage* that walks off into the wild web each turn (`docs/plan_fauna_neglect_escape.md`
    /// §2.2/§3.4). This is the "animals leave" mechanic that **replaced** the tameness-bleed: neglect
    /// costs the visible axis (herd size), never the invisible one (`domestication_progress`). It is a
    /// fraction of the **overage** (`(1 − herded_fraction) × current_animals`), not of the total, so the
    /// herd self-limits toward its labor capacity and stops shedding once it fits. `0.25` ≈ a quarter
    /// of the surplus leaves per turn (faster than a pen — no fence buys time). Validated finite &
    /// `>= 0`, and **strictly greater than `pen_escape_fraction`** (the fence must be slower). A
    /// **playtest dial**.
    pub pastoral_escape_fraction: f32,
    /// **The shed rate for an under-contained PENNED herd** — the pen twin of `pastoral_escape_fraction`,
    /// **slower because the fence buys time** (`docs/plan_fauna_neglect_escape.md` §2.2). Same code path,
    /// only the rate differs. Total abandonment (no keeper ⇒ `herded_fraction == 0`) falls out as the
    /// `overage == current_animals` limit: the whole flock sheds toward zero over several turns at this
    /// rate, and the pen is lost when the last animal goes (§2.4). `0.10` ≈ a tenth of the surplus per
    /// turn. Validated finite, `>= 0`, and **`< pastoral_escape_fraction`** — stating the invariant makes
    /// "pen faster than open range" unrepresentable. A **playtest dial**.
    pub pen_escape_fraction: f32,
    /// **The ± band the seeded RNG varies each shed rate by, for playability** (`docs/plan_fauna_neglect_escape.md`
    /// §3.1/§3.4). The effective per-turn rate is `rate × (1 + jitter)` with `jitter` drawn from
    /// `[-escape_fraction_jitter, +escape_fraction_jitter]` off the **world seed stream** (deterministic
    /// under rollback — never wall-clock `rand`). `0.25` = the rate varies ±25% turn to turn. Validated
    /// finite & `>= 0` (`0` disables the jitter, i.e. an exactly-constant rate). A **playtest dial**.
    pub escape_fraction_jitter: f32,
}

impl Default for HusbandryConfig {
    fn default() -> Self {
        Self {
            pastoral: PastoralConfig::default(),
            pen: PenConfig::default(),
            pastoral_gain: DEFAULT_PASTORAL_GAIN,
            pen_gain: DEFAULT_PEN_GAIN,
            husbandry_regrowth_cap: DEFAULT_HUSBANDRY_REGROWTH_CAP,
            pen_radius_max: DEFAULT_PEN_RADIUS_MAX,
            herders_hysteresis_fraction: DEFAULT_HERDERS_HYSTERESIS_FRACTION,
            pastoral_escape_fraction: DEFAULT_PASTORAL_ESCAPE_FRACTION,
            pen_escape_fraction: DEFAULT_PEN_ESCAPE_FRACTION,
            escape_fraction_jitter: DEFAULT_ESCAPE_FRACTION_JITTER,
        }
    }
}

/// The **mobile domesticated (pastoral) rung** of the husbandry ladder: a tamed herd that still roams
/// with the band. It pays its owner the MSY of *this* ecology every turn, passively — no worker, no
/// upkeep (a roaming herd grazes the land for free; that is what roaming *is*).
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct PastoralConfig {
    /// The ecology a *tamed, mobile* herd lives under — the **phase bands only** now. Since Grazing 2d
    /// the pastoral `regrowth_rate` is **per-species** (`min(husbandry_regrowth_cap, wild_r ×
    /// pastoral_gain)`, folded in by [`crate::fauna::herd_ecology`]); this block's own `regrowth_rate`
    /// is unused (it defaults to the wild rate and only the shared `collapse_fraction`/… bands are read,
    /// so a pastoral herd classifies Thriving/Stressed on the same scale as a wild one).
    pub ecology: EcologyConfig,
}

/// The **penned (corral) rung**: a confined herd. Highest growth rate on the ladder — and the only
/// rung with a running cost, because a penned herd **cannot graze** and so must be fed.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PenConfig {
    /// The ecology a *penned* herd lives under — the **phase bands only** now. Since Grazing 2d the pen
    /// `regrowth_rate` is **per-species** (`min(husbandry_regrowth_cap, wild_r × pen_gain)`, folded in
    /// by [`crate::fauna::herd_ecology`] / `pen_ecology_for`); this block's own `regrowth_rate` is
    /// unused (only the shared phase bands are read). The keeper harvests the per-species pen MSY.
    pub ecology: EcologyConfig,
    /// **Feed.** Food/turn the pen demands per unit of standing biomass, drawn from the keeper band's
    /// larder (`upkeep_per_biomass × biomass`). [`DEFAULT_PEN_UPKEEP_PER_BIOMASS`] carries the
    /// derivation and the net-positive invariant it must satisfy — see
    /// [`FaunaConfig::validate`], which enforces it.
    pub upkeep_per_biomass: f32,
    /// **Starvation.** An underfed pen (`fed_fraction < 1`) shrinks by `starve_shrink_rate × (1 −
    /// fed_fraction) × biomass` each turn, floored at `ecology.extinction_floor × K_pen`: the herd
    /// withers to a remnant and **recovers when fed again** (it does not despawn and does not lose the
    /// pen — a recoverable famine is better play than silently voiding a 25-turn investment).
    /// [`DEFAULT_PEN_STARVE_SHRINK_RATE`] carries the derivation. Validated in `[0, 1]`.
    pub starve_shrink_rate: f32,
}

impl Default for PenConfig {
    fn default() -> Self {
        Self {
            // Phase bands only — the pen `regrowth_rate` is per-species (Grazing 2d), so this defaults
            // to the shared wild bands and its own rate is unread.
            ecology: EcologyConfig::default(),
            upkeep_per_biomass: DEFAULT_PEN_UPKEEP_PER_BIOMASS,
            starve_shrink_rate: DEFAULT_PEN_STARVE_SHRINK_RATE,
        }
    }
}

/// **The pastoral growth multiplier (Grazing 2d §3).** A tamed, mobile herd grows `pastoral_gain ×`
/// its own wild breeding rate (capped at [`DEFAULT_HUSBANDRY_REGROWTH_CAP`]) — protection from
/// predation/disease/winter kill buys a *multiple* of the species' own `r`, not a flat rate, so a
/// tamed rabbit (0.35 → 0.525) and a tamed mammoth (0.04 → 0.06) become different economies. Retires
/// the flat `0.25`. A **playtest lever** — measure and tune (`docs/plan_grazing_2d.md` §3).
const DEFAULT_PASTORAL_GAIN: f32 = 1.5;

/// **The pen growth multiplier (Grazing 2d §3).** The ladder's top: a penned herd grows `pen_gain ×`
/// its wild rate (capped). Resulting pen `r`: rabbit `0.75` (capped, booms) · deer `0.30` · mammoth
/// `0.12` (a long-haul investment). Retires the flat `0.90`. A **playtest lever**.
const DEFAULT_PEN_GAIN: f32 = 3.0;

/// **The stable-band cap on any managed `r`.** `wild_r × gain` is clamped here so a fast breeder cannot
/// be scaled into an unstable/oscillating discrete-logistic rate. `0.75` keeps growth monotone (well
/// below the `r ≥ 1` overshoot regime). A **playtest lever**.
const DEFAULT_HUSBANDRY_REGROWTH_CAP: f32 = 0.75;

/// **The largest fenced-footprint radius a pen may reach** (Grazing 2d-β). `2` → up to a 19-tile
/// footprint; each ring is a 25-turn `ExtendPen` labor investment. A **playtest lever** (higher = pens
/// can grow into larger self-feeding operations at more keeper-turns of cost).
const DEFAULT_PEN_RADIUS_MAX: u32 = 2;

/// **The herder-requirement hysteresis deadband** as a fraction of `animals_per_herder` (see
/// [`HusbandryConfig::herders_hysteresis_fraction`]). `0.25` absorbs the ±1-animal Sustain oscillation
/// so a staffed herd holds its keeper count instead of flickering ±1. A **playtest dial**.
const DEFAULT_HERDERS_HYSTERESIS_FRACTION: f32 = 0.25;

/// **The pastoral shed rate** (`docs/plan_fauna_neglect_escape.md` §3.4): a quarter of an
/// under-contained *unfenced* herd's labor-capacity overage walks off into the wild web each turn.
/// Faster than a pen because nothing pens it. A **playtest dial**.
const DEFAULT_PASTORAL_ESCAPE_FRACTION: f32 = 0.25;

/// **The pen shed rate** — the pastoral rate's fenced twin, slower because the fence buys time
/// (`docs/plan_fauna_neglect_escape.md` §3.4). Validated **strictly below** the pastoral rate, so a
/// config that made a pen leak faster than open range is unrepresentable. A **playtest dial**.
const DEFAULT_PEN_ESCAPE_FRACTION: f32 = 0.10;

/// **The ± jitter band on each shed rate** (`docs/plan_fauna_neglect_escape.md` §3.1): the seeded
/// per-herd RNG varies the effective rate `±25%` turn to turn for playability, drawn from the world
/// seed stream so it stays deterministic under rollback. A **playtest dial**.
const DEFAULT_ESCAPE_FRACTION_JITTER: f32 = 0.25;

/// **The pen's feed cost per unit of biomass — the running cost the arc exists to add.**
///
/// **Grazing 2d inverts the old "every pen is net-positive" guarantee (§2.4).** With per-species pen
/// `r` and *situational* (pasture-dependent) feed, a static all-species guarantee no longer models the
/// system: a slow-breeder pen (mammoth pen `r ≈ 0.12` → bound `0.0011`) would reject the shipped
/// `0.002`, yet such a pen running at a loss on poor pasture is now a player's **bad placement, not a
/// config error**. So [`FaunaConfig::validate`] enforces only a **best-case sanity floor**: the upkeep
/// dial must leave the **fastest-breeding** species profitable even when *fully larder-fed* (worst
/// pasture) — `u < r_pen · p / (2 + r_pen)` for `r_pen = min(cap, max_wild_r × pen_gain)`. With
/// `r_pen(rabbit) = 0.75`: `0.002 < 0.75 × 0.02 / 2.75 ≈ 0.0055` ✓. Slow breeders and poor pasture may
/// run a pen at a **loss by design** (see [`PEN_ESCAPEMENT_QUARTERS`] for the operating-point
/// derivation the floor uses).
///
/// **Deliberately left alone by the growth-rate retune**: weakening the feed to fix a balance problem
/// would delete the mechanic the arc exists to add.
const DEFAULT_PEN_UPKEEP_PER_BIOMASS: f32 = 0.002;

/// **How fast an unfed pen wastes away**: a fully-unfed herd loses 10% of its biomass per turn. Slow
/// enough that a bad winter is survivable and visibly recoverable (the player sees the herd shrink and
/// can act), fast enough that neglecting the feed for a decade of turns really does reduce the pen to
/// a remnant.
const DEFAULT_PEN_STARVE_SHRINK_RATE: f32 = 0.10;

// **RETIRED: the `market` block and its `trade_goods_multiplier`** (`docs/plan_hunt_yield_model.md`
// §3, issue #337), and since arc #527 **the trade-goods axis it multiplied**. The multiplier paid the
// deepest rung 4× the base rate, re-welding product to policy; the rate itself was then written by
// every take site and read by nobody, while the `materials` list beside it named the same take's
// actual hides and bone. What a take is worth is now the per-species [`HuntYieldDef`]'s food rate
// plus its material rows, and nothing else.
//
// `take_fraction` stayed retired before this and is gone with the block.

/// **The graze (pasture) layer** — the land's *animal-edible* vegetal stock (grass, browse, forbs),
/// distinct from the human-edible `ForagePatch.biomass` (seeds/nuts/tubers) on food-module tiles.
/// Authoritative design: `docs/plan_grazing_foundation.md`. It lives on **any vegetated land tile**,
/// with a capacity set by that tile's biome — a temperate forest is rich in nuts and poor in graze
/// (the canopy shades out ground cover); a prairie steppe is the reverse.
///
/// **Homed in `fauna_config.json`, not a file of its own**, because graze is the *substrate of the
/// fauna model*: every consumer of it (herd carrying capacity, competition, overgrazing, migration,
/// spawn placement — Phase 2b/2c) is a fauna system, and no labor/human system may ever read it. That
/// also lets it reuse [`FaunaConfig::validate`] and its `validate_ecology` helper verbatim rather
/// than forking a second loader, env override and error enum.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct GrazeConfig {
    /// Grazeable biomass a tile of each biome carries at capacity. **A pure data table, not a
    /// formula** — every [`TerrainType`] (`TerrainType::VALUES`) must appear (enforced by
    /// [`FaunaConfig::validate`]: a missing biome would silently read as zero graze, i.e. an
    /// invisible dead zone). `0.0` is the *deliberate* reading for water, glacier, bare rock and lava.
    /// The absolute scale is a free parameter — only the *ratios* matter until Phase 2b's
    /// `fodder_per_biomass` denominates it into animals.
    pub capacity_by_biome: HashMap<TerrainType, f32>,
    /// Graze regrowth + the Thriving/Stressed/Collapsing phase bands. **Grass has no Allee
    /// depensation** — `advance_graze_regrowth` runs pure `logistic_regrowth`, never
    /// `net_biomass_delta`'s collapse branch — so `collapse_rate` here is *inert* (it is read by no
    /// graze code path; the shared [`EcologyConfig`] simply carries it, exactly as `labor_config`'s
    /// forage ecology does). `regrowth_rate` is tuned **well above** forage's 0.25 and fauna's 0.05:
    /// see [`DEFAULT_GRAZE_REGROWTH_RATE`].
    pub ecology: EcologyConfig,
    /// The reseed standing crop, as a fraction of the tile's capacity, that a depleted patch is
    /// lifted to before regrowth each turn — the exact mirror of `forage.reseed_floor_fraction`.
    /// Grass reseeds from surrounding ground, so **graze is never permanently dead**: an eaten-out
    /// tile recovers from this seed stock via the normal logistic curve instead of sticking at `0`
    /// (`logistic_regrowth(0, ..) == 0`). Kept below `ecology.collapse_fraction` so a stripped pasture
    /// still reads Collapsing — the floor stops permanent death, it does not hide overgrazing.
    pub reseed_floor_fraction: f32,
    /// **The overgrazing escapement floor** (Grazing Phase 2b-ii), as a fraction of a tile's capacity:
    /// grazing (`fauna::advance_herd_grazing`) can draw a patch down to this biomass but **no lower**
    /// in a turn. This is the constant-*escapement* discipline the coupled herd↔graze system needs to
    /// converge (`docs/plan_grazing_2b.md` §2.2, the same lesson the corral learned): the herd's demand
    /// is a constant-*catch* draw on the graze, and a catch that strips a patch past the point where its
    /// regrowth can refill the offtake collapses the range into a permanently-stripped attractor at the
    /// reseed floor (the herd surviving as a stunted remnant on dead ground). Holding the draw above
    /// this fraction bounds `K` below at `graze_sustainable_flow(escapement·cap)/fodder`, so an
    /// **overgrazed range recovers to a stable smaller herd** instead of crashing. Set **above**
    /// `reseed_floor_fraction` (so it is a real escapement, not just the reseed lift) and **below**
    /// `MSY_BIOMASS_FRACTION` (0.5, the graze's own MSY point — so overgrazing below the productive
    /// intensity is still *possible and visible*, just not unbounded). A **starting anchor** — deeper
    /// (lower) allows more dramatic overgrazing at more crash risk; measure and retune (§9.5).
    pub overgraze_escapement_fraction: f32,
}

/// Graze regrows **fast** — it is the quickest-renewing vegetal stock in the model, and that is the
/// whole economic premise of herding: a pasture eaten to the ground is back within a few seasons,
/// where a nut grove is not.
///
/// Ordering (each rung is a claim about the biology, not a knob): wild fauna `0.05` ≪ forage
/// `0.25` (`labor_config.json`) < **graze `0.40`** ≪ a fed pen `0.90` (a hyper-managed system, not a
/// wild one). At `r = 0.40` a tile's sustainable flow is `r·K/4 = 0.10·K` per turn and a stripped
/// pasture climbs back to ~90% of capacity in ~20 turns (vs ~35 at forage's `0.25`).
const DEFAULT_GRAZE_REGROWTH_RATE: f32 = 0.40;

/// Mirrors `forage.reseed_floor_fraction` (0.02) — see [`GrazeConfig::reseed_floor_fraction`].
const DEFAULT_GRAZE_RESEED_FLOOR_FRACTION: f32 = 0.02;

/// The overgrazing escapement floor (Grazing 2b-ii) — grazing cannot draw a patch below this fraction
/// of capacity, the constant-escapement discipline that keeps the herd↔graze loop convergent. Measured
/// (`core_sim/tests/grazing_2b_convergence.rs`): at `0.25` an overgrazed range settles on degraded
/// ground (graze ~0.25–0.5·cap, `K` ≥ ~0.84·`K_max`) and **recovers**, where the bare reseed floor
/// (0.02) locks it into a stripped remnant. See [`GrazeConfig::overgraze_escapement_fraction`].
const DEFAULT_GRAZE_OVERGRAZE_ESCAPEMENT_FRACTION: f32 = 0.25;

impl Default for GrazeConfig {
    fn default() -> Self {
        Self {
            // Deliberately **empty**. The per-`TerrainType` table is *data*, and its single authoritative copy is
            // `fauna_config.json` — duplicating it here would guarantee the two drift. A config whose
            // `graze` block omits (or under-fills) the table is *rejected* by [`FaunaConfig::validate`]
            // and the builtin — which has it — is used, so an incomplete table can never quietly
            // produce a map with no pasture on it.
            capacity_by_biome: HashMap::new(),
            ecology: EcologyConfig {
                regrowth_rate: DEFAULT_GRAZE_REGROWTH_RATE,
                ..EcologyConfig::default()
            },
            reseed_floor_fraction: DEFAULT_GRAZE_RESEED_FLOOR_FRACTION,
            overgraze_escapement_fraction: DEFAULT_GRAZE_OVERGRAZE_ESCAPEMENT_FRACTION,
        }
    }
}

/// Root fauna configuration.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct FaunaConfig {
    pub species: HashMap<String, SpeciesDef>,
    pub abundance: AbundanceConfig,
    pub hunt: HuntConfig,
    pub ecology: EcologyConfig,
    pub follow: FollowConfig,
    pub immigration: ImmigrationConfig,
    pub husbandry: HusbandryConfig,
    /// The per-biome graze (pasture) layer — see [`GrazeConfig`].
    pub graze: GrazeConfig,
    /// The dedicated predator pass + prey-limited ecology tuning (Predators Phase 1a) —
    /// see [`PredatorConfig`].
    pub predators: PredatorConfig,
}

impl GrazeConfig {
    /// Grazeable biomass a `terrain` tile carries at capacity. An **unknown** biome reads `0.0`, but
    /// [`FaunaConfig::validate`] guarantees the table is total over [`TerrainType::VALUES`], so on any
    /// loaded config this is a real lookup, never a silent default.
    pub fn capacity_for(&self, terrain: TerrainType) -> f32 {
        self.capacity_by_biome
            .get(&terrain)
            .copied()
            .unwrap_or(NO_GRAZE_CAPACITY)
    }
}

/// A biome that carries no animal-edible vegetation at all (open water, glacier, bare rock, lava,
/// salt flat). Named rather than bare so a `0.0` in the table reads as *"deliberately barren"* and a
/// `0.0` in code reads as *"the same thing"*, not as a fallback that lost its lookup.
pub const NO_GRAZE_CAPACITY: f32 = 0.0;

/// The largest a fraction-valued lever may be (`[0, 1]` / `(0, 1]` bounds in [`FaunaConfig::validate`]).
const MAX_FRACTION: f32 = 1.0;

/// The pen's **escapement point**, expressed in quarters of `K` — the managed harvest never takes the
/// herd below `K/2` (`fauna::managed_yield_biomass`), so `K/2 = 2/4 · K` is where a settled pen sits.
/// Not a tuning value: it is the MSY point of the logistic curve. It appears in the pen's
/// net-positive bound (below), whose derivation is:
///
/// At the settled operating point the herd stands at `K/2` **after** the keeper's take. The feed,
/// however, is charged on the biomass standing **before** it — `K/2 + r·K/4`, i.e. after that turn's
/// regrowth: you feed every animal in the pen, including the ones you are about to harvest. So
///
/// ```text
/// yield = r·K/4 · p            feed = u · (K/2 + r·K/4) = u · K·(2 + r)/4
/// net > 0  ⟺  u < r·p / (2 + r)
/// ```
///
/// (The idealised `u < r·p/2` ignores that the feed is charged post-regrowth, and is therefore a hair
/// *too loose* — it would admit a narrow band of upkeep values that are in fact a net loss.)
///
/// Since Grazing 2d the `r` in that bound is the **fastest** species' pen rate (§2.4) — the floor is a
/// best-case sanity check, not an every-species guarantee.
const PEN_ESCAPEMENT_QUARTERS: f32 = 2.0;

/// **The wariness at which the retreat stage is an exact identity** — no draw, no randomness
/// consumed, every engaged animal stays (`docs/plan_hunt_through_combat.md` §3). No roster row ships
/// it; it is what [`FaunaConfig::without_retreat`] installs to keep a deterministic harness
/// deterministic.
pub const NO_RETREAT: f32 = 0.0;

impl FaunaConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            Self::from_json_str(BUILTIN_FAUNA_CONFIG)
                .expect("builtin fauna config should parse and validate"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, FaunaConfigError> {
        let config: FaunaConfig = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

    /// **This roster with every species' `combat.wariness` held at `0`** — the retreat stage
    /// (`docs/plan_hunt_through_combat.md` §3) reduced to its exact identity, so a hunt take is a
    /// *deterministic* function of the crew, the floor and the fight again.
    ///
    /// # Why a shared helper rather than a per-suite pin
    ///
    /// Slice 7 authored a non-zero `wariness` on all 20 species, which makes every take on the
    /// animal web stochastic. The existing suite is this arc's **deterministic regression net**: a
    /// test carrying variance can no longer tell a real regression from a draw, which is the one
    /// thing it exists to do. So every pre-existing harness holds wariness at `0` and keeps pinning
    /// the numbers it pinned before, and the variance lives *only* in the tests written for it
    /// (`core_sim/tests/hunt_wariness.rs`).
    ///
    /// This is [`crate::fauna::animals_that_stay`]'s zero-identity used as a lever, and it is the
    /// same move `hunt_yield_vector::steady_quarry` already makes for `engage_rate` and `defense`
    /// — one more field, hoisted to a shared helper because eleven suites need it and a copy in each
    /// would drift.
    ///
    /// **It is not a general "make the hunt deterministic" switch**: the fight's own draw is
    /// `combat_config.hit_chance`, which ships at `1.0` and is already an identity.
    pub fn without_retreat(&self) -> Self {
        let mut config = self.clone();
        for def in config.species.values_mut() {
            def.combat.wariness = NO_RETREAT;
        }
        config
    }

    pub fn from_file(path: &Path) -> Result<Self, FaunaConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| FaunaConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        FaunaConfig::from_json_str(&contents)
    }

    /// Enforce the invariants that, if broken, would make the fauna model **silently incoherent**
    /// rather than merely differently-tuned. Runs inside [`FaunaConfig::from_json_str`], so **every**
    /// load path (builtin, default file, `FAUNA_CONFIG_PATH` override) is covered — the
    /// `expedition_config.rs` / `crisis_config.rs` convention. A broken invariant is logged at
    /// **error** level by [`load_fauna_config_from_env`] and the known-good builtin is used instead.
    ///
    /// The load-bearing one is **the pen's net-positive bound**: a pen whose feed costs more than its
    /// harvest yields is a *trap* — the player pays a 25-turn build and a permanent keeper to make
    /// their food situation strictly worse, with nothing in the UI to explain it. See
    /// [`DEFAULT_PEN_UPKEEP_PER_BIOMASS`].
    pub fn validate(&self) -> Result<(), FaunaConfigError> {
        // --- The migratory herd budget (issue #290). All three are integer *counts*, so the checks
        // are the degenerate readings rather than range bands: a `0` divide-by-zero density, a `0`
        // floor that leaves a small map with no migration at all, and an inverted clamp — which is the
        // specific failure this block exists to remove, since it would make `min_herds` unreachable
        // and hand the decision back to a silent default.
        let migratory = &self.abundance.migratory;
        if migratory.tiles_per_herd == 0 {
            return Err(FaunaConfigError::Invalid {
                field: "abundance.migratory.tiles_per_herd",
                constraint: "be greater than 0 (it is a divisor — tiles per migratory herd)".into(),
                value: migratory.tiles_per_herd.to_string(),
            });
        }
        if migratory.min_herds == 0 {
            return Err(FaunaConfigError::Invalid {
                field: "abundance.migratory.min_herds",
                constraint:
                    "be at least 1 (a map with no migratory herd has no migration to follow)".into(),
                value: migratory.min_herds.to_string(),
            });
        }
        if migratory.max_herds < migratory.min_herds {
            return Err(FaunaConfigError::Invalid {
                field: "abundance.migratory.max_herds",
                constraint: format!(
                    "be at least abundance.migratory.min_herds (= {}) — an inverted clamp makes the \
                     floor unreachable",
                    migratory.min_herds
                ),
                value: migratory.max_herds.to_string(),
            });
        }

        // --- Hunt: the biomass→provisions rate the WHOLE ladder is denominated in. At `0` every rung
        // (wild, pastoral, pen) pays nothing and the food economy silently stops.
        require_positive_finite(
            "hunt.provisions_per_biomass",
            self.hunt.provisions_per_biomass,
        )?;

        // NB: **the retired multiples-of-MSY ordering and the Surplus raid-floor bounds are GONE**
        // with the `HuntConfig` fields they constrained. A validate failure is a boot panic, so a
        // bound over a lever with no reader could only ever kill a server for a number that changed
        // nothing — see `HuntConfig`'s doc. The ordering they guaranteed ("each option takes more than
        // the last") is now a property of the continuous floor itself: a deeper floor leaves less
        // standing, by construction.

        // --- The three ecologies. `regrowth_rate` at `0` is a dead resource (no MSY, no regrowth);
        // the phase fractions must be ordered `extinction_floor < collapse < stressed < 1` or the
        // Thriving/Stressed/Collapsing classification is nonsense.
        validate_ecology("ecology", &self.ecology)?;
        validate_ecology(
            "husbandry.pastoral.ecology",
            &self.husbandry.pastoral.ecology,
        )?;
        validate_ecology("husbandry.pen.ecology", &self.husbandry.pen.ecology)?;

        // --- Per-species levers (Grazing Phase 2b-ii). A `regrowth_rate` present but non-positive is a
        // dead wild herd (no MSY, never grows); a negative/NaN `fodder_per_biomass` would make the
        // range draw-down and the range-derived `K` nonsense. Both are `#[serde(default)]`, so an older
        // config that omits them stays valid (fodder → 0.0 = non-grazing; regrowth → the global wild
        // rate). Iterated in stable key order so the error names a deterministic species.
        let mut species: Vec<(&String, &SpeciesDef)> = self.species.iter().collect();
        species.sort_by(|a, b| a.0.cmp(b.0));
        for (key, def) in species {
            // `"species.<key>.<leaf>"`, leaked to a `&'static str` like [`field`] (a fixed handful,
            // one per species per load — the config is loaded a bounded number of times).
            let species_field = |leaf: &str| -> &'static str {
                Box::leak(format!("species.{key}.{leaf}").into_boxed_str())
            };
            require_non_negative_finite(
                species_field("fodder_per_biomass"),
                def.fodder_per_biomass,
            )?;
            // **The hunt-yield vector** (`docs/plan_hunt_yield_model.md` §3). Only *present*
            // components are bounded — an omitted one is the global, already bounded above — and the
            // bound is `>= 0`, **not** `> 0`: a `0.0` is the meaningful "you do not eat me" (the wolf),
            // which is exactly what the `Option` exists to distinguish from "unset". A negative or
            // non-finite rate would pay a negative take or NaN the larder.
            if let Some(provisions) = def.hunt_yield.provisions_per_biomass {
                require_non_negative_finite(
                    species_field("hunt_yield.provisions_per_biomass"),
                    provisions,
                )?;
            }
            if let Some(regrowth_rate) = def.regrowth_rate {
                require_positive_finite(species_field("regrowth_rate"), regrowth_rate)?;
            }
            // The taming timescale (slice 3c). **Positive is the whole bound**: the multiplier dilates
            // the `animal:pastoral` rung's `work_cost`, and `build_decay` reads its bleed off that scaled cost, so the
            // ladder's own "taming must out-run its decay" check (`LadderConfig::validate`) already
            // covers every species — the ratio is invariant under a positive scale. At `0` the species
            // would silently never tame while reading as tameable; negative would *un*-tame a herd the
            // crew is working, and (via the same decay) push its progress up while it is abandoned.
            require_positive_finite(
                species_field("taming_cost_multiplier"),
                def.taming_cost_multiplier,
            )?;
            // At `0`/negative a managed herd of this species would demand infinitely many herders — it
            // could never be fully staffed, so every pastoral/penned herd would decay forever with no
            // way for the player to stop it. The dial's *upper* end is a tuning question (how much
            // waste the collection cap creates), not an invariant — measured, not rejected.
            require_positive_finite(species_field("animals_per_herder"), def.animals_per_herder)?;
            // **The animal quantum** (slice 8). Positive is the whole bound, and it is not
            // cosmetic: `quantise_animal_take` divides by this. At `0` a herd would hold infinitely
            // many animals and `floor(escapement / 0) = inf` would strip the whole stock in one
            // turn; negative would invert the floor and hand back a negative kill count.
            require_positive_finite(species_field("body_mass"), def.body_mass)?;
            // **The engagement throughput** (`plan_hunt_through_combat.md` §2). Positive is the whole
            // bound: at `0` no party of any size could ever reach the species, which is not a
            // balance choice but an unhuntable animal expressed as a typo; negative would hand back a
            // negative engagement and invert the take.
            require_positive_finite(species_field("engage_rate"), def.engage_rate)?;
            // **The husbandry density gains** — the per-rung K multiplier (`>= 1.0`). A gain **below 1**
            // would mean domestication *reduces* the land's carrying capacity, inverting the whole point
            // of the dial; `1.0` is neutral (a wild/untagged species). Both `#[serde(default)]` to 1.0,
            // so an older config that omits them stays valid.
            require_at_least_one(species_field("pastoral_density"), def.pastoral_density)?;
            require_at_least_one(species_field("pen_density"), def.pen_density)?;
            // **The intrinsic combat body** (Predators Phase 0, `docs/plan_predators.md`). `attack` may
            // be `0` (most prey just runs — a harmless hunt), and so may **`defense`**: it is the hard
            // gate `max(0, attack − defense)` (`docs/plan_hunt_through_combat.md` §4.2), and `0` is the
            // meaningful statement *"no protection at all"* that the five small-game rows carry — the
            // whole of why a bare-handed band can still take a rabbit. It appears in the kill/wound
            // split's denominator too, where `resolve_fight` already guards the `0/0` a harmless
            // attacker would otherwise produce. `aggression` is a `[0, 1]` probability of initiating a
            // raid. All finite — a NaN would poison the fight arithmetic.
            require_non_negative_finite(species_field("combat.attack"), def.combat.attack)?;
            require_non_negative_finite(species_field("combat.defense"), def.combat.defense)?;
            // **Durability is a DENOMINATOR** (`units_down = damage / durability`, §4.2), so `0` would
            // turn a single point of damage into every animal in the engagement — a species wiped out
            // by one hunter — and negative would hand back a negative kill count. Unlike `defense`
            // there is no coherent zero: a body that soaks nothing is not "unprotected", it is absent.
            require_positive_finite(species_field("combat.durability"), def.combat.durability)?;
            require_in_unit_range(species_field("aggression"), def.aggression)?;
            // `ferocity` is a probability (fights back vs flees), so the same `[0, 1]` bound as
            // `aggression`. It scales the animal's effective attack in the hunt-casualty adapters.
            require_in_unit_range(species_field("ferocity"), def.ferocity)?;
            // `combat.wariness` is a probability too (breaks off at contact vs stays to be fought),
            // and `CombatStats` is `#[serde(default)]`, so the JSON can author anything here.
            // `fauna::animals_that_stay` clamps an out-of-range value with `.min(1.0)`, which hides an
            // authoring slip — and a **NaN** is worse than hidden: `NaN <= 0.0` is false so the
            // wariness-`0` early return is skipped, and `NaN.min(1.0)` is `1.0` in Rust, so every
            // engaged animal retreats and the species' take is **silently zero** on every hunt.
            require_in_unit_range(species_field("combat.wariness"), def.combat.wariness)?;
            // **Carnivore coherence** (Predators Phase 1a, `docs/plan_predators.md`). A carnivore's
            // carrying capacity is `Σ prey_flow / prey_per_biomass`, so `prey_per_biomass` is a
            // denominator and must be `> 0` (a `0` yields infinite `K`). And a carnivore whose `attack`
            // clears no defense has an empty prey set at every biomass — an incoherent predator — so its
            // `combat.attack` must be `> 0` too (the general `combat.attack >= 0` bound above admits `0`
            // for a fleeing herbivore, which a carnivore may not be). Herbivores keep `prey_per_biomass`
            // at its inert `0.0`.
            if def.diet == Diet::Carnivore {
                require_positive_finite(species_field("prey_per_biomass"), def.prey_per_biomass)?;
                require_positive_finite(
                    species_field("combat.attack (carnivore)"),
                    def.combat.attack,
                )?;
                // A carnivore's pack count is `round(eligible_prey_herds × prey_ratio)`, so a `0`
                // (or negative/non-finite) ratio seats no packs at all — an incoherent predator.
                require_positive_finite(species_field("prey_ratio"), def.prey_ratio)?;
            }
            // **The shore predicate is only applied on the short-range game path.** The migratory
            // placement path (`suitable_tiles_for` / `build_migratory_route`) picks its loiter
            // anchors off `host_biomes` alone and never consults the site rule, so a migratory
            // species asking for adjacent water would have that request **silently ignored** — it
            // would spawn inland and nothing would say why. Make the unhandled state
            // unrepresentable and loud rather than quietly wrong.
            if def.migratory && def.adjacent_water.is_required() {
                return Err(FaunaConfigError::Invalid {
                    field: species_field("adjacent_water"),
                    constraint:
                        "not be combined with `migratory: true` — the migratory placement path does \
                         not apply the site rule, so the water requirement would be silently ignored"
                            .to_string(),
                    value: def.adjacent_water.as_str().to_string(),
                });
            }
        }

        // --- The ladder is MONOTONE, now as GAINS (Grazing 2d §3): management buys a *multiple* of the
        // species' own wild `r`, so each rung grows faster than the one below it for **every** species.
        // Invert this and penning a herd would *lower* its yield. `pastoral_gain > 1` (management must
        // beat wild growth); `pen_gain > pastoral_gain` (the pen tops the ladder); the cap is a live
        // positive rate (the stable-band ceiling the gains clamp to).
        require_greater_than(
            "husbandry.pastoral_gain",
            self.husbandry.pastoral_gain,
            "1.0 (management must beat wild growth)",
            MAX_FRACTION,
        )?;
        require_greater_than(
            "husbandry.pen_gain",
            self.husbandry.pen_gain,
            "husbandry.pastoral_gain",
            self.husbandry.pastoral_gain,
        )?;
        require_positive_finite(
            "husbandry.husbandry_regrowth_cap",
            self.husbandry.husbandry_regrowth_cap,
        )?;
        // `pen_radius_max` at `0` would forbid every `ExtendPen` (2d-β) — the command could never grow a
        // pen past its single tile, silently disabling the mechanic.
        if self.husbandry.pen_radius_max < 1 {
            return Err(FaunaConfigError::Invalid {
                field: "husbandry.pen_radius_max",
                constraint: "be at least 1 (a 0 cap forbids every pen extension)".to_string(),
                value: self.husbandry.pen_radius_max.to_string(),
            });
        }
        // The herder-hysteresis deadband is a fraction of `animals_per_herder` — finite & non-negative
        // (`0` disables the deadband, restoring the raw stateless flicker; negative is nonsense).
        require_non_negative_finite(
            "husbandry.herders_hysteresis_fraction",
            self.husbandry.herders_hysteresis_fraction,
        )?;

        // --- The neglect-escape shed rates (`docs/plan_fauna_neglect_escape.md` §3.4). Each is a
        // fraction of the overage that walks off per turn, so both must be finite & non-negative; the
        // jitter band likewise (`0` = an exactly-constant rate). The load-bearing invariant is
        // `pen < pastoral` — the fence must slow the shed, and stating it here makes "a pen that leaks
        // faster than open range" unrepresentable rather than a silent misconfiguration.
        require_non_negative_finite(
            "husbandry.pastoral_escape_fraction",
            self.husbandry.pastoral_escape_fraction,
        )?;
        require_non_negative_finite(
            "husbandry.pen_escape_fraction",
            self.husbandry.pen_escape_fraction,
        )?;
        require_non_negative_finite(
            "husbandry.escape_fraction_jitter",
            self.husbandry.escape_fraction_jitter,
        )?;
        require_greater_than(
            "husbandry.pastoral_escape_fraction",
            self.husbandry.pastoral_escape_fraction,
            "husbandry.pen_escape_fraction (the fence must slow the shed)",
            self.husbandry.pen_escape_fraction,
        )?;

        // --- The pen's feed. A shrink rate above 1 would drive an underfed herd's biomass *negative* in
        // one turn; below 0 it would *grow* a starving herd.
        require_in_unit_range(
            "husbandry.pen.starve_shrink_rate",
            self.husbandry.pen.starve_shrink_rate,
        )?;
        require_non_negative_finite(
            "husbandry.pen.upkeep_per_biomass",
            self.husbandry.pen.upkeep_per_biomass,
        )?;
        // **THE PEN MUST NOT BE A TRAP — a BEST-CASE floor (Grazing 2d §2.4).** With per-species pen `r`
        // and pasture-dependent feed, the old "every pen nets positive" guarantee no longer models the
        // system (it would reject slow-breeder worlds outright), and a slow breeder on poor pasture
        // running at a loss is now a player's bad placement, **not** a config error. So we require only
        // that the **fastest-breeding** species stays net-positive even when *fully larder-fed* (worst
        // pasture): at the operating point a pen yields `r·K/4 · p` and eats `u · K·(2 + r)/4`, so it
        // nets positive iff `u < r_pen · p / (2 + r_pen)` for `r_pen = min(cap, max_wild_r × pen_gain)`
        // (see [`PEN_ESCAPEMENT_QUARTERS`]). Shipped: `0.002 < 0.75 × 0.02 / 2.75 ≈ 0.0055` ✓. A
        // violating override would make **even the best pen** a permanent net food LOSS.
        let fastest_pen_r = (self.max_wild_regrowth_rate() * self.husbandry.pen_gain)
            .min(self.husbandry.husbandry_regrowth_cap);
        let net_positive_bound = fastest_pen_r * self.hunt.provisions_per_biomass
            / (PEN_ESCAPEMENT_QUARTERS + fastest_pen_r);
        if self.husbandry.pen.upkeep_per_biomass >= net_positive_bound {
            return Err(FaunaConfigError::Invalid {
                field: "husbandry.pen.upkeep_per_biomass",
                constraint: format!(
                    "be less than r_pen × hunt.provisions_per_biomass / (2 + r_pen) (= \
                     {net_positive_bound}), where r_pen is the FASTEST species' pen rate \
                     min(husbandry_regrowth_cap, max_wild_r × pen_gain) — otherwise even the best pen \
                     costs more feed than its harvest yields"
                ),
                value: self.husbandry.pen.upkeep_per_biomass.to_string(),
            });
        }

        // --- (Husbandry's *build* dials — the pen's rate and its investment dip — are bounded by
        // `LadderConfig::validate`, which owns the `animal:pen` rung's `build` block; so are the
        // **earned-knowledge** dials as of slice 4, which moved to the ladder's `knowledge` block
        // when the earn path became one rung-driven seam. Both bounds now hold for BOTH webs from a
        // single statement, instead of each web restating its own copy.)

        // --- Follow / immigration (ported from the builtin-only unit assertions). The `market`
        // block's trade multiplier is **retired** — see the `MarketConfig` gravestone above.
        require_in_unit_range(
            "immigration.chance_per_turn",
            self.immigration.chance_per_turn,
        )?;

        // --- The graze (pasture) layer. Same ecology invariants as every other rung; plus the two
        // that make the *table* trustworthy.
        validate_ecology("graze.ecology", &self.graze.ecology)?;
        validate_graze(&self.graze)?;

        // --- The dedicated predator pass + prey-limited ecology (Predators Phase 1a).
        validate_predators(&self.predators)?;

        Ok(())
    }

    /// The **fastest wild breeding rate** across the species table — each species' own `regrowth_rate`
    /// (or the global wild rate for a row that omits it), folded with `f32::max` and seeded from the
    /// global rate so an empty table falls back to it. The best-case input to the pen's net-positive
    /// floor (Grazing 2d §2.4): the fastest species is the one that must stay profitable.
    fn max_wild_regrowth_rate(&self) -> f32 {
        self.species
            .values()
            .map(|def| def.regrowth_rate_or(self.ecology.regrowth_rate))
            .fold(self.ecology.regrowth_rate, f32::max)
    }

    /// `(key, def)` pairs for every migratory species, in a stable key order.
    pub fn migratory_species(&self) -> Vec<(&String, &SpeciesDef)> {
        let mut out: Vec<_> = self
            .species
            .iter()
            .filter(|(_, def)| def.migratory)
            .collect();
        out.sort_by(|a, b| a.0.cmp(b.0));
        out
    }

    /// Resolve a species row by its `display_name` (the value a `Herd` stores in `species`), so
    /// `advance_herds` can read the herd's movement cadence levers. Display names are unique.
    pub fn species_by_display(&self, display: &str) -> Option<&SpeciesDef> {
        self.species
            .values()
            .find(|def| def.display_name == display)
    }

    /// **The species' taming cost multiplier** ([`SpeciesDef::taming_cost_multiplier`]), resolved by
    /// the display name a `Herd` carries — the same live-resolution path the movement cadence levers
    /// take (`fauna::advance_herds` → [`FaunaConfig::species_by_display`]), so retuning the dial takes
    /// effect on herds already on the map instead of freezing at spawn. A species the table cannot
    /// resolve (an isolated test fixture) reads [`DEFAULT_TAMING_COST_MULTIPLIER`] — the rung's own
    /// price, i.e. exactly the pre-dial behaviour.
    pub fn taming_cost_multiplier_for(&self, display: &str) -> f32 {
        self.species_by_display(display)
            .map_or(DEFAULT_TAMING_COST_MULTIPLIER, |def| {
                def.taming_cost_multiplier
            })
    }

    /// **The animals one herder of this species minds** ([`SpeciesDef::animals_per_herder`]), resolved
    /// by the display name a `Herd` carries — the [`FaunaConfig::taming_cost_multiplier_for`] path, so retuning
    /// the dial reaches herds already on the map instead of freezing at spawn. A species the table
    /// cannot resolve (an isolated test fixture) reads [`DEFAULT_ANIMALS_PER_HERDER`].
    pub fn animals_per_herder_for(&self, display: &str) -> f32 {
        self.species_by_display(display)
            .map_or(DEFAULT_ANIMALS_PER_HERDER, |def| def.animals_per_herder)
    }

    /// **The probability an animal of this species breaks off at contact**
    /// ([`crate::combat::CombatStats::wariness`]), resolved by display name — the
    /// [`FaunaConfig::taming_cost_multiplier_for`] path. An unresolvable species reads `0.0`: no retreat, which
    /// is the identity, and the honest reading of a fixture the roster does not describe.
    pub fn wariness_for(&self, display: &str) -> f32 {
        self.species_by_display(display)
            .map_or(0.0, |def| def.combat.wariness)
    }

    /// **The animals one hunter of this species can bring into contact per turn**
    /// ([`SpeciesDef::engage_rate`]), resolved by the display name a `Herd` carries — the
    /// [`FaunaConfig::taming_cost_multiplier_for`] path, so retuning the dial reaches herds already on the map.
    ///
    /// **A species the table cannot resolve reads [`f32::INFINITY`] — no engagement bound at all**,
    /// not a small number. The unresolvable case is an isolated test fixture, and the honest reading
    /// of "this herd is not in the roster" is *the engagement stage has nothing to say about it*,
    /// which leaves such a fixture's take exactly as it was before this arc. A finite default would
    /// silently cap fixtures at a number nobody chose.
    pub fn engage_rate_for(&self, display: &str) -> f32 {
        self.species_by_display(display)
            .map_or(f32::INFINITY, |def| def.engage_rate)
    }

    /// **The quarry's side of a hunt fight** ([`crate::fauna::QuarryFight`] — its combat body plus the
    /// `ferocity` that decides whether it fights back), resolved by display name through the
    /// [`FaunaConfig::taming_cost_multiplier_for`] path. **THE seam** every take and forecast path resolves the
    /// fight's quarry through, so none of them can assemble a different animal.
    ///
    /// An unresolvable species (an isolated test fixture) reads [`crate::combat::CombatStats`]'s
    /// default with `ferocity 0` — a harmless body at the neutral `durability 1`, which is the honest
    /// reading of *"the roster does not describe this herd"* and matches what
    /// [`FaunaConfig::wariness_for`] already answers for the same case.
    ///
    /// **It answers the SPECIES, so the wounds come back empty** — an un-hunted animal. A path with a
    /// live herd in hand resolves through [`crate::fauna::herd_quarry_fight`] instead, which carries
    /// [`crate::fauna::Herd::wounds`] into the fight.
    pub fn quarry_fight_for(&self, display: &str) -> crate::fauna::QuarryFight {
        self.species_by_display(display).map_or(
            crate::fauna::QuarryFight {
                profile: CombatStats::default(),
                ferocity: 0.0,
                wounds: crate::combat::DamageLedger::default(),
            },
            |def| crate::fauna::QuarryFight {
                profile: def.combat,
                ferocity: def.ferocity,
                wounds: crate::combat::DamageLedger::default(),
            },
        )
    }

    /// **The species' resolved hunt-yield vector** ([`SpeciesDef::hunt_yield`]) — *what* a take of this
    /// species pays, per unit of biomass. **THE single seam**: no call site may read
    /// `hunt.provisions_per_biomass` for a *take* directly, because the `None ⇒ global` fallback must
    /// be stated exactly once (a second copy is how a wolf starts paying meat on one path and nothing
    /// on another).
    ///
    /// Resolved **live** by display name, the [`FaunaConfig::taming_cost_multiplier_for`] path, so a retune reaches
    /// herds already on the map and it needs no snapshot field.
    ///
    /// An **unknown species key is a config bug, not a runtime case** — but a test fixture may legally
    /// carry a synthetic species name, and the sibling resolvers all document that case as reading the
    /// default. So an unresolvable name falls back to the globals (a release build still pays something
    /// rather than silently zeroing a herd's yield) and is **not** a panic.
    pub fn hunt_yield_for(&self, display: &str) -> HuntYield {
        let configured = self.species_by_display(display).map(|def| &def.hunt_yield);
        HuntYield {
            provisions_per_biomass: configured
                .and_then(|def| def.provisions_per_biomass)
                .unwrap_or(self.hunt.provisions_per_biomass),
            // **Derived here, at the same seam, so the two halves of "is this worth hunting" cannot
            // be resolved from different rows.** Unlike the rate above there is no global to fall
            // back to, so an unresolvable species reads *no materials* — the same answer an omitted
            // list gives.
            yields_materials: configured.is_some_and(|def| !def.materials.is_empty()),
        }
    }

    /// **The species' MATERIAL yield rows** ([`HuntYieldDef::materials`]) — what the carcass is made
    /// of, per unit of biomass carried home.
    ///
    /// A separate seam from [`Self::hunt_yield_for`] because there is no global to resolve against:
    /// the two rate components fall back to `hunt.*`, and a material list has nothing to fall back
    /// **to** — an unlisted species yields no material, which is a real answer rather than a gap. An
    /// unresolvable name yields none, matching the sibling resolvers' fixture-tolerant behaviour.
    pub fn hunt_materials_for(
        &self,
        display: &str,
    ) -> &[crate::materials_config::MaterialYieldDef] {
        self.species_by_display(display)
            .map(|def| def.hunt_yield.materials.as_slice())
            .unwrap_or_default()
    }

    /// Reconcile every species' material yield with the materials table — the cross-config half of
    /// `validate`, run by [`load_fauna_config_from_env`] with the loaded table passed in so it has
    /// exactly one copy. See [`crate::materials_config::MaterialsConfig::validate_yield`].
    pub fn validate_against_materials(
        &self,
        materials: &crate::materials_config::MaterialsConfig,
    ) -> Result<(), crate::materials_config::MaterialYieldError> {
        for (key, def) in &self.species {
            materials.validate_yield(
                &format!("species.{key}.hunt_yield"),
                &def.hunt_yield.materials,
            )?;
        }
        Ok(())
    }

    /// **The species' pastoral density gain** ([`SpeciesDef::pastoral_density`]), resolved by the
    /// display name a `Herd` carries — the [`FaunaConfig::taming_cost_multiplier_for`] path, so retuning the dial
    /// reaches herds already on the map instead of freezing at spawn. A species the table cannot resolve
    /// (an isolated test fixture) reads [`DEFAULT_HUSBANDRY_DENSITY`] (neutral, `×1.0`).
    pub fn pastoral_density_for(&self, display: &str) -> f32 {
        self.species_by_display(display)
            .map_or(DEFAULT_HUSBANDRY_DENSITY, |def| def.pastoral_density)
    }

    /// **The species' pen density gain** ([`SpeciesDef::pen_density`]), resolved by display name — the
    /// [`FaunaConfig::pastoral_density_for`] path. An unresolvable species reads
    /// [`DEFAULT_HUSBANDRY_DENSITY`].
    pub fn pen_density_for(&self, display: &str) -> f32 {
        self.species_by_display(display)
            .map_or(DEFAULT_HUSBANDRY_DENSITY, |def| def.pen_density)
    }

    /// `(key, def)` pairs for every non-migratory (short-range) **herbivore** game species that
    /// hosts in `module_key`, in a stable key order.
    ///
    /// **Carnivores are excluded** (Predators Phase 1a): the short-range spawn pool and the
    /// immigration path both draw from here, and a predator must not seat from the herbivore budget or
    /// immigrate — it seeds **only** via the dedicated `fauna::spawn_predators` pass (once; if prey
    /// collapse it dies out and does not respawn — idea 6).
    pub fn game_species_for_biome(&self, module_key: &str) -> Vec<(&String, &SpeciesDef)> {
        let mut out: Vec<_> = self
            .species
            .iter()
            .filter(|(_, def)| {
                !def.migratory && def.diet == Diet::Herbivore && def.hosts_biome(module_key)
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(b.0));
        out
    }

    /// `(key, def)` pairs for every non-migratory **carnivore** species that hosts in `module_key`, in
    /// a stable key order — the predator twin of [`FaunaConfig::game_species_for_biome`], drawn from by
    /// the dedicated `fauna::spawn_predators` pass.
    pub fn carnivore_species_for_biome(&self, module_key: &str) -> Vec<(&String, &SpeciesDef)> {
        let mut out: Vec<_> = self
            .species
            .iter()
            .filter(|(_, def)| {
                !def.migratory && def.diet == Diet::Carnivore && def.hosts_biome(module_key)
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(b.0));
        out
    }

    /// `(key, def)` pairs for every non-migratory **carnivore** species (all biomes), in a stable key
    /// order — the roster [`fauna::spawn_predators`] sizes its per-species prey-derived pack targets
    /// over.
    pub fn carnivore_species(&self) -> Vec<(&String, &SpeciesDef)> {
        let mut out: Vec<_> = self
            .species
            .iter()
            .filter(|(_, def)| !def.migratory && def.diet == Diet::Carnivore)
            .collect();
        out.sort_by(|a, b| a.0.cmp(b.0));
        out
    }
}

/// The graze table's own invariants — the ones that decide whether the **land layer** is trustworthy.
///
/// - **Totality.** The table must name every `TerrainType` (`TerrainType::VALUES`). A missing row silently reads
///   `0.0` ([`NO_GRAZE_CAPACITY`]) — an invisible dead zone in the pasture layer that no error, no
///   log line and no overlay would ever explain. Zero must be *stated*, never *defaulted*.
/// - **At least one positive row.** An all-zero table disables the entire layer (no herd could be
///   fed anywhere) while parsing perfectly — exactly the class of "silently turns a feature off"
///   lever this validation exists to catch.
/// - **`reseed_floor_fraction` below `collapse_fraction`.** The floor exists to stop *permanent*
///   death, not to hide overgrazing: at or above the collapse band a stripped pasture would be lifted
///   straight back into a healthier phase every turn, and the ecology phase (and the client's
///   overgrazing warning) would never be able to read Collapsing.
fn validate_graze(graze: &GrazeConfig) -> Result<(), FaunaConfigError> {
    let mut positive_rows = 0usize;
    for terrain in TerrainType::VALUES {
        let Some(&capacity) = graze.capacity_by_biome.get(&terrain) else {
            return Err(FaunaConfigError::Invalid {
                field: "graze.capacity_by_biome",
                constraint: format!(
                    "name every one of the {} biomes (missing {terrain:?}); an absent biome silently \
                     reads as zero graze",
                    TerrainType::VALUES.len()
                ),
                value: format!("{} rows", graze.capacity_by_biome.len()),
            });
        };
        if !capacity.is_finite() || capacity < NO_GRAZE_CAPACITY {
            return Err(FaunaConfigError::Invalid {
                field: "graze.capacity_by_biome",
                constraint: format!("be finite and at least {NO_GRAZE_CAPACITY} for every biome"),
                value: format!("{terrain:?} = {capacity}"),
            });
        }
        if capacity > NO_GRAZE_CAPACITY {
            positive_rows += 1;
        }
    }
    if positive_rows == 0 {
        return Err(FaunaConfigError::Invalid {
            field: "graze.capacity_by_biome",
            constraint: "give at least one biome a positive capacity, or there is no pasture \
                         anywhere on any map"
                .to_string(),
            value: "every biome is 0".to_string(),
        });
    }

    require_in_unit_range("graze.reseed_floor_fraction", graze.reseed_floor_fraction)?;
    require_greater_than(
        "graze.ecology.collapse_fraction",
        graze.ecology.collapse_fraction,
        "graze.reseed_floor_fraction",
        graze.reseed_floor_fraction,
    )?;

    // The overgrazing escapement floor (2b-ii): a real escapement above the reseed lift, and below the
    // graze MSY point (0.5·cap) so overgrazing is still possible/visible. Outside this band it is either
    // useless (≤ reseed floor → the crash-prevention it exists for is gone) or degenerate (≥ 0.5 → no
    // overgrazing can ever happen; a range is pinned at its most-productive intensity forever).
    require_in_unit_range(
        "graze.overgraze_escapement_fraction",
        graze.overgraze_escapement_fraction,
    )?;
    require_greater_than(
        "graze.overgraze_escapement_fraction",
        graze.overgraze_escapement_fraction,
        "graze.reseed_floor_fraction",
        graze.reseed_floor_fraction,
    )?;
    require_greater_than(
        "the graze MSY point (0.5)",
        GRAZE_MSY_BIOMASS_FRACTION,
        "graze.overgraze_escapement_fraction",
        graze.overgraze_escapement_fraction,
    )?;
    Ok(())
}

/// The graze's MSY biomass fraction (`cap/2`) — mirrors `fauna::MSY_BIOMASS_FRACTION` (the logistic
/// peak), named here so the escapement-floor bound reads against the concept, not a bare `0.5`.
const GRAZE_MSY_BIOMASS_FRACTION: f32 = 0.5;

/// The dedicated predator pass's invariants (Predators Phase 1a). The pack count is prey-derived
/// (`round(eligible_prey_herds × prey_ratio)`, validated on the carnivore's own row), so there is no
/// `max_packs` cap to check here; the four block dials that can break: a `0` spacing would stack packs
/// on one tile; a `0` sensing disk would contain no prey (prey are sparse points) so `K` would always
/// be `0`; the functional-response taper must sit above zero and below the prey's MSY point (the band
/// `graze.overgraze_escapement_fraction` also lives in); and every per-biome probability is a `[0, 1]`
/// chance.
fn validate_predators(predators: &PredatorConfig) -> Result<(), FaunaConfigError> {
    if predators.min_spacing < 1 {
        return Err(FaunaConfigError::Invalid {
            field: "predators.min_spacing",
            constraint: "be at least 1 (a 0 spacing would stack packs on one tile)".to_string(),
            value: predators.min_spacing.to_string(),
        });
    }
    if !predators.predation_escapement_fraction.is_finite()
        || predators.predation_escapement_fraction <= 0.0
        || predators.predation_escapement_fraction >= crate::fauna::MSY_BIOMASS_FRACTION
    {
        return Err(FaunaConfigError::Invalid {
            field: "predators.predation_escapement_fraction",
            constraint: format!(
                "be finite and in (0, {}) — above zero, below the prey's MSY point (the taper stops \
                 predation before it strips a prey herd to nothing)",
                crate::fauna::MSY_BIOMASS_FRACTION
            ),
            value: predators.predation_escapement_fraction.to_string(),
        });
    }
    if predators.prey_sense_radius < 1 {
        return Err(FaunaConfigError::Invalid {
            field: "predators.prey_sense_radius",
            constraint: "be at least 1 (a 0-radius disk senses no prey, so K_pred is always 0)"
                .to_string(),
            value: predators.prey_sense_radius.to_string(),
        });
    }
    if predators.pursuit_radius < 1 {
        return Err(FaunaConfigError::Invalid {
            field: "predators.pursuit_radius",
            constraint: "be at least 1 (a 0-radius acquisition disk senses no prey, so a wild \
                         carnivore can never pursue and just roams)"
                .to_string(),
            value: predators.pursuit_radius.to_string(),
        });
    }
    if predators.raid_radius < 1 {
        return Err(FaunaConfigError::Invalid {
            field: "predators.raid_radius",
            constraint: "be at least 1 (a 0-radius raid can never reach any band, so the trigger \
                         never fires)"
                .to_string(),
            value: predators.raid_radius.to_string(),
        });
    }
    if !predators.raid_exposure.is_finite() || predators.raid_exposure <= 0.0 {
        return Err(FaunaConfigError::Invalid {
            field: "predators.raid_exposure",
            constraint:
                "be finite and > 0 (a 0 exposure leaves no defender-side populace, so a raid \
                         can kill no one and the trigger is inert)"
                    .to_string(),
            value: predators.raid_exposure.to_string(),
        });
    }
    require_in_unit_range(
        "predators.raid_yield_forfeit_fraction",
        predators.raid_yield_forfeit_fraction,
    )?;
    // Every per-biome probability finite in `[0, 1]`, iterated in stable key order for a deterministic
    // error message (the `species` loop convention).
    let mut per_biome: Vec<(&String, &f32)> = predators.per_biome.iter().collect();
    per_biome.sort_by(|a, b| a.0.cmp(b.0));
    for (module, prob) in per_biome {
        let field = Box::leak(format!("predators.per_biome.{module}").into_boxed_str());
        require_in_unit_range(field, *prob)?;
    }
    Ok(())
}

/// Every ecology block (wild / pastoral / pen — and each is a full [`EcologyConfig`]) shares the same
/// invariants: a live growth rate, and phase thresholds ordered `extinction_floor < collapse_fraction
/// < stressed_fraction < 1` so `classify_ecology_phase` can actually separate the three bands.
fn validate_ecology(prefix: &'static str, ecology: &EcologyConfig) -> Result<(), FaunaConfigError> {
    // A `0` regrowth rate is a dead resource: MSY is 0, so every rung of the ladder that reads this
    // ecology silently pays nothing forever.
    require_positive_finite(field(prefix, "regrowth_rate"), ecology.regrowth_rate)?;
    require_positive_finite(field(prefix, "collapse_rate"), ecology.collapse_rate)?;
    require_in_unit_range(field(prefix, "extinction_floor"), ecology.extinction_floor)?;
    require_in_unit_range(
        field(prefix, "collapse_fraction"),
        ecology.collapse_fraction,
    )?;
    require_in_unit_range(
        field(prefix, "stressed_fraction"),
        ecology.stressed_fraction,
    )?;
    require_greater_than(
        field(prefix, "collapse_fraction"),
        ecology.collapse_fraction,
        field(prefix, "extinction_floor"),
        ecology.extinction_floor,
    )?;
    require_greater_than(
        field(prefix, "stressed_fraction"),
        ecology.stressed_fraction,
        field(prefix, "collapse_fraction"),
        ecology.collapse_fraction,
    )?;
    require_greater_than(
        "1.0 (a resource cannot be 'stressed' at capacity)",
        MAX_FRACTION,
        field(prefix, "stressed_fraction"),
        ecology.stressed_fraction,
    )?;
    Ok(())
}

/// `"<prefix>.<leaf>"` as a `&'static str` — the ecology checks are run over three different blocks,
/// so the error must name *which* one. Leaked deliberately: there are a fixed handful of these, they
/// live for the process, and it keeps [`FaunaConfigError::Invalid`]'s `field` a cheap `&'static str`
/// (matching the `expedition_config.rs` convention) instead of forcing a `String` on every call site.
fn field(prefix: &'static str, leaf: &'static str) -> &'static str {
    Box::leak(format!("{prefix}.{leaf}").into_boxed_str())
}

fn require_positive_finite(field: &'static str, value: f32) -> Result<(), FaunaConfigError> {
    if !value.is_finite() || value <= 0.0 {
        return Err(FaunaConfigError::Invalid {
            field,
            constraint: "be finite and greater than 0".to_string(),
            value: value.to_string(),
        });
    }
    Ok(())
}

fn require_non_negative_finite(field: &'static str, value: f32) -> Result<(), FaunaConfigError> {
    if !value.is_finite() || value < 0.0 {
        return Err(FaunaConfigError::Invalid {
            field,
            constraint: "be finite and at least 0".to_string(),
            value: value.to_string(),
        });
    }
    Ok(())
}

/// `[0, 1]` — a fraction that may legitimately be zero (an off switch) or whole.
fn require_in_unit_range(field: &'static str, value: f32) -> Result<(), FaunaConfigError> {
    if !value.is_finite() || !(0.0..=MAX_FRACTION).contains(&value) {
        return Err(FaunaConfigError::Invalid {
            field,
            constraint: format!("be finite and in [0, {MAX_FRACTION}]"),
            value: value.to_string(),
        });
    }
    Ok(())
}

// NB: `require_fraction` — the `(0, 1]` bound — went with the earned-knowledge dials it was this
// config's only caller of (slice 4). It lives on as `intensification::validate_knowledge`'s
// `completion_threshold` check, which now states the bound once for both food webs.

// NB: `require_open_unit_fraction` — the strict `(0, 1)` bound — went with the proportional-skim
// dials it was the only caller of, and the multiples-of-MSY dials that briefly replaced them have
// since gone too (see `HuntConfig`). `require_greater_than` remains for the orderings that are still
// live, an ordering being a stronger statement than a range: a value cannot be individually "in
// range" yet out of order.

/// A **gain that must not shrink** the quantity it scales: finite and `>= 1.0`. A husbandry density
/// below 1 would make domestication *reduce* a herd's carrying capacity — the exact inversion the dial
/// exists to prevent (see [`SpeciesDef::pastoral_density`]). `1.0` is the neutral (wild) value.
fn require_at_least_one(field: &'static str, value: f32) -> Result<(), FaunaConfigError> {
    if !value.is_finite() || value < MAX_FRACTION {
        return Err(FaunaConfigError::Invalid {
            field,
            constraint: format!(
                "be finite and at least {MAX_FRACTION} (a density gain below 1 would make \
                 domestication reduce carrying capacity)"
            ),
            value: value.to_string(),
        });
    }
    Ok(())
}

/// A strict cross-field ordering (`value > other`) — the shape most of this config's real invariants
/// take (the monotone ladder, the ordered phase bands, accrual out-running decay).
fn require_greater_than(
    field: &'static str,
    value: f32,
    other_field: &'static str,
    other: f32,
) -> Result<(), FaunaConfigError> {
    if !value.is_finite() || value <= other {
        return Err(FaunaConfigError::Invalid {
            field,
            constraint: format!("be finite and greater than {other_field} (= {other})"),
            value: value.to_string(),
        });
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum FaunaConfigError {
    #[error("failed to read fauna config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse fauna config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid fauna config: `{field}` must {constraint}, got {value}")]
    Invalid {
        field: &'static str,
        constraint: String,
        value: String,
    },
    /// The cross-config half: a species' `hunt_yield.materials` row that the materials table refuses.
    #[error("invalid fauna config: {0}")]
    MaterialYield(#[from] crate::materials_config::MaterialYieldError),
}

impl ConfigLoadError for FaunaConfigError {
    /// Only a genuinely absent file is a benign absence; every other variant is a file that is
    /// there and wrong, which the boot loader refuses to paper over with the builtin.
    fn is_not_found(&self) -> bool {
        matches!(self, Self::Read { source, .. } if source.kind() == io::ErrorKind::NotFound)
    }
}

/// Handle for accessing the fauna configuration.
#[derive(Resource, Debug, Clone)]
pub struct FaunaConfigHandle(pub Arc<FaunaConfig>);

impl FaunaConfigHandle {
    pub fn new(config: Arc<FaunaConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<FaunaConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<FaunaConfig>) {
        self.0 = config;
    }

    /// **Hold the whole roster's `combat.wariness` at `0` in place** — the handle-side spelling of
    /// [`FaunaConfig::without_retreat`], which is how a harness that already has the world's
    /// resources in hand keeps its take deterministic in one line.
    pub fn hold_wariness_at_zero(&mut self) {
        self.0 = Arc::new(self.0.without_retreat());
    }
}

impl Default for FaunaConfigHandle {
    fn default() -> Self {
        Self(FaunaConfig::builtin())
    }
}

/// Metadata about the fauna configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct FaunaConfigMetadata {
    path: Option<PathBuf>,
}

impl FaunaConfigMetadata {
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

/// Load fauna configuration from environment (`FAUNA_CONFIG_PATH`) or the default data path.
///
/// The file goes through [`FaunaConfig::from_json_str`], so it is **validated** before it can reach
/// the sim: a config that would silently break the model (a pen that eats more than it yields, an
/// inverted husbandry ladder, an unreachable knowledge gate, …) is refused, and a broken invariant
/// is as fatal as a parse error — it looks live, which is exactly why it must not be swapped out
/// quietly.
///
/// The roster's **material** yield edge is reconciled against the materials table at the same time
/// ([`FaunaConfig::validate_against_materials`]), so a species naming a material that does not
/// exist — or stating a reading on an axis it does not have — is a boot panic rather than a herd
/// that silently yields nothing. The table is passed in rather than re-read so it keeps one copy,
/// exactly as [`crate::flora_config::load_flora_config_from_env`] takes the forage capacities.
///
/// Only an absent *default* path falls back to the builtin; a present-but-broken file, or a
/// `FAUNA_CONFIG_PATH` that names a missing or broken file, is a boot panic — see
/// [`crate::config_load::resolve_config`].
pub fn load_fauna_config_from_env(
    materials: &crate::materials_config::MaterialsConfig,
) -> (Arc<FaunaConfig>, FaunaConfigMetadata) {
    let (config, source) = load_config_from_env(
        "FAUNA_CONFIG_PATH",
        "fauna_config",
        "src/data/fauna_config.json",
        FaunaConfig::builtin,
        |path| -> Result<FaunaConfig, FaunaConfigError> {
            let config = FaunaConfig::from_file(path)?;
            config.validate_against_materials(materials)?;
            Ok(config)
        },
    );

    if source.is_none() {
        // The builtin is checked too: it is the fallback, so a materials table that drifted out from
        // under the roster leaves the same hole here and it must be loud. Deliberately not fatal —
        // unlike a file the operator edited, there is no alternative roster to point at, and
        // `builtin_config_parses` already pins the shipped pair.
        if let Err(err) = config.validate_against_materials(materials) {
            tracing::error!(
                target: "shadow_scale::config",
                error = %err,
                "fauna_config.builtin_material_yield_broken"
            );
        }
    }

    (config, FaunaConfigMetadata::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intensification::{LadderConfig, RungKey};

    #[test]
    fn builtin_config_parses() {
        let config = FaunaConfig::builtin();
        assert!(config.species.contains_key("deer"));
        assert!(config.species.contains_key("rabbit"));
        assert!(config.species.contains_key("mammoth"));
        // Display names must embed the client icon keyword.
        assert!(config.species["deer"]
            .display_name
            .to_lowercase()
            .contains("deer"));
        assert!(config.species["boar"]
            .display_name
            .to_lowercase()
            .contains("boar"));
    }

    /// Grazing 2d-δ: the shipped roster's husbandry ceilings, and the `pen` default for an omitted one.
    #[test]
    fn builtin_husbandry_ceilings_match_the_roster() {
        let config = FaunaConfig::builtin();
        use HusbandryCeiling::*;
        for (key, expected) in [
            ("mammoth", Wild),
            ("deer", Wild),
            ("steppe_runner", Pastoral),
            ("marsh_grazer", Pastoral),
            ("boar", Pen),
            ("rabbit", Pen),
            ("fowl", Pen),
        ] {
            assert_eq!(
                config.species[key].husbandry_ceiling, expected,
                "{key} husbandry_ceiling"
            );
        }
        // An omitted field defaults to `pen` (the full ladder), preserving pre-δ behaviour.
        // `body_mass` is REQUIRED (slice 8) — a species with no quantum is not a species, so it must
        // fail to parse rather than default to something.
        let def: SpeciesDef = serde_json::from_str(
            r#"{"display_name":"X","route_len":[1,1],"biomass":[1,1],"body_mass":1,"engage_rate":1}"#,
        )
        .unwrap();
        assert_eq!(def.husbandry_ceiling, HusbandryCeiling::Pen);
    }

    /// The shipped per-species taming **costs**, and the `1.0` default for an omitted one. The
    /// **work units** each implies is what the roster is really claiming, so assert that — a dial read
    /// back as a number nobody can interpret is not a guard.
    #[test]
    fn builtin_taming_costs_match_the_roster() {
        let config = FaunaConfig::builtin();
        let ladder = LadderConfig::builtin();
        let pastoral = ladder.rung(RungKey::AnimalPastoral);

        for (key, multiplier, work_units) in [
            ("rabbit", 1.0_f32, 50.0_f32),
            ("fowl", 1.0, 50.0),
            ("crag_goat", 1.0, 50.0),
            ("boar", 1.25, 62.5),
            ("aurochs", 2.0, 100.0),
            ("steppe_runner", 5.0, 250.0),
            ("marsh_grazer", 5.0, 250.0),
        ] {
            let def = &config.species[key];
            assert_eq!(
                def.taming_cost_multiplier, multiplier,
                "{key} taming_cost_multiplier"
            );
            assert_eq!(
                pastoral.build_cost(def.taming_cost_multiplier),
                Some(work_units),
                "{key} should cost ~{work_units} work units to tame"
            );
        }
        // A `wild`-ceiling species never tames at all, so it states no cost (and reads the default).
        for key in ["deer", "mammoth"] {
            assert_eq!(
                config.species[key].husbandry_ceiling,
                HusbandryCeiling::Wild
            );
            assert_eq!(
                config.species[key].taming_cost_multiplier,
                DEFAULT_TAMING_COST_MULTIPLIER
            );
        }
        // An omitted field costing the rung's own price is what keeps an untagged/future species on
        // today's pacing.
        // `body_mass` is REQUIRED (slice 8) — a species with no quantum is not a species, so it must
        // fail to parse rather than default to something.
        let def: SpeciesDef = serde_json::from_str(
            r#"{"display_name":"X","route_len":[1,1],"biomass":[1,1],"body_mass":1,"engage_rate":1}"#,
        )
        .unwrap();
        assert_eq!(def.taming_cost_multiplier, DEFAULT_TAMING_COST_MULTIPLIER);
        // And an unresolvable species reads the same, so a fixture herd can never tame for free.
        assert_eq!(
            config.taming_cost_multiplier_for("No Such Beast"),
            DEFAULT_TAMING_COST_MULTIPLIER
        );
    }

    /// A `taming_cost_multiplier` of `0` reads as "tameable" everywhere (the ceiling still says
    /// `pastoral`) while the job costs nothing — the herd tames the instant any crew touches it,
    /// which is the silent-disable failure mode config validation exists to catch. A negative one is
    /// meaningless.
    #[test]
    fn validate_rejects_a_non_positive_taming_cost() {
        for bad in [0.0, -0.2] {
            let err =
                reject(|json| json["species"]["rabbit"]["taming_cost_multiplier"] = (bad).into());
            assert_rejects_field(err, "species.rabbit.taming_cost_multiplier");
        }
    }

    /// **A husbandry density below 1 makes domestication REDUCE the land's carrying capacity** — the
    /// exact inversion the dial exists to prevent (a tamed goat's range would hold *fewer* goats than a
    /// wild one). One rejection per bound; the neutral `1.0` and the shipped gains stay valid.
    #[test]
    fn validate_rejects_a_pastoral_density_below_one() {
        for bad in [0.99, 0.0, -1.0] {
            let err =
                reject(|json| json["species"]["crag_goat"]["pastoral_density"] = (bad).into());
            assert_rejects_field(err, "species.crag_goat.pastoral_density");
        }
        assert!(FaunaConfig::builtin().validate().is_ok());
    }

    #[test]
    fn validate_rejects_a_pen_density_below_one() {
        for bad in [0.99, 0.0, -1.0] {
            let err = reject(|json| json["species"]["crag_goat"]["pen_density"] = (bad).into());
            assert_rejects_field(err, "species.crag_goat.pen_density");
        }
    }

    /// The density gains default to the neutral `1.0` (a wild/untagged species is unchanged) and
    /// resolve live by display name — the `taming_cost_multiplier_for` path, so a retune reaches herds on the map.
    #[test]
    fn husbandry_density_defaults_to_neutral_and_resolves_live() {
        let config = FaunaConfig::builtin();
        // A row that omits both dials reads the neutral gain.
        let def: SpeciesDef = serde_json::from_str(
            r#"{"display_name":"X","route_len":[1,1],"biomass":[1,1],"body_mass":1,"engage_rate":1}"#,
        )
        .unwrap();
        assert_eq!(def.pastoral_density, DEFAULT_HUSBANDRY_DENSITY);
        assert_eq!(def.pen_density, DEFAULT_HUSBANDRY_DENSITY);
        // The prime grazer domesticate carries the big pen bump; an unresolvable species is neutral.
        assert_eq!(config.pastoral_density_for("Crag Goats"), 2.0);
        assert_eq!(config.pen_density_for("Crag Goats"), 5.0);
        assert_eq!(
            config.pen_density_for("No Such Beast"),
            DEFAULT_HUSBANDRY_DENSITY
        );
    }

    /// **A `body_mass` of `0` is a herd of infinitely many animals** — `floor(escapement / 0)` is
    /// `inf`, so the first hunter would strip the whole stock in one turn while every readout still
    /// looked sane. A negative one inverts the floor and hands back a negative kill count. Neither is
    /// a tuning choice; both are the silent-catastrophe failure mode validation exists to catch.
    #[test]
    fn validate_rejects_a_non_positive_body_mass() {
        for bad in [0.0, -50.0] {
            let err = reject(|json| json["species"]["rabbit"]["body_mass"] = (bad).into());
            assert_rejects_field(err, "species.rabbit.body_mass");
        }
    }

    #[test]
    fn migratory_and_game_partitions() {
        let config = FaunaConfig::builtin();
        let migratory = config.migratory_species();
        assert!(migratory.iter().all(|(_, def)| def.migratory));
        assert!(migratory.iter().any(|(k, _)| k.as_str() == "mammoth"));

        // Deer hosts in temperate forest and is short-range game.
        let forest_game = config.game_species_for_biome("temperate_forest");
        assert!(forest_game.iter().any(|(k, _)| k.as_str() == "deer"));
        assert!(forest_game.iter().all(|(_, def)| !def.migratory));
    }

    #[test]
    fn abundance_probability_clamps() {
        let config = FaunaConfig::builtin();
        assert!(config.abundance.probability_for("temperate_forest") > 0.0);
        assert_eq!(config.abundance.probability_for("deep_ocean"), 0.0);
    }

    /// The levers `validate()` deliberately does NOT bound (they have coherent meanings at their
    /// extremes) — everything else moved into the validator, which every load path now runs
    /// (`builtin()` would panic below if the shipped config broke one).
    ///
    /// **The `take_from` clamp assertions are gone with the function**: Eradicate takes the whole
    /// standing stock now, no dial — it is the `floor = 0` case of
    /// `fauna::hunt_escapement_ceiling`.
    #[test]
    fn hunt_and_ecology_present() {
        let config = FaunaConfig::builtin();
        assert_eq!(config.hunt.pursuit_radius, 1);
        assert!(config.immigration.max_attempts >= 1);
        assert!(config.follow.reveal_radius >= 1);
    }

    /// **THE HUNT BLOCK'S SHAPE, ASSERTED AT COMPILE TIME.** `harvest_floor_trade_rebalance::the_
    /// deleted_levers_are_gone_and_the_allee_threshold_is_not` reads the shipped JSON, so it can only
    /// ever say a *key* is absent — and `#[serde(default)]` fills a field whose key is gone, silently.
    /// That is exactly how `surplus_multiplier`, `deplete_multiplier` and
    /// `surplus_escapement_fraction` outlived their own key deletion: struct fields with no reader,
    /// still defaulted, still **validated**, so a `FAUNA_CONFIG_PATH` file naming one could panic the
    /// server at boot over a number that changed nothing.
    ///
    /// An exhaustive destructuring is the guard a value assertion cannot be: re-adding a field to
    /// [`HuntConfig`] fails to **compile** here (*"pattern does not mention field"*), before it can
    /// grow a default, a bound or a reader. A new field is welcome — it just has to be named here,
    /// which is the moment to ask what reads it.
    #[test]
    fn the_hunt_block_carries_no_take_multiplier() {
        let HuntConfig {
            provisions_per_biomass: _,
            pursuit_radius: _,
            pursuit_tiles_per_turn: _,
            max_pursuit_turns: _,
        } = FaunaConfig::builtin().hunt;
    }

    // NB: `the_shipped_hunt_multipliers_are_ordered`, `validate_rejects_a_surplus_multiplier_at_or_
    // below_one`, `validate_rejects_a_deplete_multiplier_at_or_below_surplus` and the two
    // `validate_rejects_a_surplus_escapement_*` cases went with the `HuntConfig` fields they
    // exercised. They asserted an ordering among dials nothing read; what they were standing in for
    // — *"a deeper draw leaves a leaner source"* — is now a property of the escapement expression
    // itself and is covered on the TAKE (`fauna_deplete::hunt_policy_takes_are_strictly_ordered_at_
    // every_biomass`), which is the shipped representation.

    /// **Every species declares a positive body mass** — the quantum a hunt take is floored to
    /// (slice 8). A missing/zero row would mean a herd of infinitely many animals; `validate()`
    /// rejects it, and `builtin()` would panic here if the shipped table ever lost one.
    #[test]
    fn every_species_declares_a_body_mass() {
        let config = FaunaConfig::builtin();
        for (key, def) in &config.species {
            assert!(
                def.body_mass.is_finite() && def.body_mass > 0.0,
                "species {key} must declare a positive body_mass, got {}",
                def.body_mass
            );
            // A body cannot outweigh the whole herd's capacity, or the species could never be hunted
            // at all (`floor(escapement / body_mass)` would be 0 even at full capacity).
            assert!(
                def.body_mass < def.carrying_capacity(),
                "species {key}'s body_mass {} must be below its carrying capacity {}",
                def.body_mass,
                def.carrying_capacity()
            );
        }
    }

    /// The shipped ladder is monotone (management buys a growth rate) and the pen nets positive at its
    /// operating point — the two invariants the whole arc rests on, asserted on the *shipped* numbers.
    #[test]
    fn builtin_husbandry_ladder_is_monotone_and_the_pen_pays() {
        let config = FaunaConfig::builtin();
        // The ladder is monotone as GAINS now (Grazing 2d): pastoral beats wild, pen tops pastoral.
        assert!(
            config.husbandry.pen_gain > config.husbandry.pastoral_gain
                && config.husbandry.pastoral_gain > 1.0,
            "1.0 < {} < {}",
            config.husbandry.pastoral_gain,
            config.husbandry.pen_gain
        );
        // Best-case floor: the FASTEST species' pen rate must still net positive when fully larder-fed.
        let fastest_pen_r = (config.max_wild_regrowth_rate() * config.husbandry.pen_gain)
            .min(config.husbandry.husbandry_regrowth_cap);
        let bound = fastest_pen_r * config.hunt.provisions_per_biomass
            / (PEN_ESCAPEMENT_QUARTERS + fastest_pen_r);
        assert!(
            config.husbandry.pen.upkeep_per_biomass < bound,
            "the shipped pen must net positive for the fastest breeder: {} < {bound}",
            config.husbandry.pen.upkeep_per_biomass
        );
    }

    /// Mutate the builtin, re-serialize, and re-load it through `from_json_str` — the *only* entry
    /// point, so this exercises the same validation every load path (builtin/file/env override) runs.
    fn reject(mutate: impl FnOnce(&mut serde_json::Value)) -> FaunaConfigError {
        let mut json: serde_json::Value =
            serde_json::from_str(BUILTIN_FAUNA_CONFIG).expect("builtin parses");
        mutate(&mut json);
        FaunaConfig::from_json_str(&json.to_string())
            .expect_err("a broken invariant must be rejected")
    }

    fn assert_rejects_field(err: FaunaConfigError, expected: &str) {
        match err {
            FaunaConfigError::Invalid { field, .. } => assert_eq!(field, expected),
            other => panic!("expected an Invalid error for {expected}, got {other:?}"),
        }
    }

    /// **The per-species hunt-yield rate is REJECTED when it is not a rate**
    /// (`docs/plan_hunt_yield_model.md` §3), in the `validate_rejects_a_non_positive_body_mass`
    /// style, because a negative or non-finite rate would pay a negative take or NaN the larder and
    /// neither shows up as a compile error.
    #[test]
    fn validate_rejects_a_negative_species_hunt_yield_provisions() {
        let err = reject(|json| {
            json["species"]["deer"]["hunt_yield"] =
                serde_json::json!({ "provisions_per_biomass": -0.01 })
        });
        assert_rejects_field(err, "species.deer.hunt_yield.provisions_per_biomass");
    }

    /// **ZERO IS LEGAL, and it is the whole point** — it is how a wolf says *"you do not eat me"*, and
    /// the reason the component is `Option<f32>` rather than a float with a `0` sentinel. A
    /// `> 0` bound here would make the inedible species unrepresentable.
    ///
    /// **And the wolf is still worth hunting**, on its hide and its bone: `yields_nothing` counts
    /// materials since arc #527, so retiring the trade axis did not prune every rung but denial off
    /// the one shipped species that reads `0` food.
    #[test]
    fn a_zero_hunt_yield_component_is_accepted_and_reads_as_inedible() {
        let config = FaunaConfig::builtin();
        assert!(config.validate().is_ok());
        let wolf = config.hunt_yield_for("Grey Wolf Pack");
        assert_eq!(wolf.provisions_per_biomass, 0.0);
        assert!(!wolf.edible(), "a wolf is not food");
        assert!(wolf.yields_materials, "a wolf is a pelt and a bone");
        assert!(!wolf.yields_nothing());
    }

    /// **A species worth neither meat nor material is the ONE degenerate case**, and no shipped row
    /// is it — pinned on a synthetic species so the branch is exercised rather than assumed, the
    /// treatment `hunt_yield_vector.rs` gives the picker rule it feeds.
    #[test]
    fn a_species_with_no_food_and_no_materials_yields_nothing() {
        let pest = HuntYield {
            provisions_per_biomass: 0.0,
            yields_materials: false,
        };
        assert!(pest.yields_nothing());
        let config = FaunaConfig::builtin();
        for (key, def) in &config.species {
            let resolved = config.hunt_yield_for(&def.display_name);
            assert!(
                !resolved.yields_nothing(),
                "shipped species '{key}' is worth neither meat nor material"
            );
        }
    }

    /// **An omitted block falls back to the global rate**, which is the property that keeps every
    /// species but the wolf byte-identical across the yield-vector arc.
    #[test]
    fn an_omitted_hunt_yield_reads_the_globals() {
        let config = FaunaConfig::builtin();
        let deer = config.hunt_yield_for("Red Deer");
        assert_eq!(
            deer.provisions_per_biomass,
            config.hunt.provisions_per_biomass
        );
        // An unresolvable name (a synthetic test fixture) reads the same global rather than zeroing
        // a herd's yield — the `taming_cost_multiplier_for`/`animals_per_herder_for` contract. It carries no
        // materials, because there is no global list to fall back to.
        let unknown = config.hunt_yield_for("No Such Beast");
        assert_eq!(unknown.provisions_per_biomass, deer.provisions_per_biomass);
        assert!(!unknown.yields_materials);
    }

    /// **`apply` converts the whole take in one call** — the property that makes it impossible to
    /// convert one account and forget another across ~20 readout sites.
    #[test]
    fn apply_scales_every_component_by_the_take_and_the_output_multiplier() {
        let hy = HuntYield {
            provisions_per_biomass: 0.02,
            yields_materials: true,
        };
        let paid = hy.apply(100.0, 2.0);
        assert!((paid.provisions - 4.0).abs() < 1e-6, "{paid:?}");
        // An animal pays no fodder — a structural zero, not an unprojected gap.
        assert_eq!(paid.fodder, 0.0, "{paid:?}");
        assert_eq!(hy.apply(0.0, 1.0), YieldAccounts::default());
    }

    /// `tiles_per_herd` is a **divisor** (`area / tiles_per_herd`), so a `0` is the one value with no
    /// sensible reading at all.
    #[test]
    fn validate_rejects_a_zero_migratory_tiles_per_herd() {
        let err = reject(|json| json["abundance"]["migratory"]["tiles_per_herd"] = (0).into());
        assert_rejects_field(err, "abundance.migratory.tiles_per_herd");
    }

    /// A `0` floor lets a small map hold **no** migratory herd at all — no migration to follow, on a
    /// map that still shows the biomes those species host.
    #[test]
    fn validate_rejects_a_zero_migratory_min_herds() {
        let err = reject(|json| json["abundance"]["migratory"]["min_herds"] = (0).into());
        assert_rejects_field(err, "abundance.migratory.min_herds");
    }

    /// **The load-bearing one for this block.** An inverted clamp makes `min_herds` unreachable and
    /// hands the per-map count back to a silent ceiling — precisely the "one clamp quietly decides
    /// everything" failure (issue #290) that promoting these literals into config exists to remove.
    #[test]
    fn validate_rejects_an_inverted_migratory_herd_clamp() {
        let err = reject(|json| {
            json["abundance"]["migratory"]["min_herds"] = (6).into();
            json["abundance"]["migratory"]["max_herds"] = (5).into();
        });
        assert_rejects_field(err, "abundance.migratory.max_herds");
    }

    /// The shipped budget puts the standard 80×52 map at **5** migratory herds — one slot per migratory
    /// row, so each row's *expected* count is 1 and presence per species is `1 − (4/5)^5 ≈ 67%`. Pinned
    /// because it is the number issue #290 measured, and because the previous value's real defect was
    /// that the density was **inert** at the shipped size (4160/3000 = 1, clamp-floored to 2) — a
    /// regression here would be silent again.
    #[test]
    fn the_standard_map_budgets_one_migratory_slot_per_roster_row() {
        let config = FaunaConfig::builtin();
        let rows = config.migratory_species().len();
        assert_eq!(rows, 5, "the shipped migratory roster is 5 rows");
        assert_eq!(
            config.abundance.migratory.herds_for_map(80, 52),
            rows as u32,
            "the standard map should budget one migratory slot per migratory row"
        );
    }

    /// The clamps bind at both ends, and the density — not a clamp — decides the shipped size. The
    /// retired `area/3000` formula failed exactly this: every real map sat on a clamp.
    #[test]
    fn the_migratory_budget_clamps_at_both_ends_but_scales_between_them() {
        let migratory = &FaunaConfig::builtin().abundance.migratory;
        // Tiny map → the floor binds.
        assert_eq!(migratory.herds_for_map(10, 10), migratory.min_herds);
        // Huge map → the ceiling binds.
        assert_eq!(migratory.herds_for_map(256, 192), migratory.max_herds);
        // Between them the density is the authority, and it is monotone in area.
        assert!(
            migratory.herds_for_map(80, 52) > migratory.min_herds
                && migratory.herds_for_map(80, 52) < migratory.max_herds,
            "the shipped map size must sit strictly inside the clamps, or the density is inert"
        );
        assert!(migratory.herds_for_map(120, 80) >= migratory.herds_for_map(80, 52));
    }

    /// **The load-bearing one.** A pen whose feed costs more than its harvest yields is a trap: the
    /// player pays a 25-turn build + a permanent keeper to make their food situation strictly worse.
    #[test]
    fn validate_rejects_a_pen_that_eats_more_than_it_yields() {
        // Best-case floor (Grazing 2d §2.4): r_pen(fastest) = min(1.0, 0.35 × 4.0) = 1.0, so the
        // bound is 1.0 × 0.02 / 3.0 ≈ 0.0067; at or above it EVEN THE BEST pen is a net loss.
        let err = reject(|json| json["husbandry"]["pen"]["upkeep_per_biomass"] = (0.007).into());
        assert_rejects_field(err, "husbandry.pen.upkeep_per_biomass");
        let err = reject(|json| json["husbandry"]["pen"]["upkeep_per_biomass"] = (0.008).into());
        assert_rejects_field(err, "husbandry.pen.upkeep_per_biomass");
        // The shipped value has ample room inside the bound.
        assert!(FaunaConfig::builtin().validate().is_ok());
    }

    /// A negative shed rate would *add* animals to an under-contained herd (`docs/plan_fauna_neglect_escape.md`
    /// §3.4). Each of the three neglect-escape dials must be finite & non-negative.
    #[test]
    fn validate_rejects_a_negative_pastoral_escape_fraction() {
        for bad in [-0.01, -1.0] {
            let err = reject(|json| json["husbandry"]["pastoral_escape_fraction"] = (bad).into());
            assert_rejects_field(err, "husbandry.pastoral_escape_fraction");
        }
    }

    #[test]
    fn validate_rejects_a_negative_pen_escape_fraction() {
        for bad in [-0.01, -1.0] {
            let err = reject(|json| json["husbandry"]["pen_escape_fraction"] = (bad).into());
            assert_rejects_field(err, "husbandry.pen_escape_fraction");
        }
    }

    #[test]
    fn validate_rejects_a_negative_escape_fraction_jitter() {
        for bad in [-0.01, -1.0] {
            let err = reject(|json| json["husbandry"]["escape_fraction_jitter"] = (bad).into());
            assert_rejects_field(err, "husbandry.escape_fraction_jitter");
        }
    }

    /// **The fence must slow the shed** (`docs/plan_fauna_neglect_escape.md` §3.4): a pen that leaks at
    /// or above the open-range rate is unrepresentable. The check fires on the pastoral field (it is the
    /// one required to be the greater).
    #[test]
    fn validate_rejects_a_pen_escape_at_or_above_the_pastoral_rate() {
        // Equal to the pastoral rate (0.25) — not strictly slower.
        let err = reject(|json| json["husbandry"]["pen_escape_fraction"] = (0.25).into());
        assert_rejects_field(err, "husbandry.pastoral_escape_fraction");
        // Strictly faster than open range — the inversion the invariant forbids.
        let err = reject(|json| json["husbandry"]["pen_escape_fraction"] = (0.30).into());
        assert_rejects_field(err, "husbandry.pastoral_escape_fraction");
        assert!(FaunaConfig::builtin().validate().is_ok());
    }

    /// The ladder must be monotone in `r`: a pen that grows no faster than the pastoral rung would
    /// pay *less* than it (it also carries feed), inverting the whole intensification incentive.
    #[test]
    fn validate_rejects_an_inverted_husbandry_ladder() {
        // The ladder is monotone as GAINS now (Grazing 2d): a pen that grows no faster than the
        // pastoral rung inverts the incentive.
        let err = reject(|json| json["husbandry"]["pen_gain"] = (1.2).into());
        assert_rejects_field(err, "husbandry.pen_gain");
        // Management must beat wild growth, or taming is a downgrade.
        let err = reject(|json| json["husbandry"]["pastoral_gain"] = (0.9).into());
        assert_rejects_field(err, "husbandry.pastoral_gain");
    }

    #[test]
    fn validate_rejects_a_dead_ecology() {
        let err = reject(|json| json["ecology"]["regrowth_rate"] = (0.0).into());
        assert_rejects_field(err, "ecology.regrowth_rate");
        let err =
            reject(|json| json["husbandry"]["pen"]["ecology"]["regrowth_rate"] = (0.0).into());
        // The pen ecology block still carries the shared phase bands, so a `0` regrowth trips
        // `validate_ecology` (its `regrowth_rate` must be a live rate, even though the *managed* growth
        // rate is now per-species and does not read it).
        assert!(matches!(err, FaunaConfigError::Invalid { .. }));
    }

    #[test]
    fn validate_rejects_unordered_ecology_phase_bands() {
        let err = reject(|json| json["ecology"]["stressed_fraction"] = (0.10).into());
        assert_rejects_field(err, "ecology.stressed_fraction");
        let err = reject(|json| json["ecology"]["extinction_floor"] = (0.50).into());
        assert_rejects_field(err, "ecology.collapse_fraction");
    }

    #[test]
    fn validate_rejects_an_out_of_range_starve_rate() {
        let err = reject(|json| json["husbandry"]["pen"]["starve_shrink_rate"] = (1.5).into());
        assert_rejects_field(err, "husbandry.pen.starve_shrink_rate");
    }

    // The pen's *build* dials moved to the ladder — their rejection tests moved with them, to
    // `crate::intensification`'s `rejects_a_free_investment` / `rejects_a_starving_investment` /
    // `rejects_a_non_building_progress_rate`.

    // NB: the earned-knowledge dials moved to the ladder in slice 4 (both webs' copies were
    // identical once the earn path became one rung-driven seam), and so did this rejection test —
    // `intensification::tests::rejects_a_ladder_nobody_could_ever_learn` /
    // `rejects_a_knowledge_gate_that_is_open_or_shut_from_the_start` now assert the bound **once**,
    // for both food webs, instead of each web guarding its own copy.

    // NB: "taming must out-run its own decay" is still guarded — it moved to
    // `intensification::tests::rejects_taming_that_cannot_outrun_its_decay` along with the dials
    // themselves (the `animal:pastoral` rung's `build` block), where `LadderConfig::validate` now
    // owns the bound for *every* rung of *both* food webs rather than each web re-asserting it.

    #[test]
    fn validate_rejects_a_zero_provisions_rate() {
        // The rate the WHOLE ladder is denominated in: at `0` every rung silently pays nothing.
        let err = reject(|json| json["hunt"]["provisions_per_biomass"] = (0.0).into());
        assert_rejects_field(err, "hunt.provisions_per_biomass");
    }

    /// A rejected override must fall back to the **known-good builtin**, never disable the model.
    #[test]
    fn an_invalid_override_falls_back_to_the_builtin() {
        let dir = std::env::temp_dir().join("shadow_scale_fauna_config_validate");
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("trap_pen.json");
        let mut json: serde_json::Value =
            serde_json::from_str(BUILTIN_FAUNA_CONFIG).expect("builtin parses");
        json["husbandry"]["pen"]["upkeep_per_biomass"] = (10.0).into();
        fs::write(&path, json.to_string()).expect("write override");

        assert!(
            FaunaConfig::from_file(&path).is_err(),
            "the trap pen is refused"
        );
        // The builtin is still loadable and sane — the sim keeps running on it.
        let builtin = FaunaConfig::builtin();
        assert!(builtin.validate().is_ok());
    }

    #[test]
    fn size_class_round_trips() {
        assert_eq!(SizeClass::Big.as_str(), "big");
        assert_eq!(SizeClass::Migratory.as_str(), "migratory");
    }

    /// The graze table must be **total** over every `TerrainType`. A missing row would silently read as
    /// zero graze — an invisible dead zone in the pasture layer that nothing would ever explain.
    /// A `raid_radius` of 0 means a raid can never reach any band — the whole trigger is inert.
    #[test]
    fn validate_rejects_a_zero_raid_radius() {
        let err = reject(|json| json["predators"]["raid_radius"] = (0).into());
        assert_rejects_field(err, "predators.raid_radius");
    }

    /// A `pursuit_radius` of 0 means a wild carnivore acquires no prey, so `pursue` degrades to a
    /// plain roam — the transient-zero-prey stranding this dial exists to fix.
    #[test]
    fn validate_rejects_a_zero_pursuit_radius() {
        let err = reject(|json| json["predators"]["pursuit_radius"] = (0).into());
        assert_rejects_field(err, "predators.pursuit_radius");
    }

    /// A non-positive `raid_exposure` leaves no defender-side populace, so a raid can kill nobody.
    #[test]
    fn validate_rejects_a_non_positive_raid_exposure() {
        let err = reject(|json| json["predators"]["raid_exposure"] = (0.0).into());
        assert_rejects_field(err, "predators.raid_exposure");
    }

    /// The raid yield-forfeit is a FRACTION of the band's food income (Predators Phase 3): a value
    /// below 0 or above 1 is out of range (0 = a raid costs only people; 1 = the whole turn's income).
    #[test]
    fn validate_rejects_an_out_of_range_raid_yield_forfeit_fraction() {
        for bad in [-0.01, 1.01] {
            let err =
                reject(|json| json["predators"]["raid_yield_forfeit_fraction"] = (bad).into());
            assert_rejects_field(err, "predators.raid_yield_forfeit_fraction");
        }
        assert!(FaunaConfig::builtin().validate().is_ok());
    }

    #[test]
    fn validate_rejects_a_partial_graze_biome_table() {
        let err = reject(|json| {
            json["graze"]["capacity_by_biome"]
                .as_object_mut()
                .expect("table")
                .remove("PrairieSteppe");
        });
        assert_rejects_field(err, "graze.capacity_by_biome");
    }

    /// An all-zero table parses perfectly and disables the entire layer — no pasture anywhere, on any
    /// map. Exactly the "silently turns a feature off" class of lever validation exists to catch.
    #[test]
    fn validate_rejects_an_all_zero_graze_table() {
        let err = reject(|json| {
            let table = json["graze"]["capacity_by_biome"]
                .as_object_mut()
                .expect("table");
            for value in table.values_mut() {
                *value = (0.0).into();
            }
        });
        assert_rejects_field(err, "graze.capacity_by_biome");
    }

    #[test]
    fn validate_rejects_a_negative_graze_capacity() {
        let err =
            reject(|json| json["graze"]["capacity_by_biome"]["PrairieSteppe"] = (-1.0).into());
        assert_rejects_field(err, "graze.capacity_by_biome");
    }

    /// A dead graze ecology (`r = 0`) means grass never regrows — every pasture is a one-shot stock
    /// and, from Phase 2b, every herd starves.
    #[test]
    fn validate_rejects_a_dead_graze_ecology() {
        let err = reject(|json| json["graze"]["ecology"]["regrowth_rate"] = (0.0).into());
        assert_rejects_field(err, "graze.ecology.regrowth_rate");
    }

    /// The reseed floor stops *permanent* death; it must not hide overgrazing. At or above
    /// `collapse_fraction` a stripped pasture is lifted back into a healthier band every turn and the
    /// Collapsing phase (and the client's overgrazing warning) becomes unreachable.
    #[test]
    fn validate_rejects_a_reseed_floor_that_hides_overgrazing() {
        let err = reject(|json| json["graze"]["reseed_floor_fraction"] = (0.5).into());
        assert_rejects_field(err, "graze.ecology.collapse_fraction");
    }

    /// The shipped table's model claims, asserted rather than assumed: open grassland is pasture,
    /// closed-canopy forest is not, and water/ice/rock carry nothing at all.
    #[test]
    fn builtin_graze_table_is_total_and_sane() {
        let config = FaunaConfig::builtin();
        let graze = &config.graze;
        assert_eq!(graze.capacity_by_biome.len(), TerrainType::VALUES.len());
        let prairie = graze.capacity_for(TerrainType::PrairieSteppe);
        assert!(prairie > 0.0);
        assert!(prairie > graze.capacity_for(TerrainType::MixedWoodland));
        assert!(prairie > graze.capacity_for(TerrainType::Tundra));
        assert_eq!(
            graze.capacity_for(TerrainType::DeepOcean),
            NO_GRAZE_CAPACITY
        );
        assert_eq!(graze.capacity_for(TerrainType::Glacier), NO_GRAZE_CAPACITY);
        assert!(graze.reseed_floor_fraction < graze.ecology.collapse_fraction);
    }
}
