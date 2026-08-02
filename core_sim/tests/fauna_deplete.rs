//! Deplete hunting: `0.15` takes `deplete_multiplier × MSY` (2.5×), the
//! harshest of the four **ascending multiples of MSY** (Sustain ≤ 1× < Surplus 1.5× < Deplete 2.5× <
//! Eradicate = everything) — constant catch this far above MSY has no equilibrium, so it drives a herd
//! extinct. Also home to the axis's ordering invariant
//! (`hunt_policy_takes_are_strictly_ordered_at_every_biomass`). Uses the source-centric labor
//! allocation (a Hunt assignment) that replaced the retired persistent follow.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::MinimalPlugins;

use core_sim::hunt_escapement_ceiling;
use core_sim::{
    advance_herds, advance_husbandry, advance_labor_allocation, scalar_from_f32, scalar_one,
    scalar_zero, spawn_initial_herds, spawn_initial_world, CommandEventLog, CultureManager,
    DiscoveryProgressLedger, EcologyPhase, FactionId, FactionInventory, FaunaConfigHandle,
    ForageRegistry, GenerationId, GenerationRegistry, HerdDensityMap, HerdRegistry, HerdTelemetry,
    LaborAllocation, LaborAssignment, LaborConfigHandle, LaborTarget, LadderConfigHandle,
    LocalStore, MapPresets, MapPresetsHandle, MoraleCause, PopulationCohort, SimulationConfig,
    SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle, StartLocation,
    StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, StartingUnit, TileRegistry,
    WellbeingConfigHandle, TRADE_GOODS,
};

/// Whole-worker head-count assigned to the hunt — large enough that the per-worker biomass cap
/// never binds, so the take is set entirely by the policy ceiling.
const HUNT_WORKERS: u32 = 5000;

fn spawn_world() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = 119304647;
    app.world.insert_resource(config);

    app.world
        .insert_resource(MapPresetsHandle::new(MapPresets::builtin()));
    app.world
        .insert_resource(GenerationRegistry::with_seed(42, 8));
    app.world.insert_resource(SimulationTick::default());
    app.world.insert_resource(CultureManager::new());
    app.world.insert_resource(StartLocation::default());
    app.world
        .insert_resource(DiscoveryProgressLedger::default());
    app.world.insert_resource(FactionInventory::default());
    app.world
        .insert_resource(StartProfileKnowledgeTagsHandle::new(
            StartProfileKnowledgeTags::builtin(),
        ));
    app.world.insert_resource(SnapshotOverlaysConfigHandle::new(
        SnapshotOverlaysConfig::builtin(),
    ));

    app.add_systems(bevy::app::Startup, spawn_initial_world);
    app.update();

    app.world.insert_resource(HerdRegistry::default());
    app.world.insert_resource(ForageRegistry::default());
    app.world.insert_resource(HerdTelemetry::default());
    app.world.insert_resource(HerdDensityMap::default());
    app.world.insert_resource(FaunaConfigHandle::default());
    app.world.insert_resource(LaborConfigHandle::default());
    app.world
        .insert_resource(core_sim::FloraConfigHandle::default());
    app.world.insert_resource(LadderConfigHandle::default());
    app.world.insert_resource(WellbeingConfigHandle::default());
    app.world
        .insert_resource(core_sim::CombatConfigHandle::default());
    app.world
        .insert_resource(core_sim::CreaturesConfigHandle::default());
    app.world.insert_resource(CommandEventLog::default());
    app.world.run_system_once(spawn_initial_herds);
    app
}

/// The **body mass both comparison herds are seated at** (intensification ladder slice 8). Deer-scale
/// (60) — the "slow breeder, deer/megafauna territory" these tests are about, and heavy enough that
/// the whole-animal quantiser is genuinely engaged rather than approximating a fluid.
///
/// **This is the fix for a FALSE GREEN, and it is the whole reason this constant exists.** The map
/// hands out the first two route-1 game herds, and they are **different species** — a Wild Fowl
/// (`body_mass` 1) and a Rabbit Warren (`body_mass` 2). While ruling 4 made Surplus and Deplete the
/// same take, `deplete_declines_faster_and_earns_more_trade_than_surplus` still passed — on **nothing
/// but the rounding slop between a 1-unit body and a 2-unit body** (600.54 vs 601.61, both pinned at
/// the identical `0.15·K` floor). It was measuring `body_mass`, not policy. Seating both herds at one
/// body mass means the **only** difference between the two rows is the policy, so the test fails when
/// the doctrine breaks and for no other reason.
const COMPARISON_BODY_MASS: f32 = 60.0;

/// Two distinct stationary game herds (route length 1) primed **identically** — same capacity, same
/// biomass, same [`COMPARISON_BODY_MASS`] — at a large half-capacity size (Thriving) for side-by-side
/// policy comparison. The size is inflated so the per-turn take is big enough that integer
/// trade/provisions yields don't quantize to zero.
///
/// **Identical in every respect the take reads** (the callers pin `regrowth_rate` on top, which is the
/// last one): the two herds must differ *only* by the policy under test. See
/// [`COMPARISON_BODY_MASS`] for the false green this closes.
fn prime_two_stationary_herds(app: &mut App) -> (String, String) {
    const CAP: f32 = 4000.0;
    let ids: Vec<String> = {
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .herds
            .iter()
            .filter(|h| h.id.starts_with("game_") && h.route_length() == 1)
            .map(|h| h.id.clone())
            .take(2)
            .collect()
    };
    assert!(ids.len() == 2, "need two stationary game herds");
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    for id in &ids {
        let herd = registry.herds.iter_mut().find(|h| &h.id == id).unwrap();
        herd.carrying_capacity = CAP;
        herd.biomass = CAP * 0.5;
        herd.body_mass = COMPARISON_BODY_MASS;
    }
    (ids[0].clone(), ids[1].clone())
}

fn spawn_hunter(
    app: &mut App,
    herd_id: &str,
    policy: f32,
    faction: FactionId,
) -> bevy::prelude::Entity {
    let pos = app
        .world
        .resource::<HerdRegistry>()
        .find(herd_id)
        .unwrap()
        .position();
    let tile = app
        .world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("herd tile resolves");
    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 30,
                children: scalar_zero(),
                working: scalar_from_f32(HUNT_WORKERS as f32),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
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
                faction,
                knowledge: Vec::new(),
                migration: None,
            },
            StartingUnit {
                kind: "BandHunter".to_string(),
                tags: Vec::new(),
            },
            LaborAllocation {
                assignments: vec![LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: herd_id.to_string(),
                        floor: policy,
                    },
                    workers: HUNT_WORKERS,
                    improvement: None,
                }],
                ..Default::default()
            },
        ))
        .id()
}

fn run_turns(app: &mut App, turns: u32) {
    for _ in 0..turns {
        app.world.run_system_once(advance_herds);
        app.world.run_system_once(advance_husbandry);
        app.world.run_system_once(advance_labor_allocation);
    }
}

fn biomass_ratio(app: &App, id: &str) -> Option<f32> {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .map(|h| h.biomass / h.carrying_capacity)
}

/// Trade goods sitting in ONE BAND's own store. Every ongoing harvest credits `TRADE_GOODS` on the
/// producing cohort's `LocalStore` (the `FOOD`/`FODDER` treatment) — `FactionInventory` now only ever
/// holds the start profile's opening grant, so reading it here would report `0` forever.
fn trade_goods(app: &App, band: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the hunting band still exists")
        .stores
        .get(TRADE_GOODS)
        .to_f32()
}

fn has_hunt_assignment(app: &App, band: bevy::prelude::Entity) -> bool {
    app.world
        .get::<LaborAllocation>(band)
        .map(|a| {
            a.assignments
                .iter()
                .any(|x| matches!(x.target, LaborTarget::Hunt { .. }))
        })
        .unwrap_or(false)
}

/// **The retired stance tokens are refused, not silently reinterpreted.** `FollowPolicy` is gone, so
/// there is no round-trip left to pin — what matters is that a stale client sending `deplete` where a
/// floor belongs is told the grammar moved. The guard is `sim_runtime`'s `reject_retired_stance`,
/// spelled out as literal strings so it outlives the type it names.
#[test]
fn a_retired_stance_token_is_refused_where_a_floor_belongs() {
    use sim_runtime::command_text::{parse_command_line, CommandParseError};
    for retired in ["sustain", "surplus", "deplete", "eradicate"] {
        assert!(
            matches!(
                parse_command_line(&format!("assign_labor 0 904 hunt game_deer_07 {retired} 4")),
                Err(CommandParseError::RetiredStanceToken(_))
            ),
            "'{retired}' must name the grammar that moved, not fail as a bad number"
        );
    }
}

/// **Deplete declines a herd faster than Surplus, both decline it while Sustain holds it steady — and
/// Deplete out-earns Surplus on trade** (slice 8b — the multiplier model).
///
/// Every extractive policy is now a constant catch that is a **multiple of MSY**: Surplus 1.5× and
/// Deplete 2.5× both exceed the herd's max regrowth (1× MSY), so both decline it — Deplete faster.
/// Sustain (≤ 1× MSY, escapement) holds a herd at `K/2`. Measured on the same species (so the take
/// difference is policy, not `body_mass`), pinned `r` for determinism.
#[test]
fn deplete_and_surplus_decline_faster_than_sustain_holds() {
    /// Pinned only for determinism (the ambient per-species `r` is order-dependent in the shared
    /// binary); the multiples scale with MSY, so the ordering is `r`-independent.
    const PINNED_R: f32 = 0.05;
    let mut app = spawn_world();
    let (deplete_herd, surplus_herd) = prime_two_stationary_herds(&mut app);
    // A third herd on Sustain, to show it holds while the other two decline.
    let sustain_herd = {
        let reg = app.world.resource::<HerdRegistry>();
        reg.herds
            .iter()
            .find(|h| h.id.starts_with("game_") && h.id != deplete_herd && h.id != surplus_herd)
            .map(|h| h.id.clone())
            .expect("a third game herd")
    };
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        for id in [&deplete_herd, &surplus_herd, &sustain_herd] {
            let h = registry.herds.iter_mut().find(|h| &h.id == id).unwrap();
            h.regrowth_rate = PINNED_R;
            h.carrying_capacity = 4000.0;
            h.biomass = 4000.0; // start FULL so the decline is visible
            h.body_mass = COMPARISON_BODY_MASS;
        }
    }
    let deplete_band = spawn_hunter(&mut app, &deplete_herd, 0.15, FactionId(0));
    let surplus_band = spawn_hunter(&mut app, &surplus_herd, 0.3, FactionId(1));
    spawn_hunter(&mut app, &sustain_herd, 0.5, FactionId(2));

    run_turns(&mut app, 10);

    let deplete =
        biomass_ratio(&app, &deplete_herd).expect("deplete herd still declining, not gone");
    let surplus = biomass_ratio(&app, &surplus_herd).expect("surplus herd still declining");
    let sustain = biomass_ratio(&app, &sustain_herd).expect("sustain herd held");
    assert!(
        deplete < surplus,
        "Deplete declines faster than Surplus: {deplete} vs {surplus}"
    );
    assert!(
        surplus < sustain,
        "Surplus declines while Sustain holds: surplus {surplus} vs sustain {sustain}"
    );
    // Sustain settles a full herd toward K/2 and holds it — well above either extraction floor.
    assert!(
        sustain > 0.5,
        "Sustain holds the herd at/above its K/2 operating point: {sustain}"
    );
    // Commercial harvest: bigger take + boosted trade rate → far more trade goods.
    let deplete_trade = trade_goods(&app, deplete_band);
    let surplus_trade = trade_goods(&app, surplus_band);
    assert!(
        deplete_trade > surplus_trade,
        "deplete should out-earn surplus on trade: deplete {deplete_trade} vs surplus {surplus_trade}"
    );
}

/// **Deplete strips a herd to the Allee brink and PINS it there; ERADICATE is what ends it.**
///
/// Under constant escapement a stance *is* its floor (`docs/plan_harvest_floor.md` §1), so where a
/// herd ends up is decided by that one number and nothing else: Deplete's transitional floor is
/// `ecology.collapse_fraction · K`, the depensation threshold, so a Deplete hunt takes everything
/// above it and then takes only the trickle the brink regrows — a Collapsing remnant that survives.
/// **Extinction is the floor-`0` case**: Eradicate leaves nothing standing, the herd falls under
/// `extinction_floor · K`, and `advance_herds` despawns it.
///
/// That is a deliberate change of meaning. The retired axis made Deplete a *constant catch* of
/// `2.5 × MSY`, which has no equilibrium and therefore drove extinction as a side effect of
/// arithmetic; a floor is a statement about where you stop, and one placed at the brink stops at the
/// brink. Slow breeders make both traces legible within the horizon.
#[test]
fn deplete_pins_a_herd_at_the_brink_while_eradicate_ends_it() {
    /// Below the ~0.25 collapse threshold — deer/megafauna, the slow game a heavy cull cannot outrun.
    const SLOW_BREEDER_R: f32 = 0.05;
    /// Long enough for either outcome to have resolved several times over.
    const HORIZON_TURNS: u32 = 40;

    // --- Deplete: stripped to the brink, still alive.
    let mut app = spawn_world();
    let (herd, _other) = prime_two_stationary_herds(&mut app);
    let cap = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let h = registry.herds.iter_mut().find(|h| h.id == herd).unwrap();
        h.regrowth_rate = SLOW_BREEDER_R;
        h.carrying_capacity
    };
    spawn_hunter(&mut app, &herd, 0.15, FactionId(0));
    run_turns(&mut app, HORIZON_TURNS);
    let collapse_fraction = app
        .world
        .resource::<FaunaConfigHandle>()
        .get()
        .ecology
        .collapse_fraction;
    let remnant = biomass_ratio(&app, &herd)
        .map(|ratio| ratio * cap)
        .expect("a Deplete hunt leaves a remnant standing at its floor, it does not end the herd");
    assert!(
        (remnant - collapse_fraction * cap).abs() < cap * 0.05,
        "Deplete pins the herd at its `collapse_fraction · K` floor ({}): got {remnant}",
        collapse_fraction * cap
    );
    // The phase is classified after Logistics regrowth, so a herd pinned at the brink reads the band
    // just above it — distressed, never Thriving, which is the warning the player is owed.
    let phase = app
        .world
        .resource::<HerdRegistry>()
        .find(&herd)
        .map(|h| h.ecology_phase);
    assert!(
        matches!(
            phase,
            Some(EcologyPhase::Stressed) | Some(EcologyPhase::Collapsing)
        ),
        "a herd held at the Allee brink is visibly distressed, never Thriving: {phase:?}"
    );

    // --- Eradicate: nothing left standing, and the herd is gone.
    let mut app = spawn_world();
    let (herd, _other) = prime_two_stationary_herds(&mut app);
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let h = registry.herds.iter_mut().find(|h| h.id == herd).unwrap();
        h.regrowth_rate = SLOW_BREEDER_R;
    }
    let band = spawn_hunter(&mut app, &herd, 0.0, FactionId(0));
    run_turns(&mut app, HORIZON_TURNS);
    assert!(
        app.world.resource::<HerdRegistry>().find(&herd).is_none(),
        "an Eradicate hunt takes the whole standing stock and the herd despawns"
    );
    // Once the herd is gone the assignment lapses.
    assert!(
        !has_hunt_assignment(&app, band),
        "assignment should lapse after the herd despawns"
    );
}

/// Deplete hunting never tames a herd — only Sustain accrues husbandry.
#[test]
fn deplete_hunt_does_not_domesticate() {
    let mut app = spawn_world();
    let (herd, _other) = prime_two_stationary_herds(&mut app);
    spawn_hunter(&mut app, &herd, 0.15, FactionId(0));
    run_turns(&mut app, 4);
    let progress = app
        .world
        .resource::<HerdRegistry>()
        .find(&herd)
        .map(|h| h.domestication_progress)
        .unwrap_or(0.0);
    assert_eq!(
        progress, 0.0,
        "deplete hunting must not accrue domestication"
    );
}

/// **THE ordering invariant the whole rework exists to guarantee: `Sustain ≤ Surplus ≤ Deplete ≤
/// Eradicate` in per-turn take, at every biomass and for every species.**
///
/// *"Each option must take more than the previous, or it looks strange to the player."* This is the
/// property a single-point measurement hid and a proportional skim silently broke (a fixed `%` does not
/// scale with MSY, so it inverts against Sustain on a fast breeder — measured in play: Wild Fowl `r`
/// 0.35, Sustain 0.22 vs a 0.10·B Surplus 0.15). With Surplus/Deplete as **ascending multiples of the
/// same MSY base** (1.5× / 2.5×) it holds by construction, and this test is the regression guard
/// against anyone reintroducing a skim or reordering the multipliers.
///
/// Asserted **non-strict** (`≤`): Sustain is `0` below `K/2` (escapement), so it legitimately ties
/// Surplus's clamped-to-tiny-stock take there. Where biomass clears the escapement point (B = K) the
/// order is checked **strict**.
///
/// `r`-swept because the guarantee must be `r`-independent — the multiples are of MSY (which scales
/// with `r`, so all four scale together), and a take that depended on `r` *differently* per policy (a
/// skim) is exactly the failure mode this guards.
#[test]
fn hunt_policy_takes_are_strictly_ordered_at_every_biomass() {
    const CAP: f32 = 4000.0;
    // The four *sustaining/extracting* policies in ascending harshness — the ladder the player reads.
    let axis = [0.5, 0.3, 0.15, 0.0];

    // Fast AND slow: the ordering must not depend on the breeding rate at all — and since the harvest
    // floor it *cannot*, because the ceiling has no `r` term to depend on. Swept anyway: the sweep is
    // the guard against anyone putting one back.
    for r in [0.35f32, 0.05] {
        // B = K (clears every floor → strict), just above K/2, K/2, and down at the brink.
        for frac in [1.0f32, 0.55, 0.51, 0.50, 0.30, 0.16] {
            let biomass = CAP * frac;
            // The TAKE this turn: the stock standing above each stance's escapement floor
            // (`hunt_escapement_ceiling`). A deeper floor leaves less standing, so it takes more —
            // the ordering is now a property of the floor table rather than of a multiplier ladder.
            let takes: Vec<f32> = axis
                .iter()
                .map(|p| hunt_escapement_ceiling(*p, biomass, CAP))
                .collect();
            for pair in takes.windows(2) {
                assert!(
                    pair[0] <= pair[1] + 1e-3,
                    "hunt takes must ascend Sustain≤Surplus≤Deplete≤Eradicate (r={r}, B={biomass}): \
                     {takes:?}"
                );
            }
            // Above K/2 Sustain's rate is a full MSY and the stock dwarfs every multiple, so the order
            // is STRICT — the healthy-herd case the player reads, and the one the skim inverted. (On a
            // small remnant the multiples clamp to the stock and tie Eradicate — non-strict.)
            if frac >= 0.55 {
                for pair in takes.windows(2) {
                    assert!(
                        pair[0] < pair[1],
                        "on a healthy herd every option must take strictly more than the last \
                         (r={r}, B={biomass}): {takes:?}"
                    );
                }
            }
        }
    }
}

/// Seat a herd at an explicit `(biomass, cap, r, body)` for a whole-animal measurement, and keep
/// `biomass_before_regrowth` in sync (Sustain's rate reads it — see `Herd::biomass_before_regrowth`).
fn seat_measure_herd(app: &mut App, id: &str, biomass: f32, cap: f32, r: f32, body: f32) {
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    herd.carrying_capacity = cap;
    herd.biomass = biomass;
    herd.biomass_before_regrowth = biomass;
    herd.regrowth_rate = r;
    herd.body_mass = body;
    herd.hunt_credit = 0.0;
}

/// **A FULL herd (B = K) under Sustain lands on `K/2` and then pays ~MSY forever — it does NOT stick
/// at `K` yielding nothing** (the original playtest bug), **and it never goes below `K/2`**.
///
/// The bug: a Sustain rate written as `min(MSY, regen(B))` is `min(MSY, 0) = 0` at `B = K` (regrowth is
/// zero at capacity), so a full herd yielded nothing, never dropped below `K`, and stayed stuck forever
/// (observed on full Crag Goat / Red Deer herds). Constant escapement answers it head-on: a full herd
/// is **all** surplus above the floor.
///
/// The **first** harvest is therefore the accumulated stock, not a rate — a real windfall, and the
/// steady MSY only starts once the herd is standing on its floor. So the averaging window here begins
/// *after* the drawdown, which is what makes the ~MSY claim a statement about the steady state rather
/// than about one big turn. This runs the **full turn** (`advance_herds` regrowth, then the take), so
/// the `regen(K) = 0` interaction is live.
#[test]
fn a_full_herd_under_sustain_settles_on_half_k_and_then_pays_msy() {
    for (label, k, r, body) in [
        ("Crag Goat", 130.0f32, 0.22f32, 20.0f32),
        ("Red Deer", 1200.0f32, 0.10f32, 60.0f32),
    ] {
        let mut app = spawn_world();
        let (herd, _o) = prime_two_stationary_herds(&mut app);
        seat_measure_herd(&mut app, &herd, k, k, r, body); // FULL: B = K
        let band = spawn_hunter(&mut app, &herd, 0.5, FactionId(0));
        let provisions_per_biomass = {
            let fauna = app.world.resource::<FaunaConfigHandle>().get();
            fauna.hunt.provisions_per_biomass
        };
        let msy_provisions = r * k / 4.0 * provisions_per_biomass;

        let take_this_turn = |app: &App| {
            app.world
                .get::<LaborAllocation>(band)
                .unwrap()
                .last_yields
                .first()
                .map(|y| y.actual)
                .unwrap_or(0.0)
        };

        // **The opening windfall.** `HUNT_WORKERS` is unbounded throughput, so one turn clears the
        // whole standing surplus and lands the herd on its floor (within one body — whole animals).
        run_turns(&mut app, 1);
        let first = take_this_turn(&app);
        let after_first = biomass_ratio(&app, &herd).map(|x| x * k).unwrap();
        assert!(
            first > msy_provisions,
            "{label}: the first harvest of a full herd is its accumulated stock, not a rate: \
             {first} vs MSY {msy_provisions}"
        );
        assert!(
            (after_first - k * 0.5).abs() <= body,
            "{label}: one Sustain turn lands a full herd ON `K/2` ({}), within a whole animal — got \
             {after_first}",
            k * 0.5
        );

        // **Then the steady state.** Long enough for the whole-animal pulse to average out (a Crag
        // Goat's MSY of 7.15 biomass is under its body of 20, so it pays every ~3 turns). Read the
        // ACTUAL provisions off the yield telemetry, not inferred from biomass.
        const STEADY_TURNS: u32 = 30;
        let mut total = 0.0;
        for _ in 0..STEADY_TURNS {
            run_turns(&mut app, 1);
            total += take_this_turn(&app);
        }
        let end = biomass_ratio(&app, &herd).map(|x| x * k).unwrap();
        let avg = total / STEADY_TURNS as f32;

        assert!(
            (avg - msy_provisions).abs() < msy_provisions * 0.15,
            "{label}: at its floor a Sustain hunt pays ~MSY ({msy_provisions}), NOT 0 — got {avg}/turn"
        );
        assert!(
            end >= k * 0.5 - body,
            "{label}: Sustain never draws a herd below `K/2` ({}) — got {end}",
            k * 0.5
        );
        assert!(
            end < k * 0.6,
            "{label}: …and it holds there rather than drifting back to K — got {end}"
        );
    }
}

/// **A below-`K/2` herd under Sustain HOLDS or RECOVERS — it never declines** (slice 8b, the
/// coordinator's explicit requirement).
///
/// Sustain's rate is `regen(min(B, K/2))` sized against the **pre-regrowth** biomass, so below `K/2` it
/// takes exactly one turn's growth and the herd holds. (Sizing it against the *post-regrowth* stock
/// would take slightly more than the herd grew — `regen(B_post) > regen(B_pre)` — and slowly leak a
/// depleted herd down, which is the corner this pins shut.) The kill-credit bank keeps Sustain
/// *selectable* at any biomass: the sub-MSY rate accumulates and pays a whole animal every few turns.
#[test]
fn a_below_half_k_herd_under_sustain_recovers_never_declines() {
    let mut app = spawn_world();
    let (herd, _o) = prime_two_stationary_herds(&mut app);
    const K: f32 = 4000.0;
    let start = 0.30 * K; // well below K/2
    seat_measure_herd(&mut app, &herd, start, K, 0.10, 60.0);
    spawn_hunter(&mut app, &herd, 0.5, FactionId(0));

    // Run a long time; the herd must never drift meaningfully below where it started.
    let mut min_seen = start;
    for _ in 0..120 {
        run_turns(&mut app, 1);
        let b = biomass_ratio(&app, &herd)
            .map(|r| r * K)
            .expect("the herd is never hunted out under Sustain");
        min_seen = min_seen.min(b);
    }
    let end = biomass_ratio(&app, &herd).map(|r| r * K).unwrap();
    assert!(
        min_seen >= start - 60.0,
        "a below-K/2 Sustain herd must not decline (start {start}, min over 120 turns {min_seen})"
    );
    assert!(
        end >= start - 60.0,
        "…and ends at or above where it started (start {start}, end {end})"
    );
}

/// **The kill-credit accumulator produces whole lumpy animals** (slice 8b): a fast breeder takes a
/// MULTIPLE of the animal every turn, a big animal waits then takes one — and the rhythm quickens up
/// the policy ladder. This is the property that makes multiples-of-MSY huntable where a flow was not.
#[test]
fn the_kill_credit_pays_multiples_for_fast_game_and_a_pulse_for_big_game() {
    // Rabbit-scale (fast, light body): MSY dwarfs one body, so it kills several per turn from turn one.
    {
        let mut app = spawn_world();
        let (herd, _o) = prime_two_stationary_herds(&mut app);
        const K: f32 = 4000.0;
        seat_measure_herd(&mut app, &herd, K, K, 0.35, 2.0); // full, fast, tiny body
        spawn_hunter(&mut app, &herd, 0.5, FactionId(0));
        let before = biomass_ratio(&app, &herd).unwrap() * K;
        run_turns(&mut app, 1);
        let after = biomass_ratio(&app, &herd).unwrap() * K;
        let killed = ((before + 0.35 * K / 4.0) - after) / 2.0; // grew then took
        assert!(
            killed >= 2.0,
            "a fast breeder's Sustain take is a MULTIPLE of the animal per turn, not clamped to one \
             (killed ~{killed})"
        );
    }
    // Big-bodied (MSY < one body): waits, then kills exactly one — more often up the ladder.
    for (policy, max_wait) in [(0.5, 9u32), (0.15, 5u32)] {
        let mut app = spawn_world();
        let (herd, _o) = prime_two_stationary_herds(&mut app);
        const K: f32 = 12000.0;
        seat_measure_herd(&mut app, &herd, K, K, 0.04, 800.0); // mammoth-scale
        spawn_hunter(&mut app, &herd, policy, FactionId(0));
        // Find the first kill — biomass drops by ~one body.
        let mut first_kill = None;
        let mut prev = biomass_ratio(&app, &herd).unwrap() * K;
        for t in 1..=20 {
            run_turns(&mut app, 1);
            let b = biomass_ratio(&app, &herd).map(|r| r * K).unwrap_or(0.0);
            if prev - b > 400.0 {
                first_kill = Some(t);
                break;
            }
            prev = b;
        }
        let t = first_kill.unwrap_or(u32::MAX);
        assert!(
            t <= max_wait,
            "{policy:?}: a big animal is hunted on a wait-then-one rhythm, quicker up the ladder \
             (first kill at turn {t}, expected ≤ {max_wait})"
        );
    }
}
