use std::convert::TryFrom;

use prost::Message;
use thiserror::Error;

mod proto {
    include!(concat!(env!("OUT_DIR"), "/shadow_scale.commands.rs"));
}

use proto as pb;

/// High-level representation of a command envelope.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandEnvelope {
    pub payload: CommandPayload,
    pub correlation_id: Option<u64>,
}

/// Supported command payloads.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum CommandPayload {
    Turn {
        steps: u32,
    },
    ResetMap {
        width: u32,
        height: u32,
    },
    Orders {
        faction_id: u32,
        directive: OrdersDirective,
    },
    Rollback {
        tick: u64,
    },
    UpdateEspionageGenerators {
        updates: Vec<EspionageGeneratorUpdate>,
    },
    QueueEspionageMission {
        mission_id: String,
        owner_faction: u32,
        target_owner_faction: u32,
        discovery_id: u32,
        agent_handle: u32,
        target_tier: Option<u8>,
        scheduled_tick: Option<u64>,
    },
    UpdateEspionageQueueDefaults {
        scheduled_tick_offset: Option<u32>,
        target_tier: Option<u8>,
    },
    UpdateCounterIntelPolicy {
        faction: u32,
        policy: SecurityPolicyKind,
    },
    AdjustCounterIntelBudget {
        faction: u32,
        reserve: Option<f32>,
        delta: Option<f32>,
    },
    ReloadConfig {
        kind: ReloadConfigKind,
        path: Option<String>,
    },
    SetCrisisAutoSeed {
        enabled: bool,
    },
    SetFogEnabled {
        enabled: bool,
    },
    /// Republish the world as a FULL snapshot. Client-initiated recovery for delta streaming —
    /// see `ResyncCommand` in `command.proto`.
    Resync,
    SpawnCrisis {
        faction_id: u32,
        archetype_id: String,
    },
    SetStartProfile {
        profile_id: String,
    },
    ScoutArea {
        faction_id: u32,
        target_x: u32,
        target_y: u32,
        band_id: Option<u64>,
    },
    FollowHerd {
        faction_id: u32,
        herd_id: String,
        policy: Option<String>,
        band_id: Option<u64>,
    },
    FoundSettlement {
        faction_id: u32,
        target_x: u32,
        target_y: u32,
    },
    ForageTile {
        faction_id: u32,
        target_x: u32,
        target_y: u32,
        module: String,
        band_id: Option<u64>,
    },
    HuntGame {
        faction_id: u32,
        target_x: u32,
        target_y: u32,
        band_id: Option<u64>,
    },
    HuntFauna {
        faction_id: u32,
        herd_id: String,
        band_id: Option<u64>,
    },
    Tame {
        faction_id: u32,
        herd_id: String,
    },
    Cultivate {
        faction_id: u32,
        target_x: u32,
        target_y: u32,
    },
    Sow {
        faction_id: u32,
        target_x: u32,
        target_y: u32,
    },
    Corral {
        faction_id: u32,
        target_x: u32,
        target_y: u32,
    },
    /// **Abandon a running improvement** — clear the build verb off every band of `faction_id`
    /// working the named source, leaving the harvest stance and the crew untouched (issue #442).
    ///
    /// The one command that passes `None` where `Cultivate`/`Sow`/`Tame`/`Corral` pass a verb. It is
    /// **ungated**: abandonment is not a rung transition, and a *stalled* build on unhealthy ground is
    /// exactly when a player reaches for it. It does not zero the meter — each web's existing
    /// unworked-source rule takes over (see `AbandonImprovementCommand` in `command.proto`).
    ///
    /// Names a **source**, not a verb: `kind` is `"forage"` (uses `target_x`/`target_y`) or `"hunt"`
    /// (uses `fauna_id`), because at most one improvement is ever in flight on a source.
    AbandonImprovement {
        faction_id: u32,
        kind: String,
        target_x: u32,
        target_y: u32,
        fauna_id: String,
    },
    ExtendPen {
        faction_id: u32,
        target_x: u32,
        target_y: u32,
    },
    /// The Telling: answer a pending narrative fork with one of its authored choices.
    AnswerFork {
        faction_id: u32,
        beat_id: String,
        choice_id: String,
    },
    CancelOrder {
        faction_id: u32,
        band_id: Option<u64>,
        scope: CancelScope,
    },
    AssignLabor {
        faction_id: u32,
        band_id: Option<u64>,
        role: String,
        workers: u32,
        target_x: Option<u32>,
        target_y: Option<u32>,
        fauna_id: Option<String>,
        /// **RETIRED by the harvest floor arc** — a labor assignment carries a [`Self::AssignLabor`]
        /// `floor`, not a stance. Kept on the payload only because the proto field number is
        /// immutable once shipped; the server ignores it.
        policy: Option<String>,
        /// Which named plant a **forage** `Cultivate`/`Sow` should commit the patch to — a
        /// `flora_config.json` species key. `None` = *"pick the tile's dominant legal plant"*
        /// (`docs/plan_flora_roster.md` §4.3). Ignored for every other role.
        species: Option<String>,
        /// **WHERE THE CREW STOPS, as a fraction of the source's carrying capacity.** `None` = the
        /// sim's default (`components::DEFAULT_ESCAPEMENT_FLOOR`). Validated `0.0..=1.0` at the
        /// server boundary and **rejected**, never clamped. Ignored by the band-wide roles.
        floor: Option<f32>,
    },
    MoveBand {
        faction_id: u32,
        band_id: Option<u64>,
        target_x: u32,
        target_y: u32,
    },
    SendExpedition {
        faction_id: u32,
        band_id: Option<u64>,
        party_workers: u32,
        target_x: u32,
        target_y: u32,
    },
    RecallExpedition {
        faction_id: u32,
        expedition_band_id: u64,
    },
    SendHuntExpedition {
        faction_id: u32,
        band_id: Option<u64>,
        party_workers: u32,
        fauna_id: String,
        /// **Where the raid stops**, as a fraction of the herd's carrying capacity. `None` = the
        /// sim's default (`components::DEFAULT_ESCAPEMENT_FLOOR`); validated `0.0..=1.0` at the
        /// server boundary and **rejected**, never clamped.
        floor: Option<f32>,
    },
    ExportMap {
        path: Option<String>,
    },
    /// Boot-idle new game: generate a world on demand (the server boots with none). `seed == 0`
    /// randomizes the map seed, mirroring `ResetMap`; an unknown `profile_id` is rejected server-side.
    /// Proto field 43.
    NewGame {
        preset_id: String,
        width: u32,
        height: u32,
        seed: u64,
        profile_id: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct EspionageGeneratorUpdate {
    pub template_id: String,
    pub enabled: Option<bool>,
    pub per_faction: Option<u8>,
}

/// What a `cancel_order` clears on the band. The Band panel splits the old single "cancel"
/// button into per-section clears, so the verb has to name its target rather than always wiping
/// everything.
///
/// [`CancelScope::All`] is the default (an absent wire/text token decodes to it) and keeps the
/// historical behaviour. The two narrow scopes deliberately leave travel alone: moving is not
/// working, so unassigning a band's sources must not also strand it mid-journey.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CancelScope {
    /// Worked sources + standing roles, and stop any in-progress travel.
    #[default]
    All,
    /// Worked food sources only (Forage/Hunt). Roles and travel untouched.
    Work,
    /// Standing roles only (Scout/Warrior). Sources and travel untouched.
    Roles,
}

impl CancelScope {
    /// Wire/text token for this scope.
    pub fn as_str(self) -> &'static str {
        match self {
            CancelScope::All => "all",
            CancelScope::Work => "work",
            CancelScope::Roles => "roles",
        }
    }

    /// Parse a scope token (case-insensitive). `None` for anything unrecognised, so callers choose
    /// whether to fail closed (the text parser) or fall back to [`CancelScope::All`] (the wire).
    pub fn parse(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "all" => Some(CancelScope::All),
            "work" => Some(CancelScope::Work),
            "roles" => Some(CancelScope::Roles),
            _ => None,
        }
    }
}

/// Directive for faction orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrdersDirective {
    Ready,
}

/// Configuration kinds supported by reload commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadConfigKind {
    Simulation,
    TurnPipeline,
    SnapshotOverlays,
    CrisisArchetypes,
    CrisisModifiers,
    CrisisTelemetry,
}

/// Counter-intelligence security posture controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityPolicyKind {
    Lenient,
    Standard,
    Hardened,
    Crisis,
}

/// Error returned when encoding a command envelope fails.
#[derive(Debug, Error)]
pub enum CommandEncodeError {
    #[error("encode failed: {0}")]
    Encode(#[from] prost::EncodeError),
}

/// Error returned when decoding a command envelope fails.
///
/// The protobuf schema reserves `*_UNSPECIFIED` enum values (encoded as `0`).
/// Decoding such a value yields [`CommandDecodeError::InvalidEnum`] so callers
/// can reject malformed or legacy payloads early.
#[derive(Debug, Error)]
pub enum CommandDecodeError {
    #[error("decode failed: {0}")]
    Decode(#[from] prost::DecodeError),
    #[error("command envelope missing payload")]
    MissingPayload,
    #[error("invalid enum value {value} for {field}")]
    InvalidEnum { field: &'static str, value: i32 },
}

impl CommandEnvelope {
    /// Encode the envelope into a protobuf binary frame.
    pub fn encode_to_vec(&self) -> Result<Vec<u8>, CommandEncodeError> {
        let proto = self.to_proto();
        let mut buffer = Vec::with_capacity(proto.encoded_len());
        proto.encode(&mut buffer)?;
        Ok(buffer)
    }

    /// Decode an envelope from a protobuf binary frame.
    pub fn decode(bytes: &[u8]) -> Result<Self, CommandDecodeError> {
        let proto = pb::CommandEnvelope::decode(bytes)?;
        Self::try_from_proto(proto)
    }

    /// Convert the high-level envelope into its protobuf representation.
    pub fn to_proto(&self) -> pb::CommandEnvelope {
        let command = Some(match &self.payload {
            CommandPayload::Turn { steps } => {
                pb::command_envelope::Command::Turn(pb::TurnCommand { steps: *steps })
            }
            CommandPayload::ResetMap { width, height } => {
                pb::command_envelope::Command::ResetMap(pb::ResetMapCommand {
                    width: *width,
                    height: *height,
                })
            }
            CommandPayload::Orders {
                faction_id,
                directive,
            } => pb::command_envelope::Command::Orders(pb::OrdersCommand {
                faction_id: *faction_id,
                directive: orders_directive_to_proto(*directive) as i32,
            }),
            CommandPayload::Rollback { tick } => {
                pb::command_envelope::Command::Rollback(pb::RollbackCommand { tick: *tick })
            }
            CommandPayload::UpdateEspionageGenerators { updates } => {
                pb::command_envelope::Command::UpdateEspionageGenerators(
                    pb::UpdateEspionageGeneratorsCommand {
                        updates: updates
                            .iter()
                            .map(|update| pb::EspionageGeneratorUpdate {
                                template_id: update.template_id.clone(),
                                enabled: update.enabled,
                                per_faction: update.per_faction.map(|value| value as u32),
                            })
                            .collect(),
                    },
                )
            }
            CommandPayload::QueueEspionageMission {
                mission_id,
                owner_faction,
                target_owner_faction,
                discovery_id,
                agent_handle,
                target_tier,
                scheduled_tick,
            } => pb::command_envelope::Command::QueueEspionageMission(
                pb::QueueEspionageMissionCommand {
                    mission_id: mission_id.clone(),
                    owner_faction: *owner_faction,
                    target_owner_faction: *target_owner_faction,
                    discovery_id: *discovery_id,
                    agent_handle: *agent_handle,
                    target_tier: target_tier.map(|value| value as u32),
                    scheduled_tick: *scheduled_tick,
                },
            ),
            CommandPayload::UpdateEspionageQueueDefaults {
                scheduled_tick_offset,
                target_tier,
            } => pb::command_envelope::Command::UpdateEspionageQueueDefaults(
                pb::UpdateEspionageQueueDefaultsCommand {
                    scheduled_tick_offset: *scheduled_tick_offset,
                    target_tier: target_tier.map(|value| value as u32),
                },
            ),
            CommandPayload::UpdateCounterIntelPolicy { faction, policy } => {
                pb::command_envelope::Command::UpdateCounterIntelPolicy(
                    pb::UpdateCounterIntelPolicyCommand {
                        faction: *faction,
                        policy: security_policy_kind_to_proto(*policy) as i32,
                    },
                )
            }
            CommandPayload::AdjustCounterIntelBudget {
                faction,
                reserve,
                delta,
            } => pb::command_envelope::Command::AdjustCounterIntelBudget(
                pb::AdjustCounterIntelBudgetCommand {
                    faction: *faction,
                    reserve: *reserve,
                    delta: *delta,
                },
            ),
            CommandPayload::ReloadConfig { kind, path } => {
                pb::command_envelope::Command::ReloadConfig(pb::ReloadConfigCommand {
                    kind: reload_config_kind_to_proto(*kind) as i32,
                    path: path.clone(),
                })
            }
            CommandPayload::SetCrisisAutoSeed { enabled } => {
                pb::command_envelope::Command::SetCrisisAutoSeed(pb::SetCrisisAutoSeedCommand {
                    enabled: *enabled,
                })
            }
            CommandPayload::SetFogEnabled { enabled } => {
                pb::command_envelope::Command::SetFog(pb::SetFogEnabledCommand {
                    enabled: *enabled,
                })
            }
            CommandPayload::Resync => pb::command_envelope::Command::Resync(pb::ResyncCommand {}),
            CommandPayload::SpawnCrisis {
                faction_id,
                archetype_id,
            } => pb::command_envelope::Command::SpawnCrisis(pb::SpawnCrisisCommand {
                faction: *faction_id,
                archetype_id: archetype_id.clone(),
            }),
            CommandPayload::SetStartProfile { profile_id } => {
                pb::command_envelope::Command::SetStartProfile(pb::SetStartProfileCommand {
                    profile_id: profile_id.clone(),
                })
            }
            CommandPayload::ScoutArea {
                faction_id,
                target_x,
                target_y,
                band_id,
            } => pb::command_envelope::Command::ScoutArea(pb::ScoutAreaCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
                band_id: *band_id,
            }),
            CommandPayload::FollowHerd {
                faction_id,
                herd_id,
                policy,
                band_id,
            } => pb::command_envelope::Command::FollowHerd(pb::FollowHerdCommand {
                faction_id: *faction_id,
                herd_id: herd_id.clone(),
                policy: policy.clone(),
                band_id: *band_id,
            }),
            CommandPayload::FoundSettlement {
                faction_id,
                target_x,
                target_y,
            } => pb::command_envelope::Command::FoundSettlement(pb::FoundSettlementCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
            }),
            CommandPayload::ForageTile {
                faction_id,
                target_x,
                target_y,
                module,
                band_id,
            } => pb::command_envelope::Command::ForageTile(pb::ForageTileCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
                module: module.clone(),
                band_id: *band_id,
            }),
            CommandPayload::HuntGame {
                faction_id,
                target_x,
                target_y,
                band_id,
            } => pb::command_envelope::Command::HuntGame(pb::HuntGameCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
                band_id: *band_id,
            }),
            CommandPayload::HuntFauna {
                faction_id,
                herd_id,
                band_id,
            } => pb::command_envelope::Command::HuntFauna(pb::HuntFaunaCommand {
                faction_id: *faction_id,
                herd_id: herd_id.clone(),
                band_id: *band_id,
            }),
            CommandPayload::Tame {
                faction_id,
                herd_id,
            } => pb::command_envelope::Command::Tame(pb::TameCommand {
                faction_id: *faction_id,
                herd_id: herd_id.clone(),
            }),
            CommandPayload::Cultivate {
                faction_id,
                target_x,
                target_y,
            } => pb::command_envelope::Command::Cultivate(pb::CultivateCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
            }),
            CommandPayload::Sow {
                faction_id,
                target_x,
                target_y,
            } => pb::command_envelope::Command::Sow(pb::SowCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
            }),
            CommandPayload::AbandonImprovement {
                faction_id,
                kind,
                target_x,
                target_y,
                fauna_id,
            } => pb::command_envelope::Command::AbandonImprovement(pb::AbandonImprovementCommand {
                faction_id: *faction_id,
                kind: kind.clone(),
                target_x: *target_x,
                target_y: *target_y,
                fauna_id: fauna_id.clone(),
            }),
            CommandPayload::Corral {
                faction_id,
                target_x,
                target_y,
            } => pb::command_envelope::Command::Corral(pb::CorralCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
            }),
            CommandPayload::ExtendPen {
                faction_id,
                target_x,
                target_y,
            } => pb::command_envelope::Command::ExtendPen(pb::ExtendPenCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
            }),
            CommandPayload::AnswerFork {
                faction_id,
                beat_id,
                choice_id,
            } => pb::command_envelope::Command::AnswerFork(pb::AnswerForkCommand {
                faction_id: *faction_id,
                beat_id: beat_id.clone(),
                choice_id: choice_id.clone(),
            }),
            CommandPayload::CancelOrder {
                faction_id,
                band_id,
                scope,
            } => pb::command_envelope::Command::CancelOrder(pb::CancelOrderCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                scope: Some(scope.as_str().to_string()),
            }),
            CommandPayload::AssignLabor {
                faction_id,
                band_id,
                role,
                workers,
                target_x,
                target_y,
                fauna_id,
                policy,
                species,
                floor,
            } => pb::command_envelope::Command::AssignLabor(pb::AssignLaborCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                role: role.clone(),
                workers: *workers,
                target_x: *target_x,
                target_y: *target_y,
                fauna_id: fauna_id.clone(),
                policy: policy.clone(),
                species: species.clone(),
                floor: *floor,
            }),
            CommandPayload::MoveBand {
                faction_id,
                band_id,
                target_x,
                target_y,
            } => pb::command_envelope::Command::MoveBand(pb::MoveBandCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                target_x: *target_x,
                target_y: *target_y,
            }),
            CommandPayload::SendExpedition {
                faction_id,
                band_id,
                party_workers,
                target_x,
                target_y,
            } => pb::command_envelope::Command::SendExpedition(pb::SendExpeditionCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                party_workers: *party_workers,
                target_x: *target_x,
                target_y: *target_y,
            }),
            CommandPayload::RecallExpedition {
                faction_id,
                expedition_band_id,
            } => pb::command_envelope::Command::RecallExpedition(pb::RecallExpeditionCommand {
                faction_id: *faction_id,
                expedition_band_id: *expedition_band_id,
            }),
            CommandPayload::SendHuntExpedition {
                faction_id,
                band_id,
                party_workers,
                fauna_id,
                floor,
            } => pb::command_envelope::Command::SendHuntExpedition(pb::SendHuntExpeditionCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                party_workers: *party_workers,
                fauna_id: fauna_id.clone(),
                // Retired by the harvest floor arc; the number is immutable, the value unread.
                policy: None,
                floor: *floor,
            }),
            CommandPayload::ExportMap { path } => {
                pb::command_envelope::Command::ExportMap(pb::ExportMapCommand {
                    path: path.clone(),
                })
            }
            CommandPayload::NewGame {
                preset_id,
                width,
                height,
                seed,
                profile_id,
            } => pb::command_envelope::Command::NewGame(pb::NewGameCommand {
                preset_id: preset_id.clone(),
                width: *width,
                height: *height,
                seed: *seed,
                profile_id: profile_id.clone(),
            }),
        });

        pb::CommandEnvelope {
            command,
            correlation_id: self.correlation_id,
        }
    }

    /// Attempt to build a high-level envelope from the protobuf representation.
    pub fn try_from_proto(proto: pb::CommandEnvelope) -> Result<Self, CommandDecodeError> {
        let payload = match proto.command.ok_or(CommandDecodeError::MissingPayload)? {
            pb::command_envelope::Command::Turn(cmd) => CommandPayload::Turn { steps: cmd.steps },
            pb::command_envelope::Command::ResetMap(cmd) => CommandPayload::ResetMap {
                width: cmd.width,
                height: cmd.height,
            },
            pb::command_envelope::Command::Orders(cmd) => CommandPayload::Orders {
                faction_id: cmd.faction_id,
                directive: OrdersDirective::try_from(cmd.directive)?,
            },
            pb::command_envelope::Command::Rollback(cmd) => {
                CommandPayload::Rollback { tick: cmd.tick }
            }
            pb::command_envelope::Command::UpdateEspionageGenerators(cmd) => {
                let mut updates = Vec::with_capacity(cmd.updates.len());
                for update in cmd.updates {
                    let per_faction = match update.per_faction {
                        Some(value) if value <= u8::MAX as u32 => Some(value as u8),
                        Some(value) => {
                            return Err(CommandDecodeError::InvalidEnum {
                                field: "EspionageGeneratorUpdate.per_faction",
                                value: value as i32,
                            })
                        }
                        None => None,
                    };
                    updates.push(EspionageGeneratorUpdate {
                        template_id: update.template_id,
                        enabled: update.enabled,
                        per_faction,
                    });
                }
                CommandPayload::UpdateEspionageGenerators { updates }
            }
            pb::command_envelope::Command::QueueEspionageMission(cmd) => {
                let target_tier = match cmd.target_tier {
                    Some(value) if value <= u8::MAX as u32 => Some(value as u8),
                    Some(value) => {
                        return Err(CommandDecodeError::InvalidEnum {
                            field: "QueueEspionageMissionCommand.target_tier",
                            value: value as i32,
                        })
                    }
                    None => None,
                };
                CommandPayload::QueueEspionageMission {
                    mission_id: cmd.mission_id,
                    owner_faction: cmd.owner_faction,
                    target_owner_faction: cmd.target_owner_faction,
                    discovery_id: cmd.discovery_id,
                    agent_handle: cmd.agent_handle,
                    target_tier,
                    scheduled_tick: cmd.scheduled_tick,
                }
            }
            pb::command_envelope::Command::UpdateEspionageQueueDefaults(cmd) => {
                let target_tier = match cmd.target_tier {
                    Some(value) if value <= u8::MAX as u32 => Some(value as u8),
                    Some(value) => {
                        return Err(CommandDecodeError::InvalidEnum {
                            field: "UpdateEspionageQueueDefaultsCommand.target_tier",
                            value: value as i32,
                        })
                    }
                    None => None,
                };
                CommandPayload::UpdateEspionageQueueDefaults {
                    scheduled_tick_offset: cmd.scheduled_tick_offset,
                    target_tier,
                }
            }
            pb::command_envelope::Command::UpdateCounterIntelPolicy(cmd) => {
                let policy = security_policy_kind_from_proto(cmd.policy)?;
                CommandPayload::UpdateCounterIntelPolicy {
                    faction: cmd.faction,
                    policy,
                }
            }
            pb::command_envelope::Command::AdjustCounterIntelBudget(cmd) => {
                CommandPayload::AdjustCounterIntelBudget {
                    faction: cmd.faction,
                    reserve: cmd.reserve,
                    delta: cmd.delta,
                }
            }
            pb::command_envelope::Command::ReloadConfig(cmd) => {
                let kind = reload_config_kind_from_proto(cmd.kind)?;
                CommandPayload::ReloadConfig {
                    kind,
                    path: cmd.path,
                }
            }
            pb::command_envelope::Command::SetCrisisAutoSeed(cmd) => {
                CommandPayload::SetCrisisAutoSeed {
                    enabled: cmd.enabled,
                }
            }
            pb::command_envelope::Command::SetFog(cmd) => CommandPayload::SetFogEnabled {
                enabled: cmd.enabled,
            },
            pb::command_envelope::Command::Resync(_) => CommandPayload::Resync,
            pb::command_envelope::Command::SpawnCrisis(cmd) => CommandPayload::SpawnCrisis {
                faction_id: cmd.faction,
                archetype_id: cmd.archetype_id,
            },
            pb::command_envelope::Command::SetStartProfile(cmd) => {
                CommandPayload::SetStartProfile {
                    profile_id: cmd.profile_id,
                }
            }
            pb::command_envelope::Command::ScoutArea(cmd) => CommandPayload::ScoutArea {
                faction_id: cmd.faction_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
                band_id: cmd.band_id,
            },
            pb::command_envelope::Command::FollowHerd(cmd) => CommandPayload::FollowHerd {
                faction_id: cmd.faction_id,
                herd_id: cmd.herd_id,
                policy: cmd.policy,
                band_id: cmd.band_id,
            },
            pb::command_envelope::Command::FoundSettlement(cmd) => {
                CommandPayload::FoundSettlement {
                    faction_id: cmd.faction_id,
                    target_x: cmd.target_x,
                    target_y: cmd.target_y,
                }
            }
            pb::command_envelope::Command::ForageTile(cmd) => CommandPayload::ForageTile {
                faction_id: cmd.faction_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
                module: cmd.module,
                band_id: cmd.band_id,
            },
            pb::command_envelope::Command::HuntGame(cmd) => CommandPayload::HuntGame {
                faction_id: cmd.faction_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
                band_id: cmd.band_id,
            },
            pb::command_envelope::Command::HuntFauna(cmd) => CommandPayload::HuntFauna {
                faction_id: cmd.faction_id,
                herd_id: cmd.herd_id,
                band_id: cmd.band_id,
            },
            pb::command_envelope::Command::Tame(cmd) => CommandPayload::Tame {
                faction_id: cmd.faction_id,
                herd_id: cmd.herd_id,
            },
            pb::command_envelope::Command::Cultivate(cmd) => CommandPayload::Cultivate {
                faction_id: cmd.faction_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
            },
            pb::command_envelope::Command::Sow(cmd) => CommandPayload::Sow {
                faction_id: cmd.faction_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
            },
            pb::command_envelope::Command::Corral(cmd) => CommandPayload::Corral {
                faction_id: cmd.faction_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
            },
            pb::command_envelope::Command::AbandonImprovement(cmd) => {
                CommandPayload::AbandonImprovement {
                    faction_id: cmd.faction_id,
                    kind: cmd.kind,
                    target_x: cmd.target_x,
                    target_y: cmd.target_y,
                    fauna_id: cmd.fauna_id,
                }
            }
            pb::command_envelope::Command::ExtendPen(cmd) => CommandPayload::ExtendPen {
                faction_id: cmd.faction_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
            },
            pb::command_envelope::Command::AnswerFork(cmd) => CommandPayload::AnswerFork {
                faction_id: cmd.faction_id,
                beat_id: cmd.beat_id,
                choice_id: cmd.choice_id,
            },
            pb::command_envelope::Command::CancelOrder(cmd) => CommandPayload::CancelOrder {
                faction_id: cmd.faction_id,
                band_id: cmd.band_id,
                // Absent legitimately means "all", and the wire is not a boundary worth hard-failing
                // on: an unrecognised token degrades to the historical clear-everything behaviour.
                scope: cmd
                    .scope
                    .as_deref()
                    .and_then(CancelScope::parse)
                    .unwrap_or_default(),
            },
            pb::command_envelope::Command::AssignLabor(cmd) => CommandPayload::AssignLabor {
                faction_id: cmd.faction_id,
                band_id: cmd.band_id,
                role: cmd.role,
                workers: cmd.workers,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
                fauna_id: cmd.fauna_id,
                policy: cmd.policy,
                species: cmd.species,
                floor: cmd.floor,
            },
            pb::command_envelope::Command::MoveBand(cmd) => CommandPayload::MoveBand {
                faction_id: cmd.faction_id,
                band_id: cmd.band_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
            },
            pb::command_envelope::Command::SendExpedition(cmd) => CommandPayload::SendExpedition {
                faction_id: cmd.faction_id,
                band_id: cmd.band_id,
                party_workers: cmd.party_workers,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
            },
            pb::command_envelope::Command::RecallExpedition(cmd) => {
                CommandPayload::RecallExpedition {
                    faction_id: cmd.faction_id,
                    expedition_band_id: cmd.expedition_band_id,
                }
            }
            pb::command_envelope::Command::SendHuntExpedition(cmd) => {
                CommandPayload::SendHuntExpedition {
                    faction_id: cmd.faction_id,
                    band_id: cmd.band_id,
                    party_workers: cmd.party_workers,
                    fauna_id: cmd.fauna_id,
                    floor: cmd.floor,
                }
            }
            pb::command_envelope::Command::ExportMap(cmd) => {
                CommandPayload::ExportMap { path: cmd.path }
            }
            pb::command_envelope::Command::NewGame(cmd) => CommandPayload::NewGame {
                preset_id: cmd.preset_id,
                width: cmd.width,
                height: cmd.height,
                seed: cmd.seed,
                profile_id: cmd.profile_id,
            },
        };

        Ok(CommandEnvelope {
            payload,
            correlation_id: proto.correlation_id,
        })
    }
}

impl TryFrom<i32> for OrdersDirective {
    type Error = CommandDecodeError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match pb::OrdersDirective::try_from(value) {
            Ok(pb::OrdersDirective::Ready) => Ok(OrdersDirective::Ready),
            _ => Err(CommandDecodeError::InvalidEnum {
                field: "OrdersDirective",
                value,
            }),
        }
    }
}

impl From<OrdersDirective> for pb::OrdersDirective {
    fn from(value: OrdersDirective) -> Self {
        match value {
            OrdersDirective::Ready => pb::OrdersDirective::Ready,
        }
    }
}

fn orders_directive_to_proto(value: OrdersDirective) -> pb::OrdersDirective {
    value.into()
}

fn security_policy_kind_to_proto(value: SecurityPolicyKind) -> pb::SecurityPolicyKind {
    match value {
        SecurityPolicyKind::Lenient => pb::SecurityPolicyKind::Lenient,
        SecurityPolicyKind::Standard => pb::SecurityPolicyKind::Standard,
        SecurityPolicyKind::Hardened => pb::SecurityPolicyKind::Hardened,
        SecurityPolicyKind::Crisis => pb::SecurityPolicyKind::Crisis,
    }
}

fn reload_config_kind_to_proto(kind: ReloadConfigKind) -> pb::ReloadConfigKind {
    match kind {
        ReloadConfigKind::Simulation => pb::ReloadConfigKind::Simulation,
        ReloadConfigKind::TurnPipeline => pb::ReloadConfigKind::TurnPipeline,
        ReloadConfigKind::SnapshotOverlays => pb::ReloadConfigKind::SnapshotOverlays,
        ReloadConfigKind::CrisisArchetypes => pb::ReloadConfigKind::CrisisArchetypes,
        ReloadConfigKind::CrisisModifiers => pb::ReloadConfigKind::CrisisModifiers,
        ReloadConfigKind::CrisisTelemetry => pb::ReloadConfigKind::CrisisTelemetry,
    }
}

fn security_policy_kind_from_proto(value: i32) -> Result<SecurityPolicyKind, CommandDecodeError> {
    match pb::SecurityPolicyKind::try_from(value) {
        Ok(pb::SecurityPolicyKind::Lenient) => Ok(SecurityPolicyKind::Lenient),
        Ok(pb::SecurityPolicyKind::Standard) => Ok(SecurityPolicyKind::Standard),
        Ok(pb::SecurityPolicyKind::Hardened) => Ok(SecurityPolicyKind::Hardened),
        Ok(pb::SecurityPolicyKind::Crisis) => Ok(SecurityPolicyKind::Crisis),
        _ => Err(CommandDecodeError::InvalidEnum {
            field: "SecurityPolicyKind",
            value,
        }),
    }
}

fn reload_config_kind_from_proto(value: i32) -> Result<ReloadConfigKind, CommandDecodeError> {
    match pb::ReloadConfigKind::try_from(value) {
        Ok(pb::ReloadConfigKind::Simulation) => Ok(ReloadConfigKind::Simulation),
        Ok(pb::ReloadConfigKind::TurnPipeline) => Ok(ReloadConfigKind::TurnPipeline),
        Ok(pb::ReloadConfigKind::SnapshotOverlays) => Ok(ReloadConfigKind::SnapshotOverlays),
        Ok(pb::ReloadConfigKind::CrisisArchetypes) => Ok(ReloadConfigKind::CrisisArchetypes),
        Ok(pb::ReloadConfigKind::CrisisModifiers) => Ok(ReloadConfigKind::CrisisModifiers),
        Ok(pb::ReloadConfigKind::CrisisTelemetry) => Ok(ReloadConfigKind::CrisisTelemetry),
        Ok(pb::ReloadConfigKind::Unspecified) | Err(_) => Err(CommandDecodeError::InvalidEnum {
            field: "ReloadConfigKind",
            value,
        }),
    }
}
