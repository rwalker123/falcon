//! Stance — **accreted signal + declared offset** (`docs/plan_the_telling.md` §1c, concept §6).
//!
//! The concept's flow is *accrete → name → ratify or resist → steer*, so a stance axis has two
//! contributions and the engine keeps them apart:
//!
//! - the **accreted value** — what the player's behaviour actually says, read from the axis's
//!   backing signal (`beat_config.json` `stance.axes[].signal`) and normalized from that axis's
//!   configured `range` onto `[-1, 1]`;
//! - the **declared offset** — what the player *said* when they answered a fork, accumulated in
//!   [`BeatLedger::stance`](super::BeatLedger) from each choice's `writes.stance`.
//!
//! ```text
//! effective_stance(axis) = clamp(normalize(signal) + declared_offset(axis), -1.0, 1.0)
//! ```
//!
//! **The ledger stores only the offsets** — the part that is not derivable — and the effective
//! value is computed at sample time. That split is what makes *resist* representable: a player
//! whose sedentarization is climbing but who answered "we are the trail" carries a negative offset
//! pulling against their own behaviour, and the tension survives instead of being averaged away by
//! a single stored number.
//!
//! Every axis is also exposed as a readable `stance.<axis>` signal, so `gloss` can print it and
//! future beats can gate on it.

use std::collections::BTreeMap;

use crate::scalar::Scalar;

use super::{
    catalog::WardrobeEntry,
    config::{BeatConfig, SelectionConfig, StanceAxis},
    signals::SignalSample,
};

/// Namespace prefix for the per-axis stance signals.
pub const STANCE_SIGNAL_PREFIX: &str = "stance.";

/// Bounds of every stance value — accreted, declared, and effective alike.
pub const STANCE_MIN: f32 = -1.0;
pub const STANCE_MAX: f32 = 1.0;

/// The signal id an axis is readable under.
pub fn stance_signal_id(axis_id: &str) -> String {
    format!("{STANCE_SIGNAL_PREFIX}{axis_id}")
}

/// The configured axis a `stance.*` signal id names, if any.
pub fn axis_for_signal<'a>(signal: &str, config: &'a BeatConfig) -> Option<&'a StanceAxis> {
    let id = signal.strip_prefix(STANCE_SIGNAL_PREFIX)?;
    config.stance.axes.iter().find(|axis| axis.id == id)
}

/// Is `signal` a readable stance signal under this config? The stance family is **config-driven**
/// (unlike the static registry in `signals.rs`), so it is resolved wherever the config is in hand.
pub fn is_stance_signal(signal: &str, config: &BeatConfig) -> bool {
    axis_for_signal(signal, config).is_some()
}

/// Is `axis_id` a configured stance axis? Backs the load-time checks on `soul.fork` and on every
/// `writes.stance` key.
pub fn is_configured_axis(axis_id: &str, config: &BeatConfig) -> bool {
    config.stance.axes.iter().any(|axis| axis.id == axis_id)
}

/// Map a backing signal's raw value onto `[-1, 1]` through the axis's configured `range`.
/// `range[0] < range[1]` is a validated config invariant, so this cannot divide by zero.
pub fn normalize(value: f64, range: [f32; 2]) -> f32 {
    let [lo, hi] = range;
    let fraction = ((value as f32 - lo) / (hi - lo)).clamp(0.0, 1.0);
    (fraction * 2.0 - 1.0).clamp(STANCE_MIN, STANCE_MAX)
}

/// The **declared offset** for one axis (the ledger's stored half), clamped to the stance bounds.
pub fn declared_offset(offsets: &BTreeMap<String, Scalar>, axis_id: &str) -> f32 {
    offsets
        .get(axis_id)
        .map(|value| value.to_f32().clamp(STANCE_MIN, STANCE_MAX))
        .unwrap_or(0.0)
}

/// Every axis's **effective** stance: normalized signal + declared offset, clamped.
pub fn effective_stance(
    config: &BeatConfig,
    sample: &SignalSample,
    offsets: &BTreeMap<String, Scalar>,
) -> BTreeMap<String, f32> {
    config
        .stance
        .axes
        .iter()
        .map(|axis| {
            let accreted = normalize(sample.get(&axis.signal), axis.range);
            let value =
                (accreted + declared_offset(offsets, &axis.id)).clamp(STANCE_MIN, STANCE_MAX);
            (axis.id.clone(), value)
        })
        .collect()
}

/// The selection weight's **re-coloring** term (`docs/plan_the_telling.md` §4):
///
/// ```text
/// 1.0 + stance_affinity_weight * Σ_axis (affinity[axis] * effective_stance[axis])
/// ```
///
/// Floored at `stance_affinity_floor` rather than at zero: a wrong-stance dressing should become
/// *unlikely*, not impossible. The wardrobe pool is small, and hard exclusion risks a beat with
/// nothing left to dress it in. An entry with no `stance_affinity` gets exactly `1.0`, so
/// uncoloured beats behave precisely as they did before the fork tier existed.
pub fn affinity_term(
    entry: &WardrobeEntry,
    effective: &BTreeMap<String, f32>,
    cfg: &SelectionConfig,
) -> f32 {
    let Some(affinity) = entry.stance_affinity.as_ref() else {
        return 1.0;
    };
    let alignment: f32 = affinity
        .iter()
        .map(|(axis, weight)| weight * effective.get(axis).copied().unwrap_or(0.0))
        .sum();
    (1.0 + cfg.stance_affinity_weight * alignment).max(cfg.stance_affinity_floor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn config() -> std::sync::Arc<BeatConfig> {
        BeatConfig::builtin()
    }

    #[test]
    fn normalize_maps_the_configured_range_onto_the_axis() {
        assert_eq!(normalize(0.0, [0.0, 100.0]), -1.0);
        assert_eq!(normalize(50.0, [0.0, 100.0]), 0.0);
        assert_eq!(normalize(100.0, [0.0, 100.0]), 1.0);
        // Out-of-range samples clamp rather than running off the axis.
        assert_eq!(normalize(-40.0, [0.0, 100.0]), -1.0);
        assert_eq!(normalize(400.0, [0.0, 100.0]), 1.0);
    }

    #[test]
    fn effective_stance_is_the_signal_plus_the_declared_offset() {
        let config = config();
        // Sedentarization 70/100 accretes to +0.4 on `roam_settle`.
        let sample = SignalSample::from_pairs([("sedentarization.score".to_string(), 70.0)]);
        let none = BTreeMap::new();
        let accreted = effective_stance(&config, &sample, &none)["roam_settle"];
        assert!((accreted - 0.4).abs() < 1e-6, "{accreted}");

        let declared = BTreeMap::from([("roam_settle".to_string(), Scalar::from_f32(0.25))]);
        let ratified = effective_stance(&config, &sample, &declared)["roam_settle"];
        assert!((ratified - 0.65).abs() < 1e-6, "{ratified}");
    }

    /// The design claim of the split: a player can **resist** their own behaviour, and the tension
    /// stays representable rather than being collapsed into one stored number.
    #[test]
    fn a_declared_offset_can_oppose_the_accreted_signal() {
        let config = config();
        let sample = SignalSample::from_pairs([("sedentarization.score".to_string(), 70.0)]);
        let resisting = BTreeMap::from([("roam_settle".to_string(), Scalar::from_f32(-0.4))]);
        let value = effective_stance(&config, &sample, &resisting)["roam_settle"];
        assert!(
            value.abs() < 1e-6,
            "resisting should cancel the drift: {value}"
        );

        // And it can carry the player past neutral, against the signal's direction.
        let hard = BTreeMap::from([("roam_settle".to_string(), Scalar::from_f32(-1.0))]);
        let value = effective_stance(&config, &sample, &hard)["roam_settle"];
        assert!(value < 0.0, "{value}");
    }

    #[test]
    fn effective_stance_clamps_to_the_axis_bounds() {
        let config = config();
        let sample = SignalSample::from_pairs([("sedentarization.score".to_string(), 100.0)]);
        let piled_on = BTreeMap::from([("roam_settle".to_string(), Scalar::from_f32(1.0))]);
        assert_eq!(
            effective_stance(&config, &sample, &piled_on)["roam_settle"],
            1.0
        );
    }

    #[test]
    fn the_affinity_term_is_neutral_without_an_affinity_and_floors_when_opposed() {
        let config = config();
        let cfg = &config.selection;
        let mut entry = WardrobeEntry {
            id: "e".to_string(),
            fit: Default::default(),
            voice: BTreeMap::new(),
            stance_affinity: None,
        };
        let settle = BTreeMap::from([("roam_settle".to_string(), 1.0)]);
        assert_eq!(affinity_term(&entry, &settle, cfg), 1.0);

        entry.stance_affinity = Some(BTreeMap::from([("roam_settle".to_string(), 1.0)]));
        let aligned = affinity_term(&entry, &settle, cfg);
        assert!(aligned > 1.0, "{aligned}");

        let roam = BTreeMap::from([("roam_settle".to_string(), -1.0)]);
        let opposed = affinity_term(&entry, &roam, cfg);
        assert!(opposed < 1.0, "{opposed}");
        assert!(
            opposed >= cfg.stance_affinity_floor,
            "a wrong-stance entry must stay reachable, got {opposed}"
        );
    }
}
