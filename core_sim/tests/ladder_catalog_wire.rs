//! **THE LADDER CATALOG ON THE WIRE** - `ladderRungs` and `ladderBranches`, once per world, derived
//! from `intensification_ladder.json` and nothing else. They replace the route and deposit catalogs
//! (and give the plant and animal branches one), so these tests read the ENCODED envelope through
//! the accessor chain a client uses and hold every row to the config's own record - a catalog that
//! drifted from the config would otherwise draw a ladder the sim does not run.

use bevy::app::App;
use core_sim::{build_test_app, LadderConfigHandle, SnapshotHistory};
use shadow_scale_flatbuffers::generated::shadow_scale::sim as fb;

fn a_world_with_a_catalog() -> App {
    let mut app = build_test_app();
    app.update();
    app
}

fn encoded(app: &App) -> Vec<u8> {
    let snapshot = app
        .world
        .resource::<SnapshotHistory>()
        .latest_entry()
        .expect("a snapshot was captured")
        .snapshot;
    sim_schema::encode_snapshot_flatbuffer(snapshot.as_ref())
}

/// **EVERY RUNG OF EVERY BRANCH IS PUBLISHED, IN CLIMB ORDER WITHIN ITS BRANCH**, and each row's
/// identity, copy, verb, kind and traits are the config record's own.
#[test]
fn the_catalog_publishes_every_rung_as_the_config_declares_it() {
    let app = a_world_with_a_catalog();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let bytes = encoded(&app);
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let section = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .subsistence()
        .expect("the subsistence section");
    let rows = section
        .ladderRungs()
        .expect("the rung catalog is published");
    assert_eq!(rows.len(), ladder.rungs.len(), "one row per config rung");

    let mut at = 0;
    for branch in ladder.branches_in_config_order() {
        let kind = ladder
            .source_kind(branch)
            .expect("a declared branch has a kind")
            .as_str();
        for rung in ladder.rungs_of(branch) {
            let row = rows.get(at);
            at += 1;
            assert_eq!(row.rungKey(), Some(rung.wire_key().as_str()));
            assert_eq!(row.branch(), Some(branch.as_str()));
            assert_eq!(row.order(), rung.order);
            assert_eq!(row.label(), Some(rung.label.as_str()));
            assert_eq!(row.labelFallback(), Some(rung.label_fallback.as_str()));
            assert_eq!(row.verb(), Some(rung.verb.as_deref().unwrap_or_default()));
            assert_eq!(row.sourceKind(), Some(kind));
            let traits: Vec<&str> = row
                .traits()
                .map(|names| names.iter().collect())
                .unwrap_or_default();
            let expected: Vec<&str> = rung.trait_set().members().map(|t| t.as_str()).collect();
            assert_eq!(traits, expected, "{} traits", rung.wire_key());
            assert_eq!(
                row.requiresRung(),
                Some(rung.requires_rung_wire_key().unwrap_or_default().as_str())
            );
            assert_eq!(
                row.earnsKnowledge(),
                Some(rung.earns_knowledge.as_deref().unwrap_or_default())
            );
        }
    }
    assert_eq!(at, rows.len());
}

/// **THE PAYOFF BLOCKS RIDE THEIR OWN KIND AND NEUTRAL ELSEWHERE** - a road rung carries its
/// friction and link reach, a deposit rung its take, and a plant rung neither: a defaulted row must
/// never claim to be a road that helps a haul or a rung that stops the ground growing.
#[test]
fn a_rung_carries_only_the_payoff_block_its_kind_declares() {
    let app = a_world_with_a_catalog();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let bytes = encoded(&app);
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let rows = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .subsistence()
        .and_then(|section| section.ladderRungs())
        .expect("the rung catalog is published");
    let mut saw_road = false;
    let mut saw_deposit = false;
    let mut saw_neutral = false;
    for row in rows.iter() {
        let rung = ladder
            .rungs
            .iter()
            .find(|rung| Some(rung.wire_key().as_str()) == row.rungKey())
            .expect("every published row names a config rung");
        if let Some(payoff) = &rung.route_payoff {
            saw_road = true;
            assert_eq!(row.frictionMultiplier(), payoff.friction_multiplier);
            assert_eq!(row.holdsLinkToTiles(), payoff.holds_link_to_tiles);
        } else {
            saw_neutral = true;
            assert_eq!(row.frictionMultiplier(), 1.0, "no road payoff, no help");
            assert_eq!(row.holdsLinkToTiles(), 0);
            assert!(!row.grantsSight());
        }
        if let Some(payoff) = &rung.extraction_payoff {
            saw_deposit = true;
            assert_eq!(row.yieldPerWorkerTurn(), payoff.yield_per_worker_turn);
            assert_eq!(row.recoveryFraction(), payoff.recovery_fraction);
            assert_eq!(row.regrowthMultiplier(), payoff.regrowth_multiplier);
        } else {
            assert_eq!(row.yieldPerWorkerTurn(), 0.0);
            assert_eq!(row.recoveryFraction(), 0.0);
            assert_eq!(row.regrowthMultiplier(), 1.0, "the ground grows as it did");
        }
    }
    assert!(
        saw_road && saw_deposit && saw_neutral,
        "the shipped ladder has road, deposit and plain rungs, or this proves nothing"
    );
}

/// **A RUNG'S GATE PROPERTIES RIDE THE CATALOG AS THE CONFIG DECLARES THEM**, so a client reads
/// `requiresHeldRungBelow` / `requiresCrop` / `requiresWorkRoom` / `requiresOwnership` /
/// `placesSource` off the row instead of guessing them from an id. Every row matches its record, and
/// each flag is set on at least one shipped rung and clear on at least one other - a catalog that
/// published a constant would pass either half alone.
#[test]
fn a_rung_publishes_its_gate_properties_as_configured() {
    let app = a_world_with_a_catalog();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let bytes = encoded(&app);
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let rows = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .subsistence()
        .and_then(|section| section.ladderRungs())
        .expect("the rung catalog is published");
    // [set somewhere, clear somewhere] per flag, in the order the flags are asserted below.
    let mut seen = [[false; 2]; 5];
    for row in rows.iter() {
        let rung = ladder
            .rungs
            .iter()
            .find(|rung| Some(rung.wire_key().as_str()) == row.rungKey())
            .expect("every published row names a config rung");
        let published = [
            (row.requiresHeldRungBelow(), rung.requires_held_rung_below),
            (row.requiresCrop(), rung.requires_crop),
            (row.requiresWorkRoom(), rung.requires_work_room),
            (row.requiresOwnership(), rung.requires_ownership),
            (row.placesSource(), rung.places_source),
        ];
        for (flag, (wire, configured)) in published.into_iter().enumerate() {
            assert_eq!(wire, configured, "{} flag {flag}", rung.wire_key());
            seen[flag][usize::from(!configured)] = true;
        }
    }
    for (flag, both) in seen.iter().enumerate() {
        assert!(
            both[0] && both[1],
            "flag {flag} must be set on one shipped rung and clear on another"
        );
    }
}

/// **A RUNG'S SITE RULE AND THE LESSONS THAT LOOSEN IT RIDE THE CATALOG** as the config declares
/// them: every row's `siteRule` is the rung's own `site_requirement` (all-clear where it has none),
/// `siteRelaxations` lists its relaxations in order, and the shipped Farm publishes the one
/// `worked_land` relaxation that drops the gathering-site rule and keeps the water rule - the data a
/// client words the "learn Worked Land" hint from.
#[test]
fn a_rung_publishes_its_site_rule_and_the_lessons_that_loosen_it() {
    let app = a_world_with_a_catalog();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let bytes = encoded(&app);
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let rows = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .subsistence()
        .and_then(|section| section.ladderRungs())
        .expect("the rung catalog is published");
    let mut saw_farm = false;
    for row in rows.iter() {
        let rung = ladder
            .rungs
            .iter()
            .find(|rung| Some(rung.wire_key().as_str()) == row.rungKey())
            .expect("every published row names a config rung");
        let rule = row.siteRule().expect("every row carries its site rule");
        let own = rung.site_requirement.as_ref();
        assert_eq!(
            rule.requiresGatheringSite(),
            own.is_some_and(|site| site.requires_gathering_site),
            "{}",
            rung.wire_key()
        );
        assert_eq!(
            rule.minForageCapacity(),
            own.map_or(0.0, |site| site.min_forage_capacity)
        );
        assert_eq!(
            rule.requiresFreshWater(),
            own.is_some_and(|site| site.requires_fresh_water)
        );
        let published = row.siteRelaxations().map_or(0, |rows| rows.len());
        assert_eq!(
            published,
            rung.site_relaxations.len(),
            "{}",
            rung.wire_key()
        );
        if rung.wire_key() == "plant:farm" {
            saw_farm = true;
            let relaxation = row.siteRelaxations().expect("the Farm is relaxed").get(0);
            assert_eq!(relaxation.knowledge(), Some("worked_land"));
            let loosened = relaxation.rule().expect("the relaxation carries its rule");
            assert!(!loosened.requiresGatheringSite());
            assert!(loosened.requiresFreshWater());
            assert!(loosened.minForageCapacity() > rule.minForageCapacity());
        }
    }
    assert!(saw_farm, "the shipped ladder has a plant:farm rung");
}

/// **THE PEN PUBLISHES ITS SIZE TIER AND THE LESSON GAINS** - the data a client words a Ranch from.
#[test]
fn the_pen_publishes_its_size_tiers_and_knowledge_gains() {
    let app = a_world_with_a_catalog();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let bytes = encoded(&app);
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let rows = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .subsistence()
        .and_then(|section| section.ladderRungs())
        .expect("the rung catalog is published");
    let pen = ladder.rung(core_sim::shipped::animal_pen());
    let row = rows
        .iter()
        .find(|row| row.rungKey() == Some(pen.wire_key().as_str()))
        .expect("the pen is published");
    let tier = row.sizeTiers().expect("tiers").get(0);
    let declared = pen.size_tiers.first().expect("the pen declares a tier");
    assert_eq!(tier.id(), Some(declared.id.as_str()));
    assert_eq!(tier.minRings(), declared.min_rings);
    assert_eq!(tier.keeperRatioGain(), declared.keeper_ratio_gain);
    assert_eq!(tier.earnsKnowledge(), declared.earns_knowledge.as_deref());
    let gain = row.knowledgeGains().expect("gains").get(0);
    assert_eq!(gain.knowledge(), Some("selective_breeding"));
    assert_eq!(gain.regrowthGain(), pen.knowledge_gains[0].regrowth_gain);
}

/// **THE BRANCHES ARE PUBLISHED IN THE CATALOG'S OWN ORDER** with their area, kind and heading.
#[test]
fn the_branches_are_published_beside_the_rungs() {
    let app = a_world_with_a_catalog();
    let ladder = app.world.resource::<LadderConfigHandle>().get();
    let bytes = encoded(&app);
    let envelope = fb::root_as_envelope(bytes.as_ref()).expect("a valid envelope");
    let section = envelope
        .payload_as_snapshot()
        .expect("a snapshot")
        .subsistence()
        .expect("the subsistence section");
    let rows = section
        .ladderBranches()
        .expect("the branch list is published");
    let declared = ladder.branches_in_config_order();
    assert_eq!(rows.len(), declared.len());
    for (row, branch) in rows.iter().zip(declared) {
        let def = ladder.branch_def(branch).expect("a declared branch");
        assert_eq!(row.branch(), Some(branch.as_str()));
        assert_eq!(row.area(), Some(def.area.as_str()));
        assert_eq!(row.sourceKind(), Some(def.source.as_str()));
        assert_eq!(row.label(), Some(def.label.as_str()));
    }
}
