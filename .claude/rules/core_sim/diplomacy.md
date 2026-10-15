---
paths:
  - "core_sim/src/dealings.rs"
  - "core_sim/src/diplomacy_config.rs"
  - "core_sim/src/data/diplomacy_config.json"
  - "core_sim/src/snapshot/dealings.rs"
  - "core_sim/tests/dealings.rs"
  - "sim_schema/src/state/dealings.rs"
  - "sim_schema/src/codec/dealings.rs"
  - "core_sim/src/goodwill.rs"
  - "core_sim/src/snapshot/goodwill.rs"
  - "core_sim/tests/goodwill.rs"
  - "core_sim/tests/stages.rs"
  - "sim_schema/src/state/goodwill.rs"
  - "sim_schema/src/codec/goodwill.rs"
  - "core_sim/src/treaties.rs"
  - "core_sim/src/snapshot/treaties.rs"
  - "core_sim/tests/treaties.rs"
  - "sim_schema/src/state/treaties.rs"
  - "sim_schema/src/codec/treaties.rs"
---

# Early diplomacy — offers, the pending dealing, and goodwill

Design of record: `docs/plan_early_diplomacy.md` (issue #233, arc #231). This file is the as-built
engineering rationale for **slice 1a, offers** (§1 of the plan): the config, the pending item, the
standing answer, the answer verb and the wire; and **slice 1b, goodwill** (§2): the opinion ledger,
its sources, its fade, and how it is known; and **slice 1c, the exchange stages** (§1 and the `link`
source of §2): haggling over a standing link (stage 4), a scout's pack (stage 1), and what a standing
link is worth; and **slice 1d, the AI answers** (§4): the live opinion on the viewer's own frame and
the offer's party size, which are what a player process needs to judge a dealing — the answering
itself is `sim_ai`'s (`.claude/rules/core_sim/ai-driver.md` → "`Dealings`"); and **slice 2,
treaties** (§3): road rights, open borders and agreed trade, proposed over a road or by an envoy, the
per-people borders and the foreign-road reading ("Treaties" below).

## Config files

| File | Key | Opening | Meaning |
|---|---|---|---|
| `src/data/diplomacy_config.json` | `offers.offer_wait_turns` | **1** | turns a pending dealing waits for an answer before silence refuses it; validated `>= 1` (an offer that expired the turn it was posted would be refused unseen) |
| | `offers.standing_link_min_rung` | **`route:dirt_road`** | the lowest built road rung, as a route-ladder wire key, that every tile between two camps must carry (keeping met) for a standing link to stand; validated to name a rung at or above the first built one, so a trail, a plant rung or a typo is a boot panic |
| | `goodwill.fade_per_turn` | **0.02** | proportional fade toward zero; a fraction in `0..=1` |
| | `goodwill.per_load` | **0.02** | goodwill per unit of worth; finite, `>= 0` |
| | `goodwill.per_person_stationed` | **0.001** | per stationed person per turn; finite, `>= 0` |
| | `goodwill.per_link` | **0.004** | per standing link per turn; finite, `>= 0` |
| | `goodwill.per_term` | **0.002** | per treaty term in force per turn; finite, `>= 0` |
| | `goodwill.broken_treaty` | **0.5** | the drop a breach costs; a fraction in `0..=1` |
| | `goodwill.wary_below` / `friendly_above` | **−0.2 / 0.2** | the word cuts; each within `−1..=1`, `wary_below <= friendly_above` |
| | `roads.foreign_road_share` | **0.9** | a foreigner's share of a built road's payoff; a fraction in `0..=1` |

Loader `diplomacy_config.rs` on the shared boot seam (`config-loading.md`), env override
`DIPLOMACY_CONFIG_PATH`, no hot-reload kind. **Every key is required and every table is closed**
(`deny_unknown_fields`; only the root accepts a `_comment`), so a missing or misspelled lever is a boot
panic rather than a number nobody chose. Readers: `offer_wait_turns` (offers), `standing_link_min_rung`
(the standing link) and, for goodwill, `fade_per_turn`, `per_load`, `per_person_stationed`,
`per_link`, `wary_below` and `friendly_above`, and — read by treaties — `per_term`, `broken_treaty`
and `roads.foreign_road_share`. All opening values are playtest dials. **No AI lever is here**: an AI
is a player process and cannot read sim config, so its dealing levers (`return_fairness`,
`food_runway_floor_turns`, `treaty_goodwill`) live in `sim_ai/data/ai_profiles.json`.

## The pending item: `PendingDealings`

`dealings.rs`. One record for everything that waits on a people's answer: **addressed to a people,
answered by a command, expiring on silence.** The book is `SimState` — `BTreeMap<DealingId, Dealing>`
plus the monotonic id counter, checkpointed and restored whole (`SAVE_FORMAT_VERSION` 42). A restore
that dropped the book would strand a party in a camp with nobody waiting on it; one that dropped the
counter would re-issue an id a client still holds. Ids start at `1`, so `0` on the wire means "no
dealing", and a removed id is never handed out again.

A `Dealing` is: `id`, `kind`, sender faction and band (who posted it), receiver faction and band (who
must answer), `party` (`Option<BandId>`, an offer's), `goods_sent`, `goods_asked`, `agreement`
(`Option<Agreement>`, a proposal's or counter's legs), `posted_turn`, `expires_turn`
(`posted_turn + offer_wait_turns`). Goods are `LocalStore`s — the shipment's own cargo
representation (food by keeping class, hay, material batches with their readings) — so a list is
never a summed value.

- **`DealingKind` is a fieldless enum** (`Offer`, `Proposal`, `Counter`). The carrier-specific data
  is a field on the record — the waiting party on an offer, the named legs (`Dealing::agreement`) on
  a proposal or counter — so a later `TreatyProposal` adds a variant and leaves the record's shape
  alone. The wire carries the kind as a token (`"offer"`, `"proposal"`, `"counter"`).
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
words). Seat-gated and membership-checked like every faction-bearing order. `counter give […] ask […]`
answers a proposal or a counter ("The exchange stages" below); an offer cannot be countered, and any
other word is `CommandParseError::UnsupportedAnswer` rather than an accept. The resolution is the library function
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
picks the cause with `Expedition::returning_cargo_cause()` (a trade party's `Returned` terms, or a scout
whose handover was accepted, → `ExchangeIn`; else `ShipmentReturned`) and the counterparty with
`Expedition::cargo_counterparty()`, passed to `FoldBack::book_home` by both of its call sites.

## The exchange stages (slice 1c)

### The standing link — ONE predicate

`routes::standing_link(roads, a, b, width, height, wrap, floor)`: true when **every tile of the path**
between two camps carries a road that is `Road::keeping_is_met` **and** held at `floor` or above
(`offers.standing_link_min_rung`, opening `route:dirt_road`). It reads **exactly the tile set
`path_reach_tiles` reads** — the same `trace_path` over the same registry, the same keeping filter — so
"the road holds the link open" and "the road makes this a standing link" can never be two runs of
ground. A tile with no road, an unkept road or a road below the floor breaks it; **there is no faction
branch** (whose road it is does not matter). The two endpoint tiles are tested first, so a pair of camps
with no road at either end traces nothing. `dealings::bands_have_standing_link(world, a, b)` is the
world-reading wrapper the command path uses.

⛔ **A built road must be KEPT.** The labour pass stamps every built road's bill each turn and an unkept
road's shortfall reads as unmet, so a road nobody pays holds no link once one real turn has judged it
(`stages.rs::an_unkept_road_holds_no_link_once_a_turn_has_judged_it`). A road with no stamped bill
reads as met (the shipped rule), which is why a fixture that never runs a turn can lay a bare road.

### Stage 4: `propose_trade`, and the counter

`propose_trade <faction> <band> <party_workers> <dest_band> give […] ask […]` (proto **87**,
`ProposeTradeCommand`; the lists are `TradeCargoItem`s). `dealings::propose_trade` records a
`DealingKind::Proposal` **the turn it is made**, addressed to the destination's people, and fails
closed before recording anything: the band is the proposer's resident band with the workers; the
destination passes `check_destination` (the shipment launch's own tie gate — a live tie, faction
never asked); a **standing link** holds; the sender holds the give and it fits the party's pack
(`check_cargo`, the trade verb's own carry computation); the proposal names something. **Nothing is
drawn.** The legs are `GoodsLines` (`food`, `fodder`, one total per material) — unrated, because no
batch exists to rate yet — held in `Dealing::agreement` beside the shipper and consignee.

- **`Agreement` is stated from the SHIPPER's point of view**: `give` is what the shipper sends, `ask`
  what it gets back, whoever is speaking, so *accept* means the same on a proposal and on every counter
  after it. `Dealing::sender`/`receiver` are the poster and the addressee; `shipper`/`consignee` never
  swap.
- **`answer_dealing … counter give […] ask […]`** closes the original (`status=countered`, told to
  both) and posts a `Counter` addressed back, party size carried over. There is no cap on counters:
  each costs the haggler a turn. A counter must fit the shipper's pack and the answering side must hold
  its own leg.
- **Every answer on a proposal or counter requires the standing link to still hold.** If the road has
  gone the dealing is withdrawn (`status=withdrawn reason=link_broken`, told to both) and the answer
  fails.
- **Accept** takes no return list (the terms are written; counter to change them). It checks the
  answering side's own leg now — a consignee holds the ask, a shipper can launch — and returns the
  `Agreement` in `AnswerReport::launch`. **The launch is the server's** (it lives in `bin/server.rs`):
  `launch_agreed_party` sends the shipper's party through the trade launch with the give as cargo and
  `ShipmentTerms::Agreed { agreed_return, dealing }`. When the consignee accepts and the *shipper*
  can no longer honour its leg, the dealing **fails** with a reason (`status=failed`, told to both,
  nothing moves); when the shipper is the answerer the answer fails closed and the dealing stays.
- **A proposal is open for the window it was made in and `offer_wait_turns` more** (`expires_turn =
  tick + wait`); silence refuses it. There is no party to withdraw it, only the road.

### An agreed trade settles on arrival

`systems::expeditions::settle_agreed_trade`, from the `Arrived` arm (an `Agreed` shipment never goes
through the standing answer and is never pending): if the receiver still holds the agreed return,
`dealings::settle_exchange` — **the one exchange routine an accepted offer also uses** — lands the cargo
(`ShipmentIn`), draws the return out of the receiver (`ExchangeOut`), and the party turns `Returning`
with the return (`ExchangeIn` on the fold-back); each side's goodwill rises by the worth of what it
received, at settlement. **If the receiver no longer holds the return the exchange fails**: nothing
lands, nothing is drawn, **no goodwill moves**, and the party walks home with its cargo
(`ShipmentReturned`), `status=failed reason=receiver_short`, told to both. A failed launch (accept,
then the party cannot set out) is likewise a failure with no goodwill.

### Stage 1: a scout's pack

`send_expedition … [kit <id>] [strangers give|offer|keep] [pack [food <a>] [fodder <a>] [material <id>
<a>]…]` (proto fields 7 `pack` and 8 `strangers` on `SendExpeditionCommand`). The pack is the
shipment's own manifest: it fails closed on the shipment's gates (`check_cargo`: held, within the
party's carry) **before anything is drawn**, and is drawn before the walking larder so it is exact
(`draw_goods`, shared with the trade launch). A `give`/`offer` order with no pack is refused. The
pack rides `Expedition::cargo`; the order and the handover's state ride `Expedition::scout_pack`
(`ScoutPack`: `order`, `target`, `resume`, `spent`, `handover`, `returned`), checkpointed with the
party.

- **A stranger** is a resident band of another people the party sights (in `advance_expeditions`'s
  observe pass) whose tie to the party's home band is not live (`ConnectionLedger::tie_is_live`).
  The first one sighted while the order is live and the pack aboard becomes the target; the party
  records the scouting destination it was walking to (`resume`; the tile it stood on if it was
  waiting) and aims at the stranger's live tile through `steer_toward_target`, the shipment's own
  arrival rule.
- **Arrival posts a pending `Offer`** and the party waits in `AwaitingOrders`, the wait a shipment's
  offer already uses. `give` and `offer` post the same record — the receiver names the return in both
  (an empty one is a gift) — and **the order is spent when the dealing posts**, whatever the answer:
  one pack, one handover.
- **After the dealing ends** (accepted, refused or lapsed) `Expedition::after_dealing` puts a scout
  back to `Outbound`; the `Outbound` arm walks to `resume` before it will stop. An accepted return rides
  home in the cargo and books `ExchangeIn` against the receiving band
  (`Expedition::returning_cargo_cause` / `cargo_counterparty`, the one place a trade party and a scout
  are told apart at the fold-back).
- **A stranger lost before arrival** (out of sight when the party stands on its aim, or gone) leaves
  the order **unspent**; the scout resumes. A `move_band` on the scout clears the chase so the
  player's order wins, and the pack order stays live.

### Link goodwill and news

`goodwill::credit_link_goodwill` (`TurnStage::Visibility`, `.after(sites::discover_sites)`): for every
pair of **peoples** with any band pair joined by a standing link, `per_link` into `link`, each people
→ the other, **once per people pair per turn** however many band pairs the road joins — goodwill is
a people-level opinion and a steady source is meant to settle at `s / fade_per_turn`, so crediting per
band pair would let a people multiply a road's worth by founding bands along it. The same pass
refreshes `KnownGoodwill` both ways with the value standing now, stamped with this turn: word travels
along a kept road, so no party carries it. It runs in Visibility because the roads' keeping is judged
by then (`light_kept_routes` reads it at the same moment). It filters to bands standing on a
qualifying road first, so a world with no roads walks no pair.

## The wire, slice 1c

`DealingState` gains `shipperBand` and `partyWorkers` (appended): `0` on an offer; on a proposal or a
counter `shipperBand` names the band whose party walks and `goodsSent`/`goodsAsked` are the give and
the ask as `DealingGoodsState` with **unrated** material rows (`materialId` and `amount`, no readings).
Feed lines carry `kind=proposal|counter`; statuses add `countered`, `failed` and, for an agreed
trade's arrival, `settled` (`kind=exchange`).

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

## Goodwill — one opinion per ordered pair, kept as its reasons

`goodwill.rs`. `Goodwill` (`SimState`) is `BTreeMap<(holder, about), GoodwillEntry>`; the entry is
**one `f32` per source kind** — `gifts`, `exchanges`, `stationed`, `link`, `treaty`, `breach` — and the
opinion is their sum (`GoodwillEntry::total`), never stored. `link` is written by
`credit_link_goodwill`, `treaty` by `credit_treaty_goodwill` and `breach` by `end_treaty` (the
"Treaties" section). The components are the readout's "reasons"; proportional fade (`x -= x ×
fade_per_turn` on each) treats them identically, so the sum fades exactly as one number.

- **`Goodwill::credit(holder, about, source, amount)` is the one seam.** It ignores a self-pair
  (an opinion of your own people is meaningless — never stored, never credited, so no call site
  checks) and a non-finite or zero amount. When a credit would carry the total past `[-1, +1]` it is
  **scaled down so the total lands exactly on the bound**; existing components are never rescaled.
- **Order in a turn.** `fade_goodwill` runs in `TurnStage::Influence`, the first stage; the stationed
  credit runs in `Population`; stage sets are chained, so the fade precedes every credit the turn's
  systems make. A steady source `s` settles at `s / fade_per_turn` (a post of ten: `0.01 / 0.02 =
  0.5`). `credit_stationed_goodwill` sits in the Population chain between `advance_dealings` and
  `bill_and_stock_roads` — it reads each party's phase (settled by `advance_expeditions`) and
  `PopulationCohort`, which the rest of the chain writes, so both edges are declared. Entries that
  fade to exactly nothing are dropped.

### Worth — the receiver's judgement, never a price table

`sim_runtime::worth` (re-exported by `core_sim::goodwill`, which supplies the loads and the holdings
and nothing else): `Σ over goods ( load_g × amount_g / (amount_g + held_g) )`, `held_g` the
**receiving band's holding of that good immediately before the transfer**, in the same unit. Food,
fodder and each material id are separate goods (a material's quality batches sum by amount within
that material only); `load_g` is the load of the gift's whole amount of that good in `carry.rs`'s one
currency (`CarryLoad` of one unit × the amount), so the fraction is what the receiver's holding
discounts. A good they have none of counts at its full load; one they hold ten times over counts for about
a tenth. An empty gift is worth `0`. The number becomes goodwill and nothing else — no exchange is
accepted or refused on it, and it is never published as a price. **The formula lives in
`sim_runtime`, the crate the sim and every player process both depend on**, so the goodwill credit and an
AI people's judgement of an offer are one piece of arithmetic that cannot drift; the AI restates only
the loads (the `carryFodderWeight` / `carryMaterialWeight` echoes on its own band rows) and reads the
holdings off its own frame.

### The sources

| Source | Credited by | Pair | Size |
|---|---|---|---|
| a gift lands (an accepted dealing with an empty return) | `answer_dealing` | receiver → giver | `per_load × worth(cargo)` into `gifts` |
| an exchange settles (an accepted dealing with a return) | `answer_dealing` | receiver → sender, and sender → receiver | `per_load × worth(cargo)` and `per_load × worth(return, held = the sender's HOME band's holding)`, into `exchanges`; both at acceptance |
| a stationed party | `credit_stationed_goodwill` | host people → sender people | `per_person_stationed × party workers`, each turn it is `Stationed` at a foreign host, into `stationed` |
| a standing link | `credit_link_goodwill` | each people → the other, **once per people pair per turn** | `per_link`, into `link` ("The exchange stages" below) |
| an agreed trade settles on arrival | `advance_expeditions` (`settle_agreed_trade`) | receiver → sender, and sender → receiver | as an accepted offer's exchange, at settlement |

A refusal, a lapse and a failed exchange credit nothing. A gift's pair only ever crosses peoples: a
same-people dealing (the standing answer off) credits a self-pair, which the seam drops.

### Who knows it — news is carried

A people knows its own opinion live (`Goodwill`). What another people thinks of it is
`KnownGoodwill` (`SimState`): `(viewer, other)` → the other people's components as last reported and
the turn they were observed. **It moves only when a party carrying the news lands it**:

- `Expedition::goodwill_news` (`other faction → GoodwillNews { components, turn }`, rides the
  checkpoint with the party) is written by `advance_expeditions`' observe pass **at each sighting of a
  foreign band** — the value at that sighting turn, so a scout and a trade party at its destination
  both record it. A stationed party's per-turn bundle takes the map into its `StationReport::goodwill`.
- It lands where the party's other news lands: a scout or trade party at its **comm flush** (near
  home), a stationed party when its **report lands**. Never earlier — the turn a gift is credited the
  giver's `KnownGoodwill` has not moved.
- `KnownGoodwill::record` keeps the newer `turn` and **never moves backwards** (a report no newer than
  the held one is ignored), and ignores a self-pair — `connections.md`'s clock-1 rule applied to an
  opinion.

### The wire

`GoodwillSection` appended last on both envelopes (`goodwill` rows diffed whole, `cuts` diffed
whole), **viewer-scoped**: the viewer's own `KnownGoodwill` rows. A row (`GoodwillState`):
`otherFaction`, the six components, `total` (their sum), `reportedTurn`. `GoodwillCutsState`
(`waryBelow`, `friendlyAbove`) rides the section once, off `diplomacy_config.json`, so the client
never restates the word cuts. A delta's absent `cuts` table means unchanged.

**`own` — the viewer's OWN live opinion** (`WorldSnapshot::own_goodwill`, appended last,
`GoodwillSection.own`, diffed whole): one `GoodwillState` per other people the viewer's people holds
an entry about (`snapshot::goodwill::own_goodwill_states`, from `Goodwill::iter()` filtered to the
viewer as holder), `otherFaction` the people thought **about**, the six components and `total`, and
`reportedTurn` the **current tick** — it is live, because a people's opinion is its own to know. It is
a different list from `goodwill` above (what others are *known* to think, carried news) and never
carries another people's opinion. Two readers: a client shows "what we think of them", and an AI
weighs a proposal on it (`worth_received × (1 + goodwill_toward_sender)`). A pair never credited has
no row; a reader takes that as `0`.

**An offer's `partyWorkers` is published** (`Dealing::party_workers`, set where the offer is posted —
a trade party's arrival and a scout's handover — from the party's available workers). It bounds the
return the party can walk home (`partyWorkers × carryPerWorker`), which the answering people needs to
size one; a proposal's and counter's `partyWorkers` is the shipper's, as before (`SAVE_FORMAT_VERSION`
42 carries it).


## Treaties — road rights, open borders and agreed trade (slice 2, plan §3)

`treaties.rs`. A treaty is a standing promise between **two peoples**: `Treaties` (`SimState`, keyed
by the **unordered** faction pair, low id first) holds `Treaty { road_rights, open_borders,
agreed_trade: Vec<Bundle>, formed_turn }`. **A treaty with no term true and no bundle is not stored**
(`Treaties::set` removes it), a people has no treaty with itself (every reader answers "no term" for a
same-people pair, so no caller compares factions), and the readers are `road_rights(a, b)`,
`open_borders(a, b)` and `bundles(a, b)`. A `Bundle` is two `GoodsLines` — `sent` (from the
**shipping** people's point of view) and `returned` — unrated, as a stage-4 proposal's legs are; a
bundle must send something (an empty `sent` would be covered by any cargo, any number of times).

### Proposing: `DealingKind::TreatyProposal`, and how it travels

A treaty proposal is a `Dealing` with `treaty: Some(TreatyProposal { terms, carrier })` — **the full
terms asked for** (the wire token `"treaty_proposal"`). Accepting **replaces** whatever treaty stood:
amending is proposing again, and dropping a term that way is **not** a breach. There are two carriers
(`TreatyCarrier`):

| | `Road` (`propose_treaty`) | `Envoy` (`send_envoy`) |
|---|---|---|
| posted | the turn it is made, `dealings::propose_treaty` | when the envoy reaches the receiver's camp (the `Arrived` arm of `advance_expeditions`) |
| gates | the proposer's resident band, a live tie to the destination (`check_destination`), a **standing link** (`bands_have_standing_link`), a band of **another** people, at least one term, every bundle valid | the same gates but the link, checked at launch (`handle_send_envoy`); the destination must be another people's |
| the receiver answers | only while the standing link still holds, else it is withdrawn (`reason=link_broken`) | while the envoy stands in the camp (`AwaitingOrders`); needs nothing else |
| the proposer learns | **at once** | **when the envoy folds home**, as one feed line (`reported=envoy`) |
| silence | refuses it at `expires_turn` (`offer_wait_turns`), told to both | the envoy waits `offer_wait_turns`, then walks home with `Expired` |

**The receiver's acceptance is in force at once, either way.** What the envoy carries home is the
*news*, not the effect.

`send_envoy` launches a **trade party with no cargo** — the envoy is `ExpeditionMission::Trade`
whose terms are `ShipmentTerms::Envoy { proposed, outcome }`, published as `expeditionMission`
`"envoy"` (`ExpeditionMission::as_str`) — so it reuses every arm a shipment has: provisioned like a
scout, walks to `Expedition::aim`, the same arrival rule, the same lost-destination guards, the same
fold-back. `outcome: Option<EnvoyOutcome>` (`Accepted | Refused | Expired | Countered(terms)`) is
what the answer wrote on the party for the fold-back to read (`dealings::report_envoy_home`).

### The answer, and the counter

`answer_dealing <f> <id> accept | refuse | counter <terms>` (a treaty counter is `counter
[road_rights] [open_borders] [bundle give … ask …]…`, `AnswerDealingCommand.counter_terms`, proto
**7**; `counter give … ask …` stays the stage-4 trade counter). A counter carries the receiver's
**full** terms and posts a new `TreatyProposal` the other way (a counter naming no term is refused —
refuse instead). **A counter is a new proposal and travels like one.** A road-carried one is
answered under the stage-4 link rule and its counter goes back along the road. A counter made to an
envoy standing in the camp rides home with that envoy and is posted into the book at the fold-back,
addressed to the envoy's own people. That dealing may be **accepted or refused** at home with nothing
carrying the answer (its effect is immediate in the sim, like every treaty acceptance), but **a
further `counter` on it is refused** unless a standing link joins the two bands at that moment — it
then posts along the road (`carrier = road`); otherwise the failure reason tells the player to send
an envoy (`send_envoy` with the new terms). The refusal leaves the dealing pending and untouched.

### Ending: `end_treaty`, and the breach

`end_treaty <faction> <other_faction>` removes the treaty at once and **is a breach**
(`treaties::end_treaty`): the other people's goodwill toward the breaker takes `-goodwill.broken_treaty`
into the `breach` component; the breaker's own opinion does not move; both are told. **Closing the
border toward a people you hold an open-borders term with**
(`set_open_borders … <other>`, below) is the same breach — it ends the whole treaty with the same
drop. **A treaty dissolves silently, with no breach**, when either people stops existing
(`dissolve_extinct_peoples`, Visibility: a people exists while it has a resident band); the survivor
is told it lapsed.

### Per-people borders: `FactionBorderPolicies::admits(treaties, host, from)`

**ONE function** answers "does `host`'s people take in `from`'s?", in this order: **own people**
(always); an **open-borders treaty term**; the host's **override** for that one people
(`set_open_borders <faction> <open|closed> <other_faction>`); the **default** (`default_open`, the
old `is_open`). It replaces `is_open` and `admits_posting` at **every** reader — the unhappy trickle's
destination (`walkable_destination`, shared with the breeding shed), a defecting party
(`advance_party_defection`), a stationed party's launch and its every-turn post check
(`resolve_post`, `advance_expeditions`). The frame publishes the viewer's overrides beside the default
(`FactionPolicyState.borderOverrides`).

### Road rights and the reading of a foreign road

Road rights change how a **foreigner's walk** reads a kept road — see `routes.md` → "A foreigner's
walk reads a share of a built road". They are read by every walk that goes through
`routes::path_reach_tiles` (work parties' walks, the unhappy trickle's reach, the culture term and
belief relay, supply pooling) with the walking people's treaties. A standing link is unchanged: a
foreign kept road is still a kept road.

### Agreed trade

A shipment (`send_trade_expedition`, gift or offer) between two peoples whose treaty carries bundles
settles what the bundles cover **on arrival** (`systems::expeditions::settle_treaty_bundles`):
`treaties::match_bundles` takes **whole bundles greedily, in bundle order**, while the cargo covers
each one's `sent` (`kᵢ` copies of bundle `i`). The covered part settles at once through
`dealings::settle_exchange` — the one exchange routine — as an agreed exchange: the receiver pays
`Σ kᵢ × returnedᵢ` (booked `ExchangeOut`/`ExchangeIn`), exchange goodwill rises both ways, and a
`kind=exchange` line is told to both. **The payment walks home on `Expedition::exchange_return`**,
apart from `cargo`, because an accepted offer for the *remainder* lands the whole cargo in the
receiver and must not take the payment with it (the fold-back books it `ExchangeIn`). **The remainder
follows the shipment's own terms** — a gift to another people, or an offer, becomes a pending dealing
carrying only the remainder; nothing left means nothing pending and the party turns home. **If the
receiver cannot pay the return, the covered part fails as stage 4 does** (`receiver_short`): nothing
lands, no goodwill moves, and the covered goods walk home on `Expedition::unpaid_return` — they
never changed hands, so the fold-back books them `ShipmentReturned` (the cause an undelivered shipment
uses), where a paid bundle's payment is `ExchangeIn`. A same-people shipment never reads treaties.

### Treaty goodwill

`treaties::credit_treaty_goodwill` (`TurnStage::Visibility`, after the fade and after
`dissolve_extinct_peoples`, behind `credit_link_goodwill`): each way, `goodwill.per_term × terms` into
`treaty`, where terms = road rights (1) + open borders (1) + (1 if there is any bundle). It settles near
`per_term × terms / fade_per_turn`, and refreshes `KnownGoodwill` both ways (a treaty is between two
governments that know it).

### The wire

`TreatySection` is appended last on **both** envelopes, **viewer-scoped**: `treaties` — the treaties
the viewer's people is party to (`TreatyState`: the other people, `terms` (`roadRights`, `openBorders`,
`bundles` as two goods lists each) and `formedTurn`) — and `standingLinks` — every pair of a viewer
band and another people's band the viewer holds a live tie with that a kept road joins end to end
(`routes::standing_link`; `StandingLinkState { viewerBand, otherBand, otherFaction }`). `DealingState`
gains `treaty` (the proposal's full terms; empty on other kinds) and `carrier` (`"road"` / `"envoy"`),
`PopulationCohortState` gains `envoyTerms` (an envoy party's errand), and `FactionPolicyState` gains
`borderOverrides`. `SAVE_FORMAT_VERSION` 42 carries `Treaties`, the overrides, `Dealing::treaty`,
`ShipmentTerms::Envoy` and `Expedition::exchange_return`.

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
| `tests/goodwill.rs::the_frame_carries_the_viewers_own_live_opinion_and_nobody_elses` | `GoodwillSection.own` on the **encoded** envelope: the viewer's rows only, components and total, `reportedTurn` the current tick, a people with no entry publishes none, and it is not the carried-news list |
| `tests/ai_dealings_scenario.rs` | a real `server` and a real `sim_ai` over the sockets: a human-side scout posts an offer to the AI's people, the AI answers it, and the return lands at the sender's home as `ExchangeIn` with the human people's `own_goodwill` credited — all read off the encoded frames the human seat is sent |
| `tests/goodwill.rs` | a gift credits receiver → giver only; an exchange credits both by what each received; refusal and lapse credit nothing; no self-pair entry; a post of ten follows `0.5 × (1 − 0.98^n)` turn by turn and settles near `0.5`; news reaches `KnownGoodwill` only with the returning party, stamped with the sighting turn; the frame carries the viewer's own rows and the cuts; checkpoint round trip |
| `goodwill.rs` unit tests | worth (none held = full load, 10× held ≈ 1/11, per-good, empty = 0), clamp lands on the bound without rescaling components, fade is proportional on every component, a stale or equal report never overwrites a fresher one |
| `tests/stages.rs` (stage 4) | a proposal over a kept road is posted at once, draws nothing, and is on the encoded frame of both peoples with the shipper named and unrated lines; no link (or a bare tile, an unpaid bill, a trail) refuses it and records nothing; a counter closes the original and goes back, accept means the last terms and the party size carries; an agreed party settles on arrival, goods both ways and goodwill both ways, the return home as `ExchangeIn`; a receiver short of the return fails the exchange with no goodwill and the cargo comes home; a link broken before the answer withdraws the dealing with `reason=link_broken`; the answer's own refusals leave the book as it stood; silence expires a proposal; the checkpoint round trip |
| `tests/stages.rs` (stage 1) | a `give` scout turns toward the first stranger and posts a pending offer, spent; an accepted gift credits receiver → giver and the scout resumes its recorded destination; an `offer` scout comes home with the named return as `ExchangeIn`; a refusal and a lapse bring the pack home; a band the home band holds a tie with is no stranger; `keep` never hands over; a lost stranger leaves the order unspent |
| `tests/stages.rs` (link goodwill) | one credit per people pair per turn however many band pairs, `KnownGoodwill` refreshed both ways with no party; no road, no credit; an unkept road holds no link |
| `bin/server.rs` `tests::{an_accepted_proposal…, a_countered_proposal…, a_proposal_the_sender_cannot_honour…, a_proposal_with_no_standing_link…, a_scouts_pack…}` | the launch half: an accepted proposal sends the party with the give as cargo and `Agreed` terms; a counter launches on its terms; a sender that cannot honour its leg fails the dealing; the pack is drawn at launch and fails closed |
| `tests/treaties.rs` | a treaty proposed over a kept road is posted at once, told to both, accepted into force for both **from that turn**, and on the **encoded** frame (the treaty, the standing link, the proposal's terms and carrier) for its two peoples and no third; a counter carries full terms back and accepting it puts those in force; an accepted proposal amends a standing treaty with no breach; the fail-closed gates (no link, no term, an empty bundle, an unknown good, your own people); a vanished road withdraws a road-carried proposal; `end_treaty` costs the breaker the **other** people's goodwill only; a people ceasing dissolves its treaties without a breach; treaty goodwill follows `s(1 - (1 - fade)^n)/fade`; an envoy posts on arrival, the receiver's acceptance is in force at once and the proposer hears only at the fold-back; an envoy's counter is posted to its people at the fold-back and answered with no road; an unanswered envoy lapses; bundles settle whole and the remainder follows the shipment's terms (and is pending), an exactly covered cargo leaves nothing pending, a receiver short of the return fails the covered part with no goodwill, a same-people shipment never reads a treaty; the viewer's border overrides and an envoy's errand are on the encoded frame; a treaty round-trips a checkpoint |
| `routes::foreign_road_tests`, `work_party::tests::a_foreign_road_shortens_the_walk…`, `resources::border_admission_tests` | the foreign-road rule tile by tile, a path's weakest tile for its own walker, a real caller (`resolve_walk`), and `admits`'s precedence table |
| `bin/server.rs` `tests::{an_envoy_launches…, a_border_override_closing…, end_treaty_is…, a_treaty_proposed…}` | the handlers: the envoy's launch and refusals, an override closing to a partner breaks the treaty, `end_treaty`, `propose_treaty` and a treaty counter's reply path |
| `sim_ai` `specialists::dealings::tests::a_treaty_proposal_is_accepted…` | the AI accepts a treaty at its goodwill threshold and refuses below it |
| `tests/sim_state_coverage.rs` | `PendingDealings` is classified `SIM_STATE`; the config handle and metadata are `CONFIG` |

## See also

- `.claude/rules/core_sim/expeditions.md` → "A shipment is a party that WALKS IT" — the trade verb
  this extends
- `.claude/rules/core_sim/factions.md` — the seat gate and the viewer-scoped sections
- `.claude/rules/core_sim/save-game.md` — the version table
