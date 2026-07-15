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
    CommandVerbHelp {
        verb: "start_profile",
        aliases: &["scenario"],
        summary: "Select the active start profile/scenario id.",
        usage: "start_profile <profile_id>",
    },
    CommandVerbHelp {
        verb: "scout",
        aliases: &[],
        summary: "Queue a scouting order targeting the specified tile.",
        usage: "scout <faction_id> <x> <y> [band_entity_bits]",
    },
    CommandVerbHelp {
        verb: "follow_herd",
        aliases: &[],
        summary: "Order a band to hunt a herd continuously, auto-hunting per policy each turn.",
        usage: "follow_herd <faction_id> <herd_id> [policy] [band_entity_bits]",
    },
    CommandVerbHelp {
        verb: "forage",
        aliases: &[],
        summary: "Harvest food from a tile using the specified module key.",
        usage: "forage <faction_id> <x> <y> <module_key> [band_entity_bits]",
    },
    CommandVerbHelp {
        verb: "hunt_game",
        aliases: &["hunt"],
        summary: "Hunt localized wild game at a tile.",
        usage: "hunt_game <faction_id> <x> <y> [band_entity_bits]",
    },
    CommandVerbHelp {
        verb: "hunt_fauna",
        aliases: &[],
        summary: "Order a band to pursue and hunt a fauna group (herd) by id.",
        usage: "hunt_fauna <faction_id> <herd_id> [band_entity_bits]",
    },
    CommandVerbHelp {
        verb: "domesticate",
        aliases: &[],
        summary: "Claim a tame-enough herd as domesticated livestock (needs husbandry progress from a Sustain hunt).",
        usage: "domesticate <faction_id> <herd_id>",
    },
    CommandVerbHelp {
        verb: "cultivate",
        aliases: &[],
        summary: "Set the Cultivate policy on the bands foraging a Thriving patch: an investment that pays a reduced yield while the crop is prepared, then a higher tended yield (needs Cultivation knowledge, earned by Sustain foraging).",
        usage: "cultivate <faction_id> <x> <y>",
    },
    CommandVerbHelp {
        verb: "corral",
        aliases: &[],
        summary: "Set the Corral policy on the bands hunting your domesticated herd at a tile: an investment that pays a reduced take while the pen is built, then pins the herd there (needs Herding knowledge, earned by Sustain hunting).",
        usage: "corral <faction_id> <x> <y>",
    },
    CommandVerbHelp {
        verb: "extend_pen",
        aliases: &[],
        summary: "Grow the fenced footprint of your built pen at a tile by one ring: the keeper works it off over ~25 turns at a reduced take, then the pen grazes more land (needs Herding, an owned penned herd, and room below the pen-radius max).",
        usage: "extend_pen <faction_id> <x> <y>",
    },
    CommandVerbHelp {
        verb: "cancel_order",
        aliases: &[],
        summary: "Clear all of a band's labor assignments and stop movement (fully idle).",
        usage: "cancel_order <faction_id> [band_entity_bits]",
    },
    CommandVerbHelp {
        verb: "assign_labor",
        aliases: &[],
        summary: "Set the worker count for one labor target on a band (0 unassigns; clamps to idle).",
        usage: "assign_labor <faction_id> <band> forage <x> <y> [policy] <workers> | hunt <herd_id> <policy> <workers> | scout <workers> | warrior <workers>",
    },
    CommandVerbHelp {
        verb: "move_band",
        aliases: &[],
        summary: "Travel a band toward a target tile at the band move rate.",
        usage: "move_band <faction_id> <band> <x> <y>",
    },
    CommandVerbHelp {
        verb: "send_expedition",
        aliases: &[],
        summary: "Outfit a detached scouting party (workers + provisions) and send it to a target.",
        usage: "send_expedition <faction_id> <band> <party_workers> <x> <y>",
    },
    CommandVerbHelp {
        verb: "recall_expedition",
        aliases: &[],
        summary: "Order an in-flight expedition home (folds workers + provisions back on arrival).",
        usage: "recall_expedition <faction_id> <expedition_entity_bits>",
    },
    CommandVerbHelp {
        verb: "send_hunt_expedition",
        aliases: &[],
        summary: "Outfit a detached hunting party that follows a herd, harvests food, and delivers it.",
        usage: "send_hunt_expedition <faction_id> <band> <party_workers> <fauna_id> [sustain|surplus|market|eradicate]",
    },
    CommandVerbHelp {
        verb: "export_map",
        aliases: &["export"],
        summary: "Write the current world map (terrain + seed) to a JSON file for inspection and tests.",
        usage: "export_map [path]",
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
        "start_profile" | "scenario" => {
            let profile_id = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("profile_id"))?;
            Ok(CommandPayload::SetStartProfile {
                profile_id: profile_id.to_string(),
            })
        }
        "scout" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let x_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("target_x"))?;
            let y_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("target_y"))?;
            let band_bits = match parts.next() {
                Some(raw) => Some(parse_u64(raw, "band_entity_bits")?),
                None => None,
            };
            Ok(CommandPayload::ScoutArea {
                faction_id: parse_u32(faction_str, "scout faction")?,
                target_x: parse_u32(x_str, "scout target_x")?,
                target_y: parse_u32(y_str, "scout target_y")?,
                band_entity_bits: band_bits,
            })
        }
        "follow_herd" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let herd_id = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("herd_id"))?;
            // Optional `[policy] [band_entity_bits]`. When both trail, 3rd = policy,
            // 4th = band. A lone 3rd token that is purely numeric is taken as the
            // band id (policy omitted) so `follow_herd <f> <herd> <band>` works —
            // mirroring `hunt_fauna`'s numeric band arg; policy words are never numeric.
            let third = parts.next();
            let fourth = parts.next();
            let (policy, band_bits) = match (third, fourth) {
                (Some(p), Some(b)) => (
                    Some(p.to_string()),
                    Some(parse_u64(b, "follow_herd band_entity_bits")?),
                ),
                (Some(tok), None) => match tok.parse::<u64>() {
                    Ok(bits) => (None, Some(bits)),
                    Err(_) => (Some(tok.to_string()), None),
                },
                (None, _) => (None, None),
            };
            Ok(CommandPayload::FollowHerd {
                faction_id: parse_u32(faction_str, "follow_herd faction")?,
                herd_id: herd_id.to_string(),
                policy,
                band_entity_bits: band_bits,
            })
        }
        "found_settlement" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let x_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("target_x"))?;
            let y_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("target_y"))?;
            Ok(CommandPayload::FoundSettlement {
                faction_id: parse_u32(faction_str, "found_settlement faction")?,
                target_x: parse_u32(x_str, "found_settlement target_x")?,
                target_y: parse_u32(y_str, "found_settlement target_y")?,
            })
        }
        "forage" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let x_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("target_x"))?;
            let y_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("target_y"))?;
            let module_key = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("module_key"))?;
            let band_bits = parts.next();
            Ok(CommandPayload::ForageTile {
                faction_id: parse_u32(faction_str, "forage faction")?,
                target_x: parse_u32(x_str, "forage target_x")?,
                target_y: parse_u32(y_str, "forage target_y")?,
                module: module_key.to_ascii_lowercase(),
                band_entity_bits: match band_bits {
                    Some(raw) => Some(parse_u64(raw, "forage band_entity_bits")?),
                    None => None,
                },
            })
        }
        "hunt" | "hunt_game" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let x_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("target_x"))?;
            let y_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("target_y"))?;
            let band_bits = parts.next();
            Ok(CommandPayload::HuntGame {
                faction_id: parse_u32(faction_str, "hunt_game faction")?,
                target_x: parse_u32(x_str, "hunt_game target_x")?,
                target_y: parse_u32(y_str, "hunt_game target_y")?,
                band_entity_bits: match band_bits {
                    Some(raw) => Some(parse_u64(raw, "hunt band_entity_bits")?),
                    None => None,
                },
            })
        }
        "hunt_fauna" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let herd_id = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("herd_id"))?;
            let band_bits = parts.next();
            Ok(CommandPayload::HuntFauna {
                faction_id: parse_u32(faction_str, "hunt_fauna faction")?,
                herd_id: herd_id.to_string(),
                band_entity_bits: match band_bits {
                    Some(raw) => Some(parse_u64(raw, "hunt_fauna band_entity_bits")?),
                    None => None,
                },
            })
        }
        "domesticate" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let herd_id = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("herd_id"))?;
            Ok(CommandPayload::Domesticate {
                faction_id: parse_u32(faction_str, "domesticate faction")?,
                herd_id: herd_id.to_string(),
            })
        }
        "cultivate" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let x_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("target_x"))?;
            let y_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("target_y"))?;
            Ok(CommandPayload::Cultivate {
                faction_id: parse_u32(faction_str, "cultivate faction")?,
                target_x: parse_u32(x_str, "cultivate target_x")?,
                target_y: parse_u32(y_str, "cultivate target_y")?,
            })
        }
        "corral" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let x_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("target_x"))?;
            let y_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("target_y"))?;
            Ok(CommandPayload::Corral {
                faction_id: parse_u32(faction_str, "corral faction")?,
                target_x: parse_u32(x_str, "corral target_x")?,
                target_y: parse_u32(y_str, "corral target_y")?,
            })
        }
        "extend_pen" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let x_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("target_x"))?;
            let y_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("target_y"))?;
            Ok(CommandPayload::ExtendPen {
                faction_id: parse_u32(faction_str, "extend_pen faction")?,
                target_x: parse_u32(x_str, "extend_pen target_x")?,
                target_y: parse_u32(y_str, "extend_pen target_y")?,
            })
        }
        "cancel_order" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let band_bits = parts.next();
            Ok(CommandPayload::CancelOrder {
                faction_id: parse_u32(faction_str, "cancel_order faction")?,
                band_entity_bits: match band_bits {
                    Some(raw) => Some(parse_u64(raw, "cancel_order band_entity_bits")?),
                    None => None,
                },
            })
        }
        "assign_labor" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let band_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("band_entity_bits"))?;
            let role = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("role"))?
                .to_ascii_lowercase();
            let faction_id = parse_u32(faction_str, "assign_labor faction")?;
            let band = parse_u64(band_str, "assign_labor band_entity_bits")?;
            let (workers, target_x, target_y, fauna_id, policy) = match role.as_str() {
                "forage" => {
                    let x = parts
                        .next()
                        .ok_or(CommandParseError::MissingArgument("target_x"))?;
                    let y = parts
                        .next()
                        .ok_or(CommandParseError::MissingArgument("target_y"))?;
                    // Optional policy token: `forage <x> <y> [policy] <workers>` (parity with the
                    // hunt arm's policy). If a token follows the first, the first is the policy and
                    // the second is the worker count; otherwise the lone token is the worker count.
                    let first = parts
                        .next()
                        .ok_or(CommandParseError::MissingArgument("workers"))?;
                    let (policy_tok, workers_tok) = match parts.next() {
                        Some(w) => (Some(first.to_string()), w),
                        None => (None, first),
                    };
                    (
                        parse_u32(workers_tok, "assign_labor workers")?,
                        Some(parse_u32(x, "assign_labor target_x")?),
                        Some(parse_u32(y, "assign_labor target_y")?),
                        None,
                        policy_tok,
                    )
                }
                "hunt" => {
                    let herd = parts
                        .next()
                        .ok_or(CommandParseError::MissingArgument("herd_id"))?;
                    let pol = parts
                        .next()
                        .ok_or(CommandParseError::MissingArgument("policy"))?;
                    let w = parts
                        .next()
                        .ok_or(CommandParseError::MissingArgument("workers"))?;
                    (
                        parse_u32(w, "assign_labor workers")?,
                        None,
                        None,
                        Some(herd.to_string()),
                        Some(pol.to_string()),
                    )
                }
                "scout" | "warrior" => {
                    let w = parts
                        .next()
                        .ok_or(CommandParseError::MissingArgument("workers"))?;
                    (
                        parse_u32(w, "assign_labor workers")?,
                        None,
                        None,
                        None,
                        None,
                    )
                }
                _ => return Err(CommandParseError::UnexpectedToken(role)),
            };
            Ok(CommandPayload::AssignLabor {
                faction_id,
                band_entity_bits: Some(band),
                role,
                workers,
                target_x,
                target_y,
                fauna_id,
                policy,
            })
        }
        "move_band" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let band_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("band_entity_bits"))?;
            let x_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("target_x"))?;
            let y_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("target_y"))?;
            Ok(CommandPayload::MoveBand {
                faction_id: parse_u32(faction_str, "move_band faction")?,
                band_entity_bits: Some(parse_u64(band_str, "move_band band_entity_bits")?),
                target_x: parse_u32(x_str, "move_band target_x")?,
                target_y: parse_u32(y_str, "move_band target_y")?,
            })
        }
        "send_expedition" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let band_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("band_entity_bits"))?;
            let workers_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("party_workers"))?;
            let x_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("target_x"))?;
            let y_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("target_y"))?;
            Ok(CommandPayload::SendExpedition {
                faction_id: parse_u32(faction_str, "send_expedition faction")?,
                band_entity_bits: Some(parse_u64(band_str, "send_expedition band_entity_bits")?),
                party_workers: parse_u32(workers_str, "send_expedition party_workers")?,
                target_x: parse_u32(x_str, "send_expedition target_x")?,
                target_y: parse_u32(y_str, "send_expedition target_y")?,
            })
        }
        "recall_expedition" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let expedition_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("expedition_entity_bits"))?;
            Ok(CommandPayload::RecallExpedition {
                faction_id: parse_u32(faction_str, "recall_expedition faction")?,
                expedition_entity_bits: parse_u64(
                    expedition_str,
                    "recall_expedition expedition_entity_bits",
                )?,
            })
        }
        "send_hunt_expedition" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let band_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("band_entity_bits"))?;
            let workers_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("party_workers"))?;
            let fauna_id = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("fauna_id"))?;
            // Optional trailing take policy (sustain|surplus|market|eradicate); default sustain.
            let policy = parts.next().map(|s| s.to_string());
            Ok(CommandPayload::SendHuntExpedition {
                faction_id: parse_u32(faction_str, "send_hunt_expedition faction")?,
                band_entity_bits: Some(parse_u64(
                    band_str,
                    "send_hunt_expedition band_entity_bits",
                )?),
                party_workers: parse_u32(workers_str, "send_hunt_expedition party_workers")?,
                fauna_id: fauna_id.to_string(),
                policy,
            })
        }
        "export" | "export_map" => {
            // Remaining tokens (if any) form the destination path; join so
            // paths containing spaces survive whitespace tokenization.
            let path: Vec<&str> = parts.collect();
            let path = if path.is_empty() {
                None
            } else {
                Some(path.join(" "))
            };
            Ok(CommandPayload::ExportMap { path })
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

fn parse_u8(value: &str, context: &'static str) -> Result<u8, CommandParseError> {
    value
        .parse::<u8>()
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
    fn parse_follow_herd_optional_args() {
        // Bare: no policy, no band.
        assert_eq!(
            parse_command_line("follow_herd 0 game_deer_07").unwrap(),
            CommandPayload::FollowHerd {
                faction_id: 0,
                herd_id: "game_deer_07".to_string(),
                policy: None,
                band_entity_bits: None,
            }
        );
        // Policy word only.
        assert_eq!(
            parse_command_line("follow_herd 0 game_deer_07 surplus").unwrap(),
            CommandPayload::FollowHerd {
                faction_id: 0,
                herd_id: "game_deer_07".to_string(),
                policy: Some("surplus".to_string()),
                band_entity_bits: None,
            }
        );
        // Lone numeric 3rd token = band id (policy omitted).
        assert_eq!(
            parse_command_line("follow_herd 0 game_deer_07 904").unwrap(),
            CommandPayload::FollowHerd {
                faction_id: 0,
                herd_id: "game_deer_07".to_string(),
                policy: None,
                band_entity_bits: Some(904),
            }
        );
        // Both: policy then band.
        assert_eq!(
            parse_command_line("follow_herd 0 game_deer_07 eradicate 904").unwrap(),
            CommandPayload::FollowHerd {
                faction_id: 0,
                herd_id: "game_deer_07".to_string(),
                policy: Some("eradicate".to_string()),
                band_entity_bits: Some(904),
            }
        );
    }

    #[test]
    fn parse_domesticate_command() {
        assert_eq!(
            parse_command_line("domesticate 0 game_deer_07").unwrap(),
            CommandPayload::Domesticate {
                faction_id: 0,
                herd_id: "game_deer_07".to_string(),
            }
        );
        // herd_id is required.
        assert!(matches!(
            parse_command_line("domesticate 0"),
            Err(CommandParseError::MissingArgument("herd_id"))
        ));
    }

    #[test]
    fn parse_cultivate_command() {
        assert_eq!(
            parse_command_line("cultivate 0 7 3").unwrap(),
            CommandPayload::Cultivate {
                faction_id: 0,
                target_x: 7,
                target_y: 3,
            }
        );
        // Both coordinates are required.
        assert!(matches!(
            parse_command_line("cultivate 0 7"),
            Err(CommandParseError::MissingArgument("target_y"))
        ));
    }

    #[test]
    fn parse_corral_command() {
        assert_eq!(
            parse_command_line("corral 0 7 3").unwrap(),
            CommandPayload::Corral {
                faction_id: 0,
                target_x: 7,
                target_y: 3,
            }
        );
        // Both coordinates are required.
        assert!(matches!(
            parse_command_line("corral 0 7"),
            Err(CommandParseError::MissingArgument("target_y"))
        ));
    }

    #[test]
    fn parse_extend_pen_command() {
        assert_eq!(
            parse_command_line("extend_pen 0 7 3").unwrap(),
            CommandPayload::ExtendPen {
                faction_id: 0,
                target_x: 7,
                target_y: 3,
            }
        );
        assert!(matches!(
            parse_command_line("extend_pen 0 7"),
            Err(CommandParseError::MissingArgument("target_y"))
        ));
    }

    #[test]
    fn parse_assign_labor_forage() {
        assert_eq!(
            parse_command_line("assign_labor 0 904 forage 3 5 6").unwrap(),
            CommandPayload::AssignLabor {
                faction_id: 0,
                band_entity_bits: Some(904),
                role: "forage".to_string(),
                workers: 6,
                target_x: Some(3),
                target_y: Some(5),
                fauna_id: None,
                policy: None,
            }
        );
    }

    #[test]
    fn parse_assign_labor_forage_with_policy() {
        // The optional policy token (§0-iii, parity with hunt): `forage <x> <y> <policy> <workers>`.
        for policy in ["sustain", "surplus", "market", "eradicate"] {
            assert_eq!(
                parse_command_line(&format!("assign_labor 0 904 forage 3 5 {policy} 6")).unwrap(),
                CommandPayload::AssignLabor {
                    faction_id: 0,
                    band_entity_bits: Some(904),
                    role: "forage".to_string(),
                    workers: 6,
                    target_x: Some(3),
                    target_y: Some(5),
                    fauna_id: None,
                    policy: Some(policy.to_string()),
                },
                "forage policy {policy} should round-trip"
            );
        }
    }

    #[test]
    fn parse_assign_labor_hunt_each_policy() {
        for policy in ["sustain", "surplus", "market", "eradicate"] {
            assert_eq!(
                parse_command_line(&format!("assign_labor 0 904 hunt game_deer_07 {policy} 4"))
                    .unwrap(),
                CommandPayload::AssignLabor {
                    faction_id: 0,
                    band_entity_bits: Some(904),
                    role: "hunt".to_string(),
                    workers: 4,
                    target_x: None,
                    target_y: None,
                    fauna_id: Some("game_deer_07".to_string()),
                    policy: Some(policy.to_string()),
                },
                "policy {policy} should round-trip"
            );
        }
    }

    #[test]
    fn parse_assign_labor_scout_and_warrior() {
        assert_eq!(
            parse_command_line("assign_labor 0 904 scout 5").unwrap(),
            CommandPayload::AssignLabor {
                faction_id: 0,
                band_entity_bits: Some(904),
                role: "scout".to_string(),
                workers: 5,
                target_x: None,
                target_y: None,
                fauna_id: None,
                policy: None,
            }
        );
        assert_eq!(
            parse_command_line("assign_labor 0 904 warrior 2").unwrap(),
            CommandPayload::AssignLabor {
                faction_id: 0,
                band_entity_bits: Some(904),
                role: "warrior".to_string(),
                workers: 2,
                target_x: None,
                target_y: None,
                fauna_id: None,
                policy: None,
            }
        );
    }

    #[test]
    fn parse_move_band_command() {
        assert_eq!(
            parse_command_line("move_band 0 904 12 7").unwrap(),
            CommandPayload::MoveBand {
                faction_id: 0,
                band_entity_bits: Some(904),
                target_x: 12,
                target_y: 7,
            }
        );
    }

    #[test]
    fn parse_assign_labor_and_move_band_rejects_malformed() {
        // Missing the trailing worker count on a forage assignment.
        assert!(matches!(
            parse_command_line("assign_labor 0 904 forage 3 5"),
            Err(CommandParseError::MissingArgument("workers"))
        ));
        // Missing the hunt policy token (herd present, nothing after).
        assert!(matches!(
            parse_command_line("assign_labor 0 904 hunt game_deer_07"),
            Err(CommandParseError::MissingArgument("policy"))
        ));
        // Unknown role → rejected, not a silent wrong payload.
        assert!(matches!(
            parse_command_line("assign_labor 0 904 fish 3"),
            Err(CommandParseError::UnexpectedToken(role)) if role == "fish"
        ));
        // Non-numeric worker count.
        assert!(matches!(
            parse_command_line("assign_labor 0 904 scout abc"),
            Err(CommandParseError::InvalidInteger { .. })
        ));
        // move_band missing the y coordinate.
        assert!(matches!(
            parse_command_line("move_band 0 904 12"),
            Err(CommandParseError::MissingArgument("target_y"))
        ));
    }

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
