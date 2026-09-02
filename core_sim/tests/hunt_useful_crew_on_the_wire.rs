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
    ResidentBand, SizeClass, SnapshotHistory, SourcePriority, TileRegistry, NO_USEFUL_CREW,
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
            last_turn_food_transfers: Default::default(),
            last_turn_fodder_transfers: Default::default(),
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
                priority: SourcePriority::default(),
                upkeep_kit: None,
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
/// the LAST rise, never the first flat. The reach is a plain `w × engage_rate` now, so it no longer
/// contributes treads of its own; the room and the fight's own float noise still can, and the last
/// rise is the reading that survives either.
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

/// **THE PARTY THIS BAND FIELDS AGAINST THE FIXTURE HERD** — kit coverage, wear ledger, intrinsic
/// profile and combat tuning composed exactly as `advance_labor_allocation` composes them, so a test
/// that quotes a forecast is quoting the party the turn will resolve.
fn party_of(app: &App, workers: u32, wear: &BandEquipment) -> core_sim::HuntingParty {
    let combat = app.world.resource::<CombatConfigHandle>().get();
    let intrinsic = app.world.resource::<CreaturesConfigHandle>().get().person();
    let equipment = EquipmentConfig::builtin();
    let kit = equipment.default_kit(core_sim::KitJob::Hunt);
    let body_mass = app
        .world
        .resource::<HerdRegistry>()
        .find(HERD_ID)
        .expect("the fixture herd is in the registry")
        .body_mass;
    let coverage = equipment.coverage(&kit, workers as f32, wear);
    PartyResolution {
        equipment: &equipment,
        coverage: &coverage,
        wear,
        intrinsic,
        tuning: combat.tuning(),
        hunt_injury_damage_per_animal: combat.hunt_injury_damage_per_animal,
    }
    .party_against(core_sim::Quarry::Mass(body_mass))
}

/// **WHICH ARM BINDS THIS CREW'S TAKE** — `(brought_down_rate, stayed)` for a crew of `workers`,
/// resolved through the sim's own three stages. The fight binds when the rate lands **below** what
/// stayed to be fought; the engagement binds when the party kills everything it kept.
///
/// **Which arm binds is a property of the CREW, not of the species**, which is why the regime
/// preconditions below scan the whole asked range rather than probing one crew. The reach and the
/// fight are both linear in the crew now, so on an unbounded herd one of them would bind
/// everywhere — what still moves the answer across the range is the **room**, which does not grow
/// with the crew at all, and a probe at one crew size would report the wrong regime about a fixture
/// that is fine.
fn take_and_reach(app: &App, workers: u32, wear: &BandEquipment) -> (f32, f32) {
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let herd = app
        .world
        .resource::<HerdRegistry>()
        .find(HERD_ID)
        .expect("the fixture herd is in the registry")
        .clone();
    let party = party_of(app, workers, wear);
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
/// aurochs curve stops where the fight's linear rise meets the room, the boar curve where the
/// reach's does.
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

/// **THE PUBLISHED ROWS RISE WITH EVERY HUNTER, up to the crew the herd's room stops them at.**
///
/// The reach was `floor(workers × engage_rate).max(1)`, so the shipped Wild Boar's `0.33` published
/// **the same row for crews one through six** — a stepper the player could drag five positions
/// across for no take at all, matching the play report of four hunters feeding a band exactly as
/// well as one. Asserted on the rows that cross the **socket**, because those are what the compose
/// sheet draws; the flat tail above the plateau is the escapement room, which is a fact about the
/// herd rather than a rounding.
#[test]
fn the_published_curve_rises_with_every_hunter_up_to_its_plateau() {
    let mut app = world_hunting(BOAR, stocked());
    let curve = crew_take_curve(&mut app);
    let plateau = plateau_of(&curve);
    assert!(
        plateau > 1,
        "PRECONDITION: the boar curve must climb past its first row ({curve:?}), or there is no \
         rise to check"
    );
    let mut previous = 0.0f32;
    for (workers, likely) in curve.iter().take_while(|(w, _)| *w <= plateau) {
        assert!(
            *likely > previous,
            "the published row for {workers} hunters must be strictly above the row below it \
             ({likely} vs {previous}) — a flat run here is the retired reach floor: {curve:?}"
        );
        previous = *likely;
    }
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
    world_keeping_a_pen_at(species, wear, STANDING_STOCK, STANDING_STOCK, FLOOR)
}

/// [`world_keeping_a_pen`] with the herd's stock, its ceiling and the keepers' floor all stated —
/// the shape a fixture needs when the pen's own ROOM is the term under test.
fn world_keeping_a_pen_at(
    species: &str,
    wear: BandEquipment,
    biomass: f32,
    capacity: f32,
    floor: f32,
) -> App {
    let mut app = world_hunting_at(species, wear, biomass, capacity, floor);
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

/// **THE PEN'S OWN CEILING, walked from the curve's two terms** — a second implementation, held to
/// the same standard as [`plateau_of`]: two copies that agree are evidence, one copy compared with
/// itself is not.
///
/// ```text
/// kill  = resolve_hunt_engagement(..).fight.expected_brought_down   // room, reach, retreat, FIGHT
/// carry = workers × pen carry tier ÷ body_mass                      // the keepers' haul
/// row   = min(kill, max(carry, ONE_WHOLE_ANIMAL))
/// ```
///
/// ⛔ **The first line is the whole of §4.9 item 12b.** It used to be three hand-composed terms —
/// the room, the handling and the carry — with **no retreat and no fight**, because the pen ran
/// neither. It runs both now, through the very seam `systems::hunt_take` runs, so the only term this
/// helper still composes itself is the carry.
fn pen_plateau(app: &App, wear: &BandEquipment) -> u32 {
    let herd = app
        .world
        .resource::<HerdRegistry>()
        .find(HERD_ID)
        .expect("the fixture herd is in the registry")
        .clone();
    let curve: Vec<(u32, f32)> = (1..=POOL)
        .map(|workers| {
            let carry = (workers as f32 * hunt_carry_per_worker(app, workers, wear)
                / herd.body_mass)
                // A keeper who cannot haul a whole beast still walks one out and wastes the rest —
                // a fact about the animal, not a rounding, so it survives the rate.
                .max(ONE_WHOLE_ANIMAL);
            (workers, take_and_reach(app, workers, wear).0.min(carry))
        })
        .collect();
    plateau_of(&curve)
}

/// **A PENNED ROW PUBLISHES THE PEN'S OWN CURVE — AND THE FIGHT GATES IT, EXACTLY AS IT GATES THE
/// RANGE** (`docs/plan_standing_upkeep.md` §4.9 item 12b).
///
/// This test **inverted**. It used to assert that a corralled row must *not* publish the stalking
/// curve's answer — a bare-handed band with a penned aurochs was quoted `NO_USEFUL_CREW` by a
/// stalking reading while its keepers collected perfectly well, so the producer forked on
/// `is_corralled()` into a fightless curve of its own. The **take** resolves the ordinary fight now,
/// so that fork is a lie in the other direction: the row would offer a crew for a take that pays
/// nothing. `fauna::pen_crew_take_curve` is retired and the pen is one `.min()` — the keepers' haul —
/// on the one curve.
///
/// Both halves live here, because either alone passes on a broken sim:
///
/// 1. **bare hands publish [`NO_USEFUL_CREW`]** — `max(0, attack 1 − defense 6)` is nothing at every
///    crew size, so no headcount is useful and the board's `+` is honestly shut;
/// 2. **the identical fixture, kitted, publishes a real cap**, and it is the plateau the second
///    implementation above walks.
#[test]
fn a_penned_rows_cap_is_its_own_curve_and_the_fight_gates_it_too() {
    let bare_pen = world_keeping_a_pen(AUROCHS, bare());
    assert!(
        bare_pen
            .world
            .resource::<HerdRegistry>()
            .find(HERD_ID)
            .expect("the fixture herd is in the registry")
            .is_corralled(),
        "PRECONDITION: the fixture herd must be CORRALLED, or this is the roaming test again"
    );
    assert_eq!(
        published_useful_workers(&bare_pen),
        NO_USEFUL_CREW,
        "a fence does not kill the animal: bare hands against the aurochs' defense are useful at NO          crew size, penned or not"
    );

    let kitted = world_keeping_a_pen(AUROCHS, stocked());
    let expected = pen_plateau(&kitted, &stocked());
    assert!(
        expected > NO_USEFUL_CREW,
        "LIVENESS: the kitted pen must have a real ceiling for the row to be right about — the          second implementation says {expected}"
    );
    assert_eq!(
        published_useful_workers(&kitted),
        expected,
        "a corralled row publishes the crew at which the PEN's own curve stops rising — the room          above the floor, the keepers' reach and calmed retreat, the fight, and their haul"
    );
}

/// **…AND THE SAME NUMBER CROSSES THE SOCKET** — one producer, two transports, on a penned quarry as
/// much as on a stalked one.
///
/// The compose sheet asks `fauna::hunt_crew_take_curve` for rows and plateaus them itself, so a pen
/// reading that reached only the capture would put the sheet and the worked row back into
/// disagreement — which is the exact failure the one-producer rule exists to prevent. Since §4.9
/// item 12b there is no pen *branch* left to disagree: the two rungs are one function and the pen is
/// a carry `.min()`.
#[test]
fn the_pens_published_cap_is_the_plateau_of_the_curve_the_socket_answers() {
    // **Kitted**, because a pen is gated by the fight now: a bare-handed curve over this quarry is
    // zero at every crew and "one number equals one number" would be two zeroes agreeing.
    let mut app = world_keeping_a_pen(AUROCHS, stocked());
    let published = published_useful_workers(&app);
    let curve = crew_take_curve(&mut app);
    assert_eq!(
        curve.len(),
        POOL as usize,
        "the curve must cover the same crew pool the row was priced over: {curve:?}"
    );
    assert!(
        published > NO_USEFUL_CREW,
        "LIVENESS: the kitted pen must publish a real cap, or the equality below is two zeroes:          {curve:?}"
    );
    assert_eq!(
        published,
        plateau_of(&curve),
        "the cap the snapshot published and the plateau of the curve the socket answered must be          ONE number on a penned quarry too: {curve:?}"
    );
    let rising = curve.iter().filter(|(_, likely)| *likely > 0.0).count();
    assert!(
        rising > 0,
        "fixture: the pen curve must pay something somewhere: {curve:?}"
    );
}

/// **A PEN'S THREE READINGS AGREE, AND THEY AGREE IN BOTH DIRECTIONS** — the quote, the payout and
/// the useful-crew cap, all off the **exported** artifact, on one fixture read twice.
///
/// This is the assertion that pins §4.9 item 12b's own failure mode shut. The slice made the pen's
/// *take* resolve a fight; `fauna::pen_crew_take_curve` was the one reading left that did not, so
/// for a moment a bare-handed band with a penned aurochs was **quoted nothing, paid nothing, and
/// told another pair of hands would buy it more**. A forecast and a readout disagreeing about one
/// row is the whole defect class this slice deletes — it must not survive one level down.
///
/// **Both halves are here on purpose.** A containment claim (*"all three say nothing"*) passes
/// perfectly on a pen that is simply broken, so the same fixture `stocked()` has to make all three
/// speak.
#[test]
fn a_pens_quote_its_payout_and_its_useful_crew_all_agree_at_both_kits() {
    // --- BARE: the fight gates all three ------------------------------------------------------
    let mut bare_pen = world_keeping_a_pen(AUROCHS, bare());
    let bare_band = the_band(&mut bare_pen);
    seed_the_row(&mut bare_pen, bare_band, CREW_ON_THE_ROW, &bare(), FLOOR);
    let bare_quote = published_actual_yield(&bare_pen);
    let bare_cap = published_useful_workers(&bare_pen);
    let bare_curve = crew_take_curve(&mut bare_pen);
    let bare_paid = resolve_and_republish(&mut bare_pen);
    assert_eq!(
        (bare_quote, bare_paid),
        (NOTHING, NOTHING),
        "bare hands cannot clear the aurochs' defense at either rung: quoted {bare_quote}, paid          {bare_paid}"
    );
    assert_eq!(
        bare_cap, NO_USEFUL_CREW,
        "…so no crew is useful on it either — a `+` gate that opened here would offer hands for a          take that pays nothing: {bare_curve:?}"
    );
    assert!(
        bare_curve.iter().all(|(_, likely)| *likely <= NOTHING),
        "…and the socket's own rows say the same thing: {bare_curve:?}"
    );

    // --- STOCKED: the identical fixture, and all three speak ----------------------------------
    let mut kitted = world_keeping_a_pen(AUROCHS, stocked());
    let kitted_band = the_band(&mut kitted);
    staff_the_row(&mut kitted, kitted_band, POOL);
    seed_the_row(&mut kitted, kitted_band, POOL, &stocked(), FLOOR);
    let kitted_quote = published_actual_yield(&kitted);
    let kitted_cap = published_useful_workers(&kitted);
    let kitted_curve = crew_take_curve(&mut kitted);
    let kitted_paid = resolve_and_republish(&mut kitted);
    assert!(
        kitted_quote > NOTHING && kitted_paid > NOTHING,
        "LIVENESS: spears turn the same fenced aurochs into a take — quoted {kitted_quote}, paid          {kitted_paid}"
    );
    assert!(
        kitted_cap > NO_USEFUL_CREW,
        "…and the crew cap must open with it: {kitted_curve:?}"
    );
    assert!(
        kitted_curve.iter().any(|(_, likely)| *likely > NOTHING),
        "…and the socket's rows must pay: {kitted_curve:?}"
    );
}

/// **Author `species`' `combat.durability`** — the one lever that moves a party's kill *rate* without
/// touching what it can carry, so the carry arm below becomes the binding one. Mirrors
/// `fauna_husbandry::author_pen_handling_rate`, which authors a rate for the same reason: an arm the
/// shipped roster cannot reach still has to be reachable.
fn author_quarry_durability(app: &mut App, species: &str, durability: f32) {
    let mut config = (*app.world.resource::<FaunaConfigHandle>().get()).clone();
    let key = config
        .species
        .iter()
        .find(|(_, def)| def.display_name == species)
        .map(|(key, _)| key.clone())
        .expect("the fixture names a shipped species");
    config
        .species
        .get_mut(&key)
        .expect("just resolved")
        .combat
        .durability = durability;
    app.world
        .resource_mut::<FaunaConfigHandle>()
        .replace(std::sync::Arc::new(config));
}

/// **A durability the shipped roster does not carry, chosen so the KEEPERS' HAUL binds.**
///
/// A curve row is `min(kill rate, carry)`, and on the **shipped** roster the carry arm is
/// unreachable at every pennable species and both kit tiers: the largest per-worker kill is the
/// aurochs' `120 × (20 − 6) ÷ 150 = 11.2` biomass a turn, against a bare-tier pen carry of `12` and
/// an equipped one of `40`. Measured across all seven `husbandry_ceiling: "pen"` rows — nothing
/// comes within a tenth of it.
///
/// At `10` the same aurochs kills `120 × 14 ÷ 10 = 168` biomass a worker-turn, so a lone keeper's
/// `40` is what stops it. **`10` is not a balance opinion** — it is the smallest round value that
/// clears the `< 42` the inequality asks for.
const CARRY_BOUND_DURABILITY: f32 = 10.0;

/// **THE KEEPERS' HAUL IS A REAL BOUND ON THE PEN'S CURVE** — the one term that survived §4.9 item
/// 12b as the pen's own, and the only thing left that makes a penned row's curve differ from a
/// stalking one.
///
/// The room, the reach, the retreat and the fight are all shared with the range now
/// (`fauna::resolve_hunt_engagement`); what a pen adds is *"and then somebody has to carry it
/// home"* **as a term of the kill rate**, where a wild party's carry limit is expressed downstream
/// through the waste path instead. **The RATE is the band's own** — `hunt_carry`, the same number a
/// stalking party hauls at (issue #543); what the `is_corralled()` predicate decides is *where the
/// bound is applied*, never *which rate applies*. **The shipped roster cannot reach
/// it** — see [`CARRY_BOUND_DURABILITY`] — so this fixture authors a quarry that can, exactly as
/// `fauna_husbandry::a_fractional_pen_handling_rate_collects_whole_animals` authors a handling rate
/// for the arm above it.
///
/// Asserted as a **crew cap that moves**: with the haul in the way the curve keeps rising for
/// several more keepers than the kill rate alone would, so the published cap is strictly larger than
/// the crew at which the kill rate itself plateaus. Both numbers are read from the socket's own rows
/// rather than composed here.
#[test]
fn a_pens_curve_is_bounded_by_what_its_keepers_can_carry_home() {
    let mut app = world_keeping_a_pen(AUROCHS, stocked());
    author_quarry_durability(&mut app, AUROCHS, CARRY_BOUND_DURABILITY);
    recapture_snapshot_in_place(&mut app.world);

    // **The kill rate alone**, off `resolve_hunt_engagement` — the curve with the haul taken out.
    let kill_only: Vec<(u32, f32)> = (1..=POOL)
        .map(|workers| (workers, take_and_reach(&app, workers, &stocked()).0))
        .collect();
    let kill_plateau = plateau_of(&kill_only);

    let curve = crew_take_curve(&mut app);
    let published = published_useful_workers(&app);
    assert_eq!(
        published,
        plateau_of(&curve),
        "one producer, two transports: {curve:?}"
    );
    assert!(
        published > kill_plateau,
        "the haul must keep the curve rising past the crew the KILL rate plateaus at ({kill_plateau}),          or the carry arm is not reaching this fixture at all — published {published}, curve {curve:?}"
    );
    // **LIVENESS** — the rows really pay, so the cap above is a cap on a take rather than on zero.
    assert!(
        curve.iter().any(|(_, likely)| *likely > NOTHING),
        "the authored pen must still collect: {curve:?}"
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

/// **THE SMALLEST TAKE A CREW THAT TAKES ANYTHING PUTS ON THE GROUND** — one animal. The sim's own
/// `ONE_WHOLE_ANIMAL`, restated here because [`pen_plateau`] is a second implementation and a second
/// implementation that imported the first's constants would be the first.
const ONE_WHOLE_ANIMAL: f32 = 1.0;

/// **A PEN SPARING LESS THAN A BODY A TURN STILL OFFERS A CREW.**
///
/// The stalking rows were un-floored when a Wild Aurochs held at its floor published a curve of
/// zeroes (`big_game_held_at_its_floor_publishes_a_rate_and_a_crew`); **the pen path was missed**,
/// and it is the same quantum, the same species and the same sentence — `quantise_animal_take`
/// returns `killed = 0` for any room under one body, so every crew read `0`,
/// [`core_sim::hunt_useful_crew`] answered [`NO_USEFUL_CREW`], and the Work board's `+` shut on a pen
/// whose keepers collect one beast about every two and a half turns.
///
/// **The shipped pen fixture cannot see this**: it stands a herd on `STANDING_STOCK` at its own
/// ceiling, where the room is hundreds of biomass and the floor never bites. So this one is
/// deliberately **thin** — a stock at its floor, whose one turn of penned regrowth is lighter than
/// one aurochs — and the precondition below derives that from the herd rather than asserting it of
/// the seam under test.
#[test]
fn a_thin_pen_publishes_a_rate_and_a_crew() {
    /// The pen's ceiling, and the stock held exactly on the floor below it — sized so one turn of
    /// the **penned** regrowth is a fraction of a 120-kg body.
    const PEN_CEILING: f32 = 1_000.0;
    const ON_THE_FLOOR: f32 = PEN_CEILING * HELD_AT_THE_FLOOR;
    /// The floor the keepers hold it at.
    const HELD_AT_THE_FLOOR: f32 = 0.5;

    let mut app = world_keeping_a_pen_at(
        AUROCHS,
        stocked(),
        ON_THE_FLOOR,
        PEN_CEILING,
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
        assert!(
            herd.is_corralled(),
            "PRECONDITION: the fixture herd must be CORRALLED, or this is the stalking test"
        );
        // **The room the take will find** — one Logistics regrowth on, which is the turn a curve is
        // about — and it must be positive and lighter than one body, or the flooring this test
        // exists for could not have shown.
        let room = core_sim::herd_take_room(
            &core_sim::next_turns_quarry(&herd, &fauna),
            HELD_AT_THE_FLOOR,
            &fauna,
        );
        assert!(
            room > 0.0 && room < herd.body_mass,
            "PRECONDITION: the pen's room ({room}) must be POSITIVE and lighter than one body ({}) \
             — that is the whole regime this fixture exists in",
            herd.body_mass
        );
        assert_eq!(
            core_sim::animals_affordable(room, herd.body_mass),
            0.0,
            "…and rounded to whole animals it must be exactly zero — the reading that shut the + \
             gate on a working pen"
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
        "one keeper on a pen sparing under a body a turn must still collect a positive RATE, not \
         nothing: {curve:?}"
    );

    // **AND THE PUBLISHED CAP OFFERS A CREW**, which is what the Work board's `+` reads.
    assert!(
        published_useful_workers(&app) > NO_USEFUL_CREW,
        "a pen paying a real rate must publish a useful-crew cap: {curve:?}"
    );
}

// =============================================================================================
// ...AND A PENNED ROW IS QUOTED WHAT IT COLLECTS
// =============================================================================================
//
// The crew cap above is one of two numbers a hunt row publishes about a pen; the other is the
// **yield** — `actualYield`, the assign-time seed the compose sheet prints as *"Expected yield"*.
//
// The quote and the payout disagreed on a pen, and `docs/plan_standing_upkeep.md` §4.9 item 12b
// closed the gap **from the take's side**. The quote had always run three stages; it was the *tend
// branch* that ran none of them — it `continue`d before `systems::hunt_take` and collected with a
// formula of its own — so a bare-handed band with a penned **Wild Aurochs** (`defense 6`) was quoted
// `0` and then butchered it anyway. Exempting the quote to match would have made the ladder a mode
// switch: taming and penning would buy nothing at the kill, and containment would substitute for
// weapons.
//
// **So the pen takes through `hunt_take` like every other rung**, and what penning buys is the first
// two stages only — the reach (`husbandry.pen_engage_gain`) and the retreat
// (`husbandry.pen_wariness`). The fight is the species' own `defense` against the party's `attack`
// at every rung: **no weapons, no beef**, and a pen is *reliable*, not *safe*.

/// **The keeper crews the quote sweep walks**, on the fenced [`BOAR`]. Sized so the two bounds a pen
/// has always had bind somewhere in the sweep — two keepers seat fewer bodies than the floor leaves
/// sparable (the carry arm), twenty are stopped by the room — and both are asserted below rather
/// than assumed.
///
/// ⛔ **It starts at TWO, and that is the fight rather than a rounder number.** Since §4.9 item 12b a
/// pen resolves the ordinary fight, and one speared keeper lands `attack 20 − defense 2 = 18` damage
/// against the boar's `durability 20` — nine tenths of a body, which is a **wait turn**. The wound
/// ledger banks it and the next turn finishes the animal, so a lone keeper is a real and correct
/// state; it is simply not one a single-turn `quote == payout` harness can read, exactly as the
/// aurochs is not one at any crew this fixture can staff.
const KEEPER_CREWS: [u32; 3] = [2, 4, 20];

/// **How close two provisions readings count as the same take** — the `Scalar` grid the larder
/// quantises onto with room for the different multiplication orders on either side of the
/// comparison. Orders of magnitude below one provision, so it cannot absorb a whole animal.
const YIELD_EPSILON: f32 = 1e-4;

/// **A source paying nothing at all** — what a bare-handed row quoted for a pen, and what the wild
/// row beside it honestly still quotes.
const NOTHING: f32 = 0.0;

/// **The carry tier this crew works at, PENNED OR WILD** — the sled's, coverage-weighted, which is
/// the rate `advance_labor_allocation` caps a herd row's collection by and therefore the rate the
/// seed has to be priced at (`server::seed_source_yield`).
///
/// **ONE HELPER, because the sim has one rate** (issue #543): what a worker can carry is a fact
/// about the people and their gear, blind to whether the animal is penned or wild. A second helper
/// here would let this file pass while the sim's two arms had drifted.
fn hunt_carry_per_worker(app: &App, workers: u32, wear: &BandEquipment) -> f32 {
    let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
    let equipment = EquipmentConfig::builtin();
    let kit = equipment.default_kit(core_sim::KitJob::Hunt);
    equipment
        .coverage(&kit, workers as f32, wear)
        .weighted_rate(|kit| {
            equipment.hunt_per_worker_biomass_capacity(
                labor.hunt.per_worker_biomass_capacity,
                kit,
                wear,
            )
        })
}

/// **Put `keepers` on the row** — the fixture staffs [`CREW_ON_THE_ROW`], and the sweep needs the
/// crew to be the variable.
fn staff_the_row(app: &mut App, band: Entity, keepers: u32) {
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the fixture band carries its allocation")
        .assignments[0]
        .workers = keepers;
}

/// **The fixture band's entity, resolved by its durable [`BAND`] id** — worldgen seeds bands of its
/// own, so *"the first `ResidentBand`"* finds one of those and every seed written onto it lands on a
/// row nobody reads.
fn the_band(app: &mut App) -> Entity {
    let mut query = app.world.query::<(Entity, &BandId)>();
    query
        .iter(&app.world)
        .find(|(_, id)| id.0 == BAND)
        .map(|(entity, _)| entity)
        .expect("the fixture band is in the world")
}

/// **Seed the row from its pre-commit forecast, exactly as `server::seed_source_yield` does** — the
/// same band-wide carry tier, the same party, the same output multiplier, through the same
/// `hunt_source_yield_preview` entry point. A hand-rolled quote here would prove only that this file
/// agrees with itself.
///
/// **NO `is_corralled()` FORK, because the seed no longer has one** (issue #543): the seed and the
/// resolved row must stay exactly-equal, so a branch here that the sim does not make is precisely
/// the drift this helper exists to rule out.
fn seed_the_row(app: &mut App, band: Entity, keepers: u32, wear: &BandEquipment, floor: f32) {
    let party = party_of(app, keepers, wear);
    let per_worker = hunt_carry_per_worker(app, keepers, wear);
    let seeded = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
        let combat = app.world.resource::<CombatConfigHandle>().get();
        let wellbeing = app
            .world
            .resource::<core_sim::WellbeingConfigHandle>()
            .get();
        let cohort = app
            .world
            .get::<PopulationCohort>(band)
            .expect("the fixture band carries its cohort");
        let output_mult = core_sim::output_multiplier(cohort, &wellbeing).to_f32();
        let registry = app.world.resource::<HerdRegistry>();
        let herd = registry
            .find(HERD_ID)
            .expect("the fixture herd is in the registry");
        core_sim::hunt_source_yield_preview(
            herd,
            &fauna,
            per_worker,
            &party,
            output_mult,
            keepers,
            floor,
            labor.yield_average_horizon_turns,
            labor.arrivals_horizon_turns,
            combat.forecast_range_sigmas,
        )
    };
    let target = LaborTarget::Hunt {
        fauna_id: HERD_ID.to_string(),
        floor,
    };
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the fixture band carries its allocation")
        .set_source_yield(&target, seeded);
    recapture_snapshot_in_place(&mut app.world);
}

/// **`actualYield`, read back out of the ENCODED buffer** — the artifact a client parses, exactly as
/// [`published_useful_workers`] reads the crew cap beside it.
fn published_actual_yield(app: &App) -> f32 {
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
        .actualYield()
}

/// **THE PAYOUT, off the same published row.** One Population stage on the fixture, then the wire is
/// re-read: a resolved row reports the take the turn actually banked.
///
/// **No Logistics stage runs, and the fixture is what makes that honest**: every herd here stands on
/// its own carrying capacity, so the regrowth a quote is priced across (`fauna::next_turns_quarry`)
/// moves nothing. [`the_quote_prices_the_state_the_turn_will_find`] asserts exactly that rather than
/// leaving it as a reader's assumption.
fn resolve_and_republish(app: &mut App) -> f32 {
    use bevy::ecs::system::RunSystemOnce;
    app.world
        .run_system_once(core_sim::advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);
    published_actual_yield(app)
}

/// **THE FIXTURE STANDS ON ITS OWN CEILING**, so the Logistics regrowth between a quote and the take
/// it prices is exactly zero — which is what lets every test in this section resolve the Population
/// stage alone and still compare two readings of *one* turn.
#[test]
fn the_quote_prices_the_state_the_turn_will_find() {
    let app = world_keeping_a_pen(AUROCHS, bare());
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let herd = app
        .world
        .resource::<HerdRegistry>()
        .find(HERD_ID)
        .expect("the fixture herd is in the registry")
        .clone();
    let next = core_sim::next_turns_quarry(&herd, &fauna);
    assert_eq!(
        (next.biomass, next.growth_this_turn()),
        (herd.biomass, 0.0),
        "the fixture herd must sit at its own carrying capacity, or a quote and the take beside it \
         describe two different turns"
    );
}

/// **A BARE-HANDED BAND WITH A PENNED HEAVY ANIMAL TAKES NOTHING, AND IS QUOTED NOTHING** — the
/// fight gates a pen exactly as it gates the range (§4.9 item 12b).
///
/// Asserted as an **identity** on the published row rather than as an epsilon: a fightless party's
/// take is `0`, not a small number. Bare hands swing the `person` row's `attack 1` against the
/// aurochs' `defense 6`, and `max(0, 1 − 6)` is nothing at any crew size — the precondition scans
/// the whole pool rather than probing one crew.
///
/// **The liveness half rides beside it, on the identical fixture**: the same band, the same fenced
/// aurochs, `stocked()` — quoted a real take and paid one. Without it the two zeroes above would be
/// satisfied by a broken pen, a lapsed assignment or a herd that never got penned at all.
#[test]
fn a_bare_handed_pen_is_quoted_nothing_and_paid_nothing() {
    let mut app = world_keeping_a_pen(AUROCHS, bare());
    let band = the_band(&mut app);

    let fightless: Vec<u32> = (1..=POOL)
        .filter(|workers| take_and_reach(&app, *workers, &bare()).0 <= NOTHING)
        .collect();
    assert_eq!(
        fightless.len(),
        POOL as usize,
        "PRECONDITION: bare hands against the aurochs' defense must bring down nothing at EVERY \
         crew size, or this test's zeroes are about something else. Crews that land nothing: \
         {fightless:?}"
    );

    seed_the_row(&mut app, band, CREW_ON_THE_ROW, &bare(), FLOOR);
    let quoted = published_actual_yield(&app);
    let paid = resolve_and_republish(&mut app);
    assert_eq!(
        (quoted, paid),
        (NOTHING, NOTHING),
        "a fence does not kill the animal: bare hands on a penned aurochs must be quoted nothing \
         AND paid nothing, got quoted {quoted}, paid {paid}"
    );

    // **LIVENESS — the same pen, kitted.** The whole pool, because the aurochs' `durability 150`
    // banks wounds for several turns against a smaller crew and this harness resolves one
    // (`an_equipped_wild_row_is_quoted_the_take_it_pays` documents the same regime on the range).
    let mut kitted = world_keeping_a_pen(AUROCHS, stocked());
    let kitted_band = the_band(&mut kitted);
    staff_the_row(&mut kitted, kitted_band, POOL);
    seed_the_row(&mut kitted, kitted_band, POOL, &stocked(), FLOOR);
    let kitted_quote = published_actual_yield(&kitted);
    let kitted_paid = resolve_and_republish(&mut kitted);
    assert!(
        kitted_quote > NOTHING && kitted_paid > NOTHING,
        "liveness: spears turn the same fenced aurochs into a take — quoted {kitted_quote}, paid \
         {kitted_paid}. If either is zero the identities above are the fixture, not the fight"
    );
}

/// **THE QUOTE IS THE PAYOUT ACROSS THE KEEPER RANGE.**
///
/// A pen has **four** stages that can bind now, not two: the **room** above the assignment's floor,
/// the keepers' **reach** (`husbandry.pen_engage_gain`), the **retreat** (`husbandry.pen_wariness`),
/// the **fight**, and what the keepers can **carry** home. The sweep asserts it saw the two the old
/// exemption left — room and carry — so it cannot degenerate into testing one arm four times.
///
/// **With the RETREAT held at its identity**, exactly as the wild sweep beside it is
/// (`an_equipped_wild_row_is_quoted_the_take_it_pays`): a quote reads the retreat's *expectation*
/// over a seed it cannot draw, so at a live wariness `quote == payout` is the wrong invariant to
/// assert. Held to zero the distribution is degenerate and the equality is exact.
///
/// **On the BOAR and with a kit**, because the fight is a real term now: the aurochs' `durability
/// 150` banks wounds for several turns against any crew this harness can staff, and a bare hand
/// clears no pennable species' `defense` at all.
#[test]
fn a_pens_quote_is_its_payout_at_every_keeper_count() {
    let mut saw_room_bound = false;
    let mut saw_carry_bound = false;
    for keepers in KEEPER_CREWS {
        let mut app = world_keeping_a_pen(BOAR, stocked());
        app.world
            .resource_mut::<FaunaConfigHandle>()
            .hold_wariness_at_zero();
        let band = the_band(&mut app);
        staff_the_row(&mut app, band, keepers);

        // **Which arm binds, composed from the take's own terms** — the whole bodies the floor
        // leaves sparable against the whole bodies the crew's load seats.
        let (room_animals, carry_animals) = {
            let fauna = app.world.resource::<FaunaConfigHandle>().get();
            let herd = app
                .world
                .resource::<HerdRegistry>()
                .find(HERD_ID)
                .expect("the fixture herd is in the registry")
                .clone();
            let room = core_sim::animals_affordable(
                core_sim::herd_take_room(&herd, FLOOR, &fauna),
                herd.body_mass,
            );
            let carry = (keepers as f32 * hunt_carry_per_worker(&app, keepers, &stocked())
                / herd.body_mass)
                .max(ONE_WHOLE_ANIMAL);
            (room, carry)
        };
        if carry_animals < room_animals {
            saw_carry_bound = true;
        } else {
            saw_room_bound = true;
        }

        seed_the_row(&mut app, band, keepers, &stocked(), FLOOR);
        let quoted = published_actual_yield(&app);
        let paid = resolve_and_republish(&mut app);
        assert!(
            paid > NOTHING,
            "{keepers} keepers must collect something, or this row asserts nothing"
        );
        assert!(
            (quoted - paid).abs() <= YIELD_EPSILON,
            "{keepers} keepers: quoted {quoted}, paid {paid} (room {room_animals} animals, carry \
             {carry_animals})"
        );
    }
    assert!(
        saw_room_bound && saw_carry_bound,
        "the sweep must exercise both of the bounds the pen already had: room-bound={saw_room_bound} \
         carry-bound={saw_carry_bound}"
    );
}

/// **A WILD HERD IS UNCHANGED — the fight still gates it.**
///
/// The pen fork must not have loosened the range: the same bare-handed band on the same herd, not
/// penned, must still be quoted **exactly nothing**, and must still pay exactly nothing, because it
/// cannot hurt an aurochs. Asserted as `0.0` on the published row rather than as an epsilon: a
/// fightless party's take is an identity, not a small number.
///
/// The **liveness** half rides beside it: the same herd penned pays a real take with the same bare
/// hands, so the zeroes above are the fight and not the fixture.
#[test]
fn a_wild_row_is_still_gated_by_the_fight() {
    let mut app = world_hunting(AUROCHS, bare());
    let band = the_band(&mut app);
    assert!(
        !app.world
            .resource::<HerdRegistry>()
            .find(HERD_ID)
            .expect("the fixture herd is in the registry")
            .is_corralled(),
        "PRECONDITION: this fixture's herd must be WILD, or it is the pen test again"
    );

    seed_the_row(&mut app, band, CREW_ON_THE_ROW, &bare(), FLOOR);
    let quoted = published_actual_yield(&app);
    let paid = resolve_and_republish(&mut app);
    assert_eq!(
        (quoted, paid),
        (NOTHING, NOTHING),
        "a bare-handed party cannot bring down an aurochs, and both the quote and the take must \
         still say so: quoted {quoted}, paid {paid}"
    );

    // **Liveness** — the identical herd, fenced AND the band kitted, is quoted a real take. The
    // rider used to keep the band bare-handed, because a pen ran no fight; since §4.9 item 12b it
    // does, so bare hands are quoted nothing at *either* rung and the spears are what has to change
    // for this to prove anything.
    let mut penned = world_keeping_a_pen(AUROCHS, stocked());
    let penned_band = the_band(&mut penned);
    staff_the_row(&mut penned, penned_band, POOL);
    seed_the_row(&mut penned, penned_band, POOL, &stocked(), FLOOR);
    assert!(
        published_actual_yield(&penned) > NOTHING,
        "liveness: a kitted crew must be quoted a real take on this herd, or the zeroes above are \
         the fixture rather than the fight"
    );
}

/// **The spear id the strike quantum is charged against** — named so the wear assertion reads as the
/// claim rather than as a string.
const SPEARS: &str = "spears";

/// **What the band has left of `item`**, off its live [`BandEquipment`] ledger.
fn condition_left(app: &App, band: Entity, item: &str) -> f32 {
    app.world
        .get::<BandEquipment>(band)
        .expect("the fixture band carries its ledger")
        .remaining(item, &EquipmentConfig::builtin())
}

/// **The band's working-age head count**, which is where a hunt's dead come out of
/// (`PopulationCohort::apply_combat_casualties`).
fn working_hands(app: &App, band: Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the fixture band carries its cohort")
        .working
        .to_f32()
}

/// **A CONTAINED BULL STILL GORES, AND THE SPEAR THAT KILLED IT STILL BLUNTS** — the band's side of a
/// pen's take (`docs/plan_standing_upkeep.md` §4.9 item 12b), which is the half that makes a pen
/// **reliable rather than safe**.
///
/// The tend branch used to `continue` before any of this: no fight, so no casualties to apply, no
/// blow to charge a weapon for, and a wear comment that said outright *"a penned beast is
/// slaughtered, not stalked, so there is no spear to blunt"*. Both are false now, and both are
/// asserted here on the shipped surfaces rather than on the resolver:
///
/// 1. the **spears** lose condition over the turn — the `Strike` quantum, charged from the pen;
/// 2. the **band** loses working-age hands — the aurochs' `attack 4 × ferocity 0.7` answering back.
///
/// **Liveness rides on the take itself**: the same turn must actually pay, or a pen that resolved
/// nothing at all would satisfy neither claim for the wrong reason.
#[test]
fn a_pens_take_blunts_the_spear_and_costs_the_keepers() {
    let mut app = world_keeping_a_pen(AUROCHS, stocked());
    let band = the_band(&mut app);
    staff_the_row(&mut app, band, POOL);
    seed_the_row(&mut app, band, POOL, &stocked(), FLOOR);

    let spears_before = condition_left(&app, band, SPEARS);
    let hands_before = working_hands(&app, band);
    assert!(
        spears_before > NOTHING && hands_before > NOTHING,
        "PRECONDITION: the fixture band must hold spears ({spears_before}) and hands \
         ({hands_before}) to spend"
    );

    let paid = resolve_and_republish(&mut app);
    assert!(
        paid > NOTHING,
        "liveness: the keepers must actually take an animal, or neither cost below is a cost of \
         anything"
    );

    assert!(
        condition_left(&app, band, SPEARS) < spears_before,
        "the keepers SWUNG: the spears must have lost condition over the pen's turn ({} against \
         {spears_before})",
        condition_left(&app, band, SPEARS)
    );
    assert!(
        working_hands(&app, band) < hands_before,
        "…and the bull answered: the band must be down working-age hands ({} against \
         {hands_before})",
        working_hands(&app, band)
    );
}

/// **AND AN EQUIPPED WILD ROW STILL PAYS WHAT IT IS QUOTED** — the range's own `forecast == actual`,
/// restated on the fixture this file already carries, so the pen fork cannot have been bought with a
/// wild regression.
///
/// **On the BOAR, because the aurochs pays a kitted crew nothing in one turn** — `durability 150`
/// against a party this size banks wounds for several turns and then lands a body
/// (`docs/plan_hunt_through_combat.md` §4.2), which is correct and is a distribution this one-turn
/// harness cannot show. The engagement-bound quarry kills what it reaches, so its row has a payout to
/// be equal to.
///
/// **And with the RETREAT held at its identity**, because a wild row's quote is the take's
/// *expectation* over a seed a forecast cannot draw (`docs/plan_hunt_through_combat.md` §6.4): at the
/// shipped `wariness 0.25` a crew that keeps `0.99` animals in expectation and `1` in the draw is a
/// whole boar apart, and asserting an equality there would be asserting the wrong invariant. Held to
/// zero the distribution is degenerate and the quote is the take, bit-for-bit — the same
/// neutralisation `hunt_forecast_range.rs` documents at length.
#[test]
fn an_equipped_wild_row_is_quoted_the_take_it_pays() {
    let mut app = world_hunting(BOAR, stocked());
    app.world
        .resource_mut::<FaunaConfigHandle>()
        .hold_wariness_at_zero();
    let band = the_band(&mut app);
    seed_the_row(&mut app, band, CREW_ON_THE_ROW, &stocked(), FLOOR);
    let quoted = published_actual_yield(&app);
    let paid = resolve_and_republish(&mut app);
    assert!(
        paid > NOTHING,
        "liveness: a kitted party must take something from this herd ({quoted} quoted)"
    );
    assert!(
        (quoted - paid).abs() <= YIELD_EPSILON,
        "a wild row must still be quoted the take it pays: quoted {quoted}, paid {paid}"
    );
}

/// **THE STEADY HEADLINE AND THE ARRIVAL SCHEDULE ARE THE SAME PEN AS THE QUOTE** — the two forward
/// projections beside `actualYield` on the row.
///
/// `fauna::project_realized_hunt` fills `realizedYield` (*"this source steadily pays…"*) and
/// `fauna::project_arrivals_hunt` fills `arrivalSchedule` (*"…and the next lands in N turns"*). Both
/// simulate the take forward, so both must simulate the take the tend branch runs — which since
/// §4.9 item 12b is `systems::hunt_take`, engagement, retreat and fight included.
///
/// **This test inverts.** It used to assert that a bare-handed pen projects a steady income and a
/// delivery, because the pen ran no fight and the projections did; now the pen fights, and a party
/// that cannot clear the quarry's `defense` projects **nothing** — which is exactly consistent with
/// the `NOTHING` it is quoted and paid one test above. The two halves below are that consistency and
/// its liveness: the same fixture, kitted, projects both.
#[test]
fn a_bare_handed_pen_projects_no_income_and_a_kitted_one_projects_both() {
    let mut app = world_keeping_a_pen(AUROCHS, bare());
    let band = the_band(&mut app);
    assert!(
        take_and_reach(&app, CREW_ON_THE_ROW, &bare()).0 <= NOTHING,
        "PRECONDITION: these bare hands must bring down nothing, or neither projection is gated by \
         a fight"
    );

    seed_the_row(&mut app, band, CREW_ON_THE_ROW, &bare(), FLOOR);
    let (realized, arrivals) = published_projections(&app);

    assert_eq!(
        realized, NOTHING,
        "a pen its keepers cannot kill from projects no steady income — an identity, not a small \
         number"
    );
    assert!(
        arrivals.iter().all(|slot| *slot <= NOTHING),
        "…and nothing lands in the whole horizon: {arrivals:?}"
    );

    // **LIVENESS** — spears, and the same two projections come alive on the same fenced herd.
    let mut kitted = world_keeping_a_pen(AUROCHS, stocked());
    let kitted_band = the_band(&mut kitted);
    staff_the_row(&mut kitted, kitted_band, POOL);
    seed_the_row(&mut kitted, kitted_band, POOL, &stocked(), FLOOR);
    let (kitted_realized, kitted_arrivals) = published_projections(&kitted);
    assert!(
        kitted_realized > NOTHING,
        "liveness: a pen a kitted crew CAN kill from must publish a steady income, not \
         {kitted_realized}"
    );
    assert!(
        kitted_arrivals.iter().any(|slot| *slot > NOTHING),
        "…and at least one turn in the horizon must deliver food: {kitted_arrivals:?}"
    );
}

/// **`realizedYield` and `arrivalSchedule`, off the ENCODED buffer** — the pair
/// [`published_actual_yield`] leaves behind.
fn published_projections(app: &App) -> (f32, Vec<f32>) {
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
    let row = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .flat_map(|cohort| cohort.laborAssignments().into_iter().flatten())
        .find(|assignment| assignment.kind().unwrap_or_default() == "hunt")
        .expect("the fixture band's hunt row is on the wire");
    (
        row.realizedYield(),
        row.arrivalSchedule()
            .map(|slots| slots.iter().collect())
            .unwrap_or_default(),
    )
}
