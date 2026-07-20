//! The `when` predicate grammar and its evaluator (`docs/plan_the_telling.md` §2a).
//!
//! Deliberately small: combinators `all` / `any` / `not`, and leaves that compare a **named
//! signal** (see `signals.rs`) against a threshold, an edge, a trend, a consequence flag, or the
//! fired-set. The correctness heart is [`Predicate::Crosses`]: it is true **only on the turn the
//! value crosses**, so a beat fires once per crossing rather than every turn the condition holds.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{de, Deserialize, Deserializer};

use super::{
    config::TrendConfig,
    memory::{self, Thread},
    signals::{SignalId, SignalSample},
    Answer,
};
use crate::scalar::Scalar;

/// `eq` on floats compares within this, never with `==` — sampled signals are computed values,
/// not literals, so exact bit equality is not a meaningful test.
const COMPARE_EPSILON: f64 = 1e-9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Gt,
    Gte,
    Lt,
    Lte,
    Eq,
}

impl CompareOp {
    pub fn as_str(self) -> &'static str {
        match self {
            CompareOp::Gt => "gt",
            CompareOp::Gte => "gte",
            CompareOp::Lt => "lt",
            CompareOp::Lte => "lte",
            CompareOp::Eq => "eq",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        match key {
            "gt" => Some(CompareOp::Gt),
            "gte" => Some(CompareOp::Gte),
            "lt" => Some(CompareOp::Lt),
            "lte" => Some(CompareOp::Lte),
            "eq" => Some(CompareOp::Eq),
            _ => None,
        }
    }

    fn apply(self, lhs: f64, rhs: f64) -> bool {
        match self {
            CompareOp::Gt => lhs > rhs,
            CompareOp::Gte => lhs >= rhs,
            CompareOp::Lt => lhs < rhs,
            CompareOp::Lte => lhs <= rhs,
            CompareOp::Eq => (lhs - rhs).abs() < COMPARE_EPSILON,
        }
    }
}

/// Direction of an edge crossing or a trend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeDir {
    Rising,
    Falling,
}

impl EdgeDir {
    pub fn as_str(self) -> &'static str {
        match self {
            EdgeDir::Rising => "rising",
            EdgeDir::Falling => "falling",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        match key {
            "rising" => Some(EdgeDir::Rising),
            "falling" => Some(EdgeDir::Falling),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Predicate {
    All(Vec<Predicate>),
    Any(Vec<Predicate>),
    Not(Box<Predicate>),
    /// Plain comparison on this turn's sample.
    Compare {
        signal: SignalId,
        op: CompareOp,
        value: f64,
    },
    /// True **only** on the turn the value crosses `threshold` in `dir`, computed from the
    /// previous turn's stored sample. Re-arms only once the value falls back across it.
    Crosses {
        signal: SignalId,
        dir: EdgeDir,
        threshold: f64,
    },
    /// A sustained move: the sample `over_turns` ago vs now, beyond `trend.min_delta`.
    Trend {
        signal: SignalId,
        dir: EdgeDir,
        over_turns: u32,
    },
    /// A consequence flag written by an earlier beat.
    Flag(String),
    /// Callback to a prior beat (the memory ledger).
    Fired {
        beat: String,
        within_turns: u32,
    },
    /// **The memory-thread gate**: at least `min_count` threads of `kind` exist that were first
    /// seen at least `older_than_turns` turns ago.
    Thread {
        kind: String,
        min_count: u32,
        older_than_turns: u32,
    },
    /// **The consequence gate**: the player answered fork `beat` with `choice`, at least
    /// `min_turns_since` turns ago. This is what makes a fork's promise — *"the stories that will
    /// find you now"* — literally true: a player who answered one way gets a beat the other player
    /// never sees.
    ///
    /// `min_turns_since` is the **elapsed-time** half, and it is usually load-bearing: a callback
    /// almost always means "some time after you said that". `identity.trail_endures` says *"we have
    /// kept our word to it"*, which is absurd the turn after the word is given. Do **not** reach for
    /// a `turn.index` trend to express this — `turn.index` rises unconditionally, so such a clause
    /// only ever means "we are past turn n", not "n turns since the answer".
    Answered {
        beat: String,
        choice: String,
        min_turns_since: u32,
    },
}

/// Everything a predicate reads. Assembled once per turn so evaluation is a pure function of a
/// consistent snapshot.
pub struct EvalContext<'a> {
    pub sample: &'a SignalSample,
    /// **Last turn's** samples (the ledger's edge state, read *before* this turn's update).
    /// An absent signal means "never seen before", which is why `Crosses` cannot fire on the
    /// first turn a signal appears.
    pub previous: &'a BTreeMap<String, Scalar>,
    /// Rolling per-signal history, **including this turn's sample as the last element**.
    pub history: &'a BTreeMap<String, VecDeque<Scalar>>,
    pub fired: &'a BTreeMap<String, Vec<u64>>,
    pub flags: &'a BTreeSet<String>,
    /// Beat id → the choice the player took, backing [`Predicate::Answered`].
    pub answers: &'a BTreeMap<String, Answer>,
    /// The memory threads, by kind, backing [`Predicate::Thread`].
    pub threads: &'a BTreeMap<String, Vec<Thread>>,
    pub tick: u64,
    pub trend: &'a TrendConfig,
}

impl Predicate {
    pub fn evaluate(&self, ctx: &EvalContext<'_>) -> bool {
        match self {
            Predicate::All(children) => children.iter().all(|c| c.evaluate(ctx)),
            Predicate::Any(children) => children.iter().any(|c| c.evaluate(ctx)),
            Predicate::Not(child) => !child.evaluate(ctx),
            Predicate::Compare { signal, op, value } => op.apply(ctx.sample.get(signal), *value),
            Predicate::Crosses {
                signal,
                dir,
                threshold,
            } => {
                // No previous sample => the signal is being seen for the first time; a first-ever
                // sample is never a crossing (a beat that must fire on turn 0 uses `eq`).
                let Some(prev) = ctx.previous.get(signal) else {
                    return false;
                };
                let prev = prev.to_f32() as f64;
                let now = ctx.sample.get(signal);
                match dir {
                    EdgeDir::Rising => prev < *threshold && now >= *threshold,
                    EdgeDir::Falling => prev >= *threshold && now < *threshold,
                }
            }
            Predicate::Trend {
                signal,
                dir,
                over_turns,
            } => {
                let Some(history) = ctx.history.get(signal) else {
                    return false;
                };
                let span = *over_turns as usize;
                // `history` ends with this turn's sample, so "n turns ago" is n slots back.
                if span == 0 || history.len() <= span {
                    return false;
                }
                let now = history[history.len() - 1].to_f32() as f64;
                let then = history[history.len() - 1 - span].to_f32() as f64;
                let delta = now - then;
                let min_delta = ctx.trend.min_delta as f64;
                match dir {
                    EdgeDir::Rising => delta >= min_delta,
                    EdgeDir::Falling => -delta >= min_delta,
                }
            }
            Predicate::Flag(flag) => ctx.flags.contains(flag),
            Predicate::Fired { beat, within_turns } => ctx
                .fired
                .get(beat)
                .map(|ticks| {
                    ticks
                        .iter()
                        .any(|t| ctx.tick.saturating_sub(*t) <= *within_turns as u64)
                })
                .unwrap_or(false),
            Predicate::Thread {
                kind,
                min_count,
                older_than_turns,
            } => {
                memory::count_matching(ctx.threads, kind, *older_than_turns, ctx.tick)
                    >= *min_count as usize
            }
            Predicate::Answered {
                beat,
                choice,
                min_turns_since,
            } => ctx.answers.get(beat).is_some_and(|answer| {
                answer.choice == *choice
                    && ctx.tick.saturating_sub(answer.tick) >= *min_turns_since as u64
            }),
        }
    }

    /// Every signal id this predicate references — the load-time validation hook.
    pub fn collect_signals(&self, out: &mut Vec<SignalId>) {
        match self {
            Predicate::All(children) | Predicate::Any(children) => {
                for child in children {
                    child.collect_signals(out);
                }
            }
            Predicate::Not(child) => child.collect_signals(out),
            Predicate::Compare { signal, .. }
            | Predicate::Crosses { signal, .. }
            | Predicate::Trend { signal, .. } => out.push(signal.clone()),
            Predicate::Flag(_)
            | Predicate::Fired { .. }
            | Predicate::Thread { .. }
            | Predicate::Answered { .. } => {}
        }
    }

    /// Every `{ "answered": B, "choice": C, "min_turns_since": n }` gate, as `(B, C, n)` — the
    /// load-time validation hook. A typo in `B`/`C` silently produces a beat that can never fire,
    /// which is the worst failure mode a content system has, so the catalog checks each target hard.
    pub fn collect_answered_gates(&self, out: &mut Vec<(String, String, u32)>) {
        self.walk(&mut |predicate| {
            if let Predicate::Answered {
                beat,
                choice,
                min_turns_since,
            } = predicate
            {
                out.push((beat.clone(), choice.clone(), *min_turns_since));
            }
        });
    }

    /// Every `trend` window this predicate opens — the load-time hook that catches a window wider
    /// than the ledger's retained history, which `evaluate` can only ever read as `false`.
    pub fn collect_trend_windows(&self, out: &mut Vec<u32>) {
        self.walk(&mut |predicate| {
            if let Predicate::Trend { over_turns, .. } = predicate {
                out.push(*over_turns);
            }
        });
    }

    /// Every thread `kind` this predicate gates on — the load-time hook that catches a kind no
    /// `remembers` entry ever writes (and which could therefore never be satisfied).
    pub fn collect_thread_kinds(&self, out: &mut Vec<String>) {
        self.walk(&mut |predicate| {
            if let Predicate::Thread { kind, .. } = predicate {
                out.push(kind.clone());
            }
        });
    }

    /// Visit every node of the tree, so the collectors above share one traversal.
    fn walk(&self, visit: &mut impl FnMut(&Predicate)) {
        visit(self);
        match self {
            Predicate::All(children) | Predicate::Any(children) => {
                for child in children {
                    child.walk(visit);
                }
            }
            Predicate::Not(child) => child.walk(visit),
            _ => {}
        }
    }
}

// --- parsing -----------------------------------------------------------------------------
//
// Untagged-style dispatch on which keys are present. Content authors will hit malformed leaves,
// so every failure names the offending object rather than the serde default of "unknown variant".

impl<'de> Deserialize<'de> for Predicate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        parse_predicate(&value).map_err(de::Error::custom)
    }
}

fn parse_predicate(value: &serde_json::Value) -> Result<Predicate, String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("predicate must be an object, got {value}"))?;

    if let Some(children) = object.get("all") {
        return Ok(Predicate::All(parse_children(children, "all")?));
    }
    if let Some(children) = object.get("any") {
        return Ok(Predicate::Any(parse_children(children, "any")?));
    }
    if let Some(child) = object.get("not") {
        return Ok(Predicate::Not(Box::new(parse_predicate(child)?)));
    }
    if let Some(flag) = object.get("flag") {
        let flag = flag
            .as_str()
            .ok_or_else(|| format!("`flag` must be a string, got {flag}"))?;
        return Ok(Predicate::Flag(flag.to_string()));
    }
    if let Some(beat) = object.get("fired") {
        let beat = beat
            .as_str()
            .ok_or_else(|| format!("`fired` must be a beat id string, got {beat}"))?;
        let within_turns = parse_u32(object.get("within_turns"), "within_turns", value)?;
        return Ok(Predicate::Fired {
            beat: beat.to_string(),
            within_turns,
        });
    }

    if let Some(kind) = object.get("thread") {
        let kind = kind
            .as_str()
            .ok_or_else(|| format!("`thread` must be a kind string, got {kind}"))?;
        // Both bounds default: `{ "thread": K }` reads as "we remember at least one, of any age".
        let min_count = parse_optional_u32(object.get("min_count"), "min_count", value)?
            .unwrap_or(DEFAULT_THREAD_MIN_COUNT);
        let older_than_turns =
            parse_optional_u32(object.get("older_than_turns"), "older_than_turns", value)?
                .unwrap_or(0);
        return Ok(Predicate::Thread {
            kind: kind.to_string(),
            min_count,
            older_than_turns,
        });
    }
    if let Some(beat) = object.get("answered") {
        let beat = beat
            .as_str()
            .ok_or_else(|| format!("`answered` must be a beat id string, got {beat}"))?;
        let choice = object
            .get("choice")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                format!("predicate {value} requires a `choice` naming the answer it gates on")
            })?;
        // Defaults to 0 — "the moment you answered" — so a gate that genuinely wants no delay
        // says nothing, and one that wants elapsed time says so explicitly.
        let min_turns_since =
            parse_optional_u32(object.get("min_turns_since"), "min_turns_since", value)?
                .unwrap_or(0);
        return Ok(Predicate::Answered {
            beat: beat.to_string(),
            choice: choice.to_string(),
            min_turns_since,
        });
    }

    let signal = object
        .get("signal")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            format!(
                "unrecognised predicate leaf {value}: expected one of `all`, `any`, `not`, \
                 `flag`, `fired`, or a `signal` with `crosses`/`trend`/`gt`/`gte`/`lt`/`lte`/`eq`"
            )
        })?
        .to_string();

    if let Some(dir) = object.get("crosses") {
        let dir = parse_dir(dir, "crosses")?;
        let threshold = parse_f64(object.get("threshold"), "threshold", value)?;
        return Ok(Predicate::Crosses {
            signal,
            dir,
            threshold,
        });
    }
    if let Some(dir) = object.get("trend") {
        let dir = parse_dir(dir, "trend")?;
        let over_turns = parse_u32(object.get("over_turns"), "over_turns", value)?;
        return Ok(Predicate::Trend {
            signal,
            dir,
            over_turns,
        });
    }

    for (key, raw) in object {
        if let Some(op) = CompareOp::from_key(key) {
            let value = raw
                .as_f64()
                .ok_or_else(|| format!("`{key}` must be a number, got {raw}"))?;
            return Ok(Predicate::Compare { signal, op, value });
        }
    }

    Err(format!(
        "signal predicate {value} names no operator: expected `crosses`, `trend`, or one of \
         `gt`/`gte`/`lt`/`lte`/`eq`"
    ))
}

/// "We remember at least one" — the reading a bare `{ "thread": K }` should have.
const DEFAULT_THREAD_MIN_COUNT: u32 = 1;

fn parse_children(value: &serde_json::Value, key: &str) -> Result<Vec<Predicate>, String> {
    let array = value
        .as_array()
        .ok_or_else(|| format!("`{key}` must be an array of predicates, got {value}"))?;
    array.iter().map(parse_predicate).collect()
}

fn parse_dir(value: &serde_json::Value, key: &str) -> Result<EdgeDir, String> {
    value
        .as_str()
        .and_then(EdgeDir::from_key)
        .ok_or_else(|| format!("`{key}` must be \"rising\" or \"falling\", got {value}"))
}

fn parse_f64(
    value: Option<&serde_json::Value>,
    key: &str,
    parent: &serde_json::Value,
) -> Result<f64, String> {
    value
        .and_then(serde_json::Value::as_f64)
        .ok_or_else(|| format!("predicate {parent} requires a numeric `{key}`"))
}

fn parse_u32(
    value: Option<&serde_json::Value>,
    key: &str,
    parent: &serde_json::Value,
) -> Result<u32, String> {
    value
        .and_then(serde_json::Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| format!("predicate {parent} requires a non-negative integer `{key}`"))
}

/// Like [`parse_u32`], but absent is `Ok(None)` rather than an error. A *present but malformed*
/// key still fails — a typo'd bound must never silently read as its default.
fn parse_optional_u32(
    value: Option<&serde_json::Value>,
    key: &str,
    parent: &serde_json::Value,
) -> Result<Option<u32>, String> {
    match value {
        None => Ok(None),
        Some(_) => parse_u32(value, key, parent).map(Some),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telling::nouns::Noun;

    fn parse(json: &str) -> Predicate {
        serde_json::from_str(json).expect("predicate parses")
    }

    /// A tiny harness that walks a signal through a sequence of turns, mirroring the ledger
    /// bookkeeping `telling_tick` does (evaluate against last turn's sample, then store).
    struct Walker {
        previous: BTreeMap<String, Scalar>,
        history: BTreeMap<String, VecDeque<Scalar>>,
        fired: BTreeMap<String, Vec<u64>>,
        flags: BTreeSet<String>,
        answers: BTreeMap<String, Answer>,
        threads: BTreeMap<String, Vec<Thread>>,
        trend: TrendConfig,
        tick: u64,
    }

    impl Walker {
        fn new() -> Self {
            Self {
                previous: BTreeMap::new(),
                history: BTreeMap::new(),
                fired: BTreeMap::new(),
                flags: BTreeSet::new(),
                answers: BTreeMap::new(),
                threads: BTreeMap::new(),
                trend: TrendConfig::default(),
                tick: 0,
            }
        }

        fn step(&mut self, predicate: &Predicate, signal: &str, value: f64) -> bool {
            let sample = SignalSample::from_pairs([(signal.to_string(), value)]);
            self.history
                .entry(signal.to_string())
                .or_default()
                .push_back(Scalar::from_f32(value as f32));
            let result = predicate.evaluate(&EvalContext {
                sample: &sample,
                previous: &self.previous,
                history: &self.history,
                fired: &self.fired,
                flags: &self.flags,
                answers: &self.answers,
                threads: &self.threads,
                tick: self.tick,
                trend: &self.trend,
            });
            self.previous
                .insert(signal.to_string(), Scalar::from_f32(value as f32));
            self.tick += 1;
            result
        }
    }

    #[test]
    fn compare_reads_this_turns_sample() {
        let predicate = parse(r#"{ "signal": "turn.index", "eq": 0 }"#);
        let mut w = Walker::new();
        assert!(w.step(&predicate, "turn.index", 0.0));
        assert!(!w.step(&predicate, "turn.index", 1.0));
    }

    #[test]
    fn compare_supports_every_operator() {
        for (json, value, expected) in [
            (r#"{ "signal": "band.count", "gt": 10 }"#, 11.0, true),
            (r#"{ "signal": "band.count", "gt": 10 }"#, 10.0, false),
            (r#"{ "signal": "band.count", "gte": 10 }"#, 10.0, true),
            (r#"{ "signal": "band.count", "lt": 10 }"#, 9.0, true),
            (r#"{ "signal": "band.count", "lte": 10 }"#, 10.0, true),
            (r#"{ "signal": "band.count", "eq": 10 }"#, 10.0, true),
        ] {
            let mut w = Walker::new();
            assert_eq!(
                w.step(&parse(json), "band.count", value),
                expected,
                "{json} at {value}"
            );
        }
    }

    /// The correctness heart: exactly one fire per crossing, re-arming only after a fall-back.
    #[test]
    fn crosses_fires_once_per_crossing_and_rearms_after_falling_back() {
        let predicate =
            parse(r#"{ "signal": "sedentarization.score", "crosses": "rising", "threshold": 40 }"#);
        let mut w = Walker::new();
        let signal = "sedentarization.score";

        // First-ever sample is never a crossing, even above the threshold.
        assert!(!w.step(&predicate, signal, 50.0), "no prev => no crossing");
        // Falls back below, then climbs across: exactly one fire.
        assert!(!w.step(&predicate, signal, 10.0));
        assert!(!w.step(&predicate, signal, 39.0));
        assert!(w.step(&predicate, signal, 41.0), "the crossing turn fires");
        // Holding above must NOT re-fire.
        for held in [42.0, 60.0, 99.0] {
            assert!(!w.step(&predicate, signal, held), "held at {held}");
        }
        // Re-arms only after falling back below.
        assert!(!w.step(&predicate, signal, 20.0));
        assert!(w.step(&predicate, signal, 45.0), "re-armed crossing fires");
    }

    #[test]
    fn crosses_falling_is_the_mirror() {
        let predicate =
            parse(r#"{ "signal": "provisions.total", "crosses": "falling", "threshold": 100 }"#);
        let mut w = Walker::new();
        let signal = "provisions.total";
        assert!(!w.step(&predicate, signal, 150.0));
        assert!(!w.step(&predicate, signal, 120.0));
        assert!(w.step(&predicate, signal, 80.0));
        assert!(
            !w.step(&predicate, signal, 50.0),
            "held below must not re-fire"
        );
        assert!(!w.step(&predicate, signal, 130.0));
        assert!(w.step(&predicate, signal, 90.0));
    }

    #[test]
    fn trend_needs_a_sustained_move_over_the_window() {
        let predicate =
            parse(r#"{ "signal": "provisions.total", "trend": "falling", "over_turns": 3 }"#);
        let mut w = Walker::new();
        let signal = "provisions.total";
        // Not enough history yet.
        assert!(!w.step(&predicate, signal, 100.0));
        assert!(!w.step(&predicate, signal, 90.0));
        assert!(!w.step(&predicate, signal, 80.0));
        // Now 3 turns back exists: 100 -> 70 is a fall.
        assert!(w.step(&predicate, signal, 70.0));
        // A flat stretch is not a trend.
        let mut flat = Walker::new();
        for _ in 0..6 {
            assert!(!flat.step(&predicate, signal, 50.0));
        }
    }

    #[test]
    fn trend_rising_reads_the_opposite_direction() {
        let predicate = parse(r#"{ "signal": "band.count", "trend": "rising", "over_turns": 2 }"#);
        let mut w = Walker::new();
        w.step(&predicate, "band.count", 10.0);
        w.step(&predicate, "band.count", 12.0);
        assert!(w.step(&predicate, "band.count", 20.0));
    }

    #[test]
    fn combinators_compose() {
        let predicate = parse(
            r#"{ "all": [ { "signal": "band.count", "gte": 10 },
                          { "not": { "signal": "band.count", "gt": 100 } } ] }"#,
        );
        let mut w = Walker::new();
        assert!(w.step(&predicate, "band.count", 50.0));
        assert!(!w.step(&predicate, "band.count", 500.0));

        let any = parse(
            r#"{ "any": [ { "signal": "band.count", "gt": 1000 },
                          { "signal": "band.count", "lt": 5 } ] }"#,
        );
        let mut w = Walker::new();
        assert!(w.step(&any, "band.count", 2.0));
        assert!(!w.step(&any, "band.count", 50.0));
    }

    #[test]
    fn flag_and_fired_read_the_ledger() {
        let flag = parse(r#"{ "flag": "went_hungry" }"#);
        let fired = parse(r#"{ "fired": "opening.cold_open", "within_turns": 5 }"#);
        let mut w = Walker::new();
        assert!(!w.step(&flag, "band.count", 0.0));
        assert!(!w.step(&fired, "band.count", 0.0));

        w.flags.insert("went_hungry".to_string());
        w.fired
            .insert("opening.cold_open".to_string(), vec![w.tick]);
        assert!(w.step(&flag, "band.count", 0.0));
        assert!(w.step(&fired, "band.count", 0.0));

        // Outside the window it no longer matches.
        w.tick += 50;
        assert!(!w.step(&fired, "band.count", 0.0));
    }

    #[test]
    fn thread_reads_the_memory_ledger_with_a_count_and_an_age_gate() {
        let predicate = parse(r#"{ "thread": "place", "min_count": 2, "older_than_turns": 25 }"#);
        let mut w = Walker::new();
        assert!(!w.step(&predicate, "band.count", 0.0), "no threads yet");

        memory::remember(
            &mut w.threads,
            "place",
            &Noun::named("Great Peak", "peaks", "peak"),
            0,
            8,
        );
        memory::remember(
            &mut w.threads,
            "place",
            &Noun::named("Verdant Basin", "basins", "basin"),
            0,
            8,
        );
        // Both exist but neither is old enough yet.
        w.tick = 10;
        assert!(!w.step(&predicate, "band.count", 0.0));
        w.tick = 40;
        assert!(w.step(&predicate, "band.count", 0.0));

        // The bounds default: a bare `{ "thread": K }` is "at least one, of any age".
        let bare = parse(r#"{ "thread": "place" }"#);
        assert!(matches!(
            bare,
            Predicate::Thread {
                min_count: 1,
                older_than_turns: 0,
                ..
            }
        ));
    }

    #[test]
    fn answered_reads_the_recorded_choice() {
        let predicate =
            parse(r#"{ "answered": "sedentarization.soft_drift", "choice": "yes_trail" }"#);
        let mirror = parse(r#"{ "answered": "sedentarization.soft_drift", "choice": "no_root" }"#);
        let mut w = Walker::new();
        assert!(!w.step(&predicate, "band.count", 0.0), "unanswered");

        w.answers.insert(
            "sedentarization.soft_drift".to_string(),
            Answer {
                choice: "yes_trail".to_string(),
                tick: 0,
            },
        );
        assert!(w.step(&predicate, "band.count", 0.0));
        assert!(
            !w.step(&mirror, "band.count", 0.0),
            "one answer must not satisfy the other branch"
        );
    }

    /// **`min_turns_since` is the honest expression of "some time after you said that."** The
    /// tempting alternative — a `turn.index` trend — rises unconditionally, so it only ever means
    /// "we are past turn n"; combined with an `answered` gate it fires the turn *after* the answer.
    #[test]
    fn answered_gates_on_elapsed_time_since_the_answer_not_since_turn_zero() {
        let predicate = parse(
            r#"{ "answered": "sedentarization.soft_drift", "choice": "yes_trail",
                 "min_turns_since": 20 }"#,
        );
        let mut w = Walker::new();
        // Answered late in the campaign: what matters is the gap, not the absolute turn.
        const ANSWERED_AT: u64 = 100;
        w.answers.insert(
            "sedentarization.soft_drift".to_string(),
            Answer {
                choice: "yes_trail".to_string(),
                tick: ANSWERED_AT,
            },
        );

        for elapsed in [0, 1, 19] {
            w.tick = ANSWERED_AT + elapsed;
            assert!(
                !w.step(&predicate, "band.count", 0.0),
                "{elapsed} turns after the answer is too soon — the copy says \
                 \"we have kept our word\""
            );
        }
        w.tick = ANSWERED_AT + 20;
        assert!(
            w.step(&predicate, "band.count", 0.0),
            "the gate opens at 20"
        );
        w.tick = ANSWERED_AT + 500;
        assert!(w.step(&predicate, "band.count", 0.0), "and stays open");

        // Absent, it defaults to 0 — "the moment you answered".
        let immediate =
            parse(r#"{ "answered": "sedentarization.soft_drift", "choice": "yes_trail" }"#);
        assert!(matches!(
            immediate,
            Predicate::Answered {
                min_turns_since: 0,
                ..
            }
        ));
        w.tick = ANSWERED_AT;
        assert!(w.step(&immediate, "band.count", 0.0));
    }

    #[test]
    fn collectors_walk_the_whole_tree() {
        let predicate = parse(
            r#"{ "all": [ { "answered": "a.fork", "choice": "yes" },
                          { "not": { "thread": "place", "min_count": 1 } },
                          { "any": [ { "answered": "b.fork", "choice": "no" } ] } ] }"#,
        );
        let mut answered = Vec::new();
        predicate.collect_answered_gates(&mut answered);
        assert_eq!(
            answered,
            vec![
                ("a.fork".to_string(), "yes".to_string(), 0),
                ("b.fork".to_string(), "no".to_string(), 0)
            ]
        );
        let mut kinds = Vec::new();
        predicate.collect_thread_kinds(&mut kinds);
        assert_eq!(kinds, vec!["place".to_string()]);
    }

    #[test]
    fn malformed_leaves_report_a_clear_error() {
        let err = serde_json::from_str::<Predicate>(r#"{ "signal": "band.count" }"#)
            .expect_err("no operator");
        assert!(err.to_string().contains("names no operator"), "{err}");

        let err = serde_json::from_str::<Predicate>(r#"{ "wat": 1 }"#).expect_err("unknown leaf");
        assert!(
            err.to_string().contains("unrecognised predicate leaf"),
            "{err}"
        );

        let err = serde_json::from_str::<Predicate>(
            r#"{ "signal": "band.count", "crosses": "sideways", "threshold": 1 }"#,
        )
        .expect_err("bad direction");
        assert!(err.to_string().contains("rising"), "{err}");
    }

    #[test]
    fn collect_signals_walks_the_whole_tree() {
        let predicate = parse(
            r#"{ "all": [ { "signal": "band.count", "gte": 1 },
                          { "not": { "signal": "provisions.total", "trend": "falling", "over_turns": 2 } },
                          { "flag": "x" } ] }"#,
        );
        let mut signals = Vec::new();
        predicate.collect_signals(&mut signals);
        assert_eq!(signals, vec!["band.count", "provisions.total"]);
    }
}
