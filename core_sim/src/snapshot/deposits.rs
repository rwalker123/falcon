use super::*;

use crate::extraction::{
    deposit_build_fraction, deposit_keeping_basis, deposit_measure,
    deposit_neglect_grace_remaining, deposit_payoff, deposit_reachable, deposit_runway,
    deposit_sustainable_take, deposit_upkeep_workers_needed, tile_deposit_capacity,
    tile_deposit_regrowth, DepositRegistry,
};

/// **The countdown a working with nothing at risk publishes.** Paired with
/// `has_neglect_grace: false`, which is the field a reader must check — this number is only here
/// because the wire has no optional scalars, and it deliberately reuses the *"biting now"* value
/// rather than inventing a sentinel a client could mistake for a real countdown. The road row's and
/// the subsistence rows' own convention.
const NO_NEGLECT_REMAINING: u32 = 0;

/// **THE WORKINGS THE VIEWER CAN SEE, ON THE WIRE — ONE ROW PER `(tile, material)`**
/// (`docs/plan_extraction.md` §7, issue #650).
///
/// # THE FOG GATE IS `Discovered`, AND IT IS THE ROAD ROW'S RATHER THAN THE HERD ROW'S
///
/// A working is published to a faction that has explored **its tile**. Ground you saw two hundred
/// turns ago says nothing about where a *herd* is standing today, which is why `herd_is_visible`
/// demands `Active`; a quarry does not wander off, so remembering one is remembering something
/// true. `route_states`' reading, and for its reason.
///
/// **The whole row passes or none of it does, and no field is redacted individually.** The two food
/// webs publish their rows unfiltered and leave the fog reading to the client
/// (`ForagePatchState::carrying_capacity` states the rule); the road row filters the row and redacts
/// nothing. A working is the road's shape — a sparse, tile-keyed registry of improvements a band
/// opened — so it takes the road's discipline, and a ladder-carrying field like `ladder_position`
/// never reaches a faction that has not seen the ground it stands on.
///
/// **Fails CLOSED**, the road and herd gates' rule: an absent faction map (before the first
/// `calculate_visibility`, or the turn after a rollback clears the ledger) publishes no working,
/// which is what the all-unexplored raster beside it is already saying.
///
/// # EVERY DERIVED NUMBER IS READ LIVE OFF THE TILE
///
/// Capacity, the regrowth rate and everything struck against them go through this module's seams
/// (`tile_deposit_capacity`, `tile_deposit_regrowth`, `deposit_reachable`, `deposit_payoff`) rather
/// than off a cached copy on the source. **That is the whole of `DepositSource`'s doc comment**: it
/// carries the stock and the position and nothing that could be derived, so retuning
/// `extraction.json` reaches every working already on the map.
///
/// # AND EVERY BILLED NUMBER COMES OFF THE STAMPED BILL
///
/// `demand − supplied == shortfall` must hold **verbatim on the wire**, so all three — and the
/// worker count beside them — resolve through [`deposit_keeping_basis`], the stamp
/// `extraction::advance_deposits` struck this turn at the post-decay position.
///
/// **Order is the registry's own key order**, so the section is stable frame to frame and diffs out
/// when nothing moved.
///
/// `terrain_at` is the caller's resolver for *"which tile is this"*. A working whose tile the caller
/// cannot resolve is **dropped rather than published at zero**: every number on the row is a
/// function of that ground, so a row without it would state a capacity of nothing, a runway off it
/// and an empty rate — three false readings where the honest answer is that the sim cannot see the
/// tile. Only a hand-built fixture can reach it (`advance_deposits` forgives the same case).
#[allow(clippy::too_many_arguments)] // the registry, the fog pair, both configs and both kit indexes
pub(crate) fn deposit_states<'a>(
    registry: &DepositRegistry,
    visibility: &crate::visibility::VisibilityLedger,
    viewer: FactionId,
    fog_enabled: bool,
    ladder: &LadderConfig,
    config: &crate::extraction_config::ExtractionConfig,
    // **The live builders kit per queued working, and its membership flag** — read off the bands'
    // queues at capture rather than off the source, because the row's scratch lags a command by a
    // whole turn and the state the countdown separates exists precisely in that frame.
    build_kits: &crate::snapshot::subsistence::BuildKitIds,
    // **The live keeping kit per worked working**, on the same rule one account over.
    upkeep_kits: &crate::snapshot::subsistence::UpkeepKitIds,
    terrain_at: impl Fn(UVec2) -> Option<&'a Tile>,
) -> Vec<sim_runtime::DepositState> {
    registry
        .sources
        .values()
        .filter(|source| {
            !fog_enabled || visibility.is_discovered(viewer, source.tile.x, source.tile.y)
        })
        .filter_map(|source| {
            let tile = source.tile;
            let ground = terrain_at(tile)?;
            let capacity = tile_deposit_capacity(config, &source.material, ground);
            let payoff = deposit_payoff(source.standing(), ladder);
            let measure = deposit_measure(source, ground, config);
            let demand = deposit_keeping_basis(source, measure, ladder);
            let grace = deposit_neglect_grace_remaining(source, ladder);
            let (upkeep_kit_id, upkeep_kit_named) = upkeep_kits.deposit(tile, &source.material);
            Some(sim_runtime::DepositState {
                tile_x: tile.x,
                tile_y: tile.y,
                material: source.material.clone(),
                // **The ladder this deposit is worked by** — the deposit's own, read back off the
                // rung the working stands on, never off the row that named it.
                branch: source.rung().branch().as_str().to_string(),
                stock: source.stock,
                capacity,
                // **What THIS rung can get at**, which is never simply the stock: a rung that cannot
                // reach the whole seam leaves stock it cannot take, and climbing is how you reach
                // deeper.
                reachable: deposit_reachable(source.stock, capacity, &payoff),
                // ⛔ **THE FIELD THAT DECIDES WHICH READOUT THE CLIENT DRAWS**, and it is the
                // ground's rate scaled by the rung — not the branch. A flint scatter and a quarry
                // are both `extraction` and land on opposite sides of it.
                regrowth_rate: tile_deposit_regrowth(config, &source.material, ground)
                    * payoff.regrowth_multiplier,
                // The rung it HOLDS. A client reads this string rather than thresholding the meter
                // beside it, which describes a different rung.
                rung: source.rung().wire_key(),
                build_fraction: deposit_build_fraction(source, ladder),
                ladder_position: source.ladder_position(),
                // **THE TWO READOUTS** — the growth term, and what the crews actually took. Which
                // one a client renders is `regrowth_rate` above; both are published on every row
                // because the sim owns the fork and a client that re-derived it would be a second
                // producer of one verdict.
                sustainable_take: deposit_sustainable_take(source, ground, config, ladder),
                actual_take: source.last_take,
                turns_remaining: deposit_runway(source, ground, config, ladder),
                upkeep_demand: demand,
                upkeep_supplied: source.upkeep_supplied,
                // Derived off the same basis as the demand above, never stored, so the identity
                // cannot be broken by a working nobody stamped a shortfall onto.
                upkeep_shortfall: crate::intensification::upkeep_shortfall(
                    demand,
                    source.upkeep_supplied,
                ),
                upkeep_workers_needed: deposit_upkeep_workers_needed(source, measure, ladder),
                has_neglect_grace: grace.is_some(),
                neglect_grace_remaining: grace.unwrap_or(NO_NEGLECT_REMAINING),
                // ⛔ **THE COUNTDOWN, THROUGH THE SEAM ALL FOUR BRANCHES GO THROUGH** — so a
                // working cannot publish a state as a different number than a patch does. Only a
                // QUEUED working has a real number; a rung nobody ordered has no quote, so this is
                // the honest *no estimate* rather than a `0` that would render as finished.
                build_turns_remaining: crate::snapshot::subsistence::published_build_countdown(
                    source.build_turns_remaining,
                    source.build_queue_position,
                    build_kits.deposit_is_queued(tile, &source.material),
                ),
                // Stamped by the labour pass's `Extract` arm where the quote is struck and cleared
                // by the decay pass on the one-turn cycle, so it describes the turn just resolved.
                build_blocked_reason: source.build_blocked_reason.key().to_string(),
                is_queued: build_kits.deposit_is_queued(tile, &source.material),
                build_kit_id: build_kits.deposit(tile, &source.material),
                upkeep_kit_id,
                upkeep_kit_named,
            })
        })
        .collect()
}
