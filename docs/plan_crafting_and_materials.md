# Crafting & Materials

**Status:** design settled, nothing built.
**Realizes:** issue #494 (slice 3 of #491). **Forces:** issue #520.
**See also:** `docs/plan_early_game_labor.md` → "Equipment / TOE" (the depletion cliff this is the
other side of), `.claude/rules/core_sim/equipment.md` (the TOE as built),
`docs/plan_hunt_through_combat.md` §4.8.

Equipment today is **start-stocked and not craftable**, on a two-tier durability cliff — so running a
kit dry is a one-way door and the band steps down to bare hands permanently. `plan_early_game_labor.md`
calls equipment depletion "the pacing dial of the first act", and a dial only reads as one once there
is something on the other side of it. This is that other side.

---

## 1. What a material is

**A material is generic. There is no "deer hide" — there is Hide.**

A material declares its own **characteristic axes**, its **craft**, and whether it can be worked
bare-handed. A *source* — an animal, a plant, later a deposit — states the characteristics of what
*it* yields. Nothing in the model is fauna-shaped or flora-shaped: the same yield edge hangs off all
three.

```json
// materials.json
"characteristic_bands": [ { "name": "low",  "from": 0.00 }, { "name": "fair", "from": 0.30 },
                          { "name": "high", "from": 0.55 }, { "name": "top",  "from": 0.80 } ],

"hide":   { "craft": "tanning",       "characteristics": ["toughness", "suppleness"], "hand_workable": true  },
"fibre":  { "craft": "weaving",       "characteristics": ["fineness", "strength"],    "hand_workable": true  },
"bone":   { "craft": "bone_working",  "characteristics": ["density", "length"],       "hand_workable": true  },
"metal":  { "craft": "smithing",      "characteristics": ["hardness", "working_temp"], "hand_workable": false,
            "varieties": {
              "tin":    { "hardness": 0.10, "working_temp": 0.05 },
              "copper": { "hardness": 0.25, "working_temp": 0.20 },
              "bronze": { "hardness": 0.55, "working_temp": 0.30 },
              "iron":   { "hardness": 0.75, "working_temp": 0.70 },
              "steel":  { "hardness": 0.95, "working_temp": 0.85 }
            } }
```

### A characteristic VECTOR, never a quality scalar

Mammoth hide is `toughness top · suppleness low`; a hare pelt is the reverse. A sled reads toughness
and cordage reads suppleness, so those are not two rungs of a quality ladder — **there is no "best"
hide**, only the right one for the job, and spending the mammoth hide on a basket is a mistake the
player can make. A single quality scalar would rank them, name a winner and delete that decision.

### Varieties are NAMING, not materials

If copper and tin are both just *metal*, an alloy recipe cannot name its ingredients. The physics
separates them, but specifying bronze as *"9 parts metal at hardness .20–.35 plus 1 part under .15"*
is unreadable to the author and worse to the player.

So a material may declare **varieties**: named presets over its own axes. The author writes `copper`,
the player reads "Copper", a batch displays as its nearest variety — and the material stays one
generic thing with one craft and one tool axis. It is also what collapses the metal tech ladder into
one mechanism: there is no separate Bronzeworking and Ironworking, only Smithing plus a furnace whose
temperature ceiling decides which varieties are reachable.

### Bands: categories on screen, exact numbers underneath

A source states an **exact** reading (`toughness 0.55`). The panel and the recipes speak in **four
bands** — `low · fair · high · top`. Three consequences, and all three are load-bearing:

1. **It is the merge rule.** Two arrivals landing in the same band become one batch, so a band hunting
   deer for two hundred turns holds one pile of hide rather than two hundred. Without this the store
   grows without bound.
2. **The exact value survives the merge** as the batch's weighted average, and crafting resolves the
   output's quality from it — so two `high` hides are not interchangeable, and a recipe wanting *high
   toughness* pays out differently for `.58` than for `.79`.
3. **A band name says how MUCH of the characteristic, never how good the material is.** `tough top ·
   supple low` is a mammoth hide: right for a sled, wrong for cordage.

---

## 2. Stock

A band holds a material as **batches** — a quantity plus its characteristics — in `LocalStore`,
alongside provisions, fodder and trade goods. `balance_supply_networks` is commodity-generic, so
materials already pool with same-faction bands inside supply reach and not beyond it.

> **This is a real change to `LocalStore`**, which is `BTreeMap<String, Scalar>` today and is shared
> with the food economy. It is the price of "not all hide is the same": a single pooled average would
> be flat and free, and would silently drag a mammoth hide down to a deer hide the moment the two met.

**Every yield edge carries materials.** A species/plant config is **authored per row and tunable** —
it names a material and states the characteristics *it* gives:

```json
"red_deer": {
  "hunt_yield": {
    "provisions_per_biomass": 0.050,
    "materials": [
      { "material": "hide", "per_biomass": 0.010,
        "characteristics": { "toughness": 0.55, "suppleness": 0.60 } },
      { "material": "bone", "per_biomass": 0.002,
        "characteristics": { "density": 0.80, "length": 0.35 } }
    ]
  }
}
```

The flora edge is the same shape on `flora_config.json`, and a deposit's will be too.

---

## 3. Recipes

**One structure: `inputs → outputs`, both lists of THINGS, where a thing is a material or a piece of
equipment.** Bronze and steel are not special cases.

```json
"bronze": {                                    // material from materials
  "craft": "smithing",
  "inputs":  [ { "material": "metal", "variety": "copper", "amount": 9 },
               { "material": "metal", "variety": "tin",    "amount": 1 } ],
  "outputs": [ { "material": "metal", "variety": "bronze", "amount": 10 } ],
  "needs_tool": { "stat": "working_temp", "at_least": 0.30 }
},

"sled": {                                      // equipment from a material
  "craft": "tanning",
  "inputs":  [ { "material": "hide", "amount": 18, "reads": "toughness" } ],
  "outputs": [ { "equipment": "sled", "amount": 1 } ],
  "grades": {
    "coarse":   { "when": 0.00, "effects": [ { "stat": "hunt_carry", "equipped": 34.0 } ] },
    "standard": { "when": 0.45, "effects": [ { "stat": "hunt_carry", "equipped": 40.0 } ] },
    "fine":     { "when": 0.75, "effects": [ { "stat": "hunt_carry", "equipped": 46.0 } ] }
  }
}
```

**A recipe reads ONE characteristic** (`reads`), which is what makes "no best hide" real. `when` is
the seam where the continuous reading quantises into a discrete output grade.

> **Continuous in, discrete out — and this is a constraint, not a preference.**
> `EquipmentEffect` names the value a stat **takes** and has no representation for a multiplier
> stacking on something else; that is what makes *"flat until expiry, then a step down"* structural
> rather than remembered (`equipment.md` → "The three rules"). So a continuous reading may never
> scale a resolved stat. It selects a grade, and the grade declares absolutes.

Grade is fixed at craft time and never moves, so it is **not a taper**.

---

## 4. Equipment gains a COUNT

Today `BandEquipment` records **condition, not count** — one wear number per item id, with an absent
entry reading as a *full* item. That was correct for exactly as long as nothing could make a second
spear.

Crafting ends it. Equipment stocks in **batches like materials do**: `{count, grade, wear}`. Ten
spears made together from the same hide with the same tool wear together; the next ten are their own
batch.

Three things follow:

- **"You don't own a Loom yet" is count 0.** No invariant to invert, no absent-means-full trap.
- **Idle stock does not rot.** Wear tracks the ten that went out, so stockpiling ahead of a hard
  season is a real strategy rather than a slow loss.
- **A party can be PARTLY equipped** — 16 hunters and 10 spears. Capping the party was considered and
  **rejected**: the other six still go, and they hunt bare-handed. That is a take/combat problem
  rather than a crafting one and is tracked as **#520**.

---

## 5. Crafts (knowledge) and tools

**One craft track per material** — Fibre → Weaving, Hide → Tanning, Bone → Bone-working, Metal →
Smithing. They sit in `IntensificationKnowledgeState` beside the ladder's existing five.

| | scope | lifetime | answers |
|---|---|---|---|
| **Knowledge** | faction-wide | permanent | *what can be made at all* |
| **Tools** | band-local | consumable, worn per use | *how well this band makes it* |

That is the ladder's own `"I know how"` versus `"this band can"` split, which `equipment.md` already
states for husbandry.

### Crafting is the fourth teacher

Hunting teaches Herding and Penning, foraging teaches Cultivation and Seed Selection, keeping a pen
teaches Foddering — and **crafting teaches its own crafts**. The lesson is charged on the **same
quantum as the wear**: per item completed, so the thing that consumes the tool and the thing that
teaches the craft cannot drift. What is being made decides what is learned.

> **The land decides what a band is good at.** A band that lives by hunting wears out spears and
> learns knapping; a gathering band wears out baskets and learns fibre-work; a band that cannot reach
> bone never advances Bone-working. Specialisation emerges from where you are standing rather than
> from a tech menu.

### Tools are EARNED, never a prerequisite

You cannot make a Loom at the start — Weaving must be learned first, and it is learned *by weaving
bare-handed*. So there is no opening move where everyone builds tools first, and a tool run dry drops
the band back to the rate the game already ships at rather than into a spiral.

- **A tool bounds ONE material** and grants nothing outside it — the shape `max_body_mass` already
  runs on.
- **Its payload is its `effects` list**: quality ceiling, material efficiency, bench speed. Present
  effects apply; absent ones do not. Speed alone is the weak one — no land is better at it, so a
  speed-only tool never touches the move/stay decision.
- **Output grade is `min(material reading, tool ceiling)`.** Fine flax with no loom still makes a
  standard basket.
- **A material's tool is never made from that material.** The bone awl costs fibre and hide, never
  bone, or you would need the scarce material to make the thing that stretches it.
- **A tool is gated on the crafts of what it is MADE FROM, never on the craft it unlocks.** Otherwise
  metal needs a crucible, the crucible needs metalworking, and nothing can start. The ladder then
  falls out with nothing added: work clay → learn Clay-working → make a crucible → copper and bronze
  become workable → learn Smithing → build a furnace → iron and steel come into reach.

### Organic degrades; mineral is gated

You can twist cordage by hand and scrape a hide with a sharp rock, so fibre, hide and bone fall back
to a bare-handed rate. **You cannot work metal without heat** — there is no bare rate — so that
material requires its tool outright. It is **one flag on the material** (`hand_workable`), not a
prerequisite on every recipe that touches it.

> **The sim needs no *"you cannot craft that"* branch** — a zero rate refuses it exactly as
> `max(0, attack − defense)` refuses a hunt. **But the panel must say so out loud**, greyed and
> reasoned, never a bench that silently does nothing. That is the same split `KitRoster.kit_offer`
> already makes for a snare against a Red Deer: no branch in the sim, an explicit reasoned refusal in
> the client.

---

## 6. Tiers

Quality tiers moved here from #493 deliberately: a tier is **unobtainable in a game with nothing that
can craft it**.

- **Upgrades nest inside an item; they are not separate items.** A kit's `uses` list keeps naming
  `spears`, never `flint_spears`.
- **What is SHARED stays on the item, what the MATERIAL buys sits on the tier.** A spear is a thrown
  weapon whatever it is tipped with (`dispersion` on the item), while `attack` and
  `starting_durability` are what the material changes.
- **Tiers are knowledge-gated, and flint ships known**, so nothing is locked at the start and the
  gate has a real job the day bronze exists.
- **Flint IS today's spear** — `starting_durability 100`, `attack 20`, verbatim. The migration is a
  pure re-homing and not one number changes value, which preserves `equipment.md`'s "the shipped
  opening is unchanged" invariant.
- **Ship no bronze row in the config.** An unreachable tier is the same objection #493 used to defer
  tiers here, and `SubsistenceSection.equipmentConfigJson` publishes the whole config to the Workbench,
  so it would be *visible* dead content. Cover tier switching with a test fixture; bronze becomes a
  config row the day #325 lands.

**This is what forces the equipped rates out of `labor_config.json`.** It owns the equipped haul rates
(`40.0` hunt, `8.0` forage) and several telemetry sites read them as the equipped reference. Once
grades and tiers declare their own values, the equipped side lives on the tier,
`labor_config.json`'s numbers become the **no-equipment baseline**, and every reference-rate reader
resolves through the item table.

---

## 7. The client — a Materials & Crafting panel

Prototype: the artifact linked from #494. It is drawn in `HudStyle.gd`'s palette at real proportions.

**Its own panel, launched from the Band/City panel header** — an `_make_icon_button` beside the cycler
and dock chooser, the same builder the collapse toggle uses. The header is subject-independent chrome,
so one button serves a band page and the faction page, and the band zone's 300px budget is untouched.
From a band page it opens on that band; from the faction page it opens on the last band loaded, which
`BandPanelController` already holds (`render_faction` never touches `_panel_band`).

**The band is named by a picker flanked by the shipped cycler** — `HudWidgets.build_field_key` +
`build_option_picker`, plus `BandCityPanel`'s own `◀ n / N ▶`. The arrows walk the roster; the
dropdown jumps. Both are dead today, there being one player band.

**Left rail — what you have.** One group per material, the group header carrying its craft track; one
row per batch, showing the amount and its characteristic bands. Nothing else: no provenance, no
per-turn rate, no catalogue of materials the world does not yet contain.

**Right — the bench and the ledger.** The bench states what is being made, its progress, the crew
stepper and what it is teaching. The ledger is one table in three groups: the band's kit, bench tools,
and recipes that make stock.

### Make IS the assignment

Pressing **Make** puts that recipe on the bench and draws idle workers onto it; the bench's `− 2 +`
stepper changes the crew; the running row reads *On the bench* with its button spent. **One job at a
time**, so the panel never has to explain a queue.

**So there is no Crafter role card.** Scout and Warrior are standing roles with nothing to point at;
crafting always has a subject, so it is staffed like a worked source. That also sidesteps a measured
constraint: the WORKFORCE zone reads 326px against a 275px box and its column split sits *exactly* on
`band_panel_preview`'s levelness floor, so a third role card would have to be paid for by re-authoring
that split.

### Readout rules

- **The life meter is a fuel gauge, not a performance meter.** A spear at 34% is exactly as deadly as
  one at 100%. So the tier/grade is a discrete chip that does not move until it flips to *Bare hands*,
  and the bar beside it reads in **turns left**, never percent — a single percentage bar would draw
  the taper the model does not have.
- **A refusal names its number.** *"Short 4.9 bone"*, never *"cannot craft"*; and *"Not needed yet"*
  reads differently from a shortage, because one is a shrug and the other is a problem.
- **Sorted by urgency** — worn first, untouched last, dimmed rather than hidden.

### Deferred

Where a batch came from and what it earns per turn belongs in a **popover off the material row**, the
idiom `DisclosureController` already provides for Food / Morale / Growth / Trade / Kit. **It must be a
popover, not an inline expansion** — `band-readouts.md` records that as a correctness rule: expanding
inline grew a label *after* its zone had picked a height tier, and the `clip_contents` host silently
sliced the rows beneath it.

---

## 8. Open

- **#520** — the hunt take must resolve a partly equipped party. Blocked on counts landing here.
- **#325 / #326** — minerals. Wood, stone, clay and metal have no producer until they land, so this
  arc ships with the three organic materials and bronze stays out of config.
