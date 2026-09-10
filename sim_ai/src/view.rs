//! **Perception** — the frame is the view (`docs/plan_ai_driver.md` §2).
//!
//! [`SeatView`] is the decoded `WorldSnapshot` this seat was sent, kept current by applying each
//! `WorldDelta` as it arrives. There is no second representation of "what I can see": fog is a
//! property of the bytes, and a tile the seat has not seen is simply not in them.
//!
//! A delta that does not name the frame the view holds (`ApplyDeltaError`) means the chain broke —
//! a dropped frame, a world rebuild — and the only honest answer is to ask for a full frame and
//! ignore deltas until it arrives. The event feed is append-only, so applying anything else would
//! lose history silently.

use sim_runtime::{
    decode_frame_flatbuffer, ApplyDeltaError, DecodeError, FramePayload, WorldSnapshot,
};
use tracing::{info, warn};

/// What this seat holds, and the last tick it acted on.
pub struct SeatView {
    pub snapshot: WorldSnapshot,
    /// The tick the brain last decided on. A mid-turn recapture re-sends the same tick and must
    /// not be acted on twice.
    pub last_acted_tick: Option<u64>,
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
                let last_acted_tick = self.view.as_ref().and_then(|view| view.last_acted_tick);
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
    use sim_runtime::{encode_delta_flatbuffer, encode_snapshot_flatbuffer, WorldDelta};

    const FIRST_FRAME: u64 = 3;

    fn a_full_frame() -> Vec<u8> {
        let mut snapshot = WorldSnapshot::default();
        snapshot.header.frame_seq = FIRST_FRAME;
        encode_snapshot_flatbuffer(&snapshot)
    }

    fn a_delta_on(base: u64) -> Vec<u8> {
        let mut delta = WorldDelta::default();
        delta.header.base_frame_seq = base;
        delta.header.frame_seq = base + 1;
        encode_delta_flatbuffer(&delta)
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
}
