# A Deposit Is a Stock With a Regrowth Rate, and Stone's Is Zero

**Status:** arc open (worktree `wood-and-stone-producers`, branch
`worktree-wood-and-stone-producers`). The deposit, the source and the take are **built**; the
standing bill (§6) and the readouts (§7) follow on the same branch. Closes the gap left open by
`docs/plan_standing_upkeep.md` §6 — `wood` and `stone` ship as materials that **nothing produces**.
Issue #583. As-built rationale: `.claude/rules/core_sim/extraction.md`.

**Scope:** this arc gives both materials a producer, through **one** mechanism that the minerals arc
(copper, silver, gold) is meant to slot into without re-design. It does **not** change what any
existing tool is made of — see §8.

---

## 1. The gap

`wood` and `stone` are in the material roster with real consumers and no source. The `animal:pen`
rung eats hurdles, which are woven from wood; the `route:paved_road` rung eats stone. Neither
material has any way into the game except the turn-one outfitting window
(`start_profiles.json` → `opening_loadout.pickable_materials`, 30 material points).

> ⛔ **THE ISSUE'S OWN PREMISE IS STALE AND THE REAL GAP IS SHARPER.** #583 says both are "supplied
> only through worldgen's start stock." That mechanism no longer exists: `materials.json` records
> that the per-worker start stock was **deleted rather than zeroed**, and `equipment.json` ships
> `start_stock_fraction: 0.0`. So this is not "the opening stock runs out." A player who spends
> their 30 points on the shipped `material_defaults` (bone 3 / fibre 17 / hide 8) has **zero wood and
> zero stone for the whole game**, and the pen rung and every road above dirt are unbuildable for
> that faction — decided before turn one, on information the player does not have yet.

### 1a. Why the existing material model cannot close it

**Every material in the game today is a byproduct of eating.** A species declares
`materials: [{ material, per_biomass, characteristics }]` on its yield edge, and `flora_config.rs:135`
states the rule: a harvest pays food and fodder "plus `B × per_biomass` of each material it names,"
where `B` is the biomass **taken for food**. Hunt a deer, hide and bone fall out. Forage reeds,
fibre falls out.

**That model cannot produce either of these two.**

- **Wood.** Tying wood to a forest patch's yield edge means you get timber in proportion to how many
  acorns you ate — so a band that stops foraging a wood stops getting wood from it. Cutting wood is
  its own job, and a forest holds timber whether or not anyone is eating from it.
- **Stone.** A cliff pays no food at all, so there is no `B` for a fraction to multiply. The
  arithmetic has no input.

**So they are one problem, not two:** *go to a place, spend work, get a material, get no food.*
Nothing in the game does that. Every work site in it is a food site.

### 1b. The seam this rides on already exists

`RungBranch` (`intensification.rs:1064`) already carries a branch that is **not a food web** —
`Route` — and its doc names the three ways it differs: it sits on **one tile**, it belongs to **no
camp**, and its upkeep reads **the ground** because there is no source under a road. Two of those
three are exactly what a quarry wants. This arc is a fourth and fifth branch on a seam that was
deliberately opened for the third, not a new subsystem.

---

## 2. The one idea

**A deposit is a stock with a regrowth rate, and stone's is zero.**

Flora and fauna already work this way: a standing amount, a capacity the land sets, a regrowth rate,
and a take. This arc adds no new shape.

| | stock | capacity | regrowth rate | behaviour |
|---|---|---|---|---|
| A wood | standing timber | what the land supports | slow, positive | renews; can be **over-cut** |
| Surface rock | loose stone | what the terrain scatters | slow, positive | effectively inexhaustible |
| A quarry | the rock body | the original endowment | **0** | only ever goes down; is **worked out** |
| An ore body (later) | the ore | the original endowment | **0** | the quarry's shape, smaller |

**One dial decides renewable versus finite,** and it is a rate that happens to be zero. There is no
`is_finite` flag, no finite-deposit branch, and no special case: with `regrowth_rate: 0.0` the
existing growth term returns the stock unchanged. That is the whole reason to build it this way —
**a new metal is a deposit and a config row, not a second system.**

---

## 3. Two branches, and they are knowledge tracks

`RungBranch` gains two members:

- **`Forestry`** — wood. Its knowledge is **conservationism**: the skill of taking from a wood
  without ruining it.
- **`Extraction`** — stone today, metal later. Its knowledge is cutting rock out of the ground.

**Why stone and metal share a branch.** Quarrying and mining are one skill; only the material in the
deposit differs. That is what makes copper cost a deposit and a config row rather than a third
branch, and it is the single decision this arc exists to get right.

**Why wood does not join them.** Forestry is genuinely different knowledge, and its rungs buy a
different thing (§4).

> ⛔ **REGROWTH IS A PROPERTY OF THE DEPOSIT, NOT OF THE BRANCH — and an earlier draft of this
> design had it the other way round.** That draft split the branches on renewable-versus-finite,
> which reads well until you notice that **surface flint and a quarry are the same skill and only one
> of them runs out**. Ray's case is the falsifier: *"collecting flint rock to make spears early game,
> mine rock as demand grows."* Both are `Extraction`; their regrowth rates differ. Binding
> renewability to the branch would have made a flint scatter and an ore body unrepresentable on one
> ladder, which is precisely what the minerals arc needs them to be.

### 3a. You learn a rung by practising where that rung COULD BE BUILT

`earns_knowledge` credits the lesson the source's own rung teaches, and on both deposit branches that
credit is gated on the ground: the lesson is paid only where the rung it **unlocks** could be sited.

**Picking loose stone off a 40-unit scatter teaches nothing about quarrying, because no quarry could
ever stand on a 40-unit scatter.** Picking it off a rock body teaches it. The lesson is credited for
the ground it is practised on, not for the verb. The seam, and why it asks `forage::rung_site_refusal`
rather than reading `min_deposit_capacity` a second time, are in
`.claude/rules/core_sim/extraction.md`.

**It is inert wherever there is no rung for the ground to refuse** — a lesson that opens nothing on
its branch, or one that opens a rung stating `site_requirement: null`. On these two branches that is
`forestry:deadfall`, whose woodcraft opens a `forestry:felling` that asks nothing of the
ground, and both branch **tops**, which earn nothing at all. So the shipped ladder states exactly two
live cases: `extraction:gathering` → quarrying → `extraction:quarry`, and `forestry:felling` →
conservationism → `forestry:coppice`. Both follow from the one sentence — you learn to work rock on
rock worth quarrying, and to manage a wood on a wood worth managing.

> ⛔ **THE FOOD AND ROUTE WEBS ARE UNTOUCHED BY THE *ARM*, NOT BY THE GROUND.** It is tempting to fold
> them into the sentence above as more rungs with nothing to refuse, and that reading is **false of
> the plant web**: the ladder lookup is branch-generic, `plant:wild` earns cultivation, and the
> `plant:tended` it unlocks demands a gathering site. What keeps those webs out is that the term is
> composed into the **deposit arm alone** — so wiring it into them later is a real behaviour change
> there, gating `seed_selection` on fresh water, rather than the no-op the shorter wording promises.

> ⛔ **ON THE STONE BRANCH THE GATE AND THE ESCAPEMENT DIAL ARE NOT INDEPENDENT, and that is the thing
> to know before moving either number.** Every renewing stone scatter tops out at 70, under
> `extraction:quarry`'s 100 — so the ground that carries an escapement dial is exactly the ground the
> gate now credits nothing for, and `intensification::PRACTICE_AT_THE_PLAIN_RATE` is the only pacing
> `extraction:gathering` ever pays at. Lowering that 100 under 70 does not merely admit more sites; it
> re-opens a second pacing reading on that rung.

---

## 4. What a rung buys, and it differs by branch

The ladder model is unchanged (`plan_standing_upkeep.md` §2.8): a source has **one position** in
cumulative work units; climbing costs work and materials; holding costs work per turn and materials
per turn; the position decides what the ground pays.

### 4a. Forestry rungs raise REGROWTH, not the take

This is conservationism expressed mechanically: **you do not get more per turn by cutting harder,
you get more per turn forever by managing the wood.**

| rung | what it is | what it changes |
|---|---|---|
| `forestry:deadfall` | gathering fallen wood; **free floor, no upkeep** | a trickle; barely touches standing timber |
| `forestry:felling` | actively cutting standing timber | a real rate — and **over-cutting becomes possible** |
| `forestry:coppice` | a managed, cut-and-regrow wood | **`regrowth_rate` up** |

**Rung-driven regrowth is a shipped primitive, not an invention.** `fauna.rs:1483` already
interpolates `regrowth_rate` over a herd's ladder position, and `forage::patch_ecology` does the same
for plants. The comment there states the rule this follows: *"the rate interpolates; the phase bands
step"* — a rate is a payout and blends, a classifier's cut points come from the rung actually held.

### 4b. Extraction rungs raise REACH

A finite deposit has no regrowth to raise, so the rung must buy something else. What it honestly buys
is **recovery** — how much of the deposit you can ever get out.

| rung | what it is | what it changes |
|---|---|---|
| `extraction:gathering` | picking loose stone off the ground | available almost anywhere; a trickle; reaches only the surface |
| `extraction:quarry` | a cut working face | only where there is rock; real volume; reaches most of the body |
| `extraction:mine` (minerals arc) | a shaft | reaches ore that surface work cannot touch at all |

**That makes the rung a real decision** — a cheap working now, or spend work and materials to reach
three times as much in total — and it is exactly the decision a copper mine will want.

**Mechanically it is the fauna escapement floor, upside down.** Fauna has a floor you may not draw
below; here the rung **lowers** the floor:

```
floor      = (1 − recovery_fraction(position)) × capacity
reachable  = max(0, stock − floor)
```

> ⛔ **AND THE §6 FLOOR TRAP MUST NOT ARRIVE BY A DIFFERENT DOOR.** `plan_standing_upkeep.md` §6
> records it: a rung raised the ceiling, `floor = floor_fraction × K` climbed with it while the herd
> stayed the size it was, the build's own eligibility gate read the room above that floor, and a tame
> begun on its floor **never completed at any crew size** — building faster starved you sooner.
>
> **The guard here is a rule, not an accident: NO RUNG ON EITHER BRANCH RAISES `capacity`.** A
> forestry rung raises `regrowth_rate` only; an extraction rung lowers the floor only. `capacity` is
> written once by worldgen and never moves, so the floor can only ever go **down** — the safe
> direction. Conservationism is about the rate of renewal, not about there being more forest.

### 4c. ONE PAYOFF BLOCK SERVES BOTH BRANCHES, and that is what makes the zero hold by arithmetic

A rung carries one `extraction_payoff` — `yield_per_worker_turn`, `recovery_fraction`, and
`regrowth_multiplier` — and all three interpolate on position. A **forestry** rung raises the
multiplier and leaves recovery at its ceiling; an **extraction** rung raises recovery and leaves the
multiplier at 1.0.

**There is deliberately no second block and no branch check.** The multiplier scales *the deposit's
own* regrowth rate, and rock's is zero, so `0 × anything` is still zero: *stone's rate is zero*
survives as arithmetic rather than as a rule someone has to remember not to break. A branch-keyed
payoff would have made that rule breakable by a config edit.

### 4d. THE FLOOR RUNG MUST BE BARE-HANDED, OR THE ECONOMY CANNOT START

**There is a bootstrap here and it is easy to miss.** A felling kit wants a haft, a haft is wood, and
wood comes from felling. The same loop exists for stone: `earthmoving` and `stone_dressing` each cost
3 wood + 2 bone.

So `forestry:deadfall` and `extraction:gathering` are **workable with no kit at all**, and that is
load-bearing rather than a convenience. It is the same argument `materials.json` makes for
`hand_working`: a bare-handed rate is what refuses nothing, and a zero would be a refusal branch the
sim does not have.

---

## 5. Where the stock comes from — and this is the gold/silver/copper answer

**The source reads an endowment on the tile and does not care who put it there.**

Worldgen writes, per tile, zero or more records of the shape *"this tile holds X units of material M
with characteristics C and regrowth rate R."* A wooded highland holds two. The extraction source
reads that record and never learns where it came from:

- **Wood** — derived from what is already on the map; a forest biome holds timber. **No new
  worldgen.**
- **Stone** — derived from terrain; elevation already decides highland and cliff, and elevation is
  the sole terrain authority (`docs/plan_elevation_authority.md`). **No new worldgen.**
- **Metals** — *not* derivable, and that is the minerals arc's job: a generator that places copper
  here and gold there, unevenly.

**Keeping that seam clean is the whole point of this arc.** It makes the minerals arc a **worldgen
slice plus config rows** rather than a re-design.

### 5a. The endowment gates how far up the ladder a tile goes

**This is what makes "you cannot quarry just anywhere" true without a second mechanism.** A tile whose
stone endowment is a scatter supports `extraction:gathering` and nothing above it. A tile whose
endowment clears a configured threshold supports `quarry`. A placed ore body supports `mine`.

That also dissolves the placed-versus-derived question rather than answering it: **terrain gives you
rung-0 stone nearly everywhere, and a placed endowment is what lets you climb above rung 0.** Both,
which is what the early/late split actually is — *stone for spear points is not scarce; stone for
roads is.*

**The scarcity is in the RATE, not in the material.** A band knapping points never needs a quarry; a
faction paving roads cannot do without one.

**As shipped, two rungs state a placement rule and the other three state none.**
`extraction:quarry` asks `min_deposit_capacity` **100** of the tile's own stone capacity and
`forestry:coppice` asks **70** of its wood capacity, both through the `forage::rung_site_refusal`
seam a `sow` already resolves through — so the command's rejection, the labor arm's gate, the lesson
gate (§3a) and any readout cannot drift into disagreeing about which ground takes a working.
**The two free floors state `null` deliberately**: what refuses a crew on bare ground is that the
ground holds no deposit at all, and a floor of ~0 admits every tile and reads as a placement rule
while being none — which `validate_site_requirement` rejects outright.

**Each number is a READING OF ITS OWN TABLE rather than a chosen round figure, and the two tables are
read differently because they are shaped differently.** On stone, capacity and rate are correlated by
construction — the rate-0 bodies are the large ones — so 100 sits in the gap the deposits table
leaves between its two populations (every renewing scatter at or under 70, every finite body at or
over 120) and ranks the rows exactly. On wood the two are independent, so 70 is ranked on what the
rung **buys**: a coppice doubles `regrowth_rate`, and the logistic peak `K × r / 4` splits the
shipped wood table either side of it with a gap and no overlap. Both derivations live on their own
rung's `_comment_site_requirement` in `intensification_ladder.json`, with the measured figures;
**re-read the owning one before moving either number**, and note that capacity is a looser proxy on
wood than on stone.

### 5b. Stone's axes finally get a reader, and they were authored for this

`materials.json` gives stone `hardness` and `workability` and records that nothing reads either. The
two cases above **are** those two axes: a spear point wants stone that **knaps** (`workability`), a
road wants stone that **bears load** (`hardness`). And since a source states the characteristics of
what it yields, a streambed pays knappable flint and a quarry pays building block — one generic
material, two readings, exactly as a mammoth pays tough hide and a hare supple.

> **THIS DOES NOT REOPEN THE DEFERRAL `materials.json` RECORDS.** What was deferred is an
> **improvement** reading an axis — *"a better stone makes a better road."* That stays deferred: a
> road takes 20 stone, any stone. What gains a reader is a **recipe**, which is the ordinary graded
> path every other material already uses. The distinction is that a recipe has grades and an
> improvement does not.

**And the metals need nothing new here.** `materials.json` already states that metal will be **one**
material with named varieties (copper, bronze, iron) gated by a furnace's temperature ceiling, not a
material per metal. A deposit naming its material and characteristics is the shape a flora yield edge
already has.

---

## 6. What it costs

**A woodcutter is a mouth that is not gathering.** These sites pay **no food whatsoever**. Every hand
on a quarry is a hand not feeding the band, and labor is already allocated per activity
(`plan_standing_upkeep.md` §2.2). That, and not the walk, is what makes wood cost something.

**The working belongs to a CAMP, like a patch — not to nobody, like a road.** A road follows no one
and is free to leave, which is why it belongs to no camp; a quarry you walked away from is a quarry
you lost. That is what puts an extraction site on the move-or-stay decision, and it is the one place
this arc deliberately does **not** copy `RungBranch::Route`.

**Holding it costs upkeep like everything else** — work per turn, interpolating on position
(`plan_standing_upkeep.md` §2.7), drawn from a keeping pool the two branches share. Without it a
working's position never falls and **a quarry is free to hold for ever**, which is the one thing an
improvement may not be if it is to weigh on move-or-stay.

> ⛔ **BUT THE MATERIAL HALF IS A PILE, NOT A RATE — and this section said otherwise until the
> implementation tested it.** The draft above read *"work per turn **plus materials per turn**"* and
> gave the quarry a standing wood bill. That is wrong on §2.7's own test: props, ramps and sleds are
> timbered once **as the face is opened**, so they go *into* the working and stay there, which is a
> build pile. A road's stone rate is real by the same test for the opposite reason — **re-dressing a
> road is not re-laying it**.
>
> So neither branch settles a standing material, and `validate_upkeep` **rejects an
> `upkeep.materials` on a deposit rung outright** rather than leaving the key parseable. A rate with
> no settle pass would load, validate, publish a demand and be paid by nobody — the *looks-live-but-
> isn't* failure `route:paved_road` actually shipped for one slice. Giving a branch a rate later
> means writing the settle pass **first** and then deleting that check on purpose.
>
> It also puts the two branches in the right order for a faction that starts with nothing: **you must
> have worked a wood before you can open a quarry.**

**Two of the tools already exist.** `earthmoving` (pick and spade) and `stone_dressing` (maul,
wedges, dressing hammer) ship today, tuned for 300–800 unit jobs, and are bound to the route rungs
alone (`equipment.json` `_comment_road_tools`). Those are quarry tools that currently only build
roads; they want their **rung binding widened**, not new items minted. Forestry has no kit and wants
one.

---

## 7. The readouts, and which one you get is decided by the RATE

**A renewable deposit warns that you are over-cutting.** That is the existing sustainable-versus-
actual income breakdown (`sustainable_yield`, `docs/plan_intensification.md`) pointed at a new source
— no new readout.

**A finite deposit warns that it runs out.** `turns_remaining = reachable / current take rate`, a
**forward projection** and never a trailing average, on the food-arrivals rule
(`.claude/rules/core_sim/`, the arrivals arc).

**Which sentence a source publishes is decided by `regrowth_rate > 0`, not by its branch.** A flint
scatter and a quarry are the same branch and publish different warnings, which is §3's callout
restated where it is observable.

---

## 8. Out of scope, deliberately

**Stone tools.** Every cutting tool in the game is bone: `spears` and `clubs` both read bone
`density`, and no shipped recipe takes stone at all. `recipes.json` calls bone *"the scarce one by an
order of magnitude"* — 0.0012–0.003 per biomass against hide's 0.006–0.022, so one spear is about
eleven turns of hunting. **Spear points are bone because stone had no producer**, so knappable stone
is the obvious relief for a scarcity the file already names in its own words.

That is a change to the shipped tool roster and a larger question than this arc. **Land the producers
first, then re-cut the roster with the bone economy in front of you.** Recorded here so it is not
re-derived, and so nobody reads the bone spear as a considered choice.

**Gold's value.** Gold is a metal and the varieties model holds it — a variety with poor hardness,
which is true and useful, since a soft metal makes bad tools. What is missing is that gold's *worth*
lives in ornament and exchange, and the game has no system for either. Not this arc's problem, and
not a blocker: the deposit works whatever the metal is for.

**A road reading stone quality** (§5b), and **the containment- and quality-scaling** that
`plan_standing_upkeep.md` §4.9 item 12 defers for the same reason.

---

## 9. Open items

- **Does rung-0 stone cost hands?** **Decided: yes** — picking and knapping is work, and *a hand on
  stone is a hand not on food* should be true from turn one. Cheap and widely available, but not
  free. Recorded as an item rather than a bare decision because it makes stone tools cost more
  **labor** than bone tools while costing less **material**, which is a gameplay claim rather than a
  tuning detail. **If it plays tedious the fix is a bigger yield per turn, never a free mechanism.**
- **Forestry's kit does not exist and the natural one is an axe.** With stone tools out of scope
  (§8) it would be bone-hafted, which sits oddly. The alternative is to ship forestry
  kitless until the roster is re-cut, at the cost of the branch having no gear decision at all.
- **Whether a worked-out quarry should leave anything behind.** A depleted deposit is a source whose
  reachable stock is zero; whether that source is removed, or stands as a visible spent working, is a
  readout question this arc can answer either way.

---

## See Also

- `docs/plan_standing_upkeep.md` — §2.7 the material half, §4.9 item 12 (which shipped both
  materials with no producer), §6 the open item this arc closes and the floor trap §4b guards
  against.
- `docs/plan_crafting_and_materials.md` — a material is generic with characteristic axes; the
  varieties mechanism the metals rely on.
- `docs/plan_intensification_ladder.md` — the rung ladder both new branches sit on.
- `docs/plan_elevation_authority.md` — the terrain authority stone's derived endowment reads.
