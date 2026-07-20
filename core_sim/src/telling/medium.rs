//! **The maturing voice** — the narrator's *medium* (concept §7: oral saga → painted chronicle →
//! written record → institutional archive).
//!
//! *"The narrator maturing is itself a narrative arc that makes progression felt."*
//!
//! # Medium is presentational, not a content multiplier
//!
//! Medium changes how the telling **looks** to the client, and it fires a beat when it advances.
//! That is the whole of it. It deliberately does **not** select different wardrobe copy: four
//! mediums × two registers per wardrobe entry is an 8× authoring cost for the layer's thinnest
//! payoff, and `docs/Emergent Narrative.md` §13 names authoring cost as a real risk. If a later
//! slice is tempted to "complete" the feature by writing per-medium strings — don't. The two
//! things that make progression felt are the client's presentation and the medium-advance beat.
//!
//! # Model
//!
//! Mediums are just **named thresholds over signals**, ordered least → most advanced in
//! `beat_config.json`, evaluated once per turn through the *same* predicate evaluator the beat
//! triggers use. The **highest satisfied** rung wins; the first entry is the default and needs no
//! `when`.
//!
//! **The attained rung never regresses.** If a signal falls back below its threshold the medium
//! does not step down — a people that learned to write does not forget — so the ledger persists the
//! attained index and the per-turn evaluation takes the max.
//!
//! The attained index is readable as the `voice.medium_index` signal, which is how the authored
//! `voice.medium_painted` / `voice.medium_written` beats gate themselves with `crosses`.

use super::{
    config::{BeatConfig, VoiceMedium, DEFAULT_VOICE_MEDIUM},
    predicate::EvalContext,
};

/// The rung a medium ladder currently reads at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttainedMedium {
    pub index: u32,
    pub id: String,
}

impl Default for AttainedMedium {
    fn default() -> Self {
        Self {
            index: 0,
            id: DEFAULT_VOICE_MEDIUM.to_string(),
        }
    }
}

/// The **highest satisfied** rung this turn, ignoring what was attained before. The first entry is
/// the default and is always satisfied, so this is never `None` on a validated config.
fn highest_satisfied<'a>(
    config: &'a BeatConfig,
    ctx: &EvalContext<'_>,
) -> Option<(u32, &'a VoiceMedium)> {
    config
        .voice
        .mediums
        .iter()
        .enumerate()
        .rfind(|(_, medium)| medium.when.as_ref().is_none_or(|when| when.evaluate(ctx)))
        .map(|(index, medium)| (index as u32, medium))
}

/// Advance the ladder for this turn: the highest satisfied rung, **floored at what was already
/// attained**. Never regresses.
pub fn advance(
    config: &BeatConfig,
    ctx: &EvalContext<'_>,
    attained: &AttainedMedium,
) -> AttainedMedium {
    let Some((index, medium)) = highest_satisfied(config, ctx) else {
        return attained.clone();
    };
    if index <= attained.index {
        // Hold: either nothing new is satisfied, or a signal fell back below a threshold the
        // civilization has already crossed. Re-resolve the *id* from the config so a renamed rung
        // still reports correctly, falling back to what the ledger carries.
        let id = config
            .voice
            .mediums
            .get(attained.index as usize)
            .map(|medium| medium.id.clone())
            .unwrap_or_else(|| attained.id.clone());
        return AttainedMedium {
            index: attained.index,
            id,
        };
    }
    AttainedMedium {
        index,
        id: medium.id.clone(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet, VecDeque};

    use super::*;
    use crate::telling::{
        config::TrendConfig, memory::Thread, predicate::Predicate, signals::SignalSample,
    };

    struct Harness {
        previous: BTreeMap<String, crate::scalar::Scalar>,
        history: BTreeMap<String, VecDeque<crate::scalar::Scalar>>,
        fired: BTreeMap<String, Vec<u64>>,
        flags: BTreeSet<String>,
        answers: BTreeMap<String, crate::telling::Answer>,
        threads: BTreeMap<String, Vec<Thread>>,
        trend: TrendConfig,
    }

    impl Harness {
        fn new() -> Self {
            Self {
                previous: BTreeMap::new(),
                history: BTreeMap::new(),
                fired: BTreeMap::new(),
                flags: BTreeSet::new(),
                answers: BTreeMap::new(),
                threads: BTreeMap::new(),
                trend: TrendConfig::default(),
            }
        }

        fn ctx<'a>(&'a self, sample: &'a SignalSample) -> EvalContext<'a> {
            EvalContext {
                sample,
                previous: &self.previous,
                history: &self.history,
                fired: &self.fired,
                flags: &self.flags,
                answers: &self.answers,
                threads: &self.threads,
                tick: 0,
                trend: &self.trend,
            }
        }
    }

    fn ladder() -> BeatConfig {
        let mut config = BeatConfig::default();
        config.voice.mediums = vec![
            VoiceMedium {
                id: "oral".to_string(),
                when: None,
            },
            VoiceMedium {
                id: "painted".to_string(),
                when: Some(
                    serde_json::from_str::<Predicate>(
                        r#"{ "signal": "sedentarization.score", "gte": 40 }"#,
                    )
                    .unwrap(),
                ),
            },
            VoiceMedium {
                id: "written".to_string(),
                when: Some(
                    serde_json::from_str::<Predicate>(
                        r#"{ "signal": "sedentarization.score", "gte": 70 }"#,
                    )
                    .unwrap(),
                ),
            },
        ];
        config
    }

    fn score(value: f64) -> SignalSample {
        SignalSample::from_pairs([("sedentarization.score".to_string(), value)])
    }

    #[test]
    fn the_highest_satisfied_medium_wins() {
        let config = ladder();
        let harness = Harness::new();
        let attained = AttainedMedium::default();

        for (value, expected_index, expected_id) in
            [(0.0, 0, "oral"), (50.0, 1, "painted"), (90.0, 2, "written")]
        {
            let sample = score(value);
            let advanced = advance(&config, &harness.ctx(&sample), &attained);
            assert_eq!(advanced.index, expected_index, "at {value}");
            assert_eq!(advanced.id, expected_id, "at {value}");
        }
    }

    /// A people that learned to write does not forget: a signal falling back below a threshold
    /// must not step the medium down.
    #[test]
    fn the_medium_never_regresses() {
        let config = ladder();
        let harness = Harness::new();

        let sample = score(90.0);
        let written = advance(&config, &harness.ctx(&sample), &AttainedMedium::default());
        assert_eq!(written.id, "written");

        let collapsed = score(0.0);
        let held = advance(&config, &harness.ctx(&collapsed), &written);
        assert_eq!(held, written, "the medium must hold when the signal falls");
    }

    /// The ladder skips rungs rather than walking them: crossing straight past `painted` lands on
    /// `written`, and the `crosses` gate on `voice.medium_index` still sees the rise.
    #[test]
    fn a_jump_lands_on_the_highest_rung_reached() {
        let config = ladder();
        let harness = Harness::new();
        let sample = score(95.0);
        let advanced = advance(&config, &harness.ctx(&sample), &AttainedMedium::default());
        assert_eq!(advanced.index, 2);
    }
}
