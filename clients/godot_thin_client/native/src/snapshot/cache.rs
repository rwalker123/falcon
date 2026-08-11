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
//! | Rasters (visibility, moisture, elevation, …) | whole-raster-or-absent | present → replace; absent → keep |
//! | Sections (herds, demographics, …) | whole-list-or-absent | present → replace; absent → keep |
//! | Keyed sections (tiles, populations, culture layers, …) | **sparse** — only the rows that changed, plus removed ids | patch the changed entries, then drop the removed |
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
//!
//! **And the same trap sits under every OTHER diff-carried section.** `populations`,
//! `culture_layers`, `influencers`, `power_nodes`, `generations`,
//! `discovery_progress` and the two great-discovery sections all ride the wire as sparse diffs
//! exactly like tiles do, and all were left standing at the baseline for the same reason. They are
//! all patched by the one [`SectionCache`] — see its docs for the staleness that taught us so, and
//! [`KEYED_SECTIONS`] for the roster.

use godot::prelude::*;
use std::collections::{HashMap, HashSet};

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

/// One entry's identity across frames: one integer field, or two for the sections keyed by a
/// `(faction, thing)` pair.
///
/// Two components and not an arbitrary vector because nothing on the wire needs three, and a `Vec`
/// key would allocate once per entry — 4,160 allocations every time the tile index is rebuilt, for
/// a generality no section uses.
type EntryId = (i64, Option<i64>);

/// A group of entry fields some consumer rebuilds something expensive from, reported under its own
/// name in the change manifest when any of them moves on any changed entry.
///
/// The point is to let a consumer skip work, so a group must be **narrow**: `tiles.rivers` exists
/// because 600 tiles moving their graze biomass must not force a splatmap rebuild.
pub(crate) struct WatchGroup {
    /// Manifest name, e.g. `tiles.rivers`.
    pub(crate) name: &'static str,
    pub(crate) fields: &'static [&'static str],
}

/// How one **keyed section** is identified, published and patched.
///
/// A keyed section is one the delta carries as a sparse diff — the rows that changed, plus a list
/// of removed ids — rather than whole. `sim_schema`'s `WorldDelta` builds all of these with
/// `diff_new` / `diff_removed` (`core_sim/src/snapshot/capture.rs`), which is what makes
/// merge-then-remove the correct semantics rather than a guess.
pub(crate) struct SectionSpec {
    /// The dict key the COMPLETE section rides under — the key consumers read, and the name the
    /// change manifest uses (`populations`, never `population_updates`).
    pub(crate) key: &'static str,
    /// The dict key the delta's sparse changed-row list rides under.
    pub(crate) updates_key: &'static str,
    /// The dict key the delta's removed-id list rides under, for the sections that have one.
    /// `None` where the wire carries no removals at all — those sections only ever grow or move.
    pub(crate) removed_key: Option<&'static str>,
    /// The entry field(s) identifying a row across frames. One or two; see [`EntryId`]. A section
    /// with a removal list must be identified by exactly one field, since a removed id is one
    /// integer on the wire.
    pub(crate) identity: &'static [&'static str],
    /// Extra per-entry comparisons, each reported under its own manifest name.
    pub(crate) watches: &'static [WatchGroup],
    /// Publish [`Self::updates_key`] even when the delta changed nothing in this section?
    ///
    /// Purely a **preservation of today's wire-visible behaviour**, not a design position: the
    /// delta codec emits most of these vectors unconditionally (empty when nothing moved) and two
    /// of them the decoder has always gated on non-emptiness, and several GDScript panels branch on
    /// `data.has("<section>_updates")`. Now that `changed_sections` carries the honest signal, the
    /// empty publishes could go — but that is a second contract change and belongs with the
    /// consumer work, not here.
    pub(crate) publish_empty_updates: bool,
}

/// The tile keys the terrain SPLATMAPS are built from. A changed tile that moves one of these
/// forces `MapView` to rebuild the river/terrain textures; one that only moves `graze_biomass` does
/// not.
const TILE_WATCHES: &[WatchGroup] = &[
    WatchGroup {
        name: "tiles.rivers",
        fields: &[
            "river_edges",
            "river_inflow",
            "river_channel",
            "underlying_terrain",
            "terrain",
        ],
    },
    WatchGroup {
        name: "tiles.culture_layer",
        fields: &["culture_layer"],
    },
];

/// No section but `tiles` reports a narrower concern than "this section moved".
const NO_WATCHES: &[WatchGroup] = &[];

pub(crate) const SECTION_TILES: SectionSpec = SectionSpec {
    key: "tiles",
    updates_key: "tile_updates",
    removed_key: Some("tile_removed"),
    identity: &["entity"],
    watches: TILE_WATCHES,
    publish_empty_updates: true,
};

pub(crate) const SECTION_POPULATIONS: SectionSpec = SectionSpec {
    key: "populations",
    updates_key: "population_updates",
    removed_key: Some("population_removed"),
    identity: &["entity"],
    watches: NO_WATCHES,
    publish_empty_updates: true,
};

pub(crate) const SECTION_CULTURE_LAYERS: SectionSpec = SectionSpec {
    key: "culture_layers",
    updates_key: "culture_layer_updates",
    removed_key: Some("culture_layer_removed"),
    identity: &["id"],
    watches: NO_WATCHES,
    publish_empty_updates: true,
};

pub(crate) const SECTION_INFLUENCERS: SectionSpec = SectionSpec {
    key: "influencers",
    updates_key: "influencer_updates",
    removed_key: Some("influencer_removed"),
    identity: &["id"],
    watches: NO_WATCHES,
    publish_empty_updates: true,
};

pub(crate) const SECTION_POWER_NODES: SectionSpec = SectionSpec {
    key: "power_nodes",
    updates_key: "power_updates",
    removed_key: Some("power_removed"),
    identity: &["entity"],
    watches: NO_WATCHES,
    publish_empty_updates: true,
};

pub(crate) const SECTION_GENERATIONS: SectionSpec = SectionSpec {
    key: "generations",
    updates_key: "generation_updates",
    removed_key: Some("generation_removed"),
    identity: &["id"],
    watches: NO_WATCHES,
    publish_empty_updates: true,
};

/// Keyed by `(faction, discovery)` — the same pair `capture.rs` indexes the diff by. No removal
/// list on the wire: progress rows are added and moved, never withdrawn.
pub(crate) const SECTION_DISCOVERY_PROGRESS: SectionSpec = SectionSpec {
    key: "discovery_progress",
    updates_key: "discovery_progress_updates",
    removed_key: None,
    identity: &["faction", "discovery"],
    watches: NO_WATCHES,
    publish_empty_updates: true,
};

/// Keyed by `(faction, id)`. `publish_empty_updates: false` preserves the decoder's long-standing
/// non-empty gate on this key, which `GreatDiscoveriesPanel` branches on.
pub(crate) const SECTION_GREAT_DISCOVERIES: SectionSpec = SectionSpec {
    key: "great_discoveries",
    updates_key: "great_discovery_updates",
    removed_key: None,
    identity: &["faction", "id"],
    watches: NO_WATCHES,
    publish_empty_updates: false,
};

/// Keyed by `(faction, discovery)`; same non-empty gate as [`SECTION_GREAT_DISCOVERIES`].
pub(crate) const SECTION_GREAT_DISCOVERY_PROGRESS: SectionSpec = SectionSpec {
    key: "great_discovery_progress",
    updates_key: "great_discovery_progress_updates",
    removed_key: None,
    identity: &["faction", "discovery"],
    watches: NO_WATCHES,
    publish_empty_updates: false,
};

/// Every keyed section, so a full snapshot can index them all without the decoder naming them a
/// second time. **A new diff-carried section is added HERE and at its one `merge_section` call
/// site — nowhere else.**
///
/// `WorldDelta` also diffs `knowledge_ledger`; it is absent from this list because the client
/// decoder never converts it, so there is no base key to keep honest.
pub(crate) const KEYED_SECTIONS: &[&SectionSpec] = &[
    &SECTION_TILES,
    &SECTION_POPULATIONS,
    &SECTION_CULTURE_LAYERS,
    &SECTION_INFLUENCERS,
    &SECTION_POWER_NODES,
    &SECTION_GENERATIONS,
    &SECTION_DISCOVERY_PROGRESS,
    &SECTION_GREAT_DISCOVERIES,
    &SECTION_GREAT_DISCOVERY_PROGRESS,
];

/// What one section's patch actually moved — the section half of the delta frame's change manifest.
///
/// Answered by [`SectionCache::patch`] rather than derived at the call site because only the patch
/// has both the old entry and the new one in hand; once the slot is written, the previous value is
/// gone.
#[derive(Default)]
pub(crate) struct SectionPatch {
    /// The delta carried at least one entry for this section (changed, added or removed). The
    /// SECTION changed — as distinct from the complete array being republished, which happens on
    /// every frame.
    pub(crate) changed: bool,
    /// [`WatchGroup`] names whose fields moved on at least one changed entry.
    pub(crate) moved_watches: Vec<&'static str>,
}

/// One section's complete array plus an identity → slot index over it.
///
/// **This is what makes a merged delta frame an honest complete world.** A delta carries only the
/// rows that changed; the decoder published them under the `*_updates` key and left the base key
/// standing, so every consumer reading the base key was frozen at the baseline snapshot for the
/// life of the world. Measured on `tiles`: `graze_biomass` summed over `tiles` was byte-identical
/// across nine turns while `tile_updates` carried 400–600 moved tiles per turn. It was nine
/// sections, not one — `Main`'s band alerts read `populations`, `MapView` reads `populations`,
/// `culture_layers` and `power_nodes`.
///
/// The index is what makes patching cheap enough to do every frame: without it, placing ~600
/// changed rows into a ~4,160-entry array is a linear scan each. With it the whole patch is one
/// shallow duplicate (pointer copies of the entry Variants — **never** a deep copy of an entry
/// dictionary) plus one slot write per changed row.
#[derive(Clone)]
pub(crate) struct SectionCache {
    spec: &'static SectionSpec,
    array: VarArray,
    by_id: HashMap<EntryId, usize>,
}

impl SectionCache {
    /// Index an existing complete array. Walks it once; an entry with no readable identity simply
    /// goes unindexed, so a later delta appends it rather than corrupting an unrelated slot.
    fn from_array(spec: &'static SectionSpec, array: VarArray) -> Self {
        let by_id = index_by_identity(spec, &array);
        Self { spec, array, by_id }
    }

    /// The array to publish under [`SectionSpec::key`].
    pub(crate) fn array(&self) -> &VarArray {
        &self.array
    }

    /// Merge `updates` in and drop `removed`, answering what that moved.
    ///
    /// `removed` is handled by rebuilding the index from scratch rather than by repairing the slots
    /// after it: removals are structurally rare (the tile grid is fixed; cohorts and links outlive
    /// a turn), and a shifted-index repair is the kind of cleverness that is wrong exactly once and
    /// then silently forever.
    fn patch(&mut self, updates: &VarArray, removed: &[i64]) -> SectionPatch {
        let mut patch = SectionPatch::default();
        if updates.is_empty() && removed.is_empty() {
            return patch;
        }
        // Shallow: the entries are Variant handles to the row dictionaries, so this copies
        // pointers, one per row, and shares the dictionaries themselves with the previous frame.
        // It must happen before any write — the array this cache holds is the one the PREVIOUS
        // frame published, and GDScript still has it.
        self.array = self.array.duplicate_shallow();

        for value in updates.iter_shared() {
            let Ok(entry) = value.try_to::<VarDictionary>() else {
                continue;
            };
            patch.changed = true;
            let Some(id) = entry_id(self.spec, &entry) else {
                // No identity to index by: append it so the section is not silently missing a row,
                // and treat it as new.
                self.array.push(&value);
                self.note_all_watches(&mut patch);
                continue;
            };
            match self.by_id.get(&id) {
                Some(&slot) => {
                    self.note_moved_watches(slot, &entry, &mut patch);
                    self.array.set(slot, &value);
                }
                None => {
                    // A row the baseline never carried: append and index it.
                    self.by_id.insert(id, self.array.len());
                    self.array.push(&value);
                    self.note_all_watches(&mut patch);
                }
            }
        }

        if !removed.is_empty() {
            let dropped: HashSet<i64> = removed.iter().copied().collect();
            let spec = self.spec;
            self.array = self
                .array
                .iter_shared()
                .filter(|value| {
                    value
                        .try_to::<VarDictionary>()
                        .ok()
                        .and_then(|entry| entry_id(spec, &entry))
                        .map(|(first, _)| !dropped.contains(&first))
                        .unwrap_or(true)
                })
                .collect::<VarArray>();
            self.by_id = index_by_identity(spec, &self.array);
            patch.changed = true;
            self.note_all_watches(&mut patch);
        }

        patch
    }

    /// Compare the entry about to be written against the one it replaces, and name every watch
    /// group that moved. Skips a group already named — the answer cannot change, and this is the
    /// one part of the patch whose cost is per-FIELD rather than per-row.
    fn note_moved_watches(&self, slot: usize, entry: &VarDictionary, patch: &mut SectionPatch) {
        if self.spec.watches.is_empty() || patch.moved_watches.len() == self.spec.watches.len() {
            return;
        }
        let Ok(previous) = self.array.at(slot).try_to::<VarDictionary>() else {
            // Nothing comparable in the slot, so nothing can be ruled unchanged.
            self.note_all_watches(patch);
            return;
        };
        for watch in self.spec.watches {
            if patch.moved_watches.contains(&watch.name) {
                continue;
            }
            if watch
                .fields
                .iter()
                .any(|field| previous.get(*field) != entry.get(*field))
            {
                patch.moved_watches.push(watch.name);
            }
        }
    }

    /// Name every watch group, for the changes that cannot be compared field-by-field: a new row,
    /// an unidentifiable one, or a removal.
    fn note_all_watches(&self, patch: &mut SectionPatch) {
        for watch in self.spec.watches {
            if !patch.moved_watches.contains(&watch.name) {
                patch.moved_watches.push(watch.name);
            }
        }
    }
}

/// Every keyed section's cache, indexed by [`SectionSpec::key`].
#[derive(Clone, Default)]
pub(crate) struct SectionCaches {
    by_key: HashMap<&'static str, SectionCache>,
}

impl SectionCaches {
    /// Index every registered section out of a full snapshot's dictionary — the only frame that
    /// carries all of them, and therefore the only place a baseline can be established. A section
    /// the snapshot did not carry starts empty and a later delta appends into it.
    pub(crate) fn from_snapshot(dict: &VarDictionary) -> Self {
        let mut by_key = HashMap::with_capacity(KEYED_SECTIONS.len());
        for spec in KEYED_SECTIONS {
            let array = dict
                .get(spec.key)
                .and_then(|value| value.try_to::<VarArray>().ok())
                .unwrap_or_default();
            by_key.insert(spec.key, SectionCache::from_array(spec, array));
        }
        Self { by_key }
    }

    /// Merge one section's changed rows and removals into its cached complete array.
    pub(crate) fn patch(
        &mut self,
        spec: &'static SectionSpec,
        updates: &VarArray,
        removed: &[i64],
    ) -> SectionPatch {
        self.by_key
            .entry(spec.key)
            .or_insert_with(|| SectionCache::from_array(spec, VarArray::new()))
            .patch(updates, removed)
    }

    /// The complete array to publish for `spec`, or `None` if no baseline ever established one.
    pub(crate) fn array(&self, spec: &'static SectionSpec) -> Option<&VarArray> {
        self.by_key.get(spec.key).map(SectionCache::array)
    }
}

/// Identity → slot for every entry that carries a readable one. Last writer wins on a duplicate,
/// matching what a later slot would mean if it were patched.
fn index_by_identity(spec: &'static SectionSpec, array: &VarArray) -> HashMap<EntryId, usize> {
    let mut by_id = HashMap::with_capacity(array.len());
    for (slot, value) in array.iter_shared().enumerate() {
        if let Some(id) = value
            .try_to::<VarDictionary>()
            .ok()
            .as_ref()
            .and_then(|entry| entry_id(spec, entry))
        {
            by_id.insert(id, slot);
        }
    }
    by_id
}

/// One entry's identity, or `None` if a field the spec names is missing or not an integer.
fn entry_id(spec: &'static SectionSpec, entry: &VarDictionary) -> Option<EntryId> {
    let mut fields = spec.identity.iter();
    let first = entry_field(entry, fields.next()?)?;
    let second = match fields.next() {
        Some(name) => Some(entry_field(entry, name)?),
        None => None,
    };
    Some((first, second))
}

fn entry_field(entry: &VarDictionary, name: &str) -> Option<i64> {
    entry.get(name)?.try_to::<i64>().ok()
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
    /// The complete array behind [`Self::dict`] for every keyed (diff-carried) section, each
    /// indexed by its identity field(s). Held apart from the dictionary so a delta can patch them
    /// without a lookup-and-cast per section per frame; the two always agree — whatever `patch`
    /// produces is inserted under the section's key on the same frame it is cached.
    pub(crate) sections: SectionCaches,
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
