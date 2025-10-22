use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin};
use ratatui::prelude::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};
use ratatui::Frame;

use sim_runtime::{
    influence_domains_from_mask, AxisBiasState, CorruptionEntry, CorruptionLedger,
    CorruptionSubsystem, CultureLayerScope, CultureLayerState, CultureTensionKind,
    CultureTensionState, CultureTraitAxis, GenerationState, InfluenceDomain, InfluenceLifecycle,
    InfluenceScopeKind, InfluentialIndividualState, PopulationCohortState, PowerNodeState,
    SentimentAxisTelemetry, SentimentDriverCategory, SentimentTelemetryState, TerrainOverlayState,
    TerrainTags, TerrainType, TileState, WorldDelta,
};

const HEATMAP_SIZE: usize = 21;
const SCALE_FACTOR: f32 = 1_000_000.0;
const BACKGROUND_COLOR: (u8, u8, u8) = (24, 24, 27);
const QUADRANT_COLORS: [(u8, u8, u8); 4] = [
    (59, 130, 246),
    (34, 197, 94),
    (245, 158, 11),
    (236, 72, 153),
];
const QUADRANT_LABELS: [&str; 4] = [
    "Complacent Stability",
    "Empowered Cohesion",
    "Volatile Despair",
    "Informed Resistance",
];
const AXIS_NAMES: [&str; 4] = ["Knowledge", "Trust", "Equity", "Agency"];
const AXIS_COLORS: [Color; 4] = [
    Color::Rgb(59, 130, 246),
    Color::Rgb(34, 197, 94),
    Color::Rgb(245, 158, 11),
    Color::Rgb(236, 72, 153),
];
const MAX_DRIVER_ENTRIES: usize = 4;
const WORKFORCE_LABELS: [&str; 5] = [
    "Industry",
    "Agriculture",
    "Logistics",
    "Innovation",
    "Unassigned",
];
const SENTIMENT_DELTA_LOG_THRESHOLD: f32 = 0.02;
pub const AXIS_BIAS_STEP: f32 = 0.05;
const CHANNEL_LABELS: [&str; 4] = ["Popular", "Peer", "Institutional", "Humanitarian"];
const CHANNEL_KEYS: [&str; 4] = ["popular", "peer", "institutional", "humanitarian"];
const MAX_CORRUPTION_EXPOSURES: usize = 6;
const MAX_CORRUPTION_ENTRIES: usize = 6;
const TERRAIN_TAG_LABELS: &[(TerrainTags, &str); 11] = &[
    (TerrainTags::WATER, "Water"),
    (TerrainTags::FRESHWATER, "Freshwater"),
    (TerrainTags::COASTAL, "Coastal"),
    (TerrainTags::WETLAND, "Wetland"),
    (TerrainTags::FERTILE, "Fertile"),
    (TerrainTags::ARID, "Arid"),
    (TerrainTags::POLAR, "Polar"),
    (TerrainTags::HIGHLAND, "Highland"),
    (TerrainTags::VOLCANIC, "Volcanic"),
    (TerrainTags::HAZARDOUS, "Hazardous"),
    (TerrainTags::SUBSURFACE, "Subsurface"),
];

fn scaled(value: i64) -> f32 {
    value as f32 / SCALE_FACTOR
}

fn axis_bias_state_to_f32(state: &AxisBiasState) -> [f32; 4] {
    [
        state.knowledge as f32 / SCALE_FACTOR,
        state.trust as f32 / SCALE_FACTOR,
        state.equity as f32 / SCALE_FACTOR,
        state.agency as f32 / SCALE_FACTOR,
    ]
}

fn build_heatmap_from_vector(vector: (f32, f32)) -> [[f32; HEATMAP_SIZE]; HEATMAP_SIZE] {
    let mut heatmap = [[0.0f32; HEATMAP_SIZE]; HEATMAP_SIZE];
    let center_x = ((vector.0.clamp(-1.0, 1.0) + 1.0) * 0.5 * (HEATMAP_SIZE as f32 - 1.0))
        .round()
        .clamp(0.0, HEATMAP_SIZE as f32 - 1.0) as isize;
    let center_y = ((1.0 - (vector.1.clamp(-1.0, 1.0) + 1.0) * 0.5) * (HEATMAP_SIZE as f32 - 1.0))
        .round()
        .clamp(0.0, HEATMAP_SIZE as f32 - 1.0) as isize;
    let offsets: [(isize, isize, f32); 9] = [
        (0, 0, 1.0_f32),
        (-1, 0, 0.6_f32),
        (1, 0, 0.6_f32),
        (0, -1, 0.6_f32),
        (0, 1, 0.6_f32),
        (-1, -1, 0.35_f32),
        (-1, 1, 0.35_f32),
        (1, -1, 0.35_f32),
        (1, 1, 0.35_f32),
    ];
    for (dx, dy, weight) in offsets {
        let nx = center_x + dx;
        let ny = center_y + dy;
        if nx >= 0 && nx < HEATMAP_SIZE as isize && ny >= 0 && ny < HEATMAP_SIZE as isize {
            let cell = &mut heatmap[ny as usize][nx as usize];
            *cell = cell.max(weight.clamp(0.0, 1.0));
        }
    }
    heatmap
}

fn driver_category_tag(category: SentimentDriverCategory) -> &'static str {
    match category {
        SentimentDriverCategory::Policy => "Policy",
        SentimentDriverCategory::Incident => "Incident",
        SentimentDriverCategory::Influencer => "Influencer",
    }
}

fn terrain_label(terrain: TerrainType) -> &'static str {
    match terrain {
        TerrainType::DeepOcean => "Deep Ocean",
        TerrainType::ContinentalShelf => "Continental Shelf",
        TerrainType::InlandSea => "Inland Sea",
        TerrainType::CoralShelf => "Coral Shelf",
        TerrainType::HydrothermalVentField => "Hydrothermal Vent Field",
        TerrainType::TidalFlat => "Tidal Flat",
        TerrainType::RiverDelta => "River Delta",
        TerrainType::MangroveSwamp => "Mangrove Swamp",
        TerrainType::FreshwaterMarsh => "Freshwater Marsh",
        TerrainType::Floodplain => "Floodplain",
        TerrainType::AlluvialPlain => "Alluvial Plain",
        TerrainType::PrairieSteppe => "Prairie Steppe",
        TerrainType::MixedWoodland => "Mixed Woodland",
        TerrainType::BorealTaiga => "Boreal Taiga",
        TerrainType::PeatHeath => "Peatland/Heath",
        TerrainType::HotDesertErg => "Hot Desert Erg",
        TerrainType::RockyReg => "Rocky Reg Desert",
        TerrainType::SemiAridScrub => "Semi-Arid Scrub",
        TerrainType::SaltFlat => "Salt Flat",
        TerrainType::OasisBasin => "Oasis Basin",
        TerrainType::Tundra => "Tundra",
        TerrainType::PeriglacialSteppe => "Periglacial Steppe",
        TerrainType::Glacier => "Glacier",
        TerrainType::SeasonalSnowfield => "Seasonal Snowfield",
        TerrainType::RollingHills => "Rolling Hills",
        TerrainType::HighPlateau => "High Plateau",
        TerrainType::AlpineMountain => "Alpine Mountain",
        TerrainType::KarstHighland => "Karst Highland",
        TerrainType::CanyonBadlands => "Canyon Badlands",
        TerrainType::ActiveVolcanoSlope => "Active Volcano Slope",
        TerrainType::BasalticLavaField => "Basaltic Lava Field",
        TerrainType::AshPlain => "Ash Plain",
        TerrainType::FumaroleBasin => "Fumarole Basin",
        TerrainType::ImpactCraterField => "Impact Crater Field",
        TerrainType::KarstCavernMouth => "Karst Cavern Mouth",
        TerrainType::SinkholeField => "Sinkhole Field",
        TerrainType::AquiferCeiling => "Aquifer Ceiling",
    }
}

fn terrain_color(terrain: TerrainType) -> Color {
    match terrain {
        TerrainType::DeepOcean => Color::Rgb(11, 30, 61),
        TerrainType::ContinentalShelf => Color::Rgb(20, 64, 94),
        TerrainType::InlandSea => Color::Rgb(28, 88, 114),
        TerrainType::CoralShelf => Color::Rgb(21, 122, 115),
        TerrainType::HydrothermalVentField => Color::Rgb(47, 127, 137),
        TerrainType::TidalFlat => Color::Rgb(184, 176, 138),
        TerrainType::RiverDelta => Color::Rgb(155, 195, 123),
        TerrainType::MangroveSwamp => Color::Rgb(79, 124, 56),
        TerrainType::FreshwaterMarsh => Color::Rgb(92, 140, 99),
        TerrainType::Floodplain => Color::Rgb(136, 182, 90),
        TerrainType::AlluvialPlain => Color::Rgb(201, 176, 120),
        TerrainType::PrairieSteppe => Color::Rgb(211, 165, 77),
        TerrainType::MixedWoodland => Color::Rgb(91, 127, 67),
        TerrainType::BorealTaiga => Color::Rgb(59, 79, 49),
        TerrainType::PeatHeath => Color::Rgb(100, 85, 106),
        TerrainType::HotDesertErg => Color::Rgb(231, 195, 106),
        TerrainType::RockyReg => Color::Rgb(138, 95, 60),
        TerrainType::SemiAridScrub => Color::Rgb(164, 135, 85),
        TerrainType::SaltFlat => Color::Rgb(224, 220, 210),
        TerrainType::OasisBasin => Color::Rgb(58, 162, 162),
        TerrainType::Tundra => Color::Rgb(166, 199, 207),
        TerrainType::PeriglacialSteppe => Color::Rgb(127, 183, 161),
        TerrainType::Glacier => Color::Rgb(209, 228, 236),
        TerrainType::SeasonalSnowfield => Color::Rgb(192, 202, 214),
        TerrainType::RollingHills => Color::Rgb(111, 155, 75),
        TerrainType::HighPlateau => Color::Rgb(150, 126, 92),
        TerrainType::AlpineMountain => Color::Rgb(122, 127, 136),
        TerrainType::KarstHighland => Color::Rgb(74, 106, 85),
        TerrainType::CanyonBadlands => Color::Rgb(182, 101, 68),
        TerrainType::ActiveVolcanoSlope => Color::Rgb(140, 52, 45),
        TerrainType::BasalticLavaField => Color::Rgb(64, 51, 61),
        TerrainType::AshPlain => Color::Rgb(122, 110, 104),
        TerrainType::FumaroleBasin => Color::Rgb(76, 137, 145),
        TerrainType::ImpactCraterField => Color::Rgb(91, 70, 57),
        TerrainType::KarstCavernMouth => Color::Rgb(46, 79, 92),
        TerrainType::SinkholeField => Color::Rgb(79, 75, 51),
        TerrainType::AquiferCeiling => Color::Rgb(47, 143, 178),
    }
}

#[derive(Clone, Copy, Default)]
struct SentimentAxes {
    knowledge: f32,
    trust: f32,
    equity: f32,
    agency: f32,
}

#[derive(Clone)]
struct AxisDriver {
    label: String,
    value: f32,
    weight: f32,
    category: SentimentDriverCategory,
}

impl AxisDriver {
    fn new<L: Into<String>>(
        label: L,
        value: f32,
        weight: f32,
        category: SentimentDriverCategory,
    ) -> Self {
        Self {
            label: label.into(),
            value,
            weight,
            category,
        }
    }

    fn impact(&self) -> f32 {
        (self.value * self.weight).abs()
    }
}

#[derive(Clone)]
struct SentimentViewModel {
    heatmap: [[f32; HEATMAP_SIZE]; HEATMAP_SIZE],
    axes: SentimentAxes,
    vector: (f32, f32),
    total_weight: f32,
    drivers: [Vec<AxisDriver>; 4],
}

impl Default for SentimentViewModel {
    fn default() -> Self {
        Self {
            heatmap: [[0.0; HEATMAP_SIZE]; HEATMAP_SIZE],
            axes: SentimentAxes::default(),
            vector: (0.0, 0.0),
            total_weight: 0.0,
            drivers: Default::default(),
        }
    }
}

impl SentimentViewModel {
    fn apply_telemetry(&mut self, telemetry: &SentimentTelemetryState) {
        let to_axis = |axis: &SentimentAxisTelemetry| -> (f32, Vec<AxisDriver>) {
            let value = (axis.total as f32) / SCALE_FACTOR;
            let drivers = axis
                .drivers
                .iter()
                .map(|driver| {
                    AxisDriver::new(
                        driver.label.clone(),
                        (driver.value as f32) / SCALE_FACTOR,
                        (driver.weight as f32) / SCALE_FACTOR,
                        driver.category,
                    )
                })
                .collect::<Vec<_>>();
            (value.clamp(-2.0, 2.0), drivers)
        };

        let (knowledge, knowledge_drivers) = to_axis(&telemetry.knowledge);
        let (trust, trust_drivers) = to_axis(&telemetry.trust);
        let (equity, equity_drivers) = to_axis(&telemetry.equity);
        let (agency, agency_drivers) = to_axis(&telemetry.agency);

        self.axes = SentimentAxes {
            knowledge,
            trust,
            equity,
            agency,
        };
        self.vector = (knowledge.clamp(-1.0, 1.0), trust.clamp(-1.0, 1.0));

        let total_weight = knowledge_drivers
            .iter()
            .chain(trust_drivers.iter())
            .chain(equity_drivers.iter())
            .chain(agency_drivers.iter())
            .map(|driver| driver.weight.abs())
            .sum::<f32>();
        self.total_weight = if total_weight > 0.0 {
            total_weight
        } else {
            1.0
        };

        self.heatmap = build_heatmap_from_vector(self.vector);
        self.drivers = [
            knowledge_drivers,
            trust_drivers,
            equity_drivers,
            agency_drivers,
        ];
        for axis in self.drivers.iter_mut() {
            axis.sort_by(|a, b| {
                b.impact()
                    .partial_cmp(&a.impact())
                    .unwrap_or(Ordering::Equal)
            });
            if axis.len() > MAX_DRIVER_ENTRIES {
                axis.truncate(MAX_DRIVER_ENTRIES);
            }
        }
    }

    fn rebuild(
        &mut self,
        tiles: &HashMap<u64, TileState>,
        populations: &HashMap<u64, PopulationCohortState>,
        power: &HashMap<u64, PowerNodeState>,
        generations: &HashMap<u16, GenerationState>,
        axis_bias: &[f32; 4],
        telemetry: Option<&SentimentTelemetryState>,
    ) {
        if let Some(sentiment) = telemetry {
            self.apply_telemetry(sentiment);
            return;
        }
        let mut heatmap = [[0.0f32; HEATMAP_SIZE]; HEATMAP_SIZE];
        let mut total_weight = 0.0f32;
        let mut sum_knowledge = 0.0f32;
        let mut sum_trust = 0.0f32;
        let mut sum_equity = 0.0f32;
        let mut sum_agency = 0.0f32;
        let mut drivers: [Vec<AxisDriver>; 4] = Default::default();
        let mut generation_totals: HashMap<u16, f32> = HashMap::new();

        if populations.is_empty() {
            self.heatmap = heatmap;
            self.axes = SentimentAxes::default();
            self.vector = (0.0, 0.0);
            self.total_weight = 0.0;
            self.drivers = drivers;
            return;
        }

        let avg_size = populations
            .values()
            .map(|cohort| cohort.size as f32)
            .sum::<f32>()
            / populations.len() as f32;
        let avg_size = avg_size.max(1.0);

        for cohort in populations.values() {
            let weight = cohort.size.max(1) as f32;
            let morale = (cohort.morale as f32) / SCALE_FACTOR;
            let mut trust = (morale * 2.0 - 1.0).clamp(-1.0, 1.0);
            let tile = tiles.get(&cohort.home);
            let mut knowledge = tile
                .map(|t| {
                    let temp = (t.temperature as f32) / SCALE_FACTOR;
                    let mass = (t.mass as f32) / SCALE_FACTOR;
                    let normalized_temp = (temp / 60.0).clamp(-1.5, 1.5);
                    let normalized_mass = ((mass - 1.0) / 1.0).clamp(-1.5, 1.5);
                    (normalized_temp * 0.6 + normalized_mass * 0.4).clamp(-1.0, 1.0)
                })
                .unwrap_or(0.0);

            let deviation = ((cohort.size as f32 - avg_size) / avg_size).clamp(-1.0, 1.0);
            let mut equity = ((1.0 - deviation.abs()) * 2.0 - 1.0).clamp(-1.0, 1.0);
            let cohort_weight = weight;
            let tile_label = tile
                .map(|t| format!("Tile ({:02},{:02})", t.x, t.y))
                .unwrap_or_else(|| format!("Cohort {}", cohort.entity));

            let mut agency = power
                .get(&cohort.home)
                .map(|node| {
                    let generation = (node.generation as f32) / SCALE_FACTOR;
                    let demand = (node.demand as f32) / SCALE_FACTOR;
                    let efficiency = (node.efficiency as f32) / SCALE_FACTOR;
                    let balance = generation - demand;
                    let scale = generation.abs() + demand.abs() + 1.0;
                    let net = (balance / scale + (efficiency - 1.0) * 0.25).clamp(-1.0, 1.0);
                    net
                })
                .unwrap_or(0.0);

            if let Some(state) = generations.get(&cohort.generation) {
                let biases = [
                    state.bias_knowledge as f32 / SCALE_FACTOR,
                    state.bias_trust as f32 / SCALE_FACTOR,
                    state.bias_equity as f32 / SCALE_FACTOR,
                    state.bias_agency as f32 / SCALE_FACTOR,
                ];
                knowledge = (knowledge + biases[0]).clamp(-1.0, 1.0);
                trust = (trust + biases[1]).clamp(-1.0, 1.0);
                equity = (equity + biases[2]).clamp(-1.0, 1.0);
                agency = (agency + biases[3]).clamp(-1.0, 1.0);
                generation_totals
                    .entry(cohort.generation)
                    .and_modify(|entry| *entry += weight)
                    .or_insert(weight);
            }

            knowledge = (knowledge + axis_bias[0]).clamp(-1.0, 1.0);
            trust = (trust + axis_bias[1]).clamp(-1.0, 1.0);
            equity = (equity + axis_bias[2]).clamp(-1.0, 1.0);
            agency = (agency + axis_bias[3]).clamp(-1.0, 1.0);

            let heat_x = ((knowledge + 1.0) * 0.5 * (HEATMAP_SIZE as f32 - 1.0))
                .round()
                .clamp(0.0, HEATMAP_SIZE as f32 - 1.0) as usize;
            let heat_y = ((1.0 - (trust + 1.0) * 0.5) * (HEATMAP_SIZE as f32 - 1.0))
                .round()
                .clamp(0.0, HEATMAP_SIZE as f32 - 1.0) as usize;

            heatmap[heat_y][heat_x] += weight;

            total_weight += weight;
            sum_knowledge += knowledge * weight;
            sum_trust += trust * weight;
            sum_equity += equity * weight;
            sum_agency += agency * weight;

            drivers[0].push(AxisDriver::new(
                tile_label.clone(),
                knowledge,
                cohort_weight,
                SentimentDriverCategory::Policy,
            ));
            drivers[1].push(AxisDriver::new(
                tile_label.clone(),
                trust,
                cohort_weight,
                SentimentDriverCategory::Policy,
            ));
            drivers[2].push(AxisDriver::new(
                tile_label.clone(),
                equity,
                cohort_weight,
                SentimentDriverCategory::Policy,
            ));
            drivers[3].push(AxisDriver::new(
                tile_label,
                agency,
                cohort_weight,
                SentimentDriverCategory::Policy,
            ));
        }

        for (generation_id, weight) in generation_totals.into_iter() {
            if let Some(state) = generations.get(&generation_id) {
                let biases = [
                    state.bias_knowledge as f32 / SCALE_FACTOR,
                    state.bias_trust as f32 / SCALE_FACTOR,
                    state.bias_equity as f32 / SCALE_FACTOR,
                    state.bias_agency as f32 / SCALE_FACTOR,
                ];
                let label_base = format!("Gen {}", state.name);
                for (axis_idx, bias) in biases.iter().enumerate() {
                    if bias.abs() < f32::EPSILON {
                        continue;
                    }
                    drivers[axis_idx].push(AxisDriver::new(
                        format!("{label_base} · {}", AXIS_NAMES[axis_idx]),
                        *bias,
                        weight,
                        SentimentDriverCategory::Policy,
                    ));
                }
            }
        }

        if total_weight > 0.0 {
            for (idx, bias_value) in axis_bias.iter().enumerate() {
                if bias_value.abs() < 0.0001 {
                    continue;
                }
                drivers[idx].push(AxisDriver::new(
                    "Manual Bias",
                    *bias_value,
                    total_weight,
                    SentimentDriverCategory::Policy,
                ));
            }
        }

        let mut max_cell = 0.0;
        for row in &heatmap {
            for value in row {
                if *value > max_cell {
                    max_cell = *value;
                }
            }
        }

        if max_cell > 0.0 {
            for row in heatmap.iter_mut() {
                for value in row.iter_mut() {
                    *value = (*value / max_cell).clamp(0.0, 1.0);
                }
            }
        }

        let knowledge_avg = if total_weight > 0.0 {
            (sum_knowledge / total_weight).clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let trust_avg = if total_weight > 0.0 {
            (sum_trust / total_weight).clamp(-1.0, 1.0)
        } else {
            0.0
        };

        self.heatmap = heatmap;
        self.axes = if total_weight > 0.0 {
            SentimentAxes {
                knowledge: knowledge_avg,
                trust: trust_avg,
                equity: (sum_equity / total_weight).clamp(-1.0, 1.0),
                agency: (sum_agency / total_weight).clamp(-1.0, 1.0),
            }
        } else {
            SentimentAxes::default()
        };
        self.vector = (knowledge_avg, trust_avg);
        self.total_weight = total_weight;
        for axis in drivers.iter_mut() {
            axis.sort_by(|a, b| {
                b.impact()
                    .partial_cmp(&a.impact())
                    .unwrap_or(Ordering::Equal)
            });
            if axis.len() > MAX_DRIVER_ENTRIES {
                axis.truncate(MAX_DRIVER_ENTRIES);
            }
        }
        self.drivers = drivers;
    }
}

#[derive(Clone)]
struct CorruptionEntryView {
    id: u64,
    subsystem: CorruptionSubsystem,
    intensity: f32,
    exposure_timer: u16,
    restitution_window: u16,
}

impl CorruptionEntryView {
    fn from_entry(entry: &CorruptionEntry) -> Self {
        Self {
            id: entry.incident_id,
            subsystem: entry.subsystem,
            intensity: scaled(entry.intensity),
            exposure_timer: entry.exposure_timer,
            restitution_window: entry.restitution_window,
        }
    }
}

#[derive(Clone, Default)]
struct TerrainSummary {
    width: u32,
    height: u32,
    total: usize,
    type_counts: Vec<(TerrainType, usize)>,
    tag_counts: Vec<(&'static str, usize)>,
}

impl TerrainSummary {
    fn build(tile_index: &HashMap<u64, TileState>) -> Self {
        if tile_index.is_empty() {
            return Self::default();
        }
        let mut width = 0u32;
        let mut height = 0u32;
        let mut type_counts: HashMap<TerrainType, usize> = HashMap::new();
        let mut tag_counts: HashMap<&'static str, usize> = HashMap::new();
        for tile in tile_index.values() {
            width = width.max(tile.x + 1);
            height = height.max(tile.y + 1);
            *type_counts.entry(tile.terrain).or_insert(0) += 1;
            for (mask, label) in TERRAIN_TAG_LABELS.iter() {
                if tile.terrain_tags.contains(*mask) {
                    *tag_counts.entry(*label).or_insert(0) += 1;
                }
            }
        }
        let mut type_counts_vec: Vec<(TerrainType, usize)> = type_counts.into_iter().collect();
        type_counts_vec.sort_by(|a, b| b.1.cmp(&a.1));
        if type_counts_vec.len() > 6 {
            type_counts_vec.truncate(6);
        }
        let mut tag_counts_vec: Vec<(&'static str, usize)> = tag_counts.into_iter().collect();
        tag_counts_vec.sort_by(|a, b| b.1.cmp(&a.1));
        if tag_counts_vec.len() > 6 {
            tag_counts_vec.truncate(6);
        }
        Self {
            width,
            height,
            total: tile_index.len(),
            type_counts: type_counts_vec,
            tag_counts: tag_counts_vec,
        }
    }

    fn from_overlay(overlay: &TerrainOverlayState) -> Self {
        if overlay.width == 0 || overlay.height == 0 || overlay.samples.is_empty() {
            return Self::default();
        }
        let mut type_counts: HashMap<TerrainType, usize> = HashMap::new();
        let mut tag_counts: HashMap<&'static str, usize> = HashMap::new();
        for sample in &overlay.samples {
            *type_counts.entry(sample.terrain).or_insert(0) += 1;
            for (mask, label) in TERRAIN_TAG_LABELS.iter() {
                if sample.tags.contains(*mask) {
                    *tag_counts.entry(*label).or_insert(0) += 1;
                }
            }
        }
        let mut type_counts_vec: Vec<(TerrainType, usize)> = type_counts.into_iter().collect();
        type_counts_vec.sort_by(|a, b| b.1.cmp(&a.1));
        if type_counts_vec.len() > 6 {
            type_counts_vec.truncate(6);
        }
        let mut tag_counts_vec: Vec<(&'static str, usize)> = tag_counts.into_iter().collect();
        tag_counts_vec.sort_by(|a, b| b.1.cmp(&a.1));
        if tag_counts_vec.len() > 6 {
            tag_counts_vec.truncate(6);
        }
        Self {
            width: overlay.width,
            height: overlay.height,
            total: overlay.samples.len(),
            type_counts: type_counts_vec,
            tag_counts: tag_counts_vec,
        }
    }
}

#[derive(Clone, Default)]
struct CorruptionSummary {
    active_incidents: usize,
    total_intensity: f32,
    reputation_modifier: f32,
    audit_capacity: u16,
}

#[derive(Clone)]
struct CorruptionExposureView {
    tick: u64,
    subsystem: CorruptionSubsystem,
    intensity: f32,
    incident_id: u64,
}

fn subsystem_label(subsystem: CorruptionSubsystem) -> &'static str {
    match subsystem {
        CorruptionSubsystem::Logistics => "Logistics",
        CorruptionSubsystem::Trade => "Trade",
        CorruptionSubsystem::Military => "Military",
        CorruptionSubsystem::Governance => "Governance",
    }
}

fn subsystem_command_key(subsystem: CorruptionSubsystem) -> &'static str {
    match subsystem {
        CorruptionSubsystem::Logistics => "logistics",
        CorruptionSubsystem::Trade => "trade",
        CorruptionSubsystem::Military => "military",
        CorruptionSubsystem::Governance => "governance",
    }
}

fn subsystem_color(subsystem: CorruptionSubsystem) -> Color {
    match subsystem {
        CorruptionSubsystem::Logistics => Color::Rgb(59, 130, 246),
        CorruptionSubsystem::Trade => Color::Rgb(245, 158, 11),
        CorruptionSubsystem::Military => Color::Rgb(239, 68, 68),
        CorruptionSubsystem::Governance => Color::Rgb(147, 197, 253),
    }
}

fn culture_scope_label(scope: CultureLayerScope) -> &'static str {
    match scope {
        CultureLayerScope::Global => "Global",
        CultureLayerScope::Regional => "Regional",
        CultureLayerScope::Local => "Local",
    }
}

fn culture_trait_label(axis: CultureTraitAxis) -> String {
    match axis {
        CultureTraitAxis::PassiveAggressive => "Passive ↔ Aggressive",
        CultureTraitAxis::OpenClosed => "Open ↔ Closed",
        CultureTraitAxis::CollectivistIndividualist => "Collectivist ↔ Individualist",
        CultureTraitAxis::TraditionalistRevisionist => "Traditionalist ↔ Revisionist",
        CultureTraitAxis::HierarchicalEgalitarian => "Hierarchical ↔ Egalitarian",
        CultureTraitAxis::SyncreticPurist => "Syncretic ↔ Purist",
        CultureTraitAxis::AsceticIndulgent => "Ascetic ↔ Indulgent",
        CultureTraitAxis::PragmaticIdealistic => "Pragmatic ↔ Idealistic",
        CultureTraitAxis::RationalistMystical => "Rationalist ↔ Mystical",
        CultureTraitAxis::ExpansionistInsular => "Expansionist ↔ Insular",
        CultureTraitAxis::AdaptiveStubborn => "Adaptive ↔ Stubborn",
        CultureTraitAxis::HonorBoundOpportunistic => "Honor-Bound ↔ Opportunistic",
        CultureTraitAxis::MeritOrientedLineageOriented => "Merit ↔ Lineage",
        CultureTraitAxis::SecularDevout => "Secular ↔ Devout",
        CultureTraitAxis::PluralisticMonocultural => "Pluralistic ↔ Monocultural",
    }
    .to_string()
}

fn culture_tension_label(kind: CultureTensionKind) -> &'static str {
    match kind {
        CultureTensionKind::DriftWarning => "Drift Warning",
        CultureTensionKind::AssimilationPush => "Assimilation Push",
        CultureTensionKind::SchismRisk => "Schism Risk",
    }
}

#[derive(Clone)]
struct GenerationStat {
    name: String,
    share: f32,
    avg_morale: f32,
    bias: [f32; 4],
}

#[derive(Clone)]
struct DemographicSnapshot {
    total_population: f32,
    avg_morale: f32,
    cohort_count: usize,
    generations: Vec<GenerationStat>,
    workforce_distribution: [f32; 5],
}

impl Default for DemographicSnapshot {
    fn default() -> Self {
        Self {
            total_population: 0.0,
            avg_morale: 0.0,
            cohort_count: 0,
            generations: Vec::new(),
            workforce_distribution: [0.0; 5],
        }
    }
}

impl DemographicSnapshot {
    fn rebuild(
        populations: &HashMap<u64, PopulationCohortState>,
        tiles: &HashMap<u64, TileState>,
        generations: &HashMap<u16, GenerationState>,
    ) -> Self {
        let mut snapshot = Self::default();
        if populations.is_empty() {
            return snapshot;
        }

        let mut total_population = 0.0f32;
        let mut morale_weighted = 0.0f32;
        let mut workforce_totals = [0.0f32; 5];
        let mut generation_totals: HashMap<u16, (f32, f32)> = HashMap::new();

        for cohort in populations.values() {
            let size = cohort.size.max(1) as f32;
            total_population += size;

            let morale = ((cohort.morale as f32) / SCALE_FACTOR).clamp(0.0, 1.0);
            morale_weighted += morale * size;

            generation_totals
                .entry(cohort.generation)
                .and_modify(|entry| {
                    entry.0 += size;
                    entry.1 += morale * size;
                })
                .or_insert((size, morale * size));

            let workforce_index = tiles
                .get(&cohort.home)
                .map(|tile| match tile.element {
                    0 => 0,
                    1 => 1,
                    2 => 2,
                    3 => 3,
                    _ => 4,
                })
                .unwrap_or(4);
            workforce_totals[workforce_index] += size;
        }

        if total_population > 0.0 {
            for value in workforce_totals.iter_mut() {
                *value = (*value / total_population) * 100.0;
            }
        }

        let mut generation_stats: Vec<GenerationStat> = generation_totals
            .into_iter()
            .filter_map(|(id, (population, morale_sum))| {
                if population <= 0.0 {
                    return None;
                }
                let share = (population / total_population) * 100.0;
                let avg_morale = (morale_sum / population).clamp(0.0, 1.0);
                let (name, bias) = generations
                    .get(&id)
                    .map(|state| {
                        (
                            state.name.clone(),
                            [
                                state.bias_knowledge as f32 / SCALE_FACTOR,
                                state.bias_trust as f32 / SCALE_FACTOR,
                                state.bias_equity as f32 / SCALE_FACTOR,
                                state.bias_agency as f32 / SCALE_FACTOR,
                            ],
                        )
                    })
                    .unwrap_or_else(|| {
                        (
                            format!("Generation {}", id),
                            [0.0f32, 0.0f32, 0.0f32, 0.0f32],
                        )
                    });

                Some(GenerationStat {
                    name,
                    share,
                    avg_morale,
                    bias,
                })
            })
            .collect();

        generation_stats.sort_by(|a, b| b.share.partial_cmp(&a.share).unwrap_or(Ordering::Equal));

        snapshot.total_population = total_population;
        snapshot.avg_morale = if total_population > 0.0 {
            morale_weighted / total_population
        } else {
            0.0
        };
        snapshot.cohort_count = populations.len();
        snapshot.generations = generation_stats;
        snapshot.workforce_distribution = workforce_totals;
        snapshot
    }
}

#[derive(Clone)]
struct CultureLayerOverview {
    id: u32,
    scope: CultureLayerScope,
    magnitude: f32,
}

#[derive(Clone)]
struct CultureTensionView {
    layer_id: u32,
    scope: CultureLayerScope,
    kind: CultureTensionKind,
    severity: f32,
    timer: u16,
}

#[derive(Clone, Default)]
struct CultureSummary {
    global_traits: Vec<(String, f32)>,
    divergences: Vec<CultureLayerOverview>,
    tensions: Vec<CultureTensionView>,
}

impl CultureSummary {
    fn rebuild(layers: &HashMap<u32, CultureLayerState>, tensions: &[CultureTensionState]) -> Self {
        let mut summary = CultureSummary::default();
        if layers.is_empty() {
            return summary;
        }

        if let Some(global) = layers
            .values()
            .find(|layer| matches!(layer.scope, CultureLayerScope::Global))
        {
            let mut traits: Vec<(String, f32)> = global
                .traits
                .iter()
                .map(|entry| (culture_trait_label(entry.axis), scaled(entry.value)))
                .collect();
            traits.sort_by(|a, b| b.1.abs().partial_cmp(&a.1.abs()).unwrap_or(Ordering::Equal));
            traits.truncate(6);
            summary.global_traits = traits;
        }

        let mut divergences: Vec<CultureLayerOverview> = layers
            .values()
            .filter(|layer| !matches!(layer.scope, CultureLayerScope::Global))
            .map(|layer| CultureLayerOverview {
                id: layer.id,
                scope: layer.scope,
                magnitude: scaled(layer.divergence),
            })
            .filter(|overview| overview.magnitude.abs() > 0.01)
            .collect();
        divergences.sort_by(|a, b| {
            b.magnitude
                .partial_cmp(&a.magnitude)
                .unwrap_or(Ordering::Equal)
        });
        divergences.truncate(5);
        summary.divergences = divergences;

        let mut tension_views: Vec<CultureTensionView> = tensions
            .iter()
            .map(|state| CultureTensionView {
                layer_id: state.layer_id,
                scope: state.scope,
                kind: state.kind,
                severity: scaled(state.severity),
                timer: state.timer,
            })
            .collect();
        tension_views.sort_by(|a, b| {
            b.severity
                .partial_cmp(&a.severity)
                .unwrap_or(Ordering::Equal)
                .then_with(|| b.timer.cmp(&a.timer))
        });
        summary.tensions = tension_views;

        summary
    }
}

pub struct UiState {
    pub recent_ticks: VecDeque<WorldDelta>,
    pub max_history: usize,
    pub logs: VecDeque<String>,
    pub max_logs: usize,
    tile_index: HashMap<u64, TileState>,
    population_index: HashMap<u64, PopulationCohortState>,
    power_index: HashMap<u64, PowerNodeState>,
    generation_index: HashMap<u16, GenerationState>,
    influencer_index: HashMap<u32, InfluentialIndividualState>,
    influencer_order: Vec<u32>,
    influencer_filter: Option<InfluenceLifecycle>,
    sentiment: SentimentViewModel,
    demographics: DemographicSnapshot,
    axis_bias: [f32; 4],
    sentiment_telemetry: Option<SentimentTelemetryState>,
    selected_axis: Option<usize>,
    selected_influencer: Option<u32>,
    corruption_index: HashMap<u64, CorruptionEntry>,
    corruption_entries: Vec<CorruptionEntryView>,
    corruption_summary: CorruptionSummary,
    corruption_exposures: VecDeque<CorruptionExposureView>,
    corruption_target: CorruptionSubsystem,
    culture_layers: HashMap<u32, CultureLayerState>,
    culture_tensions: Vec<CultureTensionState>,
    culture_summary: CultureSummary,
    terrain_overlay: Option<TerrainOverlayState>,
    terrain_summary: TerrainSummary,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            recent_ticks: VecDeque::new(),
            max_history: 32,
            logs: VecDeque::new(),
            max_logs: 8,
            tile_index: HashMap::new(),
            population_index: HashMap::new(),
            power_index: HashMap::new(),
            generation_index: HashMap::new(),
            influencer_index: HashMap::new(),
            influencer_order: Vec::new(),
            influencer_filter: None,
            sentiment: SentimentViewModel::default(),
            demographics: DemographicSnapshot::default(),
            axis_bias: [0.0; 4],
            sentiment_telemetry: None,
            selected_axis: None,
            selected_influencer: None,
            corruption_index: HashMap::new(),
            corruption_entries: Vec::new(),
            corruption_summary: CorruptionSummary::default(),
            corruption_exposures: VecDeque::new(),
            corruption_target: CorruptionSubsystem::Logistics,
            culture_layers: HashMap::new(),
            culture_tensions: Vec::new(),
            culture_summary: CultureSummary::default(),
            terrain_overlay: None,
            terrain_summary: TerrainSummary::default(),
        }
    }
}

impl UiState {
    pub fn push_delta(&mut self, delta: WorldDelta) {
        self.apply_delta(&delta);
        self.recent_ticks.push_front(delta);
        while self.recent_ticks.len() > self.max_history {
            self.recent_ticks.pop_back();
        }
    }

    pub fn push_log<S: Into<String>>(&mut self, line: S) {
        let mut text: String = line.into();
        while text.ends_with('\n') || text.ends_with('\r') {
            text.pop();
        }
        if text.is_empty() {
            return;
        }
        self.logs.push_front(text);
        while self.logs.len() > self.max_logs {
            self.logs.pop_back();
        }
    }

    pub fn latest_tile_entity(&self) -> Option<u64> {
        self.recent_ticks
            .front()
            .and_then(|delta| delta.tiles.first().map(|tile| tile.entity))
    }

    pub fn latest_tick(&self) -> Option<u64> {
        self.recent_ticks.front().map(|delta| delta.header.tick)
    }

    fn apply_delta(&mut self, delta: &WorldDelta) {
        for tile in &delta.tiles {
            self.tile_index.insert(tile.entity, tile.clone());
        }
        for id in &delta.removed_tiles {
            self.tile_index.remove(id);
        }

        for cohort in &delta.populations {
            self.population_index.insert(cohort.entity, cohort.clone());
        }
        for id in &delta.removed_populations {
            self.population_index.remove(id);
        }

        for node in &delta.power {
            self.power_index.insert(node.entity, node.clone());
        }
        for id in &delta.removed_power {
            self.power_index.remove(id);
        }

        for generation in &delta.generations {
            self.generation_index
                .insert(generation.id, generation.clone());
        }
        for id in &delta.removed_generations {
            self.generation_index.remove(id);
        }

        for influencer in &delta.influencers {
            self.influencer_index
                .insert(influencer.id, influencer.clone());
        }
        for id in &delta.removed_influencers {
            self.influencer_index.remove(id);
        }

        for layer in &delta.culture_layers {
            self.culture_layers.insert(layer.id, layer.clone());
        }
        for id in &delta.removed_culture_layers {
            self.culture_layers.remove(id);
        }
        if !delta.culture_tensions.is_empty() {
            self.culture_tensions = delta.culture_tensions.clone();
            for tension in &delta.culture_tensions {
                self.push_log(format!(
                    "Culture tension: {} layer #{:03} · severity {:+.2} (timer {}t)",
                    culture_tension_label(tension.kind),
                    tension.layer_id,
                    scaled(tension.severity),
                    tension.timer
                ));
            }
        }

        if let Some(bias) = delta.axis_bias.as_ref() {
            self.axis_bias = axis_bias_state_to_f32(bias);
        }

        if let Some(sentiment) = delta.sentiment.as_ref() {
            self.sentiment_telemetry = Some(sentiment.clone());
        }

        if let Some(ledger) = delta.corruption.as_ref() {
            self.update_corruption(delta.header.tick, ledger);
        }

        if let Some(terrain) = delta.terrain.as_ref() {
            self.terrain_overlay = Some(terrain.clone());
            self.terrain_summary = TerrainSummary::from_overlay(terrain);
        }

        self.rebuild_influencers();
        self.rebuild_views();
    }

    pub fn cycle_influencer_filter(&mut self) {
        self.influencer_filter = match self.influencer_filter {
            None => Some(InfluenceLifecycle::Active),
            Some(InfluenceLifecycle::Active) => Some(InfluenceLifecycle::Potential),
            Some(InfluenceLifecycle::Potential) => Some(InfluenceLifecycle::Dormant),
            Some(InfluenceLifecycle::Dormant) => None,
        };
        let status = match self.influencer_filter {
            None => "all",
            Some(InfluenceLifecycle::Active) => "active",
            Some(InfluenceLifecycle::Potential) => "potential",
            Some(InfluenceLifecycle::Dormant) => "dormant",
        };
        self.push_log(format!("Influencer filter set to {}", status));
        self.rebuild_influencers();
        if self.influencer_order.is_empty() {
            self.push_log("No influencers match current filter");
        }
    }

    pub fn dominant_channel_key(&self) -> Option<&'static str> {
        let state = self.selected_influencer()?;
        let weights = [
            scaled(state.weight_popular).clamp(0.0, 1.0),
            scaled(state.weight_peer).clamp(0.0, 1.0),
            scaled(state.weight_institutional).clamp(0.0, 1.0),
            scaled(state.weight_humanitarian).clamp(0.0, 1.0),
        ];
        let support = [
            scaled(state.support_popular).clamp(0.0, 1.0),
            scaled(state.support_peer).clamp(0.0, 1.0),
            scaled(state.support_institutional).clamp(0.0, 1.0),
            scaled(state.support_humanitarian).clamp(0.0, 1.0),
        ];
        let mut best_idx = None;
        let mut best_score = f32::MIN;
        for idx in 0..CHANNEL_LABELS.len() {
            let score = (weights[idx] * support[idx]).max(0.0);
            if score > best_score {
                best_score = score;
                best_idx = Some(idx);
            }
        }
        match best_idx {
            Some(idx) if best_score > f32::EPSILON => Some(CHANNEL_KEYS[idx]),
            _ => None,
        }
    }

    fn rebuild_influencers(&mut self) {
        let mut ids: Vec<u32> = self.influencer_index.keys().copied().collect();
        if let Some(filter) = self.influencer_filter {
            ids.retain(|id| {
                self.influencer_index
                    .get(id)
                    .map(|state| state.lifecycle == filter)
                    .unwrap_or(false)
            });
        }
        ids.sort_by(|a, b| {
            let a_state = self.influencer_index.get(a);
            let b_state = self.influencer_index.get(b);
            let a_priority = a_state
                .map(|state| lifecycle_priority(state.lifecycle))
                .unwrap_or(u8::MAX);
            let b_priority = b_state
                .map(|state| lifecycle_priority(state.lifecycle))
                .unwrap_or(u8::MAX);
            a_priority
                .cmp(&b_priority)
                .then_with(|| {
                    let a_coherence = a_state.map(|s| scaled(s.coherence)).unwrap_or(0.0);
                    let b_coherence = b_state.map(|s| scaled(s.coherence)).unwrap_or(0.0);
                    b_coherence
                        .partial_cmp(&a_coherence)
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| {
                    let a_influence = a_state.map(|s| scaled(s.influence).abs()).unwrap_or(0.0);
                    let b_influence = b_state.map(|s| scaled(s.influence).abs()).unwrap_or(0.0);
                    b_influence
                        .partial_cmp(&a_influence)
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| a.cmp(b))
        });
        self.influencer_order = ids;
        if let Some(selected) = self.selected_influencer {
            if !self.influencer_index.contains_key(&selected) {
                self.selected_influencer = self.influencer_order.first().copied();
            }
        } else {
            self.selected_influencer = self.influencer_order.first().copied();
        }
    }

    pub fn selected_influencer_id(&self) -> Option<u32> {
        self.selected_influencer
    }

    pub fn selected_influencer(&self) -> Option<&InfluentialIndividualState> {
        self.selected_influencer
            .and_then(|id| self.influencer_index.get(&id))
    }

    pub fn select_next_influencer(&mut self) {
        if self.influencer_order.is_empty() {
            self.push_log("No influencers tracked yet");
            return;
        }
        let current_index = self
            .selected_influencer
            .and_then(|id| {
                self.influencer_order
                    .iter()
                    .position(|candidate| *candidate == id)
            })
            .unwrap_or(usize::MAX);
        let next_index = if current_index == usize::MAX {
            0
        } else {
            (current_index + 1) % self.influencer_order.len()
        };
        self.selected_influencer = Some(self.influencer_order[next_index]);
        if let Some(influencer) = self.selected_influencer() {
            self.push_log(format!(
                "Focused influencer: {} ({}) [{}]",
                influencer.name,
                scope_label(influencer.scope),
                lifecycle_label(influencer.lifecycle)
            ));
        }
    }

    pub fn select_previous_influencer(&mut self) {
        if self.influencer_order.is_empty() {
            self.push_log("No influencers tracked yet");
            return;
        }
        let current_index = self
            .selected_influencer
            .and_then(|id| {
                self.influencer_order
                    .iter()
                    .position(|candidate| *candidate == id)
            })
            .unwrap_or(usize::MAX);
        let prev_index = if current_index == usize::MAX {
            self.influencer_order.len().saturating_sub(1)
        } else if current_index == 0 {
            self.influencer_order.len().saturating_sub(1)
        } else {
            current_index - 1
        };
        self.selected_influencer = self.influencer_order.get(prev_index).copied();
        if let Some(influencer) = self.selected_influencer() {
            self.push_log(format!(
                "Focused influencer: {} ({}) [{}]",
                influencer.name,
                scope_label(influencer.scope),
                lifecycle_label(influencer.lifecycle)
            ));
        }
    }

    fn log_sentiment_shift(&mut self, prev_axes: SentimentAxes, prev_weight: f32) {
        if self.sentiment.total_weight <= 0.0 || prev_weight <= 0.0 {
            return;
        }

        let axes = self.sentiment.axes;
        let deltas = [
            (0, axes.knowledge - prev_axes.knowledge),
            (1, axes.trust - prev_axes.trust),
            (2, axes.equity - prev_axes.equity),
            (3, axes.agency - prev_axes.agency),
        ];
        let (axis_idx, delta) = deltas
            .iter()
            .copied()
            .max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap_or(Ordering::Equal))
            .unwrap_or((0, 0.0));

        if delta.abs() < SENTIMENT_DELTA_LOG_THRESHOLD {
            return;
        }

        let axis_name = AXIS_NAMES[axis_idx];
        let driver_text = self.sentiment.drivers[axis_idx]
            .first()
            .map(|driver| format!("{:+.2} · {}", driver.value, driver.label))
            .unwrap_or_else(|| "no driver data".to_string());
        let generation_text = self
            .demographics
            .generations
            .first()
            .map(|gen| format!("{} ({:>4.1}% share)", gen.name, gen.share))
            .unwrap_or_else(|| "no generation data".to_string());

        self.push_log(format!(
            "Δ Sentiment {axis_name}: {:+.2} | driver {driver_text} | lead gen {generation_text}",
            delta
        ));
    }

    fn rebuild_views(&mut self) {
        let prev_axes = self.sentiment.axes;
        let prev_weight = self.sentiment.total_weight;
        self.sentiment.rebuild(
            &self.tile_index,
            &self.population_index,
            &self.power_index,
            &self.generation_index,
            &self.axis_bias,
            self.sentiment_telemetry.as_ref(),
        );
        self.demographics = DemographicSnapshot::rebuild(
            &self.population_index,
            &self.tile_index,
            &self.generation_index,
        );
        if let Some(ref overlay) = self.terrain_overlay {
            self.terrain_summary = TerrainSummary::from_overlay(overlay);
        } else {
            self.terrain_summary = TerrainSummary::build(&self.tile_index);
        }
        self.culture_summary =
            CultureSummary::rebuild(&self.culture_layers, &self.culture_tensions);
        self.log_sentiment_shift(prev_axes, prev_weight);
    }

    pub fn select_axis(&mut self, axis: usize) {
        if axis < AXIS_NAMES.len() {
            self.selected_axis = Some(axis);
            self.push_log(format!(
                "Axis '{}' selected for bias editing",
                AXIS_NAMES[axis]
            ));
        }
    }

    pub fn adjust_selected_axis(&mut self, delta: f32) -> Option<(usize, f32)> {
        let Some(axis) = self.selected_axis else {
            self.push_log("Select axis (1-4) before adjusting bias");
            return None;
        };
        self.adjust_axis_bias(axis, delta)
            .map(|value| (axis, value))
    }

    pub fn adjust_axis_bias(&mut self, axis: usize, delta: f32) -> Option<f32> {
        if axis >= self.axis_bias.len() {
            return None;
        }
        let current = self.axis_bias[axis];
        let new_value = (current + delta).clamp(-1.0, 1.0);
        if (new_value - current).abs() < f32::EPSILON {
            return None;
        }
        self.axis_bias[axis] = new_value;
        self.rebuild_views();
        self.push_log(format!(
            "Axis '{}' bias set to {:+.2}",
            AXIS_NAMES[axis], new_value
        ));
        Some(new_value)
    }

    pub fn reset_axis_bias(&mut self) -> Option<Vec<(usize, f32)>> {
        let changed_axes: Vec<usize> = self
            .axis_bias
            .iter()
            .enumerate()
            .filter_map(|(idx, value)| {
                if value.abs() > f32::EPSILON {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect();
        if changed_axes.is_empty() {
            return None;
        }
        self.axis_bias = [0.0; 4];
        self.rebuild_views();
        self.push_log("Axis biases reset to neutral");
        Some(changed_axes.into_iter().map(|idx| (idx, 0.0)).collect())
    }

    pub fn cycle_corruption_target(&mut self) {
        self.corruption_target = match self.corruption_target {
            CorruptionSubsystem::Logistics => CorruptionSubsystem::Trade,
            CorruptionSubsystem::Trade => CorruptionSubsystem::Military,
            CorruptionSubsystem::Military => CorruptionSubsystem::Governance,
            CorruptionSubsystem::Governance => CorruptionSubsystem::Logistics,
        };
        self.push_log(format!(
            "Corruption target set to {}",
            self.corruption_target_label()
        ));
    }

    pub fn corruption_target_label(&self) -> &'static str {
        subsystem_label(self.corruption_target)
    }

    pub fn corruption_target_command_key(&self) -> &'static str {
        subsystem_command_key(self.corruption_target)
    }

    fn update_corruption(&mut self, tick: u64, ledger: &CorruptionLedger) {
        let mut new_index: HashMap<u64, CorruptionEntry> =
            HashMap::with_capacity(ledger.entries.len());
        for entry in &ledger.entries {
            new_index.insert(entry.incident_id, entry.clone());
        }

        let mut exposures: Vec<CorruptionExposureView> = Vec::new();
        for (incident_id, previous) in &self.corruption_index {
            if !new_index.contains_key(incident_id) {
                exposures.push(CorruptionExposureView {
                    tick,
                    subsystem: previous.subsystem,
                    intensity: scaled(previous.intensity),
                    incident_id: *incident_id,
                });
            }
        }

        if !exposures.is_empty() {
            for exposure in exposures.into_iter() {
                if self.corruption_exposures.len() >= MAX_CORRUPTION_EXPOSURES {
                    self.corruption_exposures.pop_back();
                }
                let label = subsystem_label(exposure.subsystem);
                self.push_log(format!(
                    "Corruption exposure in {} (id {}): {:+.2}",
                    label, exposure.incident_id, exposure.intensity
                ));
                self.corruption_exposures.push_front(exposure);
            }
        }

        self.corruption_index = new_index;
        self.corruption_summary = CorruptionSummary {
            active_incidents: ledger.entries.len(),
            total_intensity: ledger
                .entries
                .iter()
                .map(|entry| scaled(entry.intensity).abs())
                .sum::<f32>(),
            reputation_modifier: scaled(ledger.reputation_modifier),
            audit_capacity: ledger.audit_capacity,
        };

        let mut views: Vec<CorruptionEntryView> = ledger
            .entries
            .iter()
            .map(CorruptionEntryView::from_entry)
            .collect();
        views.sort_by(|a, b| {
            b.intensity
                .abs()
                .partial_cmp(&a.intensity.abs())
                .unwrap_or(Ordering::Equal)
        });
        self.corruption_entries = views;
    }
}

pub fn draw_ui(frame: &mut Frame, state: &UiState) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(14),
            Constraint::Min(10),
        ])
        .split(frame.size());

    draw_header(frame, layout[0]);
    draw_commands(frame, layout[1]);

    let main_area = layout[2];
    let main_split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(main_area);

    draw_sentiment(frame, main_split[0], state);

    let sidebar = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(12),
            Constraint::Length(9),
            Constraint::Min(8),
            Constraint::Length(7),
        ])
        .split(main_split[1]);

    draw_logs(frame, sidebar[0], state);
    draw_terrain(frame, sidebar[1], state);
    draw_corruption(frame, sidebar[2], state);
    draw_influencers(frame, sidebar[3], state);
    draw_recent_ticks(frame, sidebar[4], state);
}

fn draw_header(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Shadow-Scale CLI Inspector");
    let line = Line::from(vec![
        Span::styled("Connected", Style::default().fg(Color::Green)),
        Span::raw(" | Ctrl+C or q to exit"),
    ]);
    let text = Paragraph::new(line).wrap(Wrap { trim: true });
    frame.render_widget(block, area);
    frame.render_widget(
        text,
        area.inner(&Margin {
            vertical: 1,
            horizontal: 1,
        }),
    );
}

fn draw_commands(frame: &mut Frame, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title("Hotkeys");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height < 4 {
        return;
    }

    const COMMANDS: [(&str, &str); 22] = [
        ("space", "submit orders (faction 0)"),
        ("t", "+10 turns"),
        (".", "step 1 turn"),
        ("p", "toggle auto-play"),
        ("[ / ]", "slower / faster auto-play"),
        ("b", "rollback to previous tick"),
        ("h", "heat latest tile"),
        ("1-4", "select sentiment axis"),
        ("+ / =", "raise selected axis bias"),
        ("- / _", "lower selected axis bias"),
        ("0", "reset all axis biases"),
        ("j / k", "cycle influencers"),
        ("s", "support focused influencer"),
        ("x", "suppress focused influencer"),
        ("c", "channel boost (dominant)"),
        ("f", "cycle lifecycle filter"),
        ("i", "spawn random potential influencer"),
        ("v", "cycle corruption target"),
        ("g", "inject corruption incident (debug)"),
        (
            "spawn_influencer",
            "CLI: force-create influencer [scope] [gen]",
        ),
        ("q", "exit inspector"),
        ("turn", "type in CLI to queue N turns"),
    ];

    let rows_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(2)])
        .split(inner);

    let column_count = ((COMMANDS.len() + 9) / 10).max(1);
    let mut columns_lines: Vec<Vec<Line>> = Vec::with_capacity(column_count);
    for chunk in COMMANDS.chunks(10) {
        let mut lines: Vec<Line> = Vec::new();
        for (key, desc) in chunk {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:<8}", key),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(*desc),
            ]));
        }
        columns_lines.push(lines);
    }

    let constraints: Vec<Constraint> = (0..columns_lines.len())
        .map(|_| Constraint::Ratio(1, columns_lines.len() as u32))
        .collect();
    let column_areas = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(rows_layout[0]);

    for (idx, lines) in columns_lines.into_iter().enumerate() {
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
        frame.render_widget(paragraph, column_areas[idx]);
    }

    let note = Paragraph::new(Line::from(vec![
        Span::styled("Tip:", Style::default().fg(Color::LightCyan)),
        Span::raw(" influencers emerge automatically as turns resolve (roughly every 8–18 ticks)."),
    ]))
    .wrap(Wrap { trim: true });
    frame.render_widget(note, rows_layout[1]);
}
fn draw_influencers(frame: &mut Frame, area: Rect, state: &UiState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Influential Individuals");
    if area.height <= 3 {
        frame.render_widget(block, area);
        return;
    }

    let selected_id = state.selected_influencer_id();
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(
            "Legend",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(":  "),
        Span::styled("★ Active", Style::default().fg(Color::LightGreen)),
        Span::raw("  "),
        Span::styled("△ Potential", Style::default().fg(Color::Yellow)),
        Span::raw("  "),
        Span::styled("◼ Dormant", Style::default().fg(Color::DarkGray)),
    ]));
    let filter_label = state
        .influencer_filter
        .map(lifecycle_label)
        .unwrap_or("All");
    lines.push(Line::from(vec![
        Span::styled("Filter", Style::default().fg(Color::LightBlue)),
        Span::raw(": "),
        Span::raw(filter_label),
        Span::raw("  (press 'f' to cycle)"),
    ]));
    lines.push(Line::from(""));

    if let Some(selected) = state.selected_influencer() {
        let coherence_pct = (scaled(selected.coherence) * 100.0).clamp(0.0, 100.0);
        let badge = lifecycle_badge(selected.lifecycle);
        let status_label = lifecycle_label(selected.lifecycle);
        let audience_names: Vec<String> = selected
            .audience_generations
            .iter()
            .filter_map(|gen| {
                state
                    .generation_index
                    .get(gen)
                    .map(|profile| profile.name.clone())
            })
            .collect();
        let channel_weights = [
            scaled(selected.weight_popular).clamp(0.0, 1.0),
            scaled(selected.weight_peer).clamp(0.0, 1.0),
            scaled(selected.weight_institutional).clamp(0.0, 1.0),
            scaled(selected.weight_humanitarian).clamp(0.0, 1.0),
        ];
        let channel_support = [
            scaled(selected.support_popular).clamp(0.0, 1.0),
            scaled(selected.support_peer).clamp(0.0, 1.0),
            scaled(selected.support_institutional).clamp(0.0, 1.0),
            scaled(selected.support_humanitarian).clamp(0.0, 1.0),
        ];
        let top_idx = channel_weights
            .iter()
            .enumerate()
            .max_by(|a, b| {
                (a.1 * channel_support[a.0])
                    .partial_cmp(&(b.1 * channel_support[b.0]))
                    .unwrap_or(Ordering::Equal)
            })
            .map(|(idx, _)| idx)
            .unwrap_or(0);

        lines.push(Line::from(vec![Span::styled(
            format!(
                "{} {} ({})",
                badge,
                selected.name,
                scope_label(selected.scope)
            ),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(format!(
            "  status {} | coherence {:>5.1}% | notoriety {:>5.2} | stage {} ticks",
            status_label,
            coherence_pct,
            scaled(selected.notoriety).max(0.0),
            selected.ticks_in_status
        )));
        if !audience_names.is_empty() {
            lines.push(Line::from(format!(
                "  audience {}",
                audience_names.join(", ")
            )));
        }
        lines.push(Line::from(format!(
            "  channels: {} {:+.2}/{:>3}% | {} {:+.2}/{:>3}% | {} {:+.2}/{:>3}% | {} {:+.2}/{:>3}%",
            CHANNEL_LABELS[0],
            channel_support[0],
            (channel_weights[0] * 100.0).round() as i32,
            CHANNEL_LABELS[1],
            channel_support[1],
            (channel_weights[1] * 100.0).round() as i32,
            CHANNEL_LABELS[2],
            channel_support[2],
            (channel_weights[2] * 100.0).round() as i32,
            CHANNEL_LABELS[3],
            channel_support[3],
            (channel_weights[3] * 100.0).round() as i32,
        )));
        lines.push(Line::from(format!(
            "  top channel: {}",
            CHANNEL_LABELS[top_idx]
        )));
        lines.push(Line::from(format!(
            "  ΔSentiment K {:+.2} T {:+.2} E {:+.2} A {:+.2}",
            scaled(selected.sentiment_knowledge),
            scaled(selected.sentiment_trust),
            scaled(selected.sentiment_equity),
            scaled(selected.sentiment_agency)
        )));
        lines.push(Line::from(format!(
            "  Logistics {:+.2} | Morale {:+.2} | Power {:+.2}",
            scaled(selected.logistics_bonus),
            scaled(selected.morale_bonus),
            scaled(selected.power_bonus)
        )));
        lines.push(Line::from(""));
    } else if state.influencer_index.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "No influencers tracked yet",
            Style::default().fg(Color::DarkGray),
        )]));
    }

    let mut rendered_entries = 0usize;
    for id in state.influencer_order.iter() {
        if Some(*id) == selected_id {
            continue;
        }
        if let Some(entry) = state.influencer_index.get(id) {
            let badge = lifecycle_badge(entry.lifecycle);
            let coherence_pct = (scaled(entry.coherence) * 100.0).clamp(0.0, 100.0);
            let channel_weights = [
                scaled(entry.weight_popular).clamp(0.0, 1.0),
                scaled(entry.weight_peer).clamp(0.0, 1.0),
                scaled(entry.weight_institutional).clamp(0.0, 1.0),
                scaled(entry.weight_humanitarian).clamp(0.0, 1.0),
            ];
            let channel_support = [
                scaled(entry.support_popular).clamp(0.0, 1.0),
                scaled(entry.support_peer).clamp(0.0, 1.0),
                scaled(entry.support_institutional).clamp(0.0, 1.0),
                scaled(entry.support_humanitarian).clamp(0.0, 1.0),
            ];
            let top_idx = channel_weights
                .iter()
                .enumerate()
                .max_by(|a, b| {
                    (a.1 * channel_support[a.0])
                        .partial_cmp(&(b.1 * channel_support[b.0]))
                        .unwrap_or(Ordering::Equal)
                })
                .map(|(idx, _)| idx)
                .unwrap_or(0);

            lines.push(Line::from(vec![Span::styled(
                format!("  {} {} ({})", badge, entry.name, scope_label(entry.scope)),
                Style::default().fg(Color::Gray),
            )]));
            lines.push(Line::from(format!(
                "    status {} | coh {:>5.1}% | not {:>5.2} | growth {:+.2}",
                lifecycle_label(entry.lifecycle),
                coherence_pct,
                scaled(entry.notoriety).max(0.0),
                scaled(entry.growth_rate)
            )));
            lines.push(Line::from(format!(
                "    top {} {:+.2} | domains {}",
                CHANNEL_LABELS[top_idx],
                channel_support[top_idx],
                format_domains(entry.domains)
            )));
            lines.push(Line::from(""));
            rendered_entries += 1;
            if lines.len() + 1 >= area.height as usize {
                break;
            }
        }
    }
    if rendered_entries == 0 && selected_id.is_none() && !state.influencer_index.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "No influencers match current filter",
            Style::default().fg(Color::DarkGray),
        )]));
    }
    if let Some(last) = lines.last() {
        if last.spans.is_empty() {
            lines.pop();
        }
    }

    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn draw_terrain(frame: &mut Frame, area: Rect, state: &UiState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Terrain Summary");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height < 3 {
        return;
    }

    let summary = &state.terrain_summary;
    let mut lines: Vec<Line> = Vec::new();
    if summary.total == 0 {
        lines.push(Line::from(Span::styled(
            "No terrain data",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        lines.push(Line::from(format!(
            "Tiles {:>5} | Grid {}×{}",
            summary.total, summary.width, summary.height
        )));
        if !summary.type_counts.is_empty() {
            lines.push(Line::from(Span::styled(
                "Top Biomes:",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
            for (terrain, count) in &summary.type_counts {
                let pct = (*count as f32 / summary.total as f32) * 100.0;
                lines.push(Line::from(format!(
                    "  {:<24} {:>5} ({:>4.1}%)",
                    terrain_label(*terrain),
                    count,
                    pct
                )));
            }
        }
        if !summary.tag_counts.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Tag Coverage:",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )));
            for (label, count) in &summary.tag_counts {
                let pct = (*count as f32 / summary.total as f32) * 100.0;
                lines.push(Line::from(format!(
                    "  {:<14} {:>5} ({:>4.1}%)",
                    label, count, pct
                )));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Palette Legend:",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
        let mut row: Vec<Span> = Vec::new();
        for (idx, terrain) in TerrainType::VALUES.iter().enumerate() {
            if idx % 3 == 0 {
                if !row.is_empty() {
                    lines.push(Line::from(row));
                    row = Vec::new();
                }
            }
            let color = terrain_color(*terrain);
            row.push(Span::styled("  ", Style::default().bg(color)));
            row.push(Span::raw(format!(
                " {:02} {:<18}",
                *terrain as usize,
                terrain_label(*terrain)
            )));
        }
        if !row.is_empty() {
            lines.push(Line::from(row));
        }
    }

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}

fn draw_logs(frame: &mut Frame, area: Rect, state: &UiState) {
    let block = Block::default().borders(Borders::ALL).title("Logs");
    let lines: Vec<Line> = state
        .logs
        .iter()
        .map(|entry| Line::from(Span::raw(entry)))
        .collect();
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(block, area);
    frame.render_widget(
        paragraph,
        area.inner(&Margin {
            vertical: 1,
            horizontal: 1,
        }),
    );
}

fn draw_corruption(frame: &mut Frame, area: Rect, state: &UiState) {
    let block = Block::default().borders(Borders::ALL).title("Corruption");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height < 3 {
        return;
    }

    let summary = &state.corruption_summary;
    let mut lines: Vec<Line> = Vec::new();
    let mut remaining = inner.height as usize;

    let headline = Line::from(vec![
        Span::styled(
            format!("Active {}", summary.active_incidents),
            Style::default()
                .fg(if summary.active_incidents > 0 {
                    Color::LightRed
                } else {
                    Color::LightGreen
                })
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("Σ |intensity| {:.2}", summary.total_intensity),
            Style::default().fg(Color::LightMagenta),
        ),
    ]);
    lines.push(headline);
    remaining = remaining.saturating_sub(1);

    if remaining > 0 {
        let reputation = Span::styled(
            format!("Reputation {:+.2}", summary.reputation_modifier),
            Style::default().fg(Color::LightBlue),
        );
        let audit = Span::styled(
            format!("Audit cap {}", summary.audit_capacity),
            Style::default().fg(Color::Yellow),
        );
        lines.push(Line::from(vec![reputation, Span::raw("  "), audit]));
        remaining = remaining.saturating_sub(1);
    }

    if remaining > 0 {
        lines.push(Line::from(vec![
            Span::styled(
                "Target",
                Style::default()
                    .fg(Color::LightBlue)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(": "),
            Span::styled(
                state.corruption_target_label(),
                Style::default().fg(Color::White),
            ),
            Span::raw("  (v to cycle, g to inject)"),
        ]));
        remaining = remaining.saturating_sub(1);
    }

    if remaining > 0 {
        if summary.active_incidents == 0 {
            lines.push(Line::from(Span::styled(
                "No active incidents.",
                Style::default().fg(Color::DarkGray),
            )));
            remaining = remaining.saturating_sub(1);
        } else {
            lines.push(Line::from(Span::styled(
                "Active incidents",
                Style::default()
                    .fg(Color::LightYellow)
                    .add_modifier(Modifier::BOLD),
            )));
            remaining = remaining.saturating_sub(1);
            let mut drawn = 0;
            for entry in &state.corruption_entries {
                if drawn >= remaining || drawn >= MAX_CORRUPTION_ENTRIES {
                    break;
                }
                let color = subsystem_color(entry.subsystem);
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("#{} ", entry.id),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(subsystem_label(entry.subsystem), Style::default().fg(color)),
                    Span::raw("  "),
                    Span::styled(
                        format!("{:+.2}", entry.intensity),
                        Style::default().fg(Color::LightRed),
                    ),
                    Span::raw("  τ="),
                    Span::raw(format!("{}", entry.exposure_timer)),
                    Span::raw("  ρ="),
                    Span::raw(format!("{}", entry.restitution_window)),
                ]));
                drawn += 1;
            }
            remaining = remaining.saturating_sub(drawn);
        }
    }

    if remaining > 0 && !state.corruption_exposures.is_empty() {
        lines.push(Line::from(Span::styled(
            "Recent exposures",
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        )));
        remaining = remaining.saturating_sub(1);
        let mut drawn = 0;
        for exposure in &state.corruption_exposures {
            if drawn >= remaining || drawn >= MAX_CORRUPTION_EXPOSURES {
                break;
            }
            let color = subsystem_color(exposure.subsystem);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("tick {:>4}", exposure.tick),
                    Style::default().fg(Color::Gray),
                ),
                Span::raw("  "),
                Span::styled(
                    subsystem_label(exposure.subsystem),
                    Style::default().fg(color),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("{:+.2}", exposure.intensity),
                    Style::default().fg(Color::LightRed),
                ),
                Span::raw("  id "),
                Span::raw(exposure.incident_id.to_string()),
            ]));
            drawn += 1;
        }
    }

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, inner);
}

fn draw_recent_ticks(frame: &mut Frame, area: Rect, state: &UiState) {
    let block = Block::default().borders(Borders::ALL).title("Recent Ticks");
    let lines: Vec<Line> = state.recent_ticks.iter().map(format_tick_line).collect();

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(block, area);
    frame.render_widget(
        paragraph,
        area.inner(&Margin {
            vertical: 1,
            horizontal: 1,
        }),
    );
}

fn format_tick_line(delta: &WorldDelta) -> Line<'_> {
    Line::from(vec![
        Span::styled(
            format!("tick {:>4}", delta.header.tick),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw(" | tiles "),
        Span::styled(
            format!("{:>5}", delta.header.tile_count),
            Style::default().fg(Color::Cyan),
        ),
        Span::raw(" | links "),
        Span::styled(
            format!("{:>5}", delta.header.logistics_count),
            Style::default().fg(Color::Magenta),
        ),
        Span::raw(" | pops "),
        Span::raw(format!("{:>4}", delta.header.population_count)),
        Span::raw(" | power "),
        Span::raw(format!("{:>5}", delta.header.power_count)),
    ])
}
fn scope_label(scope: InfluenceScopeKind) -> &'static str {
    match scope {
        InfluenceScopeKind::Local => "Local",
        InfluenceScopeKind::Regional => "Regional",
        InfluenceScopeKind::Global => "Global",
        InfluenceScopeKind::Generation => "Generational",
    }
}

fn lifecycle_label(lifecycle: InfluenceLifecycle) -> &'static str {
    match lifecycle {
        InfluenceLifecycle::Potential => "Potential",
        InfluenceLifecycle::Active => "Active",
        InfluenceLifecycle::Dormant => "Dormant",
    }
}

fn lifecycle_badge(lifecycle: InfluenceLifecycle) -> &'static str {
    match lifecycle {
        InfluenceLifecycle::Potential => "△",
        InfluenceLifecycle::Active => "★",
        InfluenceLifecycle::Dormant => "◼",
    }
}

fn lifecycle_priority(lifecycle: InfluenceLifecycle) -> u8 {
    match lifecycle {
        InfluenceLifecycle::Active => 0,
        InfluenceLifecycle::Potential => 1,
        InfluenceLifecycle::Dormant => 2,
    }
}

fn domain_label(domain: InfluenceDomain) -> &'static str {
    match domain {
        InfluenceDomain::Sentiment => "Sentiment",
        InfluenceDomain::Discovery => "Discovery",
        InfluenceDomain::Logistics => "Logistics",
        InfluenceDomain::Production => "Production",
        InfluenceDomain::Humanitarian => "Humanitarian",
    }
}

fn format_domains(mask: u32) -> String {
    let domains = influence_domains_from_mask(mask);
    if domains.is_empty() {
        return "—".to_string();
    }
    domains
        .into_iter()
        .map(domain_label)
        .collect::<Vec<_>>()
        .join(", ")
}

fn draw_sentiment(frame: &mut Frame, area: Rect, state: &UiState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Sentiment Sphere");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < 10 || inner.height < 6 {
        return;
    }

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(inner);

    let left = columns[0];
    let right = columns[1];

    let left_split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(left);

    let trust_label = Paragraph::new(Line::from(vec![Span::styled(
        "Trust ↑",
        Style::default().fg(Color::Gray),
    )]))
    .alignment(Alignment::Center);
    frame.render_widget(trust_label, left_split[0]);

    frame.render_widget(HeatmapWidget::new(&state.sentiment), left_split[1]);

    let bottom_line = Line::from(vec![
        Span::styled("Information Scarcity", Style::default().fg(Color::Gray)),
        Span::raw("  <->  "),
        Span::styled("Knowledge Access", Style::default().fg(Color::Gray)),
    ]);
    let bottom = Paragraph::new(bottom_line).alignment(Alignment::Center);
    frame.render_widget(bottom, left_split[2]);

    let right_split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(11),
            Constraint::Length(9),
            Constraint::Length(10),
            Constraint::Min(4),
        ])
        .split(right);

    draw_sentiment_legend(frame, right_split[0], state);
    draw_axis_drivers(frame, right_split[1], &state.sentiment);
    draw_culture_summary(frame, right_split[2], &state.culture_summary);
    draw_demographics(frame, right_split[3], &state.demographics);
}

fn draw_sentiment_legend(frame: &mut Frame, area: Rect, state: &UiState) {
    let sentiment = &state.sentiment;
    let mut lines = Vec::new();
    lines.push(Line::from(vec![Span::styled(
        "Axes",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )]));
    for (idx, axis_name) in AXIS_NAMES.iter().enumerate() {
        let value = match idx {
            0 => sentiment.axes.knowledge,
            1 => sentiment.axes.trust,
            2 => sentiment.axes.equity,
            _ => sentiment.axes.agency,
        };
        let bias = state.axis_bias[idx];
        let marker = if state.selected_axis == Some(idx) {
            Span::styled("▶ ", Style::default().fg(Color::Yellow))
        } else {
            Span::raw("  ")
        };
        lines.push(Line::from(vec![
            marker,
            Span::raw(format!("{:<9}: ", axis_name)),
            Span::raw(format!("{:+.2}", value)),
            Span::raw("  bias "),
            Span::raw(format!("{:+.2}", bias)),
        ]));
    }
    let magnitude = (sentiment.axes.knowledge.powi(2) + sentiment.axes.trust.powi(2)).sqrt();
    lines.push(Line::from(vec![Span::raw(format!(
        "Vector |v|: {:.2}",
        magnitude
    ))]));
    lines.push(Line::from(vec![Span::raw(format!(
        "Population weight: {:.0}",
        sentiment.total_weight
    ))]));
    lines.push(Line::from(vec![Span::raw(String::new())]));
    lines.push(Line::from(vec![Span::styled(
        "Quadrants",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )]));

    for (idx, label) in QUADRANT_LABELS.iter().enumerate() {
        let (r, g, b) = QUADRANT_COLORS[idx];
        let color = Color::Rgb(r, g, b);
        lines.push(Line::from(vec![
            Span::styled("■", Style::default().fg(color)),
            Span::raw(format!(" {}", label)),
        ]));
    }

    if sentiment.total_weight <= 0.0 {
        lines.push(Line::from(vec![Span::styled(
            "No cohorts sampled yet",
            Style::default().fg(Color::DarkGray),
        )]));
    }

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn draw_axis_drivers(frame: &mut Frame, area: Rect, sentiment: &SentimentViewModel) {
    let mut lines = Vec::new();
    lines.push(Line::from(vec![Span::styled(
        "Axis Drivers",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    for (idx, name) in AXIS_NAMES.iter().enumerate() {
        let color = AXIS_COLORS[idx];
        lines.push(Line::from(vec![Span::styled(
            *name,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )]));

        let entries = &sentiment.drivers[idx];
        if entries.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                "  (no data)",
                Style::default().fg(Color::DarkGray),
            )]));
            continue;
        }

        for driver in entries {
            let tag = driver_category_tag(driver.category);
            lines.push(Line::from(vec![Span::raw(format!(
                "  [{tag:^9}] {:+.2} × {:>5.2} · {}",
                driver.value, driver.weight, driver.label
            ))]));
        }
        lines.push(Line::from(""));
    }

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn draw_culture_summary(frame: &mut Frame, area: Rect, summary: &CultureSummary) {
    let mut lines = Vec::new();
    lines.push(Line::from(vec![Span::styled(
        "Culture Snapshot",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )]));

    if summary.global_traits.is_empty() && summary.divergences.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "No culture data captured yet",
            Style::default().fg(Color::DarkGray),
        )]));
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
        return;
    }

    if !summary.global_traits.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "Global Identity",
            Style::default().fg(Color::Gray),
        )]));
        for (label, value) in &summary.global_traits {
            lines.push(Line::from(vec![Span::raw(format!(
                "  {:<28} {:+.2}",
                label, value
            ))]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "Divergence Watch",
        Style::default().fg(Color::Gray),
    )]));
    if summary.divergences.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "  (stable)",
            Style::default().fg(Color::DarkGray),
        )]));
    } else {
        for entry in &summary.divergences {
            lines.push(Line::from(vec![Span::raw(format!(
                "  #{:03} [{:>8}] Δ {:+.2}",
                entry.id,
                culture_scope_label(entry.scope),
                entry.magnitude
            ))]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "Active Tensions",
        Style::default().fg(Color::Gray),
    )]));
    if summary.tensions.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "  (none)",
            Style::default().fg(Color::DarkGray),
        )]));
    } else {
        for tension in &summary.tensions {
            lines.push(Line::from(vec![Span::raw(format!(
                "  {:<16} #{:03} [{:>8}] {:+.2} ({}t)",
                culture_tension_label(tension.kind),
                tension.layer_id,
                culture_scope_label(tension.scope),
                tension.severity,
                tension.timer
            ))]));
        }
    }

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn draw_demographics(frame: &mut Frame, area: Rect, snapshot: &DemographicSnapshot) {
    let mut lines = Vec::new();
    lines.push(Line::from(vec![Span::styled(
        "Demographics",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )]));

    if snapshot.total_population <= 0.0 {
        lines.push(Line::from(vec![Span::styled(
            "No population data captured yet",
            Style::default().fg(Color::DarkGray),
        )]));
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
        frame.render_widget(paragraph, area);
        return;
    }

    lines.push(Line::from(vec![Span::raw(format!(
        "Total: {:>8.0} | Cohorts: {:>3} | Morale: {:>5.1}%",
        snapshot.total_population,
        snapshot.cohort_count,
        snapshot.avg_morale * 100.0
    ))]));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "Generations",
        Style::default().fg(Color::Gray),
    )]));
    if snapshot.generations.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "  (no generations indexed)",
            Style::default().fg(Color::DarkGray),
        )]));
    } else {
        for stat in &snapshot.generations {
            lines.push(Line::from(vec![Span::raw(format!(
                "  {:<18} {:>5.1}% pop | morale {:>5.1}%",
                stat.name,
                stat.share,
                stat.avg_morale * 100.0
            ))]));
            lines.push(Line::from(vec![Span::raw(format!(
                "      bias K:{:+.2} T:{:+.2} E:{:+.2} A:{:+.2}",
                stat.bias[0], stat.bias[1], stat.bias[2], stat.bias[3]
            ))]));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "Workforce Allocation",
        Style::default().fg(Color::Gray),
    )]));
    for (label, value) in WORKFORCE_LABELS
        .iter()
        .zip(snapshot.workforce_distribution.iter())
    {
        if *value <= 0.01 {
            continue;
        }
        lines.push(Line::from(vec![Span::raw(format!(
            "  {:<12} {:>5.1}%",
            label, value
        ))]));
    }

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

struct HeatmapWidget<'a> {
    model: &'a SentimentViewModel,
}

impl<'a> HeatmapWidget<'a> {
    fn new(model: &'a SentimentViewModel) -> Self {
        Self { model }
    }
}

impl<'a> Widget for HeatmapWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let width = area.width as usize;
        let height = area.height as usize;
        let center_x = width / 2;
        let center_y = height / 2;
        let max_x = (width.saturating_sub(1)) as f32;
        let max_y = (height.saturating_sub(1)) as f32;
        let grid_max = (HEATMAP_SIZE - 1) as f32;
        let half_index = (HEATMAP_SIZE - 1) / 2;

        for y in 0..height {
            for x in 0..width {
                let norm_x = if max_x > 0.0 {
                    (x as f32 / max_x) * grid_max
                } else {
                    0.0
                };
                let norm_y = if max_y > 0.0 {
                    (y as f32 / max_y) * grid_max
                } else {
                    0.0
                };
                let grid_x = norm_x.round() as usize;
                let grid_y = norm_y.round() as usize;

                let value =
                    self.model.heatmap[grid_y.min(HEATMAP_SIZE - 1)][grid_x.min(HEATMAP_SIZE - 1)];
                let quadrant = {
                    let qx = if grid_x > half_index { 1 } else { 0 };
                    let qy = if grid_y > half_index { 1 } else { 0 };
                    qy * 2 + qx
                };
                let color = blend_quadrant_color(quadrant, value);
                let mut symbol = " ";
                let mut style = Style::default().bg(color);
                if x == center_x && y == center_y {
                    symbol = "┼";
                    style = style.fg(Color::Gray);
                } else if x == center_x {
                    symbol = "│";
                    style = style.fg(Color::Gray);
                } else if y == center_y {
                    symbol = "─";
                    style = style.fg(Color::Gray);
                }

                let cell = buf.get_mut(area.x + x as u16, area.y + y as u16);
                cell.set_symbol(symbol);
                cell.set_style(style);
            }
        }

        let vx = self.model.vector.0.clamp(-1.0, 1.0);
        let vy = self.model.vector.1.clamp(-1.0, 1.0);

        let target_x = if max_x > 0.0 {
            ((vx + 1.0) * 0.5 * max_x).round() as i32
        } else {
            center_x as i32
        };
        let target_y = if max_y > 0.0 {
            ((1.0 - (vy + 1.0) * 0.5) * max_y).round() as i32
        } else {
            center_y as i32
        };

        let start_x = center_x as i32;
        let start_y = center_y as i32;
        let steps = (target_x - start_x).abs().max((target_y - start_y).abs());

        if steps == 0 {
            let cell = buf.get_mut(area.x + center_x as u16, area.y + center_y as u16);
            let style = cell.style();
            cell.set_symbol("•");
            cell.set_style(style.fg(Color::White));
            return;
        }

        for step in 1..=steps {
            let x = start_x + (target_x - start_x) * step / steps;
            let y = start_y + (target_y - start_y) * step / steps;
            if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
                continue;
            }
            let cell = buf.get_mut(area.x + x as u16, area.y + y as u16);
            let base_style = cell.style();
            if step == steps {
                cell.set_symbol("●");
                cell.set_style(
                    base_style
                        .fg(Color::Black)
                        .bg(Color::White)
                        .add_modifier(Modifier::BOLD),
                );
            } else {
                cell.set_symbol("·");
                cell.set_style(base_style.fg(Color::White));
            }
        }
    }
}

fn blend_quadrant_color(index: usize, intensity: f32) -> Color {
    let idx = index.min(QUADRANT_COLORS.len() - 1);
    let (r, g, b) = QUADRANT_COLORS[idx];
    blend_color((r, g, b), intensity)
}

fn blend_color(target: (u8, u8, u8), intensity: f32) -> Color {
    let intensity = intensity.clamp(0.0, 1.0);
    let (br, bg, bb) = BACKGROUND_COLOR;
    let r = br as f32 + (target.0 as f32 - br as f32) * intensity;
    let g = bg as f32 + (target.1 as f32 - bg as f32) * intensity;
    let b = bb as f32 + (target.2 as f32 - bb as f32) * intensity;
    Color::Rgb(r.round() as u8, g.round() as u8, b.round() as u8)
}
