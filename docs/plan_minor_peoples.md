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
  `settle.parent_min_workers`. **Nothing moves a band for its culture.**
- **Culture.** There is a 15-axis vector per band. Contact drift pulls tied bands toward each other,
  weighted by headcount and damped by the receiver's Purist lean. `faction_trait_average` rolls bands
  up into a per-people average. **There is no people-to-people culture distance, and no culture
  strength**: headcount is the only weight, and it already sets the drift pull.
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
`run_turn` per seat** on a Standard map, of which about 2.5 ms is the seat's snapshot capture
(`.claude/rules/core_sim/turn-profiling.md`). Four majors plus eight minors is 12 seats, roughly
+34 ms per turn over a single seat. That is well within what a turn-based game can absorb. Each
`sim_ai` process decodes the full frame and runs its specialists every tick, but **its CPU and memory
have never been measured.** That is the number that could make "one process per seat" too expensive.

**Hosting several brains in one process is allowed and changes nothing in the sim.** It is still one
connection, one claim and one seat token per people. Either layout looks the same from the server. We
consolidate only if the measurement in Slice 5 says to; until then one process per seat stays,
because it is the shape the launcher already supervises.

### Starts: an archetype sets size, a personality sets character

Two independent config catalogs, each drawn per people at world build:

- **Start archetypes** set the opening strength: band count and size, loadout, knowledge tags,
  inventory, and placement separation. `major` and `minor` are just two entries. The human seat takes
  the first archetype. The archetype is used at world build and then forgotten. **No minor flag goes
  into `SimState`.**
- **Personalities** set the character: a lean added to each starting band's culture seed, per axis
  (for example `syncretic_purist` +0.8, or `passive_aggressive` +0.5). Majors and minors draw from
  the same catalog, so any people can be any personality. The draw is seeded
  (`map_seed ^ faction_id`), so a seed reproduces its world.

A people born by fission draws neither. It inherits its bands as they stand, culture included, which
is already the as-built rule.

### AI play style follows its people's culture

A brain reads its own people's culture from its frame (the bands' `cultureTraits`, which every seat
already receives for its own bands) and derives its behaviour weights from it. An Expansionist
people's brain roams and founds; an Insular one consolidates; a Purist one turns down join offers and
an accepting one takes them. `sim_ai/data/ai_profiles.json` becomes the **mapping from culture axes to
behaviour weights** rather than a list of named profiles that the launcher would pick from. The
launcher passes no profile.

This keeps the sim AI-blind: culture is game state every player sees, and the brain reads it the way
a human reads the culture panel. It also follows drift, so a people whose culture shifts over the
game shifts its play with it, and a break-away people plays its own culture from its first turn with
no draw.

### Placement: majors first, minors into the gaps

Archetypes are placed in catalog order. Each one has its own minimum separation, measured against
every start already placed. Majors keep their 20 (now `archetypes.major.min_separation`) and minors take a smaller number, so minors land in
the space between majors. The existing relaxed pass (keep the minimum distance as large as possible
and warn) applies per archetype.

**Capacity.** The `FactionCapacity` query returns a ceiling and a pre-selected default for each
archetype, computed from that archetype's own separation over the space the earlier archetypes left
free. New Game asks for a count per archetype: "Rivals" and "Minor peoples". As now, a request over
the ceiling is clamped, never refused.

### Peaceful absorption: culture pressure, band by band

**Joining** is a band changing hands without a fight because another people's culture outweighs its
own. It works **band by band**, like every other rule that moves people (defection, independence). A
band far from any band of the other people feels nothing, so a people is never pulled whole because
one frontier touches a neighbour. A whole people joins only when each of its bands has gone over. A
one- or two-band minor people beside a strong neighbour is exactly that case.

This is the first **absorption pathway**. Later pathways, such as conquest and other forms of
overwhelming culture, are further tests that call the same hand-over.

#### Culture strength: a people's weight of tradition

A new per-people number, separate from headcount, because headcount already sets the pull in contact
drift. It is a weighted sum of inputs, on the sedentarization score's pattern, so a new source is one
more row:

`strength = base + Σ weight_i × (input_i / reference_i)`

Opening inputs:

| Input | What it counts | Why |
|---|---|---|
| **Belief** | the belief held at the anchors of the people's bands, each anchor counted once | tradition: the dead a people honours; gatherings and monuments raise it later through the same seam |
| **Ties** | the people's live ties to other peoples' bands | a people that knows many others is the one others learn from |
| **Discoveries** | wondrous sites discovered (a Verdant Basin, and so on) | renown |

`base` keeps a new people's strength above zero, so a ratio between two fresh peoples is defined.
Inputs are **not** clamped: belief never decays, so an old rooted people keeps growing heavier, which
is the point.

#### Pressure: how hard people B weighs on one band of people A

Computed each turn for each band of A that has a **live tie to a band of B**. A band without one has
no pressure from B.

`pressure = (strength_B / strength_A) × (people_B / people_A) ^ size_exponent`

The size term means a much larger neighbour needs only a small culture edge, and a much smaller one
needs a large edge. When the sizes are nearly equal, culture decides: a people one person smaller
with triple the strength is asked to absorb the larger one (pressure about 2.9 at the opening
levers), though it cannot force the join. Both sizes are whole-people headcounts; the band is
only where the pressure lands.

#### Three phases on one number

| Phase | Needs | What happens |
|---|---|---|
| **1. Pulled** | `pressure ≥ joining.pulled` | The band's owner is told "*B*'s culture is pulling *band*." No mechanics. |
| **2. Asked** | `pressure ≥ joining.asked` **and** the band's culture is close to B's, both held `joining.turns` turns | The band asks to join B. The band's owner chooses whether to let it go, and B chooses whether to take it. It joins only if both agree. |
| **3. Forced** | `pressure ≥ joining.forced` **and** close, both held `joining.turns` turns | The band joins B if B agrees. Its owner is not asked. |

- **"Close"** means the largest per-axis gap between the **band's** culture and B's people-weighted
  average (`faction_trait_average`) is at most `joining.max_culture_distance`. This is the same metric
  divergence uses. Phase 1 does not need it, because the pull is what brings cultures together.
- **The receiver always decides.** B's seat accepts or declines in phases 2 and 3. This holds for now
  and may change in playtest. An unanswered offer stands while its conditions hold, so a seat nobody
  plays never takes anyone in.
- **A refusal holds only until phase 3.** If the owner says no in phase 2, nothing else happens. If
  the pressure keeps rising, the band reaches phase 3 and the owner loses the choice.
- **The clock** is per (band, people) pair, counts whole turns, and resets the turn a condition fails.
  It lives in `SimState`.

**What drives it is already in the game.** Contact drift pulls tied bands' cultures together, and a
Purist lean slows that pull: receptiveness is `1 − purist / 2.5`, so a lean of +0.8 still takes about
two thirds of it, and the purist axis itself drifts toward the other people's. A purist band
therefore reaches "close" later than an accepting one, but it gets there unless something breaks
the tie. Personality decides how fast a band is absorbed, with no special-case code.

**The hand-over is the defection remnant's.** The band changes faction with its parties, roads and
improvements (`follow_the_band_to_its_new_people`) and its knowledge. Ties are untouched. The
existing `band_changed_hands` pair of rows tells both peoples, with `cause=culture`. A people whose
last band joins goes extinct (next section).

**Both sides see it.** The band readout shows its pressure and phase toward each people pressing on
it, and the offers wait on both seats. The AI answers them from its own culture (see "AI play style
follows its people's culture").

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
| `major.min_separation` | 20 tiles (moves here from `simulation_config.json` `faction_start_min_separation`, which is retired) | Majors crowd each other | Few majors fit |
| `minor.band_size` | 12 | Starves before contact matters | Indistinguishable from a major |
| `minor.loadout` | major's, minus kits | — | No disadvantage |
| `minor.min_separation` | 8 tiles | Minors crowd each other and majors | No room left; few minors fit |
| `minor.default_count` | null (derive from map) | Empty early game | Map is all minors |

**`personalities.json`** — each entry is `{id, culture_lean: {axis: offset}, weight}`. It starts
with a handful of entries, including one strongly Purist and one strongly Syncretic, so the map
holds both slow and fast absorption.

**`culture_corruption_config.json` → `culture.strength`**

| Lever | Opening | Too low | Too high |
|---|---|---|---|
| `base` | 1.0 | Ratios between young peoples swing wildly | Every people weighs the same for a long time |
| `weights.belief` / `references.belief` | 1.0 / 20 dead-equivalents | Old rooted peoples don't outweigh newcomers | Belief is all that matters |
| `weights.ties` / `references.ties` | 0.5 / 4 ties | Contact counts for nothing | Most-met people swallows everyone |
| `weights.discoveries` / `references.discoveries` | 0.5 / 2 sites | Sites carry no renown | A lucky scout decides absorption |

**`culture_corruption_config.json` → `culture.joining`**

| Lever | Opening | Too low | Too high |
|---|---|---|---|
| `size_exponent` | 0.5 | Size stops mattering; culture alone decides | Big always absorbs small, culture irrelevant |
| `pulled` | 1.5 | Every tie reads as a pull; the message is noise | Players learn of pressure only when asked |
| `asked` | 2.0 | Offers arrive constantly | Nobody is ever asked |
| `forced` | 4.0 | A refusal means nothing | Nobody is ever forced; phase 2 is the only path |
| `max_culture_distance` | 0.3 | Only identical cultures join | Strangers join (band soft divergence is 0.6) |
| `turns` | 10 | Bands flip on a passing contact | Nobody ever holds the conditions long enough |

## Open items

- **Culture strength inputs beyond the opening three.** Gatherings and monuments arrive through
  belief with no change here. Knowledge, influential people and later sources are candidates, each
  one more weighted row.

## Slices

1. **Archetypes and personalities** (#818) (`core_sim`, client New Game). Covers the two catalogs, the
   seeded draws, per-archetype separation and capacity, and per-archetype counts in `NewGameCommand`
   and the capacity query. Done when a Standard map starts with majors and minors at their own sizes.
2. **Extinction** (#819) (`core_sim`, launcher). Covers the extinct flag, leaving the turn queue, roster
   drop, victory skip and save format bump. This is useful on its own, since defection can already
   empty a people.
3. **Culture strength and pressure** (#820) (`core_sim`). Covers the strength sum, per-band pressure and
   phase, the pair clock, and the readout on the band (phase 1). No one changes hands yet.
4. **Joining** (#821) (`core_sim`, client). Covers the offer state on both seats, the answer command, the
   phase 3 forced join, the hand-over and its events.
5. **A brain for every people** (#822) (`sim_ai`). Covers culture-derived behaviour weights, answering join
   offers, a `sim_ai` CPU/RSS measurement at 1 + M seats, and a bench run in which minors start and
   play rather than starve. One process per seat stays unless this measurement says otherwise.

## See Also

- `docs/plan_band_fission.md` — defection (#512) and independence (#284), the other ways peoples form
  and change hands
- `docs/plan_multiplayer_seats.md` — seats are the only thing that drives a people
- `docs/plan_ai_opponents.md`, `docs/plan_ai_driver.md` — the brain as a player process; launcher
  supervision
- `docs/plan_contact_and_logistics.md` — ties and culture over a connection
- `.claude/rules/core_sim/factions.md`, `independence.md`, `launcher.md` — as-built records
