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
        verb: "new_game",
        aliases: &[],
        summary: "Generate a world on demand (the server boots idle). seed 0 randomizes.",
        usage: "new_game <preset_id> <width> <height> <seed> <profile_id>",
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
        verb: "set_fog",
        aliases: &["fog"],
        summary: "Toggle fog of war on or off; off reveals every herd and the whole map.",
        usage: "set_fog [on|off]",
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
        usage: "scout <faction_id> <x> <y> [band_id]",
    },
    CommandVerbHelp {
        verb: "follow_herd",
        aliases: &[],
        summary: "Order a band to hunt a herd continuously, auto-hunting per policy each turn.",
        usage: "follow_herd <faction_id> <herd_id> [policy] [band_id]",
    },
    CommandVerbHelp {
        verb: "forage",
        aliases: &[],
        summary: "Harvest food from a tile using the specified module key.",
        usage: "forage <faction_id> <x> <y> <module_key> [band_id]",
    },
    CommandVerbHelp {
        verb: "hunt_game",
        aliases: &["hunt"],
        summary: "Hunt localized wild game at a tile.",
        usage: "hunt_game <faction_id> <x> <y> [band_id]",
    },
    CommandVerbHelp {
        verb: "hunt_fauna",
        aliases: &[],
        summary: "Order a band to pursue and hunt a fauna group (herd) by id.",
        usage: "hunt_fauna <faction_id> <herd_id> [band_id]",
    },
    CommandVerbHelp {
        verb: "tame",
        aliases: &[],
        summary: "Set the Tame improvement on the bands hunting a wild herd (their harvest stance is left alone): an investment that pays a reduced take while the herd is gentled, then makes it pastoral livestock (needs Herding knowledge, earned by Sustain hunting, and a species that can be domesticated).",
        usage: "tame <faction_id> <herd_id>",
    },
    CommandVerbHelp {
        verb: "cultivate",
        aliases: &[],
        summary: "Set the Cultivate improvement on the bands foraging a Thriving patch (their harvest stance is left alone): an investment that pays a reduced yield while the crop is prepared, then a higher tended yield (needs Cultivation knowledge, earned by Sustain foraging).",
        usage: "cultivate <faction_id> <x> <y>",
    },
    CommandVerbHelp {
        verb: "sow",
        aliases: &[],
        summary: "Set the Sow improvement on the bands foraging a tile (their harvest stance is left alone): an investment that builds a Field, out-yielding a tended patch. It PLACES the source — even ground with no forage site on it will take seed — but only where the land is ALREADY very fertile and near fresh water (the river valleys, ~1% of the map): rung 3 can carry seed, not water or fertilizer. Needs Seed Selection knowledge, earned by working tended patches.",
        usage: "sow <faction_id> <x> <y>",
    },
    CommandVerbHelp {
        verb: "abandon_improvement",
        aliases: &["abandon"],
        summary: "Abandon the improvement a band is building on a source: the crew keeps working it under the harvest stance you chose, and stops paying the build dip. Always allowed — abandoning a STALLED build is exactly when you want it. Accumulated progress is not zeroed; it is left to whatever the source does when nobody is improving it (a plant meter bleeds away, an animal meter is kept).",
        usage: "abandon_improvement <faction_id> forage <x> <y> | abandon_improvement <faction_id> hunt <herd_id>",
    },
    CommandVerbHelp {
        verb: "corral",
        aliases: &[],
        summary: "Set the Corral improvement on the bands hunting your domesticated herd at a tile (their harvest stance is left alone): an investment that pays a reduced take while the pen is built, then pins the herd there (needs Penning knowledge, earned by working herds you have already TAMED — Herding gates tame, not corral).",
        usage: "corral <faction_id> <x> <y>",
    },
    CommandVerbHelp {
        verb: "extend_pen",
        aliases: &[],
        summary: "Grow the fenced footprint of your built pen at a tile by one ring: the keeper works it off over ~25 turns at a reduced take, then the pen grazes more land (a ring rides the same rung as the pen, so it needs Penning too — plus an owned penned herd and room below the pen-radius max).",
        usage: "extend_pen <faction_id> <x> <y>",
    },
    CommandVerbHelp {
        verb: "answer_fork",
        aliases: &[],
        summary: "Answer a pending narrative fork (The Telling) with one of its choices; every fork offers an explicit defer.",
        usage: "answer_fork <faction_id> <beat_id> <choice_id>",
    },
    CommandVerbHelp {
        verb: "cancel_order",
        aliases: &[],
        summary: "Clear a band's labor assignments: scope 'work' unassigns worked sources (forage/hunt), 'roles' clears standing roles (scout/warrior), 'all' clears both and stops movement. Defaults to 'all'; the narrow scopes leave travel running. The two trailing tokens are optional and may be given in either order, but each may appear at most once — a repeated or extra token is rejected rather than guessed at.",
        usage: "cancel_order <faction_id> [band_id] [all|work|roles]",
    },
    CommandVerbHelp {
        verb: "assign_labor",
        aliases: &[],
        summary: "Set the worker count for one labor target on a band (0 unassigns; clamps to idle).",
        usage: "assign_labor <faction_id> <band> forage <x> <y> [floor] [species] <workers> [kit <id>] | hunt <herd_id> [floor] <workers> [kit <id>] | scout <workers> | warrior <workers>",
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
        usage: "send_expedition <faction_id> <band_id> <party_workers> <x> <y>",
    },
    CommandVerbHelp {
        verb: "recall_expedition",
        aliases: &[],
        summary: "Order an in-flight expedition home (folds workers + provisions back on arrival).",
        usage: "recall_expedition <faction_id> <expedition_band_id>",
    },
    CommandVerbHelp {
        verb: "split_band",
        aliases: &[],
        summary: "Form a new band — a resident band splits in two where it stands; `workers` is the \
                  only input and everything else divides on the share it implies.",
        usage: "split_band <faction_id> <band_id> <workers>",
    },
    CommandVerbHelp {
        verb: "send_hunt_expedition",
        aliases: &[],
        summary: "Outfit a detached hunting party that follows a herd, harvests food, and delivers it.",
        usage: "send_hunt_expedition <faction_id> <band_id> <party_workers> <fauna_id> [floor] \
                [kit <id>]",
    },
    CommandVerbHelp {
        verb: "send_denial_raid",
        aliases: &[],
        summary: "Outfit a detached party to erase a herd — no floor, near-zero return.",
        usage: "send_denial_raid <faction_id> <band_id> <party_workers> <fauna_id> [kit <id>]",
    },
    CommandVerbHelp {
        verb: "export_map",
        aliases: &["export"],
        summary: "Write the current world map (terrain + seed) to a JSON file for inspection and tests.",
        usage: "export_map [path]",
    },
    CommandVerbHelp {
        verb: "resync",
        aliases: &[],
        summary: "Republish the world as a full snapshot (delta-streaming recovery).",
        usage: "resync",
    },
    CommandVerbHelp {
        verb: "set_config_override",
        aliases: &[],
        summary: "Stage a sparse config patch, validated now and applied at the next new_game.",
        usage: "set_config_override <simulation|labor|demographics|expedition|combat> <json>",
    },
    CommandVerbHelp {
        verb: "clear_config_overrides",
        aliases: &[],
        summary: "Drop every staged config override; the next new_game uses the shipped configs.",
        usage: "clear_config_overrides",
    },
];

use crate::{
    CancelScope, CommandPayload, ConfigOverrideKind, OrdersDirective, ReloadConfigKind,
    SecurityPolicyKind,
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
    #[error(
        "'{0}' is a retired harvest stance — assign_labor takes an escapement floor now, a \
         fraction of carrying capacity in 0.0..=1.0 (0.5 is the sustainable peak, 0.0 takes \
         everything)"
    )]
    RetiredStanceToken(String),
    /// **A trailing token on a verb whose grammar is closed.** `send_denial_raid` is the case it
    /// exists for: the mission carries no floor and no fill target, so a fifth token is not a value
    /// to ignore but a misunderstanding of the verb (`docs/plan_denial_raid.md` §1), and a silent
    /// acceptance would teach the player that denial takes a number.
    #[error("unexpected argument: {0}")]
    UnexpectedArgument(String),
    #[error("invalid orders directive '{0}'")]
    InvalidDirective(String),
    #[error("invalid security policy '{0}'")]
    InvalidSecurityPolicy(String),
    #[error("invalid config override kind '{0}'")]
    InvalidConfigOverrideKind(String),
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
        "new_game" => {
            let preset_id = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("preset_id"))?;
            let width_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("width"))?;
            let height_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("height"))?;
            let seed_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("seed"))?;
            let profile_id = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("profile_id"))?;
            Ok(CommandPayload::NewGame {
                preset_id: preset_id.to_string(),
                width: parse_u32(width_str, "new_game width")?,
                height: parse_u32(height_str, "new_game height")?,
                seed: parse_u64(seed_str, "new_game seed")?,
                profile_id: profile_id.to_string(),
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
        "set_fog" | "fog" => {
            let value_str = parts.next().unwrap_or("on");
            let enabled = parse_bool(value_str, "fog of war flag")?;
            Ok(CommandPayload::SetFogEnabled { enabled })
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
                Some(raw) => Some(parse_u64(raw, "band_id")?),
                None => None,
            };
            Ok(CommandPayload::ScoutArea {
                faction_id: parse_u32(faction_str, "scout faction")?,
                target_x: parse_u32(x_str, "scout target_x")?,
                target_y: parse_u32(y_str, "scout target_y")?,
                band_id: band_bits,
            })
        }
        "follow_herd" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let herd_id = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("herd_id"))?;
            // Optional `[policy] [band_id]`. When both trail, 3rd = policy,
            // 4th = band. A lone 3rd token that is purely numeric is taken as the
            // band id (policy omitted) so `follow_herd <f> <herd> <band>` works —
            // mirroring `hunt_fauna`'s numeric band arg; policy words are never numeric.
            let third = parts.next();
            let fourth = parts.next();
            let (policy, band_bits) = match (third, fourth) {
                (Some(p), Some(b)) => (
                    Some(p.to_string()),
                    Some(parse_u64(b, "follow_herd band_id")?),
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
                band_id: band_bits,
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
                band_id: match band_bits {
                    Some(raw) => Some(parse_u64(raw, "forage band_id")?),
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
                band_id: match band_bits {
                    Some(raw) => Some(parse_u64(raw, "hunt band_id")?),
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
                band_id: match band_bits {
                    Some(raw) => Some(parse_u64(raw, "hunt_fauna band_id")?),
                    None => None,
                },
            })
        }
        "answer_fork" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let beat_id = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("beat_id"))?;
            let choice_id = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("choice_id"))?;
            Ok(CommandPayload::AnswerFork {
                faction_id: parse_u32(faction_str, "answer_fork faction")?,
                beat_id: beat_id.to_string(),
                choice_id: choice_id.to_string(),
            })
        }
        "tame" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let herd_id = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("herd_id"))?;
            Ok(CommandPayload::Tame {
                faction_id: parse_u32(faction_str, "tame faction")?,
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
        "sow" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let x_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("target_x"))?;
            let y_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("target_y"))?;
            Ok(CommandPayload::Sow {
                faction_id: parse_u32(faction_str, "sow faction")?,
                target_x: parse_u32(x_str, "sow target_x")?,
                target_y: parse_u32(y_str, "sow target_y")?,
            })
        }
        "abandon_improvement" | "abandon" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let faction_id = parse_u32(faction_str, "abandon_improvement faction")?;
            let kind = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("kind"))?
                .to_ascii_lowercase();
            // The source is named the way its web names it: a patch by tile, a herd by id — so the
            // `kind` decides the arity, and an unknown one has no arity to read. **It fails closed
            // here**, matching `assign_labor`'s identical `forage`/`hunt` grammar and `cancel_order`:
            // the server does reject an unknown kind, but only *asynchronously* in the feed, and a
            // catch-all forage arm turns a typo into either the wrong diagnosis
            // (`abandon_improvement 1 foo` reporting a missing `target_x`) or a command that parses
            // and is rejected somewhere else entirely.
            match kind.as_str() {
                "hunt" => {
                    let fauna_id = parts
                        .next()
                        .ok_or(CommandParseError::MissingArgument("fauna_id"))?;
                    Ok(CommandPayload::AbandonImprovement {
                        faction_id,
                        kind,
                        target_x: 0,
                        target_y: 0,
                        fauna_id: fauna_id.to_string(),
                    })
                }
                "forage" => {
                    let x_str = parts
                        .next()
                        .ok_or(CommandParseError::MissingArgument("target_x"))?;
                    let y_str = parts
                        .next()
                        .ok_or(CommandParseError::MissingArgument("target_y"))?;
                    Ok(CommandPayload::AbandonImprovement {
                        faction_id,
                        kind,
                        target_x: parse_u32(x_str, "abandon_improvement target_x")?,
                        target_y: parse_u32(y_str, "abandon_improvement target_y")?,
                        fauna_id: String::new(),
                    })
                }
                _ => Err(CommandParseError::UnexpectedToken(kind)),
            }
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
            // Both trailing tokens are optional and order-free, so the grammar is resolved by shape
            // rather than position: a numeric token fills the band slot, anything else must be a
            // scope word. Each slot may be filled at most once and no other token is tolerated — a
            // repeat or an extra is a typo, and this verb is destructive enough that guessing which
            // of `42 43` the player meant would clear the wrong band's assignments. An unrecognised
            // word fails closed for the same reason: silently falling back to `all` would
            // mass-unassign a band whose player asked only for `work`.
            let mut band_id = None;
            let mut scope = None;
            for token in parts.by_ref() {
                if token.chars().all(|c| c.is_ascii_digit()) {
                    if band_id.is_some() {
                        return Err(CommandParseError::UnexpectedToken(token.to_string()));
                    }
                    band_id = Some(parse_u64(token, "cancel_order band_id")?);
                } else {
                    if scope.is_some() {
                        return Err(CommandParseError::UnexpectedToken(token.to_string()));
                    }
                    scope =
                        Some(CancelScope::parse(token).ok_or_else(|| {
                            CommandParseError::UnexpectedToken(token.to_string())
                        })?);
                }
            }
            Ok(CommandPayload::CancelOrder {
                faction_id: parse_u32(faction_str, "cancel_order faction")?,
                band_id,
                scope: scope.unwrap_or(CancelScope::All),
            })
        }
        "assign_labor" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let band_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("band_id"))?;
            let role = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("role"))?
                .to_ascii_lowercase();
            let faction_id = parse_u32(faction_str, "assign_labor faction")?;
            let band = parse_u64(band_str, "assign_labor band_id")?;
            // **The kit is a NAMED token, lifted out of the tail before the role's own shape is
            // read** — so it can sit anywhere after the role and none of the positional forms below
            // has to make room for it. Absent = the job's default.
            let mut tail: Vec<&str> = parts.collect();
            let kit_id = take_named_token(&mut tail, "kit", "assign_labor kit id")?;
            let mut parts = tail.into_iter();
            let (workers, target_x, target_y, fauna_id, floor, species) = match role.as_str() {
                "forage" => {
                    let x = parts
                        .next()
                        .ok_or(CommandParseError::MissingArgument("target_x"))?;
                    let y = parts
                        .next()
                        .ok_or(CommandParseError::MissingArgument("target_y"))?;
                    // Two optional tokens, positionally:
                    // `forage <x> <y> [floor] [species] <workers>`. The worker count is always
                    // **last**, so the tail is read whole and the leading 0/1/2 tokens are the
                    // floor and then the species (Flora Roster S1 — which crop a Cultivate/Sow
                    // commits this ground to). Anything longer is a typo, not a longer form.
                    //
                    // **The one-optional-token case is disambiguated by "does it parse as `f32`".**
                    // That is sound, not a heuristic: a `flora_config.json` species key is
                    // snake_case (`wild_emmer`, `river_reed`) and cannot parse as a float, while a
                    // floor is only ever a number — the two token languages are disjoint, and
                    // `a_species_key_never_parses_as_a_floor` pins that against the shipped roster
                    // rather than trusting the claim.
                    let tail: Vec<&str> = parts.collect();
                    let (workers_tok, floor_tok, species_tok) = match tail.as_slice() {
                        [w] => (*w, None, None),
                        [t, w] => {
                            // A retired stance name is refused rather than falling through to the
                            // species reading, which is the one place the two token languages are
                            // not disjoint for a *stale* client: `sustain` is not a float, so
                            // without this it would be read as a crop selection.
                            reject_retired_stance(t)?;
                            match t.parse::<f32>() {
                                Ok(floor) => (*w, Some(floor), None),
                                Err(_) => (*w, None, Some(t.to_string())),
                            }
                        }
                        [t, sp, w] => (
                            *w,
                            Some(parse_f32(t, "assign_labor floor")?),
                            Some(sp.to_string()),
                        ),
                        [] => return Err(CommandParseError::MissingArgument("workers")),
                        [_, _, _, extra, ..] => {
                            return Err(CommandParseError::UnexpectedToken(extra.to_string()))
                        }
                    };
                    (
                        parse_u32(workers_tok, "assign_labor workers")?,
                        Some(parse_u32(x, "assign_labor target_x")?),
                        Some(parse_u32(y, "assign_labor target_y")?),
                        None,
                        floor_tok,
                        species_tok,
                    )
                }
                "hunt" => {
                    let herd = parts
                        .next()
                        .ok_or(CommandParseError::MissingArgument("herd_id"))?;
                    // `hunt <herd_id> [floor] <workers>` — the floor is **optional**, where the
                    // stance it replaced was required. Symmetric with forage, and it is what makes
                    // the default floor reachable from the command line at all. Disambiguated by
                    // tail length rather than by parsing, because both tokens are numbers here.
                    let tail: Vec<&str> = parts.collect();
                    let (workers_tok, floor_tok) = match tail.as_slice() {
                        [w] => (*w, None),
                        [t, w] => (*w, Some(parse_f32(t, "assign_labor floor")?)),
                        [] => return Err(CommandParseError::MissingArgument("workers")),
                        [_, _, extra, ..] => {
                            return Err(CommandParseError::UnexpectedToken(extra.to_string()))
                        }
                    };
                    (
                        parse_u32(workers_tok, "assign_labor workers")?,
                        None,
                        None,
                        Some(herd.to_string()),
                        floor_tok,
                        None,
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
                        None,
                    )
                }
                _ => return Err(CommandParseError::UnexpectedToken(role)),
            };
            Ok(CommandPayload::AssignLabor {
                faction_id,
                band_id: Some(band),
                role,
                workers,
                target_x,
                target_y,
                fauna_id,
                // Retired by the harvest floor arc; the text grammar has no stance token any more.
                policy: None,
                species,
                floor,
                kit_id,
            })
        }
        "move_band" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let band_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("band_id"))?;
            let x_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("target_x"))?;
            let y_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("target_y"))?;
            Ok(CommandPayload::MoveBand {
                faction_id: parse_u32(faction_str, "move_band faction")?,
                band_id: Some(parse_u64(band_str, "move_band band_id")?),
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
                .ok_or(CommandParseError::MissingArgument("band_id"))?;
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
                band_id: Some(parse_u64(band_str, "send_expedition band_id")?),
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
                .ok_or(CommandParseError::MissingArgument("expedition_band_id"))?;
            Ok(CommandPayload::RecallExpedition {
                faction_id: parse_u32(faction_str, "recall_expedition faction")?,
                expedition_band_id: parse_u64(
                    expedition_str,
                    "recall_expedition expedition_band_id",
                )?,
            })
        }
        // **The founding's grammar is DELIBERATELY CLOSED** — two positional tokens, naming the
        // faction and the party, and nothing else. Everything that shapes a founding is either the
        // party as it already stands or a gate the sim evaluates live, so a trailing token is a
        // misunderstanding of the verb rather than a value to ignore. Same fail-closed reading as
        // `send_denial_raid`.
        "split_band" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let band_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("band_id"))?;
            let workers_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("workers"))?;
            if let Some(extra) = parts.next() {
                return Err(CommandParseError::UnexpectedArgument(extra.to_string()));
            }
            Ok(CommandPayload::SplitBand {
                faction_id: parse_u32(faction_str, "split_band faction")?,
                band_id: Some(parse_u64(band_str, "split_band band_id")?),
                workers: parse_u32(workers_str, "split_band workers")?,
            })
        }
        "send_hunt_expedition" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let band_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("band_id"))?;
            let workers_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("party_workers"))?;
            let fauna_id = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("fauna_id"))?;
            // **The kit is a NAMED token** (`kit <id>`), lifted out before the optional positional
            // tail is read — a second positional would make `floor` un-omittable.
            let mut tail: Vec<&str> = parts.collect();
            let kit_id = take_named_token(&mut tail, "kit", "send_hunt_expedition kit id")?;
            let mut parts = tail.into_iter();
            // Optional trailing FLOOR — where the raid stops, as a fraction of the herd's `K`.
            // Absent = the sim's default (the food peak). `parse_f32` carries the retired-stance
            // guard, so a stale client's `sustain` names the grammar that moved rather than failing
            // as an unparseable number.
            let floor = parts
                .next()
                .map(|token| parse_f32(token, "send_hunt_expedition floor"))
                .transpose()?;
            // **The floor is now the ONLY positional tail, and anything after it is refused.** The
            // retired fill target sat here (`docs/plan_hunt_through_combat.md` §5.2), so a stale
            // caller's second number would otherwise be silently dropped — accepted as a raid it did
            // not order. Same fail-closed reading as `send_denial_raid`'s closed grammar below.
            if let Some(extra) = parts.next() {
                return Err(CommandParseError::UnexpectedArgument(extra.to_string()));
            }
            Ok(CommandPayload::SendHuntExpedition {
                faction_id: parse_u32(faction_str, "send_hunt_expedition faction")?,
                band_id: Some(parse_u64(band_str, "send_hunt_expedition band_id")?),
                party_workers: parse_u32(workers_str, "send_hunt_expedition party_workers")?,
                fauna_id: fauna_id.to_string(),
                floor,
                kit_id,
            })
        }
        // **The denial raid's grammar is DELIBERATELY CLOSED** (`docs/plan_denial_raid.md` §1): it
        // takes exactly four tokens and no optional trailing ones, because the mission carries no
        // floor at all. A trailing number is therefore not a value to ignore but a
        // misunderstanding of the verb, and it is refused by the ordinary "unexpected argument"
        // parse rather than silently accepted — the same fail-closed reading the floor's own
        // validation takes.
        "send_denial_raid" => {
            let faction_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("faction_id"))?;
            let band_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("band_id"))?;
            let workers_str = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("party_workers"))?;
            let fauna_id = parts
                .next()
                .ok_or(CommandParseError::MissingArgument("fauna_id"))?;
            // **The ONE thing the closed grammar now admits, and it is not a number.** A kit is a
            // property of the *party*, not of the mission, so it is the only order a raid carrying
            // no floor and no fill target still has to give. Everything else stays refused: a
            // trailing token that is not the named `kit <id>` pair is a misunderstanding of the
            // verb, not a value to ignore.
            let mut tail: Vec<&str> = parts.collect();
            let kit_id = take_named_token(&mut tail, "kit", "send_denial_raid kit id")?;
            if let Some(extra) = tail.first() {
                return Err(CommandParseError::UnexpectedArgument(extra.to_string()));
            }
            Ok(CommandPayload::SendDenialRaid {
                faction_id: parse_u32(faction_str, "send_denial_raid faction")?,
                band_id: Some(parse_u64(band_str, "send_denial_raid band_id")?),
                party_workers: parse_u32(workers_str, "send_denial_raid party_workers")?,
                fauna_id: fauna_id.to_string(),
                kit_id,
            })
        }
        "resync" => Ok(CommandPayload::Resync),
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
        "set_config_override" => {
            // The patch is the remainder of the line **verbatim**. A compact JSON object contains
            // spaces (inside string values, if nowhere else), and re-joining whitespace-split
            // tokens would silently corrupt them — so the two head tokens are re-split off the
            // original line rather than taken from `parts`.
            let after_verb = trimmed
                .split_once(char::is_whitespace)
                .map(|(_, rest)| rest.trim_start())
                .unwrap_or("");
            let (kind_token, patch_json) = after_verb.split_once(char::is_whitespace).ok_or(
                CommandParseError::MissingArgument(if after_verb.is_empty() {
                    "config override kind"
                } else {
                    "config override patch json"
                }),
            )?;
            let kind = ConfigOverrideKind::from_wire_str(&kind_token.to_ascii_lowercase())
                .ok_or_else(|| {
                    CommandParseError::InvalidConfigOverrideKind(kind_token.to_string())
                })?;
            let patch_json = patch_json.trim();
            if patch_json.is_empty() {
                return Err(CommandParseError::MissingArgument(
                    "config override patch json",
                ));
            }
            Ok(CommandPayload::SetConfigOverride {
                kind,
                patch_json: patch_json.to_string(),
            })
        }
        "clear_config_overrides" => Ok(CommandPayload::ClearConfigOverrides),
        other => Err(CommandParseError::UnknownCommand(other.to_string())),
    }
}

/// The **named-token** form the kit selection uses: `kit <id>`, a name followed by its value.
///
/// That shape is the repo's existing one — `queue_espionage_mission … owner 1 target 2 tier 2` and
/// `counterintel_budget … reserve 40` both read it — rather than an invented `kit=<id>`.
///
/// **Named rather than positional because `send_hunt_expedition` already carries an optional
/// positional tail.** `send_hunt_expedition <faction> <band> <workers> <herd> [floor]` cannot take a
/// second positional without the floor becoming un-omittable, and `assign_labor`'s per-role tails
/// already disambiguate by shape. A named token slots in anywhere in the tail, so it needs no
/// ordering rule and extends cleanly.
///
/// Removes the pair from `tokens` and answers the value; `None` when the name is absent. A name with
/// nothing after it is a missing argument, not a value to shrug at.
fn take_named_token(
    tokens: &mut Vec<&str>,
    name: &'static str,
    missing: &'static str,
) -> Result<Option<String>, CommandParseError> {
    let Some(index) = tokens
        .iter()
        .position(|token| token.eq_ignore_ascii_case(name))
    else {
        return Ok(None);
    };
    if index + 1 >= tokens.len() {
        return Err(CommandParseError::MissingArgument(missing));
    }
    let value = tokens.remove(index + 1).to_string();
    tokens.remove(index);
    Ok(Some(value))
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

fn parse_f32(value: &str, context: &'static str) -> Result<f32, CommandParseError> {
    reject_retired_stance(value)?;
    value
        .parse::<f32>()
        .map_err(|source| CommandParseError::InvalidFloat {
            value: value.to_string(),
            context,
            source,
        })
}

/// The four harvest stances the escapement floor replaced, refused **by name** wherever a floor is
/// read (`docs/plan_harvest_floor.md` §8.2).
///
/// A stale client is the reason this exists rather than letting the token fall through. On a hunt it
/// would fail anyway as a bad float, but on a forage assignment `sustain` sits in a position where a
/// **species key** is also legal, so it would be read as a crop selection — rejected downstream by
/// `validate_species_selection`, but reported as a plant that does not exist rather than as a
/// grammar that moved. One retired-token check makes both paths name the actual mistake.
///
/// The list is spelled out rather than read off an enum because `sim_runtime` cannot
/// depend on `core_sim`, and because it must outlive the type: these strings stay refused after the
/// enum is deleted, which is precisely when a stale client is most likely to still be sending them.
fn reject_retired_stance(value: &str) -> Result<(), CommandParseError> {
    const RETIRED_STANCES: [&str; 4] = ["sustain", "surplus", "deplete", "eradicate"];
    if RETIRED_STANCES.contains(&value.to_ascii_lowercase().as_str()) {
        return Err(CommandParseError::RetiredStanceToken(value.to_string()));
    }
    Ok(())
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

fn parse_security_policy(token: &str) -> Result<SecurityPolicyKind, CommandParseError> {
    match token.to_ascii_lowercase().as_str() {
        "lenient" | "light" | "open" => Ok(SecurityPolicyKind::Lenient),
        "standard" | "baseline" | "normal" => Ok(SecurityPolicyKind::Standard),
        "hardened" | "secure" | "fortified" => Ok(SecurityPolicyKind::Hardened),
        "crisis" | "panic" | "lockdown" => Ok(SecurityPolicyKind::Crisis),
        other => Err(CommandParseError::InvalidSecurityPolicy(other.to_string())),
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
                band_id: None,
            }
        );
        // Policy word only.
        assert_eq!(
            parse_command_line("follow_herd 0 game_deer_07 surplus").unwrap(),
            CommandPayload::FollowHerd {
                faction_id: 0,
                herd_id: "game_deer_07".to_string(),
                policy: Some("surplus".to_string()),
                band_id: None,
            }
        );
        // Lone numeric 3rd token = band id (policy omitted).
        assert_eq!(
            parse_command_line("follow_herd 0 game_deer_07 904").unwrap(),
            CommandPayload::FollowHerd {
                faction_id: 0,
                herd_id: "game_deer_07".to_string(),
                policy: None,
                band_id: Some(904),
            }
        );
        // Both: policy then band.
        assert_eq!(
            parse_command_line("follow_herd 0 game_deer_07 eradicate 904").unwrap(),
            CommandPayload::FollowHerd {
                faction_id: 0,
                herd_id: "game_deer_07".to_string(),
                policy: Some("eradicate".to_string()),
                band_id: Some(904),
            }
        );
    }

    /// **The floor is the ONE optional positional tail, and a second number is refused.**
    ///
    /// Both halves matter. The floor must stay omittable (absent = the sim's default), and the slot
    /// after it must be *closed*: the retired fill target sat there
    /// (`docs/plan_hunt_through_combat.md` §5.2), so a stale caller's `… 0.42 100` must fail rather
    /// than parse as a valid command naming a raid nobody ordered.
    #[test]
    fn parse_send_hunt_expedition_reads_the_floor_and_refuses_a_second_number() {
        let expected = |floor| CommandPayload::SendHuntExpedition {
            faction_id: 0,
            band_id: Some(7),
            party_workers: 4,
            fauna_id: "game_fowl_03".to_string(),
            floor,
            kit_id: None,
        };
        // Absent: the sim's own default floor.
        assert_eq!(
            parse_command_line("send_hunt_expedition 0 7 4 game_fowl_03").unwrap(),
            expected(None)
        );
        assert_eq!(
            parse_command_line("send_hunt_expedition 0 7 4 game_fowl_03 0.42").unwrap(),
            expected(Some(0.42))
        );
        // The retired fill target's old slot — refused, not dropped.
        assert!(matches!(
            parse_command_line("send_hunt_expedition 0 7 4 game_fowl_03 0.42 100"),
            Err(CommandParseError::UnexpectedArgument(_))
        ));
    }

    /// **The denial raid's grammar is CLOSED, and that is the assertion**
    /// (`docs/plan_denial_raid.md` §1): the mission carries no floor at all, so a fifth
    /// token is a misunderstanding of the verb rather than a value to ignore. Accepting it silently
    /// would teach the player that denial takes a number — the one thing the mission exists to say
    /// it does not.
    #[test]
    fn parse_send_denial_raid_takes_a_herd_and_a_party_and_nothing_else() {
        assert_eq!(
            parse_command_line("send_denial_raid 0 7 4 game_fowl_03").unwrap(),
            CommandPayload::SendDenialRaid {
                faction_id: 0,
                band_id: Some(7),
                party_workers: 4,
                fauna_id: "game_fowl_03".to_string(),
                kit_id: None,
            }
        );
        // A floor — legal on `send_hunt_expedition`, meaningless here, and refused rather than
        // dropped.
        assert!(matches!(
            parse_command_line("send_denial_raid 0 7 4 game_fowl_03 0.42"),
            Err(CommandParseError::UnexpectedArgument(_))
        ));
        assert!(matches!(
            parse_command_line("send_denial_raid 0 7 4"),
            Err(CommandParseError::MissingArgument("fauna_id"))
        ));
    }

    /// **`split_band` is CLOSED at three positional tokens** — faction, band, workers — and a
    /// fourth is a parse error rather than a silently ignored extra. The worker count is the
    /// player's only input; everything else about the split divides on the share it implies, so
    /// there is nothing a fourth token could legitimately mean.
    ///
    /// It carries the **`BandId`**, never entity bits — the identity that survives a rollback.
    #[test]
    fn parse_split_band_takes_a_faction_a_band_and_a_worker_count() {
        assert_eq!(
            parse_command_line("split_band 0 9001 6").unwrap(),
            CommandPayload::SplitBand {
                faction_id: 0,
                band_id: Some(9001),
                workers: 6,
            }
        );
        assert!(matches!(
            parse_command_line("split_band 0 9001 6 2"),
            Err(CommandParseError::UnexpectedArgument(_))
        ));
        assert!(matches!(
            parse_command_line("split_band 0 9001"),
            Err(CommandParseError::MissingArgument("workers"))
        ));
    }

    /// **The kit is a NAMED token, and it is order-independent** — `kit <id>`, the same shape
    /// `queue_espionage_mission`'s `owner 1 target 2` already uses. It has to be named rather than
    /// positional because `send_hunt_expedition` already carries an optional positional tail: a
    /// second would make `floor` un-omittable, so the two-token form is what lets a player name a
    /// kit without also naming a floor they did not want to change.
    #[test]
    fn the_kit_token_is_named_and_can_sit_anywhere_in_the_tail() {
        let with_kit = |floor, kit: Option<&str>| CommandPayload::SendHuntExpedition {
            faction_id: 0,
            band_id: Some(7),
            party_workers: 4,
            fauna_id: "game_fowl_03".to_string(),
            floor,
            kit_id: kit.map(str::to_string),
        };
        // Named alone — the case a positional grammar could not express at all.
        assert_eq!(
            parse_command_line("send_hunt_expedition 0 7 4 game_fowl_03 kit none").unwrap(),
            with_kit(None, Some("none"))
        );
        // Before the positional tail, and after it — same reading either way.
        assert_eq!(
            parse_command_line("send_hunt_expedition 0 7 4 game_fowl_03 kit none 0.42").unwrap(),
            with_kit(Some(0.42), Some("none"))
        );
        assert_eq!(
            parse_command_line("send_hunt_expedition 0 7 4 game_fowl_03 0.42 kit none").unwrap(),
            with_kit(Some(0.42), Some("none"))
        );
        // The denial raid's grammar admits the kit and nothing else — a kit is a property of the
        // party, a floor is a property of a mission this one does not have.
        assert_eq!(
            parse_command_line("send_denial_raid 0 7 4 game_fowl_03 kit none").unwrap(),
            CommandPayload::SendDenialRaid {
                faction_id: 0,
                band_id: Some(7),
                party_workers: 4,
                fauna_id: "game_fowl_03".to_string(),
                kit_id: Some("none".to_string()),
            }
        );
        assert!(matches!(
            parse_command_line("send_denial_raid 0 7 4 game_fowl_03 kit none 0.42"),
            Err(CommandParseError::UnexpectedArgument(_))
        ));
        // A name with no value after it is a missing argument, not a token to shrug at.
        assert!(matches!(
            parse_command_line("send_denial_raid 0 7 4 game_fowl_03 kit"),
            Err(CommandParseError::MissingArgument(_))
        ));
    }

    /// The same token on `assign_labor`, on both kit-bearing roles — lifted out of the tail before
    /// each role's own positional shape is read, so neither the forage floor/species pair nor the
    /// hunt floor has to make room for it.
    #[test]
    fn assign_labor_takes_the_kit_token_on_either_kit_bearing_role() {
        let CommandPayload::AssignLabor { kit_id, floor, .. } =
            parse_command_line("assign_labor 0 7 forage 3 4 0.15 wild_emmer 6 kit none").unwrap()
        else {
            panic!("assign_labor payload");
        };
        assert_eq!(kit_id.as_deref(), Some("none"));
        assert_eq!(floor, Some(0.15), "the floor is untouched by the kit token");

        let CommandPayload::AssignLabor {
            kit_id, workers, ..
        } = parse_command_line("assign_labor 0 7 hunt game_fowl_03 kit big_game 0.3 5").unwrap()
        else {
            panic!("assign_labor payload");
        };
        assert_eq!(kit_id.as_deref(), Some("big_game"));
        assert_eq!(workers, 5, "the worker count is still read last");

        // Absent is absent — the server resolves the job's default, the parser invents nothing.
        let CommandPayload::AssignLabor { kit_id, .. } =
            parse_command_line("assign_labor 0 7 hunt game_fowl_03 5").unwrap()
        else {
            panic!("assign_labor payload");
        };
        assert_eq!(kit_id, None);
    }

    #[test]
    fn parse_answer_fork_command() {
        assert_eq!(
            parse_command_line("answer_fork 0 sedentarization.soft_drift yes_trail").unwrap(),
            CommandPayload::AnswerFork {
                faction_id: 0,
                beat_id: "sedentarization.soft_drift".to_string(),
                choice_id: "yes_trail".to_string(),
            }
        );
        assert!(matches!(
            parse_command_line("answer_fork 0 sedentarization.soft_drift"),
            Err(CommandParseError::MissingArgument("choice_id"))
        ));
    }

    #[test]
    fn parse_new_game_command() {
        assert_eq!(
            parse_command_line("new_game earthlike 80 52 0 late_forager_tribe").unwrap(),
            CommandPayload::NewGame {
                preset_id: "earthlike".to_string(),
                width: 80,
                height: 52,
                seed: 0,
                profile_id: "late_forager_tribe".to_string(),
            }
        );
        // A non-zero seed is preserved verbatim (u64 range).
        assert_eq!(
            parse_command_line("new_game earthlike 80 52 119304647 late_forager_tribe").unwrap(),
            CommandPayload::NewGame {
                preset_id: "earthlike".to_string(),
                width: 80,
                height: 52,
                seed: 119304647,
                profile_id: "late_forager_tribe".to_string(),
            }
        );
        // Every positional argument is required.
        assert!(matches!(
            parse_command_line("new_game earthlike 80 52 0"),
            Err(CommandParseError::MissingArgument("profile_id"))
        ));
        assert!(matches!(
            parse_command_line("new_game earthlike 80 52"),
            Err(CommandParseError::MissingArgument("seed"))
        ));
        assert!(matches!(
            parse_command_line("new_game"),
            Err(CommandParseError::MissingArgument("preset_id"))
        ));
        // Non-numeric width is a parse error, not a silent default.
        assert!(matches!(
            parse_command_line("new_game earthlike wide 52 0 late_forager_tribe"),
            Err(CommandParseError::InvalidInteger { .. })
        ));
    }

    #[test]
    fn parse_tame_command() {
        assert_eq!(
            parse_command_line("tame 0 game_deer_07").unwrap(),
            CommandPayload::Tame {
                faction_id: 0,
                herd_id: "game_deer_07".to_string(),
            }
        );
        // herd_id is required.
        assert!(matches!(
            parse_command_line("tame 0"),
            Err(CommandParseError::MissingArgument("herd_id"))
        ));
    }

    /// `tame` **replaced** the `domesticate` early-claim — it is not an alias for it. The claim
    /// existed to skip the taming investment, which is the whole decision, so the verb is gone: a
    /// script still sending it must fail loudly rather than silently doing something adjacent.
    #[test]
    fn the_domesticate_early_claim_verb_no_longer_exists() {
        assert!(matches!(
            parse_command_line("domesticate 0 game_deer_07"),
            Err(CommandParseError::UnknownCommand(verb)) if verb == "domesticate"
        ));
        assert!(
            !COMMAND_VERBS
                .iter()
                .any(|help| help.verb == "domesticate" || help.aliases.contains(&"domesticate")),
            "the retired early-claim must not linger in the help listing"
        );
    }

    /// The seven debug pokes are gone; `reload_config` is not.
    ///
    /// `heat`, `bias`, `support`, `suppress`, `support_channel`, `spawn_influencer` and
    /// `corruption` were manual injection entry points for systems the sim still runs on its own —
    /// tile temperature, axis bias, the influencer roster, the corruption ledger. Only the pokes
    /// went, with the Inspector tab that was their sole caller.
    #[test]
    fn the_debug_poke_verbs_are_gone_and_reload_config_is_not() {
        const REMOVED: [&str; 7] = [
            "heat",
            "bias",
            "support",
            "suppress",
            "support_channel",
            "spawn_influencer",
            "corruption",
        ];
        for verb in REMOVED {
            assert!(
                matches!(
                    parse_command_line(verb),
                    Err(CommandParseError::UnknownCommand(ref parsed)) if parsed == verb
                ),
                "'{verb}' must no longer parse"
            );
            assert!(
                !COMMAND_VERBS
                    .iter()
                    .any(|help| help.verb == verb || help.aliases.contains(&verb)),
                "'{verb}' must not linger in the help listing"
            );
        }

        // The one verb explicitly kept — every kind token, both spellings of the verb, and the
        // optional path. A neighbouring row's deletion must not take this table entry with it.
        assert!(COMMAND_VERBS.iter().any(
            |help| help.verb == "reload_config" && help.aliases.contains(&"reload_sim_config")
        ));
        for (line, expected) in [
            ("reload_config", ReloadConfigKind::Simulation),
            ("reload_sim_config", ReloadConfigKind::Simulation),
            ("reload_config sim", ReloadConfigKind::Simulation),
            ("reload_config pipeline", ReloadConfigKind::TurnPipeline),
            ("reload_config overlays", ReloadConfigKind::SnapshotOverlays),
            (
                "reload_config crisis_archetypes",
                ReloadConfigKind::CrisisArchetypes,
            ),
            (
                "reload_config crisis_modifiers",
                ReloadConfigKind::CrisisModifiers,
            ),
            (
                "reload_config crisis_telemetry",
                ReloadConfigKind::CrisisTelemetry,
            ),
        ] {
            assert_eq!(
                parse_command_line(line).unwrap(),
                CommandPayload::ReloadConfig {
                    kind: expected,
                    path: None,
                },
                "'{line}' must still parse"
            );
        }
        assert_eq!(
            parse_command_line("reload_config pipeline /tmp/turn_pipeline_config.json").unwrap(),
            CommandPayload::ReloadConfig {
                kind: ReloadConfigKind::TurnPipeline,
                path: Some("/tmp/turn_pipeline_config.json".to_string()),
            }
        );
    }

    /// `reload_config` survives the wire, not just the parser.
    ///
    /// Retiring the seven pokes freed proto field numbers 2, 5..=10, which are `reserved` rather
    /// than reusable. `reload_config` is field 16 and must round-trip untouched.
    #[test]
    fn reload_config_round_trips_the_command_envelope() {
        let envelope = crate::CommandEnvelope {
            payload: parse_command_line("reload_config crisis_telemetry cfg.json").unwrap(),
            correlation_id: Some(7),
        };
        let bytes = envelope.encode_to_vec().expect("encode");
        let decoded = crate::CommandEnvelope::decode(&bytes).expect("decode");
        assert_eq!(decoded.payload, envelope.payload);
        assert_eq!(decoded.correlation_id, Some(7));
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

    /// The plant rung-3 verb: same `<faction> <x> <y>` shape as `cultivate`, because a Field — like a
    /// tended patch and unlike a herd — **is a place**.
    #[test]
    fn parse_sow_command() {
        assert_eq!(
            parse_command_line("sow 0 7 3").unwrap(),
            CommandPayload::Sow {
                faction_id: 0,
                target_x: 7,
                target_y: 3,
            }
        );
        // Both coordinates are required.
        assert!(matches!(
            parse_command_line("sow 0 7"),
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
                band_id: Some(904),
                role: "forage".to_string(),
                workers: 6,
                target_x: Some(3),
                target_y: Some(5),
                fauna_id: None,
                policy: None,
                species: None,
                floor: None,
                kit_id: None,
            }
        );
    }

    /// **The forage tail is disambiguated by "does the token parse as `f32`", and both readings
    /// round-trip.** `forage <x> <y> [floor] [species] <workers>` has two optional tokens with the
    /// worker count always last, so a single optional token has to be read as one or the other.
    #[test]
    fn parse_assign_labor_forage_one_optional_token_reads_as_floor_or_species() {
        assert_eq!(
            parse_command_line("assign_labor 0 904 forage 3 4 0.5 12").unwrap(),
            CommandPayload::AssignLabor {
                faction_id: 0,
                band_id: Some(904),
                role: "forage".to_string(),
                workers: 12,
                target_x: Some(3),
                target_y: Some(4),
                fauna_id: None,
                policy: None,
                species: None,
                floor: Some(0.5),
                kit_id: None,
            },
            "a numeric optional token is the FLOOR"
        );
        assert_eq!(
            parse_command_line("assign_labor 0 904 forage 3 4 wild_emmer 12").unwrap(),
            CommandPayload::AssignLabor {
                faction_id: 0,
                band_id: Some(904),
                role: "forage".to_string(),
                workers: 12,
                target_x: Some(3),
                target_y: Some(4),
                fauna_id: None,
                policy: None,
                species: Some("wild_emmer".to_string()),
                floor: None,
                kit_id: None,
            },
            "a non-numeric optional token is the SPECIES"
        );
    }

    /// **THE PROOF the disambiguation above rests on**: no `flora_config.json` species key parses as
    /// a float, so the two token languages are disjoint and a single optional token is never
    /// ambiguous. Asserted against the **shipped roster** rather than against the claim — a future
    /// crop named `7` would fail here rather than silently become a floor.
    ///
    /// The roster is inlined because `sim_runtime` does not depend on `core_sim`; the companion test
    /// `core_sim/tests/flora_roster.rs::every_shipped_species_key_is_covered_by_the_command_grammar`
    /// pins this list against the real config.
    #[test]
    fn a_species_key_never_parses_as_a_floor() {
        for key in SPECIES_KEY_SHAPES {
            assert!(
                key.parse::<f32>().is_err(),
                "'{key}' parses as a float, so the forage tail would read it as a floor"
            );
        }
    }

    /// **A stale client's stance token is refused by NAME, on both webs.** The hunt form would fail
    /// anyway as a bad float, but the forage form would not: `sustain` sits where a species key is
    /// also legal, so without `reject_retired_stance` it parses as a crop selection and is reported
    /// three layers later as a plant that does not exist. Both must name the grammar that moved.
    #[test]
    fn a_retired_stance_token_is_refused_by_name_not_read_as_a_species() {
        for stance in ["sustain", "surplus", "deplete", "eradicate", "Sustain"] {
            for line in [
                format!("assign_labor 0 904 forage 3 5 {stance} 6"),
                format!("assign_labor 0 904 forage 3 5 {stance} wild_emmer 6"),
                format!("assign_labor 0 904 hunt herd-7 {stance} 6"),
            ] {
                assert!(
                    matches!(
                        parse_command_line(&line),
                        Err(CommandParseError::RetiredStanceToken(_))
                    ),
                    "'{line}' should name the retired stance, not fail some other way"
                );
            }
        }
    }

    /// The guard is **exactly** the four retired names — a crop, a floor and a herd id that merely
    /// resemble them still parse, so the check cannot quietly widen into the species language.
    #[test]
    fn the_retired_stance_guard_does_not_swallow_neighbouring_tokens() {
        for line in [
            "assign_labor 0 904 forage 3 5 sustainable_yam 6",
            "assign_labor 0 904 forage 3 5 0.5 6",
            "assign_labor 0 904 hunt deplete-ridge-herd 0.3 6",
        ] {
            assert!(
                parse_command_line(line).is_ok(),
                "'{line}' is legal and must not trip the retired-stance guard"
            );
        }
    }

    /// Species-key shapes the forage grammar must never mistake for a floor. snake_case identifiers,
    /// which is what every `flora_config.json` key is.
    const SPECIES_KEY_SHAPES: [&str; 6] = [
        "wild_emmer",
        "wild_tubers",
        "grapevine",
        "river_reed",
        "tobacco",
        "wild_rice",
    ];

    /// The floor round-trips at both ends of its range and beside a species selection.
    #[test]
    fn parse_assign_labor_forage_with_floor_and_species() {
        assert_eq!(
            parse_command_line("assign_labor 0 904 forage 3 5 0.15 wild_emmer 6").unwrap(),
            CommandPayload::AssignLabor {
                faction_id: 0,
                band_id: Some(904),
                role: "forage".to_string(),
                workers: 6,
                target_x: Some(3),
                target_y: Some(5),
                fauna_id: None,
                policy: None,
                species: Some("wild_emmer".to_string()),
                floor: Some(0.15),
                kit_id: None,
            }
        );
        // A fourth token is a typo, not a longer form — fail closed rather than silently drop it.
        assert!(matches!(
            parse_command_line("assign_labor 0 904 forage 3 5 0.15 wild_emmer 6 7"),
            Err(CommandParseError::UnexpectedToken(_))
        ));
        // With BOTH optional tokens present the first is unambiguously the floor, so a non-numeric
        // one there is a parse error rather than a second species — and a *retired stance* there
        // names itself rather than reporting as a bad float.
        assert!(matches!(
            parse_command_line("assign_labor 0 904 forage 3 5 wheat wild_emmer 6"),
            Err(CommandParseError::InvalidFloat { .. })
        ));
        assert!(matches!(
            parse_command_line("assign_labor 0 904 forage 3 5 sustain wild_emmer 6"),
            Err(CommandParseError::RetiredStanceToken(_))
        ));
    }

    /// **Hunt's floor is OPTIONAL**, where the stance it replaced was required — symmetric with
    /// forage, and what makes the default floor reachable. Both tokens are numbers, so the tail is
    /// disambiguated by length.
    #[test]
    fn parse_assign_labor_hunt_floor_is_optional() {
        assert_eq!(
            parse_command_line("assign_labor 0 904 hunt game_deer_07 4").unwrap(),
            CommandPayload::AssignLabor {
                faction_id: 0,
                band_id: Some(904),
                role: "hunt".to_string(),
                workers: 4,
                target_x: None,
                target_y: None,
                fauna_id: Some("game_deer_07".to_string()),
                policy: None,
                species: None,
                floor: None,
                kit_id: None,
            },
            "one tail token is the worker count; the floor defaults"
        );
        for floor in [0.0_f32, 0.15, 0.3, 0.5, 1.0] {
            assert_eq!(
                parse_command_line(&format!("assign_labor 0 904 hunt game_deer_07 {floor} 4"))
                    .unwrap(),
                CommandPayload::AssignLabor {
                    faction_id: 0,
                    band_id: Some(904),
                    role: "hunt".to_string(),
                    workers: 4,
                    target_x: None,
                    target_y: None,
                    fauna_id: Some("game_deer_07".to_string()),
                    policy: None,
                    species: None,
                    floor: Some(floor),
                    kit_id: None,
                },
                "hunt floor {floor} should round-trip"
            );
        }
        // The retired stance tokens are no longer a grammar, and they say so by name rather than
        // reporting as a bad float — see `reject_retired_stance`.
        assert!(matches!(
            parse_command_line("assign_labor 0 904 hunt game_deer_07 sustain 4"),
            Err(CommandParseError::RetiredStanceToken(_))
        ));
        // A third tail token is a typo, not a longer form.
        assert!(matches!(
            parse_command_line("assign_labor 0 904 hunt game_deer_07 0.5 4 9"),
            Err(CommandParseError::UnexpectedToken(_))
        ));
    }

    #[test]
    fn parse_assign_labor_scout_and_warrior() {
        assert_eq!(
            parse_command_line("assign_labor 0 904 scout 5").unwrap(),
            CommandPayload::AssignLabor {
                faction_id: 0,
                band_id: Some(904),
                role: "scout".to_string(),
                workers: 5,
                target_x: None,
                target_y: None,
                fauna_id: None,
                policy: None,
                species: None,
                floor: None,
                kit_id: None,
            }
        );
        assert_eq!(
            parse_command_line("assign_labor 0 904 warrior 2").unwrap(),
            CommandPayload::AssignLabor {
                faction_id: 0,
                band_id: Some(904),
                role: "warrior".to_string(),
                workers: 2,
                target_x: None,
                target_y: None,
                fauna_id: None,
                policy: None,
                species: None,
                floor: None,
                kit_id: None,
            }
        );
    }

    #[test]
    fn parse_move_band_command() {
        assert_eq!(
            parse_command_line("move_band 0 904 12 7").unwrap(),
            CommandPayload::MoveBand {
                faction_id: 0,
                band_id: Some(904),
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
        // Missing the trailing worker count on a hunt assignment (herd present, nothing after) —
        // the floor is optional, so the only required tail token is the crew.
        assert!(matches!(
            parse_command_line("assign_labor 0 904 hunt game_deer_07"),
            Err(CommandParseError::MissingArgument("workers"))
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

    /// Both trailing tokens are optional, so the grammar is resolved by shape: a number is the
    /// band, a word is the scope, and either may be omitted.
    #[test]
    fn parse_cancel_order_band_and_scope_are_each_optional() {
        assert_eq!(
            parse_command_line("cancel_order 1 42 work").unwrap(),
            CommandPayload::CancelOrder {
                faction_id: 1,
                band_id: Some(42),
                scope: CancelScope::Work,
            }
        );
        // No scope token → the historical clear-everything behaviour.
        assert_eq!(
            parse_command_line("cancel_order 1 42").unwrap(),
            CommandPayload::CancelOrder {
                faction_id: 1,
                band_id: Some(42),
                scope: CancelScope::All,
            }
        );
        // Band omitted, scope given — the default-band picker still applies.
        assert_eq!(
            parse_command_line("cancel_order 1 work").unwrap(),
            CommandPayload::CancelOrder {
                faction_id: 1,
                band_id: None,
                scope: CancelScope::Work,
            }
        );
        assert_eq!(
            parse_command_line("cancel_order 1 42 ROLES").unwrap(),
            CommandPayload::CancelOrder {
                faction_id: 1,
                band_id: Some(42),
                scope: CancelScope::Roles,
            }
        );
        // Shape, not position: the two trailing tokens may arrive in either order.
        assert_eq!(
            parse_command_line("cancel_order 1 work 42").unwrap(),
            CommandPayload::CancelOrder {
                faction_id: 1,
                band_id: Some(42),
                scope: CancelScope::Work,
            }
        );
    }

    /// A slot filled twice is a typo, and this verb is destructive: a fat-fingered second band id
    /// would otherwise silently clear a *different* band's assignments.
    #[test]
    fn parse_cancel_order_rejects_a_repeated_slot() {
        assert!(matches!(
            parse_command_line("cancel_order 1 42 43"),
            Err(CommandParseError::UnexpectedToken(token)) if token == "43"
        ));
        assert!(matches!(
            parse_command_line("cancel_order 1 work roles"),
            Err(CommandParseError::UnexpectedToken(token)) if token == "roles"
        ));
    }

    /// Trailing junk past a complete command is rejected rather than discarded.
    #[test]
    fn parse_cancel_order_rejects_trailing_tokens() {
        assert!(matches!(
            parse_command_line("cancel_order 1 42 work junk"),
            Err(CommandParseError::UnexpectedToken(token)) if token == "junk"
        ));
    }

    /// Fail closed: silently falling back to `all` would mass-unassign a band that asked for `work`.
    #[test]
    fn parse_cancel_order_rejects_an_unrecognised_scope() {
        assert!(matches!(
            parse_command_line("cancel_order 1 42 bogus"),
            Err(CommandParseError::UnexpectedToken(token)) if token == "bogus"
        ));
    }

    /// **Both source kinds parse at their own arity** — the control for the rejection below, so that
    /// test cannot pass by the verb being broken outright.
    #[test]
    fn parse_abandon_improvement_reads_each_webs_arity() {
        assert_eq!(
            parse_command_line("abandon_improvement 1 forage 4 7").unwrap(),
            CommandPayload::AbandonImprovement {
                faction_id: 1,
                kind: "forage".to_string(),
                target_x: 4,
                target_y: 7,
                fauna_id: String::new(),
            }
        );
        assert_eq!(
            parse_command_line("abandon 1 hunt game_test").unwrap(),
            CommandPayload::AbandonImprovement {
                faction_id: 1,
                kind: "hunt".to_string(),
                target_x: 0,
                target_y: 0,
                fauna_id: "game_test".to_string(),
            }
        );
    }

    /// Fail closed on an unknown source kind, exactly as `assign_labor`'s identical `forage`/`hunt`
    /// grammar does. A catch-all forage arm read the tile arity for *any* token, so a typo either
    /// reported an argument unrelated to the mistake (`... 1 foo` -> "missing argument: target_x") or
    /// parsed clean at four tokens and was rejected asynchronously in the feed.
    #[test]
    fn parse_abandon_improvement_rejects_an_unknown_source_kind() {
        assert!(matches!(
            parse_command_line("abandon_improvement 1 foo"),
            Err(CommandParseError::UnexpectedToken(token)) if token == "foo"
        ));
        assert!(matches!(
            parse_command_line("abandon_improvement 1 foo 4 7"),
            Err(CommandParseError::UnexpectedToken(token)) if token == "foo"
        ));
    }

    /// The patch is the **rest of the line**, verbatim: a compact JSON object carries spaces inside
    /// its string values, and whitespace-tokenizing then re-joining would corrupt them (and any
    /// deliberate formatting). Only the verb and the kind are tokens.
    #[test]
    fn parse_set_config_override_takes_the_rest_of_the_line_verbatim() {
        let json = r#"{"forage": {"per_worker_biomass_capacity": 12.0}, "note": "a b  c"}"#;
        assert_eq!(
            parse_command_line(&format!("set_config_override labor {json}")).unwrap(),
            CommandPayload::SetConfigOverride {
                kind: ConfigOverrideKind::Labor,
                patch_json: json.to_string(),
            }
        );
    }

    /// An unknown kind is rejected, never defaulted — guessing which config a designer meant to
    /// retune would install numbers nobody asked for.
    #[test]
    fn parse_set_config_override_rejects_an_unknown_kind() {
        assert!(matches!(
            parse_command_line("set_config_override flora {}"),
            Err(CommandParseError::InvalidConfigOverrideKind(kind)) if kind == "flora"
        ));
        assert!(matches!(
            parse_command_line("set_config_override labor"),
            Err(CommandParseError::MissingArgument(
                "config override patch json"
            ))
        ));
        assert!(matches!(
            parse_command_line("set_config_override"),
            Err(CommandParseError::MissingArgument("config override kind"))
        ));
    }

    #[test]
    fn parse_clear_config_overrides() {
        assert_eq!(
            parse_command_line("clear_config_overrides").unwrap(),
            CommandPayload::ClearConfigOverrides
        );
    }
}
