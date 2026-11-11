# Plan: Far kin — splitting to better land without losing the band

Status: **design, decided in conversation with Ray (2026-11-10).** Manual entry: §"Leaders — Chief,
Tribe, City, Nation" → *The tribe*. Builds on `docs/plan_contact_and_logistics.md` (ties, kin relay,
shipments), `docs/plan_standing_upkeep.md` (the route ladder) and `docs/plan_labor_at_scale.md`
(tribes).

## Why this doc exists

From many playthroughs, Ray's finding:

- **Splitting a couple of times and staying within 3 tiles is a strong opening.** The bands pool their
  larders freely and balance each other.
- **Splitting farther than 3 tiles to reach better land gets nothing from being one people.** The bands
  might as well be separate factions.

What the code does today, and why it comes out that way:

| What | Reach | Why far bands miss it |
|---|---|---|
| Free pooling (sharing larders) | `supply_network_config.json` → `reach_tiles` **3** | Past 3 tiles there is no link at all |
| Tribe locality | same supply network **and** same belief anchor (`tribe.rs`) | No network past 3 tiles, so a far pair is never *local*, earns no tribe points, and any bank fades |
| A trail (the free road rung) | holds a link **6** tiles out, no upkeep | It is worn in by traffic, and the main traffic is pooling — which a far pair does not have. Marches wear `0.05` per worker per tile against the trail's `40`, so a trail between far kin effectively never forms |
| Knowledge | faction-wide | Shared, but nothing is learned *faster* because bands are far apart |

The trail row is the trap: the one free thing that would link far kin needs the link to exist before
it can wear in.

## Decided

Three changes. Each is small on its own; together they make "far but in touch" a real position
between "local" and "separate".

### 1. A tribe is held together by a live tie, not by the supply network

**Local** (the test the tribe points bank runs on) becomes: two bands of **one people**, holding **the
same belief anchor**, with a **live tie in both directions** (`ConnectionLedger`, each direction's
strength above `NO_TIE`). The supply-network condition is removed.

- A tie stays live through sight, kin relay (`kin_relay.max_hops`, today 2) and scout / station
  reports, and lapses about **50 quiet turns** after the last contact (`strength.decay_per_turn`
  0.02). That clock now also decides how long far kin stay local.
- The score inputs do not change. "Knowing each other" already reads tie strength, so a fading tie
  already earns less each turn before it lapses.
- **Grouping** in `tribe.rs` moves from `(people, network, anchor)` sets to `(people, anchor)` sets,
  with the connected components of close pairs taken over the tie graph. The identity rules
  (`resolve_matches`) are unchanged: "a network split keeps the tribe on the majority side" reads
  as "a tie-graph split" with the same tie-breaks.
- Leaving a tribe: a band leaves when it no longer holds a live tie to the rest, instead of when it
  leaves the network.

### 2. A split's founding walk lays a full trail

When the player commands a split (`split_band_from_parent`, the wrapper the `split_band` verb runs —
not the breeding break-off or the independence break-away), the new band is tagged as on its
**founding walk**. The tag clears on the first move order it completes.

While the tag holds, the march traffic it records on each tile it crosses
(`RouteTrafficLog::marched`) is banked **straight to the top of the free floor**
(`routes::traffic_ceiling`, a full trail) instead of at `work_per_worker_tile`.

- **No new upkeep rule is needed.** A trail is already free to hold (no verb, no keeper, no bill); it
  is lost only to disuse (`disuse_grace_turns` 4 + `40 / disuse_loss_per_turn` ≈ 44 turns after the
  last traffic).
- **It keeps itself up while it is used.** At 6 tiles or less, the trail makes the two camps pool,
  and pooling traffic crosses the trail every turn, so the trail never reaches disuse. Past 6 tiles
  the trail helps people walk (friction 0.85) but holds no link, so without deliveries it fades in
  about 44 turns.
- **It is in the ground, not on the band.** A band that later moves off the trail loses the link
  (the weakest-tile rule), exactly as with any road. That is what item 3's longer base reach is for.
- It applies to every commanded split, near or far. For a split that stays within 3 tiles it changes
  nothing that matters: those bands already pool.
- The traffic still counts as traffic for `roadbuilding` (`routes::credit_route_lessons`), so a far
  split moves a people toward Graded Roads. That is intended.

### 3. Messengers — a knowledge learned by keeping far kin in touch

One new knowledge, `messengers`, on the knowledge screen beside `ritual` (area `lore`). It is not a
unit and has no verb.

**How it is learned.** Through `LadderKnowledge::ledger_credit`, the seam `ritual` and the crafts use,
credited **once per people** (knowledge is faction-level, never scaled by hands):

| Practice | Credit | Lever (opening value) |
|---|---|---|
| Each turn any resident band of the people holds a live two-way tie with a kin band standing farther than the free pooling reach | a flat amount per turn | `messengers_lesson_per_turn` **0.25** |
| A trade expedition or envoy between two bands of the same people arrives | per tile walked, so a longer trip teaches more | `messengers_lesson_per_tile` **0.5** |

`lesson_costs.messengers` **20**, the same as every other taught knowledge. At the opening values,
passive contact alone takes 80 turns; a few long deliveries cut that sharply. That is the point:
distance is what teaches it.

**What it unlocks** — each a value the stat *takes* once the people knows Messengers (the crafting
arc's convention: an effect names the value, never a multiplier), each in the config that owns the
stat:

| Stat | Without | With Messengers | Lever |
|---|---|---|---|
| Free pooling reach | 3 tiles | **4 tiles** | `supply_network_config.json` → `reach_tiles_with_messengers` |
| Kin relay depth | 2 bands | **3 bands** | `connections_config.json` → `kin_relay.max_hops_with_messengers` |
| Tie decay | 0.02 a turn (~50 quiet turns) | **0.01 a turn (~100)** | `connections_config.json` → `strength.decay_per_turn_with_messengers` |

**Why 4 tiles and not more.** The reach must stay below the trail's 6, or Messengers would give every
far pair a trail's link for free and the founding-walk trail would add nothing. 4 is an opening value;
5 is the ceiling. Validated at load: `reach_tiles <= reach_tiles_with_messengers < trail's
holds_link_to_tiles`.

**Pooling only.** The Messengers reach changes `supply::balance_supply_networks`' link test and
nothing else. `WalkReach` (migration and the culture term) and the work party's local range keep the
shipped `reach_tiles`, and the road bonus stays measured above the shipped `reach_tiles`. Otherwise
learning Messengers would shrink what a road does for a walker.

**Discovery id 2025** (the next after `ritual`'s 2024), added to `start_profile_knowledge_tags.json`
and `lesson_costs`.

## How the three fit

| Bands are | Before | After |
|---|---|---|
| ≤ 3 tiles apart | pool, can form a tribe | unchanged |
| 4–6 tiles, split and walked out | separate in all but name | the founding trail links them; they pool, the pooling keeps the trail, and they can form a tribe |
| 4–6 tiles, camp moved off the trail | separate | a tribe while in touch; pool at 4 once Messengers is known |
| Farther, in touch | separate | a tribe while the tie is live; deliveries and envoys between them teach Messengers |
| Out of touch | separate | separate — a people that loses contact really has split |

## Scope

**Sim (`server-dev`):** `tribe.rs` locality and grouping; a founding-walk tag set in
`split_band_from_parent` and read where march traffic is banked; the `messengers` knowledge, its two
credit seams (`advance_connections` for the passive tie, the shipment arrival in
`advance_expeditions`), and the three unlock reads; config fields and validation; the rules
`leadership.md`, `connections.md`, `routes.md`, `fission.md`, `campaign.md` updated to match.

**Client (`client-dev`):** `messengers` on the knowledge screen; wherever a band's tribe progress
explains why a pair is not local, it names the lapsed tie ("out of touch with X"), not the supply
network.

**Out of scope:** automatic deliveries between kin by a chief (only if playtests show manual
deliveries are a chore); gatherings as a tribe input (considered and dropped: local bands never
re-gather and far ones would only every ~50 turns).
