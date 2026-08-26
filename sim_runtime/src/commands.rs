use std::convert::TryFrom;

use prost::Message;
use thiserror::Error;

mod proto {
    include!(concat!(env!("OUT_DIR"), "/shadow_scale.commands.rs"));
}

use proto as pb;

/// High-level representation of a command envelope.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandEnvelope {
    pub payload: CommandPayload,
    pub correlation_id: Option<u64>,
}

/// Supported command payloads.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum CommandPayload {
    Turn {
        steps: u32,
    },
    ResetMap {
        width: u32,
        height: u32,
    },
    Orders {
        faction_id: u32,
        directive: OrdersDirective,
    },
    Rollback {
        tick: u64,
    },
    UpdateEspionageGenerators {
        updates: Vec<EspionageGeneratorUpdate>,
    },
    QueueEspionageMission {
        mission_id: String,
        owner_faction: u32,
        target_owner_faction: u32,
        discovery_id: u32,
        agent_handle: u32,
        target_tier: Option<u8>,
        scheduled_tick: Option<u64>,
    },
    UpdateEspionageQueueDefaults {
        scheduled_tick_offset: Option<u32>,
        target_tier: Option<u8>,
    },
    UpdateCounterIntelPolicy {
        faction: u32,
        policy: SecurityPolicyKind,
    },
    AdjustCounterIntelBudget {
        faction: u32,
        reserve: Option<f32>,
        delta: Option<f32>,
    },
    ReloadConfig {
        kind: ReloadConfigKind,
        path: Option<String>,
    },
    SetCrisisAutoSeed {
        enabled: bool,
    },
    SetFogEnabled {
        enabled: bool,
    },
    /// Republish the world as a FULL snapshot. Client-initiated recovery for delta streaming —
    /// see `ResyncCommand` in `command.proto`.
    Resync,
    SpawnCrisis {
        faction_id: u32,
        archetype_id: String,
    },
    SetStartProfile {
        profile_id: String,
    },
    ScoutArea {
        faction_id: u32,
        target_x: u32,
        target_y: u32,
        band_id: Option<u64>,
    },
    FollowHerd {
        faction_id: u32,
        herd_id: String,
        policy: Option<String>,
        band_id: Option<u64>,
    },
    FoundSettlement {
        faction_id: u32,
        target_x: u32,
        target_y: u32,
    },
    ForageTile {
        faction_id: u32,
        target_x: u32,
        target_y: u32,
        module: String,
        band_id: Option<u64>,
    },
    HuntGame {
        faction_id: u32,
        target_x: u32,
        target_y: u32,
        band_id: Option<u64>,
    },
    HuntFauna {
        faction_id: u32,
        herd_id: String,
        band_id: Option<u64>,
    },
    /// **DECLARE a Tame on this herd** — it appends an entry to the build queue of every band
    /// hunting it and **names no crew** (`docs/plan_standing_upkeep.md` §2.5).
    ///
    /// The hands are the band-level `builders` role (`assign_labor <faction> <band> builders <n>`),
    /// whose whole output goes on the **head** of that queue. The four rung verbs and `extend_pen`
    /// all lost their trailing `<workers>` together, and with it the affordability refusal that
    /// existed only for them: there is no number left to refuse.
    Tame {
        faction_id: u32,
        herd_id: String,
    },
    Cultivate {
        faction_id: u32,
        target_x: u32,
        target_y: u32,
    },
    Sow {
        faction_id: u32,
        target_x: u32,
        target_y: u32,
    },
    Corral {
        faction_id: u32,
        target_x: u32,
        target_y: u32,
    },
    // **RETIRED: `AbandonImprovement`** — "clear the build verb off every band working this source".
    //
    // The build verb is **derived from the meter** now (`forage::patch_build_verb` /
    // `fauna::herd_build_verb`, `docs/plan_standing_upkeep.md` §2.4): a source with progress on a
    // meter is building that rung, and the stored verb is only a declaration for a meter at **zero**.
    // A command that cleared a derived value would either do nothing or fight the derivation, and
    // both are worse than not having it. **The undo is its own verb now**: [`CommandPayload::Unqueue`]
    // withdraws the declaration and [`CommandPayload::Abandon`] puts the whole holding down
    // (`docs/plan_standing_upkeep.md` §2.5). Proto field 46 is reserved and was never reused.
    /// **PUT A SOURCE DOWN** — drop the band's *holding* of it: the assignment row **and** its
    /// build-queue entry, on every band of the faction working it
    /// (`docs/plan_standing_upkeep.md` §2.5).
    ///
    /// **The meters are untouched.** The ground keeps what is on it and, with nobody holding it,
    /// rots back down at the rung's own rate exactly as an unkept improvement already does — so
    /// nothing is destroyed on the spot and it needs no confirmation.
    ///
    /// **One bit per source, never a number.** It is disposal rather than a smaller share; the
    /// per-source *funding* lever stays deleted.
    Abandon {
        faction_id: u32,
        /// The source: `Some` tile coordinates for a patch, or [`Self::Abandon::herd_id`] for a
        /// herd. Exactly one form is filled.
        target_x: Option<u32>,
        target_y: Option<u32>,
        herd_id: Option<String>,
    },
    /// **WITHDRAW A DECLARATION** — drop the source's build-queue entry only, leaving the row, its
    /// take crew, its kit and the meter exactly as they are.
    ///
    /// It is the undo a declaration never had: `cultivate <f> <x> <y> 0` *set* the improvement with
    /// zero builders rather than clearing it, so an unwanted declaration was stuck. A declaration
    /// carries no crew at all now, so this is the only undo — and [`Self::Abandon`] is how a source
    /// with work already banked on it is put down.
    Unqueue {
        faction_id: u32,
        target_x: Option<u32>,
        target_y: Option<u32>,
        herd_id: Option<String>,
    },
    /// **RE-ORDER ONE BAND'S BUILD QUEUE** — move its entry for the named source to `position`
    /// (0-based, clamped to the queue's length).
    ///
    /// **The queue's defining input.** The whole `builders` pool goes on the head until that entry's
    /// meter fills, so the order *is* the funding decision — and re-ordering is the one input a list
    /// can carry that a stepper cannot (`docs/plan_standing_upkeep.md` §2.5).
    BuildOrder {
        faction_id: u32,
        band_id: u64,
        target_x: Option<u32>,
        target_y: Option<u32>,
        herd_id: Option<String>,
        position: u32,
    },
    /// **NAME THE KIT ONE QUEUED BUILD IS RAISED WITH** — on every band of the faction that has the
    /// source queued (`docs/plan_standing_upkeep.md` §4.7a ②). The row, its take crew and the meter
    /// are untouched; this sets a property of the **queue entry**.
    ///
    /// **The builders' kit is per ENTRY, not per band.** A build's default is derived from that
    /// entry's own food web — a hoe for a Cultivate, hurdles for a `Tame` — so one stored id per band
    /// is the one thing the derivation cannot express: naming a kit on the `builders` labor row
    /// pinned the animal web's tool onto every later plant build with no way back. `assign_labor`
    /// refuses a `kit` token on that role, and this is where the override lives.
    ///
    /// **An absent [`Self::BuildKit::kit_id`] CLEARS the override** back to the derivation — the same
    /// *"an absent `kitId` means the job's default"* rule every other selection follows, and what lets
    /// a client say *"back to default"* with no new vocabulary. An explicit bare-handed kit is a
    /// **real** selection and survives the round trip.
    BuildKit {
        faction_id: u32,
        target_x: Option<u32>,
        target_y: Option<u32>,
        herd_id: Option<String>,
        /// Absent = clear the override; present = this roster kit, the bare one included.
        kit_id: Option<String>,
    },
    /// **NAME THE KIT ONE WORK SITE IS KEPT WITH** — on every band of the faction that works the
    /// source (`docs/plan_standing_upkeep.md` §2.7). The take crew, its own kit, the queue entry and
    /// the meter are untouched; this sets a property of the **worked row**.
    ///
    /// **The keeping kit is per WORK SITE, not per band.** The band is the pool of workers and goods
    /// to draw from; it does not decide which tool a given site is worked with. A single stored id on
    /// the band's `agriculture` / `husbandry` role row — which is where this lived until §2.7 — could
    /// not say *hoes on the Field, bare hands on the scrub patch beside it*. `assign_labor` refuses a
    /// `kit` token on those roles, and this is where the override lives.
    ///
    /// **An absent [`Self::UpkeepKit::kit_id`] CLEARS the override** back to the site's own web
    /// derivation — the same *"an absent `kitId` means the job's default"* rule every other selection
    /// follows. An explicit bare-handed kit is a **real** selection and survives the round trip.
    ///
    /// **A kit that does not serve this site's web is a command FAILURE**, never a silent fall back,
    /// exactly as `build_kit` refuses one whose `jobs` does not list `builders`.
    UpkeepKit {
        faction_id: u32,
        target_x: Option<u32>,
        target_y: Option<u32>,
        herd_id: Option<String>,
        /// Absent = clear the override; present = this roster kit, the bare one included.
        kit_id: Option<String>,
    },
    /// **MARK ONE WORKED ROW WITH THE PLAYER'S OWN RANK** — `high` | `normal` | `low`, on the named
    /// band's assignment for that source (`docs/plan_standing_upkeep.md` §4.9 item 9b).
    ///
    /// **It is a stated value on the row, never a list position.** The band's scarcity handlers read
    /// it as the *outermost* level of their ordering — the shedding walk takes its hand off the
    /// lowest-ranked candidate **within** the step it had already chosen, and a short pen-feed store
    /// serves the high-ranked pens first — so a rank orders candidates and never creates or removes
    /// one. With every row at `normal` the behaviour is exactly what it was.
    ///
    /// The level token is lower-cased before it travels, as `upkeep_mode`'s mode is, and anything the
    /// sim does not know is refused by name rather than guessed at.
    WorkPriority {
        faction_id: u32,
        band_id: u64,
        target_x: Option<u32>,
        target_y: Option<u32>,
        herd_id: Option<String>,
        /// The level token: `"high"`, `"normal"` or `"low"`.
        level: String,
    },
    /// **Say how a band splits a maintenance pool it cannot stretch**
    /// (`docs/plan_standing_upkeep.md` §2.5) — `"spread"` (everything degrades a little) or
    /// `"priority"` (fund sources completely, most-invested first).
    ///
    /// It replaces the retired `Maintain`, which put hands on **one source's** keeping. Maintenance
    /// is a band-level standing role now (`assign_labor <faction> <band> agriculture|husbandry
    /// <workers>`), so what is left to decide is not *where the hands go* but *what happens when
    /// there are not enough of them* — and that is one decision per band, not one per source.
    UpkeepMode {
        faction_id: u32,
        band_id: u64,
        /// The mode token; anything the sim does not know is refused by name rather than guessed at.
        mode: String,
    },
    ExtendPen {
        faction_id: u32,
        target_x: u32,
        target_y: u32,
    },
    /// **Put a recipe on a band's crafting bench.** The crew is the player's to name — see
    /// [`BENCH_CREW_UNSPECIFIED`].
    ///
    /// *The bench is the assignment* (`docs/plan_crafting_and_materials.md` §7): there is no Crafter
    /// role and no labor target, because crafting always has a subject and is therefore staffed like
    /// a worked source rather than like a standing role. **One job at a time**, so this replaces
    /// whatever the bench was making — and the pile that job had already drawn goes with it.
    SetBench {
        faction_id: u32,
        band_id: u64,
        recipe_id: String,
        /// [`BENCH_CREW_UNSPECIFIED`] leaves the crew exactly as it is — which is the shape the
        /// client always sends, so a staged job starts unstaffed and the player names the number.
        workers: u32,
    },
    /// Take the job off a band's bench and hand its crew back to the idle pool.
    ClearBench {
        faction_id: u32,
        band_id: u64,
    },
    /// Change the crew on a band's running bench, leaving the job and its progress alone.
    BenchCrew {
        faction_id: u32,
        band_id: u64,
        workers: u32,
    },
    /// **MARK ONE BAND'S CRAFTING BENCH WITH THE PLAYER'S OWN RANK** — `high` | `normal` | `low`,
    /// the same [`CommandPayload::WorkPriority`] sets on a worked row.
    ///
    /// **The bench's own verb, not a `work_priority` token.** Every other bench command is addressed
    /// `<faction> <band>` with no source, and `work_priority`'s grammar reads a bare single token as
    /// a **herd id** — so `work_priority <f> <b> bench low` would be ambiguous with a herd named
    /// `bench`. A sibling verb has no ambiguity to resolve and matches the family it joins.
    BenchPriority {
        faction_id: u32,
        band_id: u64,
        /// The level token: `"high"`, `"normal"` or `"low"`.
        level: String,
    },
    /// The Telling: answer a pending narrative fork with one of its authored choices.
    AnswerFork {
        faction_id: u32,
        beat_id: String,
        choice_id: String,
    },
    CancelOrder {
        faction_id: u32,
        band_id: Option<u64>,
        scope: CancelScope,
    },
    AssignLabor {
        faction_id: u32,
        band_id: Option<u64>,
        role: String,
        workers: u32,
        target_x: Option<u32>,
        target_y: Option<u32>,
        fauna_id: Option<String>,
        /// **RETIRED by the harvest floor arc** — a labor assignment carries a [`Self::AssignLabor`]
        /// `floor`, not a stance. Kept on the payload only because the proto field number is
        /// immutable once shipped; the server ignores it.
        policy: Option<String>,
        /// Which named plant a **forage** `Cultivate`/`Sow` should commit the patch to — a
        /// `flora_config.json` species key. `None` = *"pick the tile's dominant legal plant"*
        /// (`docs/plan_flora_roster.md` §4.3). Ignored for every other role.
        species: Option<String>,
        /// **WHERE THE CREW STOPS, as a fraction of the source's carrying capacity.** `None` = the
        /// sim's default (`components::DEFAULT_ESCAPEMENT_FLOOR`). Validated `0.0..=1.0` at the
        /// server boundary and **rejected**, never clamped. Ignored by the band-wide roles.
        floor: Option<f32>,
        /// **The kit this crew works under** — an `equipment.json` roster id. `None` = the job's
        /// default. An unknown id, or one whose `jobs` does not cover this role, is a **command
        /// failure with a reason**, never a silent fall back: naming a kit is how the player
        /// compares tiers, so a quiet substitution answers a different question than the one asked.
        /// Ignored by the band-wide roles, which consume no kit component.
        kit_id: Option<String>,
        /// **WHICH PLANTS A FORAGE CREW CARRIES HOME** (the selective gather) — `flora_config.json`
        /// species keys. **Empty means take the whole basket**, the default and byte-identical to
        /// every command sent before this field existed; naming one or more leaves the rest of the
        /// stand standing.
        ///
        /// Not [`Self::AssignLabor::species`], which is the *commit* crop a `Cultivate`/`Sow` names
        /// and which is inert until that improvement completes. A key the roster does not know, or
        /// one that does not grow on **this** tile, is a **command failure with a reason** — never
        /// silently dropped, because a dropped selection is indistinguishable from *"take
        /// everything"*. Order and duplicates are irrelevant: the sim sorts and deduplicates on
        /// construction. Ignored by every other role.
        take_species: Vec<String>,
    },
    MoveBand {
        faction_id: u32,
        band_id: Option<u64>,
        target_x: u32,
        target_y: u32,
    },
    SendExpedition {
        faction_id: u32,
        band_id: Option<u64>,
        party_workers: u32,
        target_x: u32,
        target_y: u32,
    },
    RecallExpedition {
        faction_id: u32,
        expedition_band_id: u64,
    },
    /// **Form a new band** — a resident band splits in two on the tile it is standing on
    /// (`docs/plan_band_fission.md`). `workers` is the player's ONE input; children, elders and
    /// every store divide on the share it implies, so the new band is a smaller copy of the one it
    /// came from rather than a party with a composition of its own.
    SplitBand {
        faction_id: u32,
        band_id: Option<u64>,
        workers: u32,
    },
    SendHuntExpedition {
        faction_id: u32,
        band_id: Option<u64>,
        party_workers: u32,
        fauna_id: String,
        /// **Where the raid stops**, as a fraction of the herd's carrying capacity. `None` = the
        /// sim's default (`components::DEFAULT_ESCAPEMENT_FLOOR`); validated `0.0..=1.0` at the
        /// server boundary and **rejected**, never clamped.
        floor: Option<f32>,
        /// **The kit the party is SENT OUT WITH** — an `equipment.json` roster id, resolved **once**
        /// at launch and carried for the party's whole life. `None` = the hunt job's default; an
        /// unknown id, or one whose `jobs` does not include `hunt`, fails the command with a reason.
        kit_id: Option<String>,
    },
    /// **Outfit and launch a DENIAL RAID** (`docs/plan_denial_raid.md`) — the third expedition verb,
    /// beside Scout and Hunt. Proto field 49.
    ///
    /// **It carries no floor, and cannot be given one.** Its escapement ceiling is the herd's whole
    /// standing stock, so there is no floor to name — the order is *"this herd, this
    /// many people"*. That is why it is its own payload rather than a flag on
    /// [`Self::SendHuntExpedition`]: there is nothing here to validate and nothing to tune.
    ///
    /// **No target faction** — denial is aimed at a herd, not at a player.
    SendDenialRaid {
        faction_id: u32,
        band_id: Option<u64>,
        party_workers: u32,
        fauna_id: String,
        /// **The kit the raid is sent out with** — the one thing there *is* to say about a mission
        /// that carries no floor, because a kit is a property of the **party** rather than of the
        /// mission. Same rule as [`Self::SendHuntExpedition::kit_id`].
        kit_id: Option<String>,
    },
    /// **Outfit and launch a TRADE EXPEDITION** — the first rider on the connection primitive
    /// (`docs/plan_contact_and_logistics.md` §Q5, arc #527). Proto field 55.
    ///
    /// **A shipment is a party that walks it.** There is no persistent link component underneath in
    /// this slice: what maintains a link is a *route*, and the route ladder is what will hold that
    /// state.
    ///
    /// **Gated on a CONNECTION, never on a faction.** The sending band must hold a live tie to the
    /// destination band; there is deliberately no same-faction check anywhere, because faction is a
    /// property of the endpoint.
    SendTradeExpedition {
        faction_id: u32,
        band_id: Option<u64>,
        party_workers: u32,
        /// The destination's `BandId` — a durable id, never an entity.
        destination_band_id: u64,
        /// What the party is loaded with. **Empty is a command failure**, not an empty shipment.
        cargo: Vec<TradeCargoItem>,
        /// **The kit the party is sent out with**, resolved once at launch — same rule as
        /// [`Self::SendHuntExpedition::kit_id`].
        kit_id: Option<String>,
    },
    ExportMap {
        path: Option<String>,
    },
    /// Boot-idle new game: generate a world on demand (the server boots with none). `seed == 0`
    /// randomizes the map seed, mirroring `ResetMap`; an unknown `profile_id` is rejected server-side.
    /// Proto field 43.
    NewGame {
        preset_id: String,
        width: u32,
        height: u32,
        seed: u64,
        profile_id: String,
    },
    /// Stage a config-tuning override, applied at the **next** `new_game`. Proto field 47.
    ///
    /// `patch_json` is carried as an opaque string on purpose: it is a *sparse* patch whose shape is
    /// that of the target config, which `sim_runtime` deliberately knows nothing about. The server
    /// merges, validates and installs it.
    SetConfigOverride {
        kind: ConfigOverrideKind,
        patch_json: String,
    },
    /// Drop every staged config override; the next `new_game` boots on the shipped configs.
    /// Proto field 48.
    ClearConfigOverrides,
    /// **Ask the sim a question and get an answer back on the same socket.** Proto field 50, and the
    /// only payload in this enum that is *answered* rather than *applied*.
    ///
    /// It mutates nothing, so it is deliberately outside the replay log: a logged query would make a
    /// replay re-answer questions nobody asked. `request_id` is the client's own correlation number,
    /// echoed on the [`QueryReplyEnvelope`] that comes back.
    Query {
        request_id: u64,
        query: QueryPayload,
    },
}

/// Which question a [`CommandPayload::Query`] asks. Mirrors the proto `QueryCommand.query` oneof.
#[derive(Debug, Clone, PartialEq)]
pub enum QueryPayload {
    HuntTripForecast(HuntTripForecastQuery),
    DenialRaidForecast(DenialRaidForecastQuery),
    HuntCrewTake(HuntCrewTakeQuery),
}

/// *"What does this party, off this band, carrying this kit, take off this herd at this floor?"*
///
/// Every field is an exact ask, not a sample: the answer is computed for these values and echoes
/// them back on each row so a client can assert it got what it asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct HuntTripForecastQuery {
    pub faction_id: u32,
    /// The asking band's durable `BandId` — its **live** equipment wear prices the answer.
    pub band_id: u64,
    pub herd_id: String,
    /// An `equipment.json` roster id, **required**. Unknown or wrong-job is an error, never a quiet
    /// fall back to the job default.
    pub kit_id: String,
    pub party_workers: u32,
    pub floor: f32,
    /// The sheet's floor presets, answered in the same round trip at the same party size.
    pub preset_floors: Vec<f32>,
    /// **The largest party this band could field** — its idle workers. Bounds the reply's
    /// `useful_cap` plateau scan, which walks `1..=max` **contiguously**; `0` means "do not scan".
    pub max_party_workers: u32,
}

/// The denial twin of [`HuntTripForecastQuery`] — no floor, because the mission carries none.
#[derive(Debug, Clone, PartialEq)]
pub struct DenialRaidForecastQuery {
    pub faction_id: u32,
    pub band_id: u64,
    pub herd_id: String,
    pub kit_id: String,
    pub party_workers: u32,
    /// **The largest party this band could field** — its idle workers. Bounds the `party_needed`
    /// search, which walks `1..=max` and stops at the first party that drives the herd past
    /// recovery; `0` answers the sentinel.
    pub max_party_workers: u32,
}

/// *"How many animals does a **resident** band of each crew size bring down off this herd per turn,
/// at this floor, carrying this kit?"* — the Assign Herders panel's question.
///
/// **It is not the trip sheet's question.** [`HuntTripForecastQuery`] answers one party over a whole
/// detached expedition and prices it at `combat_config.expedition_danger_multiplier` (1.5× lethality
/// as shipped); a resident band hunting its own range fights at the **base** tuning. The two answers
/// differ by half again in the fight term, so neither reply may borrow the other's rows.
#[derive(Debug, Clone, PartialEq)]
pub struct HuntCrewTakeQuery {
    pub faction_id: u32,
    /// The asking band's durable `BandId` — its **live** equipment wear prices the fight.
    pub band_id: u64,
    pub herd_id: String,
    /// An `equipment.json` roster id, **required** — the same rule the trip query follows.
    pub kit_id: String,
    /// The composed floor, as a fraction of the herd's `K`. **A term in the answer, not a filter**:
    /// the escapement room bounds what the party goes after *before* the retreat and the fight
    /// (`fauna::animals_affordable`), so a curve answered at one floor cannot be reused at another.
    pub floor: f32,
    /// **The largest crew this band could put on the herd** — the stepper's own cap, and exactly the
    /// length of the reply's `per_crew`. `0` asks for nothing and is answered with an empty curve.
    pub max_workers: u32,
}

/// **The server → client answer frame**, written back on the command socket as its own
/// length-prefixed frame. A top-level envelope rather than a `CommandEnvelope` variant: the two
/// directions carry different vocabularies, and nothing but a query is ever answered.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryReplyEnvelope {
    /// Echoes the [`CommandPayload::Query::request_id`] this answers.
    pub request_id: u64,
    pub reply: QueryReply,
}

/// What came back. [`QueryReply::Error`] carries a machine-readable snake_case token
/// ([`query_error`]); the client owns the prose.
#[derive(Debug, Clone, PartialEq)]
pub enum QueryReply {
    HuntTripForecast(HuntTripForecastReply),
    DenialRaidForecast(DenialRaidForecastReply),
    HuntCrewTake(HuntCrewTakeReply),
    Error(String),
}

/// **The refusal tokens a [`QueryReply::Error`] can carry.** Named constants rather than literals at
/// the raising sites, so the client's match arms and the server's answers cannot drift apart.
pub mod query_error {
    /// The server is idle — no world has been generated yet, so there is nothing to forecast.
    pub const NO_ACTIVE_WORLD: &str = "no_active_world";
    /// No live herd carries the queried id.
    pub const UNKNOWN_HERD: &str = "unknown_herd";
    /// No band of the queried faction carries the queried `BandId`.
    pub const UNKNOWN_BAND: &str = "unknown_band";
    /// The queried `kit_id` names no `equipment.json` roster entry.
    pub const UNKNOWN_KIT: &str = "unknown_kit";
    /// The kit exists but its `jobs` list does not cover hunting.
    pub const KIT_WRONG_JOB: &str = "kit_wrong_job";
    /// A composed or preset floor outside `0..=1`. Rejected, never clamped — the same rule the
    /// launch commands follow.
    pub const INVALID_FLOOR: &str = "invalid_floor";
    /// A party of zero. There is no raid to project, so there is no answer to give.
    pub const INVALID_PARTY: &str = "invalid_party";
    /// A crew-take query asking about more workers than any band could field. The reply is one row
    /// per crew, so the ask is linear in that number and an unbounded one is a wedged command thread
    /// rather than a large answer. Refused, never clamped — the same rule an out-of-range floor
    /// follows, for the same reason: a clamp answers a question nobody asked.
    pub const INVALID_CREW: &str = "invalid_crew";
}

/// **The FOOD commodity key a shipment's food line names** — the same string `core_sim`'s
/// `components::FOOD` is, restated here because `sim_runtime` deliberately does not depend on the
/// sim.
///
/// **The duplication is safe because the server does not trust it.** A non-material cargo line whose
/// `id` is not the larder's key is a command **failure** with a reason, so if the two ever drift the
/// shipment is refused rather than quietly loaded with the wrong good.
pub const FOOD_CARGO_KEY: &str = "provisions";

/// **One line of a shipment** — a quantity of one thing, out of the sending band's own store
/// (`docs/plan_contact_and_logistics.md` §Q5).
///
/// **`is_material` disambiguates two namespaces that share a string key space.** `id` is either a
/// FOOD commodity key (the larder's `provisions`) or a `materials.json` material id, and the two
/// tables are authored independently — nothing stops a material one day being called `provisions`.
/// A flag rather than two repeated fields keeps the order the player named the lines in, and makes a
/// third account (fodder) a value rather than a schema change.
#[derive(Debug, Clone, PartialEq)]
pub struct TradeCargoItem {
    pub id: String,
    pub is_material: bool,
    pub amount: f32,
}

/// **One material a projection lands, and how much of it** — the runtime twin of the snapshot's
/// `MaterialPayoff`, and the shape every material readout in this arc uses.
///
/// It carries **no quality reading**: a rating is a characteristic vector on the batch the take
/// really creates, and a launch-sheet row asks the flat question *"how much of what"*. **Never
/// summed** into one figure — that is the retired trade-goods axis under a new name.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialPayoff {
    /// The `materials.json` id — `hide`, `bone`, `fibre`. Resolved client-side for display.
    pub material_id: String,
    /// Units of that material the trip lands.
    pub amount: f32,
}

/// One answered hunt-trip forecast. The wire twin of a row of the retired `HuntTripEstimateState`
/// table, plus the echoed `floor` / `party_workers` that make it self-describing.
#[derive(Debug, Clone, PartialEq)]
pub struct HuntTripRow {
    pub floor: f32,
    pub party_workers: u32,
    /// `0` = the raid never completed inside the forecast horizon; `bound` says which kind of never.
    pub turns_to_fill: u32,
    pub bound: String,
    pub delivers_food: bool,
    /// **Whole** animals killed — a count, typed as one, exactly as the retired
    /// `HuntTripEstimateState` typed it and as [`DenialRow::animals_killed`] types it.
    pub animals_taken: u32,
    pub delivered_food: f32,
    pub wasted_food: f32,
    /// **What the trip lands, per material** — and on an **inedible** quarry the entire payload,
    /// since `delivered_food` is `0` there. Projected off the same carried biomass `delivered_food`
    /// is. **Empty is "no row", never zero**, and it is never summed.
    pub delivered_material: Vec<MaterialPayoff>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HuntTripForecastReply {
    /// The answer at the exact floor the query named.
    pub at_composed: HuntTripRow,
    /// One row per queried preset floor, in the same order.
    pub per_preset: Vec<HuntTripRow>,
    /// The max-useful party plateau — the LAST party at which the payload still rose, so a stepper
    /// seeds ON it; `0` = no plateau found. See the proto for what the number is and what the client
    /// still owns.
    pub useful_cap: u32,
}

/// One answered denial-raid forecast. The denial twin of [`HuntTripRow`], and like it the row
/// echoes the `party_workers` it was answered for.
#[derive(Debug, Clone, PartialEq)]
pub struct DenialRow {
    pub party_workers: u32,
    pub turns_to_collapse: u32,
    pub turns_to_collapse_low: u32,
    pub turns_to_collapse_high: u32,
    pub outcome: String,
    pub animals_killed: u32,
    pub delivered_food: f32,
    pub wasted_food: f32,
    /// **What the raid lands, per material** — the same haul `delivered_food` converts, and on an
    /// inedible quarry the whole of it. **Empty is "no row", never zero**, and it is never summed.
    pub delivered_material: Vec<MaterialPayoff>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DenialRaidForecastReply {
    pub at_composed: DenialRow,
    /// **The party the sheet opens on** — the smallest party that actually drives this herd past
    /// recovery, found by walking `1..=max_party_workers` and stopping at the first success. Seed
    /// the stepper here: below the requirement a denial raid accomplishes literally nothing however
    /// long it runs, so without it the control is a guessing game.
    ///
    /// **`0` means "NO PARTY YOU CAN FIELD drives this herd down", never "send nobody".** That
    /// meaning changed with the query, and it changed for the better: the retired
    /// `HerdTelemetryState.denialPartyNeeded` searched a *sampled* axis with no notion of who was
    /// asking, so it could name a party the band had no hope of raising and present that as the
    /// answer. This searches to the band's own last worker, so `0` is a fact the player can act on —
    /// *raise more people, or pick another quarry*. Render the answered row's `outcome` beside it,
    /// never a blank, and never a stepper seeded at `0`.
    pub party_needed: u32,
}

/// **One crew size's answer on the hunt take curve.**
///
/// `animals_*` is **animals a turn, as an EXPECTED RATE and deliberately not a whole number** —
/// `fauna::HuntFight::expected_brought_down` for this crew, at the queried floor, against this
/// herd's current wound ledger. `0.75` is a real and common answer, and it means *three animals
/// every four turns*.
///
/// # It is a rate because the quantised count is a lie on most of the stepper
///
/// The sim's take is floored to whole animals and the unfinished damage is **banked** on the quarry
/// (`combat::DamageLedger`), so a crew below one body a turn genuinely takes `0` on most turns and
/// `1` on the rest. Publishing that floored count made a Wild Aurochs read `0` for every crew from
/// **1 to 11** — a plateau no equipment level moves, because the quarry's `durability 150` is capped
/// against the `0.8` animals its `engage_rate` lets a crew corner. The panel printed *"≈0
/// animals/turn"* beside a work row quoting `0.84` food from the very same take.
///
/// # It is NOT `SourceYield::realized`, and the two are not interchangeable
///
/// This is the **instantaneous** rate at the herd's current stock. `realized` is a *forward average*
/// over `hunt.forecast_horizon_turns` turns of regrow → take, so it prices a herd that moves under
/// the crew and it sums the quantised kills — which leaves up to one unfinished body uncounted at
/// the horizon. `realized` therefore sits at or slightly below this curve, and the gap widens with
/// drawdown. Show one or the other; never present them as one figure.
///
/// # It is NOT a per-hunter rate, and must never be multiplied by a crew size
///
/// That is the mistake this row exists to make impossible, and the arithmetic does not survive it.
/// The take is `min(w × fight_rate, staircase(w))`, where the engagement staircase is
/// `max(floor(w × engage_rate), 1) × stay_fraction` — flat across whole runs of crew sizes and
/// stepping at integer boundaries. On the shipped Wild Boar (`engage_rate 0.33`) crews of 1 through
/// 6 all bring down `0.75` animals/turn, so a per-hunter reading spans **6×** across the stepper's
/// first six positions; on Wild Aurochs the *binding term itself* flips from the fight to the
/// engagement between crews 8 and 11 and back at 12. Look the row up; do not scale one.
///
/// # What is already folded in, so no consumer re-applies it
///
/// - the **engagement** bound ([`fauna::animals_engaged`]) — including its `max(…, 1)` floor, which
///   is why a crew of one is never zero for want of reach;
/// - the **escapement room** at the queried floor, clamped where the sim clamps it (before the
///   retreat, `fauna::animals_affordable`) rather than as an outer `min`;
/// - the **retreat** (`HuntingParty::stayers`), with this kit's `dispersion`;
/// - the **fight** — damage over durability through `combat::resolve_fight`, at the band's live
///   attack tier against the quarry's `defense`/`durability`, including the multi-turn wound ledger
///   the herd is standing there with.
///
/// # What is not, because it is the caller's own and stays a linear `min`
///
/// The crew's **carry** throughput (`workers × per-worker yield`) and the whole-animal room
/// `floor(ceiling / body_mass)`. The sim's own take is `min(affordable, carryable, brought_down)` —
/// the room spent **before** the take (`fauna::animals_affordable`, on the engagement or, for a pen,
/// on the collection) and the pack seated inside `fauna::quantise_animal_take` — so a client that
/// `min`s this row against those two lands on the **sustained** number the turn pays — still as a
/// rate. Rounding it for display is a presentation
/// choice; rounding it to `0` and calling that the answer is the defect above.
#[derive(Debug, Clone, PartialEq)]
pub struct HuntCrewTakeRow {
    /// Echoed so the row is self-describing — a client asserts the answer is for the crew it asked
    /// about rather than trusting its position in the list. Rows ascend from `1`.
    pub workers: u32,
    /// **The band, and it is per row** rather than once on the curve, because the spread is
    /// `O(√w)` and *shrinks per hunter* as the crew grows: both stochastic stages are binomials
    /// (`combat::attacks_landed_at`, `fauna::animals_that_stay`) whose standard deviation is
    /// `√(n·p·q)`. A single per-hunter band multiplied by a crew would overstate the spread by
    /// exactly `√w`, so it cannot be reconstructed from one figure.
    ///
    /// `likely` is the **expectation** over the seed, never a re-draw. Where no stage is stochastic
    /// — a species at `wariness 0`, at the shipped `combat_config.hit_chance = 1.0` — all three are
    /// **bit-identical**, because both binomials answer their degenerate identity whatever quantile
    /// is asked for.
    pub animals_low: f32,
    pub animals_likely: f32,
    pub animals_high: f32,
}

/// **The hunt take curve** — what each crew size actually brings down, so a pre-commit panel can
/// move its stepper without re-deriving the take.
///
/// It exists because the take is **not linear in crew size** and no scalar can be published that
/// makes it so; see [`HuntCrewTakeRow`] for the shape and the two shipped species that show it.
#[derive(Debug, Clone, PartialEq)]
pub struct HuntCrewTakeReply {
    /// **One row per crew size**, ascending, `1..=HuntCrewTakeQuery::max_workers`. Empty when
    /// `max_workers` is `0`. Index `i` is the crew of `i + 1`, and each row echoes its own
    /// `workers` so a client never has to trust that.
    pub per_crew: Vec<HuntCrewTakeRow>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EspionageGeneratorUpdate {
    pub template_id: String,
    pub enabled: Option<bool>,
    pub per_faction: Option<u8>,
}

/// What a `cancel_order` clears on the band. The Band panel splits the old single "cancel"
/// button into per-section clears, so the verb has to name its target rather than always wiping
/// everything.
///
/// [`CancelScope::All`] is the default (an absent wire/text token decodes to it) and keeps the
/// historical behaviour. The two narrow scopes deliberately leave travel alone: moving is not
/// working, so unassigning a band's sources must not also strand it mid-journey.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CancelScope {
    /// Worked sources + standing roles, and stop any in-progress travel.
    #[default]
    All,
    /// Worked food sources only (Forage/Hunt). Roles and travel untouched.
    Work,
    /// Standing roles only (Scout/Warrior). Sources and travel untouched.
    Roles,
}

impl CancelScope {
    /// Wire/text token for this scope.
    pub fn as_str(self) -> &'static str {
        match self {
            CancelScope::All => "all",
            CancelScope::Work => "work",
            CancelScope::Roles => "roles",
        }
    }

    /// Parse a scope token (case-insensitive). `None` for anything unrecognised, so callers choose
    /// whether to fail closed (the text parser) or fall back to [`CancelScope::All`] (the wire).
    pub fn parse(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "all" => Some(CancelScope::All),
            "work" => Some(CancelScope::Work),
            "roles" => Some(CancelScope::Roles),
            _ => None,
        }
    }
}

/// Directive for faction orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrdersDirective {
    Ready,
}

/// Configuration kinds supported by reload commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadConfigKind {
    Simulation,
    TurnPipeline,
    SnapshotOverlays,
    CrisisArchetypes,
    CrisisModifiers,
    CrisisTelemetry,
}

/// Which boot config a staged tuning override applies to.
///
/// Deliberately **not** [`ReloadConfigKind`]: that names the configs the running world can
/// hot-reload, this names the ones the client's tuning manifest can stage for the next `new_game`.
/// The two sets barely overlap and answer different questions, so folding them together would make
/// half of each list nonsense.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigOverrideKind {
    Simulation,
    Labor,
    Demographics,
    Expedition,
    Combat,
    /// The materials table (`materials.json`). Retuning `characteristic_bands` re-partitions every
    /// batch on the map, which is why it is worth staging rather than editing under a running world.
    Materials,
    /// The recipe book (`recipes.json`) — the costs, the `work` values and the grade seams.
    Recipes,
}

impl ConfigOverrideKind {
    /// Every kind, in manifest order. Iterating this is what lets a reader (the manifest drift
    /// test, the help text) stay exhaustive without a second hand-maintained list.
    pub const ALL: &'static [ConfigOverrideKind] = &[
        ConfigOverrideKind::Simulation,
        ConfigOverrideKind::Labor,
        ConfigOverrideKind::Demographics,
        ConfigOverrideKind::Expedition,
        ConfigOverrideKind::Combat,
        ConfigOverrideKind::Materials,
        ConfigOverrideKind::Recipes,
    ];

    /// The wire spelling, shared with the client's `tuning_manifest.json` `kind` field.
    pub fn as_str(self) -> &'static str {
        match self {
            ConfigOverrideKind::Simulation => "simulation",
            ConfigOverrideKind::Labor => "labor",
            ConfigOverrideKind::Demographics => "demographics",
            ConfigOverrideKind::Expedition => "expedition",
            ConfigOverrideKind::Combat => "combat",
            ConfigOverrideKind::Materials => "materials",
            ConfigOverrideKind::Recipes => "recipes",
        }
    }

    /// Parse a wire spelling. `None` for anything else — an unknown kind is rejected, never
    /// defaulted, because guessing which config a designer meant to retune is the wrong answer.
    pub fn from_wire_str(value: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|kind| kind.as_str() == value)
    }
}

/// Counter-intelligence security posture controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityPolicyKind {
    Lenient,
    Standard,
    Hardened,
    Crisis,
}

/// **The largest length-prefixed frame the command socket will carry, in EITHER direction.**
///
/// Both ends read a 4-byte little-endian length and then that many bytes: the server's
/// `handle_proto_client` reading a [`CommandEnvelope`], and the client reading a
/// [`QueryReplyEnvelope`] back. A frame past this bound is refused, and on the read path that costs
/// the whole connection — a reader cannot resynchronise mid-stream, because it has no way to tell a
/// length prefix from payload once it is lost.
///
/// # It lives here because it is a property of the PROTOCOL, not of either end
///
/// It used to be a private `const` in the server binary, so the client's bridge restated it. Two
/// independent copies of a bound that both ends must agree on is a defect with a long fuse: nothing
/// fails while frames stay small, and the day one grows past the smaller copy the sender writes a
/// frame the receiver refuses — and drops the connection over it. One definition, beside the
/// envelopes it bounds, is what makes disagreement unrepresentable.
///
/// **It became bidirectional when the socket did.** It was written for a one-way command channel;
/// the query channel added a reply direction on the same stream, and the writer checks an encoded
/// reply against this same bound before framing it — because a reply the far end would refuse as
/// oversized must not go on the wire at all. There it costs one unanswered query instead of the
/// connection.
pub const MAX_PROTO_FRAME: usize = 64 * 1024;

/// **A `set_bench` that names no crew: DO NOT CHANGE THE CREW.**
///
/// Not *"the sim decides"* — the sim never decides this. Labor is the scarce currency and dividing
/// the band is the game's turn-to-turn decision, so how many hands stop hunting to stand at a bench
/// is the player's call and only theirs. An idle bench therefore stages the recipe with **nobody**
/// on it and waits for the stepper; a bench already running a job **keeps the crew standing there**
/// through the swap, because an absent number is not an order to send anyone home.
///
/// The value is `0` because `workers` rides a proto3 scalar, which cannot distinguish an absent
/// field from an explicit zero. No intent is lost by that reading: `bench_crew <n>` is how a player
/// sets an explicit crew — zero included, which is how a bench is stood down without taking the job
/// off it.
pub const BENCH_CREW_UNSPECIFIED: u32 = 0;

/// Error returned when encoding a command envelope fails.
#[derive(Debug, Error)]
pub enum CommandEncodeError {
    #[error("encode failed: {0}")]
    Encode(#[from] prost::EncodeError),
}

/// Error returned when decoding a command envelope fails.
///
/// The protobuf schema reserves `*_UNSPECIFIED` enum values (encoded as `0`).
/// Decoding such a value yields [`CommandDecodeError::InvalidEnum`] so callers
/// can reject malformed or legacy payloads early.
#[derive(Debug, Error)]
pub enum CommandDecodeError {
    #[error("decode failed: {0}")]
    Decode(#[from] prost::DecodeError),
    #[error("command envelope missing payload")]
    MissingPayload,
    #[error("invalid enum value {value} for {field}")]
    InvalidEnum { field: &'static str, value: i32 },
}

impl CommandEnvelope {
    /// Encode the envelope into a protobuf binary frame.
    pub fn encode_to_vec(&self) -> Result<Vec<u8>, CommandEncodeError> {
        let proto = self.to_proto();
        let mut buffer = Vec::with_capacity(proto.encoded_len());
        proto.encode(&mut buffer)?;
        Ok(buffer)
    }

    /// Decode an envelope from a protobuf binary frame.
    pub fn decode(bytes: &[u8]) -> Result<Self, CommandDecodeError> {
        let proto = pb::CommandEnvelope::decode(bytes)?;
        Self::try_from_proto(proto)
    }

    /// Convert the high-level envelope into its protobuf representation.
    pub fn to_proto(&self) -> pb::CommandEnvelope {
        let command = Some(match &self.payload {
            CommandPayload::Turn { steps } => {
                pb::command_envelope::Command::Turn(pb::TurnCommand { steps: *steps })
            }
            CommandPayload::ResetMap { width, height } => {
                pb::command_envelope::Command::ResetMap(pb::ResetMapCommand {
                    width: *width,
                    height: *height,
                })
            }
            CommandPayload::Orders {
                faction_id,
                directive,
            } => pb::command_envelope::Command::Orders(pb::OrdersCommand {
                faction_id: *faction_id,
                directive: orders_directive_to_proto(*directive) as i32,
            }),
            CommandPayload::Rollback { tick } => {
                pb::command_envelope::Command::Rollback(pb::RollbackCommand { tick: *tick })
            }
            CommandPayload::UpdateEspionageGenerators { updates } => {
                pb::command_envelope::Command::UpdateEspionageGenerators(
                    pb::UpdateEspionageGeneratorsCommand {
                        updates: updates
                            .iter()
                            .map(|update| pb::EspionageGeneratorUpdate {
                                template_id: update.template_id.clone(),
                                enabled: update.enabled,
                                per_faction: update.per_faction.map(|value| value as u32),
                            })
                            .collect(),
                    },
                )
            }
            CommandPayload::QueueEspionageMission {
                mission_id,
                owner_faction,
                target_owner_faction,
                discovery_id,
                agent_handle,
                target_tier,
                scheduled_tick,
            } => pb::command_envelope::Command::QueueEspionageMission(
                pb::QueueEspionageMissionCommand {
                    mission_id: mission_id.clone(),
                    owner_faction: *owner_faction,
                    target_owner_faction: *target_owner_faction,
                    discovery_id: *discovery_id,
                    agent_handle: *agent_handle,
                    target_tier: target_tier.map(|value| value as u32),
                    scheduled_tick: *scheduled_tick,
                },
            ),
            CommandPayload::UpdateEspionageQueueDefaults {
                scheduled_tick_offset,
                target_tier,
            } => pb::command_envelope::Command::UpdateEspionageQueueDefaults(
                pb::UpdateEspionageQueueDefaultsCommand {
                    scheduled_tick_offset: *scheduled_tick_offset,
                    target_tier: target_tier.map(|value| value as u32),
                },
            ),
            CommandPayload::UpdateCounterIntelPolicy { faction, policy } => {
                pb::command_envelope::Command::UpdateCounterIntelPolicy(
                    pb::UpdateCounterIntelPolicyCommand {
                        faction: *faction,
                        policy: security_policy_kind_to_proto(*policy) as i32,
                    },
                )
            }
            CommandPayload::AdjustCounterIntelBudget {
                faction,
                reserve,
                delta,
            } => pb::command_envelope::Command::AdjustCounterIntelBudget(
                pb::AdjustCounterIntelBudgetCommand {
                    faction: *faction,
                    reserve: *reserve,
                    delta: *delta,
                },
            ),
            CommandPayload::ReloadConfig { kind, path } => {
                pb::command_envelope::Command::ReloadConfig(pb::ReloadConfigCommand {
                    kind: reload_config_kind_to_proto(*kind) as i32,
                    path: path.clone(),
                })
            }
            CommandPayload::SetCrisisAutoSeed { enabled } => {
                pb::command_envelope::Command::SetCrisisAutoSeed(pb::SetCrisisAutoSeedCommand {
                    enabled: *enabled,
                })
            }
            CommandPayload::SetFogEnabled { enabled } => {
                pb::command_envelope::Command::SetFog(pb::SetFogEnabledCommand {
                    enabled: *enabled,
                })
            }
            CommandPayload::Resync => pb::command_envelope::Command::Resync(pb::ResyncCommand {}),
            CommandPayload::SpawnCrisis {
                faction_id,
                archetype_id,
            } => pb::command_envelope::Command::SpawnCrisis(pb::SpawnCrisisCommand {
                faction: *faction_id,
                archetype_id: archetype_id.clone(),
            }),
            CommandPayload::SetStartProfile { profile_id } => {
                pb::command_envelope::Command::SetStartProfile(pb::SetStartProfileCommand {
                    profile_id: profile_id.clone(),
                })
            }
            CommandPayload::ScoutArea {
                faction_id,
                target_x,
                target_y,
                band_id,
            } => pb::command_envelope::Command::ScoutArea(pb::ScoutAreaCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
                band_id: *band_id,
            }),
            CommandPayload::FollowHerd {
                faction_id,
                herd_id,
                policy,
                band_id,
            } => pb::command_envelope::Command::FollowHerd(pb::FollowHerdCommand {
                faction_id: *faction_id,
                herd_id: herd_id.clone(),
                policy: policy.clone(),
                band_id: *band_id,
            }),
            CommandPayload::FoundSettlement {
                faction_id,
                target_x,
                target_y,
            } => pb::command_envelope::Command::FoundSettlement(pb::FoundSettlementCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
            }),
            CommandPayload::ForageTile {
                faction_id,
                target_x,
                target_y,
                module,
                band_id,
            } => pb::command_envelope::Command::ForageTile(pb::ForageTileCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
                module: module.clone(),
                band_id: *band_id,
            }),
            CommandPayload::HuntGame {
                faction_id,
                target_x,
                target_y,
                band_id,
            } => pb::command_envelope::Command::HuntGame(pb::HuntGameCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
                band_id: *band_id,
            }),
            CommandPayload::HuntFauna {
                faction_id,
                herd_id,
                band_id,
            } => pb::command_envelope::Command::HuntFauna(pb::HuntFaunaCommand {
                faction_id: *faction_id,
                herd_id: herd_id.clone(),
                band_id: *band_id,
            }),
            CommandPayload::Tame {
                faction_id,
                herd_id,
            } => pb::command_envelope::Command::Tame(pb::TameCommand {
                faction_id: *faction_id,
                herd_id: herd_id.clone(),
            }),
            CommandPayload::Cultivate {
                faction_id,
                target_x,
                target_y,
            } => pb::command_envelope::Command::Cultivate(pb::CultivateCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
            }),
            CommandPayload::Sow {
                faction_id,
                target_x,
                target_y,
            } => pb::command_envelope::Command::Sow(pb::SowCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
            }),
            CommandPayload::Abandon {
                faction_id,
                target_x,
                target_y,
                herd_id,
            } => pb::command_envelope::Command::Abandon(pb::AbandonCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
                herd_id: herd_id.clone(),
            }),
            CommandPayload::Unqueue {
                faction_id,
                target_x,
                target_y,
                herd_id,
            } => pb::command_envelope::Command::Unqueue(pb::UnqueueCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
                herd_id: herd_id.clone(),
            }),
            CommandPayload::BuildOrder {
                faction_id,
                band_id,
                target_x,
                target_y,
                herd_id,
                position,
            } => pb::command_envelope::Command::BuildOrder(pb::BuildOrderCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                target_x: *target_x,
                target_y: *target_y,
                herd_id: herd_id.clone(),
                position: *position,
            }),
            CommandPayload::BuildKit {
                faction_id,
                target_x,
                target_y,
                herd_id,
                kit_id,
            } => pb::command_envelope::Command::BuildKit(pb::BuildKitCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
                herd_id: herd_id.clone(),
                kit_id: kit_id.clone(),
            }),
            CommandPayload::UpkeepKit {
                faction_id,
                target_x,
                target_y,
                herd_id,
                kit_id,
            } => pb::command_envelope::Command::UpkeepKit(pb::UpkeepKitCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
                herd_id: herd_id.clone(),
                kit_id: kit_id.clone(),
            }),
            CommandPayload::WorkPriority {
                faction_id,
                band_id,
                target_x,
                target_y,
                herd_id,
                level,
            } => pb::command_envelope::Command::WorkPriority(pb::WorkPriorityCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                target_x: *target_x,
                target_y: *target_y,
                herd_id: herd_id.clone(),
                level: level.clone(),
            }),
            CommandPayload::UpkeepMode {
                faction_id,
                band_id,
                mode,
            } => pb::command_envelope::Command::UpkeepMode(pb::UpkeepModeCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                mode: mode.clone(),
            }),
            CommandPayload::SetBench {
                faction_id,
                band_id,
                recipe_id,
                workers,
            } => pb::command_envelope::Command::SetBench(pb::SetBenchCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                recipe_id: recipe_id.clone(),
                workers: *workers,
            }),
            CommandPayload::ClearBench {
                faction_id,
                band_id,
            } => pb::command_envelope::Command::ClearBench(pb::ClearBenchCommand {
                faction_id: *faction_id,
                band_id: *band_id,
            }),
            CommandPayload::BenchCrew {
                faction_id,
                band_id,
                workers,
            } => pb::command_envelope::Command::BenchCrew(pb::BenchCrewCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                workers: *workers,
            }),
            CommandPayload::BenchPriority {
                faction_id,
                band_id,
                level,
            } => pb::command_envelope::Command::BenchPriority(pb::BenchPriorityCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                level: level.clone(),
            }),
            CommandPayload::Corral {
                faction_id,
                target_x,
                target_y,
            } => pb::command_envelope::Command::Corral(pb::CorralCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
            }),
            CommandPayload::ExtendPen {
                faction_id,
                target_x,
                target_y,
            } => pb::command_envelope::Command::ExtendPen(pb::ExtendPenCommand {
                faction_id: *faction_id,
                target_x: *target_x,
                target_y: *target_y,
            }),
            CommandPayload::AnswerFork {
                faction_id,
                beat_id,
                choice_id,
            } => pb::command_envelope::Command::AnswerFork(pb::AnswerForkCommand {
                faction_id: *faction_id,
                beat_id: beat_id.clone(),
                choice_id: choice_id.clone(),
            }),
            CommandPayload::CancelOrder {
                faction_id,
                band_id,
                scope,
            } => pb::command_envelope::Command::CancelOrder(pb::CancelOrderCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                scope: Some(scope.as_str().to_string()),
            }),
            CommandPayload::AssignLabor {
                faction_id,
                band_id,
                role,
                workers,
                target_x,
                target_y,
                fauna_id,
                policy,
                species,
                floor,
                kit_id,
                take_species,
            } => pb::command_envelope::Command::AssignLabor(pb::AssignLaborCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                role: role.clone(),
                workers: *workers,
                target_x: *target_x,
                target_y: *target_y,
                fauna_id: fauna_id.clone(),
                policy: policy.clone(),
                species: species.clone(),
                floor: *floor,
                kit_id: kit_id.clone(),
                take_species: take_species.clone(),
            }),
            CommandPayload::MoveBand {
                faction_id,
                band_id,
                target_x,
                target_y,
            } => pb::command_envelope::Command::MoveBand(pb::MoveBandCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                target_x: *target_x,
                target_y: *target_y,
            }),
            CommandPayload::SendExpedition {
                faction_id,
                band_id,
                party_workers,
                target_x,
                target_y,
            } => pb::command_envelope::Command::SendExpedition(pb::SendExpeditionCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                party_workers: *party_workers,
                target_x: *target_x,
                target_y: *target_y,
            }),
            CommandPayload::RecallExpedition {
                faction_id,
                expedition_band_id,
            } => pb::command_envelope::Command::RecallExpedition(pb::RecallExpeditionCommand {
                faction_id: *faction_id,
                expedition_band_id: *expedition_band_id,
            }),
            CommandPayload::SplitBand {
                faction_id,
                band_id,
                workers,
            } => pb::command_envelope::Command::SplitBand(pb::SplitBandCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                workers: *workers,
            }),
            CommandPayload::SendHuntExpedition {
                faction_id,
                band_id,
                party_workers,
                fauna_id,
                floor,
                kit_id,
            } => pb::command_envelope::Command::SendHuntExpedition(pb::SendHuntExpeditionCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                party_workers: *party_workers,
                fauna_id: fauna_id.clone(),
                // Retired by the harvest floor arc; the number is immutable, the value unread.
                policy: None,
                floor: *floor,
                // Retired with the fill target itself; same rule — the field number is immutable and
                // the value is never written.
                fill_target: None,
                kit_id: kit_id.clone(),
            }),
            CommandPayload::SendDenialRaid {
                faction_id,
                band_id,
                party_workers,
                fauna_id,
                kit_id,
            } => pb::command_envelope::Command::SendDenialRaid(pb::SendDenialRaidCommand {
                faction_id: *faction_id,
                band_id: *band_id,
                party_workers: *party_workers,
                fauna_id: fauna_id.clone(),
                kit_id: kit_id.clone(),
            }),
            CommandPayload::SendTradeExpedition {
                faction_id,
                band_id,
                party_workers,
                destination_band_id,
                cargo,
                kit_id,
            } => {
                pb::command_envelope::Command::SendTradeExpedition(pb::SendTradeExpeditionCommand {
                    faction_id: *faction_id,
                    band_id: *band_id,
                    party_workers: *party_workers,
                    destination_band_id: *destination_band_id,
                    cargo: cargo
                        .iter()
                        .map(|item| pb::TradeCargoItem {
                            id: item.id.clone(),
                            is_material: item.is_material,
                            amount: item.amount,
                        })
                        .collect(),
                    kit_id: kit_id.clone(),
                })
            }
            CommandPayload::ExportMap { path } => {
                pb::command_envelope::Command::ExportMap(pb::ExportMapCommand {
                    path: path.clone(),
                })
            }
            CommandPayload::NewGame {
                preset_id,
                width,
                height,
                seed,
                profile_id,
            } => pb::command_envelope::Command::NewGame(pb::NewGameCommand {
                preset_id: preset_id.clone(),
                width: *width,
                height: *height,
                seed: *seed,
                profile_id: profile_id.clone(),
            }),
            CommandPayload::SetConfigOverride { kind, patch_json } => {
                pb::command_envelope::Command::SetConfigOverride(pb::SetConfigOverrideCommand {
                    kind: config_override_kind_to_proto(*kind) as i32,
                    patch_json: patch_json.clone(),
                })
            }
            CommandPayload::ClearConfigOverrides => {
                pb::command_envelope::Command::ClearConfigOverrides(
                    pb::ClearConfigOverridesCommand {},
                )
            }
            CommandPayload::Query { request_id, query } => {
                pb::command_envelope::Command::Query(pb::QueryCommand {
                    request_id: *request_id,
                    query: Some(match query {
                        QueryPayload::HuntTripForecast(ask) => {
                            pb::query_command::Query::HuntTripForecast(pb::HuntTripForecastQuery {
                                faction_id: ask.faction_id,
                                band_id: ask.band_id,
                                herd_id: ask.herd_id.clone(),
                                kit_id: ask.kit_id.clone(),
                                party_workers: ask.party_workers,
                                floor: ask.floor,
                                preset_floors: ask.preset_floors.clone(),
                                max_party_workers: ask.max_party_workers,
                            })
                        }
                        QueryPayload::DenialRaidForecast(ask) => {
                            pb::query_command::Query::DenialRaidForecast(
                                pb::DenialRaidForecastQuery {
                                    faction_id: ask.faction_id,
                                    band_id: ask.band_id,
                                    herd_id: ask.herd_id.clone(),
                                    kit_id: ask.kit_id.clone(),
                                    party_workers: ask.party_workers,
                                    max_party_workers: ask.max_party_workers,
                                },
                            )
                        }
                        QueryPayload::HuntCrewTake(ask) => {
                            pb::query_command::Query::HuntCrewTake(pb::HuntCrewTakeQuery {
                                faction_id: ask.faction_id,
                                band_id: ask.band_id,
                                herd_id: ask.herd_id.clone(),
                                kit_id: ask.kit_id.clone(),
                                floor: ask.floor,
                                max_workers: ask.max_workers,
                            })
                        }
                    }),
                })
            }
        });

        pb::CommandEnvelope {
            command,
            correlation_id: self.correlation_id,
        }
    }

    /// Attempt to build a high-level envelope from the protobuf representation.
    pub fn try_from_proto(proto: pb::CommandEnvelope) -> Result<Self, CommandDecodeError> {
        let payload = match proto.command.ok_or(CommandDecodeError::MissingPayload)? {
            pb::command_envelope::Command::Turn(cmd) => CommandPayload::Turn { steps: cmd.steps },
            pb::command_envelope::Command::ResetMap(cmd) => CommandPayload::ResetMap {
                width: cmd.width,
                height: cmd.height,
            },
            pb::command_envelope::Command::Orders(cmd) => CommandPayload::Orders {
                faction_id: cmd.faction_id,
                directive: OrdersDirective::try_from(cmd.directive)?,
            },
            pb::command_envelope::Command::Rollback(cmd) => {
                CommandPayload::Rollback { tick: cmd.tick }
            }
            pb::command_envelope::Command::UpdateEspionageGenerators(cmd) => {
                let mut updates = Vec::with_capacity(cmd.updates.len());
                for update in cmd.updates {
                    let per_faction = match update.per_faction {
                        Some(value) if value <= u8::MAX as u32 => Some(value as u8),
                        Some(value) => {
                            return Err(CommandDecodeError::InvalidEnum {
                                field: "EspionageGeneratorUpdate.per_faction",
                                value: value as i32,
                            })
                        }
                        None => None,
                    };
                    updates.push(EspionageGeneratorUpdate {
                        template_id: update.template_id,
                        enabled: update.enabled,
                        per_faction,
                    });
                }
                CommandPayload::UpdateEspionageGenerators { updates }
            }
            pb::command_envelope::Command::QueueEspionageMission(cmd) => {
                let target_tier = match cmd.target_tier {
                    Some(value) if value <= u8::MAX as u32 => Some(value as u8),
                    Some(value) => {
                        return Err(CommandDecodeError::InvalidEnum {
                            field: "QueueEspionageMissionCommand.target_tier",
                            value: value as i32,
                        })
                    }
                    None => None,
                };
                CommandPayload::QueueEspionageMission {
                    mission_id: cmd.mission_id,
                    owner_faction: cmd.owner_faction,
                    target_owner_faction: cmd.target_owner_faction,
                    discovery_id: cmd.discovery_id,
                    agent_handle: cmd.agent_handle,
                    target_tier,
                    scheduled_tick: cmd.scheduled_tick,
                }
            }
            pb::command_envelope::Command::UpdateEspionageQueueDefaults(cmd) => {
                let target_tier = match cmd.target_tier {
                    Some(value) if value <= u8::MAX as u32 => Some(value as u8),
                    Some(value) => {
                        return Err(CommandDecodeError::InvalidEnum {
                            field: "UpdateEspionageQueueDefaultsCommand.target_tier",
                            value: value as i32,
                        })
                    }
                    None => None,
                };
                CommandPayload::UpdateEspionageQueueDefaults {
                    scheduled_tick_offset: cmd.scheduled_tick_offset,
                    target_tier,
                }
            }
            pb::command_envelope::Command::UpdateCounterIntelPolicy(cmd) => {
                let policy = security_policy_kind_from_proto(cmd.policy)?;
                CommandPayload::UpdateCounterIntelPolicy {
                    faction: cmd.faction,
                    policy,
                }
            }
            pb::command_envelope::Command::AdjustCounterIntelBudget(cmd) => {
                CommandPayload::AdjustCounterIntelBudget {
                    faction: cmd.faction,
                    reserve: cmd.reserve,
                    delta: cmd.delta,
                }
            }
            pb::command_envelope::Command::ReloadConfig(cmd) => {
                let kind = reload_config_kind_from_proto(cmd.kind)?;
                CommandPayload::ReloadConfig {
                    kind,
                    path: cmd.path,
                }
            }
            pb::command_envelope::Command::SetCrisisAutoSeed(cmd) => {
                CommandPayload::SetCrisisAutoSeed {
                    enabled: cmd.enabled,
                }
            }
            pb::command_envelope::Command::SetFog(cmd) => CommandPayload::SetFogEnabled {
                enabled: cmd.enabled,
            },
            pb::command_envelope::Command::Resync(_) => CommandPayload::Resync,
            pb::command_envelope::Command::SpawnCrisis(cmd) => CommandPayload::SpawnCrisis {
                faction_id: cmd.faction,
                archetype_id: cmd.archetype_id,
            },
            pb::command_envelope::Command::SetStartProfile(cmd) => {
                CommandPayload::SetStartProfile {
                    profile_id: cmd.profile_id,
                }
            }
            pb::command_envelope::Command::ScoutArea(cmd) => CommandPayload::ScoutArea {
                faction_id: cmd.faction_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
                band_id: cmd.band_id,
            },
            pb::command_envelope::Command::FollowHerd(cmd) => CommandPayload::FollowHerd {
                faction_id: cmd.faction_id,
                herd_id: cmd.herd_id,
                policy: cmd.policy,
                band_id: cmd.band_id,
            },
            pb::command_envelope::Command::FoundSettlement(cmd) => {
                CommandPayload::FoundSettlement {
                    faction_id: cmd.faction_id,
                    target_x: cmd.target_x,
                    target_y: cmd.target_y,
                }
            }
            pb::command_envelope::Command::ForageTile(cmd) => CommandPayload::ForageTile {
                faction_id: cmd.faction_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
                module: cmd.module,
                band_id: cmd.band_id,
            },
            pb::command_envelope::Command::HuntGame(cmd) => CommandPayload::HuntGame {
                faction_id: cmd.faction_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
                band_id: cmd.band_id,
            },
            pb::command_envelope::Command::HuntFauna(cmd) => CommandPayload::HuntFauna {
                faction_id: cmd.faction_id,
                herd_id: cmd.herd_id,
                band_id: cmd.band_id,
            },
            pb::command_envelope::Command::Tame(cmd) => CommandPayload::Tame {
                faction_id: cmd.faction_id,
                herd_id: cmd.herd_id,
            },
            pb::command_envelope::Command::Cultivate(cmd) => CommandPayload::Cultivate {
                faction_id: cmd.faction_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
            },
            pb::command_envelope::Command::Sow(cmd) => CommandPayload::Sow {
                faction_id: cmd.faction_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
            },
            pb::command_envelope::Command::Corral(cmd) => CommandPayload::Corral {
                faction_id: cmd.faction_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
            },
            pb::command_envelope::Command::Abandon(cmd) => CommandPayload::Abandon {
                faction_id: cmd.faction_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
                herd_id: cmd.herd_id,
            },
            pb::command_envelope::Command::Unqueue(cmd) => CommandPayload::Unqueue {
                faction_id: cmd.faction_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
                herd_id: cmd.herd_id,
            },
            pb::command_envelope::Command::BuildOrder(cmd) => CommandPayload::BuildOrder {
                faction_id: cmd.faction_id,
                band_id: cmd.band_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
                herd_id: cmd.herd_id,
                position: cmd.position,
            },
            pb::command_envelope::Command::BuildKit(cmd) => CommandPayload::BuildKit {
                faction_id: cmd.faction_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
                herd_id: cmd.herd_id,
                kit_id: cmd.kit_id,
            },
            pb::command_envelope::Command::UpkeepKit(cmd) => CommandPayload::UpkeepKit {
                faction_id: cmd.faction_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
                herd_id: cmd.herd_id,
                kit_id: cmd.kit_id,
            },
            pb::command_envelope::Command::WorkPriority(cmd) => CommandPayload::WorkPriority {
                faction_id: cmd.faction_id,
                band_id: cmd.band_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
                herd_id: cmd.herd_id,
                level: cmd.level,
            },
            pb::command_envelope::Command::UpkeepMode(cmd) => CommandPayload::UpkeepMode {
                faction_id: cmd.faction_id,
                band_id: cmd.band_id,
                mode: cmd.mode,
            },
            pb::command_envelope::Command::SetBench(cmd) => CommandPayload::SetBench {
                faction_id: cmd.faction_id,
                band_id: cmd.band_id,
                recipe_id: cmd.recipe_id,
                workers: cmd.workers,
            },
            pb::command_envelope::Command::ClearBench(cmd) => CommandPayload::ClearBench {
                faction_id: cmd.faction_id,
                band_id: cmd.band_id,
            },
            pb::command_envelope::Command::BenchCrew(cmd) => CommandPayload::BenchCrew {
                faction_id: cmd.faction_id,
                band_id: cmd.band_id,
                workers: cmd.workers,
            },
            pb::command_envelope::Command::BenchPriority(cmd) => CommandPayload::BenchPriority {
                faction_id: cmd.faction_id,
                band_id: cmd.band_id,
                level: cmd.level,
            },
            pb::command_envelope::Command::ExtendPen(cmd) => CommandPayload::ExtendPen {
                faction_id: cmd.faction_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
            },
            pb::command_envelope::Command::AnswerFork(cmd) => CommandPayload::AnswerFork {
                faction_id: cmd.faction_id,
                beat_id: cmd.beat_id,
                choice_id: cmd.choice_id,
            },
            pb::command_envelope::Command::CancelOrder(cmd) => CommandPayload::CancelOrder {
                faction_id: cmd.faction_id,
                band_id: cmd.band_id,
                // Absent legitimately means "all", and the wire is not a boundary worth hard-failing
                // on: an unrecognised token degrades to the historical clear-everything behaviour.
                scope: cmd
                    .scope
                    .as_deref()
                    .and_then(CancelScope::parse)
                    .unwrap_or_default(),
            },
            pb::command_envelope::Command::AssignLabor(cmd) => CommandPayload::AssignLabor {
                faction_id: cmd.faction_id,
                band_id: cmd.band_id,
                role: cmd.role,
                workers: cmd.workers,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
                fauna_id: cmd.fauna_id,
                policy: cmd.policy,
                species: cmd.species,
                floor: cmd.floor,
                kit_id: cmd.kit_id,
                take_species: cmd.take_species,
            },
            pb::command_envelope::Command::MoveBand(cmd) => CommandPayload::MoveBand {
                faction_id: cmd.faction_id,
                band_id: cmd.band_id,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
            },
            pb::command_envelope::Command::SendExpedition(cmd) => CommandPayload::SendExpedition {
                faction_id: cmd.faction_id,
                band_id: cmd.band_id,
                party_workers: cmd.party_workers,
                target_x: cmd.target_x,
                target_y: cmd.target_y,
            },
            pb::command_envelope::Command::RecallExpedition(cmd) => {
                CommandPayload::RecallExpedition {
                    faction_id: cmd.faction_id,
                    expedition_band_id: cmd.expedition_band_id,
                }
            }
            pb::command_envelope::Command::SplitBand(cmd) => CommandPayload::SplitBand {
                faction_id: cmd.faction_id,
                band_id: cmd.band_id,
                workers: cmd.workers,
            },
            pb::command_envelope::Command::SendHuntExpedition(cmd) => {
                CommandPayload::SendHuntExpedition {
                    faction_id: cmd.faction_id,
                    band_id: cmd.band_id,
                    party_workers: cmd.party_workers,
                    fauna_id: cmd.fauna_id,
                    floor: cmd.floor,
                    kit_id: cmd.kit_id,
                }
            }
            pb::command_envelope::Command::SendDenialRaid(cmd) => CommandPayload::SendDenialRaid {
                faction_id: cmd.faction_id,
                band_id: cmd.band_id,
                party_workers: cmd.party_workers,
                fauna_id: cmd.fauna_id,
                kit_id: cmd.kit_id,
            },
            pb::command_envelope::Command::SendTradeExpedition(cmd) => {
                CommandPayload::SendTradeExpedition {
                    faction_id: cmd.faction_id,
                    band_id: cmd.band_id,
                    party_workers: cmd.party_workers,
                    destination_band_id: cmd.destination_band_id,
                    cargo: cmd
                        .cargo
                        .into_iter()
                        .map(|item| TradeCargoItem {
                            id: item.id,
                            is_material: item.is_material,
                            amount: item.amount,
                        })
                        .collect(),
                    kit_id: cmd.kit_id,
                }
            }
            pb::command_envelope::Command::ExportMap(cmd) => {
                CommandPayload::ExportMap { path: cmd.path }
            }
            pb::command_envelope::Command::NewGame(cmd) => CommandPayload::NewGame {
                preset_id: cmd.preset_id,
                width: cmd.width,
                height: cmd.height,
                seed: cmd.seed,
                profile_id: cmd.profile_id,
            },
            pb::command_envelope::Command::SetConfigOverride(cmd) => {
                CommandPayload::SetConfigOverride {
                    kind: config_override_kind_from_proto(cmd.kind)?,
                    patch_json: cmd.patch_json,
                }
            }
            pb::command_envelope::Command::ClearConfigOverrides(_) => {
                CommandPayload::ClearConfigOverrides
            }
            pb::command_envelope::Command::Query(cmd) => {
                // A query with no question is as empty as an envelope with no command, and fails the
                // same way: there is nothing to answer and nothing to guess.
                let query = match cmd.query.ok_or(CommandDecodeError::MissingPayload)? {
                    pb::query_command::Query::HuntTripForecast(ask) => {
                        QueryPayload::HuntTripForecast(HuntTripForecastQuery {
                            faction_id: ask.faction_id,
                            band_id: ask.band_id,
                            herd_id: ask.herd_id,
                            kit_id: ask.kit_id,
                            party_workers: ask.party_workers,
                            floor: ask.floor,
                            preset_floors: ask.preset_floors,
                            max_party_workers: ask.max_party_workers,
                        })
                    }
                    pb::query_command::Query::DenialRaidForecast(ask) => {
                        QueryPayload::DenialRaidForecast(DenialRaidForecastQuery {
                            faction_id: ask.faction_id,
                            band_id: ask.band_id,
                            herd_id: ask.herd_id,
                            kit_id: ask.kit_id,
                            party_workers: ask.party_workers,
                            max_party_workers: ask.max_party_workers,
                        })
                    }
                    pb::query_command::Query::HuntCrewTake(ask) => {
                        QueryPayload::HuntCrewTake(HuntCrewTakeQuery {
                            faction_id: ask.faction_id,
                            band_id: ask.band_id,
                            herd_id: ask.herd_id,
                            kit_id: ask.kit_id,
                            floor: ask.floor,
                            max_workers: ask.max_workers,
                        })
                    }
                };
                CommandPayload::Query {
                    request_id: cmd.request_id,
                    query,
                }
            }
        };

        Ok(CommandEnvelope {
            payload,
            correlation_id: proto.correlation_id,
        })
    }
}

impl QueryReplyEnvelope {
    /// Encode the reply into a protobuf binary frame — what the server's per-connection writer
    /// thread length-prefixes onto the command socket.
    pub fn encode_to_vec(&self) -> Result<Vec<u8>, CommandEncodeError> {
        let proto = self.to_proto();
        let mut buffer = Vec::with_capacity(proto.encoded_len());
        proto.encode(&mut buffer)?;
        Ok(buffer)
    }

    /// Decode a reply frame. The client's half of the round trip, and what the transport test reads
    /// back off the socket.
    pub fn decode(bytes: &[u8]) -> Result<Self, CommandDecodeError> {
        let proto = pb::QueryReplyEnvelope::decode(bytes)?;
        Self::try_from_proto(proto)
    }

    fn to_proto(&self) -> pb::QueryReplyEnvelope {
        let reply = Some(match &self.reply {
            QueryReply::HuntTripForecast(answer) => {
                pb::query_reply_envelope::Reply::HuntTripForecast(pb::HuntTripForecastReply {
                    at_composed: Some(hunt_trip_row_to_proto(&answer.at_composed)),
                    per_preset: answer
                        .per_preset
                        .iter()
                        .map(hunt_trip_row_to_proto)
                        .collect(),
                    useful_cap: answer.useful_cap,
                })
            }
            QueryReply::DenialRaidForecast(answer) => {
                pb::query_reply_envelope::Reply::DenialRaidForecast(pb::DenialRaidForecastReply {
                    at_composed: Some(denial_row_to_proto(&answer.at_composed)),
                    party_needed: answer.party_needed,
                })
            }
            QueryReply::HuntCrewTake(answer) => {
                pb::query_reply_envelope::Reply::HuntCrewTake(pb::HuntCrewTakeReply {
                    per_crew: answer
                        .per_crew
                        .iter()
                        .map(|row| pb::HuntCrewTakeRow {
                            workers: row.workers,
                            animals_low: row.animals_low,
                            animals_likely: row.animals_likely,
                            animals_high: row.animals_high,
                        })
                        .collect(),
                })
            }
            QueryReply::Error(reason) => pb::query_reply_envelope::Reply::Error(pb::QueryError {
                reason: reason.clone(),
            }),
        });
        pb::QueryReplyEnvelope {
            request_id: self.request_id,
            reply,
        }
    }

    fn try_from_proto(proto: pb::QueryReplyEnvelope) -> Result<Self, CommandDecodeError> {
        let reply = match proto.reply.ok_or(CommandDecodeError::MissingPayload)? {
            pb::query_reply_envelope::Reply::HuntTripForecast(answer) => {
                QueryReply::HuntTripForecast(HuntTripForecastReply {
                    at_composed: hunt_trip_row_from_proto(
                        answer
                            .at_composed
                            .ok_or(CommandDecodeError::MissingPayload)?,
                    ),
                    per_preset: answer
                        .per_preset
                        .into_iter()
                        .map(hunt_trip_row_from_proto)
                        .collect(),
                    useful_cap: answer.useful_cap,
                })
            }
            pb::query_reply_envelope::Reply::DenialRaidForecast(answer) => {
                QueryReply::DenialRaidForecast(DenialRaidForecastReply {
                    at_composed: denial_row_from_proto(
                        answer
                            .at_composed
                            .ok_or(CommandDecodeError::MissingPayload)?,
                    ),
                    party_needed: answer.party_needed,
                })
            }
            pb::query_reply_envelope::Reply::HuntCrewTake(answer) => {
                QueryReply::HuntCrewTake(HuntCrewTakeReply {
                    per_crew: answer
                        .per_crew
                        .into_iter()
                        .map(|row| HuntCrewTakeRow {
                            workers: row.workers,
                            animals_low: row.animals_low,
                            animals_likely: row.animals_likely,
                            animals_high: row.animals_high,
                        })
                        .collect(),
                })
            }
            pb::query_reply_envelope::Reply::Error(error) => QueryReply::Error(error.reason),
        };
        Ok(QueryReplyEnvelope {
            request_id: proto.request_id,
            reply,
        })
    }
}

fn hunt_trip_row_to_proto(row: &HuntTripRow) -> pb::HuntTripRow {
    pb::HuntTripRow {
        floor: row.floor,
        party_workers: row.party_workers,
        turns_to_fill: row.turns_to_fill,
        bound: row.bound.clone(),
        delivers_food: row.delivers_food,
        animals_taken: row.animals_taken,
        delivered_food: row.delivered_food,
        wasted_food: row.wasted_food,
        delivered_material: row
            .delivered_material
            .iter()
            .map(|payoff| pb::MaterialPayoff {
                material_id: payoff.material_id.clone(),
                amount: payoff.amount,
            })
            .collect(),
    }
}

fn hunt_trip_row_from_proto(row: pb::HuntTripRow) -> HuntTripRow {
    HuntTripRow {
        floor: row.floor,
        party_workers: row.party_workers,
        turns_to_fill: row.turns_to_fill,
        bound: row.bound,
        delivers_food: row.delivers_food,
        animals_taken: row.animals_taken,
        delivered_food: row.delivered_food,
        wasted_food: row.wasted_food,
        delivered_material: row
            .delivered_material
            .into_iter()
            .map(|payoff| MaterialPayoff {
                material_id: payoff.material_id,
                amount: payoff.amount,
            })
            .collect(),
    }
}

fn denial_row_to_proto(row: &DenialRow) -> pb::DenialRow {
    pb::DenialRow {
        party_workers: row.party_workers,
        turns_to_collapse: row.turns_to_collapse,
        turns_to_collapse_low: row.turns_to_collapse_low,
        turns_to_collapse_high: row.turns_to_collapse_high,
        outcome: row.outcome.clone(),
        animals_killed: row.animals_killed,
        delivered_food: row.delivered_food,
        wasted_food: row.wasted_food,
        delivered_material: row
            .delivered_material
            .iter()
            .map(|payoff| pb::MaterialPayoff {
                material_id: payoff.material_id.clone(),
                amount: payoff.amount,
            })
            .collect(),
    }
}

fn denial_row_from_proto(row: pb::DenialRow) -> DenialRow {
    DenialRow {
        party_workers: row.party_workers,
        turns_to_collapse: row.turns_to_collapse,
        turns_to_collapse_low: row.turns_to_collapse_low,
        turns_to_collapse_high: row.turns_to_collapse_high,
        outcome: row.outcome,
        animals_killed: row.animals_killed,
        delivered_food: row.delivered_food,
        wasted_food: row.wasted_food,
        delivered_material: row
            .delivered_material
            .into_iter()
            .map(|payoff| MaterialPayoff {
                material_id: payoff.material_id,
                amount: payoff.amount,
            })
            .collect(),
    }
}

impl TryFrom<i32> for OrdersDirective {
    type Error = CommandDecodeError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match pb::OrdersDirective::try_from(value) {
            Ok(pb::OrdersDirective::Ready) => Ok(OrdersDirective::Ready),
            _ => Err(CommandDecodeError::InvalidEnum {
                field: "OrdersDirective",
                value,
            }),
        }
    }
}

impl From<OrdersDirective> for pb::OrdersDirective {
    fn from(value: OrdersDirective) -> Self {
        match value {
            OrdersDirective::Ready => pb::OrdersDirective::Ready,
        }
    }
}

fn orders_directive_to_proto(value: OrdersDirective) -> pb::OrdersDirective {
    value.into()
}

fn security_policy_kind_to_proto(value: SecurityPolicyKind) -> pb::SecurityPolicyKind {
    match value {
        SecurityPolicyKind::Lenient => pb::SecurityPolicyKind::Lenient,
        SecurityPolicyKind::Standard => pb::SecurityPolicyKind::Standard,
        SecurityPolicyKind::Hardened => pb::SecurityPolicyKind::Hardened,
        SecurityPolicyKind::Crisis => pb::SecurityPolicyKind::Crisis,
    }
}

fn reload_config_kind_to_proto(kind: ReloadConfigKind) -> pb::ReloadConfigKind {
    match kind {
        ReloadConfigKind::Simulation => pb::ReloadConfigKind::Simulation,
        ReloadConfigKind::TurnPipeline => pb::ReloadConfigKind::TurnPipeline,
        ReloadConfigKind::SnapshotOverlays => pb::ReloadConfigKind::SnapshotOverlays,
        ReloadConfigKind::CrisisArchetypes => pb::ReloadConfigKind::CrisisArchetypes,
        ReloadConfigKind::CrisisModifiers => pb::ReloadConfigKind::CrisisModifiers,
        ReloadConfigKind::CrisisTelemetry => pb::ReloadConfigKind::CrisisTelemetry,
    }
}

fn config_override_kind_to_proto(kind: ConfigOverrideKind) -> pb::ConfigOverrideKind {
    match kind {
        ConfigOverrideKind::Simulation => pb::ConfigOverrideKind::Simulation,
        ConfigOverrideKind::Labor => pb::ConfigOverrideKind::Labor,
        ConfigOverrideKind::Demographics => pb::ConfigOverrideKind::Demographics,
        ConfigOverrideKind::Expedition => pb::ConfigOverrideKind::Expedition,
        ConfigOverrideKind::Combat => pb::ConfigOverrideKind::Combat,
        ConfigOverrideKind::Materials => pb::ConfigOverrideKind::Materials,
        ConfigOverrideKind::Recipes => pb::ConfigOverrideKind::Recipes,
    }
}

fn config_override_kind_from_proto(value: i32) -> Result<ConfigOverrideKind, CommandDecodeError> {
    match pb::ConfigOverrideKind::try_from(value) {
        Ok(pb::ConfigOverrideKind::Simulation) => Ok(ConfigOverrideKind::Simulation),
        Ok(pb::ConfigOverrideKind::Labor) => Ok(ConfigOverrideKind::Labor),
        Ok(pb::ConfigOverrideKind::Demographics) => Ok(ConfigOverrideKind::Demographics),
        Ok(pb::ConfigOverrideKind::Expedition) => Ok(ConfigOverrideKind::Expedition),
        Ok(pb::ConfigOverrideKind::Combat) => Ok(ConfigOverrideKind::Combat),
        Ok(pb::ConfigOverrideKind::Materials) => Ok(ConfigOverrideKind::Materials),
        Ok(pb::ConfigOverrideKind::Recipes) => Ok(ConfigOverrideKind::Recipes),
        Ok(pb::ConfigOverrideKind::Unspecified) | Err(_) => Err(CommandDecodeError::InvalidEnum {
            field: "ConfigOverrideKind",
            value,
        }),
    }
}

fn security_policy_kind_from_proto(value: i32) -> Result<SecurityPolicyKind, CommandDecodeError> {
    match pb::SecurityPolicyKind::try_from(value) {
        Ok(pb::SecurityPolicyKind::Lenient) => Ok(SecurityPolicyKind::Lenient),
        Ok(pb::SecurityPolicyKind::Standard) => Ok(SecurityPolicyKind::Standard),
        Ok(pb::SecurityPolicyKind::Hardened) => Ok(SecurityPolicyKind::Hardened),
        Ok(pb::SecurityPolicyKind::Crisis) => Ok(SecurityPolicyKind::Crisis),
        _ => Err(CommandDecodeError::InvalidEnum {
            field: "SecurityPolicyKind",
            value,
        }),
    }
}

fn reload_config_kind_from_proto(value: i32) -> Result<ReloadConfigKind, CommandDecodeError> {
    match pb::ReloadConfigKind::try_from(value) {
        Ok(pb::ReloadConfigKind::Simulation) => Ok(ReloadConfigKind::Simulation),
        Ok(pb::ReloadConfigKind::TurnPipeline) => Ok(ReloadConfigKind::TurnPipeline),
        Ok(pb::ReloadConfigKind::SnapshotOverlays) => Ok(ReloadConfigKind::SnapshotOverlays),
        Ok(pb::ReloadConfigKind::CrisisArchetypes) => Ok(ReloadConfigKind::CrisisArchetypes),
        Ok(pb::ReloadConfigKind::CrisisModifiers) => Ok(ReloadConfigKind::CrisisModifiers),
        Ok(pb::ReloadConfigKind::CrisisTelemetry) => Ok(ReloadConfigKind::CrisisTelemetry),
        Ok(pb::ReloadConfigKind::Unspecified) | Err(_) => Err(CommandDecodeError::InvalidEnum {
            field: "ReloadConfigKind",
            value,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The patch rides the wire as an opaque string, so the only thing that can break it is the
    /// envelope plumbing — which this covers for the whole staged-override pair.
    #[test]
    fn config_override_commands_round_trip_through_the_envelope() {
        for payload in [
            CommandPayload::SetConfigOverride {
                kind: ConfigOverrideKind::Combat,
                patch_json: r#"{"lethality": 1.5}"#.to_string(),
            },
            CommandPayload::ClearConfigOverrides,
        ] {
            let envelope = CommandEnvelope {
                payload: payload.clone(),
                correlation_id: None,
            };
            let bytes = envelope.encode_to_vec().expect("encode");
            let decoded = CommandEnvelope::decode(&bytes).expect("decode");
            assert_eq!(decoded.payload, payload);
        }
    }

    /// **The crew-take curve survives the wire, question and answer both.**
    ///
    /// Every field is given a DISTINCT value — `low < likely < high`, ascending crews, a floor that
    /// is not a round number — because the failure this guards is a transposition, and two fields
    /// that happen to carry the same number cannot detect being swapped. The proto and the Rust
    /// mirror are hand-written on both sides of a generated struct; nothing but this notices when
    /// one of them is edited and the other is not.
    #[test]
    fn the_crew_take_curve_round_trips_through_the_wire() {
        let payload = CommandPayload::Query {
            request_id: 42,
            query: QueryPayload::HuntCrewTake(HuntCrewTakeQuery {
                faction_id: 3,
                band_id: 9_001,
                herd_id: "aurochs_north".to_string(),
                kit_id: "big_game".to_string(),
                floor: 0.375,
                max_workers: 12,
            }),
        };
        let envelope = CommandEnvelope {
            payload: payload.clone(),
            correlation_id: None,
        };
        let bytes = envelope.encode_to_vec().expect("encode");
        assert_eq!(
            CommandEnvelope::decode(&bytes).expect("decode").payload,
            payload
        );

        let reply = QueryReplyEnvelope {
            request_id: 42,
            reply: QueryReply::HuntCrewTake(HuntCrewTakeReply {
                per_crew: (1..=3)
                    .map(|workers| HuntCrewTakeRow {
                        workers,
                        animals_low: workers as f32 * 0.25,
                        animals_likely: workers as f32 * 0.5,
                        animals_high: workers as f32 * 0.75,
                    })
                    .collect(),
            }),
        };
        let bytes = reply.encode_to_vec().expect("encode");
        assert_eq!(QueryReplyEnvelope::decode(&bytes).expect("decode"), reply);
    }

    /// **The split rides the wire as a `BandId`**, and it has to survive the envelope intact: the
    /// command is replayed out of the log after a rollback, where an entity handle would resolve to
    /// nothing but an id still names the same band.
    #[test]
    fn split_band_round_trips_through_the_envelope() {
        let payload = CommandPayload::SplitBand {
            faction_id: 0,
            band_id: Some(9_001),
            workers: 6,
        };
        let envelope = CommandEnvelope {
            payload: payload.clone(),
            correlation_id: None,
        };
        let bytes = envelope.encode_to_vec().expect("encode");
        let decoded = CommandEnvelope::decode(&bytes).expect("decode");
        assert_eq!(decoded.payload, payload);
    }

    /// `0` is the protobuf default, so an unset `kind` field decodes as UNSPECIFIED. Accepting it
    /// would silently retune whichever config happened to be first in the table.
    #[test]
    fn an_unspecified_config_override_kind_is_rejected() {
        let envelope = pb::CommandEnvelope {
            correlation_id: None,
            command: Some(pb::command_envelope::Command::SetConfigOverride(
                pb::SetConfigOverrideCommand {
                    kind: pb::ConfigOverrideKind::Unspecified as i32,
                    patch_json: "{}".to_string(),
                },
            )),
        };
        let mut bytes = Vec::new();
        prost::Message::encode(&envelope, &mut bytes).expect("encode");
        assert!(matches!(
            CommandEnvelope::decode(&bytes),
            Err(CommandDecodeError::InvalidEnum {
                field: "ConfigOverrideKind",
                ..
            })
        ));
    }
}
