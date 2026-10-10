//! **A purist band that diverges too far splits off** (#702).
//!
//! The trigger is the band scope's existing `SchismRisk` (the hard threshold held for
//! `hard_trigger_ticks`). The band splits only if its Syncretic<->Purist value is above
//! `culture.split_min_purist` AND its people has another resident band. The arms stage a layer one
//! tick short of the trigger and run the real `reconcile_culture_layers` then
//! `advance_culture_splits`.

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use core_sim::{
    advance_culture_splits, reconcile_band_culture_layers, reconcile_culture_layers,
    scalar_from_f32, split_band_from_parent, BandId, CommandEventKind, CommandEventLog,
    ConnectionLedger, CultureCorruptionConfigHandle, CultureManager, CultureOwner,
    CultureTraitAxis, FactionId, FactionRegistry, PopulationCohort, ResidentBand, Scalar,
    SettleConfig,
};

mod faction_support;
use faction_support::{one_faction_world, HOME};

const SPLINTER_WORKERS: u32 = 3;
/// Far past the band scope's hard threshold (1.2), on an axis the arms do not otherwise touch.
const STRAIN_AXIS: usize = 0;
const STRAIN: f32 = 5.0;
const PURIST: f32 = 1.0;
const ACCEPTING: f32 = -1.0;
const CULTURE_CAUSE: &str = "cause=culture";

struct Fixture {
    app: App,
    strained: BandId,
    strained_entity: Entity,
}

fn entity_of(app: &mut App, band: BandId) -> Entity {
    app.world
        .query::<(Entity, &BandId)>()
        .iter(&app.world)
        .find(|(_, id)| **id == band)
        .map(|(entity, _)| entity)
        .unwrap()
}

/// A home band plus a sibling of the same people; the SIBLING is the strained one. With
/// `sole`, every other band is despawned so the strained band is its people's only one.
fn fixture(purist: f32, sole: bool) -> Fixture {
    let mut app = one_faction_world();
    let (home_entity, _) = app
        .world
        .query_filtered::<(Entity, &BandId), With<ResidentBand>>()
        .iter(&app.world)
        .map(|(e, b)| (e, *b))
        .min_by_key(|(_, b)| *b)
        .unwrap();
    let split = split_band_from_parent(
        &mut app.world,
        home_entity,
        SPLINTER_WORKERS,
        &SettleConfig {
            min_founding_workers: 1,
            parent_min_workers: 0,
        },
    )
    .expect("the staged band can split");
    let strained = split.band;
    let strained_entity = entity_of(&mut app, strained);
    if sole {
        let others: Vec<Entity> = app
            .world
            .query_filtered::<(Entity, &BandId), With<ResidentBand>>()
            .iter(&app.world)
            .filter(|(_, id)| **id != strained)
            .map(|(e, _)| e)
            .collect();
        for other in others {
            app.world.despawn(other);
        }
    }
    app.world.insert_resource(ConnectionLedger::default());
    app.world.run_system_once(reconcile_band_culture_layers);
    {
        let mut manager = app.world.resource_mut::<CultureManager>();
        let layer = manager
            .band_layer_mut_by_owner(CultureOwner::from_band(strained))
            .expect("the band has a layer");
        let purist_axis = CultureTraitAxis::SyncreticPurist.index();
        for (axis, value) in [(STRAIN_AXIS, STRAIN), (purist_axis, purist)] {
            layer.traits.update_value(axis, scalar_from_f32(value));
            layer.traits.modifier_mut()[axis] = scalar_from_f32(value);
        }
        // One tick short of the schism trigger: this turn's reconcile tips it.
        layer.divergence.ticks_above_hard = layer.divergence.hard_trigger_ticks - 1;
    }
    Fixture {
        app,
        strained,
        strained_entity,
    }
}

fn set_lever(app: &mut App, min_purist: f32) {
    let json = format!(r#"{{"culture":{{"split_min_purist":{min_purist}}}}}"#);
    app.world
        .resource_mut::<CultureCorruptionConfigHandle>()
        .replace_from_json(&json)
        .expect("the lever parses");
}

fn culture_turn(app: &mut App) {
    app.world.run_system_once(reconcile_culture_layers);
    app.world.run_system_once(advance_culture_splits);
}

fn faction_of(app: &App, entity: Entity) -> FactionId {
    app.world.get::<PopulationCohort>(entity).unwrap().faction
}

fn broke_away(app: &App) -> Vec<(FactionId, String, String)> {
    app.world
        .resource::<CommandEventLog>()
        .iter()
        .filter(|entry| entry.kind == CommandEventKind::BandBrokeAway)
        .map(|entry| {
            (
                entry.faction,
                entry.label.clone(),
                entry.detail.clone().unwrap_or_default(),
            )
        })
        .collect()
}

#[test]
fn a_purist_band_held_past_the_hard_threshold_splits_off_and_both_peoples_are_told() {
    let mut fx = fixture(PURIST, false);
    let roster_before = fx.app.world.resource::<FactionRegistry>().factions().len();
    fx.app
        .world
        .get_mut::<PopulationCohort>(fx.strained_entity)
        .unwrap()
        .grievance = scalar_from_f32(0.7);
    let layer_before = *fx
        .app
        .world
        .resource::<CultureManager>()
        .band_layer_by_owner(CultureOwner::from_band(fx.strained))
        .unwrap()
        .traits
        .modifier();

    culture_turn(&mut fx.app);

    let registry = fx.app.world.resource::<FactionRegistry>();
    assert_eq!(registry.factions().len(), roster_before + 1);
    let new_people = faction_of(&fx.app, fx.strained_entity);
    assert_ne!(new_people, HOME);
    assert!(registry.is_ai(new_people));
    assert_eq!(
        fx.app
            .world
            .get::<PopulationCohort>(fx.strained_entity)
            .unwrap()
            .grievance,
        Scalar::zero()
    );
    let told = broke_away(&fx.app);
    assert_eq!(told.len(), 2, "one row per people");
    assert!(told
        .iter()
        .all(|(_, _, detail)| detail.contains(CULTURE_CAUSE)));
    assert!(told
        .iter()
        .any(|(f, _, d)| *f == HOME && d.contains("side=lost")));
    assert!(told
        .iter()
        .any(|(f, _, d)| *f == new_people && d.contains("side=gained")));
    // Its culture layer is as it was (the band keeps its character).
    let layer_after = *fx
        .app
        .world
        .resource::<CultureManager>()
        .band_layer_by_owner(CultureOwner::from_band(fx.strained))
        .unwrap()
        .traits
        .modifier();
    assert_eq!(layer_before, layer_after);

    // A just-split band is its people's only one: a second held schism cannot re-split it.
    {
        let mut manager = fx.app.world.resource_mut::<CultureManager>();
        let layer = manager
            .band_layer_mut_by_owner(CultureOwner::from_band(fx.strained))
            .unwrap();
        layer.divergence.ticks_above_hard = layer.divergence.hard_trigger_ticks - 1;
    }
    culture_turn(&mut fx.app);
    assert_eq!(
        fx.app.world.resource::<FactionRegistry>().factions().len(),
        roster_before + 1
    );
}

#[test]
fn an_accepting_band_in_the_same_situation_stays() {
    let mut fx = fixture(ACCEPTING, false);
    let roster_before = fx.app.world.resource::<FactionRegistry>().factions().len();
    culture_turn(&mut fx.app);
    assert_eq!(faction_of(&fx.app, fx.strained_entity), HOME);
    assert_eq!(
        fx.app.world.resource::<FactionRegistry>().factions().len(),
        roster_before
    );
    assert!(broke_away(&fx.app).is_empty());
}

#[test]
fn a_purist_sole_band_has_nothing_to_break_away_from() {
    let mut fx = fixture(PURIST, true);
    culture_turn(&mut fx.app);
    assert_eq!(faction_of(&fx.app, fx.strained_entity), HOME);
    assert!(broke_away(&fx.app).is_empty());
}

#[test]
fn the_split_min_purist_lever_moves_the_line() {
    // Purist 1.0 is under a lever of 2.0: the band is "accepting" by that line.
    let mut fx = fixture(PURIST, false);
    set_lever(&mut fx.app, 2.0);
    culture_turn(&mut fx.app);
    assert_eq!(faction_of(&fx.app, fx.strained_entity), HOME);

    // And above a lever of 0.5 it splits.
    let mut fx = fixture(PURIST, false);
    set_lever(&mut fx.app, 0.5);
    culture_turn(&mut fx.app);
    assert_ne!(faction_of(&fx.app, fx.strained_entity), HOME);
}

#[test]
fn a_band_another_system_already_moved_this_turn_is_skipped() {
    let mut fx = fixture(PURIST, false);
    fx.app.world.run_system_once(reconcile_culture_layers);
    // Independence (or a defection) moved it to another people before the split runs.
    let elsewhere = FactionId(HOME.0 + 7);
    fx.app
        .world
        .get_mut::<PopulationCohort>(fx.strained_entity)
        .unwrap()
        .faction = elsewhere;
    let roster_before = fx.app.world.resource::<FactionRegistry>().factions().len();
    fx.app.world.run_system_once(advance_culture_splits);
    assert_eq!(faction_of(&fx.app, fx.strained_entity), elsewhere);
    assert_eq!(
        fx.app.world.resource::<FactionRegistry>().factions().len(),
        roster_before
    );
    assert!(broke_away(&fx.app).is_empty());
}

#[test]
fn a_negative_or_non_finite_lever_is_refused_at_load() {
    use core_sim::CultureCorruptionConfig;
    for bad in [
        r#"{"culture":{"contact_drift":{"rate":-0.1}}}"#,
        r#"{"culture":{"contact_drift":{"rate":1e999}}}"#,
    ] {
        assert!(
            CultureCorruptionConfig::from_json_str(bad).is_err(),
            "{bad}"
        );
    }
    let zero =
        CultureCorruptionConfig::from_json_str(r#"{"culture":{"contact_drift":{"rate":0}}}"#)
            .unwrap();
    assert_eq!(zero.culture().contact_drift().rate(), 0.0);
    let shipped =
        CultureCorruptionConfig::from_json_str(core_sim::BUILTIN_CULTURE_CORRUPTION_CONFIG)
            .unwrap();
    assert_eq!(shipped.culture().contact_drift().rate(), 0.014);
    assert_eq!(shipped.culture().split_min_purist(), 0.0);
}
