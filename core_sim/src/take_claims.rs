//! ⛔ **WHAT ONE ROW'S CREW CLAIMS OF THE BAND'S TAKE GEAR** (`docs/plan_site_crews.md` §2.3).
//!
//! A take kit is carried by the **take hands** — the crew less the hands its keeping takes — and a
//! row claims a kit only for the take hands that would take something with it. The claim is
//! **planned as if equipped**, exactly as the keeping claim is: the take hands that reach what the
//! site yields at the row's floor at one fully equipped hand's rate. A hand past that point claims
//! nothing and works bare.
//!
//! # One function of state, read by everyone
//!
//! The turn settles its take gear on these claims **before** any take runs, and every surface that
//! prices a crew between turns — the capture, the assign-time seed, the compose-sheet query, the
//! crew curves, the caravan pricing, the extraction quote — strikes the same claims through the same
//! functions. Between turns the source is read **regrow-first** ([`ProjectionStart`]), so a quote
//! and the next turn's settlement read one stock.
//!
//! The site's room per equipped hand is each web's own function
//! ([`crate::forage::forage_useful_take_hands`], [`crate::fauna::hunt_useful_take_hands`],
//! [`crate::extraction::deposit_useful_take_hands`]); the planned keep hands are the prospective
//! keeping split every quote already strikes ([`crate::systems::prospective_keep_hands`] through each
//! web's seam) — a plan, never the settlement it plans for.

use std::borrow::Cow;

use bevy::prelude::World;
use glam::UVec2;

use crate::combat::CombatStats;
use crate::combat_config::CombatConfig;
use crate::components::{
    BandEquipment, BuildJob, BuildSource, Improvement, LaborAllocation, LaborTarget, Tile,
};
use crate::equipment_config::{EquipmentConfig, KitChoice, Quarry, NO_CLAIMING_HANDS};
use crate::extraction::{DepositRegistry, DepositSource};
use crate::extraction_config::ExtractionConfig;
use crate::fauna::{Herd, HerdRegistry, ProjectionStart};
use crate::fauna_config::FaunaConfig;
use crate::flora_config::FloraConfig;
use crate::forage::{ForagePatch, ForageRegistry};
use crate::intensification::LadderConfig;
use crate::labor_config::LaborConfig;
use crate::materials_config::MaterialsConfig;

/// **A ROLE'S WHOLE CREW CARRIES ITS KIT** — a scout's or a warrior's gear is used by every hand
/// on the role, so no room caps the claim.
const EVERY_HAND_USES_IT: f32 = f32::INFINITY;

/// **WHAT THE CLAIM IS STRUCK FROM** — the world's sources and the tables their rates come from,
/// borrowed once by each surface that settles a band's take gear.
pub struct ClaimSources<'a> {
    pub forage: &'a ForageRegistry,
    pub herds: &'a HerdRegistry,
    pub deposits: &'a DepositRegistry,
    /// **The ground under a tile**, `None` off the map — a patch's basket and a working's capacity.
    pub ground_of: &'a dyn Fn(UVec2) -> Option<Tile>,
    /// **The tile's gathering season** — its food module's `seasonal_weight`, or
    /// [`crate::forage::NO_FORAGE_SEASON`].
    pub season_of: &'a dyn Fn(UVec2) -> f32,
    pub map_seed: u64,
    pub labor: &'a LaborConfig,
    pub flora: &'a FloraConfig,
    pub fauna: &'a FaunaConfig,
    pub equipment: &'a EquipmentConfig,
    pub extraction: &'a ExtractionConfig,
    pub ladder: &'a LadderConfig,
    pub materials: &'a MaterialsConfig,
    pub combat: &'a CombatConfig,
    /// The `person` roster row — what a hunter is before any gear.
    pub person: CombatStats,
    /// **When the surface reads the source** — inside the turn after its regrowth, or between turns
    /// before the next one.
    pub start: ProjectionStart,
}

impl ClaimSources<'_> {
    /// The patch as the next take will find it.
    fn patch(&self, tile: UVec2) -> Option<Cow<'_, ForagePatch>> {
        let patch = self.forage.patch(tile)?;
        Some(match self.start {
            ProjectionStart::AfterRegrowth => Cow::Borrowed(patch),
            ProjectionStart::BeforeRegrowth => {
                Cow::Owned(crate::forage::next_turns_stand(patch, &self.labor.forage))
            }
        })
    }

    /// The herd as the next take will find it.
    fn herd(&self, id: &str) -> Option<Cow<'_, Herd>> {
        let herd = self.herds.find(id)?;
        Some(match self.start {
            ProjectionStart::AfterRegrowth => Cow::Borrowed(herd),
            ProjectionStart::BeforeRegrowth => {
                Cow::Owned(crate::fauna::next_turns_quarry(herd, self.fauna))
            }
        })
    }

    /// The working and its ground as the next take will find them — the live working, or the one
    /// a crew put on fresh ground would open ([`crate::extraction::projected_working`]).
    fn working(&self, tile: UVec2, material: &str) -> Option<(DepositSource, Tile)> {
        let ground = (self.ground_of)(tile)?;
        let mut working = crate::extraction::projected_working(
            self.deposits,
            tile,
            material,
            &ground,
            self.extraction,
        )?;
        if self.start == ProjectionStart::BeforeRegrowth {
            crate::extraction::renew_deposit(&mut working, &ground, self.extraction, self.ladder);
        }
        Some((working, ground))
    }
}

/// **[`ClaimSources`] OFF A WHOLE WORLD, BETWEEN TURNS** — the shape the assign-time seed and the
/// compose-sheet query hold. Read regrow-first, as every between-turns quote is.
///
/// A world missing a registry or a config — a hand-built fixture — reads it as empty or as the
/// shipped table, the reading every surface already gives such a world: no source to claim, and the
/// shipped dials.
pub fn with_world_sources<R>(world: &World, read: impl FnOnce(&ClaimSources<'_>) -> R) -> R {
    fn config<H: bevy::prelude::Resource, T>(
        world: &World,
        get: impl Fn(&H) -> std::sync::Arc<T>,
        builtin: impl Fn() -> std::sync::Arc<T>,
    ) -> std::sync::Arc<T> {
        world.get_resource::<H>().map_or_else(builtin, get)
    }
    let tiles = world.get_resource::<crate::resources::TileRegistry>();
    let ground_of = |pos: UVec2| {
        tiles?
            .index(pos.x, pos.y)
            .and_then(|entity| world.get::<Tile>(entity))
            .cloned()
    };
    let season_of = |pos: UVec2| {
        tiles
            .and_then(|tiles| tiles.index(pos.x, pos.y))
            .and_then(|entity| world.get::<crate::food::FoodModuleTag>(entity))
            .map_or(crate::forage::NO_FORAGE_SEASON, |module| {
                module.seasonal_weight.max(crate::forage::NO_FORAGE_SEASON)
            })
    };
    let labor = config(world, crate::LaborConfigHandle::get, LaborConfig::builtin);
    let flora = config(world, crate::FloraConfigHandle::get, FloraConfig::builtin);
    let fauna = config(world, crate::FaunaConfigHandle::get, FaunaConfig::builtin);
    let equipment = config(
        world,
        crate::EquipmentConfigHandle::get,
        EquipmentConfig::builtin,
    );
    let extraction = config(
        world,
        crate::ExtractionConfigHandle::get,
        ExtractionConfig::builtin,
    );
    let ladder = config(world, crate::LadderConfigHandle::get, LadderConfig::builtin);
    let materials = config(
        world,
        crate::MaterialsConfigHandle::get,
        MaterialsConfig::builtin,
    );
    let combat = config(world, crate::CombatConfigHandle::get, CombatConfig::builtin);
    let person = config(
        world,
        crate::CreaturesConfigHandle::get,
        crate::creatures_config::CreaturesConfig::builtin,
    )
    .person();
    let no_forage = ForageRegistry::default();
    let no_herds = HerdRegistry::default();
    let no_deposits = DepositRegistry::default();
    read(&ClaimSources {
        forage: world.get_resource::<ForageRegistry>().unwrap_or(&no_forage),
        herds: world.get_resource::<HerdRegistry>().unwrap_or(&no_herds),
        deposits: world
            .get_resource::<DepositRegistry>()
            .unwrap_or(&no_deposits),
        ground_of: &ground_of,
        season_of: &season_of,
        map_seed: world
            .get_resource::<crate::SimulationConfig>()
            .map_or(crate::HARNESS_MAP_SEED, |config| config.map_seed),
        labor: &labor,
        flora: &flora,
        fauna: &fauna,
        equipment: &equipment,
        extraction: &extraction,
        ladder: &ladder,
        materials: &materials,
        combat: &combat,
        person,
        start: ProjectionStart::BeforeRegrowth,
    })
}

/// ⛔ **THE TAKE HANDS THAT WOULD TAKE SOMETHING WITH `kit` ON `target`** — the room the row's floor
/// leaves, over what one fully equipped hand takes of it, through each web's own function. A
/// property of the site and the band's gear, never of the crew, which is what keeps a claim still
/// while the player steps the crew.
///
/// A band-wide role answers every hand; a source no longer on the map answers none.
pub fn useful_take_hands(
    sources: &ClaimSources<'_>,
    band_kit: &BandEquipment,
    target: &LaborTarget,
    kit: &KitChoice,
) -> f32 {
    let equipment = sources.equipment;
    let hand = equipment.one_equipped_hand(kit, band_kit);
    match target {
        LaborTarget::Forage {
            tile,
            floor,
            take_species,
            ..
        } => {
            let Some(patch) = sources.patch(*tile) else {
                return NO_CLAIMING_HANDS;
            };
            let composition: Vec<crate::flora_config::FloraShare> = (sources.ground_of)(*tile)
                .map(|ground| {
                    crate::forage::tile_flora_composition(
                        sources.flora,
                        &sources.labor.forage,
                        &ground,
                        sources.map_seed,
                    )
                    .into_owned()
                })
                .unwrap_or_default();
            crate::forage::forage_useful_take_hands(
                &patch,
                &composition,
                sources.flora,
                &sources.labor.forage,
                take_species,
                *floor,
                hand.weighted_rate(|crew| {
                    equipment.forage_per_worker_biomass_capacity(
                        sources.labor.forage.per_worker_biomass_capacity,
                        crew,
                        band_kit,
                    )
                }),
                (sources.season_of)(*tile),
            )
        }
        LaborTarget::Hunt {
            fauna_id, floor, ..
        } => {
            let Some(herd) = sources.herd(fauna_id) else {
                return NO_CLAIMING_HANDS;
            };
            crate::fauna::hunt_useful_take_hands(
                &herd,
                sources.fauna,
                *floor,
                hand.weighted_rate(|crew| {
                    equipment.hunt_per_worker_biomass_capacity(
                        sources.labor.hunt.per_worker_biomass_capacity,
                        crew,
                        band_kit,
                    )
                }),
                &crate::fauna::PartyResolution {
                    equipment,
                    coverage: &hand,
                    wear: band_kit,
                    intrinsic: sources.person,
                    tuning: sources.combat.tuning(),
                    hunt_injury_damage_per_animal: sources.combat.hunt_injury_damage_per_animal,
                }
                .party_against(Quarry::Mass(herd.body_mass)),
            )
        }
        LaborTarget::Extract {
            tile,
            material,
            floor,
        } => {
            let Some((working, ground)) = sources.working(*tile, material) else {
                return NO_CLAIMING_HANDS;
            };
            crate::extraction::deposit_useful_take_hands(
                equipment,
                band_kit,
                kit,
                &working,
                &ground,
                sources.extraction,
                sources.ladder,
                crate::extraction::DepositCarry::of(sources.labor, sources.materials, material)
                    .as_ref(),
                *floor,
            )
        }
        _ => EVERY_HAND_USES_IT,
    }
}

/// **THE RUNG THE BAND'S QUEUE ENTRY ON `target` DECLARES** — what the keeping seams resolve the
/// verb against, `None` for no entry or a job that raises no rung.
pub fn declared_on(allocation: &LaborAllocation, target: &LaborTarget) -> Option<Improvement> {
    let source = BuildSource::of(target)?;
    match allocation.build_queue_entry(&source)?.declared {
        BuildJob::Rung(improvement) => Some(improvement),
        BuildJob::ExtendPen | BuildJob::SetHerdOutput(_) => None,
    }
}

/// **THE HANDS A CREW OF `crew` ON `target` WOULD SPEND KEEPING IT** — each web's prospective
/// keeping split (`docs/plan_site_crews.md` §2.1), the reading every quote strikes its take hands
/// on. None for a role or a source that keeps nothing.
pub fn planned_keep_hands(
    sources: &ClaimSources<'_>,
    band_kit: &BandEquipment,
    declared: Option<Improvement>,
    target: &LaborTarget,
    crew: u32,
) -> f32 {
    match target {
        LaborTarget::Forage { tile, .. } => {
            sources
                .patch(*tile)
                .map_or(crate::fauna::NO_HANDS, |patch| {
                    crate::fauna::crew_keep_hands(
                        crate::forage::patch_crew_keeping(
                            &patch,
                            sources.ladder,
                            &sources.labor.forage,
                            (sources.ground_of)(*tile).map(|ground| {
                                crate::forage::tile_forage_capacity(&sources.labor.forage, &ground)
                            }),
                            declared,
                        ),
                        sources.equipment,
                        band_kit,
                        crew,
                    )
                })
        }
        LaborTarget::Hunt { fauna_id, .. } => {
            sources
                .herd(fauna_id)
                .map_or(crate::fauna::NO_HANDS, |herd| {
                    crate::fauna::crew_keep_hands(
                        crate::fauna::herd_crew_keeping(
                            &herd,
                            sources.fauna,
                            sources.ladder,
                            declared,
                        ),
                        sources.equipment,
                        band_kit,
                        crew,
                    )
                })
        }
        LaborTarget::Extract { tile, material, .. } => {
            match (
                sources.deposits.source(*tile, material),
                (sources.ground_of)(*tile),
            ) {
                (Some(working), Some(ground)) => crate::extraction::crew_keep_hands(
                    sources.equipment,
                    band_kit,
                    working,
                    &ground,
                    sources.extraction,
                    sources.ladder,
                    crew,
                ),
                _ => crate::fauna::NO_HANDS,
            }
        }
        _ => crate::fauna::NO_HANDS,
    }
}

/// **THE TAKE HANDS OF A CREW OF `crew`** — the crew less its keeping, never below none.
pub fn take_hands(crew: f32, keep_hands: f32) -> f32 {
    (crew - keep_hands).max(NO_CLAIMING_HANDS)
}

/// **ONE ROW'S PLANNED SPLIT AT ONE CREW** — the hands its keeping takes and the hands that claim
/// its take kit ([`take_kit_claim`]).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CrewClaim {
    /// The planned keep hands ([`planned_keep_hands`]).
    pub keep_hands: f32,
    /// The take hands that would take something with the kit — never the head count.
    pub claim: f32,
}

impl CrewClaim {
    /// The crew's take hands — what its take kit is spread over.
    pub fn take_hands(&self, crew: u32) -> f32 {
        take_hands(crew as f32, self.keep_hands)
    }
}

/// ⛔ **THE PLANNED SPLIT OF A CREW OF `crew` ON `target` CARRYING `kit`** — its planned take hands,
/// and the claim: those take hands capped at the ones that would take something with the kit. The
/// one claim every committed and prospective row is settled on
/// ([`crate::equipment_config::BandItemBudget`]).
pub fn take_kit_claim(
    sources: &ClaimSources<'_>,
    band_kit: &BandEquipment,
    declared: Option<Improvement>,
    target: &LaborTarget,
    kit: &KitChoice,
    crew: u32,
) -> CrewClaim {
    let keep_hands = planned_keep_hands(sources, band_kit, declared, target, crew);
    CrewClaim {
        keep_hands,
        claim: take_hands(crew as f32, keep_hands)
            .min(useful_take_hands(sources, band_kit, target, kit)),
    }
}

/// **EVERY ROW'S PLANNED SPLIT**, index-aligned with `allocation.assignments`. A standing pool keeps
/// and claims nothing here: its tools are settled with the keeping.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RowClaims {
    /// What [`LaborAllocation::item_budget`] and [`LaborAllocation::rows_excluding_source`] settle on.
    pub claims: Vec<f32>,
    /// The planned keep hands each claim was struck less.
    pub keep_hands: Vec<f32>,
}

/// [`RowClaims`] for every row of `allocation`, at each row's own crew.
pub fn row_claims(
    sources: &ClaimSources<'_>,
    allocation: &LaborAllocation,
    band_kit: &BandEquipment,
) -> RowClaims {
    let mut rows = RowClaims::default();
    for assignment in &allocation.assignments {
        let split = if assignment.target.is_standing_pool() {
            CrewClaim::default()
        } else {
            take_kit_claim(
                sources,
                band_kit,
                declared_on(allocation, &assignment.target),
                &assignment.target,
                &assignment.kit_choice(sources.equipment),
                assignment.workers,
            )
        };
        rows.claims.push(split.claim);
        rows.keep_hands.push(split.keep_hands);
    }
    rows
}
