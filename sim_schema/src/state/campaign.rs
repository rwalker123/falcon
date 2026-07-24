//! Campaign-section state: campaign profiles, victory, command events, and The Telling.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CampaignLabel {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_loc_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle_loc_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CampaignProfileState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_loc_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle_loc_key: Option<String>,
    #[serde(default)]
    pub starting_units: Vec<CampaignStartingUnitState>,
    #[serde(default)]
    pub inventory: Vec<CampaignInventoryEntryState>,
    #[serde(default)]
    pub knowledge_tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub survey_radius: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fog_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_food_module: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_food_module: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CampaignInventoryEntryState {
    pub item: String,
    pub quantity: i64,
}

/// Which ticks one narrative beat has fired on (`core_sim::telling::BeatLedger`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BeatFiredState {
    #[serde(default)]
    pub beat: String,
    #[serde(default)]
    pub ticks: Vec<u64>,
}

/// One `signal → value` pair in the beat ledger's edge state (or one `axis → value` pair of the
/// stance vector — the same shape, keyed differently). `value` is **fixed-point raw**
/// (`Scalar::SCALE` = 1.0), so a rollback restores bit-exact samples.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BeatSignalValueState {
    #[serde(default)]
    pub signal: String,
    /// Fixed-point raw (`Scalar::SCALE` = 1.0).
    #[serde(default)]
    pub value: i64,
}

/// One signal's rolling sample history, oldest first, capped at `trend.max_history_turns`.
/// Samples are **fixed-point raw** (`Scalar::SCALE` = 1.0).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BeatSignalHistoryState {
    #[serde(default)]
    pub signal: String,
    #[serde(default)]
    pub samples: Vec<i64>,
}

/// When a wardrobe entry was last used (the novelty memory).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BeatWardrobeUsageState {
    #[serde(default)]
    pub wardrobe: String,
    #[serde(default)]
    pub last_used_tick: u64,
}

/// Authoritative mirror of The Telling's `BeatLedger` — the narrative memory (what fired, what the
/// signals read last turn, which dressings are stale). Round-tripped through the rollback snapshot
/// **including restore**, so a rollback past a beat lets that beat fire again instead of leaving it
/// wrongly marked fired. Every map crosses as a sorted `Vec` so the record is stable.
///
/// Per-turn scratch (the tier budget counters) is deliberately absent — it is recomputed each
/// turn, so a rehydrated ledger starts neutral. Sim-side only; not on the FlatBuffers client
/// stream (beats reach the client as `CommandEvent`s). See `docs/plan_the_telling.md` §3.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BeatLedgerState {
    #[serde(default)]
    pub fired: Vec<BeatFiredState>,
    #[serde(default)]
    pub edge_state: Vec<BeatSignalValueState>,
    #[serde(default)]
    pub history: Vec<BeatSignalHistoryState>,
    #[serde(default)]
    pub wardrobe_usage: Vec<BeatWardrobeUsageState>,
    #[serde(default)]
    pub flags: Vec<String>,
    /// The player's **declared stance offsets** (the fork tier's write-back). Only the offsets are
    /// stored — the effective stance is `normalize(signal) + offset`, recomputed each turn.
    #[serde(default)]
    pub stance: Vec<BeatSignalValueState>,
    /// Forks posted and not yet answered.
    #[serde(default)]
    pub pending_forks: Vec<BeatPendingForkState>,
    /// Beat id → the choice id the player took, so later beats can call back to what was decided.
    #[serde(default)]
    pub answers: Vec<BeatAnswerState>,
    /// Beat id → the tick a `once` beat's guard lifts (the defer branch's `rearm_after_turns`).
    #[serde(default)]
    pub rearm: Vec<BeatRearmState>,
    /// The memory threads — durable nouns later beats can call back to. Flat and kind-grouped by
    /// construction (the ledger iterates a `BTreeMap<kind, Vec<Thread>>`), so the record is stable.
    #[serde(default)]
    pub threads: Vec<BeatThreadState>,
    /// Faction → the narrator's **attained** medium. Persisted because it is monotone: a people
    /// that learned to write does not forget, so the live evaluation takes the max against this.
    #[serde(default)]
    pub mediums: Vec<BeatVoiceMediumState>,
}

/// One memory thread: a noun **snapshotted at first sight and never re-resolved**, so a callback
/// still lands after the herd went extinct or the site fell four hundred turns behind.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BeatThreadState {
    pub kind: String,
    /// Dedupe identity — the resolved noun's `name`.
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub plural: String,
    #[serde(default)]
    pub adjective: String,
    #[serde(default)]
    pub first_seen_tick: u64,
    /// The eviction clock: least recently *referenced* is what gets dropped, not oldest first-seen.
    #[serde(default)]
    pub last_referenced_tick: u64,
}

/// A faction's attained narrator medium, sim-side (the client-facing twin is [`VoiceMediumState`]).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BeatVoiceMediumState {
    pub faction: u32,
    pub medium_id: String,
    #[serde(default)]
    pub medium_index: u32,
}

/// A faction's narrator **medium** on the client stream: oral saga → painted chronicle → written
/// record. Presentational — it changes how the telling *looks*; it does **not** select different
/// copy (see `core_sim/src/telling/medium.rs`).
///
/// `mediumId` is a string (the `species` / `policy` / `register` convention) so adding a medium
/// needs no schema change.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct VoiceMediumState {
    pub faction: u32,
    pub medium_id: String,
    #[serde(default)]
    pub medium_index: u32,
}

/// One register's rendering of a player-visible narrative string.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BeatVoiceLineState {
    pub register: String,
    pub text: String,
}

/// One answer offered by a pending fork, rendered at post time in every register.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BeatForkChoiceState {
    pub choice_id: String,
    #[serde(default)]
    pub is_defer: bool,
    #[serde(default)]
    pub label: Vec<BeatVoiceLineState>,
    /// The line pushed to the feed once this choice is taken. Rendered at post time so the nouns
    /// stay pinned to the moment the fork fired.
    #[serde(default)]
    pub echo: Vec<BeatVoiceLineState>,
}

/// A fork awaiting an answer. Every register is rendered up front, because the register is a live
/// user toggle — storing a single string would freeze the fork in whichever voice was active when
/// it fired.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct BeatPendingForkState {
    pub beat_id: String,
    #[serde(default)]
    pub wardrobe_id: String,
    #[serde(default)]
    pub faction: u32,
    #[serde(default)]
    pub posted_tick: u64,
    #[serde(default)]
    pub narration: Vec<BeatVoiceLineState>,
    #[serde(default)]
    pub choices: Vec<BeatForkChoiceState>,
    /// The sampled signals behind the question ("the voice never lies"), fixed-point like every
    /// other persisted number.
    #[serde(default)]
    pub gloss: Vec<BeatSignalValueState>,
}

/// Per-faction pending narrative forks, on the client stream (the `SedentarizationState` /
/// `DiscoveredSitesState` per-faction shape). Distinct from the sim-side `BeatPendingForkState`:
/// this is what the client renders and answers with `answer_fork`.
///
/// **The turn gate is client-side.** The server never blocks turn resolution on a pending fork —
/// it auto-resolves one to its defer choice after `beat_config.budget.fork_expire_turns`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PendingForksState {
    pub faction: u32,
    #[serde(default)]
    pub forks: Vec<PendingForkState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PendingForkState {
    pub beat_id: String,
    #[serde(default)]
    pub wardrobe_id: String,
    #[serde(default)]
    pub posted_tick: u64,
    /// Every configured register, rendered when the fork fired.
    #[serde(default)]
    pub narration: Vec<VoiceLineState>,
    #[serde(default)]
    pub choices: Vec<ForkChoiceState>,
    #[serde(default)]
    pub gloss: Vec<GlossEntryState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct VoiceLineState {
    pub register: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ForkChoiceState {
    pub choice_id: String,
    #[serde(default)]
    pub label: Vec<VoiceLineState>,
    /// Computed server-side (the choice writes nothing) so the client never has to know what
    /// makes a choice a defer.
    #[serde(default)]
    pub is_defer: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct GlossEntryState {
    pub signal: String,
    pub value: f64,
}

/// A faction's **effective** stance per axis: normalized backing signal + declared offset, in
/// `[-1, 1]`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct StanceState {
    pub faction: u32,
    #[serde(default)]
    pub axes: Vec<StanceAxisState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct StanceAxisState {
    pub axis: String,
    pub value: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BeatAnswerState {
    pub beat: String,
    pub choice: String,
    /// The tick the fork was answered on. Load-bearing: the `answered` predicate's
    /// `min_turns_since` reads it, so a callback can mean "some time after you said that".
    #[serde(default)]
    pub tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct BeatRearmState {
    pub beat: String,
    pub tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CommandEventState {
    pub tick: u64,
    pub kind: String,
    pub faction: u32,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CampaignStartingUnitState {
    pub kind: String,
    pub count: u32,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct VictoryModeSnapshotState {
    pub id: String,
    pub kind: String,
    pub progress: f32,
    pub threshold: f32,
    pub achieved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct VictoryResultState {
    pub mode: String,
    pub faction: u32,
    pub tick: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct VictorySnapshotState {
    #[serde(default)]
    pub modes: Vec<VictoryModeSnapshotState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winner: Option<VictoryResultState>,
}
