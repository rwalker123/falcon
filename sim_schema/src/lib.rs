//! Wire-facing schema for the simulation: the flat world payloads and their codecs.
//!
//! The crate is partitioned along the **nine domain sections of `snapshot.fbs`**:
//!
//! - [`state`] — the world-state structs and enums, one module per section (Vision has none: its
//!   three rasters are `ScalarRasterState` fields on [`WorldSnapshot`] itself).
//! - [`world`] — the flat [`WorldSnapshot`] / [`WorldDelta`] payloads, their header, and the
//!   bincode / JSON codecs plus the on-disk [`MapExport`].
//! - [`codec`] — the FlatBuffers encoders, one module per section.
//!
//! Every item is re-exported at the crate root, so consumers keep using `sim_schema::Foo`.
//! When you add a snapshot field, append it to its section's `state` module **and** that
//! section's `codec` module — see `sim_schema/README.md`.

pub mod codec;
pub mod state;
pub mod world;

pub use codec::*;
pub use state::*;
pub use world::*;

#[cfg(test)]
mod tests {
    use super::*;
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    /// A `WorldSnapshot` carrying exactly one herd — the rest of the world is irrelevant to the herd
    /// telemetry's wire encoding.
    fn snapshot_with_herd(herd: HerdTelemetryState) -> WorldSnapshot {
        WorldSnapshot {
            herds: vec![herd],
            ..WorldSnapshot::default()
        }
    }

    /// **The pen-as-a-managed-population fields survive the wire.** `penUpkeep` (what the pen eats
    /// each turn) and `penFedFraction` (`< 1` = starving) are appended to `HerdTelemetryState`
    /// (append-only discipline), and the client renders the feed as a negative row against the
    /// **gross** `corralYield`. Encode → decode with the generated reader, so a field that silently
    /// failed to serialize cannot pass.
    #[test]
    fn herd_pen_upkeep_and_fed_fraction_round_trip_on_the_wire() {
        const UPKEEP: f32 = 1.2;
        const FED: f32 = 0.25;
        const CORRAL_YIELD: f32 = 3.6;

        let snapshot = snapshot_with_herd(HerdTelemetryState {
            id: "herd_pen".to_string(),
            species: "Red Deer".to_string(),
            corralled: true,
            corral_yield: CORRAL_YIELD,
            pen_upkeep: UPKEEP,
            pen_fed_fraction: FED,
            ..Default::default()
        });

        let bytes = encode_snapshot_flatbuffer(&snapshot);
        let envelope = fb::root_as_envelope(&bytes).expect("snapshot decodes");
        let herd = envelope
            .payload_as_snapshot()
            .expect("snapshot payload")
            .subsistence()
            .expect("subsistence section present")
            .herds()
            .expect("herds present")
            .get(0);
        assert!(herd.corralled());
        assert!((herd.corralYield() - CORRAL_YIELD).abs() < 1e-6);
        assert!((herd.penUpkeep() - UPKEEP).abs() < 1e-6);
        assert!((herd.penFedFraction() - FED).abs() < 1e-6);
    }

    /// A herd that is **not** penned eats nothing and is never starving — it decodes to the neutral
    /// pair (the `= 0` / `= 1` schema defaults).
    #[test]
    fn an_unpenned_herd_defaults_to_no_upkeep_and_fully_fed() {
        let snapshot = snapshot_with_herd(HerdTelemetryState {
            id: "herd_wild".to_string(),
            ..Default::default()
        });

        let bytes = encode_snapshot_flatbuffer(&snapshot);
        let envelope = fb::root_as_envelope(&bytes).expect("snapshot decodes");
        let herd = envelope
            .payload_as_snapshot()
            .expect("snapshot payload")
            .subsistence()
            .expect("subsistence section present")
            .herds()
            .expect("herds present")
            .get(0);
        assert_eq!(herd.penUpkeep(), 0.0);
        assert_eq!(herd.penFedFraction(), 1.0);
    }
}
