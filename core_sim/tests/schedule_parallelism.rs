//! Guards that the turn schedule can actually use more than one core.
//!
//! Two things have to hold, and neither fails loudly on its own — a lost core costs throughput
//! and nothing else, which is exactly the regression a test has to carry:
//!
//! 1. **The executor is multi-threaded.** That is a `bevy` *feature* in `core_sim/Cargo.toml`
//!    (`multi-threaded`), not code, so nothing in the crate references it and a dependency edit
//!    could drop it silently. Asserted against [`Schedule::get_executor_kind`] — the executor the
//!    schedule will really run with, rather than a string grepped out of a manifest.
//! 2. **The stages are not total orders.** `build_headless_app` declares the ordering edges each
//!    system actually has instead of `.chain()`-ing every stage wholesale; re-chaining a stage
//!    would restore a single valid order and quietly serialize the turn again.
//!
//! Determinism is guarded from the other side, in `build_headless_app` itself: the `Update`
//! schedule is built with `ambiguity_detection: LogLevel::Error`, so any pair of systems with
//! conflicting data access and no ordering edge is a **boot panic**. That is what makes dropping
//! an edge safe — the pairs where order is observable cannot be left unordered. This file is the
//! complement: it checks that edges which *aren't* required were actually dropped.

use bevy::ecs::schedule::{ExecutorKind, NodeId, Schedules};
use bevy::prelude::*;
use core_sim::{build_test_app, run_turn};
use std::collections::{HashMap, HashSet};

/// Pairs that must be free to run concurrently — the point of declaring real edges.
///
/// Each is a system pair in the *same* `TurnStage` that touches disjoint state: the flora/pasture
/// registries against the fauna run in `Logistics`, telemetry against the movement run in
/// `Population`, and so on. If one of these starts reaching the other, a stage went back to being
/// a chain.
const MUST_BE_CONCURRENT: [(&str, &str); 6] = [
    // Logistics: `ForageRegistry`/`GrazeRegistry` vs. the `HerdRegistry` backbone.
    ("advance_forage_regrowth", "advance_herds"),
    ("advance_graze_regrowth", "advance_predation"),
    ("balance_supply_networks", "advance_herd_grazing"),
    // Population: pure telemetry vs. the movement/labor run.
    ("publish_trade_telemetry", "advance_labor_allocation"),
    // GreatDiscovery: capability effects vs. the reporting tail.
    ("apply_capability_effects", "export_great_discovery_metrics"),
    // Visibility: the sweep tracker vs. the ledger clear.
    ("prune_sweep_tracker", "clear_active_visibility"),
];

/// Orderings that must survive — the negative control.
///
/// Without these the test above could pass by accident: a flattening bug that dropped every edge
/// would make *everything* look concurrent. Two of the three deliberately cross a
/// `SystemTypeSet` boundary (`.after(some_system)` records its edge against the target's type set,
/// not the system node), so they fail if set expansion regresses.
const MUST_BE_ORDERED: [(&str, &str); 3] = [
    // Documented in `lib.rs`: the band's `current_tile` must be fresh before labor reads it.
    ("advance_band_movement", "advance_labor_allocation"),
    // Declared via `.after(simulate_materials)` — an edge through a type set.
    ("simulate_materials", "advance_forage_regrowth"),
    // Declared via `.after(resolve_great_discovery)` — likewise.
    ("resolve_great_discovery", "apply_capability_effects"),
];

/// The `Update`-schedule systems allowed to take `Commands`, and the only ones.
///
/// **This is a review gate, not bookkeeping.** The ambiguity gate in `build_headless_app`
/// (`ambiguity_detection: LogLevel::Error`) is what makes de-chaining safe: a pair of systems with
/// conflicting data access and no ordering edge is a boot panic, so any ordering the turn's result
/// depends on *must* be declared. `Commands` is the one hazard that gate cannot see — `Deferred`
/// access conflicts with nothing, so two systems that communicate by spawning and reading register
/// as independent while their auto-inserted `apply_deferred` sync point depends entirely on the
/// edge between them. Bevy will happily run them in either order and nothing will complain.
///
/// Today both entries sit inside the fully-serial `Population` chain, so their sync points are
/// pinned by edges that exist for other reasons. A third system taking `Commands` is not
/// necessarily wrong — it just cannot be checked by the machinery, so it has to be checked by a
/// person: does it spawn or despawn anything another system reads in the same turn, and is the
/// edge that orders them declared? Answer that, then add it here.
///
/// It matters beyond a lost core. A rollback rewound by replaying a command log reproduces the
/// original run only if every turn is a pure function of its start state, and an undeclared
/// `apply_deferred` ordering is exactly the kind of dependency that makes it not one.
///
/// `Startup` is deliberately out of scope, both here and for the ambiguity gate: it is `.chain()`-ed
/// wholesale, which is why `spawn_initial_world`, `reconcile_food_modules` and
/// `place_wondrous_sites` take `Commands` without appearing below.
const COMMANDS_SYSTEMS: [&str; 2] = ["advance_band_movement", "advance_expeditions"];

/// Reachability over the schedule's dependency graph, flattened the way bevy flattens it.
///
/// Dependency edges are declared between *nodes*, which may be system sets — `.chain()` yields
/// system-to-system edges, but `.after(some_system)` yields an edge out of that system's
/// `SystemTypeSet`. An edge `A -> B` therefore orders every leaf system under `A` before every
/// leaf system under `B`, which is what this expands before closing over.
struct Reach {
    names: HashMap<NodeId, String>,
    edges: HashMap<NodeId, HashSet<NodeId>>,
}

impl Reach {
    fn build(app: &App) -> Self {
        let schedules = app.world.resource::<Schedules>();
        let schedule = schedules
            .get(Update)
            .expect("the turn runs in the `Update` schedule");

        assert_eq!(
            schedule.get_executor_kind(),
            ExecutorKind::MultiThreaded,
            "the turn schedule is running the single-threaded executor — `core_sim/Cargo.toml` \
             lost bevy's `multi-threaded` feature, and no amount of scheduling will use a second \
             core without it"
        );

        let names: HashMap<NodeId, String> = schedule
            .systems()
            .expect("schedule is initialized after a turn")
            .map(|(id, system)| (id, system.name().to_string()))
            .collect();

        let graph = schedule.graph();

        // set -> direct children, from the hierarchy graph
        let mut children: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for (parent, child, _) in graph.hierarchy().graph().all_edges() {
            children.entry(parent).or_default().push(child);
        }

        fn leaves(node: NodeId, children: &HashMap<NodeId, Vec<NodeId>>, out: &mut Vec<NodeId>) {
            if node.is_system() {
                out.push(node);
                return;
            }
            for &child in children.get(&node).into_iter().flatten() {
                leaves(child, children, out);
            }
        }

        let mut edges: HashMap<NodeId, HashSet<NodeId>> = HashMap::new();
        for (from, to, _) in graph.dependency().graph().all_edges() {
            let (mut befores, mut afters) = (Vec::new(), Vec::new());
            leaves(from, &children, &mut befores);
            leaves(to, &children, &mut afters);
            for &before in &befores {
                for &after in &afters {
                    edges.entry(before).or_default().insert(after);
                }
            }
        }

        Self { names, edges }
    }

    fn node(&self, suffix: &str) -> NodeId {
        let matches: Vec<NodeId> = self
            .names
            .iter()
            .filter(|(_, name)| name.ends_with(&format!("::{suffix}")))
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "expected exactly one scheduled system named `{suffix}`, found {}",
            matches.len()
        );
        matches[0]
    }

    fn reaches(&self, from: NodeId, to: NodeId) -> bool {
        let mut seen = HashSet::new();
        let mut stack = vec![from];
        while let Some(node) = stack.pop() {
            for &next in self.edges.get(&node).into_iter().flatten() {
                if next == to {
                    return true;
                }
                if seen.insert(next) {
                    stack.push(next);
                }
            }
        }
        false
    }
}

#[test]
fn independent_systems_in_a_stage_are_free_to_run_concurrently() {
    let mut app = build_test_app();
    run_turn(&mut app);
    let reach = Reach::build(&app);

    for (left, right) in MUST_BE_CONCURRENT {
        let (a, b) = (reach.node(left), reach.node(right));
        assert!(
            !reach.reaches(a, b) && !reach.reaches(b, a),
            "`{left}` and `{right}` touch disjoint state but the schedule now orders them — a \
             stage was re-`.chain()`-ed, which costs a core and fails nothing else"
        );
    }

    for (before, after) in MUST_BE_ORDERED {
        let (a, b) = (reach.node(before), reach.node(after));
        assert!(
            reach.reaches(a, b),
            "`{before}` must still run before `{after}` — this ordering is load-bearing"
        );
    }
}

/// The set of `Commands`-taking systems in the turn schedule is the reviewed one.
///
/// See [`COMMANDS_SYSTEMS`] for why this is worth a test: `Commands` is the single ordering hazard
/// the boot-time ambiguity gate is structurally unable to detect.
#[test]
fn commands_using_systems_are_the_reviewed_set() {
    let mut app = build_test_app();
    run_turn(&mut app);

    let schedules = app.world.resource::<Schedules>();
    let schedule = schedules
        .get(Update)
        .expect("the turn runs in the `Update` schedule");

    // `has_deferred()` is true for any system holding a `Deferred` param, which in this crate means
    // `Commands`. Asked of the built schedule rather than grepped out of function signatures, so a
    // system that acquires `Commands` through a nested `SystemParam` is caught too.
    let mut found: Vec<String> = schedule
        .systems()
        .expect("schedule is initialized after a turn")
        .filter(|(_, system)| system.has_deferred())
        .map(|(_, system)| {
            let name = system.name();
            name.rsplit("::").next().unwrap_or(&name).to_string()
        })
        .collect();
    found.sort();
    found.dedup();

    let mut expected: Vec<String> = COMMANDS_SYSTEMS.iter().map(|s| s.to_string()).collect();
    expected.sort();

    assert_eq!(
        found, expected,
        "the set of `Commands`-taking systems in the turn schedule changed.\n\
         `Commands` registers no data access, so the ambiguity gate cannot see an ordering \
         dependency between two systems that talk through spawn/despawn — the edge has to be \
         declared by hand and reviewed by a person. Work out whether the new system's \
         `apply_deferred` ordering is pinned by a real edge, then update `COMMANDS_SYSTEMS`."
    );
}
