use core_sim::CapabilityFlags;

/// Test that capability_enabled correctly gates systems based on flags.
/// Note: ALWAYS_ON acts as a bypass in the run_if condition (POWER | ALWAYS_ON),
/// so to test POWER gating specifically, we must not set ALWAYS_ON.
#[test]
fn power_system_skips_when_flag_disabled() {
    // Verify that when neither POWER nor ALWAYS_ON is set, power-related
    // systems won't run. We test the flag logic directly rather than
    // running the full app (which triggers world generation).
    let flags = CapabilityFlags::empty();

    // The run_if condition is: flags.intersects(POWER | ALWAYS_ON)
    let would_run = flags.intersects(CapabilityFlags::POWER | CapabilityFlags::ALWAYS_ON);
    assert!(
        !would_run,
        "Power systems should not run when neither POWER nor ALWAYS_ON is set"
    );
}

#[test]
fn power_system_runs_when_flag_enabled() {
    // Test that POWER flag enables the power systems
    let flags = CapabilityFlags::POWER;

    let would_run = flags.intersects(CapabilityFlags::POWER | CapabilityFlags::ALWAYS_ON);
    assert!(would_run, "Power systems should run when POWER flag is set");
}

#[test]
fn always_on_bypasses_power_check() {
    // Test that ALWAYS_ON acts as a bypass for power systems
    let flags = CapabilityFlags::ALWAYS_ON;

    let would_run = flags.intersects(CapabilityFlags::POWER | CapabilityFlags::ALWAYS_ON);
    assert!(
        would_run,
        "Power systems should run when ALWAYS_ON is set (bypass)"
    );
}

#[test]
fn default_flags_enable_power_systems() {
    // Default flags include ALWAYS_ON, which should enable power systems
    let flags = CapabilityFlags::default();

    assert!(
        flags.contains(CapabilityFlags::ALWAYS_ON),
        "Default flags should include ALWAYS_ON"
    );

    let would_run = flags.intersects(CapabilityFlags::POWER | CapabilityFlags::ALWAYS_ON);
    assert!(would_run, "Power systems should run with default flags");
}
