//! Market hunting: the commercial `FollowPolicy::Market` strips a herd to the Allee brink and holds it
//! there — the harshest of the four **ordered escapement targets** (Sustain K/2 > Surplus 0.30K >
//! Market 0.15K > Eradicate 0). Also home to the axis's ordering invariant
//! (`hunt_policy_takes_are_strictly_ordered_at_every_biomass`). Uses the source-centric labor
//! allocation (a Hunt assignment) that replaced the retired persistent follow.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::MinimalPlugins;

use core_sim::hunt_policy_ceiling;
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

fn biomass_of(app: &App, id: &str) -> Option<f32> {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .map(|h| h.biomass)
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

/// **Market strips a herd to a LOWER remnant than Surplus, and earns more trade** — the
/// ordered-escapement form of "Market is the harsher commercial policy".
///
/// Retargeted from `market_declines_faster_and_earns_more_trade_than_surplus`. Both policies are
/// escapement now, differing only in floor: Surplus stops at `0.30·K`, Market at the Allee brink
/// `0.15·K` (`< Surplus`). So Market takes strictly more each turn (bigger `B − floor`) and settles the
/// herd at a strictly lower remnant — which is what "declines faster / harsher" *becomes* once neither
/// is a rate. The `r`-dependent "fast breeders resist, slow breeders crash" story the old name carried
/// is a proportional-skim property, deferred to the depletion arc (`TASKS.md`); pinning `r` here is now
/// only for determinism, not to select a regime.
///
/// The **trade-goods differential is still real and still tested**: Market's larger take × its
/// `trade_goods_multiplier` out-earns Surplus. The per-turn take *ordering* itself is pinned
/// exhaustively by `hunt_policy_takes_are_strictly_ordered_at_every_biomass`.
#[test]
fn market_settles_a_lower_remnant_and_out_earns_surplus() {
    /// Immaterial to the escapement floors (fractions of `K`, not `r`); pinned only for determinism —
    /// the ambient per-species `r` was order-dependent in the shared test binary.
    const PINNED_R: f32 = 0.05;
    let mut app = spawn_world();
    let (market_herd, surplus_herd) = prime_two_stationary_herds(&mut app);
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        for id in [&market_herd, &surplus_herd] {
            registry
                .herds
                .iter_mut()
                .find(|h| &h.id == id)
                .unwrap()
                .regrowth_rate = PINNED_R;
        }
    }
    spawn_hunter(&mut app, &market_herd, FollowPolicy::Market, FactionId(0));
    spawn_hunter(&mut app, &surplus_herd, FollowPolicy::Surplus, FactionId(1));

    // Long enough for each to settle at its own floor.
    run_turns(&mut app, 12);

    let (collapse, surplus_floor) = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        (
            fauna.ecology.collapse_fraction,
            fauna.ecology.surplus_escapement_fraction,
        )
    };
    let market_ratio = biomass_ratio(&app, &market_herd).expect("market herd still a remnant");
    let surplus_ratio = biomass_ratio(&app, &surplus_herd).expect("surplus herd still a remnant");
    assert!(
        market_ratio < surplus_ratio,
        "Market settles a lower remnant than Surplus: market {market_ratio} vs surplus {surplus_ratio}"
    );
    // Each sits at its own escapement floor, not merely "lower".
    assert!(
        (market_ratio - collapse).abs() < 0.05,
        "Market settles at the collapse brink ({collapse}): {market_ratio}"
    );
    assert!(
        (surplus_ratio - surplus_floor).abs() < 0.05,
        "Surplus settles at its escapement floor ({surplus_floor}): {surplus_ratio}"
    );
    // Commercial harvest: bigger take + boosted trade rate → far more trade goods.
    let market_trade = trade_goods(&app, FactionId(0));
    let surplus_trade = trade_goods(&app, FactionId(1));
    assert!(
        market_trade > surplus_trade,
        "market should out-earn surplus on trade: market {market_trade} vs surplus {surplus_trade}"
    );
}

/// **Sustained market hunting strips a herd to a COLLAPSED REMNANT and holds it there** — it does not
/// extinguish it.
///
/// This is the ordered-escapement model (intensification ladder slice 8, option 1): Market is
/// escapement to `ecology.collapse_fraction · K` (0.15·K, the Allee brink), the lowest floor any
/// *sustaining* policy stops at. A herd under Market is stripped to that brink within a few turns and
/// **pinned** there — a permanently collapsed, minimal population — for as long as it is hunted. It
/// never recovers (still hunted) and never extincts (escapement holds *at* the floor; the strict
/// `biomass < allee` depensation check never fires from exactly the floor).
///
/// **This retired `market_hunt_drives_collapse`, and the differential crash it guarded is DEFERRED,
/// not lost.** Market as a proportional skim (`0.20 × B`) *did* drive slow breeders extinct while fast
/// ones survived — an `r`-dependent crossover no escapement floor can reproduce (a floor converges on
/// itself for every `r`). Ordered *targets* were chosen because the panel must read monotone — each
/// policy takes strictly more than the one below it, at every biomass — and a skim inverts against
/// Sustain's escapement (measured: Wild Fowl `r` 0.35, Sustain 0.22 vs Surplus 0.15). The
/// slow-breeder crash moves to the depletion arc (`TASKS.md`). Slow breeder kept here so the "collapsed
/// remnant, glacial recovery if ever abandoned" reading is the honest one.
#[test]
fn market_hunt_strips_a_herd_to_a_collapsed_remnant() {
    /// Deer/megafauna territory — the game commercial hunting actually targets. `r` is immaterial to
    /// *where* escapement pins the herd (the floor is a fraction of `K`, not of `r`), but a slow
    /// breeder makes "collapsed remnant that would barely crawl back" the truthful frame.
    const SLOW_BREEDER_R: f32 = 0.05;
    let mut app = spawn_world();
    let (herd, _other) = prime_two_stationary_herds(&mut app);
    let (cap, brink) = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let h = registry.herds.iter_mut().find(|h| h.id == herd).unwrap();
        h.regrowth_rate = SLOW_BREEDER_R;
        h.biomass = h.carrying_capacity; // start FULL, so we watch it get stripped to the brink
        (
            h.carrying_capacity,
            fauna.ecology.collapse_fraction * h.carrying_capacity,
        )
    };
    let band = spawn_hunter(&mut app, &herd, FollowPolicy::Market, FactionId(0));

    // A few turns to converge from full to the brink.
    run_turns(&mut app, 8);
    let stripped = biomass_of(&app, &herd).expect("the herd is a remnant, not gone");
    assert!(
        (stripped - brink).abs() <= brink * 0.10,
        "Market strips a full herd ({cap}) to the collapse brink (~{brink}): got {stripped}"
    );

    // ...and HOLDS it there — still hunted, it neither recovers nor extincts.
    run_turns(&mut app, 32);
    let held = biomass_of(&app, &herd).expect("a pinned remnant never extincts under escapement");
    assert!(
        (held - brink).abs() <= brink * 0.10,
        "Market PINS the remnant at the brink (~{brink}) rather than crashing or recovering: got {held}"
    );
    assert!(
        has_hunt_assignment(&app, band),
        "the herd is still there, so the Hunt assignment persists"
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
/// scale with the escapement floors, so it inverts against Sustain on a fast breeder — measured in play:
/// Wild Fowl `r` 0.35, Sustain 0.22 vs Surplus 0.15). With four **ordered escapement targets** it holds
/// by construction — descending floors ⇒ ascending takes — and this test is the regression guard
/// against anyone reintroducing a skim or reordering the floors.
///
/// Asserted **non-strict** (`≤`): below a policy's floor its take is `0`, so two policies both below
/// their floors legitimately tie at `0` (a herd at `0.16·K` spares nothing to Sustain *or* Surplus).
/// Where biomass clears every floor (B = K) the order is checked **strict**.
///
/// `r`-swept because the guarantee must be `r`-independent — the floors are fractions of `K`, and a
/// take that depended on `r` (a flow, or a skim) is exactly the failure mode this guards.
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
            let takes: Vec<f32> = axis
                .iter()
                .map(|p| hunt_policy_ceiling(*p, biomass, CAP, &ecology, &ladder))
                .collect();
            for pair in takes.windows(2) {
                assert!(
                    pair[0] <= pair[1] + 1e-3,
                    "hunt takes must ascend Sustain≤Surplus≤Market≤Eradicate (r={r}, B={biomass}): \
                     {takes:?}"
                );
            }
            // At full capacity every floor is cleared, so the order is STRICT — the case the player
            // sees on a healthy herd, and the one the skim inverted.
            if (frac - 1.0).abs() < f32::EPSILON {
                for pair in takes.windows(2) {
                    assert!(
                        pair[0] < pair[1],
                        "on a FULL herd every option must take strictly more than the last (r={r}): \
                         {takes:?}"
                    );
                }
            }
        }
    }
}
