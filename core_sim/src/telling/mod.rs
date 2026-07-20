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
pub mod nouns;
pub mod predicate;
pub mod select;
pub mod signals;

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
    load_beat_catalog_from_env, BeatCatalog, BeatCatalogHandle, BeatCatalogMetadata,
    BeatDefinition, BeatTier, Fit, NounBinding, Soul, WardrobeEntry,
};
pub use config::{
    load_beat_config_from_env, BeatConfig, BeatConfigHandle, BeatConfigMetadata, BudgetConfig,
    SelectionConfig, StanceAxis, StanceConfig, TrendConfig, VoiceConfig,
};
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
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
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
    /// Consequence flags written by beats. Empty in PR-A (nothing writes one yet).
    flags: BTreeSet<String>,
    /// The player-authored stance vector. **Empty in PR-A** — populated by PR-B's fork tier.
    stance: BTreeMap<String, Scalar>,
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

    pub fn stance(&self) -> &BTreeMap<String, Scalar> {
        &self.stance
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
        }
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
        (&'static PopulationCohort, Option<&'static LaborAllocation>),
        With<ResidentBand>,
    >,
    pub tiles: Query<'w, 's, &'static Tile>,
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
        .filter(|(cohort, _)| cohort.faction == faction)
        .map(|(cohort, labor)| BandView { cohort, labor })
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
    ledger.push_history(&sample, cfg.trend.max_history_turns);

    // The ground the band is standing on, for `biome.current_dominant` and biome fit gating.
    let current_terrain = nouns::primary_band(&bands)
        .and_then(|cohort| sources.tiles.get(cohort.current_tile).ok())
        .map(|tile| tile.resource_terrain());

    let fauna = sources.fauna_config.get();
    let sites = sources.sites_config.get();
    let noun_ctx = NounContext {
        faction,
        band_people: sample.get("band.count"),
        current_terrain,
        last_discovered_site: sources.discovered_sites.for_faction(faction).last(),
        sites: &sites,
        bands: &bands,
        herds: &sources.herds,
        fauna: &fauna,
    };

    let world_seed = sources
        .world_seed
        .as_ref()
        .map(|s| s.0)
        .unwrap_or(sources.config.map_seed);

    let mut budget = TierBudget::default();
    // Emissions are staged so the ledger stays immutably borrowed during evaluation.
    let mut emissions: Vec<(String, String, BeatTier, String, String)> = Vec::new();

    // 2/3/4/5. Candidate filter → noun resolution → weighing → seeded selection, in catalog
    // (authored) order so evaluation is stable.
    for beat in catalog.beats() {
        if beat.once && ledger.has_fired(&beat.id) {
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
            tick,
            trend: &cfg.trend,
        });
        if !fires {
            continue;
        }

        // Resolve nouns (with `fallback` chains) for this beat's declared slots.
        let mut resolved: BTreeMap<String, Noun> = BTreeMap::new();
        for (slot, binding) in &beat.nouns {
            let noun = nouns::resolve(&binding.from, &noun_ctx).or_else(|| {
                binding
                    .fallback
                    .as_deref()
                    .and_then(|fallback| nouns::resolve(fallback, &noun_ctx))
            });
            if let Some(noun) = noun {
                resolved.insert(slot.clone(), noun);
            }
        }

        let candidates = select::weigh_wardrobe(
            beat,
            &resolved,
            current_terrain,
            &ledger.wardrobe_usage,
            tick,
            &cfg.selection,
        );
        // Every dressing excluded → the beat silently does not emit, and is **not** marked fired,
        // so it can still land once the world can dress it.
        let Some(entry) = select::select_wardrobe(&candidates, world_seed, tick, &beat.id) else {
            continue;
        };

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
