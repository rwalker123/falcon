# Early Diplomacy — offers, goodwill and treaties

Issue **#233** (arc #231). This is the **policy** layer over the contact substrate
(`docs/plan_contact_and_logistics.md`, arc #527): who may do what over a connection, and what one
people thinks of another. It builds three things, in order, because each reads the one before it:

1. **Offers** — the exchange design #546 settled (`plan_contact_and_logistics.md` → "Settled by
   #546"), built: stages 1, 2 and 4.
2. **Goodwill** — what one people thinks of another, raised by every positive dealing.
3. **Treaties** — promises between two peoples about roads, borders and trade. The diplomat
   (exchange stage 3) is a treaty term.

What this plan does **not** change: the connection stays a raw primitive with no rider vocabulary
(`connections.rs` module doc), and the keystone holds — nothing here grants `Seen`.

## 1. Offers

### What #546 already decided, and is not reopened here

- A shipment is an **offer**. The receiving people accepts or refuses it. An offer that asks nothing
  back is a **gift**.
- A **standing answer** resolves an offer the turn the party arrives. Every people holds *"accept
  gifts from my own people"*, on by default, so a parent supplying its splinter is unchanged.
- Every offer no standing answer covers is a **pending offer**: addressed to the receiving people,
  answered by an ordinary command on their next turn. **Silence is a refusal** after
  `offer_wait_turns` (opening value **1**).
- **Refused cargo goes home** over the existing `ShipmentReturned` path.
- **The return rides the same party home.** One round trip, so the payment can be lost on the way.
- **A price is a list of goods, never a number.** Food, fodder and material batches with their
  quality readings, per material. The sim decides only accept or refuse on a concrete offer.

### The stages, as built

| Stage | Who names the return | When | How word reaches the receiver |
|---|---|---|---|
| **1. Scouts** | the receiver | when the scouts arrive | the scouts are standing there |
| **2. Blind trade party** | the receiver | when the party arrives | the party is standing there |
| **3. Diplomat** | both, in advance | before the party leaves | a treaty term (§3) |
| **4. Standing link** | both, haggling | any time | along the kept road |

**Stage 1 — the scouts' pack.** `send_scout` gains an optional pack (cargo, the same manifest a
shipment takes, bounded by the party's carry) and a **standing order for strangers**: `give`,
`offer` or `keep` (the default). A **stranger** is a band of another people that the party sights and
that its home band holds no live tie with. The first stranger the party sights while the order is
`give` or `offer` becomes the pack's destination: the party turns toward it and hands over at the
shipment arrival rule (comm-range proximity, destination seen). `give` lands as a gift (accepted bare; no return). `offer`
becomes a pending offer that the receiver answers by naming a return, or by refusing. Then the party
resumes the scouting aim it had, carrying any return. **One pack, one handover**: the order is spent
once used. The player learns what happened when the scouts come home, with the contact news.

**Stage 2 — the blind party.** `send_trade_expedition` gains `offer` beside today's gift form. The
receiver answers the pending offer by naming a return from its own holdings, or by refusing. **A gift
carries no return**: a receiver that accepts a gift accepts it bare, and the sim refuses a return named
on one. **The
sender then takes it or walks home**: when the receiver accepts an offer whose sender asked for
something back (a trade party sent as `offer`, a scout's `offer`), nothing moves — a **reply** goes to
the sender naming the return, and the sender accepts it (the exchange settles, goodwill is credited
then) or declines it (the party walks home with its own cargo). Silence on the reply is a decline after
`offer_wait_turns`. A gift (a shipment to another people, a scout's `give`) is given away, so the
receiver's accept settles it at once whatever return it names. The party waits up to `offer_wait_turns`
at each step, then walks home with the return or with its own cargo.

**Stage 4 — haggling over a kept road.** When the trace between two bands is a **kept road on every
tile** (rung `min_route_rung` (**2**) or above, upkeep paid — the condition §Q5 lights tiles `Seen`
with), a band may **propose** a trade that names both legs: what it sends and what it asks back. No
party walks yet. The proposal reaches the receiver **the turn it is made**, because word travels along
a kept road. The receiver answers on its next turn: **accept**, **refuse**, or **counter** with a
different return list. A counter is a proposal in the other direction, answered the same way. Each
answer is one turn, so haggling costs time, and silence is a refusal after `offer_wait_turns`. No
counter cap: each round costs the haggler a turn.

**An accepted stage-4 trade launches the sender's party with the agreed cargo.** On arrival the
exchange settles at once, because the return was agreed. If either side no longer holds its leg, the
launch is refused with a reason (sender), or the exchange fails on arrival (receiver) and the party
walks home with its cargo. A failed exchange moves no goodwill.

**A stage-1 or stage-2 receiver also sees the cargo before answering.** It is standing in its camp.

### One pending item shape

Every pending item — an offer, a reply, a proposal, a counter, a treaty proposal (§3) — is one record:
**addressed to a people, answered by a command, expiring on silence**. `PendingDealings` holds them,
`SimState`, `BTreeMap` keyed by a monotonic id. `answer_dealing <faction> <id> accept|refuse|counter
[goods…]` is the one answer verb, gated like every faction-bearing order (seat gate, membership).

### The wire

`DealingsSection` (appended on both `WorldSnapshot` and `WorldDelta`), **viewer-scoped**: the items
addressed to the viewer's people and the items the viewer's people sent. Each row: id, kind, sender
people and band, receiver people and band, both goods lists, turns left. Another pair's dealings are
theirs to know.

**A trade history.** The event feed keeps 20 turns, so the sim keeps the trades: `TradeHistory`
(`SimState`), per people a list of records — turn, own band, the other people and band, the dealing,
an outcome (`exchanged`, `gift_given`, `gift_received`, `delivered`, `declined`, `refused`, `expired`,
`withdrawn`, `failed`) and the goods that left and came in — newest last, capped at
`history.trade_history_len` (opening **100**). One record per people per trade, from its own point of
view. `TradeHistorySection` rides both envelopes, viewer-scoped to the viewer's people's records.

## 2. Goodwill

### What it is

**One number per ordered pair of peoples**: how much people **A** thinks of people **B**. It runs
from **−1** (hostile) to **+1** (friendly), starts at **0**, and is held in `Goodwill`, `SimState`,
`BTreeMap<(FactionId, FactionId), f32>`. It is a people-level opinion, not a band-level tie, and it
is never stored on a connection.

### It fades

Every turn, before this turn's credits, each entry fades toward zero by a proportional
`fade_per_turn` (opening value **0.02**):

```text
goodwill -= goodwill × fade_per_turn
```

**Proportional, not linear, on purpose.** A steady source paying `s` a turn then settles at
`s / fade_per_turn` and stays there. So goodwill reads as *what you are doing for us now*, plus the
fading memory of what you did before, and a standing dealing cannot be farmed into a climb that never
stops. The result is clamped to `[−1, +1]`.

### What raises it

Two kinds of source: **bumps** (one event) and **steady** (every turn a thing stands).

| Source | Kind | Who thinks better of whom | Size |
|---|---|---|---|
| a gift lands | bump | receiver → giver | `per_load × worth` |
| an exchange settles | bump | each → the other | `per_load × worth of what each received` |
| a stationed party | steady | host → sender | `per_person_stationed × people` |
| a standing link (a stage-4 road between them) | steady | each → the other | `per_link` |
| a treaty term in force | steady | each → the other | `per_term × terms` |

**A gift's worth is the receiver's judgement, never a price table.** A global table — gold worth 10,
fibre worth 1 — is `trade_goods` again, the scalar #537 retired. So worth is measured against what the
receiving band already holds:

```text
worth = Σ over goods in the gift ( load_g × amount_g / (amount_g + held_g) )
```

`load_g` is the good's carry load (the one carry unit, `carry.rs`). `held_g` is what the receiving
band holds of that good, in the same unit as `amount_g`. **A good they have none of counts at its full
load. A good they already hold ten times over counts for about a tenth.** Gold beats fibre exactly
when gold is rare to them, and food counts most to a camp with an empty larder. The number becomes
goodwill and nothing else: it is never published as a price, and no exchange is accepted or refused
by the sim on it.

**Traffic over a standing link counts through its exchanges.** Each settled exchange is a bump, so a
busier link earns more without a rate estimator. The link's own steady term pays for the road
standing between them.

### What lowers it

**Breaking a treaty** (§3): the other people's goodwill toward the breaker drops by `broken_treaty`
(opening value **0.5**) at once. There are no raids on peoples today (a raiding party erases a herd),
so this is the only negative source. The scale runs negative so a future hostile act has somewhere to
land.

**A refused offer costs nothing.** Saying no is not an insult, and taxing a refusal would push the AI
into bad deals.

### Who knows it

**A people knows its own opinion live.** The AI reads its own goodwill toward others directly.

**What another people thinks of you arrives as news.** `KnownGoodwill`, `SimState`, keyed
`(viewer, other)`: the other people's goodwill toward the viewer, as last reported, and the turn it was
reported. It is refreshed when a party of the viewer's people comes home from a band of that people
(a scout, trade party or envoy), when a stationed party's report lands, and every turn a standing link
stands between them (word travels along a kept road). It is never live otherwise. Clock 1's
discipline, applied to an opinion.

### The wire and the words

`GoodwillSection` (both envelopes), viewer-scoped: one row per other people the viewer has known
goodwill for — the people, the reported value, the turn reported. The client shows it as a word in the
Peoples tab, **wary / neutral / friendly**, cut at `wary_below` (**−0.2**) and `friendly_above`
(**0.2**), with the reported turn and the reasons on hover. The thresholds are sim config published
on the wire, so the client never restates them.

## 3. Treaties

### What a treaty is

**A promise between two peoples, held by both.** `Treaties`, `SimState`, keyed by the unordered
faction pair, holds the terms in force between them. A treaty has no end date: either side may end
it, and ending it is breaking it (§2). It dissolves with no penalty when either people stops existing.

### The three terms

| Term | What it does |
|---|---|
| **Road rights** | each people's built roads count fully for the other's walks |
| **Open borders** | each takes in the other's leavers and hosts the other's parties, whatever its general setting |
| **Agreed trade** | a standing exchange, settled on arrival with no waiting turn — the diplomat (stage 3) |

### Road rights — a foreigner pays a share

**Without road rights, a foreigner's walk over a built road gets `foreign_road_share` of what the rung
buys** (opening value **0.9**). Built means `dirt_road` and above. **A path or a trail stays public at
full value**, because traffic made it, and no foreign reading falls below the trail's own value.

```text
reach_for(walker, tile) = full                                       if keeper's people == walker's people
                                                                         or a road-rights treaty joins them
                        = max(trail_reach, floor(full × foreign_road_share))   otherwise
```

It applies where a road's payoff is read per walker — `routes::path_reach_tiles`, which takes the
walking people as a parameter — so the work party's walk bonus and every later payoff (vehicles) carry
the penalty through one seam. It does **not** change which tiles qualify as a kept road for stage 4:
a foreign kept road is still a kept road.

**The penalty is a share, not a fallback to a trail**, so it scales with whatever a road buys later.
Reach is in whole tiles, so on a short road the floor makes the real penalty larger than 10% (a dirt
road's 6 extra tiles become 5). That is accepted: road payoffs are already quantised the same way, and
the share is a lever.

### Open borders — per people

`FactionBorderPolicies` keeps one faction-wide default and gains a **per-people override**:
`set_open_borders <faction> <open|closed> [<other faction>]`. With no other faction it sets the
default, as today; with one, it sets the override for that people. `admits(host, from)` reads, in
order: own people → yes; an open-borders treaty term → yes; the override; the default. The frame
publishes the viewer's own default and overrides.

**An open-borders term overrides a closed default or override.** Closing your borders to a treaty
partner, by override, breaks the treaty.

### Agreed trade — the diplomat as a standing answer

The term holds **bundles**: a list of goods sent and a list of goods returned, both concrete. A
shipment between the two peoples carrying `k` whole bundles is covered by the term: it settles on
arrival, the receiver returning `k` return bundles. Cargo beyond the whole bundles is an ordinary
offer. If the receiver does not hold the return, the covered part fails as a stage-4 exchange does,
and goodwill does not move. **This is the standing answer #546 left room for**: an offer whose return
was agreed before departure.

### Proposing a treaty

A treaty proposal is a pending item (§1) naming the terms, with the bundles for agreed trade. It
reaches the other people by **an envoy** — a new `ExpeditionMission::Envoy`, the trade party's arms
with no cargo, launched by `send_envoy <faction> <band> <party_workers> <destination_band_id> <terms…>`
— or **along a kept road**, the turn it is made, as stage 4's proposals do. The receiver answers
accept, refuse or counter with different terms. Accepted, the treaty is in force from that turn for
both peoples, since the proposer agreed by sending. The envoy walks the answer home, and the proposer
learns of it on return. A road-carried answer lands at once.

**Amending a treaty is a new proposal.** Accepted, it replaces the old terms. Dropping a term by
amendment both sides accepted is not a breach.

**Ending one is a command**, `end_treaty <faction> <other faction>`, taking effect at once. It is a
breach, and costs the breaker the goodwill drop in §2.

## 4. The AI

An AI people answers every pending item by command on its turn, exactly as a human does (`TurnQueue`):

- **An offer or proposal** is accepted when what it receives is worth more to it than what it gives,
  both measured by §2's worth function against its own holdings, with its goodwill toward the sender
  added to the received side.
- **A stage-1/2 return** is named from its surplus — goods it holds most of relative to need — sized
  so its worth to the sender, as far as the AI can judge from what the sender carried, matches the
  cargo's worth to the AI.
- **A treaty proposal** is accepted when its goodwill toward the proposer is at or above
  `treaty_goodwill` (opening value **0.2**, the friendly line).

**The AI's levers are AI tuning, so they live in `sim_ai/data/ai_profiles.json`**, per seat, on the
other side of the socket (`factions.md` → "AI tuning is not a start-profile field") — never in the
sim's `diplomacy_config.json`, which the AI process cannot read.

**The AI answers and does not initiate in this issue.** It sends no gifts, offers or proposals of its
own; goodwill toward a human player rises through the human's dealings. The initiating half is
**#829** (arc #666).

## 5. Config — `src/data/diplomacy_config.json`

| Key | Opening | Meaning |
|---|---|---|
| `offers.offer_wait_turns` | **1** | turns a pending item waits for an answer before silence refuses it |
| `goodwill.fade_per_turn` | **0.02** | proportional fade toward zero each turn |
| `goodwill.per_load` | **0.02** | goodwill per unit of worth, for gifts and exchanges |
| `goodwill.per_person_stationed` | **0.001** | per stationed person per turn |
| `goodwill.per_link` | **0.004** | per standing link per turn |
| `goodwill.per_term` | **0.002** | per treaty term in force per turn |
| `goodwill.broken_treaty` | **0.5** | the drop a breach costs |
| `goodwill.wary_below` / `friendly_above` | **−0.2 / 0.2** | the word cuts |
| `roads.foreign_road_share` | **0.9** | a foreigner's share of a built road's payoff |

All opening values, to be tuned in playtest. Loader `diplomacy_config.rs` on the shared boot seam,
env override `DIPLOMACY_CONFIG_PATH`, strict (`config-loading.md`). At the opening values a post of
ten settles at `0.01 / 0.02 = 0.5` goodwill, a standing link at `0.2`, and three treaty terms at
`0.3`.

## 6. Persistence

`PendingDealings`, `Goodwill`, `KnownGoodwill` and `Treaties` are `SimState` and move the save
version. `Expedition` gains the scout pack, its order and the envoy's proposal; they ride
`ExpeditionRecord`. `FactionBorderPolicies` gains its overrides.

## 7. The client

- **The scouting sheet** gains the pack and the strangers order (give / offer / keep).
- **The trade sheet** gains *offer* beside *gift* and, where a kept road joins the bands, *propose*
  with a return list.
- **There is one Trade tab, not a Trade tab and a Dealings tab.** It is a single scroll of three
  sections: *Waiting on an answer* (the trade items addressed to you with accept / refuse / counter, a
  reply with *Take it* / *Reject it*, and your own outgoing items with turns left), *This turn* (what
  crossed the store) and *History* (every trade that ended, newest first, from `tradeHistory`). Two of
  its sections are unbounded lists, which is why the tab scrolls.
- **The Peoples tab** shows each people's goodwill word, its treaty terms, a propose-treaty action and
  an end-treaty action, and the per-people borders toggle. **Treaty proposals addressed to you wait
  here**, above the treaties in force, with the same accept / refuse / counter; a treaty is not a
  trade, so it is not on the Trade tab.
- **Event feed** lines for a gift landing, an exchange settling or failing, a refusal, a treaty
  formed, amended or broken.

## 8. Sequencing

1. **Offers and goodwill** — the pending-item shape, stages 1, 2 and 4, the answer verb, goodwill
   with its sources and fade, `KnownGoodwill`, the AI's answers, the wire and the client.
2. **Treaties** — the object, the envoy, road rights through `path_reach_tiles`, per-people borders,
   breaking.
3. **The diplomat** — agreed trade bundles as a standing answer.

## See Also

- `docs/plan_contact_and_logistics.md` — the substrate, and "Settled by #546" for the exchange design
- `.claude/rules/core_sim/connections.md` — the connection primitive and the keystone
- `.claude/rules/core_sim/routes.md` — what a rung buys, and the seams the road penalty enters through
- `.claude/rules/core_sim/factions.md` — Open Borders and the seat gate
- `.claude/rules/core_sim/expeditions.md` — the shipment, the stationed party, the scout
