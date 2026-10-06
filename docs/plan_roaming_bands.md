# Plan: Roaming Bands — the band that follows its herd

Status: **design, manual-first.** The authoritative spec for
[#251](https://github.com/rwalker123/falcon/issues/251). Manual entry: §2a "Start of Game — Nomadic
Default", the **Follow the herd** bullet.

## Why this doc exists

The design pillar says *move*, *stay* and *fork* must all be live choices with real advantages
([`docs/plan_band_fission.md`](plan_band_fission.md) owns *fork*). The rooted column is built with
care — scarce river ground for Fields, pens fixed at the fence, a granary that stays where it was dug,
belief that pins a people to its dead. The roaming column has no payoff at all. Today moving only
costs: a long move sheds what the packs cannot hold, a Field left behind goes wild, a pen left behind
empties. Staying nomadic reads as a phase you have not grown out of, not a strategy you chose.

The arc was opened from a playtest observation: **a mammoth is food too big to carry.** About 60–100
person-loads of meat. Nobody hauled one home; they moved to the kill.

## Decided

**1. A kill within the band's reach is made in camp.** `band_work_range` (2 tiles) is the camp. A
hunt inside it behaves exactly like a far hunt with a walk of zero: the crew keeps the **whole
carcass**, it lands in the larder the same turn, and nothing is hauled — so no sled wear. Beyond 2
tiles the work-party path is unchanged: porters ferry the carcass home and the sled carries it.
Shipped with this doc; as-built in `.claude/rules/core_sim/fauna.md`.

**2. Locality is decided per kill.** Herds move. One turn's kill can be a tile from camp and the
next one four, so every kill is classed by where the herd stands that turn. A hunt that started
local is not local forever.

**3. Larder rot applies, and is intended.** The fix moves the rot from the range to the larder; it
does not remove it. Fresh meat keeps 4 turns (`flesh`, `demographics_config.json`), so a small band
that lands a mammoth still loses most of it — honestly, as spoilage on the Food line, not as waste at
the kill.

**4. Preservation extends this; it is not the payoff.** The drying rack is already designed
(`docs/plan_civilization_steps.md` §Step 5): it travels with the band and lifts `flesh`'s rot line,
so it is the nomad's storage. It makes a windfall worth more. The reason to roam is the herd itself.

**5. The core of the roaming life: the band follows its herd and kills when it needs to.** A band
that lives beside a herd needs no stockpile, because the herd *is* the stockpile. That is the
payoff the rooted column cannot match — food that walks with you.

## The model (proposed)

### The verb — follow a herd

A band can be told to **follow** a herd. While following, the band's camp keeps the herd within
`band_work_range`: each turn the herd ends beyond it, the band steps toward it at
`band_move_tiles_per_turn`, exactly as a `move_band` order walks. Every kill is then a camp kill
(decided 1). Stopping is cancelling the follow; the band stays where it stands.

It is a **standing order on the band**, not a hunt row. The band's Hunt row on that herd is what kills
and at what policy; the follow is only *where the camp is*. Following without hunting is legal (a band
tracking a herd it is taming, say), and hunting without following is today's game.

The retired `follow_herd` command (`.claude/rules/core_sim/fauna.md` → "Follow is a RETIRED
command") was a one-shot teleport with rewards. This is not it: no teleport, no grant — just a camp
that moves.

### Why a band can keep up

A migratory herd loiters for 14–26 turns within `loiter_radius` 2 of an anchor, then migrates its
route at one tile a turn (`fauna_config.json`, mammoth). A band walks one tile a turn. So a following
band camps through a loiter with every kill in camp, and walks the migration leg alongside. A big herd
on its route becomes a **seasonal round** — the circuit the manual's "Seasonal routes" bullet
promises, emerging from the herd's own route rather than from a map overlay.

### Following moves are short, so nothing is shed

A long move sheds what the packs cannot hold (`.claude/rules/core_sim/band-carry.md`). A follow step
is one tile — inside `move_ferry_reach_tiles` (3) — so the band ferries everything across. That is
the existing rule, and it is the right answer here: a people moving a day's walk at a time with a
herd brings its camp along. **It is a property of the existing ferry rule, not an exemption**, and it
means a roaming band's goods are as safe as a rooted band's.

### What a roaming band gives up

Nothing new — the costs already exist and already fall on whoever leaves:

- **A Field goes wild, a pen empties** — neglect decay (`forage.rs`, `fauna.rs`).
- **A granary stays on its tile** (designed, `plan_civilization_steps.md`) — behind you, spoiling.
- **The dead stay where they were buried** — belief is a place, and walking away drags morale.

So the choice is honest in both directions without a new term: the rooted band has the dense,
storable food; the roaming band has the herd.

### Why a roaming band forks

One herd taken at *Sustain* feeds only a handful (manual §Wildlife & Hunting). A band that lives off
one herd therefore caps out at what that herd renews. Past that it either pushes the herd harder
(and watches it decline), or **splits** and sends the new band after a second herd — the fission
verb (#508) already exists for exactly this. That is how a hunting people spreads: not by founding,
by following more herds. The roaming life is where *fork* is the natural move rather than a crisis.

### No mobility meter

`SedentarizationScore` drives no simulation today — only the HUD, the Telling and a prompt
(`sedentarization.rs`). A matching "mobility" score would be a number that pushes a target.
**Don't build one.** The roaming payoff is paid *in kind* — whole carcasses, no walk, goods intact,
fresh ground — and the score keeps reading what it reads. If the score ever starts steering the sim,
revisit this.

## Open questions

1. **"Kills when it needs to."** Today a Hunt row kills at its policy's rate whatever the larder
   holds, so with whole carcasses and 4-turn meat a following band over-kills and watches it rot.
   Should a following band's hunt be **need-paced** — kill when the larder will run short of meat
   before the next kill could land — rather than rate-paced? That is closer to how herd-followers
   actually lived, and it is what makes "the herd is the stockpile" literal. It is also a new policy
   behaviour, so it wants a decision before a slice.
2. **Which herds can be followed?** Every herd, or only those that range far enough to make following
   matter? Proposal: every herd — a resident deer herd within 2 tiles already needs no following, so
   the verb is simply idle there.
3. **What does the band leave on the ground it walks?** Migratory herds already wear their corridors
   into trails (`.claude/rules/core_sim/routes.md` → "Game trails"). Does a band's own walking bank
   route work the same way, so a seasonal round becomes a road the people made? Proposal: yes, by the
   same rule as any traffic — check whether band movement already does before designing anything.
4. **Pastoral nomads.** A tamed herd already drifts toward its band (`drift_to_owner`): there the herd
   follows the band. Is that half done as-is, or does a pastoral band want a follow of its own (the
   band follows its herd to pasture)? Proposal: as-is until a playtest says otherwise.

## Slices (proposed)

1. **Camp kill** — decided 1–2. Ships with this doc.
2. **Follow a herd** — the standing order, the per-turn step, the wire field, the client verb on the
   herd and the band. Waits on open question 2.
3. **Need-paced hunting** — if open question 1 says yes.
4. **Band traffic wears trails** — if open question 3 finds it missing.

## See Also

- `docs/plan_band_fission.md` — *fork*; the split a roaming band reaches for when one herd is not
  enough.
- `docs/plan_civilization_steps.md` — spoilage, the drying rack, the granary; belief as a place.
- `docs/plan_hunt_through_combat.md` — what it takes to bring a mammoth down at all.
- `docs/plan_settlement_population.md` — the rooted column and `SedentarizationScore`.
- `.claude/rules/core_sim/work-party.md` — the far hunt and the porters.
- `.claude/rules/core_sim/band-carry.md` — the ferry reach and the long-move shed.
