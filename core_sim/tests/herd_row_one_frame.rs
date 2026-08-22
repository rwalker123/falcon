//! **A HERD ROW DESCRIBES ONE TURN.**
//!
//! `herd_snapshot_entries` fills each row from *two* sources — the display `HerdTelemetry` entry,
//! written in Logistics, and the live `Herd` the registry resolves beside it. Everything that moves
//! after the Logistics write (the rest of that stage, then all of Population) is a turn newer in the
//! registry than in the entry, so any field taken from the entry ships a turn behind the fields
//! taken live. A row that mixes frames is worse than a row that is uniformly late: the player can
//! see the contradiction.
//!
//! `.claude/rules/core_sim/husbandry.md` → "A herd row is assembled from TWO frames" owns the
//! provenance table. This file pins the two fields whose staleness had *consequences* beyond the
//! word on the row:
//!
//! - **`biomass`** is one half of the escapement ceiling the client composes as
//!   `max(0, B − floor·K) × rate`; `carryingCapacity` beside it is live, so a stale `B` quoted every
//!   yield preview from two different turns.
//! - **the heading** (`nextX`/`nextY`) is drawn on the map as a migration arrow, and `corral_at`
//!   clears `next_pos` in Population — so a herd penned this turn pointed at the roam its pen had
//!   just ended.
//!
//! Both fixtures resolve their turn in **stage order**, which is the only arrangement in which the
//! two frames can disagree at all. The snapshot unit tests fabricate the telemetry from the registry
//! in the same instant and are structurally blind to this whole class of defect.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;

use core_sim::{
    advance_herds, advance_husbandry, build_test_app, recapture_snapshot_in_place,
    FaunaConfigHandle, HerdRegistry, HerdTelemetry, SimulationConfig, SnapshotHistory,
};

/// **The species every fixture here reshapes its herd into** — a `pen`-ceiling row, so `corral_at`
/// is allowed to pen it and `tame_outright` is allowed to tame it. A `wild`-ceiling herd (deer,
/// mammoth) refuses both, and the fixture would then be asserting about a herd that never moved rung.
const PENNABLE_SPECIES: &str = "Wild Boar";

/// **The wire's "no heading" sentinel** — what `nextX`/`nextY` carry when the herd is not on a
/// migration leg, and what the client already renders as *no arrow*.
const NO_HEADING: i32 = -1;

/// A fully-unfed pen: its keeper brought nothing at all, so `starve_underfed_pen` shrinks the flock
/// by the whole `starve_shrink_rate`. The largest, least ambiguous post-telemetry move available
/// without standing up a labor fixture.
const KEEPER_PAID_NOTHING: f32 = 0.0;

/// A headless world whose one game herd has been reshaped into [`PENNABLE_SPECIES`], tamed to the
/// viewer, and made visible. Returns the app, the herd's id and the tile it stands on.
///
/// **Tamed to the VIEWER on purpose**: `HerdSnapshotInputs::herd_is_visible` passes an owned herd
/// unconditionally, so neither the row nor its heading tile can be dropped by fog and turn a
/// "published nothing" assertion into a pass for the wrong reason.
fn world_with_a_tame_pennable_herd() -> (App, String, UVec2) {
    let mut app = build_test_app();
    app.update();
    let id = {
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .herds
            .iter()
            .find(|herd| herd.id.starts_with("game_"))
            .map(|herd| herd.id.clone())
            .expect("the map seeded short-range game")
    };
    let species = app
        .world
        .resource::<FaunaConfigHandle>()
        .get()
        .species_by_display(PENNABLE_SPECIES)
        .expect("the shipped roster carries the fixture species")
        .clone();
    let viewer = app.world.resource::<core_sim::ViewerFaction>().0;
    let pos = {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|herd| herd.id == id)
            .expect("the herd the id came from");
        herd.species = PENNABLE_SPECIES.to_string();
        herd.body_mass = species.body_mass;
        herd.husbandry_ceiling = species.husbandry_ceiling;
        assert!(
            herd.tame_outright(viewer, &core_sim::LadderConfig::builtin()),
            "the fixture species must actually tame"
        );
        herd.position()
    };
    {
        let grid = app.world.resource::<SimulationConfig>().grid_size;
        let mut ledger = app.world.resource_mut::<core_sim::VisibilityLedger>();
        ledger
            .ensure_faction(viewer, grid.x, grid.y)
            .mark_active(pos.x, pos.y, 0);
    }
    (app, id, pos)
}

fn recapture(app: &mut App) -> sim_runtime::WorldSnapshot {
    app.world.run_system_once(recapture_snapshot_in_place);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .last_snapshot()
        .expect("a capture");
    (*snapshot).clone()
}

fn published_row(app: &mut App, id: &str) -> sim_schema::HerdTelemetryState {
    recapture(app)
        .herds
        .iter()
        .find(|herd| herd.id == id)
        .expect("the fixture herd is on the wire")
        .clone()
}

fn live_biomass(app: &App, id: &str) -> f32 {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .expect("the fixture herd is in the registry")
        .biomass
}

/// **THE PUBLISHED STOCK IS THE STOCK THE TURN ENDED ON.**
///
/// Two writers land after the last `HerdTelemetry` write: `advance_husbandry`'s shed/starve shrink,
/// later in the same Logistics stage, and the hunt take in `advance_labor_allocation`, in Population.
/// The row took `biomass` from the display entry, so it published the reading from *before* either —
/// while `carryingCapacity` next to it came off the live herd. The client composes the escapement
/// ceiling as `max(0, B − floor·K) × rate` from exactly that pair, so the yield preview was assembled
/// from two turns on every herd, every turn.
///
/// The fixture drives the **starve** writer, which is a real system in its real slot: pen the herd,
/// resolve Logistics' telemetry write, then leave the keeper unable to pay and run `advance_husbandry`.
#[test]
fn a_herds_published_biomass_is_the_stock_the_turns_last_writer_left() {
    let (mut app, id, pos) = world_with_a_tame_pennable_herd();
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|herd| herd.id == id)
            .expect("the fixture herd");
        assert!(
            herd.corral_at(pos, &core_sim::LadderConfig::builtin()),
            "the fixture species must actually pen"
        );
    }
    // **Logistics, part one.** The pass that ends by rebuilding the display telemetry every row is
    // walked from.
    app.world.run_system_once(advance_herds);
    let published_by_logistics = app
        .world
        .resource::<HerdTelemetry>()
        .entries
        .iter()
        .find(|entry| entry.id == id)
        .expect("the fixture herd is in the display telemetry")
        .biomass;

    // **Logistics, part two — after the telemetry write.** The keeper brings nothing, so the pen
    // starves and the flock shrinks.
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        registry
            .herds
            .iter_mut()
            .find(|herd| herd.id == id)
            .expect("the fixture herd")
            .pen_fed_fraction = KEEPER_PAID_NOTHING;
    }
    app.world.run_system_once(advance_husbandry);

    let after_the_last_writer = live_biomass(&app, &id);
    assert!(
        after_the_last_writer < published_by_logistics,
        "the fixture must actually move the stock after the telemetry write, or it pins nothing: \
         {published_by_logistics} → {after_the_last_writer}"
    );

    let row = published_row(&mut app, &id);
    assert_eq!(
        row.biomass, after_the_last_writer,
        "the row must publish the stock the turn ended on, not the display entry's earlier reading \
         of {published_by_logistics}"
    );
}

/// **A HERD PENNED THIS TURN HAS NO HEADING, AND MUST NOT PUBLISH ONE.**
///
/// `Herd::corral_at` clears `next_pos` — the pen ends the roam — but it runs in Population, after the
/// display entry was written, so the row kept publishing the leg the herd was on when Logistics last
/// looked. The map drew a migration arrow on an animal that cannot move.
///
/// Asserted **on the completing turn specifically**, with the arrow first shown to reach the wire at
/// all: a "publishes no heading" assertion passes for free against a herd that never had one.
#[test]
fn a_herd_penned_this_turn_publishes_no_heading() {
    let (mut app, id, pos) = world_with_a_tame_pennable_herd();
    let heading = a_neighbouring_tile(&app, pos);

    // **The Logistics frame**: the herd is on a leg, and the display telemetry says so. Written the
    // way `advance_herds` ends its own pass, through `HerdRegistry::snapshot_entries`.
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        registry
            .herds
            .iter_mut()
            .find(|herd| herd.id == id)
            .expect("the fixture herd")
            .next_pos = Some(heading);
    }
    let entries = app.world.resource::<HerdRegistry>().snapshot_entries();
    app.world.resource_mut::<HerdTelemetry>().entries = entries;

    let roaming = published_row(&mut app, &id);
    assert_eq!(
        (roaming.next_x, roaming.next_y),
        (heading.x as i32, heading.y as i32),
        "the fixture's heading must reach the wire, or the assertion below is vacuous"
    );

    // **Population**: the keeper's Corral completes and the roam is over.
    {
        let mut registry = app.world.resource_mut::<HerdRegistry>();
        let herd = registry
            .herds
            .iter_mut()
            .find(|herd| herd.id == id)
            .expect("the fixture herd");
        assert!(
            herd.corral_at(pos, &core_sim::LadderConfig::builtin()),
            "the fixture species must actually pen"
        );
    }

    let penned = published_row(&mut app, &id);
    assert!(
        penned.corralled,
        "the pen must reach the wire in the same frame the heading is judged in"
    );
    assert_eq!(
        (penned.next_x, penned.next_y),
        (NO_HEADING, NO_HEADING),
        "a herd penned this turn cannot move, so its row must carry no heading — it carried \
         ({}, {})",
        penned.next_x,
        penned.next_y
    );
}

/// An in-bounds hex beside `pos` for the herd to be heading toward. Which side it is on does not
/// matter; that it is a *second, real* tile does, because the heading is fog-filtered on its own
/// terms.
fn a_neighbouring_tile(app: &App, pos: UVec2) -> UVec2 {
    let width = app.world.resource::<SimulationConfig>().grid_size.x.max(1);
    let x = if pos.x + 1 < width {
        pos.x + 1
    } else {
        pos.x.saturating_sub(1)
    };
    UVec2::new(x, pos.y)
}
