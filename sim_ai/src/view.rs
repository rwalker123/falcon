//! **Perception** — the frame is the view (`docs/plan_ai_driver.md` §2).
//!
//! [`SeatView`] is the decoded `WorldSnapshot` this seat was sent, kept current by applying each
//! `WorldDelta` as it arrives. There is no second representation of "what I can see": fog is a
//! property of the bytes, and a tile the seat has not seen reads as unexplored in the visibility
//! raster.
//!
//! A delta that does not name the frame the view holds (`ApplyDeltaError`) means the chain broke —
//! a dropped frame, a world rebuild — and the only honest answer is to ask for a full frame and
//! ignore deltas until it arrives. The event feed is append-only, so applying anything else would
//! lose history silently.
//!
//! [`SeatMemory`] is what the frame no longer says: the last tick each tile was seen (decayed by
//! the difficulty's horizon), last turn's chosen intents (the commitment key), the alarms raised,
//! the move targets a band is still walking toward, the splits ordered and the children they
//! produced, and last turn's runway per band. A full frame (resync, rollback) drops every entry
//! stamped later than the new tick — nothing is patched.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use sim_runtime::{
    decode_frame_flatbuffer, ApplyDeltaError, DecodeError, ForagePatchState, FramePayload,
    LaborAssignmentState, PopulationCohortState, SnapshotHeader, WorldSnapshot, FIXED_POINT_SCALE,
};
use tracing::{info, warn};

use crate::geometry::{Grid, Tile};
use crate::orchestrator::Alarm;
use crate::profile::NO_MEMORY_DECAY;
use crate::specialists::Memo;

/// **The visibility raster's values.** Restated from `core_sim::visibility::VisibilityState`
/// (`Unexplored = 0`, `Discovered = 1`, `Active = 2`), **published fixed-point** by
/// `visibility_raster_from_ledger` (`core_sim/src/snapshot/vision.rs`): `Active` is `Scalar::SCALE`
/// (1.0), `Discovered` is half of it (0.5), `Unexplored` is 0; the server's are the authority.
pub const VISIBILITY_DISCOVERED: i64 = FIXED_POINT_SCALE / 2;
pub const VISIBILITY_ACTIVE: i64 = FIXED_POINT_SCALE;

/// What this seat holds, and the last tick it acted on.
pub struct SeatView {
    pub snapshot: WorldSnapshot,
    /// The tick the brain last decided on. A mid-turn recapture re-sends the same tick and must
    /// not be acted on twice.
    pub last_acted_tick: Option<u64>,
}

impl SeatView {
    pub fn tick(&self) -> u64 {
        self.snapshot.header.tick
    }

    /// The grid every map-sized raster is laid out on.
    pub fn grid(&self) -> Grid {
        let raster = &self.snapshot.visibility_raster;
        Grid {
            width: raster.width,
            height: raster.height,
            wrap_horizontal: self.snapshot.header.wrap_horizontal,
        }
    }

    /// The raw visibility sample at `tile`; unexplored when the raster does not cover it.
    pub fn visibility(&self, tile: Tile) -> i64 {
        self.grid()
            .index(tile)
            .and_then(|index| self.snapshot.visibility_raster.samples.get(index))
            .copied()
            .unwrap_or_default()
    }

    pub fn is_discovered(&self, tile: Tile) -> bool {
        self.visibility(tile) >= VISIBILITY_DISCOVERED
    }

    /// This faction's resident bands — not its detached parties.
    pub fn own_bands(&self, faction: u32) -> impl Iterator<Item = &PopulationCohortState> {
        self.snapshot
            .populations
            .iter()
            .filter(move |cohort| cohort.faction == faction && !cohort.is_expedition)
    }

    pub fn band(&self, band_id: u64) -> Option<&PopulationCohortState> {
        self.snapshot
            .populations
            .iter()
            .find(|cohort| cohort.band_id == band_id)
    }

    pub fn patch_at(&self, tile: Tile) -> Option<&ForagePatchState> {
        self.snapshot
            .forage_patches
            .iter()
            .find(|patch| patch.x == tile.x && patch.y == tile.y)
    }
}

/// Where a band stands.
pub fn band_tile(cohort: &PopulationCohortState) -> Tile {
    Tile::new(cohort.current_x, cohort.current_y)
}

/// **The key a worked row is remembered under**: `<band>:<kind>:<x>,<y>` for a tile source,
/// `<band>:<kind>:<fauna_id>` for a herd — the same shape `Food` builds for a source it is about to
/// propose, so the two meet in [`SeatMemory::realized`].
pub fn row_key(band_id: u64, kind: &str, x: u32, y: u32, fauna_id: &str) -> String {
    if fauna_id.is_empty() {
        format!("{band_id}:{kind}:{x},{y}")
    } else {
        format!("{band_id}:{kind}:{fauna_id}")
    }
}

fn key_of_row(band_id: u64, row: &LaborAssignmentState) -> String {
    row_key(
        band_id,
        &row.kind,
        row.target_x,
        row.target_y,
        &row.fauna_id,
    )
}

/// The `worked_turns` a row the sim marks `hunt_useful_workers == 0` jumps to: dead at once,
/// whatever the profile's `dead_row_turns`. **It is not permanent** — the next turn the sim
/// reports a useful crew on that row clears it back to [`WORKED_ONCE`] ([`SeatMemory::observe`]).
pub const WORKED_DEAD_AT_ONCE: u32 = u32::MAX;

/// The `worked_turns` of a row's first measured turn — and of the first turn after a
/// [`WORKED_DEAD_AT_ONCE`] marking is cleared, since the record before it measured nothing.
const WORKED_ONCE: u32 = 1;

/// **The denominator of a yield-per-worker observation**: the workers whose work the sim counted.
///
/// On a **hunt** row that is `hunt_useful_workers`, and `0` there is the sim saying *no crew is
/// useful here* (`LaborAssignmentState::hunt_useful_workers`) — the hunt did not happen, so the
/// row's `actual_yield` of `0.0` measures nothing about the quarry. Every other row is measured on
/// the crew itself: `hunt_useful_workers` is `0` on a non-hunt row by construction, so it must
/// never be read as one there, and [`SeatMemory::observe`] only folds rows that carry workers.
fn useful_workers(row: &LaborAssignmentState) -> u32 {
    if row.kind == crate::specialists::food::ROLE_HUNT {
        row.hunt_useful_workers
    } else {
        row.workers
    }
}

/// What a worked row has actually paid — the measurement a forecast is held against.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Realized {
    /// Last turn's `actual_yield / useful_workers` — **`None` when the row was not a measurement
    /// at all**,
    /// which is a row no worker was useful on ([`useful_workers`]). A zero-denominator observation
    /// is not evidence: folded in as a `0.0` it says "this source pays nothing", which is a claim
    /// the turn never tested.
    pub per_worker: Option<f32>,
    /// Consecutive turns the row has been worked; kept when the row is emptied, so the source it
    /// named is judged on its record rather than picked afresh the moment it is free.
    pub worked_turns: u32,
}

/// **A split ordered and not yet seen to happen**: the parent band, the site the child is for,
/// and the crew asked for. The sim spawns the child on the parent's tile next turn
/// (`split_band_from_parent`, `core_sim/src/systems/fission.rs`); an entry the next frames never
/// match is a split the sim refused, and it is dropped after the profile's `split_settle_turns`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitPending {
    pub tick: u64,
    pub target: Tile,
    pub workers: u32,
}

/// **A band this seat split off**, and the site it was split toward — dropped when it arrives
/// there, or when the memory horizon passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitBirth {
    pub tick: u64,
    pub target: Tile,
}

/// What the frame no longer says (module docs).
#[derive(Debug, Default)]
pub struct SeatMemory {
    /// The difficulty's `memory_horizon_turns`; [`NO_MEMORY_DECAY`] never forgets.
    horizon: u64,
    /// The profile's `food.split_settle_turns`: how long a pending split waits for its child.
    split_settle_turns: u32,
    /// The intents chosen on the last acted tick, and that tick.
    chosen: Option<(u64, BTreeSet<String>)>,
    /// The last tick each tile was in active sight — or first known of, for ground discovered
    /// before this process was watching.
    last_seen: HashMap<Tile, u64>,
    /// Alarms raised since the orchestrator last planned.
    alarms: Vec<Alarm>,
    /// Bands still walking toward a move target, and the intent the move was accepted under —
    /// whichever specialist proposed it ([`Memo::Move`]).
    move_targets: BTreeMap<u64, (Tile, String)>,
    /// The own bands the last observed frame carried, so a band not among them is *new*.
    known_bands: BTreeSet<u64>,
    /// Splits accepted and not yet matched to a child, by parent band.
    pending_splits: BTreeMap<u64, SplitPending>,
    /// Children this seat split off and their sites, by child band.
    born_by_split: BTreeMap<u64, SplitBirth>,
    /// **The working-age a band had when the sim refused to split it.** The sim's floors
    /// (`min_founding_workers`, `parent_min_workers`) are not on the wire and are not copied here;
    /// what the frame teaches is that a band of *this* size cannot split, so a split is asked for
    /// again only once the band has grown. Kept across the horizon: a refusal is a fact about
    /// the sim, not a sighting.
    split_refused: BTreeMap<u64, u32>,
    /// **Per band, the tile it last departed from on an accepted move, and when.** *Better
    /// ground* will not walk a band back onto it while it is remembered (decayed by the horizon).
    left_from: BTreeMap<u64, (Tile, u64)>,
    /// Last turn's `turns_of_food` per band, so "falling" is answerable.
    previous_runway: BTreeMap<u64, f32>,
    /// What each worked row ([`row_key`]) has realized.
    realized: BTreeMap<String, Realized>,
    /// What every worked row of each `kind` (`forage`, `hunt`, …) has realized per worker, summed,
    /// and over how many row-turns — the seat's experience of a whole web, the prior for a source
    /// of that web it has not worked.
    realized_by_kind: BTreeMap<String, (f32, u32)>,
}

impl SeatMemory {
    /// `horizon` is the difficulty's `memory_horizon_turns`; `split_settle_turns` the profile's
    /// `food.split_settle_turns` — passed here rather than to every `observe`, because it is a
    /// fact about this seat and not about a frame.
    pub fn new(horizon: u64, split_settle_turns: u32) -> Self {
        Self {
            horizon,
            split_settle_turns,
            ..Default::default()
        }
    }

    /// Fold one frame in: sightings, arrivals at move targets, the children of accepted splits,
    /// and `faction`'s rows that paid.
    pub fn observe(&mut self, view: &SeatView, faction: u32) {
        let tick = view.tick();
        self.observe_births(view, faction, tick);
        for band in view.own_bands(faction) {
            for row in band.labor_assignments.iter().filter(|row| row.workers > 0) {
                let key = key_of_row(band.band_id, row);
                let useful = useful_workers(row);
                let previous = self.realized.get(&key).map_or(0, |r| r.worked_turns);
                // ⛔ **THE SIM'S VERDICT IS PER TURN, SO THE MARK IT LEAVES MUST BE TOO.** A turn
                // the sim reports a useful crew is a turn the row is alive, whatever it was
                // yesterday — a bare-handed band's failed hunt must not blacklist the herd for the
                // rest of the run once the band has a kit. The count starts afresh, so the row is
                // judged on the record it has since made.
                let worked_turns = match (useful, previous) {
                    (0, _) => WORKED_DEAD_AT_ONCE,
                    (_, WORKED_DEAD_AT_ONCE) => WORKED_ONCE,
                    (_, previous) => previous + 1,
                };
                // ⛔ **DIVIDED BY THE CREW THE SIM COUNTED, NOT THE CREW ASSIGNED.** On a hunt row
                // `useful_workers` is the crew-take plateau (`hunt_useful_workers`), and
                // `per_worker_yield` is a rate *up to* it: twelve hands on a plateau of two bring
                // home what two bring home, so dividing the take by twelve reads as a sixth of the
                // forecast and declares a perfectly good herd dead.
                let per_worker = (useful > 0).then(|| row.actual_yield / useful as f32);
                self.realized.insert(
                    key,
                    Realized {
                        per_worker,
                        worked_turns,
                    },
                );
                // Only a measurement joins the web's mean: a row nobody was useful on would
                // otherwise fold a `0.0` into every *other* source of its kind, for good.
                if let Some(per_worker) = per_worker {
                    let (sum, count) = self
                        .realized_by_kind
                        .entry(row.kind.clone())
                        .or_insert((0.0, 0));
                    *sum += per_worker;
                    *count += 1;
                }
            }
        }
        let grid = view.grid();
        for (index, sample) in view.snapshot.visibility_raster.samples.iter().enumerate() {
            if grid.width == 0 {
                break;
            }
            let tile = Tile::new(index as u32 % grid.width, index as u32 / grid.width);
            if *sample >= VISIBILITY_ACTIVE {
                self.last_seen.insert(tile, tick);
            } else if *sample >= VISIBILITY_DISCOVERED {
                self.last_seen.entry(tile).or_insert(tick);
            }
        }
        self.move_targets.retain(|band_id, (target, _)| {
            view.band(*band_id)
                .is_some_and(|cohort| band_tile(cohort) != *target)
        });
    }

    /// **A new own band standing where a parent with a pending split stands is that split's
    /// child.** The sim spawns it on the parent's tile the turn after the order
    /// (`split_band_from_parent`); a pending entry no child has matched within
    /// `split_settle_turns` is a refused split and is dropped. A child is remembered until it
    /// reaches its site, or the horizon passes.
    fn observe_births(&mut self, view: &SeatView, faction: u32, tick: u64) {
        let own: Vec<&PopulationCohortState> = view.own_bands(faction).collect();
        for child in own
            .iter()
            .filter(|band| !self.known_bands.contains(&band.band_id))
        {
            let parent = self.pending_splits.iter().find(|(parent_id, _)| {
                view.band(**parent_id)
                    .is_some_and(|parent| band_tile(parent) == band_tile(child))
            });
            if let Some((parent_id, pending)) = parent.map(|(id, pending)| (*id, *pending)) {
                self.pending_splits.remove(&parent_id);
                self.born_by_split.insert(
                    child.band_id,
                    SplitBirth {
                        tick,
                        target: pending.target,
                    },
                );
            }
        }
        let settle = u64::from(self.split_settle_turns);
        let expired: Vec<u64> = self
            .pending_splits
            .iter()
            .filter(|(_, pending)| tick.saturating_sub(pending.tick) > settle)
            .map(|(parent, _)| *parent)
            .collect();
        for parent in expired {
            self.pending_splits.remove(&parent);
            // No child came: the sim refused the split. What it refused is a band of *this*
            // size, so the size is what is remembered.
            if let Some(band) = view.band(parent) {
                self.split_refused.insert(parent, band.working_age);
            }
        }
        let horizon = self.horizon;
        self.left_from.retain(|_, (_, left_at)| {
            horizon == NO_MEMORY_DECAY || tick.saturating_sub(*left_at) <= horizon
        });
        self.born_by_split.retain(|band_id, birth| {
            let unexpired =
                horizon == NO_MEMORY_DECAY || tick.saturating_sub(birth.tick) <= horizon;
            unexpired
                && view
                    .band(*band_id)
                    .is_some_and(|child| band_tile(child) != birth.target)
        });
        self.known_bands = own.iter().map(|band| band.band_id).collect();
    }

    /// The split accepted on `band_id` that no child has yet appeared for.
    pub fn pending_split(&self, band_id: u64) -> Option<&SplitPending> {
        self.pending_splits.get(&band_id)
    }

    /// The birth record of `band_id`, if this seat split it off and it has not reached its site.
    pub fn born_by_split(&self, band_id: u64) -> Option<&SplitBirth> {
        self.born_by_split.get(&band_id)
    }

    /// The working-age `band_id` had when the sim last refused to split it, if it ever did.
    pub fn split_refused_at(&self, band_id: u64) -> Option<u32> {
        self.split_refused.get(&band_id).copied()
    }

    /// The tile `band_id` most recently departed on an accepted move, while remembered.
    pub fn left_from(&self, band_id: u64) -> Option<Tile> {
        self.left_from.get(&band_id).map(|(tile, _)| *tile)
    }

    /// The last tick `tile` was in active sight (or was first known of), undecayed — what the
    /// observation record reports so a viewer can say how stale a tile's reading is.
    pub fn last_seen(&self, tile: Tile) -> Option<u64> {
        self.last_seen.get(&tile).copied()
    }

    /// The alarms raised since the orchestrator last planned — what the next plan will weigh.
    pub fn pending_alarms(&self) -> &[Alarm] {
        &self.alarms
    }

    /// Whether `tile` counts as known at `now`: seen, and not longer ago than the horizon.
    pub fn is_known(&self, tile: Tile, now: u64) -> bool {
        match self.last_seen.get(&tile) {
            None => false,
            Some(_) if self.horizon == NO_MEMORY_DECAY => true,
            Some(seen) => now.saturating_sub(*seen) <= self.horizon,
        }
    }

    /// How many tiles within `radius` of `center` are known at `now`.
    pub fn known_tiles_within(
        &self,
        view: &SeatView,
        center: Tile,
        radius: u32,
        now: u64,
    ) -> usize {
        view.grid()
            .disk(center, radius)
            .into_iter()
            .filter(|tile| self.is_known(*tile, now))
            .count()
    }

    /// Whether `intent` was among last turn's choices.
    pub fn chosen_last_turn(&self, intent: &str) -> bool {
        self.chosen
            .as_ref()
            .is_some_and(|(_, intents)| intents.contains(intent))
    }

    /// Record this turn's accepted intents and what their memos say to remember: a move target
    /// (under the intent it was accepted with), or a split awaiting its child.
    pub fn record_choices(
        &mut self,
        tick: u64,
        choices: impl Iterator<Item = (String, Option<Memo>)>,
    ) {
        let mut intents = BTreeSet::new();
        for (intent, memo) in choices {
            match memo {
                Some(Memo::Move { band, target, from }) => {
                    self.move_targets.insert(band, (target, intent.clone()));
                    self.left_from.insert(band, (from, tick));
                }
                Some(Memo::Split {
                    band,
                    target,
                    workers,
                }) => {
                    self.pending_splits.insert(
                        band,
                        SplitPending {
                            tick,
                            target,
                            workers,
                        },
                    );
                }
                None => {}
            }
            intents.insert(intent);
        }
        self.chosen = Some((tick, intents));
    }

    /// What the row under `key` has realized, if it has ever been worked.
    pub fn realized(&self, key: &str) -> Option<Realized> {
        self.realized.get(key).copied()
    }

    /// The mean per-worker take this seat has realized across every **measured** row of `kind`, if
    /// it has worked any — a weak prior for a source of that web it has not tried, and never a
    /// verdict on one: a source with its own observation, or absent one its own forecast, outranks
    /// it (`Food::rate`).
    pub fn realized_for_kind(&self, kind: &str) -> Option<f32> {
        self.realized_by_kind
            .get(kind)
            .filter(|(_, count)| *count > 0)
            .map(|(sum, count)| sum / *count as f32)
    }

    pub fn move_target(&self, band_id: u64) -> Option<Tile> {
        self.move_targets.get(&band_id).map(|(target, _)| *target)
    }

    /// The intent the band's standing move was accepted under (`land:move:<band>`,
    /// `food:settle:<band>`) — the commitment the arbiter will reward while it walks.
    pub fn move_intent(&self, band_id: u64) -> Option<&str> {
        self.move_targets
            .get(&band_id)
            .map(|(_, intent)| intent.as_str())
    }

    /// Whether `band`'s runway is below what it was last turn.
    pub fn runway_falling(&self, band_id: u64, runway_now: f32) -> bool {
        self.previous_runway
            .get(&band_id)
            .is_some_and(|previous| runway_now < *previous)
    }

    /// Remember this turn's runways for next turn's "falling".
    pub fn remember_runways(&mut self, view: &SeatView, faction: u32) {
        self.previous_runway = view
            .own_bands(faction)
            .map(|cohort| (cohort.band_id, cohort.turns_of_food))
            .collect();
    }

    pub fn push_alarm(&mut self, alarm: Alarm) {
        self.alarms.push(alarm);
    }

    /// The alarms raised since the last plan, handed over once.
    pub fn take_alarms(&mut self) -> Vec<Alarm> {
        std::mem::take(&mut self.alarms)
    }

    /// A full frame at `tick` replaced the view: everything stamped later than it is gone, and
    /// the per-turn carry (runways, targets) is unknown again.
    pub fn forget_after(&mut self, tick: u64) {
        self.last_seen.retain(|_, seen| *seen <= tick);
        if self.chosen.as_ref().is_some_and(|(at, _)| *at > tick) {
            self.chosen = None;
        }
        self.alarms.retain(|alarm| alarm.since_tick <= tick);
        self.move_targets.clear();
        self.previous_runway.clear();
        self.realized.clear();
        self.realized_by_kind.clear();
        self.pending_splits
            .retain(|_, pending| pending.tick <= tick);
        self.born_by_split.retain(|_, birth| birth.tick <= tick);
        self.known_bands.clear();
        self.split_refused.clear();
        self.left_from.clear();
    }
}

/// What ingesting one frame did.
#[derive(Debug)]
pub enum FrameOutcome {
    /// A full frame replaced the view.
    Replaced,
    /// A delta merged into the view.
    Applied,
    /// A delta did not name the frame held: the view is stale until a full frame arrives, and the
    /// caller should ask for one.
    ChainBroken(ApplyDeltaError),
    /// A delta arrived while a full frame was already being waited for; dropped.
    AwaitingFullFrame,
    /// The bytes were not a frame this build understands.
    Undecodable(DecodeError),
}

/// ⛔ **THE TICK ALREADY ACTED ON BELONGS TO ONE WORLD.** What a full frame may keep from the view
/// it replaces: the held `last_acted_tick`, but only while the incoming frame is the **same
/// world's** and no older than that tick.
///
/// A `new_game` or a load from the client's menu rebuilds the world under a live seat — the
/// launcher keeps this process (its faction is still in the roster) and `retain_claimed_seats`
/// keeps the claim — so the next full frame is tick 0 of a **new `world_epoch`**. Carrying the old
/// world's tick across it makes the turn loop's `tick <= acted` skip both `decide` and the
/// `Orders { Ready }` that follows it, for every turn up to the old world's last: the seat is
/// occupied and silent, and each of those turns waits out the server's `seat_turn_timeout_seconds`
/// before it is auto-submitted. A frame older than the acted tick is the same rebuild seen a
/// second way (or a rollback), and reads the same.
fn carried_acted_tick(held: &SeatView, incoming: &SnapshotHeader) -> Option<u64> {
    let acted = held.last_acted_tick?;
    let same_world = held.snapshot.header.world_epoch == incoming.world_epoch;
    (same_world && incoming.tick >= acted).then_some(acted)
}

/// The view plus the one piece of state the chain needs: whether a full frame is owed.
#[derive(Default)]
pub struct Perception {
    view: Option<SeatView>,
    awaiting_full_frame: bool,
}

impl Perception {
    pub fn view_mut(&mut self) -> Option<&mut SeatView> {
        self.view.as_mut()
    }

    /// Whether a resync has been requested and its full frame has not yet arrived.
    #[cfg(test)]
    fn awaiting_full_frame(&self) -> bool {
        self.awaiting_full_frame
    }

    /// Note that a full frame has been asked for, so deltas are dropped until it lands.
    pub fn expect_full_frame(&mut self) {
        self.awaiting_full_frame = true;
    }

    /// Fold one stream frame into the view.
    pub fn ingest(&mut self, bytes: &[u8]) -> FrameOutcome {
        match decode_frame_flatbuffer(bytes) {
            Ok(FramePayload::Snapshot(snapshot)) => {
                info!(
                    world_epoch = snapshot.header.world_epoch,
                    frame_seq = snapshot.header.frame_seq,
                    tick = snapshot.header.tick,
                    "full frame replaced the view"
                );
                let last_acted_tick = self
                    .view
                    .as_ref()
                    .and_then(|held| carried_acted_tick(held, &snapshot.header));
                self.view = Some(SeatView {
                    snapshot,
                    last_acted_tick,
                });
                self.awaiting_full_frame = false;
                FrameOutcome::Replaced
            }
            Ok(FramePayload::Delta(delta)) => {
                if self.awaiting_full_frame {
                    return FrameOutcome::AwaitingFullFrame;
                }
                let Some(view) = self.view.as_mut() else {
                    // A delta before any full frame: the chain never started.
                    self.awaiting_full_frame = true;
                    return FrameOutcome::ChainBroken(ApplyDeltaError::BaseMismatch {
                        expected: 0,
                        got: delta.header.base_frame_seq,
                    });
                };
                match view.snapshot.apply_delta(&delta) {
                    Ok(()) => FrameOutcome::Applied,
                    Err(err) => {
                        warn!(%err, "delta chain broke; requesting a full frame");
                        self.awaiting_full_frame = true;
                        FrameOutcome::ChainBroken(err)
                    }
                }
            }
            Err(err) => FrameOutcome::Undecodable(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::AlarmKind;
    use crate::specialists::SPECIALIST_FOOD;
    use sim_runtime::{encode_delta_flatbuffer, encode_snapshot_flatbuffer, WorldDelta};

    const FIRST_FRAME: u64 = 3;
    const WIDTH: u32 = 4;
    const HEIGHT: u32 = 3;
    const HORIZON: u64 = 2;
    /// The turns a pending split waits for its child in these tests.
    const SETTLE: u32 = 3;

    fn a_full_frame() -> Vec<u8> {
        let mut snapshot = WorldSnapshot::default();
        snapshot.header.frame_seq = FIRST_FRAME;
        encode_snapshot_flatbuffer(&snapshot)
    }

    /// A full frame of the world `epoch`, at `tick`.
    fn a_full_frame_of(epoch: u32, tick: u64) -> Vec<u8> {
        let mut snapshot = WorldSnapshot::default();
        snapshot.header.frame_seq = FIRST_FRAME;
        snapshot.header.world_epoch = epoch;
        snapshot.header.tick = tick;
        encode_snapshot_flatbuffer(&snapshot)
    }

    fn a_delta_on(base: u64) -> Vec<u8> {
        let mut delta = WorldDelta::default();
        delta.header.base_frame_seq = base;
        delta.header.frame_seq = base + 1;
        encode_delta_flatbuffer(&delta)
    }

    /// A view at `tick` whose raster has tile (1, 0) active, (2, 0) discovered, the rest unexplored.
    fn a_view_at(tick: u64) -> SeatView {
        let mut snapshot = WorldSnapshot::default();
        snapshot.header.tick = tick;
        snapshot.visibility_raster.width = WIDTH;
        snapshot.visibility_raster.height = HEIGHT;
        snapshot.visibility_raster.samples = vec![0; (WIDTH * HEIGHT) as usize];
        snapshot.visibility_raster.samples[1] = VISIBILITY_ACTIVE;
        snapshot.visibility_raster.samples[2] = VISIBILITY_DISCOVERED;
        SeatView {
            snapshot,
            last_acted_tick: None,
        }
    }

    #[test]
    fn a_full_frame_replaces_and_a_chained_delta_applies() {
        let mut perception = Perception::default();
        assert!(matches!(
            perception.ingest(&a_full_frame()),
            FrameOutcome::Replaced
        ));
        assert!(matches!(
            perception.ingest(&a_delta_on(FIRST_FRAME)),
            FrameOutcome::Applied
        ));
        assert_eq!(
            perception
                .view_mut()
                .expect("a view")
                .snapshot
                .header
                .frame_seq,
            FIRST_FRAME + 1
        );
    }

    /// ⛔ **A REBUILT WORLD IS NOT A MID-TURN RECAPTURE.** `new_game` and a load rebuild the world
    /// under a live seat, and the frame that follows is tick 0 of a new epoch. If the tick already
    /// acted on carried across it, the turn loop would skip `decide` — and the `Orders { Ready }`
    /// that follows it — for every turn up to the old world's last, and each of them would wait out
    /// the server's seat turn timeout with the seat occupied and silent.
    #[test]
    fn a_rebuilt_world_clears_the_tick_already_acted_on() {
        const OLD_EPOCH: u32 = 1;
        const NEW_EPOCH: u32 = 2;
        const ACTED_TICK: u64 = 40;
        const REBUILT_TICK: u64 = 0;

        let mut perception = Perception::default();
        perception.ingest(&a_full_frame_of(OLD_EPOCH, ACTED_TICK));
        perception.view_mut().expect("a view").last_acted_tick = Some(ACTED_TICK);

        // A mid-turn recapture of the same world: the same tick, already acted on, is not acted on
        // twice.
        perception.ingest(&a_full_frame_of(OLD_EPOCH, ACTED_TICK));
        assert_eq!(
            perception.view_mut().expect("a view").last_acted_tick,
            Some(ACTED_TICK)
        );

        // The rebuild: a new epoch at tick 0. The brain must act on it.
        perception.ingest(&a_full_frame_of(NEW_EPOCH, REBUILT_TICK));
        let view = perception.view_mut().expect("a view");
        assert_eq!(
            view.last_acted_tick, None,
            "the new world was never acted on"
        );
        // The turn loop's own guard (`main.rs`: `tick <= acted` skips the turn), inverted.
        assert!(
            view.last_acted_tick.is_none_or(|acted| view.tick() > acted),
            "the turn loop would skip this tick"
        );

        // A rebuild the epoch did not catch: time ran backwards, which reads the same.
        perception.view_mut().expect("a view").last_acted_tick = Some(ACTED_TICK);
        perception.ingest(&a_full_frame_of(NEW_EPOCH, REBUILT_TICK));
        assert_eq!(perception.view_mut().expect("a view").last_acted_tick, None);
    }

    #[test]
    fn a_delta_off_the_chain_breaks_it_and_deltas_are_dropped_until_a_full_frame() {
        let mut perception = Perception::default();
        perception.ingest(&a_full_frame());
        assert!(matches!(
            perception.ingest(&a_delta_on(FIRST_FRAME + 5)),
            FrameOutcome::ChainBroken(_)
        ));
        assert!(perception.awaiting_full_frame());
        assert!(matches!(
            perception.ingest(&a_delta_on(FIRST_FRAME)),
            FrameOutcome::AwaitingFullFrame
        ));
        assert!(matches!(
            perception.ingest(&a_full_frame()),
            FrameOutcome::Replaced
        ));
        assert!(!perception.awaiting_full_frame());
    }

    #[test]
    fn a_delta_before_any_full_frame_is_a_broken_chain() {
        let mut perception = Perception::default();
        assert!(matches!(
            perception.ingest(&a_delta_on(FIRST_FRAME)),
            FrameOutcome::ChainBroken(_)
        ));
    }

    #[test]
    fn sightings_decay_past_the_horizon_unless_it_is_zero() {
        let mut memory = SeatMemory::new(HORIZON, SETTLE);
        memory.observe(&a_view_at(10), 1);
        let active = Tile::new(1, 0);
        let discovered = Tile::new(2, 0);
        let unexplored = Tile::new(3, 0);
        assert!(memory.is_known(active, 10));
        assert!(
            memory.is_known(discovered, 10),
            "discovered ground is known, dated now"
        );
        assert!(!memory.is_known(unexplored, 10));
        assert!(memory.is_known(active, 10 + HORIZON));
        assert!(
            !memory.is_known(active, 10 + HORIZON + 1),
            "older than the horizon"
        );
        assert_eq!(
            memory.known_tiles_within(&a_view_at(10), Tile::new(1, 0), 1, 10),
            2
        );
        let mut forever = SeatMemory::new(NO_MEMORY_DECAY, SETTLE);
        forever.observe(&a_view_at(10), 1);
        assert!(forever.is_known(active, 10_000));
    }

    #[test]
    fn a_full_frame_forgets_what_was_stamped_after_it() {
        let mut memory = SeatMemory::new(NO_MEMORY_DECAY, SETTLE);
        memory.observe(&a_view_at(10), 1);
        memory.record_choices(
            10,
            [
                ("food:assign:1".to_owned(), None),
                (
                    "land:move:7".to_owned(),
                    Some(Memo::Move {
                        band: 7,
                        target: Tile::new(2, 2),
                        from: Tile::new(1, 1),
                    }),
                ),
                (
                    "food:split:8".to_owned(),
                    Some(Memo::Split {
                        band: 8,
                        target: Tile::new(3, 1),
                        workers: 5,
                    }),
                ),
            ]
            .into_iter(),
        );
        memory.push_alarm(Alarm {
            specialist: SPECIALIST_FOOD,
            kind: AlarmKind::FoodShort,
            since_tick: 10,
        });
        assert!(memory.chosen_last_turn("food:assign:1"));
        assert_eq!(memory.move_target(7), Some(Tile::new(2, 2)));
        assert_eq!(memory.move_intent(7), Some("land:move:7"));
        assert!(memory.pending_split(8).is_some());
        memory.forget_after(9);
        assert!(!memory.is_known(Tile::new(1, 0), 9));
        assert!(!memory.chosen_last_turn("food:assign:1"));
        assert_eq!(memory.move_target(7), None);
        assert_eq!(memory.pending_split(8), None);
        assert!(memory.take_alarms().is_empty());
    }

    /// The split bookkeeping: an accepted `food:split` is pending on the parent; the next frame's
    /// new own band on the parent's tile is its child, remembered with the site until it arrives;
    /// a pending entry no child answers within `split_settle_turns` is a refused split, dropped.
    #[test]
    fn a_pending_split_becomes_a_birth_on_the_next_frames_new_band_and_expires_unanswered() {
        const FACTION: u32 = 1;
        const PARENT: u64 = 7;
        const CHILD: u64 = 8;
        let site = Tile::new(3, 1);
        let mut memory = SeatMemory::new(NO_MEMORY_DECAY, SETTLE);
        let mut view = a_view_at(10);
        view.snapshot.populations.push(PopulationCohortState {
            faction: FACTION,
            band_id: PARENT,
            current_x: 1,
            current_y: 1,
            ..Default::default()
        });
        memory.observe(&view, FACTION);
        memory.record_choices(
            10,
            [(
                "food:split:7".to_owned(),
                Some(Memo::Split {
                    band: PARENT,
                    target: site,
                    workers: 5,
                }),
            )]
            .into_iter(),
        );
        assert_eq!(
            memory.pending_split(PARENT),
            Some(&SplitPending {
                tick: 10,
                target: site,
                workers: 5
            })
        );
        // Next frame: the child stands on the parent's tile.
        view.snapshot.header.tick = 11;
        view.snapshot.populations.push(PopulationCohortState {
            faction: FACTION,
            band_id: CHILD,
            current_x: 1,
            current_y: 1,
            ..Default::default()
        });
        memory.observe(&view, FACTION);
        assert_eq!(memory.pending_split(PARENT), None, "answered");
        assert_eq!(
            memory.born_by_split(CHILD),
            Some(&SplitBirth {
                tick: 11,
                target: site
            })
        );
        assert_eq!(memory.born_by_split(PARENT), None);
        // The child arrives at its site: the birth record is done.
        view.snapshot.header.tick = 14;
        let child = &mut view.snapshot.populations[1];
        child.current_x = site.x;
        child.current_y = site.y;
        memory.observe(&view, FACTION);
        assert_eq!(memory.born_by_split(CHILD), None, "arrived");

        // A split the sim refused: no new band ever appears, and the entry expires.
        let mut memory = SeatMemory::new(NO_MEMORY_DECAY, SETTLE);
        let mut view = a_view_at(20);
        view.snapshot.populations.push(PopulationCohortState {
            faction: FACTION,
            band_id: PARENT,
            working_age: 10,
            ..Default::default()
        });
        memory.observe(&view, FACTION);
        assert_eq!(memory.split_refused_at(PARENT), None);
        memory.record_choices(
            20,
            [(
                "food:split:7".to_owned(),
                Some(Memo::Split {
                    band: PARENT,
                    target: site,
                    workers: 5,
                }),
            )]
            .into_iter(),
        );
        view.snapshot.header.tick = 20 + u64::from(SETTLE);
        memory.observe(&view, FACTION);
        assert!(memory.pending_split(PARENT).is_some(), "still waiting");
        view.snapshot.header.tick = 20 + u64::from(SETTLE) + 1;
        memory.observe(&view, FACTION);
        assert_eq!(memory.pending_split(PARENT), None, "refused, forgotten");
        // …and what was refused — a band of ten — is remembered, across the horizon.
        assert_eq!(memory.split_refused_at(PARENT), Some(10));
        view.snapshot.header.tick = 200;
        memory.observe(&view, FACTION);
        assert_eq!(
            memory.split_refused_at(PARENT),
            Some(10),
            "a fact, not a sighting"
        );
        memory.forget_after(19);
        assert_eq!(memory.split_refused_at(PARENT), None);
    }

    /// The tile a band departed on an accepted move is remembered for the horizon, so *better
    /// ground* cannot walk it straight back; a full frame forgets it.
    #[test]
    fn the_tile_a_band_left_is_remembered_for_the_horizon_and_forgotten_by_a_full_frame() {
        const BAND: u64 = 7;
        let from = Tile::new(1, 1);
        let mut memory = SeatMemory::new(HORIZON, SETTLE);
        memory.record_choices(
            10,
            [(
                "land:move:7".to_owned(),
                Some(Memo::Move {
                    band: BAND,
                    target: Tile::new(2, 2),
                    from,
                }),
            )]
            .into_iter(),
        );
        assert_eq!(memory.left_from(BAND), Some(from));
        memory.observe(&a_view_at(10 + HORIZON), 1);
        assert_eq!(memory.left_from(BAND), Some(from), "within the horizon");
        memory.observe(&a_view_at(10 + HORIZON + 1), 1);
        assert_eq!(memory.left_from(BAND), None, "older than the horizon");
        let mut forever = SeatMemory::new(NO_MEMORY_DECAY, SETTLE);
        forever.record_choices(
            10,
            [(
                "land:move:7".to_owned(),
                Some(Memo::Move {
                    band: BAND,
                    target: Tile::new(2, 2),
                    from,
                }),
            )]
            .into_iter(),
        );
        forever.observe(&a_view_at(10_000), 1);
        assert_eq!(forever.left_from(BAND), Some(from));
        forever.forget_after(9);
        assert_eq!(forever.left_from(BAND), None);
    }

    #[test]
    fn a_runway_is_falling_only_against_a_remembered_one_and_a_target_clears_on_arrival() {
        const FACTION: u32 = 1;
        const BAND: u64 = 7;
        let mut memory = SeatMemory::new(NO_MEMORY_DECAY, SETTLE);
        assert!(!memory.runway_falling(BAND, 5.0), "nothing remembered yet");
        let mut view = a_view_at(10);
        view.snapshot.populations.push(PopulationCohortState {
            faction: FACTION,
            band_id: BAND,
            turns_of_food: 8.0,
            current_x: 2,
            current_y: 2,
            ..Default::default()
        });
        memory.remember_runways(&view, FACTION);
        assert!(memory.runway_falling(BAND, 7.0));
        assert!(!memory.runway_falling(BAND, 8.0));
        memory.record_choices(
            10,
            [(
                "land:move:7".to_owned(),
                Some(Memo::Move {
                    band: BAND,
                    target: Tile::new(2, 2),
                    from: Tile::new(2, 2),
                }),
            )]
            .into_iter(),
        );
        memory.observe(&view, FACTION);
        assert_eq!(
            memory.move_target(BAND),
            None,
            "the band stands on its target"
        );
    }

    #[test]
    fn a_worked_row_is_measured_and_a_useless_hunt_crew_is_dead_at_once() {
        const FACTION: u32 = 1;
        const BAND: u64 = 7;
        let mut memory = SeatMemory::new(NO_MEMORY_DECAY, SETTLE);
        let mut view = a_view_at(10);
        view.snapshot.populations.push(PopulationCohortState {
            faction: FACTION,
            band_id: BAND,
            labor_assignments: vec![
                LaborAssignmentState {
                    kind: "forage".into(),
                    target_x: 4,
                    target_y: 2,
                    workers: 3,
                    actual_yield: 0.6,
                    ..Default::default()
                },
                LaborAssignmentState {
                    kind: "hunt".into(),
                    fauna_id: "herd_9".into(),
                    workers: 5,
                    actual_yield: 0.0,
                    hunt_useful_workers: 0,
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
        let patch = row_key(BAND, "forage", 4, 2, "");
        let herd = row_key(BAND, "hunt", 0, 0, "herd_9");
        assert_eq!(memory.realized(&patch), None, "never worked");
        memory.observe(&view, FACTION);
        memory.observe(&view, FACTION);
        assert_eq!(
            memory.realized(&patch),
            Some(Realized {
                per_worker: Some(0.2),
                worked_turns: 2
            })
        );
        assert_eq!(
            memory.realized(&herd),
            Some(Realized {
                // ⛔ Not `Some(0.0)`: no crew was useful, so the turn measured nothing.
                per_worker: None,
                worked_turns: WORKED_DEAD_AT_ONCE
            })
        );
        assert_eq!(
            memory.realized_for_kind("hunt"),
            None,
            "a row nobody was useful on is not folded into the web's mean"
        );
        view.snapshot.populations[0].labor_assignments.clear();
        memory.observe(&view, FACTION);
        assert_eq!(
            memory.realized(&herd).map(|r| r.worked_turns),
            Some(WORKED_DEAD_AT_ONCE),
            "an emptied row keeps its record"
        );
        assert_eq!(
            memory.realized_for_kind("forage"),
            Some(0.2),
            "the web's mean over its row-turns"
        );
        assert_eq!(memory.realized_for_kind("scout"), None);
        memory.forget_after(9);
        assert_eq!(memory.realized(&herd), None);
        assert_eq!(memory.realized_for_kind("forage"), None);
    }

    /// ⛔ **A yield-per-worker observation with a zero denominator is not evidence.** A hunt the
    /// sim marks `hunt_useful_workers == 0` did not happen; folding its `0.0` into the web's mean
    /// once said "hunting pays nothing" about every other herd, for the rest of the run.
    #[test]
    fn a_useless_hunt_row_is_no_evidence_about_the_web_it_belongs_to() {
        const FACTION: u32 = 1;
        const BAND: u64 = 7;
        let mut memory = SeatMemory::new(NO_MEMORY_DECAY, SETTLE);
        let mut view = a_view_at(10);
        view.snapshot.populations.push(PopulationCohortState {
            faction: FACTION,
            band_id: BAND,
            labor_assignments: vec![LaborAssignmentState {
                kind: "hunt".into(),
                fauna_id: "herd_9".into(),
                workers: 12,
                actual_yield: 0.0,
                hunt_useful_workers: 0,
                ..Default::default()
            }],
            ..Default::default()
        });
        memory.observe(&view, FACTION);
        assert_eq!(memory.realized_for_kind("hunt"), None);
        // A hunt a crew *was* useful on is a measurement, and the only one the mean holds.
        view.snapshot.populations[0]
            .labor_assignments
            .push(LaborAssignmentState {
                kind: "hunt".into(),
                fauna_id: "herd_11".into(),
                workers: 4,
                actual_yield: 0.8,
                hunt_useful_workers: 4,
                ..Default::default()
            });
        memory.observe(&view, FACTION);
        assert_eq!(memory.realized_for_kind("hunt"), Some(0.2));
    }

    /// ⛔ **THE PLATEAU IS THE DENOMINATOR, NOT THE HEADCOUNT.** A hunt row's `per_worker_yield` is
    /// a rate up to `hunt_useful_workers` — the crew-take plateau (`core_sim/src/fauna.rs`) — so an
    /// over-staffed row takes what the plateau takes. Divided by the twelve hands assigned it reads
    /// as a sixth of what the sim forecast, which is under every profile's `poor_yield_fraction`,
    /// and the herd is declared dead and blacklisted. `Food::idle_hands` staffs rows without
    /// consulting the plateau, so the brain manufactures exactly this row.
    #[test]
    fn a_hunt_row_is_measured_against_its_useful_crew_not_its_headcount() {
        const FACTION: u32 = 1;
        const BAND: u64 = 7;
        const ASSIGNED: u32 = 12;
        const PLATEAU: u32 = 2;
        const TAKE: f32 = 3.0;
        let mut memory = SeatMemory::new(NO_MEMORY_DECAY, SETTLE);
        let mut view = a_view_at(10);
        view.snapshot.populations.push(PopulationCohortState {
            faction: FACTION,
            band_id: BAND,
            labor_assignments: vec![LaborAssignmentState {
                kind: "hunt".into(),
                fauna_id: "herd_9".into(),
                workers: ASSIGNED,
                actual_yield: TAKE,
                hunt_useful_workers: PLATEAU,
                ..Default::default()
            }],
            ..Default::default()
        });
        memory.observe(&view, FACTION);
        assert_eq!(
            memory
                .realized(&row_key(BAND, "hunt", 0, 0, "herd_9"))
                .and_then(|realized| realized.per_worker),
            Some(TAKE / PLATEAU as f32),
            "the take divided by the crew the sim counted"
        );
        assert_eq!(
            memory.realized_for_kind("hunt"),
            Some(TAKE / PLATEAU as f32)
        );
    }

    /// ⛔ **A DEAD ROW IS DEAD FOR THAT TURN, NOT FOR THE RUN.** `hunt_useful_workers` is a
    /// per-turn verdict: a bare-handed band cannot work a defended herd, and the same band with a
    /// kit can. Carrying the marking forward struck the herd off `reachable_sources` for the rest
    /// of the run — the row-level twin of the web-level veto `realized_for_kind` already guards.
    #[test]
    fn a_row_the_sim_reports_a_useful_crew_on_again_is_no_longer_dead() {
        const FACTION: u32 = 1;
        const BAND: u64 = 7;
        let mut memory = SeatMemory::new(NO_MEMORY_DECAY, SETTLE);
        let mut view = a_view_at(10);
        view.snapshot.populations.push(PopulationCohortState {
            faction: FACTION,
            band_id: BAND,
            labor_assignments: vec![LaborAssignmentState {
                kind: "hunt".into(),
                fauna_id: "herd_9".into(),
                workers: 4,
                actual_yield: 0.0,
                hunt_useful_workers: 0,
                ..Default::default()
            }],
            ..Default::default()
        });
        let herd = row_key(BAND, "hunt", 0, 0, "herd_9");
        memory.observe(&view, FACTION);
        assert_eq!(
            memory.realized(&herd).map(|realized| realized.worked_turns),
            Some(WORKED_DEAD_AT_ONCE)
        );
        // The band has a kit now, and the sim counts the crew.
        let row = &mut view.snapshot.populations[0].labor_assignments[0];
        row.hunt_useful_workers = 4;
        row.actual_yield = 2.0;
        memory.observe(&view, FACTION);
        assert_eq!(
            memory.realized(&herd),
            Some(Realized {
                per_worker: Some(0.5),
                worked_turns: WORKED_ONCE
            }),
            "the marking is cleared and the record starts from this turn"
        );
    }
}
