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
        target_x: u32,
        target_y: u32,
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
    /// Stage a config-tuning override, applied at the **next** `new_game`. Proto field 47.
    ///
    /// `patch_json` is carried as an opaque string on purpose: it is a *sparse* patch whose shape is
    /// that of the target config, which `sim_runtime` deliberately knows nothing about. The server
    /// merges, validates and installs it.
    SetConfigOverride {
        kind: ConfigOverrideKind,
        patch_json: String,
    },
    /// Drop every staged config override; the next `new_game` boots on the shipped configs.
    /// Proto field 48.
    ClearConfigOverrides,
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

/// Which boot config a staged tuning override applies to.
///
/// Deliberately **not** [`ReloadConfigKind`]: that names the configs the running world can
/// hot-reload, this names the ones the client's tuning manifest can stage for the next `new_game`.
/// The two sets barely overlap and answer different questions, so folding them together would make
/// half of each list nonsense.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigOverrideKind {
    Simulation,
    Labor,
    Demographics,
    Expedition,
    Combat,
}

impl ConfigOverrideKind {
    /// Every kind, in manifest order. Iterating this is what lets a reader (the manifest drift
    /// test, the help text) stay exhaustive without a second hand-maintained list.
    pub const ALL: &'static [ConfigOverrideKind] = &[
        ConfigOverrideKind::Simulation,
        ConfigOverrideKind::Labor,
        ConfigOverrideKind::Demographics,
        ConfigOverrideKind::Expedition,
        ConfigOverrideKind::Combat,
    ];

    /// The wire spelling, shared with the client's `tuning_manifest.json` `kind` field.
    pub fn as_str(self) -> &'static str {
        match self {
            ConfigOverrideKind::Simulation => "simulation",
            ConfigOverrideKind::Labor => "labor",
            ConfigOverrideKind::Demographics => "demographics",
            ConfigOverrideKind::Expedition => "expedition",
            ConfigOverrideKind::Combat => "combat",
        }
    }

    /// Parse a wire spelling. `None` for anything else — an unknown kind is rejected, never
    /// defaulted, because guessing which config a designer meant to retune is the wrong answer.
    pub fn from_wire_str(value: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|kind| kind.as_str() == value)
    }
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
            CommandPayload::Heat {
                target_x,
                target_y,
                delta,
            } => pb::command_envelope::Command::Heat(pb::HeatCommand {
                target_x: *target_x,
                target_y: *target_y,
                delta: *delta,
            }),
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
            CommandPayload::SetConfigOverride { kind, patch_json } => {
                pb::command_envelope::Command::SetConfigOverride(pb::SetConfigOverrideCommand {
                    kind: config_override_kind_to_proto(*kind) as i32,
                    patch_json: patch_json.clone(),
                })
            }
            CommandPayload::ClearConfigOverrides => {
                pb::command_envelope::Command::ClearConfigOverrides(
                    pb::ClearConfigOverridesCommand {},
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
                target_x: cmd.target_x,
                target_y: cmd.target_y,
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
            pb::command_envelope::Command::SetConfigOverride(cmd) => {
                CommandPayload::SetConfigOverride {
                    kind: config_override_kind_from_proto(cmd.kind)?,
                    patch_json: cmd.patch_json,
                }
            }
            pb::command_envelope::Command::ClearConfigOverrides(_) => {
                CommandPayload::ClearConfigOverrides
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

fn config_override_kind_to_proto(kind: ConfigOverrideKind) -> pb::ConfigOverrideKind {
    match kind {
        ConfigOverrideKind::Simulation => pb::ConfigOverrideKind::Simulation,
        ConfigOverrideKind::Labor => pb::ConfigOverrideKind::Labor,
        ConfigOverrideKind::Demographics => pb::ConfigOverrideKind::Demographics,
        ConfigOverrideKind::Expedition => pb::ConfigOverrideKind::Expedition,
        ConfigOverrideKind::Combat => pb::ConfigOverrideKind::Combat,
    }
}

fn config_override_kind_from_proto(value: i32) -> Result<ConfigOverrideKind, CommandDecodeError> {
    match pb::ConfigOverrideKind::try_from(value) {
        Ok(pb::ConfigOverrideKind::Simulation) => Ok(ConfigOverrideKind::Simulation),
        Ok(pb::ConfigOverrideKind::Labor) => Ok(ConfigOverrideKind::Labor),
        Ok(pb::ConfigOverrideKind::Demographics) => Ok(ConfigOverrideKind::Demographics),
        Ok(pb::ConfigOverrideKind::Expedition) => Ok(ConfigOverrideKind::Expedition),
        Ok(pb::ConfigOverrideKind::Combat) => Ok(ConfigOverrideKind::Combat),
        Ok(pb::ConfigOverrideKind::Unspecified) | Err(_) => Err(CommandDecodeError::InvalidEnum {
            field: "ConfigOverrideKind",
            value,
        }),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The patch rides the wire as an opaque string, so the only thing that can break it is the
    /// envelope plumbing — which this covers for the whole staged-override pair.
    #[test]
    fn config_override_commands_round_trip_through_the_envelope() {
        for payload in [
            CommandPayload::SetConfigOverride {
                kind: ConfigOverrideKind::Combat,
                patch_json: r#"{"lethality": 1.5}"#.to_string(),
            },
            CommandPayload::ClearConfigOverrides,
        ] {
            let envelope = CommandEnvelope {
                payload: payload.clone(),
                correlation_id: None,
            };
            let bytes = envelope.encode_to_vec().expect("encode");
            let decoded = CommandEnvelope::decode(&bytes).expect("decode");
            assert_eq!(decoded.payload, payload);
        }
    }

    /// `0` is the protobuf default, so an unset `kind` field decodes as UNSPECIFIED. Accepting it
    /// would silently retune whichever config happened to be first in the table.
    #[test]
    fn an_unspecified_config_override_kind_is_rejected() {
        let envelope = pb::CommandEnvelope {
            correlation_id: None,
            command: Some(pb::command_envelope::Command::SetConfigOverride(
                pb::SetConfigOverrideCommand {
                    kind: pb::ConfigOverrideKind::Unspecified as i32,
                    patch_json: "{}".to_string(),
                },
            )),
        };
        let mut bytes = Vec::new();
        prost::Message::encode(&envelope, &mut bytes).expect("encode");
        assert!(matches!(
            CommandEnvelope::decode(&bytes),
            Err(CommandDecodeError::InvalidEnum {
                field: "ConfigOverrideKind",
                ..
            })
        ));
    }
}
