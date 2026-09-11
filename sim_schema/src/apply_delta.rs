//! `WorldSnapshot::apply_delta` — the consumer's half of delta streaming, written against the
//! PRODUCER (`core_sim/src/snapshot/mod.rs` `diff_indexed` / `diff_whole` / `diff_appended`, and
//! the assembly in `capture.rs`). One rule per diff shape, and every `WorldDelta` field handled by
//! exactly one of them: the delta is destructured **exhaustively**, so a field appended to
//! `WorldDelta` fails to compile here until it has a rule, and a field with no rule cannot
//! silently drift.
//!
//! | Shape | Producer | Rule |
//! |---|---|---|
//! | header | cloned wholesale | `self.header = delta.header` |
//! | `fog_enabled` | carried every frame, never diffed | always copied |
//! | keyed (`Vec<T>` + `removed_*`) | `diff_indexed`: changed rows, then vanished keys; empty = unchanged | upsert by key in place, append new keys in delta order, then delete the removed keys |
//! | keyed, no removal list | `diff_indexed` whose removal side is never read | upsert only, never shrinks |
//! | whole (`Option<T>`) | `diff_whole`: `Some` when changed or restated, `None` otherwise | `Some(v)` replaces wholesale — `Some(vec![])` CLEARS — `None` leaves alone |
//! | `start_marker` | `Option<Option<_>>` flattened | `Some` sets; `None` is both "unchanged" and "cleared" and leaves alone |
//! | `command_events` | `diff_appended`: rows above the seat's cursor | append rows whose `seq` is new, then trim to the retention window by TICK |
//!
//! **The gate comes first and refuses without touching anything.** A delta applies only to the
//! frame it names (`base_frame_seq == self.header.frame_seq`) from the world it came from
//! (`world_epoch`) — the client's `WorldCache::accepts`. The event feed is append-only, so a delta
//! applied against the wrong base loses history with no error anywhere; a refused delta is the
//! signal to ask for a full frame.
//!
//! **Applying a restatement is idempotent.** A mid-tick recapture publishes under
//! `Baseline::Hold`, so its delta re-carries rows the previous held frame already carried; every
//! rule above lands on the same state when given the same row twice.

use crate::state::campaign::CommandEventState;
use crate::state::knowledge::KnowledgeLedgerEntryState;
use crate::world::{WorldDelta, WorldSnapshot};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// Why a delta could not be merged: it does not describe the frame this snapshot holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ApplyDeltaError {
    /// The delta names a base frame other than the one this snapshot is.
    #[error("the delta applies to frame {got}, but this snapshot is frame {expected}")]
    BaseMismatch { expected: u64, got: u64 },
    /// The delta comes from a different build of the world.
    #[error("the delta is from world epoch {got}, but this snapshot is from epoch {expected}")]
    WorldEpochMismatch { expected: u32, got: u32 },
}

impl WorldSnapshot {
    /// Merge `delta` into `self`, making `self` the snapshot the server captured when it published
    /// `delta`. Refuses — leaving `self` untouched — unless the delta names this snapshot's frame
    /// and world.
    pub fn apply_delta(&mut self, delta: &WorldDelta) -> Result<(), ApplyDeltaError> {
        if delta.header.world_epoch != self.header.world_epoch {
            return Err(ApplyDeltaError::WorldEpochMismatch {
                expected: self.header.world_epoch,
                got: delta.header.world_epoch,
            });
        }
        if delta.header.base_frame_seq != self.header.frame_seq {
            return Err(ApplyDeltaError::BaseMismatch {
                expected: self.header.frame_seq,
                got: delta.header.base_frame_seq,
            });
        }

        // Exhaustive on purpose — see the module docs.
        let WorldDelta {
            header,
            tiles,
            removed_tiles,
            populations,
            removed_populations,
            power,
            removed_power,
            power_metrics,
            great_discovery_definitions,
            great_discoveries,
            great_discovery_progress,
            great_discovery_telemetry,
            knowledge_ledger,
            removed_knowledge_ledger,
            knowledge_metrics,
            victory,
            capability_flags,
            command_events,
            command_events_retention_turns,
            campaign_profiles,
            pending_forks,
            stance_axes,
            voice_medium,
            opening_loadout,
            knowledge_timeline,
            crisis_telemetry,
            crisis_overlay,
            herds,
            food_modules,
            faction_inventory,
            sedentarization,
            discovered_sites,
            demographics,
            forage_patches,
            intensification_knowledge,
            ladder_knowledge,
            kits,
            default_hunt_kit_id,
            default_forage_kit_id,
            default_scout_kit_id,
            default_warrior_kit_id,
            default_expedition_kit_id,
            equipment_config_json,
            materials,
            characteristic_bands,
            recipes,
            craft_knowledge,
            route_rungs,
            moisture_raster,
            elevation_overlay,
            climate_bands,
            temperature_survivability,
            start_marker,
            axis_bias,
            sentiment,
            sentiment_raster,
            corruption_raster,
            culture_raster,
            military_raster,
            visibility_raster,
            fog_enabled,
            generations,
            removed_generations,
            corruption,
            influencers,
            removed_influencers,
            terrain,
            culture_layers,
            removed_culture_layers,
            culture_tensions,
            discovery_progress,
            connections,
            routes,
        } = delta;

        // --- header, and the one scalar carried on every frame -----------------------------------
        self.header = header.clone();
        self.fog_enabled = *fog_enabled;

        // --- keyed sections with a removal list --------------------------------------------------
        upsert_keyed(&mut self.tiles, tiles, |tile| tile.entity);
        remove_keyed(&mut self.tiles, removed_tiles, |tile| tile.entity);
        upsert_keyed(&mut self.populations, populations, |cohort| cohort.entity);
        remove_keyed(&mut self.populations, removed_populations, |cohort| {
            cohort.entity
        });
        upsert_keyed(&mut self.power, power, |node| node.entity);
        remove_keyed(&mut self.power, removed_power, |node| node.entity);
        upsert_keyed(&mut self.generations, generations, |generation| {
            generation.id
        });
        remove_keyed(&mut self.generations, removed_generations, |generation| {
            generation.id
        });
        upsert_keyed(&mut self.influencers, influencers, |influencer| {
            influencer.id
        });
        remove_keyed(&mut self.influencers, removed_influencers, |influencer| {
            influencer.id
        });
        upsert_keyed(&mut self.culture_layers, culture_layers, |layer| layer.id);
        remove_keyed(&mut self.culture_layers, removed_culture_layers, |layer| {
            layer.id
        });
        // The ledger's removal list carries the PACKED `(owner, discovery)` key, so both sides of
        // this section key on the wire form.
        upsert_keyed(
            &mut self.knowledge_ledger,
            knowledge_ledger,
            KnowledgeLedgerEntryState::wire_key,
        );
        remove_keyed(
            &mut self.knowledge_ledger,
            removed_knowledge_ledger,
            KnowledgeLedgerEntryState::wire_key,
        );

        // --- keyed sections with NO removal list: upsert only, never shrink ----------------------
        upsert_keyed(&mut self.discovery_progress, discovery_progress, |entry| {
            (entry.faction, entry.discovery)
        });
        upsert_keyed(&mut self.great_discoveries, great_discoveries, |entry| {
            (entry.faction, entry.id)
        });
        upsert_keyed(
            &mut self.great_discovery_progress,
            great_discovery_progress,
            |entry| (entry.faction, entry.discovery),
        );

        // --- whole sections: `Some` replaces (an empty `Some` clears), `None` leaves alone --------
        replace_if_some(&mut self.power_metrics, power_metrics);
        replace_if_some(
            &mut self.great_discovery_definitions,
            great_discovery_definitions,
        );
        replace_if_some(
            &mut self.great_discovery_telemetry,
            great_discovery_telemetry,
        );
        replace_if_some(&mut self.knowledge_metrics, knowledge_metrics);
        replace_if_some(&mut self.knowledge_timeline, knowledge_timeline);
        replace_if_some(&mut self.victory, victory);
        replace_if_some(&mut self.capability_flags, capability_flags);
        replace_if_some(
            &mut self.command_events_retention_turns,
            command_events_retention_turns,
        );
        replace_if_some(&mut self.campaign_profiles, campaign_profiles);
        replace_if_some(&mut self.pending_forks, pending_forks);
        replace_if_some(&mut self.stance_axes, stance_axes);
        replace_if_some(&mut self.voice_medium, voice_medium);
        replace_if_some(&mut self.opening_loadout, opening_loadout);
        replace_if_some(&mut self.crisis_telemetry, crisis_telemetry);
        replace_if_some(&mut self.crisis_overlay, crisis_overlay);
        replace_if_some(&mut self.herds, herds);
        replace_if_some(&mut self.food_modules, food_modules);
        replace_if_some(&mut self.faction_inventory, faction_inventory);
        replace_if_some(&mut self.sedentarization, sedentarization);
        replace_if_some(&mut self.discovered_sites, discovered_sites);
        replace_if_some(&mut self.demographics, demographics);
        replace_if_some(&mut self.forage_patches, forage_patches);
        replace_if_some(
            &mut self.intensification_knowledge,
            intensification_knowledge,
        );
        replace_if_some(&mut self.ladder_knowledge, ladder_knowledge);
        replace_if_some(&mut self.kits, kits);
        replace_if_some(&mut self.default_hunt_kit_id, default_hunt_kit_id);
        replace_if_some(&mut self.default_forage_kit_id, default_forage_kit_id);
        replace_if_some(&mut self.default_scout_kit_id, default_scout_kit_id);
        replace_if_some(&mut self.default_warrior_kit_id, default_warrior_kit_id);
        replace_if_some(
            &mut self.default_expedition_kit_id,
            default_expedition_kit_id,
        );
        replace_if_some(&mut self.equipment_config_json, equipment_config_json);
        replace_if_some(&mut self.materials, materials);
        replace_if_some(&mut self.characteristic_bands, characteristic_bands);
        replace_if_some(&mut self.recipes, recipes);
        replace_if_some(&mut self.craft_knowledge, craft_knowledge);
        replace_if_some(&mut self.route_rungs, route_rungs);
        replace_if_some(&mut self.moisture_raster, moisture_raster);
        replace_if_some(&mut self.elevation_overlay, elevation_overlay);
        replace_if_some(&mut self.climate_bands, climate_bands);
        replace_if_some(
            &mut self.temperature_survivability,
            temperature_survivability,
        );
        replace_if_some(&mut self.terrain, terrain);
        replace_if_some(&mut self.sentiment_raster, sentiment_raster);
        replace_if_some(&mut self.corruption_raster, corruption_raster);
        replace_if_some(&mut self.culture_raster, culture_raster);
        replace_if_some(&mut self.military_raster, military_raster);
        replace_if_some(&mut self.visibility_raster, visibility_raster);
        replace_if_some(&mut self.axis_bias, axis_bias);
        replace_if_some(&mut self.sentiment, sentiment);
        replace_if_some(&mut self.corruption, corruption);
        replace_if_some(&mut self.culture_tensions, culture_tensions);
        replace_if_some(&mut self.connections, connections);
        replace_if_some(&mut self.routes, routes);

        // --- the flattened `Option<Option<_>>` ---------------------------------------------------
        // The producer cannot say "cleared" (`capture.rs` flattens the whole-diff of an `Option`),
        // so `None` is "unchanged" and only `Some` moves it.
        if let Some(marker) = start_marker {
            self.start_marker = Some(marker.clone());
        }

        // --- the append-only feed ----------------------------------------------------------------
        // After the retention window above, so this frame's window is the one the trim uses.
        if let Some(appended) = command_events {
            append_events(
                &mut self.command_events,
                appended,
                self.command_events_retention_turns,
            );
        }

        Ok(())
    }
}

/// Replace every row whose key the delta carries, in place, and append the keys it does not hold
/// yet in delta order. An empty `rows` is `diff_indexed`'s "nothing changed" and touches nothing.
fn upsert_keyed<T, K>(target: &mut Vec<T>, rows: &[T], key: impl Fn(&T) -> K)
where
    T: Clone,
    K: Eq + Hash,
{
    if rows.is_empty() {
        return;
    }
    let mut index: HashMap<K, usize> = target
        .iter()
        .enumerate()
        .map(|(position, row)| (key(row), position))
        .collect();
    for row in rows {
        match index.get(&key(row)) {
            Some(&position) => target[position] = row.clone(),
            None => {
                index.insert(key(row), target.len());
                target.push(row.clone());
            }
        }
    }
}

/// Delete every row whose key the delta's `removed_*` list names. A key the snapshot does not hold
/// (a restated removal on a held frame) is nothing to do.
fn remove_keyed<T, K>(target: &mut Vec<T>, removed: &[K], key: impl Fn(&T) -> K)
where
    K: Eq + Hash + Copy,
{
    if removed.is_empty() {
        return;
    }
    let gone: HashSet<K> = removed.iter().copied().collect();
    target.retain(|row| !gone.contains(&key(row)));
}

/// `diff_whole`'s consumer: `Some` is the section's new value, whatever it holds; `None` is
/// "unchanged". There is deliberately no emptiness gate — an emptied section arrives as
/// `Some(empty)`, and reading that as "unchanged" is the bug `WorldDelta::culture_tensions` records.
fn replace_if_some<T: Clone>(target: &mut T, update: &Option<T>) {
    if let Some(value) = update {
        *target = value.clone();
    }
}

/// `diff_appended`'s consumer, mirroring `CommandEventLog`'s own rules: rows are keyed by their
/// one-based `seq` (a restated row is not a second row), arrival order is kept (a rollback replays
/// `seq` values, so the feed is never re-sorted), and the window is a count of DISTINCT TURNS
/// anchored on the newest tick held — `retention_turns` turns, the anchor's included.
///
/// A window of `0` is never published (`SimulationConfig` rejects it) and is the value a snapshot
/// that has not been told its window holds, so it trims nothing rather than everything.
fn append_events(
    target: &mut Vec<CommandEventState>,
    appended: &[CommandEventState],
    retention_turns: u32,
) {
    let mut known: HashSet<u64> = target.iter().map(|event| event.seq).collect();
    for event in appended {
        if known.insert(event.seq) {
            target.push(event.clone());
        }
    }
    if retention_turns == 0 {
        return;
    }
    let Some(newest_tick) = target.iter().map(|event| event.tick).max() else {
        return;
    };
    let turns_before_anchor = u64::from(retention_turns) - 1;
    let oldest_kept = newest_tick.saturating_sub(turns_before_anchor);
    target.retain(|event| event.tick >= oldest_kept);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::saturated_snapshot;
    use crate::state::campaign::VictorySnapshotState;
    use crate::state::map::StartMarkerState;
    use crate::world::{encode_snapshot_json, hash_snapshot, SnapshotHeader};

    /// The first live frame number and the one that follows it.
    const BASE_FRAME: u64 = 7;
    const NEXT_FRAME: u64 = BASE_FRAME + 1;
    /// A retention window small enough for a short feed to overflow it.
    const SHORT_WINDOW_TURNS: u32 = 3;

    /// The saturated world, stamped as a live frame so a delta can name it.
    fn a_world() -> WorldSnapshot {
        let mut world = saturated_snapshot().expect("the fixture builds");
        world.header.frame_seq = BASE_FRAME;
        world.header.base_frame_seq = 0;
        world
    }

    /// A delta that names `world` as its base and otherwise says nothing changed.
    fn a_delta_against(world: &WorldSnapshot) -> WorldDelta {
        let mut header = world.header.clone();
        header.base_frame_seq = world.header.frame_seq;
        header.frame_seq = NEXT_FRAME;
        WorldDelta {
            header,
            fog_enabled: world.fog_enabled,
            ..Default::default()
        }
    }

    fn json(world: &WorldSnapshot) -> String {
        encode_snapshot_json(world).expect("json")
    }

    /// `world` after `delta`, as the header alone would leave it — the comparison target for
    /// every "leaves the section alone" claim.
    fn only_the_header_moved(world: &WorldSnapshot, delta: &WorldDelta) -> WorldSnapshot {
        let mut expected = world.clone();
        expected.header = delta.header.clone();
        expected
    }

    #[test]
    fn a_keyed_section_is_upserted_in_place_appended_in_order_and_swept() {
        let world = a_world();
        let [first, second] = [&world.tiles[0], &world.tiles[1]];
        let mut changed_second = second.clone();
        changed_second.culture_layer = changed_second.culture_layer.wrapping_add(1);
        let mut brand_new = first.clone();
        brand_new.entity = world.tiles.iter().map(|tile| tile.entity).max().unwrap() + 1;

        let mut delta = a_delta_against(&world);
        delta.tiles = vec![brand_new.clone(), changed_second.clone()];
        delta.removed_tiles = vec![first.entity];

        let mut applied = world.clone();
        applied.apply_delta(&delta).expect("applies");

        let mut expected: Vec<_> = world.tiles.clone();
        expected[1] = changed_second; // replaced IN PLACE
        expected.push(brand_new); // appended, whatever order the delta listed it in
        expected.remove(0); // swept after the upsert
        assert_eq!(applied.tiles, expected);
    }

    /// Every keyed section at once, off the fixture: the delta re-carries row 0 (so a wrong key
    /// would append a duplicate) and removes row 1 (so a wrong key would remove nothing). The three
    /// sections with no removal list keep both rows.
    #[test]
    fn every_keyed_section_keys_on_the_field_the_producer_keys_on() {
        let world = a_world();
        let mut delta = a_delta_against(&world);
        delta.tiles = vec![world.tiles[0].clone()];
        delta.removed_tiles = vec![world.tiles[1].entity];
        delta.populations = vec![world.populations[0].clone()];
        delta.removed_populations = vec![world.populations[1].entity];
        delta.power = vec![world.power[0].clone()];
        delta.removed_power = vec![world.power[1].entity];
        delta.generations = vec![world.generations[0].clone()];
        delta.removed_generations = vec![world.generations[1].id];
        delta.influencers = vec![world.influencers[0].clone()];
        delta.removed_influencers = vec![world.influencers[1].id];
        delta.culture_layers = vec![world.culture_layers[0].clone()];
        delta.removed_culture_layers = vec![world.culture_layers[1].id];
        delta.knowledge_ledger = vec![world.knowledge_ledger[0].clone()];
        delta.removed_knowledge_ledger = vec![world.knowledge_ledger[1].wire_key()];
        delta.discovery_progress = vec![world.discovery_progress[0].clone()];
        delta.great_discoveries = vec![world.great_discoveries[0].clone()];
        delta.great_discovery_progress = vec![world.great_discovery_progress[0].clone()];

        let mut applied = world.clone();
        applied.apply_delta(&delta).expect("applies");

        // The fixture seeds two rows per section except `tiles`, which has one per grid cell —
        // so the expectation is "minus the second row", not "the first row only".
        fn without_second<T: Clone>(rows: &[T]) -> Vec<T> {
            let mut kept = rows.to_vec();
            kept.remove(1);
            kept
        }
        assert_eq!(applied.tiles, without_second(&world.tiles));
        assert_eq!(applied.populations, without_second(&world.populations));
        assert_eq!(applied.power, without_second(&world.power));
        assert_eq!(applied.generations, without_second(&world.generations));
        assert_eq!(applied.influencers, without_second(&world.influencers));
        assert_eq!(
            applied.culture_layers,
            without_second(&world.culture_layers)
        );
        assert_eq!(
            applied.knowledge_ledger,
            without_second(&world.knowledge_ledger)
        );
        assert_eq!(applied.discovery_progress, world.discovery_progress);
        assert_eq!(applied.great_discoveries, world.great_discoveries);
        assert_eq!(
            applied.great_discovery_progress,
            world.great_discovery_progress
        );
    }

    #[test]
    fn a_no_removal_section_is_replaced_by_key_and_never_shrinks() {
        let world = a_world();
        let mut moved = world.discovery_progress[1].clone();
        moved.progress += 1;
        let mut delta = a_delta_against(&world);
        delta.discovery_progress = vec![moved.clone()];

        let mut applied = world.clone();
        applied.apply_delta(&delta).expect("applies");

        let mut expected = world.discovery_progress.clone();
        expected[1] = moved;
        assert_eq!(applied.discovery_progress, expected);
    }

    #[test]
    fn an_empty_delta_leaves_every_section_alone() {
        let world = a_world();
        let delta = a_delta_against(&world);
        let mut applied = world.clone();
        applied.apply_delta(&delta).expect("applies");
        let expected = only_the_header_moved(&world, &delta);
        assert_eq!(json(&applied), json(&expected));
        assert_eq!(hash_snapshot(&applied), hash_snapshot(&expected));
    }

    /// `Some(empty)` on every whole section clears it — the reading the encoder keeps distinct
    /// from `None` and the one an emptiness gate would destroy.
    #[test]
    fn some_empty_clears_a_whole_section() {
        let world = a_world();
        let mut delta = a_delta_against(&world);
        delta.power_metrics = Some(Default::default());
        delta.great_discovery_definitions = Some(Vec::new());
        delta.great_discovery_telemetry = Some(Default::default());
        delta.knowledge_metrics = Some(Default::default());
        delta.knowledge_timeline = Some(Vec::new());
        delta.victory = Some(VictorySnapshotState::default());
        delta.capability_flags = Some(0);
        delta.command_events_retention_turns = Some(0);
        delta.campaign_profiles = Some(Vec::new());
        delta.pending_forks = Some(Vec::new());
        delta.stance_axes = Some(Vec::new());
        delta.voice_medium = Some(Vec::new());
        delta.opening_loadout = Some(Default::default());
        delta.crisis_telemetry = Some(Default::default());
        delta.crisis_overlay = Some(Default::default());
        delta.herds = Some(Vec::new());
        delta.food_modules = Some(Vec::new());
        delta.faction_inventory = Some(Vec::new());
        delta.sedentarization = Some(Vec::new());
        delta.discovered_sites = Some(Vec::new());
        delta.demographics = Some(Vec::new());
        delta.forage_patches = Some(Vec::new());
        delta.intensification_knowledge = Some(Vec::new());
        delta.ladder_knowledge = Some(Vec::new());
        delta.kits = Some(Vec::new());
        delta.default_hunt_kit_id = Some(String::new());
        delta.default_forage_kit_id = Some(String::new());
        delta.default_scout_kit_id = Some(String::new());
        delta.default_warrior_kit_id = Some(String::new());
        delta.default_expedition_kit_id = Some(String::new());
        delta.equipment_config_json = Some(String::new());
        delta.materials = Some(Vec::new());
        delta.characteristic_bands = Some(Vec::new());
        delta.recipes = Some(Vec::new());
        delta.craft_knowledge = Some(Vec::new());
        delta.route_rungs = Some(Vec::new());
        delta.moisture_raster = Some(Default::default());
        delta.elevation_overlay = Some(Default::default());
        delta.climate_bands = Some(Default::default());
        delta.temperature_survivability = Some(Default::default());
        delta.terrain = Some(Default::default());
        delta.sentiment_raster = Some(Default::default());
        delta.corruption_raster = Some(Default::default());
        delta.culture_raster = Some(Default::default());
        delta.military_raster = Some(Default::default());
        delta.visibility_raster = Some(Default::default());
        delta.axis_bias = Some(Default::default());
        delta.sentiment = Some(Default::default());
        delta.corruption = Some(Default::default());
        delta.culture_tensions = Some(Vec::new());
        delta.connections = Some(Vec::new());
        delta.routes = Some(Vec::new());

        let mut applied = world.clone();
        applied.apply_delta(&delta).expect("applies");

        // Everything the delta cleared is at its empty value; everything else (the keyed
        // sections, the feed, `start_marker`) is where the fixture left it.
        let mut expected = WorldSnapshot {
            header: delta.header.clone(),
            fog_enabled: world.fog_enabled,
            tiles: world.tiles.clone(),
            populations: world.populations.clone(),
            power: world.power.clone(),
            generations: world.generations.clone(),
            influencers: world.influencers.clone(),
            culture_layers: world.culture_layers.clone(),
            knowledge_ledger: world.knowledge_ledger.clone(),
            discovery_progress: world.discovery_progress.clone(),
            great_discoveries: world.great_discoveries.clone(),
            great_discovery_progress: world.great_discovery_progress.clone(),
            command_events: world.command_events.clone(),
            start_marker: world.start_marker.clone(),
            ..Default::default()
        };
        expected.fog_enabled = world.fog_enabled;
        assert_eq!(json(&applied), json(&expected));
    }

    #[test]
    fn start_marker_none_leaves_and_some_sets() {
        let world = a_world();
        let mut applied = world.clone();
        applied
            .apply_delta(&a_delta_against(&world))
            .expect("applies");
        assert_eq!(applied.start_marker, world.start_marker);

        let moved = StartMarkerState {
            x: world.start_marker.as_ref().map_or(0, |m| m.x) + 1,
            y: 0,
        };
        let mut delta = a_delta_against(&world);
        delta.start_marker = Some(moved.clone());
        let mut applied = world.clone();
        applied.apply_delta(&delta).expect("applies");
        assert_eq!(applied.start_marker, Some(moved));
    }

    #[test]
    fn fog_enabled_is_copied_from_every_delta() {
        let world = a_world();
        for fog in [false, true] {
            let mut delta = a_delta_against(&world);
            delta.fog_enabled = fog;
            let mut applied = world.clone();
            applied.apply_delta(&delta).expect("applies");
            assert_eq!(applied.fog_enabled, fog);
        }
    }

    fn event(seq: u64, tick: u64) -> CommandEventState {
        CommandEventState {
            tick,
            kind: "forage".to_owned(),
            faction: 0,
            label: format!("event {seq}"),
            detail: None,
            seq,
        }
    }

    #[test]
    fn events_are_appended_once_per_seq_and_trimmed_to_the_tick_window() {
        let mut world = a_world();
        world.command_events = vec![event(1, 1), event(2, 2)];
        world.command_events_retention_turns = SHORT_WINDOW_TURNS;

        let mut delta = a_delta_against(&world);
        // `seq 2` restated (a held frame's re-send), then two new rows on later turns.
        delta.command_events = Some(vec![event(2, 2), event(3, 3), event(4, 4)]);
        let mut applied = world.clone();
        applied.apply_delta(&delta).expect("applies");

        // Anchor 4, window 3 turns → ticks 2..=4 stay, tick 1 is evicted, seq 2 appears once.
        assert_eq!(
            applied.command_events,
            vec![event(2, 2), event(3, 3), event(4, 4)]
        );
    }

    #[test]
    fn a_narrowed_window_on_the_same_delta_trims_with_the_new_window() {
        let mut world = a_world();
        world.command_events = vec![event(1, 1), event(2, 2), event(3, 3)];
        world.command_events_retention_turns = SHORT_WINDOW_TURNS + 1;

        let mut delta = a_delta_against(&world);
        delta.command_events_retention_turns = Some(1);
        delta.command_events = Some(vec![event(4, 4)]);
        let mut applied = world.clone();
        applied.apply_delta(&delta).expect("applies");
        assert_eq!(applied.command_events, vec![event(4, 4)]);
    }

    /// A held frame restates what the previous held frame carried; applying the restatement lands
    /// on the same world.
    #[test]
    fn applying_a_restating_delta_is_idempotent() {
        let world = a_world();
        let mut first = a_delta_against(&world);
        first.tiles = vec![world.tiles[1].clone()];
        first.removed_tiles = vec![world.tiles[0].entity];
        first.demographics = Some(Vec::new());
        first.command_events = Some(vec![event(u64::MAX, world.header.tick)]);
        first.start_marker = Some(StartMarkerState { x: 3, y: 3 });

        let mut applied = world.clone();
        applied.apply_delta(&first).expect("applies");
        let after_first = json(&applied);

        let mut restated = first.clone();
        restated.header.base_frame_seq = applied.header.frame_seq;
        restated.header.frame_seq = applied.header.frame_seq + 1;
        applied.apply_delta(&restated).expect("applies again");

        let mut expected_after_second = applied.clone();
        expected_after_second.header = first.header.clone();
        assert_eq!(json(&expected_after_second), after_first);
    }

    #[test]
    fn a_delta_for_another_base_or_epoch_is_refused_and_nothing_moves() {
        let world = a_world();
        let before = json(&world);

        let mut wrong_base = a_delta_against(&world);
        wrong_base.header.base_frame_seq = world.header.frame_seq + 1;
        wrong_base.tiles = vec![world.tiles[0].clone()];
        wrong_base.removed_tiles = vec![world.tiles[1].entity];
        wrong_base.fog_enabled = !world.fog_enabled;
        let mut applied = world.clone();
        assert_eq!(
            applied.apply_delta(&wrong_base),
            Err(ApplyDeltaError::BaseMismatch {
                expected: world.header.frame_seq,
                got: world.header.frame_seq + 1,
            })
        );
        assert_eq!(json(&applied), before);

        let mut wrong_epoch = a_delta_against(&world);
        wrong_epoch.header.world_epoch = world.header.world_epoch + 1;
        wrong_epoch.fog_enabled = !world.fog_enabled;
        assert_eq!(
            applied.apply_delta(&wrong_epoch),
            Err(ApplyDeltaError::WorldEpochMismatch {
                expected: world.header.world_epoch,
                got: world.header.world_epoch + 1,
            })
        );
        assert_eq!(json(&applied), before);
    }

    /// The header is the delta's, wholesale — including the frame number the next delta will
    /// have to name.
    #[test]
    fn the_header_becomes_the_deltas_header() {
        let world = a_world();
        let mut delta = a_delta_against(&world);
        delta.header.tick = world.header.tick + 1;
        let mut applied = world.clone();
        applied.apply_delta(&delta).expect("applies");
        let expected: SnapshotHeader = delta.header.clone();
        assert_eq!(
            serde_json::to_string(&applied.header).expect("json"),
            serde_json::to_string(&expected).expect("json")
        );
        assert_eq!(applied.header.frame_seq, NEXT_FRAME);
    }
}
