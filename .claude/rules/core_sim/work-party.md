---
paths:
  - "core_sim/src/work_party.rs"
  - "core_sim/src/systems/labor.rs"
  - "core_sim/src/supply.rs"
  - "core_sim/src/labor_config.rs"
  - "core_sim/src/data/labor_config.json"
  - "core_sim/src/demographics_config.rs"
  - "core_sim/tests/labor_allocation.rs"
---

# The work party — hunt and forage are one model

Design: `docs/plan_civilization_steps.md` §One work party. Engine: `core_sim/src/work_party.rs`
(the pure arithmetic) plus the posting seam in `core_sim/src/systems/labor.rs`.

## Config files

| File | Purpose |
|------|---------|
| `src/data/labor_config.json` | `porter_fraction_per_travel_tile` (**0.12**) — the share of a work party that carries rather than works, **per tile it walks unaided**, clamped at the whole party. Validated finite and `0.0..=1.0`. `band_move_tiles_per_turn` (**1**) is read a second time here as the party's own walking speed, and is now validated `>= 1` for that reason. Carry caps are a playtest dial |
| `src/data/supply_network_config.json` | Read, not written: `reach_tiles` is the distance a party pays **no** porters and **no** friction inside, and `friction` is the loss its flow takes past it. One producer for both readers — see "The road is the escape hatch" |
| `src/data/demographics_config.json` | Read, not written: `consumption.per_capita_draw × consumption.working_factor` is the party's upkeep, through the named seam `DemographicsConsumption::worker_draw` |

## A party is state ON the labor assignment, not an entity

`LaborAssignment::party: Option<WorkParty>`. There is no second cohort, no `ResidentBand`-less
entity and no merge verb, because **the workers never stopped being the band's** — which is the
whole architectural difference from an expedition (`expeditions.md`). Consequences that fall
straight out of it and are the reason the shape was chosen:

- **The row that staffed the party is the row that reports it**, so a work board has one place to
  look for every work item, near or far. The capture needs nothing handed in
  (`snapshot::population::labor_assignment_to_state` reads `assignment.party` directly).
- **`None` is what every command constructs.** A party is posted by the *turn*, from geometry — it
  is never ordered, and there is no haul command, no chosen destination and no launch gate.
- **It is outside `LaborAssignment`'s equality.** The type has a hand-written `PartialEq` for this:
  a rank is intent, a party is a fact about the world restamped every turn from the source's live
  position, and a derived `PartialEq` would have made the command no-op guard and the rollback
  record report *nothing changed* as a change on every turn the herd moved.
- **`set_assignment` carries it across the re-push**, like the rank and the keeping kit — and for a
  stronger reason than either: dropping it would restart the transit countdown on every `−`/`+`, so
  a posting the player kept adjusting would never deliver anything.

## ⛔ The local identity is the point, and it is what the threshold is chosen to protect

A source the band's own hands reach takes **no party at all**, and every number on that row is what
it was before any of this existed. Far work *falls out of* one model instead of sitting beside it
only while that holds.

`party_begins_past` is the single place the threshold lives, and it is **`band_work_range`, the same
for every job** — the band's apron, the distance its own hands reach without anybody walking goods.

⛔ **HUNT GETS NO LONGER THRESHOLD THAN FORAGE, and that is the point of the slice.** A Hunt row used
to survive out to `LaborConfig::hunt_reach()` — `band_work_range + hunt_leash_tiles` — and the design
doc is explicit about what that number was: *"the 5 was set so the herd did not roam out of range once
a hunt was set up — in hindsight, a patch over the wrong model."* A party that follows its herd never
roams out of range, so the patch has nothing left to fix, and keeping it would leave the two jobs
measuring distance differently — the three-systems problem this arc exists to delete, surviving in
miniature.

**So this is a deliberate behaviour change rather than an additive one**: a hunt three to five tiles
out was free and instant, and now costs a porter or two and a short walk out, exactly as a forage row
at that distance always would have. The local identity is untouched — it lives at `travel_tiles == 0`,
which is inside `band_work_range` for both jobs.

**`hunt_leash_tiles` and `hunt_reach()` are dead levers**, left in place and still validated because
deleting a config key is a fingerprint change belonging to the slice that retires the expedition path.
Nothing lapses for distance any more, and nothing reads the leash to decide a party either.

Pinned by `labor_allocation::a_hunt_inside_the_old_leash_posts_a_party_on_the_same_apron_as_forage`,
which stages a herd **strictly between** the two old thresholds — the only distance at which the two
readings disagree, and therefore the only one that can fail. Its sibling
`::a_hunt_past_the_leash_posts_a_party_the_band_must_then_supply` stages seven tiles and passes under
either rule, so it cannot stand in for this one.

Pinned by `labor_allocation::a_local_row_takes_no_party_and_its_whole_take_reaches_the_larder`,
which asserts the published `actual` against the larder credit in **fixed point** — the one place a
silent haircut anywhere in the party path would show up — with a liveness assertion beside it,
because "nothing was lost" also passes when the patch paid nothing.

## What distance costs — three terms, each on the distance it is about

| term | charged on | seam |
|---|---|---|
| **porters**, out of the party itself | tiles beyond the **free reach** | `work_party::porters` |
| **friction** on what comes home | the same tiles | `work_party::arriving_fraction` |
| **a transit delay** | the **apron-measured travel** distance | `work_party::transit_turns` |

`travel_tiles = max(0, hex_distance − band_work_range)` — measured to the apron, in hex steps like
every other radius in the sim. `porter_tiles = max(0, hex_distance − free_reach)`.

**The two distances are different on purpose.** A road widens the reach a link holds itself open
at; it has not moved the source. So a trail takes the porters and the friction to zero while the
walk out is unchanged.

**The range cap is the clamp, not a lever.** Far enough out every hand is carrying and the posting
produces nothing — a cap nobody had to pick a number for.

### The road is the escape hatch, and it reads the supply network's own producer

`free_reach` is `supply::free_pooling_reach_tiles` — `reach_tiles`, or what the road between the
endpoints widens it to, whichever is greater. That function is `supply::link_holds` factored in
two: the pooling test is literally `distance <= free_pooling_reach_tiles(..)`, so **the reach a
party pays porters past and the reach two camps pool inside can never drift apart**, and the
weakest-tile rule (`routes::path_reach_tiles`) applies identically to both.

That is the promotion the design turns on: automatic pooling is the only road traffic there is, so a
posting worked steadily wears a trail that eventually covers the run, the porters go back to
producing and the link pools for free. The distance bite and its escape hatch are one mechanism.

## Who eats, and which way goods flow

**The take feeds the party first; the remainder is surplus; the shortfall is a deficit the supply
line covers.** `work_party::PartyFlow::settle_food` is the whole of it and asks **nothing about the
job** — a hunt party's take happens to be edible and a stone party's does not. A branch on target
kind at that seam is the design violated.

> ### ⛔ THE EATEN SHARE IS CREDITED HOME — it is not a second meal
>
> The band's population consumption already feeds these workers: they never left the cohort, and
> `simulate_population` charges the whole `working` bracket. So food the party ate **at the source**
> is food the band did not have to carry out, not food that vanished. Netting it out again here
> would bill the band twice for one meal and make a far posting read as a loss it is not.
>
> What eating on the spot actually buys is the **friction on that share**, which is why only the
> *surplus* is multiplied down: `home = ate + surplus × arriving`. At zero distance `arriving` is
> `1.0` and `home == produced`, which is the local identity.

**The deficit is stamped when the party is POSTED, not when it takes.** A party owes its upkeep
before it has taken anything, and an arm that returns early — a source in a state the arm declines
to work, a posting so far out that every hand is a porter — never reaches a take site. Left to the
take, such a posting reported a deficit of zero, which reads as *fully supplied*: the one state
that must never be assumed.

### ⛔ The take flows HOME, always

To the band that owns the row, **never** to whichever band the party is standing beside. A party is
an *extension of its home band*, not a peer node in the supply network; making it a peer is exactly
the bug the asymmetry exists to prevent. Feeding is the other direction and is the component's
business — what the home band then does with the food is `balance_supply_networks`' affair as it
always was.

Pinned by `labor_allocation::a_partys_take_is_credited_to_its_home_band_not_to_the_band_beside_it`:
two bands of one faction, both opening empty, the party working a patch the *other* band is camped
on, and every unit landing at home.

## One flow, degraded — not a shipment

Goods flow both ways along the home tie every turn. The transit delay is modelled as **a pipeline
priming once**: while `turns_to_first_arrival` counts down, what the party produces accumulates in
`pack_food` and the band is credited nothing; on the turn the line opens the pack is handed over
whole and every turn after that flows. Nothing produced on the walk out is lost, and a posting that
ends early walks home with its pack.

**The walk happens once.** A standing posting keeps its own countdown rather than restarting it from
today's distance — a herd drifting further costs porters and friction, not a second walk. That is
what makes this a pipeline and not a trip.

**Steady state and "amortized over the cycle" coincide, deliberately**, so the row prints one number
and not two. It is `netRateHome`, and it is the row's own `realized` put through `settle_food` — the
forward projection states **what arrives**, not what is taken, or a far posting would promise a
larder food that is still being eaten at the source or lost on the road. The arrival schedule is
scaled by the same share so it keeps its shape; the lumpiness is the quantiser's and nothing here
reshapes it.

## A party that cannot be supplied walks home

The deficit has to cross the same distance the take crosses, so the band's larder must hold it
**grossed up by the friction the outbound leg loses** (`PartyFlow::larder_needed_to_supply`). Judged
against the larder as the band's pass **opened**, for `settle_pen_hay`'s reason: taking it live
inside the walk would make *"can this band still feed its party"* depend on the row's place in
`assignments`, and `set_assignment` re-pushes an edited row to the end.

Failing it folds the row back — pack handed to the band, workers returned to the pool, queue entry
pruned with the row — and says so on the source's own feed channel:

```text
status=recalled reason=unsupplied x=… y=… travel=… deficit=…
```

**`recalled` is a new status token and it is NOTABLE, not an Alert.** `status=lapsed` is ranked
Alert because *the row was destroyed and its queued build went with it*; a fold-back is the posting
ending and the food already spent, with the pack and the workers coming home. It needs its own
`DETAIL_STATUS_STYLE` row on the client (`.claude/rules/client/event-dock.md`).

> #### ⛔ `take_crew_present` ASKS WHAT THE PLAYER STAFFED, not what is left after the porters
>
> The assignment loop's `workers` is the **working crew**, and at enough distance every hand is a
> porter. Read from that, a party walking a full load home answers *"nobody is on this row"*, and
> the holding test retires the row **silently**, with the party still out there. The row holds a
> posting; the arithmetic beneath already resolves a zero working crew to a zero take on its own.

## `BandReach` no longer asks about distance

It used to carry the band's position and the two lapse distances, because the arms abandoned an
out-of-reach row on the same `continue` every keeping draw and material spend sat beneath. Nothing
lapses for distance now, so the only thing left that makes an arm skip is **a herd the registry no
longer carries**. The type survives rather than collapsing into a bare `registry.find`, because
*"will the arm reach this row"* is the question the settlements must go on asking, and a second
reason to skip would land in one place and reach all of them.

## What this slice deliberately did not do

- **The expedition path is untouched** — retiring it is its own slice. Both models run side by side.
- **Only the FOOD account is routed through the party.** Fodder and material batches are credited to
  the band as they always were: a batch carries a characteristic vector and a band key, and a pipe
  over those is the storage arc's, not this one's.
- **The assign-time seed still declines a far row** (`bin/server.rs`'s `seed_source_yield` returns
  early past range), so a fresh posting seeds a zero until its first resolved turn. That reading is
  *honest* rather than merely absent — a party still walking out genuinely delivers nothing — but
  the steady figure does not appear until the turn resolves.
- **`drop_source_row` does not hand the pack back.** An explicit `abandon` of a far row loses what
  the party was carrying (at most `transit_turns` turns of take); the fold-back and the unassign
  paths both deliver it.
