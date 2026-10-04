//! **The breeding-population ceiling** (issue #688, `docs/plan_civilization_steps.md` §"The
//! mechanism: an isolated people cannot grow past its lines").
//!
//! A breeding population — a supply-network component, or a band alone in none — ceilings at
//! `|union of its members' founding lines| × lineage.people_per_line`. Births stop there; nobody is
//! removed by it. Driven through the **real** `simulate_population` (and, for membership, the real
//! `balance_supply_networks`) on a generated world, so every number is the demographic model's own.
//!
//! Each fixture removes the band's `LaborAllocation`, which reads as *no flow telemetry* and so a
//! neutral `trend` factor, and keeps its larder deep, so the band is well fed and fertility is the
//! only thing these tests move.

mod faction_support;

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::{Entity, With};

use core_sim::{
    balance_supply_networks, breeding_ceiling, recapture_snapshot_in_place, scalar_from_f32,
    simulate_population, split_band_from_parent, BandId, ConnectionKey, ConnectionLedger,
    ConnectionsConfig, DemographicsConfigHandle, FoundingLines, LaborAllocation, PopulationCohort,
    ResidentBand, Scalar, SettleConfig, Sighting, SnapshotHistory, SupplyNetworkMembership, Tile,
    FULL_TIE,
};
use faction_support::{one_faction_world, HOME};

/// A larder many lifetimes deep, restocked before every turn, so no fixture band ever goes hungry.
const DEEP_LARDER: f32 = 1.0e6;
/// The keeping class the deep larder is stocked in — the longest-keeping one, as worldgen seeds.
const LARDER_CLASS: &str = "dry";
/// The most turns a fixture runs before it must have reached its ceiling.
const MAX_TURNS_TO_CEILING: usize = 300;
/// Turns a fixture keeps running after reaching its ceiling, to show it holds there.
const HOLD_TURNS: usize = 40;
/// How close to its ceiling a population must come to have "reached" it, in people. At the ceiling
/// births only replace the turn's deaths, which leave it a turn's deaths short — a couple of people.
const REACHED_WITHIN_PEOPLE: f32 = 6.0;
/// How far below the ceiling a held population may sag on any one turn, in people — a bad turn's
/// deaths, which the next turn's births replace.
const HOLD_SLACK_PEOPLE: f32 = 12.0;
/// How far below its ceiling a fixture is seated, in people — close enough to reach it quickly,
/// far enough that the growth is real.
const SEAT_BELOW_CEILING: f32 = 20.0;
/// A band seated above its ceiling, by this many people.
const SEAT_ABOVE_CEILING: f32 = 48.0;
/// A band seated just under its ceiling, so this turn's births are larger than its room and only
/// part of them fit.
const SEAT_JUST_UNDER_CEILING: f32 = 1.0;
/// Lines enough that the ceiling is nowhere in sight — the "uncapped" control arm.
const UNCAPPED_LINES: u16 = 100;

/// Fission floors relaxed to the minimum, so a split here is about the lines and not about the
/// founding economy this file does not test.
const SETTLE: SettleConfig = SettleConfig {
    min_founding_workers: 1,
    parent_min_workers: 0,
};
/// The head-count the home band is seated at before it splits.
const PRE_SPLIT_PEOPLE: f32 = 100.0;
/// Workers the split takes — a quarter of the pre-split band's, so the splinter walks off with a
/// couple of the eight lines and the parent keeps the rest.
const SPLIT_WORKERS: u32 = 15;

/// The shipped `L` and `K`.
fn shipped_lineage(app: &App) -> (u16, std::num::NonZeroU16) {
    let lineage = app
        .world
        .resource::<DemographicsConfigHandle>()
        .get()
        .lineage
        .clone();
    (lineage.founding_lines.get(), lineage.people_per_line)
}

fn ceiling_for(app: &App, lines: usize) -> u32 {
    breeding_ceiling(lines, shipped_lineage(app).1)
}

/// The home faction's first band.
fn home_band(app: &mut App) -> (Entity, BandId) {
    app.world
        .query_filtered::<(Entity, &BandId, &PopulationCohort), With<ResidentBand>>()
        .iter(&app.world)
        .filter(|(_, _, cohort)| cohort.faction == HOME)
        .min_by_key(|(_, id, _)| **id)
        .map(|(entity, id, _)| (entity, *id))
        .expect("worldgen spawns a home band")
}

fn cohort(app: &App, band: Entity) -> &PopulationCohort {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the band has a cohort")
}

fn total(app: &App, band: Entity) -> f32 {
    cohort(app, band).total().to_f32()
}

fn scalar_from_u32(value: u32) -> Scalar {
    Scalar::from_u32(value)
}

/// Reshape a band into `people` in the shipped opening age shape — the settled equilibrium of the
/// shipped rates, so the fixture is not spending its first turns re-balancing brackets.
fn seat_people(app: &mut App, band: Entity, people: f32) {
    let dist = app
        .world
        .resource::<DemographicsConfigHandle>()
        .get()
        .initial_distribution
        .clone();
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(band)
        .expect("the band has a cohort");
    cohort.children = scalar_from_f32(people * dist.children);
    cohort.working = scalar_from_f32(people * dist.working);
    cohort.elders = scalar_from_f32(people * dist.elders);
    cohort.sync_size();
}

fn seat_lines(app: &mut App, band: Entity, id: BandId, lines: u16) {
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the band has a cohort")
        .founding_lines = FoundingLines::founded(id, lines);
}

/// No flow telemetry → a neutral `trend`, so a fed band breeds on hunger and reserve alone.
fn neutral_trend(app: &mut App, band: Entity) {
    app.world.entity_mut(band).remove::<LaborAllocation>();
}

fn stock_larder(app: &mut App, band: Entity) {
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the band has a cohort")
        .stores
        .reset_food(LARDER_CLASS, scalar_from_f32(DEEP_LARDER));
}

/// One demographic turn for `bands`, each restocked first.
fn turn(app: &mut App, bands: &[Entity]) {
    for &band in bands {
        stock_larder(app, band);
    }
    app.world.run_system_once(simulate_population);
}

/// Run turns until `bands` together reach `ceiling` (asserting they never exceed it), then
/// `HOLD_TURNS` more, asserting they hold near it. Returns the turn the ceiling was reached on.
fn grow_to_and_hold(app: &mut App, bands: &[Entity], ceiling: u32) -> usize {
    let combined = |app: &App| {
        bands
            .iter()
            .fold(Scalar::zero(), |sum, &band| sum + cohort(app, band).total())
    };
    let limit = scalar_from_u32(ceiling);
    let reached_at = (0..MAX_TURNS_TO_CEILING)
        .find(|_| {
            turn(app, bands);
            let now: Scalar = combined(app);
            assert!(now <= limit, "{now:?} people above a ceiling of {ceiling}");
            now.to_f32() >= ceiling as f32 - REACHED_WITHIN_PEOPLE
        })
        .unwrap_or_else(|| {
            panic!(
                "never reached a ceiling of {ceiling} in {MAX_TURNS_TO_CEILING} turns (at {})",
                combined(app).to_f32()
            )
        });
    for _ in 0..HOLD_TURNS {
        turn(app, bands);
        let now: Scalar = combined(app);
        assert!(now <= limit, "{now:?} people above a ceiling of {ceiling}");
        assert!(
            now.to_f32() >= ceiling as f32 - HOLD_SLACK_PEOPLE,
            "a well-fed population at its ceiling sagged to {now:?}"
        );
    }
    reached_at
}

/// A seeded full-strength tie both ways between two bands, so the supply pass links them.
fn tie(app: &mut App, a: Entity, b: Entity) {
    const SEEDED_ON_TURN: u64 = 0;
    let cfg = ConnectionsConfig::default();
    let contacts_to_full = (FULL_TIE.to_f32() / cfg.strength.gain_per_contact).ceil() as u32;
    for (observer, subject) in [(a, b), (b, a)] {
        let key = ConnectionKey::new(
            *app.world.get::<BandId>(observer).unwrap(),
            *app.world.get::<BandId>(subject).unwrap(),
        );
        let subject_tile = cohort(app, subject).current_tile;
        let position = app.world.get::<Tile>(subject_tile).unwrap().position;
        let mut ledger = app.world.resource_mut::<ConnectionLedger>();
        for _ in 0..contacts_to_full {
            ledger.record_contact(
                key,
                &Sighting::new(position, SEEDED_ON_TURN, ""),
                SEEDED_ON_TURN,
                &cfg,
            );
        }
    }
}

fn network_of(app: &App, band: Entity) -> u32 {
    app.world
        .resource::<SupplyNetworkMembership>()
        .network_of(band)
}

/// The home band seated at `PRE_SPLIT_PEOPLE` with the shipped `L`, split, with both halves fed
/// and on a neutral trend. `linked` seeds the tie and runs the supply pass, so the two share a
/// network; otherwise the pass runs with no tie and each is its own breeding population.
fn split_pair(app: &mut App, linked: bool) -> (Entity, Entity) {
    let (parent, id) = home_band(app);
    let (lines, _) = shipped_lineage(app);
    seat_people(app, parent, PRE_SPLIT_PEOPLE);
    seat_lines(app, parent, id, lines);
    let split = split_band_from_parent(&mut app.world, parent, SPLIT_WORKERS, &SETTLE)
        .expect("the fixture band can spare the splinter");
    let child = app
        .world
        .query::<(Entity, &BandId)>()
        .iter(&app.world)
        .find(|(_, band)| **band == split.band)
        .map(|(entity, _)| entity)
        .expect("the splinter is in the world");
    if linked {
        tie(app, parent, child);
    }
    app.world.run_system_once(balance_supply_networks);
    for band in [parent, child] {
        neutral_trend(app, band);
    }
    (parent, child)
}

/// **A well-fed isolated band grows to its `L × K` and holds there**, never once above it.
#[test]
fn an_isolated_band_grows_to_its_lines_times_k_and_holds() {
    let mut app = one_faction_world();
    let (band, id) = home_band(&mut app);
    let (lines, _) = shipped_lineage(&app);
    let ceiling = ceiling_for(&app, usize::from(lines));
    seat_lines(&mut app, band, id, lines);
    seat_people(&mut app, band, ceiling as f32 - SEAT_BELOW_CEILING);
    neutral_trend(&mut app, band);
    assert_eq!(network_of(&app, band), 0, "a lone band is in no network");

    grow_to_and_hold(&mut app, &[band], ceiling);

    let reading = cohort(&app, band).last_breeding;
    assert_eq!(reading.ceiling, ceiling);
    assert_eq!(
        reading.headcount,
        cohort(&app, band).total().to_u32(),
        "a lone band's breeding population is itself"
    );
    assert!(
        cohort(&app, band).last_fertility_factors.ceiling < Scalar::one(),
        "liveness: at its ceiling the band's births were withheld"
    );
}

/// **Two bands of one people in one network share ONE ceiling** — the union of their partitioned
/// lines × K — so one may grow past its own lines' share while the pair stays under the union's.
#[test]
fn two_bands_in_one_network_share_one_ceiling() {
    let mut app = one_faction_world();
    let (parent, child) = split_pair(&mut app, true);
    let network = network_of(&app, parent);
    assert!(network != 0, "the tied pair forms a network");
    assert_eq!(network_of(&app, child), network);

    let (lines, _) = shipped_lineage(&app);
    let child_lines = cohort(&app, child).founding_lines.len();
    let parent_lines = cohort(&app, parent).founding_lines.len();
    assert_eq!(
        child_lines + parent_lines,
        usize::from(lines),
        "the split partitioned the lines"
    );
    let shared = ceiling_for(&app, usize::from(lines));
    let child_alone = ceiling_for(&app, child_lines);

    // The splinter above what its own lines would carry, the pair well under the union's.
    seat_people(&mut app, child, child_alone as f32 + SEAT_BELOW_CEILING);
    seat_people(&mut app, parent, SEAT_BELOW_CEILING);
    turn(&mut app, &[parent, child]);
    let factors = cohort(&app, child).last_fertility_factors;
    assert_eq!(
        factors.ceiling,
        Scalar::one(),
        "the splinter breeds on the network's room, not on its own lines"
    );
    assert!(
        factors.multiplier() > Scalar::zero(),
        "liveness: the fed splinter bore children above its own lines' share"
    );

    grow_to_and_hold(&mut app, &[parent, child], shared);
    for band in [parent, child] {
        assert_eq!(cohort(&app, band).last_breeding.ceiling, shared);
    }
    assert_eq!(
        cohort(&app, parent).last_breeding.headcount,
        cohort(&app, child).last_breeding.headcount,
        "both members publish the one breeding population"
    );
}

/// **A splinter off the network caps at its own lines × K** — it walked off with them, and the
/// parent's ceiling fell by the same lines.
#[test]
fn a_splinter_off_the_network_caps_at_its_own_lines() {
    let mut app = one_faction_world();
    let (parent, child) = split_pair(&mut app, false);
    assert_eq!(network_of(&app, parent), 0, "no tie, no network");
    assert_eq!(network_of(&app, child), 0);

    let child_cap = ceiling_for(&app, cohort(&app, child).founding_lines.len());
    let parent_cap = ceiling_for(&app, cohort(&app, parent).founding_lines.len());
    let (lines, _) = shipped_lineage(&app);
    assert!(child_cap < ceiling_for(&app, usize::from(lines)));
    // The splinter seated just under its own (small) cap, so it reaches it fast.
    seat_people(&mut app, child, child_cap as f32 - SEAT_BELOW_CEILING / 2.0);

    for _ in 0..MAX_TURNS_TO_CEILING {
        turn(&mut app, &[parent, child]);
        assert!(cohort(&app, child).total() <= scalar_from_u32(child_cap));
        assert!(cohort(&app, parent).total() <= scalar_from_u32(parent_cap));
    }
    assert!(
        total(&app, child) >= child_cap as f32 - REACHED_WITHIN_PEOPLE,
        "liveness: the splinter grew to its own cap"
    );
    assert_eq!(cohort(&app, child).last_breeding.ceiling, child_cap);
    assert_eq!(cohort(&app, parent).last_breeding.ceiling, parent_cap);
}

/// **The ceiling never kills.** A band hand-set above its ceiling bears nobody, and every bracket
/// but the newborns resolves exactly as the same turn with the ceiling nowhere in sight.
#[test]
fn a_band_above_its_ceiling_bears_nobody_and_loses_no_one_to_it() {
    let mut app = one_faction_world();
    let (band, id) = home_band(&mut app);
    let (lines, _) = shipped_lineage(&app);
    let ceiling = ceiling_for(&app, usize::from(lines));
    seat_people(&mut app, band, ceiling as f32 + SEAT_ABOVE_CEILING);
    neutral_trend(&mut app, band);
    stock_larder(&mut app, band);
    let before = cohort(&app, band).clone();

    // The capped turn.
    seat_lines(&mut app, band, id, lines);
    turn(&mut app, &[band]);
    let capped = cohort(&app, band).clone();

    // The same turn, the same band, with lines enough that the ceiling is nowhere in sight.
    *app.world.get_mut::<PopulationCohort>(band).unwrap() = before.clone();
    seat_lines(&mut app, band, id, UNCAPPED_LINES);
    turn(&mut app, &[band]);
    let open = cohort(&app, band).clone();

    assert_eq!(capped.last_fertility_factors.ceiling, Scalar::zero());
    assert_eq!(open.last_fertility_factors.ceiling, Scalar::one());
    assert_eq!(
        capped.working, open.working,
        "no worker lost to the ceiling"
    );
    assert_eq!(capped.elders, open.elders, "no elder lost to the ceiling");
    let open_births = (open.children - capped.children).to_f32();
    assert!(
        open_births > 0.0,
        "liveness: the open arm bore children the capped arm did not"
    );
    assert!(
        capped.total() < before.total(),
        "above its ceiling, a band only shrinks — by its ordinary deaths"
    );
}

/// **The three wire fields read off the ENCODED envelope**: the fourth fertility factor, the
/// breeding population and its ceiling — on a turn where only part of the would-be births fit, so
/// the factor is neither of its two easy values.
#[test]
fn the_breeding_fields_reach_the_encoded_envelope() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let mut app = one_faction_world();
    let (band, id) = home_band(&mut app);
    let (lines, _) = shipped_lineage(&app);
    let ceiling = ceiling_for(&app, usize::from(lines));
    seat_lines(&mut app, band, id, lines);
    seat_people(&mut app, band, ceiling as f32 - SEAT_JUST_UNDER_CEILING);
    neutral_trend(&mut app, band);
    turn(&mut app, &[band]);

    let factor = cohort(&app, band).last_fertility_factors.ceiling;
    assert!(
        factor > Scalar::zero() && factor < Scalar::one(),
        "liveness: part of the births fit ({factor:?})"
    );
    let reading = cohort(&app, band).last_breeding;

    recapture_snapshot_in_place(&mut app.world);
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
        .find(|row| row.entity() == band.to_bits())
        .expect("the band is on the wire");
    assert_eq!(row.fertilityCeiling(), factor.raw());
    assert_eq!(row.breedingCeiling(), ceiling);
    assert_eq!(row.breedingPopulation(), reading.headcount);
    assert_eq!(
        row.breedingPopulation(),
        row.size(),
        "a lone band is its own population"
    );
}

/// The shipped `lineage.free_breeding_at`, in people.
fn free_breeding_at(app: &App) -> u32 {
    app.world
        .resource::<DemographicsConfigHandle>()
        .get()
        .lineage
        .free_breeding_at
        .get()
}

/// The fewest lines whose `× K` reaches `free_breeding_at` — the smallest people the ceiling no
/// longer binds.
fn lines_at_free_breeding(app: &App) -> u16 {
    let k = u32::from(shipped_lineage(app).1.get());
    u16::try_from(free_breeding_at(app).div_ceil(k)).expect("a line count fits a u16")
}

/// The band's three breeding fields, read off the ENCODED envelope.
fn published_breeding(app: &mut App, band: Entity) -> (i64, u32, u32) {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    recapture_snapshot_in_place(&mut app.world);
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
        .find(|row| row.entity() == band.to_bits())
        .expect("the band is on the wire");
    (
        row.fertilityCeiling(),
        row.breedingPopulation(),
        row.breedingCeiling(),
    )
}

/// **At `free_breeding_at` the inbreeding ceiling lifts.** A population whose `lines × K` reaches
/// it grows straight past that figure with a factor of `1`, and publishes a ceiling of `0` — "no
/// inbreeding ceiling" — on the encoded envelope.
#[test]
fn a_population_at_the_free_breeding_size_grows_past_its_lines() {
    let mut app = one_faction_world();
    let (band, id) = home_band(&mut app);
    let lines = lines_at_free_breeding(&app);
    let would_be_ceiling = ceiling_for(&app, usize::from(lines));
    assert!(would_be_ceiling >= free_breeding_at(&app));
    seat_lines(&mut app, band, id, lines);
    seat_people(&mut app, band, would_be_ceiling as f32 + SEAT_ABOVE_CEILING);
    neutral_trend(&mut app, band);
    let before = total(&app, band);

    for _ in 0..HOLD_TURNS {
        turn(&mut app, &[band]);
        assert_eq!(
            cohort(&app, band).last_fertility_factors.ceiling,
            Scalar::one(),
            "a lifted ceiling withholds no birth"
        );
    }
    assert!(
        total(&app, band) > before,
        "liveness: the band grew past what its lines × K would have held ({before} → {})",
        total(&app, band)
    );

    let (factor, population, ceiling) = published_breeding(&mut app, band);
    assert_eq!(factor, Scalar::one().raw());
    assert_eq!(ceiling, core_sim::NO_INBREEDING_CEILING);
    assert_eq!(population, cohort(&app, band).last_breeding.headcount);
}

/// **One line short of `free_breeding_at`, the ceiling still binds**: a population above its
/// `lines × K` bears nobody and publishes that ceiling.
#[test]
fn a_population_just_under_the_free_breeding_size_still_caps() {
    let mut app = one_faction_world();
    let (band, id) = home_band(&mut app);
    let lines = lines_at_free_breeding(&app) - 1;
    let ceiling = ceiling_for(&app, usize::from(lines));
    assert!(ceiling < free_breeding_at(&app));
    seat_lines(&mut app, band, id, lines);
    seat_people(&mut app, band, ceiling as f32 + SEAT_ABOVE_CEILING);
    neutral_trend(&mut app, band);
    turn(&mut app, &[band]);

    assert_eq!(
        cohort(&app, band).last_fertility_factors.ceiling,
        Scalar::zero()
    );
    let (factor, _, published) = published_breeding(&mut app, band);
    assert_eq!(factor, Scalar::zero().raw());
    assert_eq!(published, ceiling);
}

/// One line — a band that cannot partition it, so its split hands both halves a copy.
const ONE_LINE: u16 = 1;
/// A band large enough to split off a crew, in people.
const SPLITTABLE_PEOPLE: f32 = 100.0;
/// Workers a splitting fixture sends off — about half a `SPLITTABLE_PEOPLE` band's workers, so
/// every split of a band holding several lines walks off with some and leaves it fewer.
const HALF_THE_WORKERS: u32 = 30;
/// Where a one-line half is re-seated after its split, in people — under its shared ceiling, so the
/// test watches it grow into the share.
const SMALL_HALF_PEOPLE: f32 = 6.0;
/// Turns the shared-line pair runs, enough to fill both halves' shares.
const SHARED_LINE_TURNS: usize = 120;

/// Split `workers` off `band` and return the splinter. Its trend is left as the split made it, so
/// it can split in turn; callers neutralize every band once the splitting is done.
fn split_off(app: &mut App, band: Entity, workers: u32) -> Entity {
    let split = split_band_from_parent(&mut app.world, band, workers, &SETTLE)
        .expect("the fixture band can spare the splinter");
    app.world
        .query::<(Entity, &BandId)>()
        .iter(&app.world)
        .find(|(_, id)| **id == split.band)
        .map(|(entity, _)| entity)
        .expect("the splinter is in the world")
}

/// The home band reduced to one line and split, both halves re-seated small. Both hold a copy of
/// the same line.
fn one_line_split(app: &mut App) -> (Entity, Entity) {
    let (parent, id) = home_band(app);
    seat_lines(app, parent, id, ONE_LINE);
    seat_people(app, parent, SPLITTABLE_PEOPLE);
    let child = split_off(app, parent, HALF_THE_WORKERS);
    assert_eq!(
        cohort(app, parent).founding_lines,
        cohort(app, child).founding_lines,
        "a one-line split copies its line"
    );
    for band in [parent, child] {
        seat_people(app, band, SMALL_HALF_PEOPLE);
        neutral_trend(app, band);
    }
    (parent, child)
}

/// **A line two separate groups hold splits its K between them.** Two unlinked halves of a
/// one-line band each publish `floor(K / 2)`, and together they never exceed one line's `K`.
#[test]
fn two_unlinked_halves_of_one_line_share_its_k() {
    let mut app = one_faction_world();
    let (parent, child) = one_line_split(&mut app);
    app.world.run_system_once(balance_supply_networks);
    assert_eq!(network_of(&app, parent), 0);
    assert_eq!(network_of(&app, child), 0);

    let k = u32::from(shipped_lineage(&app).1.get());
    let one_line = Scalar::from_u32(k);
    for _ in 0..SHARED_LINE_TURNS {
        turn(&mut app, &[parent, child]);
        let combined = cohort(&app, parent).total() + cohort(&app, child).total();
        assert!(
            combined <= one_line,
            "{combined:?} people on one line's K of {k}"
        );
    }
    for band in [parent, child] {
        assert_eq!(
            cohort(&app, band).last_breeding.ceiling,
            k / 2,
            "each half holds half the line"
        );
        assert!(
            total(&app, band) >= (k / 2) as f32 - REACHED_WITHIN_PEOPLE,
            "liveness: each half grew into its share"
        );
    }
}

/// **Relinked, the line has one holder again**: the pair is one breeding population with the
/// line's whole `K`.
#[test]
fn relinked_halves_of_one_line_hold_its_whole_k() {
    let mut app = one_faction_world();
    let (parent, child) = one_line_split(&mut app);
    tie(&mut app, parent, child);
    app.world.run_system_once(balance_supply_networks);
    assert!(network_of(&app, parent) != 0);
    assert_eq!(network_of(&app, parent), network_of(&app, child));

    turn(&mut app, &[parent, child]);
    let k = u32::from(shipped_lineage(&app).1.get());
    for band in [parent, child] {
        assert_eq!(cohort(&app, band).last_breeding.ceiling, k);
    }
}

/// **Splitting never raises a people's total ceiling.** One isolated people split again and again
/// — down to one-line bands, and a one-line band split once more so its line is copied — with
/// every band unlinked: the ceilings its bands publish sum to at most `L × K`.
#[test]
fn scattering_a_people_never_raises_its_total_ceiling() {
    let mut app = one_faction_world();
    let (founder, id) = home_band(&mut app);
    let (lines, _) = shipped_lineage(&app);
    seat_lines(&mut app, founder, id, lines);
    seat_people(&mut app, founder, SPLITTABLE_PEOPLE);

    let mut bands = vec![founder];
    let mut source = founder;
    while cohort(&app, source).founding_lines.len() > usize::from(ONE_LINE) {
        seat_people(&mut app, source, SPLITTABLE_PEOPLE);
        bands.push(split_off(&mut app, source, HALF_THE_WORKERS));
        source = *bands.last().unwrap();
    }
    // `source` holds one line now; split it again so the line is held twice.
    seat_people(&mut app, source, SPLITTABLE_PEOPLE);
    let copy = split_off(&mut app, source, HALF_THE_WORKERS);
    assert_eq!(
        cohort(&app, copy).founding_lines,
        cohort(&app, source).founding_lines
    );
    bands.push(copy);
    for &band in &bands {
        neutral_trend(&mut app, band);
    }

    app.world.run_system_once(balance_supply_networks);
    assert!(bands.iter().all(|&band| network_of(&app, band) == 0));
    turn(&mut app, &bands);

    let total_ceiling: u32 = bands
        .iter()
        .map(|&band| cohort(&app, band).last_breeding.ceiling)
        .sum();
    let people_ceiling = ceiling_for(&app, usize::from(lines));
    assert!(
        total_ceiling <= people_ceiling,
        "{} scattered bands publish {total_ceiling} against the people's {people_ceiling}",
        bands.len()
    );
}
