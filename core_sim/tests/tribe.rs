//! **A tribe is earned** (#861, `.claude/rules/core_sim/leadership.md`).
//!
//! Each fixture builds its bands directly (a clone of the world's first band, given an id, a size,
//! a belief anchor, a culture layer, and where it matters a chief, a shared
//! origin and a tie) and runs `advance_tribes` on its own, one call per turn, so the points are
//! driven by exactly the inputs the rule reads. The rate arithmetic is unit-pinned beside
//! `points_per_turn`, and the identity arithmetic beside `resolve_matches`.
//!
//! With the shipped weights a pair of kin (origin 3) with identical culture (3) and a full tie (2)
//! earns 8 points a turn once one of them has a leader; the threshold is 100, so the bank is full
//! on the thirteenth such turn.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};

use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    advance_tribes, points_per_turn, publish_baseline_snapshot, reconcile_band_culture_layers,
    tile_rung_span, BandId, BandLeader, BandLeadership, BandName, BeliefRegistry, BranchId,
    CommandEventEntry, CommandEventKind, CommandEventLog, ConnectionKey, ConnectionLedger,
    ConnectionsConfigHandle, CultureManager, CultureOwner, FactionId, FoundingLines,
    LadderConfigHandle, LeaderKind, LeadershipConfigHandle, PopulationCohort, ResidentBand, RungId,
    Sighting, SimulationConfig, SimulationTick, SnapshotHistory, Tile, TileImprovementRegistry,
    TribeLeaderKind, TribeRegistry, TribeStanding, CULTURE_TRAIT_AXES, CULTURE_TRAIT_SPAN,
};

/// Test bands' ids start well clear of any worldgen band.
const TEST_BAND_BASE: u64 = 9_000;
/// The turn the fixtures start counting from.
const FIXTURE_TURN: u64 = 20;
/// Two distinct belief anchors.
const PLACE_A: UVec2 = UVec2::new(3, 3);
const PLACE_B: UVec2 = UVec2::new(8, 8);
/// A chiefed kin pair earns 8 a turn toward the shipped 100: twelve turns bank 96, the thirteenth
/// fills the bank.
const TURNS_TO_CLOSE_A_KIN_PAIR: usize = 13;
/// Enough local turns for a chiefed kin pair (or a third band joining it) to bank full.
const TURNS_TO_FILL_BANK: usize = 40;
/// The most turns any rate-measured fixture runs before giving up.
const TURN_LIMIT: usize = 400;
/// A belief stock so large its anchor weight is as near 1 as makes no difference.
const GREAT_URN: f32 = 1.0e9;

fn spawn_world() -> App {
    let mut app = core_sim::build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = core_sim::HARNESS_MAP_SEED;
    app.world.insert_resource(config);
    app.update();
    core_sim::run_turn(&mut app);
    app
}

fn id(n: u64) -> BandId {
    BandId(TEST_BAND_BASE + n)
}

/// Spawn test band number `n`: sized `size`, anchored at `anchor`, with its own
/// distinct origin and a culture layer flattened to zero (so test bands are identical unless a
/// test says otherwise).
fn band(app: &mut App, n: u64, size: u32, anchor: Option<UVec2>) -> Entity {
    let template = {
        let mut query = app
            .world
            .query_filtered::<(&BandId, &PopulationCohort), With<ResidentBand>>();
        let mut rows: Vec<_> = query.iter(&app.world).collect();
        rows.sort_by_key(|(id, _)| **id);
        rows.first().expect("the world has a band").1.clone()
    };
    let mut cohort = template;
    cohort.size = size;
    cohort.belief_anchor = anchor;
    cohort.founding_lines = FoundingLines::founded(id(n), 2);
    let entity = app
        .world
        .spawn((
            cohort,
            ResidentBand,
            id(n),
            BandName(format!("Test{n}")),
            BandLeadership::default(),
        ))
        .id();
    app.world.run_system_once(reconcile_band_culture_layers);
    let mut culture = app.world.resource_mut::<CultureManager>();
    let layer = culture
        .band_layer_mut_by_owner(CultureOwner::from_band(id(n)))
        .expect("the band has a culture layer");
    for axis in 0..CULTURE_TRAIT_AXES {
        layer
            .traits
            .update_value(axis, core_sim::scalar_from_f32(0.0));
    }
    entity
}

/// A chiefed band descended from starting band `origin`.
fn member(app: &mut App, n: u64, size: u32, anchor: Option<UVec2>, origin: u64) -> Entity {
    let entity = band(app, n, size, anchor);
    app.world
        .get_mut::<PopulationCohort>(entity)
        .unwrap()
        .founding_lines = FoundingLines::founded(id(origin), 2);
    crown(app, entity, &format!("Chief{n}"), LeaderKind::Chief);
    entity
}

/// Push one axis of a band's layer to `value`.
fn set_axis(app: &mut App, band: BandId, axis: usize, value: f32) {
    let mut culture = app.world.resource_mut::<CultureManager>();
    let layer = culture
        .band_layer_mut_by_owner(CultureOwner::from_band(band))
        .expect("the band has a culture layer");
    layer
        .traits
        .update_value(axis, core_sim::scalar_from_f32(value));
}

/// Give every pair among `ns` a full tie, in both directions (locality needs a live tie both ways).
fn tie_all(app: &mut App, ns: &[u64]) {
    let mut ledger = app.world.resource_mut::<ConnectionLedger>();
    for a in ns {
        for b in ns {
            if a != b {
                ledger.insert_full_tie(
                    ConnectionKey::new(id(*a), id(*b)),
                    &Sighting::new(UVec2::ZERO, 1, "", FactionId(0)),
                    1,
                );
            }
        }
    }
}

/// Let every tie in the ledger bleed to nothing (a parked edge is still there, but not live), as
/// the quiet turns of the schedule would. Re-tie whoever should still be in touch afterwards.
fn lapse_all_ties(app: &mut App) {
    let cfg = app.world.resource::<ConnectionsConfigHandle>().get();
    let quiet_turns = (1.0 / cfg.strength.decay_per_turn).ceil() as u64 + 1;
    let mut ledger = app.world.resource_mut::<ConnectionLedger>();
    for _ in 0..quiet_turns {
        ledger.decay_all(FIXTURE_TURN, &cfg);
    }
}

fn set_anchor(app: &mut App, band: Entity, anchor: Option<UVec2>) {
    app.world
        .get_mut::<PopulationCohort>(band)
        .unwrap()
        .belief_anchor = anchor;
}

/// Give a band a chief of its own (what `advance_leadership` would have made it).
fn crown(app: &mut App, band: Entity, name: &str, kind: LeaderKind) {
    app.world.get_mut::<BandLeadership>(band).unwrap().leader = Some(BandLeader {
        name: name.to_string(),
        kind,
        since_turn: 1,
    });
}

/// One turn: advance the fixture tick and run the tribe system.
fn turn(app: &mut App) {
    let next = app.world.resource::<SimulationTick>().0.max(FIXTURE_TURN) + 1;
    app.world.resource_mut::<SimulationTick>().0 = next;
    app.world.run_system_once(advance_tribes);
}

fn turns(app: &mut App, n: usize) {
    for _ in 0..n {
        turn(app);
    }
}

/// Run turns until a tribe exists (at most `TURNS_TO_FILL_BANK`), leaving the tick on the turn
/// it formed so that turn's events can be read.
fn until_formed(app: &mut App) {
    for _ in 0..TURNS_TO_FILL_BANK {
        turn(app);
        if !registry(app).is_empty() {
            return;
        }
    }
    panic!("no tribe formed");
}

/// Run turns until a tribe exists, returning how many ran.
fn turns_until_tribe(app: &mut App) -> usize {
    for ran in 1..=TURN_LIMIT {
        turn(app);
        if !registry(app).is_empty() {
            return ran;
        }
    }
    panic!("no tribe formed");
}

/// The pair (1, 2)'s points per turn, read off the inputs the last turn stored.
fn rate_of_pair(app: &App) -> f64 {
    let config = app.world.resource::<LeadershipConfigHandle>().get();
    let (_, inputs) = registry(app)
        .pairs_of(id(1))
        .next()
        .expect("the pair was scored");
    points_per_turn(&config, inputs)
}

/// How many turns a pair earning `rate` takes to fill the shipped threshold.
fn turns_to_fill(app: &App, rate: f64) -> usize {
    let config = app.world.resource::<LeadershipConfigHandle>().get();
    (config.tribe_score_threshold / rate).ceil() as usize
}

/// Events of `kind` filed on the current turn.
fn events(app: &App, kind: CommandEventKind) -> Vec<CommandEventEntry> {
    let now = app.world.resource::<SimulationTick>().0;
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| entry.kind == kind && entry.tick == now)
        .cloned()
        .collect()
}

fn registry(app: &App) -> &TribeRegistry {
    app.world.resource::<TribeRegistry>()
}

/// Two kin, chiefed, tied, identical bands (sizes 40 and 90) in one network at one place.
fn kin_pair() -> (App, Entity, Entity) {
    let mut app = spawn_world();
    let one = member(&mut app, 1, 40, Some(PLACE_A), 1);
    let two = member(&mut app, 2, 90, Some(PLACE_A), 1);
    tie_all(&mut app, &[1, 2]);
    (app, one, two)
}

#[test]
fn a_fresh_split_is_no_tribe_for_twelve_turns_after_its_chief() {
    let (mut app, _, _) = kin_pair();
    turns(&mut app, TURNS_TO_CLOSE_A_KIN_PAIR - 1);
    assert!(registry(&app).is_empty(), "12 turns bank 96 of 100");
    assert_eq!(registry(&app).points(id(1), id(2)), 96.0);
    assert!(events(&app, CommandEventKind::TribeFormed).is_empty());
}

#[test]
fn a_fresh_split_becomes_a_tribe_when_its_bank_fills() {
    let (mut app, _, _) = kin_pair();
    let ran = turns_until_tribe(&mut app);
    assert_eq!(rate_of_pair(&app), 8.0);
    assert_eq!(ran, turns_to_fill(&app, 8.0));
    assert_eq!(ran, TURNS_TO_CLOSE_A_KIN_PAIR);
    let tribe = registry(&app).tribe_of(id(1)).expect("the bank closed it");
    assert_eq!(tribe.members.len(), 2);
    assert_eq!(tribe.name, "Test2 tribe", "named for the largest band");
}

#[test]
fn the_leader_is_the_largest_chiefed_members_own_person_promoted() {
    let (mut app, _, _) = kin_pair();
    until_formed(&mut app);
    let tribe = registry(&app).tribe_of(id(1)).unwrap();
    assert_eq!(tribe.leader.name, "Chief2", "band 2 is the larger");
    assert_eq!(tribe.leader.band, id(2));
    assert_eq!(tribe.leader.kind, TribeLeaderKind::ParamountChief);
    let formed = events(&app, CommandEventKind::TribeFormed);
    assert_eq!(formed.len(), 1);
    assert_eq!(formed[0].band, id(2).0);
    assert_eq!(
        formed[0].label,
        "Test2 tribe forms; Chief2 is paramount chief"
    );
    let detail = formed[0].detail.as_deref().unwrap();
    assert!(detail.contains("kind=paramount_chief"), "{detail}");
    assert!(detail.contains("leader=Chief2"), "{detail}");
}

#[test]
fn a_council_is_promoted_to_a_council_of_chiefs() {
    let (mut app, one, _) = kin_pair();
    // Band 1 is made the larger and an elders' council.
    app.world.get_mut::<PopulationCohort>(one).unwrap().size = 200;
    crown(&mut app, one, "Maren", LeaderKind::ElderCouncil);
    until_formed(&mut app);
    let tribe = registry(&app).tribe_of(id(1)).unwrap();
    assert_eq!(tribe.leader.name, "Maren");
    assert_eq!(tribe.leader.kind, TribeLeaderKind::CouncilOfChiefs);
    let formed = events(&app, CommandEventKind::TribeFormed);
    assert_eq!(
        formed[0].label,
        "Test1 tribe forms; a council of chiefs gathers, Maren speaks for it"
    );
}

#[test]
fn equal_sized_chiefed_bands_promote_the_lowest_band_id() {
    let (mut app, one, two) = kin_pair();
    app.world.get_mut::<PopulationCohort>(one).unwrap().size = 60;
    app.world.get_mut::<PopulationCohort>(two).unwrap().size = 60;
    turns(&mut app, TURNS_TO_CLOSE_A_KIN_PAIR);
    assert_eq!(
        registry(&app).tribe_of(id(1)).unwrap().leader.name,
        "Chief1"
    );
}

#[test]
fn no_chief_no_points_and_the_tribe_forms_threshold_over_rate_turns_after_a_chief_is_crowned() {
    let mut app = spawn_world();
    band(&mut app, 1, 50, Some(PLACE_A));
    let two = band(&mut app, 2, 50, Some(PLACE_A));
    // Make them kin and tied: the rate is as high as it gets.
    app.world
        .get_mut::<PopulationCohort>(two)
        .unwrap()
        .founding_lines = FoundingLines::founded(id(1), 2);
    tie_all(&mut app, &[1, 2]);
    turns(&mut app, 3 * TURNS_TO_FILL_BANK);
    assert_eq!(rate_of_pair(&app), 8.0, "the pair would earn 8 a turn");
    assert_eq!(
        registry(&app).points(id(1), id(2)),
        0.0,
        "nothing banks with nobody to follow"
    );
    assert!(registry(&app).is_empty());

    crown(&mut app, two, "Hale", LeaderKind::Chief);
    let ran = turns_until_tribe(&mut app);
    assert_eq!(
        ran,
        turns_to_fill(&app, 8.0),
        "ceil(threshold / rate) turns after the crown, not one or two"
    );
    assert_eq!(registry(&app).tribe_of(id(1)).unwrap().leader.name, "Hale");
    assert_eq!(events(&app, CommandEventKind::TribeFormed).len(), 1);
}

#[test]
fn unrelated_but_alike_bands_earn_five_a_turn_and_take_twenty() {
    let mut app = spawn_world();
    // Distinct origins: likeness 3 + tie 2 = 5 a turn.
    member(&mut app, 1, 50, Some(PLACE_A), 1);
    member(&mut app, 2, 50, Some(PLACE_A), 2);
    tie_all(&mut app, &[1, 2]);
    let ran = turns_until_tribe(&mut app);
    assert_eq!(rate_of_pair(&app), 5.0);
    assert_eq!(ran, turns_to_fill(&app, 5.0));
    assert_eq!(ran, 20);
    assert!(ran > TURNS_TO_CLOSE_A_KIN_PAIR);
}

#[test]
fn unrelated_bands_with_no_tie_are_not_local_and_earn_nothing() {
    let mut app = spawn_world();
    // Same people, same urn, a chief each -- and no tie in either direction.
    member(&mut app, 1, 50, Some(PLACE_A), 1);
    member(&mut app, 2, 50, Some(PLACE_A), 2);
    turns(&mut app, TURNS_TO_FILL_BANK * 2);
    assert_eq!(registry(&app).points(id(1), id(2)), 0.0);
    assert!(registry(&app).is_empty());
    assert_eq!(
        registry(&app).standing_of(id(1)),
        Some(TribeStanding::OutOfTouch)
    );
}

#[test]
fn a_one_way_tie_is_not_local() {
    let mut app = spawn_world();
    member(&mut app, 1, 50, Some(PLACE_A), 1);
    member(&mut app, 2, 50, Some(PLACE_A), 1);
    app.world
        .resource_mut::<ConnectionLedger>()
        .insert_full_tie(
            ConnectionKey::new(id(1), id(2)),
            &Sighting::new(UVec2::ZERO, 1, "", FactionId(0)),
            1,
        );
    turns(&mut app, TURNS_TO_FILL_BANK);
    assert_eq!(registry(&app).points(id(1), id(2)), 0.0);
    assert!(registry(&app).is_empty());
}

/// Put band `n` on the tile at `at`, so a fixture can stand its bands as far apart as it likes.
fn stand_at(app: &mut App, n: u64, at: UVec2) {
    let tile = {
        let mut tiles = app.world.query::<(Entity, &Tile)>();
        tiles
            .iter(&app.world)
            .find(|(_, tile)| tile.position == at)
            .map(|(entity, _)| entity)
            .expect("the map has the tile")
    };
    let band = {
        let mut bands = app
            .world
            .query_filtered::<(Entity, &BandId), With<ResidentBand>>();
        bands
            .iter(&app.world)
            .find(|(_, band)| **band == id(n))
            .map(|(entity, _)| entity)
            .expect("the band exists")
    };
    app.world
        .get_mut::<PopulationCohort>(band)
        .unwrap()
        .current_tile = tile;
}

/// Far kin: two tiles this far apart are well past the supply network's free pooling reach.
const FAR_KIN_TILES: u32 = 8;

#[test]
fn far_kin_in_touch_earn_points_and_form_a_tribe_and_a_lapsed_tie_fades_them() {
    let (mut app, _, _) = kin_pair();
    let (home, far) = (UVec2::new(2, 2), UVec2::new(2 + FAR_KIN_TILES, 2));
    stand_at(&mut app, 1, home);
    stand_at(&mut app, 2, far);
    let reach = app
        .world
        .resource::<core_sim::SupplyNetworkConfigHandle>()
        .get()
        .reach_tiles;
    let width = app.world.resource::<core_sim::TileRegistry>().width;
    assert!(
        core_sim::grid_utils::hex_distance_wrapped(home, far, width, false) > reach,
        "the pair stands beyond the pooling reach: no supply network could join them"
    );
    let ran = turns_until_tribe(&mut app);
    assert_eq!(
        ran, TURNS_TO_CLOSE_A_KIN_PAIR,
        "a live tie is all locality asks"
    );
    assert_eq!(
        registry(&app).standing_of(id(1)),
        Some(TribeStanding::Local)
    );

    // The tie lapses: the pair is no longer local, the tribe goes and the bank fades each turn.
    lapse_all_ties(&mut app);
    turn(&mut app);
    assert!(registry(&app).is_empty(), "out of touch, no tribe");
    assert_eq!(
        registry(&app).standing_of(id(1)),
        Some(TribeStanding::OutOfTouch)
    );
    let after_one = registry(&app).points(id(1), id(2));
    assert!(after_one < 100.0, "the bank fades while out of touch");
    turns(&mut app, 5);
    assert!(registry(&app).points(id(1), id(2)) < after_one);
}

#[test]
fn a_less_alike_pair_takes_longer_to_close() {
    let (mut app, _, _) = kin_pair();
    // One axis a half range apart: likeness 0.5, so 3 + 1.5 + 2 = 6.5 a turn.
    set_axis(&mut app, id(2), 0, CULTURE_TRAIT_SPAN);
    let ran = turns_until_tribe(&mut app);
    assert_eq!(rate_of_pair(&app), 6.5);
    assert_eq!(ran, turns_to_fill(&app, 6.5));
    assert!(ran > TURNS_TO_CLOSE_A_KIN_PAIR);
}

#[test]
fn belief_on_the_shared_urn_raises_the_rate() {
    let (mut app, _, _) = kin_pair();
    app.world
        .resource_mut::<BeliefRegistry>()
        .add(PLACE_A, GREAT_URN);
    let ran = turns_until_tribe(&mut app);
    let rate = rate_of_pair(&app);
    assert!(rate > 8.0 && rate <= 9.0, "8 + about 1 of belief: {rate}");
    assert_eq!(ran, turns_to_fill(&app, rate));
    assert!(ran < TURNS_TO_CLOSE_A_KIN_PAIR);
}

/// Raise the monument on `tile` to `fraction` of its own span (1.0 = held).
fn raise_monument(app: &mut App, tile: UVec2, fraction: f32) {
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let branch = BranchId::new("belief");
    let (base, width) = tile_rung_span(RungId::new(branch, "monument"), &ladder);
    app.world
        .resource_mut::<TileImprovementRegistry>()
        .place(tile, branch, FactionId(0), &ladder)
        .set_position(base + width * fraction, &ladder);
}

#[test]
fn a_kept_monument_on_the_shared_urn_raises_the_rate() {
    let (mut app, _, _) = kin_pair();
    raise_monument(&mut app, PLACE_A, 1.0);
    let ran = turns_until_tribe(&mut app);
    assert_eq!(rate_of_pair(&app), 10.0, "8 + the monument's 2");
    assert_eq!(ran, turns_to_fill(&app, 10.0));
    assert_eq!(ran, 10);
    assert!(ran < TURNS_TO_CLOSE_A_KIN_PAIR);
}

#[test]
fn a_half_raised_monument_or_one_elsewhere_adds_nothing() {
    let (mut app, _, _) = kin_pair();
    raise_monument(&mut app, PLACE_A, 0.5);
    raise_monument(&mut app, PLACE_B, 1.0);
    turn(&mut app);
    assert_eq!(
        rate_of_pair(&app),
        8.0,
        "a half-raised monument is not held, and one off the urn is not theirs"
    );
}

#[test]
fn parting_fades_points_rather_than_resetting_them_and_a_return_rebuilds_from_what_is_left() {
    let (mut app, _, two) = kin_pair();
    turns(&mut app, TURNS_TO_FILL_BANK);
    let first = registry(&app).tribe_of(id(1)).unwrap().id;
    assert_eq!(registry(&app).points(id(1), id(2)), 100.0, "a full bank");

    // Apart ten turns: the tribe is gone at once, the bank lost a hundredth of itself a turn.
    set_anchor(&mut app, two, Some(PLACE_B));
    turn(&mut app);
    assert!(registry(&app).is_empty());
    assert_eq!(events(&app, CommandEventKind::TribeDissolved).len(), 1);
    turns(&mut app, 9);
    let kept = registry(&app).points(id(1), id(2));
    assert!((kept - 90.0).abs() < 1e-9, "faded, not reset: {kept}");

    // Back together it earns 8 on top of the 90: not yet full, then full.
    set_anchor(&mut app, two, Some(PLACE_A));
    turn(&mut app);
    assert!(registry(&app).is_empty(), "98 of 100");
    turn(&mut app);
    let again = registry(&app).tribe_of(id(1)).expect("full again");
    assert_ne!(again.id, first);
}

#[test]
fn apart_long_enough_the_points_are_nearly_gone_and_must_be_rebuilt() {
    let (mut app, _, two) = kin_pair();
    turns(&mut app, TURNS_TO_FILL_BANK);
    set_anchor(&mut app, two, Some(PLACE_B));
    turns(&mut app, 90);
    assert!((registry(&app).points(id(1), id(2)) - 10.0).abs() < 1e-9);

    set_anchor(&mut app, two, Some(PLACE_A));
    turns(&mut app, 11);
    assert!(registry(&app).is_empty(), "10 + 88 = 98 is under the line");
    turn(&mut app);
    assert!(registry(&app).tribe_of(id(1)).is_some());
}

#[test]
fn nothing_forms_without_a_shared_place_a_tie_a_people_or_an_anchor() {
    let mut app = spawn_world();
    member(&mut app, 1, 50, Some(PLACE_A), 1);
    member(&mut app, 2, 50, Some(PLACE_B), 1); // another place
    member(&mut app, 3, 50, Some(PLACE_A), 1); // same place, but tied to nobody
    member(&mut app, 4, 50, None, 1); // no anchor
    let other_people = member(&mut app, 5, 50, Some(PLACE_A), 1);
    app.world
        .get_mut::<PopulationCohort>(other_people)
        .unwrap()
        .faction = FactionId(1); // another people
    tie_all(&mut app, &[1, 2, 4, 5]);
    turns(&mut app, TURNS_TO_FILL_BANK * 2);
    assert!(registry(&app).is_empty());
}

#[test]
fn a_third_band_joins_when_its_pairs_close_and_a_lapsed_tie_leaves() {
    let (mut app, _, _) = kin_pair();
    member(&mut app, 3, 50, Some(PLACE_A), 1);
    turns(&mut app, TURNS_TO_FILL_BANK);
    let tribe = registry(&app).tribe_of(id(1)).unwrap().id;
    assert!(registry(&app).tribe_of(id(3)).is_none(), "tied to nobody");

    tie_all(&mut app, &[1, 2, 3]);
    turns(&mut app, 2);
    assert!(
        registry(&app).tribe_of(id(3)).is_none(),
        "points not yet banked"
    );
    turns(&mut app, TURNS_TO_CLOSE_A_KIN_PAIR);
    assert_eq!(registry(&app).tribe_of(id(3)).unwrap().id, tribe);

    lapse_all_ties(&mut app);
    tie_all(&mut app, &[1, 2]);
    turn(&mut app);
    assert!(registry(&app).tribe_of(id(3)).is_none());
    assert_eq!(registry(&app).tribe_of(id(1)).unwrap().id, tribe);
    let left = events(&app, CommandEventKind::TribeBandLeft);
    assert_eq!(left.len(), 1);
    assert_eq!(
        left[0].label,
        format!("Band {} leaves the Test2 tribe", id(3).0)
    );
    assert!(left[0]
        .detail
        .as_deref()
        .unwrap()
        .ends_with(&format!("band={}", id(3).0)));
}

#[test]
fn a_joining_band_is_announced_by_its_durable_id() {
    let (mut app, _, _) = kin_pair();
    member(&mut app, 3, 50, Some(PLACE_A), 1);
    turns(&mut app, TURNS_TO_FILL_BANK);
    tie_all(&mut app, &[1, 2, 3]);
    let mut joined = Vec::new();
    for _ in 0..TURNS_TO_CLOSE_A_KIN_PAIR + 2 {
        turn(&mut app);
        joined.extend(events(&app, CommandEventKind::TribeBandJoined));
    }
    assert_eq!(joined.len(), 1);
    assert_eq!(
        joined[0].label,
        format!("Band {} joins the Test2 tribe", id(3).0)
    );
    assert!(joined[0]
        .detail
        .as_deref()
        .unwrap()
        .ends_with(&format!("band={}", id(3).0)));
}

#[test]
fn a_tribe_with_no_chiefed_member_left_dissolves() {
    let (mut app, one, two) = kin_pair();
    turns(&mut app, TURNS_TO_CLOSE_A_KIN_PAIR);
    assert_eq!(registry(&app).len(), 1);
    app.world.get_mut::<BandLeadership>(one).unwrap().leader = None;
    app.world.get_mut::<BandLeadership>(two).unwrap().leader = None;
    turn(&mut app);
    assert!(registry(&app).is_empty());
    assert_eq!(events(&app, CommandEventKind::TribeDissolved).len(), 1);
}

#[test]
fn the_leader_is_re_promoted_quietly_when_its_band_leaves() {
    let mut app = spawn_world();
    let sizes = [40, 90, 70];
    let entities: Vec<Entity> = (1..=3)
        .map(|n| member(&mut app, n, sizes[n as usize - 1], Some(PLACE_A), 1))
        .collect();
    tie_all(&mut app, &[1, 2, 3]);
    turns(&mut app, TURNS_TO_FILL_BANK);
    let tribe = registry(&app).tribe_of(id(1)).unwrap().clone();
    assert_eq!(tribe.leader.name, "Chief2");

    // The leader's band goes to another place; the tribe is the other two.
    set_anchor(&mut app, entities[1], Some(PLACE_B));
    turn(&mut app);
    let kept = registry(&app).tribe_of(id(1)).unwrap();
    assert_eq!(kept.id, tribe.id, "identity survives");
    assert_eq!(
        kept.leader.name, "Chief3",
        "the next largest chiefed member"
    );
    assert_eq!(kept.leader.band, id(3));
    assert!(events(&app, CommandEventKind::TribeFormed).is_empty());
    assert_eq!(events(&app, CommandEventKind::TribeBandLeft).len(), 1);
}

#[test]
fn a_tie_graph_split_keeps_the_tribe_on_the_majority_side_and_the_other_side_forms_its_own() {
    let mut app = spawn_world();
    for n in 1..=5 {
        member(&mut app, n, 50, Some(PLACE_A), 1);
    }
    tie_all(&mut app, &[1, 2, 3, 4, 5]);
    turns(&mut app, TURNS_TO_FILL_BANK);
    let original = registry(&app).tribe_of(id(1)).unwrap().id;
    assert_eq!(registry(&app).tribe_of(id(1)).unwrap().members.len(), 5);

    lapse_all_ties(&mut app);
    tie_all(&mut app, &[1, 2, 3]);
    tie_all(&mut app, &[4, 5]);
    turn(&mut app);
    assert_eq!(registry(&app).len(), 2);
    assert_eq!(registry(&app).tribe_of(id(1)).unwrap().id, original);
    assert_eq!(registry(&app).tribe_of(id(1)).unwrap().members.len(), 3);
    let other = registry(&app).tribe_of(id(4)).unwrap();
    assert_ne!(other.id, original);
    assert_eq!(other.members.len(), 2, "its pair kept its bank");
    assert_eq!(events(&app, CommandEventKind::TribeFormed).len(), 1);
    assert_eq!(events(&app, CommandEventKind::TribeBandLeft).len(), 2);
}

#[test]
fn two_tribes_sharing_a_place_merge_into_the_one_holding_more_once_their_pairs_close() {
    let mut app = spawn_world();
    for n in 1..=5 {
        member(&mut app, n, 50, Some(PLACE_A), 1);
    }
    tie_all(&mut app, &[1, 2]);
    tie_all(&mut app, &[3, 4, 5]);
    turns(&mut app, TURNS_TO_FILL_BANK);
    let small = registry(&app).tribe_of(id(1)).unwrap().id;
    let big = registry(&app).tribe_of(id(3)).unwrap().id;
    assert_eq!(registry(&app).len(), 2);

    tie_all(&mut app, &[1, 2, 3, 4, 5]);
    turn(&mut app);
    assert_eq!(registry(&app).len(), 2, "cross pairs have no points yet");
    let mut merged_turn_joins = 0;
    for _ in 0..TURNS_TO_CLOSE_A_KIN_PAIR + 2 {
        turn(&mut app);
        if registry(&app).len() == 1 {
            merged_turn_joins = events(&app, CommandEventKind::TribeBandJoined).len();
            assert_eq!(events(&app, CommandEventKind::TribeDissolved).len(), 1);
            break;
        }
    }
    assert_eq!(
        registry(&app).len(),
        1,
        "merged once the cross pairs closed"
    );
    assert_eq!(registry(&app).tribe_of(id(1)).unwrap().id, big);
    assert!(registry(&app).get(small).is_none());
    assert_eq!(merged_turn_joins, 2);
}

#[test]
fn the_registry_and_its_banks_round_trip_through_a_checkpoint() {
    let (mut app, _, _) = kin_pair();
    turns(&mut app, TURNS_TO_CLOSE_A_KIN_PAIR);
    let third = member(&mut app, 3, 50, Some(PLACE_A), 1);
    let _ = third;
    tie_all(&mut app, &[1, 2, 3]);
    turns(&mut app, 3);
    let before = registry(&app).clone();
    assert_eq!(before.len(), 1);
    assert!(before.points(id(1), id(3)) > 0.0, "a bank in progress");

    let checkpoint = capture_sim_state(&app.world);
    turns(&mut app, 5);
    restore_sim_state(&mut app.world, &checkpoint);
    assert_eq!(registry(&app), &before);
}

#[test]
fn the_telling_tells_a_formation_once() {
    let (mut app, _, _) = kin_pair();
    turns(&mut app, TURNS_TO_CLOSE_A_KIN_PAIR);
    assert_eq!(registry(&app).len(), 1);

    let told = |app: &App| {
        app.world
            .resource::<CommandEventLog>()
            .iter()
            .filter(|entry| {
                entry.kind == CommandEventKind::NarrativeBeat
                    && entry
                        .detail
                        .as_deref()
                        .is_some_and(|d| d.contains("tribes.formed_this_turn=1"))
            })
            .count()
    };
    core_sim::run_turn(&mut app);
    assert_eq!(
        told(&app),
        1,
        "the beat fires on the turn after the formation count moves"
    );
    for _ in 0..3 {
        core_sim::run_turn(&mut app);
    }
    assert_eq!(told(&app), 1, "and not again");
}

// ---- Progress toward a tribe, as published (#861) ----

/// The published row of band `entity`: encode the frame and read its cohort row.
fn publish_rows(app: &mut App) -> Vec<u8> {
    publish_baseline_snapshot(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref()).to_vec()
}

/// Run `read` over band `n`'s decoded cohort row.
fn with_row<T>(app: &mut App, n: u64, read: impl FnOnce(fb::PopulationCohortState<'_>) -> T) -> T {
    let bytes = publish_rows(app);
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let row = envelope
        .payload_as_snapshot()
        .expect("a snapshot payload")
        .population()
        .and_then(|section| section.populations())
        .expect("the cohort list is published")
        .iter()
        .find(|row| row.bandId() == id(n).0)
        .expect("the band is on the wire");
    read(row)
}

#[test]
fn a_local_pair_publishes_its_banked_points_and_each_inputs_points_per_turn() {
    let (mut app, _, _) = kin_pair();
    turns(&mut app, 3);
    let config = app.world.resource::<LeadershipConfigHandle>().get();
    let stored = registry(&app)
        .pairs_of(id(1))
        .map(|(partner, inputs)| (partner, *inputs))
        .collect::<Vec<_>>();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].0, id(2));
    let expected = points_per_turn(&config, &stored[0].1);

    with_row(&mut app, 1, |row| {
        assert_eq!(row.tribeThreshold(), config.tribe_score_threshold as f32);
        let w = row.tribeWeights().expect("weights are published");
        assert_eq!(w.origin(), config.tribe_weight_origin as f32);
        assert_eq!(w.likeness(), config.tribe_weight_likeness as f32);
        assert_eq!(w.tie(), config.tribe_weight_tie as f32);
        assert_eq!(w.belief(), config.tribe_weight_belief as f32);
        assert_eq!(w.monument(), config.tribe_weight_monument as f32);
        assert_eq!(w.acting(), config.tribe_weight_acting as f32);
        let pairs = row.tribePairs().expect("pairs are published");
        assert_eq!(pairs.len(), 1);
        let pair = pairs.get(0);
        assert_eq!(pair.partnerBand(), id(2).0);
        assert_eq!(pair.partnerName(), Some("Test2"));
        let c = pair.contribution().expect("contribution");
        assert_eq!(c.origin(), w.origin(), "kin: origin input is 1");
        assert!((c.tie() - w.tie()).abs() < 1e-6, "full tie");
        assert!(
            (f64::from(pair.points()) - 3.0 * expected).abs() < 1e-3,
            "three turns of the pair's rate are banked: {}",
            pair.points()
        );
        let sum = c.origin() + c.likeness() + c.tie() + c.belief() + c.monument() + c.acting();
        assert!(
            (f64::from(sum) - expected).abs() < 1e-4,
            "{sum} vs {expected}"
        );
    });
    // The other band sees the same pair from its side.
    with_row(&mut app, 2, |row| {
        let pair = row.tribePairs().unwrap().get(0);
        assert_eq!(pair.partnerBand(), id(1).0);
        assert_eq!(pair.partnerName(), Some("Test1"));
    });
}

/// Read the formed-tribe fields of band `n`'s row.
fn tribe_fields(app: &mut App, n: u64) -> (String, Vec<String>, String, u8) {
    with_row(app, n, |row| {
        (
            row.tribeName().unwrap_or_default().to_string(),
            row.tribeMemberNames()
                .map(|names| names.iter().map(str::to_string).collect())
                .unwrap_or_default(),
            row.tribeLeaderName().unwrap_or_default().to_string(),
            row.tribeLeaderKind(),
        )
    })
}

fn set_origin(app: &mut App, n: u64, origin: u64) {
    let entity = app
        .world
        .query_filtered::<(Entity, &BandId), With<ResidentBand>>()
        .iter(&app.world)
        .find(|(_, b)| **b == id(n))
        .map(|(e, _)| e)
        .unwrap();
    app.world
        .get_mut::<PopulationCohort>(entity)
        .unwrap()
        .founding_lines = FoundingLines::founded(id(origin), 2);
}

#[test]
fn every_member_of_a_formed_tribe_publishes_it_chiefless_ones_included() {
    // Only band 1 is chiefed; 2 and 3 are chiefless kin, each close to 1 through its bank.
    let mut app = spawn_world();
    member(&mut app, 1, 40, Some(PLACE_A), 1);
    band(&mut app, 2, 90, Some(PLACE_A));
    band(&mut app, 3, 60, Some(PLACE_A));
    set_origin(&mut app, 2, 1);
    set_origin(&mut app, 3, 1);
    tie_all(&mut app, &[1, 2, 3]);
    turns(&mut app, TURNS_TO_FILL_BANK);
    assert_eq!(registry(&app).len(), 1, "one tribe of three");
    let expected = (
        "Test2 tribe".to_string(),
        vec![
            "Test2".to_string(),
            "Test3".to_string(),
            "Test1".to_string(),
        ],
        "Chief1".to_string(),
        sim_schema::TRIBE_LEADER_PARAMOUNT_CHIEF,
    );
    for n in [1, 2, 3] {
        assert_eq!(tribe_fields(&mut app, n), expected, "band {n}");
    }
    // Chiefless members publish their progress too: a chiefless band can join a tribe.
    with_row(&mut app, 2, |row| assert!(row.tribeThreshold() > 0.0));
}

#[test]
fn a_chain_of_close_pairs_is_one_tribe_of_three() {
    // 1 and 3 are chiefed, 2 is not. Culture on one axis runs -span, 0, +span, so 1-2 and 2-3 are
    // close (3 + 1.5 + 2 + 5) and 1-3 is not (3 + 0 + 0 + 5 = 8).
    let mut app = spawn_world();
    member(&mut app, 1, 40, Some(PLACE_A), 1);
    band(&mut app, 2, 90, Some(PLACE_A));
    member(&mut app, 3, 60, Some(PLACE_A), 1);
    set_origin(&mut app, 2, 1);
    {
        let mut ledger = app.world.resource_mut::<ConnectionLedger>();
        for (a, b) in [(1, 2), (2, 1), (2, 3), (3, 2)] {
            ledger.insert_full_tie(
                ConnectionKey::new(id(a), id(b)),
                &Sighting::new(UVec2::ZERO, 1, "", FactionId(0)),
                1,
            );
        }
    }
    set_axis(&mut app, id(1), 0, -CULTURE_TRAIT_SPAN);
    set_axis(&mut app, id(3), 0, CULTURE_TRAIT_SPAN);
    turns(&mut app, TURNS_TO_FILL_BANK);
    assert_eq!(registry(&app).len(), 1);
    let tribe = registry(&app).tribe_of(id(1)).unwrap();
    assert_eq!(tribe.members.len(), 3, "a chain is one tribe, not a pair");
}

#[test]
fn a_band_in_no_tribe_and_a_foreign_band_publish_no_tribe() {
    let (mut app, _, _) = kin_pair();
    let loner = band(&mut app, 3, 50, Some(PLACE_B));
    let foreign = band(&mut app, 4, 50, Some(PLACE_A));
    app.world
        .get_mut::<PopulationCohort>(foreign)
        .unwrap()
        .faction = FactionId(1);
    crown(&mut app, foreign, "Rival", LeaderKind::Chief);
    crown(&mut app, loner, "Solo", LeaderKind::Chief);
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.fog_enabled = false;
    app.world.insert_resource(config);
    turns(&mut app, TURNS_TO_FILL_BANK);
    assert!(registry(&app).tribe_of(id(1)).is_some());
    for n in [3, 4] {
        assert_eq!(
            tribe_fields(&mut app, n),
            (String::new(), Vec::new(), String::new(), 0),
            "band {n}"
        );
    }
}

#[test]
fn standing_says_why_a_band_is_not_in_reach() {
    let mut app = spawn_world();
    member(&mut app, 1, 50, Some(PLACE_A), 1); // shares the urn with 4, but no tie either way
    member(&mut app, 2, 50, Some(PLACE_B), 1); // alone at its place
    member(&mut app, 3, 50, None, 1); // no urn
    member(&mut app, 4, 50, Some(PLACE_A), 1); // the other end of that untied pair
    member(&mut app, 5, 50, None, 1); // no urn
    turn(&mut app);
    let r = registry(&app);
    assert_eq!(r.standing_of(id(1)), Some(TribeStanding::OutOfTouch));
    assert_eq!(r.standing_of(id(2)), Some(TribeStanding::Alone));
    assert_eq!(r.standing_of(id(3)), Some(TribeStanding::NoUrn));
    assert_eq!(r.standing_of(id(4)), Some(TribeStanding::OutOfTouch));
    assert_eq!(r.standing_of(id(5)), Some(TribeStanding::NoUrn));
    for n in [1, 3, 4, 5] {
        with_row(&mut app, n, |row| {
            assert_eq!(row.tribePairs().map_or(0, |p| p.len()), 0, "band {n}");
        });
    }
}

/// Two local bands, neither with a leader.
fn chiefless_pair() -> (App, Entity, Entity) {
    let mut app = spawn_world();
    let one = band(&mut app, 1, 40, Some(PLACE_A));
    let two = band(&mut app, 2, 90, Some(PLACE_A));
    tie_all(&mut app, &[1, 2]);
    (app, one, two)
}

#[test]
fn nothing_builds_toward_a_tribe_until_a_chief_exists() {
    let (mut app, one, _) = chiefless_pair();
    turns(&mut app, 8);
    assert_eq!(registry(&app).points(id(1), id(2)), 0.0, "held, not banked");
    // Two chiefless local bands each publish the pair, and neither partner has a leader.
    for (n, partner) in [(1, 2), (2, 1)] {
        with_row(&mut app, n, |row| {
            assert!(row.tribeThreshold() > 0.0, "progress is published");
            let pairs = row.tribePairs().expect("pairs");
            assert_eq!(pairs.len(), 1);
            assert_eq!(pairs.get(0).partnerBand(), id(partner).0);
            assert!(!pairs.get(0).partnerHasLeader(), "band {n}");
            assert_eq!(pairs.get(0).points(), 0.0);
        });
    }

    // One band is crowned: points bank from that turn, and both rows show it. These bands have
    // distinct origins, so the pair earns likeness 3 + tie 2 a turn.
    crown(&mut app, one, "Chief1", LeaderKind::Chief);
    turns(&mut app, 3);
    let built = registry(&app).points(id(1), id(2));
    let config = app.world.resource::<LeadershipConfigHandle>().get();
    let rate = config.tribe_weight_likeness + config.tribe_weight_tie;
    assert!(
        (built - 3.0 * rate).abs() < 1e-9,
        "three turns banked: {built}"
    );
    with_row(&mut app, 1, |row| {
        assert!(row.tribeThreshold() > 0.0);
        let pairs = row.tribePairs().expect("pairs");
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs.get(0).partnerBand(), id(2).0);
        assert!((pairs.get(0).points() - built as f32).abs() < 1e-6);
        assert!(!pairs.get(0).partnerHasLeader(), "band 2 has no chief");
    });
    // The chiefless band publishes its pair with the chiefed partner, points and all.
    with_row(&mut app, 2, |row| {
        assert!(row.tribeThreshold() > 0.0);
        assert!(row.tribeWeights().is_some_and(|w| w.origin() > 0.0));
        let pairs = row.tribePairs().expect("pairs");
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs.get(0).partnerBand(), id(1).0);
        assert!((pairs.get(0).points() - built as f32).abs() < 1e-6);
        assert!(pairs.get(0).partnerHasLeader(), "band 1 has a chief");
    });
}

#[test]
fn a_local_pair_with_no_leader_left_holds_its_points_and_a_parted_one_fades() {
    let (mut app, one, two) = kin_pair();
    turns(&mut app, 5);
    let built = registry(&app).points(id(1), id(2));
    // Both leaders go: the bank holds while the pair stays local.
    for e in [one, two] {
        app.world.get_mut::<BandLeadership>(e).unwrap().leader = None;
    }
    turns(&mut app, 10);
    assert_eq!(registry(&app).points(id(1), id(2)), built);
    // Parted, it fades; and the parted pair is no row on the wire.
    crown(&mut app, one, "Chief1", LeaderKind::Chief);
    set_anchor(&mut app, two, Some(PLACE_B));
    turns(&mut app, 10);
    assert!(registry(&app).points(id(1), id(2)) < built);
    with_row(&mut app, 1, |row| {
        assert_eq!(row.tribePairs().map_or(0, |p| p.len()), 0);
    });
}

#[test]
fn a_foreign_band_publishes_the_defaults() {
    let (mut app, _, _) = kin_pair();
    let foreign = band(&mut app, 3, 50, Some(PLACE_A));
    app.world
        .get_mut::<PopulationCohort>(foreign)
        .unwrap()
        .faction = FactionId(1);
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.fog_enabled = false;
    app.world.insert_resource(config);
    turns(&mut app, 3);
    with_row(&mut app, 3, |row| {
        assert_eq!(row.tribeThreshold(), 0.0);
        assert_eq!(row.tribePairs().map_or(0, |p| p.len()), 0);
        assert_eq!(row.tribeWeights().map_or(0.0, |w| w.origin()), 0.0);
    });
    // The foreign band is no partner of the viewer's bands either.
    with_row(&mut app, 1, |row| {
        assert_eq!(row.tribePairs().unwrap().len(), 1)
    });
}

#[test]
fn the_scores_round_trip_the_checkpoint_so_a_load_publishes_progress() {
    let (mut app, _, _) = kin_pair();
    turns(&mut app, 3);
    let before: Vec<_> = registry(&app)
        .pairs_of(id(1))
        .map(|(p, s)| (p, *s))
        .collect();
    let state = capture_sim_state(&app.world);
    restore_sim_state(&mut app.world, &state);
    let after: Vec<_> = registry(&app)
        .pairs_of(id(1))
        .map(|(p, s)| (p, *s))
        .collect();
    assert_eq!(before, after);
    assert_eq!(
        registry(&app).standing_of(id(1)),
        Some(TribeStanding::Local)
    );
}

#[test]
fn reach_is_published_for_a_chiefed_band_a_tribe_member_and_a_chiefless_member() {
    // Band 1 (chiefed, 200 people: over its own reach) and 2 (chiefed, 300: the largest, so its chief is
    // promoted to lead the tribe) form a tribe with chiefless kin band 3.
    let mut app = spawn_world();
    member(&mut app, 1, 200, Some(PLACE_A), 1);
    member(&mut app, 2, 300, Some(PLACE_A), 1);
    band(&mut app, 3, 60, Some(PLACE_A));
    set_origin(&mut app, 3, 1);
    // A chiefed band in no tribe, over its reach.
    member(&mut app, 4, 200, Some(PLACE_B), 4);
    // A chiefless band in no tribe.
    band(&mut app, 5, 30, None);
    tie_all(&mut app, &[1, 2, 3]);
    turns(&mut app, TURNS_TO_FILL_BANK);
    assert_eq!(registry(&app).tribe_of(id(1)).unwrap().members.len(), 3);
    let config = app.world.resource::<LeadershipConfigHandle>().get();
    let read = |row: fb::PopulationCohortState<'_>| {
        (
            row.leaderReach(),
            row.leaderLoad(),
            row.tribeLeaderReach(),
            row.tribeLeaderLoad(),
            row.ordersCarriedOut(),
        )
    };
    let reach = config.reach_chief_people as f32;
    let tribe_reach = config.reach_paramount_bands as f32;
    let eff = config.over_reach_efficiency as f32;
    // Chiefed tribe member over its own reach: the tribe factor (three bands is within reach) is
    // in `ordersCarriedOut`, the band's own, lower factor in `leaderOrdersCarriedOut`.
    let over = ((config.reach_chief_people
        + config.over_reach_efficiency * (200.0 - config.reach_chief_people))
        / 200.0) as f32;
    assert!(over < 1.0);
    assert_eq!(
        with_row(&mut app, 1, |row| (
            row.leaderReach(),
            row.leaderLoad(),
            row.tribeLeaderReach(),
            row.tribeLeaderLoad(),
            row.ordersCarriedOut(),
            row.leaderOrdersCarriedOut()
        )),
        (reach, 200.0, tribe_reach, 3.0, 1.0, over)
    );
    // Chiefless tribe member: no own reach, the tribe's, and the efficiency applied once.
    assert_eq!(
        with_row(&mut app, 3, read),
        (0.0, 0.0, tribe_reach, 3.0, eff)
    );
    // Chiefed, no tribe, over its reach: (150 + 0.5 * 50) / 200.
    assert_eq!(with_row(&mut app, 4, read), (reach, 200.0, 0.0, 0.0, over));
    assert_eq!(
        with_row(&mut app, 4, |row| row.leaderOrdersCarriedOut()),
        over
    );
    assert_eq!(
        with_row(&mut app, 3, |row| row.leaderOrdersCarriedOut()),
        0.0
    );
    // Neither leader nor tribe: nothing to show.
    assert_eq!(with_row(&mut app, 5, read), (0.0, 0.0, 0.0, 0.0, 0.0));
}
