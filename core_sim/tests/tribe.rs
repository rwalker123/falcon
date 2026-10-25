//! **A tribe is earned** (#861, `.claude/rules/core_sim/leadership.md`).
//!
//! Each fixture builds its bands directly (a clone of the world's first band, given an id, a size,
//! a belief anchor, a supply network, a culture layer, and where it matters a chief, a shared
//! origin and a tie) and runs `advance_tribes` on its own, one call per turn, so the score is
//! driven by exactly the inputs the rule reads. The score arithmetic is unit-pinned beside
//! `closeness`, and the identity arithmetic beside `resolve_matches`.
//!
//! With the shipped weights a pair of kin (origin 3) with identical culture (3) and a full tie (2)
//! scores 8 before any history; the threshold is 9, so it takes 5 turns of shared history (one
//! fifth, at 1/25 a turn) to close the gap.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::{Entity, With};

use core_sim::sim_state::{capture_sim_state, restore_sim_state};
use core_sim::{
    advance_tribes, reconcile_band_culture_layers, tile_rung_span, BandId, BandLeader,
    BandLeadership, BandName, BandSupplyMembership, BeliefRegistry, BranchId, CommandEventEntry,
    CommandEventKind, CommandEventLog, ConnectionKey, ConnectionLedger, CultureManager,
    CultureOwner, FactionId, FoundingLines, LadderConfigHandle, LeaderKind, PopulationCohort,
    ResidentBand, RungId, Sighting, SimulationConfig, SimulationTick, SupplyNetworkMembership,
    TileImprovementRegistry, TribeLeaderKind, TribeRegistry, CULTURE_TRAIT_AXES,
    CULTURE_TRAIT_SPAN,
};

/// Test bands' ids start well clear of any worldgen band.
const TEST_BAND_BASE: u64 = 9_000;
/// The turn the fixtures start counting from.
const FIXTURE_TURN: u64 = 20;
/// Two distinct belief anchors.
const PLACE_A: UVec2 = UVec2::new(3, 3);
const PLACE_B: UVec2 = UVec2::new(8, 8);
/// Two distinct supply networks.
const NET_1: u32 = 1;
const NET_2: u32 = 2;
/// A kin pair scores 8 with no history and needs a fifth of it: five local turns is not yet
/// enough under floating-point accumulation's guarantees, six always is.
const TURNS_TO_CLOSE_A_KIN_PAIR: usize = 6;
/// Enough local turns to fill any pair's history (25 at the shipped build rate).
const TURNS_TO_FILL_HISTORY: usize = 26;
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

/// Spawn test band number `n`: sized `size`, anchored at `anchor`, in network `net`, with its own
/// distinct origin and a culture layer flattened to zero (so test bands are identical unless a
/// test says otherwise).
fn band(app: &mut App, n: u64, size: u32, anchor: Option<UVec2>, net: u32) -> Entity {
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
    link(app, entity, net);
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
fn member(
    app: &mut App,
    n: u64,
    size: u32,
    anchor: Option<UVec2>,
    net: u32,
    origin: u64,
) -> Entity {
    let entity = band(app, n, size, anchor, net);
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

/// Give every pair among `ns` a full tie.
fn tie_all(app: &mut App, ns: &[u64]) {
    let mut ledger = app.world.resource_mut::<ConnectionLedger>();
    for a in ns {
        for b in ns {
            if a < b {
                ledger.insert_full_tie(
                    ConnectionKey::new(id(*a), id(*b)),
                    &Sighting::new(UVec2::ZERO, 1, "", FactionId(0)),
                    1,
                );
            }
        }
    }
}

fn link(app: &mut App, band: Entity, net: u32) {
    app.world
        .resource_mut::<SupplyNetworkMembership>()
        .restore_band(
            band,
            &BandSupplyMembership {
                network_id: net,
                ..BandSupplyMembership::default()
            },
        );
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

/// Run turns until a tribe exists (at most `TURNS_TO_FILL_HISTORY`), leaving the tick on the turn
/// it formed so that turn's events can be read.
fn until_formed(app: &mut App) {
    for _ in 0..TURNS_TO_FILL_HISTORY {
        turn(app);
        if !registry(app).is_empty() {
            return;
        }
    }
    panic!("no tribe formed");
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
    let one = member(&mut app, 1, 40, Some(PLACE_A), NET_1, 1);
    let two = member(&mut app, 2, 90, Some(PLACE_A), NET_1, 1);
    tie_all(&mut app, &[1, 2]);
    (app, one, two)
}

#[test]
fn a_fresh_split_is_no_tribe_on_the_split_turn_even_with_a_chief() {
    let (mut app, _, _) = kin_pair();
    turn(&mut app);
    assert!(registry(&app).is_empty(), "8 + a day of history < 9");
    assert!(events(&app, CommandEventKind::TribeFormed).is_empty());
    turns(&mut app, 2);
    assert!(registry(&app).is_empty());
}

#[test]
fn a_fresh_split_becomes_a_tribe_after_a_few_turns_of_shared_history() {
    let (mut app, _, _) = kin_pair();
    turns(&mut app, TURNS_TO_CLOSE_A_KIN_PAIR);
    let tribe = registry(&app).tribe_of(id(1)).expect("history closed it");
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
fn no_chief_no_tribe_however_high_the_score_and_it_forms_the_turn_one_is_crowned() {
    let mut app = spawn_world();
    band(&mut app, 1, 50, Some(PLACE_A), NET_1);
    let two = band(&mut app, 2, 50, Some(PLACE_A), NET_1);
    // Make them kin and tied so the score is high.
    app.world
        .get_mut::<PopulationCohort>(two)
        .unwrap()
        .founding_lines = FoundingLines::founded(id(1), 2);
    tie_all(&mut app, &[1, 2]);
    turns(&mut app, TURNS_TO_FILL_HISTORY + 10);
    assert!(registry(&app).shared_history(id(1), id(2)) >= 1.0 - 1e-9);
    assert!(
        registry(&app).is_empty(),
        "13 points of score, nobody to follow"
    );

    crown(&mut app, two, "Hale", LeaderKind::Chief);
    turn(&mut app);
    assert_eq!(registry(&app).tribe_of(id(1)).unwrap().leader.name, "Hale");
    assert_eq!(events(&app, CommandEventKind::TribeFormed).len(), 1);
}

#[test]
fn unrelated_but_alike_bands_need_most_of_the_history() {
    let mut app = spawn_world();
    // Distinct origins: 3 (likeness) + 2 (tie) + 5 x history; 9 needs 0.8 of it.
    member(&mut app, 1, 50, Some(PLACE_A), NET_1, 1);
    member(&mut app, 2, 50, Some(PLACE_A), NET_1, 2);
    tie_all(&mut app, &[1, 2]);
    turns(&mut app, 15);
    assert!(registry(&app).is_empty(), "15 turns is 0.6 of the history");
    turns(&mut app, 8);
    assert!(
        registry(&app).tribe_of(id(1)).is_some(),
        "23 turns is past 0.8"
    );
}

#[test]
fn unrelated_bands_with_no_tie_never_close() {
    let mut app = spawn_world();
    member(&mut app, 1, 50, Some(PLACE_A), NET_1, 1);
    member(&mut app, 2, 50, Some(PLACE_A), NET_1, 2);
    turns(&mut app, TURNS_TO_FILL_HISTORY * 3);
    assert!(registry(&app).is_empty(), "3 + 5 = 8 at the very most");
}

#[test]
fn a_less_alike_pair_takes_longer_to_close() {
    let (mut app, _, _) = kin_pair();
    // One axis a half range apart: likeness 0.5, so 3 + 1.5 + 2 = 6.5 and history must give 2.5.
    set_axis(&mut app, id(2), 0, CULTURE_TRAIT_SPAN);
    turns(&mut app, 10);
    assert!(registry(&app).is_empty(), "0.4 of the history is not 0.5");
    turns(&mut app, 5);
    assert!(registry(&app).tribe_of(id(1)).is_some(), "0.6 is");
}

#[test]
fn belief_on_the_shared_urn_closes_the_gap_sooner() {
    let (mut app, _, _) = kin_pair();
    app.world
        .resource_mut::<BeliefRegistry>()
        .add(PLACE_A, GREAT_URN);
    turn(&mut app);
    assert!(
        registry(&app).tribe_of(id(1)).is_some(),
        "8 + 0.2 history + ~1 belief clears 9 on the first turn"
    );
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
fn a_kept_monument_on_the_shared_urn_closes_a_fresh_split_at_once() {
    let (mut app, _, _) = kin_pair();
    raise_monument(&mut app, PLACE_A, 1.0);
    turn(&mut app);
    assert!(
        registry(&app).tribe_of(id(1)).is_some(),
        "8 + 0.2 history + monument 2 clears 9 on the first turn"
    );
}

#[test]
fn a_half_raised_monument_or_one_elsewhere_adds_nothing() {
    let (mut app, _, _) = kin_pair();
    raise_monument(&mut app, PLACE_A, 0.5);
    raise_monument(&mut app, PLACE_B, 1.0);
    turn(&mut app);
    assert!(
        registry(&app).is_empty(),
        "a half-raised monument is not held, and one off the urn is not theirs"
    );
}

#[test]
fn parting_fades_history_rather_than_resetting_it() {
    let (mut app, _, two) = kin_pair();
    turns(&mut app, TURNS_TO_FILL_HISTORY);
    let first = registry(&app).tribe_of(id(1)).unwrap().id;

    // Apart ten turns: the tribe is gone at once, the history is most of what it was.
    set_anchor(&mut app, two, Some(PLACE_B));
    turn(&mut app);
    assert!(registry(&app).is_empty());
    assert_eq!(events(&app, CommandEventKind::TribeDissolved).len(), 1);
    turns(&mut app, 9);
    let kept = registry(&app).shared_history(id(1), id(2));
    assert!((0.85..0.95).contains(&kept), "faded, not reset: {kept}");

    // Back together, the pair is close again on the first turn: a new tribe, a new id.
    set_anchor(&mut app, two, Some(PLACE_A));
    turn(&mut app);
    let again = registry(&app).tribe_of(id(1)).expect("still close");
    assert_ne!(again.id, first);
}

#[test]
fn apart_long_enough_the_history_is_gone_and_must_be_rebuilt() {
    let (mut app, _, two) = kin_pair();
    turns(&mut app, TURNS_TO_FILL_HISTORY);
    set_anchor(&mut app, two, Some(PLACE_B));
    turns(&mut app, 90);
    assert!(registry(&app).shared_history(id(1), id(2)) < 0.15);

    set_anchor(&mut app, two, Some(PLACE_A));
    turn(&mut app);
    assert!(registry(&app).is_empty(), "8.7 is under the line");
    turns(&mut app, 5);
    assert!(registry(&app).tribe_of(id(1)).is_some());
}

#[test]
fn nothing_forms_without_a_shared_place_a_network_or_an_anchor() {
    let mut app = spawn_world();
    member(&mut app, 1, 50, Some(PLACE_A), NET_1, 1);
    member(&mut app, 2, 50, Some(PLACE_B), NET_1, 1); // another place
    member(&mut app, 3, 50, Some(PLACE_A), 0, 1); // no network
    member(&mut app, 4, 50, None, NET_1, 1); // no anchor
    member(&mut app, 5, 50, Some(PLACE_A), NET_2, 1); // another network
    tie_all(&mut app, &[1, 2, 3, 4, 5]);
    turns(&mut app, TURNS_TO_FILL_HISTORY * 2);
    assert!(registry(&app).is_empty());
}

#[test]
fn a_third_band_joins_when_its_pairs_close_and_a_broken_link_leaves() {
    let (mut app, _, _) = kin_pair();
    let third = member(&mut app, 3, 50, Some(PLACE_A), NET_2, 1);
    tie_all(&mut app, &[1, 2, 3]);
    turns(&mut app, TURNS_TO_FILL_HISTORY);
    let tribe = registry(&app).tribe_of(id(1)).unwrap().id;
    assert!(registry(&app).tribe_of(id(3)).is_none());

    link(&mut app, third, NET_1);
    turns(&mut app, 2);
    assert!(
        registry(&app).tribe_of(id(3)).is_none(),
        "history not yet built"
    );
    turns(&mut app, TURNS_TO_CLOSE_A_KIN_PAIR);
    assert_eq!(registry(&app).tribe_of(id(3)).unwrap().id, tribe);

    link(&mut app, third, NET_2);
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
    let third = member(&mut app, 3, 50, Some(PLACE_A), NET_2, 1);
    tie_all(&mut app, &[1, 2, 3]);
    turns(&mut app, TURNS_TO_FILL_HISTORY);
    link(&mut app, third, NET_1);
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
        .map(|n| member(&mut app, n, sizes[n as usize - 1], Some(PLACE_A), NET_1, 1))
        .collect();
    tie_all(&mut app, &[1, 2, 3]);
    turns(&mut app, TURNS_TO_FILL_HISTORY);
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
fn a_network_split_keeps_the_tribe_on_the_majority_side_and_the_other_side_forms_its_own() {
    let mut app = spawn_world();
    let entities: Vec<Entity> = (1..=5)
        .map(|n| member(&mut app, n, 50, Some(PLACE_A), NET_1, 1))
        .collect();
    tie_all(&mut app, &[1, 2, 3, 4, 5]);
    turns(&mut app, TURNS_TO_FILL_HISTORY);
    let original = registry(&app).tribe_of(id(1)).unwrap().id;
    assert_eq!(registry(&app).tribe_of(id(1)).unwrap().members.len(), 5);

    link(&mut app, entities[3], NET_2);
    link(&mut app, entities[4], NET_2);
    turn(&mut app);
    assert_eq!(registry(&app).len(), 2);
    assert_eq!(registry(&app).tribe_of(id(1)).unwrap().id, original);
    assert_eq!(registry(&app).tribe_of(id(1)).unwrap().members.len(), 3);
    let other = registry(&app).tribe_of(id(4)).unwrap();
    assert_ne!(other.id, original);
    assert_eq!(other.members.len(), 2, "its pair kept its history");
    assert_eq!(events(&app, CommandEventKind::TribeFormed).len(), 1);
    assert_eq!(events(&app, CommandEventKind::TribeBandLeft).len(), 2);
}

#[test]
fn two_tribes_sharing_a_place_merge_into_the_one_holding_more_once_their_pairs_close() {
    let mut app = spawn_world();
    for n in 1..=2 {
        member(&mut app, n, 50, Some(PLACE_A), NET_1, 1);
    }
    let late: Vec<Entity> = (3..=5)
        .map(|n| member(&mut app, n, 50, Some(PLACE_A), NET_2, 1))
        .collect();
    tie_all(&mut app, &[1, 2, 3, 4, 5]);
    turns(&mut app, TURNS_TO_FILL_HISTORY);
    let small = registry(&app).tribe_of(id(1)).unwrap().id;
    let big = registry(&app).tribe_of(id(3)).unwrap().id;
    assert_eq!(registry(&app).len(), 2);

    for entity in &late {
        link(&mut app, *entity, NET_1);
    }
    turn(&mut app);
    assert_eq!(registry(&app).len(), 2, "cross pairs have no history yet");
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
fn the_registry_and_its_histories_round_trip_through_a_checkpoint() {
    let (mut app, _, _) = kin_pair();
    turns(&mut app, TURNS_TO_CLOSE_A_KIN_PAIR);
    let third = member(&mut app, 3, 50, Some(PLACE_A), NET_1, 1);
    let _ = third;
    tie_all(&mut app, &[1, 2, 3]);
    turns(&mut app, 3);
    let before = registry(&app).clone();
    assert_eq!(before.len(), 1);
    assert!(
        before.shared_history(id(1), id(3)) > 0.0,
        "a history in progress"
    );

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
