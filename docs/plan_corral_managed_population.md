# The Corral as a Managed Population — and the Husbandry Yield Ladder

**Status:** Phase 1a (sim) built, measured, **retuned**, and re-measured. Supersedes the flat-rate
corral stopgap landed in PR #117. Every number in this doc is **measured from real sim runs**, not
derived — where the design's arithmetic disagreed with the sim, **the sim won**, and §3.0 and §5 say
so explicitly. The first measured campaign (§8) showed the initial levers bit far too hard and they
were retuned; the *model* did not change, which is the point of keeping the two separable.

**Owning subsystem docs:** `core_sim/CLAUDE.md` → Fauna & Wild Game → *Corral (Intensification
Rung 1c)*; `docs/plan_intensification.md` → *Open tuning dials*.

---

## 1. The ask

> "The corral should be considered a population on its own — it needs resources (food) for the
> animals to flourish. We should consider that in the upkeep of the corral. Base the output on the
> number of animals you have, which is impacted by the resources (food, etc.) you give it."

The pen becomes a **population in its own right**: its yield follows the animals you actually keep,
those animals **eat** every turn, and underfeeding **shrinks** the herd. A one-off 25-turn build that
then prints food forever becomes a **sustained commitment with a running cost**.

---

## 2. What is actually wrong today — the stock/flow error

The corral's flat rate is one symptom of a **single error that runs through the whole husbandry
ladder**: some rungs pay a share of standing **stock**, others pay a share of regrowth **flow**, and
the two are not commensurable. No choice of scalar reconciles them.

| Rung | Formula | Kind | Red Deer (K=1200), prov/turn |
|---|---|---|---|
| Wild, Sustain hunt | `sustainable_yield(B,K,eco) × 0.02` — MSY = `r·K/4` | **flow** | **0.30** |
| Wild, Market hunt | `0.20 × B × 0.02` (crashes the herd) | stock | 4.80 |
| **Mobile domesticated** | `B × husbandry.provisions_per_biomass` (0.01) | **stock** | **12.00** |
| Corral, building | `0.25 × MSY × 0.02` | flow | 0.075 |
| **Corral, finished** | `B × corral_provisions_per_biomass` (0.012) | **stock** | **14.40** |

A band of ~30 eats **~0.76 food/turn** (`per_capita_draw` 0.03 × weighted mouths). So:

- **One mobile domesticated Red Deer herd pays 12.0 prov/turn — sixteen times a band's entire food
  demand — for zero labor, zero upkeep, forever.** (`fauna.rs:1259`, in `advance_husbandry`: no
  worker is required, and the yield is split across the owner's bands.) Domestication does not
  *improve* the food economy; it **ends** it.
- The finished corral pays **14.40** — barely more than the **12.00** you already get for free by
  domesticating and *not* penning. Today the pen costs a 25-turn build, a permanent worker, and the
  risk of losing everything to an escape, to buy a **20% raise** over doing nothing.

So the corral is not merely mispriced. **The rung below it is so overpaid that the pen is nearly
pointless**, and the ladder's payoff ordering is held up only by a rounding error.

**This is why the arc cannot fix the corral alone.** Re-basing the pen onto flow (§3) while leaving
mobile domestication at a stock share would drop the pen to ~3.6 prov/turn against a free 12.00 —
**inverting the ladder** and making corralling strictly irrational. The stock-share rate is the
disease; the corral is the symptom the player noticed.

### 2.1 Why this went unnoticed: the plant side got lucky

The **tended patch** — the plant twin, `forage.rs` — uses the *identical* formula
(`tended_provisions_per_biomass × biomass`, no draw-down, one worker) at an almost identical rate
(0.010 vs the corral's 0.012). Yet it pays a sane **3.2× wild MSY**, while the corral pays **48×**.

The ratio a flat rate actually buys is `flat_rate ÷ (r/4 × provisions_per_biomass)`:

| | `K` | `r` | `p` | wild MSY | flat rate | **ratio** |
|---|---|---|---|---|---|---|
| Tended patch | 120 | 0.25 | 0.05 | 0.375 | 0.010 | **3.2×** |
| Corral | 1200 | 0.05 | 0.02 | 0.300 | 0.012 | **48×** |

A herd regrows **5× slower** and converts **2.5× poorer**, so its MSY-per-biomass is **12.5× lower** —
and the same flat rate therefore buys 12.5× more relative payoff. **A flat `rate × biomass` payoff has
no idea what its source's regrowth is.** It happened to land in a sane place for plants and a
catastrophic one for herds. That is the whole bug, and it is an argument for never expressing a
sustainable yield as a share of stock again.

---

## 3. The model: management buys you a growth rate

One idea unifies the ladder. A herd's *sustainable* yield is its regrowth flow — its MSY. What
husbandry buys is not a licence to eat the standing stock; it is a **higher growth rate**, because a
managed herd is protected from predation, disease, and winter kill.

> **Every rung pays MSY. The rungs differ only in the ecology that MSY is computed against —
> and in what that ecology costs you.**

```
yield = managed_yield_biomass(B, K, ecology_for_this_rung) × hunt.provisions_per_biomass × output_mult
```

`regrow_biomass`, `net_biomass_delta`, `sustainable_yield` and `peak_regrowth` (`fauna.rs`) already
take an `&EcologyConfig`. **The entire model is expressible by handing them a different one.** The
rung → ecology mapping lives in exactly one place (`fauna::herd_ecology`), and the capacity that
bounds a herd in one more (`fauna::herd_capacity` — a penned herd is bounded by the *pen*, not the
land). No call site may re-derive either: a second copy of that mapping is how a forecast starts
promising a number the take won't pay.

The one genuinely new helper is `managed_yield_biomass` — the escapement rule of §3.0, which both
husbandry rungs share so the pen and the pastoral herd can never disagree about what a managed
harvest *is*.

| Rung | Ecology | `r` | Costs | Red Deer prov/turn |
|---|---|---|---|---|
| Wild, Sustain | `ecology` | 0.05 | a worker | **0.30** |
| Mobile domesticated (pastoral) | `husbandry.pastoral.ecology` | 0.25 | **none — passive** | **1.50** |
| Corral, building | `0.50 × pastoral MSY` | 0.25 | a worker, 25 turns | **0.75** (the dip) |
| Corral, finished | `husbandry.pen.ecology` | 0.90 | a worker + **food upkeep** + pinned | **5.40** gross |

A herd you are corralling is **already domesticated** (that is a gate on the `Corral` policy), so the
build dip is `corralling_yield_fraction × MSY` against the **pastoral** ecology — **0.75**, half of
the 1.50 you would collect for walking away. That gap *is* the investment (§3.3).

The pastoral rung stays **passive and labor-free** (as today — `advance_husbandry` pays every
domesticated herd's owner with no assignment required), **except while labor is working the herd**,
which is what makes the dip cost anything at all (§3.3). It is simply no longer *absurd*: at 1.50
prov/turn a tamed herd now feeds roughly **one band** and leaves a real margin to save, where today
it feeds sixteen.

**The managed harvest now draws the herd down, and that is what makes it sustainable.** The flat rates
never drew the herd down at all: a penned herd parked at capacity and printed food forever.

**Why the pen eats and the pastoral herd does not:** a roaming herd grazes the land for free — that
is what roaming *is*. A **penned** herd is confined and cannot forage, so **you** must bring it food.
The upkeep is not an arbitrary tax; it is the physical price of the thing that makes a pen a pen.

### 3.0 The harvest rule: constant escapement, not constant catch

**The original derivation in this doc was wrong, and the sim caught it.** It claimed that taking
`sustainable_yield(B, K)` each turn converges the herd on `K/2` — "solve `logistic(B) =
sustainable_yield(B,K)`, it reduces to `(B − K/2)² = 0`". That is a **continuous-time idealisation**.
The discrete sim does not behave that way, and the difference is fatal.

The take is evaluated **after** Logistics regrowth. So below `K/2` a constant-catch rule harvests
`g(B + g(B))`, which is strictly greater than `g(B)` — **more than the herd actually grew**. The herd
loses ground every turn, and each turn it loses ground it grows less, so the loss compounds. `K/2` is
an equilibrium only from *above*; from below it is a cliff. At the wild `r = 0.05` the leak is noise.
At the pen's much higher `r` it is lethal: **a fully-fed pen knocked below `K/2` decays to zero in ~12
turns and can never recover** — a pen that starves once is dead forever, no matter how well you feed
it afterwards. (Measured at `r = 0.60`, the pen's rate before the §8 retune; at the shipped `r = 0.90`
it is worse still.) Verified numerically before anything was changed.

The shipped rule is **constant escapement** (`fauna::managed_yield_biomass`):

```
take = min(peak_regrowth(K), max(0, B − K/2))
```

Harvest the biomass standing **above** the MSY point, and never more than one turn's peak regrowth
(reusing the existing `peak_regrowth` — no second formula). Identical yield at capacity and at the
operating point, it converges on `K/2` from **both** sides, and a depleted herd **rebuilds** instead
of dying. This is what makes the required "recovers when fed again" behaviour possible at all; under
constant catch it is unreachable.

This is precisely the *measure it, don't assume it* failure mode: the algebra was clean, self-
consistent, and wrong, because it modelled a continuous process the sim runs discretely.

At the settled operating point (`B* = K/2`) the Red Deer pen grosses **5.40** and pays **1.74** in
upkeep, netting **3.66** — **12× wild Sustain** and **2.4× the pastoral rung** below it. The ladder
is monotone, every rung is a flow, and the multiples are all explainable by one number. (Upkeep is
charged on the *post-regrowth* biomass — see §5 — which is why it is 1.74 and not the 1.20 an earlier
draft of this doc predicted.)

### 3.1 The turn, for a penned herd

1. **Upkeep.** Demand `upkeep_per_biomass × B` food from the keeper band's larder.
   `LocalStore::take` already returns *how much it actually took* — that is the partial-payment
   primitive, no new plumbing. `fed_fraction = paid / demand ∈ [0,1]`.
2. **Growth — scaled by how well you fed it.** `regrow_biomass` multiplies a penned herd's growth by
   `pen_fed_fraction`, so an unfed pen does not grow at all. **This is load-bearing, not a nicety:**
   without it, an unfed herd still grew on the pen's own high `r` and parked at `K/2`, paying its
   keeper a full yield for feed they never bought — *not* feeding was very nearly as profitable as
   feeding, which inverts the entire mechanic. Underfed → the herd **shrinks** by
   `pen.starve_shrink_rate × (1 − fed_fraction) × B`, floored at the extinction floor.
3. **Yield.** The keeper harvests the escapement surplus (§3.0):
   `managed_yield_biomass(B, K_pen, pen.ecology) × provisions_per_biomass × output_mult`.

`K_pen = pen.capacity_fraction × carrying_capacity` — the pen holds a share of what the land held, so
it scales per-species with no new magic absolute.

**A penned herd is exempt from `advance_herds`' extinction despawn.** Dispersal is the *mechanism* of
local extinction in this sim — a group below the viability floor scatters — and a **confined** herd
cannot disperse. So a starving pen floors at `extinction_floor × K_pen` and **recovers when fed
again**, rather than being deleted out from under a 25-turn investment. Without the exemption, a herd
starved to exactly the floor would be despawned by the retention check.

### 3.2 What this retires

- `husbandry.corral_provisions_per_biomass` (0.012) — **deleted**. There is no flat rate.
- `husbandry.provisions_per_biomass` (0.01) — **deleted**. Pastoral pays MSY under its own ecology.
- `fauna::corral_provisions` — **deleted**; the shared escapement helper does this.
- **Starvation replaces the cliff.** The binary escape (`corral_progress` → 0.0 on one untended turn)
  survives **only for the no-keeper case** — nobody is minding the gate, so the herd breaks out; that
  is the right model for it. But a keeper who *is* there and simply **cannot pay** now **starves** the
  herd gradually instead of losing everything at once. It withers toward a remnant and recovers when
  fed. Starving your animals to feed your people becomes a *decision you can watch and reverse*, not
  an accident that silently deletes 25 turns of work.

### 3.3 The build dip must not be free

`advance_husbandry` pays the passive pastoral yield for a domesticated herd **even while a band is
actively working it**. During the 25-turn Corral build the owner therefore collects the build dip
**plus** the passive rung — **more than they would get for doing nothing at all.** Corralling would
cost *nothing*, which destroys the investment mechanic the entire intensification ladder is built on:
the dip is supposed to be the price you pay to intensify.

The fix: **the passive rung skips a herd that labor is already working.** You are not paid twice for
the same animals. That restores a real dip — **0.75 while building vs 1.50 for walking away**, so the
pen costs ≈ **19 provisions forgone over 25 turns**, recouped ~15 turns after completion (pen net 3.66
vs pastoral 1.50 = **+2.16/turn**).

The double-payment is **pre-existing** — and was far worse under the flat rate (12.0/turn, free). But
the flat rate was so enormous that the dip was already meaningless, so nothing surfaced it. The flow
model makes it load-bearing, and therefore visible.

---

## 4. The decision this creates

The pen stops being a strictly-dominant upgrade and becomes a **wager on staying**:

- It out-pays every other rung — but only while you **feed it**, every turn, forever.
- Its food cost lands **exactly when food is scarce**, so a bad winter forces a real choice: eat the
  seed corn (let the herd shrink, losing future yield) or go hungry (starvation deaths) — the
  capital-vs-consumption tension that a flat rate cannot express.
- It **pins the band**, and now the pin has teeth: the running cost is the tether.

---

## 5. Config (`fauna_config.json` → `husbandry`)

Per the no-magic-numbers convention — every number below is a lever, and `EcologyConfig` is already
`#[serde(default)]`, so the nested blocks cost nothing.

```json
"husbandry": {
  "corralling_yield_fraction": 0.50,
  "pastoral": {
    "ecology": { "regrowth_rate": 0.25 }
  },
  "pen": {
    "ecology": { "regrowth_rate": 0.90 },
    "capacity_fraction": 1.0,
    "upkeep_per_biomass": 0.002,
    "starve_shrink_rate": 0.10
  }
}
```

**These are the retuned values** (`pastoral.r` 0.15 → 0.25, `pen.r` 0.60 → 0.90,
`corralling_yield_fraction` 0.25 → 0.50). `upkeep_per_biomass` was **deliberately left alone**: the
running cost is the entire point of the arc, and balance must not be "fixed" by quietly weakening it.
See §8 for the campaign measurement that forced the retune.

**Design invariant, enforced in code** — the pen must net positive at its operating point, or it is a
trap:

```
upkeep_per_biomass  <  r × p / (2 + r)          where r = pen.ecology.regrowth_rate
                                                      p = hunt.provisions_per_biomass
```

**Not `r × p / 2`, as an earlier draft of this doc had it.** Feed is charged on the **post-regrowth**
biomass — `K/2 + r·K/4`, not `K/2` — because **you feed every animal in the pen, including the ones
you are about to harvest**. Carrying that through:

```
yield  = r·K/4 · p                    upkeep = u · K(2 + r)/4
net > 0  ⟺  u < r·p / (2 + r)
```

At the shipped `r = 0.90`, `p = 0.02` the true bound is **≈0.0062**, and the shipped
`upkeep_per_biomass = 0.002` clears it by **3.1×**. The idealised `r·p/2` (= 0.009 at this `r`) is too
loose: it would admit a whole band of upkeep values that are, in fact, a **permanent net food loss** —
a pen that costs more to feed than it ever pays back, silently. This is exactly the kind of trap a
config file must not be able to set, and `validate()` has a test pinning that 0.008 is refused even
though the loose bound would have allowed it.

`FaunaConfig::validate()` (new in this arc, running inside `from_json_str` so *every* load path is
covered) enforces it, along with ladder monotonicity (`pen.r > pastoral.r > wild.r`) and the bounds on
every other lever. It also closes the [tracked validation gap](../TASKS.md) for `FaunaConfig`, which
previously asserted its invariants only over the builtin, in unit tests — so a `FAUNA_CONFIG_PATH`
override could break any of them in silence.

---

## 6. Phasing

Following the repo's established split (sim lands first, the client readout is its own slice):

- **Phase 1a — sim (one PR).** The flow-based ladder: `pastoral` + `pen` ecologies, the constant-
  escapement harvest (§3.0), food upkeep, fed-scaled growth + graded starvation with the despawn
  exemption (§3.1), the passive rung skipping a worked herd so the build dip costs something (§3.3),
  `FaunaConfig::validate()` (§5), the rebalance, and the measured tables of §7. Retires both flat
  rates and `fauna::corral_provisions`. The binary escape **survives, narrowed** to the no-keeper case
  (§3.2). Sim-only.
- **Phase 1b — client (one PR).** The readout: the pen's upkeep as a **negative** row in the band's
  food ledger (the `foodIncome` / `foodConsumption` seam already exists), the fed fraction, a
  shrinking-herd warning, and the corrected policy hints. Without this the player watches their
  larder drain with no explanation — **Phase 1a must not ship to a player without 1b**, only to
  `main`.
- **Phase 2 — grazing (deferred).** The pen's upkeep is drawn *first* from the tile's `ForagePatch`
  biomass (the animals eat grass — a resource humans cannot), and only the **shortfall** is hauled
  from the larder. This makes pasture quality gate pen size and tile choice matter — and it removes
  the one honesty wrinkle Phase 1 cannot: with a single food scalar, food-in/food-out is a physically
  backwards conversion (real livestock is calorie-*negative*; it is worth doing because it eats what
  we can't). Phase 1 papers over that with a favourable exchange rate. `ForageRegistry` and
  `regrow_patch` already exist, so Phase 2 is plumbing, not new modeling.

---

## 7. Measured, not assumed

The lesson of PR #117 — *pin every number to the take the sim actually performs, never to another
preview*. It earned its keep again here: **two of this design's derivations were wrong** (§3.0's
harvest rule, §5's upkeep bound), and both died on contact with a real sim run rather than on
reflection.

Every number below is **measured** — a real herd, run forward through the real systems in real stage
order (Logistics regrow → husbandry → Population labor). Provisions/turn.

**At capacity (`B = K`)** — the state a freshly-penned herd starts in:

| species | `K` | wild | pastoral | pen gross | upkeep | **pen net** |
|---|---|---|---|---|---|---|
| Rabbit Warren | 200 | 0.050 | 0.250 | 0.900 | 0.400 | **0.500** |
| Red Deer | 1200 | 0.300 | 1.500 | 5.400 | 2.400 | **3.000** |
| Thunder Mammoth | 12000 | 3.000 | 15.000 | 54.000 | 24.000 | **30.000** |

**At the settled operating point (`B* = K/2`)** — where escapement drives the herd, and therefore the
number that actually describes a running pen:

| species | `K` | wild | pastoral | pen gross | upkeep | **pen net** |
|---|---|---|---|---|---|---|
| Rabbit Warren | 200 | 0.050 | 0.250 | 0.900 | 0.290 | **0.610** |
| Red Deer | 1200 | 0.300 | **1.500** | **5.400** | **1.740** | **3.660** |
| Thunder Mammoth | 12000 | 3.000 | 15.000 | 54.000 | 17.400 | **36.600** |

The **ladder is monotone at both biomasses, for all three species.** The Red Deer build dip is
**0.75** against the **1.50** you would collect for walking away — ≈ **19 provisions forgone over the
25-turn build**.

Still to hold, and guarded by tests:

1. **Forecast == actual.** `hunt_forecast`'s `::tended` branch returns the same number the corral-tend
   branch of `advance_labor_allocation` pays.
2. **The underfed pen converges** — a band that cannot pay upkeep reaches a stable smaller herd, and
   recovers when fed. It does not oscillate, and it does not crash to zero (§3.0 — under the original
   constant-catch rule it *did*).

### 7.1 The nerf

Large, and deliberate. It is felt across the early game — sedentarization pressure, band food
security, `days_of_food`, and how quickly a band can afford an expedition. §8 measures all of it:

| | today | shipped | change |
|---|---|---|---|
| Mobile domesticated (Red Deer) | 12.00 | 1.50 | **−8×** |
| Corral, gross | 14.40 | 5.40 | −2.7× |
| Corral, **net of upkeep** (at `B*`) | 14.40 | **3.66** | **−4×** |

For scale, a ~30-person band eats **~0.76 food/turn**. Today *one* free domesticated herd feeds
**sixteen bands**. Afterwards a tamed herd feeds roughly **two**, and a fully-invested, fed, staffed
pen feeds **under five**. The early game stops being solved by a single lucky domestication — but, as
§8 shows, a band that tames a herd can still *save*.

---

## 8. Measured in a live campaign

The §7 tables price a *herd*. They cannot tell you whether the game still works. This section does,
and it is the most important evidence in this document: **the first set of levers passed every unit
test and every ladder check, and was still badly wrong.** Only a campaign run showed it.

**Method.** `build_headless_app` + `run_turn`, default `late_forager_tribe` start (one ~30-person
band), **3 pinned seeds** — the shipped `map_seed` is `0` (= entropy), so unpinned runs are not
comparable and any before/after on them is noise. Scripted play, identical harness on both sides,
old code restored via `git stash`. Red Deer seed shown; the other two track within ~1%.

**The harness staffs only two sources, so the absolute levels are pessimistic. The deltas are the
signal.**

### 8.1 What the first levers did (`pastoral.r` 0.15, `pen.r` 0.60, dip 0.25)

- **The pastoral rung landed on a knife-edge.** Income **1.275** vs consumption **1.294**. The band
  sat at `days_of_food ≈ 1.0` *forever*: no accumulation, no expedition, no settle prompt. A
  treadmill, not a choice. The old surplus had been *entirely* the free passive stream — deleting it
  deleted the early game's only savings mechanism.
- **The corral — the intended escape — cost a ~50% population crash.** Paying the dip out of a
  ~5-provision larder starved the band from **46 → 26 people** before the pen completed. The ladder's
  next rung was reachable only by starving half your people.

The 48× flat rate was absurd; a permanent subsistence treadmill is the opposite absurdity. So the
levers were retuned — **and the model was not touched**, which is precisely why the two are separable.

### 8.2 What the shipped levers do

Turns 1–33 are **byte-identical** before and after, and **time-to-first-domestication is unchanged at
turn 33**: the rebalance changes nothing until you tame a herd. Then:

| | flat-rate baseline | **stay pastoral** | **build the corral** |
|---|---|---|---|
| income after taming | 1.275 | **1.875** (forage 0.375 + pastoral 1.50) | **1.125** building → **5.775** gross penned |
| larder @ t50 / t100 | 6.1 / 1.27 | **16.3 / 17.2** | **3.6 / 97.7** |
| `days_of_food` @ t100 | 1.0 | **7.0** | **43.8** |
| population t33 → t100 | 46 → 55 | 46 → **99** (equilibrium 75) | 46 → **90** (179 by t160) |
| min population during build | — | — | **none — it grows, 46 → 56** |
| Sedentarization @ t100 | 14.4 (plateau) | **18.4** (plateau) | **25.8**, still climbing (35.5 @ t160) |
| 4×10 expedition affordable? | never | no (a 4×5 is, ~t50–110) | **yes, from ~t75** |

- **A taming band now accumulates.** Income 1.875 against consumption 1.15 → the larder climbs
  5.7 → 23.3 over ~45 turns. Real savings, a real buffer, a small expedition within reach.
- **The pen can be built without a famine.** Building income 1.125 vs consumption ~1.2–1.4 → the
  larder dips *gently* (min 2.6 days of food) and **the population grows straight through the build,
  46 → 56.** The dip is now paid out of a surplus. That is the whole intent of the retune.
- **The escapement model behaves in a live campaign exactly as designed.** Herd biomass *rises*
  during the build (901 → 1019) — the dip is a sustainable draw — and then pins at **exactly 600.0 =
  K/2** once penned, and stays there.
- **The pen is the only rung that banks capital.** The 2.4× *gross* ratio badly understates it: the
  decisive quantity is the **surplus**, not the gross. Pastoral converges to a hand-to-mouth
  Malthusian equilibrium (pop 75, 7 days of food); the pen reaches 179 people and 43.8 days.

### 8.3 It does not overshoot — checked deliberately

The failure mode in the *other* direction would be recreating the original problem in a smaller font:
food ceasing to be a constraint at all. It doesn't.

**Food stays binding.** The corral branch's larder peaks around **150 and then turns over** (149.8 @
t150 → 148.5 @ t160) as population growth drives consumption (4.44) past net income (5.40 gross −
1.74 feed = **4.04** at `B*`). It converges to a Malthusian equilibrium at a *larger* band rather than
running away. The largest steady surplus in any run is ~1.5 food/turn against a band that eats 4.4.
Nothing here resembles "one herd feeds sixteen bands."

### 8.4 Open flag — sedentarization is now reachable only via the pen

Staying pastoral plateaus at **18.4** and **never crosses the soft-40 settle prompt**. The corral
branch reaches 25.8 @ t100, 35.5 @ t160, and crosses 40 around **~t210**.

The score's *domestication* input is untouched by this arc — it was the **surplus** input that moved.
The shape reads right (**penning is what settles you**), but the prompt is slow. This is deliberately
**not** fixed here: it is a separate lever, and moving it in the same change would confound the
rebalance we just measured. Flagged for follow-up, not silently absorbed.

---

## See Also

- `core_sim/CLAUDE.md` → Fauna & Wild Game (`HusbandryConfig`, ecology phases, `regrow_biomass`),
  Pre-commit Yield Forecast (the forecast == actual invariant).
- `docs/plan_intensification.md` → §4b (the corral as the animal mirror of the tended patch), *Open
  tuning dials*.
- `docs/plan_settlement_population.md` → the improvement catalog and decay-as-sunk-cost this plugs
  into.
- **`TASKS.md`** → *Corral as a managed population* (this arc), *Give `FaunaConfig` and `LaborConfig`
  a real `validate()`* (folded into Phase 1).
