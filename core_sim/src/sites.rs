//! Wondrous Sites — the data-driven catalog of notable map features tiles can hold, hidden
//! under fog until a faction's vision reveals them, then recorded in a per-faction registry.
//!
//! Two halves:
//! - **Placement** (`place_wondrous_sites`, Startup after `spawn_initial_world`): for each
//!   catalog entry, run its `placement_rule` against the tiles and stamp a [`SiteTag`] on the
//!   chosen tile entities (capped, spaced, deterministic under the map seed).
//! - **Discovery** (`discover_sites`, `TurnStage::Visibility` after `calculate_visibility`):
//!   any vision source that has ever seen a site's tile records it into [`DiscoveredSites`],
//!   applies the config-driven reward once, and pushes a `SiteDiscovered` command-feed entry.
//!
//! Sites are **rare**, so discovery iterates the (few) `SiteTag` tiles × the visibility
//! ledger's factions rather than sweeping the whole map. Design: `docs/plan_exploration_and_sites.md` §3.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use rand::{rngs::SmallRng, seq::SliceRandom, SeedableRng};

use crate::{
    components::PopulationCohort,
    food::FoodModuleTag,
    mapgen::WorldGenSeed,
    orders::FactionId,
    resources::{
        CommandEventEntry, CommandEventKind, CommandEventLog, SimulationConfig, SimulationTick,
    },
    scalar::{scalar_from_f32, scalar_one, scalar_zero},
    sites_config::{PlacementRuleCfg, SitesConfigHandle},
    systems::{tile_morale_pressure, MoralePressureConfig},
    terrain::terrain_definition,
    turn_pipeline_config::TurnPipelineConfigHandle,
    visibility::VisibilityLedger,
    Tile,
};

/// RNG salt for deterministic site placement, kept distinct from the fauna spawn/immigration
/// salts so the two subsystems draw independent streams from the same map seed.
const SITE_PLACEMENT_SEED_SALT: u64 = 0x517E_5EED;

/// Marks a tile entity as holding a Wondrous Site. `site_id` keys into the sites catalog
/// (`SitesConfig::catalog`). One site per tile; hidden under fog until discovered.
#[derive(Component, Debug, Clone)]
pub struct SiteTag {
    pub site_id: String,
}

/// One discovered site in a faction's registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredSiteRecord {
    pub pos: UVec2,
    pub site_id: String,
}

/// Per-faction registry of discovered sites. Snapshot-persisted (a rollback must not
/// un-discover, nor leak discoveries made after the restore point — `restore_world_from_snapshot`
/// rebuilds it from the snapshot). A `seen` set backs an O(1) `contains(faction, pos)` check.
#[derive(Resource, Debug, Clone, Default)]
pub struct DiscoveredSites {
    by_faction: HashMap<FactionId, Vec<DiscoveredSiteRecord>>,
    seen: HashSet<(u32, u32, u32)>,
}

impl DiscoveredSites {
    /// Has `faction` already discovered the site at `pos`?
    pub fn contains(&self, faction: FactionId, pos: UVec2) -> bool {
        self.seen.contains(&(faction.0, pos.x, pos.y))
    }

    /// Record a newly discovered site. Returns `false` (no-op) if already recorded.
    pub fn record(&mut self, faction: FactionId, pos: UVec2, site_id: String) -> bool {
        if !self.seen.insert((faction.0, pos.x, pos.y)) {
            return false;
        }
        self.by_faction
            .entry(faction)
            .or_default()
            .push(DiscoveredSiteRecord { pos, site_id });
        true
    }

    /// Discovered-site records for one faction (unsorted; snapshot sorts).
    pub fn for_faction(&self, faction: FactionId) -> &[DiscoveredSiteRecord] {
        self.by_faction
            .get(&faction)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// `(faction, records)` pairs in a stable faction order (for snapshotting / iteration).
    pub fn iter_sorted(&self) -> Vec<(FactionId, &Vec<DiscoveredSiteRecord>)> {
        let mut out: Vec<_> = self.by_faction.iter().map(|(f, v)| (*f, v)).collect();
        out.sort_by_key(|(f, _)| f.0);
        out
    }

    /// Rebuild the registry from persisted `(faction, pos, site_id)` triples (rollback restore).
    pub fn rebuild_from<I>(&mut self, records: I)
    where
        I: IntoIterator<Item = (FactionId, UVec2, String)>,
    {
        self.by_faction.clear();
        self.seen.clear();
        for (faction, pos, site_id) in records {
            self.record(faction, pos, site_id);
        }
    }
}

/// Chebyshev tile distance (anti-cluster spacing check; mirrors the fauna spawner's helper).
fn chebyshev_distance(a: UVec2, b: UVec2) -> u32 {
    let dx = a.x.abs_diff(b.x);
    let dy = a.y.abs_diff(b.y);
    dx.max(dy)
}

/// Greedily place up to `rule.max_sites` sites from a pre-ordered `candidates` list, skipping any
/// tile within `rule.min_spacing` (Chebyshev) of an already-placed site. Returns the chosen tile
/// entities. The caller controls placement priority via the ordering of `candidates`.
fn place_from_ordered(
    candidates: &[(Entity, UVec2)],
    rule: &PlacementRuleCfg,
    occupied: &mut Vec<UVec2>,
) -> Vec<Entity> {
    let mut chosen = Vec::new();
    for (entity, pos) in candidates {
        if chosen.len() as u32 >= rule.max_sites {
            break;
        }
        if occupied
            .iter()
            .any(|p| chebyshev_distance(*p, *pos) < rule.min_spacing)
        {
            continue;
        }
        occupied.push(*pos);
        chosen.push(*entity);
    }
    chosen
}

/// Worldgen placement: stamp `SiteTag`s per catalog placement rule. Deterministic under the map
/// seed. Runs once at Startup (after `spawn_initial_world`); a tile already carrying a site is
/// skipped so no tile holds two sites.
#[allow(clippy::too_many_arguments)]
pub fn place_wondrous_sites(
    mut commands: Commands,
    config: Res<SimulationConfig>,
    pipeline_config: Res<TurnPipelineConfigHandle>,
    sites_config: Res<SitesConfigHandle>,
    world_seed: Option<Res<WorldGenSeed>>,
    tiles: Query<(Entity, &Tile, Option<&FoodModuleTag>)>,
    existing_sites: Query<&SiteTag>,
) {
    // Idempotent: never double-place (e.g. a re-run Startup or a pre-seeded world).
    if !existing_sites.is_empty() {
        return;
    }
    let sites = sites_config.get();
    let placeable = sites.placeable_sites();
    if placeable.is_empty() {
        return;
    }

    let seed = world_seed
        .map(|s| s.0)
        .unwrap_or(config.map_seed)
        .wrapping_add(SITE_PLACEMENT_SEED_SALT);
    let mut rng = SmallRng::seed_from_u64(seed);

    // Same place-based morale config the sim/snapshot use, so `fertile_settle` habitability
    // matches the tile's rated harshness.
    let population_cfg = pipeline_config.config().population();
    let morale_pressure_cfg = MoralePressureConfig {
        ambient_temperature: config.ambient_temperature,
        temperature_morale_penalty: config.temperature_morale_penalty,
        temperature_morale_tolerance: config.temperature_morale_tolerance,
        attrition_penalty_scale: population_cfg.attrition_penalty_scale(),
        hardness_penalty_scale: population_cfg.hardness_penalty_scale(),
    };

    // Snapshot the tile set once (Entity, pos, relief, habitability pressure, food weight).
    struct TileInfo {
        entity: Entity,
        pos: UVec2,
        relief: Option<f32>,
        habitability: f32,
        food_weight: Option<f32>,
    }
    let mut infos: Vec<TileInfo> = tiles
        .iter()
        .map(|(entity, tile, food)| {
            let habitability = tile_morale_pressure(
                &terrain_definition(tile.terrain),
                tile.temperature,
                &morale_pressure_cfg,
            )
            .total()
            .to_f32();
            TileInfo {
                entity,
                pos: tile.position,
                relief: tile.mountain.map(|m| m.relief),
                habitability,
                food_weight: food.map(|f| f.seasonal_weight),
            }
        })
        .collect();
    // Stable base order (position) so candidate ordering is deterministic before any sort/shuffle.
    infos.sort_by_key(|t| (t.pos.y, t.pos.x));

    // A tile can hold at most one site (across all rules) — tracked as we place.
    let mut occupied: Vec<UVec2> = Vec::new();

    for (site_id, def, rule) in placeable {
        let chosen = match def.placement_rule.as_str() {
            "prominent_mountain" => {
                let min_relief = rule.min_relief.unwrap_or(f32::INFINITY);
                let mut candidates: Vec<(Entity, UVec2, f32)> = infos
                    .iter()
                    .filter_map(|t| {
                        t.relief
                            .filter(|r| *r >= min_relief)
                            .map(|r| (t.entity, t.pos, r))
                    })
                    .collect();
                // Prominent = tallest first; ties broken by position for determinism.
                candidates.sort_by(|a, b| {
                    b.2.partial_cmp(&a.2)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then((a.1.y, a.1.x).cmp(&(b.1.y, b.1.x)))
                });
                let ordered: Vec<(Entity, UVec2)> =
                    candidates.into_iter().map(|(e, p, _)| (e, p)).collect();
                place_from_ordered(&ordered, rule, &mut occupied)
            }
            "fertile_settle" => {
                let max_hab = rule.max_habitability_pressure.unwrap_or(0.0);
                let min_food = rule.min_food_weight.unwrap_or(f32::INFINITY);
                let mut candidates: Vec<(Entity, UVec2)> = infos
                    .iter()
                    .filter(|t| {
                        t.habitability <= max_hab
                            && t.food_weight.map(|w| w >= min_food).unwrap_or(false)
                    })
                    .map(|t| (t.entity, t.pos))
                    .collect();
                // Shuffle so the cap + spacing spread settle-sites across the map, not by scan
                // order. Seeded → deterministic under the map seed.
                candidates.shuffle(&mut rng);
                place_from_ordered(&candidates, rule, &mut occupied)
            }
            // Unknown rule: no placement (a future rule adds a branch here + a JSON row).
            _ => Vec::new(),
        };

        for entity in chosen {
            commands.entity(entity).insert(SiteTag {
                site_id: site_id.clone(),
            });
        }
    }
}

/// Discovery: any vision source that has ever seen a site's tile records it for that faction,
/// applies the one-shot reward, and narrates it. Runs in `TurnStage::Visibility` after
/// `calculate_visibility` (so this turn's sight is current). Rare sites → cheap: iterate the
/// (few) `SiteTag` tiles × the ledger's factions.
pub fn discover_sites(
    mut discovered: ResMut<DiscoveredSites>,
    mut event_log: ResMut<CommandEventLog>,
    tick: Res<SimulationTick>,
    sites_config: Res<SitesConfigHandle>,
    visibility: Res<VisibilityLedger>,
    sites: Query<(&Tile, &SiteTag)>,
    mut cohorts: Query<&mut PopulationCohort>,
) {
    let cfg = sites_config.get();
    let factions: Vec<FactionId> = visibility.factions().collect();
    if factions.is_empty() {
        return;
    }

    // Collect newly-discovered (faction, pos, site_id) then process in a stable order so the
    // command feed + reward application are deterministic (query/ledger iteration is not).
    let mut newly: Vec<(FactionId, UVec2, String)> = Vec::new();
    for (tile, site) in sites.iter() {
        let pos = tile.position;
        for &faction in &factions {
            // "Ever seen" = Discovered or Active (state != Unexplored).
            if visibility.is_discovered(faction, pos.x, pos.y) && !discovered.contains(faction, pos)
            {
                newly.push((faction, pos, site.site_id.clone()));
            }
        }
    }
    if newly.is_empty() {
        return;
    }
    newly.sort_by(|a, b| (a.0 .0, a.1.y, a.1.x, &a.2).cmp(&(b.0 .0, b.1.y, b.1.x, &b.2)));

    for (faction, pos, site_id) in newly {
        if !discovered.record(faction, pos, site_id.clone()) {
            continue;
        }
        let Some(def) = cfg.site(&site_id) else {
            continue;
        };

        // Reward (v1): a one-shot morale bonus to each of the faction's bands (clamped 0..1).
        let bonus = scalar_from_f32(def.discovery_reward.morale_bonus);
        if bonus != scalar_zero() {
            for mut cohort in cohorts.iter_mut() {
                if cohort.faction == faction {
                    cohort.morale = (cohort.morale + bonus).clamp(scalar_zero(), scalar_one());
                }
            }
        }

        event_log.push(CommandEventEntry::new(
            tick.0,
            CommandEventKind::SiteDiscovered,
            faction,
            def.display_name.clone(),
            Some(format!(
                "category={} at ({},{})",
                def.category, pos.x, pos.y
            )),
        ));
    }
}
