use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin};
use ratatui::prelude::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};
use ratatui::Frame;

use sim_proto::{
    AxisBiasState, GenerationState, PopulationCohortState, PowerNodeState, TileState, WorldDelta,
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

fn axis_bias_state_to_f32(state: &AxisBiasState) -> [f32; 4] {
    [
        state.knowledge as f32 / SCALE_FACTOR,
        state.trust as f32 / SCALE_FACTOR,
        state.equity as f32 / SCALE_FACTOR,
        state.agency as f32 / SCALE_FACTOR,
    ]
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
}

impl AxisDriver {
    fn new<L: Into<String>>(label: L, value: f32, weight: f32) -> Self {
        Self {
            label: label.into(),
            value,
            weight,
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
    fn rebuild(
        &mut self,
        tiles: &HashMap<u64, TileState>,
        populations: &HashMap<u64, PopulationCohortState>,
        power: &HashMap<u64, PowerNodeState>,
        generations: &HashMap<u16, GenerationState>,
        axis_bias: &[f32; 4],
    ) {
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
            ));
            drivers[1].push(AxisDriver::new(tile_label.clone(), trust, cohort_weight));
            drivers[2].push(AxisDriver::new(tile_label.clone(), equity, cohort_weight));
            drivers[3].push(AxisDriver::new(tile_label, agency, cohort_weight));
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
                    ));
                }
            }
        }

        if total_weight > 0.0 {
            for (idx, bias_value) in axis_bias.iter().enumerate() {
                if bias_value.abs() < 0.0001 {
                    continue;
                }
                drivers[idx].push(AxisDriver::new("Manual Bias", *bias_value, total_weight));
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

pub struct UiState {
    pub recent_ticks: VecDeque<WorldDelta>,
    pub max_history: usize,
    pub logs: VecDeque<String>,
    pub max_logs: usize,
    tile_index: HashMap<u64, TileState>,
    population_index: HashMap<u64, PopulationCohortState>,
    power_index: HashMap<u64, PowerNodeState>,
    generation_index: HashMap<u16, GenerationState>,
    sentiment: SentimentViewModel,
    demographics: DemographicSnapshot,
    axis_bias: [f32; 4],
    selected_axis: Option<usize>,
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
            sentiment: SentimentViewModel::default(),
            demographics: DemographicSnapshot::default(),
            axis_bias: [0.0; 4],
            selected_axis: None,
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

        if let Some(bias) = delta.axis_bias.as_ref() {
            self.axis_bias = axis_bias_state_to_f32(bias);
        }

        self.rebuild_views();
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
        );
        self.demographics = DemographicSnapshot::rebuild(
            &self.population_index,
            &self.tile_index,
            &self.generation_index,
        );
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
}

pub fn draw_ui(frame: &mut Frame, state: &UiState) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(5),
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
        .constraints([Constraint::Length(7), Constraint::Min(5)])
        .split(main_split[1]);

    draw_logs(frame, sidebar[0], state);
    draw_recent_ticks(frame, sidebar[1], state);
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
    let lines = vec![
        Line::from(vec![
            Span::styled("space", Style::default().fg(Color::Yellow)),
            Span::raw("  submit orders (faction 0)"),
        ]),
        Line::from(vec![
            Span::styled("t", Style::default().fg(Color::Yellow)),
            Span::raw("      auto-resolve 10 turns"),
        ]),
        Line::from(vec![
            Span::styled("b", Style::default().fg(Color::Yellow)),
            Span::raw("      rollback to previous tick"),
        ]),
        Line::from(vec![
            Span::styled("h", Style::default().fg(Color::Yellow)),
            Span::raw("      heat most recent tile"),
        ]),
        Line::from(vec![
            Span::styled(".", Style::default().fg(Color::Yellow)),
            Span::raw("      step single turn"),
        ]),
        Line::from(vec![
            Span::styled("p", Style::default().fg(Color::Yellow)),
            Span::raw("      toggle auto-play"),
        ]),
        Line::from(vec![
            Span::styled("]", Style::default().fg(Color::Yellow)),
            Span::raw("      faster auto-play"),
        ]),
        Line::from(vec![
            Span::styled("[", Style::default().fg(Color::Yellow)),
            Span::raw("      slower auto-play"),
        ]),
        Line::from(vec![
            Span::styled("1-4", Style::default().fg(Color::Yellow)),
            Span::raw("    select axis for bias edit"),
        ]),
        Line::from(vec![
            Span::styled("=", Style::default().fg(Color::Yellow)),
            Span::raw("      increase selected axis bias (+0.05)"),
        ]),
        Line::from(vec![
            Span::styled("-", Style::default().fg(Color::Yellow)),
            Span::raw("      decrease selected axis bias (-0.05)"),
        ]),
        Line::from(vec![
            Span::styled("0", Style::default().fg(Color::Yellow)),
            Span::raw("      reset axis biases"),
        ]),
        Line::from(vec![
            Span::styled("q", Style::default().fg(Color::Yellow)),
            Span::raw("      exit inspector"),
        ]),
    ];
    let block = Block::default().borders(Borders::ALL).title("Commands");
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
            Constraint::Min(4),
        ])
        .split(right);

    draw_sentiment_legend(frame, right_split[0], state);
    draw_axis_drivers(frame, right_split[1], &state.sentiment);
    draw_demographics(frame, right_split[2], &state.demographics);
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
            lines.push(Line::from(vec![Span::raw(format!(
                "  {:+.2} · {:>5.0} · {}",
                driver.value, driver.weight, driver.label
            ))]));
        }
        lines.push(Line::from(""));
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
