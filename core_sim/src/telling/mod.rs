//! **The Telling** — the narrative beat engine (PR-A: the ambient/beat tiers).
//!
//! Each turn `telling_tick` samples the sim into a consistent [`SignalSample`], evaluates the
//! catalog's edge-gated triggers, dresses the surviving beat in nouns drawn from the live world,
//! and emits it through the existing `CommandEventLog`. The feed stops saying "Sedentarization
//! available" and starts saying "The river-bend remembers us now."
//!
//! Layering (`docs/plan_the_telling.md` §1b):
//!
//! | Layer | Home | Moddable |
//! |---|---|---|
//! | **Engine** — trigger eval, edge gating, selection, noun resolution | this module | no |
//! | **Content** — souls, wardrobes, conditions | `src/data/beat_*.json` | **yes** |
//!
//! The two extension points are the **signal registry** (`signals.rs`) and the **noun resolver
//! registry** (`nouns.rs`): content composes them and cannot invent them, which keeps the
//! surface auditable and every condition cheap.
//!
//! **Determinism.** Selection RNG is seeded per decision as
//! `world_seed ^ tick ^ TELLING_SEED_SALT ^ FnvHasher(beat.id)` — never a rolling stream — so
//! adding a beat to the catalog cannot perturb another beat's roll.
//!
//! **Rollback.** The [`BeatLedger`] round-trips through the snapshot **including restore**, so a
//! rollback past a beat lets that beat fire again rather than leaving it wrongly marked fired.

pub mod catalog;
pub mod config;
pub mod medium;
pub mod memory;
pub mod nouns;
pub mod predicate;
pub mod select;
pub mod signals;
pub mod stance;

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use bevy::{ecs::system::SystemParam, prelude::*};
use tracing::info;

use crate::{
    components::{LaborAllocation, PopulationCohort, ResidentBand, Tile},
    culture::CultureManager,
    fauna::HerdRegistry,
    fauna_config::FaunaConfigHandle,
    mapgen::WorldGenSeed,
    orders::{FactionId, FactionRegistry},
    resources::{
        CommandEventEntry, CommandEventKind, CommandEventLog, DiscoveryProgressLedger,
        SimulationConfig, SimulationTick,
    },
    scalar::Scalar,
    sedentarization::SedentarizationScore,
    sites::DiscoveredSites,
    sites_config::SitesConfigHandle,
};

pub use catalog::{
    load_beat_catalog_from_env, BeatCatalog, BeatCatalogHandle, BeatCatalogMetadata, BeatChoice,
    BeatDefinition, BeatTier, ChoiceWrites, Fit, NounBinding, Remembers, Soul, WardrobeEntry,
};
pub use config::{
    load_beat_config_from_env, BeatConfig, BeatConfigHandle, BeatConfigMetadata, BudgetConfig,
    MemoryConfig, SelectionConfig, StanceAxis, StanceConfig, TrendConfig, VoiceConfig, VoiceMedium,
};
pub use medium::AttainedMedium;
pub use memory::{Thread, ThreadSelector};
pub use nouns::{Noun, NounField};
pub use predicate::{CompareOp, EdgeDir, Predicate};
pub use select::TELLING_SEED_SALT;
pub use signals::{registered_signals, SignalId, SignalSample};

use nouns::NounContext;
use predicate::EvalContext;
use signals::{
    discovered_total_from, format_gloss_value, sample_signals, BandView, SignalSources,
    SITES_DISCOVERED_TOTAL_KEY,
};

/// Persistent narrative memory: what has fired, what the signals read last turn, and which
/// dressings are stale. `BTreeMap`/`BTreeSet` throughout for deterministic iteration and stable
/// snapshot ordering; **`Scalar`, not `f32`**, for anything persisted (rollback bit-exactness).
///
/// Per-turn scratch (the tier budget counters) is **not** here — it is recomputed each turn, so
/// a rehydrated ledger starts the turn neutral (the `fauna.rs` convention).
#[derive(Resource, Debug, Clone, Default)]
pub struct BeatLedger {
    /// Beat id → the ticks it fired on. Backs `once`, cooldowns, and `fired within`.
    fired: BTreeMap<String, Vec<u64>>,
    /// Signal id → **last turn's** sample. Backs `crosses`; an absent entry means "never seen",
    /// which is why a first-ever sample is never a crossing.
    edge_state: BTreeMap<String, Scalar>,
    /// Signal id → rolling samples, capped at `trend.max_history_turns`. Backs `trend`.
    history: BTreeMap<String, VecDeque<Scalar>>,
    /// Wardrobe entry id → the tick it was last used. Backs novelty.
    wardrobe_usage: BTreeMap<String, u64>,
    /// Consequence flags written by answered fork choices (`writes.flags`), readable by the
    /// `{ "flag": F }` predicate.
    flags: BTreeSet<String>,
    /// The **declared stance offsets** — and *only* the offsets. The accreted half is read from
    /// each axis's backing signal, so the effective stance is recomputed every turn
    /// (`telling::stance`). Storing a single number instead would make *resist* unrepresentable.
    stance: BTreeMap<String, Scalar>,
    /// Forks posted and not yet answered. **The server never blocks a turn on one** — the turn
    /// gate is client-side, and `budget.fork_expire_turns` is what keeps this list bounded.
    pending: Vec<PendingFork>,
    /// Beat id → the choice the player took **and when**. The concept's memory ledger in its
    /// smallest useful form: later beats can call back to what was decided, and — because the tick
    /// rides along — to *how long ago*. See [`Answer`].
    answers: BTreeMap<String, Answer>,
    /// Beat id → the tick its `once` guard lifts, from a choice's `rearm_after_turns`.
    rearm_at: BTreeMap<String, u64>,
    /// **The memory ledger proper**: kind → durable threads, each snapshotting its noun at first
    /// sight and *never* re-resolved (`telling::memory`). Bounded per kind by
    /// `memory.max_threads_per_kind`, evicting the least recently referenced.
    threads: BTreeMap<String, Vec<Thread>>,
    /// Faction → the narrator's attained medium (`telling::medium`). **Persisted and monotone** —
    /// a people that learned to write does not forget, so a falling signal never steps it down.
    /// Keyed by faction (unlike the rest of the ledger) because the medium is a property of a
    /// civilization and is exported per faction.
    mediums: BTreeMap<u32, AttainedMedium>,
    /// Faction → axis → **effective** stance, recomputed every turn from the signals plus the
    /// declared offsets. **Derived scratch**: not persisted (it is a pure function of state that
    /// *is*) and excluded from equality, the `LaborAllocation::last_yields` convention. Exists so
    /// the snapshot can export what the player's identity currently reads as without re-sampling.
    last_effective_stance: BTreeMap<u32, BTreeMap<String, f32>>,
}

/// Equality ignores `last_effective_stance` — it is derived per turn, so letting it participate
/// would make two ledgers with identical *state* compare unequal.
impl PartialEq for BeatLedger {
    fn eq(&self, other: &Self) -> bool {
        self.fired == other.fired
            && self.edge_state == other.edge_state
            && self.history == other.history
            && self.wardrobe_usage == other.wardrobe_usage
            && self.flags == other.flags
            && self.stance == other.stance
            && self.pending == other.pending
            && self.answers == other.answers
            && self.rearm_at == other.rearm_at
            && self.threads == other.threads
            && self.mediums == other.mediums
    }
}

impl Eq for BeatLedger {}

/// A decision the player made, and the turn they made it on.
///
/// **The tick is load-bearing, not bookkeeping.** A beat that calls back to an answer almost always
/// means "some time after you said that" — `identity.trail_endures` says *"we have kept our word to
/// it"*, which is absurd the turn after the word was given. Without the tick, the only way to
/// express elapsed time was a `turn.index` trend, which rises unconditionally and therefore means
/// nothing more than "we are past turn 20". `answered`'s `min_turns_since` is the honest version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub choice: String,
    /// The tick the fork was answered on — including an expiry auto-defer, which resolves through
    /// the same path and is deliberately indistinguishable afterwards.
    pub tick: u64,
}

/// One register's rendering of a player-visible string.
pub type VoiceLines = BTreeMap<String, String>;

/// An answer offered by a [`PendingFork`], rendered at post time in every register.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedChoice {
    pub id: String,
    /// Computed here, not re-derived downstream: a consumer must not have to know that an empty
    /// `writes` is what makes a choice a defer.
    pub is_defer: bool,
    pub label: VoiceLines,
    /// The feed line this choice pushes when taken. Rendered now so its nouns stay pinned to the
    /// moment the fork fired.
    pub echo: VoiceLines,
}

/// A fork posted to a faction and awaiting an answer.
///
/// **Every register is rendered at post time.** The register is a live user toggle, so storing one
/// rendered string would freeze the fork in whichever voice happened to be active when it fired.
/// Rendering all of them also pins noun resolution to that moment, which is correct — the herd you
/// were chasing *then* is what the question is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingFork {
    pub beat_id: String,
    pub wardrobe_id: String,
    pub faction: FactionId,
    pub posted_tick: u64,
    pub rendered: VoiceLines,
    pub choices: Vec<RenderedChoice>,
    /// Signal id → the sampled value behind the question. `Scalar`, like everything persisted.
    pub gloss: Vec<(String, Scalar)>,
}

impl PendingFork {
    /// The rendering for `register`, falling back to the default register so a fork can never
    /// render as an empty string.
    pub fn narration(&self, register: &str, default_register: &str) -> Option<&str> {
        self.rendered
            .get(register)
            .or_else(|| self.rendered.get(default_register))
            .map(String::as_str)
    }
}

/// Why an `answer_fork` was refused. Each maps to a distinct player-facing message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForkAnswerError {
    /// No such beat in the catalog.
    UnknownBeat,
    /// The beat exists, but this faction has no fork of it pending.
    NoPendingFork,
    /// The beat exists and is pending, but declares no such choice.
    UnknownChoice,
}

/// What a resolved fork leaves for the caller to announce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForkResolution {
    pub beat_id: String,
    pub choice_id: String,
    pub wardrobe_id: String,
    /// The echo, per register, rendered when the fork was posted.
    pub echo: VoiceLines,
    /// Set when the choice re-armed the beat, for the analytics/detail line.
    pub rearmed_at_tick: Option<u64>,
}

impl ForkResolution {
    /// The feed line for this resolution, in the default register.
    pub fn echo_line(&self, default_register: &str) -> String {
        self.echo
            .get(default_register)
            .or_else(|| self.echo.values().next())
            .cloned()
            .unwrap_or_default()
    }

    /// The command-event detail line — the record of what was decided and what it wrote.
    pub fn detail(&self) -> String {
        let mut detail = format!(
            "beat={} choice={} wardrobe={} tier={}",
            self.beat_id,
            self.choice_id,
            self.wardrobe_id,
            BeatTier::Fork.as_str()
        );
        if let Some(tick) = self.rearmed_at_tick {
            detail.push_str(&format!(" rearms_at={tick}"));
        }
        detail
    }
}

impl BeatLedger {
    /// Has `beat` ever fired?
    pub fn has_fired(&self, beat: &str) -> bool {
        self.fired.contains_key(beat)
    }

    /// The most recent tick `beat` fired on.
    pub fn last_fired(&self, beat: &str) -> Option<u64> {
        self.fired.get(beat).and_then(|ticks| ticks.last().copied())
    }

    /// Every tick `beat` fired on, oldest first.
    pub fn fired_ticks(&self, beat: &str) -> &[u64] {
        self.fired.get(beat).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The tick a wardrobe entry was last used, if ever.
    pub fn wardrobe_last_used(&self, wardrobe_id: &str) -> Option<u64> {
        self.wardrobe_usage.get(wardrobe_id).copied()
    }

    pub fn flags(&self) -> &BTreeSet<String> {
        &self.flags
    }

    /// The **declared offsets** only — see the field docs and `telling::stance`.
    pub fn stance(&self) -> &BTreeMap<String, Scalar> {
        &self.stance
    }

    /// Every fork awaiting an answer, in post order.
    pub fn pending_forks(&self) -> &[PendingFork] {
        &self.pending
    }

    /// The forks awaiting `faction`'s answer.
    pub fn pending_forks_for(&self, faction: FactionId) -> impl Iterator<Item = &PendingFork> {
        self.pending
            .iter()
            .filter(move |fork| fork.faction == faction)
    }

    /// The **effective** stance the last turn computed for `faction` (signal + declared offset).
    /// Empty before the first `telling_tick` of a session — it is derived, not persisted.
    pub fn effective_stance_for(&self, faction: FactionId) -> Option<&BTreeMap<String, f32>> {
        self.last_effective_stance.get(&faction.0)
    }

    /// Every faction's effective stance, in faction order.
    pub fn effective_stance_by_faction(
        &self,
    ) -> impl Iterator<Item = (FactionId, &BTreeMap<String, f32>)> {
        self.last_effective_stance
            .iter()
            .map(|(faction, axes)| (FactionId(*faction), axes))
    }

    /// The choice a beat was answered with, if it has been.
    pub fn answer(&self, beat: &str) -> Option<&str> {
        self.answers.get(beat).map(|answer| answer.choice.as_str())
    }

    /// The tick a beat was answered on, if it has been — the elapsed-time half of a callback.
    pub fn answered_at(&self, beat: &str) -> Option<u64> {
        self.answers.get(beat).map(|answer| answer.tick)
    }

    /// The memory threads, by kind.
    pub fn threads(&self) -> &BTreeMap<String, Vec<Thread>> {
        &self.threads
    }

    /// The threads of one kind, in insertion order.
    pub fn threads_of(&self, kind: &str) -> &[Thread] {
        self.threads.get(kind).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The medium a faction's narrator has attained. Absent before its first `telling_tick`.
    pub fn medium_for(&self, faction: FactionId) -> Option<&AttainedMedium> {
        self.mediums.get(&faction.0)
    }

    /// Every faction's attained medium, in faction order.
    pub fn mediums_by_faction(&self) -> impl Iterator<Item = (FactionId, &AttainedMedium)> {
        self.mediums
            .iter()
            .map(|(faction, medium)| (FactionId(*faction), medium))
    }

    /// Promote a resolved noun into a durable thread. Upsert by key, bounded per kind.
    fn remember_thread(&mut self, kind: &str, noun: &Noun, tick: u64, max_per_kind: u32) {
        memory::remember(&mut self.threads, kind, noun, tick, max_per_kind as usize);
    }

    /// Mark a thread referenced this turn — the eviction clock (least recently *referenced* wins).
    fn touch_thread(&mut self, resolver: &str, tick: u64) {
        let Some((kind, selector)) = memory::parse_thread_resolver(resolver) else {
            return;
        };
        let Some(key) =
            memory::select_thread(&self.threads, kind, selector).map(|thread| thread.key.clone())
        else {
            return;
        };
        memory::touch(&mut self.threads, kind, &key, tick);
    }

    /// Is `beat`'s `once` guard currently down? A `once` fork that was deferred re-arms after the
    /// choice's `rearm_after_turns`, which is what makes "ask me later" mean *the question returns*.
    fn once_guard_holds(&self, beat: &BeatDefinition, tick: u64) -> bool {
        if !beat.once || !self.has_fired(&beat.id) {
            return false;
        }
        self.rearm_at
            .get(&beat.id)
            .is_none_or(|rearm| tick < *rearm)
    }

    fn has_pending_fork(&self, beat: &str, faction: FactionId) -> bool {
        self.pending
            .iter()
            .any(|fork| fork.beat_id == beat && fork.faction == faction)
    }

    fn push_pending_fork(&mut self, fork: PendingFork) {
        self.pending.push(fork);
    }

    /// Take a choice on a pending fork: apply its writes, mark the beat fired **now** (a fork is
    /// fired when *answered*, not when posted — so one that expires unanswered can legitimately
    /// re-post), record the answer, re-arm if asked, and drop the fork from `pending`.
    ///
    /// Also the expiry valve's mechanism: expiring a fork resolves it to its defer choice through
    /// exactly this path, so an auto-defer and a player defer are indistinguishable afterwards.
    pub fn answer_fork(
        &mut self,
        catalog: &BeatCatalog,
        faction: FactionId,
        beat_id: &str,
        choice_id: &str,
        tick: u64,
    ) -> Result<ForkResolution, ForkAnswerError> {
        let beat = catalog.find(beat_id).ok_or(ForkAnswerError::UnknownBeat)?;
        let index = self
            .pending
            .iter()
            .position(|fork| fork.beat_id == beat_id && fork.faction == faction)
            .ok_or(ForkAnswerError::NoPendingFork)?;
        let choice = beat
            .choice(choice_id)
            .ok_or(ForkAnswerError::UnknownChoice)?;
        // The echo comes off the *pending* fork, not the catalog: it was rendered when the fork
        // fired, so its nouns describe the world that asked the question.
        let rendered = self
            .pending
            .get(index)
            .and_then(|fork| fork.choices.iter().find(|c| c.id == choice_id))
            .ok_or(ForkAnswerError::UnknownChoice)?;
        let echo = rendered.echo.clone();

        // 1. Declared stance offsets. Clamped so an offset **alone** can never leave the axis,
        //    independently of whatever the backing signal is doing.
        for (axis, delta) in &choice.writes.stance {
            let entry = self.stance.entry(axis.clone()).or_default();
            let value = (entry.to_f32() + delta).clamp(stance::STANCE_MIN, stance::STANCE_MAX);
            *entry = Scalar::from_f32(value);
        }
        // 2. Consequence flags.
        for flag in &choice.writes.flags {
            self.flags.insert(flag.clone());
        }
        // 3/4. Fired now, remembered, and re-armed if the choice asks for it.
        self.mark_fired(beat_id, tick);
        self.answers.insert(
            beat_id.to_string(),
            Answer {
                choice: choice_id.to_string(),
                tick,
            },
        );
        self.rearm_at.remove(beat_id);
        let rearmed_at_tick = choice.rearm_after_turns.map(|turns| {
            let at = tick.saturating_add(turns as u64);
            self.rearm_at.insert(beat_id.to_string(), at);
            at
        });
        // 5. Off the pending list.
        let fork = self.pending.remove(index);

        Ok(ForkResolution {
            beat_id: beat_id.to_string(),
            choice_id: choice_id.to_string(),
            wardrobe_id: fork.wardrobe_id,
            echo,
            rearmed_at_tick,
        })
    }

    /// **The safety valve.** Auto-resolve every fork older than `fork_expire_turns` to its defer
    /// choice, exactly as though the player had chosen it.
    ///
    /// Forks post for *every* faction, including AI and unattended ones, and the server never
    /// refuses to resolve a turn because one is pending. Without this, a fork posted to a faction
    /// with no client would sit in `pending` forever and accumulate.
    fn expire_pending_forks(
        &mut self,
        catalog: &BeatCatalog,
        tick: u64,
        expire_turns: u32,
    ) -> Vec<(FactionId, ForkResolution)> {
        let expired: Vec<(FactionId, String, String)> = self
            .pending
            .iter()
            .filter(|fork| tick.saturating_sub(fork.posted_tick) >= expire_turns as u64)
            .filter_map(|fork| {
                // Validation guarantees exactly one defer on every loaded fork; a catalog swapped
                // under a live ledger is the only way this is `None`, and dropping the fork on the
                // floor would be worse than leaving it pending.
                let defer = catalog.find(&fork.beat_id)?.defer_choice()?;
                Some((fork.faction, fork.beat_id.clone(), defer.id.clone()))
            })
            .collect();

        expired
            .into_iter()
            .filter_map(|(faction, beat_id, choice_id)| {
                self.answer_fork(catalog, faction, &beat_id, &choice_id, tick)
                    .ok()
                    .map(|resolution| (faction, resolution))
            })
            .collect()
    }

    fn mark_fired(&mut self, beat: &str, tick: u64) {
        self.fired.entry(beat.to_string()).or_default().push(tick);
    }

    fn mark_wardrobe_used(&mut self, wardrobe_id: &str, tick: u64) {
        self.wardrobe_usage.insert(wardrobe_id.to_string(), tick);
    }

    /// Append this turn's samples to the rolling history, trimming to the configured cap.
    fn push_history(&mut self, sample: &SignalSample, max_history_turns: u32) {
        let cap = max_history_turns.max(1) as usize;
        for (signal, value) in sample.iter() {
            let series = self.history.entry(signal.clone()).or_default();
            series.push_back(Scalar::from_f32(*value as f32));
            while series.len() > cap {
                series.pop_front();
            }
        }
    }

    /// Store this turn's samples as *next* turn's `prev`. Called **after** evaluation, so the
    /// edge machinery always compares against the previous turn.
    fn commit_edge_state(&mut self, sample: &SignalSample, discovered_total: f64) {
        for (signal, value) in sample.iter() {
            self.edge_state
                .insert(signal.clone(), Scalar::from_f32(*value as f32));
        }
        self.edge_state.insert(
            SITES_DISCOVERED_TOTAL_KEY.to_string(),
            Scalar::from_f32(discovered_total as f32),
        );
    }

    // --- snapshot round-trip -------------------------------------------------------------

    /// Serialize into the rollback snapshot state (stable ordering by construction).
    pub fn to_state(&self) -> sim_schema::BeatLedgerState {
        sim_schema::BeatLedgerState {
            fired: self
                .fired
                .iter()
                .map(|(beat, ticks)| sim_schema::BeatFiredState {
                    beat: beat.clone(),
                    ticks: ticks.clone(),
                })
                .collect(),
            edge_state: self
                .edge_state
                .iter()
                .map(|(signal, value)| sim_schema::BeatSignalValueState {
                    signal: signal.clone(),
                    value: value.raw(),
                })
                .collect(),
            history: self
                .history
                .iter()
                .map(|(signal, samples)| sim_schema::BeatSignalHistoryState {
                    signal: signal.clone(),
                    samples: samples.iter().map(|s| s.raw()).collect(),
                })
                .collect(),
            wardrobe_usage: self
                .wardrobe_usage
                .iter()
                .map(|(wardrobe, tick)| sim_schema::BeatWardrobeUsageState {
                    wardrobe: wardrobe.clone(),
                    last_used_tick: *tick,
                })
                .collect(),
            flags: self.flags.iter().cloned().collect(),
            stance: self
                .stance
                .iter()
                .map(|(axis, value)| sim_schema::BeatSignalValueState {
                    signal: axis.clone(),
                    value: value.raw(),
                })
                .collect(),
            pending_forks: self.pending.iter().map(pending_fork_to_state).collect(),
            answers: self
                .answers
                .iter()
                .map(|(beat, answer)| sim_schema::BeatAnswerState {
                    beat: beat.clone(),
                    choice: answer.choice.clone(),
                    tick: answer.tick,
                })
                .collect(),
            rearm: self
                .rearm_at
                .iter()
                .map(|(beat, tick)| sim_schema::BeatRearmState {
                    beat: beat.clone(),
                    tick: *tick,
                })
                .collect(),
            threads: self
                .threads
                .values()
                .flatten()
                .map(|thread| sim_schema::BeatThreadState {
                    kind: thread.kind.clone(),
                    key: thread.key.clone(),
                    name: thread.name.clone(),
                    plural: thread.plural.clone(),
                    adjective: thread.adjective.clone(),
                    first_seen_tick: thread.first_seen_tick,
                    last_referenced_tick: thread.last_referenced_tick,
                })
                .collect(),
            mediums: self
                .mediums
                .iter()
                .map(|(faction, medium)| sim_schema::BeatVoiceMediumState {
                    faction: *faction,
                    medium_id: medium.id.clone(),
                    medium_index: medium.index,
                })
                .collect(),
        }
    }

    /// Rebuild from the rollback snapshot state. **This is the half a captured-but-never-restored
    /// resource is missing**: without it a rollback past a beat leaves it marked fired and it can
    /// never fire again.
    pub fn from_state(state: &sim_schema::BeatLedgerState) -> Self {
        Self {
            fired: state
                .fired
                .iter()
                .map(|entry| (entry.beat.clone(), entry.ticks.clone()))
                .collect(),
            edge_state: state
                .edge_state
                .iter()
                .map(|entry| (entry.signal.clone(), Scalar::from_raw(entry.value)))
                .collect(),
            history: state
                .history
                .iter()
                .map(|entry| {
                    (
                        entry.signal.clone(),
                        entry
                            .samples
                            .iter()
                            .copied()
                            .map(Scalar::from_raw)
                            .collect(),
                    )
                })
                .collect(),
            wardrobe_usage: state
                .wardrobe_usage
                .iter()
                .map(|entry| (entry.wardrobe.clone(), entry.last_used_tick))
                .collect(),
            flags: state.flags.iter().cloned().collect(),
            stance: state
                .stance
                .iter()
                .map(|entry| (entry.signal.clone(), Scalar::from_raw(entry.value)))
                .collect(),
            pending: state
                .pending_forks
                .iter()
                .map(pending_fork_from_state)
                .collect(),
            answers: state
                .answers
                .iter()
                .map(|entry| {
                    (
                        entry.beat.clone(),
                        Answer {
                            choice: entry.choice.clone(),
                            tick: entry.tick,
                        },
                    )
                })
                .collect(),
            rearm_at: state
                .rearm
                .iter()
                .map(|entry| (entry.beat.clone(), entry.tick))
                .collect(),
            threads: state.threads.iter().fold(
                BTreeMap::<String, Vec<Thread>>::new(),
                |mut acc, entry| {
                    acc.entry(entry.kind.clone()).or_default().push(Thread {
                        kind: entry.kind.clone(),
                        key: entry.key.clone(),
                        name: entry.name.clone(),
                        plural: entry.plural.clone(),
                        adjective: entry.adjective.clone(),
                        first_seen_tick: entry.first_seen_tick,
                        last_referenced_tick: entry.last_referenced_tick,
                    });
                    acc
                },
            ),
            mediums: state
                .mediums
                .iter()
                .map(|entry| {
                    (
                        entry.faction,
                        AttainedMedium {
                            index: entry.medium_index,
                            id: entry.medium_id.clone(),
                        },
                    )
                })
                .collect(),
            // Derived, not persisted: recomputed by the next `telling_tick`.
            last_effective_stance: BTreeMap::new(),
        }
    }
}

fn voice_lines_to_state(lines: &VoiceLines) -> Vec<sim_schema::BeatVoiceLineState> {
    lines
        .iter()
        .map(|(register, text)| sim_schema::BeatVoiceLineState {
            register: register.clone(),
            text: text.clone(),
        })
        .collect()
}

fn voice_lines_from_state(lines: &[sim_schema::BeatVoiceLineState]) -> VoiceLines {
    lines
        .iter()
        .map(|line| (line.register.clone(), line.text.clone()))
        .collect()
}

fn pending_fork_to_state(fork: &PendingFork) -> sim_schema::BeatPendingForkState {
    sim_schema::BeatPendingForkState {
        beat_id: fork.beat_id.clone(),
        wardrobe_id: fork.wardrobe_id.clone(),
        faction: fork.faction.0,
        posted_tick: fork.posted_tick,
        narration: voice_lines_to_state(&fork.rendered),
        choices: fork
            .choices
            .iter()
            .map(|choice| sim_schema::BeatForkChoiceState {
                choice_id: choice.id.clone(),
                is_defer: choice.is_defer,
                label: voice_lines_to_state(&choice.label),
                echo: voice_lines_to_state(&choice.echo),
            })
            .collect(),
        gloss: fork
            .gloss
            .iter()
            .map(|(signal, value)| sim_schema::BeatSignalValueState {
                signal: signal.clone(),
                value: value.raw(),
            })
            .collect(),
    }
}

fn pending_fork_from_state(state: &sim_schema::BeatPendingForkState) -> PendingFork {
    PendingFork {
        beat_id: state.beat_id.clone(),
        wardrobe_id: state.wardrobe_id.clone(),
        faction: FactionId(state.faction),
        posted_tick: state.posted_tick,
        rendered: voice_lines_from_state(&state.narration),
        choices: state
            .choices
            .iter()
            .map(|choice| RenderedChoice {
                id: choice.choice_id.clone(),
                is_defer: choice.is_defer,
                label: voice_lines_from_state(&choice.label),
                echo: voice_lines_from_state(&choice.echo),
            })
            .collect(),
        gloss: state
            .gloss
            .iter()
            .map(|entry| (entry.signal.clone(), Scalar::from_raw(entry.value)))
            .collect(),
    }
}

/// The read-only world sources `telling_tick` samples. Grouped so the system stays within
/// Bevy's parameter budget and each source is obviously read once.
#[derive(SystemParam)]
pub struct TellingSources<'w, 's> {
    pub tick: Res<'w, SimulationTick>,
    pub config: Res<'w, SimulationConfig>,
    pub world_seed: Option<Res<'w, WorldGenSeed>>,
    pub factions: Res<'w, FactionRegistry>,
    pub sedentarization: Res<'w, SedentarizationScore>,
    pub discovered_sites: Res<'w, DiscoveredSites>,
    pub discovery_progress: Res<'w, DiscoveryProgressLedger>,
    pub herds: Res<'w, HerdRegistry>,
    pub fauna_config: Res<'w, FaunaConfigHandle>,
    pub sites_config: Res<'w, SitesConfigHandle>,
    pub culture: Res<'w, CultureManager>,
    /// Resident bands only — a detached expedition's larder is not the people's larder.
    pub cohorts: Query<
        'w,
        's,
        (
            Entity,
            &'static PopulationCohort,
            Option<&'static LaborAllocation>,
        ),
        With<ResidentBand>,
    >,
    pub tiles: Query<'w, 's, &'static Tile>,
}

/// Render one register-keyed copy block for **every** configured register, falling back to
/// `fallback` for a register the content omits (validated present for the default register; a
/// missing one is a hole in player copy, never braces).
fn render_registers(
    templates: &VoiceLines,
    fallback: &str,
    resolved: &BTreeMap<String, Noun>,
    cfg: &BeatConfig,
) -> VoiceLines {
    cfg.voice
        .registers
        .iter()
        .map(|register| {
            let template = templates
                .get(register)
                .map(String::as_str)
                .unwrap_or(fallback);
            (register.clone(), nouns::render(template, resolved))
        })
        .collect()
}

/// Per-turn tier budget scratch. Recomputed every turn, never persisted.
#[derive(Debug, Default)]
struct TierBudget {
    spent: BTreeMap<&'static str, u32>,
}

impl TierBudget {
    fn spent(&self, tier: BeatTier) -> u32 {
        self.spent.get(tier.as_str()).copied().unwrap_or(0)
    }

    fn spend(&mut self, tier: BeatTier) {
        *self.spent.entry(tier.as_str()).or_insert(0) += 1;
    }
}

/// `TurnStage::Telling` — between Crisis and Finalize, so it sees population, fauna,
/// sedentarization and crisis output, and lands before `Snapshot` so beats reach the client the
/// same turn.
pub fn telling_tick(
    mut ledger: ResMut<BeatLedger>,
    mut event_log: ResMut<CommandEventLog>,
    beat_config: Res<BeatConfigHandle>,
    beat_catalog: Res<BeatCatalogHandle>,
    sources: TellingSources,
) {
    let cfg = beat_config.get();
    let catalog = beat_catalog.get();
    let tick = sources.tick.0;

    // Signals sample for the *player* faction. There is no `player_faction` accessor; the
    // registry's first entry (`FactionId(0)` in practice) is effectively the player, so take it
    // in a stable sorted order rather than inventing one.
    let mut factions: Vec<FactionId> = sources.factions.factions.clone();
    factions.sort_by_key(|f| f.0);
    let Some(faction) = factions.first().copied() else {
        return;
    };

    // Walk the band query once — both the signal sampler and the noun resolvers read it.
    let bands: Vec<BandView<'_>> = sources
        .cohorts
        .iter()
        .filter(|(_, cohort, _)| cohort.faction == faction)
        .map(|(entity, cohort, labor)| BandView {
            entity,
            cohort,
            labor,
        })
        .collect();

    // 1. Sample every signal once, into a consistent snapshot.
    let previous_discovered_total = discovered_total_from(&ledger.edge_state);
    let (sample, discovered_total) = sample_signals(&SignalSources {
        faction,
        tick: &sources.tick,
        sedentarization: &sources.sedentarization,
        discovered_sites: &sources.discovered_sites,
        discovery_progress: &sources.discovery_progress,
        herds: &sources.herds,
        culture: &sources.culture,
        bands: &bands,
        previous_discovered_total,
    });

    // The stance axes are readable signals too (`stance.<axis>`), so they must be in the sample
    // *before* anything evaluates or glosses against them. They are computed rather than sampled:
    // the accreted half comes from each axis's backing signal (already in `sample`), the declared
    // half from the ledger's offsets.
    let effective_stance = stance::effective_stance(&cfg, &sample, ledger.stance());
    let mut sample = sample;
    for (axis, value) in &effective_stance {
        sample.set(&stance::stance_signal_id(axis), *value as f64);
    }
    let sample = sample;

    ledger
        .last_effective_stance
        .insert(faction.0, effective_stance.clone());

    // The **medium** ladder, evaluated once per turn through the same predicate evaluator the beat
    // triggers use, and injected as `voice.medium_index` *before* anything evaluates — the authored
    // `voice.medium_*` beats gate on it with `crosses`, so it must be in the sample by now. It is
    // taken as a max against what was already attained: a people that learned to write does not
    // forget.
    let attained = ledger.mediums.get(&faction.0).cloned().unwrap_or_default();
    let medium = medium::advance(
        &cfg,
        &EvalContext {
            sample: &sample,
            previous: &ledger.edge_state,
            history: &ledger.history,
            fired: &ledger.fired,
            flags: &ledger.flags,
            answers: &ledger.answers,
            threads: &ledger.threads,
            tick,
            trend: &cfg.trend,
        },
        &attained,
    );
    let mut sample = sample;
    sample.set(signals::VOICE_MEDIUM_INDEX, medium.index as f64);
    let sample = sample;
    ledger.mediums.insert(faction.0, medium);

    ledger.push_history(&sample, cfg.trend.max_history_turns);

    // **The safety valve, before anything else this turn.** A fork nobody answered auto-resolves
    // to its defer choice. This is the *only* thing bounding `pending`, because a fork posts to
    // every faction — AI and unattended ones included — and the server never blocks a turn on an
    // answer (the turn gate is client-side; do not add one here).
    for (fork_faction, resolution) in
        ledger.expire_pending_forks(&catalog, tick, cfg.budget.fork_expire_turns)
    {
        info!(
            target: "shadow_scale::analytics",
            event = "telling_fork_expired",
            faction = fork_faction.0,
            beat = %resolution.beat_id,
            choice = %resolution.choice_id,
        );
        event_log.push(CommandEventEntry::new(
            tick,
            CommandEventKind::NarrativeFork,
            fork_faction,
            resolution.echo_line(&cfg.voice.default_register),
            Some(format!("{} resolved=expired", resolution.detail())),
        ));
    }

    // The ground the band is standing on, for `biome.current_dominant` and biome fit gating.
    let current_terrain = nouns::primary_band(&bands)
        .and_then(|cohort| sources.tiles.get(cohort.current_tile).ok())
        .map(|tile| tile.resource_terrain());

    let fauna = sources.fauna_config.get();
    let sites = sources.sites_config.get();
    // The threads the resolvers read. Cloned out of the ledger so the resolution loop can borrow
    // it immutably while the ledger is being written; threads are few and small by construction
    // (`memory.max_threads_per_kind`).
    let threads = ledger.threads.clone();
    let noun_ctx = NounContext {
        faction,
        band_people: sample.get("band.count"),
        current_terrain,
        last_discovered_site: sources.discovered_sites.for_faction(faction).last(),
        sites: &sites,
        bands: &bands,
        herds: &sources.herds,
        fauna: &fauna,
        threads: &threads,
    };

    let world_seed = sources
        .world_seed
        .as_ref()
        .map(|s| s.0)
        .unwrap_or(sources.config.map_seed);

    let mut budget = TierBudget::default();
    // Emissions and fork posts are staged so the ledger stays immutably borrowed during evaluation.
    let mut emissions: Vec<(String, String, BeatTier, String, String)> = Vec::new();
    let mut posts: Vec<PendingFork> = Vec::new();
    /// What a beat that **landed** owes the memory ledger: the threads to write, and the threads a
    /// resolver drew on (so the eviction clock counts callbacks, not sightings).
    struct MemoryWrites {
        remembers: Vec<(String, Noun)>,
        touched: Vec<String>,
    }
    let mut memory_writes: Vec<MemoryWrites> = Vec::new();

    // 2/3/4/5. Candidate filter → noun resolution → weighing → seeded selection, in catalog
    // (authored) order so evaluation is stable.
    for beat in catalog.beats() {
        if ledger.once_guard_holds(beat, tick) {
            continue;
        }
        // A fork already on the table is not re-asked every turn until it is answered.
        if beat.tier == BeatTier::Fork && ledger.has_pending_fork(&beat.id, faction) {
            continue;
        }
        if let (Some(cooldown), Some(last)) = (beat.cooldown_turns, ledger.last_fired(&beat.id)) {
            if tick.saturating_sub(last) < cooldown as u64 {
                continue;
            }
        }
        let tier_cooldown = cfg.budget.global_cooldown_turns.for_tier(beat.tier);
        if tier_cooldown > 0 {
            let tier_last_fired = catalog
                .beats()
                .iter()
                .filter(|other| other.tier == beat.tier)
                .filter_map(|other| ledger.last_fired(&other.id))
                .max();
            if let Some(last) = tier_last_fired {
                if tick.saturating_sub(last) < tier_cooldown as u64 {
                    continue;
                }
            }
        }
        if budget.spent(beat.tier) >= cfg.budget.max_per_turn.for_tier(beat.tier) {
            continue;
        }

        let fires = beat.when.evaluate(&EvalContext {
            sample: &sample,
            previous: &ledger.edge_state,
            history: &ledger.history,
            fired: &ledger.fired,
            flags: &ledger.flags,
            answers: &ledger.answers,
            threads: &ledger.threads,
            tick,
            trend: &cfg.trend,
        });
        if !fires {
            continue;
        }

        // Resolve nouns (with `fallback` chains) for this beat's declared slots, keeping the
        // resolver that actually *won* each slot — a thread resolver only counts as a callback if
        // it is the one that filled the slot, not merely because it sat in a fallback chain.
        let mut resolved: BTreeMap<String, Noun> = BTreeMap::new();
        let mut winning_resolvers: BTreeMap<String, String> = BTreeMap::new();
        for (slot, binding) in &beat.nouns {
            let candidates = [Some(binding.from.as_str()), binding.fallback.as_deref()];
            for resolver in candidates.into_iter().flatten() {
                if let Some(noun) = nouns::resolve(resolver, &noun_ctx) {
                    resolved.insert(slot.clone(), noun);
                    winning_resolvers.insert(slot.clone(), resolver.to_string());
                    break;
                }
            }
        }

        let candidates = select::weigh_wardrobe(
            beat,
            &resolved,
            current_terrain,
            &ledger.wardrobe_usage,
            tick,
            &effective_stance,
            &cfg.selection,
        );
        // Every dressing excluded → the beat silently does not emit, and is **not** marked fired,
        // so it can still land once the world can dress it.
        let Some(entry) = select::select_wardrobe(&candidates, world_seed, tick, &beat.id) else {
            continue;
        };

        // The beat has **landed** (it emits below, or posts as a fork — a fork's nouns are pinned
        // at post time exactly as a thread's are), so it owes the memory ledger: promote whatever
        // `remembers` names, and mark every thread a resolver actually drew on.
        memory_writes.push(MemoryWrites {
            remembers: beat
                .remembers
                .iter()
                .filter_map(|remembers| {
                    resolved
                        .get(&remembers.slot)
                        .map(|noun| (remembers.kind.clone(), noun.clone()))
                })
                .collect(),
            touched: winning_resolvers.into_values().collect(),
        });

        // A **fork** does not push a line to the feed: it posts a decision the player answers.
        // It is deliberately **not** marked fired here — a fork is fired when *answered*, so one
        // that expires unanswered can legitimately re-post.
        if beat.tier == BeatTier::Fork {
            posts.push(PendingFork {
                beat_id: beat.id.clone(),
                wardrobe_id: entry.id.clone(),
                faction,
                posted_tick: tick,
                // Every register, rendered now — the register is a live user toggle.
                rendered: render_registers(&entry.voice, &beat.soul.question, &resolved, &cfg),
                choices: beat
                    .choices
                    .iter()
                    .map(|choice| RenderedChoice {
                        id: choice.id.clone(),
                        is_defer: choice.is_defer(),
                        label: render_registers(&choice.label, &choice.id, &resolved, &cfg),
                        echo: render_registers(&choice.echo, &choice.id, &resolved, &cfg),
                    })
                    .collect(),
                gloss: beat
                    .gloss
                    .iter()
                    .map(|signal| (signal.clone(), Scalar::from_f32(sample.get(signal) as f32)))
                    .collect(),
            });
            budget.spend(beat.tier);
            continue;
        }

        let template = entry
            .voice
            .get(&cfg.voice.default_register)
            // Validated present at load; a missing register would be a hole in player copy, so
            // fall back to the beat's soul question rather than emitting braces.
            .unwrap_or(&beat.soul.question);
        let label = nouns::render(template, &resolved);

        // The gloss is the "voice never lies" proof: the real sampled numbers behind the line.
        let mut detail = beat
            .gloss
            .iter()
            .map(|signal| format!("{signal}={}", format_gloss_value(sample.get(signal))))
            .collect::<Vec<_>>();
        detail.push(format!("tier={}", beat.tier.as_str()));

        emissions.push((
            beat.id.clone(),
            entry.id.clone(),
            beat.tier,
            label,
            detail.join(" "),
        ));
        budget.spend(beat.tier);
    }

    // 6/7. Emit, record, and mirror to analytics.
    for (beat_id, wardrobe_id, tier, label, detail) in emissions {
        event_log.push(CommandEventEntry::new(
            tick,
            CommandEventKind::NarrativeBeat,
            faction,
            label,
            Some(detail),
        ));
        ledger.mark_fired(&beat_id, tick);
        ledger.mark_wardrobe_used(&wardrobe_id, tick);
        info!(
            target: "shadow_scale::analytics",
            event = "telling_beat",
            faction = faction.0,
            beat = %beat_id,
            wardrobe = %wardrobe_id,
            tier = tier.as_str(),
        );
    }

    // Fork posts: onto the pending list, not the feed. The dressing counts as *used* (the player
    // is looking at it right now), but the beat stays unfired until it is answered.
    for fork in posts {
        info!(
            target: "shadow_scale::analytics",
            event = "telling_fork_posted",
            faction = fork.faction.0,
            beat = %fork.beat_id,
            wardrobe = %fork.wardrobe_id,
            tier = BeatTier::Fork.as_str(),
        );
        ledger.mark_wardrobe_used(&fork.wardrobe_id, tick);
        ledger.push_pending_fork(fork);
    }

    // The memory ledger's writes, after everything that read it. A landed beat's `remembers` slots
    // become threads (upsert by key — rediscovering the same site is one thread, not two), and the
    // threads its resolvers drew on have their eviction clock refreshed.
    for writes in memory_writes {
        for resolver in writes.touched {
            ledger.touch_thread(&resolver, tick);
        }
        for (kind, noun) in writes.remembers {
            ledger.remember_thread(&kind, &noun, tick, cfg.memory.max_threads_per_kind);
        }
    }

    // Store this turn's samples as next turn's `prev` — **after** evaluation, so `crosses`
    // always compares against the previous turn.
    ledger.commit_edge_state(&sample, discovered_total);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledger_round_trips_through_snapshot_state() {
        let mut ledger = BeatLedger::default();
        ledger.mark_fired("opening.cold_open", 0);
        ledger.mark_fired("discovery.site_found", 3);
        ledger.mark_fired("discovery.site_found", 9);
        ledger.mark_wardrobe_used("cold_open.bone_ground", 0);
        ledger.flags.insert("went_hungry".to_string());
        ledger
            .stance
            .insert("roam_settle".to_string(), Scalar::from_f32(-0.4));
        let sample = SignalSample::from_pairs([
            ("band.count".to_string(), 31.0),
            ("provisions.total".to_string(), 120.5),
        ]);
        ledger.push_history(&sample, 16);
        ledger.commit_edge_state(&sample, 2.0);

        // The fork tier's half: what is on the table, what was decided, and what re-arms.
        ledger.push_pending_fork(PendingFork {
            beat_id: "sedentarization.soft_drift".to_string(),
            wardrobe_id: "soft_drift.river_bend".to_string(),
            faction: FactionId(0),
            posted_tick: 12,
            rendered: BTreeMap::from([
                (
                    "mythic".to_string(),
                    "The river-bend remembers us.".to_string(),
                ),
                (
                    "warm".to_string(),
                    "The river-bend knows us now.".to_string(),
                ),
            ]),
            choices: vec![RenderedChoice {
                id: "defer".to_string(),
                is_defer: true,
                label: BTreeMap::from([("mythic".to_string(), "Say nothing".to_string())]),
                echo: BTreeMap::from([("mythic".to_string(), "The fires keep it.".to_string())]),
            }],
            gloss: vec![("sedentarization.score".to_string(), Scalar::from_f32(41.5))],
        });
        ledger.answers.insert(
            "some.prior_fork".to_string(),
            Answer {
                choice: "yes_trail".to_string(),
                tick: 12,
            },
        );
        ledger.rearm_at.insert("some.prior_fork".to_string(), 27);

        // PR-C: the memory threads and the attained medium ride the same round-trip.
        ledger.remember_thread(
            "place",
            &Noun::named("Great Peak", "peaks", "peak"),
            4,
            DEFAULT_TEST_THREAD_CAP,
        );
        ledger.remember_thread(
            "beast",
            &Noun::named("Ash Elk", "ash elk", "elk"),
            9,
            DEFAULT_TEST_THREAD_CAP,
        );
        ledger.touch_thread("thread.place.oldest", 31);
        ledger.mediums.insert(
            0,
            AttainedMedium {
                index: 1,
                id: "painted".to_string(),
            },
        );

        let restored = BeatLedger::from_state(&ledger.to_state());
        assert_eq!(
            restored, ledger,
            "the ledger must survive capture + restore"
        );
        assert_eq!(restored.fired_ticks("discovery.site_found"), &[3, 9]);
        assert_eq!(
            restored.wardrobe_last_used("cold_open.bone_ground"),
            Some(0)
        );
        assert_eq!(restored.pending_forks().len(), 1);
        assert_eq!(restored.answer("some.prior_fork"), Some("yes_trail"));
        assert_eq!(
            restored.answered_at("some.prior_fork"),
            Some(12),
            "the answering tick must survive the round-trip — `min_turns_since` reads it"
        );
        // The threads survive with their snapshotted word forms *and* their eviction clock.
        let place = &restored.threads_of("place")[0];
        assert_eq!(place.name, "Great Peak");
        assert_eq!(place.plural, "peaks");
        assert_eq!(place.first_seen_tick, 4);
        assert_eq!(place.last_referenced_tick, 31);
        assert_eq!(restored.threads_of("beast").len(), 1);
        assert_eq!(
            restored.medium_for(FactionId(0)).map(|m| m.index),
            Some(1),
            "the attained medium must rewind with the ledger, not reset to oral"
        );
    }

    /// A cap large enough that the round-trip fixture never trips eviction.
    const DEFAULT_TEST_THREAD_CAP: u32 = 8;

    #[test]
    fn threads_are_bounded_per_kind_by_the_configured_cap() {
        const CAP: u32 = 2;
        let mut ledger = BeatLedger::default();
        for (turn, name) in ["Alpha", "Bravo", "Charlie"].into_iter().enumerate() {
            ledger.remember_thread("place", &Noun::named(name, name, name), turn as u64, CAP);
        }
        assert_eq!(ledger.threads_of("place").len(), CAP as usize);
    }

    #[test]
    fn history_is_capped_at_the_configured_window() {
        let mut ledger = BeatLedger::default();
        for turn in 0..50u32 {
            ledger.push_history(
                &SignalSample::from_pairs([("band.count".to_string(), turn as f64)]),
                16,
            );
        }
        let series = ledger.history.get("band.count").expect("series exists");
        assert_eq!(series.len(), 16);
        assert_eq!(series.back().copied(), Some(Scalar::from_f32(49.0)));
    }

    #[test]
    fn edge_state_carries_the_cumulative_discovery_total_but_not_as_a_signal() {
        let mut ledger = BeatLedger::default();
        ledger.commit_edge_state(&SignalSample::default(), 4.0);
        assert_eq!(discovered_total_from(&ledger.edge_state), 4.0);
        assert!(!signals::is_registered_signal(SITES_DISCOVERED_TOTAL_KEY));
    }
}
