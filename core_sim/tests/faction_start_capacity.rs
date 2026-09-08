//! **What the New Game screen offers and pre-selects, per shipped map size** — the numbers a
//! player actually reads off the rival control, pinned against the shipped configs.
//!
//! These are asserted here rather than in `worldgen`'s unit tests because the answer is a
//! *composition* of three shipped files — `simulation_config.json`'s
//! `faction_start_min_separation` and `default_ai_faction_count`, and the active preset's
//! `target_land_pct` in `map_presets.json` — and a tuning change to any one of them moves every row
//! of the table. The pin is what makes such a change a decision rather than a surprise.
//!
//! See `.claude/rules/core_sim/factions.md` → "THE MAP DECIDES THE CEILING" for the model and for
//! the provenance of the `√land_fraction` discount.

use bevy::prelude::UVec2;

use core_sim::{
    faction_start_capacity, faction_start_land_fraction, max_faction_starts,
    unattended_ai_faction_count, MapPresets, SimulationConfig,
};

/// The five sizes the New Game screen offers, in the order it offers them.
///
/// ⛔ **This list is a MIRROR of the client's `MapSizes.OPTIONS`**
/// (`clients/godot_thin_client/src/scripts/MapSizes.gd`), which is its source of truth — the sim
/// has no map-size registry, because a size is a number the player sends, not a thing the sim
/// enumerates. It is duplicated here only so the ceilings can be pinned; a size added there and not
/// here costs a pinned row, never a wrong answer.
const SHIPPED_MAP_SIZES: [(&str, u32, u32); 5] = [
    ("tiny", 56, 36),
    ("small", 66, 42),
    ("standard", 80, 52),
    ("large", 104, 64),
    ("huge", 128, 80),
];

/// **The ceiling each shipped size offers, and the count it pre-selects.** Pinned, so a change to
/// the capacity model or to any lever feeding it has to look at these numbers and agree to them.
///
/// `(max_ai_factions, default_ai_factions)`. Measured against a sweep of what the land actually
/// seats without the picker relaxing — 4-6 starts on Tiny, 5-7 Small, 8-9 Standard, 11-13 Large,
/// 15-20 Huge — which these sit at or just under, deliberately: over-promising costs spacing,
/// under-promising costs a seat.
const SHIPPED_OFFERS: [(&str, u32, u32); 5] = [
    ("tiny", 3, 1),
    ("small", 4, 1),
    ("standard", 6, 2),
    ("large", 11, 3),
    ("huge", 17, 5),
];

/// The levers the shipped configs actually carry, read once rather than restated as numbers.
fn shipped_levers() -> (u32, f32, Option<u32>) {
    let config = SimulationConfig::builtin();
    let presets = MapPresets::builtin();
    (
        config.faction_start_min_separation,
        faction_start_land_fraction(&presets, &config.map_preset_id),
        config.default_ai_faction_count,
    )
}

/// ⛔ **SMALL AND STANDARD MUST NOT AGREE.** They did: the ceiling was a count of lattice points at
/// the minimum separation, and 66x42 and 80x52 both fit 4 columns x 3 rows at 20, so both offered
/// **11** rivals though Standard has half again the area. That was the quantisation artifact this
/// model replaced, and this row is where it would come back.
#[test]
fn every_shipped_map_size_offers_its_own_ceiling() {
    let (separation, land_fraction, _) = shipped_levers();

    let mut previous: Option<(&str, u32)> = None;
    for (key, width, height) in SHIPPED_MAP_SIZES {
        let ceiling = max_faction_starts(UVec2::new(width, height), separation, land_fraction) - 1;
        if let Some((previous_key, previous_ceiling)) = previous {
            assert!(
                ceiling > previous_ceiling,
                "{key} offers {ceiling} rivals and {previous_key} offers {previous_ceiling} — a \
                 bigger map must offer strictly more, or two sizes are indistinguishable on the \
                 rival control"
            );
        }
        previous = Some((key, ceiling));
    }
}

/// The pinned table itself: ceiling and pre-selection, per size, against the shipped configs.
#[test]
fn the_shipped_map_sizes_offer_and_preselect_exactly_these_counts() {
    let (separation, land_fraction, configured) = shipped_levers();
    assert_eq!(
        configured, None,
        "the shipped config derives the pre-selection rather than pinning it; a pin would make \
         every expected default below the pinned number instead"
    );

    for ((key, width, height), (offer_key, expected_max, expected_default)) in
        SHIPPED_MAP_SIZES.into_iter().zip(SHIPPED_OFFERS)
    {
        assert_eq!(key, offer_key, "the two tables are in the same order");
        let capacity = faction_start_capacity(
            UVec2::new(width, height),
            separation,
            land_fraction,
            configured,
        );
        assert_eq!(
            (capacity.max_ai_factions, capacity.default_ai_factions),
            (expected_max, expected_default),
            "{key} ({width}x{height}) offers up to {} rivals pre-selecting {}, not the pinned \
             ({expected_max}, {expected_default})",
            capacity.max_ai_factions,
            capacity.default_ai_factions
        );
    }
}

/// **The pre-selection scales with the map and is always grantable** — the two properties the
/// control depends on, stated independently of the pinned numbers so a retune still has to keep
/// them.
#[test]
fn the_preselected_default_rises_with_the_map_and_never_exceeds_the_ceiling() {
    let (separation, land_fraction, configured) = shipped_levers();

    let mut previous = 0;
    for (key, width, height) in SHIPPED_MAP_SIZES {
        let capacity = faction_start_capacity(
            UVec2::new(width, height),
            separation,
            land_fraction,
            configured,
        );
        assert!(
            capacity.default_ai_factions >= previous,
            "{key} pre-selects {} rivals, fewer than the size below it ({previous})",
            capacity.default_ai_factions
        );
        assert!(
            capacity.default_ai_factions <= capacity.max_ai_factions,
            "{key} pre-selects {} rivals but only offers {}",
            capacity.default_ai_factions,
            capacity.max_ai_factions
        );
        previous = capacity.default_ai_factions;
    }
    assert!(
        previous > 0,
        "the largest shipped map must pre-select a world with neighbours — pre-selecting nobody on \
         every size is the complaint this replaced"
    );
}

/// **A config pin wins over the derivation, and `Some(0)` is a pin like any other** — the
/// distinction an `Option` exists to carry. Sabotaged by treating `Some(0)` as absent, which the
/// last case catches on every size.
#[test]
fn a_configured_count_pins_the_preselection_and_zero_is_not_absent() {
    let (separation, land_fraction, _) = shipped_levers();
    const PINNED: u32 = 2;

    for (key, width, height) in SHIPPED_MAP_SIZES {
        let grid = UVec2::new(width, height);
        let derived =
            faction_start_capacity(grid, separation, land_fraction, None).default_ai_factions;
        let pinned = faction_start_capacity(grid, separation, land_fraction, Some(PINNED));
        assert_eq!(
            pinned.default_ai_factions,
            PINNED.min(pinned.max_ai_factions),
            "{key}: a pinned count is offered as-is, clamped only by the ceiling"
        );

        let zeroed =
            faction_start_capacity(grid, separation, land_fraction, Some(0)).default_ai_factions;
        assert_eq!(zeroed, 0, "{key}: an explicit zero pre-selects nobody");
        if derived > 0 {
            assert_ne!(
                zeroed, derived,
                "{key}: 'derive it' and 'explicitly none' must not be the same request"
            );
        }
    }
}

/// ⛔ **AN UNATTENDED BOOT STILL OPENS ALONE.** The pre-selection above is an *offer* to a player
/// looking at a screen; a `cargo run` server, a test harness and a `new_game` carrying no pick are
/// none of those, and there is still no AI to drive a rival — a faction they gained would sit and
/// pass. The two are different functions for exactly this reason, and merging them back together
/// fails here.
#[test]
fn an_unattended_boot_takes_no_rivals_while_the_screen_offers_some() {
    let (separation, land_fraction, configured) = shipped_levers();

    assert_eq!(
        unattended_ai_faction_count(configured),
        0,
        "the shipped config leaves the unattended roster at no rivals"
    );
    assert_eq!(
        unattended_ai_faction_count(Some(2)),
        2,
        "and the pin is the escape hatch a headless run or a test uses"
    );

    let standard = UVec2::new(80, 52);
    let offered =
        faction_start_capacity(standard, separation, land_fraction, configured).default_ai_factions;
    assert!(
        offered > unattended_ai_faction_count(configured),
        "the screen pre-selects {offered} rivals on a Standard map while an unattended boot takes \
         none — if these agree, the offer has leaked into the boot path"
    );
}

/// The world a headless boot actually builds carries the unattended roster, not the offered one —
/// the end-to-end half of the split above, through `build_test_app`'s real `build_headless_app`.
#[test]
fn a_headless_boot_builds_a_one_faction_world() {
    let app = core_sim::build_test_app();
    assert_eq!(
        app.world
            .resource::<core_sim::FactionRegistry>()
            .ai_faction_count(),
        0,
        "the shipped boot is the single-faction world it has always been"
    );
    assert_eq!(
        app.world.resource::<core_sim::TurnQueue>().awaiting().len(),
        1,
        "and the turn queue awaits exactly that roster"
    );
}
