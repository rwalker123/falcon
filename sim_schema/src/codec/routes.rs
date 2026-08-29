//! Route-section FlatBuffers serialization — the roads in the ground.

use crate::codec::FbBuilder;
use crate::state::routes::RouteState;
use crate::world::{WorldDelta, WorldSnapshot};
use flatbuffers::WIPOffset;
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

fn create_routes<'a>(
    builder: &mut FbBuilder<'a>,
    routes: &[RouteState],
) -> WIPOffset<flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<fb::RouteState<'a>>>> {
    let offsets: Vec<_> = routes
        .iter()
        .map(|route| {
            let path_x = builder.create_vector(&route.path_x);
            let path_y = builder.create_vector(&route.path_y);
            let rung = builder.create_string(&route.rung);
            fb::RouteState::create(
                builder,
                &fb::RouteStateArgs {
                    id: route.id,
                    pathX: Some(path_x),
                    pathY: Some(path_y),
                    rung: Some(rung),
                    buildFraction: route.build_fraction,
                    upkeepDemand: route.upkeep_demand,
                    upkeepSupplied: route.upkeep_supplied,
                    upkeepShortfall: route.upkeep_shortfall,
                    upkeepWorkersNeeded: route.upkeep_workers_needed,
                    hasNeglectGrace: route.has_neglect_grace,
                    neglectGraceRemaining: route.neglect_grace_remaining,
                    grantsSight: route.grants_sight,
                    frictionMultiplier: route.friction_multiplier,
                    holdsLinkToTiles: route.holds_link_to_tiles,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

pub(crate) fn serialize_route_section<'a>(
    builder: &mut FbBuilder<'a>,
    snapshot: &WorldSnapshot,
) -> WIPOffset<fb::RouteSection<'a>> {
    let routes = create_routes(builder, &snapshot.routes);
    fb::RouteSection::create(
        builder,
        &fb::RouteSectionArgs {
            routes: Some(routes),
        },
    )
}

pub(crate) fn serialize_route_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::RouteSection<'a>> {
    // `None` means "unchanged this frame"; the whole vector is re-sent when any road moves, like
    // every other `Whole` section.
    let routes = delta
        .routes
        .as_ref()
        .map(|routes| create_routes(builder, routes));
    fb::RouteSection::create(builder, &fb::RouteSectionArgs { routes })
}
