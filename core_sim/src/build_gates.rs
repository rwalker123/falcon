//! **THE BUILD GATES A BAND-KIND OR TILE-KIND VERB ASKS, ONE COPY.**
//!
//! `build <verb> ...` (the server's `validate_improvement`) and a chief's build proposal
//! ([`crate::chief_proposals`]) judge "could this band declare this build now" through these
//! functions, so a proposal can never be one the command would refuse. They take the pieces of the
//! world they read ([`GateScene`]) rather than a whole `App`, so both a `&World` (the command) and a
//! system's own parameters (the proposal pass) can build one.

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
