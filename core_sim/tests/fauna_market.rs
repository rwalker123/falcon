//! Market hunting: the commercial `FollowPolicy::Market` takes `market_multiplier × MSY` (2.5×), the
//! harshest of the four **ascending multiples of MSY** (Sustain ≤ 1× < Surplus 1.5× < Market 2.5× <
//! Eradicate = everything) — constant catch this far above MSY has no equilibrium, so it drives a herd
//! extinct. Also home to the axis's ordering invariant
//! (`hunt_policy_takes_are_strictly_ordered_at_every_biomass`). Uses the source-centric labor
//! allocation (a Hunt assignment) that replaced the retired persistent follow.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::MinimalPlugins;

use core_sim::{
    advance_herds, advance_husbandry, advance_labor_allocation, scalar_from_f32, scalar_one,
    scalar_zero, spawn_initial_herds, spawn_initial_world, CommandEventLog, CultureManager,
    DiscoveryProgressLedger, FactionId, FactionInventory, FaunaConfigHandle, FogRevealLedger,
    FollowPolicy, ForageRegistry, GenerationId, GenerationRegistry, HerdDensityMap, HerdRegistry,
    HerdTelemetry, LaborAllocation, LaborAssignment, LaborConfigHandle, LaborTarget, LadderConfig,
    LadderConfigHandle, LocalStore, MapPresets, MapPresetsHandle, MoraleCause, PopulationCohort,
    SimulationConfig, SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    StartLocation, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle, StartingUnit,
    TileRegistry, WellbeingConfigHandle,
};
use core_sim::{hunt_credit_ceiling, hunt_policy_rate};

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
    app.world.insert_resource(CommandEventLog::default());
    app.world.insert_resource(FogRevealLedger::default());
    app.world.run_system_once(spawn_initial_herds);
    app
}

/// The **body mass both comparison herds are seated at** (intensification ladder slice 8). Deer-scale
/// (60) — the "slow breeder, deer/megafauna territory" these tests are about, and heavy enough that
/// the whole-animal quantiser is genuinely engaged rather than approximating a fluid.
///
/// **This is the fix for a FALSE GREEN, and it is the whole reason this constant exists.** The map
/// hands out the first two route-1 game herds, and they are **different species** — a Wild Fowl
/// (`body_mass` 1) and a Rabbit Warren (`body_mass` 2). While ruling 4 made Surplus and Market the
/// same take, `market_declines_faster_and_earns_more_trade_than_surplus` still passed — on **nothing
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
    policy: FollowPolicy,
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
                        policy,
                    },
                    workers: HUNT_WORKERS,
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

fn trade_goods(app: &App, faction: FactionId) -> i64 {
    app.world
        .resource::<FactionInventory>()
        .stockpile(faction)
        .and_then(|m| m.get("trade_goods"))
        .copied()
        .unwrap_or(0)
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

#[test]
fn market_policy_string_round_trips() {
    assert_eq!("market".parse::<FollowPolicy>(), Ok(FollowPolicy::Market));
    assert_eq!(FollowPolicy::Market.as_str(), "market");
}

/// **Market declines a herd faster than Surplus, both decline it while Sustain holds it steady — and
/// Market out-earns Surplus on trade** (slice 8b — the multiplier model).
///
/// Every extractive policy is now a constant catch that is a **multiple of MSY**: Surplus 1.5× and
/// Market 2.5× both exceed the herd's max regrowth (1× MSY), so both decline it — Market faster.
/// Sustain (≤ 1× MSY, escapement) holds a herd at `K/2`. Measured on the same species (so the take
/// difference is policy, not `body_mass`), pinned `r` for determinism.
#[test]
fn market_and_surplus_decline_faster_than_sustain_holds() {
    /// Pinned only for determinism (the ambient per-species `r` is order-dependent in the shared
    /// binary); the multiples scale with MSY, so the ordering is `r`-independent.
    const PINNED_R: f32 = 0.05;
    let mut app = spawn_world();
    let (market_herd, surplus_herd) = prime_two_stationary_herds(&mut app);
    // A third herd on Sustain, to show it holds while the other two decline.
    let sustain_herd = {
        let reg = app.world.resource::<HerdRegistry>();
        reg.herds
            .iter()
            .find(|h| h.id.starts_with("game_") && h.id != market_herd && h.id != surplus_herd)
            .map(|h| h.id.clone())
            .expect("a third game herd")
    };
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        for id in [&market_herd, &surplus_herd, &sustain_herd] {
            let h = registry.herds.iter_mut().find(|h| &h.id == id).unwrap();
            h.regrowth_rate = PINNED_R;
            h.carrying_capacity = 4000.0;
            h.biomass = 4000.0; // start FULL so the decline is visible
            h.body_mass = COMPARISON_BODY_MASS;
        }
    }
    spawn_hunter(&mut app, &market_herd, FollowPolicy::Market, FactionId(0));
    spawn_hunter(&mut app, &surplus_herd, FollowPolicy::Surplus, FactionId(1));
    spawn_hunter(&mut app, &sustain_herd, FollowPolicy::Sustain, FactionId(2));

    run_turns(&mut app, 10);

    let market = biomass_ratio(&app, &market_herd).expect("market herd still declining, not gone");
    let surplus = biomass_ratio(&app, &surplus_herd).expect("surplus herd still declining");
    let sustain = biomass_ratio(&app, &sustain_herd).expect("sustain herd held");
    assert!(
        market < surplus,
        "Market declines faster than Surplus: {market} vs {surplus}"
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
    let market_trade = trade_goods(&app, FactionId(0));
    let surplus_trade = trade_goods(&app, FactionId(1));
    assert!(
        market_trade > surplus_trade,
        "market should out-earn surplus on trade: market {market_trade} vs surplus {surplus_trade}"
    );
}

/// **Sustained market hunting drives a herd EXTINCT** (slice 8b — extinction is real and on-map again).
///
/// Market takes `market_multiplier × MSY` (2.5×) every turn — constant catch 2.5× the herd's *maximum*
/// regrowth, so there is no equilibrium: the herd declines past the Allee threshold into the
/// depensation crash and despawns. This is the depletion mechanic the ordered-escapement cut had to
/// defer (a floor Market never crossed could only *pin* a herd at the brink); multiples of MSY restore
/// it. A slow breeder makes the extinction unambiguous within the test's horizon.
#[test]
fn market_hunt_drives_collapse() {
    /// Below the ~0.25 collapse threshold — deer/megafauna, the commercially-hunted slow game a 2.5×
    /// cull cannot outrun. (A fast breeder is driven extinct too, just faster; slow makes the trace
    /// legible.)
    const SLOW_BREEDER_R: f32 = 0.05;
    let mut app = spawn_world();
    let (herd, _other) = prime_two_stationary_herds(&mut app);
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let h = registry.herds.iter_mut().find(|h| h.id == herd).unwrap();
        h.regrowth_rate = SLOW_BREEDER_R;
    }
    let band = spawn_hunter(&mut app, &herd, FollowPolicy::Market, FactionId(0));
    run_turns(&mut app, 40);

    assert!(
        app.world.resource::<HerdRegistry>().find(&herd).is_none(),
        "market hunting should drive the group extinct"
    );
    // Once the herd is gone the assignment lapses.
    assert!(
        !has_hunt_assignment(&app, band),
        "assignment should lapse after the herd despawns"
    );
}

/// Market hunting never tames a herd — only Sustain accrues husbandry.
#[test]
fn market_hunt_does_not_domesticate() {
    let mut app = spawn_world();
    let (herd, _other) = prime_two_stationary_herds(&mut app);
    spawn_hunter(&mut app, &herd, FollowPolicy::Market, FactionId(0));
    run_turns(&mut app, 4);
    let progress = app
        .world
        .resource::<HerdRegistry>()
        .find(&herd)
        .map(|h| h.domestication_progress)
        .unwrap_or(0.0);
    assert_eq!(
        progress, 0.0,
        "market hunting must not accrue domestication"
    );
}

/// **THE ordering invariant the whole rework exists to guarantee: `Sustain ≤ Surplus ≤ Market ≤
/// Eradicate` in per-turn take, at every biomass and for every species.**
///
/// *"Each option must take more than the previous, or it looks strange to the player."* This is the
/// property a single-point measurement hid and a proportional skim silently broke (a fixed `%` does not
/// scale with MSY, so it inverts against Sustain on a fast breeder — measured in play: Wild Fowl `r`
/// 0.35, Sustain 0.22 vs a 0.10·B Surplus 0.15). With Surplus/Market as **ascending multiples of the
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
    let fauna = FaunaConfigHandle::default().get();
    let ladder = LadderConfig::builtin();
    const CAP: f32 = 4000.0;
    // The four *sustaining/extracting* policies in ascending harshness — the ladder the player reads.
    let axis = [
        FollowPolicy::Sustain,
        FollowPolicy::Surplus,
        FollowPolicy::Market,
        FollowPolicy::Eradicate,
    ];

    // Fast AND slow: the ordering must not depend on the breeding rate at all.
    for r in [0.35f32, 0.05] {
        let mut ecology = fauna.ecology;
        ecology.regrowth_rate = r;
        // B = K (clears every floor → strict), just above K/2, K/2, and down at the brink.
        for frac in [1.0f32, 0.55, 0.51, 0.50, 0.30, 0.16] {
            let biomass = CAP * frac;
            // The affordable TAKE this turn from an empty bank (credit 0): each policy's rate banked
            // and clamped to the standing stock (`hunt_credit_ceiling`). Ordering the *takes* is the
            // guarantee — the raw rate is unclamped, so a small remnant's `2.5×MSY` Market rate can
            // exceed its whole stock; the take cannot (it is `min(rate, biomass)`, `≤ Eradicate = B`).
            let takes: Vec<f32> = axis
                .iter()
                .map(|p| {
                    let rate = hunt_policy_rate(*p, biomass, CAP, &ecology, &fauna, &ladder);
                    hunt_credit_ceiling(*p, biomass, 0.0, rate)
                })
                .collect();
            for pair in takes.windows(2) {
                assert!(
                    pair[0] <= pair[1] + 1e-3,
                    "hunt takes must ascend Sustain≤Surplus≤Market≤Eradicate (r={r}, B={biomass}): \
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

/// **A FULL herd (B = K) under Sustain yields ~MSY and declines gently toward `K/2` — it does NOT
/// stick at `K` yielding nothing** (slice 8b playtest bug).
///
/// The bug: Sustain's rate written as `min(MSY, regen(B))` is `min(MSY, 0) = 0` at `B = K` (regrowth is
/// zero at capacity), so a full herd yields nothing, never drops below `K`, and stays stuck forever
/// (observed on full Crag Goat / Red Deer herds). The fix is `regen(min(B, K/2))` = **MSY at capacity**
/// (the existing `sustainable_yield` semantics). This runs the **full turn** — `advance_herds`
/// (regrowth, which is 0 at `K`) then the take — so the `regen(K) = 0` interaction is live; the
/// weaker `sustain_hunt_at_capacity_yields_msy` runs only the take and so cannot exhibit it.
#[test]
fn a_full_herd_under_sustain_yields_msy_and_declines_not_stuck_at_k() {
    for (label, k, r, body) in [
        ("Crag Goat", 130.0f32, 0.22f32, 20.0f32),
        ("Red Deer", 1200.0f32, 0.10f32, 60.0f32),
    ] {
        let mut app = spawn_world();
        let (herd, _o) = prime_two_stationary_herds(&mut app);
        seat_measure_herd(&mut app, &herd, k, k, r, body); // FULL: B = K
        let band = spawn_hunter(&mut app, &herd, FollowPolicy::Sustain, FactionId(0));
        let provisions_per_biomass = {
            let fauna = app.world.resource::<FaunaConfigHandle>().get();
            fauna.hunt.provisions_per_biomass
        };
        let msy_provisions = r * k / 4.0 * provisions_per_biomass;

        // Long enough for the kill-credit pulse to average out (Crag Goat MSY 7.15 biomass < body 20
        // waits ~3 turns per kill), stopping above K/2 so the rate is a full MSY throughout. Read the
        // ACTUAL provisions off the yield telemetry, not inferred from biomass (near K the herd's own
        // regrowth is below MSY, so a biomass-delta estimate would over-count the take).
        let mut total = 0.0;
        for _ in 0..30 {
            run_turns(&mut app, 1);
            total += app
                .world
                .get::<LaborAllocation>(band)
                .unwrap()
                .last_yields
                .first()
                .map(|y| y.actual)
                .unwrap_or(0.0);
        }
        let end = biomass_ratio(&app, &herd).map(|x| x * k).unwrap();
        let avg = total / 30.0;

        assert!(
            (avg - msy_provisions).abs() < msy_provisions * 0.15,
            "{label}: a full herd yields ~MSY ({msy_provisions}) on Sustain, NOT 0 — got {avg}/turn"
        );
        assert!(
            end < k * 0.98,
            "{label}: a full herd declines under Sustain (not stuck at K={k}) — got {end}"
        );
        assert!(
            end > k * 0.5 - body,
            "{label}: …but only GENTLY, settling toward K/2 ({}), never crashing — got {end}",
            k * 0.5
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
    spawn_hunter(&mut app, &herd, FollowPolicy::Sustain, FactionId(0));

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
        spawn_hunter(&mut app, &herd, FollowPolicy::Sustain, FactionId(0));
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
    for (policy, max_wait) in [(FollowPolicy::Sustain, 9u32), (FollowPolicy::Market, 5u32)] {
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
