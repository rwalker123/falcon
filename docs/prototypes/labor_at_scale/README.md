# Labor at Scale — UX prototypes

The interactive prototypes for `docs/plan_labor_at_scale.md` (#842), kept here so they outlive the
design session. Each file is one artboard of the "Labor at Scale prototypes" canvas
(https://claude.ai/artifact/MuahXbtPmS1kap4H1XQUdN — private to its owner).

**They are the canvas's source, not standalone pages.** Each `.dc.html` loads `./support.js`, the
canvas's own runtime, which is not in this folder — opened directly in a browser they will not run.
Read them for layout, wording and the staffing logic in each file's `<script>`; view them live on the
canvas. Every number in them is a placeholder chosen to make the screen react, not a tuned value.

**Every artboard copies a real panel from a fresh `ui_preview` frame and changes only what the design
changes.** A prototype that draws its own layout instead is a defect — the rows here reuse the game's
existing row, rung button, rung card and sheets.

## The shared control

| File | What it is |
|---|---|
| `work-strip.js` | The work strip — rung mark, `⌃` button and the "Take it to…" rung card, floor glyph, `⚠`, crew number and crew bar — written once and loaded by every artboard below that shows a work row. The game builds it once too (#877). |

## Artboards, by level

| Level | File | Artboard | Issues |
|---|---|---|---|
| Band | `Main.dc.html` | Band panel — Work tab, staffed by the game (People bar, Scouting range, rows with the work strip, Dig stone's keep-on-hand chip) | #871 #872 #874 #875 #877 #878 |
| Band | `HarvestWorking.dc.html` | Clicking a work row — the source's sheet, already working | #871 #877 |
| Band | `AssignHarvesters.dc.html` | Assign harvesters | #871 #877 |
| Band | `AssignHunters.dc.html` | Assign hunters | #871 #877 |
| Band | `Farm.dc.html` | A farm — keeping it up, and sowing it | #872 #877 |
| Band | `Stone.dc.html` | Stone — keep on hand | #874 |
| Band | `Crafting.dc.html` | Materials & Crafting — several orders at once, each with a crafter limit (and its "The tribe" mode) | #873 #864 |
| Band | `Faction.dc.html` | Faction page — one People bar, idle across bands | #878 |
| Tribe | `Tribe.dc.html` | Tribe panel — the tribe replaces its bands; drill into one | #862 #863 |
| Tribe | `SharedWork.dc.html` | Starting a shared work — the tribe on the Band: picker | #863 |
| City | `City.dc.html` | City panel — quarters, trades (Auto), great works, rations | #865 #866 #867 #868 |
| Nation | `Nation.dc.html` | Nation page — tax (Auto, ceiling), aid between members, national works, the leader's reach | #869 #853 #870 |

`canvas.json` is the canvas layout (board positions and titles).
