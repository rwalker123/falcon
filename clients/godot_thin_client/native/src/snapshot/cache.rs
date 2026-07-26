//! The cached world a delta is applied *to*.
//!
//! A delta is only meaningful against something. Until delta streaming the client had nothing to
//! apply one to — `DeltaAggregator` synthesized a full-snapshot-shaped dictionary from whatever
//! the delta happened to carry and **zero-filled the rest**, which is why the live stream had to
//! be full snapshots (`docs/plan_delta_streaming.md` §2.1). This module is the missing half.
//!
//! ## What has to be cached, and why each kind is cached differently
//!
//! | Kind | On the wire | Merge rule |
//! |---|---|---|
//! | Rasters (logistics, visibility, moisture, …) | whole-raster-or-absent | present → replace; absent → keep |
//! | Sections (herds, demographics, …) | whole-list-or-absent | present → replace; absent → keep |
//! | Tiles | **sparse** — only the tiles that changed | patch the changed entries |
//!
//! The first two need no sample-level merging at all: `WorldDelta`'s raster fields are
//! `Option<ScalarRasterState>`, so a raster that appears is *complete*. That is what makes
//! replace-or-keep correct rather than merely convenient.
//!
//! **Tiles are the exception, and the tile-DERIVED overlay channels are the trap.**
//! `pasture_capacity` and `forage_capacity` are assembled per-tile from `TileState.grazeCapacity` /
//! `forageCapacity` rather than sent as rasters (graze rides the tile so an ungrazed turn costs no
//! delta bytes). A sparse tile list cannot rebuild either field, so they are cached whole and
//! patched at the tiles the delta carried. Rebuilding them from the delta alone would publish a
//! field of zeros — a world that claims to have no pasture and no gathering sites anywhere.

use godot::prelude::*;

/// The raster-shaped inputs `snapshot_dict` derives the client's overlay channels from, kept so a
/// delta that omits a channel can re-derive it from the last full frame instead of zeroing it.
///
/// These are the **pre-normalization** values. Normalization is per-frame and map-relative
/// (`normalize_overlay`, and pasture/forage against the map max), so caching the normalized output
/// would rebase every later frame onto a stale range.
#[derive(Clone, Default)]
pub(crate) struct RasterCache {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) wrap_horizontal: bool,
    pub(crate) logistics: Vec<f32>,
    pub(crate) sentiment: Vec<f32>,
    pub(crate) corruption: Vec<f32>,
    pub(crate) culture: Vec<f32>,
    pub(crate) military: Vec<f32>,
    pub(crate) crisis: Vec<f32>,
    pub(crate) elevation: Vec<f32>,
    pub(crate) elevation_sea_level: f32,
    pub(crate) climate_bands: Option<[f32; 3]>,
    pub(crate) moisture: Vec<f32>,
    pub(crate) visibility: Vec<f32>,
    pub(crate) fog_enabled: bool,
    /// Tile-derived; see the module docs — patched per changed tile, never rebuilt from a delta.
    pub(crate) pasture_capacity: Vec<f32>,
    /// Tile-derived; see the module docs.
    pub(crate) forage_capacity: Vec<f32>,
    pub(crate) terrain: Vec<u16>,
    pub(crate) terrain_tags: Vec<u16>,
}

/// Everything the decoder remembers between frames: the last complete client dictionary, the
/// raster inputs behind it, and the publication identity that says whether an incoming delta may
/// be applied to it at all.
pub(crate) struct WorldCache {
    /// World this cache describes. A delta from a different world must never be merged into it —
    /// a full snapshot restates only what the new world *has*, never what it lacks.
    pub(crate) world_epoch: u32,
    /// `frameSeq` of the last frame merged. A delta is applicable iff its `baseFrameSeq` equals it.
    pub(crate) frame_seq: u64,
    /// The last complete client dictionary. A delta overwrites the keys it carries and leaves the
    /// rest standing, which is what makes the merged frame indistinguishable from a full snapshot
    /// of the same state — the property the convergence guard pins.
    pub(crate) dict: VarDictionary,
    pub(crate) rasters: RasterCache,
}

impl WorldCache {
    /// May `delta_base_seq`, from a delta in `delta_epoch`, be applied to this cache?
    ///
    /// Both halves are load-bearing. The epoch check stops a delta from a rebuilt world being
    /// merged into the previous one's dictionary; the sequence check stops a delta whose base the
    /// client never saw from being applied to the wrong state, which is the failure that produces
    /// a silently-wrong world rather than a visibly broken one.
    pub(crate) fn accepts(&self, delta_epoch: u32, delta_base_seq: u64) -> bool {
        self.world_epoch == delta_epoch && self.frame_seq == delta_base_seq
    }
}
