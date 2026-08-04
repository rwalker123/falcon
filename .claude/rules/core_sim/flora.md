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
| `src/data/flora_config.json` | **The flora roster** (Flora Roster F1, `flora_config.rs`, env override **`FLORA_CONFIG_PATH`**; design `docs/plan_flora_roster.md`) — the plant twin of `fauna_config.json`'s species table, and the first time a plant has a **name**. **33 species** (18 F1–F4 families + the **F5 fine-grained mass-fill** of 15 — kelp, sea_kale, wild_rice, cattail, chestnut, wild_orchard, sunflower, wild_pulses, mesquite, wild_fig, cloudberry, rock_tripe, alpine_herbs, cave_fungi, grapevine — so every non-zero biome now carries a **3–5 species basket** and per-tile realization (§10) has enough breadth to vary tile-to-tile). The F1 core is 12 biome-keyed staples + `river_fish`, which alone hosts `NavigableRiver` and means the *fishery bonus term*, not the vestigial capacity row; each: `display_name`/`plural`/`adjective`, **`role`** (`staple`\|`fodder`\|`cash` — a **DISPLAY TAG ONLY**, derived from which component of the yield vector dominates, never branched on in the sim), **`cultivation_ceiling`** (`wild`\|`tended`\|`field`, default `field` — the exact twin of `husbandry_ceiling`: one ceiling, not two flags, so "sowable but not tendable" is unrepresentable; `allows_cultivate`/`allows_sow` are **LIVE since S1** — they gate which species a `Cultivate`/`Sow` may commit a patch to, so a basket that is all-`wild` cannot be tended or sown at all), **`host_biomes`** (`TerrainType` → a **relative affinity WEIGHT, not a capacity**), **`yield`** (`provisions_per_biomass` / `fodder_per_biomass` / `trade_goods_per_biomass` — one vector, three accounts; **all three are LIVE AT EVERY RUNG since #433** — each patch's rate is the share-weighted average of its own basket's vectors, so a *wild* tile's food, fodder and trade all depend on what is actually growing there; `provisions_per_biomass` went live at S1 as a *committed* patch's conversion rate, `fodder_per_biomass` at F3 on hay Fields, `trade_goods_per_biomass` at **F4** on cash Fields — see "Cash crops — the F4 coupling"), and `regrowth_rate`. **NAMING DECOMPOSES, IT DOES NOT ADD:** `FloraConfig::composition(terrain)` is a **derived, precomputed** share table — `share = weight / Σ weights hosting that biome` — so the shares sum to `1.0` by construction and `share × forage.capacity_by_biome[biome]` always re-sums to the biome's own capacity. Adding a species **dilutes** the others; it cannot inflate a tile. Built once at load through the single constructor every `Deserialize` path routes through, so a stale table is unrepresentable, and sorted **share DESC then species key ASC** because it goes on the wire (`ForagePatchState.composition`). **The share DENOMINATOR must be summed in a deterministic order, not merely presented in one** — `HashMap` iteration order is randomized per instance and f32 addition is not associative, so a `Σ weights` accumulated in map order lands a ULP apart between two builds, and that ULP divides into every published share and changes the snapshot hash. Both share tables therefore order *before* they sum: `build_composition` sorts its rows first, and `navigable_composition` merges through a `BTreeMap`. Sorting the *output* does not fix the arithmetic — this was a ~50%-of-runs `deterministic_snapshots_match` flake, pinned now by `the_share_table_is_bit_identical_across_builds` / `navigable_composition_is_bit_identical_across_calls`. Navigable hexes blend two baskets — see `FloraConfig::navigable_composition` / `forage::tile_flora_composition`, the twin of `navigable_forage_capacity`. **`provisions_per_biomass` is hand-tuned per species and LIVE since S1** — it is the *conversion* half of the commit trade (see "Committing a patch to one plant"), so the F1 "flat 0.05 verbatim" rule is deliberately gone. Grains/tubers on their best ground are strongest (wild_emmer **0.080**); the `wild`-ceiling gathered things sit at or below the 0.05 baseline (shellfish/river fish 0.050, pine nut 0.048, oak mast 0.045, arctic greens 0.040). **Those `wild`-ceiling rates stopped being inert at #433** — they can still never be *committed*, but they are now weighted into their tile's wild basket average, so an oak-mast-heavy realization genuinely pays less per biomass than an emmer-heavy one and the numbers have to be right. `fodder_per_biomass` is **live since F3** on the one fodder crop **hay_grass** (0.20 — a hay Field harvests into the band's `FODDER` store, a pen that knows **Foddering** draws it) and 0.0 on every staple; `trade_goods_per_biomass` is **live since F4** — the flat **0.005 token** on every staple (the F1 baseline that differentiates trade value) and **trade-dominant** on the four **cash crops** **cotton** (0.20) / **flax** (0.15) / **tobacco** (0.18) / **tea** (0.16), whose harvest as a Field credits the faction `trade_goods` stockpile (see "Cash crops — the F4 coupling"); `regrowth_rate` is still `forage.ecology.regrowth_rate` verbatim — all pinned by `core_sim/tests/flora_roster.rs` **against the loaded labor config, not literals**, along with the design's own bar, **reframed at #433**: what must differ is the *crop choice* — a species must pay materially better on its best country than on its worst, and materially better than favoring a marginal neighbour on the same tile. The older form ("committing must sometimes lose to leaving it wild") is retired: with the rung-2 conversion gain, any commitment with a real share pays, and the rung-2 decision is which currency plus whether the 25-turn build is worth it. The cash crops are hosted **honestly on the river valleys** (cotton/tobacco/flax on AlluvialPlain/Floodplain/RiverDelta, tea on the uplands); **per-tile realization (§10) keeps the staples dominant on their own realized tiles** — the commit bar reads a tile's local realized share, not the uniform biome share — rather than keeping cash crops off that ground (the S1 commit-worthiness bar). `reed_and_root` is `field`-ceiling (rice/taro on a delta are the archetypal field crop; at `tended` the richest sowable ground in the game would be unsowable). **Validated** — `FloraConfig::validate()` runs inside `from_json_str` (every load path) for the per-row invariants (empty `display_name`, empty `host_biomes`, a non-positive weight, an all-zero `yield`, a non-positive `regrowth_rate`), and the **cross-web** pair `FloraConfig::validate_against_forage(&forage.capacity_by_biome)` runs on the load path with `labor_config`'s table passed in (one copy): **no nameless food** — a biome with non-zero forage capacity that no species hosts is rejected, which is what forces a **complete** roster rather than a couple of species — and **no claiming barren ground** — a species hosting a stated-zero biome is rejected. A broken invariant is logged at **error** level (`flora_config.invalid_rejected`) and the builtin is used |
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
  (`workers × per_worker_biomass_capacity × seasonal_weight`), clamps to the patch's biomass,
  **subtracts the take**, and converts to provisions
  (`take × provisions_per_biomass × output_multiplier`).
  **Since `docs/plan_harvest_floor.md` slice 1 the whole axis is ONE expression parameterised by a
  floor** — `max(0, B − floor·K) × build_dip`, the exact twin of `fauna::hunt_escapement_ceiling` —
  with the floor carried on the assignment (`LaborTarget::Forage`) as an `f32` fraction of `K`.
  A deeper floor leaves less standing and so takes more; the `Forage` arm sells the take as trade
  goods **at the basket's own rate, with no markup of any kind** — the 4×
  `market.trade_goods_multiplier` is deleted, because a factor attached to one drawdown depth
  re-welded product to intensity (`docs/plan_harvest_floor.md` §4).
  **It is `r`-INDEPENDENT and takes no `EcologyConfig`**, which is what makes the rung-2 payoff read
  as what it is: a tended patch does not get a bigger ceiling, it *refills faster*, so it has more
  stock standing above the floor next turn.
  The `Forage` arm of `advance_labor_allocation` (Population) writes the real
  `sustainable = sustainable_yield(biomass_before, cap, patch_ecology) × provisions_per_biomass ×
  output_multiplier` (MSY-based) into the yield telemetry as the **long-run reference line the player
  reads beside the take** — **not** as the ⚠ predicate. The first harvest of a stocked patch is its
  accumulated stock and legitimately exceeds one turn's regrowth under *every* stance, Sustain
  included, so the over-forage ⚠ is `components::floor_overdraws` (`floor < K/2` — a fact about where
  the crew stops), exactly as it is on the animal web.
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
  roster. `""` means **the wild mixed basket**, not "unknown". Note the pair is *recorded before it
  takes effect* — a patch still being prepared names its crop while still reading full `K` and the
  wild rate.
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
    band answers for all" (`abandon_improvement`'s rule, sound because at most one *improvement* is
    ever in flight) does not transfer.
  > **The failure mode both exist to prevent is a SILENT STALL.** A crop this rung refuses makes
  > `resolve_committed_species` return `Err`, so `patch.commit_species` is never called,
  > `patch.species` stays `None`, the rung's `eligible` gate is false and **the build meter never
  > advances, forever, with nothing said**. The split briefly left the Forage arm of
  > `validate_labor_policy` returning `Ok(())` unconditionally while both build commands passed
  > `species: None`, so no command path validated a player's crop at all — and the guard test went on
  > passing because it fed `validate_improvement` a `Some(species)` **no command could supply**.
  > Assert a command-path rejection through the command, never through the validator it calls.
- **Weeding — the composition moves, `K` NEVER DOES** (#433). `carrying_capacity` is
  `tile_forage_capacity` at every rung; the write is **`advance_forage_regrowth`'s**, once a turn,
  recomputed fresh from the tile — the plant twin of `fauna::ecological_carrying_capacity`, so it is
  idempotent, never compounding, and a lapse or a retune reaches patches already on the map. What a
  commitment changes is `forage::patch_composition`: **Tended** raises the favored share to
  `min(1.0, share × tended_weeding_gain)` (**1.5**, `labor_config.json`), taking the increase from the
  **least abundant remaining species first**; **Field** forces it to `1.0` and the rest to `0`. There
  is no rung below 4 that raises `K` **and none that lowers it** — the earlier concentration term did
  the latter, cutting a committed tile's `K` to `share × gain` and **discarding the remainder**, which
  is the bug #433 fixed. `effective_forage_capacity` / `patch_concentration` /
  `concentration_for_share` and the `field_concentration_gain` dial are **retired** with it.
  **Least abundant first is currency-free and deliberate:** ranking the weeds by yield would mean
  comparing a food rate against a trade rate — an exchange rate this codebase does not have and that
  belongs to the Market arc — and `hay_grass` (0 food, 0 trade, 0.2 fodder) has no non-arbitrary rank
  at all.
- **Conversion — a share-weighted average of the patch's own basket, at EVERY rung.**
  `forage::patch_provisions_per_biomass` and its fodder/trade twins are `Σ share × the member's
  yield component` over `patch_composition`, so *what is growing there* finally decides what the tile
  pays. **This reaches rung 1**: a wild patch used to fall through to the flat
  `forage.provisions_per_biomass` and never read its composition at all, so one constant stood in for
  a per-tile average it could not equal in either direction. That constant (**0.05**) survives only as
  the **empty-basket fallback** and as the rung-3 quality normalization baseline. Rung 2 adds
  `forage.cultivation.tended_conversion_gain` (**2.0**) on the **favored species' term only** — a
  tended stand of a *known* plant converts better, the volunteers beside it do not — so weeding and
  conversion **compound** and favoring a marginal plant barely moves the number. It multiplies food,
  fodder and trade alike, with no `role` branch. At rung 3 the managed payoff keeps its one dial and
  is scaled by the **derived** `patch_species_quality` (= the projected basket's rate ÷ the wild
  baseline, which for a Field's 100%-crop basket is exactly the crop's rate) — never a second
  per-species field that could drift from the rate it restates.
- **EACH RUNG'S PAYOFF PROJECTS TO ITS OWN RUNG — never to the rung the patch happens to stand on.**
  `forage::composition_for_rung(patch, tile_composition, forage, rung)` is the one seam, and
  `favored_conversion_gain` is keyed on the *same* `rung`, so the basket and the gain that multiplies
  it can never come from different rungs. `patch_composition` is then just
  `composition_for_rung(.., standing_rung(patch))`. **The bug this shape exists to prevent, caught in
  the #433 slice itself:** `patch_species_quality` keyed off the patch's own meter, so
  `snapshot_forage_patches` — which publishes `fieldYield` for *every* patch, tended ones included —
  quoted a Sow on a tended patch against the **weeded** basket *with rung 2's conversion gain in it*,
  overstating by 10.2% on the reference tile and by the full gain (2×) wherever weeding saturates.
  `field_fodder` / `field_trade_goods` / `managed_per_worker_*` carried the same latent defect with no
  reachable trigger. Pinned on the **shipped snapshot** by
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
  — the *same* `tended_provisions`/`field_provisions`/Sustain-MSY functions the sim quotes and pays
  with — against a hypothetical patch at this tile's own `K`, concentrated by the rung, at the
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
- **S1 changed no rung payoff dial** — it left `tended_regrowth_gain` and `field_provisions_per_biomass`
  untouched so any balance movement was attributable to the roster alone. **S2 then retired the tended
  regrowth boost** (`tended_regrowth_gain` 2.0 → **1.0**, neutral): with competitor-removal now explicit
  as concentration, a growth boost double-counts it, so tending pays through concentration + conversion
  and the rung-2 "wild < tended" guarantee moved to the roster's own bar (see "Cultivation").

### Fodder — the F3 coupling (hay is delivered graze-flow)

The arc's one reach into the animal web (`docs/plan_flora_roster.md` §5). **The one-sentence model:** a
fodder crop (**hay_grass**) is a Field whose harvest fills a band's **`FODDER` store**; a pen that knows
**Foddering** draws that store as *delivered graze-flow* — one term that both raises `K_pen` and pays down
the pen's lossy provisions-larder bill, because hay *is* feed. `FODDER = "fodder"` (`components.rs`) is a
second commodity key on the *same* `LocalStore` as `FOOD`, so it round-trips through the snapshot for free
and the two stores **never convert**.

- **The plant half — the yield vector finally routes by account.** A Field's harvest credits each yield
  component to its own store with **no `role` branch**: `field_provisions` → `FOOD` (a grain Field), the
  new `field_fodder` → `FODDER` (a hay Field), each capped by its own per-worker collection
  (`managed_per_worker_yield` / `managed_per_worker_fodder`). A grain crop's `field_fodder` is `0` (its
  vector pays no fodder); a hay crop's `field_provisions` is `0` (hay is no food) — the vector does the
  routing. `hay_grass` (`flora_config.json`, `role: fodder`, `cultivation_ceiling: field`, `yield.fodder
  0.20`) hosts the good sowable farmland (AlluvialPlain/Floodplain/RiverDelta/PrairieSteppe/RollingHills),
  so it **competes with grain for the same scarce sowable tiles** — calories *or* herd-ceiling from one
  river-valley tile. Adding it dilutes those baskets (normalization) — economy-neutral at the wild rung,
  and its weights are kept modest so a staple stays worth tending on its own best country.
- **`Foddering`** (`FODDERING_DISCOVERY_ID = 2007`, `fauna.rs`) — a **capability** knowledge, **not** a
  rung with a verb or build meter. Earned by *running a pen* (the `animal:pen` rung's `earns_knowledge`,
  `null` → `foddering`), never start-granted. It unlocks a penned herd's store-draw only.
- **The feed (§5.2), drawn BEFORE the larder** (`advance_labor_allocation`'s corral-tend branch):
  `shortfall = max(0, fodder×biomass − footprint_intake)`, `fodder_draw = min(shortfall, FODDER store)`
  **iff the faction knows Foddering**, then `larder_upkeep = upkeep×biomass×(1 − (footprint+hay)/demand)`.
  Hay is subtracted before the lossy provisions path, so growing hay *shrinks* the bread bill but never
  makes bread a better deal (property 2 — feeding a pen bread stays exactly as lossy). `pen_fed_fraction`
  reads the hay-inclusive fed share, so starvation/shrink sees a hayed pen as fed.
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
  net-positive floor (`FaunaConfig::validate`) needs **no change** — hay only lowers the larder bill (like
  pasture), so the best-case fully-larder-fed floor is unchanged. The pen-food ledger identity
  (`pen_food_ledger.rs`) holds for a hayed pen too — hay is off-ledger, so `penFeedUpkeep` (provisions
  paid) drops while `larder_delta == foodIncome − foodConsumption − penFeedUpkeep` still reconciles.
- **Wire (append-only):** `PopulationCohortState.fodderStore`, `HerdTelemetryState.fodderDraw`,
  `FloraShareInfo.sowFodderPayoff` (the crop picker's hay payoff, so hay reads its fodder value instead of
  a bare `0×` provisions ratio), plus the render-ready feed split `HerdTelemetryState.penLarderBill` (net
  larder bill after pasture + hay) / `penHayFood` (hay's food-equivalent) — see "Corral" → "Display
  snapshot" for the `pasture_food + penHayFood + penLarderBill == penUpkeep` invariant the client draws
  the feed row from.

### Cash crops — the F4 coupling (trade is the vector's third routing)

The yield vector's **third and final** account, the exact twin of the F3 fodder work
(`docs/plan_flora_roster.md` §6). **The one-sentence model:** a cash crop (**cotton**/**flax**/
**tobacco**/**tea**) is a `field`-ceiling species whose vector is *trade-dominant* and whose
`provisions_per_biomass` is `0`; harvesting it as a **Field** credits the faction **`trade_goods`
stockpile** and (near) zero food. "Calories OR cash from the same scarce sowable tile" is the
land-use tension.

- **The plant half — the vector routes by account, no `role` branch.** A Field's managed harvest
  credits each yield component to its own account, commodity-generically: `field_provisions` → the
  band's `FOOD` store, `field_fodder` → its `FODDER` store, and the new **`field_trade_goods`** → the
  **faction** `trade_goods` stockpile (`FactionInventory`, *not* a band-local store — the one place F4
  differs from F3, exactly as the Deplete-forage arm's wild sale credits the faction). Each is capped
  by its own per-worker collection (`managed_per_worker_yield` / `_fodder` / `_trade`). A cash crop's
  `field_provisions` is `0` (worthless as food), a grain's `field_trade_goods` is the negligible flat
  token (`biomass × field_provisions_per_biomass × 0.005/0.05`), a hay crop's is `0` — the vector does
  the routing.
- **No markup, anywhere.** `field_trade_goods` applies no `trade_goods_multiplier` — and neither does
  anything else now: the whole `forage.market` block is **deleted** with the four stances (see the
  retired-levers note at the top of this file). A deep floor still out-earns a shallow one on trade
  **because it takes more biomass**, which is the ladder doing the work rather than a stance bonus.
- **`provisions 0.0` is SAFE.** `patch_species_quality` divides by the **wild** `provisions_per_biomass`,
  never the species rate, so a 0-provisions cash crop yields exactly 0 food with no divide-by-zero, and
  `YieldVector::pays_something()` passes because trade `> 0`. This is the sharp "pays no calories" edge.
- **Hosting.** The four cash crops are hosted **honestly on the river valleys** — cotton/tobacco/flax
  on AlluvialPlain/Floodplain/RiverDelta (capacity ≥ the 195 field-rung floor, so they contest grain on
  real sowable ground), tea on the uplands (RollingHills/MixedWoodland/HighPlateau). **Per-tile
  realization (§10) is what keeps the staples (wild_emmer / reed_and_root) dominant** — the commit bar
  reads a tile's *local realized share*, so some alluvial tiles realize as wheat and others as cotton,
  rather than the cash crop being kept off that ground (the S1 commit-worthiness bar, reframed around
  the realized share in `flora_roster.rs` + `flora_commitment.rs`). Trade rates are **playtest dials**.
- **The picker seam.** `commit_trade_payoff` is the crop-picker's cash quote — the trade twin of
  `commit_fodder_payoff`, built through the *same* `hypothetical_patch` and the *same*
  `field_trade_goods` the sim pays with, so quote and payout cannot drift. `sowYieldRatio`/`sowPayoff`
  read `0` for a cash crop (worthless as food), so this is the number that lets the picker show its
  real value instead of a bare `0×`.
- **Wire (append-only):** `FloraShareInfo.sowTradePayoff` — trade/turn a sown Field of this plant
  would credit the working band's own `TRADE_GOODS` store (see `yield-forecast.md` → "Trade goods are a
  BAND-LOCAL store"); `0` for a staple/hay or a plant that cannot Sow here. **Client
  done:** the native reader decodes `sowTradePayoff` and the crop picker renders a cash-crop trade row
  (`FLORA_CROP_TRADE_ROW_FORMAT`).

### The `role` tag is on the wire — `FloraShareInfo.role`

Each composition entry carries its species' own `role` (`staple` | `fodder` | `cash`), appended to
`FloraShareInfo` (append-only). It is the **roster's** tag, copied at the quote site
(`snapshot/flora_quotes.rs`) and at the roster-fallback in `patch_composition_info`, so the tag has one
definition and cannot be re-derived into a second. Still a **display tag**: nothing in the sim branches
on it, and a client renders it and nothing more. `""` means **unstated** (a species the roster no longer
names), never `staple` — the `displayName` convention.

**A client cannot derive it from the payoffs beside it.** `cultivatePayoff` / `cultivateFodderPayoff` /
`cultivateTradePayoff` and the `sow*` triple are **rung-2 and rung-3** numbers — they fold in the weeding
and conversion gains rather than stating the species' own vector — and they are **all zero** for a plant
that cannot climb on this ground (`canCultivate`/`canSow` false), which is exactly the `wild`-ceiling case
where the role is still a true and useful fact. Pinned by
`sim_schema`'s `the_three_crop_roles_survive_the_wire_distinctly` /
`an_unstated_role_ships_as_an_empty_string_rather_than_a_default_category` and, at the capture site, by
`flora_quotes::tests::every_quoted_plant_carries_its_rosters_own_role`.

### The vector routes at RUNG 2 as well — a Tended Patch pays all three accounts

§3's spine is unconditional: *a harvest* of `B` biomass pays `B × yield.*` into three accounts. F3 and F4
implemented it only inside the `is_field()` branch of `advance_labor_allocation`'s Forage arm, so a
**completed Tended Patch** routed provisions alone and dropped its crop's fodder and trade on the floor —
a rung-2 cash crop (`provisions_per_biomass: 0`) produced **nothing in any currency** while being drawn
down at full MSY every turn (issue #427). The same take now feeds all three accounts at rung 2:

- **`tended_take_fodder` / `tended_take_trade_goods`** (`forage.rs`) — the take-driven twins of
  `field_fodder` / `field_trade_goods`, resolving their rates through `patch_fodder_per_biomass` /
  `patch_trade_per_biomass`, the fodder and trade twins of the `patch_provisions_per_biomass`
  conversion seam. Both read `committed_species`, so all three accounts switch on together the turn
  the rung completes, and all three read `0` for a crop whose vector does not pay them — commodity-generic,
  no `role` branch. `FODDER` credits the working band's `LocalStore` (feeding `band_fodder_inflow` as the
  Field arm does); `TRADE_GOODS` credits the **faction** stockpile.
- **TAKE-driven, not a managed rate — the deliberate difference from the Field arm.** A Field is never
  drawn down, so its harvest collapses the policy axis and is quoted as a rate on the standing crop. A
  tended patch *is* drawn down, so its non-food accounts ride the same take its food account does:
  `Deplete` on a tended cash crop earns more trade than `Sustain` because it takes more, and the
  over-farm ⚠ covers the whole vector. **A Sustain harvest of a tended cash crop therefore does pay
  trade** — that is the #427 fix, not a leak.
- **No second collection cap.** `forage_take` already caps the take by `workers × per_worker_biomass ×
  seasonal`, so unlike the Field arm's `managed_per_worker_fodder`/`_trade` there is nothing further to
  cap: the crop the crew carries home *is* the take it made.
- **One trade rule at every drawn-down rung** (#433). There is no committed-vs-wild branch left: every
  `forage_take` harvest credits `take × patch_trade_per_biomass`, the basket average, at rung 1 **and**
  rung 2 alike — and **nothing multiplies it**. The `forage.market.trade_goods_multiplier` (4.0) this
  paragraph used to describe is deleted with the stance axis: it re-welded product to policy, which is
  exactly what the harvest-floor arc removed. A deeper floor sells more because it takes more,
  settings on the same rate. The species-blind `forage.market.trade_goods_per_biomass` (0.005) is
  **retired**; the vector is the rate. Because every staple carries `trade_goods_per_biomass` 0.005,
  a staple-only basket's wild `Deplete` sale is **numerically unchanged** — only baskets holding a
  cash crop move, and a `Sustain` gather of wild ground that happens to hold grapevine now returns
  trade goods, because you gathered them. **Rung 3 keeps its no-markup rule**: a Field is never drawn
  down and has no policy axis.
- **Wild fodder is gated at the CONSUMER, not in the rate seam** (#433). The invariant reaches fodder
  too — a wild tile realizing `hay_grass` pays hay on any harvest — but crediting it to a band with
  nowhere to put it hands out animal feed nobody bid for. So an **uncommitted** patch's `FODDER`
  credit is gated on the faction knowing **Foddering** (2007, the same gate the pen's draw reads), and
  the gate lives at the **credit site in `systems/labor.rs`** so `forage.rs` stays free of knowledge
  lookups and the vector stays commodity-generic. Rungs 2 and 3 are **ungated**: committing a patch to
  `hay_grass` *is* the bid.
  - **Both halves of the fodder answer are on the wire** (#485). The rate seam publishes what the
    **land** pays — `ForagePatchState.fodderPerBiomass`, commodity-generic and knowledge-blind, as the
    gate's placement at the consumer requires — and the **capability** rides
    `IntensificationKnowledgeState.foddering`, the faction's 0..1 progress on discovery 2007, appended
    after `penning`. So a viewer holding a patch row and its faction's knowledge row can tell a
    **refused** fodder credit (positive rate, no Foddering) from an **absent** one (a patch whose
    basket grows no hay), which the rate alone cannot distinguish. `foddering` is the one field on that
    table that is **not** a rung-transition gate: no rung waits on it, the pen rung *teaches* it
    (`intensification_ladder.json`, corral's `earns_knowledge`), and it gates all three fodder seams —
    the pen's hay draw, the pen's `K` fodder term, and this wild credit. It also stands alone in
    `snapshot_intensification_knowledge`'s all-zero skip, so a faction whose only ladder progress is
    Foddering is still published rather than dropped.
- **Pinned by `core_sim/tests/forage_tended_vector.rs`**: the #427 grapevine-under-Sustain regression, hay
  crediting `FODDER`, a staple keeping its food *and* gaining its token trade, the wild `Deplete` sale
  unchanged, no double credit under `Deplete`, and `Deplete > Sustain` on the same tended cash crop. Five
  of the six fail against the pre-fix code; the wild-market pin passes both sides, which is its job.
- **The CROP PICKER quotes rung 2's other two accounts** (issue #419) — `commit_fodder_payoff` /
  `commit_trade_payoff` take a **`RungKey`**, exactly as `commit_payoff` does, and dispatch through
  `rung_fodder_payoff` / `rung_trade_payoff`: `field_fodder`/`field_trade_goods` at rung 3, the new
  **`tended_fodder`/`tended_trade_goods`** at rung 2, `0` below. On the wire as
  `FloraShareInfo.cultivateFodderPayoff` / `cultivateTradePayoff`, the tended twins of the two `sow*`
  fields. Until then both seams hardcoded `RungKey::PlantField`, so a tended cash crop was *paid*
  correctly (above) and *previewed* with a **Field's** number — a managed rate on the full standing crop
  standing in for an MSY skim off a merely-weeded basket, on the rung the player was about to spend 25
  turns on. **Two fields per account per rung, for `cultivatePayoff`/`sowPayoff`'s reason**: the rungs
  differ in basket, conversion gain *and* the SHAPE of the harvest, so one number cannot answer both.
  - **All three rung-2 accounts ride ONE take**, `tended_msy_take` — the Sustain skim on the tended
    curve, extracted so `tended_provisions` and the two new quotes cannot describe different harvests
    (the `patch_ecology` no-second-copy rule, applied to the take). The non-food quotes are
    **floor-blind**, and there is no `market.trade_goods_multiplier` left to be blind to — the lever is
    deleted. A crop-picker row states what the *crop* pays on this ground — the
    same rule `field_trade_goods` states one rung up. At the food-peak floor on a patch at `K/2` the quote and
    the credit therefore coincide exactly, which is how they are pinned:
    `forage_tended_vector::the_published_cultivate_{trade,fodder}_quote_is_the_{trade,fodder}_a_tended_patch_actually_credits`
    run a real turn and assert the published quote against what the turn credited (the §4.3 rule).
  - **A rung-2 cash crop's FOOD payoff is non-zero, and the picker states it.** Weeding raises cotton's
    share but leaves the volunteers standing, so `cultivate_payoff` is their calories — measured 0.149
    on the fixture's richest cotton tile against 4.28 trade, and a *loss* against gathering the same
    tile wild. Only a sown **Field** is 100% crop and pays exactly `0` food. Pinned by
    `a_staples_cultivate_trade_quote_is_the_flat_token_not_zero_and_not_a_cash_crops`, which also pins
    the other half of #419: **every staple's rung-2 trade quote is non-zero** (the flat 0.005 token —
    measured 0.112 for wild_emmer), which is why no client surface may treat "trade > 0" as "cash crop".
- **Still provisions-only:** `project_realized_forage` / `project_arrivals_forage` — the forward
  projection reports food, so a cash Field's contribution to it is its calories and nothing else.

