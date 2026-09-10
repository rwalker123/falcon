//! **Brains** — the plug the turn loop drives (`docs/plan_ai_opponents.md` §2).
//!
//! `decide` is handed the seat's view and hands back the commands to send this turn; the loop
//! sends them and then submits `ready` whatever came back. Two brains ship in this slice:
//!
//! - [`PassBrain`] — submits nothing. The **control** in every comparison.
//! - [`ScriptedBrain`] — replays a fixed command list. The **fixture**: it makes AI-driven turns
//!   assertable without asserting on utility scores.
//!
//! ## The script format
//!
//! One command per line, in `sim_runtime::command_text` form (`parse_command_line`), prefixed with
//! the tick it fires on:
//!
//! ```text
//! # a comment; blank lines are ignored too
//! 12: split_band {faction} {own_band:0} 4
//! +0: assign_labor {faction} {own_band:0} …      # relative: the first tick this brain saw, plus 0
//! ```
//!
//! `<tick>:` is absolute; `+<n>:` is relative to the first tick the brain decided on, for a script
//! that cannot know what tick the world it joins will be at. A line fires on the turn whose tick
//! equals its own, once.
//!
//! Two substitutions, resolved against the view at fire time so a script can name what it cannot
//! know in advance: `{faction}` is this seat's faction, and `{own_band:N}` is the `band_id` of the
//! N-th `populations` row (row order, zero-based) whose `faction` is this seat's. An unresolvable
//! substitution — no such band — is a logged error and the line is skipped.

use std::path::Path;

use rand::rngs::StdRng;
use sim_runtime::{parse_command_line, CommandPayload};
use tracing::{error, info};

use crate::view::SeatView;

/// The plug. An external program in another language implements the same contract over the
/// socket; inside this crate it is this trait.
pub trait Brain {
    fn decide(&mut self, view: &SeatView, rng: &mut StdRng) -> Vec<CommandPayload>;
}

/// Submits end-turn and nothing else.
#[derive(Debug, Default)]
pub struct PassBrain;

impl Brain for PassBrain {
    fn decide(&mut self, _view: &SeatView, _rng: &mut StdRng) -> Vec<CommandPayload> {
        Vec::new()
    }
}

/// The placeholder for this seat's faction id.
const FACTION_PLACEHOLDER: &str = "{faction}";
/// The prefix of the own-band placeholder: `{own_band:N}`.
const OWN_BAND_PLACEHOLDER_PREFIX: &str = "{own_band:";
const PLACEHOLDER_CLOSE: char = '}';
/// The line prefix of a tick relative to the first tick seen.
const RELATIVE_TICK_PREFIX: char = '+';
const TICK_SEPARATOR: char = ':';
const COMMENT_PREFIX: char = '#';

/// When a line fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FireAt {
    Tick(u64),
    /// This many ticks after the first tick the brain decided on.
    AfterFirst(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScriptLine {
    at: FireAt,
    command: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("could not read the script {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("script line {line}: no `<tick>:` prefix")]
    MissingTick { line: usize },
    #[error("script line {line}: `{text}` is not a tick")]
    InvalidTick { line: usize, text: String },
    #[error("script line {line}: no command after the tick")]
    EmptyCommand { line: usize },
}

/// Replays a fixed command list.
pub struct ScriptedBrain {
    faction: u32,
    lines: Vec<ScriptLine>,
    first_tick: Option<u64>,
}

impl ScriptedBrain {
    pub fn load(path: &Path, faction: u32) -> Result<Self, ScriptError> {
        let text = std::fs::read_to_string(path).map_err(|source| ScriptError::Read {
            path: path.display().to_string(),
            source,
        })?;
        Self::from_script(&text, faction)
    }

    pub fn from_script(text: &str, faction: u32) -> Result<Self, ScriptError> {
        Ok(Self {
            faction,
            lines: parse_script(text)?,
            first_tick: None,
        })
    }

    /// The lines that fire on `tick`.
    fn due(&self, tick: u64) -> impl Iterator<Item = &ScriptLine> {
        let first = self.first_tick.unwrap_or(tick);
        self.lines.iter().filter(move |line| match line.at {
            FireAt::Tick(at) => at == tick,
            FireAt::AfterFirst(offset) => first.saturating_add(offset) == tick,
        })
    }
}

impl Brain for ScriptedBrain {
    fn decide(&mut self, view: &SeatView, _rng: &mut StdRng) -> Vec<CommandPayload> {
        let tick = view.snapshot.header.tick;
        if self.first_tick.is_none() {
            self.first_tick = Some(tick);
        }
        let faction = self.faction;
        let mut commands = Vec::new();
        for line in self.due(tick) {
            let resolved = match substitute(&line.command, faction, view) {
                Ok(resolved) => resolved,
                Err(err) => {
                    error!(tick, line = %line.command, %err, "script line skipped");
                    continue;
                }
            };
            match parse_command_line(&resolved) {
                Ok(payload) => {
                    info!(tick, command = %resolved, "script line fires");
                    commands.push(payload);
                }
                Err(err) => error!(tick, line = %resolved, %err, "script line does not parse"),
            }
        }
        commands
    }
}

fn parse_script(text: &str) -> Result<Vec<ScriptLine>, ScriptError> {
    let mut lines = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with(COMMENT_PREFIX) {
            continue;
        }
        let Some((tick_text, command)) = trimmed.split_once(TICK_SEPARATOR) else {
            return Err(ScriptError::MissingTick { line });
        };
        let tick_text = tick_text.trim();
        let at = match tick_text.strip_prefix(RELATIVE_TICK_PREFIX) {
            Some(offset) => FireAt::AfterFirst(parse_tick(offset, line)?),
            None => FireAt::Tick(parse_tick(tick_text, line)?),
        };
        let command = command.trim();
        if command.is_empty() {
            return Err(ScriptError::EmptyCommand { line });
        }
        lines.push(ScriptLine {
            at,
            command: command.to_owned(),
        });
    }
    Ok(lines)
}

fn parse_tick(text: &str, line: usize) -> Result<u64, ScriptError> {
    text.trim().parse().map_err(|_| ScriptError::InvalidTick {
        line,
        text: text.to_owned(),
    })
}

#[derive(Debug, thiserror::Error)]
enum SubstitutionError {
    #[error("`{{own_band:{index}}}`: this seat has only {own} band rows in the view")]
    NoSuchOwnBand { index: usize, own: usize },
    #[error("`{placeholder}` is not a placeholder this brain knows")]
    Unknown { placeholder: String },
}

/// Resolve `{faction}` and `{own_band:N}` against the view.
fn substitute(command: &str, faction: u32, view: &SeatView) -> Result<String, SubstitutionError> {
    let mut out = String::with_capacity(command.len());
    let mut rest = command;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after_open = &rest[open..];
        let Some(close) = after_open.find(PLACEHOLDER_CLOSE) else {
            out.push_str(after_open);
            return Ok(out);
        };
        let placeholder = &after_open[..=close];
        if placeholder == FACTION_PLACEHOLDER {
            out.push_str(&faction.to_string());
        } else if let Some(index_text) = placeholder
            .strip_prefix(OWN_BAND_PLACEHOLDER_PREFIX)
            .and_then(|tail| tail.strip_suffix(PLACEHOLDER_CLOSE))
        {
            let index: usize = index_text.parse().map_err(|_| SubstitutionError::Unknown {
                placeholder: placeholder.to_owned(),
            })?;
            let own: Vec<u64> = view
                .snapshot
                .populations
                .iter()
                .filter(|cohort| cohort.faction == faction)
                .map(|cohort| cohort.band_id)
                .collect();
            let band_id = own.get(index).ok_or(SubstitutionError::NoSuchOwnBand {
                index,
                own: own.len(),
            })?;
            out.push_str(&band_id.to_string());
        } else {
            return Err(SubstitutionError::Unknown {
                placeholder: placeholder.to_owned(),
            });
        }
        rest = &after_open[close + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use sim_runtime::{PopulationCohortState, WorldSnapshot};

    const OUR_FACTION: u32 = 1;
    const OTHER_FACTION: u32 = 0;
    const OUR_FIRST_BAND: u64 = 7001;
    const OUR_SECOND_BAND: u64 = 7002;
    const THEIR_BAND: u64 = 9001;
    const A_TICK: u64 = 12;

    fn a_view_at(tick: u64) -> SeatView {
        let mut snapshot = WorldSnapshot::default();
        snapshot.header.tick = tick;
        for (faction, band_id) in [
            (OTHER_FACTION, THEIR_BAND),
            (OUR_FACTION, OUR_FIRST_BAND),
            (OUR_FACTION, OUR_SECOND_BAND),
        ] {
            snapshot.populations.push(PopulationCohortState {
                faction,
                band_id,
                ..Default::default()
            });
        }
        SeatView {
            snapshot,
            last_acted_tick: None,
        }
    }

    fn rng() -> StdRng {
        StdRng::seed_from_u64(0)
    }

    #[test]
    fn the_pass_brain_submits_nothing() {
        assert!(PassBrain.decide(&a_view_at(A_TICK), &mut rng()).is_empty());
    }

    #[test]
    fn comments_blank_lines_and_both_tick_forms_parse() {
        let lines = parse_script(
            "# a comment\n\n12: split_band {faction} {own_band:0} 4\n  +2 :  ready 1  \n",
        )
        .expect("parses");
        assert_eq!(
            lines,
            vec![
                ScriptLine {
                    at: FireAt::Tick(12),
                    command: "split_band {faction} {own_band:0} 4".to_owned()
                },
                ScriptLine {
                    at: FireAt::AfterFirst(2),
                    command: "ready 1".to_owned()
                },
            ]
        );
        assert!(matches!(
            parse_script("split_band 1 2 3"),
            Err(ScriptError::MissingTick { line: 1 })
        ));
        assert!(matches!(
            parse_script("x: split_band 1 2 3"),
            Err(ScriptError::InvalidTick { line: 1, .. })
        ));
        assert!(matches!(
            parse_script("3:   "),
            Err(ScriptError::EmptyCommand { line: 1 })
        ));
    }

    #[test]
    fn a_line_fires_on_its_tick_with_its_placeholders_resolved_and_only_once() {
        let mut brain = ScriptedBrain::from_script(
            &format!("{A_TICK}: split_band {{faction}} {{own_band:1}} 4"),
            OUR_FACTION,
        )
        .expect("parses");
        assert!(brain.decide(&a_view_at(A_TICK - 1), &mut rng()).is_empty());
        assert_eq!(
            brain.decide(&a_view_at(A_TICK), &mut rng()),
            vec![CommandPayload::SplitBand {
                faction_id: OUR_FACTION,
                band_id: Some(OUR_SECOND_BAND),
                workers: 4,
            }]
        );
        assert!(brain.decide(&a_view_at(A_TICK + 1), &mut rng()).is_empty());
    }

    #[test]
    fn a_relative_tick_counts_from_the_first_tick_the_brain_saw() {
        let mut brain =
            ScriptedBrain::from_script("+1: split_band {faction} {own_band:0} 4", OUR_FACTION)
                .expect("parses");
        assert!(brain.decide(&a_view_at(A_TICK), &mut rng()).is_empty());
        assert_eq!(brain.decide(&a_view_at(A_TICK + 1), &mut rng()).len(), 1);
    }

    #[test]
    fn an_unresolvable_band_or_unknown_placeholder_skips_the_line() {
        let mut brain = ScriptedBrain::from_script(
            &format!(
                "{A_TICK}: split_band {{faction}} {{own_band:5}} 4\n{A_TICK}: split_band {{faction}} {{nope}} 4"
            ),
            OUR_FACTION,
        )
        .expect("parses");
        assert!(brain.decide(&a_view_at(A_TICK), &mut rng()).is_empty());
    }
}
