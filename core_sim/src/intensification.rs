//! **The intensification ladder** — one grammar for both food webs
//! (`docs/plan_intensification_ladder.md`, authoritative spec: `core_sim/CLAUDE.md` → "The
//! Intensification Ladder").
//!
//! Plants and animals climb the *same* three-rung ladder — rung 1 you take what's there, rung 2 you
//! manage the wild source in place, rung 3 you control its reproduction — and every rung-transition
//! is the same **Cultivate-shaped verb**: pick it → the source pays a *reduced* yield while the crew
//! prepares rather than harvests → a **per-source build meter** climbs → it decays if you walk away →
//! at `1.0` the source steps up a rung.
//!
//! This module is the **data + the seam**, not a second copy of the rules:
//! - [`LadderConfig`] (`data/intensification_ladder.json`) holds one [`RungDef`] record per rung —
//!   the links (verb, unlock/earns knowledge, previous rung, husbandry ceiling) and the build dials.
//!   Adding a rung that recombines existing primitives is a one-record edit.
//! - [`RungDef::build_accrual`] / [`RungDef::build_decay`] / [`RungDef::yield_fraction_while_building`]
//!   are **the** build seam. Both tracks call them instead of reaching for their own bespoke
//!   accrue/decay/dip levers, so the plant and animal ladders can never drift apart numerically.
//!   The per-source *state* is unchanged and stays where it lives (`ForagePatch::cultivation_progress`,
//!   `Herd::domestication_progress`, `Herd::corral_progress`) — the engine supplies the amounts, the
//!   source owns its meter and the side-effects of completing it (ownership, `corralled_at`, …).
//! - [`knows`] is the one knowledge gate. It retires the inlined
//!   `ledger.get_progress(faction, ID) >= threshold` checks that used to sit in the labor arms and
//!   the command handlers.
//!
//! **The config describes what the sim does TODAY**, deliberately — a later slice changes behaviour
//! by *editing the JSON*, which is the whole point of extracting it. Slice 3a proved that: giving
//! animal `pastoral` its `tame` verb + `herding` gate + build dials was (on the config side) a
//! one-record edit. The engine simply **does not drive** a rung with no verb — which is all the
//! `wild` rungs are now.
//!
//! **Behavior primitives are parsed and validated, but nothing reads them yet** ([`RungBehavior`]).
//! They are the bounded coded set §5 calls for: a future rung that recombines them is pure config; a
//! rung needing a *new* primitive codes that one primitive once, after which it too is config.

use std::{
    collections::HashSet,
    env, fs, io,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

use crate::{
    components::FollowPolicy, fauna::HERDING_DISCOVERY_ID, fauna_config::HusbandryCeiling,
    forage::CULTIVATION_DISCOVERY_ID, orders::FactionId, resources::DiscoveryProgressLedger,
    scalar::scalar_from_f32,
};

pub const BUILTIN_INTENSIFICATION_LADDER: &str = include_str!("data/intensification_ladder.json");

/// A completed build meter. Both tracks store progress in `[0.0, 1.0]` and treat `>= 1.0` as "this
/// source has climbed the rung", so the completion point is one named constant, not a literal
/// sprinkled across the accrual sites.
pub const RUNG_COMPLETE: f32 = 1.0;

/// **The build timescale of a rung whose sources all build at the rung's own pace.** Passed by every
/// caller that has no per-source multiplier to apply (the plant `tended` patch, the animal `pen` and
/// its `ExtendPen` rings) — a rung's dials *are* its turns, undilated.
///
/// The multiplier exists because rung 2 of the animal ladder is **not** one-size-fits-all: a species
/// declares its own `taming_rate` (`fauna_config`) and the `animal:pastoral` rung's build runs at that
/// **timescale**. See [`RungDef::build_accrual`] for why it scales the whole timescale rather than the
/// speed alone.
pub const RUNG_TIMESCALE_UNSCALED: f32 = 1.0;

/// Which food web a rung belongs to. The two webs are separate ladders that never share a rung — a
/// master rancher isn't automatically a farmer (`plan_intensification_ladder.md` §4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RungBranch {
    /// The **human** food web: forage patches (`forage.rs`).
    Plant,
    /// The **animal** food web: herds (`fauna.rs`).
    Animal,
}

impl RungBranch {
    /// Stable key (the JSON `branch` value), used in validation messages.
    pub fn as_str(self) -> &'static str {
        match self {
            RungBranch::Plant => "plant",
            RungBranch::Animal => "animal",
        }
    }
}

/// **The rungs the engine currently knows how to drive.** The ladder is data, but the *code* that
/// reads a specific rung has to name it; this bounded set is what [`LadderConfig::validate`] insists
/// the config actually defines, so [`LadderConfig::rung`] is infallible and a broken override can
/// never silently no-op a shipped rung. Appending a *new* rung record is still free — this list only
/// pins the ones a system reaches for by name today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RungKey {
    /// A wild, depletable forage patch.
    PlantWild,
    /// A **tended** patch — the `Cultivate` investment (`ForagePatch::cultivation_progress`).
    PlantTended,
    /// A wild herd.
    AnimalWild,
    /// A **pastoral** (mobile domesticated) herd — the `Tame` investment
    /// (`Herd::domestication_progress`).
    AnimalPastoral,
    /// A **penned** herd — the `Corral` investment (`Herd::corral_progress`).
    AnimalPen,
}

impl RungKey {
    /// Every rung a system names today — what `validate` requires the config to define.
    pub const ALL: [RungKey; 5] = [
        RungKey::PlantWild,
        RungKey::PlantTended,
        RungKey::AnimalWild,
        RungKey::AnimalPastoral,
        RungKey::AnimalPen,
    ];

    pub fn branch(self) -> RungBranch {
        match self {
            RungKey::PlantWild | RungKey::PlantTended => RungBranch::Plant,
            RungKey::AnimalWild | RungKey::AnimalPastoral | RungKey::AnimalPen => {
                RungBranch::Animal
            }
        }
    }

    /// The record's `id` in `intensification_ladder.json`.
    pub fn id(self) -> &'static str {
        match self {
            RungKey::PlantWild | RungKey::AnimalWild => "wild",
            RungKey::PlantTended => "tended",
            RungKey::AnimalPastoral => "pastoral",
            RungKey::AnimalPen => "pen",
        }
    }
}

/// How a source at this rung moves — **the proximity spine, far → near → fixed**
/// (`docs/plan_intensification_ladder.md` §3, dial 4). A bounded coded primitive (§5), and the
/// **first one the engine actually applies**: `fauna::advance_herds` dispatches a herd's movement off
/// the rung it stands on, so a rung that recombines these is pure config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RungMovement {
    /// Pinned to one place: a forage patch, a penned herd's fence.
    Fixed,
    /// Roams its own range — you go to it.
    Roam,
    /// **Drifts toward its owner's nearest band** and stays near it: less chasing, and it *reads* as
    /// domesticated. Composes with (never replaces) the roam's own barren-avoidance / graze
    /// preference — see `fauna::advance_herd_roam`. A source with no owner, or an owner with no
    /// bands, simply roams.
    DriftToOwner,
}

/// How a source at this rung feeds itself. A bounded coded primitive (§5) — **not read yet**
/// (`movement` is the only primitive the engine reads today).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RungFeeding {
    /// Needs no feed at all — the plant web regrows from the land it stands on.
    Photosynthesis,
    /// Eats the **open** graze layer wherever it roams (`GrazePatch`, `fodder_per_biomass`).
    Forage,
    /// Feeds off its own **fenced footprint**'s graze, the keeper's larder covering the shortfall
    /// (the pen economy, `docs/plan_grazing_2d.md`).
    SelfGraze,
}

/// How the harvest comes off a source at this rung. A bounded coded primitive (§5) — **not read
/// yet**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RungHarvest {
    /// Workers **draw the source down**: a wild gather / a wild hunt.
    WorkerTake,
    /// Workers take a **managed** harvest that never overdraws (a tended patch, a pen).
    WorkerTend,
    /// Pays its owner with **no workers at all**. **No shipped rung is `passive` any more** —
    /// `plan_intensification_ladder.md` §3 retired the passive-free pastoral rung in slice 3b (every
    /// rung is worker-driven; intensifying buys *yield per worker*, not zero workers). The variant
    /// survives as vocabulary for a future rung that genuinely pays for nothing.
    Passive,
}

/// The behavior primitives a rung recombines. Bounded enums over coded behavior, per §5 — a rung
/// that recombines existing primitives is pure config. Only **`movement`** is read today (slice 3b —
/// `fauna::advance_herds`); `feeding` / `harvest` are still parsed and validated only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct RungBehavior {
    pub movement: RungMovement,
    pub feeding: RungFeeding,
    pub harvest: RungHarvest,
}

/// The **per-source build meter** dials of one rung: what it costs to climb it, in labor and in
/// forgone yield.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct RungBuild {
    /// Build progress gained per turn a crew works the source under this rung's `verb` and the
    /// caller's eligibility gates hold. `1.0 / progress_per_turn` is the build length in turns.
    /// Validated `> 0` — a zero would silently make the rung unreachable.
    pub progress_per_turn: f32,
    /// Build progress bled per turn nobody works the source — "walk away and the cleared ground
    /// grows back over". Validated `0 <= decay_per_turn < progress_per_turn`: a rung that decayed at
    /// least as fast as it built could never complete, and `0` is legitimate (a pen's construction
    /// does not bleed; an abandoned pen escapes outright instead).
    ///
    /// **That bound holds per-source too, and it is checked exactly once here.** A per-source
    /// `timescale` ([`RungDef::build_accrual`]) multiplies *both* rates, so the ratio is invariant and
    /// no per-species restatement of this bound is needed — only that the multiplier itself is
    /// positive and finite, which the roster that owns it (`FaunaConfig::validate`) enforces.
    pub decay_per_turn: f32,
    /// **The investment cost.** While the meter is filling, the source's take ceiling is only this
    /// fraction of its **Sustain (MSY)** ceiling — the crew is preparing, not harvesting. A fraction
    /// of MSY is a sustainable draw, so the source stays healthy while the work goes on. Validated
    /// `0 < f < 1`: at `0` the rung would starve its builders, at `>= 1` it would cost nothing and
    /// the whole "investment with a time horizon" decision would evaporate.
    pub yield_fraction_while_building: f32,
}

/// **One rung of one ladder** — the record §5 promises: the links and the dials, no logic.
#[derive(Debug, Clone, Deserialize)]
pub struct RungDef {
    /// Rung name, unique within its branch (`wild`, `tended`, `pastoral`, `pen`, …).
    pub id: String,
    /// Which food web this rung belongs to.
    pub branch: RungBranch,
    /// Position on its branch's ladder, `1` = the wild source. Unique within the branch; the ladder
    /// is strictly sequential (see `requires_rung`).
    pub order: u32,
    /// The [`FollowPolicy`] whose verb fills this rung's build meter. `None` = **this rung is not
    /// driven by a verb today** — the engine skips it (today's animal `pastoral`, and both wild
    /// rungs, which are nothing to build).
    pub verb: Option<String>,
    /// The knowledge a faction must hold before it may select `verb`. `None` = ungated today.
    pub unlock_knowledge: Option<String>,
    /// The knowledge **practising this rung** teaches the faction (§4 — "practice rung N unlocks rung
    /// N+1"). `None` = this rung teaches nothing today.
    pub earns_knowledge: Option<String>,
    /// The rung below this one, which a source must have climbed first. `None` iff `order == 1`.
    pub requires_rung: Option<String>,
    /// The per-species `husbandry_ceiling` a herd needs to reach this rung (Grazing 2d-δ). Animal
    /// branch only — a plant has no species ceiling.
    pub ceiling_required: Option<HusbandryCeiling>,
    /// The build meter's dials, or `None` for a rung with nothing to build.
    pub build: Option<RungBuild>,
    /// The coded primitives this rung recombines. **Not read yet.**
    pub behavior: RungBehavior,
}

impl RungDef {
    /// The policy that drives this rung's build meter, already parsed. `None` for a rung no verb
    /// drives today. (Validated at load, so the parse cannot fail here.)
    pub fn verb_policy(&self) -> Option<FollowPolicy> {
        self.verb
            .as_deref()
            .map(|verb| FollowPolicy::from_str(verb).expect("validated at load"))
    }

    /// The discovery gating this rung's verb. `None` for an ungated rung. (Validated at load.)
    pub fn unlock_discovery_id(&self) -> Option<u32> {
        self.unlock_knowledge
            .as_deref()
            .map(|name| discovery_id_for(name).expect("validated at load"))
    }

    /// The discovery practising this rung teaches. `None` for a rung that teaches nothing.
    /// (Validated at load.)
    pub fn earns_discovery_id(&self) -> Option<u32> {
        self.earns_knowledge
            .as_deref()
            .map(|name| discovery_id_for(name).expect("validated at load"))
    }

    /// **The build seam — the accrual side.** How much this rung's per-source meter advances this
    /// turn: `progress_per_turn × timescale` when `policy` **is** the rung's verb *and* the caller's
    /// rung-specific gates hold (`eligible` — knows the unlock knowledge, the source is healthy, the
    /// species' ceiling allows it, the faction owns it), otherwise `0`.
    ///
    /// A rung with **no verb** (`verb: null`) or **no build** is never driven: it returns `0` — which
    /// is what keeps the `wild` rungs (nothing to build) out of the engine.
    ///
    /// # `timescale` — the rung owns the mechanic, the source scales it
    ///
    /// The dials are the *rung's*; how fast **this** source climbs it is the source's own nature —
    /// exactly the split the fauna roster already uses (rung `pastoral_gain` × species wild `r`).
    /// Today the only scaler is a species' `taming_rate` on `animal:pastoral` (a rabbit is quick and
    /// forgiving, binding a migratory herd is generational); every other caller passes
    /// [`RUNG_TIMESCALE_UNSCALED`].
    ///
    /// **It is a TIMESCALE, not a speed — [`RungDef::build_decay`] takes the same factor.** Scaling
    /// accrual alone would be a different, broken mechanic: a Steppe Runner's `0.04 × 0.2 = 0.008`/turn
    /// would fall *below* the rung's `0.01`/turn decay, so the species could never be tamed at all and
    /// the ladder's own "taming must out-run its decay" bound would be violated per-species. Scaling
    /// both preserves the rung's 4:1 ratio — **slow to tame, slow to forget** — which is also the
    /// truer story: a beast that takes a lifetime to gentle does not go feral in a season.
    ///
    /// The caller applies the amount to its own meter (`ForagePatch::accrue_cultivation` /
    /// `Herd::accrue_domestication` / `Herd::accrue_corral`), which owns the clamp to
    /// [`RUNG_COMPLETE`] and the side-effects of completing.
    pub fn build_accrual(&self, policy: FollowPolicy, eligible: bool, timescale: f32) -> f32 {
        let Some(build) = self.build.as_ref() else {
            return 0.0;
        };
        if !eligible || self.verb_policy() != Some(policy) {
            return 0.0;
        }
        build.progress_per_turn * timescale
    }

    /// **The build seam — the decay side.** How much this rung's per-source meter bleeds on a turn
    /// nobody works the source: `decay_per_turn × timescale`, the *same* factor
    /// [`RungDef::build_accrual`] takes (see there — the multiplier dilates the whole build
    /// timescale, so the rung's build:decay ratio is invariant). `0` for a rung with no build
    /// (nothing to lose).
    pub fn build_decay(&self, timescale: f32) -> f32 {
        self.build
            .as_ref()
            .map_or(0.0, |build| build.decay_per_turn * timescale)
    }

    /// **The build seam — the investment dip.** The fraction of the source's Sustain (MSY) ceiling
    /// it pays while this rung is being built. `None` for a rung with no build — a caller with no
    /// dip to apply must not silently substitute one.
    pub fn yield_fraction_while_building(&self) -> Option<f32> {
        self.build
            .as_ref()
            .map(|build| build.yield_fraction_while_building)
    }
}

/// The whole ladder: every rung of both branches (`data/intensification_ladder.json`).
#[derive(Debug, Clone, Deserialize)]
pub struct LadderConfig {
    pub rungs: Vec<RungDef>,
}

impl LadderConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            LadderConfig::from_json_str(BUILTIN_INTENSIFICATION_LADDER)
                .expect("builtin intensification ladder should parse and validate"),
        )
    }

    /// Parse **and validate** (the `fauna_config.rs` / `labor_config.rs` convention, so *every* load
    /// path — builtin, default file, `INTENSIFICATION_LADDER_PATH` override — is covered and a
    /// broken ladder can never be silently accepted).
    pub fn from_json_str(json: &str) -> Result<Self, LadderConfigError> {
        let config: LadderConfig = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

    pub fn from_file(path: &Path) -> Result<Self, LadderConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| LadderConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        LadderConfig::from_json_str(&contents)
    }

    /// A rung the engine names ([`RungKey`]). Infallible: `validate` requires every `RungKey` to be
    /// defined, and `validate` runs on every load path.
    pub fn rung(&self, key: RungKey) -> &RungDef {
        self.find(key.branch(), key.id())
            .expect("validate requires every coded rung to be defined")
    }

    /// A rung by branch + id, if it exists.
    pub fn find(&self, branch: RungBranch, id: &str) -> Option<&RungDef> {
        self.rungs
            .iter()
            .find(|rung| rung.branch == branch && rung.id == id)
    }

    /// Invariants a ladder must satisfy to be drivable. A ladder that breaks one doesn't crash — it
    /// quietly stops a rung being reachable, or reads a rung's dial off the wrong record, which is
    /// exactly the failure mode config validation exists to catch.
    pub fn validate(&self) -> Result<(), LadderConfigError> {
        let mut seen_ids: HashSet<(RungBranch, &str)> = HashSet::new();
        let mut seen_orders: HashSet<(RungBranch, u32)> = HashSet::new();

        for rung in &self.rungs {
            let where_ = format!("rungs[{}:{}]", rung.branch.as_str(), rung.id);

            if !seen_ids.insert((rung.branch, rung.id.as_str())) {
                return Err(LadderConfigError::Invalid {
                    field: where_,
                    constraint: "name each rung of a branch exactly once — a duplicate id makes \
                                 every by-name lookup ambiguous"
                        .to_string(),
                    value: format!("id '{}' appears twice", rung.id),
                });
            }
            if !seen_orders.insert((rung.branch, rung.order)) {
                return Err(LadderConfigError::Invalid {
                    field: where_,
                    constraint: "give each rung of a branch its own order — two rungs on one step \
                                 have no defined sequence"
                        .to_string(),
                    value: format!("order {} appears twice", rung.order),
                });
            }
            self.validate_sequence(rung, &where_)?;
            validate_links(rung, &where_)?;
            validate_build(rung, &where_)?;
        }

        for branch in [RungBranch::Plant, RungBranch::Animal] {
            let roots = self
                .rungs
                .iter()
                .filter(|rung| rung.branch == branch && rung.order == FIRST_RUNG_ORDER)
                .count();
            if roots != 1 {
                return Err(LadderConfigError::Invalid {
                    field: format!("rungs[{}]", branch.as_str()),
                    constraint: format!(
                        "give the branch exactly one order-{FIRST_RUNG_ORDER} rung — the wild \
                         source every ladder starts from"
                    ),
                    value: format!("{roots} rungs at order {FIRST_RUNG_ORDER}"),
                });
            }
        }

        // Every rung a system reaches for by name must exist, so `rung()` is infallible and an
        // override can't silently delete a shipped rung out from under the engine.
        for key in RungKey::ALL {
            if self.find(key.branch(), key.id()).is_none() {
                return Err(LadderConfigError::Invalid {
                    field: format!("rungs[{}:{}]", key.branch().as_str(), key.id()),
                    constraint: "define every rung the simulation drives by name (see RungKey)"
                        .to_string(),
                    value: "missing".to_string(),
                });
            }
        }
        Ok(())
    }

    /// The ladder is strictly sequential: rung 1 requires nothing, and every rung above it names the
    /// rung directly below it (same branch, `order - 1`). You cannot skip a rung you haven't
    /// practised (`plan_intensification_ladder.md` §4).
    fn validate_sequence(&self, rung: &RungDef, where_: &str) -> Result<(), LadderConfigError> {
        match (rung.order, rung.requires_rung.as_deref()) {
            (FIRST_RUNG_ORDER, None) => Ok(()),
            (FIRST_RUNG_ORDER, Some(requires)) => Err(LadderConfigError::Invalid {
                field: where_.to_string(),
                constraint: format!(
                    "leave `requires_rung` null on the order-{FIRST_RUNG_ORDER} rung — the wild \
                     source sits on nothing"
                ),
                value: format!("requires_rung = '{requires}'"),
            }),
            (_, None) => Err(LadderConfigError::Invalid {
                field: where_.to_string(),
                constraint: "name the rung below it in `requires_rung` — the ladder is sequential"
                    .to_string(),
                value: format!("order {} with requires_rung = null", rung.order),
            }),
            (order, Some(requires)) => {
                let below = self.find(rung.branch, requires);
                match below {
                    Some(below) if below.order == order - 1 => Ok(()),
                    Some(below) => Err(LadderConfigError::Invalid {
                        field: where_.to_string(),
                        constraint: "require the rung directly below it (order - 1) — the ladder \
                                     has no skipped steps"
                            .to_string(),
                        value: format!(
                            "order {order} requires '{requires}' at order {}",
                            below.order
                        ),
                    }),
                    None => Err(LadderConfigError::Invalid {
                        field: where_.to_string(),
                        constraint: "require a rung that exists on the same branch".to_string(),
                        value: format!("requires_rung = '{requires}'"),
                    }),
                }
            }
        }
    }
}

/// The order of the wild rung every branch starts from.
const FIRST_RUNG_ORDER: u32 = 1;

/// **The knowledge ids the ladder may name** — the bounded coded set, mirroring the behavior
/// primitives: the ladder links to knowledge by *name*, and a name the sim has no discovery for is a
/// typo that would silently ungate a rung. A new rung's new knowledge codes its id once here (and in
/// `data/start_profile_knowledge_tags.json`), after which it is config.
fn discovery_id_for(name: &str) -> Option<u32> {
    match name {
        "cultivation" => Some(CULTIVATION_DISCOVERY_ID),
        "herding" => Some(HERDING_DISCOVERY_ID),
        _ => None,
    }
}

/// The verb (when named) has to be a real policy, and the knowledge links (when named) real
/// discoveries — otherwise the rung is unreachable in a way nothing on the map would explain.
fn validate_links(rung: &RungDef, where_: &str) -> Result<(), LadderConfigError> {
    if let Some(verb) = rung.verb.as_deref() {
        if FollowPolicy::from_str(verb).is_err() {
            return Err(LadderConfigError::Invalid {
                field: where_.to_string(),
                constraint:
                    "name a real FollowPolicy in `verb` (or null for a rung no verb drives)"
                        .to_string(),
                value: format!("verb = '{verb}'"),
            });
        }
    }
    for (field, knowledge) in [
        ("unlock_knowledge", rung.unlock_knowledge.as_deref()),
        ("earns_knowledge", rung.earns_knowledge.as_deref()),
    ] {
        let Some(knowledge) = knowledge else { continue };
        if discovery_id_for(knowledge).is_none() {
            return Err(LadderConfigError::Invalid {
                field: where_.to_string(),
                constraint: format!(
                    "name a knowledge the sim has a discovery for in `{field}` (see \
                     `discovery_id_for`)"
                ),
                value: format!("{field} = '{knowledge}'"),
            });
        }
    }
    Ok(())
}

/// Bound the build dials whose `0`/inverted value would silently disable the rung rather than fail
/// loudly (the `FaunaConfig::validate` discipline).
fn validate_build(rung: &RungDef, where_: &str) -> Result<(), LadderConfigError> {
    let Some(build) = rung.build.as_ref() else {
        return Ok(());
    };
    if !build.progress_per_turn.is_finite() || build.progress_per_turn <= 0.0 {
        return Err(LadderConfigError::Invalid {
            field: where_.to_string(),
            constraint: "build the rung at a positive rate — a zero/negative \
                         `progress_per_turn` makes it unreachable, silently"
                .to_string(),
            value: format!("progress_per_turn = {}", build.progress_per_turn),
        });
    }
    if !build.decay_per_turn.is_finite()
        || build.decay_per_turn < 0.0
        || build.decay_per_turn >= build.progress_per_turn
    {
        return Err(LadderConfigError::Invalid {
            field: where_.to_string(),
            constraint: "decay at a non-negative rate slower than it builds — a rung that bleeds \
                         at least as fast as it accrues can never complete"
                .to_string(),
            value: format!(
                "decay_per_turn = {} against progress_per_turn = {}",
                build.decay_per_turn, build.progress_per_turn
            ),
        });
    }
    if !build.yield_fraction_while_building.is_finite()
        || build.yield_fraction_while_building <= 0.0
        || build.yield_fraction_while_building >= 1.0
    {
        return Err(LadderConfigError::Invalid {
            field: where_.to_string(),
            constraint:
                "dip the yield while building to a strict fraction of MSY (0 < f < 1) — at \
                         0 the crew starves, at 1 intensifying costs nothing"
                    .to_string(),
            value: format!(
                "yield_fraction_while_building = {}",
                build.yield_fraction_while_building
            ),
        });
    }
    Ok(())
}

/// **THE knowledge gate.** Does `faction` know `discovery` well enough to act on it — i.e. has its
/// ledger progress reached the completion `threshold`? The single source of the check that used to
/// sit inlined at five call sites (both labor arms, the `cultivate`/`corral` assignment validators,
/// and `extend_pen`), each spelling `get_progress(..) >= threshold` for itself.
///
/// `threshold` stays a caller-supplied lever because the two food webs still keep their own
/// `knowledge_completion_threshold` (`labor_config`'s `forage.cultivation`, `fauna_config`'s
/// `husbandry`) — the helper unifies the *comparison*, not the tuning.
pub fn knows(
    ledger: &DiscoveryProgressLedger,
    faction: FactionId,
    discovery: u32,
    threshold: f32,
) -> bool {
    ledger.get_progress(faction, discovery) >= scalar_from_f32(threshold)
}

#[derive(Debug, Error)]
pub enum LadderConfigError {
    #[error("failed to read intensification ladder from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse intensification ladder: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid intensification ladder: {field} must {constraint} (was {value})")]
    Invalid {
        field: String,
        constraint: String,
        value: String,
    },
}

/// Handle for accessing the intensification ladder.
#[derive(Resource, Debug, Clone)]
pub struct LadderConfigHandle(pub Arc<LadderConfig>);

impl LadderConfigHandle {
    pub fn new(config: Arc<LadderConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<LadderConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<LadderConfig>) {
        self.0 = config;
    }
}

impl Default for LadderConfigHandle {
    fn default() -> Self {
        Self(LadderConfig::builtin())
    }
}

/// Metadata about the intensification ladder source.
#[derive(Resource, Debug, Clone, Default)]
pub struct LadderConfigMetadata {
    path: Option<PathBuf>,
}

impl LadderConfigMetadata {
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

/// Load the ladder from environment (`INTENSIFICATION_LADDER_PATH`) or the default data path,
/// falling back to the baked-in builtin.
pub fn load_intensification_ladder_from_env() -> (Arc<LadderConfig>, LadderConfigMetadata) {
    let override_path = env::var("INTENSIFICATION_LADDER_PATH")
        .ok()
        .map(PathBuf::from);
    let default_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/intensification_ladder.json");

    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match LadderConfig::from_file(&path) {
            Ok(config) => {
                tracing::info!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    "intensification_ladder.loaded=file"
                );
                return (Arc::new(config), LadderConfigMetadata::new(Some(path)));
            }
            // A *broken invariant* is louder than a missing file: the ladder parsed, so it looks
            // fine, and silently falling back to the builtin would hide a ladder the operator
            // believes is live (the `fauna_config.invalid_rejected` convention).
            Err(err @ LadderConfigError::Invalid { .. }) => {
                tracing::error!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "intensification_ladder.invalid_rejected"
                );
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "intensification_ladder.load_failed"
                );
            }
        }
    }

    let config = LadderConfig::builtin();
    tracing::info!(
        target: "shadow_scale::config",
        "intensification_ladder.loaded=builtin"
    );
    (config, LadderConfigMetadata::new(None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// Mutate the builtin ladder JSON and expect `validate` (inside `from_json_str`) to reject it —
    /// the `FaunaConfig::validate` rejection-test convention, one case per bound.
    fn reject(mutate: impl FnOnce(&mut Value)) -> LadderConfigError {
        let mut json: Value =
            serde_json::from_str(BUILTIN_INTENSIFICATION_LADDER).expect("builtin parses as json");
        mutate(&mut json);
        LadderConfig::from_json_str(&json.to_string())
            .expect_err("mutated ladder should be rejected")
    }

    fn assert_rejects(err: LadderConfigError, expect_field: &str) {
        match err {
            LadderConfigError::Invalid { field, .. } => assert!(
                field.contains(expect_field),
                "expected a rejection naming '{expect_field}', got '{field}'"
            ),
            other => panic!("expected an Invalid rejection, got {other:?}"),
        }
    }

    /// Index of a rung record in the builtin's `rungs` array.
    fn rung_index(json: &Value, branch: &str, id: &str) -> usize {
        json["rungs"]
            .as_array()
            .expect("rungs is an array")
            .iter()
            .position(|rung| rung["branch"] == branch && rung["id"] == id)
            .unwrap_or_else(|| panic!("builtin defines the {branch}:{id} rung"))
    }

    #[test]
    fn builtin_ladder_parses_and_validates() {
        let ladder = LadderConfig::builtin();
        // Both branches, all five coded rungs.
        for key in RungKey::ALL {
            let rung = ladder.rung(key);
            assert_eq!(rung.branch, key.branch());
            assert_eq!(rung.id, key.id());
        }
    }

    /// The ladder must describe **what the sim does today**, not the target model — later slices
    /// change behaviour by editing it. Pin the current truth so a drifting edit is caught here.
    #[test]
    fn builtin_ladder_describes_todays_rungs() {
        let ladder = LadderConfig::builtin();

        // Rung 1 already teaches (§0: practice-earns-knowledge is shipped on both tracks) but is
        // driven by no verb — you don't *build* a wild source.
        let plant_wild = ladder.rung(RungKey::PlantWild);
        assert_eq!(plant_wild.verb_policy(), None);
        assert_eq!(
            plant_wild.earns_discovery_id(),
            Some(CULTIVATION_DISCOVERY_ID)
        );
        let animal_wild = ladder.rung(RungKey::AnimalWild);
        assert_eq!(animal_wild.verb_policy(), None);
        assert_eq!(animal_wild.earns_discovery_id(), Some(HERDING_DISCOVERY_ID));

        // Plant rung 2 — the shipped Cultivate investment, gated on Cultivation, teaching nothing.
        let tended = ladder.rung(RungKey::PlantTended);
        assert_eq!(tended.verb_policy(), Some(FollowPolicy::Cultivate));
        assert_eq!(tended.unlock_discovery_id(), Some(CULTIVATION_DISCOVERY_ID));
        assert_eq!(tended.earns_discovery_id(), None);

        // Animal rung 2 — the `Tame` investment: an explicit, Herding-gated, *paid* verb. This is
        // the conflation fix (§4.1): the rung is driven by its own verb, not by a Sustain harvest.
        let pastoral = ladder.rung(RungKey::AnimalPastoral);
        assert_eq!(pastoral.verb_policy(), Some(FollowPolicy::Tame));
        assert_eq!(pastoral.unlock_discovery_id(), Some(HERDING_DISCOVERY_ID));
        assert_eq!(pastoral.earns_discovery_id(), None);
        assert_eq!(pastoral.ceiling_required, Some(HusbandryCeiling::Pastoral));
        // Taming now costs yield, like every other rung — the `domesticate` early-claim that let a
        // player skip this investment is gone.
        assert!(
            pastoral
                .yield_fraction_while_building()
                .is_some_and(|dip| dip > 0.0 && dip < 1.0),
            "the pastoral rung is an investment — it must dip the take while building"
        );

        // Animal rung 3 — the shipped Corral investment, gated on Herding (the §4.3 reshuffle to
        // Penning is a later slice), fenced only by a `pen`-ceiling species.
        let pen = ladder.rung(RungKey::AnimalPen);
        assert_eq!(pen.verb_policy(), Some(FollowPolicy::Corral));
        assert_eq!(pen.unlock_discovery_id(), Some(HERDING_DISCOVERY_ID));
        assert_eq!(pen.ceiling_required, Some(HusbandryCeiling::Pen));
    }

    /// The engine drives a rung only through its own verb, and only when the caller's gates hold.
    #[test]
    fn build_accrual_is_driven_by_the_rungs_own_verb() {
        let ladder = LadderConfig::builtin();
        let tended = ladder.rung(RungKey::PlantTended);
        let build = tended.build.as_ref().expect("tended rung builds");

        assert_eq!(
            tended.build_accrual(FollowPolicy::Cultivate, true, RUNG_TIMESCALE_UNSCALED),
            build.progress_per_turn
        );
        // Wrong verb → nothing, even though the crew is working the patch.
        assert_eq!(
            tended.build_accrual(FollowPolicy::Sustain, true, RUNG_TIMESCALE_UNSCALED),
            0.0
        );
        // Right verb, gate lapsed → nothing accrues (progress is neither lost nor advanced).
        assert_eq!(
            tended.build_accrual(FollowPolicy::Cultivate, false, RUNG_TIMESCALE_UNSCALED),
            0.0
        );
        assert_eq!(
            tended.build_decay(RUNG_TIMESCALE_UNSCALED),
            build.decay_per_turn
        );
        assert_eq!(
            tended.yield_fraction_while_building(),
            Some(build.yield_fraction_while_building)
        );
    }

    /// A rung with **no verb** is never driven — the `wild` rungs, which are nothing to *build*:
    /// you take what is there. (Retargeted from `pastoral`, which used to be the verbless example
    /// because taming accrued implicitly off a Sustain hunt; it now has the `tame` verb, so `wild`
    /// is what is left to make this point with.)
    #[test]
    fn a_verbless_rung_is_never_driven() {
        let ladder = LadderConfig::builtin();
        for key in [RungKey::AnimalWild, RungKey::PlantWild] {
            let wild = ladder.rung(key);
            for policy in [
                FollowPolicy::Sustain,
                FollowPolicy::Surplus,
                FollowPolicy::Market,
                FollowPolicy::Eradicate,
                FollowPolicy::Cultivate,
                FollowPolicy::Tame,
                FollowPolicy::Corral,
            ] {
                assert_eq!(
                    wild.build_accrual(policy, true, RUNG_TIMESCALE_UNSCALED),
                    0.0
                );
            }
            assert_eq!(wild.build_decay(RUNG_TIMESCALE_UNSCALED), 0.0);
            assert_eq!(wild.yield_fraction_while_building(), None);
        }
    }

    /// **Taming must out-run its own decay**, or no herd could ever be tamed by sustained work.
    /// Relocated from `fauna_config::tests::validate_rejects_taming_that_cannot_outrun_its_decay`
    /// with the dials themselves: the bound now lives on the rung that owns them, and
    /// `LadderConfig::validate` applies it to every rung of both webs rather than each web
    /// re-asserting its own copy.
    #[test]
    fn rejects_taming_that_cannot_outrun_its_decay() {
        let err = reject(|json| {
            let idx = rung_index(json, "animal", "pastoral");
            json["rungs"][idx]["build"]["decay_per_turn"] = (0.04).into();
        });
        assert_rejects(err, "animal:pastoral");
    }

    /// **Sustain is not a taming verb** — the §4.1 de-conflation, asserted at the engine seam: the
    /// `pastoral` rung's meter advances under `Tame` and under nothing else. The sim-level twin of
    /// this (a Sustain hunt leaves `domestication_progress` at zero) lives in the labor tests.
    #[test]
    fn the_pastoral_rung_is_driven_by_tame_and_only_by_tame() {
        let ladder = LadderConfig::builtin();
        let pastoral = ladder.rung(RungKey::AnimalPastoral);
        let build = pastoral.build.as_ref().expect("the pastoral rung builds");

        assert_eq!(
            pastoral.build_accrual(FollowPolicy::Tame, true, RUNG_TIMESCALE_UNSCALED),
            build.progress_per_turn
        );
        for policy in [
            FollowPolicy::Sustain,
            FollowPolicy::Surplus,
            FollowPolicy::Market,
            FollowPolicy::Eradicate,
            FollowPolicy::Cultivate,
            FollowPolicy::Corral,
        ] {
            assert_eq!(
                pastoral.build_accrual(policy, true, RUNG_TIMESCALE_UNSCALED),
                0.0,
                "{policy:?} must not tame a herd — only Tame does"
            );
        }
        // Right verb, gate lapsed → nothing accrues (progress is neither lost nor advanced).
        assert_eq!(
            pastoral.build_accrual(FollowPolicy::Tame, false, RUNG_TIMESCALE_UNSCALED),
            0.0
        );
    }

    #[test]
    fn rejects_a_duplicate_rung_id() {
        let err = reject(|json| {
            let idx = rung_index(json, "plant", "tended");
            json["rungs"][idx]["id"] = "wild".into();
        });
        assert_rejects(err, "plant:wild");
    }

    #[test]
    fn rejects_a_duplicate_rung_order() {
        let err = reject(|json| {
            let idx = rung_index(json, "animal", "pen");
            json["rungs"][idx]["order"] = (2).into();
        });
        assert_rejects(err, "animal:pen");
    }

    /// Every branch needs its wild source. (A branch that merely *renumbers* its rungs is caught
    /// earlier and more precisely by the sequential check; this guard is what's left — a branch with
    /// no ladder at all, which would otherwise read as "this food web cannot be intensified" with
    /// nothing on the map to explain it.)
    #[test]
    fn rejects_a_branch_without_exactly_one_first_rung() {
        let err = reject(|json| {
            let rungs = json["rungs"].as_array_mut().expect("array");
            rungs.retain(|rung| rung["branch"] != "plant");
        });
        assert_rejects(err, "rungs[plant]");
    }

    #[test]
    fn rejects_a_first_rung_that_requires_something() {
        let err = reject(|json| {
            let idx = rung_index(json, "animal", "wild");
            json["rungs"][idx]["requires_rung"] = "pastoral".into();
        });
        assert_rejects(err, "animal:wild");
    }

    #[test]
    fn rejects_a_rung_that_requires_nothing_below_it() {
        let err = reject(|json| {
            let idx = rung_index(json, "plant", "tended");
            json["rungs"][idx]["requires_rung"] = Value::Null;
        });
        assert_rejects(err, "plant:tended");
    }

    #[test]
    fn rejects_a_rung_that_skips_a_step() {
        let err = reject(|json| {
            let idx = rung_index(json, "animal", "pen");
            json["rungs"][idx]["requires_rung"] = "wild".into();
        });
        assert_rejects(err, "animal:pen");
    }

    #[test]
    fn rejects_a_rung_requiring_a_rung_that_does_not_exist() {
        let err = reject(|json| {
            let idx = rung_index(json, "animal", "pen");
            json["rungs"][idx]["requires_rung"] = "paddock".into();
        });
        assert_rejects(err, "animal:pen");
    }

    #[test]
    fn rejects_a_verb_that_is_not_a_policy() {
        let err = reject(|json| {
            let idx = rung_index(json, "plant", "tended");
            json["rungs"][idx]["verb"] = "plough".into();
        });
        assert_rejects(err, "plant:tended");
    }

    #[test]
    fn rejects_an_unknown_unlock_knowledge() {
        let err = reject(|json| {
            let idx = rung_index(json, "animal", "pen");
            json["rungs"][idx]["unlock_knowledge"] = "penning".into();
        });
        assert_rejects(err, "animal:pen");
    }

    #[test]
    fn rejects_an_unknown_earns_knowledge() {
        let err = reject(|json| {
            let idx = rung_index(json, "plant", "wild");
            json["rungs"][idx]["earns_knowledge"] = "seed_selection".into();
        });
        assert_rejects(err, "plant:wild");
    }

    #[test]
    fn rejects_a_non_building_progress_rate() {
        let err = reject(|json| {
            let idx = rung_index(json, "plant", "tended");
            json["rungs"][idx]["build"]["progress_per_turn"] = (0.0).into();
        });
        assert_rejects(err, "plant:tended");
    }

    #[test]
    fn rejects_decay_that_outruns_the_build() {
        let err = reject(|json| {
            let idx = rung_index(json, "plant", "tended");
            json["rungs"][idx]["build"]["decay_per_turn"] = (0.04).into();
        });
        assert_rejects(err, "plant:tended");
    }

    #[test]
    fn rejects_negative_decay() {
        let err = reject(|json| {
            let idx = rung_index(json, "plant", "tended");
            json["rungs"][idx]["build"]["decay_per_turn"] = (-0.01).into();
        });
        assert_rejects(err, "plant:tended");
    }

    #[test]
    fn rejects_a_free_investment() {
        let err = reject(|json| {
            let idx = rung_index(json, "animal", "pen");
            json["rungs"][idx]["build"]["yield_fraction_while_building"] = (1.0).into();
        });
        assert_rejects(err, "animal:pen");
    }

    #[test]
    fn rejects_a_starving_investment() {
        let err = reject(|json| {
            let idx = rung_index(json, "animal", "pen");
            json["rungs"][idx]["build"]["yield_fraction_while_building"] = (0.0).into();
        });
        assert_rejects(err, "animal:pen");
    }

    #[test]
    fn rejects_a_ladder_missing_a_rung_the_engine_drives() {
        let err = reject(|json| {
            let idx = rung_index(json, "animal", "pen");
            json["rungs"].as_array_mut().expect("array").remove(idx);
        });
        assert_rejects(err, "animal:pen");
    }
}
