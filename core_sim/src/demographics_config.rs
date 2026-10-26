//! Data-driven tuning for the demographic population model.
//!
//! Loaded from `data/demographics_config.json`. Each `PopulationCohort` carries three age
//! brackets (children / working-age / elders) plus a local food larder; `simulate_population`
//! (see `systems.rs`) draws per-capita food each turn, then resolves scarcity/cold deaths,
//! births, maturation, aging, and elder mortality from these rates. Mirrors the
//! `sedentarization_config.rs` / `fauna_config.rs` loader (baked-in builtin + optional
//! file/env override).
//!
//! **The JSON is the sole source of demographics tuning** (issue #350). There are no hand-written
//! Rust defaults to drift out of sync with it: every field is required, unknown keys are rejected,
//! and `DemographicsConfig::default()` parses [`BUILTIN_DEMOGRAPHICS_CONFIG`]. A missing or
//! misspelled key is a loud parse error, not a silent fallback to a second set of numbers.
//!
//! The **loader** is strict to match, via the shared [`crate::config_load`] seam: a
//! present-but-broken file, or a `DEMOGRAPHICS_CONFIG_PATH` that is missing or broken, is fatal at
//! boot. Only an *absent default path* falls back to the builtin — otherwise the strict schema
//! would merely move the silent substitution from one key out to the whole file.

use std::{
    fs, io,
    num::{NonZeroU16, NonZeroU32},
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

use crate::config_load::{load_config_from_env, ConfigLoadError};

pub const BUILTIN_DEMOGRAPHICS_CONFIG: &str = include_str!("data/demographics_config.json");

/// Fractions (summing to ~1.0) that split a freshly spawned cohort's head-count into the three
/// age brackets.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemographicsDistribution {
    pub children: f32,
    pub working: f32,
    pub elders: f32,
}

/// Per-turn food draw. `demand = per_capita_draw × (children·child_factor + working·working_factor
/// + elders·elder_factor)`; the per-bracket factors let dependents eat less than a working adult.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemographicsConsumption {
    pub per_capita_draw: f32,
    pub child_factor: f32,
    pub working_factor: f32,
    pub elder_factor: f32,
}

/// Campaign-start seeding. Each freshly spawned band starts with `food_reserve_days` turns of
/// its own food demand carried in its larder (food is band-local from day one — no faction pool)
/// and a `well_fed_morale_bonus` for opening the game provisioned.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemographicsStartup {
    pub food_reserve_days: f32,
    pub well_fed_morale_bonus: f32,
}

/// The **stock** fertility factor: how deep is the larder. `reserve = 1 + bonus ×
/// min(reserve_turns / saturation_turns, 1)`, where `reserve_turns` is the post-meal larder
/// measured in turns of demand. `saturation_turns = 1.0` reproduces the retired hardcoded
/// behaviour (a two-turn buffer read as maximum surplus); the shipped 10.0 means a band must bank
/// roughly a season to earn the full bonus. See `docs/plan_population_growth_model.md`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemographicsReserve {
    /// Maximum fertility bonus from a full larder (the retired `births.surplus_bonus`).
    pub bonus: f32,
    /// Turns of banked demand that earn the full `bonus`.
    pub saturation_turns: f32,
}

/// The **flow** fertility factor: is the larder growing or shrinking. Two-sided and centred at 1.0
/// — net-positive food raises fertility, net-negative lowers it — driven by
/// `net_ratio = (steady_income − demand) / demand`, the negation of the same net drain the
/// player-facing `turnsOfFood` runway divides by (a pen eats grass and hay, so neither term counts
/// livestock). See
/// `docs/plan_population_growth_model.md`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemographicsTrend {
    /// Maximum fertility bonus from net-positive food.
    pub surplus_gain: f32,
    /// Net surplus (as a multiple of demand) that earns the full `surplus_gain`.
    pub surplus_saturation: f32,
    /// Maximum fertility penalty from net-negative food, and **the damp-vs-stop lever**: `0.75`
    /// leaves a fully-collapsed band breeding at 25% of base (starvation mortality stays the real
    /// consequence of a deficit), while **`1.0` lets negative flow stop growth outright**. See
    /// `docs/plan_population_growth_model.md` §2.4.
    pub deficit_penalty: f32,
    /// Net deficit (as a multiple of demand) that reaches the full `deficit_penalty`. `1.0` means
    /// the penalty maxes out when the net flow is a full turn's demand in the red — that is at zero
    /// income for a band with no pens, and *sooner* for one whose pens also eat from the larder.
    pub deficit_saturation: f32,
}

/// Birth tuning. `births = birth_rate × working × hunger × reserve × trend`, added to children — a
/// **product of named factors** mirroring `output_multiplier`'s modifier stack, so adding a future
/// fertility driver is one entry rather than a rewrite of the birth path.
///
/// `hunger` (the food the band actually ate over what it wanted) is a **gate**: it alone reaches 0,
/// so an empty larder yields zero births. `reserve` ∈ `[1, 1+bonus]` and `trend` ∈
/// `[1−deficit_penalty, 1+surplus_gain]` are **modifiers** around 1.0 — neither can zero the
/// product on its own, which is why the stack needs no separate floor lever.
///
/// Births are **morale-independent** (Civilization Wellbeing model, `docs/plan_civ_wellbeing.md`):
/// contentment doesn't change procreation — low morale relocates people or drags output, never
/// suppresses births.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemographicsBirths {
    pub birth_rate: f32,
    pub reserve: DemographicsReserve,
    pub trend: DemographicsTrend,
}

/// Starvation tuning. When food demand outruns the larder, each bracket loses
/// `deficit_fraction × starvation_mortality × <bracket>_vulnerability` of its head-count per turn
/// (dependents typically more vulnerable than working-age) — but never more than the deficit
/// itself, so a 10% food shortfall impacts at most 10% of a bracket. Keep `starvation_mortality`
/// well below 1 so shortfalls bleed the population down over several turns rather than in one.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemographicsScarcity {
    pub starvation_mortality: f32,
    pub child_vulnerability: f32,
    pub working_vulnerability: f32,
    pub elder_vulnerability: f32,
}

/// One **tail** of the temperature-death model — the cold one below its `onset_temp`, the heat one
/// above. Every degree a tile sits past that onset kills
/// `min(excess × mortality_scale, max_mortality) × <bracket>_vulnerability` of the bracket per turn,
/// food or no food.
///
/// ⛔ **The two tails differ in ALL THREE parameters — threshold, slope and ceiling — which is why
/// the per-tail split was necessary rather than cosmetic.** No single symmetric deviation from an
/// ambient can express that: a tolerance forced the onsets to mirror each other about
/// `ambient_temperature`, putting heat death at 30 °, a warm summer day.
///
/// **Extreme heat is markedly less lethal than extreme cold, and that asymmetry is deliberate.**
/// Heat is survivable with shade and water; −57 ° demands shelter, fire and clothing. So the heat
/// ceiling is 3 % against cold's 10 % — the deadliest heat costs a band roughly a third of what the
/// deadliest cold does (~1.2 against ~3.0 people per turn on a band of 23). Do not "restore
/// symmetry" here: the gap is the model, not an oversight.
///
/// **The rates are calibrated to the temperatures the map should REACH, not the ones it currently
/// produces.** Each tail rises from zero at its onset to `max_mortality` at its own target extreme —
/// −57 ° for cold (57 ° of runway past the 0 ° onset, `0.10 ÷ 57 ≈ 0.00175`) and +57 ° for heat
/// (17 ° past the 40 ° onset, `0.03 ÷ 17 ≈ 0.00176`). Different runways *and* different ceilings, so
/// no shared scale could have served both.
///
/// **The cold onset is 0 °, which is also `climate.polar_max_temp`** — so Boreal and Temperate
/// ground is survivable end to end and only Polar ground kills. That agreement is a *consequence* of
/// two independently-set config values, not something code enforces: move either and the tile card's
/// climate label and its survivability verdict drift apart again (issue #614).
///
/// Today's generator spans −18.5 ° to +31.0 °, so the coldest reachable tile costs a worker
/// 3.2 %/turn, the ceiling is out of reach at both ends, and the heat tail is **entirely dormant** —
/// nothing comes within nine degrees of its onset. That is intended. Calibrating to today's narrow
/// range would have to be redone the moment the range widens, and would make every tile below
/// −18.5 ° kill at an identical rate once such tiles exist. Issue #622 widens the range; these four
/// numbers are the ones it must be checked against.
///
/// The vulnerabilities deliberately differ from [`DemographicsScarcity`]'s: temperature takes the
/// **old first**, then children, and is gentlest on working-age, where starvation weights children
/// and elders alike. Separate levers because the two do not hurt the same people equally. They are
/// ratios, so unlike the rates they say nothing about how wide the map's temperature range is.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemographicsTemperatureTail {
    /// The tile temperature (°) at which this tail starts costing lives — below it for `cold`,
    /// above it for `heat`.
    pub onset_temp: f32,
    pub mortality_scale: f32,
    /// ⛔ **The ceiling on the TILE's base rate — not the most any bracket can suffer.** It is
    /// applied *before* the per-bracket multiplier, so a bracket's actual ceiling is
    /// `max_mortality × <bracket>_vulnerability`: 10 % for workers, 12.5 % for children, 15 % for
    /// elders at the shipped tuning.
    ///
    /// Clamping *after* the multiplier was tried and rejected. It reads like the safer ordering — a
    /// true per-bracket ceiling nobody can exceed — but it makes the brackets converge: elders would
    /// saturate at −38.1 ° and children at −45.7 °, so from −45.7 ° down every bracket sits on exactly
    /// the same number and the age ordering vanishes in the range where cold is *most* severe, which
    /// is the one place it matters most. Capping the base rate keeps "the cold takes the old first"
    /// true all the way down.
    pub max_mortality: f32,
    pub child_vulnerability: f32,
    pub working_vulnerability: f32,
    pub elder_vulnerability: f32,
}

/// **One keeping class** (#706) — a kind of food by how it keeps. Its one property is how many
/// turns a unit of it lasts before it rots; a species names its class (`fauna_config` /
/// `flora_config` `keeping`), and preservation is a longer shelf life, nothing else.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeepingClass {
    /// The id a species' `keeping` field names, and the key a larder holds the class under.
    pub id: String,
    /// **Turns a lot of this class lasts.** Food rots whole at the end of its shelf life: a lot
    /// whose age (turns kept, walk included) reaches this many rots in that turn's rot pass
    /// (`crate::spoilage::rot_band_larders`). A caravan pack whose walk home is longer than this
    /// rots on the way (`crate::spoilage::rots_in_transit`).
    pub shelf_life_turns: f32,
}

/// **How food keeps** (#706, `docs/plan_civilization_steps.md` §Step 5 → "Only food the band cannot
/// eat in time rots"). The class table, plus the three classes the sim names where no species says.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeepingConfig {
    /// Every keeping class. Ids unique, shelf lives positive.
    pub classes: Vec<KeepingClass>,
    /// The class a new band's opening larder is seeded into — long-keeping, so the reserve it
    /// starts with is not lost on turn one.
    pub startup_class: String,
    /// The class a **plant** take lands in where no species is known (an empty basket, a ranging
    /// party's roadside gather of a stand the roster cannot decompose).
    pub plant_fallback_class: String,
    /// The class a **kill** lands in where no species is known (a herd the fauna table cannot
    /// resolve — an isolated fixture).
    pub kill_fallback_class: String,
    /// **WHAT A STORE BUYS** (#708) - the shelf life a class takes while a band holds a rung. See
    /// [`Preservation`]. Required, and empty means no rung preserves anything.
    pub preservation: Vec<Preservation>,
    /// **WHAT A FIXED STORE HOLDS** (#709) - the capacity of each tile-kind rung that is a store
    /// ([`FixedStore`]). A band's own pit is its second larder; the classes it keeps and the shelf
    /// life inside it are this rung's [`Self::preservation`] entries. Required, and empty means no
    /// tile rung is a store.
    pub fixed_stores: Vec<FixedStore>,
}

/// **ONE FIXED STORE - a tile rung that holds its owner band's food** (#709,
/// `docs/plan_civilization_steps.md` -> "The storage pit is its band's second larder").
///
/// The rung's [`Preservation`] entries say WHICH classes the store holds and how long each keeps
/// inside it; this says how MUCH it holds. **`capacity` is an OPENING VALUE / PLAYTEST DIAL**, in
/// food units: 400 is about 150 people for 20 turns at the shipped eating rate.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixedStore {
    /// Free-text note, ignored by the sim.
    #[serde(rename = "_comment")]
    pub comment: Option<String>,
    /// The tile-kind rung that is a store, `branch:id`. A holder of this rung or any above it is a
    /// store; the highest such entry a holder reaches sets its capacity.
    pub rung: crate::ladder_ids::RungId,
    /// Food units the store holds, finite and above zero.
    pub capacity: f32,
}

/// **ONE PRESERVATION - the VALUE a class's shelf life takes while a rung is held** (#708,
/// `docs/plan_civilization_steps.md` §Step 5). It names the number, never a multiplier: a drying rack
/// does not make flesh keep "5 times longer", it makes flesh keep *20 turns*, so a retune of the base
/// class cannot silently drag the preserved life with it.
///
/// **Which holder counts follows from the rung's source kind**, not from anything written here: a
/// `band`-kind rung is held by the band itself, a `tile`-kind rung by any improvement of the band's
/// own people within the supply network's reach of the band
/// ([`crate::spoilage::HeldPreservation`]). A band's shelf life for a class is the longest of the base
/// and every preservation it holds.
///
/// **Both life values are OPENING VALUES, PLAYTEST DIALS**: flesh 4 -> 20 and dry 60 -> 180.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preservation {
    /// The rung that has to be held, `branch:id`. Resolved against the ladder at boot
    /// ([`KeepingConfig::validate_preservation`]); an unknown rung is a boot panic, not a lever that
    /// does nothing.
    pub rung: crate::ladder_ids::RungId,
    /// The keeping class it lengthens.
    pub class: String,
    /// The class's shelf life while the rung is held, in turns. Finite and strictly greater than the
    /// class's base - a preservation that shortens or equals the base is a lever that does nothing.
    pub shelf_life_turns: f32,
}

/// **A fixed store as a held rung resolves it** ([`KeepingConfig::store_spec`]): the capacity, and
/// per class the shelf life INSIDE the store - a class absent from `lives` cannot be in it.
#[derive(Debug, Clone, PartialEq)]
pub struct StoreSpec {
    /// The store rung that set this spec.
    pub rung: crate::ladder_ids::RungId,
    /// Food units it holds.
    pub capacity: f32,
    /// `class -> shelf life inside the store`.
    pub lives: Vec<(String, f32)>,
}

impl StoreSpec {
    /// The shelf life of `class` inside the store, `None` for a class it does not hold.
    pub fn life_of(&self, class: &str) -> Option<f32> {
        self.lives
            .iter()
            .find(|(id, _)| id == class)
            .map(|(_, life)| *life)
    }

    /// Does the store hold `class`?
    pub fn holds(&self, class: &str) -> bool {
        self.life_of(class).is_some()
    }
}

impl KeepingConfig {
    /// Is `id` a configured class?
    pub fn has_class(&self, id: &str) -> bool {
        self.classes.iter().any(|class| class.id == id)
    }

    /// The class's shelf life, `None` for an id the table does not carry.
    pub fn shelf_life(&self, id: &str) -> Option<f32> {
        self.classes
            .iter()
            .find(|class| class.id == id)
            .map(|class| class.shelf_life_turns)
    }

    /// **The classes fastest-rotting first** — the order a band eats in and the order the rot rule
    /// walks. Ties break on the id so the order is total.
    pub fn by_shelf_life(&self) -> Vec<&KeepingClass> {
        let mut classes: Vec<&KeepingClass> = self.classes.iter().collect();
        classes.sort_by(|a, b| {
            a.shelf_life_turns
                .total_cmp(&b.shelf_life_turns)
                .then_with(|| a.id.cmp(&b.id))
        });
        classes
    }

    /// The class ids in eating order ([`Self::by_shelf_life`]).
    pub fn eat_order(&self) -> Vec<String> {
        self.by_shelf_life()
            .into_iter()
            .map(|class| class.id.clone())
            .collect()
    }

    /// **The table is coherent**: at least one class, unique ids, finite positive shelf lives, and
    /// every named class (startup and the two fallbacks) present.
    pub fn validate(&self) -> Result<(), String> {
        if self.classes.is_empty() {
            return Err("keeping.classes must name at least one class".to_string());
        }
        let mut seen = std::collections::BTreeSet::new();
        for class in &self.classes {
            if !seen.insert(class.id.as_str()) {
                return Err(format!("keeping class '{}' is declared twice", class.id));
            }
            if !(class.shelf_life_turns.is_finite() && class.shelf_life_turns > 0.0) {
                return Err(format!(
                    "keeping class '{}' needs a finite positive shelf_life_turns, got {}",
                    class.id, class.shelf_life_turns
                ));
            }
        }
        for (key, id) in [
            ("startup_class", &self.startup_class),
            ("plant_fallback_class", &self.plant_fallback_class),
            ("kill_fallback_class", &self.kill_fallback_class),
        ] {
            if !self.has_class(id) {
                return Err(format!("keeping.{key} names unknown class '{id}'"));
            }
        }
        self.validate_fixed_stores_table()?;
        self.validate_preservation_table()
    }

    /// **THE FIXED-STORE TABLE IS COHERENT ON ITS OWN**: a finite capacity above zero, no rung twice.
    fn validate_fixed_stores_table(&self) -> Result<(), String> {
        let mut seen = std::collections::BTreeSet::new();
        for entry in &self.fixed_stores {
            if !(entry.capacity.is_finite() && entry.capacity > 0.0) {
                return Err(format!(
                    "keeping.fixed_stores for {} needs a finite capacity above zero, got {}",
                    entry.rung, entry.capacity
                ));
            }
            if !seen.insert(entry.rung) {
                return Err(format!("keeping.fixed_stores states {} twice", entry.rung));
            }
        }
        Ok(())
    }

    /// **THE FIXED STORES RECONCILE WITH THE LADDER AND THE PRESERVATION TABLE** - the cross-config
    /// half, run at boot beside [`Self::validate_preservation`]. Every store rung exists, sits on a
    /// `tile` branch, is not a root, and has a preservation entry for the same rung (its classes and
    /// its shelf life inside the store). Conversely a preservation entry on a tile rung with no
    /// store entry would preserve nothing now that a band no longer lengthens its larder's life by
    /// standing near a pit, so it is refused.
    pub fn validate_fixed_stores(
        &self,
        ladder: &crate::intensification::LadderConfig,
    ) -> Result<(), String> {
        use crate::intensification::SourceKind;
        for store in &self.fixed_stores {
            let Some(rung) = ladder.rung_by_id(store.rung) else {
                return Err(format!(
                    "keeping.fixed_stores names unknown rung '{}'",
                    store.rung
                ));
            };
            if ladder.source_kind(rung.branch) != Some(SourceKind::Tile) {
                return Err(format!(
                    "keeping.fixed_stores names '{}', which is not on a tile branch",
                    store.rung
                ));
            }
            if rung.build.is_none() {
                return Err(format!(
                    "keeping.fixed_stores names '{}', a root every improvement stands on",
                    store.rung
                ));
            }
            if !self.preservation.iter().any(|p| p.rung == store.rung) {
                return Err(format!(
                    "keeping.fixed_stores names '{}' but no keeping.preservation entry states what \
                     it holds",
                    store.rung
                ));
            }
        }
        for entry in &self.preservation {
            let on_tile = ladder.source_kind(entry.rung.branch) == Some(SourceKind::Tile);
            if on_tile && !self.fixed_stores.iter().any(|s| s.rung == entry.rung) {
                return Err(format!(
                    "keeping.preservation names tile rung '{}' with no keeping.fixed_stores entry - \
                     it would preserve nothing",
                    entry.rung
                ));
            }
        }
        Ok(())
    }

    /// **WHAT A TILE IMPROVEMENT HOLDING `held` IS AS A STORE** - `None` where it holds no
    /// fixed-store rung. The highest store rung `held` is at or above sets the capacity; the classes
    /// and their in-store shelf lives are that rung's preservation entries.
    pub fn store_spec(
        &self,
        ladder: &crate::intensification::LadderConfig,
        held: crate::ladder_ids::RungId,
    ) -> Option<StoreSpec> {
        let order_of = |rung| ladder.rung_by_id(rung).map(|def| def.order);
        let top = self
            .fixed_stores
            .iter()
            .filter(|store| ladder.at_or_above(held, store.rung))
            .max_by_key(|store| order_of(store.rung))?;
        let lives = self
            .preservation
            .iter()
            .filter(|entry| entry.rung == top.rung)
            .map(|entry| (entry.class.clone(), entry.shelf_life_turns))
            .collect();
        Some(StoreSpec {
            rung: top.rung,
            capacity: top.capacity,
            lives,
        })
    }

    /// **THE PRESERVATION TABLE IS COHERENT ON ITS OWN**: every entry names a configured class, a
    /// finite shelf life strictly above that class's base, and no `(rung, class)` pair is stated twice.
    /// Whether the rung exists is [`Self::validate_preservation`]'s, which needs the ladder.
    fn validate_preservation_table(&self) -> Result<(), String> {
        let mut seen = std::collections::BTreeSet::new();
        for entry in &self.preservation {
            let Some(base) = self.shelf_life(&entry.class) else {
                return Err(format!(
                    "keeping.preservation names unknown class '{}'",
                    entry.class
                ));
            };
            if !(entry.shelf_life_turns.is_finite() && entry.shelf_life_turns > base) {
                return Err(format!(
                    "keeping.preservation for {} on '{}' needs a finite shelf_life_turns above the \
                     class's base of {base}, got {} - a preservation that does not lengthen the \
                     shelf life does nothing",
                    entry.rung, entry.class, entry.shelf_life_turns
                ));
            }
            if !seen.insert((entry.rung, entry.class.as_str())) {
                return Err(format!(
                    "keeping.preservation states {} on '{}' twice",
                    entry.rung, entry.class
                ));
            }
        }
        Ok(())
    }

    /// **EVERY PRESERVATION NAMES A RUNG THE LADDER DEFINES, AND ONE A HOLDER CAN STAND ON** - the
    /// cross-config half of the table, run at boot where both configs are in scope (`validate` runs
    /// inside `from_json_str`, which holds no ladder). A rung nobody defines, one nothing raises (a
    /// root every source stands on), or one on a branch whose kind has no holder a band can read
    /// (a plant, a herd, a road, a deposit) would be a preservation that parses and never applies.
    pub fn validate_preservation(
        &self,
        ladder: &crate::intensification::LadderConfig,
    ) -> Result<(), String> {
        use crate::intensification::SourceKind;
        for entry in &self.preservation {
            let Some(rung) = ladder.rung_by_id(entry.rung) else {
                return Err(format!(
                    "keeping.preservation names unknown rung '{}'",
                    entry.rung
                ));
            };
            if rung.build.is_none() {
                return Err(format!(
                    "keeping.preservation names '{}', a rung nothing raises - every source stands \
                     on it, so it would preserve for everyone",
                    entry.rung
                ));
            }
            if !matches!(
                ladder.source_kind(rung.branch),
                Some(SourceKind::Band | SourceKind::Tile)
            ) {
                return Err(format!(
                    "keeping.preservation names '{}', which is not on a band or tile branch - only \
                     a band's own improvement or its people's nearby tile improvement is a store",
                    entry.rung
                ));
            }
        }
        Ok(())
    }
}

/// **Founding lines** — the relatedness proxy (`crate::lineage`, issue #687).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemographicsLineage {
    /// `L` — how many unrelated families a **starting** band descends from. Every faction's
    /// starting band is founded with this many lines; a split takes a proportional share of its
    /// parent's. **At least one**, enforced by the type: a band of people descends from someone,
    /// and a `0` is a parse error rather than a band with no lines.
    pub founding_lines: NonZeroU16,
    /// `K` — **how many people one founding line can carry** before inbreeding stops a breeding
    /// population's births (issue #688). A breeding population — a people's tie-joined group of
    /// bands (`lineage::tie_joined_groups`), or a lone band — ceilings at `|union of its members' lines| × K`. With the shipped `L` an
    /// isolated starting band ceilings at `L × K`, which must sit above the starting band's size
    /// (pinned by a test, not by load-time validation). **At least one**, enforced by the type: a
    /// `0` would ceiling every people at nobody, so it is a parse error.
    pub people_per_line: NonZeroU16,
    /// **The size at which inbreeding stops binding**, in people (issue #688). A breeding
    /// population whose ceiling (`|union of lines| × K`) reaches this is large enough to find mates
    /// without outside contact — ~500 is the forager mating-network size (Birdsell's dialect tribe;
    /// Wobst's 175–475) — so the inbreeding ceiling no longer applies to it at all. Must sit above
    /// the shipped `L × K`, or a lone starting band would open unrestricted (pinned by a test, not by
    /// load-time validation). **At least one**, enforced by the type.
    pub free_breeding_at: NonZeroU32,
    /// **The share of a breeding population's excess over its ceiling that leaves each turn**
    /// (issue #690), in `(0, 1]`. A population above `|lines| × K` (a lifter lapsed) sheds
    /// `shed_share × (head-count − ceiling)` people a turn along the ways the unhappy trickle
    /// already walks, into groups with headroom; with nowhere to go, the whole excess breaks off as
    /// a people of its own. A PLAYTEST DIAL: `1` clears the excess in one turn, a small share bleeds
    /// it off slowly. Validated finite and in `(0, 1]` at parse.
    pub shed_share: f32,
}

impl DemographicsLineage {
    /// **`shed_share` is a share of an excess**: finite, above `0` (or the leak never moves
    /// anybody) and at most `1` (a share cannot shed more than the excess).
    pub fn validate(&self) -> Result<(), String> {
        if !(self.shed_share.is_finite() && self.shed_share > 0.0 && self.shed_share <= 1.0) {
            return Err(format!(
                "lineage.shed_share must be a finite share in (0, 1], got {}",
                self.shed_share
            ));
        }
        Ok(())
    }
}

/// Root demographic configuration.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DemographicsConfig {
    pub initial_distribution: DemographicsDistribution,
    pub consumption: DemographicsConsumption,
    pub startup: DemographicsStartup,
    /// How food keeps — the keeping classes and their shelf lives (#706).
    pub keeping: KeepingConfig,
    pub births: DemographicsBirths,
    /// Fraction of children that mature into the working bracket each turn.
    pub maturation_rate: f32,
    /// Fraction of working-age that age into the elder bracket each turn.
    pub aging_rate: f32,
    /// Fraction of elders that die of old age each turn.
    pub elder_mortality_rate: f32,
    pub scarcity: DemographicsScarcity,
    /// The tail below `cold.onset_temp`.
    pub cold: DemographicsTemperatureTail,
    /// The tail above `heat.onset_temp`. **Dormant on today's maps** — worldgen tops out near 31 °
    /// and the onset is 40 ° — but calibrated for the range issue #622 opens up, not for today's.
    pub heat: DemographicsTemperatureTail,
    pub lineage: DemographicsLineage,
}

/// The **only** `Default` in this module: it parses the builtin JSON, so `default()` and the
/// shipped tuning are the same numbers by construction and cannot drift.
///
/// This is non-recursive **only because no struct here carries a container-level
/// `#[serde(default)]`** — that attribute would make deserialization call back into this impl.
/// Do not re-add it; every field is deliberately required (`deny_unknown_fields` on top), so a
/// missing or unknown key fails loudly instead of falling back to a second set of numbers.
impl Default for DemographicsConfig {
    fn default() -> Self {
        Self::from_json_str(BUILTIN_DEMOGRAPHICS_CONFIG)
            .expect("builtin demographics config should parse and validate")
    }
}

impl DemographicsConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Parse **and validate** — every load path (builtin, file, override, staged patch) runs the
    /// keeping table's coherence check here, so a broken class table cannot load anywhere.
    pub fn from_json_str(json: &str) -> Result<Self, DemographicsConfigError> {
        let config: Self = serde_json::from_str(json)?;
        config
            .keeping
            .validate()
            .map_err(DemographicsConfigError::Invalid)?;
        config
            .lineage
            .validate()
            .map_err(DemographicsConfigError::Invalid)?;
        Ok(config)
    }

    pub fn from_file(path: &Path) -> Result<Self, DemographicsConfigError> {
        let contents =
            fs::read_to_string(path).map_err(|source| DemographicsConfigError::Read {
                path: path.to_path_buf(),
                source,
            })?;
        DemographicsConfig::from_json_str(&contents)
    }
}

#[derive(Debug, Error)]
pub enum DemographicsConfigError {
    #[error("failed to read demographics config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse demographics config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid demographics config: {0}")]
    Invalid(String),
}

impl ConfigLoadError for DemographicsConfigError {
    /// The file simply **is not there**, as distinct from being unreadable (permissions, a
    /// directory in the way) or invalid. This is the one distinction the loader's fallback rule
    /// turns on — see [`crate::config_load::resolve_config`].
    fn is_not_found(&self) -> bool {
        match self {
            Self::Read { source, .. } => source.kind() == io::ErrorKind::NotFound,
            Self::Parse(_) | Self::Invalid(_) => false,
        }
    }
}

/// Handle for accessing the demographic configuration.
#[derive(Resource, Debug, Clone)]
pub struct DemographicsConfigHandle(pub Arc<DemographicsConfig>);

impl DemographicsConfigHandle {
    pub fn new(config: Arc<DemographicsConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<DemographicsConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<DemographicsConfig>) {
        self.0 = config;
    }
}

impl Default for DemographicsConfigHandle {
    fn default() -> Self {
        Self(DemographicsConfig::builtin())
    }
}

/// Metadata about the demographic configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct DemographicsConfigMetadata {
    path: Option<PathBuf>,
}

impl DemographicsConfigMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }
}

/// Load demographic config from environment (`DEMOGRAPHICS_CONFIG_PATH`) or the default data path.
///
/// **Panics** when the named/default file exists but cannot be read or parsed, or when an explicit
/// `DEMOGRAPHICS_CONFIG_PATH` names a file that is not there — see
/// [`crate::config_load::resolve_config`] for why only an absent *default* path is benign. Without
/// that, the strict schema (#350) would merely move the silent substitution one layer out: a
/// fat-fingered key would stop being a silent serde default and start being a silent whole-file
/// fallback.
pub fn load_demographics_config_from_env() -> (Arc<DemographicsConfig>, DemographicsConfigMetadata)
{
    let (config, source) = load_config_from_env(
        "DEMOGRAPHICS_CONFIG_PATH",
        "demographics_config",
        "src/data/demographics_config.json",
        DemographicsConfig::builtin,
        DemographicsConfig::from_file,
    );
    (config, DemographicsConfigMetadata::new(source))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_config_parses_and_is_sane() {
        let config = DemographicsConfig::builtin();
        // Initial distribution is a probability split.
        let dist = &config.initial_distribution;
        let sum = dist.children + dist.working + dist.elders;
        assert!(
            (sum - 1.0).abs() < 1e-3,
            "initial distribution should sum to ~1.0, got {sum}"
        );
        assert!(
            dist.working > 0.0,
            "there must be a working (labor) bracket"
        );
        // Rates are valid per-turn fractions.
        for rate in [
            config.maturation_rate,
            config.aging_rate,
            config.elder_mortality_rate,
            config.births.birth_rate,
            config.consumption.per_capita_draw,
        ] {
            assert!(rate >= 0.0, "rates must be non-negative");
        }
        assert!(config.scarcity.starvation_mortality >= 0.0);
        // Both tails weight the brackets the way the model orders them: elders worst, then
        // children, workers least. Asserted as an ORDERING rather than as literals so a re-tune
        // moves the numbers freely but cannot silently invert who the temperature takes first.
        for (name, tail) in [("cold", &config.cold), ("heat", &config.heat)] {
            // The cap bounds the BASE rate, so what has to stay a fraction is the cap times the
            // steepest bracket weight — that product is the real per-bracket ceiling, and
            // `death_fraction` relies on it being ≤ 1 to do without a clamp of its own.
            let steepest = tail
                .elder_vulnerability
                .max(tail.child_vulnerability)
                .max(tail.working_vulnerability);
            assert!(
                tail.max_mortality * steepest <= 1.0,
                "{name}'s worst bracket would lose {} of itself per turn, which is not a fraction",
                tail.max_mortality * steepest
            );
            assert!(
                tail.elder_vulnerability > tail.child_vulnerability
                    && tail.child_vulnerability > tail.working_vulnerability,
                "{name} must weight elders > children > working, got {} / {} / {}",
                tail.elder_vulnerability,
                tail.child_vulnerability,
                tail.working_vulnerability
            );
            assert!(
                tail.working_vulnerability > 0.0 && tail.mortality_scale >= 0.0,
                "{name} vulnerabilities are positive multipliers and the scale is non-negative"
            );
        }
        assert!(
            config.cold.onset_temp < config.heat.onset_temp,
            "the survivable band runs cold onset -> heat onset, got {} -> {}",
            config.cold.onset_temp,
            config.heat.onset_temp
        );
        // Bands must open the game with a positive food reserve.
        assert!(config.startup.food_reserve_days > 0.0);

        // Fertility factors (`docs/plan_population_growth_model.md`). Both saturations divide, so a
        // zero would be a degenerate curve; `deficit_penalty > 1` would drive `trend` negative were
        // it not clamped, and is never what a tuner means.
        let births = &config.births;
        assert!(
            births.reserve.saturation_turns > 0.0,
            "reserve saturation must be a positive number of turns"
        );
        assert!(births.reserve.bonus >= 0.0);
        assert!(
            births.trend.surplus_saturation > 0.0 && births.trend.deficit_saturation > 0.0,
            "trend saturations must be positive — they are divisors"
        );
        assert!(
            (0.0..=1.0).contains(&births.trend.deficit_penalty),
            "deficit_penalty is a fraction of base fertility (1.0 = flow stops growth), got {}",
            births.trend.deficit_penalty
        );
        assert!(births.trend.surplus_gain >= 0.0);

        // How food keeps (#706): unique ids, positive shelf lives, and every named class present.
        let keeping = &config.keeping;
        let mut ids = std::collections::BTreeSet::new();
        for class in &keeping.classes {
            assert!(
                ids.insert(class.id.as_str()),
                "class {} declared twice",
                class.id
            );
            assert!(
                class.shelf_life_turns > 0.0,
                "class {} must keep for some turns",
                class.id
            );
        }
        for named in [
            &keeping.startup_class,
            &keeping.plant_fallback_class,
            &keeping.kill_fallback_class,
        ] {
            assert!(
                keeping.has_class(named),
                "{named} is not a configured class"
            );
        }
        // The opening reserve must outlast the reserve itself, or a new band loses it on turn one.
        assert!(
            keeping
                .shelf_life(&keeping.startup_class)
                .expect("startup class is configured")
                > config.startup.food_reserve_days,
            "the startup class must keep longer than the reserve it holds"
        );
    }

    /// **Both food webs' rosters name configured keeping classes** — the boot reconciliation, run
    /// on the shipped tables.
    #[test]
    fn every_shipped_species_names_a_configured_keeping_class() {
        let keeping = &DemographicsConfig::builtin().keeping;
        crate::fauna_config::FaunaConfig::builtin()
            .validate_keeping(keeping)
            .expect("every fauna species names a configured class");
        crate::flora_config::FloraConfig::builtin()
            .validate_keeping(keeping)
            .expect("every flora species names a configured class");
    }

    /// A broken keeping table is refused at load, not discovered at the first meal.
    #[test]
    fn a_broken_keeping_table_is_rejected() {
        let breakers: [fn(&mut serde_json::Value); 3] = [
            |v| v["keeping"]["startup_class"] = serde_json::json!("ambrosia"),
            |v| v["keeping"]["classes"][0]["shelf_life_turns"] = serde_json::json!(0.0),
            |v| {
                let first = v["keeping"]["classes"][0].clone();
                v["keeping"]["classes"]
                    .as_array_mut()
                    .expect("classes is an array")
                    .push(first);
            },
        ];
        for breaker in breakers {
            let mut value = builtin_value();
            breaker(&mut value);
            assert!(
                DemographicsConfig::from_json_str(&value.to_string()).is_err(),
                "a broken keeping table must fail to load: {}",
                value["keeping"]
            );
        }
    }

    /// **A PRESERVATION THAT DOES NOTHING IS REFUSED AT LOAD** (#708): an unknown class, a life that
    /// equals or undercuts the class's base (the lever would do nothing), a life that is not finite,
    /// and the same `(rung, class)` stated twice. Each breaker is paired with the shipped table
    /// loading, so a gate that refuses everything cannot pass.
    #[test]
    fn a_preservation_that_does_nothing_is_rejected_at_load() {
        assert!(
            DemographicsConfig::from_json_str(&builtin_value().to_string()).is_ok(),
            "control: the shipped table loads"
        );
        let base_of_flesh = DemographicsConfig::builtin()
            .keeping
            .shelf_life("flesh")
            .expect("the shipped table keeps flesh");
        /// One way to break the table: what it is, and how it edits the shipped JSON given the
        /// base life of the first entry's class.
        type Breaker = (&'static str, fn(&mut serde_json::Value, f32));
        let breakers: [Breaker; 4] = [
            ("an unknown class", |v, _| {
                v["keeping"]["preservation"][0]["class"] = serde_json::json!("ambrosia");
            }),
            ("a life equal to the base", |v, base| {
                v["keeping"]["preservation"][0]["shelf_life_turns"] = serde_json::json!(base);
            }),
            ("a life below the base", |v, base| {
                v["keeping"]["preservation"][0]["shelf_life_turns"] = serde_json::json!(base * 0.5);
            }),
            ("the same rung and class twice", |v, _| {
                let first = v["keeping"]["preservation"][0].clone();
                v["keeping"]["preservation"]
                    .as_array_mut()
                    .expect("preservation is an array")
                    .push(first);
            }),
        ];
        for (what, breaker) in breakers {
            let mut value = builtin_value();
            breaker(&mut value, base_of_flesh);
            assert!(
                DemographicsConfig::from_json_str(&value.to_string()).is_err(),
                "{what} must fail to load: {}",
                value["keeping"]["preservation"]
            );
        }
    }

    /// **A PRESERVATION NAMES A RUNG THE LADDER DEFINES, AND ONE A STORE CAN STAND ON** (#708) - the
    /// cross-config half, run at boot. The shipped table reconciles with the shipped ladder; a rung
    /// that does not exist, one nothing raises (a root), and one on a branch whose kind no band can
    /// hold (a plant) are each refused, loudly, rather than parsing into a lever that never applies.
    #[test]
    fn a_preservation_must_name_a_rung_a_store_can_stand_on() {
        let ladder = crate::intensification::LadderConfig::builtin();
        let keeping = DemographicsConfig::builtin().keeping.clone();
        keeping
            .validate_preservation(&ladder)
            .expect("control: the shipped table reconciles with the shipped ladder");

        let with_rung = |rung: &str| {
            let mut keeping = keeping.clone();
            keeping.preservation[0].rung = rung
                .parse()
                .unwrap_or_else(|_| panic!("'{rung}' is a well-formed rung key"));
            keeping
        };
        for (what, rung) in [
            ("a rung the ladder does not define", "camp_stores:granary"),
            ("a root nothing raises", "camp_stores:carried"),
            ("a rung on a plant branch", "plant:tended"),
            ("a rung on a road branch", "route:dirt_road"),
        ] {
            let refusal = with_rung(rung)
                .validate_preservation(&ladder)
                .expect_err(what);
            assert!(
                refusal.contains(rung),
                "{what}: the refusal names the rung: {refusal}"
            );
        }
    }

    /// **A FIXED STORE THAT STORES NOTHING IS REFUSED AT LOAD** (#709): a capacity that is zero,
    /// negative or not finite, and the same rung twice. Each breaker is paired with the shipped
    /// table loading.
    #[test]
    fn a_fixed_store_with_no_room_is_rejected_at_load() {
        assert!(
            DemographicsConfig::from_json_str(&builtin_value().to_string()).is_ok(),
            "control: the shipped table loads"
        );
        type Breaker = (&'static str, fn(&mut serde_json::Value));
        let breakers: [Breaker; 4] = [
            ("a zero capacity", |v| {
                v["keeping"]["fixed_stores"][0]["capacity"] = serde_json::json!(0.0);
            }),
            ("a negative capacity", |v| {
                v["keeping"]["fixed_stores"][0]["capacity"] = serde_json::json!(-5.0);
            }),
            ("a missing capacity", |v| {
                v["keeping"]["fixed_stores"][0]
                    .as_object_mut()
                    .expect("an entry")
                    .remove("capacity");
            }),
            ("the same rung twice", |v| {
                let first = v["keeping"]["fixed_stores"][0].clone();
                v["keeping"]["fixed_stores"]
                    .as_array_mut()
                    .expect("fixed_stores is an array")
                    .push(first);
            }),
        ];
        for (what, breaker) in breakers {
            let mut value = builtin_value();
            breaker(&mut value);
            assert!(
                DemographicsConfig::from_json_str(&value.to_string()).is_err(),
                "{what} must fail to load"
            );
        }
    }

    /// **THE FIXED STORES RECONCILE WITH THE LADDER AND THE PRESERVATION TABLE** (#709), at boot: each
    /// store rung exists, is on a tile branch, is not a root and has a preservation entry; and a tile
    /// rung that is preserved must be a store.
    #[test]
    fn a_fixed_store_must_name_a_tile_rung_the_preservation_table_describes() {
        let ladder = crate::intensification::LadderConfig::builtin();
        let keeping = DemographicsConfig::builtin().keeping.clone();
        keeping
            .validate_fixed_stores(&ladder)
            .expect("control: the shipped table reconciles with the shipped ladder");

        let with_store_rung = |rung: &str| {
            let mut keeping = keeping.clone();
            keeping.fixed_stores[0].rung = rung
                .parse()
                .unwrap_or_else(|_| panic!("'{rung}' is a well-formed rung key"));
            keeping
        };
        for (what, rung) in [
            ("a rung the ladder does not define", "food_stores:granary"),
            ("a root nothing raises", "food_stores:unstored"),
            ("a rung on a band branch", "camp_stores:drying_rack"),
            ("a rung on a plant branch", "plant:tended"),
        ] {
            assert!(
                with_store_rung(rung)
                    .validate_fixed_stores(&ladder)
                    .is_err(),
                "{what} must be refused"
            );
        }

        let mut unpreserved = keeping.clone();
        unpreserved
            .preservation
            .retain(|entry| entry.rung != unpreserved.fixed_stores[0].rung);
        assert!(
            unpreserved.validate_fixed_stores(&ladder).is_err(),
            "a store with no preservation entry holds nothing"
        );

        let mut unstored = keeping.clone();
        unstored.fixed_stores.clear();
        assert!(
            unstored.validate_fixed_stores(&ladder).is_err(),
            "a preserved tile rung with no store entry would do nothing"
        );
    }

    /// The JSON is the sole source (#350): `default()` **is** the shipped tuning, so a test that
    /// takes a `DemographicsConfig::default()` exercises the numbers the campaign runs on. The
    /// comparison is deliberately **total** rather than a sample of fields — re-introducing a
    /// hand-written `Default` anywhere in the tree fails here the moment any one of its literals
    /// disagrees with the data file, which is precisely the drift this change removed.
    #[test]
    fn default_is_the_shipped_tuning() {
        let shipped: DemographicsConfig =
            serde_json::from_str(BUILTIN_DEMOGRAPHICS_CONFIG).expect("builtin should parse");
        assert_eq!(
            DemographicsConfig::default(),
            shipped,
            "default() must be the shipped JSON, field for field"
        );
    }

    /// Parse the builtin into a `Value` so these tests describe a *shape* rule, not a tuning value —
    /// splicing the shipped literal into a string edit would silently no-op after any re-tune.
    fn builtin_value() -> serde_json::Value {
        serde_json::from_str(BUILTIN_DEMOGRAPHICS_CONFIG).expect("builtin should parse")
    }

    /// A missing key is a **loud parse error**, not a silent fallback — that fallback is exactly how
    /// the Rust and JSON `per_capita_draw` values drifted 5.3× apart.
    #[test]
    fn a_missing_key_is_rejected() {
        let mut value = builtin_value();
        value["consumption"]
            .as_object_mut()
            .expect("consumption is an object")
            .remove("per_capita_draw")
            .expect("builtin should carry per_capita_draw");
        assert!(
            DemographicsConfig::from_json_str(&value.to_string()).is_err(),
            "a config missing per_capita_draw must fail to parse"
        );
    }

    /// So is a key no Rust field reads — a retired lever left behind would otherwise look live.
    #[test]
    fn an_unknown_key_is_rejected() {
        let mut value = builtin_value();
        value["consumption"]
            .as_object_mut()
            .expect("consumption is an object")
            .insert("retired_lever".to_string(), serde_json::json!(1.0));
        assert!(
            DemographicsConfig::from_json_str(&value.to_string()).is_err(),
            "a retired/misspelled key must fail to parse"
        );
    }

    /// A band with **no** founding lines descends from nobody, so `lineage.founding_lines = 0` is a
    /// parse error rather than a world whose starting bands hold an empty set.
    #[test]
    fn zero_founding_lines_is_rejected() {
        let mut value = builtin_value();
        value["lineage"]["founding_lines"] = serde_json::json!(0);
        assert!(
            DemographicsConfig::from_json_str(&value.to_string()).is_err(),
            "a starting band must descend from at least one line"
        );
    }

    /// A line that carries **nobody** would ceiling every people at zero, so
    /// `lineage.people_per_line = 0` is a parse error.
    #[test]
    fn zero_people_per_line_is_rejected() {
        let mut value = builtin_value();
        value["lineage"]["people_per_line"] = serde_json::json!(0);
        assert!(
            DemographicsConfig::from_json_str(&value.to_string()).is_err(),
            "a founding line must carry at least one person"
        );
    }

    /// **The game never opens capped** (issue #688): an isolated starting band's breeding ceiling,
    /// the shipped `L × K`, sits above every shipped starting unit's head-count. A test rather than
    /// load-time cross-config validation — the two files load independently, and what this pins is
    /// the shipped tuning, not a rule a modder's config must obey.
    #[test]
    fn the_shipped_starting_band_opens_below_its_breeding_ceiling() {
        let lineage = DemographicsConfig::default().lineage;
        let ceiling = crate::lineage::breeding_ceiling(
            usize::from(lineage.founding_lines.get()),
            lineage.people_per_line,
        );
        let profiles = crate::start_profile::StartProfiles::builtin();
        let mut checked = 0;
        for profile in profiles.iter() {
            for archetype in &profile.overrides().archetypes {
                for unit in &archetype.starting_units {
                    assert!(
                        unit.band_size() < ceiling,
                        "profile '{}' archetype '{}' opens a {}-person band at or above its \
                         breeding ceiling {ceiling}",
                        profile.id,
                        archetype.id,
                        unit.band_size()
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 0, "the shipped profiles carry a starting unit");
    }

    /// `lineage.shed_share` is a share of the excess: zero never sheds, above one sheds more than
    /// there is, and neither a missing key nor a non-finite value may fall back silently.
    #[test]
    fn a_shed_share_outside_zero_to_one_is_rejected() {
        for bad in [0.0, -0.1, 1.5] {
            let mut value = builtin_value();
            value["lineage"]["shed_share"] = serde_json::json!(bad);
            assert!(
                DemographicsConfig::from_json_str(&value.to_string()).is_err(),
                "a shed_share of {bad} is not a share of an excess"
            );
        }
        let mut value = builtin_value();
        value["lineage"]
            .as_object_mut()
            .expect("lineage is an object")
            .remove("shed_share");
        assert!(
            DemographicsConfig::from_json_str(&value.to_string()).is_err(),
            "every key is required"
        );
        let mut whole = builtin_value();
        whole["lineage"]["shed_share"] = serde_json::json!(1.0);
        assert!(DemographicsConfig::from_json_str(&whole.to_string()).is_ok());
    }

    /// `lineage.free_breeding_at = 0` would lift every ceiling, so it is a parse error.
    #[test]
    fn zero_free_breeding_at_is_rejected() {
        let mut value = builtin_value();
        value["lineage"]["free_breeding_at"] = serde_json::json!(0);
        assert!(
            DemographicsConfig::from_json_str(&value.to_string()).is_err(),
            "the free-breeding size must be at least one person"
        );
    }

    /// **A lone starting band opens capped** (issue #688): the shipped `L × K` sits below
    /// `free_breeding_at`, so an isolated people is held by its lines and only contact lifts it.
    #[test]
    fn the_shipped_lone_band_ceiling_sits_below_the_free_breeding_size() {
        let lineage = DemographicsConfig::default().lineage;
        let ceiling = crate::lineage::breeding_ceiling(
            usize::from(lineage.founding_lines.get()),
            lineage.people_per_line,
        );
        assert!(
            ceiling < lineage.free_breeding_at.get(),
            "a lone band's ceiling {ceiling} must sit below free_breeding_at {}",
            lineage.free_breeding_at
        );
    }
}
