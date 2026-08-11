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
    /// **Put a recipe on a band's crafting bench.** The crew is the player's to name — see
    /// [`BENCH_CREW_UNSPECIFIED`].
    ///
    /// *The bench is the assignment* (`docs/plan_crafting_and_materials.md` §7): there is no Crafter
    /// role and no labor target, because crafting always has a subject and is therefore staffed like
    /// a worked source rather than like a standing role. **One job at a time**, so this replaces
    /// whatever the bench was making — and the pile that job had already drawn goes with it.
    SetBench {
        faction_id: u32,
        band_id: u64,
        recipe_id: String,
        /// [`BENCH_CREW_UNSPECIFIED`] leaves the crew exactly as it is — which is the shape the
        /// client always sends, so a staged job starts unstaffed and the player names the number.
        workers: u32,
    },
    /// Take the job off a band's bench and hand its crew back to the idle pool.
    ClearBench {
        faction_id: u32,
        band_id: u64,
    },
    /// Change the crew on a band's running bench, leaving the job and its progress alone.
    BenchCrew {
        faction_id: u32,
        band_id: u64,
        workers: u32,
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
        /// **The kit this crew works under** — an `equipment.json` roster id. `None` = the job's
        /// default. An unknown id, or one whose `jobs` does not cover this role, is a **command
        /// failure with a reason**, never a silent fall back: naming a kit is how the player
        /// compares tiers, so a quiet substitution answers a different question than the one asked.
        /// Ignored by the band-wide roles, which consume no kit component.
        kit_id: Option<String>,
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
    /// **Form a new band** — a resident band splits in two on the tile it is standing on
    /// (`docs/plan_band_fission.md`). `workers` is the player's ONE input; children, elders and
    /// every store divide on the share it implies, so the new band is a smaller copy of the one it
    /// came from rather than a party with a composition of its own.
    SplitBand {
        faction_id: u32,
        band_id: Option<u64>,
        workers: u32,
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
        /// **The kit the party is SENT OUT WITH** — an `equipment.json` roster id, resolved **once**
        /// at launch and carried for the party's whole life. `None` = the hunt job's default; an
        /// unknown id, or one whose `jobs` does not include `hunt`, fails the command with a reason.
        kit_id: Option<String>,
    },
    /// **Outfit and launch a DENIAL RAID** (`docs/plan_denial_raid.md`) — the third expedition verb,
    /// beside Scout and Hunt. Proto field 49.
    ///
    /// **It carries no floor, and cannot be given one.** Its escapement ceiling is the herd's whole
    /// standing stock, so there is no floor to name — the order is *"this herd, this
    /// many people"*. That is why it is its own payload rather than a flag on
    /// [`Self::SendHuntExpedition`]: there is nothing here to validate and nothing to tune.
    ///
    /// **No target faction** — denial is aimed at a herd, not at a player.
    SendDenialRaid {
        faction_id: u32,
        band_id: Option<u64>,
        party_workers: u32,
        fauna_id: String,
        /// **The kit the raid is sent out with** — the one thing there *is* to say about a mission
        /// that carries no floor, because a kit is a property of the **party** rather than of the
        /// mission. Same rule as [`Self::SendHuntExpedition::kit_id`].
        kit_id: Option<String>,
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
    /// **Ask the sim a question and get an answer back on the same socket.** Proto field 50, and the
    /// only payload in this enum that is *answered* rather than *applied*.
    ///
    /// It mutates nothing, so it is deliberately outside the replay log: a logged query would make a
    /// replay re-answer questions nobody asked. `request_id` is the client's own correlation number,
    /// echoed on the [`QueryReplyEnvelope`] that comes back.
    Query {
        request_id: u64,
        query: QueryPayload,
    },
}

/// Which question a [`CommandPayload::Query`] asks. Mirrors the proto `QueryCommand.query` oneof.
#[derive(Debug, Clone, PartialEq)]
pub enum QueryPayload {
    HuntTripForecast(HuntTripForecastQuery),
    DenialRaidForecast(DenialRaidForecastQuery),
}

/// *"What does this party, off this band, carrying this kit, take off this herd at this floor?"*
///
/// Every field is an exact ask, not a sample: the answer is computed for these values and echoes
/// them back on each row so a client can assert it got what it asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct HuntTripForecastQuery {
    pub faction_id: u32,
    /// The asking band's durable `BandId` — its **live** equipment wear prices the answer.
    pub band_id: u64,
    pub herd_id: String,
    /// An `equipment.json` roster id, **required**. Unknown or wrong-job is an error, never a quiet
    /// fall back to the job default.
    pub kit_id: String,
    pub party_workers: u32,
    pub floor: f32,
    /// The sheet's floor presets, answered in the same round trip at the same party size.
    pub preset_floors: Vec<f32>,
    /// **The largest party this band could field** — its idle workers. Bounds the reply's
    /// `useful_cap` plateau scan, which walks `1..=max` **contiguously**; `0` means "do not scan".
    pub max_party_workers: u32,
}

/// The denial twin of [`HuntTripForecastQuery`] — no floor, because the mission carries none.
#[derive(Debug, Clone, PartialEq)]
pub struct DenialRaidForecastQuery {
    pub faction_id: u32,
    pub band_id: u64,
    pub herd_id: String,
    pub kit_id: String,
    pub party_workers: u32,
    /// **The largest party this band could field** — its idle workers. Bounds the `party_needed`
    /// search, which walks `1..=max` and stops at the first party that drives the herd past
    /// recovery; `0` answers the sentinel.
    pub max_party_workers: u32,
}

/// **The server → client answer frame**, written back on the command socket as its own
/// length-prefixed frame. A top-level envelope rather than a `CommandEnvelope` variant: the two
/// directions carry different vocabularies, and nothing but a query is ever answered.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryReplyEnvelope {
    /// Echoes the [`CommandPayload::Query::request_id`] this answers.
    pub request_id: u64,
    pub reply: QueryReply,
}

/// What came back. [`QueryReply::Error`] carries a machine-readable snake_case token
/// ([`query_error`]); the client owns the prose.
#[derive(Debug, Clone, PartialEq)]
pub enum QueryReply {
    HuntTripForecast(HuntTripForecastReply),
    DenialRaidForecast(DenialRaidForecastReply),
    Error(String),
}

/// **The refusal tokens a [`QueryReply::Error`] can carry.** Named constants rather than literals at
/// the raising sites, so the client's match arms and the server's answers cannot drift apart.
pub mod query_error {
    /// The server is idle — no world has been generated yet, so there is nothing to forecast.
    pub const NO_ACTIVE_WORLD: &str = "no_active_world";
    /// No live herd carries the queried id.
    pub const UNKNOWN_HERD: &str = "unknown_herd";
    /// No band of the queried faction carries the queried `BandId`.
    pub const UNKNOWN_BAND: &str = "unknown_band";
    /// The queried `kit_id` names no `equipment.json` roster entry.
    pub const UNKNOWN_KIT: &str = "unknown_kit";
    /// The kit exists but its `jobs` list does not cover hunting.
    pub const KIT_WRONG_JOB: &str = "kit_wrong_job";
    /// A composed or preset floor outside `0..=1`. Rejected, never clamped — the same rule the
    /// launch commands follow.
    pub const INVALID_FLOOR: &str = "invalid_floor";
    /// A party of zero. There is no raid to project, so there is no answer to give.
    pub const INVALID_PARTY: &str = "invalid_party";
}

/// One answered hunt-trip forecast. The wire twin of a row of the retired `HuntTripEstimateState`
/// table, plus the echoed `floor` / `party_workers` that make it self-describing.
#[derive(Debug, Clone, PartialEq)]
pub struct HuntTripRow {
    pub floor: f32,
    pub party_workers: u32,
    /// `0` = the raid never completed inside the forecast horizon; `bound` says which kind of never.
    pub turns_to_fill: u32,
    pub bound: String,
    pub delivers_food: bool,
    /// **Whole** animals killed — a count, typed as one, exactly as the retired
    /// `HuntTripEstimateState` typed it and as [`DenialRow::animals_killed`] types it.
    pub animals_taken: u32,
    pub delivered_food: f32,
    pub wasted_food: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HuntTripForecastReply {
    /// The answer at the exact floor the query named.
    pub at_composed: HuntTripRow,
    /// One row per queried preset floor, in the same order.
    pub per_preset: Vec<HuntTripRow>,
    /// The max-useful party plateau — the LAST party at which the payload still rose, so a stepper
    /// seeds ON it; `0` = no plateau found. See the proto for what the number is and what the client
    /// still owns.
    pub useful_cap: u32,
}

/// One answered denial-raid forecast. The denial twin of [`HuntTripRow`], and like it the row
/// echoes the `party_workers` it was answered for.
#[derive(Debug, Clone, PartialEq)]
pub struct DenialRow {
    pub party_workers: u32,
    pub turns_to_collapse: u32,
    pub turns_to_collapse_low: u32,
    pub turns_to_collapse_high: u32,
    pub outcome: String,
    pub animals_killed: u32,
    pub delivered_food: f32,
    pub wasted_food: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DenialRaidForecastReply {
    pub at_composed: DenialRow,
    /// **The party the sheet opens on** — the smallest party that actually drives this herd past
    /// recovery, found by walking `1..=max_party_workers` and stopping at the first success. Seed
    /// the stepper here: below the requirement a denial raid accomplishes literally nothing however
    /// long it runs, so without it the control is a guessing game.
    ///
    /// **`0` means "NO PARTY YOU CAN FIELD drives this herd down", never "send nobody".** That
    /// meaning changed with the query, and it changed for the better: the retired
    /// `HerdTelemetryState.denialPartyNeeded` searched a *sampled* axis with no notion of who was
    /// asking, so it could name a party the band had no hope of raising and present that as the
    /// answer. This searches to the band's own last worker, so `0` is a fact the player can act on —
    /// *raise more people, or pick another quarry*. Render the answered row's `outcome` beside it,
    /// never a blank, and never a stepper seeded at `0`.
    pub party_needed: u32,
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
    /// The materials table (`materials.json`). Retuning `characteristic_bands` re-partitions every
    /// batch on the map, which is why it is worth staging rather than editing under a running world.
    Materials,
    /// The recipe book (`recipes.json`) — the costs, the `work` values and the grade seams.
    Recipes,
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
        ConfigOverrideKind::Materials,
        ConfigOverrideKind::Recipes,
    ];

    /// The wire spelling, shared with the client's `tuning_manifest.json` `kind` field.
    pub fn as_str(self) -> &'static str {
        match self {
            ConfigOverrideKind::Simulation => "simulation",
            ConfigOverrideKind::Labor => "labor",
            ConfigOverrideKind::Demographics => "demographics",
            ConfigOverrideKind::Expedition => "expedition",
            ConfigOverrideKind::Combat => "combat",
            ConfigOverrideKind::Materials => "materials",
            ConfigOverrideKind::Recipes => "recipes",
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

/// Counter-intelligence security posture controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityPolicyKind {
    Lenient,
    Standard,
    Hardened,
    Crisis,
}

/// **The largest length-prefixed frame the command socket will carry, in EITHER direction.**
///
/// Both ends read a 4-byte little-endian length and then that many bytes: the server's
/// `handle_proto_client` reading a [`CommandEnvelope`], and the client reading a
/// [`QueryReplyEnvelope`] back. A frame past this bound is refused, and on the read path that costs
/// the whole connection — a reader cannot resynchronise mid-stream, because it has no way to tell a
/// length prefix from payload once it is lost.
///
/// # It lives here because it is a property of the PROTOCOL, not of either end
///
/// It used to be a private `const` in the server binary, so the client's bridge restated it. Two
/// independent copies of a bound that both ends must agree on is a defect with a long fuse: nothing
/// fails while frames stay small, and the day one grows past the smaller copy the sender writes a
/// frame the receiver refuses — and drops the connection over it. One definition, beside the
/// envelopes it bounds, is what makes disagreement unrepresentable.
///
/// **It became bidirectional when the socket did.** It was written for a one-way command channel;
/// the query channel added a reply direction on the same stream, and the writer checks an encoded
/// reply against this same bound before framing it — because a reply the far end would refuse as
/// oversized must not go on the wire at all. There it costs one unanswered query instead of the
/// connection.
pub const MAX_PROTO_FRAME: usize = 64 * 1024;

/// **A `set_bench` that names no crew: DO NOT CHANGE THE CREW.**
///
/// Not *"the sim decides"* — the sim never decides this. Labor is the scarce currency and dividing
/// the band is the game's turn-to-turn decision, so how many hands stop hunting to stand at a bench
/// is the player's call and only theirs. An idle bench therefore stages the recipe with **nobody**
/// on it and waits for the stepper; a bench already running a job **keeps the crew standing there**
/// through the swap, because an absent number is not an order to send anyone home.
///
/// The value is `0` because `workers` rides a proto3 scalar, which cannot distinguish an absent
/// field from an explicit zero. No intent is lost by that reading: `bench_crew <n>` is how a player
/// sets an explicit crew — zero included, which is how a bench is stood down without taking the job
/// off it.
pub const BENCH_CREW_UNSPECIFIED: u32 = 0;

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
            CommandPayload::SetBench {
                faction_id,
                band_id,
                recipe_id,
                workers,
            } => pb::command_envelope::Command::SetBench(pb::SetBenchCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                recipe_id: recipe_id.clone(),
                workers: *workers,
            }),
            CommandPayload::ClearBench {
                faction_id,
                band_id,
            } => pb::command_envelope::Command::ClearBench(pb::ClearBenchCommand {
                faction_id: *faction_id,
                band_id: *band_id,
            }),
            CommandPayload::BenchCrew {
                faction_id,
                band_id,
                workers,
            } => pb::command_envelope::Command::BenchCrew(pb::BenchCrewCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                workers: *workers,
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
                kit_id,
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
                kit_id: kit_id.clone(),
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
            CommandPayload::SplitBand {
                faction_id,
                band_id,
                workers,
            } => pb::command_envelope::Command::SplitBand(pb::SplitBandCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                workers: *workers,
            }),
            CommandPayload::SendHuntExpedition {
                faction_id,
                band_id,
                party_workers,
                fauna_id,
                floor,
                kit_id,
            } => pb::command_envelope::Command::SendHuntExpedition(pb::SendHuntExpeditionCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                party_workers: *party_workers,
                fauna_id: fauna_id.clone(),
                // Retired by the harvest floor arc; the number is immutable, the value unread.
                policy: None,
                floor: *floor,
                // Retired with the fill target itself; same rule — the field number is immutable and
                // the value is never written.
                fill_target: None,
                kit_id: kit_id.clone(),
            }),
            CommandPayload::SendDenialRaid {
                faction_id,
                band_id,
                party_workers,
                fauna_id,
                kit_id,
            } => pb::command_envelope::Command::SendDenialRaid(pb::SendDenialRaidCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                party_workers: *party_workers,
                fauna_id: fauna_id.clone(),
                kit_id: kit_id.clone(),
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
            CommandPayload::Query { request_id, query } => {
                pb::command_envelope::Command::Query(pb::QueryCommand {
                    request_id: *request_id,
                    query: Some(match query {
                        QueryPayload::HuntTripForecast(ask) => {
                            pb::query_command::Query::HuntTripForecast(pb::HuntTripForecastQuery {
                                faction_id: ask.faction_id,
                                band_id: ask.band_id,
                                herd_id: ask.herd_id.clone(),
                                kit_id: ask.kit_id.clone(),
                                party_workers: ask.party_workers,
                                floor: ask.floor,
                                preset_floors: ask.preset_floors.clone(),
                                max_party_workers: ask.max_party_workers,
                            })
                        }
                        QueryPayload::DenialRaidForecast(ask) => {
                            pb::query_command::Query::DenialRaidForecast(
                                pb::DenialRaidForecastQuery {
                                    faction_id: ask.faction_id,
                                    band_id: ask.band_id,
                                    herd_id: ask.herd_id.clone(),
                                    kit_id: ask.kit_id.clone(),
                                    party_workers: ask.party_workers,
                                    max_party_workers: ask.max_party_workers,
                                },
                            )
                        }
                    }),
                })
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
            pb::command_envelope::Command::SetBench(cmd) => CommandPayload::SetBench {
                faction_id: cmd.faction_id,
                band_id: cmd.band_id,
                recipe_id: cmd.recipe_id,
                workers: cmd.workers,
            },
            pb::command_envelope::Command::ClearBench(cmd) => CommandPayload::ClearBench {
                faction_id: cmd.faction_id,
                band_id: cmd.band_id,
            },
            pb::command_envelope::Command::BenchCrew(cmd) => CommandPayload::BenchCrew {
                faction_id: cmd.faction_id,
                band_id: cmd.band_id,
                workers: cmd.workers,
            },
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
                kit_id: cmd.kit_id,
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
            pb::command_envelope::Command::SplitBand(cmd) => CommandPayload::SplitBand {
                faction_id: cmd.faction_id,
                band_id: cmd.band_id,
                workers: cmd.workers,
            },
            pb::command_envelope::Command::SendHuntExpedition(cmd) => {
                CommandPayload::SendHuntExpedition {
                    faction_id: cmd.faction_id,
                    band_id: cmd.band_id,
                    party_workers: cmd.party_workers,
                    fauna_id: cmd.fauna_id,
                    floor: cmd.floor,
                    kit_id: cmd.kit_id,
                }
            }
            pb::command_envelope::Command::SendDenialRaid(cmd) => CommandPayload::SendDenialRaid {
                faction_id: cmd.faction_id,
                band_id: cmd.band_id,
                party_workers: cmd.party_workers,
                fauna_id: cmd.fauna_id,
                kit_id: cmd.kit_id,
            },
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
            pb::command_envelope::Command::Query(cmd) => {
                // A query with no question is as empty as an envelope with no command, and fails the
                // same way: there is nothing to answer and nothing to guess.
                let query = match cmd.query.ok_or(CommandDecodeError::MissingPayload)? {
                    pb::query_command::Query::HuntTripForecast(ask) => {
                        QueryPayload::HuntTripForecast(HuntTripForecastQuery {
                            faction_id: ask.faction_id,
                            band_id: ask.band_id,
                            herd_id: ask.herd_id,
                            kit_id: ask.kit_id,
                            party_workers: ask.party_workers,
                            floor: ask.floor,
                            preset_floors: ask.preset_floors,
                            max_party_workers: ask.max_party_workers,
                        })
                    }
                    pb::query_command::Query::DenialRaidForecast(ask) => {
                        QueryPayload::DenialRaidForecast(DenialRaidForecastQuery {
                            faction_id: ask.faction_id,
                            band_id: ask.band_id,
                            herd_id: ask.herd_id,
                            kit_id: ask.kit_id,
                            party_workers: ask.party_workers,
                            max_party_workers: ask.max_party_workers,
                        })
                    }
                };
                CommandPayload::Query {
                    request_id: cmd.request_id,
                    query,
                }
            }
        };

        Ok(CommandEnvelope {
            payload,
            correlation_id: proto.correlation_id,
        })
    }
}

impl QueryReplyEnvelope {
    /// Encode the reply into a protobuf binary frame — what the server's per-connection writer
    /// thread length-prefixes onto the command socket.
    pub fn encode_to_vec(&self) -> Result<Vec<u8>, CommandEncodeError> {
        let proto = self.to_proto();
        let mut buffer = Vec::with_capacity(proto.encoded_len());
        proto.encode(&mut buffer)?;
        Ok(buffer)
    }

    /// Decode a reply frame. The client's half of the round trip, and what the transport test reads
    /// back off the socket.
    pub fn decode(bytes: &[u8]) -> Result<Self, CommandDecodeError> {
        let proto = pb::QueryReplyEnvelope::decode(bytes)?;
        Self::try_from_proto(proto)
    }

    fn to_proto(&self) -> pb::QueryReplyEnvelope {
        let reply = Some(match &self.reply {
            QueryReply::HuntTripForecast(answer) => {
                pb::query_reply_envelope::Reply::HuntTripForecast(pb::HuntTripForecastReply {
                    at_composed: Some(hunt_trip_row_to_proto(&answer.at_composed)),
                    per_preset: answer
                        .per_preset
                        .iter()
                        .map(hunt_trip_row_to_proto)
                        .collect(),
                    useful_cap: answer.useful_cap,
                })
            }
            QueryReply::DenialRaidForecast(answer) => {
                pb::query_reply_envelope::Reply::DenialRaidForecast(pb::DenialRaidForecastReply {
                    at_composed: Some(denial_row_to_proto(&answer.at_composed)),
                    party_needed: answer.party_needed,
                })
            }
            QueryReply::Error(reason) => pb::query_reply_envelope::Reply::Error(pb::QueryError {
                reason: reason.clone(),
            }),
        });
        pb::QueryReplyEnvelope {
            request_id: self.request_id,
            reply,
        }
    }

    fn try_from_proto(proto: pb::QueryReplyEnvelope) -> Result<Self, CommandDecodeError> {
        let reply = match proto.reply.ok_or(CommandDecodeError::MissingPayload)? {
            pb::query_reply_envelope::Reply::HuntTripForecast(answer) => {
                QueryReply::HuntTripForecast(HuntTripForecastReply {
                    at_composed: hunt_trip_row_from_proto(
                        answer
                            .at_composed
                            .ok_or(CommandDecodeError::MissingPayload)?,
                    ),
                    per_preset: answer
                        .per_preset
                        .into_iter()
                        .map(hunt_trip_row_from_proto)
                        .collect(),
                    useful_cap: answer.useful_cap,
                })
            }
            pb::query_reply_envelope::Reply::DenialRaidForecast(answer) => {
                QueryReply::DenialRaidForecast(DenialRaidForecastReply {
                    at_composed: denial_row_from_proto(
                        answer
                            .at_composed
                            .ok_or(CommandDecodeError::MissingPayload)?,
                    ),
                    party_needed: answer.party_needed,
                })
            }
            pb::query_reply_envelope::Reply::Error(error) => QueryReply::Error(error.reason),
        };
        Ok(QueryReplyEnvelope {
            request_id: proto.request_id,
            reply,
        })
    }
}

fn hunt_trip_row_to_proto(row: &HuntTripRow) -> pb::HuntTripRow {
    pb::HuntTripRow {
        floor: row.floor,
        party_workers: row.party_workers,
        turns_to_fill: row.turns_to_fill,
        bound: row.bound.clone(),
        delivers_food: row.delivers_food,
        animals_taken: row.animals_taken,
        delivered_food: row.delivered_food,
        wasted_food: row.wasted_food,
    }
}

fn hunt_trip_row_from_proto(row: pb::HuntTripRow) -> HuntTripRow {
    HuntTripRow {
        floor: row.floor,
        party_workers: row.party_workers,
        turns_to_fill: row.turns_to_fill,
        bound: row.bound,
        delivers_food: row.delivers_food,
        animals_taken: row.animals_taken,
        delivered_food: row.delivered_food,
        wasted_food: row.wasted_food,
    }
}

fn denial_row_to_proto(row: &DenialRow) -> pb::DenialRow {
    pb::DenialRow {
        party_workers: row.party_workers,
        turns_to_collapse: row.turns_to_collapse,
        turns_to_collapse_low: row.turns_to_collapse_low,
        turns_to_collapse_high: row.turns_to_collapse_high,
        outcome: row.outcome.clone(),
        animals_killed: row.animals_killed,
        delivered_food: row.delivered_food,
        wasted_food: row.wasted_food,
    }
}

fn denial_row_from_proto(row: pb::DenialRow) -> DenialRow {
    DenialRow {
        party_workers: row.party_workers,
        turns_to_collapse: row.turns_to_collapse,
        turns_to_collapse_low: row.turns_to_collapse_low,
        turns_to_collapse_high: row.turns_to_collapse_high,
        outcome: row.outcome,
        animals_killed: row.animals_killed,
        delivered_food: row.delivered_food,
        wasted_food: row.wasted_food,
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

fn config_override_kind_to_proto(kind: ConfigOverrideKind) -> pb::ConfigOverrideKind {
    match kind {
        ConfigOverrideKind::Simulation => pb::ConfigOverrideKind::Simulation,
        ConfigOverrideKind::Labor => pb::ConfigOverrideKind::Labor,
        ConfigOverrideKind::Demographics => pb::ConfigOverrideKind::Demographics,
        ConfigOverrideKind::Expedition => pb::ConfigOverrideKind::Expedition,
        ConfigOverrideKind::Combat => pb::ConfigOverrideKind::Combat,
        ConfigOverrideKind::Materials => pb::ConfigOverrideKind::Materials,
        ConfigOverrideKind::Recipes => pb::ConfigOverrideKind::Recipes,
    }
}

fn config_override_kind_from_proto(value: i32) -> Result<ConfigOverrideKind, CommandDecodeError> {
    match pb::ConfigOverrideKind::try_from(value) {
        Ok(pb::ConfigOverrideKind::Simulation) => Ok(ConfigOverrideKind::Simulation),
        Ok(pb::ConfigOverrideKind::Labor) => Ok(ConfigOverrideKind::Labor),
        Ok(pb::ConfigOverrideKind::Demographics) => Ok(ConfigOverrideKind::Demographics),
        Ok(pb::ConfigOverrideKind::Expedition) => Ok(ConfigOverrideKind::Expedition),
        Ok(pb::ConfigOverrideKind::Combat) => Ok(ConfigOverrideKind::Combat),
        Ok(pb::ConfigOverrideKind::Materials) => Ok(ConfigOverrideKind::Materials),
        Ok(pb::ConfigOverrideKind::Recipes) => Ok(ConfigOverrideKind::Recipes),
        Ok(pb::ConfigOverrideKind::Unspecified) | Err(_) => Err(CommandDecodeError::InvalidEnum {
            field: "ConfigOverrideKind",
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

    /// **The split rides the wire as a `BandId`**, and it has to survive the envelope intact: the
    /// command is replayed out of the log after a rollback, where an entity handle would resolve to
    /// nothing but an id still names the same band.
    #[test]
    fn split_band_round_trips_through_the_envelope() {
        let payload = CommandPayload::SplitBand {
            faction_id: 0,
            band_id: Some(9_001),
            workers: 6,
        };
        let envelope = CommandEnvelope {
            payload: payload.clone(),
            correlation_id: None,
        };
        let bytes = envelope.encode_to_vec().expect("encode");
        let decoded = CommandEnvelope::decode(&bytes).expect("decode");
        assert_eq!(decoded.payload, payload);
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
