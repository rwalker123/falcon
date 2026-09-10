# Research: the population-collapse death spiral

Status: **Research complete. No behaviour changed.** This document is the answer to issue #431 —
whether the observed unrecoverable population collapse is a real defect or working as intended — and
the decision it hands back. It proposes candidate directions and pre-commits to none; the fix is a
separate slice.

## The verdict

**It is a real defect, it is not the one the issue describes, and it is not primarily about food.**

The spiral the playtest saw is a **tile temperature** effect, and the temperature that causes it is
**6.5 °C** — a reading the game draws as comfortably temperate, warns about nowhere, and does not
kill anybody directly. Below it a band's morale falls to zero, its productivity modifier floors at
×0.5, and the food each worker brings home drops from 0.400 to 0.200 against a break-even of
**0.2298**. The band then declines at a fixed rate for ever. The direct cold deaths the model *does*
warn about only begin at 0 °C, and by the time they do the band was already dead.

**Ray's later suspicion is confirmed.** He wondered whether he had been standing on a cold tile and
had missed it because it was not reported. He had, and he could not have known: the tile was drawn
survivable, and the death feed spent every death on the phrase *"an elder died of old age"*.

## What was measured, and how

Every number below comes from the **shipped** `advance_demographics`, `discontent_fraction`,
`discontent_output_modifier` and `food_demand`, driven from the shipped `demographics_config.json`,
`wellbeing_config.json` and `simulation_config.json`. The harness lives in
`core_sim/src/systems/population.rs` (`mod collapse_research`), is `#[ignore]`d so it never runs in
CI, and prints rather than asserts:

```
cargo test -p core_sim --lib collapse_research -- --ignored --nocapture --test-threads=1
```

⛔ **It asserts nothing on purpose.** Every threshold it prints is one this report calls wrong, and a
test pinning them would freeze the defect as the specification.

## Finding 1 — the issue's hypothesis is false

The issue supposed that *"if deaths are not biased toward dependents, the [dependency] ratio can
drift monotonically the wrong way"*. Starvation mortality **is already biased toward dependents** —
`scarcity.child_vulnerability` and `elder_vulnerability` are both **1.5** against
`working_vulnerability` **1.0** — and the ratio therefore drifts the **right** way. Run a band at
0.20 food/worker (just under break-even) at a mild temperature:

| turn | head | dependents/worker | food deficit |
|---|---|---|---|
| 0 | 30.0 | 0.686 | 13.0 % |
| 20 | 23.3 | 0.598 | 9.6 % |
| 40 | 15.4 | 0.577 | 8.8 % |
| 60 | 10.3 | 0.575 | 8.7 % |

The band gets **worker-heavier** and the shortfall **narrows** — and it dies anyway. The negative
feedback the issue was looking for exists, works, and is not enough.

## Finding 2 — the model is scale-free, so there is no floor to find

Food demand is proportional to weighted mouths; food income is proportional to workers. Both scale
with the band, so **income ÷ demand is a function of the bracket shape alone and never of the
head-count**. Being small does not make food easier. The ratio asymptotes (Finding 1) and then sits
there, which is why the decline is a fixed percentage per turn with no bottom.

That also makes the boundary a **cliff, not a slope**. At 18 °C, 400 turns, income scaling with the
workforce:

| food/worker/turn | end head-count |
|---|---|
| 0.2000 | 0 (extinct) |
| 0.2250 | 26 |
| 0.2300 | 112 |
| 0.2400 | 306 |
| 0.4000 | 23 929 |

Break-even is **0.2298**. A 3 % change in per-worker income is the difference between extinction and
twenty-fold growth. There is no band of inputs that produces a stable population.

**This is a description of the model, not necessarily a defect** — see Finding 3, which is where the
missing floor is supposed to come from.

## Finding 3 — the land is the intended floor, and morale takes it away

`forage::forage_take` is `min(worker_cap, take_ceiling)`: the crew's throughput **or** the patch's
escapement room, whichever is smaller. When the **land** binds, income stops scaling with the band
and the model gets exactly the equilibrium it lacks — head-count settles at a stable multiple of the
patch's yield (≈ 7.4 people per food/turn), which is carrying capacity working as designed:

| patch cap (food/turn) | settled head-count |
|---|---|
| 1.0 | 7.4 |
| 4.0 | 29.6 |
| 8.0 | 59.3 |
| 20.0 | 148.1 |

So the model is only floorless in the **crew-limited** regime — when the band's own gathering
throughput, not the land, is what is short. Three things put a band there, and each is a
multiplication on the same term:

| what a gatherer carries | food/worker | vs break-even | outcome |
|---|---|---|---|
| bare hands (`per_worker_biomass_capacity` 1.6 × 0.05) | 0.080 | 0.35× | dies |
| flint baskets (`forage_carry` 8.0 × 0.05) | 0.400 | 1.74× | viable |
| flint baskets at the morale floor (×0.5) | 0.200 | **0.87×** | **dies** |

**Equipment is not an optimisation, it is the difference between viable and extinct** — bare-handed
gathering cannot feed a band at any size, on any land, at any temperature. And the morale
productivity floor is very nearly as large a lever as the basket itself: it cancels more than half of
what the basket bought, and lands the band just under the line.

## Finding 4 — the two cold thresholds are 6.5 °C apart, and the game only knows about one

`tile_morale_pressure`'s climate term is `(|T − 18| − 9) × 0.004` against a settling gain of `+0.01`
per turn. It breaks even at **6.5 °C**. Below that, morale falls every turn, hits the 0 clamp, and
stays there; `discontent_fraction` saturates at 1.0 and `discontent_output_modifier` floors the band
at **×0.5** output for ever. Cold *deaths* are a separate mechanism with its own onset —
`demographics_config.json` → `cold.onset_temp` — at **0.0 °C**.

| tile temp | morale settles at | output × | cold deaths | equipped band after 300 turns |
|---|---|---|---|---|
| 10.0 | 1.000 | 1.00 | no | 4 440 |
| 7.0 | 1.000 | 1.00 | no | 4 236 |
| **6.0** | **0.000** | **0.50** | no | **1.1** |
| 3.0 | 0.000 | 0.50 | no | 0 (extinct) |
| 0.0 | 0.000 | 0.50 | no | 0 (extinct) |
| −10.0 | 0.000 | 0.50 | yes | 0 (extinct) |

**One degree — 7 °C to 6 °C — separates a band of four thousand from a dead one.** Nothing in the
model, the wire or the client marks that line.

### How much of the map is on the wrong side of it

`latitude_base` is linear from `equator_temp` 30 °C to `polar_temp` −5 °C, so at sea level:

- **6.5 °C** is reached at latitude fraction 0.671 → the outer **32.9 %** of the map's rows are
  economically fatal.
- **0 °C** is reached at latitude fraction 0.857 → the outer **14.3 %** of rows are flagged lethal.

So roughly **a fifth of the world kills bands while being drawn as survivable**, before
`elevation_lapse` (up to −12 °C) pushes highland tiles at far lower latitudes over the same line.
These are derived from the climate curve, not counted on a generated map — worth confirming against
a real export before tuning against them.

## Finding 5 — two independent reporting defects, and they compound

**(a) The tile is drawn survivable.** `TileSurvivability.is_lethal` — the single authority the tile
chip's ⚠, the map overlay's hatch and `AttentionController._decline_reason` all read — tests
`temperature < cold_onset_temp`, i.e. **0 °C**. Between 6.5 °C and 0 °C the ground ends the band and
every one of those three surfaces says it is fine. This is the same class as issue #614 (a
`Temperate` tile that killed); that fix moved the *death* onset and the *morale* onset was never
re-examined beside it.

**(b) The death feed spends every death on "old age".** `push_demographic_events` accrues deaths on
**one** carry across all three brackets and labels the resulting event with the single
largest-contributing bracket and *its* cause. `flows.elder_deaths` folds in the flat
`elder_mortality_rate` of **0.20**, which outweighs almost everything else, so the elder bracket wins
that comparison nearly always. A well-fed band of 30:

| tile temp | child deaths | worker deaths | elder deaths | what the feed says | deaths unspoken for |
|---|---|---|---|---|---|
| −1.9 | 0.042 (cold) | 0.059 (cold) | 0.449 (age) | "An elder died of old age" | 18 % |
| −5.0 | 0.110 (cold) | 0.156 (cold) | 0.467 (age) | "An elder died of old age" | 36 % |
| −10.0 | 0.219 (cold) | 0.311 (cold) | 0.495 (age) | "An elder died of old age" | **52 %** |
| −15.0 | 0.329 (cold) | 0.467 (cold) | 0.524 (age) | "An elder died of old age" | 60 % |

The sim knows the per-bracket cause and puts all three on the wire. The feed drops two of them.

**Together they are the whole of Ray's "I could have missed it".** The tile said fine; the feed said
old age; the population fell. Nothing anywhere named the temperature.

## What this does NOT find

- **The labor allocator does not fall off a cliff.** `LaborAllocation::normalize` sheds one hand at a
  time down an eleven-step ladder that gives up scouts, idle warriors and spare keepers long before
  any food row, and reports every trim including partial ones.
- **`death_fraction` takes the max of hunger and cold, never their sum**, so a starving *and*
  freezing band is not double-counted. At any food deficit at or above 25 %, hunger dominates cold at
  every temperature the map produces — cold is the binding term only on a **fed** band.
- **The population cap is not involved.** `simulation_config.json`'s `population_cap` of 25 000 never
  fires at these scales.

## Candidate directions — for decision, not pre-committed

Grouped by which finding they answer. They are not alternatives to each other.

**For the unwarned lethal band (Finding 4 + 5a) — the smallest change with the largest effect:**
1. Give the client a **second** threshold from the wire — the morale break-even — and let the tile
   chip, the overlay and the decline reason distinguish *"this ground kills people"* from *"this
   ground ends your band"*. No model change at all.
2. Or align the two onsets deliberately, so the temperature at which morale starts draining and the
   temperature at which cold starts killing are one decision rather than two independently-tuned
   numbers that happen to be 6.5 ° apart.

**For the feed (Finding 5b):**
3. Name every bracket that lost someone, or name the cause with the largest **share of deaths**
   rather than the largest bracket. Either makes cold visible without touching the model. Overlaps
   #625, which found the same masking from the other end.

**For the missing floor (Findings 2 + 3):**
4. Leave it. The land *is* the intended floor and it works; the defect is that morale silently pushes
   bands out of the land-limited regime. Fixing Finding 4 may be the whole fix.
5. Or soften the productivity floor, so a discontented band is degraded rather than pushed across the
   break-even line by a single multiplier. `productivity.floor_mult` 0.5 against a basket margin of
   1.74× is what makes ×0.5 exactly lethal; the two numbers were tuned apart.

**Explicitly not recommended:** age-weighting starvation mortality further (it is already weighted,
and Finding 1 shows it is not the problem), and letting children or elders contribute labor (it
changes the break-even by changing the shape, but the cliff is still a cliff — it moves the line
without giving the model a floor).

## See Also

- `docs/plan_settlement_population.md` — the arc this belongs to; demographics are its Phase 1.
- `docs/plan_population_growth_model.md` — the fertility factors this report holds constant.
- `docs/plan_early_game_labor.md` — the per-worker forage rates and the TOE/equipment tiers that
  Finding 3 measures against.
- `docs/plan_civ_wellbeing.md` — the morale → discontent → productivity chain of Finding 4.
- `.claude/rules/core_sim/campaign.md` — the demographics/turn-loop as-built notes.
- Issue #625 (elder mortality's flat rate) and #614 (the `Temperate` tile that killed) — the two
  places this report's reporting defects were each half-seen before.
