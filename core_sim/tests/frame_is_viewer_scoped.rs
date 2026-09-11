//! ⛔ **A PUBLISHED FRAME IS ONE VIEWER'S VIEW — every faction-keyed section of it.**
//!
//! The band list was the first unfiltered section found (`foreign_band_redaction.rs`), and finding
//! one meant the *section list* needed sweeping rather than one spelling correcting. It did: the
//! stockpiles, the research ladder, the crafts, the discovered sites, the espionage ledger, the
//! great discoveries, the event feed and the improvements on the ground were all published for
//! every faction, to every client.
//!
//! **The rule this file pins**, per section: *the viewer's own rows in full; a rival's absent.* A
//! section whose subject has no thing on the map to look at — a stockpile total, a knowledge track —
//! has no "visible" tier at all, so there is nothing to redact it down to. Where a section describes
//! a thing on the ground (the improvement on a tile) the rival's half is legible exactly where the
//! viewer has been. See `factions.md` → "Which frame sections are viewer-scoped".
//!
//! **Every assertion reads the ENCODED envelope.** A field that never reached the codec still
//! satisfies an in-process assertion, and the published artifact is the thing that leaks.

use bevy::prelude::*;

mod faction_support;

use core_sim::{
    publish_baseline_snapshot, run_turn, CommandEventEntry, CommandEventKind, CommandEventLog,
    DiscoveredSites, DiscoveryProgressLedger, FactionInventory, ForageRegistry, GreatDiscoveryId,
    GreatDiscoveryLedger, GreatDiscoveryRecord, GreatDiscoveryRegistry, KnowledgeLedger,
    KnowledgeLedgerEntry, Scalar, SnapshotHistory, VisibilityLedger, CULTIVATION_DISCOVERY_ID,
};
use faction_support::{world_with, HOME, ONE_RIVAL, RIVAL};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

/// The stockpiled good each faction is given, and how much of it, so a leaked row is unmistakable in
/// a failure message rather than a plausible number.
const STOCK_ITEM: &str = "provisions";
const HOME_STOCK: i64 = 111;
const RIVAL_STOCK: i64 = 222;

/// A partial research reading — a *learning* meter, so the row has to carry a fraction rather than a
/// known/unknown bit and cannot be mistaken for an absent row.
const RIVAL_PROGRESS: f32 = 0.5;

/// The great discovery ids the fixture plants: one kept quiet, one deployed publicly.
const COVERT_DISCOVERY: u16 = 4001;
const PUBLIC_DISCOVERY: u16 = 4002;

/// **Victory progress, one distinguishable reading per faction.** Distinct values, because the mode
/// rows carry no faction on the wire — the only way to say whose progress reached the frame is to
/// give the two peoples numbers that cannot be confused.
const HOME_VICTORY_PROGRESS: f32 = 0.25;
const RIVAL_VICTORY_PROGRESS: f32 = 0.75;

/// A wondrous site id and where each faction "found" one. Distinct coordinates so a row cannot be
/// attributed to the wrong faction by accident.
const SITE_ID: &str = "a_test_site";
const HOME_SITE: UVec2 = UVec2::new(3, 3);
const RIVAL_SITE: UVec2 = UVec2::new(9, 9);

/// One decoded frame, reduced to *which factions each section speaks for*.
///
/// Per-section faction lists rather than per-section row structs: the claim under test is membership
/// — whose rows are in the frame — and a list of faction ids prints a leak legibly.
#[derive(Debug, Default)]
struct FrameFactions {
    inventory: Vec<u32>,
    sedentarization: Vec<u32>,
    discovered_sites: Vec<u32>,
    intensification_knowledge: Vec<u32>,
    craft_knowledge: Vec<u32>,
    discovery_progress: Vec<u32>,
    great_discoveries: Vec<u32>,
    great_discovery_progress: Vec<u32>,
    knowledge_ledger: Vec<u32>,
    command_events: Vec<u32>,
    demographics: Vec<u32>,
    stance_axes: Vec<u32>,
    voice_medium: Vec<u32>,
    pending_forks: Vec<u32>,
    /// The `owner` of every forage patch that publishes one — the improvement half of a patch row.
    patch_owners: Vec<u32>,
    /// The stockpiled quantities the frame carries, so the *content* of a surviving row can be
    /// checked and not just its presence.
    inventory_quantities: Vec<(u32, i64)>,
}

fn dedup(mut ids: Vec<u32>) -> Vec<u32> {
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn frame_factions(app: &App) -> FrameFactions {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let payload = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot");

    let mut out = FrameFactions::default();
    if let Some(section) = payload.economy() {
        if let Some(rows) = section.factionInventory() {
            for row in rows.iter() {
                out.inventory.push(row.faction());
                let quantity = row
                    .inventory()
                    .and_then(|items| {
                        items
                            .iter()
                            .find(|entry| entry.item().unwrap_or_default() == STOCK_ITEM)
                            .map(|entry| entry.quantity())
                    })
                    .unwrap_or_default();
                out.inventory_quantities.push((row.faction(), quantity));
            }
        }
    }
    if let Some(section) = payload.subsistence() {
        if let Some(rows) = section.sedentarization() {
            out.sedentarization.extend(rows.iter().map(|r| r.faction()));
        }
        if let Some(rows) = section.intensificationKnowledge() {
            out.intensification_knowledge
                .extend(rows.iter().map(|r| r.faction()));
        }
        if let Some(rows) = section.craftKnowledge() {
            out.craft_knowledge.extend(rows.iter().map(|r| r.faction()));
        }
        if let Some(rows) = section.foragePatches() {
            // **`hasOwner`, not `owner != 0`** — faction 0 is a real faction, and the wire carries
            // the presence bit precisely so "unowned" and "owned by faction 0" are different
            // readings.
            out.patch_owners
                .extend(rows.iter().filter(|r| r.hasOwner()).map(|r| r.owner()));
        }
    }
    if let Some(section) = payload.knowledge() {
        if let Some(rows) = section.discoveredSites() {
            out.discovered_sites
                .extend(rows.iter().map(|r| r.faction()));
        }
        if let Some(rows) = section.discoveryProgress() {
            out.discovery_progress
                .extend(rows.iter().map(|r| r.faction()));
        }
        if let Some(rows) = section.greatDiscoveries() {
            out.great_discoveries
                .extend(rows.iter().map(|r| r.faction()));
        }
        if let Some(rows) = section.greatDiscoveryProgress() {
            out.great_discovery_progress
                .extend(rows.iter().map(|r| r.faction()));
        }
        if let Some(rows) = section.knowledgeLedger() {
            out.knowledge_ledger
                .extend(rows.iter().map(|r| r.ownerFaction()));
        }
    }
    if let Some(section) = payload.campaign() {
        if let Some(rows) = section.commandEvents() {
            out.command_events.extend(rows.iter().map(|r| r.faction()));
        }
        if let Some(rows) = section.stanceAxes() {
            out.stance_axes.extend(rows.iter().map(|r| r.faction()));
        }
        if let Some(rows) = section.voiceMedium() {
            out.voice_medium.extend(rows.iter().map(|r| r.faction()));
        }
        if let Some(rows) = section.pendingForks() {
            out.pending_forks.extend(rows.iter().map(|r| r.faction()));
        }
    }
    if let Some(section) = payload.population() {
        if let Some(rows) = section.demographics() {
            out.demographics.extend(rows.iter().map(|r| r.faction()));
        }
    }
    out
}

/// A two-faction world in which **both** peoples have something to publish in every swept section.
///
/// Both halves matter. Giving only the rival state would let "the section is empty" pass as
/// "filtered"; giving only the viewer state would let "the section is the viewer's" pass as
/// "unfiltered but the rival happened to have nothing".
fn a_world_where_both_peoples_have_something_to_hide() -> App {
    let mut app = world_with(ONE_RIVAL, |_| {});

    for (faction, stock) in [(HOME, HOME_STOCK), (RIVAL, RIVAL_STOCK)] {
        app.world.resource_mut::<FactionInventory>().add_stockpile(
            faction,
            STOCK_ITEM.to_string(),
            stock,
        );
    }

    // Research: a partial reading on a ladder knowledge, which drives THREE sections at once —
    // `discoveryProgress`, `intensificationKnowledge` and `craftKnowledge` all read this ledger.
    {
        let mut discovery = app.world.resource_mut::<DiscoveryProgressLedger>();
        for faction in [HOME, RIVAL] {
            discovery.add_progress(
                faction,
                CULTIVATION_DISCOVERY_ID,
                Scalar::from_f32(RIVAL_PROGRESS),
            );
        }
    }

    // **Both peoples partway up a real constellation**, so `activeConstellations` is a live number
    // rather than a zero that agrees with everything. The requirement ids are read off the LIVE
    // registry rather than typed in: they are `great_discovery_definitions.json`'s, and a fixture
    // holding its own copy would go quietly vacuous the day the catalogue is retuned.
    {
        let requirements: Vec<u32> = app
            .world
            .resource::<GreatDiscoveryRegistry>()
            .definitions()
            .next()
            .expect("the catalogue declares at least one constellation")
            .requirements
            .iter()
            .map(|requirement| requirement.discovery_id)
            .collect();
        assert!(
            !requirements.is_empty(),
            "the fixture needs a constellation with requirements to make progress on"
        );
        let mut discovery = app.world.resource_mut::<DiscoveryProgressLedger>();
        for faction in [HOME, RIVAL] {
            for discovery_id in &requirements {
                discovery.add_progress(faction, *discovery_id, Scalar::one());
            }
        }
    }

    {
        let mut sites = app.world.resource_mut::<DiscoveredSites>();
        sites.record(HOME, HOME_SITE, SITE_ID.to_string());
        sites.record(RIVAL, RIVAL_SITE, SITE_ID.to_string());
    }

    {
        let tick = app.world.resource::<core_sim::SimulationTick>().0;
        let mut log = app.world.resource_mut::<CommandEventLog>();
        for faction in [HOME, RIVAL] {
            log.push(CommandEventEntry::new(
                tick,
                CommandEventKind::Forage,
                faction,
                format!("a thing {faction:?} did"),
                None,
            ));
        }
    }

    {
        let mut ledger = app.world.resource_mut::<GreatDiscoveryLedger>();
        for faction in [HOME, RIVAL] {
            for id in [COVERT_DISCOVERY, PUBLIC_DISCOVERY] {
                ledger.push(GreatDiscoveryRecord {
                    id: GreatDiscoveryId(id),
                    faction,
                    field: Default::default(),
                    tick: 0,
                    publicly_deployed: false,
                    effect_flags: 0,
                });
            }
            ledger.mark_public(faction, GreatDiscoveryId(PUBLIC_DISCOVERY));
        }
    }

    {
        let config = app.world.resource::<KnowledgeLedger>().config();
        let mut ledger = app.world.resource_mut::<KnowledgeLedger>();
        for faction in [HOME, RIVAL] {
            ledger.upsert_entry(KnowledgeLedgerEntry::new(
                faction,
                CULTIVATION_DISCOVERY_ID,
                &config,
            ));
        }
    }

    // **A turn FIRST, so visibility has settled** — the improvement gate below is chosen against
    // the ledger this turn produces, not against a guess about it.
    run_turn(&mut app);

    // One improvement per faction. **The rival's has to sit on ground HOME has genuinely never
    // walked, and that is established here rather than assumed.**
    //
    // ⛔ **NEVER `take(n)` OFF A `HashMap` IN A TEST.** `ForageRegistry::patches` is a `HashMap`
    // whose own doc says the iteration order is non-deterministic, and Rust seeds the hasher per
    // process — so an unsorted pick draws different tiles on different runs. The first version of
    // this fixture did exactly that and *assumed* the draw landed on unexplored ground: it passed
    // most runs, because most of the map is unexplored, and failed the ones where it did not.
    let (home_tile, rival_tile) = {
        let mut candidates: Vec<UVec2> = app
            .world
            .resource::<ForageRegistry>()
            .patches
            .keys()
            .copied()
            .collect();
        // Row-major, the same order the capture publishes patches in.
        candidates.sort_unstable_by_key(|tile| (tile.y, tile.x));
        let home_tile = *candidates
            .first()
            .expect("the map seeds forage patches to own");
        // **Chosen through the capture path's own predicate**, so the precondition cannot be wrong
        // by construction: `is_discovered` is exactly what `snapshot_forage_patches` asks.
        let ledger = app.world.resource::<VisibilityLedger>();
        let rival_tile = candidates
            .iter()
            .copied()
            .find(|tile| *tile != home_tile && !ledger.is_discovered(HOME, tile.x, tile.y))
            .expect(
                "the fixture needs a patch tile HOME has NOT discovered — the rival's improvement \
                 has to sit on unexplored ground for the gate to be the thing under test",
            );
        (home_tile, rival_tile)
    };
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        for (tile, faction) in [(home_tile, HOME), (rival_tile, RIVAL)] {
            registry
                .patches
                .get_mut(&tile)
                .expect("the tile came out of this registry")
                .owner = Some(faction);
        }
    }

    // **Re-capture rather than resolve a second turn.** The owners have to reach a published frame,
    // and the settled visibility they were chosen against has to be the visibility the frame is
    // captured with — another `run_turn` would move the world underneath both.
    publish_baseline_snapshot(&mut app.world);
    app
}

/// The single claim, applied section by section: **the frame speaks for the viewer alone.**
#[test]
fn every_faction_keyed_section_carries_the_viewer_and_nobody_else() {
    let app = a_world_where_both_peoples_have_something_to_hide();
    let frame = frame_factions(&app);
    let own = vec![HOME.0];

    for (section, factions) in [
        ("factionInventory", &frame.inventory),
        ("sedentarization", &frame.sedentarization),
        ("discoveredSites", &frame.discovered_sites),
        ("intensificationKnowledge", &frame.intensification_knowledge),
        ("craftKnowledge", &frame.craft_knowledge),
        ("discoveryProgress", &frame.discovery_progress),
        ("greatDiscoveryProgress", &frame.great_discovery_progress),
        ("knowledgeLedger", &frame.knowledge_ledger),
        ("commandEvents", &frame.command_events),
        ("demographics", &frame.demographics),
        ("stanceAxes", &frame.stance_axes),
        ("voiceMedium", &frame.voice_medium),
    ] {
        assert_eq!(
            dedup(factions.clone()),
            own,
            "`{section}` must carry the viewer's rows and nobody else's — got {factions:?}"
        );
    }

    // **`pendingForks` is empty in this world and is asserted one-sidedly**, deliberately: a fork is
    // posted by a narrative beat firing, which this fixture does not stage. What can be stated
    // without staging one is that no rival's fork is ever in the frame — and that is the half that
    // would leak.
    assert!(
        !frame.pending_forks.contains(&RIVAL.0),
        "a rival's pending decisions are not the viewer's to read: {:?}",
        frame.pending_forks
    );
}

/// The liveness half of the test above, and it is not optional: *every* one of those assertions is
/// satisfied by an empty section, so without this the whole suite passes on a frame that publishes
/// nothing at all.
#[test]
fn the_viewers_own_rows_are_really_there() {
    let app = a_world_where_both_peoples_have_something_to_hide();
    let frame = frame_factions(&app);

    assert_eq!(
        frame.inventory_quantities,
        vec![(HOME.0, HOME_STOCK)],
        "the viewer's own stockpile is published, with its real quantity"
    );
    for (section, factions) in [
        ("sedentarization", &frame.sedentarization),
        ("discoveredSites", &frame.discovered_sites),
        ("intensificationKnowledge", &frame.intensification_knowledge),
        ("craftKnowledge", &frame.craft_knowledge),
        ("discoveryProgress", &frame.discovery_progress),
        ("knowledgeLedger", &frame.knowledge_ledger),
        ("commandEvents", &frame.command_events),
        ("demographics", &frame.demographics),
    ] {
        assert!(
            !factions.is_empty(),
            "`{section}` must still carry the viewer's own rows — an empty section would satisfy \
             the filter assertions vacuously"
        );
    }
}

/// ⛔ **THE ONE DELIBERATE EXEMPTION IN THE KNOWLEDGE SECTIONS.**
///
/// `publicly_deployed` is a live flag with its own mutator, and its whole meaning is *"this faction
/// has shown the world"*. Withholding a publicly-deployed discovery would leave the flag observable
/// only to its owner — the one reader it is not for. A discovery kept quiet stays quiet, and the
/// **progress** rows carry no such exemption at all.
#[test]
fn a_rivals_publicly_deployed_discovery_is_visible_and_its_covert_one_is_not() {
    let app = a_world_where_both_peoples_have_something_to_hide();
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let rows: Vec<(u32, u16)> = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .knowledge()
        .and_then(|section| section.greatDiscoveries())
        .expect("the great-discovery list is published")
        .iter()
        .map(|row| (row.faction(), row.id()))
        .collect();

    assert!(
        rows.contains(&(HOME.0, COVERT_DISCOVERY)),
        "the viewer's own covert discovery is the viewer's to see: {rows:?}"
    );
    assert!(
        rows.contains(&(RIVAL.0, PUBLIC_DISCOVERY)),
        "a rival's PUBLICLY DEPLOYED discovery is what the flag is for: {rows:?}"
    );
    assert!(
        !rows.contains(&(RIVAL.0, COVERT_DISCOVERY)),
        "a rival's COVERT discovery must not ride the wire: {rows:?}"
    );
}

/// ⛔ **AN AGGREGATE MUST AGREE WITH THE LIST IT SUMMARISES.**
///
/// `greatDiscoveryTelemetry`'s three counters are `count(...)` over the same per-faction ledgers the
/// two lists beside them are built from, and every one of them used to count across **every
/// faction** — `totalResolved` was literally `ledger.records.len()`. A `u32` carries no faction, so
/// the section did not *look* faction-keyed and the row-by-row sweep walked past it; the symptom was
/// a client printing *"Resolved discoveries: 7"* above a list of 2.
///
/// **So the assertion is agreement, not filtering.** "The count is viewer-scoped" would pass on any
/// number that happens to be small; "the count equals the number of rows the frame carries" is the
/// claim a reader of the panel actually depends on, and it is the one that catches the next counter
/// to launder faction-keyed data into a world-looking figure.
///
/// The fixture is what gives this teeth: each faction holds one covert discovery and one public one,
/// so the viewer's list is **three** rows (own covert, own public, the rival's public) while the
/// ledger holds **four**. An unfiltered count reads 4 over a list of 3.
#[test]
fn the_discovery_counters_agree_with_the_lists_they_summarise() {
    let app = a_world_where_both_peoples_have_something_to_hide();
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let section = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .knowledge()
        .expect("the knowledge section is published");

    let resolved_rows = section
        .greatDiscoveries()
        .map(|rows| rows.len())
        .unwrap_or(0);
    // **NO PRE-FILTER.** This used to count only the rows reading `progress > 0`, which asserted
    // something weaker than the rule it was written to enforce: the list published a row for every
    // unresolved constellation and the counter counted only the started ones, so the test agreed
    // with itself while a fresh world shipped `N` rows of zeros under a count of `0`. The list and
    // the counter share one predicate now (`great_discovery::constellation_is_in_flight`), so the
    // published rows are countable as they stand.
    let active_rows = section
        .greatDiscoveryProgress()
        .map(|rows| rows.len())
        .unwrap_or(0);
    let telemetry = section
        .greatDiscoveryTelemetry()
        .expect("the telemetry rides every frame");

    assert!(
        resolved_rows > 0,
        "the liveness half: the fixture has to publish resolved discoveries, or a count of 0 \
         agrees with an empty list and proves nothing"
    );
    assert_eq!(
        telemetry.totalResolved() as usize,
        resolved_rows,
        "`totalResolved` must count the rows the frame carries, not the rows the ledger holds"
    );
    assert!(
        active_rows > 0,
        "the liveness half again, for the readiness counters: the fixture has to put the viewer \
         partway up a constellation, or `activeConstellations == 0` agrees with an empty list \
         whether it is filtered or not"
    );
    assert_eq!(
        telemetry.activeConstellations() as usize,
        active_rows,
        "`activeConstellations` must count the viewer's own in-flight rows"
    );
    assert!(
        telemetry.pendingCandidates() <= telemetry.activeConstellations(),
        "a candidate is an active constellation, so it cannot outnumber them: {} > {}",
        telemetry.pendingCandidates(),
        telemetry.activeConstellations()
    );
}

/// ⛔ **A CONSTELLATION NOBODY HAS STARTED IS NOT A ROW.**
///
/// [`GreatDiscoveryReadiness`] pre-seeds an entry per definition per faction, so *"unresolved"* alone
/// is the whole catalogue: a fresh world published `N` rows all reading `progress == 0` under an
/// `activeConstellations` of `0`, because the counter asked *"started?"* and the list did not.
///
/// **The agreement test above can no longer catch this**, and that is by construction rather than a
/// gap: the list and the counter share one predicate now, so they agree whatever that predicate
/// says. What has to be pinned separately is *which* predicate — that a published progress row means
/// research in flight — and this is that assertion.
///
/// Its liveness half is the catalogue beside it: the frame has to publish **fewer** progress rows
/// than there are constellations to chase, or the row set was never narrowed and the claim is
/// vacuous.
#[test]
fn an_unstarted_constellation_is_not_a_published_progress_row() {
    let app = a_world_where_both_peoples_have_something_to_hide();
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let section = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .knowledge()
        .expect("the knowledge section is published");

    let zeroes: Vec<u16> = section
        .greatDiscoveryProgress()
        .map(|rows| {
            rows.iter()
                .filter(|row| row.progress() == 0)
                .map(|row| row.discovery())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        zeroes.is_empty(),
        "a progress row states research in flight; these constellations are unstarted: {zeroes:?}"
    );

    let published = section
        .greatDiscoveryProgress()
        .map(|rows| rows.len())
        .unwrap_or(0);
    let catalogue = section
        .greatDiscoveryDefinitions()
        .map(|rows| rows.len())
        .unwrap_or(0);
    assert!(
        published > 0 && published < catalogue,
        "the liveness half: the viewer has to be partway up SOME constellation and not up all of \
         them, or an empty-or-total row set satisfies the assertion above without narrowing \
         anything — {published} rows against a catalogue of {catalogue}"
    );
}

/// The counter's own leak, stated directly: the ledger genuinely holds more than the frame counts.
///
/// Separate from the agreement test above because the two fail for different reasons — this one
/// fails if the *fixture* stops staging a discovery the viewer may not see, which would make the
/// agreement test vacuous without saying so.
#[test]
fn the_resolved_count_is_smaller_than_the_ledger_it_is_drawn_from() {
    let app = a_world_where_both_peoples_have_something_to_hide();
    let held = app.world.resource::<GreatDiscoveryLedger>().records().len();
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let published = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .knowledge()
        .and_then(|section| section.greatDiscoveryTelemetry())
        .expect("the telemetry rides every frame")
        .totalResolved() as usize;

    assert!(
        published < held,
        "the fixture stages a discovery the viewer may not see, so the published count MUST be \
         smaller than the {held} the ledger holds — got {published}"
    );
}

/// **The improvement on the ground follows the ground.** A patch row is a fact about a tile and
/// stays; who tends it, and how far along their meters are, is a fact about a people.
#[test]
fn a_rivals_field_on_ground_the_viewer_has_never_walked_names_no_owner() {
    let app = a_world_where_both_peoples_have_something_to_hide();
    let frame = frame_factions(&app);
    assert_eq!(
        dedup(frame.patch_owners.clone()),
        vec![HOME.0],
        "only the viewer's own improvements name an owner on unexplored ground — got {:?}",
        frame.patch_owners
    );
}

/// With fog off there is no unexplored ground, so a rival's improvement **is** legible — the same
/// rule `route_states` follows, and the same rule item 10's band filter follows: fog decides what
/// you can SEE. It is not an entitlement switch, which is why every *other* section above stays
/// viewer-only here.
#[test]
fn fog_off_makes_a_rivals_improvement_legible_and_changes_nothing_else() {
    let mut app = world_with(ONE_RIVAL, |config| config.fog_enabled = false);
    app.world.resource_mut::<FactionInventory>().add_stockpile(
        RIVAL,
        STOCK_ITEM.to_string(),
        RIVAL_STOCK,
    );
    app.world.resource_mut::<FactionInventory>().add_stockpile(
        HOME,
        STOCK_ITEM.to_string(),
        HOME_STOCK,
    );
    {
        let mut registry = app.world.resource_mut::<ForageRegistry>();
        // Sorted, for the reason the sibling fixture states at length: a `HashMap`'s first key is
        // not a stable choice. With fog off the *legibility* does not depend on which tile is
        // picked — but which tile is picked must not depend on the run.
        let mut candidates: Vec<UVec2> = registry.patches.keys().copied().collect();
        candidates.sort_unstable_by_key(|tile| (tile.y, tile.x));
        let tile = *candidates.first().expect("the map seeds forage patches");
        registry
            .patches
            .get_mut(&tile)
            .expect("the patch is there")
            .owner = Some(RIVAL);
    }
    run_turn(&mut app);

    let frame = frame_factions(&app);
    assert!(
        frame.patch_owners.contains(&RIVAL.0),
        "with fog off the rival's improvement is on ground the viewer can see: {:?}",
        frame.patch_owners
    );
    assert_eq!(
        dedup(frame.inventory.clone()),
        vec![HOME.0],
        "…and the stockpile is still the viewer's alone: fog is not a disclosure switch"
    );
}

/// ⛔ **VICTORY PROGRESS IS THE VIEWER'S; THE WINNER IS THE WORLD'S.**
///
/// Progress is a claim about one people — how many of *your* people there are and how they feel — so
/// publishing every faction's rows would be a live readout of exactly how close each rival is. The
/// **winner** is the deliberate exemption beside it, and it is not a leak: a winner is public by
/// definition, and the row names the faction that actually achieved it rather than `FactionId(0)`.
///
/// The rows carry no faction on the wire, so the two peoples are given **different progress
/// readings** and the assertion is on the value. Sabotaged by publishing `state.modes` whole: the
/// frame would then carry two rows, and by publishing faction 0's regardless of viewer: it would
/// carry the home reading while the winner says otherwise — which is the state that shipped.
#[test]
fn victory_progress_is_the_viewers_own_and_the_winner_is_public() {
    let mut app = world_with(ONE_RIVAL, |_| {});
    {
        let mut victory = app.world.resource_mut::<core_sim::VictoryState>();
        for (faction, progress) in [
            (HOME, HOME_VICTORY_PROGRESS),
            (RIVAL, RIVAL_VICTORY_PROGRESS),
        ] {
            victory.modes.insert(
                faction,
                vec![core_sim::VictoryModeState {
                    id: core_sim::VictoryModeId("hegemony".to_string()),
                    kind: core_sim::VictoryModeKind::Hegemony,
                    progress,
                    threshold: 1.0,
                    achieved: false,
                }],
            );
        }
        // The rival is the one who won, so a frame that named the player would be naming the wrong
        // people rather than merely a default.
        victory.winner = Some(core_sim::VictoryResult {
            mode: core_sim::VictoryModeId("hegemony".to_string()),
            faction: RIVAL,
            tick: 7,
        });
    }
    publish_baseline_snapshot(&mut app.world);

    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    let bytes = sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref());
    let envelope =
        fb::root_as_envelope(bytes.as_ref()).expect("the snapshot encodes to a valid envelope");
    let victory = envelope
        .payload_as_snapshot()
        .expect("the envelope carries a snapshot")
        .campaign()
        .and_then(|section| section.victory())
        .expect("the campaign section carries victory state");

    let progress: Vec<f32> = victory
        .modes()
        .expect("the viewer's mode rows are published")
        .iter()
        .map(|row| row.progress())
        .collect();
    assert_eq!(
        progress,
        vec![HOME_VICTORY_PROGRESS],
        "the frame carries the viewer's progress and nobody else's"
    );

    let winner = victory.winner().expect("the winner is published");
    assert_eq!(
        winner.faction(),
        RIVAL.0,
        "a winner is public, and it is the faction that actually achieved it"
    );
}
