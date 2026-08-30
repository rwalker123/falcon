//! **THE BUILDERS POOL AND THE BAND'S BUILD QUEUE** (`docs/plan_standing_upkeep.md` §2.5/§4.6b).
//!
//! A verb no longer names a crew — it appends an entry to the band's **ordered queue**, and the
//! band-level `builders` pool goes **entirely onto the head** of that queue until the head's meter
//! fills, then onto the next.
//!
//! | what this file pins | why it is not obvious |
//! |---|---|
//! | the pool funds **only** the head | a spread would look like progress and finish nothing |
//! | completion **retires the entry** and the pool moves on | otherwise the pool sits on a finished job |
//! | a waiting entry gets a **chained finish date**, and it comes true | a date nobody checks against the clock is the easiest thing here to get subtly wrong |
//! | a **blocked** head says `-4`, and everything behind it says `-4` too | `-1` renders as no line, which is exactly the silence a stuck pool must not be read as |
//! | the **animal web's escapement stall** reaches `-4` beside its own shortfall | that stall is the case Ray's *"say loudly that it is stuck"* ruling is about, and its remedy is off the build line entirely |
//! | a **parked** half-built meter stays the neutral `-2` | the queue must not re-mark a state the player chose |
//! | `unqueue` / `abandon` / `build_order` | the queue's three inputs, and the undo a declaration never had |
//! | the queue survives a **checkpoint**, order intact | its `PartialEq` decides whether a rollback record sees it at all |
//!
//! **Asserted on the ENCODED snapshot wherever the claim is about a published number**, because a
//! field can be right in the capture and absent from the buffer.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::prelude::Entity;

use core_sim::{
    build_test_app, recapture_snapshot_in_place, scalar_from_f32, scalar_one, scalar_zero,
    BuildJob, BuildSource, FactionId, GenerationId, Improvement, LaborAllocation, LaborAssignment,
    LaborTarget, LocalStore, MoraleCause, PopulationCohort, ResidentBand, SnapshotHistory,
    SourcePriority, TileRegistry,
};
use sim_schema::{
    BUILD_METER_HOLDS, BUILD_NOT_YET_ESTIMATED, BUILD_QUEUE_BLOCKED, NOT_IN_ANY_BUILD_QUEUE,
    NO_BUILD_TURNS_ESTIMATE,
};

// ---------------------------------------------------------------------------------------------
// The fixture: one band, three cultivable patches in its work range, one builders pool
// ---------------------------------------------------------------------------------------------

/// **The pool every arm staffs.** Two rather than one, so `work_cost / builders` is a division the
/// arithmetic can get wrong — a pool of one would make the pace and the cost the same number.
const BUILDERS: u32 = 2;

/// The gatherers on each patch. Enough to keep the ground genuinely worked (the `Cultivate` gate
/// reads the escapement room), and not so many that they strip it below the floor.
const GATHERERS: u32 = 2;

/// The floor every fixture row works at — the food peak, where `learn_multiplier` is exactly ×1.0,
/// so a build's pace is the crew's own output rather than a fraction of it.
const FOOD_PEAK: f32 = 0.5;

/// **Three queued sources** — enough that the chain's own property (entry 3 carries the sum of the
/// two above it) is a sum rather than a copy.
const THE_WHOLE_QUEUE: usize = 3;

/// One, where the arm is about a single source and a second would only add noise.
const ONE_SOURCE: usize = 1;

/// The fixture band's durable id — high enough not to collide with worldgen's own bands.
const FIXTURE_BAND: u64 = 9_001;

/// **Every cultivable patch inside one band's work range**, ordered deterministically: the anchor
/// first, then by distance from it. Three are needed at most — the chain's interesting property is
/// that entry 3 carries the sum of the two above it.
fn cultivable_sites_in_one_work_range(app: &mut App) -> Vec<UVec2> {
    let labor = app.world.resource::<core_sim::LaborConfigHandle>().get();
    let flora = app.world.resource::<core_sim::FloraConfigHandle>().get();
    let map_seed = app.world.resource::<core_sim::SimulationConfig>().map_seed;
    let work_range = labor.band_work_range;
    let wrap = app
        .world
        .resource::<core_sim::SimulationConfig>()
        .map_topology
        .wrap_horizontal;
    let width = app.world.resource::<TileRegistry>().width;
    let tiles: std::collections::HashMap<UVec2, core_sim::Tile> = {
        let mut query = app.world.query::<&core_sim::Tile>();
        query
            .iter(&app.world)
            .map(|tile| (tile.position, tile.clone()))
            .collect()
    };
    let registry = app.world.resource::<core_sim::ForageRegistry>();
    let mut cultivable: Vec<UVec2> = app
        .world
        .resource::<core_sim::FoodSiteRegistry>()
        .sites()
        .iter()
        .map(|site| site.position)
        .filter(|position| registry.patch(*position).is_some())
        .filter(|position| {
            let Some(tile) = tiles.get(position) else {
                return false;
            };
            let composition =
                core_sim::tile_flora_composition(&flora, &labor.forage, tile, map_seed);
            core_sim::default_species_for_rung(&composition, &flora, RungKey::PlantTended).is_some()
        })
        .collect();
    cultivable.sort_by_key(|tile| (tile.y, tile.x));

    // The band stands on one of them, so "in range" is measured from that anchor — the same hex
    // distance `advance_labor_allocation` uses, wrap included.
    for anchor in &cultivable {
        let mut near: Vec<UVec2> = cultivable
            .iter()
            .copied()
            .filter(|other| {
                core_sim::grid_utils::hex_distance_wrapped(*anchor, *other, width, wrap)
                    <= work_range
            })
            .collect();
        if near.len() >= 3 {
            near.sort_by_key(|tile| {
                (
                    core_sim::grid_utils::hex_distance_wrapped(*anchor, *tile, width, wrap),
                    tile.y,
                    tile.x,
                )
            });
            return near;
        }
    }
    panic!("the fixture map must carry three cultivable sites inside one band's work range");
}

mod pen_materials_support;

use core_sim::RungKey;
use core_sim::TakeSelection;

/// A world with one band that works `count` cultivable patches in its own work range, staffs
/// `builders`, keeps every meter it holds, and has queued a `Cultivate` on each source **in the
/// order returned**.
///
/// ⛔ **The sites are resolved INSIDE the world they are used in.** `simulation_config.json` ships
/// `map_seed: 0`, which means *roll a fresh seed*, so every `build_headless_app()` is a different
/// map — a fixture that scouted one world and seeded another would name tiles that are not there.
fn world_with_a_queue(count: usize, builders: u32) -> (App, Entity, Vec<UVec2>) {
    world_with_a_queue_knowing(count, builders, THE_GATE_IS_OPEN)
}

/// **The faction has learned Cultivation** — every arm but the blocked one, where the rung's own
/// gate is what refuses the head.
const THE_GATE_IS_OPEN: bool = true;

/// [`world_with_a_queue`] with the rung's own knowledge gate as a dial.
fn world_with_a_queue_knowing(
    count: usize,
    builders: u32,
    knows_cultivation: bool,
) -> (App, Entity, Vec<UVec2>) {
    let mut app = build_test_app();
    app.update();
    let sources: Vec<UVec2> = cultivable_sites_in_one_work_range(&mut app)
        .into_iter()
        .take(count)
        .collect();
    assert_eq!(
        sources.len(),
        count,
        "the fixture map must carry {count} sites"
    );
    let keepers = keeping_for(count);
    let anchor = sources[0];
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(anchor.x, anchor.y)
        .expect("the fixture tile resolves");
    if knows_cultivation {
        app.world
            .resource_mut::<core_sim::DiscoveryProgressLedger>()
            .add_progress(
                FactionId(0),
                core_sim::CULTIVATION_DISCOVERY_ID,
                scalar_one(),
            );
    }

    let mut assignments: Vec<LaborAssignment> = sources
        .iter()
        .map(|source| LaborAssignment {
            target: LaborTarget::Forage {
                tile: *source,
                floor: FOOD_PEAK,
                species: None,
                take_species: TakeSelection::EVERYTHING,
            },
            workers: GATHERERS,
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        })
        .collect();
    assignments.push(LaborAssignment {
        target: LaborTarget::Builders,
        workers: builders,
        // ⛔ A `builders` ROW carries no kit — the bare isolation rides the queue entry
        // (`docs/plan_standing_upkeep.md` §4.7a ②).
        kit: None,
        priority: SourcePriority::default(),
        upkeep_kit: None,
    });
    if keepers > 0 {
        assignments.push(LaborAssignment {
            target: LaborTarget::Agriculture,
            workers: keepers,
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        });
    }
    let staffed: u32 = assignments.iter().map(|row| row.workers).sum();
    let build_queue = sources
        .iter()
        .map(|source| core_sim::BuildQueueEntry {
            source: BuildSource::Patch(*source),
            declared: BuildJob::Rung(Improvement::Cultivate),
            kit: Some(bare_builders()),
        })
        .collect();

    let band = app
        .world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 200,
                children: scalar_zero(),
                // Sized to exactly what it staffs, so `normalize` never trims a row under test.
                working: scalar_from_f32(staffed as f32),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_turn_transfer_received: 0.0,
                last_turn_transfer_sent: 0.0,
                last_morale_delta: scalar_zero(),
                last_morale_cause: MoraleCause::None,
                last_morale_contributions: Default::default(),
                last_fertility_factors: Default::default(),
                discontent_fraction: scalar_zero(),
                grievance: scalar_zero(),
                last_emigrated: 0,
                last_immigrated: 0,
                age_turns: 0,
                generation: 0 as GenerationId,
                faction: FactionId(0),
                knowledge: Vec::new(),
                migration: None,
            },
            ResidentBand,
            // **A durable id, because a checkpoint keys bands by it** — a band without one is not
            // recorded at all, which would make the rollback arm below vacuous.
            core_sim::BandId(FIXTURE_BAND),
            LaborAllocation {
                assignments,
                build_queue,
                ..Default::default()
            },
        ))
        .id();
    (app, band, sources)
}

/// One turn in the real stage order, then republish.
fn resolve_a_turn(app: &mut App) {
    app.world.run_system_once(core_sim::advance_forage_regrowth);
    app.world.run_system_once(core_sim::advance_cultivation);
    app.world
        .run_system_once(core_sim::advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);
}

/// A patch's live cultivation meter.
fn meter(app: &App, source: UVec2) -> f32 {
    app.world
        .resource::<core_sim::ForageRegistry>()
        .patch(source)
        .expect("the fixture patch survives")
        .ladder_position()
}

/// **One published field of a patch's row, off the ENCODED buffer.**
fn published<T>(
    app: &App,
    source: UVec2,
    read: impl Fn(&shadow_scale_flatbuffers::generated::shadow_scale::sim::ForagePatchState<'_>) -> T,
) -> T {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let row = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .subsistence()
        .and_then(|section| section.foragePatches())
        .expect("the subsistence section carries the patch list")
        .iter()
        .find(|patch| patch.x() == source.x && patch.y() == source.y)
        .expect("the fixture patch is on the wire");
    read(&row)
}

fn published_turns(app: &App, source: UVec2) -> i32 {
    published(app, source, |patch| patch.buildTurnsRemaining())
}

fn published_position(app: &App, source: UVec2) -> i32 {
    published(app, source, |patch| patch.buildQueuePosition())
}

/// **WHY the queue is blocked here**, off the encoded row — `""` for an entry that is not blocked.
fn published_reason(app: &App, source: UVec2) -> String {
    published(app, source, |patch| {
        patch.buildBlockedReason().unwrap_or_default().to_string()
    })
}

/// The wire's *"this entry is not blocked"* — the empty [`buildBlockedReason`], named because `""`
/// at an assertion site says nothing about what it means.
const NOT_BLOCKED: &str = "";

/// **The keeping this band owes for three tended meters in flight** — read off the shipped ladder so
/// a retune moves the fixture with the game. Staffed wherever an arm must not be measuring rot.
fn keeping_for(sources: usize) -> u32 {
    let probe = build_test_app();
    // **THE RATE IS QUOTED PER TENDER-LOAD** (`forage::patch_tender_loads`), and this fixture's
    // sources sit on whatever ground `a_cultivable_site` picked — so the hands are read at the
    // RICHEST tile the shipped biome table offers rather than at one load. Over-staffing a keeping
    // pool costs these arms nothing (a surplus is not a credit); under-staffing would make every
    // one of them measure rot instead of what it is about.
    let labor = probe.world.resource::<core_sim::LaborConfigHandle>().get();
    let richest_capacity = labor
        .forage
        .capacity_by_biome
        .values()
        .copied()
        .fold(core_sim::NO_FORAGE_CAPACITY, f32::max);
    let richest = core_sim::patch_tender_loads(richest_capacity, &labor.forage);
    let hands = probe
        .world
        .resource::<core_sim::LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantTended)
        .upkeep_crew_needed(richest);
    hands * sources as u32
}

// ---------------------------------------------------------------------------------------------
// (1) The pool funds ONLY the head
// ---------------------------------------------------------------------------------------------

/// **ALL HANDS ON THE HEAD** (`docs/plan_standing_upkeep.md` §2.5) — the whole builders pool goes on
/// `build_queue[0]`, and the entries behind it do not move **at all**.
///
/// A spread would be the plausible wrong answer, and it would look like progress everywhere while
/// finishing nothing; the assertion is therefore exact on both halves — the head gains **precisely**
/// the pool's work, and the tail gains **zero**.
#[test]
fn the_whole_pool_funds_the_head_and_the_entries_behind_it_do_not_move() {
    let (mut app, _band, sources) = world_with_a_queue(THE_WHOLE_QUEUE, BUILDERS);

    let before: Vec<f32> = sources.iter().map(|s| meter(&app, *s)).collect();
    resolve_a_turn(&mut app);
    let after: Vec<f32> = sources.iter().map(|s| meter(&app, *s)).collect();

    let pool_work = core_sim::activity_work(BUILDERS);
    assert!(
        pool_work > 0.0,
        "fixture: the pool must produce something, or every assertion below is a comparison of \
         zeroes"
    );
    assert!(
        (after[0] - before[0] - pool_work).abs() < 1e-4,
        "the head banks the pool's whole output, not a share of it: {} -> {} at a pool of {}",
        before[0],
        after[0],
        pool_work
    );
    for index in 1..sources.len() {
        assert_eq!(
            after[index], before[index],
            "entry {index} is waiting, so its meter does not move at all"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// (2) Completion retires the entry and hands the pool on
// ---------------------------------------------------------------------------------------------

/// **COMPLETION RETIRES THE ENTRY, AND THE POOL IS ON THE NEXT ONE THE FOLLOWING TURN**
/// (§2.4: *"at its cost, the entry leaves the queue"*).
///
/// Walked to the transition rather than predicted, because how many turns a build takes depends on
/// the ground it stands on; what the test needs is the **edge**.
#[test]
fn completion_retires_the_head_and_the_pool_moves_to_the_next_entry() {
    let (mut app, band, sources) = world_with_a_queue(THE_WHOLE_QUEUE, BUILDERS);

    // Long enough for the head to finish however the fixture's ground behaves.
    let mut turns = 0;
    while !app
        .world
        .resource::<core_sim::ForageRegistry>()
        .patch(sources[0])
        .expect("the head's patch survives")
        .is_cultivated()
    {
        resolve_a_turn(&mut app);
        turns += 1;
        assert!(turns < 200, "fixture: the head must finish");
    }
    assert!(
        turns > 1,
        "fixture: the head must take real turns, or 'completion is a transition' is untested"
    );

    let queue = &app
        .world
        .get::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .build_queue;
    assert_eq!(
        queue.len(),
        sources.len() - 1,
        "the finished entry leaves the queue"
    );
    assert_eq!(
        queue[0].source,
        BuildSource::Patch(sources[1]),
        "…and what the player put next is the head"
    );

    // **The next turn, the pool is on it.** Nothing else changed — the player issued no command.
    let before = meter(&app, sources[1]);
    resolve_a_turn(&mut app);
    let after = meter(&app, sources[1]);
    let pool_work = core_sim::activity_work(BUILDERS);
    assert!(
        (after - before - pool_work).abs() < 1e-4,
        "the whole pool moves to the new head with no command from the player: {before} -> {after}"
    );
}

// ---------------------------------------------------------------------------------------------
// (3) The chained date, and the clock it is checked against
// ---------------------------------------------------------------------------------------------

/// **A WAITING ENTRY GETS A REAL FINISH DATE, AND IT COMES TRUE** (§4.6b).
///
/// The queue is deterministic, so an entry's turns are *everything above it plus its own span at the
/// full pool*. This asserts the arithmetic **and then lets it run**: a chained date nobody checks
/// against the clock is the easiest thing here to get subtly wrong.
///
/// **The position rides beside it**, because a date with no place in the line is an exact number the
/// player cannot explain — forty turns of work reads the same as eight turns behind four other jobs.
#[test]
fn a_waiting_entry_is_dated_behind_the_head_and_finishes_when_it_promised() {
    let (mut app, _band, sources) = world_with_a_queue(THE_WHOLE_QUEUE, BUILDERS);
    resolve_a_turn(&mut app);

    let dates: Vec<i32> = sources.iter().map(|s| published_turns(&app, *s)).collect();
    for (index, date) in dates.iter().enumerate() {
        assert!(
            *date > 0,
            "every entry is dated, waiting or not — entry {index} published {date}"
        );
        assert_eq!(
            published_position(&app, sources[index]),
            index as i32,
            "…beside its 0-based place in the line"
        );
    }
    assert!(
        dates[1] > dates[0] && dates[2] > dates[1],
        "the chain is strictly increasing: {dates:?}"
    );
    // **The chain is a SUM, and the increments are each entry's own span at the full pool.** The
    // three patches are the same rung on comparable ground, so their spans are equal — and the
    // head's own date is *shorter* than theirs by exactly the turn it has already banked, which is
    // what makes this a chain rather than three copies of one number.
    //
    // Stated as differences rather than as literals, so a retune of `work_cost` moves the fixture
    // with the game.
    let waiting_span = dates[1] - dates[0];
    assert_eq!(
        dates[2] - dates[1],
        waiting_span,
        "each waiting entry adds its OWN span at the full pool, and they are the same rung on \
         comparable ground: {dates:?}"
    );
    assert!(
        waiting_span > dates[0],
        "the head is nearer than a full span, because it has already banked a turn — a chain that \
         re-quoted the whole job for the head would make these equal: {dates:?}"
    );

    // **NOW LET IT RUN.** Each entry must finish on the turn it promised — one turn of the three
    // already resolved above, so the promise is counted from here.
    let promised = dates.clone();
    let mut elapsed = 0_i32;
    let mut finished_on: Vec<Option<i32>> = vec![None; sources.len()];
    while finished_on.iter().any(Option::is_none) {
        resolve_a_turn(&mut app);
        elapsed += 1;
        for (index, source) in sources.iter().enumerate() {
            if finished_on[index].is_none()
                && app
                    .world
                    .resource::<core_sim::ForageRegistry>()
                    .patch(*source)
                    .expect("the fixture patch survives")
                    .is_cultivated()
            {
                finished_on[index] = Some(elapsed);
            }
        }
        assert!(elapsed < 400, "fixture: the whole queue must drain");
    }
    for (index, landed) in finished_on.iter().enumerate() {
        assert_eq!(
            landed.expect("every entry finished"),
            promised[index],
            "entry {index} promised {} turns and landed on {landed:?}: {promised:?}",
            promised[index]
        );
    }
}

// ---------------------------------------------------------------------------------------------
// (4) The blocked head
// ---------------------------------------------------------------------------------------------

/// **A BLOCKED HEAD SAYS SO, AND SO DOES EVERYTHING BEHIND IT** (§4.6b).
///
/// The pool is staffed and standing on an entry whose own gate refuses it, so nothing banks and
/// nothing behind it moves. That is `-4` — **not** `-1`, which is the *absence* of an answer and
/// renders as no line at all, which is exactly the silence this state must not be read as.
///
/// **And the remedy is asserted too.** A test that only ever sees the failure passes with the fix
/// broken, so the gate is reopened and the queue is required to recover to real counts.
#[test]
fn a_blocked_head_publishes_minus_four_and_every_entry_behind_it_says_the_same() {
    // **The gate closed the way the plant web can close it**: the faction has not learned
    // Cultivation, so the rung refuses every entry while the pool stands on the head.
    let (mut app, band, sources) =
        world_with_a_queue_knowing(THE_WHOLE_QUEUE, BUILDERS, !THE_GATE_IS_OPEN);
    resolve_a_turn(&mut app);

    assert_eq!(
        app.world
            .get::<LaborAllocation>(band)
            .expect("the band keeps its allocation")
            .workers_on(&LaborTarget::Builders),
        BUILDERS,
        "fixture: the pool is staffed — a blocked reading is about a COMMITTED pool getting nowhere"
    );
    assert_eq!(
        meter(&app, sources[0]),
        0.0,
        "fixture: and the head's meter really does not move"
    );
    for (index, source) in sources.iter().enumerate() {
        assert_eq!(
            published_turns(&app, *source),
            BUILD_QUEUE_BLOCKED,
            "entry {index} is behind a head that never finishes, so it publishes the same news"
        );
        // **AND WHY.** `-4` alone renders as *"stuck"* with nothing to act on — the playtest that
        // filed this sat on one for turns. The cause is the conjunct that actually refused, and it
        // is **carried down the queue** with the sentinel, because the head's reason is why the
        // entries behind it are stuck too.
        assert_eq!(
            published_reason(&app, *source),
            "knowledge",
            "entry {index} must name the conjunct that refused — the unlearned Cultivation gate"
        );
    }

    // **THE REMEDY.** Reopen the gate and the queue answers with real counts again.
    app.world
        .resource_mut::<core_sim::DiscoveryProgressLedger>()
        .add_progress(
            FactionId(0),
            core_sim::CULTIVATION_DISCOVERY_ID,
            scalar_one(),
        );
    resolve_a_turn(&mut app);
    for (index, source) in sources.iter().enumerate() {
        assert!(
            published_turns(&app, *source) > 0,
            "entry {index} recovers to a real count once the gate opens — a test that only ever \
             sees the failure passes with the remedy broken"
        );
        // **The NOT-blocked half of the pair.** A producer that stamped a cause unconditionally
        // would satisfy the positive assertion above and leave every finishing build claiming a
        // refusal, so the empty key is asserted as hard as the populated one.
        assert_eq!(
            published_reason(&app, *source),
            NOT_BLOCKED,
            "entry {index} is building again, so it must publish no blocked cause at all"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// (4b) The blocked head on the ANIMAL web — the stall the ruling is about
// ---------------------------------------------------------------------------------------------

/// **THE MEASURED PERMANENT STALL, AND `-4` IS WHAT IT SAYS**
/// (`docs/plan_standing_upkeep.md` §6, `.claude/rules/core_sim/husbandry.md` → "THE REGROWTH
/// SUPPRESSION CLOSES A LOOP").
///
/// The plant arm above closes the gate with an unlearned knowledge, which exercises the mint and the
/// carry. **This is the case the ruling is about**, and it is a loop no single seam contains:
///
/// 1. the band's `husbandry` role is empty, so the herd's keeping is unmet;
/// 2. `regrow_biomass` suppresses the flock's growth entirely at `upkeep_supplied <= 0`;
/// 3. the hunters beside the build draw the flock down to their assignment's escapement floor, and
///    with no growth it never comes back above it;
/// 4. `crew_is_working_the_source` reads that room as `0`, so the `Tame`'s own gate goes false;
/// 5. nothing banks, **and nothing ever will** — it is an *eligibility* stall, not a balance one, so
///    no term the countdown is struck from can see it.
///
/// **The remedy is off the build line entirely**, which is why the sentinel has to be `-4` rather
/// than `-1`: `-1` renders as no line at all, and adding builders, re-ordering the queue and
/// re-issuing the verb all leave the room at zero. So this asserts the **pairing** a client renders
/// the remedy from — the blocked count beside a non-zero `upkeepShortfall` with the grace spent —
/// and then staffs `husbandry` and requires the queue to recover.
#[test]
fn the_animal_webs_escapement_stall_publishes_minus_four_beside_its_shortfall() {
    /// A **shallow** floor the hunters reach quickly: they draw the flock down to it, and with the
    /// keeping unmet nothing grows back above it, so the escapement room stays at zero.
    const STRIP_TO: f32 = 0.9;
    /// A bound on the walk to the stall, not a prediction of it. Deliberately tight: the same unmet
    /// keeping that shuts the gate is **shedding** the flock underneath it, so a long run measures a
    /// herd that is **gone** rather than one that is stuck.
    const WALK_LIMIT: u32 = 12;
    /// And a generous one on the walk back: the flock has to regrow past its hunters' old floor.
    const RECOVERY_LIMIT: u32 = 120;

    let (mut app, band, herd_id) = world_with_a_half_tamed_herd(NOBODY_KEEPING, STRIP_TO);

    // **Walk to the stall rather than predicting it** — and to the *whole* reported state, which is
    // the blocked countdown **with the grace spent**. A shortfall still being forgiven is not yet
    // the state the remedy is for.
    let mut blocked_on = None;
    let mut frozen = 0.0;
    for turn in 1..=WALK_LIMIT {
        // The meter as it stood **before** this turn — so the turn the gate closes can be shown to
        // have banked nothing, without resolving another turn to find out. **That matters here**: the
        // same unmet keeping that shuts the gate is shedding the flock underneath it, so the stall is
        // a window rather than a resting place, and a fixture that spent a turn confirming it would
        // be asserting against a herd that had died in the meantime.
        let before = herd_meter(&app, &herd_id);
        resolve_an_animal_turn(&mut app);
        let (turns, _, _, grace_left, _) = published_herd(&app, &herd_id);
        if turns == BUILD_QUEUE_BLOCKED && grace_left == GRACE_SPENT {
            assert_eq!(
                herd_meter(&app, &herd_id),
                before,
                "the meter is frozen on the turn the gate refuses: nothing banks"
            );
            frozen = before;
            blocked_on = Some(turn);
            break;
        }
    }
    let blocked_on =
        blocked_on.expect("the unkept Tame must reach the stall inside the walk limit");
    assert!(
        blocked_on > 1,
        "fixture: the stall must be REACHED rather than staged — it took {blocked_on} turn(s)"
    );

    // The pool really is committed — a blocked reading is about a **staffed** pool getting nowhere.
    assert_eq!(
        app.world
            .get::<LaborAllocation>(band)
            .expect("the band keeps its allocation")
            .workers_on(&LaborTarget::Builders),
        BUILDERS,
        "fixture: the builders are staffed, and standing on this herd"
    );
    assert!(
        frozen > 0.0,
        "fixture: the herd is genuinely MID-Tame — a meter at zero would reach `-1` for its own \
         reason and prove nothing about the stall"
    );
    assert!(
        frozen > HALF_BUILT * cost_of_taming(&app, &herd_id),
        "fixture: and it BANKED before it froze, so the walk measured a stall rather than a build \
         that never started"
    );

    // **THE PAIRING, on one encoded row.** A `-4` with no shortfall beside it would give the player
    // nothing to act on.
    let (turns, reason, shortfall, grace_left, has_grace) = published_herd(&app, &herd_id);
    assert_eq!(
        turns, BUILD_QUEUE_BLOCKED,
        "the head of a staffed queue whose own gate refuses it says so"
    );
    // **AND WHICH CONJUNCT REFUSED.** This is the whole point of the pairing: the keeping shortfall
    // beside it is *a* true fact, and the playtest that filed this fixed exactly that and stayed
    // blocked — because the refusal is the herd standing below its escapement floor, which only the
    // keeping can reopen and which no surface said out loud.
    assert_eq!(
        reason, "escapement",
        "the stall must name the escapement room, not merely report that the queue is stuck"
    );
    assert!(
        shortfall > 0.0,
        "…beside the herd's own unmet keeping, which is where the remedy is: {shortfall}"
    );
    assert!(
        has_grace && grace_left == GRACE_SPENT,
        "…with the grace spent, so the shortfall is biting rather than being forgiven \
         (has_grace {has_grace}, remaining {grace_left})"
    );

    // **THE REMEDY, AND ONLY IT.** `assign_labor <faction> <band> husbandry <n>` — nothing on the
    // build line reaches this, which is exactly why the sentinel has to say so out loud.
    //
    // ⛔ **The hunters stay exactly where they are, at full strength.** Their draw is the *other*
    // half of what pins the flock, so it is tempting to lift it here too — but the sentinel's copy
    // names the keeping alone as the remedy, and a fixture that also stopped the hunting would pass
    // with that copy wrong. Restoring the keeping restores `regrow_biomass`, and the regrowth
    // outruns a floor-respecting take on its own: the flock climbs back above `floor · K`, the
    // escapement room returns, and the gate opens with the same crew still hunting.
    let keepers = keeping_a_herd_needs(&app, &herd_id).max(1);
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("the band keeps its allocation");
        allocation.assignments.push(LaborAssignment {
            target: LaborTarget::Husbandry,
            workers: keepers,
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        });
    }

    // **Generously bounded.** The flock has to grow back above its hunters' floor before the room
    // returns, and how many turns that takes is the species' own `r` on the map worldgen rolled — a
    // bound, not a prediction. Measured at 7–14 turns across seeds, and ~50 on a map whose herd had
    // bled down to single-digit biomass before the walk ended.
    let mut recovered = None;
    for _ in 0..RECOVERY_LIMIT {
        resolve_an_animal_turn(&mut app);
        let published = published_herd(&app, &herd_id).0;
        if published >= 0 {
            recovered = Some(published);
            break;
        }
    }
    let recovered = recovered.expect(
        "staffing the keeping is the WHOLE remedy — with the hunters left in place — and a test \
         that only ever sees the failure passes with the remedy broken",
    );
    assert!(
        recovered > 0,
        "…and it recovers to a REAL count, not another sentinel: {recovered}"
    );
    // **The NOT-blocked half of the pair.** A cause stamped unconditionally would satisfy the
    // `escapement` assertion above while telling every recovered build it was still refused.
    assert_eq!(
        published_herd(&app, &herd_id).1,
        NOT_BLOCKED,
        "…and it stops naming a cause, because it is no longer blocked"
    );
    assert!(
        herd_meter(&app, &herd_id) > frozen,
        "…and the meter moves again once the room comes back"
    );
}

/// **Nobody on the `husbandry` role** — the staffing that closes the loop.
const NOBODY_KEEPING: u32 = 0;

/// The grace this fixture requires to be spent: a shortfall that is still being forgiven is not yet
/// the state the remedy is for.
const GRACE_SPENT: u32 = 0;

/// A world with one band **mid-`Tame`** on a domesticable herd it also hunts, its `builders` pool
/// staffed and its `husbandry` role at `keepers`. Returns the app, the band and the herd id.
fn world_with_a_half_tamed_herd(keepers: u32, floor: f32) -> (App, Entity, String) {
    let mut app = build_test_app();
    app.update();
    app.world
        .resource_mut::<core_sim::DiscoveryProgressLedger>()
        .add_progress(FactionId(0), core_sim::HERDING_DISCOVERY_ID, scalar_one());

    // A stationary herd the pastoral rung will actually take, so the gate that closes below is the
    // escapement one and not a species ceiling.
    let (herd_id, position) = {
        let registry = app.world.resource::<core_sim::HerdRegistry>();
        registry
            .herds
            .iter()
            .find(|herd| {
                herd.id.starts_with("game_")
                    && herd.route_length() == 1
                    && herd.species == FIXTURE_SPECIES
            })
            .map(|herd| (herd.id.clone(), herd.position()))
            .unwrap_or_else(|| {
                panic!(
                    "this arm needs a stationary {FIXTURE_SPECIES} on the generated map and \
                     worldgen placed none — see FIXTURE_SPECIES for why the species is pinned"
                )
            })
    };
    // **Half-built, so the entry is a build in flight rather than an unstarted one.** A meter at
    // zero publishes `-1` for its own reason and would prove nothing about the stall.
    let cost = {
        let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
        let fauna = app.world.resource::<core_sim::FaunaConfigHandle>().get();
        let species = {
            let registry = app.world.resource::<core_sim::HerdRegistry>();
            registry
                .find(&herd_id)
                .expect("the fixture herd survives")
                .species
                .clone()
        };
        ladder
            .rung(RungKey::AnimalPastoral)
            .build_cost(fauna.taming_cost_multiplier_for(&species))
            .expect("the pastoral rung builds")
    };
    {
        let mut registry = app.world.resource_mut::<core_sim::HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|herd| herd.id == herd_id)
            .expect("the fixture herd survives");
        // **A healthy flock to start**, so the escapement room is real on turn one and the hunters
        // have to *draw* it away. Seeding a herd already under its floor would stage the stall
        // rather than reproduce it.
        herd.biomass = herd.carrying_capacity;
        herd.set_ladder_position(cost * HALF_BUILT, &core_sim::LadderConfig::builtin());
        // **A half-tamed herd is OWNED** — `accrue_domestication` claims it on the first accrual, so
        // seeding the meter without the owner would be a state the sim cannot produce (and one the
        // fog would hide from the wire).
        herd.owner = Some(FactionId(0));
    }

    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(position.x, position.y)
        .expect("the herd's tile resolves");
    let mut assignments = vec![
        LaborAssignment {
            target: LaborTarget::Hunt {
                fauna_id: herd_id.clone(),
                floor,
            },
            workers: HUNTERS,
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        },
        LaborAssignment {
            target: LaborTarget::Builders,
            workers: BUILDERS,
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        },
    ];
    if keepers > 0 {
        assignments.push(LaborAssignment {
            target: LaborTarget::Husbandry,
            workers: keepers,
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        });
    }
    let staffed: u32 = assignments.iter().map(|row| row.workers).sum();
    let band = app
        .world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 200,
                children: scalar_zero(),
                // **Room to spare, and a full larder.** This arm resolves WHOLE turns (the fog
                // filter on the published herd list needs the visibility sweep), so demographics run
                // too — a band sized to exactly what it staffs, with nothing to eat, shrinks and
                // `normalize` trims away the very rows under measurement.
                working: scalar_from_f32((staffed * WORKFORCE_HEADROOM) as f32),
                elders: scalar_zero(),
                stores: well_fed(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_turn_transfer_received: 0.0,
                last_turn_transfer_sent: 0.0,
                last_morale_delta: scalar_zero(),
                last_morale_cause: MoraleCause::None,
                last_morale_contributions: Default::default(),
                last_fertility_factors: Default::default(),
                discontent_fraction: scalar_zero(),
                grievance: scalar_zero(),
                last_emigrated: 0,
                last_immigrated: 0,
                age_turns: 0,
                generation: 0 as GenerationId,
                faction: FactionId(0),
                knowledge: Vec::new(),
                migration: None,
            },
            ResidentBand,
            core_sim::BandId(FIXTURE_BAND),
            LaborAllocation {
                assignments,
                build_queue: vec![core_sim::BuildQueueEntry {
                    source: BuildSource::Herd(herd_id.clone()),
                    declared: BuildJob::Rung(Improvement::Tame),
                    kit: Some(bare_builders()),
                }],
                ..Default::default()
            },
        ))
        .id();
    (app, band, herd_id)
}

/// **A meter genuinely mid-build**, as a fraction of the rung's cost.
const HALF_BUILT: f32 = 0.5;

/// **THE SPECIES THE ANIMAL ARM IS PINNED TO** — the same one `fauna_husbandry.rs` pins, and for the
/// same reason: *"whichever herd worldgen happened to place first"* is an incidental dependency on
/// map generation rather than a property of the mechanic. The Rabbit Warren breeds fast enough that
/// its escapement room is a real term on turn one, and light enough that a take is not quantised
/// away — which is what lets the hunters actually *draw* the flock to its floor here.
const FIXTURE_SPECIES: &str = "Rabbit Warren";

/// The hunters beside the build — enough to draw the flock down to its floor and hold it there, and
/// **no more**. With the keeping unmet the flock does not regrow, so a heavier crew does not reach
/// the stall sooner; it drives the herd under its extinction floor and takes the fixture's own
/// subject with it.
const HUNTERS: u32 = 2;

/// How much bigger than what it staffs the fixture band is. Idle hands cost this fixture nothing;
/// what they buy is that a demographic wobble cannot shed the rows under measurement.
const WORKFORCE_HEADROOM: u32 = 6;

/// A larder deep enough that a dozen resolved turns cannot starve the fixture band.
fn well_fed() -> LocalStore {
    let mut stores = LocalStore::new();
    stores.add(core_sim::FOOD, scalar_from_f32(50_000.0));
    stores
}

/// One animal turn in the real stage order, then republish.
///
/// **The published herd list is fog-filtered**, and this harness never sweeps visibility — the
/// fixture herd reaches the wire because a half-tamed herd is **owned** (`herd_is_visible` passes a
/// herd its viewer owns), which is what `accrue_domestication` does on the first accrual and what
/// the fixture therefore seeds.
///
/// ⛔ **The fog is NOT switched off instead.** Mutating `SimulationConfig` after Startup regenerates
/// the world — a different map, different herd ids, and a fixture asserting on a herd that no longer
/// exists.
fn resolve_an_animal_turn(app: &mut App) {
    app.world.run_system_once(core_sim::advance_herds);
    app.world.run_system_once(core_sim::advance_husbandry);
    app.world
        .run_system_once(core_sim::advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);
}

/// The herd's live taming meter.
fn herd_meter(app: &App, id: &str) -> f32 {
    let r = app.world.resource::<core_sim::HerdRegistry>();
    match r.find(id) {
        Some(h) => h.rung_work_done(
            core_sim::RungKey::AnimalPastoral,
            &core_sim::LadderConfig::builtin(),
        ),
        None => panic!(
            "herd {id} gone; have {:?}",
            r.herds
                .iter()
                .map(|h| (h.id.clone(), h.biomass))
                .collect::<Vec<_>>()
        ),
    }
}

/// **The five fields the blocked pairing is read from, off the ENCODED buffer** — the countdown, its
/// cause, the keeping shortfall and the grace, on one row, because that is how a client sees them.
fn published_herd(app: &App, id: &str) -> (i32, String, f32, u32, bool) {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let row = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .subsistence()
        .and_then(|section| section.herds())
        .expect("the subsistence section carries the herd list")
        .iter()
        .find(|herd| herd.id().unwrap_or_default() == id)
        .expect("the fixture herd is on the wire — it is OWNED, which passes the fog gate");
    (
        row.buildTurnsRemaining(),
        row.buildBlockedReason().unwrap_or_default().to_string(),
        row.upkeepShortfall(),
        row.neglectGraceRemaining(),
        row.hasNeglectGrace(),
    )
}

/// What a `Tame` on this herd costs, at its species' own multiplier.
fn cost_of_taming(app: &App, id: &str) -> f32 {
    let fauna = app.world.resource::<core_sim::FaunaConfigHandle>().get();
    let registry = app.world.resource::<core_sim::HerdRegistry>();
    let species = registry
        .find(id)
        .expect("the fixture herd survives")
        .species
        .clone();
    app.world
        .resource::<core_sim::LadderConfigHandle>()
        .get()
        .rung(RungKey::AnimalPastoral)
        .build_cost(fauna.taming_cost_multiplier_for(&species))
        .expect("the pastoral rung builds")
}

/// **What this herd's keeping costs in whole hands AT A FULL FLOCK**, read off the shipped ladder.
///
/// Quoted at the herd's **carrying capacity** rather than at its live head count, because the whole
/// point of the remedy is that the flock grows back: an animal rung's rate rides `source_load`
/// (`head count / animals_per_herder`), so a crew sized on a shed-down herd would fall short again
/// the moment it recovered — the shrinking-bill hazard `husbandry.md` names in its own right.
fn keeping_a_herd_needs(app: &App, id: &str) -> u32 {
    let fauna = app.world.resource::<core_sim::FaunaConfigHandle>().get();
    let registry = app.world.resource::<core_sim::HerdRegistry>();
    let herd = registry.find(id).expect("the fixture herd survives");
    let live = core_sim::herd_keeper_load(herd, &fauna);
    let at_capacity = if herd.biomass > 0.0 {
        live * (herd.carrying_capacity / herd.biomass)
    } else {
        live
    };
    app.world
        .resource::<core_sim::LadderConfigHandle>()
        .get()
        .rung(RungKey::AnimalPastoral)
        .upkeep_crew_needed(at_capacity)
}

// ---------------------------------------------------------------------------------------------
// (5b) A build queued since the last turn is not a stall
// ---------------------------------------------------------------------------------------------

/// **A BUILD QUEUED SINCE THE LAST TURN IS NOT A STALL, AND THE WIRE MUST NOT SAY IT IS.**
///
/// One value carried two meanings: `NO_BUILD_TURNS_ESTIMATE` (`-1`) was published both when the
/// estimate pass ran and had no number — nobody on it, a gate refusing, or a running build that
/// banked nothing and is genuinely **stalled** — and when *no pass had ever run for the entry*. The
/// client renders `-1` as `⚠ Stalled 0%`, so a `Cultivate` the player declared a second ago, with a
/// staffed pool standing on it, was reported as a hazard until the next turn cleared it.
///
/// **The three arms are the whole claim**, and each fails on its own:
/// 1. queued, no turn yet → [`BUILD_NOT_YET_ESTIMATED`];
/// 2. one turn with the pool staffed → a **real count**, so the sentinel is transient and not sticky;
/// 3. a live-queued entry the pass *did* reach and could not date → still `-1`, so a genuine stall
///    keeps its own value. **This is the arm the meter cannot decide**: it sits at the same progress
///    as arm 1.
///
/// Plus the control that stops the fix leaking across the map: an **unqueued** patch — which also
/// carries the cleared queue place — must keep reading `-1`.
#[test]
fn a_build_queued_since_the_last_turn_publishes_not_yet_estimated_rather_than_a_stall() {
    // ① QUEUED, AND NO TURN HAS RUN. The fixture spawns the band with its entry already in place,
    //    which is exactly the frame after `cultivate` — the server re-captures on every command.
    let (mut app, _band, sources) = world_with_a_queue(1, BUILDERS);
    let source = sources[0];
    recapture_snapshot_in_place(&mut app.world);
    assert_eq!(
        published_turns(&app, source),
        BUILD_NOT_YET_ESTIMATED,
        "a build the player just queued has not been estimated — it is not stalled"
    );
    assert_eq!(
        published_position(&app, source),
        NOT_IN_ANY_BUILD_QUEUE,
        "fixture: the turn's own reset still stands, which is what says no pass has reached it"
    );

    // …AND THE CONTROL: a patch in NO band's queue carries the same cleared place, and must go on
    // reading the plain no-answer sentinel. Without this the fix would put "starts next turn" on
    // every unworked patch on the map.
    let unqueued = {
        let registry = app.world.resource::<core_sim::ForageRegistry>();
        registry
            .patches
            .values()
            .map(|patch| patch.tile)
            .find(|tile| *tile != source)
            .expect("the generated map carries more than one patch")
    };
    assert_eq!(
        published_turns(&app, unqueued),
        NO_BUILD_TURNS_ESTIMATE,
        "a patch nobody has queued has no answer — it is not a build waiting to start"
    );

    // ② ONE TURN, POOL STAFFED → a real count. The sentinel is a state the first turn leaves.
    resolve_a_turn(&mut app);
    let counted = published_turns(&app, source);
    assert!(
        counted >= 0,
        "the first turn turns the placeholder into a real finish date, got {counted}"
    );

    // ③ A GENUINE STALL KEEPS `-1`. The entry is live-queued and its meter is at the same zero as
    //    arm ①; what differs is that the pass reached it and had no number. Nobody on the pool, so
    //    it is not the blocked head's `-4` either.
    // Nobody on the pool, which is what sends a refused gate down the *no answer* branch instead of
    // the blocked head's `-4`.
    const NOBODY_BUILDING: u32 = 0;
    let (mut app, _band, sources) =
        world_with_a_queue_knowing(1, NOBODY_BUILDING, !THE_GATE_IS_OPEN);
    let stalled_source = sources[0];
    resolve_a_turn(&mut app);
    assert_eq!(
        published_turns(&app, stalled_source),
        NO_BUILD_TURNS_ESTIMATE,
        "a queued entry the pass reached and could not date is a real stall and still says so"
    );
    assert!(
        published_position(&app, stalled_source) >= BUILD_QUEUE_HEAD_POSITION,
        "…because the pass stamped its place in the line, which is the whole test"
    );
}

/// **The 0-based head of a queue**, named because the assertion it appears in is *"the pass stamped
/// a real place"* and not an index comparison.
const BUILD_QUEUE_HEAD_POSITION: i32 = 0;

/// **THE ANIMAL WEB HAS IT TOO, AND FOR THE SAME REASON** — a `Tame` or `Corral` queued since the
/// last turn is not a stall.
///
/// Both webs go through `publish_build_chain` and the same per-turn reset, so this is one fix; it is
/// asserted rather than assumed because the player's report guessed it and a shared publisher is
/// exactly the thing that can be half-wired.
///
/// **The fixture herd is HALF-BUILT**, which is the point: the sentinel is not a reading of the
/// meter. A progress bar cannot tell a fresh entry from a frozen one — only *"has a pass reached
/// it"* can.
#[test]
fn a_herds_build_queued_since_the_last_turn_publishes_not_yet_estimated_too() {
    /// A floor the hunters do not strip to, so the escapement gate stays open and arm ② gets a real
    /// count rather than the stall this file's other animal test walks to.
    const LEAVE_IT_STANDING: f32 = 0.5;

    let (mut app, _band, herd_id) = world_with_a_half_tamed_herd(NOBODY_KEEPING, LEAVE_IT_STANDING);
    recapture_snapshot_in_place(&mut app.world);
    let (turns, _, _, _, _) = published_herd(&app, &herd_id);
    assert_eq!(
        turns, BUILD_NOT_YET_ESTIMATED,
        "a Tame the player just queued is waiting for its first turn, not stalled — and the meter \
         is HALF built, so nothing about this is a progress reading"
    );

    resolve_an_animal_turn(&mut app);
    let (counted, _, _, _, _) = published_herd(&app, &herd_id);
    assert!(
        counted >= 0,
        "the first turn dates the animal web's entry exactly as it dates the plant web's, got \
         {counted}"
    );
}

// ---------------------------------------------------------------------------------------------
// (6) The parking case stays neutral
// ---------------------------------------------------------------------------------------------

/// **A PARKED HALF-BUILT METER IS `-2`, AND THE QUEUE DOES NOT RE-MARK IT** (§4.6b).
///
/// No builders, the keeping met: the balance is exactly zero and the ground stands still,
/// indefinitely and at no risk. That is a decision the player made, not a hazard — so nothing in the
/// queue path may upgrade it to one.
#[test]
fn a_parked_half_built_meter_stays_the_neutral_holding_sentinel() {
    const NOBODY_BUILDING: u32 = 0;

    let (mut app, _band, sources) = world_with_a_queue(ONE_SOURCE, NOBODY_BUILDING);

    // Put real work on the meter — the state a player parks, which an empty meter is not.
    {
        let cost = app
            .world
            .resource::<core_sim::LadderConfigHandle>()
            .get()
            .rung(RungKey::PlantTended)
            .build_cost(core_sim::RUNG_COST_UNSCALED)
            .expect("the tended rung builds");
        let mut registry = app.world.resource_mut::<core_sim::ForageRegistry>();
        let patch = registry
            .patch_mut(sources[0])
            .expect("the fixture patch exists");
        patch.set_ladder_position(cost * HALF_BUILT, &core_sim::LadderConfig::builtin());
    }
    resolve_a_turn(&mut app);

    assert_eq!(
        published_turns(&app, sources[0]),
        BUILD_METER_HOLDS,
        "a parked meter whose keeping is met holds — it is not blocked, and it is not rotting"
    );
    assert_eq!(
        published(&app, sources[0], |patch| patch.meterRotPerTurn()),
        0.0,
        "…and nothing is bleeding off it, which is what makes the sentinel neutral"
    );
}

// ---------------------------------------------------------------------------------------------
// (7) `unqueue` and `abandon`
// ---------------------------------------------------------------------------------------------

/// **`unqueue` WITHDRAWS A DECLARATION AND LEAVES EVERYTHING ELSE STANDING** — the undo a
/// declaration never had (`cultivate <f> <x> <y> 0` *set* the improvement with zero builders rather
/// than clearing it, so an unwanted verb was stuck).
#[test]
fn unqueue_withdraws_the_declaration_and_the_source_stops_publishing_a_job() {
    let (mut app, band, sources) = world_with_a_queue(ONE_SOURCE, BUILDERS);
    resolve_a_turn(&mut app);
    assert!(
        meter(&app, sources[0]) > 0.0,
        "fixture: the build must have started, or 'withdrawing it' says nothing"
    );

    let take_before = app
        .world
        .get::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .workers_on(&LaborTarget::Forage {
            tile: sources[0],
            floor: FOOD_PEAK,
            species: None,
            take_species: TakeSelection::EVERYTHING,
        });
    assert!(
        app.world
            .get_mut::<LaborAllocation>(band)
            .expect("the band keeps its allocation")
            .unqueue_build(&BuildSource::Patch(sources[0])),
        "the entry was there to withdraw"
    );
    let banked = meter(&app, sources[0]);
    resolve_a_turn(&mut app);

    let allocation = app
        .world
        .get::<LaborAllocation>(band)
        .expect("the band keeps its allocation");
    assert!(allocation.build_queue.is_empty(), "the declaration is gone");
    assert_eq!(
        allocation.workers_on(&LaborTarget::Forage {
            tile: sources[0],
            floor: FOOD_PEAK,
            species: None,
            take_species: TakeSelection::EVERYTHING,
        }),
        take_before,
        "…and the take crew is untouched: `unqueue` is the undo for a DECLARATION"
    );
    assert_eq!(
        meter(&app, sources[0]),
        banked,
        "…as is the meter — what was banked stays banked, held by the keeping"
    );
    assert_eq!(
        published_position(&app, sources[0]),
        NOT_IN_ANY_BUILD_QUEUE,
        "and the source says it is in nobody's queue"
    );
}

/// **`abandon` PUTS THE SOURCE DOWN — the row AND its entry — AND LEAVES THE METER ALONE**, which
/// the following turns' decay then bleeds (§2.5).
///
/// It is disposal rather than a smaller share: nothing is destroyed on the spot, so it needs no
/// confirmation, and the ground simply goes back to what it was.
#[test]
fn abandon_drops_the_row_and_its_entry_and_leaves_the_meter_to_rot() {
    let (mut app, band, sources) = world_with_a_queue(ONE_SOURCE, BUILDERS);
    resolve_a_turn(&mut app);
    let banked = meter(&app, sources[0]);
    assert!(
        banked > 0.0,
        "fixture: there must be work on the meter for 'the meter is untouched' to mean anything"
    );

    // The command's whole effect, through the seam it uses.
    assert!(
        app.world
            .get_mut::<LaborAllocation>(band)
            .expect("the band keeps its allocation")
            .drop_source_row(&LaborTarget::Forage {
                tile: sources[0],
                floor: FOOD_PEAK,
                species: None,
                take_species: TakeSelection::EVERYTHING,
            }),
        "the band held the source"
    );

    let allocation = app
        .world
        .get::<LaborAllocation>(band)
        .expect("the band keeps its allocation");
    assert!(
        !allocation
            .assignments
            .iter()
            .any(|row| matches!(row.target, LaborTarget::Forage { .. })),
        "the holding is put down"
    );
    assert!(
        allocation.build_queue.is_empty(),
        "…and its declaration goes with it, on the rule that an entry requires a row"
    );
    assert_eq!(
        meter(&app, sources[0]),
        banked,
        "the meter is untouched on the spot — nothing is destroyed, so nothing is confirmed"
    );

    // **And then the ground goes back to what it was.** With nobody holding it, the meter rots at
    // the rung's own rate over the following turns.
    let grace = {
        let probe = build_test_app();
        let handle = probe.world.resource::<core_sim::LadderConfigHandle>().get();
        handle.rung(RungKey::PlantTended).upkeep_grace_turns()
    };
    for _ in 0..=(grace + 1) {
        resolve_a_turn(&mut app);
    }
    assert!(
        meter(&app, sources[0]) < banked,
        "an abandoned meter is left to rot: {banked} -> {}",
        meter(&app, sources[0])
    );
}

// ---------------------------------------------------------------------------------------------
// (8) `build_order`
// ---------------------------------------------------------------------------------------------

/// **RE-ORDERING MOVES THE POOL, THE SAME TURN** — the queue's defining input (§2.5:
/// *"re-ordering is the one input a list can carry that a stepper cannot"*).
#[test]
fn build_order_moves_the_head_and_the_pool_follows_it_the_same_turn() {
    let (mut app, band, sources) = world_with_a_queue(THE_WHOLE_QUEUE, BUILDERS);
    resolve_a_turn(&mut app);
    let before: Vec<f32> = sources.iter().map(|s| meter(&app, *s)).collect();

    // Put the LAST entry first, which is the whole of what the command does.
    assert!(
        app.world
            .get_mut::<LaborAllocation>(band)
            .expect("the band keeps its allocation")
            .move_build_entry(&BuildSource::Patch(sources[2]), 0),
        "the entry was queued, so it can be moved"
    );
    resolve_a_turn(&mut app);
    let after: Vec<f32> = sources.iter().map(|s| meter(&app, *s)).collect();

    let pool_work = core_sim::activity_work(BUILDERS);
    assert!(
        (after[2] - before[2] - pool_work).abs() < 1e-4,
        "the new head takes the whole pool on the very next turn: {} -> {}",
        before[2],
        after[2]
    );
    assert_eq!(
        after[0], before[0],
        "and the old head stops dead — it is waiting now, and a waiting entry banks nothing"
    );
    assert_eq!(
        published_position(&app, sources[2]),
        0,
        "the wire says where it landed"
    );
}

// ---------------------------------------------------------------------------------------------
// (10) The checkpoint
// ---------------------------------------------------------------------------------------------

/// **THE QUEUE SURVIVES A CHECKPOINT, ORDER INTACT** — and it is the *order* that is checked, under
/// a non-insertion arrangement, because a restore that rebuilt the list from the assignment rows
/// would land on insertion order and pass a test that only counted entries.
///
/// ⛔ **It is also what catches the `PartialEq` trap.** `LaborAllocation`'s equality is intent only;
/// leaving `build_queue` out of it would make two allocations with different queues compare equal,
/// so the rollback record and the command no-op guard would both report *nothing changed* about the
/// one input the whole funding rule reads.
#[test]
fn the_build_queue_survives_a_checkpoint_in_the_order_the_player_set() {
    let (mut app, band, sources) = world_with_a_queue(THE_WHOLE_QUEUE, BUILDERS);

    // A non-trivial order: last first, so insertion order is not the answer.
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("the band keeps its allocation");
        assert!(allocation.move_build_entry(&BuildSource::Patch(sources[2]), 0));
    }
    let expected: Vec<BuildSource> = app
        .world
        .get::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .build_queue
        .iter()
        .map(|entry| entry.source.clone())
        .collect();
    assert_eq!(
        expected[0],
        BuildSource::Patch(sources[2]),
        "fixture: the order under test is not insertion order"
    );

    // **AND THE ENTRY'S KIT RIDES WITH IT.** It is a player decision like the order is, so a
    // restore that dropped it would silently put a job the player sent out bare-handed back on the
    // roster's geared derivation. The whole `LaborAllocation` is cloned into the record, so this
    // falls out — which is exactly why it is asserted rather than assumed.
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("the band keeps its allocation");
        assert!(
            allocation.set_build_entry_kit(&BuildSource::Patch(sources[1]), Some(bare_builders()))
        );
        // …and one entry deliberately left on its own web's derivation, so the restore claim is not
        // satisfied by a constant.
        assert!(allocation.set_build_entry_kit(&BuildSource::Patch(sources[0]), None));
    }
    let expected_kits: Vec<Option<String>> = app
        .world
        .get::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .build_queue
        .iter()
        .map(|entry| entry.kit.as_ref().map(|kit| kit.id().to_string()))
        .collect();
    assert!(
        expected_kits.iter().any(Option::is_some) && expected_kits.iter().any(Option::is_none),
        "fixture: the queue must mix a named kit with a derived one, or the restore claim is \
         satisfied by any constant"
    );

    // **The two queues must not compare equal**, which is the `PartialEq` half.
    let reordered = app
        .world
        .get::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .clone();
    let mut insertion_order = reordered.clone();
    assert!(insertion_order.move_build_entry(&BuildSource::Patch(sources[0]), 0));
    assert_ne!(
        reordered, insertion_order,
        "two allocations differing only in queue ORDER must not compare equal — the rollback \
         record and the no-op guard both read that comparison"
    );

    let state = core_sim::sim_state::capture_sim_state(&app.world);

    // **Lose it, then roll back.** Clearing the live queue is what a rollback has to undo; without
    // this the restore could be a no-op and the test would still pass.
    app.world
        .get_mut::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .build_queue
        .clear();
    core_sim::sim_state::restore_sim_state(&mut app.world, &state);

    // **Found by its queue, not by entity**: a restore respawns the world's bands, so the handle
    // above does not survive it — which is itself the reason the queue has to ride the record
    // rather than be rebuilt from whatever is standing afterwards.
    let landed: Vec<BuildSource> = app
        .world
        .query::<&LaborAllocation>()
        .iter(&app.world)
        .find(|allocation| !allocation.build_queue.is_empty())
        .expect("the restored world carries the band's queue")
        .build_queue
        .iter()
        .map(|entry| entry.source.clone())
        .collect();
    assert_eq!(
        landed, expected,
        "a checkpoint restores the queue the player set, in the order they set it"
    );
    let landed_kits: Vec<Option<String>> = app
        .world
        .query::<&LaborAllocation>()
        .iter(&app.world)
        .find(|allocation| !allocation.build_queue.is_empty())
        .expect("the restored world carries the band's queue")
        .build_queue
        .iter()
        .map(|entry| entry.kit.as_ref().map(|kit| kit.id().to_string()))
        .collect();
    assert_eq!(
        landed_kits, expected_kits,
        "…and the tool the player picked for each job with it: a restore that dropped the kit \
         would put a deliberately bare-handed job back on the geared derivation"
    );
}

// ---------------------------------------------------------------------------------------------
// (11) Totality
// ---------------------------------------------------------------------------------------------

/// **A NEW `BuildJob` OR `BuildSource` VARIANT MUST BREAK THIS** — the exhaustive-match guard, in the
/// shape `the_coded_climb_matches_…` uses.
///
/// The two enums are the queue's whole vocabulary, and every consumer that branches on them is
/// somewhere a new variant could be silently defaulted. This forces the question to be asked.
#[test]
fn every_build_job_and_source_kind_is_stated() {
    let patch = BuildSource::Patch(UVec2::new(1, 1));
    let herd = BuildSource::Herd("game_deer_07".to_string());
    let road = BuildSource::Road(UVec2::new(2, 2));
    for source in [&patch, &herd, &road] {
        match source {
            // A patch is named by its tile; a herd by an id that outlives its position; a ROAD by
            // its tile, because a road IS a tile (`docs/plan_standing_upkeep.md` §4.13b).
            BuildSource::Patch(tile) => assert_eq!(*tile, UVec2::new(1, 1)),
            BuildSource::Herd(id) => assert_eq!(id, "game_deer_07"),
            BuildSource::Road(tile) => assert_eq!(*tile, UVec2::new(2, 2)),
        }
    }
    assert!(
        !road.names(&LaborTarget::Forage {
            tile: UVec2::new(2, 2),
            floor: FOOD_PEAK,
            species: None,
            take_species: core_sim::TakeSelection::EVERYTHING,
        }),
        "A ROAD NAMES NO LABOR ROW, even one on its own tile: its keeper is on the road, and the \
         `Roadwork` role is band-wide and names no one of them"
    );
    assert!(
        !patch.names(&LaborTarget::Hunt {
            fauna_id: "game_deer_07".to_string(),
            floor: FOOD_PEAK,
        }),
        "a patch never names a herd's row"
    );
    assert!(herd.names(&LaborTarget::Hunt {
        fauna_id: "game_deer_07".to_string(),
        floor: FOOD_PEAK,
    }));

    for job in [
        BuildJob::Rung(Improvement::Cultivate),
        BuildJob::Rung(Improvement::Sow),
        BuildJob::Rung(Improvement::Tame),
        BuildJob::Rung(Improvement::Corral),
        BuildJob::ExtendPen,
    ] {
        match job {
            // A rung verb names a meter, so the derived rung can answer for it.
            BuildJob::Rung(improvement) => assert!(!improvement.as_str().is_empty()),
            // A ring names none — that is the gap this kind fills.
            BuildJob::ExtendPen => {}
        }
    }

    // **A standing role is not a build source**, which is what keeps a role row out of the queue.
    for role in [
        LaborTarget::Scout,
        LaborTarget::Warrior,
        LaborTarget::Agriculture,
        LaborTarget::Husbandry,
        LaborTarget::Builders,
    ] {
        assert_eq!(BuildSource::of(&role), None, "{role:?} works no source");
    }
}

/// **THE EMPTY KIT, NAMED ON A FIXTURE'S QUEUE ENTRY** — an isolation, not a default.
///
/// It rides the **entry** because that is where a build's kit lives
/// (`docs/plan_standing_upkeep.md` §4.7a ②); a kit on the `builders` row is not an input at all.
/// An absent kit means *derive from this entry's web*, and the roster's answer (`tillage` for a
/// patch, `hurdling` for a herd) adds `+0.5` work per covered worker per turn. A start-stocked band holds a
/// unit per worker and a half, so at the crews these fixtures staff every builder is geared and the
/// pool delivers half again what it asserts, moving every pacing claim below. Naming `none` holds
/// the gear axis at its identity so these arms measure the **crew**, exactly as
/// `FaunaConfig::without_retreat` holds the retreat at its identity across the hunt suites. The
/// geared default is pinned in `core_sim/tests/build_turns_closed_form.rs`.
fn bare_builders() -> core_sim::KitChoice {
    core_sim::EquipmentConfig::builtin()
        .kit("none")
        .expect("the shipped roster carries the empty kit")
}

// ---------------------------------------------------------------------------------------------
// (12) A PEN RING IS AN ORDINARY BUILD
// ---------------------------------------------------------------------------------------------

/// **The fixture pen's herd id** — seated by hand rather than scouted off worldgen, because the
/// claim is about the *queue*, not about which pennable species a generated map happened to place.
const RING_HERD: &str = "fixture_pen_0";

/// **A fixture display name that is NOT a roster species**, so its per-species husbandry density
/// resolves to the neutral default — the same isolation `grazing_2d_pen.rs` makes, and for the same
/// reason: this file measures the build queue, not the density ladder.
const RING_SPECIES: &str = "Fixture Warren";

/// The seated pen's ecology. None of it is under test — the pen only has to stand, be owned and be
/// tended — so each is a fixture constant rather than a tuning lever.
const RING_HERD_BIOMASS: f32 = 150.0;
const RING_HERD_CAPACITY: f32 = 300.0;
const RING_HERD_FODDER: f32 = 0.10;
const RING_HERD_REGROWTH: f32 = 0.35;
const RING_HERD_BODY_MASS: f32 = 40.0;

/// The keepers on the pen — enough that the herd is genuinely tended, and few enough that the take
/// crew is not what these arms measure.
const RING_KEEPERS: u32 = 2;

/// A world with **one band that keeps a pen mid-ring and gathers a patch beside it**, its queue
/// `[ExtendPen(pen), Cultivate(patch)]` — the ring at the head, one ordinary entry behind it.
///
/// The two sources are deliberately the **same tile**: the band stands on it, so the pen is inside
/// the hunt leash and the patch inside the work range without the fixture having to reason about
/// either distance.
fn world_with_a_ring_at_the_head(builders: u32) -> (App, Entity, String, UVec2) {
    let mut app = build_test_app();
    app.update();
    let source = cultivable_sites_in_one_work_range(&mut app)[0];
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(source.x, source.y)
        .expect("the fixture tile resolves");
    app.world
        .resource_mut::<core_sim::DiscoveryProgressLedger>()
        .add_progress(
            FactionId(0),
            core_sim::CULTIVATION_DISCOVERY_ID,
            scalar_one(),
        );
    seat_a_pen_mid_ring(&mut app, source);

    let assignments = vec![
        LaborAssignment {
            target: LaborTarget::Hunt {
                fauna_id: RING_HERD.to_string(),
                floor: FOOD_PEAK,
            },
            workers: RING_KEEPERS,
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        },
        LaborAssignment {
            target: LaborTarget::Forage {
                tile: source,
                floor: FOOD_PEAK,
                species: None,
                take_species: TakeSelection::EVERYTHING,
            },
            workers: GATHERERS,
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        },
        LaborAssignment {
            target: LaborTarget::Builders,
            workers: builders,
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        },
        LaborAssignment {
            target: LaborTarget::Agriculture,
            workers: keeping_for(ONE_SOURCE),
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        },
        LaborAssignment {
            target: LaborTarget::Husbandry,
            workers: RING_KEEPERS,
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        },
    ];
    let staffed: u32 = assignments.iter().map(|row| row.workers).sum();
    let band = app
        .world
        .spawn((
            fixture_cohort(tile, staffed),
            ResidentBand,
            core_sim::BandId(FIXTURE_BAND),
            LaborAllocation {
                assignments,
                build_queue: vec![
                    core_sim::BuildQueueEntry {
                        source: BuildSource::Herd(RING_HERD.to_string()),
                        declared: BuildJob::ExtendPen,
                        kit: Some(bare_builders()),
                    },
                    core_sim::BuildQueueEntry {
                        source: BuildSource::Patch(source),
                        declared: BuildJob::Rung(Improvement::Cultivate),
                        kit: Some(bare_builders()),
                    },
                ],
                ..Default::default()
            },
        ))
        .id();
    // **A RING EATS THE PEN'S OWN PILE** (`docs/plan_standing_upkeep.md` §4.9 item 12) — widening a
    // fence is a pen build, so a band with no panels is materials-blocked and this fixture would
    // measure a stall it staged itself instead of the countdown it means to read.
    pen_materials_support::stock_pen_materials(&mut app.world, band);
    (app, band, RING_HERD.to_string(), source)
}

/// Seat a **built, owned pen with a ring already in flight** at `tile` — the state
/// `corral` followed by `extend_pen` leaves behind, written straight onto the herd so the arm does
/// not have to build two rungs first.
fn seat_a_pen_mid_ring(app: &mut App, tile: UVec2) {
    let radius_max = app
        .world
        .resource::<core_sim::FaunaConfigHandle>()
        .get()
        .husbandry
        .pen_radius_max;
    let mut registry = app.world.resource_mut::<core_sim::HerdRegistry>();
    let mut herd = core_sim::Herd::new(
        RING_HERD.to_string(),
        RING_SPECIES.to_string(),
        core_sim::SizeClass::Small,
        vec![tile],
        RING_HERD_BIOMASS,
        RING_HERD_CAPACITY,
        RING_HERD_FODDER,
        RING_HERD_REGROWTH,
        RING_HERD_BODY_MASS,
    );
    herd.tame_outright(FactionId(0), &core_sim::LadderConfig::builtin());
    assert!(
        herd.corral_at(tile, &core_sim::LadderConfig::builtin()),
        "fixture: the pen must stand before a ring can widen it"
    );
    assert!(
        herd.begin_pen_extension(radius_max),
        "fixture: a built pen below the maximum radius may begin a ring"
    );
    registry.herds.push(herd);
}

/// The fixture band's cohort — sized to what it staffs, and with a larder deep enough that the pen's
/// own feed bill cannot starve it out from under the arm.
fn fixture_cohort(tile: Entity, staffed: u32) -> PopulationCohort {
    PopulationCohort {
        home: tile,
        current_tile: tile,
        size: 200,
        children: scalar_zero(),
        working: scalar_from_f32(staffed as f32),
        elders: scalar_zero(),
        stores: {
            let mut stores = LocalStore::new();
            stores.add(core_sim::FOOD, scalar_from_f32(FIXTURE_LARDER));
            stores
        },
        morale: scalar_one(),
        last_food_consumption: 0.0,
        last_turn_transfer_received: 0.0,
        last_turn_transfer_sent: 0.0,
        last_morale_delta: scalar_zero(),
        last_morale_cause: MoraleCause::None,
        last_morale_contributions: Default::default(),
        last_fertility_factors: Default::default(),
        discontent_fraction: scalar_zero(),
        grievance: scalar_zero(),
        last_emigrated: 0,
        last_immigrated: 0,
        age_turns: 0,
        generation: 0 as GenerationId,
        faction: FactionId(0),
        knowledge: Vec::new(),
        migration: None,
    }
}

/// A larder deep enough that the pen's larder bill cannot starve the fixture band over the handful
/// of turns these arms resolve.
const FIXTURE_LARDER: f32 = 50_000.0;

/// One pen turn, then republish.
///
/// **`advance_herds` is not decoration** — the published herd list is built from `HerdTelemetry`,
/// which that pass fills, so a fixture that ran the labor system alone would have its pen on no
/// wire at all. The **husbandry** sweep is deliberately left out: it rules on neglect, and these
/// arms are about the queue.
fn resolve_a_pen_turn(app: &mut App) {
    app.world.run_system_once(core_sim::advance_herds);
    // **The two Logistics decay passes, in stage order, before Population.** They are what clears
    // `upkeep_supplied` / `upkeep_demanded`; without them a second call to this helper would bank a
    // second turn's keeping on top of the first against a bill stamped once, and read the pen as
    // better kept than it is. `advance_labor_allocation` refuses to do that quietly.
    app.world.run_system_once(core_sim::advance_cultivation);
    app.world.run_system_once(core_sim::advance_husbandry);
    app.world
        .run_system_once(core_sim::advance_labor_allocation);
    recapture_snapshot_in_place(&mut app.world);
}

/// **Read the fixture pen's row off the ENCODED buffer** and hand it to `read`.
///
/// One body for every ring readout below, so a **pair** of fields a client divides is decoded from
/// ONE encode of ONE snapshot — two helpers each doing their own encode could disagree.
fn with_published_ring<T>(
    app: &App,
    id: &str,
    read: impl FnOnce(shadow_scale_flatbuffers::generated::shadow_scale::sim::HerdTelemetryState) -> T,
) -> T {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let herd = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .subsistence()
        .and_then(|section| section.herds())
        .expect("the subsistence section carries the herd list")
        .iter()
        .find(|herd| herd.id().unwrap_or_default() == id)
        .expect("the fixture pen is on the wire — it is OWNED, which passes the fog gate");
    read(herd)
}

/// **The herd row's `buildTurnsRemaining`, off the ENCODED buffer.**
fn published_ring_turns(app: &App, id: &str) -> i32 {
    with_published_ring(app, id, |herd| herd.buildTurnsRemaining())
}

/// **The ring's published meter and the cost it completes at**, as one read of one buffer.
fn published_ring_meter(app: &App, id: &str) -> (f32, f32) {
    with_published_ring(app, id, |herd| {
        (herd.penExtendProgress(), herd.penExtendCost())
    })
}

/// **What a ring costs, off the shipped ladder** — a ring is priced on the `animal:pen` rung it
/// widens (`labor.rs`'s `ring_cost`), so a retune moves the fixture with the game.
fn pen_ring_work_cost() -> f32 {
    build_test_app()
        .world
        .resource::<core_sim::LadderConfigHandle>()
        .get()
        .rung(RungKey::AnimalPen)
        .build_cost(core_sim::RUNG_COST_UNSCALED)
        .expect("the pen rung builds")
}

/// **A RING AT THE HEAD PUBLISHES ITS OWN COUNTDOWN, AND POISONS NOTHING BEHIND IT.**
///
/// `extend_pen` is a one-click shipped button, so a ring at the head of a band's queue is ordinary
/// play. A ring that recorded no build quote made `publish_build_chain` mint
/// [`sim_schema::BUILD_QUEUE_BLOCKED`] for it — *the builders are standing on this entry and its own
/// gate refuses it* — which was false; and `carried` then handed that same `-4` to **every other
/// source the band works**, every turn, while the ring accrued perfectly normally.
///
/// **Asserted on the ENCODED snapshot**, because the poisoning travels through the published chain,
/// and on **both** rows, because an arm that only checked the ring's own date would pass on a fix
/// that merely stopped the mint without quoting the ring.
#[test]
fn a_ring_at_the_head_publishes_a_real_countdown_and_leaves_the_queue_behind_it_alone() {
    let (mut app, _band, herd_id, source) = world_with_a_ring_at_the_head(BUILDERS);
    resolve_a_pen_turn(&mut app);

    let ring = published_ring_turns(&app, &herd_id);
    let behind = published_turns(&app, source);
    println!("ring publishes {ring}; the entry behind it publishes {behind}");
    assert!(
        ring > 0,
        "a ring the pool is raising has a finish date like any other build, not {ring}"
    );
    assert_ne!(
        ring, BUILD_QUEUE_BLOCKED,
        "…and it is emphatically not 'blocked': the ring is accruing"
    );
    assert!(
        behind > ring,
        "…and the entry behind it is CHAINED behind the ring ({behind} should follow {ring}), \
         not poisoned by it"
    );
}

/// **THE RING'S METER SHIPS WITH ITS DENOMINATOR, AND BOTH ARE IN WORK UNITS.**
///
/// `penExtendProgress` was a normalized `0..1` fraction until `docs/plan_standing_upkeep.md` §4.8
/// priced improvements in **work**; it has been a work COUNT ever since, and the wire went on
/// publishing it alone. A client multiplying it by 100 rendered **"Fencing 6900%"** for a ring 69
/// work units in — the meter was right, the denominator was missing.
///
/// **Asserted on the ENCODED snapshot**, because an arm reading `Herd::pen_extend_cost` in process
/// would pass even if the capture never wrote the field — which is the whole defect.
///
/// **The pair is checked against the countdown beside it**, not against a hand-written fraction: the
/// remaining work the published pair implies, over the pool's own supply, is exactly what
/// `buildTurnsRemaining` says. Two published fields that divide to one another are the proof that
/// they share units.
#[test]
fn a_ring_publishes_the_cost_its_progress_completes_at() {
    let (mut app, _band, herd_id, _source) = world_with_a_ring_at_the_head(BUILDERS);
    resolve_a_pen_turn(&mut app);

    let (progress, cost) = published_ring_meter(&app, &herd_id);
    println!("the ring publishes {progress} of {cost} work units");
    assert_eq!(
        cost,
        pen_ring_work_cost(),
        "a ring costs what the pen rung it widens costs — the wire must publish THAT, not a \
         normalized 1.0"
    );
    assert!(
        progress > 0.0 && progress < cost,
        "one turn of {BUILDERS} builders banks part of the ring, so the published meter is strictly \
         between nothing and the whole cost: {progress} of {cost}"
    );

    // The pair divides to the fraction the sim computes — and its remainder, over the bare pool's
    // own supply, dates the ring exactly as the independently published countdown does.
    let supply = core_sim::pool_work_supply(BUILDERS, core_sim::NO_BUILD_GEAR);
    let expected = core_sim::build_turns_remaining(cost, progress, supply)
        .expect("a staffed pool has a finish date") as i32;
    assert_eq!(
        published_ring_turns(&app, &herd_id),
        expected,
        "the published meter/cost pair must date the ring the same way the published countdown \
         does — if they disagree, one of them is in the wrong units"
    );
}

// ---------------------------------------------------------------------------------------------
// (13) AN ENTRY WHOSE RUNG ANOTHER BAND FINISHED IS RETIRED
// ---------------------------------------------------------------------------------------------

/// **A pool big enough to finish `plant:tended` in ONE turn** — the whole `work_cost` in hands, at
/// `PER_WORKER_OUTPUT` each and bare gear. Read off the shipped ladder so a retune moves the fixture
/// with the game.
fn a_pool_that_finishes_a_cultivate_in_one_turn() -> u32 {
    (tended_work_cost() / core_sim::PER_WORKER_OUTPUT).ceil() as u32
}

/// `plant:tended`'s own `work_cost` — the bar a bare crew strikes, since [`bare_builders`] takes
/// nothing off it.
fn tended_work_cost() -> f32 {
    build_test_app()
        .world
        .resource::<core_sim::LadderConfigHandle>()
        .get()
        .rung(RungKey::PlantTended)
        .build_cost(core_sim::RUNG_COST_UNSCALED)
        .expect("the tended rung builds")
}

/// The band whose queue the arm reads — the **survivor**, the one that did not finish the rung.
const SURVIVOR_BAND: u64 = FIXTURE_BAND;

/// The band that finishes the shared rung out from under the survivor, in one turn.
const FINISHER_BAND: u64 = FIXTURE_BAND + 1;

/// **TWO BANDS OF ONE FACTION ON ONE SOURCE**, which a single `cultivate` produces: the command
/// enqueues on **every** band with workers on that source. The finisher carries the whole rung's
/// worth of builders and one entry; the survivor carries an ordinary pool and a **second** entry
/// behind the shared one, so the arm can read what the dead head does to the entry below it.
fn world_with_two_bands_on_one_source() -> (App, Entity, Vec<UVec2>) {
    let mut app = build_test_app();
    app.update();
    let sources: Vec<UVec2> = cultivable_sites_in_one_work_range(&mut app)
        .into_iter()
        .take(2)
        .collect();
    let anchor = sources[0];
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(anchor.x, anchor.y)
        .expect("the fixture tile resolves");
    app.world
        .resource_mut::<core_sim::DiscoveryProgressLedger>()
        .add_progress(
            FactionId(0),
            core_sim::CULTIVATION_DISCOVERY_ID,
            scalar_one(),
        );

    let gather = |source: UVec2| LaborAssignment {
        target: LaborTarget::Forage {
            tile: source,
            floor: FOOD_PEAK,
            species: None,
            take_species: TakeSelection::EVERYTHING,
        },
        workers: GATHERERS,
        kit: None,
        priority: SourcePriority::default(),
        upkeep_kit: None,
    };
    let cultivate = |source: UVec2| core_sim::BuildQueueEntry {
        source: BuildSource::Patch(source),
        declared: BuildJob::Rung(Improvement::Cultivate),
        kit: Some(bare_builders()),
    };

    let finisher = vec![
        gather(sources[0]),
        LaborAssignment {
            target: LaborTarget::Builders,
            workers: a_pool_that_finishes_a_cultivate_in_one_turn(),
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        },
        LaborAssignment {
            target: LaborTarget::Agriculture,
            workers: keeping_for(ONE_SOURCE),
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        },
    ];
    let survivor = vec![
        gather(sources[0]),
        gather(sources[1]),
        LaborAssignment {
            target: LaborTarget::Builders,
            workers: BUILDERS,
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        },
        LaborAssignment {
            target: LaborTarget::Agriculture,
            workers: keeping_for(2),
            kit: None,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        },
    ];
    let finisher_staffed: u32 = finisher.iter().map(|row| row.workers).sum();
    let survivor_staffed: u32 = survivor.iter().map(|row| row.workers).sum();

    app.world.spawn((
        fixture_cohort(tile, finisher_staffed),
        ResidentBand,
        core_sim::BandId(FINISHER_BAND),
        LaborAllocation {
            assignments: finisher,
            build_queue: vec![cultivate(sources[0])],
            ..Default::default()
        },
    ));
    let band = app
        .world
        .spawn((
            fixture_cohort(tile, survivor_staffed),
            ResidentBand,
            core_sim::BandId(SURVIVOR_BAND),
            LaborAllocation {
                assignments: survivor,
                build_queue: vec![cultivate(sources[0]), cultivate(sources[1])],
                ..Default::default()
            },
        ))
        .id();
    (app, band, sources)
}

/// This band's queue, as the sources it names in order.
fn queued_sources(app: &App, band: Entity) -> Vec<BuildSource> {
    app.world
        .get::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .build_queue
        .iter()
        .map(|entry| entry.source.clone())
        .collect()
}

/// Every feed detail line the log carries, so an arm can say *which* line it wanted and print the
/// rest when it is not there.
fn feed_details(app: &App) -> Vec<String> {
    app.world
        .resource::<core_sim::CommandEventLog>()
        .iter()
        .filter_map(|entry| entry.detail.clone())
        .collect()
}

/// **AN ENTRY WHOSE RUNG IS ALREADY STANDING IS RETIRED, AND THE PLAYER IS TOLD.**
///
/// `cultivate` enqueues on **every** band of the faction working the source, so two bands on one
/// patch is the ordinary result of one command. When one of them finishes the rung, the other's
/// entry derives no verb at all: its whole `builders` pool was aimed at the head and **no arm
/// consumed it**, `completed` never fired, and `prune_build_queue` only drops entries whose *row* is
/// gone — so the pool banked nothing, for ever, silently. Worse, the projection of the patch's
/// **next** rung was consumed as the dead head's own span, mis-dating every entry behind it.
#[test]
fn an_entry_whose_rung_another_band_finished_is_retired_and_the_pool_moves_on() {
    let (mut app, survivor, sources) = world_with_two_bands_on_one_source();
    // Two turns: whichever band the query visits first, the shared rung is standing and the
    // survivor has been visited once with it standing.
    resolve_a_turn(&mut app);
    resolve_a_turn(&mut app);

    assert!(
        app.world
            .resource::<core_sim::ForageRegistry>()
            .patch(sources[0])
            .expect("the shared patch survives")
            .is_cultivated(),
        "fixture: the finisher band must actually have finished the shared rung"
    );

    let queue = queued_sources(&app, survivor);
    println!("survivor queue after the rung was finished: {queue:?}");
    assert_eq!(
        queue,
        vec![BuildSource::Patch(sources[1])],
        "the dead entry leaves the survivor's queue and the next one becomes the head"
    );

    let details = feed_details(&app);
    assert!(
        details
            .iter()
            .any(|detail| detail.contains("action=build_retired")),
        "…and the player is told why the job left the list; the feed carried only {details:?}"
    );

    // **The next entry's date is its OWN span**, not the dead head's cumulative sum: bare gear, so
    // the bar is the rung's raw `work_cost`, and the keeping is staffed, so the balance is the
    // pool's whole output.
    let banked = meter(&app, sources[1]);
    let expected = core_sim::build_turns_remaining(
        tended_work_cost(),
        banked,
        BUILDERS as f32 * core_sim::PER_WORKER_OUTPUT,
    )
    .expect("a staffed head finishes");
    assert_eq!(
        published_turns(&app, sources[1]),
        expected as i32,
        "the entry behind the retired one publishes its own span from {banked} banked"
    );

    // …and the pool is genuinely free: the new head's meter moves on the following turn.
    let before = meter(&app, sources[1]);
    resolve_a_turn(&mut app);
    assert!(
        meter(&app, sources[1]) > before,
        "the survivor's builders fund the new head: {before} -> {}",
        meter(&app, sources[1])
    );
}

// ---------------------------------------------------------------------------------------------
// (14) A RING THAT LEAVES THE QUEUE IS CANCELLED, NOT STRANDED
// ---------------------------------------------------------------------------------------------

/// **CAN THE PLAYER EXTEND THIS PEN AGAIN?** — the promise, asked through the very guard
/// `handle_extend_pen` asks it through, so the command and this arm can never disagree. Acceptance
/// is what the player experiences; `pen_extending` is only the mechanism.
fn a_second_ring_is_accepted(app: &mut App, id: &str) -> bool {
    let radius_max = app
        .world
        .resource::<core_sim::FaunaConfigHandle>()
        .get()
        .husbandry
        .pen_radius_max;
    app.world
        .resource_mut::<core_sim::HerdRegistry>()
        .herds
        .iter_mut()
        .find(|herd| herd.id == id)
        .expect("the fixture pen survives")
        .begin_pen_extension(radius_max)
}

/// **Walk the band clean off the map's other side**, so its Hunt row is past the leash and lapses —
/// the third exit, and the one no command issues.
fn walk_the_band_out_of_reach(app: &mut App, band: Entity, from: UVec2) {
    let (width, height) = {
        let registry = app.world.resource::<TileRegistry>();
        (registry.width, registry.height)
    };
    let far = UVec2::new((from.x + width / 2) % width, (from.y + height / 2) % height);
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(far.x, far.y)
        .expect("the far tile resolves");
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the band keeps its cohort")
        .current_tile = tile;
}

/// **A RING THAT LEAVES THE BUILD QUEUE CAN BE STARTED AGAIN.**
///
/// `extend_pen` sets `pen_extending` *before* it queues, and only completion cleared it. So an entry
/// dropped mid-ring left the flag set with nothing left to fund it, and
/// `Herd::begin_pen_extension` refuses while it is set: a **permanent** dead end on a pen, one `✕`
/// click away.
///
/// **The banked ring progress goes with it, and that is the honest state**: `begin_pen_extension`
/// resets `pen_extend_progress` on every start, so a preserved meter could never be resumed — it
/// would be a number nothing can read.
///
/// There are **three** exits and each gets its own arm, because they are three different seams and a
/// single test would report only the first one that broke. This is `unqueue`.
#[test]
fn an_unqueued_ring_frees_the_pen_to_be_extended_again() {
    let (mut app, band, herd_id, _) = world_with_a_ring_at_the_head(BUILDERS);
    resolve_a_pen_turn(&mut app);
    let ring_source = BuildSource::Herd(herd_id.clone());
    assert!(
        core_sim::unqueue_build_and_cancel_ring(&mut app.world, band, &ring_source),
        "fixture: the entry was there to withdraw"
    );
    println!("after unqueue: is a second ring accepted?");
    assert!(
        a_second_ring_is_accepted(&mut app, &herd_id),
        "an unqueued ring frees the pen: `extend_pen` must be accepted again"
    );
}

/// The `abandon` exit — the whole holding is put down, entry and all
/// ([`an_unqueued_ring_frees_the_pen_to_be_extended_again`] has the mechanism).
#[test]
fn an_abandoned_pen_frees_its_ring_to_be_started_again() {
    let (mut app, band, herd_id, _) = world_with_a_ring_at_the_head(BUILDERS);
    resolve_a_pen_turn(&mut app);
    let keeper_row = LaborTarget::Hunt {
        fauna_id: herd_id.clone(),
        floor: FOOD_PEAK,
    };
    assert!(
        core_sim::drop_holding_and_cancel_ring(&mut app.world, band, &keeper_row),
        "fixture: the band held the pen"
    );
    println!("after abandon: is a second ring accepted?");
    assert!(
        a_second_ring_is_accepted(&mut app, &herd_id),
        "an abandoned pen frees the ring: `extend_pen` must be accepted again"
    );
}

/// The **LAPSE** exit — nobody issued a command at all, the keepers simply walked out of reach and
/// the turn's prune took the entry. It is the easiest of the three to miss and it strands the ring
/// identically ([`an_unqueued_ring_frees_the_pen_to_be_extended_again`] has the mechanism).
#[test]
fn a_lapsed_keeper_row_frees_its_ring_to_be_started_again() {
    let (mut app, band, herd_id, source) = world_with_a_ring_at_the_head(BUILDERS);
    resolve_a_pen_turn(&mut app);
    walk_the_band_out_of_reach(&mut app, band, source);
    resolve_a_pen_turn(&mut app);
    assert!(
        queued_sources(&app, band).is_empty(),
        "fixture: the lapsed keeper row takes its ring entry with it"
    );
    println!("after the lapse: is a second ring accepted?");
    assert!(
        a_second_ring_is_accepted(&mut app, &herd_id),
        "a lapsed keeper row frees the ring: `extend_pen` must be accepted again"
    );
}

// ---------------------------------------------------------------------------------------------
// (13) A RUNG THAT ERODES BACK BELOW ITS COST IS REPAIRED BY RE-QUEUEING IT
// ---------------------------------------------------------------------------------------------
//
// `docs/plan_standing_upkeep.md` §2.4: *"A rung that erodes back below its cost is NOT re-adopted…
// repairing it is a fresh decision the player makes by putting it back in the queue."* Completion
// retires the entry (arm 2 above) and the pool only ever aims at an entry, so **the whole repair
// path is the queue**. The verb's own gate is pinned in `bin/server.rs`
// (`an_eroded_tended_patch_is_re_tendable_and_a_full_one_is_not`); what is pinned here is that the
// queue then actually does the work — and that an eroded meter **with no entry** publishes no
// countdown, which is the reporting half the derived-verb quote had regressed.

/// See [`erode_the_finished_meter`] — comfortably above the shipped `retain_fraction` of `0.75`, so
/// the rung is unambiguously still held and the only thing short is the meter.
const ERODED_BUT_STILL_HELD: f32 = 0.99;

/// A bound on the turns a two-builder pool needs to buy back one per-cent of a 50-unit job. Its only
/// job is to keep a broken repair from hanging the suite.
const REPAIR_LIMIT: usize = 20;

/// A generous bound on `work_cost / (builders × PER_WORKER_OUTPUT)` for one whole Cultivate — a
/// guard against hanging, not a prediction.
const A_WHOLE_CULTIVATE: usize = 80;

/// **The state a finished Cultivate decays into**: the meter below its cost, the bar still stamped,
/// and no queue entry — exactly what completion plus a few unkept turns leave behind. Written
/// straight onto the patch so the arm is about the *repair*, not about how the erosion got there.
fn erode_the_finished_meter(app: &mut App, source: UVec2) -> f32 {
    let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
    let cost = ladder
        .rung(RungKey::PlantTended)
        .build_cost(core_sim::RUNG_COST_UNSCALED)
        .expect("the tended rung has a build meter");
    let mut registry = app.world.resource_mut::<core_sim::ForageRegistry>();
    let patch = registry
        .patch_mut(source)
        .expect("the fixture patch is there");
    patch.set_ladder_position(cost * ERODED_BUT_STILL_HELD, &ladder);
    // **The retention bar is deleted** (`docs/plan_standing_upkeep.md` §2.8), so the state this
    // fixture used to author — *tended AND still building* — no longer exists: a position below the
    // rung's top is simply not that rung. What the test is about is unchanged, because the thing it
    // pins is the **queue**: an eroded meter with no entry publishes no estimate, and the same patch
    // once queued banks its way back to a full rung.
    assert!(
        !patch.is_cultivated(),
        "fixture: the patch must be below the rung's top, or there is nothing to re-queue"
    );
    cost
}

/// **THE PAIR.** An eroded meter with **no entry** publishes *no estimate* — nobody is building it
/// and nobody will until the player says so — and the **same patch, once queued**, publishes a real
/// countdown and banks progress back to a full meter.
///
/// The first half is the regression: the four build arms are entered on the **derived** verb, which
/// answers for any meter carrying progress, so an eroded patch pushed a running quote with no entry
/// behind it and `publish_build_chain`'s unqueued tail dated it at the **full pool** — a confident
/// `≈1 turn`, every turn, for ever, for a build with no builders on it.
#[test]
fn an_unqueued_eroded_meter_publishes_no_estimate_and_a_re_queued_one_repairs_it() {
    let (mut app, band, sources) = world_with_a_queue(ONE_SOURCE, BUILDERS);
    let source = sources[0];

    // **The Cultivate is RUN, not fabricated**, and that is load-bearing: a patch whose meter was
    // written straight onto it carries no committed crop, so the rung's own `NoCrop` gate would
    // refuse it and the arm would publish "no estimate" for a reason that has nothing to do with
    // the queue — a fixture that passes with the defect intact. Building it for real commits the
    // species, sets the owner, and retires the entry exactly as play does.
    let mut built = false;
    for _ in 0..A_WHOLE_CULTIVATE {
        resolve_a_turn(&mut app);
        if queued_sources(&app, band).is_empty() {
            built = true;
            break;
        }
    }
    assert!(
        built,
        "fixture: the Cultivate finishes and retires its entry"
    );
    assert!(
        app.world
            .resource::<core_sim::ForageRegistry>()
            .patch(source)
            .expect("the fixture patch survives")
            .species
            .is_some(),
        "fixture: a real Cultivate commits the crop, which is what keeps the rung's gate open"
    );

    // Now wear the finished meter back below its cost — the state a few unkept turns produce.
    let cost = erode_the_finished_meter(&mut app, source);
    resolve_a_turn(&mut app);

    assert_eq!(
        published_turns(&app, source),
        NO_BUILD_TURNS_ESTIMATE,
        "a build with no entry and no builders has no answer to give — it must not publish one"
    );
    assert_eq!(
        published_position(&app, source),
        NOT_IN_ANY_BUILD_QUEUE,
        "…and it says plainly that it is in no queue"
    );
    let unbuilt = meter(&app, source);
    assert!(
        unbuilt < cost,
        "fixture: the meter is genuinely short of its cost ({unbuilt} of {cost})"
    );

    // **The repair is one command: put it back in the queue.** The entry brings the builders — the
    // pool is aimed at the head and at nothing else — so nothing else has to change.
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("the band keeps its allocation");
        allocation.enqueue_build(
            BuildSource::Patch(source),
            BuildJob::Rung(Improvement::Cultivate),
        );
    }
    resolve_a_turn(&mut app);

    assert!(
        meter(&app, source) > unbuilt,
        "re-queueing puts the builders back on it: the meter has to move"
    );
    let quoted = published_turns(&app, source);
    assert!(
        quoted > 0,
        "…and the countdown comes back as a real number, not the sentinel ({quoted})"
    );

    let mut repaired = false;
    for _ in 0..REPAIR_LIMIT {
        resolve_a_turn(&mut app);
        if meter(&app, source) >= cost {
            repaired = true;
            break;
        }
    }
    assert!(
        repaired,
        "the repair finishes: the meter reaches its own cost again (stalled at {} of {cost})",
        meter(&app, source)
    );
    assert!(
        queued_sources(&app, band).is_empty(),
        "…and the repaired entry retires like any other finished job"
    );
}

// ---------------------------------------------------------------------------------------------
// (14) A KEEPING TOOL WEARS ON THE WORK IT SUPPLIED
// ---------------------------------------------------------------------------------------------
//
// `WearQuantum::UpkeepWork`. §4.8 made one supply expression feed both accounts, but only the build
// half had a quantum — so a hoe raised what a keeper supplied **for ever, free**. The charge is on
// the work the pool actually **supplied** (`forage::patch_upkeep_supply`), which is what makes the
// pair below true: keeping something wears the gear, keeping nothing wears none of it.

/// The keeping tool the plant web's roster derives for the `agriculture` role (`tillage` → hoes).
/// It wears on `build_progress` too, so both arms here staff **no builders**: the only work these
/// hoes can be doing is the keeping.
const KEEPING_TOOL: &str = "hoes";

/// No builders at all, so nothing but the keeping can touch the gear.
const NOBODY_BUILDING: u32 = 0;

/// Enough turns that a per-turn charge is unmistakable against float noise.
const A_FEW_TURNS: usize = 5;

/// Hand the band a real ledger — without one the labor loop has nothing to charge, while the *rate*
/// resolves off the absent-component fallback either way.
fn stock_the_band(app: &mut App, band: Entity) {
    let config = app
        .world
        .resource::<core_sim::EquipmentConfigHandle>()
        .get();
    let working = app
        .world
        .get::<PopulationCohort>(band)
        .expect("the fixture band has a cohort")
        .working
        .to_f32();
    let ledger = core_sim::BandEquipment::start_stocked_for(&config, working);
    app.world.entity_mut(band).insert(ledger);
}

/// What is left of the band's keeping tool, in condition.
fn tool_life(app: &App, band: Entity) -> f32 {
    let config = app
        .world
        .resource::<core_sim::EquipmentConfigHandle>()
        .get();
    app.world
        .get::<core_sim::BandEquipment>(band)
        .expect("the fixture band carries a ledger")
        .remaining(KEEPING_TOOL, &config)
}

/// **THE PAIR.** A keeping pool that actually holds a meter spends its tools on the work it
/// supplied; a pool with nothing to keep spends nothing at all.
///
/// Both halves run the same band, the same role and the same turns — only the *ground* differs — so
/// a charge that had crept onto the head count or onto the demand would fail the second half, and
/// one that never fired at all would fail the first.
#[test]
fn a_keeping_pool_wears_its_tools_on_what_it_supplied_and_an_idle_one_wears_nothing() {
    // (a) A meter at risk: the keepers hold it, so the hoes are worked.
    let (mut app, band, sources) = world_with_a_queue(ONE_SOURCE, NOBODY_BUILDING);
    stock_the_band(&mut app, band);
    erode_the_finished_meter(&mut app, sources[0]);
    let fresh = tool_life(&app, band);
    for _ in 0..A_FEW_TURNS {
        resolve_a_turn(&mut app);
    }
    let kept = tool_life(&app, band);
    assert!(
        kept < fresh,
        "a keeper who supplied work wore that work's worth of tool ({fresh} → {kept})"
    );

    // (b) The same band, the same keepers, the same turns — over ground with **nothing** banked on
    // it. No source claims a share, so no work is supplied and no gear is spent.
    let (mut idle_app, idle_band, idle_sources) = world_with_a_queue(ONE_SOURCE, NOBODY_BUILDING);
    stock_the_band(&mut idle_app, idle_band);
    {
        let mut allocation = idle_app
            .world
            .get_mut::<LaborAllocation>(idle_band)
            .expect("the band keeps its allocation");
        allocation.build_queue.clear();
    }
    assert_eq!(
        meter(&idle_app, idle_sources[0]),
        0.0,
        "fixture: there is nothing banked on this ground for the pool to hold"
    );
    let idle_fresh = tool_life(&idle_app, idle_band);
    for _ in 0..A_FEW_TURNS {
        resolve_a_turn(&mut idle_app);
    }
    assert_eq!(
        tool_life(&idle_app, idle_band),
        idle_fresh,
        "a pool with nothing to keep supplies nothing and must wear nothing — this is a use \
         count, not a turn clock"
    );
}

// ---------------------------------------------------------------------------------------------
// (13) A DECLARATION WITHDRAWN THE SAME TURN NEVER REACHES THE QUEUE
// ---------------------------------------------------------------------------------------------

/// **DECLARE AND WITHDRAW INSIDE ONE TURN, AND NOTHING SURVIVES THE BOUNDARY.**
///
/// [`unqueue_withdraws_the_declaration_and_the_source_stops_publishing_a_job`] withdraws a
/// **confirmed** entry — one that has already been through a turn and has work on its meter. This is
/// its sibling for the case reported from play on 2026-08-22: *"clicking `✕` on `Cultivate (61, 29)`
/// did not remove the row"*, on a declaration made that same turn.
///
/// **The two halves are different code paths.** A confirmed entry leaves through the turn's own
/// publishing (the position stops being stamped); a same-turn one has never been stamped at all, so
/// what has to be true is that the turn finds no entry to fund, banks nothing, and publishes
/// [`NOT_IN_ANY_BUILD_QUEUE`] rather than the head's `0`. Pinning it is what makes the remaining
/// symptom provably the client's.
#[test]
fn a_declaration_withdrawn_the_same_turn_never_reaches_the_queue() {
    let (mut app, band, sources) = world_with_a_queue(ONE_SOURCE, BUILDERS);
    // The fixture's queue is a declaration made **this** turn: nothing has resolved yet.
    assert_eq!(
        meter(&app, sources[0]),
        0.0,
        "fixture: the declaration must be unresolved, or this measures a confirmed entry"
    );
    assert!(
        app.world
            .get_mut::<LaborAllocation>(band)
            .expect("the band keeps its allocation")
            .unqueue_build(&BuildSource::Patch(sources[0])),
        "the entry was there to withdraw"
    );

    resolve_a_turn(&mut app);

    let allocation = app
        .world
        .get::<LaborAllocation>(band)
        .expect("the band keeps its allocation");
    assert!(
        allocation.build_queue.is_empty(),
        "the withdrawn declaration must not come back across the turn boundary"
    );
    assert_eq!(
        meter(&app, sources[0]),
        0.0,
        "…and the pool must have banked nothing on it: there was no entry to fund"
    );
    assert_eq!(
        published_position(&app, sources[0]),
        NOT_IN_ANY_BUILD_QUEUE,
        "…and the source publishes that it is in nobody's queue, never the head's 0"
    );
}

// ---------------------------------------------------------------------------------------------
// (14) THE KIT IS A PROPERTY OF THE ENTRY
// ---------------------------------------------------------------------------------------------

/// **The plant web's own builders kit** — what the roster derives for a Cultivate, and what an entry
/// that names nothing is raised with.
fn plant_build_kit() -> core_sim::KitChoice {
    core_sim::EquipmentConfig::builtin()
        .kit("tillage")
        .expect("the shipped roster carries the plant builders kit")
}

/// **THE WIRE STATES THE KIT A SITE IS ACTUALLY KEPT WITH, AND WHETHER A BAND NAMED IT**
/// (`docs/plan_standing_upkeep.md` §2.7).
///
/// # THE PAIR IS ONE READING, AND NEITHER HALF ANSWERS FOR THE OTHER
///
/// **The id is RESOLVED**, never *"the player named none"* — `buildKitId`'s standing rule, and it
/// matters here for its reason one account over: the keeping default is derived from the site's own
/// food web, so a row that named nothing would publish `""` while its keepers were out with hoes.
///
/// **And `named` is not recoverable from the id**, because a player may name the very kit the
/// derivation would have picked. Without it a picker cannot draw its `(default)` mark or offer *back
/// to default* — which is the whole reason the flag rides the wire instead of being re-derived.
///
/// Asserted on the **encoded** row rather than on the in-process state, because what a client reads
/// is the FlatBuffer.
#[test]
fn the_published_upkeep_kit_is_the_one_the_site_resolves_to_and_says_whether_it_was_named() {
    let (mut app, band, sources) = world_with_a_queue(2, BUILDERS);
    let published_kit = |app: &App, source: UVec2| -> (String, bool) {
        published(app, source, |patch| {
            (
                patch.upkeepKitId().unwrap_or_default().to_string(),
                patch.upkeepKitNamed(),
            )
        })
    };
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("the band keeps its allocation");
        // The first site names nothing — its web's derivation answers for it.
        // …and the second names the bare kit, which is a selection and not an absence.
        assert!(allocation.set_upkeep_kit(
            &LaborTarget::Forage {
                tile: sources[1],
                floor: FOOD_PEAK,
                species: None,
                take_species: TakeSelection::EVERYTHING,
            },
            Some(bare_builders()),
        ));
    }
    resolve_a_turn(&mut app);

    assert_eq!(
        published_kit(&app, sources[0]),
        (plant_build_kit().id().to_string(), false),
        "a site naming nothing publishes the kit its own web wants, marked as the DERIVATION — a \
         row publishing the empty string would say 'no kit' while the keepers were out with hoes"
    );
    assert_eq!(
        published_kit(&app, sources[1]),
        (bare_builders().id().to_string(), true),
        "…and an explicit bare-handed pick crosses as the roster's own bare id, marked as a NAMED \
         override: the two are different statements and the flag is what keeps them apart"
    );
}

/// **RE-DECLARING KEEPS THE ENTRY'S KIT**, exactly as it keeps its place in the line.
///
/// A second verb on an already-queued source replaces `declared` **in place**
/// (`build_order`'s *"re-declaring on a queued source does not cost the player their position"*), and
/// the kit is the same kind of player decision: correcting *what* is being raised must not silently
/// throw away the tool chosen for it.
#[test]
fn re_declaring_a_queued_source_keeps_the_kit_its_entry_carries() {
    let (mut app, band, sources) = world_with_a_queue(ONE_SOURCE, BUILDERS);
    let source = BuildSource::Patch(sources[0]);
    let mut allocation = app
        .world
        .get_mut::<LaborAllocation>(band)
        .expect("the band keeps its allocation");
    assert!(allocation.set_build_entry_kit(&source, Some(plant_build_kit())));
    assert!(
        allocation.enqueue_build(source.clone(), BuildJob::Rung(Improvement::Sow)),
        "re-declaring on a source the band works is accepted"
    );
    let entry = allocation
        .build_queue_entry(&source)
        .expect("the entry is still in the queue");
    assert_eq!(
        entry.declared,
        BuildJob::Rung(Improvement::Sow),
        "fixture: the re-declaration must have landed, or the kit claim is vacuous"
    );
    assert_eq!(
        entry.kit.as_ref().map(core_sim::KitChoice::id),
        Some(plant_build_kit().id()),
        "re-declaring is a correction to WHAT is being raised — it must not clear the tool the \
         player chose for it, any more than it costs them their place in the line"
    );
}

/// ⛔ **TWO ENTRIES, TWO KITS, AND EACH IS PRICED AND DATED AT ITS OWN.**
///
/// The head is the only entry the pool funds, but **every** entry is dated — and now that entries can
/// carry different kits, a waiting entry must be dated at the gear *it* will be raised with rather
/// than at its web's derived answer. That is the whole point of a per-entry override: a queue whose
/// second job is deliberately bare-handed has to say so in its date.
///
/// **Both arms are on the plant web**, so the branch is held constant and the only thing that moves
/// is the entry's own kit — a cross-web pair would confound the two.
#[test]
fn two_entries_on_one_band_are_each_priced_and_dated_at_their_own_kit() {
    /// Both entries geared, or the tail one deliberately bare — the one dial under test.
    fn queue_with_a_bare_tail(bare_tail: bool) -> (App, Vec<UVec2>) {
        let (mut app, band, sources) = world_with_a_queue(2, BUILDERS);
        {
            let mut allocation = app
                .world
                .get_mut::<LaborAllocation>(band)
                .expect("the band keeps its allocation");
            assert!(allocation
                .set_build_entry_kit(&BuildSource::Patch(sources[0]), Some(plant_build_kit())));
            assert!(allocation.set_build_entry_kit(
                &BuildSource::Patch(sources[1]),
                Some(if bare_tail {
                    bare_builders()
                } else {
                    plant_build_kit()
                }),
            ));
        }
        resolve_a_turn(&mut app);
        (app, sources)
    }

    let (bare_tail, sources) = queue_with_a_bare_tail(true);
    let head_gear = published(&bare_tail, sources[0], |patch| patch.buildWorkFromGear());
    let tail_gear = published(&bare_tail, sources[1], |patch| patch.buildWorkFromGear());
    assert!(
        head_gear > 0.0,
        "fixture: the geared head must actually be geared, or both claims below are vacuous — \
         got {head_gear}"
    );
    assert_eq!(
        tail_gear, 0.0,
        "a deliberately bare-handed entry is priced at ITS OWN kit, not at the kit the entry above \
         it happens to carry — got {tail_gear}"
    );

    // **And the DATE moves with it**: the same queue with a geared tail finishes sooner.
    let (geared_tail, geared_sources) = queue_with_a_bare_tail(false);
    let bare_date = published_turns(&bare_tail, sources[1]);
    let geared_date = published_turns(&geared_tail, geared_sources[1]);
    assert!(
        bare_date > geared_date,
        "a waiting entry is dated at the gear IT will be raised with: bare tail {bare_date} must \
         be later than geared tail {geared_date}"
    );
}

/// **`none` IS A REAL SELECTION AND DOES NOT COLLAPSE TO "derive".**
///
/// The two are different statements — *"send this job's builders out bare-handed to conserve gear"*
/// versus *"whatever this entry's web wants"* — and an `Option` that lost the distinction would make
/// the bare-handed choice unexpressible, which is exactly the defect the deleted per-band picker had
/// in the other direction.
#[test]
fn a_bare_kit_on_an_entry_survives_as_bare_handed_rather_than_collapsing_to_derive() {
    let (mut app, band, sources) = world_with_a_queue(ONE_SOURCE, BUILDERS);
    let source = BuildSource::Patch(sources[0]);
    assert!(app
        .world
        .get_mut::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .set_build_entry_kit(&source, Some(bare_builders())));
    resolve_a_turn(&mut app);
    assert_eq!(
        published(&app, sources[0], |patch| patch.buildWorkFromGear()),
        0.0,
        "an explicit bare kit must be honoured: collapsing it to the derivation would send the \
         pool out with the hoes the player just declined"
    );

    // **Liveness — clearing it back to `None` really does reach the derivation.**
    assert!(app
        .world
        .get_mut::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .set_build_entry_kit(&source, None));
    resolve_a_turn(&mut app);
    assert!(
        published(&app, sources[0], |patch| patch.buildWorkFromGear()) > 0.0,
        "clearing the override returns the entry to its own web's kit — without this the zero \
         above is also what a dead gear term reports"
    );
}

/// **THE WIRE STATES THE RESOLVED KIT, NEVER "the player named none"** — and `""` means *"nobody has
/// this queued"*, which is a different statement from the roster's own bare kit.
#[test]
fn the_published_build_kit_is_the_one_the_entry_resolves_to() {
    let (mut app, band, sources) = world_with_a_queue(2, BUILDERS);
    let published_kit = |app: &App, source: UVec2| -> String {
        published(app, source, |patch| {
            patch.buildKitId().unwrap_or_default().to_string()
        })
    };
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("the band keeps its allocation");
        // The head names nothing — the derivation answers for it.
        assert!(allocation.set_build_entry_kit(&BuildSource::Patch(sources[0]), None));
        // …and the tail names the bare kit, which is a selection and not an absence.
        assert!(
            allocation.set_build_entry_kit(&BuildSource::Patch(sources[1]), Some(bare_builders()))
        );
        // Nothing is queued on the third source at all.
        allocation.build_queue.retain(|entry| {
            entry.source == BuildSource::Patch(sources[0])
                || entry.source == BuildSource::Patch(sources[1])
        });
    }
    resolve_a_turn(&mut app);

    assert_eq!(
        published_kit(&app, sources[0]),
        plant_build_kit().id(),
        "an entry naming nothing publishes the kit its own web wants — a row that published the \
         empty string would say 'no kit' while the pool was out with hoes"
    );
    assert_eq!(
        published_kit(&app, sources[1]),
        bare_builders().id(),
        "…and an explicit bare kit publishes the roster's own bare id, which reads differently \
         from the empty string below"
    );

    // **An unqueued source says so with the empty string.** Withdraw one and re-publish.
    assert!(app
        .world
        .get_mut::<LaborAllocation>(band)
        .expect("the band keeps its allocation")
        .unqueue_build(&BuildSource::Patch(sources[1])));
    resolve_a_turn(&mut app);
    assert_eq!(
        published_kit(&app, sources[1]),
        "",
        "a source in nobody's queue is being raised with nothing at all"
    );
}

// ---------------------------------------------------------------------------------------------
// (12) A road entry is held by its KEEPER, and losing the keeper retires it
// ---------------------------------------------------------------------------------------------

/// **How long the fixture waits for disuse to take an unwalked road back.** Comfortably more than
/// the shipped grace plus the turns the loss rate needs to clear the free floor's top, so what fails
/// on a stuck fixture is this bound rather than the loop running for ever.
const TURNS_BEFORE_DISUSE_MUST_HAVE_BITTEN: u32 = 200;

/// **Where the fixture road is seated** — the top of the free floor, which is where a `grade` that
/// has banked nothing stands. Read off the ladder rather than written, so a retune moves it.
fn trail_top(app: &App) -> f32 {
    let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
    core_sim::traffic_ceiling(&ladder)
}

/// Seat a road on `tile` at the top of the free floor and make the fixture band its keeper — what
/// `grade` leaves behind on the turn it is typed, with no work banked yet.
fn grade_a_road(app: &mut App, tile: UVec2) {
    let position = trail_top(app);
    let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
    let mut roads = app.world.resource_mut::<core_sim::RoadRegistry>();
    let road = roads.road_or_trail(tile, &ladder);
    // The position first: `set_position` releases a keeper on a road inside the free floor, so a
    // keeper written before it would be wiped by the seating itself.
    road.set_position(position, &ladder);
    road.take_keeper(
        core_sim::RoadKeeper {
            faction: FactionId(0),
            band: core_sim::BandId(FIXTURE_BAND),
        },
        core_sim::NEAR_ENOUGH_TO_KEEP,
        &ladder,
    );
}

/// Does the fixture band still keep the road on `tile`?
fn road_has_keeper(app: &App, tile: UVec2) -> bool {
    app.world
        .resource::<core_sim::RoadRegistry>()
        .road(tile)
        .and_then(|road| road.keeper)
        .is_some_and(|keeper| keeper.band == core_sim::BandId(FIXTURE_BAND))
}

/// One turn including the **route** pass, which is what wears a road in and what takes it back.
fn resolve_a_turn_with_roads(app: &mut App) {
    app.world.run_system_once(core_sim::advance_roads);
    resolve_a_turn(app);
}

/// ⛔ **A ROAD ENTRY IS RETIRED THE TURN THE BAND STOPS BEING THE KEEPER — AND THE POOL BEHIND IT IS
/// RELEASED.**
///
/// A road is the one build source with **no labor row**: its membership is `Road::keeper`, so the
/// queue's *"an entry requires a holding"* prune has to read the road. It used to answer *held* for
/// every road unconditionally, and every exit was then closed at once: `advance_roads` releases the
/// keeper when disuse drops the road below `traffic_ceiling`, after which the road arm banks nothing
/// (it is not the keeper), `retire_entries_already_built` does not fire (the tile does not hold the
/// destination rung) and `abandon` finds no keeper to release. The entry stayed at the **head** for
/// ever.
///
/// **The second assertion is the one that hurts.** All hands go on the head, so a permanently
/// stranded head funds *nothing else the band has queued* — silently, with only the undocumented
/// `unqueue` to recover it. A test asserting only *"the entry is gone"* would pass on a fix that
/// left the pool starved, so the patch behind it is measured before and after.
#[test]
fn a_road_entry_dies_with_its_keeper_and_frees_the_pool_behind_it() {
    let (mut app, band, sources) = world_with_a_queue(ONE_SOURCE, BUILDERS);
    let patch = sources[0];
    let road_tile = patch;
    grade_a_road(&mut app, road_tile);
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("the band keeps its allocation");
        // The road goes to the HEAD, which is where a fresh `grade` puts the pool.
        allocation.build_queue.insert(
            0,
            core_sim::BuildQueueEntry {
                source: BuildSource::Road(road_tile),
                declared: BuildJob::Rung(Improvement::Grade),
                kit: Some(bare_builders()),
            },
        );
    }

    // **The liveness half: while the band keeps the road the head is the road, and the patch waits.**
    resolve_a_turn_with_roads(&mut app);
    assert!(
        road_has_keeper(&app, road_tile),
        "the graded road is still this band's job on the turn after the command"
    );
    assert_eq!(
        queued_sources(&app, band),
        vec![BuildSource::Road(road_tile), BuildSource::Patch(patch)],
        "the road is the head and the patch is queued behind it"
    );
    let waiting = meter(&app, patch);
    resolve_a_turn_with_roads(&mut app);
    assert_eq!(
        meter(&app, patch),
        waiting,
        "all hands are on the head, so the patch behind it banks nothing — the state the stranded \
         entry made permanent"
    );

    // **Now let the road go.** Nothing walks it, so after the ladder's own disuse grace it bleeds a
    // little every turn, drops below the free floor's top, and `set_position` releases the keeper —
    // the reachable path, driven rather than reached by hand.
    let mut turns = 0;
    while road_has_keeper(&app, road_tile) {
        resolve_a_turn_with_roads(&mut app);
        turns += 1;
        assert!(
            turns < TURNS_BEFORE_DISUSE_MUST_HAVE_BITTEN,
            "disuse takes an unwalked road back — {turns} turns without it means the fixture, not \
             the rule, has stopped working"
        );
    }

    assert_eq!(
        queued_sources(&app, band),
        vec![BuildSource::Patch(patch)],
        "the road entry is retired the turn the band stops being the keeper — the job is not theirs"
    );
    let before = meter(&app, patch);
    resolve_a_turn_with_roads(&mut app);
    assert!(
        meter(&app, patch) > before,
        "…and the whole builders pool moves onto what the band still holds, which is the failure \
         the stranded entry actually caused"
    );
}
