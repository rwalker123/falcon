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
//! turn toward `carrying_capacity`. The patch's state round-trips through the rollback snapshot via
//! the shared `sim_schema::ForageState`/`EcologyState` records — the same 0-i persistence pattern
//! the `HerdRegistry` uses (`ForageRegistry::from_states`/`update_from_states`).
//!
//! Unlike a wild herd, a patch uses **pure logistic regrowth** (no Allee / critical-depensation
//! crash) and **never despawns** — plants reseed, so a depleted (feral) patch always recovers. A
//! small **reseed floor** (`forage.reseed_floor_fraction × carrying_capacity`) lifts a fully-depleted
//! patch back to a seed stock before regrowth each turn, so even a patch driven to exactly `0`
//! (Eradicate / f32 underflow / a restored `biomass = 0`) recovers rather than sticking at `0`. The
//! Allee branch of `net_biomass_delta` (via `sustainable_yield`) still sizes the **Sustain** gather
//! ceiling (so a collapsed patch yields no sustainable surplus). Foraging honors the full policy axis
//! (Sustain/Surplus/Market/Eradicate — §0-iii, parity with hunting): the `LaborTarget::Forage`
//! policy flows through `advance_labor_allocation` into `forage_take`, and a Market gather sells its
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
//! shape at twice the rate. Unlike every other rung, **it needs no source below it**: seed travels, so
//! sowing a qualifying tile with no spawned patch *creates* one (`ForagePatch::sown`), at that tile's
//! own biome capacity.
//!
//! **Where it may be sown is SCARCE, and that is the mechanic** — rung 3 moves seed but cannot
//! fertilize, so the land must already be **very fertile** *and* **near fresh water** (the
//! `plant:field` rung's `site_requirement`; `rung_site_refusal` + `tile_is_fresh_watered` are the one
//! seam the command, the labor arm and the wire all judge through). **46 of 4160 tiles** on the
//! standard map — the river valleys. Thin or dry ground waits for rung 4 (Worked Land). Design:
//! `docs/plan_intensification_ladder.md` §2.

use std::{borrow::Cow, collections::HashMap};

use bevy::prelude::*;
use sim_schema::ForageState;

use crate::{
    components::{FollowPolicy, SourceYield, Tile},
    fauna::{
        classify_ecology_phase, forecast_source_yield, reseeding_logistic_regrowth,
        sustainable_yield, EcologyPhase, SourceYieldForecast, NO_PASTORAL_YIELD,
    },
    fauna_config::EcologyConfig,
    flora_config::{FloraConfig, FloraShare},
    food::FoodModuleTag,
    intensification::{
        LadderConfig, LadderConfigHandle, RungDef, RungKey, SiteRefusal, RUNG_COMPLETE,
        RUNG_TIMESCALE_UNSCALED,
    },
    labor_config::{ForageLaborConfig, LaborConfigHandle, NO_FORAGE_CAPACITY},
    orders::FactionId,
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
    /// this patch under the explicit `FollowPolicy::Cultivate` policy (faction knows Cultivation +
    /// patch Thriving); decays on a patch nobody is working (see `advance_cultivation`). The plant
    /// mirror of `Herd::corral_progress`.
    pub cultivation_progress: f32,
    /// **Field**-build progress in `[0.0, 1.0]`; `1.0` = a sown Field (the plant ladder's **rung 3**).
    /// Accrues only while a band works this patch under the explicit `FollowPolicy::Sow` policy
    /// (faction knows **Seed Selection**); decays on a patch nobody is working (see
    /// `advance_cultivation`). The plant mirror of `Herd::corral_progress` — and, exactly like the
    /// herd's two meters, it is **its own** meter rather than a second reading of
    /// `cultivation_progress`: a branch with two investment rungs carries two meters, one per rung.
    ///
    /// **Independent of `cultivation_progress`, deliberately.** `Sow` needs no prior patch (§2 — seed
    /// travels), so a Field may stand on ground that was never tended, and a Field that lapses simply
    /// reveals whatever rung the tile still supports underneath (today: wild, since the same untended
    /// turn bleeds both meters).
    pub field_progress: f32,
    /// Faction tending/owning this patch (`Some` iff either improvement meter is `> 0`).
    pub owner: Option<FactionId>,
    /// Transient per-turn flag: a Forage assignment **worked this patch as an improvement** this turn
    /// — tending a completed patch/Field, or preparing one under `FollowPolicy::Cultivate`/`Sow` (set in
    /// `advance_labor_allocation`, Population). `advance_cultivation` (Logistics, the *next* turn —
    /// Logistics runs before Population) reads it to decide feral/decay vs. spared, then clears it.
    /// Sparing a *preparing* patch too is what makes the investment accrue at the full
    /// `progress_per_turn` (25 turns) rather than net-of-decay. **Not** snapshot-persisted (derived,
    /// transient) — a rehydrated patch reads `true` for **one turn** (a deliberate grace, seeded in
    /// `forage_patch_from_state`), so the first post-restore Logistics decay pass — which runs before
    /// the labor arm can re-mark a patch a band is working — spares it rather than reverting a tended
    /// patch / Field a band tends every turn. A genuinely abandoned patch still goes feral next turn;
    /// a rollback can only *delay* a feral reversion by one turn, never resurrect a farm.
    pub tended_this_turn: bool,
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
            owner: None,
            tended_this_turn: false,
        }
    }

    /// **A patch a crew has just put seed into** — the plant rung-3 verb's create-from-nothing case
    /// (`FollowPolicy::Sow` on hospitable ground that carried no forage site at all,
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

    /// Accrue cultivation progress for `faction` (the preparing band, working the patch under
    /// `FollowPolicy::Cultivate`). Sets ownership on the first accrual; only the owner makes progress.
    /// Clamped to 1.0 — reaching it makes the patch a tended crop from the *next* turn's payout on
    /// (the accrual runs after this turn's take, so the pre-commit forecast can't lie). No-op once the
    /// patch is cultivated. Mirrors `Herd::accrue_corral`.
    pub(crate) fn accrue_cultivation(&mut self, faction: FactionId, amount: f32) {
        if self.is_cultivated() {
            return;
        }
        if self.owner.is_none() {
            self.owner = Some(faction);
        }
        if self.owner == Some(faction) {
            self.cultivation_progress = (self.cultivation_progress + amount).min(RUNG_COMPLETE);
        }
    }

    /// Accrue **Field**-build progress for `faction` (the sowing band, working the patch under
    /// `FollowPolicy::Sow`) — the exact twin of `accrue_cultivation` one rung up, with the same
    /// owner-locking, the same clamp, and the same "no-op once complete". Mirrors `Herd::accrue_corral`.
    pub(crate) fn accrue_field(&mut self, faction: FactionId, amount: f32) {
        if self.is_field() {
            return;
        }
        if self.owner.is_none() {
            self.owner = Some(faction);
        }
        if self.owner == Some(faction) {
            self.field_progress = (self.field_progress + amount).min(RUNG_COMPLETE);
        }
    }

    /// Decay cultivation progress toward zero by `amount`. Applies to **any** patch — a completed
    /// (`is_cultivated`) patch decays too (going feral once it drops below `1.0`, reverting to a wild
    /// gather patch); the *caller* (`advance_cultivation`) decides when to spare a worked patch.
    /// Mirrors `Herd::decay_domestication` (minus the domesticated short-circuit — a tended patch left
    /// untended is meant to go feral).
    pub(crate) fn decay_cultivation(&mut self, amount: f32) {
        self.cultivation_progress = (self.cultivation_progress - amount).max(0.0);
        self.reconcile_owner();
    }

    /// Decay **Field**-build progress toward zero by `amount` — the rung-3 twin of
    /// `decay_cultivation`, and (unlike the pen, which is lost outright when its herd bolts) a
    /// *gradual* bleed for the same reason cultivation bleeds gradually: **a patch is a place and a
    /// herd is not**, so leftover progress still refers to the same ground.
    pub(crate) fn decay_field(&mut self, amount: f32) {
        self.field_progress = (self.field_progress - amount).max(0.0);
        self.reconcile_owner();
    }

    /// Hold the `owner is Some ⟺ some improvement remains` invariant: ownership lapses only once
    /// **both** meters are spent, so a decaying Field doesn't strand a stale owner (which would block
    /// another faction from ever working the tile) and doesn't drop its owner while its cultivation —
    /// or its own remaining progress — is still standing.
    fn reconcile_owner(&mut self) {
        if self.cultivation_progress <= 0.0 && self.field_progress <= 0.0 {
            self.owner = None;
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

    /// Rebuild the authoritative patch map from a rollback snapshot's `ForageState`s (clear +
    /// rebuild), mirroring `HerdRegistry::update_from_states`. Restores per-patch biomass / phase so
    /// a rollback rewinds forage depletion, not just display state.
    pub fn update_from_states(&mut self, states: &[ForageState]) {
        self.patches = states
            .iter()
            .map(|state| {
                let patch = forage_patch_from_state(state);
                (patch.tile, patch)
            })
            .collect();
    }

    /// Construct a registry directly from snapshot `ForageState`s (mirrors
    /// `HerdRegistry::from_states`).
    pub fn from_states(states: &[ForageState]) -> Self {
        let mut registry = Self::default();
        registry.update_from_states(states);
        registry
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

/// Reconstruct a live `ForagePatch` from its snapshot mirror (the rollback restore side of
/// `snapshot::forage_state`). The `progress`/`owner` `EcologyState` fields carry cultivation
/// (Phase 1a) and the record's own `field_progress` the rung-3 Field meter, mirroring
/// `herd_from_state` (whose `corral_progress` likewise sits beside the ecology's own progress).
fn forage_patch_from_state(state: &ForageState) -> ForagePatch {
    ForagePatch {
        tile: UVec2::new(state.x, state.y),
        biomass: state.ecology.biomass,
        carrying_capacity: state.ecology.carrying_capacity,
        ecology_phase: EcologyPhase::from_key(&state.ecology.ecology_phase),
        cultivation_progress: state.ecology.progress,
        field_progress: state.field_progress,
        owner: state.ecology.owner.map(FactionId),
        // Transient (not persisted) — seeded `true` for a **one-turn grace**: the same signal the
        // Cultivate/Sow arm of `advance_labor_allocation` sets on any patch a band worked this turn,
        // and the plant twin of the grace `Herd::corral_at` grants a freshly-penned herd. The
        // rollback-restore path runs the Logistics decay pass (`advance_cultivation`) *before* the
        // Population labor arm can re-mark a patch a band is working, so seeding this `false` would
        // decay a tended patch / Field one tick on the very first post-restore turn — flipping
        // `is_managed()` false and destroying the improvement even while a band tends it every turn.
        // The grace spares exactly that turn; a genuinely abandoned patch still goes feral next turn.
        tended_this_turn: true,
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

/// **Does `rung`'s site requirement admit this tile?** — the one place the two readings a
/// [`RungSiteRequirement`] judges (the tile's own forage capacity, and whether it is fresh-watered)
/// are gathered, so the `sow` command's rejection and the labor arm's placement gate cannot drift into
/// disagreeing about which ground is farmable.
///
/// `None` = the rung asks nothing of the site, or the land permits it. `Some(refusal)` says **which**
/// way the ground fell short, so the caller can phrase *too poor* and *too dry* distinctly (they are
/// different problems with different answers — move, or wait for rung 4).
pub fn rung_site_refusal(
    rung: &RungDef,
    tile: &Tile,
    forage: &ForageLaborConfig,
    fresh_water: bool,
) -> Option<SiteRefusal> {
    rung.site_requirement
        .as_ref()?
        .refusal(tile_forage_capacity(forage, tile), fresh_water)
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
/// channel is), so it gets the blended basket ([`FloraConfig::navigable_composition`]); every other
/// tile reads its own biome's basket directly. Borrowed for the common case, owned only for the
/// navigable blend — the blend is the only shape that has to be built.
///
/// Every caller (today: the snapshot capture) must go through this, never
/// [`FloraConfig::composition`] on a raw terrain: reading the underlying biome alone on a navigable
/// hex leaves that hex's fishery bonus **unnamed**, which breaks the decomposition ruling on a whole
/// class of tiles and is invisible to `validate_against_forage`.
pub fn tile_flora_composition<'a>(
    flora: &'a FloraConfig,
    forage: &ForageLaborConfig,
    tile: &Tile,
) -> Cow<'a, [FloraShare]> {
    if tile.terrain == sim_runtime::TerrainType::NavigableRiver {
        Cow::Owned(flora.navigable_composition(tile.resource_terrain(), forage))
    } else {
        Cow::Borrowed(flora.composition(tile.resource_terrain()))
    }
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
) {
    let labor = labor_config.get();
    let forage = &labor.forage;
    for patch in registry.patches.values_mut() {
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
///   a completed patch/Field being worked *and* one being prepared under `FollowPolicy::Cultivate` /
///   `FollowPolicy::Sow` — so an investment accrues at the full `progress_per_turn` (25 turns at the
///   shipped default) instead of net-of-decay.
/// - An **untended** cultivated patch **goes feral**: `cultivation_progress` decays by
///   `decay_per_turn`, dropping below `1.0` so it reverts to a wild depletable gather patch, and keeps
///   decaying toward 0 over ~`1/decay_per_turn` turns (owner clears at 0 — the investment is fully
///   lost, and re-preparing must re-accrue from wherever progress landed).
/// - An **abandoned** part-prepared patch's partial accrual decays the same way (walk away mid-
///   investment and the cleared ground grows back over).
///
/// **One feral rule, both plant rungs.** An untended patch bleeds **both** improvement meters, each
/// at its own rung's `decay_per_turn` — so an abandoned **Field** reverts to a wild gather patch after
/// one untended turn, exactly as an abandoned tended patch does, and both meters lapse to zero over
/// ~100 turns (ownership clearing only once nothing is left of either). It does *not* step down to a
/// tended patch on the way: that would pay the deserter the rung-2 managed yield for free while the
/// rung-3 improvement lapsed, and the plant web has exactly one story here — *an improvement you stop
/// working goes back to the wild*.
///
/// **Stage ordering.** Logistics runs *before* Population, so the `tended_this_turn` flag this pass
/// reads was written by the labor arm **last** turn (a one-turn lag) — the flag is a deliberate
/// carry-across-turns signal, not a same-turn one. Each patch's flag is cleared here after it is read,
/// so the labor arm re-sets it next Population stage. Net effect: a patch worked every turn never
/// decays; a patch whose band leaves goes feral / reverts one turn later. The plant counterpart of
/// `fauna::advance_husbandry`'s decay side.
pub fn advance_cultivation(
    mut registry: ResMut<ForageRegistry>,
    ladder_config: Res<LadderConfigHandle>,
) {
    let ladder = ladder_config.get();
    // Each plant rung's own build decay — the shared ladder seam (`crate::intensification`), not a
    // plant-only lever. Two rungs, two rates, one pass: the ladder can be retuned per rung without
    // this system knowing anything about either number.
    let tended_decay = ladder
        .rung(RungKey::PlantTended)
        .build_decay(RUNG_TIMESCALE_UNSCALED);
    let field_decay = ladder
        .rung(RungKey::PlantField)
        .build_decay(RUNG_TIMESCALE_UNSCALED);
    for patch in registry.patches.values_mut() {
        // Spare any patch a band worked as an improvement this turn (working a completed
        // Field/patch, or preparing one under Cultivate/Sow). Everything else decays, on both rungs:
        // an untended Field or cultivated patch goes feral (reverts to wild once < 1.0), and an
        // abandoned part-prepared patch reverts toward 0.
        if !patch.tended_this_turn {
            patch.decay_field(field_decay);
            patch.decay_cultivation(tended_decay);
        }
        // Clear the transient per-turn flag after reading it (re-set next Population stage if worked).
        patch.tended_this_turn = false;
    }
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

/// The forage counterpart of `fauna::hunt_take`: resolve the per-policy ecology ceiling, cap it by
/// the gathering crew's throughput (`workers × per_worker_biomass_capacity × seasonal`), clamp to
/// the patch's remaining biomass, **subtract it from the patch**, and convert the take to provisions
/// (× the caller's productivity `output_multiplier`). Returns the provisions gathered.
///
/// Policy ceilings mirror `hunt_take` (§0-iii — forage parity with hunting): **Sustain** = the
/// Maximum Sustainable Yield (`sustainable_yield`: regrowth at the most-productive biomass K/2, so a
/// patch at carrying capacity still yields a positive skim and a collapsed patch yields nothing);
/// **Surplus** = that × `surplus_multiplier` (overdraws a healthy
/// patch → slow decline); **Market** = `market.take_fraction × biomass` (a commercial share → fast
/// depletion; the caller sells the take as trade goods); **Eradicate** = `eradicate.take_fraction ×
/// biomass` (strip the patch, no floor); **Cultivate** = the `plant:tended` rung's
/// `yield_fraction_while_building × MSY` — the
/// investment dip while the ground is being prepared. All are then throughput-capped and clamped to
/// biomass.
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

pub(crate) fn forage_take(
    patch: &mut ForagePatch,
    workers: u32,
    policy: FollowPolicy,
    forage: &ForageLaborConfig,
    ladder: &LadderConfig,
    output_multiplier: f32,
    seasonal: f32,
) -> Scalar {
    // Per-policy ecology ceiling + gather throughput, both from the shared helpers the pre-commit
    // forecast (`forage_forecast`) reads — the take and the forecast can never disagree. The ceiling
    // rides the patch's **own** curve (`patch_ecology`), so a tended patch is gathered on its boosted
    // MSY rather than the wild one — the whole of the rung-2 payoff, and the reason this one call
    // serves rungs 1 and 2 alike.
    let ecology = patch_ecology(patch, forage);
    let policy_ceiling = forage_policy_ceiling(
        policy,
        patch.biomass,
        patch.carrying_capacity,
        &ecology,
        forage,
        ladder,
    );
    let worker_cap = workers as f32 * forage_per_worker_biomass(forage, seasonal);
    let take = worker_cap
        .min(policy_ceiling)
        .max(0.0)
        .clamp(0.0, patch.biomass);
    patch.biomass -= take;
    // FOOD income is fully fractional (a few foragers may gather < 1 provision/turn).
    scalar_from_f32(forage_provisions(take, forage, output_multiplier))
}

/// The per-policy **biomass** ceiling on a gather at the patch's current stock — the single source of
/// the Sustain/Surplus/Market/Eradicate/**Cultivate** rungs, shared by `forage_take` (the take path)
/// and `forage_forecast` (the pre-commit forecast). Sustain = Maximum Sustainable Yield (regrowth at
/// K/2, so a full patch still yields and a collapsed one yields nothing), Surplus = that ×
/// `surplus_multiplier`, Market = `market.take_fraction × biomass`, Eradicate =
/// `eradicate.take_fraction × biomass`, **Cultivate** = the `plant:tended` rung's
/// `yield_fraction_while_building ×` the *same* `sustainable_yield` MSY ceiling (the preparing dip —
/// reusing the shared helper, never a second formula). Not yet clamped to biomass — callers do that
/// alongside their own throughput cap. The plant mirror of `fauna::hunt_policy_ceiling`.
///
/// `ecology` is **the patch's own** — resolved by [`patch_ecology`], never by the caller reaching for
/// `forage.ecology` directly. The tended rung is expressed *entirely* by handing this function a
/// different ecology (wild `r` = 0.25 / tended = wild × `tended_regrowth_gain`), exactly as the
/// husbandry ladder is expressed to `hunt_policy_ceiling`, so a call site that re-derives one silently
/// gathers a tended patch on the wild curve.
pub(crate) fn forage_policy_ceiling(
    policy: FollowPolicy,
    biomass: f32,
    carrying_capacity: f32,
    ecology: &EcologyConfig,
    forage: &ForageLaborConfig,
    ladder: &LadderConfig,
) -> f32 {
    match policy {
        FollowPolicy::Sustain => sustainable_yield(biomass, carrying_capacity, ecology),
        FollowPolicy::Surplus => {
            sustainable_yield(biomass, carrying_capacity, ecology) * forage.surplus_multiplier
        }
        FollowPolicy::Market => forage.market.take_fraction * biomass,
        FollowPolicy::Eradicate => forage.eradicate.take_fraction * biomass,
        // The two plant investment dips: a *fraction* of the MSY ceiling, so the preparing take is
        // sustainable and the patch stays healthy while the work goes on. Each read off its **own**
        // rung — the same seam the animal side's `Tame`/`Corral` dips read, so every rung's
        // investment cost is tuned in one file, and the two plant rungs' dips stay independently
        // tunable (the `ceiling_tame` lesson: never fold two rungs onto one number just because
        // today's levers happen to agree).
        FollowPolicy::Cultivate => {
            sustainable_yield(biomass, carrying_capacity, ecology)
                * ladder
                    .rung(RungKey::PlantTended)
                    .yield_fraction_while_building()
                    .expect("the tended rung is an investment — it has a build meter")
        }
        // On BARE ground this is a fraction of nothing — a freshly sown patch is below the Allee
        // threshold, so its MSY is `0` and the sow honestly pays ~0 while it builds (a pure
        // investment). On ground that already carries a stand it is the familiar dip.
        FollowPolicy::Sow => {
            sustainable_yield(biomass, carrying_capacity, ecology)
                * field_yield_fraction_while_building(ladder)
        }
        // `Tame`/`Corral` are animal-only policies — rejected on a Forage assignment at
        // `assign_labor` (`FollowPolicy::valid_for_forage`). Unreachable in practice; defensively
        // yield nothing rather than silently gathering under a nonsense policy.
        FollowPolicy::Tame | FollowPolicy::Corral => 0.0,
    }
}

/// Biomass one forager can gather this turn (`per_worker_biomass_capacity × seasonal_weight`) — the
/// per-worker throughput `forage_take`'s worker cap multiplies by the head-count, shared with the
/// forecast. Hunting has no seasonal factor, so it has no counterpart helper.
pub(crate) fn forage_per_worker_biomass(forage: &ForageLaborConfig, seasonal: f32) -> f32 {
    forage.per_worker_biomass_capacity * seasonal.max(0.0)
}

/// Biomass → provisions for a gather take (× the caller's productivity multiplier) — the one
/// conversion `forage_take` pays, shared with the forecast. The plant mirror of
/// `fauna::hunt_provisions`.
pub(crate) fn forage_provisions(
    biomass_take: f32,
    forage: &ForageLaborConfig,
    output_multiplier: f32,
) -> f32 {
    biomass_take * forage.provisions_per_biomass * output_multiplier
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
    biomass: f32,
    carrying_capacity: f32,
    forage: &ForageLaborConfig,
    output_multiplier: f32,
) -> f32 {
    forage_provisions(
        sustainable_yield(biomass, carrying_capacity, &tended_ecology(forage)).clamp(0.0, biomass),
        forage,
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
    biomass: f32,
    forage: &ForageLaborConfig,
    output_multiplier: f32,
) -> f32 {
    biomass * forage.cultivation.field_provisions_per_biomass * output_multiplier
}

/// **The `plant:field` rung's investment dip**, resolved off the ladder — the fraction of what a
/// patch would otherwise pay that it *does* pay while a crew sows a Field into it. One lookup, shared
/// by `forage_policy_ceiling` (via the rung), the managed-patch forecast and the managed-patch payout,
/// so a Sow on a tended patch can never be quoted one dip and paid another.
pub(crate) fn field_yield_fraction_while_building(ladder: &LadderConfig) -> f32 {
    ladder
        .rung(RungKey::PlantField)
        .yield_fraction_while_building()
        .expect("the field rung is an investment — it has a build meter")
}

/// `SourceYieldForecast::body_mass_yield` for a plant source (slice 8) — `0` = *do not quantise*.
///
/// **A deliberate asymmetry with the animal web, and a principled one — do not "fix" it.** A hunt take
/// is rounded down to whole animals because you cannot half-kill a deer; a gather is not, because you
/// harvest grain by the handful. The two food webs quantise differently because *their products
/// differ* — the same reason seed travels and a herd doesn't (`docs/plan_intensification_ladder.md`).
const PLANTS_DO_NOT_QUANTISE: f32 = 0.0;

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
///   it. That is the whole rung-2 fix: a tended patch's Sustain/Surplus/Market/Eradicate are four
///   different numbers again, and it can be over-farmed.
pub(crate) fn forage_forecast(
    patch: &ForagePatch,
    forage: &ForageLaborConfig,
    ladder: &LadderConfig,
    seasonal: f32,
    output_multiplier: f32,
) -> SourceYieldForecast {
    // A Field's harvest is biomass-based and **seasonless** — the crop is standing in the field you
    // built it to stand in — so its collection cap is too, and it must not read the gather season
    // (which is `NO_FORAGE_SEASON` on module-less ground a crew sowed: a Field there would forecast,
    // and be paid, exactly nothing).
    if patch.is_field() {
        return SourceYieldForecast::managed(
            field_provisions(patch.biomass, forage, output_multiplier),
            managed_per_worker_yield(forage, output_multiplier),
            // Plants never quantise — you harvest grain by the handful (slice 8; see
            // `SourceYieldForecast::body_mass_yield`). The whole-animal rule is animal-only because
            // *the products differ*, not by omission.
            PLANTS_DO_NOT_QUANTISE,
        );
    }
    let ecology = patch_ecology(patch, forage);
    let ceiling = |policy| {
        forage_provisions(
            forage_policy_ceiling(
                policy,
                patch.biomass,
                patch.carrying_capacity,
                &ecology,
                forage,
                ladder,
            )
            .clamp(0.0, patch.biomass),
            forage,
            output_multiplier,
        )
    };
    SourceYieldForecast {
        per_worker_yield: forage_provisions(
            forage_per_worker_biomass(forage, seasonal),
            forage,
            output_multiplier,
        ),
        body_mass_yield: PLANTS_DO_NOT_QUANTISE,
        ceiling_sustain: ceiling(FollowPolicy::Sustain),
        ceiling_surplus: ceiling(FollowPolicy::Surplus),
        ceiling_market: ceiling(FollowPolicy::Market),
        ceiling_eradicate: ceiling(FollowPolicy::Eradicate),
        // The investment rungs: what the patch pays *while preparing* (Cultivate at rung 2, Sow at
        // rung 3 — each its own field, since the two dips are independently tunable), and what it
        // will pay *once prepared* — so the client can show "preparing X → then Y" before committing.
        //
        // **Both stay honest on an ALREADY-TENDED patch**, which is the copy bug slice 7 fixed: this
        // branch used to be `SourceYieldForecast::tended(managed)`, whose every ceiling — the
        // "preparing" dip and the "then" payoff alike — was the one managed number, so a completed
        // rung-2 patch quoted "preparing 0.66 → then 0.66". Now `ceiling_prepare` is Cultivate's dip
        // on this patch's own (already boosted) curve, `ceiling_sow` is Sow's dip on it, and
        // `managed_yield` below is the rung-2 payoff — each computed, none copied.
        ceiling_prepare: ceiling(FollowPolicy::Cultivate),
        ceiling_sow: ceiling(FollowPolicy::Sow),
        // `Tame` is animal-only — a patch has no taming rung, and `forage_policy_ceiling` yields `0`
        // for it. Resolved through the same `ceiling` closure rather than a literal, so the "not a
        // forage policy" rule stays stated in exactly one place.
        ceiling_tame: ceiling(FollowPolicy::Tame),
        // **Cultivate's "then Y"** — what this patch will pay once tended, on the tended curve. On a
        // patch that is *already* tended this is simply its own `ceiling_sustain`, which is the truth:
        // the rung is built, and the number is what it pays. (Sow's "then Y" is `field_provisions`,
        // exported beside this one as the wire's `fieldYield` — two rungs, two payoff quotes, never
        // one field doing both jobs.)
        managed_yield: tended_provisions(
            patch.biomass,
            patch.carrying_capacity,
            forage,
            output_multiplier,
        ),
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
pub(crate) fn managed_per_worker_yield(forage: &ForageLaborConfig, output_multiplier: f32) -> f32 {
    forage_provisions(
        forage_per_worker_biomass(forage, MANAGED_HARVEST_SEASON),
        forage,
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
    forage: &ForageLaborConfig,
    ladder: &LadderConfig,
    seasonal: f32,
    output_multiplier: f32,
    workers: u32,
    policy: FollowPolicy,
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
            let production = field_provisions(sim.biomass, forage, output_multiplier);
            let collection = workers as f32 * managed_per_worker_yield(forage, output_multiplier);
            production.min(collection)
        } else {
            forage_take(
                &mut sim,
                workers,
                policy,
                forage,
                ladder,
                output_multiplier,
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
    forage: &ForageLaborConfig,
    ladder: &LadderConfig,
    seasonal: f32,
    output_multiplier: f32,
    workers: u32,
    policy: FollowPolicy,
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
            let production = field_provisions(sim.biomass, forage, output_multiplier);
            let collection = workers as f32 * managed_per_worker_yield(forage, output_multiplier);
            production.min(collection)
        } else {
            forage_take(
                &mut sim,
                workers,
                policy,
                forage,
                ladder,
                output_multiplier,
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
    forage: &ForageLaborConfig,
    ladder: &LadderConfig,
    seasonal: f32,
    output_multiplier: f32,
    workers: u32,
    policy: FollowPolicy,
    realized_horizon: u32,
    arrivals_horizon: u32,
) -> SourceYield {
    let forecast = forage_forecast(patch, forage, ladder, seasonal, output_multiplier);
    // The patch's OWN MSY (`patch_ecology`) — a tended patch's sustainable line sits on its boosted
    // curve, so a Sustain gather of it reads no ⚠ while a Surplus gather of it does. Reading
    // `forage.ecology` here would flag every tended Sustain as an overdraw.
    let sustainable = forage_provisions(
        sustainable_yield(
            patch.biomass,
            patch.carrying_capacity,
            &patch_ecology(patch, forage),
        ),
        forage,
        output_multiplier,
    );
    // The steady headline is the forward projection from THIS patch state — the same computation the
    // resolved Forage arm runs, so seed == first resolved value exactly.
    let realized = project_realized_forage(
        patch,
        forage,
        ladder,
        seasonal,
        output_multiplier,
        workers,
        policy,
        realized_horizon,
    );
    // The discrete twin, from the same patch state: what lands on each of the next
    // `arrivals_horizon` turns. A gather is continuous, so this is normally positive throughout.
    let arrivals = project_arrivals_forage(
        patch,
        forage,
        ladder,
        seasonal,
        output_multiplier,
        workers,
        policy,
        arrivals_horizon,
    );
    // **`managed` is rung 3 ONLY** (slice 7). It marks the sources whose harvest cannot overdraw —
    // and since rung 2 went back to being a drawn-down wild stand, a *tended* patch can be over-farmed
    // like any other, so it must keep its real sustainable line and its real ⚠.
    forecast_source_yield(
        &forecast,
        sustainable,
        patch.is_field(),
        workers,
        policy,
        realized,
        arrivals,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::labor_config::LaborConfig;
    use sim_runtime::TerrainType;
    use sim_schema::EcologyState;

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

        // First Sustain gather off the full patch: take equals MSY (positive), no longer 0.
        let biomass_before = patch.biomass;
        let expected_sustainable = sustainable_yield(biomass_before, cap, &forage.ecology);
        let provisions = forage_take(
            &mut patch,
            20,
            FollowPolicy::Sustain,
            &forage,
            &LadderConfig::builtin(),
            1.0,
            1.0,
        );
        let take = biomass_before - patch.biomass;
        assert!(
            take > 0.0,
            "a full patch under Sustain must yield > 0: {take}"
        );
        assert!((take - expected_sustainable).abs() < 1e-3);
        let actual = provisions.to_f32();
        let sustainable = expected_sustainable * forage.provisions_per_biomass;
        assert!(
            (actual - sustainable).abs() < 1e-3,
            "actual ≈ sustainable (no overdraw): {actual} vs {sustainable}"
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
                20,
                FollowPolicy::Sustain,
                &forage,
                &LadderConfig::builtin(),
                1.0,
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
                3,
                FollowPolicy::Eradicate,
                &forage,
                &LadderConfig::builtin(),
                1.0,
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

    /// The forage policy axis (§0-iii, parity with hunting): on an identical Thriving patch with
    /// ample workers (so the take is ceiling-bound, not throughput-bound), a heavier policy takes
    /// more — `Sustain ≤ Surplus < Market < Eradicate` — and the heavier policies deplete the patch
    /// faster (biomass drops more in a single turn).
    #[test]
    fn policy_ceilings_order_take_and_depletion() {
        let forage = test_forage_config();
        let cap = forage.capacity_for(TEST_BIOME);
        let start = 0.8 * cap; // Thriving, clear positive net regrowth.
        let workers = 20; // worker_cap (20 × per_worker) far exceeds every policy ceiling.

        // One-turn take under each policy from the same starting biomass.
        let take_under = |policy: FollowPolicy| -> (f32, f32) {
            let mut patch = ForagePatch::new(UVec2::new(1, 1), cap);
            patch.biomass = start;
            let provisions = forage_take(
                &mut patch,
                workers,
                policy,
                &forage,
                &LadderConfig::builtin(),
                1.0,
                1.0,
            );
            let take = start - patch.biomass;
            (take, provisions.to_f32())
        };

        let (sustain_take, _) = take_under(FollowPolicy::Sustain);
        let (surplus_take, _) = take_under(FollowPolicy::Surplus);
        let (market_take, _) = take_under(FollowPolicy::Market);
        let (eradicate_take, _) = take_under(FollowPolicy::Eradicate);

        // Sustain is the regrowth skim; Surplus overdraws it; Market/Eradicate strip a share.
        assert!(sustain_take <= surplus_take + 1e-4, "Sustain ≤ Surplus");
        assert!(surplus_take < market_take, "Surplus < Market");
        assert!(market_take < eradicate_take, "Market < Eradicate");
        // Heavier policies deplete the patch faster (more biomass removed this turn).
        assert!(
            market_take > sustain_take,
            "Market depletes faster than Sustain"
        );
        assert!(
            eradicate_take > sustain_take,
            "Eradicate depletes faster than Sustain"
        );
        // Sustain leaves the patch at/above where it started net of regrowth (no overdraw): the
        // take equals the net regrowth ceiling exactly.
        let expected_sustain = sustainable_yield(start, cap, &forage.ecology);
        assert!((sustain_take - expected_sustain).abs() < 1e-3);
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
                50,
                FollowPolicy::Eradicate,
                &forage,
                &LadderConfig::builtin(),
                1.0,
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
    fn forage_state_roundtrip_is_identity() {
        // A ForageState with non-default ecology AND **both** improvement meters, so biomass / cap /
        // phase / cultivation / field / owner all round-trip (a rollback must rewind a half-sown
        // Field, not lose the investment).
        let original = ForageState {
            x: 7,
            y: 3,
            field_progress: 0.4,
            ecology: EcologyState {
                biomass: 42.5,
                carrying_capacity: 120.0,
                ecology_phase: "stressed".to_string(),
                progress: 0.6,
                owner: Some(3),
            },
        };

        let registry = ForageRegistry::from_states(std::slice::from_ref(&original));
        let patch = registry
            .patch(UVec2::new(7, 3))
            .expect("one patch restored");
        assert_eq!(patch.cultivation_progress, 0.6);
        assert_eq!(patch.field_progress, 0.4);
        assert_eq!(patch.owner, Some(FactionId(3)));
        let restored = crate::snapshot::forage_state(patch);
        assert_eq!(restored, original);
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
