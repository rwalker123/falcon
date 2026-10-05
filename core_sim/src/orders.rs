use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use bevy::prelude::Resource;

use crate::start_profile::FactionControl;

/// Identifier for a faction participating in the turn loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FactionId(pub u32);

impl fmt::Display for FactionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// **Registry of factions recognised by the simulation server**, seeded from the AI-faction count
/// the new game was asked for (see [`FactionRegistry::with_ai_factions`]).
///
/// # ⛔ INVARIANT: `control` is keyed by exactly the ids in `factions`
///
/// Every reader that walks `factions` and then asks how a faction is driven would otherwise have to
/// handle "registered but uncontrolled", a state with no meaning. [`FactionRegistry::with_ai_factions`]
/// is the only constructor there is, and it derives **both** fields from one count, so the two
/// cannot be written apart. Ids are **positional** — `FactionId(i)` for index `i` — which is why
/// nothing can mint a duplicate id or leave a gap.
///
/// **Both fields are private, and that is what enforces the invariant.** While they were `pub` the
/// `debug_assert!` in `new` guarded only the constructor, and three test worlds pushed a second id
/// into `factions` while leaving `control` at one entry — a roster whose second faction
/// [`Self::contains`] denied, which is exactly the command-dropping state the paragraph above calls
/// unrepresentable. Readers take [`Self::factions`] and the control accessors instead; serde reads
/// and writes private fields, so the save format does not care.
#[derive(Resource, Debug, Clone, Serialize, Deserialize)]
pub struct FactionRegistry {
    factions: Vec<FactionId>,
    control: BTreeMap<FactionId, FactionControl>,
}

/// One human faction and no rivals — the world a test harness, a save-less build or any other
/// path that never asked for AI factions gets. It is exactly `with_ai_factions(0)`, so "the default
/// world" has one statement rather than two.
impl Default for FactionRegistry {
    fn default() -> Self {
        Self::with_ai_factions(0)
    }
}

impl FactionRegistry {
    /// **The roster a new game asks for: the player, then `ai_factions` rivals.**
    ///
    /// `FactionId(0)` is always the human the player commands, and ids `1..=ai_factions` are the
    /// sim's. The count is *rivals*, not roster size, so the caller never has to remember an
    /// off-by-one: pick 2 and the world has three peoples in it.
    ///
    /// The shape is structural rather than validated — there is always exactly one human and there
    /// is always a faction 0 for worldgen to place — which is what retired the "non-empty" and "at
    /// least one human" checks the start profile's authored roster needed.
    pub fn with_ai_factions(ai_factions: u32) -> Self {
        let control: BTreeMap<FactionId, FactionControl> = (0..=ai_factions)
            .map(|index| {
                // Id 0 is the player's own; every id after it is the sim's.
                let control = if index == 0 {
                    FactionControl::Human
                } else {
                    FactionControl::Ai
                };
                (FactionId(index), control)
            })
            .collect();
        let factions: Vec<FactionId> = (0..=ai_factions).map(FactionId).collect();
        debug_assert!(
            factions.len() == control.len() && factions.iter().all(|id| control.contains_key(id)),
            "faction registry control map must be keyed by exactly the registered factions"
        );
        Self { factions, control }
    }

    /// **Register one more people, driven by the AI, at the next positional id** — the runtime half
    /// of the roster (`docs/plan_band_fission.md` §Independence: a cut-off, aggrieved group of bands
    /// becomes its own people mid-game).
    ///
    /// Ids stay positional because the new id is `factions.len()`, so this can neither duplicate an
    /// id nor leave a gap, and both fields are written from that one id — the invariant
    /// [`Self::with_ai_factions`] states holds after it as before. A people born at runtime is
    /// always the sim's: id 0 is the human by construction, so even a band that leaves the player
    /// is the AI's to play.
    ///
    /// The registry is only the first of the roster-derived resources; the runtime path that calls
    /// this extends the rest (`systems::independence::grow_faction_roster`).
    pub fn add_ai_faction(&mut self) -> FactionId {
        let faction = FactionId(self.factions.len() as u32);
        self.factions.push(faction);
        self.control.insert(faction, FactionControl::Ai);
        debug_assert!(
            self.factions.len() == self.control.len()
                && self.factions.iter().all(|id| self.control.contains_key(id))
                && self
                    .factions
                    .iter()
                    .enumerate()
                    .all(|(index, id)| id.0 as usize == index),
            "faction registry control map must be keyed by exactly the registered, positional factions"
        );
        faction
    }

    /// How many of the registered factions the sim drives — the count
    /// [`Self::with_ai_factions`] was built from, read back.
    pub fn ai_faction_count(&self) -> u32 {
        self.factions.len().saturating_sub(1) as u32
    }

    /// Every registered faction, in id order — the roster the turn queue awaits and every fan-out
    /// (espionage seeding, migration destinations, the save's world statics) walks.
    pub fn factions(&self) -> &[FactionId] {
        &self.factions
    }

    /// How `faction` is driven, or `None` if it is not registered at all.
    pub fn control_of(&self, faction: FactionId) -> Option<FactionControl> {
        self.control.get(&faction).copied()
    }

    /// Whether the sim drives `faction`. An unregistered faction is not the sim's to drive, so this
    /// is `false` rather than a panic.
    pub fn is_ai(&self, faction: FactionId) -> bool {
        self.control_of(faction) == Some(FactionControl::Ai)
    }

    /// Whether `faction` is one this world recognises.
    pub fn contains(&self, faction: FactionId) -> bool {
        self.control.contains_key(&faction)
    }
}

/// Individual orders submitted by a faction. Currently a placeholder for future expansion.
#[derive(Debug, Clone)]
pub enum Order {
    EndTurn,
}

/// Collection of orders submitted by a faction for the upcoming turn.
#[derive(Debug, Clone)]
pub struct FactionOrders {
    pub orders: Vec<Order>,
    pub note: Option<String>,
}

impl FactionOrders {
    pub fn end_turn() -> Self {
        Self {
            orders: vec![Order::EndTurn],
            note: None,
        }
    }
}

/// Result of attempting to submit orders for a faction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitOutcome {
    Accepted { remaining: usize },
    ReadyToResolve,
}

/// Error that can occur when submitting orders.
#[derive(Debug, thiserror::Error)]
pub enum SubmitError {
    #[error("faction {0} is not registered")]
    UnknownFaction(FactionId),
    #[error("orders for faction {0} already submitted")]
    DuplicateSubmission(FactionId),
}

/// Tracks turn collection and resolution state.
#[derive(Resource, Debug, Clone)]
pub struct TurnQueue {
    factions: Vec<FactionId>,
    awaiting: HashSet<FactionId>,
    submissions: HashMap<FactionId, FactionOrders>,
    current_turn: u64,
}

impl TurnQueue {
    pub fn new(factions: Vec<FactionId>) -> Self {
        let awaiting: HashSet<_> = factions.iter().copied().collect();
        Self {
            factions,
            awaiting,
            submissions: HashMap::new(),
            current_turn: 0,
        }
    }

    /// **Await `faction` from the NEXT turn on** — the runtime roster's queue half.
    ///
    /// Only the roster grows: the turn in flight was already collected and drained, so `awaiting`
    /// and `submissions` are left alone and [`Self::advance_turn`] picks the new id up when it
    /// re-arms the await set. Adding it to `awaiting` mid-turn would stall a turn nobody could
    /// submit for. Idempotent — a faction already on the roster is not added twice.
    pub fn add_faction(&mut self, faction: FactionId) {
        if !self.factions.contains(&faction) {
            self.factions.push(faction);
        }
    }

    /// Every faction this queue awaits each turn, in roster order.
    pub fn factions(&self) -> &[FactionId] {
        &self.factions
    }

    pub fn current_turn(&self) -> u64 {
        self.current_turn
    }

    pub fn awaiting(&self) -> Vec<FactionId> {
        self.awaiting.iter().copied().collect()
    }

    pub fn submit_orders(
        &mut self,
        faction: FactionId,
        orders: FactionOrders,
    ) -> Result<SubmitOutcome, SubmitError> {
        if !self.factions.contains(&faction) {
            return Err(SubmitError::UnknownFaction(faction));
        }
        if self.submissions.contains_key(&faction) {
            return Err(SubmitError::DuplicateSubmission(faction));
        }
        self.submissions.insert(faction, orders);
        self.awaiting.remove(&faction);
        if self.awaiting.is_empty() {
            Ok(SubmitOutcome::ReadyToResolve)
        } else {
            Ok(SubmitOutcome::Accepted {
                remaining: self.awaiting.len(),
            })
        }
    }

    pub fn is_ready(&self) -> bool {
        self.awaiting.is_empty()
    }

    pub fn drain_ready_orders(&mut self) -> Vec<(FactionId, FactionOrders)> {
        debug_assert!(
            self.awaiting.is_empty(),
            "orders requested before all submissions"
        );
        let mut collected: Vec<_> = self.submissions.drain().collect();
        collected.sort_by_key(|(id, _)| *id);
        collected
    }

    pub fn advance_turn(&mut self) {
        self.current_turn = self.current_turn.wrapping_add(1);
        self.awaiting = self.factions.iter().copied().collect();
        self.submissions.clear();
    }

    pub fn force_submit_all<F>(&mut self, mut builder: F)
    where
        F: FnMut(FactionId) -> FactionOrders,
    {
        for faction in &self.factions {
            if !self.submissions.contains_key(faction) {
                let orders = builder(*faction);
                self.submissions.insert(*faction, orders);
                self.awaiting.remove(faction);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registry a test harness, a save-less world and any other non-profile path gets.
    #[test]
    fn the_default_registry_is_one_human_faction() {
        let registry = FactionRegistry::default();
        assert_eq!(registry.factions(), [FactionId(0)]);
        assert_eq!(
            registry.control_of(FactionId(0)),
            Some(FactionControl::Human)
        );
        assert!(!registry.is_ai(FactionId(0)));
    }

    /// **The picked count is AI factions, and the roster is one longer than it**: the human at id
    /// 0, then the rivals. Ids are positional and the control map is keyed by exactly those ids.
    #[test]
    fn a_requested_ai_count_seeds_one_human_and_that_many_rivals() {
        let registry = FactionRegistry::with_ai_factions(1);
        assert_eq!(registry.factions(), [FactionId(0), FactionId(1)]);
        assert_eq!(
            registry.control_of(FactionId(0)),
            Some(FactionControl::Human)
        );
        assert_eq!(registry.control_of(FactionId(1)), Some(FactionControl::Ai));
        assert!(!registry.is_ai(FactionId(0)));
        assert!(registry.is_ai(FactionId(1)));
        let control_keys: Vec<FactionId> = registry.control.keys().copied().collect();
        assert_eq!(control_keys, registry.factions());
        assert_eq!(registry.ai_faction_count(), 1);
    }

    /// Pick 2 and three peoples play the world — the off-by-one the count's name exists to keep out
    /// of every caller's head. Sabotaged by an `..ai_factions` range: it would answer two.
    #[test]
    fn two_ai_factions_make_a_three_faction_world() {
        let registry = FactionRegistry::with_ai_factions(2);
        assert_eq!(
            registry.factions(),
            [FactionId(0), FactionId(1), FactionId(2)]
        );
        assert_eq!(
            registry.control_of(FactionId(0)),
            Some(FactionControl::Human)
        );
        assert!(registry.is_ai(FactionId(1)));
        assert!(registry.is_ai(FactionId(2)));
        assert_eq!(registry.ai_faction_count(), 2);
    }

    /// Zero rivals is the single-faction world, and it is the same object `default()` builds — the
    /// property the whole "0 changes nothing" claim rests on.
    #[test]
    fn zero_ai_factions_is_the_default_world() {
        let picked = FactionRegistry::with_ai_factions(0);
        let default = FactionRegistry::default();
        assert_eq!(picked.factions(), default.factions());
        assert_eq!(
            picked.control_of(FactionId(0)),
            default.control_of(FactionId(0))
        );
        assert_eq!(picked.ai_faction_count(), 0);
    }

    /// An id nobody declared is not registered, is not the sim's to drive, and has no control.
    #[test]
    fn an_unregistered_faction_has_no_control_and_is_not_ai() {
        let registry = FactionRegistry::with_ai_factions(0);
        assert!(registry.contains(FactionId(0)));
        assert!(!registry.contains(FactionId(7)));
        assert_eq!(registry.control_of(FactionId(7)), None);
        assert!(!registry.is_ai(FactionId(7)));
    }

    /// **A runtime people is the next positional id, and it is the AI's.** The control map stays
    /// keyed by exactly the roster, so `contains` and `control_of` agree with `factions()`.
    #[test]
    fn adding_an_ai_faction_appends_the_next_id_under_ai_control() {
        let mut registry = FactionRegistry::with_ai_factions(1);
        let added = registry.add_ai_faction();
        assert_eq!(added, FactionId(2));
        assert_eq!(
            registry.factions(),
            [FactionId(0), FactionId(1), FactionId(2)]
        );
        assert!(registry.contains(added));
        assert!(registry.is_ai(added));
        let control_keys: Vec<FactionId> = registry.control.keys().copied().collect();
        assert_eq!(control_keys, registry.factions());
    }

    /// **A faction added mid-turn is awaited from the next turn, not the one in flight** — adding
    /// it to the in-flight await set would hold a turn nobody can submit for.
    #[test]
    fn a_faction_added_to_the_queue_is_awaited_from_the_next_turn() {
        let mut queue = TurnQueue::new(vec![FactionId(0)]);
        queue.add_faction(FactionId(1));
        assert_eq!(queue.awaiting(), vec![FactionId(0)]);
        queue.advance_turn();
        let mut awaiting = queue.awaiting();
        awaiting.sort();
        assert_eq!(awaiting, vec![FactionId(0), FactionId(1)]);
    }

    /// The turn queue is built from the registry's ids, so a seeded second faction is awaited
    /// without anything else being told about it.
    #[test]
    fn the_turn_queue_awaits_every_seeded_faction() {
        let registry = FactionRegistry::with_ai_factions(1);
        let mut queue = TurnQueue::new(registry.factions().to_vec());
        let mut awaiting = queue.awaiting();
        awaiting.sort();
        assert_eq!(awaiting, vec![FactionId(0), FactionId(1)]);

        let outcome = queue
            .submit_orders(FactionId(0), FactionOrders::end_turn())
            .expect("faction 0 is registered");
        assert_eq!(outcome, SubmitOutcome::Accepted { remaining: 1 });
        let outcome = queue
            .submit_orders(FactionId(1), FactionOrders::end_turn())
            .expect("faction 1 is registered");
        assert_eq!(outcome, SubmitOutcome::ReadyToResolve);
    }
}
