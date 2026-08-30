---
paths:
  # Each guard is a SCRIPT AND ITS SCENE. The scene is the entry point every invocation names
  # (`res://tools/<guard>.tscn`), so gating on the `.gd` alone would leave the wiring edit — the one
  # that can silently stop a gate running at all — without this file. The pre-split rule reached
  # them through a `tools/**` glob; these lists are what replaced it.
  - "clients/godot_thin_client/tools/decode_guard.gd"
  - "clients/godot_thin_client/tools/decode_guard.tscn"
  - "clients/godot_thin_client/tools/stream_frame_guard.gd"
  - "clients/godot_thin_client/tools/stream_frame_guard.tscn"
  - "clients/godot_thin_client/tools/party_removal_guard.gd"
  - "clients/godot_thin_client/tools/party_removal_guard.tscn"
  - "clients/godot_thin_client/tools/marker_field_guard.gd"
  - "clients/godot_thin_client/tools/marker_field_guard.tscn"
  - "clients/godot_thin_client/tools/snapshot_alias_guard.gd"
  - "clients/godot_thin_client/tools/snapshot_alias_guard.tscn"
  - "clients/godot_thin_client/tools/inspector_hidden_guard.gd"
  - "clients/godot_thin_client/tools/inspector_hidden_guard.tscn"
  - "clients/godot_thin_client/tools/patch_crossref_guard.gd"
  - "clients/godot_thin_client/tools/patch_crossref_guard.tscn"
  - "clients/godot_thin_client/tests/**"
  # The fixture BUILDER, which is where every claim below about what the golden covers is actually
  # decided — it lives in xtask rather than the client tree, and gating on the golden alone left the
  # one file that can silently narrow this gate's coverage outside the rule that documents it.
  - "xtask/src/decode_fixture.rs"
---

<!-- Split out of .claude/rules/client/test-harnesses.md, which was itself extracted from
     clients/godot_thin_client/CLAUDE.md at blob 20553fb8f9b193b80338a8c06765d511b81b601e.
     The pseudo-table cells this file carries were re-wrapped at 100 columns; no wording changed. -->

# The headless verification guards

The `--headless` gates: no window, no PNGs, a golden or a field contract each.

## The fixtures are GENERATED and NOT committed — the golden is the committed half

`tests/fixtures/*.bin` are the six FlatBuffers envelopes `cargo xtask decode-fixture` writes from
`xtask/src/decode_fixture.rs`. They are **gitignored**
(`clients/godot_thin_client/.gitignore`), and `tests/golden/snapshot_dict.json` is not: **the
assertion is committed, the input is derived.** A fresh checkout or worktree therefore has no
fixtures until something writes them, which is why every guard's missing-file path names the command
rather than reporting a bare read error.

**Nothing was ever testing the committed copies.** `cargo xtask decode-guard` regenerates all six
*before* it launches Godot — step 1 of four — so the bytes under test were always the freshly
written ones. A committed copy could only do two things: go stale (nothing verified it matched what
the generator would produce, because the one gate that could notice overwrote it first) or collide.
Being binary, git cannot merge one, so any two branches that both touched the schema conflicted on
every fixture they both regenerated — 7 such merges before they were untracked.

`cargo xtask decode-guard` needs nothing else. To drive a guard scene directly, write them first:

```bash
cargo xtask decode-fixture     # writes all six from the current schema
godot --headless --path clients/godot_thin_client res://tools/decode_guard.tscn
```

**Do not re-commit one to make a direct run cheaper.** Running any of these guards already requires
the native extension (`SnapshotDecoder` lives in it, and `native/bin/` is gitignored too), so a
checkout that can run the gate at all can run the generator — "standalone" only ever meant "without
the `xtask` wrapper", never "without a Rust toolchain".

## `tools/decode_guard.gd` / `.tscn`

Headless **golden gate for the FlatBuffers → `Dictionary` decode path**
(`SnapshotDecoder.decode_snapshot` → `snapshot_to_dict` → the nine `dict/*` builders) — the coverage
that path simply did not have.

**The gap was invisible because it looked covered:** `ui_preview` and `map_preview` build
hand-written GDScript fixture dicts and hand them straight to `Hud`/`MapView` — neither file so much
as names `SnapshotDecoder` — so a fully green PNG run was compatible with a completely broken
decoder, and the only in-process guard was `dict::population::cohort_decode_tests` (one struct's
fixed-point scale, and it exists precisely because `VarDictionary` cannot be built outside a live
engine). That engine requirement is why this is a Godot scene rather than a `cargo test`. It decodes
the GENERATED fixture envelope (`tests/fixtures/snapshot_envelope.bin`) through the REAL decoder,
canonicalizes the resulting dict and diffs it against `tests/golden/snapshot_dict.json`; exits
non-zero on mismatch (CI-usable). Drive it with `cargo xtask decode-guard` (regenerates the
fixtures, builds the native extension, **imports the project if it never has been**, then runs this)
or, after `cargo xtask decode-fixture` has written them, directly:
`godot --headless --path . res://tools/decode_guard.tscn`.

**That import step is not optional on a fresh checkout or WORKTREE, and its absence lies about the
cause:** Godot loads GDExtensions from `.godot/extension_list.cfg`, which only the import pass
writes and which is a build artifact no worktree starts with — so the guard reported
`SnapshotDecoder class is not registered — build the native extension first` moments after the
runner had built, copied and signed it. `ensure_project_imported` runs `godot --import` when that
file is missing and **judges it by the file, not by the exit code**: Godot 4.7's headless import
CRASHES on shutdown here (signal 11 → SIGABRT) *after* writing a sound cache, so failing on the
status would break the fix on exactly the setup it exists for. It is skipped once the file exists
(the pass takes tens of seconds; this gate is run in a tight loop).

**The fixture is SYNTHETIC, not a server capture, and that is the design**
(`xtask/src/decode_fixture.rs`): a capture is *sparse* — an early-game world carries no crisis
gauges, great discoveries or influencers, so most `dict/*` builders would go
unexercised — and *unstable*, since worldgen is retuned constantly here and a capture-derived golden
would churn on every tuning pass until its readers accepted the diff blind. The synthetic snapshot
instead makes **every section non-empty**, with two rows apiece so a builder that returns row 0 for
every row is visible.

**Every string in it is its own wire path** (`"herds[0].species"`), so the golden reads as a map
from wire field to dictionary key — a mis-wired section accessor is *legible* in the diff, not
merely different. Integers are capped at 200 so a `u8` field cannot fail to deserialize, which is
why a fixed-point `Scalar` reads as e.g. `0.000015`: tiny, but a dropped `fixed64_to_f32` divide
moves it six orders of magnitude, which is what the gate is for (verified — injecting a dropped
divide and a misspelled key both fail it, with both named in the diff).

**⛔ SATURATION SKIPS ENUMS, SO AN ENUM IS ONLY EVER COVERED AT ITS `Default` UNLESS THE FIXTURE
WRITES IT.** The rule that keeps saturation type-safe — *replace a string leaf only when its default
is empty* — is exactly what distinguishes free text from a serde enum, whose serialized form is a
non-empty variant name. That is correct and load-bearing, and it has a consequence worth stating:
**every scalar field is covered automatically forever, and every enum field is covered at one
variant.** A decoder arm for any other variant has no end-to-end coverage at all.

**It is invisible when the default is wire value `0`**, which is the shape a well-designed enum
usually has: a FlatBuffers scalar equal to its default costs no bytes, so appending the field changed
neither the fixture `.bin`s nor the golden, and the gap left no trace to notice. `LaborAssignment`'s
`SourcePriority` was the instance that found it — the client's mapping ends in a `_ => "normal"`
catch-all, so a `High` or `Low` arm wired to the wrong word decoded as `"normal"` and the guard
passed. Measured: with the old all-defaults rows, breaking the codec's `High` mapping produced a
**byte-identical** `snapshot_envelope.bin`, so no golden diff was even possible.

`labor_assignments` is therefore the one repeated section sized by an **enum** rather than by `ROWS`
— one row per `SourcePriority`, built from `EVERY_SOURCE_PRIORITY` so a variant added without a row
here is a variant the guard does not cover. **A new enum on the wire needs the same treatment**, and
adding it to the fixture is the only thing that gives its arms coverage.

**`BenchState.priority` is the same enum in a place that can only reach TWO of its three arms.** A
cohort carries exactly **one** bench, and the fixture has two cohorts, so `BENCHED_SOURCE_PRIORITIES`
gives them `High` and `Low` — the two a wrong mapping can actually reach, because a decoder maps this
enum with a `_ =>` catch-all on the default (the shipped labor mapping is `_ => "normal"`) and a wrong
`Normal` arm is therefore unreachable by construction. The `Normal` arm is covered end to end at the
**codec** level instead, by `core_sim/tests/crafting_wire.rs`, which asserts all three off the encoded
envelope.

> **AND THE FIXTURE ROWS BUY NOTHING UNTIL THE DECODER READS THE FIELD.** Measured: with the bench
> rows in place, breaking the codec's bench `High` mapping left `decode-guard` **passing** and the
> golden unmoved, because `dict/population.rs` has no `priority` key on the bench dict. That is not a
> fault in the fixture — it is what this gate *is*, a guard over the client's decode path — but it
> means "the fixture covers the arm" and "the guard covers the arm" are two claims, and the second
> waits on the accessor. The `crafting_wire` test is what holds the line in the meantime.

**The golden is STRUCTURAL, not byte-exact**: floats round to `FLOAT_DECIMALS` and an over-long
packed array records `{type, len, head, tail, checksum}` rather than every sample, so an appended
field does not rewrite thousands of lines. Re-record only deliberately, with `cargo xtask
decode-guard --write-golden`.

**A SECOND fixture rides beside it, with no golden:
`tests/fixtures/snapshot_headerless_envelope.bin`** — a `WorldSnapshot` carrying a real map section
and **no `header`**. `header` has no `required` attribute in the schema and `root_as_envelope`
verifies table STRUCTURE only, so that parses cleanly and reaches `snapshot_to_dict` with the field
absent (which used to `unwrap()` and take the client down). `_assert_headerless_frame_is_dropped`
decodes it and asserts an **EMPTY dictionary** — the "no frame" value `SnapshotLoader.poll_stream`
already skips — because the alternative reading, decoding it with header DEFAULTS, would publish a
world whose `tick`, `world_epoch` and (worst) `wrap_horizontal` are guesses, and a wrong wrap flag
silently corrupts every seam-crossing hex distance rather than failing anywhere.

**That assertion cannot see a re-introduced `unwrap()`, and the reason is MEASURED, not assumed:**
gdext catches a Rust panic at the FFI boundary and the call still comes back with the method's
DEFAULT — for `decode_snapshot`, the very empty dictionary being asserted — so a green scene run is
compatible with a panicking decoder (restoring the old `unwrap()` printed `PASS`). The missing half
lives one level up:

**`cargo xtask decode-guard` captures the run's output and FAILS on the engine's panic report**
(`PANIC_MARKERS`).

**Match gdext's actual wording — it logs `ERROR: [panic src/…rs:711]` followed by the panic message,
and note that it does NOT contain the word "panicked"**, which is what the first cut of that grep
looked for and silently missed. Two more things make the gate hang-proof rather than merely correct:
Godot **exits 0 through a caught panic**, so the status code can never be the signal, and a failed
call assigned straight into a `: Dictionary` local **aborts the calling function**, so
`get_tree().quit()` never runs and the headless process sits there forever (measured at 23 minutes
before it was killed) — hence `_decode_or_die`, which takes the result as an untyped `Variant`
purely so the run survives to be read.

**A THIRD and FOURTH fixture ride beside those two, both with no golden:
`tests/fixtures/snapshot_delta_envelope.bin` and `snapshot_delta2_envelope.bin`** — two DELTAS built
against the same synthetic world and decoded as a CHAIN (baseline → delta 1 → delta 2), the first
naming the snapshot fixture's `frameSeq` as its `baseFrameSeq` and the second naming DELTA 1's. It
moves `grazeBiomass`/`forageCapacity`/`temperature` on three tiles (plus `riverEdges`/`cultureLayer`
on exactly one of them), `size` on one of two `populations` rows and `parent` on one of two
`cultureLayers` rows — three of the ten keyed sections, chosen because they are the ones with live
consumers reading the BASE key. It has no golden **on purpose**: a merged delta is mostly the
baseline, so recording it would double the golden's surface and make it churn twice on every
unrelated schema edit. `_assert_delta_merges_onto_the_baseline` decodes it after the baseline and
asserts, per section from the `MERGED_SECTIONS` table, that the frame identifies as a delta and
names the baseline's frame (if not, the decoder DROPS it and every later assertion is vacuous); that
every changed row reads the DELTA's value under the BASE key and not the baseline's; that the base
key keeps the baseline's row COUNT; and that `changed_sections` names each moved section plus the
two splatmap concerns the one river/culture tile moved, while NOT naming `forage_patches`, which
this delta leaves alone.

**The delta path had no fixture at all, and that is exactly why nine sections shipped frozen at the
baseline for the life of a world** — mutation-tested in both directions before landing: suppressing
the patched-array publish fails with the staleness named on `tiles`, suppressing it for
`populations` alone fails naming `populations`, and removing the `populations` merge entirely fails
on the missing manifest name. The one-tile-moves-rivers design is what exercises the decoder's
old-vs-new comparison in BOTH directions; dropping it fails on the missing `tiles.rivers` name (also
verified).

**Single-delta coverage is NOT delta-on-delta coverage, and the second fixture was added after the
fact because that was mis-triaged once.** One delta only ever exercises baseline → delta; the client
takes delta → delta on every turn after the first. `decode_frame` merges into
`cache.dict.duplicate_shallow()` where `cache` is replaced after each merge — re-base that on the
ORIGINAL baseline and delta 2 silently discards delta 1, with no error and no symptom (the opposite
failure is loud: a cache that fails to advance `frame_seq` makes delta 2 unapplicable and fires
`resync_needed`). The chained assertions can only see it because the two fixtures move **DISJOINT**
rows — delta 1 takes tiles 0-2 and row 0 of `populations`/`culture_layers`, delta 2 takes tiles 3-5
and row 1 — since a delta 2 that rewrote delta 1's rows would leave losing delta 1 traceless;
`DeltaPlan::overlaps` asserts the disjointness, and the guard re-checks it on the encoded ids.

**Two mutations are needed to cover it, and the second was discovered by the first failing to
fail.** Not advancing the SECTION caches loses the keyed sections and fails on `tiles`; re-basing
the frame DICTIONARY does NOT, because a keyed section's base key is rebuilt out of `SectionCaches`
and republished every frame — that mutation left the guard PASSING. Hence the
`WHOLE_SECTION_WITNESS` (`demographics`, carried by delta 1 and deliberately NOT by delta 2): a
whole-section field lands in the merged dict once and survives only because the next merge starts
from the frame before it, so it is the only witness that can testify about that line.

**A SECOND whole-section witness rides beside it, `equipment_config_json`** (`OPAQUE_WITNESS_*`),
carried by delta 1 and again not by delta 2, and it pins one thing `demographics` cannot: that the
decoder republishes the sim's whole `EquipmentConfig` **OPAQUE**. The Workbench's Equipment and Kits
pages parse that string themselves and walk it blind, so it is asserted by **equality on the WHOLE
string**, never `contains` — the delta's value is a JSON object
(`{"fixture":"delta.equipment_config_json"}`) and its braces and quotes surviving verbatim IS the
"never parsed, never re-serialised, never trimmed" contract, which a decoder that unpacked it into
typed keys and rebuilt it would fail even with every field intact. Its two claims are ordered and
cover different failures: after delta 1 the merged frame must read the DELTA's value and not the
baseline's sentinel (the leg a delta path never wired up fails — `snapshot_to_dict` alone leaves the
boot config standing for the life of the world, the `food_modules` staleness reached one more way),
and after delta 2 it must STILL read delta 1's, which only the frame-before-it merge can produce.
The two fixture values differ on purpose and a precondition asserts the baseline established the
sentinel first, or a decoder that ignored the delta entirely would satisfy both. The delta manifest
is asserted alongside each — named on the frame that carries it, absent on the frame that does not —
because the config pages gate on `SnapshotSections.changed()` for exactly this key, so an unnamed
replacement is one they skip.

**Sabotage-verified on the two mutations the pair exists to separate**: deleting the field's line
from `decode_delta_against` fails claim 1 and names the value as the baseline's, while re-basing the
cache dictionary on the baseline fails claim 2 — reached by muting the older `demographics`
persistence check, which short-circuits first and would otherwise mask it — with claim 1 correctly
green. Its CI-reachable half is `the_delta_fixtures_chain_and_move_disjoint_rows` in
`xtask/src/decode_fixture.rs`, which pins that each delta stays applicable (delta 2 to delta 1's
frame, not the baseline's), stays sparse, keeps an untouched row in every probed section, moves the
splatmap fields on exactly one tile each, and actually moves every field the guard asserts on —
including that the fixture's culture-layer ids and demographics factions stay DISTINCT, since the
merge and the guard both index by them and a collision would silently probe the wrong row.

## `tools/stream_frame_guard.gd` / `.tscn`

Headless **regression guard for `SnapshotLoader.poll_stream`'s frame-supersession rule** — the
one-line optimisation that is always about to come back. `poll_stream` used to keep only the NEWEST
frame a poll delivered, which is free while every frame is a full snapshot (each restates the whole
world) and **silently destructive** once deltas became the steady-state carrier: a dropped delta's
turn of changes is gone forever, nothing later restates it, and the only symptom is a client world
that drifts from the server's while looking merely calm. So the rule is **supersession, not
recency** — a full snapshot supersedes everything before it, a delta supersedes nothing — and this
guard pins all four consequences: full-then-delta applies BOTH (the keep-newest regression),
`delta,FULL,delta` collapses to the full snapshot and what followed it, two fulls in one poll
collapse to one (the case keep-newest got RIGHT, kept so a fix for the others cannot lose it), and a
delta whose `baseFrameSeq` names a frame the client no longer holds is DROPPED and raises
`resync_needed` (§3.3 — merging against the wrong baseline is how the world silently diverges; not
raising the flag leaves the client frozen with nothing asking for a resync).

**Only the transport is faked** — a two-method stub (`poll`/`status`), deliberately not a
`SnapshotStream` subclass, which would drag in a real socket; the payloads are the generated
`snapshot_envelope.bin` + `snapshot_delta_envelope.bin` and the decode is the live
`SnapshotDecoder`, so a decoder that stops accepting the delta fails here too. A fresh loader per
case, since each needs its own decoder baseline.

**Mutation-tested both ways**: restoring keep-only-the-newest fails 3 of the 4 cases naming the
regression, and swallowing the decoder's resync request fails the fourth. `godot --headless --path .
res://tools/stream_frame_guard.tscn`; exits 0/1, CI-usable, no GPU. Write its fixtures first with
`cargo xtask decode-fixture` — they are gitignored, so a fresh checkout has none.

## `tools/party_removal_guard.gd` / `.tscn`

Headless gate for **`removedPopulations` reaching the Band panel** — the client half of the
ghost-party bug. Reported from play: the parties row's red `✕` did nothing, repeatedly, and the feed
answered `Expedition 2 does not exist in the simulation`.

**The recall was never broken** — the sim was correctly refusing a party the CLIENT was still
drawing. A party a `send_hunt_expedition` spawned and an in-camp `recall_expedition` despawned
inside ONE tick was published on a **held** frame, which does not store into the baseline, so
`diff_removed` had nothing to sweep and every later frame carried `populations: []` /
`removedPopulations: []`; the row never healed. The sim sends the removal now, and this guard is
what proves the client acts on it.

**The GDScript never sees that field at all** — the native decoder drops the id out of its cached
array and republishes `populations` whole (`SectionCache::patch`), and every surface is supposed to
rebuild off it. "Supposed to" was the entire state of the evidence: `decode_guard`'s delta
assertions explicitly pin that a merged frame keeps the baseline's row COUNT (*"a delta patches the
world, it never shrinks it"*), so before this the removal branch had **no fixture anywhere in the
repo** and a `patch` that simply ignored `removed` would have passed every gate. The run is *wire →
panel*: baseline envelope → `SnapshotLoader.poll_stream` → **arrival delta** (appends a player band
+ its detached hunting party) → **removal delta** (names the party in `removedPopulations`), with
the real `SnapshotDecoder` in the middle and a real `HudLayer` + `BandCityPanel` + `MapView` at the
end, fanned out the way `Main._apply_snapshot` does (`_apply_frame` keeps Main's `has()` +
`SnapshotSections.changed` PAIR, because a merged delta republishes every key and `has()` alone
stopped being a change signal at #386). Nothing here edits `HudBandLaborState` — the only thing that
ever removes the party is the wire.

**Five claims, and they fail on DISJOINT mutations**: the merged `populations` lost the party and
kept its home band (plus `population_removed` naming it — the wire half, and the only assertion that
can see the decoder); `band_parties` / `player_expeditions` / `band_party_workers` drop it (asked of
the MODEL, since a panel re-rendered from a stale grouping looks right this turn while the Workforce
header's `away` clause and the attention producers still count the ghost); the rendered header falls
from `1 out · 4 workers` to `0 out · 0 workers` (read off the panel's own Labels, never recomputed —
the difference between *the model agrees* and *the player is told*); the row **and the parties
inspector strip** leave the tree with no surviving control naming the quarry and `_party_open_key`
cleared (a strip pinned to a despawned party is the same ghost one level down); and the map marker
plus the SELECTED subject clear, so no drawer is stranded.

**Every one of them has a PRECONDITION on the arrival frame**, because each reads as "absent" both
when the removal worked and when the party never arrived — the strip is opened through the REAL row
press and the party selected through a REAL `handle_hex_click` before the removal lands.

**The two captured controls are re-captured LAST, and that is load-bearing**:
`_toggle_parties_inspector` calls `rerender`, which FREES the controls the press was made on, so a
reference taken before any later interaction is already dangling and asking whether it left the tree
would answer *yes* whatever the removal did (the first cut had exactly that bug, and it PASSED).
`_still_in_tree` / `_find_control_containing`'s `except` are consequently untyped — answering about
an already-freed node is the point, and a `Node`-typed parameter turns it into a script error
instead.

**Sabotage-verified twice, failing disjoint subsets**: making `SectionCache::patch` ignore `removed`
fails **14** assertions and names the surviving row on every surface, while dropping
`MapView.refresh_selection_payload`'s `selected_unit_id = -1` clear fails **exactly one** — the
stranded-drawer claim — which is the demonstration that the four claims cover different failures
rather than all riding one seam. Fixtures: `snapshot_party_delta_envelope.bin` /
`snapshot_party_removal_delta_envelope.bin`, built by `cargo xtask decode-fixture` (ids and shape
under *"The PARTY-REMOVAL pair"* in `xtask/src/decode_fixture.rs`). The rows are APPENDED by a delta
rather than seeded into the baseline snapshot on purpose: the saturated baseline's cohorts carry a
path-hashed `faction` (always ≥ 1), so the client's player-faction filter never sees them and the
golden does not move — and appending rows the baseline never held is itself the `patch` branch a
spawned party takes in play. FoW is forced OFF (`Main._sync_fog_of_war` owns it in the client; the
fixture world's `fogEnabled` saturates to `true`, and a fogged own-faction party would fail the
marker precondition for an unrelated reason). `godot --headless --path .
res://tools/party_removal_guard.tscn`; exits 0/1, CI-usable, no GPU. Its fixtures are gitignored —
write them first with `cargo xtask decode-fixture`.

## `tools/marker_field_guard.gd` / `.tscn`

Headless **regression guard** for the "unit marker drops a panel-consumed field" bug class (twice
hit: `hunt_mode`, then `working_age`/`idle_workers`). Feeds one realistic population entry through
the real `MapView._rebuild_unit_markers` and asserts the produced marker is a superset of
`PANEL_CONSUMED_KEYS` (the keys `Hud._unit_summary_lines` + `_build_allocation_panel` read off
`_selected_unit`) and that the drop-prone fields round-trip (not defaulted). Exits non-zero on
failure (CI-usable). No rendering, so headless: `godot --headless --path .
res://tools/marker_field_guard.tscn`. When the panel starts reading a new marker field, add it to
`PANEL_CONSUMED_KEYS`.

**It guards a SECOND bug class — the NARROWED continuous field** (`FRACTIONAL_ROUND_TRIP_KEYS`): a
field the decoder emits as a float (a fixed-point Scalar through `fixed64_to_f64`, or a `float` wire
field) that the marker copies with `int(...)`. Presence-only checks structurally cannot see it — the
key is there, the value is merely truncated — yet it is live-visible, because the marker IS the
selection payload for a band clicked ON THE MAP. The age brackets are how it was found: they shipped
as fixed-point Scalars, the marker copied them with `int(...)`, 9.29 + 16.54 + 4.64 became 9 + 16 +
4, and with every remainder zeroed the PEOPLE header read **29** beside a top bar reading **30**
until the next snapshot re-resolved it from the raw floats (indefinitely, while paused). Each key in
that dict is fed a deliberately NON-INTEGER value (the dict IS the fixture's value for that key,
merged over `FIXTURE_ENTRY`, so the two cannot drift) and must come back within
`FRACTIONAL_EPSILON`.

**Membership rule: continuous end to end** — integer counts (`size`, `working_age`, `children`,
`elders`, `idle_workers`), entity ids and coordinates are deliberately EXCLUDED, since a fractional
assertion on one would be a false claim. **The age brackets left this list when the wire started
carrying whole people**: they are counts now, covered by the guard's `_expect_int` round-trips
instead, and a fractional assertion on one would today be the false claim this rule forbids.

**`PANEL_CONSUMED_KEYS` IS GONE, replaced by an exhaustive PARTITION** now that `MapView` copies the
cohort structurally: `marker.keys() == entry.keys() ∪ MARKER_STAMPED_KEYS − MARKER_OMITTED_KEYS`,
asserted in BOTH directions — a dropped source key fails, and so does a key the marker invents
without declaring it. A per-key checklist could only ever catch a leak someone had thought to name,
and the class it exists for is the one nobody names (it shipped three times: `hunt_mode`,
`working_age`/`idle_workers`, the Minimal TOE's six). `MARKER_OMITTED_KEYS` is empty and correct
that way; an entry means someone chose to drop a field the cohort had, with a reason, and a stale
excuse for a key the source never carried also fails.

**Its power is exactly the fixture's key set** — a field absent from `FIXTURE_ENTRY` is a field the
partition says nothing about — so the PASS line prints the source key count (`59 keys + 5 declared
stamps`), a partition over an empty source being vacuously true and otherwise indistinguishable.

**The fixture carries `pending_reveal_count`** (the decoder's `pendingReveal{X,Y}` projection): the
Occupants drawer's Recall/Cancel button reads it OFF THE MARKER, `_selected_unit` being the
map-click payload, and the structural copy carries it with no `MARKER_OMITTED_KEYS` entry.
`FRACTIONAL_ROUND_TRIP_KEYS` STAYS: it covers a different failure (a value narrowing in transit),
which the duplicate makes impossible for a plain copy but not for any field a later hand-stamp
touches. Sabotage-verified in the new shape: `marker.erase("hunter_attack")` fails with `marker
DROPPED source key`, and re-introducing a five-key allowlist fails 71 times naming every one.

## `tools/patch_crossref_guard.gd` / `.tscn`

Headless **regression guard for the "a decoded forage-patch field never reaches `tile_info`" bug
class** — the plant web's second wiring, and the third time it shipped.

**A herd dict travels whole; a forage patch does not.** `MapView._tile_info_at` copies the
`forage_patches` row across key by key from an explicit list, `patch_`-prefixing each, and every
forage compose sheet reads its source out of that `tile_info`. So a field the decoder emits and that
list omits is silently absent on the plant web and fine on the animal one — first
`perWorkerBiomass`/`regrowthSamples` (which removed the harvest-floor chart and both crew targets from
every patch against a live sim), then `materialPerBiomass`/`perWorkerMaterial` (a tile 56% tobacco
whose PER TURN box named the fodder and never the tobacco).

**Both were structurally invisible to `ui_preview` and `band_panel_preview`**: their fixtures seed
`tile_info` themselves, so no frame in either harness exercises the cross-ref, and a second seeded
frame never could. That is why this is a guard over the REAL seam rather than another preview state.

The run is *wire → `tile_info`*: the generated `snapshot_envelope.bin` → the real `SnapshotDecoder` →
`_ingest_forage_patches` → `_tile_info_at` on a detached `MapView` (the `snapshot_alias_guard` idiom),
with nothing hand-written in between. **Taking the raw patch from the DECODER is the whole design** —
a literal fixture only carries the keys someone remembered to add, which is precisely the failure.
Five claims:

1. **the partition, forwards** — every wire key arrives as `patch_<key>` unless declared in
   `UNCROSSED_KEYS` with a reason (the coordinates, which are the lookup key, and the three retired
   `*_trade` slots arc #527 left on the wire);
2. **the partition, backwards** — every `patch_`-prefixed key has a source key, so a misspelling has
   nowhere to hide;
3. **the value round-trips** by equality, which is what sees a narrowing `int(...)` copy or a vector
   that lost its rows — a presence check structurally cannot;
4. **the consumer pincer** — every `FORECAST_*_KEY` / `FORECAST_*_KEYS` name `SourceForecast` declares
   (read reflectively off the script's constant map, so the list cannot drift) must be crossed if the
   patch publishes it, which is what stops an `UNCROSSED_KEYS` entry being added for a field the
   forecast layer reads;
5. **the FoW redaction** — a crossed key belongs in `FOW_DISCOVERED_HIDDEN_KEYS` unless declared in
   `FOW_EXEMPT_KEYS` as ground knowledge (the capacity, the basket, the committed species). That is
   the THIRD wiring, and it fails as silently as the second, on a hex you cannot currently see.

**Reflection over a `class_name`d script has one trap and it is a HANG, not an error.**
`get_script_constant_map()` is an instance method on `Script`, so calling it on a `preload`ed const
makes the compiler resolve the name to the CLASS and refuse it — *"Cannot call non-static function …
directly"* — which is a load failure, so the scene comes up scriptless, `get_tree().quit()` never
runs and the headless process idles forever (measured). Hence `SOURCE_FORECAST_PATH` + a
`Script`-typed local.

**Mutation-tested three ways, failing disjoint subsets**: dropping the two material cross-ref lines
fails 4 (claims 1 and 4, naming both keys); narrowing `per_worker_biomass` with `int(...)` fails claim
3 alone, naming `110.809997558594 → 110`; and removing the two material entries from
`FOW_DISCOVERED_HIDDEN_KEYS` fails claim 5 alone. Its fixture is gitignored — write it first with
`cargo xtask decode-fixture`. `godot --headless --path . res://tools/patch_crossref_guard.tscn`; exits
0/1, CI-usable, no GPU. A clean run reads `59 wire keys cross onto tile_info intact (5 declared
uncrossed)` — the count is printed for the reason `marker_field_guard` prints its own, a partition
over an empty source being vacuously true and otherwise indistinguishable. **The figure is a date
stamp, not a constant**: it moves with every appended `ForagePatchState` field, so re-measure rather
than trusting it.

### The material half was the FIFTH time, and it was found by this guard rather than in play

The guard shipped red: the material-half-of-upkeep arc appended seven fields the decoder emits and
`_tile_info_at` never copied — `build_material_cost`, `upkeep_material_demand`,
`upkeep_material_supplied`, `cultivation_upkeep_material_demand`, `field_upkeep_material_demand`,
`upkeep_kit_id`, `upkeep_kit_named` — twelve problems over claims 1 and 4. **The renderers had all
been built**, which is the whole shape of this bug class: `DetailFormat.build_blocked_lines` composes
its stuck-on-materials sentence FROM the pile so it can name the good that ran out, and with
`patch_build_material_cost` absent every such refusal on the plant web fell back to
`BUILD_BLOCKED_MATERIALS_UNNAMED` — the client's own *"we cannot say which good"*, shipped on a patch
where the wire had said exactly which. `DetailFormat.rung_material_is_short` answered `false` on every
patch in the game for the same reason, so a tended rung whose goods had run out wore no `⚠` and no
state word while the work board's row said in DANGER ink that the source was being lost.

**All seven are crossed; five are redacted and two are exempt.** The redacted five are live state — a
bill struck this turn, a store drawn down this turn, a kit resolved onto a work site this turn, and
`build_material_cost`, which prices the rung DIRECTLY ABOVE where the patch stands and so states the
ladder position `patch_current_rung` is redacted to hide.

⛔ **`patch_cultivation_upkeep_material_demand` / `patch_field_upkeep_material_demand` are in
`FOW_EXEMPT_KEYS`, beside the work twins they are the other currency of.** Both plant rungs are
`scaled_by: source_load`, so the sim strikes each rate through the patch's tender-load —
`tile_capacity / capacity_per_tender`, a pure function of TERRAIN a Discovered tile knows by
definition — before shipping it. They carry no rung, so the figure sent for an unseen hex is the one
that hex last showed, which is word-for-word `patch_cultivation_upkeep_demand`'s own exemption.
**Splitting the pair would be worse than either whole answer**: `RungLadder._price_terms` composes ONE
clause from the work rate and the goods, so a redacted material half has a remembered hex quoting a
PARTIAL price as though it were the whole one.

**Green here is necessary and not sufficient, and the shipped config is why.** No plant rung declares
a material, so all five material fields are structurally `[]` on every shipped patch and a key that
arrives empty renders exactly what a key that never arrived renders — nothing. The guard proves the
keys reach `tile_info`; `ui_preview`'s authored plant material upkeep proves the readout states
something when there is something to state (`harness-ui-preview.md`).

## `tools/snapshot_alias_guard.gd` / `.tscn`

Headless **regression guard** for the "MapView writes into the decoder's cached world" bug class,
and for the deep copies that used to make it impossible (#389, ~42 ms/turn). Drives `MapView`'s five
snapshot-ingest seams (`_ingest_culture_layers` / `_ingest_food_modules` /
`_ingest_discovered_sites` / `_ingest_forage_patches` / `_ingest_population_sites`) on a DETACHED
`MapView` — no tree, no rendering, the `marker_field_guard` idiom — over one fixture frame whose
rows all sit on one tile, then asserts **both directions at once**: (a) the frame's own rows come
back unmutated (no `terrain_id` on the `food_modules` row, no `module_label` on the cohort's
`harvest`, key counts unchanged), and (b) everything not stamped is the *same object* the frame
carried — `is_same()`, the only test that can see a `duplicate()` creeping back, since an
equal-valued copy passes every value assertion. The two pull against each other on purpose: dropping
the shallow copy fails (a), re-adding a deep one fails (b).

**Mutation-tested in both directions** — re-adding `duplicate(true)` to the forage ingest fails on 2
assertions, dropping the food row's shallow `duplicate()` fails on 3. Seat
`grid_width`/`grid_height`/`terrain_overlay` when adding a case that asserts a stamped value, or
`_terrain_id_at` answers `-1` for every coordinate and the assertion proves nothing. `godot
--headless --path . res://tools/snapshot_alias_guard.tscn`; exits non-zero on failure (CI-usable).

## `tools/inspector_hidden_guard.gd` / `.tscn`

Headless **regression guard** for the hidden-Inspector skip (see `inspector-panels.md` → "A hidden
Inspector does not render"). Instances the real `InspectorLayer.tscn` and asserts, using
`InfluencerPanel.get_influencers()` (rebuilt wholesale from a frame's `influencers` key, public) and
**`_last_turn`** as witnesses: the `_tab_panels` fan-out is **skipped** while hidden — for a DELTA
as well as a full snapshot, since delta streaming made every merged frame a complete world; the
CHEAP PREFIX above the visibility gate **still runs** for both frame kinds; and showing the panel
catches it up to the **latest** frame, not the first one seen while hidden.

**The accumulator witness changed with issue #272.** It used to be `_seen_command_events`, and the
claim it carried was that `_ingest_command_events` kept running while hidden because a dropped
per-turn event is unrecoverable — but that stream is the event dock's now (`Main` feeds
`EventDockPanel.ingest_events` directly), so the member the assertions named no longer exists and
the property it described is no longer this file's. The Inspector holds **no accumulator at all**;
what still must run above the gate is the prefix that writes `_cached_snapshot` and `_last_turn`,
which is what makes the catch-up possible. `_last_turn` is the right witness precisely because it is
written above the gate and read nowhere else on this path — mutation-verified by gating its write on
`_panel_visible`, which fails two of the three prefix assertions naming the turn it found. The two
assertions that only ever restated the de-dup ("the replay did not double-log") are retired rather
than repointed: with nothing accumulating there is nothing to double-log.

**Case 5 carries the arc's history.** Its `DELTA_WHILE_HIDDEN` fixture used to be a PARTIAL dict
(`influencer_updates`, no `influencers`) and the case asserted the opposite — that a hidden delta is
applied IN FULL, because a partial frame cannot be reconstructed. The fixture is now a MERGED
COMPLETE frame carrying the wholesale roster *and* the sparse `*_updates` key, exactly as the
decoder emits; a partial frame is still unsafe to cache and replay, it is simply no longer
producible, and the fixture says so. Its three roster sizes (3 / 5 / 6) are chosen so "catch-up
never ran", "catch-up replayed the OLDER frame" and "correct" are each distinguishable, which is
strictly more than the two-size version caught. Mutation-tested four ways — reverting the
`_cached_snapshot` write to full-snapshots-only, reverting the delta half of the skip, replaying an
older frame, and re-introducing the original bug (a hidden delta discharging
`_hidden_snapshot_pending`) — each producing a distinct, self-explaining failure. Exits non-zero on
failure: `godot --headless --path . res://tools/inspector_hidden_guard.tscn`.

**Why it must exist:** the failure is invisible in normal play — a stale-when-opened Inspector is
indistinguishable from one that simply hasn't ticked yet — so nothing else catches a refactor that
moves a line above or below the gate.
