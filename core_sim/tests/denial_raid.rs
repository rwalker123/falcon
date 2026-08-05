//! **The denial raid** (`docs/plan_denial_raid.md` slice 1) — the mission that engages hard and does
//! not clamp to carry.
//!
//! One line of behaviour separates it from a hunt: a hunting party stops engaging once its pack is
//! full, a denial party never stops (`fauna::EngagementStop`). Everything else — `ExpeditionPhase`,
//! outfitting, travel, the `Hunting` cycle, `AnimalTake` — is the hunt's, unchanged. What that one
//! line buys is the thing `floor = 0` could never buy: a party that kills what it has no intention
//! of using, and therefore erases a herd at the pace of the *fight* rather than the pace of the pack.
//!
//! Success is the **point of no return**, not zero: below `ecology.collapse_fraction` the growth flow
//! is zeroed and the herd declines irreversibly with the party gone, which is why a small party can
//! erase a large placid herd and why ordinary hunting never does it by accident.

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;

use core_sim::{
    advance_expeditions, advance_herds, advance_tick, build_headless_app, denial_forecast,
    herd_capacity, herd_ecology, herd_hunt_yield, recapture_snapshot_in_place, scalar_from_f32,
    scalar_one, scalar_zero, BandEquipment, CombatConfigHandle, CommandEventLog, DenialOutcome,
    EquipmentConfigHandle, Expedition, ExpeditionConfig, ExpeditionConfigHandle, ExpeditionMission,
    ExpeditionPhase, FactionId, FaunaConfigHandle, GenerationId, HerdRegistry, HerdTelemetry,
    HuntingParty, LaborAllocation, LaborConfigHandle, LocalStore, MoraleCause, PopulationCohort,
    ResidentBand, SimulationConfig, SnapshotHistory, StartingUnit, TileRegistry, VisibilityLedger,
    FOOD, NO_FILL_TARGET, STRIP_IT_BARE,
};

/// The reference denial party — four people, the same crew every raid fixture in
/// `expedition_hunt.rs` uses, so the two files' numbers are comparable.
const PARTY_WORKERS: u32 = 4;

/// A party too small to outpace a herd's regrowth — the "this cannot work" side of the wariness
/// fixture. One person.
const LONE_HUNTER: u32 = 1;

/// **A quarry that cannot fight back** (`Rabbit Warren`, `combat.attack 0`). A denial raid is by
/// construction the longest engagement in the game, so a quarry with `ferocity` would shrink the
/// party turn after turn and these fixtures would end up measuring attrition rather than the raid.
/// `expedition_hunt::the_raid_forecast_matches_a_real_party_run` retags for the same reason.
const HARMLESS_QUARRY: &str = "Rabbit Warren";

/// The herd every fixture in this file raids, stated in full so each test says what it is measuring
/// against rather than inheriting the roster's numbers by accident.
#[derive(Clone, Copy)]
struct RaidQuarry {
    /// One animal's biomass. Heavy against the party's pack is what makes `wasted` the headline.
    body_mass: f32,
    /// The herd's `K`. Its `collapse_fraction` share is the line the raid exists to cross.
    carrying_capacity: f32,
    /// Standing stock as a fraction of `K`.
    biomass_fraction: f32,
    /// The herd's wild `r`. Set explicitly because the whole question a denial raid asks is whether
    /// the party's kills outpace this.
    regrowth_rate: f32,
}

/// **The placid herd of the mechanic test** — a full stock of heavy animals breeding at an ordinary
/// big-game rate (the roster's `deer`/`boar` `0.10`). A four-hunter party takes several turns to push
/// it past recovery, so the fixture exercises the multi-turn grind that makes denial a campaign
/// rather than a one-turn erasure.
const PLACID_HERD: RaidQuarry = RaidQuarry {
    body_mass: 10.0,
    carrying_capacity: 800.0,
    biomass_fraction: 1.0,
    regrowth_rate: 0.10,
};

/// **The herd the wariness fixture cannot be pushed past** — the same animals, seated at a quarter of
/// `K` so it is genuinely growing back under the raid. That is the shape "the party cannot outpace
/// regrowth" actually takes; measuring it on a full herd would measure the logistic curve's zero at
/// `K` instead.
const RESISTING_HERD: RaidQuarry = RaidQuarry {
    biomass_fraction: 0.25,
    // A fast breeder (the roster's `crag_goat`/`gazelle` band), because that is what a party has to
    // outpace. On the ordinary big-game rate a lone hunter *does* eventually win, which is the
    // correct answer and the wrong fixture for a test about being repelled.
    regrowth_rate: 0.25,
    ..PLACID_HERD
};

/// **The herd the reported range is measured on** — lighter animals, so a party engages *many* of
/// them and the retreat's binomial has a shape rather than a handful of all-or-nothing draws.
///
/// That is §4.7's own property ("variance shrinks as the force grows") used deliberately: on a
/// heavy-bodied herd a four-hunter party engages so few animals that the pessimistic quantile floors
/// to zero kills and the reported window has no upper end at all — an honest reading, and one a
/// containment assertion cannot be written against.
const BANDED_HERD: RaidQuarry = RaidQuarry {
    // Light enough that a party engages many animals at once — but the whole herd is still well
    // inside one hunting kit's `starting_durability / wear_per_kill`, so the range being measured is
    // the retreat's and not the spears running dry mid-raid (which the forecast does not model).
    body_mass: 1.0,
    carrying_capacity: 200.0,
    biomass_fraction: 1.0,
    // A slow breeder (the roster's megafauna rate), so **both** ends of the window reach the line
    // and the reported band has two numbers. A faster herd repels the pessimistic quantile — which
    // is honest, and leaves nothing to assert containment against.
    regrowth_rate: 0.04,
};

/// **The quarry of the tiny-`K` fixture, named rather than retagged** — `Crag Goats`
/// (`wariness 0.60`, `engage_rate 1.5`, `durability 12`, `body_mass 6`). The defect it pins lives in
/// the *retreat*, which is resolved off the species' display name, and it only appears where
/// `engaged × (1 − wariness)` falls **below one animal** — so the species' own numbers are the
/// fixture and [`HARMLESS_QUARRY`]'s cannot stand in for them.
///
/// It is still safe to measure a raid on: a goat's `attack × ferocity` is `0.12`, under a person's
/// `defense 1`, so the gate zeroes it and the party takes **no fatalities** however long the raid
/// runs (the baseline hazard is `wounded`-only). The head count is constant, so this measures the
/// raid rather than attrition — the same property [`HARMLESS_QUARRY`] is chosen for.
const WARY_TINY_QUARRY: &str = "Crag Goats";

/// **The reported herd: three animals on barren range, so `K` IS the herd.** Not a scaled-down
/// version of the fixtures above — the tiny-`K` regime is its own thing, and it is where the defect
/// reached play instead of a test: at three animals a party's whole engagement is the herd, the
/// retreat's expectation falls under one animal, and `collapse_fraction × K` (`2.7`) is under **half
/// a body**, so reaching the line means killing essentially every goat.
const BARREN_RANGE_HERD: RaidQuarry = RaidQuarry {
    // Crag Goats' own body — three of them make the whole 18-biomass stock.
    body_mass: 6.0,
    carrying_capacity: 18.0,
    biomass_fraction: 1.0,
    // The species' own `r`. Near `K` this is under one biomass a turn — a sixth of a goat — which is
    // the arithmetic that makes "breeds back faster than this party kills" the wrong verdict.
    regrowth_rate: 0.22,
};

/// The party of the reported case — eight hunters, enough to engage every goat the herd holds every
/// turn (`8 × engage_rate 1.5 = 12`, capped by the three that exist).
const REPORTED_PARTY_WORKERS: u32 = 8;

/// **How long a raid on [`BARREN_RANGE_HERD`] may take before it is stalled rather than grinding**,
/// as a divisor of `hunt.forecast_horizon_turns`. A party that reaches every animal in the herd every
/// turn and kills roughly one goat every other turn cannot need a quarter of the projection's whole
/// length; anything past that is the projection failing to make progress, which is the shape of the
/// defect. Expressed against the horizon rather than as a turn count so it tracks the one lever that
/// sets the projection's length.
const STALLED_HORIZON_DIVISOR: u32 = 4;

/// **The whole crew** for the range fixture, because the reported band is a property of force size:
/// the retreat is binomial, so a big party's draw is tight around its mean and a small party's is
/// all-or-nothing (`docs/plan_hunt_through_combat.md` §4.7). A four-hunter party on this herd has a
/// pessimistic quantile that floors to zero kills, and no window at all.
const BANDED_PARTY_WORKERS: u32 = 8;

// ---------------------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------------------

/// [`build_headless_app`] with the roster's authored `combat.wariness` held at `0`
/// (`docs/plan_hunt_through_combat.md` §6.1) — the deterministic net every pin below but the
/// wariness fixtures runs on. The stochastic surface lives in the two tests that are *about* it.
fn placid_world() -> App {
    let mut app = build_headless_app();
    app.world
        .resource_mut::<FaunaConfigHandle>()
        .hold_wariness_at_zero();
    app.update();
    app
}

/// The shipped roster with its authored wariness intact — the world the retreat stage is real in.
fn wary_world() -> App {
    let mut app = build_headless_app();
    app.update();
    app
}

/// Pin a herd to one tile and give it the fixture's own species, body and stock, returning its id.
///
/// The species is retagged rather than searched for because the dials the raid turns on
/// (`engage_rate`, `combat`, `wariness`) are resolved off the **display name**, while `body_mass`
/// and `carrying_capacity` live on the herd — so this is the one place a fixture states all four.
fn pin_raid_herd(app: &mut App, quarry: RaidQuarry) -> (String, UVec2) {
    pin_raid_herd_of(app, HARMLESS_QUARRY, quarry)
}

/// [`pin_raid_herd`] for a **named** species — a fixture about the *retreat* has to say which animal
/// it means, because `wariness` and `engage_rate` are resolved off the display name and no amount of
/// per-herd tuning reaches them.
fn pin_raid_herd_of(app: &mut App, species: &str, quarry: RaidQuarry) -> (String, UVec2) {
    let id = {
        let registry = app.world.resource::<HerdRegistry>();
        registry
            .herds
            .iter()
            .find(|h| h.id.starts_with("game_") && h.route_length() == 1)
            .map(|h| h.id.clone())
            .expect("the campaign map seeds at least one stationary game group")
    };
    let mut registry = app.world.resource_mut::<HerdRegistry>();
    let herd = registry.herds.iter_mut().find(|h| h.id == id).unwrap();
    // **The id is pinned too, and it is load-bearing for the seeded fixtures.** The live retreat
    // draws from `fauna::retreat_seed(map_seed, tick, herd_id, workers)`, and the map seed the
    // campaign generates under is entropy by default — so the *herd's own id* varies run to run and
    // a "same seed, same outcome" fixture would not be one. Naming it here makes the draw a pure
    // function of what the test states.
    herd.id = PINNED_HERD_ID.to_string();
    herd.route = vec![herd.current_pos];
    herd.step_index = 0;
    herd.species = species.to_string();
    herd.body_mass = quarry.body_mass;
    herd.carrying_capacity = quarry.carrying_capacity;
    herd.biomass = quarry.carrying_capacity * quarry.biomass_fraction;
    herd.regrowth_rate = quarry.regrowth_rate;
    // **The herd draws nothing from the pasture layer**, which is what holds its `K` at the number
    // above. `advance_herds` otherwise recomputes `carrying_capacity` from the graze its range yields
    // every turn (`ecological_carrying_capacity`, which returns `None` for a herd with no fodder
    // demand) — so without this the fixtures would be measuring the graze loop rather than the raid,
    // and `forecast == actual` would fail on a `K` the projection resolves once and the sim moves.
    // `expedition_hunt`'s pure-forecast fixtures build their herds outside the ECS for the same
    // reason.
    herd.fodder_per_biomass = 0.0;
    let pos = herd.position();
    // **The display telemetry is DERIVED from the registry** — `advance_herds` rebuilds it at the end
    // of every turn — so a fixture that edits the registry and then captures a snapshot without
    // driving a turn has to rebuild it, or the wire carries the herd as the map generated it.
    let entries = app.world.resource::<HerdRegistry>().snapshot_entries();
    app.world.resource_mut::<HerdTelemetry>().entries = entries;
    (PINNED_HERD_ID.to_string(), pos)
}

/// The id every fixture's quarry is renamed to — see [`pin_raid_herd`] for why the *name* has to be
/// pinned and not merely the numbers.
const PINNED_HERD_ID: &str = "denial_fixture_herd";

/// The herd's `collapse_fraction × K` — the line a denial raid exists to cross.
fn point_of_no_return(app: &App, id: &str) -> f32 {
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry.find(id).expect("herd present");
    herd_ecology(herd, &fauna).collapse_fraction * herd_capacity(herd, &fauna)
}

fn herd_biomass(app: &App, id: &str) -> Option<f32> {
    app.world
        .resource::<HerdRegistry>()
        .find(id)
        .map(|herd| herd.biomass)
}

fn tile_at(app: &App, pos: UVec2) -> bevy::prelude::Entity {
    app.world
        .resource::<TileRegistry>()
        .index(pos.x, pos.y)
        .expect("tile resolves")
}

fn cohort(tile: bevy::prelude::Entity, working: u32) -> PopulationCohort {
    PopulationCohort {
        home: tile,
        current_tile: tile,
        size: 30,
        children: scalar_zero(),
        working: scalar_from_f32(working as f32),
        elders: scalar_zero(),
        stores: LocalStore::new(),
        morale: scalar_one(),
        last_food_consumption: 0.0,
        last_morale_delta: scalar_zero(),
        last_morale_cause: MoraleCause::None,
        last_morale_contributions: Default::default(),
        last_fertility_factors: Default::default(),
        discontent_fraction: scalar_zero(),
        grievance: scalar_zero(),
        last_emigrated: 0,
        last_immigrated: 0,
        age_turns: 0,
        generation: 0 as GenerationId,
        faction: FactionId(0),
        knowledge: Vec::new(),
        migration: None,
    }
}

/// A home band far from the herd, so no near-band drop-off interferes with the raid's own cycle.
fn spawn_home_band(app: &mut App, herd_pos: UVec2) -> bevy::prelude::Entity {
    let (width, height) = {
        let registry = app.world.resource::<TileRegistry>();
        (registry.width, registry.height)
    };
    let far = UVec2::new(
        (herd_pos.x + width / 3) % width,
        (herd_pos.y + height / 3) % height,
    );
    let tile = tile_at(app, far);
    app.world.spawn((cohort(tile, 20), ResidentBand)).id()
}

/// A party already in the `Hunting` phase on `mission`, positioned on the herd's tile — the state
/// `send_denial_raid` / `send_hunt_expedition` spawn into once the walk is done.
fn spawn_party(
    app: &mut App,
    home_band: bevy::prelude::Entity,
    pos: UVec2,
    workers: u32,
    mission: ExpeditionMission,
) -> bevy::prelude::Entity {
    let tile = tile_at(app, pos);
    app.world
        .spawn((
            cohort(tile, workers),
            LaborAllocation::default(),
            // **The party leaves outfitted, and wears its own kit** — `BandEquipment::default()` is
            // zero wear (`docs/plan_denial_raid.md` §1.2). Carried explicitly rather than left to
            // `advance_expeditions`' absent-component default, because these fixtures read the wear
            // back off it.
            BandEquipment::default(),
            StartingUnit::new("expedition".to_string(), Vec::new()),
            Expedition {
                home_band,
                mission,
                phase: ExpeditionPhase::Hunting,
                announced: false,
                pending_reveal: Vec::new(),
                carried_trade: 0.0,
                kit: core_sim::EquipmentConfig::builtin().default_kit(core_sim::KitJob::Hunt),
            },
        ))
        .id()
}

fn deny(fauna_id: &str) -> ExpeditionMission {
    ExpeditionMission::Deny {
        fauna_id: fauna_id.to_string(),
    }
}

fn hunt(fauna_id: &str, floor: f32) -> ExpeditionMission {
    ExpeditionMission::Hunt {
        fauna_id: fauna_id.to_string(),
        floor,
        fill_target: NO_FILL_TARGET,
    }
}

fn phase(app: &App, party: bevy::prelude::Entity) -> Option<ExpeditionPhase> {
    app.world.get::<Expedition>(party).map(|e| e.phase)
}

fn carried_food(app: &App, party: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(party)
        .map(|c| c.stores.get(FOOD).to_f32())
        .unwrap_or(0.0)
}

/// One sim turn as the raid sees it: the herd's ecology (Logistics), then the party's take
/// (Population) — the same pair, in the same order, that both forecasts simulate — and then the
/// tick, which `run_turn` advances in the Snapshot stage at the end of every turn.
///
/// **Advancing the tick is load-bearing on a WARY herd, not bookkeeping.** The live retreat draws
/// from `fauna::retreat_seed(map_seed, tick, herd_id, workers)`, so a harness that leaves the tick
/// frozen makes every turn re-draw the **same sample**: a party that lost the first roll loses it
/// again for as many turns as the fixture runs. That is invisible on a big engagement (dozens of
/// animals average out whatever the seed is) and total on a small one — on a three-animal herd the
/// raid either erased it immediately or never touched it, purely by the map seed.
fn drive_turn(app: &mut App) {
    app.world.run_system_once(advance_herds);
    app.world.run_system_once(advance_expeditions);
    app.world.run_system_once(advance_tick);
}

fn expedition_cfg(app: &App) -> std::sync::Arc<ExpeditionConfig> {
    app.world.resource::<ExpeditionConfigHandle>().get()
}

/// The equipped hunter tier every forecast in this file is quoted for — the tier a party leaves at.
fn party_profile() -> HuntingParty {
    HuntingParty::builtin_equipped()
}

fn forecast(app: &App, id: &str, workers: u32) -> core_sim::DenialForecast {
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let cfg = expedition_cfg(app);
    let sigmas = app
        .world
        .resource::<CombatConfigHandle>()
        .get()
        .forecast_range_sigmas;
    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry.find(id).expect("herd present");
    denial_forecast(
        workers,
        herd,
        &fauna,
        labor.hunt.per_worker_biomass_capacity,
        &cfg,
        &party_profile(),
        sigmas,
    )
}

// ---------------------------------------------------------------------------------------------------
// THE MECHANIC — one test
// ---------------------------------------------------------------------------------------------------

/// **A denial raid reaches collapse where a hunt does not** (`docs/plan_denial_raid.md` §5).
///
/// A placid herd raided by a given party crosses `collapse_fraction` in bounded turns and then
/// declines with **no further pressure on it**; the *same* herd worked by the *same* party as a hunt,
/// at any floor above `collapse_fraction`, never does — however long it is hunted.
///
/// **This is the whole mechanic.** The hunting half is deliberately given every advantage the
/// denial half is not: its trip is restarted the moment it ends and its pack is emptied, so it is an
/// unbounded series of raids rather than one, and it *still* cannot cross the line. That is the point
/// — the floor is what stops it, not the party's patience.
#[test]
fn a_denial_raid_reaches_collapse_where_a_hunt_does_not() {
    // --- denial ---------------------------------------------------------------------------------
    let mut app = placid_world();
    let (id, herd_pos) = pin_raid_herd(&mut app, PLACID_HERD);
    let line = point_of_no_return(&app, &id);
    let home = spawn_home_band(&mut app, herd_pos);
    let party = spawn_party(&mut app, home, herd_pos, PARTY_WORKERS, deny(&id));

    let horizon = expedition_cfg(&app).hunt.forecast_horizon_turns;
    let mut crossed_on = None;
    for turn in 1..=horizon {
        drive_turn(&mut app);
        if herd_biomass(&app, &id).is_none_or(|biomass| biomass < line) {
            crossed_on = Some(turn);
            break;
        }
    }
    let crossed_on = crossed_on.unwrap_or_else(|| {
        panic!(
            "a denial raid must push the herd past recovery ({line} biomass) inside {horizon} \
             turns; it stood at {:?}",
            herd_biomass(&app, &id)
        )
    });
    assert_ne!(
        phase(&app, party),
        Some(ExpeditionPhase::Hunting),
        "the raid completes when the herd goes past recovery — the party walks away rather than \
         staying to kill every animal"
    );

    // ...and it stays down with nobody working it. Despawn the party outright so there is no
    // pressure of any kind, and let the ecology run.
    let biomass_at_crossing = herd_biomass(&app, &id);
    app.world.despawn(party);
    for _ in 0..horizon {
        app.world.run_system_once(advance_herds);
    }
    match (biomass_at_crossing, herd_biomass(&app, &id)) {
        // The irreversible decline ran all the way to the extinction floor and `advance_herds`
        // despawned the herd — the strongest form of "it did not recover".
        (Some(_), None) => {}
        (Some(before), Some(after)) => assert!(
            after < before,
            "past the point of no return the herd must DECLINE with no further pressure \
             ({before} -> {after}); depensation is what makes the raid's walk-away work"
        ),
        (None, _) => {}
    }

    // --- the same herd, the same party, hunted ---------------------------------------------------
    // Every floor strictly above the collapse line. A hunt cannot cross it at any of them, because
    // the escapement ceiling is `max(0, B − floor·K)` and the party stops there.
    let fauna_collapse_fraction = {
        let app = placid_world();
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        fauna.ecology.collapse_fraction
    };
    for floor in [0.2_f32, 0.3, 0.5] {
        assert!(
            floor > fauna_collapse_fraction,
            "this sweep is about floors ABOVE the collapse line"
        );
        let mut app = placid_world();
        let (id, herd_pos) = pin_raid_herd(&mut app, PLACID_HERD);
        let line = point_of_no_return(&app, &id);
        let opening_biomass = herd_biomass(&app, &id).expect("herd present");
        let home = spawn_home_band(&mut app, herd_pos);
        let party = spawn_party(&mut app, home, herd_pos, PARTY_WORKERS, hunt(&id, floor));

        let mut deepest = opening_biomass;
        for _ in 1..=horizon {
            drive_turn(&mut app);
            let Some(biomass) = herd_biomass(&app, &id) else {
                panic!("floor {floor}: a hunt above the collapse line must never lose the herd");
            };
            deepest = deepest.min(biomass);
            assert!(
                biomass >= line,
                "floor {floor}: a hunt must never take the herd past recovery — {biomass} < {line}"
            );
            // Restart the trip the moment it ends, with an empty pack: an unbounded SERIES of raids,
            // so the assertion above is about the floor and not about the party's patience.
            if let Some(mut expedition) = app.world.get_mut::<Expedition>(party) {
                expedition.phase = ExpeditionPhase::Hunting;
            }
            if let Some(mut cohort) = app.world.get_mut::<PopulationCohort>(party) {
                let carried = cohort.stores.get(FOOD);
                cohort.stores.take(FOOD, carried);
            }
        }
        // **Liveness.** A hunt that took nothing would also "never cross the line", and would pass
        // the ordering assertion above while asserting nothing at all.
        assert!(
            deepest < opening_biomass,
            "floor {floor}: the hunt must actually be drawing the herd down ({opening_biomass} -> \
             {deepest}), or the never-crosses assertion is vacuous"
        );
    }

    // The two halves compared: denial crossed, and it did so inside the horizon.
    assert!(
        crossed_on <= horizon,
        "the raid crossed on turn {crossed_on}, within the {horizon}-turn horizon"
    );
}

// ---------------------------------------------------------------------------------------------------
// A wary herd resists denial — and the forecast SAYS SO
// ---------------------------------------------------------------------------------------------------

/// **A wary herd resists denial, and the forecast names the reason** (`docs/plan_denial_raid.md` §5,
/// §3): *"when the party cannot get there at all, it must say **that**, not show a blank."*
///
/// Three assertions, and the second two are the liveness the first needs:
/// 1. a lone hunter against a wary, fast-breeding herd is told [`DenialOutcome::Repelled`] with no
///    turn count — a verdict about the party, never a silent `None`;
/// 2. the **same** herd and the **same** party with the retreat held at `0` kills materially more,
///    which is the direct statement that *wariness* is what repelled it;
/// 3. a bigger party on the wary herd **does** get there, so the `Repelled` reading is not simply
///    denial being broken on this fixture.
#[test]
fn a_wary_herd_resists_denial_and_the_forecast_says_so() {
    let mut wary = wary_world();
    // Seated below its own equilibrium so the herd is genuinely growing back under the raid — the
    // shape "the party cannot outpace regrowth" actually takes.
    let (id, _) = pin_raid_herd(&mut wary, RESISTING_HERD);

    let repelled = forecast(&wary, &id, LONE_HUNTER);
    assert_eq!(
        repelled.outcome,
        DenialOutcome::Repelled,
        "a lone hunter cannot outpace a wary, fast-breeding herd — the forecast must say so"
    );
    assert_eq!(
        repelled.turns_to_collapse, None,
        "a repelled raid has no collapse turn; the OUTCOME is what the readout shows instead"
    );
    assert_eq!(
        repelled.turns_to_collapse_low, None,
        "not even the optimistic draw gets a repelled party there"
    );

    // 2. The same herd and the same lone hunter with the retreat neutralised — the mechanism pin.
    //    Wariness is the ONLY thing that differs between the two worlds, and it is the difference
    //    between a raid that works and a raid that cannot. (Cumulative kills are deliberately NOT
    //    the comparison: a repelled raid runs the whole horizon and therefore kills *more* in total
    //    than a successful one that finishes in four turns.)
    let mut placid = placid_world();
    let (placid_id, _) = pin_raid_herd(&mut placid, RESISTING_HERD);
    let unresisted = forecast(&placid, &placid_id, LONE_HUNTER);
    assert!(
        unresisted.outcome.succeeded() && unresisted.turns_to_collapse.is_some(),
        "the retreat is what resists: the same lone hunter on the same herd drives it past recovery \
         when nothing breaks off ({:?})",
        unresisted.outcome
    );

    // 3. Liveness: a bigger party on the SAME wary herd does get there, so `Repelled` is a statement
    //    about the party rather than about the fixture.
    let big_enough = expedition_cfg(&wary).estimate_party_sizes;
    let overwhelming = forecast(&wary, &id, big_enough);
    assert!(
        overwhelming.outcome.succeeded() && overwhelming.turns_to_collapse.is_some(),
        "a full party must still drive the wary herd past recovery ({:?}) — otherwise the repelled \
         reading above is denial being broken, not the herd resisting",
        overwhelming.outcome
    );
}

/// **A TINY herd is not a small version of a big one, and the forecast used to break only there.**
/// The reported case: eight hunters on three Crag Goats standing on barren range, told *"Crag Goats
/// breeds back faster than this party kills — it is never pushed past recovery"* while a driven raid
/// erased the herd in two turns.
///
/// The mechanism was the fight's cross-turn damage bank. A projection cannot draw the retreat, so it
/// reads the binomial's **mean** — `3 engaged × (1 − 0.60) = 1.2` animals, and `0.8` once the herd is
/// down to two — and `combat::DamageLedger::strike` clamped its running bank to
/// `standing × durability`. Below one standing animal that ceiling sits under one body's durability,
/// so the projection banked `0.8 × 12` damage and threw it away every turn, for sixty turns, and
/// reported a party that reached every animal in the herd as **repelled by its regrowth**.
///
/// Three assertions, and the third is what stops the fix from being "delete the verdict":
/// 1. the reported case reaches [`DenialOutcome::PastRecovery`] promptly, not at the horizon;
/// 2. a **driven** raid on the same herd in the same world also drives it past recovery — the
///    forecast is pinned to the sim, never the reverse;
/// 3. a party that genuinely cannot outpace a herd's regrowth is **still** told
///    [`DenialOutcome::Repelled`].
#[test]
fn a_tiny_wary_herd_is_erased_and_the_forecast_no_longer_calls_it_repelled() {
    // 1. The forecast on the reported fixture.
    let mut app = wary_world();
    let (id, herd_pos) = pin_raid_herd_of(&mut app, WARY_TINY_QUARRY, BARREN_RANGE_HERD);
    let line = point_of_no_return(&app, &id);
    assert!(
        line < BARREN_RANGE_HERD.body_mass,
        "the fixture must be in the regime that reached play: the collapse line ({line}) is under \
         ONE animal, so crossing it means killing essentially the whole herd"
    );

    let horizon = expedition_cfg(&app).hunt.forecast_horizon_turns;
    let stalled_after = horizon / STALLED_HORIZON_DIVISOR;
    let reported = forecast(&app, &id, REPORTED_PARTY_WORKERS);
    assert!(
        reported.outcome.succeeded(),
        "eight hunters reach every goat this range holds — the verdict must not be {:?}",
        reported.outcome
    );
    let turns = reported
        .turns_to_collapse
        .expect("a succeeding outcome names the turn the party comes home");
    assert!(
        turns <= stalled_after,
        "the raid must be projected to finish in a handful of turns, not grind to the horizon \
         (took {turns}, stalled past {stalled_after})"
    );
    assert!(
        reported.animals_killed > 0,
        "liveness — a projection that killed nothing would satisfy no reading of this mission"
    );

    // 2. The same herd, the same world, raided for real. The forecast answers for the sim.
    let home = spawn_home_band(&mut app, herd_pos);
    let party = spawn_party(&mut app, home, herd_pos, REPORTED_PARTY_WORKERS, deny(&id));
    let mut driven = None;
    for turn in 1..=horizon {
        drive_turn(&mut app);
        if herd_biomass(&app, &id).is_none_or(|biomass| biomass < line) {
            driven = Some(turn);
            break;
        }
    }
    let driven = driven.unwrap_or_else(|| {
        panic!(
            "a driven raid must push this herd past recovery; it stood at {:?} after {horizon} turns",
            herd_biomass(&app, &id)
        )
    });
    assert_ne!(
        phase(&app, party),
        Some(ExpeditionPhase::Hunting),
        "…and the party walks away on the turn it crosses the line (turn {driven})"
    );
    // **The two turn counts are deliberately NOT compared.** The projection makes no draw and
    // reports the expectation; this is one seeded run, and on an engagement of three animals a
    // single run is a *sample* with a long tail (measured: 2 to ~22 turns across map seeds). What
    // has to agree — and is what the defect got wrong — is the KIND of answer: a raid that
    // completes, against a raid reported never to.

    // 3. The verdict still fires. A party that really cannot outpace a herd's regrowth is told so —
    //    otherwise the fix above could have been "never say repelled".
    let mut resisting = wary_world();
    let (resisting_id, _) = pin_raid_herd(&mut resisting, RESISTING_HERD);
    let repelled = forecast(&resisting, &resisting_id, LONE_HUNTER);
    assert_eq!(
        repelled.outcome,
        DenialOutcome::Repelled,
        "a lone hunter against a fast-breeding herd forty times this size is still repelled — the \
         verdict must survive the fix, or the fix deleted it"
    );
    assert_eq!(
        repelled.turns_to_collapse, None,
        "and a repelled raid still names no collapse turn"
    );
}

// ---------------------------------------------------------------------------------------------------
// Waste is the point, and it is reported
// ---------------------------------------------------------------------------------------------------

/// **`wasted` is the bulk of a raid's take, and it is reported** (`docs/plan_denial_raid.md` §5) —
/// paired with the liveness assertion that a raid **delivers something**, because a raid that
/// delivered nothing at all would also pass a waste-is-large check.
///
/// Read off the hunt report the raid publishes every turn (`§6.6`, `CommandEventKind::HuntReport`),
/// which is the sim's own answer rather than a number this test recomputes.
#[test]
fn wasted_is_the_bulk_of_a_raids_take_and_is_reported() {
    let mut app = placid_world();
    let (id, herd_pos) = pin_raid_herd(&mut app, PLACID_HERD);
    let home = spawn_home_band(&mut app, herd_pos);
    let party = spawn_party(&mut app, home, herd_pos, PARTY_WORKERS, deny(&id));

    let horizon = expedition_cfg(&app).hunt.forecast_horizon_turns;
    let ledger = drive_raid(&mut app, party, horizon);

    assert!(
        ledger.reports > 0,
        "the raid must publish hunt reports at all"
    );
    // **The claim.** A raid kills far more than it can haul, and says so.
    assert!(
        ledger.wasted_biomass > ledger.carried_biomass,
        "waste must be the bulk of a raid's take: carried {}, wasted {}",
        ledger.carried_biomass,
        ledger.wasted_biomass
    );
    // **The liveness half.** A raid that hauled nothing would satisfy the line above trivially; the
    // whole design point is that it banks whatever it can on the way home.
    assert!(
        ledger.carried_biomass > 0.0,
        "a raid still banks what it can carry — a zero here means the carry half broke, and the \
         waste assertion above would pass anyway"
    );
    assert!(
        carried_food(&app, party) > 0.0,
        "and that haul is in the party's pack, not merely in a report"
    );
}

/// What a driven raid actually did, summed off the sim's **own** per-turn hunt report
/// (`CommandEventKind::HuntReport`, `docs/plan_hunt_through_combat.md` §6.6) rather than recomputed
/// by the test — the shipped statement about the raid is the thing under test.
struct RaidLedger {
    /// Biomass hauled into the party's pack over the run.
    carried_biomass: f32,
    /// Biomass killed and left on the range.
    wasted_biomass: f32,
    /// How many turns published a report at all — the liveness guard against a ledger of zeros.
    reports: u32,
}

impl RaidLedger {
    /// Everything the party put on the ground: what it hauled **plus** what it left.
    fn killed_biomass(&self) -> f32 {
        self.carried_biomass + self.wasted_biomass
    }
}

/// Drive `party`'s raid for at most `turns`, stopping early if it leaves `Hunting` (a delivery, a
/// fold-back, or a lost herd), and sum the reports it published.
fn drive_raid(app: &mut App, party: bevy::prelude::Entity, turns: u32) -> RaidLedger {
    let mut ledger = RaidLedger {
        carried_biomass: 0.0,
        wasted_biomass: 0.0,
        reports: 0,
    };
    let mut read_through = 0_u64;
    for _ in 1..=turns {
        drive_turn(app);
        // The log is a turn window and every entry carries a monotonic `seq`, so read forward from
        // where the last pass stopped rather than clearing it — the sim owns its retention.
        for entry in app.world.resource::<CommandEventLog>().iter() {
            if entry.seq <= read_through {
                continue;
            }
            read_through = entry.seq;
            if entry.kind.as_str() != "hunt_report" {
                continue;
            }
            let detail = entry.detail.clone().unwrap_or_default();
            ledger.carried_biomass += detail_value(&detail, "carried_biomass");
            ledger.wasted_biomass += detail_value(&detail, "wasted_biomass");
            ledger.reports += 1;
        }
        if phase(app, party) != Some(ExpeditionPhase::Hunting) {
            break;
        }
    }
    ledger
}

/// Read one `key=value` token off a feed entry's detail string.
fn detail_value(detail: &str, key: &str) -> f32 {
    detail
        .split_whitespace()
        .find_map(|token| token.strip_prefix(&format!("{key}=")))
        .and_then(|value| value.parse::<f32>().ok())
        .unwrap_or(0.0)
}

// ---------------------------------------------------------------------------------------------------
// …and a floor-`0` HUNT hauls its pack, exactly like every other floor
// ---------------------------------------------------------------------------------------------------

/// **A body far heavier than one hunter's pack**, so the carry bound and the kill are impossible to
/// confuse: a lone hunter's pack holds `hunt.per_worker_carry / provisions_per_biomass` = 40 biomass
/// and one animal is [`HEAVY_BODIED_HERD`]'s `body_mass`, so ~80% of every kill has to be left.
const HEAVY_BODIED_HERD: RaidQuarry = RaidQuarry {
    body_mass: 200.0,
    // Deep enough that the raid is still working a standing herd at the end of the horizon — the
    // fixture is about the carry bound, not about running the quarry out.
    carrying_capacity: 4000.0,
    biomass_fraction: 1.0,
    regrowth_rate: 0.10,
};

/// The party the pack bound is measured with: **one hunter**, the smallest legal party and the one
/// whose pack is furthest below a heavy body.
const SMALL_PARTY: u32 = LONE_HUNTER;

/// The party's pack expressed in the units the carry bound is applied in — `workers ×
/// hunt.per_worker_carry` provisions, inverted through the species' own `provisions_per_biomass`.
fn pack_biomass(app: &App, id: &str, workers: u32) -> f32 {
    let fauna = app.world.resource::<FaunaConfigHandle>().get();
    let registry = app.world.resource::<HerdRegistry>();
    let herd = registry.find(id).expect("herd present");
    let yields = herd_hunt_yield(herd, &fauna);
    workers as f32 * expedition_cfg(app).hunt.per_worker_carry / yields.provisions_per_biomass
}

fn carried_trade(app: &App, party: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<Expedition>(party)
        .map(|e| e.carried_trade)
        .unwrap_or(0.0)
}

/// **A floor-`0` hunt hauls its PACK and reports the rest as waste** — the defect this test exists
/// for (`docs/plan_denial_raid.md` §1).
///
/// A floor-`0` raid used to pass `f32::INFINITY` as its carry room on the premise that driving a herd
/// extinct makes the meat incidental. With an infinite pack `carried = killed × body_mass`, so the
/// party was recorded hauling home **everything it killed**: its hunt report published
/// `wasted_biomass = 0` for a raid that left a range of carcasses, and `Expedition::carried_trade`
/// accrued pelts off the whole kill rather than off the load. *When* a party stops engaging and *how
/// much* it can haul are separate questions — that is what [`ExpeditionMission::Deny`] is for — and
/// carry is never unbounded for a real party.
///
/// Three claims, each paired with the liveness assertion that makes it mean something:
/// 1. the raid reports **non-zero waste** — and still delivers something;
/// 2. its total haul is bounded by the **pack** — and the pack is not empty;
/// 3. its **trade accrues off `carried`, not off `killed`** — and it is non-zero.
#[test]
fn a_floor_zero_hunt_hauls_only_its_pack_and_reports_the_waste() {
    let mut app = placid_world();
    let (id, herd_pos) = pin_raid_herd(&mut app, HEAVY_BODIED_HERD);
    let home = spawn_home_band(&mut app, herd_pos);
    let party = spawn_party(
        &mut app,
        home,
        herd_pos,
        SMALL_PARTY,
        hunt(&id, STRIP_IT_BARE),
    );

    let pack = pack_biomass(&app, &id, SMALL_PARTY);
    let horizon = expedition_cfg(&app).hunt.forecast_horizon_turns;
    let ledger = drive_raid(&mut app, party, horizon);

    assert!(
        ledger.reports > 0,
        "the raid must publish hunt reports at all"
    );
    // 1. The waste is real and it is on the report.
    assert!(
        ledger.wasted_biomass > 0.0,
        "a lone hunter on a {}-biomass body leaves most of every kill: killed {}, carried {}",
        HEAVY_BODIED_HERD.body_mass,
        ledger.killed_biomass(),
        ledger.carried_biomass
    );
    // …and it delivers: a raid that engaged nothing would report zero waste too.
    assert!(
        carried_food(&app, party) > 0.0,
        "the raid must still bank the windfall it CAN carry — a floor-`0` raid that came home empty \
         would satisfy the waste claim above vacuously"
    );

    // 2. Everything it hauled fits in the pack. It never delivers at floor `0` (`done`/`relaunch`
    //    are both false), so the whole run's carry is one packful.
    assert!(
        ledger.carried_biomass <= pack + CARRY_TOLERANCE_BIOMASS,
        "a floor-`0` raid hauls its pack and no more: carried {} against a pack of {pack}",
        ledger.carried_biomass
    );
    assert!(
        ledger.killed_biomass() > pack,
        "…and the fixture must actually exceed it, or the bound above is untested"
    );

    // 3. The pelts ride on what was CARRIED. Both products come out of one conversion of the same
    //    carried biomass, so the trade banked is the carried biomass through the trade rate — and
    //    strictly less than the whole kill's worth.
    let trade_rate = {
        let fauna = app.world.resource::<FaunaConfigHandle>().get();
        let registry = app.world.resource::<HerdRegistry>();
        herd_hunt_yield(registry.find(&id).expect("herd present"), &fauna).trade_goods_per_biomass
    };
    let banked = carried_trade(&app, party);
    assert!(
        (banked - ledger.carried_biomass * trade_rate).abs() <= TRADE_TOLERANCE,
        "trade accrues off the CARRIED biomass: banked {banked} against carried {} × {trade_rate}",
        ledger.carried_biomass
    );
    assert!(
        banked > 0.0 && banked < ledger.killed_biomass() * trade_rate,
        "…and that is strictly less than the whole kill's worth ({}), which is what an unbounded \
         carry used to pay",
        ledger.killed_biomass() * trade_rate
    );
}

/// A few `Scalar` quanta of the party's pack, inverted into biomass: the pack is fixed-point and the
/// report's `carried_biomass` is an `f32` printed to 3 dp, so the two agree to about a quantum.
const CARRY_TOLERANCE_BIOMASS: f32 = 1.0;

/// The same allowance on the trade account, which is a bare `f32` accumulator against a sum of
/// 3-dp-rounded report values.
const TRADE_TOLERANCE: f32 = 0.01;

/// **Denial is unchanged by the carry fix, and the two missions now differ only where they should.**
///
/// The carry arm was never the line denial changed (`docs/plan_denial_raid.md` §1: `carried =
/// min(killed × body_mass, carry_room)` is *identical* for both), so a denial party and a floor-`0`
/// hunting party of the same size on the same herd must haul the **same** load and bank the **same**
/// pelts. Before the fix they did not — the hunt hauled its whole kill — and that difference was the
/// bug, not the mission.
///
/// Paired with the liveness half: denial still kills **at least** as much as the hunt does, because
/// the engagement bound is the line it really drops.
#[test]
fn denial_and_a_floor_zero_hunt_account_carry_identically() {
    let ledger_for = |mission: &dyn Fn(&str) -> ExpeditionMission| {
        let mut app = placid_world();
        let (id, herd_pos) = pin_raid_herd(&mut app, HEAVY_BODIED_HERD);
        let home = spawn_home_band(&mut app, herd_pos);
        let party = spawn_party(&mut app, home, herd_pos, SMALL_PARTY, mission(&id));
        let horizon = expedition_cfg(&app).hunt.forecast_horizon_turns;
        let ledger = drive_raid(&mut app, party, horizon);
        (
            ledger,
            carried_food(&app, party),
            carried_trade(&app, party),
        )
    };

    let (denial, denial_food, denial_trade) = ledger_for(&deny);
    let (raid, raid_food, raid_trade) = ledger_for(&|id| hunt(id, STRIP_IT_BARE));

    assert!(
        denial_food > 0.0 && denial_trade > 0.0,
        "the denial fixture must actually haul something ({denial_food} food, {denial_trade} trade)"
    );
    assert_eq!(
        denial_food, raid_food,
        "the pack is a carry bound for both missions: denial banks the same food a floor-`0` hunt \
         does"
    );
    assert_eq!(
        denial_trade, raid_trade,
        "…and the same pelts, off the same carried biomass"
    );
    assert!(
        (denial.carried_biomass - raid.carried_biomass).abs() <= CARRY_TOLERANCE_BIOMASS,
        "…and their reports agree: denial carried {}, the raid {}",
        denial.carried_biomass,
        raid.carried_biomass
    );
    // The liveness half: the missions are still different where they are meant to be.
    assert!(
        denial.killed_biomass() >= raid.killed_biomass(),
        "denial drops the ENGAGEMENT bound, so it never kills less than the hunt: denial {}, raid {}",
        denial.killed_biomass(),
        raid.killed_biomass()
    );
}

// ---------------------------------------------------------------------------------------------------
// The kit cost — the fourth cost, and it needed no new mechanism
// ---------------------------------------------------------------------------------------------------

/// **A denial raid is the most equipment-intensive act in the game** (`docs/plan_denial_raid.md`
/// §1.2), and it falls out of the shipped TOE with nothing added: the hunting kit is charged
/// `wear_per_kill` per animal **killed**, never per turn elapsed, so the mission that kills the most
/// for the least return burns the most irreplaceable kit.
///
/// Both halves are asserted, because the second is what makes the first mean anything:
/// 1. over the same turns on the same herd, a denial party wears its spears **harder** than a
///    hunting party at the food peak;
/// 2. the wear tracks **kills**, not the clock — a party that engaged nothing spends nothing.
#[test]
fn a_denial_raid_burns_more_kit_than_a_hunt_and_only_for_kills() {
    /// Long enough that both parties are past their first turn but neither raid has ended.
    const TURNS: u32 = 3;

    let spear_wear = |mission: fn(&str) -> ExpeditionMission| {
        let mut app = placid_world();
        let (id, herd_pos) = pin_raid_herd(&mut app, PLACID_HERD);
        let home = spawn_home_band(&mut app, herd_pos);
        let party = spawn_party(&mut app, home, herd_pos, PARTY_WORKERS, mission(&id));
        for _ in 0..TURNS {
            drive_turn(&mut app);
        }
        app.world
            .get::<BandEquipment>(party)
            .expect("the party carries its kit")
            .hunting_wear
    };

    let denial = spear_wear(deny);
    let harvest = spear_wear(|id| hunt(id, 0.5));
    assert!(
        denial > harvest,
        "a denial raid must burn more spear than a hunt over the same turns: {denial} vs {harvest}"
    );
    assert!(harvest > 0.0, "and the hunt must be wearing its kit at all");

    // **Wear tracks USE, never turns elapsed.** A party parked on a herd with nothing to spare
    // engages nothing and spends nothing — which is the property §1.2 required for the kit cost to
    // land on the raid rather than on the march.
    let mut app = placid_world();
    // Seated under the collapse line, so the raid is already over before it starts: nothing is
    // engaged, so nothing is killed.
    let (id, herd_pos) = pin_raid_herd(
        &mut app,
        RaidQuarry {
            biomass_fraction: 0.0,
            ..PLACID_HERD
        },
    );
    let home = spawn_home_band(&mut app, herd_pos);
    let party = spawn_party(&mut app, home, herd_pos, PARTY_WORKERS, deny(&id));
    for _ in 0..TURNS {
        drive_turn(&mut app);
    }
    let idle_wear = app
        .world
        .get::<BandEquipment>(party)
        .map(|kit| kit.hunting_wear)
        // A herd that despawned takes the party home; either way nothing was killed.
        .unwrap_or(0.0);
    assert_eq!(
        idle_wear, 0.0,
        "a raid that kills nothing spends no kit — wear is charged per animal, not per turn"
    );
    // Liveness for the config itself: the kit is genuinely consumable, so the comparison above is
    // not measuring two zeroes.
    let equipment = app.world.resource::<EquipmentConfigHandle>().get();
    assert!(
        equipment.hunting_kit.wear_per_kill > 0.0,
        "the hunting kit must actually wear per kill, or this whole fixture asserts nothing"
    );
}

// ---------------------------------------------------------------------------------------------------
// forecast == actual, in its restated (distribution) form, on the EXPORTED SNAPSHOT
// ---------------------------------------------------------------------------------------------------

/// **`forecast == actual` on the exported snapshot, per component**
/// (`docs/plan_hunt_through_combat.md` §6.4, restated).
///
/// Where nothing is stochastic the distribution is degenerate: the exported band is a **point**
/// (`low == likely == high`), the live raid completes on exactly that turn, and its delivered and
/// wasted food are the projected ones. The assertion reads the **wire** — the `denialEstimates` row
/// the client will consume — never the in-process forecast, so a capture that dropped or mis-keyed
/// the row fails here rather than at the client.
#[test]
fn the_exported_denial_estimate_matches_a_real_raid_at_wariness_zero() {
    let mut app = placid_world();
    let (id, herd_pos) = pin_raid_herd(&mut app, PLACID_HERD);
    reveal_herd(&mut app, herd_pos);
    recapture_snapshot_in_place(&mut app.world);

    let row = exported_denial_row(&app, &id, PARTY_WORKERS);
    assert_eq!(
        (row.turns_to_collapse_low, row.turns_to_collapse_high),
        (row.turns_to_collapse, row.turns_to_collapse),
        "with the retreat at its identity every quantile resolves the same raid, so the reported \
         window is a POINT — the half that makes the wariness-on sibling a real widening"
    );
    assert_eq!(
        row.outcome, "past_recovery",
        "the exported verdict must name the mission succeeding"
    );

    let home = spawn_home_band(&mut app, herd_pos);
    let party = spawn_party(&mut app, home, herd_pos, PARTY_WORKERS, deny(&id));
    let horizon = expedition_cfg(&app).hunt.forecast_horizon_turns;
    let mut completed = None;
    for turn in 1..=horizon {
        drive_turn(&mut app);
        if phase(&app, party) != Some(ExpeditionPhase::Hunting) {
            completed = Some(turn);
            break;
        }
    }
    assert_eq!(
        completed,
        Some(row.turns_to_collapse),
        "the exported turns-to-collapse must be the turn the real party stops raiding — fix the \
         forecast, never the sim"
    );
    // Per component: the food the party actually holds is the food the wire promised. The pack is a
    // fixed-point store, so compare on its own grid.
    let projected = scalar_from_f32(row.delivered_food).to_f32();
    assert!(
        (carried_food(&app, party) - projected).abs() <= TAKE_EPSILON,
        "delivered food: wire promised {projected}, the party holds {}",
        carried_food(&app, party)
    );
    assert!(
        row.wasted_food > row.delivered_food,
        "and the wire states the waste, which is the bulk of it: {} wasted vs {} delivered",
        row.wasted_food,
        row.delivered_food
    );
    assert!(
        row.animals_killed > 0,
        "liveness — a projection of zero kills would satisfy every comparison above"
    );
}

/// **The band widens where a stochastic stage is authored, and the seeded raids sit in it** — the
/// distribution half of the restated invariant (`docs/plan_hunt_through_combat.md` §6.4), asserted
/// across many seeds because a seeded draw cannot be predicted and one run would be pinning a sample.
///
/// **The claim is about the EXPECTATION, not about every run, and the difference is the readout's
/// unit.** §6.4's containment is stated for a *take* — one turn's yield, where evaluating the
/// arithmetic at `±sigmas` really does bracket the draw. `turns_to_collapse` is an **integral over
/// many draws**, and a projection pinned to `+2σ` on *every* turn is not an upper bound on a run that
/// got one very lucky turn early: the stock it removed compounds. So the honest assertions are the
/// three below — the band is genuinely wide, its middle is where the seeded runs actually live, and
/// the runs differ from each other at all.
#[test]
fn the_exported_denial_band_brackets_the_seeded_raids_on_a_wary_herd() {
    /// Enough seeds that a real spread shows and few enough that the fixture stays quick.
    const SEEDS: [u64; 8] = [1, 7, 19, 23, 101, 557, 9001, 77003];

    let mut window = None;
    let mut completions = Vec::new();
    for seed in SEEDS {
        let mut app = wary_world();
        app.world.resource_mut::<SimulationConfig>().map_seed = seed;
        let (id, herd_pos) = pin_raid_herd(&mut app, BANDED_HERD);
        reveal_herd(&mut app, herd_pos);
        recapture_snapshot_in_place(&mut app.world);
        let row = exported_denial_row(&app, &id, BANDED_PARTY_WORKERS);
        // The band is a property of the herd and the party, not of the seed — the projection makes
        // no draw at all. Assert that directly rather than re-reading it per seed.
        let (low, high) = (row.turns_to_collapse_low, row.turns_to_collapse_high);
        match window {
            None => window = Some((low, high)),
            Some(previous) => assert_eq!(
                previous,
                (low, high),
                "the forecast draws nothing, so its band cannot move with the map seed"
            ),
        }

        let home = spawn_home_band(&mut app, herd_pos);
        let party = spawn_party(&mut app, home, herd_pos, BANDED_PARTY_WORKERS, deny(&id));
        let horizon = expedition_cfg(&app).hunt.forecast_horizon_turns;
        for turn in 1..=horizon {
            drive_turn(&mut app);
            if phase(&app, party) != Some(ExpeditionPhase::Hunting) {
                completions.push(turn);
                break;
            }
        }
    }

    let (low, high) = window.expect("the sweep ran at least one seed");
    assert_eq!(
        completions.len(),
        SEEDS.len(),
        "every seeded raid must complete inside the horizon for the band check to mean anything"
    );
    // 1. **The band is real.** At `wariness 0` the sibling fixture above pins it to a point; here a
    //    stochastic stage is authored and the reported window must have two different ends.
    assert!(
        low < high,
        "an authored retreat must widen the reported window; it read {low}–{high}"
    );
    // 2. **Its middle is where the raids actually live** — the expectation claim §6.4 restates,
    //    measured over the sweep rather than per run.
    let mean = completions.iter().sum::<u32>() as f32 / completions.len() as f32;
    assert!(
        (low as f32..=high as f32).contains(&mean),
        "the seeded raids averaged turn {mean}, outside the reported {low}–{high}"
    );
    // 3. **Liveness.** A dead retreat stage would put every seed on the same turn and satisfy both
    //    assertions above while asserting nothing at all about a distribution.
    assert!(
        completions.iter().any(|turn| *turn != completions[0]),
        "the seeded retreat must actually move the raid's length; every seed finished on turn {}",
        completions[0]
    );
}

/// Both the wire and the pack land on the sim's fixed-point grid, at different points, so a per-
/// component comparison allows a few `Scalar` quanta.
const TAKE_EPSILON: f32 = 4.0 / core_sim::Scalar::SCALE as f32;

/// A snapshot-exported denial estimate, resolved for one herd and party size.
struct ExportedDenialRow {
    turns_to_collapse: u32,
    turns_to_collapse_low: u32,
    turns_to_collapse_high: u32,
    outcome: String,
    animals_killed: u32,
    delivered_food: f32,
    wasted_food: f32,
}

fn exported_denial_row(app: &App, id: &str, party_workers: u32) -> ExportedDenialRow {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let herd = snapshot
        .herds
        .iter()
        .find(|h| h.id == id)
        .expect("the herd is on the wire (its tile was revealed)");
    let row = herd
        .denial_estimates
        .iter()
        .find(|row| row.party_workers == party_workers)
        .unwrap_or_else(|| panic!("no denial estimate row for a party of {party_workers}"));
    ExportedDenialRow {
        turns_to_collapse: row.turns_to_collapse,
        turns_to_collapse_low: row.turns_to_collapse_low,
        turns_to_collapse_high: row.turns_to_collapse_high,
        outcome: row.outcome.clone(),
        animals_killed: row.animals_killed,
        delivered_food: row.delivered_food,
        wasted_food: row.wasted_food,
    }
}

/// Herd display telemetry is fog-filtered, so a fixture reading the wire has to reveal the ground
/// the herd is standing on — the in-game precondition for seeing that panel at all.
fn reveal_herd(app: &mut App, pos: UVec2) {
    let grid = app.world.resource::<SimulationConfig>().grid_size;
    let viewer = app.world.resource::<core_sim::ViewerFaction>().0;
    let mut ledger = app.world.resource_mut::<VisibilityLedger>();
    let map = ledger.ensure_faction(viewer, grid.x, grid.y);
    map.mark_active(pos.x, pos.y, 0);
}

// ---------------------------------------------------------------------------------------------------
// THE REPORTED CASE — a viable party must be REACHABLE, and the sheet must open on it
// ---------------------------------------------------------------------------------------------------

/// **The reported quarry** — Red Deer, and the species is the fixture: `engage_rate 1` and
/// `wariness 0.65` put one hunter at `1 × (1 − 0.65) = 0.35` animals a turn, which is the whole left
/// side of the arithmetic this test exists for. Both dials are resolved off the display name, so
/// naming the species is the only way to state them.
const REPORTED_QUARRY: &str = "Red Deer";

/// Red Deer's own `body_mass` (`fauna_config.json`). Stated here because the report counts the herd
/// in **head** and the sim counts **biomass**, and this is the conversion between them.
const RED_DEER_BODY_MASS: f32 = 15.0;

/// **The reported herd: 51 of 119 head.** Its per-turn replacement is
/// `0.10 × 51 × (1 − 51/119) = 2.91` animals, so out-killing it takes `2.91 / 0.35 = 8.3` hunters —
/// and therefore **9**, one past the sampling bound of 8 that the stepper used to stop at.
///
/// It is seated **below** the food peak deliberately: that is where the report found it, and it is
/// the regime where the raid accelerates as it works (regrowth falls with the stock), so a party
/// that clears the line clears it decisively.
const REPORTED_HERD: RaidQuarry = RaidQuarry {
    body_mass: RED_DEER_BODY_MASS,
    carrying_capacity: 119.0 * RED_DEER_BODY_MASS,
    biomass_fraction: 51.0 / 119.0,
    // The roster's ordinary big-game rate — deer's own `regrowth_rate`.
    regrowth_rate: 0.10,
};

/// **The party the arithmetic demands: `ceil(8.3)` = 9.** Pinned as a literal, because rounding it
/// the other way is the defect — `8` ties-or-loses against the herd's regrowth every turn while the
/// sheet presents it as the answer.
///
/// **The sheet opens at or ABOVE this, not on it.** The arithmetic is a bound on the search: a party
/// that only just out-kills the regrowth declines the herd so slowly that the projection runs out of
/// `hunt.forecast_horizon_turns` before it crosses `collapse_fraction`, so row 9 reads `horizon` and
/// the smallest row that actually **succeeds** is 10. That is why the assertion below is an
/// inequality against the arithmetic plus a `succeeded` test on the seed's own row, rather than an
/// equality with this constant.
const REPORTED_PARTY_NEEDED: u32 = 9;

/// The reported band's idle workers. It is **not** a bound the sim applies here — it is the number
/// that made the flat ceiling absurd: the people were there, and a lever said no.
const REPORTED_IDLE_WORKERS: u32 = 16;

/// **Enough seeds to read a mean, few enough to stay quick.** The retreat is binomial and this herd
/// is a near-run thing (9 hunters remove ~47 biomass a turn against ~44 of regrowth), so a single
/// driven raid is a *sample*: measured over 400 runs it declines ~90% of the time. The claim below
/// is therefore about where the runs live, in the shape
/// [`the_exported_denial_band_brackets_the_seeded_raids_on_a_wary_herd`] already uses.
const REPORTED_SWEEP_SEEDS: [u64; 6] = [3, 11, 29, 97, 613, 40009];

/// **The reported defect, end to end** — *"a denial raid can be impossible to staff, and the sheet
/// gives the player no number that works"* (`docs/plan_denial_raid.md` §3.1).
///
/// Red Deer at 51 of 119 head, a band holding 16 idle workers, and a stepper that stopped at 8:
///
/// ```text
/// one hunter kills   1 × (1 − 0.65) = 0.35 deer/turn
/// the herd replaces  0.10 × 51 × (1 − 51/119) = 2.91 deer/turn
/// break-even         2.91 / 0.35 = 8.3 hunters   ⇒   9 to decline
/// quoted axis        8   (`estimate_party_sizes`, and it also capped the launch)
/// ```
///
/// Two unrelated eights, and the config one landed **one below** the requirement — so the verdict
/// *"breeds back faster than this party kills"* was correct at every party size the player could
/// reach, and denial on this quarry was unreachable because a lever said so.
///
/// Four claims, and the last two are a matched pair:
/// 1. the sim **names** a party — never below the arithmetic's `9`, with `8` still repelled (the
///    rounding, in the defect's own shape) and the named row's own verdict a **success** rather than
///    merely *not repelled*;
/// 2. it is quoted **past the flat ceiling**, with headroom above it, so the sheet can open there
///    and the player can still over-staff from the workers they hold;
/// 3. driven for real, the seeded party **declines the herd** (the liveness half);
/// 4. one hunter fewer does **not** (the ordering half) — so the boundary is real rather than the
///    test measuring a raid that would have worked at any size.
#[test]
fn the_reported_red_deer_raid_is_staffable_and_its_seeded_party_declines_the_herd() {
    let mut app = wary_world();
    let (id, pos) = pin_raid_herd_of(&mut app, REPORTED_QUARRY, REPORTED_HERD);
    reveal_herd(&mut app, pos);
    recapture_snapshot_in_place(&mut app.world);
    let sheet = exported_denial_sheet(&app, &id);
    let sampled = expedition_cfg(&app).estimate_party_sizes;

    // 1. The number, and the rounding. `8.3` hunters is NINE — and the sheet never opens BELOW it.
    assert!(
        sheet.party_needed >= REPORTED_PARTY_NEEDED,
        "the sheet must never open below the arithmetic requirement; 8.3 hunters rounds UP \
         (opened on {}, requirement {REPORTED_PARTY_NEEDED})",
        sheet.party_needed
    );
    assert_eq!(
        sheet.outcome_at(REPORTED_PARTY_NEEDED - 1),
        Some(REPELLED),
        "…and the party one below the requirement must still read repelled — a floor here would \
         hand back a party that provably ties-or-loses against the regrowth"
    );
    assert!(
        sheet.succeeded_at(sheet.party_needed),
        "the seeded party's own row must say the raid SUCCEEDED — `past_recovery` / `herd_lost`, \
         never merely 'not repelled': a `horizon` row is a raid the projection never saw finish, \
         and opening there quotes a party under the verdict \"still standing when the forecast runs \
         out\""
    );
    assert!(
        !sheet.succeeded_at(sheet.party_needed - 1),
        "…and the row below the seed must NOT have succeeded, or the sheet skipped a party that works"
    );

    // 2. It is reachable at all — the defect was that this row did not exist.
    assert!(
        sheet.party_needed > sampled,
        "this fixture only means something while the requirement ({}) is past the flat sampling \
         bound ({sampled}) — that pairing IS the reported bug",
        sheet.party_needed
    );
    assert!(
        sheet.party_needed <= REPORTED_IDLE_WORKERS,
        "the band held {REPORTED_IDLE_WORKERS} idle workers; a requirement past that would make \
         this a fixture about a band too small, which is a different (and legitimate) refusal"
    );
    assert!(
        sheet.rows() > sheet.party_needed,
        "the table must quote headroom ABOVE the requirement, or a player holding more workers than \
         the herd needs has no row to read for spending them (rows {}, needed {})",
        sheet.rows(),
        sheet.party_needed
    );

    // 3 + 4. Driven for real, over seeds, because the retreat is a draw and this herd is a near-run
    //        thing. The projection is not re-read here — the sim is asked directly.
    let opening = REPORTED_HERD.carrying_capacity * REPORTED_HERD.biomass_fraction;
    let seeded = mean_biomass_after_raiding(sheet.party_needed);
    let one_short = mean_biomass_after_raiding(sheet.party_needed - 1);
    assert!(
        seeded < opening,
        "the seeded party must actually drive the herd down: {opening} -> {seeded} on average over \
         {} seeds",
        REPORTED_SWEEP_SEEDS.len()
    );
    assert!(
        one_short > seeded,
        "…and one hunter fewer must leave the herd standing higher ({one_short} vs {seeded}), or \
         the requirement is not where the sheet says it is"
    );
}

/// The wire key [`DenialOutcome::Repelled`] publishes — spelled once so a test cannot drift from the
/// enum.
const REPELLED: &str = "repelled";

/// Drive a **real** denial raid on [`REPORTED_HERD`] with `workers` hunters, once per seed, and
/// average the biomass left standing after a full forecast horizon. `0` for a herd the raid erased
/// outright (the registry despawns it), which is the strongest form of "declined".
///
/// One world per seed: the retreat draws from `(map_seed, tick, herd, party)`, so the seed is what
/// makes each run an independent sample rather than a repeat.
fn mean_biomass_after_raiding(workers: u32) -> f32 {
    let mut endings = Vec::new();
    for seed in REPORTED_SWEEP_SEEDS {
        let mut app = wary_world();
        app.world.resource_mut::<SimulationConfig>().map_seed = seed;
        let (id, pos) = pin_raid_herd_of(&mut app, REPORTED_QUARRY, REPORTED_HERD);
        let home = spawn_home_band(&mut app, pos);
        let party = spawn_party(&mut app, home, pos, workers, deny(&id));
        let horizon = expedition_cfg(&app).hunt.forecast_horizon_turns;
        for _ in 1..=horizon {
            drive_turn(&mut app);
            if phase(&app, party) != Some(ExpeditionPhase::Hunting) {
                break;
            }
        }
        endings.push(herd_biomass(&app, &id).unwrap_or(0.0));
    }
    endings.iter().sum::<f32>() / endings.len() as f32
}

/// **A herd whose replacement out-runs every party the sim will quote** — the same Red Deer, on
/// range rich enough to hold 1,333 head. Its peak replacement is `0.10 × K/4 / body = 33` deer a
/// turn against `0.35` per hunter, so the requirement is ~96 — past `deny.max_party_quoted`, which
/// is the *no viable number* case the readout has to survive rather than paper over.
const UNDENIABLE_HERD: RaidQuarry = RaidQuarry {
    body_mass: RED_DEER_BODY_MASS,
    // Seated at the food peak, where the logistic curve is at its most productive: the hardest
    // point on the path, and the one `herd_replacement_animals` sizes the requirement against.
    biomass_fraction: 0.5,
    carrying_capacity: 20_000.0,
    regrowth_rate: 0.10,
};

/// **When there is no viable number, the sheet says so — and `repelled` keeps working**
/// (`docs/plan_denial_raid.md` §3.1).
///
/// Three situations reach *"no quoted party drives this herd down"* and this pins the one that is
/// purely a **readout bound**: a requirement past `deny.max_party_quoted`. The other two — a herd
/// already past recovery, and a quarry nothing can bring into contact — are covered by
/// [`a_wary_herd_resists_denial_and_the_forecast_says_so`] and the fauna unit tests.
///
/// The pairing is what makes it an assertion rather than a tautology: the sentinel must appear
/// **and** every row must still carry a real verdict, so a client has something to render. A sheet
/// that answered `0` by emptying the table would satisfy the first half alone.
#[test]
fn a_herd_no_quoted_party_can_collapse_reports_no_viable_party_and_still_reads_repelled() {
    let mut app = wary_world();
    let (id, pos) = pin_raid_herd_of(&mut app, REPORTED_QUARRY, UNDENIABLE_HERD);
    reveal_herd(&mut app, pos);
    recapture_snapshot_in_place(&mut app.world);
    let sheet = exported_denial_sheet(&app, &id);
    let cfg = expedition_cfg(&app);

    assert_eq!(
        sheet.party_needed, NO_VIABLE_DENIAL_PARTY,
        "no party this sim will quote outpaces the herd, so the requirement is the sentinel — \
         never a number the player could send and watch fail"
    );
    // The table is still a table: capped at the quoting bound, and every row answers.
    assert_eq!(
        sheet.rows(),
        cfg.deny.max_party_quoted,
        "the axis must stop at the quoting bound rather than running away with the requirement"
    );
    assert!(
        sheet.every_row_reads(REPELLED),
        "…and every row must still name WHY, so the client renders a verdict instead of a blank"
    );
    // Liveness: the same species on an ordinary herd is deniable, so the sentinel above is a fact
    // about this range and not about the export being broken.
    let mut ordinary = wary_world();
    let (ordinary_id, ordinary_pos) =
        pin_raid_herd_of(&mut ordinary, REPORTED_QUARRY, REPORTED_HERD);
    reveal_herd(&mut ordinary, ordinary_pos);
    recapture_snapshot_in_place(&mut ordinary.world);
    assert_ne!(
        exported_denial_sheet(&ordinary, &ordinary_id).party_needed,
        NO_VIABLE_DENIAL_PARTY,
        "a normal herd of the same species must still name a party — otherwise the sentinel above \
         is the export failing, not the range being too rich to deny"
    );
}

/// The wire's *"no quoted party drives this herd down"* — `HerdTelemetryState::denial_party_needed`'s
/// sentinel, spelled once here so the fixture states what it is asserting.
const NO_VIABLE_DENIAL_PARTY: u32 = 0;

/// The exported denial sheet for one herd: the rows the client looks up, and the party size it opens
/// on. Read off the **wire**, never the in-process forecast, so a capture that dropped either fails
/// here rather than at the client.
struct ExportedDenialSheet {
    party_needed: u32,
    outcomes: Vec<(u32, String)>,
}

impl ExportedDenialSheet {
    fn rows(&self) -> u32 {
        self.outcomes.len() as u32
    }

    fn outcome_at(&self, party_workers: u32) -> Option<&str> {
        self.outcomes
            .iter()
            .find(|(workers, _)| *workers == party_workers)
            .map(|(_, outcome)| outcome.as_str())
    }

    fn every_row_reads(&self, outcome: &str) -> bool {
        !self.outcomes.is_empty() && self.outcomes.iter().all(|(_, got)| got == outcome)
    }

    /// Did the row for `party_workers` say the raid **succeeded** — the exact test the sheet's
    /// seed applies (`DenialOutcome::succeeded`), asked through the enum rather than against a
    /// hand-written list of keys, because a second list is the drift the seed's own fix removed.
    fn succeeded_at(&self, party_workers: u32) -> bool {
        self.outcome_at(party_workers)
            .and_then(DenialOutcome::from_wire)
            .is_some_and(DenialOutcome::succeeded)
    }
}

fn exported_denial_sheet(app: &App, id: &str) -> ExportedDenialSheet {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let herd = snapshot
        .herds
        .iter()
        .find(|h| h.id == id)
        .expect("the herd is on the wire (its tile was revealed)");
    ExportedDenialSheet {
        party_needed: herd.denial_party_needed,
        outcomes: herd
            .denial_estimates
            .iter()
            .map(|row| (row.party_workers, row.outcome.clone()))
            .collect(),
    }
}
