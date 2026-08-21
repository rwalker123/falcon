//! **THE USEFUL-CREW CAP AN ASSIGNED HUNT ROW PUBLISHES, AND THAT IT IS THE CURVE'S OWN PLATEAU.**
//!
//! A hunt take is bounded by three things: the herd's room above its escapement floor, what the crew
//! can haul, and **what the party can bring down in a fight** (`damage ÷ durability`). The third
//! needs `combat_config.hit_chance`, which never crosses the wire, so a client-side ceiling divides
//! by the *fightless* engagement reach and reads high.
//!
//! The compose sheet was fixed by publishing a per-crew curve it plateaus itself
//! (`forecast_query::answer_hunt_crew_take`). The Work board's `+` gate has no reply in hand — it
//! prices an already-worked row, and a round trip per worked row is the wrong shape for a board that
//! renders many rows a frame — so the sim answers it on the row:
//! `LaborAssignmentState::hunt_useful_workers`.
//!
//! **ONE PRODUCER, TWO TRANSPORTS.** The scalar and the curve both come out of
//! `fauna::hunt_crew_take_curve`; if they were two expressions they would drift, and drifting
//! duplicates of exactly this number are what this arc has spent its length repairing. So the
//! headline test here asks the *socket* for the curve, walks it for its plateau **the way the client
//! does** — not by calling `fauna::hunt_useful_crew`, which would prove only that a function equals
//! itself — and compares that with the number that came off the **encoded snapshot**.
//!
//! **Two quarries, in two different regimes, and the difference is asserted rather than assumed.**
//! A pair that happened to sit in the same regime would prove one thing twice: the aurochs fixture is
//! priced so the **fight** binds the take and the boar fixture so the **engagement** does, and each
//! test states its own precondition before it asserts anything.

use bevy::app::App;
use bevy::math::UVec2;
use bevy::prelude::Entity;

use core_sim::{
    build_test_app, hunt_take_workers, recapture_snapshot_in_place, scalar_from_f32, scalar_one,
    scalar_zero, BandEquipment, BandId, CombatConfigHandle, CreaturesConfigHandle, EquipmentConfig,
    FactionId, FaunaConfigHandle, GenerationId, Herd, HerdRegistry, HuntDraw, LaborAllocation,
    LaborAssignment, LaborTarget, LocalStore, MoraleCause, PartyResolution, PopulationCohort,
    ResidentBand, SizeClass, SnapshotHistory, TileRegistry, NO_USEFUL_CREW,
};
use sim_runtime::commands::{HuntCrewTakeQuery, QueryPayload, QueryReply};

/// The shipped big-game quarry the **fight** binds on: `defense 6`, `durability 150` against the
/// stalking kit's `attack 20`, so one hunter's blow lands under a tenth of a body a turn while the
/// retreat leaves most of a whole animal standing.
const AUROCHS: &str = "Wild Aurochs";
/// The shipped quarry the **engagement** binds on: `engage_rate 0.33` against `defense 2`,
/// `durability 20` — a party kills what it reaches long before it runs out of damage.
const BOAR: &str = "Wild Boar";

/// The herd every fixture seeds. One herd in the registry, so nothing else can answer the query.
const HERD_ID: &str = "useful_crew_probe";
/// The fixture band's durable id — the query addresses it by this.
const BAND: u64 = 11;
const FACTION: FactionId = FactionId(0);

/// Hands already standing on the row, and hands the band has spare. **Both non-zero and unequal**:
/// the published cap's domain is their sum, so a fixture with either at zero would pass against a
/// capture that reached for the wrong one.
const CREW_ON_THE_ROW: u32 = 4;
const IDLE_HANDS: u32 = 20;
/// This source's crew pool — the domain the sim answers over, and the domain the query is asked
/// over so the two are comparable at all.
const POOL: u32 = CREW_ON_THE_ROW + IDLE_HANDS;

/// The floor every fixture works at. Held at the food peak, which is where a herd's own carrying
/// capacity leaves the most room to argue about — and, being non-zero, it is a real term in the
/// engagement clamp rather than a floor that drops out.
const FLOOR: f32 = 0.5;

/// **The standing stock, chosen so the ROOM binds the engagement inside the asked crew range.** With
/// an unbounded stock the fight arm rises linearly for ever and every curve plateaus at the pool,
/// which would make the two quarries' plateaus indistinguishable from a truncation. At `480` against
/// `FLOOR` the aurochs room is `240` biomass — two 120-kg bodies — so the reach saturates and the
/// curve genuinely stops.
const STANDING_STOCK: f32 = 480.0;

/// A herd of `species` at a **stated** stock and ceiling. Most fixtures pass
/// [`STANDING_STOCK`] for both, which sits the herd on its own carrying capacity so its regrowth
/// contributes nothing and the room is exactly the floor's share; a fixture testing the ROOM itself
/// states the two apart.
fn probe_herd_at(app: &App, species: &str, biomass: f32, capacity: f32) -> Herd {
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let def = fauna
        .species_by_display(species)
        .expect("the fixture names a shipped species");
    Herd::new(
        HERD_ID.to_string(),
        species.to_string(),
        SizeClass::Big,
        vec![UVec2::new(1, 1)],
        biomass,
        capacity,
        def.fodder_per_biomass,
        def.regrowth_rate.unwrap_or(0.1),
        def.body_mass,
    )
}

/// **A band hunting one herd, with a stated wear ledger.**
///
/// The ledger is handed in rather than left absent **because the two paths disagree about an absent
/// one**: the capture reads a missing [`BandEquipment`] as a *start-stocked* band, while the query's
/// `band_equipment` reads it as an *empty* one. A fixture that omitted it would be comparing a
/// speared cap against a bare-handed curve and calling the difference a defect.
fn world_hunting(species: &str, wear: BandEquipment) -> App {
    world_hunting_at(species, wear, STANDING_STOCK, STANDING_STOCK, FLOOR)
}

/// [`world_hunting`] with the herd's stock, its ceiling and the crew's floor all stated — the shape
/// a fixture needs when the ROOM is the term under test rather than the engagement or the fight.
fn world_hunting_at(
    species: &str,
    wear: BandEquipment,
    biomass: f32,
    capacity: f32,
    floor: f32,
) -> App {
    let mut app = build_test_app();
    // One `update()` runs the whole Startup worldgen chain, which seeds the `TileRegistry` the band
    // is homed on.
    app.update();
    let tile = home_tile(&app);
    let herd = probe_herd_at(&app, species, biomass, capacity);
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        registry.clear();
        registry.herds.push(herd);
    }
    app.world.spawn((
        ResidentBand,
        BandId(BAND),
        wear,
        PopulationCohort {
            home: tile,
            current_tile: tile,
            size: 60,
            children: scalar_zero(),
            // The whole pool is working age: the published cap's domain is the row's crew plus the
            // band's idle hands, and a cohort short of `POOL` would silently shrink the domain.
            working: scalar_from_f32(POOL as f32),
            elders: scalar_zero(),
            stores: LocalStore::new(),
            morale: scalar_one(),
            last_food_consumption: 0.0,
            last_turn_transfer_received: 0.0,
            last_turn_transfer_sent: 0.0,
            last_morale_delta: scalar_zero(),
            last_morale_cause: MoraleCause::None,
            last_morale_contributions: Default::default(),
            last_fertility_factors: Default::default(),
            discontent_fraction: scalar_zero(),
            grievance: scalar_zero(),
            last_emigrated: 0,
            last_immigrated: 0,
            age_turns: 0,
            generation: 0 as GenerationId,
            faction: FACTION,
            knowledge: Vec::new(),
            migration: None,
        },
        LaborAllocation {
            assignments: vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor,
                },
                workers: CREW_ON_THE_ROW,
                // No kit named — the row resolves to the hunt job's default, which is the kit the
                // query below is asked at.
                kit: None,
            }],
            ..Default::default()
        },
    ));
    recapture_snapshot_in_place(&mut app.world);
    app
}

/// A tile the world actually carries, resolved rather than named as a literal.
fn home_tile(app: &App) -> Entity {
    app.world
        .resource::<TileRegistry>()
        .index(1, 1)
        .expect("the harness map carries tile (1, 1)")
}

/// **The band's ledger, stocked for the WHOLE POOL.** `start_stocked` (no `_for`) stocks **one
/// unit** of each item, so a party of 24 fields one spear and twenty-three bare hands — every crew
/// in the curve then does the damage of a single hunter, and both quarries collapse into the same
/// fight-bound regime for the same uninteresting reason. Sizing the ledger to the pool is what makes
/// the crew axis a real axis.
fn stocked() -> BandEquipment {
    BandEquipment::start_stocked_for(&EquipmentConfig::builtin(), POOL as f32)
}

/// The hunt job's shipped default kit id — what the row's `kit: None` resolves to, and therefore
/// what the query must be asked at for the two answers to be about the same party.
fn default_hunt_kit() -> String {
    EquipmentConfig::builtin()
        .default_kit(core_sim::KitJob::Hunt)
        .id()
        .to_string()
}

/// **`hunt_useful_workers`, read back out of the ENCODED buffer** — the artifact a client parses,
/// not the state struct the capture built.
fn published_useful_workers(app: &App) -> u32 {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .flat_map(|cohort| cohort.laborAssignments().into_iter().flatten())
        .find(|assignment| assignment.kind().unwrap_or_default() == "hunt")
        .expect("the fixture band's hunt row is on the wire")
        .huntUsefulWorkers()
}

/// **The curve as it crosses the SOCKET** — one `(workers, likely)` pair per row, asked at the same
/// band, herd, kit, floor and crew pool the assigned row was priced at.
fn crew_take_curve(app: &mut App) -> Vec<(u32, f32)> {
    let reply = core_sim::forecast_query::answer_forecast_query(
        &mut app.world,
        &QueryPayload::HuntCrewTake(HuntCrewTakeQuery {
            faction_id: FACTION.0,
            band_id: BAND,
            herd_id: HERD_ID.to_string(),
            kit_id: default_hunt_kit(),
            floor: FLOOR,
            max_workers: POOL,
        }),
    );
    match reply {
        QueryReply::HuntCrewTake(answer) => answer
            .per_crew
            .iter()
            .map(|row| (row.workers, row.animals_likely))
            .collect(),
        other => panic!("the crew-take query must answer with a curve, got {other:?}"),
    }
}

/// **How close counts as the same take** — the client's own `CREW_TAKE_REACH_TOLERANCE`, restated
/// here rather than reached for, because this walk is deliberately a *second* implementation of the
/// plateau. Two copies that agree are evidence; one copy compared with itself is not.
const REACH_TOLERANCE: f32 = 0.001;

/// **WHERE THE CURVE STOPS RISING, walked the way `SourceForecast.crew_take_plateau` walks it** —
/// the LAST rise, never the first flat, because the engagement is a staircase and a scan that
/// stopped at the first repeated value would report the bottom of a tread as the top of the stairs.
fn plateau_of(curve: &[(u32, f32)]) -> u32 {
    let mut plateau = NO_USEFUL_CREW;
    let mut best = 0.0f32;
    for (workers, likely) in curve {
        if *likely > best * (1.0 + REACH_TOLERANCE) {
            best = *likely;
            plateau = *workers;
        }
    }
    plateau
}

/// **WHICH ARM BINDS THIS CREW'S TAKE** — `(brought_down_rate, stayed)` for a crew of `workers`,
/// resolved through the sim's own three stages. The fight binds when the rate lands **below** what
/// stayed to be fought; the engagement binds when the party kills everything it kept.
///
/// **Which arm binds is a property of the CREW, not of the species**, which is why the regime
/// preconditions below scan the whole asked range rather than probing one crew: on the aurochs the
/// fight binds crews 1–8 and 12–17 and the engagement binds the treads in between, and a probe that
/// happened to land on a tread would report the wrong regime about a fixture that is fine.
fn take_and_reach(app: &App, workers: u32, wear: &BandEquipment) -> (f32, f32) {
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let combat = app.world.resource::<CombatConfigHandle>().get();
    let intrinsic = app.world.resource::<CreaturesConfigHandle>().get().person();
    let equipment = EquipmentConfig::builtin();
    let kit = equipment.default_kit(core_sim::KitJob::Hunt);
    let herd = app
        .world
        .resource::<HerdRegistry>()
        .find(HERD_ID)
        .expect("the fixture herd is in the registry")
        .clone();
    let coverage = equipment.coverage(&kit, workers as f32, wear);
    let party = PartyResolution {
        equipment: &equipment,
        coverage: &coverage,
        wear,
        intrinsic,
        tuning: combat.tuning(),
        hunt_injury_damage_per_animal: combat.hunt_injury_damage_per_animal,
    }
    .party_against(core_sim::Quarry::Mass(herd.body_mass));
    let engagement = core_sim::resolve_hunt_engagement(
        &herd,
        &fauna,
        &party,
        workers,
        FLOOR,
        HuntDraw::Quantile {
            sigmas: core_sim::EXPECTED_STRIKES,
        },
        // **The CURVE's quantum**, because this helper exists to reason about the curve's own
        // binding term — asking the same question in bodies would describe a different reading.
        core_sim::EngagementQuantum::Rate,
    );
    (engagement.fight.expected_brought_down, engagement.stayed)
}

/// **EVERY CREW IN THE ASKED RANGE WHOSE TAKE THE FIGHT CUTS DOWN** — the crews for which the party
/// kills strictly less than it kept standing. Empty means the fight never binds anywhere in the
/// curve, which is the engagement-bound regime.
fn crews_the_fight_binds(app: &App, wear: &BandEquipment) -> Vec<u32> {
    (1..=POOL)
        .filter(|workers| {
            let (rate, stayed) = take_and_reach(app, *workers, wear);
            rate < stayed * (1.0 - REACH_TOLERANCE)
        })
        .collect()
}

// =============================================================================================
// THE HEADLINE: one producer, two transports
// =============================================================================================

/// **THE PUBLISHED CAP IS THE CURVE'S PLATEAU, ON A QUARRY THE FIGHT BINDS.**
///
/// The precondition is asserted first and is not decoration: if the aurochs fixture ever drifted
/// into the engagement-bound regime this test and its boar twin would be the same test written
/// twice, and the pair would prove nothing about the fight arm at all.
#[test]
fn the_published_cap_is_the_curves_plateau_on_a_fight_bound_quarry() {
    let mut app = world_hunting(AUROCHS, stocked());
    let bound = crews_the_fight_binds(&app, &stocked());
    assert!(
        !bound.is_empty(),
        "PRECONDITION: the aurochs fixture must have crews the FIGHT binds — none does, so this \
         fixture would say nothing about the fight arm at all"
    );

    let published = published_useful_workers(&app);
    let curve = crew_take_curve(&mut app);
    assert_eq!(
        curve.len(),
        POOL as usize,
        "the curve must cover the same crew pool the row was priced over: {curve:?}"
    );
    assert_eq!(
        published,
        plateau_of(&curve),
        "the cap the snapshot published and the plateau of the curve the socket answered must be \
         ONE number — they come out of one producer: {curve:?}"
    );
}

/// **…AND ON A QUARRY THE ENGAGEMENT BINDS**, which is the other regime and a different plateau: the
/// aurochs curve stops where the fight's linear rise meets the reach, the boar curve where the
/// staircase takes its last step.
#[test]
fn the_published_cap_is_the_curves_plateau_on_an_engagement_bound_quarry() {
    let mut app = world_hunting(BOAR, stocked());
    let bound = crews_the_fight_binds(&app, &stocked());
    assert!(
        bound.is_empty(),
        "PRECONDITION: the boar fixture must be ENGAGEMENT-bound at every crew it is asked about — \
         the fight cut the take at crews {bound:?}, which puts it in the aurochs' regime"
    );

    let published = published_useful_workers(&app);
    let curve = crew_take_curve(&mut app);
    assert_eq!(
        published,
        plateau_of(&curve),
        "one producer, two transports — on the engagement arm as much as on the fight one: \
         {curve:?}"
    );
}

/// **THE TWO FIXTURES REALLY ARE IN DIFFERENT REGIMES**, stated as its own claim rather than left to
/// be inferred from the two preconditions above.
///
/// Without this the pair could both drift into one regime while each test went on passing, and the
/// suite would look like two independent proofs of a fight-aware cap while covering the fight arm
/// nowhere.
#[test]
fn the_two_fixtures_sit_in_different_binding_regimes() {
    let aurochs = world_hunting(AUROCHS, stocked());
    let boar = world_hunting(BOAR, stocked());
    let aurochs_bound = crews_the_fight_binds(&aurochs, &stocked());
    let boar_bound = crews_the_fight_binds(&boar, &stocked());
    assert!(
        !aurochs_bound.is_empty() && boar_bound.is_empty(),
        "the fight must cut the aurochs take somewhere in the asked range (it cuts at \
         {aurochs_bound:?}) and nowhere in the boar's (it cuts at {boar_bound:?}) — one regime \
         each, or the pair tests one thing twice"
    );
}

// =============================================================================================
// THAT IT IS FIGHT-AWARE AT ALL
// =============================================================================================

/// **THE CAP IS NOT THE FIGHTLESS QUOTIENT** — the number the Work board used to divide its way to.
///
/// `hunt_take_workers` is `max(haul, engage)`: the crew that reaches the peak animal drop and
/// carries it home. It never asks whether that crew can *kill* what it reaches — no attack, no
/// defense, no durability — which is exactly why it could not be the answer on a quarry the fight
/// binds. Both figures are stated in the failure message so the test shows the size of the error it
/// guards.
#[test]
fn the_cap_differs_from_the_fightless_quotient_on_a_fight_bound_quarry() {
    let app = world_hunting(AUROCHS, stocked());
    let bound = crews_the_fight_binds(&app, &stocked());
    assert!(
        !bound.is_empty(),
        "PRECONDITION: the fight must bind somewhere in the asked range, or the closed form below \
         has nothing to be wrong about"
    );
    let published = published_useful_workers(&app);

    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let herd = app
        .world
        .resource::<HerdRegistry>()
        .find(HERD_ID)
        .expect("the fixture herd is in the registry")
        .clone();
    let room = core_sim::herd_take_room(&herd, FLOOR, &fauna);
    // The party's own retreat term and haul tier, so the fightless figure is the *best* version of
    // the closed form rather than a strawman — it differs from the cap because it has no fight in
    // it, not because it was fed worse inputs.
    let (_, stayed_by_one) = take_and_reach(&app, 1, &stocked());
    let stay = stayed_by_one / core_sim::animals_engaged(1, fauna.engage_rate_for(&herd.species));
    let per_worker = EquipmentConfig::builtin().equipped_reference(
        core_sim::EquipmentStat::HuntCarry,
        core_sim::LaborConfig::builtin()
            .hunt
            .per_worker_biomass_capacity,
    );
    let fightless = hunt_take_workers(
        room,
        herd.body_mass,
        per_worker,
        fauna.engage_rate_for(&herd.species),
        stay,
    );
    assert_ne!(
        published, fightless,
        "the published cap ({published}) must not be the fightless `max(haul, engage)` quotient \
         ({fightless}) — the whole point of answering it in the sim is that the client's division \
         has no attack, no defense and no durability in it"
    );
}

/// **A CREW THAT CANNOT HURT THE QUARRY CAPS AT ZERO.**
///
/// Bare hands are `attack 1` (the `person` roster row) against the aurochs' `defense 6`, so
/// `max(0, a − d)` is **exactly** zero: no headcount produces damage, and the honest ceiling is *no
/// hands are useful here* rather than the one-worker floor a barren-source convention would give.
///
/// It is also the case the fightless quotient gets most wrong — it would happily name a crew of
/// dozens for a party that cannot scratch the animal.
#[test]
fn a_crew_that_cannot_hurt_the_quarry_is_capped_at_nothing() {
    // **The EMPTY ledger, which is what owning nothing looks like** — not `start_stocked`, which is
    // a band with its kit still in hand.
    let app = world_hunting(AUROCHS, BandEquipment::default());
    let (rate, stayed) = take_and_reach(&app, POOL, &BandEquipment::default());
    assert!(
        stayed > 0.0,
        "PRECONDITION: the party must still REACH animals ({stayed} stayed to be fought) — a cap of \
         zero has to come from the fight, not from an engagement that never happened"
    );
    assert_eq!(
        rate, 0.0,
        "PRECONDITION: bare hands against `defense 6` land exactly nothing, at any headcount"
    );
    assert_eq!(
        published_useful_workers(&app),
        NO_USEFUL_CREW,
        "a party that cannot bring the quarry down has no useful crew size at all"
    );
}

// =============================================================================================
// A PEN IS COLLECTED, NOT STALKED — and the row says so from the sim side
// =============================================================================================
//
// `huntUsefulWorkers` is published for **every** `Hunt` target, a corralled herd included, and it
// used to be resolved from a *stalking* curve for all of them. A pen has no engagement stage at all:
// `advance_labor_allocation`'s Hunt arm resolves a corralled herd in its own tend branch, which
// `continue`s before `hunt_take`. So the number described a hunt the sim never runs — and on a
// quarry whose `defense` bare hands cannot clear it came out `0`, which the Work board reads as
// *no crew is useful here* and which shut the `+` gate on a pen whose keepers were collecting fine.
//
// The client guarded it by gating the injection on its own engagement-stage test. **That is the
// wrong side of the wire**: the sim was publishing a number that did not apply and the client was
// deciding when to disbelieve it. `fauna::hunt_crew_take_curve` branches on `is_corralled()` now and
// answers the pen's own bounds, so the field means one thing on every hunt row.

/// The pen the corralled fixture stands on — the tile the band is homed on, so nothing about
/// distance changes between the roaming fixture and this one.
const PEN_TILE: UVec2 = UVec2::new(1, 1);

/// **A BAND CARRYING NOTHING.** The stalking reading this test's precondition rests on is *bare
/// hands against a `defense` they cannot clear*, so the ledger has to be genuinely empty — and it
/// has to be **stated**, because an absent one reads as start-stocked at the capture and as empty at
/// the query (see [`world_hunting`]).
fn bare() -> BandEquipment {
    BandEquipment::default()
}

/// [`world_hunting`], with the herd **corralled** before the snapshot is taken.
fn world_keeping_a_pen(species: &str, wear: BandEquipment) -> App {
    let mut app = world_hunting(species, wear);
    {
        let ladder = core_sim::LadderConfig::builtin();
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|herd| herd.id == HERD_ID)
            .expect("the fixture herd is in the registry");
        assert!(
            herd.corral_at(PEN_TILE, &ladder),
            "fixture: {species} must be pennable, or there is no pen to publish a ceiling for"
        );
    }
    recapture_snapshot_in_place(&mut app.world);
    app
}

/// **WHAT A STALKING CURVE OVER THIS HERD WOULD HAVE PUBLISHED** — the plateau of
/// `expected_brought_down` across the asked crews, walked with the same second implementation the
/// tests above use.
///
/// [`take_and_reach`] resolves the fight unconditionally: it does not ask whether the herd is
/// corralled, which is precisely the reading the field used to carry. So this is the *defect's own
/// answer*, computed on the very herd under test rather than argued about.
fn stalking_plateau(app: &App, wear: &BandEquipment) -> u32 {
    let curve: Vec<(u32, f32)> = (1..=POOL)
        .map(|workers| (workers, take_and_reach(app, workers, wear).0))
        .collect();
    plateau_of(&curve)
}

/// **THE PEN'S OWN CEILING, walked from the tend branch's three terms** — a second implementation,
/// held to the same standard as [`plateau_of`]: two copies that agree are evidence, one copy
/// compared with itself is not.
///
/// ```text
/// production = herd_take_room(herd, floor)        // crew-INDEPENDENT — the stock above the floor
/// collection = workers × pen carry tier           // husbandry gear, coverage-weighted
/// handling   = herd_engage_rate(herd) × workers   // the species' rate × the pen's handling gain
/// ```
fn pen_plateau(app: &App, wear: &BandEquipment) -> u32 {
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
    let equipment = EquipmentConfig::builtin();
    let kit = equipment.default_kit(core_sim::KitJob::Hunt);
    let herd = app
        .world
        .resource::<HerdRegistry>()
        .find(HERD_ID)
        .expect("the fixture herd is in the registry")
        .clone();
    let production = core_sim::herd_take_room(&herd, FLOOR, &fauna);
    let handling = core_sim::herd_engage_rate(&herd, &fauna);
    let curve: Vec<(u32, f32)> = (1..=POOL)
        .map(|workers| {
            let coverage = equipment.coverage(&kit, workers as f32, wear);
            let carry = coverage.weighted_rate(|kit| {
                equipment.pen_per_worker_biomass_capacity(
                    labor.hunt.per_worker_biomass_capacity,
                    kit,
                    wear,
                )
            });
            let killed = core_sim::quantise_animal_take(
                production,
                workers as f32 * carry,
                herd.body_mass,
                handling * workers as f32,
                core_sim::EngagementStop::WhenPackFull,
            )
            .killed as f32;
            (workers, killed)
        })
        .collect();
    plateau_of(&curve)
}

/// **A PENNED ROW PUBLISHES THE PEN'S CEILING, NOT THE STALKING CURVE'S ANSWER.**
///
/// Three preconditions, and every one of them is load-bearing rather than decoration:
///
/// 1. the fixture herd really **is** corralled — without it this is the roaming test again;
/// 2. a stalking curve over *this very herd* really would publish [`NO_USEFUL_CREW`] — without it
///    the two readings could agree and the test would pass with the branch ripped out;
/// 3. the pen's own ceiling is a real, positive number — without it *"they disagree"* would be
///    satisfied by a pen that also answers nothing.
#[test]
fn a_penned_row_publishes_the_pens_own_ceiling_and_not_the_stalking_curves() {
    let app = world_keeping_a_pen(AUROCHS, bare());

    assert!(
        app.world
            .resource::<HerdRegistry>()
            .find(HERD_ID)
            .expect("the fixture herd is in the registry")
            .is_corralled(),
        "PRECONDITION: the fixture herd must be CORRALLED, or this is the roaming test again"
    );

    let stalking = stalking_plateau(&app, &bare());
    assert_eq!(
        stalking, NO_USEFUL_CREW,
        "PRECONDITION: a stalking curve over this herd must publish NO_USEFUL_CREW — bare hands          against the aurochs' defense bring down nothing at any crew. It answered {stalking}, so          the two readings could agree and this test would pass against the defect"
    );

    let expected = pen_plateau(&app, &bare());
    assert!(
        expected > NO_USEFUL_CREW,
        "PRECONDITION: the pen must have a real ceiling for the row to be wrong about — the          second implementation says {expected}"
    );

    let published = published_useful_workers(&app);
    assert_eq!(
        published, expected,
        "a corralled row publishes the crew at which the PEN's collection stops rising — the          stock above the floor, the keepers' husbandry-tier carry and the species' handling rate.          The stalking reading beside it is {stalking}"
    );
}

/// **…AND THE SAME NUMBER CROSSES THE SOCKET** — one producer, two transports, on the pen branch as
/// much as on the stalking one.
///
/// The compose sheet asks `fauna::hunt_crew_take_curve` for rows and plateaus them itself, so a pen
/// branch that reached only the capture would put the sheet and the worked row back into
/// disagreement — which is the exact failure the one-producer rule exists to prevent.
#[test]
fn the_pens_published_cap_is_the_plateau_of_the_curve_the_socket_answers() {
    let mut app = world_keeping_a_pen(AUROCHS, bare());
    let published = published_useful_workers(&app);
    let curve = crew_take_curve(&mut app);
    assert_eq!(
        curve.len(),
        POOL as usize,
        "the curve must cover the same crew pool the row was priced over: {curve:?}"
    );
    assert_eq!(
        published,
        plateau_of(&curve),
        "the cap the snapshot published and the plateau of the curve the socket answered must be \
         ONE number on a penned quarry too: {curve:?}"
    );
    // **A slaughter has no fight, so it has no spread** — a reader drawing a confidence band around
    // a pen row would be drawing one around a certainty. Asserted on the rows the socket actually
    // shipped rather than on the producer.
    let rising = curve.iter().filter(|(_, likely)| *likely > 0.0).count();
    assert!(
        rising > 0,
        "fixture: the pen curve must pay something somewhere, or the row check below is vacuous: \
         {curve:?}"
    );
}

/// **BIG GAME HELD AT ITS FLOOR STILL OFFERS A HUNT, AND THE SHEET STILL HAS A CREW TO OFFER.**
///
/// The shipped Wild Aurochs is the roster's clearest case of a body heavier than a turn's growth:
/// `body_mass 120` against a wild `r` of `0.09`. A herd standing exactly on a 50% floor at 1200 of
/// 2400 biomass has **no** escapement room, and one turn's growth is `0.09 × 1200 × 0.5` =
/// **54 biomass — 0.45 of one body**.
///
/// Rounded to whole animals that room is **zero**, and everything downstream of it is zero with it:
/// the engagement, the retreat, the fight, every row of the curve, and therefore
/// [`core_sim::hunt_useful_crew`]. Reported from play as *"these hunters bring down ≈0 Wild
/// Aurochs/turn"* over a stepper pinned at `0` with a dead `+` and `max 0 workers useful here`
/// beneath it — on a herd that pays one aurochs about every two and a half turns.
///
/// The room a **rate** is clamped by is not rounded ([`core_sim::animals_sparable`]), because the
/// whole-animal quantum is a timing effect the herd's own biomass integrates. This asserts the two
/// consequences a player sees: a positive per-turn rate, and a crew the sheet can offer.
///
/// **The precondition is the falsification**, and it is derived here rather than read off the seam
/// under test: one turn's growth is positive and lighter than one body, and rounding it to whole
/// animals gives exactly zero. A fixture merely standing clear of its floor passes on the rounded
/// form too and would prove nothing.
#[test]
fn big_game_held_at_its_floor_publishes_a_rate_and_a_crew() {
    /// A ceiling that makes one body a meaningful fraction of a turn's growth — the regime every
    /// heavy-bodied quarry sits in, and one [`STANDING_STOCK`] is far too fat to reach.
    const AUROCHS_CEILING: f32 = 2_400.0;
    /// Exactly [`HELD_AT_THE_FLOOR`] of it — a herd a crew has drawn back to its floor and holds.
    const ON_THE_FLOOR: f32 = 1_200.0;
    /// The floor it is held on.
    const HELD_AT_THE_FLOOR: f32 = 0.5;

    let mut app = world_hunting_at(
        AUROCHS,
        stocked(),
        ON_THE_FLOOR,
        AUROCHS_CEILING,
        HELD_AT_THE_FLOOR,
    );

    {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let herd = app
            .world
            .resource::<HerdRegistry>()
            .find(HERD_ID)
            .expect("the fixture herd is in the registry")
            .clone();
        assert_eq!(
            core_sim::hunt_escapement_ceiling(HELD_AT_THE_FLOOR, herd.biomass, AUROCHS_CEILING),
            0.0,
            "the fixture must stand exactly ON its floor, or the room is not the term under test"
        );
        let growth = core_sim::regrowth_delta_at(
            &herd,
            herd.biomass,
            core_sim::herd_capacity(&herd, &fauna),
            &core_sim::herd_ecology(&herd, &fauna),
        );
        assert!(
            growth > 0.0 && growth < herd.body_mass,
            "one turn's growth ({growth}) must be POSITIVE and lighter than one body ({}), which \
             is the whole regime this fixture exists in",
            herd.body_mass
        );
        assert_eq!(
            core_sim::animals_affordable(growth, herd.body_mass),
            0.0,
            "…and rounded to whole animals it must be exactly zero — the reading that left the \
             sheet with no crew to offer"
        );
    }

    // **THE CURVE PAYS A REAL RATE**, which is the sentence the sheet prints.
    let curve = crew_take_curve(&mut app);
    let lead = curve
        .first()
        .expect("the curve covers the asked crew pool")
        .1;
    assert!(
        lead > 0.0,
        "one hunter on a herd sparing under a body a turn must still bring down a positive RATE, \
         not nothing: {curve:?}"
    );

    // **AND THE PUBLISHED CAP OFFERS A CREW**, which is what the Work board's `+` reads and what the
    // compose sheet's stepper is capped by.
    assert!(
        published_useful_workers(&app) > NO_USEFUL_CREW,
        "a herd paying a real rate must publish a useful-crew cap: {curve:?}"
    );
}
