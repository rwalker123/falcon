//! `SnapshotDecoder` -- the GDScript entry point that turns a FlatBuffers envelope
//! into the snapshot `Dictionary` the client renders from.

use godot::meta::AsArg;
use godot::prelude::*;
use shadow_scale_flatbuffers::shadow_scale::sim as fb;

use crate::dict::campaign::{
    campaign_profiles_to_array, command_events_to_array, pending_forks_to_array,
    stance_axes_to_array, victory_state_to_dict, voice_medium_to_array,
};
use crate::dict::connections::connections_to_array;
use crate::dict::culture::{
    axis_bias_to_dict, culture_layers_to_array, culture_tensions_to_array, influencers_to_array,
    sentiment_to_dict,
};
use crate::dict::economy::faction_inventory_to_array;
use crate::dict::governance::{
    corruption_to_dict, crisis_overlay_to_dict, crisis_telemetry_to_dict, power_metrics_to_dict,
    power_nodes_to_array,
};
use crate::dict::knowledge::{
    discovered_sites_to_array, discovery_progress_to_array, great_discovery_definitions_to_array,
    great_discovery_progress_states_to_array, great_discovery_states_to_array,
    great_discovery_telemetry_to_dict,
};
use crate::dict::map::tiles_to_array;
use crate::dict::population::{demographics_to_array, generations_to_array, populations_to_array};
use crate::dict::routes::{route_rungs_to_array, routes_to_array};
use crate::dict::subsistence::{
    characteristic_bands_to_array, craft_knowledge_to_array, food_modules_to_array,
    forage_patches_to_array, herds_to_array, intensification_knowledge_to_array, kits_to_array,
    ladder_knowledge_to_array, materials_to_array, recipes_to_array, sedentarization_to_array,
};
use crate::dict::{
    u16_vector_to_packed_int32, u32_vector_to_packed_int32, u64_vector_to_packed_int64,
};
use crate::snapshot::cache::{
    RasterCache, SectionCaches, SectionSpec, WorldCache, SECTION_CULTURE_LAYERS,
    SECTION_DISCOVERY_PROGRESS, SECTION_GENERATIONS, SECTION_GREAT_DISCOVERIES,
    SECTION_GREAT_DISCOVERY_PROGRESS, SECTION_INFLUENCERS, SECTION_POPULATIONS,
    SECTION_POWER_NODES, SECTION_TILES,
};
use crate::snapshot::delta::DeltaAggregator;
use crate::snapshot::snapshot_to_dict;

/// Wall-clock microseconds a decode that measured nothing reports — the value
/// [`SnapshotDecoder::get_last_decode_usec`] answers before any frame has been decoded.
const NO_DECODE_RECORDED_USEC: i64 = 0;

/// Dictionary key naming which of the two envelope payloads produced this frame, and its two
/// values. **This is the authoritative answer** — it is read straight off
/// `Envelope::payload_type`, the same discriminant the encoder wrote.
///
/// It replaces `Main._snapshot_is_delta`, which guessed from the presence of six delta-only keys
/// (`tile_updates`, `population_updates`, …). That guess holds today only by accident: the delta
/// codec emits an *empty* vector rather than omitting an untouched section, so `tile_updates` is
/// present on every delta including one that changed no tile. A codec that ever starts omitting
/// empty sections — which is exactly what a bandwidth-conscious delta encoder wants to do — would
/// silently reclassify deltas as full snapshots, and a full snapshot resets the command feed and
/// can trip the world-epoch reset. See `docs/plan_delta_streaming.md` §2.
const FRAME_KIND_KEY: &str = "frame_kind";
const FRAME_KIND_SNAPSHOT: &str = "snapshot";
const FRAME_KIND_DELTA: &str = "delta";

/// Publication sequence keys (`snapshot.fbs` `frameSeq` / `baseFrameSeq`). `frame_seq` rides both
/// frame kinds; `base_frame_seq` is meaningful only on a delta, where it names the frame this one
/// applies to.
const FRAME_SEQ_KEY: &str = "frame_seq";
const BASE_FRAME_SEQ_KEY: &str = "base_frame_seq";

/// Names the sections this frame CHANGED (`PackedStringArray`).
///
/// **Absent on a full snapshot, and absence means "everything changed".** That is the contract, and
/// both halves of it matter: a consumer that has never heard of the manifest keeps working
/// unchanged, and a full snapshot — the frame that rebuilds the world — can never be gated by it.
///
/// The manifest is built by a forcing function, not a hand-maintained list: [`DeltaFrame`] owns
/// both the dictionary and the name vector, and the only way to put a delta-carried key on the
/// frame is [`DeltaFrame::insert_changed`], which names it in the same call. The dangerous
/// direction here is an UNDER-complete manifest — a consumer skips a section that really did move
/// and goes silently stale, which is precisely the class of bug the `tiles` staleness was.
///
/// **A name is the dictionary KEY the frame carried the section under**, so a consumer looks up the
/// key it already reads — and for a keyed section that is the COMPLETE key (`populations`), never
/// the sparse `*_updates` twin, which is the same section published a second way. The exceptions
/// are the channels that have no key of their own: the raster/overlay channels the
/// `DeltaAggregator` re-derives from cache (so they are present on every merged frame and presence
/// cannot be the signal) are named `overlays.*`, and the tile-derived splatmap concerns are named
/// `tiles.rivers` / `tiles.culture_layer`.
///
/// **A name means "this MOVED", not "this was transmitted."** The delta codec emits most keyed
/// sections' vectors unconditionally — empty when nothing changed — so presence is no signal at
/// all; every keyed section is named from its diff being non-empty instead.
const CHANGED_SECTIONS_KEY: &str = "changed_sections";

/// Manifest names for the channels `snapshot_dict` re-derives on every frame, so their presence in
/// the merged dictionary says nothing about whether they moved. Pushed at the `apply_*` call site,
/// which is the only place that knows the delta carried the channel at all.
const SECTION_OVERLAY_TERRAIN: &str = "overlays.terrain";
const SECTION_OVERLAY_ELEVATION: &str = "overlays.elevation";
const SECTION_OVERLAY_MOISTURE: &str = "overlays.moisture";
const SECTION_OVERLAY_VISIBILITY: &str = "overlays.visibility";
const SECTION_OVERLAY_CULTURE: &str = "overlays.culture";
const SECTION_OVERLAY_SENTIMENT: &str = "overlays.sentiment";
const SECTION_OVERLAY_CORRUPTION: &str = "overlays.corruption";
const SECTION_OVERLAY_MILITARY: &str = "overlays.military";
const SECTION_OVERLAY_CRISIS: &str = "overlays.crisis";
const SECTION_CLIMATE_BANDS: &str = "climate_bands";

#[derive(Default, GodotClass)]
#[class(init, base=RefCounted)]
pub struct SnapshotDecoder {
    /// The world the next delta will be applied to: the last complete dictionary, the raster
    /// inputs behind it, and the publication identity that decides whether a delta may be merged
    /// at all. `None` until the first full snapshot — a delta arriving before one is dropped,
    /// because there is nothing for it to be a delta *of*.
    cache: Option<WorldCache>,
    /// Set when a delta was DROPPED because it could not be applied — its `baseFrameSeq` names a
    /// frame this client never applied, or it belongs to a different world. Read and cleared by
    /// `take_resync_needed`, which is what turns a dropped frame into a `resync` request.
    ///
    /// Deliberately NOT set for a malformed or headerless frame: that is corruption, and asking
    /// the server to resend the world does not fix it.
    resync_needed: bool,
    /// Microseconds the most recent `decode_snapshot`/`decode_delta` spent inside
    /// `snapshot_to_dict` — i.e. the whole FlatBuffers→`VarDictionary` conversion, which is the
    /// dominant term. Read from GDScript by `SnapshotLoader` for its `decode.native` phase, so the
    /// client's per-turn profile line can separate the conversion itself from the Variant
    /// marshalling the binding adds on top of it.
    #[init(val = NO_DECODE_RECORDED_USEC)]
    last_decode_usec: i64,
}

#[godot_api]
impl SnapshotDecoder {
    /// Decode one frame of either kind, applying a delta to the cached world.
    ///
    /// **An unapplicable delta answers an empty dictionary**, which reaches the loader as "no
    /// frame" and is skipped — the same contract a malformed snapshot already had. That is the
    /// whole point of the frame-sequence gate: merging a delta whose base the client never saw
    /// produces a world that is silently wrong rather than visibly broken, and silently wrong is
    /// the failure mode this arc is least able to detect (`docs/plan_delta_streaming.md` §3.3).
    /// The caller recovers by asking for a full snapshot.
    #[func]
    pub fn decode_snapshot(&mut self, data: PackedByteArray) -> VarDictionary {
        let started = std::time::Instant::now();
        let decoded = self.decode_frame(&data).unwrap_or_default();
        self.last_decode_usec = elapsed_usec(started);
        decoded
    }

    #[func]
    pub fn decode_delta(&mut self, data: PackedByteArray) -> VarDictionary {
        self.decode_snapshot(data)
    }

    /// Has a delta been dropped since this was last asked? Clears on read, so each dropped frame
    /// produces at most one request and the caller decides the retry cadence.
    #[func]
    pub fn take_resync_needed(&mut self) -> bool {
        std::mem::replace(&mut self.resync_needed, false)
    }

    /// True once a full snapshot has established a baseline. `SnapshotLoader` reads it to tell
    /// "dropped because we have no baseline yet" from "dropped because the frame was bad".
    #[func]
    pub fn has_baseline(&self) -> bool {
        self.cache.is_some()
    }

    /// `frameSeq` of the last frame merged, or 0 before any. The value a resync request quotes.
    #[func]
    pub fn get_applied_frame_seq(&self) -> i64 {
        self.cache.as_ref().map(|c| c.frame_seq as i64).unwrap_or(0)
    }

    /// Microseconds the LAST decode call took, or [`NO_DECODE_RECORDED_USEC`] before the first
    /// one. Per-call rather than accumulated: a poll decodes every queued frame, and the caller
    /// (`SnapshotLoader.poll_stream`) is the thing that knows how many that was.
    #[func]
    pub fn get_last_decode_usec(&self) -> i64 {
        self.last_decode_usec
    }
}

/// Elapsed microseconds since `started`, saturating rather than wrapping. A decode is milliseconds
/// long, so the clamp is unreachable in practice and exists only so the cast cannot be lossy.
fn elapsed_usec(started: std::time::Instant) -> i64 {
    i64::try_from(started.elapsed().as_micros()).unwrap_or(i64::MAX)
}

impl SnapshotDecoder {
    /// Decode one envelope, updating [`Self::cache`]. `None` means "no frame" — malformed,
    /// headerless, or a delta that cannot be applied to what we hold.
    fn decode_frame(&mut self, data: &PackedByteArray) -> Option<VarDictionary> {
        if data.is_empty() {
            return None;
        }
        let envelope = fb::root_as_envelope(data.as_slice()).ok()?;
        match envelope.payload_type() {
            fb::SnapshotPayload::snapshot => {
                let snapshot = envelope.payload_as_snapshot()?;
                // `?`, not `map`: `snapshot_to_dict` answers `None` for a headerless
                // snapshot (see its docs), and that `None` must reach the caller as "no frame"
                // rather than a `Some(...)` the loader would treat as a decoded world.
                let (mut dict, rasters) = snapshot_to_dict(snapshot)?;
                let header = snapshot.header()?;
                let _ = dict.insert(FRAME_KIND_KEY, FRAME_KIND_SNAPSHOT);
                // A full snapshot names no base — it is applicable against any client state — so
                // only `frame_seq` is published here.
                let _ = dict.insert(FRAME_SEQ_KEY, header.frameSeq() as i64);
                // The baseline the next delta patches. Indexed here, once, off the arrays this
                // frame just published — a full snapshot is the only frame that carries every
                // section whole, so it is the only place the indices can be established.
                let sections = SectionCaches::from_snapshot(&dict);
                self.cache = Some(WorldCache {
                    world_epoch: header.worldEpoch(),
                    frame_seq: header.frameSeq(),
                    dict: dict.duplicate_shallow(),
                    rasters,
                    sections,
                });
                Some(dict)
            }
            fb::SnapshotPayload::delta => {
                let delta = envelope.payload_as_delta()?;
                let header = delta.header()?;
                // No baseline, wrong world, or a base we never applied: drop the frame. Merging it
                // anyway is precisely how the client's world drifts from the server's without
                // anything failing.
                let Some(cache) = self.cache.as_ref() else {
                    // No baseline at all. The client has nothing to apply a delta to and must be
                    // sent a full world before anything can render.
                    self.resync_needed = true;
                    return None;
                };
                if !cache.accepts(header.worldEpoch(), header.baseFrameSeq()) {
                    self.resync_needed = true;
                    return None;
                }
                let (delta_dict, rasters, sections) = decode_delta_against(delta, cache)?;
                // Merge: the cached world, overwritten by every key this delta carried. Absent
                // keys keep their cached value, which is what "absent means unchanged" has always
                // meant on the wire — the difference is that there is now something for it to be
                // unchanged *from*.
                let mut merged = cache.dict.duplicate_shallow();
                for (key, value) in delta_dict.iter_shared() {
                    merged.set(&key, &value);
                }
                self.cache = Some(WorldCache {
                    world_epoch: header.worldEpoch(),
                    frame_seq: header.frameSeq(),
                    dict: merged.duplicate_shallow(),
                    rasters,
                    sections,
                });
                Some(merged)
            }
            _ => None,
        }
    }
}

/// One delta's dictionary, plus the names of the sections it changed.
///
/// **The pairing is the point.** The manifest ([`CHANGED_SECTIONS_KEY`]) is only trustworthy if it
/// cannot fall behind the code that fills the frame, and the way it falls behind is somebody adding
/// a `dict.insert("new_section", …)` and not touching a list somewhere else. So there is no dict to
/// insert into: [`Self::insert_changed`] is the only way to put a delta-carried section on the
/// frame, and it names the section in the same call.
struct DeltaFrame {
    dict: VarDictionary,
    changed: Vec<GString>,
}

impl DeltaFrame {
    /// Wrap the dictionary `DeltaAggregator` assembled, seeded with the names of the channels that
    /// changed *before* that dictionary existed — the overlay/raster channels, which the aggregator
    /// re-derives from cache and therefore publishes on every merged frame whether they moved or
    /// not. Their presence cannot be the signal, so the `apply_*` call sites collect the names.
    fn new(dict: VarDictionary, seed: &[&str]) -> Self {
        let mut frame = Self {
            dict,
            changed: Vec::with_capacity(seed.len()),
        };
        for name in seed {
            frame.name_changed(name);
        }
        frame
    }

    /// Insert a key this delta CARRIED, naming it in the manifest in the same call.
    fn insert_changed<V: AsArg<Variant>>(&mut self, key: &str, value: V) {
        let _ = self.dict.insert(key, value);
        self.name_changed(key);
    }

    /// Insert a key that rides EVERY delta frame and is therefore not a *change*: the frame's
    /// identity (`frame_kind`, `frame_seq`, `base_frame_seq`) and the patched `tiles` array, which
    /// is republished every frame so a merged delta stays indistinguishable from a full snapshot.
    fn insert_always<V: AsArg<Variant>>(&mut self, key: &str, value: V) {
        let _ = self.dict.insert(key, value);
    }

    /// Name a changed section that has no key of its own, or whose key is not the name a consumer
    /// reads (the `overlays.*` channels and the `tiles*` concerns). Deduped: several call sites can
    /// legitimately reach the same section.
    fn name_changed(&mut self, name: &str) {
        let name = GString::from(name);
        if !self.changed.contains(&name) {
            self.changed.push(name);
        }
    }

    /// The finished frame, with the manifest stamped on it.
    fn finish(mut self) -> VarDictionary {
        let manifest = PackedStringArray::from(self.changed.as_slice());
        let _ = self.dict.insert(CHANGED_SECTIONS_KEY, &manifest);
        self.dict
    }
}

/// A section's removed-id list, at the width its own wire field uses.
///
/// The widths genuinely differ (`removedTiles` is `u64`, `removedInfluencers` `u32`,
/// `removedGenerations` `u16`) and the client has always published each at its wire width, so this
/// keeps that while handing the identity index one `i64` type to work in.
enum RemovedIds {
    /// A `u16`/`u32` list, published as a `PackedInt32Array`.
    Narrow(PackedInt32Array),
    /// A `u64` list, published as a `PackedInt64Array`.
    Wide(PackedInt64Array),
    /// The section carries no removal field at all.
    None,
}

impl RemovedIds {
    fn narrow(ids: PackedInt32Array) -> Self {
        Self::Narrow(ids)
    }

    fn wide(ids: Option<flatbuffers::Vector<'_, u64>>) -> Self {
        Self::Wide(u64_vector_to_packed_int64(ids))
    }

    fn none() -> Self {
        Self::None
    }

    fn is_empty(&self) -> bool {
        match self {
            Self::Narrow(ids) => ids.is_empty(),
            Self::Wide(ids) => ids.is_empty(),
            Self::None => true,
        }
    }

    /// The ids as `i64`, for the identity index. Empty for a section with no removal field.
    fn to_i64(&self) -> Vec<i64> {
        match self {
            Self::Narrow(ids) => ids.as_slice().iter().map(|id| *id as i64).collect(),
            Self::Wide(ids) => ids.as_slice().to_vec(),
            Self::None => Vec::new(),
        }
    }
}

/// Merge one keyed section's sparse changed rows into the cached COMPLETE array, publish every key
/// the section rides under, and name it in the manifest if it actually moved.
///
/// `updates` is `None` when the delta carried no such section at all — distinct from an empty
/// list, though both merge to nothing.
fn merge_section(
    frame: &mut DeltaFrame,
    sections: &mut SectionCaches,
    spec: &'static SectionSpec,
    updates: Option<VarArray>,
    removed: RemovedIds,
) {
    let updates = updates.unwrap_or_default();
    let patch = sections.patch(spec, &updates, &removed.to_i64());
    if let Some(array) = sections.array(spec) {
        // The COMPLETE section rides EVERY delta frame, patched. That is what makes a merged delta
        // indistinguishable from a full snapshot of the same state, and it is never a "change" in
        // the manifest's sense — the name below comes from the diff, not from this insert.
        frame.insert_always(spec.key, array);
    }
    if patch.changed {
        frame.name_changed(spec.key);
    }
    for name in patch.moved_watches {
        frame.name_changed(name);
    }
    if spec.publish_empty_updates || !updates.is_empty() {
        // The sparse list itself, published exactly as it was before the manifest existed —
        // `TerrainPanel` and the inspector panels branch on `data.has("<section>_updates")`. NOT
        // named: it is the same section as `spec.key`, which the manifest already carries under the
        // key a consumer reads the world by.
        frame.insert_always(spec.updates_key, &updates);
    }
    if let Some(removed_key) = spec.removed_key {
        if !removed.is_empty() {
            match &removed {
                RemovedIds::Narrow(ids) => frame.insert_always(removed_key, ids),
                RemovedIds::Wide(ids) => frame.insert_always(removed_key, ids),
                RemovedIds::None => {}
            }
        }
    }
}

/// Build the dictionary of everything one delta carried, seeded from `cache` so the channels it
/// omits re-derive from the previous frame instead of zeroing, and the tiles it *did* carry are
/// patched into the previous frame's complete tile array.
///
/// Answers the frame, the raster state behind it, and the patched tile cache — the last two become
/// the next delta's baseline.
fn decode_delta_against(
    delta: fb::WorldDelta<'_>,
    cache: &WorldCache,
) -> Option<(VarDictionary, RasterCache, SectionCaches)> {
    let mut agg = DeltaAggregator::from_cache(&cache.rasters);
    // Cheap: `VarArray`'s `Clone` is a reference copy, so this shares the previous frame's arrays
    // and `SectionCache::patch` duplicates before it writes. Only the identity indices are copied.
    let mut sections = cache.sections.clone();
    // Names for the channels the aggregator re-derives on every frame; see `DeltaFrame::new`.
    let mut changed_channels: Vec<&str> = Vec::new();
    let mut frame_seq: u64 = 0;
    let mut base_frame_seq: u64 = 0;
    if let Some(header) = delta.header() {
        agg.tick = header.tick();
        agg.wrap_horizontal = header.wrapHorizontal();
        agg.world_epoch = header.worldEpoch();
        frame_seq = header.frameSeq();
        base_frame_seq = header.baseFrameSeq();
        if let Some(build) = header.serverBuild() {
            agg.server_build = build.to_string();
        }
    }
    if let Some(tiles) = delta.map().and_then(|s| s.tiles()) {
        for tile in tiles {
            agg.update_tile(
                tile.x(),
                tile.y(),
                tile.grazeCapacity(),
                tile.forageCapacity(),
            );
        }
    }
    if let Some(layer) = delta.map().and_then(|s| s.terrainOverlay()) {
        agg.apply_terrain_overlay(layer);
        changed_channels.push(SECTION_OVERLAY_TERRAIN);
    }
    if let Some(raster) = delta.culture().and_then(|s| s.sentimentRaster()) {
        agg.apply_sentiment_raster(raster);
        changed_channels.push(SECTION_OVERLAY_SENTIMENT);
    }
    if let Some(raster) = delta.governance().and_then(|s| s.corruptionRaster()) {
        agg.apply_corruption_raster(raster);
        changed_channels.push(SECTION_OVERLAY_CORRUPTION);
    }
    if let Some(raster) = delta.vision().and_then(|s| s.visibilityRaster()) {
        agg.apply_visibility_raster(raster);
        changed_channels.push(SECTION_OVERLAY_VISIBILITY);
    }
    if let Some(vision) = delta.vision() {
        // Deliberately NOT named: `fogEnabled` is carried on every delta rather than diffed (see
        // `WorldDelta::fog_enabled`), so it says nothing about what changed.
        agg.apply_fog_enabled(vision.fogEnabled());
    }
    if let Some(raster) = delta.culture().and_then(|s| s.cultureRaster()) {
        agg.apply_culture_raster(raster);
        changed_channels.push(SECTION_OVERLAY_CULTURE);
    }
    if let Some(raster) = delta.vision().and_then(|s| s.militaryRaster()) {
        agg.apply_military_raster(raster);
        changed_channels.push(SECTION_OVERLAY_MILITARY);
    }
    if let Some(overlay) = delta.governance().and_then(|s| s.crisisOverlay()) {
        agg.apply_crisis_overlay(overlay);
        changed_channels.push(SECTION_OVERLAY_CRISIS);
    }
    if let Some(overlay) = delta.map().and_then(|s| s.elevationOverlay()) {
        agg.apply_elevation_overlay(overlay);
        changed_channels.push(SECTION_OVERLAY_ELEVATION);
    }
    if let Some(bands) = delta.map().and_then(|s| s.climateBands()) {
        agg.apply_climate_bands(bands);
        changed_channels.push(SECTION_CLIMATE_BANDS);
    }
    if let Some(raster) = delta.map().and_then(|s| s.moistureRaster()) {
        agg.apply_moisture_raster(raster);
        changed_channels.push(SECTION_OVERLAY_MOISTURE);
    }
    let rasters = agg.raster_cache();
    let mut frame = DeltaFrame::new(agg.into_dictionary(), &changed_channels);
    frame.insert_always(FRAME_KIND_KEY, FRAME_KIND_DELTA);
    frame.insert_always(FRAME_SEQ_KEY, frame_seq as i64);
    frame.insert_always(BASE_FRAME_SEQ_KEY, base_frame_seq as i64);

    // ---- the keyed (diff-carried) sections -------------------------------------------------
    // Each one merges the delta's sparse changed rows into the cached COMPLETE array and publishes
    // that array under the key consumers read, so a merged delta is an honest whole world. One line
    // each: the machinery lives in `SectionCaches`, the roster in `KEYED_SECTIONS`.
    merge_section(
        &mut frame,
        &mut sections,
        &SECTION_TILES,
        delta.map().and_then(|s| s.tiles()).map(tiles_to_array),
        RemovedIds::wide(delta.map().and_then(|s| s.removedTiles())),
    );
    merge_section(
        &mut frame,
        &mut sections,
        &SECTION_POPULATIONS,
        delta
            .population()
            .and_then(|s| s.populations())
            .map(populations_to_array),
        RemovedIds::wide(delta.population().and_then(|s| s.removedPopulations())),
    );
    merge_section(
        &mut frame,
        &mut sections,
        &SECTION_CULTURE_LAYERS,
        delta
            .culture()
            .and_then(|s| s.cultureLayers())
            .map(culture_layers_to_array),
        RemovedIds::narrow(u32_vector_to_packed_int32(
            delta.culture().and_then(|s| s.removedCultureLayers()),
        )),
    );
    merge_section(
        &mut frame,
        &mut sections,
        &SECTION_INFLUENCERS,
        delta
            .culture()
            .and_then(|s| s.influencers())
            .map(influencers_to_array),
        RemovedIds::narrow(u32_vector_to_packed_int32(
            delta.culture().and_then(|s| s.removedInfluencers()),
        )),
    );
    merge_section(
        &mut frame,
        &mut sections,
        &SECTION_POWER_NODES,
        delta
            .governance()
            .and_then(|s| s.power())
            .map(power_nodes_to_array),
        RemovedIds::wide(delta.governance().and_then(|s| s.removedPower())),
    );
    merge_section(
        &mut frame,
        &mut sections,
        &SECTION_GENERATIONS,
        delta
            .population()
            .and_then(|s| s.generations())
            .map(generations_to_array),
        RemovedIds::narrow(u16_vector_to_packed_int32(
            delta.population().and_then(|s| s.removedGenerations()),
        )),
    );
    merge_section(
        &mut frame,
        &mut sections,
        &SECTION_DISCOVERY_PROGRESS,
        delta
            .knowledge()
            .and_then(|s| s.discoveryProgress())
            .map(discovery_progress_to_array),
        RemovedIds::none(),
    );
    merge_section(
        &mut frame,
        &mut sections,
        &SECTION_GREAT_DISCOVERIES,
        delta
            .knowledge()
            .and_then(|s| s.greatDiscoveries())
            .map(great_discovery_states_to_array),
        RemovedIds::none(),
    );
    merge_section(
        &mut frame,
        &mut sections,
        &SECTION_GREAT_DISCOVERY_PROGRESS,
        delta
            .knowledge()
            .and_then(|s| s.greatDiscoveryProgress())
            .map(great_discovery_progress_states_to_array),
        RemovedIds::none(),
    );

    if let Some(profiles) = delta.campaign().and_then(|s| s.campaignProfiles()) {
        frame.insert_changed("campaign_profiles", &campaign_profiles_to_array(profiles));
    }

    if let Some(victory) = delta.campaign().and_then(|s| s.victory()) {
        frame.insert_changed("victory", &victory_state_to_dict(victory));
    }

    if let Some(events) = delta.campaign().and_then(|s| s.commandEvents()) {
        frame.insert_changed("command_events", &command_events_to_array(events));
    }

    // The retention window is hot-reloadable, so a delta may carry a NEW value; `0` is the
    // FlatBuffers default meaning "not stated" and is withheld. `insert_always`, not
    // `insert_changed`: it rides the campaign section rather than being a section of its own, and
    // naming it in the manifest would invite a consumer to gate on a section name nothing else uses.
    let retention_turns = delta
        .campaign()
        .map(|s| s.commandEventsRetentionTurns())
        .unwrap_or(0);
    if retention_turns > 0 {
        frame.insert_always("command_events_retention_turns", retention_turns as i64);
    }

    if let Some(pending_forks) = delta.campaign().and_then(|s| s.pendingForks()) {
        frame.insert_changed("pending_forks", &pending_forks_to_array(pending_forks));
    }

    if let Some(stance_axes) = delta.campaign().and_then(|s| s.stanceAxes()) {
        frame.insert_changed("stance_axes", &stance_axes_to_array(stance_axes));
    }

    if let Some(voice_medium) = delta.campaign().and_then(|s| s.voiceMedium()) {
        frame.insert_changed("voice_medium", &voice_medium_to_array(voice_medium));
    }

    if let Some(herds) = delta.subsistence().and_then(|s| s.herds()) {
        frame.insert_changed("herds", &herds_to_array(herds));
    }

    if let Some(sedentarization) = delta.subsistence().and_then(|s| s.sedentarization()) {
        frame.insert_changed(
            "sedentarization",
            &sedentarization_to_array(sedentarization),
        );
    }

    if let Some(forage_patches) = delta.subsistence().and_then(|s| s.foragePatches()) {
        frame.insert_changed("forage_patches", &forage_patches_to_array(forage_patches));
    }

    // The KIT ROSTER and the two job defaults — whole-section fields, decoded here as well as in
    // `snapshot_to_dict` for the reason the block below records: a whole-section field read only on
    // the full path republishes the BASELINE's value for the life of the world.
    if let Some(kits) = delta.subsistence().and_then(|s| s.kits()) {
        frame.insert_changed("kits", &kits_to_array(kits));
    }
    if let Some(default_hunt_kit) = delta.subsistence().and_then(|s| s.defaultHuntKitId()) {
        frame.insert_changed("default_hunt_kit_id", default_hunt_kit);
    }
    if let Some(default_forage_kit) = delta.subsistence().and_then(|s| s.defaultForageKitId()) {
        frame.insert_changed("default_forage_kit_id", default_forage_kit);
    }
    if let Some(default_scout_kit) = delta.subsistence().and_then(|s| s.defaultScoutKitId()) {
        frame.insert_changed("default_scout_kit_id", default_scout_kit);
    }
    if let Some(default_warrior_kit) = delta.subsistence().and_then(|s| s.defaultWarriorKitId()) {
        frame.insert_changed("default_warrior_kit_id", default_warrior_kit);
    }
    // The whole effective `EquipmentConfig` as one `serde_json` string — the Workbench's two config
    // pages parse it themselves. `insert_changed`, like its siblings: the sim diffs it as a
    // `Whole<String>`, so it rides a delta ONLY when it moved and presence here IS the change
    // signal, even though it is not one on the merged frame the client finally reads.
    if let Some(equipment_config) = delta.subsistence().and_then(|s| s.equipmentConfigJson()) {
        frame.insert_changed("equipment_config_json", equipment_config);
    }

    // The CRAFTING CATALOGUES, decoded here as well as in `snapshot_to_dict` for the reason the kit
    // roster is: a whole-section field read only on the full path republishes the BASELINE's value
    // for the life of the world. `craft_knowledge` is the one of the four that genuinely moves in
    // play (a craft is learned), so it is also the one whose staleness would be visible.
    if let Some(materials) = delta.subsistence().and_then(|s| s.materials()) {
        frame.insert_changed("materials", &materials_to_array(materials));
    }
    if let Some(bands) = delta.subsistence().and_then(|s| s.characteristicBands()) {
        frame.insert_changed(
            "characteristic_bands",
            &characteristic_bands_to_array(bands),
        );
    }
    if let Some(recipes) = delta.subsistence().and_then(|s| s.recipes()) {
        frame.insert_changed("recipes", &recipes_to_array(recipes));
    }
    if let Some(knowledge) = delta.subsistence().and_then(|s| s.craftKnowledge()) {
        frame.insert_changed("craft_knowledge", &craft_knowledge_to_array(knowledge));
    }

    // These two were decoded on the full-snapshot path only — `decode_delta_against` passed `None`
    // for both — so a merged delta republished the BASELINE's food modules and stockpiles for the
    // life of the world, however many the server had since sent. Same staleness as `tiles`, reached
    // a different way: not a diff left unpatched, but a whole-section field never read at all. The
    // wire has always carried them (`WorldDelta::food_modules` / `faction_inventory`, both
    // `Option`, absent = unchanged).
    if let Some(food_modules) = delta.subsistence().and_then(|s| s.foodModules()) {
        frame.insert_changed("food_modules", &food_modules_to_array(food_modules));
    }

    if let Some(inventory) = delta.economy().and_then(|s| s.factionInventory()) {
        frame.insert_changed("faction_inventory", &faction_inventory_to_array(inventory));
    }

    if let Some(intensification) = delta
        .subsistence()
        .and_then(|s| s.intensificationKnowledge())
    {
        frame.insert_changed(
            "intensification_knowledge",
            &intensification_knowledge_to_array(intensification),
        );
    }

    if let Some(roster) = delta.subsistence().and_then(|s| s.ladderKnowledge()) {
        frame.insert_changed("ladder_knowledge", &ladder_knowledge_to_array(roster));
    }

    // ...and the ROUTE branch's rung catalog on the DELTA path too. A per-world constant read only
    // on the full path republishes the BASELINE's value for the life of the world -- the staleness
    // the `food_modules` / `faction_inventory` pair recorded, one section over.
    if let Some(catalog) = delta.subsistence().and_then(|s| s.routeRungs()) {
        frame.insert_changed("route_rungs", &route_rungs_to_array(catalog));
    }

    if let Some(demographics) = delta.population().and_then(|s| s.demographics()) {
        frame.insert_changed("demographics", &demographics_to_array(demographics));
    }

    if let Some(discovered_sites) = delta.knowledge().and_then(|s| s.discoveredSites()) {
        frame.insert_changed(
            "discovered_sites",
            &discovered_sites_to_array(discovered_sites),
        );
    }

    if let Some(definitions) = delta
        .knowledge()
        .and_then(|s| s.greatDiscoveryDefinitions())
    {
        frame.insert_changed(
            "great_discovery_definitions",
            &great_discovery_definitions_to_array(definitions),
        );
    }

    if let Some(axis_bias) = delta.culture().and_then(|s| s.axisBias()) {
        frame.insert_changed("axis_bias", &axis_bias_to_dict(axis_bias));
    }

    if let Some(sentiment) = delta.culture().and_then(|s| s.sentiment()) {
        frame.insert_changed("sentiment", &sentiment_to_dict(sentiment));
    }

    if let Some(crisis) = delta.governance().and_then(|s| s.crisisTelemetry()) {
        frame.insert_changed("crisis_telemetry", &crisis_telemetry_to_dict(crisis));
    }

    if let Some(crisis_overlay) = delta.governance().and_then(|s| s.crisisOverlay()) {
        frame.insert_changed("crisis_overlay", &crisis_overlay_to_dict(crisis_overlay));
    }

    if let Some(gd_telemetry) = delta.knowledge().and_then(|s| s.greatDiscoveryTelemetry()) {
        frame.insert_changed(
            "great_discovery_telemetry",
            &great_discovery_telemetry_to_dict(gd_telemetry),
        );
    }

    if let Some(ledger) = delta.governance().and_then(|s| s.corruption()) {
        frame.insert_changed("corruption", &corruption_to_dict(ledger));
    }

    if let Some(power_metrics) = delta.governance().and_then(|s| s.powerMetrics()) {
        frame.insert_changed("power_metrics", &power_metrics_to_dict(power_metrics));
    }

    // **THE CONTACT TIES** (arc #527) — the same whole-section replace as `culture_tensions` below,
    // and `insert_changed` for the same reason: the sim diffs the section as a `Whole<Vec<_>>`, so
    // presence on a delta IS the change signal. Present-and-EMPTY means "you hold no ties now",
    // which a `!is_empty()` gate here would swallow — the defect that blanked the culture tensions.
    if let Some(connections) = delta.connections().and_then(|s| s.connections()) {
        frame.insert_changed("connections", &connections_to_array(connections));
    }

    // **THE ROADS IN THE GROUND** (arc #532) — the same whole-section replace as the ties above,
    // and `insert_changed` for the same reason: the sim diffs the section whole, so presence on a
    // delta IS the change signal and present-and-EMPTY means "every road you knew of is gone".
    // `RouteSection` rides the delta precisely so this twin can exist — a section with no delta
    // twin is permanently stale on a delta-fed client.
    if let Some(routes) = delta.routes().and_then(|s| s.routes()) {
        frame.insert_changed("routes", &routes_to_array(routes));
    }

    // NOT a keyed diff — a whole-section replace, so present (even EMPTY) means "this is the roster
    // now" and absent means unchanged. That distinction only became expressible when the field went
    // `Option` on the wire: while it was a bare `Vec` the codec emitted an empty vector for BOTH
    // cases, and inserting it unconditionally blanked the baseline's tensions on the first delta
    // (`CulturePanel` then showed none until the next full snapshot). **Do not re-add an emptiness
    // gate here** — against an `Option` field it would swallow a genuinely-emptied roster.
    if let Some(tensions) = delta.culture().and_then(|s| s.cultureTensions()) {
        frame.insert_changed("culture_tensions", &culture_tensions_to_array(tensions));
    }

    Some((frame.finish(), rasters, sections))
}
