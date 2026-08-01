//! Wire-facing schema for the simulation: the flat world payloads and their codecs.
//!
//! The crate is partitioned along the **nine domain sections of `snapshot.fbs`**:
//!
//! - [`state`] — the world-state structs and enums, one module per section (Vision has none: its
//!   three rasters are `ScalarRasterState` fields on [`WorldSnapshot`] itself).
//! - [`world`] — the flat [`WorldSnapshot`] / [`WorldDelta`] payloads, their header, and the
//!   JSON codecs plus the on-disk [`MapExport`].
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
    /// **gross** `corralYield`. The two investment rungs' payoffs are **pairs**, so their trade
    /// halves (`pastoralTrade` / `corralTrade`) ride the same fixture — the sim held both on a
    /// `YieldAccounts` while the wire carried only the provisions half. Encode → decode with the
    /// generated reader, so a field that silently failed to serialize cannot pass.
    #[test]
    fn herd_pen_upkeep_and_fed_fraction_round_trip_on_the_wire() {
        const UPKEEP: f32 = 1.2;
        const FED: f32 = 0.25;
        const CORRAL_YIELD: f32 = 3.6;
        const CORRAL_TRADE: f32 = 0.9;
        const PASTORAL_YIELD: f32 = 1.8;
        const PASTORAL_TRADE: f32 = 0.45;

        let snapshot = snapshot_with_herd(HerdTelemetryState {
            id: "herd_pen".to_string(),
            species: "Red Deer".to_string(),
            corralled: true,
            corral_yield: CORRAL_YIELD,
            corral_trade: CORRAL_TRADE,
            pastoral_yield: PASTORAL_YIELD,
            pastoral_trade: PASTORAL_TRADE,
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
        assert!((herd.corralTrade() - CORRAL_TRADE).abs() < 1e-6);
        assert!((herd.pastoralYield() - PASTORAL_YIELD).abs() < 1e-6);
        assert!((herd.pastoralTrade() - PASTORAL_TRADE).abs() < 1e-6);
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

    /// **The harvest FLOOR survives the wire, on the assignment AND on the raid.**
    ///
    /// A labor assignment carries a `floor` and a hunt expedition carries an `expeditionFloor` — the
    /// whole of what the player decides about pressure (`docs/plan_harvest_floor.md`). The four-value
    /// `policy` label that used to ride beside them is a `(deprecated)` slot the encoder can no
    /// longer write to at all, which is the append-only discipline enforcing itself.
    ///
    /// Encode → decode with the generated reader, so a field that silently failed to serialize cannot
    /// pass — the hazard when the authority is appended *behind* the label that used to be it.
    #[test]
    fn the_harvest_floor_rides_the_wire_on_the_assignment_and_the_raid() {
        /// A floor no retired stance named, so a value that appears cannot be a defaulted label.
        const UNLABELLED_FLOOR: f32 = 0.42;
        /// The raid's own floor, deliberately different from the assignment's.
        const RAID_FLOOR: f32 = 0.07;

        let snapshot = WorldSnapshot {
            populations: vec![PopulationCohortState {
                expedition_floor: RAID_FLOOR,
                labor_assignments: vec![LaborAssignmentState {
                    kind: "forage".to_string(),
                    floor: UNLABELLED_FLOOR,
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..WorldSnapshot::default()
        };

        let bytes = encode_snapshot_flatbuffer(&snapshot);
        let envelope = fb::root_as_envelope(&bytes).expect("snapshot decodes");
        let cohort = envelope
            .payload_as_snapshot()
            .expect("snapshot payload")
            .population()
            .expect("population section present")
            .populations()
            .expect("cohorts present")
            .get(0);

        assert!(
            (cohort.expeditionFloor() - RAID_FLOOR).abs() < 1e-6,
            "the raid's floor crosses verbatim: {}",
            cohort.expeditionFloor()
        );
        let assignment = cohort
            .laborAssignments()
            .expect("labor assignments present")
            .get(0);
        assert!(
            (assignment.floor() - UNLABELLED_FLOOR).abs() < 1e-6,
            "the assignment's floor is the authority and must cross verbatim: {}",
            assignment.floor()
        );
    }

    /// **The per-biomass yield VECTOR crosses the wire, on both food webs.**
    ///
    /// It is what makes the floor draggable (`docs/plan_harvest_floor.md` §5): with `biomass`, the
    /// carrying capacity and a rate, the client evaluates `max(0, B − f·K) × rate` at any floor,
    /// which the retired four ceiling rows could not express. **An inedible species is the case that
    /// proves it must be a vector** — a wolf reads `0` food with real trade, which a food scalar
    /// could not state at all.
    #[test]
    fn the_per_biomass_yield_vector_rides_the_wire_on_both_webs() {
        const WOLF_TRADE_RATE: f32 = 0.02;
        const PATCH_FOOD_RATE: f32 = 0.058;
        const PATCH_FODDER_RATE: f32 = 0.004;

        let snapshot = WorldSnapshot {
            herds: vec![HerdTelemetryState {
                id: "herd_wolf".to_string(),
                provisions_per_biomass: 0.0,
                trade_per_biomass: WOLF_TRADE_RATE,
                ..Default::default()
            }],
            forage_patches: vec![ForagePatchState {
                provisions_per_biomass: PATCH_FOOD_RATE,
                fodder_per_biomass: PATCH_FODDER_RATE,
                ..Default::default()
            }],
            ..WorldSnapshot::default()
        };

        let bytes = encode_snapshot_flatbuffer(&snapshot);
        let envelope = fb::root_as_envelope(&bytes).expect("snapshot decodes");
        let subsistence = envelope
            .payload_as_snapshot()
            .expect("snapshot payload")
            .subsistence()
            .expect("subsistence section present");

        let herd = subsistence.herds().expect("herds present").get(0);
        assert_eq!(
            herd.provisionsPerBiomass(),
            0.0,
            "a wolf is not food, and the vector is the only shape that can say so"
        );
        assert!((herd.tradePerBiomass() - WOLF_TRADE_RATE).abs() < 1e-6);

        let patch = subsistence.foragePatches().expect("patches present").get(0);
        assert!((patch.provisionsPerBiomass() - PATCH_FOOD_RATE).abs() < 1e-6);
        assert!((patch.fodderPerBiomass() - PATCH_FODDER_RATE).abs() < 1e-6);
    }

    /// **The per-worker BIOMASS throughput crosses the wire, on both food webs — and it survives on
    /// exactly the sources where the client's old derivation dies.**
    ///
    /// The vector above turns a floor into a *ceiling*; this turns that ceiling into a number of
    /// *people* (`ceil(room / (perWorkerBiomass × dip))`). The client used to recover it as
    /// `perWorkerYield ÷ provisionsPerBiomass` — exact, and `0 / 0` on the two sources that pay no
    /// food at all: a **wolf** herd and a sown **fibre/hay Field**. So the fixture makes both of
    /// those the subject: every food term is zero, and the throughput is still there and positive.
    #[test]
    fn the_per_worker_biomass_throughput_rides_the_wire_where_the_food_rate_cannot() {
        /// One hunter's biomass carry — `labor_config.hunt.per_worker_biomass_capacity`.
        const HUNTER_CARRY: f32 = 40.0;
        /// One gatherer's biomass carry at full season — `per_worker_biomass_capacity × 1.0`.
        const GATHERER_CARRY: f32 = 8.0;
        /// What a flax Field pays in trade goods per unit of standing crop — non-zero, so the source
        /// is genuinely productive while paying no food whatever.
        const FIELD_TRADE_RATE: f32 = 0.03;

        let snapshot = WorldSnapshot {
            herds: vec![HerdTelemetryState {
                id: "herd_wolf".to_string(),
                // A wolf: no food rate and no food throughput. `perWorkerYield / provisionsPerBiomass`
                // is `0 / 0` here, which is the whole reason this field exists.
                per_worker_yield: 0.0,
                provisions_per_biomass: 0.0,
                per_worker_biomass: HUNTER_CARRY,
                ..Default::default()
            }],
            forage_patches: vec![ForagePatchState {
                // A sown flax Field: pays trade goods and nothing else.
                per_worker_yield: 0.0,
                provisions_per_biomass: 0.0,
                trade_per_biomass: FIELD_TRADE_RATE,
                per_worker_biomass: GATHERER_CARRY,
                ..Default::default()
            }],
            ..WorldSnapshot::default()
        };

        let bytes = encode_snapshot_flatbuffer(&snapshot);
        let envelope = fb::root_as_envelope(&bytes).expect("snapshot decodes");
        let subsistence = envelope
            .payload_as_snapshot()
            .expect("snapshot payload")
            .subsistence()
            .expect("subsistence section present");

        let herd = subsistence.herds().expect("herds present").get(0);
        assert_eq!(
            herd.perWorkerYield(),
            0.0,
            "the fixture's premise: the food throughput a client would divide by is zero"
        );
        assert!(
            (herd.perWorkerBiomass() - HUNTER_CARRY).abs() < 1e-6,
            "a wolf's hunters still have a biomass throughput: {}",
            herd.perWorkerBiomass()
        );

        let patch = subsistence.foragePatches().expect("patches present").get(0);
        assert_eq!(
            patch.provisionsPerBiomass(),
            0.0,
            "the fixture's premise: a fibre Field pays no food"
        );
        assert!(
            (patch.perWorkerBiomass() - GATHERER_CARRY).abs() < 1e-6,
            "…and its gatherers still have a biomass throughput: {}",
            patch.perWorkerBiomass()
        );
    }

    /// **The sampled regrowth curve crosses the wire on both webs, and the two webs are NOT the same
    /// function.** That asymmetry is the load-bearing part: a patch is pure logistic with a reseed
    /// floor and no Allee term, so **no sample is negative**; a herd has critical depensation below
    /// `collapse_fraction`, so its **low samples are negative** and a client must render them as
    /// decline rather than clamping them.
    ///
    /// It is also why the curve is *sampled* instead of published as `r` + thresholds: two different
    /// functions re-implemented in a language with no tests over them would drift, and both would go
    /// on drawing a plausible chart. The exception this test guards is the boundary stated in
    /// `.claude/rules/core_sim/yield-forecast.md` — *terms where a closed form exists, answers where
    /// one does not*.
    #[test]
    fn the_sampled_regrowth_curve_is_non_negative_on_plants_and_negative_below_the_allee_point() {
        /// A herd's curve: negative in the Allee band, positive above it, peaking at `K/2`. The
        /// values are a hand-written stand-in for `fauna::net_biomass_delta`'s shape — this test is
        /// about the WIRE preserving sign and order, not about the model, which `core_sim` owns.
        const HERD_CURVE: [f32; 5] = [-3.0, -1.5, 6.0, 4.0, 0.0];
        /// A patch's curve: the `0.0` entry is the reseed floor's lift, so it is positive too.
        const PATCH_CURVE: [f32; 5] = [0.4, 1.8, 2.4, 1.6, 0.0];

        let snapshot = WorldSnapshot {
            herds: vec![HerdTelemetryState {
                id: "herd_aurochs".to_string(),
                regrowth_samples: HERD_CURVE.to_vec(),
                ..Default::default()
            }],
            forage_patches: vec![ForagePatchState {
                regrowth_samples: PATCH_CURVE.to_vec(),
                ..Default::default()
            }],
            ..WorldSnapshot::default()
        };

        let bytes = encode_snapshot_flatbuffer(&snapshot);
        let envelope = fb::root_as_envelope(&bytes).expect("snapshot decodes");
        let subsistence = envelope
            .payload_as_snapshot()
            .expect("snapshot payload")
            .subsistence()
            .expect("subsistence section present");

        let herd = subsistence.herds().expect("herds present").get(0);
        let herd_curve: Vec<f32> = herd
            .regrowthSamples()
            .expect("the herd publishes a curve")
            .iter()
            .collect();
        assert_eq!(
            herd_curve,
            HERD_CURVE.to_vec(),
            "the herd's curve crosses sample-for-sample, signs intact"
        );
        assert!(
            herd_curve.iter().any(|sample| *sample < 0.0),
            "the Allee crash must survive the wire — a clamped curve cannot say a herd is dying"
        );

        let patch = subsistence.foragePatches().expect("patches present").get(0);
        let patch_curve: Vec<f32> = patch
            .regrowthSamples()
            .expect("the patch publishes a curve")
            .iter()
            .collect();
        assert_eq!(patch_curve, PATCH_CURVE.to_vec());
        assert!(
            patch_curve.iter().all(|sample| *sample >= 0.0),
            "plants have no Allee crash, so the plant curve never dips below zero"
        );
        assert!(
            patch_curve[0] > 0.0,
            "…and its first sample is the reseed floor's lift, not zero"
        );
    }

    /// **The ecology phase BANDS cross the wire on both webs, as an ordered pair.**
    ///
    /// `ecologyPhase` ships which band a source is in; these ship where the bands are, in the same
    /// units the harvest floor is in (fractions of `K`), which is what lets the chart draw them as
    /// the zones the floor line is dragged against. They are **per source** because a herd's cuts
    /// come from the rung it stands on — `herd_ecology` resolves wild / pastoral / pen — so the two
    /// tables carry genuinely different numbers rather than echoing one global pair.
    ///
    /// This pins the codec. That the published bands actually **bracket the published phase word**
    /// is pinned against the sim in `core_sim/tests/ecology_bands_on_the_wire.rs`, which is where
    /// the two halves can drift apart.
    #[test]
    fn the_ecology_phase_bands_ride_the_wire_on_both_webs() {
        /// A wild herd's cuts — the shipped `fauna_config` shape.
        const HERD_COLLAPSE: f32 = 0.15;
        const HERD_STRESSED: f32 = 0.40;
        /// A patch's cuts, deliberately different from the herd's so an encoder that crossed the two
        /// tables' fields would fail rather than pass by coincidence.
        const PATCH_COLLAPSE: f32 = 0.10;
        const PATCH_STRESSED: f32 = 0.35;

        let snapshot = WorldSnapshot {
            herds: vec![HerdTelemetryState {
                id: "herd_boar".to_string(),
                collapse_fraction: HERD_COLLAPSE,
                stressed_fraction: HERD_STRESSED,
                ..Default::default()
            }],
            forage_patches: vec![ForagePatchState {
                collapse_fraction: PATCH_COLLAPSE,
                stressed_fraction: PATCH_STRESSED,
                ..Default::default()
            }],
            ..WorldSnapshot::default()
        };

        let bytes = encode_snapshot_flatbuffer(&snapshot);
        let envelope = fb::root_as_envelope(&bytes).expect("snapshot decodes");
        let subsistence = envelope
            .payload_as_snapshot()
            .expect("snapshot payload")
            .subsistence()
            .expect("subsistence section present");

        let herd = subsistence.herds().expect("herds present").get(0);
        assert!((herd.collapseFraction() - HERD_COLLAPSE).abs() < 1e-6);
        assert!((herd.stressedFraction() - HERD_STRESSED).abs() < 1e-6);

        let patch = subsistence.foragePatches().expect("patches present").get(0);
        assert!((patch.collapseFraction() - PATCH_COLLAPSE).abs() < 1e-6);
        assert!((patch.stressedFraction() - PATCH_STRESSED).abs() < 1e-6);

        assert!(
            herd.collapseFraction() != patch.collapseFraction(),
            "the two tables must carry their OWN cuts — a global echo would make this pass blind"
        );
    }
}
