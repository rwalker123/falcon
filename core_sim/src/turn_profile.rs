//! Per-phase wall-clock profiler for a single simulation turn.
//!
//! The server's `turn.completed` event reports one `duration_ms` for the whole turn, which is
//! enough to notice a slow turn and useless for locating it. This module accumulates *named*
//! phases across a turn so the server can emit a second `turn.profile` event with the breakdown.
//!
//! # Two ways to record
//!
//! * **Stages** — [`enter_stage`] closes whatever stage is open and opens a new one. This is how
//!   the chained `TurnStage` sets are timed: an RAII guard cannot span two Bevy systems, so the
//!   boundary between two stages is marked by a zero-param marker system ([`stage_marker`])
//!   scheduled between them. Exactly one stage is open at a time.
//! * **Scopes** — [`scope`] returns an RAII guard that folds its elapsed time into the named entry
//!   on drop. Independent of stage tracking, so it nests at any depth inside a stage.
//!
//! # Nesting is flat, and parents include their children
//!
//! There is no tree. Every label is one flat slot, so a nested scope's time is counted **both** in
//! its own entry and in every enclosing stage/scope. `snapshot.build` therefore contains
//! `snapshot.build.tiles`, `snapshot.build.flora`, and so on; the `snapshot` stage contains all of
//! them plus `snapshot.finalize_hash`. The dotted label names carry the hierarchy — do not read
//! the numbers as a disjoint partition that should sum to the turn duration.
//!
//! Entries are ordered by when each label was first **opened**, which is what keeps a parent ahead
//! of its children in the rendered line.
//!
//! # Global, not thread-local
//!
//! The accumulator is a process-wide `Mutex`, deliberately: Bevy's multi-threaded executor may run
//! two systems of the same turn on different threads, and a thread-local would silently drop every
//! span recorded off the main thread. At the couple dozen spans a turn takes, the lock traffic is
//! far below the measurement noise, which is why the profiler is always on — no env flag, no
//! feature gate, so the numbers are in any operator's log.
//!
//! A poisoned lock is recovered from rather than propagated ([`std::sync::PoisonError::into_inner`]):
//! a profiler must never be the thing that takes the server down.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Milliseconds per second — the unit conversion [`render`] reports every phase in.
const MILLIS_PER_SECOND: f64 = 1_000.0;

/// Decimal places each phase's millisecond figure is rendered with. Two is enough to separate a
/// sub-tenth-of-a-millisecond phase from a genuinely free one without burying the log line in
/// digits.
const PHASE_MS_PRECISION: usize = 2;

/// One named phase's accumulated time across a turn.
#[derive(Debug, Clone)]
pub struct PhaseTiming {
    /// The phase's dotted label, e.g. `snapshot.build.tiles`.
    pub label: &'static str,
    /// Summed wall-clock time over every entry into this label this turn.
    pub total: Duration,
    /// How many times this label was entered this turn (1 for most).
    pub calls: u32,
}

#[derive(Default)]
struct ProfileState {
    /// Phases in first-**opened** order. Re-entering a label accumulates into its existing slot and
    /// keeps its original position, so the rendered line reads as a timeline.
    ///
    /// Ordering by open time (rather than by the moment the elapsed time is folded in) is what puts
    /// a parent *before* its children: a scope closes before the stage enclosing it does, so
    /// close-ordering would print `snapshot.build.tiles` ahead of `snapshot.build` and the dotted
    /// hierarchy would read inside-out.
    entries: Vec<PhaseTiming>,
    /// The stage opened by the most recent [`enter_stage`], if any.
    open_stage: Option<(&'static str, Instant)>,
}

impl ProfileState {
    const fn new() -> Self {
        Self {
            entries: Vec::new(),
            open_stage: None,
        }
    }

    /// Claim `label`'s slot at the moment it is *opened*, so the entry list stays in open order.
    /// A no-op for a label already present. A linear scan is right here: a turn produces a couple
    /// dozen entries, and preserving insertion order is the whole point.
    fn reserve(&mut self, label: &'static str) {
        if !self.entries.iter().any(|entry| entry.label == label) {
            self.entries.push(PhaseTiming {
                label,
                total: Duration::ZERO,
                calls: 0,
            });
        }
    }

    /// Fold `elapsed` into `label`'s slot, which [`ProfileState::reserve`] created when the phase
    /// opened (recreated defensively if a `take` intervened).
    fn record(&mut self, label: &'static str, elapsed: Duration) {
        self.reserve(label);
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.label == label) {
            entry.total += elapsed;
            entry.calls += 1;
        }
    }

    fn close_open_stage(&mut self) {
        if let Some((label, started)) = self.open_stage.take() {
            let elapsed = started.elapsed();
            self.record(label, elapsed);
        }
    }
}

static PROFILE: Mutex<ProfileState> = Mutex::new(ProfileState::new());

/// Lock the accumulator, recovering from poisoning rather than panicking.
fn state() -> MutexGuard<'static, ProfileState> {
    PROFILE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Start a fresh turn: discard any accumulated phases and any half-open stage.
///
/// Called by the server at the top of turn resolution — deliberately *not* by a marker system,
/// because order application and snapshot broadcast happen outside `app.update()` and belong to
/// the same turn's profile.
pub fn begin_turn() {
    let mut profile = state();
    profile.open_stage = None;
    profile.entries.clear();
}

/// Close the currently-open stage and open `label` in its place.
pub fn enter_stage(label: &'static str) {
    let mut profile = state();
    profile.close_open_stage();
    profile.reserve(label);
    profile.open_stage = Some((label, Instant::now()));
}

/// Close the currently-open stage, if there is one. Idempotent.
pub fn close_stage() {
    state().close_open_stage();
}

/// Close any open stage and drain the accumulated phases in open (chronological) order, leaving the
/// accumulator empty and ready for the next [`begin_turn`].
pub fn take() -> Vec<PhaseTiming> {
    let mut profile = state();
    profile.close_open_stage();
    std::mem::take(&mut profile.entries)
}

/// Open an RAII timing scope for `label`; its elapsed time is folded into `label`'s entry on drop.
///
/// Bind it to a named local (`let _s = turn_profile::scope("…")`) — binding to `_` would drop it
/// immediately and measure nothing.
pub fn scope(label: &'static str) -> Scope {
    state().reserve(label);
    Scope {
        label,
        started: Instant::now(),
    }
}

/// RAII guard returned by [`scope`].
pub struct Scope {
    label: &'static str,
    started: Instant,
}

impl Drop for Scope {
    fn drop(&mut self) {
        let elapsed = self.started.elapsed();
        state().record(self.label, elapsed);
    }
}

/// Build a zero-param Bevy system that opens `label` as the current stage (closing the previous
/// one). Schedule one between each pair of chained `TurnStage` sets.
///
/// The markers carry **no capability `run_if` gate**, unlike the stages they bracket: a gated-off
/// stage should record ~0 rather than vanish from the profile, which is itself information.
pub fn stage_marker(label: &'static str) -> impl FnMut() + Clone {
    move || enter_stage(label)
}

/// Tail marker: a zero-param Bevy system that closes the last open stage. Schedule after the final
/// `TurnStage` set.
pub fn close_stages_system() {
    close_stage();
}

/// Render phases into one compact, grep-able line for a `tracing` string field (fields must be
/// primitives, so the whole breakdown rides as one value).
///
/// Format: space-separated `label=<milliseconds>`, chronological, two decimals. A label entered
/// more than once this turn carries its count: `snapshot.build.rasters=3.10(x2)`. Labels entered
/// exactly once — the overwhelming majority — carry no suffix. `(x0)` means the phase was still
/// open when the profile was taken, so nothing was folded in.
pub fn render(phases: &[PhaseTiming]) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    for phase in phases {
        if !out.is_empty() {
            out.push(' ');
        }
        let millis = phase.total.as_secs_f64() * MILLIS_PER_SECOND;
        let _ = write!(out, "{}={:.*}", phase.label, PHASE_MS_PRECISION, millis);
        if phase.calls != 1 {
            let _ = write!(out, "(x{})", phase.calls);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The accumulator is a **process global**, so this is deliberately one test function rather
    /// than several: `cargo test` runs test functions on parallel threads, and two tests each
    /// calling `begin_turn`/`take` would clobber each other's entries.
    ///
    /// For the same reason it asserts on a *filtered* view of `take()`: other tests running
    /// concurrently drive real turns, whose `snapshot.*` scopes land in the same accumulator. The
    /// `TEST_PREFIX` labels are unique to this test, so filtering to them makes the assertions
    /// deterministic while still proving the ordering and accumulation rules.
    #[test]
    fn stages_and_scopes_accumulate_in_chronological_order() {
        const TEST_PREFIX: &str = "turn_profile.test.";
        const STAGE_A: &str = "turn_profile.test.a";
        const STAGE_B: &str = "turn_profile.test.b";
        const NESTED: &str = "turn_profile.test.a.nested";

        begin_turn();
        enter_stage(STAGE_A);
        // Both nested scopes sit strictly inside stage A's first stint, so A's total is guaranteed
        // to contain the whole of NESTED's — the flat-nesting property asserted below.
        {
            let _nested = scope(NESTED);
        }
        {
            let _nested = scope(NESTED);
        }
        enter_stage(STAGE_B);
        // Re-entering an already-recorded stage must accumulate into its original slot rather than
        // appending a second entry.
        enter_stage(STAGE_A);

        let phases = take();
        let mine: Vec<&PhaseTiming> = phases
            .iter()
            .filter(|phase| phase.label.starts_with(TEST_PREFIX))
            .collect();

        let labels: Vec<&str> = mine.iter().map(|phase| phase.label).collect();
        assert_eq!(
            labels,
            vec![STAGE_A, NESTED, STAGE_B],
            "entries keep first-OPENED order, so a parent stays ahead of the scope nested in it \
             (the nested scope closes first, and close-ordering would invert them)"
        );

        assert_eq!(mine[0].calls, 2, "re-entered stage accumulates its calls");
        assert_eq!(mine[1].calls, 2, "re-entered scope accumulates its calls");
        assert_eq!(mine[2].calls, 1);
        assert!(
            mine[0].total >= mine[1].total,
            "nesting is flat and the parent stage's total INCLUDES the scope nested inside it"
        );

        // `take` must leave the accumulator empty for the next turn.
        let drained = take();
        assert!(
            !drained
                .iter()
                .any(|phase| phase.label.starts_with(TEST_PREFIX)),
            "take() drains the accumulator"
        );

        let rendered = render(&mine.into_iter().cloned().collect::<Vec<_>>());
        assert!(
            rendered.contains(&format!("{STAGE_A}=")),
            "rendered line names each phase: {rendered}"
        );
        assert!(
            rendered.contains("(x2)"),
            "rendered line carries the call count when != 1: {rendered}"
        );
    }
}
