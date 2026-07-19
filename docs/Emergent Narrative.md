# The Telling — Emergent Narrative & Disposition Layer
### Design Concept v0.1 · Trail Sovereigns / Shadow-Scale
Status: **concept, pre-implementation.** Captured from a brainstorm. Nothing here is built; one visual prototype exists (see §15). Copy is placeholder throughout.

---

## 1. What this is

A layer that gives the simulation a **voice** — turning what the systems already do (discoveries, migrations, cultural drift, trade seasons, crises) into a continuous, remembered **story**, and letting the player author a **disposition** (who their people are becoming) that both frames those events and steers which ones come next.

It is **not** a tutorial and **not** a help manual, though it absorbs the onboarding job as a side effect. It is the narrative connective tissue across every system.

---

## 2. Core thesis

> **We do not author a story. We author the *storytelling*.**

The game's philosophy is "everything emerges, nothing is hardcoded." A scripted plot is the enemy of that. So the simulation keeps generating the events; the narrative layer only gives them **voice, continuity, and consequence.** We never write "then a famine strikes." We write the voice that speaks *when the sim decides* a famine strikes. The story is emergent; the telling is authored. This dissolves the tension between "systemic sandbox" and "hand-crafted arc."

This is the same trick behind Dwarf Fortress, RimWorld, and Crusader Kings: the sim is the plot generator; the authored layer is the narrator and the choice architecture.

---

## 3. Design pillars (non-negotiables)

1. **Stable soul, fresh wardrobe.** Every beat separates a fixed *meaning + mechanical fork* (the soul) from a deep pool of *expressions* (the wardrobe). The soul is authored once; the wardrobe is drawn fresh each time, so no two tellings repeat. This is where replayability lives, and it must be built in from the start — freshness cannot be bolted on later. (Detail: §4, §11.)

2. **The voice never lies.** Narration only ever *describes what the simulation already did.* It can foreshadow a **real** risk the sim actually holds; it can never promise a story the numbers won't deliver. The moment narration and simulation disagree, the illusion dies. Narration is driven by state, always after the fact.

3. **Disposition is a mirror, not a menu.** The game watches what you actually do, *names* the identity forming underneath it, and asks you to ratify or resist it. Earned identity beats a start-screen pick. (Detail: §6.)

4. **Teach inside the fiction.** Never explain a mechanic; pose it as a survival stake in-voice. "The children are hungry and the berry-slopes are thick — who do we send?" is the same click as "assign foragers," but it's the *tribe* deciding.

5. **The narrative layer obeys the same law as the sim.** The *archetype* is the authored constant; the *expression* is emergent, drawn from the world's own generated nouns. This is the only way the layer survives "no hardcoding."

6. **Beat budget / tiers.** Most beats are cheap ambient texture (a quiet line). The rare *interrupt-you* forks are spent only on identity-defining moments. Severity tiers + cooldowns prevent pop-up spam.

---

## 4. Building blocks (the data model)

- **Soul** — the atomic authored unit. A fixed *dramatic function* + *mechanical fork*. Example soul: *"You can strip the land for a huge short-term surplus, or restrain for long-term stability — which appetite rules you?"* Stable across all worlds and playthroughs.

- **Wardrobe** — a pool of *expressions* attached to a soul, each dramatizing the same meaning. Tagged for selection. The runtime picks one weighted by:
  - **Fit** — does this dressing's ecology match the current world (biome, fauna, culture, the player's actual history)?
  - **Novelty** — not seen recently (needs a usage-memory / fired-set).
  - **Stance** — colored by the player's disposition (a nomad and a farmer hear the same soul differently).

- **Beat** — a *trigger* + *conditions* (a pattern of sim-state that must be true) + a *soul* + selected *wardrobe* + *choices* + *consequence flags* it writes. Fires once per edge (see the `sedentarization_tick` pattern in §14).

- **Stance / disposition** — persistent axes describing who the people are becoming. **Rides on the existing culture trait stack** (§7c of the manual: roam↔settle, war↔trade, open↔closed, sacred↔secular, etc.). The narrative layer is, in large part, *a voice for culture drift.*

- **Memory ledger** — a small set of durable **threads** (a rival tribe, a sacred mountain, a valley you refused, a bloodline) and **big choices**, so later beats can call back. Callbacks are what make a 200-turn emergent game *feel* authored.

- **The Voice** — the narrator identity. Diegetic and evolving (see §7).

---

## 5. Content tiers & pacing

| Tier | What it is | Surface | Cost |
|------|-----------|---------|------|
| **Ambient** | Texture lines — "the river-bend remembers us now." | The feed (exists today, cheap) | Very low |
| **Beat** | A small in-voice moment, maybe a light choice. | Feed or attention nudge | Low |
| **Fork** | An identity-defining interrupt with steering. | **New decision surface (must build)** | Spend rarely |
| **Tent-pole arc** | Hand-authored spine moments everyone hits (opening, first settlement, first great discovery, first crisis). | Full sequence | Highest craft |

Design consequence from the code scan: **ambient narration is nearly free** (a feed channel already exists), while the **interrupt-you fork is the genuinely new build.** So lean on lots of cheap ambient texture and spend the rare "stop and choose" moments on the forks — which is good pacing anyway.

---

## 6. Disposition / stance in depth

The flow:

1. **Accrete** — the sim tracks behavior (already does, via culture axes + discovery ledgers).
2. **Name** — when an axis crosses a threshold, a beat *names the emerging identity back to the player.* "The elders have started calling us the People of the Long Chase."
3. **Ratify or resist** — the player embraces, rejects, or defers. This writes a stance flag.
4. **Steer** — the stored stance biases the beat pool (which souls surface) and **re-colors shared events.** The same herd migration reads as "the great chase calls" to a nomad and "the beasts threaten the fields" to a farmer. One event, stance-filtered narration, opposite emotional valence — cheap to build, hugely characterful.

Key nuance we want to push further than the prototype currently shows: steering isn't only *unlocking your own beats* — it's **re-coloring the events every player shares.**

---

## 7. The Voice in depth

- **Who narrates:** diegetic — the tribe's own memory. Early on you're an oral culture, so the voice is **folk memory / a Rememberer**, speaking in "we."
- **It ages with the civilization:** as you cross milestones the voice changes *medium* — oral saga → painted chronicle → written record → institutional archive. The narrator maturing is *itself* a narrative arc that makes progression felt, not tabulated. (We wired a hint of this into the prototype's light/dark theme: **Fireside** = oral telling, **Chronicle** = written parchment.)
- **Two registers to decide between (open question):**
  - **Mythic** — sparse, oral-saga, short weighty lines.
  - **Warm** — an elder talking *to* you, conversational.
  - The prototype ships both as a live toggle because the choice is best made by feel. Possible answer: blend, or shift register as the medium evolves.

---

## 8. Arc structure

- **Acts = capability milestones.** The manual's milestone ladder (Forager → Pastoralist/Tended-Patches → Agrarian → Mechanization → Electricity → Flight/Info → Advanced Power) *is* the act structure. Each crossing is an act break — pause, narrate how far the people have come, pose a new orientation question suited to the era.
- **The spine = legitimacy.** For a nomadic polity, authority is earned "through patrol networks, treaties, trail knowledge" (manual §2a), not walls. So the through-line question of the whole campaign is: **why do these people follow you?** Beats challenge it (a failed hunt, a rival claimant, a broken promise) and affirm it (a great discovery, a herd delivered, a treaty kept). This gives every disposition choice weight beyond mechanics.

---

## 9. Cross-system collisions (the real magic)

Religion/trade/culture arcs don't come from one system — they come from the layer noticing when **several systems line up at once** and voicing the coincidence:

- Drought (environment) + rising mystic (influencer roster) + high Devout culture → a **prophecy** beat.
- Fat trade season + Indulgent culture drift + a chokepoint camp → a **market-festival** and the first merchant class questioning the leaders.
- Refused treaty + Honor-Bound population → sentiment whiplash felt as story, not a stat.

**Emergent narrative = detecting coincidences across systems and giving them a voice.** The design primitive (the beat, §4) is exactly a storylet whose *conditions* are a cross-system pattern. Words authored; timing and combination emergent.

---

## 10. The opening arc — worked example

(The through-line we built the prototype around. Both voices given.)

**Cold open (turn 0).**
- *Mythic:* "We are thirty. The ground behind us is bone, and we will not go back to it. Ahead lies a country with no names — not the hills, not the waters, not the years to come. Naming it is your work now. Walk well, and be remembered."
- *Warm:* "There are thirty of us, and the old ground has nothing left to give — we're not going back. Everything ahead is unnamed... Giving it all a name — that's your job now. Lead us well, and they'll remember you for it."
- *sim gloss:* start · band = 30 · fog = full.

**First lessons (diegetic teaching — Forage vs Scout).**
- *Mythic:* "The low slopes hang heavy with berries this moon, and the children's ribs are counting themselves. But the far ridge is a shadow none of us have read. We are too few to do both well. Where do the hands go?"
- Choices: *To the berry-slopes* (Forage) / *To the far ridge* (Scout). Teaches the labor split with no tooltip.

**The same moment, two ways (before/after feed).**
- Today: "Sedentarization available." / "Cultivation learned." / "Site discovered: bitter-blue ore."
- Telling (mythic): "The river-bend remembers us now. The young no longer ask when we leave." / "A woman pressed seed into the mud to see what it would do. The mud answered. We know a new thing." / "Past the ridge, a hollow where the stone runs bitter-blue. It has our name on it now — though not yet its worth."

**The Fork (identity mirror + disposition + steering).**
- *Mythic:* "Three seasons, and each one we chased the herds and left the seed-ground unturned. The children do not remember a walled night. At the fires, they have begun to call us the People of the Long Chase. Is that who we are?"
- Choices → each reveals *the stories that will find you now*:
  - **Yes — we are the trail** → hunting expeditions; roaming-rights (totems, patrols, disputed circuits); seasonal dilemmas; and one day the question under all of them: appetite or restraint? *(writes stance = roam)*
  - **No — we were meant to root** → fields and granaries; walls and wardens; herds become a thing you keep not chase; and one day: whether a settled people can still remember the trail. *(writes stance = settle)*
  - **Say nothing** → the fires keep the question; it returns, sharper, as pressure climbs. *(no stance written; soft prompt re-arms)*
- *sim gloss:* sedentarization ≈ 41 · culture.roam ▲ 0.71.

---

## 11. Freshness — worked example (the Overreach soul)

**Soul (fixed):** Take a great surplus now and scar the land, or take only what renews. Same decision, every world; only its face changes. (Maps to the wildlife Sustain / Surplus / Eradicate policy already designed.)

**Wardrobe (drawn fresh):**
- Ash-elk pouring through a pass — feast a year, silence a generation.
- Bitter-blue seams shallow and rich — arm every hand now, or leave the deep veins for the unborn.
- Glasswing shoals thick enough to walk on — net the whole run, or take the tithe and let them return.
- Bloomwood never taller — fell it all for a lasting camp, or thin it and let the canopy close.
- A marsh loud with nesting fowl — take every clutch, or leave enough that it's loud again next spring.

Same two choices every time (*Take it all* / *Take only the tithe*); the soul stays pinned while the dressing rotates.

---

## 12. Real-world archetypes are a *design tool*, not content

We use real-world analogies ("this is a buffalo-slaughter beat") because they instantly convey an emotional shape to a human designer. But the in-game content **need not reference Earth at all** — the runtime dresses the soul in *this world's* own animals, ore, and history. So:

> **Archetype = the authored constant (design-time shorthand for a feeling). Expression = emergent (what the player actually reads).**

There is a real spectrum here — pure human allusion ↔ pure emergent-world dressing. Decision: lean toward emergent-world dressing (keeps immersion + freshness + obeys the no-hardcoding law); real-world archetypes stay behind the curtain as the writer's skeleton.

---

## 13. Tensions & risks (honest list)

- **Narration/sim dissonance** — voice must never claim a story the numbers won't deliver. State-driven, describes-after. (Pillar 2.)
- **Repetition** — templated beats go stale fast; needs deep wardrobes + novelty memory + stance coloring.
- **Pacing/spam** — too many pop-ups = a chore. Beat budget, cooldowns, severity tiers.
- **Authoring cost** — someone has to write well. This is a craft investment, not just code. Tiered content (a few dozen tent-poles + a storylet grammar + procedural nouns) keeps it finite.
- **Determinism** — narrative RNG must be seeded like everything else so replays match.
- **Early-game thinness** — emergent-world callbacks need accumulated history; the opening arc leans more on authored tent-poles until history exists.

---

## 14. Implementation grounding (what exists vs greenfield)

From two code scans of the current repo. This is where a build would plug in.

**Already wired (build on these):**
- **Great Discovery System** (`core_sim/src/great_discovery.rs`) fires Bevy events: `GreatDiscoveryResolvedEvent`, `GreatDiscoveryEffectEvent { Power|Crisis|Diplomacy }`. Catalog: `great_discovery_definitions.json`. **Gap:** nothing turns a resolved discovery into a player-facing line yet — cleanest single insertion point for a beat hook.
- **"Learning by doing"** lives in `DiscoveryProgressLedger` (per-faction 0..1 on numeric discovery ids; `add_progress(faction, id, scalar)`). Constants e.g. `CULTIVATION_DISCOVERY_ID = 2003`, `HERDING_DISCOVERY_ID = 2004`. Tag→id map: `start_profile_knowledge_tags.json`. Threshold crossings are the "player learned X" trigger.
- **Secrecy/leak model** is separate: `KnowledgeLedger` + `KnowledgeTimelineEvent` (LeakProgress/Cascade/SpyProbe/CounterIntel).
- **Surfacing pipeline (works today):** `CommandEventKind` enum + `CommandEventEntry { tick, kind, faction, label, detail }` → `CommandEventLog` (ring buffer) → snapshot `commandEvents` → Godot `CommandFeedController.gd`. The feed renders any kind **generically**, so new server event kinds need no client change. There's also a **client-only** `note(label, detail)` injection path (`Hud._note_command_feed`) — the lowest-friction text channel.
- **The storylet-trigger template already exists:** `sedentarization_tick` (`core_sim/src/sedentarization.rs`) computes a 0–100 score and, on a **rising edge** across soft (~40) / hard (~70) thresholds, pushes a one-time prompt (edge-gated on stored `SedentarizationStage` so it fires once). **Copy this pattern for beat triggers.** Also `discover_sites` (`sites.rs`) → `SiteDiscovered` + reward; both are "sim detects a thing → feed entry" templates.
- **Milestone gating:** `CapabilityFlags` bits flip in `apply_capability_effects`, stream to the client as `capability_flags`, drive `Inspector._apply_capability_gating`. Piggyback for "you have unlocked X" beats.
- **Stance substrate:** the **culture trait stack** (manual §7c) and the **influencer roster** ("whispers → figures," `influencer_config.json`) already model drift and emergent figures.
- **The beat *format* is already designed** for the endgame: manual §9b "Foreshocks & Event Chains" = numbered steps, each with a narration Sample line, a branching Choice with costs/risks, tracked Metrics, and Policy/Tech/Infra tags. **The whole concept is essentially generalizing this crisis engine game-wide and adding the stance dimension.**

**Greenfield (must build):**
- **No decision/modal/choice UI exists, and nothing pauses on a player choice.** Nearest patterns: the TurnOrb attention popover (a "decision" producer is an anticipated stub) and the top-center targeting banner. The interrupt-you **fork** is the one genuinely new client component.
- **No tutorial/first-run/scripted-sequence state**, no storylet/quest/objective engine, no trigger DSL. (User-pref persistence pattern to copy: `ConfigFile` in `BandCityPanel.gd`.)
- **Localization is a 4-key stub** (`LocalizationStore.gd` + `en.json`); nearly all copy is hardcoded. A narrative layer would either extend it or ship its own strings.
- Possible alt host: the **QuickJS script sandbox** (`ui.compose` / `alerts.emit` / `storage.session`, receives snapshot/delta callbacks) — a data-driven storylet layer *could* live mod-side rather than in engine code.

**Suggested first build slice:** generalize the `sedentarization_tick` edge-gated-prompt pattern into a small **beat registry + fired-set**, a **beat-definition JSON catalog** (mirror `great_discovery_definitions.json`), surface via the existing `CommandEventLog` for ambient tiers, and build **one** new decision component for the fork tier — proving the loop on the opening nomadic-vs-settle arc end to end.

---

## 15. Prototype status & next moves

- **Artifact (visual concept):** "The Telling" — a self-contained page that walks the opening arc, with live Voice (Mythic/Warm) and Medium (Fireside/Chronicle) toggles, an interactive Fork that reveals steering, and a re-rollable Overreach freshness demo. Placeholder copy; not shipped UI.
- **Next moves (open):**
  1. Decide the **voice register** (mythic / warm / blend / evolves-with-medium).
  2. Push **steering** deeper — show a stance *re-coloring a shared event*, not just unlocking its own beats.
  3. Define the **beat-definition schema** (soul + wardrobe tags + trigger conditions + consequence flags) as the first concrete engineering artifact.
  4. Pick the **first build slice** (§14) and scope the one new decision component.

---

## 16. Glossary

- **The Telling** — working name for the whole narrative layer / the narrator's voice.
- **Soul** — a beat's fixed meaning + mechanical fork.
- **Wardrobe** — the pool of fresh expressions dressing a soul.
- **Beat** — a triggered narrative moment (ambient, beat, fork, or tent-pole tier).
- **Stance / disposition** — the player-authored identity that steers and colors beats; rides on culture axes.
- **Gloss** — the small "beneath the telling" line exposing the real sim state driving a beat (proof the voice never lies).
- **Fireside / Chronicle** — the oral (dark) vs written (light) presentation, standing in for the voice evolving with its medium.