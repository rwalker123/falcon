---
paths:
  - "core_sim/src/{flora_config,forage,food}.rs"
  - "core_sim/src/data/flora_config.json"
  - "core_sim/tests/flora_*.rs"
---

<!-- Extracted verbatim from lines 45-45;2199-2553 of core_sim/CLAUDE.md at blob dcc757587f8c9308590997ee600abc64a34e6712
     (the PRE-SPLIT original — read it with `git cat-file blob dcc757587f8c9308590997ee600abc64a34e6712`;
     core_sim/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Depletable forage and the flora roster

## Config files

| File | Purpose |
|------|---------|
| `src/data/flora_config.json` | **The flora roster** (Flora Roster F1, `flora_config.rs`, env override **`FLORA_CONFIG_PATH`**; design `docs/plan_flora_roster.md`) — the plant twin of `fauna_config.json`'s species table, and the first time a plant has a **name**. **33 species** (18 F1–F4 families + the **F5 fine-grained mass-fill** of 15 — kelp, sea_kale, wild_rice, cattail, chestnut, wild_orchard, sunflower, wild_pulses, mesquite, wild_fig, cloudberry, rock_tripe, alpine_herbs, cave_fungi, grapevine — so every non-zero biome now carries a **3–5 species basket** and per-tile realization (§10) has enough breadth to vary tile-to-tile). The F1 core is 12 biome-keyed staples + `river_fish`, which alone hosts `NavigableRiver` and means the *fishery bonus term*, not the vestigial capacity row; each: `display_name`/`plural`/`adjective`, **`role`** (`staple`\|`fodder`\|`cash` — a **DISPLAY TAG ONLY**, derived from which component of the yield vector dominates, never branched on in the sim), **`cultivation_ceiling`** (`wild`\|`tended`\|`field`, default `field` — the exact twin of `husbandry_ceiling`: one ceiling, not two flags, so "sowable but not tendable" is unrepresentable; `allows_cultivate`/`allows_sow` are **LIVE since S1** — they gate which species a `Cultivate`/`Sow` may commit a patch to, so a basket that is all-`wild` cannot be tended or sown at all), **`host_biomes`** (`TerrainType` → a **relative affinity WEIGHT, not a capacity**), **`yield`** (`provisions_per_biomass` / `fodder_per_biomass` / `materials[]` — one vector; **the two rates are LIVE AT EVERY RUNG since #433** — each patch's rate is the share-weighted average of its own basket's vectors, so a *wild* tile's food and fodder depend on what is actually growing there; `provisions_per_biomass` went live at S1 as a *committed* patch's conversion rate, `fodder_per_biomass` at F3 on hay Fields. **`trade_goods_per_biomass` is RETIRED (arc #527)** — written by every take site, read by none — and a cash crop is paid in `materials` instead; see "Cash crops — the F4 coupling"), and `regrowth_rate`. **NAMING DECOMPOSES, IT DOES NOT ADD:** `FloraConfig::composition(terrain)` is a **derived, precomputed** share table — `share = weight / Σ weights hosting that biome` — so the shares sum to `1.0` by construction and `share × forage.capacity_by_biome[biome]` always re-sums to the biome's own capacity. Adding a species **dilutes** the others; it cannot inflate a tile. Built once at load through the single constructor every `Deserialize` path routes through, so a stale table is unrepresentable, and sorted **share DESC then species key ASC** because it goes on the wire (`ForagePatchState.composition`). **The share DENOMINATOR must be summed in a deterministic order, not merely presented in one** — `HashMap` iteration order is randomized per instance and f32 addition is not associative, so a `Σ weights` accumulated in map order lands a ULP apart between two builds, and that ULP divides into every published share and changes the snapshot hash. Both share tables therefore order *before* they sum: `build_composition` sorts its rows first, and `navigable_composition` merges through a `BTreeMap`. Sorting the *output* does not fix the arithmetic — this was a ~50%-of-runs `deterministic_snapshots_match` flake, pinned now by `the_share_table_is_bit_identical_across_builds` / `navigable_composition_is_bit_identical_across_calls`. Navigable hexes blend two baskets — see `FloraConfig::navigable_composition` / `forage::tile_flora_composition`, the twin of `navigable_forage_capacity`. **`provisions_per_biomass` is hand-tuned per species and LIVE since S1** — it is the *conversion* half of the commit trade (see "Committing a patch to one plant"), so the F1 "flat 0.05 verbatim" rule is deliberately gone. Grains/tubers on their best ground are strongest (wild_emmer **0.080**); the `wild`-ceiling gathered things sit at or below the 0.05 baseline (shellfish/river fish 0.050, pine nut 0.048, oak mast 0.045, arctic greens 0.040). **Those `wild`-ceiling rates stopped being inert at #433** — they can still never be *committed*, but they are now weighted into their tile's wild basket average, so an oak-mast-heavy realization genuinely pays less per biomass than an emmer-heavy one and the numbers have to be right. `fodder_per_biomass` is **live since F3** on the one fodder crop **hay_grass** (0.20 — a hay Field harvests into the band's `FODDER` store, a pen that knows **Foddering** draws it) and 0.0 on every staple; the five **cash crops** **cotton** / **flax** / **tobacco** / **tea** / **grapevine** read `0` in both scalar accounts and are paid entirely by their `materials` rows (cotton/flax fibre; the uncrafted `tobacco`/`tea`/`grape` since arc #527); `regrowth_rate` is still `forage.ecology.regrowth_rate` verbatim — all pinned by `core_sim/tests/flora_roster.rs` **against the loaded labor config, not literals**, along with the design's own bar, **reframed at #433**: what must differ is the *crop choice* — a species must pay materially better on its best country than on its worst, and materially better than favoring a marginal neighbour on the same tile. The older form ("committing must sometimes lose to leaving it wild") is retired: with the rung-2 conversion gain, any commitment with a real share pays, and the rung-2 decision is which currency plus whether the 25-turn build is worth it. The cash crops are hosted **honestly on the river valleys** (cotton/tobacco/flax on AlluvialPlain/Floodplain/RiverDelta, tea on the uplands); **per-tile realization (§10) keeps the staples dominant on their own realized tiles** — the commit bar reads a tile's local realized share, not the uniform biome share — rather than keeping cash crops off that ground (the S1 commit-worthiness bar). `reed_and_root` is `field`-ceiling (rice/taro on a delta are the archetypal field crop; at `tended` the richest sowable ground in the game would be unsowable). **Validated** — `FloraConfig::validate()` runs inside `from_json_str` (every load path) for the per-row invariants (empty `display_name`, empty `host_biomes`, a non-positive weight, a `yield` that pays into no account at all — every scalar zero **and** no material rows, which is what makes `pays_something` the guard that catches a species silently ceasing to pay — a non-positive `regrowth_rate`), and the **cross-web** pair `FloraConfig::validate_against_forage(&forage.capacity_by_biome)` runs on the load path with `labor_config`'s table passed in (one copy): **no nameless food** — a biome with non-zero forage capacity that no species hosts is rejected, which is what forces a **complete** roster rather than a couple of species — and **no claiming barren ground** — a species hosting a stated-zero biome is rejected. A broken invariant is logged at **error** level (`flora_config.invalid_rejected`) and the builtin is used |
## Depletable Forage (Intensification §0-ii)

Forage tiles are **depletable**, the herd biomass/regrowth model transposed onto plants (design:
`docs/plan_intensification.md` §0). Every `FoodModuleTag` tile carries a live per-patch
`{ biomass, carrying_capacity, ecology_phase }` (`ForagePatch`, `forage.rs`) held in the
authoritative **`ForageRegistry`** resource, keyed by tile coord. Foraging now **draws the stock
down** and the patch **regrows**, so the yield instrument's overdraw ⚠ (PR #110) lights up for
forage exactly as it does for overhunting. *Sim-only — the client already renders forage
`sustainable_yield` from the snapshot.*

- **Seeding** (`spawn_initial_forage`, Startup after `spawn_initial_herds`): one full patch
  (`biomass = carrying_capacity`) per `FoodModuleTag` tile, at **that tile's biome capacity** —
  `forage.capacity_by_biome[terrain]`, the human food web's per-biome table (see "The two food webs"),
  never a global constant. A food-module tile whose biome carries **nothing human-edible** (a stated
  `0` — glacier, salt pan, deep-sea vent field; the module classifier tags these off their *tags*, not
  off anything growing there) is seeded **no patch at all**, exactly as a zero-graze tile holds no
  `GrazePatch`: "no food here" is an *absent* reading, never a zero one. Idempotent (a restored world
  is skipped).
- **Regrowth** (`advance_forage_regrowth`, `TurnStage::Logistics` alongside `advance_herds`): each
  patch regrows toward its cap and refreshes its `EcologyPhase`. Unlike a wild herd, a patch uses
  **pure `logistic_regrowth`** (no Allee / critical-depensation crash) and **never despawns** —
  plants reseed, so a depleted (feral) patch always recovers. Because `logistic_regrowth` is `0` at
  `biomass = 0`, `regrow_patch` first applies a **reseed floor** — it lifts a depleted patch up to
  `reseed_floor_fraction × carrying_capacity` (a small standing crop, `max()` so a healthy patch is
  untouched) *before* regrowth — so a patch driven to exactly `0` (repeated Eradicate + f32
  underflow, `take_fraction = 1.0`, or a restored snapshot carrying `biomass = 0`) still has a seed
  stock and recovers via normal regrowth instead of sticking at `0` forever. The floor is below
  `collapse_fraction`, so Eradicate still crashes a patch hard into the Collapsing band — it just
  can't hold it permanently at `0`.
- **Draw-down** (`forage_take`, the plant mirror of `hunt_take`): resolves the stance's **escapement
  ceiling** (`forage_escapement_ceiling`), caps it by gather throughput
  (`workers × per_worker_biomass_capacity × seasonal_weight`, where the per-worker term is the band's
  **resolved basket tier** — see `equipment.md`), clamps to the patch's biomass,
  **subtracts the take**, and converts to provisions
  (`take × provisions_per_biomass × output_multiplier`).
  **Since `docs/plan_harvest_floor.md` slice 1 the whole axis is ONE expression parameterised by a
  floor** — `max(0, B − floor·K) × build_dip`, the exact twin of `fauna::hunt_escapement_ceiling` —
  with the floor carried on the assignment (`LaborTarget::Forage`) as an `f32` fraction of `K`.
  A deeper floor leaves less standing and so takes more, **with no markup of any kind** — the 4×
  `market.trade_goods_multiplier` is deleted, because a factor attached to one drawdown depth
  re-welded product to intensity (`docs/plan_harvest_floor.md` §4), and the trade axis it multiplied
  went with arc #527.
  **It is `r`-INDEPENDENT and takes no `EcologyConfig`**, which is what makes the rung-2 payoff read
  as what it is: a tended patch does not get a bigger ceiling, it *refills faster*, so it has more
  stock standing above the floor next turn.
  The `Forage` arm of `advance_labor_allocation` (Population) writes the real
  `sustainable = sustainable_yield(biomass_before, cap, patch_ecology) × provisions_per_biomass ×
  output_multiplier` (MSY-based) into the yield telemetry as the **long-run reference line the player
  reads beside the take** — **not** as the ⚠ predicate. The first harvest of a stocked patch is its
  accumulated stock and legitimately exceeds one turn's regrowth under *every* stance, Sustain
  included, so the over-forage ⚠ is `components::take_overdraws` — the floor below the food peak
  (`floor < K/2`, a fact about where the crew stops) **and** the gatherers able to draw the stand down
  to it (`forage::forage_take_overdraws`), exactly as it is on the animal web. See
  `.claude/rules/core_sim/yield-forecast.md` → "THE ⚠ IS INTENT **AND** ABILITY".
- **Config** (`labor_config.json` `forage`): **`capacity_by_biome`** (the per-biome capacity table —
  see "The two food webs"; **validated total** over every `TerrainType` by `LaborConfig::validate`),
  `per_worker_biomass_capacity`,
  `provisions_per_biomass`, an `ecology` block reusing fauna's `EcologyConfig` (`regrowth_rate` tuned
  higher than fauna's 0.05; `collapse_fraction`/`stressed_fraction` phase bands), a
  `reseed_floor_fraction` (0.02 — the reseed standing crop as a fraction of `carrying_capacity`, so a
  crashed patch recovers from a seed stock rather than sticking at `0`; below `collapse_fraction`),
  and a `cultivation` block. **The whole per-stance lever set is DELETED** — `surplus_multiplier`,
  `market` (entirely, including its 4× `trade_goods_multiplier`) and `eradicate.take_fraction` all
  existed to tune four fixed rates, and the harvest floor replaced those with one number the player
  carries (`docs/plan_harvest_floor.md` §4). **After that deletion no option carries a factor of any
  kind**, which is the section's own acceptance test
  (`harvest_floor_trade_rebalance::the_deleted_levers_are_gone_and_the_allee_threshold_is_not`).
  **`ecology.collapse_fraction` stays**: it is the Allee threshold `net_biomass_delta` reads, and it
  only ever moonlighted as one stance's floor. The old flat `forage.per_worker_yield` lever is **retired**,
  and so is the flat `forage.carrying_capacity` (120 on every food-module tile) it was replaced by:
  a **constant** human web could not diverge from the spatial animal one, so *"your best farm is not
  your best pasture"* was untrue **by construction**. Per-biome (not per-`FoodModule`) is deliberate —
  the two tables must be comparable tile-for-tile and must be able to disagree *within* a module.
  Because every yield is linear in `K`, the cultivation incentive and every escapement ceiling scale
  with the tile and need no re-derivation.
- **Floor plumbing** (the 5-site mirror of Hunt's, `docs/plan_harvest_floor.md` §4):
  `LaborTarget::Forage` carries a **`floor: f32`** — a fraction of the patch's `K`, and the whole of
  what the player decides about pressure. A floor change on the same tile is the **same source** in
  `same_source` (a mutable property, exactly as the stance it replaced was); the
  `assign_labor forage <x> <y> [floor] [species] <workers>` command-text parse takes an optional
  floor token; `handle_assign_labor` **validates it fails-closed** (`floor_is_valid` — finite and in
  `0.0..=1.0`, rejected with a command failure, never clamped) and defaults an absent one to
  `DEFAULT_ESCAPEMENT_FLOOR` (0.5, the food peak); and it rides the rollback because `SimState`
  clones `LaborAllocation` whole (`harvest_floor_rollback.rs`).

  **The forage tail is disambiguated by "does the token parse as `f32`"** — a lone optional token is
  the floor if it is a number and the **species** otherwise. That is sound rather than heuristic
  because a `flora_config.json` species key is snake_case and cannot parse as a float, which
  `flora_roster::every_shipped_species_key_is_covered_by_the_command_grammar` asserts against the
  shipped roster. With **both** optional tokens present the first is unambiguously the floor.

  On the wire the assignment ships **`floor`** (`LaborAssignment.floor`, appended) beside a
  four-value **`policy`** label, which the sim writes only when the floor is exactly one of the
  values those names stand for and leaves `""` otherwise — a label is true of the assignment or
  absent, never rounded. Read the floor.
- **Persistence** — `ForageRegistry` survives a rollback exactly like the `HerdRegistry`: the
  **checkpoint carries the registry whole** (`SimState::forage`), including the `progress`/`owner`
  fields that hold **cultivation** (Phase 1a, below), so a mutate-then-restore rewinds it like
  biomass. It reached that state by way of a per-tile `ForageState` mirror captured coord-sorted
  onto `WorldSnapshot.forage_registry`; that copy was deleted once the checkpoint left it without a
  reader (`checkpoints.md`), and the `ForageState` record plus `ForageRegistry::{from_states,
  update_from_states}` followed it, having nothing left to decode. Never wired to the FlatBuffers
  client stream.
- **Companion client slice:** the sim side of the forage policy axis (§0-iii) is complete — the
  client `%ForageAssignControls` policy picker (mirroring `%HerdAssignControls`) that emits the
  policy in the `assign_labor forage` command is a **client-dev follow-up**. A client patch-ecology
  readout (thriving/stressed/collapsing on the map/tile, like herds) is a possible later slice.

### The Flora Roster (F1) — what a tile's capacity is MADE OF

Fauna is a named roster; **plants had no identity at all** — `forage.capacity_by_biome` said *how
much*, `FoodModule` said *what kind of gathering*, and neither said *which plant*. `flora_config.json`
+ `flora_config.rs` name them (design: `docs/plan_flora_roster.md`; the config row above carries the
full record shape and every validated invariant).

> **The ruling: naming DECOMPOSES, it does not add.** A roster entry says what a tile's *existing*
> capacity is made of; it never adds capacity on top. `MixedWoodland`'s 190 becomes named shares
> (hazel + oak mast + berry) that still sum to 190 — and they do so **by construction**, because the
> shares are the affinity weights *normalized*, not authored. You cannot retune `host_biomes` into a
> capacity change; the only thing a weight edit can do is move share **between** the named plants of
> one biome. Retuning the human food web stays a `labor_config.json` edit.

Keyed on **`TerrainType`** (the 38 biomes), not on `FoodModule` (the 10 buckets): fauna keys
`host_biomes` off the module because an animal *ranges* over a region, but a plant **is** its tile,
and the buckets cannot say "this wants floodplain silt, not any wetland". `FoodModule` is untouched.

**F1 changes no behaviour and is not merely claimed to.** Every species carries today's flat values
verbatim, so today's economy is the degenerate case (one implicit species with exactly that vector),
and `core_sim/tests/flora_roster.rs` pins the neutrality **against the loaded `labor_config`, never a
literal**: every biome is either fully named (shares sum to 1) or carries no forage (empty list);
`Σ share × capacity == capacity` on every food-bearing biome; and each row's `yield`/`regrowth_rate`
equals the forage config's own numbers. Nothing reads the yield vector or the cultivation ceiling yet
— F1 ships the shape and looks at a real map first (graze 2a's discipline).

**A navigable hex has TWO capacity terms, so it has two baskets.** `tile_forage_capacity` on a
`NavigableRiver` hex is `capacity_for(underlying) + navigable_river_forage_bonus` (80) — the
`capacity_by_biome` `NavigableRiver` row is *vestigial and bypassed*. Decomposing only the underlying
biome would therefore leave the whole **80-unit fishery unnamed** on every navigable tile — exactly
the nameless food `validate_against_forage` forbids, leaking in through a path that validator cannot
see, and `Σ share × capacity != capacity` there. So **`river_fish` ("River Fish & Mussels") hosts
`NavigableRiver` alone** and means precisely the bonus term — *what the channel itself yields*, as
distinct from the valley it cut — and `FloraConfig::navigable_composition` blends the two baskets in
the same two-term shape as `navigable_forage_capacity` (each weighted by **its own** term, duplicate
species keys **merged**, then renormalized and sorted by the same total order). **`forage::tile_flora_composition`
is THE seam every caller reads** — the twin of `tile_forage_capacity`, branching on the same
condition, so the capacity and its decomposition can never disagree about a tile's shape. It now also
**realizes per tile** (see "Per-tile realization" below), so it takes `map_seed`. Never call
`FloraConfig::composition` on a raw terrain from a sim/snapshot path.

**On the wire** (append-only): `ForagePatchState.composition:[FloraShareInfo{ species, displayName,
share, canCultivate, canSow, cultivateYieldRatio, sowYieldRatio }]`, resolved through
`tile_flora_composition`. The last four are S1's crop-picker payload — see "Committing a patch to one
plant". **Derived from the tile's biome AND its coordinate, not per-patch state**: since the F4/§10
realization arc a tile publishes its **realized subset** (what is *growing* here — 2–4 species keyed on
`(map_seed, tile)`), so **two tiles of one biome publish different baskets**; the uniform *affinity*
table (`FloraConfig::composition`, what *can* grow here) is now one input to that, not the wire value.
No `ForagePatch` field feeds it. **Client follow-up:** nothing renders it — the tile-card composition
readout is a client-dev slice, and so is exposing the per-patch **commitment** (S1, below), which is
sim/rollback state only today.

### Per-tile realization — what *can* grow vs what *is* growing (Flora Roster §10)

`FloraConfig::composition(terrain)` is the **affinity** roster — *what CAN grow on this biome* —
uniform across every tile of a biome. `FloraConfig::realized_composition(terrain, tile, map_seed)` (and
its navigable twin `realized_navigable_composition`) is the per-tile **realization** — *what IS
growing here* — a **seeded weighted subset**: for tile `(x, y)` under `map_seed` it draws
`k ∈ [realized_species_min, realized_species_max]` species (clamped to how many the biome hosts) by
**Efraimidis–Spirakis** weighted sampling without replacement (probability ∝ affinity share), then
**renormalizes** the picked shares to sum to `1`. So some alluvial tiles are wheat, others tobacco,
instead of every tile carrying a diluted slice of all of them.

- **`tile_flora_composition` is still THE seam** and realizes inside itself — the non-navigable arm
  realizes the biome basket; the navigable arm realizes the **underlying valley** and blends the
  **un-realized** channel term (`river_fish`) back in (a giant river is always a fishery). Every
  non-Sow-from-nothing caller reads the realized basket: display, wild gather, Cultivate, Sow-**upgrade**,
  and the wire `ForagePatchState.composition`. **`Sow`-from-nothing** (creating a patch on bare ground —
  which does not occur on a generated map, every food-bearing tile already carries a patch) reads the
  **affinity** roster, since there is no realized basket; the fuller "Sow reads affinity everywhere"
  model is deferred (§11).
- **Pure and derived, so it costs nothing.** Realization is a pure function of
  `(map_seed, tile, terrain, affinities)` — no stored state, no RNG stream, `splitmix64` +
  FNV-of-coord for entropy — so it is **deterministic under rollback for free** and adds **nothing** to
  the snapshot/wire (the realized subset simply *is* the published `composition`). Determinism
  discipline: the affinity input is already total-ordered, ties break by species key ascending, no
  `HashMap` iteration order reaches the result. Pinned bit-exact by
  `core_sim/tests/flora_realization.rs`.
- **Per-tile neutrality — realization moves nothing at the wild rung.** The picked shares renormalize
  to `1`, so `Σ share × capacity == capacity` on every tile: the tile still yields its full biome
  capacity gathered wild, just composed of different species. The economy-neutrality F1 proved for the
  uniform basket, now per tile.
- **This dissolved the F4 cash-crop dilution bind.** The rung-2 commit bar reads a tile's **local
  realized share**, not the uniform biome share, so cotton/tobacco/flax now host AlluvialPlain/Floodplain
  *honestly* — some alluvial tiles realize as wheat (worth tending) and others as cotton — without
  eroding wheat on every alluvial tile the way a global % table did. `every_climbing_species...` and the
  `flora_commitment.rs` commit-trade tests are reframed around the realized share (best realization
  beats wild; a tile is not always the same crop). Config dials `realized_species_min` (2) /
  `realized_species_max` (4) in `flora_config.json`, validated `1 <= min <= max`.
- **Client:** no schema change (the picker already renders `composition`, now varying tile to tile), so
  it "just works" — but the ui_preview crop-picker fixture needs **two tiles of one biome** to *show*
  the variance. Flag for the client half.

### Committing a patch to one plant (Flora Roster S1) — the land owns `K`

**The one-sentence model:** committing a patch to one named plant **redistributes** the tile's `K`
(concentration) and changes how well its biomass **converts** to food (conversion) — and *tending
pays in conversion, never in concentration*. Authoritative design: `docs/plan_flora_roster.md` §4.3.

- **`ForagePatch::species: Option<String>`** — the committed `flora_config.json` species key, `None`
  = the wild mixed basket. **Set on the first turn a crew works the patch under `Cultivate`/`Sow`**
  (from the assignment's selection, else the highest-share species in *this tile's* basket the rung's
  `cultivation_ceiling` permits), fixed from then on, and **cleared when both improvement meters lapse
  to zero** (`ForagePatch::reconcile_owner` — a fully feral patch is a wild stand, and a wild stand is
  the whole basket). Held on the patch beside the two meters and rewound with the cloned registry, so
  a rollback rewinds *which crop* a farm is, not just how far along it is. **On the wire** (append-only,
  `ForagePatchState.committedSpecies` / `committedDisplayName`, slots 48/50 — strictly after
  `composition`'s 46): the key plus a server-resolved display name, because the client holds no
  roster. `""` means **the wild mixed basket**, not "unknown". The pair is *recorded before it has fully taken
  effect*: a patch still being prepared names its crop while its mix and its rate are only part of the
  way there (see "THE MIX INTERPOLATES TOO" below).
  > **⛔ THE COMMITMENT ALSO REPAIRS THE CREW'S TAKE SELECTION, and it is the only thing that does.**
  > `LaborTarget::Forage::take_species` has one other writer (`assign_labor`) and nothing else prunes
  > it, so a `Cultivate`/`Sow` reweighting the ground left a narrowed crew asking for plants it had
  > displaced — a selection summing to **zero share**, which is a zero take *ceiling* and therefore
  > `+0.00 /turn` in food **and** materials at once, with nothing on any readout saying why.
  > `ForagePatch::commit_species` reports whether *this* call is the commitment, and on that turn
  > only, `TakeSelection::pruned_for_commitment` drops the names no longer standing in
  > `patch_composition` and **adds the crop**. **It prunes, never overwrites**: a `planted` basket
  > keeps whatever stands outside the worked ground, so a sown tile with a fishery still has fish in
  > it and a blanket reset would re-tick plants the player deliberately unticked. Nothing surviving
  > the prune falls back to the whole basket rather than to the crop alone. Repeating the prune every
  > turn would go on deleting names as the mix faded under them, which is why the edge is reported
  > from the mutator rather than re-derived at the call site.
- **The selection rides the labor assignment** — `LaborTarget::Forage { tile, floor, species }`,
  beside the floor and for the same reason (a mutable property of the same source). It crosses the
  wire as `AssignLaborCommand.species` (proto field 9, append-only) and the text form
  `assign_labor <f> <b> forage <x> <y> [floor] [species] <workers>` — matching the parser's own usage
  string; **the two optional tokens are disambiguated by "does this parse as `f32`"**, which is why
  the stance words cannot be accepted here even positionally. It round-trips through
  `LaborAssignmentState.species`. `cultivate`/`sow` (the command forms of the improvement picker) name
  no crop and **carry over** whatever the band already selected.
- **Legality** (`forage::resolve_committed_species`, the one seam the `assign_labor` rejection and the
  labor arm's commit both read): the species exists, its `cultivation_ceiling` permits the rung
  (`allows_cultivate` / `allows_sow` — **live since S1**), and it is in **this tile's** basket via
  `forage::tile_flora_composition` (never `FloraConfig::composition` on a raw terrain). Refusals are
  `forage::SpeciesRefusal` (`unknown_species` / `species_ceiling_too_low` / `species_not_here` /
  `nothing_climbs_here`), in `SiteRefusal`'s style but a **separate** enum: `SiteRefusal` judges the
  *land* (and is therefore publishable per tile), this judges a *selection against a rung*.
  **Consequence, intended:** a biome whose whole basket is `wild` cannot be tended or sown at all —
  **ContinentalShelf, CoralShelf, InlandSea and AlpineMountain**. *"Not every plant climbs"* reaching
  the build meter. A **navigable hex is not in that list**: legality reads `tile_flora_composition`,
  which blends the channel's basket with the underlying biome's, so a navigable river over an
  alluvial plain offers `wild_emmer` and is cultivable like the valley it cut.
- **A named crop is judged wherever a command can change the (crop, rung) pair — which is TWO
  places, because since issue #442 no single command sets both.**
  - **`assign_labor`** sets the crop, so its stance validator judges it **at the entry rung
    `PlantTended`** (`cultivation_ceiling` is a ladder — `allows_sow` implies `allows_cultivate` — so
    tended is the weaker gate, and the stance command does not yet know which verb will follow). It
    fires **only when a crop is actually named**: absent means the auto-pick, and refusing an unnamed
    selection would make an ordinary wild gather on an all-`wild` basket (the fisheries, the alpine
    peaks) impossible. This is also the only path that sees a re-selection dropped onto a build
    already in flight.
  - **`cultivate` / `sow`** set the rung, so they judge **the crops the crews already hold** — every
    distinct one on the source, at that verb's own rung — rather than the auto-pick. It is the only
    place a `tended`-ceiling crop can be refused for `Sow`, and crops are per band, so "the first
    band answers for all" (the source-naming rule the retired `abandon_improvement` used, sound because at most one *improvement* is
    ever in flight) does not transfer.
  > **The failure mode both exist to prevent is a SILENT STALL.** A crop this rung refuses makes
  > `resolve_committed_species` return `Err`, so `patch.commit_species` is never called,
  > `patch.species` stays `None`, the rung's `eligible` gate is false and **the build meter never
  > advances, forever, with nothing said**. The split briefly left the Forage arm of
  > `validate_labor_policy` returning `Ok(())` unconditionally while both build commands passed
  > `species: None`, so no command path validated a player's crop at all — and the guard test went on
  > passing because it fed `validate_improvement` a `Some(species)` **no command could supply**.
  > Assert a command-path rejection through the command, never through the validator it calls.
- **Weeding — the composition moves, `K` NEVER DOES** (#433). **Weeding** is what that says: a
  reweight changes the basket and never the ceiling. `carrying_capacity` itself is the tile's
  `tile_forage_capacity` **multiplied by the interpolated `field_capacity_gain`** — the identity
  below `plant:field` and up to **2.53** at it, so the ceiling *does* move, but only as the **Field
  rung's payout**, never as a side effect of a reweight. **The two are not interchangeable**: a
  measure billed against the land — the upkeep scale — reads the land's own K through
  `forage::patch_land_capacity`, never `ForagePatch::carrying_capacity`, or it bills the rung for the
  K that rung raised (`cultivation.md` → "THE UPKEEP SCALE READS THE **TILE's** K", which also carries
  why all four land readings share that one seam). The write is **`advance_forage_regrowth`'s**, once
  a turn, recomputed fresh from the tile — the plant twin of `fauna::ecological_carrying_capacity`, so it is
  idempotent, never compounding, and a lapse or a retune reaches patches already on the map. What a
  commitment changes is `forage::patch_composition`: **Tended** raises the favored share to
  `min(1.0, share × tended_weeding_gain)` (**1.5**, `labor_config.json`), taking the increase from the
  **least abundant remaining species first**; **Field** gives the crop `1.0 − Σ(protected)`.
  > **⛔ BOTH REWEIGHTS SKIP A MEMBER THAT DOES NOT STAND IN THE WORKED GROUND**
  > (`FloraDef::stands_in_worked_ground`, `false` on `kelp` / `shellfish_beds` / `river_fish`). Weeding
  > ranks by **abundance alone**, so without the guard a Cultivate on a navigable hex would weed the
  > river's fishery away the moment it was the least abundant member — and a **Field** was worse, since
  > forcing the crop to `1.0` deleted it outright with no ranking involved. The gain is now an ask paid
  > only out of the clearable pool (`min(asked, Σ clearable)`), and the crop takes only the clearable
  > remainder. **It is not `cultivation_ceiling`** — see `cultivation.md`, where the six gather-only
  > plants you *can* clear are named.

  **`plant:field` is the ONE rung that raises `K`, and NO rung may lower it** — the earlier
  concentration term did the latter, cutting a committed tile's `K` to `share × gain` and
  **discarding the remainder**, which is the bug #433 fixed.
  `effective_forage_capacity` / `patch_concentration` /
  `concentration_for_share` and the `field_concentration_gain` dial are **retired** with it.
  **Least abundant first is currency-free and deliberate:** ranking the weeds by yield would mean
  comparing a food rate against a fodder rate against a material's characteristic vector — exchange
  rates this codebase does not have — and `hay_grass` (0 food, 0.2 fodder) has no non-arbitrary rank
  against a grain at all.

#### THE MIX INTERPOLATES TOO — a part-built rung is part-weeded ground

`forage::patch_composition` blends the **held** rung's basket with the **raising** rung's, per
species, at `RungStanding::credit` — `intensification::interpolate_composition`, the vector twin of
`intensification::interpolate`. A Sow 40% raised gives the crop 40% of the share it will end with and
leaves the volunteers 60% of theirs. Both plant rungs are `Continuous`, so Cultivate is as gradual as
Sow.

- **`RungPartialCredit` is honoured by reading `credit`, and by nothing else.** `RungStanding::at`
  already pins an `on_completion` rung to `NO_RUNG_CREDIT`, so `animal:pen`'s all-or-nothing mix is
  free here and no call site tests the mode.
- **⛔ THE BLEND IS RE-SORTED into the wire's total order** (`flora_config::sort_basket`, share DESC
  then key ASC — the one definition, shared by the affinity blend, the per-tile realization and both
  reweights). It is not presentation: `forage::default_species_for_rung` reads a basket's **first**
  entry as its dominant plant without a second sort, so an unsorted blend would silently change which
  plant a commitment falls to. The shares still sum to `WHOLE_BASKET`, exactly, because the blend is
  linear in two baskets that each do.
- **A member blended away to nothing is dropped** on `NO_SHARE`, the same bar `resolve_take_selection`
  and `species_stands_in` use for *"is this plant standing here"*.
- **The material account rides the same mix** (`patch_material_yields`), so a half-sown field's fibre
  row fades with the plant that pays it. What is still not blendable is a material's **characteristic
  vector**, which is why that account is decomposed per species instead of averaged.

> **⛔ THIS OVERTURNED AN EXPLICIT EARLIER RULING, and the reasoning matters.** `patch_composition`
> used to resolve at `standing.held` under a note saying a basket *"is the one thing that cannot be
> interpolated"* — mixing two baskets would invent shares of plants that are not growing there — with
> the *rates* carrying the smoothing. Both halves were wrong for this pair. `planted` is a
> reweighting of `weeded`, which is a reweighting of the tile's own realized mix, so every species in
> the later basket is already in the earlier one: a blend raises the favored share, lowers the others,
> and names no new plant. And the rates it delegated to **do not move across a Sow** —
> `tended_conversion_gain` and `field_conversion_gain` are both `2.0` by design, and
> `field_capacity_gain` / `field_regrowth_gain` land on the take *ceiling*, which sits above the
> worker cap on any normally-staffed row. Every smoothed term was inert and the one term that decides
> what the ground pays was the one that cliffed: reported from play as a tile committed to **tobacco**
> (0 food, 0 fodder) paying `+0.35 food · +0.07 fibre` one turn and `+0.00` with no material clause
> the next — the turn its Sow completed. `docs/plan_standing_upkeep.md` §4.10 carries the full record.

- **Conversion — a share-weighted average of the patch's own basket, at EVERY rung.**
  `forage::patch_provisions_per_biomass` and its fodder twin are `Σ share × the member's yield
  component` over `patch_composition`, so *what is growing there* finally decides what the tile pays.
  The **material** account is the same idea without the average: `forage::patch_material_yields`
  emits one row per species at its share-scaled rate, because a characteristic vector cannot be
  averaged (see the rung-2 section). **This reaches rung 1**: a wild patch used to fall through to the flat
  `forage.provisions_per_biomass` and never read its composition at all, so one constant stood in for
  a per-tile average it could not equal in either direction. That constant (**0.05**) survives only as
  the **empty-basket fallback** and as the rung-3 quality normalization baseline. Rung 2 adds
  `forage.cultivation.tended_conversion_gain` (**2.0**) on the **favored species' term only** — a
  tended stand of a *known* plant converts better, the volunteers beside it do not — so weeding and
  conversion **compound** and favoring a marginal plant barely moves the number. It multiplies food,
  fodder and the material rates alike, with no `role` branch. At rung 3 there is **no separate managed
  payoff left to scale**: the whole managed harvest was retired (§4.10, commit `3fb073a9`), and a
  Field is priced through `rung_payoff` off `basket_rate` exactly as every other rung is — the crop's
  own conversion rate, reached by multiplication. The **derived** `patch_species_quality`
  (= the projected basket's rate ÷ the wild baseline) that used to scale it outlived its last caller
  and is now **deleted**, with `WILD_SPECIES_QUALITY`. Its reason for being derived rather than a
  per-species config field — a second lever drifts from the rate it restates — is why nothing
  replaced it.
- **EACH RUNG'S PAYOFF PROJECTS TO ITS OWN RUNG — never to the rung the patch happens to stand on.**
  `forage::composition_for_rung(patch, tile_composition, forage, rung)` is the one seam, and
  `favored_conversion_gain` is keyed on the *same* `rung`, so the basket and the gain that multiplies
  it can never come from different rungs. `patch_composition` is then the same seam
  **interpolated across the rung being raised** rather than read at one rung —
  `intensification::interpolate_composition(&patch.standing(), |rung| composition_for_rung(.., rung))`.
  `composition_for_rung` itself is unchanged, because a per-rung *quote* wants exactly the rung it
  asked about. **The bug this shape exists to prevent, caught in
  the #433 slice itself:** `patch_species_quality` (since deleted — see above) keyed off the patch's own meter, so
  `snapshot_forage_patches` — which publishes `fieldYield` for *every* patch, tended ones included —
  quoted a Sow on a tended patch against the **weeded** basket *with rung 2's conversion gain in it*,
  overstating by 10.2% on the reference tile and by the full gain (2×) wherever weeding saturates.
  `field_fodder` / `managed_per_worker_*` carried the same latent defect with no reachable trigger. Pinned on the **shipped snapshot** by
  `the_published_field_yield_never_inherits_the_tended_rungs_basket` and, at the seam,
  `the_composition_seam_answers_the_rung_it_is_asked_about_not_the_one_the_patch_stands_on`.
- **Both terms switch on together, when the improvement COMPLETES.** A crew still clearing has
  displaced nothing, so a committed-but-building patch reads exactly like the wild stand it still is
  (full `K`, basket-average rate). There is no state where one term applies and the other does not.
- **The crop picker's payload rides `FloraShareInfo`** (append-only, slots 10/12/14/16 — strictly
  after `share`'s 8): **`canCultivate`/`canSow`** (the species' `cultivation_ceiling`, so the client
  can grey out what is *impossible* without holding a roster) and **`cultivateYieldRatio`/
  `sowYieldRatio`** — what committing *this tile* to *this plant* pays **against just gathering it
  wild**, per rung — **plus `cultivatePayoff`/`sowPayoff`, the provisions/turn each rung would pay
  committed to *that* plant** (slots 18/20), in `tendedYield`/`fieldYield`'s units so the client can
  substitute one for the other. The ratio **is those payoffs divided** (`forage::commit_yield_ratio`
  takes the two numbers, it does not re-derive them), and each payoff comes from `forage::rung_payoff`
  — the *same* Sustain-MSY expressions the sim quotes and pays with, `rung_payoff` asked about the
  rung by name (the rung-3 `field_provisions` this once named went with the managed harvest, below) — against a hypothetical patch at this tile's own `K`, concentrated by the rung, at the
  standing crop that rung settles at. `> 1` beats gathering, `< 1` is a **loss the player stays free to
  choose** — never clamped, never hidden, never refused; that choice *is* the decision. `0` means
  *cannot climb this rung* (distinct from a real ratio of 0, which cannot occur). **The raw
  `provisions_per_biomass` is deliberately NOT published**: `0.080` means nothing to a player, it is
  half the inputs (share is the other), and deriving the rest client-side would put the §4.3 formula
  where it can drift.
  > **The bug this shape exists to prevent (playtest, S1).** The first version divided
  > `concentration × rate ÷ base_rate` — a *capacity* basis, in which the ecology's `r` cancels. But
  > rungs 1–2 pay **MSY** (`r · K / 4`) and tending's payoff *is* that it scales `r` by
  > `tended_regrowth_gain`, so every Cultivate ratio shipped at **exactly half** its true value and
  > the tooltip told the player that tending a good delta crop *lost* (`0.9×`; real value `1.8×`).
  > Its test compared the same wrong basis on both sides, so code and test agreed with each other.
  > The rule that follows: **assert a published quote against the payoff functions, never against a
  > re-derivation of their arithmetic.**
  `flora_commitment::the_published_commit_ratio_is_the_sims_own_payoff_divided_by_the_wild_payoff`
  sweeps every biome × plant × both rungs asserting exactly that, and
  `the_cultivate_ratio_carries_the_tended_regrowth_gain` pins the dropped term by name. Both were
  confirmed to **fail** against the old implementation before the fix.
- **Rung 1 is untouched and tested from both sides** (`core_sim/tests/flora_commitment.rs`,
  `forage_cultivation::cultivate_commits_the_ground_to_a_plant_and_leaves_rung_one_untouched`).
- **S1 changed no rung payoff dial** — it left `tended_regrowth_gain` and rung 3's own payoff
  untouched so any balance movement was attributable to the roster alone. **S2 then retired the tended
  regrowth boost** (`tended_regrowth_gain` 2.0 → **1.0**, neutral): with competitor-removal now explicit
  as concentration, a growth boost double-counts it, so tending pays through concentration + conversion
  and the rung-2 "wild < tended" guarantee moved to the roster's own bar (see "Cultivation").

### Fodder — the F3 coupling (hay is delivered graze-flow)

The arc's one reach into the animal web (`docs/plan_flora_roster.md` §5). **The one-sentence model:** a
fodder crop (**hay_grass**) is a Field whose harvest fills a band's **`FODDER` store**; a pen that knows
**Foddering** draws that store as *delivered graze-flow* — one term that both raises `K_pen` and feeds the
pen directly, because hay *is* feed. (It used to be described as paying down the pen's provisions-larder
bill; that bill is retired — there is no food-unit feed left for hay to displace.) `FODDER = "fodder"` (`components.rs`) is a
second commodity key on the *same* `LocalStore` as `FOOD`, so it round-trips through the snapshot for free
and the two stores **never convert**.

- **The plant half — the yield vector finally routes by account.** A Field's harvest credits each yield
  component to its own store with **no `role` branch**: the food component → `FOOD` (a grain Field), the
  fodder component → `FODDER` (a hay Field). A grain crop's fodder reading is `0` (its vector pays no
  fodder); a hay crop's food reading is `0` (hay is no food) — the vector does the routing.
  > **As written this named `field_provisions` / `field_fodder`, "each capped by its own per-worker
  > collection (`managed_per_worker_yield` / `managed_per_worker_fodder`)".** All four went with the
  > rung-3 **managed harvest** (`docs/plan_standing_upkeep.md` §4.10; `forage.rs` → "RETIRED: the whole
  > rung-3 MANAGED HARVEST"). **The ROUTING claim above is what survived** — a Field is now drawn down
  > through the ordinary `forage_take` path, worker-capped once inside it like every rung beneath it,
  > so there are no per-account collection caps left to name. `hay_grass` (`flora_config.json`, `role: fodder`, `cultivation_ceiling: field`, `yield.fodder
  0.20`) hosts the good sowable farmland (AlluvialPlain/Floodplain/RiverDelta/PrairieSteppe/RollingHills),
  so it **competes with grain for the same scarce sowable tiles** — calories *or* herd-ceiling from one
  river-valley tile. Adding it dilutes those baskets (normalization) — economy-neutral at the wild rung,
  and its weights are kept modest so a staple stays worth tending on its own best country.
- **`Foddering`** (`FODDERING_DISCOVERY_ID = 2007`, `fauna.rs`) — a **capability** knowledge, **not** a
  rung with a verb or build meter. Earned by *running a pen* (the `animal:pen` rung's `earns_knowledge`,
  `null` → `foddering`), never start-granted. It unlocks a penned herd's store-draw only.
- **The feed (§5.2) — and hay is now the WHOLE of it beside the grass** (`advance_labor_allocation`,
  struck across every pen the band keeps by `settle_pen_hay`): `shortfall = max(0, fodder×biomass −
  footprint_intake)`, `fodder_draw = min(shortfall, FODDER store)` **iff the faction knows
  Foddering**, and `pen_fed_fraction = (footprint_intake + fodder_draw) / demand`. **There is no
  larder term after it** — the `larder_upkeep = upkeep×biomass×(1 − fed)` this bullet used to end on
  is retired with the whole food-unit feed (human food is not animal feed; `husbandry.md` → "THE
  PEN'S FEED IS ITS OWN MECHANISM"), so what grass and hay leave uncovered is a **shortfall** and the
  herd shrinks for it. Growing hay no longer *shrinks a bread bill*; it is the only thing besides the
  land that feeds a pen at all.
- **The `shortfall` above is also the READOUT**, rolled up per band as `fodderNeed` against
  `fodderIncome`: it is published **ungated**, before the Foddering test the draw applies, because a
  band that cannot hay a herd still has a herd that is short. Per pen what rides the herd row is what
  the shortfall leaves once `fodder_draw` is counted — `HerdTelemetryState.penFodderShortfall`,
  ungated on the same rule, stamped on the same pass, and the only term of that subtraction on the
  wire (the gap's own `penHayNeed` is `(deprecated)`: nothing read it). See `graze.md` → "The hay bill
  is published as the GAP".
- **The ceiling (§5.3) — `K_pen = (footprint_graze_flow + fodder_delivery_rate) / fodder_per_biomass`**,
  the fodder term added inside the one `K` seam `ecological_carrying_capacity`. **Critical for
  convergence: it reads the sustained FLOW, not the store stock** — `Herd::fodder_delivery_rate` is the
  per-turn hay output of the keeper band's Fields (stamped after the assignment loop, read next turn by
  `advance_herds` — the deliberate Logistics-reads-Population lag), split across the pens a band keeps. A
  buffer-driven K would spike-then-collapse and oscillate; the flow is what the farming sustainably
  delivers, so the loop settles. Both the draw and the `K` term are **gated on Foddering**, so no
  Foddering → byte-identical footprint-only pen. This **relaxes "a dead tile cannot hold a pen"** into an
  honest feedlot: a barren footprint carried entirely by delivered hay.
- **Convergence proven, not assumed** (`core_sim/tests/grazing_f3_fodder.rs`): a hay-carried pen reaches
  one stable fixed point from over- and under-stocked starts, deterministic across two runs. The
  **hay is now the ONLY thing that feeds a barren pen.** F3 shipped it as a term that *lowered* the
  keeper's larder bill; the larder feed has since been retired (human food is not animal feed —
  `husbandry.md` → "THE PEN'S FEED IS ITS OWN MECHANISM"), so a footprint that grows nothing and a band
  that cannot hay leave the pen **unfed**, and it shrinks. That also retires the pen's net-positive
  floor, which compared a food-unit upkeep against a food-unit yield. The pen-food ledger identity
  (`pen_food_ledger.rs`) holds for a hayed pen and a starving one **identically** — hay is off-ledger,
  and so is the feed it replaced, so `larder_delta == foodIncome − foodConsumption − raidForfeit`
  reconciles either way.
- **Wire (append-only):** `PopulationCohortState.fodderStore` — plus the band's hay **ledger**
  `fodderNeed` / `fodderIncome` / `turnsOfFodder`, which is where `band_fodder_inflow` finally
  reaches the client (`yield-forecast.md` → "The band's hay ledger", whose runway counts down the
  **Foddering-gated** drain rather than that ungated need) — `HerdTelemetryState.fodderDraw` and its
  `penFodderShortfall` twin (`penHayNeed` rode beside them until it turned out nothing read it, and
  is `(deprecated)` in place),
  `FloraShareInfo.sowFodderPayoff` (the crop picker's hay payoff, so hay reads its fodder value instead of
  a bare `0×` provisions ratio). **`HerdTelemetryState.penLarderBill` / `penHayFood` are retired**
  (slots `(deprecated)`) with the food-unit split they belonged to: the feed row is `penPastureFraction`
  + `fodderDraw`, both fodder against one fodder demand — see "Corral" → "Display snapshot".

### Cash crops — the F4 coupling (a cash crop is paid in MATERIALS)

`docs/plan_flora_roster.md` §6, **restated by arc #527**. **The one-sentence model:** a cash crop
(**cotton**/**flax**/**tobacco**/**tea**/**grapevine**) is a `field`-ceiling species whose
`provisions_per_biomass` is `0` and which is paid entirely in **materials**; harvesting it as a
**Field** credits the band's **material batches** and (near) zero food. "Calories OR cash from the
same scarce sowable tile" is the land-use tension, and *cash* is now literally *stuff*.

> **F4 shipped as the yield vector's third SCALAR account, and that scalar is retired.** The `trade`
> component was written by every take site and read by none — no `take(TRADE_GOODS)` existed anywhere
> — while the `materials` list beside it named the same take's concrete fibre and leaf. A flat scalar
> also collapses the distinction the crafting arc exists to keep: cotton fibre (`fineness 0.92 /
> strength 0.35`) and hay straw (`0.22 / 0.30`) are both `fibre` and are not the same thing. So
> `trade_goods_per_biomass`, `field_trade_goods`, `tended_take_trade_goods`,
> `patch_trade_per_biomass`, `managed_per_worker_trade` and `commit_trade_payoff` are **all deleted**;
> `docs/plan_contact_and_logistics.md` §Q5 carries the argument. The picker's cash quote came back as
> `commit_material_payoff` — see "The crop picker's cash quote is PER MATERIAL" below.

- **The plant half — the vector routes by account, no `role` branch.** A Field's harvest credits each
  component to its own account, commodity-generically: the food component → the band's `FOOD` store,
  the fodder component → its `FODDER` store, and `credit_material_yield` → its material batches. A cash
  crop's food reading is `0` (worthless as food) and a grain's material list is empty — the vector does
  the routing. (**Written as *"a Field's MANAGED harvest… `field_provisions` → FOOD, `field_fodder` →
  FODDER"***; the managed harvest and both helpers are retired — §4.10 — and a Field is drawn down
  through `forage_take`. The routing is unchanged.)
- **The MATERIAL account reads the harvest in BIOMASS** rather than scaling off one of the scalar
  currencies, because a cash Field's provisions are `0` and there would be nothing to scale. A Field's
  basket is 100% its crop, so it credits exactly that crop's reading. (The seam named here was
  `forage::field_harvest_biomass`, retired with the rest; it is `patch_material_yields` off the
  ordinary take now.)
- **`provisions 0.0` is SAFE, because the plant food path only ever MULTIPLIES by a species rate —
  it never divides by one.** `basket_rate` is a sum of `share × rate × gain` products, so a
  pure-cotton basket produces the food rate `0.0` exactly, and `rung_payoff` → `forage_provisions`
  carries it through as `biomass × rate × multiplier`. The wild rate is not in that expression at
  all: `basket_rate`'s wild fallback fires only on a basket naming nothing the roster knows, which a
  rostered cash crop never is. The one place a reciprocal *would* be needed is sidestepped by
  shipping `perWorkerBiomass` on the wire, so nobody computes
  `per_worker_yield ÷ provisions_per_biomass` — `0 / 0` on a Field of cotton. And
  `YieldVector::pays_something()` passes **because the species names material rows**.
  (**This clause used to say `patch_species_quality` divides by the wild rate.** That was true when
  written and stale from `3fb073a9`; the function is now deleted. A safety claim that names a
  mechanism outlives the mechanism silently — which is the whole argument for the doc-link gate.) That last clause is the assertion that turned "did we silently break a species"
  into a load failure when the trade axis went: all five cash crops read `0` provisions and `0`
  fodder, so a `pays_something` testing only the two scalars would have rejected every one of them at
  boot.
- **Three of the five needed materials of their own.** Cotton and flax already carried fibre rows;
  **tobacco**, **tea** and **grapevine** paid the trade scalar and nothing else, so they would have
  produced literally nothing. They now pay the **uncrafted** materials `tobacco` / `tea` / `grape`
  (rates 0.07 / 0.06 / 0.07, sitting with cotton's 0.07 and flax's 0.06 rather than with a byproduct
  like hazel bast's 0.010 — on a cash crop the harvested plant **is** the product). Their axes and
  readings are **provisional placeholders** for a luxury system that does not exist; see
  `crafting.md` → "A material with NO CRAFT is one nothing works".
- **Hosting.** The cash crops are hosted **honestly on the river valleys** — cotton/tobacco/flax on
  AlluvialPlain/Floodplain/RiverDelta (capacity ≥ the 195 field-rung floor, so they contest grain on
  real sowable ground), tea on the uplands (RollingHills/MixedWoodland/HighPlateau). **Per-tile
  realization (§10) is what keeps the staples (wild_emmer / reed_and_root) dominant** — the commit bar
  reads a tile's *local realized share*, so some alluvial tiles realize as wheat and others as cotton,
  rather than the cash crop being kept off that ground. Material rates are **playtest dials**.
- **The crop picker's cash quote is PER MATERIAL** — see the section below.

### THE GATHER CAN NAME WHAT IT CARRIES HOME — the selective take

**The one-sentence model:** a `Forage` crew may name one or more of the plants growing on the patch
and carry home only those, leaving the rest standing; naming nothing takes the whole basket, exactly
as every gather did before. A tile's basket mixes food with fibre and early food is scarce while
baskets — the kit that raises what a gatherer carries — are made of fibre, so **what am I here for**
becomes a real decision beside **how hard do I press** (the harvest floor).

- **It rides the assignment, beside the floor** — `LaborTarget::Forage::take_species`, a
  `components::TakeSelection`. It is a **mutable property of the same source** (a change on the same
  tile replaces the assignment rather than adding one, `same_source`) and it rides the checkpoint for
  free, because `SimState` clones `LaborAllocation` whole.
  > **It is NOT `LaborTarget::Forage::species`.** That is the *commit* crop a `Cultivate`/`Sow` names,
  > inert until an improvement completes; this one is live at rung 1, on the take itself. The two are
  > independent — a crew can gather flax off ground it is committing to emmer.
- **SORTED AND DEDUPLICATED BY CONSTRUCTION, not by presentation.** `TakeSelection` wraps a
  **`BTreeSet`** with a private field and one constructor (`from_keys`, which also drops blanks), so
  *unsorted* is unrepresentable rather than merely unusual. The selection reaches the snapshot, and a
  collection whose iteration order varies between two builds has already cost this repo a
  ~50%-of-runs `deterministic_snapshots_match` flake — see the share-denominator note in the
  `flora_config.json` row above. Sorting the *output* is not the fix; not being able to hold an
  unsorted one is.
- **The selection is resolved against THE MIX THE TAKE WILL NARROW** (`forage::resolve_take_selection`,
  the take-side twin of `resolve_committed_species`) — `patch_composition` where a patch stands, falling
  back to `tile_flora_composition` where none does, never `FloraConfig::composition` on a raw terrain,
  so a navigable hex is judged on the two-term basket it actually has. **There is no
  rung *asked* in it**: a selection says what the crew carries home from the stand that is standing, so
  a `wild`-ceiling plant is a perfectly good answer where naming it as a *crop* would be
  `CeilingTooLow` — but on a tended or sown patch the stand that is standing is the reweighted one.
  > **⛔ IT USED TO JUDGE THE WILD REALIZATION WHILE THE TAKE NARROWED THE REWEIGHTED MIX**, and the
  > two differ on every committed patch. So the boundary accepted — freshly typed, no staleness
  > involved — a selection the very next turn's take valued at exactly zero. Pinned by
  > `server::tests::assign_labor_judges_a_take_selection_against_the_patchs_own_mix`. The commit-side
  > half of the same defect is the take-selection prune under "Committing a patch to one plant".
  > #### ⛔ THE TWO WAYS A NAME IS WRONG GET TWO DIFFERENT ANSWERS — PRUNE, OR REFUSE
  >
  > **A plant no roster carries is REFUSED, and the refusal names it.** That is a typo; nothing can be
  > inferred from it, and a silently dropped key produces the identical take, crew count and row that
  > *"take everything"* produces, so the mistake would be undiagnosable from any readout the player
  > has. One unknown key refuses the **whole** selection rather than being filtered out of it — half a
  > selection is a different order than the one that was given — and the refusal changes nothing at
  > all, not even the half that was legal.
  >
  > **A plant that merely does not stand here is PRUNED** (`TakeSelection::pruned_to`, the same
  > narrowing the commitment repair runs), and the command lands carrying what survived. Nothing
  > surviving falls back to the whole basket, because a selection of nothing is a take of nothing.
  >
  > The two cannot be one rule, because the mix **moves under a stored selection** — that is what a
  > `Cultivate`/`Sow` is — so the absent names are typically ones that were legal when the player made
  > them and that the player's own crop then weeded out. Refusing those refused the whole
  > `assign_labor`, **worker count included**: reported from play at T120 on a Field standing at
  > `Wild Emmer 100%` whose row still named Wild Pulses, where raising the tenders did nothing turn
  > after turn and the only thing said was *"Harvest failed — Wild Pulses does not grow at (13, 10)"*.
  > The panel offered no way out either, since a chip is drawn only for a plant the **current** mix
  > carries, so the stale key had no control to clear it with. A prune the sim performs on the
  > player's behalf pushes one `status=pruned` feed line, because it is a change they did not ask for.
  >
  > Pinned through the command — never through the validator it calls (see `cultivation.md` on the
  > guard that passed while nothing validated) — by
  > `server::tests::a_take_selection_the_ground_no_longer_offers_is_pruned_not_refused`, which asserts
  > the crew on the **published wire row**, and by
  > `assign_labor_rejects_a_take_selection_naming_a_plant_that_does_not_exist` for the typo half.
- **THE TAKE: the selection scales the OFFER; it does not change the crew, and it does not trample
  the rest.** In `forage_take`:
  - **available** = `max(0, B − floor·K) × Σ selected share` — `forage::selected_biomass_share` over
    the patch's own basket, and an empty selection **short-circuits to `WHOLE_BASKET`** rather than
    summing the shares, which is what makes naming nothing byte-identical to the take before this
    existed rather than merely close to it;
  - the **worker cap is unchanged** — a hand carries what a hand carries;
  - **conversion decomposes over the selected subset**, each member weighted *within* the selection
    (`forage::narrowed`, applied **after** the rung's own reweight — weeding is a property of the
    ground and a crew's choice cannot change what grew). Food, fodder and every material row route
    through that one basket, **commodity-generic, no `role` branch**, exactly as
    `patch_material_yields` already decomposes;
  - **the drawdown removes only what was taken.** Taking the wheat must not destroy the cotton, so
    the biomass hit is never scaled back up to the whole stand.
- **AND THE CREW COUNT ANSWERS FOR IT — this is the readout the mechanic depends on.**
  `workers_needed`, `wasted`, the `sustainable` reference line and the whole pre-commit forecast all
  read the selected subset: the forecast scales **both** `biomass` and `carrying_capacity` by the
  share, so `ceiling_at`'s one expression `max(0, B − floor·K) × rate` narrows with no second copy of
  it, and `forecast == actual` holds per component. Pick a plant that is a tenth of the tile and the
  published count reads **1** where the whole basket reads **2** — *"there is very little of it
  standing"* has to be **visible**, not merely true.
  > #### ⛔ THE BUILD IS GATED ON THE GROUND, NOT ON THE SELECTION — the one term that does NOT narrow
  >
  > `Cultivate`'s accrual gate (`crew_is_working_the_source`) reads the **whole stand's** room above
  > the floor, `ground_is_workable`, and the selective take must never be threaded into it. The gate
  > says *"a crew stripping the ground it is sowing builds nothing"* — a statement about **the ground
  > being stripped** — and a selection does not strip the ground; it leaves the rest standing, by
  > definition. The builders are a **band-level pool that is not gathering at all**, so the gatherers'
  > pickiness has no bearing on whether the ground can be cleared and planted.
  >
  > Narrowing it makes the undiagnosable failure: tick *fibre* on a work row and a 25-turn `Cultivate`
  > ordered elsewhere quietly stops advancing, with nothing said and no way to connect the two. **The
  > LESSON does narrow** — you learn by working the ground, and a crew that carried nothing home did
  > not work it — so the two readings sit beside each other in the arm, named apart
  > (`ground_is_workable` vs `working_the_patch`), and the projection's copy of the gate reads the
  > same one the live arm does.
  >
  > It bites where a row's share of the stand is **zero** — a roster `reload_config` that drops a
  > plant from a tile's basket after the row was written, or a rung's own reweight (a tended patch
  > weeds its volunteers down; a Field drops them). Pinned by
  > `forage_selective_take::a_narrowed_gather_never_stalls_the_build_beside_it`, which asserts the
  > take is `0` **and** the build banked the same work the whole-basket run did — confirmed to fail
  > against the narrowed gate before the split landed.
- **DELIBERATELY NOT IN SCOPE: THE TILE DOES NOT DRIFT.** Selective taking does not shift the realized
  composition over time. Per-species biomass would make `realized_composition` **stored** state, and
  its being a pure function of `(map_seed, tile)` is exactly what makes it free and rollback-safe
  (see "Per-tile realization"). **Within-turn scarcity is the whole of what this delivers** — that is
  a stated limit of the model, not a gap waiting to be filled.
- **Command grammar:** `assign_labor <f> <b> forage <x> <y> [floor] [species] [take:<a>,<b>] <workers>`.
  The take selection is an **explicitly prefixed** token lifted out of the tail beside `kit`, never a
  third positional one: the tail's two optional tokens are already disambiguated by *"does this parse
  as `f32`"*, and a third would be indistinguishable from the commit species. On the wire it is
  `AssignLaborCommand.take_species` (proto field 12, append-only).
- **Wire (append-only):** `LaborAssignment.takeSpecies:[string]` — the selection itself, so it
  round-trips (a compose sheet reopened on the row has no other way to show what the crew was sent
  for), **empty = the whole basket**; and `ForagePatchState.compositionStandingBiomass:[float]`, the
  biomass each `composition` entry accounts for (`share × biomass`), **index-aligned** with it, so a
  crop chip can read `70% (63)` without holding any capacity arithmetic.
  > **The standing biomass rides the PATCH ROW rather than `FloraShareInfo`, and the reason is the
  > memo.** A composition entry is a pure function of ground and config, derived once per tile per
  > world and shared by refcount (`snapshot/flora_quotes.rs`); a standing biomass moves every turn, so
  > putting it there would rebuild — and deep-copy two `String`s per named plant of — every patch's
  > basket every turn. Every per-entry vector comes out of **one call** (`patch_composition_info`
  > returns a `PublishedBasket`), so no later edit can leave them describing different baskets.
- **THE SHEET MUST BE ABLE TO PRICE A NARROWING BEFORE COMMITTING TO IT** —
  `ForagePatchState.compositionProvisionsPerBiomass` / `compositionFodderPerBiomass`, the same
  index-aligned shape, from `forage::patch_species_rates` (the scalar twin of
  `patch_material_yields`: the patch's basket at its **standing** rung, the favored crop's
  interpolated gain on its own term, **not** scaled by share).
  > **`provisionsPerBiomass` is the BASKET AVERAGE, so it cannot answer a crop chip.** Without the
  > per-species pair a compose sheet's forecast sits still while the player ticks plants and quotes
  > live when they drag the worker dial — and *a readout that is live for one control and inert for
  > the other is worse than one that is inert for both*: it teaches that toggling chips is free, when
  > it is the entire decision. A sheet composes
  > `available = max(0, B − floor·K) × Σ_S share` and `rate = Σ_S share × rate ÷ Σ_S share`, which is
  > `materialPerBiomass` / `perWorkerMaterial`'s existing contract at finer grain — the same three
  > rules: never summed by the sim, **empty is "no row" not zero**, key always present. A forecast
  > **query** arm for a selection is the heavier alternative and is deliberately not taken.
  >
  > **The identity is what ties the grain to the economy:** `Σ share × rate` is the published basket
  > average, in both scalar accounts. Guarded the way the existing rates are — a real narrowed turn,
  > composed off the published fields alone and asserted against what the band **banked**
  > (`the_published_per_species_rates_compose_to_what_the_band_banks`), on a fixture that insists its
  > selection names two plants with **different** rates one of which is **zero**. Confirmed to fail
  > against a sim publishing the basket average per entry.
  >
  > #### …AND THE MATERIAL ACCOUNT, which is the case the feature was ARGUED on
  >
  > `ForagePatchState.compositionMaterialPerBiomass:[SpeciesMaterialRates]` — the same alignment, one
  > entry per composition entry, each a **wrapper table** holding that plant's own
  > `[MaterialPayoff]` (FlatBuffers has no vector-of-vectors; that is plumbing, not a model). Off the
  > same `patch_species_rates` seam, so all three accounts come from one basket at one rung.
  >
  > **Without it the motivating example is the one thing the sheet cannot answer.** Baskets are made
  > of **fibre** and baskets are what let a gatherer carry more food, so *"tick cotton, see how much
  > fibre"* is the first thing a player tries — and `materialPerBiomass` beside it is basket-averaged.
  > The scalar accounts were priced and the account the argument rested on was not.
  >
  > **The three rules bind harder here than for a scalar.** Rows merge by material id **within one
  > plant and stop there**: a characteristic reading belongs to the batch a take creates, and
  > averaging two species' would invent a plant that is not growing there — so the sim never sums
  > across species and the *sheet's* weighted mean is a **rate**, not a merged reading. **Empty rows
  > are "no row", never zero** (a grain pays no material; a `0` row would read as a crop that pays
  > badly), and every entry is present, empty rows and all.
  >
  > Guarded by `the_published_per_species_material_rates_compose_to_what_the_band_banks` against
  > `LocalStore::material_total` after a real narrowed turn, on a fixture that **insists its selection
  > names one material from two species** (cotton fibre beside flax fibre) — the last-write-wins trap
  > this file already records for the basket-wide rate, which passes a single-species fixture and is
  > off by a factor per species. Confirmed to fail against a sim publishing the basket-averaged rows
  > per entry, on the composed amount (`0.264` composed against `0.827` banked) as well as on the
  > empty-list rule.
- **Measured on the reference stand** (`(35, 19)` under seed `119304647`, `K = 275`, basket
  `river_fish` 0.291 / `wild_tubers` 0.248 / **cotton** 0.213 / `wild_pulses` 0.142 / `wild_rice`
  0.106, two hands at floor 0.15): the whole basket banks **0.769 food + 0.264 fibre**; narrowed to
  the four food plants it banks **0.954 food and no fibre**. Same stand, same crew, +24% food or a
  quarter-unit of fibre — that trade *is* the decision.
- **Pinned by `core_sim/tests/forage_selective_take.rs`**: naming nothing pays bit-exactly what the
  whole-basket rate seams compose (the neutrality bar, asserted as an identity rather than as a
  remembered figure); narrowing banks more food and no fibre **with the precondition that the two
  runs differ asserted**, so the pair cannot pass by both collapsing; the scarce plant's crew count
  reads 1 against the whole basket's 2, off the **encoded envelope**; and one selection publishes one
  key order however it was typed, duplicates included. The codec's own half is
  `sim_schema::the_selective_gathers_selection_and_standing_biomass_survive_the_wire`.
- **Client:** nothing renders either field yet — the chips, the picker and the sheet's per-species
  availability are the client pass.

### A WILD gather's material rate is on the wire too — the rung-1 half (arc #527)

The crop picker's quotes above answer *a commitment* at rungs 2 and 3. **Rung 1 had nothing**, and
`ForagePatchState.tradePerBiomass` sat `(deprecated)` with no replacement — so a tile whose realized
basket is 32% cotton and 26% tobacco composed a forage sheet reading `0.24 → 0.18 FOOD · — FODDER`
while the turn banked its fibre and leaf. The sim credited them correctly the whole time
(`systems/labor.rs`, decomposed per species); the wire had nowhere to say so.

| Field | Answers |
|---|---|
| `ForagePatchState.materialPerBiomass` | what **one unit of this patch's biomass** is made of — the twin of `provisionsPerBiomass`, so it composes at **any** floor: `ceiling(floor) = max(0, B − floor·K) × rate` |
| `ForagePatchState.perWorkerMaterial` | what **one gatherer** brings home per turn — the twin of `perWorkerYield`, so a sheet clamps `min(workers × rate, ceiling)` per material. **Folds in the tile's seasonal weight**, so it is honestly **empty in a dead season** |

Both `[MaterialPayoff]`, the same table the picker and the herd rates use. Same three contracts:
**never summed**, **empty is "no row" not zero**, **key always present**.

> #### THE ONE THING THAT DIFFERS FROM A HERD: A PATCH IS A MIXED BASKET
>
> A herd is one species. A patch's rows come from `patch_material_yields`, which **decomposes per
> species** — each carrying its own share *and its own exact reading* — and `material_yield_totals`
> then merges **by material id** for the rate. Two species that both give fibre sum into **one fibre
> rate**, which is what a rate means and what `LocalStore::material_total` sums the same way; that
> equality is the whole reason the quote is checkable against the store.
>
> **Their CHARACTERISTIC READINGS are never merged.** Averaging them would invent a plant that is not
> growing there — the config's own words. The reading belongs to the batch that lands in the store
> (`MaterialBatchState`); the rate says only *how much of what*. This file's rung-2 section states the
> same rule for the credit; this is it stated for the quote.

**Guarded at both levels, because the failure modes differ.**
`forage_basket_reweight::a_wild_gathers_published_material_rate_is_what_the_band_banks` drives a real
wild gather and asserts the published rate against `LocalStore::material_total` — and **insists its
fixture basket names one material from two species**, or the merge is untested (a last-write-wins
rate passes a single-species fixture and is off by 10× here).
`crafting_wire::every_sources_material_rate_reaches_the_wire` asserts, on the decoded snapshot over
the real map, that every source's rate is *published* and that a patch never publishes one material
twice — a rate derived right and written nowhere looks exactly like the retired field's absence. **It
covers `perWorkerMaterial` on both webs too**, as `materialPerBiomass × perWorkerBiomass`: that is
the field the compose sheet clamps with, so a codec that dropped it would zero every material row on
the sheet with the suite green. The plant twin carries the **dead-season** half — a patch whose
`perWorkerBiomass` is `0` publishes an **empty** per-worker vector, "no row" rather than a column of
zeros.

**Client:** the native reader surfaces `material_per_biomass` / `per_worker_material` on each patch
dict. Rendering them is the client pass.

### The crop picker's cash quote is PER MATERIAL — `commit_material_payoff` (arc #527)

The retired `commit_trade_payoff` was the one surface that told a player what sowing cotton is
**for**, and losing it made a cash crop unevaluable: the Cultivate/Sow rows showed only its (small,
rung-2) calories. `forage::commit_material_payoff` is the replacement, and it is deliberately **not**
a restoration.

> **A VECTOR, NOT A SCALAR, IS THE WHOLE DIFFERENCE.** The trade quote answered *"how much trade"* —
> a number a market could total and a player could not act on. This answers *"0.29 fibre"*, which is
> what a cash crop **is**. **Do not sum the rows into one materials/turn figure** anywhere, client
> included: that is the retired axis under a new name, and it re-collapses the distinction the
> materials model exists to keep.

- **Shape.** `Vec<MaterialPayoff { material, amount }>`, one row per material, **merged by material
  id and ordered by it** (a `BTreeMap`, so the wire order is stable). A rung-2 basket can name one
  material twice — cotton fibre beside hay straw — and merging is what makes the quote comparable to
  `LocalStore::material_total`, which sums the same way. It carries **no quality reading**: a rating
  is a characteristic vector on the batch the harvest creates, and a picker row asks the flat
  question *"how much of what"*.
- **Priced on each rung's OWN harvest**, through the same expression the payout runs — `rung_msy_take`
  asked about the rung by name, at rung 3 exactly as at rung 2, so there is no second copy of a rung's
  harvest to drift from the payout. (This named `field_harvest_production`, "split out of
  `field_harvest_biomass`… the `production` term of the payout's own `min(production, collection)`".
  Both went with the rung-3 managed harvest — `docs/plan_standing_upkeep.md` §4.10 — and with them the
  `min(production, collection)` shape: a Field is worker-capped once inside `forage_take` like every
  rung beneath it. What must not drift at rung 3 is now the production gains.)
- **EMPTY MEANS "NO ROW", NEVER "ZERO"**, and this is the field's contract rather than a nicety: a
  client renders one row per entry, so an empty quote is *no row* while a `0`-valued entry would read
  as a cash crop that pays badly. Empty is what a plant paying no material reports **and** what a
  plant that cannot climb the rung here reports — the `cultivatePayoff`-reads-`0` convention carried
  onto a vector.
- **The two rungs answer DIFFERENTLY, and that is the model.** A **Field** is 100% its crop (#433),
  so a grain Field quotes nothing at all. A **tended patch** is a *weeded basket* — the favored share
  rises but the volunteers are still standing — so committing to a grain still quotes whatever fibre
  and leaf its neighbours pay, which is exactly what the turn credits
  (`patch_material_yields` decomposes rather than averaging). It is the same fact the food account
  already records from the other side, where a rung-2 cash crop pays non-zero calories.
- **Wire (append-only, last on `FloraShareInfo`):** `sowMaterialPayoff` /
  `cultivateMaterialPayoff`, each `[MaterialPayoff { materialId, amount }]`. **A new table with a new
  id** — the freed `sowTradePayoff`/`cultivateTradePayoff` slots stay `(deprecated)` and are not
  reused. Two fields for one account, for `cultivatePayoff`/`sowPayoff`'s reason: the rungs differ in
  basket, conversion gain *and* the shape of the harvest.
- **Guarded at three levels, because the failure modes differ.**
  `flora_f4_cash::the_picker_material_quote_is_the_material_the_sim_credits` runs a real turn and
  asserts the quote against `LocalStore::material_total` — *a quote that disagrees with what lands in
  the band's store is worse than no quote*.
  `flora_quotes::every_quoted_plant_carries_its_own_per_rung_material_payoff` asserts the **capture**
  stamps the seam's own rows (a quote computed correctly and then not written onto the row looks
  exactly like the retired quote's absence). And
  `sim_schema::the_per_material_cash_quote_survives_the_wire_per_rung` pins the **codec**, including
  that an empty vector decodes as empty — a nested vector that failed to serialize decodes as
  *absent*, which is the same shape as "no row" and would otherwise hide.
- **Client:** the native reader surfaces `sow_material_payoff` / `cultivate_material_payoff` as
  arrays of `{ material_id, amount }` dicts on each `composition` entry. **Rendering them is the
  client pass** — nothing in GDScript reads the keys yet.

### The `role` tag is on the wire — `FloraShareInfo.role`

Each composition entry carries its species' own `role` (`staple` | `fodder` | `cash`), appended to
`FloraShareInfo` (append-only). It is the **roster's** tag, copied at the quote site
(`snapshot/flora_quotes.rs`) and at the roster-fallback in `patch_composition_info`, so the tag has one
definition and cannot be re-derived into a second. Still a **display tag**: nothing in the sim branches
on it, and a client renders it and nothing more. `""` means **unstated** (a species the roster no longer
names), never `staple` — the `displayName` convention.

**A client cannot derive it from the payoffs beside it.** `cultivatePayoff` /
`cultivateFodderPayoff` / `cultivateMaterialPayoff` and their `sow*` twins are **rung-2 and rung-3**
numbers — they fold in the weeding and conversion gains rather than stating the species' own vector —
and they are **all zero or empty** for a plant that cannot climb on this ground
(`canCultivate`/`canSow` false), which is exactly the `wild`-ceiling case where the role is still a
true and useful fact. Pinned by
`sim_schema`'s `the_three_crop_roles_survive_the_wire_distinctly` /
`an_unstated_role_ships_as_an_empty_string_rather_than_a_default_category` and, at the capture site, by
`flora_quotes::tests::every_quoted_plant_carries_its_rosters_own_role`.

### The vector routes at RUNG 2 as well — a Tended Patch pays every account it names

§3's spine is unconditional: *a harvest* of `B` biomass pays `B × yield.*` into every account the
species names. F3 and F4 implemented it only inside the `is_field()` branch of
`advance_labor_allocation`'s Forage arm, so a **completed Tended Patch** routed provisions alone and
dropped its crop's other accounts on the floor — a rung-2 cash crop (`provisions_per_biomass: 0`)
produced **nothing at all** while being drawn down at full MSY every turn (issue #427). The same take
now feeds every account at rung 2:

- **`tended_take_fodder`** (`forage.rs`) — the take-driven twin of the rung-3 fodder credit (then the
  retired `field_fodder`, a managed rate; rung 3 is take-driven too now), resolving its rate
  through `patch_fodder_per_biomass`, the fodder twin of the `patch_provisions_per_biomass`
  conversion seam. It reads `committed_species`, so both scalar accounts switch on together the turn
  the rung completes, and both read `0` for a crop whose vector does not pay them — commodity-generic,
  no `role` branch. `FODDER` credits the working band's `LocalStore` (feeding `band_fodder_inflow` as
  the Field arm does).
- **The MATERIAL account rides the same take**, credited by `credit_material_yield` off
  `forage::patch_material_yields` at rungs 1 and 2 alike — which is what closes #427 now that the
  trade scalar is retired: a tended grapevine patch banks **grapes**, not nothing.
  (`tended_take_trade_goods` is deleted with the axis it converted.)
- **TAKE-driven — and since the Field's managed rate retired, so is EVERY plant rung**
  (`cultivation.md` → "What a Field buys"). What follows describes the arm as it was written against
  a Field that was never
  drawn down, so its harvest collapses the policy axis and is quoted as a rate on the standing crop. A
  tended patch *is* drawn down, so its non-food accounts ride the same take its food account does:
  `Deplete` on a tended cash crop banks more material than `Sustain` because it takes more, and the
  over-farm ⚠ covers the whole vector. **A Sustain harvest of a tended cash crop therefore does pay**
  — that is the #427 fix, not a leak. Pinned by
  `forage_tended_vector::a_deeper_floor_banks_more_material_off_a_tended_cash_crop`, which asserts the
  ordering as a **ratio against the biomass ratio**, so a per-depth bonus could not hide in it.
- **No second collection cap.** `forage_take` already caps the take by `workers × per_worker_biomass ×
  seasonal`, so there is nothing further to cap: the crop the crew carries home *is* the take it made.
  (This read *"unlike the Field arm's `managed_per_worker_fodder`"* — there is no Field arm and no
  per-account cap since §4.10; **every** plant rung is capped exactly here, once.)
- **A mixed basket DECOMPOSES rather than averaging.** Food and fodder are interchangeable numbers, so
  a basket averages them into one rate; a material carries a characteristic vector, and averaging two
  species' would invent a plant that is not growing there. So a mixed tile pays one material credit
  per species, each keeping its own exact reading, and credits landing in the same band merge in the
  store.
- **Fodder is gated at the CONSUMER, not in the rate seam** (#433). The invariant reaches fodder too —
  a tile realizing `hay_grass` pays hay on any harvest — but crediting it to a band with nowhere to put
  it hands out animal feed nobody bid for. So a patch's `FODDER` credit is gated on the faction knowing
  **Foddering** (2007, the same gate the pen's draw reads), and the gate lives at the **credit site in
  `systems/labor.rs`** so `forage.rs` stays free of knowledge lookups and the vector stays
  commodity-generic.
- **The other arm is a commitment to a FODDER-BEARING SPECIES** —
  `committed_to_a_fodder_crop(patch.species, &flora) || knows(…, FODDERING)`, where the first term
  resolves the committed key through `FloraConfig::species` and asks that species' own
  `yield.fodder_per_biomass > 0.0` (`YieldVector::bears_fodder`). A crop the player chose *for hay* is
  a bid for hay and needs no capability; a crop chosen for anything else is a bid for that, whatever
  the ground underneath it happens to carry.
  - **A commitment to ANYTHING was the bug.** The predicate was `patch.species.is_some()`, which
    accepted a bid for grain as a bid for hay: a `wild_emmer` patch on ground that is ~31% `hay_grass`
    still converts at the basket's share-weighted average (#433), so it credited a real fodder income
    to a faction with **no pens, no Foddering, and nothing that could ever eat it** — `fodder_need` 0.0
    on every band while the hay piled up and pooled across the supply network. That is verbatim the
    failure the gate exists to prevent, so the predicate now asks what was actually bid for.
  - **It FAILS CLOSED.** A committed id that does not resolve in the flora table is not
    fodder-bearing; an unreadable commitment refuses the credit rather than opening the gate.
  - **It is the COMMITMENT rather than the rung**, so the gate lifts on the first turn of a
    `Cultivate`/`Sow` build *on a fodder crop*, while the patch still stands at rung 1 and still
    converts at the wild basket's rate. That is the rationale working, not an off-by-one: the bid is
    placed when the crew starts, not when the meter fills. Reading this as "rungs 2 and 3 are ungated"
    is narrower than the code and mis-states which turn the credit begins.
  - **It decides WHETHER, never HOW MUCH.** A committed patch still converts at the share-weighted
    average of its own basket (#433); crediting only the committed species' own vector is a different
    model and is not this rule. Swept over all six cases (wild / `hay_grass` / `wild_emmer` × with and
    without Foddering) by
    `forage_basket_reweight::fodder_is_gated_on_foddering_unless_the_patch_is_committed_to_a_fodder_crop`,
    and the published row is pinned against the credit over the same six by
    `the_published_fodder_is_the_fodder_the_band_was_actually_credited`.
  - **Both halves of the fodder answer are on the wire** (#485). The rate seam publishes what the
    **land** pays — `ForagePatchState.fodderPerBiomass`, commodity-generic and knowledge-blind, as the
    gate's placement at the consumer requires — and the **capability** rides the faction's knowledge
    list as `knowledges["foddering"]`, its 0..1 progress on discovery 2007. So a viewer holding a patch row and its faction's knowledge row can tell a
    **refused** fodder credit (positive rate, no Foddering) from an **absent** one (a patch whose
    basket grows no hay), which the rate alone cannot distinguish. `foddering` is the one entry on that
    list that is **not** a rung-transition gate: no rung waits on it — which the roster beside it
    states outright as `is_step: false` — and the pen rung *teaches* it
    (`intensification_ladder.json`, corral's `earns_knowledge`), and it gates all three fodder seams —
    the pen's hay draw, the pen's `K` fodder term, and this wild credit. **What that costs the capture
    is stated where the capture is edited** — `.claude/rules/core_sim/yield-forecast.md`, which owns
    `core_sim/src/snapshot/**`, not this file.
  - **The CLIENT reads the PUBLISHED commitment, never the composed improvement.**
    `RungGates.wild_fodder_reason` tests the patch's published `committed_species` — so the lock lifts
    on exactly the turn the sim's does, and a rung the player has ticked but not yet committed still
    reads as refused. Keying the client on the RUNG would have shown a refusal through the whole build
    while the sim was already paying. **It mirrors `committed_to_a_fodder_crop` species-for-species**,
    resolving the committed id in the patch's own composition and asking whether that row's
    `cultivateFodderPayoff` or `sowFodderPayoff` is positive — the wire's two-rung spelling of this
    rule's one `fodder_per_biomass > 0.0` — and failing closed on an id it cannot resolve. The two
    predicates are coupled: this one moved to the species test first and the client kept the old
    "committed to anything" reading for a window, which rendered the credit unlocked on a grain
    commitment the sim was refusing.
- **Pinned by `core_sim/tests/forage_tended_vector.rs`**: the #427 grapevine-under-Sustain regression
  (`a_tended_cash_crop_under_sustain_credits_materials_and_costs_food`), hay crediting `FODDER`, and
  `Deplete > Sustain` on the same tended cash crop **in the ratio of the biomass taken**. The nine
  trade-account tests that sat beside them are deleted with the axis, and the file's own gravestone
  names each one and where its surviving claim moved to.
- **The CROP PICKER quotes rung 2's FODDER account** (issue #419) — `commit_fodder_payoff` takes a
  **`RungKey`**, exactly as `commit_payoff` does, and dispatches through `rung_fodder_payoff`, which
  asks the rung it is given for that rung's own fodder skim (at the time of writing, the retired
  `field_fodder` at rung 3 and `tended_fodder` at rung 2; `0` below). On the wire as
  `FloraShareInfo.cultivateFodderPayoff`, the tended twin of `sowFodderPayoff`. Until then the seam
  hardcoded `RungKey::PlantField`, so a tended fodder crop was *paid* correctly (above) and
  *previewed* with a **Field's** number — a managed rate on the full standing crop standing in for an
  MSY skim off a merely-weeded basket, on the rung the player was about to spend 25 turns on. **Two
  fields per account per rung, for `cultivatePayoff`/`sowPayoff`'s reason**: the rungs differ in
  basket, conversion gain *and* the SHAPE of the harvest, so one number cannot answer both.
  - **Both rung-2 scalar accounts ride ONE take**, `tended_msy_take` — the Sustain skim on the tended
    curve, extracted so `tended_provisions` and the fodder quote cannot describe different harvests
    (the `patch_ecology` no-second-copy rule, applied to the take). The non-food quote is
    **floor-blind**: a crop-picker row states what the *crop* pays on this ground. At the food-peak
    floor on a patch at `K/2` the quote and the credit therefore coincide exactly, which is how they
    are pinned —
    `forage_tended_vector::the_published_cultivate_fodder_quote_is_the_fodder_a_tended_patch_actually_credits`
    runs a real turn and asserts the published quote against what the turn credited (the §4.3 rule).
  - **A rung-2 cash crop's FOOD payoff is non-zero, and the picker states it.** Weeding raises cotton's
    share but leaves the volunteers standing, so `cultivate_payoff` is their calories — a *loss*
    against gathering the same tile wild. Only a sown **Field** is 100% crop and pays exactly `0`
    food.
  - **The cash quote is a VECTOR now** (arc #527) — `cultivateTradePayoff` / `sowTradePayoff` went
    with the trade axis and `cultivateMaterialPayoff` / `sowMaterialPayoff` replaced them, per
    material rather than per currency. See "The crop picker's cash quote is PER MATERIAL".
- **Still provisions-only:** `project_realized_forage` / `project_arrivals_forage` — the forward
  projection reports food, so a cash Field's contribution to it is its calories and nothing else.

