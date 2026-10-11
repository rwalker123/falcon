//! **Independence — a cut-off, aggrieved band becomes its own people** (#284,
//! `docs/plan_band_fission.md` §Independence). Rationale: `.claude/rules/core_sim/independence.md`.
//!
//! Each turn, per people: its resident bands are joined wherever a **live contact tie** runs
//! between two of them (either direction — [`ConnectionLedger::tie_is_live`]), and the component
//! holding the most people is that people's **heart**. A band outside its heart is **cut off**.
//! Every cut-off component whose people-weighted mean `grievance` reaches
//! `wellbeing_config.json → independence.grievance_threshold` breaks away **as one new people**,
//! driven by the AI, and takes what is its own on defection's band-flip path.
//!
//! The clock is the tie's own bleed and the gate is the discontent block's grievance: this module
//! adds the one number that joins them and no second counter.

use std::collections::BTreeMap;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::labor::{follow_the_band_to_its_new_people, BandFlip, BandFlipFollowers};
use crate::{
    components::{BandId, PopulationCohort, ResidentBand},
    connections::{ConnectionLedger, FULL_TIE, NO_TIE},
    culture::CultureSplitQueue,
    espionage::{CounterIntelBudgets, EspionageCatalog, EspionageRoster, FactionSecurityPolicies},
    faction_names::{FactionNameCatalog, FactionNameCatalogHandle, FactionNames},
    lineage::{tie_joined_groups, FreeBreedingPeoples},
    orders::{FactionId, FactionRegistry, TurnQueue},
    resources::{
        CommandEventEntry, CommandEventKind, CommandEventLog, DiscoveryProgressLedger,
        FactionBorderPolicies, SimulationConfig, SimulationTick, TileRegistry,
    },
    scalar::{scalar_from_f32, scalar_zero, Scalar},
    systems::population::{band_label, faction_label},
    visibility::VisibilityLedger,
    wellbeing_config::WellbeingConfigHandle,
};

/// **One band's standing toward the heart of its people**, as of the last turn it was judged.
///
/// `bond` and `last_contact_turn` are read off the ties between this band and the **other** bands of
/// its people's heart — so a heart member reads the tie that holds it there draining, and a band
/// that is the whole of its heart reads [`FULL_TIE`] and the current turn (it *is* the heart).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartReading {
    /// The people the band was judged as. A band that changes people is not "losing touch" with
    /// the people it left, so the lost-touch edge only fires within one people.
    pub faction: FactionId,
    /// Outside its people's heart: no live tie, in either direction, to any band of the heart.
    pub cut_off: bool,
    /// The strongest tie (either direction) between this band and any other band of its heart.
    pub bond: Scalar,
    /// The latest `last_contact_turn` of any tie between this band and its heart; `None` when no
    /// edge joins them at all.
    pub last_contact_turn: Option<u64>,
}

/// **Every resident band's [`HeartReading`], keyed by its durable id.** Checkpoint state, not
/// derived: the lost-touch line is an EDGE, so the previous turn's `cut_off` is what a turn reads to
/// know a band has just lost touch, and the capture publishes the readings before any turn has
/// re-judged a restored world.
///
/// Rebuilt whole every turn by [`advance_band_independence`] (so a band that died leaves no row),
/// and written once outside it: a split hands the splinter its parent's reading
/// ([`Self::inherit`]), because the two were one band a moment ago.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartLedger {
    readings: BTreeMap<BandId, HeartReading>,
}

impl HeartLedger {
    /// This band's reading, or `None` for a band no turn has judged yet.
    pub fn reading(&self, band: BandId) -> Option<&HeartReading> {
        self.readings.get(&band)
    }

    /// Whether this band is cut off from its people's heart. A band no turn has judged is not.
    pub fn is_cut_off(&self, band: BandId) -> bool {
        self.reading(band).is_some_and(|reading| reading.cut_off)
    }

    /// **The splinter starts where its parent stands** — the same people, the same standing toward
    /// the heart. Without it a splinter would have no previous reading and its first turn would
    /// judge it from nothing.
    pub fn inherit(&mut self, child: BandId, parent: BandId) {
        if let Some(reading) = self.readings.get(&parent).copied() {
            self.readings.insert(child, reading);
        }
    }

    /// Every reading, in band order.
    pub fn iter(&self) -> impl Iterator<Item = (BandId, &HeartReading)> {
        self.readings.iter().map(|(band, reading)| (*band, reading))
    }
}

/// **Every resource the boot path builds from the roster**, bundled so a people born at runtime
/// extends all of them through one function ([`grow_faction_roster`]) — `factions.md` → "THE
/// ROSTER-DERIVED SET IS SIX RESOURCES, AND A RUNTIME PATH OWES ALL OF THEM".
#[derive(SystemParam)]
pub struct RosterResources<'w> {
    pub registry: ResMut<'w, FactionRegistry>,
    pub turn_queue: ResMut<'w, TurnQueue>,
    pub counter_intel: ResMut<'w, CounterIntelBudgets>,
    pub security: ResMut<'w, FactionSecurityPolicies>,
    pub borders: ResMut<'w, FactionBorderPolicies>,
    pub names: ResMut<'w, FactionNames>,
    pub espionage_roster: ResMut<'w, EspionageRoster>,
    pub espionage_catalog: Res<'w, EspionageCatalog>,
    /// The peoples whose breeding is free for good; a people born from a latched one inherits it.
    pub free_breeding: ResMut<'w, FreeBreedingPeoples>,
    /// The faction-name pool. `Option` for worldgen's reason: a hand-built world may install none,
    /// and the builtin pool is the very list `include_str!` baked in.
    pub name_catalog: Option<Res<'w, FactionNameCatalogHandle>>,
}

/// **Register one new AI people and extend every roster-derived resource for it** — the one
/// statement of the runtime roster path, so the set has one home.
///
/// Each extension goes through the boot path's own constructor seam (`seed_faction` on the
/// resource `new` builds through, `FactionNames::mint_faction` on worldgen's permutation,
/// `EspionageRoster::seed_from_catalog`), so a people born mid-game starts in exactly the state a
/// people present at world creation did. The turn queue awaits it from the NEXT turn
/// ([`TurnQueue::add_faction`]) — the turn in flight was already collected. `parent` is the people
/// it breaks from: the new people inherits its free-breeding latch ([`FreeBreedingPeoples`]).
pub fn grow_faction_roster(
    roster: &mut RosterResources,
    map_seed: u64,
    parent: FactionId,
) -> FactionId {
    let faction = roster.registry.add_ai_faction();
    roster.free_breeding.inherit(faction, parent);
    roster.turn_queue.add_faction(faction);
    let budget_config = roster
        .espionage_catalog
        .config()
        .counter_intel_budget()
        .clone();
    roster.counter_intel.seed_faction(faction, &budget_config);
    roster.security.seed_faction(faction);
    roster.borders.seed_faction(faction);
    let name_catalog = roster
        .name_catalog
        .as_ref()
        .map(|handle| handle.get())
        .unwrap_or_else(FactionNameCatalog::builtin);
    roster
        .names
        .mint_faction(faction, map_seed, name_catalog.as_ref());
    roster
        .espionage_roster
        .seed_from_catalog(&[faction], &roster.espionage_catalog);
    faction
}

/// What a break-away seeds its new people with beyond the roster: the knowledge ledger and the
/// fog. Bundled to keep the system inside Bevy's argument budget.
#[derive(SystemParam)]
pub struct NewPeopleSeeds<'w> {
    pub discovery: ResMut<'w, DiscoveryProgressLedger>,
    pub visibility: ResMut<'w, VisibilityLedger>,
    pub tiles: Res<'w, TileRegistry>,
}

/// One resident band, as the heart computation reads it.
struct BandView {
    entity: Entity,
    band: BandId,
    faction: FactionId,
    people: Scalar,
    grievance: Scalar,
}

/// One connected group of a people's bands.
struct Group {
    faction: FactionId,
    /// Indices into the band list, in `BandId` order.
    members: Vec<usize>,
    people: Scalar,
    /// The group's lowest `BandId` — its deterministic name, the heart's tie-break and the order
    /// several break-aways on one turn are processed in.
    lowest: BandId,
    is_heart: bool,
}

/// Every resident band with a durable id, sorted by `BandId`, so everything downstream is
/// independent of query order. A band with no `BandId` has no ties to read and is not judged.
fn band_views(
    cohorts: &Query<(Entity, &mut PopulationCohort, Option<&BandId>), With<ResidentBand>>,
) -> Vec<BandView> {
    let mut bands: Vec<BandView> = cohorts
        .iter()
        .filter_map(|(entity, cohort, band)| {
            band.map(|band| BandView {
                entity,
                band: *band,
                faction: cohort.faction,
                people: cohort.total(),
                grievance: cohort.grievance,
            })
        })
        .collect();
    bands.sort_by_key(|view| view.band);
    bands
}

/// **Each people's bands split into the components live ties join**, each marked whether it is its
/// people's heart. Groups come out in the order of their lowest `BandId`.
///
/// The heart is the component holding the most people, ties to the lowest `BandId` — the people
/// is where its people are, not where it started. A people with one band is its own heart.
fn heart_groups(bands: &[BandView], ledger: &ConnectionLedger) -> Vec<Group> {
    // The grouping itself is `lineage::tie_joined_groups` — the same one the breeding ceiling reads.
    let keyed: Vec<(BandId, FactionId)> =
        bands.iter().map(|view| (view.band, view.faction)).collect();
    let mut groups: Vec<Group> = Vec::new();
    for joined in tie_joined_groups(&keyed, ledger) {
        let people = joined
            .members
            .iter()
            .fold(scalar_zero(), |sum, &index| sum + bands[index].people);
        groups.push(Group {
            faction: joined.faction,
            lowest: bands[joined.members[0]].band,
            members: joined.members,
            people,
            is_heart: false,
        });
    }
    // The heart of each people: the component holding the most people. Groups open in BandId order,
    // so the first strictly-largest is the tie-break winner (the lowest BandId).
    let mut start = 0;
    while start < groups.len() {
        let faction = groups[start].faction;
        let end = groups[start..]
            .iter()
            .position(|group| group.faction != faction)
            .map_or(groups.len(), |offset| start + offset);
        let mut heart = start;
        for candidate in start..end {
            if groups[candidate].people > groups[heart].people {
                heart = candidate;
            }
        }
        groups[heart].is_heart = true;
        start = end;
    }
    groups.sort_by_key(|group| group.lowest);
    groups
}

/// The people-weighted mean grievance of a group. A group with no people has none.
fn weighted_grievance(group: &Group, bands: &[BandView]) -> Scalar {
    if group.people <= scalar_zero() {
        return scalar_zero();
    }
    let weighted = group.members.iter().fold(scalar_zero(), |sum, &index| {
        sum + bands[index].grievance * bands[index].people
    });
    weighted / group.people
}

/// The strongest tie and latest contact between `band` and the bands of `heart` other than itself.
fn standing_toward(
    band: BandId,
    heart: &[BandId],
    ledger: &ConnectionLedger,
    tick: u64,
) -> (Scalar, Option<u64>) {
    let others: Vec<BandId> = heart.iter().copied().filter(|h| *h != band).collect();
    if others.is_empty() && heart.contains(&band) {
        // The band IS its people's heart.
        return (FULL_TIE, Some(tick));
    }
    let mut bond = NO_TIE;
    let mut last: Option<u64> = None;
    for other in others {
        for edge in ledger.edges_between(band, other).into_iter().flatten() {
            bond = bond.max(edge.strength);
            last = Some(last.map_or(edge.last_contact_turn, |seen| {
                seen.max(edge.last_contact_turn)
            }));
        }
    }
    (bond, last)
}

/// **Why a band broke away** — rides the `band_broke_away` detail as `cause=` when it is not the
/// default, and picks the line each people reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BreakAwayCause {
    /// A cut-off, aggrieved group ([`advance_band_independence`]). The detail carries no `cause`.
    Grievance,
    /// A purist band whose cultural strain held past the hard threshold ([`advance_culture_splits`],
    /// #702). The detail carries [`CULTURE_CAUSE_TOKEN`].
    Culture,
}

/// The `cause=` token a culture split carries on its `band_broke_away` detail.
pub const CULTURE_CAUSE_TOKEN: &str = "culture";

/// **Tell both peoples a band broke away** — one row under the people it left, one under the people
/// it founded, on `push_band_changed_hands_events`' shape (`side=lost|gained`, same tokens).
fn push_band_broke_away_events(
    event_log: &mut CommandEventLog,
    tick: u64,
    band: BandId,
    from: FactionId,
    to: FactionId,
    cause: BreakAwayCause,
) {
    let name = band_label(band);
    let people = faction_label(to);
    let detail = |side: &str| {
        let base = format!("band={} from={} to={} side={}", band.0, from.0, to.0, side);
        Some(match cause {
            BreakAwayCause::Grievance => base,
            BreakAwayCause::Culture => format!("{base} cause={CULTURE_CAUSE_TOKEN}"),
        })
    };
    let (lost, gained) = match cause {
        BreakAwayCause::Grievance => (
            format!("{name} no longer answers to us — they call themselves {people}."),
            format!(
                "{name} broke away from {} — we answer to no one but ourselves.",
                faction_label(from)
            ),
        ),
        BreakAwayCause::Culture => (
            format!(
                "{name} grew too far from our ways and no longer answers to us — they call \
                 themselves {people}."
            ),
            format!(
                "{name} broke away from {} over its ways — we answer to no one but ourselves.",
                faction_label(from)
            ),
        ),
    };
    event_log.push(CommandEventEntry::new(
        tick,
        CommandEventKind::BandBrokeAway,
        from,
        lost,
        detail("lost"),
    ));
    event_log.push(CommandEventEntry::new(
        tick,
        CommandEventKind::BandBrokeAway,
        to,
        gained,
        detail("gained"),
    ));
}

/// **Hand a set of bands over to one new AI people** — the per-band hand-over both ways of breaking
/// away share: the roster grows ([`grow_faction_roster`]), the new people's discovery progress is
/// seeded from the old people's ledger in full and the bands' own knowledge, its fog is the old
/// people's map remembered, each band's `cohort.faction` moves and its grievance resets to zero,
/// and both peoples are told. Returns the new people and the flips for
/// `follow_the_band_to_its_new_people`, which the caller runs once every break-away of the turn is
/// in. Ties are band-keyed and are not touched.
#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
fn break_away_as_new_people(
    roster: &mut RosterResources,
    seeds: &mut NewPeopleSeeds,
    event_log: &mut CommandEventLog,
    cohorts: &mut Query<(Entity, &mut PopulationCohort, Option<&BandId>), With<ResidentBand>>,
    map_seed: u64,
    now: u64,
    from: FactionId,
    members: &[(Entity, BandId)],
    cause: BreakAwayCause,
) -> (FactionId, Vec<BandFlip>) {
    let to = grow_faction_roster(roster, map_seed, from);
    {
        let knowledge: Vec<&PopulationCohort> = members
            .iter()
            .filter_map(|(entity, _)| cohorts.get(*entity).ok())
            .map(|(_, cohort, _)| cohort)
            .collect();
        seed_new_peoples_knowledge(&mut seeds.discovery, from, to, &knowledge);
    }
    seeds
        .visibility
        .insert_remembered_copy(from, to, seeds.tiles.width, seeds.tiles.height);
    let mut flips = Vec::with_capacity(members.len());
    for &(entity, band) in members {
        if let Ok((_, mut cohort, _)) = cohorts.get_mut(entity) {
            cohort.faction = to;
            // It was a grievance against the people they left.
            cohort.grievance = scalar_zero();
        }
        push_band_broke_away_events(event_log, now, band, from, to, cause);
        flips.push(BandFlip {
            entity,
            band,
            from,
            to,
        });
    }
    (to, flips)
}

/// **Tell a people it has lost touch with one of its bands.**
fn push_lost_touch_event(
    event_log: &mut CommandEventLog,
    tick: u64,
    band: BandId,
    faction: FactionId,
) {
    event_log.push(CommandEventEntry::new(
        tick,
        CommandEventKind::LostTouch,
        faction,
        format!("We have lost touch with {}.", band_label(band)),
        Some(format!("band={}", band.0)),
    ));
}

/// **Seed a new people's discovery progress with everything they knew** — the OLD people's ledger
/// in full, and every fragment any of the group's bands holds, each discovery at the best progress
/// any source has on it.
///
/// The old people's ledger is the half that matters most: the lessons a people earns by practice
/// (the intensification ladder's — Cultivation, Herding, Foddering…) are credited to the faction
/// and never to a band's `knowledge`, so seeding from the bands alone would leave a break-away
/// unable to work what it worked the turn before. Knowledge is not conserved: the old people does
/// not forget it, two sources knowing one thing do not know it twice, and nobody left them, so
/// nothing is scaled down.
fn seed_new_peoples_knowledge(
    discovery: &mut DiscoveryProgressLedger,
    from: FactionId,
    faction: FactionId,
    knowledge: &[&PopulationCohort],
) {
    let mut best: BTreeMap<u32, Scalar> = discovery
        .progress
        .get(&from)
        .map(|lessons| {
            lessons
                .iter()
                .map(|(id, progress)| (*id, *progress))
                .collect()
        })
        .unwrap_or_default();
    for cohort in knowledge {
        for fragment in &cohort.knowledge {
            let entry = best.entry(fragment.discovery_id).or_insert(scalar_zero());
            *entry = (*entry).max(fragment.progress);
        }
    }
    for (discovery_id, progress) in best {
        if progress > scalar_zero() {
            discovery.add_progress(faction, discovery_id, progress);
        }
    }
}

/// **Judge every band against its people's heart; let each cut-off group that has had enough go.**
///
/// Runs in the Population chain after `advance_population_migration` and `advance_party_defection`,
/// so every band's people is final for the turn and its grievance is this turn's. Reads the contact
/// ties as the previous Visibility stage left them (the supply network's arrangement).
///
/// 1. **Groups.** Per people, the components live ties join; the heart is the largest.
/// 2. **Break-aways.** Every non-heart group whose people-weighted mean grievance is
///    `>= independence.grievance_threshold` becomes ONE new AI people, in the order of its lowest
///    `BandId`: the roster grows ([`grow_faction_roster`]), its discovery progress is seeded from
///    the old people's ledger in full and the bands' own knowledge, its fog is the old people's map remembered (no tile `Active`), each
///    band's `cohort.faction` moves and its grievance resets to zero, both peoples are told, and
///    `follow_the_band_to_its_new_people` carries its roads, its improvements, its parties and (on a
///    clash) its name. Ties are band-keyed and are not touched.
/// 3. **Readings.** Every band's [`HeartReading`] is rebuilt against the post-break-away roster, and
///    a band of the same people going from in touch to cut off pushes a lost-touch line.
#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn advance_band_independence(
    sim_config: Res<SimulationConfig>,
    wellbeing_config: Res<WellbeingConfigHandle>,
    tick: Res<SimulationTick>,
    connections: Res<ConnectionLedger>,
    mut heart_ledger: ResMut<HeartLedger>,
    mut event_log: ResMut<CommandEventLog>,
    mut roster: RosterResources,
    mut seeds: NewPeopleSeeds,
    mut followers: BandFlipFollowers,
    mut cohorts: Query<(Entity, &mut PopulationCohort, Option<&BandId>), With<ResidentBand>>,
) {
    let threshold = scalar_from_f32(wellbeing_config.get().independence.grievance_threshold);
    let now = tick.0;

    // ---- 1 + 2: the groups, and every one that breaks away ----
    let bands = band_views(&cohorts);
    let groups = heart_groups(&bands, &connections);
    let breaking: Vec<&Group> = groups
        .iter()
        .filter(|group| !group.is_heart && weighted_grievance(group, &bands) >= threshold)
        .collect();
    let mut flips: Vec<BandFlip> = Vec::new();
    for group in breaking {
        let members: Vec<(Entity, BandId)> = group
            .members
            .iter()
            .map(|&index| (bands[index].entity, bands[index].band))
            .collect();
        let (_, group_flips) = break_away_as_new_people(
            &mut roster,
            &mut seeds,
            &mut event_log,
            &mut cohorts,
            sim_config.map_seed,
            now,
            group.faction,
            &members,
            BreakAwayCause::Grievance,
        );
        flips.extend(group_flips);
    }
    let any_broke_away = !flips.is_empty();
    if any_broke_away {
        // After every band's people is final, so the name test sees this turn's whole roster.
        follow_the_band_to_its_new_people(&mut followers, &cohorts, flips, sim_config.map_seed);
    }

    // ---- 3: every band's standing, against the roster as it now is ----
    let (bands, groups) = if any_broke_away {
        let bands = band_views(&cohorts);
        let groups = heart_groups(&bands, &connections);
        (bands, groups)
    } else {
        (bands, groups)
    };
    let hearts: BTreeMap<FactionId, Vec<BandId>> = groups
        .iter()
        .filter(|group| group.is_heart)
        .map(|group| {
            (
                group.faction,
                group
                    .members
                    .iter()
                    .map(|&index| bands[index].band)
                    .collect(),
            )
        })
        .collect();
    let mut readings: BTreeMap<BandId, HeartReading> = BTreeMap::new();
    for group in &groups {
        let heart = hearts.get(&group.faction).map(Vec::as_slice).unwrap_or(&[]);
        for &index in &group.members {
            let view = &bands[index];
            let (bond, last_contact_turn) = standing_toward(view.band, heart, &connections, now);
            let reading = HeartReading {
                faction: view.faction,
                cut_off: !group.is_heart,
                bond,
                last_contact_turn,
            };
            let was_in_touch = heart_ledger
                .reading(view.band)
                .is_some_and(|previous| previous.faction == view.faction && !previous.cut_off);
            if was_in_touch && reading.cut_off {
                push_lost_touch_event(&mut event_log, now, view.band, view.faction);
            }
            readings.insert(view.band, reading);
        }
    }
    heart_ledger.readings = readings;
}

/// **A purist band that grew too far from its people splits off** (#702,
/// `docs/plan_contact_and_logistics.md` §"Settled by #530").
///
/// [`crate::culture::reconcile_culture_layers`] queues a band whose band-scope schism held past the
/// hard threshold while its Syncretic<->Purist value stood above `culture.split_min_purist` and its
/// people had another resident band. This system performs the split: that ONE band (not its
/// tie-joined group) becomes a new AI people through the same hand-over a break-away uses
/// ([`break_away_as_new_people`]), keeping its culture layer as it is.
///
/// **Runs directly after [`advance_band_independence`]** in the Population chain, so every band's
/// people is final for the turn. A queued band whose people is no longer the one culture judged
/// (independence, a defection or a remnant flip moved it) is skipped, and so is one whose people
/// has no other resident band left. The band's [`HeartReading`] is written here as a sole-band
/// heart's, because independence has already rebuilt the turn's readings.
#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn advance_culture_splits(
    sim_config: Res<SimulationConfig>,
    tick: Res<SimulationTick>,
    mut queue: ResMut<CultureSplitQueue>,
    mut heart_ledger: ResMut<HeartLedger>,
    mut event_log: ResMut<CommandEventLog>,
    mut roster: RosterResources,
    mut seeds: NewPeopleSeeds,
    mut followers: BandFlipFollowers,
    mut cohorts: Query<(Entity, &mut PopulationCohort, Option<&BandId>), With<ResidentBand>>,
) {
    if queue.bands.is_empty() {
        return;
    }
    let now = tick.0;
    let queued = std::mem::take(&mut queue.bands);
    let mut flips: Vec<BandFlip> = Vec::new();
    // `BTreeMap` order: lowest BandId first, so several splits in one turn are deterministic.
    for (band, judged_as) in queued {
        let Some((entity, faction)) = cohorts
            .iter()
            .find(|(_, _, id)| id.is_some_and(|id| *id == band))
            .map(|(entity, cohort, _)| (entity, cohort.faction))
        else {
            continue;
        };
        if faction != judged_as {
            continue;
        }
        let has_sibling = cohorts
            .iter()
            .any(|(other, cohort, _)| other != entity && cohort.faction == faction);
        if !has_sibling {
            continue;
        }
        let (to, group_flips) = break_away_as_new_people(
            &mut roster,
            &mut seeds,
            &mut event_log,
            &mut cohorts,
            sim_config.map_seed,
            now,
            faction,
            &[(entity, band)],
            BreakAwayCause::Culture,
        );
        heart_ledger.readings.insert(
            band,
            HeartReading {
                faction: to,
                cut_off: false,
                bond: FULL_TIE,
                last_contact_turn: Some(now),
            },
        );
        flips.extend(group_flips);
    }
    if !flips.is_empty() {
        follow_the_band_to_its_new_people(&mut followers, &cohorts, flips, sim_config.map_seed);
    }
}
