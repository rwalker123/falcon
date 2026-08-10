//! **The forecast query channel's answering half** — the sim side of the one client message that is
//! *answered* rather than *applied*.
//!
//! # Why a query at all, when the snapshot already ships estimate tables
//!
//! It ships the wrong ones. `snapshot::hunt_trip_estimate_entries` and
//! `snapshot::denial_estimate_entries` sample **floors × party sizes**, per herd, every frame — and
//! they can only do that by fixing everything else:
//!
//! - **One kit for every band**: the hunt job's *default*. A player who picked the trapping kit is
//!   reading rows priced for spears, and the error is total rather than marginal — a mass-bounded
//!   weapon against a quarry outside its window grants no attack at all.
//! - **A FRESH component set** ([`BandEquipment::default`] is zero wear). A band whose spears have
//!   run dry hunts at the intrinsic `attack 1` — effective attack **zero** against a Red Deer's
//!   `defense 1.0` — while the table quotes it a working party (`expeditions.md`).
//! - **Marks on a dial**, not the player's numbers: the client resolves its composed floor and party
//!   to the nearest sampled rung and quotes *that* row.
//!
//! A query fixes none of those, because it does not have to: it is asked for one exact
//! (band, kit, party, floor) and answered from the **live** world, this band's own wear included.
//! That is why [`answer_forecast_query`] resolves the party through
//! [`crate::equipment_config::EquipmentConfig::hunter_profile_against`] — against *this* quarry's
//! body mass — rather than through the tables' quarry-blind `hunter_profile_unbounded`.
//!
//! **And it fights at the EXPEDITION's lethality.** The tables priced a detached raid at
//! resident-hunt lethality, which under-states its casualties and therefore over-states its take;
//! this resolves the party through
//! [`crate::combat_config::CombatConfig::expedition_tuning`], the one constructor
//! `advance_expeditions` now also resolves the live raid through. See [`query_hunting_party`].
//!
//! # Nothing here mutates the world
//!
//! Every entry point is a read. `&mut World` appears only because Bevy's `World::query` caches its
//! own state behind `&mut self`; no game state is touched, which is precisely why the dispatcher
//! keeps a query out of the replay log.
//!
//! # Fails closed, with a token
//!
//! An unknown herd, an unknown band, an unknown or wrong-job kit, an out-of-range floor and an empty
//! party are all [`QueryReply::Error`]s carrying a [`sim_runtime::commands::query_error`] token.
//! **A kit is never quietly swapped for the job default** — the same rule the launch commands
//! follow, and for the same reason: a party silently re-armed answers a different question than the
//! one asked.

use bevy::prelude::World;
use sim_runtime::commands::{
    query_error, DenialRaidForecastQuery, DenialRaidForecastReply, DenialRow,
    HuntTripForecastQuery, HuntTripForecastReply, HuntTripRow, QueryPayload, QueryReply,
};

use crate::combat_config::CombatConfigHandle;
use crate::components::{floor_is_valid, BandEquipment, BandId};
use crate::creatures_config::CreaturesConfigHandle;
use crate::equipment_config::{EquipmentConfig, EquipmentConfigHandle, KitJob};
use crate::expedition_config::{ExpeditionConfig, ExpeditionConfigHandle};
use crate::fauna::{Herd, HerdRegistry, HuntingParty};
use crate::fauna_config::{FaunaConfig, FaunaConfigHandle};
use crate::labor_config::LaborConfigHandle;
use crate::orders::FactionId;
use crate::systems::{denial_forecast, hunt_trip_forecast};
use crate::PopulationCohort;

/// **The whole query surface**: resolve what the ask names, or refuse with a token.
///
/// `&mut World` is a Bevy artefact, not an intent — see the module docs.
pub fn answer_forecast_query(world: &mut World, query: &QueryPayload) -> QueryReply {
    match query {
        QueryPayload::HuntTripForecast(ask) => answer_hunt_trip_forecast(world, ask),
        QueryPayload::DenialRaidForecast(ask) => answer_denial_raid_forecast(world, ask),
    }
}

/// Everything an answer is computed from, resolved once out of the live world. Both verbs resolve
/// the *same* four things — herd, band wear, kit, party — so they resolve them through one function
/// and cannot drift into two readings of "which band is asking".
struct ResolvedAsk {
    herd: Herd,
    party: HuntingParty,
    /// The chosen kit's per-hunter haul (sled) tier at this band's live wear.
    per_worker_haul: f32,
}

/// Resolve a query's (band, herd, kit, party) against the live world, or answer with the token that
/// says which one failed.
///
/// **The band's LIVE [`BandEquipment`] is what prices it.** A band is entitled to be told what *its*
/// gear can do, and the query is the first thing on this wire that can say — the per-herd tables
/// have one row for every band by construction.
fn resolve_ask(
    world: &mut World,
    faction_id: u32,
    band_id: u64,
    herd_id: &str,
    kit_id: &str,
    party_workers: u32,
) -> Result<ResolvedAsk, QueryReply> {
    // **A party of nobody has no raid to project.** The launch verbs refuse it too; a forecast for
    // zero hunters is not a cheap answer, it is a meaningless one.
    if party_workers == 0 {
        return Err(query_failure(query_error::INVALID_PARTY));
    }

    // The herd, cloned: the projections run on a private copy of the quarry anyway, and holding a
    // borrow of the registry across the resource reads below would fight the borrow checker for
    // nothing.
    let Some(herd) = world
        .get_resource::<HerdRegistry>()
        .and_then(|registry| registry.find(herd_id).cloned())
    else {
        return Err(query_failure(query_error::UNKNOWN_HERD));
    };

    let Some(wear) = band_equipment(world, FactionId(faction_id), band_id) else {
        return Err(query_failure(query_error::UNKNOWN_BAND));
    };

    let equipment = world.resource::<EquipmentConfigHandle>().get();
    // **Named, and never defaulted.** `resolve_kit_for_job(None, ..)` is the "player named no kit"
    // path and answers the job default — which is exactly the silent substitution a query must not
    // make, so the id is always passed through as `Some`.
    let kit = match equipment.resolve_kit_for_job(Some(kit_id), KitJob::Hunt) {
        Ok(kit) => kit,
        Err(crate::equipment_config::KitSelectionError::Unknown { .. }) => {
            return Err(query_failure(query_error::UNKNOWN_KIT))
        }
        Err(crate::equipment_config::KitSelectionError::WrongJob { .. }) => {
            return Err(query_failure(query_error::KIT_WRONG_JOB))
        }
    };

    // **How this band's gear divides the party it is asking about** — resolved once and read by
    // both halves below, so the fight it is quoted and the haul it is quoted describe the same
    // people (`equipment.md` → "the partly-equipped party").
    let coverage = equipment.coverage(&kit, party_workers as f32, &wear);
    let party = query_hunting_party(world, &equipment, &coverage, &wear, herd.body_mass);
    let per_worker_haul = query_per_worker_haul(world, &equipment, &coverage, &wear);
    Ok(ResolvedAsk {
        herd,
        party,
        per_worker_haul,
    })
}

/// The asking band's live wear ledger — `None` if no band of `faction` carries `band_id`.
///
/// **A band with no [`BandEquipment`] component reads as ZERO WEAR, not as "no such band."** That is
/// the component's own convention (`Default` is an empty ledger, which is what makes a band start
/// kitted for free), so a band that has simply never worn anything must answer like a fresh one
/// rather than fall through to a refusal.
fn band_equipment(world: &mut World, faction: FactionId, band_id: u64) -> Option<BandEquipment> {
    let wanted = BandId(band_id);
    let mut query = world.query::<(bevy::prelude::Entity, &BandId, &PopulationCohort)>();
    let entity = query
        .iter(world)
        .find(|(_, id, cohort)| **id == wanted && cohort.faction == faction)
        .map(|(entity, _, _)| entity)?;
    Some(
        world
            .get::<BandEquipment>(entity)
            .cloned()
            .unwrap_or_default(),
    )
}

/// **The party the answer is quoted for** — the same four `equipment.*` seams
/// `snapshot::kit_roster_states` resolves a kit's published tiers through, but over the asking
/// band's **live** wear and against **this** quarry.
///
/// Three deliberate differences from the roster's fresh-kit statement, and they are the query's whole
/// point:
///
/// - `wear` is the band's own, so a worn-out kit is priced at the tier it actually delivers.
/// - the attack resolves through `hunter_profile_against` rather than `hunter_profile_unbounded`,
///   because a mass-bounded weapon is only a weapon against animals it can hold. The roster is
///   quarry-blind because it has no quarry; a query always has one, so quoting the kit's best case
///   here would promise a trapping party a Red Deer it cannot touch.
/// - **the tuning is the EXPEDITION's**, not the base.
///
/// # The `expedition_danger_multiplier` belongs here, and the tables were wrong to omit it
///
/// A hunting party takes casualties like a resident band but **bloodier** — far from home and
/// unsupported, so the same beast costs it more. `advance_expeditions` scales `lethality` by
/// `combat.expedition_danger_multiplier` when it resolves a live raid, and both the launch line and
/// the in-flight ETA scale it too, explicitly so the ETA and the turn agree.
///
/// The per-herd estimate tables did **not**: they priced a detached raid at *resident-hunt*
/// lethality. That under-states casualties, and therefore over-states the take and under-states the
/// fill time, on every expedition and every denial raid quoted. It is not a second defensible
/// reading — [`hunt_trip_forecast`] is only ever consumed on expedition branches, so there is no
/// caller for which the base tuning is correct.
///
/// Applied here **the same way `advance_expeditions` applies it** (scale `lethality` on the config's
/// own `tuning()`), so a retune moves the forecast and the turn together. The pairing is held by a
/// test rather than by this comment.
fn query_hunting_party(
    world: &World,
    equipment: &EquipmentConfig,
    coverage: &crate::equipment_config::KitCoverage,
    wear: &BandEquipment,
    quarry_body_mass: f32,
) -> HuntingParty {
    let combat = world.resource::<CombatConfigHandle>().get();
    let intrinsic = world.resource::<CreaturesConfigHandle>().get().person();
    crate::fauna::PartyResolution {
        equipment,
        coverage,
        wear,
        intrinsic,
        tuning: combat.expedition_tuning(),
        hunt_injury_damage_per_animal: combat.hunt_injury_damage_per_animal,
    }
    .party_against(crate::equipment_config::Quarry::Mass(quarry_body_mass))
}

/// The haul half of [`query_hunting_party`] — the kit's *sled* tier at the band's live wear. Both
/// halves have to move together: a bare-handed fight beside a kitted haul would promise a party that
/// kills nothing and drags it home fast.
fn query_per_worker_haul(
    world: &World,
    equipment: &EquipmentConfig,
    coverage: &crate::equipment_config::KitCoverage,
    wear: &BandEquipment,
) -> f32 {
    let equipped_rate = world
        .resource::<LaborConfigHandle>()
        .get()
        .hunt
        .per_worker_biomass_capacity;
    // **Weighted across the crews** — a party short of sleds hauls at the mean of what its people
    // are actually dragging, not at the tier the best-equipped of them has.
    coverage
        .weighted_rate(|kit| equipment.hunt_per_worker_biomass_capacity(equipped_rate, kit, wear))
}

/// Answer a hunt-trip forecast: the composed floor, then every preset floor, at the same party.
///
/// **Every floor is validated before any is answered**, so a bad preset cannot come back as a
/// half-filled reply whose row order no longer matches the presets that were asked for.
fn answer_hunt_trip_forecast(world: &mut World, ask: &HuntTripForecastQuery) -> QueryReply {
    if !floor_is_valid(ask.floor) || !ask.preset_floors.iter().copied().all(floor_is_valid) {
        return query_failure(query_error::INVALID_FLOOR);
    }
    let resolved = match resolve_ask(
        world,
        ask.faction_id,
        ask.band_id,
        &ask.herd_id,
        &ask.kit_id,
        ask.party_workers,
    ) {
        Ok(resolved) => resolved,
        Err(failure) => return failure,
    };

    let fauna = world.resource::<FaunaConfigHandle>().get();
    let expedition = world.resource::<ExpeditionConfigHandle>().get();
    let row = |floor: f32| hunt_trip_row(floor, ask.party_workers, &resolved, &fauna, &expedition);
    QueryReply::HuntTripForecast(HuntTripForecastReply {
        at_composed: row(ask.floor),
        per_preset: ask.preset_floors.iter().copied().map(row).collect(),
        useful_cap: useful_party_cap(
            ask.floor,
            ask.max_party_workers,
            &resolved,
            &fauna,
            &expedition,
        ),
    })
}

/// One [`hunt_trip_forecast`] call, shaped for the wire. The `floor` / `party_workers` echo is
/// deliberate: it makes the row self-describing, so a client asserts the answer is for what it asked
/// instead of trusting its position in a list.
fn hunt_trip_row(
    floor: f32,
    party_workers: u32,
    resolved: &ResolvedAsk,
    fauna: &crate::fauna_config::FaunaConfig,
    expedition: &ExpeditionConfig,
) -> HuntTripRow {
    let forecast = hunt_trip_forecast(
        party_workers,
        &resolved.herd,
        floor,
        fauna,
        resolved.per_worker_haul,
        expedition,
        &resolved.party,
    );
    HuntTripRow {
        floor,
        party_workers,
        turns_to_fill: forecast.turns_to_fill.unwrap_or(NEVER_FILLED),
        bound: forecast.bound.as_str().to_string(),
        delivers_food: forecast.delivers_food,
        delivers_trade: forecast.delivers_trade,
        animals_taken: forecast.animals_taken,
        delivered_food: forecast.delivered_food,
        delivered_trade: forecast.delivered_trade,
        wasted_food: forecast.wasted_food,
    }
}

/// **The max-useful party plateau**, ported from the client's `SourceForecast.expedition_useful_cap`
/// table scan — which cannot survive the table it scanned.
///
/// The delivered payload **plateaus** with party size once the standing surplus (rather than the
/// pack) binds, so past the plateau extra hunters raise the take by nothing. Walk the sampled party
/// ladder ascending at the composed floor and return the last size at which the payload was still
/// rising.
///
/// Three properties of the original are load-bearing and are kept:
///
/// - **It scans the DELIVERED payload, not `animals_taken`.** The whole-animal count sits at `1`
///   across every small party on big game, and that leading-zeros plateau capped the sheet at one
///   hunter; delivered payload rises smoothly because a party too small to haul its kill whole still
///   lands a partial.
/// - **It scans the component this QUARRY pays.** An inedible species delivers `0` food at every
///   size, so a food-only scan finds no plateau at all on exactly the quarry whose payload is trade.
/// - **A payload that never rises above zero is not a plateau.** A raid every quoted party comes home
///   empty from is *flat at zero*, and reading that flatness as "the first size was enough" is how
///   the sheet came to say *"max 1 worker useful"* about a party that kills nothing.
///
/// # It walks `1..=max_party_workers` CONTIGUOUSLY, and that is the point
///
/// The scan used to walk `expedition_config.estimate_party_sizes`, a **sampled ladder**
/// (`1, 2, 3, 4, 8, 16, 32, 64` as shipped) — and a sampled scan finds a sampled plateau. It could
/// only ever answer *"the rung after which the payload stopped rising"*: a herd whose true plateau
/// was 6 reported 4, and the sheet told the player six hunters were three too many.
///
/// **The ladder existed to make a pre-computed TABLE affordable, and nothing else.** Every rung was
/// a row the capture paid for on every huntable herd on every frame, so the axis was sparse where it
/// was expensive. A query answers one herd for one band when a player asks, so the sampling that
/// bought that affordability buys nothing — and the ladder is gone with it.
///
/// The bound is the **band's own idle workers**, which the client already knows and already caps its
/// stepper at. That is the honest ceiling: a plateau above what the band could field is not a fact
/// the player can act on, and scanning past it would be work spent to report a party that cannot be
/// sent. `0` scans nothing.
///
/// Returns the SCAN only. The engagement-crew floor the client maxes into it is derived from fields
/// the herd row already carries, so it stays client-side with the prose that explains it. `0` = no
/// plateau found, which the client reads as "no usefulness cap to name".
fn useful_party_cap(
    floor: f32,
    max_party_workers: u32,
    resolved: &ResolvedAsk,
    fauna: &crate::fauna_config::FaunaConfig,
    expedition: &ExpeditionConfig,
) -> u32 {
    let mut previous = NO_PAYLOAD_YET;
    let mut plateau = NO_USEFUL_CAP;
    for party_workers in 1..=max_party_workers {
        let row = hunt_trip_row(floor, party_workers, resolved, fauna, expedition);
        let delivered = if row.delivers_food {
            row.delivered_food
        } else {
            row.delivered_trade
        };
        if delivered > previous {
            previous = delivered;
            if delivered > 0.0 {
                plateau = party_workers;
            }
        } else {
            break;
        }
    }
    plateau
}

/// **The wire's "no usefulness cap to name"** on `HuntTripForecastReply::useful_cap` — the scan found
/// no plateau, because the payload never rose above zero, or was still rising at the band's last
/// fieldable worker, or no scan was asked for. Named because a bare `0` beside a party count reads
/// as *"send nobody"*, which it is not.
const NO_USEFUL_CAP: u32 = 0;

/// The plateau scan's seed: **below any payload a raid can deliver**, including a delivered `0`, so
/// the first party is compared against "nothing has been seen yet" rather than against a real
/// reading. A `0.0` seed would make an all-empty scan's first party fail the rise test and break the
/// walk before it starts.
const NO_PAYLOAD_YET: f32 = -1.0;

/// Answer a denial-raid forecast: the exact party the query names, plus the party the sheet opens on.
///
/// **Two evaluations, because they answer two questions.** `at_composed` is the single-point
/// projection at the party the player has dialled; `party_needed` is a property of the *herd* against
/// this kit — the smallest quoted party whose own row succeeds — and it can only be read off the
/// quoted axis, which is what [`denial_estimate_entries`] builds. Recomputing it from the closed form
/// instead would reintroduce exactly the bug that seam exists to prevent: the closed form is linear in
/// the party and therefore blind to the whole-animal quantiser, to the fight, and to the engagement
/// floor.
fn answer_denial_raid_forecast(world: &mut World, ask: &DenialRaidForecastQuery) -> QueryReply {
    let resolved = match resolve_ask(
        world,
        ask.faction_id,
        ask.band_id,
        &ask.herd_id,
        &ask.kit_id,
        ask.party_workers,
    ) {
        Ok(resolved) => resolved,
        Err(failure) => return failure,
    };

    let fauna = world.resource::<FaunaConfigHandle>().get();
    let expedition = world.resource::<ExpeditionConfigHandle>().get();
    // The reported band's width — a readout lever, so widening it cannot move an animal.
    let range_sigmas = world
        .resource::<CombatConfigHandle>()
        .get()
        .forecast_range_sigmas;

    let forecast = denial_forecast(
        ask.party_workers,
        &resolved.herd,
        &fauna,
        resolved.per_worker_haul,
        &expedition,
        &resolved.party,
        range_sigmas,
    );
    let party_needed = seeded_denial_party_for(
        &resolved.herd,
        &fauna,
        &expedition,
        &resolved.party,
        resolved.per_worker_haul,
        range_sigmas,
        ask.max_party_workers,
    );

    QueryReply::DenialRaidForecast(DenialRaidForecastReply {
        at_composed: DenialRow {
            party_workers: ask.party_workers,
            turns_to_collapse: forecast.turns_to_collapse.unwrap_or(NEVER_PAST_RECOVERY),
            turns_to_collapse_low: forecast
                .turns_to_collapse_low
                .unwrap_or(NEVER_PAST_RECOVERY),
            turns_to_collapse_high: forecast
                .turns_to_collapse_high
                .unwrap_or(NEVER_PAST_RECOVERY),
            outcome: forecast.outcome.as_str().to_string(),
            animals_killed: forecast.animals_killed,
            delivered_food: forecast.delivered_food,
            wasted_food: forecast.wasted_food,
            delivered_trade: forecast.delivered_trade,
            wasted_trade: forecast.wasted_trade,
        },
        party_needed,
    })
}

/// A refusal, as its token. One constructor so the reply shape cannot drift between the seven
/// failure paths.
fn query_failure(reason: &str) -> QueryReply {
    QueryReply::Error(reason.to_string())
}

// ===========================================================================================
// THE DENIAL AXIS, rehomed from `snapshot::subsistence`
// ===========================================================================================
//
// **This used to be published, per herd, every frame.** It was the pre-launch denial table: one
// simulated raid per quoted party size, three quantiles deep, for every huntable herd on the map —
// and the client looked its answer up in it. Together with the hunt table beside it that was 46 ms
// of a 49 ms capture on a fully-revealed 80x52 map with 128 huntable herds.
//
// **Only `party_needed` survived the move, and only because it cannot be computed any other way.**
// The seeded party is the smallest quoted party whose *own row* succeeded, read off a forward
// simulation rather than off the closed form — see [`seeded_denial_party`] for why the closed form
// is a bound on the search and not the answer. So the search still runs; it just runs once, for one
// herd, when a player asks, instead of 128 times a turn for nobody.

/// **The wire's "this raid never fills the pack"** — the `0` sentinel on
/// [`sim_runtime::commands::HuntTripRow::turns_to_fill`]. Named for the reason its denial twin
/// [`NEVER_PAST_RECOVERY`] is: a bare `0` beside a turn count reads as *"immediately"*, which is the
/// opposite of what it means; the row's `bound` carries the reason.
///
/// **It is horizon-relative, and the scale it is relative to rides the wire** as
/// `PopulationCohortState::expedition_forecast_horizon_turns` — so a client can say *"more than N
/// turns"* rather than *"many"*. Read that field's doc before quoting it: the horizon bounds the
/// **hunting** only, and the trip's floor is `horizon + round-trip travel`.
const NEVER_FILLED: u32 = 0;

/// **The party the launch sheet opens on** — the smallest party that genuinely drives this herd past
/// recovery, found by walking `1..=max_party_workers` and stopping at the first one that
/// **succeeds** (`docs/plan_denial_raid.md` §3.1).
///
/// # The test is success, not "not repelled"
///
/// [`crate::DenialOutcome::succeeded`] is `past_recovery` or `herd_lost`. Those two read the same as
/// `!= Repelled` on three of the four verdicts and differ on the one that matters: a
/// [`crate::DenialOutcome::Horizon`] result is a raid the projection ran to its whole length with the
/// herd still standing, so it demonstrates nothing the sim will vouch for. Seeding there quoted a
/// Wild Aurochs party of 5 under the verdict *"still standing when the forecast runs out"* — a
/// number presented as the answer, one short of one.
///
/// # There WAS a closed form, and it is gone — deleted rather than left beside this
///
/// `fauna::denial_party_needed` computed the requirement as
/// `floor(replacement_animals / (engage_rate × (1 − wariness))) + 1`, and it sized the sampled axis
/// this search replaced. It is **deleted**, not kept as a fast path or a starting hint: a `pub fn`
/// returning a *linear approximation* of a number the sim now answers exactly is an invitation to
/// call the wrong one. What it knew is worth keeping, because it is why this walks a projection.
///
/// **It erred in BOTH directions, which is what makes it unusable as an answer.**
///
/// - **Too low.** It is linear in the party, so it sees neither the whole-animal quantiser (a raid
///   kills whole animals, not fractions) nor the fight — a party has to *land* its strikes, and a
///   quarry's `defense` and `durability` decide how many turns each kill takes.
/// - **Too high.** It is also blind to [`crate::fauna::animals_engaged`]'s `max(1)` floor, which
///   lets a lone hunter reach **one** mammoth where the arithmetic reads `0.05` — the closed form
///   asks for a crowd against a quarry one person can in fact start working.
///
/// **And the number it divided is subtler than "the herd's regrowth".** The replacement a raid must
/// out-kill is the **peak on the path down**, not the rate where the herd stands now: the logistic
/// curve peaks at the food peak, so a herd above `K/2` regrows *faster* as the raid draws it down. A
/// party sized on a full herd's instantaneous regrowth — which is **zero** — reads *one hunter*,
/// drives the herd to `K/2`, and stalls there forever. Below `K/2` the current stock binds instead
/// and the raid accelerates as it works. The forward simulation gets all of this for free, because
/// it *is* the curve: each projected turn is the same `regrow_biomass` + take pair the live raid
/// runs.
///
/// **The rounding question disappears with it.** The closed form had to round, and had to round the
/// right way: `floor(x) + 1`, never `ceil(x)`, because a party whose kills exactly *tie* with the
/// replacement declines nothing, and `ceil` is wrong by one at precisely the round value a tuner is
/// most likely to author (the reported Red Deer: `2.91 / 0.35 = 8.3` hunters, so **nine**). A search
/// over whole parties never rounds — it asks each one whether it succeeded, and a tie does not.
///
/// # Contiguous, because it can afford to be
///
/// The retired form sampled: the shared `estimate_party_sizes` ladder plus a short run of
/// `deny.requirement_rows` parties around the closed form, so the seed could only be a **sampled**
/// party — either a ladder rung or one of the few rows near a requirement that was not the answer.
/// Both existed to bound a table the capture built for every huntable herd on every frame. A query
/// runs once, for one herd, when a player asks, so it walks every party.
///
/// **`max_party_workers` is the band's own idle workers**, which is also where the sheet's stepper
/// stops. Bounding there is what terminates the walk, and it changes what the sentinel means for the
/// better: [`NO_VIABLE_DENIAL_PARTY`] now says *"no party you can field drives this herd down"* —
/// something the player can act on — where the retired field could name a party the band had no
/// hope of raising and call that an answer.
///
/// **The walk is short in the case that matters and long only where it must be.** It stops at the
/// first success, so a deniable herd costs a handful of projections; a herd nothing can deny costs
/// the whole range, which is exactly the answer that has to be earned.
fn seeded_denial_party_for(
    herd: &Herd,
    fauna: &FaunaConfig,
    expedition: &ExpeditionConfig,
    // **The ASKING BAND's party**, at its own kit and its own wear — the whole difference between
    // this and the table it replaced, which quoted one default-kit party to every band on the map.
    party: &HuntingParty,
    // That same party's per-hunter haul tier.
    per_worker_haul: f32,
    // `combat_config.forecast_range_sigmas` — the reported band's width, a readout lever.
    range_sigmas: f32,
    // The largest party the asking band could field.
    max_party_workers: u32,
) -> u32 {
    (1..=max_party_workers)
        .find(|party_workers| {
            crate::systems::denial_forecast(
                *party_workers,
                herd,
                fauna,
                per_worker_haul,
                expedition,
                party,
                range_sigmas,
            )
            .outcome
            .succeeded()
        })
        .unwrap_or(NO_VIABLE_DENIAL_PARTY)
}

/// **"No party this band can field drives this herd down"** — the `0` on
/// `DenialRaidForecastReply::party_needed`. Named because a bare `0` beside a party count reads as
/// *"send nobody"*, which is not what it means; the answered row's `outcome` carries the reason.
///
/// Three situations reach it and all three are honest: a quarry nothing can bring into contact
/// (`wariness >= 1`), a herd whose regrowth out-runs every party the band could raise, and a band
/// simply too small for this quarry. The client renders the verdict beside it, never a blank.
const NO_VIABLE_DENIAL_PARTY: u32 = 0;

/// **The wire's "this party never gets there"** — the `0` sentinel on
/// [`sim_runtime::commands::DenialRow::turns_to_collapse`] and its two range ends. Named because a
/// bare `0` beside
/// a turn count reads as *"immediately"*, which is the opposite of what it means; the row's `outcome`
/// carries the reason.
///
/// **The denial forecast runs over the SAME horizon the hunt forecast does** —
/// `denial_projection_at` and `hunt_trip_forecast_seeded` both read
/// `expedition_config.hunt.forecast_horizon_turns` — so the one published lever
/// `PopulationCohortState::expedition_forecast_horizon_turns` is the scale for this sentinel and for
/// [`NEVER_FILLED`] alike, and no second horizon belongs on the wire.
const NEVER_PAST_RECOVERY: u32 = 0;

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::UVec2;
    use bevy::prelude::Entity;

    use crate::components::LocalStore;
    use crate::fauna_config::SizeClass;
    use crate::scalar::{scalar_from_f32, scalar_one, scalar_zero};

    const FACTION: FactionId = FactionId(0);
    const BAND: u64 = 7;
    const HERD: &str = "game_query";
    /// The hunt job's shipped default — the kit both retired estimate tables were quoted at, so it
    /// is the kit the port-fidelity comparison has to use.
    const DEFAULT_HUNT_KIT: &str = "big_game";
    /// A **forage-only** roster entry, so naming it on a hunt query is `kit_wrong_job` rather than
    /// `unknown_kit`. The two failures are different facts and the client renders them differently.
    const FORAGE_ONLY_KIT: &str = "gathering";
    const PARTY: u32 = 4;
    /// A floor mid-range, deliberately **not** one of `RAID_FORECAST_FLOOR_SAMPLES` in the fidelity
    /// test's usage — the point of the query is that it answers the floor it is asked for.
    const A_FLOOR: f32 = 0.30;
    /// **One worker against the one-unit reference ledger**, which is exactly covered — so the
    /// coverage below is a single fully-armed crew and the party it builds is `uniform`, the shape
    /// every fixture in this module assumed before the partly-equipped party landed. The party is
    /// scale-free (its crews carry *shares*), so quoting it at one worker and then projecting a
    /// party of four is the same party.
    const ONE_FULLY_ARMED_WORKER: f32 = 1.0;

    /// The coverage of a party whose gear reaches everybody — see [`ONE_FULLY_ARMED_WORKER`].
    fn fully_armed(
        equipment: &EquipmentConfig,
        kit: &crate::equipment_config::KitChoice,
        wear: &BandEquipment,
    ) -> crate::equipment_config::KitCoverage {
        equipment.coverage(kit, ONE_FULLY_ARMED_WORKER, wear)
    }

    /// Wild Boar's shipped shape, big enough that a party of four lands a real payload.
    fn test_herd() -> Herd {
        Herd::new(
            HERD.to_string(),
            "Wild Boar".to_string(),
            SizeClass::Big,
            vec![UVec2::new(1, 1)],
            900.0,
            1000.0,
            0.0,
            0.4,
            60.0,
        )
    }

    /// A world carrying only what an answer reads: the six config handles and a herd registry.
    ///
    /// Deliberately **not** `build_headless_app` — the answering path touches no system, no
    /// schedule and no map, so a bare `World` is both faster and a statement about what the query
    /// actually depends on. A resource this forgets shows up as a panic naming the resource.
    fn test_world() -> World {
        let mut world = World::new();
        world.insert_resource(EquipmentConfigHandle::default());
        world.insert_resource(CreaturesConfigHandle::default());
        world.insert_resource(CombatConfigHandle::default());
        world.insert_resource(LaborConfigHandle::default());
        world.insert_resource(FaunaConfigHandle::default());
        world.insert_resource(ExpeditionConfigHandle::default());
        world.insert_resource(HerdRegistry {
            herds: vec![test_herd()],
        });
        world
    }

    /// A band of `faction` carrying `band_id`, with a **fresh** (zero-wear) kit ledger — the
    /// condition the retired tables assumed of everyone.
    fn spawn_band(world: &mut World, faction: FactionId, band_id: u64) -> Entity {
        let home = world.spawn_empty().id();
        world
            .spawn((
                BandId(band_id),
                PopulationCohort {
                    home,
                    current_tile: home,
                    size: 30,
                    children: scalar_zero(),
                    working: scalar_from_f32(30.0),
                    elders: scalar_zero(),
                    stores: LocalStore::new(),
                    morale: scalar_one(),
                    last_food_consumption: 0.0,
                    last_morale_delta: scalar_zero(),
                    last_morale_cause: Default::default(),
                    last_morale_contributions: Default::default(),
                    last_fertility_factors: Default::default(),
                    discontent_fraction: scalar_zero(),
                    grievance: scalar_zero(),
                    last_emigrated: 0,
                    last_immigrated: 0,
                    age_turns: 0,
                    generation: 0,
                    faction,
                    knowledge: Vec::new(),
                    migration: None,
                },
                BandEquipment::start_stocked(&EquipmentConfig::builtin()),
            ))
            .id()
    }

    /// A world with one band and one herd — the ordinary case every error test perturbs by exactly
    /// one field.
    fn world_with_band() -> World {
        let mut world = test_world();
        spawn_band(&mut world, FACTION, BAND);
        world
    }

    fn hunt_ask() -> HuntTripForecastQuery {
        HuntTripForecastQuery {
            faction_id: FACTION.0,
            band_id: BAND,
            herd_id: HERD.to_string(),
            kit_id: DEFAULT_HUNT_KIT.to_string(),
            party_workers: PARTY,
            floor: A_FLOOR,
            preset_floors: Vec::new(),
            // No plateau scan unless a test asks for one — it costs a projection per party.
            max_party_workers: 0,
        }
    }

    fn error_token(reply: &QueryReply) -> &str {
        match reply {
            QueryReply::Error(reason) => reason.as_str(),
            other => panic!("expected a QueryError, got {other:?}"),
        }
    }

    // --- the refusals -----------------------------------------------------------------------

    /// One refusal case's edit to an otherwise-valid query. Named rather than written inline because
    /// the boxed closure's type is what a `Vec` of these needs, and spelling it at the binding is
    /// noise around the only thing the table is about — which field is broken, and which token that
    /// must produce.
    type Perturbation = Box<dyn Fn(&mut HuntTripForecastQuery)>;

    /// **Every refusal path, one table.** They are cheap individually and worth having together:
    /// each is a distinct fact the client renders differently, and the failure mode this guards is a
    /// resolution step that answers the *wrong* token — "unknown_herd" for a kit typo sends a player
    /// looking at the map instead of at the picker.
    #[test]
    fn each_refusal_answers_its_own_token() {
        let cases: Vec<(&str, Perturbation)> = vec![
            (
                query_error::UNKNOWN_HERD,
                Box::new(|ask: &mut HuntTripForecastQuery| ask.herd_id = "no_such_herd".into()),
            ),
            (
                query_error::UNKNOWN_BAND,
                Box::new(|ask: &mut HuntTripForecastQuery| ask.band_id = BAND + 1),
            ),
            (
                query_error::UNKNOWN_KIT,
                Box::new(|ask: &mut HuntTripForecastQuery| ask.kit_id = "no_such_kit".into()),
            ),
            (
                query_error::KIT_WRONG_JOB,
                Box::new(|ask: &mut HuntTripForecastQuery| ask.kit_id = FORAGE_ONLY_KIT.into()),
            ),
            (
                query_error::INVALID_FLOOR,
                Box::new(|ask: &mut HuntTripForecastQuery| ask.floor = 1.5),
            ),
            (
                query_error::INVALID_PARTY,
                Box::new(|ask: &mut HuntTripForecastQuery| ask.party_workers = 0),
            ),
        ];

        for (expected, perturb) in cases {
            let mut world = world_with_band();
            let mut ask = hunt_ask();
            perturb(&mut ask);
            let reply = answer_forecast_query(&mut world, &QueryPayload::HuntTripForecast(ask));
            assert_eq!(
                error_token(&reply),
                expected,
                "the perturbation for '{expected}' answered the wrong token"
            );
        }
    }

    /// **A band of ANOTHER faction is `unknown_band`, not someone else's forecast.** The band id is
    /// real and resolves; only the ownership check stands between a player and a rival's readout.
    #[test]
    fn a_band_of_another_faction_does_not_resolve() {
        let mut world = test_world();
        spawn_band(&mut world, FactionId(FACTION.0 + 1), BAND);
        let reply = answer_forecast_query(&mut world, &QueryPayload::HuntTripForecast(hunt_ask()));
        assert_eq!(error_token(&reply), query_error::UNKNOWN_BAND);
    }

    /// **A bad PRESET floor refuses the whole reply**, rather than answering a short `per_preset`
    /// list whose positions no longer line up with what was asked. The rows are correlated by
    /// position, so a partial answer is a silently mislabelled one.
    #[test]
    fn one_bad_preset_floor_refuses_the_whole_query() {
        let mut world = world_with_band();
        let mut ask = hunt_ask();
        ask.preset_floors = vec![0.0, 1.5, 0.5];
        let reply = answer_forecast_query(&mut world, &QueryPayload::HuntTripForecast(ask));
        assert_eq!(error_token(&reply), query_error::INVALID_FLOOR);
    }

    /// The denial verb resolves through the *same* seam, so it refuses on the same terms. Worth one
    /// case rather than six: what is being checked is that it shares the resolution, not that the
    /// tokens exist twice.
    #[test]
    fn the_denial_verb_refuses_an_unknown_kit_too() {
        let mut world = world_with_band();
        let reply = answer_forecast_query(
            &mut world,
            &QueryPayload::DenialRaidForecast(DenialRaidForecastQuery {
                faction_id: FACTION.0,
                band_id: BAND,
                herd_id: HERD.to_string(),
                kit_id: "no_such_kit".to_string(),
                party_workers: PARTY,
                max_party_workers: PARTY,
            }),
        );
        assert_eq!(error_token(&reply), query_error::UNKNOWN_KIT);
    }

    // --- the answers ------------------------------------------------------------------------

    /// **A query echoes what it was asked**, on the composed row and on every preset row, in order.
    /// That echo is the client's assertion that the answer it is rendering is the answer to its own
    /// question — the thing a sampled table could never offer, because it always answered the
    /// nearest rung instead.
    #[test]
    fn every_row_echoes_the_floor_and_party_it_was_asked_for() {
        let mut world = world_with_band();
        let mut ask = hunt_ask();
        let presets = vec![0.0, 0.25, 0.5];
        ask.preset_floors = presets.clone();

        let QueryReply::HuntTripForecast(answer) =
            answer_forecast_query(&mut world, &QueryPayload::HuntTripForecast(ask))
        else {
            panic!("a well-formed hunt query is answered");
        };
        assert_eq!(answer.at_composed.floor, A_FLOOR);
        assert_eq!(answer.at_composed.party_workers, PARTY);
        assert_eq!(answer.per_preset.len(), presets.len());
        for (row, floor) in answer.per_preset.iter().zip(presets) {
            assert_eq!(row.floor, floor);
            assert_eq!(row.party_workers, PARTY);
        }
    }

    /// **The denial answer carries the party it was asked for and the party the sheet should open
    /// on**, and those are two different numbers with two different meanings.
    #[test]
    fn a_denial_answer_carries_both_the_asked_party_and_the_seeded_one() {
        let mut world = world_with_band();
        let QueryReply::DenialRaidForecast(answer) = answer_forecast_query(
            &mut world,
            &QueryPayload::DenialRaidForecast(DenialRaidForecastQuery {
                faction_id: FACTION.0,
                band_id: BAND,
                herd_id: HERD.to_string(),
                kit_id: DEFAULT_HUNT_KIT.to_string(),
                party_workers: PARTY,
                max_party_workers: PARTY,
            }),
        ) else {
            panic!("a well-formed denial query is answered");
        };
        assert_eq!(answer.at_composed.party_workers, PARTY);
        assert!(
            !answer.at_composed.outcome.is_empty(),
            "every projection names its outcome — a blank verdict is the one thing the sheet \
             cannot render"
        );
    }

    // --- the row mapping --------------------------------------------------------------------

    /// **The wire row is `hunt_trip_forecast`'s answer, transcribed — never a second computation of
    /// it.**
    ///
    /// This is what the port-fidelity test became. While the estimate tables still existed it
    /// compared the query's rows against `hunt_trip_estimate_entries` cell for cell, which proved the
    /// port had not approximated anything. The table is gone, so comparing against it would mean
    /// keeping a fossil implementation alive purely to be compared against — the exact second copy
    /// of the model this arc exists to remove.
    ///
    /// What is durable, and what this keeps, is the half that can still break: the **mapping**.
    /// `hunt_trip_row` transcribes a `HuntTripForecast` into a wire row, and every field is a place
    /// a rename or a reorder could silently swap two numbers of the same type — `delivered_food` for
    /// `wasted_food`, `delivers_food` for `delivers_trade`. Those are all `f32` and `bool`; nothing
    /// but an assertion catches a transposition.
    ///
    /// It also pins the two sentinels, which are the only values the row does not carry verbatim:
    /// `turns_to_fill` collapses `None` to [`NEVER_FILLED`], and `bound` is the enum's own key.
    #[test]
    fn the_wire_row_transcribes_the_forecast_field_for_field() {
        let world = world_with_band();
        let herd = test_herd();
        let equipment = world.resource::<EquipmentConfigHandle>().get();
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let expedition = world.resource::<ExpeditionConfigHandle>().get();
        let kit = equipment
            .resolve_kit_for_job(Some(DEFAULT_HUNT_KIT), KitJob::Hunt)
            .expect("the shipped default hunt kit resolves");
        let fresh = BandEquipment::start_stocked(&EquipmentConfig::builtin());
        let party = query_hunting_party(
            &world,
            &equipment,
            &fully_armed(&equipment, &kit, &fresh),
            &fresh,
            herd.body_mass,
        );
        let per_worker_haul = query_per_worker_haul(
            &world,
            &equipment,
            &fully_armed(&equipment, &kit, &fresh),
            &fresh,
        );
        let resolved = ResolvedAsk {
            herd: herd.clone(),
            party,
            per_worker_haul,
        };

        // Several floors and several party sizes, because a transposition can hide behind a row
        // where the two swapped fields happen to be equal (a raid that wastes nothing, say).
        let mut saw_a_payload = false;
        for floor in [0.0_f32, 0.3, 0.5, 0.8] {
            for party_workers in [1_u32, 4, 12] {
                let row = hunt_trip_row(floor, party_workers, &resolved, &fauna, &expedition);
                let direct = crate::systems::hunt_trip_forecast(
                    party_workers,
                    &herd,
                    floor,
                    &fauna,
                    per_worker_haul,
                    &expedition,
                    &resolved.party,
                );

                assert_eq!(
                    row.floor, floor,
                    "the row echoes the floor it was asked for"
                );
                assert_eq!(row.party_workers, party_workers);
                assert_eq!(
                    row.turns_to_fill,
                    direct.turns_to_fill.unwrap_or(NEVER_FILLED),
                    "a raid that never completes reports the NEVER_FILLED sentinel, not a blank"
                );
                assert_eq!(row.bound, direct.bound.as_str());
                assert_eq!(row.delivers_food, direct.delivers_food);
                assert_eq!(row.delivers_trade, direct.delivers_trade);
                assert_eq!(row.animals_taken, direct.animals_taken);
                assert_eq!(row.delivered_food, direct.delivered_food);
                assert_eq!(row.delivered_trade, direct.delivered_trade);
                assert_eq!(row.wasted_food, direct.wasted_food);

                saw_a_payload |= row.delivered_food > 0.0 || row.delivered_trade > 0.0;
            }
        }
        assert!(
            saw_a_payload,
            "the fixture must land a real payload somewhere, or every field compared was zero and \
             a transposition would pass"
        );
    }

    // --- the lethality contract -------------------------------------------------------------

    /// **A quoted raid fights at the EXPEDITION's lethality**, which is the defect the estimate
    /// tables carried: they priced a detached party at resident-hunt severity, under-stating its
    /// casualties and so over-stating its take.
    ///
    /// Asserted **two ways on purpose**. Against `expedition_tuning()`, which is the seam
    /// `advance_expeditions` resolves the live raid through — so forecast and turn cannot diverge.
    /// And against a locally recomputed `tuning().lethality × expedition_danger_multiplier`, so the
    /// constructor cannot quietly stop meaning that: with one shared function the two call sites can
    /// no longer drift apart, and what is left to protect is that the multiply still happens exactly
    /// once and still says what we think it says.
    #[test]
    fn a_quoted_raid_fights_at_expedition_lethality() {
        let world = world_with_band();
        let equipment = world.resource::<EquipmentConfigHandle>().get();
        let combat = world.resource::<CombatConfigHandle>().get();
        let kit = equipment
            .resolve_kit_for_job(Some(DEFAULT_HUNT_KIT), KitJob::Hunt)
            .expect("the shipped default hunt kit resolves");

        let fresh = BandEquipment::start_stocked(&EquipmentConfig::builtin());
        let party = query_hunting_party(
            &world,
            &equipment,
            &fully_armed(&equipment, &kit, &fresh),
            &fresh,
            test_herd().body_mass,
        );

        assert_eq!(
            party.tuning,
            combat.expedition_tuning(),
            "the quoted party must fight at the same tuning `advance_expeditions` resolves the \
             live raid at"
        );
        let mut recomputed = combat.tuning();
        recomputed.lethality *= combat.expedition_danger_multiplier;
        assert_eq!(
            party.tuning, recomputed,
            "`expedition_tuning` no longer means `tuning` with lethality scaled by the danger \
             multiplier"
        );
        assert!(
            party.tuning.lethality > combat.tuning().lethality,
            "the shipped `expedition_danger_multiplier` is > 1, so a detached raid must be quoted \
             bloodier than a resident hunt — if this fails the test fixture has stopped \
             distinguishing the two cases at all"
        );
    }

    // --- the denial seed ---------------------------------------------------------------------

    /// **The seed's test is SUCCESS, not "not repelled"** — the reported Wild Aurochs defect
    /// (`docs/plan_denial_raid.md` §3.1), asserted on the predicate the search actually calls.
    ///
    /// The two readings agree on three of the four verdicts and differ on the one that matters. A
    /// [`crate::DenialOutcome::Horizon`] result is a raid the projection ran to its whole length
    /// with the herd still standing, so it demonstrates nothing the sim will vouch for; seeding
    /// there quoted a party of 5 under its own verdict line *"still standing when the forecast runs
    /// out"* — a number offered as the answer while being one short of one.
    ///
    /// This used to be asserted by handing `seeded_denial_party` an array of rows with a `horizon`
    /// row sitting below a success row. There is no array now — the search walks parties and stops
    /// at the first success — so the claim is made where it lives: on the verdict itself.
    #[test]
    fn only_a_raid_that_finished_the_herd_counts_as_a_success() {
        use crate::DenialOutcome;

        assert!(DenialOutcome::PastRecovery.succeeded());
        assert!(DenialOutcome::HerdLost.succeeded());
        assert!(
            !DenialOutcome::Horizon.succeeded(),
            "a raid still running when the projection ended proves nothing — seeding there is the \
             defect this predicate exists to prevent"
        );
        assert!(!DenialOutcome::Repelled.succeeded());
    }

    /// **The seed is the FIRST party that works, and every party below it does not** — the whole
    /// contract of a contiguous upward search, asserted without pinning a number a retune would
    /// have to chase.
    ///
    /// It is derived rather than hardcoded on purpose: the test computes the seed, then re-runs the
    /// projection at every party below it and at the seed itself. That is a statement about the
    /// **search**, so it holds whatever the fixture herd's requirement happens to be — and it is
    /// exactly what a sampled axis could not promise, because the smallest *sampled* success is not
    /// the smallest success.
    #[test]
    fn the_seed_is_the_smallest_party_that_actually_drives_the_herd_down() {
        /// Well past the fixture boar's requirement, so the search has room to find one.
        const BAND_CAN_FIELD: u32 = 40;

        let world = world_with_band();
        let herd = test_herd();
        let equipment = world.resource::<EquipmentConfigHandle>().get();
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let expedition = world.resource::<ExpeditionConfigHandle>().get();
        let combat = world.resource::<CombatConfigHandle>().get();
        let kit = equipment
            .resolve_kit_for_job(Some(DEFAULT_HUNT_KIT), KitJob::Hunt)
            .expect("the shipped default hunt kit resolves");
        let fresh = BandEquipment::start_stocked(&EquipmentConfig::builtin());
        let party = query_hunting_party(
            &world,
            &equipment,
            &fully_armed(&equipment, &kit, &fresh),
            &fresh,
            herd.body_mass,
        );
        let per_worker_haul = query_per_worker_haul(
            &world,
            &equipment,
            &fully_armed(&equipment, &kit, &fresh),
            &fresh,
        );
        let range_sigmas = combat.forecast_range_sigmas;

        let succeeds = |party_workers: u32| {
            crate::systems::denial_forecast(
                party_workers,
                &herd,
                &fauna,
                per_worker_haul,
                &expedition,
                &party,
                range_sigmas,
            )
            .outcome
            .succeeded()
        };

        let seed = seeded_denial_party_for(
            &herd,
            &fauna,
            &expedition,
            &party,
            per_worker_haul,
            range_sigmas,
            BAND_CAN_FIELD,
        );

        assert_ne!(
            seed, NO_VIABLE_DENIAL_PARTY,
            "the fixture must be deniable by a party of {BAND_CAN_FIELD} or this test asserts \
             nothing about the search"
        );
        assert!(
            succeeds(seed),
            "the seeded party {seed} must itself drive the herd past recovery"
        );
        for smaller in 1..seed {
            assert!(
                !succeeds(smaller),
                "a party of {smaller} is below the seed {seed} and must NOT succeed — the seed is \
                 the smallest party that works, not merely one that does"
            );
        }
    }

    /// **The sentinel is reached by searching and finding nothing, not by refusing to search.**
    ///
    /// A band that can field one hunter against a herd nothing that small can dent must read
    /// [`NO_VIABLE_DENIAL_PARTY`], and so must a band that can field nobody at all. Paired with the
    /// test above, which is what stops "answer 0 always" from passing.
    #[test]
    fn a_party_the_band_cannot_raise_seeds_the_sentinel() {
        let world = world_with_band();
        let herd = test_herd();
        let equipment = world.resource::<EquipmentConfigHandle>().get();
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let expedition = world.resource::<ExpeditionConfigHandle>().get();
        let combat = world.resource::<CombatConfigHandle>().get();
        let kit = equipment
            .resolve_kit_for_job(Some(DEFAULT_HUNT_KIT), KitJob::Hunt)
            .expect("the shipped default hunt kit resolves");
        let fresh = BandEquipment::start_stocked(&EquipmentConfig::builtin());
        let party = query_hunting_party(
            &world,
            &equipment,
            &fully_armed(&equipment, &kit, &fresh),
            &fresh,
            herd.body_mass,
        );
        let per_worker_haul = query_per_worker_haul(
            &world,
            &equipment,
            &fully_armed(&equipment, &kit, &fresh),
            &fresh,
        );

        let seed_at = |max_party_workers: u32| {
            seeded_denial_party_for(
                &herd,
                &fauna,
                &expedition,
                &party,
                per_worker_haul,
                combat.forecast_range_sigmas,
                max_party_workers,
            )
        };

        assert_eq!(
            seed_at(0),
            NO_VIABLE_DENIAL_PARTY,
            "a band that can field nobody has no party to open on"
        );
        assert_eq!(
            seed_at(1),
            NO_VIABLE_DENIAL_PARTY,
            "…and one hunter cannot deny a full boar range, so the sentinel is the honest answer \
             rather than a party the player could send and watch fail"
        );
    }

    // --- the contiguous useful cap -------------------------------------------------------------

    /// **`useful_cap` walks every party, so the plateau is the real one.**
    ///
    /// The scan used to walk `expedition_config.estimate_party_sizes` — `1, 2, 3, 4, 8, 16, 32, 64`
    /// as shipped — so it could only ever report a *rung*. This asserts the property that buys:
    /// the answer is allowed to be a party the retired ladder did not carry.
    ///
    /// Asserted as an invariant rather than a pinned number: the cap must be a party at which the
    /// payload is still rising, and the party **above** it must not raise the payload further. That
    /// is what "plateau" means, and it holds for any herd and any kit.
    #[test]
    fn the_useful_cap_is_the_real_plateau_not_a_sampled_rung() {
        /// Wide enough to run past the fixture boar's plateau.
        const BAND_CAN_FIELD: u32 = 30;

        let world = world_with_band();
        let herd = test_herd();
        let equipment = world.resource::<EquipmentConfigHandle>().get();
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let expedition = world.resource::<ExpeditionConfigHandle>().get();
        let kit = equipment
            .resolve_kit_for_job(Some(DEFAULT_HUNT_KIT), KitJob::Hunt)
            .expect("the shipped default hunt kit resolves");
        let fresh = BandEquipment::start_stocked(&EquipmentConfig::builtin());
        let resolved = ResolvedAsk {
            herd: herd.clone(),
            party: query_hunting_party(
                &world,
                &equipment,
                &fully_armed(&equipment, &kit, &fresh),
                &fresh,
                herd.body_mass,
            ),
            per_worker_haul: query_per_worker_haul(
                &world,
                &equipment,
                &fully_armed(&equipment, &kit, &fresh),
                &fresh,
            ),
        };

        let delivered = |party_workers: u32| {
            let row = hunt_trip_row(A_FLOOR, party_workers, &resolved, &fauna, &expedition);
            if row.delivers_food {
                row.delivered_food
            } else {
                row.delivered_trade
            }
        };

        let cap = useful_party_cap(A_FLOOR, BAND_CAN_FIELD, &resolved, &fauna, &expedition);
        assert_ne!(
            cap, NO_USEFUL_CAP,
            "the fixture must actually plateau inside {BAND_CAN_FIELD} workers, or this test \
             asserts nothing"
        );
        assert!(
            delivered(cap) > 0.0,
            "a cap must land a payload — a raid that comes home empty at every size is flat at \
             zero, which is not a plateau"
        );
        if cap > 1 {
            assert!(
                delivered(cap) > delivered(cap - 1),
                "the payload must still be RISING at the cap ({} vs {} one worker below)",
                delivered(cap),
                delivered(cap - 1)
            );
        }
        assert!(
            delivered(cap + 1) <= delivered(cap),
            "…and the party above the cap must add nothing ({} at {} vs {} at {})",
            delivered(cap + 1),
            cap + 1,
            delivered(cap),
            cap
        );
    }

    /// **A band that can field nobody gets no cap**, rather than a cap of one it cannot staff.
    #[test]
    fn a_band_that_can_field_nobody_gets_no_useful_cap() {
        let world = world_with_band();
        let herd = test_herd();
        let equipment = world.resource::<EquipmentConfigHandle>().get();
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let expedition = world.resource::<ExpeditionConfigHandle>().get();
        let kit = equipment
            .resolve_kit_for_job(Some(DEFAULT_HUNT_KIT), KitJob::Hunt)
            .expect("the shipped default hunt kit resolves");
        let fresh = BandEquipment::start_stocked(&EquipmentConfig::builtin());
        let resolved = ResolvedAsk {
            herd: herd.clone(),
            party: query_hunting_party(
                &world,
                &equipment,
                &fully_armed(&equipment, &kit, &fresh),
                &fresh,
                herd.body_mass,
            ),
            per_worker_haul: query_per_worker_haul(
                &world,
                &equipment,
                &fully_armed(&equipment, &kit, &fresh),
                &fresh,
            ),
        };

        assert_eq!(
            useful_party_cap(A_FLOOR, 0, &resolved, &fauna, &expedition),
            NO_USEFUL_CAP
        );
    }
}
