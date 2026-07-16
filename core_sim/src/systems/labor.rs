use super::*;

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
/// - **Scout**: reveals fog outward from the band. **Warrior**: inert (occupies workers only).
///
/// Husbandry (Phase E) re-homes here: a Sustain hunt on a Thriving herd accrues domestication for
/// the acting faction, exactly as the retired follow did.
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
    fauna_config: Res<FaunaConfigHandle>,
    labor_config: Res<LaborConfigHandle>,
    ladder_config: Res<LadderConfigHandle>,
    wellbeing_config: Res<WellbeingConfigHandle>,
    tiles: Query<&Tile>,
    food_modules: Query<&FoodModuleTag>,
    mut cohorts: Query<(&mut PopulationCohort, &mut LaborAllocation)>,
) {
    let fauna = fauna_config.get();
    let labor = labor_config.get();
    let ladder = ladder_config.get();
    let wellbeing = wellbeing_config.get();
    let hunt = &fauna.hunt;
    let husbandry = &fauna.husbandry;
    let market = &fauna.market;
    let work_range = labor.band_work_range;
    let hunt_reach = labor.hunt_reach();
    // Rung 1b (earned Cultivation knowledge): the per-turn ledger accrual and the "known" threshold,
    // hoisted out of the per-cohort loop. A **Sustain**-forage on a Thriving patch teaches the faction
    // Cultivation (knowledge is still learned by ordinary foraging); a patch cannot accrue
    // `cultivation_progress` until the faction's ledger progress reaches `knowledge_threshold` AND a
    // band explicitly works it under the **Cultivate** policy.
    let cultivation = &labor.forage.cultivation;
    let knowledge_delta = scalar_from_f32(cultivation.knowledge_progress_per_turn);
    let knowledge_threshold = cultivation.knowledge_completion_threshold;
    // The two rungs the build engine drives (`crate::intensification`): the plant's tended patch and
    // the animal's pen. Their build dials — accrual rate, feral decay, and the investment dip — are
    // the ladder's, not each web's, so the two paths can never be tuned apart. Hoisted out of the
    // per-cohort loop alongside the knowledge levers.
    let tended_rung = ladder.rung(RungKey::PlantTended);
    let pastoral_rung = ladder.rung(RungKey::AnimalPastoral);
    let pen_rung = ladder.rung(RungKey::AnimalPen);
    // **Extending** a pen (2d-β) re-uses the pen rung's own build dials — a ring is the same fencing
    // labor at the same forgone-yield price, so it must never drift from the initial build.
    let pen_build_rate = pen_rung.build_accrual(FollowPolicy::Corral, true);
    let pen_build_dip = pen_rung
        .yield_fraction_while_building()
        .expect("the pen rung is an investment — it has a build meter");
    // Rung 1 (earned Herding knowledge, the animal mirror of Cultivation): a Sustain-hunt on a
    // Thriving herd teaches the faction Herding — the gate on **both** animal investment verbs
    // today (`Tame` at rung 2, `Corral` at rung 3; §4.3's reshuffle onto a separate `penning`
    // knowledge is a later slice). Sustain *teaches*; it no longer tames.
    let herding_knowledge_delta = scalar_from_f32(husbandry.knowledge_progress_per_turn);
    let herding_knowledge_threshold = husbandry.knowledge_completion_threshold;
    // In-range checks use true hex distance (not Chebyshev on offset coords, whose square
    // corners are actually 3 hex-steps away), wrap-aware to match the rest of the sim.
    let grid_width = tile_registry.width;
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
        for (idx, assignment) in allocation.assignments.iter().enumerate() {
            let workers = assignment.workers;
            if workers == 0 {
                continue;
            }
            match &assignment.target {
                LaborTarget::Forage { tile, policy } => {
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
                    let Ok(module) = food_modules.get(tile_entity) else {
                        continue; // module lost → 0 this turn.
                    };
                    let seasonal = module.seasonal_weight.max(0.0);
                    // Depletable patch (Intensification §0-ii): draw the biomass down via the shared
                    // `forage_take` primitive (mirrors the Hunt arm). Every `FoodModuleTag` tile is
                    // seeded a patch at Startup; a missing one (a dynamically-tagged tile) is skipped
                    // this turn. Gather per the assignment's policy (§0-iii, parity with hunting).
                    let Some(patch) = forage_registry.patch_mut(*tile) else {
                        continue;
                    };
                    // Rung 1b — the earned-knowledge ladder (§4b). A **Sustain** forage on a Thriving
                    // patch **teaches the faction Cultivation** (accrued in the shared
                    // `DiscoveryProgressLedger`, never start-granted). Knowledge only — it no longer
                    // tames the patch: cultivation is an **explicit `Cultivate` policy with an
                    // investment cost** (below), not a free by-product of Sustain.
                    if matches!(policy, FollowPolicy::Sustain)
                        && patch.ecology_phase == EcologyPhase::Thriving
                    {
                        discovery.add_progress(faction, CULTIVATION_DISCOVERY_ID, knowledge_delta);
                    }
                    // A cultivated ("tended") patch is worked, not wild-gathered (Rung 1a): the band
                    // whose Forage assignment tends it (≥1 worker here → place-local by construction)
                    // is paid `biomass × tended_provisions_per_biomass` — a **managed harvest** of the
                    // full standing crop, WITHOUT drawing biomass down (a tended patch regrows freely,
                    // so biomass sits near cap and the yield out-runs the same patch's wild MSY skim,
                    // the intensification incentive). This is maintenance labor: the amount is
                    // biomass-based (presence, not head-count, gates it beyond the `workers == 0`
                    // check above). Marking the patch tended-this-turn stops `advance_cultivation`
                    // taking it feral. Sustainable == actual (a managed harvest never overdraws → no
                    // ⚠). Mirrors a domesticated herd's husbandry income, but paid place-local here
                    // instead of split across the owner's bands.
                    if patch.is_cultivated() {
                        patch.tended_this_turn = true;
                        // Shared with the pre-commit forecast (`forage::forage_forecast`) so the
                        // client's "expected yield" for a tended patch is exactly what it is paid.
                        let provisions = scalar_from_f32(tended_provisions(
                            patch.biomass,
                            &labor.forage,
                            mult_f,
                        ));
                        if provisions > scalar_zero() {
                            cohort.stores.add(FOOD, provisions);
                        }
                        let tended = provisions.to_f32();
                        // A tended patch is maintenance labor (biomass-based payout, presence-gated),
                        // not scaling gather → a fixed one-worker "need" (never overstaffed by count).
                        yields[idx] = SourceYield {
                            actual: tended,
                            sustainable: tended,
                            workers_needed: TENDED_SOURCE_WORKERS_NEEDED,
                        };
                        continue;
                    }
                    let biomass_before = patch.biomass;
                    let provisions = forage_take(
                        patch,
                        workers,
                        *policy,
                        &labor.forage,
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
                        }) && patch.ecology_phase == EcologyPhase::Thriving;
                        // THE build seam: the rung supplies the accrual (0 unless Cultivate is the
                        // rung's verb and the gates hold); the patch owns its meter and the
                        // side-effects of completing it.
                        let accrual = tended_rung.build_accrual(*policy, eligible);
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
                    // Sustainable = one turn's net regrowth of the patch at its **pre-take** biomass,
                    // in provisions (same conversion + output multiplier as the actual take). This
                    // lights the over-forage ⚠ for free the moment `actual > sustainable`.
                    let sustainable = sustainable_yield(
                        biomass_before,
                        patch.carrying_capacity,
                        &labor.forage.ecology,
                    ) * labor.forage.provisions_per_biomass
                        * mult_f;
                    // Overstaffing: invert the take by the **effective** per-worker throughput this
                    // turn (`per_worker_biomass_capacity × seasonal`, matching `forage_take`'s worker
                    // cap) so a labor-bound low-season patch isn't falsely flagged.
                    let workers_needed = workers_needed_for_take(
                        take,
                        labor.forage.per_worker_biomass_capacity * seasonal,
                        workers,
                    );
                    yields[idx] = SourceYield {
                        actual: provisions.to_f32(),
                        sustainable,
                        workers_needed,
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
                    // **This band is working this herd** — whatever the policy. `advance_husbandry`
                    // reads the flag next turn (Logistics, the deliberate one-turn lag) and skips the
                    // **passive pastoral rung** for it: a band already paid through this arm is not
                    // *also* paid for leaving the herd alone. Without it the passive payment stacks on
                    // the `Corral` build dip and makes penning cost *less* than nothing — the "free
                    // path" the intensification ladder exists to delete (see `Herd::worked_this_turn`).
                    herd.worked_this_turn = true;
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
                        let demand = pen_upkeep(herd, &fauna) * (1.0 - pasture_fraction);
                        let paid = cohort.stores.take(FOOD, scalar_from_f32(demand)).to_f32();
                        pen_feed_paid += paid;
                        // The herd's TOTAL fed fraction: the footprint's share plus the paid share of
                        // the (reduced) larder bill. Fully fed when the larder covers its remainder (or
                        // nothing was demanded). A well-pastured pen whose keeper can't pay is still fed
                        // by its grass — `pasture_fraction`, never falsely 0.
                        let larder_covered = if demand > 0.0 {
                            (paid / demand).clamp(0.0, 1.0)
                        } else {
                            1.0
                        };
                        herd.pen_fed_fraction =
                            pasture_fraction + (1.0 - pasture_fraction) * larder_covered;
                        // Shared with the pre-commit forecast (`fauna::hunt_forecast`) so the
                        // client's "expected yield" for a corralled herd is exactly what it is paid.
                        // **While EXTENDING the pen (2d-β) the keeper is fencing, not fully
                        // harvesting**, so the take is DIPPED to the pen rung's
                        // `yield_fraction_while_building` — the forgone yield IS the labor cost of the
                        // ring, and it is literally the same dip the corral *build* pays because both
                        // read the one rung (§4 "worked by the keeper band's labor, no materials").
                        let mut take_biomass = fauna::pen_yield_biomass(herd, &fauna);
                        if herd.pen_extending {
                            take_biomass *= pen_build_dip;
                        }
                        herd.biomass -= take_biomass;
                        let provisions =
                            scalar_from_f32(hunt_provisions(take_biomass, &fauna, mult_f));
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
                        // A corralled herd is worker-tended maintenance (the animal mirror of the
                        // tended patch): a fixed one-worker "need", not scaling with the take. And a
                        // *managed* harvest never overdraws — it takes exactly the MSY — so
                        // `sustainable == actual` (no overdraw ⚠).
                        yields[idx] = SourceYield {
                            actual: tended,
                            sustainable: tended,
                            workers_needed: TENDED_SOURCE_WORKERS_NEEDED,
                        };
                        continue;
                    }
                    // Take food via the shared primitive (per-policy ceiling + worker-cap +
                    // biomass→provisions, × the band's productivity multiplier). Read biomass
                    // before/after for the raw take that trade goods + husbandry are scaled from.
                    let biomass_before = herd.biomass;
                    // The band has no carry room — it eats/banks the whole take, so pass an
                    // unbounded carry cap (behaviour unchanged from before the expedition clamp).
                    let provisions = hunt_take(
                        herd,
                        workers,
                        *policy,
                        labor.hunt.per_worker_biomass_capacity,
                        &fauna,
                        &ladder,
                        mult_f,
                        f32::INFINITY,
                    );
                    let take = biomass_before - herd.biomass;
                    // Rung 1 — the earned-knowledge ladder (§4). A **Sustain** hunt on a Thriving
                    // herd **teaches the faction Herding** (accrued in the shared
                    // `DiscoveryProgressLedger`, never start-granted). Knowledge only — it no longer
                    // tames the herd: taming is an **explicit `Tame` policy with an investment
                    // cost** (below), not a free by-product of Sustain. The exact mirror of the
                    // plant arm's Sustain→Cultivation branch.
                    if matches!(policy, FollowPolicy::Sustain)
                        && herd.ecology_phase == EcologyPhase::Thriving
                    {
                        discovery.add_progress(
                            faction,
                            HERDING_DISCOVERY_ID,
                            herding_knowledge_delta,
                        );
                    }
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
                                knows(&discovery, faction, knowledge, herding_knowledge_threshold)
                            }) && herd.can_domesticate()
                                && herd.ecology_phase == EcologyPhase::Thriving;
                        // THE build seam — the same call the plant side's Cultivate arm makes.
                        let accrual = pastoral_rung.build_accrual(*policy, eligible);
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
                    // cost), and here the pen accrues. Gates: the faction must **know Herding** and **own a
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
                            knows(&discovery, faction, knowledge, herding_knowledge_threshold)
                        }) && herd.can_pen()
                            && herd.is_domesticated()
                            && herd.owner == Some(faction);
                        // THE build seam — the same call the plant side's Cultivate arm makes.
                        let accrual = pen_rung.build_accrual(*policy, eligible);
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
                    let trade_goods =
                        (take * hunt.trade_goods_per_biomass * trade_multiplier * mult_f).round()
                            as i64;
                    if provisions > scalar_zero() {
                        cohort.stores.add(FOOD, provisions);
                    }
                    if trade_goods > 0 {
                        inventory.add_stockpile(faction, "trade_goods", trade_goods);
                    }
                    // Sustainable take = one turn's net regrowth of the herd at its **pre-take**
                    // biomass, in provisions (same `provisions_per_biomass` + output multiplier as
                    // the actual take). An overdraw (Surplus/Eradicate) reads `actual > sustainable`;
                    // a Sustain draw reads `actual ≈ sustainable`.
                    // The herd's OWN ecology/capacity (`herd_ecology`/`herd_capacity` — a tamed herd
                    // grows 3× faster, so its sustainable skim is 3× a wild one's).
                    let sustainable = sustainable_yield(
                        biomass_before,
                        herd_capacity(herd, &fauna),
                        &herd_ecology(herd, &fauna),
                    ) * hunt.provisions_per_biomass
                        * mult_f;
                    // Overstaffing: invert the biomass take by the per-hunter throughput (hunt has no
                    // seasonal factor, unlike forage).
                    let workers_needed = workers_needed_for_take(
                        take,
                        labor.hunt.per_worker_biomass_capacity,
                        workers,
                    );
                    yields[idx] = SourceYield {
                        actual: provisions.to_f32(),
                        sustainable,
                        workers_needed,
                    };
                }
                LaborTarget::Scout => {
                    // Scouts act as forward observers in `calculate_visibility`: staffed scouts
                    // post vantage points out from the band (`labor.scout.vantage_distance(scouts)`)
                    // and reveal from each, re-marked Active every turn — no work is done here.
                }
                LaborTarget::Warrior => {
                    // Inert this slice — the predator slice consumes Warrior strength.
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
    //! the same formula; and an overdraw reads `actual > sustainable`.
    use super::{advance_labor_allocation, TENDED_SOURCE_WORKERS_NEEDED};
    use crate::components::{
        FollowPolicy, LaborAllocation, LaborAssignment, LaborTarget, LocalStore, MoraleCause,
        PopulationCohort, Tile,
    };
    use crate::fauna::{
        forecast_expected_take, hunt_forecast, sustainable_yield, EcologyPhase, Herd, HerdRegistry,
        SourceYieldForecast, HERDING_DISCOVERY_ID,
    };
    use crate::fauna_config::{FaunaConfigHandle, SizeClass};
    use crate::food::{FoodModule, FoodModuleTag, FoodSiteKind};
    use crate::forage::{advance_forage_regrowth, forage_forecast, CULTIVATION_DISCOVERY_ID};
    use crate::forage::{ForagePatch, ForageRegistry};
    use crate::intensification::{LadderConfig, LadderConfigHandle, RungKey, RUNG_COMPLETE};
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
        world.insert_resource(LadderConfigHandle::default());
        world.insert_resource(WellbeingConfigHandle::default());
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
    /// the MSY-based `sustainable_yield` value at the pre-take biomass, and a Sustain draw under a
    /// binding regrowth ceiling skims exactly that (`actual ≈ sustainable`); (c) forage
    /// `sustainable ≡ actual`.
    #[test]
    fn forage_and_sustain_hunt_capture_yields() {
        let start = CAP * 0.5; // half cap → clear positive regrowth.
        let (mut world, tile) = world_with_source(start);
        let band = spawn_band(
            &mut world,
            tile,
            vec![
                LaborAssignment {
                    target: LaborTarget::Forage {
                        tile: UVec2::new(0, 0),
                        policy: FollowPolicy::Sustain,
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

        world.run_system_once(advance_labor_allocation);

        let alloc = world.get::<LaborAllocation>(band).unwrap();
        assert_eq!(alloc.last_yields.len(), 2, "one yield row per assignment");
        let forage = alloc.last_yields[0];
        let hunt = alloc.last_yields[1];
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
        assert!(
            (hunt.actual - hunt.sustainable).abs() < 1e-6,
            "a Sustain draw under the regrowth ceiling skims exactly the regrowth: {} vs {}",
            hunt.actual,
            hunt.sustainable
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

        let hunt = world.get::<LaborAllocation>(band).unwrap().last_yields[0];
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

        let hunt = world.get::<LaborAllocation>(band).unwrap().last_yields[0];
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
        assert!(
            (hunt.actual - hunt.sustainable).abs() < 1e-6,
            "a Sustain draw off a full herd skims exactly MSY: {} vs {}",
            hunt.actual,
            hunt.sustainable
        );
    }

    use crate::components::FOOD;

    /// Set the source-tile forage patch cultivated (owned by faction 0) at the given biomass.
    fn cultivate_source_patch(world: &mut World, biomass: f32) {
        let ecology = world.resource::<LaborConfigHandle>().get().forage.ecology;
        let mut registry = world.resource_mut::<ForageRegistry>();
        let patch = registry.patches.get_mut(&UVec2::new(0, 0)).unwrap();
        patch.cultivation_progress = 1.0;
        patch.owner = Some(FactionId(0));
        patch.biomass = biomass;
        patch.refresh_ecology_phase(&ecology);
    }

    /// Set the (wild, un-cultivated) source patch's biomass and refresh its ecology phase — for the
    /// `workers_needed` overstaffing tests, which need a full patch so the per-policy biomass-fraction
    /// ceiling binds rather than the seeded half-cap stock.
    fn set_wild_patch_biomass(world: &mut World, biomass: f32) {
        let ecology = world.resource::<LaborConfigHandle>().get().forage.ecology;
        let mut registry = world.resource_mut::<ForageRegistry>();
        let patch = registry.patches.get_mut(&UVec2::new(0, 0)).unwrap();
        patch.biomass = biomass;
        patch.refresh_ecology_phase(&ecology);
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
        let (mut world, tile) = world_with_source(CAP * 0.5); // half cap → clear positive MSY skim.
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

        world.run_system_once(advance_labor_allocation);

        let hunt = world.get::<LaborAllocation>(band).unwrap().last_yields[0];
        assert!(
            hunt.actual > 0.0,
            "the sustain hunt produced food: {hunt:?}"
        );
        assert_eq!(
            hunt.workers_needed, 1,
            "the MSY throughput needs a single worker: {hunt:?}"
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
                },
                workers: assigned,
            }],
        );

        world.run_system_once(advance_labor_allocation);

        let forage = world.get::<LaborAllocation>(band).unwrap().last_yields[0];
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

    /// A tended (cultivated) patch and a corralled herd are maintenance labor, not scaling gather, so
    /// each reports `workers_needed == 1` regardless of how many workers tend it.
    #[test]
    fn tended_patch_and_corral_report_one_worker_needed() {
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
            registry.herds[0].corral_at(UVec2::new(0, 0));
        }

        let forager = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: UVec2::new(0, 0),
                    policy: FollowPolicy::Sustain,
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

        let tended = world.get::<LaborAllocation>(forager).unwrap().last_yields[0];
        let corral = world.get::<LaborAllocation>(keeper).unwrap().last_yields[0];
        assert!(
            tended.actual > 0.0 && corral.actual > 0.0,
            "both tended sources pay out: tended={tended:?} corral={corral:?}"
        );
        assert_eq!(
            tended.workers_needed, 1,
            "a tended patch needs one tending presence: {tended:?}"
        );
        assert_eq!(
            corral.workers_needed, 1,
            "a corralled herd needs one keeper: {corral:?}"
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

    /// **Hunt forecast == actual.** The fauna twin of the forage test. The herd is re-seated at a
    /// large capacity so the Eradicate ceiling exceeds a single hunter's throughput (a labor-bound
    /// case); 20 hunters overstaff every policy (the ceiling binds).
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

    /// A **tended patch** / **corralled herd** forecasts its managed yield with ONE worker: every
    /// policy ceiling is that yield, `per_worker_yield` equals it (→ `max_useful_workers == 1`), and
    /// it is exactly what the sim pays the tending/keeping band.
    #[test]
    fn tended_patch_and_corral_forecast_full_yield_with_one_worker() {
        let (mut world, tile) = world_with_source(CAP);
        let patch_cap = world
            .resource::<LaborConfigHandle>()
            .get()
            .forage
            .capacity_for(SOURCE_BIOME);
        cultivate_source_patch(&mut world, patch_cap);
        {
            let mut registry = world.resource_mut::<HerdRegistry>();
            registry.herds[0].corral_at(SOURCE);
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

        // One worker suffices, and no policy changes the managed yield (including the investment
        // rungs — the improvement is already built, so "preparing" reads as the managed yield too).
        for policy in FORAGE_POLICIES {
            assert_eq!(max_useful_workers(&patch_forecast, policy), 1);
        }
        for policy in HUNT_POLICIES {
            assert_eq!(max_useful_workers(&herd_forecast, policy), 1);
        }

        // A single tending/keeping worker collects the whole forecast yield — and that IS what the
        // sim pays (both bands staff one worker here).
        let forager = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    policy: FollowPolicy::Sustain,
                },
                workers: TENDED_SOURCE_WORKERS_NEEDED,
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
                workers: TENDED_SOURCE_WORKERS_NEEDED,
            }],
        );
        world.run_system_once(advance_labor_allocation);

        let tended = world.get::<LaborAllocation>(forager).unwrap().last_yields[0].actual;
        let corral = world.get::<LaborAllocation>(keeper).unwrap().last_yields[0].actual;
        let tended_forecast = expected_yield(
            &patch_forecast,
            TENDED_SOURCE_WORKERS_NEEDED,
            FollowPolicy::Sustain,
        );
        let corral_forecast = expected_yield(
            &herd_forecast,
            TENDED_SOURCE_WORKERS_NEEDED,
            FollowPolicy::Sustain,
        );
        assert!(tended_forecast > 0.0 && corral_forecast > 0.0);
        assert!(
            (tended - tended_forecast).abs() < FORECAST_EPSILON,
            "tended patch forecast must equal the actual payout: {tended_forecast} vs {tended}"
        );
        assert!(
            (corral - corral_forecast).abs() < FORECAST_EPSILON,
            "corral forecast must equal the actual payout: {corral_forecast} vs {corral}"
        );
    }

    /// Rung 1a: a tended (cultivated) patch pays the band that tends it (a Forage assignment on the
    /// tile) `biomass × tended_provisions_per_biomass` — higher than the same patch's wild MSY skim —
    /// **without** drawing biomass down, and marks the patch tended-this-turn.
    #[test]
    fn tended_patch_pays_tending_band_above_msy_no_drawdown() {
        let (mut world, tile) = world_with_source(CAP);
        let cfg = world.resource::<LaborConfigHandle>().get();
        let patch_cap = cfg.forage.capacity_for(SOURCE_BIOME);
        // A tended patch regrows freely toward the cap; harvest is on its full standing crop.
        let biomass = patch_cap;
        let tended_rate = cfg.forage.cultivation.tended_provisions_per_biomass;
        let wild_msy = sustainable_yield(biomass, patch_cap, &cfg.forage.ecology)
            * cfg.forage.provisions_per_biomass;
        drop(cfg);
        cultivate_source_patch(&mut world, biomass);

        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: UVec2::new(0, 0),
                    policy: FollowPolicy::Sustain,
                },
                workers: WORKERS,
            }],
        );

        world.run_system_once(advance_labor_allocation);

        let expected = biomass * tended_rate; // output multiplier 1.0 at morale 1.
        let paid = world
            .get::<PopulationCohort>(band)
            .unwrap()
            .stores
            .get(FOOD)
            .to_f32();
        assert!(
            (paid - expected).abs() < 1e-3,
            "tended band paid biomass × tended rate: {paid} vs {expected}"
        );
        assert!(
            paid > wild_msy,
            "tended yield out-yields the wild MSY skim: {paid} vs {wild_msy}"
        );
        // No draw-down: a tended patch is a managed harvest, biomass unchanged.
        let patch = world
            .resource::<ForageRegistry>()
            .patch(UVec2::new(0, 0))
            .unwrap();
        assert!(
            (patch.biomass - biomass).abs() < 1e-6,
            "tended patch is not gather-drawn: {} vs {biomass}",
            patch.biomass
        );
        assert!(patch.tended_this_turn, "tending marks the patch worked");
        // Telemetry: a managed harvest never overdraws → actual == sustainable (no ⚠).
        let row = world.get::<LaborAllocation>(band).unwrap().last_yields[0];
        assert!((row.actual - expected).abs() < 1e-3);
        assert!((row.actual - row.sustainable).abs() < 1e-6);
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
                tended.build_accrual(FollowPolicy::Cultivate, true),
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
        world.run_system_once(advance_labor_allocation);
        let tended = world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual;
        assert!(
            tended > sustain_yield,
            "a tended patch out-pays the wild Sustain skim: {tended} vs {sustain_yield}"
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
        let (fraction, build_per_turn) = {
            let (world, _) = world_with_source(CAP);
            let ladder = world.resource::<LadderConfigHandle>().get();
            let pen = ladder.rung(RungKey::AnimalPen);
            (
                pen.yield_fraction_while_building()
                    .expect("the pen rung is an investment"),
                pen.build_accrual(FollowPolicy::Corral, true),
            )
        };

        // Baseline Sustain hunt yield on the same herd (ample hunters → ceiling-bound = MSY).
        // **It must be DOMESTICATED too**: Corral can only be worked on a domesticated herd, and the
        // husbandry ladder means a tamed herd lives on the *pastoral* ecology (`r` = 0.15, 3× wild).
        // Comparing the dip against a *wild* herd's MSY would compare two different rungs.
        let (mut world, tile) = world_with_source(CAP);
        reseat_herd(&mut world, BIG_HERD_CAP, BIG_HERD_CAP);
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

        // Corral on a domesticated herd the faction owns + knows Herding for.
        let (mut world, tile) = world_with_source(CAP);
        reseat_herd(&mut world, BIG_HERD_CAP, BIG_HERD_CAP);
        grant_knowledge(&mut world, HERDING_DISCOVERY_ID);
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
        assert!(
            (preparing - fraction * sustain_yield).abs() < FORECAST_EPSILON,
            "building the pen pays fraction × the Sustain yield: {preparing} vs {}",
            fraction * sustain_yield
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
            "Corral without Herding knowledge builds nothing"
        );
        assert!(!herd.is_corralled());
    }

    /// A Corral assignment on a herd that is **not domesticated** builds nothing (the second gate).
    #[test]
    fn corral_accrues_nothing_on_a_wild_herd() {
        let (mut world, tile) = world_with_source(CAP);
        grant_knowledge(&mut world, HERDING_DISCOVERY_ID);
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
}
