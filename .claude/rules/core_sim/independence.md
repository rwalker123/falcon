---
paths:
  - "core_sim/src/systems/independence.rs"
  - "core_sim/tests/band_independence.rs"
  - "core_sim/src/wellbeing_config.rs"
  - "core_sim/src/data/wellbeing_config.json"
---

# Independence — a cut-off, aggrieved band becomes its own people (#284)

Design of record: `docs/plan_band_fission.md` §Independence. Engine:
`core_sim/src/systems/independence.rs` (`advance_band_independence`, `grow_faction_roster`,
`HeartLedger`). Defection is your people choosing a better-off people; independence is your people
choosing **no one**. One verb splits a band (always same-faction, `fission.md`); whether a far band
stays yours is decided later, by the sim.

## Config files

| File | Key | Purpose |
|---|---|---|
| `src/data/wellbeing_config.json` | `independence.grievance_threshold` (**1.0**) | People-weighted mean `grievance` at which a cut-off group becomes its own people. Serde-default like the other wellbeing blocks; validated finite and `>= 0` in `WellbeingConfig::from_json_str` (`WellbeingConfigError::Invalid`). `0` is a real setting — a group leaves the turn it is cut off. Staged from the Workbench as the `wellbeing` tuning kind (`ConfigOverrideKind::Wellbeing`, proto `CONFIG_OVERRIDE_KIND_WELLBEING = 8`) |

The clock is the contact tie's own bleed (`connections_config.json` → `strength.*`, `connections.md`)
and the gate is the discontent block's `grievance` (`campaign.md` → Civilization Wellbeing). This
block adds the one number that joins them and no second counter.

## The heart, and what "cut off" means

`advance_band_independence` runs in the Population chain **after `advance_population_migration` and
`advance_party_defection`**, so every band's people is final for the turn and its grievance is this
turn's. It reads the contact ledger as the previous Visibility stage left it — the supply network's
arrangement.

- **Groups.** Per people, its `ResidentBand`s with a `BandId` are joined wherever
  `ConnectionLedger::tie_is_live(a, b)` holds — undirected, the pooling rule's reading. Expeditions
  are never members.
  The grouping is `lineage::tie_joined_groups`, **shared with the breeding ceiling**
  (`resolve_breeding_ceilings` groups a people's bands into breeding populations with the same
  function), so there is one notion of "a people's bands in touch".
- **The heart** is the group holding the most people (`cohort.total()`), ties to the group with the
  lowest `BandId`. A people with one band is its own heart and can never be cut off.
- **Cut off** = a band outside its people's heart.
- **Order-independent by construction**: bands are sorted by `BandId` before anything is computed,
  groups come out in the order of their lowest `BandId`, and that is the order break-aways are
  processed in.

## The break-away

Every non-heart group whose **people-weighted mean grievance** (`Σ grievance × people / Σ people`)
is `>= independence.grievance_threshold` becomes **one** new people — all its bands together. Two far
bands still tied to each other are one group, so one far cluster never becomes two peoples on two
turns. Several groups on one turn each found their own people, lowest `BandId` first.

For each breaking group:

| Step | What happens |
|---|---|
| the roster | `grow_faction_roster` — `FactionRegistry::add_ai_faction` (the next positional id, `FactionControl::Ai`), then every roster-derived resource (below) |
| knowledge | the new people's `DiscoveryProgressLedger` is seeded from the **old people's ledger in full** and every fragment any of the group's bands holds in `cohort.knowledge`, each discovery at the **best** progress any source has — knowledge is not conserved, the old people forgets nothing, and nobody left them, so nothing is scaled |
| the map | `VisibilityLedger::insert_remembered_copy(old, new)` — the old people's map with every `Active` tile demoted to `Discovered` (its `last_seen_turn` kept). Presence is rebuilt by the next sight sweep from where the new people's own bands stand |
| each band | `cohort.faction` = the new id, `cohort.grievance` = 0 (it was a grievance against the people they left), a `band_broke_away` pair pushed |
| what is its own | `follow_the_band_to_its_new_people` — defection's path (`factions.md` → "A band that changes people takes what is its own"): roads, improvements only it works, parties out, a name re-minted only on a clash |
| ties | untouched — a connection is band-keyed and faction-blind, so they still know you and you them |

The pass then re-groups against the post-break-away roster and writes every band's reading.

### Why the seed reads the old people's ledger, not only the bands

`cohort.knowledge` holds the start profile's fragments and whatever migration has merged into the
band. Lessons a people earns by practice (the intensification ladder's knowledges — Cultivation,
Herding, Foddering…) are credited to the **faction's** `DiscoveryProgressLedger` and never to a
band's `knowledge`. Seeding from the bands alone would leave a break-away unable to work the Field it
worked the turn before; "they keep everything they knew" needs the ledger. Pinned by
`a_lesson_the_old_people_earned_by_practice_goes_with_the_break_away`, which credits Cultivation to
the old people only (the band holds a lesser fragment of it, so the max is exercised too).

## The roster grows at runtime — ONE function

`grow_faction_roster(&mut RosterResources, map_seed, parent)` is the one statement of the runtime path,
extending the set `factions.md` → "THE ROSTER-DERIVED SET IS SIX RESOURCES" names, each through the
seam the boot constructor uses:

| Resource | Runtime extension |
|---|---|
| `FactionRegistry` | `add_ai_faction` |
| `TurnQueue` | `add_faction` — the roster only; `advance_turn` awaits it from the **next** turn, so the turn in flight is not stalled |
| `CounterIntelBudgets` | `seed_faction` (what `new` loops over) |
| `FactionSecurityPolicies` | `seed_faction` (Standard) |
| `FactionBorderPolicies` | `seed_faction` (open) |
| `FactionNames` | `mint_faction` — worldgen's one-per-world permutation under the world's `map_seed`, so the new id takes the name it would have had at creation |
| `EspionageRoster` | `seed_from_catalog(&[new])` |
| `FreeBreedingPeoples` | `inherit(new, parent)` — a people born from a latched people breeds freely too (`campaign.md` → "The breeding ceiling") |

**The other faction-keyed state was swept and needs no row.** `SedentarizationScore`,
`GreatDiscoveryReadiness`, `ObservationField`, `DiscoveredSites`, `KnowledgeLedger` and the band-name
counters (`BandNameAllocator`) are filled lazily on first write; `VictoryState` is rebuilt from the
registry every `victory_tick`; `SimulationMetrics` is derived; `FactionInventory` reads an absent
faction as an empty stockpile, which is the truth; `SeatRegistry` and the capture's per-seat
publication state are made on a claim; The Telling samples faction 0 alone. **`StartLocation` is not
extended**: a broken-away people has no opening ground, so its frame carries no start marker.

### The roster is checkpoint state now

`FactionRegistry` moved from `WorldStatics` to `SimState::factions` (save format 22). Left a world
static, a rollback past a break-away kept the grown roster and the replay grew it a second time — two
peoples for one break. `apply_save` and the rollback both rebuild `TurnQueue` from the restored
registry.

### Seats

`bin/server.rs`'s `resolve_ready_turn` compares the roster across `run_turn` and, when it grew (and
the server is not replaying), calls `announce_roster` — the same `seats.roster` event and run-record
description every world build emits (`launcher.md`), so the launcher starts a `sim_ai` for the new
seat the turn it is born. The seat is vacant until claimed, and a vacant seat never holds the turn. A
rollback announces the roster it restored when that differs from the one it replaced.

## A split starts fully tied — they were one band

`split_band_from_parent` writes `ConnectionLedger::insert_full_tie` for parent → child and child →
parent at `FULL_TIE`, stamped with the split's turn and tile, and copies the parent's `HeartReading`
to the splinter (`HeartLedger::inherit`). Without the ties the next turn's heart pass — which runs
before that turn's sight sweep — would read a splinter standing on its parent's tile as cut off. The
ties then bleed on the ordinary clock. This is the one place outside the sight sweep that writes a
tie, and it writes kinship, not contact.

## The readings, and the lost-touch edge

`HeartLedger` (`BTreeMap<BandId, HeartReading>`) is rebuilt whole every turn and is **checkpoint
state** (`SimState::hearts`): the lost-touch line is an edge read off the previous reading, and the
capture publishes the readings before a restored world runs a turn.

`HeartReading { faction, cut_off, bond, last_contact_turn }` — `bond` and `last_contact_turn` are
read off the ties between the band and the **other** bands of its people's heart (the strongest
strength, the latest `last_contact_turn`, either direction). A band that is the whole of its heart
reads `FULL_TIE` and the current turn. A heart member therefore reads the very tie that holds it in
the heart, draining while nobody visits; a cut-off band's live ties to the heart are none by
definition, so its `bond` is a parked edge's `0` and its `last_contact_turn` says when it was last
seen.

**Why a heart member does not simply read `1.0`.** A band with *any* live tie to the heart is *in*
the heart, so the drain this field exists to show — the Bond bar emptying while nobody visits — is
only ever visible inside it. Pinned at `1.0` for every heart member, `heartBond` would be `1.0` or
`0` and nothing between: a copy of `cutOff`, with the drain the design asks the player to watch never
on screen. Only a band that is the whole of its heart, with no other band to be tied to, reads `1.0`.

**Lost touch fires once, on the transition**, under the band's own people: a band whose previous
reading was in touch, **of the same people**, that is cut off this turn. A band no turn has judged
has no previous reading and fires nothing (a fresh world, a load); a band that changed people this
turn is not losing touch with the people it left.

## A purist band that diverges too far splits off (#702)

The second way a band breaks away, and it needs no cut-off and no grievance. `reconcile_culture_layers`
(Influence) reads this turn's band-scope `SchismRisk` records. A band is **queued** (`CultureSplitQueue`,
band -> the people it was judged as) when its Syncretic<->Purist value is **above**
`culture.split_min_purist` (default `0.0`) **and** its people has at least one other resident band.
An accepting band (at or below the lever) never splits: its kin's contact drift is the absorption. A
people of one band has nothing to break away from, which also stops a just-split band re-splitting.

`systems::advance_culture_splits` drains the queue **directly after `advance_band_independence`** in
the Population chain, so every band's people is final for the turn. It re-checks that the band's
people still equals the one culture judged (independence, a defection or a remnant flip may have
moved it) and that a sibling remains, then breaks that ONE band (not its tie-joined group) away
through `break_away_as_new_people` — the per-group hand-over `advance_band_independence` shares
(roster growth, discovery seed, remembered fog, `cohort.faction`, grievance reset, both peoples
told), then `follow_the_band_to_its_new_people`. Its culture layer is untouched. Independence has
already rebuilt the turn's `HeartLedger`, so the split writes the band's reading as a sole-band
heart's.

The announcement is `band_broke_away` with the same two rows and `side=` tokens; the detail also
carries **`cause=culture`** and the label reads as culture (*"... grew too far from our ways ..."*).
The grievance break-away carries no `cause` token. **The player is warned first:** a band in the
drift-warning state (`ticks_above_soft` at its trigger) that passes the same predicate
(`culture::may_break_away`: purist above the lever and a sibling) is published as
`cultureBreakAwayRisk:bool` on its cohort row (own bands only; `CultureManager::break_away_risk`,
checkpointed, save format 32), because band tensions never reach `active_tensions`. The queue is a derived resource (filled and
drained within one turn), listed in `sim_state_coverage.rs`'s `DERIVED_RESOURCES`.

## Events

| Kind | Rows | Label | Detail |
|---|---|---|---|
| `band_broke_away` | two per band, one per people (`push_band_changed_hands_events`' shape) | lost: *"Band 7 no longer answers to us — they call themselves Faction 1."* / gained: *"Band 7 broke away from Faction 0 — we answer to no one but ourselves."* | `band= from= to= side=lost\|gained` |
| `lost_touch` | one, the band's own people | *"We have lost touch with Band 7."* | `band=` |

Labels name the durable ids (`band_label` / `faction_label`); the client substitutes names by joining
on the tokens (`band-names.md`). Neither kind reaches The Telling — `band_changed_hands` does not
either.

## On the wire

`PopulationCohortState`, appended last: `cutOff:bool`, `heartBond:float` (0..1),
`heartLastContactTurn:long` (`sim_schema::state::NO_HEART_CONTACT` = `-1` when no tie joins the band
to its heart), and `independenceGrievanceThreshold:float` (the config echo, the `foundingMinWorkers`
idiom). Read off `HeartLedger` at capture, never re-derived. A band no turn has judged — and a
detached party, never a member — publishes no reading (`false`, `0`, `NO_HEART_CONTACT`). Own-band
fields: a redacted foreign row carries the defaults (`factions.md` → the allow-list).

## Tests

`core_sim/tests/band_independence.rs`: a far aggrieved band becomes a new AI people and both peoples
are told (the queue awaits it next turn, not this one); every roster-derived resource has a row; a
below-threshold band and a still-tied aggrieved band stay; a one-band people never breaks away; two
tied far bands found ONE people and two untied groups found two in `BandId` order; the new people's
fog has no `Active` tile and every tile the old one had seen; a practice-earned lesson on the old
people's ledger carries over in full; the knowledge seed takes the best of the group; a split never reads as lost touch over its first turns; losing the last tie is told once; the
wire carries the four fields off the encoded envelope; a checkpoint and a save keep the grown roster
and `cut_off`; a break-away through `run_turn` reaches both peoples' published feeds.
