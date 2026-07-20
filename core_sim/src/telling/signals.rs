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

use bevy::prelude::Entity;

use crate::{
    components::{LaborAllocation, PopulationCohort, FOOD},
    culture::{CultureManager, CultureOwner, CultureTraitAxis},
    fauna::{EcologyPhase, HerdRegistry, HERDING_DISCOVERY_ID},
    forage::CULTIVATION_DISCOVERY_ID,
    orders::FactionId,
    resources::{DiscoveryProgressLedger, SimulationTick},
    scalar::Scalar,
    sedentarization::SedentarizationScore,
    sites::DiscoveredSites,
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

/// Every signal id content may reference, other than the per-axis culture family.
const BASE_SIGNALS: [&str; 8] = [
    "turn.index",
    "band.count",
    "provisions.total",
    "sedentarization.score",
    "sites.discovered_this_turn",
    "discovery.progress.cultivation",
    "discovery.progress.herding",
    "fauna.collapsing_group_count",
];

/// Stable snake_case key for a culture axis, forming `culture.axis.<key>`. Written out rather
/// than derived from the enum's debug name so the wire-visible content vocabulary can never
/// shift under a rename.
const fn culture_axis_key(axis: CultureTraitAxis) -> &'static str {
    match axis {
        CultureTraitAxis::PassiveAggressive => "passive_aggressive",
        CultureTraitAxis::OpenClosed => "open_closed",
        CultureTraitAxis::CollectivistIndividualist => "collectivist_individualist",
        CultureTraitAxis::TraditionalistRevisionist => "traditionalist_revisionist",
        CultureTraitAxis::HierarchicalEgalitarian => "hierarchical_egalitarian",
        CultureTraitAxis::SyncreticPurist => "syncretic_purist",
        CultureTraitAxis::AsceticIndulgent => "ascetic_indulgent",
        CultureTraitAxis::PragmaticIdealistic => "pragmatic_idealistic",
        CultureTraitAxis::RationalistMystical => "rationalist_mystical",
        CultureTraitAxis::ExpansionistInsular => "expansionist_insular",
        CultureTraitAxis::AdaptiveStubborn => "adaptive_stubborn",
        CultureTraitAxis::HonorBoundOpportunistic => "honor_bound_opportunistic",
        CultureTraitAxis::MeritOrientedLineageOriented => "merit_oriented_lineage_oriented",
        CultureTraitAxis::SecularDevout => "secular_devout",
        CultureTraitAxis::PluralisticMonocultural => "pluralistic_monocultural",
    }
}

/// Resolve a `culture.axis.*` signal id to its axis, if it names one.
fn culture_axis_for(signal: &str) -> Option<CultureTraitAxis> {
    let key = signal.strip_prefix(CULTURE_AXIS_PREFIX)?;
    CultureTraitAxis::ALL
        .into_iter()
        .find(|axis| culture_axis_key(*axis) == key)
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
    pub discovery_progress: &'a DiscoveryProgressLedger,
    pub herds: &'a HerdRegistry,
    pub culture: &'a CultureManager,
    /// Resident bands of `faction`: `(cohort, its labor allocation)`. Also the noun resolvers'
    /// view of the band layer, so the query is walked once for both.
    pub bands: &'a [BandView<'a>],
    /// Last turn's cumulative discovered-site count, for the `sites.discovered_this_turn` diff.
    pub previous_discovered_total: f64,
}

/// One resident band, as the signal sampler and the noun resolvers see it.
pub struct BandView<'a> {
    /// The band entity — the key its local culture layer is filed under
    /// (`CultureOwner::from_entity`), for the faction culture rollup.
    pub entity: Entity,
    pub cohort: &'a PopulationCohort,
    pub labor: Option<&'a LaborAllocation>,
}

/// Sample every registered signal for the player faction. Called once per turn.
pub fn sample_signals(sources: &SignalSources<'_>) -> (SignalSample, f64) {
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
        (discovered_total - sources.previous_discovered_total).max(0.0),
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
        .map(|band| (CultureOwner::from_entity(band.entity), band.cohort.size))
        .collect();
    let faction_traits = sources.culture.faction_trait_average(&band_weights);
    for axis in CultureTraitAxis::ALL {
        let value = faction_traits[axis.index()] as f64;
        sample.set(
            &format!("{CULTURE_AXIS_PREFIX}{}", culture_axis_key(axis)),
            value,
        );
    }

    (sample, discovered_total)
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

/// Cumulative-discovery bookkeeping key helper, so callers never spell the constant twice.
pub fn discovered_total_from(edge_state: &BTreeMap<String, Scalar>) -> f64 {
    edge_state
        .get(SITES_DISCOVERED_TOTAL_KEY)
        .map(|s| s.to_f32() as f64)
        .unwrap_or(0.0)
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
