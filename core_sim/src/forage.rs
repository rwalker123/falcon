//! Depletable forage patches (Intensification §0-ii — "forage parity with hunting").
//!
//! Transposes the herd biomass / logistic-regrowth model (`fauna.rs`) onto worked forage tiles.
//! Every `FoodModuleTag` tile gains a live per-patch `{ biomass, carrying_capacity, ecology_phase }`
//! (`ForagePatch`) held in the authoritative `ForageRegistry` resource, keyed by tile coord.
//!
//! **This is the HUMAN food web** — seeds, nuts, tubers, fruit, shellfish — and its capacity is a
//! property of the **land**: `forage.capacity_by_biome`, a per-biome table over the 37 biomes
//! (`labor_config.json`). Its twin is the *animal* food web, `graze.capacity_by_biome`
//! (`fauna_config.json`, `graze.rs`), and **the two are meant to disagree**: a closed-canopy
//! woodland is rich in mast and poor in pasture, a prairie steppe is the reverse, a silt floodplain
//! is cropland rather than range. *Your best farm is not your best pasture*
//! (`docs/plan_grazing_foundation.md` §1). The `FoodModuleTag` still decides what **kind** of
//! gathering a tile offers (and its `seasonal_weight`); the table decides **how much** is there.
//! Foraging **draws the patch down** (`forage_take`), and `advance_forage_regrowth` regrows it each
//! turn toward `carrying_capacity`. The patch's state round-trips through rollback because the
//! checkpoint carries the whole `ForageRegistry` (`SimState::forage`) — the same way the
//! `HerdRegistry` persists.
//!
//! Unlike a wild herd, a patch uses **pure logistic regrowth** (no Allee / critical-depensation
//! crash) and **never despawns** — plants reseed, so a depleted (feral) patch always recovers. A
//! small **reseed floor** (`forage.reseed_floor_fraction × carrying_capacity`) lifts a fully-depleted
//! patch back to a seed stock before regrowth each turn, so even a patch driven to exactly `0`
//! (Eradicate / f32 underflow / a restored `biomass = 0`) recovers rather than sticking at `0`. The
//! Allee branch of `net_biomass_delta` (via `sustainable_yield`) still sizes the **Sustain** gather
//! ceiling (so a collapsed patch yields no sustainable surplus). Foraging honors the full policy axis
//! (Sustain/Surplus/Deplete/Eradicate — §0-iii, parity with hunting): the `LaborTarget::Forage`
//! policy flows through `advance_labor_allocation` into `forage_take`, and a Deplete gather sells its
//! take as trade goods.
//!
//! **Cultivation** (Phase 1a) is the plant mirror of `fauna.rs`'s corral — an *investment*, not a
//! by-product of gathering (authoritative spec: `core_sim/CLAUDE.md` → Cultivation):
//! - A **Sustain** forage on a **Thriving** patch earns the faction **Cultivation knowledge**
//!   (`CULTIVATION_DISCOVERY_ID`, in the `DiscoveryProgressLedger`) — the gate on the policy below.
//!   Sustain **never** accrues a patch's `cultivation_progress`.
//! - Taming a patch means **paying the `Cultivate` policy's investment**: a reduced take
//!   (the `plant:tended` rung's `yield_fraction_while_building ×` the Sustain/MSY ceiling — read off
//!   the shared ladder, `crate::intensification`) while `cultivation_progress` accrues
//!   toward `1.0`. The `cultivate` command only **sets that policy** on bands already foraging the
//!   tile; it claims nothing.
//! - A completed ("tended") patch pays only the band that **tends it** (a Forage assignment worked it
//!   this turn — place-local, in `advance_labor_allocation`) a higher-than-wild yield without drawing
//!   biomass down; `advance_cultivation` takes an **untended** patch **feral** (progress decays back
//!   below the cultivated threshold, reverting it to a wild gather patch).
//!
//! **The Field** (rung 3, slice 5) is the same patch one rung up: `Sow` fills `field_progress`, and a
//! completed Field pays its workers `biomass × field_provisions_per_biomass` — the tended patch's
//! shape at twice the rate. It needs no *patch* below it: sowing a qualifying tile that spawned none
//! *creates* one (`ForagePatch::sown`), at that tile's own biome capacity.
//!
//! **Where it may be sown is SCARCE, and that is the mechanic** — the tile must be a **gathering
//! site** (the curated `FoodSiteRegistry`) *and* **near fresh water**: rung 3 can move seed but not
//! water, and does not yet work ground its people do not already gather. The `plant:field` rung's
//! `site_requirement` states it; `rung_site_refusal` + `tile_is_fresh_watered` are the one seam the
//! command, the labor arm and the wire all judge through.
//!
//! **This reversed the earlier rule that Sow "needs no source below it — seed travels", under which
//! any sufficiently fertile, watered tile was a legal target (46 of 4160 on the standard map).** The
//! problem was reach: gathering is itself site-bound, so the only tiles a band works are gathering
//! sites, and ground that qualified on fertility alone could never be occupied to sow. "Seed travels"
//! is now rung 4 (Farm)'s identity — the first rung to drop the gathering-site term, with a fertility
//! floor back in its place. Design: `docs/plan_intensification_ladder.md` §2.

use std::{borrow::Cow, collections::HashMap};

use bevy::prelude::*;

use crate::{
    components::{Improvement, SourceYield, Tile},
    fauna::{
        classify_ecology_phase, escapement_ceiling, forecast_source_yield,
        reseeding_logistic_regrowth, sustainable_yield, EcologyPhase, SourceYieldForecast,
        NO_PASTORAL_YIELD,
    },
    fauna_config::{EcologyConfig, YieldAccounts},
    flora_config::{FloraConfig, FloraShare},
    food::FoodModuleTag,
    intensification::{
        BuildDips, LadderConfig, LadderConfigHandle, RungBranch, RungDef, RungKey, SiteRefusal,
        NEGLECT_NONE, RUNG_COMPLETE, RUNG_TIMESCALE_UNSCALED, RUNG_UNSTARTED,
    },
    labor_config::{ForageLaborConfig, LaborConfigHandle, NO_FORAGE_CAPACITY},
    orders::FactionId,
    resources::{CommandEventEntry, CommandEventKind, CommandEventLog, SimulationTick},
    scalar::{scalar_from_f32, Scalar},
};

/// Discovery id for the faction-level **Cultivation** knowledge (Intensification Rung 1b — the
/// earned-knowledge gate on the plant path, `docs/plan_intensification.md` §4b). Knowledge is
/// **earned by doing**: a band Sustain-foraging a Thriving patch accrues this discovery in the
/// per-faction `DiscoveryProgressLedger` (`advance_labor_allocation`), and a patch cannot become a
/// tended crop until the faction knows Cultivation. Declared as a start-profile knowledge tag
/// (`cultivation` → this id in `data/start_profile_knowledge_tags.json`) purely so it is mappable;
/// it is deliberately **not** listed in any start profile's `starting_knowledge_tags`, so no faction
/// starts knowing it. Next free id after `nomadic_wayfinding` (2001) / `portable_forge` (2002).
pub const CULTIVATION_DISCOVERY_ID: u32 = 2003;

/// Discovery id for the faction-level **Seed Selection** knowledge — the plant ladder's **rung-3**
/// gate (`docs/plan_intensification_ladder.md` §2a/§4.3), and the twin of `fauna::PENNING_DISCOVERY_ID`.
///
/// **Earned by practising rung 2**: working a *tended* patch under a stewardship policy teaches it
/// (`RungDef::knowledge_earned`, driven by the `plant:tended` rung's `earns_knowledge`) — you learn
/// to select seed by *farming*, not by gathering wild stands. Like every other ladder knowledge it is
/// declared as a start-profile knowledge tag (`seed_selection` → this id in
/// `data/start_profile_knowledge_tags.json`) purely so it is mappable, and is deliberately **not**
/// listed in any start profile's `starting_knowledge_tags` — nothing on the ladder is start-granted.
///
/// **Its consumer landed in slice 5**: it gates the `Sow` verb (the `plant:field` rung's
/// `unlock_knowledge`), so a faction may only place a Field once it has learned to select seed by
/// farming. Earned in slice 4, spent here — a knowledge you accumulate before its verb exists is
/// exactly the "practice paces the ladder" model. Next free id after `herding` (2004).
pub const SEED_SELECTION_DISCOVERY_ID: u32 = 2005;

/// **The gather season of a tile with no `FoodModuleTag`** — i.e. no wild gather at all: the season
/// scales a forager's *throughput* (`forage_per_worker_biomass`), so a zero here means no worker can
/// gather anything there, which is exactly right for ground the wild put no food site on.
///
/// It became a reachable reading in slice 5: `Sow` places a Field on any ground the `plant:field`
/// rung's `site_requirement` accepts — module or not — so a patch may now stand on a tile with no
/// module. Such a patch offers nothing to
/// **gather** — the only thing to work there is the crop you sowed, whose managed harvest is
/// biomass-based and seasonless (`field_provisions`). Shared by the Forage labor arm, the assign-time
/// yield seed and the snapshot forecast, so all three read the same "no season" answer.
pub const NO_FORAGE_SEASON: f32 = 0.0;

/// **The season a MANAGED harvest is worked at** — full weight, always. A Field's crop is not a wild
/// stand whose bounty comes and goes with the year: it is standing where you planted it, and its
/// harvest is biomass-based and seasonless (`field_provisions`). So the crew's collection cap on it
/// reads the throughput at full season rather than the tile's `FoodModuleTag::seasonal_weight`.
///
/// **Load-bearing, not cosmetic:** `Sow` may place a Field on ground with **no food module at all**
/// (slice 5), whose gather season is [`NO_FORAGE_SEASON`] — zero. Capping a Field's collection by that
/// would let a crew carry home exactly nothing from the rung the whole arc climbs toward.
const MANAGED_HARVEST_SEASON: f32 = 1.0;

/// A live depletable forage patch on a `FoodModuleTag` tile. Mirrors the herd biomass model's
/// ecology subset, including cultivation (`cultivation_progress`/`owner`) — the plant analog of a
/// herd's domestication (Phase 1a).
#[derive(Debug, Clone)]
pub struct ForagePatch {
    /// Tile the patch sits on (its registry key).
    pub tile: UVec2,
    /// Live gatherable stock, drawn down by `forage_take`, regrown by `advance_forage_regrowth`.
    pub biomass: f32,
    /// Per-patch carrying cap that biomass regrows toward — **the tile's**, seeded from
    /// `forage.capacity_by_biome[terrain]` (the human food web's per-biome table), never a global
    /// constant. The exact counterpart of `GrazePatch::carrying_capacity`.
    pub carrying_capacity: f32,
    /// Coarse health band (Thriving/Stressed/Collapsing), recomputed each turn from biomass vs
    /// `carrying_capacity`. Lights the client over-forage readout the same way herds do.
    pub ecology_phase: EcologyPhase,
    /// Cultivation progress in `[0.0, 1.0]`; `1.0` = cultivated. Accrues **only** while a band works
    /// this patch with the [`crate::components::Improvement::Cultivate`] verb in flight (faction knows
    /// Cultivation + patch Thriving); decays on a patch nobody is working (see `advance_cultivation`).
    /// The plant mirror of `Herd::corral_progress`.
    pub cultivation_progress: f32,
    /// **Field**-build progress in `[0.0, 1.0]`; `1.0` = a sown Field (the plant ladder's **rung 3**).
    /// Accrues only while a band works this patch with [`crate::components::Improvement::Sow`] in
    /// flight (faction knows **Seed Selection**); decays on a patch nobody is working (see
    /// `advance_cultivation`). The plant mirror of `Herd::corral_progress` — and, exactly like the
    /// herd's two meters, it is **its own** meter rather than a second reading of
    /// `cultivation_progress`: a branch with two investment rungs carries two meters, one per rung.
    ///
    /// **Independent of `cultivation_progress`, deliberately.** `Sow` needs no prior patch (§2 — seed
    /// travels), so a Field may stand on ground that was never tended, and a Field that lapses simply
    /// reveals whatever rung the tile still supports underneath (today: wild, since the same untended
    /// turn bleeds both meters).
    pub field_progress: f32,
    /// **The named plant this patch is COMMITTED to** — a `flora_config.json` species key, or `None`
    /// for the **wild mixed basket** (`docs/plan_flora_roster.md` §4.2/§4.3). Stored as the config
    /// key rather than the display name because the key is what `FloraConfig::species` and
    /// `FloraShare::species` are keyed by; the animal side stores a display name on `Herd::species`
    /// only because *its* roster is looked up that way.
    ///
    /// **Set on the first turn a crew works this patch under `Cultivate` or `Sow`** (the assignment's
    /// selection, or the highest-share species in this tile's basket that the rung's
    /// `cultivation_ceiling` permits), and fixed from then on. **Cleared when both improvement meters
    /// lapse to zero** ([`ForagePatch::reconcile_owner`]) — a patch that has gone fully feral is a
    /// wild stand again, and a wild stand is the whole basket.
    ///
    /// What the commitment *does* is two things and only two ([`patch_composition`] /
    /// [`patch_provisions_per_biomass`]): it **reweights** the tile's basket toward this one plant
    /// (weeding at rung 2, planting at rung 3 — the tile's `K` never moves, because the land owns
    /// it), and it changes how well biomass **converts** in every account (the tended rung's
    /// `tended_conversion_gain`, on this species' term alone). Both take effect only once the
    /// improvement is *complete* — while a crew is still preparing, the stand is still the mixed
    /// basket it started as.
    pub species: Option<String>,
    /// Faction tending/owning this patch (`Some` iff either improvement meter is `> 0`).
    pub owner: Option<FactionId>,
    /// Transient per-turn flag: a Forage assignment **worked this patch as an improvement** this turn
    /// — tending a completed patch/Field, or preparing one under `Cultivate`/`Sow` (set in
    /// `advance_labor_allocation`, Population). `advance_cultivation` (Logistics, the *next* turn —
    /// Logistics runs before Population) reads it to decide feral/decay vs. spared, then clears it.
    /// Sparing a *preparing* patch too is what makes the investment accrue at the full
    /// `progress_per_turn` (25 turns) rather than net-of-decay. **Not** on the client wire (derived,
    /// transient), but it **does survive a rollback**: the checkpoint clones the whole
    /// `ForageRegistry` (`SimState::forage`), so a restored patch resumes with exactly the worked flag
    /// it was captured with. That is what keeps the first post-restore Logistics decay pass — which
    /// runs before the labor arm can re-mark a patch a band is working — from reverting a tended patch
    /// / Field a band tends every turn.
    pub tended_this_turn: bool,
    /// **How many consecutive turns nobody has worked this patch as an improvement** — the neglect
    /// counter `advance_cultivation` gates the feral bleed on. Reset to [`NEGLECT_NONE`] on any turn
    /// `tended_this_turn` is set; incremented on every turn it is not. The bleed applies only while
    /// this exceeds the decaying rung's [`RungBuild::grace_turns`], so a crew may be away for a few
    /// turns — re-tasked, raided, following a herd — without the patch starting to revert.
    ///
    /// **The requirement stays per-SOURCE, not per-band** (`tended_this_turn` is set by *any* crew on
    /// the tile), and it stays **binary**: a partly-crewed build accrues more slowly
    /// ([`RungDef::build_accrual`]'s crew scale) but is not *neglect*. Grading neglect by crew size is
    /// a separate decision.
    ///
    /// Rides the checkpoint with the rest of the registry, so a rollback rewinds the grace along with
    /// the meter it protects — otherwise a restore could hand a patch a fresh grace it had already
    /// spent.
    pub neglect_turns: u16,
}

impl ForagePatch {
    /// A fresh patch at full biomass (= carrying capacity). Phase is `Thriving` until refreshed
    /// against the ecology config.
    pub fn new(tile: UVec2, carrying_capacity: f32) -> Self {
        Self {
            tile,
            biomass: carrying_capacity,
            carrying_capacity,
            ecology_phase: EcologyPhase::Thriving,
            cultivation_progress: 0.0,
            field_progress: 0.0,
            species: None,
            owner: None,
            tended_this_turn: false,
            neglect_turns: NEGLECT_NONE,
        }
    }

    /// **A patch a crew has just put seed into** — the plant rung-3 verb's create-from-nothing case
    /// ([`crate::components::Improvement::Sow`] on hospitable ground that carried no forage site at all,
    /// `docs/plan_intensification_ladder.md` §2). It is an ordinary patch from this moment on: same
    /// biomass model, same **tile** capacity (`tile_forage_capacity` — the *same* source a wild patch
    /// is seeded from, never a Field-specific table), same logistic regrowth.
    ///
    /// It starts at the **reseed floor**'s standing crop, not at capacity: sown ground is seed, and
    /// the floor is already this module's word for "the smallest stand plants recover from". So a new
    /// Field is worth nothing on the turn it is placed and grows into its yield — which is also why
    /// the `Sow` accrual is *not* gated on the patch being Thriving (see `advance_labor_allocation`):
    /// a freshly sown tile is Collapsing by construction, and gating it would make sowing bare ground
    /// impossible.
    pub(crate) fn sown(tile: UVec2, carrying_capacity: f32, reseed_floor_fraction: f32) -> Self {
        Self {
            biomass: carrying_capacity * reseed_floor_fraction,
            ..Self::new(tile, carrying_capacity)
        }
    }

    /// Recompute `ecology_phase` from the current biomass against the forage ecology config.
    pub(crate) fn refresh_ecology_phase(&mut self, ecology: &EcologyConfig) {
        self.ecology_phase = classify_ecology_phase(self.biomass, self.carrying_capacity, ecology);
    }

    /// A fully-cultivated ("tended crop") patch: pays the band that tends it a higher-than-wild yield
    /// each turn (place-local, in `advance_labor_allocation`) and is not gather-drawn. Reverts to a
    /// wild gather patch the moment `cultivation_progress` decays below `1.0` (feral — see
    /// `advance_cultivation`). The plant mirror of `Herd::is_domesticated`.
    pub fn is_cultivated(&self) -> bool {
        self.cultivation_progress >= RUNG_COMPLETE
    }

    /// A fully-sown **Field** (the plant ladder's rung 3): pays the band that works it a *higher*
    /// managed yield than a tended patch (`field_provisions`) and, like a tended patch, is not
    /// gather-drawn. Reverts the moment `field_progress` decays below `1.0` (see
    /// `advance_cultivation`). The plant mirror of `Herd::is_corralled`.
    pub fn is_field(&self) -> bool {
        self.field_progress >= RUNG_COMPLETE
    }

    /// Is this patch a **completed improvement** — a Field or a tended patch? The single predicate
    /// for "this source is worked, not gathered": its harvest is biomass-based and never overdraws
    /// (`sustainable == actual`, no ⚠) and one worker suffices
    /// ([`crate::fauna::TENDED_SOURCE_WORKERS_NEEDED`]). Both the payout path and the forecast branch
    /// on it, so the two cannot disagree about which patches are managed.
    pub fn is_managed(&self) -> bool {
        self.is_field() || self.is_cultivated()
    }

    /// Accrue cultivation progress for `faction` (the preparing band, working the patch with
    /// [`crate::components::Improvement::Cultivate`] in flight). Sets ownership on the first accrual;
    /// only the owner makes progress.
    /// Clamped to 1.0 — reaching it makes the patch a tended crop from the *next* turn's payout on
    /// (the accrual runs after this turn's take, so the pre-commit forecast can't lie). No-op once the
    /// patch is cultivated. Mirrors `Herd::accrue_corral`.
    ///
    /// **Returns `true` only when THIS call finished the rung** — `accrue_corral`'s convention, and
    /// load-bearing for the feed line: `handle_cultivate` sets the verb on *every* band working the
    /// patch, so a post-hoc `is_cultivated()` test would announce "Cultivated patch at (x, y)" once
    /// per band. Whether a band's *improvement* should be cleared is a different question (it should,
    /// whoever finished it) and is answered separately by the caller.
    pub(crate) fn accrue_cultivation(&mut self, faction: FactionId, amount: f32) -> bool {
        if self.is_cultivated() {
            return false;
        }
        if self.owner.is_none() {
            self.owner = Some(faction);
        }
        if self.owner != Some(faction) {
            return false;
        }
        self.cultivation_progress = (self.cultivation_progress + amount).min(RUNG_COMPLETE);
        self.is_cultivated()
    }

    /// Accrue **Field**-build progress for `faction` (the sowing band, working the patch with
    /// [`crate::components::Improvement::Sow`] in flight) — the exact twin of `accrue_cultivation` one
    /// rung up, with the same
    /// owner-locking, the same clamp, the same "no-op once complete", and the same
    /// this-call-finished-it return.
    pub(crate) fn accrue_field(&mut self, faction: FactionId, amount: f32) -> bool {
        if self.is_field() {
            return false;
        }
        if self.owner.is_none() {
            self.owner = Some(faction);
        }
        if self.owner != Some(faction) {
            return false;
        }
        self.field_progress = (self.field_progress + amount).min(RUNG_COMPLETE);
        self.is_field()
    }

    /// Decay cultivation progress toward zero by `amount`. Applies to **any** patch — a completed
    /// (`is_cultivated`) patch decays too (going feral once it drops below `1.0`, reverting to a wild
    /// gather patch); the *caller* (`advance_cultivation`) decides when to spare a worked patch.
    /// Mirrors `Herd::decay_domestication` (minus the domesticated short-circuit — a tended patch left
    /// untended is meant to go feral).
    ///
    /// **Returns `true` only when THIS call took the rung back below [`RUNG_COMPLETE`]** — the feral
    /// *edge*, the exact mirror of [`Self::accrue_cultivation`]'s "did this call finish it". The
    /// caller announces on that edge and nowhere else: a 25-turn investment's payoff has just been
    /// destroyed, and the feed says so once rather than every turn of the long bleed that follows.
    pub(crate) fn decay_cultivation(&mut self, amount: f32) -> bool {
        let was_cultivated = self.is_cultivated();
        self.cultivation_progress = (self.cultivation_progress - amount).max(0.0);
        self.reconcile_owner();
        was_cultivated && !self.is_cultivated()
    }

    /// Decay **Field**-build progress toward zero by `amount` — the rung-3 twin of
    /// `decay_cultivation`, and (unlike the pen, which is lost outright when its herd bolts) a
    /// *gradual* bleed for the same reason cultivation bleeds gradually: **a patch is a place and a
    /// herd is not**, so leftover progress still refers to the same ground.
    ///
    /// **Returns `true` only when THIS call took the rung back below [`RUNG_COMPLETE`]** — see
    /// [`Self::decay_cultivation`] for why the announcement rides the edge.
    pub(crate) fn decay_field(&mut self, amount: f32) -> bool {
        let was_field = self.is_field();
        self.field_progress = (self.field_progress - amount).max(0.0);
        self.reconcile_owner();
        was_field && !self.is_field()
    }

    /// **Commit this patch to one named plant** — the first turn a crew works it under
    /// `Cultivate`/`Sow` (`docs/plan_flora_roster.md` §4.3). Idempotent and one-way: a patch already
    /// committed keeps its plant, because *"which crop is this ground"* is exactly the decision the
    /// rung exists to make and re-deciding it for free every turn would erase it. The commitment is
    /// released only by going fully feral ([`Self::reconcile_owner`]).
    pub(crate) fn commit_species(&mut self, species: &str) {
        if self.species.is_none() {
            self.species = Some(species.to_string());
        }
    }

    /// Hold the `owner is Some ⟺ some improvement remains` invariant: ownership lapses only once
    /// **both** meters are spent, so a decaying Field doesn't strand a stale owner (which would block
    /// another faction from ever working the tile) and doesn't drop its owner while its cultivation —
    /// or its own remaining progress — is still standing.
    ///
    /// **The species commitment lapses on exactly the same edge**, and for the same reason: once
    /// nothing is left of either improvement the ground is a wild stand again, and a wild stand is
    /// the tile's whole mixed basket rather than one plant somebody once chose. Re-committing then
    /// costs the full build again, at whatever the tile now favours.
    fn reconcile_owner(&mut self) {
        if self.cultivation_progress <= 0.0 && self.field_progress <= 0.0 {
            self.owner = None;
            self.species = None;
        }
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct ForageRegistry {
    /// Live patches keyed by tile coord. Iteration order is non-deterministic; the snapshot capture
    /// sorts by coord for a stable rollback record.
    pub patches: HashMap<UVec2, ForagePatch>,
}

impl ForageRegistry {
    pub fn patch(&self, tile: UVec2) -> Option<&ForagePatch> {
        self.patches.get(&tile)
    }

    pub fn patch_mut(&mut self, tile: UVec2) -> Option<&mut ForagePatch> {
        self.patches.get_mut(&tile)
    }

    pub fn is_empty(&self) -> bool {
        self.patches.is_empty()
    }

    pub fn len(&self) -> usize {
        self.patches.len()
    }

    /// Number of **completed plant improvements** owned by `faction` — tended patches *and* sown
    /// Fields (`ForagePatch::is_managed`). Folded (with domesticated herds) into the sedentarization
    /// "domestication" signal — plant + animal domestication share one driver. The plant mirror of
    /// `HerdRegistry::domesticated_count`.
    ///
    /// It counts Fields deliberately: a Field is rung **3**, so reading it as *less* domesticated
    /// than the rung-2 patch below it would invert the signal (and a bare-ground Field carries no
    /// cultivation meter at all — see `ForagePatch::field_progress`).
    pub fn cultivated_count(&self, faction: FactionId) -> usize {
        self.patches
            .values()
            .filter(|patch| patch.is_managed() && patch.owner == Some(faction))
            .count()
    }
}

/// **Is this tile on or beside FRESH water?** — the water half of a rung's
/// [`RungSiteRequirement`], and the reason rung 3 lands in river valleys.
///
/// Three ways to be watered, all read off **existing** hydrology seams (`hydrology.rs` — this
/// invents no adjacency concept of its own):
/// 1. **The tile is fresh-water ground** (`TerrainTags::FRESHWATER`) — a floodplain, a river delta,
///    an oasis basin, a marsh, a lake, a navigable channel.
/// 2. **A river runs along one of its six sides** (`Tile::has_any_river_edge`) — the riverbank. This
///    is *the* edge-river primitive, and `generate_hydrology` sets it on **both** hexes flanking every
///    traced edge, so "I am on the river" needs no neighbour lookup at all.
/// 3. **A fresh-water hex is next door** — the lake shore, the bank of a navigable trunk. Odd-r hex
///    adjacency (`hex_neighbors_wrapped`, wrap-aware), the same adjacency gameplay and the client use.
///
/// **A salt coast is NOT water for this purpose.** `ContinentalShelf`, `TidalFlat`, `MangroveSwamp`
/// and `CoralShelf` are `COASTAL` without `FRESHWATER`; you cannot farm on sea spray, and admitting
/// them would hand every shoreline the rung-3 gate the rule exists to withhold.
///
/// `neighbor_tags` resolves a coord to that tile's tags (`None` = off-map / no tile). A closure rather
/// than a `&TileRegistry` + query pair because the two callers reach tiles differently — the `sow`
/// command through `&App`, the labor arm through its `Query` — and the *rule* must live in one place
/// even though the lookup cannot.
pub fn tile_is_fresh_watered(
    tile: &Tile,
    grid_width: u32,
    grid_height: u32,
    wrap_horizontal: bool,
    neighbor_tags: impl Fn(UVec2) -> Option<sim_runtime::TerrainTags>,
) -> bool {
    if tile
        .terrain_tags
        .contains(sim_runtime::TerrainTags::FRESHWATER)
        || tile.has_any_river_edge()
    {
        return true;
    }
    crate::grid_utils::hex_neighbors_wrapped(
        tile.position.x,
        tile.position.y,
        grid_width,
        grid_height,
        wrap_horizontal,
    )
    .any(|(x, y)| {
        neighbor_tags(UVec2::new(x, y))
            .is_some_and(|tags| tags.contains(sim_runtime::TerrainTags::FRESHWATER))
    })
}

/// **Does `rung`'s site requirement admit this tile?** — the one place the three readings a
/// [`RungSiteRequirement`] judges (whether the tile is a gathering site, its own forage capacity, and
/// whether it is fresh-watered) are gathered, so every gate on the plant branch — the `assign_labor`
/// Forage arm, `cultivate`, `sow`, and the wire's own refusal — resolves the *same* rule and they
/// cannot drift into disagreeing about which ground may be worked.
///
/// `gathering_site` is the caller's `FoodSiteRegistry::is_site` reading; it is passed IN rather than
/// looked up here so this stays a pure function of the rung and the ground, like the other two.
///
/// `None` = the rung asks nothing of the site, or the land permits it. `Some(refusal)` says **which**
/// way the ground fell short, so the caller can phrase each distinctly — they are different problems
/// with different answers (work a site instead, move, or wait for a rung that relaxes the dial).
pub fn rung_site_refusal(
    rung: &RungDef,
    tile: &Tile,
    forage: &ForageLaborConfig,
    gathering_site: bool,
    fresh_water: bool,
) -> Option<SiteRefusal> {
    rung.site_requirement.as_ref()?.refusal(
        gathering_site,
        tile_forage_capacity(forage, tile),
        fresh_water,
    )
}

/// THE forage-capacity of a tile — the single source the seeding path and the wire path both read,
/// so a navigable hex's seeded patch and its exported `forage_capacity` can never drift.
///
/// A `NavigableRiver` hex reads its **underlying** biome (`resource_terrain()`) plus the river
/// fishing bonus (`navigable_forage_capacity`, always `> 0` — a navigable river is always a fishery,
/// so it always seeds a patch even over a barren biome). Every other tile reads its own biome
/// (`resource_terrain()` == `terrain` there).
pub fn tile_forage_capacity(forage: &ForageLaborConfig, tile: &Tile) -> f32 {
    if tile.terrain == sim_runtime::TerrainType::NavigableRiver {
        forage.navigable_forage_capacity(tile.resource_terrain())
    } else {
        forage.capacity_for(tile.resource_terrain())
    }
}

/// THE named plants a tile's forage capacity is made of — the **flora twin of
/// [`tile_forage_capacity`]**, branching on exactly the same condition so the composition and the
/// capacity it decomposes can never disagree about a tile's shape.
///
/// A `NavigableRiver` hex has a **two-term** capacity (the valley it cut **plus** the fishery the
/// channel is), so it gets the blended basket ([`FloraConfig::realized_navigable_composition`]); every
/// other tile reads its own biome's basket. **The result is `Cow::Owned` on both arms** — since the
/// §10 realization addition each tile's basket is a freshly-built subset, so neither arm borrows.
///
/// Every caller (today: the snapshot capture) must go through this, never
/// [`FloraConfig::composition`] on a raw terrain: reading the underlying biome alone on a navigable
/// hex leaves that hex's fishery bonus **unnamed**, which breaks the decomposition ruling on a whole
/// class of tiles and is invisible to `validate_against_forage`.
/// **Now realizes per tile** (`docs/plan_flora_roster.md` §10): the affinity roster answers *what CAN
/// grow here*, and this seam answers *what IS growing here* — a seeded, deterministic subset keyed on
/// `(map_seed, tile)`, so two tiles of one biome carry different baskets. Every non-Sow-from-nothing
/// caller (display, wild gather, Cultivate, Sow-upgrade, and the wire `ForagePatchState.composition`)
/// reads the realized basket through this one function. Owned on both arms now, because realization
/// always produces a fresh subset.
pub fn tile_flora_composition<'a>(
    flora: &'a FloraConfig,
    forage: &ForageLaborConfig,
    tile: &Tile,
    map_seed: u64,
) -> Cow<'a, [FloraShare]> {
    if tile.terrain == sim_runtime::TerrainType::NavigableRiver {
        Cow::Owned(flora.realized_navigable_composition(
            tile.resource_terrain(),
            forage,
            tile.position,
            map_seed,
        ))
    } else {
        Cow::Owned(flora.realized_composition(tile.resource_terrain(), tile.position, map_seed))
    }
}

/// **The whole of a tile's basket** — `1.0`. The ceiling weeding may push a favored crop to, and the
/// share a planted Field's single crop holds. Named rather than a bare `1.0` because at both sites it
/// states *which* whole the number is one of: **the land owns `K`**, so a rung may only change what
/// the tile's constant production is *made of*, never how much of it there is
/// (`docs/plan_flora_roster.md` §4.3).
pub const WHOLE_BASKET: f32 = 1.0;

/// **The conversion gain a species the patch is NOT committed to converts at** — the identity. The
/// volunteers still standing in a tended field are still wild, so only the favored term is multiplied
/// (see [`basket_rate`]).
const NO_CONVERSION_GAIN: f32 = 1.0;

/// **A remaining share this small is subtraction residue, not a plant.** `weeded` takes the whole of
/// an entry whenever this much or less would be left of it, so a favored crop weeded all the way to
/// the [`WHOLE_BASKET`] leaves a basket of exactly one species rather than one species plus ~1e-8 of
/// a ghost. Orders of magnitude below any realized share (the smallest in the shipped roster is a few
/// percent) and below the wire's own zero-share filter.
const VANISHED_SHARE: f32 = 1e-6;

/// **How far out of balance a weeded basket may land before it is a bug** — pure f32 slack. `weeded`
/// moves `delta` out of the other species and into the favored one, so both the "the others could
/// cover it" balance and the "still sums to 1" invariant are exact in real arithmetic and only ever
/// off by accumulated rounding here.
const WEEDING_BALANCE_EPSILON: f32 = 1e-3;

/// **THE effective-basket seam** — the plants a patch's biomass is ACTUALLY made of right now
/// (`docs/plan_flora_roster.md` §4.3). Every yield rate on the plant web is the share-weighted
/// average of this, at *every* rung including wild, which is what makes "a tile's production is
/// constant across rungs 1–3; a rung changes only which plants it is made of" a property of the code
/// rather than a claim about it.
///
/// - **wild** (uncommitted, or an improvement still building) — the tile's realized basket verbatim.
///   A patch still being cleared reads the tile basket for the same reason it always did: the crew
///   has not displaced anything yet, and both halves of a commitment switch on together at completion.
/// - **tended** (rung 2) — [`weeded`]: the favored crop's share rises to `min(1, share × gain)`, taken
///   from the least abundant remaining species first. That *is* weeding.
/// - **field** (rung 3) — [`planted`]: one entry, the crop, at [`WHOLE_BASKET`]. You sowed it.
///
/// Borrowed on the wild arm (`Cow`), because that arm is >99% of patches and this is resolved inside
/// the forward-projection loops — deep-copying a `String` per named plant per simulated turn is the
/// cost the memo in `snapshot/flora_quotes.rs` exists to avoid paying elsewhere.
pub fn patch_composition<'a>(
    patch: &ForagePatch,
    tile_composition: &'a [FloraShare],
    forage: &ForageLaborConfig,
) -> Cow<'a, [FloraShare]> {
    composition_for_rung(patch, tile_composition, forage, standing_rung(patch))
}

/// **The rung a patch actually STANDS on**, as a [`RungKey`] — sown → `plant:field`, cultivated →
/// `plant:tended`, else `plant:wild`. The ladder-free twin of [`patch_rung`] (which resolves a whole
/// `RungDef` and therefore needs the ladder config); this one exists because the rate seams need the
/// *key* and nothing else, on paths that carry no ladder.
fn standing_rung(patch: &ForagePatch) -> RungKey {
    if patch.is_field() {
        RungKey::PlantField
    } else if patch.is_cultivated() {
        RungKey::PlantTended
    } else {
        RungKey::PlantWild
    }
}

/// **The basket this patch's crop would make of the tile STANDING ON `rung`** — the seam every
/// *quote* reads, and the reason a rung's payoff can never be assembled out of another rung's
/// composition.
///
/// - `PlantField` → [`planted`]: one entry, the crop, holding the whole basket.
/// - `PlantTended` → [`weeded`]: the favored share rises to `min(1, share × tended_weeding_gain)`.
/// - anything below → the tile's basket verbatim; there is nothing a rung-1 stand reweights.
///
/// **It answers the rung it is ASKED about, never the rung the patch happens to stand on**, and that
/// is load-bearing. `fieldYield` is published for *every* patch — including a tended one — so a Field
/// quote that read the asking patch's own rung would hand the rung-3 number rung 2's weeded basket
/// *and* its conversion gain, overstating it by roughly `tended_conversion_gain`: a published quote
/// disagreeing with what the sim would pay, which is exactly the class of bug
/// [`commit_yield_ratio`]'s history records. It is the same rule `hypothetical_patch`'s per-rung
/// standing crop and the forecast's separate `ceiling_cultivate`/`ceiling_sow` already encode — **two
/// investment rungs on one branch never share a number.**
///
/// [`patch_composition`] is this seam at the patch's own [`standing_rung`], which is the *live*
/// reading the take path and the wire's published basket want.
pub fn composition_for_rung<'a>(
    patch: &ForagePatch,
    tile_composition: &'a [FloraShare],
    forage: &ForageLaborConfig,
    rung: RungKey,
) -> Cow<'a, [FloraShare]> {
    let Some(favored) = patch.species.as_deref() else {
        return Cow::Borrowed(tile_composition);
    };
    match rung {
        RungKey::PlantField => Cow::Owned(planted(favored)),
        RungKey::PlantTended => Cow::Owned(weeded(
            tile_composition,
            favored,
            forage.cultivation.tended_weeding_gain,
        )),
        _ => Cow::Borrowed(tile_composition),
    }
}

/// **WEEDING, stated once** — the rung-2 reweight: the favored crop's share rises to
/// `min(WHOLE_BASKET, share × gain)` and the increase is taken from the **least abundant remaining
/// species first** (share ASC, ties by species key ASC), each giving up `min(its share, what is left
/// to take)`. Entries emptied to `0` drop out; the result comes back in the wire's total order
/// (share DESC, then species key ASC) and still sums to [`WHOLE_BASKET`].
///
/// **Least abundant first is deliberate, and it is NOT "lowest-yielding".** Ranking by yield would
/// mean comparing a food rate against a trade rate — an exchange rate this codebase does not have and
/// should not invent. Abundance is currency-free, deterministic from the composition alone, and
/// independent of which crop was favored. Do not "improve" this to a yield ranking.
///
/// A `favored` the tile does not actually grow returns the basket verbatim: there is nothing to weed
/// toward.
fn weeded(composition: &[FloraShare], favored: &str, gain: f32) -> Vec<FloraShare> {
    let Some(share) = composition
        .iter()
        .find(|entry| entry.species == favored)
        .map(|entry| entry.share)
        .filter(|share| *share > NO_SHARE)
    else {
        return composition.to_vec();
    };
    let target = (share * gain).min(WHOLE_BASKET);
    let mut owed = target - share;
    // The others, LEAST ABUNDANT FIRST. Sorted before anything is summed — this output goes on the
    // wire, so a differently-ordered f32 addition is a snapshot-hash flake (`flora.md`).
    let mut others: Vec<FloraShare> = composition
        .iter()
        .filter(|entry| entry.species != favored)
        .cloned()
        .collect();
    others.sort_by(|a, b| {
        a.share
            .total_cmp(&b.share)
            .then_with(|| a.species.cmp(&b.species))
    });
    for entry in others.iter_mut() {
        let wanted = owed.max(0.0);
        // Take the whole entry whenever what would be left of it is f32 residue rather than a
        // plant — otherwise a saturating weed leaves ~1e-8 of a species standing, and a basket
        // that is supposed to be one crop publishes two.
        let taken = if entry.share - wanted <= VANISHED_SHARE {
            entry.share
        } else {
            wanted
        };
        entry.share -= taken;
        owed -= taken;
    }
    // `owed <= WHOLE_BASKET - share = Σ others`, so the others can always cover it.
    debug_assert!(
        owed <= WEEDING_BALANCE_EPSILON,
        "weeding {favored} to {target} left {owed} unpaid — the basket did not sum to 1"
    );
    let mut weeded: Vec<FloraShare> = others
        .into_iter()
        .filter(|entry| entry.share > NO_SHARE)
        .collect();
    weeded.push(FloraShare {
        species: favored.to_string(),
        share: target,
    });
    weeded.sort_by(|a, b| {
        b.share
            .total_cmp(&a.share)
            .then_with(|| a.species.cmp(&b.species))
    });
    debug_assert!(
        (weeded.iter().map(|entry| entry.share).sum::<f32>() - WHOLE_BASKET).abs()
            <= WEEDING_BALANCE_EPSILON,
        "a weeded basket must still be a whole basket"
    );
    weeded
}

/// **PLANTING, stated once** — the rung-3 reweight: one entry, the sown crop, holding the
/// [`WHOLE_BASKET`]. A Field has no volunteers.
fn planted(favored: &str) -> Vec<FloraShare> {
    vec![FloraShare {
        species: favored.to_string(),
        share: WHOLE_BASKET,
    }]
}

/// **The multiplier the FAVORED species' yield vector carries on `rung`** —
/// `cultivation.tended_conversion_gain` at rung 2, the identity at every other rung (a Field converts
/// at its own dial, `field_provisions_per_biomass`; a wild stand at nobody's).
///
/// **It applies to the favored term ONLY, and that is the whole point.** Tending is knowing *your*
/// crop; a blanket multiplier on the entire basket would make every commitment pay ~`gain` whatever
/// you favored, which erases the crop choice. On the favored term it *compounds* with weeding, so
/// favoring a dominant plant pays and favoring a marginal one barely moves. It multiplies the whole
/// vector — food, fodder and trade alike — so this stays commodity-generic with no `role` branch.
///
/// Keyed on the **rung being asked about**, exactly as [`composition_for_rung`] is, so the gain and
/// the basket it multiplies can never come from two different rungs.
fn favored_conversion_gain(rung: RungKey, forage: &ForageLaborConfig) -> f32 {
    match rung {
        RungKey::PlantTended => forage.cultivation.tended_conversion_gain,
        _ => NO_CONVERSION_GAIN,
    }
}

/// **What one unit of this patch's biomass would convert at STANDING ON `rung`** — the basket that
/// rung would make of the tile, priced through that rung's own conversion gain. The single seam every
/// per-rung rate below is one line of, so no consumer can pair one rung's basket with another's gain.
fn rung_rate(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    rung: RungKey,
    rate_of: impl Fn(&crate::flora_config::FloraDef) -> f32,
    fallback: f32,
) -> f32 {
    basket_rate(
        &composition_for_rung(patch, tile_composition, forage, rung),
        patch.species.as_deref(),
        favored_conversion_gain(rung, forage),
        flora,
        rate_of,
        fallback,
    )
}

/// **THE basket arithmetic**, stated once: `Σ shareᵢ × rate(speciesᵢ)`, with the **favored** species'
/// term multiplied by `favored_gain`. `fallback` when the basket is empty or names nothing the roster
/// knows — the only two ways a basket cannot be decomposed at all.
///
/// Commodity-generic by construction: `rate_of` picks the component (provisions / fodder / trade) off
/// the species' one yield vector, so all three accounts are priced through the same average and a
/// fourth costs a closure, not a branch.
fn basket_rate(
    composition: &[FloraShare],
    favored: Option<&str>,
    favored_gain: f32,
    flora: &FloraConfig,
    rate_of: impl Fn(&crate::flora_config::FloraDef) -> f32,
    fallback: f32,
) -> f32 {
    let mut named = NO_SHARE;
    let mut rate = 0.0_f32;
    // The composition is already in a total order (share DESC, species key ASC) wherever it is built,
    // so this sum is in a fixed order and nothing here reaches `HashMap` iteration order.
    for entry in composition {
        let Some(def) = flora.species.get(&entry.species) else {
            continue;
        };
        named += entry.share;
        let gain = if favored == Some(entry.species.as_str()) {
            favored_gain
        } else {
            NO_CONVERSION_GAIN
        };
        rate += entry.share * rate_of(def) * gain;
    }
    if named <= NO_SHARE {
        fallback
    } else {
        rate
    }
}

/// **The MATERIAL account of the same basket** — what a harvest of this patch is *made of*, per unit
/// of biomass (`docs/plan_crafting_and_materials.md` §2).
///
/// It cannot ride [`basket_rate`]'s closure the way the other three accounts do, and the reason is
/// the model rather than the plumbing: food, fodder and trade are interchangeable **scalars**, so a
/// basket averages them into one number, while a material carries a **characteristic vector** and
/// averaging two species' would invent a plant that is not growing there. So the basket is
/// *decomposed* instead of summed: one row per species per material, each keeping that species' own
/// exact reading and carrying its share in the rate. Rows that land in the same band merge in the
/// store, which is where merging belongs.
///
/// Reads the patch's **standing** rung and applies the same favored-crop conversion gain the other
/// three accounts get — tending is knowing your crop, whichever account it pays into.
pub fn patch_material_yields(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
) -> Vec<crate::materials_config::MaterialYieldDef> {
    let rung = standing_rung(patch);
    let composition = composition_for_rung(patch, tile_composition, forage, rung);
    let favored_gain = favored_conversion_gain(rung, forage);
    let mut rows = Vec::new();
    for entry in composition.iter() {
        let Some(def) = flora.species.get(&entry.species) else {
            continue;
        };
        let gain = if patch.species.as_deref() == Some(entry.species.as_str()) {
            favored_gain
        } else {
            NO_CONVERSION_GAIN
        };
        for row in &def.yield_.materials {
            rows.push(crate::materials_config::MaterialYieldDef {
                material: row.material.clone(),
                per_biomass: entry.share * row.per_biomass * gain,
                characteristics: row.characteristics.clone(),
            });
        }
    }
    rows
}

/// **THE conversion seam** — how well one unit of this patch's biomass turns into food
/// (`docs/plan_flora_roster.md` §4.3): the share-weighted average of the patch's **effective** basket
/// ([`patch_composition`]), with the tended rung's conversion gain on the favored crop's term.
///
/// A **wild** patch therefore pays *its own tile's* basket rather than a map-wide constant — two tiles
/// of one biome with different realized baskets pay different rates, which is what makes the §10
/// realization visible in the economy. `forage.provisions_per_biomass` survives only as the
/// **empty-basket fallback** (and as the rung-3 quality normalization baseline).
///
/// Every biomass→provisions conversion on the plant web resolves the rate here; no call site may
/// reach for `forage.provisions_per_biomass` on a patch directly, for the reason `patch_ecology`
/// exists.
pub fn patch_provisions_per_biomass(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
) -> f32 {
    rung_rate(
        patch,
        tile_composition,
        flora,
        forage,
        standing_rung(patch),
        |def| def.yield_.provisions_per_biomass,
        forage.provisions_per_biomass,
    )
}

/// The conversion rate this patch's crop would reach **on `rung`** — [`patch_provisions_per_biomass`]
/// asked about a rung the patch may not stand on yet (or may already have passed). Used by the two
/// managed-rung payoff quotes, each naming *its own* rung: [`tended_provisions`] asks
/// `PlantTended`, [`field_provisions`] (through [`patch_species_quality`]) asks `PlantField`.
fn rung_provisions_per_biomass(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    rung: RungKey,
) -> f32 {
    rung_rate(
        patch,
        tile_composition,
        flora,
        forage,
        rung,
        |def| def.yield_.provisions_per_biomass,
        forage.provisions_per_biomass,
    )
}

/// **The FIELD basket's conversion rate RELATIVE to the wild baseline** — dimensionless, `1.0` =
/// exactly baseline. Rung 3's managed rate is a *rate on the standing crop*, so it scales by this
/// rather than by the absolute rate: `field_payoff = biomass × field_provisions_per_biomass ×
/// species_quality`.
///
/// **It reads `PlantField` whatever rung the patch stands on**, which is the whole point: a Field's
/// basket is 100% its crop and takes no rung-2 conversion gain, so this is exactly `crop rate ÷ wild
/// rate` — the number a Field would really pay — even when the patch it is asked about is currently
/// tended. `fieldYield` is published for every patch, so anything else is a quote that disagrees with
/// the payout (see [`composition_for_rung`]).
///
/// **Derived, never a second config field.** A `field_provisions_multiplier` per species would be a
/// redundant lever that could drift from the conversion rate it is supposed to express.
pub fn patch_species_quality(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
) -> f32 {
    if forage.provisions_per_biomass <= 0.0 {
        return WILD_SPECIES_QUALITY; // `validate()` pins the wild rate positive; never divide by 0.
    }
    rung_provisions_per_biomass(patch, tile_composition, flora, forage, RungKey::PlantField)
        / forage.provisions_per_biomass
}

/// **The species-quality of a basket that converts exactly at the wild baseline** — the dimensionless
/// `1.0` [`patch_species_quality`] falls back to. Named because "1.0" at a call site says nothing
/// about which baseline it is one *of*.
const WILD_SPECIES_QUALITY: f32 = 1.0;

/// **The wire quote `0` uses for "this plant cannot climb this rung".** Distinct from a real ratio of
/// `0`, which cannot occur: a species that appears in a tile's basket has `share > 0`, and
/// `FloraConfig::validate` pins every yield vector positive.
pub const CANNOT_CLIMB_RATIO: f32 = 0.0;

/// **Which plant a `Cultivate`/`Sow` on this tile may commit to** — the legality rule, stated once
/// (`docs/plan_flora_roster.md` §4.3) and read by both the `assign_labor` rejection and the labor
/// arm's commit.
///
/// A selection is legal iff the roster knows it, the rung's `cultivation_ceiling` permits it
/// (`allows_cultivate` for the tended rung, `allows_sow` for the Field), **and it is in this tile's
/// basket** — resolved through [`tile_flora_composition`], never `FloraConfig::composition` on a raw
/// terrain, so a navigable hex's two-term basket is judged the way it is actually made.
pub fn species_is_legal_here(
    species: &str,
    composition: &[FloraShare],
    flora: &FloraConfig,
    rung: RungKey,
) -> bool {
    composition
        .iter()
        .any(|entry| entry.species == species && species_climbs(species, entry.share, flora, rung))
}

/// **The share a species must exceed to count as present in a tile's basket.** A zero-share entry is
/// a plant that is named on the tile and takes none of it — nothing to commit to.
const NO_SHARE: f32 = 0.0;

/// **The plant a commitment falls to when the player named none** — the highest-share species in this
/// tile's basket that the rung permits. The composition is already sorted share-DESC then key-ASC (a
/// *total* order), so this is deterministic without a second sort.
///
/// `None` = **this ground grows nothing that can climb this rung** — an open-water fishery, an alpine
/// peak, a MixedWoodland asked to be sown. That is the `cultivation_ceiling` ruling working ("not
/// every plant climbs"), not a gap, and the caller turns it into a refusal.
pub fn default_species_for_rung(
    composition: &[FloraShare],
    flora: &FloraConfig,
    rung: RungKey,
) -> Option<String> {
    composition
        .iter()
        .find(|entry| species_is_legal_here(&entry.species, composition, flora, rung))
        .map(|entry| entry.species.clone())
}

/// **Why a Cultivate/Sow may not commit this patch to this plant** — the species-side twin of
/// [`SiteRefusal`], in the same style (a small enum with a stable string key, the live value staying
/// serde-free). They are deliberately *separate* enums because they judge different things:
/// `SiteRefusal` judges **the land** (and is therefore a property of the tile the wire can publish
/// per-tile), while this judges **a selection against a rung** and only exists in the context of one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeciesRefusal {
    /// The named key is not in `flora_config.json` at all.
    Unknown,
    /// The species exists but its `cultivation_ceiling` stops below this rung — an oak's mast is a
    /// wild harvest forever.
    CeilingTooLow,
    /// The species exists and climbs, but it does not grow on this tile.
    NotHere,
    /// **Nothing** in this tile's basket can climb this rung, so there is nothing to commit to.
    NothingClimbsHere,
}

impl SpeciesRefusal {
    /// Stable string key — the [`SiteRefusal::as_str`] convention.
    pub fn as_str(self) -> &'static str {
        match self {
            SpeciesRefusal::Unknown => "unknown_species",
            SpeciesRefusal::CeilingTooLow => "species_ceiling_too_low",
            SpeciesRefusal::NotHere => "species_not_here",
            SpeciesRefusal::NothingClimbsHere => "nothing_climbs_here",
        }
    }
}

/// **Resolve what a Cultivate/Sow on this tile would commit to** — the one seam the `assign_labor`
/// rejection and the labor arm's commit both read, so a selection the command accepted can never be
/// one the turn then refuses.
///
/// `selection` is the assignment's species choice (`None` = "pick for me"). `Ok` carries the species
/// key that will be committed; `Err` names why nothing can be.
pub fn resolve_committed_species(
    selection: Option<&str>,
    composition: &[FloraShare],
    flora: &FloraConfig,
    rung: RungKey,
) -> Result<String, SpeciesRefusal> {
    match selection {
        Some(species) => {
            if species_is_legal_here(species, composition, flora, rung) {
                return Ok(species.to_string());
            }
            let Some(def) = flora.species.get(species) else {
                return Err(SpeciesRefusal::Unknown);
            };
            let climbs = match rung {
                RungKey::PlantField => def.cultivation_ceiling.allows_sow(),
                _ => def.cultivation_ceiling.allows_cultivate(),
            };
            Err(if climbs {
                SpeciesRefusal::NotHere
            } else {
                SpeciesRefusal::CeilingTooLow
            })
        }
        None => default_species_for_rung(composition, flora, rung)
            .ok_or(SpeciesRefusal::NothingClimbsHere),
    }
}

/// **What a patch pays, standing on `rung`** — in provisions/turn, through the *same* helpers the sim
/// itself quotes and pays each rung with, never a re-derivation of their arithmetic:
///
/// - **wild / anything below rung 2** — its long-run sustainable yield (MSY) on the patch's own wild
///   ecology, converted at the patch's **basket** rate. **A rung PAYOFF, not a take ceiling**: since
///   the harvest floor the take is constant escapement (`forage_escapement_ceiling`), which is
///   `r`-independent and so cannot compare two rungs at all — `r` is exactly what a rung buys;
/// - **tended** — [`tended_provisions`], the rung-2 payoff quote (the wire's `tendedYield`), which
///   rides `tended_ecology` and therefore **carries `cultivation.tended_regrowth_gain`**;
/// - **field** — [`field_provisions`], the rung-3 managed rate the labor arm actually pays.
///
/// That third bullet is the whole reason this exists. The two drawn-down rungs are compared as MSY
/// (`r · K / 4`), where `r` **does not cancel** between wild and tended — tending changes `r`, that is
/// its payoff — so any comparison built on capacity alone silently drops the regrowth gain and
/// understates rung 2 by exactly it. Rung 3 is not an MSY at all but a flat rate on the standing
/// crop, so it is not even the same *shape* of number. One function, three arms, so a quote can never
/// be assembled out of the wrong shape again.
///
/// `tile_composition` is the **tile's** realized basket — the rung derives the patch's effective one
/// from it ([`patch_composition`]), so a quote and the payout it quotes read the same reweight.
pub fn rung_payoff(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    output_multiplier: f32,
    rung: RungKey,
) -> f32 {
    match rung {
        RungKey::PlantField => {
            field_provisions(patch, tile_composition, forage, flora, output_multiplier)
        }
        RungKey::PlantTended => {
            tended_provisions(patch, tile_composition, forage, flora, output_multiplier)
        }
        _ => forage_provisions(
            sustainable_yield(patch.biomass, patch.carrying_capacity, &forage.ecology)
                .clamp(0.0, patch.biomass),
            patch_provisions_per_biomass(patch, tile_composition, flora, forage),
            output_multiplier,
        ),
    }
}

/// **The patch a tile WOULD carry, had this crop been committed and the rung finished** — the
/// hypothetical every per-species quote is taken against: the rung's meter complete, **the tile's own
/// `K`**, and the standing crop that rung settles at.
///
/// **Its capacity is the tile's at every rung** (#433): no rung below 4 raises `K` and none lowers
/// it, so the hypothetical differs from the wild counterfactual only in what the patch's biomass is
/// *made of* — which the rate seams derive from the commitment through [`patch_composition`]. The
/// retired concentration term used to shrink this capacity to `share × gain`, which is exactly the
/// bug that made a commitment cost production.
///
/// `species = None` builds the **wild** counterfactual, which is the denominator of
/// [`commit_yield_ratio`] and the reason both sides of that ratio come out of one construction rather
/// than two.
///
/// **The standing crop is per-rung, and that is load-bearing.** Each rung is quoted where a *running*
/// patch on it actually stands: the drawn-down rungs at their MSY operating point (Sustain settles a
/// patch at `K/2`), and a Field at its capacity, because a Field is never drawn down and regrows to
/// it. For a rung already built, that is the number the shipped `tendedYield`/`fieldYield` read too.
fn hypothetical_patch(
    tile: UVec2,
    tile_capacity: f32,
    species: Option<&str>,
    rung: RungKey,
) -> ForagePatch {
    let mut patch = ForagePatch::new(tile, tile_capacity);
    patch.biomass = tile_capacity * settled_biomass_fraction(rung);
    if let Some(key) = species {
        patch.species = Some(key.to_string());
        match rung {
            RungKey::PlantField => patch.field_progress = RUNG_COMPLETE,
            _ => patch.cultivation_progress = RUNG_COMPLETE,
        }
    }
    patch
}

/// **This species' share of a tile's basket** — [`NO_SHARE`] when the tile does not grow it. The one
/// lookup the per-species quotes take against the composition they are handed, so a quote's legality
/// check and its payoff read the same number.
fn share_of(composition: &[FloraShare], species: &str) -> f32 {
    composition
        .iter()
        .find(|entry| entry.species == species)
        .map_or(NO_SHARE, |entry| entry.share)
}

/// **The fraction of its own capacity a patch on `rung` settles at.** A drawn-down rung is gathered
/// to its MSY operating point (`MSY_BIOMASS_FRACTION` — Sustain's escapement, the point a harvested
/// stand *lives* at); a Field is never drawn down, so it stands at its capacity.
fn settled_biomass_fraction(rung: RungKey) -> f32 {
    match rung {
        RungKey::PlantField => FULL_STANDING_CROP,
        _ => crate::fauna::MSY_BIOMASS_FRACTION,
    }
}

/// **A Field's standing crop is its whole capacity** — it is never drawn down, so it regrows to `K`
/// and stays there. Named rather than a bare `1.0` because it states *which* stock the number is a
/// fraction of.
const FULL_STANDING_CROP: f32 = 1.0;

/// **What this tile would pay per turn once committed to THIS plant and worked up to `rung`** —
/// provisions/turn, in the same units and at the same `output_multiplier` convention as the shipped
/// per-patch forecast quotes (`tendedYield`/`fieldYield`), so the client can substitute one for the
/// other with no arithmetic of its own.
///
/// The point of it being *per species* is that the shipped quotes are species-**blind**: they read
/// whatever the patch is already committed to (usually nothing), so a player choosing between crops
/// in the compose sheet is shown one number for every option. [`CANNOT_CLIMB_RATIO`] when the plant
/// cannot climb `rung` here.
// A per-species quote needs the whole tile context (where, how much land, how much standing crop,
// which plant, how much of the basket it is) plus both config tables and the rung — the same shape
// `forage_source_yield_preview` already carries, and none of it is derivable from the rest.
#[allow(clippy::too_many_arguments)]
pub fn commit_payoff(
    tile: UVec2,
    tile_capacity: f32,
    species: &str,
    composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    output_multiplier: f32,
    rung: RungKey,
) -> f32 {
    if !species_climbs(species, share_of(composition, species), flora, rung) {
        return CANNOT_CLIMB_RATIO;
    }
    let patch = hypothetical_patch(tile, tile_capacity, Some(species), rung);
    rung_payoff(&patch, composition, forage, flora, output_multiplier, rung)
}

/// **The FODDER (hay) committing this tile to THIS plant would pay per turn, on `rung`** (Flora Roster
/// F3, §5, Part D) — the fodder twin of [`commit_payoff`], so the crop picker can show a hay crop's
/// real value instead of the bare `0×` its provisions ratio reads. Built through the *same*
/// `hypothetical_patch` construction and the *same* payoff functions the sim pays with (the §4.3
/// "assert the quote against the payoff function" rule), so the published number and the payout cannot
/// drift. `0.0` for a plant that pays no fodder or cannot climb `rung` here.
///
/// **It takes a `rung` for the reason [`commit_payoff`] does**, and it did not always: F3 quoted the
/// Field arm alone, so the Cultivate row of the picker had nothing to state but a *sown Field's* hay.
/// The two rungs pay different amounts off different baskets, so one number cannot answer both.
#[allow(clippy::too_many_arguments)]
pub fn commit_fodder_payoff(
    tile: UVec2,
    tile_capacity: f32,
    species: &str,
    composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    output_multiplier: f32,
    rung: RungKey,
) -> f32 {
    if !species_climbs(species, share_of(composition, species), flora, rung) {
        return 0.0;
    }
    let patch = hypothetical_patch(tile, tile_capacity, Some(species), rung);
    rung_fodder_payoff(&patch, composition, forage, flora, output_multiplier, rung)
}

/// **What a patch pays in FODDER, standing on `rung`** — the fodder arm of [`rung_payoff`], dispatching
/// to the *same* helpers the sim pays each rung with: [`field_fodder`] at rung 3 (a managed rate on the
/// standing crop) and [`tended_fodder`] at rung 2 (the MSY skim, because rung 2 is drawn down). Rung 1
/// pays no *committed* fodder quote — a wild gather's hay is not a commitment's payoff — so it is `0`,
/// the same "cannot climb this rung" sentinel the ratios use.
fn rung_fodder_payoff(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    output_multiplier: f32,
    rung: RungKey,
) -> f32 {
    match rung {
        RungKey::PlantField => {
            field_fodder(patch, tile_composition, forage, flora, output_multiplier)
        }
        RungKey::PlantTended => {
            tended_fodder(patch, tile_composition, forage, flora, output_multiplier)
        }
        _ => 0.0,
    }
}

/// **The TRADE GOODS committing this tile to THIS plant would credit per turn, on `rung`** (Flora
/// Roster F4, §6) — the exact trade twin of [`commit_fodder_payoff`], routing the yield vector's
/// `trade_goods_per_biomass` component instead of its `fodder_per_biomass` one. Built through the
/// *same* `hypothetical_patch` construction and the *same* payoff functions the sim pays with (the
/// §4.3 "assert the quote against the payoff function" rule), so the picker's cash-crop row and the
/// payout cannot drift. `cultivatePayoff`/`sowPayoff` read `0` for a cash crop — it is worthless as
/// food — so this is the number that lets the picker show its real value instead of a bare `0×`.
/// `0.0` for HAY — whose vector pays no trade at all — or a plant that cannot climb `rung`. A
/// STAPLE reads the small flat token (`trade_goods_per_biomass` 0.005), never `0`, which is exactly
/// why no surface may read "trade > 0" as "cash crop".
///
/// **The rung parameter closes a real hole, not a symmetry gap.** #433 made a tended cash crop *pay*
/// trade ([`tended_take_trade_goods`]); this is what makes it *quote* it. Until now the Cultivate row
/// of the picker printed the Field number — off by the whole difference between a managed rate and an
/// MSY skim, on a rung the player was about to commit 25 turns to.
#[allow(clippy::too_many_arguments)]
pub fn commit_trade_payoff(
    tile: UVec2,
    tile_capacity: f32,
    species: &str,
    composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    output_multiplier: f32,
    rung: RungKey,
) -> f32 {
    if !species_climbs(species, share_of(composition, species), flora, rung) {
        return 0.0;
    }
    let patch = hypothetical_patch(tile, tile_capacity, Some(species), rung);
    rung_trade_payoff(&patch, composition, forage, flora, output_multiplier, rung)
}

/// **What a patch pays in TRADE GOODS, standing on `rung`** — the trade arm of [`rung_payoff`], the
/// exact twin of [`rung_fodder_payoff`]: [`field_trade_goods`] at rung 3, [`tended_trade_goods`] at
/// rung 2, `0` below.
fn rung_trade_payoff(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    output_multiplier: f32,
    rung: RungKey,
) -> f32 {
    match rung {
        RungKey::PlantField => {
            field_trade_goods(patch, tile_composition, forage, flora, output_multiplier)
        }
        RungKey::PlantTended => {
            tended_trade_goods(patch, tile_composition, forage, flora, output_multiplier)
        }
        _ => 0.0,
    }
}

/// **What this tile pays per turn left WILD** — the denominator of [`commit_yield_ratio`], and the
/// same Sustain skim `rung_payoff` gives any uncommitted patch.
///
/// It takes the **composition** rather than nothing because a wild gather is no longer priced at a
/// map-wide constant (#433): you gather the whole basket, so you get *this tile's* basket average.
pub fn wild_payoff(
    tile: UVec2,
    tile_capacity: f32,
    composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    output_multiplier: f32,
) -> f32 {
    let patch = hypothetical_patch(tile, tile_capacity, None, RungKey::PlantWild);
    rung_payoff(
        &patch,
        composition,
        forage,
        flora,
        output_multiplier,
        RungKey::PlantWild,
    )
}

/// Can this plant climb `rung` on a tile where it holds `share` of the basket? The ceiling half of
/// [`species_is_legal_here`], split out for the quote path, which already has the share in hand.
fn species_climbs(species: &str, share: f32, flora: &FloraConfig, rung: RungKey) -> bool {
    let Some(def) = flora.species.get(species) else {
        return false;
    };
    let climbs = match rung {
        RungKey::PlantField => def.cultivation_ceiling.allows_sow(),
        _ => def.cultivation_ceiling.allows_cultivate(),
    };
    climbs && share > NO_SHARE
}

/// **What committing THIS tile to THIS plant is worth, against just gathering it wild** — the single
/// number the crop-picker decision turns on (`docs/plan_flora_roster.md` §4.3).
///
/// **It is the two published payoffs, divided — not a formula that reproduces them.** Both arguments
/// come from [`commit_payoff`] / [`wild_payoff`], i.e. from the functions the sim itself quotes and
/// pays each rung with, so the ratio and the payoffs it relates are one computation and cannot
/// disagree. Taking a ratio of *arithmetic* instead of a ratio of *payoffs* is exactly the bug this
/// signature exists to make unrepresentable: the previous version divided
/// `concentration × rate ÷ base_rate`, a **capacity**-based basis in which the ecology's `r` cancels
/// — but rungs 1–2 pay **MSY** (`r · K / 4`) and tending's payoff *is* that it scales `r` by
/// `cultivation.tended_regrowth_gain`, so every Cultivate ratio shipped at exactly half its true
/// value and told the player that tending a good delta crop *lost*.
///
/// `> 1.0` committing beats gathering the whole basket; `< 1.0` it is a loss — **a legal one the
/// player stays free to choose**, which is the whole decision, so this is never clamped and a sub-1
/// crop is never refused. [`CANNOT_CLIMB_RATIO`] when the plant cannot climb the rung (`payoff` is
/// then the same sentinel) or when the tile pays nothing wild.
pub fn commit_yield_ratio(payoff: f32, wild: f32) -> f32 {
    if wild <= 0.0 {
        return CANNOT_CLIMB_RATIO;
    }
    payoff / wild
}

/// Seed a full patch on every `FoodModuleTag` tile at Startup (idempotent — a world that already
/// carries patches, e.g. after a rollback restore, is skipped). Runs in the Startup chain after
/// `spawn_initial_world` has stamped the food-module tags. Mirrors `spawn_initial_herds`.
///
/// **The patch's cap is the TILE's, not a constant** — `forage.capacity_by_biome[tile.terrain]`, the
/// human food web's per-biome table (the mirror of `graze.capacity_by_biome`). A food-module tile
/// whose biome carries nothing human-edible (`NO_FORAGE_CAPACITY` — a glacier, a salt pan, a
/// deep-sea vent field: the module classifier tags these off their *tags*, not off anything growing
/// there) is seeded **no patch at all**, exactly as a zero-graze tile holds no `GrazePatch`: "no food
/// here" is an *absent* reading, never a zero one, and a zero-cap patch would be a permanently
/// Collapsing source with a zero reseed floor.
pub fn spawn_initial_forage(
    mut registry: ResMut<ForageRegistry>,
    labor_config: Res<LaborConfigHandle>,
    tiles: Query<(&Tile, &FoodModuleTag)>,
) {
    if !registry.patches.is_empty() {
        return;
    }
    let labor = labor_config.get();
    let forage = &labor.forage;
    for (tile, _module) in tiles.iter() {
        let capacity = tile_forage_capacity(forage, tile);
        if capacity <= NO_FORAGE_CAPACITY {
            continue;
        }
        let mut patch = ForagePatch::new(tile.position, capacity);
        patch.refresh_ecology_phase(&forage.ecology);
        registry.patches.insert(tile.position, patch);
    }
}

/// Per-turn forage regrowth (`TurnStage::Logistics`, alongside `advance_herds`): regrow every patch
/// toward its carrying capacity and refresh its ecology phase. Patches never despawn.
pub fn advance_forage_regrowth(
    mut registry: ResMut<ForageRegistry>,
    labor_config: Res<LaborConfigHandle>,
    tile_registry: Res<crate::resources::TileRegistry>,
    tiles: Query<&Tile>,
) {
    let labor = labor_config.get();
    let forage = &labor.forage;
    for patch in registry.patches.values_mut() {
        // **THE LAND OWNS `K`, recomputed fresh from the tile every turn** — the plant twin of
        // `fauna::ecological_carrying_capacity`'s one write, and since #433 the tile's capacity
        // *verbatim* at every rung: no rung below 4 raises `K` and **none lowers it**, so a
        // commitment changes only what the patch's biomass is made of ([`patch_composition`]).
        // Idempotent (the tile's capacity is never read back into itself), so a retuned
        // `capacity_by_biome` reaches patches already on the map without a second write path. A
        // patch whose tile is absent from the map keeps whatever capacity it was seeded with —
        // which is what lets test harnesses build synthetic patches on tiles that do not exist.
        if let Some(tile) = tile_registry
            .index(patch.tile.x, patch.tile.y)
            .and_then(|entity| tiles.get(entity).ok())
        {
            patch.carrying_capacity = tile_forage_capacity(forage, tile);
        }
        regrow_patch(patch, forage);
    }
}

/// Per-turn cultivation feral/decay pass (`TurnStage::Logistics`, alongside `advance_forage_regrowth`).
///
/// **A tended patch is worked, not passive.** The tended-crop *food* is no longer paid here (the old
/// even-split across all the owner's bands is retired): it is paid **place-local** in the labor arm
/// (`advance_labor_allocation`, Population) to the band whose Forage assignment actually tends the
/// patch, at a higher-than-wild rate — see that system. This pass now only handles **decay/feral**:
/// - A patch **worked as an improvement this turn** (`tended_this_turn`) is **spared**. That covers
///   a completed patch/Field being worked *and* one being prepared under `Improvement::Cultivate` /
///   `Improvement::Sow` — so an investment accrues at the full `progress_per_turn` (25 turns at the
///   shipped default) instead of net-of-decay.
/// - An **untended** cultivated patch **goes feral**: `cultivation_progress` decays by
///   `decay_per_turn`, dropping below `1.0` so it reverts to a wild depletable gather patch, and keeps
///   decaying toward 0 over ~`1/decay_per_turn` turns (owner clears at 0 — the investment is fully
///   lost, and re-preparing must re-accrue from wherever progress landed).
/// - An **abandoned** part-prepared patch's partial accrual decays the same way (walk away mid-
///   investment and the cleared ground grows back over).
///
/// **A GRACE first.** Nothing decays on the first un-worked turn any more. Each patch carries a
/// [`ForagePatch::neglect_turns`] counter — reset whenever the patch is worked, incremented whenever
/// it is not — and the bleed applies only while it *exceeds* the decaying rung's `grace_turns`
/// ([`RungDef::neglect_grace_turns`]). A crew re-tasked for a couple of turns, a band that walked to
/// answer a raid, a keeper following a herd: none of those cost the investment now. The animal twin is
/// the same counter gating the shed in [`crate::fauna::advance_husbandry`] — one trigger, two
/// penalties.
///
/// **The unwind is NEWEST-FIRST: one meter at a time, the highest rung with progress on it.** The
/// least-established improvement is the most fragile, so a Field bleeds to nothing *before* the tended
/// ground beneath it loses anything, and `cultivation_progress` **cannot move while `field_progress >
/// RUNG_UNSTARTED`**. Bleeding both at once produced an unrecoverable state: a gap in a `Sow` knocked
/// cultivation to `0.99`, and once the crew came back the running `Sow` marked the patch worked every
/// turn, so the tended rung could neither decay further nor re-accrue (only `Cultivate` accrues it, and
/// at most one improvement is ever in flight). The patch was stranded one hundredth below a rung it had
/// already paid for, permanently. Ordering the unwind makes that state unreachable by construction.
///
/// It still does *not* step a lapsing Field down to a tended patch: rung 3 unwinding to zero reveals
/// whatever rung 2 the ground already had — which may be nothing — and never pays the deserter a rung
/// they did not build.
///
/// **A lost rung is ANNOUNCED.** Crossing back below [`RUNG_COMPLETE`] destroys a 25-turn investment's
/// payoff, so each decay call reports that edge and this pass pushes the rung's own feed line
/// (`CommandEventKind::Cultivate` / `Sow`) — once, on the transition, the way the animal web has always
/// announced a lost pen (`fauna::announce_pen_lost`). The long bleed to zero that follows says nothing
/// further: the loss already happened.
///
/// **Stage ordering.** Logistics runs *before* Population, so the `tended_this_turn` flag this pass
/// reads was written by the labor arm **last** turn (a one-turn lag) — the flag is a deliberate
/// carry-across-turns signal, not a same-turn one. Each patch's flag is cleared here after it is read,
/// so the labor arm re-sets it next Population stage. Net effect: a patch worked every turn never
/// decays; a patch whose band leaves starts counting toward its rung's grace one turn later. The plant
/// counterpart of `fauna::advance_husbandry`'s decay side.
/// **Which rung's meter is unwinding on this patch right now** — the *newest* improvement that still
/// has progress banked on it, because the plant web unwinds newest-first (see
/// [`advance_cultivation`]). `None` for a wild patch: nothing has been built here, so there is nothing
/// to lose and no grace to spend.
///
/// **One seam, two readers**, and that is the point: `advance_cultivation` bleeds the rung this
/// returns, and `snapshot_forage_patches` publishes *that* rung's remaining grace. Deriving the
/// at-risk rung twice is how the wire comes to count down a grace on a rung the sim is not touching.
pub fn patch_unwinding_rung<'a>(
    patch: &ForagePatch,
    ladder: &'a LadderConfig,
) -> Option<&'a RungDef> {
    if patch.field_progress > RUNG_UNSTARTED {
        Some(ladder.rung(RungKey::PlantField))
    } else if patch.cultivation_progress > RUNG_UNSTARTED {
        Some(ladder.rung(RungKey::PlantTended))
    } else {
        None
    }
}

/// **Turns of neglect this patch can still absorb before its feral bleed starts** — the wire's
/// countdown, resolved through [`patch_unwinding_rung`] so it always describes the rung
/// [`advance_cultivation`] would actually bleed. `None` = a wild patch, with nothing at risk.
pub fn patch_neglect_grace_remaining(patch: &ForagePatch, ladder: &LadderConfig) -> Option<u32> {
    patch_unwinding_rung(patch, ladder).map(|rung| {
        crate::intensification::neglect_grace_remaining(
            patch.neglect_turns,
            rung.neglect_grace_turns(),
        )
    })
}

pub fn advance_cultivation(
    mut registry: ResMut<ForageRegistry>,
    ladder_config: Res<LadderConfigHandle>,
    mut event_log: ResMut<CommandEventLog>,
    tick: Res<SimulationTick>,
) {
    let ladder = ladder_config.get();
    for patch in registry.patches.values_mut() {
        // Spare any patch a band worked as an improvement this turn (working a completed
        // Field/patch, or preparing one under Cultivate/Sow) — and forgive whatever neglect it had
        // accumulated, so the grace is about *consecutive* absence rather than a lifetime budget.
        if patch.tended_this_turn {
            patch.neglect_turns = NEGLECT_NONE;
        } else {
            patch.neglect_turns = patch.neglect_turns.saturating_add(1);
            let neglect = u32::from(patch.neglect_turns);
            // **Newest first, through the one seam the wire reads too.** Exactly one meter unwinds
            // per turn — the Field while it has anything left, then the tended ground under it — and
            // the rung whose meter is moving owns *both* dials, because the grace sits beside the
            // `decay_per_turn` it gates. Every number here is the ladder's
            // (`crate::intensification`), so a rung can be retuned without this system knowing what
            // it says.
            if let Some(rung) = patch_unwinding_rung(patch, &ladder) {
                if neglect > rung.neglect_grace_turns() {
                    let decay = rung.build_decay(RUNG_TIMESCALE_UNSCALED);
                    let verb = rung.verb_improvement();
                    let lost = if verb == Some(Improvement::Sow) {
                        patch.decay_field(decay)
                    } else {
                        patch.decay_cultivation(decay)
                    };
                    if lost {
                        announce_rung_lost(&mut event_log, tick.0, patch.owner, verb, patch.tile);
                    }
                }
            }
        }
        // Clear the transient per-turn flag after reading it (re-set next Population stage if worked).
        patch.tended_this_turn = false;
    }
}

/// **Announce a lost plant rung** — the plant twin of `fauna::announce_pen_lost`, and pushed on the
/// same edge: the turn a *completed* improvement crosses back below [`RUNG_COMPLETE`]. A completed rung
/// is 25 turns of forgone harvest, so losing it is never silent; the partial bleed that follows is not
/// announced, because the thing that mattered has already happened.
///
/// Rides the verb's **own** feed kind (`cultivate` / `sow`), so a rung's whole life — the command, the
/// completion, the loss — reads on one channel, exactly as the pen's does.
fn announce_rung_lost(
    event_log: &mut CommandEventLog,
    tick: u64,
    owner: Option<FactionId>,
    verb: Option<Improvement>,
    tile: UVec2,
) {
    let (Some(owner), Some(verb)) = (owner, verb) else {
        return;
    };
    let (kind, what) = match verb {
        Improvement::Sow => (CommandEventKind::Sow, "field"),
        _ => (CommandEventKind::Cultivate, "tended patch"),
    };
    let (x, y) = (tile.x, tile.y);
    event_log.push(CommandEventEntry::new(
        tick,
        kind,
        owner,
        format!("The {what} at ({x}, {y}) has gone feral — untended, the ground is reverting"),
        Some(format!(
            "status=feral reason=untended action={} x={x} y={y}",
            verb.as_str()
        )),
    ));
}

/// Apply one turn of **pure logistic** regrowth toward the patch's carrying capacity and refresh its
/// ecology phase. Unlike a wild herd (`fauna::regrow_biomass`, which crashes below the Allee
/// threshold and despawns), a patch has no critical-depensation crash — a depleted (feral) patch
/// always recovers, and patches never despawn.
///
/// **Reseed floor.** `logistic_regrowth` returns `0` at `biomass == 0`, so a patch driven to exactly
/// `0` (repeated Eradicate + f32 underflow, `take_fraction = 1.0`, or a restored snapshot carrying
/// `biomass = 0`) would otherwise be stuck at `0` forever — contradicting the "always recovers"
/// invariant. To model plants reseeding from surrounding vegetation, a depleted patch is first lifted
/// to a small standing crop (`reseed_floor_fraction × carrying_capacity`) before regrowth, so it
/// recovers from that floor via the normal logistic curve. The lift only touches patches below the
/// floor — a healthy patch is untouched — and the floor is small (below `collapse_fraction`), so
/// Eradicate still crashes a patch hard into the Collapsing band; it just can't hold it at `0`.
///
/// **The patch's OWN ecology** ([`patch_ecology`]), never `forage.ecology` reached for directly: a
/// tended patch regrows on the boosted `r` its rung bought, which is what makes its faster MSY a
/// harvest the land can actually sustain rather than a promise the stock cannot keep. The animal
/// mirror is `fauna::regrow_biomass`, which resolves `herd_ecology` for exactly this reason.
fn regrow_patch(patch: &mut ForagePatch, forage: &ForageLaborConfig) {
    let ecology = patch_ecology(patch, forage);
    // The reseed lift + logistic step is the shared plant curve (`fauna::reseeding_logistic_regrowth`),
    // so the human-edible forage stock and the animal-edible graze stock can never drift apart.
    patch.biomass = reseeding_logistic_regrowth(
        patch.biomass,
        patch.carrying_capacity,
        ecology.regrowth_rate,
        forage.reseed_floor_fraction,
    );
    patch.refresh_ecology_phase(&ecology);
}

/// **The rung a patch stands on** — the plant ladder resolved for one patch, top-down: sown →
/// `plant:field`, cultivated → `plant:tended`, else `plant:wild`. The exact twin of
/// `fauna::herd_rung`, and the same seam: a system asks the patch for its rung and reads what that
/// rung declares, rather than re-deriving the ladder from `is_cultivated()` at the call site.
///
/// Its one reader today is the Forage arm of `advance_labor_allocation` — **which knowledge this
/// patch's rung teaches** (`RungDef::knowledge_earned`, slice 4). The plant web has no movement
/// primitive to dispatch (a patch is a place), so unlike the animal side there is no second caller.
pub(crate) fn patch_rung<'a>(patch: &ForagePatch, ladder: &'a LadderConfig) -> &'a RungDef {
    ladder.rung(if patch.is_field() {
        RungKey::PlantField
    } else if patch.is_cultivated() {
        RungKey::PlantTended
    } else {
        RungKey::PlantWild
    })
}

/// The forage counterpart of `fauna::hunt_take`: resolve the **escapement ceiling**, cap it by the
/// gathering crew's throughput (`workers × per_worker_biomass_capacity × seasonal × build_dip`),
/// clamp to the patch's remaining biomass, **subtract it from the patch**, and convert the take to
/// provisions (× the caller's productivity `output_multiplier`). Returns the provisions gathered.
///
/// **The two webs' take paths are the same expression** (`docs/plan_harvest_floor.md` §1 + §3.1):
/// `min(crew throughput × build_dip, max(0, B − floor·K))`. The **floor** is a fraction of `K` the
/// assignment carries (`0.5` holds the patch on its most productive biomass, `0` strips it); the
/// **dip** is whatever the crew is building, and it multiplies the *crew* — see
/// [`forage_escapement_ceiling`] for why it left the ceiling.
///
/// The take resolves the patch's **conversion rate** off its own basket as well as its ecology, so
/// it carries the tile's composition and the flora table alongside the forage config — one extra
/// reference each, not one extra model.
#[allow(clippy::too_many_arguments)]
pub(crate) fn forage_take(
    patch: &mut ForagePatch,
    tile_composition: &[FloraShare],
    workers: u32,
    floor: f32,
    improvement: Option<Improvement>,
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    ladder: &LadderConfig,
    output_multiplier: f32,
    // **This crew's resolved BASKET tier**, in biomass/worker before the season — see
    // `forage_per_worker_biomass`.
    per_worker_biomass_capacity: f32,
    seasonal: f32,
) -> Scalar {
    // The stance's escapement ceiling + the gather throughput, both from the shared helpers the
    // pre-commit forecast (`forage_forecast`) reads — the take and the forecast can never disagree.
    // The ceiling is `r`-independent, so unlike the retired MSY skim it does **not** vary with the
    // patch's rung: what a tended patch buys is a faster refill, which shows up next turn as more
    // stock standing above the floor. One call still serves rungs 1 and 2 alike.
    let take_ceiling = forage_escapement_ceiling(floor, patch.biomass, patch.carrying_capacity);
    // **The build dip rides the CREW** (`docs/plan_harvest_floor.md` §3.1): a crew clearing ground
    // carries a fraction of what a gathering crew carries, whatever floor it holds. Multiplying the
    // ceiling instead is what let the harshest draw build for free.
    let worker_cap = workers as f32
        * forage_per_worker_biomass(per_worker_biomass_capacity, seasonal)
        * ladder.build_dip(improvement);
    let take = worker_cap
        .min(take_ceiling)
        .max(0.0)
        .clamp(0.0, patch.biomass);
    // The **conversion** half of the commit trade: every patch turns its biomass into food at its own
    // effective basket's share-weighted average, with the tended rung's gain on the favored crop.
    // Resolved before the take is applied so it reads the same patch state the ceiling did.
    let rate = patch_provisions_per_biomass(patch, tile_composition, flora, forage);
    patch.biomass -= take;
    // FOOD income is fully fractional (a few foragers may gather < 1 provision/turn).
    scalar_from_f32(forage_provisions(take, rate, output_multiplier))
}

/// The **biomass standing above the assignment's floor** at the patch's current stock — the single
/// source of the gather ceiling, shared by `forage_take` (the take path) and `forage_forecast` (the
/// pre-commit forecast), and the exact plant-web twin of `fauna::hunt_escapement_ceiling`:
///
/// ```text
/// max(0, B − floor·K)
/// ```
///
/// **Constant escapement replaced the four per-stance RATES** (`docs/plan_harvest_floor.md` §1): the
/// MSY skim, its `surplus_multiplier`, and the two fraction-of-stock draws (`market.take_fraction`,
/// `eradicate.take_fraction`) are all one expression parameterised by a floor, which the assignment
/// now carries directly ([`crate::components::LaborTarget::Forage`]). Not yet clamped to biomass —
/// callers do that alongside their own throughput cap, and it is belt-and-braces there
/// (`B − floor·K ≤ B` for any floor `≥ 0`).
///
/// **No `ecology`, no `ForageLaborConfig`, and that removal is the point.** An escapement ceiling is
/// `r`-INDEPENDENT, so a take path that cannot reach the growth curve cannot accidentally start
/// depending on it again. The rung-2 payoff is unchanged in substance and clearer in mechanism: a
/// tended patch regrows faster, so *next* turn it has more stock standing above the floor.
///
/// **THE BUILD DIP IS NO LONGER HERE — it multiplies the CREW, not the ceiling**
/// (`docs/plan_harvest_floor.md` §3.1). `yield_fraction_while_building` used to scale this ceiling,
/// which made the harshest draw build for free: a deeper floor offers a bigger stock, so a fraction
/// of a bigger stock still filled the crew's baskets and every stance completed a 25-turn Cultivate
/// on schedule (§0.3). On throughput it is **floor-independent by construction** — there is no floor
/// you can pick that dodges it, because it no longer touches the floor's term — and it is legible:
/// at 25% carry it takes four times the people to clear the same standing surplus. The factor is
/// applied by `forage_take`'s worker cap and by `fauna::forecast_expected_take`, both through the one
/// [`LadderConfig::build_dip`] seam, so the two webs' dips cannot be applied differently.
pub(crate) fn forage_escapement_ceiling(floor: f32, biomass: f32, carrying_capacity: f32) -> f32 {
    escapement_ceiling(floor, biomass, carrying_capacity)
}

/// Biomass one forager can gather this turn (`per_worker_biomass_capacity × seasonal_weight`) — the
/// per-worker throughput `forage_take`'s worker cap multiplies by the head-count, shared with the
/// forecast. Hunting has no seasonal factor, so it has no counterpart helper.
///
/// **`per_worker_biomass_capacity` is a RESOLVED tier, not a config read** (`plan_hunt_through_combat`
/// §4.8): a band with baskets gathers at `labor_config.json`'s `forage.per_worker_biomass_capacity`
/// and a bare-handed one at `equipment.json`'s `basket_kit.unequipped_per_worker_biomass_capacity`,
/// resolved once per band per turn through
/// [`crate::equipment_config::EquipmentConfig::forage_per_worker_biomass_capacity`]. Sites with no
/// band to resolve against (a tile's telemetry, a Field's managed collection cap) pass the shipped
/// *equipped reference* rate, exactly as `HerdTelemetryState::per_worker_biomass` does on the animal
/// web.
pub fn forage_per_worker_biomass(per_worker_biomass_capacity: f32, seasonal: f32) -> f32 {
    per_worker_biomass_capacity * seasonal.max(0.0)
}

/// Biomass → provisions for a gather take (× the caller's productivity multiplier) — the one
/// conversion `forage_take` pays, shared with the forecast. The plant mirror of
/// the animal web's `HuntYield::apply` (which retired the global `fauna::hunt_provisions`).
pub fn forage_provisions(
    biomass_take: f32,
    provisions_per_biomass: f32,
    output_multiplier: f32,
) -> f32 {
    biomass_take * provisions_per_biomass * output_multiplier
}

/// **What a patch would pay its gatherers as a TENDED patch**, in provisions — its Sustain (MSY)
/// ceiling on the *tended* curve ([`tended_ecology`]), clamped to the standing crop.
///
/// This is the plant ladder's **rung-2 payoff quote**, and slice 7 retargeted what it means. It used
/// to be `biomass × tended_provisions_per_biomass` — a *managed rate*, paid whatever the policy, never
/// drawing the patch down. But rung 2 is **still a wild stand**: what tending buys is a faster curve,
/// so the honest quote is "the best sustainable skim this patch will offer once tended", which is
/// exactly the number the tended patch's own `ceiling_sustain` then reads. Its consumer is the
/// forecast's `managed_yield` — the "then Y" of Cultivate's *"preparing X → then Y"* pair — and the
/// wire's `ForagePatchState.tendedYield`.
///
/// The rung-3 twin, [`field_provisions`], **stays** a managed rate: a Field is yours.
pub(crate) fn tended_provisions(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    output_multiplier: f32,
) -> f32 {
    forage_provisions(
        tended_msy_take(patch, forage),
        // The rate this patch converts at **on the TENDED rung** — asked about rung 2 by name, so a
        // patch mid-Cultivate quotes the weeded basket it is planting rather than the one it is
        // still in, and a patch that has already climbed to a Field still answers what rung 2 pays
        // here rather than borrowing rung 3's.
        rung_provisions_per_biomass(patch, tile_composition, flora, forage, RungKey::PlantTended),
        output_multiplier,
    )
}

/// **THE take a rung-2 quote is priced on** — the Sustain (MSY) skim on the *tended* curve, clamped to
/// the standing crop. Stated once because rung 2 pays **three** accounts off one take
/// ([`tended_provisions`], [`tended_fodder`], [`tended_trade_goods`]), and a second copy of this
/// expression is exactly how the food quote and the trade quote would start describing different
/// harvests — the `patch_ecology` lesson, applied to the take instead of the curve.
///
/// It is the **quote's** take, not a policy's: worker-unconstrained and policy-blind, the same
/// convention `tendedYield` has always been published under. What the sim actually credits rides
/// `forage_take`'s policy ceiling and worker cap ([`tended_take_fodder`] /
/// [`tended_take_trade_goods`]), and under `Sustain` the two coincide.
fn tended_msy_take(patch: &ForagePatch, forage: &ForageLaborConfig) -> f32 {
    sustainable_yield(
        patch.biomass,
        patch.carrying_capacity,
        &tended_ecology(forage),
    )
    .clamp(0.0, patch.biomass)
}

/// **What a patch would pay in FODDER as a TENDED patch** — the rung-2 quote twin of
/// [`tended_provisions`], routing the yield vector's fodder component instead of its provisions one.
/// The hay counterpart of [`field_fodder`] one rung down, and the number the crop picker's Cultivate
/// rung needs: before this, the picker had only `sowFodderPayoff` and therefore quoted a *sown Field's*
/// hay on the Cultivate row.
///
/// **Priced on [`tended_msy_take`], the same take the food quote uses**, and converted through the
/// same [`rung_rate`] seam at `PlantTended` — so the three accounts of one rung-2 harvest are one
/// harvest, split three ways, and cannot disagree about its size. `0` for a crop whose vector pays no
/// fodder, with no `role` branch.
pub(crate) fn tended_fodder(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    output_multiplier: f32,
) -> f32 {
    forage_provisions(
        tended_msy_take(patch, forage),
        rung_fodder_per_biomass(patch, tile_composition, flora, forage, RungKey::PlantTended),
        output_multiplier,
    )
}

/// **What a patch would pay in TRADE GOODS as a TENDED patch** — the exact trade twin of
/// [`tended_fodder`], and the quote a rung-2 **cash crop** never had. A tended cotton patch has been
/// *paid* trade since #433 ([`tended_take_trade_goods`]) while being *previewed* as `0`, because the
/// only trade quote on the wire was `sowTradePayoff`, a Field number.
///
/// **No `market.trade_goods_multiplier`.** That markup is a `Deplete`-*policy* concept applied at the
/// credit site; a crop-picker row states what the *crop* pays on this ground, so it is quoted
/// policy-blind at the Sustain skim — the same rule [`field_trade_goods`] states one rung up, and the
/// same convention [`tended_provisions`] already answers the food question under.
pub(crate) fn tended_trade_goods(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    output_multiplier: f32,
) -> f32 {
    forage_provisions(
        tended_msy_take(patch, forage),
        rung_trade_per_biomass(patch, tile_composition, flora, forage, RungKey::PlantTended),
        output_multiplier,
    )
}

/// **THE ecology a patch actually lives under** — the plant twin of `fauna::herd_ecology`, and the one
/// place the plant ladder's rung → growth-rate mapping lives. Tending buys a *growth rate*, and
/// nothing else:
///
/// - **wild** (`forage.ecology`, `r` = 0.25) — an untended stand;
/// - **managed** (a tended patch or a Field) — [`tended_ecology`]: `r × cultivation.tended_regrowth_gain`.
///
/// Every consumer of a patch's ecology — regrowth, the MSY/policy ceilings, the phase classification,
/// the forecast — resolves it *here*. **No call site may re-derive it**: a second copy of this mapping
/// is exactly how a forecast starts promising a number the take won't pay (the lesson `herd_ecology`
/// already paid for).
///
/// **Both managed rungs share one curve, deliberately.** A Field is never drawn down (its harvest is a
/// managed rate on the standing crop), so its `r` moves nothing but how fast it recovers from a
/// collapse — inventing a `field_regrowth_gain` nobody's yield reads would be a lever that lies about
/// having an effect. Rung 3's payoff is `field_provisions`, not a curve.
pub fn patch_ecology(patch: &ForagePatch, forage: &ForageLaborConfig) -> EcologyConfig {
    if patch.is_managed() {
        tended_ecology(forage)
    } else {
        forage.ecology
    }
}

/// The **tended** curve: the wild forage ecology with only its `regrowth_rate` scaled by the rung's
/// `cultivation.tended_regrowth_gain`, leaving the shared phase bands
/// (`collapse_fraction`/`stressed_fraction`/`extinction_floor`) intact — the exact shape
/// `fauna::pastoral_ecology_for` gives a tamed herd. Split out from [`patch_ecology`] because the
/// forecast must also answer it for a patch that is **not tended yet** ("what will this pay once
/// cultivated?").
fn tended_ecology(forage: &ForageLaborConfig) -> EcologyConfig {
    EcologyConfig {
        regrowth_rate: forage.ecology.regrowth_rate * forage.cultivation.tended_regrowth_gain,
        ..forage.ecology
    }
}

/// The place-local managed harvest a sown **Field** (rung 3) pays the band working it each turn:
/// `biomass × cultivation.field_provisions_per_biomass`, no biomass drawn down — the *same shape* as
/// [`tended_provisions`] one rung down, at a higher rate. That shape is the point: rung 3 must
/// out-yield rung 2 on the same tile at the same biomass, or the rung is pointless, and holding the
/// shape fixed makes the comparison a single lever rather than a re-derivation.
///
/// Shared by the Forage arm of `advance_labor_allocation` (the payout) and `forage_forecast`, so
/// forecast == actual.
pub(crate) fn field_provisions(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    output_multiplier: f32,
) -> f32 {
    // **Scaled by the projected basket's relative quality** (Flora Roster S1) —
    // `field_provisions_per_biomass` stays the rung's one dial and `species_quality` is *derived*
    // from the conversion rate, never a second per-species field that could drift from it. A sown
    // Field's basket is 100% its crop, so this is exactly the crop's rate over the wild baseline.
    patch.biomass
        * forage.cultivation.field_provisions_per_biomass
        * patch_species_quality(patch, tile_composition, flora, forage)
        * output_multiplier
}

/// **What a sown Field hands over each turn, stated in BIOMASS** — the managed harvest before it is
/// routed into any one currency, capped by what the crew can carry.
///
/// The three scalar accounts each convert this through their own rate, so none of them ever needs
/// the biomass itself. The **material** account does: a material's `per_biomass` is a rate on the
/// crop rather than on the currency it would otherwise have been sold as, and a cash Field's
/// provisions are `0`, so there is no currency to scale off. Same `min(production, collection)` shape
/// the other three run — an understaffed Field brings home less of everything, in step.
///
/// A Field is never drawn down, so this is a *rate on the standing crop* and `patch.biomass` is
/// unchanged by it.
pub(crate) fn field_harvest_biomass(
    patch: &ForagePatch,
    forage: &ForageLaborConfig,
    equipped_gather_rate: f32,
    workers: u32,
) -> f32 {
    let production = patch.biomass * forage.cultivation.field_provisions_per_biomass;
    let collection =
        workers as f32 * forage_per_worker_biomass(equipped_gather_rate, MANAGED_HARVEST_SEASON);
    production.min(collection)
}

/// The **projected** fodder conversion rate — the projected basket's `yield.fodder_per_biomass`
/// average once the improvement completes (the fodder twin of `projected_provisions_per_biomass`,
/// `docs/plan_flora_roster.md` §5). A sown Field's basket is 100% its crop, so this reads `0.0` for a
/// grain Field and the hay rate for a hay Field, with **no `role` branch** — the vector does the
/// routing. Used by the managed-fodder payout and forecast so a hay Field being sown quotes the hay
/// it *will* pay.
fn field_fodder_per_biomass(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
) -> f32 {
    rung_rate(
        patch,
        tile_composition,
        flora,
        forage,
        RungKey::PlantField,
        |def| def.yield_.fodder_per_biomass,
        NO_UNCOMMITTED_YIELD_RATE,
    )
}

/// The place-local managed **fodder** harvest a sown hay **Field** (rung 3) pays into the band's
/// `FODDER` store each turn — the exact fodder twin of [`field_provisions`], routed by the yield
/// vector's fodder component instead of its provisions component. Same shape
/// (`biomass × field_provisions_per_biomass × fodder_quality`, no biomass drawn down), so a hay Field
/// and a grain Field of the same standing crop harvest the same *fraction* of their biomass — they
/// differ only in which account it lands in. `0` for any patch not committed to a fodder crop, so a
/// grain Field credits no fodder, with no role branch.
///
/// `fodder_quality` = the committed crop's `fodder_per_biomass` relative to the **wild provisions
/// baseline** — the same normalization [`patch_species_quality`] uses for the food account, so the
/// field rung's one rate dial (`field_provisions_per_biomass`) prices both accounts consistently.
pub(crate) fn field_fodder(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    output_multiplier: f32,
) -> f32 {
    if forage.provisions_per_biomass <= 0.0 {
        return 0.0;
    }
    let fodder_quality = field_fodder_per_biomass(patch, tile_composition, flora, forage)
        / forage.provisions_per_biomass;
    patch.biomass
        * forage.cultivation.field_provisions_per_biomass
        * fodder_quality
        * output_multiplier
}

/// **What one worker can carry home from a hay Field**, in fodder/turn — the fodder twin of
/// [`managed_per_worker_yield`]. The crew carries hay exactly as it carries grain, at the same
/// per-worker throughput, so the collection cap on a hay Field is this, in fodder units. `0` for a
/// non-fodder crop (a grain Field's fodder collection is moot).
pub(crate) fn managed_per_worker_fodder(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    equipped_gather_rate: f32,
    flora: &FloraConfig,
    output_multiplier: f32,
) -> f32 {
    forage_provisions(
        forage_per_worker_biomass(equipped_gather_rate, MANAGED_HARVEST_SEASON),
        field_fodder_per_biomass(patch, tile_composition, flora, forage),
        output_multiplier,
    )
}

/// The **projected** trade conversion rate — the projected basket's `yield.trade_goods_per_biomass`
/// average once the improvement completes (the trade twin of [`projected_fodder_per_biomass`],
/// `docs/plan_flora_roster.md` §6). A sown Field's basket is 100% its crop, so a cash Field reads its
/// crop's rate and a hay Field `0`, with **no `role` branch** — the vector does the routing. Used by
/// the managed-trade payout and forecast so a cash Field being sown quotes the trade goods it *will*
/// pay.
fn field_trade_per_biomass(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
) -> f32 {
    rung_rate(
        patch,
        tile_composition,
        flora,
        forage,
        RungKey::PlantField,
        |def| def.yield_.trade_goods_per_biomass,
        NO_UNCOMMITTED_YIELD_RATE,
    )
}

/// The place-local managed **trade goods** a sown cash-crop **Field** (rung 3) credits to the
/// faction `trade_goods` stockpile each turn — the exact trade twin of [`field_fodder`], routed by
/// the yield vector's trade component instead of its fodder component. Same shape
/// (`biomass × field_provisions_per_biomass × trade_quality`, no biomass drawn down), so a cash
/// Field and a grain Field of the same standing crop harvest the same *fraction* of their biomass —
/// they differ only in which account it lands in. `0` for any patch not committed to a cash crop, so
/// a grain Field credits no trade, with no role branch.
///
/// `trade_quality` = the committed crop's `trade_goods_per_biomass` relative to the **wild provisions
/// baseline** — the same normalization [`patch_species_quality`] uses for the food account, so the
/// field rung's one rate dial (`field_provisions_per_biomass`) prices all three accounts
/// consistently. **No `market.trade_goods_multiplier` is applied**: that markup is a `Deplete`-*policy*
/// concept for wild commercial gathering; a managed Field harvest does not carry it.
pub(crate) fn field_trade_goods(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    output_multiplier: f32,
) -> f32 {
    if forage.provisions_per_biomass <= 0.0 {
        return 0.0;
    }
    let trade_quality = field_trade_per_biomass(patch, tile_composition, flora, forage)
        / forage.provisions_per_biomass;
    patch.biomass
        * forage.cultivation.field_provisions_per_biomass
        * trade_quality
        * output_multiplier
}

/// **What one worker can carry home from a cash-crop Field**, in trade-goods/turn — the trade twin of
/// [`managed_per_worker_fodder`]. The crew carries the cash crop exactly as it carries grain, at the
/// same per-worker throughput, so the collection cap on a cash Field is this, in trade units. `0` for
/// a non-cash crop (a grain Field's trade collection is moot).
pub(crate) fn managed_per_worker_trade(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    equipped_gather_rate: f32,
    flora: &FloraConfig,
    output_multiplier: f32,
) -> f32 {
    forage_provisions(
        forage_per_worker_biomass(equipped_gather_rate, MANAGED_HARVEST_SEASON),
        field_trade_per_biomass(patch, tile_composition, flora, forage),
        output_multiplier,
    )
}

/// **The rate a basket the roster cannot decompose pays in the two non-food accounts** — nothing. It
/// is the [`basket_rate`] fallback for fodder and trade, where the food account falls back to
/// `forage.provisions_per_biomass` instead: a stand nobody can name pays *some* food (it is food, that
/// is why the tile has a capacity at all) but no hay and no cash. Named rather than a bare `0.0`
/// because at these call sites the zero is a *statement about an undecomposable basket*, not an absent
/// value.
const NO_UNCOMMITTED_YIELD_RATE: f32 = 0.0;

/// **THE fodder conversion seam** — the fodder twin of [`patch_provisions_per_biomass`]: how well one
/// unit of *this* patch's biomass turns into hay, as the share-weighted average of its **effective**
/// basket. A wild tile that realizes `hay_grass` therefore pays hay on any harvest — the §3 spine is
/// unconditional — and a basket with no fodder crop in it pays [`NO_UNCOMMITTED_YIELD_RATE`], so a
/// tended grain patch credits no hay with no `role` branch.
///
/// **The wild credit's KNOWLEDGE gate is not here.** Whether a band may bank hay it did not commit to
/// is a question about the *faction* (Foddering), and it lives at the credit site in `systems/labor.rs`
/// so this seam stays free of knowledge lookups and commodity-generic.
pub(crate) fn patch_fodder_per_biomass(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
) -> f32 {
    rung_fodder_per_biomass(patch, tile_composition, flora, forage, standing_rung(patch))
}

/// The fodder rate this patch would convert at **standing on `rung`** — [`patch_fodder_per_biomass`]
/// asked about a rung by name, the fodder twin of [`rung_provisions_per_biomass`]. A *quote* must ask
/// by name: a patch mid-Cultivate has to be told what the rung it is building pays, not what the wild
/// stand it still is does.
fn rung_fodder_per_biomass(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    rung: RungKey,
) -> f32 {
    rung_rate(
        patch,
        tile_composition,
        flora,
        forage,
        rung,
        |def| def.yield_.fodder_per_biomass,
        NO_UNCOMMITTED_YIELD_RATE,
    )
}

/// **THE trade conversion seam** — the trade twin of [`patch_fodder_per_biomass`], routing the yield
/// vector's `trade_goods_per_biomass` component. Since #433 it is the **one** trade rate at every
/// drawn-down rung: the species-blind flat `market.trade_goods_per_biomass` sale is retired, and
/// `Deplete` is a *markup* on this rate rather than a separate route (see the Forage arm of
/// `advance_labor_allocation`).
pub fn patch_trade_per_biomass(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
) -> f32 {
    rung_trade_per_biomass(patch, tile_composition, flora, forage, standing_rung(patch))
}

/// The trade rate this patch would convert at **standing on `rung`** — the exact trade twin of
/// [`rung_fodder_per_biomass`], and what lets a rung-2 quote price a cash crop the Cultivate rung
/// really will pay for.
fn rung_trade_per_biomass(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    rung: RungKey,
) -> f32 {
    rung_rate(
        patch,
        tile_composition,
        flora,
        forage,
        rung,
        |def| def.yield_.trade_goods_per_biomass,
        NO_UNCOMMITTED_YIELD_RATE,
    )
}

/// **The FODDER a completed Tended Patch (rung 2) harvest pays** into the working band's `FODDER`
/// store — `take × the committed crop's fodder_per_biomass`, the fodder twin of the provisions
/// conversion [`forage_take`] itself performs, through the same [`forage_provisions`] arithmetic.
/// `0` for an uncommitted patch or a crop whose vector pays no fodder, so this is commodity-generic
/// with **no `role` branch** — a harvest of `B` biomass pays `B × yield.*` into all three accounts
/// (`docs/plan_flora_roster.md` §3), at every rung, not only at rung 3.
///
/// **Driven by the TAKE, not by a managed rate — the deliberate difference from [`field_fodder`].**
/// A Field is never drawn down, so its harvest collapses the policy axis and is quoted as a rate on
/// the standing crop. A tended patch *is* drawn down by the ordinary gather, so its non-food accounts
/// must ride the same take the food account does: `Deplete` on a tended hay patch earns more fodder
/// than `Sustain` because it takes more, and over-farming it shows up in the ⚠ exactly as it does for
/// food. **The take is already worker-capped** by `forage_take`'s `workers × per_worker_biomass`
/// term, so there is deliberately no second collection cap here — the crop the crew carries home is
/// the take it made.
pub fn tended_take_fodder(
    take_biomass: f32,
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    output_multiplier: f32,
) -> f32 {
    forage_provisions(
        take_biomass,
        patch_fodder_per_biomass(patch, tile_composition, flora, forage),
        output_multiplier,
    )
}

/// **The TRADE GOODS a completed Tended Patch (rung 2) harvest credits** to the *faction*
/// `trade_goods` stockpile — the exact trade twin of [`tended_take_fodder`], take-driven for the same
/// reason, and the fix for a tended cash crop (`grapevine`/`cotton`/`flax`/`tobacco`/`tea`,
/// `provisions_per_biomass: 0`) producing nothing in any currency while being drawn down at full MSY.
///
/// **THE one trade rate at every drawn-down rung** (#433). The species-blind flat `market.*` sale is
/// retired, so there is no committed-vs-wild branch left to get wrong: rungs 1 and 2 both credit
/// `take × patch_trade_per_biomass`, and the caller multiplies by
/// `market.trade_goods_multiplier` **iff the policy is `Deplete`** — a *policy* markup ("sell
/// harder") on goods you were already producing, not a rung concept. The markup lives at the credit
/// site rather than here because this function is the *rate*, and the rate does not know the policy.
///
/// (Rung 3 keeps its own no-markup rule — see [`field_trade_goods`]: a Field is never drawn down and
/// has no policy axis at all.)
pub fn tended_take_trade_goods(
    take_biomass: f32,
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    flora: &FloraConfig,
    forage: &ForageLaborConfig,
    output_multiplier: f32,
) -> f32 {
    forage_provisions(
        take_biomass,
        patch_trade_per_biomass(patch, tile_composition, flora, forage),
        output_multiplier,
    )
}

// **RETIRED: `field_yield_fraction_while_building`** — the `plant:field` rung's dip, looked up here
// because two plant sites needed it and only one of them went through the shared ceiling helper. It has
// no callers left: since issue #442 *every* dip on both webs is read through the one
// `LadderConfig::build_dip(improvement)` seam, which is keyed on the verb rather than hard-coding a
// `RungKey`, so a per-rung accessor could only ever be a second way to ask the same question.

/// `SourceYieldForecast::body_mass_yield` for a plant source (slice 8) — `0` = *do not quantise*.
///
/// **A deliberate asymmetry with the animal web, and a principled one — do not "fix" it.** A hunt take
/// is rounded down to whole animals because you cannot half-kill a deer; a gather is not, because you
/// harvest grain by the handful. The two food webs quantise differently because *their products
/// differ* — the same reason seed travels and a herd doesn't (`docs/plan_intensification_ladder.md`).
const PLANTS_DO_NOT_QUANTISE: YieldAccounts = YieldAccounts::ZERO;

/// **The plant web's forecast trade component — a KNOWN GAP, not a claim that plants sell nothing**
/// (`docs/plan_hunt_yield_model.md` §8, issue #337).
///
/// The `Deplete` gather really does sell its take (`labor_config`'s `forage.market.*`, credited by
/// `advance_labor_allocation`), so a patch's honest trade forecast is **not** zero — the sim simply
/// has not projected it yet, exactly as it did not before this arc. #337 vectorised the *animal* web;
/// the plant web's trade forecast is its own arc.
///
/// It is safe to ship as `0.0` because of the client-side rule the animal side introduced: a trade
/// line renders **only when `trade_goods > 0`** — flora's cash-crop rule — so a patch shows *no trade
/// line* rather than a false "0 trade goods/turn". Do not let a reader treat this as "plants have no
/// trade value".
pub(crate) const PLANT_TRADE_FORECAST_NOT_YET_PROJECTED: f32 = 0.0;

/// A plant source's provisions-only forecast component: the food number the plant web computes, with
/// its trade **and fodder** components the [`PLANT_TRADE_FORECAST_NOT_YET_PROJECTED`] gap.
///
/// **This helper is the remaining half of #426 and is meant to disappear.** Projecting the other two
/// accounts needs each component built from the rung's *biomass* ceiling times that rung's own rate
/// (`rung_provisions_per_biomass` / `rung_trade_per_biomass` / `rung_fodder_per_biomass`), which is a
/// restructure of [`forage_forecast`] rather than a wider return type here: this signature takes an
/// already-converted food number and so has nothing left to convert the other accounts *from*.
fn plant_food_only(provisions: f32) -> YieldAccounts {
    YieldAccounts {
        provisions,
        trade_goods: PLANT_TRADE_FORECAST_NOT_YET_PROJECTED,
        fodder: PLANT_TRADE_FORECAST_NOT_YET_PROJECTED,
    }
}

/// Pre-commit yield forecast for foraging `patch` at this tile's `seasonal` weight (its
/// `FoodModuleTag::seasonal_weight`). Mirrors `forage_take` exactly: same resolved ecology
/// ([`patch_ecology`]), same per-policy ceilings, same seasonal-folded per-worker throughput, same
/// biomass clamp, same biomass→provisions conversion — so the client's
/// `min(workers × per_worker_yield, ceiling[policy])` IS the take the sim pays. The plant mirror of
/// `fauna::hunt_forecast`.
///
/// **Two shapes, one per rung-kind** (slice 7 — this is where the plant ladder stopped collapsing a
/// rung early):
/// - A **Field** (rung 3) is *yours*: it pays a managed rate whatever the policy, so it forecasts
///   through [`SourceYieldForecast::managed`] — every ceiling is that rate, and `per_worker_yield` is
///   the crew's real throughput, so `max_useful_workers` falls out as the honest
///   `ceil(production / per_worker)` rather than a hardcoded 1.
/// - A **wild or tended** patch (rungs 1–2) is a wild stand either way, so it takes the full
///   policy-live path below — the *same* code, differing only in the ecology `patch_ecology` hands
///   it. That is the whole rung-2 fix: a tended patch's Sustain/Surplus/Deplete/Eradicate are four
///   different numbers again, and it can be over-farmed.
#[allow(clippy::too_many_arguments)] // the patch, both configs, the ladder and two rates are inputs
pub(crate) fn forage_forecast(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    equipped_gather_rate: f32,
    ladder: &LadderConfig,
    // **The crew's per-gatherer throughput for THIS turn, season already folded in**
    // (`forage_per_worker_biomass(resolved basket tier, seasonal)`). Taken pre-folded rather than as
    // the tier + the season, so this signature stays inside clippy's argument budget and there is
    // exactly one place the two multiply.
    per_worker_gather_biomass: f32,
    output_multiplier: f32,
) -> SourceYieldForecast {
    // A Field's harvest is biomass-based and **seasonless** — the crop is standing in the field you
    // built it to stand in — so its collection cap is too, and it must not read the gather season
    // (which is `NO_FORAGE_SEASON` on module-less ground a crew sowed: a Field there would forecast,
    // and be paid, exactly nothing).
    if patch.is_field() {
        return SourceYieldForecast::managed(
            plant_food_only(field_provisions(
                patch,
                tile_composition,
                forage,
                flora,
                output_multiplier,
            )),
            plant_food_only(managed_per_worker_yield(
                patch,
                tile_composition,
                forage,
                equipped_gather_rate,
                flora,
                output_multiplier,
            )),
            // Plants never quantise — you harvest grain by the handful (slice 8; see
            // `SourceYieldForecast::body_mass_yield`). The whole-animal rule is animal-only because
            // *the products differ*, not by omission.
            PLANTS_DO_NOT_QUANTISE,
        );
    }
    // The patch's IN-EFFECT conversion rate — the same one `forage_take` pays with, so every ceiling
    // the forecast composes is the number the sim will hand over.
    let rate = patch_provisions_per_biomass(patch, tile_composition, flora, forage);
    SourceYieldForecast {
        // A plant is not stalked — the engagement stage is an animal-web concept, and so is the fight
        // it feeds. Nothing on the plant web is brought down.
        engage_rate: f32::INFINITY,
        fight: None,
        per_worker_yield: plant_food_only(forage_provisions(
            per_worker_gather_biomass,
            rate,
            output_multiplier,
        )),
        body_mass_yield: PLANTS_DO_NOT_QUANTISE,
        // **The TERMS of the take** — `ceiling_at(floor, improvement)` composes exactly what
        // `forage_take` computes, at any floor the player's dial can name.
        biomass: patch.biomass,
        carrying_capacity: patch.carrying_capacity,
        // What one unit of this patch's standing crop is worth, at its own basket rate. Food-only:
        // the plant web's trade/fodder PROJECTION is a known gap (`plant_food_only`), while the
        // trade a gather actually earns is reported on the resolved row.
        per_biomass_yield: plant_food_only(forage_provisions(
            crate::fauna::ONE_UNIT_OF_BIOMASS,
            rate,
            output_multiplier,
        )),
        // A wild or tended patch IS drawn down — it is a wild stand either way, which is what makes
        // rungs 1 and 2 floor-live and rung 3 (a Field) not.
        managed_production: None,
        // **The plant web's two build dips, as the FACTORS they are** (issue #442 §2.2). They used
        // to be three more ceiling *rows* — `ceiling_prepare` (Cultivate), `ceiling_sow` and a
        // permanently-zero `ceiling_tame` — each the rung's fraction of the **Sustain** ceiling,
        // which was only expressible while a build verb *was* the policy. A patch is a plant source,
        // so it prices `Cultivate` and `Sow`; `Tame`/`Corral` are not askable of it by type.
        build_dips: BuildDips::for_branch(ladder, RungBranch::Plant),
        // **Cultivate's "then Y"** — what this patch will pay once tended, on the tended curve. On a
        // patch that is *already* tended this is simply its own `ceiling_sustain`, which is the truth:
        // the rung is built, and the number is what it pays. (Sow's "then Y" is `field_provisions`,
        // exported beside this one as the wire's `fieldYield` — two rungs, two payoff quotes, never
        // one field doing both jobs.)
        managed_yield: plant_food_only(tended_provisions(
            patch,
            tile_composition,
            forage,
            flora,
            output_multiplier,
        )),
        // `Tame` is hunt-only — a patch has no pastoral rung — so it advertises no Tame payoff (the
        // plant twin of `ceiling_tame: 0`).
        pastoral_yield: NO_PASTORAL_YIELD,
    }
}

/// **What one worker can carry home from a MANAGED plant source** (a Field), in provisions/turn — the
/// gather throughput `forage_per_worker_biomass` gives, at the seasonless weight, through the gather
/// conversion.
///
/// This is the **collection** half of production-vs-collection (slice 7): rung 3 collapses the *policy*
/// axis (the crop is yours; there is no wild stock to over-skim) but **not** the worker cap — you
/// still have to carry the harvest home, so a Field's actual take is
/// `min(field_provisions, workers × this)` and the surplus it offered beyond that is wasted. Deliberately
/// **not** a new lever: it is the same `per_worker_biomass_capacity` a wild gather is capped by, which
/// is what keeps "a worker can carry X" one number for the whole plant web.
pub(crate) fn managed_per_worker_yield(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    equipped_gather_rate: f32,
    flora: &FloraConfig,
    output_multiplier: f32,
) -> f32 {
    forage_provisions(
        forage_per_worker_biomass(equipped_gather_rate, MANAGED_HARVEST_SEASON),
        rung_provisions_per_biomass(patch, tile_composition, flora, forage, RungKey::PlantField),
        output_multiplier,
    )
}

/// **The negligible-take floor (in PROVISIONS) that ends a `realized` forward projection.** Below
/// this a patch is treated as *spent* — stripped to nothing — so the loop stops and the average
/// divides only by the turns that actually delivered.
///
/// **Provisions-space, which is why it is not [`crate::fauna::REALIZED_PROJECTION_TAKE_EPSILON`]**:
/// the animal twin breaks on a *biomass* take, while both branches here are already converted
/// (`field_provisions`, `forage_take`), so the two thresholds justify their magnitudes on different
/// scales and each gets its own constant rather than sharing one whose doc only covers biomass.
///
/// The magnitude is deliberately far below any live patch's one-turn gather: the smallest is a wild
/// Sustain skim, `r·K/4 × provisions_per_biomass` — ~0.61 provisions on the measured K=195
/// AlluvialPlain stand (see `labor_config.json` → `cultivation`), and a Field pays several times
/// that. Four orders of magnitude of headroom, so a healthy patch never trips it and a dead one
/// always does.
const REALIZED_PROJECTION_PROVISIONS_EPSILON: f32 = 1e-4;

/// **The steady `realized` yield for a forage source — a FORWARD PROJECTION** (the plant twin of
/// `fauna::project_realized_hunt`). The average food/turn the patch delivers over the next `horizon`
/// turns, simulated forward from its CURRENT state under `policy` + `workers`, mirroring the real turn
/// order (Logistics regrow → Population take). A **pure function of the passed patch state**, so the
/// assign-time seed and the resolved row compute the identical number (exact forecast == actual).
///
/// Foraging was never lumpy — `forage_take` is already rate-based (no kill-credit bank) — so the
/// projection just reuses the *same* take path the real turn runs each simulated turn: a **Field**
/// (rung 3) pays its managed `field_provisions` capped by the crew's throughput and never draws down;
/// every other patch pays `forage_take`'s drawn-down policy gather. So the projection is exactly the
/// forward average of what the source really pays, computed through one shared take path.
// The projection needs the full take context (source, config, ladder, season, multiplier, crew,
// policy, horizon) — the same shape `forage_source_yield_preview` already carries.
#[allow(clippy::too_many_arguments)]
pub fn project_realized_forage(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    equipped_gather_rate: f32,
    ladder: &LadderConfig,
    per_worker_biomass_capacity: f32,
    seasonal: f32,
    output_multiplier: f32,
    workers: u32,
    floor: f32,
    improvement: Option<Improvement>,
    horizon: u32,
) -> f32 {
    if horizon == 0 {
        return 0.0; // `LaborConfig::validate` pins `horizon > 0`; belt-and-braces against /0.
    }
    let mut sim = patch.clone();
    let mut total = 0.0_f32;
    // Turns actually simulated — the average divides by this, not the full `horizon`, so a
    // self-terminating gather (an Eradicate strip) reads the rate it delivers while the stand lasts
    // rather than being diluted by empty turns (the animal twin's rule). A patch reseeds, so in
    // practice it rarely trips the break — but the rule is uniform with `project_realized_hunt`.
    let mut turns = 0u32;
    for _ in 0..horizon {
        // Logistics: the patch regrows first, exactly as `advance_forage_regrowth` runs before the
        // Population stage's gather.
        regrow_patch(&mut sim, forage);
        // Population: a Field is a managed harvest (no drawdown, policy axis collapsed, worker-capped);
        // every other patch is the drawn-down policy gather through the shared `forage_take` path.
        let take = if sim.is_field() {
            let production =
                field_provisions(&sim, tile_composition, forage, flora, output_multiplier);
            let collection = workers as f32
                * managed_per_worker_yield(
                    &sim,
                    tile_composition,
                    forage,
                    equipped_gather_rate,
                    flora,
                    output_multiplier,
                );
            production.min(collection)
        } else {
            forage_take(
                &mut sim,
                tile_composition,
                workers,
                floor,
                improvement,
                forage,
                flora,
                ladder,
                output_multiplier,
                per_worker_biomass_capacity,
                seasonal,
            )
            .to_f32()
        };
        if take <= REALIZED_PROJECTION_PROVISIONS_EPSILON {
            break; // the stand is spent — stop before diluting the average with empty turns.
        }
        total += take;
        turns += 1;
    }
    if turns > 0 {
        total / turns as f32
    } else {
        0.0
    }
}

/// **WHEN the food lands for a forage source** (the plant twin of `fauna::project_arrivals_hunt`) —
/// the discrete sibling of [`project_realized_forage`], run over the same forward simulation and
/// recording what is delivered on each projected turn. Returns exactly `horizon` entries: **index `i`
/// is the food delivered `i + 1` turns from now**.
///
/// **A gather is continuous, so a healthy patch is positive in EVERY slot** — and that is the correct
/// reading, not a degenerate one: `forage_take` has no kill-credit bank to quantise it, so the plant
/// web's schedule is a solid run where the animal web's is a pulse. The pair still exists for the
/// plant side because the *client* composes one larder projection out of every source's schedule, and
/// a continuous source has to contribute its own turns rather than be special-cased there.
///
/// Simulated on a private clone through the same take path the real turn runs, so the schedule is
/// what the sim will really pay. Unlike its animal twin there is no early completion test: a stripped
/// stand reseeds and regrows, so its remaining slots are genuinely small-but-positive rather than
/// "gone", and a truly dead source simply fills the schedule with zeros.
// Same shape as its `realized` sibling — the projection needs the full take context.
#[allow(clippy::too_many_arguments)]
pub fn project_arrivals_forage(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    equipped_gather_rate: f32,
    ladder: &LadderConfig,
    per_worker_biomass_capacity: f32,
    seasonal: f32,
    output_multiplier: f32,
    workers: u32,
    floor: f32,
    improvement: Option<Improvement>,
    horizon: u32,
) -> Vec<f32> {
    // `LaborConfig::validate` pins `horizon > 0`; a zero horizon yields an empty schedule, which the
    // client reads as "no data" exactly like an unprojected row.
    let mut schedule = vec![0.0_f32; horizon as usize];
    let mut sim = patch.clone();
    for slot in schedule.iter_mut() {
        // Logistics: the patch regrows first, exactly as `advance_forage_regrowth` runs before the
        // Population stage's gather.
        regrow_patch(&mut sim, forage);
        // Population: the same branch `project_realized_forage` and the real Forage arm both take — a
        // Field is a managed harvest (no drawdown, policy axis collapsed, worker-capped); every other
        // patch is the drawn-down policy gather through the shared `forage_take` path.
        *slot = if sim.is_field() {
            let production =
                field_provisions(&sim, tile_composition, forage, flora, output_multiplier);
            let collection = workers as f32
                * managed_per_worker_yield(
                    &sim,
                    tile_composition,
                    forage,
                    equipped_gather_rate,
                    flora,
                    output_multiplier,
                );
            production.min(collection)
        } else {
            forage_take(
                &mut sim,
                tile_composition,
                workers,
                floor,
                improvement,
                forage,
                flora,
                ladder,
                output_multiplier,
                per_worker_biomass_capacity,
                seasonal,
            )
            .to_f32()
        };
    }
    schedule
}

/// The assign-time yield telemetry seed for a **Forage** source: what staffing `patch` with `workers`
/// gatherers under `policy` will pay next turn, in the same shape the Forage arm of
/// `advance_labor_allocation` records after the take. Reuses `forage_forecast` (hence `forage_take`'s
/// own ceiling/conversion helpers) and the shared MSY `sustainable_yield`, so the seed is exactly the
/// number the turn then produces — no jump. The animal mirror is `fauna::hunt_source_yield_preview`.
// The seed composes the whole telemetry row, so it carries the full take context (see the sibling
// `project_realized_forage`).
#[allow(clippy::too_many_arguments)]
pub fn forage_source_yield_preview(
    patch: &ForagePatch,
    tile_composition: &[FloraShare],
    forage: &ForageLaborConfig,
    flora: &FloraConfig,
    equipped_gather_rate: f32,
    ladder: &LadderConfig,
    per_worker_biomass_capacity: f32,
    seasonal: f32,
    output_multiplier: f32,
    workers: u32,
    floor: f32,
    improvement: Option<Improvement>,
    realized_horizon: u32,
    arrivals_horizon: u32,
    // `combat_config.forecast_range_sigmas`. **The plant web has no stochastic stage** — no
    // engagement, no retreat, no fight — so its band is always a point whatever this says; it is
    // threaded so both webs seed their row through the one `fauna::forecast_source_yield`.
    range_sigmas: f32,
) -> SourceYield {
    let forecast = forage_forecast(
        patch,
        tile_composition,
        forage,
        flora,
        equipped_gather_rate,
        ladder,
        forage_per_worker_biomass(per_worker_biomass_capacity, seasonal),
        output_multiplier,
    );
    // The patch's OWN MSY (`patch_ecology`) — a tended patch's sustainable line sits on its boosted
    // curve, so a Sustain gather of it reads no ⚠ while a Surplus gather of it does. Reading
    // `forage.ecology` here would flag every tended Sustain as an overdraw.
    let sustainable = forage_provisions(
        sustainable_yield(
            patch.biomass,
            patch.carrying_capacity,
            &patch_ecology(patch, forage),
        ),
        patch_provisions_per_biomass(patch, tile_composition, flora, forage),
        output_multiplier,
    );
    // The steady headline is the forward projection from THIS patch state — the same computation the
    // resolved Forage arm runs, so seed == first resolved value exactly.
    let realized = project_realized_forage(
        patch,
        tile_composition,
        forage,
        flora,
        equipped_gather_rate,
        ladder,
        per_worker_biomass_capacity,
        seasonal,
        output_multiplier,
        workers,
        floor,
        improvement,
        realized_horizon,
    );
    // The discrete twin, from the same patch state: what lands on each of the next
    // `arrivals_horizon` turns. A gather is continuous, so this is normally positive throughout.
    let arrivals = project_arrivals_forage(
        patch,
        tile_composition,
        forage,
        flora,
        equipped_gather_rate,
        ladder,
        per_worker_biomass_capacity,
        seasonal,
        output_multiplier,
        workers,
        floor,
        improvement,
        arrivals_horizon,
    );
    // **`managed` is rung 3 ONLY** (slice 7). It marks the sources whose harvest cannot overdraw —
    // and since rung 2 went back to being a drawn-down wild stand, a *tended* patch can be over-farmed
    // like any other, so it must keep its real sustainable line and its real ⚠.
    forecast_source_yield(
        &forecast,
        sustainable,
        patch.is_field(),
        // **The plant web's standing crew is the BUILD's** — a patch has no herders, so what a crew is
        // owed regardless of the take is whatever rung it is preparing (`LadderConfig::build_crew`,
        // `NO_BUILD_CREW` for a pure gather). This is the *same* seam the resolved Forage arm floors
        // on, which is the point: the seed used to pass `0` here, so a freshly-composed `Cultivate`
        // inverted the dipped take alone and reported "only 1 of 2 working" against the compose
        // sheet's own "max 2 workers useful here" — until the next turn resolved and overwrote it.
        ladder.build_crew(improvement),
        workers,
        floor,
        improvement,
        realized,
        // The plant web's steady TRADE projection is the same gap the forecast carries — see
        // [`PLANT_TRADE_FORECAST_NOT_YET_PROJECTED`]. The trade a Deplete gather *actually* earns is
        // reported (the resolved row fills `SourceYield::trade`); only the projection is missing.
        PLANT_TRADE_FORECAST_NOT_YET_PROJECTED,
        arrivals,
        range_sigmas,
    )
}

/// TEMPORARY measurement harness (`#[ignore]`d) for the harvest-stance design review — it drives the
/// shipped take/regrowth/build functions of **both** webs forward and prints the tables. Lives here
/// because the plant half needs this module's private `regrow_patch`. Delete with the review.
#[cfg(test)]
mod stance_probe;

#[cfg(test)]
mod tests {
    /// **The shipped EQUIPPED haul rate** — what a kitted band drags, off the sled's own tier.
    /// `labor_config`'s `hunt.per_worker_biomass_capacity` is the *bare-handed* baseline since
    /// quality tiers landed, so a fixture that wants "an ordinary band" asks the item table.
    #[allow(dead_code)]
    fn equipped_haul_rate() -> f32 {
        crate::equipment_config::EquipmentConfig::builtin().equipped_reference(
            crate::equipment_config::EquipmentStat::HuntCarry,
            crate::labor_config::LaborConfig::builtin()
                .hunt
                .per_worker_biomass_capacity,
        )
    }

    /// The gather twin of [`equipped_haul_rate`] — the baskets' own tier.
    #[allow(dead_code)]
    fn equipped_gather_rate() -> f32 {
        crate::equipment_config::EquipmentConfig::builtin().equipped_reference(
            crate::equipment_config::EquipmentStat::ForageCarry,
            crate::labor_config::LaborConfig::builtin()
                .forage
                .per_worker_biomass_capacity,
        )
    }

    use super::*;
    use crate::components::NO_IMPROVEMENT_UNDERWAY;
    use crate::labor_config::LaborConfig;
    use sim_runtime::TerrainType;

    /// The **shipped** forage config (the per-biome capacity table lives only in the JSON — the
    /// struct default is deliberately empty, so `ForageLaborConfig::default()` would read every
    /// biome as barren). Mirrors `graze::tests::test_graze_config`.
    fn test_forage_config() -> ForageLaborConfig {
        LaborConfig::builtin().forage.clone()
    }

    /// The biome the patch-mechanics tests stand their patch on. Any positive-capacity biome works
    /// (the mechanics are cap-relative); `AlluvialPlain` is the richest common human ground and the
    /// one a `RiverineDelta` food module actually sits on.
    const TEST_BIOME: TerrainType = TerrainType::AlluvialPlain;

    /// **The basket of a patch standing on no tile at all** — empty, which is exactly what these
    /// mechanics tests want: they exercise biomass/regrowth/policy, not composition, and an empty
    /// basket makes every rate fall back to `forage.provisions_per_biomass`, the number the
    /// pre-basket assertions were written against. The basket's *own* behaviour is pinned in
    /// `tests/forage_basket_reweight.rs`.
    const NO_BASKET: &[FloraShare] = &[];

    /// A navigable river keeps the valley it cut: it stays mechanically `NavigableRiver`, but its
    /// RESOURCE reads route through the preserved underlying biome (`resource_terrain`), and it is
    /// always a fishery (forage gets the river bonus on top of the underlying; graze gets the plain
    /// underlying value — you don't pasture on the channel).
    #[test]
    fn navigable_hex_reads_underlying_biome_plus_river_forage_bonus() {
        use crate::fauna_config::FaunaConfig;
        use sim_runtime::{TerrainTags, TerrainType};

        let forage = test_forage_config();
        let graze = FaunaConfig::builtin().graze.clone();

        // A navigable hex cut through fertile grassland: mechanically water, underlying preserved.
        let underlying = TerrainType::PrairieSteppe;
        let navigable = Tile {
            terrain: TerrainType::NavigableRiver,
            terrain_tags: TerrainTags::WATER | TerrainTags::FRESHWATER,
            underlying_terrain: Some(underlying),
            ..Default::default()
        };

        // Terrain stays NavigableRiver (movement/naval unchanged); resources read the valley.
        assert_eq!(navigable.terrain, TerrainType::NavigableRiver);
        assert_eq!(navigable.resource_terrain(), underlying);

        // Forage = underlying + river fishing bonus (the seeded patch cap, via the SHARED helper).
        let expected_forage = forage.capacity_for(underlying) + forage.navigable_river_forage_bonus;
        assert_eq!(tile_forage_capacity(&forage, &navigable), expected_forage);
        assert!(expected_forage > forage.capacity_for(underlying)); // strictly richer than dry land

        // Graze = the underlying biome's pasture, no bonus.
        assert_eq!(
            graze.capacity_for(navigable.resource_terrain()),
            graze.capacity_for(underlying)
        );
        assert!(graze.capacity_for(navigable.resource_terrain()) > 0.0); // grassland grazes

        // Even over an otherwise-barren biome (no human food), a navigable hex STILL seeds a patch —
        // a navigable river is always a fishery — at just the bonus.
        let barren = TerrainType::Glacier;
        assert_eq!(forage.capacity_for(barren), NO_FORAGE_CAPACITY);
        let navigable_over_barren = Tile {
            terrain: TerrainType::NavigableRiver,
            underlying_terrain: Some(barren),
            ..Default::default()
        };
        assert_eq!(
            tile_forage_capacity(&forage, &navigable_over_barren),
            forage.navigable_river_forage_bonus
        );
        assert!(tile_forage_capacity(&forage, &navigable_over_barren) > NO_FORAGE_CAPACITY);
    }

    #[test]
    fn sustain_on_full_patch_yields_msy_and_draws_to_half_cap() {
        // Regression (Phase 0 bug): a patch AT carrying capacity used to yield 0 under Sustain
        // (logistic regrowth is 0 at K), so a full patch stayed stuck at 0 forever. The MSY-based
        // `sustainable_yield` ceiling skims regrowth at the most-productive biomass (K/2), so a
        // full patch yields a positive harvest and Sustain draws it DOWN toward K/2 and holds.
        let forage = test_forage_config();
        let cap = forage.capacity_for(TEST_BIOME);
        let half_cap = cap * 0.5;
        let msy = sustainable_yield(cap, cap, &forage.ecology);
        assert!(
            msy > 0.0,
            "a full patch must be sustainably harvestable: {msy}"
        );

        // Seed FULL, exactly as real forage patches spawn.
        let mut patch = ForagePatch::new(UVec2::new(1, 1), cap);
        patch.biomass = cap;
        patch.refresh_ecology_phase(&forage.ecology);
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);

        // First Sustain gather off the full patch: **the standing surplus above `K/2`**, capped by
        // what the crew can carry. Under constant escapement the first harvest of an untouched patch
        // is the accumulated stock, not a rate — the crew empties the store the patch built up before
        // anyone worked it, and lands it exactly on its most productive biomass.
        let biomass_before = patch.biomass;
        let crew_cap = 20.0 * forage_per_worker_biomass(equipped_gather_rate(), 1.0);
        let expected_first = crew_cap.min(biomass_before - half_cap);
        let provisions = forage_take(
            &mut patch,
            NO_BASKET,
            20,
            0.5,
            NO_IMPROVEMENT_UNDERWAY,
            &forage,
            &FloraConfig::builtin(),
            &LadderConfig::builtin(),
            1.0,
            equipped_gather_rate(),
            1.0,
        );
        let take = biomass_before - patch.biomass;
        assert!(
            take > 0.0,
            "a full patch under Sustain must yield > 0: {take}"
        );
        assert!(
            (take - expected_first).abs() < 1e-3,
            "the first gather is the escapement surplus (crew permitting): {take} vs {expected_first}"
        );
        let actual = provisions.to_f32();
        assert!(
            (actual - take * forage.provisions_per_biomass).abs() < 1e-3,
            "the provisions paid are the take through the patch's own rate: {actual}"
        );
        assert!(
            patch.biomass >= half_cap - 1e-3,
            "Sustain never draws a patch below its escapement floor: {} vs {half_cap}",
            patch.biomass
        );

        // Over many take+regrowth turns Sustain draws the patch DOWN from full and then HOLDS: the
        // post-take biomass settles at the MSY point (K/2), so the stored biomass stabilizes just
        // above K/2 and the per-turn yield stays ≈ MSY (never falling back to 0).
        let mut prev = patch.biomass;
        let mut last_take = take;
        for turn in 0..200 {
            let before = patch.biomass;
            let _ = forage_take(
                &mut patch,
                NO_BASKET,
                20,
                0.5,
                NO_IMPROVEMENT_UNDERWAY,
                &forage,
                &FloraConfig::builtin(),
                &LadderConfig::builtin(),
                1.0,
                equipped_gather_rate(),
                1.0,
            );
            last_take = before - patch.biomass;
            regrow_patch(&mut patch, &forage);
            if turn >= 190 {
                assert!(
                    (patch.biomass - prev).abs() < 1.0,
                    "late turns: biomass has stabilized: {} vs {}",
                    patch.biomass,
                    prev
                );
            }
            prev = patch.biomass;
        }
        assert!(
            patch.biomass < cap,
            "Sustain drew the full patch down: {}",
            patch.biomass
        );
        assert!(
            patch.biomass > half_cap,
            "Sustain holds at/above the MSY point K/2: {} vs {}",
            patch.biomass,
            half_cap
        );
        assert!(
            (last_take - msy).abs() < 1e-3 && last_take > 0.0,
            "steady-state yield stays ≈ MSY: {last_take} vs {msy}"
        );
    }

    #[test]
    fn heavy_take_depletes_patch_and_drops_phase() {
        let forage = test_forage_config();
        let cap = forage.capacity_for(TEST_BIOME);
        let mut patch = ForagePatch::new(UVec2::new(2, 3), cap);
        patch.refresh_ecology_phase(&forage.ecology);
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);

        // A heavier-than-sustainable draw (non-Sustain ceiling = throughput only) with enough
        // workers to out-pace regrowth drives biomass DOWN turn over turn and drops the phase.
        let mut last = patch.biomass;
        let mut saw_stressed = false;
        for _ in 0..40 {
            let _ = forage_take(
                &mut patch,
                NO_BASKET,
                3,
                0.0,
                NO_IMPROVEMENT_UNDERWAY,
                &forage,
                &FloraConfig::builtin(),
                &LadderConfig::builtin(),
                1.0,
                equipped_gather_rate(),
                1.0,
            );
            regrow_patch(&mut patch, &forage);
            assert!(patch.biomass < last + 1e-3, "biomass must trend downward");
            last = patch.biomass;
            if patch.ecology_phase == EcologyPhase::Stressed {
                saw_stressed = true;
            }
        }
        assert!(
            saw_stressed,
            "phase should pass through Stressed while depleting"
        );
        assert_eq!(patch.ecology_phase, EcologyPhase::Collapsing);
        assert!(patch.biomass < forage.ecology.collapse_fraction * cap);
    }

    /// The forage policy axis (parity with hunting): on an identical Thriving patch with ample
    /// workers (so the take is ceiling-bound, not throughput-bound), a **deeper floor** takes more —
    /// `Sustain ≤ Surplus < Deplete < Eradicate` — and the deeper floors deplete the patch faster
    /// (biomass drops more in a single turn).
    #[test]
    fn policy_ceilings_order_take_and_depletion() {
        let forage = test_forage_config();
        let cap = forage.capacity_for(TEST_BIOME);
        let start = 0.8 * cap; // Thriving, clear positive net regrowth.
        let workers = 20; // worker_cap (20 × per_worker) far exceeds every policy ceiling.

        // One-turn take under each policy from the same starting biomass.
        let take_under = |policy: f32| -> (f32, f32) {
            let mut patch = ForagePatch::new(UVec2::new(1, 1), cap);
            patch.biomass = start;
            let provisions = forage_take(
                &mut patch,
                NO_BASKET,
                workers,
                policy,
                NO_IMPROVEMENT_UNDERWAY,
                &forage,
                &FloraConfig::builtin(),
                &LadderConfig::builtin(),
                1.0,
                equipped_gather_rate(),
                1.0,
            );
            let take = start - patch.biomass;
            (take, provisions.to_f32())
        };

        let (sustain_take, _) = take_under(0.5);
        let (surplus_take, _) = take_under(0.3);
        let (deplete_take, _) = take_under(0.15);
        let (eradicate_take, _) = take_under(0.0);

        // Sustain is the regrowth skim; Surplus overdraws it; Deplete/Eradicate strip a share.
        assert!(sustain_take <= surplus_take + 1e-4, "Sustain ≤ Surplus");
        assert!(surplus_take < deplete_take, "Surplus < Deplete");
        assert!(deplete_take < eradicate_take, "Deplete < Eradicate");
        // Heavier policies deplete the patch faster (more biomass removed this turn).
        assert!(
            deplete_take > sustain_take,
            "Deplete depletes faster than Sustain"
        );
        assert!(
            eradicate_take > sustain_take,
            "Eradicate depletes faster than Sustain"
        );
        // Sustain takes exactly the stock standing above its escapement floor — `B − K/2` at
        // `B = 0.8·K` — and so leaves the patch **on** its most productive biomass, never below it.
        let expected_sustain = start - cap * crate::fauna::MSY_BIOMASS_FRACTION;
        assert!(
            (sustain_take - expected_sustain).abs() < 1e-3,
            "Sustain takes `B - K/2`: {sustain_take} vs {expected_sustain}"
        );
    }

    #[test]
    fn below_cap_patch_regrows_toward_cap() {
        let forage = test_forage_config();
        let cap = forage.capacity_for(TEST_BIOME);
        let mut patch = ForagePatch::new(UVec2::new(0, 0), cap);
        patch.biomass = 0.25 * cap;
        patch.refresh_ecology_phase(&forage.ecology);

        let mut prev = patch.biomass;
        for _ in 0..30 {
            regrow_patch(&mut patch, &forage);
            assert!(patch.biomass >= prev, "regrowth must be monotonic upward");
            prev = patch.biomass;
        }
        // Converges toward the cap.
        assert!(patch.biomass > 0.9 * cap);
        assert!(patch.biomass <= cap);
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);
    }

    #[test]
    fn crashed_patch_recovers_no_extinction() {
        // Pure-logistic regrowth: a patch driven far below the Allee threshold still recovers
        // (plants have no critical-depensation crash / extinction floor).
        let forage = test_forage_config();
        let cap = forage.capacity_for(TEST_BIOME);
        let mut patch = ForagePatch::new(UVec2::new(4, 4), cap);
        patch.biomass = 0.02 * cap;
        patch.refresh_ecology_phase(&forage.ecology);
        assert_eq!(patch.ecology_phase, EcologyPhase::Collapsing);

        for _ in 0..80 {
            regrow_patch(&mut patch, &forage);
        }
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);
        assert!(patch.biomass > forage.ecology.stressed_fraction * cap);
    }

    #[test]
    fn zero_biomass_patch_reseeds_and_recovers() {
        // Regression: a patch driven to *exactly* 0 (repeated Eradicate + f32 underflow,
        // `take_fraction = 1.0`, or a snapshot restore carrying biomass = 0) used to be stuck at 0
        // forever, because `logistic_regrowth(0, ..) == 0`. The reseed floor lifts a depleted patch
        // to a small standing crop each turn, so it recovers via normal regrowth — the "a feral
        // patch always recovers" invariant is now backed by code, not just the docstring.
        let forage = test_forage_config();
        let cap = forage.capacity_for(TEST_BIOME);
        let floor = forage.reseed_floor_fraction * cap;
        assert!(floor > 0.0, "reseed floor must be a positive standing crop");

        let mut patch = ForagePatch::new(UVec2::new(5, 5), cap);
        patch.biomass = 0.0;
        patch.refresh_ecology_phase(&forage.ecology);

        // One turn off dead-zero: reseeded to the floor and already regrowing above it (> 0).
        regrow_patch(&mut patch, &forage);
        assert!(
            patch.biomass > 0.0,
            "a 0-biomass patch must escape 0 via the reseed floor: {}",
            patch.biomass
        );
        assert!(patch.biomass >= floor);

        // Over subsequent turns it recovers toward a healthy level (Thriving), just like a patch
        // seeded a hair above 0 — no permanent stall at 0.
        for _ in 0..80 {
            regrow_patch(&mut patch, &forage);
        }
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);
        assert!(patch.biomass > forage.ecology.stressed_fraction * cap);
    }

    #[test]
    fn continuous_eradicate_bottoms_at_floor_then_recovers() {
        // The floor is small enough that Eradicate still crashes the patch hard (into Collapsing),
        // but it can't drive it *permanently* to 0: the patch bottoms out at ~the reseed floor and
        // recovers once Eradicate stops.
        let forage = test_forage_config();
        let cap = forage.capacity_for(TEST_BIOME);
        let floor = forage.reseed_floor_fraction * cap;
        let mut patch = ForagePatch::new(UVec2::new(6, 6), cap);
        patch.refresh_ecology_phase(&forage.ecology);

        // Hammer with Eradicate + regrowth: biomass crashes but never sits at 0 — it floats at/above
        // the reseed floor while still reading Collapsing (a hard crash, not extinction).
        for _ in 0..60 {
            let _ = forage_take(
                &mut patch,
                NO_BASKET,
                50,
                0.0,
                NO_IMPROVEMENT_UNDERWAY,
                &forage,
                &FloraConfig::builtin(),
                &LadderConfig::builtin(),
                1.0,
                equipped_gather_rate(),
                1.0,
            );
            regrow_patch(&mut patch, &forage);
            assert!(
                patch.biomass > 0.0,
                "Eradicate must not permanently zero a patch"
            );
        }
        assert!(
            patch.biomass < cap * forage.ecology.collapse_fraction,
            "Eradicate still crashes the patch hard: {} vs {}",
            patch.biomass,
            cap * forage.ecology.collapse_fraction
        );
        assert_eq!(patch.ecology_phase, EcologyPhase::Collapsing);

        // Stop hunting: from the crashed floor the patch recovers all the way back to Thriving.
        for _ in 0..120 {
            regrow_patch(&mut patch, &forage);
        }
        assert_eq!(patch.ecology_phase, EcologyPhase::Thriving);
        assert!(patch.biomass >= floor);
    }

    #[test]
    fn reseed_floor_leaves_healthy_patch_regrowth_unchanged() {
        // A patch above the floor must regrow identically with or without the reseed lift (the floor
        // only reseeds depleted patches — a healthy patch is untouched).
        let forage = test_forage_config();
        // The "no reseed" baseline — the shipped config with only the lift switched off.
        let no_floor_forage = ForageLaborConfig {
            reseed_floor_fraction: 0.0,
            ..forage.clone()
        };
        let cap = forage.capacity_for(TEST_BIOME);
        let start = 0.5 * cap; // comfortably above reseed_floor_fraction × cap.

        let mut with_floor = ForagePatch::new(UVec2::new(7, 7), cap);
        with_floor.biomass = start;
        let mut without_floor = ForagePatch::new(UVec2::new(8, 8), cap);
        without_floor.biomass = start;

        for _ in 0..30 {
            regrow_patch(&mut with_floor, &forage);
            // A zero floor is the "no reseed" baseline.
            regrow_patch(&mut without_floor, &no_floor_forage);
        }
        assert!(
            (with_floor.biomass - without_floor.biomass).abs() < 1e-6,
            "reseed floor must not perturb a healthy patch's regrowth: {} vs {}",
            with_floor.biomass,
            without_floor.biomass
        );
    }

    #[test]
    fn sustainable_yield_is_zero_below_allee() {
        // A collapsing (sub-Allee) patch is not sustainably harvestable.
        let forage = test_forage_config();
        let cap = forage.capacity_for(TEST_BIOME);
        let below_allee = forage.ecology.collapse_fraction * cap * 0.5;
        assert_eq!(
            sustainable_yield(below_allee, cap, &forage.ecology),
            0.0,
            "a collapsing patch has no sustainable yield"
        );
    }

    #[test]
    fn sustainable_yield_plateaus_at_msy_above_half_cap() {
        // For any healthy biomass (>= K/2) the MSY ceiling is flat at the K/2 peak.
        let forage = test_forage_config();
        let cap = forage.capacity_for(TEST_BIOME);
        let msy = sustainable_yield(cap * 0.5, cap, &forage.ecology);
        assert!(msy > 0.0);
        for frac in [0.5_f32, 0.6, 0.75, 0.9, 1.0] {
            assert!(
                (sustainable_yield(cap * frac, cap, &forage.ecology) - msy).abs() < 1e-6,
                "flat MSY plateau at biomass = {frac}·K"
            );
        }
    }

    #[test]
    fn cultivation_accrual_is_owner_locked_and_clamped() {
        let mut patch = ForagePatch::new(UVec2::new(1, 1), 120.0);
        // First accrual claims ownership for the acting faction.
        patch.accrue_cultivation(FactionId(0), 0.3);
        assert_eq!(patch.owner, Some(FactionId(0)));
        assert!((patch.cultivation_progress - 0.3).abs() < 1e-6);
        // A different faction cannot accrue on an already-owned patch.
        patch.accrue_cultivation(FactionId(1), 0.5);
        assert_eq!(patch.owner, Some(FactionId(0)));
        assert!((patch.cultivation_progress - 0.3).abs() < 1e-6);
        // Owner accrues; progress clamps at 1.0 and latches cultivated.
        patch.accrue_cultivation(FactionId(0), 0.9);
        assert!(patch.is_cultivated());
        assert_eq!(patch.cultivation_progress, 1.0);
        // A cultivated patch is a no-op for further accrual.
        patch.accrue_cultivation(FactionId(0), 0.5);
        assert_eq!(patch.cultivation_progress, 1.0);
    }

    #[test]
    fn cultivation_decay_clears_owner_at_zero_and_takes_cultivated_feral() {
        let mut patch = ForagePatch::new(UVec2::new(2, 2), 120.0);
        patch.accrue_cultivation(FactionId(0), 0.05);
        patch.decay_cultivation(0.02);
        assert!((patch.cultivation_progress - 0.03).abs() < 1e-6);
        assert_eq!(patch.owner, Some(FactionId(0)), "owner held above zero");
        // Decaying to zero clears ownership so another faction can later tend it.
        patch.decay_cultivation(1.0);
        assert_eq!(patch.cultivation_progress, 0.0);
        assert_eq!(patch.owner, None);
        // Rung 1a: a cultivated patch now DOES decay when decayed (an untended tended patch goes
        // feral) — it reverts to wild the moment progress drops below 1.0.
        patch.cultivation_progress = 1.0;
        patch.owner = Some(FactionId(1));
        assert!(patch.is_cultivated());
        patch.decay_cultivation(0.5);
        assert!(
            !patch.is_cultivated(),
            "an untended tended patch reverts to wild"
        );
        assert!((patch.cultivation_progress - 0.5).abs() < 1e-6);
    }

    /// **The commitment is recorded once and released only by going fully feral** (Flora Roster S1).
    /// Re-deciding which crop a patch is every turn would erase the decision the rung exists to
    /// make; keeping it after both meters lapse would leave a wild stand wearing one plant's name.
    #[test]
    fn a_species_commitment_is_one_way_and_lapses_only_when_the_patch_goes_fully_feral() {
        let mut patch = ForagePatch::new(UVec2::new(3, 4), 120.0);
        patch.commit_species("wild_emmer");
        assert_eq!(patch.species.as_deref(), Some("wild_emmer"));
        // One-way while the ground is committed: a later assignment cannot re-crop it for free.
        patch.commit_species("wild_tubers");
        assert_eq!(patch.species.as_deref(), Some("wild_emmer"));

        // A patch with *either* meter still standing keeps its crop...
        patch.accrue_cultivation(FactionId(0), RUNG_COMPLETE);
        patch.accrue_field(FactionId(0), RUNG_COMPLETE);
        patch.decay_field(RUNG_COMPLETE);
        assert_eq!(
            patch.species.as_deref(),
            Some("wild_emmer"),
            "a lapsed Field over a standing tended patch is still that crop"
        );
        // ...and lapses only when nothing is left of either.
        patch.decay_cultivation(RUNG_COMPLETE);
        assert_eq!(patch.cultivation_progress, 0.0);
        assert_eq!(
            patch.species, None,
            "a fully feral patch is the wild basket again"
        );
        assert_eq!(patch.owner, None);
    }

    /// Rung 1a feral mechanic (`advance_cultivation` decay side, tested at the patch level): a
    /// cultivated patch tended this turn is spared; an untended one goes feral — it reverts to wild
    /// after the first untended turn and fully decays to 0 (owner cleared) over ~`1/decay_per_turn`
    /// turns. Replicates the system's `if !(is_cultivated && tended_this_turn) { decay }; clear`.
    #[test]
    fn tended_patch_spared_untended_goes_feral() {
        let forage = test_forage_config();
        // The feral rate is the `plant:tended` rung's build decay — the same value
        // `advance_cultivation` bleeds.
        let ladder = LadderConfig::builtin();
        let decay = ladder
            .rung(RungKey::PlantTended)
            .build_decay(RUNG_TIMESCALE_UNSCALED);
        assert!(decay > 0.0);

        // Tended every turn → never decays, stays cultivated.
        let mut tended = ForagePatch::new(UVec2::new(1, 1), forage.capacity_for(TEST_BIOME));
        tended.cultivation_progress = 1.0;
        tended.owner = Some(FactionId(0));
        for _ in 0..200 {
            tended.tended_this_turn = true; // labor arm marks it worked
            if !(tended.is_cultivated() && tended.tended_this_turn) {
                tended.decay_cultivation(decay);
            }
            tended.tended_this_turn = false;
        }
        assert!(tended.is_cultivated(), "a tended patch never decays");
        assert_eq!(tended.owner, Some(FactionId(0)));

        // Untended → feral. Reverts to wild after the first untended turn, then fully decays to 0.
        let mut feral = ForagePatch::new(UVec2::new(2, 2), forage.capacity_for(TEST_BIOME));
        feral.cultivation_progress = 1.0;
        feral.owner = Some(FactionId(0));
        // Turn 1 untended: decays below 1.0 → no longer cultivated.
        if !(feral.is_cultivated() && feral.tended_this_turn) {
            feral.decay_cultivation(decay);
        }
        feral.tended_this_turn = false;
        assert!(
            !feral.is_cultivated(),
            "one untended turn reverts a farm to wild"
        );
        // Over ~1/decay_per_turn total turns it fully decays and clears ownership.
        let turns_to_zero = (1.0_f32 / decay).ceil() as usize + 2;
        for _ in 0..turns_to_zero {
            if !(feral.is_cultivated() && feral.tended_this_turn) {
                feral.decay_cultivation(decay);
            }
            feral.tended_this_turn = false;
        }
        assert_eq!(feral.cultivation_progress, 0.0, "feral patch fully reverts");
        assert_eq!(feral.owner, None, "ownership lapses once fully feral");
    }

    #[test]
    fn cultivated_count_filters_by_owner() {
        let mut registry = ForageRegistry::default();
        let mut a = ForagePatch::new(UVec2::new(0, 0), 120.0);
        a.cultivation_progress = 1.0;
        a.owner = Some(FactionId(0));
        let mut b = ForagePatch::new(UVec2::new(1, 0), 120.0);
        b.cultivation_progress = 1.0;
        b.owner = Some(FactionId(1));
        let uncultivated = ForagePatch::new(UVec2::new(2, 0), 120.0);
        registry.patches.insert(a.tile, a);
        registry.patches.insert(b.tile, b);
        registry.patches.insert(uncultivated.tile, uncultivated);
        assert_eq!(registry.cultivated_count(FactionId(0)), 1);
        assert_eq!(registry.cultivated_count(FactionId(1)), 1);
        assert_eq!(registry.cultivated_count(FactionId(2)), 0);
    }
}
