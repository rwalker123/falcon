---
paths:
  - "core_sim/src/resources.rs"
  - "core_sim/src/systems/population.rs"
  - "core_sim/src/systems/labor.rs"
  - "core_sim/src/snapshot/mod.rs"
  - "core_sim/src/snapshot/campaign.rs"
  - "core_sim/tests/demographic_events.rs"
  - "core_sim/tests/delta_streaming.rs"
  - "xtask/src/decode_fixture.rs"
---

# The event feed: a turn window, a sequence, and an append-only delta

`CommandEventLog` is the one player-facing stream of *things that happened* — command echoes, the
Telling's beats, a pen lost, a predator raid, and (issue #272) the demographic flows the sim had
always resolved and discarded. This file is the engineering rationale for the three changes that
made it a feed a player can actually read: the bound, the sequence, and the delta.

Design of record: `docs/event_dock_ux_proposal.html`. The client half — the dock, the rungs, the
grouped log — is `.claude/rules/client/`.

## Config files

| File | Key | Purpose |
|---|---|---|
| `src/data/simulation_config.json` | `command_events_retention_turns` (**20**) | How many turns of world events the log keeps. Long enough to answer *"what happened while I was away"*, short enough to bound a full snapshot. Read into `SimulationConfig`, applied to the log at `build_headless_app` and re-applied on a `reload_config` (`set_retention_turns` prunes immediately, so the published window and the published rows cannot disagree). Published as `CampaignSection.commandEventsRetentionTurns` |

## The bound is a TURN WINDOW, not a count

The log used to keep the newest **32 entries**. That was fine while it carried only command echoes.
Once births, deaths and coming-of-age report per band per turn, a count-bounded ring evicts a wolf
raid inside two turns — the cap eats exactly what it exists to preserve.

So `push` drops whole turns off the back: everything older than
`newest_tick − retention_turns` goes. A turn is the unit the player thinks in, the unit the client
groups by, and the unit *"earlier turns"* walks backwards through.

- **The newest entry's tick is the window's anchor.** Pushes are monotonic in tick (a turn resolves
  before the next begins; a rollback replaces the log whole), so the row just pushed is the latest
  turn the log knows about.
- **`MAX_RETAINED_EVENTS` (512) is a backstop, not the bound.** It exists so one pathological turn
  cannot grow the log — and therefore the resync snapshot — without limit. Reaching it drops events
  from *inside* the window, which is why it sits well above a normal turn's traffic rather than at
  it.

> **A test that counts rows in this log across a long run is now measuring the window.** Several
> already did. `telling_memory`'s medium-advance guard drove 25 turns of collapse and then asserted
> the log still held exactly one advance line; the honest form is *"nothing fired **after** the
> advance"*, keyed on `tick`, because the original legitimately ages out while a re-fire would be
> inside the window and still visible. Same for the demographic guards: they run 15 turns, not 40.

## The sequence is ONE-BASED, and that is load-bearing

Every entry carries a `seq`, stamped by `push` and never reused. A delta ships the rows whose `seq`
exceeds the client's cursor, and **a fresh cursor is `0`** — so a zeroth event would be permanently
unsendable to every new client. `FIRST_COMMAND_EVENT_SEQ` is therefore `1`, and `0` survives as the
*never-pushed* value `CommandEventEntry::new` leaves behind (the log is the only writer).

`next_seq` is monotonic **across eviction**: the cursor is a statement about what the client has
seen, not an index into the ring, so a reissued number would silently suppress a real event.

## `diff_appended` — the third diff shape

Beside `diff_whole` / `diff_indexed` in `snapshot/mod.rs`, and the only one whose baseline is a
single `u64`. `command_events` is a *log*: rows are appended, never edited, and the oldest fall out
of the window. `diff_whole` re-serialised the whole retained ring every turn any event fired — at a
20-turn window, ~200 rows to say that three are new, and the cost grows with the very feature being
added.

**A dropped delta permanently loses the events it carried**, where the old whole-vector resend was
self-healing. That is safe for exactly one reason, and the pairing is not optional:

> the client applies a delta only when it holds the named base frame (`WorldCache::accepts`), and a
> mismatch raises `resync_needed`, whose answer is a **full snapshot** carrying the entire retained
> ring.

`delta_streaming::a_dropped_delta_is_detectable_and_the_resync_answer_re_backfills_every_event`
pins both halves — that the gap is detectable, and that the resync answer really does re-backfill.

**`Baseline::Hold` must NOT advance the cursor.** A mid-tick recapture diffs from the *turn's*
baseline, so advancing the cursor there would consume the rows: the recapture frame would carry
them and the committed turn delta — which the client may be the only recipient of — would never
send them at all. It is the same cumulativity property every other section already has for
recaptures, and here it is the difference between a lost frame being free and being unrecoverable.
A rollback rewinds the cursor to the highest `seq` in the restored entry (`0` when empty), for the
mirror reason: a cursor left ahead of a rewound world suppresses the re-send.

## The demographic flows: a rate becomes an event

`advance_demographics` resolved `births`, `maturation` and three death terms as locals and dropped
them, so a band that lost two elders to cold and gained a child looked exactly like a band that did
neither. `DemographicOutcome::flows` returns them; `DemographicFlowAccumulator` turns them into
events.

**Births are a rate.** `births = working × fertility` on a thirty-person band is a fraction of a
person per turn. Rounding per turn either invents a birth in a band too small to have had one, or
reports none all game. So each flow accrues on a per-band carry, and an event fires only when the
carry crosses a whole person, `accrue` subtracting exactly the count it reported and keeping the
remainder. That remainder is what makes a small band's births *late* rather than *absent*, and it is
pinned directly (`components::tests::the_remainder_survives_the_crossing`).

- **One event names a COUNT**, never one event per person: three elders lost to one cold snap is one
  line.
- **The death cause is recorded on the turn it happens** (`DeathCause`, the dominant of the
  starvation and cold terms, ties to `Hunger`), and read at the crossing. Nothing afterwards can
  answer it — post-turn brackets carry no record of which term emptied them.
- **Migration needs no accumulator.** `last_emigrated` / `last_immigrated` are whole people already,
  so `push_migration_events` fires from `advance_population_migration` — where those counts are
  resolved — rather than from `simulate_population`, which would report the *previous* turn's moves
  under the current tick.
- **The carry is checkpoint state** (`sim_state::BandRecord::flow_accumulator`, classified in
  `SIM_STATE_COMPONENTS`), for the same reason `BandTravel` is: a band two-thirds of the way to a
  birth was two-thirds of the way there, and dropping the remainder re-times every event after a
  restore.

**Detail tokens are space-delimited `key=value`**, the form the client's feed already parses:

| Kind | Detail |
|---|---|
| `born` | `band= count=` |
| `came_of_age` | `band= count=` |
| `died` | `band= count= bracket={child\|working\|elder} cause={hunger\|cold}` |
| `migrated` | `band= count= direction={out\|in}` |

**The label names `Band {id}`**, because the snapshot carries no band *name* — the client renders a
positional "Band N" (`HudFormat.band_display_name`). Every event also carries the id as a `band=`
token, which is what lets the client re-label the row with whatever it calls that band.

`Option<&BandId>` / `Option<&mut DemographicFlowAccumulator>` on the query: worldgen gives every
real band both, and `demographic_events::every_resident_band_carries_a_flow_accumulator` fails if a
spawn seam forgets — a band that silently never narrates is the failure mode that would otherwise
ship unnoticed.

## What is NOT here

There is **no `System` event kind**. System/console lines are synthesized client-side; the sim
publishes only things that happened in the world.
