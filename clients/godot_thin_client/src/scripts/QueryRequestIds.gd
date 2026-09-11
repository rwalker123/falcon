extends RefCounted
class_name QueryRequestIds

## **THE ONE ALLOCATOR FOR QUERY REQUEST IDS**, shared by every seam that rides the native query
## worker (`bridge/query.rs`) — `SaveSlots` and `FactionCapacity` today, and whatever asks next.
##
## Every one of those seams is fed from the SAME drain (`CommandBridge.poll_query_replies`) and routes
## a reply by its `request_id`, so an id spent by one seam and read by another is a reply delivered to
## the wrong question. Two seams each keeping their own counter is exactly how that happens, which is
## why the counter lives here and nowhere else.
##
## Pure statics; never instantiated.

## **THE FLOOR THAT KEEPS THESE SEAMS CLEAR OF `ForecastQuery`.**
##
## `ForecastQuery` counts up from 1 in `Main`, and its replies arrive on the same drain. Ids are `u64`
## on the wire and every id handed out here sits at or above this floor, so a collision with a
## forecast would need four billion of them in one session — and there is no coordination to forget.
const REQUEST_ID_BASE := 1 << 40

## **ONE BLOCK OF IDS PER SEAM INSTANCE, so an id cannot be REUSED ACROSS A SCENE CHANGE.**
##
## The worker's answer channel is process-global (`QUERY_ANSWERS`, an `OnceLock` in `bridge/query.rs`)
## and outlives every scene; a seam does not. A load or a theme apply swaps the scene, the world that
## comes up builds NEW seams, and an ask still in flight across that swap is drained by the new scene
## and offered to it. Were every instance to restart at `REQUEST_ID_BASE`, that stale answer would
## carry an id the new seam had just spent — and a reply that says `ok: true` finishes whatever the
## new seam thought that id was (`.claude/rules/client/save-load-menu.md` has the load that was
## refused and reported as a success).
##
## Ids reserved to one seam. A seam spends a handful — one per pane open, one per verb — so the block
## cannot be walked out of, and the block index below stays far short of the `u64` the wire carries.
const IDS_PER_SESSION := 1 << 16

## **THE BLOCK INDEX IS THE MONOTONIC MICROSECOND CLOCK, plus a tie-break count.** The clock cannot
## repeat within a process and — unlike a `static var`, whose lifetime is the SCRIPT's rather than the
## process's — is not reset by a scene change. The counter separates two seams built inside the same
## microsecond, which the clock alone cannot, and it is what makes `SaveSlots` and `FactionCapacity`
## disjoint when a screen builds both in one `_ready`. Because both terms are non-decreasing and the
## counter rises by one on every call, successive block starts are strictly increasing whatever the
## clock does.
static var _blocks_reserved := 0

## The id of a request that was never made. Shared so every seam spells "no id" the same way.
const NO_REQUEST_ID := 0


## The first id of a fresh block. One call per seam instance, at construction.
static func reserve_block() -> int:
	_blocks_reserved += 1
	return REQUEST_ID_BASE + (Time.get_ticks_usec() + _blocks_reserved) * IDS_PER_SESSION
