use super::*;

/// **The crew a pen-extension ring is credited with.** `ExtendPen` is command-driven rather than
/// assignment-driven, so it has no worker count of its own to hand [`RungDef::build_accrual`] — and
/// it does not need one: the `animal:pen` rung declares no `crew_needed` (the animal web sizes a crew
/// off the herd, via `fauna::herders_needed`, not off the rung), so the crew factor is the identity
/// whatever is passed. Named so that the day an animal rung *does* declare a crew, this site reads as
/// an assumption to revisit rather than a silently under-crewed fence.
const PEN_EXTEND_CREW: u32 = 1;

/// **A rung-3 managed source is TENDED, so a crew standing on it is working it** — the `eligible`
/// term a Field's and a pen's [`credit_rung_lesson`] pass where the extractive rungs ask
/// [`crew_is_working_the_source`].
///
/// At rung 3 the source is *yours*: the keeper feeds, herds and minds it every turn, and the harvest
/// is a consequence rather than the work. There is no escapement room to ask about either — a
/// managed source pays its `managed_production` at every floor and is never drawn down — so the
/// extractive predicate has nothing to read here. Reaching either branch already requires
/// `workers > 0`.
const MANAGED_SOURCE_IS_TENDED: bool = true;

/// **"Is this crew actually working the source?"** — THE eligibility term that replaced the
/// `EcologyPhase::Thriving` gate on both webs (`docs/plan_harvest_floor.md` §3.2), asked of the
/// **escapement room**: is there anything standing above this assignment's floor?
///
/// # It is the CEILING, deliberately, and not the take
///
/// The obvious spelling is `take > 0`, and on the plant web the two coincide — a gather is
/// continuous, so any positive room yields a positive take. **On the animal web they do not**, and
/// the difference is a quantisation artifact rather than a fact about work.
/// [`crate::fauna::quantise_animal_take`] rounds to whole animals, so a herd whose room is 60 biomass
/// against an 80-unit body hands over **nothing** this turn while the crew tracks, culls and handles
/// it exactly as they did last turn. Reading `AnimalTake::killed == 0` as *"not working"* would make
/// the learning and build rates depend on `body_mass`: big-bodied species would tame and teach
/// several times slower than small ones, for a reason nobody designed and nothing measured.
///
/// Asked **in biomass, before quantisation and before the worker cap** — the number
/// `forage::forage_escapement_ceiling` / `fauna::hunt_escapement_ceiling` returns — so it is the same
/// question on both webs and the rule stops being web-specific.
///
/// It still separates the two cases the gate exists to separate:
/// - **nothing stands above your floor** → you are watching, not working. No lesson, no build. That
///   is also what makes `floor = 1.0` degenerate: the room is `0` by construction.
/// - **there is surplus you have not yet banked into a whole body** → you are working it, and the
///   pulse the quantiser pays is about *when* the food lands, not whether the crew showed up.
///
/// The other degenerate end is [`crate::intensification::learn_multiplier`]'s, not this one's:
/// `floor = 0` leaves nothing standing, so the *rate* is zero however much room there was.
fn crew_is_working_the_source(standing_above_floor: f32) -> bool {
    standing_above_floor > NOTHING_STANDS_ABOVE_THE_FLOOR
}

/// **An empty escapement room** — the value [`crew_is_working_the_source`] compares against, named
/// because `0.0` as a bare literal there reads as an arbitrary epsilon rather than as the exact
/// boundary `max(0, B − floor·K)` is clamped at.
const NOTHING_STANDS_ABOVE_THE_FLOOR: f32 = 0.0;

/// **Credit the lesson the source's rung teaches, at the rate its crew's floor earns.** The caller
/// side of [`RungDef::knowledge_accrual`]: the rung says *what* is learned and *how much*, this
/// applies it to the ledger.
///
/// It exists as a function rather than as one hoisted call because **`eligible` is not knowable
/// until the source's own branch is reached** — a Field and a pen answer it differently from a wild
/// stand (see [`MANAGED_SOURCE_IS_TENDED`]), and the extractive branches need the escapement room
/// resolved against the pre-take biomass. Each of the four branches (a Field, a wild gather, a pen's
/// tend, a wild hunt) calls this with its own answer; the *rule* stays in one place so the two webs
/// cannot drift.
fn credit_rung_lesson(
    rung: &RungDef,
    floor: f32,
    eligible: bool,
    knowledge: &LadderKnowledge,
    faction: FactionId,
    discovery: &mut DiscoveryProgressLedger,
) {
    if let Some((lesson, amount)) = rung.knowledge_accrual(floor, eligible, knowledge) {
        discovery.add_progress(faction, lesson, scalar_from_f32(amount));
    }
}

/// The config handles [`advance_labor_allocation`] reads, bundled into one `SystemParam` so the
/// system stays under Bevy's 16-parameter ceiling as new configs join it (Predators Phase 0 added
/// combat + creatures). Each is resolved to its `Arc` once at the top of the system.
#[derive(bevy::ecs::system::SystemParam)]
pub struct LaborConfigs<'w> {
    pub fauna: Res<'w, FaunaConfigHandle>,
    pub labor: Res<'w, LaborConfigHandle>,
    pub flora: Res<'w, FloraConfigHandle>,
    pub ladder: Res<'w, LadderConfigHandle>,
    pub wellbeing: Res<'w, WellbeingConfigHandle>,
    pub combat: Res<'w, CombatConfigHandle>,
    pub creatures: Res<'w, CreaturesConfigHandle>,
}

/// Resolve each band's per-worker labor yields (Early-Game Labor, slice 3a). Replaces the retired
/// single-task systems (`advance_harvest_assignments` / `advance_scout_assignments` /
/// `advance_fauna_pursuits`): a band now draws subsistence from *many* in-range sources at once,
/// with yield scaled by the workers assigned to each. Runs in the Population stage after
/// consumption drains the larder, so labor income lands the same turn (matching the old timing).
///
/// - **Forage** `{ tile }`: within `band_work_range` of the band and carrying a `FoodModuleTag` →
///   draws down the tile's depletable forage patch (§0-ii) via the shared `forage_take` primitive
///   (Sustain gather = the regrowth skim; `sustainable` = one turn's net patch regrowth), the plant
///   mirror of the Hunt take. Module-less / unseeded → 0 this turn, assignment kept (source
///   conditions that recover in place). **Out of range lapses** the assignment and returns its
///   workers to the pool (feed entry), the plant twin of the hunt leash: a patch is fixed, so
///   out-of-range can only mean the band walked away.
/// - **Hunt** `{ fauna_id, policy }`: reuses the per-policy ecology ceiling; the take is
///   `min(workers × per_worker_biomass_capacity, policy_ceiling)`, so under-hunting a Sustain herd
///   (`worker_cap < regrowth`) lets it GROW. Tracks a roaming herd out to `band_work_range +
///   hunt_leash_tiles` (leashed follow); past that — or if the herd is gone — the assignment lapses
///   and its workers return to the pool (feed entry).
/// - **Scout**: reveals fog outward from the band. **Warrior**: inert (band-wide standing guard; it
///   does not escort or mitigate a hunt — its first consumer is the Phase 1 predator-raid path).
///
/// Husbandry (Phase E) re-homes here, but **Sustain no longer tames** (slice 3a): a `Tame` hunt
/// fills the herd's domestication meter, while any *stewardship* policy on a **Thriving** source
/// earns the faction the knowledge that source's **current rung** teaches (slice 4 — Herding on a
/// wild herd, Penning on a pastoral one; Cultivation/Seed Selection on the plant side).
#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn advance_labor_allocation(
    mut registry: ResMut<HerdRegistry>,
    mut forage_registry: ResMut<ForageRegistry>,
    mut discovery: ResMut<DiscoveryProgressLedger>,
    mut event_log: ResMut<CommandEventLog>,
    tick: Res<SimulationTick>,
    tile_registry: Res<TileRegistry>,
    sim_config: Res<SimulationConfig>,
    configs: LaborConfigs,
    tiles: Query<&Tile>,
    food_modules: Query<&FoodModuleTag>,
    mut cohorts: Query<(&mut PopulationCohort, &mut LaborAllocation)>,
) {
    let fauna = configs.fauna.get();
    let labor = configs.labor.get();
    let flora = configs.flora.get();
    let ladder = configs.ladder.get();
    let wellbeing = configs.wellbeing.get();
    // **Predators Phase 0 — the hunt-danger seam** (`docs/plan_predators.md`). The resolver tuning and
    // the base human's intrinsic combat profile, resolved once: a dangerous hunt builds a fight from
    // the hunting party (the hunters on that herd) vs the animal's fighting stock and applies the
    // band-side casualties. Hoisted out of the per-cohort loop — neither changes within a turn.
    let combat_tuning = configs.combat.get().tuning();
    let person_profile = configs.creatures.get().person();
    let map_seed = sim_config.map_seed;
    let husbandry = &fauna.husbandry;
    let work_range = labor.band_work_range;
    let hunt_reach = labor.hunt_reach();
    // The forward-projection horizon for each source's steady `realized` yield: `realized` is the
    // average food/turn the source will deliver over the next N turns, simulated forward from its
    // current (pre-take) state, so the headline "Food /turn" is smooth and the assign-time seed matches
    // the first resolved value exactly.
    let realized_horizon = labor.yield_average_horizon_turns;
    // The horizon for each source's discrete **arrival schedule** — what lands on each of the next N
    // turns, from the same forward simulation `realized` averages, reported per TURN instead of
    // averaged. **The lumpiness is the whole-animal quantiser and the herd's own regrowth**, not a
    // kill-credit bank: `project_arrivals_hunt` never touches `Herd::hunt_credit`, which left the
    // resident path when the take became a stock (see `Herd::hunt_credit`). Its own lever: a schedule
    // is a display span the client charts, where `realized_horizon` is a smoothing window.
    let arrivals_horizon = labor.arrivals_horizon_turns;
    // **The ladder's knowledge dials (§4)** — the per-turn accrual every teaching rung pays, and the
    // ledger bar at which a faction may act on a knowledge. Hoisted out of the per-cohort loop.
    // **One pair for BOTH webs**: these used to be duplicated at identical values in
    // `labor_config.forage.cultivation` and `fauna_config.husbandry`, back when each web had its own
    // hard-coded earn site. The earn path is one rung-driven seam now, so the dials live on the
    // ladder with the build dials — the plant and animal ladders can only be paced together.
    // **`progress_per_turn` is the BASE, not the amount**: since the harvest floor it is scaled per
    // call by the assignment's own floor (`intensification::learn_multiplier`), so the whole block
    // travels to `credit_rung_lesson` rather than a pre-multiplied delta.
    let knowledge_dials = &ladder.knowledge;
    let knowledge_threshold = ladder.knowledge.completion_threshold;
    // The two rungs the build engine drives (`crate::intensification`): the plant's tended patch and
    // the animal's pen. Their build dials — accrual rate, feral decay, and the investment dip — are
    // the ladder's, not each web's, so the two paths can never be tuned apart. Hoisted out of the
    // per-cohort loop alongside the knowledge levers.
    let tended_rung = ladder.rung(RungKey::PlantTended);
    let field_rung = ladder.rung(RungKey::PlantField);
    let pastoral_rung = ladder.rung(RungKey::AnimalPastoral);
    let pen_rung = ladder.rung(RungKey::AnimalPen);
    // **Extending** a pen (2d-β) re-uses the pen rung's own build dials — a ring is the same fencing
    // labor at the same forgone-yield price, so it must never drift from the initial build.
    // `workers` is `PEN_EXTEND_CREW` because the `animal:pen` rung declares no `crew_needed` — the
    // animal web sizes a crew off the herd (`herders_needed`), not off the rung — so the value cannot
    // change the rate; it is named rather than a bare literal so a future rung-level animal crew
    // makes this site's assumption visible instead of silently under-crewing every ring.
    //
    // **The floor is [`MANAGED_SOURCE_FLOOR`], not the assignment's**: a ring is only ever built
    // around a herd that is already penned, whose take is its `managed_production` at every floor —
    // so there is no pressure the keeper chose for the dial to scale.
    let pen_build_rate = pen_rung.build_accrual(
        Some(Improvement::Corral),
        true,
        MANAGED_SOURCE_FLOOR,
        RUNG_TIMESCALE_UNSCALED,
        PEN_EXTEND_CREW,
    );
    let pen_build_dip = pen_rung
        .yield_fraction_while_building()
        .expect("the pen rung is an investment — it has a build meter");
    // In-range checks use true hex distance (not Chebyshev on offset coords, whose square
    // corners are actually 3 hex-steps away), wrap-aware to match the rest of the sim.
    let grid_width = tile_registry.width;
    let grid_height = tile_registry.height;
    let wrap_horizontal = sim_config.map_topology.wrap_horizontal;

    for (mut cohort, mut allocation) in cohorts.iter_mut() {
        // Normalize each turn: if `working` shrank, trim assignments so Σ ≤ available.
        let available = available_workers(cohort.working);
        let faction = cohort.faction;
        // **A trimmed-away assignment is ANNOUNCED.** `normalize` drops from the tail when the band
        // no longer has the hands, and until this it did so in total silence — the one place in the
        // labor system that abandoned work without saying so, while the out-of-range lapse a hundred
        // lines below has always pushed a feed entry. The improvement rides the *assignment*, so a
        // population dip could destroy a 25-turn build commitment and the player would only find out
        // by noticing a tended patch with nobody on it.
        for assignment in allocation.normalize(available) {
            announce_dropped_assignment(&mut event_log, tick.0, faction, &assignment);
        }
        if allocation.assignments.is_empty() {
            continue;
        }
        let Ok(band_pos) = tiles.get(cohort.current_tile).map(|tile| tile.position) else {
            continue;
        };
        // Productivity modifier stack (wellbeing): scale every yield by the band's output
        // multiplier at PAYOUT. One call — future modifiers slot into `output_multiplier`.
        let mult = output_multiplier(&cohort, &wellbeing);
        let mult_f = mult.to_f32();

        let mut lapsed: Vec<usize> = Vec::new();
        // Assignments whose investment **completed this turn**: the source has climbed its rung, so
        // there is nothing left to build and the improvement slot is cleared after the loop. The
        // assignment's *stance* is untouched — that is the point of the two-axis split (issue #442);
        // the crew simply goes on harvesting the improved source under the stance the player chose.
        // Collected rather than applied in place because the loop iterates `assignments` immutably;
        // applied **before** the `lapsed` removal below, which invalidates indices.
        let mut completed: Vec<usize> = Vec::new();
        // Retained per-source yield telemetry, rebuilt from scratch: one entry per assignment in
        // iteration order, pre-seeded to zero so any arm that `continue`s (out of range, module
        // lost, herd gone) leaves a correct 0-yield row and index alignment is preserved. This also
        // *overwrites* any assign-time forecast seed (`LaborAllocation::set_source_yield`) with the
        // resolved take — the seed is only the pre-resolution stand-in.
        let mut yields: Vec<SourceYield> = vec![SourceYield::ZERO; allocation.assignments.len()];
        // The pen feed this band ACTUALLY pays this turn, summed across every pen it keeps (a band may
        // keep more than one). Rebuilt from scratch each turn, exactly like `yields` — it is the real
        // debit off `cohort.stores`, and it appears in neither `food_income` nor `food_consumption`, so
        // the snapshot must export it or the band's net-food readout overstates the surplus by exactly
        // this much (see `LaborAllocation::last_pen_feed_upkeep`).
        let mut pen_feed_paid = 0.0_f32;
        // **The band's fodder inflow rate this turn** (Flora Roster F3, §5.3) — the fodder its hay
        // Fields harvest into the `FODDER` store, summed across every Forage assignment. This is the
        // *sustained flow* the pen's `K_pen` term reads (NOT the store's stock, which would spike K
        // off a buffer and oscillate): in steady state inflow = the field output the store holds
        // steady at. Cached onto each pen this band keeps after the assignment loop and read next turn
        // by `advance_herds`' `ecological_carrying_capacity` — the deliberate Logistics-reads-what-
        // Population-wrote one-turn lag, exactly as `footprint_intake` is.
        let mut band_fodder_inflow = 0.0_f32;
        // The fauna ids of the pens this band tends this turn — the keepers whose `K_pen` gets the
        // fodder term. Collected in the loop; the rate is stamped on them post-loop (the take arm
        // already borrows the herd mutably, so a second pass keeps the borrows simple).
        let mut kept_pens: Vec<String> = Vec::new();
        for (idx, assignment) in allocation.assignments.iter().enumerate() {
            let workers = assignment.workers;
            if workers == 0 {
                continue;
            }
            // **The second axis** (issue #442): what this crew is *building*, independent of how hard
            // it is pulling. `None` = a pure harvest. It dips the take ceiling, drives the build
            // meter, and is the thing completion clears — `policy` is never written by this system.
            let improvement = assignment.improvement;
            match &assignment.target {
                LaborTarget::Forage {
                    tile,
                    floor,
                    species,
                } => {
                    // **Out of range → the assignment is ABANDONED**, the plant twin of the hunt
                    // leash lapse. A patch cannot move, so beyond `band_work_range` the band walked
                    // away from it — a decision, not a drift, and there is nothing to follow. Keeping
                    // the assignment would pay a correct `+0.00` forever while the tile still renders
                    // as worked and its workers stay booked, so the workers return to the pool and the
                    // player is told which tile was given up.
                    let distance = crate::grid_utils::hex_distance_wrapped(
                        band_pos,
                        *tile,
                        grid_width,
                        wrap_horizontal,
                    );
                    if distance > work_range {
                        lapsed.push(idx);
                        event_log.push(CommandEventEntry::new(
                            tick.0,
                            CommandEventKind::Forage,
                            faction,
                            format!(
                                "foragers abandoned ({}, {}) — out of the band's work range",
                                tile.x, tile.y
                            ),
                            Some(format!(
                                "status=lapsed reason=out_of_range x={} y={} distance={} range={}",
                                tile.x, tile.y, distance, work_range
                            )),
                        ));
                        continue;
                    }
                    let Some(tile_entity) = tile_registry.index(tile.x, tile.y) else {
                        continue;
                    };
                    // The **gather** season is the food module's. A tile with no module offers no
                    // wild gather at all (`NO_FORAGE_SEASON` → zero per-worker throughput), which is
                    // exactly right — and, since slice 5, a real state rather than an impossible one:
                    // `Sow` places a Field on ground the `plant:field` rung's `site_requirement`
                    // accepts (rich + watered), module or not, and a Field's harvest is biomass-based
                    // and seasonless.
                    let seasonal = food_modules
                        .get(tile_entity)
                        .map_or(NO_FORAGE_SEASON, |module| module.seasonal_weight.max(0.0));
                    // **May this faction sow THIS ground?** — the `plant:field` rung's two gates,
                    // both resolved off the rung record, both read here because each gates the *same*
                    // two things below: the seed going into the ground at all, and the build meter it
                    // then fills.
                    //  - **the knowledge**: does the faction know Seed Selection?
                    //  - **the SITE** (`site_requirement`): is the land already very fertile, and near
                    //    fresh water? Rung 3 knows how to move seed, not how to fertilize — so it can
                    //    only place a Field where the land does the fertilizing itself. That is the
                    //    scarcity the rung is *made of*, and the ground the `sow` command refuses up
                    //    front with the reason (too poor / too dry / both).
                    let sow_permitted = improvement == Some(Improvement::Sow)
                        && field_rung.unlock_discovery_id().is_none_or(|knowledge| {
                            knows(&discovery, faction, knowledge, knowledge_threshold)
                        })
                        && tiles.get(tile_entity).is_ok_and(|ground| {
                            let fresh_water = tile_is_fresh_watered(
                                ground,
                                grid_width,
                                grid_height,
                                wrap_horizontal,
                                |coord| {
                                    tile_registry
                                        .index(coord.x, coord.y)
                                        .and_then(|entity| tiles.get(entity).ok())
                                        .map(|neighbor| neighbor.terrain_tags)
                                },
                            );
                            rung_site_refusal(field_rung, ground, &labor.forage, fresh_water)
                                .is_none()
                        });
                    // **WHICH NAMED PLANT this ground would be committed to** (Flora Roster S1,
                    // `docs/plan_flora_roster.md` §4.3). Resolved through the *same*
                    // `resolve_committed_species` seam the `assign_labor` rejection reads, so a
                    // selection the command accepted can never be one the turn then refuses — and
                    // through `tile_flora_composition`, never `FloraConfig::composition` on a raw
                    // terrain, so a navigable hex is judged on the basket it actually has.
                    //
                    // `None` means **there is nothing here this rung can commit to**: either the
                    // player's pick is illegal, or the whole basket's `cultivation_ceiling` stops
                    // below this rung (an open-water fishery, an alpine peak). Either way the
                    // investment simply does not accrue — you cannot farm what will not climb.
                    let committing =
                        matches!(improvement, Some(Improvement::Cultivate | Improvement::Sow))
                            .then(|| {
                                let rung = if improvement == Some(Improvement::Sow) {
                                    RungKey::PlantField
                                } else {
                                    RungKey::PlantTended
                                };
                                tiles.get(tile_entity).ok().and_then(|ground| {
                                    // §10 scoping: Cultivate and a Sow that **upgrades** an existing
                                    // patch commit against the tile's **realized** basket (what is
                                    // growing here); a Sow that **creates** a patch on bare ground has no
                                    // realized basket, so it reads the **affinity** roster (what CAN grow
                                    // here). The create case does not occur on a generated map — every
                                    // food-bearing tile already carries a patch — but the branch keeps the
                                    // "you sow what grows here; unwilling ground is rung 4" rule honest.
                                    let sow_from_nothing = improvement == Some(Improvement::Sow)
                                        && forage_registry.patch(*tile).is_none();
                                    if sow_from_nothing {
                                        resolve_committed_species(
                                            species.as_deref(),
                                            flora.composition(ground.resource_terrain()),
                                            &flora,
                                            rung,
                                        )
                                        .ok()
                                    } else {
                                        let composition = tile_flora_composition(
                                            &flora,
                                            &labor.forage,
                                            ground,
                                            map_seed,
                                        );
                                        resolve_committed_species(
                                            species.as_deref(),
                                            &composition,
                                            &flora,
                                            rung,
                                        )
                                        .ok()
                                    }
                                })
                            })
                            .flatten();
                    // A Field may only be placed on ground that grows something sowable — the
                    // species half of "the land must take seed", beside the site half above.
                    let sow_permitted = sow_permitted && committing.is_some();
                    // **`Sow` PLACES the source** (§2 — the one rung that needs no source below it:
                    // seed travels, unlike a herd you never tamed). The first turn a crew works
                    // sowable ground, the seed goes in and the patch exists from here on — at the
                    // tile's **own** biome capacity (`tile_forage_capacity`, the same source a wild
                    // patch is seeded from — there is no Field-specific table) and at the reseed
                    // floor's standing crop.
                    if sow_permitted && forage_registry.patch(*tile).is_none() {
                        if let Ok(sown_tile) = tiles.get(tile_entity) {
                            let mut patch = ForagePatch::sown(
                                *tile,
                                tile_forage_capacity(&labor.forage, sown_tile),
                                labor.forage.reseed_floor_fraction,
                            );
                            patch.refresh_ecology_phase(&labor.forage.ecology);
                            forage_registry.patches.insert(*tile, patch);
                        }
                    }
                    // **What is actually growing on this tile** — the realized basket, resolved once
                    // per assignment through the one `tile_flora_composition` seam (never
                    // `FloraConfig::composition` on a raw terrain). Every rate this arm pays is the
                    // share-weighted average of the *patch's* basket, which `forage.rs` derives from
                    // this one (#433) — so it is resolved *before* the registry is borrowed mutably.
                    // A tile that is not on the map names no plants: the rates then fall back to the
                    // empty-basket defaults, which is the honest reading of ground nobody can see.
                    let tile_composition = tiles.get(tile_entity).map_or_else(
                        |_| Cow::Owned(Vec::new()),
                        |ground| tile_flora_composition(&flora, &labor.forage, ground, map_seed),
                    );
                    // Depletable patch (Intensification §0-ii): draw the biomass down via the shared
                    // `forage_take` primitive (mirrors the Hunt arm). Every `FoodModuleTag` tile is
                    // seeded a patch at Startup; a missing one (a dynamically-tagged tile, or ground
                    // nobody has sown) is skipped this turn. Gather per the assignment's policy
                    // (§0-iii, parity with hunting).
                    let Some(patch) = forage_registry.patch_mut(*tile) else {
                        continue;
                    };
                    // **The commitment, recorded once and fixed until the patch goes feral.** This is
                    // the first turn a crew works this ground under Cultivate/Sow, so this is where
                    // the tile stops being a mixed basket and becomes one named crop. It takes effect
                    // (weeding + conversion) when the improvement *completes* — while the crew
                    // is still clearing, the stand is still the basket it started as.
                    if let Some(chosen) = committing.as_deref() {
                        patch.commit_species(chosen);
                    }
                    // **NOTHING LEFT TO BUILD → hand the verb back, whoever finished it.** The four
                    // accrual arms below only record a completion the *acting* band achieved, but
                    // `handle_cultivate`/`handle_sow` set the improvement on **every** band working
                    // the source, so a second crew is left holding a verb for a rung another crew
                    // climbed. Stated once, here, before the Field arm's early return — which is what
                    // made a finished Field permanently un-clearable for a second band's `Sow`, the
                    // one case that could not self-heal (PR #448 review).
                    if improvement.is_some_and(|verb| forage_rung_already_built(patch, verb)) {
                        completed.push(idx);
                    }
                    // **THE earn path (§4): practising rung N teaches the knowledge that unlocks rung
                    // N+1.** Driven entirely by the rung the patch *currently stands on* — a wild
                    // patch teaches **Cultivation**, a tended one **Seed Selection** — so the lesson
                    // is a property of the source's rung, not of the verb. The old hard-coded
                    // `Sustain && Thriving → CULTIVATION_DISCOVERY_ID` branch is gone: `earns_knowledge`
                    // was declarative when slice 2 landed it, and this is where it goes live.
                    //
                    // Knowledge is all that is earned here — working a patch never *tames* it:
                    // cultivation is an explicit `Cultivate` improvement with an investment cost
                    // (below). The rung is resolved *here*, above the branches, because it is a
                    // property of the pre-take patch; the **credit** is applied inside each branch,
                    // once its take is known — see `credit_rung_lesson`.
                    let lesson_rung = patch_rung(patch, &ladder);
                    // **The steady headline** — the forward-projected average food/turn over the next
                    // `realized_horizon` turns, computed from the patch's PRE-take state (before either
                    // branch draws it down), so it equals the assign-time seed exactly. Both the Field
                    // and the drawn-down branches record this one value.
                    let forage_realized = crate::forage::project_realized_forage(
                        patch,
                        &tile_composition,
                        &labor.forage,
                        &flora,
                        &ladder,
                        seasonal,
                        mult_f,
                        workers,
                        *floor,
                        improvement,
                        realized_horizon,
                    );
                    // **A FIELD (rung 3) is worked, not wild-gathered** — the plant web's one managed
                    // rung, and the twin of a penned herd's keeper income (paid place-local here).
                    // The band whose Forage assignment works it (≥1 worker here → place-local by
                    // construction) takes `biomass × field_provisions_per_biomass` off the full
                    // standing crop, WITHOUT drawing biomass down: the crop is yours, so there is no
                    // wild stock to over-skim, the policy axis honestly collapses, and `sustainable ==
                    // actual` (no ⚠). Marking the patch tended-this-turn stops `advance_cultivation`
                    // taking it feral.
                    //
                    // **A TENDED patch (rung 2) is NOT here any more** (slice 7). It is still a *wild
                    // stand* — better cared for, growing on the boosted `tended_regrowth_gain` curve —
                    // so it falls through to the ordinary `forage_take` below: policy-live,
                    // worker-capped, and drawn down, exactly as a *pastoral* herd is hunted on its
                    // boosted `r`. The plant web used to collapse a rung earlier than the animal one;
                    // that asymmetry was the bug.
                    //
                    // **Working a completed improvement IS tending it**, at either rung — so the flag
                    // is set here, before the rungs part company, and `advance_cultivation` spares the
                    // patch. Load-bearing for rung 2 now that it takes the wild path: the flag used to
                    // ride the managed branch, so moving the tended patch out of it without this would
                    // send every patch a band Sustain-gathers *feral* while they worked it.
                    if patch.is_managed() {
                        patch.tended_this_turn = true;
                    }
                    if patch.is_field() {
                        // **Production**: what the Field offers this turn. Shared with the pre-commit
                        // forecast (`forage::forage_forecast`), so the client's "expected yield" is
                        // exactly what it is paid.
                        let production = field_provisions(
                            patch,
                            &tile_composition,
                            &labor.forage,
                            &flora,
                            mult_f,
                        );
                        // **Collection**: what the crew can carry home — the *same* per-worker
                        // throughput a wild gather is capped by, at the seasonless managed weight (a
                        // Field's crop stands where you planted it). Rung 3 collapses the policy axis;
                        // it does NOT excuse you from the harvest. One worker used to collect the
                        // whole Field however rich it was.
                        let collection = workers as f32
                            * managed_per_worker_yield(
                                patch,
                                &tile_composition,
                                &labor.forage,
                                &flora,
                                mult_f,
                            );
                        let provisions = scalar_from_f32(production.min(collection));
                        if provisions > scalar_zero() {
                            cohort.stores.add(FOOD, provisions);
                        }
                        let paid = provisions.to_f32();
                        // **THE earn path, rung 3.** A Field's take is its `managed_production` at
                        // every floor, so the dial the crew set is inert here and
                        // [`MANAGED_SOURCE_FLOOR`] is what the lesson is paced by. `plant:field`
                        // teaches nothing today (`irrigation`/`rotation` is rung 4's business), so
                        // this is the uniformity that stops rung 4 from having to remember: the
                        // branch that `continue`s still reaches the earn path.
                        credit_rung_lesson(
                            lesson_rung,
                            MANAGED_SOURCE_FLOOR,
                            MANAGED_SOURCE_IS_TENDED,
                            knowledge_dials,
                            faction,
                            &mut discovery,
                        );
                        // **The FODDER account (Flora Roster F3, §5.1).** The *same* managed harvest,
                        // routed by the yield vector's fodder component instead of its provisions
                        // component — a grain Field's `field_fodder` is `0` (its crop pays no fodder),
                        // a hay Field's `field_provisions` is `0` (hay is no food), so this is
                        // commodity-generic with **no role branch**. The crew carries hay home at the
                        // same throughput it carries grain, so the collection cap is
                        // `managed_per_worker_fodder`. Credited to the same `FODDER` `LocalStore` key,
                        // which round-trips through the snapshot for free.
                        let fodder_production =
                            field_fodder(patch, &tile_composition, &labor.forage, &flora, mult_f);
                        let fodder_collection = workers as f32
                            * managed_per_worker_fodder(
                                patch,
                                &tile_composition,
                                &labor.forage,
                                &flora,
                                mult_f,
                            );
                        let fodder = scalar_from_f32(fodder_production.min(fodder_collection));
                        if fodder > scalar_zero() {
                            cohort.stores.add(FODDER, fodder);
                            band_fodder_inflow += fodder.to_f32();
                        }
                        // **The TRADE-GOODS account (Flora Roster F4).** The SAME managed harvest, routed by the yield
                        // vector's trade component — a staple/hay Field's field_trade_goods is ~0, a cash crop's is
                        // dominant, so this is commodity-generic with NO role branch. Credited to the band's own
                        // `TRADE_GOODS` `LocalStore` key, exactly like FOOD/FODDER above: goods sit where they were
                        // produced until a supply network reaches them (see `TRADE_GOODS`).
                        let trade_production = field_trade_goods(
                            patch,
                            &tile_composition,
                            &labor.forage,
                            &flora,
                            mult_f,
                        );
                        let trade_collection = workers as f32
                            * managed_per_worker_trade(
                                patch,
                                &tile_composition,
                                &labor.forage,
                                &flora,
                                mult_f,
                            );
                        let trade_goods = scalar_from_f32(trade_production.min(trade_collection));
                        if trade_goods > scalar_zero() {
                            cohort.stores.add(TRADE_GOODS, trade_goods);
                        }
                        // **The arrival schedule — computed POST-take, unlike `realized`.** It
                        // answers "when does the next food land", so it must start from the state the
                        // turn leaves behind: projecting from the pre-take state would re-promise the
                        // delivery this turn has already paid. Slot 0 is therefore genuinely the
                        // *next* turn's delivery.
                        let arrivals = crate::forage::project_arrivals_forage(
                            patch,
                            &tile_composition,
                            &labor.forage,
                            &flora,
                            &ladder,
                            seasonal,
                            mult_f,
                            workers,
                            *floor,
                            improvement,
                            arrivals_horizon,
                        );
                        yields[idx] = SourceYield {
                            actual: paid,
                            // A cash crop's harvest really does sell (Flora Roster F4) — the same
                            // `min(production, collection)` the band's trade store was credited with.
                            trade: trade_production.min(trade_collection),
                            realized_trade: crate::forage::PLANT_TRADE_FORECAST_NOT_YET_PROJECTED,
                            // A managed harvest never draws the stock down, so it can never overdraw.
                            sustainable: paid,
                            // The forward-projected steady headline (computed pre-take above).
                            realized: forage_realized,
                            arrivals,
                            // The crop the crew could not carry: it stood in the field and rotted.
                            // The understaffing signal — "add hands here" — and the reason a rich
                            // Field is a real labor sink rather than a free ration.
                            wasted: (production - paid).max(0.0),
                            // **Floored at the build crew like every other row**, even though a Field
                            // has nothing left to build: a second crew's verb is handed back by the
                            // once-per-source "nothing left to build" test *after* this arm's early
                            // return, so for one turn a band can hold a `Sow` on a finished Field —
                            // and the assign-time seed prices that same stale verb. Whichever number
                            // is right, both halves must say it (`forecast_source_yield`).
                            workers_needed: source_crew_needed(
                                ladder.build_crew(improvement),
                                workers_needed_for_take(
                                    paid,
                                    managed_per_worker_yield(
                                        patch,
                                        &tile_composition,
                                        &labor.forage,
                                        &flora,
                                        mult_f,
                                    ),
                                    workers,
                                ),
                            ),
                            // A managed rung-3 harvest cannot overdraw — no ⚠, whatever the policy.
                            overdraws: false,
                        };
                        continue;
                    }
                    let biomass_before = patch.biomass;
                    // **The escapement room, resolved PRE-take** — the stock standing above this
                    // assignment's floor, in biomass and before any cap. It is the source of two
                    // different answers below: the work predicate ([`crew_is_working_the_source`],
                    // which replaced this arm's `EcologyPhase::Thriving` gate) and the `production`
                    // the telemetry row reports as offered.
                    let standing_above_floor =
                        forage_escapement_ceiling(*floor, biomass_before, patch.carrying_capacity);
                    let working_the_patch = crew_is_working_the_source(standing_above_floor);
                    let provisions = forage_take(
                        patch,
                        &tile_composition,
                        workers,
                        *floor,
                        improvement,
                        &labor.forage,
                        &flora,
                        &ladder,
                        mult_f,
                        seasonal,
                    );
                    let take = biomass_before - patch.biomass;
                    // **THE earn path, rungs 1–2** — the drawn-down half of the split above. A crew
                    // with nothing standing above its floor is watching the stand, not practising on
                    // it, whatever it intended; that is what replaced the `EcologyPhase::Thriving`
                    // term this site used to carry — a cliff where the model now wants a rate — and
                    // it is what makes `floor = 1.0` (leave it all standing, learn at ×2) honestly
                    // earn nothing.
                    credit_rung_lesson(
                        lesson_rung,
                        *floor,
                        working_the_patch,
                        knowledge_dials,
                        faction,
                        &mut discovery,
                    );
                    if provisions > scalar_zero() {
                        cohort.stores.add(FOOD, provisions);
                    }
                    // **The FODDER account at rung 2** (issue #427). *A harvest* of `B` biomass pays
                    // `B × yield.*` into all three accounts (`docs/plan_flora_roster.md` §3) — that is
                    // unconditional, not a Field-only rule. So the SAME take `forage_take` just paid
                    // food from is routed through the patch basket's fodder component here, exactly
                    // as the Field arm routes its managed harvest. `0` for a basket with no fodder
                    // crop in it, so this is commodity-generic with no `role` branch. **No second
                    // collection cap**: unlike a Field's managed rate, the take is already
                    // worker-capped inside `forage_take`, so the crop the crew carries home *is* the
                    // take it made.
                    //
                    // **The WILD credit is gated on Foddering** (#433) — the same 2007 capability the
                    // pen's own hay draw reads. Since every rate is now the basket's average, a wild
                    // tile that happens to realize `hay_grass` pays hay on any harvest; banking it for
                    // a faction that has not learned to hay a herd would hand out animal feed nobody
                    // bid for. A **committed** patch (rung 2 or 3) is ungated: committing to
                    // `hay_grass` *is* the bid. The gate lives here, at the credit site, so the rate
                    // seam in `forage.rs` stays free of knowledge lookups.
                    let fodder_permitted = patch.species.is_some()
                        || knows(
                            &discovery,
                            faction,
                            FODDERING_DISCOVERY_ID,
                            knowledge_threshold,
                        );
                    let fodder = if fodder_permitted {
                        scalar_from_f32(tended_take_fodder(
                            take,
                            patch,
                            &tile_composition,
                            &flora,
                            &labor.forage,
                            mult_f,
                        ))
                    } else {
                        scalar_zero()
                    };
                    if fodder > scalar_zero() {
                        cohort.stores.add(FODDER, fodder);
                        band_fodder_inflow += fodder.to_f32();
                    }
                    // **Cultivate — the investment.** The crew is clearing and planting, not
                    // gathering: `forage_take` above already paid only the reduced Cultivate ceiling
                    // (the rung's `yield_fraction_while_building × the crew's throughput` — the
                    // up-front cost), and here the patch accrues toward becoming a tended crop.
                    // Gates: the faction must **know Cultivation** (earned above) and the crew must
                    // have actually drawn something off the patch.
                    //
                    // **There is no health gate any more** (`docs/plan_harvest_floor.md` §3.2). The
                    // patch's `EcologyPhase::Thriving` used to gate this, so a build stalled outright
                    // the moment a crew — anyone's crew — pulled the stand below Thriving, and the
                    // "stops accruing but is not lost" lapse state existed to make that survivable.
                    // The floor replaced it with a **rate**: `learn_multiplier` scales the accrual by
                    // how much the crew leaves standing, so pulling harder slows the build in
                    // proportion instead of stopping it at a cliff. Nothing lapses, so there is no
                    // lapse to hold progress across.
                    //
                    // **Ordering: accrue AFTER the take.** The patch pays this turn per its state at
                    // the *start* of the turn, so the pre-commit forecast the client showed is exactly
                    // what the sim paid (forecast == actual). The turn progress reaches `1.0` is the
                    // last preparing take; the full tended yield starts the next turn.
                    if improvement == Some(Improvement::Cultivate) {
                        // Marked worked-as-improvement so `advance_cultivation` spares it: a patch
                        // under active preparation neither goes feral nor bleeds its partial progress.
                        patch.tended_this_turn = true;
                        // The rung's own gates, resolved for the engine: the faction must know the
                        // rung's unlock knowledge (Cultivation), and the crew must actually be
                        // working the patch ([`crew_is_working_the_source`] — the term that replaced
                        // the Thriving gate).
                        let eligible = tended_rung.unlock_discovery_id().is_none_or(|knowledge| {
                            knows(&discovery, faction, knowledge, knowledge_threshold)
                        }) && working_the_patch
                            // **Nothing to tend if nothing here climbs.** A patch with no committed
                            // plant is one whose basket the tended rung's `cultivation_ceiling`
                            // refuses outright — the "not every plant climbs" ruling reaching the
                            // build meter.
                            && patch.species.is_some();
                        // THE build seam: the rung supplies the accrual (0 unless Cultivate is the
                        // rung's verb and the gates hold); the patch owns its meter and the
                        // side-effects of completing it. The **floor** is the assignment's own — the
                        // same dial that paced the lesson above paces the build.
                        let accrual = tended_rung.build_accrual(
                            improvement,
                            eligible,
                            *floor,
                            RUNG_TIMESCALE_UNSCALED,
                            workers,
                        );
                        // **The feed line rides the TRANSITION, not the state.** `accrue_cultivation`
                        // answers "did this call finish it", so a second band working an
                        // already-tended patch clears its verb (above) without announcing the
                        // cultivation a second time.
                        if accrual > 0.0 && patch.accrue_cultivation(faction, accrual) {
                            completed.push(idx);
                            event_log.push(CommandEventEntry::new(
                                tick.0,
                                CommandEventKind::Cultivate,
                                faction,
                                format!("Cultivated patch at ({}, {})", tile.x, tile.y),
                                Some(format!(
                                    "status=complete action=cultivate x={} y={}",
                                    tile.x, tile.y
                                )),
                            ));
                        }
                    }
                    // **Sow — the rung-3 investment**, the twin of Cultivate above and the
                    // same shape: `forage_take` has already paid only the `plant:field` rung's dip,
                    // and here the patch accrues toward becoming a Field. On ground the crew *just*
                    // sowed that dip is honestly ~0 (there is no standing crop to take a fraction of):
                    // a bare-ground field is pure investment, paid entirely in the 25 turns of labor.
                    //
                    // **Not gated on Thriving, unlike Cultivate** — and that is load-bearing, not a
                    // relaxation: freshly sown ground starts at the reseed floor, i.e. *Collapsing* by
                    // construction, so a health gate would make sowing bare ground impossible. You
                    // *tend* a healthy wild stand; you *plant* bare ground. (The animal side already
                    // draws the same line — `Tame` has no health gate either.)
                    if improvement == Some(Improvement::Sow) {
                        // Marked worked-as-improvement so `advance_cultivation` spares it: a patch
                        // under active preparation neither goes feral nor bleeds its partial progress.
                        patch.tended_this_turn = true;
                        if accrue_field(
                            patch,
                            field_rung,
                            improvement,
                            sow_permitted,
                            *floor,
                            faction,
                            &mut event_log,
                            tick.0,
                            *tile,
                            workers,
                        ) {
                            completed.push(idx);
                        }
                    }
                    // **Every harvest sells its basket's trade component, at the basket's own
                    // rate.** No factor of any kind rides the depth of the draw: the retired
                    // `market.trade_goods_multiplier` (4×) paid one drawdown rate a product bonus,
                    // which re-welded product to intensity — the exact thing the yield vector
                    // exists to separate (`docs/plan_hunt_yield_model.md` §2). A deep floor still
                    // out-earns a shallow one on trade, because it *takes more biomass*: that is
                    // the intensity ladder doing the work, not a bonus.
                    //
                    // (Rung 3 keeps its own rule in `field_trade_goods` — a Field is never drawn
                    // down at all.)
                    let forage_trade = tended_take_trade_goods(
                        take,
                        patch,
                        &tile_composition,
                        &flora,
                        &labor.forage,
                        mult_f,
                    );
                    {
                        let trade_goods = scalar_from_f32(forage_trade);
                        if trade_goods > scalar_zero() {
                            cohort.stores.add(TRADE_GOODS, trade_goods);
                        }
                    }
                    // Sustainable = one turn's MSY of the patch at its **pre-take** biomass, in
                    // provisions (same conversion + output multiplier as the actual take), against
                    // the patch's **own** curve (`patch_ecology`) — a tended patch's sustainable line
                    // sits on its boosted `r`, so Sustain-gathering it reads no ⚠ while
                    // Surplus-gathering it does. This lights the over-forage ⚠ for free the moment
                    // `actual > sustainable`, and since slice 7 that fires on a **tended** patch too:
                    // rung 2 draws down, so it can be over-farmed. (It never could before — the old
                    // managed branch recorded `sustainable == actual` by construction.)
                    let sustainable = sustainable_yield(
                        biomass_before,
                        patch.carrying_capacity,
                        &patch_ecology(patch, &labor.forage),
                    ) * patch_provisions_per_biomass(
                        patch,
                        &tile_composition,
                        &flora,
                        &labor.forage,
                    ) * mult_f;
                    // The two staffing signals, from the same take. **Overstaffing**: invert the take
                    // by the **effective** per-worker throughput this turn — the whole of
                    // `forage_take`'s worker cap, `per_worker_biomass_capacity × seasonal ×
                    // build_dip`, so a labor-bound low-season patch isn't falsely flagged and neither
                    // is a labor-bound *building* one. §3.1 moved the dip onto the crew and this
                    // divisor kept the pre-§3.1 shape, so a fully-employed Cultivate crew inverted to
                    // `workers × dip` and the row said "only 4 of 8 working" about 8 hands that were
                    // every one of them gathering — advice that, taken, halves the take. Read through
                    // the one [`LadderConfig::build_dip`] seam, the same one `forage_take` multiplies
                    // by. **Understaffing** (`wasted`): what the escapement ceiling offered beyond
                    // what the crew could gather — here it is not lost, it simply stays in the stock
                    // and regrows, but it is the same "add hands" answer.
                    let per_worker_biomass = forage_per_worker_biomass(&labor.forage, seasonal)
                        * ladder.build_dip(improvement);
                    // **Floored at the build's own crew**, the plant twin of a herd's `herders_needed`
                    // (see [`source_crew_needed`]): a rung declares how many hands its build wants,
                    // and a thin patch can absorb fewer gatherers than that — inverting the take
                    // alone told the player a 25-turn build wanted fewer hands than the rung
                    // demands. Read through `LadderConfig::build_crew`, the *same*
                    // lookup `forage::forage_source_yield_preview` seeds with, so the row this turn
                    // writes cannot disagree with the row the compose that staffed it wrote.
                    let workers_needed = source_crew_needed(
                        ladder.build_crew(improvement),
                        workers_needed_for_take(take, per_worker_biomass, workers),
                    );
                    // The stock the patch **offered** this turn — the same pre-take escapement room
                    // the work predicate read. Undipped since the build dip moved onto the crew
                    // (`docs/plan_harvest_floor.md` §3.1): the ground standing above the floor is
                    // there whether the crew is gathering it or clearing it, so a building crew's
                    // shortfall shows up honestly as `wasted` — "this is what more hands would have
                    // brought home" — rather than being hidden in the ceiling.
                    let production = standing_above_floor.clamp(0.0, biomass_before);
                    // **The arrival schedule — computed POST-take, unlike `realized`.** It
                    // answers "when does the next food land", so it must start from the state the
                    // turn leaves behind: projecting from the pre-take state would re-promise the
                    // delivery this turn has already paid. Slot 0 is therefore genuinely the
                    // *next* turn's delivery.
                    let arrivals = crate::forage::project_arrivals_forage(
                        patch,
                        &tile_composition,
                        &labor.forage,
                        &flora,
                        &ladder,
                        seasonal,
                        mult_f,
                        workers,
                        *floor,
                        improvement,
                        arrivals_horizon,
                    );
                    yields[idx] = SourceYield {
                        actual: provisions.to_f32(),
                        // **The other currency this gather produced** — the patch basket's trade
                        // component on the take, at the `Deplete` markup if that was the policy.
                        // Never summed into `food_income` (`docs/plan_hunt_yield_model.md` §9) —
                        // that would break the larder identity.
                        trade: forage_trade,
                        // The plant web's steady TRADE projection is #337's known gap — see
                        // `forage::PLANT_TRADE_FORECAST_NOT_YET_PROJECTED`. The trade a gather
                        // *actually* earned is reported above; only the projection is missing.
                        realized_trade: crate::forage::PLANT_TRADE_FORECAST_NOT_YET_PROJECTED,
                        sustainable,
                        // The forward-projected steady headline (computed pre-take above).
                        realized: forage_realized,
                        arrivals,
                        wasted: forage_provisions(
                            (production - take).max(0.0),
                            patch_provisions_per_biomass(
                                patch,
                                &tile_composition,
                                &flora,
                                &labor.forage,
                            ),
                            mult_f,
                        ),
                        workers_needed,
                        // Plants stay flow-based (slice 8), so the wild/tended gather ⚠ is unchanged:
                        // Sustain/Cultivate/Sow take the MSY or a dip on it, Surplus/Deplete/Eradicate
                        // draw the patch down.
                        overdraws: floor_overdraws(*floor),
                    };
                }
                LaborTarget::Hunt { fauna_id, floor } => {
                    let Some(herd_pos) = registry.find(fauna_id).map(|herd| herd.position()) else {
                        // Herd despawned (extinction / another hunter) → lapse.
                        lapsed.push(idx);
                        event_log.push(CommandEventEntry::new(
                            tick.0,
                            CommandEventKind::Hunt,
                            faction,
                            format!("hunters lost {} (herd dispersed)", fauna_id),
                            Some("status=lapsed reason=herd_gone".to_string()),
                        ));
                        continue;
                    };
                    let distance = crate::grid_utils::hex_distance_wrapped(
                        band_pos,
                        herd_pos,
                        grid_width,
                        wrap_horizontal,
                    );
                    if distance > hunt_reach {
                        // Past the leash → the assignment lapses; workers return to the pool.
                        lapsed.push(idx);
                        event_log.push(CommandEventEntry::new(
                            tick.0,
                            CommandEventKind::Hunt,
                            faction,
                            format!("hunters lost the {} — it ranged too far", fauna_id),
                            Some(format!(
                                "status=lapsed reason=out_of_leash distance={} reach={}",
                                distance, hunt_reach
                            )),
                        ));
                        continue;
                    }
                    let Some(herd) = registry.herds.iter_mut().find(|herd| herd.id == *fauna_id)
                    else {
                        continue;
                    };
                    // **NOTHING LEFT TO BUILD → hand the verb back, whoever finished it** — the
                    // animal twin of the Forage arm's identical check, and stated before the pen's
                    // tend branch `continue`s for the same reason: `handle_corral` sets the verb on
                    // every band hunting the herd, so the band that did not finish the pen would
                    // otherwise hold `Corral` on a penned herd forever (PR #448 review).
                    if improvement.is_some_and(|verb| hunt_rung_already_built(herd, verb)) {
                        completed.push(idx);
                    }
                    // **The steady headline** — the forward-projected average food/turn over the next
                    // `realized_horizon` turns, computed from the herd's PRE-take state (before the pen
                    // feed/harvest or the wild take mutates it), so it equals the assign-time seed
                    // exactly. Rate-based (an average over the horizon), so it is smooth where `actual` pulses;
                    // a corralled herd projects its managed pen yield instead. Both the pen-tend and the
                    // wild-take branches record this one value.
                    let hunt_realized = fauna::project_realized_hunt(
                        herd,
                        &fauna,
                        &ladder,
                        labor.hunt.per_worker_biomass_capacity,
                        mult_f,
                        workers,
                        *floor,
                        improvement,
                        realized_horizon,
                    );
                    // **THE earn path (§4)** — the exact mirror of the Forage arm's call, and the
                    // heart of this ladder: the lesson is read off **the rung this herd stands on**,
                    // so the *same* Sustain hunt teaches **Herding** on a wild herd and **Penning** on
                    // a tamed one ("you learn herding by managing wild herds; penning by managing
                    // tamed ones"). The old hard-coded `Sustain && Thriving → HERDING_DISCOVERY_ID`
                    // branch is retired; `earns_knowledge` drives it now.
                    //
                    // **The RUNG is resolved here, above the branches; the CREDIT is applied inside
                    // each of them** — the corral tend arm `continue`s, and the two branches answer
                    // `eligible` differently (a pen is *tended*, a wild herd must have stock standing
                    // above the crew's floor). It used to be one call here, which was behaviour-
                    // neutral only while the gate read `ecology_phase`, a value no take moves. Both branches call `credit_rung_lesson`, so every rung still reaches the
                    // earn path — including the pen, whose `earns_knowledge` is Foddering.
                    //
                    // The two webs cannot cross-teach (§4.2) for free: a herd resolves to an `animal`
                    // rung, so only an animal knowledge is reachable from here.
                    let lesson_rung = fauna::herd_rung(herd, &ladder);
                    // **Corral (Rung 1c) — the pen is a managed POPULATION, not a flat rate.** A Hunt
                    // assignment on a **corralled** herd is herding/tending it, not hunting, and the
                    // turn has two halves (`docs/plan_corral_managed_population.md` §3.1):
                    //
                    // 1. **FEED.** The pen demands `pen.upkeep_per_biomass × biomass` from the
                    //    keeper's own larder — a penned herd is confined and cannot graze, so the
                    //    keeper must bring it food. `LocalStore::take` returns what it *actually*
                    //    took, which is the partial-payment primitive: `fed_fraction = paid / demand`.
                    //    A keeper who cannot pay starves the herd (next turn's `advance_husbandry`
                    //    reads the flag and shrinks it — the deliberate one-turn lag).
                    // 2. **HARVEST.** The keeper takes the *pen's* MSY (`corral_provisions` →
                    //    `sustainable_yield` under the pen's ecology, `r` = 0.60), and — unlike the
                    //    retired flat rate — this **draws the herd down**, which is exactly what makes
                    //    it sustainable: the herd converges on `K_pen/2` and pays `r·K/4` forever.
                    //
                    // The credited yield is **gross** (the feed is a separate debit above), so the
                    // player sees both halves of the trade rather than one netted number. Marks the
                    // herd tended so it doesn't escape in `advance_husbandry`. The animal mirror of
                    // the tended-patch arm in Forage.
                    // **The standing herder cost — owed by EVERY managed rung, every turn** (slice
                    // 8), resolved *before* the rung branches so a pastoral herd and a pen are charged
                    // by the same rule. `herders_needed` scales with the herd (`ceil(animals /
                    // animals_per_herder)`), retiring "a pen of 2 and a pen of 200 need one keeper".
                    //
                    // **It is owed on WAIT turns too.** A herd that cannot spare a whole animal this
                    // turn still has to be watched, kept from running off, and its fences kept up — so
                    // this is written from the assignment's head-count, never from whether a take
                    // happened. `advance_husbandry` reads it next turn (the `pen_fed_fraction` lag) and
                    // degrades an under-herded herd **proportionally** — never a binary escape.
                    //
                    // A **wild** herd writes nothing here: it isn't yours to maintain
                    // (`fauna::herders_needed` — hunt = reach + carry, harvest = maintain + take).
                    //
                    // **An INVESTMENT policy (Tame/Corral) uses the ownership-INDEPENDENT would-be crew**
                    // (taming-startup-lag fix): a Tame/Corral assignment *means* the herd is being
                    // managed, but ownership is only recorded later this turn (Population, the Tame arm's
                    // `accrue_domestication`), so the ownership-gated `herd_herders_needed` reads `0` on
                    // the turn taming starts and the crew collapses to the take-side hauler count — "1 of
                    // 3 working" on a full crew. `would_be_herders_needed` is the biomass-derived crew
                    // regardless of recorded ownership (0 only for a `wild`-ceiling species, which cannot
                    // be tamed). **Extractive policies stay ownership-gated** — a wild Sustain-hunted herd
                    // must read `0` here, or its `herded_fraction` would drop below 1 and it would falsely
                    // read under-herded and shed.
                    let herders_needed = if improvement.is_some() {
                        fauna::would_be_herders_needed(herd, &fauna)
                    } else {
                        fauna::herd_herders_needed(herd, &fauna)
                    };
                    if herders_needed > 0 {
                        herd.herded_fraction = fauna::herded_fraction(workers, herders_needed);
                    }
                    if herd.is_corralled() {
                        herd.corralled_tended_this_turn = true;
                        // **The larder offset (Grazing 2d §2.3).** A penned herd grazes its fenced
                        // footprint (`advance_herd_grazing`, Logistics → `footprint_intake`), and that
                        // grass covers part of its feed. The keeper's larder pays only the remainder:
                        //   demand_grass     = fodder_per_biomass × biomass   (grass to fully feed it)
                        //   pasture_fraction = clamp(footprint_intake / demand_grass, 0, 1)
                        //   larder_upkeep    = pen.upkeep_per_biomass × biomass × (1 − pasture_fraction)
                        // A lush footprint (pasture_fraction → 1) feeds the pen for free; a barren one
                        // (→ 0) pays the full bill (today's worst case, preserved).
                        let demand_grass = (herd.fodder_per_biomass * herd.biomass).max(0.0);
                        let pasture_fraction = if demand_grass > 0.0 {
                            (herd.footprint_intake / demand_grass).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        herd.pen_pasture_fraction = pasture_fraction;
                        // **HAY, drawn BEFORE the lossy larder (Flora Roster F3, §5.2).** Hay is
                        // delivered graze-flow: it enters the pen economy at exactly the point graze
                        // does, covering the gap the footprint left BEFORE any human food is hauled.
                        // Gated on **Foddering** — no Foddering, no draw, and everything below is
                        // byte-identical to the pre-F3 pasture-only pen. The draw is bounded by the gap
                        // AND the `FODDER` store (a stock — this is the buffer the overwintering carry
                        // rides), and `LocalStore::take` returns what it *actually* took.
                        let grass_shortfall = (demand_grass - herd.footprint_intake).max(0.0);
                        let fodder_draw = if grass_shortfall > 0.0
                            && knows(
                                &discovery,
                                faction,
                                FODDERING_DISCOVERY_ID,
                                knowledge_threshold,
                            ) {
                            cohort
                                .stores
                                .take(FODDER, scalar_from_f32(grass_shortfall))
                                .to_f32()
                        } else {
                            0.0
                        };
                        herd.fodder_draw = fodder_draw;
                        // The share fed by the LAND and HAY together (grass + delivered hay), before
                        // the larder is touched. Hay *is* feed, so it pays down the larder bill exactly
                        // as pasture does — one term, both jobs.
                        let land_hay_fraction = if demand_grass > 0.0 {
                            ((herd.footprint_intake + fodder_draw) / demand_grass).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        // **The three-terms-of-one-demand split (Flora Roster F3).** The gross bread
                        // bill (`pen_upkeep`, on the SAME basis `corralYield` uses) is paid down by three
                        // sources that PARTITION it — the footprint's pasture, delivered hay, and the
                        // larder. Stamp the two NET, food-unit terms the client renders (pasture is
                        // `gross × pen_pasture_fraction`, so it needs no field of its own), ready to draw
                        // "Fed by pasture NN% · hay X.X · larder Y.Y" with zero client arithmetic:
                        //   pasture_food + pen_hay_food + pen_larder_bill == gross   (± f32 epsilon)
                        // Hay's food-equivalent is the share of the bread bill it paid off — its grass
                        // draw over the grass demand — converting `fodder_draw` out of grass units (~25×
                        // the food scale) so it sits in the same row as the food-unit pasture/larder
                        // terms. Computed from the same locals, so the wire cannot disagree with what the
                        // pen paid.
                        let gross_upkeep = pen_upkeep(herd, &fauna);
                        herd.pen_hay_food = if demand_grass > 0.0 {
                            gross_upkeep * (fodder_draw / demand_grass)
                        } else {
                            0.0
                        };
                        let demand = gross_upkeep * (1.0 - land_hay_fraction);
                        // The NET larder bill after pasture + hay — the exact number billed just below.
                        herd.pen_larder_bill = demand;
                        let paid = cohort.stores.take(FOOD, scalar_from_f32(demand)).to_f32();
                        pen_feed_paid += paid;
                        // The herd's TOTAL fed fraction: the land+hay share plus the paid share of the
                        // (further-reduced) larder bill. Fully fed when the larder covers its remainder
                        // (or nothing was demanded). A pen fed by its grass and hay whose keeper can't
                        // pay is still fed by them — `land_hay_fraction`, never falsely 0 — so
                        // starvation/shrink sees a hayed pen as fed.
                        let larder_covered = if demand > 0.0 {
                            (paid / demand).clamp(0.0, 1.0)
                        } else {
                            1.0
                        };
                        herd.pen_fed_fraction =
                            land_hay_fraction + (1.0 - land_hay_fraction) * larder_covered;
                        // This band keeps this pen — its `K_pen` gets the fodder-flow term next turn.
                        kept_pens.push(fauna_id.clone());
                        // Shared with the pre-commit forecast (`fauna::hunt_forecast`) so the
                        // client's "expected yield" for a corralled herd is exactly what it is paid.
                        // **While EXTENDING the pen (2d-β) the keeper is fencing, not fully
                        // harvesting**, so the take is DIPPED to the pen rung's
                        // `yield_fraction_while_building` — the forgone yield IS the labor cost of the
                        // ring, and it is literally the same dip the corral *build* pays because both
                        // read the one rung (§4 "worked by the keeper band's labor, no materials").
                        let mut production = fauna::pen_yield_biomass(herd, &fauna);
                        if herd.pen_extending {
                            production *= pen_build_dip;
                        }
                        // **Collection** (slice 7 — the Field's twin): the keeper still has to carry
                        // the meat home, so the take is capped by the crew's own throughput — the
                        // *same* `per_worker_biomass_capacity` a wild hunt is capped by. The pen
                        // collapses the *policy* axis (the herd is yours), never the worker cap; one
                        // keeper used to collect the whole pen however big it grew.
                        //
                        // **And it is butchered in WHOLE ANIMALS** (slice 8 — the same
                        // `quantise_animal_take` a wild hunt runs): you cannot slaughter half a cow
                        // any more than you can half-kill a mammoth. A keeper who cannot haul a whole
                        // beast still takes one and wastes the rest.
                        //
                        // **The pen nonetheless reads steady — emergently, not by exemption.** It
                        // breeds at up to 3× the wild rate (`pen_gain`), so its MSY clears one body's
                        // worth every turn for every pennable species and `affordable >= 1` always
                        // holds. A herd that breeds fast enough to slaughter from continuously never
                        // has to wait — that is the real-world reason a pen is a steady supply, and
                        // rung 3's actual payoffs are the faster `r`, no chasing, the self-feeding
                        // footprint and a `K` you control. On poor enough range a pen *will* pulse
                        // (the aurochs is closest), and that is honest. See `managed_yield_biomass`.
                        let collection = workers as f32 * labor.hunt.per_worker_biomass_capacity;
                        let take =
                            // A penned animal is not stalked: no engagement bound.
                            fauna::quantise_animal_take(
                                production,
                                collection,
                                herd.body_mass,
                                f32::INFINITY,
                            );
                        herd.biomass -= take.killed_biomass();
                        // **A pen changes the INTENSITY, never the PRODUCT** — the keeper is paid
                        // this herd's own species vector, so a penned wolf yields pelts and no meat
                        // exactly as a wild one does (`docs/plan_hunt_yield_model.md`).
                        let pen_yield = herd_hunt_yield(herd, &fauna);
                        let paid = pen_yield.apply(take.carried, mult_f);
                        let provisions = scalar_from_f32(paid.provisions);
                        if provisions > scalar_zero() {
                            cohort.stores.add(FOOD, provisions);
                        }
                        // **THE earn path, rung 3** — *you learn to hay a herd by keeping one*
                        // (`animal:pen` earns Foddering). The pen's take is its managed production at
                        // every floor, so the keeper's dial is inert and the lesson runs at
                        // [`MANAGED_SOURCE_FLOOR`]; the work is the tending, not the slaughter.
                        credit_rung_lesson(
                            lesson_rung,
                            MANAGED_SOURCE_FLOOR,
                            MANAGED_SOURCE_IS_TENDED,
                            knowledge_dials,
                            faction,
                            &mut discovery,
                        );
                        // Trade goods land in the keeper band's own store, like the food beside them,
                        // and are scaled off what was **carried home**.
                        let pen_trade = scalar_from_f32(paid.trade_goods);
                        if pen_trade > scalar_zero() {
                            cohort.stores.add(TRADE_GOODS, pen_trade);
                        }
                        let tended = provisions.to_f32();
                        // Accrue the extension ring **after** the take (mirroring `accrue_corral`), so
                        // this turn pays exactly the dipped yield the forecast promised; the completed
                        // larger footprint's higher K arrives on the next `advance_herds`.
                        if herd.pen_extending
                            && herd.accrue_pen_extension(pen_build_rate, husbandry.pen_radius_max)
                        {
                            let pen_tile = herd.corralled_at.unwrap_or_else(|| herd.position());
                            event_log.push(CommandEventEntry::new(
                                tick.0,
                                CommandEventKind::Corral,
                                faction,
                                format!(
                                    "Extended the pen for {} to radius {}",
                                    fauna_id, herd.pen_radius
                                ),
                                Some(format!(
                                    "status=extended action=extend_pen herd={} radius={} x={} y={}",
                                    fauna_id, herd.pen_radius, pen_tile.x, pen_tile.y
                                )),
                            ));
                        }
                        // A *managed* harvest never overdraws — it takes at most the escapement MSY —
                        // so `sustainable == actual` (no overdraw ⚠). The two staffing signals are
                        // derived like every other rung's: how many keepers the take really needed,
                        // and how much of the harvest went uncollected for want of hands. **`wasted`
                        // is measured against the animals SLAUGHTERED, not against the pen's offered
                        // escapement** (slice 8): a beast the keeper never killed is still standing in
                        // the pen, alive and breeding — it was never produced, so it cannot have been
                        // wasted. What `killed_biomass − carried` measures is meat that really rotted.
                        // **The arrival schedule — computed POST-take, unlike `realized`.** It
                        // answers "when does the next food land", so it must start from the state the
                        // turn leaves behind: projecting from the pre-take state would re-promise the
                        // delivery this turn has already paid. Slot 0 is therefore genuinely the
                        // *next* turn's delivery.
                        let arrivals = fauna::project_arrivals_hunt(
                            herd,
                            &fauna,
                            &ladder,
                            labor.hunt.per_worker_biomass_capacity,
                            mult_f,
                            workers,
                            *floor,
                            improvement,
                            arrivals_horizon,
                        );
                        yields[idx] = SourceYield {
                            actual: tended,
                            // A penned wolf pays pelts; the pen changes the intensity, not the product.
                            trade: paid.trade_goods,
                            realized_trade: hunt_realized.trade_goods,
                            sustainable: tended,
                            // The forward-projected steady headline (computed pre-take above; a pen
                            // projects its managed yield, already smooth).
                            realized: hunt_realized.provisions,
                            arrivals,
                            wasted: pen_yield.apply(take.wasted, mult_f).provisions,
                            // **ONE CREW doing both jobs** ([`source_crew_needed`]): big enough to
                            // mind the heads *and* to haul the meat. The haul side is the **steady
                            // peak-drop carry crew** ([`fauna::hunt_haul_workers`]) off the pen's
                            // per-turn `production`, NOT this turn's lumpy `take.carried` — a slow-
                            // breeding pen (the aurochs pulses) drops 0 animals on a wait turn, which
                            // would collapse the crew to the herder count and contradict `wasted`.
                            workers_needed: source_crew_needed(
                                herders_needed,
                                fauna::hunt_haul_workers(
                                    production,
                                    herd.body_mass,
                                    labor.hunt.per_worker_biomass_capacity,
                                ),
                            ),
                            overdraws: false,
                        };
                        continue;
                    }
                    // Take food via the shared primitive: the per-policy escapement ceiling, rounded
                    // to **whole animals** against the crew's collection (slice 8). It hands back the
                    // kill in biomass — killed / carried / wasted — and has already drawn every animal
                    // killed off the herd.
                    let biomass_before = herd.biomass;
                    // **The escapement room, resolved PRE-take** — the stock standing above this
                    // assignment's floor, in biomass and before the whole-animal quantiser. Two
                    // readers below: the work predicate ([`crew_is_working_the_source`], which
                    // replaced this arm's `EcologyPhase::Thriving` gate) and the crew the telemetry
                    // row sizes off what the herd offered.
                    let standing_above_floor = fauna::hunt_escapement_ceiling(
                        *floor,
                        biomass_before,
                        herd_capacity(herd, &fauna),
                    );
                    let working_the_herd = crew_is_working_the_source(standing_above_floor);
                    // The band has no carry room — it eats/banks whatever it hauls, so pass an
                    // unbounded carry cap (behaviour unchanged from before the expedition clamp).
                    let take = hunt_take(
                        herd,
                        workers,
                        *floor,
                        improvement,
                        labor.hunt.per_worker_biomass_capacity,
                        &fauna,
                        &ladder,
                        f32::INFINITY,
                        fauna::retreat_seed(sim_config.map_seed, tick.0, &herd.id, workers),
                    );
                    // **THE earn path, rungs 1–2** — the drawn-down half of the split above, and the
                    // heart of the ladder: the same hunt teaches **Herding** on a wild herd and
                    // **Penning** on a tamed one. The gate is the **escapement room**, never
                    // `take.killed > 0`: a herd whose room is lighter than one body hands over
                    // nothing this turn while the crew tracks and handles it exactly as before, and
                    // reading that as *"not working"* would pace the whole ladder off `body_mass`.
                    // See [`crew_is_working_the_source`]. It replaced the `EcologyPhase::Thriving`
                    // gate this site used to carry, and it is what makes `floor = 1.0` (leave the
                    // whole herd standing, learn at ×2) honestly earn nothing.
                    credit_rung_lesson(
                        lesson_rung,
                        *floor,
                        working_the_herd,
                        knowledge_dials,
                        faction,
                        &mut discovery,
                    );
                    // **THE take's yield: product × intensity** (`docs/plan_hunt_yield_model.md`).
                    // `hunt_take` above decided HOW MUCH biomass came home (the policy's job); the
                    // species' `HuntYield` decides WHAT that biomass is worth, in one call that
                    // yields both products so neither can be converted without the other.
                    let hunt_yield = herd_hunt_yield(herd, &fauna);
                    let paid = hunt_yield.apply(take.carried, mult_f);
                    let provisions = scalar_from_f32(paid.provisions);
                    // **Tame — the investment** (the animal twin of Cultivate, and the rung
                    // below Corral). The crew is gentling the herd, not hunting it: `hunt_take`
                    // above already paid only the reduced Tame ceiling (the `animal:pastoral` rung's
                    // `yield_fraction_while_building × MSY` — the up-front cost), and here the herd
                    // accrues toward pastoral. Gates: the faction must **know Herding** (earned by
                    // hunting, above), the species' husbandry ceiling must allow taming
                    // (Grazing 2d-δ — a `wild`-ceiling species never tames; `accrue_domestication`
                    // self-guards too, and the command path rejects it, so this is belt and braces),
                    // and the herd must be **standing above the crew's floor**
                    // ([`crew_is_working_the_source`] — not "an animal died", which is a
                    // quantisation fact rather than a fact about work).
                    //
                    // **There is no health gate any more** (`docs/plan_harvest_floor.md` §3.2), the
                    // plant side's change applied to the identical `EcologyPhase::Thriving` condition
                    // here. The floor replaced the cliff with a rate: gentling a herd you are pulling
                    // hard on is *slow*, not *stopped*, so there is no lapse state left to hold
                    // progress across. `validate_tame` never had a phase gate, so the command side
                    // was already consistent with removing it.
                    //
                    // **Ownership is NOT in `eligible`** — `accrue_domestication` owns the
                    // `owner is None || owner == faction` rule (and sets ownership on first accrual),
                    // exactly as `accrue_cultivation` owns it on the plant side. One rule, one place.
                    //
                    // **Ordering: accrue AFTER the take** (mirrors Cultivate/Corral), so this turn
                    // pays exactly the dipped yield the pre-commit forecast promised.
                    if improvement == Some(Improvement::Tame) {
                        // Marked worked-as-improvement so `advance_husbandry` spares it: a herd
                        // under active taming neither goes feral nor bleeds its partial progress.
                        herd.tamed_this_turn = true;
                        let eligible =
                            pastoral_rung.unlock_discovery_id().is_none_or(|knowledge| {
                                knows(&discovery, faction, knowledge, knowledge_threshold)
                            }) && herd.can_domesticate()
                                && working_the_herd;
                        // THE build seam — the same call the plant side's Cultivate arm makes, at
                        // **this species' own taming timescale** (slice 3c): the rung owns the
                        // mechanic, the species scales it (rabbit ×1.0 → 25 turns, Steppe Runner ×0.2
                        // → 125). The seam applies the multiplier to the decay too, so a herd that is
                        // slow to tame is equally slow to forget — see `RungDef::build_accrual`.
                        // The **floor** is the assignment's own, the same dial that paced the lesson
                        // above; it rides *beside* the timescale rather than folding into it, because
                        // the timescale reaches the decay and the floor must not.
                        let accrual = pastoral_rung.build_accrual(
                            improvement,
                            eligible,
                            *floor,
                            fauna.taming_rate_for(&herd.species),
                            workers,
                        );
                        // The TRANSITION, not the state (the Cultivate arm's rule): a second band
                        // taming the same herd clears its verb via the already-built check above
                        // without re-announcing the taming.
                        if accrual > 0.0 && herd.accrue_domestication(faction, accrual) {
                            completed.push(idx);
                            event_log.push(CommandEventEntry::new(
                                tick.0,
                                CommandEventKind::Tame,
                                faction,
                                format!("Tamed the {} herd", herd.species),
                                Some(format!("status=complete action=tame herd={}", herd.id)),
                            ));
                        }
                    }
                    // **Corral — the investment** (the animal twin of Cultivate). The crew is
                    // building the pen, not hunting: `hunt_take` above already paid only the reduced
                    // Corral ceiling (the rung's `yield_fraction_while_building × MSY` — the up-front
                    // cost), and here the pen accrues. Gates: the faction must **know Penning** (the
                    // rung's own `unlock_knowledge` — Herding gates `tame` alone since §4.3) and **own a
                    // domesticated herd**. A gate that lapses mid-build just stops accrual that turn
                    // (progress is kept — a half-built pen is materials on the ground). Accrued
                    // **after** the take, so this turn pays exactly what the pre-commit forecast
                    // promised; the corral yield starts the turn after the pen completes.
                    if improvement == Some(Improvement::Corral) {
                        // The rung's own gates, resolved for the engine: the faction knows the rung's
                        // unlock knowledge (Herding today), the species' husbandry ceiling reaches
                        // this rung (Grazing 2d-δ: only a `Pen`-ceiling species may build a pen — a
                        // `Wild`/`Pastoral` herd never accrues, and the command path rejects it too,
                        // so this is belt and braces), the herd has climbed the rung below, and the
                        // faction owns it.
                        let eligible = pen_rung.unlock_discovery_id().is_none_or(|knowledge| {
                            knows(&discovery, faction, knowledge, knowledge_threshold)
                        }) && herd.can_pen()
                            && herd.is_domesticated()
                            && herd.owner == Some(faction);
                        // THE build seam — the same call the plant side's Cultivate arm makes.
                        // Penning is a flat build for every species — only *taming* varies (slice
                        // 3c): a fence is a fence. The **floor** paces it as it paces every build.
                        //
                        // **The work predicate is deliberately NOT in `eligible` here**, for
                        // `accrue_field`'s reason (see there): it replaced a rung's
                        // `EcologyPhase::Thriving` gate, and rung 3 never had one on either web.
                        // Fencing a herd is ground work — a pen goes up around a flock already drawn
                        // down to its keeper's own floor.
                        let accrual = pen_rung.build_accrual(
                            improvement,
                            eligible,
                            *floor,
                            RUNG_TIMESCALE_UNSCALED,
                            workers,
                        );
                        if accrual > 0.0 {
                            let pen_tile = herd.position();
                            if herd.accrue_corral(faction, accrual, pen_tile) {
                                completed.push(idx);
                                event_log.push(CommandEventEntry::new(
                                    tick.0,
                                    CommandEventKind::Corral,
                                    faction,
                                    format!(
                                        "Corralled {} at ({}, {})",
                                        fauna_id, pen_tile.x, pen_tile.y
                                    ),
                                    Some(format!(
                                        "status=complete action=corral herd={} x={} y={}",
                                        fauna_id, pen_tile.x, pen_tile.y
                                    )),
                                ));
                            }
                        }
                    }
                    // **EVERY extractive rung sells, including Eradicate.** The retired 4×
                    // `market.trade_goods_multiplier` on the Deplete rung re-welded product to
                    // policy — the thing this arc removes. Deplete still out-earns Sustain on trade,
                    // because it *takes* 2.5× more biomass: that is the intensity ladder doing the
                    // work, not a per-rung bonus.
                    //
                    // Both accounts are fully fractional and band-local. Both scale off the meat
                    // actually **carried home**, not the animals killed: you cannot trade a hide you
                    // left on the range.
                    let trade_goods = scalar_from_f32(paid.trade_goods);
                    if provisions > scalar_zero() {
                        cohort.stores.add(FOOD, provisions);
                    }
                    if trade_goods > scalar_zero() {
                        cohort.stores.add(TRADE_GOODS, trade_goods);
                    }
                    // **The LONG-RUN sustainable rate** — one turn's net regrowth at the herd's
                    // **pre-take** biomass (the herd's OWN ecology/capacity: a tamed herd grows 1.5×
                    // faster, so its sustainable skim is 1.5× a wild one's).
                    //
                    // Since slice 8 this is deliberately **not** comparable to `actual` turn by turn:
                    // a whole-animal take pays in lumps (nothing for 6 turns, then a whole mammoth),
                    // so `actual` swings around this rate rather than tracking it. That swing is
                    // *true* and it is the mechanic — so `sustainable` keeps reporting the honest
                    // average ("this herd sustains ~0.78/turn"), and whether the take **overdraws** is
                    // answered by the policy's own floor (`overdraws` below) instead of by comparing
                    // the two. See `SourceYield`.
                    let sustainable = hunt_yield
                        .apply(
                            sustainable_yield(
                                biomass_before,
                                herd_capacity(herd, &fauna),
                                &herd_ecology(herd, &fauna),
                            ),
                            mult_f,
                        )
                        .provisions;
                    // The two staffing signals, from the same take. **Overstaffing**: invert the
                    // carried biomass by the per-hunter throughput (hunt has no seasonal factor,
                    // unlike forage). **Understaffing** (`wasted`): the meat the crew killed but could
                    // not haul — **a real loss**, left to rot on the range. Measured against the
                    // animals *slaughtered*, never against the escapement the herd could have spared:
                    // an animal nobody killed is still alive out there, so it was never produced and
                    // cannot have been wasted (`fauna::forecast_production_and_take`).
                    //
                    // **A MANAGED herd reports its whole CREW** ([`source_crew_needed`]) — the
                    // herders who mind it are the ones who take from it, and the crew must be big
                    // enough for both jobs. A **wild** herd is untouched by the herder term:
                    // `herders_needed` is `0` (it isn't yours to maintain), so the `max` collapses to
                    // the haul-side count.
                    //
                    // The haul side is the **peak-drop carry crew** ([`fauna::hunt_haul_workers`]) off
                    // the SAME escapement ceiling the take was bounded by — NOT this turn's lumpy
                    // `take.carried`. A slow breeder whose room is lighter than one body carries `0` on
                    // a wait turn, which would collapse `workers_needed` and contradict `wasted_yield`;
                    // sizing off the ceiling keeps the two in agreement and equals the client's
                    // max-useful count. It is re-derived at the **pre-take** biomass, which is what
                    // `hunt_take` read, so the crew describes the take that was just paid.
                    // The **ceiling** is the same pre-take escapement room the work predicate read,
                    // and it is undipped: the herd offers what stands above the floor whether the
                    // hunters are harvesting it or gentling it. **The THROUGHPUT is dipped**, because
                    // §3.1 put the build dip on the crew — a gentling hunter hauls
                    // `yield_fraction_while_building ×` what a harvesting one does, so it takes
                    // proportionally more of them to clear the same room. Dividing an undipped rate
                    // into a room the take is dipped against sized the crew at the harvesting count
                    // and then paid it the building take: the row read "enough hands" while the crew
                    // demonstrably could not lift the drop, and it disagreed with the client's own
                    // cap (`SourceForecast.max_useful_workers`, which divides by `carry × dip`) by
                    // exactly the dip. Read through the one [`LadderConfig::build_dip`] seam so the
                    // two webs cannot dip differently.
                    let take_workers = fauna::hunt_haul_workers(
                        standing_above_floor,
                        herd.body_mass,
                        labor.hunt.per_worker_biomass_capacity * ladder.build_dip(improvement),
                    );
                    let workers_needed = source_crew_needed(herders_needed, take_workers);
                    // **The arrival schedule — computed POST-take, unlike `realized`.** It
                    // answers "when does the next food land", so it must start from the state the
                    // turn leaves behind: projecting from the pre-take state would re-promise the
                    // delivery this turn has already paid. Slot 0 is therefore genuinely the
                    // *next* turn's delivery.
                    let arrivals = fauna::project_arrivals_hunt(
                        herd,
                        &fauna,
                        &ladder,
                        labor.hunt.per_worker_biomass_capacity,
                        mult_f,
                        workers,
                        *floor,
                        improvement,
                        arrivals_horizon,
                    );
                    yields[idx] = SourceYield {
                        actual: provisions.to_f32(),
                        // **The other currency this take produced.** Never summed into `food_income`
                        // (`docs/plan_hunt_yield_model.md` §9) — that would break the larder identity.
                        trade: paid.trade_goods,
                        realized_trade: hunt_realized.trade_goods,
                        sustainable,
                        wasted: hunt_yield.apply(take.wasted, mult_f).provisions,
                        workers_needed,
                        overdraws: floor_overdraws(*floor),
                        // The forward-projected steady headline (computed pre-take above): rate-based,
                        // so it is smooth where `actual` (the whole-animal kill) pulses.
                        realized: hunt_realized.provisions,
                        arrivals,
                    };
                    // **Predators Phase 0 — the hunt turns dangerous** (`docs/plan_predators.md`).
                    // A herd whose species can fight back (`combat.attack > 0` — mammoth, ox) turns on
                    // the party after the take resolves. It composes a fight (the hunters assigned to
                    // this herd vs the beast's fighting stock), resolves it through the neutral combat
                    // subsystem, and applies **only the band-side** casualties — the take path already
                    // removed the animal's biomass, so applying the animal-side result too would
                    // double-count (discarded in Phase 0).
                    if let Some(species) = fauna.species_by_display(&herd.species) {
                        // **Danger = strength × BEHAVIOUR** (`docs/plan_predators.md`): a hunt only faces
                        // the animal's attack to the extent it *fights back* rather than flees, so the
                        // beast's effective attack is `attack × ferocity`. A fleeing deer (ferocity ~0.15)
                        // costs almost nothing; a cornered boar (0.6) does; a mammoth (0.9) is deadly.
                        let effective_attack = species.combat.attack * species.ferocity;
                        if effective_attack > 0.0 {
                            // **The hunting party answers the danger itself** — its defending strength is
                            // just the hunters assigned to THIS herd (bare-hands `person` profile today).
                            // Warriors are a band-wide standing guard (border/camp patrol) and do NOT
                            // mitigate a hunt; the hunters' own equipment (TOE, deferred) will compose
                            // into this profile when it lands, with no rework here.
                            let party_count = workers as f32;
                            // The animal fights at its ferocity-scaled attack (defense/range unchanged).
                            let animal_profile = CombatStats {
                                attack: effective_attack,
                                ..species.combat
                            };
                            // A single beast turns on the party each dangerous hunt-turn — a deliberate
                            // Phase-0 simplification (scaling the engaged count with take/party size is a
                            // later refinement). Its intrinsic combat body is the same `attack` predation
                            // will one day read.
                            // Deterministic, rollback-stable seed (reserved/unused by the placeholder
                            // resolver, but a real value): map_seed ^ tick ^ herd-id hash.
                            let mut hasher = crate::hashing::FnvHasher::new();
                            std::hash::Hash::hash(&herd.id, &mut hasher);
                            let seed = map_seed ^ tick.0 ^ std::hash::Hasher::finish(&hasher);
                            let payload = FightPayload {
                                sides: vec![
                                    Force {
                                        id: ForceId(0),
                                        posture: Posture::Aggressor,
                                        contingents: vec![Contingent {
                                            kind: ContingentId::from("person"),
                                            count: party_count,
                                            profile: person_profile,
                                        }],
                                    },
                                    Force {
                                        id: ForceId(1),
                                        posture: Posture::Defender,
                                        contingents: vec![Contingent {
                                            kind: ContingentId(herd.species.clone()),
                                            count: 1.0,
                                            profile: animal_profile,
                                        }],
                                    },
                                ],
                                terrain: vec![TerrainContext {
                                    hex: (band_pos.x, band_pos.y),
                                }],
                                seed,
                            };
                            let outcome = resolve_fight(&payload, &combat_tuning);
                            // Apply ONLY the band side (`ForceId(0)`); discard the animal side.
                            let band_side = outcome
                                .results
                                .iter()
                                .find(|r| r.force == ForceId(0))
                                .map(|r| (r.killed, r.wounded))
                                .unwrap_or((0.0, 0.0));
                            let (killed_f, wounded_f) = band_side;
                            if killed_f + wounded_f > 0.0 {
                                // `killed` come out of the working-age bracket (the new casualty
                                // mortality path); `wounded` is **computed and surfaced but mechanically
                                // inert this phase** — no capacity/recovery effect yet (a later slice).
                                cohort.apply_combat_casualties(scalar_from_f32(killed_f));
                                // The prose rounds `killed` for a readable "cost N lives"; the **detail
                                // carries the fractional truth** (casualties are `Scalar`-fractional by
                                // design — a well-guarded party takes a fraction of a death), so a
                                // consumer reads precise killed/wounded rather than a rounded 0.
                                let killed_r = killed_f.round() as u32;
                                event_log.push(CommandEventEntry::new(
                                    tick.0,
                                    CommandEventKind::HuntDanger,
                                    faction,
                                    // Human text names the SPECIES, never the internal herd id.
                                    format!(
                                        "The {} hunt cost {} lives",
                                        species.display_name, killed_r
                                    ),
                                    Some(format!(
                                        "killed={:.3} wounded={:.3} species={}",
                                        killed_f, wounded_f, species.display_name
                                    )),
                                ));
                            }
                        }
                    }
                }
                LaborTarget::Scout => {
                    // Scouts act as forward observers in `calculate_visibility`: staffed scouts
                    // post vantage points out from the band (`labor.scout.vantage_distance(scouts)`)
                    // and reveal from each, re-marked Active every turn — no work is done here.
                }
                LaborTarget::Warrior => {
                    // Still a no-op **in the labor pass** — warriors do no per-worker yield here, and
                    // they are a band-wide standing guard (border/camp patrol), not a hunting escort, so
                    // they do **not** mitigate hunt danger (the hunting party answers that itself, via
                    // its own equipment). But warriors are **no longer inert overall** (Phase 1b): the
                    // warrior head-count is now **consumed by [`advance_predator_raids`]** as the band's
                    // defending contingent when a carnivore raids its camp. Keep this branch.
                }
            }
        }
        // **Stamp the fodder-flow rate onto every pen this band keeps** (Flora Roster F3, §5.3), now
        // that the whole band's hay harvest (`band_fodder_inflow`) is summed. Split evenly across the
        // band's pens so the *total* K contribution reflects the *total* hay grown, not N copies of
        // it. Read next turn by `ecological_carrying_capacity` (the one-turn Logistics-reads-Population
        // lag). **Gated on Foddering** exactly as the feed draw is: a faction that grew hay but has not
        // learned to hay a herd delivers nothing to the pen's ceiling, so `K_pen` stays byte-identical
        // to its footprint-only self — the fodder term is all-or-nothing with the capability, never a
        // free K boost from unusable hay. Always written (0 when un-foddered), so a pen a band stops
        // keeping does not carry a stale rate.
        if !kept_pens.is_empty() {
            let per_pen = if knows(
                &discovery,
                faction,
                FODDERING_DISCOVERY_ID,
                knowledge_threshold,
            ) {
                band_fodder_inflow / kept_pens.len() as f32
            } else {
                0.0
            };
            for fauna_id in &kept_pens {
                if let Some(herd) = registry.herds.iter_mut().find(|herd| &herd.id == fauna_id) {
                    herd.fodder_delivery_rate = per_pen;
                }
            }
        }
        // **Clear the improvement of every build that completed this turn** — the one seam all four
        // rungs (Cultivate/Sow/Tame/Corral) hand off through. There is nothing left to build on this
        // source, so leaving the verb set would charge `yield_fraction_while_building` forever on a
        // rung that can never accomplish anything more (issue #420).
        //
        // **The STANCE is not touched, and that is the whole point of the two-axis split** (issue
        // #442, `docs/plan_investment_rung_toggle.md` §1). This pass used to *rewrite* `policy` onto
        // a module constant (`HARVEST_POLICY_AFTER_BUILD = Sustain`) — the sim silently replacing the
        // player's stated policy on a turn they could not predict — because the build verb had
        // occupied the stance slot and completion had to hand something back. With the verb in its own
        // slot the stance was never vacated: the crew, the tile and its committed `species` (or the
        // herd id) and the stance all simply stay as they are, and only `improvement` returns to
        // `None`.
        //
        // **This turn's take is already banked above and is NOT rewound** — the turn a meter reaches
        // `1.0` is the last preparing take, exactly as the accrue-after-take ordering promises the
        // pre-commit forecast. The undipped ceiling starts paying next turn.
        //
        // **Before the `lapsed` removal below**, which shifts rows and invalidates these indices.
        for idx in &completed {
            if let Some(assignment) = allocation.assignments.get_mut(*idx) {
                assignment.improvement = None;
            }
        }
        // Drop lapsed sources — Forage (tile out of work range) or Hunt (herd past the leash or
        // gone) — in reverse order to keep indices valid; workers return to the pool.
        // Remove the matching telemetry rows too so `last_yields` stays index-aligned with the
        // surviving assignments (lapsed rows carry a 0 yield anyway).
        for idx in lapsed.into_iter().rev() {
            allocation.assignments.remove(idx);
            yields.remove(idx);
        }
        allocation.last_yields = yields;
        allocation.last_pen_feed_upkeep = pen_feed_paid;
    }
}

/// **Say what the band just stopped doing** — the feed line for an assignment
/// [`LaborAllocation::normalize`] trimmed away because the band no longer has the workers for it.
///
/// Shaped like the out-of-range Forage lapse it sits beside: the source named in the label, a
/// `status=lapsed reason=…` detail, and the verb's own `CommandEventKind` so the line lands on the
/// channel the player is already watching for that source.
///
/// **The improvement is named explicitly when one was in flight**, because that is the expensive
/// half: workers come back, but a build meter that stops being worked starts reverting, and a
/// 25-turn commitment is exactly the thing a player must not lose without being told. A lapse with no
/// build says so plainly instead of leaving a blank where the interesting clause would be.
fn announce_dropped_assignment(
    event_log: &mut CommandEventLog,
    tick: u64,
    faction: FactionId,
    assignment: &LaborAssignment,
) {
    // A band-wide role (Scout/Warrior) has no source to name and no verb channel of its own; it is
    // reported on the label alone, through the role's own kind where one exists.
    let (kind, source_label, source_detail) = match &assignment.target {
        LaborTarget::Forage { tile, .. } => (
            CommandEventKind::Forage,
            format!("foragers at ({}, {})", tile.x, tile.y),
            format!("kind=forage x={} y={}", tile.x, tile.y),
        ),
        LaborTarget::Hunt { fauna_id, .. } => (
            CommandEventKind::Hunt,
            format!("hunters on {fauna_id}"),
            format!("kind=hunt herd={fauna_id}"),
        ),
        LaborTarget::Scout => (
            CommandEventKind::Scout,
            "scouts".to_string(),
            "kind=scout".to_string(),
        ),
        LaborTarget::Warrior => (
            CommandEventKind::CancelOrder,
            "warriors".to_string(),
            "kind=warrior".to_string(),
        ),
    };
    // The build verb, if one was in flight — appended to both halves rather than folded in, so a
    // lapse that cost nothing but hands does not read as though it cost an investment.
    let lost_build = assignment
        .improvement
        .map(|improvement| improvement.as_str());
    let (label, build_detail) = match lost_build {
        Some(verb) => (
            format!(
                "{source_label} disbanded — too few workers, and the {verb} underway there is \
                 abandoned"
            ),
            format!(" action={verb}"),
        ),
        None => (
            format!("{source_label} disbanded — too few workers"),
            String::new(),
        ),
    };
    event_log.push(CommandEventEntry::new(
        tick,
        kind,
        faction,
        label,
        Some(format!(
            "status=lapsed reason=too_few_workers {source_detail} workers={}{build_detail}",
            assignment.workers,
        )),
    ));
}

/// **Has this patch already climbed the rung `improvement` builds?** The plant half of the
/// completion seam's "nothing left to build" test, asked once per worked source *before* the arm
/// branches by rung — so it reaches a finished Field, whose managed branch returns early and never
/// visits the build blocks.
///
/// It answers **`false` for the animal verbs**, which is the honest reading rather than a defensive
/// one: nothing has been built toward `Tame` on a patch and nothing ever will be. That state is
/// unreachable anyway — `validate_improvement` refuses a cross-web verb at every command path — and
/// answering `true` would silently *clear* a mis-set verb instead of leaving the evidence in place.
///
/// **`Cultivate` is answered by `is_managed()`, not `is_cultivated()` — a Field is above rung 2.**
/// `Sow` needs no prior patch, so a Field can stand on ground that was never tended
/// (`cultivation_progress == 0`), and on such a patch `is_cultivated()` is false while the Field arm
/// `continue`s past the Cultivate block entirely: the verb was neither cleared nor accrued, so a
/// `cultivate` on a wild-sown Field **stalled forever, silently**, and only `abandon_improvement`
/// could clear it. Reading the *whole* managed state answers the question the seam is actually
/// asking — *is there anything left to build at this rung on this source* — and a Field that later
/// lapses flips the answer back, because this is evaluated against the current state each turn.
fn forage_rung_already_built(patch: &ForagePatch, improvement: Improvement) -> bool {
    match improvement {
        Improvement::Cultivate => patch.is_managed(),
        Improvement::Sow => patch.is_field(),
        Improvement::Tame | Improvement::Corral => false,
    }
}

/// The animal twin of [`forage_rung_already_built`], with the same cross-web rule.
fn hunt_rung_already_built(herd: &Herd, improvement: Improvement) -> bool {
    match improvement {
        Improvement::Tame => herd.is_domesticated(),
        Improvement::Corral => herd.is_corralled(),
        Improvement::Cultivate | Improvement::Sow => false,
    }
}

/// **The `plant:field` rung's build step**, factored out because the Forage arm reaches it from two
/// places — sowing a *wild/bare* patch (the take path) and sowing an *already tended* one (the managed
/// path) — and the two must not drift into different gates, rates or completion side-effects.
///
/// THE build seam: the rung supplies the accrual (`0` unless `Sow` is the rung's verb and `eligible`
/// holds); the patch owns its meter, the clamp, and ownership. `RUNG_TIMESCALE_UNSCALED` because
/// sowing is a flat 25 turns — the only per-source timescale on the ladder is a species' `taming_rate`
/// (a plant has no species).
///
/// `eligible` is the faction's **Seed Selection** gate and nothing else. A lapse just stops accrual
/// for the turn: progress is neither lost nor silently switched.
///
/// **It deliberately does NOT carry the work predicate** ([`crew_is_working_the_source`]), which
/// every other build gate gained with the harvest floor (`docs/plan_harvest_floor.md` §3.2). That
/// term replaced each rung's `EcologyPhase::Thriving` gate, and rung 3 never had one — for the reason
/// that also forbids the term: **bare ground stands below every floor**, by construction, so
/// requiring a positive escapement room would make the create-from-nothing case the rung exists for
/// impossible. `floor` still paces it, so a crew stripping the ground it is sowing still builds
/// nothing.
///
/// `workers` is the crew this assignment put on the tile: the rung's `crew_needed` scales the accrual
/// by `min(workers / crew_needed, 1)`, so a Sow the player under-staffed takes proportionally longer.
///
/// Returns **`true` when THIS call completed the Field** — the caller clears the assignment's
/// `improvement` on that signal. Shaped like `Herd::accrue_corral`'s completion bool rather than
/// swallowing the completion into the event push, so both plant build rungs report the same thing to
/// the same seam.
#[allow(clippy::too_many_arguments)] // the rung, the gate, the actor and the feed line are all inputs
fn accrue_field(
    patch: &mut ForagePatch,
    field_rung: &RungDef,
    improvement: Option<Improvement>,
    eligible: bool,
    floor: f32,
    faction: FactionId,
    event_log: &mut CommandEventLog,
    tick: u64,
    tile: UVec2,
    workers: u32,
) -> bool {
    let accrual = field_rung.build_accrual(
        improvement,
        eligible,
        floor,
        RUNG_TIMESCALE_UNSCALED,
        workers,
    );
    if accrual <= 0.0 {
        return false;
    }
    // The TRANSITION, not the state — `ForagePatch::accrue_field` answers "did this call finish it",
    // so a second band cannot re-announce a Field the first one sowed.
    if patch.accrue_field(faction, accrual) {
        event_log.push(CommandEventEntry::new(
            tick,
            CommandEventKind::Sow,
            faction,
            format!("Field sown at ({}, {})", tile.x, tile.y),
            Some(format!(
                "status=complete action=sow x={} y={}",
                tile.x, tile.y
            )),
        ));
        return true;
    }
    false
}

/// Layer 3b (wellbeing) — tech-gated migration: relocate-or-stay, population conserved within the
/// faction (`docs/plan_civ_wellbeing.md`). Runs in the Population stage **after** demographics so
/// morale is current. **Decoupled from `discontent_fraction`** (productivity-only): migration has its
/// own morale-scaled onset at `migration.morale_threshold` (0.25). Each band below the threshold
/// sheds `total × migration_move_fraction(morale)` people, composed mostly of working-age (the total
/// is split across brackets ∝ `bracket_size × weight`, working = 1.0, dependents =
/// `migration.dependent_weight`), who seek the highest-morale eligible same-faction band within
/// reach; found → they **relocate** (source shrinks, destination grows), none reachable → they
/// **stay** (grievance accrues faster via the trapped bonus). Morale NEVER causes faction population
/// loss.
///
/// Destinations are chosen from a single **pre-migration snapshot** of this turn's post-demographics
/// morale/brackets, and every move is computed before any is applied — so relocation is
/// order-independent (a band that receives immigrants this turn isn't re-evaluated as a fuller
/// source, and a source's outflow is unaffected by another source feeding it).
pub fn advance_population_migration(
    sim_config: Res<SimulationConfig>,
    wellbeing_config: Res<WellbeingConfigHandle>,
    tile_registry: Res<TileRegistry>,
    tiles: Query<&Tile>,
    // `With<ResidentBand>`: migration relocates people between real bands only — an expedition is
    // never a migration source or destination.
    mut cohorts: Query<(Entity, &mut PopulationCohort), With<ResidentBand>>,
) {
    let wellbeing = wellbeing_config.get();
    let disc_cfg = &wellbeing.discontent;
    let mig_cfg = &wellbeing.migration;
    let width = tile_registry.width;
    let wrap = sim_config.map_topology.wrap_horizontal;

    // Movement-tech reach factor. No concrete movement/transport tech signal exists in the sim yet
    // (capability flags cover construction/industry/power/naval/air/espionage/megaprojects, none of
    // which is a mobility tier), so Phase 1 keeps this at 1.0.
    // TODO(phase2): scale by the civilization's movement/transport tech tier (design doc defers
    // concrete tiers) so advanced factions send emigrants farther.
    let movement_tech_factor = 1.0_f32;
    let reach = mig_cfg.base_reach * movement_tech_factor;
    let reach_sq = (reach * reach) as i32;
    let attractive_morale = scalar_from_f32(mig_cfg.attractive_morale);
    let min_gap = scalar_from_f32(mig_cfg.min_morale_gap);
    let dependent_weight = scalar_from_f32(mig_cfg.dependent_weight);
    let morale_threshold = scalar_from_f32(mig_cfg.morale_threshold);

    // Pre-migration snapshot: everything the destination search + would-move sizing reads. The total
    // leaving is `total × move_fraction`, split across brackets ∝ `bracket_size × weight` so the
    // headline fraction is exact while working-age dominates the composition.
    struct Band {
        entity: Entity,
        faction: FactionId,
        pos: Option<UVec2>,
        morale: Scalar,
        wants_to_move: bool,
        move_working: Scalar,
        move_children: Scalar,
        move_elders: Scalar,
    }
    let mut bands: Vec<Band> = cohorts
        .iter()
        .map(|(entity, cohort)| {
            let move_fraction = migration_move_fraction(cohort.morale, mig_cfg);
            // Weighted bracket masses; the total is apportioned in proportion to these.
            let w_working = cohort.working;
            let w_children = cohort.children * dependent_weight;
            let w_elders = cohort.elders * dependent_weight;
            let denom = w_working + w_children + w_elders;
            // Clamp the headline leaving amount to the weighted denominator so no bracket can be
            // over-drafted (`move_x ≤ w_x ≤ bracket_x`), preserving faction population conservation.
            // A no-op under shipped tuning (`total × max_rate ≤ denom` always), but a safety net for
            // extreme-but-valid config (e.g. a very low `dependent_weight` on a dependent-heavy band).
            let total_leaving = (cohort.total() * move_fraction).min(denom);
            let (move_working, move_children, move_elders) = if denom > scalar_zero() {
                (
                    total_leaving * w_working / denom,
                    total_leaving * w_children / denom,
                    total_leaving * w_elders / denom,
                )
            } else {
                (scalar_zero(), scalar_zero(), scalar_zero())
            };
            Band {
                entity,
                faction: cohort.faction,
                pos: tiles.get(cohort.home).ok().map(|tile| tile.position),
                morale: cohort.morale,
                wants_to_move: total_leaving > scalar_zero(),
                move_working,
                move_children,
                move_elders,
            }
        })
        .collect();
    // Bevy query iteration order is not guaranteed stable across runs/rollback, but turn
    // resolution must be deterministic. Sort by entity id so the destination tie-break
    // (first-encountered wins on a morale tie) is reproducible.
    bands.sort_by_key(|b| b.entity.to_bits());

    // For each band that wants to move (morale below the migration threshold), find the
    // highest-morale eligible same-faction band within reach.
    let mut destination_of: Vec<Option<usize>> = vec![None; bands.len()];
    for i in 0..bands.len() {
        if !bands[i].wants_to_move {
            continue;
        }
        let Some(src_pos) = bands[i].pos else {
            continue;
        };
        let mut best: Option<(usize, Scalar)> = None;
        for (j, dest) in bands.iter().enumerate() {
            if j == i || dest.faction != bands[i].faction {
                continue;
            }
            let Some(dest_pos) = dest.pos else {
                continue;
            };
            // Eligible = meaningfully happier than a bare threshold AND than the source.
            if dest.morale < attractive_morale || dest.morale <= bands[i].morale + min_gap {
                continue;
            }
            if crate::grid_utils::wrapped_distance_sq(src_pos, dest_pos, width, wrap) > reach_sq {
                continue;
            }
            if best.is_none_or(|(_, m)| dest.morale > m) {
                best = Some((j, dest.morale));
            }
        }
        destination_of[i] = best.map(|(j, _)| j);
    }

    // Accumulate per-band bracket deltas + head-count tallies from all moves (computed against the
    // snapshot), then apply in one mutating pass so relocation is order-independent.
    let mut deltas: HashMap<Entity, (Scalar, Scalar, Scalar)> = HashMap::new();
    let mut emigrated: HashMap<Entity, u32> = HashMap::new();
    let mut immigrated: HashMap<Entity, u32> = HashMap::new();
    for (i, dest) in destination_of.iter().enumerate() {
        let Some(j) = *dest else { continue };
        let src_entity = bands[i].entity;
        let dest_entity = bands[j].entity;
        let (mw, mc, me) = (
            bands[i].move_working,
            bands[i].move_children,
            bands[i].move_elders,
        );
        let moved_head = (mw + mc + me).round().to_u32();
        if moved_head == 0 {
            continue;
        }
        let src = deltas.entry(src_entity).or_default();
        src.0 -= mw;
        src.1 -= mc;
        src.2 -= me;
        let dst = deltas.entry(dest_entity).or_default();
        dst.0 += mw;
        dst.1 += mc;
        dst.2 += me;
        *emigrated.entry(src_entity).or_default() += moved_head;
        *immigrated.entry(dest_entity).or_default() += moved_head;
    }

    // Apply relocation + refresh the derived per-turn emigrant/immigrant readouts + accrue/decay
    // the grievance accumulator. Base accrual is `grievance_gain × discontent_fraction` (the 0.6
    // discontent onset, unchanged); the trapped bonus applies specifically when the band is below
    // the migration threshold (people *want* to leave) AND has no reachable destination.
    let trapped_multiplier = scalar_from_f32(disc_cfg.trapped_multiplier);
    let grievance_gain = scalar_from_f32(disc_cfg.grievance_gain);
    let grievance_decay = scalar_from_f32(disc_cfg.grievance_decay);
    let index_of: HashMap<Entity, usize> = bands
        .iter()
        .enumerate()
        .map(|(i, b)| (b.entity, i))
        .collect();
    for (entity, mut cohort) in cohorts.iter_mut() {
        cohort.last_emigrated = emigrated.get(&entity).copied().unwrap_or(0);
        cohort.last_immigrated = immigrated.get(&entity).copied().unwrap_or(0);
        if let Some((dw, dc, de)) = deltas.get(&entity) {
            cohort.working = (cohort.working + *dw).max(scalar_zero());
            cohort.children = (cohort.children + *dc).max(scalar_zero());
            cohort.elders = (cohort.elders + *de).max(scalar_zero());
            cohort.sync_size();
        }
        if cohort.discontent_fraction <= scalar_zero() {
            cohort.grievance = (cohort.grievance - grievance_decay).max(scalar_zero());
        } else {
            // Trapped = wants to migrate (morale < threshold) but nowhere reachable to go.
            let trapped = cohort.morale < morale_threshold
                && index_of
                    .get(&entity)
                    .map(|&i| destination_of[i].is_none())
                    .unwrap_or(true);
            let mult = if trapped {
                trapped_multiplier
            } else {
                scalar_one()
            };
            let gain = grievance_gain * cohort.discontent_fraction * mult;
            cohort.grievance += gain;
        }
    }
}

/// The config handles [`advance_predator_raids`] reads, bundled into one `SystemParam` (the
/// [`LaborConfigs`] idiom) so the system stays within Bevy's argument budget without silencing clippy.
/// Each is resolved to its `Arc` once at the top of the system.
#[derive(bevy::ecs::system::SystemParam)]
pub struct RaidConfigs<'w> {
    pub fauna: Res<'w, FaunaConfigHandle>,
    pub combat: Res<'w, CombatConfigHandle>,
    pub creatures: Res<'w, CreaturesConfigHandle>,
}

/// **Predators Phase 1b — the raid trigger, and the Warrior role's first live consumer**
/// (`docs/plan_predators.md`). A carnivore with `aggression > 0` within `predators.raid_radius` of a
/// resident band turns on its camp; the band is defended by its **Warriors** (the head-count assigned
/// to [`LaborTarget::Warrior`]). Like the hunt-danger adapter, this builds a [`FightPayload`], resolves
/// it through the neutral combat subsystem, and applies **only the band/defender side's** casualties —
/// working-age only this phase (`wounded` is surfaced in the feed but mechanically inert, as in
/// Phase 0). Runs in the Population stage right after [`advance_labor_allocation`], so warrior counts
/// and band positions are current.
///
/// **Why the band side is TWO contingents, and why that is load-bearing** (do not "simplify" it into a
/// warriors-only side): the placeholder resolver clamps a side's losses to *its own* headcount, so a
/// side with `count 0` takes ZERO losses. A warriors-only band side would therefore give a
/// **0-warrior band zero casualties** — the exact inverse of "an under-guarded band costs it people".
/// So the band's *exposed populace* is present as its own contingent (the thing that can die, at
/// **zero attack** — it dilutes the blow and adds no offense), and the Warriors are the *additional
/// armed defenders* that add power (cutting the enemy-relative loss ratio) and shift the kill/wound
/// split toward wounded. The aggressor's engaged count is a **single** representative of the pack, a
/// deliberate Phase-1b simplification that keeps `power_enemy` modest (≈ `attack × aggression`) so a
/// handful of warriors at attack 1 can meaningfully reduce `(power_enemy / power_self)` — with the
/// whole pack engaged, warriors could never keep up and every raid would be a massacre. Scaling the
/// engaged count with pack size is a Phase-2+ refinement.
pub fn advance_predator_raids(
    herds: Res<HerdRegistry>,
    configs: RaidConfigs,
    sim_config: Res<SimulationConfig>,
    tick: Res<SimulationTick>,
    mut event_log: ResMut<CommandEventLog>,
    tiles: Query<&Tile>,
    mut bands: Query<(Entity, &mut PopulationCohort, &mut LaborAllocation), With<ResidentBand>>,
) {
    // Resolved once — none of these change within a turn (the hunt-danger adapter's discipline).
    let fauna = configs.fauna.get();
    let tuning = configs.combat.get().tuning();
    let person = configs.creatures.get().person();
    let raid_radius = fauna.predators.raid_radius;
    let raid_exposure = fauna.predators.raid_exposure;
    let raid_yield_forfeit_fraction = fauna.predators.raid_yield_forfeit_fraction;
    let width = sim_config.grid_size.x;
    let wrap = sim_config.map_topology.wrap_horizontal;
    let map_seed = sim_config.map_seed;
    let tick = tick.0;

    for (entity, mut cohort, mut alloc) in bands.iter_mut() {
        // Reset the per-turn raid forfeit up front — this system is its only writer, so a band that
        // is NOT raided this turn must read `0.0` rather than keep last turn's debit.
        alloc.last_raid_forfeit = 0.0;
        let Ok(band_pos) = tiles.get(cohort.current_tile).map(|t| t.position) else {
            continue;
        };
        // Working-age adults are both the defenders and the only bracket Phase-1b casualties come from,
        // so a band with none of them neither defends nor dies.
        let working_age = cohort.working.to_f32();
        if working_age <= 0.0 {
            continue;
        }
        let faction = cohort.faction;
        // Warriors can't exceed the working-age adults present; the rest of the exposed bracket is the
        // populace that stands in the pack's path (bounded by the `raid_exposure` dial).
        let warriors = alloc.workers_on(&LaborTarget::Warrior) as f32;
        let warrior_count = warriors.min(working_age);
        let exposed = raid_exposure.min((working_age - warrior_count).max(0.0));

        // Casualties from every raiding predator this turn are additive and order-independent, so they
        // accumulate into one cohort mutation at the end.
        let mut total_killed = 0.0f32;
        // Feed lines are DEFERRED: a casualty-causing raid also forfeits food (a band-level debit
        // computed after the loop), which is folded into the line's detail before it is pushed.
        let mut raid_lines: Vec<CommandEventEntry> = Vec::new();
        for herd in &herds.herds {
            // Only a **carnivore** raids — the diet gate.
            let Some(def) = fauna.species_by_display(&herd.species) else {
                continue;
            };
            if def.diet != Diet::Carnivore {
                continue;
            }
            // **The raid trigger** (`docs/plan_predators.md`): a carnivore raids to the extent it is
            // aggressive, so its effective attack is `attack × aggression`. A carnivore with
            // `aggression 0` does not raid at all — the gate.
            let effective_attack = def.combat.attack * def.aggression;
            if effective_attack <= 0.0 {
                continue;
            }
            // The pack must have reached the camp — a tighter reach than the prey-sensing disk.
            if crate::grid_utils::hex_distance_wrapped(herd.current_pos, band_pos, width, wrap)
                > raid_radius
            {
                continue;
            }

            // Rollback-stable seed distinct per (predator, band) pair — hash BOTH the herd id and the
            // band entity, so two predators on one band and one predator on two bands all differ. The
            // placeholder resolver ignores `seed`, but it is supplied as a real value (the hunt-danger
            // adapter's discipline).
            let mut hasher = crate::hashing::FnvHasher::new();
            std::hash::Hash::hash(&herd.id, &mut hasher);
            std::hash::Hash::hash(&entity, &mut hasher);
            let seed = map_seed ^ tick ^ std::hash::Hasher::finish(&hasher);

            let payload = FightPayload {
                sides: vec![
                    // Aggressor: a single fighting representative of the pack, at its
                    // aggression-scaled attack (defense/range unchanged).
                    Force {
                        id: ForceId(0),
                        posture: Posture::Aggressor,
                        contingents: vec![Contingent {
                            kind: ContingentId(herd.species.clone()),
                            count: 1.0,
                            profile: CombatStats {
                                attack: effective_attack,
                                ..def.combat
                            },
                        }],
                    },
                    // Defender: the band. TWO contingents (see the fn doc-comment) — the armed Warriors
                    // that add power, and the unarmed exposed folk that can die but add no offense.
                    Force {
                        id: ForceId(1),
                        posture: Posture::Defender,
                        contingents: vec![
                            Contingent {
                                kind: ContingentId::from("warrior"),
                                count: warrior_count,
                                profile: person,
                            },
                            Contingent {
                                kind: ContingentId::from("person"),
                                count: exposed,
                                profile: CombatStats {
                                    attack: 0.0,
                                    defense: person.defense,
                                    range: person.range,
                                    // Hunters do not break off — the party chose this fight, and
                                    // whether it holds is the resolver's business, not a per-hunter
                                    // flight roll. Dynamic troop morale is a later arc (§3).
                                    wariness: person.wariness,
                                },
                            },
                        ],
                    },
                ],
                terrain: vec![TerrainContext {
                    hex: (band_pos.x, band_pos.y),
                }],
                seed,
            };
            let outcome = resolve_fight(&payload, &tuning);
            // Apply ONLY the defender side (`ForceId(1)`); the predator side is discarded (no biomass
            // take here to reconcile, but band casualties are all this phase cares about).
            let (killed_f, wounded_f) =
                outcome.results.iter().fold((0.0f32, 0.0f32), |(k, w), r| {
                    if r.force == ForceId(1) {
                        (k + r.killed, w + r.wounded)
                    } else {
                        (k, w)
                    }
                });
            if killed_f + wounded_f > 0.0 {
                total_killed += killed_f;
                // One feed line per raiding predator, pushed now. Human text names the SPECIES, never
                // the internal herd id; the detail carries the fractional truth (`wounded` is inert this
                // phase — recovery is a later slice, as in Phase 0).
                let killed_r = killed_f.round() as u32;
                raid_lines.push(CommandEventEntry::new(
                    tick,
                    CommandEventKind::PredatorRaid,
                    faction,
                    format!("A {} raid cost {} lives", def.display_name, killed_r),
                    Some(format!(
                        "killed={:.3} wounded={:.3} warriors={} species={}",
                        killed_f, wounded_f, warrior_count as u32, def.display_name
                    )),
                ));
            }
        }
        // **Raids forfeit food** (Predators Phase 3): the band's people were defending or fleeing, not
        // gathering, so a **casualty-causing** raid also costs a fraction of THIS turn's food income.
        // `advance_labor_allocation` ran earlier this Population stage and already credited that income
        // to the larder, so the forfeit is a real `LocalStore::take` debit, capped at what remains. An
        // idle raided band (no income) loses only people. Recorded as the ACTUALLY-taken amount.
        if total_killed > 0.0 {
            let income: f32 = alloc.last_yields.iter().map(|y| y.actual).sum();
            let forfeit = raid_yield_forfeit_fraction * income;
            let taken = cohort.stores.take(FOOD, scalar_from_f32(forfeit)).to_f32();
            alloc.last_raid_forfeit = taken;
            // Fold the forfeit into the raid's feed detail (the wire's `raidForfeit` is the client's
            // authoritative number; this is the human/debug line).
            for line in &mut raid_lines {
                if let Some(detail) = line.detail.as_mut() {
                    detail.push_str(&format!(" forfeit={taken:.3}"));
                }
            }
        }
        for line in raid_lines {
            event_log.push(line);
        }
        // One mutation per band — working-age only this phase.
        cohort.apply_combat_casualties(scalar_from_f32(total_killed));
    }
}

#[cfg(test)]
mod labor_yield_tests {
    //! Retained per-source food-yield telemetry (`LaborAllocation.last_yields`): a depletable
    //! forage patch's `sustainable = sustainable_yield(pre-take biomass) ×
    //! provisions_per_biomass × output_multiplier` (MSY-based — regrowth at the most-productive
    //! biomass K/2, so a resource at carrying capacity still reads a positive sustainable harvest;
    //! a Sustain gather skims exactly that, so `actual ≈ sustainable`); a hunt's `sustainable` uses
    //! the same formula.
    //!
    //! **Slice 8 split the two webs here, deliberately.** A *gather* is still continuous, so the plant
    //! rows keep `actual ≈ sustainable` under Sustain. A *hunt* takes **whole animals**, so its
    //! `actual` pays in lumps around that rate instead of tracking it, and comparing the two per turn
    //! is no longer the overdraw question — `SourceYield::overdraws` answers it from the policy's own
    //! escapement floor. See `SourceYield`.
    use super::advance_labor_allocation;

    /// **The floor at which `intensification::learn_multiplier` is exactly ×1.0** — the food peak.
    /// Every accrual assertion below that is *not about the floor* passes it, so the call reads the
    /// rung's stated `progress_per_turn` rather than a floor's fraction of it.
    const FOOD_PEAK_FLOOR: f32 = crate::fauna::MSY_BIOMASS_FRACTION;

    use crate::components::{
        Improvement, LaborAllocation, LaborAssignment, LaborTarget, LocalStore, MoraleCause,
        PopulationCohort, SourceYield, Tile,
    };
    use crate::fauna::{
        forecast_expected_take, hunt_forecast, sustainable_yield, EcologyPhase, Herd, HerdRegistry,
        SourceYieldForecast, HERDING_DISCOVERY_ID, PENNING_DISCOVERY_ID,
    };
    use crate::fauna_config::{FaunaConfigHandle, SizeClass};
    use crate::flora_config::FloraConfig;
    use crate::food::{FoodModule, FoodModuleTag, FoodSiteKind};
    use crate::forage::patch_ecology;
    use crate::forage::{
        advance_forage_regrowth, forage_forecast, CULTIVATION_DISCOVERY_ID,
        SEED_SELECTION_DISCOVERY_ID,
    };
    use crate::forage::{ForagePatch, ForageRegistry};
    use crate::intensification::{
        LadderConfig, LadderConfigHandle, RungKey, RUNG_COMPLETE, RUNG_TIMESCALE_UNSCALED,
    };
    use crate::labor_config::LaborConfigHandle;
    use crate::orders::FactionId;
    use crate::resources::{
        CommandEventLog, DiscoveryProgressLedger, FactionInventory, SimulationConfig,
        SimulationTick, TileRegistry,
    };
    use crate::scalar::{scalar_from_f32, scalar_one, scalar_zero};
    use crate::systems::workers_needed_for_take;
    use crate::wellbeing_config::WellbeingConfigHandle;
    use crate::NO_IMPROVEMENT_UNDERWAY;
    use bevy::math::UVec2;
    use bevy::prelude::{Entity, World};
    use bevy_ecs::system::RunSystemOnce;
    use sim_runtime::TerrainType;

    const HERD_ID: &str = "game_test";
    const CAP: f32 = 100.0;
    /// One test animal (slice 8). Deliberately **big enough to bind**: at `CAP = 100` the Sustain
    /// escapement at full capacity is 50, so a 5-unit body quantises the take to at most 10 animals
    /// and a lightly-staffed crew genuinely rounds down. A `1.0` here would have made every take
    /// effectively continuous again and quietly stopped these forecast==actual sweeps from covering
    /// the quantiser at all.
    const TEST_GAME_BODY_MASS: f32 = 5.0;
    /// The faction every `spawn_band` band belongs to in this harness.
    const BAND_FACTION: FactionId = FactionId(0);
    /// Whole workers on each assignment: large enough that forage yields clearly and the hunt's
    /// per-worker biomass cap never binds (so a Sustain take is set by the regrowth ceiling).
    const WORKERS: u32 = 10;

    /// **Staff a build to the rung's full crew**, so an assertion about a rung's `progress_per_turn`
    /// reads its stated rate rather than an under-crewed fraction of it. A rung declaring no crew
    /// (both animal rungs) is unscaled, so [`WORKERS`] is as good as any number there.
    fn full_crew(rung: &crate::intensification::RungDef) -> u32 {
        rung.build_crew_needed().unwrap_or(WORKERS)
    }
    /// The biome under the harness's food-module tile — grassland, matching the
    /// `FoodModule::SavannaGrassland` tag it carries. A forage patch's carrying capacity is the
    /// **tile's** (`forage.capacity_by_biome`, the human food web's per-biome table), so the harness
    /// must name a biome rather than read a global constant.
    const SOURCE_BIOME: TerrainType = TerrainType::PrairieSteppe;

    /// A 3×1 world with a food-module tile + a stationary game herd (given `biomass`, cap `CAP`)
    /// both anchored on tile (0,0). Returns the world and that source tile's entity.
    fn world_with_source(biomass: f32) -> (World, Entity) {
        let mut world = World::default();
        let mut config = SimulationConfig::builtin();
        config.map_topology.wrap_horizontal = false;
        world.insert_resource(config);
        world.insert_resource(FaunaConfigHandle::default());
        world.insert_resource(LaborConfigHandle::default());
        world.insert_resource(crate::flora_config::FloraConfigHandle::default());
        world.insert_resource(LadderConfigHandle::default());
        world.insert_resource(WellbeingConfigHandle::default());
        world.insert_resource(crate::combat_config::CombatConfigHandle::default());
        world.insert_resource(crate::creatures_config::CreaturesConfigHandle::default());
        world.insert_resource(FactionInventory::default());
        world.insert_resource(DiscoveryProgressLedger::default());
        world.insert_resource(CommandEventLog::default());
        world.insert_resource(SimulationTick::default());

        let tiles: Vec<Entity> = (0..3)
            .map(|x| {
                world
                    .spawn(Tile {
                        position: UVec2::new(x, 0),
                        terrain: SOURCE_BIOME,
                        ..Default::default()
                    })
                    .id()
            })
            .collect();
        let source_tile = tiles[0];
        world.entity_mut(source_tile).insert(FoodModuleTag {
            module: FoodModule::SavannaGrassland,
            seasonal_weight: 1.0,
            kind: FoodSiteKind::SavannaTrack,
        });
        world.insert_resource(TileRegistry {
            tiles,
            width: 3,
            height: 1,
        });

        let fauna = world.resource::<FaunaConfigHandle>().get();
        let mut herd = Herd::new(
            HERD_ID.to_string(),
            "Test Game".to_string(),
            SizeClass::Small,
            vec![UVec2::new(0, 0)],
            biomass,
            CAP,
            0.0,
            fauna.ecology.regrowth_rate,
            TEST_GAME_BODY_MASS,
        );
        herd.refresh_ecology_phase(&fauna);
        drop(fauna);
        let mut registry = HerdRegistry::default();
        registry.herds.push(herd);
        world.insert_resource(registry);

        // Depletable forage patch on the source tile, seeded at the **post-regrowth steady state a
        // Sustain gather holds it at**: `K/2` (Sustain's escapement floor) plus the one turn of
        // regrowth Logistics adds before Population takes. These unit tests run
        // `advance_labor_allocation` alone, so the regrowth has to be in the fixture — seating the
        // patch *at* `K/2` would leave a Sustain gather nothing standing above its floor and every
        // row would read `0`.
        let forage_cfg = world.resource::<LaborConfigHandle>().get();
        let patch_cap = forage_cfg.forage.capacity_for(SOURCE_BIOME);
        let mut patch = ForagePatch::new(UVec2::new(0, 0), patch_cap);
        patch.biomass = patch_cap * crate::fauna::MSY_BIOMASS_FRACTION
            + sustainable_yield(
                patch_cap * crate::fauna::MSY_BIOMASS_FRACTION,
                patch_cap,
                &forage_cfg.forage.ecology,
            );
        patch.refresh_ecology_phase(&forage_cfg.forage.ecology);
        drop(forage_cfg);
        let mut forage_registry = ForageRegistry::default();
        forage_registry.patches.insert(UVec2::new(0, 0), patch);
        world.insert_resource(forage_registry);

        (world, source_tile)
    }

    /// A content band (morale 1 → output multiplier 1.0) on `tile` with the given assignments.
    fn spawn_band(world: &mut World, tile: Entity, assignments: Vec<LaborAssignment>) -> Entity {
        world
            .spawn((
                PopulationCohort {
                    home: tile,
                    current_tile: tile,
                    size: 30,
                    children: scalar_zero(),
                    working: scalar_from_f32(100.0),
                    elders: scalar_zero(),
                    stores: LocalStore::new(),
                    morale: scalar_one(),
                    last_food_consumption: 0.0,
                    last_morale_delta: scalar_zero(),
                    last_morale_cause: MoraleCause::None,
                    last_morale_contributions: Default::default(),
                    last_fertility_factors: Default::default(),
                    discontent_fraction: scalar_zero(),
                    grievance: scalar_zero(),
                    last_emigrated: 0,
                    last_immigrated: 0,
                    age_turns: 0,
                    generation: 0,
                    faction: FactionId(0),
                    knowledge: Vec::new(),
                    migration: None,
                },
                LaborAllocation {
                    assignments,
                    ..Default::default()
                },
            ))
            .id()
    }

    /// (a) both a Forage and a Hunt source capture `actual > 0`; (b) the hunt's `sustainable` equals
    /// the MSY-based `sustainable_yield` value at the pre-take biomass; (c) forage
    /// `sustainable ≡ actual`.
    ///
    /// **RETARGETED IN SLICE 8 on both the start state and the hunt assertion.** It used to start the
    /// herd at *exactly* `CAP * 0.5` ("half cap → clear positive regrowth") and assert the Sustain
    /// take skimmed exactly that regrowth. Both halves were flow-model artifacts:
    /// - `K/2` **is** the Sustain escapement point, so a herd standing there spares **nothing** — the
    ///   fixture was seeding the one biomass at which the hunt correctly takes `0` and then asserting
    ///   it took something. Started above the point, so the herd genuinely has animals to spare.
    /// - `actual ≈ sustainable` is no longer what Sustain means. The take is whole animals off the
    ///   escapement, so it pays in **lumps** around the long-run MSY rate rather than tracking it turn
    ///   by turn. `sustainable` is still asserted to be that honest rate — it is just no longer the
    ///   same question as "did this overdraw", which `overdraws` now answers directly.
    #[test]
    fn forage_and_sustain_hunt_capture_yields() {
        // Above the escapement point, so the herd has whole animals to spare this turn.
        let start = CAP * 0.9;
        let (mut world, tile) = world_with_source(start);
        let band = spawn_band(
            &mut world,
            tile,
            vec![
                LaborAssignment {
                    target: LaborTarget::Forage {
                        tile: UVec2::new(0, 0),
                        floor: 0.5,
                        species: None,
                    },
                    workers: WORKERS,
                    improvement: None,
                },
                LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: HERD_ID.to_string(),
                        floor: 0.5,
                    },
                    workers: WORKERS,
                    improvement: None,
                },
            ],
        );

        // Expected hunt sustainable = one turn's net regrowth at the PRE-take biomass, in provisions
        // (output multiplier is 1.0 at morale 1).
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let expected_sustainable =
            sustainable_yield(start, CAP, &fauna.ecology) * fauna.hunt.provisions_per_biomass;
        drop(fauna);

        // **No bank to seed — the fixture's own STOCK is what lands the animal.** This used to prime
        // `Herd::hunt_credit` to one body so turn one paid, back when the resident take was a banked
        // rate; the take is a stock now (`docs/plan_harvest_floor.md` §1), so the seeding was inert and
        // the comment described a mechanism the path no longer reads. The herd stands at `CAP * 0.9`,
        // which leaves 40 biomass above the food peak — eight whole bodies — so the take is a kill
        // turn by construction rather than by priming.
        world.run_system_once(advance_labor_allocation);

        let alloc = world.get::<LaborAllocation>(band).unwrap();
        assert_eq!(alloc.last_yields.len(), 2, "one yield row per assignment");
        let forage = alloc.last_yields[0].clone();
        let hunt = alloc.last_yields[1].clone();
        assert!(forage.actual > 0.0, "forage produced food: {forage:?}");
        assert!(hunt.actual > 0.0, "hunt produced food: {hunt:?}");
        // Depletable forage (§0-ii): a Sustain gather under the binding regrowth ceiling skims
        // exactly one turn's net regrowth, so `actual ≈ sustainable` (no over-forage flag).
        assert!(
            (forage.actual - forage.sustainable).abs() < 1e-4,
            "sustain forage skims the regrowth → actual ≈ sustainable: {} vs {}",
            forage.actual,
            forage.sustainable
        );
        assert!(
            forage.actual <= forage.sustainable + 1e-4,
            "a Sustain forage draw must not over-forage: {forage:?}"
        );
        assert!(
            (hunt.sustainable - expected_sustainable).abs() < 1e-6,
            "hunt sustainable = net regrowth × provisions_per_biomass: {} vs {}",
            hunt.sustainable,
            expected_sustainable
        );
        // A Sustain hunt is escapement to K/2: it is sustainable **by construction** (it cannot land
        // the herd below its most-productive biomass), whatever this turn's lump happens to be.
        assert!(
            !hunt.overdraws,
            "a Sustain hunt never overdraws — it stops at the MSY point: {hunt:?}"
        );
        assert!(
            !forage.overdraws,
            "a Sustain gather never overdraws: {forage:?}"
        );
    }

    /// An Eradicate hunt near carrying capacity overdraws the herd's meagre regrowth, so the captured
    /// telemetry reads `actual > sustainable` — the leading overhunting signal.
    #[test]
    fn overdraw_reads_actual_above_sustainable() {
        let start = CAP * 0.9; // near cap → small regrowth, so any real take overdraws.
        let (mut world, tile) = world_with_source(start);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.0,
                },
                workers: WORKERS,
                improvement: None,
            }],
        );
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let expected_sustainable =
            sustainable_yield(start, CAP, &fauna.ecology) * fauna.hunt.provisions_per_biomass;
        drop(fauna);

        world.run_system_once(advance_labor_allocation);

        let hunt = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();
        assert!(
            (hunt.sustainable - expected_sustainable).abs() < 1e-6,
            "sustainable pinned to the pre-take net regrowth"
        );
        assert!(
            hunt.actual > hunt.sustainable,
            "an Eradicate overdraw reads actual > sustainable: {} vs {}",
            hunt.actual,
            hunt.sustainable
        );
    }

    /// Regression (Phase 0 bug): a herd AT carrying capacity used to yield 0 under a Sustain hunt
    /// (logistic regrowth is 0 at K), leaving a full herd stuck. Constant escapement answers that
    /// case directly — a full herd is **all** surplus above `K/2` — so it stays huntable, and the
    /// harvest lands it exactly on its most productive biomass and never below.
    #[test]
    fn sustain_hunt_at_capacity_yields_its_surplus_and_stops_at_the_floor() {
        let start = CAP; // full herd — the old net_biomass_delta(K) == 0 bug.
        let (mut world, tile) = world_with_source(start);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.5,
                },
                workers: WORKERS,
                improvement: None,
            }],
        );
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let expected_sustainable =
            sustainable_yield(start, CAP, &fauna.ecology) * fauna.hunt.provisions_per_biomass;
        drop(fauna);

        world.run_system_once(advance_labor_allocation);

        let hunt = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();
        assert!(
            hunt.sustainable > 0.0,
            "a herd at carrying capacity must stay sustainably huntable: {hunt:?}"
        );
        assert!(
            (hunt.sustainable - expected_sustainable).abs() < 1e-6,
            "sustainable = MSY × provisions_per_biomass: {} vs {}",
            hunt.sustainable,
            expected_sustainable
        );
        // **The first harvest is the accumulated stock, and it is honestly larger than one turn's
        // regrowth** (`docs/plan_harvest_floor.md` §1). `sustainable` still reports the long-run MSY
        // line, so `actual > sustainable` here is not an overdraw and must not be read as one — the ⚠
        // is `overdraws`, a fact about the stance's FLOOR.
        assert!(
            hunt.actual > hunt.sustainable,
            "a full herd hands over its standing surplus, not a rate: {hunt:?}"
        );
        assert!(!hunt.overdraws, "Sustain never overdraws: {hunt:?}");

        // **And it stops dead on the floor.** No `advance_herds` here, so the herd never regrows:
        // every later turn takes exactly nothing, because nothing stands above `K/2`. That is the
        // whole of "Sustain cannot draw a herd below its most productive biomass".
        let floor = CAP * crate::fauna::MSY_BIOMASS_FRACTION;
        for _ in 0..8 {
            world.run_system_once(advance_labor_allocation);
        }
        let biomass = world
            .resource::<HerdRegistry>()
            .find(HERD_ID)
            .unwrap()
            .biomass;
        assert!(
            biomass >= floor - TEST_GAME_BODY_MASS && biomass < floor + TEST_GAME_BODY_MASS,
            "a Sustain-hunted herd settles ON its escapement floor ({floor}), within one body: \
             {biomass}"
        );
        let last = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();
        assert_eq!(
            last.actual, 0.0,
            "at the floor there is nothing standing above it to take: {last:?}"
        );
        assert!(!last.overdraws, "Sustain never overdraws: {last:?}");
    }

    use crate::components::FOOD;

    /// Set the source-tile forage patch cultivated (owned by faction 0) at the given biomass.
    fn cultivate_source_patch(world: &mut World, biomass: f32) {
        let forage = world.resource::<LaborConfigHandle>().get().forage.clone();
        let mut registry = world.resource_mut::<ForageRegistry>();
        let patch = registry.patches.get_mut(&UVec2::new(0, 0)).unwrap();
        patch.cultivation_progress = 1.0;
        patch.owner = Some(FactionId(0));
        patch.biomass = biomass;
        // The patch's OWN curve — a tended patch's phase bands ride `patch_ecology`, exactly as the
        // live regrowth pass resolves them.
        patch.refresh_ecology_phase(&patch_ecology(patch, &forage));
    }

    /// Switch a band's (single) Forage assignment to `policy` — what the client's picker does. (The
    /// *finishing* case needs no picker since issue #420: completion retires the build verb itself.)
    fn set_forage_floor(world: &mut World, band: Entity, floor: f32) {
        let mut allocation = world
            .get_mut::<LaborAllocation>(band)
            .expect("band forages");
        let assignment = allocation
            .assignments
            .iter_mut()
            .find(|assignment| matches!(assignment.target, LaborTarget::Forage { .. }))
            .expect("a Forage assignment");
        let LaborTarget::Forage { floor: current, .. } = &mut assignment.target else {
            unreachable!("filtered to Forage above");
        };
        *current = floor;
    }

    /// Stand the source patch up as a completed **Field** (rung 3) at `biomass` — the plant twin of
    /// `Herd::corral_at`, for the tests that need a sown fixture without paying the 25-turn build.
    fn sow_source_patch(world: &mut World, biomass: f32) {
        cultivate_source_patch(world, biomass);
        let forage = world.resource::<LaborConfigHandle>().get().forage.clone();
        let mut registry = world.resource_mut::<ForageRegistry>();
        let patch = registry.patches.get_mut(&UVec2::new(0, 0)).unwrap();
        patch.field_progress = RUNG_COMPLETE;
        patch.refresh_ecology_phase(&patch_ecology(patch, &forage));
    }

    /// Set the (wild, un-cultivated) source patch's biomass and refresh its ecology phase — for the
    /// `workers_needed` overstaffing tests, which need a full patch so the per-policy biomass-fraction
    /// ceiling binds rather than the seeded half-cap stock.
    fn set_wild_patch_biomass(world: &mut World, biomass: f32) {
        let forage = world.resource::<LaborConfigHandle>().get().forage.clone();
        let mut registry = world.resource_mut::<ForageRegistry>();
        let patch = registry.patches.get_mut(&UVec2::new(0, 0)).unwrap();
        patch.biomass = biomass;
        patch.refresh_ecology_phase(&patch_ecology(patch, &forage));
    }

    /// Run a single Forage assignment (given policy) with `WORKERS` on a full patch and return the
    /// captured `workers_needed` — the throughput to invert the per-policy take into a worker count.
    fn forage_workers_needed(floor: f32) -> u32 {
        let (mut world, tile) = world_with_source(CAP);
        let patch_cap = world
            .resource::<LaborConfigHandle>()
            .get()
            .forage
            .capacity_for(SOURCE_BIOME);
        set_wild_patch_biomass(&mut world, patch_cap);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: UVec2::new(0, 0),
                    floor,
                    species: None,
                },
                workers: WORKERS,
                improvement: None,
            }],
        );
        world.run_system_once(advance_labor_allocation);
        world.get::<LaborAllocation>(band).unwrap().last_yields[0].workers_needed
    }

    /// Overstaffing: a Sustain hunt whose take is set by the **escapement ceiling** — not labor —
    /// reports the crew that ceiling needs and no more, so `workers_needed < assigned` and the idle
    /// hands are visible.
    ///
    /// **The count is the crew that would clear the herd to its floor in one turn**
    /// (`docs/plan_harvest_floor.md` §7.6), which is bigger than the old MSY-rate count and is
    /// deliberately not clamped: it is what makes *"this crew cannot draw the herd that low"* a thing
    /// the readout can say.
    #[test]
    fn sustain_source_overstaffed_reports_fewer_workers_than_assigned() {
        // **Above the escapement point**: `K/2` is exactly where a Sustain hunt spares nothing, so the
        // old `CAP * 0.5` seeds the one biomass at which this test's premise cannot hold.
        let (mut world, tile) = world_with_source(CAP * 0.9);
        let assigned = 5;
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.5,
                },
                workers: assigned,
                improvement: None,
            }],
        );

        // The crew the escapement ceiling asks for, off the same helper the sim uses.
        let expected_crew = {
            let fauna = world.resource::<FaunaConfigHandle>().get();
            let labor = world.resource::<LaborConfigHandle>().get();
            let herd = world.resource::<HerdRegistry>().find(HERD_ID).unwrap();
            crate::fauna::hunt_haul_workers(
                crate::fauna::hunt_escapement_ceiling(
                    0.5,
                    herd.biomass,
                    crate::fauna::herd_capacity(herd, &fauna),
                ),
                herd.body_mass,
                labor.hunt.per_worker_biomass_capacity,
            )
        };

        world.run_system_once(advance_labor_allocation);

        let hunt = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();
        assert!(
            hunt.actual > 0.0,
            "the sustain hunt produced food: {hunt:?}"
        );
        assert_eq!(
            hunt.workers_needed, expected_crew,
            "the crew is the one the escapement ceiling needs: {hunt:?}"
        );
        assert!(
            hunt.workers_needed < assigned,
            "the source is overstaffed (extra workers idle): {hunt:?}"
        );
    }

    /// The other extreme: when worker throughput is the binding constraint (few workers, a high
    /// biomass-fraction Eradicate ceiling), every assigned worker was productive → `workers_needed ==
    /// assigned` (no overstaffing).
    #[test]
    fn labor_bound_take_reports_all_assigned_workers_needed() {
        let (mut world, tile) = world_with_source(CAP);
        let cfg = world.resource::<LaborConfigHandle>().get();
        let patch_cap = cfg.forage.capacity_for(SOURCE_BIOME);
        let capacity = cfg.forage.per_worker_biomass_capacity;
        drop(cfg);
        set_wild_patch_biomass(&mut world, patch_cap); // full patch.
        let assigned = 2;
        // The scenario is labor-bound iff worker throughput is below the stance's escapement ceiling.
        // Eradicate's floor is `0`, so on a full patch that ceiling is the whole standing crop.
        assert!(
            assigned as f32 * capacity < patch_cap,
            "test precondition: the take must be labor-bound, not ceiling-bound"
        );
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: UVec2::new(0, 0),
                    floor: 0.0,
                    species: None,
                },
                workers: assigned,
                improvement: None,
            }],
        );

        world.run_system_once(advance_labor_allocation);

        let forage = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();
        assert_eq!(
            forage.workers_needed, assigned,
            "a labor-bound take needs every assigned worker: {forage:?}"
        );
    }

    /// A deeper floor needs more workers on the **same** resource: Deplete/Eradicate leave less
    /// standing, so more of the crop is takeable and their inverted worker count exceeds Sustain's on
    /// identical full patches.
    #[test]
    fn deplete_and_eradicate_need_more_workers_than_sustain() {
        let sustain = forage_workers_needed(0.5);
        let deplete = forage_workers_needed(0.15);
        let eradicate = forage_workers_needed(0.0);
        assert!(
            deplete > sustain,
            "deplete's larger take needs more workers: {deplete} vs {sustain}"
        );
        assert!(
            eradicate > sustain,
            "eradicate's larger take needs more workers: {eradicate} vs {sustain}"
        );
        assert!(
            eradicate >= deplete,
            "eradicate's ceiling is ≥ deplete's: {eradicate} vs {deplete}"
        );
    }

    /// A tended (cultivated) patch and a corralled herd both pay out, and each reports an honest
    /// staffing need — **but they no longer report the same KIND of need**, and that is the point.
    ///
    /// The name's original claim (`workers_needed == 1` for both, "maintenance labor, not scaling
    /// gather") is dead twice over: slice 7 retired `TENDED_SOURCE_WORKERS_NEEDED = 1` for the payout,
    /// and slice 8 gave the pen a **standing, herd-sized herder demand**. What the pen reports now is
    /// [`source_crew_needed`] — **one crew sized by whichever of its two jobs binds**: enough hands to
    /// *mind* the heads (`ceil(animals / animals_per_herder)`) **and** to *haul* the meat
    /// (`ceil(take / per_worker_throughput)`). Herding is per head, hauling is per biomass, so neither
    /// term dominates across the roster — this fixture's pen happens to be **haul**-bound.
    #[test]
    fn tended_patch_and_corral_report_their_staffing_need() {
        let (mut world, tile) = world_with_source(CAP);
        let patch_cap = world
            .resource::<LaborConfigHandle>()
            .get()
            .forage
            .capacity_for(SOURCE_BIOME);
        cultivate_source_patch(&mut world, patch_cap);
        // Pen the herd in place (Rung 1c) so a Hunt assignment tends rather than hunts it.
        {
            let mut registry = world.resource_mut::<HerdRegistry>();
            assert!(
                registry.herds[0].corral_at(UVec2::new(0, 0)),
                "the fixture species must be pennable"
            );
        }

        let forager = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: UVec2::new(0, 0),
                    floor: 0.5,
                    species: None,
                },
                workers: WORKERS,
                improvement: None,
            }],
        );
        let keeper = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.5,
                },
                workers: WORKERS,
                improvement: None,
            }],
        );

        world.run_system_once(advance_labor_allocation);

        let tended = world.get::<LaborAllocation>(forager).unwrap().last_yields[0].clone();
        let corral = world.get::<LaborAllocation>(keeper).unwrap().last_yields[0].clone();
        assert!(
            tended.actual > 0.0 && corral.actual > 0.0,
            "both tended sources pay out: tended={tended:?} corral={corral:?}"
        );
        // A tended patch's staffing need is **derived** like every other rung's (slice 7): the
        // boosted curve (`tended_regrowth_gain`) can now pay out more biomass than a single forager
        // carries, so the honest count is `ceil(take / per-worker throughput)`, not a fixed `1`.
        // Asserted against the shared helper rather than a magic number, so it tracks a gain retune.
        let expected_foragers = {
            let world_labor = world.resource::<LaborConfigHandle>().get();
            let flora = world
                .resource::<crate::flora_config::FloraConfigHandle>()
                .get();
            let composition = source_tile_composition(&world);
            let patch = world.resource::<ForageRegistry>().patch(SOURCE).unwrap();
            // **The patch's OWN basket rate**, not the flat global one: a tended patch converts at
            // `patch_provisions_per_biomass`, so inverting `actual` through anything else measures a
            // different take than the sim staffed.
            let rate = crate::forage::patch_provisions_per_biomass(
                patch,
                &composition,
                &flora,
                &world_labor.forage,
            );
            let take_biomass = tended.actual / rate;
            let per_worker = crate::forage::forage_per_worker_biomass(&world_labor.forage, 1.0);
            (take_biomass / per_worker).ceil() as u32
        };
        assert!(
            expected_foragers >= 1,
            "the tended patch must pay out, or this asserts nothing"
        );
        assert_eq!(
            tended.workers_needed, expected_foragers,
            "a tended patch reports the crew its boosted take needs: {tended:?}"
        );
        // **The pen's staffing need is its whole CREW** (slice 8): `max(herders, haulers)`. Asserted
        // against the shared helpers rather than magic numbers, so it tracks a roster retune.
        let (herders, haulers) = {
            let world_fauna = world.resource::<FaunaConfigHandle>().get();
            let world_labor = world.resource::<LaborConfigHandle>().get();
            let registry = world.resource::<HerdRegistry>();
            let herders = crate::fauna::herd_herders_needed(&registry.herds[0], &world_fauna);
            let per_worker = crate::fauna::herd_hunt_yield(&registry.herds[0], &world_fauna)
                .apply(world_labor.hunt.per_worker_biomass_capacity, 1.0)
                .provisions;
            (herders, (corral.actual / per_worker).ceil() as u32)
        };
        assert!(
            herders >= 1,
            "the fixture pen must demand at least one keeper, or this asserts nothing"
        );
        assert_eq!(
            corral.workers_needed,
            herders.max(haulers),
            "the pen reports ONE crew sized by whichever job binds — minding {herders} head vs hauling \
             the take ({haulers}): {corral:?}"
        );
    }

    /// **A wild herd being TAMED reports its full would-be crew from turn one — no ownership lag**
    /// (taming-startup-lag fix). On the turn a `Tame` assignment starts, ownership is set only later in
    /// Population (`accrue_domestication`), so the ownership-gated `herd_herders_needed` reads `0` and the
    /// crew used to collapse to the tiny Tame-dip haul count — "1 of N working" on a full crew. An
    /// improvement in flight now sizes the herder term ownership-INDEPENDENTLY (`would_be_herders_needed`), so
    /// **both** the assign-time seed AND the resolved row report the real crew even while the herd is
    /// unowned; and an **extractive** policy still drops the herder term (a wild Sustain herd stays at the
    /// haul count, so it is never falsely flagged under-herded).
    #[test]
    fn a_wild_herd_being_tamed_reports_its_full_crew_without_the_ownership_lag() {
        let (mut world, tile) = world_with_source(CAP);
        // Reseat the wild fixture so its would-be herder crew clearly EXCEEDS the Tame-dip haul crew
        // (the rabbit-warren shape where the lag showed).
        let crew = {
            let fauna = world.resource::<FaunaConfigHandle>().get();
            let mut registry = world.resource_mut::<HerdRegistry>();
            let herd = &mut registry.herds[0];
            herd.body_mass = 1.0;
            herd.carrying_capacity = 200.0;
            herd.biomass = 200.0; // 200 animals ⇒ crew = ceil(200 / DEFAULT_ANIMALS_PER_HERDER 25) = 8
            herd.refresh_ecology_phase(&fauna);
            assert!(
                herd.owner.is_none() && !herd.is_corralled(),
                "the herd starts WILD (unowned, unpenned)"
            );
            crate::fauna::would_be_herders_needed(herd, &fauna)
        };
        assert!(
            crew >= 2,
            "the fixture crew must be non-trivial to observe the fix: {crew}"
        );

        // The expected haul crew (the value the OLD code collapsed to) + the ownership gate. **One
        // number, not two**: the haul crew is taken on the ceiling, and since the build dip moved
        // onto crew throughput (`docs/plan_harvest_floor.md` §3.1) a build no longer changes the
        // ceiling at all, so the Tame row and the pure-harvest row are sized on the same one.
        let (haul, gated) = {
            let fauna = world.resource::<FaunaConfigHandle>().get();
            let ladder = world.resource::<LadderConfigHandle>().get();
            let labor = world.resource::<LaborConfigHandle>().get();
            let registry = world.resource::<HerdRegistry>();
            let herd = &registry.herds[0];
            let forecast = crate::fauna::hunt_forecast(
                herd,
                &fauna,
                &ladder,
                labor.hunt.per_worker_biomass_capacity,
                1.0,
            );
            (
                crate::fauna::hunt_haul_workers(
                    forecast.ceiling_at(0.5).provisions,
                    forecast.body_mass_yield.provisions,
                    forecast.per_worker_yield.provisions,
                ),
                crate::fauna::herd_herders_needed(herd, &fauna),
            )
        };
        assert_eq!(
            gated, 0,
            "an unowned herd's ownership-gated herder count is 0 — the collapse this fix routes around"
        );
        assert!(
            crew > haul,
            "the crew must exceed the haul count, or the fix is invisible: crew {crew} vs haul \
             {haul}"
        );

        // Build the two seeds (a Tame build vs a pure harvest) on the still-WILD herd. Both hold the
        // same Sustain stance — the axis under test is the improvement.
        let seed = |improvement: Option<Improvement>, world: &World| {
            let fauna = world.resource::<FaunaConfigHandle>().get();
            let ladder = world.resource::<LadderConfigHandle>().get();
            let labor = world.resource::<LaborConfigHandle>().get();
            let registry = world.resource::<HerdRegistry>();
            crate::fauna::hunt_source_yield_preview(
                &registry.herds[0],
                &fauna,
                &ladder,
                labor.hunt.per_worker_biomass_capacity,
                1.0,
                crew,
                0.5,
                improvement,
                labor.yield_average_horizon_turns,
                labor.arrivals_horizon_turns,
            )
        };
        let tame_seed = seed(Some(Improvement::Tame), &world);
        assert_eq!(
            tame_seed.workers_needed, crew,
            "the assign-time seed reports the full would-be crew for a Tame source, not the haul: \
             {tame_seed:?}"
        );
        // The extractive contrast: Sustain on the same WILD herd drops the herder term → haul only.
        let sustain_seed = seed(None, &world);
        assert_eq!(
            sustain_seed.workers_needed, haul,
            "a pure harvest on a wild herd stays at the haul count (herder term 0): {sustain_seed:?}"
        );

        // The RESOLVED row: a real Tame keeper of `crew` hunters, one turn.
        let keeper = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.5,
                },
                workers: crew,
                improvement: Some(Improvement::Tame),
            }],
        );
        world.run_system_once(advance_labor_allocation);
        let resolved = world.get::<LaborAllocation>(keeper).unwrap().last_yields[0].clone();
        assert_eq!(
            resolved.workers_needed, crew,
            "the resolved row reports the same full crew — the herd was unowned when it was sized: \
             {resolved:?}"
        );
        assert_eq!(
            tame_seed.workers_needed, resolved.workers_needed,
            "seed == resolved (no jump between the pending assign and the turn it resolves)"
        );
    }

    /// **A patch being CULTIVATED reports its build crew from the assign-time seed, not the dipped
    /// take** — the plant twin of the taming-startup-lag test above, and the invariant that pins the
    /// seed and the resolved row against *each other*.
    ///
    /// The two halves computed `workers_needed` in two places and only one knew about the build crew:
    /// the resolved Forage arm floored on the rung's `crew_needed` while the assign-time seed
    /// (`forage::forage_source_yield_preview` → `fauna::forecast_source_yield`) inverted the take
    /// alone. A build is paid the **dip**, so that inversion lands *below* the crew: on a patch staffed
    /// to the rung's own crew, the compose sheet said *"max 2 workers useful here"* while the tile
    /// card beside it said *"only 1 of 2 working"* — **in the same frame**, on the same patch, both
    /// quoting the same (correct) yield. It self-healed the next turn, which is exactly why it
    /// survived: it was wrong only while the player was looking at it.
    ///
    /// So this asserts the *relation* rather than either number alone — a test that reads only the
    /// resolved turn cannot see this class of bug at all.
    #[test]
    fn a_patch_being_cultivated_seeds_the_same_build_crew_the_turn_resolves() {
        let (mut world, tile) = world_with_source(CAP);
        // The same committed-crop ground the other rung-2 payoff tests stand on, so the dipped take
        // is priced off a realization a crop is actually at home in (#433).
        world.resource_mut::<SimulationConfig>().map_seed = WORTH_TENDING_SEED;
        grant_knowledge(&mut world, CULTIVATION_DISCOVERY_ID);

        let crew = {
            let ladder = world.resource::<LadderConfigHandle>().get();
            ladder
                .rung(RungKey::PlantTended)
                .build_crew_needed()
                .expect("the plant tended rung declares a build crew")
        };
        assert!(
            crew >= 2,
            "the rung's crew must be non-trivial to observe the fix: {crew}"
        );

        // The seed the compose writes, for a build and for the pure gather beside it. Both hold the
        // same Sustain stance — the axis under test is the improvement.
        let composition = source_tile_composition(&world);
        let seed = |improvement: Option<Improvement>, world: &World| {
            let labor = world.resource::<LaborConfigHandle>().get();
            let flora = world
                .resource::<crate::flora_config::FloraConfigHandle>()
                .get();
            let ladder = world.resource::<LadderConfigHandle>().get();
            let registry = world.resource::<ForageRegistry>();
            crate::forage::forage_source_yield_preview(
                registry.patch(SOURCE).expect("the fixture seeded a patch"),
                &composition,
                &labor.forage,
                &flora,
                &ladder,
                SEASONAL_WEIGHT,
                NEUTRAL_OUTPUT_MULT,
                crew,
                SHALLOW_DRAW_FLOOR,
                improvement,
                labor.yield_average_horizon_turns,
                labor.arrivals_horizon_turns,
            )
        };
        let cultivate_seed = seed(Some(Improvement::Cultivate), &world);
        let gather_seed = seed(NO_IMPROVEMENT_UNDERWAY, &world);

        // **The take side of the seed, re-derived here** — the dipped take inverted by the crew's
        // **dipped** throughput, the exact arithmetic `forecast_source_yield`'s continuous branch
        // does. (It divided by the *undipped* rate until the §3.1 follow-up; that inversion reported
        // `workers × dip` hands working out of `workers` assigned, which is a different bug from the
        // one this test guards and had to be fixed before this margin meant anything.) It must come
        // in *below* the crew, or the floor is invisible and this test asserts nothing — which is
        // what [`SHALLOW_DRAW_FLOOR`] buys.
        let per_worker = {
            let labor = world.resource::<LaborConfigHandle>().get();
            let flora = world
                .resource::<crate::flora_config::FloraConfigHandle>()
                .get();
            let ladder = world.resource::<LadderConfigHandle>().get();
            let registry = world.resource::<ForageRegistry>();
            forage_forecast(
                registry.patch(SOURCE).expect("the fixture seeded a patch"),
                &composition,
                &labor.forage,
                &flora,
                &ladder,
                SEASONAL_WEIGHT,
                NEUTRAL_OUTPUT_MULT,
            )
            .per_worker_yield
            .provisions
        };
        let dipped_take_crew = workers_needed_for_take(
            cultivate_seed.actual,
            per_worker * build_dip(&world, Improvement::Cultivate),
            crew,
        );
        assert!(
            dipped_take_crew < crew,
            "the dipped take must invert below the build crew, or the floor is invisible: \
             take crew {dipped_take_crew} vs build crew {crew}"
        );
        assert_eq!(
            cultivate_seed.workers_needed, crew,
            "the assign-time seed reports the build's own crew, not the dipped take's: \
             {cultivate_seed:?}"
        );
        // The contrast: a pure gather on the same patch has no build, so it keeps the plain
        // overstaffing inversion — the floor can only ever *raise* a building source's count.
        assert_eq!(
            gather_seed.workers_needed,
            workers_needed_for_take(gather_seed.actual, per_worker, crew),
            "a pure gather is unfloored (no build, no standing crew): {gather_seed:?}"
        );

        // The RESOLVED row: a real Cultivate crew of `crew` foragers, one turn.
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    floor: SHALLOW_DRAW_FLOOR,
                    species: None,
                },
                workers: crew,
                improvement: Some(Improvement::Cultivate),
            }],
        );
        world.run_system_once(advance_labor_allocation);
        let resolved = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();
        assert_eq!(
            resolved.workers_needed, crew,
            "the resolved row reports the build crew too: {resolved:?}"
        );
        assert_eq!(
            cultivate_seed.workers_needed, resolved.workers_needed,
            "seed == resolved (the compose sheet and the tile card cannot disagree in one frame)"
        );
    }

    /// **A CULTIVATING crew that is fully employed reports every hand working** — the plant half of
    /// §3.1's dip-on-the-crew move, checked on the *resolved* row.
    ///
    /// The bug: `forage_take` caps the crew at `per_worker × seasonal × build_dip`, but the
    /// overstaffing inversion beside it divided the resulting take by the **undipped**
    /// `per_worker × seasonal`. The quotient is then literally `workers × dip` — at the shipped 0.50,
    /// **half the assigned crew** — so a labor-bound Cultivate reported *"only 4 of 8 working"* about
    /// eight hands that were every one of them gathering, in the same row as a positive `wasted` that
    /// says *add hands*. Acting on the advice halves the take and doubles the build.
    ///
    /// Asserted as the CONTRADICTION rather than as a bare number: `wasted > 0` (the patch offered
    /// more than the crew carried) and `workers_needed < workers` (drop hands) cannot both be true of
    /// one row. Before the fix they both were.
    #[test]
    fn a_labor_bound_cultivate_crew_is_not_reported_overstaffed() {
        let (mut world, tile) = world_with_source(CAP);
        world.resource_mut::<SimulationConfig>().map_seed = WORTH_TENDING_SEED;
        grant_knowledge(&mut world, CULTIVATION_DISCOVERY_ID);

        // **Staffed so the CREW binds, not the patch**: at the bare floor the whole standing stock is
        // offerable, and this crew's dipped throughput cannot carry all of it. The build crew floor
        // (2) is well below the head-count, so it cannot be what the assertion is reading.
        let workers = LABOR_BOUND_CULTIVATE_CREW;
        let (offered, carried) = {
            let labor = world.resource::<LaborConfigHandle>().get();
            let registry = world.resource::<ForageRegistry>();
            let patch = registry.patch(SOURCE).expect("the fixture seeded a patch");
            (
                patch.biomass,
                workers as f32
                    * crate::forage::forage_per_worker_biomass(&labor.forage, SEASONAL_WEIGHT)
                    * build_dip(&world, Improvement::Cultivate),
            )
        };
        assert!(
            carried < offered,
            "the crew must be the binding term, or there is no overstaffing claim to test: \
             carries {carried} of {offered} standing"
        );

        // **The ASSIGN-TIME seed says the same number** — `forecast_source_yield`'s *continuous*
        // branch, the plant twin of the animal branch's haul crew, and the half a compose sheet
        // shows before the turn resolves. Both halves inverted by the undipped rate, so they agreed
        // with each other and both disagreed with the take.
        let seed = {
            let labor = world.resource::<LaborConfigHandle>().get();
            let flora = world
                .resource::<crate::flora_config::FloraConfigHandle>()
                .get();
            let ladder = world.resource::<LadderConfigHandle>().get();
            let composition = source_tile_composition(&world);
            let registry = world.resource::<ForageRegistry>();
            crate::forage::forage_source_yield_preview(
                registry.patch(SOURCE).expect("the fixture seeded a patch"),
                &composition,
                &labor.forage,
                &flora,
                &ladder,
                SEASONAL_WEIGHT,
                NEUTRAL_OUTPUT_MULT,
                workers,
                crate::components::STRIP_IT_BARE,
                Some(Improvement::Cultivate),
                labor.yield_average_horizon_turns,
                labor.arrivals_horizon_turns,
            )
        };

        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    floor: crate::components::STRIP_IT_BARE,
                    species: None,
                },
                workers,
                improvement: Some(Improvement::Cultivate),
            }],
        );
        world.run_system_once(advance_labor_allocation);
        let row = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();

        assert!(
            row.wasted > 0.0,
            "the patch offered more than the crew carried, so the row says 'add hands': {row:?}"
        );
        assert_eq!(
            row.workers_needed, workers,
            "…and it must not ALSO say 'drop hands': every assigned forager was gathering, so the \
             count is the crew itself. Dividing the dipped take by the undipped rate reported \
             `workers × dip` here: {row:?}"
        );

        assert_eq!(
            seed.workers_needed, row.workers_needed,
            "seed == resolved (the compose sheet and the tile card cannot disagree in one frame): \
             {seed:?} vs {row:?}"
        );
    }

    /// Foragers on the harness patch at the bare floor: enough that the crew's **dipped** throughput
    /// falls short of the standing stock (patch `K` 70, seeded near `K/2` + one turn's regrowth ⇒ ~39
    /// standing; 8 foragers carry `8 × 8 × 0.50 = 32`), and comfortably above the `plant:tended`
    /// rung's crew of 2 so the build floor cannot be what a passing assertion is reading.
    const LABOR_BOUND_CULTIVATE_CREW: u32 = 8;

    /// **A crew GENTLING a herd needs MORE haulers than one hunting it** — the animal half of §3.1's
    /// dip-on-the-crew move.
    ///
    /// `hunt_haul_workers` answers *"how many hands carry home the peak drop this ceiling allows"*,
    /// and it exists so `workers_needed` and `wasted` can never contradict each other. The bug: the
    /// ceiling was passed undipped **and so was the per-hauler rate**, while the take the row
    /// describes was paid at `rate × build_dip`. So a Tame row quoted the *hunting* crew for a
    /// *gentling* take — a crew that provably cannot lift the drop — and disagreed with the client's
    /// own stepper cap (`SourceForecast.max_useful_workers`, which divides by `carry × dip`) by
    /// exactly the dip.
    ///
    /// The ceiling stays undipped, and that asymmetry is the whole point: the herd offers what stands
    /// above the floor whether the party is harvesting it or gentling it. Only the *carrying* is
    /// slower.
    #[test]
    fn a_herd_being_tamed_sizes_its_haul_crew_on_the_dipped_carry() {
        let dip = {
            let (world, _) = world_with_source(CAP);
            build_dip(&world, Improvement::Tame)
        };
        assert!(
            (0.0..1.0).contains(&dip),
            "the shipped Tame dip must be a real discount, or the two rows cannot differ: {dip}"
        );

        // One hunt turn on the slow breeder at a KILL biomass, with and without a Tame in flight.
        // Same herd, same floor, same crew — the improvement is the only axis.
        let row = |improvement: Option<Improvement>| {
            let (mut world, tile) = world_with_source(CAP);
            reseat_slow_breeder(&mut world, SLOW_BREEDER_KILL_BIOMASS);
            let band = spawn_band(
                &mut world,
                tile,
                vec![LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: HERD_ID.to_string(),
                        floor: crate::fauna::MSY_BIOMASS_FRACTION,
                    },
                    workers: WORKERS,
                    improvement,
                }],
            );
            world.run_system_once(advance_labor_allocation);
            world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone()
        };
        let taming = row(Some(Improvement::Tame));
        let hunting = row(NO_IMPROVEMENT_UNDERWAY);

        let per_worker = LaborConfigHandle::default()
            .get()
            .hunt
            .per_worker_biomass_capacity;
        let ceiling = crate::fauna::escapement_ceiling(
            crate::fauna::MSY_BIOMASS_FRACTION,
            SLOW_BREEDER_KILL_BIOMASS,
            SLOW_BREEDER_CAP,
        );
        assert_eq!(
            taming.workers_needed,
            crate::fauna::hunt_haul_workers(ceiling, SLOW_BREEDER_BODY, per_worker * dip),
            "a gentling crew is sized on its DIPPED carry against the same undipped ceiling: \
             {taming:?}"
        );
        assert!(
            taming.workers_needed > hunting.workers_needed,
            "…which is strictly more hands than the same herd wants from a pure hunt — reading the \
             two as equal is the bug: taming {} vs hunting {}",
            taming.workers_needed,
            hunting.workers_needed
        );
        // **The ASSIGN-TIME seed says the same number.** Both halves used to divide by the undipped
        // rate, so the seed and the resolved row agreed *with each other* while both disagreed with
        // the take — which is exactly why a seed==resolved test on its own cannot see this class of
        // bug, and why the assertion above had to come first.
        let seed = {
            let (mut world, _) = world_with_source(CAP);
            reseat_slow_breeder(&mut world, SLOW_BREEDER_KILL_BIOMASS);
            let fauna = world.resource::<FaunaConfigHandle>().get();
            let labor = world.resource::<LaborConfigHandle>().get();
            let ladder = world.resource::<LadderConfigHandle>().get();
            let registry = world.resource::<HerdRegistry>();
            crate::fauna::hunt_source_yield_preview(
                registry.find(HERD_ID).expect("the fixture seeded a herd"),
                &fauna,
                &ladder,
                labor.hunt.per_worker_biomass_capacity,
                NEUTRAL_OUTPUT_MULT,
                WORKERS,
                crate::fauna::MSY_BIOMASS_FRACTION,
                Some(Improvement::Tame),
                labor.yield_average_horizon_turns,
                labor.arrivals_horizon_turns,
            )
        };
        assert_eq!(
            seed.workers_needed, taming.workers_needed,
            "seed == resolved (the compose sheet and the band panel cannot disagree in one frame): \
             {seed:?} vs {taming:?}"
        );
        // The property the count exists to guarantee: at `workers_needed` the crew can actually lift
        // the biggest drop the ceiling allows (`floor(ceiling/body) + 1` whole bodies).
        let peak_biomass = ((ceiling / SLOW_BREEDER_BODY).floor() + 1.0) * SLOW_BREEDER_BODY;
        assert!(
            taming.workers_needed as f32 * per_worker * dip >= peak_biomass,
            "the reported crew must be able to haul the peak drop it was sized on: {} hands carry \
             {} of {peak_biomass}",
            taming.workers_needed,
            taming.workers_needed as f32 * per_worker * dip
        );
    }

    /// Reseat the harness herd as a **Wild-Aurochs-shaped slow breeder**: a `body_mass` heavier than one
    /// turn's regrowth at the operating point (`r·K/4 = 0.05·400/4 = 5 ≪ 80`), so it **pulses** — it
    /// spares zero animals on most turns while the stock above its floor rebuilds, then a whole one
    /// when that room clears a body. `biomass` is what picks the turn a test measures: below
    /// `K/2 + body` is a **wait**, at or above it a **kill**.
    fn reseat_slow_breeder(world: &mut World, biomass: f32) {
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let mut registry = world.resource_mut::<HerdRegistry>();
        let herd = &mut registry.herds[0];
        herd.body_mass = SLOW_BREEDER_BODY;
        herd.carrying_capacity = SLOW_BREEDER_CAP;
        herd.biomass = biomass;
        // These fixtures set biomass directly (no `regrow_biomass`); the rung payoff projections read
        // `biomass_before_regrowth` — keep it in sync.
        herd.biomass_before_regrowth = biomass;
        herd.refresh_ecology_phase(&fauna);
    }

    /// One aurochs-shaped body — heavier than one turn's regrowth, and heavier than one hauler carries.
    const SLOW_BREEDER_BODY: f32 = 80.0;
    /// The slow breeder's capacity: `MSY = r·K/4 = 5`, far below `SLOW_BREEDER_BODY`, and big enough
    /// that `K/2 + body` is a reachable biomass (so a **kill** turn is expressible at all).
    const SLOW_BREEDER_CAP: f32 = 400.0;
    /// Above the escapement point (`K/2 = 200`), but by **less than one body** — the WAIT turn: there
    /// is standing surplus, just not a whole animal of it.
    const SLOW_BREEDER_BIOMASS: f32 = 240.0;
    /// `K/2` plus more than one body — the KILL turn.
    const SLOW_BREEDER_KILL_BIOMASS: f32 = 300.0;

    /// A single Sustain-hunt turn on the slow breeder at `biomass` with `workers` assigned; returns
    /// the captured yield row.
    fn slow_breeder_hunt(biomass: f32, workers: u32) -> SourceYield {
        let (mut world, tile) = world_with_source(CAP);
        reseat_slow_breeder(&mut world, biomass);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.5,
                },
                workers,
                improvement: None,
            }],
        );
        world.run_system_once(advance_labor_allocation);
        world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone()
    }

    /// One hunt turn under `policy` on the slow breeder (biomass above `K/2`, empty bank), staffed so
    /// the worker cap never binds; returns the captured yield row.
    fn slow_breeder_hunt_at(floor: f32) -> SourceYield {
        let (mut world, tile) = world_with_source(CAP);
        reseat_slow_breeder(&mut world, SLOW_BREEDER_BIOMASS);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor,
                },
                workers: WORKERS,
                improvement: None,
            }],
        );
        world.run_system_once(advance_labor_allocation);
        world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone()
    }

    /// **The forward-projected `realized` reads the HONEST OVERHUNTING RATE — and sees the decline.**
    /// `sustainable` is the herd's MSY (the overhunting reference), policy-independent. The lumpy
    /// `actual` cannot be compared to it turn by turn (a kill lands a whole animal and spikes above
    /// MSY even under Sustain), which is why `overdraws` exists. The forward-projected `realized` IS
    /// comparable, and it is ordered by how deep the stance's floor is.
    ///
    /// **A Sustain projection sits ABOVE MSY, and that is honest, not an overdraw.** The window opens
    /// on a herd standing above `K/2`, so the first projected turn draws that accumulated surplus down
    /// to the floor and the rest of the horizon pays the regrowth — an average between the two. What
    /// makes Sustain sustainable is its floor, not its being under a line.
    ///
    /// **The decline is visible as `realized < actual` on Surplus**: the opening turn draws the stock
    /// down to `0.30·K` and the horizon that follows pays only the trickle back, so the steady
    /// headline lands well below the turn the player just watched. **Deplete cannot be read that way**
    /// — it leaves the herd *at* the Allee brink, where the projection terminates (nothing more to
    /// take), so its average is the strip rate over the turns it actually delivered, exactly like
    /// Eradicate's. That termination rule is what keeps it from being diluted toward zero.
    #[test]
    fn realized_reads_the_honest_overhunting_rate() {
        let sustain = slow_breeder_hunt_at(0.5);
        let surplus = slow_breeder_hunt_at(0.3);
        let deplete = slow_breeder_hunt_at(0.15);

        // `sustainable` is MSY, the same under every policy (it is the reference, not the take).
        assert!(
            (sustain.sustainable - surplus.sustainable).abs() < 1e-6
                && (sustain.sustainable - deplete.sustainable).abs() < 1e-6,
            "sustainable is the policy-independent MSY reference: {sustain:?} {surplus:?} {deplete:?}"
        );
        // Sustain projects at or above its sustainable MSY — the opening drawdown to `K/2` plus a
        // horizon of regrowth — and never below it: a Sustain hunt is not an under-draw either.
        assert!(
            sustain.realized >= sustain.sustainable - 1e-5,
            "a Sustain hunt projects at least its sustainable MSY: {sustain:?}"
        );
        assert!(
            sustain.realized > 0.0,
            "a Sustain hunt on a healthy herd projects a LIVE rate, not zero: {sustain:?}"
        );
        // Overhunting projects the honest rate ABOVE the sustainable reference, ordered by policy.
        assert!(
            surplus.realized > surplus.sustainable,
            "Surplus projects above the sustainable MSY (the honest overhunt rate): {surplus:?}"
        );
        assert!(
            deplete.realized > surplus.realized,
            "Deplete projects deeper than Surplus: {deplete:?} {surplus:?}"
        );
        // The projection SEES THE DECLINE on the stance that survives its own draw: Surplus takes the
        // standing surplus on turn one and then lives on the regrowth above `0.30·K`, so its horizon
        // average is far below the take the player just watched land. The instantaneous reading could
        // not produce that.
        assert!(
            surplus.realized > 0.0 && surplus.realized < surplus.actual,
            "Surplus projects well below its opening draw (sees the decline): {surplus:?}"
        );
        // Deplete leaves the herd at the Allee brink with nothing standing above it, so its projection
        // terminates and reports the strip it delivered rather than a horizon-diluted average.
        assert!(
            deplete.realized > 10.0 * deplete.sustainable,
            "Deplete reads the strip it delivered, not a diluted average: {deplete:?}"
        );
    }

    /// **Eradicate reads the STRIP RATE it delivers, NOT a diluted average.** Eradicate strips the herd
    /// in ~1 turn; the projection breaks the moment the source is spent and divides by the turns it
    /// actually delivered, so `realized` reads the high one-shot strip rate — far above Sustain's MSY —
    /// rather than that rate smeared thin across ~40 mostly-empty horizon turns (which would read
    /// *below* Sustain, the exact dilution the divide-by-turns-simulated rule prevents).
    #[test]
    fn eradicate_realized_reads_the_strip_rate_not_a_diluted_average() {
        let sustain = slow_breeder_hunt_at(0.5);
        let eradicate = slow_breeder_hunt_at(0.0);

        assert!(
            eradicate.realized > sustain.realized,
            "Eradicate strips faster than Sustain sustains: {eradicate:?} vs {sustain:?}"
        );
        // Not diluted toward zero: the one-turn strip of the whole standing stock dwarfs the
        // sustainable MSY. Diluting it over the full horizon would drop it to ~MSY/horizon, *below*
        // Sustain — so this margin is what proves the loop divided by the turns actually simulated.
        assert!(
            eradicate.realized > 10.0 * sustain.sustainable,
            "Eradicate reads its strip rate, not a horizon-diluted average: {eradicate:?} \
             (sustainable {})",
            sustain.sustainable
        );
    }

    /// **A hunt's `workers_needed` is its CEILING's carry crew — never the lumpy `0` of a wait turn.**
    /// The bug: sizing the crew off *this turn's* `take.carried` reads `0` on a slow breeder's wait turn
    /// (the room above the floor is lighter than one body, so nothing drops), collapsing
    /// `workers_needed` beside a `wasted_yield` that says the crew is understaffed — *drop workers* and
    /// *add workers* on one row. The ceiling-derived crew cannot flicker with the pulse, because it is
    /// taken on the same number `wasted_yield` is.
    #[test]
    fn a_slow_breeder_hunt_reports_its_carry_crew_on_a_wait_turn_never_zero() {
        let per_worker = LaborConfigHandle::default()
            .get()
            .hunt
            .per_worker_biomass_capacity;
        // The crew each turn's ceiling asks for, off the same helper the sim uses.
        let crew_for = |biomass: f32| {
            crate::fauna::hunt_haul_workers(
                crate::fauna::escapement_ceiling(
                    crate::fauna::MSY_BIOMASS_FRACTION,
                    biomass,
                    SLOW_BREEDER_CAP,
                ),
                SLOW_BREEDER_BODY,
                per_worker,
            )
        };
        let wait_crew = crew_for(SLOW_BREEDER_BIOMASS);
        assert!(
            wait_crew >= 2,
            "the fixture must need more than one hauler, or the wait-turn collapse is invisible"
        );

        // Wait turn: the room above the floor is under one body, so nothing drops — but the crew is
        // still the one the ceiling asks for, NOT the old `0`.
        let wait = slow_breeder_hunt(SLOW_BREEDER_BIOMASS, wait_crew);
        assert_eq!(
            wait.actual, 0.0,
            "a slow breeder waits while its room rebuilds: {wait:?}"
        );
        assert_eq!(
            wait.workers_needed, wait_crew,
            "the wait-turn crew is the ceiling's carry crew, not the lumpy 0: {wait:?}"
        );

        // Kill turn: the room clears a body, an animal lands, and the crew is still the ceiling's.
        let kill_crew = crew_for(SLOW_BREEDER_KILL_BIOMASS);
        let kill = slow_breeder_hunt(SLOW_BREEDER_KILL_BIOMASS, kill_crew);
        assert!(kill.actual > 0.0, "the whole animal lands: {kill:?}");
        assert_eq!(
            kill.workers_needed, kill_crew,
            "the kill-turn crew is the ceiling's carry crew too: {kill:?}"
        );
        assert_eq!(
            kill.wasted, 0.0,
            "a crew sized to the ceiling wastes nothing — the pairing `workers_needed`/`wasted` \
             must never disagree: {kill:?}"
        );

        // Overstaffed beyond that crew: the count is ceiling-derived (not clamped up to assigned), so
        // an extra hand is still flagged.
        let over = slow_breeder_hunt(SLOW_BREEDER_KILL_BIOMASS, kill_crew + 1);
        assert_eq!(
            over.workers_needed, kill_crew,
            "the crew is the ceiling's need, independent of overstaffing: {over:?}"
        );
        assert!(
            kill_crew + 1 > over.workers_needed,
            "a herd overstaffed beyond its crew still flags the idle hand: {over:?}"
        );
    }

    /// **A domesticated slow breeder reports `max(herders_needed, steady_haul)`, and it equals the
    /// client's `_max_useful_workers`.** The managed rung staffs one crew big enough for both jobs; the
    /// haul side is the steady carry crew (stable across the pulse), so the band panel's overstaff note
    /// and the compose panel's stepper cap read the same number — which is the whole point of the fix.
    #[test]
    fn a_domesticated_slow_breeder_reports_max_of_herders_and_steady_crew_matching_the_client() {
        let (mut world, tile) = world_with_source(CAP);
        reseat_slow_breeder(&mut world, SLOW_BREEDER_BIOMASS);
        // Tame it outright so it owes a standing herder cost (owner = the band's faction).
        {
            let mut registry = world.resource_mut::<HerdRegistry>();
            let herd = &mut registry.herds[0];
            herd.accrue_domestication(FactionId(0), 1.0);
            assert!(herd.is_domesticated(), "the fixture herd must be tamed");
        }
        let assigned = 3;
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.5,
                },
                workers: assigned,
                improvement: None,
            }],
        );
        // The sim's expectation: one crew, `max(herders, steady_haul)` — taken on the **pre-take**
        // herd, which is the state the labor arm sizes the crew against (an escapement ceiling falls
        // with the take that just drew it, so reading it afterwards would measure a different turn).
        let (herders, steady_haul, client_max_useful) = {
            let fauna = world.resource::<FaunaConfigHandle>().get();
            let labor = world.resource::<LaborConfigHandle>().get();
            let ladder = LadderConfig::builtin();
            let herd = world.resource::<HerdRegistry>().find(HERD_ID).unwrap();
            let herders = crate::fauna::herd_herders_needed(herd, &fauna);
            let ceiling_biomass = crate::fauna::hunt_escapement_ceiling(
                0.5,
                herd.biomass,
                crate::fauna::herd_capacity(herd, &fauna),
            );
            let steady_haul = crate::fauna::hunt_haul_workers(
                ceiling_biomass,
                herd.body_mass,
                labor.hunt.per_worker_biomass_capacity,
            );
            // The client's `_max_useful_workers`, in food-space off the same forecast the compose panel
            // reads: ceil((floor(ceiling / foodPerAnimal) + 1) × foodPerAnimal / perWorkerYield).
            let forecast = crate::fauna::hunt_forecast(
                herd,
                &fauna,
                &ladder,
                labor.hunt.per_worker_biomass_capacity,
                1.0,
            );
            let ceiling = forecast
                .ceiling_at(crate::fauna::MSY_BIOMASS_FRACTION)
                .provisions;
            let food_per_animal = forecast.body_mass_yield.provisions;
            let per_worker_yield = forecast.per_worker_yield.provisions;
            let client = ((((ceiling / food_per_animal).floor() + 1.0) * food_per_animal
                / per_worker_yield)
                .ceil()) as u32;
            (herders, steady_haul, client)
        };
        world.run_system_once(advance_labor_allocation);
        let yielded = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();

        assert!(
            herders >= 1,
            "a tamed herd owes at least one keeper, or this asserts nothing"
        );
        assert_eq!(
            yielded.workers_needed,
            herders.max(steady_haul),
            "a managed herd reports one crew = max(herders, steady haul): {yielded:?}"
        );
        assert_eq!(
            steady_haul, client_max_useful,
            "the sim's steady haul crew equals the client's max-useful count by construction"
        );
        assert!(
            assigned > yielded.workers_needed,
            "the 3-worker fixture is overstaffed past the steady crew: {yielded:?}"
        );
    }

    // --- Pre-commit yield forecast: forecast == actual (the client's "Expected yield") -------------
    //
    // The snapshot exposes a per-source forecast (`per_worker_yield` + the four policy ceilings) so
    // the client can show "Expected yield: +X.XX /turn" and cap its worker stepper BEFORE the player
    // commits. It only works if the forecast agrees with what the sim actually pays — these tests are
    // the guard: they run the REAL `advance_labor_allocation` and compare its payout against the
    // client's composition `min(workers × per_worker_yield, ceiling[policy])`.

    /// The tile coord `world_with_source` anchors its forage patch + herd on.
    const SOURCE: UVec2 = UVec2::new(0, 0);
    /// The `FoodModuleTag::seasonal_weight` `world_with_source` stamps on the source tile — the same
    /// weight the client reads for the tile and folds into its forecast.
    const SEASONAL_WEIGHT: f32 = 1.0;
    /// `spawn_band` bands sit at morale 1.0 → a neutral productivity multiplier, which is also the
    /// multiplier the snapshot captures forecasts at (`FORECAST_OUTPUT_MULTIPLIER`).
    const NEUTRAL_OUTPUT_MULT: f32 = 1.0;
    /// **A floor just ABOVE the food peak**, so the harness patch offers only a sliver of standing
    /// stock — less than one *dipped* worker's throughput. It exists so the build-crew floor stays
    /// load-bearing in `a_patch_being_cultivated_seeds_the_same_build_crew_the_turn_resolves`: at the
    /// peak itself this patch happens to offer almost exactly the rung's crew's dipped throughput, so
    /// the take side and the build crew agree and the assertion would hold for the wrong reason.
    const SHALLOW_DRAW_FLOOR: f32 = 0.55;

    /// The shipped `yield_fraction_while_building` for `improvement`, off the world's own ladder —
    /// the one seam the sim reads it through ([`LadderConfig::build_dip`]), so a test can never
    /// hard-code a fraction the config has since retuned.
    fn build_dip(world: &World, improvement: Improvement) -> f32 {
        world
            .resource::<LadderConfigHandle>()
            .get()
            .build_dip(Some(improvement))
    }
    /// f32 slack between the forecast (`workers × per_worker_yield`, provisions) and the sim's take
    /// (biomass → fixed-point provisions): different multiplication order + a 1e-6 fixed-point grid.
    /// Orders of magnitude below one provision.
    const FORECAST_EPSILON: f32 = 1e-4;
    /// Every improvement a **Forage** assignment may carry, plus the pure harvest. Swept **against
    /// every stance** since issue #442: the dip is a factor on the selected stance, so an
    /// (stance × improvement) grid is what forecast == actual now has to hold over — where the old
    /// single list could only ever check each dip against Sustain.
    const FORAGE_IMPROVEMENTS: [Option<Improvement>; 3] =
        [None, Some(Improvement::Cultivate), Some(Improvement::Sow)];
    /// The animal twin of [`FORAGE_IMPROVEMENTS`].
    const HUNT_IMPROVEMENTS: [Option<Improvement>; 3] =
        [None, Some(Improvement::Tame), Some(Improvement::Corral)];

    /// The client's composition: what it would display as the expected yield for this staffing. The
    /// shared helper — the *same* one the assign-time telemetry seed uses — so these tests pin the
    /// number the client shows, not a re-derivation of it.
    fn expected_yield(
        forecast: &SourceYieldForecast,
        workers: u32,
        floor: f32,
        improvement: Option<Improvement>,
    ) -> f32 {
        forecast_expected_take(forecast, workers, floor, improvement).provisions
    }

    /// The client's worker-stepper cap.
    fn max_useful_workers(forecast: &SourceYieldForecast, floor: f32) -> u32 {
        (forecast.ceiling_at(floor).provisions / forecast.per_worker_yield.provisions).ceil() as u32
    }

    /// Re-seat the test herd at `biomass`/`cap` (the harness's default 100-cap herd saturates every
    /// hunt policy ceiling with a single 40-biomass hunter, so a labor-bound hunt needs a bigger one).
    fn reseat_herd(world: &mut World, biomass: f32, cap: f32) {
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let mut registry = world.resource_mut::<HerdRegistry>();
        let herd = &mut registry.herds[0];
        herd.carrying_capacity = cap;
        herd.biomass = biomass;
        // Keep the pre-regrowth reading in sync (slice 8b): these tests set the biomass directly
        // without running `regrow_biomass`, and Sustain's rate reads `biomass_before_regrowth`.
        herd.biomass_before_regrowth = biomass;
        herd.refresh_ecology_phase(&fauna);
    }

    /// **The floors both forecast==actual sweeps walk.** The four the retired stance axis named
    /// (`0.50 / 0.30 / 0.15 / 0`), plus `0.80` and `1.0` — values the assignment can carry now and
    /// the stance axis could not express, `1.0` being the degenerate *"take nothing"* end where the
    /// room is exactly zero.
    const SWEPT_FLOORS: [f32; 6] = [0.0, 0.15, 0.3, 0.5, 0.8, 1.0];

    /// **Forage forecast == actual, at every FLOOR.** For every floor × staffing (labor-bound,
    /// ceiling-bound), the client's `min(workers × per_worker_yield, ceiling_at(floor))` equals the
    /// provisions `advance_labor_allocation` actually pays. Both binding regimes are asserted to
    /// have been exercised, so this can't silently degenerate into testing one branch.
    ///
    /// **Swept over floors rather than over four stances** (`docs/plan_harvest_floor.md` §5): the
    /// assignment carries a continuous number now, so a sweep of four fixed values would only pin
    /// the four the retired axis happened to name.
    #[test]
    fn forage_forecast_equals_actual_take_for_every_floor_and_staffing() {
        let mut saw_labor_bound = false;
        let mut saw_ceiling_bound = false;
        for policy in SWEPT_FLOORS {
            for improvement in FORAGE_IMPROVEMENTS {
                for workers in [1u32, 2, 20] {
                    let (mut world, tile) = world_with_source(CAP);
                    // Forecast off the PRE-turn patch state, exactly as the client reads it from the
                    // snapshot captured at the end of last turn.
                    let patch = world
                        .resource::<ForageRegistry>()
                        .patch(SOURCE)
                        .cloned()
                        .expect("seeded patch");
                    let composition = source_tile_composition(&world);
                    let labor = world.resource::<LaborConfigHandle>().get();
                    let forecast = forage_forecast(
                        &patch,
                        &composition,
                        &labor.forage,
                        &FloraConfig::builtin(),
                        &LadderConfig::builtin(),
                        SEASONAL_WEIGHT,
                        NEUTRAL_OUTPUT_MULT,
                    );
                    drop(labor);

                    let band = spawn_band(
                        &mut world,
                        tile,
                        vec![LaborAssignment {
                            target: LaborTarget::Forage {
                                tile: SOURCE,
                                floor: policy,
                                species: None,
                            },
                            workers,
                            improvement,
                        }],
                    );
                    world.run_system_once(advance_labor_allocation);
                    let actual = world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual;

                    let labor_term = workers as f32 * forecast.per_worker_yield.provisions;
                    let ceiling = forecast.ceiling_at(policy).provisions;
                    if labor_term < ceiling {
                        saw_labor_bound = true;
                    } else {
                        saw_ceiling_bound = true;
                    }
                    let expected = expected_yield(&forecast, workers, policy, improvement);
                    assert!(
                    (actual - expected).abs() < FORECAST_EPSILON,
                    "forage forecast must equal the actual take (floor {policy} + {improvement:?}, \
                     {workers} workers): forecast={expected} actual={actual} ({forecast:?})"
                );
                }
            }
        }
        assert!(
            saw_labor_bound && saw_ceiling_bound,
            "both regimes must be covered: labor-bound={saw_labor_bound} ceiling-bound={saw_ceiling_bound}"
        );
    }

    /// **Hunt forecast == actual, on a fresh (empty-bank) herd.** The fauna twin of the forage test.
    /// The herd is re-seated at a large capacity so the Eradicate ceiling exceeds a single hunter's
    /// throughput (a labor-bound case); 20 hunters overstaff every policy (the ceiling binds).
    ///
    /// **The forecast IS the take**, helper for helper: both are
    /// `min(crew throughput, hunt_escapement_ceiling(...))` quantised to whole animals, so the
    /// invariant holds turn by turn rather than in the long run. The old caveat — that it held only on
    /// an empty kill-credit bank, because the readout was a steady rate while the take cashed a
    /// banked burst — died with the bank (`Herd::hunt_credit`).
    ///
    /// **It sweeps TWO stock levels**: a full herd and [`DRAWN_DOWN_BIOMASS`], a remnant standing
    /// barely above the deepest floors, where a whole-animal take is a large fraction of what is left.
    /// The `stock_cap` clamp is asserted **inert** throughout — an escapement ceiling is `B − floor·K`
    /// and so cannot exceed `B` — which is the property that retired the dip-versus-clamp ordering
    /// question rather than an assumption made about it.
    #[test]
    fn hunt_forecast_equals_actual_take_for_every_floor_and_staffing() {
        let mut saw_labor_bound = false;
        let mut saw_ceiling_bound = false;
        for biomass in [BIG_HERD_CAP, DRAWN_DOWN_BIOMASS] {
            for policy in SWEPT_FLOORS {
                for improvement in HUNT_IMPROVEMENTS {
                    for workers in [1u32, 2, 20] {
                        let (mut world, tile) = world_with_source(CAP);
                        reseat_herd(&mut world, biomass, BIG_HERD_CAP);
                        let herd = world
                            .resource::<HerdRegistry>()
                            .find(HERD_ID)
                            .cloned()
                            .expect("seeded herd");
                        assert_eq!(
                            herd.hunt_credit, 0.0,
                            "the resident take path must not read or write the expedition's bank"
                        );
                        let fauna = world.resource::<FaunaConfigHandle>().get();
                        let per_worker = world
                            .resource::<LaborConfigHandle>()
                            .get()
                            .hunt
                            .per_worker_biomass_capacity;
                        let forecast = hunt_forecast(
                            &herd,
                            &fauna,
                            &LadderConfig::builtin(),
                            per_worker,
                            NEUTRAL_OUTPUT_MULT,
                        );
                        drop(fauna);

                        let band = spawn_band(
                            &mut world,
                            tile,
                            vec![LaborAssignment {
                                target: LaborTarget::Hunt {
                                    fauna_id: HERD_ID.to_string(),
                                    floor: policy,
                                },
                                workers,
                                improvement,
                            }],
                        );
                        world.run_system_once(advance_labor_allocation);
                        let actual =
                            world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual;

                        // **The build dip is on the LABOR term, not the ceiling**
                        // (`docs/plan_harvest_floor.md` §3.1) — so which side binds is itself a
                        // function of the improvement, and both regimes have to be reached with the
                        // dip in place or the sweep would only ever exercise the undipped half.
                        let dip = forecast.build_dips.of(improvement);
                        let labor_term =
                            workers as f32 * forecast.per_worker_yield.provisions * dip;
                        let ceiling = forecast.ceiling_at(policy).provisions;
                        if labor_term < ceiling {
                            saw_labor_bound = true;
                        } else {
                            saw_ceiling_bound = true;
                        }
                        let expected = expected_yield(&forecast, workers, policy, improvement);
                        assert!(
                            (actual - expected).abs() < FORECAST_EPSILON,
                            "hunt forecast must equal the actual take (B={biomass}, floor {policy} + \
                             {improvement:?}, {workers} workers): forecast={expected} \
                             actual={actual} ({forecast:?})"
                        );
                    }
                }
            }
        }
        assert!(
            saw_labor_bound && saw_ceiling_bound,
            "both regimes must be covered: labor-bound={saw_labor_bound} ceiling-bound={saw_ceiling_bound}"
        );
    }

    /// Carrying capacity the hunt forecast sweep re-seats its herd at: large enough that the
    /// Eradicate ceiling exceeds a single hunter's throughput (a labor-bound case), while 20 hunters
    /// overstaff every policy (the ceiling binds).
    const BIG_HERD_CAP: f32 = 1_000.0;

    /// **A remnant herd, standing barely above the deepest escapement floors.** With `K = 1000` it is
    /// under Sustain's `K/2` and under Surplus's `0.30·K` (so those rows honestly offer nothing),
    /// a hair above Deplete's `0.15·K`, and — with `TEST_GAME_BODY_MASS = 5.0` — its Eradicate room
    /// is a handful of whole animals rather than a smooth fraction. That is the regime where a
    /// forecast is easiest to get wrong: near-empty rows, quantisation biting hard, and the standing
    /// stock within a rounding error of the ceiling.
    const DRAWN_DOWN_BIOMASS: f32 = 155.0;

    /// **The rung-3 shape: the POLICY axis collapses, the WORKER cap does not** (slice 7). A **Field**
    /// and a **pen** are yours — you control their reproduction, so no policy takes more or less than
    /// the managed yield. But you still have to carry the harvest home, so `per_worker_yield` is the
    /// crew's real throughput and `max_useful_workers` is the honest `ceil(production / per_worker)`.
    ///
    /// **Retargeted, not weakened.** This test used to be
    /// `tended_patch_and_corral_forecast_full_yield_with_one_worker` and asserted
    /// `max_useful_workers == 1` for every policy — pinning the two defects this slice fixes: the
    /// forecast encoded "one worker collects everything the land offers", and it covered *tended*
    /// patches, which are rung **2** and never belonged in the managed shape at all. Both claims are
    /// now inverted deliberately: the worker count must exceed 1 on a source this rich, and the
    /// fixture is a **Field**. The rung-2 half moved to
    /// `a_tended_patch_is_policy_live_worker_capped_and_can_be_over_farmed`.
    #[test]
    fn a_field_and_a_pen_collapse_the_policy_axis_but_still_need_carrying_home() {
        let (mut world, tile) = world_with_source(CAP);
        let patch_cap = world
            .resource::<LaborConfigHandle>()
            .get()
            .forage
            .capacity_for(SOURCE_BIOME);
        sow_source_patch(&mut world, patch_cap);
        {
            let mut registry = world.resource_mut::<HerdRegistry>();
            assert!(
                registry.herds[0].corral_at(SOURCE),
                "the fixture species must be pennable"
            );
        }

        let patch = world
            .resource::<ForageRegistry>()
            .patch(SOURCE)
            .cloned()
            .expect("seeded patch");
        let composition = source_tile_composition(&world);
        let labor = world.resource::<LaborConfigHandle>().get();
        let patch_forecast = forage_forecast(
            &patch,
            &composition,
            &labor.forage,
            &FloraConfig::builtin(),
            &LadderConfig::builtin(),
            SEASONAL_WEIGHT,
            NEUTRAL_OUTPUT_MULT,
        );
        let hunt_per_worker = labor.hunt.per_worker_biomass_capacity;
        drop(labor);
        let herd = world
            .resource::<HerdRegistry>()
            .find(HERD_ID)
            .cloned()
            .expect("seeded herd");
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let herd_forecast = hunt_forecast(
            &herd,
            &fauna,
            &LadderConfig::builtin(),
            hunt_per_worker,
            NEUTRAL_OUTPUT_MULT,
        );
        drop(fauna);

        // **The floor axis is collapsed**: every floor — and every improvement, since the build is
        // already done — quotes the one managed yield.
        for policy in SWEPT_FLOORS {
            for improvement in FORAGE_IMPROVEMENTS {
                assert_eq!(
                    patch_forecast.ceiling_at(policy).provisions,
                    patch_forecast.managed_yield.provisions,
                    "a Field is yours — no floor takes more or less of it, and there is nothing \
                     left to build on it (floor {policy} + {improvement:?})"
                );
            }
        }
        for policy in SWEPT_FLOORS {
            for improvement in HUNT_IMPROVEMENTS {
                assert_eq!(
                    herd_forecast.ceiling_at(policy).provisions,
                    herd_forecast.managed_yield.provisions,
                    "a pen is yours — no stance takes more or less of it ({policy:?} + \
                     {improvement:?})"
                );
            }
        }

        // **The worker cap is NOT collapsed.** `per_worker_yield` is the crew's real throughput, so
        // this Field genuinely needs more than one pair of hands — the readout the old hardcoded `1`
        // made permanently false.
        let field_workers_needed = max_useful_workers(&patch_forecast, 0.5);
        assert!(
            field_workers_needed > 1,
            "a Field at capacity offers more than one worker can carry: {field_workers_needed}"
        );
        for policy in SWEPT_FLOORS {
            assert_eq!(
                max_useful_workers(&patch_forecast, policy),
                field_workers_needed
            );
        }

        // Staffed to exactly that count, the crew collects the whole production — and that IS what
        // the sim pays. Understaffed by one, it collects strictly less: the cap really binds.
        let field_band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    floor: 0.5,
                    species: None,
                },
                workers: field_workers_needed,
                improvement: None,
            }],
        );
        let short_handed = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.5,
                },
                workers: 1,
                improvement: None,
            }],
        );
        world.run_system_once(advance_labor_allocation);

        let field_row = world
            .get::<LaborAllocation>(field_band)
            .unwrap()
            .last_yields[0]
            .clone();
        let field_forecast = expected_yield(&patch_forecast, field_workers_needed, 0.5, None);
        assert!(field_forecast > 0.0);
        assert!(
            (field_row.actual - field_forecast).abs() < FORECAST_EPSILON,
            "Field forecast must equal the actual payout: {field_forecast} vs {}",
            field_row.actual
        );
        assert!(
            (field_row.actual - patch_forecast.managed_yield.provisions).abs() < FORECAST_EPSILON,
            "a fully-staffed Field collects everything it produces"
        );
        assert!(
            field_row.wasted < FORECAST_EPSILON,
            "a fully-staffed Field wastes nothing: {}",
            field_row.wasted
        );

        let pen_row = world
            .get::<LaborAllocation>(short_handed)
            .unwrap()
            .last_yields[0]
            .clone();
        let pen_forecast = expected_yield(&herd_forecast, 1, 0.5, None);
        assert!(pen_forecast > 0.0);
        assert!(
            (pen_row.actual - pen_forecast).abs() < FORECAST_EPSILON,
            "pen forecast must equal the actual payout: {pen_forecast} vs {}",
            pen_row.actual
        );
    }

    /// **Rung 2 is a WILD stand, and since Flora Roster S2 it is a NEUTRAL one** — the plant twin of a
    /// *pastoral* herd, but no longer on a boosted curve. A *bare* (uncommitted) tended patch is
    /// Sustain-gathered at **exactly wild MSY** (`wild MSY × tended_regrowth_gain`, and the gain is now
    /// `1.0`): it regrows and yields exactly as fast as the same patch wild. It still **draws down**
    /// like any wild stand and is marked tended-this-turn — this test pins that neutrality plus those
    /// rung mechanics (it draws down, marks the patch worked, and its Sustain take is honestly
    /// sustainable).
    ///
    /// **The intensification incentive moved to the committed crop.** It was once a flat managed rate (no
    /// draw-down), then a boosted MSY curve; S2 retired the boost because, with S1 making
    /// competitor-removal explicit as a *composition* term, a growth boost double-counted it. So
    /// "tended beats wild" now lives entirely in a committed crop — **weeding + conversion** (§4.3) — and
    /// is pinned by the roster's own bar (`core_sim/tests/flora_roster.rs`) and `flora_commitment.rs`,
    /// which see the crop this scale-free rung mechanic cannot.
    #[test]
    fn a_bare_tended_patch_is_neutral_versus_wild_and_draws_down() {
        let (mut world, tile) = world_with_source(CAP);
        let cfg = world.resource::<LaborConfigHandle>().get();
        let forage = cfg.forage.clone();
        let patch_cap = forage.capacity_for(SOURCE_BIOME);
        let biomass = patch_cap;
        drop(cfg);
        // **The wild rate is this tile's own basket average** (#433), not the flat
        // `provisions_per_biomass` — the point of "bare tended is neutral" is that a patch with no
        // crop committed reads exactly what the same ground reads wild, whatever that ground grows.
        let composition = source_tile_composition(&world);
        let wild_rate = {
            let cfg = world.resource::<LaborConfigHandle>().get();
            let flora = world
                .resource::<crate::flora_config::FloraConfigHandle>()
                .get();
            let wild = ForagePatch::new(SOURCE, patch_cap);
            crate::forage::patch_provisions_per_biomass(&wild, &composition, &flora, &cfg.forage)
        };
        // The **wild counterfactual take**: the stock standing above Sustain's escapement floor,
        // capped by the crew. It is deliberately computed off the wild patch's numbers — the whole
        // claim is that a bare tended patch pays exactly this.
        let wild_take = {
            let cfg = world.resource::<LaborConfigHandle>().get();
            let crew = WORKERS as f32 * crate::forage::forage_per_worker_biomass(&cfg.forage, 1.0);
            crew.min(biomass - patch_cap * crate::fauna::MSY_BIOMASS_FRACTION) * wild_rate
        };
        cultivate_source_patch(&mut world, biomass);

        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: UVec2::new(0, 0),
                    floor: 0.5,
                    species: None,
                },
                workers: WORKERS,
                improvement: None,
            }],
        );

        world.run_system_once(advance_labor_allocation);

        // A bare tended patch reads exactly what the same ground reads wild. Under constant
        // escapement that is now true for **two independent reasons** and the test pins both: the
        // ceiling is `r`-free (so the rung's boosted curve cannot enter it at all), and with no crop
        // committed the conversion rate is the wild basket's.
        let expected = wild_take;
        let paid = world
            .get::<PopulationCohort>(band)
            .unwrap()
            .stores
            .get(FOOD)
            .to_f32();
        assert!(
            (paid - expected).abs() < 1e-3,
            "bare tended band gathers the wild escapement surplus: {paid} vs {expected}"
        );
        // **It draws down** — the correction. A tended patch is a wild stand, so gathering it takes
        // biomass out of it, which is what makes over-farming it possible at all.
        let patch = world
            .resource::<ForageRegistry>()
            .patch(UVec2::new(0, 0))
            .unwrap();
        assert!(
            patch.biomass < biomass,
            "a tended patch is still gathered from a real stock: {} vs {biomass}",
            patch.biomass
        );
        assert!(patch.tended_this_turn, "tending marks the patch worked");
        // Telemetry: `sustainable` is a *measured* MSY line, and a Sustain take is sustainable by
        // its FLOOR (`overdraws`), not by being under that line — the first harvest of a full patch
        // is its accumulated stock and legitimately exceeds one turn's regrowth.
        let row = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();
        assert!((row.actual - expected).abs() < 1e-3);
        assert!(!row.overdraws, "a Sustain gather never overdraws: {row:?}");
    }

    /// **The playtest bug, pinned: every policy on a completed Tended Patch forecast the identical
    /// number.** Rung 2 reads the policy axis again — four policies, four different takes, ordered as
    /// their design intends — and Surplus really does over-farm the patch, so the overdraw ⚠ can
    /// finally fire on the plant web's rung 2. Before slice 7 the managed branch recorded
    /// `sustainable == actual` by construction, so `actual > sustainable` was unreachable here.
    ///
    /// Measured on a **drawn-down** patch (a patch being farmed is below capacity), deliberately.
    /// **Since Flora Roster S2 the gain is neutral (`1.0`)**, so a tended patch reads the same curve as
    /// a wild one and the policies fall in their natural order: Sustain (MSY) < Surplus (`1.6 × MSY`) <
    /// Deplete (20% of biomass) < Eradicate (30%). (At the retired gain 2.0 the boosted Surplus rode
    /// past the flat Deplete skim; that swap is gone with the boost.)
    #[test]
    fn a_tended_patch_is_policy_live_worker_capped_and_can_be_over_farmed() {
        let extractive = [0.5, 0.3, 0.15, 0.0];
        // A real operating point: a patch under active harvest sits below its cap (still above K/2, so
        // Sustain reads the MSY plateau). Full-cap would land Surplus exactly on Deplete (see docstring).
        const OPERATING_FRACTION: f32 = 0.8;
        let mut takes: Vec<(f32, f32)> = Vec::new();
        for policy in extractive {
            let (mut world, tile) = world_with_source(CAP);
            let patch_cap = world
                .resource::<LaborConfigHandle>()
                .get()
                .forage
                .capacity_for(SOURCE_BIOME);
            cultivate_source_patch(&mut world, patch_cap * OPERATING_FRACTION);
            let band = spawn_band(
                &mut world,
                tile,
                vec![LaborAssignment {
                    target: LaborTarget::Forage {
                        tile: SOURCE,
                        floor: policy,
                        species: None,
                    },
                    workers: WORKERS,
                    improvement: None,
                }],
            );
            world.run_system_once(advance_labor_allocation);
            let row = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();
            let patch = world.resource::<ForageRegistry>().patch(SOURCE).unwrap();
            assert!(
                patch.biomass < patch_cap,
                "{policy:?} must draw the tended patch down"
            );
            if policy >= crate::fauna::MSY_BIOMASS_FRACTION {
                // Sustainable **by its floor**, not by sitting under the MSY line: a first harvest of
                // a patch standing above `K/2` legitimately takes the accumulated surplus, and lands
                // the patch exactly on its most productive biomass.
                assert!(
                    !row.overdraws,
                    "Sustain stops at the MSY point — no ⚠: {row:?}"
                );
                assert!(
                    patch.biomass >= patch_cap * crate::fauna::MSY_BIOMASS_FRACTION - 1e-3,
                    "Sustain never draws a tended patch below `K/2`: {row:?}"
                );
            } else {
                assert!(
                    row.actual > row.sustainable,
                    "{policy:?} over-farms a tended patch — the ⚠ that could never fire before: \
                     {row:?}"
                );
            }
            takes.push((policy, row.actual));
        }
        // Four policies, four DIFFERENT takes — the playtest's "+0.66 whatever I pick", inverted.
        for (i, (policy, take)) in takes.iter().enumerate() {
            for (other_policy, other) in takes.iter().skip(i + 1) {
                assert!(
                    (take - other).abs() > 1e-3,
                    "the policy axis must be live on a tended patch: {policy:?} and \
                     {other_policy:?} both pay {take}"
                );
            }
        }
        // ...and ordered as the axis means: restraint takes least, denial takes most. At the S2 neutral
        // gain the tended patch reads the wild curve, so the natural order holds end to end — Sustain
        // the leanest, then the boosted Surplus, then the flat Deplete skim, Eradicate the deepest.
        let take_of = |wanted: f32| {
            takes
                .iter()
                .find(|(policy, _)| *policy == wanted)
                .expect("every policy ran")
                .1
        };
        assert!(take_of(0.5) < take_of(0.3));
        assert!(take_of(0.3) < take_of(0.15));
        assert!(take_of(0.15) < take_of(0.0));
    }

    /// Place-locality: only the band that tends the cultivated patch is paid. A second same-faction
    /// band that does not tend it (forages an empty neighbor tile) receives nothing — the retired
    /// even-split would have paid it a share.
    #[test]
    fn tended_yield_is_place_local_not_split() {
        let (mut world, tile) = world_with_source(CAP);
        let patch_cap = world
            .resource::<LaborConfigHandle>()
            .get()
            .forage
            .capacity_for(SOURCE_BIOME);
        cultivate_source_patch(&mut world, patch_cap);

        // Band A tends the cultivated patch on (0,0).
        let tending = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: UVec2::new(0, 0),
                    floor: 0.5,
                    species: None,
                },
                workers: WORKERS,
                improvement: None,
            }],
        );
        // Band B (same faction) forages the neighbor tile (1,0), which has no food module/patch →
        // it earns nothing from the cultivated patch.
        let idle_tile = world.resource::<TileRegistry>().tiles[1];
        let non_tending = spawn_band(
            &mut world,
            idle_tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: UVec2::new(1, 0),
                    floor: 0.5,
                    species: None,
                },
                workers: WORKERS,
                improvement: None,
            }],
        );

        world.run_system_once(advance_labor_allocation);

        let tending_food = world
            .get::<PopulationCohort>(tending)
            .unwrap()
            .stores
            .get(FOOD)
            .to_f32();
        let other_food = world
            .get::<PopulationCohort>(non_tending)
            .unwrap()
            .stores
            .get(FOOD)
            .to_f32();
        assert!(
            tending_food > 0.0,
            "the tending band is paid: {tending_food}"
        );
        assert!(
            other_food.abs() < 1e-9,
            "a non-tending same-faction band gets no tended yield (no even-split): {other_food}"
        );
    }

    /// **The free path is gone.** Sustain-foraging a Thriving patch still *teaches the faction
    /// Cultivation* (Rung 1b knowledge, earned by doing), but it **never** accrues
    /// `cultivation_progress` any more — not even once the faction knows Cultivation. Cultivating is
    /// an explicit policy with an investment cost, not a free by-product of gathering.
    #[test]
    fn sustain_forage_teaches_cultivation_but_never_accrues_patch_progress() {
        let (mut world, tile) = world_with_source(CAP * 0.5);
        spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    floor: 0.5,
                    species: None,
                },
                workers: WORKERS,
                improvement: None,
            }],
        );

        world.run_system_once(advance_labor_allocation);
        let learned = world
            .resource::<DiscoveryProgressLedger>()
            .get_progress(FactionId(0), CULTIVATION_DISCOVERY_ID)
            .to_f32();
        assert!(
            learned > 0.0,
            "Sustain-forage still earns Cultivation knowledge: {learned}"
        );
        assert_eq!(
            patch_progress(&world),
            0.0,
            "Sustain must not silently tame the patch"
        );

        // Even with Cultivation fully known, Sustain still accrues nothing — the old free path.
        world
            .resource_mut::<DiscoveryProgressLedger>()
            .add_progress(FactionId(0), CULTIVATION_DISCOVERY_ID, scalar_one());
        world.run_system_once(advance_labor_allocation);
        assert_eq!(
            patch_progress(&world),
            0.0,
            "knowing Cultivation must not make Sustain tame the patch — Cultivate is the only path"
        );
    }

    /// The source patch's live `cultivation_progress`.
    fn patch_progress(world: &World) -> f32 {
        world
            .resource::<ForageRegistry>()
            .patch(SOURCE)
            .expect("seeded patch")
            .cultivation_progress
    }

    /// **A map seed whose realization of the source tile is worth tending.** Per-tile realization
    /// (§10) draws a different basket per `(map_seed, tile)`, and under the default seed 0 the
    /// harness tile realizes its staple at a diluted ~0.40 share — correct behaviour, but not the
    /// "a crop is at home here" ground a *payoff* test needs. Seed 3 realizes `seed_grasses` at
    /// ~0.77, so weeding saturates and the tended payoff clears wild by a visible margin.
    const WORTH_TENDING_SEED: u64 = 3;

    /// Grant the harness faction full knowledge of a discovery (the Rung 1b/1c ledger gate that the
    /// Cultivate / Corral improvements check).
    fn grant_knowledge(world: &mut World, discovery: u32) {
        world
            .resource_mut::<DiscoveryProgressLedger>()
            .add_progress(BAND_FACTION, discovery, scalar_one());
    }

    /// **Cultivate is an investment.** With Cultivation known and the patch Thriving, working it under
    /// the `Cultivate` policy pays only the `plant:tended` rung's `yield_fraction_while_building ×
    /// the Sustain (MSY) yield` (the dip) while accruing progress each turn; once progress reaches `1.0` the patch is cultivated and
    /// pays the full tended yield instead — strictly more than the wild Sustain skim.
    #[test]
    fn cultivate_policy_pays_the_dip_then_the_tended_yield() {
        let (mut world, tile) = world_with_source(CAP);
        // **Both halves of this test must stand on the SAME ground.** The dip is a fraction of the
        // Sustain yield, and since #433 that yield is the tile's own basket average — so the Sustain
        // baseline and the Cultivate run have to share the seed that decides the tile's realization
        // (see the note on the Cultivate world below), or the comparison is between two baskets.
        world.resource_mut::<SimulationConfig>().map_seed = WORTH_TENDING_SEED;
        grant_knowledge(&mut world, CULTIVATION_DISCOVERY_ID);
        // The dip is read only to assert the rung *is* an investment; its exact composition is
        // pinned by the forecast==actual sweep (see the note at the end of this test).
        let (_dip_fraction, progress_per_turn) = {
            let ladder = world.resource::<LadderConfigHandle>().get();
            let tended = ladder.rung(RungKey::PlantTended);
            (
                tended
                    .yield_fraction_while_building()
                    .expect("the tended rung is an investment"),
                tended.build_accrual(
                    Some(Improvement::Cultivate),
                    true,
                    FOOD_PEAK_FLOOR,
                    RUNG_TIMESCALE_UNSCALED,
                    full_crew(tended),
                ),
            )
        };

        // Baseline: what the same patch pays under Sustain (the MSY skim) with ample workers.
        let sustain_world_band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    floor: 0.5,
                    species: None,
                },
                workers: WORKERS,
                improvement: None,
            }],
        );
        world.run_system_once(advance_labor_allocation);
        let sustain_yield = world
            .get::<LaborAllocation>(sustain_world_band)
            .unwrap()
            .last_yields[0]
            .actual;

        // Cultivate on a fresh patch: the take is the dip, and progress accrues.
        let (mut world, tile) = world_with_source(CAP);
        // Seat this world on a map seed where the source tile's per-tile realization (§10) puts its
        // dominant staple high — with F5's fuller PrairieSteppe basket, tile (0,0) realizes a diluted
        // slice under the default seed 0 (seed_grasses at share ~0.40, not worth tending), which is
        // *correct* realization behaviour but not the "worth-tending tile" this yield test needs.
        world.resource_mut::<SimulationConfig>().map_seed = WORTH_TENDING_SEED;
        grant_knowledge(&mut world, CULTIVATION_DISCOVERY_ID);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    floor: 0.5,
                    species: None,
                },
                workers: WORKERS,
                improvement: Some(Improvement::Cultivate),
            }],
        );
        world.run_system_once(advance_labor_allocation);
        let preparing = world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual;
        // **The dip prices HANDS, not the floor** (`docs/plan_harvest_floor.md` §3.1). The take is
        // `min(workers × per_worker × dip, ceiling)`, so this crew — big enough to saturate the
        // ceiling several times over — pays **nothing** for the build: hands were not the scarce
        // thing here. That is the legible half of the change ("at 25% carry it takes four times the
        // people to clear the same surplus"), and it is asserted beside the sparse-crew case below,
        // because either reading alone looks like a bug.
        assert!(
            (preparing - sustain_yield).abs() < FORECAST_EPSILON,
            "a crew that saturates the ceiling anyway pays no dip: {preparing} vs {sustain_yield}"
        );
        assert!(
            (patch_progress(&world) - progress_per_turn).abs() < 1e-6,
            "one Cultivate turn accrues progress_per_turn: {}",
            patch_progress(&world)
        );

        // Run it to completion. The regrowth system runs alongside (as it does in the real Logistics
        // stage) — the preparing take is a *fraction* of MSY, so it is sustainable and the patch stays
        // healthy while the ground is prepared: exactly the point of drawing the dip off the MSY
        // ceiling rather than depleting the patch to pay for the investment.
        let turns_to_prepare = (1.0 / progress_per_turn).ceil() as u32;
        for _ in 0..turns_to_prepare {
            world.run_system_once(advance_forage_regrowth);
            world.run_system_once(advance_labor_allocation);
        }
        assert_eq!(
            world
                .resource::<ForageRegistry>()
                .patch(SOURCE)
                .unwrap()
                .ecology_phase,
            EcologyPhase::Thriving,
            "the preparing dip is a sustainable draw — the patch never leaves Thriving"
        );
        assert!(
            world
                .resource::<ForageRegistry>()
                .patch(SOURCE)
                .unwrap()
                .is_cultivated(),
            "sustained Cultivate work completes the patch"
        );
        // **Harvest the finished patch to read the payoff.** The loop above already ran past the
        // completing turn, so the sim has retired `Cultivate` onto the harvest rung itself (issue
        // #420) and this call is a no-op re-assert — kept because what this test measures is the
        // *payoff*, and it must read that number off the harvest rung whatever put the band there.
        // The retire itself is pinned by
        // `a_completed_cultivation_retires_the_build_verb_onto_the_harvest_rung`.
        set_forage_floor(&mut world, band, 0.5);
        // One Logistics turn first: under constant escapement a patch that was just gathered is
        // sitting **on** its floor with nothing above it, so a payoff read without the regrowth would
        // measure an empty turn rather than the rung.
        world.run_system_once(advance_forage_regrowth);
        world.run_system_once(advance_labor_allocation);
        let tended = world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual;
        assert!(
            tended > sustain_yield,
            "a tended patch out-pays the wild Sustain skim — the whole point of the 25 turns: \
             {tended} vs {sustain_yield}"
        );
        assert!(
            tended > preparing,
            "the payoff exceeds the preparing dip: {tended} vs {preparing}"
        );

        // **…and a SPARSE crew does pay it.** One forager's throughput is the binding term under
        // both, so the dip shows up undiluted: `fraction ×` what the same lone forager gathers.
        // This is the half that makes the build a real cost — the crew clearing ground carries a
        // fraction of what a gathering crew carries.
        let sparse_take = |improvement: Option<Improvement>| {
            let (mut world, tile) = world_with_source(CAP);
            world.resource_mut::<SimulationConfig>().map_seed = WORTH_TENDING_SEED;
            grant_knowledge(&mut world, CULTIVATION_DISCOVERY_ID);
            let band = spawn_band(
                &mut world,
                tile,
                vec![LaborAssignment {
                    target: LaborTarget::Forage {
                        tile: SOURCE,
                        floor: 0.5,
                        species: None,
                    },
                    workers: SOLE_FORAGER,
                    improvement,
                }],
            );
            world.run_system_once(advance_labor_allocation);
            world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual
        };
        let sparse_building = sparse_take(Some(Improvement::Cultivate));
        let sparse_gathering = sparse_take(None);
        assert!(
            sparse_building < sparse_gathering,
            "a crew that is the binding term really is slowed by the dip: {sparse_building} vs \
             {sparse_gathering}"
        );
        // The exact composition `min(workers × per_worker × dip, ceiling)` is pinned per component
        // and at both binding regimes by
        // `forage_forecast_equals_actual_take_for_every_floor_and_staffing`, against a real
        // `advance_labor_allocation` run — not restated here.
    }

    /// **One forager**, so the crew's throughput is the binding term rather than the patch's
    /// standing stock — the only regime in which the build dip is visible at all since it moved onto
    /// crew throughput (`docs/plan_harvest_floor.md` §3.1).
    const SOLE_FORAGER: u32 = 1;

    /// **Corral mirrors Cultivate.** With Herding known and a domesticated herd it owns, a band working
    /// it under `Corral` takes only `corralling_yield_fraction × the Sustain (MSY) yield` while the pen
    /// accrues; at `corral_progress == 1.0` the herd is penned and pays the corral yield.
    #[test]
    fn corral_policy_pays_the_dip_then_pens_and_pays_the_corral_yield() {
        const BIG_HERD_CAP: f32 = 1_000.0;
        /// Seat the herd a little **above** its `K/2` escapement point: enough spare biomass that a
        /// Sustain take is a real, ceiling-bound number, few enough animals that 10 hunters can carry
        /// all of them. Both halves of the comparison must be ceiling-bound or the dip identity is
        /// measuring the carry cap instead (see below).
        const DIP_TEST_ESCAPEMENT_FRACTION: f32 = 0.55;
        let (fraction, build_per_turn) = {
            let (world, _) = world_with_source(CAP);
            let ladder = world.resource::<LadderConfigHandle>().get();
            let pen = ladder.rung(RungKey::AnimalPen);
            (
                pen.yield_fraction_while_building()
                    .expect("the pen rung is an investment"),
                pen.build_accrual(
                    Some(Improvement::Corral),
                    true,
                    FOOD_PEAK_FLOOR,
                    RUNG_TIMESCALE_UNSCALED,
                    full_crew(pen),
                ),
            )
        };

        // Baseline Sustain hunt yield on the same herd (ample hunters → **ceiling**-bound).
        // **It must be DOMESTICATED too**: Corral can only be worked on a domesticated herd, and the
        // husbandry ladder means a tamed herd lives on the *pastoral* ecology (`r` = 0.15, 3× wild).
        // Comparing the dip against a *wild* herd's MSY would compare two different rungs.
        //
        // **RETARGETED IN SLICE 8 — the herd is seated JUST ABOVE its escapement point, not at
        // capacity.** "The dip pays `fraction ×` the Sustain yield" is only true when **both** takes
        // are ceiling-bound; the moment Sustain becomes *collection*-bound the dip is a fraction of a
        // ceiling the baseline never reached, and the identity is arithmetically false rather than
        // broken. At capacity that is now exactly what happens: escapement is `K/2` = 500 biomass, so
        // 10 hunters (400) are no longer "ample" — Sustain reads 8, Corral reads its full ceiling 5,
        // and `0.5 × 8 = 4 ≠ 5`. Seating the herd at `0.55 × K` restores the fixture's own stated
        // premise (a small escapement the crew can comfortably carry), so the test measures the dip
        // instead of measuring the carry cap.
        let (mut world, tile) = world_with_source(CAP);
        reseat_herd(
            &mut world,
            BIG_HERD_CAP * DIP_TEST_ESCAPEMENT_FRACTION,
            BIG_HERD_CAP,
        );
        {
            let mut registry = world.resource_mut::<HerdRegistry>();
            registry.herds[0].accrue_domestication(BAND_FACTION, RUNG_COMPLETE);
        }
        let sustain_band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.5,
                },
                workers: WORKERS,
                improvement: None,
            }],
        );
        world.run_system_once(advance_labor_allocation);
        let sustain_yield = world
            .get::<LaborAllocation>(sustain_band)
            .unwrap()
            .last_yields[0]
            .actual;

        // Corral on a domesticated herd the faction owns + knows **Penning** for (the §4.3
        // reshuffle: rung 3's gate moved off Herding, which now gates `tame` alone).
        let (mut world, tile) = world_with_source(CAP);
        reseat_herd(
            &mut world,
            BIG_HERD_CAP * DIP_TEST_ESCAPEMENT_FRACTION,
            BIG_HERD_CAP,
        );
        grant_knowledge(&mut world, PENNING_DISCOVERY_ID);
        {
            let mut registry = world.resource_mut::<HerdRegistry>();
            registry.herds[0].accrue_domestication(BAND_FACTION, RUNG_COMPLETE);
        }
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.5,
                },
                workers: WORKERS,
                improvement: Some(Improvement::Corral),
            }],
        );
        world.run_system_once(advance_labor_allocation);
        let preparing = world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual;
        // **RETARGETED BY THE HARVEST FLOOR: the dip prices HANDS, not the escapement** (§3.1). It
        // multiplies `workers × per_worker_carry`, so this crew — ample enough that the herd's own
        // escapement is the binding term either way — pays **nothing** for the pen. That is the
        // legible half of the move ("at 50% carry it takes twice the people to bring the same
        // animals home"), and the sparse-crew case below is the other; either alone reads as a bug.
        assert!(
            (preparing - sustain_yield).abs() < FORECAST_EPSILON,
            "a crew the escapement binds anyway pays no dip: {preparing} vs {sustain_yield}"
        );

        let turns_to_build = (1.0 / build_per_turn).ceil() as u32;
        for _ in 0..turns_to_build {
            world.run_system_once(advance_labor_allocation);
        }
        assert!(
            world
                .resource::<HerdRegistry>()
                .find(HERD_ID)
                .unwrap()
                .is_corralled(),
            "sustained Corral work finishes the pen"
        );
        // This harness runs the Population stage ONLY — no Logistics, so the herd never regrows while
        // the pen is built, and 25 turns of the build dip draw it below the managed harvest's
        // escapement point (`K/2`), where a pen correctly pays nothing. (In the live turn loop
        // `advance_herds` regrows it every turn — a real campaign's herd *rises* during the build,
        // because the dip is well under its MSY.) Re-seat it at capacity so this test measures what it
        // is about: the penned rung out-paying the build dip.
        reseat_herd(&mut world, BIG_HERD_CAP, BIG_HERD_CAP);
        world.run_system_once(advance_labor_allocation);
        let corral_yield = world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual;
        assert!(
            corral_yield > preparing,
            "a penned herd out-pays the build dip: {corral_yield} vs {preparing}"
        );

        // **…and a SPARSE crew pays the dip exactly.** One hunter's carry is the binding term under
        // both, and the dip halves it, so the whole-animal quantiser divides the same body mass into
        // both takes and the identity survives rounding: `preparing == fraction × hunting`.
        let sparse_take = |improvement: Option<Improvement>| {
            let (mut world, tile) = world_with_source(CAP);
            reseat_herd(
                &mut world,
                BIG_HERD_CAP * DIP_TEST_ESCAPEMENT_FRACTION,
                BIG_HERD_CAP,
            );
            grant_knowledge(&mut world, PENNING_DISCOVERY_ID);
            {
                let mut registry = world.resource_mut::<HerdRegistry>();
                registry.herds[0].accrue_domestication(BAND_FACTION, RUNG_COMPLETE);
            }
            let band = spawn_band(
                &mut world,
                tile,
                vec![LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: HERD_ID.to_string(),
                        floor: 0.5,
                    },
                    workers: SOLE_HUNTER,
                    improvement,
                }],
            );
            world.run_system_once(advance_labor_allocation);
            world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual
        };
        let sparse_building = sparse_take(Some(Improvement::Corral));
        let sparse_hunting = sparse_take(None);
        assert!(
            sparse_building < sparse_hunting,
            "a crew that is the binding term really is slowed by the dip: {sparse_building} vs \
             {sparse_hunting}"
        );
        assert!(
            (sparse_building - fraction * sparse_hunting).abs() < FORECAST_EPSILON,
            "…and pays exactly `fraction ×` what the same lone hunter takes: {sparse_building} vs \
             {}",
            fraction * sparse_hunting
        );
    }

    /// **One hunter**, so the crew's carry is the binding term rather than the herd's escapement —
    /// the only regime in which the build dip is visible at all since it moved onto crew throughput
    /// (`docs/plan_harvest_floor.md` §3.1).
    const SOLE_HUNTER: u32 = 1;

    // ---------------------------------------------------------------------------------------------
    // **Completion CLEARS the improvement and leaves the stance alone** (issues #420 + #442). All four
    // rungs share one seam: the turn a build meter fills, the assignment's `improvement` returns to
    // `None`, preserving the source, the commitment, the crew — and, since #442, the player's stated
    // stance. Left on the build verb the band paid `yield_fraction_while_building` forever on a rung
    // that could never accomplish anything more (#420); rewritten onto a hardcoded harvest stance, the
    // sim silently replaced a policy the player chose (#442).
    // ---------------------------------------------------------------------------------------------

    /// **A deliberately NON-default stance for the completion tests.** The handoff used to rewrite
    /// `policy` to `Sustain`, so a completion test run under Sustain could not tell "the stance was
    /// left alone" from "the stance was rewritten to the value it already had". Surplus is a real
    /// player choice and is *not* what the retired constant would have written.
    ///
    /// **Every completion test computes its build length AT this floor**, not at the food peak:
    /// since `docs/plan_harvest_floor.md` §3 the accrual is `progress_per_turn ×
    /// learn_multiplier(floor)`, so a builder holding `0.3` takes `0.5/0.3` times as many turns. A
    /// fixture that counted peak-rate turns would stop one short of the completion it is asserting.
    /// There is no health gate left for the floor to trip — pulling harder now *slows* the meter
    /// rather than stopping it.
    const BUILDER_FLOOR: f32 = 0.3;

    /// The client's pre-turn expected take on the source patch at `floor`, off the patch's
    /// **current** state — the same `forage_forecast` composition the forecast==actual sweep uses. Lets
    /// a test name the exact number a turn should pay without re-deriving the MSY/dip arithmetic.
    fn forage_expected_take(
        world: &World,
        workers: u32,
        floor: f32,
        improvement: Option<Improvement>,
    ) -> f32 {
        let patch = world
            .resource::<ForageRegistry>()
            .patch(SOURCE)
            .cloned()
            .expect("seeded patch");
        let composition = source_tile_composition(world);
        let labor = world.resource::<LaborConfigHandle>().get();
        let forecast = forage_forecast(
            &patch,
            &composition,
            &labor.forage,
            &FloraConfig::builtin(),
            &LadderConfig::builtin(),
            SEASONAL_WEIGHT,
            NEUTRAL_OUTPUT_MULT,
        );
        expected_yield(&forecast, workers, floor, improvement)
    }

    /// **What the source tile grows** — the realized basket, through the one `tile_flora_composition`
    /// seam the labor arm reads, so a test forecast is priced off exactly the composition the turn
    /// will pay from (#433).
    fn source_tile_composition(world: &World) -> Vec<crate::flora_config::FloraShare> {
        let labor = world.resource::<LaborConfigHandle>().get();
        let flora = world
            .resource::<crate::flora_config::FloraConfigHandle>()
            .get();
        let map_seed = world.resource::<SimulationConfig>().map_seed;
        let tile_entity = world.resource::<TileRegistry>().tiles[0];
        let ground = world.get::<Tile>(tile_entity).expect("the source tile");
        crate::forage::tile_flora_composition(&flora, &labor.forage, ground, map_seed).into_owned()
    }

    /// The plant the source tile's realized basket auto-picks for `rung` — the same
    /// `default_species_for_rung` answer the labor arm reaches. Named **explicitly** on the test
    /// assignment so the retire pass can be asserted to carry the *commitment* across, not merely the
    /// tile coordinate.
    fn source_tile_default_crop(world: &World, rung: RungKey) -> String {
        let labor = world.resource::<LaborConfigHandle>().get();
        let flora = world
            .resource::<crate::flora_config::FloraConfigHandle>()
            .get();
        let map_seed = world.resource::<SimulationConfig>().map_seed;
        let tile_entity = world.resource::<TileRegistry>().tiles[0];
        let ground = world.get::<Tile>(tile_entity).expect("the source tile");
        let composition =
            crate::forage::tile_flora_composition(&flora, &labor.forage, ground, map_seed);
        crate::forage::default_species_for_rung(&composition, &flora, rung)
            .expect("the source tile grows something the tended rung can commit to")
    }

    /// The band's single assignment — completion clears one field *in place*, so every other field of
    /// the row is evidence that nothing else moved.
    fn only_assignment(world: &World, band: Entity) -> LaborAssignment {
        let allocation = world.get::<LaborAllocation>(band).expect("the band works");
        assert_eq!(
            allocation.assignments.len(),
            1,
            "completion edits a row, it never adds or drops one"
        );
        allocation.assignments[0].clone()
    }

    /// **THE issue-#420 + #442 fix, plant rung 2.** A band whose patch finishes cultivating this
    /// turn:
    ///
    /// 1. still pays the **dipped** take on the completing turn (the accrue-after-take ordering — the
    ///    pre-commit forecast promised the dip, and completing must not retroactively pay more);
    /// 2. has its **improvement cleared** afterwards, with its worker count, its tile, its committed
    ///    species **and its stance** intact — the last of those being what #442 fixed: the sim used to
    ///    rewrite `policy` to a hardcoded Sustain, replacing a choice the player made;
    /// 3. **pays the undipped take the NEXT turn** — the actual #420 bug: left on the build verb the
    ///    band went on paying the dip forever on ground that was already prepared.
    #[test]
    fn a_completed_cultivation_clears_the_improvement_and_leaves_the_stance_alone() {
        let (mut world, tile) = world_with_source(CAP);
        // The same worth-tending seed `cultivate_policy_pays_the_dip_then_the_tended_yield` pins: the
        // source tile's realization must concentrate its staple hard enough that the tended payoff
        // clears wild, or step 3 would be measuring a marginal crop rather than the retire.
        world.resource_mut::<SimulationConfig>().map_seed = 3;
        grant_knowledge(&mut world, CULTIVATION_DISCOVERY_ID);
        let crop = source_tile_default_crop(&world, RungKey::PlantTended);
        let progress_per_turn = {
            let ladder = world.resource::<LadderConfigHandle>().get();
            {
                let tended = ladder.rung(RungKey::PlantTended);
                tended.build_accrual(
                    Some(Improvement::Cultivate),
                    true,
                    BUILDER_FLOOR,
                    RUNG_TIMESCALE_UNSCALED,
                    full_crew(tended),
                )
            }
        };
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    floor: BUILDER_FLOOR,
                    species: Some(crop.clone()),
                },
                workers: WORKERS,
                improvement: Some(Improvement::Cultivate),
            }],
        );

        // Every turn but the last: the meter fills and the verb stays put.
        let turns_to_prepare = (1.0 / progress_per_turn).ceil() as u32;
        for _ in 0..turns_to_prepare - 1 {
            world.run_system_once(advance_forage_regrowth);
            world.run_system_once(advance_labor_allocation);
        }
        assert!(
            !world
                .resource::<ForageRegistry>()
                .patch(SOURCE)
                .unwrap()
                .is_cultivated(),
            "fixture: the patch must still be under construction here"
        );
        assert_eq!(
            only_assignment(&world, band).improvement,
            Some(Improvement::Cultivate),
            "an unfinished build keeps its verb — only completion clears it"
        );

        // (1) The completing turn still pays the dip, to the number.
        world.run_system_once(advance_forage_regrowth);
        let promised_dip =
            forage_expected_take(&world, WORKERS, BUILDER_FLOOR, Some(Improvement::Cultivate));
        world.run_system_once(advance_labor_allocation);
        let completing = world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual;
        assert!(
            world
                .resource::<ForageRegistry>()
                .patch(SOURCE)
                .unwrap()
                .is_cultivated(),
            "fixture: this is the completing turn"
        );
        assert!(
            (completing - promised_dip).abs() < FORECAST_EPSILON,
            "the turn progress reaches 1.0 is the LAST preparing take — it pays the dip the \
             pre-commit forecast promised: {completing} vs {promised_dip}"
        );

        // (2) The handoff: the improvement cleared, and NOTHING else moved — least of all the
        // stance. Rewriting `policy` here is exactly what issue #442 deletes.
        let completed = only_assignment(&world, band);
        assert_eq!(completed.workers, WORKERS, "the crew stays on the source");
        assert_eq!(
            completed.improvement, None,
            "completion clears the improvement — there is nothing left to build here"
        );
        let LaborTarget::Forage {
            tile: completed_tile,
            floor,
            species,
        } = &completed.target
        else {
            panic!("completion must not change the target's KIND: {completed:?}");
        };
        assert_eq!(
            *floor, BUILDER_FLOOR,
            "THE #442 fix: the sim never rewrites the player's floor — it was never vacated, so \
             there is nothing to hand back"
        );
        assert_eq!(*completed_tile, SOURCE, "the same ground");
        assert_eq!(
            species.as_deref(),
            Some(crop.as_str()),
            "the crop the crew committed 25 turns to survives the handoff"
        );

        // (3) The bug: the next turn pays the tended harvest, not the dip.
        world.run_system_once(advance_forage_regrowth);
        let promised_harvest = forage_expected_take(&world, WORKERS, BUILDER_FLOOR, None);
        world.run_system_once(advance_labor_allocation);
        let after = world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual;
        assert!(
            (after - promised_harvest).abs() < FORECAST_EPSILON,
            "the band collects the undipped take under its OWN stance: {after} vs \
             {promised_harvest}"
        );
        assert!(
            after > completing,
            "the payoff the 25 turns bought arrives WITHOUT the player touching the picker — the \
             whole of issue #420: {after} vs the dip {completing}"
        );
    }

    /// One Logistics-stage regrowth for the source herd, through the shipped `fauna::regrow_biomass`
    /// — the **exact twin of the `advance_forage_regrowth` call the plant completion test above
    /// already makes**, and the asymmetry it removes.
    ///
    /// The completion harnesses otherwise drive the Population stage only, so a herd never regrows.
    /// A 42-turn build at [`BUILDER_FLOOR`] would then be asserted against a herd its own crew
    /// emptied to that floor in four turns — after which nothing stands above the floor and
    /// `crew_is_working_the_source` is correctly false. That is the sim behaving properly, not a
    /// gate to route around: a Population-only loop is half a turn, and a *completion* test needs the
    /// order the sim runs. (It was invisible before the harvest floor only because the retired
    /// `EcologyPhase::Thriving` gate read a phase `refresh_ecology_phase` never updated here, so it
    /// stayed frozen at the value `reseat_herd` set.)
    fn regrow_source_herd(world: &mut World) {
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let mut registry = world.resource_mut::<HerdRegistry>();
        crate::fauna::regrow_biomass(&mut registry.herds[0], &fauna);
    }

    /// **The animal twin, rung 2.** A herd that finishes taming this turn hands its crew to the harvest
    /// rung with the herd id and the crew intact — so the band starts collecting the pastoral payoff
    /// instead of paying the taming dip on an already-tame herd forever.
    #[test]
    fn a_completed_taming_clears_the_improvement_and_leaves_the_stance_alone() {
        const BIG_HERD_CAP: f32 = 1_000.0;
        let (mut world, tile) = world_with_source(CAP);
        reseat_herd(&mut world, BIG_HERD_CAP, BIG_HERD_CAP);
        grant_knowledge(&mut world, HERDING_DISCOVERY_ID);
        let (taming_per_turn, species) = {
            let ladder = world.resource::<LadderConfigHandle>().get();
            let fauna = world.resource::<FaunaConfigHandle>().get();
            let species = world.resource::<HerdRegistry>().herds[0].species.clone();
            (
                {
                    let pastoral = ladder.rung(RungKey::AnimalPastoral);
                    pastoral.build_accrual(
                        Some(Improvement::Tame),
                        true,
                        BUILDER_FLOOR,
                        fauna.taming_rate_for(&species),
                        full_crew(pastoral),
                    )
                },
                species,
            )
        };
        assert!(
            taming_per_turn > 0.0,
            "fixture: the {species} herd must actually tame"
        );
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: BUILDER_FLOOR,
                },
                workers: WORKERS,
                improvement: Some(Improvement::Tame),
            }],
        );

        let turns_to_tame = (1.0 / taming_per_turn).ceil() as u32;
        for _ in 0..turns_to_tame - 1 {
            regrow_source_herd(&mut world);
            world.run_system_once(advance_labor_allocation);
        }
        assert!(
            !world
                .resource::<HerdRegistry>()
                .find(HERD_ID)
                .unwrap()
                .is_domesticated(),
            "fixture: the herd must still be being gentled here"
        );
        assert_eq!(
            only_assignment(&world, band).improvement,
            Some(Improvement::Tame),
            "an unfinished build keeps its verb"
        );

        regrow_source_herd(&mut world);
        world.run_system_once(advance_labor_allocation);
        assert!(
            world
                .resource::<HerdRegistry>()
                .find(HERD_ID)
                .unwrap()
                .is_domesticated(),
            "fixture: this is the completing turn"
        );
        let completed = only_assignment(&world, band);
        assert_eq!(completed.workers, WORKERS, "the crew stays on the herd");
        assert_eq!(completed.improvement, None, "completion clears the verb");
        let LaborTarget::Hunt { fauna_id, floor } = &completed.target else {
            panic!("completion must not change the target's KIND: {completed:?}");
        };
        assert_eq!(
            *floor, BUILDER_FLOOR,
            "the player's stance is never rewritten (issue #442)"
        );
        assert_eq!(fauna_id, HERD_ID, "the same herd");
    }

    /// **The animal twin, rung 3.** A pen that finishes this turn clears `Corral` the same way — the
    /// keeper crew stays on the herd, under the stance it chose, and starts drawing the pen's
    /// harvest.
    #[test]
    fn a_completed_pen_clears_the_improvement_and_leaves_the_stance_alone() {
        const BIG_HERD_CAP: f32 = 1_000.0;
        let (mut world, tile) = world_with_source(CAP);
        reseat_herd(&mut world, BIG_HERD_CAP, BIG_HERD_CAP);
        grant_knowledge(&mut world, PENNING_DISCOVERY_ID);
        {
            let mut registry = world.resource_mut::<HerdRegistry>();
            registry.herds[0].accrue_domestication(BAND_FACTION, RUNG_COMPLETE);
        }
        let build_per_turn = {
            let ladder = world.resource::<LadderConfigHandle>().get();
            {
                let pen = ladder.rung(RungKey::AnimalPen);
                pen.build_accrual(
                    Some(Improvement::Corral),
                    true,
                    BUILDER_FLOOR,
                    RUNG_TIMESCALE_UNSCALED,
                    full_crew(pen),
                )
            }
        };
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: BUILDER_FLOOR,
                },
                workers: WORKERS,
                improvement: Some(Improvement::Corral),
            }],
        );

        let turns_to_build = (1.0 / build_per_turn).ceil() as u32;
        for _ in 0..turns_to_build - 1 {
            world.run_system_once(advance_labor_allocation);
        }
        assert!(
            !world
                .resource::<HerdRegistry>()
                .find(HERD_ID)
                .unwrap()
                .is_corralled(),
            "fixture: the pen must still be going up here"
        );
        assert_eq!(
            only_assignment(&world, band).improvement,
            Some(Improvement::Corral),
            "an unfinished build keeps its verb"
        );

        world.run_system_once(advance_labor_allocation);
        assert!(
            world
                .resource::<HerdRegistry>()
                .find(HERD_ID)
                .unwrap()
                .is_corralled(),
            "fixture: this is the completing turn"
        );
        let completed = only_assignment(&world, band);
        assert_eq!(
            completed.workers, WORKERS,
            "the keeper crew stays on the pen"
        );
        assert_eq!(completed.improvement, None, "completion clears the verb");
        let LaborTarget::Hunt { fauna_id, floor } = &completed.target else {
            panic!("completion must not change the target's KIND: {completed:?}");
        };
        assert_eq!(
            *floor, BUILDER_FLOOR,
            "the player's stance is never rewritten (issue #442)"
        );
        assert_eq!(fauna_id, HERD_ID, "the same herd");
    }

    /// Without the earned knowledge, the improvements accrue **nothing** — the take is still the
    /// reduced preparing dip (the crew tries, and gets nowhere), but no progress is made. The command
    /// layer rejects the assignment outright; this guards the sim-side gate underneath it.
    #[test]
    fn investment_policies_accrue_nothing_without_the_knowledge() {
        let (mut world, tile) = world_with_source(CAP);
        spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    floor: 0.5,
                    species: None,
                },
                workers: WORKERS,
                improvement: Some(Improvement::Cultivate),
            }],
        );
        world.run_system_once(advance_labor_allocation);
        assert_eq!(
            patch_progress(&world),
            0.0,
            "Cultivate without Cultivation knowledge accrues nothing"
        );

        let (mut world, tile) = world_with_source(CAP);
        {
            let mut registry = world.resource_mut::<HerdRegistry>();
            registry.herds[0].accrue_domestication(BAND_FACTION, RUNG_COMPLETE);
        }
        spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.5,
                },
                workers: WORKERS,
                improvement: Some(Improvement::Corral),
            }],
        );
        world.run_system_once(advance_labor_allocation);
        let herd = world.resource::<HerdRegistry>().find(HERD_ID).unwrap();
        assert_eq!(
            herd.corral_progress, 0.0,
            "Corral without PENNING knowledge builds nothing (the §4.3 gate reshuffle — Herding \
             is no longer enough)"
        );
        assert!(!herd.is_corralled());
    }

    /// A Corral assignment on a herd that is **not domesticated** builds nothing (the second gate).
    #[test]
    fn corral_accrues_nothing_on_a_wild_herd() {
        let (mut world, tile) = world_with_source(CAP);
        grant_knowledge(&mut world, PENNING_DISCOVERY_ID);
        spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.5,
                },
                workers: WORKERS,
                improvement: Some(Improvement::Corral),
            }],
        );
        world.run_system_once(advance_labor_allocation);
        let herd = world.resource::<HerdRegistry>().find(HERD_ID).unwrap();
        assert_eq!(
            herd.corral_progress, 0.0,
            "a wild herd cannot be penned — tame it first"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // The knowledge pattern (slice 4, `docs/plan_intensification_ladder.md` §4): **practising a rung
    // teaches the knowledge that unlocks the next rung's verb** — where "practising rung N" means
    // *working a source that currently STANDS ON rung N*, not "using rung N's verb".
    // ---------------------------------------------------------------------------------------------

    /// A herd big enough that a Sustain/Tame take never scrapes it out of the `Thriving` band
    /// mid-test — the earn gate reads the phase, so a starved fixture would pass for the wrong
    /// reason. (Mirrors the local const the corral/tame yield tests use.)
    const TEACHING_HERD_CAP: f32 = 1_000.0;

    /// Faction 0's ledger progress on `discovery`.
    fn knowledge(world: &World, discovery: u32) -> f32 {
        world
            .resource::<DiscoveryProgressLedger>()
            .get_progress(BAND_FACTION, discovery)
            .to_f32()
    }

    /// Staff a band on the source herd under `policy` and resolve one turn.
    fn hunt_one_turn(world: &mut World, tile: Entity, policy: f32) {
        spawn_band(
            world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: policy,
                },
                workers: WORKERS,
                improvement: None,
            }],
        );
        world.run_system_once(advance_labor_allocation);
    }

    /// Staff a band on the source patch under `policy` and resolve one turn.
    fn forage_one_turn(world: &mut World, tile: Entity, policy: f32) {
        spawn_band(
            world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    floor: policy,
                    species: None,
                },
                workers: WORKERS,
                improvement: None,
            }],
        );
        world.run_system_once(advance_labor_allocation);
    }

    /// **Rung 1 is unchanged by the refactor.** A Sustain hunt on a Thriving *wild* herd still earns
    /// Herding — the shipped §0 behaviour — now driven by the `animal:wild` rung's `earns_knowledge`
    /// rather than a hard-coded branch. It teaches **Herding and nothing else**: Penning is the rung
    /// above, and rung 1 must not skip it.
    #[test]
    fn sustain_hunting_a_wild_herd_still_earns_herding_only() {
        let (mut world, tile) = world_with_source(CAP);
        hunt_one_turn(&mut world, tile, 0.5);

        assert!(
            knowledge(&world, HERDING_DISCOVERY_ID) > 0.0,
            "a Sustain hunt on a Thriving wild herd still earns Herding"
        );
        assert_eq!(
            knowledge(&world, PENNING_DISCOVERY_ID),
            0.0,
            "a WILD herd teaches Herding — Penning comes from keeping TAMED ones"
        );
    }

    /// **The heart of the arc.** The *same* Sustain hunt on a herd that has climbed to **pastoral**
    /// earns **Penning** instead — "you learn herding by managing wild herds; penning by managing
    /// tamed ones". Same verb, different rung, different lesson.
    #[test]
    fn sustain_hunting_a_pastoral_herd_earns_penning() {
        let (mut world, tile) = world_with_source(CAP);
        reseat_herd(&mut world, TEACHING_HERD_CAP, TEACHING_HERD_CAP);
        {
            let mut registry = world.resource_mut::<HerdRegistry>();
            registry.herds[0].accrue_domestication(BAND_FACTION, RUNG_COMPLETE);
            assert!(
                registry.herds[0].is_domesticated(),
                "the herd stands on rung 2"
            );
        }
        hunt_one_turn(&mut world, tile, 0.5);

        assert!(
            knowledge(&world, PENNING_DISCOVERY_ID) > 0.0,
            "working a PASTORAL herd earns Penning — the rung it stands on decides the lesson"
        );
    }

    /// The plant twin: working a **tended** patch earns **Seed Selection**. The rung decides, not the
    /// verb — a tended patch pays its managed harvest under Sustain, and tending it *is* the practice.
    #[test]
    fn working_a_tended_patch_earns_seed_selection() {
        let (mut world, _tile) = world_with_source(CAP);
        let tile = world.resource::<TileRegistry>().tiles[0];
        {
            let mut registry = world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(SOURCE).expect("seeded patch");
            patch.accrue_cultivation(BAND_FACTION, RUNG_COMPLETE);
            assert!(patch.is_cultivated(), "the patch stands on rung 2");
        }
        forage_one_turn(&mut world, tile, 0.5);

        assert!(
            knowledge(&world, SEED_SELECTION_DISCOVERY_ID) > 0.0,
            "working a TENDED patch earns Seed Selection"
        );
    }

    /// **§4.2, RESTATED AS A RATE — a deeper floor learns SLOWER, and stripping learns nothing.**
    ///
    /// It replaced `the_overdrawing_policies_teach_nothing_at_any_rung`, whose subject was a **step**
    /// at the food peak (teach at or above it, nothing below). The harvest floor made restraint a
    /// rate (`intensification::learn_multiplier`, §3), so "these floors teach nothing" is no longer
    /// true of anything but `floor = 0` — and asserting the old inequality would now be asserting
    /// the model the arc removed. Swept across both webs and both of the rungs that teach, so a
    /// future rung cannot quietly opt out.
    #[test]
    fn a_deeper_floor_learns_slower_and_stripping_learns_nothing() {
        // Descending, so each entry must teach strictly less than the one before it. It reaches
        // **above** the food peak — the range the retired four-stance axis could not express.
        const DESCENDING_FLOORS: [f32; 4] = [0.9, 0.5, 0.3, 0.15];

        /// The floor at which nothing is left standing — the one that must teach exactly nothing
        /// because the *rate* is zero.
        const STRIP_IT_BARE: f32 = 0.0;

        /// **Leave it all standing.** The other degenerate end: the rate is its highest (×2), but
        /// nothing stands above the floor, so `crew_is_working_the_source` is false. Watching teaches
        /// nothing — the trade the dial offers, taken past its limit.
        const TOUCH_NOTHING: f32 = 1.0;

        // Animal rung 1 (wild) and rung 2 (pastoral), then plant rung 1 (wild) and rung 2 (tended).
        let hunt_lesson = |floor: f32, tamed: bool| {
            let (mut world, tile) = world_with_source(CAP);
            reseat_herd(&mut world, TEACHING_HERD_CAP, TEACHING_HERD_CAP);
            if tamed {
                world.resource_mut::<HerdRegistry>().herds[0]
                    .accrue_domestication(BAND_FACTION, RUNG_COMPLETE);
            }
            hunt_one_turn(&mut world, tile, floor);
            let lesson = if tamed {
                PENNING_DISCOVERY_ID
            } else {
                HERDING_DISCOVERY_ID
            };
            knowledge(&world, lesson)
        };
        let forage_lesson = |floor: f32, cultivated: bool| {
            let (mut world, _) = world_with_source(CAP);
            let tile = world.resource::<TileRegistry>().tiles[0];
            // Seated at capacity, so a floor **above** the food peak still leaves stock standing and
            // the sweep can reach the over-restraint half of the dial at all. The default fixture
            // sits on `K/2`, where every such floor honestly takes nothing.
            {
                let mut registry = world.resource_mut::<ForageRegistry>();
                let patch = registry.patch_mut(SOURCE).expect("seeded patch");
                patch.biomass = patch.carrying_capacity;
            }
            if cultivated {
                world
                    .resource_mut::<ForageRegistry>()
                    .patch_mut(SOURCE)
                    .expect("seeded patch")
                    .accrue_cultivation(BAND_FACTION, RUNG_COMPLETE);
            }
            forage_one_turn(&mut world, tile, floor);
            let lesson = if cultivated {
                SEED_SELECTION_DISCOVERY_ID
            } else {
                CULTIVATION_DISCOVERY_ID
            };
            knowledge(&world, lesson)
        };

        // **Both webs assert the SAME shape**, which is the point of the predicate the earn path
        // reads: it is the escapement room, in biomass, before the whole-animal quantiser — so the
        // *lesson* is `progress_per_turn × learn_multiplier(floor)` on both webs and orders strictly
        // in the floor. It does not, and must not, depend on `body_mass`.
        for rung_two in [false, true] {
            for (web, lesson_at) in [
                ("plant", &forage_lesson as &dyn Fn(f32, bool) -> f32),
                ("animal", &hunt_lesson as &dyn Fn(f32, bool) -> f32),
            ] {
                // **Liveness first**: a diff-based property improves when the feature breaks, so an
                // ordering sweep alone would pass on an earn path that credited zero everywhere.
                assert!(
                    lesson_at(DESCENDING_FLOORS[0], rung_two) > 0.0,
                    "{web}: the rung must actually teach at the top floor (rung 2 = {rung_two})"
                );
                let mut previous = f32::INFINITY;
                for floor in DESCENDING_FLOORS {
                    let learned = lesson_at(floor, rung_two);
                    assert!(
                        learned < previous,
                        "{web} floor {floor} must learn strictly less than the floor above it \
                         (rung 2 = {rung_two}): {learned} vs {previous}"
                    );
                    previous = learned;
                }
                assert_eq!(
                    lesson_at(STRIP_IT_BARE, rung_two),
                    0.0,
                    "{web}: stripping the source bare teaches nothing (rung 2 = {rung_two})"
                );
                assert_eq!(
                    lesson_at(TOUCH_NOTHING, rung_two),
                    0.0,
                    "{web}: …and watching it teaches nothing either (rung 2 = {rung_two})"
                );
            }
        }
    }

    // `a_source_that_is_not_thriving_teaches_nothing` was deleted with its subject: the
    // `EcologyPhase::Thriving` gate both earn sites carried is gone (`docs/plan_harvest_floor.md`
    // §3.2), replaced by `crew_is_working_the_source` and a floor-paced rate. A collapsing source
    // that still stands above the crew's floor is still being practised on.

    /// **§4.2 — the two food webs learn separately.** Hunting only ever advances the animal track and
    /// foraging the plant track: a master rancher isn't automatically a farmer. This falls out of the
    /// rung's branch, but it is the claim the design makes, so it is asserted directly.
    #[test]
    fn the_two_food_webs_do_not_cross_teach() {
        // Hunting a wild herd teaches Herding and touches NEITHER plant knowledge.
        let (mut world, tile) = world_with_source(CAP);
        hunt_one_turn(&mut world, tile, 0.5);
        assert!(knowledge(&world, HERDING_DISCOVERY_ID) > 0.0);
        assert_eq!(
            knowledge(&world, CULTIVATION_DISCOVERY_ID),
            0.0,
            "hunting must not teach Cultivation"
        );
        assert_eq!(
            knowledge(&world, SEED_SELECTION_DISCOVERY_ID),
            0.0,
            "hunting must not teach Seed Selection"
        );

        // Foraging a wild patch teaches Cultivation and touches NEITHER animal knowledge.
        let (mut world, _) = world_with_source(CAP);
        let tile = world.resource::<TileRegistry>().tiles[0];
        forage_one_turn(&mut world, tile, 0.5);
        assert!(knowledge(&world, CULTIVATION_DISCOVERY_ID) > 0.0);
        assert_eq!(
            knowledge(&world, HERDING_DISCOVERY_ID),
            0.0,
            "foraging must not teach Herding"
        );
        assert_eq!(
            knowledge(&world, PENNING_DISCOVERY_ID),
            0.0,
            "foraging must not teach Penning"
        );
    }
}
