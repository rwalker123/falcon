use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::backend::CrosstermBackend;
use ratatui::prelude::*;
use sim_proto::WorldDelta;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tracing::{error, info};

use crate::ui::{draw_ui, UiState};

pub struct InspectorApp {
    terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
    ui_state: UiState,
    receiver: UnboundedReceiver<WorldDelta>,
    command_sender: Sender<ClientCommand>,
    shutdown_sender: Sender<()>,
    log_receiver: Receiver<String>,
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

            if last_draw.elapsed() >= std::time::Duration::from_millis(100) {
                self.terminal.draw(|frame| draw_ui(frame, &self.ui_state))?;
                last_draw = Instant::now();
            }

            if event::poll(std::time::Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char(' ') => {
                            if let Err(err) = self.command_sender.send(ClientCommand::Turn(1)) {
                                error!("Failed to send turn command: {}", err);
                            } else {
                                info!("Requested turn +1");
                            }
                        }
                        KeyCode::Char('t') => {
                            if let Err(err) = self.command_sender.send(ClientCommand::Turn(10)) {
                                error!("Failed to send turn command: {}", err);
                            } else {
                                info!("Requested turn +10");
                            }
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
}

pub fn channel() -> (UnboundedSender<WorldDelta>, UnboundedReceiver<WorldDelta>) {
    unbounded_channel()
}

#[derive(Debug, Clone)]
pub enum ClientCommand {
    Turn(u32),
    Heat { entity: u64, delta: i64 },
}
