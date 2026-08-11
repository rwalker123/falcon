---
paths:
  - "core_sim/src/connections.rs"
  - "core_sim/src/connections_config.rs"
  - "core_sim/src/data/connections_config.json"
  - "core_sim/src/snapshot/connections.rs"
  - "core_sim/src/visibility_systems.rs"
  - "core_sim/src/systems/expeditions.rs"
  - "core_sim/src/components.rs"
  - "core_sim/tests/connections.rs"
---

# Contact & connections — the tie two groups leave behind

Design of record: `docs/plan_contact_and_logistics.md` (arc #527, this slice #538). **Contact** is an
event — a group is standing in a tile you can see. What it leaves behind is a **connection**: a
directed, persisting, decaying tie. Logistics, culture, knowledge and cargo are all *riders* on one.

## Config files

| File | Purpose |
|------|---------|
| `src/data/connections_config.json` | `strength.gain_per_contact` (0.25 — four turns of contact reach a full tie), `strength.decay_per_turn` (0.02 — a full tie bleeds to nothing over fifty quiet turns), `forget_turns` (200 — how long after the last contact the edge itself is reaped). The `1.0` ceiling is **not** here: strength is a `0..=1` fraction by definition, so its top is the named constant `connections::FULL_TIE`, and tuning it would change the unit rather than the balance. No hot-reload kind. |

## A connection is a RAW primitive, and that is the whole discipline

It knows nothing about goods, culture or knowledge. It says only *these two groups know each other,
this much, right now* — the same way `LocalStore` knows nothing about food. Every rider builds on top
and owns its own state.

**The failure mode this exists to prevent is named and dated.** The retired `TradeLink` carried
`from_faction` / `to_faction` / `tariff` / `leak_timer` on what should have been an edge: four
riders' opinions wearing the primitive's clothes, on a component that was **never inserted anywhere
at runtime** for the whole life of the band game. So there is no rider vocabulary in any field or
method name in `connections.rs`, and a field that belongs to one rider does not belong here.

## Faction is a property of the ENDPOINT

Both endpoints of `ConnectionKey` are a `BandId`. There is deliberately **no faction field on the
edge and no same-faction / cross-faction branch anywhere in the module**. The arc is shaped so the
arrival of a second faction (#513) changes almost nothing, and that is only true if the code never
asks. The one place faction is resolved is the snapshot filter, which asks whose ties to *publish* —
a viewer question, not a model one.

## Range is the observing BAND's sight, not the faction's

The fog ledger is keyed by faction, but a connection is keyed by band, and reading "range is your
currently-seen tile set" as the faction union would break the arc. Faction-wide sight connects every
band of yours to every other the moment any of them is seen, at full strength, permanently — which
deletes exactly the distance pressure a distant splinter is supposed to feel.

So contact is per observing band. Two of your own bands twenty tiles apart hold no tie: you know
where both are, and neither can *reach* the other.

## Contact is found INSIDE the sight sweep, never beside it

`calculate_visibility` already resolves an effective range per source out of the per-kind config
base, the elevation bonus, terrain modifiers, LOS, the wayfinding kit and posted scout vantages. A
second system that re-derived any of that would drift from this one **silently** — both would keep
producing plausible answers.

So the reveal loop carries the contact half:

- `VisionSource` names what each source is, including `observer_band` — *whose people are standing
  here*. A settlement and a cohort with no `BandId` reveal fog exactly as before and observe nobody.
- `resident_band_occupancy` is built **once** per sweep: which resident bands stand on which tile.
- `ContactSink` rides the existing per-tile reveal closure in `reveal_tiles_in_range`. One map probe
  per revealed tile, **no new geometry**.

Two consequences worth holding onto:

- **Contact is presence in a tile you can see, not first sight of it.** A band you have watched for
  ten turns is contacted again on the eleventh — unlike the wayfinding kit's wear quantum beside it,
  which deliberately fires only on a tile's *first* sighting.
- **A worked source observes.** A band's foragers standing on a forage tile are people standing
  there, so they find whoever else is. That is what "presence" means.

**Subjects are resident bands only.** A detached expedition is not a subject; seeing someone's scouts
is a separate question (#533).

## An expedition reports a people the way it reports the map

A scouting party extends its home band's range, and it reports through the same comm gate its map
reveals already use: `Expedition::pending_contacts` accumulates alongside `pending_reveal`, and both
drain when the party comes within comm range of home. The connection is credited to the party's
**`home_band`**.

**One contact event per subject per flush**, however many turns the party watched them. What came
home is one report, and crediting a march's worth of retroactive contact would let a stale sighting
peg a tie to full.

**A report carries TWO turns, and they are not the same turn.** `record_contact` takes both: the turn
the subject was *observed* stamps clock 1, and the turn the report *landed* drives clocks 2 and 3. A
party that saw a band on turn 40 and walked home until turn 60 leaves `last_seen_turn = 40` beside
`last_contact_turn = 60` — which is why **`last_seen_turn < first_contact_turn` is reachable and
correct** on a tie a report founded: you learned of them on turn 60, and what you learned was where
they stood on turn 40.

Collapsing the two into one parameter is how this first shipped, and it made the observation turn a
dead field at its only consumption point while `lastSeenTurn` published the flush turn on the wire.
For a live sighting the two turns are equal, so direct sight is unaffected either way — which is
exactly why the expedition path is the one that has to be tested for it.

The field rides the existing checkpoint path unchanged — `capture_sim_state` clones the whole
`Expedition` into `ExpeditionRecord` and restore clones it back.

## The three clocks

They decay at genuinely different speeds and are three separate levers.

| What decays | Speed | What it means |
|---|---|---|
| `last_seen_position` / `last_seen_turn` | immediately on losing sight | you know where they *were*. Same as a remembered herd. |
| `strength` | over turns without contact, down to zero | the currency of what you know. **At zero nothing flows.** |
| the edge itself | very slowly, but not never | eventually you have forgotten there was such a people |

**Zero parks the edge; it does not delete it.** A parked edge means *"we know such a people exist and
have no current tie"*. That is what keeps the third clock a genuinely separate lever instead of a
duplicate of the second — delete on zero and `forget_turns` would have nothing left to reap.

**Clock 1 is untouched by decay, and its being untouched is the whole feature.** `decay_all` moves
`strength` and nothing else, so where they were survives a tie bleeding out entirely.

**Clock 1 also never moves BACKWARDS.** `record_contact` advances `last_seen_position` /
`last_seen_turn` only when the incoming `observed_turn >= last_seen_turn`; a contact that loses that
test still raises strength and refreshes `last_contact_turn`, it simply does not rewrite the memory.
The guard is not decoration: `ContactsThisTurn`'s fresher-wins rule resolves collisions *within* one
turn and never compares an incoming report against the ledger, so without it a party flushing an old
sighting would drag a band's remembered position back to where it used to be **and stamp it as the
more recent reading** — clock 1 regressing while claiming to be fresh.

**Clock 3 measures from the last CONTACT, not from the strength.** A parked edge is still a memory,
and only time erases it.

**"Saw contact this turn" is read off `last_contact_turn`**, which `record_contact` has already
stamped — so `decay_all` needs no second copy of the turn's contact set and cannot disagree with one.

## `ContactsThisTurn` is derived; `ConnectionLedger` is `SimState`

The ledger is persisted state with its own clocks, so it is checkpointed and restored whole. The
turn's contact set is rebuilt from scratch every turn and is not. Contrast `SupplyNetworkMembership`,
which is correctly derived; see `checkpoints.md` for why the distinction is load-bearing.

`ConnectionLedger` and `ContactsThisTurn` are both **`BTreeMap`, not `HashMap`** — the iteration
order is observed by the snapshot and the checkpoint, so it has to be an order and not an accident.
The occupancy index inside the sweep is a `HashMap` because it is *probed and never iterated*.

`ContactsThisTurn` is keyed by the edge rather than a set of triples: a subject stands in exactly one
place, so its position is a value and not part of the identity. The turn stored beside it is **when
the position was observed**, which is not always this turn — an expedition's report is what the party
saw on the march — and when two reports name the same edge the **fresher observation wins**.

## THE KEYSTONE — a connection can only ever grant `Discovered`

> **Only presence makes a tile `Seen` (`VisibilityState::Active`). A connection can only ever grant
> `Discovered`.**

Meet a band, exchange maps, and their land becomes `Discovered` — frozen at the moment they told you.
To *watch* anything you need presence there, maintained. A remembered band therefore behaves exactly
like a remembered herd: you know where they were, and they may have moved, split or starved since.
Nothing new to teach, because the player has already been surprised by a herd that wasn't where they
left it.

This is the rule every rider will be tempted to break, so it is stated in the module docs where a
rider author has to read it, and asserted in `core_sim/tests/connections.rs`: two identical worlds,
one seeded with full ties, produce the same `Active` set — paired with a liveness half, because "the
sets match" also passes when contact has quietly stopped firing.

The one carve-out the arc allows is a **maintained logistics route**, whose tiles stay `Seen` for as
long as it is held. That is not an exception so much as an instance of the rule: those tiles are seen
because there are people walking them.

## The wire

`ConnectionState` / `ConnectionSection`, appended last on **both** `WorldSnapshot` and `WorldDelta`.
A section with no delta twin is permanently stale on a delta-fed client — the defect
`campaign_profiles` actually was, and the one `core_sim/tests/delta_streaming.rs` exists to catch.

**Filtered to the viewer by the OBSERVER's faction**: you see who *you* know, not who other peoples
know. The subject is published whatever faction it belongs to — that is the point of a contact — and
faction never appears on the row. An observer band that cannot be resolved to a faction (despawned
this turn) is skipped rather than published against a guess.

Order is the ledger's `BTreeMap` order, so the section is stable frame to frame and diffs out when
nothing moved.

**The tests assert on the encoded envelope**, through `root_as_envelope` and the accessor chain a
client uses — not on the in-process ledger. A section that never reaches the codec still passes an
in-process assertion, and this one has no client reader yet to notice.

## Metrics

`SimulationMetrics` carries `connections_live` / `connections_formed` / `connections_reaped`, written
by `advance_connections` (not by `collect_metrics` — only that system knows which edges *formed* and
which were *reaped*, and both are differences that cannot be re-derived from the ledger afterwards).

**They ride the `turn.completed` log line**, which is what makes them an observer rather than three
numbers nobody reads: until a client consumes the `connections` section, a tie forming or being
reaped is otherwise invisible in a running game, and a subsystem you cannot watch is one you cannot
play-test.

The other live surface is **`export_map`**, whose JSON carries the whole `WorldSnapshot` — including
`snapshot.connections` for the viewer faction, with each edge's strength, remembered position and
three turn stamps.

## See Also

- `docs/plan_contact_and_logistics.md` — the arc: range, contact, the riders, the route ladder
- `.claude/rules/core_sim/fission.md` — band fission, the arc that first makes a connection necessary
- `.claude/rules/core_sim/checkpoints.md` — why the ledger is `SimState` and the contact set is not
- `.claude/rules/core_sim/ecs-systems.md` — fog of war, whose sweep carries the contact half
- `.claude/rules/core_sim/expeditions.md` — the comm-range flush a party's contact report rides
