# Plan: Band Fission — a splinter group leaves and starts a new band

Status: **Design — not implemented.** The authoritative spec for arc
[#508](https://github.com/rwalker123/falcon/issues/508): splitting a band. It answers the six
questions the feature slices cannot answer for themselves — *does the new band stay yours, who is
allowed to leave, who actually goes, what they take, why you would ever do it, and what the player
sees* — and hands each slice a decided rule to build.

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

Fission is the answer, and the machinery for it already exists. A scouting expedition **is** a
detached band — its own cohort, id, marker, larder and kit — that happens to come home. The whole
feature is the case where it doesn't.

## The mechanism, in one line

**A founding is an expedition that stops being one.** The party drops its `Expedition` component and
gains `ResidentBand`, keeping the `BandId` it was allocated at launch and the map it learned on the
way. That swap is already the documented target (`core_sim/src/components.rs:735`,
`docs/plan_exploration_and_sites.md` §2 deferred breakaway). So the player verb is a **third arrival
action** beside *send onward* and *recall*: **"start a life here."** You scout somewhere good, and
instead of coming home, you stay.

## As-built — what exists and what doesn't (verified 2026-08-08)

- **Detaching a party works and is well-factored.** The outfit path (`core_sim/src/bin/server.rs`,
  around the `max_party`/provision-draw block at `:2903–2975`), `fold_party_into_band`
  (`core_sim/src/systems/expeditions.rs:989`), `BandIdAllocator`, and `ResidentBand` as a positive
  isolation marker.
- **`AwaitingOrders` has exactly two exits** — re-aim via `move_band`, or `recall_expedition`. There
  is no third.
- **No code path creates a resident band at runtime.** `ResidentBand` is inserted in exactly two
  places: worldgen (`core_sim/src/systems/worldgen.rs:3018`) and checkpoint restore
  (`core_sim/src/sim_state.rs:487`).
- **A party is 100% working-age.** The launcher clones the parent cohort, then sets
  `children = 0`, `working = party`, `elders = 0` (`server.rs:2937–2939`). Nobody's family goes.
- **The party bound is availability and nothing else** — `1..=available_workers(cohort.working)`,
  and the comment above it is explicit that this is deliberate: *"you cannot detach workers you do
  not have, and you may detach all the ones you do."* There is no floor and no parent check.
- **The only thing a party carries is food** — `party × distance × provision_draw_per_worker_per_tile`
  off the parent's larder (`server.rs:2919–2921`). Not stock, not stores beyond food.
- **A party leaves with a pristine kit.** `BandEquipment` is a *wear ledger*
  (`core_sim/src/components.rs:1403`) and the party is spawned with `BandEquipment::default()` — zero
  wear on every item. For a party that comes home this is invisible; for one that never does, it is
  free equipment. See §4, *The kit is inherited worn*.
- **Knowledge already travels.** The party's cohort is a clone of the parent's, and `knowledge` is
  not among the fields zeroed — a detached party carries the parent's knowledge fragments today.
- **Everything cross-faction is same-faction by construction.** Migration filters
  `dest.faction != source.faction` unconditionally (`core_sim/src/systems/labor.rs`, in
  `advance_population_migration`); supply pooling bins by `(faction, cell)` (`core_sim/src/supply.rs`).
  The one line that would ever change a band's faction — `cohort.faction = migration.destination`
  (`core_sim/src/systems/population.rs:945`) — is dead, because `FactionRegistry::default()` holds
  only `FactionId(0)` (`core_sim/src/orders.rs:22`) and worldgen hardcodes every band to it.

---

## Q1 — Same faction, or a new polity?

**A player-directed founding is always same-faction. Independence is never chosen; it is something
that happens to you.**

This is the arc's central decision and it is not a placeholder for multi-faction. Two reasons:

- **A colony you deliberately founded is yours.** "Start a life here" is the settle half of the
  settle/don't-settle fork. Offering the player a checkbox marked *make this an independent tribe*
  is offering them a button that reads *lose a band* — nobody presses it, and if they did, the
  interesting part (the drift, the resentment, the moment you notice they stopped answering) would
  have been skipped entirely.
- **Independence has to be earned by the world, not declared at launch.** The push that historically
  breaks a polity apart is that the bonds stop working: too far to trade with, too far to help, and
  nothing coming back. The sim already measures that, in two live signals — the supply network's
  connected components (`balance_supply_networks` links same-faction bands within a configurable
  reach and auto-balances stores; beyond reach a band lives off its own larder) and per-cohort
  `grievance`.

So: **one verb, two outcomes, and the second one is decided later by the sim.** A band that has been
outside its faction's supply network for a sustained stretch, and whose grievance is high, drifts
toward independence and eventually forks into a polity of its own. That is #284's emergent fission
and this doc hands it the trigger: **disconnection + grievance over time, never distance alone.**
Distance is a proxy; the supply network is the real thing, and it already accounts for reach,
throughput and friction.

**Sequencing.** The emergent half is **blocked on #513** (multi-faction support) — a band cannot
become independent when the registry holds one faction and every worldgen band is hardcoded into it.
Build the same-faction case first and completely; it is a whole feature on its own, and it exercises
every seam the independent case later needs.

**Consequence to design around, recorded now:** when a band *does* fork, the faction-flip line at
`population.rs:945` and the two `(faction, …)` bins above are the exact places that stop being
trivially true. #458 (cross-faction proximity trade) is the same seam approached from the other side.

## Q2 — What is the minimum viable group, and what floor must the parent stay above?

Two gates, and **both fire at the founding, not at the compose sheet.**

That placement is the decision. The existing party bound is deliberately permissive — the band is the
bound and the only one — and adding a floor at compose time would break a rule the raid and
expedition paths both rely on. It also isn't where the harm is: **detaching is reversible.** A party
that walks out and gets recalled folds its workers, its food and its pelts straight back
(`fold_party_into_band`). Nothing is irreversible until the moment the party stops being an
expedition. So that is the moment the sim checks:

- **Party viability** — the founding party must have at least `min_founding_workers` working-age
  people. Below that there is no labor pool to allocate and the new band is a death notice with a
  marker on it.
- **Parent viability** — the *parent as it stands right now* must clear two floors after the split
  is made permanent: at least `parent_min_workers` remaining, and a post-split dependency ratio
  `(children + elders) / working` no worse than `parent_max_dependency_ratio`. This is the guard
  against the #431 spiral: the failure mode there is dependents outnumbering workers, and hollowing
  out the home band to crew a colony is the fastest way to arrange it.

Evaluating the parent live at founding time — rather than freezing a verdict at launch — is what
makes this honest: the home band may have grown, starved or split again in the twenty turns the party
spent walking. **A refusal is a refusal to found, not a loss of the party** — it stays an expedition
in `AwaitingOrders`, and *recall* is still there.

**The compose sheet forecasts both gates and warns; it does not refuse.** Same pattern as the hunt
trip forecast: the sim exports the verdict, the client reads it. The player should see "founding
would leave the home band below its floor" before they walk twenty tiles, and should still be allowed
to walk if they mean to.

## Q3 — Who goes?

**Dependents may travel, and are not required to.** Both compositions are legitimate strategies, and
they trade against each other cleanly with mechanisms that already exist:

| Party | On arrival | The cost |
|---|---|---|
| **Workers only** (today's shape) | Every mouth produces. Strong, fast, immediately productive. | No children means no maturation inflow for many turns — the new band grows only from births by a founding cohort, from a standing start. |
| **A family group** | A real age pyramid: children maturing on the existing `maturation_rate` clock. | Arrives with a high dependency ratio and few workers — the #431 spiral, on the new band, on turn one. And it eats for everyone the whole way there (Q4). |

And it cuts on the parent's side too: **sending dependents relieves the parent's dependency ratio**
while sending workers worsens it. A struggling band has a genuine reason to send families — which is
exactly the historical push — and a comfortable one has a reason to send hands.

**No travel-speed penalty for dependents.** There is no per-band movement-speed concept in the sim —
`BandTravel` is a bare `target` (`core_sim/src/components.rs:1839`) and movement steps uniformly — so
a "families are slower" rule means inventing a speed mechanism to express a cost that provisioning
already expresses. The cost of bringing families is that you feed them the whole way and they cannot
work when they arrive. That is enough.

## Q4 — What do they carry? (the dowry)

Everything below is subtracted from the parent **at the founding**, not at launch — consistent with
Q2. Until then the party is carrying it, and a recall brings it home.

- **A seed larder sized for arrival plus establishment, not for a round trip.** Today's draw is
  `party_workers × distance × provision_draw_per_worker_per_tile` — a there-and-back budget for
  workers. A founding party needs `total_party_size × per_capita_draw × (distance +
  establishment_turns)`: every mouth including dependents, for the walk *and* for the window before
  the new band's own forage and hunt income comes up. `per_capita_draw` is the existing demographics
  lever (`demographics_config.json` → `consumption.per_capita_draw`, 0.16) — reuse it rather than
  minting a second consumption rate that can drift from the first.
- **The kit is inherited worn.** The new band takes a **copy of the parent's `BandEquipment` wear
  ledger**, not `BandEquipment::default()`. Otherwise founding a band mints a fresh kit out of
  nothing, permanently, and the intended pull into the crafting economy (`plan_early_game_labor.md`
  §TOE — running your kit dry *is* the pull) is trivially defeated by splitting. The splinter is
  exactly as worn out as the people it came from.
- **Knowledge is copied in full, not divided.** It already is — the party cohort is a clone. Knowledge
  is not a conserved quantity; people who know how to knap take that with them without the parent
  forgetting. Divergence afterwards is the Telling arc's business, not this one's.
- **The map goes with them.** The party keeps the `Discovered` view it left with plus everything it
  saw on the way. Already the documented intent (`plan_exploration_and_sites.md` §2, *knowledge as
  dowry*).
- **Breeding stock, if the parent has a pen.** Capped at `breeding_stock_fraction_max` of the pen's
  stock — a herd that can walk somewhere and still breed on arrival, without gutting the parent's
  pen. Gated on the parent actually having one; a forager band's dowry has no animals in it.

**The dowry is the decision.** Each line above is a real subtraction from a band the player has been
nursing for fifty turns, and the compose sheet's job (Q6) is to make that subtraction legible before
the player commits to it.

## Q5 — Why would you ever?

The push and the pull are both already in the sim; this arc just gives them somewhere to go.

**Push — the home tile stops being enough:**
- **Work range.** A band works ~19 tiles (`R` = 2). That is a hard ceiling on how much land one band
  can touch, and the only way past it is a second band standing somewhere else.
- **Carry capacity caps population** (`plan_early_game_labor.md` decision 7). The cap is the plateau;
  fission and storage are the two ways off it.
- **Forage depletion** around a long-parked band (the intensification arc) — the tiles you have been
  working get worse the longer you stay.
- **Per-hex crowding** (#277) and the dependency-ratio pressure of #431.

**Pull — somewhere better, that you actually found:**
- A **settle-site** discovery (`sites_config.json`, the *Settle site* category — a flagged
  good-to-root spot) is literally the reward this verb consumes. Wondrous Sites gave exploration
  something to find; this gives finding it a consequence.
- A better biome, unworked herds, a river.

**The shape of the choice.** #251 (roaming payoff) is the other half: staying mobile keeps your
options and your options' options; splitting spends people to hold two places at once. This arc is
what makes that a fork rather than a slogan.

## Q6 — What does the player see?

- **On the compose sheet** — the parent's **after** state, side by side with its now: workers,
  dependency ratio, larder, pen stock, and both Q2 gates as a forecast verdict. Reuse the existing
  compose-sheet forecast pattern (the hunt trip estimate is the model: the sim exports the answer,
  the client does no arithmetic). This is the screen where the player sees what leaving costs.
- **On the expedition panel** — *start a life here*, a third button beside *onward* and *recall*,
  enabled or disabled with a reason from the Q2 gates.
- **When it happens** — a new event kind on the **Alert** rung of `RUNG_BY_KIND`
  (`.claude/rules/client/event-dock.md`). The dock's own rule is that `died` and `migrated` sit at
  Notable *because they are things that happen to a band as a matter of course*; a founding is the
  opposite — rare, player-initiated, and the first act in the band economy that cannot be undone.
  Also a beat worth telling (`docs/plan_the_telling.md`).
- **The new band's name** — #271 owns the mechanism. The rule this arc asks for: a band founded on a
  **named site** takes that site's name; otherwise it draws from the faction's name pool; either way
  the player can rename it. A band the player will be looking at for the rest of the game should not
  be called *Band 2*.

---

## Config levers

A new `settle` block in `expedition_config.json`, beside `hunt` and `replenish`. Opening values are
dials to be tuned live, sized against the ~30-person / ~16-worker starting band
(`plan_early_game_labor.md` §Starting state).

| Lever | Opening value | What it does |
|---|---|---|
| `min_founding_workers` | 4 | Floor on the founding party's working-age at the moment of founding (Q2). |
| `parent_min_workers` | 6 | Workers the parent must still have after the split is permanent (Q2). |
| `parent_max_dependency_ratio` | 1.0 | Parent's post-split `(children + elders) / working` ceiling. The default start is ≈0.82 (30/55/15), so this leaves real headroom before it bites (Q2). |
| `establishment_turns` | 10 | The window the seed larder must cover beyond travel (Q4). |
| `breeding_stock_fraction_max` | 0.5 | Most of a pen's stock that may leave with the party (Q4). |
| `min_site_habitability` | *(matches the settle-site derivation threshold)* | The founding tile must be somewhere people can live. |

Deliberately **not** levers: the seed larder's consumption rate (reuses
`demographics_config.json` → `consumption.per_capita_draw`) and the party size bound (unchanged — the
band is still the only bound on *detaching*).

## Sequencing

1. **Design doc (this document).** ✅ #509.
2. **"Start a life here"** — #510. The arrival verb: the component swap, snapshot persistence of a
   mid-game founding (`sim_state.rs:487` must re-attach `ResidentBand` for a band worldgen never
   made), the event and feed lines, and the client affordance. Build it same-faction, with the party
   as it is composed today.
3. **Compose the founding party** — #511. The Q2 gates, dependents travelling, the Q4 dowry, and the
   compose sheet's parent-after forecast.
4. **Naming** — #271, generalized so it serves a founded band and not only the player's first one.
5. **Blocked on #513, then:** the emergent half — #284 (drift → independent polity, on the Q1
   trigger), #512 (scouts defecting to a better-off faction), #458 (cross-faction proximity trade).

Slices 2 and 3 are ordered this way on purpose: #510 makes the verb real with a party the sim can
already build, so the founding path is exercised end to end before #511 changes what a party is made
of.

## Cross-cutting touchpoints

- **The `ResidentBand` marker is a membership switch, not a label.** A long list of systems query
  `With<ResidentBand>` precisely so expeditions are excluded — supply pooling, sedentarization,
  migration, demographics, startup seeding, herd drift, the default-band command pickers. Gaining the
  marker means the new band **silently joins all of them on the turn it is founded.** Each needs a
  look for a band whose `age_turns` is small, whose stores are whatever the party was carrying, and
  whose position is nowhere near the parent. This is the largest hidden surface in the arc and #510
  owns it.
- **A founded band can launch parties of its own** — `handle_send_expedition` gates the *source* on
  `ResidentBand`, so recursion arrives for free the moment the swap lands. That is the arc working as
  intended; it also means the Q2 parent floors have to hold for a band that was itself founded
  fifteen turns ago.
- **Supply network** (`supply.rs`): two same-faction bands within reach pool their food automatically.
  A colony founded *inside* reach is a logistics extension of the parent; one founded outside is on
  its own from turn one. The player will feel this without being told, and it is also the signal Q1's
  independence trigger reads.
- **Snapshot / schema**: a mid-game founding must survive a rollback, which means the checkpoint path
  can no longer assume every `ResidentBand` came from worldgen.
- **Client**: the compose-sheet forecast, the third arrival button, the dock entry, the new band on
  the band/city dock's band list.

## Open items

- **The independence trigger's shape** (turns disconnected × grievance, and where the threshold sits)
  is specified here in kind but not in numbers. It cannot be tuned before #513 makes a second faction
  reachable, and guessing constants against an unexercisable path would be inventing a balance
  claim — #284 sets them when it can measure them.
- **Merging is not designed here.** The manual says *"Split/merge is allowed"*; this doc covers the
  split. Merge is the inverse of `fold_party_into_band` between two resident bands and is a separate
  slice — worth filing once fission ships and there are two bands to merge.
- **Scout-party wear is discarded on fold-back** — `fold_party_into_band` settles workers, food and
  trade but not the party's `BandEquipment`, so a returning expedition's wear evaporates. That is a
  pre-existing leak on the expedition path, not something this arc introduces; noted here because §4
  is where the wear ledger's ownership got decided. Out of scope for fission.

## See Also

- `docs/plan_exploration_and_sites.md` — §2, the detached-party machinery and the deferred breakaway
  this doc realizes; knowledge-as-dowry; the settle-site category that motivates the pull.
- `docs/plan_early_game_labor.md` — the band-as-labor-pool model, the TOE/wear economy §4 protects,
  the carry-capacity cap that is the push, and decisions 1 and 14 that defer here by name.
- `docs/plan_settlement_population.md` — §Migration (*"how colonists split off"*), the demographic
  brackets, and the supply network Q1's independence trigger reads.
- `shadow_scale_strategy_game_concept_technical_plan_v_0.md` — §Start of Game (*"the band splits into
  more bands as it grows"*), §Default Start Profile (*"split/merge is allowed"*).
- `core_sim/CLAUDE.md` → Scouting & Hunting Expeditions; `.claude/rules/core_sim/` for the per-arc
  engineering rationale once slices land.
