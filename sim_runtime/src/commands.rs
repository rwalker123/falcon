use std::convert::TryFrom;

use prost::Message;
use thiserror::Error;

use crate::{CorruptionSubsystem, InfluenceScopeKind};

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
    Heat {
        entity_bits: u64,
        delta: i64,
    },
    Orders {
        faction_id: u32,
        directive: OrdersDirective,
    },
    Rollback {
        tick: u64,
    },
    AxisBias {
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
    SupportInfluencerChannel {
        id: u32,
        channel: SupportChannel,
        magnitude: f32,
    },
    SpawnInfluencer {
        scope: Option<InfluenceScopeKind>,
        generation: Option<u16>,
    },
    InjectCorruption {
        subsystem: CorruptionSubsystem,
        intensity: f32,
        exposure_timer: u32,
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
        band_entity_bits: Option<u64>,
    },
    FollowHerd {
        faction_id: u32,
        herd_id: String,
    },
    FoundCamp {
        faction_id: u32,
        target_x: u32,
        target_y: u32,
    },
    ForageTile {
        faction_id: u32,
        target_x: u32,
        target_y: u32,
        module: String,
        band_entity_bits: Option<u64>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct EspionageGeneratorUpdate {
    pub template_id: String,
    pub enabled: Option<bool>,
    pub per_faction: Option<u8>,
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

/// Influencer support channels exposed to the command surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportChannel {
    Popular,
    Peer,
    Institutional,
    Humanitarian,
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
    #[error("generation id {value} exceeds u16 range")]
    GenerationOverflow { value: u32 },
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
            CommandPayload::Heat { entity_bits, delta } => {
                pb::command_envelope::Command::Heat(pb::HeatCommand {
                    entity_bits: *entity_bits,
                    delta: *delta,
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
            CommandPayload::AxisBias { axis, value } => {
                pb::command_envelope::Command::AxisBias(pb::AxisBiasCommand {
                    axis: *axis,
                    value: *value,
                })
            }
            CommandPayload::SupportInfluencer { id, magnitude } => {
                pb::command_envelope::Command::SupportInfluencer(pb::SupportInfluencerCommand {
                    id: *id,
                    magnitude: *magnitude,
                })
            }
            CommandPayload::SuppressInfluencer { id, magnitude } => {
                pb::command_envelope::Command::SuppressInfluencer(pb::SuppressInfluencerCommand {
                    id: *id,
                    magnitude: *magnitude,
                })
            }
            CommandPayload::SupportInfluencerChannel {
                id,
                channel,
                magnitude,
            } => {
                pb::command_envelope::Command::SupportChannel(pb::SupportInfluencerChannelCommand {
                    id: *id,
                    channel: support_channel_to_proto(*channel) as i32,
                    magnitude: *magnitude,
                })
            }
            CommandPayload::SpawnInfluencer { scope, generation } => {
                pb::command_envelope::Command::SpawnInfluencer(pb::SpawnInfluencerCommand {
                    scope: scope.map(influence_scope_to_proto).map(|v| v as i32),
                    generation: generation.map(|value| value as u32),
                })
            }
            CommandPayload::InjectCorruption {
                subsystem,
                intensity,
                exposure_timer,
            } => pb::command_envelope::Command::InjectCorruption(pb::InjectCorruptionCommand {
                subsystem: corruption_subsystem_to_proto(*subsystem) as i32,
                intensity: *intensity,
                exposure_timer: *exposure_timer,
            }),
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
                band_entity_bits,
            } => pb::command_envelope::Command::ScoutArea(pb::ScoutAreaCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
                band_entity_bits: *band_entity_bits,
            }),
            CommandPayload::FollowHerd {
                faction_id,
                herd_id,
            } => pb::command_envelope::Command::FollowHerd(pb::FollowHerdCommand {
                faction_id: *faction_id,
                herd_id: herd_id.clone(),
            }),
            CommandPayload::FoundCamp {
                faction_id,
                target_x,
                target_y,
            } => pb::command_envelope::Command::FoundCamp(pb::FoundCampCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
            }),
            CommandPayload::ForageTile {
                faction_id,
                target_x,
                target_y,
                module,
                band_entity_bits,
            } => pb::command_envelope::Command::ForageTile(pb::ForageTileCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
                module: module.clone(),
                band_entity_bits: *band_entity_bits,
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
            pb::command_envelope::Command::Heat(cmd) => CommandPayload::Heat {
                entity_bits: cmd.entity_bits,
                delta: cmd.delta,
            },
            pb::command_envelope::Command::Orders(cmd) => CommandPayload::Orders {
                faction_id: cmd.faction_id,
                directive: OrdersDirective::try_from(cmd.directive)?,
            },
            pb::command_envelope::Command::Rollback(cmd) => {
                CommandPayload::Rollback { tick: cmd.tick }
            }
            pb::command_envelope::Command::AxisBias(cmd) => CommandPayload::AxisBias {
                axis: cmd.axis,
                value: cmd.value,
            },
            pb::command_envelope::Command::SupportInfluencer(cmd) => {
                CommandPayload::SupportInfluencer {
                    id: cmd.id,
                    magnitude: cmd.magnitude,
                }
            }
            pb::command_envelope::Command::SuppressInfluencer(cmd) => {
                CommandPayload::SuppressInfluencer {
                    id: cmd.id,
                    magnitude: cmd.magnitude,
                }
            }
            pb::command_envelope::Command::SupportChannel(cmd) => {
                let channel = SupportChannel::try_from(cmd.channel)?;
                CommandPayload::SupportInfluencerChannel {
                    id: cmd.id,
                    channel,
                    magnitude: cmd.magnitude,
                }
            }
            pb::command_envelope::Command::SpawnInfluencer(cmd) => {
                let scope = match cmd.scope {
                    Some(value) => Some(influence_scope_from_proto(value)?),
                    None => None,
                };
                let generation = match cmd.generation {
                    Some(value) => {
                        if value > u16::MAX as u32 {
                            return Err(CommandDecodeError::GenerationOverflow { value });
                        }
                        Some(value as u16)
                    }
                    None => None,
                };
                CommandPayload::SpawnInfluencer { scope, generation }
            }
            pb::command_envelope::Command::InjectCorruption(cmd) => {
                let subsystem = corruption_subsystem_from_proto(cmd.subsystem)?;
                CommandPayload::InjectCorruption {
                    subsystem,
                    intensity: cmd.intensity,
                    exposure_timer: cmd.exposure_timer,
                }
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
                band_entity_bits: cmd.band_entity_bits,
            },
            pb::command_envelope::Command::FollowHerd(cmd) => CommandPayload::FollowHerd {
                faction_id: cmd.faction_id,
                herd_id: cmd.herd_id,
            },
            pb::command_envelope::Command::FoundCamp(cmd) => CommandPayload::FoundCamp {
                faction_id: cmd.faction_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
            },
            pb::command_envelope::Command::ForageTile(cmd) => CommandPayload::ForageTile {
                faction_id: cmd.faction_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
                module: cmd.module,
                band_entity_bits: cmd.band_entity_bits,
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

impl TryFrom<i32> for SupportChannel {
    type Error = CommandDecodeError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match pb::SupportChannel::try_from(value) {
            Ok(pb::SupportChannel::Popular) => Ok(SupportChannel::Popular),
            Ok(pb::SupportChannel::Peer) => Ok(SupportChannel::Peer),
            Ok(pb::SupportChannel::Institutional) => Ok(SupportChannel::Institutional),
            Ok(pb::SupportChannel::Humanitarian) => Ok(SupportChannel::Humanitarian),
            _ => Err(CommandDecodeError::InvalidEnum {
                field: "SupportChannel",
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

fn support_channel_to_proto(value: SupportChannel) -> pb::SupportChannel {
    match value {
        SupportChannel::Popular => pb::SupportChannel::Popular,
        SupportChannel::Peer => pb::SupportChannel::Peer,
        SupportChannel::Institutional => pb::SupportChannel::Institutional,
        SupportChannel::Humanitarian => pb::SupportChannel::Humanitarian,
    }
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

fn influence_scope_to_proto(value: InfluenceScopeKind) -> pb::InfluenceScopeKind {
    match value {
        InfluenceScopeKind::Local => pb::InfluenceScopeKind::Local,
        InfluenceScopeKind::Regional => pb::InfluenceScopeKind::Regional,
        InfluenceScopeKind::Global => pb::InfluenceScopeKind::Global,
        InfluenceScopeKind::Generation => pb::InfluenceScopeKind::Generation,
    }
}

fn influence_scope_from_proto(value: i32) -> Result<InfluenceScopeKind, CommandDecodeError> {
    match pb::InfluenceScopeKind::try_from(value) {
        Ok(pb::InfluenceScopeKind::Local) => Ok(InfluenceScopeKind::Local),
        Ok(pb::InfluenceScopeKind::Regional) => Ok(InfluenceScopeKind::Regional),
        Ok(pb::InfluenceScopeKind::Global) => Ok(InfluenceScopeKind::Global),
        Ok(pb::InfluenceScopeKind::Generation) => Ok(InfluenceScopeKind::Generation),
        _ => Err(CommandDecodeError::InvalidEnum {
            field: "InfluenceScopeKind",
            value,
        }),
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

fn corruption_subsystem_to_proto(value: CorruptionSubsystem) -> pb::CorruptionSubsystem {
    match value {
        CorruptionSubsystem::Logistics => pb::CorruptionSubsystem::Logistics,
        CorruptionSubsystem::Trade => pb::CorruptionSubsystem::Trade,
        CorruptionSubsystem::Military => pb::CorruptionSubsystem::Military,
        CorruptionSubsystem::Governance => pb::CorruptionSubsystem::Governance,
    }
}

fn corruption_subsystem_from_proto(value: i32) -> Result<CorruptionSubsystem, CommandDecodeError> {
    match pb::CorruptionSubsystem::try_from(value) {
        Ok(pb::CorruptionSubsystem::Logistics) => Ok(CorruptionSubsystem::Logistics),
        Ok(pb::CorruptionSubsystem::Trade) => Ok(CorruptionSubsystem::Trade),
        Ok(pb::CorruptionSubsystem::Military) => Ok(CorruptionSubsystem::Military),
        Ok(pb::CorruptionSubsystem::Governance) => Ok(CorruptionSubsystem::Governance),
        _ => Err(CommandDecodeError::InvalidEnum {
            field: "CorruptionSubsystem",
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
