//! **What the New Game screen offers and pre-selects, per shipped map size** — the numbers a
//! player actually reads off the people controls, pinned against the shipped configs.
//!
//! These are asserted here rather than in `worldgen`'s unit tests because the answer is a
//! *composition* of two shipped files — `start_profiles.json`'s per-archetype `min_separation` and
//! `default_count`, and the active preset's `target_land_pct` in `map_presets.json` — and a tuning
//! change to either moves every row of the table. The pin is what makes such a change a decision
//! rather than a surprise.
//!
//! See `.claude/rules/core_sim/factions.md` → "THE MAP DECIDES THE CEILING" for the model and for
//! the provenance of the `√land_fraction` discount.

use bevy::prelude::UVec2;

use core_sim::{
    archetype_capacities, archetype_ceiling, faction_start_land_fraction, max_faction_starts,
    unattended_archetype_counts, ArchetypeCapacity, MapPresets, SimulationConfig, StartArchetype,
    StartProfiles,
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

/// The shipped profile every pinned row is measured on.
const SHIPPED_PROFILE: &str = "late_forager_tribe";

/// **The sites each shipped size seats and the count it pre-selects, per archetype.** Pinned, so a
/// change to the capacity model or to any lever feeding it has to look at these numbers and agree
/// to them.
///
/// `(major_sites, major_default, minor_sites, minor_default)`. The major ceiling is `sites - 1`
/// (3/4/6/11/17), measured against a sweep of what the land actually seats without the picker
/// relaxing — 4-6 starts on Tiny, 5-7 Small, 8-9 Standard, 11-13 Large, 15-20 Huge — which these sit
/// at or just under, deliberately: over-promising costs spacing, under-promising costs a seat. The
/// major pre-selections are `1 + ceiling/2` and the minor's is `floor(ceiling x 0.25)` (its `default_share`), each against the EARLIER archetypes' defaults.
const SHIPPED_OFFERS: [(&str, u32, u32, u32, u32); 5] = [
    ("tiny", 4, 2, 22, 4),
    ("small", 5, 3, 31, 6),
    ("standard", 7, 4, 46, 10),
    ("large", 12, 6, 74, 16),
    ("huge", 18, 9, 114, 26),
];

fn shipped_catalog() -> Vec<StartArchetype> {
    StartProfiles::builtin()
        .get(SHIPPED_PROFILE)
        .expect("the shipped profile")
        .overrides()
        .archetypes
        .clone()
}

fn shipped_land_fraction() -> f32 {
    faction_start_land_fraction(
        &MapPresets::builtin(),
        &SimulationConfig::builtin().map_preset_id,
    )
}

/// Every shipped size's capacity rows, in the order the screen offers the sizes.
fn shipped_rows() -> Vec<(&'static str, Vec<ArchetypeCapacity>)> {
    let catalog = shipped_catalog();
    let land_fraction = shipped_land_fraction();
    SHIPPED_MAP_SIZES
        .into_iter()
        .map(|(key, width, height)| {
            (
                key,
                archetype_capacities(&catalog, UVec2::new(width, height), land_fraction),
            )
        })
        .collect()
}

/// ⛔ **SMALL AND STANDARD MUST NOT AGREE.** They did: the ceiling was a count of lattice points at
/// the minimum separation, and 66x42 and 80x52 both fit 4 columns x 3 rows at 20, so both offered
/// **11** rivals though Standard has half again the area. That was the quantisation artifact this
/// model replaced, and this row is where it would come back.
#[test]
fn every_shipped_map_size_offers_its_own_ceiling() {
    let mut previous: Option<(&str, u32)> = None;
    for (key, rows) in shipped_rows() {
        let ceiling = archetype_ceiling(rows[0].site_count, 0);
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

/// The pinned table itself: sites and pre-selection per archetype, per size, against the shipped
/// configs.
#[test]
fn the_shipped_map_sizes_offer_and_preselect_exactly_these_counts() {
    let catalog = shipped_catalog();
    assert!(
        catalog
            .iter()
            .all(|archetype| archetype.default_count.is_none()),
        "the shipped profile derives every pre-selection rather than pinning it; a pin would make \
         every expected default below the pinned number instead"
    );

    for ((key, rows), (offer_key, major_sites, major_default, minor_sites, minor_default)) in
        shipped_rows().into_iter().zip(SHIPPED_OFFERS)
    {
        assert_eq!(key, offer_key, "the two tables are in the same order");
        assert_eq!(
            (
                rows[0].site_count,
                rows[0].default_count,
                rows[1].site_count,
                rows[1].default_count
            ),
            (major_sites, major_default, minor_sites, minor_default),
            "{key}: (major sites, major default, minor sites, minor default) drifted from the pin"
        );
    }
}

/// ⛔ **THE PRE-SELECTION RISES STRICTLY, NEVER MERELY WEAKLY.** A bigger world must open with more
/// company in it, and *equal* is the failure this exists to catch: under the share-of-the-ceiling
/// rule this replaced, Tiny and Small both pre-selected **1**, so the two sizes were
/// indistinguishable on the one control that says how populated a game will feel.
///
/// Stated as a property rather than as the numbers, so a retune of the separation, the land target
/// or the capacity model still has to keep it. Asserted on the rival (first) archetype, which is
/// the one whose control this was written for.
#[test]
fn every_shipped_map_size_preselects_strictly_more_rivals_than_the_one_below_it() {
    let mut previous: Option<(&str, u32)> = None;
    for (key, rows) in shipped_rows() {
        let default = rows[0].default_count;
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

/// **Every pre-selection is grantable, and the smallest map still opens with neighbours** — the two
/// floors under the ladder. `> 0` is asserted on the *smallest* size deliberately: a game with
/// nobody else in it is the thing this pre-selection exists to prevent, so it is the map most likely
/// to regress that has to prove it.
#[test]
fn no_shipped_size_preselects_more_than_it_seats_or_leaves_the_player_alone() {
    let rows = shipped_rows();
    for (key, archetypes) in &rows {
        let mut earlier = 0;
        for row in archetypes {
            let ceiling = archetype_ceiling(row.site_count, earlier);
            assert!(
                row.default_count <= ceiling,
                "{key} pre-selects {} but only seats {ceiling}",
                row.default_count
            );
            earlier += row.default_count;
        }
    }
    let (smallest_key, smallest) = &rows[0];
    assert!(
        smallest[0].default_count > 0,
        "the smallest shipped map ({smallest_key}) pre-selects no rivals — a world with nobody \
         else in it is what this pre-selection exists to prevent"
    );
}

/// ⛔ **A DEGENERATE GRID CANNOT PRE-SELECT A PEOPLE IT HAS NOWHERE TO PUT.** The rule carries a
/// floor term, so the clamp to the ceiling is what stops a map seating 1 or 2 peoples from offering
/// a start that does not exist. Both degenerate ceilings are exercised — 0 (the grid seats the
/// player alone) and 1 (it seats exactly one rival, which is also what the floor asks for).
#[test]
fn a_map_that_seats_almost_nobody_preselects_only_what_it_seats() {
    let catalog = shipped_catalog();
    let separation = catalog[0].min_separation;
    // A grid too small for a second start at the shipped separation, and one that seats exactly two.
    let alone = UVec2::new(10, 10);
    assert_eq!(
        max_faction_starts(alone, separation, 1.0) - 1,
        0,
        "fixture: this grid seats the player and nobody else"
    );
    let rows = archetype_capacities(&catalog, alone, 1.0);
    assert_eq!(
        rows[0].default_count, 0,
        "a grid that seats one people pre-selects no rivals at all"
    );

    let barely = UVec2::new(28, 20);
    assert_eq!(
        max_faction_starts(barely, separation, 1.0) - 1,
        1,
        "fixture: this grid seats exactly one rival"
    );
    let rows = archetype_capacities(&catalog, barely, 1.0);
    assert_eq!(
        rows[0].default_count, 1,
        "and a grid that seats one rival pre-selects that one, never more"
    );
}

/// **A pin wins over the derivation, and `Some(0)` is a pin like any other** — the distinction an
/// `Option` exists to carry. Sabotaged by treating `Some(0)` as absent, which the last case catches
/// on every size.
#[test]
fn a_pinned_count_pins_the_preselection_and_zero_is_not_absent() {
    const PINNED: u32 = 2;
    let land_fraction = shipped_land_fraction();

    for (key, width, height) in SHIPPED_MAP_SIZES {
        let grid = UVec2::new(width, height);
        let with_pin = |pin: Option<u32>| {
            let mut catalog = shipped_catalog();
            catalog[0].default_count = pin;
            archetype_capacities(&catalog, grid, land_fraction)[0]
        };
        let derived = with_pin(None);
        let pinned = with_pin(Some(PINNED));
        assert_eq!(
            pinned.default_count,
            PINNED.min(archetype_ceiling(pinned.site_count, 0)),
            "{key}: a pinned count is offered as-is, clamped only by the ceiling"
        );
        let zeroed = with_pin(Some(0));
        assert_eq!(
            zeroed.default_count, 0,
            "{key}: an explicit zero pre-selects nobody"
        );
        if derived.default_count > 0 {
            assert_ne!(
                zeroed.default_count, derived.default_count,
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
fn an_unattended_boot_takes_none_while_the_screen_offers_some() {
    let catalog = shipped_catalog();
    let unattended = unattended_archetype_counts(&catalog);
    assert_eq!(
        unattended,
        vec![0; catalog.len()],
        "the shipped profile leaves every archetype's unattended count at none"
    );

    let standard = UVec2::new(80, 52);
    let offered =
        archetype_capacities(&catalog, standard, shipped_land_fraction())[0].default_count;
    assert!(
        offered > unattended[0],
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
