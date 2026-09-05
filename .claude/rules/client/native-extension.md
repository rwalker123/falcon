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
| `dict/{map,economy,population,subsistence,knowledge,governance,culture,campaign,connections}.rs` | The ~60 `*_to_dict` / `*_to_array` / `*_label` converters, one module per `snapshot.fbs` section |

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
predator-nearby were frozen — a player-visible gameplay bug), and `MapView` reads `populations`
and `culture_layers` (it read `trade_links` too, until that section and the overlay it fed were
retired — `.claude/rules/client/overlay-channels.md`).

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
branch on them. `WorldDelta` also diffs `knowledge_ledger`; it is absent from the registry because
the client decoder never converts it, so there is no base key to keep honest.

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
crisis}` and `climate_bands`, which `DeltaAggregator` re-derives from cache and therefore
publishes on every merged frame (presence cannot be the signal, so the name is pushed at the
`apply_*` call site), plus `tiles.rivers` / `tiles.culture_layer` — `WatchGroup`s, derived by
comparing each changed tile against the entry it replaced so a turn that only moved graze biomass
costs no splatmap rebuild.

**A name means "this MOVED", not "this was transmitted."** The delta codec emits most keyed
sections' vectors unconditionally — empty when nothing changed — so presence is no signal at all;
every keyed section is named from its diff being non-empty. A steady-state delta on the decode
fixture names five things, not thirteen.

**THE `logistics` CHANNEL IS GONE, AND SO IS ITS DIMENSION SIDE EFFECT.** `OverlaySlices.logistics`
/ `RasterCache.logistics` / `DeltaAggregator::apply_logistics_raster` / the `overlays.logistics`
manifest name and the top-level `contrast` alias were all removed when the sim stopped publishing a
`logisticsRaster` (`docs/plan_contact_and_logistics.md`). Two things about the removal are worth
knowing before touching `snapshot_to_dict`:

- **The logistics grid was the GRID-EXTENT source for every other channel.** Its absent-raster
  fallback walked `MapSection.tiles` for `max(x + 1, y + 1)` — filling the plane with tile
  TEMPERATURE on the way, which is what made the channel meaningless long before the raster went —
  and every other channel's fallback dimensions plus `final_width`/`final_height` were taken over
  `logistics_dims`. That measurement survives as **`tile_dims`**, read straight off the tiles with
  no plane behind it. Delete it and a snapshot whose only grid-shaped evidence is its tile list
  renders at 1×1.
- **`DeltaAggregator::tile_updates` went with it.** That `HashMap<(u32, u32), f32>` of tile
  temperature existed solely to feed the delta path's copy of the same fallback, so `update_tile`
  no longer takes a `temperature` at all. Tile temperature still reaches the client the ordinary
  way, on the tile row (`dict/map.rs`).

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

## THE AGE BRACKETS ARE WHOLE PEOPLE, AND THERE ARE ONLY THREE OF THEM

`population_to_dict` publishes `children` ← `cohort.childrenCount()`, `working_age` ←
`cohort.workingAge()` and `elders` ← `cohort.eldersCount()`, all plain `uint` counts cast to `i64`.
**None of them is a `CohortScalars` member** — they are not fixed-point and take no divide.

**`working_age` IS the working bracket.** There is deliberately no fourth `age_working`-style key
beside it. This decoder used to carry both: `working_age` (the assignable workers) and an `age_*`
trio decoded from `PopulationCohortState.children/working/elders`, which were raw `Scalar`s. The
`age_*` prefix existed to keep the two apart, and the naming trap it guarded is gone with the second
number — the deprecated Scalar slots are no longer written and their accessors are gone from the
generated bindings. Two names for one number is how a band came to render "17" in the panel's PEOPLE
bar beside "0 idle of 16" in the WORKFORCE header on the same frame.

The fraction the sim keeps internally is a growth accumulator, not a fact about people. It rounds
once, writes `size` as `childrenCount + workingAge + eldersCount`, and nothing here re-decides it.
The neighbouring `PopulationDemographicsState` `children`/`working`/`elders` (the faction-wide
figures) were always plain `uint` counts and are unaffected.

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
raids THIS turn — the ledger's only debit beyond consumption, `pen_feed_upkeep` having been retired
(human food is not animal feed; see `band-readouts.md`). Both are consumed client-side by the band panel:
`raid_radius` derives the "Predator nearby" Warrior alert (the DANGER itself is derived on the client
from visible-herd telemetry, never a wire flag), `raid_forfeit` is the "Lost to raids" food-ledger row.

**THE BAND'S HAY LEDGER, three cohort keys appended last** — `fodder_need` / `fodder_income` /
`turns_of_fodder`, the fodder twins of `food_income` / `food_consumption` / `turns_of_food`, in FODDER
units against the `fodder_store` above. `fodder_need` is the hay the band's pens are SHORT per turn,
**summed by the sim** over every pen it keeps (each pen's own share is the gap its fenced footprint
leaves, which the sim computes but does NOT publish per pen — see `pen_fodder_shortfall` below, which
is a DIFFERENT quantity and does not sum to this one): herd rows are fog-filtered, so a client-side
sum silently drops the pens it cannot see — the
mistake the retired `pen_feed_upkeep` was minted to avoid. `fodder_income` is the raw harvest its
fodder Fields took, not a Foddering-gated share. `turns_of_fodder` comes off the sim's own
`larder_runway_turns`, **999 no-drain sentinel included**, so the client reads it through
`BandFoodStatus.is_limited` / `DetailFormat.food_turns_text` exactly as it reads the food runway —
one idea, one spelling, no second constant and no second branch.

`herds_to_array` decodes the per-pen hay key **`pen_fodder_shortfall`**, beside `pen_pasture_fraction`:
how much MORE fodder this pen needs per turn, in fodder units — `max(0, hay gap − fodderDraw)`, the
gap its own fenced footprint leaves less the hay its keeper actually carried in. `0` on an unpenned
herd and on a pen its land already feeds (so the readout says nothing rather than `needs 0.0`). It is
**not gated on Foddering** — a keeper who cannot draw hay is short its WHOLE need, which is precisely
the case the row exists for. A FIXED footprint under a growing herd is a RISING shortfall, which is
the slow trap the field surfaces before an animal dies of it.

**THE GAP ITSELF IS NOT DECODED, BECAUSE IT IS NOT PUBLISHED.** It rode this row as `penHayNeed` until
it turned out nothing rendered it — the pen row states how much more is needed, not the gross gap —
and the wire slot is now `(deprecated)`. The sim owns that subtraction and publishes only its result,
which is what makes it impossible for the difference to describe a different turn from its terms; a
decoder that re-derived the gap from `pen_fodder_shortfall + fodder_draw` would be minting a wire
field client-side.

`population_to_dict` also decodes the **minimal TOE** (`docs/plan_hunt_through_combat.md` §4.8) — the
band's three consumable kits and the tiers they resolve to: `hunting_kit_durability` /
`sled_kit_durability` / `basket_kit_durability` (condition on equipment.json's 0-100 scale, `0` = dry)
plus `hunter_attack` / `hunt_carry_per_worker_biomass` / `forage_carry_per_worker_biomass`. All six
shipped on the wire with **no consumer here at all** — the third time this arc reproduced this crate's
most-repeated bug — as did the labor assignment's forecast BAND (`actual_yield_low`/`_high`, plus a
`trade_yield_low`/`_high` pair arc #527 has since retired with its account, §6.4) and `HerdTelemetryState.durability` (§4.2/§6.5, the last term the
combat gate needed). Eleven fields, thirty golden lines, no fixture edit: `decode_fixture.rs`'s
SATURATION reaches an appended scalar automatically, so the only step an appended scalar needs here is
the converter and a re-record.

**A VECTOR FIELD IS NOT AN APPENDED SCALAR, and the three material fields are the worked example**
(arc #527 follow-up): `HerdTelemetryState.materialPerBiomass` / `perWorkerMaterial` →
`material_per_biomass` / `per_worker_material` on the herd dict, and `LaborAssignment.materialYield` →
`material_yield` on the assignment, each an `Array` of `{material_id, amount}` dicts. Saturation still
reaches them, so the re-record is still the only golden step — but a consumer that treats one like a
scalar fails LOUDLY and at a distance: `HudBandLaborState.OPTIONAL_YIELD_KEYS` coerces every entry
through `float()`, and an `Array` through that constructor is `Invalid call. Nonexistent 'float'
constructor` raised inside `effective_worker_map`, which surfaces as a work board with **zero rows**
rather than as a bad number. The vector is copied beside that list, verbatim; normalizing is
`SourceForecast.material_payoff_rows`' job, beside the readouts that spend it.

**AND THE STANDING-UPKEEP SLICE ADDED EIGHT MORE, ON THREE TABLES** (`docs/plan_standing_upkeep.md`
§2.7): `ForagePatchState` / `HerdTelemetryState` gained `buildMaterialCost` / `upkeepMaterialDemand` /
`upkeepMaterialSupplied`, `LaborAssignment` gained `materialUpkeepDemand` / `materialUpkeepSupplied`,
and `PopulationCohortState` gained `materialUpkeepNeed` / `materialUpkeepIncome` / `materialStore` —
every one a `[MaterialPayoff]` through `material_payoffs_to_array`, so the decoder step was a snake_case
insert per field and nothing else.

⛔ **THE ALLOWLIST BIT THE ASSIGNMENT PAIR EXACTLY AS THIS SECTION WARNS.** The decoder emitted both
terms and the work board's note came out EMPTY, because `HudBandLaborState.effective_worker_map` is a
hand-listed allowlist and a key not copied there does not exist as far as the board is concerned. The
pair is copied verbatim beside `material_yield` — never into `OPTIONAL_YIELD_KEYS`, whose `float()`
coercion is what the paragraph above is about.

**THE EXPEDITION HALF ADDS ONE MORE VECTOR AND NEEDED NO NEW DECODER AT ALL.**
`HuntTripRow.delivered_material` → `delivered_material` on every row of the `HuntTripForecast` QUERY
reply (`bridge/query.rs`, not the snapshot path) — the trip's whole payload per material, which is
what makes an inedible quarry's raid legible. Beside it, **`PopulationCohortState.materialBatches` is
resolved from `cohort.stores` with NO resident-band gate**, so a detached party's carried materials
were already decoded onto the cohort dict as `material_batches` and had simply never been rendered
for a party. **That is the failure worth remembering here**: a field the decoder emits correctly and
no surface reads is invisible to every guard in the tree — the golden asserts it decoded, and nothing
asserts anyone looked.

## The basket entry carries FOUR per-species facts, and none of them is a parallel array

`ForagePatchState.compositionStandingBiomass`, `compositionProvisionsPerBiomass`,
`compositionFodderPerBiomass` and `compositionMaterialPerBiomass` are all index-aligned with
`composition`, and `dict/subsistence.rs` **folds each onto the entry it belongs to** rather than
publishing four sibling arrays. The wire
keeps them apart for a memo reason — a composition entry is a pure function of ground and config and
is shared by refcount across every frame, while a standing biomass and a rate move every turn — and
on this side the schema's own rule is that a client must read them as ONE OBJECT, which folding makes
structural: a consumer cannot index the lists apart.

It also means the patch's cross-ref needs NO new key. `composition` travels whole in
`patch_composition`, so the two-wirings trap that has bitten the plant web three times
(`labor-ui.md` → "THE PATCH'S FORECAST FIELDS REACH THE SHEET THROUGH `tile_info`") cannot reach any
of the three.

**PRESENCE IS CARRIED BY THE KEY EXISTING, NEVER BY THE VALUE, and on the three rates that earns its
keep twice.** An entry a vector is too short for carries no key at all — the server stated nothing —
while a cash crop honestly converts at `0.0`, and a missing-means-zero reading would make those two
indistinguishable on exactly the plants the selective gather is about. The client then quotes the
`0.0` as a real rate and says the unstated one is unpriced (`labor-ui.md` → "…AND WHAT IS STILL NOT
KNOWN IS SAID OUT LOUD").

**NONE OF THE THREE RATES IS PRE-SCALED BY SHARE**, which is what makes a subset composable, and
**summing them across species without the shares is not a total of anything** —
`SourceForecast.selection_rates` is the one place in the client that composes them, as a weighted
mean (the material one **per material id**, never as a scalar).

**THE MATERIAL ONE IS A VECTOR OF VECTORS AND ITS WRAPPER IS PLUMBING.** FlatBuffers has no
vector-of-vectors, so `compositionMaterialPerBiomass[i]` is a one-field `SpeciesMaterialRates` table
and the decoder reads `.rows()` through the same `material_payoffs_to_array` every other material
vector goes through — read it as "entry i's materials", not as a model. **The key is written for
every entry the wrapper vector reaches, EMPTY ROWS AND ALL**: a grain pays no material and says so
with an empty list, while a `0`-valued row would read as a crop that pays badly.

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

**RETIRED — the waste PAIR.** `wasted_trade` rode `DenialRow` as the twin `delivered_trade` already
had, because the sim priced both out of ONE `HuntYield::apply` over the wasted biomass and decoding
the food half alone reported a raid whose quarry pays pelts as wasting nothing — on the one mission
whose entire readout is what it destroys and does not bring home. **Arc #527 retired the account**, so
the reply carries `wasted_food` alone and the client's waste clause is a single figure. **The rule
that put the pair there survives it**: an appended field that is one half of a sim-side pair must be
decoded with its sibling, or a readout states half a fact and reads as a zero.

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
| `PopulationCohortState.huntCrews:[BandKitCrew]` | `hunt_crews` on the band dict — one `{workers, hunter_attack, item_ids}` row per run of hunters holding identical gear, best-equipped first, `Σ workers` = the hunt head count. **Never empty**, so no reader needs a "no crews" branch; a band with nobody on the hunt publishes one row at `workers 0`. Its inner `item_ids` is a repeated field inside a repeated one, which is the shape a decoder is most likely to drop or flatten | `dict/population.rs` |
| `KitItemCondition.workersHolding` | `workers_holding` beside `count` and `remaining` — **`count` is UNITS, this is PEOPLE**, and the two differ whenever the band is short or holds the spawn's reserve. A `0` is three sentences (nobody staffed, owns none, no quoted kit carries it), which is why `count` rides beside it | `dict/population.rs` |
| `KitItemCondition.workersOnQuotedJob` | `workers_on_quoted_job` — **its DENOMINATOR, and the pair is one sentence**: the head count of the job the row is quoted at, off the same coverage the numerator came from, so the two can never describe different jobs. It is what makes a BASKET / CLUB / WAYFINDING shortfall sayable at all — before it, `Σ huntCrews.workers` was the only job head count on the wire. **A `0` here means NOBODY IS STAFFED, not a shortfall**, and nothing may divide by it | `dict/population.rs` |

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

> #### ⛔ A FIXTURE COVERING AN ARM IS NOT THE GUARD COVERING IT — and the bench's rank is the worked example
>
> `BenchState.priority` reached the wire with `BENCHED_SOURCE_PRIORITIES` already seeding a `High`
> bench and a `Low` one into the decode fixture, and the guard still passed with a deliberately broken
> `High` mapping — because **no converter read the field**, so it never reached the decoded dictionary
> and the golden had no line for it to move. *"The fixture covers the arm"* and *"the guard covers the
> arm"* were two claims and only the first was true.
>
> Adding `priority` to `bench_dict` is what closed it, and the closure was DEMONSTRATED rather than
> asserted: with the key in place and the golden re-recorded, emitting `"normal"` from every arm makes
> `cargo xtask decode-guard` **FAIL**, naming both moved rows (`1827 "high" → "normal"`,
> `2458 "low" → "normal"`); restoring it passes at 55 top-level keys. **A seeded fixture value proves
> nothing until a golden line moves with it** — which is the enum form of the "decoded in
> `native/src/lib.rs`" bug this file records six times, and the reason a new arm is worth breaking on
> purpose once.
>
> **The `_ =>` catch-all is why only two of the three arms can be guarded here.** A cohort carries
> exactly ONE bench, so two cohorts reach two of the three levels — and those are the two that matter:
> a wrong `Normal` arm is unreachable by construction (the catch-all IS `Normal`), while a wrong `High`
> or `Low` decodes silently as `normal`. The `Normal` arm is covered end to end at the codec level by
> `core_sim/tests/crafting_wire.rs`.

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


## The BUILD, priced in WORK — thirteen key names over sixteen sites, and one the decoder deliberately DROPPED

`docs/plan_unit_costed_work.md` §8. An improvement costs a fixed number of WORK UNITS now and turns
are the OUTPUT, so `dict/subsistence.rs` decodes the absolutes beside the `0..1` fractions it already
carried: `cultivation_work_{done,cost}` / `field_work_{done,cost}` on a patch,
`tame_work_{done,cost}` / `corral_work_{done,cost}` on a herd, plus **three per SOURCE** (at most one
improvement is ever in flight on one) — `build_turns_remaining` and `build_work_from_gear`, the sim's
own answer and the resolved gear it already spent, and `build_work_per_worker_turn`, the source's own
term in the same estimate. **The estimate's GEAR terms are not source fields at all** — see below.

- **`buildTurnsRemaining` is an `i32` and `-1` is its SENTINEL** — "no estimate", for a stalled build
  or a source nobody works. It is cast `as i64` and published raw; nothing here may substitute a `0`,
  which the client would render as a build about to land.
- **`build_work_per_worker_turn` rides BESIDE that answer, never instead of it**, and the decoder must
  carry both shapes because two client surfaces ask different questions of them: the sheet evaluates
  `turns(workers)` against a crew the player is proposing, the tile card renders the answer for the
  crew already there (`.claude/rules/client/labor-ui.md` → "TWO SURFACES ASK DIFFERENT QUESTIONS").
  **It is decoded rather than assumed to be the `1.0` it is today** — the sim writes worker output as
  a sum of terms, so a client-side constant would go stale silently the day a second one lands.
- **The GEAR half rides the KIT ROW** — `build_work_per_worker` / `build_work_saturating_crew` on each
  `PopulationCohortState.kitTiers[]` entry, decoded in `dict/population.rs`, because both facts behind
  them are the band's ledger rather than anything about a worked source. That is what lets a rung
  nobody has started carry a quote at all, and what makes a compose sheet's kit picker re-price the
  whole estimate. `build_work_from_gear` on the source is the RESOLVED contribution for the crew that
  worked it this turn — a different question, and not one a stepper can move.
- **`build_rate` is NO LONGER DECODED, on either kit table** (`KitOption` and the cohort's
  `BandKitTiers`). The wire keeps the slot frozen at its neutral `1` so a client still compiles, and
  `buildWorkPerWorker` supersedes it — the work units one equipped worker takes off a build. Leaving
  the old key decoded is the trap rather than the safe option: every kit then reads "changes no
  build", which silently strips the handling gear's own clause AND withholds the kit carrying it from
  the herd being tamed (`KitRoster.kit_offer` asks that axis first).
- **The plant seven are TWO wirings, and the guard is what says so.** A patch does not travel whole:
  `MapView._tile_info_at` copies it key by key, so every one of them also needs the `patch_`-prefixed
  cross-ref and a `FOW_DISCOVERED_HIDDEN_KEYS` entry. `tools/patch_crossref_guard.gd` caught exactly
  that omission on this arc — the decoder emitted all seven and the panel would have read none. (The
  kit row travels whole, like a herd dict, so the gear pair is one wiring.)

## The `connections` section, and the cohort fields the shipment arc appended

Arc #527. `dict/connections.rs` → `connections_to_array` is the client's FIRST reader of the contact
ties (#538 shipped the section with none), and it is a **whole-section replace** — decoded on BOTH
paths through `insert_changed` on the delta, exactly like `culture_tensions` and the crafting
catalogues. Present-and-EMPTY means *"you hold no ties now"*, which is why there is no emptiness gate
here: adding one is the defect that blanked the culture tensions on every first delta.

**No faction column, and the decoder must not invent one.** Faction is a property of the endpoint
(`.claude/rules/core_sim/connections.md`), and the section is already filtered sim-side to the
viewer's observing bands — a client-side re-filter would be the first place the arc's discipline
broke. **`strength == 0` is a PARKED tie, not an absent one**, so the row is published and the picker
renders it disabled; **`last_seen_{x,y}` is CLOCK 1** — where the subject was, not where they are —
and a consumer that renders it as a live position claims a sighting the tie never granted.

`dict/population.rs` gained ten cohort keys in that arc, in four groups, and two more when hay
became a shipment cargo (issue #590):

| keys | shape |
|---|---|
| `expedition_destination_band` / `expedition_destination_name` | the KEY and its DISPLAY TWIN, the `expedition_target_herd` / `expedition_target_species` rule — the name is resolved at launch and carried, because a party outlives its destination's presence in the viewer's world |
| `expedition_cargo_food` / `expedition_cargo_materials` | the shipment, the materials reusing `MaterialPayoff` — **never summed**, empty means "no row", the key always present |
| `expedition_cargo_fodder` / `expedition_trade_fodder_carry_weight` | the THIRD cargo account and its own pack-space lever (issue #590), appended at the END of the cohort table rather than beside their twins because field order is the append-only contract. Hay and food are two keys that NEVER convert — a decoder or readout that adds them has re-minted the retired trade-goods axis. The lever is FINITE AND >= 0, not positive: `0` legitimately means "hay is weightless" |
| `transfer_received` / `transfer_sent` · `expedition_trade_per_worker_carry` / `expedition_trade_material_carry_weight` | the food-ledger pair, and the two per-cohort numbers the outfit UI prices a manifest with for a party that does not exist yet. **They are not the same KIND of number** (issue #626): the material weight is a config lever echoed verbatim, because what a unit of hide costs in pack space is a property of the GOODS; the carry is the sim's already-RESOLVED answer to *"what does one worker on this shipment carry"*, so a carrier-side model — a cart kit's stat, a tech factor, a road grade — moves the published number and the client's `cap = party_workers × this` needs no edit |
| `transfer_received_turn` / `transfer_sent_turn` | the same two facts taken PER TURN, and the pair a readout renders — the accumulating pair above is cleared once the turn's capture reads it, so it is `0` on every command-refreshed frame |

**`expedition_carry_cap` resolves per MISSION, and that is the trap worth naming**: a raid's pack is
its provisions ceiling, a shipment's is what its people can carry out. They are different numbers on
different levers, so `expedition_per_worker_carry` (the HUNT lever) must never be used to price a
shipment — a client doing so is one config edit from quoting a cap the launch command refuses.

**`expedition_cargo_materials` is a VECTOR field, so it takes the `material_yield` treatment** rather
than an appended scalar's: saturation reaches it and the golden re-record is the only step, but a
consumer that coerces it through `float()` fails loudly and at a distance (see the vector-field note
above). All thirteen keys are in the golden — re-record with `cargo xtask decode-guard --write-golden`
after any intended change here.

**BOTH TRANSFER PAIRS ARE DECODED, AND NEITHER SUBSTITUTES FOR THE OTHER** (issue #517). The
accumulating pair spans the publication window and closes the larder identity between two TURN
frames; the per-turn pair is cohort state a recapture re-reads unchanged, and is what the band
panel's Food breakdown renders. They are equal on a turn's own frame and differ only on a frame a
dispatched command refreshed — which is precisely the frame a panel reading the accumulating pair
renders nothing on, so a decoder that emitted one of them would look correct in a golden and lose the
rows in play.

**AND ISSUE #548 SPLIT THE PER-TURN PAIR BY WHAT CARRIED THE GOODS, EIGHT MORE COHORT KEYS**:
`transfer_local_{received,sent}_turn` / `transfer_route_{received,sent}_turn` and the `fodder_`
prefixed four, all plain `float`s cast `as f64` beside the pair they refine. `local` is the automatic
proximity pooling of a supply network (plus a fission dowry); `route` is an expedition PARTY carrying
it, whatever its errand, which is why a hunt's homecoming is `route` and not a third kind. **The two
are exhaustive** — local + route equals the generic pair in each direction by construction — so a
readout may render both rows and trust that nothing is missing between them.

**FOUR PAIRS SHIP AS FOUR PAIRS; THE DECODER NETS NOTHING.** `DisclosureController` nets each kind
into one signed row, which keeps the gross figures available to any surface that later wants them.
The fodder four are separate keys rather than a shared set because hay and grain cross the same links
on the same turn in different amounts. All eight are appended scalars, so the fixture's saturation
reaches them and a golden re-record is the only step — each takes a distinct value there, which is
what makes a swapped accessor show as a moved line rather than merely a different one.
