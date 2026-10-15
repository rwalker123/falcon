---
paths:
  - "core_sim/src/dealings.rs"
  - "core_sim/src/diplomacy_config.rs"
  - "core_sim/src/data/diplomacy_config.json"
  - "core_sim/src/snapshot/dealings.rs"
  - "core_sim/tests/dealings.rs"
  - "sim_schema/src/state/dealings.rs"
  - "sim_schema/src/codec/dealings.rs"
---

# Early diplomacy — offers, and the pending dealing

Design of record: `docs/plan_early_diplomacy.md` (issue #233, arc #231). This file is the as-built
engineering rationale for **slice 1a, offers** (§1 of the plan): the config, the pending item, the
standing answer, the answer verb and the wire. Goodwill, treaties, the envoy, road rights and the AI
are later slices; their levers are loaded and validated here and read there.

## Config files

| File | Key | Opening | Meaning |
|---|---|---|---|
| `src/data/diplomacy_config.json` | `offers.offer_wait_turns` | **1** | turns a pending dealing waits for an answer before silence refuses it; validated `>= 1` (an offer that expired the turn it was posted would be refused unseen) |
| | `goodwill.fade_per_turn` | **0.02** | proportional fade toward zero; a fraction in `0..=1` |
| | `goodwill.per_load` | **0.02** | goodwill per unit of worth; finite, `>= 0` |
| | `goodwill.per_person_stationed` | **0.001** | per stationed person per turn; finite, `>= 0` |
| | `goodwill.per_link` | **0.004** | per standing link per turn; finite, `>= 0` |
| | `goodwill.per_term` | **0.002** | per treaty term in force per turn; finite, `>= 0` |
| | `goodwill.broken_treaty` | **0.5** | the drop a breach costs; a fraction in `0..=1` |
| | `goodwill.wary_below` / `friendly_above` | **−0.2 / 0.2** | the word cuts; each within `−1..=1`, `wary_below <= friendly_above` |
| | `roads.foreign_road_share` | **0.9** | a foreigner's share of a built road's payoff; a fraction in `0..=1` |
| | `ai.ai_treaty_goodwill` | **0.2** | the goodwill an AI needs to accept a treaty; within `−1..=1` |

Loader `diplomacy_config.rs` on the shared boot seam (`config-loading.md`), env override
`DIPLOMACY_CONFIG_PATH`, no hot-reload kind. **Every key is required and every table is closed**
(`deny_unknown_fields`; only the root accepts a `_comment`), so a missing or misspelled lever is a boot
panic rather than a number nobody chose. Only `offer_wait_turns` has a reader in this slice; the
rest are loaded so a bad file stops the boot the day it is edited, not the day a later slice first
reads it. All opening values are playtest dials.

## The pending item: `PendingDealings`

`dealings.rs`. One record for everything that waits on a people's answer: **addressed to a people,
answered by a command, expiring on silence.** The book is `SimState` — `BTreeMap<DealingId, Dealing>`
plus the monotonic id counter, checkpointed and restored whole (`SAVE_FORMAT_VERSION` 42). A restore
that dropped the book would strand a party in a camp with nobody waiting on it; one that dropped the
counter would re-issue an id a client still holds. Ids start at `1`, so `0` on the wire means "no
dealing", and a removed id is never handed out again.

A `Dealing` is: `id`, `kind`, sender faction and band, receiver faction and band, `party`
(`Option<BandId>`), `goods_sent`, `goods_asked`, `posted_turn`, `expires_turn`
(`posted_turn + offer_wait_turns`). Goods are `LocalStore`s — the shipment's own cargo
representation (food by keeping class, hay, material batches with their readings) — so a list is
never a summed value.

- **`DealingKind` is a fieldless enum** (`Offer` today). The carrier-specific data (the waiting party)
  is a field on the record, so a later `Proposal`, `Counter` or `TreatyProposal` adds a variant and
  leaves the record's shape alone. The wire carries the kind as a token (`"offer"`).
- **The goods stay on the party.** `goods_sent` is a manifest; the physical cargo is still
  `Expedition::cargo`, so a recall, a lost destination or a refusal needs no goods-moving code: the
  party turns `Returning` and the existing fold-back carries the cargo home as `ShipmentReturned`.
  `goods_asked` is empty on a stage-2 offer — the receiver names the return in its answer.
- **`Dealing::turns_left(now)` counts the resolve the next turn runs at**, so a dealing that expires
  at `now` reads **1** ("answer this turn or silence refuses it"), never a `0` that reads as gone.

## A shipment's terms, and the standing answer

`send_trade_expedition` gains one word, `offer` (`gift` is the default, accepted spelled out; both
words together or either twice is refused). It rides `SendTradeExpeditionCommand.offer` (proto field
**7**) and lands on `ExpeditionMission::Trade::terms: ShipmentTerms { Gift, Offer, Returned }`.
`Returned` is set by the answer, never at launch (below).

**The standing answer** is each people's "accept gifts from my own people", on by default, held as
`FactionBorderPolicies::accept_own_gifts` beside the Open Borders row (so it is seeded from the roster,
checkpointed, and re-seeded on a runtime break-away with no resource of its own). Command
`set_accept_own_gifts <faction> <on|off>` (proto **85**), gated like `set_open_borders` through
`commanding_faction`.

`standing_answer_covers(terms, sender, receiver, policies)` is the **one** statement of what the
answer settles: a gift, sender and receiver the same people, the receiver's answer on. It is the only
thing the arrival path asks about the receiver. A shipment it covers lands exactly as it always has
(`TradeDelivered`, the party turns `Returning`); nothing else in that path branches on a faction.
An offer, a gift to another people, and a gift to one's own people with the answer off all become
pending.

## Arrival: post, and wait in the camp

`advance_expeditions`' `Arrived` arm is the fork. A shipment no standing answer covers is posted as a
`Dealing` and the party **stands in the camp in `ExpeditionPhase::AwaitingOrders`** with its
`BandTravel` removed. **No new phase**: `AwaitingOrders` already means "standing at the destination,
waiting for a decision", and the existing arms that touch it (the lost-destination guard, recall,
`move_band`) already do the right thing to a waiting party. The cargo is not touched.

A world without the book or without a band id for either end (a hand-rolled test world) keeps the
direct hand-over. The hand-over itself is one routine, `systems::land_cargo_at_band`, shared by the
arrival and by an accepted offer so the two cannot book a delivery differently.

`advance_dealings` (Population chain, right after `advance_expeditions`) runs each turn:

- a dealing whose party is no longer `AwaitingOrders` (recalled, sent on with `move_band`, turned
  home because its destination vanished) is **withdrawn** — removed, and the receiver told;
- otherwise, at `expires_turn` the party turns `Returning` with its own cargo (**silence is a
  refusal**), and both peoples are told it lapsed.

## The answer verb

`answer_dealing <faction> <id> accept [food <a>] [fodder <a>] [material <id> <a>]… | refuse`
(proto **86**, `AnswerDealingCommand`; the return list is `TradeCargoItem`s, the shipment's own cargo
words). Seat-gated and membership-checked like every faction-bearing order. **`counter` is the next
slice: the text grammar refuses it** with `CommandParseError::UnsupportedAnswer` and a reason, rather
than reading it as an accept or dropping it. The resolution is the library function
`dealings::answer_dealing(world, faction, id, &answer)`, which the server handler wraps (a refusal is
a `CommandEventKind::Dealing` failure line) and the tests call directly.

**It resolves in the turn it is submitted**: posted at tick N, answered in the window of N+1, applied
at once. Only the receiver's people may answer; a dealing addressed to someone else reads as "not
waiting on your people", the same message as no dealing at all.

**Accept fails closed, with a reason, before anything moves**: the dealing must exist and be
addressed to the answering people; its party must still be standing in the camp (else it is
withdrawn); the receiving band must still be there; every line must be a positive finite amount of a
known commodity or material; the return's load (`CarryLoad`, the one carry formula, at the sending
people's pack — travois included) must fit the party's `carry_capacity`; and the receiving band must
hold every line (food, hay, and each material's total). Duplicate lines sum per good. A refusal of the
answer leaves both bands, the party and the book exactly as they stood.

Then, in order: the cargo lands in the receiving band (`land_cargo_at_band`, booked `ShipmentIn`);
the named return is drawn from that band into the party's cargo (food by class, hay, each material
batch at its own rating) and booked on the receiver as `ExchangeOut`; the mission's terms become
`Returned`; the party turns `Returning`; the dealing leaves the book. An **empty return is allowed**.
**Refuse** turns the party `Returning` with its own cargo. The ledger identity stays closed because
every term has a booking on both ends: the receiver's `ExchangeOut` is its `transferSent`, and the
fold-back books the cargo at home through `FoldBack::book_home`.

### Two transfer causes, because the books need them

`TransferCause::ExchangeOut` (wire `9`) and `ExchangeIn` (wire `10`), both on the route arm.
`ExchangeOut` is the receiver paying the return; `ExchangeIn` is that return landing in the party's
home band. Without them an accepted offer's return would come home as `ShipmentReturned`, which says
"a shipment never arrived" — and the receiver's payment would have no cause at all. The fold-back
picks the cause with `ExpeditionMission::returning_cargo_cause()` (`Returned` → `ExchangeIn`, else
`ShipmentReturned`), passed to `FoldBack::book_home` by both of its call sites.

## The feed

`CommandEventKind::Dealing` (appended last, string `dealing`), pushed **once per people** because the
feed is per-faction on the wire (`BandChangedHands`' shape). `status=` carries the stage:

| status | told to | detail tokens |
|---|---|---|
| `posted` | the sender | `dealing= kind= destination=<band> expedition=` |
| `offered` | the receiver | `dealing= kind= from=<faction> sender=<band> band=<band>` |
| `accepted` / `refused` / `expired` | both | the same tokens, minus `expedition=` |
| `withdrawn` | the receiver only | the same |

Band ids ride as tokens (and as `CommandEventEntry::band`) so a client swaps in its own label; the
receiver's lines never carry the party's entity handle.

## The wire

`DealingsSection` is appended last on **both** `WorldSnapshot` and `WorldDelta` (`dealings`, diffed
whole), **viewer-scoped**: `PendingDealings::visible_to(viewer)` — addressed to the viewer's people or
sent by them. Another pair's dealings are theirs to know, and a third people's frame carries none.
A row (`DealingState`): `id`, `kind`, `senderFaction`, `senderBand`, `receiverFaction`,
`receiverBand`, `goodsSent` and `goodsAsked` (`DealingGoodsState`: `food`, `fodder`, and
`materials:[MaterialBatchState]` — the band store's row, readings and all), `turnsLeft`. The viewer's
own standing answer is `FactionPolicyState.acceptOwnGifts` (appended; default on) beside
`openBorders`. `TransferCrossingState.cause` gains `9` and `10`.

## Tests

| Test | Claim |
|---|---|
| `tests/dealings.rs::an_offer_waits_as_a_pending_dealing_visible_to_its_two_peoples_and_no_third` | the party waits `AwaitingOrders` with its cargo; the row is on the **encoded** frame of the receiver and the sender and absent from a third people's; one feed line per side |
| `::accepting_with_a_return_lands_the_cargo_and_the_return_comes_home` | cargo in the receiver, return out of it (`ExchangeOut`), return lands at home as `ExchangeIn` on the fold-back, never `ShipmentReturned` |
| `::accepting_with_no_return_…`, `::refusing_sends_the_cargo_home` | an empty return is allowed; a refusal returns the whole cargo as `ShipmentReturned` |
| `::silence_expires_the_offer_…`, `::a_longer_wait_holds_the_offer_open_that_many_turns` | silence refuses after `offer_wait_turns`, and the wait is the config's |
| `::a_same_people_gift_still_lands_on_arrival`, `::…_with_the_answer_off_becomes_pending`, `::a_gift_to_another_people_waits_…` | the standing answer's three sides, and its setting on the frame |
| `::an_answer_that_cannot_be_honoured_is_refused_and_nothing_moves`, `::a_return_over_the_partys_carry_is_refused` | every fail-closed check, with the bands, party and book unchanged |
| `::only_the_receiving_people_may_answer`, `::a_dealing_whose_party_has_gone_is_withdrawn` | who may answer, and a vanished party |
| `::a_pending_dealing_round_trips_a_checkpoint` | the book, the counter and the terms ride `SimState` |
| `tests/sim_state_coverage.rs` | `PendingDealings` is classified `SIM_STATE`; the config handle and metadata are `CONFIG` |

## See also

- `.claude/rules/core_sim/expeditions.md` → "A shipment is a party that WALKS IT" — the trade verb
  this extends
- `.claude/rules/core_sim/factions.md` — the seat gate and the viewer-scoped sections
- `.claude/rules/core_sim/save-game.md` — the version table
