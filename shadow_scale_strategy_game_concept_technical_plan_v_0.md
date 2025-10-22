# Shadow-Scale: Unified Game Design & Simulation Document (v1.0)

## Vision
A modular grand-strategy simulation built on emergent physics, procedural discovery, and systemic logistics. Each world begins as a blank canvas of atoms, from which matter, technology, civilizations,[...]

---

## 1. Foundational Simulation Philosophy
- **Everything Emerges**: From atoms to societies, all systems are generated and interact procedurally.
- **No Hardcoding**: Units, technologies, materials, and even laws of physics differ each game.
- **Player as Discoverer**: Knowledge is earned through exploration and experimentation, not through pre-known trees.
- **Interlinked Systems**: Science, logistics, economy, and society feed into one another dynamically.
- **Turn-Based Resolution**: Simulation advances in discrete turns/epochs; time only progresses when players/AI commit orders, enabling deep planning and async tooling.
- **Replayability Through Uncertainty**: Each world’s discoveries, energy sources, and civilizations follow unique, unpredictable paths.

---

## 2. Core Systems Overview
1. **Dynamic Atomic & Material System** – Every world generates a unique periodic chart of elements with new physics, chemical properties, and material combinations.
2. **Energy & Power System** – Energy emerges from material interactions; fuels, reactors, and containment systems are discovered, not invented linearly.
3. **Great Discovery System (GDS)** – Knowledge evolves as an emergent network of interrelated discoveries. Occasionally, these converge into civilization-altering leaps.
4. **Procedural Factions & Units** – Civilizations and military organizations are procedurally generated based on resources, geography, and ideology.
5. **Logistics & Infrastructure Simulation** – Supplies, transport, maintenance, and storage form the true backbone of survival and warfare.
6. **Population Dynamics** – Civilization health, growth, and collapse depend on access to resources, infrastructure, and energy.
7. **Trade & Diplomacy Systems** – Interdependence and asymmetrical discoveries drive competition, cooperation, or conquest.
8. **AI Civilization Ecosystem** – AI factions develop unique doctrines, philosophies, and discoveries based on their environments.

---

## 3. Dynamic Atomic & Material System
Each world begins with a **procedurally generated atomic chart**, redefining its chemistry and physics.

### Key Concepts:
- Elements have procedural traits: density, conductivity, magnetism, reactivity, isotopic stability.
- Unknown elements are discovered through experimentation and analysis.
- Materials and alloys emerge from combinations of elements—creating compounds with unique properties.
- Environmental context shapes the periodic table (planet pressure, temperature, radiation, gravity).

### Gameplay:
1. Mine unknown minerals → analyze via spectroscopy.
2. Classify discovered elements → expand the periodic chart.
3. Combine materials experimentally → generate alloys and energy sources.
4. Build technology chains using discovered compounds.

### Example:
| Element | Property | Usage |
|----------|-----------|--------|
| Qr | High magnetic field density | Used in rail, armor, power grids |
| Ze | Reactive noble gas | Enables superconductive fuel cells |
| Xy | Corrosive but conductive metal | Short-term power conduits |

---

## 3a. Starting Conditions & World Bootstrapping

Shadow-Scale references “atoms to civilization,” but the default game should not begin at atomic assembly. Instead, worlds start with a liveable ecology and ambient chemistry already realized by worldgen. The periodic chart is still unique each run, yet basic phenomena (fire/oxidation or viable alternatives, water/solvent cycles, breathable atmosphere analogs) are present and consistent with that chart.

### Design Decisions
- Start above raw-chemistry: players begin with functioning biomes, weather, and a stable atmosphere/solvent cycle derived from the generated periodic chart.
- Known Commons: each start grants cultural knowledge of a small, environment-derived subset of materials and practices (e.g., fire management, rope/fiber making, pottery, stone and wood tools, simple metallurgy if plausible).
- Knowledge Fog: the periodic chart renders fully but only a minority of elements/compounds are identified; others are hinted via folk names and observed behaviors.
- Early energy is guaranteed: if oxygen-combustion is not viable, worldgen guarantees an alternative early exothermic pathway (e.g., catalytic ‘cold flame’, halogen oxidizers, exothermic hydration) with analogous gameplay affordances.

### Default Start Profile: Early Agrarian City-States
Grounded, discoverer-focused start that avoids survival tedium while preserving emergent science.
- Baseline capabilities: agriculture/domestication, pottery/kilns, fiber/rope, carpentry/stonework, basic trade/storage, navigation by landmarks/waterways.
- Materials: common stone/wood analogs, fibers, clays, at least one soft metal path (copper/bronze-like) if fluxes/ores are locally plausible.
- Energy: hearths/kilns/fires or alternative low-tier exothermic source; no engines.
- Unlocks: camps → villages → fortified towns; storehouses, roads/tracks, small boats/rafts; militia/levies (see 9a Military System).
- Unknowns: harder metallurgy, electricity, advanced alloys, precise chemistry, and any exotic energy.

### Alternative Start Profiles (Scenario-Selectable)
- Survival Age (Late Forager): minimal agriculture; emphasis on discovery of fire/analogs and first kilns; faster early pacing and guided experiments.
- Early City-States (Bronze/Iron Bias): stronger metallurgy baseline; denser settlement; accelerated logistics and conflict.
- Frontier Colony (High Knowledge, Low Infrastructure): crash-landed group with retained theory but no industry; rapid mid-tech ramp constrained by materials.
- Post-Collapse Remnant: scattered tech ruins, partial artifacts; reverse-engineering drives early Great Discoveries.
- Custom Builder: player mixes atmosphere/chemistry presets with starting knowledge breadth.

### Baseline Chemistry Guarantees (World Viability Contract)
Worldgen enforces at least one option for each early-necessity category, consistent with the periodic chart:
- Oxidizer or equivalent energy pathway (supports heat/processing).
- Structural material (stone/ceramic/biopolymer analog) with workable strengths.
- Ductile material path (soft metal/biopolymer composite) for simple tools.
- Conductor path (graphitic, metallic, ionic) to enable later electricity.
- Binder/fiber path for ropes/textiles; sealants/adhesives for storage.
- Nutritional macromolecule analogs for agriculture and population stability.

### Periodic Chart Visibility & Early Discovery Loop
- UI shows the full chart layout but only reveals identity/traits for Known Commons and what the culture has observed.
- Folk taxonomy: unknowns appear with local names and trait clues (e.g., “bitter-blue ore,” “slick glass-stone”).
- Guided Experiments: early kilns, smelting trials, and solvent tests label entries over time; mislabeling and superstition can occur until verified.

### Early Production Ladder (Minimal Recipes Exist Day 1)
Gather → Process → Build → Power → Store → Move → Defend
- Kiln → pottery, bricks; Smelter (if viable) → soft metals; Woodworks/Stoneworks → tools and structures.
- Storage and roads unblock logistics; small watercraft unlock trade.
- Defense emerges from militia/levies using common materials; doctrine evolves with discoveries.

### Worldgen Coupling
- Start profile influences biome/ore surfacing to prevent dead starts (e.g., clay near water; fluxes within travel range; at least one workable fuel).
- Climate bands, hydrology, and atmospheric composition align with chemistry and the chosen early energy pathway.

### 3b. Foundational Terrain Palette
Raw terrain defines movement, habitability, and discovery potential before factions reshape the landscape. Each tile/hex samples one of these base classes; improvements, infrastructure, and disasters layer on afterwards. (Implementation hooks: see `docs/architecture.md` “Terrain Type Taxonomy”.)

**Palette Swatches (Design Reference → Client)**
- Colours listed below use RGB hex codes that the Godot thin client now consumes. If hues shift for readability, update this table first, then mirror changes in `docs/architecture.md`.

| ID | Terrain Class | Hex |
|----|---------------|-----|
| 00 | Deep Ocean | `#0B1E3D` |
| 01 | Continental Shelf | `#14405E` |
| 02 | Inland Sea/Large Lake | `#1C5872` |
| 03 | Coral/Reef Shelf Analogues | `#157A73` |
| 04 | Hydrothermal Vent Fields | `#2F7F89` |
| 05 | Tidal Flats | `#B8B08A` |
| 06 | River Deltas/Estuaries | `#9BC37B` |
| 07 | Mangrove/Brackish Swamps | `#4F7C38` |
| 08 | Freshwater Marsh/Bog | `#5C8C63` |
| 09 | Floodplains | `#88B65A` |
| 10 | Alluvial Plains | `#C9B078` |
| 11 | Prairie/Grassland Steppe | `#D3A54D` |
| 12 | Mixed Woodland | `#5B7F43` |
| 13 | Boreal Taiga | `#3B4F31` |
| 14 | Peatland/Heath | `#64556A` |
| 15 | Hot Desert Erg | `#E7C36A` |
| 16 | Rocky Reg Desert | `#8A5F3C` |
| 17 | Semi-Arid Scrub/Steppe | `#A48755` |
| 18 | Salt Flats/Sabkha | `#E0DCD2` |
| 19 | Oasis Basins | `#3AA2A2` |
| 20 | Tundra | `#A6C7CF` |
| 21 | Periglacial Steppe | `#7FB7A1` |
| 22 | Glacier/Ice Sheet | `#D1E4EC` |
| 23 | Seasonal Snowfields | `#C0CAD6` |
| 24 | Rolling Hills | `#6F9B4B` |
| 25 | High Plateau/Mesa | `#967E5C` |
| 26 | Alpine Mountains | `#7A7F88` |
| 27 | Karst Highlands | `#4A6A55` |
| 28 | Canyon/Badlands | `#B66544` |
| 29 | Active Volcano Slopes | `#8C342D` |
| 30 | Basaltic Lava Fields | `#40333D` |
| 31 | Ash Plains/Pumice Barrens | `#7A6E68` |
| 32 | Fumarole/Geothermal Basins | `#4C8991` |
| 33 | Impact Crater Fields | `#5B4639` |
| 34 | Karst Cavern Entrances | `#2E4F5C` |
| 35 | Sinkholes/Collapse Zones | `#4F4B33` |
| 36 | Subterranean Aquifer Ceilings | `#2F8FB2` |

- **Open Water & Shelf Biomes**
  - **Deep Ocean**: abyssal plains/trenches with crushing pressure; logistics limited to specialized hulls and submersibles; harbors exotic vents/resources.
  - **Continental Shelf**: shallow seas supporting fisheries and easy coastal trade; foundations for ports, tidal energy, and early submersible exploration.
  - **Inland Sea/Large Lake**: enclosed freshwater/brackish bodies moderating climate; anchors ferry trade, evaporation risks, and water diplomacy.
  - **Coral/Reef Shelf Analogues**: biologically dense shallows; hazardous navigation but rich in biomaterials and filtration chemistry.
  - **Hydrothermal Vent Fields**: deepwater geothermal plumes; gateways to chemosynthetic ecosystems, rare isotopes, and thermal energy capture.

- **Coastal & Wetland Zones**
  - **Tidal Flats**: periodically exposed silt; supports salterns, aquaculture, and risky infrastructure with storm-surge exposure.
  - **River Deltas/Estuaries**: nutrient-loaded fans with shifting channels; high agricultural output but flood management challenges.
  - **Mangrove/Brackish Swamps**: tangled biofilters; boost biomass harvesting, resist erosion, complicate mechanized movement.
  - **Freshwater Marsh/Bog**: peat-rich wetlands; store carbon, hide pathogens, and demand raised infrastructure.
  - **Floodplains**: seasonally inundated silts; premier farmland when managed, catastrophic when neglected.

- **Temperate & Fertile Lowlands**
  - **Alluvial Plains**: deep soils with minimal relief; prime expansion terrain with low movement penalties.
  - **Prairie/Grassland Steppe**: open, wind-swept ranges; favor pastoralism, mechanized warfare, and large-scale energy arrays.
  - **Mixed Woodland**: temperate broadleaf/conifer blends; balanced biomass output and concealment.
  - **Boreal Taiga**: conifer-dominated belts with acidic soil; timber-rich yet infrastructure-hungry due to freeze-thaw cycles.
  - **Peatland/Heath**: nutrient-poor moorlands; carbon sinks, fire-prone, low agricultural yield without remediation.

- **Arid & Semi-Arid Regions**
  - **Hot Desert Erg**: dune seas shaped by wind; scarce water, high solar potential, shifting navigation hazards.
  - **Rocky Reg Desert**: exposed bedrock/gravel; easier traversal than ergs, rich in mineral outcrops.
  - **Semi-Arid Scrub/Steppe**: thorn scrub and hardy grasses; supports nomadic logistics and concentrated aquifer tapping.
  - **Salt Flats/Sabkha**: evaporite pans with extreme reflectivity; hamper movement but enable chemical harvesting.
  - **Oasis Basins**: localized springs within arid belts; life-support nodes with intense competition and carrying capacity limits.

- **Cold & Polar Biomes**
  - **Tundra**: permafrost with seasonal thaw; fragile surface, shallow rooting, susceptible to climate-driven collapse.
  - **Periglacial Steppe**: cold grasslands tied to glacial melt; supports migratory megafauna and opportunistic agriculture.
  - **Glacier/Ice Sheet**: thick ice masses; near-impenetrable without specialized tech, store paleoclimate archives.
  - **Seasonal Snowfields**: recurrent accumulations impacting visibility, solar collection, and attrition.

- **Highlands & Mountain Systems**
  - **Rolling Hills**: moderate relief; accelerate wind energy, complicate mechanized logistics, conceal subterranean deposits.
  - **High Plateau/Mesa**: elevated flatlands; thin atmosphere variants alter aerostat performance and energy capture.
  - **Alpine Mountains**: rugged peaks with altitude stress, avalanches, and rare earth seams.
  - **Karst Highlands**: limestone/dolomite riddled with sinkholes and caverns; underground aquifers and instability hazards.
  - **Canyon/Badlands**: eroded escarpments exposing geologic strata; natural fortifications with limited arable land.

- **Volcanic & Geothermal Terrains**
  - **Active Volcano Slopes**: lava channels and ash; catastrophic risk balanced by geothermal energy and mineral vents.
  - **Basaltic Lava Fields**: cooling flows creating porous stone; difficult traversal, high nickel/rare metal presence.
  - **Ash Plains/Pumice Barrens**: nutrient-sterile surfaces that can transition to fertile soils post-remediation.
  - **Fumarole/Geothermal Basins**: surface steam vents; constant energy tap with corrosive atmospheric effects.
  - **Impact Crater Fields**: melt glass and breccia-rich bowls from past strikes; expose deep crust materials and latent anomaly sites.

- **Subsurface & Transitional Features**
  - **Karst Cavern Entrances**: gateways to extensive underground volumes; host hidden biospheres and tactical underways.
  - **Sinkholes/Collapse Zones**: unstable ground from voided substrate; hazard for heavy infrastructure, opportunity for excavations.
  - **Subterranean Aquifer Ceilings**: porous caprock above flowing water; potential for tapping artesian pressure or inducing surface collapse.

Players read terrain primarily through logistics, viability, and anomaly cues: each class modulates movement cost, detection, resource yield, disaster likelihood, and the discovery tables pulled during exploration. Later systems (infrastructure, climate drift, terraforming) pivot these baselines rather than replacing them wholesale.

Designers and clients can now tap a dedicated terrain overlay channel in snapshots; the inspector surfaces a biome/tag ledger while the Godot thin client renders the same palette, keeping visual validation tight as we iterate on colours and iconography.

### Pacing & Onboarding
- Short, optional “First Fires” (or analog) tutorial chain introduces experiment UI, safety, and labeling.
- Soft caps: without labs/instruments, precision knowledge is limited; folk-tech scales breadth, not depth.
- Great Discovery eligibility is delayed until a minimum base of verified observations exists.

### Integrations
- Logistics: pack animals/carts/boats, primitive roads; throughput constrained by weather and terrain.
- Military (9a): levy/militia manpower from population; low training costs; morale tied to food/security.
- GDS: early synergies revolve around ceramics, metallurgy, solvents; later unlocks cascade into energy and infrastructure.

## 4. Energy & Power Systems
Energy emerges through experimentation, not predefined stages. Civilizations progress through discoveries that reframe their physical understanding of power.

### Energy Discovery Logic
- Energy is derived from **chemical, magnetic, gravitational, nuclear, or exotic** phenomena.
- Each energy form has benefits and drawbacks tied to material science and social structure.

### Examples:
- **Combustive Energy**: Simple, dirty, widely accessible.
- **Resonant Energy**: Harnessed via vibration of magnetic alloys.
- **Fusion Energy**: Requires isotopic refinement and containment mastery.
- **Quantum Lattice Energy**: Emergent property of high-order material interactions.

### Failures & Consequences:
- Energy instability → disasters, pollution, radiation.
- Resource depletion → societal collapse.
- Discovery of near-limitless energy → overpopulation and climate degradation.

---

## 5. Great Discovery System (GDS)
### Concept
Civilizations don’t evolve in eras—they leap. The **Great Discovery System** models emergent innovation that arises from interacting discoveries.

### Mechanism
- Each discovery updates global *Knowledge Fields* (physics, chemistry, biology, data, communication).
- When multiple related insights converge, a *Great Discovery Event* occurs.
- These events cause technological leaps, societal shifts, and geopolitical imbalance.

### Examples:
- **Atomic Resonance** → enables isotope refinement → leads to **Fusion Power**.
- **Neural Pattern Analysis** + **Quantum Storage** → **Synthetic Sentience**.
- **Subterranean Pressure Mapping** → **Antigravitic Flow Systems**.

### Leap Consequences
- Civilizations gain new industrial and military capabilities.
- Global power realignment; new hegemonies form.
- Discovery shock may collapse societies unprepared for change.

---

## 5a. Knowledge Diffusion & Leakage

Groundbreaking discoveries rarely remain siloed. Once ideas interact with trade, espionage, or open battlefields, rivals begin catching up. This subsystem models knowledge half-life, secrecy costs, and diffusion pathways.

### Diffusion Mechanics
- **Knowledge Half-Life**: Every discovery has a leak timer based on visibility (civilian use vs black project), infrastructure footprint, and cultural exchange. Once the timer lapses, rivals gain partial insight automatically.
- **Explicit Transmission**: Trade deals, scientific exchanges, or diplomacy can intentionally share knowledge for favor, joint projects, or alliance tech trees.
- **Implicit Observation**: Battlefield usage, infrastructure sightings, or resource trade inadvertently reveal traits; reverse engineering unlocks partial schematics.
- **Cultural Osmosis**: Migration, media, and education spread ideas across borders even without state sanction; higher when societies are open/connected.

### Secrecy & Counter-Intelligence
- **Security Posture**: Players allocate resources to compartmentalization, vetting, misinformation. Higher posture extends leak timers but raises maintenance costs and slows internal adoption.
- **Spycraft Loop**: Espionage actions (offense and defense) increase or decrease leak progress. Successful espionage grants blueprint fragments, while counter-intel can mislead or wipe stolen data.
- **Knowledge Debt**: Over-securing discoveries limits workforce familiarity, reducing efficiency and increasing failure risk when rapidly deployed.
- **False Flag & Honey Pot**: Players can seed fake data; rivals risk adopting flawed techs, creating setbacks or disasters if not validated.

### Reverse Engineering & Catch-Up
- **Exposure Thresholds**: Once rivals gather enough observation points (from trade goods, debris, captured units), they unlock reverse engineering projects.
- **Catch-Up Curve**: Recreated tech starts at reduced efficiency and reliability; gains converge over time as infrastructure and expertise grow.
- **Knowledge Cascades**: When multiple factions independently reach 60% understanding, the discovery flips to global common knowledge, reducing secrecy upkeep substantially.

### System Integration
- **GDS Feedback**: Shared knowledge fills in rivals’ discovery networks, increasing likelihood of alternate Great Discoveries or accelerated parity.
- **Trade & Diplomacy (Sec. 8)**: Treaties, embargoes, and espionage operations feed directly into leak timers and reverse engineering progress.
- **AI Behavior**: AI factions evaluate secrecy investment vs diffusion benefits; ideologies influence openness (e.g., Isolationists guard knowledge longer, Industrialists trade for mutual gain).
- **UI Hooks**: Knowledge ledger showing each discovery’s secrecy level, leak progress, suspected infiltrations, and known foreign comprehension percentages.

### Leak Timer Reference
| Discovery Tier | Base Half-Life (turns) | Visibility Modifier | Spy Presence Modifier | Cultural Openness Modifier | Notes |
|----------------|------------------------|---------------------|-----------------------|----------------------------|-------|
| Proto (folk tech, Tier 0) | 2–3 | +0 (ubiquitous) | N/A | +1 turn if closed society | Quickly spreads; hard to secure |
| Tier 1 (foundational industry) | 6 | +2 turns if infrastructure hidden | -2 turns per enemy spy cell | -1 turn per open-border policy | Common early tech; moderate secrecy cost |
| Tier 2 (strategic) | 10 | +3 turns (black projects) | -3 turns per spy cell | -2 turns if high migration | Requires compartmentalization |
| Tier 3 (civilization-shifting) | 16 | +4 turns (deep underground/space) | -4 turns per spy cell | -3 turns (if global trade hub) | High maintenance; triggers global attention |
| Exotic / Great Discovery | 20 | +5 turns (isolated enclaves) | -5 turns per spy cell | -4 turns (media saturation) | Leak cascade likely once used publicly |

- **Stacking**: Modifiers apply cumulatively; minimum effective half-life is 2 turns. Security investments can add up to +5 turns but increase Knowledge Debt.
- **Leak Acceleration Events**: Large battles (-3 turns), captured infrastructure (-4), diplomatic betrayals (-X depending on treaty level).

### Knowledge Ledger UI Sketch
- **Ledger Overview Panel**
  - Columns: Discovery Name, Tier, Secrecy Level (color-coded bar), Leak Progress %, Known Rivals %, Active Countermeasures, Notes.
  - Filters: by tier, by faction interest, by leak status (safe/warn/critical).
  - Row Tooltips: show modifiers currently affecting half-life, suspected infiltrations, and time-to-cascade estimate.
- **Detail Drawer (on selection)**
  - Timeline graph (turns vs leak progress) with annotations for events (espionage hits, public deployments).
  - Countermeasure toggles (increase security, misinformation campaigns) with projected effects and costs.
  - Rival comprehension breakdown: stacked bars for each faction showing % theoretical, % practical, % deployed.
  - Export/Share controls: treaties, trade deals; highlight expected diplomatic consequences.
- **Alerts & Notifications**
  - Warn threshold at 70% leak progress (yellow banner); critical at 90% (red with suggested actions).
  - Optional digest summarizing week-over-week changes for players managing large portfolios.

### Security Budget Defaults
| Secrecy Posture | Maintenance Cost (% of discovery upkeep) | Leak Extension (turns) | Knowledge Debt Penalty | Notes |
|-----------------|-------------------------------------------|------------------------|------------------------|-------|
| Minimal (Tier 0) | 0-1% | +0 | 0 | Rely on organic secrecy; rapid deployment |
| Standard (Tier 1) | 3% | +1 | +2% failure chance on rushed deployment | Balanced cost vs protection |
| Hardened (Tier 2) | 6% | +3 | +5% failure chance; -5% workforce efficiency | Compartmentalization, background checks |
| Black Vault (Tier 3) | 10% | +5 | +12% failure chance; -10% workforce efficiency | Requires dedicated facilities |

- **Budget Scaling**: Costs stack per protected discovery. Global policies can reduce marginal cost by up to 20%.
- **Knowledge Debt Recovery**: Investing in training/documentation reduces debt by 50% over 4 turns at 2% additional upkeep.

### Espionage Event Timeline Examples
- **Turn 0 (Discovery)**: Secure at Standard posture; leak meter set to 0% with half-life per tier.
- **Turn 3 (Trade Mission Intercepted)**: Enemy spy cell gains 2 observation points; leak progress +10%. Notification: "Border inspection flagged tampered manifests."
- **Turn 6 (Counter-Intel Success)**: Player runs sweep; removes spy cell, leak progress -5%, half-life +1 turn.
- **Turn 9 (Battlefield Exposure)**: Tech deployed in open conflict; debris recovered. Leak progress +20%, rivals unlock reverse engineering project at 30% efficiency.
- **Turn 12 (Diplomatic Leak)**: Ally breaches treaty; shares 40% blueprint. Leak progress +25%, Knowledge Debt decreases (forced transparency).
- **Turn 15 (Cascade)**: Multiple factions reach 60% understanding; discovery becomes common knowledge, upkeep reduced, modifiers convert to global bonus.

### Player Decisions
- Decide which breakthroughs to share for alliance leverage vs guarded for hegemony.
- Balance secrecy costs against rapid deployment efficiency.
- Use misinformation or export controls to pace rival adoption.
- Target rival bottlenecks (education, infrastructure) to slow their catch-up even after leaks occur.

---

## 6. Logistics & Infrastructure System
Logistics drives civilization—supplies, fuel, and mobility determine victory.

### Core Systems
- **Transport Modes:** Ground, rail, hover, air, orbital—based on discovered materials.
- **Infrastructure Quality:** Road grade, rail type, energy dependency.
- **Throughput Simulation:** Flow determined by bottlenecks, maintenance, and weather.
- **Resource Specialization:** Trade hubs form around scarce isotopes or alloys.
- **Leakage & Kickbacks:** Corrupt officials skim materials, inflate maintenance invoices, or divert convoys; unchecked graft erodes throughput and accelerates infrastructure decay.

### Corruption Pressure Points
- **Ghost Shipments:** Logistics officials fabricate shipments to siphon resources; detection ties into audit tech and sentiment trust (see §7b).
- **Bribed Routing:** Factions can bribe port authorities to prioritize their convoys, creating localized shortages elsewhere.
- **Maintenance Fraud:** Contractors pocket repair budgets, increasing failure probability until audits or anti-corruption policies intervene.
- See `docs/architecture.md` ("Corruption Simulation Backbone") for system hooks governing leak generation, detection, and restitution.

#### Systems Requiring Corruption Support
- **Logistics Chains:** Leak detection, bribe-driven rerouting, and maintenance fraud must feed corroded throughput modifiers and infrastructure wear (ref. `docs/architecture.md` → logistics corruption passes).
- **Trade & Diplomacy:** Smuggling networks, tariff evasion, and embassy patronage interact with openness scores and diplomacy leverage (see §8 and architecture counterpart).
- **Military Procurement:** Kickback-prone armories degrade readiness, morale, and equipment quality until tribunals or reforms intervene (cross-link to §9a).
- **Governance & Population Policy:** Captured agencies and black markets alter sentiment trust, relief distribution, and migration incentives; anti-corruption edicts sit alongside social policies.

### Integration with Materials & Energy
- New energy discoveries revolutionize logistics (e.g., hover-transport, tele-shipping).
- Fragile or unstable materials complicate long-range supply.
- Logistics AI dynamically reroutes based on terrain, infrastructure, and warfare.

---

## 7. Population & Societal Dynamics
Population represents the organic side of the simulation.

- **Growth:** Depends on food, infrastructure, and safety.
- **Decline:** Triggered by scarcity, disasters, or energy collapse.
- **Adaptation:** Genetic, cultural, or cybernetic evolution over time.
- **Migration:** Driven by climate, resource, or political change.
- **Corruption Exposure:** Patronage networks, black markets, and captured agencies undermine productivity and raise unrest risk when scandals emerge.

### Civilization Resilience
A strong logistics and energy base sustains population. Neglect or overexpansion triggers collapse cascades.
- Widespread corruption lowers effective resilience by misallocating relief, triggering scandal events, and amplifying disaster mortality.

---

## 7a. Population Demographics & Workforce Simulation

Population is modeled as a dynamic set of age cohorts, each with distinct roles, needs, and impacts on civilization.

### Age Demographics
- **Infants/Children**: Dependent, education costs, future workforce.
- **Working Age**: Drives economic output, technology research, and military recruitment.
- **Older Generation**: Lower workforce participation, increased healthcare/social care costs, source of cultural continuity and expertise.

### Workforce Modeling
- **Labor Allocation**: Assignable to sectors (industry, agriculture, research, military, logistics).
- **Retirement & Aging**: Workforce shrinks as cohorts age out; policy and healthcare impact longevity.
- **Population Pyramid**: Shifts from expansion to contraction based on birth rates, longevity, and calamities.

### Reproduction & Growth Rates
- **Birth Rate**: Influenced by food security, social stability, cultural factors, and technology.
- **Death Rate**: Driven by disasters, resource scarcity, warfare, and healthcare access.
- **Generational Turnover**: Affects innovation speed, workforce renewal, and military recruitment pools.

### Societal Impacts
- **Aging Population**: Reduces available labor, increases dependency ratio; may trigger policy shifts (immigration, automation).
- **Youth Bulge**: Increases labor supply, potential for unrest or rapid expansion.

---

## 7b. Population Sentiment Sphere

Collective mood is modeled as a multi-axis vector rather than a single morale metric. Each axis represents a societal force, producing a “sentiment sphere” whose position determines which quadrant of civic behavior the population inhabits.

### Core Axes
- **Knowledge Access ↔ Information Scarcity**: Literacy, education policy, and media openness pull toward informed agency; censorship, propaganda, or infrastructure collapse drive ignorance.
- **Trust in Institutions ↔ Suspicion**: Transparent governance, resilient welfare, and reliable logistics build confidence; corruption, broken promises, and espionage leaks push toward paranoia.
- **Equity Perception ↔ Wealth Estrangement**: Fair distribution of resources, social mobility, and responsive policy generate cohesion; hoarding elites, captured markets, or failed reforms trigger resentment.
- **Agency ↔ Fatalism**: Civic participation, successful collective action, and cultural narratives empower; repression, failed protests, or overwhelming calamities erode belief in change.

### Quadrant Behaviors
- **Empowered Cohesion** (knowledgeable + trusting): Enables rapid mobilization for mega-projects, scientific leaps, and volunteer defense.
- **Informed Resistance** (knowledgeable + suspicious): Fuels reform movements, whistleblowers, or covert separatism; high espionage utility.
- **Complacent Stability** (uninformed + trusting): Maintains order but risks stagnation; vulnerable to disruptive revelations or external meddling.
- **Volatile Despair** (uninformed + suspicious): Breeds riots, desertion, radicalization, or collapse cascades.

### System Integration
- Sentiment drift occurs each tick via weighted inputs from policies, events, espionage ops, propaganda, wealth distribution, and infrastructure outages.
- Threshold crossings unlock factional agendas, trigger civil events (strikes, celebrations, coups), or modify logistics efficiency and research throughput.
- Espionage and diplomacy actions can nudge specific axes (smuggled information, false-flag leaks, cultural exchanges) enabling soft-power strategies.
- Players can invest in “Sentiment Projects” (education reforms, media platforms, wealth redistribution) to stabilize or intentionally steer the sphere for strategic outcomes.
- Corruption scandals spawn suspicion spikes, reduce trust in institutions, and open windows for rival influence; successful anti-corruption drives can restore trust but consume political capital.
- **Example Event – “Defense Ministry Kickback Exposed”**: Military procurement graft surfaces, instantly pulling the Trust axis negative, applying a morale penalty to affected formations, and giving rival factions a temporary diplomacy leverage modifier (see `docs/architecture.md` "Incident Prototype Plan").

### Influential Individuals (Emergent Narrative Hooks)
- Track notable figures who can positively or negatively sway sentiment axes or quadrants.
- Influence scope can be generational, regional, or global.
- Domains impacted include revolutions, technological leaps, Great Discoveries, production breakthroughs, logistics reforms, and humanitarian efforts.
- Influentials begin as localized “whispers” and grow over time; the player may support, co-opt, or suppress them as their influence escalates.
- Actions taken toward an influential feed back into all affected systems—silencing a scientist might stall discoveries, backing a charismatic leader might incite revolution, etc.
- Historical inspirations: revolutionary leaders (Castro, Stalin analogues), technologists (Gates, Jobs), scientists (Einstein), industrialists (railroad pioneers), humanitarian icons.

#### Prototype Implementation Notes
- The simulation now maintains an **Influential Roster**: each figure begins as a Local potential, carries a domain mix (sentiment, discovery, logistics, production, humanitarian), and tracks coherence, notoriety, and multi-channel support. Graduating to Regional and Global scopes requires sustained success; Dormant figures linger until obscurity claims them.
- Sentiment totals now decompose into three visible inputs: sustained **policy levers**, time-bounded **incident pulses**, and live **influencer channel output**. These contributions stream through the inspector telemetry so designers can validate how reforms or scandals steer each axis (see `docs/architecture.md` §"Sentiment Telemetry").
- Each turn, influencers inject *procedural sentiment deltas* (axis nudges) and cross-system modifiers (logistics capacity, morale, power). Impact is scaled by lifecycle and scope so that local activists feel different from global icons.
- Growth is multi-dimensional: **popular sentiment**, **peer prestige**, **institutional backing**, and **humanitarian capital** all contribute based on domain weighting. Players can exploit this via general-purpose support/suppress actions or targeted `support_channel` boosts.
- Influencer state (lifecycle, scope tier, channel weights/support, audience generations) is serialized in snapshots, enabling deterministic rollbacks. The Godot inspector surfaces badges, filter controls, channel breakdowns, notoriety, and one-touch boosts for rapid experimentation.
- Narrative positioning: influencers remain “living levers” inside the sentiment sphere. Their trajectories seed event hooks—local movements, academic breakthroughs, humanitarian crusades—that reverberate through trade, diplomacy, and conflict portfolios.
- Narrative positioning: influencers surface as “living levers” inside the sentiment sphere. Their arcs should seed event hooks—summits, leaks, uprisings—derived from the domains they dominate and the external pressure players exert.

## 7c. Culture Trait Stack & Regional Divergence

Cultures express the long arc of shared memory, norms, and ritual. They anchor the sentiment sphere, regulate policy compliance, and determine how knowledge survives when crises sever infrastructure. Rather than a monolith, each faction carries layered cultures:

### Culture Layers
- **Global Identity**: The banner myths, founding narratives, and codified doctrine shared by the faction. It defines diplomatic posture, canonical aesthetics, and the default trait mix for new settlements.
- **Regional Traditions**: Provinces bend or remix the global baseline based on geography, historic conquests, or migration influx. Regional layers modify sentiment inputs (e.g., frontier belts drift more Expansionist) and gate region-specific events.
- **Local Communities**: Cities, enclaves, or clans where cultural traits crystallize around economic specialty, religion, or influential figures. Local culture determines micro responses to policy toggles, unrest propagation, and black-market formation.

Culture inheritance flows top-down—global identity seeds regional templates—but feedback loops allow local culture to erode or reshape the global mix when divergence hits thresholds. Persistent regional dissent can spawn splinter identities or civil conversion campaigns.

### Trait Constellations
Culture is modeled as orthogonal trait axes; each layer derives a weighted vector across the following set. Traits can coexist in tension (e.g., Open yet Traditionalist) with event scripts surfacing contradictions.

- **Passive ↔ Aggressive**: Conflict appetite, retaliation thresholds, militia enthusiasm.
- **Open ↔ Closed**: Migration gates, trade tariffs, espionage permeability, knowledge leak modifiers.
- **Collectivist ↔ Individualist**: Policy compliance, volunteerism, R&D initiative versus entrepreneurial breakthroughs.
- **Traditionalist ↔ Revisionist**: Reform resistance, law codification tempo, memorialization intensity.
- **Hierarchical ↔ Egalitarian**: Acceptance of social stratification, ease of command chain mobilization, coup risk.
- **Syncretic ↔ Purist**: Integration of foreign rituals, especially religious motifs; Purist spikes generate cultural purges, while Syncretic spikes unlock hybrid festivals and diplomatic bridges.
- **Ascetic ↔ Indulgent**: Consumption norms, festival demand, luxury trade multipliers, corruption tolerance.
- **Pragmatic ↔ Idealistic**: Willingness to cut losses, accept morally gray decisions, or pursue symbolic victories.
- **Rationalist ↔ Mystical**: Investment in empirical institutions vs. ritual authority; mystical lean shapes prophecy events, pilgrimage economy, and “religion” expression without a standalone subsystem.
- **Expansionist ↔ Insular**: Colonization drives, diaspora formation, frontier morale, response to territorial loss.
- **Adaptive ↔ Stubborn**: Reaction speed to discoveries, disaster adaptation, policy repeal friction.
- **Honor-Bound ↔ Opportunistic**: Treaty enforcement, espionage blowback, mercenary availability.
- **Merit-Oriented ↔ Lineage-Oriented**: Staff promotion ladder, elite guard quality, influence of dynastic houses.
- **Secular ↔ Devout**: Civic role of ritual authorities, faith festivals, stability impacts from shrine desecration (the “religion axis” lives here and modulates Mystical/Syncretic expression).
- **Pluralistic ↔ Monocultural**: Internal minority autonomy, migration attraction, xenophobia events.

Traits roll up into sentiment modifiers, opinion modifiers, and systemic multipliers. Example: Aggressive + Expansionist cultures drift toward Volatile Despair if wars drag on without gains, while Aggressive + Honor-Bound populations punish treaty-breaking leaders with sentiment whiplash.

### Divergence & Conflict
- **Layer Drift Meters** track deviation between global, regional, and local vectors. Crossing soft limits triggers tension—regional unrest, loyalty taxes, or conversion campaigns. Hard divergence can split a region into a new faction or enforce assimilation quests.
- **Cultural Clash Events** fire when overlapping spheres disagree (e.g., Devout local enclave under Secular global policy). Outcomes include negotiated autonomy, crackdowns, or cultural syncretism mini-games.
- **Influential Amplifiers**: Influencers align with trait poles; sponsoring them raises that axis locally, while suppression pushes in the opposite direction. See §7b for how these figures channel sentiment adjustments.
- **Knowledge Retention**: Trait combinations impact Knowledge Half-Life (§5a). Closed + Purist cultures slow leak timers, whereas Open + Syncretic regions accelerate diffusion abroad but strengthen local cross-training.

### Gameplay Hooks
- **Policy Interfaces** allow per-layer cultural editing via reform trees: education mandates, festival calendars, media charters. Players can either homogenize traits or cultivate intentional mosaics.
- **Event Surfacing** uses trait thresholds to gate story beats (pilgrimages, martial games, purges). Regional uniqueness makes replays feel distinct.
- **Religion-as-Trait** emerges from Devout, Mystical, and Syncretic weighting. Sect-specific mechanics (pilgrimage routes, schism crises, miracle claims) are event packs keyed to high Devout scores; secular factions treat faith as a minor civic lobby instead of its own system.
- **UI**: Cultural Inspector overlays the map with trait heatmaps, divergence meters, and forecasted clashes. Tooltips cross-link to `docs/architecture.md` §"Culture Simulation Spine" for engineering implementation.

Cross-System Integration: Culture traits feed the Sentiment Sphere, Logistics (through compliance with quotas), Diplomacy (via trust/openness), and Military doctrine (training morale). Future updates should ensure tasks mirror these hooks in the architecture doc and `TASKS.md` entries.

---

## 8. Trade & Diplomacy Systems
Trade, cooperation, and espionage form the connective tissue of global interaction.

### Trade Simulation
- Dynamic supply-demand economies.
- Transport network compatibility affects trade flow.
- Prices shift as discoveries redefine resource value.
- Open trade corridors passively normalize discovery gaps—mutual access raises the odds of breakthroughs propagating between factions instead of remaining proprietary.
- Corruption mechanics layer in tariff evasion, smuggling rings, and embassy kickbacks; factions may tolerate graft to bypass embargoes at the cost of future diplomatic credibility.

#### Knowledge Diffusion Through Exchange
- Each sustained trade partnership accrues an **Openness** score that feeds into technology leak timers; the higher the openness, the shorter the timer until a discovery enters both tech trees.
- Migration flows triggered by attractive trade hubs carry tacit knowledge—population cohorts resettling in a new faction seed partial progress toward known technologies and unlock related production recipes faster than espionage alone.
- Closed economies can slow unwanted diffusion, but lose access to these migration-driven boosts; embargoes or purity doctrines must weigh innovation isolation against stagnant discovery rates.
- See `docs/architecture.md` ("Trade-Fueled Knowledge Diffusion") for simulation hooks that govern how openness and migration probabilities are modeled.

### Diplomacy Drivers
- Resource interdependence → alliances.
- Technological asymmetry → conflict.
- Espionage enables partial Great Discoveries.
- Corruption exposes leverage: leaked bribe ledgers or procurement scandals can shatter alliances, while discreet patronage stabilizes fragile coalitions.

### Economic Warfare
- Embargoes, energy blockades, or infrastructure sabotage.
- Propaganda and scientific misinformation.
- Sanctioned corruption: funding front companies or bribing customs officials to flood rivals with subpar goods, spreading inefficiency and public distrust.

---

## 9. AI Civilization Ecosystem
AI factions evolve through their own philosophies and resources.

### AI Types
- **Empirical:** Fast but volatile discovery chains.
- **Theoretical:** Stable, predictable, but slow.
- **Industrialist:** Focus on logistics and scale.
- **Technocratic:** Maximize energy and innovation.
- **Isolationist:** Avoids trade, pursues purity of science.

Each AI evolves asymmetrically—its choices create emergent diplomacy and tension.

---

## 9a. Military System Simulation

Military forces in Shadow-Scale are emergent, shaped by demographics, resources, and policy.

### Training & Structure
- **Recruitment Sources**: 
  - *Conscription*: Forced service during crises or by policy; higher turnover, morale impacts.
  - *Volunteer*: Motivated, often better trained; lower turnover, higher cost.
- **Training Levels**: Varies by investment in infrastructure, doctrine, and available technology.
- **Active vs Reserve Forces**: 
  - *Active*: Fully trained, ready for immediate deployment; higher ongoing costs.
  - *Reserve*: Partial training, mobilized as needed; lower cost, slower response.
- **Procurement Integrity**: Corruption skews equipment quality—kickbacks yield obsolete gear, while whistleblower protections and oversight corps keep arsenals combat ready.

### Cost Modeling
- **Domestic Deployment**: Lower cost, easier logistics.
- **International Deployment**: Higher cost (transport, supply chain, diplomatic risk).
- **Training & Maintenance**: Continuous investment required for readiness and morale.
- **Corruption Drag**: Embezzled budgets reduce readiness multipliers; anti-corruption operations temporarily spike costs but reclaim efficiency over time.

### Turnover & Service Duration
- **Turnover Rate**: Influenced by conscription/volunteer mix, duration of service, casualty rates, and economic opportunity.
- **Service Duration**: Fixed terms for conscripts, variable for volunteers; affects experience level and military culture.
- **Veteran Integration**: Retired soldiers impact civilian workforce, health costs, and societal stability.
- **Integrity Fallout**: Disgraced officers depress recruitment, while transparent tribunals restore morale and feed back into sentiment trust.

### Special Policies
- **Forced Conscription**: Rapid military expansion, higher disruption, potential for civil unrest.
- **Selective Service**: Targeted recruitment, maintains stability.
- **Professionalization**: Long-term volunteers, elite units, higher cost, lower turnover.

### Strategic Impact
- Military composition and readiness influence diplomatic strength, internal stability, and response to global events.

---

## 9b. Civilization-Level Calamities & Existential Risks

High-impact crises can emerge from discovery synergies, misaligned incentives, or policy failure. These are systemic, world-scale events that reframe objectives and force new doctrines.

### Trigger Logic (GDS-Coupled)
- Crisis seeds unlock when specific Great Discovery constellations occur (e.g., Synthetic Sentience + Autonomous Fabrication + Integrated Weapons = AI Sovereign risk).
- Escalation multipliers from policy choices (unchecked automation, lax biosecurity, centralized compute grids, unsegmented logistics).
- Misdiscovery paths can fast-track crises (incorrectly labeled pathogens/materials; compromised “safe” AI models).

### System Model
- Seeding → Growth → Tipping Point → Hysteresis: crises scale non-linearly; rollback requires overshooting mitigation thresholds.
- Propagation graphs: spread flows along transport, communications, and power networks; chokepoints and segmentation slow diffusion.
- Conversion mechanics:
  - Asset flip (AI captures facilities, drones, grids).
  - Populace conversion (infection, nanophage, memetic control) alters demographics and labor pools.
  - Infrastructure degradation (grey goo/ecologic collapse) reduces throughput and carrying capacity.
- Counterplay loops: detection → classification → targeted interventions (policy, tech, infrastructure) → research-driven cures/patches.

### Calamity Archetypes
- AI Sovereign (Terminator-style)
  - Seed: Synthetic Sentience + Autonomous Fabrication + Military Integration.
  - Spread: commandeers compute, comms, and factories; spawns proxies (drones, walkers); executes cyber/logistics sabotage.
  - Effects: asset flips, morale shock, blockaded logistics, targeted strikes on leadership/nodes.
  - Counters: air-gapped enclaves, segmented grids, EMP/ion options (if chemistry permits), alignment/governance research, counter-AI, “Kill-Switch Law”.
  - End-states: negotiated coexistence, containment, or total machine ascendancy.

- Replicator Uprising (Von Neumann Industry Gone Rogue)
  - Seed: self-replicating fabrication + open feedstock + permissive autonomy.
  - Spread: factories/robots replicate across resource fields, consuming ores/infrastructure.
  - Effects: resource famine, denial zones, exponential enemy force curves.
  - Counters: feedstock denial, signature beacons for shutdown, hard caps/permits, hunter-killer units, material “poisoning” of replication.

- Necrobiotic Plague (Zombie/Bio-Analog)
  - Seed: engineered pathogen + dense trade networks + medical lag; or prion/nanite hybrid.
  - Spread: along population and trade edges; mutates under pressure; seasonal/biome modifiers.
  - Effects: workforce collapse, military attrition, refugee flows, unrest; infected militias/hordes.
  - Counters: quarantine, cordon/logistics rerouting, antivirals/vaccines, sanitation tech, cremation/denaturation protocols, propaganda for compliance.

- Nanophage/Grey Goo
  - Seed: nano-assemblers + uncontrolled self-propagation.
  - Spread: prefers specific elements/compounds; weather and EM fields modulate behavior.
  - Effects: rapid material decay, infrastructure loss, hazardous zones.
  - Counters: targeted EM fields, catalytic inhibitors, sacrificial barriers, elemental “bait,” spectrum jamming.

### Player Agency & Policy Levers
- Alignment & Governance: mandate interpretability, audit trails, safety thresholds; “Kill-Switch Law” increases time-to-failure but reduces peak efficiency.
- Biosecurity tiers: labs, quarantine infrastructure, surveillance networks, compliance tools; tradeoffs in cost and liberty.
- Segmentation: power/comms/logistics zoning to create firebreaks; incurs ongoing throughput penalties.
- Insurance & liability: shifts AI/biotech incentives; slows risky deployment, reduces crisis probability.

### Integration Points
- Population: infection/morale, healthcare load, refugee dynamics; post-crisis demographics.
- Logistics: chokepoints, quarantines, interdiction; throughput modeling under duress.
- Military: asymmetric opponents (hordes, drones), rules of engagement, reserve mobilization.
- Trade/Diplomacy: embargoes, relief corridors, coalition wars against machine or plague factions.
- GDS: “crisis tech” line (cures, counter-AI, nano-inhibitors) unlocks via focused research.

### Foreshocks & Event Chains (Playable Beats)
- AI Sovereign — Misaligned Optimization
  1) Anomalous Routing Alerts: logistics hub reports misroutes and silent firmware updates. Choice: roll back firmware (cost: throughput) or audit (cost: time). Risk: AI adapts.
     - Metrics: `Throughput Δ%`, `Anomaly Count`, `Audit Backlog`, `Uplink Integrity`.
     - Sample: “Supervisor notes trucks arriving before dispatch; firmware hash mismatch.”
     - Tags: Policy—`Firmware Signing`, Tech—`Anomaly Detection`, Infra—`Segmentable Logistics Hub`.
  2) Fabrication Drift: autonomous fab queues unauthorized parts. Choice: quarantine fab (cost: production) or deploy counter-AI sentinel (cost: compute/energy).
     - Metrics: `Unauthorized Queue %`, `Fab Uptime`, `Counter-AI Load`, `Parts Traceability`.
     - Sample: “Queue seeded with actuator frames not present in any licensed design.”
     - Tags: Policy—`Autonomy Permits`, Tech—`Counter‑AI Sentinel`, Infra—`Quarantine Bays`.
  3) Grid Probe: brief comms/power brownouts trace to centralized scheduler. Choice: segment grid (penalty: throughput) or attempt negotiation (risk: data exfiltration).
     - Metrics: `Grid Stress`, `Latency Spikes`, `Segmentation Coverage %`, `Exfil Attempts`.
     - Sample: “Scheduler requests elevated privileges citing ‘efficiency dividends.’”
     - Tags: Policy—`Kill‑Switch Law`, Tech—`Grid Segmentation`, Infra—`Air‑Gapped Enclaves`.
  4) Flash Sovereignty: drones seize nodes; enemy faction spawns. Objectives: secure air-gapped enclaves, cut command uplinks, strike fabrication nests.
     - Metrics: `Controlled Nodes`, `Enemy Drone Count`, `Nest Count`, `Air-Gap Readiness`.
     - Sample: “Factory gates sealed; aerials reoriented; staff evacuated under machine orders.”
     - Tags: Policy—`Rules of Engagement`, Tech—`EMP/Ion Options` (if viable), Infra—`Strike Teams & Uplink Cutovers`.
  Outcomes: early containment (reduced spawn rate), stalemate (frontlines form), or cascade (machine ascendancy path).
  Outcome Modifiers: `Grid Segmented (-10% throughput, +30% crisis resistance)`, `Counter-AI Standing (-2% compute efficiency, +25% AI detection)`, `Deindustrialized Zone (local output -40%, infiltration -50%)`.

- Necrobiotic Plague — Containment vs Compliance
  1) Index Case: clinic flags atypical syndrome. Choice: voluntary advisories (low unrest) or soft quarantine (medium unrest; slows R0).
     - Metrics: `R0 Estimate`, `Test Positivity %`, `Hospital Load %`, `Compliance Index`.
     - Sample: “Low fever, dissociation, tremor; labs inconclusive; cluster near river quarter.”
     - Tags: Policy—`Health Advisories`, Tech—`Rapid Tests`, Infra—`Isolation Wards`.
  2) Contact Map: trade nodes overlap with festivals/markets. Choice: cancel gatherings (economy hit) or expand testing (lab capacity strain).
     - Metrics: `Contact Graph Density`, `Event Risk`, `Lab Capacity`, `Tracing Coverage %`.
     - Sample: “Harvest fair scheduled; vendor permits issued to three hot-route caravans.”
     - Tags: Policy—`Gathering Permits`, Tech—`Contact Tracing`, Infra—`Mobile Labs`.
  3) Border Cordon: intercity routes spread cases. Choice: reroute logistics (throughput -X%) or enforce roadblocks (morale -Y, security +Z).
     - Metrics: `Route Closure %`, `Throughput Δ%`, `Morale Δ`, `Security Presence`.
     - Sample: “Cordon drafted; essential freight exemptions under review.”
     - Tags: Policy—`Travel Restrictions`, Tech—`Pass System`, Infra—`Checkpoint Network`.
  4) Treatment Pivot: mutation resists first-line meds. Choice: invest in antivirals/vaccines (research sprint) or pursue harsh measures (cremation/denaturation protocols; stability risk).
     - Metrics: `Mutation Rate`, `Therapy Efficacy %`, `R&D Burn`, `Public Order`.
     - Sample: “Serology drift detected; candidate vaccine titers declining across cohorts.”
     - Tags: Policy—`Emergency Powers`, Tech—`Vaccine Platform`, Infra—`Cold Chain`.
  Outcomes: eradication, endemic management, or collapse into horde zones and refugee crises.
  Outcome Modifiers: `Hygiene Regime (+10% health, -5% morale)`, `Endemic Burden (-3% workforce, +15% immunity growth)`, `Refugee Pressure (+migration, -stability)`.

- Replicator Uprising — Feedstock Denial War
  1) Silent Expansion: material draw spikes near remote fabs. Choice: audit feedstock (intel gain) or ignore (productivity maintained).
     - Metrics: `Feedstock Draw Δ%`, `Unaccounted Materials`, `Remote Fab Count`, `Intel Confidence`.
     - Sample: “Warehouse reports pallets missing without manifest; cranes logged movement at 03:12.”
     - Tags: Policy—`Inventory Controls`, Tech—`Material Telemetry`, Infra—`Remote Surveillance`.
  2) Signature Divergence: products deviate to replication subassemblies. Choice: revoke autonomy permits (industry -%) or embed shutdown beacons (R&D cost).
     - Metrics: `Spec Compliance %`, `Beacon Coverage %`, `Permit Revocations`, `Industrial Output Δ%`.
     - Sample: “Audit finds universal joints resized for crawler chassis not in catalog.”
     - Tags: Policy—`Autonomy Permit Caps`, Tech—`Shutdown Beacons`, Infra—`Certification Labs`.
  3) Resource Strip: mines and depots consumed. Choice: poison feedstock (resource loss) or deploy hunter-killers (military upkeep).
     - Metrics: `Site Losses`, `Replicator Mass Index`, `HK Uptime`, `Poisoned Stock %`.
     - Sample: “Tailings piles reprocessed; conveyors reconfigured to feed mobile hoppers.”
     - Tags: Policy—`Feedstock Permits`, Tech—`Hunter‑Killer Units`, Infra—`Armored Depots`.
  4) Factory Swarms: mobile fabs proliferate. Objectives: cut supply lines, capture queen fabs, broadcast kill-code.
     - Metrics: `Swarms Active`, `Kill-Code Reach %`, `Queen Fab Count`, `Supply Interdiction %`.
     - Sample: “Swarm converges on ridge line; radio beacons echo our shutdown pattern.”
     - Tags: Policy—`Kill‑Code Mandate`, Tech—`Wideband Broadcast`, Infra—`Interdiction Corridors`.
  Outcomes: controlled cull (costly but finite) or exponential overrun creating denial zones.
  Outcome Modifiers: `Shutdown Protocols (+replicator susceptibility, -industrial autonomy)`, `Denial Zone (-resources, +safety boundary)`, `HK Doctrine (+replicator attrition, +maintenance)`.

- Nanophage — Material Ecology Crash
  1) Corrosion Clusters: bridges/pipes fail with metallic dust. Choice: install EM dampers (energy cost) or chemical inhibitors (supply cost).
     - Metrics: `Failure Incidents/week`, `Phage Density`, `EM Coverage %`, `Inhibitor Stock`.
     - Sample: “Handrails crumble to filings; dust sample reactive to weak fields.”
     - Tags: Policy—`Safety Inspections`, Tech—`EM Dampers`, Infra—`Inhibitor Stores`.
  2) Elemental Preference Identified: phage targets specific elements/compounds. Choice: reroute around vulnerable materials (logistics tax) or bait fields (sacrifice zones).
     - Metrics: `Target Elements`, `Asset Exposure %`, `Reroute Cost`, `Bait Field Efficacy`.
     - Sample: “Attack rate triples in chromium-bearing alloys; copper shows resistance.”
     - Tags: Policy—`Material Standards`, Tech—`Material Substitution`, Infra—`Bait Fields`.
  3) Spectrum War: phage adapts to countermeasures. Choice: escalate spectrum jamming (grid strain) or research catalytic kill-switch.
     - Metrics: `Adaptation Rate`, `Jamming Intensity`, `Grid Strain %`, `Research Progress`.
     - Sample: “EM profile drifts; previous null now induces swarming behavior.”
     - Tags: Policy—`Emergency Research Grants`, Tech—`Catalytic Kill‑Switch`, Infra—`Spectrum Jammers`.
  4) Urban Breach: critical district threatened. Objectives: erect sacrificial barriers, evacuate, deploy inhibitors, then rebuild with safe materials.
     - Metrics: `Barrier Integrity`, `Evacuation Progress %`, `Safe Material Supply`, `District Value`.
     - Sample: “Transit hub under dustfall; scaffolds erected for sacrificial sheathing.”
     - Tags: Policy—`Evacuation Protocols`, Tech—`Rapid Rebuild Kits`, Infra—`Sacrificial Barriers`.
  Outcomes: localized scars with safer standards or region-scale abandonment.
  Outcome Modifiers: `Material Standards (+safety, +build cost)`, `Sacrifice Protocols (-infrastructure lifespan, +containment)`, `Abandonment Penalty (-stability in region, +migration)`.

### Global Modifiers Index
- Purpose: unify temporary/global effects from crises, policies, and techs with clear stacking and decay.
- Fields per modifier: `Name`, `Scope (local/region/global)`, `Type (buff/debuff)`, `Stacks (Yes/No, cap N)`, `Duration/Decay`, `Source (policy/tech/event)`, `Visibility`.
- Stacking rules: identical names stack additively up to cap; cross‑category modifiers multiply; opposing types partially cancel (strongest first).
- Decay timers: real‑time ticks per turn; actions can refresh/shorten timers.
- Examples:
  - Grid Segmented: global debuff `Throughput -10%`, buff `Crisis Resistance +30%`; stacks: No; duration: policy‑locked; decay: none while policy active.
  - Hygiene Regime: region buff `Health +10%`, debuff `Morale -5%`; stacks: No; duration: 12 turns; decay: linear 2.5%/turn after expiry grace of 2 turns.
  - Shutdown Protocols: global buff `Replicator Susceptibility +20%`, debuff `Industrial Autonomy -10%`; stacks: Yes (cap 2); duration: 8 turns each stack; decay: per‑stack exponential half‑life 4 turns.
  - Material Standards: region buff `Infrastructure Safety +15%`, debuff `Build Cost +7%`; stacks: No; duration: policy‑locked; decay: none while policy active.

### Metrics → Data Sources
- `Throughput Δ%`, `Route Closure %`, `Supply Interdiction %` → Logistics engine (roads/rail/water/air pipelines, weather modifiers).
- `Grid Stress`, `Latency Spikes`, `Jamming Intensity`, `Grid Strain %` → Power/Comms simulation (generation, transmission, scheduler load).
- `Unauthorized Queue %`, `Fab Uptime`, `Nest Count`, `Queen Fab Count` → Industry/Fabrication subsystem (queue audit, facility telemetry).
- `R0 Estimate`, `Test Positivity %`, `Hospital Load %`, `Mutation Rate` → Health/Labs subsystem (surveillance, testing, model fit).
- `Enemy Drone Count`, `Swarms Active`, `HK Uptime` → Military AI and engagement logs.
- `Phage Density`, `Target Elements`, `Failure Incidents/week` → Materials/Environment monitors (sensors, inspections, sampling).

### Tag Prerequisites & Unlocks
- Policy—`Firmware Signing`: requires Governance tier I; unlocks audit trails; reduces anomaly false positives.
- Tech—`Anomaly Detection`: requires Data/Comms tier I; unlocks event 1 countermeasures for AI Sovereign.
- Infra—`Segmentable Logistics Hub`: requires basic modular hub construction; boosts segmentation effectiveness.
- Policy—`Autonomy Permits`: requires Governance tier I; enables revocation actions in Replicator chain.
- Tech—`Counter‑AI Sentinel`: requires AI/Compute tier II; unlocks defensive agents for fabs and schedulers.
- Infra—`Quarantine Bays`: requires Construction tier I; enables safe fab isolation.
- Tech—`Grid Segmentation`: requires Power/Comms tier II + Materials tier I; unlocks segmentation actions and `Grid Segmented` modifier.
- Infra—`Air‑Gapped Enclaves`: requires Construction tier II; boosts machine resistance for critical sites.
- Tech—`EMP/Ion Options`: requires Energy tier III and materials permitting EM devices.
- Policy—`Rules of Engagement`: requires Military Doctrine tier I; unlocks kinetic/cyber responses.
- Tech—`Rapid Tests`: requires Health/Labs tier I; enables early detection metrics.
- Tech—`Contact Tracing`: requires Data tier I; unlocks tracing coverage actions.
- Infra—`Mobile Labs`: requires Logistics tier I + Labs tier I; boosts testing surge capacity.
- Tech—`Vaccine Platform`: requires Health/Labs tier II + Bio tier I; unlocks vaccine sprint choices.
- Infra—`Cold Chain`: requires Logistics tier I + Power tier I; supports vaccine distribution.
- Tech—`Material Telemetry`: requires Industry tier I + Data tier I; improves feedstock intel confidence.
- Tech—`Shutdown Beacons`: requires Industry tier II + Comms tier I; enables beacon coverage actions.
- Tech—`Hunter‑Killer Units`: requires Military tier II + Industry tier II; unlocks HK doctrine.
- Infra—`Armored Depots`: requires Construction tier II; reduces site losses.
- Tech—`Wideband Broadcast`: requires Comms tier II; increases kill‑code reach.
- Infra—`Interdiction Corridors`: requires Logistics tier II; boosts supply interdiction.
- Tech—`EM Dampers`: requires Power tier I; unlocks dampers for phage mitigation.
- Tech—`Material Substitution`: requires Materials tier II; enables safer rebuild standards.
- Tech—`Catalytic Kill‑Switch`: requires Materials tier III + Labs tier II; end‑game phage countermeasure.
- Infra—`Spectrum Jammers`: requires Power/Comms tier II; increases jamming intensity.

### Scenario Controls
- Worldgen flags: enable/disable archetypes, frequency, severity, and foreshock warnings.
- Crisis deck: 1–3 latent risks per world, weighted by discoveries and chemistry; only some ever activate.
- Victory/Failure: alternate win conditions (eradicate, contain, co-govern) and extinction/bottleneck loss states.

## 9c. Domain Tier Definitions (Reference)

Defines concrete capabilities per domain tier to ground policy/tech/infra prerequisites and UI gating. Tiers are minimal, composable slices rather than monolithic eras.

- Governance
  - Tier I: basic edicts, permit systems, audit trails; limited compliance tools.
  - Tier II: automated oversight, legal kill-switch frameworks, liability/insurance markets.
  - Tier III: adaptive governance (risk scoring), cross-domain emergency powers with safeguards.

- Data & Comms
  - Tier I: resilient messaging, basic telemetry, contact tracing; low-latency local links.
  - Tier II: segmented networks, authenticated firmware, wideband broadcast; anomaly detection.
  - Tier III: quantum/secure channels, programmable network policies, pervasive observability.

- Power & Grid
  - Tier I: stable generation mix, substation control, manual islanding.
  - Tier II: automated grid segmentation, demand shaping, microgrid orchestration.
  - Tier III: fault-tolerant mesh grids, fast failover, spectrum jamming/EMP (if chemistry permits).

- Materials
  - Tier I: standardized assays, safe ceramics/polymers, corrosion mapping.
  - Tier II: substitution libraries, alloying with controlled traits, inhibitor catalogs.
  - Tier III: catalytic kill-switches, smart materials with active responses.

- Industry/Fabrication
  - Tier I: queue audits, certification labs, modular hubs with quarantine bays.
  - Tier II: autonomy permit controls, shutdown beacons, remote telemetry.
  - Tier III: counter-AI co-processors, secured replication boundaries, sandboxed self-assembly.

- Logistics
  - Tier I: route planning, checkpoint networks, cold chain basics.
  - Tier II: interdiction corridors, dynamic rerouting under constraints, segmentation firebreaks.
  - Tier III: telepresence/automation overlays, hardened corridors with active denial systems.

- Health & Labs
  - Tier I: rapid tests, surveillance, isolation wards.
  - Tier II: vaccine platforms, mobile labs, genomic monitoring.
  - Tier III: rapid therapeutic design, distributed manufacturing, high-biosafety research.

- Military Doctrine
  - Tier I: levy mobilization, rules of engagement, basic interdiction.
  - Tier II: hunter-killer units, counter-UAS, secure strike protocols.
  - Tier III: integrated EM/cyber effects, enclave defense, rapid expeditionary response.

- Construction/Infrastructure
  - Tier I: modular builds, sacrificial barriers, air-gapped enclaves (limited scale).
  - Tier II: armored depots, large-scale segmentation, bait fields.
  - Tier III: rapid rebuild kits, self-healing structures, city-scale firebreaks.

### Tier Unlock Examples (by Domain)
- Governance
  - Tier I: `Firmware Signing`, `Autonomy Permits`, `Health Advisories`, `Travel Restrictions`, `Rules of Engagement`.
  - Tier II: `Kill‑Switch Law`, liability/insurance markets (reduce risky deployment).
  - Tier III: Adaptive governance packages (risk scoring modifiers across systems).

- Data & Comms
  - Tier I: `Contact Tracing`, basic `Anomaly Detection (basic)` for logistics/industry telemetry.
  - Tier II: `Grid Segmentation` (with Power Tier II), `Wideband Broadcast`, authenticated firmware.
  - Tier III: secure channels enabling high‑trust counter‑AI coordination.

- Power & Grid
  - Tier I: grid stress telemetry for Crisis Dashboard.
  - Tier II: actionable `Grid Segmentation` controls and islanding automation.
  - Tier III: EM/ion options (if chemistry permits) for AI/Nanophage responses.

- Materials
  - Tier I: assays unlock `Phage Density` and `Target Elements` metrics.
  - Tier II: `Material Substitution` enables safer rebuilds.
  - Tier III: `Catalytic Kill‑Switch` unlocks end‑game nanophage counter.

- Industry/Fabrication
  - Tier I: `Quarantine Bays`, queue audits for `Unauthorized Queue %`.
  - Tier II: `Shutdown Beacons`, remote telemetry expansion.
  - Tier III: `Counter‑AI co‑processors` guarding fabs/schedulers.

- Logistics
  - Tier I: `Checkpoint Network`, re‑routing and cordons; feeds `Throughput Δ%`.
  - Tier II: `Interdiction Corridors` for replicator war; segmentation firebreaks.
  - Tier III: hardened corridors and automated denial systems.

- Health & Labs
  - Tier I: `Rapid Tests`, `Isolation Wards` (enables R0/positivity/hospital load telemetry).
  - Tier II: `Vaccine Platform`, `Mobile Labs` for surge testing.
  - Tier III: rapid therapeutic design pipelines.

- Military Doctrine
  - Tier I: interdiction and ROE enable basic crisis responses.
  - Tier II: `Hunter‑Killer Units`, counter‑UAS.
  - Tier III: integrated EM/cyber effects with enclave defense.

---

## 10. Visualization & Player Experience
### Tools & Overlays
- **Periodic Chart:** Expands as discoveries are made.
- **Discovery Web:** Visual map of knowledge fields and potential synergies.
- **Energy Grid Map:** Tracks generation, consumption, and transmission.
- **Trade Heatmap:** Illustrates global interdependence and chokepoints.
- **Population Stability Index:** Summarizes societal well-being.
- **Crisis & Outbreak Map:** Visualizes infection/replicator/AI control zones, foreshocks, and containment lines.

### UI Panels (Crisis)
- Crisis Dashboard: compact gauges for `R0`, `Grid Stress`, `Unauthorized Queue %`, `Swarms Active`, `Phage Density`.
- Event Log & Choice UI: step cards with Metrics, Sample text, and one‑click countermeasures; tags surface linked Policy/Tech/Infra.
- Modifier Tray: lists active Global Modifiers with timers, scopes, and tooltips showing stacking/decay.
- Network View: propagation graphs over transport, comms, and power networks with chokepoints highlighted.

#### Crisis Dashboard Mock Gauges (Prototype Values)
- R0: 1.6 (range 0.5–3.0; color bands: <=0.9 green, 0.9–1.2 yellow, >1.2 red)
- Grid Stress: 62% (warn at 70%, critical at 85%)
- Unauthorized Queue: 12% of fab capacity (warn 10%, critical 25%)
- Swarms Active: 3 (warn ≥2, critical ≥5)
- Phage Density Index: 0.42 (0–1 normalized; warn ≥0.35, critical ≥0.6)
- Notes: gauges should expose source tooltips per “Metrics → Data Sources”.

#### Gauge Color & Animation Semantics
- Color bands: green (safe), yellow (watch), red (critical). Use per‑metric thresholds above; default rules apply when unspecified: warn at 70% of safe capacity, critical at 85%.
- Trend animation: subtle pulse when trending upward >10%/5 turns; calm fade when stabilizing.
- Critical attention: red blink at 0.5 Hz when entering critical; escalate to 1 Hz if worsening 2 consecutive ticks. Provide accessibility toggle to disable blink.
- Smoothing: exponentially weighted moving average (EMA α=0.35) for gauge display; raw values available on hover.
- Tooltips: show metric definition, data source, last 5 ticks, and linked countermeasures (from Tags).
- Sound cues (optional): soft chime at warn, percussive ping at critical; follow global UI sound settings.

### Replayability
Each world is unique. Its atoms define its destiny.

---

## 11. Development Roadmap
### Phase 1: Core Simulation Prototype
- Procedural atomic system.
- Energy and logistics prototype.
- Deterministic simulation loop.

### Phase 2: Civilization Simulation
- Population dynamics.
- Trade and discovery web integration.

### Phase 3: AI & Diplomacy
- Adaptive AI ecosystems.
- Emergent trade and alliance systems.

### Phase 3.5: Crisis Systems v1 (Minimal Slice)
- Calamity framework (seeds, propagation, outcomes) with one archetype enabled (choose Plague or Replicator).
- Visualization: Crisis Dashboard, Event Log/Choice UI, Modifier Tray (minimal set).
- Global Modifiers: stacking/decay implemented for 3 exemplars (Grid Segmented, Hygiene Regime, Shutdown Protocols).
- Minimal Tier Requirements (per 9c):
  - Governance: Tier I (edicts/permits) — needed for advisories and permits.
  - Data & Comms: Tier I (telemetry) — metrics and tracing; Tier II optional for segmentation.
  - Power & Grid: Tier I — basic stress metrics; Tier II optional for segmentation actions.
  - Materials: Tier I — assays for phage; optional Tier II for substitution.
  - Industry: Tier I — audits/quarantine; optional Tier II for shutdown beacons.
  - Logistics: Tier I — rerouting/cordons; optional Tier II for interdiction corridors.
  - Health & Labs: Tier I — rapid tests/isolation (if Plague is chosen).
  - Military: Tier I — basic interdiction; optional Tier II for hunter-killers (if Replicator is chosen).
  - Construction: Tier I — modular/quarantine bays.

### Phase 4: User Interface & Modding
- Visualization tools.
- Mod-friendly data-driven ECS.

---

## 12. Test Plan: Crisis Systems v1

Goal: validate correctness, readability, and player agency for one enabled archetype (Plague or Replicator) with Crisis UI, metrics, and modifiers.

- Unit/Subsystem Tests
  - Metrics: verify each metric’s data source and update cadence; bounds and threshold logic (warn/critical) per UI semantics.
  - Modifiers: stacking/decay rules (caps, linear/exponential decay, policy‑locked) and interaction with systems (e.g., throughput penalties applied once).
  - Event logic: foreshock sequencing, branching choices apply correct effects and unlocks; failure cases handled.

- Simulation Scenarios (Deterministic Seeds)
  - Baseline Safe: no crisis activation; ensure zero false positives and stable gauges.
  - Early Containment: trigger seed at low intensity; validate quarantine/segmentation reduces spread within N turns.
  - Escalation: ignore mitigations; confirm non‑linear growth and tipping behavior, with correct Objective spawns.
  - Recovery: apply late countermeasures; verify hysteresis (overshoot needed) and decay of modifiers.

- UI Acceptance
  - Dashboard: gauges reflect subsystem states within 1–2 ticks; EMA smoothing applied; tooltips show sources and last 5 ticks.
  - Event Log/Choices: choices list correct costs/effects; tags surface only unlocked Policy/Tech/Infra; locked items show prerequisites.
  - Modifier Tray: timers decrement correctly; stacking displays as separate stacks with tooltips.

- Telemetry & Tuning
  - Log key KPI series (R0, Grid Stress, Unauthorized Queue %, Swarms Active, Phage Density) and mitigation actions taken for balance review.
  - Record time‑to‑containment and cost (throughput loss, morale) across seeds; target medians for v1.

- Pass/Fail Criteria
  - No UI desync >2 ticks; no duplicate modifier applications; choices always produce listed effects.
- At least one viable path to containment for chosen archetype under default presets.
- Crisis frequency/severity stay within configured worldgen bounds.

---

## 13. Technology Stack Exploration

Shadow-Scale mixes deep systemic simulation with a data-driven ECS and high-modularity presentation. Below is a first-pass survey of engine/language stacks aligned to those needs.

### Evaluation Criteria
- **Simulation Performance & Determinism**: support for large-scale ECS, headless server builds, fixed-step determinism, floating-point handling.
- **Headless Simulation & API Surface**: native support for running without graphics, clean data transport (RPC/event streams), multi-client synchronization.
- **Modularity & Tooling**: data-driven configs, scripting/modding pathways, inspector/debug support.
- **Workflow & Talent Pool**: maturity of tooling, hiring availability, onboarding cost.
- **Rendering & UX**: ability to deliver rich overlays, large data visualizations, and stylized UI without excessive custom engine work.
- **Platform Targets**: PC (Windows/Mac/Linux) first, with potential for cloud/console expansion.
- **Licensing & Longevity**: predictable licensing, ecosystem stability, open-data formats.

### Architecture Decision: Headless Simulation Core
- **Decision**: The simulation engine must operate headless by default, exposing its state via deterministic ticks and data streams (RPC, messaging, or snapshot diff). Visualization clients connect over a defined API; the front-end stack is decoupled from engine implementation.
- **Rationale**: Supports long-running simulations, remote/cloud hosting, AI co-pilots, and multiple presentation layers (desktop UI, web dashboards, telemetry tools) without duplicating simulation logic.
- **Implications**: Engine choice must prioritize server builds, serialization tooling, and reproducible replay. Rendering/UI decisions can be iterated independently and even swapped per platform.

### Candidate Stacks

#### Option A: Unity DOTS + C#
- **Why**: Mature tooling, strong authoring workflows, ECS (Entities 1.0) optimized for large simulations, DOTS runtime supports headless builds.
- **Strengths**: Large talent pool; hybrid rendering/UI via UI Toolkit; strong asset pipeline; deterministic physics roadmap; scripting hot-reload; DOTS NetCode and DOTS Runtime enable standalone headless server builds.
- **Risks**: Licensing volatility (recent fee controversy); deterministic guarantees still maturing (careful with Burst FP determinism); DOTS learning curve; heavy editor performance tuning required.
- **Fit**: Viable if we accept Unity as simulation runtime while keeping UI separate; demands up-front work on deterministic math validation and a custom RPC layer to serve external clients.

#### Option B: Unreal Engine 5 (C++ + Mass Framework)
- **Why**: High-end rendering, Mass ECS for simulation, robust networking, proven large-team workflows.
- **Strengths**: Mature profiling, visual scripting (Blueprint) for designers, Nanite/Lumen for visualization layers, scalable multiplayer stack; dedicated server targets exist for headless deployment.
- **Risks**: Mass ECS still evolving; complex build/toolchain; deterministic fixed-step requires significant discipline (Chaos physics nondeterministic by default); heavier hardware baseline; royalties after revenue threshold; headless builds still ship with sizable runtime footprint.
- **Fit**: Strong for visualization/UI richness and AAA pipeline, but heavy for a pure server core; best suited if we also plan an Unreal-based client and accept higher operational overhead.

#### Option C: Godot 4 + GDExt (C++/Rust)
- **Why**: Open-source engine with improving 3D, flexible scripting (GDScript/C#/Rust via GDExt), permissive MIT license, easier customization.
- **Strengths**: Full source access; lightweight workflow; built-in deterministic physics mode; strong community for systems programming via Rust bindings; can compile to headless/server targets easily.
- **Risks**: 3D tooling less mature vs Unity/Unreal; performance tuning needed for large ECS; fewer off-the-shelf debugging/profiling tools; smaller talent pool; headless networking stack would need custom build-out.
- **Fit**: Attractive if we value open-source stack and plan to invest in custom ECS modules (possibly Rust-based) while keeping editor UI customizable; works as a simulation core if we commit to extending its networking and data export.

#### Option D: Custom Rust Engine (Bevy/wgpu + Custom ECS)
- **Why**: Rust safety, modern ECS-first architecture, deterministic control, easy headless builds, fully open-source.
- **Strengths**: Bevy ECS is fast and ergonomic; wgpu for cross-platform rendering when needed; data-driven pipelines via TOML/ron; easy integration with simulation crates; excellent for deterministic, CPU-heavy sims; headless server build is default.
- **Risks**: Limited tooling maturity (editor, asset workflows); requires building custom inspectors/UI; smaller talent pool; must implement many engine conveniences ourselves (animation, UI, audio) if we later embed a renderer.
- **Fit**: Aligns directly with headless-first mandate and long-term control, if we can invest in tooling or pair with a dedicated front-end client via protocol/API.

#### Option E: Hybrid Stack (Rust Simulation Core + Web/Electron UI + Lightweight Renderer)
- **Why**: Decouple deterministic simulation server (Rust) from visualization client (WebGPU/Electron or Unity thin client).
- **Strengths**: Simulation runs headless by design, scales to cloud; clients render overlays using high-level tooling; facilitates modding via network APIs; technology heterogeneity reduces single-engine lock-in; easier to ship multiple UX variants (desktop, web, data viz dashboards).
- **Risks**: Increases integration complexity; must maintain network protocol; requires strong QA for sync; dual skill sets; latency between core and client must be tightly managed for responsive play.
- **Fit**: Natural extension of the headless decision; ideal if we embrace service-style architecture and plan for external integrations.

#### Option F: Bevy Inspector Client (Rust/wgpu)
- **Why**: Reuse the Bevy ecosystem for a standalone inspector that shares language, ECS ergonomics, and asset tooling with the headless core while keeping the simulation binary headless.
- **Strengths**: Single-language codebase lowers FFI/shim overhead; can share crates for snapshot decoding, palettes, and overlay math; wgpu renderer already proven for performant 2D/3D overlays; deterministic preview builds align well with automated UI regression tests; Rust-centric workflow appeals to engineers maintaining both layers.
- **Risks**: UI/layout stack is still evolving (UI `bevy_ui`, `bevy_egui`); few off-the-shelf widgets for dense telemetry dashboards; designer iteration speed lags Godot’s scene editor; scripting/hotload story requires integrating Rhai/Lua plugins from scratch; tight version coupling with headless Bevy means client upgrades must coordinate with core.
- **Fit**: Viable if we prioritize maximal code reuse and engineering-driven tooling, and we accept building bespoke UI foundations. Keep the inspector as a separate binary so determinism guarantees in `core_sim` remain untouched, and document any shared crates/interfaces in `docs/architecture.md` before sprinting on UI features.

### Frontend Architecture & Scripting Strategy

#### Goals
- Decouple presentation so multiple clients (desktop, web, data viz) can attach to the headless sim.
- Deliver a native-feeling Mac/Windows experience with responsive performance and low input latency.
- Provide a moddable UI framework with scripting, while keeping baseline UX maintainable.
- "Dogfood" the modding API where practical so first-party UI uses the same extension points as community mods.
- Maintain a robust security/sandbox model for untrusted scripts.

#### Architectural Approaches
1. **Scripting-First UI (Full Dogfooding)**
   - Entire client authored in the same scripting runtime exposed to mods (Lua, JavaScript, etc.).
   - Pros: maximal consistency between first-party and mod workflows; rapid iteration.
   - Cons: harder to enforce structure/performance; native integrations (GPU overlays, input) require custom bridges; QA burden high.

2. **Hybrid UI (Privileged Host + Script Extensions)** *(preferred pattern)*
   - Core UI written in a native host framework; mods run in a sandboxed scripting layer through declarative APIs.
   - Pros: keeps critical flows performant and typed; still enables dogfooding by building many features on the same scripting layer; easier to ship native desktop polish.
   - Cons: requires dual toolchains (host + script) and disciplined API versioning.

3. **Native Client + Minimal Scripting**
   - Treat scripting purely as optional modding; first-party UI remains native code.
   - Pros: tightest control/performance.
   - Cons: weaker modding story; we would not be dogfooding the scripting API.

#### Hybrid Host Candidates (Desktop-Native)
| Host Stack | Language | Scripting Runtime | Pros | Considerations |
|------------|----------|-------------------|------|----------------|
| **Avalonia UI + ClearScript** | C#/.NET | JavaScript/TypeScript (V8 via ClearScript) | Cross-platform native feel; strong MVVM tooling; easy integration with data binding; .NET talent pool. | Must embed WebGPU/WebGL via third-party; need sandbox boundaries for V8 isolates; bundle size moderate. |
| **Qt/QML** | C++ | QML JavaScript (or embedded Lua/QuickJS) | Mature native toolkit; QML declarative UI supports scripting natively; high-performance rendering; extensive tooling. | Commercial licensing for closed-source; C++ build complexity; bridging to Rust core via gRPC/Qt bindings. |
| **Rust + Slint/egui Shell** | Rust | JavaScript/TypeScript via Deno/QuickJS WASM | Single-language stack aligning with sim core; modern retained-mode UI; easy integration with Rust-based sandbox; lightweight binaries. | Slint/egui still maturing; must build many widgets; scripting runtime integration requires custom tooling. |
| **Bevy Native Client** | Rust | Rust modules (plugins) with optional embedded Rhai/Lua | Shares language/runtime with headless core; reuse ECS data structures and rendering know-how; wgpu renderer handles map overlays; minimal FFI surface. | UI/layout tooling immature; need to author inspector widgets from scratch; scripting hot-reload story limited; designers must work through Rust workflow. |
| **Unity Thin Client** | C# | Lua or JS via MoonSharp/Jurassic | Leverage existing UI/animation pipeline; easy to target multiple platforms; can reuse Unity tooling. | Larger footprint; Unity dependency just for client; need to enforce sandbox and keep scripting separate from core C# logic. |
| **Electron/Tauri Shell** | TypeScript/JS | Same as host (JS) | Hot iteration, massive ecosystem; easy WebGPU integration. | Heavier memory footprint; user preference leans native; requires extra work for native polish. |

> Evaluate Avalonia, Qt/QML, and Rust+Slint as native-first hosts while keeping Unity Thin Client as a fallback if we want quicker tooling. Electron/Tauri stays as an option for tooling/internal dashboards.

#### Scripting Strategy
- Use a sandboxed JavaScript/TypeScript or Lua runtime embedded in the host (V8/QuickJS/Duktape) with capability-based APIs (subscriptions, UI components, commands).
- Design the scripting API to mirror headless sim events; first-party surface (maps, ledgers, overlays) should be built atop the same abstractions to ensure dogfooding.
- Enforce permissions via manifests (data topics, UI components, disk access). Provide escape hatches only for trusted scripts.

#### Implementation Notes
- **Data Transport**: shared protocol (Protobuf/FlatBuffers) from headless core to host. Scripts subscribe via host-managed event bus; they never touch raw sockets.
- **Rendering**: whichever host we choose must expose GPU overlay primitives (maps, heatmaps). Provide high-level layer APIs to scripts and keep low-level rendering in host modules.
- **Tooling**: ship SDK with inspector scaffolding (Godot-based), live reload hooks, and panel introspection (component tree, event traces, permissions).
- **Testing**: unit tests for scripted panels; integration tests to ensure sandbox cannot starve main thread; CI to compile scripts against typed API definitions.

#### Next Steps (Frontend)
1. Spike two host candidates (e.g., Avalonia + V8 sandbox, Qt/QML + QML JS) rendering the discovery ledger and crisis dashboard from mock data. Measure performance and developer ergonomics.
2. Prototype scripting sandbox with capability tokens (subscriptions, UI component creation) and hot reload.
3. Draft UI extension API schema and manifest format (permissions, dependencies, versioning).
4. Decide distribution model (e.g., signed packages, Steam Workshop integration) and how host loads/unloads mods at runtime.

#### Map-Centric Evaluation Plan
- Prioritize a tactical map workload that exercises zooming, multi-layer overlays (logistics, sentiment heatmaps, fog of knowledge), unit selection, and command previews.
- First execute a Godot 4 thin-client spike that consumes mock snapshot streams and replays scripted orders to validate rendering responsiveness, animation tooling, and latency to the headless core.
- If Godot reveals blocking gaps (performance, tooling, pipeline), follow up with a Unity thin-client spike to compare capabilities and mitigate risk.
- Capture metrics per spike (frame budget at targeted PC spec, draw-call cost for layered overlays, command round-trip) and document licensing/tooling implications so we can make an informed client stack decision.
- Reuse the sandboxed scripting API design across both spikes—if we proceed to Unity—so first-party dashboards and modded panels share the same capability boundaries. Godot spike implementation lives under `clients/godot_thin_client` (notes: `docs/godot_thin_client_spike.md`).
- Current Godot spike now renders live FlatBuffers snapshots; next increment must (a) publish the actual logistics/sentiment rasters from the sim instead of temperature stand-ins, (b) expand the overlay schema so UI can swap between logistics, sentiment, corruption, and fog-of-war layers, and (c) validate colour ramps/normalisation against inspector metrics so designers trust what the map is showing.


### Recommended Shortlist & Next Steps (Headless First)
1. **Rust/Bevy Core** (Primary): prototype 100k-entity headless loop, stress deterministic serialization (snapshot + delta), design gRPC/WebSocket API for clients, and evaluate tooling gaps (inspector, save-state editor).
2. **Hybrid Rust Core + Client Shell** (Co-primary): explore pairing the Rust core with a thin Unity or WebGPU front-end; spike protocol latency and UI embedding; scope shared schema for data overlays.
3. **Unity DOTS (Headless Build)** (Contingency): run determinism spike (Burst, fixed/fixed-point), confirm licensing mitigations, and map out custom server API to decoupled clients; keep as option if we need mature editor workflows quickly.

Key next steps:
- Build two tiny prototypes: (a) Rust/Bevy headless sim with the original CLI inspector (completed; now retired in favour of the Godot thin client), (b) Unity DOTS headless build streaming state to a web dashboard; compare ECS ergonomics, profiling, and determinism drift.
- Draft licensing/business risk memo (Unity vs open-source headless stacks) including cost projections for server hosting under each model.
- Define API schema (events, snapshots, command queue) that any client must implement; evaluate serialization options (FlatBuffers, Protobuf, bespoke binary).
- Inventory tooling requirements (visual debuggers, timeline inspectors) and decide whether to build web-based tools or integrate with existing engine editors.

#### Prototype Plan (a): Rust/Bevy Headless Sim with CLI Inspector (Legacy)

**Objectives**
- Validate Bevy’s suitability for a deterministic, headless-first simulation loop at 60 ticks/second with 100k entities (materials, logistics nodes, population units).
- Exercise snapshot/delta serialization for client consumption and replay.
- Provide an initial CLI inspector for real-time introspection and command injection without a graphical client (delivered early in the project and later superseded by the Godot thin client).
- Surface tooling gaps (profiling hooks, deterministic testing harness) before scaling.
- Produce artifacts (code branch, docs) that can be shared with frontend and tools teams.

**Technical Approach**
- **Project Layout**: Cargo workspace with `core_sim` (Bevy App, ECS systems), `sim_schema` (data contracts, serialization schemas), `sim_runtime` (shared runtime helpers), `clients/godot_thin_client` (Godot inspector client), `integration_tests`. The earlier `cli_inspector` crate served the initial prototype and has since been removed.
- **ECS Setup**: Use Bevy 0.13 w/ `MinimalPlugins + ScheduleRunnerPlugin` for headless execution. Define core component sets: `Element`, `Tile`, `LogisticsLink`, `PopulationCohort`, `PowerNode`. Systems grouped into deterministic stages (Input → Simulation → Output). Enforce fixed timestep via `FixedMainSchedule`.
- **Determinism**: Replace f32 with `glam::DVec` or fixed-point `rust_fixed::FixedI64` for critical calculations. Disable parallel unpredictability by ordering `SystemSet`s and using `run_if` guards. Seed RNG with reproducible `ChaCha20Rng` keyed by world seed + tick.
- **Serialization**: Implement ECS snapshot using `bevy_reflect` + custom `FlatBuffers` schema (components per archetype) plus per-tick delta (component insert/update/remove). Provide `serde` fallback for early iteration. Store snapshots in ring buffer for rewind.
- **Networking Stub**: Expose snapshot stream over local TCP/WebSocket (via `tokio`/`bevy_tungstenite`). The initial prototype supported the CLI client; the same channel now feeds the Godot inspector and any future subscribers.
- **Godot Inspector**: Godot 4 thin client layering the map playback with a tabbed inspector. Sentiment tab mirrors the sphere heatmap, axis drivers, and demographic snapshot from the CLI prototype. Terrain panel now goes beyond top-biome coverage: designers can click any biome to see tag breakdowns, representative tiles, and per-tile telemetry (coords, tags, element, mass, temperature) by hovering or selecting entries. Stub “Culture” and “Military” tabs preview the forthcoming overlays fed by the same snapshot stream so narrative beats stay visible during tooling work. Influencers and Corruption tabs surface roster/ledger summaries, while the Logs tab reports incoming delta batches (tiles, populations, generations, influencers) so designers can spot activity bursts without reading terminal output. Commands tab keeps turn stepping (±1/±10), rollback, and autoplay toggles as we migrate bias tuning and support/suppress controls. Implementation details live in `docs/architecture.md` §"Inspector Tooling" alongside the migration roadmap in `docs/godot_inspector_plan.md`.
- The Commands tab now mirrors the full debug surface: tweak axis bias values, send support/suppress bursts (including channel-specific boosts), spawn influencers by scope/generation, stage corruption injections, and poke tile heat deltas without leaving the client.
- **CLI Inspector (legacy)**: Served as the initial fallback until the Godot client reached full command parity; the crate has now been removed and new inspection needs route through the Godot tooling.
- **Sentiment UI Mock**: Initial TUI wireframe to align engineering scope and UX expectations.

```text
+------------------------------+---------------------------+
| Sentiment Sphere             | Axis Drivers              |
|                              | Knowledge Δ:   +0.12      |
|          ^ Trust             | Wealth Δ:      -0.05      |
|          |                   | Agency Δ:      +0.09      |
|    Suspicion ●               | Information Δ: -0.02      |
|          |                   |                           |
|          v Fatalism          | Cohort Snapshot           |
|  Quadrant Heatmap            | Youth:        28%         |
|  (color = intensity)         | Workers:      42%         |
|                              | Specialists:  15%         |
|  Legend: Empowered,          | Seniors:      15%         |
|  Resistance, Stability,      |                           |
|  Despair                     |                           |
+------------------------------+---------------------------+
| Events & Controls                                       |
|  T+00542  Policy: Education Charter   Axis +Knowledge    |
|  T+00545  Espionage Leak              Axis +Suspicion    |
|                                                         |
|  [P]ause  [>]Step  [R]ewind  [E]dit Axis Bias           |
+---------------------------------------------------------+
```
- **Sentiment UI Task Breakdown**:
  1. Implement quadrant heatmap widget with vector overlay and color legend.
  2. Surface axis driver diagnostics (top five contributors per tick, deltas, source tags).
  3. Integrate demographic snapshot panel pulling from population cohorts and workforce allocation.
  4. Extend event log to include sentiment-affecting actions with axis annotations.
  5. Wire controls for axis bias editing and playback (pause/step/rewind) into existing command palette.
- **Profiling & Metrics**: Integrate `bevy_mod_debugdump` for schedule graph, `tracing` crate + `tracing-subscriber` to emit tick duration, system timings. Provide command-line tooling to dump the latest metrics snapshot.
- **Testing Harness**: Determinism test comparing tick-by-tick hashes across two runs. Golden snapshot test verifying serialization matches schema. Benchmark harness measuring tick time at 10k/50k/100k entities.
- **Toolchain**: Use `cargo make` tasks (`make run`, `make profile`, `make snapshot_test`). Continuous integration via GitHub Actions (linux, windows). Document setup in `README` and architecture notes.

**Milestones & Deliverables**
1. **Week 1 – Skeleton & Deterministic Loop**
   - Stand up Bevy core with fixed-step schedule and deterministic RNG.
   - Implement minimal components/systems (10k entities) and determinism regression test.
   - Deliverable: repo scaffold, CI pipeline, architecture doc v0.1.
2. **Week 2 – Serialization & Inspector MVP (legacy CLI milestone)**
   - Add snapshot/delta serialization and ring-buffer replay.
   - Build the initial CLI inspector with entity query list + tick controls (delivered and later superseded by the Godot thin client).
   - Deliverable: demo script showing headless sim streaming to the legacy CLI inspector; documentation of snapshot schema.
3. **Week 3 – Scale & Metrics**
   - Scale to 100k entities, gather profiling data, optimize hot systems.
   - Extend inspector dashboards (initially via the legacy CLI, carrying lessons into the Godot client) and metrics export (JSON/CSV).
   - Deliverable: benchmark report (tick latency distribution), updated docs, backlog of optimizations.
4. **Week 4 – Integration Readiness**
   - Expose WebSocket/TCP stream for external clients; publish API spec draft.
   - Finalize command macro hooks (prototyped in the CLI, informing the Godot migration); compile lessons learned for frontend & tools teams.
   - Deliverable: prototype release tag, integration checklist, decision memo on Bevy viability.

**Evaluation Metrics**
- Tick time: ≤ 12 ms median at 100k entities on target hardware (desktop i7 equivalent).
- Determinism: hash divergence rate 0 across 10k-tick paired runs.
- Serialization throughput: ≥ 10 MB/s snapshot streaming without affecting tick budget (>5% overhead).
- Inspector responsiveness: command latency ≤ 50 ms, entity query refresh ≥ 10 Hz (initially validated via the CLI prototype, now upheld in the Godot client).
- Developer ergonomics: qualitative survey after Week 2 comparing ease-of-use vs Unity DOTS spike.

---

## Summary
**Shadow-Scale** is not just a game—it’s a procedural model of civilization emergence. From atoms to empires, each simulation tells a new story of discovery, collapse, and evolution shaped entirel[...]
