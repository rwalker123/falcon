//! **THE BUILD GATES A BAND-KIND OR TILE-KIND VERB ASKS, ONE COPY.**
//!
//! `build <verb> ...` (the server's `validate_improvement`) and a chief's build proposal
//! ([`crate::chief_proposals`]) judge "could this band declare this build now" through these
//! functions, so a proposal can never be one the command would refuse. They take the pieces of the
//! world they read ([`GateScene`]) rather than a whole `App`, so both a `&World` (the command) and a
//! system's own parameters (the proposal pass) can build one.

#[allow(unused_imports)]
use crate::*;
use bevy::ecs::world::World;
use bevy::math::UVec2;

use crate::belief::BeliefRegistry;
use crate::components::{BandId, Tile};
use crate::forage::rung_site_refusal;
use crate::intensification::{knows, BranchId, LadderConfig, RungDef, SiteRefusal};
use crate::labor_config::LaborConfig;
use crate::orders::FactionId;
use crate::resources::DiscoveryProgressLedger;
use crate::tile_improvement::TileImprovementRegistry;

/// The hint a refused second Corral carries.
pub const PEN_TAKEN_HINT: &str = "Another band is already penning this herd";

/// What the gates read of the world.
pub struct GateScene<'a> {
    pub ladder: &'a LadderConfig,
    pub discovery: &'a DiscoveryProgressLedger,
    pub improvements: &'a TileImprovementRegistry,
    pub labor: &'a LaborConfig,
    pub belief: &'a BeliefRegistry,
    /// The ground at a tile, `None` off the map.
    pub ground: &'a dyn Fn(UVec2) -> Option<&'a Tile>,
}

/// **The land's refusal, phrased for the player** — one wording per fault, shared by every plant
/// gate, so the same ground never gets two different explanations depending on which verb asked.
pub fn site_refusal_message(refusal: SiteRefusal, tile: UVec2, verb: &str) -> String {
    match refusal {
        SiteRefusal::NotGatheringSite => format!(
            "Nobody gathers at ({}, {}) — your people cannot {} ground they do not already work. \
             Choose a gathering site, or move to one.",
            tile.x, tile.y, verb
        ),
        SiteRefusal::NoDeposit => format!(
            "There is not enough at ({}, {}) to {} — a scatter is not a body of rock. Find real \
             stone: the highlands, the plateaus and the badlands carry it.",
            tile.x, tile.y, verb
        ),
        SiteRefusal::TooPoor => format!(
            "Nothing will grow at ({}, {}) — that ground is too thin to take a crop. Your people \
             cannot yet feed the land.",
            tile.x, tile.y
        ),
        SiteRefusal::TooDry => format!(
            "Nothing will grow at ({}, {}) — that ground is too dry to take a crop. Your people can \
             carry seed, but not yet water: sow the well-watered ground along the rivers.",
            tile.x, tile.y
        ),
        SiteRefusal::TooPoorAndTooDry => format!(
            "Nothing will grow at ({}, {}) — that ground is too thin and too dry to take a crop. \
             Your people can carry seed, but not yet water or feed the land.",
            tile.x, tile.y
        ),
        SiteRefusal::NoBelief => format!(
            "({}, {}) does not hold enough belief to {} - belief gathers where people have buried \
             their dead and met, so hold more gatherings and bury your dead there first.",
            tile.x, tile.y, verb
        ),
        SiteRefusal::NeedsLesson => format!(
            "Your people have not learned how to {} ground beyond the sites they already gather, \
             at ({}, {}) - a lesson they have not learned would open it.",
            verb, tile.x, tile.y
        ),
        SiteRefusal::PenTaken => PEN_TAKEN_HINT.to_string(),
    }
}

/// **"Your people have not learned X yet"** — the knowledge gate, off the rung record's own
/// `unlock_knowledge`, judged by the ladder's own `knows` so the command and the turn compare one
/// threshold. `None` when the rung is ungated or the knowledge is known.
pub fn knowledge_refusal(
    scene: &GateScene<'_>,
    faction: FactionId,
    rung: &RungDef,
) -> Option<String> {
    let id = rung.unlock_discovery_id()?;
    let ladder = scene.ladder;
    let known = knows(
        scene.discovery,
        faction,
        id,
        ladder.knowledge.completion_threshold,
    );
    (!known).then(|| {
        let name = rung.unlock_knowledge.as_deref().unwrap_or_default();
        let title = crate::knowledge_title_from_id(name);
        // **How it is learned is the ladder's own fact**: the rung that `earns_knowledge` this name
        // teaches it, so the sentence names that rung's `label` rather than a per-verb tail.
        let teacher = ladder
            .branches_in_config_order()
            .into_iter()
            .flat_map(|branch| ladder.rungs_of(branch))
            .find(|candidate| candidate.earns_knowledge.as_deref() == Some(name));
        match teacher {
            Some(teacher) => format!(
                "Your people have not learned {title} yet. Sustain-work the {} to learn it.",
                teacher.label.to_lowercase()
            ),
            None => format!("Your people have not learned {title} yet."),
        }
    })
}

/// **A tile-kind verb's gate**: the faction knows the rung, the ground meets its `site_requirement`,
/// and the tile stands on the rung beneath (or at the branch's root).
pub fn validate_tile_verb(
    scene: &GateScene<'_>,
    faction: FactionId,
    tile: UVec2,
    branch: BranchId,
    rung: &RungDef,
) -> Result<(), String> {
    let ladder = scene.ladder;
    if let Some(refusal) = knowledge_refusal(scene, faction, rung) {
        return Err(refusal);
    }
    let destination = rung.rung_id();
    let required = ladder
        .below_id(destination)
        .expect("a rung a verb raises stands on the rung beneath it");
    let label = rung.label.to_lowercase();
    // **The ground, through the one `rung_site_refusal` seam** every other source's command and
    // gate resolve.
    if rung.site_requirement.is_some() {
        let Some(ground) = (scene.ground)(tile) else {
            return Err(format!("There is no tile at ({}, {}).", tile.x, tile.y));
        };
        if let Some(refusal) = rung_site_refusal(
            rung,
            ground,
            &scene.labor.forage,
            // A tile rung asks nothing about gathering or water: both are passed permissive.
            true,
            true,
            crate::NO_DEPOSIT_FLOOR,
            scene.belief.get(tile),
            // A tile rung declares no `site_relaxations`, so no lesson changes its rule.
            &|_| false,
        ) {
            return Err(site_refusal_message(
                refusal,
                tile,
                rung.verb.as_deref().unwrap_or(&label),
            ));
        }
    }
    match scene.improvements.get(tile, branch) {
        Some(improvement) if improvement.faction != faction => Err(format!(
            "The {} at ({}, {}) belongs to another people.",
            ladder.rung(improvement.held_rung()).label.to_lowercase(),
            tile.x,
            tile.y
        )),
        Some(improvement) if ladder.at_or_above(improvement.held_rung(), destination) => Err(
            format!("The {label} at ({}, {}) is already built.", tile.x, tile.y),
        ),
        Some(improvement) if !ladder.at_or_above(improvement.held_rung(), required) => {
            Err(format!(
                "It is only a {} - it must be a {} before you can raise it to a {label}.",
                ladder.rung(improvement.held_rung()).label.to_lowercase(),
                ladder.rung(required).label.to_lowercase(),
            ))
        }
        Some(_) => Ok(()),
        None if ladder.root_of(branch).map(RungDef::rung_id) == Some(required) => Ok(()),
        None => Err(format!(
            "Nothing stands at ({}, {}) yet - raise the {} first.",
            tile.x,
            tile.y,
            ladder.rung(required).label.to_lowercase()
        )),
    }
}

/// **A band-kind verb's gate** — [`validate_tile_verb`]'s twin for an improvement a band holds at its
/// camp. The caller has already established that `faction` commands `band`.
pub fn validate_band_verb(
    scene: &GateScene<'_>,
    faction: FactionId,
    band: BandId,
    branch: BranchId,
    rung: &RungDef,
) -> Result<(), String> {
    let ladder = scene.ladder;
    if let Some(refusal) = knowledge_refusal(scene, faction, rung) {
        return Err(refusal);
    }
    let destination = rung.rung_id();
    let required = ladder
        .below_id(destination)
        .expect("a rung a verb raises stands on the rung beneath it");
    let label = rung.label.to_lowercase();
    match scene.improvements.bands().get(band, branch) {
        Some(improvement) if ladder.at_or_above(improvement.held_rung(), destination) => {
            Err(format!("Band {}'s {label} is already built.", band.0))
        }
        Some(improvement) if !ladder.at_or_above(improvement.held_rung(), required) => {
            Err(format!(
                "It is only a {} - it must be a {} before you can raise it to a {label}.",
                ladder.rung(improvement.held_rung()).label.to_lowercase(),
                ladder.rung(required).label.to_lowercase(),
            ))
        }
        Some(_) => Ok(()),
        None if ladder.root_of(branch).map(RungDef::rung_id) == Some(required) => Ok(()),
        None => Err(format!(
            "Band {} holds no {} yet - raise the {} first.",
            band.0,
            branch.as_str(),
            ladder.rung(required).label.to_lowercase()
        )),
    }
}

/// Build a [`GateScene`] over `world` and run `f` on it - the one place a scene is assembled from a
/// live world, shared by the `build` command and the chief's proposal pass.
pub fn with_scene<R>(world: &bevy::prelude::World, f: impl FnOnce(&GateScene<'_>) -> R) -> R {
    use crate::labor_config::LaborConfigHandle;
    use crate::resources::TileRegistry;
    let ladder = world.resource::<crate::LadderConfigHandle>().get();
    let labor = world.resource::<LaborConfigHandle>().get();
    let registry = world.get_resource::<TileRegistry>();
    let ground = |tile: UVec2| -> Option<&Tile> {
        registry
            .and_then(|registry| registry.index(tile.x, tile.y))
            .and_then(|entity| world.get::<Tile>(entity))
    };
    let scene = GateScene {
        ladder: &ladder,
        discovery: world.resource::<DiscoveryProgressLedger>(),
        improvements: world.resource::<TileImprovementRegistry>(),
        labor: &labor,
        belief: world.resource::<BeliefRegistry>(),
        ground: &ground,
    };
    f(&scene)
}

/// The knowledge gate over a live world.
pub fn knowledge_refusal_world(
    world: &World,
    faction: FactionId,
    rung: &RungDef,
) -> Option<String> {
    with_scene(world, |scene| knowledge_refusal(scene, faction, rung))
}

/// **The next step up from `rung`** as a sentence tail - the verb that raises the rung above it, or
/// that there is nothing higher. What an "already built" refusal tells the player to do next.
pub fn next_step_tail(ladder: &crate::LadderConfig, rung: &crate::RungDef) -> String {
    match ladder
        .above_rung(rung)
        .and_then(|above| above.verb.as_deref().map(|verb| (above, verb)))
    {
        Some((above, verb)) => format!(
            "`build {verb}` takes it up to {}.",
            above.label.to_lowercase()
        ),
        None => "there is nothing higher to raise it to.".to_string(),
    }
}

/// **The rung to raise first** - the verb of the rung beneath `rung` on its branch, when it has one.
pub fn rung_below_tail(ladder: &crate::LadderConfig, rung: &crate::RungDef) -> String {
    let below = ladder
        .rungs_of(rung.branch)
        .into_iter()
        .rfind(|candidate| candidate.order < rung.order);
    match below.and_then(|below| below.verb.as_deref().map(|verb| (below, verb))) {
        Some((below, verb)) => format!(
            "raise the {} first (`build {verb}`).",
            below.label.to_lowercase()
        ),
        None => "raise that first.".to_string(),
    }
}

/// **Does the land admit `rung` here?** — the ONE place the server resolves a plant rung's
/// `site_requirement`, so the gather gate, `cultivate` and `sow` cannot drift into disagreeing about
/// which ground may be worked. It gathers the three readings [`rung_site_refusal`] judges (is this a
/// gathering site, what is the tile's own forage capacity, is it fresh-watered) and hands them to
/// that one seam — the same seam the labor arm's placement gate and the wire's `sowSiteRefusal` use.
///
/// `None` on a tile that is off the map: the caller's own "there is no tile there" error is the
/// better message, and inventing a site refusal for a coordinate that does not exist would hide it.
///
/// **A world with no `TileRegistry` answers `None`, and that must stay a `get_resource`** — the idle
/// boot and the command-unit harnesses run an `App` that has never generated a map, and
/// `Command::AssignLabor` is dispatched with no `world_active` gate, so a `assign_labor … forage`
/// arriving before `new_game` reaches this function. Panicking here unwinds out of the command loop
/// and kills the server. The permissive answer is the same one [`validate_species_selection`] gives
/// for exactly this case: with no tiles there is no ground to judge, and the labor arm — which
/// always has the real tiles — remains the authority.
pub fn plant_rung_site_refusal(
    world: &World,
    faction: FactionId,
    rung_key: RungId,
    tile: UVec2,
) -> Option<SiteRefusal> {
    let registry = world.get_resource::<TileRegistry>()?;
    let tile_entity = registry.index(tile.x, tile.y)?;
    let ground = world.get::<Tile>(tile_entity)?;
    let (grid_width, grid_height) = (registry.width, registry.height);
    let wrap_horizontal = world
        .resource::<SimulationConfig>()
        .map_topology
        .wrap_horizontal;
    let fresh_water =
        tile_is_fresh_watered(ground, grid_width, grid_height, wrap_horizontal, |coord| {
            registry
                .index(coord.x, coord.y)
                .and_then(|entity| world.get::<Tile>(entity))
                .map(|neighbor| neighbor.terrain_tags)
        });
    let labor = world.resource::<LaborConfigHandle>().get();
    let ladder = world.resource::<LadderConfigHandle>().get();
    rung_site_refusal(
        ladder.rung(rung_key),
        ground,
        &labor.forage,
        world.resource::<FoodSiteRegistry>().is_site(tile),
        fresh_water,
        // **A plant rung asks nothing of a deposit** — every rung off the two deposit branches
        // leaves `min_deposit_capacity` at this neutral, so the term cannot refuse one.
        crate::NO_DEPOSIT_FLOOR,
        // **A plant rung asks nothing of belief** - only a belief branch's rungs set `min_belief`.
        crate::NO_BELIEF,
        // **The acting faction's lessons**: a rung's `site_relaxations` loosen its site rule for a
        // people that holds the lesson, and every gate reads the same answer.
        &|name| {
            crate::knows_lesson(
                world.resource::<DiscoveryProgressLedger>(),
                faction,
                name,
                ladder.knowledge.completion_threshold,
            )
        },
    )
}

/// **MAY THIS BAND RAISE THIS RUNG ON THIS DEPOSIT?** — the deposit branches' arm of
/// [`validate_improvement`], and the twin of `validate_sow`.
///
/// Two terms, and they are exactly the two `deposit_head_gate` (in `systems::labor`) resolves for the turn:
/// - **the ground**, through the **same** `rung_site_refusal` seam every plant gate uses, so the
///   command's rejection and the labor arm's gate cannot drift into disagreeing about which ground
///   takes a working. On the shipped ladder only `extraction:quarry` asks anything — the tile's own
///   stone must clear its `min_deposit_capacity` — which is the whole of *you cannot quarry just
///   anywhere*;
/// - **the knowledge**, off the rung record's own `unlock_discovery_id`, never a literal.
///
/// ⛔ **THE STOCK IS DELIBERATELY NOT A TERM.** A scatter already worked down to
/// `extraction:gathering`'s floor has nothing reachable, and opening a quarry is precisely how you
/// reach deeper — refusing there would be the `plan_standing_upkeep.md` §6 floor trap arriving by a
/// different door.
pub fn validate_deposit_verb(
    world: &World,
    faction: FactionId,
    tile: UVec2,
    material: &str,
    improvement: VerbId,
) -> Result<(), String> {
    let extraction = world.resource::<ExtractionConfigHandle>().get();
    let Some(deposit) = extraction.deposit(material) else {
        return Err(format!(
            "'{material}' is not something that comes out of the ground."
        ));
    };
    let ladder = world.resource::<LadderConfigHandle>().get();
    let Some(rung) = ladder.rung_for_verb(improvement) else {
        return Err(format!(
            "'{}' is not a build verb this ladder knows.",
            improvement.as_str()
        ));
    };
    if rung.branch != deposit.branch {
        return Err(format!(
            "'{}' is not how you work {material} — that is the other ladder.",
            improvement.as_str()
        ));
    }
    let labor = world.resource::<LaborConfigHandle>().get();
    let registry = world.resource::<TileRegistry>();
    let Some(ground) = registry
        .index(tile.x, tile.y)
        .and_then(|entity| world.get::<Tile>(entity))
    else {
        return Err(format!("There is no tile at ({}, {}).", tile.x, tile.y));
    };
    if let Some(refusal) = rung_site_refusal(
        rung,
        ground,
        &labor.forage,
        // A deposit rung asks nothing about gathering or water, so both readings are passed at
        // their permissive values rather than looked up.
        true,
        true,
        crate::extraction::tile_deposit_capacity(&extraction, material, ground),
        crate::NO_BELIEF,
        // A deposit rung declares no `site_relaxations`, so no lesson changes its rule.
        &|_| false,
    ) {
        return Err(site_refusal_message(refusal, tile, improvement.as_str()));
    }
    // The ladder's own `knows`, so the command's gate and the turn's are one comparison.
    if let Some(refusal) = knowledge_refusal_world(world, faction, rung) {
        return Err(refusal);
    }
    Ok(())
}

/// **THE REFUSAL FOR GROUND A LESSON WOULD OPEN, naming the lesson and where it is learned** - the
/// [`SiteRefusal::NeedsLesson`] answer with its particulars filled from the rung's own
/// `site_relaxations` and the rung that `earns_knowledge` it, so the sentence says what to do next.
pub fn needs_lesson_message(
    ladder: &crate::LadderConfig,
    rung: &crate::RungDef,
    tile: UVec2,
    verb: &str,
) -> String {
    let Some(lesson) = rung
        .site_relaxations
        .last()
        .map(|relaxation| relaxation.knowledge.as_str())
    else {
        return site_refusal_message(SiteRefusal::NeedsLesson, tile, verb);
    };
    let title = crate::knowledge_title_from_id(lesson);
    let teacher = ladder
        .branches_in_config_order()
        .into_iter()
        .flat_map(|branch| ladder.rungs_of(branch))
        .find(|candidate| candidate.earns_knowledge.as_deref() == Some(lesson));
    match teacher {
        Some(teacher) => format!(
            "({}, {}) is not ground your people already gather. Learn {title} to {verb} beyond \
             the gathering sites - work a {} to learn it.",
            tile.x,
            tile.y,
            teacher.label.to_lowercase()
        ),
        None => format!(
            "({}, {}) is not ground your people already gather. Learn {title} to {verb} beyond \
             the gathering sites.",
            tile.x, tile.y
        ),
    }
}

/// **MAY THIS FACTION RAISE `rung` ON THE PATCH AT `tile`?** - every patch-kind verb's gates,
/// composed from the rung's own properties and in one order, so the two old per-verb validators
/// (`cultivate`, `sow`) are one function and a patch branch added by config is gated for free. The
/// terms, in order: the tile exists, the land admits the rung (`site_requirement`), the knowledge
/// (`unlock_knowledge`), a patch is there unless the rung `places_source`, the rung is not already
/// held, `requires_ownership` (not another people's), and `requires_crop` (the selection is one
/// this ground can grow).
///
/// The land is judged before the knowledge because it is a property of the *place*, not of the
/// player (the order `sow` has always had).
pub fn validate_patch_build(
    world: &World,
    faction: FactionId,
    tile: UVec2,
    species: Option<&str>,
    rung: &crate::RungDef,
) -> Result<(), String> {
    // `get_resource`, not `resource`: a world that has never been generated carries no
    // `TileRegistry`, and this is reachable from an ungated `assign_labor` before `new_game`.
    if let Some(registry) = world.get_resource::<TileRegistry>() {
        if registry.index(tile.x, tile.y).is_none() {
            return Err(format!("There is no tile at ({}, {}).", tile.x, tile.y));
        }
    }
    let ladder = world.resource::<LadderConfigHandle>().get();
    let verb = rung.verb.as_deref().unwrap_or_default();
    let key = rung.rung_id();
    if let Some(refusal) = plant_rung_site_refusal(world, faction, key, tile) {
        return Err(match refusal {
            SiteRefusal::NeedsLesson => needs_lesson_message(&ladder, rung, tile, verb),
            other => site_refusal_message(other, tile, verb),
        });
    }
    if let Some(refusal) = knowledge_refusal_world(world, faction, rung) {
        return Err(refusal);
    }
    let registry = world.resource::<ForageRegistry>();
    let patch = registry.patch(tile);
    if patch.is_none() && !rung.places_source {
        return Err(format!("No forage patch at ({}, {}).", tile.x, tile.y));
    }
    if let Some(patch) = patch {
        // **The meter's fullness is the achieved rung** (one position, no retention bar), so
        // re-queueing an eroded rung is an ordinary build and a full one is refused.
        if patch.standing().holds_at_or_above(rung) {
            return Err(format!(
                "The {} at ({}, {}) is already built - {}",
                rung.label.to_lowercase(),
                tile.x,
                tile.y,
                next_step_tail(&ladder, rung)
            ));
        }
        if rung.requires_ownership && patch.owner.is_some_and(|owner| owner != faction) {
            return Err(format!(
                "Another people hold the patch at ({}, {}).",
                tile.x, tile.y
            ));
        }
    }
    if rung.requires_crop {
        return validate_species_selection(world, tile, species, key);
    }
    Ok(())
}

/// **MAY THIS FACTION RAISE `rung` ON THE HERD `fauna_id`?** - every herd-kind verb's gates,
/// composed from the rung's own properties: the knowledge, the herd exists, the species'
/// `ceiling_required`, not already held, `requires_held_rung_below`, `requires_ownership`.
///
/// The species' ceiling is checked *before* ownership because it is a property of the *animal*, not
/// of who is hunting it. There is deliberately no health gate: a herd's phase swings as it is
/// hunted, and the labor arm handles a lapsed phase gracefully.
pub fn validate_herd_build(
    world: &World,
    faction: FactionId,
    fauna_id: &str,
    rung: &crate::RungDef,
) -> Result<(), String> {
    let ladder = world.resource::<LadderConfigHandle>().get();
    if let Some(refusal) = knowledge_refusal_world(world, faction, rung) {
        return Err(refusal);
    }
    let Some(herd) = world.resource::<HerdRegistry>().find(fauna_id) else {
        return Err(format!("No herd '{fauna_id}' to build on."));
    };
    if let Some(required) = rung.ceiling_required {
        if !ladder.ceiling_reaches(herd.husbandry_ceiling, required) {
            return Err(format!(
                "{} cannot be taken to {} - its kind goes no further than {}.",
                herd.species,
                rung.label.to_lowercase(),
                ladder.ceiling_wire_id(herd.husbandry_ceiling, SourceKind::Herd)
            ));
        }
    }
    let standing = herd.standing();
    if standing.holds_at_or_above(rung) {
        return Err(format!(
            "{fauna_id} is already built - it stands as a {}; {}",
            rung.label.to_lowercase(),
            next_step_tail(&ladder, rung)
        ));
    }
    if rung.requires_held_rung_below {
        let below_order = u32::from(standing.held_order) + 1;
        if below_order < rung.order {
            return Err(format!(
                "{fauna_id} has not reached the rung beneath {} yet - {}",
                rung.label.to_lowercase(),
                rung_below_tail(&ladder, rung)
            ));
        }
        // **A rung that needs the one below held ALSO needs it to be ours** - a herd nobody has
        // tamed has no owner to build a pen for.
        if herd.owner != Some(faction) {
            return Err(format!("You do not own {fauna_id}."));
        }
    }
    if rung.requires_ownership && herd.owner.is_some_and(|owner| owner != faction) {
        return Err(format!("Another people hold {fauna_id}."));
    }
    Ok(())
}

/// **May a `Cultivate`/`Sow` on this tile commit to this plant?** — the species-side gate
/// (`docs/plan_flora_roster.md` §4.3), phrased for the player.
///
/// It resolves through the *same* `forage::resolve_committed_species` seam the labor arm commits
/// with, so a selection this accepts can never be one the turn then refuses (the `rung_site_refusal`
/// discipline). The composition comes from `forage::tile_flora_composition` — the one seam — so a
/// navigable hex is judged on the two-term basket it actually has.
pub fn validate_species_selection(
    world: &World,
    tile: UVec2,
    species: Option<&str>,
    rung: RungId,
) -> Result<(), String> {
    // **No map, nothing to judge.** A world that has not been generated yet (the idle boot, and the
    // command-unit harnesses) carries no `TileRegistry` at all, so there is no basket to read; the
    // labor arm, which always has the real tiles, remains the authority and simply accrues nothing
    // if the ground grows nothing that climbs.
    let Some(registry) = world.get_resource::<TileRegistry>() else {
        return Ok(());
    };
    let Some(ground) = registry
        .index(tile.x, tile.y)
        .and_then(|entity| world.get::<Tile>(entity))
    else {
        return Err(format!("There is no tile at ({}, {}).", tile.x, tile.y));
    };
    let labor = world.resource::<LaborConfigHandle>().get();
    let flora = world.resource::<FloraConfigHandle>().get();
    let map_seed = world.resource::<SimulationConfig>().map_seed;
    let composition = tile_flora_composition(&flora, &labor.forage, ground, map_seed);
    let ladder = world.resource::<LadderConfigHandle>().get();
    let verb = if ladder.rung(rung).trait_set().has(crate::RungTrait::Sown) {
        "sown"
    } else {
        "tended"
    };
    match resolve_committed_species(species, &composition, &flora, &ladder, rung) {
        Ok(_) => Ok(()),
        Err(SpeciesRefusal::Unknown) => Err(format!(
            "Your people know no plant called '{}'.",
            species.unwrap_or_default()
        )),
        Err(SpeciesRefusal::CeilingTooLow) => Err(format!(
            "{} cannot be {verb} — it is a wild harvest, gathered where it grows.",
            flora.species.get(species.unwrap_or_default()).map_or_else(
                || species.unwrap_or_default().to_string(),
                |def| def.display_name.clone()
            )
        )),
        Err(SpeciesRefusal::NotHere) => Err(format!(
            "{} does not grow at ({}, {}).",
            flora.species.get(species.unwrap_or_default()).map_or_else(
                || species.unwrap_or_default().to_string(),
                |def| def.display_name.clone()
            ),
            tile.x,
            tile.y
        )),
        Err(SpeciesRefusal::NothingClimbsHere) => Err(format!(
            "Nothing that grows at ({}, {}) can be {verb} — what the ground offers there is a wild \
             harvest.",
            tile.x, tile.y
        )),
    }
}

/// Validate an **build verb** against the source it names — the gate half of the second axis,
/// and the home of every check `validate_labor_policy` used to carry for a build verb.
///
/// 1. **The web must match the source kind.** `Cultivate`/`Sow` are plant-only, `Tame`/`Corral`
///    animal-only (the branch's declared source kind). Rejected
///    outright rather than silently coerced.
/// 2. **Gates — one knowledge per rung-transition** (§4.3). Each verb resolves its gate off its OWN
///    rung record, never a hard-coded id: `Cultivate` needs **Cultivation** + a **Thriving** patch
///    (not already tended, not someone else's); `Sow` needs **Seed Selection** + ground the
///    `plant:farm` rung's `site_requirement` accepts; `Tame` needs **Herding**; and `Corral` needs
///    **Penning** — *not* Herding, which gates `tame` alone — plus an owned, **domesticated**,
///    not-yet-penned herd of a `pen`-ceiling species. Each is unchanged by the axis split.
pub fn validate_improvement(
    world: &World,
    faction: FactionId,
    target: &LaborTarget,
    improvement: VerbId,
) -> Result<(), String> {
    let ladder = world.resource::<LadderConfigHandle>().get();
    let Some(rung) = ladder.rung_for_verb(improvement) else {
        return Err(format!(
            "'{}' is not a build verb this ladder knows.",
            improvement.as_str()
        ));
    };
    // **The kind the verb builds on is the BRANCH's declaration**, read off the ladder - never a
    // per-verb match. A verb aimed at the wrong kind of source is refused outright rather than
    // coerced.
    let verb_kind = ladder.source_kind(rung.branch);
    let wrong_kind = |here: &str| {
        Err(format!(
            "'{}' is not something you build on {here} - it applies to {}.",
            improvement.as_str(),
            verb_kind.map_or("something else", source_kind_noun)
        ))
    };
    match target {
        LaborTarget::Forage { tile, species, .. } => {
            if verb_kind != Some(SourceKind::Patch) {
                return wrong_kind("the land");
            }
            validate_patch_build(world, faction, *tile, species.as_deref(), rung)
        }
        LaborTarget::Hunt { fauna_id, .. } => {
            if verb_kind != Some(SourceKind::Herd) {
                return wrong_kind("a herd");
            }
            validate_herd_build(world, faction, fauna_id, rung)
        }
        LaborTarget::Extract { tile, material, .. } => {
            if verb_kind != Some(SourceKind::Deposit) {
                return wrong_kind("a deposit");
            }
            validate_deposit_verb(world, faction, *tile, material, improvement)
        }
        LaborTarget::Tile { tile, branch } => {
            if verb_kind != Some(SourceKind::Tile) {
                return wrong_kind("a tile improvement");
            }
            with_scene(world, |scene| {
                validate_tile_verb(scene, faction, *tile, *branch, rung)
            })
        }
        LaborTarget::Band { band, branch } => {
            if verb_kind != Some(SourceKind::Band) {
                return wrong_kind("a band");
            }
            let held_by_faction = world.iter_entities().any(|entity| {
                entity.get::<BandId>() == Some(band)
                    && entity
                        .get::<crate::components::PopulationCohort>()
                        .is_some_and(|cohort| cohort.faction == faction)
            });
            if !held_by_faction {
                return Err(format!(
                    "Band {} is not one of your people, so it cannot raise a {}.",
                    band.0,
                    rung.label.to_lowercase()
                ));
            }
            with_scene(world, |scene| {
                validate_band_verb(scene, faction, *band, *branch, rung)
            })
        }
        LaborTarget::Scout | LaborTarget::Warrior | LaborTarget::Roadwork => Err(format!(
            "There is nothing to {} on a standing role.",
            improvement.as_str()
        )),
    }
}

/// How a player-facing sentence names a source kind's rungs' ground.
pub fn source_kind_noun(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Patch => "forage patches",
        SourceKind::Herd => "herds",
        SourceKind::Road => "roads",
        SourceKind::Deposit => "deposits",
        SourceKind::Tile => "tiles",
        SourceKind::Band => "bands",
    }
}
