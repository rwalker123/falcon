use std::collections::{HashMap, HashSet};
use std::fmt;

use bevy::prelude::Resource;

/// Identifier for a faction participating in the turn loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FactionId(pub u32);

impl fmt::Display for FactionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Registry of factions recognised by the simulation server.
#[derive(Resource, Debug, Clone)]
pub struct FactionRegistry {
    pub factions: Vec<FactionId>,
}

impl Default for FactionRegistry {
    fn default() -> Self {
        Self {
            factions: vec![FactionId(0)],
        }
    }
}

impl FactionRegistry {
    pub fn new(factions: Vec<FactionId>) -> Self {
        Self { factions }
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
