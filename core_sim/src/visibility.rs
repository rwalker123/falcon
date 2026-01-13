//! Fog of War visibility system.
//!
//! Tracks per-faction visibility state for each tile with three states:
//! - Unexplored (0): Never seen
//! - Discovered (1): Previously seen but not currently visible
//! - Active (2): Currently visible
//!
//! Visibility decays over time: tiles that haven't been seen for a configurable
//! number of turns revert from Discovered back to Unexplored.

use std::collections::HashMap;

use bevy::math::UVec2;
use bevy::prelude::*;

use crate::orders::FactionId;

/// Visibility state for a single tile from a faction's perspective.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VisibilityState {
    #[default]
    Unexplored = 0,
    Discovered = 1,
    Active = 2,
}

impl VisibilityState {
    /// Convert to u8 for serialization.
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Convert from u8, defaulting to Unexplored for invalid values.
    pub fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Discovered,
            2 => Self::Active,
            _ => Self::Unexplored,
        }
    }
}

/// Per-tile visibility metadata for a single faction.
#[derive(Debug, Clone, Default)]
pub struct TileVisibility {
    pub state: VisibilityState,
    pub last_seen_turn: u64,
}

/// Per-faction visibility map tracking all tiles.
#[derive(Debug, Clone)]
pub struct FactionVisibilityMap {
    pub faction: FactionId,
    pub width: u32,
    pub height: u32,
    tiles: Vec<TileVisibility>,
}

impl FactionVisibilityMap {
    /// Create a new visibility map with all tiles unexplored.
    pub fn new(faction: FactionId, width: u32, height: u32) -> Self {
        let total = (width * height) as usize;
        Self {
            faction,
            width,
            height,
            tiles: vec![TileVisibility::default(); total],
        }
    }

    /// Get the tile index for (x, y) coordinates.
    #[inline]
    fn index(&self, x: u32, y: u32) -> Option<usize> {
        if x < self.width && y < self.height {
            Some((y * self.width + x) as usize)
        } else {
            None
        }
    }

    /// Get visibility state for a tile.
    #[inline]
    pub fn get(&self, x: u32, y: u32) -> Option<&TileVisibility> {
        self.index(x, y).and_then(|idx| self.tiles.get(idx))
    }

    /// Get mutable visibility state for a tile.
    #[inline]
    pub fn get_mut(&mut self, x: u32, y: u32) -> Option<&mut TileVisibility> {
        self.index(x, y).and_then(|idx| self.tiles.get_mut(idx))
    }

    /// Mark a tile as actively visible.
    pub fn mark_active(&mut self, x: u32, y: u32, current_turn: u64) {
        if let Some(tile) = self.get_mut(x, y) {
            tile.state = VisibilityState::Active;
            tile.last_seen_turn = current_turn;
        }
    }

    /// Mark a tile as discovered (was visible but no longer is).
    pub fn mark_discovered(&mut self, x: u32, y: u32) {
        if let Some(tile) = self.get_mut(x, y) {
            if tile.state == VisibilityState::Active {
                tile.state = VisibilityState::Discovered;
            }
        }
    }

    /// Mark a tile as unexplored (for decay).
    pub fn mark_unexplored(&mut self, x: u32, y: u32) {
        if let Some(tile) = self.get_mut(x, y) {
            tile.state = VisibilityState::Unexplored;
            tile.last_seen_turn = 0;
        }
    }

    /// Iterate over all tiles with their coordinates.
    pub fn iter_tiles(&self) -> impl Iterator<Item = (UVec2, &TileVisibility)> {
        self.tiles.iter().enumerate().map(move |(idx, tile)| {
            let x = (idx as u32) % self.width;
            let y = (idx as u32) / self.width;
            (UVec2::new(x, y), tile)
        })
    }

    /// Iterate over all tiles mutably with their coordinates.
    pub fn iter_tiles_mut(&mut self) -> impl Iterator<Item = (UVec2, &mut TileVisibility)> {
        let width = self.width;
        self.tiles.iter_mut().enumerate().map(move |(idx, tile)| {
            let x = (idx as u32) % width;
            let y = (idx as u32) / width;
            (UVec2::new(x, y), tile)
        })
    }

    /// Get visibility state as u8 for a tile.
    pub fn state_at(&self, x: u32, y: u32) -> u8 {
        self.get(x, y).map(|t| t.state.as_u8()).unwrap_or(0)
    }

    /// Export visibility states as a flat byte array (row-major).
    pub fn to_byte_raster(&self) -> Vec<u8> {
        self.tiles.iter().map(|t| t.state.as_u8()).collect()
    }

    /// Count tiles by visibility state.
    pub fn count_by_state(&self) -> (usize, usize, usize) {
        let mut unexplored = 0;
        let mut discovered = 0;
        let mut active = 0;
        for tile in &self.tiles {
            match tile.state {
                VisibilityState::Unexplored => unexplored += 1,
                VisibilityState::Discovered => discovered += 1,
                VisibilityState::Active => active += 1,
            }
        }
        (unexplored, discovered, active)
    }
}

/// Global visibility state resource tracking all factions.
#[derive(Resource, Debug, Clone, Default)]
pub struct VisibilityLedger {
    faction_maps: HashMap<FactionId, FactionVisibilityMap>,
}

/// The faction whose visibility is exported in snapshots.
/// In single-player, this is the player's faction. In multiplayer,
/// each client would have a different viewer faction.
#[derive(Resource, Debug, Clone, Copy)]
pub struct ViewerFaction(pub FactionId);

impl Default for ViewerFaction {
    fn default() -> Self {
        Self(FactionId(0))
    }
}

impl VisibilityLedger {
    /// Ensure a faction has a visibility map, creating one if needed.
    pub fn ensure_faction(
        &mut self,
        faction: FactionId,
        width: u32,
        height: u32,
    ) -> &mut FactionVisibilityMap {
        self.faction_maps
            .entry(faction)
            .or_insert_with(|| FactionVisibilityMap::new(faction, width, height))
    }

    /// Get a faction's visibility map.
    pub fn get_faction(&self, faction: FactionId) -> Option<&FactionVisibilityMap> {
        self.faction_maps.get(&faction)
    }

    /// Get a mutable reference to a faction's visibility map.
    pub fn get_faction_mut(&mut self, faction: FactionId) -> Option<&mut FactionVisibilityMap> {
        self.faction_maps.get_mut(&faction)
    }

    /// Iterate over all faction IDs.
    pub fn factions(&self) -> impl Iterator<Item = FactionId> + '_ {
        self.faction_maps.keys().copied()
    }

    /// Check if a tile is visible to a faction.
    pub fn is_visible(&self, faction: FactionId, x: u32, y: u32) -> bool {
        self.get_faction(faction)
            .and_then(|map| map.get(x, y))
            .map(|t| t.state == VisibilityState::Active)
            .unwrap_or(false)
    }

    /// Check if a tile has been discovered by a faction.
    pub fn is_discovered(&self, faction: FactionId, x: u32, y: u32) -> bool {
        self.get_faction(faction)
            .and_then(|map| map.get(x, y))
            .map(|t| t.state != VisibilityState::Unexplored)
            .unwrap_or(false)
    }

    /// Get visibility state for a tile.
    pub fn visibility_state(&self, faction: FactionId, x: u32, y: u32) -> VisibilityState {
        self.get_faction(faction)
            .and_then(|map| map.get(x, y))
            .map(|t| t.state)
            .unwrap_or(VisibilityState::Unexplored)
    }
}

/// Marker component for entities that provide visibility.
#[derive(Component, Debug, Clone)]
pub struct VisibilitySource {
    pub faction: FactionId,
    pub base_sight_range: u32,
    pub elevation_bonus_factor: f32,
}

impl VisibilitySource {
    pub fn new(faction: FactionId, base_range: u32) -> Self {
        Self {
            faction,
            base_sight_range: base_range,
            elevation_bonus_factor: 1.0,
        }
    }

    pub fn with_elevation_bonus(mut self, factor: f32) -> Self {
        self.elevation_bonus_factor = factor;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_state_conversion() {
        assert_eq!(VisibilityState::Unexplored.as_u8(), 0);
        assert_eq!(VisibilityState::Discovered.as_u8(), 1);
        assert_eq!(VisibilityState::Active.as_u8(), 2);

        assert_eq!(VisibilityState::from_u8(0), VisibilityState::Unexplored);
        assert_eq!(VisibilityState::from_u8(1), VisibilityState::Discovered);
        assert_eq!(VisibilityState::from_u8(2), VisibilityState::Active);
        assert_eq!(VisibilityState::from_u8(255), VisibilityState::Unexplored);
    }

    #[test]
    fn faction_visibility_map_basics() {
        let faction = FactionId(0);
        let mut map = FactionVisibilityMap::new(faction, 10, 10);

        // Initially all unexplored
        let (unexplored, discovered, active) = map.count_by_state();
        assert_eq!(unexplored, 100);
        assert_eq!(discovered, 0);
        assert_eq!(active, 0);

        // Mark a tile active
        map.mark_active(5, 5, 1);
        assert_eq!(map.get(5, 5).unwrap().state, VisibilityState::Active);
        assert_eq!(map.get(5, 5).unwrap().last_seen_turn, 1);

        // Mark it discovered
        map.mark_discovered(5, 5);
        assert_eq!(map.get(5, 5).unwrap().state, VisibilityState::Discovered);

        // Mark it unexplored
        map.mark_unexplored(5, 5);
        assert_eq!(map.get(5, 5).unwrap().state, VisibilityState::Unexplored);
    }

    #[test]
    fn visibility_ledger_multi_faction() {
        let mut ledger = VisibilityLedger::default();

        let faction_a = FactionId(0);
        let faction_b = FactionId(1);

        ledger
            .ensure_faction(faction_a, 10, 10)
            .mark_active(0, 0, 1);
        ledger
            .ensure_faction(faction_b, 10, 10)
            .mark_active(9, 9, 1);

        assert!(ledger.is_visible(faction_a, 0, 0));
        assert!(!ledger.is_visible(faction_a, 9, 9));
        assert!(!ledger.is_visible(faction_b, 0, 0));
        assert!(ledger.is_visible(faction_b, 9, 9));
    }

    #[test]
    fn byte_raster_export() {
        let faction = FactionId(0);
        let mut map = FactionVisibilityMap::new(faction, 3, 3);

        map.mark_active(0, 0, 1);
        map.mark_active(1, 1, 1);
        map.mark_discovered(1, 1);

        let raster = map.to_byte_raster();
        assert_eq!(raster.len(), 9);
        assert_eq!(raster[0], 2); // (0,0) active
        assert_eq!(raster[4], 1); // (1,1) discovered
        assert_eq!(raster[8], 0); // (2,2) unexplored
    }
}
