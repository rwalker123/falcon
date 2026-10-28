# Band orders — UX prototypes

The interactive prototypes for the band orders in `docs/plan_labor_at_scale.md` ("Band orders — the
chief", #860), kept here so they outlive the design session. Each file is one artboard of the
"Chief's stone order (#860)" canvas (https://claude.ai/artifact/9Q2tH8AsmU9hL5Y5epTdz7 — private to
its owner).

**They are the canvas's source, not standalone pages.** Each `.dc.html` loads `./support.js`, the
canvas's own runtime, which is not in this folder — opened directly in a browser they will not run.
The chief's portrait is the canvas's upload of `assets/portraits/leader_placeholder.png`
(`/_blob/…`), so it shows only on the canvas. Every number in them is a placeholder chosen to make
the screen react, not a tuned value.

**Every row and sheet copies the game's.** The Work tab rows use the shared work strip
(`work-strip.js`, the same file as `docs/prototypes/labor_at_scale/`), and each source sheet copies a
fresh `ui_preview` frame of today's sheet.

| # | File | Artboard |
|---|---|---|
| 1 | `Main.dc.html` | Before a chief — the Work tab, stone dug per site with its Max as the crew, as today (the same board as `labor_at_scale/Main.dc.html`) |
| 2 | `ChiefStone.dc.html` | Stone with a chief — the Stone sheet as his order, *Crew it yourself*, *Hand it back*; the Band picker swaps in today's Diggers sheet for a band with no leader |
| 3 | `ChiefWood.dc.html` | Wood with a chief — keep on hand across the stands he leaves to grow back; the amber line when they regrow too little |
| 4 | `ChiefOrders.dc.html` | The orders together on the Work tab — one pool of hands, today's rows under each order, the *↳ Also* line for a mixed patch |
| 5 | `ChiefFood.dc.html` | The food order — the Food sheet, and a hunt the order is working |
| 6 | `ChiefFodder.dc.html` | The fodder order — what the pens eat, and a hay meadow the order is cutting |

`canvas.json` is the canvas layout (board positions and titles).
