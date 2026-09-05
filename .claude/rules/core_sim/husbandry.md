---
paths:
  - "core_sim/src/{fauna,fauna_config,intensification}.rs"
  - "core_sim/src/data/intensification_ladder.json"
  - "core_sim/tests/{fauna_husbandry,grazing_2d_pen,rollback_tended_survival}.rs"
---

<!-- Extracted verbatim from lines 1589-2062 of core_sim/CLAUDE.md at blob dcc757587f8c9308590997ee600abc64a34e6712
     (the PRE-SPLIT original — read it with `git cat-file blob dcc757587f8c9308590997ee600abc64a34e6712`;
     core_sim/CLAUDE.md itself is now the hub, where the routing table lives).
     Regenerate with scripts/split_claude_md.sh -->

# Husbandry — the yield ladder, the `Tame` verb, Corral

## ⛔ THE ANIMAL WEB IS ONE POSITION TOO — read this before any `domestication_progress` reference below

**A herd has ONE number: `Herd::ladder_position`, how far up the animal branch it has been worked, in
cumulative work units** — the exact twin of `ForagePatch::ladder_position` (`cultivation.md` → "THE
PLANT WEB IS ONE POSITION"; `docs/plan_standing_upkeep.md` §2.8, landed for plants in §4.10 and for
animals in §4.11). `domestication_progress` / `domestication_cost` and `corral_progress` /
`corral_cost` — **four fields, two unconnected meters** — are gone. Prose below that names them is
describing the retired shape; the seam it points at is `Herd::rung_work_done`, the position read into
a rung's own span **through the herd's standing**, which is what the wire's `domestication` and
`corralProgress` meters are still published from (**the raw meter fraction** — `partial_credit`
governs what a half-built pen is *worth*, never what its progress bar reads).

**A rung the standing HOLDS reads FULL, and on `animal:pen` that is load-bearing rather than
cosmetic**: the pen's base is this herd's *taming* price, so the bare subtraction
`position − base` is a rival answer to `corral_meter_full()` that `f32` can make disagree with it.
Live on the plant web (a finished Field published `0.99999994`); latent here only because every
shipped `taming_cost_multiplier` happens to be exactly representable. `intensification.md` → "A
RUNG'S METER IS A **PUBLICATION** OF THE STANDING" owns the mechanism and the measurements.

- **`Herd::standing` is derived and re-stamped on every write**, and `set_ladder_position` is the only
  mutator, so the pair cannot drift. `is_domesticated()` and `is_corralled()` keep their signatures and
  their call sites.
- **THE PAYOUTS AND THE COST NOW INTERPOLATE, and that is the whole of §4.11's animal half.**
  `herd_density_gain` (the `K` multiplier) and `herd_ecology`'s `regrowth_rate` climb continuously with
  the position, as does `herd_upkeep_demand`. **Before this they were step functions on
  `is_domesticated()`** — a completion predicate — so a herd paid the *whole* pastoral keeping bill from
  the first turn of work and received *none* of the benefit until the last: 100% of the cost on day one,
  0% of the payout until day N, the §2.8 asymmetry inverted.
- **`herd_keeping_meter` IS RETIRED** (gravestone at its old site). `herd_claims_keeping` answers *does
  this source claim at all*, and `herd_keeping_rung` reads the standing. The demand takes **no verb**,
  exactly as `patch_upkeep_demand` stopped taking one — there is no step left for the
  Population→Logistics carry to straddle. The verb survives only on the *claim*, which is the one-turn
  carry.
- **`Herd::upkeep_demanded` is the stamped BILL**, the animal twin of `ForagePatch::upkeep_demanded` and
  present for the identical reason: an interpolated demand moves *within* the turn, so a fully-staffed
  keeping would otherwise read permanently short. `herd_keeping_basis` is its one reader.
- **THE PEN STILL STEPS, and it does so for free.** `animal:pen` declares `partial_credit:
  on_completion`, and `RungStanding::at` already zeroes `credit` for such a rung — so a herd raising a
  fence interpolates between wild and *pastoral* and reaches the pen's rate only when the fence closes.
  No call site tests for the pen. Half a fence is no fence; that is the deliberate difference from the
  Field, where half a sown field genuinely has half a crop in the ground.
- **⛔ A PEN'S CREW CURVE IS THE STALKING CURVE PLUS ONE `.min()`, AND THAT `.min()` IS THE HAUL.**
  `fauna::hunt_crew_take_curve` is **one function** since §4.9 item 12b: every row is
  `resolve_hunt_engagement(..).fight.expected_brought_down` — the room, the reach, the retreat and
  the fight, at the rung the herd stands on — and a **corralled** row then takes `.min(keepers'
  carry)` at the band's own `hunt_carry` tier — the same number a stalking party hauls at (issue
  #543); what the `is_corralled()` predicate decides is *where* the bound is applied, never *which
  rate*. So *"would another pair of hands buy me more"* has a real answer
  for a pen, and a pen row carries a real `low <= likely <= high` band like every other row.
  > **⛔ `pen_crew_take_curve` IS RETIRED, AND IT WAS THE LAST PEN EXEMPTION.** It priced the room,
  > the handling and the carry with **no retreat and no fight**, and published
  > `low == likely == high`. Before 12b it agreed with the take (both were fightless); after, it did
  > not — a bare-handed band with a penned aurochs was quoted nothing and paid nothing while
  > `hunt_useful_crew` and the Work board's `+` went on answering *"another pair of hands buys you
  > more"*. **A forecast and a readout disagreeing about one row is the defect class this slice
  > deletes**, so it was fixed here and not filed. Its two surviving terms were already inside
  > `resolve_hunt_engagement` (the room clamp and `herd_engage_rate`), so folding the pen back in
  > applies neither twice.
  >
  > **The carry `.min()` is UNREACHABLE on the shipped roster**, and that is measured rather than
  > assumed: it binds only where `body_mass × (attack − defense) ÷ durability` beats the crew's
  > `hunt_carry` tier, and the largest per-worker kill across all seven pennable species is the
  > aurochs' `120 × 14 ÷ 150 = 11.2` biomass a turn — under the **bare** tier's `12`, let alone the
  > equipped `40`. It is kept because it is the honest model and a retune can walk into it, and it is
  > exercised by an authored fixture
  > (`hunt_useful_crew_on_the_wire::a_pens_curve_is_bounded_by_what_its_keepers_can_carry_home`),
  > exactly as `pen_engage_gain`'s handling arm is.
  > **THE BRANCH IS AT THE ONE PRODUCER, so both transports inherit it** — the snapshot's
  > `huntUsefulWorkers` and the compose sheet's query rows. It was briefly the *client* deciding when
  > to disbelieve the sim (gating the field on an engagement-stage test of its own), which is the
  > shape this arc exists to remove: a number that does not apply must not be published and then
  > guarded downstream. **`0` means "no crew is useful here" on every hunt row and never "this row has
  > no such answer"** — the pen branch is why that collapse does not exist.
  >
  > Measured on a bare-handed band against a corralled Wild Aurochs: the stalking curve reads **0**
  > and the pen's own ceiling is **20** of a 24-hand pool. The guard's liveness assertion is what
  > earns that test — with the branch removed the whole socket curve is zeroes, so
  > `plateau == published == 0` and the equality **alone would have passed**.
- **⛔ THERE ARE TWO KEEPER COUNTS ON THE ANIMAL WEB, and the wire keeps them apart.** They answer
  different questions and one of them must never interpolate:
  - **`fauna::herd_upkeep_workers_needed`** — *how many hands does this herd's KEEPING BILL take*, =
    `ceil(herd_keeping_basis / PER_WORKER_OUTPUT)`. It **does** slide with the ladder position, because
    the bill does. It is what `upkeepWorkersNeeded` publishes and what the panel's keeper figure prints.
    The plant twin is `forage::patch_upkeep_workers_needed`; the identity is stated on the wire and the
    client is told to consume it **without arithmetic**, so a producer that lands between the two breaks
    a published contract rather than merely a number.
  - **`fauna::herd_herders_needed`** — *how many keepers does a herd of this species and this size want*,
    from head count over `animals_per_herder`. It is **not** a function of ladder position and must not
    become one. Three things pin that: `would_be_herders_needed` has to quote a crew at position **zero**
    (an interpolated answer there is `0`, the startup-lag bug it was written to close);
    `stabilize_herders_needed`'s hysteresis damps a **head count**, and a term also sliding with a build
    meter would be a second undamped flicker source in the same field; and the client's preview harnesses
    pin `herdersNeededIfManaged == herdersNeeded` on every managed herd, which interpolation breaks on
    every herd mid-`Tame`.

  > **One function used to answer both**, and §4.11 made that visible: once the demand interpolated and
  > the crew count did not, the herd card told the player to staff **2** keepers against a bill **1**
  > covered, at every position from 10% to 90% up the rung. The fix was to **separate them**, not to make
  > one impersonate the other — collapsing two real questions to satisfy an identity would have traded a
  > wrong number for a wrong model. **Every client reader of `herdersNeeded` is a boolean gate**
  > (`> 0` ⇒ *this herd is managed and owes keepers*); the count the panel prints comes from
  > `upkeepWorkersNeeded`, so the card corrected itself with no GDScript change.
- **The ecology's PHASE BANDS deliberately do not interpolate.** `r` is a payout and slides;
  `collapse_fraction` / `stressed_fraction` / `extinction_floor` are the classifier's cut points and are
  taken from the rung the herd **holds** — blending two definitions of *Collapsing* would invent a third.

> **⛔ AND THE MEASUREMENT THAT CAME OUT OF IT, because it is the reason the escapement floor is being
> changed.** The floor is `floor_fraction × K` and `K` is the density-boosted ceiling, so raising a
> rung raises the floor while the herd stays the same size. Measured on aurochs starting **exactly on**
> its floor, through a full `Tame`: the room above the floor reaches zero at turn **6** with one herder,
> turn **3** with four and turn **2** with eight — *the faster you build, the sooner you starve* — and
> because `eligible` reads that same room, **the tame then never completes at any crew size**. It is the
> `-4` escapement stall reached by the floor climbing rather than by over-hunting. Five of the eleven
> tameable species sit on the losing side of that race (aurochs, marsh grazer, reindeer, steppe runner,
> wild horse); only the fast breeders clear it comfortably. Interpolating turned the cliff into a slide
> and did not remove it.


## The husbandry yield ladder — every rung pays MSY

Authoritative design: `docs/plan_corral_managed_population.md`. **Management buys a *growth rate*, not
a licence to eat the standing stock.** Every rung of the ladder pays the Maximum Sustainable Yield; the
rungs differ *only* in the **ecology** that MSY is computed against, and in what that ecology costs you.
**Every rung costs a worker** (intensification ladder §3): what climbing buys is **yield per worker**,
not a rung that works itself.

**The husbandry ladder scales BOTH `r` AND `K`, on two orthogonal per-species dials.** `r` (the growth
*rate*) climbs on the global gains `husbandry.pastoral_gain` / `pen_gain` (folded per-species by
`herd_ecology`). `K` (the carrying *ceiling*) climbs on the per-species **density** dials
`SpeciesDef.pastoral_density` / `pen_density` (`fauna_config.json`, default **1.0** = neutral, so a wild
herd is byte-identical) — *domestication makes the land hold more animals, non-linearly by species*.
Without the density axis a species on marginal range (a goat penned at `K≈24`) stayed tiny even penned
and a fast wild breeder out-yielded the prime domesticates at every rung, because taming touched only
`r`. The gain for the herd's **current rung** is `fauna::herd_density_gain` (corralled → `pen_density`,
tamed → `pastoral_density`, wild → `1.0`; mirrors `herd_ecology`'s dispatch, resolved live by display
name via `FaunaConfig::pen_density_for` / `pastoral_density_for`), applied at the **one K seam**
`ecological_carrying_capacity` — it multiplies the final range/footprint-derived `K` (`Some(flow /
fodder × gain)`), so it is recomputed fresh each turn (idempotent, never a compounding read). Roster:
crag_goat/aurochs **2.0 / 5.0**, boar **1.5 / 4.0**, rabbit/fowl **1.1 / 1.5**,
steppe_runner/marsh_grazer **1.5 / 1.0** (pastoral only, so `pen_density` is inert), deer/mammoth omit
both (wild ceiling → always `×1.0`). Validated finite & `>= 1.0` (a gain below 1 would make
domestication *reduce* capacity). **Playtest dials.**

> #### The ladder is monotone in the LONG-RUN rate, NOT in any single turn — do not "fix" this back
>
> A **Sustain** hunt is **constant escapement on whole animals**: the herd hands over `B − K/2`, which
> is a **stock**, not a rate. At `B = K` Sustain's escapement is `K − K/2` = **`K/2` for every rung —
> `r` cancels out entirely**, so a full herd's first harvest is *identical* wild, pastoral and penned.
> **That is correct and load-bearing.** The surplus standing above the escapement point is
> *accumulated stock*, and stock does not care how fast you breed. What the ladder buys is that **the
> next animal comes sooner** — so *"management buys a growth rate"* is now literally and exclusively
> true, rather than being smeared across a stock term.
>
> **`docs/plan_harvest_floor.md` slice 1 made this true of the WILD hunt too, and changed nothing
> here.** It generalised the pen's harvest rule to a floor the stance names (`Surplus 0.30·K`,
> `Deplete 0.15·K`, `Eradicate 0`), so every rung on both webs is now the shape this callout already
> described. What the arc **did** change is which comparisons are meaningful: a stance ceiling is a
> stock and a rung payoff (`pastoralYield`, `corralYield`) is a long-run rate, so the two cannot be
> ordered against each other at a single turn — the ladder lives on the payoff axis, and the 600-turn
> average is where it is measured (`the_husbandry_ladder_is_a_per_species_growth_rate_ladder`, and the
> plan's own `stance_probe` property tests).
>
> A single turn therefore cannot see the ladder at **either** biomass: at `B = K` you read the
> rung-blind stock, and at `B = K/2` you read a **pulse** — zero for any species whose one-turn MSY is
> lighter than one animal (a wild mammoth regrows 120 biomass against an **800-unit** body, so it
> correctly **waits** ~7 turns, then pays 16 provisions at once). Both readings are honest; neither is
> the ladder.
>
> So the invariant is asserted as a **long-run average over enough turns to contain the refills**
> (`fauna_husbandry::the_husbandry_ladder_is_a_per_species_growth_rate_ladder`, 600 turns from `B*`),
> where the pulses and the stock both wash out and what remains is `r·K/4`. **Measured**
> (barren harness ⇒ the pen is fed entirely on hay):
>
> | species | `K` | wild | pastoral | pen gross | hay/turn |
> |---|---|---|---|---|---|
> | Rabbit Warren | 200 | 0.350 | 0.700 | 1.000 | 15.013 |
> | Red Deer | 1200 | 0.600 | 1.200 | 2.400 | 72.000 |
> | Thunder Mammoths | 12000 | 2.373 | 4.747 | 9.520 | 687.285 |
>
> **There is no `pen net` column, and there cannot be one.** It used to read `pen gross − upkeep`,
> both in provisions, because the feed came out of the same larder the yield was paid into — the
> modelling error this arc removed. A pen pays **provisions** and eats **fodder**: two stores that
> never trade, so the subtraction has no meaning and the hay column is a cost in its own currency.
> What a lush footprint buys is that the keeper does not have to farm that hay.
>
> Monotone in gross at every row, and `pastoral / wild` is exactly `pastoral_gain` (2.0). The **rabbit
> pen gross rides the cap** (`r_pen = min(husbandry_regrowth_cap 1.0, 0.35 × pen_gain 4.0) = 1.0`), so
> its pen/wild ratio is `1.0/0.35 ≈ 2.86`, not the full `pen_gain` — a fast breeder is clamped into the
> stable logistic band, the cap's whole job. The mammoth's negative *barren* pen net is the §2.4
> slow-breeder loss **by design** (a placement decision — on real pasture the footprint feeds it and
> `upkeep → 0`), not a regression.

| Rung | Ecology | `r` (Grazing 2d — **per-species**) | Costs |
|---|---|---|---|
| Wild, Sustain hunt | `ecology` | `wild_r` (rabbit 0.35 · deer 0.10 · mammoth 0.04) | a worker |
| Mobile domesticated (**pastoral**) | `husbandry.pastoral.ecology` | `min(cap, wild_r × pastoral_gain)` (gain 2.0) | **a worker** (a Hunt assignment, like a wild herd — passive-free pastoral is retired) |
| Corral, building | unchanged — the hunters beside the build carry what hunters carry | — | **the build's own crew** (`corral … <workers>`, hands not hunting) and **75 work units** (not a fixed turn count — the crew is the throughput, so the turns move with the number the player typed) |
| Corral, finished (**pen**) | `husbandry.pen.ecology` | `min(cap, wild_r × pen_gain)` (gain 4.0, cap 1.0) | a worker + **feed (footprint-offset)** + pinned |

- **Grazing 2d retired the flat pastoral 0.25 / pen 0.90.** The managed rungs now scale each species'
  **own wild `r`** by `husbandry.pastoral_gain` (2.0) / `pen_gain` (4.0), clamped to
  `husbandry_regrowth_cap` (1.0) — a penned rabbit (`r` 1.0, cap-bound and booming) and a penned mammoth
  (`r` 0.16, a long-haul investment) are different economies. This also fixes the fast-breeder pastoral
  inversion (pastoral `r` = `wild_r × 2.0 > wild_r` for every species). `fauna::herd_ecology` folds the per-species
  rate in; `pen_ecology_for` / `pastoral_ecology_for` are the seams, `managed_regrowth_rate` the `wild_r ×
  gain → capped` map.
  > **THE CAP IS A PEN-ONLY EFFECT, AND IT SILENTLY DISCARDS PART OF `pen_gain` ON THE FAST BREEDERS.**
  > Any species whose wild `r` exceeds `cap / pen_gain` = **0.25** cannot receive the whole pen bonus.
  > Of the seven **pennable** species, three lose some of it: **fowl** and **rabbit** forfeit **29%**
  > (`0.35 × 4 = 1.4`, delivered `1.0`) and **snow hare** **17%** (`0.30 × 4 = 1.2`).
  > `forest_grouse` and `river_fish` are also cap-bound but are `wild`-ceiling, so nothing can pen them
  > and the loss is unreachable. **The cap never binds at PASTORAL** — the fastest pastoral rate on the
  > roster is `0.70` — which is why the effect reads in play as the pen underperforming rather than as a
  > clamp. **It is also a retune trap**: raising `pen_gain` moves those three species not at all, so a
  > spread tuned on the big-game rows would silently fail to reach the small ones. The mechanism is
  > right — `r = 1.0` already doubles a herd every turn and an uncapped `1.4` is a discrete-logistic
  > oscillation — but the roster and the cap were authored against different assumptions. Recorded in
  > `docs/plan_standing_upkeep.md` §4.14 as a dial that arc measured but does not own.
- **A penned herd's `K` is its FENCED FOOTPRINT's graze flow** (`hex_range_tiles(corralled_at,
  pen_radius)`), recomputed each turn — penned herds are no longer frozen and `pen.capacity_fraction` /
  `pen_capacity` are **deleted** (a penned herd's `K` is just `herd.carrying_capacity`, so
  `herd_capacity` collapses to that field for every herd). A penned herd **grazes its footprint**
  (escapement-floored, like a wild herd) and the grass it eats is **the first of the two things that
  feed it**: `pasture_fraction = clamp(footprint_intake / (fodder_per_biomass × biomass), 0, 1)`, and
  **hay** covers what is left. A pen on lush steppe feeds itself for free (`pasture_fraction → 1`); a
  **wholly-barren** footprint keeps the herd's frozen `K` and lives entirely on hay — and, with none,
  goes unfed and shrinks. See "Phase 2d".
- **`fauna::herd_ecology(herd, fauna)` and `fauna::herd_capacity(herd, fauna)` are THE single source of
  that mapping.** `regrow_biomass`, `hunt_escapement_ceiling` (capacity only — the take reads no
  ecology at all), `hunt_forecast`, `refresh_ecology_phase`,
  the expedition ceiling/bound/simulation — **every** consumer resolves through them. **No call site may
  re-derive an ecology or a capacity**: a second copy of this mapping is exactly how a forecast starts
  promising a number the take won't pay (see "Pre-commit Yield Forecast").
- **⛔ EVERY RUNG IS DRAWN DOWN THROUGH ONE PATH — the "managed harvest" is retired.** A penned herd
  used to switch to a flat production with **no escapement floor, no engagement bound, no overdraw**
  and `sustainable == actual`. **Production and draw are separate concerns: a rung may change
  production; NO RUNG CHANGES THE DRAW.** A pen is hunted through the ordinary path — floor-live,
  worker-capped, engagement-bounded, drawn down — so **a pen can be over-hunted and the ⚠ fires on
  it**, exactly as the plant web's Field can (`cultivation.md` → "What a Field buys").
  > **The re-expression was EXACT, and here is why.** `pen_yield_biomass` was
  > `managed_yield_biomass`, whose whole body was `(biomass − capacity × MSY_BIOMASS_FRACTION)` — the
  > escapement ceiling with the floor **nailed** to Sustain and the ecology argument unused. The pen
  > was already taking an escapement ceiling; it simply refused to read the player's dial. So routing
  > it through `hunt_escapement_ceiling(floor, herd.biomass, herd_capacity(..))` changes **nothing** at
  > Sustain and everything at every other floor. Measured on the pinned `Rabbit Warren`
  > (`fauna_husbandry.rs` → `the_re_expressed_pen_lands_where_the_managed_rate_did`): wild **0.3510** →
  > pastoral **0.6966** → pen **0.9990**, before and after, to four figures. **No gain was retuned.**
- **THE ENGAGEMENT BOUND EXISTS AT EVERY RUNG.** A pen passed `f32::INFINITY`, and an infinite bound
  is not *"a penned animal is not stalked"* — it is **no bound**, and it is what let the pen's take
  escape every check the wild path applies. `fauna::herd_engage_rate` is the one seam: the species'
  own `engage_rate`, times `husbandry.pen_engage_gain` for a corralled herd, because a keeper genuinely
  handles far more animals per turn than a hunter — a **number**, not the absence of one.

  > **The shipped roster rarely reaches the handling arm**, which is why the pen's numbers did not move:
  > `pen_engage_gain` is authored at `20` precisely so the keepers' *carry* binds first on every
  > pennable species (the constraint on a keeper is carrying the meat home, not catching the animal).
  > The arm exists to be reachable — `fauna_husbandry::a_fractional_pen_handling_rate_collects_whole_animals`
  > authors a fractional rate to reach it, because the shipped one cannot.

  **AND AT THE PASTORAL RUNG TOO** — `husbandry.pastoral_engage_gain` (`2.0`), the same shape one
  rung down, through the same seam. It exists because **rung 2 was raising nothing that bound**:
  `pastoral_gain` moves the breeding rate, and no shipped source is limited by its herd running out —
  measured across the whole roster the take is capped by reach, by the fight or by the escapement
  floor, never by stock. So a band paid a taming cost for a take that did not move, and the *middle*
  of the animal ladder was where the climb stopped paying (measured: the pastoral rung sat **1.46×**
  behind its plant partner while the wild rung sat at **1.03×**). The fiction is habituation, and
  habituation **is** reach. Validated strictly between `1.0` and `pen_engage_gain`, the same
  monotonicity `pen_gain > pastoral_gain > 1` already follows.

- **⛔ THE TAKE RUNS ITS THREE STAGES AT EVERY RUNG, AND THE RUNG TUNES THE FIRST TWO ONLY**
  (`docs/plan_standing_upkeep.md` §4.9 item 12b). The corral-tend branch calls
  `systems::hunt_take` — the same function the range arm calls — with the pen's own two terms handed
  in: the **husbandry** carry tier and the assignment's floor. There is one take path, not two that
  agree.

  | stage | what the rung buys | seam |
  |---|---|---|
  | **engage** | `husbandry.pastoral_engage_gain` / `pen_engage_gain` × the species' `engage_rate` | `fauna::herd_engage_rate` |
  | **retreat** | `husbandry.pastoral_wariness` / `pen_wariness` × the species' `combat.wariness` | `fauna::herd_wariness` |
  | **fight** | pastoral: `husbandry.pastoral_resistance` × `defense` **and** `durability`. **Pen: there is no fight** | `fauna::herd_resistance` / `fauna::herd_fight_stage` |

  > **⛔ A PEN HAS NO FIGHT — YOU SLAUGHTER IT.** Everything the keepers reach and that does not
  > break off goes down: no attack-vs-defense gate, no durability grind, no wound ledger, no
  > casualties. `fauna::herd_fight_stage` answers `None` at `animal:pen` and `fauna::resolve_hunt_kill`
  > routes that to `fauna::slaughter`; `SourceYieldForecast::slaughters` carries the same fact into the
  > preview so forecast and take run one kill arm.
  >
  > **WHAT DID NOT GO: THE ENGAGEMENT BOUND AND THE ESCAPEMENT CLAMP.** The pen is still capped by
  > the keepers' handling rate (`pen_engage_gain` × the species' `engage_rate`) and by the room above
  > the floor, and its take is still quantised to whole animals. The infinite bound §4.10 ② retired
  > has **not** come back — what came back is the exemption at the one stage it was always about.
  > `SourceYieldForecast::fight` therefore stays `Some` at a pen, because that tuple is what the
  > *retreat* reads (`pen_wariness`); nulling it would delete the retreat with the fight.
  >
  > **A HALTER SOFTENS THE FIGHT, IT DOES NOT DELETE IT.** `pastoral_resistance` (`0.5`) scales both
  > halves of the resolver's gate on a tamed quarry — `defense` (whether a strike lands at all) and
  > `durability` (how many landed strikes a body absorbs) — so rung 2 is a real fight against a
  > softer animal while rung 3 is no fight at all.
  >
  > **A PENNED ANIMAL CAN NO LONGER HURT ITS KEEPERS.** The three pennable species that carried an
  > `attack` — Wild Aurochs `4.0`, Wild Boar `1.5`, Crag Goats `0.6`, Wild Sheep `0.4` — and the
  > baseline `combat_config.hunt_injury_damage_per_animal` that applied to every fight regardless, all
  > stop reaching a pen crew. *"A contained bull still gores"* was the previous ruling and is retired
  > with the fight it belonged to.
  >
  > **AND BARE HANDS NOW WORK BEHIND A FENCE.** *"No weapons, no beef"* held only because the pen ran
  > the fight: `attack 1` cleared no pennable `defense` but the three `defense 0` rows. With no fight
  > to gate them, an unarmed keeper slaughters an aurochs. What a kit still decides at a pen is the
  > **carry** — `herd_default_hunt_kit` already special-cases a corralled herd onto
  > `kit_supplying(HuntCarry)` rather than a mass-matched weapon, so the pen was always selected for
  > hauling; the weapon's `attack` is simply inert there now.
  >
  > **What retired with the exemption**: `fauna::animals_handled` (the pen's own collection stage),
  > `fauna::NO_FIGHT_STAGE`, the `is_corralled()` forks in `hunt_forecast`,
  > `forecast_production_and_take_at`, `project_realized_hunt` and `project_arrivals_hunt`, and the
  > `husbandry` **kit** (see `equipment.md`). `NO_RETREAT_STAGE_STAY` survives for the plant web
  > alone. The room clamp the pen used to spend at its own call site is
  > `resolve_hunt_engagement`'s `reach.min(animals_affordable(ceiling))` now, shared with the range,
  > so restraint is still free at every rung and the whole-animal floor is still
  > `resolve_hunt_fight`'s.
  >
  > **Why the ladder could not simply exempt the quote instead.** The quote had always run three
  > stages; it was the payout that ran none. Matching them by *deleting* the quote's stages would
  > have made the ladder a **mode switch** — taming and penning would buy nothing at the kill, and a
  > fenced animal would be collected by a formula the range never uses.

- **EVERYTHING PENNING BUYS STEPS AT THE FENCE**, and that is the deliberate difference from the
  plant web. The pen's regrowth gain, its density multiplier on `K`, its escape fraction and its
  handling gain all read `is_corralled()` — a **stored fact set at completion** — so `animal:pen`
  keeps `partial_credit: on_completion`. **Half a fence is not half a pen**: the animals are still
  roaming and nothing about them has changed. A half-sown Field genuinely has half a crop in the
  ground, which is why `plant:field` is `continuous`; the two are different facts, not an
  inconsistency.
- **AND SO DOES THE BILL** (`docs/plan_standing_upkeep.md` §2.8). The retired `herd_keeping_meter`
  billed the **pen** rung's upkeep from the first fencing work banked, while every benefit above waited
  for the fence — a herd paying to keep a pen that did not exist. That is the asymmetry §2.8 forbids and
  it was in the shipped game.
  > **§4.11 CLOSED THE SECOND HALF OF THE SAME ASYMMETRY, on the rung below.** Fixing the pen left
  > `pastoral` still stepping: `owner` is set by the **first** `Tame` accrual, so a herd owed the whole
  > pastoral rate from turn one while `is_domesticated()` — a completion predicate — withheld every
  > payout until the last. `herd_upkeep_demand` now interpolates on the position like everything else,
  > so a herd a tenth of the way up owes a tenth. **The pen's step survives on its own merits**, through
  > `partial_credit: on_completion` rather than through a hand-written predicate: cost and benefit still
  > move together, and at the fence they both move at once.
- **The pastoral rung is worked, so it cannot be double-paid** (slice 3b). It *used* to pay its owner
  passively, and `advance_husbandry` had to **skip** that payment for any herd a labor assignment
  worked last turn (a `Herd::worked_this_turn` flag) — because without the skip a Red Deer under
  construction collected the then-live `Corral` dip (0.50 × 1.50 = 0.75) **plus** the passive rung
  (1.50) = 2.25/turn, *more* than the 1.50 of walking away, turning the pen's investment cost into a
  profit. Retiring the passive rung removes the hazard by construction (there is no second payment
  left to stack), so the flag and the skip are **deleted**. **The dip itself has since retired**
  (`docs/plan_standing_upkeep.md` §2.2): a build's cost is now the crew the player staffs on it,
  measured against what those same hands would have hunted — which is the comparison
  `fauna_husbandry::building_a_corral_costs_more_than_hunting_the_same_herd` makes.
- **It is constant-*escapement* MSY** — `take = min(peak_regrowth(K), max(0, B − K/2))` — **not** the
  constant-catch `sustainable_yield` a *wild* `Sustain` hunt takes. The sim regrows in Logistics and
  harvests in Population, so a constant-catch take is evaluated at the **post**-regrowth biomass; above
  `K/2` that is harmless (both forms cap at MSY and converge on `K/2`), but **below `K/2` it takes
  `g(B + g(B)) > g(B)`** — strictly more than the herd grew. At the wild `r` = 0.05 that leak is a
  rounding error; at the pen's fast per-species `r` (up to 0.75) it is fatal: a **fully fed** pen knocked below `K/2` spirals
  to zero in ~12 turns and can never recover. Escapement never takes a herd below `K/2`, so a depleted
  managed herd **rebuilds** (yielding less, or nothing, while it does) and then pays `r·K/4` forever —
  stable from *both* sides, same yield at capacity and at the operating point.
- **A pen CAN overdraw**, and that is the point: `actual != sustainable` is reachable at rung 3 and
  the ⚠ fires on it. Its `workers_needed` is derived like every other rung's (slice 7) — the keeper
  still carries the meat home, so the take is `min(what the floor offers, hunters ×
  hunt.per_worker_biomass_capacity, what the crew can handle)` and the surplus beyond that is reported
  as `wasted`. The retired `TENDED_SOURCE_WORKERS_NEEDED = 1` claimed one keeper could collect a pen
  of any size. **The `policy` axis no longer collapses either** — that was the managed harvest's own
  claim, and it went with it.
- **What the fence still switches, and it is not payout**: where the animals live (`corralled_at`),
  the hay feeding and `pen_fed_fraction`, and the frozen capacity on a barren footprint. Those are
  facts about the pen, not rates, and they are untouched.

Ecology/husbandry tunables live in the `ecology` (`regrowth_rate`, `collapse_fraction`,
`collapse_rate`, `stressed_fraction`, `extinction_floor`), `immigration`, and `husbandry`
(**`pastoral.ecology`**, **`pen`** — see "Corral" — plus the per-species growth gains) blocks of
`fauna_config.json`.
**The pen's BUILD dials live in `intensification_ladder.json`** (the `animal:pen`
rung's `build` block), and as of slice 4 so do the **earned-knowledge dials** (the ladder-level
`knowledge` block — the old `knowledge_progress_per_turn`, since split into `learn_rate` plus a
per-knowledge `lesson_costs` entry, and `knowledge_completion_threshold`, which `labor_config`
duplicated verbatim) — see "The Intensification Ladder": both food webs climb on the same
numbers.
**`FaunaConfig` is validated** (`FaunaConfig::validate`, run inside `from_json_str`, so every load path
— builtin, default file, `FAUNA_CONFIG_PATH` override — is covered; the `expedition_config.rs` /
`crisis_config.rs` convention). A broken invariant is logged at **error** level
(`fauna_config.invalid_rejected`) and the known-good builtin is used instead. Enforced: **the ladder
is monotone as gains** (`pen_gain > pastoral_gain > 1`), ordered ecology phase
bands (`extinction_floor < collapse_fraction < stressed_fraction < 1`) in all three ecologies, every
`regrowth_rate > 0`, `husbandry_regrowth_cap > 0`, `0 ≤ pen.starve_shrink_rate ≤ 1`,
`hunt.provisions_per_biomass > 0`, and the follow/market bounds. (The **knowledge** bounds moved to
`LadderConfig::validate` with the dials in slice 4, where they hold for both webs at once.)

### THE KEEPER DEMAND IS AN UPKEEP RATE, and the shed is its shortfall penalty

**`herders_needed` stopped being a declared head count and became work per turn**
(`docs/plan_standing_upkeep.md` §2.4). Both managed rungs declare
`upkeep: { work_per_turn: 1.0, scaled_by: source_load, meter_decay: null }`, and the source supplies
the **keeper load**
— `head count / animals_per_herder` (`fauna::herd_keeper_loads`). Since one worker-turn is
`PER_WORKER_OUTPUT`, `upkeep_crew_needed = ceil(load)` **is** the
`ceil((biomass/body_mass)/animals_per_herder)` the retired helper computed, so every species asks for
exactly the keepers it always asked for (`every_species_asks_for_the_keepers_it_asked_for_before`).

- **The species owns the ratio, the rung owns the rate** — the same division `taming_cost_multiplier`
  and `work_cost` make of the taming job. `animals_per_herder` stays in `fauna_config.json` and is
  folded into the *measure* before the ladder ever sees it, which is why the scale primitive is
  `source_load` and emphatically **not** a per-head rate: per-head says *"one keeper per 100 fowl but
  one per 2 boar"* and invents a 45-herder steppe megaherd that is a pure artifact of the unit.
- **There is exactly one definition of it.** `fauna::raw_herders_needed` is the rung's own
  `upkeep_crew_needed` at this herd's load; `herd_herders_needed` prefers the hysteresis-stabilized
  `Herd::herders_needed` and falls back to it; `Herd::stabilize_herders_needed` is *handed* it rather
  than recomputing a `ceil` of its own. The free `fauna::herders_needed` is retired with the second
  copy it would otherwise have been.
- **`Herd::herded_fraction` is retired too**, and `Herd::upkeep_supplied` carries what it did — the
  one stored fact of the keeping. The published ratio (`fauna::herd_herded_fraction`), the shortfall
  and the animals nobody can hold are all derived from it, so no two of them can describe different
  staffings. *"Zero keepers last turn"* — the total-abandonment gate `regrow_biomass` and the bleed-out
  read — is simply `upkeep_supplied <= 0`.

> #### ⛔ THE REGROWTH SUPPRESSION CLOSES A LOOP, AND AN UNKEPT BUILD DOES NOT RECOVER FROM IT
>
> `regrow_biomass` zeroing growth at `upkeep_supplied <= 0` is what makes an unkept **build** on the
> animal web a **permanent** stall rather than a slow one, and the loop is worth stating whole because
> no single seam contains it:
>
> 1. the band's `husbandry` role is empty, so the herd's keeping is unmet;
> 2. `regrow_biomass` suppresses the flock's growth entirely;
> 3. the take crew beside the build draws the flock down to its assignment's escapement floor, and
>    with no growth it never comes back above it;
> 4. `systems::labor::crew_is_working_the_source` reads that room — `max(0, B − floor·K)` — as `0`, so
>    the `Tame`'s own `eligible` goes false;
> 5. `RungDef::build_supply` answers `None`, and — with the band's `builders` pool **staffed and
>    standing on this entry** — the countdown publishes `BUILD_QUEUE_BLOCKED` (**`-4`**), *"the queue
>    is stuck here"*, for as long as it lasts, while the meter sits frozen and the flock bleeds away
>    underneath it. **Every entry behind it in that band's queue publishes `-4` too**, because
>    nothing below a head that never finishes finishes either.
>
> **`-4` RATHER THAN `-1` IS THE WHOLE POINT OF THE SENTINEL** (`docs/plan_standing_upkeep.md`
> §4.6b). `-1` is the *absence* of an answer and renders as **no line at all** — which is exactly the
> silence this state must not be read as, on the one failure in the game whose remedy the player
> cannot guess. A pool with nobody on it still reads `-1`: the blocked reading is about a
> **committed** pool getting nowhere, and with no commitment there is nothing to report on.
>
> **THE REMEDY IS `assign_labor <faction> <band> husbandry <n>`, and nothing on the build reaches
> it.** Adding builders, re-ordering the queue and re-issuing the verb all leave the room at zero.
> Step 4 is why: it is an **eligibility** stall, not a balance one, so no term the countdown is
> struck from — `build_work`, the rot, the keeping share — can see it. **`meterRotPerTurn` is `0`
> here and honestly so**: neither animal rung declares a `meter_decay`, so nothing is eating the
> meter; what is being lost is the *herd*. A surface showing `-4` must therefore pair it with this
> herd's own `upkeepShortfall` / `neglectGraceRemaining`, which are where the sentence lives.
>
> **The plant web does not have this**, and the difference is not the predicate. A patch nobody
> gathers regrows toward `K`, so its escapement room is large, its gate stays open, and an abandoned
> half-built meter publishes an honest `-3`
> (`build_turns_on_the_wire.rs::an_abandoned_half_built_patch_publishes_rotting_and_a_kept_one_holding`).
> The plant web reaches `-4` only through a rung's *other* gates — an unlearned knowledge, a species
> nothing in the basket can climb — which `core_sim/tests/build_queue.rs`'s blocked-head arm stages
> and then **un**-stages, because a test that only ever sees the failure passes with the remedy
> broken.
> It is the **hunters' draw plus the suppressed regrowth together** that pins an animal source at its
> floor; neither alone would. **Escaping it is not symmetric** — lifting the suppression is enough on
> its own. Restoring the keeping restores `regrow_biomass`, and the regrowth outruns a
> floor-respecting take, so the flock climbs back above `floor · K`, the room returns and the gate
> opens **with the hunt row still at full strength**. Measured on
> `build_queue.rs::the_animal_webs_escapement_stall_publishes_minus_four_beside_its_shortfall`:
> 7–14 turns to a real countdown with the hunters left in place, indistinguishable from the same arm
> with the hunters taken off, which is why the surface can name the keeping as the whole remedy.

> #### THE HERD'S UPKEEP DEMAND *FALLS* AS AN UNKEPT FLOCK BLEEDS — a readout hazard, not a bug
>
> `animal:pastoral` and `animal:pen` both quote their rate per **keeper-load**
> (`scaled_by: source_load`, `head count / animals_per_herder`), so `upkeepDemand` is a reading of the
> flock's *current* size. A shedding herd therefore publishes a **shrinking** bill: measured on a
> half-tamed fixture with its `husbandry` role empty, `upkeepDemand` fell `6.08 → 5.43 → 4.13 → …
> → 1.22` over eight turns while the herd went `4837 → 974` biomass and the build sat frozen.
>
> **A player reading that number alone sees the bill improving while the investment dies.** It is
> correct — holding what is left really is cheaper — and it is a property of the scale primitive
> rather than of any one slice, so it is the *pairing* that has to carry the news:
> `upkeepShortfall` stays non-zero throughout and `neglectGraceRemaining` reads `0` from the first
> turn. A surface that renders the demand without one of those two is reporting a recovery.
>
> **The plant rungs cannot do this even though they are now scaled too.** Both declare
> `scaled_by: source_load`, but the plant load reads the **tile's** forage capacity — terrain, which
> no amount of neglect moves — where the herd load reads the flock's live head count. A patch's bill
> is constant for the ground it stands on; a herd's falls as the herd dies.

### The shed waits out a NEGLECT GRACE, and the notice does not

`Herd::neglect_turns` counts **consecutive** turns the keeping went unmet, reset outright by any turn
it was met — and by a herd not being managed at all, since a wild herd is nobody's to neglect.
**Animals leave only while that counter exceeds the herd's rung's `upkeep.grace_turns`**
(`RungDef::upkeep_grace_turns`), resolved through **`fauna::herd_keeping_rung`**: `animal:pen` once
there is any pen progress, `animal:pastoral` for any other managed herd.

> #### ⛔ THE GRACE AND THE PRESSURE ANSWER TO **ONE** PREDICATE, `fauna::herd_is_neglected`
>
> They did not, and the divergence was a herd-killer. `neglect_turns` rose only on
> `overage_last_turn.is_some()`, which `uncontained_overage` gates at **one whole animal**;
> `neglect_pressure` rose on any `shortfall_fraction > 0`, fell only on a turn the bill was met, and
> had **no ceiling** — while being the exponent in `rate × (1 + escape_acceleration)^pressure`.
>
> **The failure is silent for as long as you like and then total.** A 3-head herd kept at 90%
> staffing has an overage of `0.3`, under the whole-animal gate, so `neglect_turns` resets every turn
> and nothing ever sheds — while pressure climbs `+0.1` a turn for ever. Three hundred such turns put
> it at `30`; the turn the herd breeds past the gate, the first shed fires at a rate clamped to the
> whole herd. **A herd the player kept 90% staffed, and that was never once under-contained, is gone
> in one turn.**
>
> **The fix is the shared predicate, and deliberately NOT a cap.** Both terms now ask
> `herd_is_neglected`, so pressure can only rise on a turn the grace counter also rises and the same
> reset bounds both. A ceiling on top would be a second mechanism guarding a case the first one
> already closes — and it would leave the two definitions in place to drift again.
> `ninety_percent_keeping_never_frays_a_herd_below_the_whole_animal_gate` is the pin.

- **THE SHED IS CONTINUOUS IN THE SHORTFALL.** `uncontained_overage` is the unmet demand converted
  back into animals (`shortfall_in_loads × animals_per_herder`), which is the same number the retired
  `herded_fraction × herders_needed × animals_per_herder` capacity reconstruction produced — so half
  the keepers a herd wants leaves half its animals uncontained. The retired
  `herded_fraction < FULLY_HERDED` gate was a threshold that answered only *whether* a herd was
  under-contained, the same step the plant web's binary `tended_this_turn` flag took.
- **`MIN_ESCAPE_ANIMALS` is the animal branch's quantum, and it is why its counter can differ from the
  plant web's.** A plant meter is continuous, so any shortfall bleeds; a herd loses **whole animals**,
  so a shortfall of less than one animal is not under-containment at all — the same whole-animal
  discipline `quantise_animal_take` imposes on the take.
- **EVERY METER CARRYING WORK IS OWED THE BAND'S KEEPING POOL, AT ANY FULLNESS**
  (`fauna::herd_upkeep_supply`, the twin of `forage::patch_upkeep_supply`;
  `docs/plan_standing_upkeep.md` §4.6a). A `Tame` in flight owes exactly what a tamed herd owes, to
  the same hands.
  - **THE RATE AND THE PAYER ARE THE SAME EITHER WAY, and the two webs answer identically.** The
    meter's **fullness** used to move the supplier (`fauna::herd_is_maintaining`, deleted): a
    half-tamed herd was billed to its build crew, so taking the taming crew off it left keepers idle
    in the `husbandry` role with nothing they could be aimed at. Two earlier cuts are both gone — that
    one, and *"the animals are standing there whether or not the fence is up"* before it.
  - **SO THE RATE DOES NOT TAX AN ANIMAL BUILD**: a `Tame` or `Corral` banks its crew's whole output,
    and what a short keeping costs is the **shed**, in proportion, exactly as it costs an abandoned
    rung. Because the rate scales with the flock, a big herd is dearer to *hold* in hands — but it is
    no dearer to *build*. Pinned by
    `fauna_husbandry::a_half_tamed_herd_sheds_only_when_its_keeping_is_short`.
  - **AND NOTHING EATS AN ANIMAL BUILD'S METER.** Neither animal rung declares a `meter_decay`, so
    `fauna::herd_meter_rot` is always `0` and a staffed `Tame` or `Corral` always publishes a real
    finish date however short its keeping is. That is the model — the shortfall is paid in animals —
    not a missing red on the wire.
  **The verb names the meter**, so a `Corral` starting on a herd with no pen progress answers for
  `animal:pen` from its first turn: the supply is stamped in Population and read by the *next*
  Logistics pass, so it has to describe the meter that pass will judge.
- **The supply accumulates across the bands working the herd** (`+=`, cleared once per turn by
  `advance_husbandry` after everything downstream has read it). The demand is per-**source**, so two
  bands each put a fraction of it on the ground; assigning would let whichever band the loop visited
  last speak for all of them.
- **It is `herd_keeping_rung`, not `herd_rung`.** The latter answers which rung the herd has
  *completed*, and a half-tamed herd is already owned and already sheds — reading `animal:wild` there
  (no build, so no grace) would hand the herd in the middle of a 25-turn investment the *least*
  forgiveness on the ladder. It reads the **newest meter with progress**, so a half-raised pen already
  owes the pen rung's longer grace; the escape *rate* still reads `is_corralled()`, because a fence
  that is not up yet holds nothing.
- **The under-herded notice is deliberately NOT gated on the grace.** It fires the turn the herd
  genuinely becomes under-contained, which is exactly the window in which the player can still send
  hands and lose nothing; warning only once the animals were leaving would spend the grace on silence.
  This is why the measurement was split out of the shed: **`fauna::uncontained_overage`** answers *is
  this herd under-contained*, `shed_uncontained_animals` answers *do animals leave this turn*.
- **On the wire:** `HerdTelemetryState.hasNeglectGrace` / `neglectGraceRemaining` — the countdown, not
  the counter (`0` = shedding now), published through the same `herd_keeping_rung` seam the shed gates
  on, so the wire cannot count down against a rung the sim is not applying. See the plant twin in
  `cultivation.md`.
- **`decay_fraction_per_turn` IS GONE FROM THE LADDER ENTIRELY**, and the animal branch is why it was
  ever suspect: the neglect-escape arc made `domestication_progress` monotone-up, so the `0.01` on
  `animal:pastoral` (and the `0.0` on `animal:pen`) described a tameness-bleed the sim does not have
  and **nothing read**. It was deleted from the animal rungs first and retired outright when the plant
  branch moved onto the standing upkeep, where **shortfall is the decay**
  (`docs/plan_standing_upkeep.md` §2.4): a rung that loses exactly the work nobody supplied needs no
  second dial saying how fast it forgets. **`build.grace_turns` went the same way** — both animal
  rungs declare `null` and their live grace is `upkeep.grace_turns`, because two numbers for one
  trigger is what that arrangement exists to prevent.

### THE PEN COSTS HURDLES TOO — the material half of the same standing bill

**Work was never the whole price of holding a fence** (`docs/plan_standing_upkeep.md` §2.7 / §4.9
item 12). `animal:pen` is the shipped ladder's **one** declarer of a material on either term:
**6 hurdles** on its build pile and **0.05 per keeper-load per turn** on its upkeep rate. The engine
half — how a pile is drawn, how a short store stalls a build, and why the decay takes the *worst* of
the two shortfall fractions — is `intensification.md` → "THE MATERIAL HALF"; what is animal-specific
is here.

- **THE RATE READS THE HERD, exactly as the work rate does** — `scaled_by: source_load`, so a pen
  holding twice the herd mends twice the fence. `fauna::herd_upkeep_material_demand` interpolates it
  on the position through the same `interpolate` the work demand goes through.
- **AND IT STEPS AT THE FENCE, for free.** `animal:pen` is `partial_credit: on_completion`, so a herd
  raising a fence interpolates between wild and *pastoral* — which names no material — and owes its
  first hurdle only when the fence closes. **Half a fence is no fence**, and nothing here tests for
  the pen rung to make that true.
- **AND AN EXTENSION RING EATS THE SAME PILE, off the same rung record.** Widening a pen *is* a pen
  build — the same `build_cost`, the same builders pool, the same keepers' tools — so it is the same
  6 hurdles. It bids in the **same** `settle_material_upkeep` pass at its own row's `SourcePriority`,
  through `systems::labor::head_ring_leg`: one leg `(AnimalPen, ring_cost − pen_extend_progress,
  ring_cost)` handed to the **same** `build_material_wants` every rung leg goes through, so *"a pile
  draws in proportion to the work banked"* has one expression and a ring and a fresh pen draw alike.
  A short store stalls the ring in proportion and a dry one publishes `BuildGate::Materials`.
  ⛔ **`source_banking_its_first_work` still filters `ExtendPen` out, and that is only about the
  VERB** — a ring fills no rung meter, so it names none; the pile is laid off the ring's own
  `pen_extending` gate instead. Deciding both questions there is what made widening a pen materially
  free while raising one cost six panels.
- **AND THE WIRE STATES THAT PILE ON THE ONE ROW A RING IS OFFERED FROM** —
  `HerdTelemetryState.corralBuildMaterialCost`, the material twin of `corralWorkCost` and the build
  twin of `corralUpkeepMaterialDemand`. It exists because `buildMaterialCost` beside it prices the
  rung **directly above** where a herd stands, and a ring is only ever offered on a herd already
  standing on `animal:pen` — the **top of its branch**, where that field is deliberately empty
  (*"the honest reading rather than a repeat of the pen's own"*). So a ring price card could state
  the ring's 75 work and its standing bill and **not the six hurdles it swallows**, which is the
  number a player short of panels is deciding on. Published off the ladder rung alone, **unscaled
  and with no herd term**, because `head_ring_leg` prices a ring's width at
  `build_cost(RUNG_COST_UNSCALED)` — a scaled quote would show one price and charge another. On a
  **pastoral** herd it equals `buildMaterialCost` by construction (one
  `rung_material_pile(ladder, AnimalPen)` reached through two selectors); on a **corralled** one it
  is the only reading of the pile there is. `core_sim/tests/rung_material_quote.rs` pins both, and
  pins the published pile against what a **completed ring actually takes off the band's shelf**.
  ⛔ **THERE IS NO `tameBuildMaterialCost`**: the tame rung has no repeatable increment, and
  `buildMaterialCost` already carries the tame pile on the only row that can climb to it — a wild
  herd. The asymmetry against the `tameUpkeepMaterialDemand`/`corralUpkeepMaterialDemand` pair is
  that that pair prices two rungs a source can **stand on**, where this prices the one job a source
  can **repeat**.
- **THE SHORTFALL SHEDS ANIMALS, and no new penalty was added.** `uncontained_overage` reads
  `intensification::keeping_shortfall_fraction` now — the worst of the work fraction and each good's
  — so a pen fully staffed with no hurdles to mend the fence sheds at the hurdles' rate, one with
  hurdles and no hands at the hands' rate, and one short of both at the worse. The **same**
  `neglect_turns` and the **same** `upkeep.grace_turns` govern both kinds.
- **`Herd::upkeep_materials_demanded` is the STAMPED bill**, and it is written in the *same pre-loop
  pass* as `upkeep_demanded` rather than in the arm. It has to be: the arm is skipped for a herd out
  of the hunt leash or gone from the registry, and a work stamp without its material twin would read
  as *"a band answered and this rung eats nothing"* — an abandoned pen judged short of hands and
  fully supplied with hurdles.

> ⛔ **IT IS NOT FEED, AND THE TWO ACCOUNTS NEVER MEET.** Hay is what the animals **eat**, settled by
> `settle_pen_hay` in fodder units against the herd's appetite; this is what the **structure** costs to
> stand, and an empty pen would owe it just the same. The material settlement draws the band's
> **material batches** and touches neither the `FODDER` store nor the `FOOD` larder — which is what
> keeps §2.7's retired human-food path retired.

### THE PEN'S FEED IS ITS OWN MECHANISM, and it is not the upkeep

The `upkeep` block is the **work** half of holding a rung — hands, in work units. What a pen *eats* is
a separate account in a separate currency: **fodder**. Its demand is the herd's own
`fodder_per_biomass × biomass`, met by the fenced footprint's grass (`penPastureFraction`) and then by
hay off the band's `FODDER` store (`fodderDraw`); `pen_fed_fraction` records how much of the demand the
two covered between them, and `husbandry.pen.starve_shrink_rate` is what an underfed pen loses. **What
the footprint leaves for the keeper to grow is published as the band's `fodderNeed`** — the gap,
ungated by Foddering, with its income and its (gated) runway beside it — and what that gap still
leaves *after* the draw is `penFodderShortfall`, the per-pen figure the row actually asks the player
for; the reasoning for both is in `graze.md` → "The hay bill is published as the GAP". A
keeper who is present but has **no grass and no hay** starves the herd; a keeper who is **absent** lets
it shed. The two penalties are orthogonal and a pen can take both in one turn.

> #### ⛔ THE FEED IS NOT PRICED IN FOOD, AND `upkeep_per_biomass` IS RETIRED
>
> `husbandry.pen` used to carry `upkeep_per_biomass` — a **food**/turn rate per unit of standing
> biomass, drawn from the keeper band's `FOOD` larder for the share the footprint could not graze.
> **Human food is not animal feed.** It short-circuited the starvation path (a pen whose pasture failed
> ate its keepers' bread instead of shrinking) and it put the pen's one demand in two units at once,
> which is what let the mixing happen. The lever is gone from the struct **and** from the JSON, and
> `PenConfig` carries `#[serde(deny_unknown_fields)]` so an overriding file that still tunes it **fails
> to load** rather than being silently ignored — the authoring comments moved up to `husbandry` level
> (`_comment_pen`, `_comment_pen_starve_shrink_rate`) to make room for that.
>
> Folding the feed into the `upkeep` block would still be wrong, for the original reason: food and
> labour in one number. What changed is that the feed account is denominated in fodder, so it does not
> appear in the food ledger at all —
> `larder_delta == foodIncome − foodConsumption − raidForfeit`.

> ### ⛔ EVERY FOOD FIGURE BELOW WAS MEASURED WITH A **STOCKED** BAND, AND A SPAWN NO LONGER IS ONE
>
> `equipment.json`'s `start_stock_fraction` ships **`0.0`** — a spawning band owns no equipment at all
> (`equipment.md` → "A SPAWNING BAND OWNS NO EQUIPMENT"). Every table from here to the end of the
> space-budget section was read at the previous `1.5`, so each is *"what this source pays a band that
> has crafted its kit"* rather than *"what a new band collects"*. **They are still the right numbers
> for the thing they compare** — a source's ceiling and which term binds it are properties of the land
> and the roster — but three readings invert once the gear is gone and are recorded here rather than
> restated everywhere:
>
> - **The seven pens are BIT-IDENTICAL bare-handed.** A pen has no fight stage and `pen_is_a_larder`
>   means carry does not bind, so nothing a pen's take reads is a kit tier. Every pen figure below
>   holds unchanged.
> - **Every wild-hunt and pastoral row falls to `0.0000`** at every crew size: bare hands are
>   `attack 1` against a defended quarry, and only the `defense 0` species (hare, catfish, rabbit,
>   fowl, grouse) pay anything at all. The rung-pair comparisons below are therefore about a band that
>   has armed itself.
> - **The plant web's two drawdowns disappear.** The wild food-only `+25%` and tended `+17%` overdraws
>   named below both come from a *basketed* crew out-taking a narrowed stand; at the bare carry rate
>   every plant row sits well under its line with headroom. **Nothing in the model is a drawdown at a
>   bare spawn.**
>
> A Field's `12.48` is likewise the **stand's** capability and is unmoved — a bare crew simply cannot
> carry it, and stays carry-bound past the 13 gatherers the sweep reaches (`13 × 0.256 = 3.33`).

## ⛔ EVERY ANIMAL RUNG IS AT OR UNDER ITS SUSTAINABLE LINE — the numbers are steady rates

Measured across the whole roster at the 3- and 5-worker crews (`food_economy_table.rs` Section E),
with `pen_is_a_larder` on: **not one animal row is a drawdown.** Every wild, pastoral and penned take
sits at or below what its herd reproduces that turn. The pen figures are the tightest — a pen is
*designed* to sit on its production line — and they sit **on** it, not past it:

| species | rung | `r` per turn | `K` biomass | reproduces food/turn | taken at 5 keepers | verdict |
|---|---|---|---|---|---|---|
| Wild Aurochs | pen | `0.360` | 3968 | **7.14** | 7.0200 | at the line |
| Wild Sheep | pen | `0.800` | 1587 | **6.35** | 6.3252 | at the line |
| Wild Boar | pen | `0.400` | 3171 | **6.34** | 6.3240 | at the line |
| Crag Goats | pen | `0.880` | 1443 | **6.35** | 6.3120 | at the line |
| Wild Fowl | pen | `1.000` | 1015 | **5.07** | 5.0458 | at the line |
| Rabbit Warren | pen | `1.000` | 951 | **4.76** | 2.9916 | 37% under — reaches its line at 8 |
| Snow Hare Warren | pen | `1.000` | 951 | **4.76** | 4.7220 | at the line |

**The aurochs' slow reproduction does not cost it the top of the ladder**, because `K` and `r` trade
off: `r_pen 0.36` against a 3968 `K` reproduces 7.14 food/turn, while a fowl's `r_pen 1.0` against a
1015 `K` reproduces 5.07. **`sustainable ≈ r · K / 4`, and `K` is the bigger lever on this roster** —
which is why `pen_density` is where the pens were tuned and `pen_engage_gain` only decides the crew.

**The rabbit is the one row deliberately off the line at five keepers**, and it is a reach reading
rather than a production one: its `pen_engage_gain` is tuned to reach its ceiling at **8** where every
other pen reaches it at 5, so a five-keeper column catches it part-way up its own curve. At eight it
takes **4.7230** against the same 4.76 line — at the line, like the rest.

**Both plant-web drawdowns this measurement used to report are gone with the gear**, and the callout
above says why: they were a *basketed* crew out-taking a narrowed food-only stand. **Nothing on
either web is a drawdown at the shipped bare spawn.**

## A FIELD DOES RUN OUT — at ten gatherers on the reference tile

`food_economy_table.rs` Section D walks each plant rung up the crew curve. A Field is carry-bound to
**8 gatherers** and hits its own sustainable line at **10**:

| workers | field food/turn | binds | vs sustainable (12.48 food/turn) |
|---|---|---|---|
| 1 | 1.28 | carry | 90% under |
| 3 | 3.84 | carry | 69% under |
| 5 | 6.40 | carry | 49% under |
| 8 | 10.24 | carry | 18% under |
| **10** | **12.48** | **stand** | **at the line** |
| 13 | 12.48 | stand | at the line |
| 20 | 12.48 | stand | at the line |

**It flattens exactly as a herd does** — the 1.28-per-worker figure the earlier tables report is the
*carry-bound* regime, not a property of Fields. The wild and tended rungs flatten at **3** gatherers,
because their sustainable line is an order of magnitude lower (0.70 and 1.33 food/turn against the
Field's 12.48). The Field's headroom is what `field_capacity_gain` and `field_regrowth_gain` (2.53
each) buy: `K` 195 → 493 and `r` 0.25 → 0.632.

## `husbandry.hex_space_budget` — the space half of `K` (ships ON, `2530.3`)

`ecological_carrying_capacity` is `(graze_flow + fodder_delivery_rate) / fodder_per_biomass ×
density_gain` — **entirely feed**, with no physical-space term in it at all. Its own note calls a
barren footprint "carried entirely by delivered hay" an honest feedlot, which means an unlimited
number of animals fits on one tile provided enough hay is trucked in. Feed and space are different
questions and only one of them is soft:

```text
K = min(feed_K, space_K)
```

`fauna::herd_space_capacity` is the one expression; `null` answers `NO_SPACE_CAP_AT_ALL` and the
`min` becomes an identity.

### ⛔ ITS ONLY JOB IS TO STOP FODDER, AND IT MUST NOT TOUCH ORDINARY PLAY

`fodder_delivery_rate` enters the feed `K` with **nothing bounding it**, so before this term a barren
tile carried entirely by trucked-in hay held any number of animals. The cap exists for that and only
that. **A budget that changes no normally-grazed source is the dial working**, not the dial being
inert — the number it buys is the **headroom**: how much hay a pen may be fed before space stops it.

Measured at the shipped `2530.3`, over all twenty species at every rung, **nothing clips**:

| rung | tightest headroom (space `K` ÷ feed `K`) |
|---|---|
| fowl pen | **1.001×** |
| rabbit pen | 1.363× |
| boar pen | 1.448× |
| snow_hare pen | 1.778× |
| wild_sheep pen | 2.006× |
| aurochs pen | 2.743× |
| every wild and pastoral row | 2.46× (`river_fish`) to 257× (`wolf`) |

So a penned aurochs may be fed to **2.7× the herd its ground grazes** before space refuses more, and a
penned fowl essentially not at all — its coop is already full.

### Why 104 aurochs per hex and not 100

`104 × 120^(2/3) = 2530.3`. At a 100-aurochs budget **fowl in a pen is the single row anywhere on the
roster that clips**, and only because the `pen_density` retune took its `K` to 1280 against a 1233
allowance. `104` lifts that allowance to **1281.8 against 1280.0 — a 1.8 kg, 0.14% margin.** It is the
tightest number in the config and any further rise in `fowl`'s `pen_density` or `biomass[1]` will push
it through.

**Do not lower it** toward 40 or 30 aurochs per hex: measured, those bit into normal grazing and
clipped wild rows (`river_fish`, pastoral `wild_sheep`), which is the cap doing the wrong job.

**Two things deliberately do not reach the space term.** The **fodder flow** — hay offsets feed and
never makes the field bigger — and the **density gain** (`pen_density` / `pastoral_density`), because
domestication does not enlarge the tile either. Both stay on the feed side, above the `min`.
**Universal, not a pen rule**, and applied on both diet branches.

### ⛔ SPACE IS AN **AREA** BUDGET, NOT A MASS ONE

The first cut of this dial was flat kilograms per tile (`space_biomass_per_tile`), which says a hex
holds the same *mass* of fowl as of aurochs. A fowl is **1/900th** an aurochs by weight and nowhere
near 1/900th of it by floor space, so that reading is physically wrong — and it is why no flat value
could bind a pen without also clipping wild small game (at `850`, wild `river_fish` and pastoral
`wild_sheep` were both clipped, and two grazing fixtures failed).

An animal's footprint scales with the **2/3 power of its mass** — area goes as length², mass as
length³. So the budget buys `budget / body_mass^(2/3)` **animals** per hex, and the biomass that
represents is that count × each animal's mass. `herd_space_capacity` computes it in exactly those two
steps rather than as the algebraically equal `budget × body^(1/3)`, which reads as a bare cube root
that states nothing about why.

### The anchor: `2433` is one hex holding 100 aurochs

`100 × 120^(2/3) = 2433`. What that gives the seven pennable species, per hex:

| species | body mass | animals/hex | `space_K` (1 tile) | pen `feed_K` | `K` from |
|---|---|---|---|---|---|
| aurochs | 120 | **100.0** | 12001 | 6500 | feed |
| boar | 12 | **464.2** | 5570 | 4000 | feed |
| crag_goat | 6 | **736.8** | 4421 | 2000 | feed |
| wild_sheep | 5.6 | **771.5** | 4321 | 3500 | feed |
| snow_hare | 0.6 | **3420.1** | 2052 | 300 | feed |
| rabbit | 0.27 | **5824.2** | 1573 | 300 | feed |
| fowl | 0.13 | **9480.8** | 1233 | 240 | feed |

### Measured at three anchors

| aurochs/hex | `hex_space_budget` | penned species space-bound | top pen, 5 workers | spread, 5 workers | clipped outside a pen |
|---|---|---|---|---|---|
| 100 | `2433` | none | Aurochs 11.58 food/turn | 11.58 / 1.19 | none |
| 40 | `973.15` | aurochs, boar, crag_goat, wild_sheep | Aurochs 8.52 food/turn | 8.52 / 1.19 | wild `river_fish` `K` 900 → 852 |
| 30 | `729.86` | aurochs, boar, crag_goat, wild_sheep | Aurochs 6.30 food/turn | 6.30 / 1.19 | wild `river_fish` 900 → 639, pastoral `wild_sheep` 1400 → 1296 |

**The area model keeps the aurochs on top at every setting** — `40 → 8.52`, `30 → 6.30`, ordering
`aurochs > crag_goat > wild_sheep > boar > rabbit ≈ snow_hare > fowl` unchanged. That is the
difference from the retired flat dial, which inverted the aurochs to **last**: a flat mass budget
gave every species the same `K`, so output collapsed onto `r_pen` and the slowest breeder lost. Under
`body_mass^(2/3)` a big animal keeps a proportionally bigger `K`.

**The three small-game pens never move** at any setting — their feed `K` (240–300) is far below their
space `K` (370–821 even at 30 aurochs/hex), so they are feed-bound throughout.

**No wild or pastoral FOOD figure moves at any of the three settings.** Two `K` values are clipped at
the tighter anchors and both are `small`-class one-tile footprints; neither row's food changes,
because both are bound by the fight rather than by production.

### The cap moves no food figure at the shipped budget — by design

`min(feed_K, space_K)` is an identity on every row, so every food figure is byte-identical to the dial
at `null`, and the suite passes at the same 1934/5 either way. **That is the acceptance criterion, not
a shortfall**: the headroom table above is what the term buys.

## `pen_density` — retuned, because it was backwards from the fiction

`pen_density` sets a penned herd's `K`, and with `pen_is_a_larder` on the take is `r · K / 4`, so one
dial sets the whole rung:

```text
food/turn = r_pen × base_K × pen_density × 0.005
```

It shipped **cattle at 5.0 and fowl/rabbit at 1.5** — a hex packing more cattle than poultry, when
small stock packs far tighter than cattle. That is why the big animals clustered near a Field's
**12.48 food/turn** sustainable line while the small ones sat ten times below.

### The shipped values, set against that same 12.48 line

| species | `pen_density` | `r_pen` | base `K` | pen `K` | reproduces food/turn |
|---|---|---|---|---|---|
| aurochs | 5.0 → **3.0523** | 0.36 | 1300 | 3968 | **7.14** |
| crag_goat | 5.0 → **3.6073** | 0.88 | 400 | 1443 | **6.35** |
| wild_sheep | 5.0 → **2.2675** | 0.80 | 700 | 1587 | **6.35** |
| boar | 4.0 → **3.1713** | 0.40 | 1000 | 3171 | **6.34** |
| fowl | 1.5 → **6.3425** | 1.00 | 160 | 1015 | **5.07** |
| rabbit | 1.5 → **4.7569** | 1.00 | 200 | 951 | **4.76** |
| snow_hare | 1.5 → **4.7569** | 1.00 | 200 | 951 | **4.76** |

**The whole column was then scaled by one factor to put the roster's top at 7.0 food/turn** — rank
order and relative spacing preserved, so the shape above is the tuning and the scale is a level. The
aurochs measures **7.02**; exactly 7.000 is not reachable, because a take quantised to whole 120 kg
bodies lands on a lattice.

**`pastoral_density` is untouched**, so the pastoral rung does not move: `pen_density` is read only at
`RungKey::AnimalPen` (`rung_density_gain`). Verified — every wild and pastoral row is byte-identical
across the retune.

### What each pen is actually limited by, at five keepers

Nothing is a drawdown; every row takes its sustainable line or less.

| species | reproduces food/turn | takes at 5 keepers | limited by |
|---|---|---|---|
| Wild Aurochs | 7.14 | 7.0200 | herd production |
| Wild Sheep | 6.35 | 6.3252 | herd production |
| Wild Boar | 6.34 | 6.3240 | herd production |
| Crag Goats | 6.35 | 6.3120 | herd production |
| Wild Fowl | 5.07 | 5.0458 | herd production |
| Rabbit Warren | 4.76 | 2.9916 | the keepers' handling rate — it wants 8 |
| Snow Hare Warren | 4.76 | 4.7220 | herd production |

**Every row but the rabbit is herd-production-bound at five keepers**, because that is the crew each
of them is tuned to reach its ceiling at. The rabbit is tuned to reach its ceiling at **8**, so at 5
its keepers' handling rate is still the binding term — deliberately, and it is the only pen for which
adding a keeper still buys anything.

### `pen_engage_gain` is overridable too, and it sets HOW MANY KEEPERS a pen wants

The take at `W` keepers is bounded by `reach = W × engage_rate × pen_engage_gain` animals, so this
dial **does not move a pen's maximum** — that is `r × K / 4`, set by `pen_density`. It moves the
**crew size at which the maximum is reached**.

**Every penned species except rabbit is tuned to reach its line at exactly 5 keepers:**

| species | `pen_engage_gain` | reaches its max at |
|---|---|---|
| aurochs | **4.2** | 5 keepers |
| wild_sheep | **8.5** | 5 |
| crag_goat | **7.9** | 5 |
| snow_hare | **9.4** | 5 |
| boar | **20.0** (the global, stated) | 5 |
| fowl | **44.0** | 5 |
| rabbit | **12.5** | **8** |

> **⛔ EVERY ONE OF THESE MOVED WHEN `pen_density` WAS SCALED DOWN, AND THAT IS STRUCTURAL.**
> Lowering `K` lowers the room (`r · K / 4`), so the *same* reach clears it a keeper sooner: the
> previous set (aurochs `6.0`, wild_sheep `10.6`, crag_goat `9.9`, snow_hare `11.8`, boar `25.0`,
> fowl `55.0`) had been tuned against the pre-scale densities and slipped every pen to **4** keepers,
> and the rabbit — which carried no row at all — from 8 to 5. **A `pen_density` retune therefore
> always owes a `pen_engage_gain` retune**, and the ratio is the obvious one: each value above is its
> predecessor times the density scale, then measured. **The maxima did not move under either set** —
> this dial moves the crew and never the level, which is exactly why the drift was silent.

**Most of these are BELOW the global 20.0, and that is the fix.** At the global, one keeper worked
nearly the whole herd — an aurochs pen delivered **7.14 of its 8.19 line at a single keeper** — which
made a pen worth about **2.5× a Field per head** (3.00 food/worker against 1.25) — figures read at
the densities of the time, before the roster was scaled to a 7.0 top. Fowl is the
exception in the other direction: `10 × 20 = 200` birds per keeper is a cattle number wearing a
chicken's `engage_rate`, so its flock could not be worked out at all without a large override.

**Derived, then measured.** The bracket is *reach ≥ affordable at 5 and < affordable at 4*, i.e.
`G ∈ [affordable/(5·engage_rate), affordable/(4·engage_rate))`. But the 40-turn drive's effective room
**exceeds** the static `r · K / 4` — biomass accumulates on the wait turns — so every shipped value
was measured against the sim rather than read off the static bracket; the static figures put four of
the six one keeper low.

**The aurochs is the tight one.** Its bracket's lower edge sits *below* its own
`pastoral_engage_gain` of `4.0`, and the per-species ordering check (`pen > pastoral` — a fence
handles better than a halter) correctly **rejected** the first value tried. That check is not
decoration; it caught a config that would have made a fenced aurochs no easier to work than a
haltered one.

**The ladder ordering is now checked on each species' effective pair** (`pen_engage_gain >
pastoral_engage_gain`), not on the globals: either arm may be overridden, and comparing an override
against the global it does not belong beside would let a fence handle worse than a halter on exactly
the animal carrying a row.

## Deleting the `pen_is_a_larder` flag — the shape of it

Three branch sites, all reached through **one seam**:

| site | what it does |
|---|---|
| `fauna::herd_collection` | the only `if … pen_is_a_larder` — returns `NO_CARRY_BOUND` instead of `workers × per_worker` |
| `SourceYieldForecast::larder` | the field, set from `herd_collection(..).is_infinite()` in `hunt_forecast`, and `false` in `forage_forecast` |
| `forecast_production_and_take_at` | reads `forecast.larder` to hand the quantiser `NO_CARRY_BOUND` |

`herd_collection` **does collapse cleanly**: dropping the flag leaves
`if workers == 0 { 0 } else if herd.is_corralled() { NO_CARRY_BOUND } else { workers × per_worker }`,
which is still the one term the quantiser's pack seat, `project_realized_hunt`'s `min` and
`hunt_take_bound`'s `carryable` all read. The `larder` field cannot be dropped with it — the forecast
holds no herd, so it still needs the fact carried — but its producer becomes
`herd.is_corralled()` and its meaning stops depending on config.

**Cost: the config field, its default constant, its validation-free `#[serde(default)]`, one `&&`,
one JSON key and its `_comment_`.** No call-site signature changes. The five failing pen tests would
need rewriting either way.

## `husbandry.pen_is_a_larder` — a pen is NOT carry-bound (ships ON)

**Ships `true`.** The party's **carry** does not bound a take at `animal:pen`, and a pen produces no
carry waste:

> *"A penned animal is a larder on the hoof. You slaughter what you need this turn and the rest stays
> alive."*

A carry bound says *you had to take it all at once and haul it home*, and at a pen you never do — the
animals stand behind a fence built at most `pen_radius_max` tiles away, so what is not butchered this
turn is next turn's stock, still breeding.

**The pen rung only.** Wild and pastoral keep their carry bound untouched, and everything else at the
pen is untouched too: the keepers' handling bound (`pen_engage_gain`), the escapement clamp, the
whole-animal quantum and the slaughter all still apply.

**One infinity, one seam.** `fauna::herd_collection` is the single term every site that can bind on
carry reads — the quantiser's pack seat, `project_realized_hunt`'s `min`, and `hunt_take_bound`'s
`carryable` — so the bound and the waste go together instead of in three edits that could drift.
`carried = killed_biomass.min(collection)` becomes an identity, which is exactly *"the meat does not
rot, it is still standing in the pen"*. `SourceYieldForecast::larder` carries the same fact into the
preview. A crew of **nobody** collects nothing, larder or not — `0 × ∞` is `NaN`, which is why the
seam is a function rather than a multiplication at each site.

**It is a package with `pen_density`.** While a pen was carry-bound it paid the sled's own ceiling
whatever its `K` said, so the per-species densities did nothing at rung 3 — the retune below and this
flag land together or neither means anything.

**The flag survives only so the pre-larder behaviour stays reachable for a bisect.** Three places
branch on it, all through one seam — see "Deleting the flag" below.

### Measured, at the 3-worker reference crew (at the pre-retune densities)

| species | pen, lever off | pen, lever on | what sets the level once carry is gone |
|---|---|---|---|
| Wild Aurochs | `0.8000` | **`3.8600`** | herd production — its own escapement MSY |
| Wild Sheep | `0.8000` | **`3.0987`** | the keepers' handling rate |
| Crag Goats | `0.8000` | **`2.9170`** | herd production |
| Wild Boar | `0.8000` | **`1.5200`** | the keepers' handling rate |
| Rabbit Warren | `0.4963` | `0.4963` | herd production (was never carry-bound) |
| Snow Hare Warren | `0.4962` | `0.4962` | herd production (was never carry-bound) |
| Wild Fowl | `0.3977` | `0.3977` | herd production (was never carry-bound) |

**The rung un-flattens.** Four species paid an *identical* `0.8000` with the lever off because they
were all pinned to the same sled; the spread across the seven goes from `2.01×` to `9.71×`, and every
figure is then a property of the animal — its `pen_density × biomass` band and `pen_gain × r` on one
side, its `engage_rate × pen_engage_gain` and `body_mass` on the other. **Carry waste at the pen goes
to zero** on all seven, as the model says it must.

**The bound enum reports `Engagement` on all seven either way, and cannot tell the two apart.** The
reach is clamped by the escapement room *before* the retreat (`animals_affordable`), so a
production-limited pen and a handling-limited pen both end with `brought_down == floor(stayed)`.
`HuntTakeBound` has no *"the herd could not spare more"* variant short of `Floor`, which fires only
when nothing whole could be taken at all. Which of the two is really binding is read by comparing the
measured take against `r_pen · K / 4` and against `floor(reach × stay_fraction) × body_mass`.

## ⛔ A SPECIES MAY OVERRIDE THE PASTORAL RUNG GAINS — absent means "use the global"

`SpeciesDef::pastoral_engage_gain` / `SpeciesDef::pastoral_resistance` are `Option<f32>` shadowing
`husbandry.pastoral_engage_gain` / `husbandry.pastoral_resistance`, resolved through
`FaunaConfig::pastoral_engage_gain_for` / `pastoral_resistance_for` — the `taming_cost_multiplier_for`
path, so a retune reaches herds already on the map. `fauna::herd_engage_rate` and
`fauna::herd_resistance` are the only readers, so an override cannot be honoured on one path and
missed on another.

### Why a rung gain needed a per-species arm

Every husbandry gain was **one global number applied to every animal**, while each species carries its
own `engage_rate`, `durability`, `wariness` and `body_mass` — so there was no way to say *"a rabbit
gets more out of being herded"* without also changing wild rabbits.

**The pastoral take runs into two ceilings in series, and which one binds is a property of body
mass.** The heavy-bodied half is **reach**-bound, so only `pastoral_engage_gain` moves it; the
small-bodied half is **fight**-bound, so only `pastoral_resistance` moves it — by exactly
`1 / resistance`, because the party's kill throughput sits at almost the height their reach ceiling
did. Measured, no global pair lifts both halves without inflating the first:

| global reach × | global resistance | best pastoral row | did the small game move? |
|---|---|---|---|
| `1.0` | `1.0` | `0.3033` | — (baseline) |
| `1.01` | `0.5` | `0.3710` | yes, ×2.0 |
| `4.0` | `1.0` | `0.4067` | **no, ×1.00** |
| `2.0` | `0.75` | `0.5600` | yes, ×1.33 |
| `4.0` | `0.5` | `0.8000` | yes — and **pinned to the pen's own carry ceiling** |

(against a tended patch's `0.4426` and a pen's `0.8000`, at the 3-worker reference crew).

### The shipped allocation

Globals stay at `engage 2.0` / `resistance 1.0` (the identity), and six species carry a row:

| species | override | why |
|---|---|---|
| `rabbit`, `fowl`, `snow_hare` | resistance `0.5` | fight-bound small game; reach is **inert** on them, and `0.5` doubles them exactly |
| `marsh_grazer`, `steppe_runner` | resistance `0.75` | fight-bound at `durability 60`, the toughest nomads — **and pastoral is their top rung** (`husbandry_ceiling: pastoral`), so it has to pay |
| `aurochs` | engage `4.0` **and** resistance `0.75` | the only species that needed **both** arms: `engage_rate 0.17` is the lowest of any tameable so `×2` barely clears one animal, and `durability 150` / `defense 6` is the toughest body on the roster, so unlocking reach alone just handed it to the fight |

**Everything else uses the global**, which is what keeps the roster readable: only exceptions carry a
row. It is deliberately **explicit per animal** rather than a body-mass formula a reader would have to
reverse-engineer.

**Per-species values validate exactly as the globals do** — engage gain finite, `> 1.0` and
`< pen_engage_gain`; resistance finite and in `(0, 1]`. An override that broke the ladder's own
ordering would read as a tuning and behave as a defect.

**Only the two PASTORAL dials are overridable today.** `pen_engage_gain` and the rest stay global
because nothing has needed them per-species; adding one is two fields and a resolver on the same
pattern.

## The `Tame` verb (Intensification rung 2) — the grammar fix

**The animal twin of `Cultivate`**, and the correction the plant side already made
(`docs/plan_intensification_ladder.md` §4.1). Taming used to be a **hidden side effect of a harvest
policy**: one `Sustain` branch in `advance_labor_allocation` advanced Herding knowledge *and*
`accrue_domestication`, so the same action both taught and tamed, invisibly and for free — while the
*visible* verb (`Corral`) was disabled until the herd was already tame. Rung 2 is now an explicit,
gated, **paid** verb, so both food webs read the same:

| | plants | animals |
|---|---|---|
| rung 1 Sustain | earns **Cultivation** — *and tames nothing* | earns **Herding** — *and tames nothing* |
| rung 2 verb | `Cultivate` → tended patch | **`Tame`** → pastoral herd |
| rung 3 verb | *(`Sow` — a later slice)* | `Corral` → pen |

- **`Improvement::Tame`** (wire key `"tame"`) — **animal-only** (`Improvement::valid_for_hunt`; a
  Forage assignment carrying it is rejected at its command, exactly as `Corral` is). Since issue #442
  it is an `Improvement`, not a pressure setting: it rides `LaborAssignment.improvement` **beside**
  whatever floor the crew holds, so a herd being tamed is still being hunted at that floor. It emits no
  `huntTripEstimates` row, because an expedition's mission carries a **floor** and therefore
  cannot name it at all.
- **The investment is the band's builders pool.** `tame` names no crew — it appends a queue entry
  (`docs/plan_standing_upkeep.md` §2.5) — and the hands that raise it, when this herd reaches the
  head of that queue, are hands that are not hunting. That, and nothing else, is what a Tame costs. The
  rung's `yield_fraction_while_building` is **retired**: the hunters beside the build carry exactly
  what they carried before, so the price is the same statement at every staffing, where the dip's
  depended on whether the herd's own escapement was binding the crew.
  `domestication_progress` accrues that crew's output in **work units** —
  the band's **builders pool** × `PER_WORKER_OUTPUT`, and only while this herd is the **head** of
  that band's build queue, **no floor term** (a build crew is not pulling on the
  herd) — against a job costing
  `work_cost × the species' taming_cost_multiplier` (**50 units for a rabbit, 250 for a Steppe
  Runner**), via the shared `RungDef::build_accrual` / `build_cost` seam. **The keepers' KIT is not
  in that expression**: the handling gear takes work off the **job**, never off the crew's output —
  see "The build axis" in `equipment.md`. **Turns are the output**,
  so a bigger keeper crew gentles the same herd sooner; see "An improvement costs WORK, not turns" in
  `intensification.md`. **Gates:** the faction knows **Herding**, the species'
  `husbandry_ceiling` allows domestication (Grazing 2d-δ), and **something stands above the crew's
  floor** (`systems::labor::crew_is_working_the_source` — the escapement room in biomass, read before
  the whole-animal quantiser, so a herd that cannot yet spare a whole body is still being worked).
  **THE `Thriving` GATE IS GONE** (`docs/plan_harvest_floor.md` §3.2 — the same deletion the plant
  rung 2 took, for the same reason): gentling a herd you are pulling hard on is now *slow*, not
  *stopped*, so there is no lapse state left and no "progress is held across it" rule to state.
  `validate_tame` never had a phase gate, so the command side was already consistent with removing it.
  The herd is still marked `tamed_this_turn`. Ownership is **not** in the gate: `accrue_domestication` owns the
  `owner is None || owner == faction` rule, exactly as `accrue_cultivation` does on the plant side.
  Accrued **after** the take (mirroring Cultivate/Corral), so the turn pays what the forecast promised.
  **The turn the herd becomes domesticated, the assignment's `improvement` is cleared** — the herd,
  the hunting crew **and the floor** all intact, and the **build's** crew handed on — to the keeping
  if the finished rung declares an upkeep, otherwise back to the idle pool, announced either way, so
  the player can re-task rather than leaving hands on a herd with nothing left to gentle. The
  completing turn still pays the build's whole price. One seam for all four rungs — see "Completion CLEARS the improvement" in
  `intensification.md`.
- **Tameness is PERMANENT once earned (neglect-escape arc, `docs/plan_fauna_neglect_escape.md` §2.1).**
  `domestication_progress` is monotone-up: `Tame` builds it and **nothing decays it** — the tameness-bleed
  (`decay_under_herded`/`decay_domestication`) is deleted. An abandoned part-tamed herd keeps its earned
  progress; what neglect costs is **animals** (the shed, see "Herding is standing labor"). `Herd::tamed_this_turn`
  is still cleared each turn so it can't go stale, but its consumer (the retired decay-sparing) is gone.
  `taming_cost_multiplier` now prices only the `Tame` *job*, never a decay. **Distinct from an ordinary
  hunt at any other floor**: a plain hunt *harvests* a herd; only `Tame` raises the taming meter.
- **`tame <faction_id> <herd_id>` command** (`handle_tame`; `TameCommand` proto field **40**, its
  `workers` field `reserved`, `CommandEventKind::Tame`) — **queues the `Tame`** on the bands already
  hunting the herd,
  the command form of the client's checkbox (issue #442: the floor beside it is left alone). It **tames nothing outright**. It targets a **herd id**
  (not a tile like `corral`): taming is the verb you reach for on a *roaming* herd, identified by who
  follows it, not by where it stands this turn. Rejections, each distinct (`validate_tame`, reached through `validate_improvement`): faction hasn't learned Herding / no such herd / the species is wild
  game (hunt-only) / already domesticated (corral it instead) / another people are taming it / **no
  band is hunting it** (staff it first). It never carried a phase gate — a herd's `ecology_phase`
  swings as it is hunted — and since `docs/plan_harvest_floor.md` §3.2 neither does the labor arm.
- **The `domesticate` early-claim is REMOVED** — command, `claim_threshold` lever, its validate bound,
  and `Herd::claim_domestication`. It let the player snap progress to `1.0` and skip the investment,
  which is the entire decision (the plant side removed its twin for that exact reason). **Proto field
  30 is reserved and must never be reused.** Tests that needed a tamed-herd fixture now run the real
  accrual through **`Herd::tame_outright(faction)`**, which obeys the husbandry ceiling — you cannot
  fabricate a domesticated `wild` herd. (It replaced `accrue_domestication(faction, RUNG_COMPLETE)`,
  which stopped meaning anything once a job had a size; it pays a nominal one-worker-turn job, and the
  *size* of a fabricated job cannot affect any predicate, which all read `progress >= cost`.)
- **Per-species price.** The rung's single cost would make *every* species the same job — a rabbit
  costing what a Steppe Runner costs. The species declares its own **`taming_cost_multiplier`**
  (`fauna_config.json`, default 1.0), and `RungDef::build_cost` takes it — the one seam that honors
  it, so every caller pricing a Tame reads the same number. **It prices the JOB and nothing else.** It
  used to reach the decay as well (a bleed was a fraction of the rung's own cost, which kept a
  build:decay ratio invariant per species for free — *slow to tame, slow to forget*); shortfall is
  the decay now, so what an improvement loses is what its keepers did not supply — a fact about a
  crew and a rung, not about how big the job was. Every other rung passes `RUNG_COST_UNSCALED`
  (penning is a flat job for every species — a fence is a fence; only *taming* varies).
  > **It was `taming_rate`, a build TIMESCALE, and the inversion is the honest statement**
  > (`docs/plan_unit_costed_work.md` §3.1). `0.2` on a Steppe Runner said *your people are five times
  > worse at their job on this animal*; `taming_cost_multiplier: 5.0` says *the animal is five times
  > the work*, which is what anyone would have meant — and it composes with a later cost spread,
  > where a rate could not. Same pacing at the same crew.
  See the `fauna_config.json` row for the roster.
- **Config** — the whole rung is `intensification_ladder.json`'s `animal:pastoral` record: verb `tame`,
  `unlock_knowledge: "herding"`, **`earns_knowledge: "penning"`** (slice 4 — a config edit, exactly as
  promised),
  `ceiling_required: "pastoral"`, `build: { work_cost 50, grace_turns null }`,
  `upkeep: { work_per_turn 1.0, scaled_by source_load, grace_turns 2 }`.
  The **50 is a reference-crew choice, not a derivation** — the rung declares no crew, so there was
  nothing to multiply today's 25 turns by, and 2 keepers is what rung 2 of the plant web wants for the
  same claim. The **upkeep is the pacing-neutral inversion of `herders_needed`**: at `1.0` work per
  keeper-load, `ceil(demand)` is the count this rung has always asked for. `grace_turns 2` is this
  rung's own former *build* grace moved onto the upkeep's trigger unchanged — a tamed herd with no
  fence stays near its people for a turn or two, then drifts — and `build.grace_turns` is `null`
  because there is only one trigger now;
  **`crew_needed` and `yield_fraction_while_building` are both retired** — the player states the
  build's crew on the verb, so neither a rung-level staffing floor nor a dip has anything left to say.
  **The Tame IS crew-scaled now**, at **the crew the player staffs** — it was crew-*blind* before,
  taking 25 turns whether two hands or twenty worked the herd, and a crew-blind build is exactly what
  pricing improvements in work removes (`docs/plan_unit_costed_work.md` §1.2).
- **Slice 3b landed the rest of the rung:** passive-free pastoral is **retired** (a tamed herd yields
  only through a worker's Hunt assignment, at the pastoral `r` — see "Domestication / husbandry" and
  "The husbandry yield ladder") and the **`drift_to_owner`** movement primitive is live (see "Herd
  movement is a rung primitive").
- **Slice 4 completed the rung's knowledge half:** practising it (hunting the resulting **pastoral**
  herd) earns **Penning**, which now gates `corral` — so Herding gates `tame` and **only** `tame`.
  See "The knowledge pattern".

See Also: "Cultivation (Intensification Phase 1a)" (the plant rung 2 this now mirrors exactly), "Corral
(Intensification Rung 1c)" (the rung above), "The Intensification Ladder" (the engine + the config).

## Corral (Intensification Rung 1c)

The **animal mirror of the tended patch** (`docs/plan_intensification.md` §4b) — the place-bound form
of the *existing* herd domestication, and the fauna-side twin of "Cultivation" under Depletable
Forage. Taming a herd is *symmetric* with preparing a patch, but the **product differs and that
difference is the settle mechanic**: an *un*corralled domesticated herd stays **mobile** (pastoralism
travels with the band); **corralling pins it**. Like Cultivate, corralling is an **explicit `Corral`
policy with an investment cost** — not a free command. A `Herd` carries `corral_progress: f32` (**work
units**, complete at its stored `corral_cost`; the pen under construction), `corralled_at: Option<UVec2>` (`Some` = penned at that tile) + a transient
`corralled_tended_this_turn` flag. *Sim-only — the client readout is a follow-up (see below).*

- **Rung-3 earned-knowledge gate — PENNING** (slice 4's §4.3 reshuffle; **it was Herding**). *Learned
  by doing* and **never start-granted**: hunting a **pastoral** (tamed) herd accrues faction
  **Penning** knowledge (discovery `PENNING_DISCOVERY_ID` = 2006, `fauna.rs`) at the ladder's
  `knowledge.learn_rate` **scaled by the assignment's floor, over Penning's own `lesson_cost`** —
  *"you learn penning by managing tamed herds"*, and how fast depends on how much you leave standing.
  **It does NOT depend on how many hands are on the herd**: a lesson is credited once per source per
  turn, in **practice units**, which is the currency the build's work units are deliberately kept
  apart from (`intensification.md` → "A LESSON COSTS PRACTICE — and practice is NOT work")
  (`intensification::learn_multiplier`; the health gate is gone, `docs/plan_harvest_floor.md` §3.2). The **`Corral` policy** (and the `corral` / `extend_pen` commands, which ride the same
  `animal:pen` rung) is refused until the faction knows it; every gate resolves the id off the rung
  record, never a literal. The `penning` tag → discovery 2006 mapping is declared in
  `start_profile_knowledge_tags.json` purely so it is mappable; **no start profile lists it**
  (guarded by `start_profile::tests::no_start_profile_grants_a_ladder_knowledge`).
  **The old Cultivation asymmetry is gone:** taming is no longer ungated (Herding gates `Tame`), so
  both webs now gate rung 2 on the knowledge rung 1 teaches, and rung 3 on the knowledge rung 2
  teaches. One knowledge per transition. See "The knowledge pattern".
- **The `Corral` improvement — the investment.** In `advance_labor_allocation`'s **Hunt** arm, a herd
  worked with `Improvement::Corral` (animal-only) in flight costs **the fencing crew the command
  named** — hands not hunting (`docs/plan_standing_upkeep.md` §2.2; the rung's
  `yield_fraction_while_building` is retired, and the keepers beside the build take what they always
  took). `corral_progress` accrues that crew's own output in work units against the rung's
  `work_cost` of **75** (25 turns at a reference crew of 3). **Its `eligible`
  deliberately carries no work predicate**, unlike the two rung-2 builds: it replaced a rung's
  `Thriving` gate, rung 3 never had one, and fencing a herd is ground work — a pen goes up around a
  flock already drawn down to its keeper's own floor. **Gates:** the faction knows **Herding**
  AND owns the **domesticated** herd; a gate that lapses **mid-build** just stops accrual that turn
  (progress is kept — a half-built pen is materials on the ground; unlike cultivation it does **not**
  decay *gradually*). That "progress is kept" applies to a **mid-build** lapse only — a **completed
  pen whose herd escapes loses its progress outright** (reset to `0.0`; see *Escapes-if-untended*
  below). Accrued **after** the take, so the turn pays exactly what the forecast promised. At the job's
  cost `Herd::corral_at` pens it (sets `corralled_at`, stops roaming, grants the one-turn tended grace),
  pushes a `CommandEventKind::Corral` feed line, and **clears the assignment's `improvement`** — the
  keeper crew stays on the herd, under the stance it chose, rather than fencing a pen that is already
  up. Extending a pen is command-driven (`herd.pen_extending`), not improvement-driven, so the clear
  cannot block a later ring. One seam for all five build kinds — see "THE QUEUE IS THE DECLARATION"
  in `intensification.md`.

> #### THE PEN RING'S LIFE, AND THE ONE STATE THAT COULD STRAND IT
>
> A ring (`extend_pen`, 2d-β) rides the `animal:pen` rung but has no rung of its own to complete, so
> it carries a **flag** where the four rung verbs carry a meter: `Herd::pen_extending` is *"a ring is
> in flight"*, `pen_extend_progress` is its meter, and `Herd::begin_pen_extension` **refuses while
> the flag is set**. Three consequences follow, and they are the whole of the ring's lifecycle:
>
> | moment | what happens |
> |---|---|
> | `extend_pen` | `begin_pen_extension` sets the flag and **resets `pen_extend_progress` to `RUNG_UNSTARTED`**, *then* `BuildJob::ExtendPen` is queued on every keeping band |
> | at the head | the band's whole `builders` pool raises it (`ring_workers`), against the pen rung's own `work_cost` at the pool's handling gear |
> | completion | `accrue_pen_extension` widens the pen, **resets the meter, and clears the flag** |
> | **the entry leaves the queue** | `Herd::cancel_pen_extension` clears the flag **and** the meter |
>
> **THE LAST ROW IS THE ONE THAT WAS MISSING, AND ITS ABSENCE WAS A PERMANENT DEAD END.** Because the
> flag is set *before* the entry exists and only completion cleared it, an entry dropped mid-ring left
> `pen_extending` set with nothing left to fund it — and every later `extend_pen` on that pen was
> refused, for ever. The build-queue block puts a `✕` on that entry, so it was one click away.
>
> **Three exits drop an entry, and all three go through `fauna::cancel_dropped_rings`:**
> `unqueue` (`fauna::unqueue_build_and_cancel_ring`, what `handle_unqueue` calls), `abandon`
> (`fauna::drop_holding_and_cancel_ring`, what `handle_abandon` calls), and the **lapse** — the
> per-turn `prune_build_queue` in `advance_labor_allocation`, which is the exit no command issues and
> the easiest of the three to miss. The two command seams live in `fauna.rs` rather than on
> `LaborAllocation` because the ring lives on the **herd**, and an allocation holds no registry.
>
> **THE BANKED RING PROGRESS IS DISCARDED, AND THAT IS THE HONEST STATE.** `unqueue`'s contract is
> that it leaves the source's meter alone, which argues for keeping `pen_extend_progress` — but
> `begin_pen_extension` **resets that meter on every start**, so a preserved ring meter could never be
> resumed by any path the game has. Keeping it would be storing a number nothing can read.
>
> **A ring another band's entry was funding stops for that band too**, and the dead entry left behind
> is retired the next turn by the already-built sweep (`!pen_extending` is a ring's *"already
> built"*) — see "A DEAD ENTRY PARKS THE POOL FOR EVER" in `intensification.md`.

> #### THE RING METER IS IN WORK UNITS, AND THE WIRE CARRIES ITS DENOMINATOR
>
> `pen_extend_progress` is an **absolute work count**, not a `0..1` fraction: it was normalized until
> `docs/plan_standing_upkeep.md` §4.8 priced improvements in work, and it completes at
> `pen_extend_cost` — the `animal:pen` rung's own `work_cost`, which `accrue_pen_extension` **stamps
> on the first worked turn** (`begin_pen_extension` leaves it at `RUNG_UNSTARTED`, and both reset
> together when the ring lands).
>
> **So the pair ships, never the meter alone.** `penExtendProgress` and `penExtendCost` cross to the
> client from the same herd in the same expression in `snapshot/subsistence.rs`, and a "Fencing N%"
> badge is their quotient with the zero denominator guarded — `0 / 0` is *"no ring"*, not *"0%"*.
> The one field on its own is an unscaled work count that a percentage readout renders as nonsense
> (a 69-unit ring read as *"Fencing 6900%"*), which is what a schema comment still describing a
> normalized fraction cost. It is the same meter/pile pair `tameWorkDone`/`tameWorkCost` and
> `corralWorkDone`/`corralWorkCost` already publish, with one difference: those costs are **resolved
> at capture** so a compose sheet can price an uncommitted build, while this one is the herd's
> **stamped** cost. Guarded by
> `build_queue::a_ring_publishes_the_cost_its_progress_completes_at`, which asserts on the encoded
> snapshot — an arm reading `Herd::pen_extend_cost` in process would pass even with the capture
> never writing the field.
- **`corral` command (repurposed)** — `corral <faction> <x> <y>` (`handle_corral`; `CorralCommand`
  proto field 38 with its `workers` field `reserved`, `CommandEventKind::Corral`) **queues the
  `Corral`** on the band(s) already hunting the herd standing on that tile — the command form of the
  client's checkbox. Since issue #442 it touches the improvement slot **only**: the band's
  stance and crew are untouched by construction. It **pens nothing outright**. Rejections: no herd there / faction
  hasn't learned **Penning** ("…have not learned Penning yet. Tame and keep herds to learn it.") / not
  domesticated / not the owner / already corralled / **no band is hunting it** (staff it first). Same
  gates as `validate_improvement`'s `Corral` arm, which every path shares.
- **The pen is a managed POPULATION** (`docs/plan_corral_managed_population.md`): its yield follows the
  animals you actually keep, those animals **eat** every turn, and underfeeding **shrinks** the herd. A
  one-off 25-turn build that then printed food forever is now a **sustained commitment with a running
  cost**. Corralled = fixed + place-local worker-tended + **fed** + escapes-if-untended:
  - *Fixed* — `advance_herds` skips a corralled herd's `advance_herd_roam` (it stays at `corralled_at`,
    no heading arrow); it still grazes its footprint + regrows toward the footprint's `K` (Grazing 2d).
    Since slice 3b this is **read off the `animal:pen` rung's `behavior.movement: fixed`**, not
    hard-coded on `is_corralled()` — see "Herd movement is a rung primitive".
  - *Place-local worker-tended* — a **Hunt assignment on a corralled herd** is herding/tending it, and
    the turn has two halves (the tend branch of `advance_labor_allocation`'s Hunt arm, which `continue`s
    before `hunt_take` — a corralled herd is never both hunt-drawn AND paid):
    1. **FEED (grass then hay, one unit — Grazing 2d §2.3, Flora Roster F3 §5.2).** The pen demands
       `demand_grass = fodder_per_biomass × biomass` of **fodder**. Its fenced footprint covers what it
       can (`advance_herd_grazing` → `footprint_intake`, `pasture_fraction =
       clamp(footprint_intake / demand_grass, 0, 1)`) and **hay** off the band's `FODDER` store covers
       the rest, priority-ranked across every pen the band keeps (`settle_pen_hay`). Then
       `pen_fed_fraction = clamp((footprint_intake + fodder_draw) / demand_grass, 0, 1)`. A lush
       footprint feeds the pen for free; a barren one lives on hay you had to farm — **the tether that
       gives "the pen pins the band" its teeth**, now cheap on good land.
       > **The keeper's larder is NOT a third source.** A pen used to draw `FOOD` for the share pasture
       > and hay left unpaid; human food is not animal feed, and what that draw really did was hide the
       > starvation path below. A shortfall is a shortfall.
    2. **HARVEST.** The keeper takes the **pen's MSY** (`fauna::pen_yield_biomass` →
       `managed_yield_biomass` under the herd's per-species pen ecology (`pen_ecology_for`), against its
       footprint `K` = `herd.carrying_capacity`), which **draws the herd
       down** — exactly what makes it sustainable (see "The husbandry yield ladder"). The credited yield
       is **gross**, and now also net: the feed is fodder, so there is no provisions debit standing
       against it.
  - *Starves if underfed* — `advance_husbandry` reads last turn's `pen_fed_fraction` and, if the keeper
    could not pay, shrinks the herd by `pen.starve_shrink_rate × (1 − fed) × biomass`, floored at
    `pen.ecology.extinction_floor × K_pen`. **The pen's growth is what the feed buys**: `regrow_biomass`
    scales a penned herd's growth by `pen_fed_fraction`, so an unfed pen does **not** grow (without this
    the pen's own fast `r` out-runs the 10%/turn wasting several times over — an "unfed" herd would keep
    growing and quietly pay a yield for feed nobody bought). The herd **withers to a remnant and
    recovers when fed again**: it does **not** despawn (a penned herd is exempt from `advance_herds`'
    dispersal retention — dispersal is the *mechanism* of local extinction, and a confined herd cannot
    disperse) and it does **not** lose the pen. Deliberate: a recoverable famine the player can see and
    fix is better play than silently voiding a 25-turn investment. It is **never silent** — an
    edge-gated `CommandEventKind::Corral` feed line fires on the turn the famine *starts*
    (`"The <species> herd is starving — the pen has no feed"`, detail `status=starving fed=<f>
    action=corral herd=<id>`), not every turn it continues. **Starving your animals to feed your people
    becomes a *decision*, not an accident.**
  - *The decision this creates* — the pen stops being a strictly-dominant upgrade and becomes a **wager
    on staying**: it out-pays every other rung, but only while you feed it, every turn, forever — and
    its food cost lands **exactly when food is scarce**, so a bad winter forces a real choice (eat the
    seed corn and lose future yield, or go hungry).
  - *Sheds-if-under-contained, AFTER A GRACE (neglect-escape arc, `docs/plan_fauna_neglect_escape.md`)*
    — the binary escape is **retired**. In `advance_husbandry` (Logistics, before Population — on
    `Herd::corralled_tended_this_turn`, which gates the pen's **feed** and survives both webs' move
    onto the upkeep because feed is a separate account) an under-contained pen **sheds whole
    animals over its labor capacity** into the wild web at `pen_escape_fraction` (slower than pastoral
    — the fence buys time, and the pen rung's longer `upkeep.grace_turns` says the same thing on the
    turns axis),
    and an untended one is BOTH un-herded (sheds) and un-fed (`pen_fed_fraction = NOT_FED`, so it does
    not regrow — a fast breeder's growth would otherwise cancel the shed). A **fully-abandoned pen bleeds
    its whole flock out and DESPAWNS**: it keeps shedding until it can no longer shed a whole animal
    (`biomass < body_mass`), then the emptied entity is removed (`advance_husbandry` Phase 3) — the pen,
    fence and all, dies with it (no field reset needed — the entity is gone). `owner`/`corralled_at`
    are **never cleared at a floor** (that would stop the shed and strand a husk); the herd stays
    corralled and bleeds down. The flock is already in the wild web via the shed (at domestication 0),
    so nothing is lost but the empty pen — and losing it is what makes the tending obligation real (the
    "pins the band" mechanic), re-penning being a fresh herd's full 25-turn investment. Because loss
    **destroys a 25-turn investment**, it is **never silent**: `announce_pen_lost` pushes the same
    `CommandEventKind::Corral` feed line the pen's *completion* pushes (one kind for the pen's whole
    life), reading `"The <species> herd has drifted off — untended, the pen is lost"` with
    `status=escaped reason=untended action=corral herd=<id> x=<x> y=<y>` in the detail. `corral_at`
    grants a one-turn grace so a freshly-penned herd doesn't shed before its keeper takes up tending. A
    keeper who is present but *broke* **starves** the herd (above) — it produces no shed (a keeper holds
    the flock — its keeping is met), so it keeps its pen and recovers when fed; only animals *leaving*
    empty a herd.
- **Persistence** — the checkpoint clones the whole `HerdRegistry` (`SimState::herds`), so **every**
  `Herd` field rewinds with a rollback, `corralled_at` / `corral_progress` / `pen_radius` /
  `pen_extend_progress` / `pen_extending` (Grazing 2d) included: a half-built pen (or an in-flight fence
  extension) is restored rather than lost. The transient per-turn scratch —
  `corralled_tended_this_turn`, **`pen_fed_fraction`, `pen_starving`, `footprint_intake` and
  `pen_pasture_fraction`** — is transient *within a turn*, not across a checkpoint: it is captured and
  restored verbatim like everything else, so a restored pen resumes exactly as tended and as fed as it
  was, and a rollback can neither invent a famine nor destroy a standing pen. These fields are still
  **off the client wire**; "not persisted" is no longer true of any of them.
- **Config** (`fauna_config.json` `husbandry`): the **`pen`** block — `ecology` carries **phase bands
  only** now (its `regrowth_rate` is unused; the pen `r` is per-species — Grazing 2d) and
  `starve_shrink_rate` (**0.10** — a fully-unfed herd loses 10%/turn). `capacity_fraction` is
  **deleted** (`K_pen` is the fenced footprint's graze flow) and so is **`upkeep_per_biomass`** — the
  block is `deny_unknown_fields`, so a file that still carries it fails the load (see "THE PEN'S FEED
  IS ITS OWN MECHANISM"). Plus the **per-species growth gains** `pastoral_gain` (2.0) / `pen_gain`
  (4.0) / `husbandry_regrowth_cap` (1.0), **`pen_radius_max`** (2 — the `ExtendPen` fence cap, 2d-β,
  validated `>= 1`), the **`pastoral`** block (phase bands only). **The pen's
  investment cost and build rate moved to `intensification_ladder.json`'s `animal:pen` rung** — the old
  `corralling_yield_fraction` 0.50 became its `yield_fraction_while_building`, since **retired**
  outright with the dip; the old
  `corral_build_progress_per_turn` 0.04 its rate, since re-expressed as **`work_cost` 75** work units
  — 25 turns at a reference crew of 3, the pacing-neutral cost — and in **slice 4**
  the earned-knowledge levers `knowledge_progress_per_turn` (0.05) / `knowledge_completion_threshold`
  (1.0) moved to that file's ladder-level **`knowledge`** block at the same values, `labor_config`
  having duplicated them verbatim; the rate has since split into `learn_rate` 1.0 over a per-knowledge
  `lesson_costs` entry of 20, which is the same 0.05 (see "The Intensification Ladder").
  `claim_threshold` is **deleted** with the `domesticate` early-claim it gated (slice 3a — it let the
  player skip the investment). The retired flat rates
  `provisions_per_biomass` (0.01) / `corral_provisions_per_biomass` (0.012) and `fauna::corral_provisions`
  are **deleted**.
  - **Retuned once, against measurement** (a scripted 100-turn campaign on three pinned seeds — the
    default `map_seed` is `0`/entropy, so a probe *must* pin one): the first cut (`pastoral` 0.15,
    `pen` 0.60, dip 0.25) left a freshly-taming band at income **1.275** vs consumption **1.294** — a
    permanent one-day-of-food treadmill, no savings, no affordable expedition — and made the pen
    reachable only through a **~50% population crash** (the then-live build dip had to be paid out of a
    famine).
    The shipped values put the pastoral rung clearly *above* subsistence (a real surplus) and let the
    pen's build be paid from it. The retuning deliberately left `upkeep_per_biomass` alone, on the
    grounds that the running cost was the point of the arc; the lever has since been **retired
    outright**, because the running cost it charged was in the wrong currency.
  - **The pen's best-case net-positive floor is retired with it.** `validate()` enforced
    `upkeep_per_biomass < r_pen · p / (2 + r_pen)` for the fastest species — at the operating point a
    pen yields `r·K/4 · p` and ate `u · K·(2 + r)/4`, so `net > 0 ⟺ u < r·p/(2 + r)`. Both sides of that
    comparison had to be provisions. With the feed denominated in fodder there is no food-unit running
    cost to clear, so the bound, its `PEN_ESCAPEMENT_QUARTERS` derivation and `max_wild_regrowth_rate`
    (its only caller) are gone. What `validate()` still enforces is listed under "Ecology/husbandry
    tunables". A pen that cannot be fed starves its herd; it cannot drain a granary.
- **The band's food ledger has no pen term — `PopulationCohortState.penFeedUpkeep` is RETIRED.** It
  was the food a band's pens drew from its larder in a turn (the summed `LocalStore::take` *return*,
  carried on `LaborAllocation::last_pen_feed_upkeep`), exported as its own **negative** row so that
  "my people ate X" and "my animals ate Y" were separate lines and the client's net-food readout was
  not overstated by exactly the upkeep. **The export was sound; the draw was the modelling error.** A
  pen eats grass and hay, so no food crosses from the people's larder to the animals and there is
  nothing to report. The identity loses its term:
  ```text
  larder_delta == foodIncome − foodConsumption − raidForfeit
  ```
  pinned against a **real turn** through the real systems and the real snapshot export by
  `integration_tests/tests/pen_food_ledger.rs` — at the pen's *hungriest* (barren footprint, no hay),
  which is exactly the turn the retired draw would have billed the most for, so the reconciliation is
  not vacuous. The `.fbs` slot is `(deprecated)` in place; field ids are positional.

  > **`FoodFlow::pen_feed_upkeep` went with it** — the fertility half of the same error. `trend_factor`
  > subtracted the pens' bread bill from the band's food flow, so keeping animals suppressed the
  > people's births. `net_ratio` is `(steady_income − demand) / demand` now, and the `turnsOfFood`
  > runway drain lost the same term, so the two readouts still cannot disagree.
- **Display snapshot (on the wire).** The corral state is exposed to the client stream on both
  `WorldSnapshot` and `WorldDelta` (`snapshot.fbs`, `sim_schema`, `snapshot.rs`
  `herd_snapshot_entries`): `HerdTelemetryState.corralled:bool` (= `Herd::is_corralled()`) and
  **`corralProgress:float`** (0..1, the pen-building meter — the animal twin of
  `ForagePatchState.cultivationProgress`), plus **`penFedFraction:float`** — per-herd, for the herd
  drawer and the starving warning.
  - **`penUpkeep` is RETIRED** (the slot is `(deprecated)`). It was the **food** this pen demanded, or
    would demand once built, at the herd's current biomass (`pen.upkeep_per_biomass × biomass`) — a
    projection for an unpenned herd, the live demand for a penned one, computed on the same biomass
    basis as `corralYield` so the two were a matched pair the client **subtracted** on the pre-commit
    `Corral` row. **A pen has no food-unit running cost now**, so there is nothing to subtract and
    `corralYield` is the whole answer. `corralYield` keeps the always-meaningful discipline
    `penUpkeep` used to state: at or below `K/2` the projected yield is honestly `0` (escapement — the
    pen pays nothing until the herd rebuilds), never `0`-because-unpenned.
    > **`buildTurnsRemaining` is the same rule applied to TIME.** It publishes the turns a running
    > Tame/Corral still needs, and — with nothing being built — what the rung this herd would climb
    > next would take the crew currently working it. `-1` only where there is genuinely no answer
    > (already penned, a gate refuses, no crew, or a stalled build). `intensification.md` → "The build
    > on the wire" owns the seam, the gates it carries and which `*WorkCost` it belongs beside.
  - **`penFedFraction`** = last turn's fed fraction, `(footprint_intake + fodderDraw) ÷
    (fodder_per_biomass × biomass)` (`1.0` = fully fed, `< 1` = **starving** — the herd and its yield
    are shrinking, and it recovers when fed again). **One expression in one unit**: it used to add a
    fodder-unit land share to the paid share of a food-unit larder bill, which is how the two units
    came to be mixed at all.
  - **The feed split is `penPastureFraction` + `fodderDraw`, and that is all of it.** Both are fodder,
    both measured against the one demand, so the client draws *"Fed by pasture NN% · hay X.X"* with
    **zero arithmetic** and whatever the two leave uncovered is what the herd starves for. The other
    field on that row is **not** a term of the split: **`penFodderShortfall`** is `max(0, gap −
    fodderDraw)` — what is still missing once the draw is counted, where the gap is what the *land*
    leaves uncovered — which is the figure the row asks the player to act on (`graze.md` → "The number
    the player acts on is `penFodderShortfall`, and the sim subtracts"). Pinned by
    `core_sim/tests/grazing_f3_fodder.rs::the_pen_feed_terms_sum_to_the_fodder_demand_and_never_touch_the_larder`.
    - **`penHayNeed` is RETIRED** (slot `(deprecated)`). It published the gap un-differenced, and
      nothing rendered it: what a pen row states is how much MORE the pen needs. The quantity is still
      struck and still summed into the band's `fodderNeed` — only the per-pen field is gone, along with
      the `Herd` scratch that carried it.
    - **`penLarderBill` / `penHayFood` are RETIRED** (slots `(deprecated)`). They were the FOOD-unit
      terms of a three-way split `pasture_food + penHayFood + penLarderBill == penUpkeep` — the bread a
      keeper handed its livestock, and hay restated in the units the *people* eat in so it could share
      that row. Both die with the larder feed. `fodderDraw` itself survives, in its own grass units,
      which is where it always belonged.

  Plus the forecast pair `huntPolicyCeilings`' **corral** row / `corralYield` (see
  "Pre-commit Yield Forecast"). See "Intensification display snapshot" under Cultivation for the
  plant-side + faction-knowledge fields.
- **Follow-up (final Phase-1 slice):** the **client _rendering_ for both ladders** — cultivation +
  Cultivation-knowledge + tended-patch on the plant side, and domestication + Herding-knowledge +
  corral on the animal side — is the last remaining client-dev slice (the data is now all on the wire).
  **Phase 1b of the managed-population arc rides with it:** the `penFedFraction` starving warning and
  the corrected policy hints. Its third item — the pen's `penUpkeep` as a *negative* row in the band's
  food ledger — is **retired**: there is no such row, because a pen takes nothing from the larder. The
  same is true of that plan's **Phase 2** ("the upkeep is drawn *first* from the tile's biomass and
  only the shortfall is hauled from the larder"): Grazing 2d delivered the first half as the fenced
  footprint's graze, and the shortfall half is now hay or nothing.

### A herd row is assembled from TWO frames, and it must describe ONE turn

`herd_snapshot_entries` walks the display **`HerdTelemetry`** entries and resolves each one's live
**`Herd`** out of the registry (`registry.find(&entry.id)`), then fills the row from *both*. The two
are not the same frame:

| source | written | what it is as of |
|---|---|---|
| `HerdTelemetry` entry | Startup, then Logistics — the end of `advance_herds`, and `repopulate_fauna` on an immigration | **the previous turn** for anything `advance_husbandry` or Population touches |
| live `Herd` | authoritative at all times | this turn |

Three stages of writes land **after** the last telemetry write and before the capture: the rest of
Logistics (`advance_predation`, then `advance_husbandry`'s shed / starve / despawn), the whole of
Population (`advance_labor_allocation` — the build accrual, the hunt take, `corral_at`), and every
stage between. So **every field a row takes from the entry is a turn behind every field it takes
live**, and a row that mixes the two lets the player *see* the contradiction — which is worse than a
row that is uniformly late.

**The governing rule: two fields describing one fact must agree in the frame they ship in.** What is
left on the entry, and why:

| field | source | why |
|---|---|---|
| `id` · `label` · `species` · `sizeClass` · `huntable` | entry | identity and shape; nothing writes them after spawn |
| `x` / `y` | entry | **kept as a pair with the fog gate**, which decided this row's visibility against `entry.position` — publishing a different tile beside it would describe a herd whose presence was judged somewhere else. They cannot differ in any case: the only Population-stage writer of `current_pos` is `Herd::corral_at`, and the tile it is handed is `herd.position()`, the hex the herd already stands on |
| everything else | **live `Herd`** | see below |

- **The build meters** — `domestication`, `corralled`, `corralProgress` — through
  `intensification::build_fraction`, because the build engine accrues in `advance_labor_allocation`
  at Population.
- **`biomass`**, because `advance_husbandry`'s shed/starve shrink and the Population hunt take both
  land after the entry. This one is **not cosmetic**: the client composes the escapement ceiling as
  `max(0, B − floor·K) × rate`, taking `B` from here and `K` from the live `carryingCapacity`, so a
  stale `B` quoted every yield preview from two different turns, every turn.
- **`ecologyPhase`**, **re-derived at capture** from the same stock, capacity and ecology the row
  publishes beside it (`classify_ecology_phase(biomass, herd_capacity, herd_ecology)` — the same
  three seams `Herd::refresh_ecology_phase` uses, so it restates the sim's own call rather than
  modelling it again). The entry's copy was classified in Logistics, while the cut points beside it
  (`collapseFraction` / `stressedFraction`) and `regrowthSamples` are read live off `herd_ecology`,
  which switches rung the instant a Tame or Corral completes — so on a completing turn the published
  word and the published cuts described **different rungs**. Re-deriving is safe because **nothing in
  the sim gates on `Herd::ecology_phase`**: the rung health gates were replaced by the floor's learn
  multiplier, and its only remaining readers are the analytics log line, the display mirror, and the
  Telling's `fauna.collapsing_group_count` / `most_collapsed_species`, which sample the stored word
  and are untouched. There is no behaviour for the wire to disagree with.
- **The heading** — `nextX` / `nextY`, and `routeLength` beside them — because `Herd::corral_at`
  clears `next_pos` in Population, so a herd penned this turn published the heading of the roam its
  pen had just ended and the map drew a migration arrow on an animal that cannot move. (`routeLength`
  is live for uniformity only; a herd's route is built at spawn and never rewritten.)

**The lag became a contradiction when the work pair joined it in the same sentence.** `tameWorkDone`
/ `tameWorkCost` are live, so a completed Tame published as *"Domesticating 50 / 50 work (99%)"* —
one row, one meter, two frames — and read Domesticated the turn after, with nothing in the sim having
reverted.

**The three tests that can see this class of defect all resolve a turn in STAGE ORDER**, which is the
only arrangement in which the two frames can disagree; the snapshot unit tests fabricate the telemetry
from the registry in the same instant and are structurally blind to it.
`core_sim/tests/build_turns_closed_form.rs` pins each build meter against its work pair on the turn
that rung completes. `core_sim/tests/ecology_bands_on_the_wire.rs` pins the phase word against its own
published cuts on the turn a Tame completes — **and it authors distinct pastoral bands to do it**,
because the shipped `husbandry.pastoral.ecology` block is `{}` and inherits the wild cuts, so on the
shipped numbers a rung transition moves the ecology object without moving its cut points and no
assertion could tell the two rungs apart. `core_sim/tests/herd_row_one_frame.rs` pins the stock
against a real post-telemetry writer (a starving pen) and the heading against the turn the pen
completes.

**`HerdTelemetry` is NOT rebuilt after Population, deliberately.** Per-field live reads were chosen
over a second rebuild because a rebuild's blast radius covers the herds `advance_husbandry` sheds or
despawns later in Logistics, which today publish with no registry herd and a zeroed forecast. The
`entry` copy survives on every live-read field as the fallback for the unreachable "in telemetry,
gone from the registry" case.

See Also: "Cultivation (Intensification Phase 1a)" under Depletable Forage — the plant twin of this
mechanic (the two are near-mechanical transposes).

> `FaunaPursuit` is **not** snapshot-persisted (unlike `HarvestAssignment`): a
> `rollback` mid-pursuit cleanly cancels the in-flight hunt (the rehydrated cohort
> simply lacks the component). Pursuits are short-lived; revisit if needed. Domestication
> state lives on the `Herd` (in `HerdRegistry`), alongside `biomass`.

> **The authoritative `HerdRegistry` *is* rollback-persisted** (as of the intensification
> arc's first slice, `docs/plan_intensification.md` §0-i). Each live `Herd` — identity,
> movement (`route`/`step_index`/`current_pos`/`dwell_remaining`/`roam`/`next_pos`/`corralled_at`),
> **and** its depletable-ecology subset (`biomass`/`carrying_capacity`/`ecology_phase`/
> `domestication_progress`/`owner`) — survives a rollback because the **checkpoint carries the whole
> registry** (`SimState::herds`). It reached that state by way of a serde `HerdState` mirror on
> `WorldSnapshot.herd_registry`; the checkpoint arc made the mirror redundant and it was deleted
> (`checkpoints.md`). This closes a **latent bug**: only the lossy display `HerdTelemetry`
> (`WorldSnapshot.herds`) used to be captured, so herd biomass/position silently kept their
> post-rollback values. Restore rebuilds the derived `HerdDensityMap` + `HerdTelemetry` (as
> `advance_herds` does post-loop) so nothing is stale for a turn. The FlatBuffers client stream is
> untouched (it keeps using the display telemetry). The serde `HerdState` / `HerdRoamState` /
> `EcologyState` records and the registry-side `HerdRegistry::{from_states, update_from_states}`
> constructors that read them are **deleted** — with the mirror gone there was no producer left, and a
> decoder with nothing to decode is a second, drifting definition of a restored herd.

Market hunting shipped as the third extractive rung, later renamed `Deplete`
(`docs/plan_hunt_yield_model.md` §2 — every policy sells, so the rung is named for its
pressure); `SedentarizationScore` shipped (see
"Sedentarization" under Campaign Loop); **corrals shipped** (Intensification Rung 1c — see "Corral"
below). Still deferred (`docs/plan_wildlife_hunting_overlay.md`): the `Camp` entity, and wiring the
sedentarization hard prompt to an actual `found_settlement`. The tile-based `HuntGame` handler stays
neutralized (its client button no longer surfaces).

---

