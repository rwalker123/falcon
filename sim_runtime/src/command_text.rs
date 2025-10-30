use std::num::{ParseFloatError, ParseIntError};

use thiserror::Error;

/// Describes a runtime command verb, its aliases, and usage hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandVerbHelp {
    pub verb: &'static str,
    pub aliases: &'static [&'static str],
    pub summary: &'static str,
    pub usage: &'static str,
}

/// Canonical list of supported runtime command verbs.
pub const COMMAND_VERBS: &[CommandVerbHelp] = &[
    CommandVerbHelp {
        verb: "turn",
        aliases: &[],
        summary: "Advance the simulation by one or more turns (default 1).",
        usage: "turn [steps]",
    },
    CommandVerbHelp {
        verb: "map_size",
        aliases: &[],
        summary: "Resize the active map grid and emit a fresh snapshot.",
        usage: "map_size <width> <height>",
    },
    CommandVerbHelp {
        verb: "heat",
        aliases: &[],
        summary: "Adjust an entity's heat budget by the provided delta (default 100000).",
        usage: "heat <entity_bits> [delta]",
    },
    CommandVerbHelp {
        verb: "order",
        aliases: &[],
        summary: "Submit orders for a faction (currently only 'ready').",
        usage: "order <faction_id> [ready]",
    },
    CommandVerbHelp {
        verb: "rollback",
        aliases: &[],
        summary: "Rollback the simulation to a specific tick.",
        usage: "rollback <tick>",
    },
    CommandVerbHelp {
        verb: "bias",
        aliases: &[],
        summary: "Override an axis bias with a floating-point value.",
        usage: "bias <axis> <value>",
    },
    CommandVerbHelp {
        verb: "support",
        aliases: &[],
        summary: "Add support for an influencer by id (default magnitude 1.0).",
        usage: "support <id> [magnitude]",
    },
    CommandVerbHelp {
        verb: "suppress",
        aliases: &[],
        summary: "Suppress support for an influencer by id (default magnitude 1.0).",
        usage: "suppress <id> [magnitude]",
    },
    CommandVerbHelp {
        verb: "support_channel",
        aliases: &[],
        summary: "Boost an influencer's specific support channel.",
        usage: "support_channel <id> <channel> [magnitude]",
    },
    CommandVerbHelp {
        verb: "spawn_influencer",
        aliases: &[],
        summary: "Spawn a new influencer with optional scope or generation id.",
        usage: "spawn_influencer [local|regional|global|generation [id]]",
    },
    CommandVerbHelp {
        verb: "counterintel_policy",
        aliases: &[],
        summary: "Set the counter-intelligence policy for a faction.",
        usage: "counterintel_policy <faction_id> <lenient|standard|hardened|crisis>",
    },
    CommandVerbHelp {
        verb: "counterintel_budget",
        aliases: &[],
        summary: "Adjust or set the counter-intel reserve for a faction.",
        usage: "counterintel_budget <faction_id> [reserve <value>|delta <value>|<value>]",
    },
    CommandVerbHelp {
        verb: "queue_espionage_mission",
        aliases: &["queue_mission"],
        summary: "Queue an espionage mission with owner/target metadata.",
        usage: "queue_espionage_mission <mission_id> owner <id> target <id> discovery <id> agent <handle> [tier <value>] [tick <value>]",
    },
    CommandVerbHelp {
        verb: "corruption",
        aliases: &[],
        summary: "Inject corruption into a subsystem with optional intensity/exposure.",
        usage: "corruption [logistics|trade|military|governance] [intensity] [exposure_ticks]",
    },
    CommandVerbHelp {
        verb: "reload_config",
        aliases: &["reload_sim_config"],
        summary: "Reload simulation or pipeline configuration from disk.",
        usage: "reload_config [simulation|turn_pipeline|crisis_archetypes|crisis_modifiers|crisis_telemetry|snapshot_overlays] [path]",
    },
    CommandVerbHelp {
        verb: "crisis_autoseed",
        aliases: &["crisis_auto_seed"],
        summary: "Toggle automatic crisis seeding on or off.",
        usage: "crisis_autoseed [on|off]",
    },
    CommandVerbHelp {
        verb: "spawn_crisis",
        aliases: &[],
        summary: "Spawn a crisis by archetype for the specified faction (default 0).",
        usage: "spawn_crisis <archetype_id> [faction_id]",
    },
];

use crate::{
    CommandPayload, CorruptionSubsystem, InfluenceScopeKind, OrdersDirective, ReloadConfigKind,
    SecurityPolicyKind, SupportChannel,
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
    #[error("invalid boolean '{value}' for {context}")]
    InvalidBoolean {
        value: String,
        context: &'static str,
    },
    #[error("invalid support channel '{0}'")]
    InvalidSupportChannel(String),
    #[error("invalid influence scope '{0}'")]
    InvalidScope(String),
    #[error("invalid corruption subsystem '{0}'")]
    InvalidSubsystem(String),
    #[error("invalid orders directive '{0}'")]
    InvalidDirective(String),
    #[error("invalid security policy '{0}'")]
    InvalidSecurityPolicy(String),
    #[error("unexpected token '{0}'")]
    UnexpectedToken(String),
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
        "counterintel_policy" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction"))?;
            let policy_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("policy"))?;
            let faction = parse_u32(faction_str, "counterintel policy faction")?;
            let policy = parse_security_policy(policy_str)?;
            Ok(CommandPayload::UpdateCounterIntelPolicy { faction, policy })
        }
        "counterintel_budget" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction"))?;
            let faction = parse_u32(faction_str, "counterintel budget faction")?;
            let token = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("value"))?;

            let mut reserve: Option<f32> = None;
            let mut delta: Option<f32> = None;
            match token.to_ascii_lowercase().as_str() {
                "reserve" | "set" => {
                    let value_str = parts
                        .next()
                        .ok_or(CommandParseError::MissingArgument("reserve value"))?;
                    reserve = Some(parse_f32(value_str, "counterintel reserve")?);
                }
                "delta" | "adjust" => {
                    let value_str = parts
                        .next()
                        .ok_or(CommandParseError::MissingArgument("delta value"))?;
                    delta = Some(parse_f32(value_str, "counterintel delta")?);
                }
                other => {
                    reserve = Some(parse_f32(other, "counterintel reserve")?);
                }
            }

            if reserve.is_none() && delta.is_none() {
                return Err(CommandParseError::MissingArgument("reserve or delta"));
            }

            Ok(CommandPayload::AdjustCounterIntelBudget {
                faction,
                reserve,
                delta,
            })
        }
        "queue_espionage_mission" | "queue_mission" => {
            let mission_id = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("mission_id"))?
                .to_string();

            let mut owner: Option<u32> = None;
            let mut target_owner: Option<u32> = None;
            let mut discovery_id: Option<u32> = None;
            let mut agent_handle: Option<u32> = None;
            let mut target_tier: Option<u8> = None;
            let mut scheduled_tick: Option<u64> = None;

            while let Some(token) = parts.next() {
                match token.to_ascii_lowercase().as_str() {
                    "owner" | "owner_faction" => {
                        let value = parts
                            .next()
                            .ok_or(CommandParseError::MissingArgument("owner faction"))?;
                        owner = Some(parse_u32(value, "mission owner faction")?);
                    }
                    "target" | "target_owner" | "target_faction" => {
                        let value = parts
                            .next()
                            .ok_or(CommandParseError::MissingArgument("target faction"))?;
                        target_owner = Some(parse_u32(value, "mission target faction")?);
                    }
                    "discovery" | "discovery_id" => {
                        let value = parts
                            .next()
                            .ok_or(CommandParseError::MissingArgument("discovery id"))?;
                        discovery_id = Some(parse_u32(value, "mission discovery id")?);
                    }
                    "agent" | "agent_handle" => {
                        let value = parts
                            .next()
                            .ok_or(CommandParseError::MissingArgument("agent handle"))?;
                        if value.eq_ignore_ascii_case("auto") {
                            agent_handle = Some(u32::MAX);
                        } else {
                            agent_handle = Some(parse_u32(value, "mission agent handle")?);
                        }
                    }
                    "tier" | "target_tier" => {
                        let value = parts
                            .next()
                            .ok_or(CommandParseError::MissingArgument("target tier"))?;
                        target_tier = Some(parse_u8(value, "mission target tier")?);
                    }
                    "tick" | "scheduled" | "scheduled_tick" => {
                        let value = parts
                            .next()
                            .ok_or(CommandParseError::MissingArgument("scheduled tick"))?;
                        scheduled_tick = Some(parse_u64(value, "mission scheduled tick")?);
                    }
                    other => {
                        return Err(CommandParseError::UnexpectedToken(other.to_string()));
                    }
                }
            }

            let owner_faction = owner.ok_or(CommandParseError::MissingArgument("owner faction"))?;
            let target_owner_faction =
                target_owner.ok_or(CommandParseError::MissingArgument("target faction"))?;
            let discovery_id =
                discovery_id.ok_or(CommandParseError::MissingArgument("discovery id"))?;
            let agent_handle =
                agent_handle.ok_or(CommandParseError::MissingArgument("agent handle"))?;

            Ok(CommandPayload::QueueEspionageMission {
                mission_id,
                owner_faction,
                target_owner_faction,
                discovery_id,
                agent_handle,
                target_tier,
                scheduled_tick,
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
        "reload_config" | "reload_sim_config" => {
            let mut tokens: Vec<String> = parts.map(|p| p.to_string()).collect();
            let mut kind = ReloadConfigKind::Simulation;
            if let Some(first) = tokens.first() {
                match first.to_ascii_lowercase().as_str() {
                    "sim" | "simulation" | "sim_config" => {
                        tokens.remove(0);
                        kind = ReloadConfigKind::Simulation;
                    }
                    "pipeline" | "turn" | "turn_pipeline" | "phase" => {
                        tokens.remove(0);
                        kind = ReloadConfigKind::TurnPipeline;
                    }
                    "crisis_archetypes" | "crisis_catalog" | "crisis_archetype" => {
                        tokens.remove(0);
                        kind = ReloadConfigKind::CrisisArchetypes;
                    }
                    "crisis_modifiers" | "crisis_modifier" | "crisis_mod" => {
                        tokens.remove(0);
                        kind = ReloadConfigKind::CrisisModifiers;
                    }
                    "crisis_telemetry" | "crisis_telemetry_config" | "crisis_metrics" => {
                        tokens.remove(0);
                        kind = ReloadConfigKind::CrisisTelemetry;
                    }
                    "overlay" | "overlays" | "snapshot" | "snapshot_overlays" => {
                        tokens.remove(0);
                        kind = ReloadConfigKind::SnapshotOverlays;
                    }
                    _ => {}
                }
            }
            let path = if tokens.is_empty() {
                None
            } else {
                Some(tokens.join(" "))
            };
            Ok(CommandPayload::ReloadConfig { kind, path })
        }
        "crisis_autoseed" | "crisis_auto_seed" => {
            let value_str = parts.next().unwrap_or("on");
            let enabled = parse_bool(value_str, "crisis auto-seed flag")?;
            Ok(CommandPayload::SetCrisisAutoSeed { enabled })
        }
        "spawn_crisis" => {
            let archetype_id = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("archetype_id"))?;
            let faction_str = parts.next().unwrap_or("0");
            let faction_id = parse_u32(faction_str, "crisis faction")?;
            Ok(CommandPayload::SpawnCrisis {
                faction_id,
                archetype_id: archetype_id.to_string(),
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

fn parse_u8(value: &str, context: &'static str) -> Result<u8, CommandParseError> {
    value
        .parse::<u8>()
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

fn parse_bool(value: &str, context: &'static str) -> Result<bool, CommandParseError> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "t" | "yes" | "y" | "1" | "on" => Ok(true),
        "false" | "f" | "no" | "n" | "0" | "off" => Ok(false),
        other => Err(CommandParseError::InvalidBoolean {
            value: other.to_string(),
            context,
        }),
    }
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

fn parse_security_policy(token: &str) -> Result<SecurityPolicyKind, CommandParseError> {
    match token.to_ascii_lowercase().as_str() {
        "lenient" | "light" | "open" => Ok(SecurityPolicyKind::Lenient),
        "standard" | "baseline" | "normal" => Ok(SecurityPolicyKind::Standard),
        "hardened" | "secure" | "fortified" => Ok(SecurityPolicyKind::Hardened),
        "crisis" | "panic" | "lockdown" => Ok(SecurityPolicyKind::Crisis),
        other => Err(CommandParseError::InvalidSecurityPolicy(other.to_string())),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_counterintel_policy_command() {
        let payload = parse_command_line("counterintel_policy 3 hardened").unwrap();
        assert_eq!(
            payload,
            CommandPayload::UpdateCounterIntelPolicy {
                faction: 3,
                policy: SecurityPolicyKind::Hardened,
            }
        );
    }

    #[test]
    fn parse_counterintel_budget_command() {
        let payload = parse_command_line("counterintel_budget 2 reserve 5.5").unwrap();
        assert_eq!(
            payload,
            CommandPayload::AdjustCounterIntelBudget {
                faction: 2,
                reserve: Some(5.5),
                delta: None,
            }
        );

        let delta_payload = parse_command_line("counterintel_budget 1 delta -1.25").unwrap();
        assert_eq!(
            delta_payload,
            CommandPayload::AdjustCounterIntelBudget {
                faction: 1,
                reserve: None,
                delta: Some(-1.25),
            }
        );
    }

    #[test]
    fn parse_queue_espionage_mission_command() {
        let payload = parse_command_line(
            "queue_espionage_mission probe_basic owner 1 target 2 discovery 17 agent 8 tier 2 tick 42",
        )
        .unwrap();
        assert_eq!(
            payload,
            CommandPayload::QueueEspionageMission {
                mission_id: "probe_basic".into(),
                owner_faction: 1,
                target_owner_faction: 2,
                discovery_id: 17,
                agent_handle: 8,
                target_tier: Some(2),
                scheduled_tick: Some(42),
            }
        );
    }

    #[test]
    fn parse_queue_espionage_mission_auto_agent() {
        let payload =
            parse_command_line("queue_mission sweep_auto owner 3 target 4 discovery 11 agent auto")
                .unwrap();
        assert_eq!(
            payload,
            CommandPayload::QueueEspionageMission {
                mission_id: "sweep_auto".into(),
                owner_faction: 3,
                target_owner_faction: 4,
                discovery_id: 11,
                agent_handle: u32::MAX,
                target_tier: None,
                scheduled_tick: None,
            }
        );
    }
}
