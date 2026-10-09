# Plan: Roaming Bands — the band that migrates with its herd

Status: **design, manual-first.** The authoritative spec for
[#251](https://github.com/rwalker123/falcon/issues/251). Manual entry: §2a "Start of Game — Nomadic
Default", the **Follow the migration** bullet.

## Why this doc exists

The design pillar says *move*, *stay* and *fork* must all be live choices with real advantages
([`docs/plan_band_fission.md`](plan_band_fission.md) owns *fork*). The rooted column is built with
care — scarce river ground for Fields, pens fixed at the fence, a granary that stays where it was dug,
belief that pins a people to its dead. The roaming column had no payoff. Moving only cost: a long move
sheds what the packs cannot hold, a Field left behind goes wild, a pen left behind empties. Staying
nomadic read as a phase you had not grown out of, not a strategy you chose.

The arc was opened from a playtest observation: **a mammoth is food too big to carry.** Nobody hauled
one home; they moved to the kill.

## Decided

### A kill within the band's reach is made in camp — shipped

`band_work_range` (2 tiles) is the camp. A hunt inside it behaves exactly like a far hunt with a walk
of zero: the crew keeps the **whole carcass**, it lands in the larder the same turn, and nothing is
hauled, so no sled wear. Beyond 2 tiles the work-party path is unchanged: porters ferry the carcass
home and the sled carries it. **Locality is decided per kill**, from where the herd stands that turn —
herds move, so a hunt that started local is not local forever. As-built:
`.claude/rules/core_sim/fauna.md` → "EVERY HUNT KEEPS THE WHOLE KILL".

### Larder rot applies

Meat in the larder rots when its shelf life runs out (`flesh` keeps 4 turns): a kill is eaten from
for 4 turns and whatever is left then rots, whole (`docs/plan_civilization_steps.md` §Step 5). That
is permanent, not a stopgap.

### Migration mode is for MIGRATORY herds only

Every hunt already follows its herd — the hunters go where the animals are. That is not this. This is
the **whole band relocating with a migrating herd**, physically, across the map. A resident herd
(deer, boar) never leaves its few tiles, so there is nothing to relocate for; only a herd that
migrates (`migratory: true` in `fauna_config.json` — mammoths, steppe runners, marsh grazers,
reindeer, wild horses) can be followed.

Two phases, both the herd's own (`RoamState` in `fauna.rs`):

- **The herd is in its seasonal grounds** (`Loiter`, 14–26 turns within `loiter_radius` 2 of an
  anchor). The band **camps in the middle of the herd** and is an ordinary band — it forages, works
  other hunts, does whatever local work it would do anywhere. The only difference is where it stands:
  every hunt of that herd is a camp kill.
- **The herd migrates** (`Migrate`, one hex a turn to the next anchor). **The band moves with it.** A
  band also walks one hex a turn (`band_move_tiles_per_turn`), so it keeps pace.

**The band keeps up independently of hunting.** Staying near the herd is the mode's own movement, not
a side effect of a kill: the herd moves, the band moves.

### A band in the herd never makes a far kill

The herd moves first each turn and the band moves right after it, both one hex, so a band camped in
the herd stays in the herd, through migration too, and **every kill is a camp kill**. The only far
case is the start: a band that turns the mode on while several hexes off has to catch up, and a
migrating herd moves as fast as it does, so it closes the gap when the herd stops at its next
grounds. **While it catches up the hunt is an ordinary far hunt** — porters carry the kills home to
the moving camp, exactly as today. No carcass waits on a tile and the band never walks to a kill.

### Band movement rules are unchanged

A migration leg is a long move, and a long move sheds what the packs cannot hold, exactly as any long
move does (`.claude/rules/core_sim/band-carry.md`). **No new movement mechanism**: migration mode
issues the same movement the band already has. Food is first in the packs, so a band that just killed
loses nothing of the meat by leaving; what a long migration costs is the heavy goods.

### When a kill coincides with the herd leaving: go immediately

A camp kill is already in the larder, and the larder walks with the band (food first in the packs), so
leaving at once costs no meat. **The band follows immediately.**

### Migration mode is a choice on the hunt, not its own order

Following a herd only makes sense while hunting it: a band that follows without hunters walks across
the map and never eats from it. So migration mode is **one box on the hunt order**, never a band verb.
Prototype: `docs/migration_mode_ux_proposal.html`.

- **Turning it on.** The Assign hunters sheet of a **migratory** herd carries a **Move camp with the
  herd** box, where the WORK PARTY section sits; a resident herd's sheet has no box. Everything else
  on the sheet is unchanged: a band still catching up hunts with porters, so the sheet's work-party
  forecast stays true until it arrives.
- **Turning it off.** The hunt row on the Work tab carries the same toggle: off, the band stays where
  it is and its hunters keep working the herd as an ordinary hunt. Cancelling the hunt ends the
  following with it.
- **The hunt row** says where the band stands, one line: *Camped in the herd. Kills land in camp.* /
  *Moving with it · next (x, y)* / *Catching up · N hexes behind*. While it
  catches up, the row's existing work-party lines run beneath it. The herd card's worked line adds
  *moving with the herd*.
- **The map.** The band token wears a 👣 badge and the followed herd a dashed ring; the herd's
  next-step arrow stays drawn while the band follows it; the band's own travel line shows its next
  step.
- **A Move order ends it.** Moving the band clears the box on its hunt, or the next turn would pull it
  straight back to the herd. A band follows one herd at a time: ticking the box on one hunt clears it
  on the band's others.

### Hunting by need

A band in migration mode hunts **when it needs meat**, not at its policy's fixed rate: it kills when
the larder will run short of food before another kill could land. Between kills the band's workers do
other work. A mammoth is indivisible and a whole one at once is the point — need-pacing decides
*when*, never *how much of one*.

### Drying is learned, and it is what makes a herd growth-sustaining

**The numbers.** A mammoth is 800 biomass × `hunt.provisions_per_biomass` 0.06 = **48 food**. A
30-person band eats 30 × 0.16 = **4.8 a turn**, so a mammoth is 10 turns of food if nothing rots. But
`flesh` keeps 4 turns, so the band eats about 19 of the 48 and the other 29 rot on the fourth turn:
**today a mammoth feeds a 30-person band for about 4 turns**, and a smaller band for the same 4.

**That is the intended opening, not a dead end.** A band following a migration is not living on
mammoths alone — it forages and hunts other game like any band. The rot is the lesson: the storage
rule already designed in `docs/plan_civilization_steps.md` §Step 5 ("Rot teaches storage") makes
**food lost to spoilage while a surplus sat** the practice signal that teaches the drying rack, and a
mammoth kill is the biggest such signal in the game. So the loop is:

1. The band kills a mammoth, eats what it can, and most of it rots — while its workers do other
   things.
2. That rot teaches drying, on the knowledge ledger, by practice.
3. After a few kills the band can dry meat. The rack lengthens `flesh`'s shelf life, so a mammoth becomes
   many turns of food, and a migrating herd becomes **growth-sustaining**.

**Drying is learned, never granted at start.** The rack is #708's (the storage branch); this arc
consumes it. A mammoth herd taken at *Sustain* pays one animal every ~7 turns (MSY 120 ÷ body 800) —
about 7.2 food a turn, enough for ~45 people — so once drying lets the band keep what it kills, one
herd carries a band well past its starting size.

### Why a roaming band forks

When a band outgrows what its herd renews at *Sustain*, it either pushes the herd harder (and watches
it decline) or **splits** and sends the new band after a second migrating herd — the fission verb
(#508). How large a band one herd carries depends on the herd: a mammoth herd with drying carries a lot.

### The mobility score

Roaming has to achieve something, or nobody will do it. Two layers:

**1. Skills learned by doing.** Migrating with a herd is practice on the same knowledge ledger the
intensification ladder uses (learn a thing by doing the thing below it). Candidates:

| Skill | What it does | Built on |
|---|---|---|
| **Herd lore** | see a migrating herd's next seasonal ground, so the band can pre-position | the herd's `route` anchors |
| **Travois / pack animals** | more carry per worker, so a long migration sheds less | the existing carry model (`carry.rs`) |
| **Drying** | lengthens `flesh`'s shelf life | #708 |
| **Drives** (e.g. a bison jump) | more kills at once from a large herd | the hunt fight |

**2. The score.** It reads the band's standing on that roaming track and how far it has travelled with
its herds — the roaming counterpart to `SedentarizationScore`. It feeds the manual's **Cultural
Diffusion (Nomadic)** victory (§Victory Conditions). That link is real, not decorative: range is the
observing band's sight and contact is found inside the sight sweep (`.claude/rules/core_sim/connections.md`),
so a band that walks long migrations meets more peoples, holds more ties, and culture spreads over
ties (`docs/plan_contact_and_logistics.md` §Settled by #530). Moving well is how a nomadic people wins.

## Open items

- **The score's formula** — the weights of skills against distance travelled, and the victory
  threshold. Opening values are Workbench levers, settled by playtest.

## Slices

Sub-issues of the arc, #251.

1. **Camp kill** — shipped with this doc (#796).
2. **Migration mode** (#797) — the band's standing order on a migratory herd: camp in the herd while
   it loiters, move with it while it migrates, catch up with porters running. The wire field and the
   box on the hunt order (§Migration mode is a choice on the hunt).
3. **Hunting by need** (#798) — the need-paced trigger for a band in migration mode.
4. **The roaming skills** (#799) — Herd lore, travois, drives, on the knowledge ledger. Drying is
   #708's.
5. **The mobility score** (#800) — the readout, and its input to Cultural Diffusion.

## See Also

- `docs/plan_band_fission.md` — *fork*; the split a roaming band reaches for when one herd is not
  enough.
- `docs/plan_civilization_steps.md` — spoilage, "Rot teaches storage", the drying rack; belief as a
  place.
- `docs/plan_contact_and_logistics.md` — contact, ties, and culture over them.
- `docs/plan_hunt_through_combat.md` — what it takes to bring a mammoth down at all.
- `docs/plan_settlement_population.md` — the rooted column and `SedentarizationScore`.
- `.claude/rules/core_sim/work-party.md` — the far hunt, the porters, the load at the source.
- `.claude/rules/core_sim/band-carry.md` — the ferry reach and the long-move shed.
