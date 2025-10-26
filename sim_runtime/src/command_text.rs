use std::num::{ParseFloatError, ParseIntError};

use thiserror::Error;

use crate::{
    CommandPayload, CorruptionSubsystem, InfluenceScopeKind, OrdersDirective, SupportChannel,
};

#[derive(Debug, Error)]
pub enum CommandParseError {
    #[error("empty command")]
    Empty,
    #[error("unknown command: {0}")]
    UnknownCommand(String),
    #[error("missing argument: {0}")]
    MissingArgument(&'static str),
    #[error("invalid integer '{value}' for {context}: {source}")]
    InvalidInteger {
        value: String,
        context: &'static str,
        source: ParseIntError,
    },
    #[error("invalid float '{value}' for {context}: {source}")]
    InvalidFloat {
        value: String,
        context: &'static str,
        source: ParseFloatError,
    },
    #[error("invalid support channel '{0}'")]
    InvalidSupportChannel(String),
    #[error("invalid influence scope '{0}'")]
    InvalidScope(String),
    #[error("invalid corruption subsystem '{0}'")]
    InvalidSubsystem(String),
    #[error("invalid orders directive '{0}'")]
    InvalidDirective(String),
}

pub fn parse_command_line(input: &str) -> Result<CommandPayload, CommandParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(CommandParseError::Empty);
    }

    let mut parts = trimmed.split_whitespace();
    let verb = parts
        .next()
        .map(|v| v.to_ascii_lowercase())
        .ok_or(CommandParseError::Empty)?;

    match verb.as_str() {
        "turn" => {
            let steps_str = parts.next().unwrap_or("1");
            let steps = parse_u32(steps_str, "turn steps")?;
            Ok(CommandPayload::Turn { steps })
        }
        "map_size" => {
            let width_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("width"))?;
            let height_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("height"))?;
            let width = parse_u32(width_str, "map width")?;
            let height = parse_u32(height_str, "map height")?;
            Ok(CommandPayload::ResetMap { width, height })
        }
        "heat" => {
            let entity_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("entity"))?;
            let delta_str = parts.next().unwrap_or("100000");
            let entity = parse_u64(entity_str, "heat entity")?;
            let delta = parse_i64(delta_str, "heat delta")?;
            Ok(CommandPayload::Heat {
                entity_bits: entity,
                delta,
            })
        }
        "order" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction"))?;
            let directive_str = parts.next().unwrap_or("ready").to_ascii_lowercase();
            let faction = parse_u32(faction_str, "order faction")?;
            let directive = match directive_str.as_str() {
                "ready" | "end" | "commit" => OrdersDirective::Ready,
                other => {
                    return Err(CommandParseError::InvalidDirective(other.to_string()));
                }
            };
            Ok(CommandPayload::Orders {
                faction_id: faction,
                directive,
            })
        }
        "rollback" => {
            let tick_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("tick"))?;
            let tick = parse_u64(tick_str, "rollback tick")?;
            Ok(CommandPayload::Rollback { tick })
        }
        "bias" => {
            let axis_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("axis"))?;
            let value_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("value"))?;
            let axis = parse_u32(axis_str, "bias axis")?;
            let value = parse_f32(value_str, "bias value")?;
            Ok(CommandPayload::AxisBias { axis, value })
        }
        "support" => {
            let id_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("id"))?;
            let magnitude_str = parts.next().unwrap_or("1.0");
            let id = parse_u32(id_str, "support id")?;
            let magnitude = parse_f32(magnitude_str, "support magnitude")?;
            Ok(CommandPayload::SupportInfluencer { id, magnitude })
        }
        "suppress" => {
            let id_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("id"))?;
            let magnitude_str = parts.next().unwrap_or("1.0");
            let id = parse_u32(id_str, "suppress id")?;
            let magnitude = parse_f32(magnitude_str, "suppress magnitude")?;
            Ok(CommandPayload::SuppressInfluencer { id, magnitude })
        }
        "support_channel" => {
            let id_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("id"))?;
            let channel_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("channel"))?;
            let magnitude_str = parts.next().unwrap_or("1.0");
            let id = parse_u32(id_str, "support_channel id")?;
            let channel = parse_support_channel(channel_str)?;
            let magnitude = parse_f32(magnitude_str, "support_channel magnitude")?;
            Ok(CommandPayload::SupportInfluencerChannel {
                id,
                channel,
                magnitude,
            })
        }
        "spawn_influencer" => {
            let mut scope: Option<InfluenceScopeKind> = None;
            let mut generation: Option<u16> = None;
            if let Some(token) = parts.next() {
                let token_lower = token.to_ascii_lowercase();
                match token_lower.as_str() {
                    "local" => scope = Some(InfluenceScopeKind::Local),
                    "regional" => scope = Some(InfluenceScopeKind::Regional),
                    "global" => scope = Some(InfluenceScopeKind::Global),
                    "generation" | "gen" => {
                        scope = Some(InfluenceScopeKind::Generation);
                        if let Some(gen_token) = parts.next() {
                            generation = Some(parse_u16(gen_token, "generation id")?);
                        }
                    }
                    other => {
                        if let Ok(value) = other.parse::<u16>() {
                            scope = Some(InfluenceScopeKind::Generation);
                            generation = Some(value);
                        } else {
                            return Err(CommandParseError::InvalidScope(other.to_string()));
                        }
                    }
                }
            }
            Ok(CommandPayload::SpawnInfluencer { scope, generation })
        }
        "corruption" => {
            let subsystem_str = parts.next().unwrap_or("logistics").to_ascii_lowercase();
            let subsystem = parse_corruption_subsystem(&subsystem_str)?;
            let intensity_str = parts.next().unwrap_or("0.25");
            let exposure_str = parts.next().unwrap_or("3");
            let intensity = parse_f32(intensity_str, "corruption intensity")?;
            let exposure_timer = parse_u32(exposure_str, "corruption exposure")?;
            Ok(CommandPayload::InjectCorruption {
                subsystem,
                intensity,
                exposure_timer,
            })
        }
        other => Err(CommandParseError::UnknownCommand(other.to_string())),
    }
}

fn parse_u32(value: &str, context: &'static str) -> Result<u32, CommandParseError> {
    value
        .parse::<u32>()
        .map_err(|source| CommandParseError::InvalidInteger {
            value: value.to_string(),
            context,
            source,
        })
}

fn parse_u16(value: &str, context: &'static str) -> Result<u16, CommandParseError> {
    value
        .parse::<u16>()
        .map_err(|source| CommandParseError::InvalidInteger {
            value: value.to_string(),
            context,
            source,
        })
}

fn parse_u64(value: &str, context: &'static str) -> Result<u64, CommandParseError> {
    value
        .parse::<u64>()
        .map_err(|source| CommandParseError::InvalidInteger {
            value: value.to_string(),
            context,
            source,
        })
}

fn parse_i64(value: &str, context: &'static str) -> Result<i64, CommandParseError> {
    value
        .parse::<i64>()
        .map_err(|source| CommandParseError::InvalidInteger {
            value: value.to_string(),
            context,
            source,
        })
}

fn parse_f32(value: &str, context: &'static str) -> Result<f32, CommandParseError> {
    value
        .parse::<f32>()
        .map_err(|source| CommandParseError::InvalidFloat {
            value: value.to_string(),
            context,
            source,
        })
}

fn parse_support_channel(token: &str) -> Result<SupportChannel, CommandParseError> {
    match token.to_ascii_lowercase().as_str() {
        "popular" | "pop" | "mass" => Ok(SupportChannel::Popular),
        "peer" | "prestige" | "research" => Ok(SupportChannel::Peer),
        "institutional" | "institution" | "industrial" | "inst" => {
            Ok(SupportChannel::Institutional)
        }
        "humanitarian" | "hum" | "civic" => Ok(SupportChannel::Humanitarian),
        other => Err(CommandParseError::InvalidSupportChannel(other.to_string())),
    }
}

fn parse_corruption_subsystem(token: &str) -> Result<CorruptionSubsystem, CommandParseError> {
    match token {
        "logistics" | "log" | "supply" => Ok(CorruptionSubsystem::Logistics),
        "trade" | "smuggling" | "commerce" => Ok(CorruptionSubsystem::Trade),
        "military" | "procurement" | "army" => Ok(CorruptionSubsystem::Military),
        "governance" | "bureaucracy" | "civic" => Ok(CorruptionSubsystem::Governance),
        other => Err(CommandParseError::InvalidSubsystem(other.to_string())),
    }
}
