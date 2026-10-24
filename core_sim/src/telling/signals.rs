//! The signal registry — **the engine/content boundary** of The Telling.
//!
//! Signals are named accessors implemented in Rust and enumerated in one table. Content
//! (`beat_definitions.json`) *composes* signals but cannot invent them, which keeps the surface
//! auditable and every predicate cheap to evaluate (`docs/plan_the_telling.md` §2a).
//!
//! Every signal samples to `f64` **once per turn**, at the top of `telling_tick`, into a
//! [`SignalSample`] map — so every predicate in a turn sees one consistent snapshot and each
//! source is read exactly once.

use std::collections::BTreeMap;

use crate::components::BandId;
use crate::leadership::{BandLeadership, LeaderKind};
use crate::{
    components::{LaborAllocation, PopulationCohort, FOOD},
    connections::GatheringLedger,
    culture::{
        culture_axis_from_key, culture_axis_key, CultureManager, CultureOwner, CultureTraitAxis,
    },
    fauna::{EcologyPhase, HerdRegistry, HERDING_DISCOVERY_ID},
    forage::CULTIVATION_DISCOVERY_ID,
    orders::FactionId,
    resources::{DiscoveryProgressLedger, SimulationTick},
    scalar::Scalar,
    sedentarization::SedentarizationScore,
    sites::DiscoveredSites,
    tribe::TribeRegistry,
};

/// A content-facing signal name (e.g. `"sedentarization.score"`).
pub type SignalId = String;

/// Namespace prefix for the per-axis culture signals.
const CULTURE_AXIS_PREFIX: &str = "culture.axis.";

/// The **cumulative** discovered-site count, retained in the ledger's edge state so
/// `sites.discovered_this_turn` can be expressed as a per-turn diff. Deliberately **not** a
/// registered signal — content sees only the diff — but it rides `edge_state`, so it round-trips
/// through the rollback snapshot for free and a rollback cannot invent a phantom discovery.
pub const SITES_DISCOVERED_TOTAL_KEY: &str = "internal.sites.discovered_total";

/// The **cumulative** gathering count for the player faction, retained in the ledger's edge state
/// so `gatherings.held_this_turn` is a per-turn diff, exactly as [`SITES_DISCOVERED_TOTAL_KEY`] is
/// for sites. Not a registered signal; it round-trips through the rollback snapshot for free.
pub const GATHERINGS_HELD_TOTAL_KEY: &str = "internal.gatherings.held_total";

/// The **cumulative** tribe-formation count for the player faction, the edge-state total
/// `tribes.formed_this_turn` diffs (the [`GATHERINGS_HELD_TOTAL_KEY`] pattern).
pub const TRIBES_FORMED_TOTAL_KEY: &str = "internal.tribes.formed_total";

/// The narrator's attained medium, as a 0-based index into `voice.mediums`. **Set by
/// `telling_tick`, not by [`sample_signals`]** — it is derived from the ledger's attained rung plus
/// this turn's sample, so it is injected after the base sample exists (the `stance.*` idiom). It is
/// a registered signal so the authored `voice.medium_*` beats can gate on it with `crosses`.
pub const VOICE_MEDIUM_INDEX: &str = "voice.medium_index";

/// Every signal id content may reference, other than the per-axis culture family.
const BASE_SIGNALS: [&str; 13] = [
    VOICE_MEDIUM_INDEX,
    "turn.index",
    "band.count",
    "provisions.total",
    "sedentarization.score",
    "sites.discovered_this_turn",
    "gatherings.held_this_turn",
    "tribes.formed_this_turn",
    "leadership.chiefs_emerged_this_turn",
    "leadership.councils_emerged_this_turn",
    "discovery.progress.cultivation",
    "discovery.progress.herding",
    "fauna.collapsing_group_count",
];

/// Resolve a `culture.axis.*` signal id to its axis, if it names one.
fn culture_axis_for(signal: &str) -> Option<CultureTraitAxis> {
    culture_axis_from_key(signal.strip_prefix(CULTURE_AXIS_PREFIX)?)
}

/// Is `signal` a signal the engine can sample? Load-time validation for `when` / `gloss` /
/// `stance.axes[].signal` calls this, so an authoring typo fails at load rather than silently
/// evaluating to zero forever.
pub fn is_registered_signal(signal: &str) -> bool {
    BASE_SIGNALS.contains(&signal) || culture_axis_for(signal).is_some()
}

/// Every registered signal id, in a stable order (base signals then culture axes).
pub fn registered_signals() -> Vec<SignalId> {
    let mut out: Vec<SignalId> = BASE_SIGNALS.iter().map(|s| (*s).to_string()).collect();
    out.extend(
        CultureTraitAxis::ALL
            .into_iter()
            .map(|axis| format!("{CULTURE_AXIS_PREFIX}{}", culture_axis_key(axis))),
    );
    out
}

/// One turn's sample of every registered signal, keyed by signal id.
#[derive(Debug, Clone, Default)]
pub struct SignalSample {
    values: BTreeMap<SignalId, f64>,
}

impl SignalSample {
    /// The turn's value for `signal`. An unregistered id reads `0.0` — unreachable through
    /// validated content, but a sample lookup must never panic mid-turn.
    pub fn get(&self, signal: &str) -> f64 {
        self.values.get(signal).copied().unwrap_or(0.0)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&SignalId, &f64)> {
        self.values.iter()
    }

    pub(crate) fn set(&mut self, signal: &str, value: f64) {
        self.values.insert(signal.to_string(), value);
    }

    /// Build a sample directly from `(signal, value)` pairs — the construction seam that lets the
    /// predicate/selection/stance machinery be exercised (and re-evaluated) without standing up a
    /// whole world.
    pub fn from_pairs<I: IntoIterator<Item = (SignalId, f64)>>(pairs: I) -> Self {
        Self {
            values: pairs.into_iter().collect(),
        }
    }
}

/// Everything the sampler reads, gathered so `telling_tick` reads each source exactly once.
pub struct SignalSources<'a> {
    pub faction: FactionId,
    pub tick: &'a SimulationTick,
    pub sedentarization: &'a SedentarizationScore,
    pub discovered_sites: &'a DiscoveredSites,
    pub gatherings: &'a GatheringLedger,
    pub tribes: &'a TribeRegistry,
    pub discovery_progress: &'a DiscoveryProgressLedger,
    pub herds: &'a HerdRegistry,
    pub culture: &'a CultureManager,
    /// Resident bands of `faction`: `(cohort, its labor allocation)`. Also the noun resolvers'
    /// view of the band layer, so the query is walked once for both.
    pub bands: &'a [BandView<'a>],
    /// Last turn's cumulative totals, for the `sites.discovered_this_turn` and
    /// `gatherings.held_this_turn` diffs.
    pub previous: CumulativeTotals,
}

/// The **cumulative** counts a per-turn signal is a diff of. Kept in the ledger's reserved
/// `internal.*` edge-state keys, so they round-trip through the rollback snapshot for free and a
/// rollback cannot invent a phantom discovery.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CumulativeTotals {
    pub discovered_sites: f64,
    pub gatherings_held: f64,
    pub tribes_formed: f64,
}

impl CumulativeTotals {
    /// Read last turn's totals back out of the ledger's edge state.
    pub fn from_edge_state(edge_state: &BTreeMap<String, Scalar>) -> Self {
        let read = |key: &str| {
            edge_state
                .get(key)
                .map(|s| s.to_f32() as f64)
                .unwrap_or(0.0)
        };
        Self {
            discovered_sites: read(SITES_DISCOVERED_TOTAL_KEY),
            gatherings_held: read(GATHERINGS_HELD_TOTAL_KEY),
            tribes_formed: read(TRIBES_FORMED_TOTAL_KEY),
        }
    }

    /// Write this turn's totals into the edge state, to be next turn's `previous`.
    pub fn commit(&self, edge_state: &mut BTreeMap<String, Scalar>) {
        edge_state.insert(
            SITES_DISCOVERED_TOTAL_KEY.to_string(),
            Scalar::from_f32(self.discovered_sites as f32),
        );
        edge_state.insert(
            GATHERINGS_HELD_TOTAL_KEY.to_string(),
            Scalar::from_f32(self.gatherings_held as f32),
        );
        edge_state.insert(
            TRIBES_FORMED_TOTAL_KEY.to_string(),
            Scalar::from_f32(self.tribes_formed as f32),
        );
    }
}

/// One resident band, as the signal sampler and the noun resolvers see it.
pub struct BandView<'a> {
    /// The band's durable id — the key its own culture layer is filed under
    /// (`CultureOwner::from_band`), which is what the faction culture rollup averages. A band with
    /// no layer yet (it has not been through an `Influence` stage) simply does not vote, and a
    /// faction where none of them do falls through to the global layer.
    pub band: BandId,
    pub cohort: &'a PopulationCohort,
    pub labor: Option<&'a LaborAllocation>,
    /// The band's leader, if it has accepted one (`crate::leadership`).
    pub leadership: Option<&'a BandLeadership>,
}

/// How many of the faction's bands accepted a leader of `kind` on this very turn.
fn emerged_this_turn(sources: &SignalSources<'_>, kind: LeaderKind) -> f64 {
    sources
        .bands
        .iter()
        .filter_map(|band| band.leadership.and_then(|l| l.leader.as_ref()))
        .filter(|leader| leader.kind == kind && leader.since_turn == sources.tick.0)
        .count() as f64
}

/// Sample every registered signal for the player faction. Called once per turn.
pub fn sample_signals(sources: &SignalSources<'_>) -> (SignalSample, CumulativeTotals) {
    let mut sample = SignalSample::default();

    sample.set("turn.index", sources.tick.0 as f64);

    // The band layer: total people, and the summed band-local larders. Provisions left
    // `FactionInventory` entirely in the population-economy arc, so food is read from the bands.
    let mut people: u64 = 0;
    let mut provisions: f64 = 0.0;
    for band in sources.bands {
        people += band.cohort.size as u64;
        provisions += band.cohort.stores.get(FOOD).to_f32().max(0.0) as f64;
    }
    sample.set("band.count", people as f64);
    sample.set("provisions.total", provisions);

    sample.set(
        "sedentarization.score",
        sources.sedentarization.score(sources.faction) as f64,
    );

    // Discovery is a *cumulative* registry, so "this turn" is a diff against last turn's total.
    let discovered_total = sources.discovered_sites.for_faction(sources.faction).len() as f64;
    sample.set(
        "sites.discovered_this_turn",
        (discovered_total - sources.previous.discovered_sites).max(0.0),
    );

    // A leader emerging is read off the leader's own `since_turn`, not diffed against a standing
    // count: a led band lost the same turn (death, merge, leaving the faction) must not mask a
    // new leader elsewhere.
    sample.set(
        "leadership.chiefs_emerged_this_turn",
        emerged_this_turn(sources, LeaderKind::Chief),
    );
    sample.set(
        "leadership.councils_emerged_this_turn",
        emerged_this_turn(sources, LeaderKind::ElderCouncil),
    );

    // Gatherings are a cumulative per-people count too, so "this turn" is the same kind of diff.
    let gatherings_total = sources.gatherings.held_by(sources.faction) as f64;
    sample.set(
        "gatherings.held_this_turn",
        (gatherings_total - sources.previous.gatherings_held).max(0.0),
    );

    // Tribe formations are a cumulative per-people count as well; the same diff.
    let tribes_total = sources.tribes.formed_by(sources.faction) as f64;
    sample.set(
        "tribes.formed_this_turn",
        (tribes_total - sources.previous.tribes_formed).max(0.0),
    );

    for (signal, discovery_id) in [
        ("discovery.progress.cultivation", CULTIVATION_DISCOVERY_ID),
        ("discovery.progress.herding", HERDING_DISCOVERY_ID),
    ] {
        let progress = sources
            .discovery_progress
            .get_progress(sources.faction, discovery_id);
        sample.set(signal, progress.to_f32() as f64);
    }

    let collapsing = sources
        .herds
        .entries()
        .iter()
        .filter(|herd| herd.ecology_phase == EcologyPhase::Collapsing)
        .count();
    sample.set("fauna.collapsing_group_count", collapsing as f64);

    // Culture reads the **faction rollup**: a population-weighted average of this faction's
    // resident bands' local layers, falling back to the global layer when it has none.
    let band_weights: Vec<(CultureOwner, u32)> = sources
        .bands
        .iter()
        .map(|band| (CultureOwner::from_band(band.band), band.cohort.size))
        .collect();
    let faction_traits = sources.culture.faction_trait_average(&band_weights);
    for axis in CultureTraitAxis::ALL {
        let value = faction_traits[axis.index()] as f64;
        sample.set(
            &format!("{CULTURE_AXIS_PREFIX}{}", culture_axis_key(axis)),
            value,
        );
    }

    (
        sample,
        CumulativeTotals {
            discovered_sites: discovered_total,
            gatherings_held: gatherings_total,
            tribes_formed: tribes_total,
        },
    )
}

/// Format a sampled value for the gloss line — "the voice never lies", so this shows the real
/// number. Integral values print bare; fractional ones keep two decimals.
pub fn format_gloss_value(value: f64) -> String {
    /// Below this, a value is close enough to its rounding to print as an integer.
    const INTEGRAL_EPSILON: f64 = 1e-6;
    if (value - value.round()).abs() < INTEGRAL_EPSILON {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_and_culture_signals_are_registered() {
        for signal in BASE_SIGNALS {
            assert!(is_registered_signal(signal), "{signal} should be known");
        }
        assert!(is_registered_signal("culture.axis.secular_devout"));
        assert!(is_registered_signal("culture.axis.ascetic_indulgent"));
    }

    #[test]
    fn unknown_signals_are_rejected() {
        assert!(!is_registered_signal("vibes.total"));
        assert!(!is_registered_signal("culture.axis.nonexistent"));
        assert!(!is_registered_signal(SITES_DISCOVERED_TOTAL_KEY));
    }

    #[test]
    fn registered_signals_covers_every_culture_axis() {
        let all = registered_signals();
        assert_eq!(all.len(), BASE_SIGNALS.len() + CultureTraitAxis::ALL.len());
        for signal in &all {
            assert!(is_registered_signal(signal));
        }
    }

    #[test]
    fn gloss_values_print_integers_bare() {
        assert_eq!(format_gloss_value(41.0), "41");
        assert_eq!(format_gloss_value(0.5), "0.50");
    }
}
