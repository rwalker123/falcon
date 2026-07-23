use super::*;

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
///   mirror of the Hunt take. Out of range / module-less / unseeded → 0 this turn, assignment kept.
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
    mut inventory: ResMut<FactionInventory>,
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
    let hunt = &fauna.hunt;
    let husbandry = &fauna.husbandry;
    let market = &fauna.market;
    let work_range = labor.band_work_range;
    let hunt_reach = labor.hunt_reach();
    // The forward-projection horizon for each source's steady `realized` yield: `realized` is the
    // average food/turn the source will deliver over the next N turns, simulated forward from its
    // current (pre-take) state, so the headline "Food /turn" is smooth and the assign-time seed matches
    // the first resolved value exactly.
    let realized_horizon = labor.yield_average_horizon_turns;
    // The horizon for each source's discrete **arrival schedule** — what lands on each of the next N
    // turns, from the same forward simulation run WITH the kill-credit bank. Its own lever: a
    // schedule is a display span the client charts, where `realized_horizon` is a smoothing window.
    let arrivals_horizon = labor.arrivals_horizon_turns;
    // **The ladder's knowledge dials (§4)** — the per-turn accrual every teaching rung pays, and the
    // ledger bar at which a faction may act on a knowledge. Hoisted out of the per-cohort loop.
    // **One pair for BOTH webs**: these used to be duplicated at identical values in
    // `labor_config.forage.cultivation` and `fauna_config.husbandry`, back when each web had its own
    // hard-coded earn site. The earn path is one rung-driven seam now, so the dials live on the
    // ladder with the build dials — the plant and animal ladders can only be paced together.
    let knowledge_delta = scalar_from_f32(ladder.knowledge.progress_per_turn);
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
    let pen_build_rate =
        pen_rung.build_accrual(FollowPolicy::Corral, true, RUNG_TIMESCALE_UNSCALED);
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
        allocation.normalize(available);
        if allocation.assignments.is_empty() {
            continue;
        }
        let faction = cohort.faction;
        let Ok(band_pos) = tiles.get(cohort.current_tile).map(|tile| tile.position) else {
            continue;
        };
        // Productivity modifier stack (wellbeing): scale every yield by the band's output
        // multiplier at PAYOUT. One call — future modifiers slot into `output_multiplier`.
        let mult = output_multiplier(&cohort, &wellbeing);
        let mult_f = mult.to_f32();

        let mut lapsed: Vec<usize> = Vec::new();
        // Retained per-source yield telemetry (derived, not persisted): one entry per assignment in
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
            match &assignment.target {
                LaborTarget::Forage {
                    tile,
                    policy,
                    species,
                } => {
                    // Out of range this turn → no yield, but keep the assignment (the band may
                    // move back into range).
                    if crate::grid_utils::hex_distance_wrapped(
                        band_pos,
                        *tile,
                        grid_width,
                        wrap_horizontal,
                    ) > work_range
                    {
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
                    let sow_permitted = matches!(policy, FollowPolicy::Sow)
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
                    let committing = matches!(policy, FollowPolicy::Cultivate | FollowPolicy::Sow)
                        .then(|| {
                            let rung = if matches!(policy, FollowPolicy::Sow) {
                                RungKey::PlantField
                            } else {
                                RungKey::PlantTended
                            };
                            tiles.get(tile_entity).ok().and_then(|ground| {
                                let composition =
                                    tile_flora_composition(&flora, &labor.forage, ground);
                                resolve_committed_species(
                                    species.as_deref(),
                                    &composition,
                                    &flora,
                                    rung,
                                )
                                .ok()
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
                    // (concentration + conversion) when the improvement *completes* — while the crew
                    // is still clearing, the stand is still the basket it started as.
                    if let Some(chosen) = committing.as_deref() {
                        patch.commit_species(chosen);
                    }
                    // **THE earn path (§4): practising rung N teaches the knowledge that unlocks rung N+1.**
                    // One call, driven entirely by the rung the patch *currently stands on* — a wild
                    // patch teaches **Cultivation**, a tended one **Seed Selection** — so the lesson
                    // is a property of the source's rung, not of the verb. The old hard-coded
                    // `Sustain && Thriving → CULTIVATION_DISCOVERY_ID` branch is gone: `earns_knowledge`
                    // was declarative when slice 2 landed it, and this is where it goes live.
                    //
                    // Knowledge is all that is earned here — working a patch never *tames* it:
                    // cultivation is an explicit `Cultivate` policy with an investment cost (below).
                    // The seam owns the §4.2 stewardship rule (Surplus/Market/Eradicate teach
                    // nothing); `eligible` carries the health gate — **you learn from a healthy
                    // source** — which is the shipped `Thriving` requirement, unchanged.
                    if let Some(knowledge) = patch_rung(patch, &ladder)
                        .knowledge_earned(*policy, patch.ecology_phase == EcologyPhase::Thriving)
                    {
                        discovery.add_progress(faction, knowledge, knowledge_delta);
                    }
                    // **The steady headline** — the forward-projected average food/turn over the next
                    // `realized_horizon` turns, computed from the patch's PRE-take state (before either
                    // branch draws it down), so it equals the assign-time seed exactly. Both the Field
                    // and the drawn-down branches record this one value.
                    let forage_realized = crate::forage::project_realized_forage(
                        patch,
                        &labor.forage,
                        &flora,
                        &ladder,
                        seasonal,
                        mult_f,
                        workers,
                        *policy,
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
                        let production = field_provisions(patch, &labor.forage, &flora, mult_f);
                        // **Collection**: what the crew can carry home — the *same* per-worker
                        // throughput a wild gather is capped by, at the seasonless managed weight (a
                        // Field's crop stands where you planted it). Rung 3 collapses the policy axis;
                        // it does NOT excuse you from the harvest. One worker used to collect the
                        // whole Field however rich it was.
                        let collection = workers as f32
                            * managed_per_worker_yield(patch, &labor.forage, &flora, mult_f);
                        let provisions = scalar_from_f32(production.min(collection));
                        if provisions > scalar_zero() {
                            cohort.stores.add(FOOD, provisions);
                        }
                        let paid = provisions.to_f32();
                        // **The FODDER account (Flora Roster F3, §5.1).** The *same* managed harvest,
                        // routed by the yield vector's fodder component instead of its provisions
                        // component — a grain Field's `field_fodder` is `0` (its crop pays no fodder),
                        // a hay Field's `field_provisions` is `0` (hay is no food), so this is
                        // commodity-generic with **no role branch**. The crew carries hay home at the
                        // same throughput it carries grain, so the collection cap is
                        // `managed_per_worker_fodder`. Credited to the same `FODDER` `LocalStore` key,
                        // which round-trips through the snapshot for free.
                        let fodder_production = field_fodder(patch, &labor.forage, &flora, mult_f);
                        let fodder_collection = workers as f32
                            * managed_per_worker_fodder(patch, &labor.forage, &flora, mult_f);
                        let fodder = scalar_from_f32(fodder_production.min(fodder_collection));
                        if fodder > scalar_zero() {
                            cohort.stores.add(FODDER, fodder);
                            band_fodder_inflow += fodder.to_f32();
                        }
                        // **The arrival schedule — computed POST-take, unlike `realized`.** It
                        // answers "when does the next food land", so it must start from the state the
                        // turn leaves behind: projecting from the pre-take state would re-promise the
                        // delivery this turn has already paid (and, on a hunt, spend the kill-credit
                        // bank twice). Slot 0 is therefore genuinely the *next* turn's delivery.
                        let arrivals = crate::forage::project_arrivals_forage(
                            patch,
                            &labor.forage,
                            &flora,
                            &ladder,
                            seasonal,
                            mult_f,
                            workers,
                            *policy,
                            arrivals_horizon,
                        );
                        yields[idx] = SourceYield {
                            actual: paid,
                            // A managed harvest never draws the stock down, so it can never overdraw.
                            sustainable: paid,
                            // The forward-projected steady headline (computed pre-take above).
                            realized: forage_realized,
                            arrivals,
                            // The crop the crew could not carry: it stood in the field and rotted.
                            // The understaffing signal — "add hands here" — and the reason a rich
                            // Field is a real labor sink rather than a free ration.
                            wasted: (production - paid).max(0.0),
                            workers_needed: workers_needed_for_take(
                                paid,
                                managed_per_worker_yield(patch, &labor.forage, &flora, mult_f),
                                workers,
                            ),
                            // A managed rung-3 harvest cannot overdraw — no ⚠, whatever the policy.
                            overdraws: false,
                        };
                        continue;
                    }
                    let biomass_before = patch.biomass;
                    let provisions = forage_take(
                        patch,
                        workers,
                        *policy,
                        &labor.forage,
                        &flora,
                        &ladder,
                        mult_f,
                        seasonal,
                    );
                    let take = biomass_before - patch.biomass;
                    if provisions > scalar_zero() {
                        cohort.stores.add(FOOD, provisions);
                    }
                    // **Cultivate — the investment policy.** The crew is clearing and planting, not
                    // gathering: `forage_take` above already paid only the reduced Cultivate ceiling
                    // (the rung's `yield_fraction_while_building × MSY` — the up-front cost), and here the patch
                    // accrues toward becoming a tended crop. Gates: the faction must **know
                    // Cultivation** (earned by Sustain-foraging, above) and the patch must be
                    // **Thriving**. If a gate lapses mid-run (e.g. another band overdraws the patch to
                    // Stressed) progress simply **stops accruing that turn** — it is neither lost nor
                    // silently switched; the patch is still marked worked below, so it doesn't decay
                    // either, and accrual resumes when the patch recovers.
                    //
                    // **Ordering: accrue AFTER the take.** The patch pays this turn per its state at
                    // the *start* of the turn, so the pre-commit forecast the client showed is exactly
                    // what the sim paid (forecast == actual). The turn progress reaches `1.0` is the
                    // last preparing take; the full tended yield starts the next turn.
                    if matches!(policy, FollowPolicy::Cultivate) {
                        // Marked worked-as-improvement so `advance_cultivation` spares it: a patch
                        // under active preparation neither goes feral nor bleeds its partial progress.
                        patch.tended_this_turn = true;
                        // The rung's own gates, resolved for the engine: the faction must know the
                        // rung's unlock knowledge (Cultivation) and the patch must be Thriving.
                        let eligible = tended_rung.unlock_discovery_id().is_none_or(|knowledge| {
                            knows(&discovery, faction, knowledge, knowledge_threshold)
                        }) && patch.ecology_phase == EcologyPhase::Thriving
                            // **Nothing to tend if nothing here climbs.** A patch with no committed
                            // plant is one whose basket the tended rung's `cultivation_ceiling`
                            // refuses outright — the "not every plant climbs" ruling reaching the
                            // build meter.
                            && patch.species.is_some();
                        // THE build seam: the rung supplies the accrual (0 unless Cultivate is the
                        // rung's verb and the gates hold); the patch owns its meter and the
                        // side-effects of completing it.
                        let accrual =
                            tended_rung.build_accrual(*policy, eligible, RUNG_TIMESCALE_UNSCALED);
                        if accrual > 0.0 {
                            patch.accrue_cultivation(faction, accrual);
                            if patch.is_cultivated() {
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
                    }
                    // **Sow — the rung-3 investment policy**, the twin of Cultivate above and the
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
                    if matches!(policy, FollowPolicy::Sow) {
                        // Marked worked-as-improvement so `advance_cultivation` spares it: a patch
                        // under active preparation neither goes feral nor bleeds its partial progress.
                        patch.tended_this_turn = true;
                        accrue_field(
                            patch,
                            field_rung,
                            *policy,
                            sow_permitted,
                            faction,
                            &mut event_log,
                            tick.0,
                            *tile,
                        );
                    }
                    // Market forage = gathered goods sold: convert the raw take to trade goods
                    // (mirror of the Hunt-Market arm). Only Market sells — Sustain/Surplus/Eradicate
                    // produce no trade goods (Eradicate is denial, not commerce).
                    if matches!(policy, FollowPolicy::Market) {
                        let forage_market = &labor.forage.market;
                        let trade_goods = (take
                            * forage_market.trade_goods_per_biomass
                            * forage_market.trade_goods_multiplier
                            * mult_f)
                            .round() as i64;
                        if trade_goods > 0 {
                            inventory.add_stockpile(faction, "trade_goods", trade_goods);
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
                    ) * labor.forage.provisions_per_biomass
                        * mult_f;
                    // The two staffing signals, from the same take. **Overstaffing**: invert the take
                    // by the **effective** per-worker throughput this turn (`per_worker_biomass_capacity
                    // × seasonal`, matching `forage_take`'s worker cap) so a labor-bound low-season
                    // patch isn't falsely flagged. **Understaffing** (`wasted`): what the policy
                    // ceiling offered beyond what the crew could gather — here it is not lost, it
                    // simply stays in the stock and regrows, but it is the same "add hands" answer.
                    let per_worker_biomass = forage_per_worker_biomass(&labor.forage, seasonal);
                    let workers_needed = workers_needed_for_take(take, per_worker_biomass, workers);
                    let production = forage_policy_ceiling(
                        *policy,
                        biomass_before,
                        patch.carrying_capacity,
                        &patch_ecology(patch, &labor.forage),
                        &labor.forage,
                        &ladder,
                    )
                    .clamp(0.0, biomass_before);
                    // **The arrival schedule — computed POST-take, unlike `realized`.** It
                    // answers "when does the next food land", so it must start from the state the
                    // turn leaves behind: projecting from the pre-take state would re-promise the
                    // delivery this turn has already paid (and, on a hunt, spend the kill-credit
                    // bank twice). Slot 0 is therefore genuinely the *next* turn's delivery.
                    let arrivals = crate::forage::project_arrivals_forage(
                        patch,
                        &labor.forage,
                        &flora,
                        &ladder,
                        seasonal,
                        mult_f,
                        workers,
                        *policy,
                        arrivals_horizon,
                    );
                    yields[idx] = SourceYield {
                        actual: provisions.to_f32(),
                        sustainable,
                        // The forward-projected steady headline (computed pre-take above).
                        realized: forage_realized,
                        arrivals,
                        wasted: forage_provisions(
                            (production - take).max(0.0),
                            patch_provisions_per_biomass(patch, &flora, &labor.forage),
                            mult_f,
                        ),
                        workers_needed,
                        // Plants stay flow-based (slice 8), so the wild/tended gather ⚠ is unchanged:
                        // Sustain/Cultivate/Sow take the MSY or a dip on it, Surplus/Market/Eradicate
                        // draw the patch down.
                        overdraws: policy.overdraws(),
                    };
                }
                LaborTarget::Hunt { fauna_id, policy } => {
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
                    // **The steady headline** — the forward-projected average food/turn over the next
                    // `realized_horizon` turns, computed from the herd's PRE-take state (before the pen
                    // feed/harvest or the wild take mutates it), so it equals the assign-time seed
                    // exactly. Rate-based (no kill-credit bank), so it is smooth where `actual` pulses;
                    // a corralled herd projects its managed pen yield instead. Both the pen-tend and the
                    // wild-take branches record this one value.
                    let hunt_realized = fauna::project_realized_hunt(
                        herd,
                        &fauna,
                        &ladder,
                        labor.hunt.per_worker_biomass_capacity,
                        mult_f,
                        workers,
                        *policy,
                        realized_horizon,
                    );
                    // **THE earn path (§4)** — the exact mirror of the Forage arm's call, and the
                    // heart of this ladder: the lesson is read off **the rung this herd stands on**,
                    // so the *same* Sustain hunt teaches **Herding** on a wild herd and **Penning** on
                    // a tamed one ("you learn herding by managing wild herds; penning by managing
                    // tamed ones"). The old hard-coded `Sustain && Thriving → HERDING_DISCOVERY_ID`
                    // branch is retired; `earns_knowledge` drives it now.
                    //
                    // **Resolved BEFORE the rung branches below** (the corral tend arm `continue`s,
                    // and the take arm draws biomass), so *every* rung reaches the earn path
                    // uniformly — including the pen, whose `earns_knowledge` is null today but is
                    // where rung 4's `selective_breeding` will hang. Moving it ahead of the take is
                    // behaviour-neutral: `ecology_phase` is written only by `refresh_ecology_phase`
                    // in Logistics, never by a take, so the gate reads the same value either side.
                    //
                    // The two webs cannot cross-teach (§4.2) for free: a herd resolves to an `animal`
                    // rung, so only an animal knowledge is reachable from here.
                    if let Some(knowledge) = fauna::herd_rung(herd, &ladder)
                        .knowledge_earned(*policy, herd.ecology_phase == EcologyPhase::Thriving)
                    {
                        discovery.add_progress(faction, knowledge, knowledge_delta);
                    }
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
                    let herders_needed = fauna::herd_herders_needed(herd, &fauna);
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
                            fauna::quantise_animal_take(production, collection, herd.body_mass);
                        herd.biomass -= take.killed_biomass();
                        let provisions =
                            scalar_from_f32(hunt_provisions(take.carried, &fauna, mult_f));
                        if provisions > scalar_zero() {
                            cohort.stores.add(FOOD, provisions);
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
                        // delivery this turn has already paid, and would spend the herd's kill-credit
                        // bank twice. Slot 0 is therefore genuinely the *next* turn's delivery.
                        let arrivals = fauna::project_arrivals_hunt(
                            herd,
                            &fauna,
                            &ladder,
                            labor.hunt.per_worker_biomass_capacity,
                            mult_f,
                            workers,
                            *policy,
                            arrivals_horizon,
                        );
                        yields[idx] = SourceYield {
                            actual: tended,
                            sustainable: tended,
                            // The forward-projected steady headline (computed pre-take above; a pen
                            // projects its managed yield, already smooth).
                            realized: hunt_realized,
                            arrivals,
                            wasted: hunt_provisions(take.wasted, &fauna, mult_f),
                            // **ONE CREW doing both jobs** ([`managed_crew_needed`]): big enough to
                            // mind the heads *and* to haul the meat. The haul side is the **steady
                            // peak-drop carry crew** ([`fauna::hunt_haul_workers`]) off the pen's
                            // per-turn `production`, NOT this turn's lumpy `take.carried` — a slow-
                            // breeding pen (the aurochs pulses) drops 0 animals on a wait turn, which
                            // would collapse the crew to the herder count and contradict `wasted`.
                            workers_needed: managed_crew_needed(
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
                    // The band has no carry room — it eats/banks whatever it hauls, so pass an
                    // unbounded carry cap (behaviour unchanged from before the expedition clamp).
                    let take = hunt_take(
                        herd,
                        workers,
                        *policy,
                        labor.hunt.per_worker_biomass_capacity,
                        &fauna,
                        &ladder,
                        f32::INFINITY,
                    );
                    let provisions = scalar_from_f32(hunt_provisions(take.carried, &fauna, mult_f));
                    // **Tame — the investment policy** (the animal twin of Cultivate, and the rung
                    // below Corral). The crew is gentling the herd, not hunting it: `hunt_take`
                    // above already paid only the reduced Tame ceiling (the `animal:pastoral` rung's
                    // `yield_fraction_while_building × MSY` — the up-front cost), and here the herd
                    // accrues toward pastoral. Gates: the faction must **know Herding** (earned by
                    // Sustain-hunting, above), the species' husbandry ceiling must allow taming
                    // (Grazing 2d-δ — a `wild`-ceiling species never tames; `accrue_domestication`
                    // self-guards too, and the command path rejects it, so this is belt and braces),
                    // and the herd must be **Thriving**. A gate that lapses mid-run just stops
                    // accrual that turn — progress is neither lost nor silently switched, and the
                    // herd is marked tamed-this-turn below so it doesn't decay either.
                    //
                    // **Ownership is NOT in `eligible`** — `accrue_domestication` owns the
                    // `owner is None || owner == faction` rule (and sets ownership on first accrual),
                    // exactly as `accrue_cultivation` owns it on the plant side. One rule, one place.
                    //
                    // **Ordering: accrue AFTER the take** (mirrors Cultivate/Corral), so this turn
                    // pays exactly the dipped yield the pre-commit forecast promised.
                    if matches!(policy, FollowPolicy::Tame) {
                        // Marked worked-as-improvement so `advance_husbandry` spares it: a herd
                        // under active taming neither goes feral nor bleeds its partial progress.
                        herd.tamed_this_turn = true;
                        let eligible =
                            pastoral_rung.unlock_discovery_id().is_none_or(|knowledge| {
                                knows(&discovery, faction, knowledge, knowledge_threshold)
                            }) && herd.can_domesticate()
                                && herd.ecology_phase == EcologyPhase::Thriving;
                        // THE build seam — the same call the plant side's Cultivate arm makes, at
                        // **this species' own taming timescale** (slice 3c): the rung owns the
                        // mechanic, the species scales it (rabbit ×1.0 → 25 turns, Steppe Runner ×0.2
                        // → 125). The seam applies the multiplier to the decay too, so a herd that is
                        // slow to tame is equally slow to forget — see `RungDef::build_accrual`.
                        let accrual = pastoral_rung.build_accrual(
                            *policy,
                            eligible,
                            fauna.taming_rate_for(&herd.species),
                        );
                        if accrual > 0.0 {
                            herd.accrue_domestication(faction, accrual);
                            if herd.is_domesticated() {
                                event_log.push(CommandEventEntry::new(
                                    tick.0,
                                    CommandEventKind::Tame,
                                    faction,
                                    format!("Tamed the {} herd", herd.species),
                                    Some(format!("status=complete action=tame herd={}", herd.id)),
                                ));
                            }
                        }
                    }
                    // **Corral — the investment policy** (the animal twin of Cultivate). The crew is
                    // building the pen, not hunting: `hunt_take` above already paid only the reduced
                    // Corral ceiling (the rung's `yield_fraction_while_building × MSY` — the up-front
                    // cost), and here the pen accrues. Gates: the faction must **know Penning** (the
                    // rung's own `unlock_knowledge` — Herding gates `tame` alone since §4.3) and **own a
                    // domesticated herd**. A gate that lapses mid-build just stops accrual that turn
                    // (progress is kept — a half-built pen is materials on the ground). Accrued
                    // **after** the take, so this turn pays exactly what the pre-commit forecast
                    // promised; the corral yield starts the turn after the pen completes.
                    if matches!(policy, FollowPolicy::Corral) {
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
                        // 3c): a fence is a fence.
                        let accrual =
                            pen_rung.build_accrual(*policy, eligible, RUNG_TIMESCALE_UNSCALED);
                        if accrual > 0.0 {
                            let pen_tile = herd.position();
                            if herd.accrue_corral(faction, accrual, pen_tile) {
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
                    let trade_multiplier = if matches!(policy, FollowPolicy::Market) {
                        market.trade_goods_multiplier
                    } else {
                        1.0
                    };
                    // FOOD income is fully fractional; trade goods stay integer → FactionInventory.
                    // Scaled off the meat actually **carried home**, not the animals killed: you
                    // cannot trade a hide you left on the range.
                    let trade_goods =
                        (take.carried * hunt.trade_goods_per_biomass * trade_multiplier * mult_f)
                            .round() as i64;
                    if provisions > scalar_zero() {
                        cohort.stores.add(FOOD, provisions);
                    }
                    if trade_goods > 0 {
                        inventory.add_stockpile(faction, "trade_goods", trade_goods);
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
                    let sustainable = sustainable_yield(
                        biomass_before,
                        herd_capacity(herd, &fauna),
                        &herd_ecology(herd, &fauna),
                    ) * hunt.provisions_per_biomass
                        * mult_f;
                    // The two staffing signals, from the same take. **Overstaffing**: invert the
                    // carried biomass by the per-hunter throughput (hunt has no seasonal factor,
                    // unlike forage). **Understaffing** (`wasted`): the meat the crew killed but could
                    // not haul — **a real loss**, left to rot on the range. Measured against the
                    // animals *slaughtered*, never against the escapement the herd could have spared:
                    // an animal nobody killed is still alive out there, so it was never produced and
                    // cannot have been wasted (`fauna::forecast_production_and_take`).
                    //
                    // **A MANAGED herd reports its whole CREW** ([`managed_crew_needed`]) — the
                    // herders who mind it are the ones who take from it, and the crew must be big
                    // enough for both jobs. A **wild** herd is untouched by the herder term:
                    // `herders_needed` is `0` (it isn't yours to maintain), so the `max` collapses to
                    // the haul-side count.
                    //
                    // The haul side is the **steady peak-drop carry crew** ([`fauna::hunt_haul_workers`])
                    // off the policy's steady per-turn rate — the SAME `hunt_policy_rate` the take path
                    // banked — NOT this turn's lumpy `take.carried`. A slow breeder whose MSY <
                    // `body_mass` carries `0` on a wait turn, which would collapse `workers_needed` and
                    // contradict `wasted_yield`; the steady crew equals the client's max-useful count, so
                    // the overstaff note is stable across wait/kill turns. The rate reads the herd's own
                    // ecology/capacity + pre-regrowth biomass, unchanged by the take above, so it matches
                    // what `hunt_take` used.
                    let rate = fauna::hunt_policy_rate(
                        *policy,
                        herd.biomass_before_regrowth,
                        herd_capacity(herd, &fauna),
                        &herd_ecology(herd, &fauna),
                        &fauna,
                        &ladder,
                    );
                    let take_workers = fauna::hunt_haul_workers(
                        rate,
                        herd.body_mass,
                        labor.hunt.per_worker_biomass_capacity,
                    );
                    let workers_needed = managed_crew_needed(herders_needed, take_workers);
                    // **The arrival schedule — computed POST-take, unlike `realized`.** It
                    // answers "when does the next food land", so it must start from the state the
                    // turn leaves behind: projecting from the pre-take state would re-promise the
                    // delivery this turn has already paid, and would spend the herd's kill-credit
                    // bank twice. Slot 0 is therefore genuinely the *next* turn's delivery.
                    let arrivals = fauna::project_arrivals_hunt(
                        herd,
                        &fauna,
                        &ladder,
                        labor.hunt.per_worker_biomass_capacity,
                        mult_f,
                        workers,
                        *policy,
                        arrivals_horizon,
                    );
                    yields[idx] = SourceYield {
                        actual: provisions.to_f32(),
                        sustainable,
                        wasted: hunt_provisions(take.wasted, &fauna, mult_f),
                        workers_needed,
                        overdraws: policy.overdraws(),
                        // The forward-projected steady headline (computed pre-take above): rate-based,
                        // so it is smooth where `actual` (the whole-animal kill) pulses.
                        realized: hunt_realized,
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
                    // Inert in Phase 0. Warriors are a band-wide standing guard (border/camp patrol),
                    // not a hunting escort — they do **not** mitigate hunt danger (the hunting party
                    // answers that itself, via its own equipment). Their first live consumer is the
                    // **Phase 1 predator-raid path**: a carnivore with `aggression > 0` raiding a band,
                    // band as Defender. Do not delete this branch.
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
        // Drop lapsed hunts (reverse order to keep indices valid); workers return to the pool.
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

/// **The crew a MANAGED (pastoral or penned) source needs**: `max(herders, take)` — one crew, sized by
/// whichever of its two jobs binds.
///
/// # ONE need, not two — but "one need" means one CREW, not one formula
///
/// The same people mind the herd *and* slaughter it, so a managed source reports **one** number and
/// staffs **one** team. But that team has to be big enough to do **both** jobs, and the two jobs scale
/// on **different units**:
///
/// ```text
/// herding = per HEAD     — one herder minds 12 aurochs   (animals_per_herder)
/// hauling = per BIOMASS  — one hauler carries 40 biomass  (per_worker_biomass_capacity)
/// ```
///
/// **That asymmetry is real and must not be "simplified" away.** A shepherd minds ~300 sheep and could
/// not carry three of them; an aurochs herder minds 12 head (960 biomass) but hauls 40. So neither
/// count dominates the other across the roster — small-bodied species are herder-bound (a fowl pen
/// needs 13 pairs of eyes for 3.3 provisions), big-bodied ones are haul-bound (one aurochs herder
/// would leave half the pen rotting). `max()` is what makes the answer true in both regimes.
///
/// **Reporting only the herder count made the UI contradict itself**: `workersNeeded: 1` beside
/// `wastedYield: 0.80` — *drop workers* and *add workers*, at the same time, on the same row. Two
/// separate *needs* (staff a herding team **and** a butchering team) is what was ruled out; this is not
/// that. `+` would be two teams; `max` is one crew that can cover its busiest job.
///
/// **Wild hunting is untouched by construction**: a wild herd isn't yours to maintain, so
/// `fauna::herd_herders_needed` is `0` and this collapses to the take-side count — the shipped
/// behaviour, verbatim. (`hunt = reach + carry`; `harvest = maintain + take`.)
fn managed_crew_needed(herders_needed: u32, take_workers: u32) -> u32 {
    herders_needed.max(take_workers)
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
#[allow(clippy::too_many_arguments)] // the rung, the gate, the actor and the feed line are all inputs
fn accrue_field(
    patch: &mut ForagePatch,
    field_rung: &RungDef,
    policy: FollowPolicy,
    eligible: bool,
    faction: FactionId,
    event_log: &mut CommandEventLog,
    tick: u64,
    tile: UVec2,
) {
    let accrual = field_rung.build_accrual(policy, eligible, RUNG_TIMESCALE_UNSCALED);
    if accrual <= 0.0 {
        return;
    }
    patch.accrue_field(faction, accrual);
    if patch.is_field() {
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
    }
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
    use crate::components::{
        FollowPolicy, LaborAllocation, LaborAssignment, LaborTarget, LocalStore, MoraleCause,
        PopulationCohort, SourceYield, Tile,
    };
    use crate::fauna::hunt_provisions;
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
    use crate::wellbeing_config::WellbeingConfigHandle;
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

        // Depletable forage patch on the source tile, seeded at half its carrying capacity so a
        // Sustain gather draws a clear (positive) regrowth skim (`forage.actual > 0`).
        let forage_cfg = world.resource::<LaborConfigHandle>().get();
        let patch_cap = forage_cfg.forage.capacity_for(SOURCE_BIOME);
        let mut patch = ForagePatch::new(UVec2::new(0, 0), patch_cap);
        patch.biomass = patch_cap * 0.5;
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
                        policy: FollowPolicy::Sustain,
                        species: None,
                    },
                    workers: WORKERS,
                },
                LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: HERD_ID.to_string(),
                        policy: FollowPolicy::Sustain,
                    },
                    workers: WORKERS,
                },
            ],
        );

        // Expected hunt sustainable = one turn's net regrowth at the PRE-take biomass, in provisions
        // (output multiplier is 1.0 at morale 1).
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let expected_sustainable =
            sustainable_yield(start, CAP, &fauna.ecology) * fauna.hunt.provisions_per_biomass;
        drop(fauna);

        // **Seed the kill-credit bank to one body** (slice 8b) so turn one lands a whole animal. This
        // test is about a Hunt row *capturing its yield telemetry* alongside a Forage row; a fresh
        // bank would correctly *wait* for MSY (1.25) to accumulate to a body (5) — that pulse is
        // `sustain_hunt_at_capacity_yields_msy`'s subject, not this one's.
        {
            let mut registry = world.resource_mut::<HerdRegistry>();
            let herd = registry
                .herds
                .iter_mut()
                .find(|h| h.id == HERD_ID)
                .expect("seeded herd");
            herd.hunt_credit = herd.body_mass;
        }

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
                    policy: FollowPolicy::Eradicate,
                },
                workers: WORKERS,
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
    /// (logistic regrowth is 0 at K), leaving a full herd stuck. The MSY-based `sustainable_yield`
    /// ceiling skims regrowth at the most-productive biomass (K/2), so a full herd stays
    /// sustainably huntable — the parity fix mirroring the forage full-patch case.
    #[test]
    fn sustain_hunt_at_capacity_yields_msy() {
        let start = CAP; // full herd — the old net_biomass_delta(K) == 0 bug.
        let (mut world, tile) = world_with_source(start);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    policy: FollowPolicy::Sustain,
                },
                workers: WORKERS,
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
        // **RETARGETED FOR THE KILL-CREDIT MODEL (slice 8b): a full herd yields MSY GENTLY, no burst.**
        // The escapement *burst* (this used to assert `actual > sustainable`, cropping a full herd to
        // `K/2` in one turn) is gone: Sustain banks its MSY rate into `hunt_credit` and pays a whole
        // animal only when the bank clears one body. The fixture's MSY (1.25) is well under one body
        // (5), so turn one is a **wait** (`actual == 0`) and a kill lands every ~4 turns.
        //
        // So the test now pins the two properties the credit model guarantees: **(1) no burst** — the
        // herd is not slashed to `K/2` on turn one; **(2) the long-run take is MSY** — averaged over
        // enough turns to contain the pulses. (No `advance_herds` here, so the herd does not regrow;
        // the standing surplus above `K/2` is what the MSY draw comes out of, gently.)
        assert!(
            hunt.actual < hunt.sustainable + 1e-4,
            "no burst: turn one takes AT MOST the MSY rate, never the whole `B − K/2` surplus: {hunt:?}"
        );
        assert!(
            world
                .resource::<HerdRegistry>()
                .find(HERD_ID)
                .unwrap()
                .biomass
                > start * 0.9,
            "no burst: a full herd is not cropped toward K/2 in one turn (still ~full)"
        );

        // Average take over many turns == the MSY rate (the pulses wash out).
        // 6 whole pulse cycles (body/MSY = 4 turns each), and the herd stays above K/2 throughout
        // (24 × MSY = 30 killed, 100 → 70), so the rate is a full MSY on every one.
        const AVG_TURNS: u32 = 24;
        let mut total = hunt.actual;
        for _ in 1..AVG_TURNS {
            world.run_system_once(advance_labor_allocation);
            total += world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual;
        }
        let avg = total / AVG_TURNS as f32;
        assert!(
            (avg - expected_sustainable).abs() < expected_sustainable * 0.1,
            "the long-run Sustain take averages MSY ({expected_sustainable}), realised as a kill every \
             few turns: got {avg}"
        );
        let last = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();
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

    /// Switch a band's (single) Forage assignment to `policy` — what the client's picker does, and
    /// what a player does the turn an improvement finishes and they want to start harvesting it.
    fn set_forage_policy(world: &mut World, band: Entity, policy: FollowPolicy) {
        let mut allocation = world
            .get_mut::<LaborAllocation>(band)
            .expect("band forages");
        let assignment = allocation
            .assignments
            .iter_mut()
            .find(|assignment| matches!(assignment.target, LaborTarget::Forage { .. }))
            .expect("a Forage assignment");
        let LaborTarget::Forage {
            policy: current, ..
        } = &mut assignment.target
        else {
            unreachable!("filtered to Forage above");
        };
        *current = policy;
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
    fn forage_workers_needed(policy: FollowPolicy) -> u32 {
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
                    policy,
                    species: None,
                },
                workers: WORKERS,
            }],
        );
        world.run_system_once(advance_labor_allocation);
        world.get::<LaborAllocation>(band).unwrap().last_yields[0].workers_needed
    }

    /// Overstaffing: a Sustain hunt whose take is set by the regrowth (MSY) ceiling — not labor —
    /// needs a **single** worker even with 5 assigned, so `workers_needed == 1 < assigned`.
    #[test]
    fn sustain_source_overstaffed_reports_one_worker_needed() {
        // **Above the escapement point** (slice 8): `K/2` is exactly where a Sustain hunt spares
        // nothing, so the old `CAP * 0.5` ("half cap → clear positive MSY skim" — a flow-model
        // reading) now seeds the one biomass at which this test's premise cannot hold.
        let (mut world, tile) = world_with_source(CAP * 0.9);
        let assigned = 5;
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    policy: FollowPolicy::Sustain,
                },
                workers: assigned,
            }],
        );

        // Seed the kill-credit bank to one body so this turn lands a whole animal (slice 8b) — the
        // point here is the *overstaffing* readout on a real take, not the accumulation wait.
        {
            let mut registry = world.resource_mut::<HerdRegistry>();
            let herd = registry
                .herds
                .iter_mut()
                .find(|h| h.id == HERD_ID)
                .expect("seeded herd");
            herd.hunt_credit = herd.body_mass;
        }

        world.run_system_once(advance_labor_allocation);

        let hunt = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();
        assert!(
            hunt.actual > 0.0,
            "the sustain hunt produced food: {hunt:?}"
        );
        assert_eq!(
            hunt.workers_needed, 1,
            "one animal's throughput needs a single worker: {hunt:?}"
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
        let eradicate_fraction = cfg.forage.eradicate.take_fraction;
        drop(cfg);
        set_wild_patch_biomass(&mut world, patch_cap); // full patch.
        let assigned = 2;
        // The scenario is labor-bound iff worker throughput is below the Eradicate biomass ceiling.
        assert!(
            assigned as f32 * capacity < eradicate_fraction * patch_cap,
            "test precondition: the take must be labor-bound, not ceiling-bound"
        );
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: UVec2::new(0, 0),
                    policy: FollowPolicy::Eradicate,
                    species: None,
                },
                workers: assigned,
            }],
        );

        world.run_system_once(advance_labor_allocation);

        let forage = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();
        assert_eq!(
            forage.workers_needed, assigned,
            "a labor-bound take needs every assigned worker: {forage:?}"
        );
    }

    /// A higher-take policy needs more workers on the **same** resource: Market/Eradicate draw a large
    /// biomass fraction, so their inverted worker count exceeds Sustain's MSY skim on identical full
    /// patches.
    #[test]
    fn market_and_eradicate_need_more_workers_than_sustain() {
        let sustain = forage_workers_needed(FollowPolicy::Sustain);
        let market = forage_workers_needed(FollowPolicy::Market);
        let eradicate = forage_workers_needed(FollowPolicy::Eradicate);
        assert!(
            market > sustain,
            "market's larger take needs more workers: {market} vs {sustain}"
        );
        assert!(
            eradicate > sustain,
            "eradicate's larger take needs more workers: {eradicate} vs {sustain}"
        );
        assert!(
            eradicate >= market,
            "eradicate's ceiling is ≥ market's: {eradicate} vs {market}"
        );
    }

    /// A tended (cultivated) patch and a corralled herd both pay out, and each reports an honest
    /// staffing need — **but they no longer report the same KIND of need**, and that is the point.
    ///
    /// The name's original claim (`workers_needed == 1` for both, "maintenance labor, not scaling
    /// gather") is dead twice over: slice 7 retired `TENDED_SOURCE_WORKERS_NEEDED = 1` for the payout,
    /// and slice 8 gave the pen a **standing, herd-sized herder demand**. What the pen reports now is
    /// [`managed_crew_needed`] — **one crew sized by whichever of its two jobs binds**: enough hands to
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
                    policy: FollowPolicy::Sustain,
                    species: None,
                },
                workers: WORKERS,
            }],
        );
        let keeper = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    policy: FollowPolicy::Sustain,
                },
                workers: WORKERS,
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
            let take_biomass = tended.actual / world_labor.forage.provisions_per_biomass;
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
            let per_worker = hunt_provisions(
                world_labor.hunt.per_worker_biomass_capacity,
                &world_fauna,
                1.0,
            );
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

    /// Reseat the harness herd as a **Wild-Aurochs-shaped slow breeder**: a `body_mass` heavier than one
    /// turn's MSY (`r·K/4 = 0.05·100/4 = 1.25 ≪ 80`), so it **pulses** — it drops zero animals on most
    /// turns while its kill-credit banks, then a whole one when the bank clears a body. `credit` seeds
    /// that bank so a test can force a **kill** turn (`credit = body`) or a **wait** turn (`credit = 0`).
    fn reseat_slow_breeder(world: &mut World, biomass: f32, credit: f32) {
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let mut registry = world.resource_mut::<HerdRegistry>();
        let herd = &mut registry.herds[0];
        herd.body_mass = SLOW_BREEDER_BODY;
        herd.carrying_capacity = SLOW_BREEDER_CAP;
        herd.biomass = biomass;
        // These fixtures set biomass directly (no `regrow_biomass`), and the take rate reads
        // `biomass_before_regrowth` (slice 8b) — keep it in sync.
        herd.biomass_before_regrowth = biomass;
        herd.hunt_credit = credit;
        herd.refresh_ecology_phase(&fauna);
    }

    /// One aurochs-shaped body — heavier than one turn's MSY, and heavier than one hauler carries.
    const SLOW_BREEDER_BODY: f32 = 80.0;
    /// The slow breeder's capacity: `MSY = r·K/4 = 1.25`, far below `SLOW_BREEDER_BODY`.
    const SLOW_BREEDER_CAP: f32 = 100.0;
    /// Above the escapement point (`K/2`), so the herd has whole animals to spare once the bank clears.
    const SLOW_BREEDER_BIOMASS: f32 = 90.0;

    /// A single Sustain-hunt turn on the slow breeder with `credit` banked and `workers` assigned;
    /// returns the captured yield row.
    fn slow_breeder_hunt(credit: f32, workers: u32) -> SourceYield {
        let (mut world, tile) = world_with_source(CAP);
        reseat_slow_breeder(&mut world, SLOW_BREEDER_BIOMASS, credit);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    policy: FollowPolicy::Sustain,
                },
                workers,
            }],
        );
        world.run_system_once(advance_labor_allocation);
        world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone()
    }

    /// One hunt turn under `policy` on the slow breeder (biomass above `K/2`, empty bank), staffed so
    /// the worker cap never binds; returns the captured yield row.
    fn slow_breeder_hunt_policy(policy: FollowPolicy) -> SourceYield {
        let (mut world, tile) = world_with_source(CAP);
        reseat_slow_breeder(&mut world, SLOW_BREEDER_BIOMASS, 0.0);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    policy,
                },
                workers: WORKERS,
            }],
        );
        world.run_system_once(advance_labor_allocation);
        world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone()
    }

    /// **The forward-projected `realized` reads the HONEST OVERHUNTING RATE — and sees the decline.**
    /// `sustainable` is the herd's MSY (the overhunting reference), policy-independent. The lumpy
    /// `actual` cannot be compared to it turn by turn (a kill cashes a whole banked animal and spikes
    /// above MSY even under Sustain), which is why `overdraws` exists. The forward-projected `realized`
    /// IS comparable: a **Sustain** hunt projects `≈ sustainable` (stable at K/2), an overhunting
    /// **Surplus/Market** hunt projects *above* it — and, because the projection simulates the herd
    /// declining under the overdraw, **Market projects BELOW its naive `2.5×MSY` steady rate** (the
    /// stock runs out inside the horizon), the honest reading the instantaneous rate could not give.
    #[test]
    fn realized_reads_the_honest_overhunting_rate() {
        let sustain = slow_breeder_hunt_policy(FollowPolicy::Sustain);
        let surplus = slow_breeder_hunt_policy(FollowPolicy::Surplus);
        let market = slow_breeder_hunt_policy(FollowPolicy::Market);

        // `sustainable` is MSY, the same under every policy (it is the reference, not the take).
        assert!(
            (sustain.sustainable - surplus.sustainable).abs() < 1e-6
                && (sustain.sustainable - market.sustainable).abs() < 1e-6,
            "sustainable is the policy-independent MSY reference: {sustain:?} {surplus:?} {market:?}"
        );
        // Sustain projects ~its sustainable MSY (a Sustain hunt holds the herd at K/2 and does not
        // overdraw, so the whole horizon pays MSY).
        assert!(
            (sustain.realized - sustain.sustainable).abs() < 1e-5,
            "a Sustain hunt projects ≈ its sustainable MSY: {sustain:?}"
        );
        // Overhunting projects the honest rate ABOVE the sustainable reference, ordered by policy.
        assert!(
            surplus.realized > surplus.sustainable,
            "Surplus projects above the sustainable MSY (the honest overhunt rate): {surplus:?}"
        );
        assert!(
            market.realized > surplus.realized,
            "Market projects deeper than Surplus: {market:?} {surplus:?}"
        );
        // The projection SEES THE DECLINE: Market drives the herd out within the horizon, so its
        // average is strictly below the naive instantaneous `market_multiplier × MSY` steady rate — the
        // honest reading the old instantaneous rate could not produce. MSY = `sustainable` (both are
        // `hunt_provisions(peak_regrowth)` above K/2), so the naive Market rate is `2.5 × sustainable`.
        let naive_market_rate =
            FaunaConfigHandle::default().get().hunt.market_multiplier * market.sustainable;
        assert!(
            market.realized > 0.0 && market.realized < naive_market_rate,
            "Market projects below its naive {naive_market_rate} steady rate (sees the decline): \
             {market:?}"
        );
    }

    /// **Eradicate reads the STRIP RATE it delivers, NOT a diluted average.** Eradicate strips the herd
    /// in ~1 turn; the projection breaks the moment the source is spent and divides by the turns it
    /// actually delivered, so `realized` reads the high one-shot strip rate — far above Sustain's MSY —
    /// rather than that rate smeared thin across ~40 mostly-empty horizon turns (which would read
    /// *below* Sustain, the exact dilution the divide-by-turns-simulated rule prevents).
    #[test]
    fn eradicate_realized_reads_the_strip_rate_not_a_diluted_average() {
        let sustain = slow_breeder_hunt_policy(FollowPolicy::Sustain);
        let eradicate = slow_breeder_hunt_policy(FollowPolicy::Eradicate);

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

    /// **A hunt's `workers_needed` is its STEADY carry crew — the same on a wait turn and a kill turn.**
    /// The bug: sizing the crew off *this turn's* `take.carried` reads `0` on a slow breeder's wait turn
    /// (MSY < `body_mass`, so nothing drops while the bank fills), collapsing `workers_needed` beside a
    /// `wasted_yield` that says the crew is understaffed — *drop workers* and *add workers* on one row.
    /// The steady peak-drop crew (`ceil(body_mass / per_worker)` here) does not flicker with the pulse,
    /// so the band-panel overstaff note is stable.
    #[test]
    fn a_slow_breeder_hunt_reports_its_steady_carry_crew_on_wait_and_kill_turns() {
        let steady_crew = {
            let per_worker = LaborConfigHandle::default()
                .get()
                .hunt
                .per_worker_biomass_capacity;
            (SLOW_BREEDER_BODY / per_worker).ceil() as u32 // ceil(80 / 40) = 2.
        };
        assert!(
            steady_crew >= 2,
            "the fixture must need more than one hauler, or the wait-turn collapse is invisible"
        );

        // Wait turn (empty bank): the herd spares nothing, but the crew is still the steady carry crew,
        // NOT the old `0`.
        let wait = slow_breeder_hunt(0.0, steady_crew);
        assert_eq!(
            wait.actual, 0.0,
            "a slow breeder waits on an empty bank: {wait:?}"
        );
        assert_eq!(
            wait.workers_needed, steady_crew,
            "the wait-turn crew is the steady carry crew, not the lumpy 0: {wait:?}"
        );

        // Kill turn (one body banked): a whole animal lands, and the crew is UNCHANGED.
        let kill = slow_breeder_hunt(SLOW_BREEDER_BODY, steady_crew);
        assert!(
            kill.actual > 0.0,
            "the banked body lands this turn: {kill:?}"
        );
        assert_eq!(
            kill.workers_needed, steady_crew,
            "the kill-turn crew equals the wait-turn crew — no flicker: {kill:?}"
        );

        // Overstaffed beyond the steady crew: the count is rate-derived (not clamped up to assigned), so
        // an extra hand is still flagged.
        let over = slow_breeder_hunt(SLOW_BREEDER_BODY, steady_crew + 1);
        assert_eq!(
            over.workers_needed, steady_crew,
            "the crew is the steady need, independent of overstaffing: {over:?}"
        );
        assert!(
            steady_crew + 1 > over.workers_needed,
            "a herd overstaffed beyond its steady crew still flags the idle hand: {over:?}"
        );
    }

    /// **A domesticated slow breeder reports `max(herders_needed, steady_haul)`, and it equals the
    /// client's `_max_useful_workers`.** The managed rung staffs one crew big enough for both jobs; the
    /// haul side is the steady carry crew (stable across the pulse), so the band panel's overstaff note
    /// and the compose panel's stepper cap read the same number — which is the whole point of the fix.
    #[test]
    fn a_domesticated_slow_breeder_reports_max_of_herders_and_steady_crew_matching_the_client() {
        let (mut world, tile) = world_with_source(CAP);
        reseat_slow_breeder(&mut world, SLOW_BREEDER_BIOMASS, SLOW_BREEDER_BODY);
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
                    policy: FollowPolicy::Sustain,
                },
                workers: assigned,
            }],
        );
        world.run_system_once(advance_labor_allocation);
        let yielded = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();

        // The sim's expectation: one crew, `max(herders, steady_haul)`.
        let (herders, steady_haul, client_max_useful) = {
            let fauna = world.resource::<FaunaConfigHandle>().get();
            let labor = world.resource::<LaborConfigHandle>().get();
            let ladder = LadderConfig::builtin();
            let herd = world.resource::<HerdRegistry>().find(HERD_ID).unwrap();
            let herders = crate::fauna::herd_herders_needed(herd, &fauna);
            let rate = crate::fauna::hunt_policy_rate(
                FollowPolicy::Sustain,
                herd.biomass_before_regrowth,
                crate::fauna::herd_capacity(herd, &fauna),
                &crate::fauna::herd_ecology(herd, &fauna),
                &fauna,
                &ladder,
            );
            let steady_haul = crate::fauna::hunt_haul_workers(
                rate,
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
            let ceiling = forecast.ceiling_for(FollowPolicy::Sustain);
            let food_per_animal = forecast.body_mass_yield;
            let per_worker_yield = forecast.per_worker_yield;
            let client = ((((ceiling / food_per_animal).floor() + 1.0) * food_per_animal
                / per_worker_yield)
                .ceil()) as u32;
            (herders, steady_haul, client)
        };

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
    /// f32 slack between the forecast (`workers × per_worker_yield`, provisions) and the sim's take
    /// (biomass → fixed-point provisions): different multiplication order + a 1e-6 fixed-point grid.
    /// Orders of magnitude below one provision.
    const FORECAST_EPSILON: f32 = 1e-4;
    /// Every policy a **Forage** assignment accepts: the four extractive rungs + the Cultivate
    /// investment rung (whose ceiling is the *preparing* dip).
    const FORAGE_POLICIES: [FollowPolicy; 5] = [
        FollowPolicy::Sustain,
        FollowPolicy::Surplus,
        FollowPolicy::Market,
        FollowPolicy::Eradicate,
        FollowPolicy::Cultivate,
    ];
    /// Every policy a **Hunt** assignment accepts: the four extractive rungs + the Corral investment
    /// rung.
    const HUNT_POLICIES: [FollowPolicy; 5] = [
        FollowPolicy::Sustain,
        FollowPolicy::Surplus,
        FollowPolicy::Market,
        FollowPolicy::Eradicate,
        FollowPolicy::Corral,
    ];

    /// The client's composition: what it would display as the expected yield for this staffing. The
    /// shared helper — the *same* one the assign-time telemetry seed uses — so these tests pin the
    /// number the client shows, not a re-derivation of it.
    fn expected_yield(forecast: &SourceYieldForecast, workers: u32, policy: FollowPolicy) -> f32 {
        forecast_expected_take(forecast, workers, policy)
    }

    /// The client's worker-stepper cap.
    fn max_useful_workers(forecast: &SourceYieldForecast, policy: FollowPolicy) -> u32 {
        (forecast.ceiling_for(policy) / forecast.per_worker_yield).ceil() as u32
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

    /// **Forage forecast == actual.** For every policy × staffing (labor-bound, ceiling-bound), the
    /// client's `min(workers × per_worker_yield, ceiling[policy])` equals the provisions
    /// `advance_labor_allocation` actually pays. Both binding regimes are asserted to have been
    /// exercised, so this can't silently degenerate into testing one branch.
    #[test]
    fn forage_forecast_equals_actual_take_for_every_policy_and_staffing() {
        let mut saw_labor_bound = false;
        let mut saw_ceiling_bound = false;
        for policy in FORAGE_POLICIES {
            for workers in [1u32, 2, 20] {
                let (mut world, tile) = world_with_source(CAP);
                // Forecast off the PRE-turn patch state, exactly as the client reads it from the
                // snapshot captured at the end of last turn.
                let patch = world
                    .resource::<ForageRegistry>()
                    .patch(SOURCE)
                    .cloned()
                    .expect("seeded patch");
                let labor = world.resource::<LaborConfigHandle>().get();
                let forecast = forage_forecast(
                    &patch,
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
                            policy,
                            species: None,
                        },
                        workers,
                    }],
                );
                world.run_system_once(advance_labor_allocation);
                let actual = world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual;

                let labor_term = workers as f32 * forecast.per_worker_yield;
                let ceiling = forecast.ceiling_for(policy);
                if labor_term < ceiling {
                    saw_labor_bound = true;
                } else {
                    saw_ceiling_bound = true;
                }
                let expected = expected_yield(&forecast, workers, policy);
                assert!(
                    (actual - expected).abs() < FORECAST_EPSILON,
                    "forage forecast must equal the actual take ({policy:?}, {workers} workers): \
                     forecast={expected} actual={actual} ({forecast:?})"
                );
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
    /// **The forecast is now the STEADY sustainable rate, not the credit-inclusive burst** — it drops
    /// the transient `hunt_credit` term (see `hunt_forecast`). So forecast == actual holds exactly when
    /// **`hunt_credit == 0`**: with an empty bank the take path's `min(0 + rate, biomass)` IS the steady
    /// rate, so the first turn's take equals the displayed ceiling. A herd carrying banked credit would
    /// legitimately take *more* this turn (it cashes the bank) than the steady readout advertises — that
    /// lumpiness is the take's, not the forecast's — so the invariant is asserted on a fresh herd, and
    /// the `hunt_credit == 0` precondition below is load-bearing, not incidental.
    #[test]
    fn hunt_forecast_equals_actual_take_for_every_policy_and_staffing() {
        const BIG_HERD_CAP: f32 = 1_000.0;
        let mut saw_labor_bound = false;
        let mut saw_ceiling_bound = false;
        for policy in HUNT_POLICIES {
            for workers in [1u32, 2, 20] {
                let (mut world, tile) = world_with_source(CAP);
                reseat_herd(&mut world, BIG_HERD_CAP, BIG_HERD_CAP);
                let herd = world
                    .resource::<HerdRegistry>()
                    .find(HERD_ID)
                    .cloned()
                    .expect("seeded herd");
                assert_eq!(
                    herd.hunt_credit, 0.0,
                    "forecast == actual is the empty-bank invariant: the steady readout matches the \
                     take only when no banked credit is waiting to be cashed"
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
                            policy,
                        },
                        workers,
                    }],
                );
                world.run_system_once(advance_labor_allocation);
                let actual = world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual;

                let labor_term = workers as f32 * forecast.per_worker_yield;
                let ceiling = forecast.ceiling_for(policy);
                if labor_term < ceiling {
                    saw_labor_bound = true;
                } else {
                    saw_ceiling_bound = true;
                }
                let expected = expected_yield(&forecast, workers, policy);
                assert!(
                    (actual - expected).abs() < FORECAST_EPSILON,
                    "hunt forecast must equal the actual take ({policy:?}, {workers} workers): \
                     forecast={expected} actual={actual} ({forecast:?})"
                );
            }
        }
        assert!(
            saw_labor_bound && saw_ceiling_bound,
            "both regimes must be covered: labor-bound={saw_labor_bound} ceiling-bound={saw_ceiling_bound}"
        );
    }

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
        let labor = world.resource::<LaborConfigHandle>().get();
        let patch_forecast = forage_forecast(
            &patch,
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

        // **The policy axis is collapsed**: every policy — including the investment rungs, since the
        // improvement is already built — quotes the one managed yield.
        for policy in FORAGE_POLICIES {
            assert_eq!(
                patch_forecast.ceiling_for(policy),
                patch_forecast.managed_yield,
                "a Field is yours — no policy takes more or less of it ({policy:?})"
            );
        }
        for policy in HUNT_POLICIES {
            assert_eq!(
                herd_forecast.ceiling_for(policy),
                herd_forecast.managed_yield,
                "a pen is yours — no policy takes more or less of it ({policy:?})"
            );
        }

        // **The worker cap is NOT collapsed.** `per_worker_yield` is the crew's real throughput, so
        // this Field genuinely needs more than one pair of hands — the readout the old hardcoded `1`
        // made permanently false.
        let field_workers_needed = max_useful_workers(&patch_forecast, FollowPolicy::Sustain);
        assert!(
            field_workers_needed > 1,
            "a Field at capacity offers more than one worker can carry: {field_workers_needed}"
        );
        for policy in FORAGE_POLICIES {
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
                    policy: FollowPolicy::Sustain,
                    species: None,
                },
                workers: field_workers_needed,
            }],
        );
        let short_handed = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    policy: FollowPolicy::Sustain,
                },
                workers: 1,
            }],
        );
        world.run_system_once(advance_labor_allocation);

        let field_row = world
            .get::<LaborAllocation>(field_band)
            .unwrap()
            .last_yields[0]
            .clone();
        let field_forecast =
            expected_yield(&patch_forecast, field_workers_needed, FollowPolicy::Sustain);
        assert!(field_forecast > 0.0);
        assert!(
            (field_row.actual - field_forecast).abs() < FORECAST_EPSILON,
            "Field forecast must equal the actual payout: {field_forecast} vs {}",
            field_row.actual
        );
        assert!(
            (field_row.actual - patch_forecast.managed_yield).abs() < FORECAST_EPSILON,
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
        let pen_forecast = expected_yield(&herd_forecast, 1, FollowPolicy::Sustain);
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
    /// competitor-removal explicit as *concentration*, a growth boost double-counted it. So "tended
    /// beats wild" now lives entirely in a committed crop — **concentration + conversion** (§4.3) — and
    /// is pinned by the roster's own bar (`core_sim/tests/flora_roster.rs`) and `flora_commitment.rs`,
    /// which see the crop this scale-free rung mechanic cannot.
    #[test]
    fn a_bare_tended_patch_is_neutral_versus_wild_and_draws_down() {
        let (mut world, tile) = world_with_source(CAP);
        let cfg = world.resource::<LaborConfigHandle>().get();
        let forage = cfg.forage.clone();
        let patch_cap = forage.capacity_for(SOURCE_BIOME);
        let biomass = patch_cap;
        let wild_msy =
            sustainable_yield(biomass, patch_cap, &forage.ecology) * forage.provisions_per_biomass;
        drop(cfg);
        cultivate_source_patch(&mut world, biomass);

        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: UVec2::new(0, 0),
                    policy: FollowPolicy::Sustain,
                    species: None,
                },
                workers: WORKERS,
            }],
        );

        world.run_system_once(advance_labor_allocation);

        // At the neutral gain a bare tended patch reads the same MSY curve as wild — the boost is
        // retired, and no committed crop means no conversion. `WORKERS` is enough hands to reach the
        // ceiling, so the ceiling — not the crew — binds.
        let expected = wild_msy * forage.cultivation.tended_regrowth_gain;
        let paid = world
            .get::<PopulationCohort>(band)
            .unwrap()
            .stores
            .get(FOOD)
            .to_f32();
        assert!(
            (paid - expected).abs() < 1e-3,
            "bare tended band gathers the neutral MSY: {paid} vs {expected}"
        );
        assert!(
            (paid - wild_msy).abs() < 1e-3,
            "with the boost retired and no crop committed, a bare tended patch pays exactly wild — \
             the payoff moved to concentration + conversion: {paid} vs {wild_msy}"
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
        // Telemetry: a Sustain take of the boosted curve is exactly sustainable → no ⚠, but
        // `sustainable` is now a *measured* line rather than a copy of `actual`.
        let row = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();
        assert!((row.actual - expected).abs() < 1e-3);
        assert!((row.actual - row.sustainable).abs() < 1e-3);
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
    /// Market (20% of biomass) < Eradicate (30%). (At the retired gain 2.0 the boosted Surplus rode
    /// past the flat Market skim; that swap is gone with the boost.)
    #[test]
    fn a_tended_patch_is_policy_live_worker_capped_and_can_be_over_farmed() {
        let extractive = [
            FollowPolicy::Sustain,
            FollowPolicy::Surplus,
            FollowPolicy::Market,
            FollowPolicy::Eradicate,
        ];
        // A real operating point: a patch under active harvest sits below its cap (still above K/2, so
        // Sustain reads the MSY plateau). Full-cap would land Surplus exactly on Market (see docstring).
        const OPERATING_FRACTION: f32 = 0.8;
        let mut takes: Vec<(FollowPolicy, f32)> = Vec::new();
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
                        policy,
                        species: None,
                    },
                    workers: WORKERS,
                }],
            );
            world.run_system_once(advance_labor_allocation);
            let row = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();
            let patch = world.resource::<ForageRegistry>().patch(SOURCE).unwrap();
            assert!(
                patch.biomass < patch_cap,
                "{policy:?} must draw the tended patch down"
            );
            if matches!(policy, FollowPolicy::Sustain) {
                assert!(
                    row.actual <= row.sustainable + 1e-3,
                    "Sustain on the boosted curve is sustainable — no ⚠: {row:?}"
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
        // the leanest, then the boosted Surplus, then the flat Market skim, Eradicate the deepest.
        let take_of = |wanted: FollowPolicy| {
            takes
                .iter()
                .find(|(policy, _)| *policy == wanted)
                .expect("every policy ran")
                .1
        };
        assert!(take_of(FollowPolicy::Sustain) < take_of(FollowPolicy::Surplus));
        assert!(take_of(FollowPolicy::Surplus) < take_of(FollowPolicy::Market));
        assert!(take_of(FollowPolicy::Market) < take_of(FollowPolicy::Eradicate));
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
                    policy: FollowPolicy::Sustain,
                    species: None,
                },
                workers: WORKERS,
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
                    policy: FollowPolicy::Sustain,
                    species: None,
                },
                workers: WORKERS,
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
                    policy: FollowPolicy::Sustain,
                    species: None,
                },
                workers: WORKERS,
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

    /// Grant the harness faction full knowledge of a discovery (the Rung 1b/1c ledger gate that the
    /// Cultivate / Corral investment policies check).
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
        grant_knowledge(&mut world, CULTIVATION_DISCOVERY_ID);
        let (fraction, progress_per_turn) = {
            let ladder = world.resource::<LadderConfigHandle>().get();
            let tended = ladder.rung(RungKey::PlantTended);
            (
                tended
                    .yield_fraction_while_building()
                    .expect("the tended rung is an investment"),
                tended.build_accrual(FollowPolicy::Cultivate, true, RUNG_TIMESCALE_UNSCALED),
            )
        };

        // Baseline: what the same patch pays under Sustain (the MSY skim) with ample workers.
        let sustain_world_band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    policy: FollowPolicy::Sustain,
                    species: None,
                },
                workers: WORKERS,
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
        grant_knowledge(&mut world, CULTIVATION_DISCOVERY_ID);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    policy: FollowPolicy::Cultivate,
                    species: None,
                },
                workers: WORKERS,
            }],
        );
        world.run_system_once(advance_labor_allocation);
        let preparing = world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual;
        assert!(
            (preparing - fraction * sustain_yield).abs() < FORECAST_EPSILON,
            "preparing pays fraction × the Sustain yield: {preparing} vs {}",
            fraction * sustain_yield
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
        // **Harvest the finished patch to read the payoff.** `Cultivate` is the *build* verb, and its
        // dip is "the crew is preparing ground, not gathering" — a fact that does not stop being true
        // because the ground is now ready, so a completed patch left on `Cultivate` still pays the
        // dip. (The animal side has always behaved this way: `Tame` on an already-tamed herd pays the
        // pastoral dip too. Slice 7 made the plant side agree — the old managed branch ignored the
        // policy and paid full, which is why this line used to pass without switching.) The player
        // switches back to a harvest policy; so does the test.
        set_forage_policy(&mut world, band, FollowPolicy::Sustain);
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
    }

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
                pen.build_accrual(FollowPolicy::Corral, true, RUNG_TIMESCALE_UNSCALED),
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
                    policy: FollowPolicy::Sustain,
                },
                workers: WORKERS,
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
                    policy: FollowPolicy::Corral,
                },
                workers: WORKERS,
            }],
        );
        world.run_system_once(advance_labor_allocation);
        let preparing = world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual;
        // **RETARGETED IN SLICE 8: the dip is exact in BIOMASS, and it cannot be exact in ANIMALS.**
        // The take is `floor(dip × escapement / body_mass)` whole animals, and `floor` does not
        // distribute over a scale: `floor(0.5 × 9 animals)` is 4, not 4.5. So asserting
        // `preparing == fraction × sustain_yield` to a float epsilon is asserting that a rounding
        // artifact doesn't exist. The **contract** the dip actually has is that it is the rung's
        // fraction of the Sustain take *to within the one animal quantisation can cost*, and that it
        // is strictly less than Sustain — the investment must visibly cost something.
        let one_animal = {
            let fauna = world.resource::<FaunaConfigHandle>().get();
            hunt_provisions(TEST_GAME_BODY_MASS, &fauna, 1.0)
        };
        assert!(
            (preparing - fraction * sustain_yield).abs() <= one_animal + FORECAST_EPSILON,
            "building the pen pays fraction × the Sustain yield, to within one whole animal: \
             {preparing} vs {} (one animal = {one_animal})",
            fraction * sustain_yield
        );
        assert!(
            preparing < sustain_yield,
            "the pen build must cost real yield against Sustain: {preparing} vs {sustain_yield}"
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
    }

    /// Without the earned knowledge, the investment policies accrue **nothing** — the take is still the
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
                    policy: FollowPolicy::Cultivate,
                    species: None,
                },
                workers: WORKERS,
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
                    policy: FollowPolicy::Corral,
                },
                workers: WORKERS,
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
                    policy: FollowPolicy::Corral,
                },
                workers: WORKERS,
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
    fn hunt_one_turn(world: &mut World, tile: Entity, policy: FollowPolicy) {
        spawn_band(
            world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    policy,
                },
                workers: WORKERS,
            }],
        );
        world.run_system_once(advance_labor_allocation);
    }

    /// Staff a band on the source patch under `policy` and resolve one turn.
    fn forage_one_turn(world: &mut World, tile: Entity, policy: FollowPolicy) {
        spawn_band(
            world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    policy,
                    species: None,
                },
                workers: WORKERS,
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
        hunt_one_turn(&mut world, tile, FollowPolicy::Sustain);

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
        hunt_one_turn(&mut world, tile, FollowPolicy::Sustain);

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
        forage_one_turn(&mut world, tile, FollowPolicy::Sustain);

        assert!(
            knowledge(&world, SEED_SELECTION_DISCOVERY_ID) > 0.0,
            "working a TENDED patch earns Seed Selection"
        );
    }

    /// **§4.2 — only stewardship teaches.** The overdrawing policies earn **nothing, at any rung**:
    /// you learn husbandry by managing, not by slaughtering. Swept across both webs and both of the
    /// rungs that teach, so a future rung cannot quietly opt out of the rule.
    #[test]
    fn the_overdrawing_policies_teach_nothing_at_any_rung() {
        for policy in [
            FollowPolicy::Surplus,
            FollowPolicy::Market,
            FollowPolicy::Eradicate,
        ] {
            // Animal rung 1 (wild) and rung 2 (pastoral).
            for tamed in [false, true] {
                let (mut world, tile) = world_with_source(CAP);
                reseat_herd(&mut world, TEACHING_HERD_CAP, TEACHING_HERD_CAP);
                if tamed {
                    world.resource_mut::<HerdRegistry>().herds[0]
                        .accrue_domestication(BAND_FACTION, RUNG_COMPLETE);
                }
                hunt_one_turn(&mut world, tile, policy);
                assert_eq!(
                    knowledge(&world, HERDING_DISCOVERY_ID),
                    0.0,
                    "{policy:?} must teach no Herding (tamed={tamed})"
                );
                assert_eq!(
                    knowledge(&world, PENNING_DISCOVERY_ID),
                    0.0,
                    "{policy:?} must teach no Penning (tamed={tamed})"
                );
            }

            // Plant rung 1 (wild) and rung 2 (tended).
            for cultivated in [false, true] {
                let (mut world, _) = world_with_source(CAP);
                let tile = world.resource::<TileRegistry>().tiles[0];
                if cultivated {
                    world
                        .resource_mut::<ForageRegistry>()
                        .patch_mut(SOURCE)
                        .expect("seeded patch")
                        .accrue_cultivation(BAND_FACTION, RUNG_COMPLETE);
                }
                forage_one_turn(&mut world, tile, policy);
                assert_eq!(
                    knowledge(&world, CULTIVATION_DISCOVERY_ID),
                    0.0,
                    "{policy:?} must teach no Cultivation (cultivated={cultivated})"
                );
                assert_eq!(
                    knowledge(&world, SEED_SELECTION_DISCOVERY_ID),
                    0.0,
                    "{policy:?} must teach no Seed Selection (cultivated={cultivated})"
                );
            }
        }
    }

    /// **You learn from a HEALTHY source** — the `Thriving` gate both shipped earn sites had, and the
    /// refactor preserves. A collapsing herd teaches nothing even under Sustain.
    #[test]
    fn a_source_that_is_not_thriving_teaches_nothing() {
        let (mut world, tile) = world_with_source(CAP);
        {
            let mut registry = world.resource_mut::<HerdRegistry>();
            registry.herds[0].ecology_phase = EcologyPhase::Collapsing;
        }
        hunt_one_turn(&mut world, tile, FollowPolicy::Sustain);
        assert_eq!(
            knowledge(&world, HERDING_DISCOVERY_ID),
            0.0,
            "a collapsing herd teaches nothing — you learn from a healthy source"
        );

        let (mut world, _) = world_with_source(CAP);
        let tile = world.resource::<TileRegistry>().tiles[0];
        {
            let mut registry = world.resource_mut::<ForageRegistry>();
            registry
                .patch_mut(SOURCE)
                .expect("seeded patch")
                .ecology_phase = EcologyPhase::Collapsing;
        }
        forage_one_turn(&mut world, tile, FollowPolicy::Sustain);
        assert_eq!(
            knowledge(&world, CULTIVATION_DISCOVERY_ID),
            0.0,
            "a collapsing patch teaches nothing"
        );
    }

    /// **§4.2 — the two food webs learn separately.** Hunting only ever advances the animal track and
    /// foraging the plant track: a master rancher isn't automatically a farmer. This falls out of the
    /// rung's branch, but it is the claim the design makes, so it is asserted directly.
    #[test]
    fn the_two_food_webs_do_not_cross_teach() {
        // Hunting a wild herd teaches Herding and touches NEITHER plant knowledge.
        let (mut world, tile) = world_with_source(CAP);
        hunt_one_turn(&mut world, tile, FollowPolicy::Sustain);
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
        forage_one_turn(&mut world, tile, FollowPolicy::Sustain);
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
