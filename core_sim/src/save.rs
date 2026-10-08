//! **A world as bytes, and back into a process where no worldgen has run.**
//!
//! [`crate::sim_state::restore_sim_state`] restores into the *same live `World`*, which still holds
//! the map worldgen built — the rasters, the province assignment, the curated gathering sites. A
//! save file has no such world underneath it, and `.claude/rules/core_sim/checkpoints.md` records
//! that the world-static bucket's reason carries exactly that expiry. This module collects it.
//!
//! ## The blob
//!
//! ```text
//! [ SAVE_MAGIC : 8 bytes ][ SaveHeader : one CBOR document ][ gzip( SavePayload : one CBOR doc ) ]
//! ```
//!
//! Three parts in that order so a **slot list can be built without decoding the payload**: the menu
//! reads the magic, then one small header carrying the turn number, the campaign label and the world
//! identity. Paying a full world decode per row to render a list is the thing the split exists to
//! avoid, and `ciborium` reads exactly one document from a reader, so the two documents cost nothing
//! to separate.
//!
//! **Only the payload is compressed**, which is the same split for the same reason: a header the
//! listing has to inflate before reading is a header the listing pays a decompressor for. Measured
//! on a 160x104 world, the payload goes 20,766,409 -> 1,257,874 bytes at
//! [`PAYLOAD_COMPRESSION_LEVEL`] — CBOR writes a field-name string per field per tile, so the
//! redundancy is enormous and gzip finds essentially all of it.
//!
//! ## Version mismatch is a refusal, not an attempt
//!
//! The repo ships no back-compat, so there is no migration code here **on purpose**. The point of
//! [`SAVE_FORMAT_VERSION`] is that a stale save is *rejected by a typed error naming both versions*
//! rather than fed to a decoder that will mis-read it into a plausible wrong world. The version is
//! checked before the payload is looked at.
//!
//! ## What is saved, and what is not
//!
//! Three different treatments, because "the world-static resources" is not one kind of thing:
//!
//! | Treatment | Resources | Why |
//! |---|---|---|
//! | **Saved** | [`ElevationField`], [`MoistureRaster`], [`HydrologyState`], [`ProvinceMap`], [`FoodSiteRegistry`], [`FoodSiteWaterBiasReport`], [`StartLocation`], [`WorldGenSeed`] | Ground truth that nothing can recompute — re-running worldgen would produce a *different map* if any tuning moved |
//! | **Rebuilt from the restored entities** | `TileRegistry`, `PowerTopology` | Both were `Entity`-bearing; a handle cannot cross a process. `restore_sim_state` already rebuilds the registry in its pass 4a |
//! | **Re-derived** | `BiomePalette` | A pure function of (preset, world seed, tile count), all three of which the save carries |
//! | **Re-resolved from live config by id** | [`StartProfileLookup`], `ActiveStartProfile`, `CampaignLabel`, `GreatDiscoveryRegistry` | Config in disguise. Saving them would reinstall the tuning that was live at capture, which is the second construction rule. `StartProfileLookup` is the *id itself*, and it rides in [`SaveHeader`] (`world.start_profile_id`) because a slot row needs it without a payload — so a copy in the payload would be a second authority for one string |
//!
//! `GenerationRegistry` and `GreatDiscoveryRegistry` need no work at all: `build_headless_app` fills
//! both from live config before any world exists, so a freshly built app already has them.

use std::io::{BufWriter, Cursor, Read};

use bevy::prelude::*;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    biome_palette::BiomePalette,
    config_fingerprint::ConfigFingerprint,
    heightfield::ElevationField,
    hydrology::HydrologyState,
    map_preset::MapPresetsHandle,
    mapgen::WorldGenSeed,
    orders::{FactionRegistry, TurnQueue},
    power::PowerTopology,
    provinces::ProvinceMap,
    resources::{
        FoodSiteRegistry, FoodSiteWaterBiasReport, MoistureRaster, SimulationConfig, StartLocation,
        TileRegistry,
    },
    sim_state::{capture_sim_state, restore_sim_state, SimState},
    start_profile::{
        resolve_active_profile, ActiveStartProfile, CampaignLabel, StartProfileLookup,
        StartProfileOverrides, StartProfilesHandle,
    },
};

/// Leading bytes of every save, checked before anything is decoded.
///
/// A fixed constant rather than a hash of anything: its only job is to answer *"is this one of
/// ours"* for a file the player picked, so that a JPEG produces "not a save" rather than a CBOR
/// parse error from somewhere in the middle of a world.
pub const SAVE_MAGIC: [u8; 8] = *b"SHDWSAV\x01";

/// Bumped whenever the encoded shape of [`SaveHeader`] or [`SavePayload`] changes.
///
/// There is no migration path by design — see the module note. A save from a different version is
/// refused with [`SaveError::VersionMismatch`].
///
/// **A field added to `SimState` is a shape change and must bump this**, which is the case that
/// actually arises: `SimState` gained `crisis_overlay` and the old blobs stopped being decodable.
/// Without a bump, `ciborium` fails on the missing field somewhere inside a world and the player is
/// told their save is `unreadable` — a sentence that describes corruption rather than the truth,
/// which is that this build reads a newer format. The bump is what turns that into an error naming
/// both versions.
///
/// | version | shape |
/// |---|---|
/// | 1 | the initial format |
/// | 2 | `SimState.crisis_overlay` added — a load published an empty crisis heatmap |
/// | 3 | `WorldStatics.start_profile` removed — written into every payload and never read back |
/// | 4 | the transfer counters on `PopulationCohort` and `LaborAllocation` became `TransferLedger` structs, and each gained a fodder twin — two `f32` totals per account replaced by four magnitudes split by link |
/// | 5 | `SimState.starting_loadout` became a **map of per-band windows** — one `open`/budget triple replaced by a `BandId`-keyed table carrying each band's supply and its standing take |
/// | 6 | `WorldStatics.factions` (`FactionRegistry`) gained `control` — the roster now carries how each faction is driven, and that map has no serde default, so without the bump a version-5 blob dies on the missing field inside the decoder and reads as `unreadable` rather than as the wrong version |
/// | 7 | `WorldStatics.start_location` (`StartLocation`) became a **per-faction map** — one `Option<UVec2>` replaced by a `FactionId`-keyed table, because worldgen now places every registered faction at its own start, and a rival founding a settlement must not move your marker |
/// | 8 | `SimState.victory` (`VictoryState.modes`) became a **per-faction map** — one mode list replaced by a `FactionId`-keyed table, because a victory threshold measures one people and the world's totals were scoring everybody's; `SimulationMetrics` gained its per-faction twins in the same change |
/// | 9 | `LaborAllocation` gained `last_pool_toe` — the five standing pools' settled tables of equipment, which the wire publishes as `PopulationCohortState.poolToe`. A band's whole allocation rides `BandRecord::labor`, so a version-8 blob has no such field and a bincode decode of one runs off the end of the record rather than reporting a version |
/// | 10 | `LaborAllocation` gained `last_pool_crew` — the four keeping pools' crew accounts, each a pair (`keepers` the turn settled the pool at, and how many of them its bill did not consume), which the wire publishes as `PopulationCohortState.poolCrew`. A band's whole allocation rides `BandRecord::labor`, so a version-9 blob has no such field and a decode of one runs off the end of the record rather than reporting a version |
/// | 11 | `BandBench` gained `last_started` — which recipe the band last started for each thing it makes, the fact the crafting ledger suggests from when an item has several recipes. The whole bench rides `BandRecord::bench`, so a version-10 blob has no such field |
/// | 12 | `LaborAllocation` gained `last_transfer_crossings` and `PopulationCohort` its per-turn twin `last_turn_transfer_crossings` — the cause-keyed crossings list beneath the transfer ledgers (issue #731), which the wire publishes as `PopulationCohortState.transferCrossings`. Both ride `BandRecord`, so a version-11 blob has no such field. In the same version `ExpeditionMission::Trade` gained `destination_faction` (it rides `ExpeditionRecord`), which a returned shipment's `TransferCause::ShipmentReturned` row names |
/// | 13 | `BandRecord` gained `supply` — the band's supply-network id, its own pooling links and its network's span, which the capture publishes as `PopulationCohortState.supplyNetworkId` / `poolingLinks` / `supplyNetworkSpanTiles`. The live resource is keyed by `Entity`, so it rides each band's record; a version-12 blob has no such field |
/// | 14 | Defection (#512): `SimState` gained `border_policies` (`FactionBorderPolicies`, each people's Open Borders setting) and `faction_names` (`FactionNames`, every faction's minted name — the save wins, so a pool edit never renames a saved world's factions), `Expedition` gained `defection_pull` (it rides `ExpeditionRecord`), and `PopulationCohort` lost `migration` with the whole-band knowledge migration it queued and gained `last_food_need` (what the meal was measured against, beside what was eaten). All three move `BandRecord`/`ExpeditionRecord`/`SimState`, so a version-13 blob does not decode |
/// | 15 | Site crews (#751): `BuildQueueEntry` gained `priority` (the Build mark), `LaborAllocation` lost `last_quarrywork_demand` / `last_quarrywork_supplied` with the retired `quarrywork` pool and gained `last_keeping_issued` (the site crews' keeping-tool issues), `LaborTarget` lost `Agriculture` / `Husbandry` / `Quarrywork`, `ForagePatch` / `Herd` / `DepositSource` gained `upkeep_hands` and `upkeep_toe` (each people's keeping-tool lines), `PublishedBuildLeg` gained `starts_after` / `work_per_turn` and `DepositSource` gained `build_pace` (the keeping forecast's pace), and `ForagePatch` / `Road` / `DepositSource` gained `peak_rung` (the highest rung achieved since the position last stood at its base, which gates the loss Alert). All of them ride `BandRecord` or the registries in `SimState`, so a version-14 blob does not decode |
/// | 16 | `BandBench` gained `finished` — equipment the bench completed, parked until the top of the next turn stocks it into `BandEquipment` (issue #720). The whole bench rides `BandRecord::bench`, so a version-15 blob has no such field |
/// | 17 | `PopulationCohort` gained `founding_lines` — the set of unrelated families the band descends from (issue #687, `crate::lineage`), which the wire publishes as `PopulationCohortState.foundingLines`. The cohort rides `BandRecord`, so a version-16 blob has no such field |
/// | 18 | Food spoilage (#706): `LocalStore` holds food **by keeping class** in its own `food` map (`FoodMix`) instead of a `provisions` scalar among its goods, `PopulationCohort` gained `last_food_spoiled` (the turn's rot, the ledger identity's `spoiled` term), `WorkParty` / `Walker` gained `load_classes` / `classes` (what a caravan's load and each pack are made of) and `WorkParty` its forecast's `spoiled_rate_home` / `transit_keeps_turns`, and `LaborAllocation` gained `homeward` (stood-down parties still walking home, which outlive their rows); `WorkParty` gained `load_goods` and `fodder_rate_home` / `materials_rate_home`, and `Walker` `bulk` / `goods` (a far forage pack's fodder and materials). Every band store, expedition cargo and posting rides `BandRecord` / `ExpeditionRecord`, so a version-17 blob does not decode |
/// | 19 | `CommandEventEntry` gained `band` — the one band an event is about (`0` = none), which the wire publishes as `CommandEventState.band`; a refused `set_starting_loadout` names its band there (issue #723). The event log rides `SimState.command_events`, so a version-18 blob has no such field |
/// | 20 | `SimState` gained `belief` — the `BeliefRegistry`, belief on every place (issue #697, `crate::belief`). A version-19 blob has no such field |
/// | 21 | Band carry (#732): `LoadoutSupply::Grant` lost `kit_budget` / `material_budget` for one optional `carry_budget` (a load in `CarryLoad` units; `None` = the band's own carry, read live), `LoadoutSupply::Parent` gained the take's `carry_budget`, `LoadoutWindow` gained `dowry` (a splinter's food share and what has crossed), and `LaborAllocation` gained `last_food_left_behind` — the food ledger's long-move term. |
/// | 22 | The bench became a queue (#776): `BandBench` lost `recipe_id` / `progress` / `drawn` / `items_completed` for `orders` — an ordered list of `BenchOrder { recipe_id, count, made, progress, drawn }`, each carrying its own pass in flight — `KeepingIssue` gained `required` (a site crew's claim beside its issue, which the craft suggestions read), and `BandTravel` gained `departed` (whether a move order has taken its first step, where a long move sheds). All ride `BandRecord`, so a version-21 blob does not decode |
/// | 23 | The breeding ceiling (#688): `FertilityFactors` gained `ceiling` (the fourth fertility factor) and `PopulationCohort` gained `last_breeding` (the band's breeding population and its ceiling, `crate::lineage::BreedingReading`), which the wire publishes as `PopulationCohortState.fertilityCeiling` / `breedingPopulation` / `breedingCeiling`. The cohort rides `BandRecord`, so a version-22 blob has no such fields |
/// | 24 | `PopulationCohort` gained `belief_anchor` — the one belief tile a band remembers as its ancestors' place, which the culture morale term reads (issue #699, `.claude/rules/core_sim/belief.md` → "The culture morale term"). The cohort rides `BandRecord`, so a version-23 blob has no such field |
/// | 25 | `PopulationCohort` gained `last_belief_relay_hops` — the kin-relay hop count the culture term was priced from this turn (issue #699), which the wire publishes as `PopulationCohortState.beliefRelayHops`. The cohort rides `BandRecord`, so a version-24 blob has no such field |
/// | 26 | Independence (#284): `FactionRegistry` moved from `WorldStatics.factions` to `SimState.factions` — the roster grows mid-game when a cut-off group of bands becomes its own people, so it rewinds with the turn — and `SimState` gained `hearts` (`HeartLedger`, every band's standing toward its people's heart). `CommandEventKind` gained `BandBrokeAway` and `LostTouch`, appended. |
/// | 27 | Food rots at the end of its shelf life (#706): `FoodMix` holds each keeping class as age-stamped `FoodBatch` lots (`class -> [{age, amount}]`, oldest first) instead of one amount per class, so every band's larder, every party pack and every in-flight cargo serializes its lots. |
pub const SAVE_FORMAT_VERSION: u32 = 27;

/// gzip level for the payload document.
///
/// **6, not 9.** Measured on a 160x104 world: level 6 takes the payload to 1,257,874 bytes and
/// level 9 to 1,221,708 — 2.9% smaller for materially more CPU, on a blob the autosave hook rewrites
/// on a cadence. The size that mattered was the 16.5x, and level 6 has all of it.
pub const PAYLOAD_COMPRESSION_LEVEL: u32 = 6;

/// How much CBOR to accumulate before handing it to the deflater. See
/// [`append_compressed_payload`] for the 57x this is worth.
const PAYLOAD_WRITE_BUFFER_BYTES: usize = 64 * 1024;

/// Which world this is, so a loader can say what it is about to open — and so a save cannot be
/// silently opened against a build whose map would come out different.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldIdentity {
    pub world_seed: u64,
    pub map_preset_id: String,
    pub width: u32,
    pub height: u32,
    pub start_profile_id: String,
}

/// Everything needed to render a save-slot row, plus the version and fingerprint gates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveHeader {
    pub format_version: u32,
    pub world: WorldIdentity,
    /// The tick the payload holds — "the world at tick N" in the sense
    /// `.claude/rules/core_sim/checkpoints.md` fixes: immediately after the Nth turn resolved.
    pub turn: u64,
    /// The campaign's display title, **as a string rather than a `CampaignLabel`**. The label is
    /// re-resolved from `start_profile_id` on load (it is config), but a slot list has to draw a row
    /// without loading anything, and for that it needs the text that was on screen when the save was
    /// written.
    pub campaign_title: String,
    /// The tuning this world booted on, per config file. Stored, not compared — a load-time warning
    /// needs a load path to warn from.
    pub config_fingerprint: ConfigFingerprint,
}

/// The map worldgen built, as ground truth rather than as a recipe.
///
/// **Re-running worldgen on load would be the bug**, not the saving of this: worldgen is a function
/// of config as well as seed, so a preset edited between the save and the load would silently
/// regenerate a *different map* under a population that remembers the old one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldStatics {
    pub elevation: ElevationField,
    pub moisture: MoistureRaster,
    pub hydrology: HydrologyState,
    pub provinces: ProvinceMap,
    pub food_sites: FoodSiteRegistry,
    pub food_site_water_bias: FoodSiteWaterBiasReport,
    pub start_location: StartLocation,
    pub world_seed: WorldGenSeed,
}

/// The world itself: the checkpoint, plus the ground it stands on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavePayload {
    pub sim: SimState,
    pub statics: WorldStatics,
}

/// Why a blob is not a world.
#[derive(Debug, Error)]
pub enum SaveError {
    #[error("not a Shadow-Scale save: expected magic {expected:02x?}, found {found:02x?}")]
    BadMagic { expected: [u8; 8], found: Vec<u8> },
    #[error(
        "save is format version {found}, this build reads version {expected}; there is no \
         migration path — start a new campaign or use a matching build"
    )]
    VersionMismatch { expected: u32, found: u32 },
    #[error("the save header could not be decoded: {0}")]
    Header(#[source] ciborium::de::Error<std::io::Error>),
    #[error("the save payload could not be decoded: {0}")]
    Payload(#[source] ciborium::de::Error<std::io::Error>),
    #[error("the save could not be encoded: {0}")]
    Encode(#[source] ciborium::ser::Error<std::io::Error>),
    #[error("the save payload could not be decompressed: {0}")]
    Decompress(#[source] std::io::Error),
    #[error("the save payload could not be compressed: {0}")]
    Compress(#[source] std::io::Error),
}

/// Set on a world that is about to be loaded into, so `Startup`'s worldgen chain does not run.
///
/// **Absence means "generate a world"**, which is why the run condition below takes an `Option` —
/// every existing caller of `build_headless_app` keeps generating one, and nothing about the normal
/// path changes. This is the same shape as `sim_state::Replaying`: a flag whose only job is to make
/// one scheduled thing not happen.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SuppressWorldgen;

/// Run condition on the `Startup` worldgen chain: true unless a load suppressed it.
pub fn worldgen_wanted(suppressed: Option<Res<SuppressWorldgen>>) -> bool {
    suppressed.is_none()
}

/// Read the map worldgen built out of a live world.
pub fn capture_world_statics(world: &World) -> WorldStatics {
    WorldStatics {
        elevation: world.resource::<ElevationField>().clone(),
        moisture: world.resource::<MoistureRaster>().clone(),
        hydrology: world.resource::<HydrologyState>().clone(),
        provinces: world.resource::<ProvinceMap>().clone(),
        food_sites: world.resource::<FoodSiteRegistry>().clone(),
        food_site_water_bias: world.resource::<FoodSiteWaterBiasReport>().clone(),
        start_location: world.resource::<StartLocation>().clone(),
        world_seed: *world.resource::<WorldGenSeed>(),
    }
}

/// Build the header for a live world.
fn capture_header(world: &World, sim: &SimState) -> SaveHeader {
    let config = world.resource::<SimulationConfig>();
    let label = world.resource::<CampaignLabel>();
    SaveHeader {
        format_version: SAVE_FORMAT_VERSION,
        world: WorldIdentity {
            world_seed: world.resource::<WorldGenSeed>().0,
            map_preset_id: config.map_preset_id.clone(),
            width: config.grid_size.x,
            height: config.grid_size.y,
            start_profile_id: world.resource::<StartProfileLookup>().id.clone(),
        },
        turn: sim.tick.0,
        campaign_title: label
            .title
            .text_as_str()
            .unwrap_or(&label.profile_id)
            .to_string(),
        config_fingerprint: world.resource::<ConfigFingerprint>().clone(),
    }
}

/// Encode a live world as a save blob.
pub fn encode_save(world: &World) -> Result<Vec<u8>, SaveError> {
    let sim = capture_sim_state(world);
    let header = capture_header(world, &sim);
    let payload = SavePayload {
        sim,
        statics: capture_world_statics(world),
    };

    let mut bytes = Vec::from(SAVE_MAGIC);
    ciborium::into_writer(&header, &mut bytes).map_err(SaveError::Encode)?;
    append_compressed_payload(&mut bytes, &payload)?;
    Ok(bytes)
}

/// Encode the payload and gzip it onto the end of `bytes`.
///
/// **The `BufWriter` is not optional.** `ciborium` writes in very small pieces — a byte for most
/// headers, then the item — and `GzEncoder` deflates on every `write` call it receives. Streaming
/// CBOR straight into the encoder made a 160x104 save take **1083 ms**, against 19 ms to encode the
/// checkpoint and 86 ms to gzip it; almost all of that was per-call deflate overhead rather than any
/// work on the data. Buffering into `PAYLOAD_WRITE_BUFFER_BYTES` chunks removes it.
fn append_compressed_payload(bytes: &mut Vec<u8>, payload: &SavePayload) -> Result<(), SaveError> {
    let encoder = GzEncoder::new(Vec::new(), Compression::new(PAYLOAD_COMPRESSION_LEVEL));
    let mut buffered = BufWriter::with_capacity(PAYLOAD_WRITE_BUFFER_BYTES, encoder);
    ciborium::into_writer(payload, &mut buffered).map_err(SaveError::Encode)?;
    let encoder = buffered
        .into_inner()
        .map_err(|err| SaveError::Compress(err.into_error()))?;
    let compressed = encoder.finish().map_err(SaveError::Compress)?;
    bytes.extend_from_slice(&compressed);
    Ok(())
}

/// Check the magic and hand back everything after it.
fn strip_magic(bytes: &[u8]) -> Result<&[u8], SaveError> {
    if bytes.len() < SAVE_MAGIC.len() || bytes[..SAVE_MAGIC.len()] != SAVE_MAGIC {
        return Err(SaveError::BadMagic {
            expected: SAVE_MAGIC,
            found: bytes[..bytes.len().min(SAVE_MAGIC.len())].to_vec(),
        });
    }
    Ok(&bytes[SAVE_MAGIC.len()..])
}

/// **Read the header alone** — what a slot list calls, once per file, without touching the payload.
pub fn read_save_header(bytes: &[u8]) -> Result<SaveHeader, SaveError> {
    let mut cursor = Cursor::new(strip_magic(bytes)?);
    let header: SaveHeader = ciborium::from_reader(&mut cursor).map_err(SaveError::Header)?;
    check_version(&header)?;
    Ok(header)
}

/// The version gate. Separate from decoding so it can run **before** the payload is read.
fn check_version(header: &SaveHeader) -> Result<(), SaveError> {
    if header.format_version != SAVE_FORMAT_VERSION {
        return Err(SaveError::VersionMismatch {
            expected: SAVE_FORMAT_VERSION,
            found: header.format_version,
        });
    }
    Ok(())
}

/// Decode a whole save. The version is checked before the payload is looked at.
pub fn decode_save(bytes: &[u8]) -> Result<(SaveHeader, SavePayload), SaveError> {
    let body = strip_magic(bytes)?;
    let mut cursor = Cursor::new(body);
    let header: SaveHeader = ciborium::from_reader(&mut cursor).map_err(SaveError::Header)?;
    check_version(&header)?;

    // Everything the header did not consume is the gzipped payload document. Taking the cursor's
    // position is what keeps the two documents' boundary a fact about the encoding rather than a
    // length field that could disagree with it.
    let payload_start = cursor.position() as usize;
    let mut raw = Vec::new();
    GzDecoder::new(&body[payload_start..])
        .read_to_end(&mut raw)
        .map_err(SaveError::Decompress)?;
    let payload: SavePayload = ciborium::from_reader(raw.as_slice()).map_err(SaveError::Payload)?;
    Ok((header, payload))
}

/// Put a decoded save into a world that has never run worldgen.
///
/// ## Ordering, and why it is forced
///
/// 1. **`SimulationConfig`** — everything below reads the grid size from it, including
///    `restore_sim_state`'s `TileRegistry` pass.
/// 2. **The start profile**, re-resolved from the saved id against the catalog live *now*. This is
///    the "no config crosses a save" rule: the profile's contents are tuning, its id is not.
/// 3. **The world statics**, so the ground exists before anything stands on it.
/// 4. **`BiomePalette`**, re-derived from the preset, the saved seed and the tile count.
/// 5. **`restore_sim_state`**, which spawns the tiles, bands and settlements and rebuilds
///    `TileRegistry` from the entities it just created.
/// 6. **`PowerTopology`**, which is sized from the tiles pass 5 spawned.
pub fn apply_save(world: &mut World, header: &SaveHeader, payload: &SavePayload) {
    // --- 1: the config the rest of the load reads --------------------------------------------
    {
        let mut config = world.resource_mut::<SimulationConfig>();
        config.grid_size = UVec2::new(header.world.width, header.world.height);
        config.map_seed = header.world.world_seed;
        config.map_preset_id = header.world.map_preset_id.clone();
        config.start_profile_id = header.world.start_profile_id.clone();
    }

    // --- 2: the start profile, by id, from live config ----------------------------------------
    let profiles = world.resource::<StartProfilesHandle>().clone();
    let (profile, used_fallback) =
        resolve_active_profile(&profiles, &header.world.start_profile_id);
    if used_fallback {
        warn!(
            target: "shadow_scale::save",
            requested = %header.world.start_profile_id,
            fallback = %profile.id,
            "save.load.start_profile_missing"
        );
    }
    world
        .resource_mut::<SimulationConfig>()
        .start_profile_overrides = StartProfileOverrides::from_profile(&profile);
    world.insert_resource(CampaignLabel::from_profile(&profile));
    world.insert_resource(StartProfileLookup::new(profile.id.clone()));
    world.insert_resource(ActiveStartProfile::new(profile));

    // --- 3: the ground ------------------------------------------------------------------------
    let statics = &payload.statics;
    world.insert_resource(statics.elevation.clone());
    world.insert_resource(statics.moisture.clone());
    world.insert_resource(statics.hydrology.clone());
    world.insert_resource(statics.provinces.clone());
    world.insert_resource(statics.food_sites.clone());
    world.insert_resource(statics.food_site_water_bias.clone());
    world.insert_resource(statics.start_location.clone());
    world.insert_resource(statics.world_seed);

    // --- 4: the palette, re-derived rather than carried ---------------------------------------
    let tile_count = (header.world.width * header.world.height).max(1);
    let presets = world.resource::<MapPresetsHandle>().get();
    if let Some(preset) = presets.get(&header.world.map_preset_id) {
        world.insert_resource(BiomePalette::build(
            preset,
            statics.world_seed.0,
            tile_count,
        ));
    } else {
        // A preset-less map keeps worldgen's own unrestricted behaviour, which is what the absence
        // of the resource means — the palette clamp reads it as an `Option`.
        warn!(
            target: "shadow_scale::save",
            preset = %header.world.map_preset_id,
            "save.load.map_preset_missing"
        );
    }

    // --- 5: the checkpoint --------------------------------------------------------------------
    restore_sim_state(world, &payload.sim);
    // ⛔ **The queue is rebuilt from the RESTORED roster, here rather than in the caller.** The
    // roster is checkpoint state (`SimState::factions` — it grows when a people breaks away), so
    // this follows the restore. `TurnQueue` is server-side order intake, so it is not checkpoint
    // state and no payload carries it — but the app a load is applied into was built from whatever
    // start profile the *file* named, and its queue awaits that profile's factions. A two-faction
    // save opened on a one-faction profile would then resolve turns without ever awaiting faction
    // 1. The rollback path rebuilds it from the registry for the same reason; putting it beside the
    // registry restore is what keeps the next caller of `apply_save` from having to remember.
    let factions = world.resource::<FactionRegistry>().factions().to_vec();
    world.insert_resource(TurnQueue::new(factions));

    // --- 6: the power grid's adjacency, sized from the tiles pass 5 spawned --------------------
    let node_count = world.resource::<TileRegistry>().tiles.len();
    let capacity = world.resource::<SimulationConfig>().power_line_capacity;
    world.insert_resource(PowerTopology::from_grid(
        node_count,
        header.world.width,
        header.world.height,
        capacity,
    ));
}

/// Decode a blob and build the app it describes — **without running worldgen**.
///
/// The returned `App` has not been `update()`d, so its `Startup` schedule has not run yet. When the
/// caller does update it, [`SuppressWorldgen`] keeps the generation chain from overwriting the map
/// this function just installed, and the turn schedule runs as normal.
pub fn load_save(bytes: &[u8]) -> Result<(App, SaveHeader), SaveError> {
    let (header, payload) = decode_save(bytes)?;
    let mut app = crate::build_headless_app();
    app.insert_resource(SuppressWorldgen);
    apply_save(&mut app.world, &header, &payload);
    Ok((app, header))
}
