//! **Bands carry their own culture** (issue #407), and the tile layers are keyed by position.
//!
//! Four properties are pinned here, each of which failed before this arc:
//!
//! 1. the faction rollup averages the **band** layers, rather than silently falling through to the
//!    global layer (it read a map no band was ever in),
//! 2. a band re-homed on a new province **lags** toward it instead of snapping,
//! 3. a band that stays diverged past its hard threshold for its full trigger window raises a
//!    `SchismRisk` naming that band, and
//! 4. band layers survive a checkpoint round-trip.
//!
//! Plus the snapshot-level guard for the tile-key defect: `tiles[].culture_layer` and the culture
//! raster were both keyed on `tile.entity` while `attach_local` files layers under
//! `CultureOwner::from_tile`, so every lookup missed and both shipped empty on every frame.
//!
//! And the reconcile pass's own guard: a resident band is live whether or not its tile resolves, so
//! an unresolvable `current_tile` never routes the band through the stale sweep.
//!
//! Plus the founding seed (`attach_band_from_source`): a colony carries the culture of the band
//! that sent it, mints its own character offset, and parents on the province it landed in. The
//! fixtures stage a real divergence first, because every layer on a fresh manager is the baseline
//! and both candidate seeds would otherwise give the same answer.

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::{Entity, With};
use core_sim::{
    build_test_app, recapture_snapshot_in_place, reconcile_band_culture_layers, scalar_from_f32,
    seeded_modifiers_for_band, BandId, CultureCorruptionConfig, CultureLayerScope, CultureManager,
    CultureOwner, CultureTensionKind, InfluencerCultureResonance, PopulationCohort, ResidentBand,
    SimulationTick, SnapshotHistory, CULTURE_TRAIT_AXES,
};

/// The axis every test below writes; any single axis would do, the rollup is per-axis.
const AXIS: usize = 0;

/// A propagation config built from JSON, so the tests drive the *shipped* config path rather than
/// a constructor the sim does not use.
fn manager_from_json(
    band_elasticity: f32,
    band_soft: f32,
    band_hard: f32,
    band_soft_ticks: u16,
    band_hard_ticks: u16,
    band_amplitude: f32,
) -> CultureManager {
    let json = format!(
        r#"{{
          "culture": {{
            "propagation": {{
              "global": {{ "elasticity": 1.0, "soft_threshold": 9.0, "hard_threshold": 9.0,
                           "soft_trigger_ticks": 1, "hard_trigger_ticks": 1 }},
              "regional": {{ "elasticity": 1.0, "soft_threshold": 9.0, "hard_threshold": 9.0,
                             "soft_trigger_ticks": 1, "hard_trigger_ticks": 1 }},
              "local": {{ "elasticity": 0.4, "soft_threshold": 9.0, "hard_threshold": 9.0,
                          "soft_trigger_ticks": 1, "hard_trigger_ticks": 1 }},
              "band": {{ "elasticity": {band_elasticity}, "soft_threshold": {band_soft},
                         "hard_threshold": {band_hard}, "soft_trigger_ticks": {band_soft_ticks},
                         "hard_trigger_ticks": {band_hard_ticks} }},
              "band_character_amplitude": {band_amplitude},
              "resonance_response": 0.08
            }}
          }}
        }}"#
    );
    let config = CultureCorruptionConfig::from_json_str(&json).expect("test config parses");
    CultureManager::from_config(config.culture().propagation())
}

fn reconcile(manager: &mut CultureManager, tick: u64) {
    manager.reconcile(
        &SimulationTick(tick),
        &InfluencerCultureResonance::default(),
    );
}

fn band_axis(manager: &CultureManager, band: BandId) -> f32 {
    manager
        .band_layer_by_owner(CultureOwner::from_band(band))
        .expect("band layer exists")
        .traits
        .values()[AXIS]
        .to_f32()
}

fn region_axis(manager: &CultureManager, region_id: u32) -> f32 {
    manager
        .regional_layers()
        .find(|layer| layer.owner == CultureOwner::from_region(region_id))
        .expect("regional layer exists")
        .traits
        .values()[AXIS]
        .to_f32()
}

/// Force a band's resolved trait vector to a flat value on every axis.
fn set_band_values(manager: &mut CultureManager, band: BandId, value: f32) {
    let layer = manager
        .band_layer_mut_by_owner(CultureOwner::from_band(band))
        .expect("band layer exists");
    for idx in 0..CULTURE_TRAIT_AXES {
        layer.traits.update_value(idx, scalar_from_f32(value));
    }
}

/// **The rollup reads the bands, not the global layer.**
///
/// Asserting merely "non-zero" would pass on the silent fallback this replaced, so the expected
/// value is the population-weighted mean of two *different* band vectors — a number that matches
/// neither band, and is nowhere near the global layer's.
#[test]
fn the_faction_rollup_is_the_population_weighted_average_of_its_bands() {
    let mut manager = manager_from_json(0.2, 0.6, 1.2, 3, 5, 0.0);
    manager.ensure_global();
    let region = manager.upsert_regional(1);

    // A global layer deliberately far from both bands: if the rollup ever falls through, it shows.
    {
        let global = manager.global_layer_mut().expect("global layer");
        for idx in 0..CULTURE_TRAIT_AXES {
            global.traits.update_value(idx, scalar_from_f32(-2.0));
        }
    }

    let quiet = BandId(11);
    let loud = BandId(12);
    manager.attach_band(quiet, region);
    manager.attach_band(loud, region);
    set_band_values(&mut manager, quiet, 0.4);
    set_band_values(&mut manager, loud, 1.0);

    // 10 people at 0.4, 30 at 1.0 → 0.85.
    let average = manager.faction_trait_average(&[
        (CultureOwner::from_band(quiet), 10),
        (CultureOwner::from_band(loud), 30),
    ]);

    let expected = (0.4 * 10.0 + 1.0 * 30.0) / 40.0;
    assert!(
        (average[AXIS] - expected).abs() < 1e-4,
        "rollup should be the weighted mean of the two bands ({expected}), got {}",
        average[AXIS]
    );
    assert!(
        (average[AXIS] - 0.4).abs() > 1e-3 && (average[AXIS] - 1.0).abs() > 1e-3,
        "the mean must match neither band alone, or the test proves nothing"
    );
    assert!(
        (average[AXIS] + 2.0).abs() > 1.0,
        "the rollup must not read the global layer"
    );
}

/// **A migrating band lags.** Re-parenting keeps the culture the band arrived with and lets it
/// chase the new province at the band scope's elasticity — the property the province-parent choice
/// exists to produce. If `set_band_parent` ever reseeded traits, the first reconcile would land on
/// the new province exactly and this fails.
#[test]
fn a_band_re_parented_to_a_new_province_moves_toward_it_over_several_turns() {
    // Amplitude 0 so the band converges on its province exactly and the assertion is about lag
    // alone, not about the band's own character offset.
    let mut manager = manager_from_json(0.2, 9.0, 9.0, 99, 99, 0.0);
    manager.ensure_global();
    let home = manager.upsert_regional(1);
    let away = manager.upsert_regional(2);

    // Two provinces pinned to opposite corners of the axis (regional elasticity is 1.0, so they
    // land on their modifiers immediately and stay there).
    for (region_id, offset) in [(1u32, 1.0f32), (2, -1.0)] {
        let layer = manager
            .regional_layer_mut_by_region(region_id)
            .expect("regional layer exists");
        layer.traits.modifier_mut()[AXIS] = scalar_from_f32(offset);
    }

    let band = BandId(5);
    manager.attach_band(band, home);
    for tick in 1..=40 {
        reconcile(&mut manager, tick);
    }
    let settled = band_axis(&manager, band);
    let home_value = region_axis(&manager, 1);
    assert!(
        (settled - home_value).abs() < 0.05,
        "the band should have settled on its home province ({home_value}), got {settled}"
    );

    manager.set_band_parent(band, away);
    reconcile(&mut manager, 41);
    let after_one = band_axis(&manager, band);
    let away_value = region_axis(&manager, 2);
    assert!(
        (after_one - away_value).abs() < (settled - away_value).abs(),
        "one turn should move the band toward its new province"
    );
    assert!(
        (after_one - away_value).abs() > 0.5,
        "…but must NOT arrive: a move is supposed to lag, got {after_one} against {away_value}"
    );

    for tick in 42..=100 {
        reconcile(&mut manager, tick);
    }
    let arrived = band_axis(&manager, band);
    assert!(
        (arrived - away_value).abs() < 0.05,
        "given enough turns the band assimilates to its new province ({away_value}), got {arrived}"
    );
}

/// **A band that stays diverged schisms**, and the record names the band.
#[test]
fn a_band_held_past_its_hard_threshold_raises_a_schism_naming_that_band() {
    const HARD_TRIGGER_TICKS: u16 = 4;
    // Elasticity 1.0 so the band lands on `parent + modifier` on the first reconcile and the only
    // variable under test is the trigger window.
    let mut manager = manager_from_json(1.0, 0.2, 0.5, 1, HARD_TRIGGER_TICKS, 0.0);
    manager.ensure_global();
    let region = manager.upsert_regional(3);

    let band = BandId(9);
    manager.attach_band(band, region);
    {
        let layer = manager
            .band_layer_mut_by_owner(CultureOwner::from_band(band))
            .expect("band layer exists");
        layer.traits.modifier_mut()[AXIS] = scalar_from_f32(1.5);
    }

    for tick in 1..HARD_TRIGGER_TICKS as u64 {
        reconcile(&mut manager, tick);
        assert!(
            manager
                .take_tension_events()
                .iter()
                .all(|record| record.kind != CultureTensionKind::SchismRisk),
            "a schism must wait for the band scope's full trigger window"
        );
    }

    reconcile(&mut manager, HARD_TRIGGER_TICKS as u64);
    let schism = manager
        .take_tension_events()
        .into_iter()
        .find(|record| record.kind == CultureTensionKind::SchismRisk)
        .expect("the band should schism once it has been diverged for the full window");
    assert_eq!(schism.scope, CultureLayerScope::Band);
    assert_eq!(schism.owner, CultureOwner::from_band(band));
}

/// **Band layers ride the checkpoint.** They are held by `CultureManager`, not by a resource or a
/// component of their own, so this is the only thing standing between a rollback and a faction
/// whose every band forgot its culture.
#[test]
fn band_layers_survive_a_checkpoint_round_trip() {
    let mut manager = manager_from_json(0.2, 0.6, 1.2, 3, 5, 0.2);
    manager.ensure_global();
    let region = manager.upsert_regional(1);

    let band = BandId(21);
    let layer_id = manager.attach_band(band, region);
    set_band_values(&mut manager, band, 0.7);

    let checkpoint = manager.checkpoint();
    manager.detach_band(band);
    assert!(
        manager
            .band_layer_by_owner(CultureOwner::from_band(band))
            .is_none(),
        "the layer is gone before the restore, or the test proves nothing"
    );

    manager.restore_checkpoint(&checkpoint);
    let restored = manager
        .band_layer_by_owner(CultureOwner::from_band(band))
        .expect("the band layer comes back with the checkpoint");
    assert_eq!(restored.id, layer_id);
    assert_eq!(restored.scope, CultureLayerScope::Band);
    assert_eq!(restored.parent, Some(region));
    assert!((restored.traits.values()[AXIS].to_f32() - 0.7).abs() < 1e-4);
}

/// **A colony carries the culture of the band that sent it, not of the ground it landed on.**
///
/// The parent is **staged into a real divergence first** — its values are pushed a full unit clear
/// of both provinces — because on a fresh manager every layer is still neutral and a colony seeded
/// from either source would land on the same numbers, so the test would pass without asserting
/// anything. The destination province is pinned in the opposite direction for the same reason.
///
/// The lag half is the point of parenting on the destination: the colony *starts* as its parent and
/// then chases the locals at the band scope's elasticity, exactly as a band that walked there does.
#[test]
fn a_colony_seeded_from_its_parent_starts_as_the_parent_and_then_chases_its_new_province() {
    const PARENT_VALUE: f32 = 1.5;
    let mut manager = manager_from_json(0.2, 9.0, 9.0, 99, 99, 0.0);
    manager.ensure_global();
    let home = manager.upsert_regional(1);
    let destination = manager.upsert_regional(2);
    manager
        .regional_layer_mut_by_region(2)
        .expect("regional layer exists")
        .traits
        .modifier_mut()[AXIS] = scalar_from_f32(-1.0);
    reconcile(&mut manager, 1);

    let parent = BandId(31);
    manager.attach_band(parent, home);
    set_band_values(&mut manager, parent, PARENT_VALUE);

    let destination_value = region_axis(&manager, 2);
    assert!(
        (PARENT_VALUE - destination_value).abs() > 1.0,
        "the fixture must stage a real divergence between the parent ({PARENT_VALUE}) and the \
         destination province ({destination_value}), or a colony seeded from either source looks \
         the same and the test is vacuous"
    );

    let colony = BandId(32);
    manager.attach_band_from_source(colony, destination, parent);

    let seeded = band_axis(&manager, colony);
    assert!(
        (seeded - PARENT_VALUE).abs() < 1e-3,
        "the colony opens as its parent ({PARENT_VALUE}), got {seeded}"
    );
    assert!(
        (seeded - destination_value).abs() > 1.0,
        "…and NOT as the province it landed in ({destination_value})"
    );
    assert_eq!(
        manager
            .band_layer_by_owner(CultureOwner::from_band(colony))
            .expect("the colony owns a layer")
            .parent,
        Some(destination),
        "the colony is parented on the province it was founded in, so it chases the locals"
    );

    reconcile(&mut manager, 2);
    let after_one = band_axis(&manager, colony);
    assert!(
        (after_one - destination_value).abs() < (seeded - destination_value).abs(),
        "one turn moves the colony toward its new province"
    );
    assert!(
        (after_one - destination_value).abs() > 0.5,
        "…but must not arrive: a founding lags exactly as a migration does, got {after_one} \
         against {destination_value}"
    );
}

/// **A colony is not a clone of its parent.** It inherits the traits and mints its *own* character
/// offset — inheriting that too would fix the two bands together forever, since the offset is the
/// only reason any two bands diverge at all.
#[test]
fn a_colony_carries_its_own_character_offset() {
    const AMPLITUDE: f32 = 0.2;
    let mut manager = manager_from_json(1.0, 9.0, 9.0, 99, 99, AMPLITUDE);
    manager.ensure_global();
    let region = manager.upsert_regional(1);

    let parent = BandId(41);
    let colony = BandId(42);
    manager.attach_band(parent, region);
    set_band_values(&mut manager, parent, 0.9);
    manager.attach_band_from_source(colony, region, parent);

    let parent_modifier = *manager
        .band_layer_by_owner(CultureOwner::from_band(parent))
        .expect("parent layer")
        .traits
        .modifier();
    let colony_modifier = *manager
        .band_layer_by_owner(CultureOwner::from_band(colony))
        .expect("colony layer")
        .traits
        .modifier();
    let expected = seeded_modifiers_for_band(colony, AMPLITUDE);
    for idx in 0..CULTURE_TRAIT_AXES {
        assert!(
            (colony_modifier[idx].to_f32() - expected[idx].to_f32()).abs() < 1e-4,
            "the colony's offset is its own seeded character, axis {idx}"
        );
    }
    let spread: f32 = (0..CULTURE_TRAIT_AXES)
        .map(|idx| (colony_modifier[idx].to_f32() - parent_modifier[idx].to_f32()).abs())
        .fold(0.0, f32::max);
    assert!(
        spread > 1e-3,
        "the colony must not inherit the parent's offset, or the two are the same band forever \
         (max axis gap {spread})"
    );
}

/// **A source with no layer falls back to the province** — today's behaviour, and the honest answer
/// when the home band cannot be resolved at all.
#[test]
fn a_colony_whose_source_owns_no_layer_is_seeded_from_the_province() {
    let mut manager = manager_from_json(0.2, 9.0, 9.0, 99, 99, 0.0);
    manager.ensure_global();
    let region = manager.upsert_regional(1);
    manager
        .regional_layer_mut_by_region(1)
        .expect("regional layer exists")
        .traits
        .modifier_mut()[AXIS] = scalar_from_f32(0.8);
    reconcile(&mut manager, 1);

    let colony = BandId(51);
    manager.attach_band_from_source(colony, region, BandId(52));

    let region_value = region_axis(&manager, 1);
    assert!(
        region_value.abs() > 0.1,
        "the province must be somewhere other than neutral, or the fallback is untestable"
    );
    let seeded = band_axis(&manager, colony);
    assert!(
        (seeded - region_value).abs() < 1e-3,
        "with no source layer the colony is seeded from its province ({region_value}), got {seeded}"
    );
}

/// **A band's character is not the province's.** Two bands in one province must resolve to
/// different vectors, or the rollup degenerates into an average of provinces and nothing ever
/// schisms.
#[test]
fn two_bands_in_one_province_drift_apart() {
    let mut manager = manager_from_json(1.0, 9.0, 9.0, 99, 99, 0.2);
    manager.ensure_global();
    let region = manager.upsert_regional(1);

    let first = BandId(1);
    let second = BandId(2);
    manager.attach_band(first, region);
    manager.attach_band(second, region);
    for tick in 1..=10 {
        reconcile(&mut manager, tick);
    }

    let a = manager
        .band_layer_by_owner(CultureOwner::from_band(first))
        .expect("band layer")
        .traits
        .values();
    let b = manager
        .band_layer_by_owner(CultureOwner::from_band(second))
        .expect("band layer")
        .traits
        .values();
    let spread: f32 = (0..CULTURE_TRAIT_AXES)
        .map(|idx| (a[idx].to_f32() - b[idx].to_f32()).abs())
        .fold(0.0, f32::max);
    assert!(
        spread > 0.05,
        "two bands sharing a province must not be the same culture (max axis gap {spread})"
    );
}

/// **Worldgen's bands own layers by the time the first snapshot is captured.**
#[test]
fn resident_bands_own_culture_layers_after_the_first_update() {
    let mut app = build_test_app();
    app.update();

    let manager = app.world.resource::<CultureManager>();
    let band_layers: Vec<_> = manager.band_layers().collect();
    assert!(
        !band_layers.is_empty(),
        "the starting bands should have been given culture layers in the Influence stage"
    );
    for layer in band_layers {
        assert_eq!(layer.scope, CultureLayerScope::Band);
        assert!(
            layer.parent.is_some(),
            "a band layer is always parented to the province it stands in"
        );
    }
}

/// **A band's liveness does not depend on its tile resolving.** `reconcile_band_culture_layers`
/// builds the live set from the resident-band query and then sweeps every layer whose band is not
/// in it, so a band skipped because its `current_tile` no longer resolves reads to the sweep as a
/// *dead* band and is detached. That is silent state loss rather than a skipped turn: `attach_band`
/// reseeds from the parent province, so the band returns the following turn with its accumulated
/// drift, divergence and trigger timers replaced by a fresh layer.
///
/// The assertion is therefore on the layer's **identity and trait value**, not on its existence —
/// a detach-then-reattach leaves a layer standing, and only the numbers show it is not the same
/// culture. The marker value is set a full unit away from the province the band would be reseeded
/// from, so the two outcomes cannot be confused.
#[test]
fn a_band_whose_tile_cannot_be_resolved_keeps_its_layer_and_its_traits() {
    let mut app = build_test_app();
    app.update();

    let (band_entity, band) = {
        let mut bands = app
            .world
            .query_filtered::<(Entity, &BandId), With<ResidentBand>>();
        let (entity, band) = bands
            .iter(&app.world)
            .next()
            .expect("worldgen spawns at least one resident band");
        (entity, *band)
    };

    let (layer_id, parent, marker) = {
        let manager = app.world.resource::<CultureManager>();
        let layer = manager
            .band_layer_by_owner(CultureOwner::from_band(band))
            .expect("the band owns a culture layer after the first update");
        let parent = layer
            .parent
            .expect("a band layer is parented to the province it stands in");
        let parent_axis = manager
            .regional_layers()
            .find(|regional| regional.id == parent)
            .expect("the parent province layer exists")
            .traits
            .values()[AXIS]
            .to_f32();
        (layer.id, parent, parent_axis + 1.0)
    };
    {
        let mut manager = app.world.resource_mut::<CultureManager>();
        manager
            .band_layer_mut_by_owner(CultureOwner::from_band(band))
            .expect("the band owns a culture layer")
            .traits
            .update_value(AXIS, scalar_from_f32(marker));
    }

    // Point the band at an entity that is not a tile — what a despawned tile leaves behind.
    let orphan = app.world.spawn_empty().id();
    app.world
        .entity_mut(band_entity)
        .get_mut::<PopulationCohort>()
        .expect("a resident band carries a cohort")
        .current_tile = orphan;

    app.world.run_system_once(reconcile_band_culture_layers);

    let manager = app.world.resource::<CultureManager>();
    let layer = manager
        .band_layer_by_owner(CultureOwner::from_band(band))
        .expect(
            "a band whose tile failed to resolve keeps its layer — the stale sweep must not \
                 read an unresolvable tile as a dead band",
        );
    assert_eq!(
        layer.id, layer_id,
        "the layer must be the same one, not a replacement from a detach-then-reattach"
    );
    assert_eq!(
        layer.parent,
        Some(parent),
        "a band that was not re-homed this turn keeps the province it was parented to"
    );
    let held = layer.traits.values()[AXIS].to_f32();
    assert!(
        (held - marker).abs() < 1e-3,
        "the band's accumulated traits must survive untouched (expected {marker}, got {held}); \
         a value near its province means the layer was reseeded"
    );
}

/// **The tile lookup is keyed on POSITION** — the Part 1 defect, asserted on the shipped snapshot
/// rather than on the manager, because keying at the call site is exactly what broke.
///
/// Both readers were asking for `CultureOwner(tile.entity)` while `attach_local` files layers under
/// `CultureOwner::from_tile`. The two key spaces are disjoint (`from_tile` always sets bit 63), so
/// every lookup missed: `tiles[].culture_layer` shipped as a uniform `0` and `culture_raster` as
/// all zeroes, on every frame of every game.
#[test]
fn the_snapshot_stamps_every_tile_with_its_culture_layer_and_fills_the_raster() {
    let mut app = build_test_app();
    app.update();
    recapture_snapshot_in_place(&mut app.world);

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;

    assert!(!snapshot.tiles.is_empty(), "the world has tiles");
    let unstamped = snapshot
        .tiles
        .iter()
        .filter(|tile| tile.culture_layer == 0)
        .count();
    assert_eq!(
        unstamped,
        0,
        "every tile owns a local culture layer, so none may ship with culture_layer == 0 \
         ({unstamped} of {} did)",
        snapshot.tiles.len()
    );

    let raster = &snapshot.culture_raster;
    assert!(
        raster.samples.iter().any(|sample| *sample != 0),
        "the culture raster must carry at least one non-zero divergence sample"
    );
    assert_eq!(
        raster.samples.len(),
        (raster.width as usize) * (raster.height as usize),
        "the raster is a full grid"
    );
}
