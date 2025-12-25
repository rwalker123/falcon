use core_sim::{
    build_headless_app, CapabilityFlags, PowerGridNodeTelemetry, PowerGridState, PowerNodeId,
    SimulationTick,
};

#[test]
fn power_system_skips_when_flag_disabled() {
    // Build a minimal app and turn off the Power flag.
    let mut app = build_headless_app();
    {
        let mut flags = app.world.resource_mut::<CapabilityFlags>();
        *flags = CapabilityFlags::ALWAYS_ON & !CapabilityFlags::POWER;
    }

    // Prime a tiny power grid state.
    app.world.resource_mut::<PowerGridState>().nodes.clear(); // keep empty to detect unexpected mutations

    // Run one turn; finalize set should be gated.
    app.update();

    // Tick still advances, but power grid remains untouched.
    assert_eq!(app.world.resource::<SimulationTick>().0, 1);
    assert!(app.world.resource::<PowerGridState>().nodes.is_empty());
}

#[test]
fn power_system_runs_when_flag_enabled() {
    let mut app = build_headless_app();
    // Ensure power flag is on.
    {
        let mut flags = app.world.resource_mut::<CapabilityFlags>();
        flags.insert(CapabilityFlags::POWER);
    }

    // Insert a dummy power node via resource to ensure system runs.
    // Ensure the map isn't empty to avoid accidental removal; use topology sizing as a proxy.
    app.world
        .resource_mut::<PowerGridState>()
        .nodes
        .insert(PowerNodeId(0), PowerGridNodeTelemetry::default());

    app.update();

    // Tick advanced and power grid still has the node (system didn't panic/gate it away).
    assert_eq!(app.world.resource::<SimulationTick>().0, 1);
    assert!(!app.world.resource::<PowerGridState>().nodes.is_empty());
}
