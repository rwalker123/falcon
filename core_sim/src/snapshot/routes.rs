use super::*;

use crate::routes::{
    route_build_fraction, route_keeping_basis, route_neglect_grace_remaining,
    route_upkeep_workers_needed, span_of_terrains, RouteLedger,
};

/// **The countdown a road with nothing at risk publishes.** Paired with `has_neglect_grace: false`,
/// which is the field a reader must check — this number is only here because the wire has no
/// optional scalars, and it deliberately reuses the *"biting now"* value rather than inventing a
/// sentinel a client could mistake for a real countdown. The subsistence rows' own convention.
const NO_NEGLECT_REMAINING: u32 = 0;

/// **THE ROADS THE VIEWER CAN SEE, ON THE WIRE** (`docs/plan_standing_upkeep.md` §4.13, issue #532).
///
/// # THE FOG GATE IS `Discovered`, NOT `Active` — AND THAT IS THE OPPOSITE OF THE HERD ROW'S
///
/// A road is published to a faction that has explored **at least one** of its path tiles. Ground you
/// saw two hundred turns ago says nothing about where a *herd* is standing today, which is why
/// `herd_is_visible` demands `Active`; a road does not wander off, so remembering one is remembering
/// something true. **A road on tiles the faction has never seen does not reach it at all.**
///
/// **"At least one tile" is the route arc's rule 2 read back.** A band standing on any tile of a
/// road is *served by* that road and is billed for its keeping — so a faction that has stood on one
/// tile of it demonstrably knows of the road, and its own `roadworkDemand` names that bill. A gate
/// demanding the whole path would make a half-explored road vanish from the very band paying for it.
///
/// **Fails CLOSED**, the herd gate's rule: an absent faction map (before the first
/// `calculate_visibility`, or the turn after a rollback clears the ledger) publishes no road, which
/// is what the all-unexplored raster beside it is already saying.
///
/// # EVERY NUMBER COMES OFF THE STAMPED BILL
///
/// `demand − supplied == shortfall` must hold **verbatim on the wire**, so all three — and the
/// worker count beside them — resolve through [`route_keeping_basis`], the stamp
/// `settle_route_keeping` struck this turn. An interpolated demand moves *within* a turn as bands
/// walk on and off a road, and this branch is the one most exposed to that: the arc has had the
/// defect twice.
///
/// **Order is the ledger's `BTreeMap` order**, so the section is stable frame to frame and diffs out
/// when nothing moved.
///
/// `terrain_at` is the caller's resolver for *"what is under this tile"*, which is all
/// [`span_of_terrains`] — the one definition of the span's arithmetic — needs. A tile the caller
/// cannot resolve contributes nothing, exactly as `routes::route_span` reads an off-map path.
pub(crate) fn route_states(
    ledger: &RouteLedger,
    visibility: &crate::visibility::VisibilityLedger,
    viewer: FactionId,
    fog_enabled: bool,
    ladder: &LadderConfig,
    terrain_at: impl Fn(UVec2) -> Option<sim_runtime::TerrainType>,
) -> Vec<sim_runtime::RouteState> {
    ledger
        .iter()
        .filter(|(_, route)| {
            !fog_enabled
                || route
                    .path
                    .iter()
                    .any(|pos| visibility.is_discovered(viewer, pos.x, pos.y))
        })
        .map(|(id, route)| {
            let span = span_of_terrains(route.path.iter().filter_map(|pos| terrain_at(*pos)));
            let demand = route_keeping_basis(route, span, ladder);
            let grace = route_neglect_grace_remaining(route, ladder);
            let payoff = route.payoff();
            sim_runtime::RouteState {
                id: id.0,
                path_x: route.path.iter().map(|pos| pos.x).collect(),
                path_y: route.path.iter().map(|pos| pos.y).collect(),
                // The rung it HOLDS. The client reads this string rather than thresholding the
                // meter beside it, which describes a different rung.
                rung: route.held_rung().wire_key(),
                build_fraction: route_build_fraction(route, ladder),
                upkeep_demand: demand,
                upkeep_supplied: route.upkeep_supplied,
                // Derived off the same basis as the demand above, never stored, so the identity
                // cannot be broken by a road nobody stamped a shortfall onto.
                upkeep_shortfall: crate::intensification::upkeep_shortfall(
                    demand,
                    route.upkeep_supplied,
                ),
                upkeep_workers_needed: route_upkeep_workers_needed(route, span, ladder),
                has_neglect_grace: grace.is_some(),
                neglect_grace_remaining: grace.unwrap_or(NO_NEGLECT_REMAINING),
                // **The resolved answer.** A client cannot re-derive *"is the bill met"* — that is a
                // comparison against the stamped basis with the sim's own epsilon.
                grants_sight: route.grants_sight(),
                friction_multiplier: payoff.friction_multiplier,
                holds_link_to_tiles: payoff.holds_link_to_tiles,
            }
        })
        .collect()
}
