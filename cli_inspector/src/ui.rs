use std::collections::VecDeque;

use ratatui::layout::{Constraint, Direction, Layout, Margin};
use ratatui::prelude::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use sim_proto::WorldDelta;

pub struct UiState {
    pub recent_ticks: VecDeque<WorldDelta>,
    pub max_history: usize,
    pub logs: VecDeque<String>,
    pub max_logs: usize,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            recent_ticks: VecDeque::new(),
            max_history: 32,
            logs: VecDeque::new(),
            max_logs: 8,
        }
    }
}

impl UiState {
    pub fn push_delta(&mut self, delta: WorldDelta) {
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
}

pub fn draw_ui(frame: &mut Frame, state: &UiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Length(7),
            Constraint::Min(5),
        ])
        .split(frame.size());

    draw_header(frame, chunks[0]);
    draw_commands(frame, chunks[1]);
    draw_logs(frame, chunks[2], state);
    draw_recent_ticks(frame, chunks[3], state);
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
    let lines: Vec<Line> = state
        .recent_ticks
        .iter()
        .map(|delta| {
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
        })
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
