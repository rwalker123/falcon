# Plan: Band Fission — a band splits in two

Status: **The verb is a split, not a settlement.** A band divides where it stands and both halves
walk away as ordinary bands. The authoritative spec for arc
[#508](https://github.com/rwalker123/falcon/issues/508). It answers the questions the feature slices
cannot answer for themselves — *does the new band stay yours, who is allowed to split, who actually
goes, what they take, why you would ever do it, and what the player sees* — and hands each slice a
decided rule to build.

The manual promises this twice and nothing implements it: *"The band splits into more bands as it
grows"* (§Start of Game) and *"Split/merge is allowed"* (§Default Start Profile: Late Forager Tribe).
Three design docs defer to it **by name**: `docs/plan_exploration_and_sites.md` (§2, the deferred
breakaway), `docs/plan_early_game_labor.md` (decisions 1 and 14, "the SplitClan/migration seam"), and
`docs/plan_settlement_population.md` §Migration (*"how bands settle, how colonists split off, and how
people urbanize"*).

## Motivation

The player has exactly one band and no way to make a second. That is not a missing button — it is a
missing *decision*. Everything the economy is built around assumes you must choose where your people
stand: a band works only the tiles within `R` (labor_config, default 2), so a band is permanently
limited to ~19 tiles' worth of forage, game and pasture. Grow past what those 19 tiles yield and
there is nothing to do about it. You cannot work a second patch of land without abandoning the first.

## The mechanism, in one line

**A band splits in two on the tile it is standing on, and you move the new one like any other band.**
Form a band, then move it. The new band is a `ResidentBand` from the moment the command resolves —
it eats, ages, forages, births and starves on the same systems as every other band, on its first
turn. Nothing about it is special and nothing about it is deferred.

### Why not "an expedition that settles"

An earlier draft of this document built the verb the other way: compose a party, walk it somewhere,
and let it stop being an expedition on arrival. That framing is retired, and the reason is worth
recording because it is what the whole design now rests on.

**A scouting party is composed for scouting.** It is outfitted with a there-and-back food budget, no
dependants, and a target chosen for what it might *find*. Letting that party become a band means
founding one from inputs nobody chose for the purpose — the player never decided how big it should
be, who should be in it, or what it should carry, because at the moment those were fixed they were
answering a different question. **So a scouting party cannot found a band, and the arrival verb is
gone.** A splinter that is a band from the start is composed as one.

Everything that framing needed, this one does not have to answer. Deleted outright: the reachability
gate and its BFS over the faction map, a destination on the compose sheet, a distance-scaled seed
larder, a fourth expedition mission, the arrival affordance, the split between a *forecast* refusal
and a *blocking* one, the re-check on arrival, and fold-back on recall. There is no gap between
deciding and doing, so every rule fires once, at the split.

## As-built — what exists (verified 2026-08-09)

- **A resident band can be created at runtime.** `systems::expeditions::found_band_from_expedition`
  built one from an arriving party: allocate a `BandId`, flush the map the party learned, attach a
  culture layer seeded from the home band (`CultureManager::attach_band_from_source`), drop
  `Expedition`, insert `ResidentBand` + `DemographicFlowAccumulator`. **The band-creation half of
  this is what the split reuses**; the expedition half goes.
- **A mid-game founding survives a rollback.** `sim_state.rs` no longer assumes every `ResidentBand`
  came from worldgen. Unchanged by this document and needed exactly as it stands.
- **Cohorts are fractional and the client apportions them.** A real band is 9.29 children / 16.54
  workers / 4.64 elders (`Scalar`), and `HudFormat.apportion_people` renders whole bodies by
  largest remainder so the parts sum to the displayed total. The comment on
  `BandPanelController._build_people_block` records the bug that forced it: independent rounding made
  the panel read 9 + 17 + 5 = 31 beside a top bar reading 30, *"the same band, counted twice,
  disagreeing by a person."*
- **Consumption is bracket-weighted** — `food_demand = per_capita_draw × (children·0.6 +
  working·1.0 + elders·0.8)` (`demographics_config.json`). A dependant is cheaper than a worker but
  never free.
- **A band's stores hold exactly two things** — `provisions` and `trade_goods`
  (`components.rs:267,290`). There is no livestock quantity on a band; see §Q4.
- **Everything cross-faction is same-faction by construction.** Migration filters
  `dest.faction != source.faction` unconditionally; supply pooling bins by `(faction, cell)`. The one
  line that would ever change a band's faction (`population.rs:945`) is dead, because
  `FactionRegistry::default()` holds only `FactionId(0)`.

---

## Q1 — Same faction, or a new polity?

**A player-directed split is always same-faction. Independence is never chosen; it is something that
happens to you.**

This is the arc's central decision and it is not a placeholder for multi-faction. Two reasons:

- **A band you deliberately split off is yours.** Offering the player a checkbox marked *make this an
  independent tribe* is offering them a button that reads *lose a band* — nobody presses it, and if
  they did, the interesting part (the drift, the resentment, the moment you notice they stopped
  answering) would have been skipped entirely.
- **Independence has to be earned by the world, not declared at the split.** The push that
  historically breaks a polity apart is that the bonds stop working: too far to trade with, too far
  to help, and nothing coming back. The sim already measures that, in two live signals — the supply
  network's connected components (`balance_supply_networks` links same-faction bands within a
  configurable reach and auto-balances stores; beyond reach a band lives off its own larder) and
  per-cohort `grievance`.

So: **one verb, two outcomes, and the second one is decided later by the sim.** A band that has been
outside its faction's supply network for a sustained stretch, and whose grievance is high, drifts
toward independence and eventually forks into a polity of its own. That is #284's emergent fission
and this doc hands it the trigger: **disconnection + grievance over time, never distance alone.**

**Sequencing.** The emergent half is **blocked on #513** (multi-faction support) — a band cannot
become independent when the registry holds one faction and every worldgen band is hardcoded into it.
Build the same-faction case first and completely.

## Q2 — What floors must hold?

**Two, both counted in workers, both evaluated at the split, which is the only moment there is.**

- **The new band must start with at least `min_founding_workers` working-age people.** Below that
  there is nobody to staff a single food role and it starves where it stands. Playtest founded a
  colony with **one** person, which is what put this floor in the game.
- **The parent must keep at least `parent_min_workers`.** The guard against hollowing out the home
  band to crew a second one and killing both.

That is the whole gate set. Three rules an earlier draft carried are gone, and each is gone for a
reason that is now structural rather than a judgement call:

- **No reachability rule.** A new band appears on its parent's tile — trivially reachable — and then
  walks wherever you send it under the ordinary move order. *You can only settle ground you can get
  to* stops being a gate and becomes a fact about legs.
- **No dependency-ratio ceiling on the parent.** A proportional split (§Q3) takes the same share of
  mouths as of hands, so the parent's `(children + elders) / working` comes out **exactly** where it
  went in. A gate that can never fire is not a safeguard, it is a line of code that lies about what
  the model can do.
- **No habitability rule.** Splitting raises no question about the ground — the new band is standing
  where the old one already was. Where it goes afterwards is a move order, and a bad move is a
  mistake the player can walk out of.

**Both floors are forecast on the compose sheet before the player commits**, which is easy here
because there is no gap between the forecast and the act.

## Q3 — Who goes?

**The player picks one number — how many workers leave — and everything else divides on that same
share.** Children and elders follow in proportion; so do the provisions. The new band is a smaller
copy of the band it came from, not a party with a composition of its own.

`share = workers_chosen ÷ parent_workers`, applied to children, elders and the larder alike.

Three things follow, and they are the reason this is the model rather than per-bracket steppers:

- **There is no composition to game.** The failure mode per-bracket allocation invites is a band that
  cannot feed itself shedding the people who cannot feed it — split off the elders, keep the workers,
  and the home band's demand falls ~14% (or ~35% with the children) while its workforce is untouched.
  Under a proportional split that move does not exist, because you cannot choose to shed only them.
  **The exploit is closed by the shape of the model, not by a lever guarding against it** — which is
  why no `max_dependency_ratio` dial appears anywhere in this document.
- **The decision the player actually makes is *how big a split*,** not which twelve individuals walk.
  A band is a cohort, not a roster; asking the player to compose an age pyramid is asking a question
  the rest of the game never asks.
- **Both halves are viable in the same way the parent was.** Whatever ratio the parent was surviving
  on, both halves inherit — so a split never manufactures a #431 spiral on either side.

**No travel-speed penalty for dependants.** Speed is not a property a band has: `BandTravel` is a
bare `target` and every band moves at the single global `labor_config.band_move_tiles_per_turn`. A
family group being slower would mean **making movement speed per-band**, new machinery built to
express a cost that provisioning already expresses.

**The sim moves the exact fraction; the sheet shows whole bodies.** 3.27 children is a perfectly
ordinary `Scalar` and moving it exactly is what keeps the two cohorts conserved with no rounding rule
and no leftover to reconcile. The player never sees it, because the display apportions — see §Q6.

## Q4 — What do they take?

- **A proportional share of the larder.** Not a reserve calculation and not a new number: the new
  band starts stocked because its people were already sitting on that food. `share × parent larder`,
  the same fraction as everything else.
- **The kit is inherited worn.** The new band takes a **copy of the parent's `BandEquipment` wear
  ledger**, not `BandEquipment::default()`. Otherwise splitting mints a fresh kit out of nothing,
  permanently, and the intended pull into the crafting economy (`plan_early_game_labor.md` §TOE —
  running your kit dry *is* the pull) is trivially defeated by splitting. The splinter is exactly as
  worn out as the people it came from.
- **Knowledge is copied in full, not divided.** Knowledge is not a conserved quantity; people who
  know how to knap take that with them without the parent forgetting. Divergence afterwards is the
  Telling arc's business.
- **The map goes with them** — the `Discovered` view is the faction's, and both halves are the same
  faction.
- **The culture goes with them, and it costs the parent nothing.** The new band's culture layer is
  attached at the split, seeded from the parent's current traits and parented on the province it is
  standing in (`CultureManager::attach_band_from_source`). The precedent is migration: a band that
  *walks* twenty tiles keeps the culture it arrived with and chases its new province at the band
  scope's elasticity. Unlike knowledge it starts diverging immediately, because the new band mints
  its own character offset.

### There is deliberately NO breeding stock in the dowry

An earlier draft listed animals as a dowry line with a `breeding_stock_fraction_max` lever. **There
is nothing there to divide**, on three counts, and the lever is retired with it:

- **A band owns no livestock.** Its stores hold `provisions` and `trade_goods` and nothing else, so
  there is no quantity to take a fraction of.
- **A pen is fenced land with a herd pinned to it.** A corralled `Herd` carries
  `corralled_at: Some(tile)` and a `pen_radius` — the footprint of fenced ground it grazes and
  derives its carrying capacity `K` over. It does not travel, and neither does a fraction of it.
- **Ownership is a faction's, not a band's** — `Herd::owner: Option<FactionId>`. Since a split is
  always same-faction (§Q1), the new band already co-owns every pen the faction has. Nothing changes
  hands.

**What a band loses by walking away is reach, not title.** Labor assignments lapse with
`reason=out_of_range` once a source is beyond the band's work range, so a splinter that walks off
simply stops working the pen, and one that settles within range can work it alongside its parent.
Penning knowledge copies over with everything else, so a colony that lands near wild game corrals its
own herd on the normal ladder — it earns one rather than inheriting one.

## Q5 — Why would you ever?

The push and the pull are both already in the sim; this arc just gives them somewhere to go.

**Push — the home tile stops being enough:**
- **Work range.** A band works ~19 tiles (`R` = 2). That is a hard ceiling on how much land one band
  can touch, and the only way past it is a second band standing somewhere else.
- **Carry capacity caps population** (`plan_early_game_labor.md` decision 7). The cap is the plateau;
  fission and storage are the two ways off it.
- **Forage depletion** around a long-parked band — the tiles you have been working get worse the
  longer you stay.
- **Per-hex crowding** (#277) and the dependency-ratio pressure of #431.

**Pull — somewhere better:** a **settle-site** discovery (`sites_config.json`), a better biome,
unworked herds, a river.

**The shape of the choice.** #251 (roaming payoff) is the other half: staying mobile keeps your
options and your options' options; splitting spends people to hold two places at once.

## Q6 — What does the player see?

- **A compose sheet on the Parties tab.** One stepper (workers), the share it implies, a readout of
  **the new band** — people, brackets, dependants per worker, provisions, and how many turns those
  provisions last unfed — the **home band's after-state beside its now**, and the floors' verdicts.
  The sim exports the floors; the client composes the forecast from them and does no gate of its own.
- **People are rendered whole, and both halves are apportioned in ONE pass.** Running
  `apportion_people` separately over each half lets both round the same way and show 31 people
  leaving a band of 30 — precisely the bug that function exists to prevent, reintroduced on a new
  surface. The chosen worker count is pinned to the integer the player picked and left out of the
  apportionment, so the stepper can never disagree with the readout.
- **The home band's dependency ratio is shown deliberately flat.** It is the readout that teaches
  why there is no composition to game.
- **When it happens** — a new event kind on the **Alert** rung of `RUNG_BY_KIND`
  (`.claude/rules/client/event-dock.md`). The dock's own rule is that `died` and `migrated` sit at
  Notable *because they are things that happen to a band as a matter of course*; a split is the
  opposite — rare, player-initiated, and irreversible. Also a beat worth telling
  (`docs/plan_the_telling.md`).
- **The new band's name** — #271 owns the mechanism. The rule this arc asks for: a new band draws
  from the faction's name pool and the player can rename it. A band the player will be looking at for
  the rest of the game should not be called *Band 2*.

---

## Config levers

A `settle` block in `expedition_config.json`. **Two dials, both floors on workers, both saying what
they do in their own name.** Opening values are sized against the ~30-person / ~16-worker starting
band (`plan_early_game_labor.md` §Starting state) and want playtest. The failure columns say which
way each one breaks, because that is what tells you which direction to move it.

| Lever | Opening | What it means | Too low | Too high |
|---|---|---|---|---|
| `min_founding_workers` | **4** | Working-age people the **new** band must start with. | One or two people can form a band that cannot staff a single food role — a death notice with a marker on it. | At 8+ a split costs half the home workforce, so the verb ships and is never used. |
| `parent_min_workers` | **6** | Workers the **parent** must still have after the split. | The home band can be hollowed out to crew a second one, killing both. | Only an already-large band can split, which pushes the first fission very late. |

Deliberately **not** levers: the split share (it is the player's one input), the provisions carried
(the share decides it), and any consumption rate (`demographics_config.json` →
`consumption.per_capita_draw` is the one answer).

Retired levers, each with the rule it governed: `parent_max_dependency_ratio` (a proportional split
cannot move the ratio), `establishment_turns` (no walk to provision for), `breeding_stock_fraction_max`
(nothing to divide, §Q4), and any party-side dependency ceiling (no per-bracket choice to bound).

### Every settle dial ships in the Workbench

These are numbers nobody can pick correctly at a desk — they are picked by playing. So the values
above are opening positions, and the slice's real obligation is to make them **live-adjustable in the
Workbench's config tuning page** so playtest moves them without a rebuild.

The surface already exists and already covers this file. `ConfigTuningPage` is manifest-driven
(`.claude/rules/client/workbench.md`), and `tuning_manifest.json` already carries an **`expedition`**
kind bound to `EXPEDITION_CONFIG_PATH`. Each dial is one manifest row:

| pointer | type | min · max · step | default | unit | hint |
|---|---|---|---|---|---|
| `/settle/min_founding_workers` | int | 1 · 12 · 1 | 4 | workers | Working-age floor the new band must start with. `1` is the "off" setting and is still a real band. |
| `/settle/parent_min_workers` | int | 0 · 20 · 1 | 6 | workers | Workers the home band must keep. **0 turns the gate off** for a playtest run. |

The ranges are wider than the values are likely to want, deliberately: a dial you cannot push past
where it plays well cannot show you *why* it plays well.

## Sequencing

1. **Design doc (this document).** ✅ #509.
2. **"Start a life here"** — #510, **retired**. It built the arrival verb on the expedition path;
   §The mechanism records why that framing is gone. What it left behind and this arc keeps: the
   runtime band-creation machinery, the culture attach at founding, the checkpoint path that
   re-attaches `ResidentBand` for a band worldgen never made, and the `settle` config block.
3. **Form a new band** — #511. The split command, the proportional share, the two floors, the
   dowry, the Parties-tab compose sheet, and both `tuning_manifest.json` rows — the dials land
   **with** the gates they govern, because a gate that cannot be moved during a playtest cannot be
   judged during one.
4. **Naming** — #271, generalized so it serves a split band and not only the player's first one.
5. **Blocked on #513, then:** the emergent half — #284 (drift → independent polity, on the Q1
   trigger), #512 (scouts defecting to a better-off faction), #458 (cross-faction proximity trade).

## Cross-cutting touchpoints

- **The `ResidentBand` marker is a membership switch, not a label.** A long list of systems query
  `With<ResidentBand>` precisely so expeditions are excluded — supply pooling, sedentarization,
  migration, demographics, startup seeding, herd drift, the default-band command pickers. The new
  band **silently joins all of them on the turn it is formed**, with `age_turns` at 0, stores that
  are a fraction of its parent's, and a position identical to its parent's.
- **A band formed by a split can split again** — the command gates the source on `ResidentBand`, so
  recursion arrives for free. It also means the parent floor has to hold for a band that was itself
  formed fifteen turns ago.
- **Two bands on one tile.** A split leaves both halves co-located until the player moves one. Per-hex
  crowding (#277), supply pooling and the work-range overlap all see that state on the turn it
  happens.
- **Supply network** (`supply.rs`): two same-faction bands within reach pool their food automatically,
  so a band that stays close is a logistics extension of its parent and one that walks away is on its
  own. The player will feel this without being told, and it is also the signal Q1's independence
  trigger reads.
- **Client**: the compose sheet, the dock entry, the new band on the band/city dock's band list, and
  the two `settle` rows on the Workbench config tuning page.

## Open items

- **The independence trigger's shape** (turns disconnected × grievance, and where the threshold sits)
  is specified here in kind but not in numbers. It cannot be tuned before #513 makes a second faction
  reachable — #284 sets them when it can measure them.
- **Merging is not designed here.** The manual says *"Split/merge is allowed"*; this doc covers the
  split. Merge is two resident bands on one tile becoming one, and is a separate slice — worth filing
  once fission ships and there are two bands to merge.
- **Scout-party wear is discarded on fold-back** — `fold_party_into_band` settles workers, food and
  trade but not the party's `BandEquipment`, so a returning expedition's wear evaporates. A
  pre-existing leak on the expedition path, noted here because §Q4 is where the wear ledger's
  ownership got decided. Out of scope for fission.

## See Also

- `docs/plan_exploration_and_sites.md` — §2, the detached-party machinery and the deferred breakaway;
  knowledge as dowry; the settle-site category that motivates the pull.
- `docs/plan_early_game_labor.md` — the band-as-labor-pool model, the TOE/wear economy §Q4 protects,
  the carry-capacity cap that is the push, and decisions 1 and 14 that defer here by name.
- `docs/plan_settlement_population.md` — §Migration (*"how colonists split off"*), the demographic
  brackets, and the supply network Q1's independence trigger reads.
- `shadow_scale_strategy_game_concept_technical_plan_v_0.md` — §Start of Game (*"the band splits into
  more bands as it grows"*), §Default Start Profile (*"split/merge is allowed"*).
- `core_sim/CLAUDE.md` → Scouting & Hunting Expeditions; `.claude/rules/core_sim/` for the per-arc
  engineering rationale once slices land.
