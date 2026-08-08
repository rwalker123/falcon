---
paths:
  - "clients/godot_thin_client/native/src/**"
  - "clients/godot_thin_client/native/Cargo.toml"
---

<!-- Extracted verbatim from lines 243-336 of clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e
     (the PRE-SPLIT original — read it with `git cat-file blob 20553fb8f9b193b80338a8c06765d511b81b601e`;
     clients/godot_thin_client/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Native extension — the GDExtension module map

## Native Extension
`native/` contains GDExtension bindings for FlatBuffers decoding (generated from `sim_schema/schemas/snapshot.fbs`).

### Module map (`native/src/`)
The decoder was one 5,617-line `lib.rs`; it is now split along **the same nine domain
sections `snapshot.fbs` uses**, mirroring the `sim_schema/src/{state,codec}` split on the
server side, so the two ends of the wire have the same shape.

| Module | Holds |
|--------|-------|
| `lib.rs` | The gdextension entry point (`ShadowScaleExtension` + `entry_symbol`) and the crate's public re-exports. Nothing else — no decode logic |
| `bridge/command.rs` | `CommandBridge` (`#[godot_api]`), the command worker thread, `command_sender`, `resolve_entry_path` |
| `bridge/script_host.rs` | `ScriptHostBridge` (`#[godot_api]`) over the embedded script runtime |
| `bridge/decoder.rs` | `SnapshotDecoder` (`#[godot_api]`) + the free `decode_snapshot` / `decode_delta`. **The only entry into the decode path** (`SnapshotLoader.gd` is its one caller) |
| `bridge/variant.rs` | `Variant` ↔ `serde_json` marshalling shared by the bridges |
| `snapshot/mod.rs` | The two top-level assemblers: `snapshot_dict` (rasters + sections → the client dict) and `snapshot_to_dict` (walks a `WorldSnapshot`) |
| `snapshot/raster.rs` | `GridSize`, `OverlaySlices`, `TerrainSlices`, `OverlayChannelParams`, `packed_from_slice`, `insert_overlay_channel`, `normalize_overlay` |
| `snapshot/delta.rs` | `DeltaAggregator` + `CrisisAnnotationRecord` — a delta carries only changed sections, so it accumulates them into full-snapshot shape and re-enters `snapshot_dict` |
| `snapshot/cache.rs` | `WorldCache` — the world a delta is applied *to*: the last complete client dict, the pre-normalization `RasterCache` behind it, the `SectionCaches` (one complete array + identity index per diff-carried section, configured by the `KEYED_SECTIONS` registry), and the epoch/frame-sequence gate that says whether an incoming delta may be merged at all |
| `dict/mod.rs` | ONLY the leaf helpers with consumers in two or more sections: `strings_to_variant_array`, `string_vector_to_packed`, the `u16/u32/u64_vector_to_packed_*` packers, `fixed64_to_f32` / `fixed64_to_f64` |
| `dict/{map,economy,population,subsistence,knowledge,governance,culture,campaign}.rs` | The ~60 `*_to_dict` / `*_to_array` / `*_label` converters, one module per `snapshot.fbs` section |

There is deliberately **no `dict/vision.rs`** — the vision section is only the
fog/visibility/military rasters, which `snapshot/raster.rs` and the assemblers already
own (`sim_schema` makes the same call: a `codec/vision.rs`, no `state/vision.rs`).

## A merged delta frame must be an honest complete world

The client renders from ONE dictionary shape whichever payload produced it, so a merged delta has to
be indistinguishable from a full snapshot of the same state. Two things in `bridge/decoder.rs`
enforce that, and the first is there because it once was not.

**Every diff-carried section is PATCHED on every delta, never left standing.** A delta carries only
the rows that changed, and `snapshot_dict` — the assembler the delta path re-enters — does not
insert these keys at all; only `snapshot_to_dict` does. So the decoder published the changed rows
under the `*_updates` key and the BASE key kept the baseline snapshot's array **for the life of the
world**. Measured on `tiles`: `graze_biomass` summed over `tiles` was byte-identical across nine
consecutive turns while `tile_updates` carried 400–600 moved tiles per turn. It was **nine sections,
not one** — `Main`'s band alerts read `populations` (so food warnings, idle workers and
predator-nearby were frozen — a player-visible gameplay bug), and `MapView` reads `populations`,
`culture_layers` and `trade_links`.

`SectionCache` fixes the whole class with one mechanism: an identity → slot index built when a full
snapshot establishes the baseline, then per delta a shallow duplicate of the cached array (**pointer
copies of the entry Variants — never a deep copy of a row dictionary**) and one slot write per
changed row. The roster is the `KEYED_SECTIONS` registry in `snapshot/cache.rs`; a `SectionSpec`
carries the base key, the `*_updates` and `*_removed` keys, the identity field(s) and any watch
groups, so **a new diff-carried section is added there and at its one `merge_section` call site,
nowhere else**. Identity is one field or two (`populations` by `entity`, `culture_layers` by `id`,
`discovery_progress` by `(faction, discovery)`); removal ids differ in wire width (u16/u32/u64) and
are normalised to `i64` for the index while `RemovedIds` still publishes each at its own width.
Removals rebuild the index from scratch rather than repairing shifted slots — removals are
structurally rare, and a shifted-index repair is wrong exactly once and then silently forever. The
`*_updates` keys still ride the frame unchanged, because `TerrainPanel` and the inspector panels
branch on them. `WorldDelta` also diffs `logistics` and `knowledge_ledger`; they are absent from the
registry because the client decoder never converts either, so there is no base key to keep honest.

**Two whole-section fields were never read on the delta path at all**, which is the same staleness
reached a different way: `decode_delta_against` passed `None` for `food_modules` and
`faction_inventory`, so a merged delta republished the BASELINE's food modules and stockpiles for
the life of the world however many the server had since sent. The wire always carried them
(`WorldDelta::food_modules` / `faction_inventory`, both `Option`, absent = unchanged). When adding a
whole-section field, add it to BOTH `snapshot_to_dict` and `decode_delta_against`; the delta path
having its own list of sections is exactly why one can be forgotten.

**`culture_tensions` is NOT a keyed diff and must not be merged as one.** It is a whole-section
replace: present (even EMPTY) means "this is the roster now", absent means unchanged. That
distinction only became expressible when the field went `Option` on the wire — while it was a bare
`Vec` the codec emitted an empty vector for *both* cases, and the decoder's unconditional insert
blanked the baseline's tensions on the first delta, so `CulturePanel` showed none until the next
full snapshot. Do not re-add a client-side emptiness gate: against an `Option` field it would
swallow a genuinely-emptied roster instead.

**Every delta frame carries `changed_sections`, a `PackedStringArray` of what moved.** It is
**absent on a full snapshot, and absence means "everything changed"** — so a consumer that has never
heard of the manifest keeps working, and the frame that rebuilds the world is never gated by it. A
name is the dictionary KEY the frame carried the section under, so a consumer looks up the key it
already reads — and for a keyed section that is the COMPLETE key (`populations`), never the sparse
`*_updates` twin. The exceptions are the channels with no key of their own:
`overlays.{terrain, elevation, moisture, visibility, culture, sentiment, corruption, military,
logistics, crisis}` and `climate_bands`, which `DeltaAggregator` re-derives from cache and therefore
publishes on every merged frame (presence cannot be the signal, so the name is pushed at the
`apply_*` call site), plus `tiles.rivers` / `tiles.culture_layer` — `WatchGroup`s, derived by
comparing each changed tile against the entry it replaced so a turn that only moved graze biomass
costs no splatmap rebuild.

**A name means "this MOVED", not "this was transmitted."** The delta codec emits most keyed
sections' vectors unconditionally — empty when nothing changed — so presence is no signal at all;
every keyed section is named from its diff being non-empty. A steady-state delta on the decode
fixture names five things, not thirteen.

## Each delta merges into the frame BEFORE it, not into the baseline

`decode_frame` builds the merged frame from `cache.dict.duplicate_shallow()` and then **replaces**
`cache` — so frame N+1 starts from frame N, not from the full snapshot. Re-base that on the original
baseline and delta 2 silently discards delta 1: no error, no symptom, the world just drifts. (The
opposite failure is loud and therefore not the one to worry about: a cache that fails to advance
`frame_seq` makes the next delta unapplicable, which fires `resync_needed` and gets a full snapshot
every turn — slow, not wrong.)

**Two caches advance, and they fail differently — which is why `decode_guard` needs two witnesses
and one of them was only found by a mutation that failed to fail.** `SectionCaches` carries the
keyed sections, and their base keys are rebuilt and republished on EVERY frame out of that cache, so
they survive even a merge that re-bases the frame dictionary. Whole-section fields (`demographics`,
`herds`, `sedentarization`, …) do not: each lands in the merged dict once, on the frame that carries
it, and stays only because the next merge starts from that frame. So the chained fixture probes both
— the keyed sections for a section cache that stops advancing, and two whole-section witnesses for
the frame dictionary, each carried by delta 1 and absent from delta 2: `demographics` (a repeated
section) and `equipment_config_json` (a bare `Option<String>`, see THE KIT ROSTER below).

**The delta path is guarded by a CHAIN, not a single frame** (`snapshot_delta_envelope.bin` +
`snapshot_delta2_envelope.bin`, moving deliberately disjoint rows). One delta only exercises
baseline → delta; the client takes delta → delta on every turn after the first, and that path went
unguarded until it was added after the fact.

**The dangerous direction is an UNDER-complete manifest** (a consumer skips a section that really
moved and goes silently stale), which is why it is built by a forcing function rather than a
hand-maintained list: `DeltaFrame` owns both the dictionary and the name vector, and
`insert_changed` is the only way to put a delta-carried section on the frame. `insert_always` is for
what rides every frame and is therefore not a change: the three identity keys and the patched
complete arrays.

**The rule for a new snapshot field: its converter goes in its section's `dict/` module** —
the section is whichever `.section()` accessor `snapshot_to_dict` reaches it through. Put a
helper in `dict/mod.rs` only once a *second* section needs it, and hoist rather than
duplicate. Fixed-point (`Scalar`, 1e6) fields go through `fixed64_to_f32`/`_f64`, never an
inline divide — and a new `Scalar` **cohort** field belongs in `CohortScalars`
(`dict/population.rs`), which is the one part of this crate `cargo test` can reach
(`VarDictionary` cannot be built outside a live engine).

`command_events_to_array` (`dict/campaign.rs`) carries **`seq`**, and the campaign section carries
**`command_events_retention_turns`** — the two fields the event dock needs (issue #272,
`.claude/rules/client/event-dock.md`). Two decode rules ride them, and both are about a FlatBuffers
default being indistinguishable from a real value:

- **`seq` is published raw and is ONE-BASED**: `0` is both the schema default and the sim's own
  "never pushed through `CommandEventLog`" sentinel, so the CLIENT decides what to do with it (it
  falls back to signature de-duplication). The decoder does not invent a substitute.
- **`command_events_retention_turns` is WITHHELD when 0**, on both the full and the delta path,
  because "not stated" and "a retention window of zero" are the same bits and only one of them is
  ever meant. The client's own default then stands. It goes on the delta through **`insert_always`,
  not `insert_changed`**: it rides the campaign section rather than being a section of its own, and
  naming it in the manifest would invite a consumer to gate on a section name nothing else uses.

**The decoder does NOT accumulate `command_events`.** It is per-frame history by existing contract —
a delta carries only the newly appended rows, a full snapshot the whole retained ring — and the
accumulation belongs to the CONSUMERS (`EventDockPanel`, `TellingPanel`), each with its own retention
and de-duplication. `WorldCache` must not grow a ring for it.

## `pending_reveal_count` — the one field this decoder PROJECTS rather than copies

`PopulationCohortState.pendingRevealX` / `pendingRevealY` are the tiles a party has observed and not
yet reported. `population_to_dict` emits **their LENGTH, as `pending_reveal_count`, and neither
array**.

The client's only question of them is *"does this party still owe its home band a map report"* — the
fourth term of the sim's cancel-in-camp test (`cancel_party_standing_in_camp` → `party_owes_a_report`,
itself just `!pending_reveal.is_empty()`), which decides whether a recall reads **Cancel** or **Recall**
(`band-city-panel.md`). The coordinates are a scout's ACCUMULATED reveals — hundreds of tiles per
cohort per frame, every frame until it reports — so marshalling them into GDScript would carry that
whole payload to answer a boolean.

**A decoder that projects needs its reason written at the site**, which is why the comment sits with
`is_expedition` / `home_band_entity` rather than here alone: the next reader's instinct on finding no
`pending_reveal` key is to add the arrays. `0` is the honest reading for a resident band and for a
party with nothing left to deliver.

`population_to_dict` decodes two **Predators Phase 3** cohort keys (appended after `fodderStore` in
the schema): `raid_radius` ← `cohort.raidRadius()` (a plain `uint` reach, `as i64` — like `work_range`,
NOT a Scalar), the odd-r hex distance within which an aggressive carnivore herd raids this band's
larder; and `raid_forfeit` ← `cohort.raidForfeit()` (`float`, `as f64`), the food this band lost to
raids THIS turn — the raid twin of `pen_feed_upkeep`. Both are consumed client-side by the band panel:
`raid_radius` derives the "Predator nearby" Warrior alert (the DANGER itself is derived on the client
from visible-herd telemetry, never a wire flag), `raid_forfeit` is the "Lost to raids" food-ledger row.

`population_to_dict` also decodes the **minimal TOE** (`docs/plan_hunt_through_combat.md` §4.8) — the
band's three consumable kits and the tiers they resolve to: `hunting_kit_durability` /
`sled_kit_durability` / `basket_kit_durability` (condition on equipment.json's 0-100 scale, `0` = dry)
plus `hunter_attack` / `hunt_carry_per_worker_biomass` / `forage_carry_per_worker_biomass`. All six
shipped on the wire with **no consumer here at all** — the third time this arc reproduced this crate's
most-repeated bug — as did the labor assignment's forecast BAND (`actual_yield_low`/`_high`,
`trade_yield_low`/`_high`, §6.4) and `HerdTelemetryState.durability` (§4.2/§6.5, the last term the
combat gate needed). Eleven fields, thirty golden lines, no fixture edit: `decode_fixture.rs`'s
SATURATION reaches an appended scalar automatically, so the only step an appended scalar needs here is
the converter and a re-record.

**ONE KIT, ONE JOB, and the two carry tiers are not two readings of one number.** A band can be out of
baskets with its sled untouched, so `hunt_carry_per_worker_biomass` and
`forage_carry_per_worker_biomass` must never be rendered on each other's rows — the defect slice 5
corrected sim-side. The golden gives every field a DISTINCT saturated value, which is what makes a
swapped accessor visible in the diff rather than merely different.

It also decodes **`expedition_forecast_horizon_turns`** ← `cohort.expeditionForecastHorizonTurns()`, a
plain `uint` echoed on every cohort beside `expedition_viability_warn_turns` — the SCALE every "never
completed" sentinel on this wire is relative to (`turns_to_fill == 0`,
`turns_to_collapse{,_low,_high} == 0`, `expedition_trip_bound == "horizon"`). **It is not a trip
length**: it bounds the hunting alone, so a client quoting it as one understates the trip by the whole
walk — the floor on a hunt's span is `this + round-trip travel` (`labor-ui.md` → "An unbounded raid
quotes a FLOOR"). Because the MARKER is a structural `duplicate()` of the cohort, it reaches the
in-flight denial readout — whose caller has no band and reads the horizon off the launched party —
without a stamp; `marker_field_guard` carries it so the copy stays honest.

**THE PRE-LAUNCH RAID FORECASTS ARE NOT ON THE SNAPSHOT, SO `herds_to_array` DECODES NO ESTIMATE
TABLE.** `HerdTelemetryState`'s `huntTripEstimates` / `denialEstimates` / `denialPartyNeeded` and the
two `*EstimatesKitId` fields are `(deprecated)` slots in `snapshot.fbs` that the sim no longer
writes: a herd row is a fact about a *herd*, and a raid's numbers depend on the asking band's kit and
live equipment wear, which no per-herd row can carry. The client **asks** instead — see
`.claude/rules/core_sim/expeditions.md` → "The forecast is ASKED FOR".

`bridge/query.rs` owns that exchange. It encodes a `QueryCommand` (`sim_runtime/proto/command.proto`)
and reads back a `QueryReplyEnvelope` on **the command socket**, which is bidirectional: the reply
frames share the read path's 4-byte little-endian length prefix and the same
`sim_runtime::MAX_PROTO_FRAME` bound, and that bound has one definition precisely because both ends
must agree on it. Replies cross to the main thread through `CommandBridge.poll_query_replies` rather
than a signal off the worker, so a render never observes a half-applied answer.

**A QUERY TRIGGERS NO SNAPSHOT.** The server answers and skips the recapture — the query changed
nothing, and that recapture is the expensive half of a turn. Nothing downstream may wait on a frame
to render an answer; the reply is the whole of it.

**THE WASTE IS A PAIR, AND BOTH HALVES ARE DECODED** — `wasted_trade` is the twin `delivered_trade`
already had, and it rides `DenialRow` on the reply for the reason it rode the retired table: the sim
prices both out of ONE `HuntYield::apply` over the wasted biomass, so a kill left on the range takes
its hides with it. Decoding the food half alone reported a raid whose quarry pays pelts as wasting
nothing — on the one mission whose entire readout is what it destroys and does not bring home.

**A `0` ON ANY TURN FIELD MEANS "not within the horizon on that end", never "immediately"**, and
`outcome` is what the client renders instead of a blank — decode them together or the consumer cannot
tell a repelled party from an expired clock.

`DenialRaidForecastReply.party_needed` is the smallest party whose raid SUCCEEDED (`past_recovery` /
`herd_lost`, never `horizon`, whose projection merely ran out) — the party the compose sheet's
stepper opens on. **`0` means no party the band can field drives this herd down**, because the
search walks contiguously to the band's last fieldable worker rather than stopping at a sampled rung;
it is never "send nobody", and it can no longer name a party the band had no hope of raising.

## THE KIT ROSTER — six additions, three homes (`docs/plan_denial_raid.md`)

Kit selection lands as one roster plus five per-row ids, and they are decoded in the module that owns
each one's section:

| wire | key | module |
|---|---|---|
| `SubsistenceSection.kits:[KitOption]` | `kits` (array of `{id, display_name, jobs, attack, hunt_carry_per_worker_biomass, forage_carry_per_worker_biomass, item_ids, …}`) | `dict/subsistence.rs` → `kits_to_array` |
| `KitOption.itemIds` | `item_ids` — the kit's `uses` list verbatim, in config order | `dict/subsistence.rs` → `kits_to_array` |
| `SubsistenceSection.defaultHuntKitId` / `defaultForageKitId` | `default_hunt_kit_id` / `default_forage_kit_id` | `snapshot/mod.rs` + `bridge/decoder.rs` |
| `SubsistenceSection.equipmentConfigJson` | `equipment_config_json` — the whole effective `EquipmentConfig`, `serde_json`-serialized | `snapshot/mod.rs` + `bridge/decoder.rs` |
| `PopulationCohortState.kitId` | `kit_id` on the band dict | `dict/population.rs` |
| `LaborAssignment.kitId` | `kit_id` on the assignment entry | `dict/population.rs` |
| `PopulationCohortState.kitTiers:[BandKitTiers]` | `kit_tiers` on the band dict — per kit id, that band's tiers resolved against its LIVE equipment wear | `dict/population.rs` |

**The roster, its two defaults and the serialized config are WHOLE-SECTION fields, so they are
decoded on BOTH paths** — `snapshot_to_dict` and `decode_delta_against` — which is the rule the
`food_modules` / `faction_inventory` staleness above records. A whole-section field read only on the
full path republishes the baseline's value for the life of the world. The sim diffs each of them as a
`Whole<_>`, so it rides a delta ONLY when it moved: presence on the delta IS the change signal there,
which is why all four go through `insert_changed` and not `insert_always`.

**`equipmentConfigJson` is republished as ONE OPAQUE STRING and is deliberately never parsed here.**
The Workbench's Equipment and Kits pages parse it themselves and walk it blind, which is what lets a
field added to `equipment.json` reach the surface with no client edit (`workbench.md` → "The two
config pages PRINT the config"). A decoder that unpacked it into typed keys would put the hardcoded
field list back, one layer lower down where nothing on the GDScript side would catch it.

**`equipment_config_json` rides the delta CHAIN as a whole-section witness; the other three do
not.** Delta 1 restates it as `{"fixture":"delta.equipment_config_json"}` — deliberately unequal to
the baseline's saturated `"equipment_config_json"`, because a decoder that ignored the delta and
republished the baseline would otherwise satisfy the guard — and delta 2 leaves it absent, so the
merged frame keeps it only because each delta merges into the frame before it. It is the scalar twin
of `demographics`: a delta path that handled the repeated whole sections and forgot the bare
`Option<String>`s is exactly what it catches. The seeding and both properties live in
`xtask/src/decode_fixture.rs` (`WholeSectionWitnesses`, `DELTA_EQUIPMENT_CONFIG_JSON`), whose own
CI-reachable test asserts delta 1's value differs from the baseline's and that delta 2 carries none —
without that, a fixture drifting back to the baseline's value would make the guard's assertion
vacuous while nothing went red.

`kits`, `defaultHuntKitId` and `defaultForageKitId` are still full-path-only: the golden gains their
line and the chain assertions never see them. That is not an argument that their delta half is
optional; it is the reason it has to be written by rule rather than by the guard going red.

**`KitOption`'s three tiers are the FRESH-kit ones, and they are not any band's numbers.** What a
given band's wear does to them is the band's own cohort row (`hunter_attack` /
`hunt_carry_per_worker_biomass` / `forage_carry_per_worker_biomass`), and the client composes the
effective tier from the two (`KitRoster.effective_tiers`). A readout quoting the roster's number to a
band with dry spears is the defect class this arc keeps correcting.

**`none` is an ORDINARY roster entry and nothing here special-cases its id** — it grants nothing, so
its tiers are the unequipped ones throughout. It is authored last in `equipment.json` and the decode
preserves that order, which is the whole of why it sorts last in the picker.

**`kitTiers` is the RESOLVED answer, and nothing here may re-derive a tier from the roster plus
`kitItemConditions`.** The derivation is impossible rather than merely redundant: the axis→item
mapping is per kit — `big_game` supplies `attack` from `spears`, `trapping` from `traps` — so
`itemIds` says what a kit carries but not what each item is *for*, and set-cover, positional order,
any-item-live and all-items-dry each fail a shipped case. The sim resolves it once against the band's
own wear and publishes the result.

**The whole path is gated by `tools/decode_guard.gd`** (see its Key Scripts row) — the answer to
"`VarDictionary` cannot be built outside a live engine", which is why the coverage here was a single
`cohort_decode_tests` module for so long. Run it from the workspace root:

```bash
cargo xtask decode-guard                  # regenerate fixture → build native → diff the golden
cargo xtask decode-guard --write-golden   # re-record after an INTENDED decode change
```

**When you append a snapshot field, that command is what tells you the decoder actually emitted
it.** The golden gains a line carrying the field's own wire path as its value; if the new key does
not appear, the converter was never wired up — the "decoded in `native/src/lib.rs`" bug this file
records **six** times. Two forcing functions sit under it, both in `xtask/src/decode_fixture.rs`:
appending a **repeated** field fails the fixture build until it is seeded (`assert_no_empty_arrays`
names the path), and appending to one of the state structs that has no `Default` fails the *compile*
(those blanks are exhaustive literals on purpose).

**Those two forcing functions reach CI; the golden diff does not.** CI has no Godot and the decoder
returns a `VarDictionary`, so the diff is a **local** gate — but `xtask`'s own `cargo test` builds
the fixture, which means the unseeded-repeated-field alarm and the fixture's determinism are checked
on every PR. Run `cargo xtask decode-guard` yourself for the part CI cannot.

**A MALFORMED snapshot must DEGRADE, never panic — and `snapshot_to_dict` returns `Option` to say
so.** `snapshot.fbs` marks nothing `required` and `root_as_envelope` verifies table STRUCTURE only,
so a verifiable payload can still be missing a field the decoder needs. Today that is the `header`:
absent, it answers `None`, which reaches the loader as an empty dictionary and the frame is skipped
(`SnapshotLoader.poll_stream` already had that branch). **Dropping the frame is deliberate and is
the rule for any field the decoder cannot do without** — the header carries the frame's identity
(`tick`, `worldEpoch`) and the grid's topology (`wrapHorizontal`), each with a plausible-looking
zero, so filling in defaults publishes a coherent-looking world that is quietly wrong instead of one
that is honestly absent. The delta path reaches the same "never unwrap" outcome its own way
(`if let Some(header)` in `bridge/decoder.rs`) and is what the snapshot path was inconsistent with.
Both halves are gated: the headerless fixture pins the empty-dictionary contract, and the xtask
runner fails the run on a caught Rust panic (see the `decode_guard.gd` Key Scripts row).

> Doc references elsewhere in this file of the form "decoded in `native/src/lib.rs
> `*fn*`" predate the split — the named function now lives in its section's module above
> (e.g. `herds_to_array` → `dict/subsistence.rs`, `tile_to_dict` → `dict/map.rs`,
> `population_to_dict` → `dict/population.rs`). The function names did not change.

> **Note:** Elevation is not rendered as 3D relief. A shallow-3D heightfield view was
> prototyped and permanently removed; elevation is surfaced as the 2D **Elevation
> Heatmap** overlay and as a per-tile **Height** readout in the tile panels (the HUD
> selection panel via `MapView._tile_info_at` → `Hud._tile_summary_lines`, and the
> Inspector Terrain tab). All read the same normalized `ElevationOverlay.samples` raster —
> there is no per-tile elevation on `TileState`. **Height is a relative 0..100 indicator**
> (a number + filled/empty bar), NOT meters: it exists so a player can reason about line of
> sight — a higher tile can occlude the tile behind it (matching the LOS raycast in
> `visibility_systems.rs`). `MapView.relative_height_at` rescales the above-sea-level span
> into 0..100 (at/below sea level reads 0, since open water occludes nothing). The sea level
> is the **active map's** `sea_level`, streamed per-snapshot as `ElevationOverlay.seaLevel`
> (pre-normalized server-side to the raster's [min,max] scale) and read into
> `MapView._elevation_sea_level` — no hardcode; `HEIGHT_DEFAULT_SEA_LEVEL` is only the
> pre-first-snapshot fallback. `MapView.format_height` is the single source of truth for the
> number+bar formatting. The
> raster still streams from the core for the heatmap and for gameplay (LOS), but the
> per-vertex `normals` field (3D-only) was dropped from the schema. See
> `docs/architecture.md` → "Removed: 3D Relief Rendering".

---

