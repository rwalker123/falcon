//! **A STANDING POOL BUILDS ITS OWN KIT** — `docs/plan_pool_toe.md`, slice 1.
//!
//! The five standing pools each work many sites out of one band-wide stock of tools, and each used
//! to resolve **one** kit through a lookup that asked *"which tool serves this web?"* without naming
//! a rung. Roads already broke it — a `Roadwork` pool keeping a dirt road and a paved road needs
//! earthmoving gear **and** stone-dressing gear — and a rung-tied plant tool would have broken it
//! *silently*: the lookup would resolve nothing, the pool would register no demand, and nothing
//! would flag it.
//!
//! What replaced it: each site requires the tools that serve **its own branch at its own rung**, a
//! pool's TOE is the sum over its sites, and every pool's claim on one tool is settled **band-wide**
//! by the player's own `SourcePriority`.

use bevy::prelude::{App, Entity, UVec2, With};
use core_sim::extraction::{
    deposit_rung_span, tile_deposit_capacity, DepositRegistry, DepositSource,
};
use core_sim::{
    build_test_app, BandEquipment, BandId, EquipmentConfig, FactionId, LaborAllocation,
    LaborTarget, LadderConfig, PopulationCohort, ResidentBand, RoadKeeper, RoadRegistry,
    RungBranch, RungKey, SourcePriority, Tile, TileRegistry, UpkeepFundMode, ViewerFaction,
};
use sim_schema::TerrainType;

// ---------------------------------------------------------------------------------------------
// The tools the shipped roster carries for the branches under test
// ---------------------------------------------------------------------------------------------

/// The `route:dirt_road` rung's tool.
const EARTHMOVING: &str = "earthmoving";
/// The tool `route:paved_road` **and** `extraction:quarry` both want — the one item two pools reach
/// for, and therefore the whole reason the settlement is band-wide.
const STONE_DRESSING: &str = "stone_dressing";
/// The plant branch's tool. It names no rung, so every plant rung wants it.
const HOES: &str = "hoes";

// ---------------------------------------------------------------------------------------------
// (1) A POOL'S TOE IS BUILT FROM ITS SITES
// ---------------------------------------------------------------------------------------------

/// Every tool this rung wants, as ids in roster order.
fn tools_for(config: &EquipmentConfig, rung: RungKey) -> Vec<String> {
    config
        .pool_toe(rung.branch(), Some(&rung.wire_key()))
        .tools()
        .iter()
        .map(|tool| tool.item.to_string())
        .collect()
}

/// ⛔ **A `Roadwork` POOL KEEPING A DIRT ROAD AND A PAVED ROAD REQUIRES BOTH TOOLS — the case one
/// kit per pool cannot express at all.**
///
/// `earthmoving` is bound to `route:dirt_road` and `stone_dressing` to `route:paved_road`, so a
/// lookup that answers with **one** roster entry per pool must be wrong about one of the two roads
/// whichever entry it picks. Asked per site, at the site's own held rung, the question has a
/// complete answer and the pool's TOE is their sum.
///
/// **Stated from both ends.** The two rungs want **different** tools (a per-rung answer, not a
/// per-branch one), and the pool's requirement over a band keeping one of each is **both**.
#[test]
fn a_roadwork_pool_keeping_both_rungs_requires_both_tools() {
    let config = EquipmentConfig::builtin();

    assert_eq!(
        tools_for(&config, RungKey::RouteDirtRoad),
        vec![EARTHMOVING.to_string()],
        "a dirt road wants the earthmoving gear and nothing else"
    );
    assert_eq!(
        tools_for(&config, RungKey::RoutePavedRoad),
        vec![STONE_DRESSING.to_string()],
        "…and a paved road wants the stone-dressing gear, which is a DIFFERENT tool on the SAME \
         branch — the pair no single kit per pool can carry"
    );

    // **The pool's TOE is the sum over its sites**, one unit per hand on each.
    const HANDS_ON_EACH_ROAD: f32 = 3.0;
    let required = required_units(
        &config,
        &[
            (RungKey::RouteDirtRoad, HANDS_ON_EACH_ROAD),
            (RungKey::RoutePavedRoad, HANDS_ON_EACH_ROAD),
        ],
    );
    assert_eq!(
        required,
        vec![
            (EARTHMOVING.to_string(), HANDS_ON_EACH_ROAD),
            (STONE_DRESSING.to_string(), HANDS_ON_EACH_ROAD),
        ],
        "one pool, two rungs, two tool lines — and the pool would register demand for neither \
         under the retired lookup, because both are rung-bound and a role row stands on no rung"
    );
}

/// ⛔ **AND THE POOL REALLY DOES WORK BOTH ROADS WITH BOTH TOOLS — SPENDING EACH ON ITS OWN ROAD.**
///
/// The requirement above is a query; this is the turn. One band, one `Roadwork` pool, a dirt road
/// and a paved road, and one set of each tool. After the turn **both** items have lost condition —
/// which is a statement one kit per pool cannot make, because whichever kit it picked the other
/// road's tool was never in anybody's hands.
///
/// **The two controls are what make it a statement about the RUNG.** A band keeping only the dirt
/// road spends earthmoving gear and leaves the chisel untouched, and a band keeping only the paved
/// road does the reverse — so *"both wore"* above cannot be a pool that simply spends everything it
/// owns.
#[test]
fn a_roadwork_pool_spends_each_roads_own_tool_and_only_on_that_road() {
    let both = a_roadwork_turn(&[RungKey::RouteDirtRoad, RungKey::RoutePavedRoad]);
    assert!(
        both.earthmoving_worn > 0.0 && both.stone_dressing_worn > 0.0,
        "one pool keeping both rungs works both tools: earthmoving {} chisel {}",
        both.earthmoving_worn,
        both.stone_dressing_worn
    );

    let dirt_only = a_roadwork_turn(&[RungKey::RouteDirtRoad]);
    assert!(
        dirt_only.earthmoving_worn > 0.0,
        "fixture: a dirt road alone must still work the earthmoving gear, or the control below \
         says nothing"
    );
    assert_eq!(
        dirt_only.stone_dressing_worn, 0.0,
        "…and the chisel it also owns is never in that road's requirement, so it is charged nothing"
    );

    let paved_only = a_roadwork_turn(&[RungKey::RoutePavedRoad]);
    assert!(
        paved_only.stone_dressing_worn > 0.0,
        "fixture: a paved road alone must work the chisel"
    );
    assert_eq!(
        paved_only.earthmoving_worn, 0.0,
        "…and the earthmoving gear beside it is charged nothing"
    );
}

/// What one `Roadwork` turn spent, per tool.
struct RoadworkTurn {
    earthmoving_worn: f32,
    stone_dressing_worn: f32,
}

/// **A band keeping one road per named rung, holding one set of BOTH road tools.** Owning both is
/// the point: a pool that spends only what its rungs want has to be handed more than it wants.
fn a_roadwork_turn(rungs: &[RungKey]) -> RoadworkTurn {
    /// Enough hands that every road gets some, and enough tools that the settlement fills every
    /// line — this fixture is about *which* tool is spent, never about scarcity.
    const KEEPERS: u32 = 4;
    const A_LONG_HAUL: f32 = 12.0;

    let mut app = spawn_world();
    let (band, _, band_id, home) = first_band(&mut app);
    for (step, rung) in rungs.iter().enumerate() {
        let tile = tile_east_of(&app, home, step as u32 + 1);
        seat_road(&mut app, tile, *rung, band_id, A_LONG_HAUL);
    }
    staff_one_role(
        &mut app,
        band,
        LaborTarget::Roadwork,
        KEEPERS,
        UpkeepFundMode::Spread,
    );
    let fresh = stock_exactly(
        &mut app,
        band,
        &[(EARTHMOVING, KEEPERS), (STONE_DRESSING, KEEPERS)],
    );

    app.update();

    let worn = app
        .world
        .get::<BandEquipment>(band)
        .expect("the fixture band keeps its ledger");
    RoadworkTurn {
        earthmoving_worn: worn.wear_of(EARTHMOVING) - fresh.wear_of(EARTHMOVING),
        stone_dressing_worn: worn.wear_of(STONE_DRESSING) - fresh.wear_of(STONE_DRESSING),
    }
}

/// **A POOL'S TOE, SUMMED OVER ITS SITES** — `hands ÷ workers_per_unit` per tool
/// (`docs/plan_pool_toe.md` §2.1), sorted by item id so a comparison is order-free.
fn required_units(config: &EquipmentConfig, sites: &[(RungKey, f32)]) -> Vec<(String, f32)> {
    let mut lines: Vec<(String, f32)> = Vec::new();
    for (rung, hands) in sites {
        for tool in config
            .pool_toe(rung.branch(), Some(&rung.wire_key()))
            .tools()
        {
            let want = hands / tool.workers_per_unit as f32;
            match lines.iter_mut().find(|(id, _)| id == tool.item.as_ref()) {
                Some((_, units)) => *units += want,
                None => lines.push((tool.item.to_string(), want)),
            }
        }
    }
    lines.sort_by(|a, b| a.0.cmp(&b.0));
    lines
}

/// ⛔ **A RUNG-TIED PLANT TOOL RESOLVES PER SITE INSTEAD OF SILENTLY RESOLVING NOTHING.**
///
/// This is the failure the arc exists to make unreachable, and it is **not** reachable on the
/// shipped roster — a hoe serves every plant rung, so the retired lookup found it. The first plant
/// tool bound to a rung would have refused a lookup that named none
/// ([`core_sim::EquipmentEffect::serves_build`]'s `(Some(_), None)` arm), the pool would have
/// resolved no kit, registered no demand for its tools, and nothing anywhere would have said so.
///
/// So the plough is a **fixture item**: no plough ships, and one is minted here from the hoes with a
/// `plant:field` bound added.
///
/// **The worked example from §2.1 is asserted literally** — Agriculture with 4 hands on tended
/// patches and 2 on a Field reads **6 hoes, 2 ploughs** — because that is the arithmetic the whole
/// model rests on, and a requirement that merely *mentioned* both tools would pass a weaker check.
#[test]
fn a_rung_tied_plant_tool_resolves_per_site_instead_of_nothing() {
    const PLOUGH: &str = "plough";
    /// §2.1's own worked example.
    const HANDS_ON_TENDED_PATCHES: f32 = 4.0;
    const HANDS_ON_A_FIELD: f32 = 2.0;

    let config = a_roster_with_a_plough(PLOUGH);

    // **(a) THE SILENT FAILURE, STATED.** No kit carries the plough, and the roster lookup the model
    // replaced answers with a *kit* — so the tool is invisible to it however loudly it declares
    // itself.
    assert!(
        !config.item_is_kit_carried(PLOUGH),
        "fixture: the plough must be in no kit, which is the state a rung-tied tool arrives in \
         before anybody adds a roster entry for it"
    );
    let derived = config
        .build_kit_for_branch(RungBranch::Plant, Some(&RungKey::PlantField.wire_key()))
        .expect("the roster still carries the tillage kit for the plant web");
    assert!(
        !derived.uses().any(|item| item == PLOUGH),
        "the kit lookup cannot see a tool no kit carries — it answers {:?}, and that is the \
         silent nothing the per-site requirement replaces",
        derived.id()
    );

    // **(b) THE REQUIREMENT SEES IT, AT THE RUNG IT SERVES AND NOWHERE ELSE.**
    assert_eq!(
        tools_for(&config, RungKey::PlantTended),
        vec![HOES.to_string()],
        "a tended patch is served by the hoe alone — the plough is bound to the rung above it"
    );
    assert_eq!(
        tools_for(&config, RungKey::PlantField),
        vec![HOES.to_string(), PLOUGH.to_string()],
        "…and a Field wants both, which is the line the retired lookup produced none of"
    );

    // **(c) §2.1's WORKED EXAMPLE, LITERALLY.**
    assert_eq!(
        required_units(
            &config,
            &[
                (RungKey::PlantTended, HANDS_ON_TENDED_PATCHES),
                (RungKey::PlantField, HANDS_ON_A_FIELD),
            ],
        ),
        vec![
            (HOES.to_string(), HANDS_ON_TENDED_PATCHES + HANDS_ON_A_FIELD),
            (PLOUGH.to_string(), HANDS_ON_A_FIELD),
        ],
        "4 hands on tended patches and 2 on a Field read 6 hoes and 2 ploughs"
    );
}

/// **The shipped roster plus a hypothetical plough bound to `plant:field`.**
///
/// It is minted from the **hoes** — same wear, same durability, same worth — so the one thing that
/// differs between the two items is the `rung` bound, and every reading below is about that bound
/// rather than about a second tool's dials.
fn a_roster_with_a_plough(id: &str) -> EquipmentConfig {
    let mut config = EquipmentConfig::builtin().as_ref().clone();
    let mut plough = config
        .items
        .get(HOES)
        .expect("the shipped roster carries the hoes")
        .clone();
    for tier in plough.tiers.iter_mut() {
        for effect in tier.effects.iter_mut() {
            effect.rung = Some(RungKey::PlantField.wire_key());
        }
    }
    config.items.insert(id.to_string(), plough);
    config
}

// ---------------------------------------------------------------------------------------------
// (2) THE SETTLEMENT IS BAND-WIDE PER TOOL, RANKED BY THE PLAYER'S OWN PRIORITY
// ---------------------------------------------------------------------------------------------

/// ⛔ **STONE-DRESSING WANTED BY `Roadwork` AND `Quarrywork` IS ONE STOCK, SERVED HIGH FIRST.**
///
/// The tool serves `route:paved_road` **and** `extraction:quarry`, so a settlement struck per pool
/// would issue the band's one unit twice. One settlement, ranked by the player's own
/// `SourcePriority`: `High` in full, then `Normal`, then `Low`.
///
/// **A road bids at the DEFAULT tier and cannot be marked** (`docs/plan_pool_toe.md` §2.2): there is
/// no per-road labor row to carry a rank, and a road's *materials* already bid exactly this for
/// exactly that reason. So the mark under test is the **quarry's**, and the road is the `Normal` it
/// is ranked against — which is what makes the pair a `High`-before-`Low` statement across two
/// pools rather than within one.
///
/// **Both arms, because either alone is satisfied by a model that always serves the same pool.**
#[test]
fn stone_dressing_shared_by_roadwork_and_quarrywork_serves_high_first() {
    let high = a_band_keeping_a_paved_road_and_a_quarry(SourcePriority::High);
    let low = a_band_keeping_a_paved_road_and_a_quarry(SourcePriority::Low);

    assert!(
        high.quarry_supplied > low.quarry_supplied,
        "a quarry marked High takes the band's one chisel ahead of the road, and the same quarry \
         marked Low does not: {} against {}",
        high.quarry_supplied,
        low.quarry_supplied
    );
    assert!(
        low.road_supplied > high.road_supplied,
        "…and the road — pinned at the default rank — is served the better of the two when the \
         quarry is marked below it: {} against {}",
        low.road_supplied,
        high.road_supplied
    );
    // ⛔ **LIVENESS: THE CHISEL IS GENUINELY SCARCE AND GENUINELY WORTH SOMETHING.** Without this
    // the ordering above is also what two fully-served pools, or two bare ones, would report. One
    // keeper stands on each pool, so a **bare** one supplies at most `PER_WORKER_OUTPUT` and a
    // geared one strictly more.
    assert!(
        high.quarry_supplied > core_sim::PER_WORKER_OUTPUT,
        "the High quarry's keeper is armed: {} against a bare hand's {}",
        high.quarry_supplied,
        core_sim::PER_WORKER_OUTPUT
    );
    assert!(
        low.quarry_supplied <= core_sim::PER_WORKER_OUTPUT,
        "…and the Low one's is not, because the band owns exactly one chisel and the road took it:          {} against a bare hand's {}",
        low.quarry_supplied,
        core_sim::PER_WORKER_OUTPUT
    );
}

/// What one arm of the shared-tool settlement put on the ground.
struct SharedToolTurn {
    road_supplied: f32,
    quarry_supplied: f32,
}

/// **A band keeping a paved road and holding a quarry, with exactly ONE set of stone-dressing gear**
/// — the two pools that reach for that tool, and a stock that cannot arm both.
fn a_band_keeping_a_paved_road_and_a_quarry(quarry_rank: SourcePriority) -> SharedToolTurn {
    const ONE_CHISEL: u32 = 1;
    const ONE_KEEPER: u32 = 1;
    const A_TAKE_CREW: u32 = 1;

    let mut app = spawn_world();
    let (band, _, band_id, home) = first_band(&mut app);
    let road_tile = tile_east_of(&app, home, 1);
    let quarry_tile = tile_east_of(&app, home, 2);

    // ⛔ **THE ROAD IS A LONG HAUL, SO ITS OWN ASK TAKES THE WHOLE CHISEL.** A route rung's upkeep
    // scales with the keeper's remoteness; kept from next door it wants well under one keeper's
    // worth, which leaves a remainder for the `Low` tier and softens the very ordering under test.
    const A_LONG_HAUL: f32 = 12.0;
    seat_road(
        &mut app,
        road_tile,
        RungKey::RoutePavedRoad,
        band_id,
        A_LONG_HAUL,
    );
    let material = seat_a_quarry(&mut app, quarry_tile);

    let staffed = {
        let mut allocation = LaborAllocation::default();
        allocation.assignments.push(core_sim::LaborAssignment {
            party: None,
            target: LaborTarget::Extract {
                tile: quarry_tile,
                material: material.clone(),
                floor: core_sim::DEFAULT_ESCAPEMENT_FLOOR,
            },
            workers: A_TAKE_CREW,
            kit: None,
            priority: quarry_rank,
            upkeep_kit: None,
        });
        for role in [LaborTarget::Roadwork, LaborTarget::Quarrywork] {
            allocation.assignments.push(core_sim::LaborAssignment {
                party: None,
                target: role,
                workers: ONE_KEEPER,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            });
        }
        let staffed: u32 = allocation.assignments.iter().map(|row| row.workers).sum();
        app.world.entity_mut(band).insert(allocation);
        staffed
    };
    size_the_band(&mut app, band, staffed);
    stock_exactly(&mut app, band, &[(STONE_DRESSING, ONE_CHISEL)]);

    app.update();

    let road_supplied = app
        .world
        .resource::<RoadRegistry>()
        .road(road_tile)
        .expect("the seated road survives the turn")
        .upkeep_supplied;
    let quarry_supplied = app
        .world
        .resource::<DepositRegistry>()
        .source(quarry_tile, &material)
        .expect("the seated working survives the turn")
        .upkeep_supplied;
    SharedToolTurn {
        road_supplied,
        quarry_supplied,
    }
}

// ---------------------------------------------------------------------------------------------
// (3) A POOL THAT IS NOT SHORT OF TOOLS IS BIT-IDENTICAL TO THE RETIRED SPLIT
// ---------------------------------------------------------------------------------------------

/// ⛔ **A BAND THAT IS NOT SHORT OF TOOLS SPLITS ITS POOL EXACTLY AS IT ALWAYS DID.**
///
/// The four-step order (§2.3) plans the hands *before* the tools are settled, at the rate each site
/// would work with its lines filled — so where the settlement then fills them, both the hands and
/// the supply are the retired arithmetic to the bit.
///
/// This restates `forage_cultivation::upkeep_kit_per_site_is_pacing_neutral_on_the_shipped_roster`
/// on the branch the arc actually changed, and it asserts against **the retired seam itself**:
/// `EquipmentConfig::keeping_kit_for` — the per-site *kit* lookup the requirement replaced — over
/// `EquipmentConfig::coverage`, which is what `keeping_rates` resolved a claim's rate through. That
/// is the claim in its falsifiable form: on a rung the kit lookup could answer for, the requirement
/// resolves the **same tools at the same worth**, so nothing moves.
///
/// **Both roads on ONE rung**, so every site's rate is equal and any difference would be the model
/// rather than the ground. Both fund modes, because `upkeep_fund_mode` still governs the split and
/// the two are different arithmetic: `Spread` scales every need by one coverage, `Priority` walks
/// the slice.
///
/// **Exactly, not nearly.** A tolerance here would pass for a model that had quietly changed the
/// pacing by a percent, which is the one outcome this change was not allowed to have.
#[test]
fn a_roadwork_pool_that_is_not_short_of_tools_splits_exactly_as_the_retired_one_did() {
    /// Short of what the two roads want between them, so the split is a live division rather than
    /// two saturated bills that would agree under any model.
    const ONE_KEEPER: u32 = 1;
    /// One tool per hand — the band is **not short**, which is the case under test.
    const A_TOOL_PER_HAND: u32 = ONE_KEEPER;
    /// ⛔ **A LONG HAUL, so the two bills genuinely outrun one keeper.** A route rung's upkeep scales
    /// with the keeper's own remoteness, and at the near reading a dirt road costs well under what
    /// one geared hand delivers — which would saturate the split and make the comparison agree under
    /// any model at all. Distance is a cost on this branch, never a wall.
    const A_LONG_HAUL: f32 = 12.0;

    for mode in [UpkeepFundMode::Spread, UpkeepFundMode::Priority] {
        let mut app = spawn_world();
        let (band, _, band_id, home) = first_band(&mut app);
        let near = tile_east_of(&app, home, 1);
        let far = tile_east_of(&app, home, 2);
        seat_road(&mut app, near, RungKey::RouteDirtRoad, band_id, A_LONG_HAUL);
        seat_road(&mut app, far, RungKey::RouteDirtRoad, band_id, A_LONG_HAUL);
        staff_one_role(&mut app, band, LaborTarget::Roadwork, ONE_KEEPER, mode);
        let ledger = stock_exactly(&mut app, band, &[(EARTHMOVING, A_TOOL_PER_HAND)]);

        // The two bills, read before the turn spends against them. The claims are sorted
        // most-invested first and tie-broken on `(y, x)`; these two stand on one rung at one
        // position on one row, so the order is west to east.
        let bills = [road_bill(&app, near), road_bill(&app, far)];
        assert!(
            bills[0] > 0.0 && bills[1] > 0.0,
            "fixture: both roads must owe something, or the split has nothing to divide: {bills:?}"
        );

        // **THE RETIRED SEAM, ASKED DIRECTLY.** `keeping_kit_for` is the per-site kit lookup the
        // requirement replaced, and `coverage` is how `keeping_rates` turned it into a rate. The
        // split it fed was already in **worker-need** units (the kit moved to the site in
        // `plan_standing_upkeep.md` §2.7), which is why the comparison is made there rather than in
        // work units: `demand ÷ r × r` is a float round trip, and a work-unit form would differ by
        // an ULP for a reason that predates this arc entirely.
        let equipment = EquipmentConfig::builtin();
        let rung = RungKey::RouteDirtRoad.wire_key();
        let retired_kit = equipment.keeping_kit_for(None, RungBranch::Route, Some(&rung));
        let retired_rate = core_sim::build_work_per_worker_turn(
            equipment
                .coverage(&retired_kit, ONE_KEEPER as f32, &ledger)
                .weighted_rate(|crew| {
                    equipment.build_work_per_worker(crew, &ledger, RungBranch::Route, Some(&rung))
                }),
        );
        assert!(
            retired_rate > core_sim::PER_WORKER_OUTPUT,
            "fixture: the retired lookup must actually find the road tool on this rung, or the \
             comparison is between two bare-handed splits — got {retired_rate}"
        );
        let needs: Vec<f32> = bills.iter().map(|bill| bill / retired_rate).collect();
        let retired: Vec<f32> = core_sim::distribute_upkeep_pool(ONE_KEEPER as f32, &needs, mode)
            .into_iter()
            .map(|hands| hands * retired_rate)
            .collect();
        assert!(
            needs[0] + needs[1] > ONE_KEEPER as f32,
            "fixture: the pool must be SHORT of both bills under {mode:?}, or a saturated split \
             would agree under any model — {needs:?} against {ONE_KEEPER} keeper"
        );

        app.update();
        let supplied = [road_supplied(&app, near), road_supplied(&app, far)];
        assert_eq!(
            supplied[0], retired[0],
            "the per-site requirement must land bit for bit on the retired kit lookup's split \
             under {mode:?}: {supplied:?} against {retired:?}"
        );
        assert_eq!(
            supplied[1], retired[1],
            "…and so must the second road's share under {mode:?}: {supplied:?} against {retired:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// (4) THE WIRE — `poolToe`, and the two fields it retires
// ---------------------------------------------------------------------------------------------

/// ⛔ **THE POOL'S TOOLS CROSS THE WIRE AS A TABLE, AND EVERY LINE STATES BOTH TERMS**
/// (`docs/plan_pool_toe.md` §4).
///
/// One `Roadwork` pool keeping a dirt road and a paved road wants two tools. The band is stocked
/// with the earthmoving gear and **none** of the stone-dressing gear, so one line is filled and one
/// is not — which is the pair a reader has to be able to tell apart:
///
/// - **a filled line KEEPS its row**, with `filled` **covering** `required`. A surface shows no tool
///   line when every line is filled, and it cannot tell *satisfied* from *not applicable* off an
///   absent row;
/// - **a line the settlement reached with nothing reads `filled == 0` beside a positive
///   `required`** — the shortfall, in the client's own `N of M` terms.
///
/// ⛔ **`filled` COVERS `required` RATHER THAN EQUALLING IT, because a tool is issued to a POOL in
/// WHOLE UNITS** (`settle_scarce_tools`). This pool wants `0.136` of the earthmoving gear — a
/// keeper's fraction of a turn on the dirt road — and is handed the whole tool that keeper carries,
/// so the line reads `filled 1.0` against `required 0.136`. The client clamps `filled` to `required`
/// before testing shortness, so an over-filled line reads as covered, which is the intended reading.
///
/// **And a pool that requires nothing has no row at all**, which is the other half of the same
/// distinction: this band staffs no `agriculture`, so the whole pool is absent rather than present
/// at zero.
///
/// Asserted on the **encoded** frame, because what a client reads is the FlatBuffer.
#[test]
fn a_pools_toe_crosses_the_wire_with_both_terms_on_every_line() {
    const ONE_KEEPER: u32 = 1;
    const ONE_TOOL: u32 = 1;
    let app = a_band_keeping_a_dirt_and_a_paved_road(&[(EARTHMOVING, ONE_TOOL)], ONE_KEEPER);
    let toe = published_pool_toe(&app);

    let earthmoving = toe
        .iter()
        .find(|line| line.pool == "roadwork" && line.item == EARTHMOVING)
        .unwrap_or_else(|| panic!("the dirt road's tool is a line of the roadwork TOE: {toe:?}"));
    assert!(
        earthmoving.required > 0.0,
        "a published line always states a real requirement: {earthmoving:?}"
    );
    assert!(
        earthmoving.filled >= earthmoving.required,
        "the band holds the earthmoving gear, and a filled line KEEPS its row rather than dropping \
         out of the table: {earthmoving:?}"
    );
    assert_eq!(
        earthmoving.filled, ONE_TOOL as f32,
        "…and the pool was issued the WHOLE tool its keeper carries — one site claims the item \
         here, so the pool's whole allocation lands on this line: {earthmoving:?}"
    );

    let dressing = toe
        .iter()
        .find(|line| line.pool == "roadwork" && line.item == STONE_DRESSING)
        .unwrap_or_else(|| panic!("the paved road's tool is a line of the same TOE: {toe:?}"));
    assert!(
        dressing.required > 0.0,
        "the paved road requires its own tool whether or not the band owns one: {dressing:?}"
    );
    assert_eq!(
        dressing.filled, 0.0,
        "…and the band owns none, so the settlement reached it with nothing: {dressing:?}"
    );

    assert!(
        !toe.iter().any(|line| line.pool == "agriculture"),
        "a pool that requires nothing has NO line — not a line at zero, which a reader could not \
         tell from an unmet one: {toe:?}"
    );
}

/// ⛔ **A POOL ROW PUBLISHES NO KIT, AND NOTHING TO BE SHORT OF** (`docs/plan_pool_toe.md` §4).
///
/// `LaborAssignment.kitId` and `kitWorkersHolding` described one kit over one row, which is the
/// shape a pool cannot have: the band above is short of a tool its pool genuinely wants, and the
/// pair would have to say so about *one* of the two. It does not try — the id is empty and the reach
/// equals the row's own head count — and the shortfall is stated on `poolToe` instead, which the
/// test above reads.
///
/// **The band IS short**, asserted here off the same frame, so this is not the trivially-satisfied
/// reading a fully-equipped band would give.
#[test]
fn a_pool_row_publishes_no_kit_and_no_shortfall() {
    const KEEPERS: u32 = 2;
    const ONE_TOOL: u32 = 1;
    let app = a_band_keeping_a_dirt_and_a_paved_road(&[(EARTHMOVING, ONE_TOOL)], KEEPERS);

    assert!(
        published_pool_toe(&app)
            .iter()
            .any(|line| line.pool == "roadwork" && line.filled < line.required),
        "fixture: the pool must be short of something, or the row below has nothing to have \
         reported"
    );
    let row = published_pool_row(&app, "roadwork");
    assert_eq!(
        row.0, "",
        "a pool row names no kit: its tools are its sites', and they ride `poolToe`"
    );
    assert_eq!(
        row.1, KEEPERS as f32,
        "…and every hand on it reads as holding what it carries, which is the `nothing to be short \
         of` reading: a reader of this pair must not see a shortfall on a pool row"
    );
}

/// **A `Roadwork` band keeping one dirt road and one paved road**, holding exactly `stock`.
///
/// The two rungs want different tools ([`a_roadwork_pool_keeping_both_rungs_requires_both_tools`]),
/// so one pool's TOE is two lines and the caller decides which of them the band can fill.
fn a_band_keeping_a_dirt_and_a_paved_road(stock: &[(&str, u32)], keepers: u32) -> App {
    /// A haul long enough that the pool is genuinely short of the two bills — the same reading the
    /// settlement fixtures above take, and for the same reason.
    const A_LONG_HAUL: f32 = 12.0;

    let mut app = spawn_world();
    let (band, _, band_id, home) = first_band(&mut app);
    let dirt = tile_east_of(&app, home, 1);
    let paved = tile_east_of(&app, home, 2);
    seat_road(&mut app, dirt, RungKey::RouteDirtRoad, band_id, A_LONG_HAUL);
    seat_road(
        &mut app,
        paved,
        RungKey::RoutePavedRoad,
        band_id,
        A_LONG_HAUL,
    );
    staff_one_role(
        &mut app,
        band,
        LaborTarget::Roadwork,
        keepers,
        UpkeepFundMode::Spread,
    );
    stock_exactly(&mut app, band, stock);
    app.update();
    app
}

/// One published `poolToe` line — see [`published_pool_toe`].
#[derive(Debug)]
struct PublishedToeLine {
    pool: String,
    item: String,
    required: f32,
    filled: f32,
}

/// **THE FIRST BAND'S PUBLISHED POOL TOEs**, decoded off the encoded frame in wire order.
fn published_pool_toe(app: &App) -> Vec<PublishedToeLine> {
    with_published_cohort(app, |cohort| {
        cohort
            .poolToe()
            .map(|lines| {
                lines
                    .iter()
                    .map(|line| PublishedToeLine {
                        pool: line.pool().unwrap_or_default().to_string(),
                        item: line.itemId().unwrap_or_default().to_string(),
                        required: line.required(),
                        filled: line.filled(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// **THE FIRST BAND'S PUBLISHED `poolCrew`**, decoded off the encoded frame in wire order —
/// `(pool token, keepers the turn's bill did not consume)` (issue #715).
fn published_pool_crew(app: &App) -> Vec<(String, f32)> {
    with_published_cohort(app, |cohort| {
        cohort
            .poolCrew()
            .map(|lines| {
                lines
                    .iter()
                    .map(|line| {
                        (
                            line.pool().unwrap_or_default().to_string(),
                            line.idleKeepers(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// **ONE POOL'S PUBLISHED IDLE KEEPERS** — see [`published_pool_crew`].
///
/// Panics where the pool states no line, because a keeping pool always states one: a pool that
/// employed every hand says so with `0`, and an absent row would be a different claim.
fn published_idle_keepers(app: &App, pool: &str) -> f32 {
    published_pool_crew(app)
        .into_iter()
        .find(|(token, _)| token == pool)
        .map(|(_, idle)| idle)
        .unwrap_or_else(|| panic!("the {pool} pool states a crew line"))
}

/// **ONE POOL'S PUBLISHED `keepers`** — the head count [`published_idle_keepers`] was struck
/// against, read off the same decoded line.
///
/// Panics where the pool states no line, for [`published_idle_keepers`]' reason.
fn published_settled_keepers(app: &App, pool: &str) -> f32 {
    with_published_cohort(app, |cohort| {
        cohort
            .poolCrew()
            .and_then(|lines| {
                lines
                    .iter()
                    .find(|line| line.pool() == Some(pool))
                    .map(|line| line.keepers())
            })
            .unwrap_or_else(|| panic!("the {pool} pool states a crew line"))
    })
}

/// **THE HEAD COUNT A POOL'S PUBLISHED LABOR ROW CARRIES** — what a client reads as the pool's
/// *current* staffing. The row is captured live off the allocation, so a command that moved it
/// shows here on the very next frame.
fn published_row_workers(app: &App, pool: &str) -> u32 {
    with_published_cohort(app, |cohort| {
        cohort
            .laborAssignments()
            .expect("the band publishes its rows")
            .iter()
            .find(|row| row.kind() == Some(pool))
            .map(|row| row.workers())
            .unwrap_or_else(|| panic!("the band staffs a '{pool}' row"))
    })
}

/// **A standing pool row's published `(kitId, kitWorkersHolding)`**, by the row's `kind` token.
fn published_pool_row(app: &App, pool: &str) -> (String, f32) {
    with_published_cohort(app, |cohort| {
        cohort
            .laborAssignments()
            .expect("the band publishes its rows")
            .iter()
            .find(|row| row.kind() == Some(pool))
            .map(|row| {
                (
                    row.kitId().unwrap_or_default().to_string(),
                    row.kitWorkersHolding(),
                )
            })
            .unwrap_or_else(|| panic!("the band staffs a '{pool}' row"))
    })
}

/// The one decode of the published frame both readers above go through — the first cohort of the
/// latest capture, read through the accessor chain a client uses.
fn with_published_cohort<T>(
    app: &App,
    read: impl FnOnce(
        shadow_scale_flatbuffers::generated::shadow_scale::sim::PopulationCohortState,
    ) -> T,
) -> T {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    let snapshot = app
        .world
        .resource::<core_sim::SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let cohort = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .get(0);
    read(cohort)
}

// ---------------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------------

fn spawn_world() -> App {
    let mut app = build_test_app();
    app.update();
    app
}

/// The campaign's first resident band: entity, faction, `BandId` and the tile it stands on.
fn first_band(app: &mut App) -> (Entity, FactionId, BandId, UVec2) {
    let (entity, faction, band, tile) = {
        let mut query = app
            .world
            .query_filtered::<(Entity, &PopulationCohort, &BandId), With<ResidentBand>>();
        let (entity, cohort, band) = query
            .iter(&app.world)
            .next()
            .expect("the campaign spawns at least one resident band");
        (entity, cohort.faction, *band, cohort.current_tile)
    };
    let position = app
        .world
        .get::<Tile>(tile)
        .expect("a band stands on a real tile")
        .position;
    app.world.insert_resource(ViewerFaction(faction));
    (entity, faction, band, position)
}

fn tile_east_of(app: &App, head: UVec2, steps: u32) -> UVec2 {
    let width = app.world.resource::<TileRegistry>().width;
    UVec2::new((head.x + steps) % width, head.y)
}

/// **Seat a road at the top of `rung`, `remoteness` tiles from the band that keeps it.**
///
/// ⛔ **THE KEEPER GOES IN FIRST AND THE SPAN IS READ AT ITS OWN REMOTENESS.** A route rung's span
/// is priced at `keeper_remoteness` (`routes::road_rung_span`), so a position computed at the near
/// reading lands lower on a remote ladder — possibly back inside the free floor, where
/// `set_position` releases the keeper and the fixture silently seats an unkept road.
fn seat_road(app: &mut App, tile: UVec2, rung: RungKey, keeper: BandId, remoteness: f32) {
    let ladder = LadderConfig::builtin();
    let (base, width) = core_sim::road_rung_span(rung, &ladder, remoteness);
    let faction = app.world.resource::<ViewerFaction>().0;
    let mut roads = app.world.resource_mut::<RoadRegistry>();
    let road = roads.road_or_trail(tile, &ladder);
    road.take_keeper(
        RoadKeeper {
            faction,
            band: keeper,
        },
        remoteness,
        &ladder,
    );
    road.set_position(base + width, &ladder);
    assert_eq!(
        road.held_rung(),
        rung,
        "fixture: the road must stand on the rung it was seated at"
    );
    assert!(
        road.keeper.is_some(),
        "fixture: the road must still be this band's job — `set_position` releases a keeper inside          the free floor"
    );
}

/// **Seat a quarry on `tile`**, re-grounding it first so the fixture does not depend on what the
/// generated map put there. Returns the material the working holds.
fn seat_a_quarry(app: &mut App, tile: UVec2) -> String {
    const STONE: &str = "stone";
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(tile.x, tile.y)
        .expect("the fixture tile is on the map");
    app.world
        .get_mut::<Tile>(entity)
        .expect("the fixture tile carries terrain")
        .terrain = TerrainType::AlpineMountain;
    let ladder = LadderConfig::builtin();
    let config = core_sim::ExtractionConfig::builtin();
    let ground = app
        .world
        .get::<Tile>(entity)
        .expect("the fixture tile carries terrain");
    let capacity = tile_deposit_capacity(&config, STONE, ground);
    assert!(capacity > 0.0, "fixture: that ground must hold stone");
    let mut working =
        DepositSource::opening(tile, STONE, capacity, RungKey::ExtractionQuarry.branch());
    let (base, width) = deposit_rung_span(RungKey::ExtractionQuarry, &ladder);
    working.set_ladder_position(base + width, &ladder, RungKey::ExtractionQuarry.branch());
    assert_eq!(
        working.rung(),
        RungKey::ExtractionQuarry,
        "fixture: seated on the wrong rung"
    );
    app.world.resource_mut::<DepositRegistry>().insert(working);
    STONE.to_string()
}

/// Put `keepers` hands on one standing role and nothing else, under `mode`.
fn staff_one_role(
    app: &mut App,
    band: Entity,
    role: LaborTarget,
    keepers: u32,
    mode: UpkeepFundMode,
) {
    let mut allocation = LaborAllocation {
        upkeep_fund_mode: mode,
        ..Default::default()
    };
    allocation.assignments.push(core_sim::LaborAssignment {
        party: None,
        target: role,
        workers: keepers,
        kit: None,
        priority: SourcePriority::default(),
        upkeep_kit: None,
    });
    app.world.entity_mut(band).insert(allocation);
    size_the_band(app, band, keepers);
}

/// **MOVE A ROLE'S HEAD COUNT THE WAY A STEPPER PRESS DOES** — straight onto the band's
/// `LaborAllocation` with **no turn in between**, which is what `handle_assign_labor` does with the
/// command (`core_sim/src/bin/server.rs`): the row moves immediately and nothing re-settles the
/// pool until the next turn resolves.
///
/// The command handler itself lives in the server **binary** and no integration test can call it;
/// what this reproduces is the state it leaves behind, which is the whole of the defect — an
/// allocation whose row has moved past the crew account the last turn stamped.
fn restaff_outside_the_turn(app: &mut App, band: Entity, role: &LaborTarget, keepers: u32) {
    {
        let mut allocation = app
            .world
            .get_mut::<LaborAllocation>(band)
            .expect("the fixture band holds an allocation");
        let assignment = allocation
            .assignments
            .iter_mut()
            .find(|assignment| assignment.target.same_source(role))
            .expect("the fixture band staffs the role under test");
        assignment.workers = keepers;
    }
    size_the_band(app, band, keepers);
}

/// **Size the cohort to exactly what it staffs**, so `LaborAllocation::normalize` never trims a row
/// under test.
fn size_the_band(app: &mut App, band: Entity, staffed: u32) {
    app.world
        .get_mut::<PopulationCohort>(band)
        .expect("the fixture band has a cohort")
        .working = core_sim::scalar_from_f32(staffed as f32);
}

/// **Give the band a ledger holding exactly these units and nothing else** — the scarcity every
/// settlement claim below is struck against. Without an explicit ledger the labour pass invents one
/// sized to the band's head count, which is never short.
fn stock_exactly(app: &mut App, band: Entity, units: &[(&str, u32)]) -> BandEquipment {
    let config = EquipmentConfig::builtin();
    let mut ledger = BandEquipment::default();
    for (item, count) in units {
        let tier = config
            .item(item)
            .unwrap_or_else(|| panic!("the shipped item table carries '{item}'"))
            .default_tier()
            .id
            .clone();
        ledger.stock(item, *count, &tier, None);
    }
    app.world.entity_mut(band).insert(ledger.clone());
    ledger
}

/// **What one road owes its keeper this turn**, in work units.
fn road_bill(app: &App, tile: UVec2) -> f32 {
    let ladder = LadderConfig::builtin();
    let road = app
        .world
        .resource::<RoadRegistry>()
        .road(tile)
        .expect("the road is in the registry");
    let terrain = app
        .world
        .resource::<TileRegistry>()
        .index(tile.x, tile.y)
        .and_then(|entity| app.world.get::<Tile>(entity))
        .expect("a seated road stands on a real tile")
        .terrain;
    core_sim::road_upkeep_demand(
        road,
        core_sim::road_upkeep_measure(terrain, road.keeper_remoteness),
        &ladder,
    )
}

fn road_supplied(app: &App, tile: UVec2) -> f32 {
    app.world
        .resource::<RoadRegistry>()
        .road(tile)
        .expect("the seated road survives the turn")
        .upkeep_supplied
}

// ---------------------------------------------------------------------------------------------
// (5) A TOOL IS ISSUED TO A PERSON — whole units BETWEEN pools, continuous WITHIN one
// ---------------------------------------------------------------------------------------------
//
// `settle_pool_tools` used to serve its claims through the `settle_scarce_store` every *material*
// claim goes through, which splits a short tier **pro-rata** — the right rule for hay out of the
// `FODDER` store and for a material upkeep bill, and the wrong one for a hoe. `settle_scarce_tools`
// is that function's sibling and is **stage 1** of the settlement the turn now runs: one bid per
// `(pool, priority tier)` group, `High` → `Normal` → `Low`, and within a short tier **largest
// remainder on the raw bid**. Stage 2 splits a group's whole allocation across that group's own
// sites, pro-rata — one pool's hands are one crew carrying their tools from site to site, so a
// fractional unit there is correct and only a different POOL is a different set of hands.
//
// The tests below drive stage 1 directly; `one_pools_two_sites_share_the_whole_tool_between_them`
// drives both stages through a real turn.

/// ⛔ **THE PLAYED CASE — band Teasel, turn 32.** Both pools bid at `Normal` on the band's two hoes:
/// the Agriculture pool `0.7906` for a plant site mid-Cultivate, the builders `2.0`. Pro-rata paid
/// them `0.5666` and `1.4334`.
///
/// **The two pools are different PEOPLE and cannot pass one hoe between them**, and nobody holds
/// 0.5666 of a hoe either. [`EquipmentConfig::coverage_from_units`] arms a **prefix** of a site's
/// hands off the units it was settled, so the fractional share left the plant site working at 72%
/// cover with its ground slipping — off a stock that could have armed it outright. In whole units
/// each pool takes one hoe, and the Agriculture pool's single site is then **not short**: its share
/// of that hoe covers its whole requirement.
///
/// The group order is the settlement's own — Agriculture first, the builders last
/// (`docs/plan_pool_toe.md` §2.3) — so this is the vector the turn really handed over. The second
/// half reads the answer back through the seam that **consumes** it, which is where being short is
/// a fact rather than a number.
#[test]
fn the_played_case_arms_both_pools_with_a_whole_hoe_each() {
    /// The Agriculture pool's bid, off the published `poolToe` line.
    const AGRICULTURE_BID: f32 = 0.7906;
    /// The builders' bid, off the same frame.
    const BUILDERS_BID: f32 = 2.0;
    /// What the band owned.
    const TWO_HOES: u32 = 2;
    /// What the retired pro-rata split paid Agriculture — the published `filled 0.5666`.
    const THE_FRACTIONAL_SHARE_THE_RETIRED_SPLIT_PAID: f32 = 0.5666;
    const ONE_HOE: f32 = 1.0;

    let settled = core_sim::settle_scarce_tools(
        &[
            (SourcePriority::Normal, AGRICULTURE_BID),
            (SourcePriority::Normal, BUILDERS_BID),
        ],
        TWO_HOES,
    );
    assert_eq!(
        settled,
        vec![ONE_HOE, ONE_HOE],
        "two hoes across a 0.79 claim and a 2.0 claim are one hoe each — never 0.5666 and 1.4334"
    );

    // **AND THE SITE IS THEREFORE NOT SHORT.** `keeping_rate_from` arms a pool's hands through
    // `coverage_from_units` over exactly these settled units, and a hoe crews one hand — so 0.7906
    // hands are covered outright by one hoe and only partly by 0.5666 of one.
    let equipment = EquipmentConfig::builtin();
    let toe = equipment.pool_toe(RungBranch::Plant, Some(&RungKey::PlantTended.wire_key()));
    let mut ledger = BandEquipment::default();
    let tier = equipment
        .item(HOES)
        .expect("the shipped roster carries the hoes")
        .default_tier()
        .id
        .clone();
    ledger.stock(HOES, TWO_HOES, &tier, None);
    // The site's hands are its requirement times what one unit crews, which is one for a hoe.
    let hands = AGRICULTURE_BID;
    let armed = equipment
        .coverage_from_units(toe.kit(), hands, &ledger, |_| settled[0])
        .workers_holding_whole_kit();
    assert_eq!(
        armed, hands,
        "the whole hoe arms every one of the site's {hands} hands — it is not short"
    );
    let armed_pro_rata = equipment
        .coverage_from_units(toe.kit(), hands, &ledger, |_| {
            THE_FRACTIONAL_SHARE_THE_RETIRED_SPLIT_PAID
        })
        .workers_holding_whole_kit();
    assert!(
        armed_pro_rata < hands,
        "…and the fractional share did not, which is the defect this replaces: {armed_pro_rata} of \
         {hands} hands armed"
    );
}

/// ⛔ **EVERY POOL IS SETTLED A WHOLE NUMBER OF TOOLS** — the claim the whole change is about,
/// asserted directly over a tier the stock cannot cover.
///
/// Four **pools** whose bids are fractional or over-large, so a pro-rata split would land every one
/// of them on a fraction of a tool no crew can carry.
#[test]
fn every_settled_tool_is_a_whole_unit() {
    const BIDS: [f32; 4] = [0.7906, 2.0, 1.25, 3.5];
    const A_SHORT_STOCK: u32 = 4;

    let demands: Vec<(SourcePriority, f32)> = BIDS
        .iter()
        .map(|bid| (SourcePriority::Normal, *bid))
        .collect();
    let settled = core_sim::settle_scarce_tools(&demands, A_SHORT_STOCK);

    assert!(
        settled.iter().all(|units| units.fract() == 0.0),
        "no pool may hold a fraction of a tool, because no crew can carry one: {settled:?}"
    );
    assert_eq!(
        settled.iter().sum::<f32>(),
        A_SHORT_STOCK as f32,
        "…and a short tier consumes the stock exactly, so the whole units are not bought by \
         throwing gear away: {settled:?}"
    );
}

/// ⛔ **A TRIVIAL POOL DOES NOT EAT A LARGE ONE'S TOOL.**
///
/// Both bids `ceil` to a whole tool, so a tier ranked on what each pool *wants* would let the `0.2`
/// pool tie with the `2.0` one for the single unit on the shelf — and the fixed group order
/// (Agriculture, Husbandry, Roadwork, Quarrywork, builders) would then settle that tie by vector
/// position, arming Agriculture first on every band in the game. Ranked on the **raw bid**, the tool
/// goes to the pool that needs it.
#[test]
fn a_trivial_pool_does_not_eat_a_large_ones_tool() {
    const A_TRIVIAL_BID: f32 = 0.2;
    const A_LARGE_BID: f32 = 2.0;
    const ONE_TOOL: u32 = 1;

    assert_eq!(
        core_sim::settle_scarce_tools(
            &[
                (SourcePriority::Normal, A_TRIVIAL_BID),
                (SourcePriority::Normal, A_LARGE_BID),
            ],
            ONE_TOOL,
        ),
        vec![0.0, 1.0],
        "the band's one tool goes to the pool bidding 2.0, not to the 0.2 pool that happens to \
         stand earlier in the vector"
    );
}

/// ⛔ **THE PLAYER'S RANK STILL OUTRANKS SIZE.** A `High` group bidding `0.3` takes the band's last
/// tool from a `Normal` group bidding `5.0`: the tiers are served in order, and largest remainder is
/// a rule *within* one tier and never across two. The group key is `(pool, tier)` rather than the
/// pool alone precisely so a site's own rank still decides where the tools go.
#[test]
fn priority_still_outranks_size() {
    const A_MARKED_TRICKLE: f32 = 0.3;
    const AN_UNMARKED_TORRENT: f32 = 5.0;
    const ONE_TOOL: u32 = 1;

    assert_eq!(
        core_sim::settle_scarce_tools(
            &[
                (SourcePriority::Normal, AN_UNMARKED_TORRENT),
                (SourcePriority::High, A_MARKED_TRICKLE),
            ],
            ONE_TOOL,
        ),
        vec![0.0, 1.0],
        "the High group is served first and in full, whatever the Normal group beside it bid"
    );
}

/// ⛔ **NOTHING IS HANDED OUT TWICE** — `Σ settled <= available` across all three tiers and five
/// groups, which is what a per-tier remainder that failed to shrink would break.
#[test]
fn no_tool_is_handed_out_twice_across_three_tiers() {
    const A_SHORT_STOCK: u32 = 3;
    let demands = [
        (SourcePriority::High, 1.5),
        (SourcePriority::Normal, 2.2),
        (SourcePriority::Normal, 0.9),
        (SourcePriority::Low, 0.4),
        (SourcePriority::Low, 3.0),
    ];

    let settled = core_sim::settle_scarce_tools(&demands, A_SHORT_STOCK);
    assert!(
        settled.iter().sum::<f32>() <= A_SHORT_STOCK as f32,
        "the three tiers between them may not issue more than the band owns: {settled:?} off \
         {A_SHORT_STOCK}"
    );
    // **LIVENESS**: the stock really was spent, so the bound above is not the trivially-satisfied
    // reading a settlement that paid nobody would give.
    assert_eq!(
        settled.iter().sum::<f32>(),
        A_SHORT_STOCK as f32,
        "…and every tool is issued to somebody: {settled:?}"
    );
    assert_eq!(
        settled[0], 2.0,
        "the High group is armed to its own ceil ahead of both tiers beneath it: {settled:?}"
    );
}

/// ⛔ **THE SPLIT IS DETERMINISTIC, AND A TIE GOES TO THE EARLIER GROUP.** Two equal `Normal` pools
/// against an odd count cannot both take the odd tool; which of them does must not depend on float
/// ordering or on a sort's stability, because a turn is replayed from a checkpoint.
#[test]
fn an_odd_tool_between_two_equal_pools_goes_to_the_earlier_one_every_run() {
    const AN_EQUAL_BID: f32 = 2.0;
    const AN_ODD_STOCK: u32 = 3;
    /// Enough repetitions that an ordering decided by anything but the rule would show.
    const RUNS: usize = 32;

    let demands = [
        (SourcePriority::Normal, AN_EQUAL_BID),
        (SourcePriority::Normal, AN_EQUAL_BID),
    ];
    let first = core_sim::settle_scarce_tools(&demands, AN_ODD_STOCK);
    assert_eq!(
        first,
        vec![2.0, 1.0],
        "the tied remainder goes to the earlier pool, and the later one takes what is left"
    );
    for _ in 0..RUNS {
        assert_eq!(
            core_sim::settle_scarce_tools(&demands, AN_ODD_STOCK),
            first,
            "the same pools settle the same way on every run"
        );
    }
}

/// **A tier the stock covers is paid every pool's whole want** — `ceil(demand)`, because a crew
/// cannot carry a fraction of a tool — and the remainder falls through to the tier beneath it. This
/// is the regime a band that is **not short** lives in, where the change may move no rate.
#[test]
fn a_covered_tier_is_paid_its_whole_want_and_passes_the_rest_down() {
    const A_FULL_SHELF: u32 = 6;

    assert_eq!(
        core_sim::settle_scarce_tools(
            &[
                (SourcePriority::High, 1.2),
                (SourcePriority::Normal, 2.0),
                (SourcePriority::Low, 0.1),
            ],
            A_FULL_SHELF,
        ),
        vec![2.0, 2.0, 1.0],
        "each pool is armed to its own ceil, and six tools cover all three tiers"
    );
}

/// **A pool that asks for nothing settles nothing**, exactly as `settle_scarce_store` skips it — so
/// a pool naming a tool it needs none of never holds one while a bidder goes bare.
#[test]
fn a_pool_that_asks_for_nothing_is_handed_nothing() {
    const ONE_TOOL: u32 = 1;

    assert_eq!(
        core_sim::settle_scarce_tools(
            &[(SourcePriority::High, 0.0), (SourcePriority::Normal, 0.5)],
            ONE_TOOL,
        ),
        vec![0.0, 1.0],
        "the High pool demanded nothing, so the tool falls to the Normal pool that did"
    );
}

/// **An empty shelf arms nobody and the settlement terminates saying so** — the largest-remainder
/// pass's lap guard, stated as a behaviour rather than as a comment.
#[test]
fn an_empty_shelf_arms_nobody() {
    const NOTHING_ON_THE_SHELF: u32 = 0;

    assert_eq!(
        core_sim::settle_scarce_tools(
            &[(SourcePriority::High, 1.0), (SourcePriority::Normal, 2.5)],
            NOTHING_ON_THE_SHELF,
        ),
        vec![0.0, 0.0],
        "a band that owns no tools arms nobody"
    );
}

/// ⛔ **ONE POOL'S TWO SITES SHARE THE WHOLE TOOL BETWEEN THEM, AND NEITHER IS SHORT** — stage 2,
/// through a real turn.
///
/// A `Roadwork` pool of **one keeper** funding **two** dirt roads splits its hands `0.5 / 0.5`, so
/// each site requires half a set of earthmoving gear and the pool requires one. **The band owns
/// exactly one**, and that one tool is the keeper's: they carry it from the first road to the second
/// as they split their turn between them. Each site is settled half a unit — a *fraction* of a tool,
/// which is correct here and meaningless between pools — and both roads are kept at the geared rate.
///
/// **The falsifiable claim is the EQUALITY.** Were the whole-unit rule struck per *site* rather than
/// per pool, each site's `0.5` would `ceil` to a whole tool, the pair would want two, and the
/// largest-remainder pass would arm one road and leave the other bare-handed — so the two roads'
/// supply would differ. They must not.
///
/// The bare control is the liveness half: the same band owning **no** tool supplies strictly less on
/// both roads, so *"equal"* above is not the trivial truth about two bare-handed keepers.
#[test]
fn one_pools_two_sites_share_the_whole_tool_between_them() {
    /// One keeper, two roads: the hands split `0.5 / 0.5` and the pool wants one whole set.
    const ONE_KEEPER: u32 = 1;
    const ONE_TOOL: u32 = 1;
    /// A haul long enough that both bills genuinely outrun the keeper, so each road's share of the
    /// supply is the rate it worked at rather than a saturated bill.
    const A_LONG_HAUL: f32 = 12.0;

    let geared = a_roadwork_pool_keeping_two_dirt_roads(ONE_TOOL, ONE_KEEPER, A_LONG_HAUL);
    let bare = a_roadwork_pool_keeping_two_dirt_roads(0, ONE_KEEPER, A_LONG_HAUL);

    let line = published_pool_toe(&geared.app)
        .into_iter()
        .find(|line| line.pool == "roadwork" && line.item == EARTHMOVING)
        .unwrap_or_else(|| panic!("the roadwork pool states its earthmoving line"));
    assert!(
        line.required > 0.0 && line.filled >= line.required,
        "the pool wants one whole set for its two half-hands and holds one, so its line is \
         covered: {line:?}"
    );
    assert_eq!(
        line.filled, ONE_TOOL as f32,
        "…and what it holds is the one whole tool the band owns: {line:?}"
    );

    assert_eq!(
        geared.supplied, [geared.supplied[0]; 2],
        "the keeper carries their one tool to BOTH roads, so the two sites are kept at the same \
         rate — a per-SITE whole-unit rule would arm one and leave the other bare: {:?}",
        geared.supplied
    );
    assert!(
        geared.supplied[0] > bare.supplied[0] && geared.supplied[1] > bare.supplied[1],
        "liveness: the tool is really doing something on both roads — geared {:?} against bare {:?}",
        geared.supplied,
        bare.supplied
    );
}

/// One `Roadwork` turn over two dirt roads, and what each road was supplied.
struct TwoRoadTurn {
    app: App,
    supplied: [f32; 2],
}

/// **A band keeping two dirt roads with one `Roadwork` pool**, holding `tools` sets of earthmoving
/// gear. Both roads stand on one rung at one remoteness, so their bills — and therefore the hands
/// the pool splits between them — are equal, and any difference in what they are supplied is the
/// settlement rather than the ground.
fn a_roadwork_pool_keeping_two_dirt_roads(tools: u32, keepers: u32, haul: f32) -> TwoRoadTurn {
    let mut app = spawn_world();
    let (band, _, band_id, home) = first_band(&mut app);
    let near = tile_east_of(&app, home, 1);
    let far = tile_east_of(&app, home, 2);
    seat_road(&mut app, near, RungKey::RouteDirtRoad, band_id, haul);
    seat_road(&mut app, far, RungKey::RouteDirtRoad, band_id, haul);
    staff_one_role(
        &mut app,
        band,
        LaborTarget::Roadwork,
        keepers,
        UpkeepFundMode::Spread,
    );
    stock_exactly(&mut app, band, &[(EARTHMOVING, tools)]);

    app.update();

    let supplied = [road_supplied(&app, near), road_supplied(&app, far)];
    TwoRoadTurn { app, supplied }
}

// ---------------------------------------------------------------------------------------------
// (6) STEP 5 — THE HANDS NOBODY TOOK GO TO THE WORK STILL OWED (issue #714)
// ---------------------------------------------------------------------------------------------
//
// **A site owing N units of work is owed N units of work.** Steps 1–4 plan a pool's hands at the
// rate their tools *would* buy and then never look at whether the work arrived, so a site the
// band-wide settlement left bare works its own hands slower while hands the plan never allocated
// stand idle. Step 5 puts those hands — and only those — on whatever deficit is left, **bare**
// (`docs/plan_pool_toe.md` §2.3 step 5).
//
// Every reading here comes off the shipped path: the bill through `road_bill` /
// `deposit_keeping_basis`, the supply off the registry the turn wrote, and the hands off the
// **published** `poolToe.required` — `earthmoving` and `stone_dressing` are both
// `workers_per_unit: 1`, so a pool's required units ARE the hands the split put on its sites. That
// published requirement must **not** grow by the top-up: a bare hand claims no tool.
mod a_pool_puts_its_idle_hands_on_the_work_still_owed {
    use super::*;

    /// The haul every fixture above uses, so the bills genuinely outrun one keeper.
    const A_LONG_HAUL: f32 = 12.0;
    /// **A haul short enough that the two roads' need does not outrun the pool.** A route bill
    /// scales linearly on the keeper's remoteness (`routes::road_upkeep_measure`), so this is the
    /// one lever that decides whether a three-keeper pool has a hand to spare at all — at
    /// [`A_LONG_HAUL`] two dirt roads want 3.96 hands and there is no surplus to strand.
    const A_SHORT_HAUL: f32 = 4.0;

    /// **What one turn measured.** `bills` are read BEFORE the turn, `supplied` after.
    struct Measured {
        app: App,
        bills: Vec<f32>,
        supplied: Vec<f32>,
        /// The band's ledger as stocked, so the plan rate can be struck off the same tools the
        /// planner reads.
        ledger: BandEquipment,
    }

    /// **The rate `fully_equipped_keeper_rate` answers for this rung off this ledger** — its body,
    /// through the public seams (`pool_toe` → `build_work_per_worker` →
    /// `build_work_per_worker_turn`), which is how
    /// `a_roadwork_pool_that_is_not_short_of_tools_splits_exactly_as_the_retired_one_did` already
    /// reads a rate from outside.
    fn plan_rate(ledger: &BandEquipment, rung: RungKey) -> f32 {
        let equipment = EquipmentConfig::builtin();
        let key = rung.wire_key();
        let toe = equipment.pool_toe(rung.branch(), Some(&key));
        core_sim::build_work_per_worker_turn(equipment.build_work_per_worker(
            toe.kit(),
            ledger,
            rung.branch(),
            Some(&key),
        ))
    }

    /// One tool unit arms one hand for both pool tools under test, asserted rather than assumed —
    /// it is what lets `poolToe.required` be read as a head count.
    fn one_unit_arms_one_hand(rung: RungKey) -> bool {
        EquipmentConfig::builtin()
            .pool_toe(rung.branch(), Some(&rung.wire_key()))
            .tools()
            .iter()
            .all(|tool| tool.workers_per_unit == 1)
    }

    /// The published `(required, filled)` of one pool's line for one item.
    fn toe_line(app: &App, pool: &str, item: &str) -> (f32, f32) {
        published_pool_toe(app)
            .iter()
            .find(|line| line.pool == pool && line.item == item)
            .map(|line| (line.required, line.filled))
            .unwrap_or_else(|| panic!("the {pool} pool states a '{item}' line"))
    }

    /// **A `Roadwork` pool keeping `roads` dirt roads**, holding exactly `stock`.
    fn a_roadwork_pool_over_dirt_roads(
        stock: &[(&str, u32)],
        keepers: u32,
        roads: u32,
        haul: f32,
    ) -> Measured {
        let mut app = spawn_world();
        let (band, _, band_id, home) = first_band(&mut app);
        let tiles: Vec<UVec2> = (1..=roads)
            .map(|step| tile_east_of(&app, home, step))
            .collect();
        for tile in &tiles {
            seat_road(&mut app, *tile, RungKey::RouteDirtRoad, band_id, haul);
        }
        staff_one_role(
            &mut app,
            band,
            LaborTarget::Roadwork,
            keepers,
            UpkeepFundMode::Spread,
        );
        let ledger = stock_exactly(&mut app, band, stock);
        let bills: Vec<f32> = tiles.iter().map(|tile| road_bill(&app, *tile)).collect();
        app.update();
        let supplied: Vec<f32> = tiles
            .iter()
            .map(|tile| road_supplied(&app, *tile))
            .collect();
        Measured {
            app,
            bills,
            supplied,
            ledger,
        }
    }

    /// ⛔ **A BAND THAT OWNS NO GEAR AT ALL PLANS AT THE BARE RATE, SO NOTHING IS STRANDED** — the
    /// half of #714 that does **not** reproduce, kept because it is the reading the issue's stated
    /// trigger rests on.
    ///
    /// `fully_equipped_keeper_rate` reads the band's ledger for the tools' tier and condition, and
    /// an empty ledger has neither: `KitChoice::best_build_work` finds no live item and the rate
    /// falls back to the bare hand. The need is therefore struck at `bill ÷ 1.0`, outruns the two
    /// keepers, and **both of them are put to work** — *"no hoe"* cannot by itself leave a keeper
    /// standing.
    #[test]
    fn a_band_with_no_gear_at_all_plans_bare_and_works_every_keeper() {
        const KEEPERS: u32 = 2;
        const ONE_ROAD: u32 = 1;
        /// How close the planned hands must come to the head count for *"every keeper is working"*
        /// to be a true statement. `distribute_upkeep_pool` scales each need by one coverage, so
        /// the shares sum to the pool only to within float error.
        const A_WHOLE_POOL: f32 = 1.0e-5;

        assert!(
            one_unit_arms_one_hand(RungKey::RouteDirtRoad),
            "fixture: a dirt road's tool arms one hand per unit, which is what lets the published \
             requirement be read as a head count"
        );
        let turn = a_roadwork_pool_over_dirt_roads(&[], KEEPERS, ONE_ROAD, A_LONG_HAUL);
        let rate = plan_rate(&turn.ledger, RungKey::RouteDirtRoad);
        let (required, filled) = toe_line(&turn.app, "roadwork", EARTHMOVING);

        assert_eq!(
            rate,
            core_sim::PER_WORKER_OUTPUT,
            "an empty ledger plans at the BARE hand, not at the gear the band does not own"
        );
        assert!(
            turn.bills[0] / rate > KEEPERS as f32,
            "fixture: the bill must outrun the pool at that rate, or the cap rather than the \
             shortage is what this measures — {} over {KEEPERS}",
            turn.bills[0] / rate
        );
        assert!(
            (KEEPERS as f32 - required).abs() < A_WHOLE_POOL,
            "both keepers are put on the road: {required} of {KEEPERS}"
        );
        assert_eq!(
            filled, 0.0,
            "…and the settlement reached the line with nothing, which is the state the issue \
             describes: {filled}"
        );
        assert!(
            (turn.supplied[0] - KEEPERS as f32 * core_sim::PER_WORKER_OUTPUT).abs() < A_WHOLE_POOL,
            "so the road is supplied two bare hands' work: {}",
            turn.supplied[0]
        );
    }

    /// ⛔ **A POOL WHOSE PLAN WANTS EVERY HAND IT HAS IS UNTOUCHED — the control.**
    ///
    /// Two dirt roads at [`A_LONG_HAUL`] want `3.96` keepers between them and the pool has **3**,
    /// so `distribute_upkeep_pool`'s coverage binds from below and there is no idle hand for step 5
    /// to spend. Both roads stay short by exactly what they were short of before step 5 existed.
    ///
    /// **Without this, *"the fix closes shortfalls"* would also pass on a step 5 that fired where
    /// nothing was spare** — which would be the re-split step 4 refuses, arriving by the back door.
    #[test]
    fn a_pool_whose_plan_wants_every_hand_supplies_exactly_what_it_did_before() {
        const KEEPERS: u32 = 3;
        const TWO_ROADS: u32 = 2;
        const ONE_TOOL: u32 = 1;
        /// **What each road was supplied before step 5 existed**, measured on this fixture: `1.5`
        /// hands at the `1.667` rate one tool split two ways buys them.
        const THE_SPLIT_THIS_POOL_ALREADY_MADE: f32 = 2.5;

        let turn = a_roadwork_pool_over_dirt_roads(
            &[(EARTHMOVING, ONE_TOOL)],
            KEEPERS,
            TWO_ROADS,
            A_LONG_HAUL,
        );
        let rate = plan_rate(&turn.ledger, RungKey::RouteDirtRoad);
        let (required, _) = toe_line(&turn.app, "roadwork", EARTHMOVING);

        assert!(
            (turn.bills[0] + turn.bills[1]) / rate > KEEPERS as f32,
            "fixture: the two bills must outrun the pool, or there would be a surplus and this \
             would not be a control — {} over {KEEPERS}",
            (turn.bills[0] + turn.bills[1]) / rate
        );
        assert_eq!(
            required, KEEPERS as f32,
            "every keeper is already planned onto a road, so nothing is idle: {required}"
        );
        assert_eq!(
            turn.supplied,
            vec![
                THE_SPLIT_THIS_POOL_ALREADY_MADE,
                THE_SPLIT_THIS_POOL_ALREADY_MADE
            ],
            "…and both roads are supplied exactly what the pool supplied them before step 5"
        );
        assert!(
            turn.supplied[0] < turn.bills[0],
            "liveness: these roads really are short — a saturated pair would be bit-identical \
             under any model at all: {:?} against {:?}",
            turn.supplied,
            turn.bills
        );
    }

    /// ⛔ **TWO ROADS SHARING ONE TOOL CLOSE THEIR SHORTFALL OUT OF THE IDLE HANDS.**
    ///
    /// At [`A_SHORT_HAUL`] the two bills want `1.32` keepers of the pool's **3**, so the plan caps
    /// each road at its own need and `1.68` hands are left standing. The band owns **one**
    /// earthmoving set against a requirement of `1.32`, so both roads worked below the rate they
    /// were planned at and each fell `0.32` work units short — while those `1.68` hands did
    /// nothing.
    ///
    /// **The roads share one rank**, because `route_keeping_claims` pins every road at
    /// `SourcePriority::default()`: there is no per-road labor row to carry a mark, so the pair is
    /// half-armed together rather than one being served and one going bare.
    #[test]
    fn two_roads_sharing_one_tool_close_their_shortfall_from_the_idle_hands() {
        const KEEPERS: u32 = 3;
        const TWO_ROADS: u32 = 2;
        const ONE_TOOL: u32 = 1;

        let turn = a_roadwork_pool_over_dirt_roads(
            &[(EARTHMOVING, ONE_TOOL)],
            KEEPERS,
            TWO_ROADS,
            A_SHORT_HAUL,
        );
        let rate = plan_rate(&turn.ledger, RungKey::RouteDirtRoad);
        let (required, filled) = toe_line(&turn.app, "roadwork", EARTHMOVING);

        // **The two conditions that make this the defect rather than ordinary scarcity.**
        assert!(
            required < KEEPERS as f32,
            "fixture: the plan must leave a hand standing, or there is nothing to put to work — \
             {required} of {KEEPERS}"
        );
        assert!(
            filled < required,
            "fixture: and the pool must be short of the tool those hands were planned at, or \
             there is no deficit to close: filled {filled} against required {required}"
        );
        assert_eq!(
            required,
            (turn.bills[0] + turn.bills[1]) / rate,
            "⛔ the published requirement is the GEARED plan's hands and nothing else — a bare \
             top-up hand claims no tool, so step 5 may not grow this line"
        );

        assert_eq!(
            turn.supplied, turn.bills,
            "both roads are supplied the whole of what they owe, out of hands that were idle"
        );
        // ⛔ **LIVENESS — THE PLANNED HANDS COULD NOT HAVE DONE IT ON THEIR OWN.** Coverage arms a
        // **prefix** of a site's hands, so the most the geared plan can deliver is its armed hands
        // at the geared rate plus the rest of them bare. That ceiling is struck off the published
        // line alone, and it is strictly under the two bills — so *"both roads are covered"* above
        // cannot be a split that was saturated all along.
        let the_most_the_planned_hands_could_do =
            filled * rate + (required - filled) * core_sim::PER_WORKER_OUTPUT;
        assert!(
            the_most_the_planned_hands_could_do < turn.bills[0] + turn.bills[1],
            "liveness: the planned hands reach at most {the_most_the_planned_hands_could_do} of \
             the {} owed, so what closed the gap is the top-up",
            turn.bills[0] + turn.bills[1]
        );
    }

    /// ⛔ **THE WORKING THAT LOST THE TOOL SETTLEMENT IS FILLED BY THE IDLE KEEPERS** — #714 as
    /// titled, on the one pool whose sites can carry a rank.
    ///
    /// A road bids at `SourcePriority::default()` and cannot be marked, so the High/Low pair the
    /// issue describes is unreachable on the route branch. Two quarries under one `Quarrywork` pool
    /// can: one `High`, one `Low`, one set of stone-dressing gear between them and **3** keepers
    /// against a plan that wants `1.4`.
    ///
    /// Stage 1 gives the whole tool to the `High` group, so the `Low` working's `0.7` planned hands
    /// worked **bare** and delivered `0.7` of the `2.1` it owed — while `1.6` keepers stood idle.
    /// Those keepers close it exactly, and **the `High` working is not touched**, because step 5
    /// only ever assigns hands nobody took.
    #[test]
    fn the_working_that_lost_the_tool_settlement_is_filled_by_the_idle_keepers() {
        const KEEPERS: u32 = 3;
        const ONE_TOOL: u32 = 1;
        const A_TAKE_CREW: u32 = 1;

        let mut app = spawn_world();
        let (band, _, _, home) = first_band(&mut app);
        let high_tile = tile_east_of(&app, home, 1);
        let low_tile = tile_east_of(&app, home, 2);
        let high_material = seat_a_quarry(&mut app, high_tile);
        let low_material = seat_a_quarry(&mut app, low_tile);

        let staffed = {
            let mut allocation = LaborAllocation::default();
            for (tile, material, rank) in [
                (high_tile, high_material.clone(), SourcePriority::High),
                (low_tile, low_material.clone(), SourcePriority::Low),
            ] {
                allocation.assignments.push(core_sim::LaborAssignment {
                    target: LaborTarget::Extract {
                        tile,
                        material,
                        floor: core_sim::DEFAULT_ESCAPEMENT_FLOOR,
                    },
                    workers: A_TAKE_CREW,
                    kit: None,
                    priority: rank,
                    upkeep_kit: None,
                });
            }
            allocation.assignments.push(core_sim::LaborAssignment {
                target: LaborTarget::Quarrywork,
                workers: KEEPERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            });
            let staffed: u32 = allocation.assignments.iter().map(|row| row.workers).sum();
            app.world.entity_mut(band).insert(allocation);
            staffed
        };
        size_the_band(&mut app, band, staffed);
        let ledger = stock_exactly(&mut app, band, &[(STONE_DRESSING, ONE_TOOL)]);

        let bills = quarry_bills(
            &app,
            [(high_tile, &high_material), (low_tile, &low_material)],
        );

        app.update();

        let supplied = {
            let deposits = app.world.resource::<DepositRegistry>();
            [
                deposits
                    .source(high_tile, &high_material)
                    .expect("the High working survives the turn")
                    .upkeep_supplied,
                deposits
                    .source(low_tile, &low_material)
                    .expect("the Low working survives the turn")
                    .upkeep_supplied,
            ]
        };
        let rate = plan_rate(&ledger, RungKey::ExtractionQuarry);
        let (required, filled) = toe_line(&app, "quarrywork", STONE_DRESSING);

        assert!(
            one_unit_arms_one_hand(RungKey::ExtractionQuarry),
            "fixture: the quarry's tool arms one hand per unit"
        );
        assert!(
            required < KEEPERS as f32,
            "fixture: the plan must leave keepers standing — {required} of {KEEPERS}"
        );
        assert!(
            filled < required,
            "fixture: and one of the two groups must have lost the settlement: filled {filled} \
             against required {required}"
        );
        assert_eq!(
            required,
            (bills[0] + bills[1]) / rate,
            "⛔ the published requirement states the GEARED plan's hands alone"
        );

        assert_eq!(
            supplied[0], bills[0],
            "the High working keeps the tool and is supplied its whole bill, exactly as before"
        );
        assert_eq!(
            supplied[1], bills[1],
            "…and the Low working, which the settlement reached with nothing, is filled the rest \
             of the way by the keepers the plan left idle"
        );
        assert!(
            supplied[1] > required / 2.0 * core_sim::PER_WORKER_OUTPUT,
            "liveness: strictly more than its own bare-handed planned hands delivered, which is \
             what it was paid before step 5: {} against {}",
            supplied[1],
            required / 2.0 * core_sim::PER_WORKER_OUTPUT
        );
    }

    /// ⛔ **A POOL WITH HANDS TO SPARE AND NOTHING OWED SUPPLIES EXACTLY THE BILL — the second
    /// control.**
    ///
    /// The same two short-haul roads and the same three keepers, with **two** earthmoving sets: the
    /// requirement is covered, both roads are supplied in full by their geared hands alone, and
    /// `1.68` keepers are still idle. Step 5 must find no deficit and hand them nothing — a top-up
    /// struck against anything but the *remaining* gap would push a road past what it owes.
    #[test]
    fn a_pool_with_idle_hands_and_nothing_owed_supplies_exactly_the_bill() {
        const KEEPERS: u32 = 3;
        const TWO_ROADS: u32 = 2;
        const A_TOOL_PER_ROAD: u32 = 2;

        let turn = a_roadwork_pool_over_dirt_roads(
            &[(EARTHMOVING, A_TOOL_PER_ROAD)],
            KEEPERS,
            TWO_ROADS,
            A_SHORT_HAUL,
        );
        let (required, filled) = toe_line(&turn.app, "roadwork", EARTHMOVING);

        assert!(
            required < KEEPERS as f32,
            "fixture: keepers must be left standing, or this control says nothing about them — \
             {required} of {KEEPERS}"
        );
        assert!(
            filled >= required,
            "fixture: and the pool must be armed in full, so there is no deficit at all: filled \
             {filled} against required {required}"
        );
        assert_eq!(
            turn.supplied, turn.bills,
            "each road is supplied exactly what it owes — never more, however many hands the pool \
             still has standing"
        );
    }

    /// **What each working owes its keepers this turn**, in work units, read before the turn spends
    /// against it — `road_bill`'s deposit twin, through the same
    /// `extraction::deposit_keeping_basis` seam the claim builder reads.
    fn quarry_bills(app: &App, workings: [(UVec2, &str); 2]) -> [f32; 2] {
        let ladder = LadderConfig::builtin();
        let config = core_sim::ExtractionConfig::builtin();
        let read = |tile: UVec2, material: &str| {
            let entity = app
                .world
                .resource::<TileRegistry>()
                .index(tile.x, tile.y)
                .expect("the fixture tile is on the map");
            let ground = app.world.get::<Tile>(entity).expect("the tile has terrain");
            let working = app
                .world
                .resource::<DepositRegistry>()
                .source(tile, material)
                .expect("the seated working is in the registry");
            let measure = core_sim::extraction::deposit_measure(working, ground, &config);
            core_sim::extraction::deposit_keeping_basis(working, measure, &ladder)
        };
        [
            read(workings[0].0, workings[0].1),
            read(workings[1].0, workings[1].1),
        ]
    }

    // -----------------------------------------------------------------------------------------
    // (6b) …AND THE WIRE SAYS HOW MANY KEEPERS IT DID NOT USE (issue #715)
    // -----------------------------------------------------------------------------------------
    //
    // **The number a *"step this pool down"* mark is drawn off**, published per pool as
    // `PopulationCohortState.poolCrew`. It is the sim's to state and not a client's to derive: a
    // client projecting a pool's supply off a *notional* kit knows neither which tools the band's
    // settlement handed this pool nor that step 5 puts leftover hands back onto sites still short,
    // so its answer would be wrong in exactly the cases the section above is about.
    //
    // Every reading here comes off the **encoded** frame, because what a client reads is the
    // FlatBuffer.
    mod and_the_wire_says_how_many_keepers_it_did_not_use {
        use super::*;

        /// ⛔ **THE ISSUE'S OWN CASE — a bill one geared keeper covers, with a second keeper
        /// assigned, publishes the second keeper as standing.**
        ///
        /// One short-haul dirt road wants `0.66` of a keeper at the geared rate and the band owns
        /// the one tool that arms them, so the road is paid in full out of the plan alone and step
        /// 5 has no deficit to spend anything on. The pool was given **2** keepers, so `1.34` of
        /// them did nothing at all — more than a whole person, which is the reading a stepper acts
        /// on.
        #[test]
        fn a_bill_one_geared_keeper_covers_leaves_the_second_keeper_standing() {
            const KEEPERS: u32 = 2;
            const ONE_ROAD: u32 = 1;
            const ONE_TOOL: u32 = 1;
            /// **What this fixture strands**, measured: `2` keepers less the `0.66` the road's
            /// bill asked for at the geared rate.
            const THE_KEEPERS_THE_BILL_NEVER_REACHED_FOR: f32 = 1.3399999;
            /// A whole person standing is what makes this the issue rather than a rounding
            /// remainder.
            const A_WHOLE_KEEPER: f32 = 1.0;

            let turn = a_roadwork_pool_over_dirt_roads(
                &[(EARTHMOVING, ONE_TOOL)],
                KEEPERS,
                ONE_ROAD,
                A_SHORT_HAUL,
            );
            let (required, filled) = toe_line(&turn.app, "roadwork", EARTHMOVING);
            let idle = published_idle_keepers(&turn.app, "roadwork");

            assert!(
                filled >= required,
                "fixture: the hands the bill wanted are armed, so there is no deficit for step 5 \
                 to spend the spare keeper on: filled {filled} against required {required}"
            );
            assert_eq!(
                turn.supplied, turn.bills,
                "fixture: …and the road is paid in full by the geared plan alone"
            );
            assert_eq!(
                idle, THE_KEEPERS_THE_BILL_NEVER_REACHED_FOR,
                "the wire states the keepers the turn's bill did not consume"
            );
            assert_eq!(
                idle,
                KEEPERS as f32 - required,
                "…which is the head count less the hands the plan put on the road"
            );
            assert!(
                idle > A_WHOLE_KEEPER,
                "…and it is a whole person and more, which is what a step-down mark acts on: \
                 {idle}"
            );
        }

        /// ⛔ **THE CONTROL THAT MAKES THE CLAIM MEAN SOMETHING — the bare-handed pair.**
        ///
        /// Two keepers on the **same** road with no tool at all report **no** idle keeper, because
        /// neither can be freed: an empty ledger plans at the bare hand
        /// ([`a_band_with_no_gear_at_all_plans_bare_and_works_every_keeper`]), the need outruns the
        /// pool, and both of them are on the road.
        ///
        /// **A change that cannot tell this apart from the test above has done nothing.** The two
        /// fixtures differ only in what the band owns, and the pair is the whole difference between
        /// *"you have a keeper to spare"* and *"you are short-handed"*.
        #[test]
        fn a_bare_handed_pair_on_one_road_frees_nobody() {
            const KEEPERS: u32 = 2;
            const ONE_ROAD: u32 = 1;
            /// How close the planned hands must come to the head count for *"every keeper is
            /// working"* to be a true statement — `distribute_upkeep_pool` scales each need by one
            /// coverage, so the shares sum to the pool only to within float error.
            const A_WHOLE_POOL: f32 = 1.0e-5;

            let turn = a_roadwork_pool_over_dirt_roads(&[], KEEPERS, ONE_ROAD, A_LONG_HAUL);
            let (required, filled) = toe_line(&turn.app, "roadwork", EARTHMOVING);

            assert_eq!(
                filled, 0.0,
                "fixture: the pair really is bare-handed — the settlement reached the line with \
                 nothing: {filled}"
            );
            assert!(
                (KEEPERS as f32 - required).abs() < A_WHOLE_POOL,
                "fixture: and both keepers are planned onto the road: {required} of {KEEPERS}"
            );
            assert_eq!(
                published_idle_keepers(&turn.app, "roadwork"),
                0.0,
                "⛔ neither keeper can be freed, so the wire frees neither — two bare hands are \
                 not one geared one"
            );
        }

        /// ⛔ **IDLE IS STRUCK AFTER THE TOP-UP, NOT BEFORE IT.**
        ///
        /// Three keepers over two short-haul roads: the plan wants `1.32` of them, so `1.68` are
        /// left standing — and the band's single earthmoving set cannot arm the hands the plan
        /// placed, so both roads end the split short. Step 5 spends part of those `1.68` closing
        /// the gap, **bare**, and what the wire publishes is what is left after it did.
        ///
        /// **This is the assertion that fails if someone later publishes the pre-top-up figure.**
        /// Reporting `1.68` here would tell the player to step down a keeper the sim has working —
        /// issue #714's own defect, arriving through the readout instead of through the split.
        #[test]
        fn the_hands_step_five_spent_are_not_published_as_standing() {
            const KEEPERS: u32 = 3;
            const TWO_ROADS: u32 = 2;
            const ONE_TOOL: u32 = 1;
            /// **What the plan left standing before step 5 ran**, measured: `3` keepers less the
            /// `1.32` the two bills asked for at the geared rate.
            const BEFORE_THE_TOP_UP: f32 = 1.68;
            /// **And what was still standing after it**, measured: step 5 spent `0.648` bare hands
            /// closing the two roads' `0.648` work units of deficit.
            const AFTER_THE_TOP_UP: f32 = 1.0400001;

            let turn = a_roadwork_pool_over_dirt_roads(
                &[(EARTHMOVING, ONE_TOOL)],
                KEEPERS,
                TWO_ROADS,
                A_SHORT_HAUL,
            );
            let (required, filled) = toe_line(&turn.app, "roadwork", EARTHMOVING);
            let idle = published_idle_keepers(&turn.app, "roadwork");

            assert_eq!(
                KEEPERS as f32 - required,
                BEFORE_THE_TOP_UP,
                "fixture: the plan really does leave hands standing — {required} of {KEEPERS}"
            );
            assert!(
                filled < required,
                "fixture: and the pool is short of the tool those hands were planned at, so there \
                 is a deficit for step 5 to spend them on: filled {filled} against required \
                 {required}"
            );
            assert_eq!(
                turn.supplied, turn.bills,
                "fixture: …which it does, closing both roads out of hands that were idle"
            );

            assert_eq!(
                idle, AFTER_THE_TOP_UP,
                "⛔ the wire states what step 5 could NOT place, not what the plan left over"
            );
            assert!(
                idle < BEFORE_THE_TOP_UP,
                "⛔ strictly fewer than the plan left standing: the hands step 5 spent are working \
                 and must not be offered up — {idle} against {BEFORE_THE_TOP_UP}"
            );
        }

        /// ⛔ **A POOL WITH A HEAD COUNT AND NO SITES PUBLISHES ITS WHOLE HEAD COUNT** — probably
        /// the commonest shape there is, and the one an early return would have silently omitted.
        ///
        /// Three keepers on `roadwork` and not a road in the world: there is no bill at all, so
        /// every one of them stands. The other three keeping pools are unstaffed and say `0`, and
        /// **`builders` states no line** — it is not a keeping pool, `build_workers` puts the whole
        /// head count on the queue head, so no builder is ever left standing by a plan that wanted
        /// fewer.
        #[test]
        fn a_pool_with_a_head_count_and_no_sites_publishes_all_of_it() {
            const KEEPERS: u32 = 3;
            const NO_ROADS: u32 = 0;

            let turn = a_roadwork_pool_over_dirt_roads(&[], KEEPERS, NO_ROADS, A_SHORT_HAUL);

            assert!(
                turn.bills.is_empty(),
                "fixture: there is no road and therefore no bill: {:?}",
                turn.bills
            );
            assert_eq!(
                published_idle_keepers(&turn.app, "roadwork"),
                KEEPERS as f32,
                "every keeper the player put on the role stands, and the wire says so"
            );

            let crew = published_pool_crew(&turn.app);
            assert_eq!(
                crew.iter()
                    .filter(|(pool, idle)| pool != "roadwork" && *idle == 0.0)
                    .count(),
                3,
                "the three unstaffed keeping pools each state a zero rather than no line: {crew:?}"
            );
            assert!(
                crew.iter().all(|(pool, _)| pool != "builders"),
                "⛔ the builders are not a keeping pool and state no crew line: {crew:?}"
            );
        }

        /// ⛔ **A FULLY COMMITTED POOL REPORTS EXACTLY NONE — no tolerance.**
        ///
        /// Two long-haul roads want `3.96` keepers of the pool's `3`, so the split is capped from
        /// below and there is nothing over. A tolerance here would pass for a model that reported a
        /// sliver of a keeper nobody has — which is precisely what `keepers − Σ assigned hands`
        /// does under `Spread` (see `ToeClaim::need`), and the number would be drawn as an
        /// offer to step the pool down.
        #[test]
        fn a_fully_committed_pool_publishes_exactly_none() {
            const KEEPERS: u32 = 3;
            const TWO_ROADS: u32 = 2;
            const ONE_TOOL: u32 = 1;

            let turn = a_roadwork_pool_over_dirt_roads(
                &[(EARTHMOVING, ONE_TOOL)],
                KEEPERS,
                TWO_ROADS,
                A_LONG_HAUL,
            );
            let rate = plan_rate(&turn.ledger, RungKey::RouteDirtRoad);

            assert!(
                (turn.bills[0] + turn.bills[1]) / rate > KEEPERS as f32,
                "fixture: the two bills must outrun the pool, or nothing is committed — {} over \
                 {KEEPERS}",
                (turn.bills[0] + turn.bills[1]) / rate
            );
            assert_eq!(
                published_idle_keepers(&turn.app, "roadwork"),
                0.0,
                "⛔ exactly none, with no tolerance: a pool that wanted more hands than it has has \
                 none to spare"
            );
        }

        /// ⛔ **THE REPORTED CASE — a head count moved AFTER the settle publishes the head count it
        /// was SETTLED at, not the one the band's row carries now.**
        ///
        /// `assign_labor` writes the band's row the instant the player presses the stepper, outside
        /// the turn; the crew account is stamped only where the turn settles the pool. So on every
        /// frame between a press and the next turn resolution the two disagree — and a client
        /// projecting the press as `idleKeepers + (row − keepers)` gets the right answer only if
        /// `keepers` is the **settled** basis. Publishing the row's live head count here would make
        /// that difference `0` on exactly the frame the player is deciding from, collapsing the
        /// projection to a turn-old figure: three keepers freshly put on a pool with nothing to do
        /// would report none.
        #[test]
        fn the_published_head_count_is_the_one_the_turn_settled_not_the_row_as_it_stands_now() {
            const SETTLED_WITH: u32 = 1;
            const AFTER_THE_PRESS: u32 = 3;
            const NO_ROADS: u32 = 0;

            let mut turn =
                a_roadwork_pool_over_dirt_roads(&[], SETTLED_WITH, NO_ROADS, A_SHORT_HAUL);
            assert_eq!(
                published_settled_keepers(&turn.app, "roadwork"),
                SETTLED_WITH as f32,
                "fixture: the turn settled the pool at the head count it was staffed with"
            );

            let (band, _, _, _) = first_band(&mut turn.app);
            restaff_outside_the_turn(&mut turn.app, band, &LaborTarget::Roadwork, AFTER_THE_PRESS);
            core_sim::recapture_snapshot_in_place(&mut turn.app.world);

            assert_eq!(
                published_row_workers(&turn.app, "roadwork"),
                AFTER_THE_PRESS,
                "fixture: the press really did move the band's row on this very frame, with no \
                 turn between"
            );
            assert_eq!(
                published_settled_keepers(&turn.app, "roadwork"),
                SETTLED_WITH as f32,
                "⛔ the crew account states the head count it was STRUCK against — the row has \
                 moved past it and the account must not follow"
            );
            assert_eq!(
                published_idle_keepers(&turn.app, "roadwork"),
                SETTLED_WITH as f32,
                "…and its idle figure is still the settled one, unchanged by a press the turn has \
                 not seen"
            );

            let projected = published_idle_keepers(&turn.app, "roadwork")
                + (published_row_workers(&turn.app, "roadwork") as f32
                    - published_settled_keepers(&turn.app, "roadwork"));
            assert_eq!(
                projected, AFTER_THE_PRESS as f32,
                "⛔ which is what lets a reader project the pending edit: every keeper on a pool \
                 with nothing to do, on the frame of the press"
            );
        }

        /// ⛔ **THE TWO TERMS COME FROM ONE MOMENT** — a pool holding no site at all reports its
        /// whole head count idle, so `idleKeepers == keepers` exactly.
        ///
        /// It is the invariant that catches the pair being stamped from two different moments: any
        /// seam that took the idle figure from the turn and the head count from anywhere else
        /// breaks this the moment the two disagree, and a pool with no claims is where they are
        /// provably equal.
        #[test]
        fn a_pool_with_no_claims_reports_every_keeper_it_was_struck_with() {
            const KEEPERS: u32 = 3;
            const NO_ROADS: u32 = 0;

            let turn = a_roadwork_pool_over_dirt_roads(&[], KEEPERS, NO_ROADS, A_SHORT_HAUL);
            let idle = published_idle_keepers(&turn.app, "roadwork");
            let keepers = published_settled_keepers(&turn.app, "roadwork");

            assert_eq!(
                keepers, KEEPERS as f32,
                "the head count published is the one the pool was settled with"
            );
            assert_eq!(
                idle, keepers,
                "⛔ nothing was claimed, so every keeper the pool was struck with is idle — the \
                 two terms are one turn's arithmetic"
            );
        }

        /// **A BAND THAT WORKS NOTHING AT ALL** — the turn-1 shape: a cohort with hands and an
        /// **empty** `LaborAllocation::assignments`.
        ///
        /// ⛔ **IT IS THE ONLY FIXTURE IN THIS FILE THAT CROSSES THE ASSIGNMENT LOOP'S
        /// `assignments.is_empty()` GUARD**, and crossing it is the whole point. Every other
        /// fixture here goes through `staff_one_role`, and **a staffed role is itself an assignment
        /// row** — so each of them arrives at the guard with a non-empty list, walks straight past
        /// it, and reaches the food webs' crew stamp however few sites it holds. That is why they
        /// stayed green while the shipped game published only two of the four lines.
        fn a_band_that_works_nothing(hands: u32) -> App {
            let mut app = spawn_world();
            let (band, _, _, _) = first_band(&mut app);
            app.world
                .entity_mut(band)
                .insert(LaborAllocation::default());
            size_the_band(&mut app, band, hands);
            app.update();
            app
        }

        /// **PUT A HEAD COUNT ON A ROLE THE BAND DOES NOT YET STAFF**, the way a stepper press does
        /// — [`restaff_outside_the_turn`]'s twin for a row that has to be created rather than
        /// moved, with no turn in between.
        fn staff_outside_the_turn(app: &mut App, band: Entity, role: LaborTarget, keepers: u32) {
            {
                let mut allocation = app
                    .world
                    .get_mut::<LaborAllocation>(band)
                    .expect("the fixture band holds an allocation");
                allocation.assignments.push(core_sim::LaborAssignment {
                    target: role,
                    workers: keepers,
                    kit: None,
                    priority: SourcePriority::default(),
                    upkeep_kit: None,
                });
            }
            size_the_band(app, band, keepers);
        }

        /// **HOW MANY LABOR ROWS THE BAND PUBLISHED** — read off the wire, so *"this band works
        /// nothing"* is asserted against the frame a client sees rather than against the component.
        fn published_row_count(app: &App) -> usize {
            with_published_cohort(app, |cohort| {
                cohort.laborAssignments().map_or(0, |rows| rows.len())
            })
        }

        /// ⛔ **THE REPORTED DEFECT — A BAND WITH NO WORKED SOURCES PUBLISHES ALL FOUR LINES.**
        ///
        /// `roadwork` and `quarrywork` are settled **above** the assignment loop's two `continue`s
        /// and the two food webs' shares are read back below them, so a band whose `assignments`
        /// are empty used to publish two crew lines and not four — and a client's reader, handed no
        /// `agriculture` row, drew nothing at all.
        ///
        /// **The inversion is the thing**: a band with no worked sources is *precisely* the band
        /// whose keepers have nothing to do, so the guard skipped the stamp in the one case the
        /// figure exists to report. All four lines read `0` here because the head count is summed
        /// off the rows (`LaborAllocation::workers_on`) and there are none — what the test holds is
        /// that the **line exists**, which is what the next press is read against.
        #[test]
        fn a_band_with_no_assignments_at_all_publishes_all_four_crew_lines() {
            const IDLE_HANDS: u32 = 3;
            const NO_ROWS: usize = 0;
            /// Every pool's head count is summed off the band's rows, and a band with no rows has
            /// none on any of them.
            const UNSTAFFED: f32 = 0.0;
            const THE_FOUR_KEEPING_POOLS: [&str; 4] =
                ["agriculture", "husbandry", "quarrywork", "roadwork"];

            let app = a_band_that_works_nothing(IDLE_HANDS);

            assert_eq!(
                published_row_count(&app),
                NO_ROWS,
                "fixture: the band staffs nothing, so the turn really does hit the \
                 empty-assignments guard this test is about"
            );
            let crew = published_pool_crew(&app);
            assert_eq!(
                crew.iter()
                    .map(|(pool, _)| pool.as_str())
                    .collect::<Vec<_>>(),
                THE_FOUR_KEEPING_POOLS,
                "⛔ all four keeping pools state a line, not just the two settled above the \
                 guards: {crew:?}"
            );
            for pool in THE_FOUR_KEEPING_POOLS {
                assert_eq!(
                    published_settled_keepers(&app, pool),
                    UNSTAFFED,
                    "the {pool} pool was settled with nobody on it"
                );
                assert_eq!(
                    published_idle_keepers(&app, pool),
                    UNSTAFFED,
                    "…and nobody on it is standing, which is a `0` and not an absent row"
                );
            }
        }

        /// ⛔ **AND THAT LINE IS WHAT THE FIRST PRESS IS READ AGAINST** — the player-facing half of
        /// the same defect.
        ///
        /// A reader projects a pending edit as `idleKeepers + (row − keepers)`
        /// (`the_published_head_count_is_the_one_the_turn_settled_not_the_row_as_it_stands_now`).
        /// On a band that worked nothing there was no `agriculture` line to project **from**, so
        /// putting the band's first three keepers on the plant web drew no figure at all until a
        /// turn had resolved. With the line published at `0 / 0` the same arithmetic answers on the
        /// frame of the press: three keepers, none of them with anything to do.
        #[test]
        fn the_first_keeper_put_on_an_unworked_web_reads_as_idle_on_the_frame_of_the_press() {
            const IDLE_HANDS: u32 = 3;
            const THE_PRESS: u32 = 3;

            let mut app = a_band_that_works_nothing(IDLE_HANDS);
            let (band, _, _, _) = first_band(&mut app);
            staff_outside_the_turn(&mut app, band, LaborTarget::Agriculture, THE_PRESS);
            core_sim::recapture_snapshot_in_place(&mut app.world);

            assert_eq!(
                published_row_workers(&app, "agriculture"),
                THE_PRESS,
                "fixture: the press really did put the hands on the row, with no turn between"
            );
            let projected = published_idle_keepers(&app, "agriculture")
                + (published_row_workers(&app, "agriculture") as f32
                    - published_settled_keepers(&app, "agriculture"));
            assert_eq!(
                projected, THE_PRESS as f32,
                "⛔ every keeper the press put on a web with no tended ground is standing, and the \
                 reader can say so because the line it projects from exists"
            );
        }
    }
}
