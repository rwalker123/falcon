//! Route-section FlatBuffers serialization — the roads in the ground, one row per tile.

use crate::codec::subsistence::{
    create_build_toe, create_upkeep_toe, decode_build_toe, decode_upkeep_toe,
};
use crate::codec::{map_rows, map_rows_if_present, text, FbBuilder};
use crate::state::routes::{RouteState, TileImprovementState};
use crate::state::subsistence::BuildLegState;
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
            let rung = builder.create_string(&route.rung);
            // Why the pool is stuck on this tile — `""` when it is not stuck at all.
            let build_blocked_reason = builder.create_string(&route.build_blocked_reason);
            let build_toe = create_build_toe(builder, &route.build_toe);
            fb::RouteState::create(
                builder,
                &fb::RouteStateArgs {
                    tileX: route.tile_x,
                    tileY: route.tile_y,
                    rung: Some(rung),
                    buildFraction: route.build_fraction,
                    hasKeeper: route.has_keeper,
                    keeperBandId: route.keeper_band_id,
                    keeperRemoteness: route.keeper_remoteness,
                    upkeepDemand: route.upkeep_demand,
                    upkeepSupplied: route.upkeep_supplied,
                    upkeepShortfall: route.upkeep_shortfall,
                    upkeepWorkersNeeded: route.upkeep_workers_needed,
                    hasNeglectGrace: route.has_neglect_grace,
                    neglectGraceRemaining: route.neglect_grace_remaining,
                    grantsSight: route.grants_sight,
                    frictionMultiplier: route.friction_multiplier,
                    holdsLinkToTiles: route.holds_link_to_tiles,
                    buildBlockedReason: Some(build_blocked_reason),
                    buildersStaffed: route.builders_staffed,
                    buildersKitted: route.builders_kitted,
                    buildersCap: route.builders_cap,
                    buildToe: build_toe,
                    buildMaterialDemand: route.build_material_demand,
                    buildMaterialSupplied: route.build_material_supplied,
                    buildTurnsRemaining: route.build_turns_remaining,
                    upkeepMaterialDemand: route.upkeep_material_demand,
                    upkeepMaterialSupplied: route.upkeep_material_supplied,
                },
            )
        })
        .collect();
    builder.create_vector(&offsets)
}

fn create_tile_improvements<'a>(
    builder: &mut FbBuilder<'a>,
    improvements: &[TileImprovementState],
) -> WIPOffset<flatbuffers::Vector<'a, flatbuffers::ForwardsUOffset<fb::TileImprovementState<'a>>>>
{
    let offsets: Vec<_> = improvements
        .iter()
        .map(|improvement| {
            let branch = builder.create_string(&improvement.branch);
            let current_rung = builder.create_string(&improvement.current_rung);
            let build_destination_rung = builder.create_string(&improvement.build_destination_rung);
            let leg_offsets: Vec<_> = improvement
                .build_legs
                .iter()
                .map(|leg: &BuildLegState| {
                    let rung = builder.create_string(&leg.rung);
                    fb::BuildLegState::create(
                        builder,
                        &fb::BuildLegStateArgs {
                            rung: Some(rung),
                            workRemaining: leg.work_remaining,
                            turnsRemaining: leg.turns_remaining,
                        },
                    )
                })
                .collect();
            let build_legs = builder.create_vector(&leg_offsets);
            let build_blocked_reason = builder.create_string(&improvement.build_blocked_reason);
            let upkeep_toe = create_upkeep_toe(builder, &improvement.upkeep_toe);
            let build_toe = create_build_toe(builder, &improvement.build_toe);
            fb::TileImprovementState::create(
                builder,
                &fb::TileImprovementStateArgs {
                    tileX: improvement.tile_x,
                    tileY: improvement.tile_y,
                    branch: Some(branch),
                    currentRung: Some(current_rung),
                    buildDestinationRung: Some(build_destination_rung),
                    buildLegs: Some(build_legs),
                    buildTurnsRemaining: improvement.build_turns_remaining,
                    buildBlockedReason: Some(build_blocked_reason),
                    upkeepDemand: improvement.upkeep_demand,
                    upkeepSupplied: improvement.upkeep_supplied,
                    upkeepShortfall: improvement.upkeep_shortfall,
                    neglectTurns: improvement.neglect_turns,
                    hasNeglectGrace: improvement.has_neglect_grace,
                    neglectGraceRemaining: improvement.neglect_grace_remaining,
                    upkeepWorkersNeeded: improvement.upkeep_workers_needed,
                    upkeepHands: improvement.upkeep_hands,
                    upkeepToolsShort: improvement.upkeep_tools_short,
                    upkeepToe: upkeep_toe,
                    buildersStaffed: improvement.builders_staffed,
                    buildersKitted: improvement.builders_kitted,
                    buildersCap: improvement.builders_cap,
                    buildToe: build_toe,
                    isQueued: improvement.is_queued,
                    storedFood: improvement.stored_food,
                    storeCapacity: improvement.store_capacity,
                    hasStoreOwner: improvement.has_store_owner,
                    storeOwnerBand: improvement.store_owner_band,
                    storeSpoiled: improvement.store_spoiled,
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
    let tile_improvements = create_tile_improvements(builder, &snapshot.tile_improvements);
    fb::RouteSection::create(
        builder,
        &fb::RouteSectionArgs {
            routes: Some(routes),
            tileImprovements: Some(tile_improvements),
        },
    )
}

pub(crate) fn serialize_route_section_delta<'a>(
    builder: &mut FbBuilder<'a>,
    delta: &WorldDelta,
) -> WIPOffset<fb::RouteSection<'a>> {
    // `None` means "unchanged this frame"; the whole vector is re-sent when any road tile moves,
    // like every other `Whole` section.
    let routes = delta
        .routes
        .as_ref()
        .map(|routes| create_routes(builder, routes));
    let tile_improvements = delta
        .tile_improvements
        .as_ref()
        .map(|improvements| create_tile_improvements(builder, improvements));
    fb::RouteSection::create(
        builder,
        &fb::RouteSectionArgs {
            routes,
            tileImprovements: tile_improvements,
        },
    )
}

// ---------------------------------------------------------------------------
// Decoders — the inverse of every `create_*` above, in the same order.
// ---------------------------------------------------------------------------

pub(crate) fn decode_route_section(section: fb::RouteSection<'_>, snapshot: &mut WorldSnapshot) {
    snapshot.routes = map_rows(section.routes(), decode_route);
    snapshot.tile_improvements = map_rows(section.tileImprovements(), decode_tile_improvement);
}

pub(crate) fn decode_route_section_delta(section: fb::RouteSection<'_>, delta: &mut WorldDelta) {
    // Absent is "unchanged this frame", exactly as the serializer left it.
    delta.routes = map_rows_if_present(section.routes(), decode_route);
    delta.tile_improvements =
        map_rows_if_present(section.tileImprovements(), decode_tile_improvement);
}

fn decode_tile_improvement(improvement: fb::TileImprovementState<'_>) -> TileImprovementState {
    TileImprovementState {
        tile_x: improvement.tileX(),
        tile_y: improvement.tileY(),
        branch: text(improvement.branch()),
        current_rung: text(improvement.currentRung()),
        build_destination_rung: text(improvement.buildDestinationRung()),
        build_legs: improvement
            .buildLegs()
            .map(|legs| {
                legs.iter()
                    .map(|leg| BuildLegState {
                        rung: text(leg.rung()),
                        work_remaining: leg.workRemaining(),
                        turns_remaining: leg.turnsRemaining(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        build_turns_remaining: improvement.buildTurnsRemaining(),
        build_blocked_reason: text(improvement.buildBlockedReason()),
        upkeep_demand: improvement.upkeepDemand(),
        upkeep_supplied: improvement.upkeepSupplied(),
        upkeep_shortfall: improvement.upkeepShortfall(),
        neglect_turns: improvement.neglectTurns(),
        has_neglect_grace: improvement.hasNeglectGrace(),
        neglect_grace_remaining: improvement.neglectGraceRemaining(),
        upkeep_workers_needed: improvement.upkeepWorkersNeeded(),
        upkeep_hands: improvement.upkeepHands(),
        upkeep_tools_short: improvement.upkeepToolsShort(),
        upkeep_toe: decode_upkeep_toe(improvement.upkeepToe()),
        builders_staffed: improvement.buildersStaffed(),
        builders_kitted: improvement.buildersKitted(),
        builders_cap: improvement.buildersCap(),
        build_toe: decode_build_toe(improvement.buildToe()),
        is_queued: improvement.isQueued(),
        stored_food: improvement.storedFood(),
        store_capacity: improvement.storeCapacity(),
        has_store_owner: improvement.hasStoreOwner(),
        store_owner_band: improvement.storeOwnerBand(),
        store_spoiled: improvement.storeSpoiled(),
    }
}

fn decode_route(route: fb::RouteState<'_>) -> RouteState {
    RouteState {
        tile_x: route.tileX(),
        tile_y: route.tileY(),
        rung: text(route.rung()),
        build_fraction: route.buildFraction(),
        has_keeper: route.hasKeeper(),
        keeper_band_id: route.keeperBandId(),
        keeper_remoteness: route.keeperRemoteness(),
        upkeep_demand: route.upkeepDemand(),
        upkeep_supplied: route.upkeepSupplied(),
        upkeep_shortfall: route.upkeepShortfall(),
        upkeep_workers_needed: route.upkeepWorkersNeeded(),
        has_neglect_grace: route.hasNeglectGrace(),
        neglect_grace_remaining: route.neglectGraceRemaining(),
        grants_sight: route.grantsSight(),
        friction_multiplier: route.frictionMultiplier(),
        holds_link_to_tiles: route.holdsLinkToTiles(),
        build_blocked_reason: text(route.buildBlockedReason()),
        builders_staffed: route.buildersStaffed(),
        builders_kitted: route.buildersKitted(),
        builders_cap: route.buildersCap(),
        build_toe: decode_build_toe(route.buildToe()),
        build_material_demand: route.buildMaterialDemand(),
        build_material_supplied: route.buildMaterialSupplied(),
        build_turns_remaining: route.buildTurnsRemaining(),
        upkeep_material_demand: route.upkeepMaterialDemand(),
        upkeep_material_supplied: route.upkeepMaterialSupplied(),
    }
}
