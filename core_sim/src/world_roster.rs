//! **The two roster/profile steps every world-build path runs before Startup** (`new_game`,
//! `load_scenario`): apply a start profile to the app's campaign resources, and seat the roster the
//! build was asked for. They live in the library so the server's builders and the scenario loader's
//! tests drive one body.

use crate::espionage::{
    CounterIntelBudgets, EspionageCatalog, FactionSecurityPolicies, SecurityPolicy,
};
use crate::orders::{FactionRegistry, TurnQueue};
use crate::resources::{FactionBorderPolicies, SimulationConfig};
use crate::start_profile::{
    ActiveStartProfile, CampaignLabel, StartArchetypeAssignment, StartProfile, StartProfileLookup,
    StartProfileOverrides,
};

/// Apply a resolved start profile to the app's campaign resources — config overrides,
/// `StartProfileLookup`, `ActiveStartProfile`, `CampaignLabel`. Shared by
/// `handle_set_start_profile` and the `new_game` rebuild; it does NOT regenerate the world, and the
/// caller runs Startup afterward.
///
/// ⛔ **It does not touch the roster, because a profile no longer declares one.** Who plays a world
/// is the AI count the new game was asked for ([`seed_faction_roster`]), so swapping the profile
/// mid-session restocks the opening and leaves the peoples alone — there is nothing about the
/// roster in the new profile to disagree with the registry the world is already running.
pub fn apply_start_profile(app: &mut bevy::prelude::App, profile: &StartProfile) {
    {
        let mut config = app.world.resource_mut::<SimulationConfig>();
        config.start_profile_id = profile.id.clone();
        config.start_profile_overrides = StartProfileOverrides::from_profile(profile);
    }
    {
        let mut lookup = app.world.resource_mut::<StartProfileLookup>();
        lookup.id = profile.id.clone();
    }
    {
        let mut active = app.world.resource_mut::<ActiveStartProfile>();
        *active = ActiveStartProfile::new(profile.clone());
    }
    {
        let mut label = app.world.resource_mut::<CampaignLabel>();
        *label = CampaignLabel::from_profile(profile);
    }
}

/// **Seat one human and `archetype_counts` rivals (per archetype, catalog order) in a world about to
/// be generated**, and re-seed
/// everything the boot path derives from that roster.
///
/// Called inside `rebuild_world_from_config`'s `configure` hook, i.e. after the replacement app is
/// built and before Startup: `build_headless_app` seeded the registry from the CONFIG FILE's count,
/// clamped against the config file's grid, and a `new_game` is neither of those things. Without this
/// the count the player picked reached `SimulationConfig` and nothing that resolves a turn ever
/// heard about it.
///
/// The counts are expected to be **already clamped** by
/// [`crate::granted_archetype_counts`] — the clamp is the caller's, because only the caller knows
/// which grid the world is being built on.
///
/// ⛔ **EVERY resource `build_headless_app` seeds from the roster is re-seeded here.** The list is
/// `FactionRegistry`, `TurnQueue`, `EspionageRoster`, `CounterIntelBudgets`,
/// `FactionSecurityPolicies` and `FactionBorderPolicies` — the six constructions taken from
/// `faction_registry.factions()` in `lib.rs`, and re-seeding only some of them is the same defect one resource further along. They
/// are built through the boot path's own constructors so a fresh faction's starting state has one
/// definition rather than two.
///
/// **`EspionageRoster` is deliberately not among them**: `initialise_espionage_roster` is a
/// `Startup` system that seeds from whatever registry it finds, and the caller runs Startup after
/// this — so re-seeding it here would be a second, earlier copy of a job the schedule already does
/// against the same roster.
pub fn seed_faction_roster(app: &mut bevy::prelude::App, archetype_counts: &[u32]) {
    // Who is which archetype: transient and config-side, read by worldgen in the Startup the caller
    // runs next (see `StartArchetypeAssignment`).
    app.world
        .resource_mut::<SimulationConfig>()
        .start_archetype_assignment = StartArchetypeAssignment::from_counts(archetype_counts);
    let registry = FactionRegistry::with_ai_factions(archetype_counts.iter().sum());
    let factions = registry.factions().to_vec();
    app.world.insert_resource(registry);
    app.world.insert_resource(TurnQueue::new(factions.clone()));
    let budget_config = app
        .world
        .resource::<EspionageCatalog>()
        .config()
        .counter_intel_budget()
        .clone();
    app.world
        .insert_resource(CounterIntelBudgets::new(&factions, &budget_config));
    app.world.insert_resource(FactionSecurityPolicies::new(
        &factions,
        SecurityPolicy::Standard,
    ));
    app.world
        .insert_resource(FactionBorderPolicies::new(&factions));
}
