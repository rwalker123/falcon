use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::backend::CrosstermBackend;
use ratatui::prelude::*;
use sim_runtime::WorldDelta;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tracing::{error, info, trace, warn};

use crate::ui::{draw_ui, UiState, AXIS_BIAS_STEP};

const CLIENT_CHANNEL_KEYS: [&str; 4] = ["popular", "peer", "institutional", "humanitarian"];

#[derive(Debug, Clone, Copy)]
pub enum ClientSupportChannel {
    Popular,
    Peer,
    Institutional,
    Humanitarian,
}

impl ClientSupportChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            ClientSupportChannel::Popular => CLIENT_CHANNEL_KEYS[0],
            ClientSupportChannel::Peer => CLIENT_CHANNEL_KEYS[1],
            ClientSupportChannel::Institutional => CLIENT_CHANNEL_KEYS[2],
            ClientSupportChannel::Humanitarian => CLIENT_CHANNEL_KEYS[3],
        }
    }

    fn from_key(value: &str) -> Option<Self> {
        match value {
            "popular" => Some(ClientSupportChannel::Popular),
            "peer" => Some(ClientSupportChannel::Peer),
            "institutional" => Some(ClientSupportChannel::Institutional),
            "humanitarian" => Some(ClientSupportChannel::Humanitarian),
            _ => None,
        }
    }
}

pub struct InspectorApp {
    terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
    ui_state: UiState,
    receiver: UnboundedReceiver<WorldDelta>,
    command_sender: Sender<ClientCommand>,
    shutdown_sender: Sender<()>,
    log_receiver: Receiver<String>,
    playback_active: bool,
    playback_interval: Duration,
    last_playback: Instant,
}

impl InspectorApp {
    pub fn new(
        receiver: UnboundedReceiver<WorldDelta>,
        command_sender: Sender<ClientCommand>,
        shutdown_sender: Sender<()>,
        log_receiver: Receiver<String>,
    ) -> Result<Self> {
        let stdout = std::io::stdout();
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        crossterm::terminal::enable_raw_mode()?;
        terminal.clear()?;
        terminal.hide_cursor()?;
        Ok(Self {
            terminal,
            ui_state: UiState::default(),
            receiver,
            command_sender,
            shutdown_sender,
            log_receiver,
            playback_active: false,
            playback_interval: Duration::from_millis(500),
            last_playback: Instant::now(),
        })
    }

    pub fn run(mut self) -> Result<()> {
        let mut last_draw = Instant::now();

        loop {
            while let Ok(delta) = self.receiver.try_recv() {
                self.ui_state.push_delta(delta);
            }

            while let Ok(line) = self.log_receiver.try_recv() {
                self.ui_state.push_log(line);
            }

            if self.playback_active && self.last_playback.elapsed() >= self.playback_interval {
                match self.command_sender.send(ClientCommand::Turn(1)) {
                    Ok(_) => {
                        self.last_playback = Instant::now();
                        trace!("auto-play: turn advanced by 1");
                    }
                    Err(err) => {
                        error!("Failed to advance turn during auto-play: {}", err);
                        self.playback_active = false;
                        self.ui_state
                            .push_log("Auto-play disabled due to command error");
                    }
                }
            }

            if last_draw.elapsed() >= std::time::Duration::from_millis(100) {
                self.terminal.draw(|frame| draw_ui(frame, &self.ui_state))?;
                last_draw = Instant::now();
            }

            if event::poll(std::time::Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char(' ') => {
                            if let Err(err) = self
                                .command_sender
                                .send(ClientCommand::SubmitOrders { faction: 0 })
                            {
                                error!("Failed to submit orders: {}", err);
                            } else {
                                info!("Submitted orders for faction 0");
                            }
                        }
                        KeyCode::Char('t') => {
                            if let Err(err) = self.command_sender.send(ClientCommand::Turn(10)) {
                                error!("Failed to send turn command: {}", err);
                            } else {
                                info!("Requested turn +10");
                            }
                        }
                        KeyCode::Char('b') => {
                            if let Some(current_tick) = self.ui_state.latest_tick() {
                                if current_tick == 0 {
                                    warn!("Cannot rollback before tick 0");
                                } else if let Err(err) =
                                    self.command_sender.send(ClientCommand::Rollback {
                                        tick: current_tick.saturating_sub(1),
                                    })
                                {
                                    error!("Failed to send rollback command: {}", err);
                                } else {
                                    info!("Requested rollback to tick {}", current_tick - 1);
                                }
                            } else {
                                warn!("No snapshot history recorded yet");
                            }
                        }
                        KeyCode::Char('j') => {
                            self.ui_state.select_next_influencer();
                        }
                        KeyCode::Char('k') => {
                            self.ui_state.select_previous_influencer();
                        }
                        KeyCode::Char('s') => {
                            self.send_influencer_command(ClientInfluencerAction::Support);
                        }
                        KeyCode::Char('x') => {
                            self.send_influencer_command(ClientInfluencerAction::Suppress);
                        }
                        KeyCode::Char('c') => {
                            self.send_channel_support(1.0);
                        }
                        KeyCode::Char('f') => {
                            self.ui_state.cycle_influencer_filter();
                        }
                        KeyCode::Char('i') => {
                            self.send_spawn_influencer(None, None);
                        }
                        KeyCode::Char('v') => {
                            self.ui_state.cycle_corruption_target();
                        }
                        KeyCode::Char('g') => {
                            self.send_corruption_injection();
                        }
                        KeyCode::Char('h') => {
                            if let Some(entity) = self.ui_state.latest_tile_entity() {
                                if let Err(err) = self.command_sender.send(ClientCommand::Heat {
                                    entity,
                                    delta: 100_000,
                                }) {
                                    error!("Failed to send heat command: {}", err);
                                } else {
                                    info!("Requested heat for entity {}", entity);
                                }
                            }
                        }
                        KeyCode::Char('.') => {
                            if let Err(err) = self.command_sender.send(ClientCommand::Turn(1)) {
                                error!("Failed to send turn command: {}", err);
                            } else {
                                trace!("Manual step: turn+1");
                            }
                        }
                        KeyCode::Char('p') | KeyCode::Char('P') => {
                            self.playback_active = !self.playback_active;
                            if self.playback_active {
                                self.last_playback = Instant::now();
                                let secs = self.playback_interval.as_secs_f32();
                                self.ui_state
                                    .push_log(format!("Auto-play enabled ({:.2}s interval)", secs));
                            } else {
                                self.ui_state.push_log("Auto-play paused");
                            }
                        }
                        KeyCode::Char(']') | KeyCode::Char('}') => {
                            self.adjust_playback_interval(0.75);
                        }
                        KeyCode::Char('[') | KeyCode::Char('{') => {
                            self.adjust_playback_interval(1.25);
                        }
                        KeyCode::Char('1') => self.ui_state.select_axis(0),
                        KeyCode::Char('2') => self.ui_state.select_axis(1),
                        KeyCode::Char('3') => self.ui_state.select_axis(2),
                        KeyCode::Char('4') => self.ui_state.select_axis(3),
                        KeyCode::Char('=') | KeyCode::Char('+') => {
                            if let Some((axis, value)) =
                                self.ui_state.adjust_selected_axis(AXIS_BIAS_STEP)
                            {
                                self.send_axis_bias(axis, value);
                            }
                        }
                        KeyCode::Char('-') | KeyCode::Char('_') => {
                            if let Some((axis, value)) =
                                self.ui_state.adjust_selected_axis(-AXIS_BIAS_STEP)
                            {
                                self.send_axis_bias(axis, value);
                            }
                        }
                        KeyCode::Char('0') => {
                            if let Some(changes) = self.ui_state.reset_axis_bias() {
                                for (axis, value) in changes {
                                    self.send_axis_bias(axis, value);
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        self.terminal.show_cursor()?;
        crossterm::terminal::disable_raw_mode()?;
        let _ = self.shutdown_sender.send(());
        Ok(())
    }

    fn adjust_playback_interval(&mut self, factor: f32) {
        let current = self.playback_interval.as_secs_f32();
        let mut new_value = current * factor;
        if !new_value.is_finite() || new_value <= 0.0 {
            new_value = 0.05;
        }
        new_value = new_value.clamp(0.05, 5.0);
        self.playback_interval = Duration::from_secs_f64(new_value as f64);
        self.last_playback = Instant::now();
        self.ui_state
            .push_log(format!("Auto-play interval set to {:.2}s", new_value));
    }

    fn send_influencer_command(&mut self, action: ClientInfluencerAction) {
        let Some(id) = self.ui_state.selected_influencer_id() else {
            self.ui_state.push_log("No influencer selected");
            return;
        };
        let name = self
            .ui_state
            .selected_influencer()
            .map(|info| info.name.clone())
            .unwrap_or_else(|| id.to_string());
        let magnitude = 1.0f32;
        let command = match action {
            ClientInfluencerAction::Support => ClientCommand::SupportInfluencer { id, magnitude },
            ClientInfluencerAction::Suppress => ClientCommand::SuppressInfluencer { id, magnitude },
        };
        if let Err(err) = self.command_sender.send(command) {
            error!("Failed to send influencer command: {}", err);
            self.ui_state
                .push_log("Failed to dispatch influencer command");
        } else {
            match action {
                ClientInfluencerAction::Support => self
                    .ui_state
                    .push_log(format!("Support sent to {} (+{:.1})", name, magnitude)),
                ClientInfluencerAction::Suppress => self
                    .ui_state
                    .push_log(format!("Suppress sent to {} (-{:.1})", name, magnitude)),
            }
        }
    }

    fn send_spawn_influencer(&mut self, scope: Option<String>, generation: Option<u32>) {
        if let Err(err) = self
            .command_sender
            .send(ClientCommand::SpawnInfluencer { scope, generation })
        {
            error!("Failed to send spawn influencer command: {}", err);
            self.ui_state
                .push_log("Failed to send spawn influencer command");
        } else {
            self.ui_state.push_log("Spawn influencer command sent");
        }
    }

    fn send_channel_support(&mut self, magnitude: f32) {
        let Some(id) = self.ui_state.selected_influencer_id() else {
            self.ui_state
                .push_log("Select an influencer before applying channel support");
            return;
        };
        let Some(channel_key) = self.ui_state.dominant_channel_key() else {
            self.ui_state
                .push_log("No dominant channel identified for the current selection");
            return;
        };
        let Some(channel) = ClientSupportChannel::from_key(channel_key) else {
            self.ui_state
                .push_log(format!("Unsupported channel '{}'", channel_key));
            return;
        };
        let name = self
            .ui_state
            .selected_influencer()
            .map(|info| info.name.clone())
            .unwrap_or_else(|| id.to_string());
        if let Err(err) = self.command_sender.send(ClientCommand::SupportChannel {
            id,
            channel,
            magnitude,
        }) {
            error!("Failed to send channel support command: {}", err);
            self.ui_state
                .push_log("Failed to dispatch channel support command");
        } else {
            self.ui_state.push_log(format!(
                "Channel boost ({}) sent to {} (+{:.1})",
                channel.as_str(),
                name,
                magnitude
            ));
        }
    }

    fn send_corruption_injection(&mut self) {
        let subsystem_key = self.ui_state.corruption_target_command_key().to_string();
        let subsystem_label = self.ui_state.corruption_target_label();
        let intensity = 0.25f32;
        let exposure_timer = 3u16;
        if let Err(err) = self.command_sender.send(ClientCommand::InjectCorruption {
            subsystem: subsystem_key,
            intensity,
            exposure_timer,
        }) {
            error!("Failed to send corruption injection: {}", err);
            self.ui_state
                .push_log("Failed to inject corruption incident");
        } else {
            self.ui_state.push_log(format!(
                "Injected {} corruption ({:+.2}, τ={})",
                subsystem_label, intensity, exposure_timer
            ));
        }
    }

    fn send_axis_bias(&self, axis: usize, value: f32) {
        if axis >= 4 {
            warn!(axis, "Axis bias command rejected: invalid axis index");
            return;
        }
        let clamped = value.clamp(-1.0, 1.0);
        if let Err(err) = self.command_sender.send(ClientCommand::SetAxisBias {
            axis: axis as u32,
            value: clamped,
        }) {
            error!("Failed to send axis bias command: {}", err);
        } else {
            trace!(axis, value = clamped, "Axis bias command dispatched");
        }
    }
}

pub fn channel() -> (UnboundedSender<WorldDelta>, UnboundedReceiver<WorldDelta>) {
    unbounded_channel()
}

#[derive(Debug, Clone, Copy)]
enum ClientInfluencerAction {
    Support,
    Suppress,
}

#[derive(Debug, Clone)]
pub enum ClientCommand {
    Turn(u32),
    Heat {
        entity: u64,
        delta: i64,
    },
    SubmitOrders {
        faction: u32,
    },
    Rollback {
        tick: u64,
    },
    SetAxisBias {
        axis: u32,
        value: f32,
    },
    SupportInfluencer {
        id: u32,
        magnitude: f32,
    },
    SuppressInfluencer {
        id: u32,
        magnitude: f32,
    },
    SupportChannel {
        id: u32,
        channel: ClientSupportChannel,
        magnitude: f32,
    },
    SpawnInfluencer {
        scope: Option<String>,
        generation: Option<u32>,
    },
    InjectCorruption {
        subsystem: String,
        intensity: f32,
        exposure_timer: u16,
    },
}
