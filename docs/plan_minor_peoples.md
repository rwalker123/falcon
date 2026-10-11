# Plan: Minor Peoples — more, smaller peoples from turn 1

Status: **design, first draft.** The spec for
[#809](https://github.com/rwalker123/falcon/issues/809), under the arc
[#808](https://github.com/rwalker123/falcon/issues/808).

## Why this doc exists

The early game is contact: ties, culture drift, break-aways (#702), trade, raids. With only a handful
of major peoples spread 20 tiles apart, a people can go a long time with no one to meet. Minor peoples
put neighbours on the map from turn 1, so there is someone to absorb, fight and trade with. Most will
be assimilated or destroyed. Nothing hardcodes that outcome, and a minor people can grow into a power.
The nearest 4X comparison is barbarians and city-states, but the model here is simpler than either.

## The rule, in one line

**A minor people is a smaller start, not a kind of faction.** It has the same rules, the same verbs
and the same seat as any other people. The only difference is the opening: fewer people and a thinner
loadout. Nothing in the sim stores whether a people is minor once the world is built.

A purist band that breaks away (#702), or a grievance split (#284), already produces exactly this: a
small new people under the same rules. Minor peoples therefore arise in two ways, at the start and by
fission, and the same machinery handles both.

## As-built — what exists (verified 2026-10-11)

- **Seats.** The sim knows seats and never knows who fills them (`.claude/rules/core_sim/factions.md`
  → seats). `FactionControl::{Human, Ai}` exists, but nothing in the sim branches on it.
- **One `sim_ai` process per seat.** The launcher reads the `seats.roster` event and spawns one
  `sim_ai --faction N` for every faction except 0 (`launcher/src/main.rs` `Session::reconcile`). It
  does this at every roster change, so a mid-game break-away **already gets a brain** under the
  launcher. `scripts/run_stack.sh` starts no rivals.
- **A seat nobody commands starves.** No labour is ever assigned and the sim has no fallback; the
  bench's Pass seat starves on every seed.
- **Mid-game peoples.** `grow_faction_roster` (`core_sim/src/systems/independence.rs`) adds a people
  at runtime with no cap. **No mechanism ever removes one.** An emptied people stays in the roster
  and the turn queue indefinitely.
- **Changing hands.** Defection (#512) moves bands one at a time, driven by misery, across a live tie
  and into open borders. The leftover band changes faction when it drops below
  `settle.parent_min_workers`. **No whole-people join exists.**
- **Culture.** There is a 15-axis vector per band. Contact drift pulls tied bands toward each other,
  weighted by headcount and damped by the receiver's Purist lean. `faction_trait_average` rolls bands
  up into a per-people average. **There is no people-to-people culture distance.**
- **Starts.** One start profile (`late_forager_tribe`: 1 band of 30) is applied identically to every
  people. Placement keeps starts at least `faction_start_min_separation` (20) apart. The map sets a
  ceiling on how many starts fit (Tiny 3 … Huge 17).
- **Personality.** Starting culture comes from **the place**: the province layer plus a small per-band
  modifier. Nothing is set per people. AI personality is a `sim_ai` profile (`forager`, `rover`) that
  the launcher never passes, so every rival runs the default.
- **Victory.** `victory_tick` scores every people. Only Hegemony is enabled.

## Decided

### Who plays them: a brain per seat, exactly like a major

Minor peoples need nothing new. Each one is a seat, and the launcher already gives every non-human
seat a `sim_ai` process, including seats born mid-game. **There is no sim-side fallback.** A fallback
would make the sim tell a commanded people apart from an uncommanded one, which is exactly the
distinction it must never make.

**Cost, as measured.** The server pays per *occupied* seat on the turn thread: about **+3.1 ms
`run_turn` and +2.5 ms snapshot capture per seat** on a Standard map
(`.claude/rules/core_sim/turn-profiling.md`). Four majors plus eight minors is 12 seats, roughly
+60 ms per turn over a single seat. That is well within what a turn-based game can absorb. Each
`sim_ai` process decodes the full frame and runs its specialists every tick, but **its CPU and memory
have never been measured.** That is the number that could make "one process per seat" too expensive.

**Hosting several brains in one process is allowed and changes nothing in the sim.** It is still one
connection, one claim and one seat token per people. Either layout looks the same from the server. We
consolidate only if the measurement in Slice 4 says to; until then one process per seat stays,
because it is the shape the launcher already supervises.

### Starts: an archetype sets size, a personality sets character

Two independent config catalogs, each drawn per people at world build:

- **Start archetypes** set the opening strength: band count and size, loadout, knowledge tags,
  inventory, and placement separation. `major` and `minor` are just two entries. The human seat takes
  the first archetype. The archetype is used at world build and then forgotten. **No minor flag goes
  into `SimState`.**
- **Personalities** set the character: a lean added to each starting band's culture seed, per axis
  (for example `syncretic_purist` +0.8, or `passive_aggressive` +0.5). Majors and minors draw from the same catalog, so any
  people can be any personality. The draw is seeded (`map_seed ^ faction_id`), so a seed reproduces
  its world.

A people born by fission draws neither. It inherits its bands as they stand, culture included, which
is already the as-built rule.

**The AI side of personality is the launcher's job.** The launcher picks a `sim_ai` profile per seat
with the same seeded draw and passes `--profile`. The sim never learns which profile a seat runs. See
Open items for whether a brain should read its people's culture.

### Placement: majors first, minors into the gaps

Archetypes are placed in catalog order. Each one has its own minimum separation, measured against
every start already placed. Majors keep their 20 and minors take a smaller number, so minors land in
the space between majors. The existing relaxed pass (keep the minimum distance as large as possible
and warn) applies per archetype.

**Capacity.** The `FactionCapacity` query returns a ceiling and a pre-selected default for each
archetype, computed from that archetype's own separation over the space the earlier archetypes left
free. New Game asks for a count per archetype: "Rivals" and "Minor peoples". As now, a request over
the ceiling is clamped, never refused.

### Peaceful absorption: joining, with kinship as the first pathway

**Joining** is a whole people changing hands without a fight. It is built as a shared hand-over plus
a list of **pathways**, where each pathway is a test that one people joins another. This draft
specifies one pathway. Others, such as overwhelming culture, are added as further tests that call the
same hand-over.

**Kinship (the first pathway).** People A joins people B when all of the following hold for
`joining.kinship.turns` consecutive turns:

1. **A long tie.** At least one live tie exists between a band of A and a band of B
   (`ConnectionLedger::tie_is_live`).
2. **Converged culture.** The culture distance between A and B is at most
   `joining.kinship.max_culture_distance`. Distance is the **largest per-axis gap** between the two
   people-weighted averages (`faction_trait_average`), the same metric divergence already uses.
3. **A is the smaller people,** counted by headcount. This sets the direction of the join; it is not a
   separate rule.
4. **B's borders are open,** the same consent defection asks of the receiver. A people that does not
   want new members closes its borders.

If any condition fails for one turn, the clock resets to zero. It is a whole-turn accumulator, not a
smoothed average. The clock is per ordered pair and lives in `SimState`, so it is saved.

**What drives it is already in the game.** Contact drift pulls tied bands together, the larger
people's pull dominates, and a Purist lean resists. A small accepting people beside a big neighbour
converges and joins. A small purist people holds out and must be fought or left alone. Personality
therefore decides who gets absorbed, with no special-case code.

**The hand-over** reuses defection's path. Every band of A moves to B, along with A's parties, roads
and improvements (`follow_the_band_to_its_new_people`). Knowledge merges into B's ledger the way a
defecting band's does. Ties are untouched. One event is recorded: "*A* joined *B*". A then has no
bands and goes extinct (next section).

**The player sees it coming.** A people within reach of joining shows a risk line, following the
`cultureBreakAwayRisk` precedent: who it would join and how many turns of kinship are left. This
appears on both sides, the people about to be absorbed and the one about to absorb.

### Extinction: an empty people leaves the table

Minor peoples mostly do not survive, so the roster needs a way out. **A people with no bands and no
parties is extinct.** This happens at the end of the Population stage, and the cause can be joining,
starvation, defection or, later, conquest. An extinct people:

- **keeps its id and name** (the Telling, events and saves refer to them) but is flagged extinct;
- **leaves the turn queue and releases its seat.** The next `seats.roster` event drops it, and the
  launcher's existing reconcile then stops its brain;
- **is not scored** by `victory_tick`;
- **never comes back.** A later break-away mints a new id.

If the human's own people goes extinct, that is the end of the human's game. What the client shows
for that is a client slice, not a sim rule.

### Victory, and telling minor from major

**Minor peoples count for victory exactly like anyone else.** No marker separates minor from major,
on the map or in any panel: a small people simply looks small. A minor people that grows is a power,
and the game never tells it otherwise.

## Config levers

Opening values are guesses to be tuned in playtest, sized against the 30-person major band. The
failure columns say which way each lever breaks.

**`start_profiles.json` → `archetypes`**

| Lever | Opening | Too low | Too high |
|---|---|---|---|
| `minor.band_size` | 12 | Starves before contact matters | Indistinguishable from a major |
| `minor.loadout` | major's, minus kits | — | No disadvantage |
| `minor.min_separation` | 8 tiles | Minors crowd each other and majors | No room left; few minors fit |
| `minor.default_count` | null (derive from map) | Empty early game | Map is all minors |

**`personalities.json`** — each entry is `{id, culture_lean: {axis: offset}, weight}`. It starts
with a handful of entries, including one strongly Purist and one strongly Syncretic, so absorption
has both outcomes on the map.

**`wellbeing_config.json` → `joining.kinship`**

| Lever | Opening | Too low | Too high |
|---|---|---|---|
| `turns` | 20 | Minors join before they're met | Nobody ever joins |
| `max_culture_distance` | 0.3 | Only identical peoples join | Strangers join (soft divergence is 0.6) |

## Open items

- **Does an absorbed people consent?** As drafted, kinship joining is emergent like defection: A's
  seat is not asked. That includes a human seat that is the smaller people. The alternative is a
  join *offer* that the smaller people's seat accepts, which is a verb the AI then has to play.
- **Should a brain read its people's culture?** As drafted, the launcher draws the AI profile
  independently of the personality, so a Purist people might be played by an expansionist brain.
  Coupling them means the brain derives its weights from the culture in its snapshot, which keeps the
  sim AI-blind. It could be deferred to an AI slice.

## Slices

1. **Archetypes and personalities** (`core_sim`, client New Game). Covers the two catalogs, the
   seeded draws, per-archetype separation and capacity, and per-archetype counts in `NewGameCommand`
   and the capacity query. Done when a Standard map starts with majors and minors at their own sizes.
2. **Extinction** (`core_sim`, launcher). Covers the extinct flag, leaving the turn queue, roster
   drop, victory skip and save format bump. This is useful on its own, since defection can already
   empty a people.
3. **Joining: kinship pathway** (`core_sim` + client risk line). Covers the pair clock, culture
   distance, the hand-over, events, and the risk readout on both sides.
4. **A brain for every people** (launcher, `sim_ai`). Covers the per-seat `--profile` draw, a
   `sim_ai` CPU/RSS measurement at 1 + M seats, and a bench run in which minors start and play rather
   than starve. One process per seat stays unless this measurement says otherwise.

## See Also

- `docs/plan_band_fission.md` — defection (#512) and independence (#284), the other ways peoples form
  and change hands
- `docs/plan_multiplayer_seats.md` — seats are the only thing that drives a people
- `docs/plan_ai_opponents.md`, `docs/plan_ai_driver.md` — the brain as a player process; launcher
  supervision
- `docs/plan_contact_and_logistics.md` — ties and culture over a connection
- `.claude/rules/core_sim/factions.md`, `independence.md`, `launcher.md` — as-built records
