//! **A resident band's exported local-hunt yield preview equals what `hunt_take` really pays it** —
//! pinned over a real captured snapshot, every floor, every staffing and three herd states.

use std::sync::Arc;

use bevy::math::UVec2;

use core_sim::{
    build_test_app, herd_hunt_yield, hunt_escapement_ceiling, hunt_source_yield_preview, hunt_take,
    recapture_snapshot_in_place, scalar_from_f32, CombatConfigHandle, FaunaConfig,
    FaunaConfigHandle, HerdRegistry, HuntDraw, LaborConfigHandle, PopulationCohort, Scalar,
    SimulationConfig, SnapshotHistory, VisibilityLedger,
};

use bevy::app::App;

/// The reference party: 4 hunters.
const PARTY_WORKERS: u32 = 4;

/// The floors these sweeps walk — the four the retired stance axis named, plus the ends of the dial
/// it could not express (`0.8` deliberate under-harvest, `1.0` take nothing).
const SWEPT_FLOORS: [f32; 6] = [0.0, 0.15, 0.3, 0.5, 0.8, 1.0];

/// **The shipped EQUIPPED haul rate** — what a kitted band drags, off the sled's own tier.
/// `labor_config`'s `hunt.per_worker_biomass_capacity` is the *bare-handed* baseline since quality
/// tiers landed, so a fixture that wants "an ordinary band" asks the item table.
fn equipped_haul_rate() -> f32 {
    core_sim::EquipmentConfig::builtin().equipped_reference(
        core_sim::EquipmentStat::HuntCarry,
        core_sim::LaborConfig::builtin()
            .hunt
            .per_worker_biomass_capacity,
    )
}

/// Mark the named herds' tiles visible to the viewer faction.
///
/// Herd display telemetry is **fog-filtered** (issue #264) — a herd on ground the viewer cannot see
/// is not published at all. The tests below pick herds off the registry by index rather than by
/// where the starting band happens to stand, so most of them are in the dark; they are about *what a
/// visible herd's exported readout says*, not about whether it is visible. Revealing the herd is the
/// in-game precondition for reading that panel at all (a band works or scouts within sight of it),
/// so the fixture states it explicitly rather than blanketing the map.
fn reveal_herds(app: &mut App, ids: &[String]) {
    let positions: Vec<UVec2> = {
        let registry = app.world.resource::<HerdRegistry>();
        ids.iter()
            .filter_map(|id| registry.find(id).map(|herd| herd.position()))
            .collect()
    };
    let grid = app.world.resource::<SimulationConfig>().grid_size;
    let viewer = app.world.resource::<core_sim::ViewerFaction>().0;
    let mut ledger = app.world.resource_mut::<VisibilityLedger>();
    let map = ledger.ensure_faction(viewer, grid.x, grid.y);
    for pos in positions {
        map.mark_active(pos.x, pos.y, 0);
    }
}

/// Seed a herd's biomass as a fraction of its carrying capacity; returns `(position, biomass, cap)`.
fn seed_herd(app: &mut App, id: &str, cap_fraction: f32) -> (UVec2, f32, f32) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry
        .herds
        .iter_mut()
        .find(|h| h.id == id)
        .expect("herd present");
    herd.biomass = herd.carrying_capacity * cap_fraction;
    (herd.position(), herd.biomass, herd.carrying_capacity)
}

/// [`build_headless_app`] with the whole roster's `combat.wariness` held at `0` — the world-driven
/// twin of [`deterministic_fauna`], for the fixtures that run a real turn instead of calling a pure
/// helper. See [`FaunaConfig::without_retreat`].
fn deterministic_headless_app() -> App {
    let mut app = build_test_app();
    app.world
        .resource_mut::<FaunaConfigHandle>()
        .hold_wariness_at_zero();
    app
}

/// **The party every pure-forecast fixture below fights with** — the shipped, fully-kitted hunter
/// (`docs/plan_hunt_through_combat.md` §4.8's spear tier). The take resolves through the fight now,
/// so a raid forecast is quoted for a *party*, and these fixtures mean "an ordinary outfitted one".
fn hunting_party() -> core_sim::HuntingParty {
    core_sim::HuntingParty::builtin_equipped()
}

/// Both sides run the same linear formula but land on the sim's fixed-point grid at *different* points,
/// so the band-hunt guards allow a few `Scalar` quanta of rounding.
const TAKE_ABS_EPSILON: f32 = 4.0 / Scalar::SCALE as f32;
/// …plus f32 slop proportional to the magnitude (a big-game take runs to hundreds of provisions).
const TAKE_REL_EPSILON: f32 = 1e-5;

/// Assert a snapshot-derived preview matches the provisions the sim's real take produced.
fn assert_provisions_eq(preview: f32, real_take: f32, context: &str) {
    let tolerance = TAKE_ABS_EPSILON + real_take.abs() * TAKE_REL_EPSILON;
    assert!(
        (preview - real_take).abs() <= tolerance,
        "{context}: snapshot preview {preview} != real take {real_take}"
    );
}

/// Worker counts the band-hunt guard sweeps: an unstaffed assignment (both sides must read 0), a
/// lone hunter, the reference party, and a crew big enough that its throughput overshoots a herd's
/// policy ceiling — so **both** branches of the `min(worker_cap, ceiling)` are exercised.
const BAND_HUNT_WORKER_COUNTS: [u32; 4] = [0, 1, PARTY_WORKERS, 60];

/// Discontent seeded on the band for the second pass, so its exported `outputMultiplier` is
/// genuinely `!= 1.0` (with the shipped wellbeing levers — `discontent_weight` 1.0, `floor_mult`
/// 0.5 — this lands at 0.6). Without it the multiplier would be the identity and the guard would
/// pass even if the client's `× outputMultiplier` term were dropped.
const BAND_DISCONTENT_FRACTION: f32 = 0.4;

/// Biomass (as a fraction of carrying capacity) of the depleted-but-viable herd: above the Allee
/// threshold (`collapse_fraction` = 0.15 → a *positive* Sustain/Surplus ceiling), but low enough
/// that under `CLAMP_BINDING_REGROWTH_RATE` the policy ceiling overshoots what is actually left, so
/// the biomass clamp binds.
const DEPLETED_CAP_FRACTION: f32 = 0.2;

/// Regrowth rate for the clamp-binding pass. The **shipped** `ecology.regrowth_rate` (0.05) is far
/// too gentle for any policy ceiling to exceed a herd's remaining biomass (MSY ≤ 0.05 × biomass,
/// Surplus ≤ 0.08 × biomass), so the biomass clamp is inert under today's levers — but it is a
/// *config lever*, and a designer raising it must not silently break the client's preview. At 2.0
/// the Surplus/Sustain ceiling on a
/// `DEPLETED_CAP_FRACTION` herd is ~1.6×/~0.3× its biomass, so the exported ceiling's biomass clamp
/// (and `hunt_take`'s) genuinely binds and the two must still agree.
const CLAMP_BINDING_REGROWTH_RATE: f32 = 2.0;

/// Seed every cohort's discontent, so the exported `outputMultiplier` is a known non-identity value.
fn set_discontent(app: &mut App, fraction: f32) {
    let mut cohorts = app.world.query::<&mut PopulationCohort>();
    for mut cohort in cohorts.iter_mut(&mut app.world) {
        cohort.discontent_fraction = scalar_from_f32(fraction);
    }
}

/// Swap in a fauna config with a tweaked ecology regrowth rate (test-local tuning — the species
/// table and every other lever stay as shipped).
fn set_fauna_regrowth_rate(app: &mut App, regrowth_rate: f32) {
    let mut fauna = FaunaConfig::clone(&app.world.resource::<FaunaConfigHandle>().get());
    fauna.ecology.regrowth_rate = regrowth_rate;
    app.world
        .insert_resource(FaunaConfigHandle::new(Arc::new(fauna)));
}

/// Pin the **exported local-hunt yield preview** to the provisions `hunt_take` really pays a resident
/// band, over every worker count × every policy × each of `herd_ids`.
///
/// **RETARGETED IN SLICE 8 — the preview is an exported ANSWER now, not client arithmetic.** This used
/// to replay the client's own formula, `min(workers × huntPerWorkerProvisions, ceiling) ×
/// outputMultiplier`, which was exact because every term was linear and factored out of the `min`.
/// A whole-animal take runs through `floor()`, and **`floor` does not factor out of anything**: no
/// combination of a per-worker rate and a ceiling lets the client re-derive "3 boars, one of them only
/// half carried". So the sim exports the number (`fauna::hunt_source_yield_preview` →
/// `SourceYield.actual`, the same seam that seeds the assign-time telemetry) and this asserts THAT
/// equals the take.
///
/// The guard is **stronger, not weaker**: it still pins a client-visible preview to the sim's real
/// take across the same sweep, and it now pins the *actual* thing the client renders instead of a
/// formula the client is no longer allowed to use. The exported per-policy `ceiling` rows are still
/// checked to exist and to exclude the forage-only verbs — they remain the honest "what will this herd
/// give up at all" readout, they are simply no longer a *staffing* formula's input.
fn assert_band_preview_matches_hunt_take(app: &mut App, herd_ids: &[String], case: &str) {
    reveal_herds(app, herd_ids);
    recapture_snapshot_in_place(&mut app.world);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let labor = app.world.resource::<LaborConfigHandle>().get();

    let cohort = snapshot
        .populations
        .first()
        .expect("the campaign spawns at least one band");
    // The band applies its morale/discontent productivity modifier at payout — the client reads the
    // already-exported multiplier rather than recomputing the wellbeing stack.
    let output_multiplier = Scalar::from_raw(cohort.output_multiplier).to_f32();

    for id in herd_ids {
        let exported = snapshot
            .herds
            .iter()
            .find(|h| &h.id == id)
            .unwrap_or_else(|| panic!("{case}: herd {id} is in the snapshot"));

        // **THE CLIENT COMPOSES THE CEILING FROM THE PER-BIOMASS VECTOR** — the four ceiling rows
        // are retired, because four rows cannot answer a continuous dial
        // (`docs/plan_harvest_floor.md` §5). The exported rate must be the species' own, so the
        // curve the client draws is the sim's arithmetic and not an approximation of it.
        {
            let live_yield = {
                let registry = app.world.resource::<HerdRegistry>();
                herd_hunt_yield(registry.find(id).expect("herd present"), &fauna)
            };
            assert!(
                (exported.provisions_per_biomass - live_yield.provisions_per_biomass).abs() < 1e-6,
                "{case}: {id}: the exported per-biomass rate must be the species' own"
            );
        }
        // Every floor a Hunt assignment accepts.
        for policy in SWEPT_FLOORS {
            // A composed ceiling must never promise more than the source is standing there
            // holding. **Inherent** rather than a clamp someone has to remember: it is `B − floor·K`
            // with `floor >= 0`. Composed and bounded against the SAME snapshot's numbers — the live
            // herd moves under the sweep as each staffing takes from it, so mixing the two would
            // compare two different turns.
            let ceiling = (exported.biomass - policy * exported.carrying_capacity).max(0.0)
                * exported.provisions_per_biomass;
            assert!(
                ceiling <= exported.biomass * exported.provisions_per_biomass + TAKE_ABS_EPSILON,
                "{case}: {id} floor {policy}: composed ceiling {ceiling} exceeds the biomass the \
                 same snapshot published"
            );

            for workers in BAND_HUNT_WORKER_COUNTS {
                // What the client renders: the sim's own exported preview for this staffing.
                let preview = {
                    let registry = app.world.resource::<HerdRegistry>();
                    let herd = registry.find(id).expect("herd present");
                    hunt_source_yield_preview(
                        herd,
                        &fauna,
                        equipped_haul_rate(),
                        &hunting_party(),
                        output_multiplier,
                        workers,
                        core_sim::NO_HANDS,
                        policy,
                        labor.yield_average_horizon_turns,
                        labor.arrivals_horizon_turns,
                        app.world
                            .resource::<CombatConfigHandle>()
                            .get()
                            .forecast_range_sigmas,
                    )
                    .actual
                };

                // The sim's real band take (a resident band has no carry limit — it eats/banks the
                // whole take, so `carry_room_biomass = INFINITY`, exactly as the Hunt labor arm
                // passes). Cloned so each sweep entry sees the same pre-take state — and **regrown
                // once**, because that is the turn the preview beside it is about: a pre-commit
                // forecast prices the herd as the next take will find it (`next_turns_quarry`), and
                // a real turn runs Logistics before Population. Taking from the un-regrown herd
                // would compare two turns and call the growth a drift.
                let mut herd = {
                    let registry = app.world.resource::<HerdRegistry>();
                    core_sim::next_turns_quarry(registry.find(id).expect("herd present"), &fauna)
                };
                let take = hunt_take(
                    &mut herd,
                    workers as f32,
                    policy,
                    equipped_haul_rate(),
                    &hunting_party(),
                    &fauna,
                    f32::INFINITY,
                    // The preview pins `forecast == actual`, so the retreat draw is held fixed —
                    // every species here ships `wariness 0`, making it an identity anyway.
                    HuntDraw::Seeded(0),
                )
                .take;
                let sim_rate = herd_hunt_yield(&herd, &fauna)
                    .apply(take.carried, output_multiplier)
                    .provisions;

                assert_provisions_eq(
                    preview,
                    sim_rate,
                    &format!("{case}: {id} {policy:?} ×{workers} (mult {output_multiplier})"),
                );
            }
        }
    }
}

/// **THE BAND-TAKE ANTI-DRIFT GUARD**. The client previews a
/// resident band's per-turn hunt yield from the snapshot alone:
///
/// ```text
/// rate = min(workers × huntPerWorkerProvisions, ceiling_for(policy)) × outputMultiplier
/// ```
///
/// which is arithmetically `hunt_take(.., carry_room_biomass = INFINITY)` — the biomass→provisions
/// conversion and the productivity multiplier are both linear, so they factor out of the `min`, and
/// the exported ceiling is **biomass-clamped** exactly as the take is. This test replays that
/// arithmetic over a **real captured snapshot** and asserts it equals the provisions `hunt_take`
/// actually hands the band, across every party size × all four policies × a healthy herd, a
/// **depleted herd where the biomass clamp binds**, and a collapsing (sub-Allee) herd — under both
/// a unit and a discontent-reduced output multiplier. If the two ever diverge, the client's
/// local-hunt preview is lying, and this test fails.
#[test]
fn exported_snapshot_fields_reproduce_band_hunt_take() {
    let mut app = deterministic_headless_app();
    app.update();

    let collapse_fraction = app
        .world
        .resource::<FaunaConfigHandle>()
        .get()
        .ecology
        .collapse_fraction;

    let (healthy, depleted, collapsing) = {
        let registry = app.world.resource::<HerdRegistry>();
        let mut ids = registry.herds.iter().map(|h| h.id.clone());
        (
            ids.next().expect("map seeds at least three herds"),
            ids.next().expect("map seeds at least three herds"),
            ids.next().expect("map seeds at least three herds"),
        )
    };
    seed_herd(&mut app, &healthy, 0.9);
    let (_, depleted_biomass, depleted_cap) = seed_herd(&mut app, &depleted, DEPLETED_CAP_FRACTION);
    // Sub-Allee: Sustain/Surplus yield nothing there, so both sides must agree on a 0 take.
    seed_herd(&mut app, &collapsing, collapse_fraction * 0.5);
    let herds = [healthy, depleted, collapsing];

    // Pass 1: the shipped ecology levers, unit output multiplier (a content band).
    assert_band_preview_matches_hunt_take(&mut app, &herds, "shipped ecology, content band");

    // Pass 2: a discontented band — the exported `outputMultiplier` is now genuinely != 1.0.
    set_discontent(&mut app, BAND_DISCONTENT_FRACTION);
    assert_band_preview_matches_hunt_take(&mut app, &herds, "shipped ecology, discontented band");

    // Pass 3: **the case that used to need a biomass clamp, kept as the proof nothing can make it
    // fire.** `CLAMP_BINDING_REGROWTH_RATE` is an extreme (hot-reloadable) `r` under which the OLD
    // **flow** ceilings — `MSY` (Sustain) and `1.6 × MSY` (Surplus) — computed a take *larger than the
    // herd was standing there holding*, so the exported ceiling had to be explicitly clamped or the
    // preview over-stated it.
    //
    // The harvest floor makes that unreachable **by construction**, and by the strongest available
    // argument: every stance's ceiling is now `max(0, B − floor·K) × dip`, which is `≤ B` for any
    // floor `≥ 0` and any dip `≤ 1` — and it **cannot read `r` at all**, the growth rate having been
    // removed from the take path's signature.
    //
    // The pass is kept (retargeted from "the clamp fires" to "nothing can make it need to fire"): it
    // still sweeps the whole preview==take matrix at an off-nominal lever, and it now pins the
    // stronger property. `assert_band_preview_matches_hunt_take` asserts the bound on every row.
    set_fauna_regrowth_rate(&mut app, CLAMP_BINDING_REGROWTH_RATE);
    {
        for policy in SWEPT_FLOORS {
            assert!(
                hunt_escapement_ceiling(policy, depleted_biomass, depleted_cap)
                    <= depleted_biomass,
                "{policy:?}: the escapement ceiling can never exceed the herd's own biomass, at any \
                 regrowth rate"
            );
        }
    }
    assert_band_preview_matches_hunt_take(&mut app, &herds, "clamp-binding ecology");
}
