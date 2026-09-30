//! **A faction's name is identity, and the sim owns it** — the band-name rule, one level up
//! (`.claude/rules/core_sim/band-names.md` → "Faction names").
//!
//! Minted by worldgen from the roster and the map seed (one permutation per world, so two factions
//! never share a name), carried by the checkpoint so a save wins over a later pool edit, and
//! published world-visible as `CampaignSection.factionNames`. Event labels say `Faction N` —
//! pinned by `defection.rs`, whose migration and party lines assert the `Faction` spelling.

use std::collections::HashSet;

use bevy::prelude::*;

mod faction_support;

use core_sim::{
    publish_baseline_snapshot, run_turn, FactionId, FactionNameCatalog, FactionNames,
    FactionRegistry, SimulationConfig, SnapshotHistory, ViewerFaction,
};
use faction_support::{world_with, HOME, ONE_RIVAL};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

/// A roster with several rivals, so "every faction" is more than a pair.
const SEVERAL_RIVALS: u32 = 3;

fn names(app: &App) -> FactionNames {
    app.world.resource::<FactionNames>().clone()
}

fn roster(app: &App) -> Vec<FactionId> {
    app.world.resource::<FactionRegistry>().factions().to_vec()
}

#[test]
fn every_faction_is_named_from_the_seed_and_no_two_share_a_name() {
    let app = world_with(SEVERAL_RIVALS, |_| {});
    let minted = names(&app);
    let roster = roster(&app);
    assert_eq!(
        minted.len(),
        roster.len(),
        "one name per faction on the roster"
    );
    let seed = app.world.resource::<SimulationConfig>().map_seed;
    assert_eq!(
        minted,
        FactionNames::mint(&roster, seed, &FactionNameCatalog::builtin()),
        "the names are a pure function of the roster and the map seed"
    );
    let distinct: HashSet<&str> = minted.iter().map(|(_, name)| name).collect();
    assert_eq!(distinct.len(), roster.len(), "no two factions share a name");
    assert!(minted.iter().all(|(_, name)| !name.is_empty()));
}

#[test]
fn a_roster_larger_than_the_pool_is_still_all_distinct() {
    let catalog = FactionNameCatalog::builtin();
    let roster: Vec<FactionId> = (0..(catalog.len() as u32 * 2 + 1)).map(FactionId).collect();
    let minted = FactionNames::mint(&roster, 7, &catalog);
    let distinct: HashSet<&str> = minted.iter().map(|(_, name)| name).collect();
    assert_eq!(distinct.len(), roster.len());
}

#[test]
fn the_same_seed_and_roster_rebuild_the_same_names() {
    // What a `ResetMap` with a pinned seed does: a fresh app, the same roster, the same seed. Band
    // names follow the same rule — both are minted by worldgen from the seed — so a re-rolled seed
    // renames both, and a pinned one renames neither.
    let seed_of = |app: &App| app.world.resource::<SimulationConfig>().map_seed;
    let first = world_with(ONE_RIVAL, |_| {});
    let seed = seed_of(&first);
    let second = world_with(ONE_RIVAL, |config| config.map_seed = seed);
    assert_eq!(seed_of(&second), seed, "fixture: the seed is pinned");
    assert_eq!(names(&first), names(&second));
}

#[test]
fn a_save_keeps_its_names_even_when_the_pool_would_now_say_otherwise() {
    let mut app = world_with(ONE_RIVAL, |_| {});
    run_turn(&mut app);
    // Stand in for a pool edit after the save: the world carries a name the pool never minted.
    let mut edited = names(&app);
    let renamed = FactionNames::mint(&roster(&app), 0, &FactionNameCatalog::builtin());
    assert_ne!(
        renamed, edited,
        "fixture: a different seed mints different names"
    );
    edited = renamed;
    app.world.insert_resource(edited.clone());

    let blob = core_sim::save::encode_save(&app.world).expect("the world encodes");
    let (loaded, _) = core_sim::save::load_save(&blob).expect("the save loads");
    assert_eq!(
        loaded.world.resource::<FactionNames>(),
        &edited,
        "the save wins: a load restores the names it carried, never re-derives them"
    );
}

#[test]
fn every_factions_name_is_on_the_encoded_snapshot() {
    let mut app = world_with(SEVERAL_RIVALS, |_| {});
    app.world.insert_resource(ViewerFaction(HOME));
    publish_baseline_snapshot(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let rows = envelope
        .payload_as_snapshot()
        .expect("a snapshot payload")
        .campaign()
        .and_then(|section| section.factionNames())
        .expect("the campaign section carries factionNames");
    let published: Vec<(u32, String)> = rows
        .iter()
        .map(|row| (row.faction(), row.name().unwrap_or_default().to_string()))
        .collect();
    let expected: Vec<(u32, String)> = names(&app)
        .iter()
        .map(|(faction, name)| (faction.0, name.to_string()))
        .collect();
    assert_eq!(
        published, expected,
        "every faction, world-visible — not only the viewer's"
    );
    assert_eq!(published.len(), roster(&app).len());
}
