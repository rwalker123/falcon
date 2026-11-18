//! **The band's yield telemetry seeding and re-staffing, shared by the command handlers and the
//! scenario loader** (`docs/plan_scenarios.md` section 2.9: a replayed verb must run the same code a
//! player's command does).
//!
//! These lived in `bin/server.rs`; they take a `World` so a library caller can run them with no
//! `App`. The server keeps thin wrappers with its old signatures.

use std::borrow::Cow;

use bevy::prelude::*;

use crate::*;

/// Fetch (or insert a default) mutable [`LaborAllocation`] on a band entity.
pub fn band_allocation_mut(
    world: &mut World,
    band: Entity,
) -> bevy::prelude::Mut<'_, LaborAllocation> {
    if world.get::<LaborAllocation>(band).is_none() {
        world.entity_mut(band).insert(LaborAllocation::default());
    }
    world
        .get_mut::<LaborAllocation>(band)
        .expect("labor allocation inserted above")
}

/// **RE-DECIDE A BAND'S STAFFED CREWS AND RE-QUOTE THEM** (#871) - after a command moved an input
/// the sim staffs from (a Max, a floor, a kit, a rank, a stopped row), every staffed row's crew may
/// have changed, not just the one the player touched (the pool is shared), so each is re-seeded from
/// its pre-commit forecast exactly as the touched row always was, and a row left with nobody reads
/// nothing rather than the reading of the crew it used to have.
pub fn restaff_and_reseed(world: &mut World, band: Entity) {
    restaff_and_reseed_after_loss(world, band, 0);
}

/// [`restaff_and_reseed`] for a command that removed people from the band: `lost_people` staffed
/// hands left with them and are discarded rather than walked home (`staffing::restaff_band_after_loss`).
pub fn restaff_and_reseed_after_loss(world: &mut World, band: Entity, lost_people: u32) {
    crate::staffing::restaff_band_after_loss(world, band, lost_people);
    let rows: Vec<(LaborTarget, u32)> = world
        .get::<LaborAllocation>(band)
        .map(|allocation| {
            allocation
                .assignments
                .iter()
                .filter(|row| row.target.is_staffed())
                .map(|row| (row.target.clone(), row.workers))
                .collect()
        })
        .unwrap_or_default();
    for (target, workers) in rows {
        if workers == crate::NO_CREW_ON_THIS_ACTIVITY {
            if let Some(mut allocation) = world.get_mut::<LaborAllocation>(band) {
                allocation.set_source_yield(&target, crate::SourceYield::ZERO);
            }
            continue;
        }
        let improvement = build_rung_on_source_at(world, band, &target);
        seed_source_yield(world, band, &target, improvement, workers);
    }
}

/// Seed the touched source's **yield telemetry** (`LaborAllocation.last_yields`) from its
/// **pre-commit forecast**, right after the allocation is mutated.
///
/// Without this, telemetry is only ever written during turn resolution (`advance_labor_allocation`),
/// so between "player assigns workers" and "player advances the turn" a brand-new source has no row
/// and the display snapshot serializes `actual_yield = 0.0` — the client cannot tell "0 because not
/// computed yet" from "0 because the source is barren", and every fresh assignment reads `+0.00`.
///
/// The seeded row is exactly what the turn will pay under unchanged conditions: it reuses the same
/// forecast helpers the take path reads (`forecast == actual` — see "Pre-commit Yield Forecast" in
/// `core_sim/CLAUDE.md`), and it is the same number the client's compose-time "Expected yield" row
/// promises — so there is no jump when the turn lands, and it is overwritten by the resolved take.
///
/// Only the **one source the command touched** is seeded (other sources keep their real actuals), and
/// only where the resolution path would actually pay: a source the turn would skip (an unseeded
/// patch, a vanished herd) keeps its zero row, and a genuinely barren source seeds `0.0` — `+0.00`
/// stays reachable, and correct, there. A source past the band's work range is not skipped: it posts
/// a work party, and is seeded by stepping that caravan.
pub fn seed_source_yield(
    world: &mut World,
    band: Entity,
    target: &LaborTarget,
    improvement: Option<RungId>,
    workers: u32,
) {
    // Unassigned (`workers == 0`): `set_assignment` already dropped the source's row with its
    // assignment. Scout/Warrior are band-wide roles with no food yield — the resolution path leaves
    // them at zero, so seeding must too.
    if workers == 0
        || !matches!(
            target,
            LaborTarget::Forage { .. } | LaborTarget::Hunt { .. } | LaborTarget::Extract { .. }
        )
    {
        return;
    }
    // **The band's people, and what they have learned** (#799): the seed resolves the reach the
    // turn's take will have, so a band that knows `drives` is quoted the driven herd.
    let Some(cohort) = world.get::<PopulationCohort>(band) else {
        return;
    };
    let faction = cohort.faction;
    let drives_known = crate::roaming_skills::world_faction_knows(
        world,
        faction,
        crate::roaming_skills::DRIVES_DISCOVERY_ID,
    );
    // **The kit this crew was assigned with, read off the assignment itself** — not off the band's
    // stock. `set_assignment` has already stored it, so the seed and the turn resolve the identical
    // tier through the identical seam, which is what `forecast == actual` rests on.
    //
    // **And the band's per-item BUDGET beside it, for the same reason one step on**
    // (`equipment.md` → "ONE BAND, ONE SET OF GEAR"): the turn arms this row from its *share* of the
    // ledger, so a seed priced against the whole ledger promises a band with two rows on one item a
    // haul only one of them can make. `set_assignment` has already landed this row, so the
    // allocation read here is the post-assignment truth and its denominator is the one
    // `advance_labor_allocation` will divide by next turn.
    //
    // ⛔ **SETTLED ON EVERY ROW'S CLAIM, NOT ITS HEAD COUNT** (`docs/plan_site_crews.md` §2.3) —
    // struck through the one function the turn settles with ([`crate::take_claims::row_claims`]),
    // so a brand-new row is quoted on its real claim at once.
    let crew_gear = {
        let equipment_cfg = world.resource::<EquipmentConfigHandle>().get();
        let band_wear = world
            .get::<BandEquipment>(band)
            .cloned()
            .unwrap_or_default();
        world.get::<LaborAllocation>(band).and_then(|allocation| {
            let index = allocation
                .assignments
                .iter()
                .position(|assignment| assignment.target.same_source(target))?;
            let assignment = &allocation.assignments[index];
            let claims = crate::take_claims::with_world_sources(world, faction, |sources| {
                crate::take_claims::row_claims(sources, allocation, &band_wear)
            });
            Some((
                assignment.kit_choice(&equipment_cfg),
                assignment.priority,
                allocation.item_budget(&equipment_cfg, &claims.claims),
                // **The rows this one competes with for the band's gear** — what a far row's
                // caravan forecast is priced beside (`crate::work_party::CaravanPricing`).
                allocation.rows_excluding_source(&equipment_cfg, target, &claims.claims),
                // **And this crew's own planned split** — its keep hands and the take hands
                // that claim its kit.
                crate::take_claims::CrewClaim {
                    keep_hands: claims.keep_hands[index],
                    claim: claims.claims[index],
                },
                claims.claims,
            ))
        })
    };
    let Some((crew_kit, crew_priority, item_budget, other_rows, crew_split, row_claims)) =
        crew_gear
    else {
        return;
    };
    let current_tile = cohort.current_tile;
    // The band's productivity multiplier is applied at payout in the resolution path, so the forecast
    // must fold it in too (the snapshot's per-source forecast is captured at 1.0 and scaled client-side
    // by this same multiplier).
    let wellbeing = world.resource::<WellbeingConfigHandle>().get();
    let output_mult = output_multiplier(cohort, &wellbeing).to_f32();
    let Some(band_pos) = world.get::<Tile>(current_tile).map(|tile| tile.position) else {
        return;
    };
    let labor = world.resource::<LaborConfigHandle>().get();
    // **The reported band's width** (`combat_config.forecast_range_sigmas`) — a readout lever, not a
    // model term (`docs/plan_hunt_through_combat.md` §6.4). Read on both webs so the one
    // `forecast_source_yield` seeds every row's range the same way.
    let range_sigmas = world
        .resource::<CombatConfigHandle>()
        .get()
        .forecast_range_sigmas;

    let seeded = match target {
        // The three band-wide roles work no source, so there is nothing to price — see the arm at
        // the end of this match.
        LaborTarget::Forage {
            tile,
            floor,
            take_species,
            ..
        } => {
            // ⛔ **PAST THE BAND'S WORK RANGE THE ROW IS A CARAVAN, AND IS SEEDED AS ONE** — never
            // declined. The row survives past range now, so a seed that returned here published
            // `+0.00` for the whole first turn of every far posting.
            let caravan = caravan_seed_party(world, band, target, *tile, band_pos, workers);
            let Some(tile_entity) = world.resource::<TileRegistry>().index(tile.x, tile.y) else {
                return;
            };
            // No food module → no wild **gather** season (`NO_FORAGE_SEASON`), exactly as the labor
            // arm reads it. Not an early return: since slice 5 a sown Field may stand on a
            // module-less tile, and its managed harvest is biomass-based and seasonless — returning
            // here would seed that Field a `0` row and reintroduce the `+0.00`-then-jump bug the seed
            // exists to kill.
            let seasonal = world
                .get::<FoodModuleTag>(tile_entity)
                .map_or(NO_FORAGE_SEASON, |module| module.seasonal_weight.max(0.0));
            let Some(patch) = world.resource::<ForageRegistry>().patch(*tile) else {
                return; // unseeded patch → the turn pays 0 (a bare-ground sow's honest opening row).
            };
            // Gathering is site-bound: a wild patch on ground the wild rung refuses has no gather
            // season, exactly as the labor arm reads it (`forage::gather_season`).
            let seasonal = crate::gather_season(
                seasonal,
                Some(patch),
                world.get::<Tile>(tile_entity).is_some_and(|ground| {
                    crate::wild_ground_refuses_gathering(
                        &world.resource::<LadderConfigHandle>().get(),
                        &world.resource::<LaborConfigHandle>().get().forage,
                        ground,
                        world.resource::<crate::FoodSiteRegistry>().is_site(*tile),
                    )
                }),
            );
            let flora = world.resource::<FloraConfigHandle>().get();
            // **What is growing on this tile** — the same realized basket the labor arm and the
            // snapshot read, so the assign-time seed is priced off the identical composition the
            // turn will pay from (#433). Absent ground names no plants.
            let map_seed = world.resource::<SimulationConfig>().map_seed;
            let tile_composition = world.get::<Tile>(tile_entity).map_or_else(
                || Cow::Owned(Vec::new()),
                |ground| tile_flora_composition(&flora, &labor.forage, ground, map_seed),
            );
            // **The seed must be priced at THIS band's BASKET tier**, for the same reason the hunt
            // arm below prices its haul at the band's sled tier: `advance_labor_allocation` resolves
            // the same tier through the same seam, so a band-agnostic equipped rate here would
            // promise a bare-handed band a basketful (`yield-forecast.md`).
            let equipment_cfg = world.resource::<EquipmentConfigHandle>().get();
            let band_wear = world
                .get::<BandEquipment>(band)
                .cloned()
                .unwrap_or_default();
            // **Through the same coverage the turn resolves** (`equipment.md` → "the
            // partly-equipped party"): baskets cover gatherers one unit at a time, so a seed priced
            // at the whole crew's best tier would promise a basketful to people holding nothing —
            // and off this row's **share** of the band's baskets, because the rows beside it are
            // reaching for the same ones.
            // **Spread over the take hands** — a keeper carries the keeping tools, never the take kit.
            let crew_coverage = equipment_cfg.coverage_from_units(
                &crew_kit,
                crew_split.take_hands(workers),
                &band_wear,
                item_budget.share_for_source(target, &band_wear, &equipment_cfg),
            );
            let per_worker_biomass = crew_coverage.weighted_rate(|kit| {
                equipment_cfg.forage_per_worker_biomass_capacity(
                    labor.forage.per_worker_biomass_capacity,
                    kit,
                    &band_wear,
                )
            });
            // **THE CREW KEEPS THE PATCH FIRST** (`docs/plan_site_crews.md` §2.1), so the seed quotes
            // the take on the hands its keeping leaves — the turn's own split, priced before the
            // band's tools are settled.
            let keep_hands = crate::crew_keep_hands(
                crate::patch_crew_keeping(
                    patch,
                    &world.resource::<LadderConfigHandle>().get(),
                    &labor.forage,
                    world
                        .get::<Tile>(tile_entity)
                        .map(|ground| crate::tile_forage_capacity(&labor.forage, ground)),
                    improvement,
                ),
                &equipment_cfg,
                &band_wear,
                workers,
            );
            let mut seeded = forage_source_yield_preview(
                patch,
                &tile_composition,
                &labor.forage,
                &flora,
                per_worker_biomass,
                seasonal,
                output_mult,
                workers,
                keep_hands,
                *floor,
                // **The crew's own take selection** — a seed priced on the whole basket would
                // promise a narrowed crew a stand it will not touch, which is exactly the
                // forecast-vs-actual split the seed exists to close.
                take_species,
                labor.yield_average_horizon_turns,
                labor.arrivals_horizon_turns,
                range_sigmas,
            );
            // ⛔ **THE `+` CAP, STRUCK AS THE TURN STRIKES IT** — the fewest take hands whose
            // capacity at this row's settled units reaches the room next turn's take will find, plus
            // the keep hands, rounded up once (`crate::forage_crew_needed`).
            let per_hand = |kit: &crate::KitChoice| {
                crate::forage_per_worker_biomass(
                    equipment_cfg.forage_per_worker_biomass_capacity(
                        labor.forage.per_worker_biomass_capacity,
                        kit,
                        &band_wear,
                    ),
                    seasonal,
                )
            };
            seeded.workers_needed = crate::forage_crew_needed(
                crate::crew_take_room(
                    &crate::next_turns_stand(patch, &labor.forage),
                    &tile_composition,
                    &flora,
                    &labor.forage,
                    take_species,
                    *floor,
                ),
                &item_budget.reach_coverage_for_source(
                    target,
                    &crew_kit,
                    crew_split.take_hands(workers),
                    &band_wear,
                    &equipment_cfg,
                ),
                per_hand,
                per_hand(&equipment_cfg.no_kit()),
                keep_hands,
            );
            // **A far row is priced by stepping its caravan** — the same function the turn's
            // published `netRateHome` answers through, at the same pricing.
            if let Some(party) = caravan {
                let pricing = crate::work_party::CaravanPricing::resolve(
                    &equipment_cfg,
                    &crew_kit,
                    crew_split.take_hands(workers),
                    crew_split.claim,
                    crew_priority,
                    &band_wear,
                    &other_rows,
                    &labor,
                );
                let forecast = crate::work_party::forecast_forage_caravan(
                    &party,
                    patch,
                    &tile_composition,
                    &labor.forage,
                    &flora,
                    pricing.forage_carry,
                    seasonal,
                    output_mult,
                    *floor,
                    take_species,
                    keep_hands,
                    labor.yield_average_horizon_turns,
                    &world
                        .resource::<crate::DemographicsConfigHandle>()
                        .get()
                        .keeping,
                );
                seed_caravan_row(&mut seeded, &forecast, labor.arrivals_horizon_turns, false);
            }
            seeded
        }
        LaborTarget::Hunt {
            fauna_id, floor, ..
        } => {
            let Some(herd) = world.resource::<HerdRegistry>().find(fauna_id) else {
                return; // herd gone → the assignment lapses next turn.
            };
            // Past the band's work range a hunt is seeded as a caravan, exactly as a gather is and
            // exactly as the turn posts it — the seed and the turn share one distance rule.
            let caravan =
                caravan_seed_party(world, band, target, herd.position(), band_pos, workers);
            let fauna = world.resource::<FaunaConfigHandle>().get();
            // **The seed must be priced at THIS band's SLED tier** (the minimal TOE), or the
            // exact-forecast-equals-actual invariant breaks the moment a band's baskets run dry:
            // `advance_labor_allocation` resolves the same tier through the same seam, so a
            // band-agnostic equipped rate here would promise a dry band a kitted haul.
            let equipment_cfg = world.resource::<EquipmentConfigHandle>().get();
            let band_wear = world
                .get::<BandEquipment>(band)
                .cloned()
                .unwrap_or_default();
            // **ONE CARRY RATE, PENNED OR WILD** (issue #543) — the same single rate
            // `advance_labor_allocation` resolves, because the seed has to reach `hunt_forecast`
            // holding the rate the turn will pay it at. A seed and a resolved row that priced a herd
            // differently is `yield-forecast.md`'s invariant broken on the one surface the player
            // commits from, so this must stay exactly-equal to the sim's arm.
            // **The same coverage the turn resolves** (`equipment.md` → "the partly-equipped
            // party"): `advance_labor_allocation` divides this crew by the gear the band actually
            // owns, so a seed priced at the whole party's best tier would promise a haul only the
            // armed half can make — and by this row's **share** of it, because the hunt row beside
            // it is reaching into the same stock of traps.
            // **Spread over the take hands** — a keeper carries the keeping tools, never the take kit.
            let hunt_coverage = equipment_cfg.coverage_from_units(
                &crew_kit,
                crew_split.take_hands(workers),
                &band_wear,
                item_budget.share_for_source(target, &band_wear, &equipment_cfg),
            );
            let per_worker_biomass = hunt_coverage.weighted_rate(|kit| {
                equipment_cfg.hunt_per_worker_biomass_capacity(
                    labor.hunt.per_worker_biomass_capacity,
                    kit,
                    &band_wear,
                )
            });
            // **And at THIS band's FIGHTING tier**, for the same reason and through the same seam
            // (`docs/plan_hunt_through_combat.md` §4): the take now resolves through the combat
            // system, so a band whose spears are gone brings down less — or, past a quarry's
            // `defense`, nothing at all — and the seed has to say so.
            let combat_cfg = world.resource::<CombatConfigHandle>().get();
            let hunting_party = crate::PartyResolution {
                equipment: &equipment_cfg,
                coverage: &hunt_coverage,
                wear: &band_wear,
                intrinsic: world.resource::<CreaturesConfigHandle>().get().person(),
                tuning: combat_cfg.tuning(),
                hunt_injury_damage_per_animal: combat_cfg.hunt_injury_damage_per_animal,
                drives_known,
            }
            .party_against(crate::Quarry::Mass(herd.body_mass));
            // **THE HUNT ROW IS THE HERD'S CREW AND KEEPS IT FIRST** (`docs/plan_site_crews.md`
            // §2.2) — the seed quotes the cull on the hands its keeping leaves.
            let keep_hands = crate::crew_keep_hands(
                crate::herd_crew_keeping_next_turn(
                    herd,
                    &fauna,
                    &world.resource::<LadderConfigHandle>().get(),
                    improvement,
                ),
                &equipment_cfg,
                &band_wear,
                workers,
            );
            let mut seeded = hunt_source_yield_preview(
                herd,
                &fauna,
                per_worker_biomass,
                &hunting_party,
                output_mult,
                workers,
                keep_hands,
                *floor,
                labor.yield_average_horizon_turns,
                labor.arrivals_horizon_turns,
                range_sigmas,
                // No posting ⇒ a camp kill: nothing is hauled, so the pack bounds nothing.
                caravan.is_none(),
            );
            // ⛔ **THE TAKE CREW, STRUCK AS THE TURN STRIKES IT** — crew-independent, the haul
            // walked over this row's own units ([`crate::hunt_crew_needed`]), on the herd as the
            // next turn's take will find it, with the keeping folded in once.
            let quarry = crate::next_turns_quarry(herd, &fauna);
            let carry = |kit: &crate::KitChoice| {
                equipment_cfg.hunt_per_worker_biomass_capacity(
                    labor.hunt.per_worker_biomass_capacity,
                    kit,
                    &band_wear,
                )
            };
            seeded.workers_needed = crate::crew_needed_with_keeping(
                crate::hunt_crew_needed(
                    crate::hunt_crew_room(&quarry, &fauna, *floor),
                    &quarry,
                    &fauna,
                    &item_budget.reach_coverage_for_source(
                        target,
                        &crew_kit,
                        crew_split.take_hands(workers),
                        &band_wear,
                        &equipment_cfg,
                    ),
                    carry,
                    carry(&equipment_cfg.no_kit()),
                    &hunting_party,
                    drives_known,
                ),
                keep_hands,
            );
            if let Some(party) = caravan {
                let pricing = crate::work_party::CaravanPricing::resolve(
                    &equipment_cfg,
                    &crew_kit,
                    crew_split.take_hands(workers),
                    crew_split.claim,
                    crew_priority,
                    &band_wear,
                    &other_rows,
                    &labor,
                );
                let hunters = pricing.hunters(
                    &equipment_cfg,
                    &band_wear,
                    &combat_cfg,
                    world.resource::<CreaturesConfigHandle>().get().person(),
                    herd.body_mass,
                    drives_known,
                );
                let forecast = crate::work_party::forecast_hunt_caravan(
                    &party,
                    herd,
                    &fauna,
                    pricing.haul_carry,
                    &hunters,
                    output_mult,
                    *floor,
                    keep_hands,
                    labor.yield_average_horizon_turns,
                    &world
                        .resource::<crate::DemographicsConfigHandle>()
                        .get()
                        .keeping,
                    world.get::<LaborAllocation>(band).map_or(
                        crate::work_party::NOTHING_CAPS_THE_MEAL,
                        |allocation| {
                            crate::work_party::band_net_eat_without(
                                cohort,
                                allocation,
                                &world.resource::<crate::DemographicsConfigHandle>().get(),
                                Some(target),
                            )
                        },
                    ),
                    &world.resource::<crate::MaterialsConfigHandle>().get(),
                );
                seed_caravan_row(&mut seeded, &forecast, labor.arrivals_horizon_turns, true);
            }
            seeded
        }
        // **A DEPOSIT PAYS NO FOOD, AND IT DOES PAY A MATERIAL — WHICH IS THE ONLY THING THIS ARM
        // SEEDS** (`docs/plan_extraction.md` §6, issue #650).
        //
        // ⛔ **`actual` STAYS `SourceYield::ZERO`, and that half of the original refusal was right.**
        // `PopulationCohortState::food_income` is `Σ actual` and one side of the pinned larder
        // identity, so a working must contribute nothing to it — seeded or resolved. The turn's own
        // `Extract` arm writes `row.materials` and touches no other field of the row, and this
        // mirrors it exactly, which is what keeps `forecast == actual` true per component here.
        //
        // What the refusal got wrong was the **row**: `actual_yield` is unconditionally on the wire
        // at `0.0` whether or not this seed runs, so declining to seed did not remove a `+0.00` food
        // line — it removed the *material* figure beside it, and a freshly-assigned deposit crew read
        // `+0.00` on the tile card where a freshly-assigned forage crew read a real number.
        LaborTarget::Extract {
            tile,
            material,
            floor,
        } => {
            // ⛔ **PAST THE BAND'S WORK RANGE THE ROW IS A CARAVAN, AND IS SEEDED AS ONE** — the
            // Forage arm's rule. A far working posts a party now rather than lapsing, so a seed that
            // declined it would publish nothing for the whole first turn of every far posting.
            let caravan = caravan_seed_party(world, band, target, *tile, band_pos, workers);
            let Some(tile_entity) = world.resource::<TileRegistry>().index(tile.x, tile.y) else {
                return;
            };
            let Some(ground) = world.get::<Tile>(tile_entity).cloned() else {
                return;
            };
            let extraction = world.resource::<ExtractionConfigHandle>().get();
            let ladder = world.resource::<LadderConfigHandle>().get();
            // **THE WORKING AS THE NEXT TURN WILL FIND IT.** A working nobody has opened is
            // DERIVED, not seeded — full stock at the tile's capacity, on its branch's free floor —
            // which is `snapshot::deposits`' own rule and the reason the commonest case of all (a
            // crew put on fresh ground) has a figure at all. Ground that holds none of it opens no
            // working and pays nothing — absence is the answer, and `validate_labor_policy` has
            // already refused the command in that case.
            let Some(working) = crate::extraction::projected_working(
                world.resource::<crate::DepositRegistry>(),
                *tile,
                material,
                &ground,
                &extraction,
                &ladder,
            ) else {
                return;
            };
            // **THE CREW'S KIT, claimed whole like every job's** — its tools' `deposit_take` apply
            // on the rungs they name, and its haul carry caps the cut and is a far party's pack.
            let equipment_cfg = world.resource::<EquipmentConfigHandle>().get();
            let band_wear = world
                .get::<BandEquipment>(band)
                .cloned()
                .unwrap_or_default();
            let held_rung = working.standing().held;
            let held_key = held_rung.wire_key();
            let Some(carry) = crate::extraction::DepositCarry::of(
                &labor,
                &world.resource::<crate::MaterialsConfigHandle>().get(),
                material,
            ) else {
                return;
            };
            // **REGROW FIRST, THEN TAKE, through the one projection the turn's forecast steps**
            // (`crate::extraction::DepositProjection`). The seed is read between turns, so the
            // live stock is the one *this* turn's take already drew down; pricing against it quotes
            // a turn the sim has not run (`yield-forecast.md` → "A FORECAST REGROWS FIRST"). The
            // take is the turn's own seam — `min(hands + tools, what the crew is allowed to reach)`
            // above `max(rung floor, this row's floor)` — so raising the floor lowers the seeded
            // figure by exactly what it will lower the take by.
            let taken = match caravan {
                // **A far working is priced by stepping its caravan** — the same function the turn's
                // published `netRateHome` answers through, at the same pricing — and what it seeds
                // is what lands home next turn: nothing while the party walks out.
                Some(party) => {
                    let pricing = crate::work_party::CaravanPricing::resolve(
                        &equipment_cfg,
                        &crew_kit,
                        crew_split.take_hands(workers),
                        crew_split.claim,
                        crew_priority,
                        &band_wear,
                        &other_rows,
                        &labor,
                    );
                    crate::work_party::forecast_extract_caravan(
                        &party,
                        &working,
                        &ground,
                        &extraction,
                        &ladder,
                        crate::work_party::material_pack(pricing.haul_carry, carry.weight),
                        pricing.deposit_gear_per_worker(
                            &equipment_cfg,
                            &band_wear,
                            held_rung.branch,
                            &held_key,
                        ),
                        *floor,
                        // The crew keeps the working first, off the hands present every turn.
                        crate::extraction::crew_keep_hands(
                            &equipment_cfg,
                            &band_wear,
                            &working,
                            &ground,
                            &extraction,
                            &ladder,
                            workers,
                        ),
                        labor.yield_average_horizon_turns,
                    )
                    .home_by_turn
                    .first()
                    .copied()
                    .unwrap_or(crate::work_party::NOTHING_CARRIED)
                }
                // **A local working is priced at the whole crew, off this row's own coverage of its
                // share of the band's gear** — the seam the turn strikes it at: its tools
                // (`EquipmentConfig::deposit_gear`) and its carry
                // (`crate::extraction::DepositCarry`). A seed priced bare would promise an
                // axe-carrying crew the bare-handed cut and jump the moment the turn landed.
                None => {
                    // **THE CREW KEEPS THE WORKING FIRST** (`docs/plan_site_crews.md` §2.5), so the
                    // seed quotes the cut on the hands its keeping leaves, with the cutters' share
                    // of the crew's lift — the turn's own split — and its take kit is budgeted less
                    // the keeping tools this crew's keeping is issued first
                    // (`extraction::prospective_deposit_gear`, the quote's own ration).
                    let allocation = world
                        .get::<LaborAllocation>(band)
                        .cloned()
                        .unwrap_or_default();
                    let keeping = crate::extraction::crew_keeping_issue(
                        &equipment_cfg,
                        &band_wear,
                        &working,
                        &ground,
                        &extraction,
                        &ladder,
                        workers,
                    );
                    let keep_hands = crate::extraction::crew_keep_hands(
                        &equipment_cfg,
                        &band_wear,
                        &working,
                        &ground,
                        &extraction,
                        &ladder,
                        workers,
                    );
                    let take_hands = (workers as f32 - keep_hands)
                        .max(crate::extraction::NO_HANDS_ON_THE_DEPOSIT);
                    let crew = crate::extraction::prospective_deposit_gear(
                        &equipment_cfg,
                        &allocation,
                        &row_claims,
                        target,
                        &crew_kit,
                        take_hands,
                        crew_split.claim,
                        &band_wear,
                        held_rung,
                        &carry,
                        &keeping,
                    );
                    crate::extraction::DepositProjection::new(&working)
                        .step(take_hands, crew.lift, *floor, &ground, &extraction, &ladder)
                        .unwrap_or(crate::extraction::DEPOSIT_EMPTY)
                }
            };
            crate::SourceYield {
                // **EMPTY IS "NO ROW", NEVER A ZERO ENTRY** — `SourceYield::materials`' own rule: a
                // crew that will take nothing next turn publishes no material line rather than one
                // reading `0.00`.
                materials: if taken > crate::extraction::DEPOSIT_EMPTY {
                    vec![crate::MaterialPayoff {
                        material: material.clone(),
                        amount: taken,
                    }]
                } else {
                    Vec::new()
                },
                ..crate::SourceYield::ZERO
            }
        }
        // A band-wide role produces no per-source yield, so there is no row to seed - and neither
        // does a tile improvement's crew, which keeps a standing and takes nothing.
        LaborTarget::Tile { .. }
        | LaborTarget::Band { .. }
        | LaborTarget::Scout
        | LaborTarget::Warrior
        | LaborTarget::Roadwork => return,
    };
    band_allocation_mut(world, band).set_source_yield(target, seeded);
}

/// **THE CARAVAN A FAR ROW'S SEED STEPS FROM**, or `None` inside the band's work range.
///
/// The row's standing party if it has one — a stepper press on a live posting re-seeds that posting,
/// not a fresh one that would re-promise a walk out — else a party posted now. The walk comes off
/// the one resolver the turn and the query read (`crate::work_party::resolve_walk`).
pub fn caravan_seed_party(
    world: &World,
    band: Entity,
    target: &LaborTarget,
    source_pos: UVec2,
    band_pos: UVec2,
    workers: u32,
) -> Option<crate::WorkParty> {
    let labor = world.resource::<LaborConfigHandle>().get();
    let supply = world.resource::<crate::SupplyNetworkConfigHandle>().get();
    let ladder = world.resource::<LadderConfigHandle>().get();
    let registry = world.resource::<TileRegistry>();
    let geometry = (
        registry.width,
        registry.height,
        world
            .resource::<SimulationConfig>()
            .map_topology
            .wrap_horizontal,
    );
    let (walk_tiles, walk_turns) = crate::work_party::resolve_walk(
        band_pos,
        source_pos,
        &labor,
        &supply,
        world.resource::<crate::RoadRegistry>(),
        &crate::routes::road_access_in_world(
            world,
            world
                .get::<crate::PopulationCohort>(band)
                .map_or(FactionId(0), |cohort| cohort.faction),
            &ladder,
        ),
        crate::routes::max_route_reach_tiles(&ladder),
        geometry,
    )?;
    let mut party = world
        .get::<LaborAllocation>(band)
        .and_then(|allocation| {
            allocation
                .assignments
                .iter()
                .find(|row| row.target.same_source(target))
                .and_then(|row| row.party.clone())
        })
        .unwrap_or_else(|| crate::WorkParty::posted(source_pos, walk_tiles, walk_turns));
    party.restamp(source_pos, workers, walk_tiles, walk_turns);
    Some(party)
}

/// **A far row's seed, off its caravan forecast.** `actual` is what the caravan lands **next turn**
/// — `0` for a party still walking out, which is the honest reading rather than a missing one — and
/// is a point, because a caravan's first turn has no retreat draw to spread; the rest is the turn's
/// own [`crate::work_party::publish_caravan_projection`].
pub fn seed_caravan_row(
    row: &mut crate::SourceYield,
    forecast: &crate::CaravanForecast,
    arrivals_horizon: u32,
    keeps_the_carcass: bool,
) {
    row.actual = forecast
        .home_by_turn
        .first()
        .copied()
        .unwrap_or(crate::work_party::NOTHING_CARRIED);
    row.range = crate::YieldRange::certain(row.actual);
    crate::work_party::publish_caravan_projection(
        row,
        forecast,
        arrivals_horizon,
        keeps_the_carcass,
    );
}

/// **WHAT ONE BAND IS ACTUALLY RAISING ON A SOURCE** — its declared build's declaration resolved
/// against the ground (`forage::patch_build_verb` / `fauna::herd_build_verb`), which is what the
/// yield seed has to price: a declaration answers only while the meter it names is at zero.
///
/// `None` for a source this band has not queued, for a ring (which raises no rung and pays no dip),
/// and for an entry whose ground has moved on — a **dead** entry.
pub fn build_rung_on_source(
    allocation: &LaborAllocation,
    target: &LaborTarget,
    world: &World,
) -> Option<RungId> {
    let source = BuildSource::of(target)?;
    // Neither "work on a rung already held" kind raises a rung or pays a dip.
    let declared = allocation.build_declaration(&source)?.declared.climb()?;
    let ladder = world.resource::<LadderConfigHandle>().get();
    match source {
        BuildSource::Patch(tile) => world
            .resource::<ForageRegistry>()
            .patch(tile)
            .and_then(|patch| patch_build_rung(patch, &ladder, Some(declared))),
        BuildSource::Herd(id) => world
            .resource::<HerdRegistry>()
            .find(&id)
            .and_then(|herd| herd_build_rung(herd, &ladder, Some(declared))),
        // **A working's declaration answers for itself**, exactly as a road's does and for its
        // reason: `fell`, `coppice` and `quarry` are the only things that ever raise one, so there
        // is no second, meter-derived declaration for a stale entry to override.
        BuildSource::Deposit { .. } => Some(declared),
        // **A road's declaration answers for itself**, with no meter-derived twin behind it: `grade`
        // and `pave` are the only things that ever raise a road, so there is no second statement for
        // the ground to override. Unreachable through this path in any case - the source comes from a
        // *labor target*, and no target names a road.
        BuildSource::Road(_) => Some(declared),
        // A tile improvement's declaration answers for itself, the road's reason exactly.
        BuildSource::Tile { .. } => Some(declared),
        // So is a band improvement's.
        BuildSource::Band { .. } => Some(declared),
    }
}

/// [`build_rung_on_source`] for a band named by entity — the shape the queueing loop needs, where
/// the allocation borrow has already been released.
pub fn build_rung_on_source_at(
    world: &World,
    band: Entity,
    target: &LaborTarget,
) -> Option<RungId> {
    let allocation = world.get::<LaborAllocation>(band)?.clone();
    build_rung_on_source(&allocation, target, world)
}
