# Trade Network Integration with Fog of War

## Status: READY - Waiting for Trade Route Establishment

**The visibility system is enabled and working.** TradeLinks are no longer spawned on all LogisticsLinks at world generation. When trade routes are established between settlements, TradeLink components will be added to the relevant LogisticsLinks, and the visibility system will automatically make those tiles visible.

### What's Done

1. Removed TradeLink from LogisticsLink world spawn (`systems.rs:520-537`)
2. Re-enabled `apply_trade_route_visibility` system
3. System correctly handles zero trade routes (does nothing)

### Remaining Work

When trade route establishment is implemented, it needs to:
1. Find the path of LogisticsLinks between settlements
2. Attach TradeLink components to those links with correct `from_faction`/`to_faction`

See `docs/plan_trade_route_data_model.md` for implementation details.

---

## Overview

Integrate trade networks into the Fog of War system so that active trade routes provide visibility along their entire path. If a faction has a trade route, the source tile, destination tile, and all intermediate tiles along the path should be marked as **Active** (fully visible).

## Current State

### Fog of War System (implemented in `feat/fow` branch)
- `VisibilityLedger` tracks per-faction visibility maps
- Three states: `Unexplored` (0), `Discovered` (1), `Active` (2)
- Visibility calculated from units/settlements with range and line-of-sight
- Runs in `TurnStage::Visibility` after Population, before Crisis
- Files: `core_sim/src/visibility.rs`, `visibility_systems.rs`, `visibility_config.rs`

### Trade Network System
- `TradeLink` components attached to `LogisticsLink` entities
- Each link connects two **adjacent tiles** (grid 4-neighbor topology)
- Links store: `from_faction`, `to_faction`, `throughput`, `tariff`, `openness`
- Tile references: `from_tile` and `to_tile` entity IDs
- **No explicit path storage** - routes are implicit chains of adjacent logistics links
- Files: `core_sim/src/components.rs` (TradeLink), `core_sim/src/systems.rs` (trade_knowledge_diffusion)

## Design

### Approach: Mark Trade Route Tiles as Active

When calculating visibility, after processing units/settlements:
1. Query all `TradeLink` components owned by the faction
2. For each trade link, mark both endpoint tiles as `Active`
3. Since links connect adjacent tiles, the full route is covered by marking all link endpoints

### Data Flow

```
TurnStage::Visibility:
  1. clear_active_visibility()     - Reset Active → Discovered
  2. calculate_visibility()        - Mark tiles visible from units/settlements
  3. apply_trade_route_visibility() - NEW: Mark trade route tiles as Active
  4. apply_visibility_decay()      - Decay old Discovered → Unexplored
```

### Key Insight

Trade routes are chains of `LogisticsLink` entities with `TradeLink` components. Each link connects two adjacent tiles. To mark the entire path:
- Iterate all `(LogisticsLink, TradeLink)` pairs
- Filter to links where `from_faction` OR `to_faction` matches the player faction
- For each matching link, mark both `from_tile` and `to_tile` as Active

This automatically covers the entire path because:
- Route A→B→C→D consists of links: (A,B), (B,C), (C,D)
- Marking endpoints of each: A, B, B, C, C, D
- Result: All tiles A, B, C, D are Active

## Implementation

### File: `core_sim/src/visibility_systems.rs`

Add new system after `calculate_visibility`:

```rust
/// Mark tiles along trade routes as Active for visibility.
/// Trade routes provide visibility because merchants travel the path.
pub fn apply_trade_route_visibility(
    mut ledger: ResMut<VisibilityLedger>,
    tick: Res<Tick>,
    grid: Res<GridSize>,
    trade_links: Query<(&LogisticsLink, &TradeLink)>,
    tiles: Query<&TilePosition>,
) {
    let current_turn = tick.turn;

    for (logistics, trade) in trade_links.iter() {
        // Get tile positions from logistics link endpoints
        let from_pos = tiles.get(logistics.from_entity).ok();
        let to_pos = tiles.get(logistics.to_entity).ok();

        // Mark tiles visible for the "from" faction
        if let Some(map) = ledger.get_faction_mut(trade.from_faction) {
            if let Some(pos) = from_pos {
                map.mark_active(pos.x, pos.y, current_turn);
            }
            if let Some(pos) = to_pos {
                map.mark_active(pos.x, pos.y, current_turn);
            }
        }

        // Mark tiles visible for the "to" faction (they see the route too)
        if let Some(map) = ledger.get_faction_mut(trade.to_faction) {
            if let Some(pos) = from_pos {
                map.mark_active(pos.x, pos.y, current_turn);
            }
            if let Some(pos) = to_pos {
                map.mark_active(pos.x, pos.y, current_turn);
            }
        }
    }
}
```

### File: `core_sim/src/lib.rs`

Register the new system in the turn pipeline:

```rust
// In configure_systems() or equivalent:
app.add_systems(
    Update,
    (
        clear_active_visibility,
        calculate_visibility,
        apply_trade_route_visibility,  // NEW
        apply_visibility_decay,
    )
        .chain()
        .in_set(TurnStage::Visibility),
);
```

## Configuration (Optional)

Add to `visibility_config.json` if configurable behavior is desired:

```json
{
  "trade_route_visibility": {
    "enabled": true,
    "requires_active_throughput": false,  // If true, only routes with throughput > 0
    "visibility_state": "active"          // Could be "discovered" for lesser visibility
  }
}
```

## Files to Modify

| File | Changes |
|------|---------|
| `core_sim/src/visibility_systems.rs` | Add `apply_trade_route_visibility` system |
| `core_sim/src/lib.rs` | Register new system in `TurnStage::Visibility` |
| `core_sim/src/data/visibility_config.json` | (Optional) Add trade route config |

## Testing

### Unit Test

```rust
#[test]
fn trade_route_grants_visibility() {
    // Setup: Create two factions with a trade route between tiles (5,5) and (5,6)
    // Verify: Both tiles are Active for both factions after visibility calculation
}
```

### Integration Test

1. Start game with two settlements
2. Establish trade route between them
3. Enable FoW (press F)
4. Verify: All tiles along trade route show terrain (not black)
5. Verify: Trade route path remains visible even if units move away

### Manual Verification

1. `cargo build -p core_sim`
2. `cargo test -p core_sim`
3. Run stack: `scripts/run_stack.sh`
4. Create trade route in game
5. Press F to toggle FoW
6. Confirm trade route tiles are visible

## Edge Cases

1. **Broken trade routes**: If a trade link is removed, the tiles should fall back to normal visibility rules (may become Discovered/Unexplored based on decay)

2. **One-way vs bidirectional**: Current design marks tiles for both `from_faction` and `to_faction`. This means both parties see the route.

3. **Throughput zero**: Routes with zero throughput still grant visibility (merchants still travel, just no goods). Can be changed via config if desired.

4. **Multiple routes**: If multiple routes share tiles, each route independently marks tiles as Active. No special handling needed.

## Future Enhancements

1. **Route highlighting on map**: Client could draw trade route paths as lines/overlays
2. **Visibility decay for inactive routes**: Routes with zero throughput for N turns could stop granting visibility
3. **Partial visibility**: Routes could grant "Discovered" instead of "Active" for balance
4. **Trade route fog pulse**: Opening a new route could trigger a one-time visibility reveal animation

---

## Implementation Notes (2026-01-13)

### Problem Discovered

When implementing `apply_trade_route_visibility`, we found that:

1. **ALL `LogisticsLink` entities have `TradeLink` components** - spawned at world generation
2. **`TradeLink.throughput` contains arbitrary values** - some positive, some negative, not initialized to zero
3. **`TradeLink.from_faction` and `to_faction` are both `FactionId(0)`** - matching the player faction

This means:
- Filtering by faction matches ALL links (8000+ for an 80x52 grid)
- Filtering by `throughput > 0` still matches ~20% of links with garbage positive values
- There's no way to distinguish "actual trade route" from "logistics infrastructure link"

### Root Cause

The `TradeLink` component is attached to every `LogisticsLink` as infrastructure for potential future trade. The `throughput` field is meant to track goods flow but isn't properly initialized or used yet. The current state represents "potential routes" not "active routes".

### Required Fix

To implement trade route visibility, we need one of:

1. **Marker component approach**:
   ```rust
   #[derive(Component)]
   pub struct ActiveTradeRoute {
       pub route_id: u32,
       pub source_settlement: Entity,
       pub dest_settlement: Entity,
   }
   ```
   Add this to LogisticsLink entities when a trade deal is established.

2. **Boolean flag approach**:
   ```rust
   pub struct TradeLink {
       // ... existing fields ...
       pub is_active: bool,  // NEW: true when actual trade flows
   }
   ```
   Set `is_active = true` when trade is established between settlements.

3. **Settlement-based query**:
   Instead of querying individual links, track trade agreements at settlement level:
   ```rust
   pub struct TradeAgreement {
       pub from_settlement: Entity,
       pub to_settlement: Entity,
       pub path: Vec<Entity>,  // LogisticsLink entities in the route
   }
   ```
   Then query agreements and mark their path tiles as visible.

The system stub is in place at `visibility_systems.rs:apply_trade_route_visibility()` ready to be implemented once the data model is fixed.
