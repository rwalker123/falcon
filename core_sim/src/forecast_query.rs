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
    query_error, DenialRaidForecastQuery, DenialRaidForecastReply, DenialRow, HuntCrewTakeQuery,
    HuntCrewTakeReply, HuntCrewTakeRow, HuntTripForecastQuery, HuntTripForecastReply, HuntTripRow,
    QueryPayload, QueryReply,
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
        QueryPayload::HuntCrewTake(ask) => answer_hunt_crew_take(world, ask),
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
    let (herd, wear, kit) = resolve_quarry_and_kit(world, faction_id, band_id, herd_id, kit_id)?;
    let equipment = world.resource::<EquipmentConfigHandle>().get();

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

/// **The three things every query names before a party can be built** — the quarry, the asking
/// band's live wear ledger, and the kit it is carrying — or the token that says which one failed.
///
/// Shared by the two party-priced verbs and by the crew-take curve, so *"which band is asking"* and
/// *"is that a hunt kit"* cannot come to be answered two ways. It deliberately stops short of
/// building the party: the curve resolves one **per crew size** (coverage depends on how many people
/// the kit has to stretch over) and at the **base** tuning rather than the expedition's.
fn resolve_quarry_and_kit(
    world: &mut World,
    faction_id: u32,
    band_id: u64,
    herd_id: &str,
    kit_id: &str,
) -> Result<(Herd, BandEquipment, crate::equipment_config::KitChoice), QueryReply> {
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
    Ok((herd, wear, kit))
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
        animals_taken: forecast.animals_taken,
        delivered_food: forecast.delivered_food,
        wasted_food: forecast.wasted_food,
        // **What the trip lands, per material** — the whole payload on an inedible quarry, whose
        // `delivered_food` is honestly `0`. Transcribed, never re-projected.
        delivered_material: forecast
            .delivered_material
            .iter()
            .map(|payoff| sim_runtime::commands::MaterialPayoff {
                material_id: payoff.material.clone(),
                amount: payoff.amount,
            })
            .collect(),
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
/// - **It scans the measure this QUARRY pays in.** An inedible species delivers `0` food at every
///   size, so a food-only scan finds no plateau at all on exactly the quarry whose whole payload is
///   hides — which this reply cannot carry, so the scan counts its ANIMALS instead.
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
        // **An INEDIBLE quarry's payload is counted in ANIMALS**, not in food it does not pay.
        // It used to be counted in the retired trade scalar; what such a raid really brings home is
        // material batches, which this reply does not carry — but the *plateau* is a fact about the
        // herd's surplus rather than about a currency, and the kill count reaches it at exactly the
        // party size any payload measure would. Without this arm a wolf raid's `useful_cap` would be
        // `0` (*"no party is worth sending"*) for a raid the sim will happily pay in pelts.
        let delivered = if row.delivers_food {
            row.delivered_food
        } else {
            row.animals_taken as f32
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
            // What the raid lands, per material — the whole payload on an inedible quarry.
            delivered_material: forecast
                .delivered_material
                .iter()
                .map(|payoff| sim_runtime::commands::MaterialPayoff {
                    material_id: payoff.material.clone(),
                    amount: payoff.amount,
                })
                .collect(),
        },
        party_needed,
    })
}

/// **THE HUNT TAKE CURVE** — one row per crew size, answering *"if I put N herders on this herd,
/// how many animals a turn do they bring down?"* for every N the panel's stepper can reach.
///
/// # The rows are a RATE — animals per turn, not bodies next turn
///
/// [`crate::fauna::HuntFight::expected_brought_down`], never `brought_down`. The distinction is the
/// whole-animal quantiser, and publishing the quantised side of it is a defect rather than a
/// rounding preference: a Wild Aurochs (`defense 6`, `durability 150`, `engage_rate 0.17`) is
/// engaged one animal at a time by every crew from 1 to 11, so `stayed` is `0.8` and the blow is
/// capped at `120` damage against a `150`-durability body — `floor(damage / durability)` is **`0`
/// for every one of them**, and the panel printed *"≈0 animals/turn"* for a crew genuinely taking
/// `0.75`. It is a plateau, not a near-miss: no equipment level moves it, because the cap is the
/// body in front of the party rather than the party's damage.
///
/// **The sim already knew.** [`crate::fauna::project_realized_hunt`] resolves the fight *inside* its
/// forward loop and carries the wound ledger between turns for exactly this reason — *"a
/// sub-threshold party brings down nothing for several turns and then a whole animal, and a
/// projection that froze the first turn's answer would quote zero forever"*. A curve is one frozen
/// turn by construction, so it must publish the rate the ledger integrates rather than the turn's
/// floored count.
///
/// # How it relates to `SourceYield::realized`, and why they are not the same number
///
/// Both are per-turn expectations of the same take and they agree closely, but they are **not**
/// interchangeable and neither is derivable from the other:
///
/// - **This curve** is the *instantaneous* rate at the herd's **current** stock —
///   `min(w × (attack − defense) × lethality / durability, stayed)`, where `stayed` already carries
///   the retreat and the escapement room. One turn, evaluated at three quantiles, for every crew.
/// - **[`crate::fauna::project_realized_hunt`]** is a *forward average over a changing stock*: it
///   walks `hunt.forecast_horizon_turns` turns of regrow → take, so the herd it is quoting is not
///   the herd it started from, and it additionally caps each turn by the crew's carry throughput.
///   It also sums the **quantised** kills, so up to one unfinished body sits on the ledger when the
///   horizon ends and is never counted.
///
/// So `realized` runs **at or below** the curve on a stock the take is drawing down, and trails it
/// by up to `1 / horizon` animals a turn from the unfinished body alone. `the_curve_and_realized_
/// agree_on_a_stable_stock` pins that relationship on a fixture where the drawdown is negligible;
/// it is a documented, tested gap rather than a coincidence, and the two must never be published as
/// one figure.
///
/// # Why a curve and not a per-hunter rate
///
/// Because the take is **not linear in crew size**, and measuring the shipped roster is what
/// established that rather than an argument about it. The take is
/// `min(w × fight_rate, max(floor(w × engage_rate), 1) × stay_fraction)`: the fight half is exactly
/// linear in the crew, the engagement half is a **staircase** that is flat across whole runs of crew
/// sizes and steps at integer boundaries, and which of the two binds changes *within* the stepper's
/// own range. On the shipped Wild Boar (`engage_rate 0.33`) crews of 1 through 6 all bring down
/// `0.75` animals/turn — a per-hunter reading spanning 6× across six adjacent stepper positions. On
/// Wild Aurochs the binding term flips from the fight to the engagement between crews 8 and 11 and
/// back at 12, so even sampling the endpoints would miss a 28% error sitting between them.
///
/// A scalar cannot carry that, and the **band** cannot be carried by one either: both stochastic
/// stages are binomials, so the spread is `O(√w)` and *shrinks* per hunter as the crew grows. At the
/// shipped `hit_chance = 1.0` it collapses to a point and no test would notice the difference — which
/// is exactly why it must not be published in a shape that is only correct at today's tuning.
///
/// # It is the RESIDENT band's answer, at the base tuning
///
/// [`answer_hunt_trip_forecast`] prices a **detached** party at
/// [`crate::combat_config::CombatConfig::expedition_tuning`] (1.5× lethality as shipped), because a
/// raid far from home is bloodier. A band hunting its own range is not on a raid, so the party here
/// resolves at the base tuning — the same one `advance_labor_allocation`'s Hunt arm fights at. The
/// two differ by half again in the fight term; borrowing the trip sheet's rows for this panel would
/// have been wrong by that much.
///
/// # The party is re-resolved per crew size, and that is a term rather than an accident
///
/// [`crate::equipment_config::EquipmentConfig::coverage`] divides the kit the band actually holds
/// across the people sent, so a band with five spears fields a different *mix* at four hunters than
/// at twelve — a third source of curvature, and it lands in the curve for free because each row
/// builds its own party.
fn answer_hunt_crew_take(world: &mut World, ask: &HuntCrewTakeQuery) -> QueryReply {
    if !floor_is_valid(ask.floor) {
        return query_failure(query_error::INVALID_FLOOR);
    }
    // **AND THE LOOP BOUND IS VALIDATED LIKE THE FLOOR IS** — `max_workers` is fed straight into
    // `hunt_crew_take_curve`'s `1..=max_workers`, whose body resolves a kit coverage and three
    // engagements per crew and materialises one reply row each. Every other field on this ask is
    // checked (`floor_is_valid`, `resolve_quarry_and_kit`); this one was not, so a client bug
    // sending `u32::MAX` wedged the command thread and allocated tens of gigabytes answering a
    // question about a band that cannot exist.
    if ask.max_workers > MAX_CREW_TAKE_WORKERS {
        return query_failure(query_error::INVALID_CREW);
    }
    let (herd, wear, kit) = match resolve_quarry_and_kit(
        world,
        ask.faction_id,
        ask.band_id,
        &ask.herd_id,
        &ask.kit_id,
    ) {
        Ok(resolved) => resolved,
        Err(failure) => return failure,
    };
    let equipment = world.resource::<EquipmentConfigHandle>().get();
    let fauna = world.resource::<FaunaConfigHandle>().get();
    let combat = world.resource::<CombatConfigHandle>().get();
    let intrinsic = world.resource::<CreaturesConfigHandle>().get().person();
    // **THE ONE PRODUCER** ([`crate::fauna::hunt_crew_take_curve`]) — the same call the capture makes
    // to publish an assigned row's `hunt_useful_workers`, so the rows this reply ships and the cap
    // the Work board reads cannot be two different arithmetics. This half is only the transport.
    let curve = crate::fauna::hunt_crew_take_curve(&crate::fauna::HuntCrewCurveInputs {
        herd: &herd,
        fauna: &fauna,
        equipment: &equipment,
        kit: &kit,
        wear: &wear,
        intrinsic,
        // **BASE, not `expedition_tuning`** — see this function's doc.
        tuning: combat.tuning(),
        hunt_injury_damage_per_animal: combat.hunt_injury_damage_per_animal,
        // The reported band's width — the same readout lever every other quantile pair on this
        // channel is drawn at (`combat_config.forecast_range_sigmas`).
        range_sigmas: combat.forecast_range_sigmas,
        floor: ask.floor,
        // A **corralled** quarry is collected rather than stalked, and its curve's crew term is the
        // keepers' own throughput off this baseline. A stalked one never reads it.
        baseline_haul_rate: world
            .resource::<crate::labor_config::LaborConfigHandle>()
            .get()
            .hunt
            .per_worker_biomass_capacity,
        max_workers: ask.max_workers,
    });
    let per_crew = curve
        .into_iter()
        .map(|row| HuntCrewTakeRow {
            workers: row.workers,
            animals_low: row.low,
            animals_likely: row.likely,
            animals_high: row.high,
        })
        .collect();
    QueryReply::HuntCrewTake(HuntCrewTakeReply { per_crew })
}

/// **THE LARGEST CREW A TAKE CURVE MAY BE ASKED ABOUT.** The reply is one row per crew over
/// `1..=max_workers`, so both the work and the allocation are linear in the ask — and the domain the
/// number *means* is a band's own crew pool, which is its working population. A thousand hunters on
/// one herd is already an order of magnitude past anything the demographics produce, so the bound
/// refuses nonsense without ever refusing play.
///
/// **A refusal, not a clamp** — the rule `floor_is_valid` follows at the same boundary: silently
/// answering a smaller crew than was asked about would hand a client a curve whose last row is not
/// the plateau it thinks it is.
const MAX_CREW_TAKE_WORKERS: u32 = 1_000;

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
    use crate::fauna::HuntDraw;
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
                    last_turn_transfer_received: 0.0,
                    last_turn_transfer_sent: 0.0,
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
    /// a rename or a reorder could silently swap two numbers of the same type — `delivered_food`
    /// for `wasted_food`. Those are all `f32` and `bool`; nothing but an assertion catches a
    /// transposition.
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
                assert_eq!(row.animals_taken, direct.animals_taken);
                assert_eq!(row.delivered_food, direct.delivered_food);
                assert_eq!(row.wasted_food, direct.wasted_food);

                saw_a_payload |= row.delivered_food > 0.0 || row.animals_taken > 0;
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

    // --- THE HUNT TAKE CURVE ------------------------------------------------------------------

    /// The fight-bound quarry: `defense 6`, `durability 150`, `engage_rate 0.17`, `body 120`. A
    /// speared party's `w x (20 - 6) / 150` sits below its reach over most of the stepper's range,
    /// so this is where the FIGHT decides the take — and where the client's fight-blind arithmetic
    /// over-quoted by 2.3x.
    const AUROCHS: &str = "Wild Aurochs";
    /// The engagement-bound quarry, and the module's default fixture: `engage_rate 0.33` floors to
    /// **one** animal for every crew from 1 to 6, so six adjacent stepper positions all take the
    /// same 0.75 animals a turn. That flat run is `animals_engaged`'s `max(.., 1)`.
    const BOAR: &str = "Wild Boar";
    /// Far past every flat run and every binding flip on both fixtures, so a sweep sees the shape
    /// rather than one arm of it.
    const SWEEP_CREW: u32 = 24;

    /// A herd of `species` fat enough that the escapement room never binds at the swept floors —
    /// the point of these fixtures is the engagement and the fight, and a starved herd would hide
    /// both behind the room.
    /// Standing stock far above anything a party can take, so the escapement room never binds and
    /// the take is decided by the engagement and the fight — which is what most of these fixtures
    /// are about.
    const FAT_HERD: f32 = 90_000.0;

    /// A herd of `species` at a stated standing stock — [`FAT_HERD`] for the fixtures that want the
    /// engagement and the fight to decide the take, and a thin one for the fixture that wants the
    /// escapement room to.
    fn herd_of_biomass(species: &str, body_mass: f32, biomass: f32) -> Herd {
        Herd::new(
            HERD.to_string(),
            species.to_string(),
            SizeClass::Big,
            vec![UVec2::new(1, 1)],
            biomass,
            100_000.0,
            0.0,
            0.4,
            body_mass,
        )
    }

    /// **The largest crew any fixture here fields**, and the size the band is stocked for.
    ///
    /// [`BandEquipment::start_stocked`] is *one unit of everything*, and
    /// [`EquipmentConfig::coverage`] spreads a kit over the party — so at a crew of six it arms one
    /// hunter and sends five out bare-handed, and bare hands (`attack 1`) cannot clear a Wild Boar's
    /// `defense 2`. A curve read off that band is all zeroes at every crew, and every assertion in
    /// this section would pass on them. Stocking for the whole sweep is what makes the fixtures say
    /// something.
    const FULLY_ARMED_CREW: u32 = 200;

    /// The fixture band's ledger — stocked for [`FULLY_ARMED_CREW`], so the kit reaches everybody at
    /// every crew size swept. Read by the world builder *and* by the two direct-call helpers, so the
    /// curve and the take it is compared against are quoted for the same gear.
    fn fixture_wear(equipment: &EquipmentConfig) -> BandEquipment {
        BandEquipment::start_stocked_for(equipment, FULLY_ARMED_CREW as f32)
    }

    /// A world with the fixture band (armed for the whole sweep) and one FAT herd of `species`.
    fn world_hunting(species: &str, body_mass: f32) -> World {
        world_hunting_biomass(species, body_mass, FAT_HERD)
    }

    /// [`world_hunting`] at a stated standing stock.
    fn world_hunting_biomass(species: &str, body_mass: f32, biomass: f32) -> World {
        let mut world = test_world();
        let band = spawn_band(&mut world, FACTION, BAND);
        let equipment = world.resource::<EquipmentConfigHandle>().get();
        world.entity_mut(band).insert(fixture_wear(&equipment));
        world.insert_resource(HerdRegistry {
            herds: vec![herd_of_biomass(species, body_mass, biomass)],
        });
        world
    }

    /// [`world_hunting`] with an **UNSTOCKED** band — an empty [`BandEquipment`] ledger, so
    /// [`EquipmentConfig::coverage`] arms nobody and the whole party fights at the intrinsic
    /// `attack 1`. The fixture for a crew that genuinely cannot hurt its quarry.
    fn world_hunting_bare_handed(species: &str, body_mass: f32) -> World {
        let mut world = test_world();
        let band = spawn_band(&mut world, FACTION, BAND);
        world.entity_mut(band).insert(BandEquipment::default());
        world.insert_resource(HerdRegistry {
            herds: vec![herd_of_biomass(species, body_mass, FAT_HERD)],
        });
        world
    }

    fn crew_ask(max_workers: u32, floor: f32) -> HuntCrewTakeQuery {
        HuntCrewTakeQuery {
            faction_id: FACTION.0,
            band_id: BAND,
            herd_id: HERD.to_string(),
            kit_id: DEFAULT_HUNT_KIT.to_string(),
            floor,
            max_workers,
        }
    }

    fn crew_curve(world: &mut World, ask: &HuntCrewTakeQuery) -> Vec<HuntCrewTakeRow> {
        match answer_hunt_crew_take(world, ask) {
            QueryReply::HuntCrewTake(reply) => reply.per_crew,
            other => panic!("expected a crew-take curve, got {other:?}"),
        }
    }

    /// **The fixture band's party at a stated crew size**, resolved exactly as
    /// [`answer_hunt_crew_take`] resolves each row's: the shipped default hunt kit, this module's
    /// [`fixture_wear`] ledger, and the **resident** tuning rather than `expedition_tuning`.
    ///
    /// One helper, because every direct-call comparison below has to be quoted for the same gear the
    /// curve is or it compares two different parties and calls the difference a defect.
    ///
    /// `wear` is taken by the caller when it wants a bare-handed band ([`fixture_party_with_wear`]);
    /// this arm is the armed one every fixture but that test uses.
    fn fixture_party(world: &World, body_mass: f32, workers: u32) -> HuntingParty {
        let equipment = world.resource::<EquipmentConfigHandle>().get();
        fixture_party_with_wear(world, &fixture_wear(&equipment), body_mass, workers)
    }

    /// [`fixture_party`] over a **stated** wear ledger — the seam the bare-handed fixture needs, and
    /// the reason the two are split.
    fn fixture_party_with_wear(
        world: &World,
        wear: &BandEquipment,
        body_mass: f32,
        workers: u32,
    ) -> HuntingParty {
        let equipment = world.resource::<EquipmentConfigHandle>().get();
        let combat = world.resource::<CombatConfigHandle>().get();
        let intrinsic = world.resource::<CreaturesConfigHandle>().get().person();
        let kit = equipment
            .resolve_kit_for_job(Some(DEFAULT_HUNT_KIT), KitJob::Hunt)
            .expect("the shipped default hunt kit resolves");
        let coverage = equipment.coverage(&kit, workers as f32, wear);
        crate::fauna::PartyResolution {
            equipment: &equipment,
            coverage: &coverage,
            wear,
            intrinsic,
            // The RESIDENT band's tuning — the same one the curve is answered at, and deliberately
            // not `expedition_tuning`.
            tuning: combat.tuning(),
            hunt_injury_damage_per_animal: combat.hunt_injury_damage_per_animal,
        }
        .party_against(crate::equipment_config::Quarry::Mass(body_mass))
    }

    /// **What the sim itself pays this crew, PER TURN OVER A RUN** — `systems::hunt_take` on a
    /// private clone of the fixture herd, at the same floor and the same party, read at the take's
    /// own expectation and averaged over [`LEDGER_TURNS`].
    ///
    /// It is the *whole* take (`AnimalTake::killed`), quantiser and all, so the comparison below is
    /// against the number the turn actually credits rather than against an intermediate the curve
    /// could trivially echo.
    ///
    /// # ⛔ It must be a RUN, and that is the whole defect this suite failed to catch
    ///
    /// A single turn's `killed` is floored to whole animals with the remainder **banked** on the
    /// quarry, so an aurochs crew of eight reads `0` on a turn while genuinely taking `0.75` a turn.
    /// Comparing the published curve against one turn compared zero to zero at eleven of the
    /// thirteen crews a stepper can reach, and passed on a curve that quoted the player nothing.
    ///
    /// **The stock is held level** between turns — the wound ledger carries (`hunt_take` writes it
    /// back), the biomass does not. These fixtures are about the engagement and the fight, so a herd
    /// that visibly drew down would be measuring the drawdown instead; the thin-herd fixture wants
    /// its escapement room *constant* for the same reason.
    ///
    /// # ⛔ Each turn REGROWS before it takes, because the sim's turn does
    ///
    /// `hunt_take` runs in Population, one whole stage after Logistics' [`regrow_biomass`], so a
    /// harness that called it on the raw fixture was pricing a turn the sim never runs — and the
    /// curve it was comparing against is asked between turns, about the take *after* the next
    /// regrowth. Both sides now stand at the same point in the turn, which is the only arrangement
    /// in which *"the curve reproduces the sim's own take"* is a statement about one quantity.
    ///
    /// Measured in play before the two were aligned: a Rabbit Warren's row published
    /// `actualYield 0.0216` — four rabbits — with a positive `arrivalSchedule` in all twenty slots,
    /// while the curve resolved at the same herd's *post-take* stock read **zero at every crew
    /// size**. This harness passed throughout, because it was reading the herd a turn early on both
    /// sides at once.
    fn sim_take(
        world: &World,
        species: &str,
        body_mass: f32,
        biomass: f32,
        workers: u32,
        floor: f32,
    ) -> f32 {
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let party = fixture_party(world, body_mass, workers);
        let mut herd = herd_of_biomass(species, body_mass, biomass);
        let mut killed = 0.0_f32;
        for _ in 0..LEDGER_TURNS {
            // Logistics, then Population — the turn order the take actually runs in, so what is held
            // level between turns is the stock *before* the regrowth.
            crate::fauna::regrow_biomass(&mut herd, &fauna);
            killed += crate::systems::hunt_take(
                &mut herd,
                workers,
                floor,
                RESIDENT_CARRY_PER_WORKER,
                &party,
                &fauna,
                // A resident band banks its whole take — the same `f32::INFINITY` the Hunt arm
                // passes.
                f32::INFINITY,
                HuntDraw::EXPECTED,
            )
            .take
            .killed as f32;
            herd.biomass = biomass;
        }
        killed / LEDGER_TURNS as f32
    }

    /// A resident hunter's shipped haul tier, so `carryable` is a real bound rather than a
    /// convenient infinity — the curve has to survive the client's other two `min` arms being live.
    const RESIDENT_CARRY_PER_WORKER: f32 = 40.0;

    /// **THE CURVE REPRODUCES THE SIM'S OWN TAKE AT EVERY CREW IN ITS RANGE.**
    ///
    /// Swept, never sampled. On Wild Aurochs the binding term flips from the fight to the engagement
    /// and back *between* crews 8 and 12, so a spot-check at either end passes while a 28% error
    /// sits in the middle — which is exactly how this arc shipped a green suite over a broken take
    /// before.
    ///
    /// The client's remaining arithmetic is written out in full here (`affordable`, `carryable`,
    /// then the published row) because that composition **is** the contract: if the row needed any
    /// other treatment to land on the sim's number, this is where it would show.
    ///
    /// **Compared against the SUSTAINED take** ([`sim_take`]), not one turn's. The row is a rate, and
    /// one turn of the sim is a floored count that is `0` for most crews on most fixtures — so the
    /// single-turn form of this comparison passed identically on the curve that quoted every aurochs
    /// crew from 1 to 11 a flat zero.
    fn the_curve_reproduces_the_take(species: &str, body_mass: f32, biomass: f32, floor: f32) {
        let mut world = world_hunting_biomass(species, body_mass, biomass);
        let rows = crew_curve(&mut world, &crew_ask(SWEEP_CREW, floor));
        assert_eq!(
            rows.len(),
            SWEEP_CREW as usize,
            "one row per crew size, `1..=max_workers`"
        );
        // **THE CLIENT'S ROOM ARM IS NEXT TURN'S, and it is written that way here because that is
        // what the client does** — `SourceForecast.escapement_room_next_turn` regrows before it
        // subtracts the floor, for the same reason the curve does: the take being priced runs after
        // the next Logistics pass. Composing a *standing* room against a next-turn row would put the
        // two halves of one `min` a whole turn apart, which is the defect this suite now pins.
        let ceiling = {
            let fauna = world.resource::<FaunaConfigHandle>().get();
            crate::fauna::herd_take_room(
                &crate::fauna::next_turns_quarry(
                    &herd_of_biomass(species, body_mass, biomass),
                    &fauna,
                ),
                floor,
                &fauna,
            )
        };
        // **WHAT A FROZEN STOCK THROWS AWAY, NAMED AND BOUNDED** — the whole of the slack allowed
        // below, and `0` on every fixture whose room divides evenly into bodies.
        //
        // `sim_take` holds the herd's biomass level between turns so these fixtures measure the
        // engagement and the fight rather than a drawdown. That reset is also what discards the
        // **remainder**: in the sim a crew that may spare `3.9` bodies kills `3` and leaves `0.9`
        // standing, which joins next turn's room — the herd's own biomass is the accumulator, and
        // it is the reason the curve publishes an un-floored rate ([`fauna::animals_sparable`]). A
        // frozen stock cannot integrate that, so it pays `floor` every turn for ever, and the two
        // readings differ by exactly the fraction the floor drops, carried through the retreat.
        //
        // **The fight's remainder is NOT in this figure, and does not need to be**: `hunt_take`
        // writes `herd.wounds` back and the harness keeps it, so that quantum already integrates.
        // The room's was the one the harness dropped — the same asymmetry the curve itself had.
        let discarded_by_the_frozen_stock = {
            let sparable = ceiling / body_mass;
            let wariness = world
                .resource::<FaunaConfigHandle>()
                .get()
                .wariness_for(species);
            (sparable - sparable.floor())
                * fixture_party(&world, body_mass, 1).stay_fraction(wariness)
        };
        let mut saw_a_kill = false;
        for (index, row) in rows.iter().enumerate() {
            let workers = index as u32 + 1;
            assert_eq!(
                row.workers, workers,
                "rows ascend from 1 and echo their crew"
            );
            // The client's whole job: two caps it already holds, and the published row.
            // **THE ROOM ARM IS NOT FLOORED**, because the client does not floor it —
            // `DrawerComposeController._hunt_delivered_and_waste` composes `min(room / body,
            // haulable, brought_down)` and only the HAUL arm takes a `floor`. A `.floor()` here
            // modelled a client that does not exist.
            let sparable = ceiling / body_mass;
            let carryable = ((workers as f32 * RESIDENT_CARRY_PER_WORKER) / body_mass)
                .floor()
                .max(1.0);
            let composed = sparable.min(carryable).min(row.animals_likely);
            let paid = sim_take(&world, species, body_mass, biomass, workers, floor);
            assert!(
                paid <= composed + SUSTAINED_RATE_EPSILON
                    && composed <= paid + discarded_by_the_frozen_stock + SUSTAINED_RATE_EPSILON,
                "a crew of {workers} on {species}: the published curve, min'd against the two caps \
                 the client already derives, reads {composed}/turn where `hunt_take` sustains \
                 {paid}/turn (a frozen stock may discard at most \
                 {discarded_by_the_frozen_stock}/turn)"
            );
            saw_a_kill |= composed > 0.0;
        }
        assert!(
            saw_a_kill,
            "{species} must actually be killable somewhere in `1..={SWEEP_CREW}`, or every row \
             compared zero to zero and a broken curve would pass"
        );
    }

    #[test]
    fn the_curve_reproduces_the_take_where_the_fight_binds() {
        let world = world_hunting(AUROCHS, AUROCHS_BODY);
        // **THE PRECONDITION.** Without it the pair could pass with both sides collapsing onto the
        // engagement bound and the fight — the whole subject of this test — never entering.
        let (killed, stayed) = binding_terms(&world, AUROCHS, AUROCHS_BODY, FAT_HERD, 1);
        assert!(
            fight_binds(&world, AUROCHS, AUROCHS_BODY, 1),
            "the aurochs fixture must be FIGHT-bound at a crew of one ({killed} killed per turn \
             against {stayed} standing), or this test is not about the fight at all"
        );
        the_curve_reproduces_the_take(AUROCHS, AUROCHS_BODY, FAT_HERD, STRIP_IT_BARE);
    }

    /// **EVERY EXTRA HUNTER BUYS TAKE — THE ACCEPTANCE TEST for the un-floored reach.**
    ///
    /// The reach was `floor(w × engage_rate).max(1)`, so the shipped Wild Boar's `0.33` answered
    /// **one animal for every crew from 1 to 6**: four hunters took exactly what one took, and the
    /// row read `0.18 food/turn` either way. The run is now strictly increasing, and it is asserted
    /// on the *take* (the sustained kill rate the sim pays) rather than on the reach, because a cap
    /// that rises while the take does not is the defect wearing a different hat.
    #[test]
    fn the_curve_reproduces_the_take_where_engagement_binds() {
        let world = world_hunting(BOAR, BOAR_BODY);
        // **THE PRECONDITION** — the engagement really is the binding term on this fixture, so the
        // rise below is the reach's and not the fight's.
        assert!(
            !fight_binds(&world, BOAR, BOAR_BODY, 6),
            "the boar fixture must be ENGAGEMENT-bound at a crew of six, or this test is not about \
             the engagement"
        );
        let rising: Vec<f32> = (1..=6)
            .map(|workers| binding_terms(&world, BOAR, BOAR_BODY, FAT_HERD, workers).0)
            .collect();
        assert!(
            rising.windows(2).all(|pair| pair[1] > pair[0]) && rising[0] > 0.0,
            "each crew from 1 to 6 must take STRICTLY more boar than the crew below it ({rising:?})\
             — a flat run there is the retired `floor(w × engage_rate).max(1)`, under which four \
             hunters fed a band no better than one"
        );
        the_curve_reproduces_the_take(BOAR, BOAR_BODY, FAT_HERD, STRIP_IT_BARE);
    }

    /// **THE ROOM CLAMPS THE ENGAGEMENT, NOT THE OUTCOME** — the third regime, and the one whose
    /// *order* is easy to get wrong in a way no fat-herd fixture can see.
    ///
    /// The escapement room bounds what the party goes after **before** the retreat
    /// (`fauna::animals_affordable`), because restraint is free: a crew at its floor does not corner
    /// animals it will decline to kill. Clamping after the retreat instead would retreat a *bigger*
    /// party than the take does and over-quote every turn the room binds — a whole extra
    /// `1 / stay_fraction` of animals on a wary quarry.
    ///
    /// It needs a thin herd. On the fat fixtures above the room never binds at any crew, so this
    /// regime is invisible there and a mis-ordered clamp passes them all.
    #[test]
    fn the_curve_reproduces_the_take_where_the_room_binds() {
        /// Thin enough that the stock standing above the floor covers fewer deer than the crew can
        /// reach — the only condition under which the clamp's *position* is observable at all.
        const THIN_HERD: f32 = 70.0;

        let world = world_hunting_biomass(DEER, DEER_BODY, THIN_HERD);
        let fauna = world.resource::<FaunaConfigHandle>().get();
        // **THE PRECONDITIONS, and there are two**, because the mis-ordered clamp is only visible
        // where the room is the tightest term *and* the retreat is strong enough for the difference
        // between retreating four animals and retreating twenty-four to survive the whole-animal
        // floor. A calm quarry hides the defect; a fat herd hides it; the aurochs fixture above
        // hides it twice over, which is why this one is a Red Deer.
        let room = crate::fauna::herd_take_room(
            &herd_of_biomass(DEER, DEER_BODY, THIN_HERD),
            STRIP_IT_BARE,
            &fauna,
        );
        let affordable = crate::fauna::animals_affordable(room, DEER_BODY);
        let reach = crate::fauna::animals_engaged(SWEEP_CREW, fauna.engage_rate_for(DEER));
        assert!(
            affordable > 0.0 && affordable < reach,
            "the thin fixture must let the ROOM bind at a crew of {SWEEP_CREW} ({affordable} \
             animals affordable against a reach of {reach}) while still affording a kill, or this \
             test is not about the room at all"
        );
        let stay = crate::fauna::stay_fraction(fauna.wariness_for(DEER), NEUTRAL_DISPERSION);
        assert!(
            stay < MOSTLY_BOLTS,
            "the fixture quarry must be WARY ({stay} stays), or clamping the room after the retreat \
             instead of before it moves nothing and this test cannot see the difference"
        );
        drop(fauna);

        // **AND THE ORDER ITSELF, asserted directly.** The reproduction sweep below compares the
        // curve against `hunt_take`, and the two share [`crate::fauna::resolve_hunt_engagement`] —
        // so a clamp moved on that seam moves *both* sides and the sweep cannot see it. This is the
        // half that can: clamping the room before the retreat means what stands can never exceed
        // `affordable x stay_fraction`, where clamping it after would leave the whole `affordable`
        // standing. The shipped suite guarded neither.
        let stayed = binding_terms(&world, DEER, DEER_BODY, THIN_HERD, SWEEP_CREW).1;
        assert!(
            stayed <= affordable * stay + LEDGER_AVERAGE_EPSILON,
            "{stayed} deer stood against {affordable} affordable at a {stay} stay — the escapement \
             room must clamp the ENGAGEMENT, before the retreat, or the party retreats a bigger \
             crowd than the take does and every room-bound turn is over-quoted"
        );

        the_curve_reproduces_the_take(DEER, DEER_BODY, THIN_HERD, STRIP_IT_BARE);
    }

    /// **A SOURCE HELD AT ITS FLOOR — the state a working crew keeps a herd in, and the one the
    /// standing-frame curve answered `0` for at every crew size.**
    ///
    /// The three fixtures above all stand well clear of their floor, so the curve's *frame* — which
    /// point in the turn it reads the herd at — cannot be seen in any of them: a fat herd's room is
    /// enormous either way. This one is the regime the game spends its time in. A crew working a
    /// source draws it back to the floor every turn, so at capture time
    ///
    /// - the escapement room is approximately **nothing** (the take just removed it), and
    /// - [`Herd::growth_this_turn`] is **zero**, because it is `biomass − biomass_before_regrowth`
    ///   and the take is subtracted from `biomass` after `regrow_biomass` stamps the pair — so the
    ///   growth-share backstop that exists to pay a source sitting at its floor is switched off by
    ///   exactly the harvesting that puts it there.
    ///
    /// Reported from play on a Rabbit Warren: the row published `actualYield 0.0216` — four rabbits —
    /// with a positive `arrivalSchedule` in all twenty slots, beside a compose sheet reading
    /// *"these hunters bring down ≈0 Rabbit Warren/turn"* and a `huntUsefulWorkers` of `0`.
    ///
    /// **The precondition is the falsification, and it is asserted rather than described**: the
    /// standing room here affords **zero whole animals**, so a curve that reads the herd as it stands
    /// is identically zero at every crew and every assertion below fails. Nothing else in this
    /// section stages that.
    #[test]
    fn the_curve_reproduces_the_take_on_a_herd_held_at_its_floor() {
        /// Exactly [`HELD_AT_THE_FLOOR`] of `herd_of_biomass`'s capacity — a herd a crew has drawn
        /// back to its floor and holds there.
        const ON_THE_FLOOR: f32 = 50_000.0;
        /// The floor that stock sits on.
        const HELD_AT_THE_FLOOR: f32 = 0.5;

        let mut world = world_hunting_biomass(DEER, DEER_BODY, ON_THE_FLOOR);
        let held = herd_of_biomass(DEER, DEER_BODY, ON_THE_FLOOR);
        {
            let fauna = world.resource::<FaunaConfigHandle>().get();
            assert_eq!(
                held.growth_this_turn(),
                0.0,
                "the fixture must stand as a WORKED source does — the take has already eaten this \
                 turn's growth, which is what silences the growth-share backstop"
            );
            let standing = crate::fauna::herd_take_room(&held, HELD_AT_THE_FLOOR, &fauna);
            assert_eq!(
                crate::fauna::animals_affordable(standing, DEER_BODY),
                0.0,
                "the standing room ({standing}) must afford NO whole animal, or the old frame \
                 answers the same number as the new one and this fixture proves nothing"
            );
            // Regrown HERE rather than through `fauna::next_turns_quarry`, deliberately: that is the
            // seam under test, and a precondition written in terms of it would go quiet in exactly
            // the sabotage this fixture exists to catch — leaving the reproduction assertion below to
            // report the failure instead of this one masking it.
            let mut grown = held.clone();
            crate::fauna::regrow_biomass(&mut grown, &fauna);
            let next = crate::fauna::herd_take_room(&grown, HELD_AT_THE_FLOOR, &fauna);
            assert!(
                crate::fauna::animals_affordable(next, DEER_BODY) > 0.0,
                "…while next turn's room ({next}) affords a real take — the whole of the gap this \
                 fixture exists to pin"
            );
        }

        // **AND THE PUBLISHED SCALAR AGREES WITH THE ROWS**, since both come out of the one producer
        // and the play report had them disagreeing by the whole of the answer: `huntUsefulWorkers`
        // read `NO_USEFUL_CREW` on a row that was feeding the band. Walked off the SOCKET rows rather
        // than by calling the curve again, so this is the two transports compared and not a function
        // compared with itself.
        let curve: Vec<crate::fauna::HuntCrewTake> =
            crew_curve(&mut world, &crew_ask(SWEEP_CREW, HELD_AT_THE_FLOOR))
                .iter()
                .map(|row| crate::fauna::HuntCrewTake {
                    workers: row.workers,
                    low: row.animals_low,
                    likely: row.animals_likely,
                    high: row.animals_high,
                })
                .collect();
        assert!(
            crate::fauna::hunt_useful_crew(&curve) > crate::fauna::NO_USEFUL_CREW,
            "a herd paying a real take must publish a real useful-crew cap, not `no crew is useful \
             here`"
        );

        the_curve_reproduces_the_take(DEER, DEER_BODY, ON_THE_FLOOR, HELD_AT_THE_FLOOR);
    }

    /// **THE BINDING TERM FLIPS INSIDE THE STEPPER'S OWN RANGE**, which is why the sweep above is a
    /// sweep. Asserted directly so a retune that flattened the curve into something a scalar *could*
    /// carry fails here, loudly, rather than leaving the curve looking like over-engineering.
    ///
    /// # It is the ROOM that flips it, and after the un-floored reach it is the only thing that can
    ///
    /// The reach (`w × engage_rate × stay`) and the fight (`w × landed × damage / durability`) are
    /// **both linear in the crew**, so on a herd nobody can exhaust, whichever of them is smaller at
    /// one hunter is smaller at every hunter — the fight/engagement flip this test used to sweep for
    /// existed only because `floor(w × engage_rate).max(1)` made the reach a *staircase*, and a
    /// staircase crossing a line is an artefact of the rounding rather than a fact about hunting.
    ///
    /// What genuinely is not linear is the **escapement room**: it does not grow with the crew at
    /// all. So a thin herd is reach-bound at small crews and room-bound at large ones, the curve
    /// rises and then stops rising inside one stepper's range, and that plateau is precisely what no
    /// per-hunter scalar can carry. Read off the **published rows**, because that is the transport
    /// the client's stepper walks.
    #[test]
    fn the_binding_term_changes_within_one_stepper_range() {
        /// Thin enough that the stock standing above the floor covers fewer deer than a full crew
        /// can reach, and fat enough that a small crew is still the tighter term.
        const THIN_HERD: f32 = 70.0;

        let mut world = world_hunting_biomass(DEER, DEER_BODY, THIN_HERD);
        let rows = crew_curve(&mut world, &crew_ask(SWEEP_CREW, STRIP_IT_BARE));
        let plateau = crate::fauna::hunt_useful_crew(
            &rows
                .iter()
                .map(|row| crate::fauna::HuntCrewTake {
                    workers: row.workers,
                    low: row.animals_low,
                    likely: row.animals_likely,
                    high: row.animals_high,
                })
                .collect::<Vec<_>>(),
        );
        let takes: Vec<f32> = rows.iter().map(|row| row.animals_likely).collect();
        assert!(
            plateau > 1 && plateau < SWEEP_CREW,
            "the curve must still be climbing at some crews and flat at others within \
             `1..={SWEEP_CREW}` (it plateaus at {plateau}: {takes:?}) — a single per-hunter rate \
             cannot express that, which is the whole reason this reply is a curve"
        );
    }

    /// **THE CREW OF ONE**, whose reach is a *fraction of one animal* — `1 × 0.17` of an aurochs.
    /// Pinned on its own because a sweep that started at two would never see it, and because this is
    /// the crew a rounding at either end of the pipeline silently deletes: the retired
    /// `max(.., 1)` used to hand it a whole animal it had not reached, and a bare `floor()` would
    /// hand it nothing, for ever.
    #[test]
    fn a_crew_of_one_reaches_one_animal_and_the_curve_says_so() {
        let world = world_hunting(AUROCHS, AUROCHS_BODY);
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let rate = fauna.engage_rate_for(AUROCHS);
        let stay = crate::fauna::stay_fraction(fauna.wariness_for(AUROCHS), NEUTRAL_DISPERSION);
        assert!(
            rate < 1.0,
            "the fixture must have a FRACTIONAL engage_rate ({rate}) or this test is not about the \
             part body at all"
        );
        assert_eq!(
            crate::fauna::animals_engaged(1, rate),
            rate,
            "one hunter reaches its own rate — a part body, neither rounded up to one nor down to \
             nothing"
        );
        drop(fauna);

        let mut world = world;
        let rows = crew_curve(&mut world, &crew_ask(1, STRIP_IT_BARE));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].workers, 1);

        // **THE PART BODY SURVIVES THE RETREAT, stated as the difference it makes.** The retreat
        // draws whole bodies and keeps the remainder in closed form, so what stands in front of a
        // lone hunter is `rate × stay_fraction` of an animal — a re-floor at this stage would leave
        // it exactly zero and the fight nothing to bank.
        let (killed, stayed) = binding_terms(&world, AUROCHS, AUROCHS_BODY, FAT_HERD, 1);
        assert_eq!(
            stayed,
            rate * stay,
            "one hunter corners `engage_rate` of an animal and keeps `stay_fraction` of that"
        );
        assert!(
            stayed > 0.0 && stayed < 1.0,
            "the fixture must actually be a PART body ({stayed}), or it exercises neither rounding"
        );

        // **AND THE LONE HUNTER REALLY DOES GET ONE.** A single turn reads `0` here — a
        // 150-durability aurochs takes about sixteen hunter-turns and the damage is banked, not lost
        // — so asserting on turn one alone would be asserting on the pulse rather than on the reach.
        // That bank ([`crate::combat::DamageLedger`]) is what makes a sub-body reach a *cadence*
        // rather than a never.
        assert!(
            killed > 0.0,
            "a crew of one must bring an aurochs down eventually ({killed}/turn sustained); the \
             wound ledger is what carries its part body between turns"
        );
        let paid = sim_take(&world, AUROCHS, AUROCHS_BODY, FAT_HERD, 1, STRIP_IT_BARE);
        assert!(
            (rows[0].animals_likely - paid).abs() <= SUSTAINED_RATE_EPSILON,
            "and the published crew-of-one row ({}) is still what the turn pays, sustained \
             ({paid}/turn) — the row is a RATE, so it says `one aurochs about every eleven turns` \
             rather than the `0` a single floored turn reports",
            rows[0].animals_likely
        );
        assert!(
            rows[0].animals_likely > 0.0,
            "…and it is not zero, which is the answer the floored count gave the player"
        );
    }

    /// **`(sustained kills per turn, animals standing to be killed)` for one crew** — the pair whose
    /// comparison says which term is binding, read off the production seam
    /// ([`crate::fauna::resolve_hunt_engagement`]) rather than restated, so a precondition cannot
    /// quietly assert the thing it is guarding.
    ///
    /// **Averaged over [`LEDGER_TURNS`], because a single turn cannot answer the question.**
    /// `HuntFight::brought_down` is floored to WHOLE animals and the unfinished damage is banked on
    /// the quarry (`combat::DamageLedger`), so a party working a 150-durability aurochs reads `0` for
    /// several turns and then `1`. Reading turn one would call every big-game party fight-bound at a
    /// rate of zero. The average over a run with the ledger carrying is the party's real capacity —
    /// and, because the ledger also clamps each blow to what is standing, it is exactly
    /// `min(fight_rate, stayed)`, which is what makes the comparison below a decision procedure.
    fn binding_terms(
        world: &World,
        species: &str,
        body_mass: f32,
        biomass: f32,
        workers: u32,
    ) -> (f32, f32) {
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let party = fixture_party(world, body_mass, workers);
        let mut herd = herd_of_biomass(species, body_mass, biomass);
        let mut killed = 0.0_f32;
        let mut stayed = 0.0_f32;
        for _ in 0..LEDGER_TURNS {
            let resolved = crate::fauna::resolve_hunt_engagement(
                &herd,
                &fauna,
                &party,
                workers,
                STRIP_IT_BARE,
                HuntDraw::EXPECTED,
                crate::fauna::EngagementQuantum::WholeAnimals,
            );
            // The wounds carry, exactly as `hunt_take` carries them. The herd's biomass is left
            // alone: these fixtures stand far above every floor, so the room is constant and the
            // only thing moving between turns is the damage banked on the quarry.
            herd.wounds = resolved.fight.wounds;
            killed += resolved.fight.brought_down;
            stayed = resolved.stayed;
        }
        (killed / LEDGER_TURNS as f32, stayed)
    }

    /// Long enough that the slowest fixture kill (a 150-durability aurochs under one hunter, ~16
    /// turns a body) is averaged over many completed animals rather than over the accident of where
    /// the window ended.
    const LEDGER_TURNS: u32 = 400;

    /// **Does the FIGHT bind for this crew, or the engagement?**
    ///
    /// The ledger clamps each turn's blow to what is standing, so the sustained kill rate is exactly
    /// `min(fight_rate, stayed)`: strictly below what stood means the party could not finish what it
    /// cornered (the fight binds); level with it means it killed everything that stayed and wanted
    /// more animals (the engagement binds).
    fn fight_binds(world: &World, species: &str, body_mass: f32, workers: u32) -> bool {
        let (killed, stayed) = binding_terms(world, species, body_mass, FAT_HERD, workers);
        killed < stayed - LEDGER_AVERAGE_EPSILON
    }

    /// Slack for the float accumulation in [`binding_terms`]'s sum — an engagement-bound crew kills
    /// *exactly* what stands, so anything short of it by more than a rounding error is the fight.
    const LEDGER_AVERAGE_EPSILON: f32 = 1e-4;

    /// **The gap a FINITE run leaves between the published rate and a measured average**, and it is
    /// derived rather than tuned.
    ///
    /// At most one unfinished body is sitting on the quarry's wound ledger when the window closes,
    /// and it is never counted — so a [`LEDGER_TURNS`]-turn average of the floored take runs up to
    /// `1 / LEDGER_TURNS` animals a turn *below* the rate. Widening the window tightens the bound on
    /// its own, which is what stops this from becoming a number somebody nudges.
    const SUSTAINED_RATE_EPSILON: f32 = 1.0 / LEDGER_TURNS as f32;

    /// Take everything the herd can spare: these fixtures are about the engagement and the fight, so
    /// the floor is deliberately not a term in them.
    const STRIP_IT_BARE: f32 = 0.0;
    /// The shipped Wild Aurochs body — stated so `herd_of` and the client-side `affordable` /
    /// `carryable` arithmetic in the sweep divide by the same number the roster does.
    const AUROCHS_BODY: f32 = 120.0;
    /// The shipped Wild Boar body.
    const BOAR_BODY: f32 = 12.0;
    /// The shipped mid-game quarry, and the *wary* one: `wariness 0.65`, so a party keeps barely a
    /// third of what it corners. That is what makes the room-clamp's position observable.
    const DEER: &str = "Red Deer";
    /// The shipped Red Deer body.
    const DEER_BODY: f32 = 15.0;
    /// A quarry that keeps less than half of what a party reaches — "wary enough that where the
    /// retreat sits in the order actually matters".
    const MOSTLY_BOLTS: f32 = 0.5;
    /// A kit that neither quietens nor advertises the party — the identity on the retreat, so the
    /// species' own `wariness` is the whole of it. The shipped spear kit declares no `dispersion`.
    const NEUTRAL_DISPERSION: f32 = 1.0;

    /// **THE BAND COLLAPSES TO A POINT, BIT-FOR-BIT, WHERE NOTHING IS STOCHASTIC** — the
    /// `actualYield*` invariant, on the curve.
    ///
    /// Both stochastic stages answer a *degenerate* distribution here: the retreat is held at
    /// `wariness 0` and the shipped `hit_chance` is `1.0`, so `attacks_landed_at` and
    /// `animals_that_stay` return their identities whatever quantile is asked for. `assert_eq!` on
    /// floats is therefore the correct assertion and not a tolerance waiting to be widened.
    #[test]
    fn the_band_collapses_to_a_point_when_nothing_is_stochastic() {
        let mut world = world_hunting(AUROCHS, AUROCHS_BODY);
        // **THE PRECONDITIONS**, both of them: a band that is open on either stage would make this
        // pass for the wrong reason, and a `forecast_range_sigmas` of zero would make it pass for no
        // reason at all.
        let combat = world.resource::<CombatConfigHandle>().get();
        assert_eq!(
            combat.tuning().hit_chance,
            1.0,
            "the shipped tuning must be certain, or the fight's binomial has real spread and this \
             is not the degenerate case"
        );
        assert!(
            combat.forecast_range_sigmas > 0.0,
            "a zero band width would collapse every quantile trivially and assert nothing"
        );
        let fauna = world.resource::<FaunaConfigHandle>().get();
        assert!(
            fauna.wariness_for(AUROCHS) > 0.0,
            "the shipped roster must be wary here, so holding it at zero below is a real change"
        );
        world.insert_resource(FaunaConfigHandle::new(std::sync::Arc::new(
            fauna.without_retreat(),
        )));

        let rows = crew_curve(&mut world, &crew_ask(SWEEP_CREW, STRIP_IT_BARE));
        let mut saw_a_kill = false;
        for row in &rows {
            assert_eq!(
                (row.animals_low, row.animals_high),
                (row.animals_likely, row.animals_likely),
                "with no randomness in either stage, low/likely/high must be the SAME NUMBER at a \
                 crew of {} — not merely close",
                row.workers
            );
            saw_a_kill |= row.animals_likely > 0.0;
        }
        assert!(
            saw_a_kill,
            "the collapsed curve must still kill something, or three zeroes proved the invariant"
        );
    }

    /// **THE BAND IS `O(sqrt(w))`, AND A PER-HUNTER BAND WOULD BE `O(w)`** — the error a scalar
    /// shape would have shipped invisibly, because at the shipped `hit_chance = 1.0` the band is a
    /// point and no fixture would ever have caught it.
    ///
    /// Staged at a sub-certain `hit_chance` so the fight's binomial is genuinely open, then asserted
    /// on the shape rather than on a number: quadrupling the crew must **less than double** the
    /// per-hunter spread away — under a linear band it would hold it exactly constant, and under any
    /// super-linear one it would grow.
    #[test]
    fn the_band_narrows_per_hunter_as_the_crew_grows() {
        /// Open enough that the binomial has real variance at every crew below, and far enough from
        /// `1.0` that the degenerate short-circuit in `attacks_landed_at` cannot be reached.
        const OPEN_HIT_CHANCE: f32 = 0.5;
        /// Big enough that the whole-animal floor is not the dominant term in the spread — a crew
        /// whose whole band rounds into one body says nothing about its width.
        const SMALL_CREW: u32 = 40;
        const BIG_CREW: u32 = 160;

        let mut world = world_hunting(AUROCHS, AUROCHS_BODY);
        let mut combat = (*world.resource::<CombatConfigHandle>().get()).clone();
        combat.hit_chance = OPEN_HIT_CHANCE;
        world.insert_resource(CombatConfigHandle::new(std::sync::Arc::new(combat)));

        let rows = crew_curve(&mut world, &crew_ask(BIG_CREW, STRIP_IT_BARE));
        let spread = |workers: u32| {
            let row = &rows[workers as usize - 1];
            assert_eq!(row.workers, workers);
            row.animals_high - row.animals_low
        };
        let small = spread(SMALL_CREW);
        let big = spread(BIG_CREW);
        // **LIVENESS FIRST.** "The band narrows" is trivially true of two zeroes, and a fixture that
        // closed the band would assert exactly nothing.
        assert!(
            small > 0.0 && big > 0.0,
            "the staged `hit_chance` must leave the band OPEN at both crews ({small} / {big}), or \
             this test proves nothing about its shape"
        );
        let ratio = big / small;
        assert!(
            ratio < 2.5,
            "quadrupling the crew widened the band {ratio}x. A `sqrt` band widens ~2x; a PER-HUNTER \
             band multiplied by the crew would widen 4x — which is the shape this reply refuses to \
             publish"
        );
        let per_hunter_big = big / BIG_CREW as f32;
        let per_hunter_small = small / SMALL_CREW as f32;
        assert!(
            per_hunter_big < per_hunter_small,
            "the per-hunter spread must NARROW as the crew grows ({} at {BIG_CREW} vs {} at \
             {SMALL_CREW}) — that is what makes the band unpublishable as a per-hunter scalar",
            per_hunter_big,
            per_hunter_small
        );
    }

    /// **A curve is answered at the RESIDENT tuning, never the expedition's.** The trip sheet beside
    /// it prices a detached raid at `expedition_danger_multiplier` (1.5x lethality as shipped), and
    /// borrowing those rows for the Assign Herders panel would over-quote the fight by half again.
    #[test]
    fn the_crew_curve_fights_at_resident_lethality_not_the_expeditions() {
        let mut world = world_hunting(AUROCHS, AUROCHS_BODY);
        let combat = world.resource::<CombatConfigHandle>().get();
        assert!(
            combat.expedition_danger_multiplier > 1.0,
            "the shipped multiplier must actually differ, or the two tunings are the same fixture"
        );
        let resident = crew_curve(&mut world, &crew_ask(SWEEP_CREW, STRIP_IT_BARE));

        // The same curve with the danger multiplier folded into the BASE tuning — i.e. what the rows
        // would read if the answer had been priced as a raid.
        let mut world_as_a_raid = world_hunting(AUROCHS, AUROCHS_BODY);
        let mut bloodier = (*combat).clone();
        bloodier.lethality *= combat.expedition_danger_multiplier;
        world_as_a_raid.insert_resource(CombatConfigHandle::new(std::sync::Arc::new(bloodier)));
        let as_a_raid = crew_curve(&mut world_as_a_raid, &crew_ask(SWEEP_CREW, STRIP_IT_BARE));

        assert!(
            resident
                .iter()
                .zip(&as_a_raid)
                .any(|(here, raid)| here.animals_likely < raid.animals_likely),
            "a resident crew must be quoted a SMALLER take than the same crew priced at expedition \
             lethality somewhere on the sweep — if the two curves are identical this test has \
             stopped distinguishing them"
        );
    }

    /// A crew cap of zero asks for nothing, and is answered with nothing — the same "0 scans
    /// nothing" contract `max_party_workers` already carries, so a panel with no assignable workers
    /// gets an empty curve rather than a refusal.
    #[test]
    fn a_band_that_can_field_nobody_gets_an_empty_curve() {
        let mut world = world_hunting(BOAR, BOAR_BODY);
        assert!(crew_curve(&mut world, &crew_ask(0, STRIP_IT_BARE)).is_empty());
    }

    // --- THE CURVE IS A RATE ---------------------------------------------------------------------

    /// **THE DEFECT, PINNED: a crew whose whole turn floors to zero must still read its rate.**
    ///
    /// Eight speared hunters on a Wild Aurochs deal `8 × (20 − 6) = 112` damage into a body worth
    /// `150`, and the `engage_rate 0.17` lets them corner exactly one animal of which `0.8` stays —
    /// so the blow is capped at `0.8 × 150 = 120` and `floor(112 / 150)` is **`0`**. The published
    /// curve read `0` there, and the panel printed *"≈0 WILD AUROCHS/TURN · 0.00 FOOD"* beside a work
    /// row quoting food from the very same take.
    ///
    /// The expectation is asserted against the **continuous form, rebuilt from the party and the
    /// quarry** rather than read back off `HuntFight` — a curve that echoed its own intermediate
    /// would satisfy any comparison against itself.
    #[test]
    fn a_crew_whose_turn_floors_to_zero_still_reads_its_rate() {
        /// The crew the defect was measured at: past the point where the fight binds, and far below
        /// the twelfth hunter where the engagement staircase finally steps.
        const ZERO_READING_CREW: u32 = 8;

        let mut world = world_hunting(AUROCHS, AUROCHS_BODY);
        let party = fixture_party(&world, AUROCHS_BODY, ZERO_READING_CREW);
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let quarry = fauna.quarry_fight_for(AUROCHS);
        let engagement = crate::fauna::resolve_hunt_engagement(
            &herd_of_biomass(AUROCHS, AUROCHS_BODY, FAT_HERD),
            &fauna,
            &party,
            ZERO_READING_CREW,
            STRIP_IT_BARE,
            HuntDraw::EXPECTED,
            crate::fauna::EngagementQuantum::WholeAnimals,
        );

        // **THE PRECONDITION, and without it this test cannot tell the defect from the fix.** The
        // single-turn floored count really must be zero here — on an un-hunted herd, which is the
        // state a query always answers from.
        assert_eq!(
            engagement.fight.brought_down, 0.0,
            "a crew of {ZERO_READING_CREW} must bring down NO whole aurochs in one turn, or the \
             fixture has stopped reproducing the defect and this test asserts nothing"
        );

        // The continuous form, rebuilt: every crew's landed damage over the quarry's durability,
        // capped by what stood there. At the shipped `hit_chance = 1.0` every hunter's strike lands,
        // so the crew's damage is its head count times the gate.
        assert_eq!(
            party.tuning.hit_chance, 1.0,
            "this closed form assumes every strike lands; a sub-certain tuning needs the binomial \
             here instead"
        );
        let damage: f32 = party
            .crews
            .iter()
            .map(|crew| {
                ZERO_READING_CREW as f32
                    * crew.share
                    * crate::combat::strike_damage(crew.hunter.attack, quarry.profile.defense)
                    * party.tuning.lethality
            })
            .sum();
        let continuous = (damage / quarry.profile.durability).min(engagement.stayed);
        assert!(
            continuous > 0.0,
            "the fixture crew must actually be able to hurt the quarry, or `0 == 0` would pass"
        );
        drop(fauna);

        let rows = crew_curve(&mut world, &crew_ask(ZERO_READING_CREW, STRIP_IT_BARE));
        let published = rows[ZERO_READING_CREW as usize - 1].animals_likely;
        assert!(
            (published - continuous).abs() <= LEDGER_AVERAGE_EPSILON,
            "the curve published {published} aurochs/turn for a crew of {ZERO_READING_CREW} where \
             the fight's own arithmetic says {continuous} — the row is the RATE, not the turn's \
             floored body count"
        );
    }

    /// **THE PLATEAU, SWEPT** — the eleven adjacent stepper positions the floored curve read `0` at.
    ///
    /// Two claims over `1..=STEPPER_CREW`, and each catches a different way of getting this wrong:
    /// **non-zero wherever the crew can damage the quarry at all** (the defect), and **monotone
    /// non-decreasing** (a "fix" that divided by the crew, or one that let the engagement staircase
    /// and the fight cross the wrong way, would break this and not the first).
    #[test]
    fn the_curve_is_non_zero_and_rises_across_the_whole_plateau() {
        /// The stepper the panel that reported this actually shows — a thirteen-worker band.
        const STEPPER_CREW: u32 = 13;

        let mut world = world_hunting(AUROCHS, AUROCHS_BODY);
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let quarry = fauna.quarry_fight_for(AUROCHS);
        // **Can this crew hurt the quarry at all** — the resolver's own gate, per crew, so a genuine
        // zero (no gear that clears `defense`) is never demanded to be non-zero below.
        let can_damage: Vec<bool> = (1..=STEPPER_CREW)
            .map(|workers| {
                fixture_party(&world, AUROCHS_BODY, workers)
                    .crews
                    .iter()
                    .any(|crew| {
                        crate::combat::strike_damage(crew.hunter.attack, quarry.profile.defense)
                            > 0.0
                    })
            })
            .collect();
        assert!(
            can_damage.iter().all(|able| *able),
            "the fully-armed fixture must clear the aurochs' defence at every crew in \
             `1..={STEPPER_CREW}` ({can_damage:?}), or the sweep below is asserting on a fixture \
             that cannot kill"
        );
        drop(fauna);

        let rows = crew_curve(&mut world, &crew_ask(STEPPER_CREW, STRIP_IT_BARE));
        for row in &rows {
            assert!(
                row.animals_likely > 0.0,
                "a crew of {} clears the quarry's defence, so it takes a NON-ZERO number of aurochs \
                 a turn — the whole curve read `0` across this plateau, at every equipment level",
                row.workers
            );
        }
        for pair in rows.windows(2) {
            assert!(
                pair[1].animals_likely >= pair[0].animals_likely,
                "adding a hunter must never lower the take ({} at {} vs {} at {})",
                pair[1].animals_likely,
                pair[1].workers,
                pair[0].animals_likely,
                pair[0].workers
            );
        }
    }

    /// **THE CURVE AND `realized` ARE THE SAME TAKE, AND THE GAP BETWEEN THEM IS STATED.**
    ///
    /// The bug was two shipped surfaces disagreeing on one screen: the work row published a
    /// `SourceYield::realized` of ~`0.84` food while the compose panel, quoting the curve, said
    /// `0.00`. They are computed by genuinely different routes — the curve is one turn's expected
    /// rate at the herd's current stock, `project_realized_hunt` is a forward average over
    /// `forecast_horizon_turns` turns of regrow → take — so they cannot be asserted equal, and this
    /// pins the relationship instead.
    ///
    /// **`realized` runs at or below the curve**, by one unfinished body spread over the horizon
    /// plus [`REALIZED_DRAWDOWN_SLACK`]: it sums the *quantised* kills, so up to one body's damage is
    /// still on the wound ledger when the horizon ends and is never counted. The fixture is
    /// deliberately a fat herd at a near-stable stock, so that residual body is the whole of the
    /// difference — as shipped, `realized` trails by 1.8% against a 2.2% ledger bound.
    #[test]
    fn the_curve_and_realized_agree_on_a_stable_stock() {
        /// The crew the defect was reported at, and the one whose two surfaces disagreed.
        const REPORTED_CREW: u32 = 8;

        let mut world = world_hunting(AUROCHS, AUROCHS_BODY);
        let horizon = world
            .resource::<ExpeditionConfigHandle>()
            .get()
            .hunt
            .forecast_horizon_turns;
        let rows = crew_curve(&mut world, &crew_ask(REPORTED_CREW, STRIP_IT_BARE));
        let published = rows[REPORTED_CREW as usize - 1].animals_likely;

        let fauna = world.resource::<FaunaConfigHandle>().get();
        let herd = herd_of_biomass(AUROCHS, AUROCHS_BODY, FAT_HERD);
        let party = fixture_party(&world, AUROCHS_BODY, REPORTED_CREW);
        let realized = crate::fauna::project_realized_hunt(
            &herd,
            &fauna,
            RESIDENT_CARRY_PER_WORKER,
            &party,
            NEUTRAL_OUTPUT,
            REPORTED_CREW,
            STRIP_IT_BARE,
            horizon,
        );
        // The curve is in animals; `realized` is in food. One conversion, the species' own
        // (`HuntYield::apply`), so the comparison is not two different readings of the roster.
        let curve_food = crate::fauna::herd_hunt_yield(&herd, &fauna)
            .apply(published * AUROCHS_BODY, NEUTRAL_OUTPUT)
            .provisions;

        // **LIVENESS FIRST** — two zeroes would satisfy every bound below, and two zeroes is exactly
        // what the defect published on one of the two sides.
        assert!(
            curve_food > 0.0 && realized.provisions > 0.0,
            "both surfaces must quote a real take ({curve_food} food from the curve, {} from \
             `realized`), or this test cannot see the disagreement it exists for",
            realized.provisions
        );
        assert!(
            realized.provisions <= curve_food + LEDGER_AVERAGE_EPSILON,
            "`realized` ({}) must not exceed the curve ({curve_food}): it sums the quantised kills \
             over a stock the crew is drawing down, so it can only lag",
            realized.provisions
        );
        let shortfall = (curve_food - realized.provisions) / curve_food;
        // **The bound is DERIVED, not tuned.** `project_realized_hunt` sums whole animals, so at
        // most one body's worth of damage is still banked on the quarry when the window closes and
        // is never counted — `1 / (rate × horizon)` of the total. Lengthening the horizon or arming
        // the crew tightens this on its own, which is what keeps it from becoming a number somebody
        // nudges whenever a retune moves the roster.
        let unfinished_body = 1.0 / (published * horizon as f32);
        assert!(
            shortfall <= unfinished_body + REALIZED_DRAWDOWN_SLACK,
            "`realized` ({}) trails the curve ({curve_food}) by {:.1}% — more than the one \
             unfinished body a {horizon}-turn window can leave on the ledger ({:.1}%) plus the \
             fixture's own drawdown, so the two have stopped being the same take",
            realized.provisions,
            shortfall * 100.0,
            unfinished_body * 100.0
        );
    }

    /// **What the herd itself moves under the crew across a `realized` window**, on top of the
    /// unfinished body the horizon leaves on the ledger.
    ///
    /// The fixture is deliberately a fat herd — the run takes a few percent of the standing stock
    /// and logistic regrowth returns most of it — so this is a small allowance rather than the term
    /// that dominates. On a herd the crew genuinely strips, the gap *is* the drawdown and no
    /// tolerance would make the two numbers agree; that is why [`answer_hunt_crew_take`]'s doc states
    /// them as two quantities rather than one.
    const REALIZED_DRAWDOWN_SLACK: f32 = 0.01;

    /// A band with no productivity bonus — the identity on `HuntYield::apply`, so the comparison
    /// above is about the take rather than about a multiplier.
    const NEUTRAL_OUTPUT: f32 = 1.0;

    /// **A GENUINE ZERO SURVIVES.** Bare hands are `attack 1` and a Wild Aurochs is `defense 6`, so
    /// `strike_damage` is *exactly* `0` — no head count and no horizon accumulates that into a kill
    /// (`combat::resolve_fight`'s gate, and `DamageLedger`'s "banking zero forever is still zero").
    ///
    /// The rate must report it as `0`, not as a small number. This is the assertion that stops the
    /// fix from becoming "never publish zero".
    #[test]
    fn a_crew_that_cannot_hurt_the_quarry_still_reads_zero() {
        let mut world = world_hunting_bare_handed(AUROCHS, AUROCHS_BODY);
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let quarry = fauna.quarry_fight_for(AUROCHS);
        // **THE PRECONDITION**: the fixture band really is holding nothing that clears the gate at
        // any crew on the sweep. Without it a stocked band would pass this by taking zero for some
        // *other* reason.
        let bare = BandEquipment::default();
        for workers in 1..=SWEEP_CREW {
            let party = fixture_party_with_wear(&world, &bare, AUROCHS_BODY, workers);
            assert!(
                party.crews.iter().all(|crew| crate::combat::strike_damage(
                    crew.hunter.attack,
                    quarry.profile.defense
                ) <= 0.0),
                "a crew of {workers} from an unstocked band must land NOTHING on a `defense {}` \
                 aurochs, or this fixture is not the incapable one",
                quarry.profile.defense
            );
        }
        drop(fauna);

        let rows = crew_curve(&mut world, &crew_ask(SWEEP_CREW, STRIP_IT_BARE));
        for row in &rows {
            assert_eq!(
                (row.animals_low, row.animals_likely, row.animals_high),
                (0.0, 0.0, 0.0),
                "a crew of {} that cannot clear the quarry's defence takes EXACTLY nothing — a rate \
                 must not smear that into a small number",
                row.workers
            );
        }
    }

    /// The curve refuses on the same tokens the two older verbs do — it resolves the band, the herd
    /// and the kit through the one shared seam, and a floor outside `0..=1` is rejected rather than
    /// clamped.
    /// One refusal case's edit to an otherwise-valid crew-take query — the curve's twin of
    /// [`Perturbation`], named for the same reason: the boxed closure's type is what a `Vec` of
    /// these needs, and spelling it at the binding buries the only thing the table is about.
    type CrewPerturbation = Box<dyn Fn(&mut HuntCrewTakeQuery)>;

    /// **THE BOUND IS A CEILING, NOT A NARROWING** — the largest legal ask is still answered, so the
    /// refusal above cannot be satisfied by a validator that turned the query off.
    #[test]
    fn the_largest_legal_crew_is_still_answered() {
        let mut world = world_hunting(BOAR, BOAR_BODY);
        let ask = crew_ask(MAX_CREW_TAKE_WORKERS, A_FLOOR);
        let curve = crew_curve(&mut world, &ask);
        assert_eq!(
            curve.len(),
            MAX_CREW_TAKE_WORKERS as usize,
            "a crew exactly at the bound is a legal question with a full answer"
        );
    }

    #[test]
    fn the_crew_curve_refuses_on_the_shared_tokens() {
        let mut world = world_hunting(BOAR, BOAR_BODY);
        let cases: Vec<(&str, CrewPerturbation)> = vec![
            (
                query_error::UNKNOWN_HERD,
                Box::new(|ask: &mut HuntCrewTakeQuery| ask.herd_id = "no_such_herd".into()),
            ),
            (
                query_error::UNKNOWN_BAND,
                Box::new(|ask: &mut HuntCrewTakeQuery| ask.band_id = BAND + 1),
            ),
            (
                query_error::UNKNOWN_KIT,
                Box::new(|ask: &mut HuntCrewTakeQuery| ask.kit_id = "no_such_kit".into()),
            ),
            (
                query_error::KIT_WRONG_JOB,
                Box::new(|ask: &mut HuntCrewTakeQuery| ask.kit_id = FORAGE_ONLY_KIT.into()),
            ),
            (
                query_error::INVALID_FLOOR,
                Box::new(|ask: &mut HuntCrewTakeQuery| ask.floor = 1.5),
            ),
            // **The loop bound, which was the one unvalidated field on this ask.** `u32::MAX` is the
            // shape a client bug actually takes, and the curve would have tried to answer it.
            (
                query_error::INVALID_CREW,
                Box::new(|ask: &mut HuntCrewTakeQuery| ask.max_workers = u32::MAX),
            ),
        ];
        for (token, perturb) in cases {
            let mut ask = crew_ask(4, A_FLOOR);
            perturb(&mut ask);
            assert_eq!(
                error_token(&answer_hunt_crew_take(&mut world, &ask)),
                token,
                "the curve must refuse with its own token"
            );
        }
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
                row.animals_taken as f32
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
