use super::*;

use crate::routes::{
    road_build_fraction, road_keeping_basis, road_neglect_grace_remaining, road_upkeep_measure,
    road_upkeep_workers_needed, RoadRegistry,
};

/// **A TILE IMPROVEMENT NOBODY IS RAISING NAMES NO DESTINATION** - the `buildDestinationRung` an
/// improvement with nothing queued (or whose builder is not the viewer's) publishes.
const NO_DESTINATION: &str = "";

/// **THE TILE IMPROVEMENTS THE VIEWER CAN SEE, ON THE WIRE - ONE ROW PER `(TILE, BRANCH)`.**
///
/// The road list's gate and its reasoning: `Discovered`, not `Active`, because an improvement is in
/// the ground and does not wander off; fails closed on an absent faction map. Order is the
/// registry's own `(y, x, branch)` key order, so the section is stable frame to frame and diffs out
/// when nothing moved.
///
/// # THE BUILDER'S SCRATCH FOLLOWS THE BUILDER
///
/// `buildDestinationRung`, `buildLegs`, `buildTurnsRemaining` and `buildBlockedReason` describe a
/// band's queue, not the ground - the source rows' rule (`factions.md` -> "The improvement follows
/// the GROUND; the BUILDER'S state follows the builder"). They publish only where one of the
/// viewer's own bands has the improvement queued (`build_kits`), and read their no-estimate values
/// everywhere else, so a rival's queue is not legible from a row on explored ground.
///
/// # EVERY NUMBER COMES OFF THE STAMPED BILL
///
/// `demand - supplied == shortfall` must hold verbatim on the wire, so all three resolve through
/// [`crate::tile_improvement::TileImprovement::keeping_basis`], the stamp `advance_tile_improvements`
/// struck and the labour pass paid against this turn.
pub(crate) fn tile_improvement_states(
    registry: &crate::tile_improvement::TileImprovementRegistry,
    visibility: &crate::visibility::VisibilityLedger,
    viewer: FactionId,
    fog_enabled: bool,
    ladder: &LadderConfig,
    keeping: &crate::demographics_config::KeepingConfig,
    build_kits: &crate::snapshot::subsistence::QueuedBuildSources,
) -> Vec<sim_runtime::TileImprovementState> {
    registry
        .iter()
        .filter(|improvement| {
            !fog_enabled || visibility.is_discovered(viewer, improvement.tile.x, improvement.tile.y)
        })
        .map(|improvement| {
            let demand = improvement.keeping_basis(ladder);
            let at_risk = ladder.rung(improvement.at_risk_rung());
            let grace = at_risk.upkeep.as_ref().map(|_| {
                crate::intensification::neglect_grace_remaining(
                    improvement.neglect_turns,
                    at_risk.upkeep_grace_turns(),
                )
            });
            // **The viewer's own queue, or nothing** - the builder's scratch rule above.
            let queued = build_kits.tile_is_queued(improvement.tile, improvement.branch);
            let crew = build_kits.crew_of(&crate::components::BuildSource::Tile {
                tile: improvement.tile,
                branch: improvement.branch,
            });
            // **The fixed store** (#709): its capacity follows the row's own fog (a fact about the
            // rung held); what is in it, who owns it and what rotted are the improvement's own
            // people's business and read zero to every other viewer.
            let capacity = keeping
                .store_spec(ladder, improvement.held_rung())
                .map_or(0.0, |spec| spec.capacity);
            let own = improvement.faction == viewer;
            sim_runtime::TileImprovementState {
                stored_food: if own {
                    improvement.contents.total().to_f32()
                } else {
                    0.0
                },
                store_capacity: capacity,
                has_store_owner: own && improvement.owner.is_some(),
                store_owner_band: if own {
                    improvement.owner.map_or(0, |band| band.0)
                } else {
                    0
                },
                store_spoiled: if own {
                    improvement.spoiled_this_turn
                } else {
                    0.0
                },
                tile_x: improvement.tile.x,
                tile_y: improvement.tile.y,
                branch: improvement.branch.as_str().to_string(),
                current_rung: improvement.held_rung().wire_key(),
                build_destination_rung: if queued {
                    improvement
                        .build_destination
                        .map_or_else(|| NO_DESTINATION.to_string(), |rung| rung.wire_key())
                } else {
                    NO_DESTINATION.to_string()
                },
                build_legs: if queued {
                    crate::snapshot::subsistence::published_build_legs(&improvement.build_legs)
                } else {
                    Vec::new()
                },
                build_turns_remaining: if queued {
                    crate::snapshot::subsistence::published_build_countdown(
                        improvement.build_turns_remaining,
                        improvement.build_queue_position,
                        queued,
                    )
                } else {
                    sim_runtime::NO_BUILD_TURNS_ESTIMATE
                },
                build_blocked_reason: if queued {
                    improvement.build_blocked_reason.key().to_string()
                } else {
                    String::new()
                },
                upkeep_demand: demand,
                upkeep_supplied: improvement.upkeep_supplied,
                upkeep_shortfall: crate::intensification::upkeep_shortfall(
                    demand,
                    improvement.upkeep_supplied,
                ),
                neglect_turns: u32::from(improvement.neglect_turns),
                has_neglect_grace: grace.is_some(),
                neglect_grace_remaining: grace.unwrap_or(NO_NEGLECT_REMAINING),
                upkeep_workers_needed: improvement.upkeep_workers_needed(ladder),
                upkeep_hands: improvement.upkeep_hands,
                upkeep_tools_short: crate::snapshot::subsistence::viewer_keeping_tools_short(
                    &improvement.upkeep_toe,
                    viewer,
                ),
                upkeep_toe: crate::snapshot::subsistence::upkeep_toe_lines(
                    &improvement.upkeep_toe,
                    viewer,
                ),
                builders_staffed: crew.staffed,
                builders_kitted: crew.kitted,
                builders_cap: crew.cap,
                build_toe: crew.toe,
                is_queued: queued,
                build_fraction: improvement.build_fraction(ladder),
            }
        })
        .collect()
}

/// **The countdown a road with nothing at risk publishes.** Paired with `has_neglect_grace: false`,
/// which is the field a reader must check — this number is only here because the wire has no
/// optional scalars, and it deliberately reuses the *"biting now"* value rather than inventing a
/// sentinel a client could mistake for a real countdown. The subsistence rows' own convention.
const NO_NEGLECT_REMAINING: u32 = 0;

/// **A ROAD NOBODY HAS TAKEN ON** — the `keeperBandId` a keeperless road publishes, paired with
/// `hasKeeper: false`. `0` is a real [`crate::components::BandId`], which is exactly why the bool
/// beside it is the field to read; the `hasNeglectGrace` pair one field over is minted under the same
/// rule.
const NO_KEEPER_BAND: u64 = 0;

/// **THE ROADS THE VIEWER CAN SEE, ON THE WIRE — ONE ROW PER TILE**
/// (`docs/plan_standing_upkeep.md` §4.13b, issue #532).
///
/// # THE FOG GATE IS `Discovered`, NOT `Active` — AND THAT IS THE OPPOSITE OF THE HERD ROW'S
///
/// A road is published to a faction that has explored **its tile**. Ground you saw two hundred turns
/// ago says nothing about where a *herd* is standing today, which is why `herd_is_visible` demands
/// `Active`; a road does not wander off, so remembering one is remembering something true.
///
/// **Per tile, the gate stopped needing a rule of its own.** The stored-path model had to decide how
/// much of a path had to be explored for the road to reach you; a road *is* a tile now, so the
/// question is *"have you seen that tile"* and nothing else.
///
/// **Fails CLOSED**, the herd gate's rule: an absent faction map (before the first
/// `calculate_visibility`, or the turn after a rollback clears the ledger) publishes no road, which
/// is what the all-unexplored raster beside it is already saying.
///
/// # EVERY NUMBER COMES OFF THE STAMPED BILL
///
/// `demand − supplied == shortfall` must hold **verbatim on the wire**, so all three — and the
/// worker count beside them — resolve through [`road_keeping_basis`], the stamp
/// `settle_bands_roadwork` struck this turn.
///
/// **Order is the registry's own row-major key order**, so the section is stable frame to frame and
/// diffs out when nothing moved.
///
/// `terrain_at` is the caller's resolver for *"what is under this tile"*, which is all
/// [`road_upkeep_measure`] — the one definition of the scale measure — needs. A tile the caller
/// cannot resolve measures nothing, exactly as `routes::road_measure` reads an off-map tile.
pub(crate) fn route_states(
    registry: &RoadRegistry,
    visibility: &crate::visibility::VisibilityLedger,
    viewer: FactionId,
    fog_enabled: bool,
    ladder: &LadderConfig,
    // **Which tiles some band has QUEUED** — the `queued_live` term of the countdown below, read off
    // the bands' own queues because the row's scratch lags a command by a whole turn and the state
    // it separates exists precisely in that frame.
    build_kits: &crate::snapshot::subsistence::QueuedBuildSources,
    terrain_at: impl Fn(UVec2) -> Option<sim_runtime::TerrainType>,
) -> Vec<sim_runtime::RouteState> {
    registry
        .iter()
        .filter(|(tile, _)| !fog_enabled || visibility.is_discovered(viewer, tile.x, tile.y))
        .map(|(tile, road)| {
            let measure = terrain_at(tile).map_or(0.0, |terrain| {
                road_upkeep_measure(terrain, road.keeper_remoteness)
            });
            let demand = road_keeping_basis(road, measure, ladder);
            let grace = road_neglect_grace_remaining(road, ladder);
            let payoff = road.payoff();
            let crew = build_kits.crew_of(&crate::components::BuildSource::Road(tile));
            sim_runtime::RouteState {
                tile_x: tile.x,
                tile_y: tile.y,
                // The rung it HOLDS. The client reads this string rather than thresholding the
                // meter beside it, which describes a different rung.
                rung: road.held_rung().wire_key(),
                build_fraction: road_build_fraction(road, ladder),
                // **The band whose job this tile is** — `false` across the whole free floor, which
                // is the commonest road in the game rather than an edge case.
                has_keeper: road.keeper.is_some(),
                keeper_band_id: road.keeper.map_or(NO_KEEPER_BAND, |keeper| keeper.band.0),
                // **What distance did to this road's price**, quoted when the keeper took it on —
                // `1.0` inside the base range and on every road nobody keeps.
                keeper_remoteness: road.keeper_remoteness,
                upkeep_demand: demand,
                upkeep_supplied: road.upkeep_supplied,
                // Derived off the same basis as the demand above, never stored, so the identity
                // cannot be broken by a road nobody stamped a shortfall onto.
                upkeep_shortfall: crate::intensification::upkeep_shortfall(
                    demand,
                    road.upkeep_supplied,
                ),
                upkeep_workers_needed: road_upkeep_workers_needed(road, measure, ladder),
                has_neglect_grace: grace.is_some(),
                neglect_grace_remaining: grace.unwrap_or(NO_NEGLECT_REMAINING),
                // **The resolved answer.** A client cannot re-derive *"is the bill met"* — that is a
                // comparison against the stamped basis with the sim's own epsilon.
                grants_sight: road.grants_sight(),
                friction_multiplier: payoff.friction_multiplier,
                holds_link_to_tiles: payoff.holds_link_to_tiles,
                // **THE BUILD IN FRONT OF THIS TILE** — a road is a source row and this table is it,
                // so the three fields are the twins a patch row publishes one branch over. All three
                // are stamped by the build arm and cleared by the decay pass on the one-turn cycle,
                // so they describe the turn just resolved rather than a stale stall.
                build_blocked_reason: road.build_blocked_reason.key().to_string(),
                build_material_demand: road.build_material_demanded,
                build_material_supplied: road.build_material_supplied,
                // ⛔ **THE COUNTDOWN, THROUGH THE SEAM BOTH FOOD WEBS GO THROUGH** — so a road
                // cannot publish a state as a different number than a patch does. The road shipped
                // publishing nothing here and the client filled the silence with a hardcoded *"not
                // yet estimated"*, which is why a road under way read `Queued 97%` for 147 turns.
                //
                // **Only a QUEUED road has a real number.** A rung nobody ordered has no quote, so
                // this is the honest *no estimate* rather than a `0` that would render as a
                // finished build; a client estimating an unbuilt rung against a hypothetical crew
                // is doing its own arithmetic and is right to.
                // ⛔ **THE STANDING MATERIAL BILL — THE HALF THAT SAYS *SHORT OF STONE* RATHER
                // THAN *SHORT OF KEEPERS*.** Summed across the materials the rung names because the
                // wire carries one number and the branch eats exactly one; the noun is the rung
                // catalog's. Both read the **stamped** bill, so `demand - supplied` is the shortfall
                // verbatim, exactly as the work pair above it is.
                upkeep_material_demand: road.upkeep_materials_demanded.values().sum(),
                upkeep_material_supplied: road.upkeep_materials_supplied.values().sum(),
                build_turns_remaining: crate::snapshot::subsistence::published_build_countdown(
                    road.build_turns_remaining,
                    road.build_queue_position,
                    build_kits.road_is_queued(tile),
                ),
                builders_staffed: crew.staffed,
                builders_kitted: crew.kitted,
                builders_cap: crew.cap,
                build_toe: crew.toe,
            }
        })
        .collect()
}
