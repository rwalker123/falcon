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
//! the move targets a band is still walking toward, and last turn's runway per band. A full frame
//! (resync, rollback) drops every entry stamped later than the new tick — nothing is patched.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use sim_runtime::{
    decode_frame_flatbuffer, ApplyDeltaError, CommandPayload, DecodeError, ForagePatchState,
    FramePayload, LaborAssignmentState, PopulationCohortState, SnapshotHeader, WorldSnapshot,
    FIXED_POINT_SCALE,
};
use tracing::{info, warn};

use crate::geometry::{Grid, Tile};
use crate::orchestrator::Alarm;
use crate::profile::NO_MEMORY_DECAY;

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
/// whatever the profile's `dead_row_turns`.
pub const WORKED_DEAD_AT_ONCE: u32 = u32::MAX;

/// What a worked row has actually paid — the measurement a forecast is held against.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Realized {
    /// Last turn's `actual_yield / workers`.
    pub per_worker: f32,
    /// Consecutive turns the row has been worked; kept when the row is emptied, so the source it
    /// named is judged on its record rather than picked afresh the moment it is free.
    pub worked_turns: u32,
}

/// What the frame no longer says (module docs).
#[derive(Debug, Default)]
pub struct SeatMemory {
    /// The difficulty's `memory_horizon_turns`; [`NO_MEMORY_DECAY`] never forgets.
    horizon: u64,
    /// The intents chosen on the last acted tick, and that tick.
    chosen: Option<(u64, BTreeSet<String>)>,
    /// The last tick each tile was in active sight — or first known of, for ground discovered
    /// before this process was watching.
    last_seen: HashMap<Tile, u64>,
    /// Alarms raised since the orchestrator last planned.
    alarms: Vec<Alarm>,
    /// Bands still walking toward a `land:move` target.
    move_targets: BTreeMap<u64, Tile>,
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
    pub fn new(horizon: u64) -> Self {
        Self {
            horizon,
            ..Default::default()
        }
    }

    /// Fold one frame in: sightings, arrivals at move targets, and `faction`'s rows that paid.
    pub fn observe(&mut self, view: &SeatView, faction: u32) {
        let tick = view.tick();
        for band in view.own_bands(faction) {
            for row in band.labor_assignments.iter().filter(|row| row.workers > 0) {
                let key = key_of_row(band.band_id, row);
                let useless_hunt =
                    row.kind == crate::specialists::food::ROLE_HUNT && row.hunt_useful_workers == 0;
                let previous = self.realized.get(&key).map_or(0, |r| r.worked_turns);
                let worked_turns = if useless_hunt || previous == WORKED_DEAD_AT_ONCE {
                    WORKED_DEAD_AT_ONCE
                } else {
                    previous + 1
                };
                let per_worker = row.actual_yield / row.workers as f32;
                self.realized.insert(
                    key,
                    Realized {
                        per_worker,
                        worked_turns,
                    },
                );
                let (sum, count) = self
                    .realized_by_kind
                    .entry(row.kind.clone())
                    .or_insert((0.0, 0));
                *sum += per_worker;
                *count += 1;
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
        self.move_targets.retain(|band_id, target| {
            view.band(*band_id)
                .is_some_and(|cohort| band_tile(cohort) != *target)
        });
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

    /// Record this turn's accepted intents and learn the move targets their commands carry.
    pub fn record_choices<'a>(
        &mut self,
        tick: u64,
        intents: BTreeSet<String>,
        commands: impl Iterator<Item = &'a CommandPayload>,
    ) {
        for command in commands {
            if let CommandPayload::MoveBand {
                band_id: Some(band_id),
                target_x,
                target_y,
                ..
            } = command
            {
                self.move_targets
                    .insert(*band_id, Tile::new(*target_x, *target_y));
            }
        }
        self.chosen = Some((tick, intents));
    }

    /// What the row under `key` has realized, if it has ever been worked.
    pub fn realized(&self, key: &str) -> Option<Realized> {
        self.realized.get(key).copied()
    }

    /// The mean per-worker take this seat has realized across every row of `kind`, if it has
    /// worked any — the prior for a source of that web it has not tried.
    pub fn realized_for_kind(&self, kind: &str) -> Option<f32> {
        self.realized_by_kind
            .get(kind)
            .filter(|(_, count)| *count > 0)
            .map(|(sum, count)| sum / *count as f32)
    }

    pub fn move_target(&self, band_id: u64) -> Option<Tile> {
        self.move_targets.get(&band_id).copied()
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
        let mut memory = SeatMemory::new(HORIZON);
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
        let mut forever = SeatMemory::new(NO_MEMORY_DECAY);
        forever.observe(&a_view_at(10), 1);
        assert!(forever.is_known(active, 10_000));
    }

    #[test]
    fn a_full_frame_forgets_what_was_stamped_after_it() {
        let mut memory = SeatMemory::new(NO_MEMORY_DECAY);
        memory.observe(&a_view_at(10), 1);
        memory.record_choices(
            10,
            BTreeSet::from(["food:assign:1".to_owned()]),
            [CommandPayload::MoveBand {
                faction_id: 1,
                band_id: Some(7),
                target_x: 2,
                target_y: 2,
            }]
            .iter(),
        );
        memory.push_alarm(Alarm {
            specialist: SPECIALIST_FOOD,
            kind: AlarmKind::FoodShort,
            since_tick: 10,
        });
        assert!(memory.chosen_last_turn("food:assign:1"));
        assert_eq!(memory.move_target(7), Some(Tile::new(2, 2)));
        memory.forget_after(9);
        assert!(!memory.is_known(Tile::new(1, 0), 9));
        assert!(!memory.chosen_last_turn("food:assign:1"));
        assert_eq!(memory.move_target(7), None);
        assert!(memory.take_alarms().is_empty());
    }

    #[test]
    fn a_runway_is_falling_only_against_a_remembered_one_and_a_target_clears_on_arrival() {
        const FACTION: u32 = 1;
        const BAND: u64 = 7;
        let mut memory = SeatMemory::new(NO_MEMORY_DECAY);
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
            BTreeSet::new(),
            [CommandPayload::MoveBand {
                faction_id: FACTION,
                band_id: Some(BAND),
                target_x: 2,
                target_y: 2,
            }]
            .iter(),
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
        let mut memory = SeatMemory::new(NO_MEMORY_DECAY);
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
                per_worker: 0.2,
                worked_turns: 2
            })
        );
        assert_eq!(
            memory.realized(&herd).map(|r| r.worked_turns),
            Some(WORKED_DEAD_AT_ONCE)
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
}
