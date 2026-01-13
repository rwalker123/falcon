# Plan: Fix Trade Route Data Model

## Status

- [x] Step 1: Remove TradeLink from world spawn (DONE)
- [x] Step 4: Re-enable visibility system (DONE)
- [ ] Step 2: Update systems that query TradeLink (not needed yet - they handle empty results)
- [ ] Step 3: Add TradeLink when trade is established (future work)

## Summary

Trade routes should only exist when established between settlements. ~~Currently, `TradeLink` components are incorrectly spawned on ALL `LogisticsLink` entities at world generation, even though no trade exists at game start.~~ **Fixed: TradeLinks are no longer spawned at world generation.**

## The Bug

**File**: `core_sim/src/systems.rs` lines 520-561

Every `LogisticsLink` gets a `TradeLink` attached at world spawn:

```rust
commands
    .spawn(LogisticsLink { ... })
    .insert(TradeLink { ... });  // BUG: Why attach trade to every link?
```

This creates ~8000 TradeLink entities for an 80x52 grid, all with garbage/default data.

## Correct Architecture

| Component | Purpose | When Created |
|-----------|---------|--------------|
| `LogisticsLink` | Infrastructure connecting adjacent tiles | World generation (correct) |
| `TradeLink` | Trade metadata for an active route | When trade is established (NOT at world spawn) |

## Fix

### Step 1: Remove TradeLink from World Spawn

**File**: `core_sim/src/systems.rs` lines 520-561

Change from:
```rust
commands
    .spawn(LogisticsLink {
        from: from_entity,
        to: to_entity,
        capacity: config.base_link_capacity,
        flow: scalar_zero(),
    })
    .insert(TradeLink { ... });  // REMOVE THIS
```

To:
```rust
commands.spawn(LogisticsLink {
    from: from_entity,
    to: to_entity,
    capacity: config.base_link_capacity,
    flow: scalar_zero(),
});
// No TradeLink - will be added when trade is established
```

### Step 2: Update Systems That Query TradeLink

Any system that queries `(LogisticsLink, TradeLink)` needs to handle links without TradeLink.

**File**: `core_sim/src/systems.rs`

`trade_knowledge_diffusion()` (line 2854) - Change query:
```rust
// Before: Query<(&LogisticsLink, &mut TradeLink)>
// After: Query<(&LogisticsLink, Option<&mut TradeLink>)>
// Or: Only query links that have TradeLink
```

### Step 3: Add TradeLink When Trade Is Established

When two settlements establish trade, compute the path and add `TradeLink` to links along that path.

This requires:
1. A pathfinding algorithm to find LogisticsLink chain between settlements
2. A command/event to establish trade: `EstablishTradeRoute { from: Entity, to: Entity }`
3. System that handles the command and attaches TradeLink components

```rust
fn establish_trade_route(
    mut commands: Commands,
    // ... pathfinding resources ...
) {
    // 1. Find path of LogisticsLink entities between settlements
    // 2. For each link in path:
    commands.entity(link_entity).insert(TradeLink {
        from_faction: source_settlement.faction,
        to_faction: dest_settlement.faction,
        throughput: scalar_zero(),  // Will be calculated by trade system
        // ... other fields ...
    });
}
```

### Step 4: Enable Visibility System

**File**: `core_sim/src/visibility_systems.rs`

Once TradeLink only exists on actual trade routes, the visibility system works as designed:

```rust
pub fn apply_trade_route_visibility(
    mut ledger: ResMut<VisibilityLedger>,
    tick: Res<SimulationTick>,
    trade_links: Query<(&LogisticsLink, &TradeLink)>,  // Now only matches real routes
    tiles: Query<&Tile>,
) {
    for (logistics, trade) in trade_links.iter() {
        // Mark tiles visible - all links here are actual trade routes
        // ...
    }
}
```

## Files to Modify

| File | Change |
|------|--------|
| `core_sim/src/systems.rs:520-561` | Remove `.insert(TradeLink {...})` from LogisticsLink spawn |
| `core_sim/src/systems.rs:2854` | Update `trade_knowledge_diffusion` to handle missing TradeLink |
| `core_sim/src/systems.rs` | Add `establish_trade_route` system (new) |
| `core_sim/src/visibility_systems.rs:440` | Enable the system (already written correctly) |

## Dependencies / Questions

1. **How is trade established?** Need to understand the game design for establishing trade routes. Is it:
   - Automatic when two settlements exist?
   - Player command?
   - Based on some threshold?

2. **Pathfinding**: Need algorithm to find LogisticsLink path between two tiles. May already exist in logistics system.

3. **Trade removal**: When trade ends, remove TradeLink components from the path.

## Testing

1. Start game - no TradeLink entities should exist
2. Found settlements - still no TradeLinks
3. Establish trade between settlements - TradeLinks appear on path
4. FoW shows trade route visible
5. End trade - TradeLinks removed, visibility decays
