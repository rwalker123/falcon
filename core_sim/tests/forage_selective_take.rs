//! **A GATHER CAN NAME WHAT IT CARRIES HOME** — the selective gather
//! (`docs/plan_flora_roster.md`, the rung-1 half).
//!
//! A tile's basket mixes food with fibre, so *what am I here for* is a decision beside *how hard do
//! I press* (the harvest floor). A crew may name one or more of the plants growing on the patch and
//! leave the rest standing; naming nothing takes the whole basket, exactly as every gather did
//! before this existed.
//!
//! What this file pins, in the order the mechanic has to hold together:
//!
//! 1. **Naming nothing moves no number** — the neutrality bar, asserted as an identity against the
//!    whole-basket seams rather than as a remembered figure.
//! 2. **Narrowing to the food banks more food and no fibre**, with the worker cap binding — and the
//!    *precondition* that the two runs differ at all, so the pair cannot pass by both collapsing.
//! 3. **A scarce species exhausts before a second worker is useful** — the published crew count
//!    reads `1` where the whole basket reads `2`. This is the readout the whole mechanic depends
//!    on: if it does not move, the choice teaches nothing.
//! 4. **The published selection is the keys' own order, however they were typed** — the determinism
//!    half, and the reason the selection is an ordered collection rather than a sorted `Vec`.
//!
//! **Asserted on the ENCODED envelope wherever the claim is about a readout**, never on the
//! in-process row: a count can be right in the capture and absent from the buffer.
//!
//! The command's own refusals (an unknown plant, a plant that does not grow here) are pinned where
//! the command lives — `core_sim/src/bin/server.rs`'s `mod tests` — because a guard that calls the
//! validator directly passes while no command path validates anything (`cultivation.md`).

use bevy::app::App;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::UVec2;

use core_sim::{
    advance_labor_allocation, build_test_app, patch_material_yields, patch_provisions_per_biomass,
    recapture_snapshot_in_place, scalar_from_f32, scalar_one, scalar_zero, selected_biomass_share,
    tile_flora_composition, FactionId, FloraConfigHandle, FloraShare, ForagePatch, ForageRegistry,
    GenerationId, LaborAllocation, LaborAssignment, LaborConfigHandle, LaborTarget, LocalStore,
    MoraleCause, PopulationCohort, ResidentBand, SimulationConfig, SnapshotHistory, StartingUnit,
    TakeSelection, Tile, TileRegistry, WHOLE_BASKET,
};

/// The map every fixture here stands on — the standard seed the flora suites are quoted against, so
/// two files measuring the same tile are reading the same world.
const STANDARD_SEED: u64 = 119_304_647;

/// The gather floor these fixtures press at — **below** the food peak, so there is real stock
/// standing above it to be taken and narrowed.
const DEEP_DRAW_FLOOR: f32 = 0.15;

/// **A stand thin enough that a scarce plant's share binds the take** — the room above the floor
/// still carries two hands on the whole basket (so the build gate is comfortably satisfied and the
/// crew cap binds there), while a tenth of it does not. The build fixture needs both at once.
const LEAN_STANDING_CROP: f32 = 0.5;

/// The patch's standing crop as a fraction of `K` where a fixture wants a full stand rather than a
/// measured one.
const STOCKED_STANDING_CROP: f32 = 0.9;

/// f32 slack on a rate (a chain of ~3 multiplications) or on a biomass.
const EPSILON: f32 = 1e-4;

/// **A share this small is a scarce plant** — the bar the crew-count fixture insists its chosen
/// species clears, so *"pick a species that is a tenth of the tile"* is a fact about the fixture and
/// not a hope about the realization draw.
const SCARCE_SHARE: f32 = 0.25;

/// Head counts. Two, because the whole claim of the crew-count fixture is that the second hand is
/// useful on the whole basket and idle on a scarce plant.
const TWO_HANDS: u32 = 2;

/// One hand — the throughput probe's crew, so a take on a deep stand is exactly one worker's load.
const ONE_HAND: u32 = 1;

/// **Nobody on the take row.** The row survives on its queue entry — a declaration is a holding —
/// and it is how the build fixture below states *"the crew carried nothing home"* without inventing
/// a state the command could not produce.
const NO_GATHERERS: u32 = 0;

/// The builders pool the build fixture staffs. Any positive count does; two matches the tended
/// rung's own reference crew, so the turns it banks read as the shipped pace.
const BUILDERS: u32 = 2;

// ---------------------------------------------------------------------------------------------
// 1. Neutrality — naming nothing is the gather that was already there.
// ---------------------------------------------------------------------------------------------

/// **NAMING NOTHING TAKES THE WHOLE BASKET, AND THE ARITHMETIC SAYS SO EXACTLY.**
///
/// The empty selection short-circuits to [`WHOLE_BASKET`] rather than summing the shares, so every
/// number downstream is the pre-selection expression multiplied by exactly `1.0`. Both halves are
/// asserted:
///
/// - the share the ceiling is scaled by is **bit-exactly** the whole basket;
/// - a real turn's food and materials are **bit-exactly** what the unchanged whole-basket rate seams
///   (`patch_provisions_per_biomass` / `patch_material_yields`) compose over the biomass it took.
///
/// That second half is the neutrality bar: those two seams are what every take paid through before
/// the selection existed, so an equality against them is an equality against the old take.
#[test]
fn naming_nothing_takes_the_whole_basket_and_moves_no_number() {
    let mut app = world();
    let (tile_entity, coord) = a_mixed_patch_tile(&mut app);
    let composition = tile_composition(&app, coord);
    stock_patch(&mut app, coord, STOCKED_STANDING_CROP);

    assert_eq!(
        selected_biomass_share(&composition, &TakeSelection::EVERYTHING),
        WHOLE_BASKET,
        "an empty selection is the whole basket, exactly — not a sum that lands near it"
    );

    // The rates the pre-selection take paid through, resolved on the PRE-take patch exactly as
    // `forage_take` resolves them.
    let before = patch_at(&app, coord);
    let flora = app.world.resource::<FloraConfigHandle>().get();
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let rate = patch_provisions_per_biomass(&before, &composition, &flora, &labor.forage);
    let material_rows = patch_material_yields(&before, &composition, &flora, &labor.forage);
    drop(flora);
    drop(labor);

    let band = spawn_forager(
        &mut app,
        tile_entity,
        coord,
        TWO_HANDS,
        TakeSelection::EVERYTHING,
    );
    app.world.run_system_once(advance_labor_allocation);

    let take = before.biomass - patch_at(&app, coord).biomass;
    assert!(take > 0.0, "the fixture gather must draw the stand down");
    assert_eq!(
        band_food(&app, band),
        take * rate,
        "an unnamed selection banks exactly what the whole-basket rate composes over its take"
    );
    for row in core_sim::material_yield_totals(&material_rows, take, 1.0) {
        assert!(row.amount > 0.0, "a published rate is a rate that pays");
        let held = band_material(&app, band, &row.material);
        assert!(
            (held - row.amount).abs() <= EPSILON * row.amount.max(1.0),
            "an unnamed selection banks exactly the {} the whole-basket decomposition composes: \
             {held} vs {}",
            row.material,
            row.amount
        );
    }
}

// ---------------------------------------------------------------------------------------------
// 2. The decision — food or fibre off one stand.
// ---------------------------------------------------------------------------------------------

/// **NARROWING TO THE FOOD BANKS MORE FOOD AND NO FIBRE**, off the same stand with the same crew.
///
/// The crew is small enough that its own throughput binds, which is the regime the choice matters
/// in: the hands carry a fixed load either way, so what the player is choosing is *what fills the
/// baskets*. Against a mixed basket that load is part cotton and converts at the basket's average;
/// against the staples alone it is all grain.
///
/// **The precondition is asserted, not assumed.** A whole-basket run that banked no fibre — or a
/// narrowed run that banked no food — would make the pair pass by both collapsing to zero, which is
/// exactly the failure a two-sided comparison invites.
#[test]
fn narrowing_to_the_food_banks_more_food_and_no_fibre() {
    let (food_only_food, food_only_material) = gather_with(|composition, flora| {
        // **The plants that are food and ONLY food.** A staple that also sheds a byproduct fibre
        // would bank material off a food-only gather and make "no fibre" a claim about the roster
        // rather than about the selection.
        TakeSelection::from_keys(
            composition
                .iter()
                .filter(|entry| {
                    flora
                        .species
                        .get(&entry.species)
                        .is_some_and(pays_food_alone)
                })
                .map(|entry| entry.species.clone())
                .collect::<Vec<_>>(),
        )
    });
    let (whole_food, whole_material) = gather_with(|_, _| TakeSelection::EVERYTHING);

    // **The precondition** — the two runs must be describing a genuinely mixed stand.
    assert!(
        whole_material > 0.0,
        "the fixture tile must bank fibre when the whole basket is taken, or 'no fibre' is vacuous"
    );
    assert!(
        whole_food > 0.0,
        "the fixture tile must bank food either way, or 'more food' is vacuous"
    );

    assert!(
        food_only_food > whole_food,
        "gathering the food species alone must bank MORE food than filling the same baskets from \
         the whole stand: {food_only_food} vs {whole_food}"
    );
    assert_eq!(
        food_only_material, 0.0,
        "a crew that named only the food plants leaves the fibre standing"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The readout — worker count finally means something at rung 1.
// ---------------------------------------------------------------------------------------------

/// **A SCARCE PLANT EXHAUSTS BEFORE THE SECOND WORKER IS USEFUL, AND THE WIRE SAYS SO.**
///
/// The stand is set so the whole basket offers exactly two workers' load above the floor: with the
/// whole basket the published crew count is `2`, and narrowed to a plant that is a fraction of the
/// tile it is `1` — the same tile, the same two hands, the same floor.
///
/// **This is the readout the mechanic depends on.** *"There is very little of it standing"* has to be
/// visible, not merely true, or narrowing teaches nothing; and the count is read off the **encoded
/// envelope**, because a number that is right in the capture and absent from the buffer is invisible
/// in exactly the way it must not be.
#[test]
fn a_scarce_species_exhausts_before_a_second_worker_is_useful() {
    // One worker on a deep stand takes exactly its own load — the throughput this fixture sizes the
    // stand against, measured rather than assumed (it is a resolved kit tier, not a config read).
    let per_worker = {
        let mut app = world();
        let (tile_entity, coord) = a_mixed_patch_tile(&mut app);
        stock_patch(&mut app, coord, STOCKED_STANDING_CROP);
        let before = patch_at(&app, coord).biomass;
        spawn_forager(
            &mut app,
            tile_entity,
            coord,
            ONE_HAND,
            TakeSelection::EVERYTHING,
        );
        app.world.run_system_once(advance_labor_allocation);
        before - patch_at(&app, coord).biomass
    };
    assert!(per_worker > 0.0, "one gatherer must carry something home");

    let crew_needed = |take: TakeSelection| -> u32 {
        let mut app = world();
        let (tile_entity, coord) = a_mixed_patch_tile(&mut app);
        // **The stand offers exactly two workers' load above the floor.** So the whole basket wants
        // both hands, and any share of it below a half wants one.
        let capacity = patch_at(&app, coord).carrying_capacity;
        let standing = DEEP_DRAW_FLOOR * capacity + TWO_HANDS as f32 * per_worker;
        set_standing_crop(&mut app, coord, standing);
        spawn_forager(&mut app, tile_entity, coord, TWO_HANDS, take);
        app.world.run_system_once(advance_labor_allocation);
        published_workers_needed(&mut app)
    };

    let scarce = {
        let mut app = world();
        let (_, coord) = a_mixed_patch_tile(&mut app);
        let composition = tile_composition(&app, coord);
        let entry = composition
            .iter()
            .min_by(|a, b| a.share.total_cmp(&b.share))
            .expect("the fixture tile names plants")
            .clone();
        assert!(
            entry.share <= SCARCE_SHARE,
            "the fixture wants a genuinely scarce plant to narrow to; the thinnest here is {} at {}",
            entry.species,
            entry.share
        );
        entry.species
    };

    assert_eq!(
        crew_needed(TakeSelection::EVERYTHING),
        TWO_HANDS,
        "the whole stand offers two workers' load, so both hands are useful on it"
    );
    assert_eq!(
        crew_needed(TakeSelection::from_keys([scarce.as_str()])),
        1,
        "there is very little {scarce} standing, so the second hand has nothing to fill a day with"
    );
}

// ---------------------------------------------------------------------------------------------
// 4. The build is gated on the GROUND, not on the gatherers' pickiness.
// ---------------------------------------------------------------------------------------------

/// **NARROWING THE GATHERERS NEVER STALLS THE BUILD BESIDE THEM.**
///
/// `Cultivate`'s accrual is gated on `crew_is_working_the_source`, which exists to say *"a crew
/// stripping the ground it is sowing builds nothing"* — a statement about **the ground being
/// stripped**. A take selection does not strip the ground; it leaves the rest standing, by
/// definition. And the builders are a **band-level pool that is not gathering at all**, so what the
/// gatherers chose to carry home has no bearing on whether the ground can be cleared and planted.
///
/// Gating the build on the narrowed room made the worst kind of failure: tick *fibre* on a work row
/// and a 25-turn build ordered elsewhere quietly stops advancing, with nothing said and no way to
/// connect the two.
///
/// **The pair is asserted at both ends**, so neither half can pass on its own: the take genuinely
/// moves with the selection — narrowed < whole, and **zero** both when nobody is gathering and when
/// the crew is asking for a plant with nothing standing — while the build banks the **same** work in
/// all four runs.
///
/// **How a selection comes to have nothing standing**, since the command refuses one this tile's
/// basket does not carry: the basket it is judged against can move *after* the row is written — a
/// `reload_config` of the roster is the shipped path, and a rung's own reweight is the other (a
/// tended patch weeds its volunteers down, a Field drops them entirely). The zero-share row is
/// therefore a state the arm has to answer for, and *stalling a queued build* is not the answer.
#[test]
fn a_narrowed_gather_never_stalls_the_build_beside_it() {
    let run = |gatherers: u32, take: TakeSelection| -> (f32, f32) {
        let mut app = world();
        let (tile_entity, coord) = a_mixed_patch_tile(&mut app);
        stock_patch(&mut app, coord, LEAN_STANDING_CROP);
        // The rung's own knowledge gate — this test is about the work predicate, not about
        // Cultivation being unlearned.
        app.world
            .resource_mut::<core_sim::DiscoveryProgressLedger>()
            .add_progress(
                FactionId(0),
                core_sim::CULTIVATION_DISCOVERY_ID,
                scalar_one(),
            );
        let before = patch_at(&app, coord).biomass;
        spawn_forager_building(&mut app, tile_entity, coord, gatherers, take);
        app.world.run_system_once(advance_labor_allocation);
        let ladder = app.world.resource::<core_sim::LadderConfigHandle>().get();
        let banked = core_sim::patch_rung_work_done(
            &patch_at(&app, coord),
            core_sim::RungKey::PlantTended,
            &ladder,
        );
        (before - patch_at(&app, coord).biomass, banked)
    };

    let scarce = {
        let mut app = world();
        let (_, coord) = a_mixed_patch_tile(&mut app);
        tile_composition(&app, coord)
            .iter()
            .min_by(|a, b| a.share.total_cmp(&b.share))
            .expect("the fixture tile names plants")
            .species
            .clone()
    };

    let stranded = {
        let mut app = world();
        let (_, coord) = a_mixed_patch_tile(&mut app);
        a_species_this_tile_does_not_grow(&app, coord)
    };

    let (whole_take, whole_build) = run(TWO_HANDS, TakeSelection::EVERYTHING);
    let (narrow_take, narrow_build) = run(TWO_HANDS, TakeSelection::from_keys([scarce.as_str()]));
    let (idle_take, idle_build) = run(NO_GATHERERS, TakeSelection::EVERYTHING);
    let (stranded_take, stranded_build) =
        run(TWO_HANDS, TakeSelection::from_keys([stranded.as_str()]));

    // The control: this fixture really is building something.
    assert!(
        whole_build > 0.0,
        "the fixture must bank real Cultivate work, or the equalities below are vacuous"
    );
    // …and the selection really is live on the take, so the equalities cannot pass by it being
    // ignored outright.
    assert!(
        narrow_take > 0.0 && narrow_take < whole_take,
        "narrowing must move the take: {narrow_take} against {whole_take}"
    );
    assert_eq!(
        idle_take, 0.0,
        "a row with no gatherers carries nothing home — the literal 'took nothing' case"
    );
    assert_eq!(
        stranded_take, 0.0,
        "a crew asking for {stranded}, which is not standing here, carries nothing home"
    );

    assert_eq!(
        narrow_build, whole_build,
        "the build is gated on the ground, not on what the gatherers chose to carry home"
    );
    assert_eq!(
        idle_build, whole_build,
        "and it advances even where the crew took nothing at all — the builders are a band pool, \
         and the stand above the floor is what says the ground can be worked"
    );
    assert_eq!(
        stranded_build, whole_build,
        "…including where the crew's own share of the stand is ZERO: the ground is unstripped and \
         the builders are still on it, so the 25-turn build must not stall silently"
    );
}

/// A roster plant this tile does **not** grow — the input for *"the crew's share of the stand is
/// zero"*. Resolved against the live roster and the tile's own realized basket so it cannot go
/// stale, and asserted rather than defaulted: a fixture that silently found nothing would measure
/// the whole basket twice.
fn a_species_this_tile_does_not_grow(app: &App, coord: UVec2) -> String {
    let here = tile_composition(app, coord);
    let flora = app.world.resource::<FloraConfigHandle>().get();
    flora
        .species
        .keys()
        .find(|key| !here.iter().any(|entry| &&entry.species == key))
        .expect("the roster carries a plant this tile does not grow")
        .clone()
}

// ---------------------------------------------------------------------------------------------
// 5. The sheet can price a narrowing BEFORE committing to it.
// ---------------------------------------------------------------------------------------------

/// **THE PUBLISHED PER-SPECIES RATES COMPOSE TO WHAT THE BAND ACTUALLY BANKED.**
///
/// `provisionsPerBiomass` is the **basket average**, so a compose sheet holding only that one cannot
/// price a *narrowing* at all: tick a crop chip and the forecast sits still until the player commits
/// and waits a turn, while the worker dial beside it quotes live. A readout that is live for one
/// control and inert for the other teaches that toggling chips is free — when it is the whole
/// decision this feature exists to create. `compositionProvisionsPerBiomass` and its fodder twin are
/// what let the sheet compose `Σ_S share × rate ÷ Σ_S share` itself.
///
/// **Asserted against the payout, never against a re-derivation of the sim's arithmetic** — the rule
/// this file's own history records (`flora.md` → the commit-ratio bug). A real turn runs with a
/// narrowed selection, and the published rates are composed the way a client would compose them and
/// checked against the `FOOD` the band is holding.
///
/// **The fixture insists on a basket that can tell the implementations apart**: the selection names
/// **two** plants with **different** published rates, one of which pays **zero** food (a cash crop) —
/// against a sim that published one rate for everything, or that published the basket average per
/// entry, the composed number would be wrong rather than merely unproven.
#[test]
fn the_published_per_species_rates_compose_to_what_the_band_banks() {
    let mut app = world();
    let (tile_entity, coord) = a_mixed_patch_tile(&mut app);
    stock_patch(&mut app, coord, LEAN_STANDING_CROP);
    let composition = tile_composition(&app, coord);

    // A cash crop (no food at all) beside the fattest plant that does pay food — two rates that
    // cannot be confused, and one of them zero.
    let (cash, staple) = {
        let flora = app.world.resource::<FloraConfigHandle>().get();
        let cash = composition
            .iter()
            .find(|entry| {
                flora
                    .species
                    .get(&entry.species)
                    .is_some_and(|def| def.yield_.provisions_per_biomass == 0.0)
            })
            .expect("the fixture tile must grow a plant that pays no food")
            .species
            .clone();
        let staple = composition
            .iter()
            .find(|entry| {
                flora
                    .species
                    .get(&entry.species)
                    .is_some_and(pays_food_alone)
            })
            .expect("the fixture tile must grow a plant that pays food")
            .species
            .clone();
        (cash, staple)
    };
    let selection = [cash.as_str(), staple.as_str()];

    let before = patch_at(&app, coord).biomass;
    let band = spawn_forager(
        &mut app,
        tile_entity,
        coord,
        TWO_HANDS,
        TakeSelection::from_keys(selection),
    );
    app.world.run_system_once(advance_labor_allocation);
    let take = before - patch_at(&app, coord).biomass;
    assert!(take > 0.0, "the fixture gather must draw the stand down");

    let published = published_patch_basket(&mut app, coord);
    let rate_of = |species: &str| {
        published
            .iter()
            .find(|entry| entry.species == species)
            .unwrap_or_else(|| panic!("{species} must be on the published basket"))
    };

    // **The fixture's own bar**, asserted rather than assumed.
    assert_eq!(
        rate_of(&cash).provisions_per_biomass,
        0.0,
        "a cash crop's published food rate is exactly zero — the basket average never is"
    );
    assert!(
        rate_of(&staple).provisions_per_biomass > 0.0
            && rate_of(&staple).provisions_per_biomass != rate_of(&cash).provisions_per_biomass,
        "the two selected plants must publish DIFFERENT rates, or one rate for everything passes"
    );

    // **Composed exactly as a compose sheet would compose it**, off the published fields alone.
    let selected: Vec<&PublishedShare> = selection.iter().map(|key| rate_of(key)).collect();
    let selected_share: f32 = selected.iter().map(|entry| entry.share).sum();
    assert!(selected_share > 0.0, "the selection must be standing here");
    let composed_rate: f32 = selected
        .iter()
        .map(|entry| entry.share * entry.provisions_per_biomass)
        .sum::<f32>()
        / selected_share;
    let expected = take * composed_rate;
    let banked = band_food(&app, band);
    assert!(
        (banked - expected).abs() <= EPSILON * expected.max(1.0),
        "the published per-species rates compose to {expected} over a {take}-biomass gather, and \
         the band holds {banked}"
    );

    // **AND THE FINER GRAIN AGREES WITH THE NUMBER THE SIM PAYS WITH** — `Σ share × rate` is the
    // basket average published beside it, in both scalar accounts. A per-species table that drifted
    // from it would price a narrowing against an economy that does not exist.
    let (whole_food, whole_fodder) = published_patch_rates(&mut app, coord);
    let summed_food: f32 = published
        .iter()
        .map(|entry| entry.share * entry.provisions_per_biomass)
        .sum();
    let summed_fodder: f32 = published
        .iter()
        .map(|entry| entry.share * entry.fodder_per_biomass)
        .sum();
    assert!(
        (summed_food - whole_food).abs() <= EPSILON * whole_food.max(1.0),
        "Σ share × rate must be the published basket average: {summed_food} vs {whole_food}"
    );
    assert!(
        (summed_fodder - whole_fodder).abs() <= EPSILON * whole_fodder.max(1.0),
        "…and the fodder twin the same: {summed_fodder} vs {whole_fodder}"
    );
}

/// **AND THE MATERIAL ACCOUNT COMPOSES THE SAME WAY — the headline case of the whole feature.**
///
/// `materialPerBiomass` is basket-averaged, so a crew narrowing to a **cash crop** could be told
/// nothing at all about what it would bring home. That is the motivating example failing: baskets
/// are made of **fibre** and baskets are what let a gatherer carry more food, so *"tick cotton, see
/// how much fibre"* is the first thing a player tries.
///
/// A real turn runs narrowed to two plants, the published per-species rows are composed the way a
/// client would compose them, and the result is asserted against what `LocalStore::material_total`
/// actually holds — the bar
/// `forage_basket_reweight::a_wild_gathers_published_material_rate_is_what_the_band_banks` sets for
/// the whole-basket rate.
///
/// **The fixture insists its selection names ONE MATERIAL FROM TWO SPECIES** (cotton fibre beside
/// flax fibre on the reference stand). That trap is recorded in `flora.md` for the basket-wide rate
/// and bites harder per species: a last-write-wins implementation passes a single-species fixture
/// and is off by a factor here.
#[test]
fn the_published_per_species_material_rates_compose_to_what_the_band_banks() {
    let mut app = world();
    let (tile_entity, coord, shared) = a_patch_where_two_plants_pay_one_material(&mut app);
    stock_patch(&mut app, coord, LEAN_STANDING_CROP);

    let selection = [shared.first.as_str(), shared.second.as_str()];
    let before = patch_at(&app, coord).biomass;
    let band = spawn_forager(
        &mut app,
        tile_entity,
        coord,
        TWO_HANDS,
        TakeSelection::from_keys(selection),
    );
    app.world.run_system_once(advance_labor_allocation);
    let take = before - patch_at(&app, coord).biomass;
    assert!(take > 0.0, "the fixture gather must draw the stand down");

    let published = published_patch_basket(&mut app, coord);
    let entry_of = |species: &str| {
        published
            .iter()
            .find(|entry| entry.species == species)
            .unwrap_or_else(|| panic!("{species} must be on the published basket"))
    };

    // **The fixture's own bar** — the merge case is live, and a grain really does publish nothing.
    for species in selection {
        assert!(
            entry_of(species)
                .materials
                .iter()
                .any(|row| row.0 == shared.material),
            "{species} must publish a {} row, or the two-species merge is untested",
            shared.material
        );
    }
    assert!(
        published.iter().any(|entry| entry.materials.is_empty()),
        "the fixture basket must also carry a plant that pays NO material — an empty list is how \
         'no row' is said, and a fixture where everything pays cannot show it"
    );

    // **Composed exactly as a compose sheet would compose it**, off the published rows alone: the
    // share-weighted mean *within the selection*, per material id.
    let selected: Vec<&PublishedShare> = selection.iter().map(|key| entry_of(key)).collect();
    let selected_share: f32 = selected.iter().map(|entry| entry.share).sum();
    assert!(selected_share > 0.0, "the selection must be standing here");
    let mut composed: std::collections::BTreeMap<String, f32> = Default::default();
    for entry in &selected {
        for (material, amount) in &entry.materials {
            *composed.entry(material.clone()).or_insert(0.0) +=
                entry.share * amount / selected_share;
        }
    }
    assert!(
        composed.contains_key(&shared.material),
        "the composed quote must name the material both plants pay"
    );

    for (material, rate) in &composed {
        let expected = take * rate;
        let held = band_material(&app, band, material);
        assert!(
            (held - expected).abs() <= EPSILON * expected.max(1.0),
            "the published per-species {material} rows compose to {expected} over a {take}-biomass \
             gather, and the band holds {held}"
        );
        assert!(*rate > 0.0, "a published row is a row that pays");
    }
}

// ---------------------------------------------------------------------------------------------
// 6. Determinism — one selection, one published order.
// ---------------------------------------------------------------------------------------------

/// **THE SAME SELECTION PUBLISHES THE SAME ROW, HOWEVER IT WAS TYPED.**
///
/// The selection reaches the snapshot, and this repo has already eaten a ~50%-of-runs determinism
/// flake from a collection whose iteration order varied (`flora.md` → the share-denominator note).
/// It is a `BTreeSet`, so *unsorted* is unrepresentable rather than merely unusual: two crews given
/// the same plants in different orders — one of them twice — publish the identical key list **and
/// bank the identical food**, and the published list is ascending.
#[test]
fn the_same_selection_publishes_the_same_row_however_it_was_typed() {
    let run = |keys: &[&str]| -> (Vec<String>, f32) {
        let mut app = world();
        let (tile_entity, coord) = a_mixed_patch_tile(&mut app);
        stock_patch(&mut app, coord, STOCKED_STANDING_CROP);
        let band = spawn_forager(
            &mut app,
            tile_entity,
            coord,
            TWO_HANDS,
            TakeSelection::from_keys(keys.iter().copied()),
        );
        app.world.run_system_once(advance_labor_allocation);
        (published_take_species(&mut app), band_food(&app, band))
    };

    let plants = {
        let mut app = world();
        let (_, coord) = a_mixed_patch_tile(&mut app);
        let composition = tile_composition(&app, coord);
        assert!(
            composition.len() >= 2,
            "the fixture tile must grow at least two plants to order them two ways"
        );
        composition
            .iter()
            .map(|entry| entry.species.clone())
            .collect::<Vec<_>>()
    };
    let forwards: Vec<&str> = plants.iter().map(String::as_str).collect();
    let mut backwards: Vec<&str> = forwards.iter().copied().rev().collect();
    // …and a duplicate, because a selection is a set and the wire must not be able to tell.
    backwards.push(forwards[0]);

    let (keys_a, food_a) = run(&forwards);
    let (keys_b, food_b) = run(&backwards);

    assert!(!keys_a.is_empty(), "the fixture publishes a real selection");
    assert_eq!(
        keys_a, keys_b,
        "the published selection is the keys' own order, not the order they were typed in"
    );
    let mut sorted = keys_a.clone();
    sorted.sort();
    assert_eq!(keys_a, sorted, "the published order is ascending");
    assert_eq!(
        food_a, food_b,
        "the same selection pays the same food whichever way it was written"
    );
}

// ---------------------------------------------------------------------------------------------
// The fixture.
// ---------------------------------------------------------------------------------------------

/// The standard world, through the real Startup chain — worldgen, hydrology and the forage seeding
/// all as shipped, because a hand-rolled partial chain measures a map the sim cannot produce.
fn world() -> App {
    let mut app = build_test_app();
    let mut config = app.world.resource::<SimulationConfig>().clone();
    config.map_seed = STANDARD_SEED;
    app.world.insert_resource(config);
    app.update();
    app
}

/// **A patch tile whose realized basket mixes food with fibre** — the tile every fixture here works,
/// resolved through the one `tile_flora_composition` seam and pinned to the richest such tile so the
/// take is large enough to measure. A cash crop (`provisions_per_biomass: 0`, paid in materials) is
/// what makes "food or fibre" a real question on one stand.
fn a_mixed_patch_tile(app: &mut App) -> (bevy::prelude::Entity, UVec2) {
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let flora = app.world.resource::<FloraConfigHandle>().get();
    let map_seed = app.world.resource::<SimulationConfig>().map_seed;
    let coord = {
        let mut query = app.world.query::<(&Tile, &core_sim::FoodModuleTag)>();
        let registry = app.world.resource::<ForageRegistry>();
        query
            .iter(&app.world)
            .filter(|(_, module)| module.seasonal_weight > 0.0)
            .filter(|(tile, _)| {
                let composition = tile_flora_composition(&flora, &labor.forage, tile, map_seed);
                let pays_food = composition.iter().any(|entry| {
                    flora
                        .species
                        .get(&entry.species)
                        .is_some_and(pays_food_alone)
                });
                let pays_material = composition.iter().any(|entry| {
                    flora
                        .species
                        .get(&entry.species)
                        .is_some_and(|def| !def.yield_.materials.is_empty())
                });
                pays_food && pays_material
            })
            .filter_map(|(tile, _)| registry.patch(tile.position))
            .max_by(|a, b| {
                a.carrying_capacity
                    .total_cmp(&b.carrying_capacity)
                    .then_with(|| b.tile.y.cmp(&a.tile.y))
                    .then_with(|| b.tile.x.cmp(&a.tile.x))
            })
            .expect("the standard map must carry an in-season patch growing food and fibre")
            .tile
    };
    drop(labor);
    drop(flora);
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(coord.x, coord.y)
        .expect("tile entity resolves");
    (entity, coord)
}

/// **Is this plant food and nothing else?** — the predicate the "more food, no fibre" fixture
/// narrows on. Stated once, because the tile search and the selection must agree about which plants
/// they mean or the fixture measures a tile that cannot show the effect.
fn pays_food_alone(def: &core_sim::FloraDef) -> bool {
    def.yield_.provisions_per_biomass > 0.0 && def.yield_.materials.is_empty()
}

/// Two plants on one tile that pay the **same** material, one of which pays no food.
struct SharedMaterial {
    material: String,
    first: String,
    second: String,
}

/// **A patch whose basket names ONE MATERIAL FROM TWO SPECIES**, at least one of them a cash crop —
/// the only shape that can tell a per-species decomposition apart from a last-write-wins one.
/// Resolved against the live roster and the tile's own realized basket, and pinned to the richest
/// such tile so the take is large enough to measure.
fn a_patch_where_two_plants_pay_one_material(
    app: &mut App,
) -> (bevy::prelude::Entity, UVec2, SharedMaterial) {
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let flora = app.world.resource::<FloraConfigHandle>().get();
    let map_seed = app.world.resource::<SimulationConfig>().map_seed;
    let shared_on = |composition: &[FloraShare]| -> Option<SharedMaterial> {
        // Ordered so the pick is deterministic across runs, not merely stable within one.
        let mut by_material: std::collections::BTreeMap<&str, Vec<&str>> = Default::default();
        for entry in composition {
            let Some(def) = flora.species.get(&entry.species) else {
                continue;
            };
            for row in &def.yield_.materials {
                by_material
                    .entry(row.material.as_str())
                    .or_default()
                    .push(entry.species.as_str());
            }
        }
        let (material, plants) = by_material.iter().find(|(_, plants)| plants.len() > 1)?;
        // The headline case: one of the two must be a cash crop, so the narrowed run is the "tick
        // cotton, see how much fibre" question and not merely a byproduct check.
        plants
            .iter()
            .any(|species| {
                flora
                    .species
                    .get(*species)
                    .is_some_and(|def| def.yield_.provisions_per_biomass == 0.0)
            })
            .then(|| SharedMaterial {
                material: (*material).to_string(),
                first: plants[0].to_string(),
                second: plants[1].to_string(),
            })
    };
    let found = {
        let mut query = app.world.query::<(&Tile, &core_sim::FoodModuleTag)>();
        let registry = app.world.resource::<ForageRegistry>();
        query
            .iter(&app.world)
            .filter(|(_, module)| module.seasonal_weight > 0.0)
            .filter_map(|(tile, _)| {
                let composition = tile_flora_composition(&flora, &labor.forage, tile, map_seed);
                let shared = shared_on(&composition)?;
                let patch = registry.patch(tile.position)?;
                Some((patch.tile, patch.carrying_capacity, shared))
            })
            .max_by(|a, b| {
                a.1.total_cmp(&b.1)
                    .then_with(|| b.0.y.cmp(&a.0.y))
                    .then_with(|| b.0.x.cmp(&a.0.x))
            })
            .expect("the standard map must carry a patch where two plants pay one material")
    };
    drop(labor);
    drop(flora);
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(found.0.x, found.0.y)
        .expect("tile entity resolves");
    (entity, found.0, found.2)
}

/// What is growing on `coord`, through the one seam the sim judges and pays with.
fn tile_composition(app: &App, coord: UVec2) -> Vec<FloraShare> {
    let labor = app.world.resource::<LaborConfigHandle>().get();
    let flora = app.world.resource::<FloraConfigHandle>().get();
    let map_seed = app.world.resource::<SimulationConfig>().map_seed;
    let entity = app
        .world
        .resource::<TileRegistry>()
        .index(coord.x, coord.y)
        .expect("tile entity resolves");
    let ground = app.world.get::<Tile>(entity).expect("the tile");
    tile_flora_composition(&flora, &labor.forage, ground, map_seed).into_owned()
}

fn patch_at(app: &App, coord: UVec2) -> ForagePatch {
    app.world
        .resource::<ForageRegistry>()
        .patch(coord)
        .expect("the fixture tile carries a patch")
        .clone()
}

fn stock_patch(app: &mut App, coord: UVec2, fraction: f32) {
    let capacity = patch_at(app, coord).carrying_capacity;
    set_standing_crop(app, coord, capacity * fraction);
}

fn set_standing_crop(app: &mut App, coord: UVec2, biomass: f32) {
    let mut registry = app.world.resource_mut::<ForageRegistry>();
    registry
        .patch_mut(coord)
        .expect("the fixture tile carries a patch")
        .biomass = biomass;
}

/// One run of the mechanic: gather the fixture tile with a crew whose own throughput binds, and
/// report `(food banked, material banked)`.
fn gather_with(
    select: impl Fn(&[FloraShare], &core_sim::FloraConfig) -> TakeSelection,
) -> (f32, f32) {
    let mut app = world();
    let (tile_entity, coord) = a_mixed_patch_tile(&mut app);
    stock_patch(&mut app, coord, STOCKED_STANDING_CROP);
    let composition = tile_composition(&app, coord);
    let take = {
        let flora = app.world.resource::<FloraConfigHandle>().get();
        select(&composition, &flora)
    };
    let band = spawn_forager(&mut app, tile_entity, coord, TWO_HANDS, take);
    app.world.run_system_once(advance_labor_allocation);
    (band_food(&app, band), band_materials(&app, band))
}

fn band_food(app: &App, band: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the foraging band still exists")
        .stores
        .get(core_sim::FOOD)
        .to_f32()
}

fn band_material(app: &App, band: bevy::prelude::Entity, material: &str) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the foraging band still exists")
        .stores
        .material_total(material)
        .to_f32()
}

/// **Every material the band holds, summed** — this file asks *did any fibre come home*, and which
/// material it is belongs to `materials.rs`.
fn band_materials(app: &App, band: bevy::prelude::Entity) -> f32 {
    app.world
        .get::<PopulationCohort>(band)
        .expect("the foraging band still exists")
        .stores
        .materials()
        .flat_map(|(_, batches)| batches.values())
        .map(|batch| batch.amount.to_f32())
        .sum()
}

/// The fixture band's own forage row, off the **encoded** snapshot.
fn published_row<T>(
    app: &mut App,
    read: impl Fn(shadow_scale_flatbuffers::generated::shadow_scale::sim::LaborAssignment) -> T,
) -> T {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    app.world.run_system_once(recapture_snapshot_in_place);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let row = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .population()
        .and_then(|section| section.populations())
        .expect("the population section carries the cohort list")
        .iter()
        .flat_map(|cohort| cohort.laborAssignments().into_iter().flatten())
        .find(|assignment| assignment.kind().unwrap_or_default() == "forage")
        .expect("the fixture band's forage row is on the wire");
    read(row)
}

/// One entry of a published patch basket, with the vectors that ride index-aligned beside it zipped
/// back onto it — which is how a client is meant to read them, and how a test proves the alignment
/// rather than assuming it.
struct PublishedShare {
    species: String,
    share: f32,
    provisions_per_biomass: f32,
    fodder_per_biomass: f32,
    /// `(material id, amount)` — **empty is "no row"**, which is what a grain publishes.
    materials: Vec<(String, f32)>,
}

/// The patch row's basket off the **encoded** snapshot, zipped. Asserts the vectors are the same
/// length, because that is the contract a client depends on and a codec bug would break silently.
fn published_patch_basket(app: &mut App, coord: UVec2) -> Vec<PublishedShare> {
    published_patch(app, coord, |patch| {
        let composition = patch.composition().expect("the patch publishes a basket");
        let provisions = patch
            .compositionProvisionsPerBiomass()
            .expect("the patch publishes a per-species food rate");
        let fodder = patch
            .compositionFodderPerBiomass()
            .expect("the patch publishes a per-species fodder rate");
        let materials = patch
            .compositionMaterialPerBiomass()
            .expect("the patch publishes per-species material rows");
        assert_eq!(
            materials.len(),
            composition.len(),
            "the per-species material rows are index-aligned with the basket"
        );
        assert_eq!(
            (composition.len(), provisions.len()),
            (composition.len(), composition.len()),
            "the per-species rates are index-aligned with the basket"
        );
        assert_eq!(fodder.len(), composition.len());
        composition
            .iter()
            .enumerate()
            .map(|(index, entry)| PublishedShare {
                species: entry.species().unwrap_or_default().to_string(),
                share: entry.share(),
                provisions_per_biomass: provisions.get(index),
                fodder_per_biomass: fodder.get(index),
                materials: materials
                    .get(index)
                    .rows()
                    .into_iter()
                    .flatten()
                    .map(|row| {
                        (
                            row.materialId().unwrap_or_default().to_string(),
                            row.amount(),
                        )
                    })
                    .collect(),
            })
            .collect()
    })
}

/// The patch row's **basket-averaged** rates — the pair the per-species table must sum to.
fn published_patch_rates(app: &mut App, coord: UVec2) -> (f32, f32) {
    published_patch(app, coord, |patch| {
        (patch.provisionsPerBiomass(), patch.fodderPerBiomass())
    })
}

fn published_patch<T>(
    app: &mut App,
    coord: UVec2,
    read: impl Fn(shadow_scale_flatbuffers::generated::shadow_scale::sim::ForagePatchState) -> T,
) -> T {
    use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

    app.world.run_system_once(recapture_snapshot_in_place);
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let patch = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .subsistence()
        .and_then(|section| section.foragePatches())
        .expect("the subsistence section carries the patch list")
        .iter()
        .find(|patch| patch.x() == coord.x && patch.y() == coord.y)
        .expect("the fixture patch is on the wire");
    read(patch)
}

fn published_workers_needed(app: &mut App) -> u32 {
    published_row(app, |row| row.workersNeeded())
}

fn published_take_species(app: &mut App) -> Vec<String> {
    published_row(app, |row| {
        row.takeSpecies()
            .into_iter()
            .flatten()
            .map(str::to_string)
            .collect()
    })
}

/// A band standing on `tile`, gathering `patch` at [`DEEP_DRAW_FLOOR`] with `workers` hands and the
/// stated take selection.
fn spawn_forager(
    app: &mut App,
    tile: bevy::prelude::Entity,
    patch: UVec2,
    workers: u32,
    take_species: TakeSelection,
) -> bevy::prelude::Entity {
    spawn_band(app, tile, patch, workers, take_species, NO_BUILDERS)
}

/// **No builders staffed** — every fixture but the build one, which is the only place a queue entry
/// exists to raise.
const NO_BUILDERS: u32 = 0;

/// The same band with its **builders pool staffed and a `Cultivate` queued on the patch** — the
/// fixture for "the gatherers' selection has no bearing on the build".
fn spawn_forager_building(
    app: &mut App,
    tile: bevy::prelude::Entity,
    patch: UVec2,
    workers: u32,
    take_species: TakeSelection,
) -> bevy::prelude::Entity {
    spawn_band(app, tile, patch, workers, take_species, BUILDERS)
}

fn spawn_band(
    app: &mut App,
    tile: bevy::prelude::Entity,
    patch: UVec2,
    workers: u32,
    take_species: TakeSelection,
    builders: u32,
) -> bevy::prelude::Entity {
    app.world
        .spawn((
            PopulationCohort {
                home: tile,
                current_tile: tile,
                size: 30,
                children: scalar_zero(),
                // Sized to exactly what it staffs, so `normalize` never trims a row under test.
                working: scalar_from_f32((workers + builders) as f32),
                elders: scalar_zero(),
                stores: LocalStore::new(),
                morale: scalar_one(),
                last_food_consumption: 0.0,
                last_turn_transfer_received: 0.0,
                last_turn_transfer_sent: 0.0,
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
            },
            StartingUnit {
                kind: "BandForager".to_string(),
                tags: Vec::new(),
            },
            ResidentBand,
            LaborAllocation {
                assignments: build_rows(patch, workers, take_species, builders),
                // **A DECLARATION IS A HOLDING** — it is also what keeps the row alive at zero
                // gatherers, which is the state the build fixture measures. Empty when nobody is
                // building, so every other fixture here is an ordinary unqueued gather.
                build_queue: if builders > 0 {
                    vec![core_sim::BuildQueueEntry {
                        source: core_sim::BuildSource::Patch(patch),
                        declared: core_sim::BuildJob::Rung(core_sim::Improvement::Cultivate),
                        kit: None,
                    }]
                } else {
                    Vec::new()
                },
                ..Default::default()
            },
        ))
        .id()
}

/// The band's rows: the take crew, plus the **builders pool** when the fixture is raising something.
/// The builders are a band-level role — they stand on no source — which is the whole reason the
/// gatherers' take selection cannot speak for them.
fn build_rows(
    patch: UVec2,
    workers: u32,
    take_species: TakeSelection,
    builders: u32,
) -> Vec<LaborAssignment> {
    let mut rows = vec![LaborAssignment {
        target: LaborTarget::Forage {
            tile: patch,
            floor: DEEP_DRAW_FLOOR,
            species: None,
            take_species,
        },
        workers,
        kit: None,
    }];
    if builders > 0 {
        rows.push(LaborAssignment {
            target: LaborTarget::Builders,
            workers: builders,
            kit: None,
        });
    }
    rows
}
