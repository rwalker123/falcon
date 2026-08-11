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

    /// **The kit roster, its carry list, and every per-row kit id survive the wire.**
    ///
    /// Encode → decode through the generated reader, because a field appended behind an existing one
    /// is exactly the shape that silently fails to serialize — and every id here is what a consumer
    /// resolves a readout against, so an absent one reads as "quoted for nothing" rather than as a
    /// missing field.
    #[test]
    fn the_kit_roster_and_every_kit_id_ride_the_wire() {
        const BARE_HUNT_CARRY: f32 = 12.0;
        const BARE_FORAGE_CARRY: f32 = 1.6;
        const BARE_ATTACK: f32 = 1.0;
        const BARE_PEN_CARRY: f32 = 12.0;
        const BARE_VANTAGE_RANGE: f32 = 1.0;
        // The BAND-resolved twins of the three above (`PopulationCohortState`), deliberately unlike
        // the roster's fresh-kit numbers: a band's row is its own wear resolved against its own
        // job defaults, and the two must never be read as one value.
        const BAND_PEN_CARRY: f32 = 40.0;
        const BAND_VANTAGE_RANGE: f32 = 2.0;
        const BAND_WARRIOR_ATTACK: f32 = 6.0;

        let snapshot = WorldSnapshot {
            kits: vec![
                KitOptionState {
                    id: "none".to_string(),
                    display_name: "No kit".to_string(),
                    jobs: vec![
                        "hunt".to_string(),
                        "forage".to_string(),
                        "scout".to_string(),
                        "warrior".to_string(),
                    ],
                    attack: BARE_ATTACK,
                    hunt_carry_per_worker_biomass: BARE_HUNT_CARRY,
                    forage_carry_per_worker_biomass: BARE_FORAGE_CARRY,
                    pen_carry_per_worker_biomass: BARE_PEN_CARRY,
                    scout_vantage_range: BARE_VANTAGE_RANGE,
                    // `none` carries nothing, so every multiplier reads its neutral and its attack —
                    // the bare hand's — is bounded by nothing.
                    attack_min_body_mass: 0.0,
                    attack_max_body_mass: 0.0,
                    dispersion: 1.0,
                    exposure: 1.0,
                    build_rate: 1.0,
                    // Carrying nothing is a real answer, and an EMPTY vector is how it is said.
                    item_ids: Vec::new(),
                },
                // A second entry that actually carries gear, because the empty case above cannot
                // distinguish "this kit holds nothing" from "the field never reached the wire" —
                // and telling those apart is the entire reason `item_ids` exists.
                KitOptionState {
                    id: "big_game".to_string(),
                    display_name: "Big-game kit".to_string(),
                    jobs: vec!["hunt".to_string()],
                    item_ids: vec!["spears".to_string(), "sled".to_string()],
                    ..Default::default()
                },
            ],
            default_hunt_kit_id: "big_game".to_string(),
            default_forage_kit_id: "gathering".to_string(),
            default_scout_kit_id: "wayfinding".to_string(),
            default_warrior_kit_id: "warrior".to_string(),
            herds: vec![HerdTelemetryState {
                id: "herd_wild".to_string(),
                // The quarry's OWN default kit, deliberately not the snapshot's
                // `default_hunt_kit_id` above: a slot wired to the wrong string then shows up as a
                // swap rather than as a coincidence.
                default_kit_id: "trapping".to_string(),
                ..Default::default()
            }],
            populations: vec![PopulationCohortState {
                kit_id: "none".to_string(),
                // The three band-resolved tiers the expanded roster added. Given values DISTINCT
                // from each other and from the roster row above, so a codec entry wired to the
                // wrong field shows up as a swapped number rather than as a coincidence.
                pen_carry_per_worker_biomass: BAND_PEN_CARRY,
                scout_vantage_range: BAND_VANTAGE_RANGE,
                warrior_attack: BAND_WARRIOR_ATTACK,
                labor_assignments: vec![LaborAssignmentState {
                    kind: "hunt".to_string(),
                    kit_id: "big_game".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..WorldSnapshot::default()
        };

        let bytes = encode_snapshot_flatbuffer(&snapshot);
        let envelope = fb::root_as_envelope(&bytes).expect("snapshot decodes");
        let payload = envelope.payload_as_snapshot().expect("snapshot payload");

        let subsistence = payload.subsistence().expect("subsistence section present");
        assert_eq!(subsistence.defaultHuntKitId(), Some("big_game"));
        assert_eq!(subsistence.defaultForageKitId(), Some("gathering"));
        // The two band-wide roles' defaults ride the same way — they had no kit axis, and therefore
        // no default to name, until the roster gained wayfinding gear and clubs.
        assert_eq!(subsistence.defaultScoutKitId(), Some("wayfinding"));
        assert_eq!(subsistence.defaultWarriorKitId(), Some("warrior"));
        let option = subsistence.kits().expect("the roster is published").get(0);
        assert_eq!(option.id(), Some("none"));
        assert_eq!(option.displayName(), Some("No kit"));
        assert_eq!(option.attack(), BARE_ATTACK);
        assert_eq!(option.huntCarryPerWorkerBiomass(), BARE_HUNT_CARRY);
        assert_eq!(option.forageCarryPerWorkerBiomass(), BARE_FORAGE_CARRY);
        assert_eq!(option.penCarryPerWorkerBiomass(), BARE_PEN_CARRY);
        assert_eq!(option.scoutVantageRange(), BARE_VANTAGE_RANGE);
        let jobs = option
            .jobs()
            .expect("a kit states the jobs it may be sent on");
        assert_eq!(jobs.len(), 4);
        assert_eq!(jobs.get(0), "hunt");
        assert_eq!(jobs.get(2), "scout");
        assert_eq!(jobs.get(3), "warrior");
        assert_eq!(
            option
                .itemIds()
                .expect("a kit states what it carries")
                .len(),
            0,
            "`none` carries nothing, and says so with an empty list"
        );

        // **WHICH ITEMS A KIT CARRIES, in config order.** Without this the client has to infer the
        // gear from the tiers — which it did, by hardcoding `attack → spears`, and so quoted a
        // Trapping party the SPEARS' durability.
        let kitted = subsistence.kits().expect("the roster is published").get(1);
        let items = kitted.itemIds().expect("a kit states what it carries");
        assert_eq!(items.len(), 2);
        assert_eq!(
            items.get(0),
            "spears",
            "the weapon comes first, as config has it"
        );
        assert_eq!(items.get(1), "sled");

        // The herd row still ships. The two `*_kit_id` disclaimers that used to be asserted here are
        // retired with the estimate tables they described: they told a client *"these rows were
        // priced at the hunt default, refuse to show them for any other kit"*, which is what you
        // publish when you cannot answer the question. The client asks now, and names the kit.
        let herd = subsistence.herds().expect("herds present").get(0);
        assert_eq!(herd.id(), Some("herd_wild"));
        assert_eq!(herd.defaultKitId(), Some("trapping"));

        let cohort = payload
            .population()
            .expect("population section present")
            .populations()
            .expect("populations present")
            .get(0);
        assert_eq!(cohort.kitId(), Some("none"));
        // Read off the DECODED cohort, not the in-process struct: a field that never reached the
        // codec still passes an in-process assertion, which is the failure this whole test exists
        // to catch for an appended slot.
        assert_eq!(cohort.penCarryPerWorkerBiomass(), BAND_PEN_CARRY);
        assert_eq!(cohort.scoutVantageRange(), BAND_VANTAGE_RANGE);
        assert_eq!(cohort.warriorAttack(), BAND_WARRIOR_ATTACK);
        let row = cohort
            .laborAssignments()
            .expect("labor rows present")
            .get(0);
        assert_eq!(row.kitId(), Some("big_game"));
    }

    /// **THE THREE RETREAT/HAZARD MULTIPLIERS READ `1` THROUGH ALL THREE DOORS.**
    ///
    /// `stay_fraction`, `dispersion` and `exposure` are multipliers whose neutral is `1`, and each is
    /// reachable by three separate defaulting mechanisms that have no compiler relationship to each
    /// other: the FlatBuffers schema's `= 1`, `serde`'s missing-field default, and the Rust `Default`
    /// impl. **Two of the three were `0` until this test was written**, which is the wrong answer in
    /// the *reassuring* direction — `dispersion 0` says the party scares nothing and `exposure 0` says
    /// nobody can be hurt, so a field arriving by any of these doors would have handed every kit the
    /// passive device's whole advantage. (`stay_fraction 0` fails loudly instead: the take is zero.)
    ///
    /// **Each door is sabotage-verified to fail ALONE** — flipping the `Default` impl, the
    /// `#[serde(default = …)]` attribute, or the schema's `= 1` fails exactly its own leg. The wire
    /// leg took two attempts to make real, and the failed one is worth knowing because it is the
    /// obvious way to write it: see the comment at door 3.
    #[test]
    fn the_retreat_and_hazard_multipliers_are_neutral_at_one_on_every_defaulting_path() {
        const NEUTRAL: f32 = 1.0;

        // Door 1 — the Rust `Default` impls, which `..Default::default()` fixtures ride.
        assert_eq!(HerdTelemetryState::default().stay_fraction, NEUTRAL);
        assert_eq!(KitOptionState::default().dispersion, NEUTRAL);
        assert_eq!(KitOptionState::default().exposure, NEUTRAL);
        // …and the sentinel pair beside them is NOT neutral-at-one: `0` means unbounded there, and a
        // well-meaning sweep that "fixed" these to 1.0 would silently bound every weapon at 1 kg.
        assert_eq!(KitOptionState::default().attack_min_body_mass, 0.0);
        assert_eq!(KitOptionState::default().attack_max_body_mass, 0.0);

        // Door 2 — `serde`, with the field absent. Built by serializing a state that states a
        // deliberately NON-neutral value and then DELETING the key, so the number read back cannot
        // have come from the object under test: it is `serde`'s own missing-field default or nothing.
        // (Most of these structs' other fields are required, so a hand-written JSON literal would be
        // a maintenance burden that goes stale on every appended field.)
        fn without_key<T: serde::Serialize, U: serde::de::DeserializeOwned>(
            value: T,
            key: &str,
        ) -> U {
            let mut json = serde_json::to_value(value).expect("the state serializes");
            let object = json.as_object_mut().expect("a state is a JSON object");
            assert!(
                object.remove(key).is_some(),
                "`{key}` was not on the serialized state — this test is checking a field that moved"
            );
            serde_json::from_value(json).expect("the state deserializes with the key absent")
        }

        let herd: HerdTelemetryState = without_key(
            HerdTelemetryState {
                stay_fraction: 0.3,
                ..Default::default()
            },
            "stay_fraction",
        );
        assert_eq!(herd.stay_fraction, NEUTRAL);
        let kit: KitOptionState = without_key(
            KitOptionState {
                dispersion: 0.3,
                ..Default::default()
            },
            "dispersion",
        );
        assert_eq!(kit.dispersion, NEUTRAL);
        let kit: KitOptionState = without_key(
            KitOptionState {
                exposure: 0.3,
                ..Default::default()
            },
            "exposure",
        );
        assert_eq!(kit.exposure, NEUTRAL);

        // Door 3 — the FlatBuffers schema's own `= 1`, read off a table where the field is GENUINELY
        // ABSENT. That last word is the whole design of this leg, and the obvious version of it does
        // not work: encoding a populated state and reading it back is vacuous here, because the
        // generated builder omits a field only while it EQUALS the schema default — flip the schema
        // to `= 0` and the encoder starts writing the `1.0` it was handed, so the read still answers
        // `1.0` and the check passes through the very change it was meant to catch (measured).
        //
        // So the tables below are built from `..Default::default()` ARGS, which take the schema's
        // numbers and therefore write no multiplier at all. What comes back is the vtable's answer
        // for a missing field, which is the schema default and nothing else.
        let mut fbb = flatbuffers::FlatBufferBuilder::new();
        let kit = fb::KitOption::create(&mut fbb, &fb::KitOptionArgs::default());
        fbb.finish_minimal(kit);
        let kit = flatbuffers::root::<fb::KitOption>(fbb.finished_data())
            .expect("a defaulted KitOption table reads back");
        assert_eq!(kit.dispersion(), NEUTRAL);
        assert_eq!(kit.exposure(), NEUTRAL);
        // …and the sentinel pair stays at its own schema default, for the reason given above.
        assert_eq!(kit.attackMinBodyMass(), 0.0);
        assert_eq!(kit.attackMaxBodyMass(), 0.0);

        let mut fbb = flatbuffers::FlatBufferBuilder::new();
        let herd = fb::HerdTelemetryState::create(&mut fbb, &fb::HerdTelemetryStateArgs::default());
        fbb.finish_minimal(herd);
        let herd = flatbuffers::root::<fb::HerdTelemetryState>(fbb.finished_data())
            .expect("a defaulted HerdTelemetryState table reads back");
        assert_eq!(herd.stayFraction(), NEUTRAL);
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
        const PASTORAL_YIELD: f32 = 1.8;

        let snapshot = snapshot_with_herd(HerdTelemetryState {
            id: "herd_pen".to_string(),
            species: "Red Deer".to_string(),
            corralled: true,
            corral_yield: CORRAL_YIELD,
            pastoral_yield: PASTORAL_YIELD,
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
        assert!((herd.pastoralYield() - PASTORAL_YIELD).abs() < 1e-6);
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
    /// which the retired four ceiling rows could not express. **An inedible species is the case the
    /// pair still has to answer for** — a wolf reads `0` food, and what it is really worth is
    /// material batches this table does not carry, so the honest reading is a published zero rather
    /// than a missing field.
    #[test]
    fn the_per_biomass_yield_vector_rides_the_wire_on_both_webs() {
        const PATCH_FOOD_RATE: f32 = 0.058;
        const PATCH_FODDER_RATE: f32 = 0.004;

        let snapshot = WorldSnapshot {
            herds: vec![HerdTelemetryState {
                id: "herd_wolf".to_string(),
                provisions_per_biomass: 0.0,
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
            "a wolf is not food, and the wire says so rather than omitting the row"
        );

        let patch = subsistence.foragePatches().expect("patches present").get(0);
        assert!((patch.provisionsPerBiomass() - PATCH_FOOD_RATE).abs() < 1e-6);
        assert!((patch.fodderPerBiomass() - PATCH_FODDER_RATE).abs() < 1e-6);
    }

    /// **The FODDER CAPABILITY rides the wire beside the fodder RATE.** `fodderPerBiomass` (above)
    /// states what the *land* pays; `IntensificationKnowledgeState.foddering` states whether *this
    /// faction* may bank it — the wild forage fodder credit is gated on the Foddering discovery
    /// (`core_sim/src/systems/labor.rs`), so without this field a client composes a hay account the
    /// sim will refuse. Encode → decode through the generated reader, because a struct field that
    /// never reached the codec would still pass an in-process assertion.
    #[test]
    fn the_foddering_capability_rides_the_wire_beside_the_fodder_rate() {
        /// Partway to Foddering — a learning meter, so a bool would lose the reading.
        const FODDERING_PROGRESS: f32 = 0.75;
        /// A known rung-gate beside it, so the appended slot cannot be read off a neighbour's value.
        const PENNING_PROGRESS: f32 = 1.0;

        let snapshot = WorldSnapshot {
            intensification_knowledge: vec![IntensificationKnowledgeState {
                faction: 1,
                penning: PENNING_PROGRESS,
                foddering: FODDERING_PROGRESS,
                ..Default::default()
            }],
            ..WorldSnapshot::default()
        };

        let bytes = encode_snapshot_flatbuffer(&snapshot);
        let envelope = fb::root_as_envelope(&bytes).expect("snapshot decodes");
        let knowledge = envelope
            .payload_as_snapshot()
            .expect("snapshot payload")
            .subsistence()
            .expect("subsistence section present")
            .intensificationKnowledge()
            .expect("intensification knowledge present")
            .get(0);

        assert_eq!(knowledge.faction(), 1);
        assert!((knowledge.penning() - PENNING_PROGRESS).abs() < 1e-6);
        assert!(
            (knowledge.foddering() - FODDERING_PROGRESS).abs() < 1e-6,
            "the fodder capability must cross verbatim: {}",
            knowledge.foddering()
        );
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
                // A sown flax Field: pays fibre and nothing else, so every food term is zero while
                // the crew's throughput is real.
                per_worker_yield: 0.0,
                provisions_per_biomass: 0.0,
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

    /// **The ENGAGEMENT throughput crosses the wire, and a source with no engagement stage is
    /// distinguishable from one that has not stated it.**
    ///
    /// It is the third arm of the client's `min()` (`docs/plan_hunt_through_combat.md` §2): without it
    /// a compose sheet bounds a hunt by carry and stock alone and quotes a take the sim will never pay
    /// — measured at ~30× on a light-bodied species. The two readings are pinned **together** because
    /// they are the same number on the wire: a hunted herd's real rate, and the `0` a **pen** publishes
    /// for *"a penned animal is not stalked, drop this term"*.
    #[test]
    fn the_engagement_throughput_rides_the_wire_and_a_pen_states_it_has_none() {
        /// The shipped Wild Fowl rate — one hunter reaches ten birds a turn.
        const STALKED_ENGAGE_RATE: f32 = 10.0;
        /// The wire's finite reading of the sim's `f32::INFINITY`: no engagement stage at all.
        const NO_ENGAGEMENT_STAGE: f32 = 0.0;

        let snapshot = WorldSnapshot {
            herds: vec![
                HerdTelemetryState {
                    id: "herd_wild".to_string(),
                    engage_rate: STALKED_ENGAGE_RATE,
                    ..Default::default()
                },
                HerdTelemetryState {
                    id: "herd_pen".to_string(),
                    corralled: true,
                    engage_rate: NO_ENGAGEMENT_STAGE,
                    ..Default::default()
                },
            ],
            ..WorldSnapshot::default()
        };

        let bytes = encode_snapshot_flatbuffer(&snapshot);
        let envelope = fb::root_as_envelope(&bytes).expect("snapshot decodes");
        let herds = envelope
            .payload_as_snapshot()
            .expect("snapshot payload")
            .subsistence()
            .expect("subsistence section present")
            .herds()
            .expect("herds present");

        assert!(
            (herds.get(0).engageRate() - STALKED_ENGAGE_RATE).abs() < 1e-6,
            "a hunted herd publishes its real reach: {}",
            herds.get(0).engageRate()
        );
        assert_eq!(
            herds.get(1).engageRate(),
            NO_ENGAGEMENT_STAGE,
            "…and a pen publishes none, which a reader treats as unbounded"
        );
    }

    /// **THE FORECAST'S BAND CROSSES THE WIRE**
    /// (`docs/plan_hunt_through_combat.md` §6.4).
    ///
    /// A forecast has no event seed, so `actualYield` is the take's **expectation** and the band is
    /// what the invariant now claims contains it. Asserted on the **decoded** FlatBuffers rather than
    /// the in-process struct, for the reason `IntensificationKnowledgeState::foddering` already
    /// records: a field that never reached the codec still passes an in-process assertion.
    ///
    /// The **degenerate** row is pinned beside the widened one, because it is the shipped case —
    /// `wariness 0` and `hit_chance 1.0` make the band a point — and a reader must render one number
    /// there rather than a range of zero width.
    #[test]
    fn the_forecast_band_rides_the_wire() {
        /// A stochastic hunt: the point estimate with a band either side of it.
        const LIKELY_FOOD: f32 = 9.0;
        const LOW_FOOD: f32 = 6.0;
        const HIGH_FOOD: f32 = 11.0;
        /// The shipped case: no stochastic stage, so the band is the point estimate.
        const CERTAIN_FOOD: f32 = 4.0;

        let snapshot = WorldSnapshot {
            populations: vec![PopulationCohortState {
                entity: 1,
                labor_assignments: vec![
                    LaborAssignmentState {
                        kind: "hunt".to_string(),
                        actual_yield: LIKELY_FOOD,
                        actual_yield_low: LOW_FOOD,
                        actual_yield_high: HIGH_FOOD,
                        ..Default::default()
                    },
                    LaborAssignmentState {
                        kind: "hunt".to_string(),
                        actual_yield: CERTAIN_FOOD,
                        actual_yield_low: CERTAIN_FOOD,
                        actual_yield_high: CERTAIN_FOOD,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..WorldSnapshot::default()
        };

        let bytes = encode_snapshot_flatbuffer(&snapshot);
        let envelope = fb::root_as_envelope(&bytes).expect("snapshot decodes");
        let rows = envelope
            .payload_as_snapshot()
            .expect("snapshot payload")
            .population()
            .expect("population section present")
            .populations()
            .expect("cohorts present")
            .get(0)
            .laborAssignments()
            .expect("the assignments ride the cohort");

        let stochastic = rows.get(0);
        assert_eq!(
            (
                stochastic.actualYieldLow(),
                stochastic.actualYield(),
                stochastic.actualYieldHigh()
            ),
            (LOW_FOOD, LIKELY_FOOD, HIGH_FOOD),
            "the FOOD band and its point estimate must survive the codec"
        );

        let certain = rows.get(1);
        assert_eq!(
            (certain.actualYieldLow(), certain.actualYieldHigh()),
            (CERTAIN_FOOD, CERTAIN_FOOD),
            "the shipped roster has no stochastic stage, so its band is a point and a client renders \
             one number"
        );
    }

    /// **`fodderYield` crosses the wire** (issue #449) — the second account beside `actualYield`,
    /// without which a sown hay Field publishes its whole product as `+0.00`.
    ///
    /// Asserted on the **decoded** FlatBuffers rather than the in-process struct, for the reason
    /// `IntensificationKnowledgeState::foddering` already records: a field that never reached the
    /// codec still passes an in-process assertion. Pinned **beside** the food account it must not be
    /// confused with — a codec that read fodder off the food slot would pass a single-field check —
    /// and against a **hunt** row, whose `0.0` is structural (no animal pays fodder).
    #[test]
    fn the_feed_currency_rides_the_wire_beside_the_food_account() {
        /// A sown hay Field's whole product: no provisions, real fodder.
        const HAY_FODDER: f32 = 3.25;
        /// The hunt's food, non-zero so the food slot is distinguishable too.
        const HUNT_FOOD: f32 = 9.0;

        let snapshot = WorldSnapshot {
            populations: vec![PopulationCohortState {
                entity: 1,
                labor_assignments: vec![
                    LaborAssignmentState {
                        kind: "forage".to_string(),
                        fodder_yield: HAY_FODDER,
                        ..Default::default()
                    },
                    LaborAssignmentState {
                        kind: "hunt".to_string(),
                        actual_yield: HUNT_FOOD,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..WorldSnapshot::default()
        };

        let bytes = encode_snapshot_flatbuffer(&snapshot);
        let envelope = fb::root_as_envelope(&bytes).expect("snapshot decodes");
        let rows = envelope
            .payload_as_snapshot()
            .expect("snapshot payload")
            .population()
            .expect("population section present")
            .populations()
            .expect("cohorts present")
            .get(0)
            .laborAssignments()
            .expect("the assignments ride the cohort");

        let hay = rows.get(0);
        assert_eq!(
            (hay.actualYield(), hay.fodderYield()),
            (0.0, HAY_FODDER),
            "the two accounts must survive the codec in their own slots"
        );

        let hunt = rows.get(1);
        assert_eq!(
            (hunt.actualYield(), hunt.fodderYield()),
            (HUNT_FOOD, 0.0),
            "no animal pays fodder, so a hunt row's zero is structural rather than unset"
        );
    }

    /// **`durability` crosses the wire** (`docs/plan_hunt_through_combat.md` §4.2/§6.5) — the term
    /// that turns the combat gate from *"you cannot"* into *"you cannot, and here is how long it
    /// would take"*.
    ///
    /// Pinned **beside `defense`**, because the two blur and must not: defense is whether a hit counts
    /// at all, durability is how many counting hits it takes. A decoder that read one for the other
    /// would pass a single-field check.
    #[test]
    fn a_herd_publishes_the_gate_and_the_attrition_denominator_separately() {
        /// The shipped megafauna row: a hide that stops a bare hand, and a body that soaks 500.
        const MEGAFAUNA_DEFENSE: f32 = 12.0;
        const MEGAFAUNA_DURABILITY: f32 = 500.0;

        let snapshot = WorldSnapshot {
            herds: vec![HerdTelemetryState {
                id: "herd_mammoth".to_string(),
                defense: MEGAFAUNA_DEFENSE,
                durability: MEGAFAUNA_DURABILITY,
                ..Default::default()
            }],
            ..WorldSnapshot::default()
        };

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

        assert_eq!(
            herd.defense(),
            MEGAFAUNA_DEFENSE,
            "the gate crosses the wire"
        );
        assert_eq!(
            herd.durability(),
            MEGAFAUNA_DURABILITY,
            "…and so does the attrition denominator beside it"
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

    /// **The three shipped crop roles reach the client as three distinct words.**
    ///
    /// `FloraShareInfo::role` is a display tag the client cannot re-derive — the payoffs beside it
    /// are rung-2/rung-3 numbers and read `0` for a plant that cannot climb here — so a codec that
    /// dropped it, or wrote one row's string for every row, would be invisible until a tile card
    /// painted every crop with the same icon. Asserted on the **encoded envelope** rather than the
    /// state structs, because it is the encoding that has to preserve the distinction.
    #[test]
    fn the_three_crop_roles_survive_the_wire_distinctly() {
        let roles = [
            ("wild_emmer", "staple"),
            ("cotton", "cash"),
            ("hay_grass", "fodder"),
        ];
        let snapshot = WorldSnapshot {
            forage_patches: vec![ForagePatchState {
                composition: roles
                    .iter()
                    .map(|(species, role)| FloraShareInfo {
                        species: (*species).to_string(),
                        role: (*role).to_string(),
                        ..FloraShareInfo::default()
                    })
                    .collect::<Vec<_>>()
                    .into(),
                ..ForagePatchState::default()
            }],
            ..WorldSnapshot::default()
        };

        let bytes = encode_snapshot_flatbuffer(&snapshot);
        let envelope = fb::root_as_envelope(&bytes).expect("a decodable snapshot envelope");
        let composition = envelope
            .payload_as_snapshot()
            .expect("a snapshot payload")
            .subsistence()
            .expect("a subsistence section")
            .foragePatches()
            .expect("the forage patches")
            .get(0)
            .composition()
            .expect("the patch's composition");

        assert_eq!(composition.len(), roles.len());
        for (index, (species, role)) in roles.iter().enumerate() {
            let share = composition.get(index);
            assert_eq!(share.species(), Some(*species));
            assert_eq!(
                share.role(),
                Some(*role),
                "{species} must ship its own role, not a neighbour's"
            );
        }
    }

    /// **THE PER-MATERIAL CASH QUOTE SURVIVES THE WIRE, and EMPTY stays EMPTY** (arc #527).
    ///
    /// `sowMaterialPayoff` / `cultivateMaterialPayoff` replaced the retired `sowTradePayoff` /
    /// `cultivateTradePayoff` with a **vector**, because a material yield is *which material and how
    /// much*, not one number. Two properties have to survive the codec and neither is checkable in
    /// process:
    ///
    /// 1. **Each rung carries its OWN rows** — the two fields exist precisely because a Field's
    ///    managed rate and a tended patch's MSY skim are different harvests, so a codec that wrote
    ///    one vector into both slots would silently quote a Field's number on the Cultivate row,
    ///    which is the exact defect issue #419 fixed for the fodder account.
    /// 2. **An empty quote decodes as empty** — the "no row" reading the field's contract rests on.
    ///    A nested vector that failed to serialize decodes as *absent*, which is the same shape, so
    ///    the populated row beside it is what keeps this from passing blind.
    #[test]
    fn the_per_material_cash_quote_survives_the_wire_per_rung() {
        /// A Field is 100% its crop; the tended patch beside it is a weeded basket, so the two rungs
        /// legitimately quote different amounts of the same material.
        const SOW_FIBRE: f32 = 4.28;
        const CULTIVATE_FIBRE: f32 = 0.29;

        let snapshot = WorldSnapshot {
            forage_patches: vec![ForagePatchState {
                composition: vec![
                    FloraShareInfo {
                        species: "cotton".to_string(),
                        sow_material_payoff: vec![MaterialPayoff {
                            material_id: "fibre".to_string(),
                            amount: SOW_FIBRE,
                        }],
                        cultivate_material_payoff: vec![MaterialPayoff {
                            material_id: "fibre".to_string(),
                            amount: CULTIVATE_FIBRE,
                        }],
                        ..FloraShareInfo::default()
                    },
                    // A grain Field beside it: no material at all, and it must stay that way.
                    FloraShareInfo {
                        species: "wild_emmer".to_string(),
                        ..FloraShareInfo::default()
                    },
                ]
                .into(),
                ..ForagePatchState::default()
            }],
            ..WorldSnapshot::default()
        };

        let bytes = encode_snapshot_flatbuffer(&snapshot);
        let envelope = fb::root_as_envelope(&bytes).expect("a decodable snapshot envelope");
        let composition = envelope
            .payload_as_snapshot()
            .expect("a snapshot payload")
            .subsistence()
            .expect("a subsistence section")
            .foragePatches()
            .expect("the forage patches")
            .get(0)
            .composition()
            .expect("the patch's composition");

        let cotton = composition.get(0);
        let sow = cotton
            .sowMaterialPayoff()
            .expect("the Sow quote survives the codec");
        let cultivate = cotton
            .cultivateMaterialPayoff()
            .expect("the Cultivate quote survives the codec");
        assert_eq!((sow.len(), cultivate.len()), (1, 1));
        assert_eq!(sow.get(0).materialId(), Some("fibre"));
        assert_eq!(cultivate.get(0).materialId(), Some("fibre"));
        assert!((sow.get(0).amount() - SOW_FIBRE).abs() < 1e-6);
        assert!(
            (cultivate.get(0).amount() - CULTIVATE_FIBRE).abs() < 1e-6,
            "each rung must carry its OWN rows — a Field's number on the Cultivate row is the \
             defect the two fields exist to prevent"
        );

        let grain = composition.get(1);
        assert!(
            grain.sowMaterialPayoff().is_none_or(|rows| rows.is_empty()),
            "a food crop quotes NO ROW — never a zero-valued one"
        );
    }

    /// A species the roster does not name ships `""` — **unstated**, which a client must not read as
    /// `"staple"`. The empty-string convention is only worth anything if the encoder actually writes
    /// the field rather than leaving the slot absent.
    #[test]
    fn an_unstated_role_ships_as_an_empty_string_rather_than_a_default_category() {
        let snapshot = WorldSnapshot {
            forage_patches: vec![ForagePatchState {
                composition: vec![FloraShareInfo {
                    species: "a_plant_this_roster_forgot".to_string(),
                    ..FloraShareInfo::default()
                }]
                .into(),
                ..ForagePatchState::default()
            }],
            ..WorldSnapshot::default()
        };

        let bytes = encode_snapshot_flatbuffer(&snapshot);
        let envelope = fb::root_as_envelope(&bytes).expect("a decodable snapshot envelope");
        let role = envelope
            .payload_as_snapshot()
            .expect("a snapshot payload")
            .subsistence()
            .expect("a subsistence section")
            .foragePatches()
            .expect("the forage patches")
            .get(0)
            .composition()
            .expect("the patch's composition")
            .get(0)
            .role();

        assert_eq!(
            role,
            Some(""),
            "an unstated role is empty, never a category"
        );
    }
}
