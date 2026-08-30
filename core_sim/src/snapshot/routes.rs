use super::*;

use crate::routes::{
    road_build_fraction, road_keeping_basis, road_neglect_grace_remaining, road_upkeep_measure,
    road_upkeep_workers_needed, RoadRegistry,
};

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
/// `settle_route_keeping` struck this turn.
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
            }
        })
        .collect()
}
