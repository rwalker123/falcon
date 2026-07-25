---
paths:
  - "core_sim/src/combat/**"
  - "core_sim/src/combat_config.rs"
  - "core_sim/src/data/combat_config.json"
  - "core_sim/tests/predators.rs"
---

<!-- Extracted verbatim from core_sim/CLAUDE.md lines 3194-3429.
     Routing table and shared vocabulary live in core_sim/CLAUDE.md.
     Regenerate with scripts/split_claude_md.sh -->

# Combat & Casualties (Predators arc — Phase 0)

The **combat subsystem** and the first live consumer of the long-inert **Warrior** role. Authoritative
design: `docs/plan_predators.md`. Phase 0 stands up the *seam* — a dangerous hunt is just a *fight*, so
the casualty math lives in a first-class module, never as a bespoke hunt formula.

- **`core_sim/src/combat/` — a first-class module that imports NOTHING from fauna/labor/population**
  (dependency inversion is one-way: fauna/labor → combat, never back). It owns the algorithm and the
  neutral types; domains adapt *into* it. Types: **`CombatStats { attack, defense, range }`** (the
  shared per-unit body — the *same* struct describes a wolf and a human), `RangeBand` (`Melee|Ranged`,
  persisted-enum, accepted/ignored by the placeholder resolver), `Posture`, `FightPayload { sides,
  terrain, seed }`, `Force { id, posture, contingents }`, `Contingent { kind, count, profile }`,
  `FightOutcome { results, victor, disengaged }`, `ContingentResult { force, kind, killed, wounded }`.
  Callers describe **composition** (contingents), never an aggregate "power" scalar.
- **`resolve_fight(&FightPayload, &CombatTuning) -> FightOutcome` — a PURE function, no RNG** (the
  `seed` is accepted and reserved for future variance). The placeholder model: `power(side) = Σ count ×
  attack`; `victor` = strictly-higher power (`None` on a tie); each side's **total losses depend on
  ENEMY power relative to OWN power** (`losses = (power_enemy / power_self) × lethality`, clamped to
  headcount) — so **more/stronger defenders → FEWER own casualties**; the kill/wound split is
  `killed_frac = incoming_per_defender / (incoming_per_defender + own.defense)` with
  `incoming_per_defender = power_enemy / count_self` — so **more defenders → more wounded** (a bigger
  party thins each blow) and **higher own defense → more wounded** (the equip-to-shift-severity lever).
  `disengaged` iff the loser's losses exceed `disengage_fraction` of its headcount. `range`/`terrain`/
  `posture`/`seed` are accepted and **ignored** (reserved for the real resolver). Casualties stay
  **f32** (whole-unit quantization is a later refinement). **Note:** the design prose spelled the
  split's denominator `power_enemy / count_enemy`; with the enemy a single beast that leaves the split
  constant regardless of party size — contradicting the equip-to-shift-severity thesis — so the seam
  divides by the **defenders** (`count_self`), which is what makes a stronger party move severity.
- **Strength ≠ danger — `SpeciesDef` splits STRENGTH from BEHAVIOUR.** A big `attack` does **not** make
  a thing dangerous to you (a mammoth is immensely strong but will never raid your camp); danger is
  **DERIVED, never stored**. Four fields, all `#[serde(default)]` (so every existing species is
  byte-identical):
  - **STRENGTH** — `combat: CombatStats { attack, defense, range }` (open-ended, human = 1; defaults to
    `{ attack 0, defense 1, Melee }`).
  - **BEHAVIOUR** — `aggression: f32` (0..1 — P(initiates a raid unprovoked)) and **`ferocity: f32`**
    (0..1 — P(fights back when hunted, vs flees)). `aggression` is *"does it start it"*, `ferocity` is
    *"does it finish it"*.
  - **`diet: Diet` (`Herbivore|Carnivore`)** — the trophic knob. Inert this phase.
  Danger is composed client-side, never a stored scalar: **hunt-danger ≈ `attack × ferocity`**,
  **camp-threat ≈ `attack × aggression`** (Phase 1). `diet` and `aggression` are **inert this phase**
  (Phase 1 consumes them: prey-derived carrying capacity + the predator-raid trigger). `FaunaConfig::validate`
  enforces `combat.attack ≥ 0` finite, `combat.defense > 0` finite (a denominator), `0 ≤ aggression ≤ 1`,
  `0 ≤ ferocity ≤ 1`.
- **The graduated roster** (`fauna_config.json`, playtest dials): mammoth `attack 8 / ferocity 0.9`
  (strong AND fights back → deadly), aurochs `4 / 0.7 / aggression 0.1`, boar `1.5 / 0.6` (cornered
  and mean), steppe/marsh grazer `2.5 / 0.4`, elk `2 / 0.4`, horse `2 / 0.3`, seal `1 / 0.3`, down to
  deer `0.8 / 0.15` and gazelle `0.3 / 0.05` (skittish — flee, cost almost nothing). Rabbit / fowl /
  snow hare / grouse / catfish stay fully default (attack 0 → harmless).
- **The hunt-path adapter (`advance_labor_allocation`, the Hunt arm's wild-take branch).** After a wild
  hunt take resolves, a herd that will **fight back** (`attack × ferocity > 0` — a hunt only faces the
  animal's attack to the extent it doesn't flee) turns on the party: the adapter builds a
  `FightPayload` — **band side** (`Aggressor`, one `"person"` contingent, `count = the hunters assigned
  to THIS herd`, profile = the creatures roster's `person`), **animal side** (`Defender`, one contingent
  `count = 1.0`, profile = `species.combat` with **attack scaled to `attack × ferocity`**) — with a
  rollback-stable `seed = map_seed ^ tick ^ herd-id
  hash` (reserved/unused by the resolver but a real value), and applies **only the band side's** outcome
  (the take already removed the animal's biomass; applying its casualties too would double-count).
  `killed` comes out of the cohort's **working-age** bracket via
  `PopulationCohort::apply_combat_casualties` (the `death_fraction` seam's combat twin — a net-new
  mortality path); `wounded` is **computed and surfaced but mechanically inert this phase** (recovery is
  a later slice). A `CommandEventKind::HuntDanger` (`"hunt_danger"`) feed line fires when casualties
  occur — label names the **species** ("The Thunder Mammoths hunt cost 2 lives"), detail carries the
  **fractional** `killed=<k> wounded=<w> species=<s>`. **The hunting party answers the danger with its
  OWN strength** — the hunters' bare-hands `person` profile today; their equipment (TOE, deferred) will
  compose into that profile with no rework here.
- **The expedition-hunt adapter is the SAME seam, but bloodier** (`advance_expeditions`, the
  `ExpeditionPhase::Hunting` arm — after `expedition_take_biomass`, inside the `in_reach` guard so it
  only fires on an engagement turn). A hunting **expedition** builds the identical fight — its party
  (`count = the party workers`) vs the beast (at `attack × ferocity`) — and applies **only the band side's** `killed` to the
  expedition's own `PopulationCohort`, narrating on the same `HuntDanger` feed. **It fights at a scaled
  lethality:** `tuning.lethality *= combat_config.expedition_danger_multiplier` (**1.5**) — a detached
  party is far from home, unsupported and tired, so the same beast costs it more than it costs a
  resident band. The **resolver is untouched** and the resident-band path uses the base tuning; only the
  expedition adapter scales it. A deferred **general combat-modifiers layer** (proximity / fatigue /
  supply) will supersede this flat multiplier and add a *home-advantage discount* to local hunts.
- **Warrior stays inert in Phase 0.** Warriors are a band-wide **standing guard** (border/camp patrol),
  not a hunting escort — they do **not** mitigate hunt danger (the hunting party answers that itself,
  via its own equipment). Its labor arm remains a no-op branch. **Its first live consumer is the
  Phase 1 predator-raid path** — a carnivore with `aggression > 0` raiding a band, band as Defender.
- **The wire carries the RAW combat components, not a stored `danger`** — danger is derived, so
  `HerdTelemetryState` publishes `attack` / `defense` / `ferocity` / `aggression` (append-only, from
  `snapshot/subsistence.rs`'s `herd_snapshot_entries` via `fauna.species_by_display`) and the **client
  derives** hunt-danger (`attack × ferocity`) and camp-threat (`attack × aggression`) itself. There is
  **no stored `danger` scalar** (a big `attack` alone is strength, not danger) and **no TileState danger
  field** — the overlay projects the derived per-herd value onto tiles client-side. `HerdTelemetryEntry`
  is untouched. **Client follow-up:** the native reader + band-panel/overlay display of the four
  components are a separate client-dev task.

Tests: `core_sim/src/combat/mod.rs` unit tests (even fight, 5:1, adding-defenders mitigation + wounded
shift, defense→wounded shift, determinism, zero-attack → zero casualties); `core_sim/tests/predators.rs`
(a mammoth hunt costs working-age lives with a killed/wounded split; a **rabbit** hunt — ferocity 0 —
costs nobody; ferocity scales hunt-danger; config-validation rejections including ferocity); `core_sim/tests/expedition_hunt.rs`
(a hunting expedition takes casualties against a mammoth; the `expedition_danger_multiplier` scales
losses). **Note:** `hunt_trip_forecast` does not yet model casualties, so `the_raid_forecast_matches_a_real_party_run`
hunts a harmless species — wiring casualties into the launch forecast is a Phase-1+ follow-up.

See Also: `docs/plan_predators.md` (the whole arc), "Fauna & Wild Game" (the `SpeciesDef` table + the
Warrior role), "Population & Demographics" (the `death_fraction`/bracket seam casualties apply at).

## Predation (Phase 1a) — carnivore herds

**A predator is an ordinary `Herd` whose food layer is *other herds* (prey) instead of the per-tile
`GrazeRegistry`** — the trophic transpose of the grazer model. `SpeciesDef.diet` (`herbivore` |
`carnivore`, inert since Phase 0) is now consumed at the **one K seam**. Design: `docs/plan_predators.md`
§ "Phase 1a"; **1b (the raid trigger + Warrior) is a later PR — untouched here.**

- **Diet-branched carrying capacity** (`fauna::ecological_carrying_capacity`): an herbivore is unchanged
  (graze path); a **carnivore**'s `K_pred = Σ_prey prey_sustainable_flow(prey) / prey_per_biomass` over
  the prey herds in its **prey-sensing disk**, ignoring graze / `fodder_per_biomass` /
  `herd_density_gain` entirely. `prey_sustainable_flow(B, cap, r)` is `graze_sustainable_flow`'s exact
  logistic shape against the **prey herd's own** `regrowth_rate`, read at the prey's **current** (drawn-
  down) biomass — so a thinned prey base lowers `K_pred` (the coupled feedback). `SpeciesDef.prey_per_biomass`
  (`#[serde(default)]` 0.0, inert for herbivores) is the carnivore analog of `fodder_per_biomass`;
  `validate` requires a carnivore's `prey_per_biomass > 0` **and** `combat.attack > 0` (a predator that
  clears no defense is incoherent).
- **Prey = herbivore herds whose `defense ≤ predator.attack`** — the pure `attack ≥ defense` rule
  (idea 7: a wolf's `attack 3` never counts a mammoth's `defense 12` or an aurochs' `6`), no `is_prey`
  flag. This is **one definition, three readers**: the clearance comparison lives in the single
  `attack_clears_defense` helper, and "which herds are prey candidates" (herbivore, with cached
  `defense`) lives in `build_prey_index`/`PreyDatum` — so the carnivore `K`, `advance_predation`, and
  the prey-derived spawn count never carry a second, divergent predicate. The **prey-sensing disk**
  (`predators.prey_sense_radius`, default **4**) is deliberately **wider** than a graze footprint (0–1)
  because prey are sparse points; a graze-sized disk would contain zero prey most turns and snap `K→0`.
  It was **widened 3 → 4** because a pack roaming transiently out of prey range got `K→0` and was
  clamped away — measured ~45% of packs despawning within 10 turns at radius 3; the 61-tile disk (vs 37)
  cuts those transient-zero-prey despawns (the deeper fix is Phase-2 prey-pursuit).
- **The cross-herd borrow.** `ecological_carrying_capacity` runs inside `advance_herds`' `iter_mut`
  loop, so it cannot read the other live herds. `advance_herds` snapshots a **prey index**
  (`Vec<PreyDatum>`, one per herbivore herd) in an immutable pass *before* the loop — start-of-turn prey
  biomass, the same one-turn lag graze `K` has — and passes it in.
- **`advance_predation`** (new system) mirrors `advance_herd_grazing` with prey herds as the layer: each
  carnivore demands `prey_per_biomass × biomass` and draws it from the in-range prey herds it can clear,
  **proportional to each prey herd's available biomass** (above the functional-response floor
  `predators.predation_escapement_fraction × prey.cap`, default **0.15**) — the taper that makes a pack
  take less as prey thins and stop before zero. Index-based over the herd Vec (predator `i` mutates prey
  `j`, always distinct), deterministic in registry order. **Credits no food to anyone** (a wolf's dinner
  is abstracted biomass). Registered in Logistics **after `advance_herd_grazing`, before
  `advance_graze_regrowth`**.
- **Idea 6 falls out of shared machinery:** a pack with no prey in its disk gets `K_pred → 0`,
  `regrow_biomass`'s `clamp(0, cap)` drives its biomass to 0, and the existing extinction `retain`
  despawns it — no game, they leave/die.
- **The dedicated predator pass** (`spawn_predators`, called from `spawn_initial_herds` **after both
  prey passes**): same winner-collection → shuffle → greedy-`min_spacing`-spaced placement as
  `spawn_short_range_game`, drawing **only carnivore** species, so predators are rare and do **not**
  consume the `abundance.max_total_game` prey budget. Predator ids carry the `pred_` prefix. Carnivores
  are filtered **out** of the herbivore short-range pool *and* `repopulate_fauna` immigration
  (`game_species_for_biome` is herbivore-only; `carnivore_species_for_biome` is its twin), so a predator
  seeds **once** and does not respawn.
- **The count is PREY-DERIVED, not a fixed cap** — `max_packs` is **gone**. Each carnivore species
  carries a **target** = `round(eligible_prey_herds × SpeciesDef.prey_ratio)` — its own prey set (every
  herbivore herd its `attack` clears, map-wide, counted once both prey passes have run) × its own ratio,
  because a predator population is *defined by* its prey base. A winning tile seats one of the carnivore
  species hosting its biome **whose per-species target is not yet met** (uniform among them, as before),
  and the loop ends when every species' target is met or the winners are exhausted. For the single
  shipped carnivore this is "place up to `target` packs", but it generalizes to N predators (a future
  big cat with its own `prey_ratio`/prey set). **Placement can cap below the target on prey-rich maps**:
  measured on the 6-seed standard sweep the wolf target is 10–11 (prey herds 97–108, `prey_ratio 0.10`)
  and packs come in `11 / 6 / 10 / 11 / 10 / 11` — five hit the target, seed 2 is placement-starved (the
  shipped `min_spacing 6` + low per-biome probabilities cannot seat 11 well-spaced packs there). The
  lever to close that gap, if wanted, is `min_spacing`/`predators.per_biome`, not the target.
  (Guard: `predators::the_predator_count_scales_with_the_prey_base`.)
- **Spawn is PREY-GATED — no stranded pack** (idea 6 at spawn). A winning tile only seats a carnivore
  species whose **prey-derived `K` at that tile reaches its `min_spawn_biomass()`** (the low end of its
  `biomass` range) — the gate lives in `spawn_predator_group_at`'s candidate filter, measured with
  **`carnivore_k_at(pos, attack, prey_per_biomass, prey_index, prey_sense_radius, …)`**, the *same*
  position-parameterized formula the live per-turn `K` reads (`carnivore_carrying_capacity` now
  delegates to it), so the spawn gate and the running `K` can never diverge (DRY, and the prey rule
  stays the single `attack_clears_defense`). Without it a pack drops onto an isolated tundra tile with
  no game in reach, gets `K ≈ 0`, and despawns almost immediately (observed live: a wolf stranded on
  Tundra with the nearest game several tiles off across water). On a prey-sparse map this places
  **fewer** than the derived target — correct: a viable pack near game beats a stillborn one. **Measured
  a no-op on the standard-map sweep** (prey is dense enough that every winning wolf-host tile already
  clears the gate — counts still `11 / 6 / 10 / 11 / 10 / 11`); it only bites where prey is genuinely
  sparse. (Guard: `predators::every_spawned_predator_lands_where_the_prey_can_feed_it`.)
- **The wolf row** (`fauna_config.json`, playtest anchors): `Grey Wolf Pack`, `diet carnivore`,
  `combat { attack 3, defense 3 }`, `prey_per_biomass 0.3`, **`prey_ratio 0.10`** (a pack per ~10 prey
  herds), `regrowth_rate 0.15`, `ferocity 0.8`, `aggression 0.6` (set now, **inert until 1b**),
  `husbandry_ceiling wild`, hosting savanna / temperate-forest / boreal / highland. `attack 3` fixes its
  prey set for free. `FaunaConfig::validate` requires a carnivore's `prey_ratio` finite `> 0` (a `0`
  seats no packs). Plus a **`predators` config block** (`per_biome` / `min_spacing` /
  `predation_escapement_fraction` / `prey_sense_radius`, all validated — no `max_packs`).
- **The prey-sense ring is on the wire** — `HerdTelemetryState.preySenseRadius:uint` (append-only,
  strictly after `aggression`; `snapshot/subsistence.rs`'s `herd_snapshot_entries`) =
  `fauna.predators.prey_sense_radius` when the herd's species is a **carnivore** (`def.diet ==
  Diet::Carnivore`, resolved via `species_by_display`), else **0**. So `preySenseRadius > 0` is BOTH the
  client's "this is a predator" signal AND its view-ring radius: a carnivore's graze-range ring is
  meaningless (it hunts other herds), so the client draws a prey-sense "view" ring of this radius
  instead; a herbivore reads 0 and keeps drawing its `grazeRangeRadius` ring. **Client half (a separate
  task):** the native reader + the view-ring render. Guard:
  `snapshot::tests::herd_snapshot_reports_prey_sense_radius_for_carnivores_only`.

## Predator raids (Phase 1b) — the raid trigger + the Warrior goes live

**A carnivore with `aggression > 0` within `predators.raid_radius` of a resident band raids its camp**,
and the band is defended by its **Warriors** — the long-inert Warrior role's **first live consumer**.
Design: `docs/plan_predators.md` § "Phase 1b". `SpeciesDef.aggression` (set on the wolf row since
Phase 0, inert until now) is the trigger; `diet` gates it to carnivores.

- **`advance_predator_raids`** (`systems/labor.rs`, sibling of the Phase-0 hunt-danger adapter) runs in
  the **Population stage right after `advance_labor_allocation`** (so warrior counts and band positions
  are current) and **before `advance_population_migration`**. For each `ResidentBand` × each carnivore
  herd, if the pack's **raid attack `= combat.attack × aggression > 0`** (aggression 0 ⇒ no raid) and it
  is within `predators.raid_radius` (odd-r `hex_distance_wrapped`) of the band, it builds a
  `FightPayload`, resolves it through the neutral `combat::resolve_fight`, and applies **only the
  band/defender side's** casualties — **working-age only this phase** (`cohort.apply_combat_casualties`,
  one mutation per band; multiple raiders are additive/order-independent). `wounded` is surfaced in the
  feed but mechanically inert (recovery is a later slice, exactly as in Phase 0).
- **The band side is TWO contingents, and that is load-bearing** — the placeholder resolver clamps a
  side's losses to *its own* headcount, so a `count 0` side takes ZERO losses. A warriors-only band side
  would therefore give a **0-warrior band zero casualties** — the exact inverse of "an under-guarded
  band costs it people". So the fight is: **Aggressor** = one representative of the pack
  (`count 1.0`, profile `combat` with `attack = attack × aggression`) — a Phase-1b simplification that
  keeps `power_enemy` modest so a handful of warriors can meaningfully cut the loss ratio (the whole pack
  engaged would make every raid a massacre); **Defender** = the band's **Warriors** (`count =
  workers_on(Warrior)` clamped to working-age, profile = the creatures roster's `person` — the armed
  defenders that add power and shift the split toward wounded) **plus the exposed populace**
  (`count = min(predators.raid_exposure, working_age − warriors)`, profile `{ attack 0, person.defense,
  person.range }` — the unarmed folk that can die and dilute the blow but add no offense). The seed is
  rollback-stable and distinct per (predator, band) pair (both the herd id and the band entity hashed).
- **Config** — two `#[serde(default)]` levers on `PredatorConfig` (`fauna_config.json` `predators`
  block), both playtest dials: **`raid_radius`** (`2` — how close the pack must be to raid; its own
  lever, deliberately tighter than the `prey_sense_radius` disk) and **`raid_exposure`** (`4.0` — how
  many working-age folk stand exposed beyond the warriors, bounding a raid to a skirmish).
  `FaunaConfig::validate` rejects `raid_radius < 1` and a non-finite/`<= 0` `raid_exposure`.
- **Feed** — `CommandEventKind::PredatorRaid` (`"predator_raid"`, server label "Predator raid") fires
  each casualty-causing raid turn: label names the **species** (`"A {species} raid cost {N} lives"`),
  detail carries the fractional `killed`/`wounded` + `warriors` + `species`. Edge-gating a repeated raid
  to one line is deferred to Phase 3.
- **The Warrior labor branch stays a no-op in the labor pass** (warriors do no per-worker yield) but is
  **no longer inert overall** — the warrior head-count is consumed here as the band's defending
  contingent. Tests: `core_sim/tests/predator_raid.rs` (unguarded band bleeds + narrates; warriors cut
  losses; no raid from a herbivore or an out-of-range carnivore; aggression scales lethality;
  determinism) + the two `fauna_config` rejection tests.

---

