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
///
/// The pre-selections are `1 + ceiling/2` — a floor of one rival on any map that can seat one, plus
/// half of what the map holds. The share-of-the-ceiling rule this replaced gave Tiny and Small the
/// same `1`, and a rival control that reads identically on two map sizes says the two sizes are the
/// same world.
const SHIPPED_OFFERS: [(&str, u32, u32); 5] = [
    ("tiny", 3, 2),
    ("small", 4, 3),
    ("standard", 6, 4),
    ("large", 11, 6),
    ("huge", 17, 9),
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

/// The pre-selection each shipped size derives, in the order the screen offers them.
fn preselected_defaults() -> Vec<(&'static str, u32, u32)> {
    let (separation, land_fraction, configured) = shipped_levers();
    SHIPPED_MAP_SIZES
        .into_iter()
        .map(|(key, width, height)| {
            let capacity = faction_start_capacity(
                UVec2::new(width, height),
                separation,
                land_fraction,
                configured,
            );
            (key, capacity.default_ai_factions, capacity.max_ai_factions)
        })
        .collect()
}

/// ⛔ **THE PRE-SELECTION RISES STRICTLY, NEVER MERELY WEAKLY.** A bigger world must open with more
/// company in it, and *equal* is the failure this exists to catch: under the share-of-the-ceiling
/// rule this replaced, Tiny and Small both pre-selected **1**, so the two sizes were
/// indistinguishable on the one control that says how populated a game will feel. Reverting to a
/// share fails here on that pair.
///
/// Stated as a property rather than as the numbers, so a retune of the separation, the land target
/// or the capacity model still has to keep it.
#[test]
fn every_shipped_map_size_preselects_strictly_more_rivals_than_the_one_below_it() {
    let mut previous: Option<(&str, u32)> = None;
    for (key, default, _) in preselected_defaults() {
        if let Some((previous_key, previous_default)) = previous {
            assert!(
                default > previous_default,
                "{key} pre-selects {default} rivals and {previous_key} pre-selects \
                 {previous_default} — a bigger map must open with strictly more company, or the \
                 two sizes read identically on the rival control"
            );
        }
        previous = Some((key, default));
    }
}

/// **No two shipped sizes may share a pre-selection.** Implied by the strict rise above while that
/// holds, and asserted separately anyway: it is the property the player actually notices, and it
/// must survive somebody weakening the ordering test.
#[test]
fn no_two_shipped_map_sizes_preselect_the_same_rival_count() {
    let defaults = preselected_defaults();
    for (index, (key, default, _)) in defaults.iter().enumerate() {
        for (other_key, other_default, _) in &defaults[index + 1..] {
            assert_ne!(
                default, other_default,
                "{key} and {other_key} both pre-select {default} rivals — two map sizes offering \
                 the same starting world are the same map size as far as this control is concerned"
            );
        }
    }
}

/// **Every pre-selection is grantable, and the smallest map still opens with neighbours** — the two
/// floors under the ladder. `> 0` is asserted on the *smallest* size deliberately: a game with
/// nobody else in it is the thing this pre-selection exists to prevent, so it is the map most likely
/// to regress that has to prove it.
#[test]
fn no_shipped_size_preselects_more_rivals_than_it_seats_or_leaves_the_player_alone() {
    let defaults = preselected_defaults();
    for (key, default, max) in &defaults {
        assert!(
            default <= max,
            "{key} pre-selects {default} rivals but only offers {max}"
        );
    }
    let (smallest_key, smallest_default, _) = defaults[0];
    assert!(
        smallest_default > 0,
        "the smallest shipped map ({smallest_key}) pre-selects {smallest_default} rivals — a world \
         with nobody else in it is what this pre-selection exists to prevent"
    );
}

/// ⛔ **A DEGENERATE GRID CANNOT PRE-SELECT A RIVAL IT HAS NOWHERE TO PUT.** The rule carries a
/// floor term, so the clamp to the ceiling is what stops a map seating 1 or 2 peoples from offering
/// a start that does not exist. Both degenerate ceilings are exercised — 0 (the grid seats the
/// player alone) and 1 (it seats exactly one rival, which is also what the floor asks for).
#[test]
fn a_map_that_seats_almost_nobody_preselects_only_what_it_seats() {
    let (separation, _, configured) = shipped_levers();
    // A grid too small for a second start at the shipped separation, and the shipped Tiny grid with
    // its land discounted until it seats exactly two peoples — the two ways a ceiling gets small.
    let alone = UVec2::new(10, 10);
    assert_eq!(
        max_faction_starts(alone, separation, 1.0) - 1,
        0,
        "fixture: this grid seats the player and nobody else"
    );
    let capacity = faction_start_capacity(alone, separation, 1.0, configured);
    assert_eq!(
        (capacity.max_ai_factions, capacity.default_ai_factions),
        (0, 0),
        "a grid that seats one people pre-selects no rivals at all"
    );

    let barely = UVec2::new(28, 20);
    assert_eq!(
        max_faction_starts(barely, separation, 1.0) - 1,
        1,
        "fixture: this grid seats exactly one rival"
    );
    let capacity = faction_start_capacity(barely, separation, 1.0, configured);
    assert_eq!(
        (capacity.max_ai_factions, capacity.default_ai_factions),
        (1, 1),
        "and a grid that seats one rival pre-selects that one, never more"
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
