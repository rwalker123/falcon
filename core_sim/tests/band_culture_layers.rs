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

use core_sim::{
    build_headless_app, recapture_snapshot_in_place, scalar_from_f32, BandId,
    CultureCorruptionConfig, CultureLayerScope, CultureManager, CultureOwner, CultureTensionKind,
    InfluencerCultureResonance, SimulationTick, SnapshotHistory, CULTURE_TRAIT_AXES,
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
    let mut app = build_headless_app();
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

/// **The tile lookup is keyed on POSITION** — the Part 1 defect, asserted on the shipped snapshot
/// rather than on the manager, because keying at the call site is exactly what broke.
///
/// Both readers were asking for `CultureOwner(tile.entity)` while `attach_local` files layers under
/// `CultureOwner::from_tile`. The two key spaces are disjoint (`from_tile` always sets bit 63), so
/// every lookup missed: `tiles[].culture_layer` shipped as a uniform `0` and `culture_raster` as
/// all zeroes, on every frame of every game.
#[test]
fn the_snapshot_stamps_every_tile_with_its_culture_layer_and_fills_the_raster() {
    let mut app = build_headless_app();
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
