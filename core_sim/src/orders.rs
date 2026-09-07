use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use bevy::prelude::Resource;

use crate::start_profile::{default_factions, FactionControl, FactionSpec};

/// Identifier for a faction participating in the turn loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FactionId(pub u32);

impl fmt::Display for FactionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// **Registry of factions recognised by the simulation server**, seeded from the active start
/// profile's `factions` list (see [`crate::start_profile::FactionSpec`]).
///
/// # ⛔ INVARIANT: `control` is keyed by exactly the ids in `factions`
///
/// Every reader that walks `factions` and then asks how a faction is driven would otherwise have to
/// handle "registered but uncontrolled", a state with no meaning. [`FactionRegistry::new`] is the
/// only constructor that can produce a non-default registry, and it derives **both** fields from one
/// list, so the two cannot be written apart. Ids are **positional** — `FactionId(i)` for index `i` —
/// which is why a profile cannot mint a duplicate id or leave a gap.
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

/// One human faction — the shipped world, and what a test harness or any other non-profile
/// construction path gets. Shares [`crate::start_profile::default_factions`] with the profile
/// layer's default so the two statements of "the default world" cannot drift.
impl Default for FactionRegistry {
    fn default() -> Self {
        Self::new(&default_factions())
    }
}

impl FactionRegistry {
    /// Derives ids and control from one declaration order: entry `i` becomes `FactionId(i)`.
    pub fn new(factions: &[FactionSpec]) -> Self {
        let control: BTreeMap<FactionId, FactionControl> = factions
            .iter()
            .enumerate()
            .map(|(index, spec)| (FactionId(index as u32), spec.control))
            .collect();
        let factions: Vec<FactionId> = (0..factions.len())
            .map(|index| FactionId(index as u32))
            .collect();
        debug_assert!(
            factions.len() == control.len() && factions.iter().all(|id| control.contains_key(id)),
            "faction registry control map must be keyed by exactly the registered factions"
        );
        Self { factions, control }
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

    fn spec(control: FactionControl) -> FactionSpec {
        FactionSpec { control }
    }

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

    /// **Ids are positional**: entry `i` of the declaration is `FactionId(i)`, and the control map
    /// is keyed by exactly those ids.
    #[test]
    fn a_declared_roster_seeds_positional_ids_and_their_control() {
        let registry =
            FactionRegistry::new(&[spec(FactionControl::Human), spec(FactionControl::Ai)]);
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
    }

    /// An id nobody declared is not registered, is not the sim's to drive, and has no control.
    #[test]
    fn an_unregistered_faction_has_no_control_and_is_not_ai() {
        let registry = FactionRegistry::new(&[spec(FactionControl::Human)]);
        assert!(registry.contains(FactionId(0)));
        assert!(!registry.contains(FactionId(7)));
        assert_eq!(registry.control_of(FactionId(7)), None);
        assert!(!registry.is_ai(FactionId(7)));
    }

    /// The turn queue is built from the registry's ids, so a seeded second faction is awaited
    /// without anything else being told about it.
    #[test]
    fn the_turn_queue_awaits_every_seeded_faction() {
        let registry =
            FactionRegistry::new(&[spec(FactionControl::Human), spec(FactionControl::Ai)]);
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
