# Map Generation TODOs

- [x] Thread a worldgen seed through the map pipeline so elevation, landmask growth, and island placement become reproducible-but-variable per run.
- [x] Add a tectonics pass after macro landmask growth to derive collision belts, fault lines, volcanic arcs, and plateau/dome candidates.
- [x] Stamp long fold-mountain ranges along plate-collision boundaries with configurable belt width/strength.
- [x] Generate fault-block ranges and uplifts along interior fault lines for broken mountain chains (e.g., Sierra Nevada analogues).
- [x] Seed sparse volcanic peaks/arcs on subduction and ridge zones; surface them with dedicated volcanic terrain tags.
- [x] Re-stamp elevation using the tectonic mountain mask before shelf classification so hydrology, rain shadows, and coastal bands respect new relief.
- [x] Dial back volcanic arc saturation on sprawling coastlines so peaks taper instead of covering entire shelves.
- [x] Add a gentle erosion/smoothing pass post-tectonics to widen coastal lowlands and reduce cliffy transitions.
- [x] Introduce polar microplate variation so high-latitude landmasses get a mix of uplifted and low relief zones.
- [x] Integrate mountain metadata into the moisture model (windward lift, leeward drying) to create realistic rain shadows.
- [x] Update biome classification to honor structural mountain tiles (fold/fault/volcanic/dome) before applying climate/moisture-driven biomes.
- [x] Capture regression seeds per preset with target metrics (land ratio, mountain counts, polar stats) and wire them into tests.
- [x] Enhance dome/high plateau styling with secondary microrelief (rims/terraces) so plateaus read distinctly.
- [x] Audit biome/tag cohesion: ensure new highland tags propagate into moisture, climate bands, and tag budget scoring. (Windward moisture pass, highland terrain overrides, tag solver regression test.)
- [x] Add automated checks that terrain tag solver stays within preset tolerance across representative seeds. (see `locked_tag_solver_respects_tolerances_across_representative_seeds` covering earthlike & polar_contrast seeds)
- [x] Surface mountain type + relief metadata in inspector overlays or debug exports for quick iteration. (inspector tile panels now show mountain kind + relief and tile list previews include mountain labels)
- [ ] Documentation remains untouched: TASKS-mapgen.md, docs/architecture.md, and the narrative manual should be updated once the feature work is finalized.

### Follow-up Backlog
- [x] Allow presets to declare `locked_terrain_tags` so the solver keeps a small set of families within tolerance, and add regression tests for those locks (earthlike + polar contrast).
- [ ] Evaluate how many additional locks we can realistically support per preset without oscillation, and document tuning guidance for designers.
- [ ] Capture and document actual-vs-target terrain tag ratios for key presets/seeds to quantify current drift ahead of solver work.

---

## Next Session Brief: Mapgen Regression & Styling Follow-ups

### Context
- We're working inside `/Users/raywalker/source/falcon` on the map generation pipeline (Rust, Bevy ECS).
- Recent updates added polar microplates, structural biome overrides, and new unit tests in `core_sim`. Pending items are now listed above.
- Mountain metadata (relief scale + type) is covered by tests; `cargo test -p core_sim` must stay green. No river work in this session.

### Goals for the Session
1. **Regression Metrics**: For each built-in preset (at minimum `earthlike` & `polar_contrast`; optionally a small-map preset) capture a deterministic seed run. Record/assert land ratio, fold/fault/volcanic counts, and polar microplate stats. Wire these checks into automated tests so tectonic tweaks don't drift silently.
2. **Plateau Styling**: Prototype a secondary pass adding microrelief to dome/high plateau tiles (rim cliffs/terraces). Expose knobs in presets; validate via logs/screenshot.
3. **Cohesion Audit & Tag Budgets**: Ensure structural highland tags propagate through moisture/climate/tag solvers. Add unit/integration tests that terrain tag budgets stay within preset tolerance for a representative seed.
4. **Inspector Metadata**: Surface mountain type + relief scale in a debug overlay or export (code hook is enough; full UI polish not required).

### Key References
- `core_sim/src/mapgen.rs` (tectonics, restamp, new tests)
- `core_sim/src/systems.rs` (biome classifier overrides)
- `core_sim/src/data/map_presets.json`
- `TASKS-mapgen.md` (this file)
- Tests added previously: `mapgen::tests::polar_contrast_preset_builds_bands`, `systems::tests::*` overrides.

### Guardrails
- Keep `cargo test -p core_sim` green—extend the suite rather than removing assertions.
- No river-generation work yet.
- Preserve determinism of existing seeds.

### Deliverables
- New/updated tests covering preset regression metrics and tag tolerance.
- Plateau styling prototype behind knobs with brief docs/comments.
- Inspector/overlay hook exposing mountain type + relief.
- Documentation/task updates once work is complete.
- [x] Document the tectonic + mountain workflow in `docs/architecture.md` and cross-link preset knobs.
