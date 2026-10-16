# Plan: Heritage and Influence — two ladders, and the victories they feed

Status: **design, manual-first.** Manual entries: §"Organic Settlement" (Heritage and the settlement
ladder) and §"Victory Conditions" (the victory list). Supersedes the Cultural Diffusion wording in
[`docs/plan_roaming_bands.md`](plan_roaming_bands.md) §The mobility score.

## Why this doc exists

The game has two 0–100 scores per people, and neither has a job:

- **The Sedentarization score** (on screen: the Settling block's stage row, `Nomadic 10/100`). Its
  only readers are the Telling: the 40 and 70 crossings post a message and a fork, and they move the
  narrator's voice from oral to painted to written. Nothing about play reads it.
- **The Mobility score** (#800). Its only planned reader was the Cultural Diffusion (Nomadic) victory
  (#234), which is not built.

Beside them sits a third ladder that ignores both: a band's settlement stage (Nomadic band → Seasonal
camp → Village, `settlement_stage_config.json`) is decided by head count alone. The faction page
therefore said "Nomadic" twice with two meanings, and showed two meters a player could not act on.

What was decided in the #800 review: the two scores get era-neutral names, each climbs a ladder of
rungs that pays a bonus, and the scores feed the victories. On screen, the victories lead and the
scores are their details.

## Decided

### The two scores: Heritage and Influence

| Score | Was | What it measures | Inputs today | Inputs later |
|---|---|---|---|---|
| **Heritage** | Sedentarization | how deeply a people holds the land it stands on | tamed herds and sown or tended plants, food to spare, the land's richness, head count, belief underfoot (the dead) | gatherings (#698), a monument (#692), granaries, fields, buildings, walls |
| **Influence** | Mobility | how far a people's world reaches beyond where it stands | the roaming skills (herd lore, travois, drives), steps walked with a followed herd | roads and route rungs, ties held, shipments, treaties (#233), later ships and flight |

- **Neither name belongs to an era.** A city has deep Heritage; a trading empire has wide Influence.
  "Nomadic", "Settling" and "Mobility" all describe the early game and go.
- **They are not opposites.** Early on they pull against each other (a band cannot camp on its dead
  and follow the horses at once); late on a people can hold both.
- **The formulas are the shipped ones**, renamed: `sedentarization_tick` becomes Heritage's, and
  `mobility_tick` becomes Influence's. Their inputs grow by adding terms, never by a second score.
- **"From the dead" is not a row.** It is one Heritage input and is shown in Heritage's breakdown
  with the others.

### Two ladders, each rung paying a bonus

A band's rung on each ladder is earned by its Heritage or Influence score **and**, on the Heritage
ladder, its head count. Each rung's bonus is a multiplier or offset on a number the game already
has, so a bonus is one term at an existing seam, not a new system.

**The rule that keeps neither path better:** a bonus pays only while the band plays the way that
earned it. Settled bonuses are worth nothing to a band that keeps moving; roaming bonuses are worth
nothing to a band that stays. Both scores are smoothed, so switching paths costs the turns it takes
to rebuild.

| Rung | Heritage ladder (the settlement stages) | Earned by | Bonus | Seam |
|---|---|---|---|---|
| 0 | **Nomadic band** | — | — | — |
| 1 | **Seasonal camp** | `min_size` 50 and Heritage ≥ 40 | the larder keeps longer | `spoilage::band_shelf_life` |
| 2 | **Village** | `min_size` 100 and Heritage ≥ 70 | works land one hex further out | `band_work_range`, read per band |
| 3 | later | later | improvements cost less to keep | standing upkeep (`docs/plan_standing_upkeep.md`) |

| Rung | Influence ladder | Earned by | Bonus | Seam |
|---|---|---|---|---|
| 0 | (none) | — | — | — |
| 1 | **Trail band** | Influence ≥ 40 | sees one hex further | effective sight range per source (`visibility_systems`) |
| 2 | **Herd people** | Influence ≥ 70 | ties with peoples it meets form faster | `gain_per_contact` at `record_contact`, for this observer band |
| 3 | later | later | long moves shed less; walking away from the dead costs no morale | `carry::move_ferry_reach_tiles`; the culture term's `away_drag` |

- **The Heritage ladder is the settlement stage ladder** — the same three rungs, the same names, the
  same `settlement_stage_config.json`, with a Heritage criterion beside `min_size`. It is not a
  second ladder. The score's own stage words (`Nomadic` / `Seasonal base` / `Ready to settle`) retire.
- **Every threshold and bonus is a config lever** with an opening value, settled by playtest. The 40
  and 70 above reuse the score's existing thresholds.
- Rung names on the Influence ladder are provisional.

### The victories: specialists and one balanced win

| Victory | Kind | What it measures |
|---|---|---|
| **Cultural** | specialist | one people's Influence winning over another's Heritage (below) |
| **Diplomatic** | specialist | treaties and goodwill (#233), with Influence as what diplomacy spends |
| **Military** | specialist | conquest: holding the other peoples' capitals |
| **Economic** | specialist | a dominant share of trade (the manual's existing entry) |
| **Scientific** | specialist | late Great Discoveries and an Ascension project (the manual's Ascension) |
| **Legacy** | balanced | leading across the board: population, production, stability, Heritage and Influence |

- **The manual's Hegemony was two victories.** Its "hold all opposing capitals" clause is the Military
  victory; its share of population and production plus stability is the core of Legacy. Hegemony
  retires as a name.
- **Legacy counts Influence as well as Heritage.** If only Heritage fed the balanced win while
  Influence fed Cultural and Diplomatic, Influence would be worth more. The balanced win rewards a
  people that did some of everything.
- **Stewardship and Survival stay as the manual has them** until the crisis arc is designed; they are
  about the world's crises, not about a people.

### The Cultural victory is a contest, not a race

A people's Influence pushes against another people's Heritage. It wins culturally when its Influence
over every other people outweighs that people's own Heritage: its ways have won them over.

- **Heritage is both a bonus ladder and a defence.** A deeply rooted people is hard to win over.
  Influence wins games; Heritage keeps you from losing them.
- **Influence over a people travels on ties.** Contact is band to band and culture is already a rider
  on a connection (`docs/plan_contact_and_logistics.md`), so "Influence over people B" is read off the
  ties A's bands hold with B's bands, not off A's score alone.

### A victory never counts a capped score

Both scores are 0–100 against fixed reference values, so every people can reach 100. "First to 100"
would end every long game in a tie. A victory measures something relative instead: a share of the
world total, points banked over turns, or (Cultural) one people against another.

### The faction page leads with the victories

The faction page lists the **enabled victories** with each one's progress. What feeds a victory —
Heritage, Influence, their inputs, the rung each band holds — is its **detail**, on hover or a
popover, in the game's own words: what it does for you and what raises it. A band's own rungs show
on its page. A score with no reader is not shown: nothing appears on the page that the player cannot
use.

## As built today

- `victory.rs` knows `Hegemony`, `Ascension`, `Economic`, `Diplomatic`, `Stewardship` and `Survival`;
  `victory_config.json` enables Hegemony only. Hegemony's progress is
  `0.45 × population + 0.25 × morale + 0.2 × grid relief + 0.1 × surplus`, an absolute measure.
  Diplomatic's is a placeholder (morale plus a turn ramp). There is no Cultural kind and no Military
  kind.
- `SedentarizationScore` and `MobilityScore` are per people (faction); the settlement stage is per
  band.

## Open

- **Per-band scores.** Proposed: Heritage and Influence become per band, so the ladders (per band)
  read a band's own score and a people can hold a village and a roaming hunting band at once. The
  skills part of Influence stays per people, since it is knowledge; the people's value is the
  population-weighted mean. Not yet confirmed.
- **Losing a rung.** Proposed: a band drops a rung when its score falls below the rung's threshold,
  so a band cannot earn rungs standing still and then wander off with them.
- **The victory measures** — the share or points rule for each victory, the Cultural contest's
  per-tie arithmetic, and the thresholds. Workbench levers once built.
- **Which victories a default game enables.**

## Slices

Proposed issues; none filed yet.

1. **Rename the scores to Heritage and Influence** — the sim's resources, wire fields, config files
   and client vocabulary; the faction page rows retitled; "From the dead" folded into Heritage's
   breakdown.
2. **Per-band Heritage and Influence** — the open item above, settled first because both ladders read it.
3. **The Heritage ladder** — the settlement stages gain a Heritage criterion; rungs 1 and 2 bonuses.
4. **The Influence ladder** — the rungs, their names, rungs 1 and 2 bonuses.
5. **Victory list rework** — split Hegemony into Military and Legacy; add Cultural; the relative
   measures (updates #226–#229, rescopes #234).
6. **The Cultural contest** — Influence over a people read off ties, against that people's Heritage
   (#234, rescoped).
7. **The Diplomatic victory** — on treaties and goodwill (#233), spending Influence.
8. **The faction page leads with victories** — progress per enabled victory, feeders as details.

## See Also

- [`docs/plan_roaming_bands.md`](plan_roaming_bands.md) — the roaming skills and the mobility score.
- [`docs/plan_settlement_population.md`](plan_settlement_population.md) — the settlement stages and
  the sedentarization tether.
- [`docs/plan_contact_and_logistics.md`](plan_contact_and_logistics.md) — ties, and culture as a
  rider on them.
- [`docs/plan_early_diplomacy.md`](plan_early_diplomacy.md) — goodwill and treaties.
