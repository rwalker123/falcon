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

/// Influencer support channels exposed to the command surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportChannel {
    Popular,
    Peer,
    Institutional,
    Humanitarian,
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
