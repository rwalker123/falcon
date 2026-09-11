//! The **saturated fixture snapshot** — one synthetic `WorldSnapshot` with every section, every
//! repeated field and every scalar leaf populated, shared by the two guards that need a world with
//! nothing left empty in it:
//!
//! - the **client decode guard** (`xtask/src/decode_fixture.rs` encodes it to the FlatBuffers
//!   envelope `tools/decode_guard.gd` reads through the real `SnapshotDecoder`), and
//! - the **codec round trip** (`codec/mod.rs`'s tests decode what the encoder wrote and require the
//!   result to re-encode byte-for-byte).
//!
//! It lives in this crate rather than in `xtask` because the round trip has to run where the codec
//! is (`docs/plan_ai_driver.md` §2: a partial decoder fails silently, and a section left empty in
//! the fixture is a section the round trip does not test). One builder, two consumers: a field
//! seeded here is covered by both gates, and one left unseeded fails both builds by name.
//!
//! ## Why synthetic rather than captured from a running server
//!
//! A capture would be *realistic* but is the wrong instrument twice over. It is **sparse** — an
//! early-game world carries no crisis gauges, no great discoveries, no trade links, no influencers,
//! so most of the decoders would go unexercised — and it is **unstable**, because this repo retunes
//! worldgen constantly (biome palette, climate, rivers), so a capture-derived golden would churn on
//! every tuning change and train reviewers to accept the diff blind. A synthetic snapshot is
//! deterministic, regenerable, and can make *every* section non-empty.
//!
//! ## How a value is chosen: saturation
//!
//! Hand-writing a literal for ~60 state structs would go stale the moment a field is appended — and
//! "the decoder silently dropped an appended field" is this codebase's most-repeated client bug. So
//! the fixture is built in two steps:
//!
//! 1. **Seed** — every `Vec` gets elements (this is the one thing serde cannot do for us: an empty
//!    array carries no element type). [`assert_no_empty_arrays`] fails the build, naming the JSON
//!    path, if a newly appended repeated field is left unseeded.
//! 2. **Saturate** — the snapshot is serialized to JSON and every *scalar leaf* is replaced with a
//!    deterministic, path-derived sentinel, then deserialized back. An appended scalar field is
//!    therefore populated automatically, forever, with no edit here.
//!
//! A **string** leaf becomes its own JSON path (`"populations[0].id"`), which is the highest-value
//! property of the whole scheme: a decoded value then says, in plain text, which wire field it came
//! from, so an accessor wired to the wrong field is legible in a diff rather than merely different.
//!
//! A **boolean** leaf alternates on the parity of its path, so two flags in one table disagree and
//! a bool↔bool swap in a decoder moves the bytes.
//!
//! Two rules keep saturation type-safe:
//! - A string leaf is replaced **only when its default is empty**. Rust `String` fields default to
//!   `""`; a serde-serialized enum defaults to a *variant name*, which is non-empty — so this one
//!   test distinguishes "free text" from "enum" without a field list.
//! - Integers stay in `1..=200` so a `u8` field cannot fail to deserialize. Distinctness comes from
//!   the string paths; the integers only need to be non-default.
//!
//! Structural fields (grid dimensions, raster lengths, tile coordinates, terrain enums) are set in
//! typed Rust *after* saturation, since a path-derived sentinel there would put a tile outside the
//! grid or a raster out of step with its width.

use crate::state::campaign::*;
use crate::state::culture::*;
use crate::state::governance::*;
use crate::state::map::*;
use crate::state::population::*;
use crate::state::subsistence::{
    CharacteristicBandState, CraftKnowledgeState, FloraShareInfo, LadderKnowledgeProgress,
    LadderKnowledgeState, MaterialDefState, RecipeDefState, RecipeInputState, RecipeOutputState,
    SpeciesMaterialRates,
};
use crate::world::WorldSnapshot;
use std::error::Error;

/// The fixture's grid. Deliberately tiny: the golden records rasters by summary rather than by
/// sample, but a small grid keeps the envelope a few KB and the failure output readable.
pub const GRID_W: u32 = 4;
pub const GRID_H: u32 = 3;
pub const GRID_CELLS: usize = (GRID_W * GRID_H) as usize;

/// Rows per repeated section. Two, not one: a per-row sentinel varies with the row index, so a
/// builder that returns the first element for every row (or reuses one offset) is visible.
pub const ROWS: usize = 2;

/// ⛔ **A ROW'S X AND Y MUST NEVER BE EQUAL.** The offset the structural fixups give a row's `y`
/// over its `x`.
///
/// Without it the two coordinates are the same number on every row of a two-row section — `i %
/// GRID_W` and `i % GRID_H` both reduce to `i` for `i` in `{0, 1}` — and a decoder that reads
/// `y()` into `x` and `x()` into `y` round-trips **byte-identically**: the swap is invisible to
/// both gates. One is the smallest offset that separates them and still lands inside a 4×3 grid
/// for every row.
const ROW_Y_OFFSET: u32 = 1;

/// The start marker's tile — the one located row that is not part of a repeated section, so it
/// carries its own pair rather than a row index. Distinct for the reason [`ROW_Y_OFFSET`] exists.
const START_MARKER_X: u32 = 1;
const START_MARKER_Y: u32 = 2;

/// **THE TWO RANKS A BENCH CAN CARRY HERE — ONE PER COHORT, AND `Normal` IS NOT AMONG THEM.**
///
/// A cohort has exactly **one** bench, so a fixture cannot cycle a bench through the three ranks the
/// way it cycles the labor rows; it can only give each cohort's bench a different one. These two are
/// the **non-default** ranks, which is what a wrong mapping can move: `Normal` is the decoder's own
/// catch-all arm, so a bench carrying it is indistinguishable from a bench whose rank was dropped —
/// and it is covered where that distinction *is* observable, on the labor rows below
/// ([`EVERY_SOURCE_PRIORITY`]), which is the codec-level coverage this array leans on.
pub const BENCHED_SOURCE_PRIORITIES: [SourcePriorityState; 2] =
    [SourcePriorityState::High, SourcePriorityState::Low];

/// **EVERY `SourcePriority` THE WIRE CAN CARRY**, one per fixture labor row — the one repeated
/// section that is deliberately sized by an *enum* rather than by [`ROWS`].
///
/// An enum survives saturation (its serialized form is a variant name, which is not the empty
/// default a string leaf is replaced on), so the only way a non-default variant reaches the decoder
/// is for it to be written here. Covering all three end to end is what makes a mis-mapped arm move
/// the golden.
pub const EVERY_SOURCE_PRIORITY: [SourcePriorityState; 3] = [
    SourcePriorityState::Normal,
    SourcePriorityState::High,
    SourcePriorityState::Low,
];

/// What a ladder knowledge's `display_name` is built from, so it is never the `knowledge_id`
/// verbatim — see the roster in `seed_snapshot`.
const LADDER_DISPLAY_NAME_PREFIX: &str = "the knowledge of ";

/// The length of the seeded `regrowthSamples` curve — the **shipped** sample count
/// (`core_sim::snapshot::REGROWTH_CURVE_SAMPLES`), restated here rather than imported because
/// `core_sim` depends on this crate and not the other way round. It only has to be a plausible
/// non-empty length: saturation overwrites every value, and the guard is about the field being
/// *present and repeated*.
const REGROWTH_CURVE_SAMPLES: usize = 11;

/// ⛔ **BOTH READINGS OF AN OPTIONAL FIELD MUST BE ON THE WIRE.** Whether a row seeds its
/// `Option` fields: the even rows do, the odd rows leave them `None`.
///
/// An optional field that is `Some` on **every** row never encodes its absent form, so a decoder
/// that drops the presence flag — `hasOwner`, `hasFreshnessWindow`, or a `-1` sentinel — round
/// trips clean while reading every row wrong; one that is `None` on every row leaves the present
/// form untested the same way (`build_destination_capacity` was `None` everywhere, so
/// `decode_build_destination_capacity` was only ever handed the sentinel and a body returning
/// `None` unconditionally passed). With [`ROWS`] = 2, seeding on the even rows puts each optional
/// field on the wire both ways.
fn seeded_on(row: usize) -> bool {
    row.is_multiple_of(2)
}

/// Seed → saturate → fix up. See the module docs for why it is done in that order.
pub fn saturated_snapshot() -> Result<WorldSnapshot, Box<dyn Error>> {
    let seeded = seed_snapshot();

    let mut value = serde_json::to_value(&seeded)?;
    assert_no_empty_arrays(&value, "")?;
    saturate(&mut value, "");

    let mut snapshot: WorldSnapshot = serde_json::from_value(value)?;
    apply_structural_fixups(&mut snapshot);
    Ok(snapshot)
}

// ---------------------------------------------------------------------------
// Step 1 — seed every repeated field
// ---------------------------------------------------------------------------

fn rows<T: Default + Clone>() -> Vec<T> {
    vec![T::default(); ROWS]
}

fn rows_of<T: Clone>(blank: T) -> Vec<T> {
    vec![blank; ROWS]
}

// ---------------------------------------------------------------------------
// Blanks for the state structs that do not derive `Default`
// ---------------------------------------------------------------------------
//
// The values here are placeholders — saturation overwrites every one of them. What matters is that
// each literal is **exhaustive** (no `..Default::default()`), so appending a field to one of these
// structs breaks *this* build and whoever appended it is told, at compile time, that the client
// decode gate needs to know about it. That is the same forcing function `assert_no_empty_arrays`
// provides for repeated fields, bought here for free because these structs have no `Default` to
// fall back on.

fn blank_tile() -> TileState {
    TileState {
        entity: 0,
        x: 0,
        y: 0,
        element: 0,
        temperature: 0,
        terrain: TerrainType::AlluvialPlain,
        terrain_tags: TerrainTags::empty(),
        culture_layer: 0,
        mountain_kind: MountainKind::Fold,
        mountain_relief: 0.0,
        habitability: 0,
        river_edges: 0,
        river_inflow: 0,
        river_channel: 0,
        graze_biomass: 0.0,
        graze_capacity: 0.0,
        graze_ecology_phase: 0,
        forage_capacity: 0.0,
        underlying_terrain: TerrainType::AlluvialPlain,
    }
}

fn blank_generation() -> GenerationState {
    GenerationState {
        id: 0,
        name: String::new(),
        bias_knowledge: 0,
        bias_trust: 0,
        bias_equity: 0,
        bias_agency: 0,
    }
}

fn blank_power_node() -> PowerNodeState {
    PowerNodeState {
        entity: 0,
        node_id: 0,
        generation: 0,
        demand: 0,
        efficiency: 0,
        storage_level: 0,
        storage_capacity: 0,
        stability: 0,
        surplus: 0,
        deficit: 0,
        incident_count: 0,
    }
}

fn blank_culture_trait() -> CultureTraitEntry {
    CultureTraitEntry {
        axis: CultureTraitAxis::OpenClosed,
        baseline: 0,
        modifier: 0,
        value: 0,
    }
}

fn blank_culture_layer() -> CultureLayerState {
    CultureLayerState {
        id: 0,
        owner: 0,
        parent: 0,
        scope: CultureLayerScope::Regional,
        traits: Vec::new(),
        divergence: 0,
        soft_threshold: 0,
        hard_threshold: 0,
        ticks_above_soft: 0,
        ticks_above_hard: 0,
        last_updated_tick: 0,
    }
}

fn blank_culture_tension() -> CultureTensionState {
    CultureTensionState {
        layer_id: 0,
        scope: CultureLayerScope::Local,
        owner: 0,
        severity: 0,
        timer: 0,
        kind: CultureTensionKind::SchismRisk,
    }
}

fn blank_culture_resonance() -> InfluencerCultureResonanceEntry {
    InfluencerCultureResonanceEntry {
        axis: CultureTraitAxis::SecularDevout,
        weight: 0,
        output: 0,
    }
}

fn blank_influencer() -> InfluentialIndividualState {
    InfluentialIndividualState {
        id: 0,
        name: String::new(),
        influence: 0,
        growth_rate: 0,
        baseline_growth: 0,
        notoriety: 0,
        sentiment_knowledge: 0,
        sentiment_trust: 0,
        sentiment_equity: 0,
        sentiment_agency: 0,
        sentiment_weight_knowledge: 0,
        sentiment_weight_trust: 0,
        sentiment_weight_equity: 0,
        sentiment_weight_agency: 0,
        logistics_bonus: 0,
        morale_bonus: 0,
        power_bonus: 0,
        logistics_weight: 0,
        morale_weight: 0,
        power_weight: 0,
        support_charge: 0,
        suppress_pressure: 0,
        domains: 0,
        scope: InfluenceScopeKind::Generation,
        generation_scope: 0,
        supported: false,
        suppressed: false,
        lifecycle: InfluenceLifecycle::Active,
        coherence: 0,
        ticks_in_status: 0,
        audience_generations: Vec::new(),
        support_popular: 0,
        support_peer: 0,
        support_institutional: 0,
        support_humanitarian: 0,
        weight_popular: 0,
        weight_peer: 0,
        weight_institutional: 0,
        weight_humanitarian: 0,
        culture_resonance: Vec::new(),
    }
}

/// Every `Vec` on the snapshot gets elements. Rasters are sized to the grid rather than to [`ROWS`]
/// — their length is structural, not decorative.
///
/// If you append a repeated field and forget it here, [`assert_no_empty_arrays`] names its path.
fn seed_snapshot() -> WorldSnapshot {
    // `WorldSnapshot`'s derived `Default` gives `false` for `fog_enabled` while the schema default is
    // `true`, so state it explicitly rather than letting the fixture claim fog of war is off.
    let mut s = WorldSnapshot {
        fog_enabled: true,
        ..Default::default()
    };

    // Every field is `Option` + `skip_serializing_if`, so a `None` never reaches the JSON and
    // saturation cannot reach it — and `create_campaign_label` drops an all-`None` label outright,
    // which is how the whole section went missing from the first recorded golden.
    s.header.campaign_label = Some(CampaignLabel {
        profile_id: Some(String::new()),
        title: Some(String::new()),
        title_loc_key: Some(String::new()),
        subtitle: Some(String::new()),
        subtitle_loc_key: Some(String::new()),
    });

    // --- map -------------------------------------------------------------
    s.tiles = vec![blank_tile(); GRID_CELLS];
    s.terrain.samples = vec![TerrainSample::default(); GRID_CELLS];
    s.elevation_overlay.samples = vec![0u16; GRID_CELLS];
    s.moisture_raster.samples = vec![0.0f32; GRID_CELLS];
    s.start_marker = Some(StartMarkerState::default());

    // --- vision / scalar rasters -----------------------------------------
    for raster in [
        &mut s.sentiment_raster,
        &mut s.corruption_raster,
        &mut s.culture_raster,
        &mut s.military_raster,
        &mut s.visibility_raster,
    ] {
        raster.samples = vec![0i64; GRID_CELLS];
    }

    // --- economy ---------------------------------------------------------
    s.faction_inventory = rows();
    for inv in &mut s.faction_inventory {
        inv.inventory = rows();
    }

    // --- population ------------------------------------------------------
    s.populations = rows();
    for (row, cohort) in s.populations.iter_mut().enumerate() {
        cohort.stores = rows();
        // **The TOE, one row per item.** `rows()` would give every row the same default id, and a
        // list keyed by `item_id` with duplicate keys is not a thing the server can emit — so the
        // ids are spelled out. They are the shipped item table, which is what a client reading this
        // golden will actually be handed.
        cohort.kit_item_conditions = ["spears", "sled", "baskets", "traps"]
            .iter()
            .enumerate()
            .map(|(index, item)| KitItemConditionState {
                item_id: (*item).to_string(),
                // Distinct per row, and one of them DRY, so the golden exercises both sides of the
                // cliff rather than recording four healthy numbers.
                remaining: if index == 1 {
                    0.0
                } else {
                    90.0 - index as f32 * 7.5
                },
                // **Ownership, stated.** The dry row above owns none — `remaining 0` means "owns
                // none" since the count slice — so the golden carries both readings of a zero.
                count: if index == 1 { 0 } else { 1 },
                // **And how many people it reaches** — a unit arms a worker, so a row the band owns
                // one of still has to say whether anybody is holding it. Distinct from `count` here
                // deliberately: a golden where the two matched would pass a decoder that read
                // either field for the other.
                workers_holding: if index == 1 { 0.0 } else { 3.0 },
                // **Its denominator** — a third distinct value, so a golden where a decoder read
                // `count` or `workersHolding` for this field would not pass. The dry row is the
                // "staffed job, nobody holding it" reading (5 on the job, 0 holding), which is the
                // one a client must render as a shortfall.
                workers_on_quoted_job: 5.0,
            })
            .collect();
        // **The per-band resolved tiers**, one row per shipped kit. Spelled out for the same reason
        // the item conditions above are: a list keyed by `kit_id` cannot carry duplicate keys, which
        // is what `rows()` would produce. The values are inert here — saturation overwrites them —
        // but the row COUNT and the ids are what a client decodes against.
        cohort.kit_tiers = ["big_game", "trapping", "gathering", "none"]
            .iter()
            .map(|kit| BandKitTiersState {
                kit_id: (*kit).to_string(),
                ..Default::default()
            })
            .collect();
        // **The partly-equipped party's own division.** TWO rows, and they must not be identical:
        // a golden with one crew would let a decoder that read only the first row pass, which is
        // the exact failure `huntCrews` exists to prevent (`hunterAttack` alone is the best crew's
        // answer). The inner `item_ids` is a repeated field inside a repeated field, so the armed
        // row carries elements or the guard never decodes one.
        cohort.hunt_crews = vec![
            BandKitCrewState {
                workers: 6.0,
                hunter_attack: 20.0,
                item_ids: vec!["spears".to_string(), "sled".to_string()],
            },
            BandKitCrewState {
                workers: 4.0,
                hunter_attack: 1.0,
                item_ids: vec!["sled".to_string()],
            },
        ];
        // --- CRAFTING & MATERIALS ------------------------------------------------------------
        // Every one of these is a repeated field inside a repeated field, so both levels need
        // elements or the guard never exercises the inner ones. Saturation overwrites the values;
        // what matters here is the SHAPE and the ids, which cannot be `rows()` for the same reason
        // the item conditions above cannot: a list keyed by id has no duplicate keys.
        cohort.material_batches = ["hide", "fibre", "bone"]
            .iter()
            .map(|material| MaterialBatchState {
                material_id: (*material).to_string(),
                readings: vec![CharacteristicReadingState::default(); 2],
                ..Default::default()
            })
            .collect();
        cohort.bench = BenchState {
            shortfalls: rows_of(MaterialShortfallState::default()),
            // The pile already cut for the job in flight — a repeated field inside a repeated field
            // like the shortfalls above it, so it needs elements or the guard never decodes a row.
            drawn_inputs: rows_of(DrawnInputState::default()),
            ..Default::default()
        };
        // **One row per recipe, always** — the contract this field exists for, so the fixture
        // carries a real book's worth rather than one row.
        cohort.craft_offers = ["sled", "baskets", "spears", "loom"]
            .iter()
            .map(|recipe| CraftOfferState {
                recipe_id: (*recipe).to_string(),
                shortfalls: rows_of(MaterialShortfallState::default()),
                ..Default::default()
            })
            .collect();
        cohort.equipment_batches = ["spears", "sled", "baskets", "loom"]
            .iter()
            .map(|item| EquipmentBatchState {
                item_id: (*item).to_string(),
                ..Default::default()
            })
            .collect();
        // **THE BAND'S OWN BUILD QUEUE** (`docs/plan_standing_upkeep.md` §4.9 item 9a) — a repeated
        // field on the cohort, seeded for the reason every other one here is: an empty vector
        // carries no element type, so the decode guard would never see the field at all. The rank
        // is the INDEX, so the golden's element ORDER is part of what it pins.
        cohort.build_queue = rows();
        // **ONE ROW PER `SourcePriority`, WHICH IS WHY THIS LIST IS NOT `rows()`.**
        //
        // The rank is a serde *enum*, so saturation leaves it alone (a variant name is a non-empty
        // string — see the module docs' two saturation rules), and `Default` is `Normal`. A fixture
        // built from `rows()` therefore exercised the decoder's **default arm only**: the client's
        // mapping ends in a `_ => "normal"` catch-all, so a `High` or `Low` arm wired to the wrong
        // word would decode as `"normal"` and the golden would not move.
        //
        // **It was invisible for a reason worth stating: `Normal` is wire value `0`**, and a
        // FlatBuffers scalar equal to its default costs no bytes — so the fixture `.bin`s did not
        // change at all when the field was appended, and neither did the golden.
        //
        // The three rows are the whole enum, in declaration order, so a variant added to
        // `SourcePriorityState` without a row here is a variant this guard does not cover.
        cohort.labor_assignments = EVERY_SOURCE_PRIORITY
            .iter()
            .map(|priority| LaborAssignmentState {
                priority: *priority,
                ..Default::default()
            })
            .collect();
        for assignment in &mut cohort.labor_assignments {
            assignment.arrival_schedule = vec![0.0f32; 4];
            // The row's MATERIAL account (arc #527) — a nested repeated field, seeded for the same
            // reason `arrival_schedule` is: an empty one is a field the decode guard cannot see.
            assignment.material_yield = rows();
            // …and the good-side shortfall pair (the material half of the standing upkeep), for the
            // same reason: two nested repeated fields the guard cannot see while they are empty.
            assignment.material_upkeep_demand = rows();
            assignment.material_upkeep_supplied = rows();
            // **WHICH PLANTS THE CREW CARRIES HOME** (the selective gather) — a `[string]`, seeded
            // for the same reason: a repeated field the fixture leaves empty is a field the decode
            // guard cannot exercise.
            //
            // **SORTED, because the sim can emit it no other way.** `TakeSelection` wraps a
            // `BTreeSet` and the capture publishes its keys, so a golden baked with the pair the
            // other way round would show a client author an ordering the schema promises cannot
            // happen — and invite exactly the re-sort it forbids.
            assignment.take_species = vec!["flax".to_string(), "wild_emmer".to_string()];
        }
        // **A trade party's shipment** (arc #527) — a repeated field on the cohort, seeded for the
        // same reason the assignment's material account above is: an empty one is a field the decode
        // guard cannot see.
        cohort.expedition_cargo_materials = rows();
        // **The band's STANDING MATERIAL BILL** — three repeated fields, seeded for the same reason.
        cohort.material_upkeep_need = rows();
        cohort.material_upkeep_income = rows();
        cohort.material_store = rows();
        cohort.pending_reveal_x = vec![0u32; ROWS];
        cohort.pending_reveal_y = vec![0u32; ROWS];
        cohort.knowledge_fragments = rows();
        // The cohort's optional tables, on the even rows only — see [`seeded_on`].
        cohort.migration = seeded_on(row).then(|| PendingMigrationState {
            fragments: rows(),
            ..Default::default()
        });
        cohort.harvest_task = seeded_on(row).then(HarvestTaskState::default);
        cohort.scout_task = seeded_on(row).then(ScoutTaskState::default);
        cohort.accessible_stockpile = seeded_on(row).then(|| AccessibleStockpileState {
            entries: rows(),
            ..Default::default()
        });
        // **THIS BAND'S OUTFITTING WINDOW** — four repeated fields inside a table inside a repeated
        // field, so each needs elements or the guard never decodes a row of it. The two supplies are
        // the TAKE half and are spelled out for the reason every id-keyed list here is: a list keyed
        // by id cannot carry the duplicate keys `rows()` would produce.
        //
        // `parent_band_id` is left at its `0` default, which is the GRANT reading — the take arm is
        // covered by the supplies below carrying rows regardless, since the decoder emits both
        // unconditionally and the golden records whichever it wrote.
        cohort.loadout_window = seeded_on(row).then(|| BandLoadoutWindowState {
            kits: rows(),
            materials: rows(),
            parent_item_supply: ["spears", "sled"]
                .iter()
                .map(|item| BandLoadoutSupplyRowState {
                    id: (*item).to_string(),
                    ..Default::default()
                })
                .collect(),
            parent_material_supply: ["hide", "fibre"]
                .iter()
                .map(|material| BandLoadoutSupplyRowState {
                    id: (*material).to_string(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        });
    }
    s.generations = rows_of(blank_generation());
    s.demographics = rows();

    // --- subsistence -----------------------------------------------------
    s.herds = rows();
    for (row, herd) in s.herds.iter_mut().enumerate() {
        // **The `-1` sentinel is only half the field.** `None` crosses as
        // `NO_BUILD_DESTINATION_CAPACITY`, so a fixture that never seeds this hands
        // `decode_build_destination_capacity` nothing but the sentinel and a body returning `None`
        // unconditionally passes. Seeded on the even rows — see [`seeded_on`]; the value is
        // overwritten by saturation, which never produces the sentinel (its floats are ≥ 1.0).
        herd.build_destination_capacity = seeded_on(row).then(f32::default);
        // The two pre-launch estimate tables that used to be seeded here are retired: the client
        // asks for a forecast now (`sim_runtime`'s `QueryCommand`) instead of reading a table the
        // capture pre-computed for every herd on every frame.
        // The sampled regrowth curve — a `[float]`, so it needs seeding like every other repeated
        // field or the decode guard cannot see it. Saturation overwrites the values; only the LENGTH
        // matters here, and it is the shipped one so the fixture exercises a real-shaped curve.
        herd.regrowth_samples = vec![0.0; REGROWTH_CURVE_SAMPLES];
        // What a hunt of this herd is MADE OF (arc #527) — two nested repeated fields, seeded for
        // the same reason the curve above is.
        herd.material_per_biomass = rows();
        herd.per_worker_material = rows();
        // …and the two investment rungs' material payoffs.
        herd.corral_material = rows();
        herd.pastoral_material = rows();
        // …and the MATERIAL half of the ladder's price: the pile the next rung eats, the rate this
        // one costs to hold, and what the store paid toward it.
        herd.build_material_cost = rows();
        herd.upkeep_material_demand = rows();
        herd.upkeep_material_supplied = rows();
        // …and the per-rung PRE-COMMIT quote pair, which is a different question from the stamped
        // bill above and therefore a different pair of nested repeated fields.
        herd.tame_upkeep_material_demand = rows();
        herd.corral_upkeep_material_demand = rows();
        // …and the ring's own pile, which is the BUILD twin of that pair: `build_material_cost`
        // above prices the rung *above* a source, and a ring is only ever offered on a herd already
        // at the top of its branch.
        herd.corral_build_material_cost = rows();
        // The animal twin of `ForagePatchState.build_legs` — one leg on this web today, and seeded
        // for the same reason: a repeated field the fixture leaves empty is a field the decode guard
        // cannot exercise.
        herd.build_legs = rows();
    }
    s.food_modules = rows();
    // **The kit roster**, and each entry's `jobs` / `item_ids` — repeated fields inside a repeated
    // field, so both levels need elements or the decode guard never exercises the inner ones.
    s.kits = rows();
    for option in &mut s.kits {
        option.jobs = vec![String::new(); ROWS];
        // Which items the kit carries. Seeded for the reason every list here is: an empty one is
        // indistinguishable from a field that never reached the client, and this one exists
        // precisely so a durability readout stops guessing the gear from the tiers.
        option.item_ids = vec![String::new(); ROWS];
    }
    // The three per-world crafting catalogues and the learned one. `axes` / `requires_knowledge` /
    // `inputs` / `outputs` are repeated fields inside repeated fields, so each needs elements.
    s.materials = ["hide", "fibre", "bone"]
        .iter()
        .map(|material| MaterialDefState {
            id: (*material).to_string(),
            axes: vec![String::new(); 2],
            ..Default::default()
        })
        .collect();
    s.characteristic_bands = rows_of(CharacteristicBandState::default());
    s.recipes = ["sled", "baskets", "spears", "loom"]
        .iter()
        .map(|recipe| RecipeDefState {
            id: (*recipe).to_string(),
            requires_knowledge: vec![String::new(); 2],
            inputs: vec![RecipeInputState::default(); 2],
            outputs: vec![RecipeOutputState::default(); 1],
            ..Default::default()
        })
        .collect();
    s.craft_knowledge = ["tanning", "weaving", "bone_working"]
        .iter()
        .map(|craft| CraftKnowledgeState {
            craft_id: (*craft).to_string(),
            ..Default::default()
        })
        .collect();
    s.sedentarization = rows();
    s.forage_patches = rows();
    for (row, patch) in s.forage_patches.iter_mut().enumerate() {
        // The patch's two optional readings, on the even rows only — see [`seeded_on`]. `owner`
        // rides a `hasOwner` flag and the capacity rides the `-1` sentinel, and a decoder that
        // dropped either would round trip clean on a fixture that only ever seeds one side.
        patch.owner = seeded_on(row).then(u32::default);
        patch.build_destination_capacity = seeded_on(row).then(f32::default);
        let mut composition = rows::<FloraShareInfo>();
        for share in &mut composition {
            // The crop picker's PER-MATERIAL cash quote (arc #527) — a nested repeated field, so it
            // is seeded for the same reason `regrowth_samples` is: an empty one is a field the
            // decode guard cannot exercise.
            share.sow_material_payoff = rows();
            share.cultivate_material_payoff = rows();
        }
        // **How much of each plant is standing** (the selective gather) — index-aligned with the
        // basket above, so it is seeded to the SAME length rather than to `ROWS`: a fixture whose
        // two vectors disagreed would encode a shape the capture cannot produce.
        patch.composition_standing_biomass = vec![0.0; composition.len()];
        // …and the two per-species conversion rates, index-aligned with the same basket for the same
        // reason (the selective gather's pre-commit sheet composes all three together).
        patch.composition_provisions_per_biomass = vec![0.0; composition.len()];
        patch.composition_fodder_per_biomass = vec![0.0; composition.len()];
        // …and the per-species MATERIAL rows, one entry per basket entry with real rows inside, so
        // the guard exercises the nested vector rather than a column of empty tables.
        patch.composition_material_per_biomass = composition
            .iter()
            .map(|_| SpeciesMaterialRates { rows: rows() })
            .collect();
        // Shared on the state struct (a tile's basket, not a frame's), so the fixture's rows are
        // handed over as one — the encoded bytes are identical either way.
        patch.composition = composition.into();
        // What a gather of this patch is MADE OF (arc #527) — two nested repeated fields, seeded for
        // the same reason the curve below is.
        patch.material_per_biomass = rows();
        patch.per_worker_material = rows();
        // The MATERIAL half of the ladder's price — the pile the next rung eats and the rate this
        // one costs to hold, plus what the store paid. Three more nested repeated fields.
        patch.build_material_cost = rows();
        patch.upkeep_material_demand = rows();
        patch.upkeep_material_supplied = rows();
        // …and the per-rung PRE-COMMIT quote pair — see the herd twin.
        patch.cultivation_upkeep_material_demand = rows();
        patch.field_upkeep_material_demand = rows();
        // The TILE's per-rung vector (#426) — the plant twin of `hunt_policy_ceilings` above, and
        // seeded for the same reason: a repeated field the fixture leaves empty is a field the decode
        // guard cannot exercise, which is how four appended fields reached the client as zeros.
        patch.regrowth_samples = vec![0.0; REGROWTH_CURVE_SAMPLES];
        // **THE LEGS OF A QUEUE ENTRY'S CLIMB** — a repeated field, seeded for the same reason the
        // curve above is. A `sow` on untended ground is a two-leg climb, which is what the fixture
        // stands for here.
        patch.build_legs = rows();
    }
    // **THE LADDER'S KNOWLEDGE ROSTER** (what there is to learn) and the per-faction PROGRESS list
    // beside it. Both are seeded with real ids rather than defaulted rows, because the roster's
    // whole job is to name knowledges and a column of empty strings cannot show the join working.
    s.ladder_knowledge = ["cultivation", "herding", "roadbuilding"]
        .iter()
        .map(|knowledge| LadderKnowledgeState {
            knowledge_id: (*knowledge).to_string(),
            // **Not the id.** Two string leaves carrying the same literal are one wire path
            // between them: swapping `knowledgeId` and `displayName` in the decoder would round
            // trip clean. The prefix keeps them apart while still reading as this row's name.
            display_name: format!("{LADDER_DISPLAY_NAME_PREFIX}{knowledge}"),
            branch: "plant".to_string(),
            ..Default::default()
        })
        .collect();
    s.intensification_knowledge = rows();
    for row in &mut s.intensification_knowledge {
        // Index-aligned with the roster above, so the fixture encodes the shape the capture
        // produces: sparse in VALUE, never in MEMBERSHIP.
        row.knowledges = s
            .ladder_knowledge
            .iter()
            .map(|entry| LadderKnowledgeProgress {
                knowledge_id: entry.knowledge_id.clone(),
                ..Default::default()
            })
            .collect();
    }

    // **THE ROUTE BRANCH'S RUNG CATALOG** — what a road may become, once per world. Seeded for the
    // reason every repeated field here is: an empty vector is a field the decode guard cannot
    // exercise, which is how an appended field reaches the client as nothing at all.
    s.route_rungs = rows();

    // --- connections -----------------------------------------------------
    // The contact primitive's own section (arc #527). Seeded for the reason every repeated
    // field here is: an empty vector is a field the decode guard cannot exercise.
    s.connections = rows();

    // --- routes ----------------------------------------------------------
    // The roads in the ground (arc #532), **one row per tile**. Seeded for the same reason every
    // repeated field here is: an empty vector is a field the decode guard cannot exercise.
    //
    // **There are no nested repeated fields on this row any more.** The stored `path_x`/`path_y`
    // halves went with the path object — a road is a per-tile improvement, so the row carries its
    // own `tile_x`/`tile_y` scalars and nothing to walk.
    s.routes = rows();

    // --- knowledge -------------------------------------------------------
    s.discovered_sites = rows();
    for entry in &mut s.discovered_sites {
        entry.sites = rows();
    }
    s.discovery_progress = rows();
    s.knowledge_ledger = rows();
    for entry in &mut s.knowledge_ledger {
        entry.countermeasures = rows();
        entry.infiltrations = rows();
        entry.modifiers = rows();
        for (row, modifier) in entry.modifiers.iter_mut().enumerate() {
            modifier.note_handle = seeded_on(row).then(String::new);
        }
    }
    s.knowledge_timeline = rows();
    for (row, event) in s.knowledge_timeline.iter_mut().enumerate() {
        event.note_handle = seeded_on(row).then(String::new);
    }
    s.great_discovery_definitions = rows();
    for (row, def) in s.great_discovery_definitions.iter_mut().enumerate() {
        // The definition's optional half, on the even rows only — see [`seeded_on`].
        // `freshness_window` rides a `hasFreshnessWindow` flag, so its absent reading is only
        // decoded because one row leaves it out.
        def.tier = seeded_on(row).then(String::new);
        def.summary = seeded_on(row).then(String::new);
        def.tags = vec![String::new(); ROWS];
        def.freshness_window = seeded_on(row).then(u16::default);
        def.effects_summary = vec![String::new(); ROWS];
        def.observation_notes = seeded_on(row).then(String::new);
        def.leak_profile = seeded_on(row).then(String::new);
        def.requirements = rows();
        for (row, req) in def.requirements.iter_mut().enumerate() {
            req.name = seeded_on(row).then(String::new);
            req.summary = seeded_on(row).then(String::new);
        }
    }
    s.great_discoveries = rows();
    s.great_discovery_progress = rows();

    // --- governance ------------------------------------------------------
    s.power = rows_of(blank_power_node());
    s.power_metrics.incidents = rows();
    s.crisis_telemetry.gauges = rows();
    for gauge in &mut s.crisis_telemetry.gauges {
        gauge.history = rows();
    }
    s.crisis_overlay.heatmap.samples = vec![0i64; GRID_CELLS];
    s.crisis_overlay.annotations = rows();
    for annotation in &mut s.crisis_overlay.annotations {
        annotation.path = vec![0u32; 4];
    }
    s.corruption.entries = rows();

    // --- culture ---------------------------------------------------------
    s.culture_layers = rows_of(blank_culture_layer());
    for layer in &mut s.culture_layers {
        layer.traits = rows_of(blank_culture_trait());
    }
    s.culture_tensions = rows_of(blank_culture_tension());
    s.influencers = rows_of(blank_influencer());
    for influencer in &mut s.influencers {
        influencer.audience_generations = vec![0u16; ROWS];
        influencer.culture_resonance = rows_of(blank_culture_resonance());
    }
    for axis in [
        &mut s.sentiment.knowledge,
        &mut s.sentiment.trust,
        &mut s.sentiment.equity,
        &mut s.sentiment.agency,
    ] {
        axis.drivers = rows();
    }

    // --- campaign --------------------------------------------------------
    s.campaign_profiles = rows();
    for (row, profile) in s.campaign_profiles.iter_mut().enumerate() {
        // Every one of these is optional on the wire, so the even rows carry them and the odd rows
        // leave them absent — see [`seeded_on`].
        profile.id = seeded_on(row).then(String::new);
        profile.title = seeded_on(row).then(String::new);
        profile.title_loc_key = seeded_on(row).then(String::new);
        profile.subtitle = seeded_on(row).then(String::new);
        profile.subtitle_loc_key = seeded_on(row).then(String::new);
        profile.starting_units = rows();
        for unit in &mut profile.starting_units {
            unit.tags = vec![String::new(); ROWS];
        }
        profile.inventory = rows();
        profile.knowledge_tags = vec![String::new(); ROWS];
        profile.primary_food_module = seeded_on(row).then(String::new);
        profile.secondary_food_module = seeded_on(row).then(String::new);
    }
    s.command_events = rows();
    for (row, event) in s.command_events.iter_mut().enumerate() {
        event.detail = seeded_on(row).then(String::new);
    }
    s.pending_forks = rows();
    for entry in &mut s.pending_forks {
        entry.forks = rows();
        for fork in &mut entry.forks {
            fork.narration = rows();
            fork.choices = rows();
            for choice in &mut fork.choices {
                choice.label = rows();
            }
            fork.gloss = rows();
        }
    }
    s.stance_axes = rows();
    for stance in &mut s.stance_axes {
        stance.axes = rows();
    }
    s.voice_medium = rows();
    s.opening_loadout.pickable_materials = rows();
    s.opening_loadout.material_defaults = rows();
    s.opening_loadout.craftable_recipe_ids = rows();
    s.opening_loadout.kit_defaults = rows();
    s.victory.modes = rows();
    s.victory.winner = Some(VictoryResultState::default());

    s
}

// ---------------------------------------------------------------------------
// Step 2 — saturate every scalar leaf
// ---------------------------------------------------------------------------

/// Fails the fixture build when a repeated field carries no elements — i.e. when a `Vec` was
/// appended to the schema and not seeded in [`seed_snapshot`]. An empty array is invisible to both
/// gates (the client decoder emits an empty array either way, and the codec round trip cannot tell
/// "decoded nothing" from "there was nothing"), so it is exactly the gap that would let an appended
/// repeated field ship untested; refusing to build is the point.
///
/// **Nothing is exempt.** `knowledge_ledger` and `knowledge_timeline` used to be, because the client
/// decoder never reads them — but the codec round trip does, and a section this builder leaves
/// empty is a section that round trip does not test.
pub fn assert_no_empty_arrays(value: &serde_json::Value, path: &str) -> Result<(), Box<dyn Error>> {
    match value {
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                return Err(format!(
                    "fixture snapshot: `{path}` is an empty array — seed it in seed_snapshot() so \
                     both decode gates actually exercise the repeated field (see \
                     sim_schema/src/fixture.rs)"
                )
                .into());
            }
            for (i, item) in items.iter().enumerate() {
                assert_no_empty_arrays(item, &format!("{path}[{i}]"))?;
            }
        }
        serde_json::Value::Object(fields) => {
            for (key, field) in fields {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                assert_no_empty_arrays(field, &child)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn saturate(value: &mut serde_json::Value, path: &str) {
    match value {
        serde_json::Value::Array(items) => {
            for (i, item) in items.iter_mut().enumerate() {
                saturate(item, &format!("{path}[{i}]"));
            }
        }
        serde_json::Value::Object(fields) => {
            // The booleans of a table are numbered as they are walked (serde_json's map is
            // ordered, so the numbering is stable) and alternated — see [`flag_value`].
            let mut flags = 0usize;
            for (key, field) in fields.iter_mut() {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                if let serde_json::Value::Bool(flag) = field {
                    *flag = flag_value(path, flags);
                    flags += 1;
                } else {
                    saturate(field, &child);
                }
            }
        }
        // A boolean outside any table — an element of a `[bool]`. Its own path is all there is to
        // go on, so it alternates on that.
        serde_json::Value::Bool(flag) => *flag = hash(path).is_multiple_of(2),
        serde_json::Value::String(text) => {
            // Only free text. A non-empty default is a serde-serialized enum variant name, and
            // replacing that with a path would fail to deserialize.
            if text.is_empty() {
                *text = path.to_string();
            }
        }
        serde_json::Value::Number(number) => {
            *value = if number.is_f64() {
                // Fractional, so a dropped `fixed64_to_f32` divide or an `int()` narrowing shows.
                let scaled = (hash(path) % 90_000) as f64 / 800.0;
                serde_json::json!((1.0 + scaled * 100.0).round() / 100.0)
            } else {
                // Capped so a `u8` field cannot fail to deserialize; distinctness lives in the
                // string paths, not here.
                serde_json::json!(1 + hash(path) % 200)
            };
        }
        // `null` is `Option::None`, whose inner type is unknowable here. Options worth covering are
        // seeded to `Some(Default)` in `seed_snapshot` and saturate as ordinary values.
        serde_json::Value::Null => {}
    }
}

/// ⛔ **NOT `true` EVERYWHERE.** The value of the `ordinal`-th boolean of the table at `path`:
/// consecutive flags of one table **always disagree**, and the table's own path sets the phase, so
/// the same field reads differently from one row to the next.
///
/// Saturation used to set every boolean `true`, which made a bool↔bool swap inside a table
/// invisible to both gates: `corralled` and `huntable` carried the same value, so a decoder
/// reading one into the other round-tripped byte-identically. Alternating by ordinal is what makes
/// a neighbouring pair legible; FlatBuffers omits a default-valued field, so the `false` half also
/// puts every boolean's *absent* encoding on the wire.
///
/// ⚠ Two rows can only tell so many flags apart. A table's flags 0 and 2 (and 1 and 3) take the
/// same value in **both** [`ROWS`], because with two rows there are only two patterns that carry
/// both values; swapping *those* two would still round trip. The alternation covers the
/// neighbouring pairs, which is what a mis-wired accessor most often is.
fn flag_value(path: &str, ordinal: usize) -> bool {
    (hash(path) as usize)
        .wrapping_add(ordinal)
        .is_multiple_of(2)
}

/// FNV-1a over the path. Any stable hash would do; this one keeps the fixture reproducible across
/// machines and Rust versions (`DefaultHasher` guarantees neither).
fn hash(path: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in path.as_bytes() {
        h ^= *byte as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

// ---------------------------------------------------------------------------
// Step 3 — structural fixups
// ---------------------------------------------------------------------------

/// A row's tile, kept inside the grid and with the two coordinates **different** — see
/// [`ROW_Y_OFFSET`].
fn row_x(row: usize) -> u32 {
    row as u32 % GRID_W
}

fn row_y(row: usize) -> u32 {
    (row as u32 + ROW_Y_OFFSET) % GRID_H
}

/// Restores the fields whose values are *structural* rather than arbitrary: a raster's dimensions
/// must match its sample count, and a tile's coordinates must land inside the grid, or the decoder
/// indexes a raster out of step with the grid it was told about.
fn apply_structural_fixups(s: &mut WorldSnapshot) {
    s.header.tick = 41;
    s.header.tile_count = GRID_CELLS as u32;
    s.header.wrap_horizontal = true;

    s.terrain.width = GRID_W;
    s.terrain.height = GRID_H;
    // Vary the biome per cell so `terrain_label_from_id` and the tag-label table are exercised
    // rather than answering one question 12 times.
    for (i, sample) in s.terrain.samples.iter_mut().enumerate() {
        sample.terrain = TerrainType::VALUES[i % TerrainType::VALUES.len()];
    }

    s.elevation_overlay.width = GRID_W;
    s.elevation_overlay.height = GRID_H;
    s.moisture_raster.width = GRID_W;
    s.moisture_raster.height = GRID_H;

    for raster in [
        &mut s.sentiment_raster,
        &mut s.corruption_raster,
        &mut s.culture_raster,
        &mut s.military_raster,
        &mut s.visibility_raster,
        &mut s.crisis_overlay.heatmap,
    ] {
        raster.width = GRID_W;
        raster.height = GRID_H;
    }

    for (i, tile) in s.tiles.iter_mut().enumerate() {
        tile.entity = i as u64;
        tile.x = (i as u32) % GRID_W;
        tile.y = (i as u32) / GRID_W;
        tile.terrain = TerrainType::VALUES[i % TerrainType::VALUES.len()];
        tile.underlying_terrain = TerrainType::VALUES[(i + 1) % TerrainType::VALUES.len()];
    }

    if let Some(marker) = s.start_marker.as_mut() {
        marker.x = START_MARKER_X;
        marker.y = START_MARKER_Y;
    }

    // Keep every located row on the map. A saturated coordinate would sit far outside a 4×3 grid,
    // which is legal on the wire but makes the golden read as nonsense.
    for (i, cohort) in s.populations.iter_mut().enumerate() {
        cohort.entity = 100 + i as u64;
        cohort.current_x = row_x(i);
        cohort.current_y = row_y(i);
        // **THE BENCH'S RANK IS AN ENUM, SO SATURATION LEAVES IT AT ITS DEFAULT** — the same gap the
        // labor rows' `EVERY_SOURCE_PRIORITY` closes (see that const). A cohort carries exactly ONE
        // bench, so with two cohorts the fixture can reach two of the three arms, and these are the
        // two it must reach: a decoder maps this enum with a `_ =>` catch-all on the DEFAULT (the
        // shipped `LaborAssignment` mapping is `_ => "normal"`), so a wrong `Normal` arm is
        // unreachable by construction while a wrong `High` or `Low` decodes silently as `normal`.
        // The `Normal` arm is covered end-to-end at the codec level by
        // `core_sim/tests/crafting_wire.rs`, which asserts it off the encoded envelope.
        cohort.bench.priority = BENCHED_SOURCE_PRIORITIES[i % BENCHED_SOURCE_PRIORITIES.len()];
    }
    for (i, herd) in s.herds.iter_mut().enumerate() {
        herd.x = row_x(i);
        herd.y = row_y(i);
    }
    for (i, module) in s.food_modules.iter_mut().enumerate() {
        module.x = row_x(i);
        module.y = row_y(i);
    }
    for (i, patch) in s.forage_patches.iter_mut().enumerate() {
        patch.x = row_x(i);
        patch.y = row_y(i);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::encode_snapshot_flatbuffer;

    /// The half of the decode gate that runs **without Godot**, and therefore in CI.
    ///
    /// `tools/decode_guard.gd` needs a live engine (`VarDictionary` cannot be constructed outside
    /// one) and CI has no Godot, so the golden diff is a local gate. This test carries what does
    /// travel: `saturated_snapshot` runs `assert_no_empty_arrays`, so **appending a repeated
    /// field to the schema and forgetting to seed it fails CI**, naming the path — the schema-drift
    /// alarm, without the engine.
    #[test]
    fn the_fixture_covers_every_repeated_field_and_encodes() {
        let snapshot = saturated_snapshot().expect("fixture builds");
        let bytes = encode_snapshot_flatbuffer(&snapshot);
        assert!(
            bytes.len() > 1024,
            "fixture envelope is suspiciously small ({} bytes) — did a section stop encoding?",
            bytes.len()
        );
    }

    /// Saturation must be **deterministic**, or the committed golden would drift with no decoder
    /// change and every reader would learn to ignore the diff. Path-derived FNV, no clock, no RNG.
    #[test]
    fn the_fixture_is_byte_identical_across_builds() {
        let first = encode_snapshot_flatbuffer(&saturated_snapshot().expect("first build"));
        let second = encode_snapshot_flatbuffer(&saturated_snapshot().expect("second build"));
        assert_eq!(first, second, "fixture encoding is not deterministic");
    }

    /// ⛔ **NO TABLE MAY CARRY ONE VALUE IN EVERY ONE OF ITS FLAGS.** The fixture used to set every
    /// boolean `true`, which made a bool↔bool swap inside a table invisible to both decode gates.
    /// This walks the saturated snapshot and fails on any table whose booleans all agree, naming
    /// it — the guard on [`flag_value`]'s alternation surviving a future field.
    #[test]
    fn saturation_leaves_no_table_with_every_flag_alike() {
        fn walk(value: &serde_json::Value, path: &str, alike: &mut Vec<String>) {
            match value {
                serde_json::Value::Array(items) => {
                    for (i, item) in items.iter().enumerate() {
                        walk(item, &format!("{path}[{i}]"), alike);
                    }
                }
                serde_json::Value::Object(fields) => {
                    let flags: Vec<bool> = fields
                        .values()
                        .filter_map(|field| match field {
                            serde_json::Value::Bool(flag) => Some(*flag),
                            _ => None,
                        })
                        .collect();
                    if flags.len() > 1 && flags.iter().all(|flag| *flag == flags[0]) {
                        alike.push(format!("{path} ({} flags, all {})", flags.len(), flags[0]));
                    }
                    for (key, field) in fields {
                        let child = if path.is_empty() {
                            key.clone()
                        } else {
                            format!("{path}.{key}")
                        };
                        walk(field, &child, alike);
                    }
                }
                _ => {}
            }
        }

        let snapshot = saturated_snapshot().expect("fixture builds");
        let value = serde_json::to_value(&snapshot).expect("the fixture serialises");
        let mut alike = Vec::new();
        walk(&value, "", &mut alike);
        assert!(
            alike.is_empty(),
            "these tables carry one value in every flag, so a bool<->bool swap inside them round \
             trips clean: {alike:#?}"
        );
    }

    /// The two saturation rules the whole scheme rests on: an enum's variant name must survive
    /// (only an *empty* string is free text), and integers must stay inside `u8` so a narrow field
    /// cannot fail to deserialize.
    #[test]
    fn saturation_preserves_enum_variants_and_stays_in_u8_range() {
        let mut value = serde_json::json!({
            "free_text": "",
            "an_enum": "AlluvialPlain",
            "a_count": 0,
            "a_rate": 0.0,
        });
        saturate(&mut value, "root");

        assert_eq!(value["free_text"], serde_json::json!("root.free_text"));
        assert_eq!(value["an_enum"], serde_json::json!("AlluvialPlain"));

        let count = value["a_count"].as_u64().expect("count stays an integer");
        assert!(
            (1..=200).contains(&count),
            "integer sentinel {count} would not fit a u8 field"
        );
        let rate = value["a_rate"].as_f64().expect("rate stays a float");
        assert!(
            rate.fract() != 0.0,
            "float sentinel {rate} has no fraction, so an int() narrowing would be invisible"
        );
    }
}
