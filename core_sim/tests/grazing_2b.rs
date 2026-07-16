//! Grazing Phase 2b-i — herds eat their range, movement is graze-aware, and it is all INERT on
//! carrying capacity. Runs the real Startup worldgen + the real Logistics fauna pipeline on a pinned
//! seed and measures the graze layer's response (the measure-first discipline of the design doc).

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;
use bevy::MinimalPlugins;

use core_sim::{
    advance_graze_regrowth, advance_herd_grazing, advance_herds, spawn_initial_graze,
    spawn_initial_herds, spawn_initial_world, CultureManager, DiscoveryProgressLedger,
    FactionInventory, FaunaConfigHandle, GenerationRegistry, GrazeRegistry, HerdDensityMap,
    HerdRegistry, HerdTelemetry, LadderConfigHandle, MapPresets, MapPresetsHandle,
    SimulationConfig, SimulationTick, SnapshotOverlaysConfig, SnapshotOverlaysConfigHandle,
    StartLocation, StartProfileKnowledgeTags, StartProfileKnowledgeTagsHandle,
};

/// A pinned earthlike map (the same seed the husbandry integration test uses, so it is known to
/// spawn a healthy fauna population).
const MAP_SEED: u64 = 119304647;

fn spawn_world() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);

    let mut config = SimulationConfig::builtin();
    config.map_preset_id = "earthlike".to_string();
    config.map_seed = MAP_SEED;
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
    app.world.insert_resource(HerdTelemetry::default());
    app.world.insert_resource(HerdDensityMap::default());
    app.world.insert_resource(GrazeRegistry::default());
    app.world.insert_resource(FaunaConfigHandle::default());
    app.world.insert_resource(LadderConfigHandle::default());
    // Herds spawn BEFORE graze patches (the Startup order build_route relies on) — mirror it here.
    app.world.run_system_once(spawn_initial_herds);
    app.world.run_system_once(spawn_initial_graze);
    app
}

/// One turn of the fauna Logistics chain in real stage order: herds roam → herds eat their range →
/// graze regrows the eaten state.
fn run_turn(app: &mut App) {
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(advance_herd_grazing);
    app.world.run_system_once(advance_graze_regrowth);
}

fn graze_biomass_at(app: &App, tile: UVec2) -> Option<f32> {
    app.world
        .resource::<GrazeRegistry>()
        .patch(tile)
        .map(|p| p.biomass)
}

fn graze_capacity_at(app: &App, tile: UVec2) -> Option<f32> {
    app.world
        .resource::<GrazeRegistry>()
        .patch(tile)
        .map(|p| p.carrying_capacity)
}

/// The set of tiles a mobile, non-corralled herd is currently grazing (its range).
fn occupied_tiles(app: &App) -> std::collections::HashSet<UVec2> {
    use std::collections::HashSet;
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let config = app.world.resource::<SimulationConfig>();
    let width = config.grid_size.x.max(1);
    let height = config.grid_size.y.max(1);
    let wrap = config.map_topology.wrap_horizontal;
    let mut set = HashSet::new();
    for herd in app.world.resource::<HerdRegistry>().herds.iter() {
        if herd.is_corralled() || herd.fodder_per_biomass <= 0.0 {
            continue;
        }
        let def = fauna.species_by_display(&herd.species);
        let radius = herd.graze_range_radius(def);
        for t in
            core_sim::grid_utils::hex_range_tiles(herd.current_pos, radius, width, height, wrap)
        {
            set.insert(t);
        }
    }
    set
}

#[test]
fn herds_draw_their_range_down_but_barren_stays_zero_grazed() {
    let mut app = spawn_world();
    // A campaign forward on the pinned seed.
    const TURNS: u32 = 40;
    for _ in 0..TURNS {
        run_turn(&mut app);
    }

    // (a) Mean graze on herd-occupied vs unoccupied tiles — grazing should visibly draw range down.
    let occupied = occupied_tiles(&app);
    let mut occ_ratio_sum = 0.0;
    let mut occ_n = 0u32;
    let mut free_ratio_sum = 0.0;
    let mut free_n = 0u32;
    let registry = app.world.resource::<GrazeRegistry>();
    for (tile, patch) in registry.patches.iter() {
        if patch.carrying_capacity <= 0.0 {
            continue;
        }
        let ratio = patch.biomass / patch.carrying_capacity;
        if occupied.contains(tile) {
            occ_ratio_sum += ratio;
            occ_n += 1;
        } else {
            free_ratio_sum += ratio;
            free_n += 1;
        }
    }
    let occ_mean = if occ_n > 0 {
        occ_ratio_sum / occ_n as f32
    } else {
        1.0
    };
    let free_mean = if free_n > 0 {
        free_ratio_sum / free_n as f32
    } else {
        1.0
    };
    println!(
        "graze fill: occupied {:.1}% over {occ_n} tiles, unoccupied {:.1}% over {free_n} tiles",
        occ_mean * 100.0,
        free_mean * 100.0
    );
    assert!(occ_n > 0, "some herds should be grazing on a live map");
    assert!(
        occ_mean < free_mean,
        "herd-occupied pasture is drawn below untouched pasture: {occ_mean} vs {free_mean}"
    );
    assert!(
        free_mean > 0.98,
        "untouched pasture sits at capacity: {free_mean}"
    );

    // (b) The fraction of mobile herds ending a turn on a zero-graze tile (no patch) must be ~0 —
    // movement avoids barren ground.
    let herds = app.world.resource::<HerdRegistry>();
    let mobile: Vec<_> = herds.herds.iter().filter(|h| !h.is_corralled()).collect();
    let on_barren = mobile
        .iter()
        .filter(|h| graze_capacity_at(&app, h.current_pos).unwrap_or(0.0) <= 0.0)
        .count();
    let frac = on_barren as f32 / mobile.len().max(1) as f32;
    println!(
        "herds on zero-graze tiles: {on_barren}/{} = {:.1}%",
        mobile.len(),
        frac * 100.0
    );
    assert!(
        frac <= 0.05,
        "almost no herd ends a turn on barren ground: {on_barren}/{}",
        mobile.len()
    );
}

#[test]
fn a_grazed_cluster_recovers_after_the_herd_leaves() {
    let mut app = spawn_world();
    // Watch the cluster a migratory herd is grazing (its whole loiter range). Fall back to any grazer.
    let cluster = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let config = app.world.resource::<SimulationConfig>();
        let (w, h, wrap) = (
            config.grid_size.x.max(1),
            config.grid_size.y.max(1),
            config.map_topology.wrap_horizontal,
        );
        let herds = app.world.resource::<HerdRegistry>();
        let herd = herds
            .herds
            .iter()
            .find(|h| h.fodder_per_biomass > 0.0 && h.size_class == core_sim::SizeClass::Migratory)
            .or_else(|| herds.herds.iter().find(|h| h.fodder_per_biomass > 0.0))
            .expect("a grazing herd spawned");
        let def = fauna.species_by_display(&herd.species);
        let radius = herd.graze_range_radius(def);
        core_sim::grid_utils::hex_range_tiles(herd.current_pos, radius, w, h, wrap)
    };

    // Graze the map hard for a stretch, then measure the cluster's low point.
    for _ in 0..12 {
        run_turn(&mut app);
    }
    let grazed_min = cluster
        .iter()
        .filter_map(|t| Some(graze_biomass_at(&app, *t)? / graze_capacity_at(&app, *t)?))
        .fold(f32::INFINITY, f32::min);
    println!("cluster low point while grazed: {:.1}%", grazed_min * 100.0);
    assert!(
        grazed_min < 0.95,
        "the cluster was genuinely grazed down: {grazed_min}"
    );

    // Every herd leaves (clear the registry) — nothing eats the cluster now, so it regrows. Clearing
    // isolates the recovery from *other* herds wandering onto the vacated ground, which a live map
    // full of herds otherwise allows (the emergent chase is deferred to 2c anyway).
    app.world.resource_mut::<HerdRegistry>().herds.clear();
    for _ in 0..40 {
        app.world.run_system_once(advance_graze_regrowth);
    }
    let recovered_min = cluster
        .iter()
        .filter_map(|t| Some(graze_biomass_at(&app, *t)? / graze_capacity_at(&app, *t)?))
        .fold(f32::INFINITY, f32::min);
    println!(
        "cluster low point after recovery: {:.1}%",
        recovered_min * 100.0
    );
    assert!(
        recovered_min > grazed_min + 0.05 && recovered_min > 0.9,
        "the vacated cluster recovers toward capacity: {recovered_min} vs grazed {grazed_min}"
    );
}

#[test]
fn grazing_makes_carrying_capacity_ecological() {
    // Grazing 2b-ii: `K` is no longer the species constant — a mobile herd's carrying capacity is
    // recomputed each turn from the graze its range yields. So on a real map (a) the economy MOVES off
    // the constants (the layer is live, not inert), and (b) every surviving herd's `K` stays finite and
    // positive — movement (§4.1) keeps herds on grass, so K never crashes to zero on recoverable ground.
    let mut app = spawn_world();

    // The species-constant K each herd carried at spawn (the pre-2b-ii value).
    let spawn_caps: std::collections::HashMap<String, f32> = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        app.world
            .resource::<HerdRegistry>()
            .herds
            .iter()
            .map(|h| {
                let cap = fauna
                    .species_by_display(&h.species)
                    .map(|d| d.carrying_capacity())
                    .unwrap_or(h.carrying_capacity);
                (h.id.clone(), cap)
            })
            .collect()
    };

    for _ in 0..30 {
        run_turn(&mut app);
    }

    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let mut moved_off_constant = 0usize;
    let mut grazing_herds = 0usize;
    for herd in app.world.resource::<HerdRegistry>().herds.iter() {
        // A surviving herd's K is finite and strictly positive — no NaN, no crash-to-zero.
        assert!(
            herd.carrying_capacity.is_finite() && herd.carrying_capacity > 0.0,
            "herd {} keeps a finite, positive ecological K, got {}",
            herd.id,
            herd.carrying_capacity
        );
        // The resolver still reads the (now dynamic) cached field for a mobile herd — every downstream
        // consumer is unchanged; only the value it caches moved.
        if !herd.is_corralled() {
            assert_eq!(
                core_sim::herd_capacity(herd, &fauna),
                herd.carrying_capacity
            );
        }
        if herd.fodder_per_biomass > 0.0 {
            grazing_herds += 1;
            if let Some(&spawn_cap) = spawn_caps.get(&herd.id) {
                if (herd.carrying_capacity - spawn_cap).abs() > 1e-3 {
                    moved_off_constant += 1;
                }
            }
        }
    }
    assert!(grazing_herds > 0, "the map spawns grazing herds to measure");
    assert!(
        moved_off_constant > 0,
        "at least one grazing herd's K became range-derived (the layer is live, not inert): \
         {moved_off_constant}/{grazing_herds} moved off the species constant"
    );
}

/// **THE 2b-ii MEASUREMENT** (`docs/plan_grazing_2b.md` §9). Run the real earthlike map forward until
/// K settles, then report — per species — the ecological-K distribution (min/mean/max across the map's
/// herds) vs. the retired constant, and the re-balanced hunting economy (Sustain MSY = `r·K/4·p` at the
/// *new* per-species `r`; Market = `take_fraction·K·p`). Prints only — it asserts nothing, so retuning
/// the levers moves the numbers, not the verdict. Run with `--nocapture`.
#[test]
fn the_2b_ii_measurement_report() {
    let mut app = spawn_world();
    // Long enough for the coupled loop to settle every herd on its range (the convergence test shows
    // fixed points reached far sooner).
    for _ in 0..120 {
        run_turn(&mut app);
    }
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let provisions = fauna.hunt.provisions_per_biomass;
    let market_fraction = fauna.market.take_fraction;
    let wild_default = fauna.ecology.regrowth_rate;

    // Group surviving herds by species.
    use std::collections::BTreeMap;
    let mut by_species: BTreeMap<String, Vec<f32>> = BTreeMap::new();
    for herd in app.world.resource::<HerdRegistry>().herds.iter() {
        by_species
            .entry(herd.species.clone())
            .or_default()
            .push(herd.carrying_capacity);
    }

    println!("\n=== 2b-ii K distribution (earthlike seed {MAP_SEED}, 120 turns) ===");
    println!(
        "  {:<18} {:>4} {:>8} {:>8} {:>8} {:>10} {:>6} {:>8} {:>9} {:>9}",
        "species", "n", "K min", "K mean", "K max", "old const", "r", "old r", "Sustain", "Market"
    );
    for (species, caps) in &by_species {
        let n = caps.len();
        let (mut lo, mut hi, mut sum) = (f32::INFINITY, f32::NEG_INFINITY, 0.0);
        for &c in caps {
            lo = lo.min(c);
            hi = hi.max(c);
            sum += c;
        }
        let mean = sum / n as f32;
        let def = fauna.species_by_display(species);
        let old_const = def.map(|d| d.carrying_capacity()).unwrap_or(0.0);
        let r = def.and_then(|d| d.regrowth_rate).unwrap_or(wild_default);
        // Sustain MSY and Market take at the MEAN ecological K, in provisions/turn.
        let sustain = r * mean / 4.0 * provisions;
        let market = market_fraction * mean * provisions;
        println!(
            "  {species:<18} {n:>4} {lo:>8.0} {mean:>8.0} {hi:>8.0} {old_const:>10.0} {r:>6.2} \
             {wild_default:>8.2} {sustain:>9.3} {market:>9.3}"
        );
    }
    println!(
        "  (Sustain = r·K/4·{provisions}; Market = {market_fraction}·K·{provisions}; old r was the \
         single global {wild_default})\n"
    );
}
