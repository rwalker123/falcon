//! **Founding lines on a band** (issue #687, `docs/plan_civilization_steps.md` §"The mechanism: a
//! breeding population cannot grow past its lines").
//!
//! A band carries the SET of unrelated families it descends from. This file pins the state alone —
//! where lines are minted, how a split divides them, that the checkpoint carries them and that the
//! count reaches the encoded envelope. Nothing here reads a ceiling; that is a later slice.

mod faction_support;

use std::collections::BTreeSet;

use bevy::app::App;
use bevy::prelude::{Entity, With};

use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    recapture_snapshot_in_place, scalar_from_f32, split_band_from_parent, BandId,
    DemographicsConfigHandle, FoundingLines, LineId, PopulationCohort, ResidentBand, SettleConfig,
    SnapshotHistory,
};

/// Fission floors relaxed to the minimum, so a split here is about the lines and not about the
/// founding economy this file does not test.
const SETTLE: SettleConfig = SettleConfig {
    min_founding_workers: 1,
    parent_min_workers: 0,
};

/// The fixture band's brackets: a 30-person band with a whole number of workers, so every share
/// below is an exact fraction of its working-age people.
const CHILDREN: f32 = 10.0;
const WORKERS: u32 = 18;
const ELDERS: f32 = 2.0;

/// The fixture's `L`. Stated rather than read from the shipped config, because the split arithmetic
/// below is pinned against it; `starting_band_is_founded_with_l_lines_of_its_own` reads the shipped
/// lever.
const LINES: u16 = 8;

/// **A five-person splinter off thirty**: three of eighteen workers is a share of one sixth, which
/// takes three workers and one sixth of the twelve dependents — five people. `8 × 1/6 ≈ 1.3`.
const FIVE_PEOPLE_WORKERS: u32 = 3;
const FIVE_PEOPLE_LINES: usize = 1;

/// All but one worker — `8 × 17/18 ≈ 7.6`, which would take every line without the clamp.
const ALL_BUT_ONE_WORKER: u32 = WORKERS - 1;

/// One worker — `8 × 1/18 ≈ 0.4`, which would take no line without the clamp.
const ONE_WORKER: u32 = 1;

/// The single line a band holds when it cannot be partitioned.
const ONE_LINE: u16 = 1;

/// The fewest lines either half of any split may hold.
const AT_LEAST_ONE: usize = core_sim::MIN_BAND_LINES as usize;

fn world() -> App {
    faction_support::two_faction_world()
}

fn lines_of(app: &App, entity: Entity) -> FoundingLines {
    app.world
        .get::<PopulationCohort>(entity)
        .expect("the band has a cohort")
        .founding_lines
        .clone()
}

fn line_set(lines: &FoundingLines) -> BTreeSet<LineId> {
    lines.iter().copied().collect()
}

/// The first resident band of faction 0, with its durable id.
fn a_home_band(app: &mut App) -> (Entity, BandId) {
    let mut query = app
        .world
        .query_filtered::<(Entity, &BandId, &PopulationCohort), With<ResidentBand>>();
    query
        .iter(&app.world)
        .filter(|(_, _, cohort)| cohort.faction == faction_support::HOME)
        .map(|(entity, id, _)| (entity, *id))
        .min_by_key(|(_, id)| *id)
        .expect("worldgen spawns a home band")
}

/// Reshape a band into the 30-person fixture holding `lines` founding lines of its own.
fn seat_fixture_band(app: &mut App, entity: Entity, id: BandId, lines: u16) {
    let mut cohort = app
        .world
        .get_mut::<PopulationCohort>(entity)
        .expect("the band has a cohort");
    cohort.children = scalar_from_f32(CHILDREN);
    cohort.working = scalar_from_f32(WORKERS as f32);
    cohort.elders = scalar_from_f32(ELDERS);
    cohort.sync_size();
    cohort.founding_lines = FoundingLines::founded(id, lines);
}

/// The entity that now carries `band`.
fn entity_of(app: &mut App, band: BandId) -> Entity {
    let mut query = app.world.query::<(Entity, &BandId)>();
    query
        .iter(&app.world)
        .find(|(_, id)| **id == band)
        .map(|(entity, _)| entity)
        .expect("the band is in the world")
}

/// Split `asked` workers off the fixture band and return `(parent lines, child lines, original)`.
fn split_fixture(
    app: &mut App,
    lines: u16,
    asked: u32,
) -> (FoundingLines, FoundingLines, FoundingLines) {
    let (parent, id) = a_home_band(app);
    seat_fixture_band(app, parent, id, lines);
    let original = lines_of(app, parent);
    let split = split_band_from_parent(&mut app.world, parent, asked, &SETTLE)
        .expect("the fixture band can spare the splinter");
    let child = entity_of(app, split.band);
    (lines_of(app, parent), lines_of(app, child), original)
}

/// **Every faction's starting band is founded with `L` lines, every one originating on its own id**
/// — AI rivals included, because a rival is placed by the same spawn.
#[test]
fn starting_band_is_founded_with_l_lines_of_its_own() {
    let mut app = world();
    let founding_lines = usize::from(
        app.world
            .resource::<DemographicsConfigHandle>()
            .get()
            .lineage
            .founding_lines
            .get(),
    );
    let mut query = app
        .world
        .query_filtered::<(&BandId, &PopulationCohort), With<ResidentBand>>();
    let bands: Vec<_> = query.iter(&app.world).collect();
    let factions: BTreeSet<_> = bands.iter().map(|(_, cohort)| cohort.faction).collect();
    assert!(
        factions.contains(&faction_support::HOME) && factions.contains(&faction_support::RIVAL),
        "both peoples have a starting band: {factions:?}"
    );
    for (id, cohort) in bands {
        assert_eq!(cohort.founding_lines.len(), founding_lines, "band {id:?}");
        assert!(
            cohort
                .founding_lines
                .iter()
                .all(|line| line.origin_band == id.0),
            "band {id:?} holds a line founded elsewhere"
        );
    }
}

/// **A five-person splinter off thirty takes one line and the parent keeps seven** — disjoint, and
/// together exactly the set the parent held.
#[test]
fn a_small_split_takes_a_proportional_share_and_partitions_the_set() {
    let mut app = world();
    let (parent, child, original) = split_fixture(&mut app, LINES, FIVE_PEOPLE_WORKERS);
    assert_eq!(child.len(), FIVE_PEOPLE_LINES);
    assert_eq!(parent.len(), usize::from(LINES) - FIVE_PEOPLE_LINES);
    let (parent, child) = (line_set(&parent), line_set(&child));
    assert!(parent.is_disjoint(&child), "a line walked off AND stayed");
    let union: BTreeSet<_> = parent.union(&child).copied().collect();
    assert_eq!(union, line_set(&original), "a line was lost or minted");
}

/// **The clamp, both ways**: a near-total split leaves the parent one line, and a one-worker split
/// still takes one.
#[test]
fn no_split_leaves_either_half_without_a_line() {
    let mut app = world();
    let (parent, child, _) = split_fixture(&mut app, LINES, ALL_BUT_ONE_WORKER);
    assert_eq!(
        parent.len(),
        AT_LEAST_ONE,
        "a large split took the last line"
    );
    assert_eq!(child.len(), usize::from(LINES) - AT_LEAST_ONE);

    let mut app = world();
    let (parent, child, _) = split_fixture(&mut app, LINES, ONE_WORKER);
    assert_eq!(
        child.len(),
        AT_LEAST_ONE,
        "a tiny split walked off with no line"
    );
    assert_eq!(parent.len(), usize::from(LINES) - AT_LEAST_ONE);
}

/// **A band of one line cannot partition it**: both halves descend from that same family.
#[test]
fn a_single_line_is_shared_by_both_halves() {
    let mut app = world();
    let (parent, child, original) = split_fixture(&mut app, ONE_LINE, FIVE_PEOPLE_WORKERS);
    assert_eq!(line_set(&parent), line_set(&original));
    assert_eq!(line_set(&child), line_set(&original));
}

/// **The checkpoint carries the exact line set** — through the shipped capture/restore path, after
/// a split, so the sets being compared are not simply the founded ones.
#[test]
fn a_checkpoint_round_trip_preserves_every_line_set() {
    let mut app = world();
    let (parent, id) = a_home_band(&mut app);
    seat_fixture_band(&mut app, parent, id, LINES);
    let split = split_band_from_parent(&mut app.world, parent, FIVE_PEOPLE_WORKERS, &SETTLE)
        .expect("the fixture band can spare the splinter");

    let by_band = |app: &mut App| {
        let mut query = app.world.query::<(&BandId, &PopulationCohort)>();
        query
            .iter(&app.world)
            .map(|(id, cohort)| (*id, line_set(&cohort.founding_lines)))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let before = by_band(&mut app);
    assert!(before.contains_key(&split.band) && before.contains_key(&id));

    // The save blob is the checkpoint SERIALIZED — the in-process restore below only clones it, so
    // this is the half that proves the field survives an encode.
    let blob = core_sim::save::encode_save(&app.world).expect("the world encodes");
    let (mut loaded, _) = core_sim::save::load_save(&blob).expect("the save loads");
    assert_eq!(
        by_band(&mut loaded),
        before,
        "a save/load changed a band's lines"
    );

    let checkpoint = capture_sim_state(&app.world);
    restore_sim_state(&mut app.world, &checkpoint);
    assert_eq!(
        by_band(&mut app),
        before,
        "a restore changed a band's lines"
    );
}

/// **The count reaches the encoded envelope**, read off the FlatBuffers row a client parses, for
/// both halves of a split.
#[test]
fn the_snapshot_publishes_each_bands_line_count() {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let mut app = world();
    let parent = a_home_band(&mut app).0;
    let (_, child_lines, _) = split_fixture(&mut app, LINES, FIVE_PEOPLE_WORKERS);
    let parent_lines = lines_of(&app, parent);
    let child = {
        let mut query = app.world.query::<(Entity, &PopulationCohort)>();
        query
            .iter(&app.world)
            .find(|(_, cohort)| line_set(&cohort.founding_lines) == line_set(&child_lines))
            .map(|(entity, _)| entity)
            .expect("the splinter is in the world")
    };
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
    let rows = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list");
    let published = |entity: Entity| {
        rows.iter()
            .find(|row| row.entity() == entity.to_bits())
            .map(|row| row.foundingLines())
            .expect("the band is on the wire")
    };
    assert_eq!(published(parent) as usize, parent_lines.len());
    assert_eq!(published(child) as usize, child_lines.len());
    assert_ne!(
        published(parent),
        published(child),
        "the two halves must publish their own counts, not one shared figure"
    );
}
