# Plan: Band Fission — a splinter group leaves and starts a new band

Status: **The arrival verb ships; the party it sends is still today's.** `settle_expedition` (#510)
founds a resident band, same-faction, gated on **reachability** and on a **minimum founding party**
(`min_founding_workers`, live on the Workbench). What a founding party is *made of* — the parent-side
viability gates, dependents travelling, the dowry and the remaining dials — is #511. The authoritative spec for arc
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
- **`AwaitingOrders` had exactly two exits** — re-aim via `move_band`, or `recall_expedition`.
  ✅ **#510 added the third**, `settle_expedition`, which is the whole of this arc's opening slice.
- **No code path created a resident band at runtime.** `ResidentBand` was inserted in exactly two
  places: worldgen (`core_sim/src/systems/worldgen.rs:3018`) and checkpoint restore
  (`core_sim/src/sim_state.rs:487`). ✅ **#510 added the third**,
  `systems::expeditions::found_band_from_expedition` — which is why `components.rs`'s note on the
  marker now reads "the *other* two places".
- **A party is 100% working-age.** The launcher clones the parent cohort, then sets
  `children = 0`, `working = party`, `elders = 0` (`server.rs:2937–2939`). Nobody's family goes.
- **The party bound is availability and nothing else** — `1..=available_workers(cohort.working)`,
  and the comment above it is explicit that this is deliberate: *"you cannot detach workers you do
  not have, and you may detach all the ones you do."* There is no floor and no parent check.
  **Still true, and deliberately** — #510's `min_founding_workers` floor fires at the *founding*, not
  at the detaching, exactly as Q2 places it. Walking out is still free and still reversible.
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

## Q2 — Who may leave, where may they go, and what floor must the parent stay above?

Three gates, and **all three fire at the founding, not at the compose sheet.**

That placement is the decision. The existing party bound is deliberately permissive — the band is the
bound and the only one — and adding a floor at compose time would break a rule the raid and
expedition paths both rely on. It also isn't where the harm is: **detaching is reversible.** A party
that walks out and gets recalled folds its workers, its food and its pelts straight back
(`fold_party_into_band`). Nothing is irreversible until the moment the party stops being an
expedition. So that is the moment the sim checks:

- **Party viability** — the founding party must have at least `min_founding_workers` working-age
  people. Below that there is no labor pool to allocate and the new band is a death notice with a
  marker on it. **Shipped in #510** alongside reachability, out of sequence: playtest founded a
  colony with **one** person, and a verb that ships without this gate is a verb that teaches the
  player the wrong price for it.
- **Parent viability** — the *parent as it stands right now* must clear two floors after the split
  is made permanent (**#511**): at least `parent_min_workers` remaining, and a post-split dependency ratio
  `(children + elders) / working` no worse than `parent_max_dependency_ratio`. This is the guard
  against the #431 spiral: the failure mode there is dependents outnumbering workers, and hollowing
  out the home band to crew a colony is the fastest way to arrange it.
- **Reachability** — the founding tile must connect, through tiles the faction has **discovered**, to
  a tile held by one of its **resident** bands. See below. **Shipped in #510.**

Evaluating the parent live at founding time — rather than freezing a verdict at launch — is what
makes this honest: the home band may have grown, starved or split again in the twenty turns the party
spent walking. **A refusal is a refusal to found, not a loss of the party** — it stays an expedition
in `AwaitingOrders`, and *recall* is still there.

**The compose sheet forecasts these gates and warns; it does not refuse** (#511). Same pattern as the
hunt trip forecast: the sim exports the verdict, the client reads it. The player should see "founding
would leave the home band below its floor" before they walk twenty tiles, and should still be allowed
to walk if they mean to.

**The two gates #510 ships already forecast, on the ARRIVAL affordance rather than on the compose
sheet.** A refusal delivered after the press is honest but late — playtest pressed *Start a life
here* with one worker and read the reason in the event dock seconds later — so the sim publishes its
verdict (`PopulationCohortState.foundingRefusals`) and the button greys out carrying **every** reason
that applies. That the client is told rather than deciding is the same rule the hunt forecast follows:
it holds neither `min_founding_workers` nor the faction map's connectivity, and a client-side copy of
either is a second model that can disagree with the sim.

**All applicable reasons, never the first one.** A one-worker party on unmapped ground fails both
gates and is told both. Reporting one at a time means the player fixes it, presses again, and
discovers the next — learning the rules by a sequence of refusals rather than reading them.

What #511 still adds is the forecast on the **compose sheet**, before the party walks: the parent
floors it introduces are the ones you most want to see *at* twenty tiles' distance rather than after
them.

### Reachability — you can only settle ground you can point at

**The founding tile must connect to one of the faction's resident bands through tiles the faction has
discovered.** Not distance, not supply reach, not terrain quality — a contiguous run of mapped ground.

The rule *is* the fiction rather than a balance lever sitting on top of one. A splinter group
announcing where it means to go can only do that by naming a place both halves of the band know:
*we are going over to that valley we found.* Ground nobody has mapped cannot be named, so a party that
walks twenty tiles into the unknown and stops has not founded a colony — it is a party you have lost
track of. Refusing to call that a band is the honest answer, not a restriction.

It also earns its keep mechanically. Q1 hands the independence trigger to the supply network's
connected components, and #511's dowry is measured in walking distance; both assume the colony is a
place the parent could actually reach. A founding that skips this gate produces a band the rest of the
arc has no way to reason about.

**What "discovered" means here is the faction map, and that is not incidental.** An expedition is
excluded from live fog reveal (`Without<Expedition>` in `calculate_visibility`) because discovery is
comm-range gated: a party buffers what it sees into a private `pending_reveal` and promotes it to the
faction map only on coming back within `comm_range_tiles` of its home band. So the corridor a scout
walked is not mapped until the scout reports, and neither is the tile it is standing on. The loop that
falls out — **scout out, report, then send the founding party along ground you now hold** — is already
how the pull works: a settle-site is recorded when its tile becomes Discovered for the faction, at
that same flush. The Verdant Basin you settle is one you learned about by the scout coming home.

Rules:

- **Resident bands only.** A party cannot anchor a path to another party — that would let two
  expeditions bootstrap a colony out of ground neither has reported.
- **Land only.** Water is discovered like anything else, but a path across a mapped strait would
  qualify a colony nobody can walk to. The party's own tile is land by construction
  (`send_expedition` validates a land target).
- **Evaluated live at the founding**, like the parent floors above and for the same reason: the home
  band moves, and so does the frontier.
- **No length bound.** The discovered set is the bound.

### There is deliberately NO habitability gate

The founding tile's quality is **not** checked, and an earlier draft of this document was wrong to
reserve a `min_site_habitability` lever for it (it proposed reusing `fertile_settle`'s
`max_habitability_pressure`, 0.02 — which is exactly the client's *Hospitable* ceiling, so it would
have refused every tile the tile card rates merely *Fair*).

**Founding raises a party to a band that can forage, hunt and move on its own. That is the entire
effect.** It does not root anyone: a band is mobile, so settling harsh ground is a mistake the player
can walk out of, and pricing it as an illegal move confuses *bad idea* with *impossible*. The land's
quality already speaks through every system that reads it — morale drain, forage and pasture yield,
carrying capacity — which is the feedback that belongs on this decision, delivered where the player
can act on it.

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

**No travel-speed penalty for dependents.** Speed is not a property a band has. `BandTravel` is a
bare `target` (`core_sim/src/components.rs:1839`) and every band moves at the single global
`labor_config.band_move_tiles_per_turn` — echoed per-cohort on the wire, but sourced from one number
for everyone. So "families are slower" means **making movement speed per-band**, new machinery built
to express a cost that provisioning already expresses. The cost of bringing families is that you feed
them the whole way and they cannot work when they arrive. That is enough.

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
- **The culture goes with them, and it costs the parent nothing.** *(built)* The colony's culture
  layer is attached at the founding, seeded from the **home band's** current traits and parented on
  the province it landed in (`CultureManager::attach_band_from_source`). The precedent is migration:
  a band that *walks* twenty tiles keeps the culture it arrived with and chases its new province at
  the band scope's elasticity, and a party that settles twenty tiles out is the same journey — it
  would be strange for the walk to preserve a people and the founding to replace them with the
  locals. Like knowledge, culture is not conserved, so this is a line in the dowry the player pays
  nothing for; unlike knowledge, it starts diverging immediately, because the colony mints its own
  character offset and lags toward a *different* province than the parent's.

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
(`plan_early_game_labor.md` §Starting state). **Only two numbers below are derived rather than
chosen** — `per_capita_draw` (0.16) and `food_reserve_days` (20), both already in
`demographics_config.json`; the rest are opening positions that want playtest. The failure columns
say which way each one breaks, because that is what tells you which direction to move it.

| Lever | Opening | What it means | Too low | Too high |
|---|---|---|---|---|
| `min_founding_workers` | **4** | The founding party must hold at least this many **working-age** people at the moment of founding (Q2). **Shipped — #510.** | One or two people can found a band that cannot staff a single food role — a death notice with a marker on it. | At 8+ a split costs half the home workforce, so the verb ships and is never used. |
| `parent_min_workers` | **6** | Workers the **parent** must still have once the split is permanent (Q2). | The home band can be hollowed out to crew a colony, killing both. | Only an already-large band can split, which pushes the first fission very late. |
| `parent_max_dependency_ratio` | **1.0** | The parent's post-split `(children + elders) / working` ceiling — at most one dependent per worker. The #431 spiral guard. | The default start is ≈0.82 (30/55/15), so anything under ~0.9 means you can barely ever split. | At 1.5 you can leave home with three workers feeding five mouths — the spiral, arranged deliberately. |
| `establishment_turns` | **20** | Turns of food the seed larder must cover **after arrival**, on top of the walk: `total_party_size × per_capita_draw × (distance + establishment_turns)` (Q4). How long the colony has to bring its own forage and hunt income up. | Every colony starves on arrival however good the tile is. | The dowry guts the parent's larder, so splitting is gated on food you rarely have. |
| `breeding_stock_fraction_max` | **0.5** | The most of a pen's stock that may leave with the party (Q4). | At 0.1 the animals that go cannot breed a viable herd — pastoral colonies are impossible. | At 1.0 one command empties a pen the player spent many turns filling. |

**The reachability gate has no lever, and wants none.** It is a yes/no question about the faction map
(Q2), so there is no number to tune — the dial that moves it is `comm_range_tiles`, which already
exists and belongs to scouting.

**`establishment_turns` defaults to 20 to match `startup.food_reserve_days`.** Worldgen already
answers *how much food does a brand-new band need in order not to immediately die* — it seeds every
band it spawns with 20 turns of reserve (`demographics_config.json`, `startup.food_reserve_days`).
Choosing a different number here would be a second, quieter answer to the same question, which is the
drift the `per_capita_draw` reuse below exists to avoid. It stays a **separate** lever only so
splitting can be priced above or below worldgen's gift as a balance decision, not by accident.

*What that costs in practice:* a party of 6 walking 8 tiles draws `6 × 0.16 × 28 ≈ 27` food, against
a healthy 30-person band's `30 × 0.16 × 20 = 96`. Under a third of a full larder — a subtraction the
player feels, not a prohibition.

Deliberately **not** levers: the seed larder's consumption rate (reuses
`demographics_config.json` → `consumption.per_capita_draw`) and the party size bound (unchanged — the
band is still the only bound on *detaching*).

### Every settle dial ships in the Workbench

These are numbers nobody can pick correctly at a desk — they are picked by playing. So they are
**not** decided in this document; the values above are opening positions, and the slice's real
obligation is to make them **live-adjustable in the Workbench's config tuning page** so playtest
moves them without a rebuild.

The surface already exists and already covers this file. `ConfigTuningPage` is manifest-driven
(`.claude/rules/client/workbench.md`), and `clients/godot_thin_client/src/config/tuning_manifest.json`
already carries an **`expedition`** kind bound to `EXPEDITION_CONFIG_PATH`. Each dial is therefore one
manifest row, not new machinery:

| pointer | type | min · max · step | default | unit | hint |
|---|---|---|---|---|---|
| `/settle/min_founding_workers` | int | 1 · 12 · 1 | 4 | workers | Working-age floor a party must clear to found a band. **Shipped — #510**, with the gate it governs. `1` is the "off" setting and is still a real party. |
| `/settle/parent_min_workers` | int | 0 · 20 · 1 | 6 | workers | Workers the home band must keep after the split. **0 turns the gate off** for a playtest run. |
| `/settle/parent_max_dependency_ratio` | float | 0.5 · 2.5 · 0.05 | 1.0 | — | Dependents per worker the home band may be left with. |
| `/settle/establishment_turns` | int | 0 · 60 · 1 | 20 | turns | Food the seed larder carries beyond the walk. |
| `/settle/breeding_stock_fraction_max` | float | 0.0 · 1.0 · 0.05 | 0.5 | — | Share of a pen's stock that may leave with the party. |

The ranges are wider than the values are likely to want, deliberately: a dial you cannot push past
where it plays well cannot show you *why* it plays well. `min_site_habitability` gets no row — it
reuses the settle-site threshold rather than being a number of its own.

## Sequencing

1. **Design doc (this document).** ✅ #509.
2. **"Start a life here"** — ✅ #510. The arrival verb: the component swap, the **reachability gate**
   and the **party floor** (`min_founding_workers`, with its Workbench row — the two of Q2's three
   gates that ship here; the party floor came in on playtest, which founded a colony with one
   person), snapshot persistence of a mid-game
   founding (`sim_state.rs:487` must re-attach `ResidentBand` for a band worldgen never made), the
   event and feed lines, and the client affordance — **which forecasts both of its gates**, since the
   sim publishes the whole refusal set per party and the button greys out carrying every reason.
   Build it same-faction, with the party as it is composed today.
3. **Compose the founding party** — #511. The Q2 **parent** gates, dependents travelling, the Q4
   dowry, the **compose-sheet** forecast of all three gates (#510 forecasts its two on the arrival
   affordance; #511 moves the warning to before the party walks), and the remaining four
   `tuning_manifest.json` rows — the dials land
   in the Workbench **with** the gates they govern, not as a follow-up, because a gate that cannot be
   moved during a playtest cannot be judged during one.
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
  the band/city dock's band list, and the five `settle` rows on the Workbench config tuning page.

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
