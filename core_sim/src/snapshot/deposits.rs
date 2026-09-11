use super::*;

use crate::extraction::{
    deposit_build_fraction, deposit_floor_fraction, deposit_keeping_basis, deposit_measure,
    deposit_neglect_grace_remaining, deposit_payoff, deposit_reachable, deposit_regrowth,
    deposit_runway, deposit_sustainable_take, deposit_upkeep_workers_needed, tile_deposit_capacity,
    tile_deposit_regrowth, DepositRegistry, DepositSource,
};
use crate::extraction_config::NO_DEPOSIT;
use crate::snapshot::subsistence::{regrowth_sample_fraction, REGROWTH_CURVE_SAMPLES};

/// **The countdown a working with nothing at risk publishes.** Paired with
/// `has_neglect_grace: false`, which is the field a reader must check — this number is only here
/// because the wire has no optional scalars, and it deliberately reuses the *"biting now"* value
/// rather than inventing a sentinel a client could mistake for a real countdown. The road row's and
/// the subsistence rows' own convention.
const NO_NEGLECT_REMAINING: u32 = 0;

/// **THE DEPOSITS THE VIEWER CAN SEE, ON THE WIRE — ONE ROW PER `(tile, material)`**
/// (`docs/plan_extraction.md` §7, issue #650).
///
/// # A ROW DESCRIBES THE GROUND; THE WORKING IS ITS STATE — THE FORAGE PATCH'S MODEL
///
/// A row is published for **every discovered tile that holds a deposit** — every `(tile, material)`
/// pair whose [`tile_deposit_capacity`] is above [`NO_DEPOSIT`] — and the registry's live
/// [`DepositSource`] is merged in where one exists. That is `snapshot_forage_patches`' shape: a
/// patch row stands on every food-bearing tile whether or not anybody has touched it, because what
/// the row is *about* is the land.
///
/// ⛔ **THE REGISTRY IS NOT SEEDED TO ACHIEVE THIS**, and must not be. An unopened deposit's row is
/// derived here, at capture, from [`DepositSource::opening`] — full stock at the tile's capacity,
/// the branch's free floor, no upkeep, no neglect, no take — and is saved nowhere. So retuning
/// `extraction.json` still reaches it and a checkpoint does not grow a row per land tile per
/// material. The registry stays exactly what it was: the **lazily opened** set of workings a band
/// has actually put a crew on (`extraction::DepositRegistry::open`).
///
/// Publishing only the registry is what made the whole feature unreachable: the client builds its
/// tile-card affordance off these rows, so a fresh world offered no way to open the *first* working
/// anywhere on the map.
///
/// # THE FOG GATE IS `Discovered`, AND IT IS THE ROAD ROW'S RATHER THAN THE HERD ROW'S
///
/// A deposit is published to a faction that has explored **its tile**. Ground you saw two hundred
/// turns ago says nothing about where a *herd* is standing today, which is why `herd_is_visible`
/// demands `Active`; a quarry does not wander off, so remembering one is remembering something
/// true. `route_states`' reading, and for its reason.
///
/// **The whole row passes or none of it does, and no field is redacted individually.** The two food
/// webs publish their rows unfiltered and leave the fog reading to the client
/// (`ForagePatchState::carrying_capacity` states the rule); the road row filters the row and redacts
/// nothing. A worked deposit carries a ladder position a band earned, so it takes the road's
/// discipline, and a field like `ladder_position` never reaches a faction that has not seen the
/// ground it stands on.
///
/// **Fails CLOSED**, the road and herd gates' rule: an absent faction map (before the first
/// `calculate_visibility`, or the turn after a rollback clears the ledger) publishes no row, which
/// is what the all-unexplored raster beside it is already saying.
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
/// **Order is `(y, x, material)`**, `snapshot_forage_patches`' own sort and for its reason: the
/// caller hands over the tiles in whatever order the tile query walked them, and the section is
/// diffed as a whole vector, so it has to be an order and not an accident.
///
/// `ground` is every tile that holds *something* (`extraction::tile_holds_a_deposit`), picked out of
/// the capture's one full tile sweep. A tile the sweep never saw publishes nothing rather than a row
/// at zero: every number on the row is a function of that ground, so a row without it would state a
/// capacity of nothing, a runway off it and an empty rate — three false readings where the honest
/// answer is that the sim cannot see the tile.
#[allow(clippy::too_many_arguments)] // the registry, the fog pair, both configs, both kit indexes
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
    ground: impl Iterator<Item = &'a Tile>,
) -> Vec<sim_runtime::DepositState> {
    let mut rows: Vec<sim_runtime::DepositState> = ground
        .filter(|tile| {
            !fog_enabled || visibility.is_discovered(viewer, tile.position.x, tile.position.y)
        })
        .flat_map(|tile| {
            config.deposits().filter_map(move |(material, deposit)| {
                let capacity = tile_deposit_capacity(config, material, tile);
                if capacity <= NO_DEPOSIT {
                    return None;
                }
                // **THE DERIVED OPENING STATE, NOT A SEEDED ONE** — held in a local so the row can
                // borrow it, and dropped the moment the row is built. A live working in the
                // registry always wins: a derivation must never overwrite a stock a crew moved.
                let unopened;
                let source = match registry.source(tile.position, material) {
                    Some(live) => live,
                    None => {
                        unopened = DepositSource::opening(
                            tile.position,
                            material,
                            capacity,
                            deposit.branch,
                        );
                        &unopened
                    }
                };
                Some(deposit_row(
                    source,
                    tile,
                    capacity,
                    ladder,
                    config,
                    build_kits,
                    upkeep_kits,
                ))
            })
        })
        .collect();
    rows.sort_unstable_by(|left, right| {
        (left.tile_y, left.tile_x)
            .cmp(&(right.tile_y, right.tile_x))
            .then_with(|| left.material.cmp(&right.material))
    });
    rows
}

/// **THE DEPOSIT'S OWN PER-TURN REGROWTH, SAMPLED ACROSS ITS CAPACITY** — the third curve on the
/// wire beside `patch_regrowth_samples` / `herd_regrowth_samples`, on the same implicit x-axis
/// ([`regrowth_sample_fraction`]) so one client interpolation serves all three.
///
/// Each entry is a **delta in the material's own units**: what one Logistics pass adds at that
/// standing stock, through [`deposit_regrowth`] — the same seam `renew_deposit` advances the stock
/// with, at the rung's own scaled rate, so a coppiced wood's curve is the one its rung bought.
///
/// ⛔ **A QUARRY'S SAMPLES ARE ALL ZERO, AND THAT IS THE HONEST ANSWER RATHER THAN AN ABSENCE.**
/// Rock's rate is `NEVER_RENEWS`, so the delta is exactly `0` at every reading point and the client
/// draws a flat curve — *this does not grow*. An empty vector would say *no curve was sent*, which
/// is a different claim and the one a client blanks its chart on.
///
/// **No sample is ever negative**: a deposit has no Allee term, so it is the plant curve's shape
/// rather than the herd curve's, and the seeded reading inside `deposit_regrowth` is what lets a
/// wood cut clean come back while leaving rock at zero.
fn deposit_regrowth_samples(
    source: &DepositSource,
    ground: &Tile,
    capacity: f32,
    config: &crate::extraction_config::ExtractionConfig,
    ladder: &LadderConfig,
) -> Vec<f32> {
    let payoff = deposit_payoff(source.standing(), ladder);
    let rate = tile_deposit_regrowth(config, &source.material, ground) * payoff.regrowth_multiplier;
    (0..REGROWTH_CURVE_SAMPLES)
        .map(|index| {
            let standing = regrowth_sample_fraction(index) * capacity;
            deposit_regrowth(standing, capacity, rate, config.seed_fraction) - standing
        })
        .collect()
}

/// **ONE `(tile, material)` ROW**, off whichever [`DepositSource`] the caller resolved — the live
/// working where a band opened one, and the derived opening state where none has been.
///
/// It is one function precisely so the two cannot diverge: an unopened deposit publishes the same
/// derivations, through the same seams, as a working standing on its branch's free floor. `capacity`
/// is passed in rather than re-read because the caller struck it to decide the row exists at all.
fn deposit_row(
    source: &DepositSource,
    ground: &Tile,
    capacity: f32,
    ladder: &LadderConfig,
    config: &crate::extraction_config::ExtractionConfig,
    build_kits: &crate::snapshot::subsistence::BuildKitIds,
    upkeep_kits: &crate::snapshot::subsistence::UpkeepKitIds,
) -> sim_runtime::DepositState {
    let tile = source.tile;
    let payoff = deposit_payoff(source.standing(), ladder);
    let measure = deposit_measure(source, ground, config);
    let demand = deposit_keeping_basis(source, measure, ladder);
    let grace = deposit_neglect_grace_remaining(source, ladder);
    // **THE GROUND'S OWN RATE, UN-SCALED** — the reading `deposit_effective_floor` forks the crew's
    // floor on and `deposit_runway` forks the readout on. The field published below scales it by the
    // rung; these are two different questions off one number, so it is read once.
    let regrowth_rate = tile_deposit_regrowth(config, &source.material, ground);
    // **A tile nobody has worked is in no queue and carries no kit** — both indexes are keyed
    // `(tile, material)` and answer the empty/false default for a key they do not hold, which is
    // the honest reading rather than a fabricated one.
    let (upkeep_kit_id, upkeep_kit_named) = upkeep_kits.deposit(tile, &source.material);
    sim_runtime::DepositState {
        tile_x: tile.x,
        tile_y: tile.y,
        material: source.material.clone(),
        // **The ladder this deposit is worked by** — the deposit's own, read back off the rung the
        // working stands on, never off the row that named it.
        branch: source.rung().branch().as_str().to_string(),
        stock: source.stock,
        capacity,
        // **What the CREWS ON IT can get at**, which is never simply the stock: a rung that cannot
        // reach the whole seam leaves stock it cannot take, climbing is how you reach deeper — and
        // since #650 a crew told to leave more standing than the rung already cannot reach binds
        // instead — **on a renewing deposit**. `deposit_reachable` composes the pair as a maximum
        // and drops the crew's half at `NEVER_RENEWS`, and it is the only place either is done, so
        // the published reach and the take a quarry gets cannot disagree.
        reachable: deposit_reachable(
            source.stock,
            capacity,
            regrowth_rate,
            &payoff,
            source.escapement_floor(),
        ),
        // **THE RUNG'S OWN FLOOR, AS A FRACTION OF CAPACITY**, so a client can compose it with the
        // player's exactly as the sim does. ⛔ **THEY ARE A MAXIMUM, NEVER A SUM**: a chart that
        // added them would draw a crew stopping 85% of a seam short of where it really stops on the
        // gathering rung. **The player's half is not on this row** — it is `LaborAssignmentState::
        // floor`, per band row; the source-level restatement of where the stock came to rest was
        // published here and read by nobody, and `reachable` above already composes the pair.
        rung_floor_fraction: deposit_floor_fraction(&payoff),
        // **WHAT ONE CUTTER MOVES PER TURN AT THIS RUNG**, in the material's own units — the deposit
        // twin of `ForagePatchState::per_worker_biomass` and named after it, because the client's
        // crew arithmetic (*clear it now* / *hold it after*) is the same division on either web.
        // There is no seasonal weight and no kit term on this branch, so it is the rung's rate flat.
        per_worker_biomass: payoff.yield_per_worker_turn,
        // **THIS DEPOSIT'S OWN GROWTH CURVE, SAMPLED** — the third model on the wire's one
        // x-axis, and a **quarry's is all zeros** rather than absent: *this does not grow* is a
        // different claim from *no curve was sent*, and the rate is what a client forks on.
        regrowth_samples: deposit_regrowth_samples(source, ground, capacity, config, ladder),
        // ⛔ **THE FIELD THAT DECIDES WHICH READOUT THE CLIENT DRAWS**, and it is the ground's rate
        // scaled by the rung — not the branch. A flint scatter and a quarry are both `extraction`
        // and land on opposite sides of it.
        regrowth_rate: regrowth_rate * payoff.regrowth_multiplier,
        // The rung it HOLDS. A client reads this string rather than thresholding the meter beside
        // it, which describes a different rung.
        rung: source.rung().wire_key(),
        build_fraction: deposit_build_fraction(source, ladder),
        ladder_position: source.ladder_position(),
        // **THE TWO READOUTS** — the growth term, and what the crews actually took. Which one a
        // client renders is `regrowth_rate` above; both are published on every row because the sim
        // owns the fork and a client that re-derived it would be a second producer of one verdict.
        sustainable_take: deposit_sustainable_take(source, ground, config, ladder),
        actual_take: source.last_take,
        turns_remaining: deposit_runway(source, ground, config, ladder),
        upkeep_demand: demand,
        upkeep_supplied: source.upkeep_supplied,
        // Derived off the same basis as the demand above, never stored, so the identity cannot be
        // broken by a working nobody stamped a shortfall onto.
        upkeep_shortfall: crate::intensification::upkeep_shortfall(demand, source.upkeep_supplied),
        upkeep_workers_needed: deposit_upkeep_workers_needed(source, measure, ladder),
        has_neglect_grace: grace.is_some(),
        neglect_grace_remaining: grace.unwrap_or(NO_NEGLECT_REMAINING),
        // ⛔ **THE COUNTDOWN, THROUGH THE SEAM ALL FOUR BRANCHES GO THROUGH** — so a working cannot
        // publish a state as a different number than a patch does. Only a QUEUED working has a real
        // number; a rung nobody ordered has no quote, so this is the honest *no estimate* rather
        // than a `0` that would render as finished.
        build_turns_remaining: crate::snapshot::subsistence::published_build_countdown(
            source.build_turns_remaining,
            source.build_queue_position,
            build_kits.deposit_is_queued(tile, &source.material),
        ),
        // Stamped by the labour pass's `Extract` arm where the quote is struck and cleared by the
        // decay pass on the one-turn cycle, so it describes the turn just resolved.
        build_blocked_reason: source.build_blocked_reason.key().to_string(),
        is_queued: build_kits.deposit_is_queued(tile, &source.material),
        build_kit_id: build_kits.deposit(tile, &source.material),
        upkeep_kit_id,
        upkeep_kit_named,
    }
}
