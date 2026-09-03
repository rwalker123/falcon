use std::collections::{BTreeMap, BTreeSet};

use super::*;

/// **"Is this crew actually working the source?"** — THE eligibility term that replaced the
/// `EcologyPhase::Thriving` gate on both webs (`docs/plan_harvest_floor.md` §3.2), asked of the
/// **escapement room**: is there anything standing above this assignment's floor?
///
/// # It is the CEILING, deliberately, and not the take
///
/// The obvious spelling is `take > 0`, and on the plant web the two coincide — a gather is
/// continuous, so any positive room yields a positive take. **On the animal web they do not**, and
/// the difference is a quantisation artifact rather than a fact about work.
/// [`crate::fauna::quantise_animal_take`] rounds to whole animals, so a herd whose room is 60 biomass
/// against an 80-unit body hands over **nothing** this turn while the crew tracks, culls and handles
/// it exactly as they did last turn. Reading `AnimalTake::killed == 0` as *"not working"* would make
/// the learning and build rates depend on `body_mass`: big-bodied species would tame and teach
/// several times slower than small ones, for a reason nobody designed and nothing measured.
///
/// Asked **in biomass, before quantisation and before the worker cap** — the number
/// `forage::forage_escapement_ceiling` / `fauna::hunt_escapement_ceiling` returns — so it is the same
/// question on both webs and the rule stops being web-specific.
///
/// It still separates the two cases the gate exists to separate:
/// - **nothing stands above your floor** → you are watching, not working. No lesson, no build. That
///   is also what makes `floor = 1.0` degenerate: the room is `0` by construction.
/// - **there is surplus you have not yet banked into a whole body** → you are working it, and the
///   pulse the quantiser pays is about *when* the food lands, not whether the crew showed up.
///
/// The other degenerate end is [`crate::intensification::learn_multiplier`]'s, not this one's:
/// `floor = 0` leaves nothing standing, so the *rate* is zero however much room there was.
fn crew_is_working_the_source(standing_above_floor: f32) -> bool {
    standing_above_floor > NOTHING_STANDS_ABOVE_THE_FLOOR
}

/// **An empty escapement room** — the value [`crew_is_working_the_source`] compares against, named
/// because `0.0` as a bare literal there reads as an arbitrary epsilon rather than as the exact
/// boundary `max(0, B − floor·K)` is clamped at.
const NOTHING_STANDS_ABOVE_THE_FLOOR: f32 = 0.0;

/// **A CLAIM THAT ASKS FOR NOTHING** — the boundary [`settle_scarce_store`] skips a tier at and the
/// value it settles an unserved claim to. Named for [`NOTHING_STANDS_ABOVE_THE_FLOOR`]'s reason: a
/// bare `0.0` there reads as an epsilon rather than as the exact "no demand" boundary.
pub(crate) const NOTHING_DEMANDED: f32 = 0.0;

/// **THE WHOLE OF A CLAIM'S DEMAND** — the cap on [`settle_scarce_store`]'s per-tier served
/// fraction, so a tier the remaining store covers is paid in full and never more than once.
pub(crate) const FULLY_SERVED: f32 = 1.0;

/// **A BAND WITH NO HAY LEDGER AT ALL** — what `LaborAllocation`'s three fodder rates are cleared to
/// at the top of every band's turn, before the exits that can end it without reaching the re-sum.
/// Named for [`NOTHING_DEMANDED`]'s reason: this is the exact "keeps no pens, works no Fields"
/// reading and not a small quantity of hay.
const NO_FODDER_LEDGER: f32 = 0.0;

/// **A BAND STANDING ON NO ROAD AT ALL** — what `LaborAllocation`'s two roadwork rates are cleared
/// to at the top of every band's iteration in [`settle_bands_roadwork`], before the exits that can
/// end it without reaching the sum. [`NO_FODDER_LEDGER`]'s twin, and named for its reason: this is
/// the exact *"keeps no roads"* reading and not a small quantity of work.
const NO_ROADWORK_LEDGER: f32 = 0.0;

/// **"Is there anything here for this crew to work with?"** — THE eligibility term a **build** is
/// gated on, asked of [`crate::fauna::take_room`]: the escapement room **or** the share of this
/// turn's growth the player's own floor left takeable, whichever is larger.
///
/// # ⛔ IT IS A DIFFERENT QUESTION FROM [`crew_is_working_the_source`], AND THE SPLIT IS THE POINT
///
/// The two were one bool, and that is what stalled a tame forever. **A rung raises `K`** — a tamed
/// herd's `pastoral_density`, a sown field's `field_capacity_gain` — so `floor · K` climbs while the
/// stock does not, and a source standing *exactly* on its floor when a build starts is pushed
/// **below** it by its own improvement. The build's gate then read that empty room and refused the
/// very job that had moved it. Measured on an aurochs tame begun on the floor
/// (`forage::stance_probe`'s `probe_the_tame_floor_squeeze`): the room hit zero on turn 6 at one
/// herder, turn 3 at four and turn 2 at eight — **more hands made it worse** — and the tame never
/// completed at any crew size.
///
/// **Only this one widened.** [`crew_is_working_the_source`] still gates the **lesson** on the raw
/// escapement room, because `intensification::learn_multiplier`'s self-limit lives there: *"watching
/// teaches nothing"* at `floor = 1.0` is what stops a near-`1.0` floor farming knowledge at ×2 for
/// free, and its own doc forbids clamping it. Widening the shared predicate would have opened that;
/// widening this one cannot reach it.
///
/// **A legal build target that yields nothing is now unrepresentable**, not merely avoided: this
/// reads the *same* number the take is bounded by, so the two cannot drift apart when either is
/// retuned. At `floor = 1.0` the growth share is `× 0` and the room is `0`, so this refuses — which
/// is *"leave the whole source standing"* meaning exactly that, with no special case.
fn source_is_workable(take_room: f32) -> bool {
    take_room > NOTHING_STANDS_ABOVE_THE_FLOOR
}

/// **Credit the lesson the source's rung teaches, at the rate its crew's floor earns.** The caller
/// side of [`RungDef::knowledge_accrual`]: the rung says *what* is learned and *how much*, this
/// applies it to the ledger.
///
/// It exists as a function rather than as one hoisted call because **`eligible` is not knowable
/// until the source's own branch is reached** — a Field and a pen answer it differently from a wild
/// stand (see [`credit_managed_rung_lesson`]), and the extractive branches need the escapement room
/// resolved against the pre-take biomass. Each of the four branches (a Field, a wild gather, a pen's
/// tend, a wild hunt) calls this with its own answer; the *rule* stays in one place so the two webs
/// cannot drift.
fn credit_rung_lesson(
    rung: &RungDef,
    floor: f32,
    eligible: bool,
    knowledge: &LadderKnowledge,
    faction: FactionId,
    discovery: &mut DiscoveryProgressLedger,
) {
    if let Some((lesson, amount)) = rung.knowledge_accrual(floor, eligible, knowledge) {
        discovery.add_progress(faction, lesson, scalar_from_f32(amount));
    }
}

/// **Credit a RUNG-3 MANAGED source's lesson** — a Field, a penned herd — where the floor axis has
/// **collapsed** and the crew is always working.
///
/// It exists so that fact has one home instead of two named constants passed positionally at four
/// call sites. A managed source's take is its `managed_production` at *every* floor
/// (`SourceYieldForecast::managed`), so:
///
/// - **the floor the assignment carries is inert**, and reading it would pace a keeper's learning by
///   a dial that changed nothing about what they took. The lesson runs at the food peak instead,
///   where [`crate::intensification::learn_multiplier`] is exactly `×1.0` — *"the pressure axis has
///   collapsed here"*, not *"the food peak happens to be right"*;
/// - **a keeper who is there is working it by definition**: there is no standing stock to be above
///   or below, so the escapement predicate every drawn-down branch has to evaluate collapses, and
///   all that is left of `eligible` is `crew_is_present` — *is anybody on the take at all*. A source
///   a band merely **holds** (`docs/plan_standing_upkeep.md` §2.2 — take, build and keeping are
///   three allocations) draws its share of the keeping pool and learns nothing, because the lesson
///   is credited per assignment rather than per worker and would otherwise be free.
fn credit_managed_rung_lesson(
    rung: &RungDef,
    crew_is_present: bool,
    knowledge: &LadderKnowledge,
    faction: FactionId,
    discovery: &mut DiscoveryProgressLedger,
) {
    /// The floor at which `learn_multiplier` is the identity — the food peak.
    const THE_FLOOR_AXIS_HAS_COLLAPSED: f32 = crate::fauna::MSY_BIOMASS_FRACTION;
    credit_rung_lesson(
        rung,
        THE_FLOOR_AXIS_HAS_COLLAPSED,
        crew_is_present,
        knowledge,
        faction,
        discovery,
    );
}

/// **DOES THIS BAND STILL HAVE ANYTHING AT THIS SOURCE?** — the one predicate that decides whether a
/// row with no hands on any of its three activities is worth keeping (`docs/plan_standing_upkeep.md`
/// §2.2/§2.5).
///
/// A source row is the band's **holding**, so it survives losing its take crew — that separation is
/// the whole point of §2.2, and without it a finished Field whose gatherers moved on contributed no
/// demand to `agriculture`, drew no share, and bled its full rate with keepers idle in the role. What
/// bounds the rows is this: a holding lasts exactly as long as there is a **meter carrying progress**
/// on the ground, which is precisely what the keeping pool funds and what the decay pass bleeds
/// ([`crate::forage::patch_unwinding_rung`] / [`crate::fauna::herd_keeping_rung`]). A wild stand and a
/// herd nobody owns answer `false`, so unstaffing one really does end the band's business there.
///
/// **Asked at two moments, and it must be the same question at both**: the command, so
/// `assign_labor … 0` on a wild patch clears the row on the spot rather than leaving a `+0.00` row
/// the player has to watch age out; and the turn, so a holding whose meter finally rots away is
/// retired without the player having to touch it.
///
/// # ⛔ IT IS NOT THE KEEPING POOL'S ELIGIBILITY TEST — that is `*_keeping_meter`
///
/// It reads through `forage::patch_unwinding_rung` / `fauna::herd_keeping_rung`, which are
/// progress-only, and **every** caller pairs it with *"…or the source carries a queue entry"* — so a
/// build declared on bare ground keeps its row on the declaration alone. [`maintenance_shares`] used
/// to borrow it as its claim gate, where the entry term is not available in that shape, and the
/// missing half was exactly the turn a build banked its first work. What the pool funds is
/// `forage::patch_keeping_meter` / `fauna::herd_keeping_meter`, resolved with the verb, because that
/// is what the payment side resolves.
///
/// A band-wide role is never a holding — it *is* its head count — and answers `false`.
pub fn source_has_a_meter_at_risk(
    target: &LaborTarget,
    forage_registry: &ForageRegistry,
    herds: &HerdRegistry,
    ladder: &LadderConfig,
) -> bool {
    match target {
        LaborTarget::Forage { tile, .. } => forage_registry
            .patch(*tile)
            .is_some_and(|patch| crate::forage::patch_unwinding_rung(patch, ladder).is_some()),
        LaborTarget::Hunt { fauna_id, .. } => herds
            .find(fauna_id)
            .is_some_and(|herd| fauna::herd_keeping_rung(herd, ladder).is_some()),
        LaborTarget::Scout
        | LaborTarget::Warrior
        | LaborTarget::Agriculture
        | LaborTarget::Husbandry
        | LaborTarget::Roadwork
        | LaborTarget::Builders => false,
    }
}

/// The config handles [`advance_labor_allocation`] reads, bundled into one `SystemParam` so the
/// system stays under Bevy's 16-parameter ceiling as new configs join it (Predators Phase 0 added
/// combat + creatures). Each is resolved to its `Arc` once at the top of the system.
#[derive(bevy::ecs::system::SystemParam)]
pub struct LaborConfigs<'w> {
    pub fauna: Res<'w, FaunaConfigHandle>,
    pub labor: Res<'w, LaborConfigHandle>,
    pub flora: Res<'w, FloraConfigHandle>,
    pub ladder: Res<'w, LadderConfigHandle>,
    pub wellbeing: Res<'w, WellbeingConfigHandle>,
    pub combat: Res<'w, CombatConfigHandle>,
    pub creatures: Res<'w, CreaturesConfigHandle>,
    pub equipment: Res<'w, EquipmentConfigHandle>,
    /// The materials table — needed to turn a source's stated reading into the batch's merge key
    /// (`docs/plan_crafting_and_materials.md` §1). The *rows* come from the source's own config; only
    /// the banding lives here, because deriving a band needs the material's axis list.
    pub materials: Res<'w, crate::materials_config::MaterialsConfigHandle>,
    /// The recipe book — read for **one** thing here: what this band's bench will bank per turn
    /// ([`crate::systems::bench_material_rate`]), which is half the inflow the material-shortfall
    /// Alert judges against. The bench itself is `advance_crafting`'s.
    pub recipes: Res<'w, crate::recipes_config::RecipesConfigHandle>,
}

/// **WHAT EACH OF A BAND'S SOURCES GETS OUT OF ITS MAINTENANCE POOLS** — one work amount per
/// assignment index, in the allocation's own order (`docs/plan_standing_upkeep.md` §2.5).
///
/// # ONE POOL PER WEB, AGAINST THE SUM OF WHAT THE BAND HOLDS
///
/// The band staffs two standing roles — [`LaborTarget::Agriculture`] and
/// [`LaborTarget::Husbandry`] — and each role's hands are a **pool** measured against the summed
/// [`crate::forage::patch_upkeep_demand`] / [`crate::fauna::herd_upkeep_demand`] of every source on
/// that web. Nothing is wasted: the per-source keeper crew this replaced had to round a fractional
/// demand up to whole workers and threw the remainder away, once per source.
///
/// # EVERY METER CARRYING WORK DRAWS FROM IT, AT ANY FULLNESS
///
/// A source claims a share exactly where a meter **answers for its keeping** —
/// `forage::patch_keeping_meter` / `fauna::herd_keeping_meter`, the same resolver the stamp
/// (`patch_upkeep_supply` / `herd_upkeep_supply`) and the demand read. A source mid-Cultivate
/// contributes its demand and takes its share exactly as a finished one does.
///
/// **It used to be `source_has_a_meter_at_risk`, and that was a SECOND definition** — a
/// progress-only test, where the payment side resolves progress-or-verb. This pass runs before the
/// assignment loop's build accrual, so on the turn a build banked its **first** work the two
/// disagreed: the source was skipped for having nothing on its meter, `shares[idx]` came back `0`,
/// and the stamp paid that zero into a meter the capture then read as owed the whole rate. Reported
/// from play as *"short 2 of the 2 work a turn this band's tended ground needs"* on a band 6% into a
/// Cultivate with `agriculture` staffed. `source_has_a_meter_at_risk` still answers the **row
/// survival** question, which is a different one (a queue entry keeps a row alive on its own, at
/// every call site).
///
/// **The meter's FULLNESS used to be the test** — a meter still being raised was owed its builders,
/// so it put nothing into the pool — and deleting it is what §4.6a of
/// `docs/plan_standing_upkeep.md` is. Two states reported from ordinary play were wrong under it:
/// a **half-built** meter whose builders left could not be held at all, bleeding its full rate with
/// keepers idle in the role and no command that could aim them at it; and a **held** rung eroding to
/// 99% stopped being the pool's business at the very moment it started needing it. The bill now
/// starts at the **first work banked** and ends with the last.
///
/// # THE PRIORITY ORDER IS TOTAL, BECAUSE A CHECKPOINT HAS TO REPRODUCE IT
///
/// [`crate::intensification::UpkeepFundMode::Priority`] funds in slice order, and the slice is
/// sorted **most-invested first** on the at-risk meter's stored cost, tie-broken on a stable
/// per-source key (a tile's coordinates, a herd's id). Two sources of equal investment therefore
/// fund in the same order on a restored world as on the original, which is the whole reason the
/// tie-break exists.
/// **WHAT THE BAND'S BUILDERS' TOOLS ADD TO WHAT THEY DELIVER, RESOLVED PER FOOD WEB.**
///
/// One pool, one queue — but **two kits**, because a hoe and a set of hurdles are tools for
/// different work and `EquipmentStat::BuildWork` is a per-worker *sum*. Without the split a bundle
/// carrying both would deliver `0.5 + 0.5` per worker on a plant build, and a single builders kit
/// would have to
/// serve every rung on both ladders (which is how the husbandry kit came to be offered for a
/// Cultivate).
///
/// # The kit is DERIVED PER ENTRY, and the ENTRY's own choice OVERRIDES it
///
/// 1. **A kit named on the queue ENTRY wins**, `none` included — that is how a player sends the pool
///    out bare-handed on one job to conserve gear, and it is the same *"an absent `kitId` means the
///    job's default"* rule every other selection follows
///    (`docs/plan_standing_upkeep.md` §4.7a ②).
/// 2. **Otherwise the roster answers**, per branch, through
///    [`EquipmentConfig::build_kit_for_branch`] — the shape `fauna::kit_supplying` already uses for a
///    penned herd's default kit. ⛔ **No `BuildJob → kit id` match exists in Rust**, so a third build
///    tool is a roster edit.
/// 3. **`default_kits.builders` is the fall-back**, not the answer: a roster with no kit serving a
///    web leaves that web's builds on whatever the job's default is (`none` today).
///
/// ⛔ **The `builders` ROW's kit is not an input.** It was rule ① until §4.7 and it is the one thing
/// the derivation cannot express: a single stored id per **band** pinned one web's tool onto every
/// later build of the other with no way back.
///
/// A source's own branch decides which **derived** reading it gets — a patch is plant, a herd is
/// animal — so the **head** entry is the one whose branch is actually funded, and everything below
/// it is *dated* at the gear it would be raised with when its turn comes. An entry carrying its own
/// kit is dated at **that** kit rather than at its web's derived one, which is the whole point of
/// the override.
struct BuildersGear<'a> {
    /// The roster every answer is resolved through.
    equipment: &'a crate::equipment_config::EquipmentConfig,
    /// **The band's ledger as the turn found it** — the wear snapshot every rate is struck at, so a
    /// tool that breaks mid-turn does not re-price the work already banked beside it.
    band_kit: &'a BandEquipment,
    /// The whole pool, since all hands go on the head.
    builders: u32,
    /// **The entries that named a kit of their own**, keyed by source.
    ///
    /// A plain `Vec` walked linearly rather than a map: a band's queue is a handful of entries and
    /// only the overridden ones are here, so the probe is cheaper than hashing a `BuildSource`.
    overrides: Vec<(BuildSource, crate::equipment_config::KitChoice)>,
}

/// One build's answer: the kit the pool works it with, and what that kit is worth over the pool.
struct BuildersRungGear {
    /// **The kit resolved for this web, narrowed to the tools that actually served it** — which is
    /// what the wear
    /// is charged against ([`crate::equipment_config::EquipmentConfig::build_gear_kit`]).
    ///
    /// **Wear follows the work actually done.** A player who names `hurdling` on the builders row and
    /// then raises a Cultivate takes *nothing* off that job — the branch filter zeroes the hurdles'
    /// contribution — so charging them would run a tool down for work it did not do. The full kit is
    /// not kept here because nothing downstream may price a build from it: the wire's own copy is
    /// resolved once, at capture, through [`LaborAllocation::builders_kit`].
    wear_kit: crate::equipment_config::KitChoice,
    /// **The coverage-weighted per-worker contribution** — what one of these builders' kit adds to
    /// its own output per turn, on **any** job on this web. The term
    /// [`crate::intensification::build_work_per_worker_turn`] takes, and therefore the term every
    /// accrual, balance and projection on this web is struck at.
    work_per_worker: f32,
    /// **That contribution summed over the whole pool** — a READOUT
    /// ([`crate::intensification::gear_work_supply`], published as `buildWorkFromGear`), and
    /// nothing divides by it. A kit raises what a builder delivers; it never shrinks the job
    /// (`docs/plan_standing_upkeep.md` §4.8).
    gear_supply: f32,
}

impl<'a> BuildersGear<'a> {
    fn resolve(
        equipment: &'a crate::equipment_config::EquipmentConfig,
        build_queue: &[crate::components::BuildQueueEntry],
        builders: u32,
        band_kit: &'a BandEquipment,
    ) -> Self {
        Self {
            equipment,
            band_kit,
            builders,
            // **Only the entries that named a kit are recorded** — everything else is served by the
            // roster's own answer for the job in front of it, which is the same kit it would resolve
            // to. The *pricing* is not done here: it depends on the rung, which is a fact about the
            // turn rather than about the queue.
            overrides: build_queue
                .iter()
                .filter_map(|entry| Some((entry.source.clone(), entry.kit.as_ref()?.clone())))
                .collect(),
        }
    }

    /// **THE GEAR ONE BUILD IS RAISED WITH** — this source's entry's own kit, else the roster's
    /// answer for the branch **and rung** in front of the pool, priced over the whole builders pool.
    ///
    /// ⛔ **THE RUNG IS THE ONE BEING WORKED THIS TURN, NEVER THE ENTRY'S DESTINATION.** A `pave`
    /// standing on ground below a dirt road is doing *grading* work, so it must resolve the grading
    /// tool and be paid the grading tool's offset; pricing it against where it is *going* would hand
    /// the paving kit's uplift to earthmoving, which is the whole failure
    /// [`crate::equipment_config::EquipmentEffect::rung`] exists to prevent. It is the rule
    /// [`crate::equipment_config::EquipmentConfig::build_gear_kit`] already states for wear — *the
    /// work actually done decides* — read one seam earlier.
    ///
    /// **`None` is a real answer and it is the conservative one**: a source nobody has queued is
    /// climbing nothing, and a rung-bound tool must not be quoted where no rung was named (see
    /// `serves_build`'s `(Some(_), None)` arm). A tool bound to no rung — every shipped plant and
    /// animal build tool — answers identically whatever is passed, which is what keeps the two food
    /// webs byte-identical across this change.
    ///
    /// **Resolved on demand rather than cached per branch.** The reading was one per ladder while a
    /// branch was the only bound there was; a rung-bound tool makes it one per *rung*, and a build
    /// asks this a handful of times a band-turn.
    fn for_source(
        &self,
        source: &BuildSource,
        rung: Option<crate::intensification::RungKey>,
    ) -> BuildersRungGear {
        let branch = source_branch(source);
        let key = rung.map(|rung| rung.wire_key());
        let key = key.as_deref();
        let named = self
            .overrides
            .iter()
            .find(|(source_key, _)| source_key == source)
            .map(|(_, kit)| kit);
        let kit = self.equipment.builders_kit_for(named, Some(branch), key);
        // **The coverage is over the POOL**, so the rate the wire publishes and the rate the accrual
        // is struck at are one number for the whole band.
        let coverage = self
            .equipment
            .coverage(&kit, self.builders as f32, self.band_kit);
        let work_per_worker = coverage.weighted_rate(|crew| {
            self.equipment
                .build_work_per_worker(crew, self.band_kit, branch, key)
        });
        BuildersRungGear {
            wear_kit: self
                .equipment
                .build_gear_kit(&kit, self.band_kit, branch, key),
            work_per_worker,
            gear_supply: gear_work_supply(work_per_worker, self.builders),
        }
    }
}

/// **THE RUNG A QUEUED SOURCE IS CLIMBING TOWARD**, or `None` for a source this band has not
/// queued.
///
/// It is the entry's **destination** and is used only where the source's own standing is not in
/// hand — the four rung arms, whose tools declare no rung bound at all, so every rung on their
/// branch resolves the same kit and the same worth. Where the distinction can be seen (the route
/// branch, whose two rungs want different tools) the caller reads the rung actually in flight
/// instead: `pile_legs.first()` for the material draw, `Road::held_rung` for the road arm.
///
/// **`None` is the right answer for an unqueued source and not a gap** — nothing is being raised
/// there, so no rung-bound tool may be quoted against it
/// ([`crate::equipment_config::EquipmentEffect::serves_build`]).
fn queued_destination(
    build_queue: &[crate::components::BuildQueueEntry],
    source: &BuildSource,
) -> Option<crate::intensification::RungKey> {
    build_queue
        .iter()
        .find(|entry| &entry.source == source)
        .map(|entry| entry.declared.destination())
}

/// **THE LADDER A BUILD SOURCE BELONGS TO** — a patch is plant, a herd is animal, a road tile is
/// route. One expression, because three copies of this match are three places a fourth source kind
/// has to be remembered.
fn source_branch(source: &BuildSource) -> crate::intensification::RungBranch {
    match source {
        BuildSource::Patch(_) => crate::intensification::RungBranch::Plant,
        BuildSource::Herd(_) => crate::intensification::RungBranch::Animal,
        BuildSource::Road(_) => crate::intensification::RungBranch::Route,
    }
}

/// **WHAT ONE KEEPER DELIVERS ON ONE WORK SITE, AND WHAT THAT WORK WEARS DOWN** — [`BuildersGear`]'s
/// twin one account over (`docs/plan_standing_upkeep.md` §4.8), resolved **per site**.
///
/// # ONE SUPPLY EXPRESSION, TWO CONSUMERS
///
/// *"Upkeep is just work/turn and worker productivity is work/turn."* A build divides its **pile**
/// by [`crate::intensification::pool_work_supply`] to get turns; an upkeep compares its **demand**
/// against the same expression to see whether it is covered. So a keeping pool stopped being a head
/// count and became `workers × (PER_WORKER_OUTPUT + what the kit delivers)` — the same shape, one
/// account over.
///
/// **THE DEMANDS DO NOT MOVE.** `plant:tended` asks `2.0` work a turn with hoes and without; what
/// changes is what a keeper *supplies* against it. That is the build rule's mirror — *the job's work
/// requirement never changes* — stated about a rate instead of a pile.
///
/// # ⛔ THE KIT IS THE SITE'S, NOT THE BAND'S
///
/// The band is the pool of workers and goods to draw from; it does not decide which tool a given
/// site is worked with. So the rate is resolved per **claim** off that claim's own row
/// ([`crate::components::LaborAssignment::upkeep_kit`]), exactly as a build's is resolved per queue
/// entry — and `None` on the row is the **web's derived default**
/// ([`crate::equipment_config::EquipmentConfig::keeping_kit_for`]), which is what keeps the whole
/// seam live with no player action. A single stored id on the `agriculture` / `husbandry` role row
/// could not say *hoes on the Field, bare hands on the scrub beside it*.
struct KeepingRate {
    /// **What one of this site's keepers banks per turn**, bare hands included — the `r` a claim's
    /// worker need `demand ÷ r` divides by.
    ///
    /// It cannot be zero: [`crate::intensification::build_work_per_worker_turn`] floors its gear
    /// term at bare hands and adds a positive `PER_WORKER_OUTPUT`. [`Self::worker_need`] checks
    /// anyway, because a division whose safety lives in another module is one config edit from a
    /// `NaN` share.
    per_worker: f32,
    /// **The site's kit, narrowed to the tools that actually serve its web** — what the
    /// [`crate::equipment_config::WearQuantum::UpkeepWork`] charge is billed against, resolved
    /// through the same [`crate::equipment_config::EquipmentConfig::build_gear_kit`] the builders'
    /// wear kit is. The narrowing is what stops a site whose kit holds the *other* web's tool from
    /// spending it on work that tool contributed nothing to — wear follows the work actually done.
    wear_kit: crate::equipment_config::KitChoice,
}

/// **THE KEEPING RATE AT WHICH NOBODY DELIVERS ANYTHING** — the zero
/// [`KeepingRate::worker_need`] refuses to divide by.
const NO_KEEPING_RATE: f32 = 0.0;

impl KeepingRate {
    /// **HOW MANY KEEPERS THIS SITE'S BILL NEEDS** — `demand ÷ r`, and **the unit the pool is split
    /// in** since the kit became per site.
    ///
    /// The split has to be in *workers* rather than in work, because the two stopped being
    /// interchangeable: with one rate for the whole web, splitting the work pool in proportion to
    /// demand and splitting the worker pool in proportion to worker-need are the same arithmetic,
    /// and with two rates they are not. Two sites owing the same demand, one hoed and one bare, ask
    /// for **different numbers of hands** — which is exactly what a per-site tool means.
    fn worker_need(&self, demand: f32) -> f32 {
        if self.per_worker > NO_KEEPING_RATE {
            demand / self.per_worker
        } else {
            NO_UPKEEP_DEMAND
        }
    }
}

/// **WHAT ONE ASSIGNMENT ROW WAS AWARDED FROM ITS WEB'S KEEPING POOL THIS TURN** —
/// [`maintenance_shares`]'s answer, index-aligned with `allocation.assignments`.
///
/// **The work and the kit that did it travel together**, because the wear charge is billed on what
/// the pool *supplied* to that source and the two must describe one site: paying one site's hours
/// against another site's tool is the defect a per-band kit made unavoidable.
#[derive(Clone)]
struct KeepingAward {
    /// This source's share of its web's pool, in **work** units.
    work: f32,
    /// The kit that work was done with, already narrowed to the tools serving this web. `None` for a
    /// row that made no claim — it was supplied nothing, so it wore nothing.
    wear_kit: Option<crate::equipment_config::KitChoice>,
}

impl Default for KeepingAward {
    fn default() -> Self {
        Self {
            work: NO_UPKEEP_DEMAND,
            wear_kit: None,
        }
    }
}

/// **WHAT EACH CLAIM'S KEEPERS DELIVER, AND WHAT THEIR WORK WEARS** — index-aligned with `claims`.
///
/// # ⛔ SITES SHARING A KIT SHARE ITS SCARCITY
///
/// [`crate::equipment_config::EquipmentConfig::coverage`] answers *"of these workers, how many
/// actually carry the kit's items, given what the band owns"*. Asked naively per site it
/// **double-counts**: a band owning three hoes, with two keepers on one patch and three on another,
/// would arm two on the first and three on the second — five equipped hands off three hoes.
///
/// So the claims are **grouped by their resolved kit** and coverage is taken **once** per group,
/// over that group's whole share of the pool. Sites naming different kits do not compete for each
/// other's gear; sites naming the same one degrade together, exactly as the single band-wide pool
/// did. With one distinct kit on a branch — which is every branch on the shipped roster — this is
/// one call over the whole role, bit for bit what shipped before the kit moved to the site.
///
/// **THE GROUPING KEY IS THE KIT ID, so two DIFFERENT kits sharing an ITEM still double-count it** —
/// two plant kits both listing `hoes` would each arm their own group off the same stock. No shipped
/// kit shares an item with another on its own web, and the band-wide code this replaced had the
/// identical property across the two branches, so nothing regressed and nothing is reachable today.
/// Closing it means grouping by the item rather than by the kit, which is a real change to what
/// "sharing scarcity" means and wants a case in front of it first.
///
/// # THE GROUP'S SHARE OF THE POOL IS STRUCK OFF THE BILL, AND IT HAS TO BE
///
/// A group's *rate* depends on how many hands stand in it, and how many hands stand in it depends on
/// every group's rate — so the coverage read cannot be taken over the split it is an input to. It is
/// taken over the group's share of the **demand** instead, which is the one measure of *how much of
/// this band's keeping is this group* that does not mention a kit. The split proper
/// ([`maintenance_shares`]) then runs in worker-need units against these rates and may land
/// somewhere else — a group whose tool is efficient needs fewer hands than its share of the bill.
fn keeping_rates(
    equipment: &crate::equipment_config::EquipmentConfig,
    band_kit: &BandEquipment,
    branch: crate::intensification::RungBranch,
    keepers: u32,
    claims: &[KeepingClaim],
) -> Vec<KeepingRate> {
    let total_demand = keeping_demand(claims);
    // The distinct kits on this branch, and what each group's sites ask for between them. Keyed by
    // roster id: an id determines the kit's items, so two claims that resolved the same id are
    // drawing on the same units and must share one coverage between them.
    //
    // ⛔ **THE RUNG IS NOT PART OF THE KEY, AND THAT IS DELIBERATE.** Coverage answers *"how many of
    // these hands does the band own gear for"* — a fact about the LEDGER — so splitting one kit into
    // two groups by rung would arm a prefix of each and put two equipped hands behind one tool. What
    // the rung decides is the RATE, which is why it is applied per claim below against the group's
    // own partition rather than by re-partitioning.
    let mut group_kits: Vec<crate::equipment_config::KitChoice> = Vec::new();
    let mut group_demand: Vec<f32> = Vec::new();
    let mut group_of_claim: Vec<usize> = Vec::with_capacity(claims.len());
    for claim in claims {
        let group = group_kits
            .iter()
            .position(|kit| kit.id() == claim.kit.id())
            .unwrap_or_else(|| {
                group_kits.push(claim.kit.clone());
                group_demand.push(NO_UPKEEP_DEMAND);
                group_kits.len() - 1
            });
        group_demand[group] += claim.demand;
        group_of_claim.push(group);
    }
    // **The coverage is over the GROUP**, exactly as the builders' is over their pool: the seam arms
    // a prefix, so a part-equipped group gets the share it actually carries and the bare hands
    // beside it still bring their own `PER_WORKER_OUTPUT`.
    let coverage: Vec<crate::equipment_config::KitCoverage> = group_kits
        .iter()
        .zip(&group_demand)
        .map(|(kit, demand)| {
            let share = if total_demand > NO_UPKEEP_DEMAND {
                keepers as f32 * (demand / total_demand)
            } else {
                NO_UPKEEP_DEMAND
            };
            equipment.coverage(kit, share, band_kit)
        })
        .collect();
    // **THE RATE IS PER CLAIM, AT THE RUNG THAT SITE STANDS ON** — the group's partition is shared
    // (the band owns what it owns) but a rung-bound tool is worth nothing on a rung it does not
    // serve, so a dirt road and a paved one kept out of one bundle read different rates off the same
    // coverage. Where nothing in the kit is rung-bound — every plant and animal tool that ships —
    // every claim in a group reads the identical number and this is the same fold it always was.
    group_of_claim
        .into_iter()
        .zip(claims)
        .map(|(group, claim)| {
            let rung_key = claim.rung.map(|rung| rung.wire_key());
            let rung_key = rung_key.as_deref();
            let gear = coverage[group].weighted_rate(|crew| {
                equipment.build_work_per_worker(crew, band_kit, branch, rung_key)
            });
            KeepingRate {
                per_worker: crate::intensification::build_work_per_worker_turn(gear),
                wear_kit: equipment.build_gear_kit(&group_kits[group], band_kit, branch, rung_key),
            }
        })
        .collect()
}

/// **HOW MANY KEEPERS ONE WEB'S BILL NEEDS THIS TURN** — the sum of every claim's
/// [`KeepingRate::worker_need`], and the number [`spare_keepers`] strikes a role's head count
/// against.
///
/// **It is struck through the same seam the split is** ([`keeping_rates`]), so *"more keepers than
/// the bill needs"* and *"what each site is owed"* cannot come from two different readings of the
/// same gear.
fn keeping_worker_need(
    equipment: &crate::equipment_config::EquipmentConfig,
    band_kit: &BandEquipment,
    branch: crate::intensification::RungBranch,
    keepers: u32,
    claims: &[KeepingClaim],
) -> f32 {
    keeping_rates(equipment, band_kit, branch, keepers, claims)
        .iter()
        .zip(claims)
        .map(|(rate, claim)| rate.worker_need(claim.demand))
        .sum()
}

/// **THE ONE SOURCE THIS BAND'S BUILDERS CAN PUT WORK ON THE GROUND FOR THIS TURN**, and the rung it
/// declared there — the verb term of [`maintenance_shares`]'s eligibility, and nothing else.
///
/// # WHY THE VERB TERM IS NARROWED TO THE FUNDED HEAD, AND THEN TO A HEAD THAT CAN ACTUALLY BANK
///
/// The keeping bill starts at the **first work banked** (`docs/plan_standing_upkeep.md` §2.4/§4.6a),
/// and the *only* reason the claim side needs a verb at all is that `maintenance_shares` runs before
/// the accrual that banks it: on that one turn the ground does not yet carry the work the pool is
/// about to owe for. **All hands go on the head** (§2.5), so at most one of a band's sources can bank
/// its first work in a turn, and every entry behind the head banks nothing however long it waits.
///
/// Honouring a *waiting* entry's declaration would therefore bill the pool for ground with **nothing
/// on it** — and, because the default `UpkeepFundMode::Spread` funds in proportion to demand, a band
/// with two queued-but-unfunded builds would dilute the share of the Field it actually holds. That is
/// not the fix this seam is for; it is a new way to starve a real holding.
///
/// **A head whose own rung GATE refuses banks nothing either**, for as long as the block lasts, so
/// the same dilution reaches the same real holdings through the funded entry rather than a waiting
/// one — the state reported from play as a blocked `Tame` starving a band's `husbandry`. The head's
/// gate is therefore resolved *before* the pool is split ([`head_rung_gate`]) and joins this test.
/// It cuts **only a meter at zero**: a blocked head that has already banked work is answered for by
/// its own progress (`forage::patch_build_verb` / `fauna::herd_build_verb` honour a declaration only
/// at a zero meter), so it goes on claiming exactly as it did — which is right, because the pool
/// still owes for the work standing on that ground.
///
/// **The invariant this preserves is `claim-side verb ⊆ payment-side verb`.** The payment side reads
/// *any* queue entry's declaration and applies no gate; every term added here can only remove
/// sources from the claim, so the two seams stay a subset and cannot disagree in the direction that
/// caused the first-turn bug. What a further narrowing risks instead is refusing a share on a turn
/// the build really does bank — which is why the gate is resolved fresh from this turn's ground
/// rather than read off last turn's published `build_blocked_reason` (the decay pass clears that at
/// the top of every turn, so at this point in the turn it is always [`crate::intensification::BuildGate::Open`]).
///
/// `None` when the queue is empty, when the head declares a **ring** (which fills no rung meter and
/// so names no verb), or when nobody is on the `builders` row — a declaration with no hands behind it
/// puts nothing on the ground either.
///
/// ⛔ **THE RING'S ABSENCE HERE IS ABOUT THE VERB AND NOTHING ELSE.** This seam once decided the
/// build's *material* claim too, and a ring therefore bid for no pile and widened a pen for free.
/// The pile is laid by [`head_ring_leg`] now, off the ring's own gate — so filtering `ExtendPen` out
/// of a **verb** answer no longer makes a ring materially free.
struct SourceBankingFirstWork {
    source: Option<(BuildSource, Improvement)>,
}

impl SourceBankingFirstWork {
    /// The rung declared **on this target**, or `None` for every other source the band works. The
    /// per-web `*_build_verb` seams then answer whether that declaration is live: a meter already
    /// carrying progress declares for itself and needs nothing from here.
    fn declared_on(&self, target: &LaborTarget) -> Option<Improvement> {
        let (banking, verb) = self.source.as_ref()?;
        let source = BuildSource::of(target)?;
        (&source == banking).then_some(*verb)
    }
}

/// Resolve [`SourceBankingFirstWork`] for a band — the head of its queue, if that entry declares a
/// rung, the band has builders to raise it, **and that rung's own gate holds**. The first two mirror
/// the assignment loop's own `build_workers` rule (`is_queue_head && declared.is_some()`, else
/// nobody); the third mirrors [`RungDef::build_accrual`]'s `eligible`, because both are asking the
/// same question — *will a work unit land on this ground this turn* — one stage earlier.
///
/// `head_gate` is [`head_rung_gate`] at the call site, where the ledger, the land and the configs
/// are all in hand.
fn source_banking_its_first_work(
    allocation: &LaborAllocation,
    head_gate: impl FnOnce(&BuildSource, Improvement) -> BuildGate,
) -> SourceBankingFirstWork {
    if allocation.workers_on(&LaborTarget::Builders) == NO_CREW_ON_THIS_ACTIVITY {
        return SourceBankingFirstWork { source: None };
    }
    let source = allocation
        .build_queue
        .first()
        .and_then(|entry| match entry.declared {
            BuildJob::Rung(improvement) => Some((entry.source.clone(), improvement)),
            BuildJob::ExtendPen => None,
        })
        .filter(|(source, improvement)| head_gate(source, *improvement).holds());
    SourceBankingFirstWork { source }
}

/// **THE `plant:tended` GATE**, stated once — the terms of the `Cultivate` arm's `eligible`, in the
/// order their refusals are published in.
///
/// The four rung gates are functions rather than inline `first_refusal` lists because two callers
/// ask each of them: the arm that acts on the verdict, and [`head_rung_gate`], which asks a stage
/// earlier so the keeping pool does not fund a head that will bank nothing. A second inline copy of
/// the term list is exactly how the two would come to publish different causes for one refusal.
fn plant_tended_gate(knows_rung: bool, working_the_patch: bool, has_crop: bool) -> BuildGate {
    BuildGate::first_refusal(&[
        (knows_rung, BuildGate::Knowledge),
        (working_the_patch, BuildGate::Escapement),
        // **Nothing to tend if nothing here climbs.** A patch with no committed plant is one whose
        // basket the tended rung's `cultivation_ceiling` refuses outright — the "not every plant
        // climbs" ruling reaching the build meter.
        (has_crop, BuildGate::NoCrop),
    ])
}

/// **THE `plant:field` GATE** — [`plant_tended_gate`]'s twin one rung up. `declared_sow` is the
/// dead-entry term (`BuildGate::Undeclared`): rung 3 is the one rung whose arm is entered on the
/// *declaration* rather than on a meter, so a stale entry has to be able to say so.
fn plant_field_gate(
    declared_sow: bool,
    knows_rung: bool,
    land_admits: bool,
    has_crop: bool,
) -> BuildGate {
    BuildGate::first_refusal(&[
        (declared_sow, BuildGate::Undeclared),
        (knows_rung, BuildGate::Knowledge),
        (land_admits, BuildGate::Site),
        // A Field may only be placed on ground that grows something sowable — the species half of
        // "the land must take seed", beside the site half above.
        (has_crop, BuildGate::NoCrop),
    ])
}

/// **DID THE PLAYER BID FOR HAY?** — the ungated arm of the wild-fodder credit (#433, issue #590's
/// follow-up), and the one question that arm is entitled to ask.
///
/// A patch committed to a fodder-bearing plant is a patch whose owner chose to grow animal feed, so
/// the harvest banks hay whether or not the faction has learned Foddering. A patch committed to
/// anything else — a grain, a fruit, a fibre crop — is a bid for **that**, and the hay the tile's
/// basket happens to contain is not something anyone asked for. Testing the rate on the **committed
/// species** rather than on the tile is the whole distinction: a `wild_emmer` field on ground that
/// is 31% `hay_grass` still converts at the basket average, so an `is_some()` test paid a faction
/// with no pens and no Foddering a hay income nothing it owned could ever eat.
///
/// **Fails closed on both `None` arms.** An uncommitted patch is the wild basket and was never a bid
/// at all; a committed id that does not resolve in the flora table is a config the sim cannot read,
/// and an unreadable commitment must refuse the credit rather than open the gate.
///
/// It answers only *whether* the fodder component is banked. **How much** is untouched: a committed
/// patch still converts at the share-weighted average of its own basket.
fn committed_to_a_fodder_crop(
    species: Option<&str>,
    flora: &crate::flora_config::FloraConfig,
) -> bool {
    species
        .and_then(|key| flora.species.get(key))
        .is_some_and(|def| def.yield_.bears_fodder())
}

/// **THE `animal:pastoral` GATE** — the `Tame` arm's `eligible`. Ownership is deliberately absent:
/// `Herd::accrue_domestication` owns the `owner is None || owner == faction` rule, exactly as
/// `accrue_cultivation` does on the plant side.
fn animal_pastoral_gate(
    knows_rung: bool,
    can_domesticate: bool,
    working_the_herd: bool,
) -> BuildGate {
    BuildGate::first_refusal(&[
        (knows_rung, BuildGate::Knowledge),
        (can_domesticate, BuildGate::SpeciesCeiling),
        (working_the_herd, BuildGate::Escapement),
    ])
}

/// **THE `animal:pen` GATE** — the `Corral` arm's `eligible`. It carries **no work predicate**, for
/// `accrue_field`'s reason: the term replaced a rung's `Thriving` gate and rung 3 never had one on
/// either web, because a fence goes up around a flock already drawn down to its keeper's own floor.
fn animal_pen_gate(
    knows_rung: bool,
    can_pen: bool,
    is_domesticated: bool,
    owned_by_faction: bool,
) -> BuildGate {
    BuildGate::first_refusal(&[
        (knows_rung, BuildGate::Knowledge),
        (can_pen, BuildGate::SpeciesCeiling),
        (is_domesticated, BuildGate::RungBelow),
        (owned_by_faction, BuildGate::OwnedByOther),
    ])
}

/// **DOES THE HEAD OF THIS BAND'S QUEUE ACTUALLY CLEAR ITS OWN RUNG'S GATE THIS TURN?** — asked
/// before the keeping pool is split, so a build the ground refuses cannot claim a share of it.
///
/// # It composes the ARM'S OWN gate, not a second reading of it
///
/// Every verdict comes back through [`plant_tended_gate`] / [`plant_field_gate`] /
/// [`animal_pastoral_gate`] / [`animal_pen_gate`], the same four functions each arm's `eligible` is
/// read from, and every term is resolved through the seam that arm resolves it through — the rung's
/// own `unlock_discovery_id`, [`crew_is_working_the_source`] over the escapement room,
/// `forage::resolve_committed_species` against `forage::tile_flora_composition`,
/// `forage::rung_site_refusal`, and the herd's own ceiling/ownership predicates.
///
/// **The terms are read PRE-TAKE and PRE-ACCRUAL, which is exactly where the arm reads them.**
/// `biomass` is the same number the arm's `biomass_before` will be (a band holds at most one row per
/// source, so nothing between here and there moves it), and the crop term asks
/// `patch.species.is_some() || the selection resolves` because `ForagePatch::commit_species` is
/// idempotent — the arm commits before it composes, so *"already committed, or committable"* is the
/// same bool it will read.
///
/// [`BuildGate::Unworked`] where the source is not on the ground at all: a gate nobody can judge is
/// not one that holds, and `maintenance_shares` skips such a row in any case.
///
/// # ⛔ THE ROUTE BRANCH IS ANSWERED BEFORE THE LABOR-ROW LOOKUP, BECAUSE A ROAD HAS NO ROW
///
/// Every other source is found by matching the queue entry against `assignments`, and
/// [`BuildSource::of`] never yields a `Road` — a road's holding lives on `routes::Road::keeper`, not
/// on a row. So a road head fell straight through to [`BuildGate::Unworked`], which meant
/// `source_banking_its_first_work` filtered it out, `banking.source` was permanently `None` for a
/// road, and **the whole material path was unreachable**: no pile was ever struck for a `pave`, and
/// [`head_build_legs`]' road arm was dead code whose comment described the emptiness as exact.
///
/// [`route_head_gate`] answers it instead, from the same two terms the road build arm itself
/// resolves — *does this band still keep the tile* and *does the faction know the rung* — so the
/// claim side and the payment side cannot disagree about whether a road banks this turn.
#[allow(clippy::too_many_arguments)] // one source, one rung, and every seam its gate is judged by
fn head_rung_gate(
    source: &BuildSource,
    improvement: Improvement,
    allocation: &LaborAllocation,
    forage_registry: &ForageRegistry,
    herds: &HerdRegistry,
    roads: &crate::routes::RoadRegistry,
    band_id: Option<BandId>,
    faction: FactionId,
    discovery: &DiscoveryProgressLedger,
    knowledge_threshold: f32,
    ladder: &LadderConfig,
    fauna: &FaunaConfig,
    labor: &LaborConfig,
    flora: &crate::flora_config::FloraConfig,
    food_sites: &FoodSiteRegistry,
    tile_registry: &TileRegistry,
    tiles: &Query<&Tile>,
    map_seed: u64,
    wrap_horizontal: bool,
) -> BuildGate {
    if let BuildSource::Road(tile) = source {
        return route_head_gate(
            roads,
            band_id,
            *tile,
            improvement,
            faction,
            discovery,
            knowledge_threshold,
            ladder,
        );
    }
    let Some(target) = allocation
        .assignments
        .iter()
        .map(|assignment| &assignment.target)
        .find(|target| BuildSource::of(target).is_some_and(|held| &held == source))
    else {
        // An entry with no row is retired by `prune_build_queue` this same turn; until then it is a
        // declaration nobody is standing behind.
        return BuildGate::Unworked;
    };
    let knows_rung = |rung: &RungDef| {
        rung.unlock_discovery_id()
            .is_none_or(|knowledge| knows(discovery, faction, knowledge, knowledge_threshold))
    };
    match (source, target) {
        (BuildSource::Patch(tile), LaborTarget::Forage { floor, species, .. }) => {
            let ground = tile_registry
                .index(tile.x, tile.y)
                .and_then(|entity| tiles.get(entity).ok());
            let Some(ground) = ground else {
                return BuildGate::Unworked;
            };
            // The tile's **realized** basket — what is growing here — the same seam the arm commits
            // against, so a selection the arm would accept is one this gate accepts.
            let composition = tile_flora_composition(flora, &labor.forage, ground, map_seed);
            match improvement {
                Improvement::Cultivate => {
                    let Some(patch) = forage_registry.patch(*tile) else {
                        return BuildGate::Unworked;
                    };
                    let rung = ladder.rung(RungKey::PlantTended);
                    let working_the_patch =
                        source_is_workable(crate::forage::patch_take_room(patch, *floor));
                    let has_crop = patch.species.is_some()
                        || resolve_committed_species(
                            species.as_deref(),
                            &composition,
                            flora,
                            RungKey::PlantTended,
                        )
                        .is_ok();
                    plant_tended_gate(knows_rung(rung), working_the_patch, has_crop)
                }
                Improvement::Sow => {
                    let rung = ladder.rung(RungKey::PlantField);
                    let fresh_water = tile_is_fresh_watered(
                        ground,
                        tile_registry.width,
                        tile_registry.height,
                        wrap_horizontal,
                        |coord| {
                            tile_registry
                                .index(coord.x, coord.y)
                                .and_then(|entity| tiles.get(entity).ok())
                                .map(|neighbor| neighbor.terrain_tags)
                        },
                    );
                    let land_admits = rung_site_refusal(
                        rung,
                        ground,
                        &labor.forage,
                        food_sites.is_site(ground.position),
                        fresh_water,
                    )
                    .is_none();
                    // §10 scoping, exactly as the arm scopes it: a Sow that **upgrades** an existing
                    // patch commits against the realized basket, one that **creates** a patch on bare
                    // ground has no realized basket and reads the affinity roster.
                    let basket = if forage_registry.patch(*tile).is_none() {
                        Cow::Borrowed(flora.composition(ground.resource_terrain()))
                    } else {
                        Cow::Borrowed(composition.as_ref())
                    };
                    let has_crop = resolve_committed_species(
                        species.as_deref(),
                        &basket,
                        flora,
                        RungKey::PlantField,
                    )
                    .is_ok();
                    plant_field_gate(true, knows_rung(rung), land_admits, has_crop)
                }
                // A rung another web owns can never stand on ground — a dead entry.
                Improvement::Tame
                | Improvement::Corral
                | Improvement::Grade
                | Improvement::Pave => BuildGate::Undeclared,
            }
        }
        (BuildSource::Herd(id), LaborTarget::Hunt { floor, .. }) => {
            let Some(herd) = herds.find(id) else {
                return BuildGate::Unworked;
            };
            match improvement {
                Improvement::Tame => {
                    let rung = ladder.rung(RungKey::AnimalPastoral);
                    // **THE BUILD'S GATE READS WHAT THE TAKE WILL PAY** ([`source_is_workable`]),
                    // never the raw escapement room: taming raises the herd's `K`, so the floor
                    // climbs out from under a herd that started on it and the gate would refuse the
                    // very build that moved it.
                    let workable = source_is_workable(fauna::herd_take_room(herd, *floor, fauna));
                    animal_pastoral_gate(knows_rung(rung), herd.can_domesticate(), workable)
                }
                Improvement::Corral => {
                    let rung = ladder.rung(RungKey::AnimalPen);
                    animal_pen_gate(
                        knows_rung(rung),
                        herd.can_pen(),
                        herd.is_domesticated(),
                        herd.owner == Some(faction),
                    )
                }
                // A rung another web owns can never stand on a herd — a dead entry.
                Improvement::Cultivate
                | Improvement::Sow
                | Improvement::Grade
                | Improvement::Pave => BuildGate::Undeclared,
            }
        }
        // A queue entry always names a Forage tile or a Hunt herd; a band-wide role holds neither.
        _ => BuildGate::Unworked,
    }
}

/// **THE `route:*` GATE**, stated once — the terms of the road build arm's own `eligible`, in the
/// order their refusals are published in.
///
/// It is the four rung gates' shape one branch over, and it exists for their reason: two callers ask
/// it. The arm acts on the verdict, and [`head_rung_gate`] asks a stage earlier so the pile and the
/// keeping pool are not struck for a head that will bank nothing.
///
/// **The keeper, then the knowledge, and there is deliberately nothing else.**
/// - **The keeper, ASKED AS TWO QUESTIONS.** A road is the job of the band that graded it
///   (`routes::Road::keeper`), and a band whose `grade` was superseded banks nothing — but *why* it
///   was superseded is two different situations with two different remedies, so it is two causes:
///   - [`BuildGate::NoKeeper`] — **nobody keeps this tile.** `Road::set_position` releases the
///     keeper the moment decay or disuse takes the road back below `traffic_ceiling`, so this is the
///     ordinary end of a road nobody walked. The remedy is to take it on: re-issuing `grade` /
///     `pave` adopts it, which is why the branch ships no separate adoption verb.
///   - [`BuildGate::OwnedByOther`] — **another band keeps it.** A real rival holding real ground.
///
///   ⛔ **REPORTING THE SECOND FOR THE FIRST IS A FALSE SENTENCE**, not merely a terse one: it sends
///   the player looking for a band that does not exist, past the one road on the map they could
///   simply have claimed.
/// - **The knowledge.** `roadbuilding` gates a `grade` and `paving` a `pave`, off the rung record's
///   own `unlock_discovery_id`.
///
/// `site_requirement` is `null` on every route rung — a road asks nothing of the land it crosses, it
/// is *priced* by it — so there is no ground term to refuse, and the rung beneath is guaranteed by
/// the keeper: a band cannot hold a road it never graded.
#[allow(clippy::too_many_arguments)] // the tile, the rung it declares, and every seam its gate reads
fn route_head_gate(
    roads: &crate::routes::RoadRegistry,
    band_id: Option<BandId>,
    tile: UVec2,
    improvement: Improvement,
    faction: FactionId,
    discovery: &DiscoveryProgressLedger,
    knowledge_threshold: f32,
    ladder: &LadderConfig,
) -> BuildGate {
    let destination = RungKey::built_by(improvement);
    if destination.branch() != crate::intensification::RungBranch::Route {
        // A rung another web owns can never stand on a road — a dead entry, exactly as a `Tame`
        // declared on a patch is.
        return BuildGate::Undeclared;
    }
    let knows_rung = ladder
        .rung(destination)
        .unlock_discovery_id()
        .is_none_or(|id| knows(discovery, faction, id, knowledge_threshold));
    // **Is anybody keeping it at all**, asked before *is it ours* — the order is what makes the
    // pair say two different things. `first_refusal` reports the earliest failing term, so an
    // unkept road answers `NoKeeper` and only a road somebody else really holds reaches the second.
    let kept_by_somebody = roads.road(tile).is_some_and(|road| road.keeper.is_some());
    BuildGate::first_refusal(&[
        (kept_by_somebody, BuildGate::NoKeeper),
        // **Through the same seam the prune and the build arm ask**, so *"is this road this band's
        // job"* has one answer in one place.
        (
            band_keeps_road(roads, band_id, tile),
            BuildGate::OwnedByOther,
        ),
        (knows_rung, BuildGate::Knowledge),
    ])
}

/// One source's claim on its web's pool: where to write the share back, what it asks for, **what it
/// is worked with**, and the two keys that make *most-invested first* a total order.
struct KeepingClaim {
    index: usize,
    demand: f32,
    /// **THE KIT THIS SITE IS KEPT WITH** — its own row's selection, else its web's derivation
    /// ([`crate::equipment_config::EquipmentConfig::keeping_kit_for`]). Resolved here, with the
    /// claim, because the rate a claim is funded at and the wear that rate spends are two readings
    /// of one choice and must not be taken from two places.
    kit: crate::equipment_config::KitChoice,
    /// **THE RUNG THIS SITE STANDS ON** — the bound the kit was resolved at and the one its keeping
    /// is priced at, so the two cannot come from two readings.
    ///
    /// ⛔ **THE SITE'S OWN RUNG, NEVER A DESTINATION.** A keeper is holding what is there; nothing
    /// about a queued build changes what this turn's keeping is worth, so a Field being widened is
    /// kept exactly as a Field.
    rung: Option<crate::intensification::RungKey>,
    invested: f32,
    tiebreak: String,
}

/// **WHAT THIS BAND'S ROWS CLAIM FROM THEIR WEBS' KEEPING POOLS THIS TURN** — the plant claims and
/// the animal claims, in row order.
///
/// **THE one definition of the band's keeping bill.** [`maintenance_shares`] divides the two pools
/// against it, and the shedding order's spare-keeper step counts a role's hands against its **sum**
/// ([`keeping_demand`]) — so *"more keepers than the bill needs"* and *"what each source is owed"*
/// can never be struck off two different readings of the same ground.
#[allow(clippy::too_many_arguments)] // one source, one rung, and every seam its gate is judged by
fn keeping_claims(
    allocation: &LaborAllocation,
    banking: &SourceBankingFirstWork,
    // **The roster the site kits resolve against** — a claim carries the kit it is worked with, so
    // the derivation that answers for a row naming none is read here rather than a seam later.
    equipment: &crate::equipment_config::EquipmentConfig,
    forage_registry: &ForageRegistry,
    // **The ground under each plant claim**, resolved by coord: the plant demand is quoted per
    // tender-load of the TILE's own `K` (`forage::patch_tender_loads`), so a claim cannot be priced
    // off the patch alone. A tile that is not on the map presents no land and therefore no claim.
    tile_capacity_of: &dyn Fn(UVec2) -> f32,
    forage: &crate::labor_config::ForageLaborConfig,
    herds: &HerdRegistry,
    fauna: &FaunaConfig,
    ladder: &LadderConfig,
) -> (Vec<KeepingClaim>, Vec<KeepingClaim>) {
    let mut plant: Vec<KeepingClaim> = Vec::new();
    let mut animal: Vec<KeepingClaim> = Vec::new();
    for (index, assignment) in allocation.assignments.iter().enumerate() {
        // **THE TAKE CREW IS NOT A TERM HERE, and that separation is the point** (§2.2). A row's
        // eligibility is the *ground's* answer — *does this source have a meter carrying work* —
        // never how many gatherers happen to be standing on it this turn. Filtering on
        // `assignment.workers` made a band that moved its foragers to a richer patch unable to keep
        // the Field it had just finished: no demand, no share, and a full-rate bleed with idle
        // keepers in the role.
        //
        // **AND THE ELIGIBILITY IS THE PAYMENT SIDE'S OWN RESOLVER** —
        // `forage::patch_keeping_meter` / `fauna::herd_keeping_meter`, the one definition of *"a
        // meter needing keeping"*, which the stamp below the loop and every demand reading also go
        // through. It used to be `source_has_a_meter_at_risk`, a **progress-only** test, and this
        // pass runs *before* the turn's build accrual — so on the turn a build banked its first
        // work the claim side said *nothing here* while the payment side, resolving progress-or-
        // verb, knew perfectly well the pool owed for it. The share came back `0`, the stamp paid
        // that zero, and the capture — reading the source after the accrual — published the whole
        // demand as a shortfall on a **staffed** keeping role.
        let declared = banking.declared_on(&assignment.target);
        match &assignment.target {
            LaborTarget::Forage { tile, .. } => {
                let Some(patch) = forage_registry.patch(*tile) else {
                    continue;
                };
                let verb = crate::forage::patch_build_verb(patch, declared);
                if !crate::forage::patch_claims_keeping(patch, verb) {
                    continue;
                }
                let rung = crate::forage::patch_rung_key(patch);
                plant.push(KeepingClaim {
                    index,
                    kit: equipment.keeping_kit_for(
                        assignment.upkeep_kit.as_ref(),
                        crate::intensification::RungBranch::Plant,
                        Some(&rung.wire_key()),
                    ),
                    rung: Some(rung),
                    // **The DEMAND takes no verb any more** — it interpolates on the patch's own
                    // position, so there is no step for the one-turn carry to straddle. The verb
                    // survives one line up, where it still answers *does this source claim at all
                    // on the turn its first work is about to land*.
                    //
                    // **⛔ AND IT IS THE STAMPED BILL, NOT THE LIVE DEMAND** — the same
                    // `patch_keeping_basis` the capture publishes and `advance_cultivation` bleeds
                    // against. This pass runs inside the **per-band** loop and the build accrual
                    // that moves the position runs later in the same iteration, so on a source two
                    // bands work — one keeping it, one building it — a share struck off the *live*
                    // demand is struck at a position the published bill was never taken at. The
                    // wire then states `upkeepDemand` from the first band's stamp against an
                    // `upkeepSupplied` paid at a later one, and `demand − supplied == shortfall` —
                    // written into `snapshot.fbs` for both webs — is false while the pool spends
                    // work the source never owed.
                    demand: crate::forage::patch_keeping_basis(
                        patch,
                        ladder,
                        tile_capacity_of(*tile),
                        forage,
                    ),
                    invested: crate::forage::patch_at_risk_cost(patch),
                    tiebreak: format!("{:010}:{:010}", tile.x, tile.y),
                });
            }
            LaborTarget::Hunt { fauna_id, .. } => {
                let Some(herd) = herds.find(fauna_id) else {
                    continue;
                };
                let verb = fauna::herd_build_verb(herd, declared);
                if !fauna::herd_claims_keeping(herd, verb) {
                    continue;
                }
                let rung = fauna::herd_rung_key(herd);
                animal.push(KeepingClaim {
                    index,
                    kit: equipment.keeping_kit_for(
                        assignment.upkeep_kit.as_ref(),
                        crate::intensification::RungBranch::Animal,
                        Some(&rung.wire_key()),
                    ),
                    rung: Some(rung),
                    // **The DEMAND takes no verb any more** — it interpolates on the herd's own
                    // position, so there is no step for the one-turn carry to straddle. The verb
                    // survives one line up, where it still answers *does this source claim at all
                    // on the turn its first work is about to land*.
                    //
                    // **⛔ AND IT IS THE STAMPED BILL** — `herd_keeping_basis`, for the reason the
                    // plant arm above states at length: the share and the published demand must be
                    // struck at one position, or the wire's `demand − supplied == shortfall` is
                    // false on every source two bands share.
                    demand: fauna::herd_keeping_basis(herd, fauna, ladder),
                    invested: fauna::herd_at_risk_cost(herd),
                    tiebreak: herd.id.clone(),
                });
            }
            LaborTarget::Scout
            | LaborTarget::Warrior
            | LaborTarget::Agriculture
            | LaborTarget::Husbandry
            | LaborTarget::Roadwork
            | LaborTarget::Builders => {}
        }
    }
    (plant, animal)
}

/// **WHAT ONE WEB'S KEEPING POOL IS BILLED FOR THIS TURN**, in work units — the sum of its claims,
/// which is the number a keeping role has to cover for nothing to rot.
fn keeping_demand(claims: &[KeepingClaim]) -> f32 {
    claims.iter().map(|claim| claim.demand).sum()
}

/// **HANDS ON A KEEPING ROLE THE BILL DOES NOT NEED** — the largest number that can leave the role
/// with what remains still covering every claim in full. The shedding order's step 3 spends exactly
/// these, and only these, before anything that costs output.
///
/// A pool does not divide into whole people, so the crew that must stay is `ceil(worker_need)` —
/// [`keeping_worker_need`], which is each site's own `demand ÷ its own keeper rate` summed. **The
/// sum is over sites rather than one division of the web's whole bill**, because since the kit
/// became per site the web has no single rate to divide by: a hoed Field and a bare patch owing the
/// same demand need different numbers of hands.
fn spare_keepers(workers: u32, worker_need: f32) -> u32 {
    let needed = worker_need.ceil().max(0.0) as u32;
    workers.saturating_sub(needed)
}

/// [`source_banking_its_first_work`] with [`head_rung_gate`] wired to this system's resources.
///
/// **A function rather than a reusable closure, because `advance_labor_allocation` asks it twice**
/// — once of the allocation the player left (the keeping bill the shedding order counts spare
/// keepers against) and once of what survived the shed (the pool [`maintenance_shares`] funds). A
/// closure capturing `allocation` would hold it borrowed across [`LaborAllocation::normalize`]'s
/// `&mut`, so the alternative is the argument list written out twice at the call sites.
#[allow(clippy::too_many_arguments)] // every seam the head's own rung gate is judged by
fn band_banking(
    allocation: &LaborAllocation,
    forage_registry: &ForageRegistry,
    herds: &HerdRegistry,
    // **The road registry and this band's id**, because the route branch's gate is answered off the
    // tile's KEEPER rather than off a labor row ([`route_head_gate`]).
    roads: &crate::routes::RoadRegistry,
    band_id: Option<BandId>,
    faction: FactionId,
    discovery: &DiscoveryProgressLedger,
    knowledge_threshold: f32,
    ladder: &LadderConfig,
    fauna: &FaunaConfig,
    labor: &LaborConfig,
    flora: &crate::flora_config::FloraConfig,
    food_sites: &FoodSiteRegistry,
    tile_registry: &TileRegistry,
    tiles: &Query<&Tile>,
    map_seed: u64,
    wrap_horizontal: bool,
) -> SourceBankingFirstWork {
    source_banking_its_first_work(allocation, |source, improvement| {
        head_rung_gate(
            source,
            improvement,
            allocation,
            forage_registry,
            herds,
            roads,
            band_id,
            faction,
            discovery,
            knowledge_threshold,
            ladder,
            fauna,
            labor,
            flora,
            food_sites,
            tile_registry,
            tiles,
            map_seed,
            wrap_horizontal,
        )
    })
}

/// **IS ANYTHING COMING FOR THIS BAND** — [`ShedFacts::threatened`], and **the same trigger
/// [`advance_predator_raids`] fires on**: a carnivore with `aggression > 0` (a herbivore never
/// raids, and an unaggressive carnivore does not either) standing within
/// `fauna.predators.raid_radius` of the band's tile.
///
/// **Read one system early, off the same herd positions.** The raid pass runs straight after
/// `advance_labor_allocation` in the Population stage and nothing moves a herd between them, so a
/// band the pack reaches this turn keeps its guard through this turn's shedding. A second, looser
/// predicate here would let the shedding order disarm a band the raid pass is about to hit.
///
/// A band whose tile cannot be resolved is treated as **threatened**: the guard is the reading that
/// costs people when it is wrong, so an unanswerable question keeps it.
fn band_is_threatened(
    band_pos: Option<UVec2>,
    herds: &HerdRegistry,
    fauna: &FaunaConfig,
    width: u32,
    wrap: bool,
) -> bool {
    let Some(band_pos) = band_pos else {
        return true;
    };
    herds.herds.iter().any(|herd| {
        let Some(def) = fauna.species_by_display(&herd.species) else {
            return false;
        };
        def.diet == Diet::Carnivore
            && def.combat.attack * def.aggression > NO_RAID_ATTACK
            && crate::grid_utils::hex_distance_wrapped(herd.current_pos, band_pos, width, wrap)
                <= fauna.predators.raid_radius
    })
}

/// **THE RAID ATTACK AT WHICH A PACK DOES NOT COME AT ALL** — `attack × aggression`'s own gate in
/// [`advance_predator_raids`], named here so [`band_is_threatened`] states the same threshold rather
/// than a bare zero.
const NO_RAID_ATTACK: f32 = 0.0;

/// **IS THIS SOURCE STILL TEACHING THE FACTION SOMETHING** — [`SourceShedFacts::accruing_knowledge`],
/// the term step 5 passes over.
///
/// Three conditions, and they are exactly [`RungDef::knowledge_accrual`]'s own:
/// the rung this source stands on names a lesson, the faction has not yet completed it, and the
/// row's floor leaves practice to be had (`intensification::learn_multiplier` is `0` at a floor of
/// `0` — *stripping teaches nothing*).
///
/// **What it deliberately does NOT ask is the escapement room** (`crew_is_working_the_source`), the
/// fourth term the live credit is gated on. That room is resolved from this turn's take, which has
/// not happened yet at the top of the pass — so this is *"is there a lesson here to lose"* rather
/// than *"will a lesson be banked this turn"*, and a source standing exactly on its floor reads as
/// teaching. It is the conservative direction: it protects a row from being thinned, never exposes
/// one.
fn source_is_still_teaching(
    rung: &crate::intensification::RungDef,
    floor: f32,
    faction: FactionId,
    discovery: &DiscoveryProgressLedger,
    knowledge_threshold: f32,
) -> bool {
    let Some(lesson) = rung.earns_discovery_id() else {
        return false;
    };
    crate::intensification::learn_multiplier(floor) > NO_PRACTICE
        && !knows(discovery, faction, lesson, knowledge_threshold)
}

/// **THE PRACTICE RATE AT WHICH NOTHING IS LEARNED** — `learn_multiplier`'s own zero, named so
/// [`source_is_still_teaching`] reads as *"is any practice happening"*.
const NO_PRACTICE: f32 = 0.0;

/// **WHAT THE SHEDDING ORDER NEEDS AND [`LaborAllocation`] DOES NOT HOLD** — resolved against the
/// band **as the player left it**, before a single hand is shed, because every one of these is a
/// question about the allocation being cut down rather than about the one that survives.
///
/// It is the whole of this system's part in the order: the steps themselves are walked in
/// [`LaborAllocation::normalize`], so no seam here knows which fact outranks which.
#[allow(clippy::too_many_arguments)] // every seam a source's rung and its keeping bill are read from
fn resolve_shed_facts(
    allocation: &LaborAllocation,
    banking: &SourceBankingFirstWork,
    band_pos: Option<UVec2>,
    faction: FactionId,
    forage_registry: &ForageRegistry,
    herds: &HerdRegistry,
    tile_capacity_of: &dyn Fn(UVec2) -> f32,
    forage: &crate::labor_config::ForageLaborConfig,
    fauna: &FaunaConfig,
    ladder: &LadderConfig,
    discovery: &DiscoveryProgressLedger,
    knowledge_threshold: f32,
    equipment: &crate::equipment_config::EquipmentConfig,
    band_kit: &BandEquipment,
    // **The roads under this band's own tile**, resolved by the caller because the ledger and the
    // tile query are its to hand ([`route_keeping_claims`]). The route pool has no LABOR ROW for
    // this pass to read a claim off — a road's holding is its keeper, not an `assignments` entry —
    // so its bill arrives already struck. (It has a *source row* on the wire, `RouteState`; the two
    // are different things and conflating them is what made the branch's material half look
    // impossible.)
    road_claims: &[KeepingClaim],
    width: u32,
    wrap: bool,
) -> ShedFacts {
    let (plant_claims, animal_claims) = keeping_claims(
        allocation,
        banking,
        equipment,
        forage_registry,
        tile_capacity_of,
        forage,
        herds,
        fauna,
        ladder,
    );
    let sources = allocation
        .assignments
        .iter()
        .map(|assignment| match &assignment.target {
            LaborTarget::Forage { tile, floor, .. } => {
                forage_registry
                    .patch(*tile)
                    .map_or(SourceShedFacts::default(), |patch| SourceShedFacts {
                        accruing_knowledge: source_is_still_teaching(
                            crate::forage::patch_rung(patch, ladder),
                            *floor,
                            faction,
                            discovery,
                            knowledge_threshold,
                        ),
                        improved: crate::forage::patch_at_risk_cost(patch) > RUNG_UNSTARTED,
                    })
            }
            LaborTarget::Hunt { fauna_id, floor } => {
                herds
                    .find(fauna_id)
                    .map_or(SourceShedFacts::default(), |herd| SourceShedFacts {
                        accruing_knowledge: source_is_still_teaching(
                            fauna::herd_rung(herd, ladder),
                            *floor,
                            faction,
                            discovery,
                            knowledge_threshold,
                        ),
                        improved: fauna::herd_at_risk_cost(herd) > RUNG_UNSTARTED,
                    })
            }
            // A band-wide role stands on no ground, so it carries neither a lesson nor a meter. No
            // step of the order asks these of a role row; the entry exists to hold the alignment.
            LaborTarget::Scout
            | LaborTarget::Warrior
            | LaborTarget::Agriculture
            | LaborTarget::Husbandry
            | LaborTarget::Roadwork
            | LaborTarget::Builders => SourceShedFacts::default(),
        })
        .collect();
    ShedFacts {
        sources,
        threatened: band_is_threatened(band_pos, herds, fauna, width, wrap),
        spare_agriculture_keepers: spare_keepers(
            allocation.workers_on(&LaborTarget::Agriculture),
            keeping_worker_need(
                equipment,
                band_kit,
                crate::intensification::RungBranch::Plant,
                allocation.workers_on(&LaborTarget::Agriculture),
                &plant_claims,
            ),
        ),
        spare_husbandry_keepers: spare_keepers(
            allocation.workers_on(&LaborTarget::Husbandry),
            keeping_worker_need(
                equipment,
                band_kit,
                crate::intensification::RungBranch::Animal,
                allocation.workers_on(&LaborTarget::Husbandry),
                &animal_claims,
            ),
        ),
        spare_roadwork_keepers: spare_keepers(
            allocation.workers_on(&LaborTarget::Roadwork),
            keeping_worker_need(
                equipment,
                band_kit,
                crate::intensification::RungBranch::Route,
                allocation.workers_on(&LaborTarget::Roadwork),
                road_claims,
            ),
        ),
    }
}

#[allow(clippy::too_many_arguments)] // one source, one rung, and every seam its gate is judged by
fn maintenance_shares(
    allocation: &LaborAllocation,
    banking: &SourceBankingFirstWork,
    forage_registry: &ForageRegistry,
    tile_capacity_of: &dyn Fn(UVec2) -> f32,
    forage: &crate::labor_config::ForageLaborConfig,
    herds: &HerdRegistry,
    fauna: &FaunaConfig,
    ladder: &LadderConfig,
    equipment: &crate::equipment_config::EquipmentConfig,
    band_kit: &BandEquipment,
) -> Vec<KeepingAward> {
    let mut awards = vec![KeepingAward::default(); allocation.assignments.len()];
    let (mut plant, mut animal) = keeping_claims(
        allocation,
        banking,
        equipment,
        forage_registry,
        tile_capacity_of,
        forage,
        herds,
        fauna,
        ladder,
    );
    let mode = allocation.upkeep_fund_mode;
    for (role, branch, claims) in [
        (
            LaborTarget::Agriculture,
            crate::intensification::RungBranch::Plant,
            &mut plant,
        ),
        (
            LaborTarget::Husbandry,
            crate::intensification::RungBranch::Animal,
            &mut animal,
        ),
    ] {
        claims.sort_by(|a, b| {
            b.invested
                .total_cmp(&a.invested)
                .then_with(|| a.tiebreak.cmp(&b.tiebreak))
        });
        // **THE SAME SUPPLY EXPRESSION A BUILD DIVIDES ITS PILE BY** (§4.8) — an equipped keeper
        // covers more demand than a bare one, and the rung's demand is untouched by either. See
        // [`KeepingRate`] for where each site's kit comes from.
        //
        // # ⛔ WHAT IS SPLIT IS THE WORKERS, AND THE UNIT IS EACH SITE'S OWN WORKER-NEED
        //
        // The pool used to be one work total struck at one rate for the whole web, divided in
        // proportion to **work demand**. With the kit on the site there is no one rate to strike it
        // at, so the **head count** is what the band actually has to divide and a site's claim on it
        // is `demand ÷ what one of its own keepers delivers`. What each site is then supplied is
        // `its hands × its own rate`.
        //
        // **It is the same arithmetic wherever the rates agree**, which is every branch on the
        // shipped roster: with one `r`, `d_i / r` is `d_i` scaled by a constant, `distribute_upkeep_pool`
        // is homogeneous in that constant under both modes, and `w_i × r` lands exactly on the share
        // the work-unit split produced. The proof that this change moves nothing that ships is that
        // equality — see `upkeep_kit_per_site_is_pacing_neutral_on_the_shipped_roster`.
        let keepers = allocation.workers_on(&role);
        let rates = keeping_rates(equipment, band_kit, branch, keepers, claims);
        let needs: Vec<f32> = claims
            .iter()
            .zip(&rates)
            .map(|(claim, rate)| rate.worker_need(claim.demand))
            .collect();
        for ((claim, rate), hands) in
            claims
                .iter()
                .zip(&rates)
                .zip(distribute_upkeep_pool(keepers as f32, &needs, mode))
        {
            awards[claim.index] = KeepingAward {
                work: hands * rate.per_worker,
                wear_kit: Some(rate.wear_kit.clone()),
            };
        }
    }
    awards
}

/// **WHAT THE ROADS THIS BAND KEEPS CLAIM FROM ITS `Roadwork` POOL** — one claim per road **tile the
/// band is the keeper of**, index-aligned with the returned tiles.
///
/// # ⛔ THE CATCHMENT IS THE KEEPER, NOT WHO IS STANDING THERE
///
/// [`crate::routes::RoadRegistry::kept_by`] — the band that issued `grade` or `pave` on that tile,
/// and nobody else. **One keeper per tile, no shares**, which is what finally disposes of the
/// *"several bands each pay a part"* model Ray rejected: it is unrepresentable here rather than
/// merely discouraged.
///
/// **The band's position does not enter it.** A band four tiles from a road it graded goes on paying
/// for it — what distance costs is priced into the road's own `keeper_remoteness`, never into
/// whether the bill exists. That is *"distance is a cost, never a wall"* in the one place it would
/// otherwise have become a wall.
///
/// **The claims carry no assignment index**, unlike [`keeping_claims`]': a road has no labor row of
/// its own, so `KeepingClaim::index` indexes the returned tile vector instead. That is the only
/// structural difference between this pool and the two food webs' — everything downstream
/// ([`keeping_rates`], [`KeepingRate::worker_need`],
/// [`crate::intensification::distribute_upkeep_pool`]) is the identical seam, which is the point:
/// **a road keeper is funded exactly as a field or a flock keeper is.**
///
/// Sorted **most-invested first** on the road's own position, tie-broken on the tile coord — the
/// total order `UpkeepFundMode::Priority` funds in, stated here because `distribute_upkeep_pool`
/// funds in slice order and the caller owns the ranking.
///
/// **No claim can carry a NAMED kit**: there is no per-road row for a player to name one on, so
/// every road takes the roster's own derivation. **The derivation is per road**, because the route
/// branch's keeping tools are bound to a rung
/// ([`crate::equipment_config::EquipmentEffect::rung`]) — a dirt road and a paved one on one band's
/// books are kept with different tools, and one kit resolved for the band would hand whichever it
/// happened to pick to both.
fn route_keeping_claims(
    registry: &crate::routes::RoadRegistry,
    band: Option<BandId>,
    equipment: &crate::equipment_config::EquipmentConfig,
    tile_registry: &TileRegistry,
    tiles: &Query<&Tile>,
    ladder: &LadderConfig,
) -> (Vec<UVec2>, Vec<KeepingClaim>) {
    let Some(band) = band else {
        return (Vec::new(), Vec::new());
    };
    let mut kept: Vec<UVec2> = Vec::new();
    let mut claims: Vec<KeepingClaim> = Vec::new();
    for (tile, road) in registry.kept_by(band) {
        let measure = crate::routes::road_measure(road, tile_registry, tiles);
        // **The rung the road STANDS on**, which is what its keepers are holding — not whatever a
        // queued `pave` is climbing toward.
        let rung = road.held_rung();
        claims.push(KeepingClaim {
            index: kept.len(),
            kit: equipment.keeping_kit_for(
                None,
                crate::intensification::RungBranch::Route,
                Some(&rung.wire_key()),
            ),
            rung: Some(rung),
            // **The stamped bill where this turn's pass has struck one, the live demand where it
            // has not** — `routes::road_keeping_basis`, the plant web's own rule.
            demand: crate::routes::road_keeping_basis(road, measure, ladder),
            // *"Most invested"* on the route branch is how far up it the tile has been worked: the
            // position **is** the accumulator here, so there is no separate stored cost to read.
            invested: road.position(),
            tiebreak: format!("{:010}:{:010}", tile.y, tile.x),
        });
        kept.push(tile);
    }
    claims.sort_by(|a, b| {
        b.invested
            .total_cmp(&a.invested)
            .then_with(|| a.tiebreak.cmp(&b.tiebreak))
    });
    (kept, claims)
}

/// **THE ROADS' BILL, AND THE STONE THAT PAYS IT** — struck **before** the builders run, and that
/// ordering is the whole reason this is its own system.
///
/// # ⛔ HOLDING WHAT YOU HAVE OUTRANKS EXPANDING
///
/// A band's **standing** paved roads take their stone before a new paving build may touch the store.
/// Pushing a road out can no longer quietly starve the roads already under it — which is what
/// happened while the build pile settled in `advance_labor_allocation` and the standing rate settled
/// after it: the build simply got there first, an ordering nobody chose.
///
/// # WHY THE ORDERING MOVED HERE RATHER THAN THE BILL MOVING LATER
///
/// ⛔ **BOTH OF A ROAD'S BILLS MUST BE STRUCK AT ONE POSITION.** The build arm moves a paving road's
/// meter inside the same turn, so a work bill struck on one side of it and a material bill on the
/// other are two readings of two different roads, and `demand − supplied == shortfall` goes false in
/// whichever currency lagged. That invariant is not negotiable, so the **draw** had to move rather
/// than the **stamp** being split — and moving the stamp *earlier* keeps the pair together while
/// putting the material draw ahead of the build's.
///
/// **The pre-accrual position is also the RIGHT one, and roads were the odd branch out.** Both food
/// webs stamp `upkeep_demanded` *before* the turn's build accrual, for the reason
/// `advance_labor_allocation` states at its own stamp: *"here — between the split and the first
/// accrual — is the one point where the bill and the share describe the same position."* Roads billed
/// **after** their accrual until this pass existed. So this is one correction, not a trade.
///
/// **What stays behind in [`settle_bands_roadwork`] is the WORK payment alone**, because that half
/// needs the one thing this pass cannot have: the `roadwork` head count **the shedding order left**,
/// which does not exist until `advance_labor_allocation` has run.
///
/// ⛔ **THE PLANT AND ANIMAL WEBS ARE NOT REORDERED.** `settle_material_upkeep` still settles their
/// standing materials and the build pile in one call, ranked by the player's own `SourcePriority`.
/// What changed is that the ROUTE branch's standing draw now happens before that call rather than
/// after it — so a road's keeping outranks a *build*, including a pen's, on any material they share.
/// Nothing reorders *within* the two food webs, and on the shipped roster nothing is shared at all:
/// a pen eats hurdles and a road eats stone.
pub fn bill_and_stock_roads(
    mut registry: ResMut<crate::routes::RoadRegistry>,
    ladder: Res<LadderConfigHandle>,
    equipment: Res<EquipmentConfigHandle>,
    tile_registry: Res<TileRegistry>,
    tiles: Query<&Tile>,
    mut bands: Query<(&mut PopulationCohort, &BandId), With<BandId>>,
) {
    let ladder = ladder.get();
    let equipment_cfg = equipment.get();

    // ## (a) The bill, on every road in the world — **both currencies, in one pass**.
    //
    // ⛔ **THE TWO STAMPS ARE STRUCK TOGETHER, AT THE PRE-ACCRUAL POSITION** — see this system's own
    // note. The `get_or_insert` shape is kept: a road already billed this turn keeps that bill, and
    // `advance_roads` is what clears it — one turn's statement on one cycle.
    for road in registry.iter_mut() {
        if road.upkeep_demanded.is_some() {
            continue;
        }
        let measure = crate::routes::road_measure(road, &tile_registry, &tiles);
        road.upkeep_demanded = Some(crate::routes::road_upkeep_demand(road, measure, &ladder));
        road.upkeep_materials_demanded =
            crate::routes::road_upkeep_material_demands(road, measure, &ladder);
    }

    // ## (b) The STONE, out of each keeper's own stores.
    for (mut cohort, band) in bands.iter_mut() {
        let (kept, claims) = route_keeping_claims(
            &registry,
            Some(*band),
            &equipment_cfg,
            &tile_registry,
            &tiles,
            &ladder,
        );
        if claims.is_empty() {
            continue;
        }
        // **Through [`settle_scarce_store`], the seam every other material claim goes through**: a
        // short store splits by the player's own `SourcePriority` and then in proportion to demand,
        // so no road's place in the registry decides anything.
        //
        // ⛔ **A FRACTIONAL RATE IS NOT A ROUNDING PROBLEM HERE, AND MUST NEVER BECOME ONE.** The
        // shipped rate is far below one whole stone a turn, and a material store is a **continuous**
        // fixed-point quantity (`Scalar`, micro-units) rather than a count of discrete items — so a
        // draw of `0.1667` simply subtracts `0.1667`, exactly, and the stock crosses whole units on
        // its own. Rounding the per-turn draw would either lose every charge below half a unit or
        // bill a whole stone every turn; the accumulation *is* the stock, and there is deliberately
        // no second accumulator beside it.
        let material_claims: Vec<(SourcePriority, BTreeMap<String, f32>)> = claims
            .iter()
            .map(|claim| {
                let demand = registry
                    .road(kept[claim.index])
                    .map(|road| road.upkeep_materials_demanded.clone())
                    .unwrap_or_default();
                // A road carries no per-row rank for a player to set, so every road bids at the
                // default tier — the same answer `build_priority` gives a road's build pile.
                (SourcePriority::default(), demand)
            })
            .collect();
        let material_ids: BTreeSet<String> = material_claims
            .iter()
            .flat_map(|(_, demand)| demand.keys().cloned())
            .collect();
        for id in &material_ids {
            let bids: Vec<(SourcePriority, f32)> = material_claims
                .iter()
                .map(|(priority, demand)| {
                    (
                        *priority,
                        demand.get(id.as_str()).copied().unwrap_or(NOTHING_DEMANDED),
                    )
                })
                .collect();
            let settled =
                settle_scarce_store(&bids, cohort.stores.material_total(id.as_str()).to_f32());
            for (claim, paid) in claims.iter().zip(&settled) {
                if *paid <= NOTHING_DEMANDED {
                    continue;
                }
                // **Spent, and decay refunds nothing** (§2.7): stone goes into the roadbed and does
                // not come back out when the position falls.
                cohort
                    .stores
                    .take_material_batches(id, crate::scalar::scalar_from_f32(*paid));
                if let Some(road) = registry.road_mut(kept[claim.index]) {
                    *road
                        .upkeep_materials_supplied
                        .entry(id.clone())
                        .or_insert(NOTHING_DEMANDED) += *paid;
                }
            }
        }
    }
}

/// **PAY FOR THE ROADS THIS BAND KEEPS** — the `Roadwork` keeping pool, the third of the three and
/// the one whose sites carry no **labor row** (`docs/plan_standing_upkeep.md` §4.13b, issue #532):
/// a road is held by its `routes::Road::keeper`, not by an entry in `assignments`. It has a wire
/// **source row** all the same, `RouteState`, keyed by tile as a patch's is.
///
/// **The pool survived the per-tile model and the automatic billing did not.** An earlier cut of
/// this design replaced it with per-tile work rows; that was wrong, because there is no per-turn
/// activity on a road whose output scales with people standing there. Ray: *"a road isn't active
/// like hunting or foraging is so you don't need the tile workers. You just need to say you want a
/// 'road' in this tile and it builds (with builders) and then maintains."* What was actually broken
/// was the **automatic** bill; with the free floor free and `grade` the only way onto a paid rung,
/// every road a band pays for is one it chose by typing a command — so `Roadwork` covers *the roads
/// this band is the keeper of*, exactly as `Agriculture` covers the patches it cultivated.
///
/// # ⛔ IT IS CALLED FROM INSIDE [`advance_labor_allocation`], AND THAT IS THE WHOLE ARRANGEMENT
///
/// **It was a system of its own, `.after(advance_labor_allocation)`, and that ordering published a
/// false countdown.** [`crate::routes::advance_roads`] clears `Road::upkeep_supplied` a whole stage
/// earlier, so a payment made *after* the labour pass left that field at **zero for the whole of
/// it** — and the road build quote struck inside that pass reads exactly that field, through
/// [`crate::routes::road_meter_rot`]. Every billed road therefore quoted its rot at a work shortfall
/// of `1.0` whatever its keepers had done, and a road **fully funded** whose `neglect_turns` still
/// stood above its grace published the full rot in place of the `0` it had earned. Both food webs
/// settle their keeping *inside* the labour pass, ahead of the quote that reads it; the route branch
/// was the odd one out.
///
/// **It cannot move any earlier than this.** The head count it divides is the one **the shedding
/// order left** ([`crate::components::LaborAllocation::normalize`]), which does not exist until that
/// shed has run — which is why the payment ended up late in the first place. So the seat is: after
/// the shed, ahead of the band's `continue`s, and a whole assignment loop before the road build arm.
///
/// ⛔ **IT DOES NOT MOVE THE STAMP, AND THE TWO BILLS STAY STRUCK AT ONE POSITION.**
/// [`bill_and_stock_roads`] still strikes `upkeep_demanded` **and** the material pair together,
/// before the builders run; this pays the WORK half against that same stamp, exactly as it did from
/// one system later. `demand − supplied` is the shortfall in both currencies, verbatim, unchanged.
///
/// ⛔ **AND THE PLANT AND ANIMAL PASSES ARE NOT REORDERED.** `settle_material_upkeep` still
/// settles both food webs' standing materials and the build pile in one call, ranked by the player's
/// own `SourcePriority`, exactly where it always ran.
///
/// # ⛔ (a) THE BILL IS STAMPED ON **EVERY** ROAD, KEEPER OR NOT — AND NOT HERE
///
/// This is the load-bearing half, and it belongs to [`bill_and_stock_roads`].
/// [`crate::routes::Road::keeping_is_met`] answers `true` for a road with **no stamped bill** — an
/// honest *"it has not been judged this turn"* — so a pass that stamped only the roads somebody
/// keeps would leave a **keeperless** road reading as kept for ever: never arming its neglect
/// counter, never decaying, never pruned. That is *"a road whose keeper is gone decays like any
/// unkept improvement"* deleted outright, and it would fail as **no decay at all** rather than as a
/// slow one.
///
/// # (b) THE PAYMENT IS THE SAME SUPPLY EXPRESSION THE OTHER TWO POOLS USE
///
/// [`keeping_rates`] at [`crate::intensification::RungBranch::Route`] for the per-worker rate and
/// the wear kit, [`crate::intensification::distribute_upkeep_pool`] for the split, and the band's
/// own [`crate::components::LaborAllocation::upkeep_fund_mode`] for the policy. There is
/// deliberately **no second supply expression**: an equipped road keeper covers more of a road's
/// bill than a bare one for the same reason an equipped tender does, and the day a barrow declares a
/// `build_work` stat serving `route` this seam picks it up with no code change.
///
/// **`upkeep_supplied` accumulates (`+=`)**, the §2.5 rule kept unchanged even though a tile now has
/// exactly one keeper — the split hands a keeper's pool out claim by claim, and the field is cleared
/// once per turn by [`crate::routes::advance_roads`] rather than here.
///
/// # (c) THE BAND'S OWN ROADWORK LEDGER, SUMMED HERE BECAUSE A CLIENT CANNOT
///
/// [`crate::components::LaborAllocation::last_roadwork_demand`] and its supplied twin are the
/// band-level roll-up the Work board renders the `roadwork` role's need from — `fodderNeed`'s exact
/// shape, minted under its exact rule: **road rows are fog-filtered**, so a road out of sight would
/// silently drop out of any client-side total while the band certainly still owes its keeping.
///
/// **The demand is summed before the head-count gate**, so a band with nobody on the role publishes
/// the bill it is failing to pay rather than a zero. That is the alarm, and it is the same reason
/// the hay need is ungated by Foddering.
///
/// **Both are cleared ahead of every exit**, the [`advance_labor_allocation`] rule: a band that
/// abandons its last road must stop republishing last turn's bill. It is also why the call site sits
/// **above** that pass's `continue`s rather than beside the road arm — a band whose whole allocation
/// was shed leaves the loop early, and it still owes what its roads cost.
///
/// **`band` is the keeper's durable id.** A cohort with no `BandId` is not a band the route layer
/// knows — exactly as `balance_supply_networks` refuses one as a pooling endpoint — so the caller
/// skips it: a keeper *is* a `BandId`, and an anonymous cohort has nothing to claim with.
///
/// **The cohort is read-only.** A paved road's standing stone is drawn by [`bill_and_stock_roads`]
/// before the builders run, which is what puts a band's standing roads ahead of a new paving build
/// on the store. What is left here is the WORK payment, and the cohort is read only for its head
/// count.
#[allow(clippy::too_many_arguments)] // The per-band slice of what was a Bevy system's parameter list
pub fn settle_bands_roadwork(
    registry: &mut crate::routes::RoadRegistry,
    cohort: &PopulationCohort,
    allocation: &mut LaborAllocation,
    mut band_equipment: Option<&mut BandEquipment>,
    band: BandId,
    equipment_cfg: &crate::equipment_config::EquipmentConfig,
    ladder: &LadderConfig,
    tile_registry: &TileRegistry,
    tiles: &Query<&Tile>,
) {
    // **(c) cleared ahead of every exit below**, so a band that has put its last road down stops
    // republishing a bill it no longer owes.
    allocation.last_roadwork_demand = NO_ROADWORK_LEDGER;
    allocation.last_roadwork_supplied = NO_ROADWORK_LEDGER;
    let (kept, claims) = route_keeping_claims(
        registry,
        Some(band),
        equipment_cfg,
        tile_registry,
        tiles,
        ladder,
    );
    if claims.is_empty() {
        return;
    }
    // **(c) THE DEMAND IS SUMMED BEFORE THE HEAD-COUNT GATE.** A band with nobody on the role
    // owes exactly this much and this is the field that says so — the hay need's own rule.
    allocation.last_roadwork_demand = claims.iter().map(|claim| claim.demand).sum();
    let keepers = allocation.workers_on(&LaborTarget::Roadwork);
    if keepers == NO_CREW_ON_THIS_ACTIVITY {
        return;
    }
    // **Sized to the band's workers**, `advance_labor_allocation`'s own rule: an absent
    // component means the gear ledger was never built, which reads as start-stocked.
    let band_kit = band_equipment.as_deref().cloned().unwrap_or_else(|| {
        BandEquipment::start_stocked_for(equipment_cfg, available_workers(cohort.working) as f32)
    });
    let rates = keeping_rates(
        equipment_cfg,
        &band_kit,
        crate::intensification::RungBranch::Route,
        keepers,
        &claims,
    );
    let needs: Vec<f32> = claims
        .iter()
        .zip(&rates)
        .map(|(claim, rate)| rate.worker_need(claim.demand))
        .collect();
    let fund_mode = allocation.upkeep_fund_mode;
    for ((claim, rate), hands) in
        claims
            .iter()
            .zip(&rates)
            .zip(distribute_upkeep_pool(keepers as f32, &needs, fund_mode))
    {
        let supplied = hands * rate.per_worker;
        if let Some(road) = registry.road_mut(kept[claim.index]) {
            road.upkeep_supplied += supplied;
        }
        // **(c) this band's own contribution**, accumulated across the roads it keeps.
        allocation.last_roadwork_supplied += supplied;
        // **The keeper's tools are spent on exactly that work** — billed on what the pool
        // *supplied* to this road, never on what the rung demanded. Inert with the shipped bare
        // `none` kit, and wired so a future road kit is a config edit and nothing else.
        charge_keeping_wear(
            band_equipment.as_deref_mut(),
            equipment_cfg,
            Some(&rate.wear_kit),
            supplied,
        );
    }
}

/// Resolve each band's per-worker labor yields (Early-Game Labor, slice 3a). Replaces the retired
/// single-task systems (`advance_harvest_assignments` / `advance_scout_assignments` /
/// `advance_fauna_pursuits`): a band now draws subsistence from *many* in-range sources at once,
/// with yield scaled by the workers assigned to each. Runs in the Population stage after
/// consumption drains the larder, so labor income lands the same turn (matching the old timing).
///
/// - **Forage** `{ tile }`: within `band_work_range` of the band and carrying a `FoodModuleTag` →
///   draws down the tile's depletable forage patch (§0-ii) via the shared `forage_take` primitive
///   (Sustain gather = the regrowth skim; `sustainable` = one turn's net patch regrowth), the plant
///   mirror of the Hunt take. Module-less / unseeded → 0 this turn, assignment kept (source
///   conditions that recover in place). **Out of range lapses** the assignment and returns its
///   workers to the pool (feed entry), the plant twin of the hunt leash: a patch is fixed, so
///   out-of-range can only mean the band walked away.
/// - **Hunt** `{ fauna_id, policy }`: reuses the per-policy ecology ceiling; the take is
///   `min(workers × per_worker_biomass_capacity, policy_ceiling)`, so under-hunting a Sustain herd
///   (`worker_cap < regrowth`) lets it GROW. Tracks a roaming herd out to `band_work_range +
///   hunt_leash_tiles` (leashed follow); past that — or if the herd is gone — the assignment lapses
///   and its workers return to the pool (feed entry).
/// - **Scout**: reveals fog outward from the band. **Warrior**: inert (band-wide standing guard; it
///   does not escort or mitigate a hunt — its first consumer is the Phase 1 predator-raid path).
///
/// Husbandry (Phase E) re-homes here, but **Sustain no longer tames** (slice 3a): a `Tame` hunt
/// fills the herd's domestication meter, while any *stewardship* policy on a **Thriving** source
/// earns the faction the knowledge that source's **current rung** teaches (slice 4 — Herding on a
/// wild herd, Penning on a pastoral one; Cultivation/Seed Selection on the plant side).
/// **THE FIRST SOURCE THIS PASS HAS ALREADY BANKED KEEPING ON THIS TURN**, or `None` if the slate is
/// clean — the read behind [`advance_labor_allocation`]'s once-per-turn guard.
///
/// **The test is the PAIR — banked supply beside a stamped bill — and each half is load-bearing.**
/// The two fields are written together, in the same arm, for the same source, and both are wiped a
/// whole stage earlier by the decay passes (`forage::advance_cultivation`,
/// `fauna::advance_husbandry`), which walk **every** patch and **every** herd unconditionally, ahead
/// of any of their own `continue`s. So the pair standing at the *top* of this pass can only have
/// been written by a previous run of it.
///
/// - **The supply alone is not the test.** A harness may seat `upkeep_supplied` by hand to stand a
///   herd up as *kept last turn* — the state Logistics reads — and that writes no bill. Firing there
///   would be reporting a fixture's own authorship as a driver fault.
/// - **The bill alone is not the test either.** It is stamped for every *worked* source, keeping or
///   none, an honest `Some(0.0)` on a wild patch — so it says only *"a pass has run"*, which is true
///   of a great many harnesses that stage no keeping and double nothing. Measured: on the strict
///   reading twenty-four tests trip a guard where no keeping figure moves.
///
/// What is left is exactly the misuse: supply this pass banked, about to be added to, against a bill
/// that is never re-struck.
///
/// **Gated to match its only call site.** The guard it serves is `#[cfg(debug_assertions)]`, so
/// without the same gate here this function is dead code in a release build — and `cargo clippy`
/// defaults to the dev profile, where the call site compiles and the orphan is invisible.
#[cfg(debug_assertions)]
fn source_with_keeping_already_banked(
    forage: &ForageRegistry,
    herds: &HerdRegistry,
) -> Option<String> {
    if let Some(patch) = forage
        .patches
        .values()
        .find(|patch| patch.upkeep_supplied > NO_UPKEEP_DEMAND && patch.upkeep_demanded.is_some())
    {
        return Some(format!(
            "the patch at ({}, {}) already carries {} keeping work",
            patch.tile.x, patch.tile.y, patch.upkeep_supplied
        ));
    }
    herds
        .herds
        .iter()
        .find(|herd| herd.upkeep_supplied > NO_UPKEEP_DEMAND && herd.upkeep_demanded.is_some())
        .map(|herd| {
            format!(
                "herd {} already carries {} keeping work",
                herd.id, herd.upkeep_supplied
            )
        })
}

/// **WHAT ONE PEN IS SETTLED TO EAT** — the whole feed decision, struck by [`settle_pen_hay`]
/// *before* the assignment loop and merely applied inside it.
///
/// **Two sources, one unit.** A pen eats the grass its fenced footprint grew and the hay its keeper
/// carried in, both fodder, both measured against one demand (`fodder_per_biomass × biomass`).
/// Nothing else feeds it: the keeper's `FOOD` larder is what the *people* eat, and a pen its pasture
/// and hay cannot fill goes underfed and shrinks (`Herd::pen_fed_fraction` → `starve_underfed_pen`).
///
/// Every field is the number the loop stamps or spends; nothing here is recomputed downstream, which
/// is the point. [`Self::fodder_share`] is the **cap** the loop's `LocalStore::take` is made with, and
/// it is covered by the store by construction (the settlement never hands out more than it saw), so
/// the take returns it in full.
#[derive(Debug, Clone, Copy)]
struct PenFeedShare {
    /// The share of this pen's grass demand its footprint already covers
    /// (`Herd::pen_pasture_fraction`).
    pasture_fraction: f32,
    /// Grass units off the band's `FODDER` store — `0.0` without Foddering, which is what keeps a
    /// pen byte-identical to the pre-hay pasture-only one.
    fodder_share: f32,
    /// **The whole fed fraction** — `(footprint_intake + fodder_share) / demand_grass`, clamped
    /// `[0, 1]`, and [`FULLY_SERVED`] when nothing was demanded (a pen with no biomass is not
    /// starving). Stamped onto `Herd::pen_fed_fraction` by the corral arm.
    fed_fraction: f32,
    /// **The hay this pen needs per turn** — `max(0, demand_grass − footprint_intake)`, in fodder
    /// units, and the number the player acts on (grazing is free; hay is what has to be grown).
    /// Summed into the band's own `LaborAllocation::last_fodder_need` by the corral arm, and
    /// differenced against the draw for `Herd::pen_fodder_shortfall`. Those are its only readers —
    /// the gap itself is not published (`penHayNeed` is retired).
    ///
    /// ⛔ **It is the bid BEFORE the Foddering gate**, unlike [`Self::fodder_share`] beside it. A
    /// band that cannot draw hay at all still keeps a herd that is short exactly this much, and a
    /// need zeroed because the remedy is knowledge would hide the very case the readout is for.
    hay_need: f32,
    /// **The hay this pen ASKED THE STORE FOR** — [`Self::hay_need`] *after* the Foddering gate and
    /// *before* the split, so it is what the pen would draw every turn if the store could cover it.
    /// Summed into the band's `LaborAllocation::last_fodder_drain`, which is the rate the published
    /// fodder runway counts down.
    ///
    /// **It is neither of its neighbours.** [`Self::hay_need`] is ungated, so a band that cannot hay
    /// a herd would appear to be emptying a store it never touches; [`Self::fodder_share`] is what a
    /// *short* store could actually pay, and a runway off that would say the store lasts longer the
    /// emptier it gets. What drains a store is what is asked of it — the same forward reading the
    /// larder runway takes on `demand` rather than on last turn's debit.
    fodder_demand: f32,
}

/// **SERVE ONE SCARCE STORE ACROSS EVERY CLAIM ON IT AT ONCE** — [`SourcePriority::High`] in full,
/// then `Normal`, then `Low`, and **within a tier, proportionally to demand** when the remainder
/// cannot cover that tier.
///
/// Returns one settled amount per input claim, index-aligned to `demands`.
///
/// # ⛔ PROPORTIONAL WITHIN A TIER IS THE WHOLE REASON THIS IS NOT A LOOP
///
/// The draws it replaces ran *inside* the assignment loop, each taking what it wanted off the store
/// in turn — so the earliest claim in the vector ate and the last starved, and since
/// [`LaborAllocation::set_assignment`] re-pushes an edited row to the **end**, the row the player had
/// just adjusted was the one served last. Splitting a short tier in proportion needs no second
/// ordering rule at all, so there is nothing left for a vector position to decide.
fn settle_scarce_store(demands: &[(SourcePriority, f32)], available: f32) -> Vec<f32> {
    let mut settled = vec![NOTHING_DEMANDED; demands.len()];
    let mut remaining = available.max(NOTHING_DEMANDED);
    for tier in SourcePriority::SERVED_FIRST_TO_LAST {
        let tier_demand: f32 = demands
            .iter()
            .filter(|(priority, _)| *priority == tier)
            .map(|(_, demand)| demand)
            .sum();
        if tier_demand <= NOTHING_DEMANDED {
            continue;
        }
        // The fraction of its demand every claim in this tier gets. `FULLY_SERVED` is the cap, so a
        // tier the remainder covers is paid in full and nothing is ever handed out twice.
        let served = (remaining / tier_demand).min(FULLY_SERVED);
        for (index, (priority, demand)) in demands.iter().enumerate() {
            if *priority == tier {
                settled[index] = demand * served;
            }
        }
        remaining = (remaining - tier_demand.min(remaining)).max(NOTHING_DEMANDED);
    }
    settled
}

/// **HOW FAR THIS BAND'S HANDS ACTUALLY GO** — a patch inside `band_work_range`, a herd inside
/// `hunt_reach`, and nothing beyond either.
///
/// # ⛔ EVERY SETTLEMENT STRUCK BEFORE THE ASSIGNMENT LOOP MUST ASK IT
///
/// Both arms of the loop lapse an out-of-reach row and `continue` **past** every keeping draw and
/// material spend beneath them, so a settlement that reserved a store for that row would hold a
/// reservation nothing ever draws — starving the row that *is* in reach with a shortfall it did not
/// cause. [`settle_pen_hay`] carried the rule inline first; it is a type here so
/// [`settle_material_upkeep`] beside it cannot state a second version of it.
///
/// The failure it closes: a band keeping two `Normal` pens, one past the leash, with exactly one
/// pen's hurdles on the shelf. Each was settled half; the out-of-leash pen spent nothing; the
/// **in-reach** pen was judged half-short and took the neglect counter, the decay fraction and the
/// shed.
#[derive(Clone, Copy)]
struct BandReach {
    band_pos: UVec2,
    grid_width: u32,
    wrap_horizontal: bool,
    /// [`crate::labor_config::LaborConfig::band_work_range`] — the Forage arm's own lapse distance.
    work_range: u32,
    /// [`crate::labor_config::LaborConfig::hunt_reach`] — the Hunt arm's.
    hunt_reach: u32,
}

impl BandReach {
    /// **Will the assignment loop reach this row this turn?** A herd the registry no longer carries
    /// is `false`, because the Hunt arm lapses that row on the very same `continue`.
    fn holds(&self, target: &LaborTarget, registry: &HerdRegistry) -> bool {
        let (position, limit) = match target {
            LaborTarget::Forage { tile, .. } => (*tile, self.work_range),
            LaborTarget::Hunt { fauna_id, .. } => match registry.find(fauna_id) {
                Some(herd) => (herd.position(), self.hunt_reach),
                None => return false,
            },
            // Every other role is worked where the band stands, so there is no distance to fail.
            _ => return true,
        };
        crate::grid_utils::hex_distance_wrapped(
            self.band_pos,
            position,
            self.grid_width,
            self.wrap_horizontal,
        ) <= limit
    }
}

/// **THE HAY SPLIT, STRUCK ONCE FOR EVERY PEN THIS BAND KEEPS** — the fix for the positional
/// allocation described on [`settle_scarce_store`], and the one place the corral arm's pasture-and-hay
/// arithmetic lives. Served by [`settle_scarce_store`], so within one priority tier a short `FODDER`
/// store splits in proportion to demand and no pen's place in `assignments` decides anything.
///
/// **The arithmetic is the corral arm's own, lifted rather than re-derived.** The Foddering gate is
/// applied here, once for the faction, and a band that has not learned it settles a `0.0` hay share
/// — so every term below collapses to the pasture-only pen exactly as it did before hay existed.
///
/// **It settles only the rows the corral arm will actually reach**: the herd must exist and must be
/// inside the hunt leash, the two gates the arm itself applies before its tend branch. A pen the arm
/// lapses would otherwise hold a reservation nothing ever draws, starving a pen that is in reach.
///
/// # ⛔ `FODDER` IS A STOCK, AND THAT IS THE MODEL
///
/// It is settled here, against the store standing at the **top of the pass** — so a pen eats the hay
/// its band harvested on a *previous* turn. That is the store's own nature (the buffer the
/// overwintering carry rides), and the alternative is the defect this settlement exists to kill:
/// same-turn hay was reachable only by a pen whose row happened to sit after the hay Field's in
/// `assignments`.
///
/// **There is no second store to settle.** A pen used to bid on the keeper's `FOOD` larder for
/// whatever pasture and hay left unpaid, which had to wait until after the loop because provisions
/// are credited *inside* it. Human food is not animal feed, so that bid is gone and with it the whole
/// second pass: what grass and hay do not cover is a **shortfall**, and a shortfall starves the herd.
#[allow(clippy::too_many_arguments)] // the store, the config, the leash, and the knowledge gate
fn settle_pen_hay(
    assignments: &[LaborAssignment],
    registry: &HerdRegistry,
    stores: &LocalStore,
    foddering_known: bool,
    reach: BandReach,
) -> HashMap<String, PenFeedShare> {
    /// One pen's bid: its id, the player's rank on the row, its grass demand, the share of that
    /// demand the footprint covers, and the hay it is asking the `FODDER` store for.
    struct PenBid<'a> {
        fauna_id: &'a str,
        priority: SourcePriority,
        demand_grass: f32,
        pasture_fraction: f32,
        /// What the footprint leaves uncovered, **ungated** — the readout's `hay_need`.
        grass_shortfall: f32,
        fodder_demand: f32,
    }
    // The pens in hand, each with the demands it is about to bid with. Collected first so both
    // stores are split across the whole set rather than one row at a time.
    let mut pens: Vec<PenBid> = Vec::new();
    for assignment in assignments {
        let LaborTarget::Hunt { fauna_id, .. } = &assignment.target else {
            continue;
        };
        let Some(herd) = registry.find(fauna_id) else {
            continue;
        };
        if !herd.is_corralled() {
            continue;
        }
        // **The gate the corral arm itself applies** — see [`BandReach`].
        if !reach.holds(&assignment.target, registry) {
            continue;
        }
        // The grass this pen would eat if nothing else fed it, and the gap its footprint leaves —
        // the corral arm's own two lines.
        let demand_grass = (herd.fodder_per_biomass * herd.biomass).max(0.0);
        let pasture_fraction = if demand_grass > 0.0 {
            (herd.footprint_intake / demand_grass).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let grass_shortfall = (demand_grass - herd.footprint_intake).max(0.0);
        // **THE FODDERING GATE, ASKED ONCE.** No Foddering, no hay bid at all.
        let fodder_demand = if foddering_known {
            grass_shortfall
        } else {
            NOTHING_DEMANDED
        };
        pens.push(PenBid {
            fauna_id: fauna_id.as_str(),
            priority: assignment.priority,
            demand_grass,
            pasture_fraction,
            grass_shortfall,
            fodder_demand,
        });
    }
    if pens.is_empty() {
        return HashMap::new();
    }
    let fodder_bids: Vec<(SourcePriority, f32)> = pens
        .iter()
        .map(|bid| (bid.priority, bid.fodder_demand))
        .collect();
    let fodder_shares = settle_scarce_store(&fodder_bids, stores.get(FODDER).to_f32());
    // With the hay settled every pen's feed is decided, because hay and grass are the whole of it.
    // **One expression in one unit**: what the land grew plus what the keeper carried in, over what
    // the herd eats. Anything it falls short of is a shortfall, and the shortfall starves the herd.
    let mut settled: Vec<(String, PenFeedShare)> = Vec::with_capacity(pens.len());
    for (index, bid) in pens.iter().enumerate() {
        let herd = registry
            .find(bid.fauna_id)
            .expect("every bid was collected from this registry");
        let fodder_share = fodder_shares[index];
        // A pen that demands nothing (no biomass left) is not starving — the same
        // nothing-demanded reading `Herd::pen_fed_fraction` has always carried.
        let fed_fraction = if bid.demand_grass > NOTHING_DEMANDED {
            ((herd.footprint_intake + fodder_share) / bid.demand_grass)
                .clamp(NOTHING_DEMANDED, FULLY_SERVED)
        } else {
            FULLY_SERVED
        };
        settled.push((
            bid.fauna_id.to_string(),
            PenFeedShare {
                pasture_fraction: bid.pasture_fraction,
                fodder_share,
                fed_fraction,
                hay_need: bid.grass_shortfall,
                fodder_demand: bid.fodder_demand,
            },
        ));
    }
    settled.into_iter().collect()
}

/// **WHAT THIS BAND'S MATERIAL STORES ARE SETTLED TO PAY THIS TURN** — the material half of the
/// standing upkeep and of the build pile, struck by [`settle_material_upkeep`] *before* the
/// assignment loop and merely applied inside it (`docs/plan_standing_upkeep.md` §2.7).
///
/// **Both accounts bid on ONE store in ONE call**, per material id: every source's upkeep rate, and
/// the pile the head queue entry wants for the work its builders are about to bank. They are settled
/// together because they are the same scarcity — a band mending fences and raising a new pen out of
/// one pile of hurdles is choosing between them, and two settlements would let the earlier one eat
/// the store before the later one saw it.
struct MaterialSettlement {
    /// **Index-aligned with `assignments`** — the same convention `upkeep_shares` and `last_yields`
    /// follow, so an arm reads its own row by the index it is already iterating on.
    upkeep: Vec<MaterialUpkeepShare>,
    /// The head entry's own draw. [`BuildMaterialDraw::coverage`] is what the arms scale by.
    build: BuildMaterialDraw,
}

/// One source's material keeping for the turn: the bill it was handed, and what the store paid.
#[derive(Debug, Clone, Default)]
struct MaterialUpkeepShare {
    /// The interpolated per-material demand, struck at the **pre-accrual** position — stamped onto
    /// `ForagePatch::upkeep_materials_demanded` / `Herd::upkeep_materials_demanded` on the work
    /// stamp's own first-write-wins rule, and for its reason.
    demanded: BTreeMap<String, f32>,
    /// What [`settle_scarce_store`] handed this claim, per material.
    settled: BTreeMap<String, f32>,
}

impl MaterialUpkeepShare {
    /// Whether this source asked the store for anything at all — the cheap skip every arm takes
    /// before touching a map, since no plant rung and no rung below `animal:pen` declares a material.
    fn is_empty(&self) -> bool {
        self.demanded.is_empty()
    }
}

/// **THE HEAD BUILD ENTRY'S SHARE OF THE PILE** — what the store will let this turn's accrual buy.
///
/// # ⛔ A SHORT STORE STALLS THE BUILD PROPORTIONALLY; IT NEVER REFUSES IT
///
/// [`Self::coverage`] scales **both** the work banked and the materials drawn, so a store covering a
/// third of the hurdles a turn's fencing wants banks a third of the turn's work and eats a third of
/// the pile. The unbanked remainder is **wasted**, not returned to the pool — §2.5's stated rule for
/// an indivisible supplier, and it needs no new machinery because the builders' output was never a
/// stock anyone could carry forward.
///
/// **There is deliberately no affordability gate anywhere.** The five verbs' gate was retired in
/// §2.5; a build the store cannot cover **queues and stalls**, which is what a build whose builders
/// walk away already does.
#[derive(Debug, Clone)]
struct BuildMaterialDraw {
    /// **`s` — the fraction of this turn's accrual the store can pay for**, `min(1, min over the
    /// declared materials of settled / wanted)`. [`FULLY_SERVED`] when the head declares no material
    /// at all, which is every rung on the shipped ladder but `animal:pen`.
    coverage: f32,
    /// The whole pile this turn's accrual would draw at full coverage, per material — so the spend
    /// is `coverage × wanted` and the arithmetic is stated once.
    wanted: BTreeMap<String, f32>,
}

impl BuildMaterialDraw {
    /// **A BUILD THAT ASKS THE STORE FOR NOTHING** — full coverage and an empty pile, which is what
    /// a band with no queue, no builders, a refused head gate or a rung declaring no material gets.
    fn unbilled() -> Self {
        Self {
            coverage: FULLY_SERVED,
            wanted: BTreeMap::new(),
        }
    }
}

/// **THE MATERIAL SETTLEMENT, STRUCK ONCE FOR THE WHOLE BAND** — the material twin of
/// [`settle_pen_hay`], served by [`settle_scarce_store`] for the same reason: within one priority
/// tier a short store splits **in proportion to demand**, so no source's place in `assignments`
/// decides anything and an edited row re-pushed to the end is not starved.
///
/// ⛔ **`SourcePriority` IS THE ONLY ORDERING, AND `upkeep_mode` IS NOT READ**
/// (`docs/plan_standing_upkeep.md` §4.9 item 12). The rank is the player's own per-row answer; the
/// fund mode exists for a **pool** that has none, and reading both would let a row the player marked
/// `High` starve with nothing on screen saying why. [`distribute_upkeep_pool`] keeps the work half
/// and takes no material axis.
///
/// **The store is read at the top of the pass**, exactly as the hay is: a material is a stock, so
/// what a band holds now is what it can spend now, and a same-turn craft reaches next turn's bill.
#[allow(clippy::too_many_arguments)] // both registries, both configs, the store and the head entry
fn settle_material_upkeep(
    assignments: &[LaborAssignment],
    forage_registry: &ForageRegistry,
    registry: &HerdRegistry,
    tile_capacity_of: impl Fn(UVec2) -> f32,
    forage_cfg: &crate::labor_config::ForageLaborConfig,
    fauna: &FaunaConfig,
    ladder: &LadderConfig,
    stores: &LocalStore,
    build: BuildMaterialDraw,
    build_priority: SourcePriority,
    reach: BandReach,
) -> MaterialSettlement {
    let mut upkeep: Vec<MaterialUpkeepShare> =
        vec![MaterialUpkeepShare::default(); assignments.len()];
    for (index, assignment) in assignments.iter().enumerate() {
        // **A ROW THE ARM WILL NOT REACH BIDS FOR NOTHING** — [`BandReach`]'s own rule, the same one
        // [`settle_pen_hay`] applies to the hay. The arm `continue`s past `apply_material_keeping`
        // for an out-of-leash row, so a claim settled here would reserve hurdles nothing ever spends
        // and leave the pen that *is* in reach judged short.
        if !reach.holds(&assignment.target, registry) {
            continue;
        }
        upkeep[index].demanded = match &assignment.target {
            LaborTarget::Forage { tile, .. } => forage_registry
                .patch(*tile)
                .map(|patch| {
                    crate::forage::patch_upkeep_material_demands(
                        patch,
                        ladder,
                        tile_capacity_of(*tile),
                        forage_cfg,
                    )
                })
                .unwrap_or_default(),
            LaborTarget::Hunt { fauna_id, .. } => registry
                .find(fauna_id)
                .map(|herd| fauna::herd_upkeep_material_demands(herd, fauna, ladder))
                .unwrap_or_default(),
            _ => BTreeMap::new(),
        };
    }
    // The union of every id either account names, so one `settle_scarce_store` call answers for one
    // material and no id is settled twice.
    let ids: BTreeSet<String> = upkeep
        .iter()
        .flat_map(|share| share.demanded.keys())
        .chain(build.wanted.keys())
        .cloned()
        .collect();
    // **The build's coverage is the WORST of its materials**, so a store rich in one good and empty
    // of another stalls the build at the empty one's rate. Folded across the ids below; `FULLY_SERVED`
    // is the identity a build declaring nothing keeps.
    let mut coverage = FULLY_SERVED;
    for id in &ids {
        // The claim vector: every source's upkeep in assignment order, then the build's, so the
        // settled vector splits back on the same index.
        let mut claims: Vec<(SourcePriority, f32)> = assignments
            .iter()
            .enumerate()
            .map(|(index, assignment)| {
                (
                    assignment.priority,
                    upkeep[index]
                        .demanded
                        .get(id.as_str())
                        .copied()
                        .unwrap_or(NOTHING_DEMANDED),
                )
            })
            .collect();
        let wanted = build
            .wanted
            .get(id.as_str())
            .copied()
            .unwrap_or(NOTHING_DEMANDED);
        claims.push((build_priority, wanted));
        let settled = settle_scarce_store(&claims, stores.material_total(id.as_str()).to_f32());
        for (index, share) in upkeep.iter_mut().enumerate() {
            if settled[index] > NOTHING_DEMANDED {
                share.settled.insert(id.clone(), settled[index]);
            }
        }
        if wanted > NOTHING_DEMANDED {
            let paid = settled[assignments.len()];
            coverage = coverage.min(paid / wanted);
        }
    }
    MaterialSettlement {
        upkeep,
        build: BuildMaterialDraw {
            coverage: coverage.clamp(NOTHING_DEMANDED, FULLY_SERVED),
            wanted: build.wanted,
        },
    }
}

/// **WHAT THE HEAD ENTRY'S TURN OF WORK WOULD SWALLOW AT FULL COVERAGE**, per material — the pile
/// drawn *in proportion to the work banked*, never on completion (`docs/plan_standing_upkeep.md`
/// §2.7).
///
/// `legs` are the entry's own legs in climb order, each carrying the rung, what it still **owes**
/// from the source's position, and its **full span width** on this source's price list. A turn's
/// accrual is walked across them — a queue entry can span two, and **each leg draws its own rung's
/// pile** at `pile × (accrual_in_this_leg / width)`.
///
/// **The owed cap is what makes the completing turn honest**: a leg one work unit from full draws a
/// unit's worth of pile and no more, so the whole climb draws exactly the whole pile.
/// **STAMP ONE SOURCE'S MATERIAL KEEPING AND SPEND WHAT THE STORE SETTLED** — the material twin of
/// the `upkeep_supplied` / `upkeep_demanded` pair, applied in the arm where the source is in hand.
///
/// - the **demand** is stamped **first-write-wins**, the work stamp's own rule: it interpolates on a
///   position that moves later in the turn, and every band's share was struck before the accrual, so
///   the first visit is the one that still sees the bill the shares were split against;
/// - the **supply** is `+=`, because the demand is per **source**: two bands each put a part of it on
///   the ground, and assigning would let whichever band the loop visited last speak for all of them;
/// - and the store is debited by exactly what was settled, which the settlement guarantees it holds.
///
/// **Decay refunds nothing** — nothing anywhere credits a material back when a position falls
/// (`docs/plan_standing_upkeep.md` §2.7). That is what makes neglect self-limiting: the position
/// falls, the rate falls with it, and an abandoned thing decays toward costing nothing rather than
/// bleeding a store for ever.
fn apply_material_keeping(
    stores: &mut LocalStore,
    share: &MaterialUpkeepShare,
    demanded: &mut BTreeMap<String, f32>,
    supplied: &mut BTreeMap<String, f32>,
) {
    if share.is_empty() {
        return;
    }
    if demanded.is_empty() {
        *demanded = share.demanded.clone();
    }
    for (id, paid) in &share.settled {
        if *paid <= NOTHING_DEMANDED {
            continue;
        }
        stores.take_material_batches(id, crate::scalar::scalar_from_f32(*paid));
        *supplied.entry(id.clone()).or_insert(NOTHING_DEMANDED) += *paid;
    }
}

/// **THE LEGS A HEAD ENTRY STILL HAS TO LAY, AS THE MATERIAL DRAW NEEDS THEM** — every rung between
/// where the source stands and the entry's destination, each as `(rung, owed, width)` on **this
/// source's own price list**.
///
/// It is `forage::patch_build_legs`' own walk with the second term the pile needs: `owed` is what is
/// left of the rung from here (which caps a completing turn's draw) and `width` is the rung's whole
/// span (which is what the pile is spread over, so climbing the whole rung draws the whole pile).
///
/// **The price is the one IN FORCE on the source** — `rung_cost` reads the patch's stamped Field
/// quote where a Field leg has started and the reference span otherwise, which is what the arm will
/// charge. The published leg list resolves the *live* quote instead, because it has to date a job
/// that has not started; a draw is against work being banked now.
///
/// # ⛔ A ROAD'S LEG IS REMOTENESS-SCALED AND ITS PILE IS FLAT, AND BOTH FALL OUT OF THIS ONE WIDTH
///
/// `routes::road_rung_span` prices a route rung's span at the keeper's own `keeper_remoteness`, so a
/// road far from the band that keeps it is a **wider** leg — more worker-turns for the same tile,
/// which is the branch's whole distance term.
///
/// **The stone does not scale, and no special case is needed to keep it from scaling.** The draw is
/// `pile × (accrual_in_this_leg / width)` ([`RungDef::build_material_draw`]), and a whole climb banks
/// exactly `width`, so the remoteness in the denominator is cancelled by the remoteness in the work
/// that fills it: **the total is the declared pile at any distance.** A remote road draws its stone
/// *more slowly*, over more turns, and swallows the same twenty. Passing the *unscaled* width here
/// instead would multiply the pile by remoteness — the double tax
/// `intensification_ladder.json`'s `_comment_build_materials` refuses — which is why the width is
/// read through the same seam the arm charges the work against.
fn head_build_legs(
    source: &BuildSource,
    destination: RungKey,
    forage_registry: &ForageRegistry,
    registry: &HerdRegistry,
    roads: &crate::routes::RoadRegistry,
    ladder: &LadderConfig,
) -> Vec<(RungKey, f32, f32)> {
    let mut legs = Vec::new();
    let mut cursor = destination.branch().root_rung();
    while let Some(rung) = cursor.above() {
        if !destination.is_at_or_above(rung) {
            break;
        }
        let span = match source {
            BuildSource::Patch(tile) => forage_registry.patch(*tile).map(|patch| {
                (
                    crate::forage::patch_rung_span(patch, rung, ladder).1,
                    crate::forage::patch_rung_work_done(patch, rung, ladder),
                )
            }),
            BuildSource::Herd(id) => registry.find(id).map(|herd| {
                (
                    herd.rung_cost(rung, ladder),
                    herd.rung_work_done(rung, ladder),
                )
            }),
            // **A road lays legs like anything else**, off its own tile's priced span. The width
            // is `road_rung_span` at this road's stamped `keeper_remoteness` — the same quote the
            // build arm banks against — and the work done is how far into that rung the tile's
            // position has already climbed. See the flat-pile note above for why quoting the width
            // at remoteness is what keeps the *stone* flat.
            BuildSource::Road(tile) => roads.road(*tile).map(|road| {
                let (base, width) =
                    crate::routes::road_rung_span(rung, ladder, road.keeper_remoteness);
                (
                    width,
                    (road.position() - base).clamp(NOTHING_DEMANDED, width),
                )
            }),
        };
        if let Some((width, done)) = span {
            let owed = (width - done).max(NOTHING_DEMANDED);
            if owed > NOTHING_DEMANDED {
                legs.push((rung, owed, width));
            }
        }
        cursor = rung;
    }
    legs
}

/// **THE ONE LEG A HEAD *RING* LAYS** — [`head_build_legs`]' twin for the queue entry that widens a
/// pen instead of climbing a rung, in the same `(rung, owed, width)` shape and for the same reader.
///
/// # A RING IS A PEN BUILD, AND THE MATERIAL ACCOUNT IS WHERE THAT WAS LAST UNTRUE
///
/// Widening a fence is the same fencing labour on the same `animal:pen` record: the ring is priced at
/// that rung's [`RungDef::build_cost`], raised by the same builders pool, and wears the same keepers'
/// tools. **So it eats the same pile.** It was materially free for exactly one turn of this arc's
/// life, because [`SourceBankingFirstWork`] answers a *verb* question and a ring names no verb — so
/// nothing bid for it and it was quoted at [`FULLY_SERVED`].
///
/// Stating it as a **leg** rather than as a second draw is what keeps the two honest: the leg goes
/// through [`build_material_wants`] with every rung leg, so *"a pile draws in proportion to the work
/// banked"* has one expression, and the `owed` cap (`ring_cost − pen_extend_progress`) makes the
/// closing turn draw only the remainder exactly as a rung's completing leg does.
///
/// `None` where there is no ring to pay for — the source is not a herd, the herd has left the
/// registry, the pen rung has no build meter, or [`Herd::pen_extending`] (**the ring's whole gate**,
/// the same flag the arm passes as the rung's `eligible`) is clear.
fn head_ring_leg(
    source: &BuildSource,
    registry: &HerdRegistry,
    ladder: &LadderConfig,
) -> Option<(RungKey, f32, f32)> {
    let BuildSource::Herd(fauna_id) = source else {
        return None;
    };
    let herd = registry.find(fauna_id)?;
    if !herd.pen_extending {
        return None;
    }
    // **The ring's width is the pen's own cost, unscaled** — the arm's `ring_cost`, read off the same
    // rung record so a retune moves both or neither. Penning takes no per-species multiplier.
    let width = ladder
        .rung(RungKey::AnimalPen)
        .build_cost(RUNG_COST_UNSCALED)?;
    let owed = (width - herd.pen_extend_progress).max(NOTHING_DEMANDED);
    (owed > NOTHING_DEMANDED).then_some((RungKey::AnimalPen, owed, width))
}

fn build_material_wants(
    legs: &[(RungKey, f32, f32)],
    accrual: f32,
    ladder: &LadderConfig,
) -> BTreeMap<String, f32> {
    let mut wants: BTreeMap<String, f32> = BTreeMap::new();
    let mut remaining = accrual.max(NOTHING_DEMANDED);
    for (rung, owed, width) in legs {
        if remaining <= NOTHING_DEMANDED {
            break;
        }
        let banked = remaining.min(*owed);
        remaining -= banked;
        let def = ladder.rung(*rung);
        for (id, _) in def.build_materials() {
            let draw = def.build_material_draw(id, banked, *width);
            if draw > NOTHING_DEMANDED {
                *wants.entry(id.to_string()).or_insert(NOTHING_DEMANDED) += draw;
            }
        }
    }
    wants
}

// **RETIRED: `settle_pen_larder` / `PenLarderBid`** — the bread half of the pen feed, settled across
// every pen after the assignment loop because `FOOD` is credited *inside* it. It drew the keeper's
// larder for whatever pasture and hay left unpaid, and it was a modelling error: **human food is not
// animal feed**. Its real effect was to hide the starvation path — a pen whose pasture failed took the
// food out of its keepers' mouths instead of shrinking, so `starve_underfed_pen` only ever ran once
// the *people* were already starving. What grass and hay leave unpaid is a shortfall now, and
// [`settle_pen_hay`] is the only settlement there is.

/// **THE BAND'S SIDE OF A HUNT, SETTLED — at every rung.** The animal side is already off the herd
/// inside [`crate::systems::hunt_take`] (`take.killed_biomass()`); this is where what the animals did
/// back lands.
///
/// # Why it is a function rather than two copies
///
/// The range arm and the pen's tend branch resolve the *same* fight through the same `hunt_take`
/// since `docs/plan_standing_upkeep.md` §4.9 item 12b — **a contained bull still gores** — so a
/// second copy of this block would be two readings of one event, which is exactly the split
/// §0.1 of `docs/plan_hunt_through_combat.md` closed on the take itself.
///
/// # The death line is gated on a DEATH, not on any casualty
///
/// §4.5: snaring rabbits is not a war. The hunt carries a baseline injury risk (§4.6), so every
/// engagement produces *some* `wounded` and `casualties.any()` would push a "cost 0 lives" line for
/// every band every turn. `killed` come out of the working-age bracket (the casualty mortality
/// path); `wounded` is computed and surfaced but mechanically inert this phase.
///
/// # The hunt report is NOT that line's wounded-only twin
///
/// §6.6: it is what happened, as facts, every turn a hunt happens — the wounded ride there, beside
/// which bound actually ended the take. [`crate::systems::expeditions::hunt_report_event`] returns
/// `None` for a turn that engaged nothing, so a wait turn writes no line.
fn settle_hunt_band_side(
    outcome: &crate::systems::expeditions::HuntOutcome,
    species: &str,
    fauna: &FaunaConfig,
    tick: u64,
    faction: FactionId,
    cohort: &mut PopulationCohort,
    event_log: &mut CommandEventLog,
) {
    // Human text names the SPECIES, never the internal herd id.
    let species_name = fauna
        .species_by_display(species)
        .map(|def| def.display_name.clone())
        .unwrap_or_else(|| species.to_string());
    if outcome.fight.casualties.killed > fauna::NO_DEATHS_TO_REPORT {
        let killed_f = outcome.fight.casualties.killed;
        let wounded_f = outcome.fight.casualties.wounded;
        cohort.apply_combat_casualties(scalar_from_f32(killed_f));
        // The prose rounds `killed` for a readable "cost N lives"; the **detail carries the
        // fractional truth** (casualties are `Scalar`-fractional by design — a well-guarded party
        // takes a fraction of a death), so a consumer reads precise killed/wounded rather than a
        // rounded 0.
        let killed_r = killed_f.round() as u32;
        event_log.push(CommandEventEntry::new(
            tick,
            CommandEventKind::HuntDanger,
            faction,
            format!("The {} hunt cost {} lives", species_name, killed_r),
            Some(format!(
                "killed={:.3} wounded={:.3} species={}",
                killed_f, wounded_f, species_name
            )),
        ));
    }
    if let Some(entry) =
        crate::systems::expeditions::hunt_report_event(tick, faction, &species_name, outcome)
    {
        event_log.push(entry);
    }
}

/// **EVERY PIECE OF A BAND [`advance_labor_allocation`] TOUCHES**, named because the tuple crossed
/// clippy's complexity bar when the bench joined it.
///
/// **The bench is here because the shedding order ranks it beside the rows.** It spends the same pool
/// `assign_labor` does but is not a [`LaborTarget`], so `normalize` has to be *handed* it rather than
/// finding it in `assignments`.
///
/// The three `Option`s are for one reason: a hand-rolled fixture (and a band spawned before a
/// component existed) may carry none of them, and an absent component reads as *nothing there* —
/// no id to link a feed line to, no gear, no bench holding anybody.
type LaborBandParts = (
    &'static mut PopulationCohort,
    &'static mut LaborAllocation,
    Option<&'static mut BandEquipment>,
    Option<&'static BandId>,
    Option<&'static mut BandBench>,
);

#[allow(clippy::too_many_arguments)] // Bevy system parameters require explicit resource access
pub fn advance_labor_allocation(
    mut registry: ResMut<HerdRegistry>,
    mut forage_registry: ResMut<ForageRegistry>,
    mut discovery: ResMut<DiscoveryProgressLedger>,
    mut event_log: ResMut<CommandEventLog>,
    tick: Res<SimulationTick>,
    tile_registry: Res<TileRegistry>,
    // The gathering sites — the plant ladder's site rule reads this, so `Sow`'s placement gate here
    // and the `sow` command's own rejection resolve the same ground (`rung_site_refusal`).
    food_sites: Res<FoodSiteRegistry>,
    sim_config: Res<SimulationConfig>,
    configs: LaborConfigs,
    tiles: Query<&Tile>,
    food_modules: Query<&FoodModuleTag>,
    // `Option<&BandId>`: worldgen gives every real band a durable id, but a hand-built test world
    // may spawn a bare cohort. It is read for one purpose here — the `band=` detail token every line
    // this pass writes about a *row* carries, so the event dock can offer that band's Work tab
    // ([`band_detail_token`]) — so a band without one still works, gathers and lapses exactly as
    // before and simply publishes rows the dock cannot link.
    // **Written here, in three separate acts.** The shed counts a band's spare road keepers against
    // what the roads it KEEPS cost ([`route_keeping_claims`]); the keeping is then **paid** by
    // [`settle_bands_roadwork`], called from this pass ahead of the band's `continue`s; and the build
    // half is this pass's own — see the road arm below. Only the traffic that wears the free floor in
    // belongs elsewhere (`routes::advance_roads`, a whole stage earlier).
    mut roads: ResMut<crate::routes::RoadRegistry>,
    mut cohorts: Query<LaborBandParts>,
) {
    // # ⛔ THIS PASS MAY RUN ONCE PER LOGISTICS CLEAR, AND A SECOND RUN OVERSTATES THE KEEPING
    //
    // The two accounts this pass writes are deliberately asymmetric. `upkeep_supplied` **accumulates**
    // (`+=`) across the bands working a source, because the upkeep is per-SOURCE and two bands each
    // put a fraction of it on the ground; `upkeep_demanded` is stamped **first-write-wins** and is
    // never re-struck, because the bill has to describe the position the shares were split against.
    // Both are cleared a whole stage earlier, by the Logistics decay passes.
    //
    // So a driver that runs this pass **twice with no Logistics pass between** measures a doubled
    // supply against one turn's bill. Measured over three consecutive passes, one patch's
    // `upkeep_supplied` ran `3.7037 → 5.5556 → 7.4074` against a demand stamped once at `1.8519` —
    // it **overstates keeping**, silently, and everything struck from the pair (the shortfall, the
    // rot, the neglect counter, the published `upkeepShortfall`) is wrong in the flattering
    // direction.
    //
    // **THE PRODUCTION ORDERING IS NOT THE DEFECT AND MUST NOT BE "FIXED".** The accumulation across
    // bands within one turn is what makes a multi-band holding add up at all. What must stop being
    // silent is the **misuse** — so this is a debug-only guard on the driver rather than a runtime
    // refusal: a guard that quietly skipped the second stamp would leave the doubled supply standing
    // while claiming the pair was sound, which is the same wrong number with the alarm switched off.
    #[cfg(debug_assertions)]
    if let Some(source) = source_with_keeping_already_banked(&forage_registry, &registry) {
        panic!(
            "advance_labor_allocation ran twice with no Logistics pass between them: {source} \
             from the previous run. `upkeep_supplied` accumulates across bands while \
             `upkeep_demanded` is stamped once and never re-struck, so this pass would measure a \
             doubled supply against one turn's bill and report the keeping as better met than it \
             is. Run a whole turn — or `forage::advance_cultivation` + `fauna::advance_husbandry` \
             — between labour passes."
        );
    }
    let fauna = configs.fauna.get();
    let labor = configs.labor.get();
    let flora = configs.flora.get();
    let ladder = configs.ladder.get();
    let wellbeing = configs.wellbeing.get();
    // **Predators Phase 0 — the hunt-danger seam** (`docs/plan_predators.md`). The resolver tuning and
    // the base human's intrinsic combat profile, resolved once: a dangerous hunt builds a fight from
    // the hunting party (the hunters on that herd) vs the animal's fighting stock and applies the
    // band-side casualties. Hoisted out of the per-cohort loop — neither changes within a turn.
    let combat_config = configs.combat.get();
    let combat_tuning = combat_config.tuning();
    // The hunt's own baseline hazard, per animal engaged — hoisted with the tuning it rides beside.
    let hunt_injury_damage = combat_config.hunt_injury_damage_per_animal;
    let person_profile = configs.creatures.get().person();
    // **The minimal TOE** (`docs/plan_hunt_through_combat.md` §4.8) — the two-tier table and the
    // durability dials, resolved once. What varies per band is only its `BandEquipment` *wear*.
    let equipment_cfg = configs.equipment.get();
    // **The materials table** (`docs/plan_crafting_and_materials.md` §1) — resolved once, because it
    // decides only how a stated reading BANDS. What each source yields is that source's own config.
    let materials_cfg = configs.materials.get();
    // The recipe book, for the bench half of `LaborAllocation::material_income` — see
    // [`LaborConfigs::recipes`].
    let recipes_cfg = configs.recipes.get();
    // **A band with no `BandEquipment` at all** — a hand-built test world may spawn a bare cohort,
    // and `bench_tiers` wants a wear ledger to read a live tool out of. An empty one is the honest
    // answer for a band that owns nothing: every tier falls back to the material's bare-handed rate.
    let no_kit_at_all = BandEquipment::default();
    // The **no-equipment baselines** of the two carry kits. `labor_config.json`'s rates are what a
    // sledless party drags and a bare-handed forager holds; the *equipped* side of each lives on its
    // item's own tier now, and `EquipmentConfig::{hunt,forage}_per_worker_biomass_capacity` is what
    // steps a band up to it. **One kit, one job** (§4.8): the sled answers for the hunt, baskets for
    // the gather, and neither can be read for the other.
    let baseline_haul_rate = labor.hunt.per_worker_biomass_capacity;
    let baseline_gather_rate = labor.forage.per_worker_biomass_capacity;
    // **The EQUIPPED reference gather rate** — what a *kitted* crew carries, off the item table's
    // **RETIRED: `equipped_gather_reference`** — the equipped tier a rung-3 Field's collection cap
    // used to be quoted at, rather than at the working crew's own basket. Its whole justification was
    // that a Field's harvest drew no biomass down and so had no quantum to charge a basket against;
    // the Field is drawn down like every other plant rung now, so it is basket-resolved with them and
    // there is nothing left to quote at a reference.
    let map_seed = sim_config.map_seed;
    let husbandry = &fauna.husbandry;
    let work_range = labor.band_work_range;
    let hunt_reach = labor.hunt_reach();
    // The forward-projection horizon for each source's steady `realized` yield: `realized` is the
    // average food/turn the source will deliver over the next N turns, simulated forward from its
    // current (pre-take) state, so the headline "Food /turn" is smooth and the assign-time seed matches
    // the first resolved value exactly.
    let realized_horizon = labor.yield_average_horizon_turns;
    // The horizon for each source's discrete **arrival schedule** — what lands on each of the next N
    // turns, from the same forward simulation `realized` averages, reported per TURN instead of
    // averaged. **The lumpiness is the whole-animal quantiser and the herd's own regrowth**, not a
    // kill-credit bank: `project_arrivals_hunt` never touches `Herd::hunt_credit`, which left the
    // resident path when the take became a stock (see `Herd::hunt_credit`). Its own lever: a schedule
    // is a display span the client charts, where `realized_horizon` is a smoothing window.
    let arrivals_horizon = labor.arrivals_horizon_turns;
    // **The ladder's knowledge dials (§4)** — the per-turn accrual every teaching rung pays, and the
    // ledger bar at which a faction may act on a knowledge. Hoisted out of the per-cohort loop.
    // **One pair for BOTH webs**: these used to be duplicated at identical values in
    // `labor_config.forage.cultivation` and `fauna_config.husbandry`, back when each web had its own
    // hard-coded earn site. The earn path is one rung-driven seam now, so the dials live on the
    // ladder with the build dials — the plant and animal ladders can only be paced together.
    // **The ladder's `knowledge.learn_rate` is the PRACTICE, not the ledger amount**: it is scaled per
    // call by the assignment's own floor (`intensification::learn_multiplier`), so the whole block
    // travels to `credit_rung_lesson` rather than a pre-multiplied delta.
    let knowledge_dials = &ladder.knowledge;
    let knowledge_threshold = ladder.knowledge.completion_threshold;
    // The two rungs the build engine drives (`crate::intensification`): the plant's tended patch and
    // the animal's pen. Their build dials — accrual rate, feral decay, and the investment dip — are
    // the ladder's, not each web's, so the two paths can never be tuned apart. Hoisted out of the
    // per-cohort loop alongside the knowledge levers.
    let tended_rung = ladder.rung(RungKey::PlantTended);
    let field_rung = ladder.rung(RungKey::PlantField);
    let pastoral_rung = ladder.rung(RungKey::AnimalPastoral);
    let pen_rung = ladder.rung(RungKey::AnimalPen);
    // In-range checks use true hex distance (not Chebyshev on offset coords, whose square
    // corners are actually 3 hex-steps away), wrap-aware to match the rest of the sim.
    let grid_width = tile_registry.width;
    let grid_height = tile_registry.height;
    let wrap_horizontal = sim_config.map_topology.wrap_horizontal;
    // **WHICH CREW'S BUILD ESTIMATE EACH SOURCE PUBLISHES** — see [`BuildEstimateClaims`]. Declared
    // outside the band loop because the whole point is that several *bands* may work one source in a
    // turn; one set per web, keyed by whatever names a source there (a patch by its tile, a herd by
    // its id).
    let mut patch_build_claims: BuildEstimateClaims<UVec2> = BuildEstimateClaims::default();
    let mut herd_build_claims: BuildEstimateClaims<String> = BuildEstimateClaims::default();

    for (mut cohort, mut allocation, mut band_equipment, band_id, mut bench) in cohorts.iter_mut() {
        // **WHOSE WORK BOARD THIS TURN'S LOSSES BELONG TO** — the `band=` token appended to every
        // line below that reports a row the band did not ask to lose. Copied out of the query up
        // front because the announcements are written from several arms of the loop.
        let band_id = band_id.copied();
        // **This band's carry tier, resolved ONCE per band per turn.** The component records what
        // the band OWNS, so an absent **entry inside it** means *not owned* — but an absent
        // **component** means the ledger was never built, and that reads as start-stocked, which is
        // what every spawn path inserts. The two are different questions and only the first is the
        // count slice's flip. Resolved *before* the assignment loop so every source this band works
        // is priced on one kit state: a kit that expires part-way through the loop must not pay two
        // different rates to two herds in the same turn.
        // Normalize each turn: if `working` shrank, trim assignments so Σ ≤ available.
        let available = available_workers(cohort.working);
        // **Sized to the band's workers** — a spawn stocks a party's worth, so the absent-component
        // fallback has to as well or a fixture band would resolve as one armed hand and the rest
        // bare ([`BandEquipment::start_stocked_for`]).
        let band_kit = band_equipment
            .as_deref()
            .cloned()
            .unwrap_or_else(|| BandEquipment::start_stocked_for(&equipment_cfg, available as f32));
        let faction = cohort.faction;
        // **THE SIZE OF THE LAND UNDER A PLANT CLAIM** — the tile's own `K` through the one
        // `forage::tile_forage_capacity` seam, resolved the way every other tile reading in this
        // system is (the registry index, then the query), and handed to
        // `forage::patch_land_capacity` so a coord that is **not on the map** reads the patch's
        // seeded capacity here exactly as it does in the decay pass that bills against this share.
        // Ground with no patch on it presents no load and therefore no claim.
        //
        // Declared at the top of the band's iteration because **both** readers of the keeping bill
        // want it — the shedding order's spare-keeper count below, and `maintenance_shares` further
        // down. It borrows `forage_registry` immutably and nothing mutates that registry between
        // the two.
        let tile_capacity_of = |coord: UVec2| {
            let ground = tile_registry
                .index(coord.x, coord.y)
                .and_then(|entity| tiles.get(entity).ok())
                .map(|tile| tile_forage_capacity(&labor.forage, tile));
            forage_registry
                .patch(coord)
                .map_or(crate::labor_config::NO_FORAGE_CAPACITY, |patch| {
                    crate::forage::patch_land_capacity(patch, ground)
                })
        };
        // **THE SHEDDING ORDER'S FACTS, STRUCK BEFORE A SINGLE HAND IS SHED.** Every one of them is
        // a question about the allocation the *player* left — *has this band more keepers than its
        // bill needs*, *is anything coming for it*, *what is standing on each row's ground* — so the
        // gear and the funded head are resolved here against that allocation, and resolved again
        // below against whatever survives, which is the reading the split funds.
        let shed_banking = band_banking(
            &allocation,
            &forage_registry,
            &registry,
            &roads,
            band_id,
            faction,
            &discovery,
            knowledge_threshold,
            &ladder,
            &fauna,
            &labor,
            &flora,
            &food_sites,
            &tile_registry,
            &tiles,
            map_seed,
            wrap_horizontal,
        );
        // **WHERE THIS BAND IS STANDING** — read once: the shed's threat probe wants it, and so
        // does the route claim beneath it, whose whole catchment is this one tile (rule 2).
        let band_pos = tiles
            .get(cohort.current_tile)
            .map(|tile| tile.position)
            .ok();
        // **What the roads this band KEEPS cost it to hold** — struck here, off the same seam
        // [`settle_bands_roadwork`] funds them through, so *"more road keepers than the bill needs"*
        // and *"what each road is owed"* cannot come from two readings of the same ground. This
        // reading is the **pre-shed** one the shedding order is entitled to; the payment below strikes
        // its own against what survived.
        let (_, road_claims) = route_keeping_claims(
            &roads,
            band_id,
            &equipment_cfg,
            &tile_registry,
            &tiles,
            &ladder,
        );
        let shed_facts = resolve_shed_facts(
            &allocation,
            &shed_banking,
            band_pos,
            faction,
            &forage_registry,
            &registry,
            &tile_capacity_of,
            &labor.forage,
            &fauna,
            &ladder,
            &discovery,
            knowledge_threshold,
            &equipment_cfg,
            &band_kit,
            &road_claims,
            grid_width,
            wrap_horizontal,
        );
        // **EVERY HAND `normalize` SHEDS IS ANNOUNCED, trims and drops alike.** It walks the decided
        // shedding order ([`ShedStep`]) when the band no longer has the people, and it used to do so
        // in total silence — the one place in the labor system that gave up work without saying so,
        // while the out-of-range lapse a hundred lines below has always pushed a feed entry. A row
        // destroyed outright can cost a 25-turn build commitment (the queue entry goes with it on
        // the prune below); a row merely cut is the crew the player set moving on its own. Neither
        // may happen quietly.
        for shed in allocation.normalize(bench.as_deref_mut(), available, shed_facts) {
            announce_shed_crew(&mut event_log, tick.0, faction, band_id, &shed);
        }
        // **THE HAY LEDGER IS CLEARED BEFORE ANY EXIT OUT OF THIS BAND'S TURN**, and re-summed at the
        // foot of the loop from the rows it actually resolved. Every other per-turn ledger the
        // cohort publishes is rebuilt from an emptied container (`last_yields` is resized to the
        // surviving assignments by `normalize` above), and these three are plain accumulators
        // written only at the foot — so a band that takes either `continue` below would keep
        // republishing the previous turn's `fodderNeed` / `fodderIncome` and the runway derived from
        // them, for pens it no longer keeps and Fields it no longer works. The band that loses its
        // last working-age hand sheds every row and leaves here, which is exactly when the stale
        // figures would be least true.
        allocation.last_fodder_need = NO_FODDER_LEDGER;
        allocation.last_fodder_inflow = NO_FODDER_LEDGER;
        // **The standing MATERIAL bill rides the same cycle and the same early-exit rule** — cleared
        // ahead of the shed's `continue`s, so a band that loses its last worker stops republishing
        // last turn's need for holdings it no longer keeps.
        allocation.last_material_need.clear();
        allocation.last_material_income.clear();
        allocation.last_fodder_drain = NO_FODDER_LEDGER;
        // ## ⛔ THE ROADS THIS BAND KEEPS, PAID HERE — AFTER THE SHED AND BEFORE THE QUOTE
        //
        // The third keeping pool ([`settle_bands_roadwork`]), and this seat is the whole of what
        // makes the road build quote further down honest. `routes::advance_roads` clears
        // `Road::upkeep_supplied` a stage earlier and this is its only writer, so while the payment
        // ran as a system *after* this pass the quote's `routes::road_meter_rot` read a supply of
        // **zero** for every road in the world — pinning its work shortfall at `1.0` and publishing
        // the full rot for roads that were fully funded. Both food webs already settle their keeping
        // inside this pass ahead of the quote that reads it; the route branch was the odd one out.
        //
        // **It cannot sit any earlier**: the pool it divides is the `roadwork` head count the shed
        // above left, not the one the player typed. **And it cannot sit any later**: below this line
        // are the two `continue`s, and a band whose whole allocation was shed still owes what its
        // roads cost — the roll-up clears beside the fodder and material ledgers for that reason.
        //
        // **Its own claim reading, deliberately.** `road_claims` above was struck for the shedding
        // order, off the **pre-shed** allocation; this one funds what survived. The two are the same
        // number today — a claim is a property of the ROADS, not of the allocation — and keeping
        // them separate is what stops a later change to one silently retuning the other.
        if let Some(band_id) = band_id {
            settle_bands_roadwork(
                &mut roads,
                &cohort,
                &mut allocation,
                band_equipment.as_deref_mut(),
                band_id,
                &equipment_cfg,
                &ladder,
                &tile_registry,
                &tiles,
            );
        }
        if allocation.assignments.is_empty() {
            continue;
        }
        let Ok(band_pos) = tiles.get(cohort.current_tile).map(|tile| tile.position) else {
            continue;
        };
        // **HOW FAR THIS BAND'S HANDS GO**, bundled once for every settlement struck before the
        // assignment loop — see [`BandReach`] for why a settlement that ignores it starves the rows
        // that *are* in reach.
        let band_reach = BandReach {
            band_pos,
            grid_width,
            wrap_horizontal,
            work_range,
            hunt_reach,
        };
        // Productivity modifier stack (wellbeing): scale every yield by the band's output
        // multiplier at PAYOUT. One call — future modifiers slot into `output_multiplier`.
        let mult = output_multiplier(&cohort, &wellbeing);
        let mult_f = mult.to_f32();

        let mut lapsed: Vec<usize> = Vec::new();
        // **TAKE SELECTIONS A COMMITMENT REPAIRED THIS TURN** — `(assignment index, the pruned
        // selection)`. `LaborTarget::Forage::take_species` has exactly one other writer (the
        // `assign_labor` command) and nothing used to prune it, so a `Cultivate`/`Sow` reweighting
        // the ground could leave a crew asking for plants it had displaced — a zero selected share,
        // and therefore a zero take ceiling in every account at once
        // (`TakeSelection::pruned_for_commitment`).
        //
        // Collected rather than applied in place for `completed`'s reason: the loop borrows the
        // allocation's assignments immutably. Applied **before** the `lapsed` removal below, so
        // these indices are still the ones this loop saw.
        let mut repaired_takes: Vec<(usize, TakeSelection)> = Vec::new();
        // **Builds that COMPLETED this turn** — the source has climbed its rung, so there is
        // nothing left to raise and its **queue entry retires** after the loop, handing the pool to
        // whatever the player put next (`docs/plan_standing_upkeep.md` §2.4: *"at its cost, the
        // entry leaves the queue"*). Collected rather than applied in place because the loop borrows
        // the allocation's assignments immutably.
        //
        // Keyed by **source and job** rather than by assignment index: the queue is what is being
        // edited now, and an index into a list the `lapsed` removal is about to shuffle was only
        // ever right because it was applied first.
        let mut completed: Vec<(BuildSource, BuildJob)> = Vec::new();
        // Retained per-source yield telemetry, rebuilt from scratch: one entry per assignment in
        // iteration order, pre-seeded to zero so any arm that `continue`s (out of range, module
        // lost, herd gone) leaves a correct 0-yield row and index alignment is preserved. This also
        // *overwrites* any assign-time forecast seed (`LaborAllocation::set_source_yield`) with the
        // resolved take — the seed is only the pre-resolution stand-in.
        let mut yields: Vec<SourceYield> = vec![SourceYield::ZERO; allocation.assignments.len()];
        // **THE BAND'S MAINTENANCE POOLS, SPLIT ACROSS ITS SOURCES** — one work amount per
        // assignment index (`maintenance_shares`). Resolved **before** the loop because the split is
        // a property of the band's *whole* holding on a web: what one patch gets depends on what
        // every other one asked for, which nothing inside a per-assignment pass can see.
        // **THE FUNDED HEAD, AND ONLY IF ITS OWN GATE HOLDS** — the claim side's verb term
        // ([`SourceBankingFirstWork`]). A head the ground refuses banks nothing however long it
        // stands there, so letting it claim would dilute the share of everything the band really
        // holds under the default `Spread`.
        // **Re-struck against what SURVIVED the shed**, unlike the pre-shed reading the shedding
        // order was handed: a band whose builders row was emptied above funds no head at all, and
        // the split must not fund one it no longer has the hands to bank.
        let banking = band_banking(
            &allocation,
            &forage_registry,
            &registry,
            &roads,
            band_id,
            faction,
            &discovery,
            knowledge_threshold,
            &ladder,
            &fauna,
            &labor,
            &flora,
            &food_sites,
            &tile_registry,
            &tiles,
            map_seed,
            wrap_horizontal,
        );
        let upkeep_shares = maintenance_shares(
            &allocation,
            &banking,
            &forage_registry,
            &tile_capacity_of,
            &labor.forage,
            &registry,
            &fauna,
            &ladder,
            &equipment_cfg,
            &band_kit,
        );
        // **⛔ AND THE BILL EACH HERD WAS HANDED, STAMPED AT THIS EXACT MOMENT.**
        //
        // The animal keeping demand **interpolates on the herd's position** since the animal web got
        // its one-position ladder, and the position moves *later this turn* — the band's `builders`
        // pool is spent below, and a `Tame` banking its first work takes the demand from `0` to a
        // real number. Judged a turn later against the risen demand, a fully-staffed keeping reads
        // permanently short.
        //
        // **It cannot be stamped inside the per-assignment arm**, unlike the plant web's: the animal
        // build accrual runs in the build-queue pass *above* that arm, so by the time the arm is
        // reached the position has already moved. Here — between the split and the first accrual —
        // is the one point where the bill and the share describe the same position.
        //
        // **First write wins**, so several bands working one herd all judge the same bill.
        for assignment in &allocation.assignments {
            let LaborTarget::Hunt { fauna_id, .. } = &assignment.target else {
                continue;
            };
            let Some(herd) = registry.herds.iter_mut().find(|herd| &herd.id == fauna_id) else {
                continue;
            };
            if herd.upkeep_demanded.is_some() {
                continue;
            }
            // **Stamped for every worked herd, claiming or not.** The bill is *"what this source owed
            // at the position its share was struck against"*, and a herd that owed nothing owed
            // exactly `0` — recording that is what makes `supplied == demand` on the turn a Tame
            // banks its first work, where the live demand read a turn later is already positive.
            herd.upkeep_demanded = Some(fauna::herd_upkeep_demand(herd, &fauna, &ladder));
            // **AND THE MATERIAL HALF, STAMPED IN THE SAME BREATH.** It has to be here rather than in
            // the arm below: the arm is skipped for a herd out of the hunt leash or gone from the
            // registry, and a `upkeep_demanded` stamped without its material twin would read as *"a
            // band answered and this rung eats nothing"* — an abandoned pen judged short of hands and
            // fully supplied with hurdles. One pass stamps both, so the pair cannot come apart.
            herd.upkeep_materials_demanded =
                fauna::herd_upkeep_material_demands(herd, &fauna, &ladder);
        }
        // **AN ENTRY REQUIRES A ROW** (`docs/plan_standing_upkeep.md` §3.2 of the slice brief): the
        // queue is pruned of anything the band no longer works before a single work unit is aimed,
        // so no seam that drops a row can leave the pool funding ground nobody stands on. A ring
        // whose entry goes here stops with it ([`fauna::cancel_dropped_rings`]).
        let pruned_entries =
            allocation.prune_build_queue(&|tile| band_keeps_road(&roads, band_id, tile));
        fauna::cancel_dropped_rings(&mut registry, &pruned_entries);
        // **THE BAND'S BUILDERS** — one pool, whose whole output goes on the **head** of the queue
        // until that entry's meter fills, then on the next (§2.5). It is not a crew on any tile: a
        // verb declares, and the hands are here.
        let builders = allocation.workers_on(&LaborTarget::Builders);
        // **THE HEAD STAYS THE HEAD EVEN WHEN ITS GATE REFUSES.** It is not skipped, not reordered
        // and not passed over — a stuck head says so loudly (`crate::intensification::BuildTurns::Blocked`) rather than
        // letting the queue quietly fund something the player did not put first.
        let head_entry = allocation.build_queue.first().cloned();
        // The queue as it stands for this turn, read inside the assignment loop (which borrows the
        // allocation's assignments) and walked again by the chain pass after it.
        let build_queue = allocation.build_queue.clone();
        // **THE BUILDERS' OWN GEAR, ONE READING PER FOOD WEB PLUS ONE PER OVERRIDDEN ENTRY.** The
        // question *"which kit"* is the **entry's** — a queue item is one job — so the two derived
        // per-web answers serve every entry that named nothing and an entry that named a kit is
        // resolved on its own (§4.7a ②). See [`BuildersGear`].
        let builders_gear =
            BuildersGear::resolve(&equipment_cfg, &build_queue, builders, &band_kit);
        // **WHAT EACH SOURCE CONTRIBUTED TO THE CHAIN**, recorded as the loop goes and evaluated in
        // **queue order** afterwards — the loop visits assignments, and the queue's order is the
        // player's.
        let mut build_quotes: Vec<(BuildSource, BuildQuote)> = Vec::new();
        // **The band's fodder inflow rate this turn** (Flora Roster F3, §5.3) — the fodder its hay
        // Fields harvest into the `FODDER` store, summed across every Forage assignment. This is the
        // *sustained flow* the pen's `K_pen` term reads (NOT the store's stock, which would spike K
        // off a buffer and oscillate): in steady state inflow = the field output the store holds
        // steady at. Cached onto each pen this band keeps after the assignment loop and read next turn
        // by `advance_herds`' `ecological_carrying_capacity` — the deliberate Logistics-reads-what-
        // Population-wrote one-turn lag, exactly as `footprint_intake` is.
        let mut band_fodder_inflow = 0.0_f32;
        // **The hay this band's pens are short, summed** (in fodder units per turn) — each kept pen's
        // `max(0, demand_grass − footprint_intake)`, accumulated by the corral arm as it stamps the
        // herds. Published as `fodderNeed` against the `fodderIncome` beside it, so the client renders
        // *"need 6.0/turn · growing 5.0/turn"* without summing pen rows of its own: **the sim does the
        // arithmetic**, which is the rule the retired `pen_feed_upkeep` was minted under.
        let mut band_fodder_need = 0.0_f32;
        // **The hay this band's pens will actually DRAW, summed** (in fodder units per turn) — the
        // need above *after* the Foddering gate, so a band that has not learned to hay a herd draws
        // nothing however short its pens are. It is the rate the published fodder runway counts down
        // (`turnsOfFodder`), which is why it is a second accumulator and not the need: the need is
        // the alarm and this is the drain, and only one of them empties the store.
        let mut band_fodder_drain = 0.0_f32;
        // The fauna ids of the pens this band tends this turn — the keepers whose `K_pen` gets the
        // fodder term. Collected in the loop; the rate is stamped on them post-loop (the take arm
        // already borrows the herd mutably, so a second pass keeps the borrows simple).
        let mut kept_pens: Vec<String> = Vec::new();
        // **EVERY PEN'S HAY, SETTLED BEFORE A SINGLE ROW IS VISITED** (`docs/plan_standing_upkeep.md`
        // §4.9 item 9b). The corral arm used to draw hay and then bread off the band's stores *inside*
        // the loop below, so a store that could not cover every pen fed the earliest row in
        // `assignments` and starved the last — and since `set_assignment` re-pushes an edited row to
        // the end, the pen the player had just adjusted was the one fed last. One pass that sees every
        // pen at once has no vector position left to spend: see [`settle_pen_hay`].
        //
        // **The hay is the whole of it.** `FODDER` is a stock, so the store standing at the top of
        // the pass is the right one to split, and there is no second settlement behind it: a pen the
        // land and the hay cannot fill is underfed, and the keeper's `FOOD` larder — which is what
        // the *people* eat — is never asked.
        // **THE MATERIAL HALF OF THE STANDING UPKEEP AND OF THE BUILD PILE, SETTLED ONCE**
        // (`docs/plan_standing_upkeep.md` §2.7). Both accounts bid on one store in one call per
        // material, through [`settle_scarce_store`] — so a short store splits by the player's own
        // `SourcePriority` and then in proportion to demand, and no row's place in `assignments`
        // decides anything (`set_assignment` re-pushes an edited row to the end).
        //
        // **The build's want is struck against the head entry alone**, and only where its gate holds
        // — `banking` is that gate, resolved above and shared with the keeping claim, so the pool
        // cannot draw a pile for work it will not bank.
        //
        // **AND THE HEAD MAY BE A RING**, which is why the two arms below meet in one pair. A ring is
        // a pen build — the same rung record prices it, funds it and wears the same tools on it — so
        // it draws the same pile, laid as [`head_ring_leg`] and spread by the very same
        // [`build_material_wants`]. `banking` cannot answer for it (it resolves a *verb*, and a ring
        // names none), so the ring's own gate is asked here instead: the head entry declares
        // `ExtendPen`, the band has builders, and `Herd::pen_extending` is set.
        let (pile_source, pile_legs) = match banking.source.as_ref() {
            Some((source, improvement)) => (
                Some(source.clone()),
                head_build_legs(
                    source,
                    BuildJob::Rung(*improvement).destination(),
                    &forage_registry,
                    &registry,
                    &roads,
                    &ladder,
                ),
            ),
            None => match head_entry.as_ref().filter(|entry| {
                matches!(entry.declared, BuildJob::ExtendPen) && builders > NO_CREW_ON_THIS_ACTIVITY
            }) {
                Some(entry) => (
                    Some(entry.source.clone()),
                    head_ring_leg(&entry.source, &registry, &ladder)
                        .into_iter()
                        .collect(),
                ),
                None => (None, Vec::new()),
            },
        };
        let build_want = match pile_source.as_ref() {
            Some(source) => {
                // The turn's accrual as the arm will compute it — the whole pool at the entry's own
                // kit, which is `RungDef::build_accrual`'s body once its gate has held, and the
                // ring arm's `pen_extend_accrual` on the same terms.
                // **The rung the pool is standing on this turn is the FIRST LEG** — the legs
                // are in climb order and each carries what it still owes, so the head of the list
                // is the one this turn's work lands in. It is the same rung the arm below charges
                // the wear against.
                let in_flight = pile_legs.first().map(|(rung, _, _)| *rung);
                let accrual = crate::intensification::pool_work_supply(
                    builders,
                    builders_gear.for_source(source, in_flight).work_per_worker,
                );
                BuildMaterialDraw {
                    coverage: FULLY_SERVED,
                    wanted: build_material_wants(&pile_legs, accrual, &ladder),
                }
            }
            None => BuildMaterialDraw::unbilled(),
        };
        // The head row's own rank, so the build competes for the store on the player's answer for
        // that source rather than on a rank of its own — a ring's included, so a widening pen queues
        // for the hurdles behind a `High`-marked holding exactly as a fresh pen does.
        let build_priority = pile_source
            .as_ref()
            .and_then(|source| {
                allocation
                    .assignments
                    .iter()
                    .find(|assignment| BuildSource::of(&assignment.target).as_ref() == Some(source))
                    .map(|assignment| assignment.priority)
            })
            .unwrap_or_default();
        let material_settlement = settle_material_upkeep(
            &allocation.assignments,
            &forage_registry,
            &registry,
            tile_capacity_of,
            &labor.forage,
            &fauna,
            &ladder,
            &cohort.stores,
            build_want,
            build_priority,
            band_reach,
        );
        // **AND THE BUILD'S SHARE IS SPENT HERE, at the coverage the store could pay.** The materials
        // go in as the meter climbs — `coverage` scales the accrual in every build arm below, so the
        // work banked and the pile drawn are one fraction of the same turn. **Decay refunds nothing**:
        // nothing credits this back when a position falls.
        for (id, wanted) in &material_settlement.build.wanted {
            let drawn = wanted * material_settlement.build.coverage;
            if drawn > NOTHING_DEMANDED {
                cohort
                    .stores
                    .take_material_batches(id, crate::scalar::scalar_from_f32(drawn));
            }
        }
        let build_coverage = material_settlement.build.coverage;
        // **THE KIT'S LIFE AS THE TURN FOUND IT** — the before half of the crossing
        // [`announce_kit_life`] reads at the foot of this band's turn. Taken here, ahead of every
        // wear charge, so the pair is one turn's transition and not a level test.
        let kit_life_before = kit_life_fractions(&equipment_cfg, band_equipment.as_deref());
        let pen_feed = settle_pen_hay(
            &allocation.assignments,
            &registry,
            &cohort.stores,
            knows(
                &discovery,
                faction,
                FODDERING_DISCOVERY_ID,
                knowledge_threshold,
            ),
            band_reach,
        );
        for (idx, assignment) in allocation.assignments.iter().enumerate() {
            let workers = assignment.workers;
            // **A ROW WITH NO TAKE CREW IS STILL VISITED, because the row is the band's HOLDING**
            // (`docs/plan_standing_upkeep.md` §2.2/§2.5). The take crew is one of three allocations
            // on a source, so skipping the row on `workers == 0` withheld the *keeping* from every
            // improvement whose gatherers had moved on: the pool's share was never stamped and the
            // meter bled its full rate with keepers idle in the role. Everything below resolves to
            // nothing on its own for an unstaffed take — the takes are `crew × rate`, the wear
            // quanta are the biomass taken — so the arms need no zero-crew special case; what they
            // do need is to **retire a row with nothing left on it**, which each does once its
            // source is in hand.
            //
            // **The one thing that does NOT fall out of the arithmetic is the LESSON**, which is
            // credited per assignment rather than per worker. A crew that is not there is not
            // practising, so it rides this predicate at each of the four earn sites.
            let take_crew_present = workers > NO_CREW_ON_THIS_ACTIVITY;
            // **THIS SOURCE'S PLACE IN THE BAND'S QUEUE, and what it declared there.** The queue
            // **is** the declaration now (`docs/plan_standing_upkeep.md` §2.4) — there is no second
            // authority on the row for it to drift from.
            let build_source = BuildSource::of(&assignment.target);
            let queued = build_source
                .as_ref()
                .and_then(|source| build_queue.iter().find(|entry| &entry.source == source));
            // **The player's DECLARATION, which answers only for a meter at zero.** What is
            // actually being built is **derived from the source's own meters**
            // (`forage::patch_build_verb` / `fauna::herd_build_verb`), because a meter carrying
            // progress *is* the declaration — so an eroded rung is repairable without the player
            // re-issuing a verb they never withdrew. A ring (`BuildJob::ExtendPen`) names no rung
            // and therefore declares nothing here; it is the tend branch's own kind.
            let declared = match queued.map(|entry| entry.declared) {
                Some(BuildJob::Rung(improvement)) => Some(improvement),
                Some(BuildJob::ExtendPen) | None => None,
            };
            // **IS THIS SOURCE THE HEAD?** — the one test that decides where the pool lands. Only
            // the head receives work; everything below it is dated, not funded.
            let is_queue_head = match (&build_source, &head_entry) {
                (Some(source), Some(head)) => &head.source == source,
                _ => false,
            };
            // **ALL HANDS ON THE HEAD** (§2.5). A waiting entry gets `0` and its meter does not
            // move — its rot is the keeping pool's business, not the builders'.
            let build_workers = if is_queue_head && declared.is_some() {
                builders
            } else {
                NO_CREW_ON_THIS_ACTIVITY
            };
            // **THE SHARE OF ITS MATERIAL PILE THIS ENTRY WAS SETTLED**
            // (`docs/plan_standing_upkeep.md` §2.7), which scales its accrual **and its countdown**:
            // a forecast and a take must not disagree, and an unscaled countdown published *"≈20
            // turns"* for a build banking a quarter of its turn.
            //
            // ⛔ **ONLY THE HEAD HAS ONE.** The settlement struck its want against `banking.source`,
            // which is the head and only the head; a waiting entry has bid on nothing and its store
            // draw is not decided until it is funded, so it is quoted at [`FULLY_SERVED`] — the same
            // convention that quotes every waiting entry at the **full pool**.
            let entry_material_coverage = if is_queue_head {
                build_coverage
            } else {
                FULLY_SERVED
            };
            // **A RUNNING QUOTE DESCRIBES A QUEUE ENTRY** — the ring's rule (`if ring_queued`
            // below), stated for the four rung arms too, and the fix for a patch that published
            // `≈1 turn` for ever.
            //
            // The four arms are entered on the **derived** verb, which answers for any meter
            // carrying progress — so a tended patch eroded below its cost derives `Cultivate` with
            // **no entry and therefore no builders** (`build_workers` above is `0`), banked nothing,
            // and still pushed a quote. `publish_build_chain`'s unqueued tail then dated it at the
            // **full pool** and published a confident countdown for a build nobody was working: at
            // 99% of its cost that reads `≈1 turn`, every turn, for ever.
            //
            // **The honest answer for an unqueued running meter is NO ESTIMATE** — no quote is
            // pushed, the tail skips the source, and `advance_cultivation` / `advance_husbandry`
            // leave the field at the `None` they cleared it to. The *projection* is untouched and is
            // still dated at the back of the line: it fires only where nothing is being built
            // (`improvement.is_none()`), which is exactly the compose-sheet question, and re-queueing
            // the eroded meter restores a real countdown by restoring the entry.
            let entry_declares_a_rung = declared.is_some();
            // **WHERE THE PLAYER SAID THIS LAND SHOULD END UP** — the entry's destination
            // ([`BuildJob::destination`]), which is what the entry retires at and what its legs are
            // laid toward. A ring names the pen rung it widens; it climbs nothing, so it lays no legs.
            let entry_destination = queued.map(|entry| entry.declared.destination());
            // **THE ENTRY'S OWN JOB, for the RETIREMENT line.** A rung completing mid-climb is
            // announced on its own verb's channel (the arms below); the *queue* line names what the
            // player ordered, which is the destination's verb.
            let entry_job = queued.map(|entry| entry.declared);
            // **HAS THIS BAND DECLARED A RING HERE?** — the ring's own membership test, kept apart
            // from the head test below because a *waiting* ring must still be **quoted** (every
            // entry is dated at the full pool, §4.6b) even though it is funded at nothing.
            let ring_queued = matches!(
                queued.map(|entry| entry.declared),
                Some(BuildJob::ExtendPen)
            );
            // The **ring's** own funding, which the head declares as its own queue kind rather than
            // through a rung verb: a built pen carries no meter for a verb to name.
            let ring_workers = if is_queue_head && ring_queued {
                builders
            } else {
                NO_CREW_ON_THIS_ACTIVITY
            };
            // **THIS SOURCE'S SHARE OF THE BAND'S MAINTENANCE POOL**, in work units — no longer a
            // crew standing on the tile (`docs/plan_standing_upkeep.md` §2.5). It is already work
            // rather than workers, because a share of a pool does not divide into whole people and
            // that indivisibility is exactly the waste the pool retired.
            let keeping_share = upkeep_shares
                .get(idx)
                .map_or(NO_UPKEEP_DEMAND, |award| award.work);
            // **AND THE KIT THAT SHARE WAS WORKED WITH** — this site's own, resolved with the share
            // it pays for (`docs/plan_standing_upkeep.md` §2.7). It travels beside the work rather
            // than being re-derived here, so the hours charged and the tool charged for them can
            // never describe two different sites.
            let keeping_wear_kit = upkeep_shares
                .get(idx)
                .and_then(|award| award.wear_kit.as_ref());
            // **THE KIT THIS CREW WAS SENT OUT WITH** (`equipment.json`'s roster) — the mask that
            // decides which of the three components serve it at all, re-resolved from the
            // *assignment* every turn and never from what the band happens to hold. `None` = the
            // crew named no kit, which is its job's default: the two shipped working kits reach for
            // exactly the components this loop used to consult unconditionally, so a crew that named
            // nothing is priced bit-for-bit as it was before the roster existed.
            //
            // **The WEAR half is still resolved once per band** (`band_kit` above), so a kit that
            // expires part-way through the loop cannot pay two different rates to two herds in the
            // same turn; only the *mask* varies per assignment.
            let crew_kit = assignment.kit_choice(&equipment_cfg);
            // **HOW THIS CREW'S GEAR DIVIDES ITS PEOPLE** (`equipment.md` → "the partly-equipped
            // party"). A band owning five spears and staffing ten hunters sends five out armed and
            // five bare-handed, so every tier below that a *count* can bind is read through this
            // one coverage rather than off the crew's kit alone.
            let crew_coverage = equipment_cfg.coverage(&crew_kit, workers as f32, &band_kit);
            // This crew's HUNT haul tier — the **sled**, if its kit carries one and the band still
            // has condition in it — **averaged over the crews**, because a party short of sleds
            // drags home what its people are actually dragging.
            let hunt_per_worker_biomass = crew_coverage.weighted_rate(|kit| {
                equipment_cfg.hunt_per_worker_biomass_capacity(baseline_haul_rate, kit, &band_kit)
            });
            // **And its GATHER tier** — the forage web's carry, which before §4.8's "one kit, one
            // job" correction had no kit at all. It is the undipped, pre-seasonal per-gatherer
            // throughput every `forage_take`, gather forecast and staffing inversion below is capped
            // by; a hunt item can never reach it, because the two declare different stats.
            //
            // **And it is covered like the other two.** A band with five baskets and sixteen
            // gatherers gathers with five baskets: the model is *gear covers people*, not *gear
            // covers jobs*, so the plant web reads the same seam the animal web does.
            let forage_per_worker_capacity = crew_coverage.weighted_rate(|kit| {
                equipment_cfg.forage_per_worker_biomass_capacity(
                    baseline_gather_rate,
                    kit,
                    &band_kit,
                )
            });
            // **THE BUILD TIER IS NOT ON THIS ROW ANY MORE** (`docs/plan_standing_upkeep.md` §2.5).
            // A build's gear rate used to be resolved here, over the *source row's* kit and
            // averaged over the build crew standing on the tile — so a `Corral` was priced off
            // whatever the hunt row was carrying. The builders are their own role now, so the rate
            // is read off **their** row and their coverage, once per band
            // (`BuildersBranchGear::work_per_worker` above), exactly as every other role's tier is
            // read off its own row.
            // **And its FIGHTING tier** (`docs/plan_hunt_through_combat.md` §4). The kit swaps the
            // whole `attack` tier (`1` bare-handed, `20` speared), which is the gate every take
            // resolves through — so a crew sent out with no spears stops being able to hurt anything
            // with a `defense`, and the seed it was assigned on says so on the same tier.
            //
            // **Two more kit-resolved terms ride beside it**, both neutral at `1.0` so a kit that
            // declares neither is priced bit-for-bit as it was before the effects model:
            // `dispersion` multiplies the quarry's own `wariness` at the retreat (a device that is
            // not there scares nothing), and `exposure` multiplies the hunt's baseline injury hazard
            // (a stand-off instrument wears out instead of its user getting hurt).
            //
            // **A FACTORY, not a value, because the ATTACK TIER DEPENDS ON THE QUARRY.** A
            // mass-bounded weapon (a snare) is only a weapon against animals it can hold, so the
            // profile cannot be resolved before the assignment's target is known. Everything else
            // about the party is quarry-blind and is captured once.
            let party_resolution = fauna::PartyResolution {
                equipment: &equipment_cfg,
                coverage: &crew_coverage,
                wear: &band_kit,
                intrinsic: person_profile,
                tuning: combat_tuning,
                hunt_injury_damage_per_animal: hunt_injury_damage,
            };
            let party_for = |body_mass: f32| {
                party_resolution.party_against(crate::equipment_config::Quarry::Mass(body_mass))
            };

            match &assignment.target {
                LaborTarget::Forage {
                    tile,
                    floor,
                    species,
                    take_species,
                } => {
                    // **Out of range → the assignment is ABANDONED**, the plant twin of the hunt
                    // leash lapse. A patch cannot move, so beyond `band_work_range` the band walked
                    // away from it — a decision, not a drift, and there is nothing to follow. Keeping
                    // the assignment would pay a correct `+0.00` forever while the tile still renders
                    // as worked and its workers stay booked, so the workers return to the pool and the
                    // player is told which tile was given up.
                    let distance = crate::grid_utils::hex_distance_wrapped(
                        band_pos,
                        *tile,
                        grid_width,
                        wrap_horizontal,
                    );
                    if distance > work_range {
                        lapsed.push(idx);
                        event_log.push(CommandEventEntry::new(
                            tick.0,
                            CommandEventKind::Forage,
                            faction,
                            format!(
                                "foragers abandoned ({}, {}) — out of the band's work range",
                                tile.x, tile.y
                            ),
                            Some(band_detail_token(
                                format!(
                                    "status=lapsed reason=out_of_range x={} y={} distance={} range={}",
                                    tile.x, tile.y, distance, work_range
                                ),
                                band_id,
                            )),
                        ));
                        continue;
                    }
                    // **A HOLDING ROW LASTS EXACTLY AS LONG AS THERE IS SOMETHING TO HOLD.** With no
                    // hands on any of the three activities the row says only *"this band's ground"*,
                    // and the ground answers whether that is still true: a meter carrying progress
                    // is what the keeping pool funds and what the decay pass bleeds
                    // (`forage::patch_unwinding_rung`). Once it is empty — the patch went feral, or
                    // the player unstaffed a wild stand they were only gathering — the band has
                    // nothing here and the row goes, which is what stops rows accumulating for the
                    // life of a game. Silently: the player emptied it themselves, and the reversion
                    // it may have followed announces itself.
                    //
                    // **A QUEUE ENTRY IS A HOLDING TOO.** A `Sow` declared on bare ground has no
                    // meter yet and may have no gatherers either — that is the create-from-nothing
                    // case the rung exists for — so the row survives on the declaration alone until
                    // the player withdraws it (`unqueue`) or puts the source down (`abandon`).
                    if !take_crew_present
                        && queued.is_none()
                        && !source_has_a_meter_at_risk(
                            &assignment.target,
                            &forage_registry,
                            &registry,
                            &ladder,
                        )
                    {
                        lapsed.push(idx);
                        continue;
                    }
                    let Some(tile_entity) = tile_registry.index(tile.x, tile.y) else {
                        continue;
                    };
                    // **THE GEAR THIS SOURCE'S OWN ENTRY IS RAISED WITH** — the kit it named, else
                    // the plant web's derived answer. Resolved once for the arm, because the
                    // accrual, the balance, the projection and the wear charge must all be struck at
                    // one number or the countdown and the meter disagree.
                    let patch_source = BuildSource::Patch(*tile);
                    let entry_gear = builders_gear.for_source(
                        &patch_source,
                        queued_destination(&build_queue, &patch_source),
                    );
                    // The **gather** season is the food module's. A tile with no module offers no
                    // wild gather at all (`NO_FORAGE_SEASON` → zero per-worker throughput), which is
                    // exactly right — and, since slice 5, a real state rather than an impossible one:
                    // `Sow` places a Field on ground the `plant:field` rung's `site_requirement`
                    // accepts (a watered gathering site), module or not, and a Field's harvest is
                    // biomass-based and seasonless.
                    let seasonal = food_modules
                        .get(tile_entity)
                        .map_or(NO_FORAGE_SEASON, |module| module.seasonal_weight.max(0.0));
                    // **May this faction sow THIS ground?** — the `plant:field` rung's two gates,
                    // both resolved off the rung record, both read here because each gates the *same*
                    // two things below: the seed going into the ground at all, and the build meter it
                    // then fills.
                    //  - **the knowledge**: does the faction know Seed Selection?
                    //  - **the SITE** (`site_requirement`): is this a gathering site, and is it near
                    //    fresh water? Rung 3 knows how to move seed, not how to carry water or
                    //    fertilize — so it can only place a Field on ground the people already work,
                    //    where the land waters itself. That is the scarcity the rung is *made of*, and
                    //    the ground the `sow` command refuses up front with the reason (not a
                    //    gathering site / too dry).
                    //
                    // **Hoisted into a closure because the PROJECTION asks the same question of a
                    // different rung** — "what would rung N+1 take this crew?" is only honest if it
                    // is judged by the gate that would actually run, so the running `Sow` and the
                    // quoted next rung resolve one expression rather than two copies of it.
                    let land_admits = |rung: &RungDef| {
                        tiles.get(tile_entity).is_ok_and(|ground| {
                            let fresh_water = tile_is_fresh_watered(
                                ground,
                                grid_width,
                                grid_height,
                                wrap_horizontal,
                                |coord| {
                                    tile_registry
                                        .index(coord.x, coord.y)
                                        .and_then(|entity| tiles.get(entity).ok())
                                        .map(|neighbor| neighbor.terrain_tags)
                                },
                            );
                            rung_site_refusal(
                                rung,
                                ground,
                                &labor.forage,
                                food_sites.is_site(ground.position),
                                fresh_water,
                            )
                            .is_none()
                        })
                    };
                    // Stated as terms rather than a `&&` chain, so the refusing conjunct reaches
                    // the wire as a blocked head's cause — see the Cultivate arm. The **crop** term
                    // joins them below, once the selection has been resolved.
                    let sow_knows_rung = field_rung.unlock_discovery_id().is_none_or(|knowledge| {
                        knows(&discovery, faction, knowledge, knowledge_threshold)
                    });
                    let sow_land_admits = land_admits(field_rung);
                    // **WHICH NAMED PLANT this ground would be committed to** (Flora Roster S1,
                    // `docs/plan_flora_roster.md` §4.3). Resolved through the *same*
                    // `resolve_committed_species` seam the `assign_labor` rejection reads, so a
                    // selection the command accepted can never be one the turn then refuses — and
                    // through `tile_flora_composition`, never `FloraConfig::composition` on a raw
                    // terrain, so a navigable hex is judged on the basket it actually has.
                    //
                    // `None` means **there is nothing here this rung can commit to**: either the
                    // player's pick is illegal, or the whole basket's `cultivation_ceiling` stops
                    // below this rung (an open-water fishery, an alpine peak). Either way the
                    // investment simply does not accrue — you cannot farm what will not climb.
                    let committing =
                        matches!(declared, Some(Improvement::Cultivate | Improvement::Sow))
                            .then(|| {
                                let rung = if declared == Some(Improvement::Sow) {
                                    RungKey::PlantField
                                } else {
                                    RungKey::PlantTended
                                };
                                tiles.get(tile_entity).ok().and_then(|ground| {
                                    // §10 scoping: Cultivate and a Sow that **upgrades** an existing
                                    // patch commit against the tile's **realized** basket (what is
                                    // growing here); a Sow that **creates** a patch on bare ground has no
                                    // realized basket, so it reads the **affinity** roster (what CAN grow
                                    // here). The create case does not occur on a generated map — every
                                    // food-bearing tile already carries a patch — but the branch keeps the
                                    // "you sow what grows here; unwilling ground is rung 4" rule honest.
                                    let sow_from_nothing = declared == Some(Improvement::Sow)
                                        && forage_registry.patch(*tile).is_none();
                                    if sow_from_nothing {
                                        resolve_committed_species(
                                            species.as_deref(),
                                            flora.composition(ground.resource_terrain()),
                                            &flora,
                                            rung,
                                        )
                                        .ok()
                                    } else {
                                        let composition = tile_flora_composition(
                                            &flora,
                                            &labor.forage,
                                            ground,
                                            map_seed,
                                        );
                                        resolve_committed_species(
                                            species.as_deref(),
                                            &composition,
                                            &flora,
                                            rung,
                                        )
                                        .ok()
                                    }
                                })
                            })
                            .flatten();
                    // A Field may only be placed on ground that grows something sowable — the
                    // species half of "the land must take seed", beside the site half above. It joins
                    // the gate rather than the bool so a blocked Sow can name it.
                    let sow_gate = plant_field_gate(
                        declared == Some(Improvement::Sow),
                        sow_knows_rung,
                        sow_land_admits,
                        committing.is_some(),
                    );
                    let sow_permitted = sow_gate.holds();
                    // **`Sow` PLACES the source** — the one rung that needs no *patch* below it,
                    // unlike a herd you never tamed. (§2 used to read "no source below it: seed
                    // travels", meaning any qualifying tile; the gathering-site rule above reversed
                    // that and handed "seed travels" to rung 4. What survives is narrower: a
                    // gathering site the wild seeded no patch on is still a legal target.) The first
                    // turn a crew works sowable ground, the seed goes in and the patch exists — at the
                    // tile's **own** biome capacity (`tile_forage_capacity`, the same source a wild
                    // patch is seeded from — there is no Field-specific table) and at the reseed
                    // floor's standing crop.
                    if sow_permitted && forage_registry.patch(*tile).is_none() {
                        if let Ok(sown_tile) = tiles.get(tile_entity) {
                            let mut patch = ForagePatch::sown(
                                *tile,
                                tile_forage_capacity(&labor.forage, sown_tile),
                                labor.forage.reseed_floor_fraction,
                            );
                            patch.refresh_ecology_phase(&labor.forage.ecology);
                            forage_registry.patches.insert(*tile, patch);
                        }
                    }
                    // **What is actually growing on this tile** — the realized basket, resolved once
                    // per assignment through the one `tile_flora_composition` seam (never
                    // `FloraConfig::composition` on a raw terrain). Every rate this arm pays is the
                    // share-weighted average of the *patch's* basket, which `forage.rs` derives from
                    // this one (#433) — so it is resolved *before* the registry is borrowed mutably.
                    // A tile that is not on the map names no plants: the rates then fall back to the
                    // empty-basket defaults, which is the honest reading of ground nobody can see.
                    let tile_composition = tiles.get(tile_entity).map_or_else(
                        |_| Cow::Owned(Vec::new()),
                        |ground| tile_flora_composition(&flora, &labor.forage, ground, map_seed),
                    );
                    // **THE SIZE OF THE LAND** — the tile's own `K`, resolved here beside the basket
                    // and off the same tile, because the standing upkeep is quoted per **tender-load**
                    // of it (`forage::patch_tender_loads`). Resolved as an `Option` here and folded
                    // into the patch's own land reading below (`forage::patch_land_capacity`), so a
                    // coord that is **not on the map** bills off the capacity the patch was seeded
                    // with at the stamp exactly as it does at the claim and in the decay pass.
                    let plant_tile_ground = tiles
                        .get(tile_entity)
                        .ok()
                        .map(|ground| tile_forage_capacity(&labor.forage, ground));
                    // Depletable patch (Intensification §0-ii): draw the biomass down via the shared
                    // `forage_take` primitive (mirrors the Hunt arm). Every `FoodModuleTag` tile is
                    // seeded a patch at Startup; a missing one (a dynamically-tagged tile, or ground
                    // nobody has sown) is skipped this turn. Gather per the assignment's policy
                    // (§0-iii, parity with hunting).
                    let Some(patch) = forage_registry.patch_mut(*tile) else {
                        continue;
                    };
                    let plant_tile_capacity =
                        crate::forage::patch_land_capacity(patch, plant_tile_ground);
                    // **THE LIVE VERB, DERIVED** — the declaration above counts only where the meter
                    // it names is at zero; otherwise the newest meter with progress on it decides.
                    let improvement = crate::forage::patch_build_verb(patch, declared);
                    // **The commitment, recorded once and fixed until the patch goes feral.** This is
                    // the first turn a crew works this ground under Cultivate/Sow, so this is where
                    // the tile stops being a mixed basket and becomes one named crop. It takes effect
                    // (weeding + conversion) when the improvement *completes* — while the crew
                    // is still clearing, the stand is still the basket it started as.
                    if let Some(chosen) = committing.as_deref() {
                        // **AND THE COMMITMENT REPAIRS THE CREW'S TAKE SELECTION**, on the turn it
                        // is made and only that turn (`ForagePatch::commit_species` reports the
                        // edge). The ground is now becoming one crop, so a selection naming the
                        // plants it displaces is a selection of nothing — see
                        // [`TakeSelection::pruned_for_commitment`], which prunes the stale names,
                        // adds the crop, and leaves whatever still stands (a fishery the hoe never
                        // reaches) exactly as the player set it.
                        if patch.commit_species(chosen) {
                            let mix = crate::forage::patch_composition(
                                patch,
                                &tile_composition,
                                &flora,
                                &labor.forage,
                            );
                            let repaired = take_species.pruned_for_commitment(
                                |species| crate::forage::species_stands_in(&mix, species),
                                chosen,
                            );
                            if &repaired != take_species {
                                repaired_takes.push((idx, repaired));
                            }
                        }
                    }
                    // **NOTHING LEFT TO BUILD needs no test any more.** A declaration is honoured
                    // only where the meter it names is at zero (`forage::patch_build_verb`), so a
                    // stale verb on a finished rung — including a second band's, set by the command
                    // that fans a verb across every band working the source — derives to `None` and
                    // drives nothing. The clear that used to be needed to stop it is gone with the
                    // authority it was cleaning up after.
                    // **THE STANDING UPKEEP, PAID BY THE BAND'S KEEPING POOL**
                    // (`docs/plan_standing_upkeep.md` §2.4). What is left over is the shortfall, and
                    // the shortfall **is** the decay (`RungDef::upkeep_decay`, past the grace).
                    //
                    // **The demand belongs to the rung AT RISK** (`forage::patch_unwinding_rung`) —
                    // the newest meter with progress on it, which is the very meter
                    // `advance_cultivation` would bleed. That is one fact rather than two: what a
                    // patch costs to hold is what it costs to hold the thing it would otherwise
                    // lose. A patch standing on `plant:tended` with a half-built Sow therefore owes
                    // the **field** rung's demand.
                    //
                    // **THE POOL ANSWERS FOR IT AT ANY FULLNESS** (§4.6a): a meter still being
                    // raised is billed exactly as a finished one is, and the builders beside it
                    // supply nothing toward the rate. The retired fullness test is what made a
                    // half-built patch unholdable by idle keepers.
                    //
                    // **Stamped once per worked source, before the arm branches by rung**, so a
                    // Field's early return cannot skip it — and stamped rather than re-derived at
                    // capture, because it describes *this* turn's crew and the capture does not
                    // hold them (the `build_turns_remaining` discipline).
                    //
                    // **The supply is the only thing stored**, because it is the only thing a
                    // crew authors; the demand and the shortfall are derived wherever they are
                    // wanted (`forage::patch_upkeep_shortfall`), so they cannot drift from it or
                    // from each other.
                    //
                    // **IT ACCUMULATES ACROSS THE BANDS WORKING THE SOURCE**, and that is a `+=` the
                    // requirement's own shape demands: the upkeep is per-SOURCE, so two bands each
                    // put a fraction of it on the ground. Assigning would let whichever band the
                    // loop happened to visit last speak for all of them — a crew *gathering* a patch
                    // a second crew is sowing would overwrite the sowers' supply with its own zero
                    // and revert the very meter they were filling. `advance_cultivation` zeroes it at
                    // the top of every turn, so the sum is always this turn's.
                    let keeping_supplied =
                        crate::forage::patch_upkeep_supply(patch, improvement, keeping_share);
                    patch.upkeep_supplied += keeping_supplied;
                    // **AND THE BILL IT ANSWERS**, recorded because the plant demand INTERPOLATES on
                    // the source's position and this stamp is read a whole turn later, after the
                    // build has banked more work. Judged against the risen demand, a fully-staffed
                    // keeping reads permanently short — see `ForagePatch::upkeep_demanded`.
                    //
                    // # ⛔ THE FIRST BAND TO REACH THE SOURCE WRITES IT, AND NOBODY OVERWRITES IT
                    //
                    // It is **not** *assigned* per band. The demand is per-source, but the position
                    // it interpolates on **moves between band visits**: the build accrual below runs
                    // inside each band's own arm, so a later band would stamp a bill struck after an
                    // earlier band's builders had already banked their turn — while every band's
                    // `keeping_share` was split from the pool *before* the loop, against the
                    // position as it stood then. Two bands on one patch — one keeping it, one
                    // building it — would then judge a correctly-staffed keeping against a bill
                    // nobody was handed, re-arming `neglect_turns` every turn.
                    //
                    // The shares are all struck at the pre-accrual position, so the bill has to be
                    // too: the first visit is the one that still sees it, and this arm runs before
                    // its own band's accrual. `advance_cultivation` clears it at the top of every
                    // turn, so "already stamped" always means *this* turn.
                    if patch.upkeep_demanded.is_none() {
                        patch.upkeep_demanded = Some(crate::forage::patch_upkeep_demand(
                            patch,
                            &ladder,
                            plant_tile_capacity,
                            &labor.forage,
                        ));
                    }
                    // **AND THE MATERIAL HALF OF THE SAME BILL, on the same two rules** — the demand
                    // stamped first-write-wins (it interpolates on the same moving position) and the
                    // store's payment accumulated across the bands answering for this source. The
                    // amounts stay **separate** from the work beside them: a full store must not be
                    // able to paper over missing hands (§4.9 item 12), so the decay pass takes the
                    // *worst* of the two shortfall fractions rather than a summed one.
                    apply_material_keeping(
                        &mut cohort.stores,
                        &material_settlement.upkeep[idx],
                        &mut patch.upkeep_materials_demanded,
                        &mut patch.upkeep_materials_supplied,
                    );
                    // **AND THE KEEPER'S TOOLS ARE SPENT ON EXACTLY THAT WORK** — the
                    // `WearQuantum::UpkeepWork` charge, billed on what the pool **supplied** to this
                    // patch and not on what the rung demanded, so an under-staffed pool wears only
                    // the hours it worked and a pool with nothing at risk wears nothing.
                    charge_keeping_wear(
                        band_equipment.as_deref_mut(),
                        &equipment_cfg,
                        keeping_wear_kit,
                        keeping_supplied,
                    );
                    // **WHAT THE GROUND WILL LOSE UNDER THE BUILDERS** — exactly what the next
                    // `advance_cultivation` will bleed off the at-risk meter, resolved once here off
                    // the supply just stamped. That pass judges *this* supply, so the bleed is
                    // already determined and the forecast is exact (`RungDef::meter_rot`). It is the **countdown's denominator** (a build crew
                    // supplies nothing toward the rate, so what eats a build is the rot) and the
                    // wire's `meterRotPerTurn`, and it is one number precisely so the card, the
                    // compose sheet and the decay pass cannot disagree. Constant with respect to the
                    // build crew, which is what lets the sheet re-price a *proposed* crew.
                    //
                    // **Not stored**: `upkeep_supplied` and `neglect_turns` are, so the capture
                    // re-derives the same number through the same seam — and an *unworked* patch,
                    // which this loop never visits, is then honest rather than reading a stale `0`.
                    let meter_rot = crate::forage::patch_meter_rot(
                        patch,
                        &ladder,
                        plant_tile_capacity,
                        &labor.forage,
                    );
                    // **WHAT A SOW COSTS ON THIS GROUND** — the `plant:field` rung's own price
                    // multiplier, by how much of the tile the chosen crop has still to **replace**
                    // (`forage::patch_field_cost_multiplier`, `docs/plan_standing_upkeep.md` §4.15).
                    // The stamp once the Field leg has started, the live measure before it, so the
                    // arm that charges the job and every surface that quotes it read one number.
                    //
                    // **Resolved PRE-ACCRUAL, beside the rot and for its reason**: the quote a leg is
                    // struck at is a fact about the ground as the turn found it, and this turn's own
                    // work must not be able to move the price it is charged.
                    let field_cost_multiplier = crate::forage::patch_field_cost_multiplier(
                        patch,
                        &tile_composition,
                        &flora,
                        &labor.forage,
                        &ladder,
                    );
                    // **THE earn path (§4): practising rung N teaches the knowledge that unlocks rung
                    // N+1.** Driven entirely by the rung the patch *currently stands on* — a wild
                    // patch teaches **Cultivation**, a tended one **Seed Selection** — so the lesson
                    // is a property of the source's rung, not of the verb. The old hard-coded
                    // `Sustain && Thriving → CULTIVATION_DISCOVERY_ID` branch is gone: `earns_knowledge`
                    // was declarative when slice 2 landed it, and this is where it goes live.
                    //
                    // Knowledge is all that is earned here — working a patch never *tames* it:
                    // cultivation is an explicit `Cultivate` improvement with an investment cost
                    // (below). The rung is resolved *here*, above the branches, because it is a
                    // property of the pre-take patch; the **credit** is applied inside each branch,
                    // once its take is known — see `credit_rung_lesson`.
                    let lesson_rung = patch_rung(patch, &ladder);
                    // **The steady headline** — the forward-projected average food/turn over the next
                    // `realized_horizon` turns, computed from the patch's PRE-take state (before either
                    // branch draws it down), so it equals the assign-time seed exactly. Both the Field
                    // and the drawn-down branches record this one value.
                    let forage_realized = crate::forage::project_realized_forage(
                        patch,
                        &tile_composition,
                        &labor.forage,
                        &flora,
                        forage_per_worker_capacity,
                        seasonal,
                        mult_f,
                        workers,
                        *floor,
                        take_species,
                        realized_horizon,
                    );
                    // **RETIRED: the rung-3 MANAGED HARVEST BRANCH.** A Field used to be paid a
                    // flat rate on its whole standing crop and never drawn down — no escapement
                    // floor, no overdraw, `sustainable == actual` by construction.
                    //
                    // **A FIELD CHANGED HOW YOU HARVEST, WHEN ITS JOB IS TO CHANGE HOW MUCH THE TILE
                    // GROWS.** So the harvest floor — the one pressure lever the player holds — did
                    // nothing at all on the rung the whole ladder climbs toward, and the rung's
                    // payout could not interpolate because it was a different *kind* of harvest from
                    // the rung below it.
                    //
                    // **Production and draw are separate concerns. A rung may change production; no
                    // rung changes the draw.** A Field now falls through to the ordinary
                    // `forage_take` below exactly as a tended patch and a wild stand do — floor-live,
                    // worker-capped, **drawn down** — so `sustainable != actual` is reachable at rung
                    // 3 and the ⚠ fires on it: strip a field every turn and it fails. What rung 3
                    // buys is a **capacity** gain and a **regrowth** gain
                    // (`forage::rung_capacity_gain` / `rung_regrowth_gain`), which is the shape the
                    // animal web has had all along — a herd gets both at pastoral and again at pen.
                    let biomass_before = patch.biomass;
                    // **The escapement room, resolved PRE-take** — the stock standing above this
                    // assignment's floor, in biomass and before any cap. It is the source of two
                    // different answers below: the work predicate ([`crew_is_working_the_source`],
                    // which replaced this arm's `EcologyPhase::Thriving` gate) and the `production`
                    // the telemetry row reports as offered.
                    // **HOW MUCH OF THE STAND THIS CREW IS HERE FOR** — the selected species'
                    // summed share of the patch's own basket, `WHOLE_BASKET` for a crew that named
                    // nothing (the default, and the neutrality bar). Every reading below that
                    // describes what the ground *offered these gatherers* is taken on it, so the
                    // published crew count, the wasted signal and the sustainable line all answer
                    // for the selection rather than for the whole basket a narrowed crew never
                    // touched. Resolved before the take, off the same pre-take state the ceiling is.
                    let selected_share = crate::forage::selected_biomass_share(
                        &crate::forage::patch_composition(
                            patch,
                            &tile_composition,
                            &flora,
                            &labor.forage,
                        ),
                        take_species,
                    );
                    // **THE WHOLE STAND'S ROOM ABOVE THE FLOOR — what the BUILD is gated on.**
                    //
                    // The gate exists to say *"a crew stripping the ground it is sowing builds
                    // nothing"*, which is a statement about **the ground being stripped**. A take
                    // selection does not strip the ground — it leaves the rest standing, by
                    // definition — and the builders are a band-level pool that is not gathering at
                    // all, so the gatherers' pickiness has no bearing on whether this ground can be
                    // worked. Narrowing this term stalled a 25-turn `Cultivate` the moment a player
                    // ticked *fibre* on the take row, silently and with no way to connect the two.
                    // It is also what `head_rung_gate` reads a stage earlier, so the quoted gate and
                    // the live one keep answering the same question.
                    let stand_above_floor =
                        forage_escapement_ceiling(*floor, biomass_before, patch.carrying_capacity);
                    // **THE BUILD'S GATE READS WHAT THE TAKE WILL PAY** ([`source_is_workable`]), the
                    // animal web's rule mirrored: a `Sow` raises the patch's `K` by
                    // `field_capacity_gain`, so the floor climbs out from under a stand that started
                    // on it and the raw escapement room would refuse the very job that moved it.
                    let ground_is_workable = source_is_workable(crate::forage::forage_take_room(
                        *floor,
                        biomass_before,
                        patch.carrying_capacity,
                        patch.growth_this_turn(),
                    ));
                    // **AND THE CREW'S OWN ROOM — what the TAKE is measured against**: the whole
                    // stand narrowed to the plants these gatherers came for. It answers the
                    // production the row reports as offered, and the lesson: you learn by working
                    // the ground, and a crew that carried nothing home did not work it.
                    let standing_above_floor = stand_above_floor * selected_share;
                    let working_the_patch = crew_is_working_the_source(standing_above_floor);
                    let provisions = forage_take(
                        patch,
                        &tile_composition,
                        workers,
                        *floor,
                        take_species,
                        &labor.forage,
                        &flora,
                        mult_f,
                        forage_per_worker_capacity,
                        seasonal,
                    );
                    let take = biomass_before - patch.biomass;
                    // **The BASKETS are charged for USE, and only for use** (§4.8,
                    // `docs/plan_denial_raid.md` §1.2). The gather's quantum is the *biomass* the
                    // crew took off the patch — the same number the fodder credit below routes — so a
                    // band that hunts all turn and gathers nothing wears no baskets at all, and a
                    // crew that found nothing standing above its floor pays nothing either. Charged
                    // **after** the take, the accrue-after-take ordering every rung's build meter
                    // uses: the turn is paid at the tier it was priced with and the cliff lands on
                    // the next turn.
                    //
                    // **Gated on the SAME predicate that chose the tier** — a crew whose kit carries
                    // no baskets gathered by hand, so there is nothing to wear out.
                    {
                        if let Some(kit) = band_equipment.as_mut() {
                            kit.wear_kit(
                                &equipment_cfg,
                                &crew_kit,
                                crate::equipment_config::WearQuantum::BiomassGathered,
                                take,
                            );
                        }
                    }
                    // **THE earn path, rungs 1–2** — the drawn-down half of the split above. A crew
                    // with nothing standing above its floor is watching the stand, not practising on
                    // it, whatever it intended; that is what replaced the `EcologyPhase::Thriving`
                    // term this site used to carry — a cliff where the model now wants a rate — and
                    // it is what makes `floor = 1.0` (leave it all standing, learn at ×2) honestly
                    // earn nothing.
                    credit_rung_lesson(
                        lesson_rung,
                        *floor,
                        take_crew_present && working_the_patch,
                        knowledge_dials,
                        faction,
                        &mut discovery,
                    );
                    if provisions > scalar_zero() {
                        cohort.stores.add(FOOD, provisions);
                    }
                    // **The FODDER account at rung 2** (issue #427). *A harvest* of `B` biomass pays
                    // `B × yield.*` into all three accounts (`docs/plan_flora_roster.md` §3) — that is
                    // unconditional, not a Field-only rule. So the SAME take `forage_take` just paid
                    // food from is routed through the patch basket's fodder component here. `0`
                    // for a basket with no fodder crop in it, so this is commodity-generic with no
                    // `role` branch. **No second collection cap**: the take is already worker-capped
                    // inside `forage_take`, so the crop the crew carries home *is* the take it made.
                    // (This used to add *"unlike a Field's managed rate"* and *"exactly as the Field
                    // arm routes its managed harvest"* — there is no Field arm and no managed rate
                    // since `docs/plan_standing_upkeep.md` §4.10; **every** plant rung is drawn down
                    // and worker-capped through this one path.)
                    //
                    // **The credit is gated on Foddering** (#433) — the same 2007 capability the
                    // pen's own hay draw reads. Since every rate is now the basket's average, a tile
                    // that happens to realize `hay_grass` pays hay on any harvest; banking it for a
                    // faction that has not learned to hay a herd would hand out animal feed nobody
                    // bid for.
                    //
                    // ⛔ **THE OTHER ARM IS A COMMITMENT TO A FODDER-BEARING SPECIES, NOT A
                    // COMMITMENT TO ANYTHING.** A crop the player chose to grow *for hay* is a bid
                    // for hay, and needs no capability to be honoured — but committing to
                    // `wild_emmer` is a bid for **grain**, and a grain field standing on ground that
                    // is 31% `hay_grass` still converts at the basket average, so an
                    // `is_some()` test paid animal feed to a faction with no pens, no Foddering and
                    // nothing that could ever eat it. That is verbatim the failure the gate exists to
                    // prevent, so the predicate asks what was actually bid for.
                    //
                    // **It is the COMMITMENT, not the rung**, so the gate lifts on the first turn of
                    // a Cultivate/Sow build on a fodder crop, while the patch still stands at rung 1
                    // and still converts at the wild basket's rate: the bid is placed when the crew
                    // starts, not when the meter fills. (Reading this as "rungs 2 and 3 are ungated"
                    // is narrower than the code and mis-states which turn the credit begins.)
                    //
                    // **It FAILS CLOSED**: a `patch.species` that does not resolve in the flora table
                    // is not fodder-bearing, so an unknown id refuses the credit rather than opening
                    // the gate. And the gate lives here, at the credit site, so the rate seam in
                    // `forage.rs` stays free of knowledge lookups.
                    //
                    // **What it does NOT touch is the conversion.** A committed patch still converts
                    // at the share-weighted average of its own basket (#433); this decides only
                    // whether the fodder component is banked at all, never how much of it there is.
                    let fodder_permitted =
                        committed_to_a_fodder_crop(patch.species.as_deref(), &flora)
                            || knows(
                                &discovery,
                                faction,
                                FODDERING_DISCOVERY_ID,
                                knowledge_threshold,
                            );
                    let fodder = if fodder_permitted {
                        scalar_from_f32(tended_take_fodder(
                            take,
                            patch,
                            &tile_composition,
                            &flora,
                            &labor.forage,
                            mult_f,
                            take_species,
                        ))
                    } else {
                        scalar_zero()
                    };
                    if fodder > scalar_zero() {
                        cohort.stores.add(FODDER, fodder);
                        band_fodder_inflow += fodder.to_f32();
                    }
                    // **Cultivate — the investment.** The crew is clearing and planting, not
                    // gathering: `forage_take` above already paid only the reduced Cultivate ceiling
                    // (the rung's `yield_fraction_while_building × the crew's throughput` — the
                    // up-front cost), and here the patch accrues toward becoming a tended crop.
                    // Gates: the faction must **know Cultivation** (earned above) and the crew must
                    // have actually drawn something off the patch.
                    //
                    // **There is no health gate any more** (`docs/plan_harvest_floor.md` §3.2). The
                    // patch's `EcologyPhase::Thriving` used to gate this, so a build stalled outright
                    // the moment a crew — anyone's crew — pulled the stand below Thriving, and the
                    // "stops accruing but is not lost" lapse state existed to make that survivable.
                    // The floor replaced it with a **rate**: `learn_multiplier` scales the accrual by
                    // how much the crew leaves standing, so pulling harder slows the build in
                    // proportion instead of stopping it at a cliff. Nothing lapses, so there is no
                    // lapse to hold progress across.
                    //
                    // **Ordering: accrue AFTER the take.** The patch pays this turn per its state at
                    // the *start* of the turn, so the pre-commit forecast the client showed is exactly
                    // what the sim paid (forecast == actual). The turn progress reaches `1.0` is the
                    // last preparing take; the full tended yield starts the next turn.
                    if improvement == Some(Improvement::Cultivate) {
                        // The rung's own gates, resolved for the engine: the faction must know the
                        // rung's unlock knowledge (Cultivation), and the crew must actually be
                        // working the patch ([`crew_is_working_the_source`] — the term that replaced
                        // the Thriving gate).
                        //
                        // **Written as its TERMS, not as a `&&` chain** — [`BuildGate`] carries
                        // which conjunct refused all the way to the wire, so a blocked head can say
                        // why. `eligible` is that value read as a bool, so the gate the sim acts on
                        // and the cause it publishes are one expression. The term list itself lives
                        // in [`plant_tended_gate`], because [`head_rung_gate`] asks the same
                        // question a stage earlier and two copies would publish two causes.
                        let gate = plant_tended_gate(
                            tended_rung.unlock_discovery_id().is_none_or(|knowledge| {
                                knows(&discovery, faction, knowledge, knowledge_threshold)
                            }),
                            // **The WHOLE stand's room, not this crew's narrowed share** — see
                            // `ground_is_workable`. What the gatherers chose to carry home says
                            // nothing about whether the ground can be cleared and planted.
                            ground_is_workable,
                            patch.species.is_some(),
                        );
                        let eligible = gate.holds();
                        // THE build seam: the rung supplies the accrual (0 unless Cultivate is the
                        // rung's verb and the gates hold); the patch owns its meter and the
                        // side-effects of completing it. **The crew is the BUILD's own**, and the
                        // floor is not a term at all — see [`RungDef::build_accrual`].
                        // **⛔ AND THE MATERIAL STORE SCALES IT** (`docs/plan_standing_upkeep.md`
                        // §2.7). `build_coverage` is the fraction of this turn's pile the band's
                        // store could pay for, settled before the loop across every claim on it —
                        // so a short store **stalls the build proportionally** rather than refusing
                        // it, and the unbanked `(1 − s)` of the crew's output is **wasted**, which
                        // is §2.5's stated rule for an indivisible supplier. It is `FULLY_SERVED`
                        // for every rung that declares no material, which is every one on the
                        // shipped ladder but `animal:pen`.
                        let accrual =
                            // **THE CREW'S WHOLE OUTPUT** — a build crew supplies nothing toward the
                            // maintenance rate, which the band's keeping pool owes for this meter at
                            // any fullness (§4.6a), so the pace is `work_cost / crew` again.
                            tended_rung.build_accrual(
                                improvement,
                                eligible,
                                build_workers,
                                entry_gear.work_per_worker,
                            ) * build_coverage;
                        // **THE SIGNED TWIN, NET OF THE ROT** — what the countdown is struck from. A
                        // meter may only be *added* to and the bleed is the decay pass's, so the
                        // estimate is where the two accounts meet: builders raising a meter more
                        // slowly than it rots are losing work already bought
                        // (`RungDef::build_balance`).
                        //
                        // **At the FULL POOL, whatever this entry is funded at this turn** — every
                        // entry is dated at `builders`, because that is what the head will hand it
                        // when its turn comes (§4.6b).
                        let balance = tended_rung.build_balance(
                            improvement,
                            eligible,
                            builders,
                            entry_gear.work_per_worker,
                            meter_rot,
                            entry_material_coverage,
                        );
                        // **THE JOB'S PRICE**, in work units — `RUNG_COST_UNSCALED` because a patch
                        // is a patch: the only per-source cost multiplier on the ladder is a
                        // species' `taming_cost_multiplier`, and a plant has no species.
                        let cultivate_cost = tended_rung
                            .build_cost(RUNG_COST_UNSCALED)
                            .expect("a rung a verb builds has a build meter");
                        // **The feed line rides the TRANSITION, not the state.** `accrue_cultivation`
                        // answers "did this call finish it", so a second band working an
                        // already-tended patch clears its verb (above) without announcing the
                        // cultivation a second time.
                        // **The gear is charged for the progress the METER TOOK, not the progress
                        // the rung offered** — measured as the meter's own delta across the accrual,
                        // so a build the patch refuses (another faction owns it) is structurally
                        // free rather than free-if-the-caller-remembered. Charged on the
                        // `build_progress` quantum against the **branch-restricted** kit
                        // (`BuildersBranchGear::wear_kit`), so a pool carrying the animal web's
                        // hurdles to a Cultivate spends none of them — wear follows the work
                        // actually done, and the branch filter zeroed their contribution.
                        let progress_before = crate::forage::patch_rung_work_done(
                            patch,
                            RungKey::PlantTended,
                            &ladder,
                        );
                        let cultivated =
                            accrual > 0.0 && patch.accrue_cultivation(faction, accrual, &ladder);
                        // **The countdown's four terms, recorded rather than published.** The band's
                        // chain pass evaluates them in **queue order** afterwards, because an
                        // entry's date is the sum of everything above it plus its own span (§4.6b) —
                        // which no per-source site can see. **Against the JOB'S OWN COST** — the
                        // pool's kit is in the `balance` beside it (§4.8), never in the bar.
                        //
                        // **A GATE THAT REFUSES IS "NO ESTIMATE"** — nothing has been promised at
                        // all, which is not the same as a staffing that never gets there. What the
                        // *hands* decide is only the empty-meter case: work already banked promises
                        // as much as a crew does, so a half-built meter with nobody on it answers.
                        //
                        // **AND ONLY FOR A SOURCE THAT CARRIES AN ENTRY** — see
                        // `entry_declares_a_rung`.
                        if entry_declares_a_rung {
                            build_quotes.push((
                                BuildSource::Patch(*tile),
                                BuildQuote {
                                    cost: cultivate_cost,
                                    banked: crate::forage::patch_rung_work_done(
                                        patch,
                                        RungKey::PlantTended,
                                        &ladder,
                                    ),
                                    legs: entry_destination.map_or_else(Vec::new, |destination| {
                                        crate::forage::patch_build_legs(
                                            patch,
                                            destination,
                                            &ladder,
                                            field_cost_multiplier,
                                        )
                                    }),
                                    balance,
                                    gate,
                                    material_coverage: entry_material_coverage,
                                },
                            ));
                        }
                        charge_build_wear(
                            band_equipment.as_deref_mut(),
                            &equipment_cfg,
                            &entry_gear.wear_kit,
                            crate::forage::patch_rung_work_done(
                                patch,
                                RungKey::PlantTended,
                                &ladder,
                            ) - progress_before,
                        );
                        if cultivated {
                            // **THE RUNG'S OWN COMPLETION IS ALWAYS ANNOUNCED** (just below), because
                            // a player who ordered *"take it to Field"* wants to see the ground become
                            // Cultivated on the way. **The QUEUE only retires at the DESTINATION** —
                            // an entry names where the land should end up and stays at the head until
                            // it arrives, so a two-leg `sow` does not hand the pool away at 50.
                            if arrived_at_destination(entry_destination, patch.standing().held) {
                                completed.push((
                                    BuildSource::Patch(*tile),
                                    entry_job.unwrap_or(BuildJob::Rung(Improvement::Cultivate)),
                                ));
                            }
                            event_log.push(CommandEventEntry::new(
                                tick.0,
                                CommandEventKind::Cultivate,
                                faction,
                                format!("Cultivated patch at ({}, {})", tile.x, tile.y),
                                Some(format!(
                                    "status=complete action=cultivate x={} y={}",
                                    tile.x, tile.y
                                )),
                            ));
                        }
                    }
                    // **Sow — the rung-3 investment**, the twin of Cultivate above and the
                    // same shape: `forage_take` has already paid only the `plant:field` rung's dip,
                    // and here the patch accrues toward becoming a Field. On ground the crew *just*
                    // sowed that dip is honestly ~0 (there is no standing crop to take a fraction of):
                    // a bare-ground field is pure investment, paid entirely in the 25 turns of labor.
                    //
                    // **Not gated on Thriving, unlike Cultivate** — and that is load-bearing, not a
                    // relaxation: freshly sown ground starts at the reseed floor, i.e. *Collapsing* by
                    // construction, so a health gate would make sowing bare ground impossible. You
                    // *tend* a healthy wild stand; you *plant* bare ground. (The animal side already
                    // draws the same line — `Tame` has no health gate either.)
                    if improvement == Some(Improvement::Sow)
                        && accrue_field(
                            patch,
                            field_rung,
                            improvement,
                            sow_gate,
                            faction,
                            &mut event_log,
                            tick.0,
                            *tile,
                            build_workers,
                            builders,
                            entry_gear.work_per_worker,
                            &ladder,
                            band_equipment.as_deref_mut(),
                            &equipment_cfg,
                            &entry_gear.wear_kit,
                            &mut build_quotes,
                            entry_declares_a_rung,
                            meter_rot,
                            field_cost_multiplier,
                            entry_material_coverage,
                        )
                    {
                        // The Field is the top of the plant branch, so finishing it is always
                        // arriving — but the test is stated rather than assumed, so a rung 4 does not
                        // silently make this the wrong retirement.
                        if arrived_at_destination(entry_destination, RungKey::PlantField) {
                            completed.push((
                                BuildSource::Patch(*tile),
                                BuildJob::Rung(Improvement::Sow),
                            ));
                        }
                    }
                    // **THE PROJECTION — "what would the next rung take THIS crew?"**
                    // (`docs/plan_unit_costed_work.md` §11). Nothing is being built here, which is by
                    // definition the state the compose sheet is looking at, so a `-1` would withhold
                    // the one readout that makes the arc's thesis legible at exactly the moment the
                    // player is deciding. It is the `penUpkeep` rule applied to turns: **always
                    // meaningful, never `-1`-because-unstarted**.
                    //
                    // Quoted **at the band's whole builders pool** against the rung the patch would
                    // climb next — `patch_rung_key(..).above()`, the ladder's own order — and from the
                    // work already banked on **that** rung, so a build the player walked away from
                    // quotes the turns still owed rather than the whole job again. The gates below are
                    // the ones `validate_cultivate` / `validate_sow` would apply: **a projection must
                    // never quote a rung the command would refuse**, which is the `sowSiteRefusal`
                    // failure mode wearing a turn count.
                    //
                    // **AND IT IS DATED AT THE BACK OF THE LINE.** The chain pass adds everything
                    // already queued ahead of it, because that is where a newly queued build would
                    // actually go — quoting it as though it went to the head would over-promise by
                    // the whole queue.
                    if improvement.is_none() {
                        let projected = patch_rung_key(patch).above().and_then(|next_key| {
                            let next = ladder.rung(next_key);
                            // The **work predicate rides rung 2 only**, exactly as the live arms
                            // do: `Cultivate`'s `eligible` carries `crew_is_working_the_source`
                            // and `accrue_field`'s deliberately does not — bare ground stands
                            // below every floor, so requiring room would make the rung
                            // create-from-nothing exists for unquotable.
                            let crew_is_at_work = match next.verb_improvement() {
                                // The whole stand's room, exactly as the live arm reads it — a
                                // projection that quoted the gatherers' narrowed share would
                                // promise a stall the build will not actually hit.
                                Some(Improvement::Cultivate) => ground_is_workable,
                                _ => true,
                            };
                            // Stated as terms, like the live arms', so a projection quote carries
                            // its refusing conjunct too.
                            let gate = BuildGate::first_refusal(&[
                                (
                                    next.unlock_discovery_id().is_none_or(|knowledge| {
                                        knows(&discovery, faction, knowledge, knowledge_threshold)
                                    }),
                                    BuildGate::Knowledge,
                                ),
                                (crew_is_at_work, BuildGate::Escapement),
                                (land_admits(next), BuildGate::Site),
                                (
                                    patch.owner.is_none_or(|owner| owner == faction),
                                    BuildGate::OwnedByOther,
                                ),
                                // **Something in this basket has to climb**, the same
                                // `resolve_committed_species` seam the commit above reads — a
                                // patch whose whole basket stops below the rung is one the build
                                // meter would never move.
                                (
                                    resolve_committed_species(
                                        species.as_deref(),
                                        &tile_composition,
                                        &flora,
                                        next_key,
                                    )
                                    .is_ok(),
                                    BuildGate::NoCrop,
                                ),
                            ]);
                            // The meter the quoted rung would fill — the twin of
                            // `advance_cultivation`'s own verb dispatch over the two plant
                            // meters.
                            let banked = match next.verb_improvement() {
                                Some(Improvement::Sow) => crate::forage::patch_rung_work_done(
                                    patch,
                                    RungKey::PlantField,
                                    &ladder,
                                ),
                                _ => crate::forage::patch_rung_work_done(
                                    patch,
                                    RungKey::PlantTended,
                                    &ladder,
                                ),
                            };
                            ladder.projected_build_quote(
                                next,
                                // **THIS GROUND'S PRICE FOR THE RUNG IT WOULD CLIMB NEXT.**
                                // Clearing is clearing, so `plant:tended` is flat; a Sow is priced
                                // by how much of the tile the crop still has to replace (§4.15).
                                // **This is the pre-commit quote** — the number the compose sheet
                                // and the `⌃` mark show before the player declares anything — so it
                                // has to be the same multiplier the arm will charge, or the sheet
                                // prices a job the sim does not.
                                match next_key {
                                    RungKey::PlantField => field_cost_multiplier,
                                    _ => RUNG_COST_UNSCALED,
                                },
                                banked,
                                builders,
                                entry_gear.work_per_worker,
                                gate,
                                // **The SOURCE's live bleed**, not the quoted rung's rate — a build
                                // crew supplies nothing toward the rate, so what nets off a quote is
                                // what the ground is losing. On a rung nobody has started there is
                                // nothing banked and therefore nothing to rot, and the quote is the
                                // honest `work_cost / pool`.
                                meter_rot,
                            )
                        });
                        if let Some(quote) = projected {
                            build_quotes.push((BuildSource::Patch(*tile), quote));
                        }
                    }
                    // **The MATERIAL account of the same take** (`docs/plan_crafting_and_materials.md`
                    // §2) — the bast, boll and stem in what the crew carried off the patch, and since
                    // arc #527 the **only** non-food account a drawn-down patch pays. It is
                    // **decomposed** rather than averaged: one credit per species in the basket, each
                    // keeping its own exact reading, because averaging two plants' characteristic
                    // vectors would invent a plant that is not growing there
                    // ([`crate::forage::patch_material_yields`]). Credits that land in the same band
                    // merge in the store, which is where merging belongs.
                    let credited_materials = crate::materials_config::credit_material_yield(
                        &mut cohort.stores,
                        &materials_cfg,
                        &crate::forage::patch_material_yields_taking(
                            patch,
                            &tile_composition,
                            &flora,
                            &labor.forage,
                            take_species,
                        ),
                        take,
                        mult_f,
                    );
                    // Sustainable = one turn's MSY of the patch at its **pre-take** biomass, in
                    // provisions (same conversion + output multiplier as the actual take), against
                    // the patch's **own** curve (`patch_ecology`) — a tended patch's sustainable line
                    // sits on its boosted `r`, so Sustain-gathering it reads no ⚠ while
                    // Surplus-gathering it does. This lights the over-forage ⚠ for free the moment
                    // `actual > sustainable`, and since slice 7 that fires on a **tended** patch too:
                    // rung 2 draws down, so it can be over-farmed. (It never could before — the old
                    // managed branch recorded `sustainable == actual` by construction.)
                    let sustainable = sustainable_yield(
                        biomass_before * selected_share,
                        patch.carrying_capacity * selected_share,
                        &patch_ecology(patch, &labor.forage),
                    ) * patch_provisions_per_biomass_taking(
                        patch,
                        &tile_composition,
                        &flora,
                        &labor.forage,
                        take_species,
                    ) * mult_f;
                    // The two staffing signals, from the same take, and **both about the TAKE
                    // activity alone** (`docs/plan_standing_upkeep.md` §2.2) — the build's crew and
                    // the keeping's are the player's own numbers and need no inverting.
                    // **Overstaffing**: invert the take by the per-worker throughput the take
                    // actually ran at, so a labor-bound low-season patch isn't falsely flagged.
                    // **Understaffing** (`wasted`): what the escapement ceiling offered beyond what
                    // the crew could gather — here it is not lost, it simply stays in the stock and
                    // regrows, but it is the same "add hands" answer.
                    let per_worker_biomass =
                        forage_per_worker_biomass(forage_per_worker_capacity, seasonal);
                    let workers_needed = workers_needed_for_take(take, per_worker_biomass, workers);
                    // The stock the patch **offered** this turn — the same pre-take escapement room
                    // the work predicate read, and unscaled by anything the build is doing: the
                    // ground standing above the floor is there whether or not a second crew is
                    // clearing it, so a thin gathering crew's shortfall shows up honestly as
                    // `wasted` — "this is what more hands would have brought home".
                    let production =
                        standing_above_floor.clamp(0.0, biomass_before * selected_share);
                    // **The arrival schedule — computed POST-take, unlike `realized`.** It
                    // answers "when does the next food land", so it must start from the state the
                    // turn leaves behind: projecting from the pre-take state would re-promise the
                    // delivery this turn has already paid. Slot 0 is therefore genuinely the
                    // *next* turn's delivery.
                    let arrivals = crate::forage::project_arrivals_forage(
                        patch,
                        &tile_composition,
                        &labor.forage,
                        &flora,
                        forage_per_worker_capacity,
                        seasonal,
                        mult_f,
                        workers,
                        *floor,
                        take_species,
                        arrivals_horizon,
                    );
                    yields[idx] = SourceYield {
                        actual: provisions.to_f32(),
                        // **The credited value, not a recomputation** (issue #449) — the very
                        // `fodder` scalar added to the `FODDER` store above, **including the
                        // `fodder_permitted` gate**: a faction that has not learned Foddering was
                        // credited nothing unless the patch is committed to a fodder-bearing species,
                        // so the row must read `0.0` on a grain field for exactly the turns nobody
                        // was paid. Re-deriving `tended_take_fodder` here would publish a number
                        // nobody was ever paid, which is precisely what this readout exists not to
                        // do.
                        fodder: fodder.to_f32(),
                        // **The credited materials, not a recomputation** — exactly what
                        // `credit_material_yield` deposited (the discipline `fodder` above carries).
                        materials: credited_materials,
                        sustainable,
                        // The forward-projected steady headline (computed pre-take above).
                        realized: forage_realized,
                        arrivals,
                        // Resolved: a fact, so the band is a point.
                        range: YieldRange::certain(provisions.to_f32()),
                        wasted: forage_provisions(
                            (production - take).max(0.0),
                            patch_provisions_per_biomass_taking(
                                patch,
                                &tile_composition,
                                &flora,
                                &labor.forage,
                                take_species,
                            ),
                            mult_f,
                        ),
                        workers_needed,
                        // **The ⚠ — intent AND ability**, through the plant web's one producer
                        // ([`crate::forage::forage_take_overdraws`]). A floor below the food peak is
                        // only an overdraw if these gatherers can actually get the stand down to it;
                        // a crew that settles above the peak and holds there is drawing nothing
                        // below what the patch sustains, whatever the dial says. Stock terms are the
                        // **selected** share's and the pre-take biomass, matching `sustainable`.
                        overdraws: crate::forage::forage_take_overdraws(
                            patch,
                            &labor.forage,
                            biomass_before * selected_share,
                            patch.carrying_capacity * selected_share,
                            workers as f32 * per_worker_biomass,
                            *floor,
                        ),
                    };
                }
                LaborTarget::Hunt { fauna_id, floor } => {
                    let Some(herd_pos) = registry.find(fauna_id).map(|herd| herd.position()) else {
                        // Herd despawned (extinction / another hunter) → lapse.
                        lapsed.push(idx);
                        event_log.push(CommandEventEntry::new(
                            tick.0,
                            CommandEventKind::Hunt,
                            faction,
                            format!("hunters lost {} (herd dispersed)", fauna_id),
                            Some(band_detail_token(
                                "status=lapsed reason=herd_gone".to_string(),
                                band_id,
                            )),
                        ));
                        continue;
                    };
                    let distance = crate::grid_utils::hex_distance_wrapped(
                        band_pos,
                        herd_pos,
                        grid_width,
                        wrap_horizontal,
                    );
                    if distance > hunt_reach {
                        // Past the leash → the assignment lapses; workers return to the pool.
                        lapsed.push(idx);
                        event_log.push(CommandEventEntry::new(
                            tick.0,
                            CommandEventKind::Hunt,
                            faction,
                            format!("hunters lost the {} — it ranged too far", fauna_id),
                            Some(band_detail_token(
                                format!(
                                    "status=lapsed reason=out_of_leash distance={} reach={}",
                                    distance, hunt_reach
                                ),
                                band_id,
                            )),
                        ));
                        continue;
                    }
                    // **A HOLDING ROW LASTS EXACTLY AS LONG AS THERE IS SOMETHING TO HOLD** — the
                    // animal twin of the Forage arm's, on the animal web's own seam
                    // (`fauna::herd_keeping_rung`, which is `None` for a herd nobody owns and has
                    // not penned). A band with no hands on a wild herd is simply not hunting it.
                    if !take_crew_present
                        && queued.is_none()
                        && !source_has_a_meter_at_risk(
                            &assignment.target,
                            &forage_registry,
                            &registry,
                            &ladder,
                        )
                    {
                        lapsed.push(idx);
                        continue;
                    }
                    // **THE GEAR THIS SOURCE'S OWN ENTRY IS RAISED WITH** — the animal twin of the
                    // Forage arm's, resolved once so the accrual, the balance, the projection and
                    // the wear charge are all struck at one number.
                    let herd_source = BuildSource::Herd(fauna_id.clone());
                    let entry_gear = builders_gear
                        .for_source(&herd_source, queued_destination(&build_queue, &herd_source));
                    let Some(herd) = registry.herds.iter_mut().find(|herd| herd.id == *fauna_id)
                    else {
                        continue;
                    };
                    // **WHICH CARRY TIER THIS HERD IS WORKED AT — the pen's or the range's.**
                    // **ONE CARRY RATE, PENNED OR WILD** (issue #543). What a worker can carry is a
                    // fact about the people and their gear, asked once from the band's kit and blind
                    // to the ground they are standing on — so the pen reads the same
                    // `hunt_per_worker_biomass` the range does. A `PenCarry` stat used to fork here;
                    // it survived the item that discriminated (the hurdles, retired to a material by
                    // `docs/plan_standing_upkeep.md` §4.9 item 12) with nothing left to say, and was
                    // deleted. See `.claude/rules/core_sim/equipment.md` → "Carry is carry".
                    //
                    // **It is resolved once, above, and every forecast, projection, crew inversion
                    // and take below reads it** — because the forecast-equals-actual invariant
                    // (`yield-forecast.md`) is exactly the promise that the number the seed quoted
                    // and the number the turn pays came from one place.
                    let herd_carry_per_worker = hunt_per_worker_biomass;
                    // **THE LIVE VERB, DERIVED** — the animal twin of the Forage arm's: the
                    // declaration counts only where the meter it names is at zero, and both animal
                    // meters are monotone, so a part-built rung stays in flight until it completes.
                    let improvement = fauna::herd_build_verb(herd, declared);
                    // **NOTHING LEFT TO BUILD needs no test any more** — see the Forage arm: a
                    // declaration on a finished meter derives to `None` (`fauna::herd_build_verb`).
                    // **THE STANDING UPKEEP, PAID BY THE BAND'S KEEPING POOL** — the animal twin;
                    // see the Forage arm for why it is stamped once, here, before the pen's tend
                    // branch returns. A herd's demand is its **keeper load** (`head count /
                    // animals_per_herder`) times the rung's rate (`UpkeepScale::SourceLoad`), so a
                    // shepherd's 200 fowl and a cowherd's 12 aurochs are one rate.
                    //
                    // **The pool answers for it at any fullness** (§4.6a) — a `Tame` in flight is
                    // billed exactly as a tamed herd is, and its builders supply nothing toward the
                    // rate. The verb still names the meter, so a `Corral` starting on a herd with no
                    // pen progress answers for `animal:pen` from its first turn: the supply is read
                    // by the *next* Logistics pass, so it has to describe the meter that pass judges.
                    //
                    // **Accumulated, not assigned** — the demand is per-SOURCE, so the keepers of
                    // every band working this herd sum into one supply and the last band visited
                    // must not speak for all of them. `advance_husbandry` zeroes it once per turn.
                    let keeping_supplied =
                        fauna::herd_upkeep_supply(herd, improvement, keeping_share);
                    herd.upkeep_supplied += keeping_supplied;
                    // **AND THE MATERIAL HALF OF THE SAME BILL** — the pen's hurdles, on the plant
                    // twin's own two rules (see the Forage arm). The bill's *work* stamp is struck
                    // pre-loop for this web, so this is the one place the material stamp can be:
                    // `apply_material_keeping` is first-write-wins for the same reason.
                    apply_material_keeping(
                        &mut cohort.stores,
                        &material_settlement.upkeep[idx],
                        &mut herd.upkeep_materials_demanded,
                        &mut herd.upkeep_materials_supplied,
                    );
                    // **The plant twin's charge** — the keeping tools are spent on the work the pool
                    // actually supplied to this herd. See the Forage arm.
                    charge_keeping_wear(
                        band_equipment.as_deref_mut(),
                        &equipment_cfg,
                        keeping_wear_kit,
                        keeping_supplied,
                    );
                    // **WHAT THE METER IS LOSING** — the plant twin's seam, and on the shipped
                    // ladder always `0`: neither animal rung declares a `meter_decay`, because an
                    // under-kept flock **sheds animals** instead. So nothing eats an animal build,
                    // and the countdown below reads the crew's own output. See `fauna::herd_meter_rot`.
                    let meter_rot = fauna::herd_meter_rot(herd, &fauna, &ladder);
                    // **The steady headline** — the forward-projected average food/turn over the next
                    // `realized_horizon` turns, computed from the herd's PRE-take state (before the pen
                    // feed/harvest or the wild take mutates it), so it equals the assign-time seed
                    // exactly. Rate-based (an average over the horizon), so it is smooth where `actual` pulses;
                    // a corralled herd projects its managed pen yield instead. Both the pen-tend and the
                    // wild-take branches record this one value.
                    let hunt_realized = fauna::project_realized_hunt(
                        herd,
                        &fauna,
                        herd_carry_per_worker,
                        &party_for(herd.body_mass),
                        mult_f,
                        workers,
                        *floor,
                        realized_horizon,
                    );
                    // **THE earn path (§4)** — the exact mirror of the Forage arm's call, and the
                    // heart of this ladder: the lesson is read off **the rung this herd stands on**,
                    // so the *same* Sustain hunt teaches **Herding** on a wild herd and **Penning** on
                    // a tamed one ("you learn herding by managing wild herds; penning by managing
                    // tamed ones"). The old hard-coded `Sustain && Thriving → HERDING_DISCOVERY_ID`
                    // branch is retired; `earns_knowledge` drives it now.
                    //
                    // **The RUNG is resolved here, above the branches; the CREDIT is applied inside
                    // each of them** — the corral tend arm `continue`s, and the two branches answer
                    // `eligible` differently (a pen is *tended*, a wild herd must have stock standing
                    // above the crew's floor). It used to be one call here, which was behaviour-
                    // neutral only while the gate read `ecology_phase`, a value no take moves. Both branches call `credit_rung_lesson`, so every rung still reaches the
                    // earn path — including the pen, whose `earns_knowledge` is Foddering.
                    //
                    // The two webs cannot cross-teach (§4.2) for free: a herd resolves to an `animal`
                    // rung, so only an animal knowledge is reachable from here.
                    let lesson_rung = fauna::herd_rung(herd, &ladder);
                    // **Corral (Rung 1c) — the pen is a managed POPULATION, not a flat rate.** A Hunt
                    // assignment on a **corralled** herd is herding/tending it, not hunting, and the
                    // turn has two halves (`docs/plan_corral_managed_population.md` §3.1):
                    //
                    // 1. **FEED.** The pen demands `fodder_per_biomass × biomass` in **fodder** — a
                    //    penned herd is confined and cannot roam to graze, so it eats the grass its
                    //    fenced footprint grew and the hay its keeper carried in, and nothing else.
                    //    **The keeper's larder is not on the table**: human food is not animal feed,
                    //    and a pen that outgrows its pasture must shrink rather than take the food out
                    //    of its keepers' mouths. What grass and hay leave unpaid is a shortfall, so
                    //    `fed_fraction < 1` and next turn's `advance_husbandry` reads the flag and
                    //    shrinks the herd — the deliberate one-turn lag.
                    // 2. **HARVEST.** The keeper takes the *pen's* MSY (`corral_provisions` →
                    //    `sustainable_yield` under the pen's ecology, `r` = 0.60), and — unlike the
                    //    retired flat rate — this **draws the herd down**, which is exactly what makes
                    //    it sustainable: the herd converges on `K_pen/2` and pays `r·K/4` forever.
                    //
                    // Marks the herd tended so it doesn't escape in `advance_husbandry`. The animal
                    // mirror of the tended-patch arm in Forage.
                    if herd.is_corralled() {
                        herd.corralled_tended_this_turn = true;
                        // **THE FEED IS ALREADY SETTLED** (`docs/plan_standing_upkeep.md` §4.9 item
                        // 9b). Both terms below — the pasture offset (Grazing 2d §2.3) and the hay
                        // draw with its Foddering gate (Flora Roster F3 §5.2) — are struck by
                        // [`settle_pen_hay`] across **every** pen this band keeps, before the loop.
                        // What is left here is applying this pen's share: the arm stamps the herd and
                        // spends the settled hay, and takes no allocation decision of its own.
                        //
                        // **Why it moved.** The draws used to happen here, in loop order, so a store
                        // that could not cover every pen fed the earliest row and starved the last.
                        //
                        //   demand_grass     = fodder_per_biomass × biomass   (grass to fully feed it)
                        //   pasture_fraction = clamp(footprint_intake / demand_grass, 0, 1)
                        //   fed_fraction     = clamp((footprint_intake + hay) / demand_grass, 0, 1)
                        //
                        // A lush footprint (pasture_fraction → 1) feeds the pen for free; a barren one
                        // (→ 0) lives entirely on hay, and starves for whatever the hay cannot cover.
                        //
                        // **A pen with no settled share is a pen the settlement did not see**, which
                        // is only possible if this arm and [`settle_pen_hay`] disagree about which
                        // rows are pens in reach — so it feeds on nothing (and reads starving) rather
                        // than silently inventing a draw the split never accounted for.
                        let share = pen_feed.get(fauna_id).copied();
                        herd.pen_pasture_fraction =
                            share.map_or(NOTHING_DEMANDED, |share| share.pasture_fraction);
                        // The settled hay, spent. The share is bounded by what the store held when it
                        // was struck and nothing takes `FODDER` between then and here, so this take
                        // pays in full; `LocalStore::take` still reports what it actually took, which
                        // is the number the herd is stamped with.
                        let fodder_draw = share.map_or(NOTHING_DEMANDED, |share| {
                            cohort
                                .stores
                                .take(FODDER, scalar_from_f32(share.fodder_share))
                                .to_f32()
                        });
                        herd.fodder_draw = fodder_draw;
                        // **WHAT THIS PEN STILL HAS TO BE GROWN FOR**, in fodder units per turn —
                        // the gap the footprint's own grass leaves. It is summed into this band's own
                        // `last_fodder_need` below and differenced against the draw just above, and
                        // those are its only two readers: it rode the wire as `penHayNeed` until
                        // nothing turned out to read it (`Herd::pen_hay_need`), because what a pen row
                        // states is how much MORE it needs.
                        //
                        // **Ungated, unlike the draw above.** A band that has not learned Foddering
                        // settles a `0.0` hay *share* and still keeps a herd short by exactly this
                        // much, so the need states the herd's condition and the draw states what was
                        // done about it.
                        let hay_need = share.map_or(NOTHING_DEMANDED, |share| share.hay_need);
                        band_fodder_need += hay_need;
                        // **AND WHAT IT WILL ACTUALLY ASK THE STORE FOR** — the same gap behind the
                        // Foddering gate the draw above is behind, summed into the band's drain so
                        // the runway counts down the hay that really leaves the store.
                        band_fodder_drain +=
                            share.map_or(NOTHING_DEMANDED, |share| share.fodder_demand);
                        // **HOW MUCH MORE FODDER THIS PEN NEEDS** — the need above less the draw
                        // above it, published as `penFodderShortfall`. It is the number the player
                        // acts on: the row reads "40% pasture · 7% fodder · needs 11.3 more/turn"
                        // instead of asking a reader to subtract two figures sitting on one line.
                        //
                        // **Stamped here, between its own two terms**, so the difference cannot
                        // describe a different turn from the numbers it is a difference of.
                        //
                        // **Ungated, like the need and unlike the draw.** A band without Foddering
                        // draws nothing, so its shortfall is its whole need — the herd is dying and
                        // the remedy is knowledge, which is the case this readout is most for.
                        //
                        // **Clamped**, though [`settle_pen_hay`] never settles a pen more hay than
                        // its own gap: the take is quantised through `Scalar`, which can round a
                        // fully-served pen's draw a fraction of a unit above the need it was
                        // settled from. A negative shortfall is not a reading, so it floors.
                        herd.pen_fodder_shortfall = (hay_need - fodder_draw).max(NOTHING_DEMANDED);
                        // **THE FED FRACTION, IN ONE UNIT** — `(footprint_intake + hay) ÷ demand`, all
                        // fodder. It used to add this land-and-hay share to the paid share of a
                        // *food*-unit larder bill, and mixing the two units is precisely how the
                        // people's bread came to be counted as feed. Read a stage later and a turn
                        // later (`advance_husbandry`'s `starve_underfed_pen` / `regrow_biomass`, both
                        // in Logistics, which precedes Population), so nothing in this loop depends on
                        // when in the pass it is stamped.
                        herd.pen_fed_fraction =
                            share.map_or(NOTHING_DEMANDED, |share| share.fed_fraction);
                        // This band keeps this pen — its `K_pen` gets the fodder-flow term next turn.
                        kept_pens.push(fauna_id.clone());
                        // Shared with the pre-commit forecast (`fauna::hunt_forecast`) so the
                        // client's "expected yield" for a corralled herd is exactly what it is paid.
                        // **THE ORDINARY ESCAPEMENT CEILING, AT THIS ASSIGNMENT'S OWN FLOOR** — the
                        // stock standing above the floor, exactly as a wild and a pastoral take
                        // resolve it. It used to be `pen_yield_biomass`: a flat managed production
                        // with no floor term at all, so the harvest floor — the one pressure lever
                        // the player holds — did **nothing** at rung 3 and a pen could not be
                        // over-hunted.
                        //
                        // **Production and draw are separate concerns. A rung may change production;
                        // no rung changes the draw.** What penning buys is the `r` gain and the
                        // density gain the ceiling is *computed from* (`herd_ecology` /
                        // `herd_capacity`), the slower escape, and the handling gain below.
                        //
                        // **At the herd's CURRENT biomass**, which is the basis `hunt_take` uses —
                        // one draw model means one basis too, or the pen would be harvesting this
                        // turn's regrowth on top of the standing surplus while the range take does
                        // not.
                        // **One draw model means one basis** — the same [`fauna::take_room`] the
                        // range take is bounded by, growth share and all, so a penned herd below its
                        // own climbing floor is not quietly harvested on a different rule.
                        let production = fauna::herd_take_room(herd, *floor, &fauna);
                        // **Collection** (slice 7 — the Field's twin): the keeper still has to carry
                        // the meat home, so the take is capped by the crew's own throughput — the
                        // *same* `per_worker_biomass_capacity` a wild hunt is capped by. The pen
                        // collapses the *policy* axis (the herd is yours), never the worker cap; one
                        // keeper used to collect the whole pen however big it grew.
                        //
                        // **And it is butchered in WHOLE ANIMALS** (slice 8 — the same
                        // `quantise_animal_take` a wild hunt runs): you cannot slaughter half a cow
                        // any more than you can half-kill a mammoth. A keeper who cannot haul a whole
                        // beast still takes one and wastes the rest.
                        //
                        // **The pen nonetheless reads steady — emergently, not by exemption.** It
                        // breeds at up to 3× the wild rate (`pen_gain`), so its MSY clears one body's
                        // worth every turn for every pennable species and `affordable >= 1` always
                        // holds. A herd that breeds fast enough to slaughter from continuously never
                        // has to wait — that is the real-world reason a pen is a steady supply, and
                        // rung 3's actual payoffs are the faster `r`, no chasing, the self-feeding
                        // footprint and a `K` you control. On poor enough range a pen *will* pulse
                        // (the aurochs is closest), and that is honest. See `managed_yield_biomass`.
                        // **`workers` is the TAKE crew, and extending the pen does not touch it**
                        // (`docs/plan_standing_upkeep.md` §2.5). A ring is raised by the band's
                        // `builders` pool, and only while it is the head of that band's queue
                        // (`ring_workers`, below), so the keepers slaughtering out of the pen go on
                        // slaughtering at their own rate.
                        // The forgone yield that used to price a ring — the pen rung's retired
                        // `yield_fraction_while_building`, and then a share of one shared budget — is
                        // now simply *the hands that are fencing instead of butchering*, which is a
                        // number the player typed rather than one the sim derived.
                        // # ⛔ THE PEN TAKES THROUGH `hunt_take`, LIKE EVERY OTHER RUNG
                        //
                        // This arm used to compose a take of its own — the keepers' handling rate
                        // clamped by the room, floored, quantised — which made the ladder a **mode
                        // switch**: a fenced herd ran no retreat and no fight, so taming and penning
                        // bought nothing at the kill and a bare-handed band butchered an aurochs a
                        // stalking party could not scratch. **The take runs its three stages at every
                        // rung now** (`docs/plan_standing_upkeep.md` §4.9 item 12b), and the rung
                        // tunes the first two only:
                        //
                        // - **engage** — `husbandry.pen_engage_gain` on the species' own rate
                        //   (`fauna::herd_engage_rate`): a keeper handles far more animals a turn
                        //   than a hunter, because they are standing still rather than running away;
                        // - **retreat** — `husbandry.pen_wariness` on the species' own `wariness`
                        //   (`fauna::herd_wariness`): a fence calms, it does not hypnotise;
                        // - **fight** — the species' `defense` against this party's `attack`,
                        //   **undiscounted**. Containment solves catching; weapons solve killing.
                        //   No weapons, no beef.
                        //
                        // So this is the *same call* the range arm makes below, with the pen's own
                        // two terms handed in: the band's own carry tier (`herd_carry_per_worker`,
                        // off `hunt_carry` — carry is carry, issue #543) and this assignment's own
                        // floor. The band has no carry
                        // room, so the cap is unbounded exactly as the Hunt row passes it.
                        //
                        // `hunt_take` does the room, the three stages, the wound store-back, the
                        // quantiser and the herd's own loss — one path rather than two that agree
                        // today.
                        let outcome = hunt_take(
                            herd,
                            workers,
                            *floor,
                            herd_carry_per_worker,
                            &party_for(herd.body_mass),
                            &fauna,
                            f32::INFINITY,
                            fauna::HuntDraw::Seeded(fauna::retreat_seed(
                                sim_config.map_seed,
                                tick.0,
                                &herd.id,
                                workers,
                            )),
                        );
                        let take = outcome.take;
                        // **A pen charges TWO quanta over TWO DIFFERENT NUMBERS: the sled is charged
                        // for what it HAULED, the handling gear for what it BUTCHERED.** Hurdles,
                        // halters, a butchering stone and vessels are worked on the whole beast that
                        // was brought out of the pen and killed — not on the fraction of it that made
                        // it home — so `biomass_collected` rides [`AnimalTake::killed_biomass`] while
                        // `biomass_hauled` rides `carried`. Charging both over `carried` under-charged
                        // the handling gear for exactly the animal it did the most work on: waste in
                        // this branch needs `workers × hunt_carry < body_mass`, which a Wild Aurochs
                        // (`body_mass 120`, one required keeper at `animals_per_herder 12`) reaches on
                        // every slaughter — 120 killed against 40 carried at the equipped tier.
                        //
                        // Two quanta rather than one is *separately* what lets a band that only keeps
                        // pens leave a sled it never took onto the range untouched, and what lets
                        // either life be retuned without moving the other.
                        //
                        // **AND SINCE §4.9 item 12b A THIRD RIDES BESIDE THEM: the WEAPON, per
                        // strike.** The keepers are swinging now — a pen resolves the ordinary fight
                        // — so the spear that killed the beast blunts on the blow exactly as it does
                        // on the range. It is *in addition to* the two above, never instead of them:
                        // the sled still hauls what was carried and the handling gear still works
                        // the whole carcass. (What this comment used to say — *"a penned beast is
                        // slaughtered, not stalked, so there is no fight and no spear to blunt"* —
                        // was true of the take path that retired with the exemption.)
                        //
                        // Each charge is gated on the predicate that chose its own tier, so a keeper
                        // with no sled dragged the carcass by hand and wore nothing out doing it.
                        if let Some(kit) = band_equipment.as_mut() {
                            outcome.fight.charge_strike_wear(kit, &equipment_cfg);
                            kit.wear_kit(
                                &equipment_cfg,
                                &crew_kit,
                                crate::equipment_config::WearQuantum::BiomassHauled,
                                take.carried,
                            );
                            kit.wear_kit(
                                &equipment_cfg,
                                &crew_kit,
                                crate::equipment_config::WearQuantum::BiomassCollected,
                                take.killed_biomass(),
                            );
                        }
                        // **A pen changes the INTENSITY, never the PRODUCT** — the keeper is paid
                        // this herd's own species vector, so a penned wolf yields pelts and no meat
                        // exactly as a wild one does (`docs/plan_hunt_yield_model.md`).
                        let pen_yield = herd_hunt_yield(herd, &fauna);
                        let paid = pen_yield.apply(take.carried, mult_f);
                        let provisions = scalar_from_f32(paid.provisions);
                        if provisions > scalar_zero() {
                            cohort.stores.add(FOOD, provisions);
                        }
                        // **THE earn path, rung 3** — *you learn to hay a herd by keeping one*
                        // (`animal:pen` earns Foddering). Kept on the **managed** credit even though
                        // the take is drawn down now: the work rung 3 teaches is the *tending*, which
                        // a keeper does whether or not this turn's escapement room cleared a whole
                        // body — see [`credit_managed_rung_lesson`]. Pacing a keeper's learning off
                        // the slaughter quantum is the `body_mass` artefact the harvest-floor arc
                        // already rejected on the take side.
                        credit_managed_rung_lesson(
                            lesson_rung,
                            take_crew_present,
                            knowledge_dials,
                            faction,
                            &mut discovery,
                        );
                        // **A pen changes the INTENSITY, never the PRODUCT** — so the keeper is paid
                        // this herd's own material rows too, off what was carried home, exactly as
                        // the range take is. Penning an animal does not change what it is made of.
                        let credited_materials = crate::materials_config::credit_material_yield(
                            &mut cohort.stores,
                            &materials_cfg,
                            fauna.hunt_materials_for(&herd.species),
                            take.carried,
                            mult_f,
                        );
                        let tended = provisions.to_f32();
                        // **Extending** a pen (2d-β) re-uses the pen rung's own build dials — a ring
                        // is the same fencing labor at the same forgone-yield price, so it must
                        // never drift from the initial build.
                        //
                        // **THE RING IS RAISED BY THE BAND'S BUILDERS, AND ONLY AT THE HEAD OF THE
                        // QUEUE.** A ring is fencing work on the same `animal:pen` rung as the pen it
                        // widens, so it is funded from the same pool as every other build
                        // (`docs/plan_standing_upkeep.md` §2.5) — otherwise widening a fence would be
                        // the one build in the game that costs nothing, which is exactly what it
                        // became when the investment dip retired.
                        //
                        // **It is the one entry kind that names no rung verb.** A built pen carries
                        // no meter for the derived verb to name, so `extend_pen` queues
                        // [`BuildJob::ExtendPen`] and this arm reads `ring_workers` rather than the
                        // rung arms' `build_workers`. `pen_extending` stays the ring's own in-flight
                        // flag.
                        //
                        // **No floor term** — a build crew is not pulling on the herd; see
                        // [`RungDef::build_accrual`].
                        //
                        // **It is resolved HERE rather than hoisted out of the band loop, and that
                        // is what issue #515 changed.** The rate reads the crew's own handling gear
                        // now, so it is no longer one number for the whole world — a keeper who
                        // brought hurdles raises a ring faster than one who did not, exactly as they
                        // build the original pen faster.
                        //
                        // **`pen_extending` IS THE RING'S WHOLE GATE**, and it is passed as the
                        // rung's `eligible` rather than checked beside it, so the accrual and the
                        // quote below cannot come to disagree about whether a ring is running.
                        //
                        // **⛔ AND THE MATERIAL STORE SCALES IT, exactly as it scales a rung's**
                        // (`docs/plan_standing_upkeep.md` §2.7). A ring bid for `animal:pen`'s own
                        // build pile in the settlement above ([`head_ring_leg`]), so a store short of
                        // hurdles stalls a widening fence in proportion and a dry one blocks it —
                        // widening a pen costs the same panels as raising the pen it widens.
                        let ring_in_flight = herd.pen_extending;
                        let pen_extend_accrual = pen_rung.build_accrual(
                            Some(Improvement::Corral),
                            ring_in_flight,
                            ring_workers,
                            entry_gear.work_per_worker,
                        ) * build_coverage;
                        // A ring costs what the pen it widens costs — the same rung record, so the
                        // two can never drift — and the same keepers' tools raise both at the same
                        // rate.
                        let ring_cost = pen_rung
                            .build_cost(RUNG_COST_UNSCALED)
                            .expect("the pen rung has a build meter");
                        // **The countdown's signed twin, at the FULL POOL** — the Corral arm's rule,
                        // on the ring's own gate. Recorded below so `publish_build_chain` can date
                        // the ring like any other entry.
                        let ring_balance = pen_rung.build_balance(
                            Some(Improvement::Corral),
                            ring_in_flight,
                            builders,
                            entry_gear.work_per_worker,
                            meter_rot,
                            entry_material_coverage,
                        );
                        // Accrue the extension ring **after** the take (mirroring `accrue_corral`), so
                        // this turn pays exactly the dipped yield the forecast promised; the completed
                        // larger footprint's higher K arrives on the next `advance_herds`.
                        // **A ring wears the gear like the pen it widens** — same rung, same work,
                        // so the same charge. Gated on `pen_extending` so a keeper merely tending a
                        // finished pen spends nothing on a build that is not running.
                        //
                        // **The one arm that bills the OFFERED accrual rather than its meter's
                        // delta**, and deliberately: a ring is only ever raised around a herd the
                        // faction already owns, so there is no owner-lock here to refuse the offer
                        // and turn it into a phantom charge; and `accrue_pen_extension` *resets*
                        // `pen_extend_progress` to zero when the ring completes, so a before/after
                        // delta would read negative on exactly the turn the crew worked hardest.
                        if ring_in_flight {
                            charge_build_wear(
                                band_equipment.as_deref_mut(),
                                &equipment_cfg,
                                &entry_gear.wear_kit,
                                pen_extend_accrual,
                            );
                        }
                        let ring_finished = ring_in_flight
                            && herd.accrue_pen_extension(
                                pen_extend_accrual,
                                ring_cost,
                                husbandry.pen_radius_max,
                            );
                        // **A RING IS AN ORDINARY BUILD AND IS DATED LIKE ONE** — recorded for the
                        // band's chain pass exactly as the four rung arms record theirs.
                        //
                        // **Without this the ring was the one queue entry with no quote**, so
                        // `publish_build_chain`'s `None` arm minted [`crate::intensification::BuildTurns::Blocked`] for a
                        // ring that was accruing perfectly normally — and `carried` then handed that
                        // `-4` to **every other source the band works**. `extend_pen` is a one-click
                        // shipped button, so that was ordinary play, not an edge.
                        //
                        // **The meter is read as the ring left it, not as the reset left it.**
                        // `accrue_pen_extension` resets `pen_extend_progress` to `RUNG_UNSTARTED`
                        // on the turn the ring completes, so quoting the live field there would
                        // publish the span of a whole *new* ring on the very turn the old one
                        // finished. Reading the cost it just cleared makes the completing turn say
                        // what the Corral arm's does: there is nothing left to wait for.
                        let ring_banked = if ring_finished {
                            ring_cost
                        } else {
                            herd.pen_extend_progress
                        };
                        if ring_queued {
                            build_quotes.push((
                                BuildSource::Herd(herd.id.clone()),
                                BuildQuote {
                                    cost: ring_cost,
                                    banked: ring_banked,
                                    // **THE COVERAGE THE SETTLEMENT HANDED THE RING** — a ring
                                    // bids for `animal:pen`'s pile like any other build of that
                                    // rung, so a store that cannot cover the panels publishes
                                    // [`BuildGate::Materials`] here through
                                    // [`crate::intensification::BuildQuote::blocking_gate`] rather
                                    // than a confident countdown for a build nothing is feeding.
                                    material_coverage: entry_material_coverage,
                                    // **The animal web still carries per-rung meters**, so an entry
                                    // there is one rung and lays no legs — `work_remaining` falls
                                    // back to this meter's own remainder. Legs arrive on this web
                                    // when it moves onto one position.
                                    legs: Vec::new(),
                                    balance: ring_balance,
                                    // **The ring's whole gate is its in-flight flag**, so a
                                    // queued ring that is not running publishes that as its cause.
                                    gate: if ring_in_flight {
                                        crate::intensification::BuildGate::Open
                                    } else {
                                        BuildGate::RingIdle
                                    },
                                },
                            ));
                        }
                        if ring_finished {
                            completed
                                .push((BuildSource::Herd(herd.id.clone()), BuildJob::ExtendPen));
                            let pen_tile = herd.corralled_at.unwrap_or_else(|| herd.position());
                            event_log.push(CommandEventEntry::new(
                                tick.0,
                                CommandEventKind::Corral,
                                faction,
                                format!(
                                    "Extended the pen for {} to radius {}",
                                    fauna_id, herd.pen_radius
                                ),
                                Some(format!(
                                    "status=extended action=extend_pen herd={} radius={} x={} y={}",
                                    fauna_id, herd.pen_radius, pen_tile.x, pen_tile.y
                                )),
                            ));
                        }
                        // A *managed* harvest never overdraws — it takes at most the escapement MSY —
                        // so `sustainable == actual` (no overdraw ⚠). The two staffing signals are
                        // derived like every other rung's: how many keepers the take really needed,
                        // and how much of the harvest went uncollected for want of hands. **`wasted`
                        // is measured against the animals SLAUGHTERED, not against the pen's offered
                        // escapement** (slice 8): a beast the keeper never killed is still standing in
                        // the pen, alive and breeding — it was never produced, so it cannot have been
                        // wasted. What `killed_biomass − carried` measures is meat that really rotted.
                        // **The arrival schedule — computed POST-take, unlike `realized`.** It
                        // answers "when does the next food land", so it must start from the state the
                        // turn leaves behind: projecting from the pre-take state would re-promise the
                        // delivery this turn has already paid. Slot 0 is therefore genuinely the
                        // *next* turn's delivery.
                        let arrivals = fauna::project_arrivals_hunt(
                            herd,
                            &fauna,
                            herd_carry_per_worker,
                            &party_for(herd.body_mass),
                            mult_f,
                            workers,
                            *floor,
                            arrivals_horizon,
                        );
                        yields[idx] = SourceYield {
                            actual: tended,
                            // No animal pays fodder, so this arm credits the `FODDER` store nothing
                            // and the row reports the same nothing (see [`SourceYield::fodder`]).
                            fodder: 0.0,
                            // **The credited materials, not a recomputation** — exactly what
                            // `credit_material_yield` deposited. On a wolf this is the whole of what
                            // the hunt paid, which is why the row cannot be food-only.
                            materials: credited_materials,
                            sustainable: tended,
                            // The forward-projected steady headline (computed pre-take above; a pen
                            // projects its managed yield, already smooth).
                            realized: hunt_realized.provisions,
                            arrivals,
                            // Resolved: a fact, so the band is a point — the same reading the
                            // range arm's row carries. (It said *"a pen has no stochastic stage at
                            // all"* until §4.9 item 12b; a pen retreats and fights now, so its
                            // **quote** carries a real distribution like any other hunting row.)
                            range: YieldRange::certain(tended),
                            wasted: pen_yield.apply(take.wasted, mult_f).provisions,
                            // **THE SAME THREE-UNIT CREW THE RANGE ROW SIZES**
                            // ([`fauna::hunt_take_workers`]): hands enough to *reach* the drop and to
                            // *carry* it, off the pen's per-turn `production` rather than this turn's
                            // lumpy `take.carried` — a slow-breeding pen (the aurochs pulses) drops 0
                            // animals on a wait turn, which would collapse the crew and contradict
                            // `wasted`.
                            //
                            // **The engagement term is a real one here** — a pen has a reach
                            // (`herd_engage_rate`, the keepers' handling) and a retreat
                            // (`herd_wariness`, the fence's calm), so the crew inverts the bound the
                            // take was actually paid at. It was `hunt_haul_workers` while the pen ran
                            // neither stage, and `forecast_source_yield` — the assign-time seed for
                            // this very row — has always used `hunt_take_workers`, so the two
                            // disagreed the moment the pen started retreating.
                            workers_needed: fauna::hunt_take_workers(
                                production,
                                herd.body_mass,
                                herd_carry_per_worker,
                                fauna::herd_engage_rate(herd, &fauna),
                                party_for(herd.body_mass)
                                    .stay_fraction(fauna::herd_wariness(herd, &fauna)),
                            ),
                            overdraws: false,
                        };
                        // **THE BAND'S SIDE OF THE SAME FIGHT** — a contained bull still gores. The
                        // pen resolves the ordinary fight now, so its casualties land exactly where
                        // the range arm's do, through the one seam both call.
                        settle_hunt_band_side(
                            &outcome,
                            &herd.species,
                            &fauna,
                            tick.0,
                            faction,
                            &mut cohort,
                            &mut event_log,
                        );
                        continue;
                    }
                    // Take food via the shared primitive: the per-policy escapement ceiling, rounded
                    // to **whole animals** against the crew's collection (slice 8). It hands back the
                    // kill in biomass — killed / carried / wasted — and has already drawn every animal
                    // killed off the herd.
                    let biomass_before = herd.biomass;
                    // **The escapement room, resolved PRE-take** — the stock standing above this
                    // assignment's floor, in biomass and before the whole-animal quantiser. Two
                    // readers below: the work predicate ([`crew_is_working_the_source`], which
                    // replaced this arm's `EcologyPhase::Thriving` gate) and the crew the telemetry
                    // row sizes off what the herd offered.
                    let standing_above_floor = fauna::hunt_escapement_ceiling(
                        *floor,
                        biomass_before,
                        herd_capacity(herd, &fauna),
                    );
                    // **THE LESSON'S GATE — the pure escapement room, deliberately unwidened.**
                    // `learn_multiplier`'s self-limit lives here: *"watching teaches nothing"* at
                    // `floor = 1.0` is what stops a near-`1.0` floor farming knowledge at x2 for
                    // free, and it holds only because the source must stand above the floor.
                    let working_the_herd = crew_is_working_the_source(standing_above_floor);
                    // **THE BUILD'S GATE — what the take will actually pay.** A herd pushed below
                    // its floor by the `K` its own taming raised is still a herd, still growing, and
                    // still a legal thing to gentle. Same number `hunt_take` is bounded by, so a
                    // legal build target that yields nothing is unrepresentable.
                    let herd_is_workable =
                        source_is_workable(fauna::herd_take_room(herd, *floor, &fauna));
                    // The band has no carry room — it eats/banks whatever it hauls, so pass an
                    // unbounded carry cap (behaviour unchanged from before the expedition clamp).
                    let outcome = hunt_take(
                        herd,
                        workers,
                        *floor,
                        herd_carry_per_worker,
                        &party_for(herd.body_mass),
                        &fauna,
                        f32::INFINITY,
                        fauna::HuntDraw::Seeded(fauna::retreat_seed(
                            sim_config.map_seed,
                            tick.0,
                            &herd.id,
                            workers,
                        )),
                    );
                    let take = outcome.take;
                    // **BOTH KITS ARE CHARGED FOR USE, AND ONLY FOR USE** (the minimal TOE,
                    // `docs/plan_denial_raid.md` §1.2). The hunting kit pays per **animal killed**
                    // and the carry kit per **biomass carried home**, so a party that waits out a
                    // herd too thin to spare a body — or that marches all turn without engaging —
                    // spends nothing. A turn-based clock would charge an idle march the same as a
                    // slaughter, which is exactly what would make denial free.
                    //
                    // Charged AFTER the take, so this turn's take is paid at the tier the take was
                    // priced with and the cliff lands on the *next* turn. That is the same
                    // accrue-after-take ordering every rung's build meter uses.
                    //
                    // **Each charge is gated on the predicate that chose its own tier**, and the two
                    // are independent: a kit with spears but no sled blunts spears only.
                    if let Some(kit) = band_equipment.as_mut() {
                        // **The WEAPON is charged PER CREW, for the blows it landed** — the run
                        // that could not clear the quarry's defence swung at nothing and pays
                        // nothing, and the run holding no weapon has nothing in its kit to charge.
                        outcome.fight.charge_strike_wear(kit, &equipment_cfg);
                        kit.wear_kit(
                            &equipment_cfg,
                            &crew_kit,
                            crate::equipment_config::WearQuantum::BiomassHauled,
                            take.carried,
                        );
                    }
                    // **THE earn path, rungs 1–2** — the drawn-down half of the split above, and the
                    // heart of the ladder: the same hunt teaches **Herding** on a wild herd and
                    // **Penning** on a tamed one. The gate is the **escapement room**, never
                    // `take.killed > 0`: a herd whose room is lighter than one body hands over
                    // nothing this turn while the crew tracks and handles it exactly as before, and
                    // reading that as *"not working"* would pace the whole ladder off `body_mass`.
                    // See [`crew_is_working_the_source`]. It replaced the `EcologyPhase::Thriving`
                    // gate this site used to carry, and it is what makes `floor = 1.0` (leave the
                    // whole herd standing, learn at ×2) honestly earn nothing.
                    credit_rung_lesson(
                        lesson_rung,
                        *floor,
                        take_crew_present && working_the_herd,
                        knowledge_dials,
                        faction,
                        &mut discovery,
                    );
                    // **THE take's yield: product × intensity** (`docs/plan_hunt_yield_model.md`).
                    // `hunt_take` above decided HOW MUCH biomass came home (the policy's job); the
                    // species' `HuntYield` decides WHAT that biomass is worth, in one call that
                    // yields both products so neither can be converted without the other.
                    let hunt_yield = herd_hunt_yield(herd, &fauna);
                    let paid = hunt_yield.apply(take.carried, mult_f);
                    let provisions = scalar_from_f32(paid.provisions);
                    // **Tame — the investment** (the animal twin of Cultivate, and the rung
                    // below Corral). The crew is gentling the herd, not hunting it: `hunt_take`
                    // above already paid only the reduced Tame ceiling (the `animal:pastoral` rung's
                    // `yield_fraction_while_building × MSY` — the up-front cost), and here the herd
                    // accrues toward pastoral. Gates: the faction must **know Herding** (earned by
                    // hunting, above), the species' husbandry ceiling must allow taming
                    // (Grazing 2d-δ — a `wild`-ceiling species never tames; `accrue_domestication`
                    // self-guards too, and the command path rejects it, so this is belt and braces),
                    // and the herd must be **standing above the crew's floor**
                    // ([`crew_is_working_the_source`] — not "an animal died", which is a
                    // quantisation fact rather than a fact about work).
                    //
                    // **There is no health gate any more** (`docs/plan_harvest_floor.md` §3.2), the
                    // plant side's change applied to the identical `EcologyPhase::Thriving` condition
                    // here. The floor replaced the cliff with a rate: gentling a herd you are pulling
                    // hard on is *slow*, not *stopped*, so there is no lapse state left to hold
                    // progress across. `validate_tame` never had a phase gate, so the command side
                    // was already consistent with removing it.
                    //
                    // **Ownership is NOT in `eligible`** — `accrue_domestication` owns the
                    // `owner is None || owner == faction` rule (and sets ownership on first accrual),
                    // exactly as `accrue_cultivation` owns it on the plant side. One rule, one place.
                    //
                    // **Ordering: accrue AFTER the take** (mirrors Cultivate/Corral), so this turn
                    // pays exactly the dipped yield the pre-commit forecast promised.
                    if improvement == Some(Improvement::Tame) {
                        // Marked worked-as-improvement so `advance_husbandry` spares it: a herd
                        // under active taming neither goes feral nor bleeds its partial progress.
                        herd.tamed_this_turn = true;
                        // Stated as terms rather than a `&&` chain, so the refusing conjunct
                        // reaches the wire — see the Cultivate arm. **`Escapement` is the one the
                        // playtest sat on**: the hunters draw the flock to their floor, the unmet
                        // keeping suppresses its regrowth, and nothing on the build line reopens it.
                        let gate = animal_pastoral_gate(
                            pastoral_rung.unlock_discovery_id().is_none_or(|knowledge| {
                                knows(&discovery, faction, knowledge, knowledge_threshold)
                            }),
                            herd.can_domesticate(),
                            herd_is_workable,
                        );
                        let eligible = gate.holds();
                        // THE build seam — the same call the plant side's Cultivate arm makes, and it
                        // is **species-blind**: the crew banks `workers × PER_WORKER_OUTPUT ×
                        // learn_multiplier(floor)` work units whatever animal it is gentling. What
                        // varies per species is the **price** of the job, not the crew's rate —
                        // `taming_cost_multiplier` below (`docs/plan_unit_costed_work.md` §3.1
                        // inverted the retired `taming_rate` timescale, which said *your people are
                        // five times worse at this animal*).
                        //
                        // **The crew is the TAME's own**, and the floor is not a term: a gentling
                        // crew is not pulling on the herd, so there is no pressure of theirs to read
                        // (`docs/plan_standing_upkeep.md` §2.2).
                        // The crew's whole output: the keeping pool owes this herd's rate at any
                        // meter fullness, so a `Tame` banks `work_cost / crew` — the animal web
                        // answers exactly as the plant web does, with no exception (§4.6a).
                        let accrual = pastoral_rung.build_accrual(
                            improvement,
                            eligible,
                            build_workers,
                            entry_gear.work_per_worker,
                        ) * build_coverage;
                        // **The countdown's signed twin** — the Cultivate arm's rule, net of the
                        // meter's rot, which on the animal web is always `0` (no `meter_decay`: an
                        // under-kept flock sheds animals instead). At the **full pool**, like every
                        // entry's quote.
                        let balance = pastoral_rung.build_balance(
                            improvement,
                            eligible,
                            builders,
                            entry_gear.work_per_worker,
                            meter_rot,
                            entry_material_coverage,
                        );
                        // **THE JOB'S PRICE** — the rung's `work_cost` times this species' own
                        // `taming_cost_multiplier` (slice 3c inverted): the rung owns the mechanic,
                        // the species prices it. A Steppe Runner is five times the work, not a crew
                        // five times worse at their job.
                        // **The species' own price multiplier**, which the herd stamps so it can
                        // place its own rung boundaries; the work-unit `tame_cost` beside it is what
                        // the build quote is denominated in.
                        let tame_multiplier = fauna.taming_cost_multiplier_for(&herd.species);
                        let tame_cost = pastoral_rung
                            .build_cost(fauna.taming_cost_multiplier_for(&herd.species))
                            .expect("a rung a verb builds has a build meter");
                        // **The handling gear's whole point** (issue #515, as re-cut by §4.8):
                        // hurdles, halters and a butchering stone are animal-handling tools, and a
                        // `Tame` is exactly the turns a band spends handling animals. Each equipped
                        // keeper *delivers* its worth on top of their own hands — it is in the
                        // `accrual` and the `balance` above, and the job below is untouched by it.
                        // The TRANSITION, not the state (the Cultivate arm's rule): a second band
                        // taming the same herd clears its verb via the already-built check above
                        // without re-announcing the taming.
                        //
                        // **The gear is charged off the METER'S DELTA.** Ownership lives in
                        // `accrue_domestication` and deliberately not in `eligible`, so a band whose
                        // `Tame` outlived another faction claiming the herd computes a positive
                        // `accrual` every turn while the herd refuses all of it — billing the
                        // offered amount would bleed its gear dry against a meter that never moves.
                        // Its verb is never cleared either (`hunt_rung_already_built` reads
                        // `is_domesticated`), so it would bleed forever.
                        let progress_before = herd.ladder_position();
                        let tamed = accrual > 0.0
                            && herd.accrue_domestication(
                                faction,
                                accrual,
                                tame_multiplier,
                                &ladder,
                            );
                        // Recorded for the band's chain pass, which dates it at its place in the
                        // queue — see the Cultivate arm.
                        // **Only for a source that carries an entry** — see
                        // `entry_declares_a_rung`.
                        if entry_declares_a_rung {
                            build_quotes.push((
                                BuildSource::Herd(herd.id.clone()),
                                BuildQuote {
                                    cost: tame_cost,
                                    banked: herd.rung_work_done(RungKey::AnimalPastoral, &ladder),
                                    // The animal web still carries per-rung meters — see the ring's
                                    // note above.
                                    legs: Vec::new(),
                                    balance,
                                    gate,
                                    material_coverage: entry_material_coverage,
                                },
                            ));
                        }
                        charge_build_wear(
                            band_equipment.as_deref_mut(),
                            &equipment_cfg,
                            &entry_gear.wear_kit,
                            herd.ladder_position() - progress_before,
                        );
                        if tamed {
                            completed.push((
                                BuildSource::Herd(herd.id.clone()),
                                BuildJob::Rung(Improvement::Tame),
                            ));
                            event_log.push(CommandEventEntry::new(
                                tick.0,
                                CommandEventKind::Tame,
                                faction,
                                format!("Tamed the {} herd", herd.species),
                                Some(format!("status=complete action=tame herd={}", herd.id)),
                            ));
                        }
                    }
                    // **Corral — the investment** (the animal twin of Cultivate). The crew is
                    // building the pen, not hunting: `hunt_take` above already paid only the reduced
                    // Corral ceiling (the rung's `yield_fraction_while_building × MSY` — the up-front
                    // cost), and here the pen accrues. Gates: the faction must **know Penning** (the
                    // rung's own `unlock_knowledge` — Herding gates `tame` alone since §4.3) and **own a
                    // domesticated herd**. A gate that lapses mid-build just stops accrual that turn
                    // (progress is kept — a half-built pen is materials on the ground). Accrued
                    // **after** the take, so this turn pays exactly what the pre-commit forecast
                    // promised; the corral yield starts the turn after the pen completes.
                    if improvement == Some(Improvement::Corral) {
                        // The rung's own gates, resolved for the engine: the faction knows the rung's
                        // unlock knowledge (Herding today), the species' husbandry ceiling reaches
                        // this rung (Grazing 2d-δ: only a `Pen`-ceiling species may build a pen — a
                        // `Wild`/`Pastoral` herd never accrues, and the command path rejects it too,
                        // so this is belt and braces), the herd has climbed the rung below, and the
                        // faction owns it.
                        // Stated as terms rather than a `&&` chain — see the Cultivate arm.
                        let gate = animal_pen_gate(
                            pen_rung.unlock_discovery_id().is_none_or(|knowledge| {
                                knows(&discovery, faction, knowledge, knowledge_threshold)
                            }),
                            herd.can_pen(),
                            herd.is_domesticated(),
                            herd.owner == Some(faction),
                        );
                        let eligible = gate.holds();
                        // THE build seam — the same call the plant side's Cultivate arm makes.
                        // Penning is a flat build for every species — only *taming* varies (slice
                        // 3c): a fence is a fence. **The crew is the CORRAL's own.**
                        //
                        // **The work predicate is deliberately NOT in `eligible` here**, for
                        // `accrue_field`'s reason (see there): it replaced a rung's
                        // `EcologyPhase::Thriving` gate, and rung 3 never had one on either web.
                        // Fencing a herd is ground work — a pen goes up around a flock already drawn
                        // down to its keeper's own floor.
                        let accrual = pen_rung.build_accrual(
                            improvement,
                            eligible,
                            build_workers,
                            entry_gear.work_per_worker,
                        ) * build_coverage;
                        // **The countdown's signed twin** — the Cultivate arm's rule, at the full
                        // pool.
                        let balance = pen_rung.build_balance(
                            improvement,
                            eligible,
                            builders,
                            entry_gear.work_per_worker,
                            meter_rot,
                            entry_material_coverage,
                        );
                        // Penning is a flat job for every species — a fence is a fence — so the pen
                        // takes no per-species multiplier; only *taming* varies.
                        let pen_cost = pen_rung
                            .build_cost(RUNG_COST_UNSCALED)
                            .expect("a rung a verb builds has a build meter");
                        // **Charged off the pen meter's own delta** (the Tame arm's rule): the
                        // owner-lock lives in `accrue_corral`, so a keeper the herd refuses spends
                        // nothing structurally rather than by this site re-checking the gate.
                        let pen_tile = herd.position();
                        let progress_before = herd.ladder_position();
                        let penned = accrual > 0.0
                            && herd.accrue_corral(faction, accrual, &ladder, pen_tile);
                        // **Only for a source that carries an entry** — see
                        // `entry_declares_a_rung`.
                        if entry_declares_a_rung {
                            build_quotes.push((
                                BuildSource::Herd(herd.id.clone()),
                                BuildQuote {
                                    cost: pen_cost,
                                    banked: herd.rung_work_done(RungKey::AnimalPen, &ladder),
                                    // The animal web still carries per-rung meters — see the ring's
                                    // note above.
                                    legs: Vec::new(),
                                    balance,
                                    gate,
                                    material_coverage: entry_material_coverage,
                                },
                            ));
                        }
                        charge_build_wear(
                            band_equipment.as_deref_mut(),
                            &equipment_cfg,
                            &entry_gear.wear_kit,
                            herd.ladder_position() - progress_before,
                        );
                        if penned {
                            completed.push((
                                BuildSource::Herd(herd.id.clone()),
                                BuildJob::Rung(Improvement::Corral),
                            ));
                            event_log.push(CommandEventEntry::new(
                                tick.0,
                                CommandEventKind::Corral,
                                faction,
                                format!(
                                    "Corralled {} at ({}, {})",
                                    fauna_id, pen_tile.x, pen_tile.y
                                ),
                                Some(format!(
                                    "status=complete action=corral herd={} x={} y={}",
                                    fauna_id, pen_tile.x, pen_tile.y
                                )),
                            ));
                        }
                    }
                    // **THE PROJECTION — the animal twin of the Forage arm's** (see there for why a
                    // `-1` on an unstarted source is the defect, and `penUpkeep`'s precedent). Quoted
                    // against `herd_rung_key(..).above()` at this crew, floor and kit, from the work
                    // already banked on that rung; `None` at the top of the ladder (a penned herd has
                    // nothing left to build — and never reaches here, the tend branch returns first).
                    if improvement.is_none() {
                        let projected = fauna::herd_rung_key(herd).above().and_then(|next_key| {
                            let next = ladder.rung(next_key);
                            // Each rung's own gates, as `validate_tame` / `validate_corral` state
                            // them — including the ownership terms the live arms leave to
                            // `accrue_domestication` / `accrue_corral`, because a quote for a herd
                            // another people are taming is a job this faction cannot take.
                            let (gated, banked, cost_multiplier) = match next.verb_improvement() {
                                Some(Improvement::Corral) => (
                                    BuildGate::first_refusal(&[
                                        (herd.can_pen(), BuildGate::SpeciesCeiling),
                                        (herd.is_domesticated(), BuildGate::RungBelow),
                                        (herd.owner == Some(faction), BuildGate::OwnedByOther),
                                    ]),
                                    herd.rung_work_done(RungKey::AnimalPen, &ladder),
                                    // Penning is a flat job for every species — a fence is a
                                    // fence; only taming varies.
                                    RUNG_COST_UNSCALED,
                                ),
                                _ => (
                                    BuildGate::first_refusal(&[
                                        (herd.can_domesticate(), BuildGate::SpeciesCeiling),
                                        // **The build's own question**, so the wire's blocked reason
                                        // and the accrual's gate cannot disagree about whether this
                                        // herd is workable.
                                        (herd_is_workable, BuildGate::Escapement),
                                        (
                                            herd.owner.is_none_or(|owner| owner == faction),
                                            BuildGate::OwnedByOther,
                                        ),
                                    ]),
                                    herd.rung_work_done(RungKey::AnimalPastoral, &ladder),
                                    fauna.taming_cost_multiplier_for(&herd.species),
                                ),
                            };
                            // The knowledge term is common to both rungs and is asked **after** the
                            // per-rung ones, which is the order the retired `&&` chain evaluated.
                            let gate = BuildGate::first_refusal(&[
                                (gated.holds(), gated),
                                (
                                    next.unlock_discovery_id().is_none_or(|knowledge| {
                                        knows(&discovery, faction, knowledge, knowledge_threshold)
                                    }),
                                    BuildGate::Knowledge,
                                ),
                            ]);
                            ladder.projected_build_quote(
                                next,
                                cost_multiplier,
                                banked,
                                builders,
                                entry_gear.work_per_worker,
                                gate,
                                // The source's live bleed — always `0` on the animal web, whose
                                // rungs declare no `meter_decay`. See the Forage arm.
                                meter_rot,
                            )
                        });
                        if let Some(quote) = projected {
                            build_quotes.push((BuildSource::Herd(herd.id.clone()), quote));
                        }
                    }
                    if provisions > scalar_zero() {
                        cohort.stores.add(FOOD, provisions);
                    }
                    // **The MATERIAL account of the same take** (`docs/plan_crafting_and_materials.md`
                    // §2) — hide, sinew and bone, off the meat **carried home** exactly as the two
                    // accounts above are, so a party that killed a mammoth and hauled a leg of it
                    // brings back a leg's worth of hide. A take that hauls nothing home yields none.
                    let credited_materials = crate::materials_config::credit_material_yield(
                        &mut cohort.stores,
                        &materials_cfg,
                        fauna.hunt_materials_for(&herd.species),
                        take.carried,
                        mult_f,
                    );
                    // **The LONG-RUN sustainable rate** — one turn's net regrowth at the herd's
                    // **pre-take** biomass (the herd's OWN ecology/capacity: a tamed herd grows 1.5×
                    // faster, so its sustainable skim is 1.5× a wild one's).
                    //
                    // Since slice 8 this is deliberately **not** comparable to `actual` turn by turn:
                    // a whole-animal take pays in lumps (nothing for 6 turns, then a whole mammoth),
                    // so `actual` swings around this rate rather than tracking it. That swing is
                    // *true* and it is the mechanic — so `sustainable` keeps reporting the honest
                    // average ("this herd sustains ~0.78/turn"), and whether the take **overdraws** is
                    // answered by the policy's own floor (`overdraws` below) instead of by comparing
                    // the two. See `SourceYield`.
                    let sustainable = hunt_yield
                        .apply(
                            sustainable_yield(
                                biomass_before,
                                herd_capacity(herd, &fauna),
                                &herd_ecology(herd, &fauna),
                            ),
                            mult_f,
                        )
                        .provisions;
                    // The two staffing signals, from the same take. **Overstaffing**: invert the
                    // carried biomass by the per-hunter throughput (hunt has no seasonal factor,
                    // unlike forage). **Understaffing** (`wasted`): the meat the crew killed but could
                    // not haul — **a real loss**, left to rot on the range. Measured against the
                    // animals *slaughtered*, never against the escapement the herd could have spared:
                    // an animal nobody killed is still alive out there, so it was never produced and
                    // cannot have been wasted (`fauna::forecast_production_and_take`).
                    //
                    // **A MANAGED herd reports its whole CREW** ([`source_crew_needed`]) — the
                    // herders who mind it are the ones who take from it, and the crew must be big
                    // enough for both jobs. A **wild** herd is untouched by the herder term:
                    // `herders_needed` is `0` (it isn't yours to maintain), so the `max` collapses to
                    // the haul-side count.
                    //
                    // The take side is [`fauna::hunt_take_workers`] — the crew that can both **reach**
                    // and **carry** the peak animal drop off
                    // the SAME escapement ceiling the take was bounded by — NOT this turn's lumpy
                    // `take.carried`. A slow breeder whose room is lighter than one body carries `0` on
                    // a wait turn, which would collapse `workers_needed` and contradict `wasted_yield`;
                    // sizing off the ceiling keeps the two in agreement and equals the client's
                    // max-useful count. It is re-derived at the **pre-take** biomass, which is what
                    // `hunt_take` read, so the crew describes the take that was just paid.
                    // The **ceiling** is the same pre-take escapement room the work predicate read,
                    // and it is unscaled: the herd offers what stands above the floor whether the
                    // hunters are harvesting it or gentling it. **The THROUGHPUT carries the crew's
                    // throughput**, which is simply the hunters' own: a build on this herd is
                    // staffed in its own right (`docs/plan_standing_upkeep.md` §2.2), so nothing
                    // scales what a hauler carries.
                    //
                    // **The ENGAGEMENT side is the second unit** ([`fauna::hunt_engage_workers`],
                    // `docs/plan_hunt_through_combat.md` §2): a hunter brings down
                    // `engage_rate × stay` animals a turn whatever they can carry, so the crew
                    // that clears the ceiling needs `ceil(peak drop / that)` hands. Sizing on carry
                    // alone reported "more hands would be idle" about small-bodied game — 470 fowl
                    // above the floor is 61 biomass, two haulers' worth, and dozens of hunters' worth
                    // of reach.
                    //
                    // **The retreat rides it, off THIS party's own kit** — `party_for`'s
                    // `dispersion` against the quarry's `wariness`, which is the same
                    // `stay_fraction` `hunt_take` above priced the take with. Reading the species'
                    // bare `1 − wariness` here (or the neutral `1.0`) would size a crew at a
                    // dispersion the take was never resolved at.
                    //
                    // **`herders_needed` no longer folds in.** Keeping a herd and hauling from it are
                    // different jobs in different units; the herder count keeps its own wire field
                    // and this row answers for the take alone.
                    let workers_needed = fauna::hunt_take_workers(
                        standing_above_floor,
                        herd.body_mass,
                        herd_carry_per_worker,
                        fauna.engage_rate_for(&herd.species),
                        party_for(herd.body_mass).stay_fraction(fauna::herd_wariness(herd, &fauna)),
                    );
                    // **The arrival schedule — computed POST-take, unlike `realized`.** It
                    // answers "when does the next food land", so it must start from the state the
                    // turn leaves behind: projecting from the pre-take state would re-promise the
                    // delivery this turn has already paid. Slot 0 is therefore genuinely the
                    // *next* turn's delivery.
                    let arrivals = fauna::project_arrivals_hunt(
                        herd,
                        &fauna,
                        herd_carry_per_worker,
                        &party_for(herd.body_mass),
                        mult_f,
                        workers,
                        *floor,
                        arrivals_horizon,
                    );
                    yields[idx] = SourceYield {
                        actual: provisions.to_f32(),
                        // No animal pays fodder, so this arm credits the `FODDER` store nothing and
                        // the row reports the same nothing (see [`SourceYield::fodder`]).
                        fodder: 0.0,
                        // **The credited materials, not a recomputation** — exactly what
                        // `credit_material_yield` deposited (the discipline `fodder` above carries).
                        materials: credited_materials,
                        sustainable,
                        wasted: hunt_yield.apply(take.wasted, mult_f).provisions,
                        workers_needed,
                        // **The ⚠ — intent AND ability**, through the animal web's one producer
                        // ([`fauna::hunt_take_overdraws`]). A floor below the food peak is only an
                        // overdraw if this party can actually get the herd down to it; four herders
                        // whose take settles the herd above the peak are drawing nothing below what
                        // it sustains, whatever the dial says. Pre-take biomass, matching
                        // `sustainable` beside it.
                        overdraws: fauna::hunt_take_overdraws(
                            herd,
                            &fauna,
                            biomass_before,
                            herd_carry_per_worker,
                            &party_for(herd.body_mass),
                            workers,
                            *floor,
                        ),
                        // The forward-projected steady headline (computed pre-take above): rate-based,
                        // so it is smooth where `actual` (the whole-animal kill) pulses.
                        realized: hunt_realized.provisions,
                        arrivals,
                        // Resolved: a fact, so the band is a point. This is
                        // the row whose *seeded* twin carries a real distribution once `wariness` or
                        // a sub-1 `hit_chance` is authored.
                        range: YieldRange::certain(provisions.to_f32()),
                    };
                    // **The fight already happened — inside the take** (`docs/plan_hunt_through_combat.md`
                    // §0.1), and both rungs settle its band side through the one seam.
                    settle_hunt_band_side(
                        &outcome,
                        &herd.species,
                        &fauna,
                        tick.0,
                        faction,
                        &mut cohort,
                        &mut event_log,
                    );
                }
                LaborTarget::Scout => {
                    // Scouts act as forward observers in `calculate_visibility`: staffed scouts
                    // post vantage points out from the band (`labor.scout.vantage_distance(scouts)`)
                    // and reveal from each, re-marked Active every turn — no work is done here.
                }
                LaborTarget::Agriculture | LaborTarget::Husbandry => {
                    // **The two keeping roles do no per-worker yield here either.** Their hands are
                    // a *pool*, spent by `maintenance_shares` before this loop began and stamped
                    // onto each source's `upkeep_supplied` in the two arms above — so by the time
                    // the loop reaches the role's own row there is nothing left for it to do
                    // (`docs/plan_standing_upkeep.md` §2.5).
                }
                LaborTarget::Roadwork => {
                    // **The third keeping pool, and the one this LOOP does not spend.** A road is
                    // not a source row and has no arm above to stamp — what it funds is resolved from
                    // the roads the band keeps rather than from `assignments` — so the split runs
                    // once per band in [`settle_bands_roadwork`], above this loop and ahead of the
                    // band's `continue`s, and this row is only the head count that call divides.
                }
                LaborTarget::Builders => {
                    // **And neither do the builders**, for the same reason one level over: their
                    // hands are a pool too, resolved before the loop (`builders`) and spent entirely
                    // on the **head** of the band's queue inside whichever source arm that entry
                    // names. By the time the loop reaches the role's own row the work is already
                    // banked.
                }
                LaborTarget::Warrior => {
                    // Still a no-op **in the labor pass** — warriors do no per-worker yield here, and
                    // they are a band-wide standing guard (border/camp patrol), not a hunting escort, so
                    // they do **not** mitigate hunt danger (the hunting party answers that itself, via
                    // its own equipment). But warriors are **no longer inert overall** (Phase 1b): the
                    // warrior head-count is now **consumed by [`advance_predator_raids`]** as the band's
                    // defending contingent when a carnivore raids its camp. Keep this branch.
                }
            }
        }
        // **Stamp the fodder-flow rate onto every pen this band keeps** (Flora Roster F3, §5.3), now
        // that the whole band's hay harvest (`band_fodder_inflow`) is summed. Split evenly across the
        // band's pens so the *total* K contribution reflects the *total* hay grown, not N copies of
        // it. Read next turn by `ecological_carrying_capacity` (the one-turn Logistics-reads-Population
        // lag). **Gated on Foddering** exactly as the feed draw is: a faction that grew hay but has not
        // learned to hay a herd delivers nothing to the pen's ceiling, so `K_pen` stays byte-identical
        // to its footprint-only self — the fodder term is all-or-nothing with the capability, never a
        // free K boost from unusable hay. Always written (0 when un-foddered), so a pen a band stops
        // keeping does not carry a stale rate.
        // **THE BAND'S OWN HAY LEDGER, stamped once the loop has seen every row** — the need its pens
        // carry and the hay they will draw against it (both summed above), against the hay its Fields
        // grew this turn. All three are per-turn **rates** in fodder units.
        //
        // **A band that reaches an early exit above never gets here**, which is why the three are
        // zeroed before those exits rather than only written here: a band whose last worker died
        // sheds every row, leaves through the empty-assignments `continue`, and would otherwise
        // republish last turn's figures forever for pens it no longer keeps.
        //
        // **The inflow is the RAW harvest, not the Foddering-gated share below.** What the pens may
        // *draw* is a capability question; what the Fields *grew* is not, and a band watching its hay
        // arrive is entitled to see it before it has learned what to do with it.
        allocation.last_fodder_need = band_fodder_need;
        allocation.last_fodder_inflow = band_fodder_inflow;
        // **THE STANDING MATERIAL BILL, SUMMED BY THE SIM** (`docs/plan_standing_upkeep.md` §2.7) —
        // the need off the settlement's own per-source bills, the income off what the rows credited.
        //
        // ⛔ **The client cannot sum the need itself**: herd rows are fog-filtered, so a pen out of
        // sight would silently drop out of a total the band certainly still owes — the rule the hay
        // ledger beside it already states.
        for share in &material_settlement.upkeep {
            for (id, amount) in &share.demanded {
                *allocation
                    .last_material_need
                    .entry(id.clone())
                    .or_insert(NOTHING_DEMANDED) += *amount;
            }
        }
        // **Reported, never recomputed** — the amounts `credit_material_yield` actually deposited,
        // which is the discipline `SourceYield::materials` already carries. This is only **half** the
        // band's inflow: a bench is not a source row and deposits nothing until its meter crosses, so
        // its forward rate joins here through `LaborAllocation::material_income` — the one producer
        // the wire row and the shortfall Alert both read.
        for row in &yields {
            for payoff in &row.materials {
                *allocation
                    .last_material_income
                    .entry(payoff.material.clone())
                    .or_insert(NOTHING_DEMANDED) += payoff.amount;
            }
        }
        allocation.last_fodder_drain = band_fodder_drain;
        // **THE TWO NOTIFICATIONS THE MATERIAL HALF OWES THE PLAYER** — a kit item crossing a
        // `life_readout` seam, and a good the standing bills eat faster than it arrives
        // (`docs/plan_standing_upkeep.md` §4.9 item 12). Both are pushed here, after the turn's wear
        // and after the bill is summed, so each reads the state it is about.
        announce_kit_life(
            &mut event_log,
            tick.0,
            faction,
            band_id,
            &equipment_cfg,
            &kit_life_before,
            &kit_life_fractions(&equipment_cfg, band_equipment.as_deref()),
        );
        // **THE WHOLE INFLOW, THROUGH THE WIRE ROW'S OWN PRODUCER** — the credited take plus what
        // this band's bench will bank. `hurdles` have no producer but a bench on the shipped roster,
        // so an Alert struck on the take alone fires for every band that keeps a pen.
        let band_material_income =
            allocation.material_income(&crate::systems::bench_material_rate(
                bench.as_deref(),
                &cohort.stores,
                &recipes_cfg,
                &materials_cfg,
                &equipment_cfg,
                band_equipment.as_deref().unwrap_or(&no_kit_at_all),
            ));
        announce_material_shortfall(
            &mut event_log,
            tick.0,
            faction,
            band_id,
            &mut allocation,
            &cohort.stores,
            &band_material_income,
        );
        if !kept_pens.is_empty() {
            let per_pen = if knows(
                &discovery,
                faction,
                FODDERING_DISCOVERY_ID,
                knowledge_threshold,
            ) {
                band_fodder_inflow / kept_pens.len() as f32
            } else {
                0.0
            };
            for fauna_id in &kept_pens {
                if let Some(herd) = registry.herds.iter_mut().find(|herd| &herd.id == fauna_id) {
                    herd.fodder_delivery_rate = per_pen;
                }
            }
        }
        // ## ⛔ THE ROAD AT THE HEAD OF THE QUEUE — RAISED BY THE BAND'S BUILDERS, NOT BY TRAFFIC
        //
        // **`grade` and `pave` are ordinary crew builds** (`docs/plan_standing_upkeep.md` §4.13a
        // rule 2): traffic wears the free floor in and stops there, and everything above it is a
        // decision somebody typed and a pool somebody staffed.
        //
        // **It is its own arm rather than a third case in the assignment loop, and the reason is
        // structural: A ROAD HAS NO LABOR ROW.** That loop visits `assignments`, and every source it
        // can reach is named by one — a road is named by its **keeper** instead, which is why
        // `LaborAllocation::holds_build_source` exempts it from the prune. So the head is read
        // directly here, after the loop, at exactly the point the other four verbs' completions are
        // collected.
        //
        // **All hands on the head**, the same rule as everywhere else (§2.5): the entry banks the
        // whole `builders` pool at its own kit, or nothing.
        //
        // ⛔ **AND IT BANKS ONLY WHAT THE KEEPER OWNS.** A band whose `grade` has been superseded —
        // the tile decayed back into the free floor and lost its keeper ([`BuildGate::NoKeeper`]),
        // or another band adopted it ([`BuildGate::OwnedByOther`]) — banks nothing, because the road
        // is no longer that band's job. **The entry itself is
        // dropped by the next turn's prune**, which asks [`band_keeps_road`]: the keeper *is* a road
        // entry's membership, so losing it retires the entry exactly as a vanished row retires a
        // patch's. Nothing is second-guessed here.
        //
        // ## ⛔ AND EVERY ROAD ENTRY RECORDS A `BuildQuote`, HEAD OR NOT
        //
        // A road pushed none until this slice, and the cost was not confined to roads. A head with
        // no quote and a staffed pool is minted [`crate::intensification::BuildTurns::Blocked`] by
        // [`publish_build_chain`] with `blocked_reason(None)` — [`BuildGate::Unworked`], *a block
        // with no cause* — and `carried` then hands that same `-4` to **every entry behind it and
        // every unqueued source the band works**. So a band that typed `grade` and staffed its
        // builders published *"⚠ Blocked"* on its patches and its herds, for a road that was
        // building perfectly well. It is the same hole the pen ring fell into and it is closed the
        // same way: the entry kind records a quote.
        //
        // The walk is over the **whole queue** rather than the head alone, because a waiting entry
        // is dated too — at the full pool and [`FULLY_SERVED`], the convention every other waiting
        // entry is quoted under.
        for entry in &build_queue {
            let (BuildSource::Road(tile), BuildJob::Rung(improvement)) =
                (&entry.source, entry.declared)
            else {
                continue;
            };
            let destination = RungKey::built_by(improvement);
            // **The gate, through the one seam the claim side asked it through** — so a head the
            // keeping split refused to fund cannot quietly bank work here, and a head it *did* fund
            // cannot publish a refusal.
            let gate = route_head_gate(
                &roads,
                band_id,
                *tile,
                improvement,
                faction,
                &discovery,
                knowledge_threshold,
                &ladder,
            );
            let is_head = allocation.build_queue_position(&entry.source) == Some(BUILD_QUEUE_HEAD);
            // **THE RUNG THE ROAD IS ACTUALLY CLIMBING, not where the entry is going** — a `pave`
            // on a road that has decayed back below a dirt road is doing *grading* work this turn,
            // so it resolves and wears the grading tool. The destination caps the climb below; it
            // does not price it.
            let in_flight = roads
                .road(*tile)
                .map(|road| road.held_rung())
                .and_then(|held| held.above())
                .filter(|next| destination.is_at_or_above(*next))
                .unwrap_or(destination);
            let gear = builders_gear.for_source(&entry.source, Some(in_flight));
            // **Only the head bid for the pile**, so only the head is scaled by what the store
            // settled; a waiting entry has bid on nothing and is quoted at full coverage.
            let entry_material_coverage = if is_head {
                build_coverage
            } else {
                FULLY_SERVED
            };
            // What this turn's work WOULD bank before the store scales it — the figure the pile is
            // struck against, so the demand on the row and the draw the settlement made are one
            // number rather than two readings of the same turn.
            let at_full_coverage =
                crate::intensification::pool_work_supply(builders, gear.work_per_worker);
            let legs = head_build_legs(
                &entry.source,
                destination,
                &forage_registry,
                &registry,
                &roads,
                &ladder,
            );
            let Some(road) = roads.road(*tile) else {
                continue;
            };
            let (base, width) =
                crate::routes::road_rung_span(destination, &ladder, road.keeper_remoteness);
            let banked = (road.position() - base).clamp(NOTHING_DEMANDED, width);
            let measure = crate::routes::road_measure(road, &tile_registry, &tiles);
            let meter_rot = crate::routes::road_meter_rot(road, measure, &ladder);
            // **THE ROW'S OWN BUILD SCRATCH, stamped where the quote is struck** — a road is a
            // source row (`RouteState`) and this is the pair a patch row publishes one branch over.
            // The cause is [`BuildQuote::blocking_gate`]'s, so the tile and the queue entry cannot
            // give the player two different reasons for one stall.
            let pile = if is_head {
                build_material_wants(&legs, at_full_coverage, &ladder)
                    .values()
                    .sum::<f32>()
            } else {
                // A waiting entry has bid on nothing: it draws no stone this turn, and saying it
                // wanted some would put a demand on the wire that nothing was ever going to pay.
                crate::routes::NO_MATERIAL_DRAWN
            };
            if let Some(road) = roads.road_mut(*tile) {
                road.build_material_demanded = pile;
                road.build_material_supplied = pile * entry_material_coverage;
            }
            let quote = BuildQuote {
                cost: width,
                banked,
                balance: ladder.rung(destination).build_balance(
                    Some(improvement),
                    gate.holds(),
                    builders,
                    gear.work_per_worker,
                    meter_rot,
                    entry_material_coverage,
                ),
                gate,
                legs: legs
                    .iter()
                    .map(|(rung, owed, _)| crate::intensification::BuildLeg {
                        rung: *rung,
                        work_remaining: *owed,
                    })
                    .collect(),
                material_coverage: entry_material_coverage,
            };
            // **THE CAUSE COMES OFF THE QUOTE, never off the gate directly** — a head the store
            // emptied has an `Open` rung gate, so reading `gate` here would stamp a road that is
            // stuck on stone with no cause at all, the same silence
            // [`crate::intensification::BuildQuote::blocking_gate`] exists to end.
            //
            // **Only the head carries one**, on `publish_build_chain`'s own rule: an entry merely
            // waiting its turn is not stuck and must not publish a reason it would have to explain
            // away.
            if let Some(road) = roads.road_mut(*tile) {
                road.build_blocked_reason = if is_head {
                    quote.blocking_gate()
                } else {
                    BuildGate::Open
                };
            }
            build_quotes.push((entry.source.clone(), quote));
            if !is_head || !gate.holds() || builders == NO_CREW_ON_THIS_ACTIVITY {
                continue;
            }
            // ⛔ **THE STORE SCALES THE WORK, exactly as it scales the pile**
            // (`docs/plan_standing_upkeep.md` §2.7). This was the road's one departure from the
            // pen's stated rule — *"a short store stalls the build proportionally and never refuses
            // it"* — and the departure ran in the player's favour: the settlement debited the stone
            // at `coverage` while the arm banked a **full** turn of work, so an empty shelf laid
            // pavement for free. The unbanked remainder is WASTED, not returned to the pool, which
            // is §2.5's rule for an indivisible supplier.
            let accrual = at_full_coverage * build_coverage;
            let Some(road) = roads.road_mut(*tile) else {
                continue;
            };
            // **Capped at the DESTINATION's top**, so a `grade` does not run on into the paved road
            // nobody ordered — the queue entry names where the road should end up, and §2.8's *"an
            // entry retires at its destination"* is the same rule read from the other side.
            let before = road.position();
            road.set_position((before + accrual).min(base + width), &ladder);
            // **The gear is charged for the progress the METER TOOK**, never for the work the pool
            // offered — the plant arm's rule, so a road already at its destination wears nothing,
            // and a road stalled on an empty store wears only what the covered fraction laid.
            charge_build_wear(
                band_equipment.as_deref_mut(),
                &equipment_cfg,
                &gear.wear_kit,
                road.position() - before,
            );
            if road.held_rung().is_at_or_above(destination) {
                completed.push((entry.source.clone(), entry.declared));
            }
        }
        // **RETIRE THE QUEUE ENTRY OF EVERY BUILD THAT COMPLETED THIS TURN** — the one seam all five
        // build kinds (Cultivate/Sow/Tame/Corral, and the pen ring) pass through. There is nothing
        // left to raise on this source, so the entry leaves the queue and the whole pool moves to
        // whatever the player put next (`docs/plan_standing_upkeep.md` §2.4).
        //
        // **THE HAND-OFF ONTO THE KEEPING ROLE IS RETIRED** (§2.3), and so is the crew it handed:
        // completion frees nobody now, because the builders never stood on the source. What the
        // player wants to know is that the head has moved on, which is what the line says.
        //
        // **It is ANNOUNCED**, on the finished verb's own feed channel so a rung's whole life reads
        // on one line.
        //
        // **The STANCE is not touched** — the row was never the build's home, so the crew, the tile
        // and its committed `species` (or the herd id) and the stance all simply stay as they are.
        //
        // **This turn's take is already banked above and is NOT rewound** — the turn a meter reaches
        // its cost is the last building turn, exactly as the accrue-after-take ordering promises the
        // pre-commit forecast.
        for (source, job) in &completed {
            // A completion another band finished retires nothing here and announces nothing: this
            // band never had the entry.
            if !allocation.unqueue_build(source) {
                continue;
            }
            let (channel, verb) = match job {
                BuildJob::Rung(improvement) => {
                    (improvement_feed_channel(*improvement), improvement.as_str())
                }
                // A ring is fencing work on the pen rung, so it reports on the pen's channel under
                // the command's own name — the same name the player typed.
                BuildJob::ExtendPen => (CommandEventKind::Corral, EXTEND_PEN_ACTION),
            };
            let named = describe_build_source(source);
            event_log.push(CommandEventEntry::new(
                tick.0,
                channel,
                faction,
                format!("{named} is built — your builders move to the next job"),
                Some(format!("status=complete action=build_complete job={verb}")),
            ));
        }
        // **THE REPAIRED TAKE SELECTIONS, written back** — before the `lapsed` removal shuffles the
        // indices they were collected against.
        for (idx, repaired) in repaired_takes {
            let Some(assignment) = allocation.assignments.get_mut(idx) else {
                continue;
            };
            if let LaborTarget::Forage { take_species, .. } = &mut assignment.target {
                *take_species = repaired;
            }
        }
        // Drop lapsed sources — Forage (tile out of work range) or Hunt (herd past the leash or
        // gone) — in reverse order to keep indices valid; workers return to the pool.
        // Remove the matching telemetry rows too so `last_yields` stays index-aligned with the
        // surviving assignments (lapsed rows carry a 0 yield anyway).
        for idx in lapsed.into_iter().rev() {
            allocation.assignments.remove(idx);
            yields.remove(idx);
        }
        allocation.last_yields = yields;
        // **A row that lapsed mid-loop takes its declaration with it**, on the same rule the
        // pre-loop prune enforces: an entry requires a row. **And a ring the dropped entry was
        // funding stops with it** — see [`fauna::cancel_dropped_rings`]; the lapse is the one exit
        // no command issues, so nothing else would clear the flag.
        let lapsed_entries =
            allocation.prune_build_queue(&|tile| band_keeps_road(&roads, band_id, tile));
        fauna::cancel_dropped_rings(&mut registry, &lapsed_entries);
        // **RETIRE EVERY ENTRY WHOSE DECLARED JOB IS ALREADY STANDING** — and say so on the verb's
        // own channel, exactly as a completion is announced.
        //
        // A verb enqueues on **every** band of the faction working the source
        // (`queue_build_on_working_bands`), so two bands on one patch or one herd is the ordinary
        // result of a single `cultivate` / `corral` / `extend_pen`. When one of them finishes the
        // rung, the other's entry declares a job that no longer exists: `build_workers` is still the
        // whole pool, **no arm consumes it**, `completed` never fires for that band, and
        // `prune_build_queue` only drops entries whose *row* is gone. So the survivor's builders
        // banked nothing, for ever, with no line saying why — and, on a patch, the projection of the
        // **next** rung was consumed by the chain as the dead head's own span, mis-dating every
        // entry behind it.
        //
        // **The test is "this rung is already achieved", never "the derived verb is `None`"**
        // (`forage::patch_rung_already_built` / `fauna::herd_rung_already_built`): a verb also
        // derives `None` for a source with nothing banked and nothing declared, which is a live
        // entry that has simply not started.
        retire_entries_already_built(
            &mut allocation,
            &forage_registry,
            &registry,
            &roads,
            faction,
            tick.0,
            &mut event_log,
        );
        // **THE CHAIN** — every entry's published finish date, walked in **queue order**
        // (`docs/plan_standing_upkeep.md` §4.6b). It runs here, after the meters have moved and the
        // queue has been edited, because the answer is a fact about the band's whole list and no
        // per-source site can see one.
        publish_build_chain(
            &allocation,
            &build_quotes,
            builders,
            &builders_gear,
            &mut forage_registry,
            &mut registry,
            &mut roads,
            &mut patch_build_claims,
            &mut herd_build_claims,
        );
    }
}

/// **DROP EVERY QUEUE ENTRY WHOSE DECLARED JOB IS ALREADY STANDING, AND ANNOUNCE EACH ONE.**
///
/// The completion path's twin, for the job **another band** (or an earlier turn) finished: there is
/// nothing left to raise, so the entry leaves the queue and the whole pool moves to whatever the
/// player put next (`docs/plan_standing_upkeep.md` §2.4). It runs **post-loop, beside
/// `completed`** — the same place and the same shape — so the next entry becomes the head on the
/// same schedule a real completion gives it, and the chain pass below dates a queue with no dead
/// entry in it.
///
/// # WHAT COUNTS AS ALREADY BUILT
///
/// | entry | test | seam |
/// |---|---|---|
/// | `Rung(Cultivate)` / `Rung(Sow)` | the meter it names is full | [`forage::patch_rung_already_built`] |
/// | `Rung(Tame)` / `Rung(Corral)` | the meter it names is full | [`fauna::herd_rung_already_built`] |
/// | `ExtendPen` | the ring is **not in flight** | `Herd::pen_extending` |
///
/// A ring's test is its flag rather than a meter because a ring has no rung of its own to complete:
/// `extend_pen` sets `pen_extending` *before* it queues, so an entry standing over a cleared flag is
/// a ring that finished, was cancelled, or was never begun — dead in every case.
///
/// ⛔ **DOES THIS BAND STILL KEEP THE ROAD ON THIS TILE?** — the route branch's half of *an entry
/// requires a holding*, handed to [`LaborAllocation::prune_build_queue`] because a road's membership
/// is `routes::Road::keeper` and the component cannot see it.
///
/// **A road with no keeper is nobody's job**, and a road kept by another band is not this band's:
/// both answer `false`, and the entry goes. That covers every way a keeper is lost with one rule
/// rather than a special case per exit — `abandon`, adoption by another band, and the one that
/// stranded a queue in play, `routes::advance_roads` releasing the keeper when decay drops the road
/// below `routes::traffic_ceiling`.
///
/// A band with no `BandId` keeps nothing: a keeper names a band, so an unnamed cohort cannot be one.
fn band_keeps_road(
    roads: &crate::routes::RoadRegistry,
    band_id: Option<BandId>,
    tile: UVec2,
) -> bool {
    band_id.is_some_and(|band| {
        roads
            .road(tile)
            .and_then(|road| road.keeper)
            .is_some_and(|keeper| keeper.band == band)
    })
}

/// A source the band no longer holds is not this function's business — `prune_build_queue` has
/// already taken it, and an entry naming a source neither registry can resolve is left alone rather
/// than guessed at.
fn retire_entries_already_built(
    allocation: &mut LaborAllocation,
    forage_registry: &ForageRegistry,
    herds: &HerdRegistry,
    roads: &crate::routes::RoadRegistry,
    faction: FactionId,
    tick: u64,
    event_log: &mut CommandEventLog,
) {
    let dead: Vec<BuildQueueEntry> = allocation
        .build_queue
        .iter()
        .filter(|entry| entry_job_already_built(entry, forage_registry, herds, roads))
        .cloned()
        .collect();
    for entry in &dead {
        if !allocation.unqueue_build(&entry.source) {
            continue;
        }
        let (channel, verb) = match entry.declared {
            BuildJob::Rung(improvement) => {
                (improvement_feed_channel(improvement), improvement.as_str())
            }
            BuildJob::ExtendPen => (CommandEventKind::Corral, EXTEND_PEN_ACTION),
        };
        let named = describe_build_source(&entry.source);
        event_log.push(CommandEventEntry::new(
            tick,
            channel,
            faction,
            format!("{named} is already built — your builders move to the next job"),
            Some(format!(
                "status=already_built action=build_retired job={verb}"
            )),
        ));
    }
}

/// Is this one entry's declared job already standing? See [`retire_entries_already_built`] for the
/// table this implements.
fn entry_job_already_built(
    entry: &BuildQueueEntry,
    forage_registry: &ForageRegistry,
    herds: &HerdRegistry,
    roads: &crate::routes::RoadRegistry,
) -> bool {
    match (&entry.source, entry.declared) {
        (BuildSource::Patch(tile), BuildJob::Rung(improvement)) => forage_registry
            .patch(*tile)
            .is_some_and(|patch| crate::forage::patch_rung_already_built(patch, improvement)),
        (BuildSource::Herd(id), BuildJob::Rung(improvement)) => herds
            .find(id.as_str())
            .is_some_and(|herd| fauna::herd_rung_already_built(herd, improvement)),
        (BuildSource::Herd(id), BuildJob::ExtendPen) => herds
            .find(id.as_str())
            .is_some_and(|herd| !herd.pen_extending),
        // A ring names a herd; a patch entry can never carry one.
        (BuildSource::Patch(_), BuildJob::ExtendPen) => false,
        // **A road's test is the rung it HOLDS**, the same *"this rung is already achieved"* shape
        // the two webs use. A road tile that has vanished from the registry — pruned back to bare
        // ground — reads as **not** built here, and that is the honest answer to *this* question:
        // what retires such an entry is the **prune**, which asks [`band_keeps_road`] and finds no
        // keeper on a road that is no longer there. This function judges the job; the prune judges
        // the holding.
        (BuildSource::Road(tile), BuildJob::Rung(improvement)) => {
            roads.road(*tile).is_some_and(|road| {
                road.held_rung()
                    .is_at_or_above(RungKey::built_by(improvement))
            })
        }
        // A ring names a herd; a road entry can never carry one.
        (BuildSource::Road(_), BuildJob::ExtendPen) => false,
    }
}

/// **DATE EVERY ENTRY IN ONE BAND'S QUEUE, AND EVERY SOURCE BEHIND IT** — the countdown half of
/// *all hands on the head* (`docs/plan_standing_upkeep.md` §4.6b).
///
/// # A WAITING ENTRY GETS A REAL DATE, NOT A "QUEUED" BADGE
///
/// The queue is deterministic — the head takes the whole pool until it fills, then the next one does
/// — so an entry's turns are **the sum of everything above it plus its own span at the full pool**.
/// Under §4.6a a waiting entry's meter is held by the *keeping*, not by the builders, so that
/// chained number is exact rather than an estimate that drifts.
///
/// # BAD NEWS PROPAGATES, AND THAT IS FREE
///
/// If the head never finishes, nothing below it does either — so the first entry that cannot name a
/// number is what **every** entry below it publishes. `carried` is that value, and once set nothing
/// further is evaluated.
///
/// | the head's own answer | it publishes | why |
/// |---|---|---|
/// | a count | `cumulative + n` | its own span, behind everything above it |
/// | [`BuildTurns::Holding`] / [`BuildTurns::Rotting`] | itself, carried down | a meter standing still or losing ground never reaches its cost |
/// | no answer, **at the head, with a staffed pool** | [`crate::intensification::BuildTurns::Blocked`], carried down | the pool is standing on a gate that refuses it |
/// | no answer, anywhere else | `None`, carried down | a *waiting* entry may well be eligible by the time it reaches the head, so it says the honest *"no estimate"* — and we cannot date what is behind an unanswerable entry |
///
/// # AND A BLOCKED HEAD SAYS **WHY**, DOWN THE WHOLE QUEUE
///
/// The `Blocked` sentinel states only *that* the pool is stuck, and the playtest it was filed
/// against sat on `⚠ Blocked 32%` for turns while fixing the one thing a surface happened to name.
/// The quote carries the refusing conjunct ([`BuildGate`]), so the head publishes its cause and
/// **`carried` takes it down the queue with the sentinel** — everything behind a blocked head is
/// stuck for the head's reason, which is the only reason there is to give.
///
/// A head that produced **no quote at all** is [`BuildGate::Unworked`]: not a rung's gate refusing,
/// but the labor loop never reaching this source. Everything that is not a blocked entry publishes
/// [`crate::intensification::BuildGate::Open`], whose key is `""`.
///
/// # A SOURCE WITH NO ENTRY IS QUOTED AT THE BACK OF THE LINE
///
/// That is where a newly queued build would actually go, so quoting it as though it went to the head
/// would over-promise the compose sheet by the whole queue.
#[allow(clippy::too_many_arguments)] // the band's queue, its quotes, its pool and all three registries
fn publish_build_chain(
    allocation: &LaborAllocation,
    quotes: &[(BuildSource, BuildQuote)],
    builders: u32,
    builders_gear: &BuildersGear,
    forage_registry: &mut ForageRegistry,
    herds: &mut HerdRegistry,
    // **Written, not merely read.** A road is a source row and this pass stamps its countdown, the
    // one figure on it that only the queue can answer for.
    roads: &mut crate::routes::RoadRegistry,
    patch_claims: &mut BuildEstimateClaims<UVec2>,
    herd_claims: &mut BuildEstimateClaims<String>,
) {
    let quote_for = |source: &BuildSource| {
        quotes
            .iter()
            .find(|(key, _)| key == source)
            .map(|(_, quote)| quote.clone())
    };
    // Turns already promised to the entries above this one.
    let mut cumulative: u32 = 0;
    // Once an entry cannot name a number, every entry below it publishes the same thing. `Some`
    // means *"decided"*; the inner `Option` is the published answer, `None` being the wire's
    // "no estimate", and the [`BuildGate`] beside it is the cause a blocked answer carries.
    let mut carried: Option<(Option<BuildTurns>, BuildGate)> = None;
    for (position, entry) in allocation.build_queue.iter().enumerate() {
        let quote = quote_for(&entry.source);
        // **The legs, dated where the entry can be dated and bare where it cannot.** An entry whose
        // countdown is a stall or a block publishes its legs' *work* — which is a fact about the
        // source and still true — with the stall's own sentinel on each date, because a leg cannot
        // be dated when the entry carrying it cannot.
        let mut legs: Vec<crate::intensification::PublishedBuildLeg> = Vec::new();
        let (published, reason) = match carried {
            Some(value) => value,
            None => match quote.as_ref().and_then(|quote| quote.turns(builders)) {
                Some(crate::intensification::BuildTurns::Turns(turns)) => {
                    // **THE LEGS ARE DATED ON THE SAME RUNNING SUM** — everything above this entry,
                    // then each leg's own span in climb order — so the last leg's number is exactly
                    // the entry's, by construction rather than by a second calculation agreeing with
                    // the first. Struck *before* `cumulative` absorbs the entry's whole span, and the
                    // final leg leaves it exactly where that would have.
                    legs = quote
                        .as_ref()
                        .map_or_else(Vec::new, |quote| leg_chain(quote, cumulative, builders));
                    cumulative = cumulative.saturating_add(turns);
                    (
                        Some(crate::intensification::BuildTurns::Turns(cumulative)),
                        crate::intensification::BuildGate::Open,
                    )
                }
                Some(state @ (BuildTurns::Holding | BuildTurns::Rotting)) => {
                    carried = Some((Some(state), crate::intensification::BuildGate::Open));
                    (Some(state), crate::intensification::BuildGate::Open)
                }
                // `build_turns_estimate` answers for one build's arithmetic and never returns this;
                // the arm below is the only place it is minted. Carried like any other stall.
                Some(crate::intensification::BuildTurns::Blocked) => {
                    let value = (
                        Some(crate::intensification::BuildTurns::Blocked),
                        blocked_reason(quote.clone()),
                    );
                    carried = Some(value);
                    value
                }
                None => {
                    // **ONLY THE HEAD, WITH A STAFFED POOL, IS BLOCKED.** A waiting entry whose gate
                    // refuses may well be eligible by the time it reaches the head.
                    let value =
                        if position == BUILD_QUEUE_HEAD && builders > NO_CREW_ON_THIS_ACTIVITY {
                            // ⛔ **A SOURCE THAT IS ON THE GROUND MUST RECORD A QUOTE, AND THIS
                            // IS WHERE A KIND THAT DOES NOT IS CAUGHT.** A staffed head with no
                            // quote is minted `Blocked` here with `blocked_reason(None)` —
                            // [`BuildGate::Unworked`], *"the labor loop never reached this
                            // source"* — and `carried` then hands that same `-4` down the whole
                            // queue and onto every unqueued source the band works.
                            //
                            // For a source that is genuinely **not there** (a `sow` ordered on bare
                            // ground the faction cannot seed, which places no patch) that answer is
                            // the truth and the cause is the right one. For a source that *is*
                            // there it is a lie, and two entry kinds have shipped telling it: the
                            // pen ring, and the road — a band that typed `grade` and staffed its
                            // builders published `⚠ Blocked` on its patches and its herds while the
                            // road built perfectly well. Both were fixed by their arm pushing a
                            // quote; this states the invariant so a third kind fails loudly in a
                            // test rather than quietly on a player's screen.
                            debug_assert!(
                                quote.is_some()
                                    || !source_is_on_the_ground(
                                        &entry.source,
                                        forage_registry,
                                        herds,
                                        roads,
                                    ),
                                "a staffed head standing on real ground must record a BuildQuote - \
                                 without one it publishes Blocked with the cause `unworked`, which \
                                 is false here, and carries that answer onto every entry behind \
                                 it: {:?}",
                                entry.source
                            );
                            (
                                Some(crate::intensification::BuildTurns::Blocked),
                                blocked_reason(quote.clone()),
                            )
                        } else {
                            // **Not blocked, so no cause** — an entry merely waiting its turn is not
                            // stuck and must not publish a reason it would have to explain away.
                            (None, crate::intensification::BuildGate::Open)
                        };
                    carried = Some(value);
                    value
                }
            },
        };
        if legs.is_empty() {
            // Not dated: carry the entry's own answer onto every leg, so the pair on the wire says
            // one thing.
            legs = quote.as_ref().map_or_else(Vec::new, |quote| {
                quote
                    .legs
                    .iter()
                    .map(|leg| crate::intensification::PublishedBuildLeg {
                        leg: *leg,
                        turns: published,
                    })
                    .collect()
            });
        }
        publish_entry(
            &entry.source,
            position,
            published,
            reason,
            Some(entry.declared.destination()),
            legs,
            builders_gear,
            forage_registry,
            herds,
            roads,
            patch_claims,
            herd_claims,
        );
    }
    // **Everything the band works that is NOT queued** — quoted where it would land if the player
    // queued it now, which is the tail.
    for (source, quote) in quotes {
        if allocation.build_queue_position(source).is_some() {
            continue;
        }
        // **The carried cause rides with the carried sentinel here too.** A source the band works
        // but has not queued is dated behind a blocked head like everything else, so it is stuck for
        // the head's reason and must be able to say so.
        let (projected, reason) = match carried {
            Some(value) => value,
            None => match quote.turns(builders) {
                Some(crate::intensification::BuildTurns::Turns(turns)) => (
                    Some(crate::intensification::BuildTurns::Turns(
                        cumulative.saturating_add(turns),
                    )),
                    crate::intensification::BuildGate::Open,
                ),
                other => (other, crate::intensification::BuildGate::Open),
            },
        };
        match source {
            BuildSource::Patch(tile) => {
                if let Some(patch) = forage_registry.patch_mut(*tile) {
                    patch_claims.publish_projected(
                        tile,
                        &mut patch.build_turns_remaining,
                        &mut patch.build_blocked_reason,
                        projected,
                        reason,
                    );
                }
            }
            BuildSource::Herd(id) => {
                if let Some(herd) = herds.herds.iter_mut().find(|herd| &herd.id == id) {
                    herd_claims.publish_projected(
                        id,
                        &mut herd.build_turns_remaining,
                        &mut herd.build_blocked_reason,
                        projected,
                        reason,
                    );
                }
            }
            // ⛔ **A ROAD PUBLISHES NO *CHAINED COUNTDOWN*, AND IT IS NOT FOR WANT OF A ROW.**
            // `RouteState` **is** the road's source row — keyed by tile exactly as a patch row is — and it
            // carries the tile's own build state (`buildBlockedReason` and the material pair), stamped by
            // the road build arm. What it does not carry is `buildTurnsRemaining` and its four siblings,
            // which are per-patch and per-herd *scratch* written by this pass; a road's countdown is
            // client-side work this slice leaves. The arm is stated rather than defaulted so a future row
            // cannot be forgotten here.
            //
            // **The road's quote IS recorded** ­— see the road build arm — which is the half that mattered:
            // without one, a staffed road head published `Blocked` with no cause and `carried` handed that
            // same answer to every entry behind it.
            BuildSource::Road(_) => {}
        }
    }
}

/// **IS THIS QUEUED SOURCE ACTUALLY THERE?** — the term that tells a *missing quote* apart from a
/// *missing source*, and the whole of what makes the staffed-head invariant assertable.
///
/// A queue entry can name ground that does not exist: `sow` places a Field on **bare** ground, so an
/// entry the faction cannot yet seed names a tile with no patch on it, turn after turn. That head
/// records no quote and publishing [`BuildGate::Unworked`] for it is exactly right — nothing is
/// there. What must never happen is the same answer for a source that **is** there, which is the
/// defect the pen ring and the road both shipped.
fn source_is_on_the_ground(
    source: &BuildSource,
    forage_registry: &ForageRegistry,
    herds: &HerdRegistry,
    roads: &crate::routes::RoadRegistry,
) -> bool {
    match source {
        BuildSource::Patch(tile) => forage_registry.patch(*tile).is_some(),
        BuildSource::Herd(id) => herds.find(id).is_some(),
        BuildSource::Road(tile) => roads.road(*tile).is_some(),
    }
}

/// **WHY THE POOL IS STUCK ON THIS ENTRY** — the cause a [`crate::intensification::BuildTurns::Blocked`] head publishes.
///
/// A quote whose gate refused answers with that conjunct. Everything else is
/// [`BuildGate::Unworked`]: either the source produced no quote this turn (the labor loop never
/// reached it), or it produced one whose gate *held* — which `build_turns_estimate` makes
/// unreachable at a staffed head, and which would otherwise publish a block with no cause, the very
/// silence this field exists to end.
/// **HAS THE SOURCE REACHED WHERE THE PLAYER SENT IT?** — the one test that retires a queue entry
/// (`docs/plan_standing_upkeep.md` §2.8): an entry names a **destination**, so it stays at the head
/// until `held` is at or above it, whatever intermediate rungs completed on the way.
///
/// **`None` — a source with no entry — is never "arrived"**, because there is nothing to retire. A
/// rung that completes on unqueued ground is announced on its own channel and changes no queue.
/// **DATE EACH LEG ON THE QUEUE'S OWN RUNNING SUM** — `already` is the turns promised to the entries
/// *above* this one, and each leg adds its own span to it in climb order.
///
/// **It is `publish_build_chain`'s arithmetic one level down, not a second copy of it**: the chain
/// dates an entry as *everything above it plus its own span at the full pool*, and a leg is dated the
/// same way against the legs above it. The last leg therefore lands on exactly the entry's own
/// number by construction, which is what stops the two readouts drifting.
///
/// A leg whose own span cannot be dated — the pool banks nothing, the meter is rotting — stops the
/// chain: everything from there down carries that same answer, exactly as a stalled entry does to
/// the entries behind it.
fn leg_chain(
    quote: &BuildQuote,
    already: u32,
    builders: u32,
) -> Vec<crate::intensification::PublishedBuildLeg> {
    let mut cumulative = already;
    let mut carried: Option<Option<BuildTurns>> = None;
    quote
        .legs
        .iter()
        .map(|leg| {
            let turns = match carried {
                Some(value) => value,
                None => {
                    // **Each leg is quoted from a standing start on its own remainder** — the work
                    // is already *remaining* work (`BuildLeg::work_remaining`), so there is nothing
                    // banked left to subtract, and the gate and balance are the entry's.
                    let span = crate::intensification::build_turns_estimate(
                        leg.work_remaining,
                        crate::intensification::RUNG_UNSTARTED,
                        quote.balance,
                        quote.gate.holds(),
                        builders,
                    );
                    match span {
                        Some(crate::intensification::BuildTurns::Turns(turns)) => {
                            cumulative = cumulative.saturating_add(turns);
                            Some(crate::intensification::BuildTurns::Turns(cumulative))
                        }
                        other => {
                            carried = Some(other);
                            other
                        }
                    }
                }
            };
            crate::intensification::PublishedBuildLeg { leg: *leg, turns }
        })
        .collect()
}

fn arrived_at_destination(destination: Option<RungKey>, held: RungKey) -> bool {
    destination.is_some_and(|target| held.is_at_or_above(target))
}

fn blocked_reason(quote: Option<BuildQuote>) -> BuildGate {
    match quote {
        // ⛔ **THROUGH `blocking_gate`, NEVER `gate` DIRECTLY.** A build the **store** stopped has a
        // rung gate that *holds* — there is no affordability gate on a build
        // (`docs/plan_standing_upkeep.md` §2.5) — so reading `gate` would publish a block with no
        // cause, the exact silence this field was added to end. `blocking_gate` folds the two into
        // one answer, and it is the only reader of either.
        Some(quote) if !quote.blocking_gate().holds() => quote.blocking_gate(),
        _ => BuildGate::Unworked,
    }
}

/// Stamp one queued source's countdown, its place in the line and the pool's gear delivery — through
/// the claims seam, so a second band on the same source cannot overwrite a sooner answer with a
/// later one ([`BuildEstimateClaims`]).
#[allow(clippy::too_many_arguments)] // one source, its answer, and both webs' registries and claims
fn publish_entry(
    source: &BuildSource,
    position: usize,
    turns: Option<BuildTurns>,
    reason: BuildGate,
    destination: Option<RungKey>,
    legs: Vec<crate::intensification::PublishedBuildLeg>,
    builders_gear: &BuildersGear,
    forage_registry: &mut ForageRegistry,
    herds: &mut HerdRegistry,
    roads: &mut crate::routes::RoadRegistry,
    patch_claims: &mut BuildEstimateClaims<UVec2>,
    herd_claims: &mut BuildEstimateClaims<String>,
) {
    let answer = BuildEstimate {
        turns,
        reason,
        // **The rung in flight is the first leg**, the same reading the material draw takes: the
        // legs are in climb order, so the head of the list is where this turn's work lands and
        // therefore which tool the pool is holding. A row with no legs is climbing nothing.
        gear: builders_gear
            .for_source(source, legs.first().map(|published| published.leg.rung))
            .gear_supply,
        position: position as i32,
        destination,
        legs,
    };
    match source {
        BuildSource::Patch(tile) => {
            if let Some(patch) = forage_registry.patch_mut(*tile) {
                patch_claims.publish_running(
                    *tile,
                    BuildEstimateSlots {
                        turns: &mut patch.build_turns_remaining,
                        reason: &mut patch.build_blocked_reason,
                        gear: &mut patch.build_work_from_gear,
                        position: &mut patch.build_queue_position,
                        destination: &mut patch.build_destination,
                        legs: &mut patch.build_legs,
                    },
                    answer,
                );
            }
        }
        BuildSource::Herd(id) => {
            if let Some(herd) = herds.herds.iter_mut().find(|herd| &herd.id == id) {
                herd_claims.publish_running(
                    id.clone(),
                    BuildEstimateSlots {
                        turns: &mut herd.build_turns_remaining,
                        reason: &mut herd.build_blocked_reason,
                        gear: &mut herd.build_work_from_gear,
                        position: &mut herd.build_queue_position,
                        destination: &mut herd.build_destination,
                        legs: &mut herd.build_legs,
                    },
                    answer,
                );
            }
        }
        // ⛔ **A ROAD IS A SOURCE ROW AND IT PUBLISHES A CHAINED COUNTDOWN LIKE ANY OTHER.**
        // `RouteState` is that row — keyed by tile exactly as a patch row is — and this is the one
        // figure on it that **only the queue can answer for**: an entry is dated as everything above
        // it plus its own span, which no per-tile seam can see. The rest of the road's build state
        // (`buildBlockedReason`, the material pair) is stamped by the road build arm, where the quote
        // is struck.
        //
        // **It shipped stamping nothing, and the client filled the silence with a constant**: every
        // road queue model hardcoded the *"not yet estimated"* sentinel, so a road read `Queued` on
        // turn 1 and on turn 147 alike. The claim behind it — *a road has no source row for the sim
        // to stamp one on* — was never true of this table.
        //
        // ⛔ **NO CLAIMS OBJECT, AND THAT IS STRUCTURAL RATHER THAN AN OMISSION.** The patch and herd
        // arms go through [`BuildEstimateClaims`] because several bands can work one source and the
        // **sooner** answer must win. A road cannot be contested: there is one keeper per tile, and
        // each band's own `prune_build_queue` drops the entry for a road it does not keep **before**
        // that band's queue is walked — so by the time this pass runs, at most one band holds an
        // entry for any tile. A claims set would be a rule with nothing to arbitrate.
        BuildSource::Road(tile) => {
            if let Some(road) = roads.road_mut(*tile) {
                road.build_turns_remaining = answer.turns;
                // **The place in the line rides with the date**, the same pairing the two food webs
                // publish: it is what lets the countdown tell *"queued since the last pass"* from
                // *"looked at and stalled"*, which sit at the same `0%`.
                road.build_queue_position = answer.position;
            }
        }
    }
}

/// **The 0-based head of a build queue** — named rather than a bare `0` at the one site that tests
/// it, because the test is *"is this the entry the pool is standing on"* and not an index.
const BUILD_QUEUE_HEAD: usize = 0;

/// The `extend_pen` command's own name — the feed detail of a ring that completed, and the job token
/// a ring's row publishes. A ring is not one of the four rung verbs, so it has no
/// `Improvement::as_str` to borrow, and the command's own name is the one word the player typed.
pub const EXTEND_PEN_ACTION: &str = "extend_pen";

/// **SAY WHOSE WORK BOARD A LOST ROW WAS ON** — appends the `band=` detail token to `detail`, or
/// leaves it alone for a cohort carrying no durable id.
///
/// The event dock's per-row *"Work tab"* link is the only consumer, and it is why the token exists:
/// a `status=trimmed` / `lapsed` / `pruned` row offers a jump to the band whose crew the sim cut, and
/// the dock refuses to recover a band by reading the label's prose. `CommandEventState` on the wire
/// is `{tick, kind, faction, label, detail, seq}` and carries no band field, so the detail token is
/// the whole channel — a detail-token addition, not a schema change.
///
/// > #### ⛔ THE DURABLE [`BandId`], NEVER THE `Entity`
/// >
/// > Both are `u64` and neither would fail to compile here. The client resolves this id through a
/// > **roster join keyed on `band_id`**, so entity bits would name a band that does not exist and the
/// > link would jump nowhere. `xtask/src/command_guard.rs` exists because that exact confusion once
/// > silently broke every band-addressed order.
///
/// **Appended, so it must stay ahead of any multi-word trailing value.** Detail tokens are
/// space-delimited `key=value` (`.claude/rules/core_sim/event-feed.md`), a numeric id needs no
/// position, and every line this is applied to ends in a numeric or single-word token. A line whose
/// last token is a display name or a comma-joined list must interpolate `band=` earlier instead.
fn band_detail_token(detail: String, band: Option<BandId>) -> String {
    match band {
        Some(id) => format!("{detail} band={}", id.0),
        // A band with no durable id has nothing to name the jump after — the same rule the
        // demographic feed follows (`systems::population`), and the row simply renders linkless.
        None => detail,
    }
}

/// **Name a build source for a feed line** — `(x, y)` for a patch, its id for a herd.
fn describe_build_source(source: &BuildSource) -> String {
    match source {
        BuildSource::Patch(tile) => format!("({}, {})", tile.x, tile.y),
        BuildSource::Herd(id) => id.clone(),
        BuildSource::Road(tile) => format!("the road at ({}, {})", tile.x, tile.y),
    }
}

// **RETIRED: `describe_worked_source`** — it named a worked source for the completion hand-off's
// feed line, and that line now names a **queue entry** rather than a labor row
// (`docs/plan_standing_upkeep.md` §2.5). [`describe_build_source`] is its replacement, over the
// vocabulary the queue is keyed in.

/// **The feed channel an improvement's events ride** — the verb's own, so a rung's whole life
/// (start → complete → where its crew went) reads on one line. The labor system's copy of the
/// server's `improvement_event_kind`, which the server keeps for the *command* half.
fn improvement_feed_channel(improvement: Improvement) -> CommandEventKind {
    match improvement {
        Improvement::Cultivate => CommandEventKind::Cultivate,
        Improvement::Sow => CommandEventKind::Sow,
        Improvement::Tame => CommandEventKind::Tame,
        Improvement::Corral => CommandEventKind::Corral,
        // **One channel for both road verbs** — a road tile climbing its branch is one thing the
        // player is watching, and which verb it was rides the detail.
        Improvement::Grade | Improvement::Pave => CommandEventKind::Road,
    }
}

/// **Say what the band just stopped doing** — the feed line for hands
/// [`LaborAllocation::normalize`] took off a row because the band no longer has the workers for it.
///
/// Shaped like the out-of-range Forage lapse it sits beside: the source named in the label, a
/// `status=… reason=too_few_workers` detail, and the verb's own `CommandEventKind` so the line lands
/// on the channel the player is already watching for that source.
///
/// > #### ⛔ A CREW THAT MERELY SHRANK GETS A LINE TOO
/// >
/// > [`ShedCrew::row_survived`] picks between `status=trimmed` and `status=lapsed`, and the trim half
/// > is the one that was missing. This used to be called only for rows destroyed outright, so a crew
/// > going `6 → 3` on a band that had lost a worker was published with **no event on any channel** —
/// > and from the player's side that is a number they had just raised moving on its own. It is the
/// > same event as the lapse at a smaller magnitude, so it is the same function and the same
/// > channel; what differs is that the trim names the crew still standing there, because that is the
/// > number the row now reads and the thing the player will go looking for.
///
/// **The improvement is not named**, on either line: a build lives in the band's queue rather than on
/// the row (`docs/plan_standing_upkeep.md` §2.5), so what a shed costs is exactly the hands named
/// here, and a dropped row's entry is retired by the turn's prune on the rule that an entry requires
/// a row.
///
/// **The band IS named, on the detail** ([`band_detail_token`]). It is the only channel the dock's
/// per-row *"Work tab"* link has, and the band-wide roles (`kind=scout` / `warrior` / `builders`)
/// name no source at all — so on those lines there is nothing whatever to infer a band from.
fn announce_shed_crew(
    event_log: &mut CommandEventLog,
    tick: u64,
    faction: FactionId,
    band: Option<BandId>,
    shed: &ShedCrew,
) {
    // A band-wide role (Scout/Warrior) has no source to name and no verb channel of its own; it is
    // reported on the label alone, through the role's own kind where one exists.
    //
    // **The bench is not a `LaborTarget` and is reported as itself** — one band, one bench, so it
    // needs no id — through the crafting verb's own kind.
    let Some(target) = shed.subject.row() else {
        announce_shed_bench(event_log, tick, faction, band, shed);
        return;
    };
    let (kind, source_label, source_detail) = match target {
        LaborTarget::Forage { tile, .. } => (
            CommandEventKind::Forage,
            format!("foragers at ({}, {})", tile.x, tile.y),
            format!("kind=forage x={} y={}", tile.x, tile.y),
        ),
        LaborTarget::Hunt { fauna_id, .. } => (
            CommandEventKind::Hunt,
            format!("hunters on {fauna_id}"),
            format!("kind=hunt herd={fauna_id}"),
        ),
        LaborTarget::Scout => (
            CommandEventKind::Scout,
            "scouts".to_string(),
            "kind=scout".to_string(),
        ),
        LaborTarget::Warrior => (
            CommandEventKind::CancelOrder,
            "warriors".to_string(),
            "kind=warrior".to_string(),
        ),
        LaborTarget::Agriculture => (
            CommandEventKind::Cultivate,
            "field keepers".to_string(),
            "kind=agriculture".to_string(),
        ),
        LaborTarget::Husbandry => (
            CommandEventKind::Corral,
            "herd keepers".to_string(),
            "kind=husbandry".to_string(),
        ),
        // **The road keepers report on the generic channel**, as the builders and the warriors do:
        // the route branch declares no verb (traffic is the crew), so there is no web's feed line
        // for a road's hands to ride the way `agriculture` rides `cultivate`.
        LaborTarget::Roadwork => (
            CommandEventKind::CancelOrder,
            "road keepers".to_string(),
            "kind=roadwork".to_string(),
        ),
        LaborTarget::Builders => (
            CommandEventKind::CancelOrder,
            "builders".to_string(),
            "kind=builders".to_string(),
        ),
    };
    // **The crew that is left is what the two lines differ on.** A row still worked says the number
    // it now reads, so the player can find it; a row that is gone says it is gone.
    let (label, status, workers) = if shed.row_survived() {
        (
            format!("{source_label} cut to {} — too few workers", shed.remaining),
            "trimmed",
            shed.remaining,
        )
    } else {
        (
            format!("{source_label} disbanded — too few workers"),
            "lapsed",
            shed.lost,
        )
    };
    event_log.push(CommandEventEntry::new(
        tick,
        kind,
        faction,
        label,
        Some(band_detail_token(
            format!(
                "status={status} reason=too_few_workers {source_detail} workers={workers} lost={}",
                shed.lost,
            ),
            band,
        )),
    ));
}

/// **SAY THE BENCH LOST HANDS** — the crafting arm of [`announce_shed_crew`], split out because its
/// two readings are not the row's two readings.
///
/// # ⛔ `status=stalled` IS A THIRD TOKEN, AND NEITHER EXISTING ONE WOULD HAVE BEEN TRUE
///
/// The client ranks a shed line on this token, so it has to be the fact:
///
/// - **`trimmed`** means *the crew is smaller than you set and the source is still worked*. A bench
///   at zero is not being worked at all, so on the last hand that is false.
/// - **`lapsed`** means *the row is GONE and its investment with it* — and it is ranked ALERT for
///   exactly that reason. The bench keeps its recipe, its progress, its finished count **and the
///   materials it had already drawn**; re-staffing resumes rather than restarts. Nothing is
///   destroyed, so `lapsed` would be false *and* would shout.
///
/// A bench that still has hands on it **is** a trim, in the token's own terms, and reuses it — the
/// third token exists only for the state neither describes.
///
/// **`stalled` ranks with `trimmed` (NOTABLE), not with `lapsed` (ALERT)**: it is recoverable by one
/// command and costs the player nothing they cannot get back. A client that has not learned the token
/// yet renders it at the quietest rung, which is the wrong direction — so the token is reported to
/// the client half rather than assumed.
fn announce_shed_bench(
    event_log: &mut CommandEventLog,
    tick: u64,
    faction: FactionId,
    band: Option<BandId>,
    shed: &ShedCrew,
) {
    let (label, status, workers) = if shed.row_survived() {
        (
            format!("crafters cut to {} — too few workers", shed.remaining),
            "trimmed",
            shed.remaining,
        )
    } else {
        (
            "the bench stalled — too few workers".to_string(),
            "stalled",
            shed.lost,
        )
    };
    event_log.push(CommandEventEntry::new(
        tick,
        CommandEventKind::Craft,
        faction,
        label,
        Some(band_detail_token(
            format!(
                "status={status} reason=too_few_workers kind=bench workers={workers} lost={}",
                shed.lost,
            ),
            band,
        )),
    ));
}

// **RETIRED: `forage_rung_already_built` / `hunt_rung_already_built`** — the "is there anything left
// to build at this rung on this source" test, whose one job was to clear a verb the sim would
// otherwise have driven on a finished rung.
//
// **The verb is derived from the meter now** (`forage::patch_build_verb` /
// `fauna::herd_build_verb`): a declaration is honoured only where the meter it names is at zero, so a
// stale one on a finished rung answers `None` on its own and there is nothing to clear. The test was
// cleaning up after an authority that no longer exists.

/// **CHARGE THE CREW'S GEAR FOR THE BUILD PROGRESS IT JUST BOUGHT** — the
/// [`crate::equipment_config::WearQuantum::BuildProgress`] site, and the only one (issue #515).
///
/// One helper rather than five copies, because every build arm charges the identical thing and a
/// per-arm spelling is how one of them comes to charge the wrong quantum or forget the kit mask.
/// `wear_kit` filters by quantum and by the kit's own mask, and `usable_uses` floors a zero.
///
/// **CHARGED FOR THE PROGRESS THE METER TOOK, NOT THE PROGRESS THE RUNG OFFERED.** Every build arm
/// reads its source's meter before the accrual and passes the *delta*, because the accrual a rung
/// computes is only an offer: the accrue helpers own the `owner is None || owner == faction` rule
/// (deliberately absent from each arm's `eligible`), so a source can refuse the whole of it. A band
/// whose `Tame` outlived another faction claiming the herd is the reachable case — it passes every
/// gate this site checks, banks nothing, and never has its verb cleared — and billing the offer
/// would bleed its gear dry against a meter that never moved. The delta makes *"a build that was
/// refused spends nothing"* structural rather than a rule each caller must remember, and it bills
/// the completing turn for exactly the progress it banked instead of a full turn's offer clamped
/// away by the job's own cost.
fn charge_build_wear(
    equipment: Option<&mut BandEquipment>,
    config: &crate::equipment_config::EquipmentConfig,
    kit: &crate::equipment_config::KitChoice,
    accrual: f32,
) {
    if let Some(wear) = equipment {
        wear.wear_kit(
            config,
            kit,
            crate::equipment_config::WearQuantum::BuildProgress,
            accrual,
        );
    }
}

/// **CHARGE A KEEPER'S GEAR FOR THE WORK IT JUST SUPPLIED** — the
/// [`crate::equipment_config::WearQuantum::UpkeepWork`] site, and the only one, sitting beside
/// [`charge_build_wear`] for the same reason: one helper rather than one per web.
///
/// **`supplied` is what the source's meter was actually held with this turn** — the value
/// `forage::patch_upkeep_supply` / `fauna::herd_upkeep_supply` returned and stamped, never the
/// rung's demand and never the pool's head count. A share is capped at the demand
/// ([`crate::intensification::distribute_upkeep_pool`]), so a pool larger than what the band holds
/// spends only what it was asked for, and a pool with nothing at risk spends nothing at all.
///
/// **Once per source per band**, at the same seam that accumulates `upkeep_supplied` — so two bands
/// keeping one patch each wear their own gear for their own share, exactly as they each supply it.
/// **EVERY KIT ITEM'S LIFE, AS A FRACTION OF ONE FRESH UNIT** — the reading
/// `snapshot::crafting::life_severity` colours, taken here so a turn's wear can be read as a
/// **transition** rather than as a level.
///
/// # ⛔ THE PANEL'S OWN NUMBER, PER BATCH, ROLLED UP TO THE ITEM'S WORST
///
/// Each batch is struck by [`crate::snapshot::crafting::batch_life_fraction`] — the *only* producer
/// of this fraction, at the **batch's own tier** — and the item takes the **minimum**, which is
/// exactly the worst `equipmentBatches` row the player is looking at. So the dock fires when a row
/// on the panel turns amber and never otherwise.
///
/// Reading it any other way is what made the two disagree: an item-level
/// `(default_durability − Σ batch wear) / durability` clamped to `[0, 1]` announced a two-hoe batch
/// at `1.30` (*healthy* on the panel) as `0.30` — *warn* — and summed two half-worn batches to `0`,
/// a `danger` Alert on mostly-fresh gear.
///
/// An item the band owns no batch of has **no entry**, which is how the crossing test skips gear
/// gained or lost this turn.
pub(crate) fn kit_life_fractions(
    equipment: &crate::equipment_config::EquipmentConfig,
    wear: Option<&BandEquipment>,
) -> BTreeMap<String, f32> {
    let mut lives = BTreeMap::new();
    let Some(wear) = wear else {
        return lives;
    };
    for (id, def) in equipment.items() {
        let worst = wear
            .batches_of(id)
            .iter()
            .map(|batch| {
                crate::snapshot::crafting::batch_life_fraction(
                    def.tier_or_default(&batch.tier),
                    batch,
                )
            })
            .fold(None::<f32>, |worst, life| {
                Some(worst.map_or(life, |worst| worst.min(life)))
            });
        if let Some(worst) = worst {
            lives.insert(id.to_string(), worst);
        }
    }
    lives
}

/// **THE KIT-LIFE NOTIFICATION** (`docs/plan_standing_upkeep.md` §4.9 item 12) — the two seams
/// `equipment.json`'s `life_readout` has shipped with all along, finally reaching the player.
///
/// **Edge-gated on the CROSSING, with no stored state.** A level test would push the same line every
/// turn for the rest of a spear's life; the fractions taken before and after this turn's wear are the
/// transition itself, so a line fires exactly once per item per seam crossed and nothing has to be
/// checkpointed. An item the band gained or lost this turn is not a crossing and is skipped.
///
/// **And the fraction is the PANEL'S** ([`kit_life_fractions`] → the one
/// [`crate::snapshot::crafting::batch_life_fraction`]), so the dock and the ledger cannot disagree
/// about when a spear is nearly out. `remaining=` therefore rides above `1.00` for a stockpile,
/// exactly as the `equipmentBatches` row does.
///
/// **The sim publishes a KIND and a DETAIL, never a rung.** There is no importance field on the wire;
/// the Alert/Notable/Routine ladder is resolved client-side off `kind`, so the seam that was crossed
/// rides the detail as `severity=warn|danger` — and it is
/// [`crate::snapshot::crafting::life_severity`]'s own answer, never a second reading of the same two
/// thresholds.
fn announce_kit_life(
    event_log: &mut CommandEventLog,
    tick: u64,
    faction: FactionId,
    band: Option<BandId>,
    equipment: &crate::equipment_config::EquipmentConfig,
    before: &BTreeMap<String, f32>,
    after: &BTreeMap<String, f32>,
) {
    for (id, now) in after {
        let Some(was) = before.get(id) else {
            continue;
        };
        let crossed = crate::snapshot::crafting::life_severity(*now, equipment);
        if crossed == crate::snapshot::crafting::life_severity(*was, equipment)
            || crossed == crate::snapshot::crafting::LIFE_HEALTHY
        {
            continue;
        }
        let name = crate::crafting::title_from_id(id);
        event_log.push(CommandEventEntry::new(
            tick,
            CommandEventKind::KitLife,
            faction,
            format!("{name} are wearing out"),
            Some(band_detail_token(
                format!(
                    "status=wearing severity={crossed} item={id} remaining={now:.2} action=craft"
                ),
                band,
            )),
        ));
    }
}

/// **THE MATERIAL-SHORTFALL ALERT — and it NAMES THE BAND**
/// (`docs/plan_standing_upkeep.md` §4.9 item 12), which is what replaces the faction `Gear` row's
/// *"⚠ 1 band"* discovery path: a faction-level line says something is wrong and not where.
///
/// **Driven off the standing bill the same turn publishes**, so the event and the disclosure row
/// cannot describe different turns: the band's summed per-turn need against
/// [`LaborAllocation::material_income`] — **the `material_upkeep_income` row's own producer**, take
/// plus bench — with the store on the shelf as the buffer between them.
///
/// ⛔ The bench half is not optional garnish. `hurdles` have no producer but a bench on the shipped
/// roster, so an Alert struck on the credited take alone sees zero income for ever, calls the whole
/// pen bill a gap, and fires for **every band that keeps a pen** — including one whose bench
/// out-produces its pens.
///
/// **The condition is *"the shelf will not outlast the gap"***, not *"the bill went unpaid"* — a bill
/// paid out of a store that is emptying is exactly the case a player wants warning of, and the alert
/// would otherwise arrive on the turn the fence started falling down.
///
/// **One line per band per material**, and edge-gated on the band's own transient
/// [`LaborAllocation::material_shortfall_warned`] so a standing famine does not repeat every turn. It
/// is deliberately **not** checkpointed: a rollback may re-announce once, which is the same
/// concession `Herd::pen_starving` makes and is cheaper than a second persisted flag.
fn announce_material_shortfall(
    event_log: &mut CommandEventLog,
    tick: u64,
    faction: FactionId,
    band: Option<BandId>,
    allocation: &mut LaborAllocation,
    stores: &LocalStore,
    // **THE WHOLE PER-TURN INFLOW** — [`LaborAllocation::material_income`], which is the same figure
    // `PopulationCohortState::material_upkeep_income` publishes. ⛔ Never
    // [`LaborAllocation::last_material_income`] on its own: that is the *credited take*, and a bench
    // credits nothing until its meter crosses, so a gap struck against it counts a band's whole
    // hurdle bill as unfunded however hard its bench is working.
    income: &BTreeMap<String, f32>,
) {
    /// **How many turns of shelf is "about to run out"** — a store that outlasts this is not news.
    /// A **named constant, not a config lever**: it is the alert's own sensitivity and nothing in the
    /// sim branches on it, so a dial here would be a number with no consequence to observe.
    const SHELF_TURNS_WORTH_WARNING: f32 = 5.0;
    let mut short: Vec<(String, f32)> = Vec::new();
    for (id, need) in &allocation.last_material_need {
        let arriving = income.get(id).copied().unwrap_or(NOTHING_DEMANDED);
        let gap = need - arriving;
        if gap <= NOTHING_DEMANDED {
            continue;
        }
        if stores.material_total(id).to_f32() > gap * SHELF_TURNS_WORTH_WARNING {
            continue;
        }
        short.push((id.clone(), gap));
    }
    let names: Vec<String> = short.iter().map(|(id, _)| id.clone()).collect();
    if names == allocation.material_shortfall_warned {
        return;
    }
    for (id, gap) in &short {
        if allocation.material_shortfall_warned.contains(id) {
            continue;
        }
        let name = crate::crafting::title_from_id(id);
        event_log.push(CommandEventEntry::new(
            tick,
            CommandEventKind::MaterialShortfall,
            faction,
            format!("{name} is running out"),
            Some(band_detail_token(
                format!(
                    "status=outrunning material={id} short={gap:.2} held={:.2} action=craft",
                    stores.material_total(id).to_f32()
                ),
                band,
            )),
        ));
    }
    allocation.material_shortfall_warned = names;
}

/// **SPEND THIS SITE'S KEEPING TOOLS ON THE WORK ITS KEEPERS ACTUALLY DID** — the
/// [`crate::equipment_config::WearQuantum::UpkeepWork`] charge.
///
/// `kit` is **the site's own** ([`KeepingAward::wear_kit`]), never the band's: two patches this band
/// keeps with two different tools wear two different tools, and billing both against one would run
/// down gear that was never in that site's hands. `None` is a row that claimed nothing, which
/// therefore wore nothing.
fn charge_keeping_wear(
    equipment: Option<&mut BandEquipment>,
    config: &crate::equipment_config::EquipmentConfig,
    kit: Option<&crate::equipment_config::KitChoice>,
    supplied: f32,
) {
    if let (Some(wear), Some(kit)) = (equipment, kit) {
        wear.wear_kit(
            config,
            kit,
            crate::equipment_config::WearQuantum::UpkeepWork,
            supplied,
        );
    }
}

/// **WHOSE ANSWER A SOURCE PUBLISHES WHEN SEVERAL BANDS WORK IT IN ONE TURN.**
///
/// `build_turns_remaining` and `build_work_from_gear` are **per-source** fields written **per
/// assignment**, and more than one band may work one patch or one herd in a turn — two crews on a
/// Cultivate, or a crew building beside a crew that is only gathering. Without a rule the field is
/// **last-writer-wins**, decided by the order the labor loop happens to visit bands in: a band that
/// is merely foraging published its *projection of the next rung* over the running build's
/// countdown, so the tile card quoted turns for a crew that was not building.
///
/// **The rule, in order:**
/// 1. **A RUNNING BUILD BEATS A PROJECTION.** A projection answers *"what would the next rung
///    cost?"*; while a build is in flight that is not the question the card is asking, and the
///    running rung is not even the rung the projection quotes.
/// 2. **Among running builds, the SOONEST finish wins.** Every crew on one source fills the **same**
///    meter, so each one's answer counts only its own output and is therefore an over-estimate; the
///    smallest is the least wrong. (The exact joint answer would need the turn's work summed per
///    source before any crew is quoted — a bigger change than the defect warrants, and it would only
///    move the number further in the direction this rule already takes it.)
/// 3. **A stall never displaces a moving crew.** `None` is *"no answer"*, so it loses to any number
///    — but it still **claims** the source, because a projection of the next rung is not the right
///    answer for a build that is merely stalled.
///
/// **The gear rides the same winner.** The two fields are read as one pair (`yield-forecast.md` →
/// "THE BOUNDARY, stated once": the client's closed form checks its gear term against
/// `buildWorkFromGear`), so publishing one crew's turns beside another crew's kit is the same
/// defect one field over.
///
/// Per-turn scratch of the labor system itself rather than state on the source: the sources' own
/// estimates are cleared by the next turn's Logistics pass, so a claim only has to outlive the band
/// loop.
/// **THE THREE FIELDS A RUNNING BUILD PUBLISHES, AS ONE SET.** They are read as one on the client
/// (`yield-forecast.md` → "THE BOUNDARY, stated once"), so they are written as one here: publishing
/// one crew's turns beside another crew's kit — or another band's place in the line — is the same
/// defect one field over.
#[allow(clippy::struct_field_names)]
struct BuildEstimateSlots<'a> {
    turns: &'a mut Option<BuildTurns>,
    /// **Why the entry is blocked**, [`crate::intensification::BuildGate::Open`] when it is not — it rides the same winner
    /// as the countdown beside it, because a cause taken from one band's queue beside a date from
    /// another's would be two answers pretending to be one.
    reason: &'a mut BuildGate,
    gear: &'a mut f32,
    position: &'a mut i32,
    /// **Where the entry is taking this source**, `None` when it is not queued — and **the legs it
    /// still has to lay** to get there. They ride the same winner as everything above, for the same
    /// reason: a destination from one band's queue beside a date from another's would be two answers
    /// pretending to be one.
    destination: &'a mut Option<RungKey>,
    legs: &'a mut Vec<crate::intensification::PublishedBuildLeg>,
}

/// The answer [`BuildEstimateSlots`] carries.
#[derive(Clone)]
struct BuildEstimate {
    turns: Option<BuildTurns>,
    reason: BuildGate,
    gear: f32,
    position: i32,
    destination: Option<RungKey>,
    legs: Vec<crate::intensification::PublishedBuildLeg>,
}

#[derive(Default)]
struct BuildEstimateClaims<K: Eq + std::hash::Hash> {
    /// The sources a **running build** has already answered for this turn.
    claimed: HashSet<K>,
}

impl<K: Eq + std::hash::Hash> BuildEstimateClaims<K> {
    /// Publish a **running build's** answer, keeping the sooner of it and whatever another crew on
    /// this source already published (rules 1–3 above).
    fn publish_running(&mut self, key: K, slots: BuildEstimateSlots<'_>, answer: BuildEstimate) {
        let first_claim = self.claimed.insert(key);
        if first_claim || is_a_sooner_estimate(answer.turns, *slots.turns) {
            *slots.turns = answer.turns;
            // **And the blocked cause with it** — same winner, for the same reason the position is.
            *slots.reason = answer.reason;
            *slots.gear = answer.gear;
            // **The place in the line rides the same winner** — a date from one band's queue beside
            // another band's position would be two answers pretending to be one.
            *slots.position = answer.position;
            // **And so do the destination and the legs**: they describe the same entry the date came
            // from, so taking them from a different band's queue would be the same defect one level
            // down — a climb quoted against a countdown that is not measuring it.
            *slots.destination = answer.destination;
            *slots.legs = answer.legs;
        }
    }

    /// Publish a **projection** — the quote for the rung this source would climb next — unless a
    /// running build on it has already answered. The cause rides with the countdown, since a
    /// projection dated behind a blocked head carries that head's sentinel.
    fn publish_projected(
        &self,
        key: &K,
        turns_slot: &mut Option<BuildTurns>,
        reason_slot: &mut BuildGate,
        turns: Option<BuildTurns>,
        reason: BuildGate,
    ) {
        if !self.claimed.contains(key) {
            *turns_slot = turns;
            *reason_slot = reason;
        }
    }
}

/// **Does `proposed` finish sooner than what is already published?** Strictly, on
/// [`EstimateStanding`]'s total order — so equal standings never displace one another and the
/// published answer cannot depend on the order the labor loop visits bands in.
fn is_a_sooner_estimate(proposed: Option<BuildTurns>, published: Option<BuildTurns>) -> bool {
    estimate_standing(proposed) < estimate_standing(published)
}

/// **THE FIVE ANSWERS, RANKED — and the whole ranking is one statement: MORE NET SUPPLY IS BETTER
/// NEWS.**
///
/// Several crews can work one source, and each quote counts only its own output
/// ([`BuildEstimateClaims`]), so the source publishes the best of them. The order below is the
/// derived `Ord` on this enum — variant order first, payload second — which makes it **total**, so
/// two equal standings compare equal and neither displaces the other:
///
/// | standing | net supply | why it sits here |
/// |---|---|---|
/// | `Finishes(n)` | `> 0` | a crew that is moving the meter is never displaced by one that is not; among them the **smaller count** wins, and for one source that is the larger net |
/// | `Holds` | `== 0` | the meter is preserved, so this crew is strictly **closer to a finish** than one losing the work |
/// | `Rots` | `< 0` | going backwards is still an answer, and the worst of the supplying three |
/// | `Blocked` | none at all | the pool is standing on a gate that refuses it, so nothing is supplied |
/// | `Silent` | — | the *absence* of an answer, which any answer beats |
///
/// **Holding above rotting is not a taste call** — it is the same monotonicity that orders the real
/// counts. A larger net supply is a sooner finish, and the three non-count states continue that
/// line past zero rather than starting a second rule.
///
/// **`Silent` must be last**, which is why this exists rather than an `Ord` on
/// `Option<BuildTurns>`: `Option`'s derived order puts `None` **first**, i.e. makes silence beat
/// every answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum EstimateStanding {
    /// A real count — the fewer turns, the better the news.
    Finishes(u32),
    /// The meter holds exactly where it is ([`BuildTurns::Holding`]).
    Holds,
    /// The meter is going backwards ([`BuildTurns::Rotting`]).
    Rots,
    /// **The head of this band's queue is refused by its own gate** ([`crate::intensification::BuildTurns::Blocked`]).
    ///
    /// **Below `Rots` and above `Silent`**, which continues the one stated rule rather than starting
    /// a second: a band that is rotting a meter is at least *supplying* something to it, a blocked
    /// queue is supplying nothing at all, and silence is the absence of an answer. So another band's
    /// rotting build is better news about this source than this band's blocked one, and any answer
    /// beats none.
    Blocked,
    /// No answer at all — nobody has promised anything on this source.
    Silent,
}

/// [`EstimateStanding`] for a published answer. An exhaustive match, so a fifth `BuildTurns` fails
/// to compile until someone states where in the order it belongs.
fn estimate_standing(estimate: Option<BuildTurns>) -> EstimateStanding {
    match estimate {
        Some(crate::intensification::BuildTurns::Turns(turns)) => EstimateStanding::Finishes(turns),
        Some(BuildTurns::Holding) => EstimateStanding::Holds,
        Some(BuildTurns::Rotting) => EstimateStanding::Rots,
        Some(crate::intensification::BuildTurns::Blocked) => EstimateStanding::Blocked,
        None => EstimateStanding::Silent,
    }
}

/// **The `plant:field` rung's build step**, factored out because the Forage arm reaches it from two
/// places — sowing a *wild/bare* patch (the take path) and sowing an *already tended* one (the managed
/// path) — and the two must not drift into different gates, rates or completion side-effects.
///
/// THE build seam: the rung supplies the accrual (`0` unless `Sow` is the rung's verb and `eligible`
/// holds); the patch owns its meter, the clamp, and ownership.
///
/// **`field_cost_multiplier` IS THIS GROUND'S PRICE FOR THE RUNG** — how much of the tile the chosen
/// crop still has to replace (`forage::patch_field_cost_multiplier`,
/// `docs/plan_standing_upkeep.md` §4.15), resolved pre-accrual by the caller. It is fixed onto the
/// patch on the turn the Field leg first takes work ([`ForagePatch::price_field_rung`]) and quoted
/// live before that, so the sheet's forecast, the leg list and the charge are one number.
///
/// `gate` is the faction's **Seed Selection** gate and nothing else, carried as a [`BuildGate`] so
/// a blocked head can name it. A lapse just stops accrual for the turn: progress is neither lost nor
/// silently switched.
///
/// **It deliberately does NOT carry the work predicate** ([`crew_is_working_the_source`]), which
/// every other build gate gained with the harvest floor (`docs/plan_harvest_floor.md` §3.2). That
/// term replaced each rung's `EcologyPhase::Thriving` gate, and rung 3 never had one — for the reason
/// that also forbids the term: **bare ground stands below every floor**, by construction, so
/// requiring a positive escapement room would make the create-from-nothing case the rung exists for
/// impossible. `floor` still paces it, so a crew stripping the ground it is sowing still builds
/// nothing.
///
/// `workers` is the crew this assignment put on the tile, and **it IS the throughput**: the crew
/// produces `workers × PER_WORKER_OUTPUT` work units a turn against the rung's fixed `work_cost`, so
/// a Sow the player under-staffed takes proportionally longer and one they over-staff finishes
/// sooner, with no cap (`docs/plan_unit_costed_work.md` §1.2).
///
/// Returns **`true` when THIS call completed the Field** — the caller clears the assignment's
/// `improvement` on that signal. Shaped like `Herd::accrue_corral`'s completion bool rather than
/// swallowing the completion into the event push, so both plant build rungs report the same thing to
/// the same seam.
#[allow(clippy::too_many_arguments)] // the rung, the gate, the actor and the feed line are all inputs
fn accrue_field(
    patch: &mut ForagePatch,
    field_rung: &RungDef,
    improvement: Option<Improvement>,
    gate: BuildGate,
    faction: FactionId,
    event_log: &mut CommandEventLog,
    tick: u64,
    tile: UVec2,
    // **The hands actually on this Sow this turn** — the band's whole builders pool when this patch
    // is the head of its queue, and zero otherwise (`docs/plan_standing_upkeep.md` §2.5).
    workers: u32,
    // **The pool the quote is struck at**, which is the full builders count whether or not this
    // entry is the one being funded: every entry in a queue is dated at the crew the head will hand
    // it when its turn comes.
    pool: u32,
    // **What one of the pool's builders' kit ADDS to its own output per turn**
    // (`BuildersBranchGear::work_per_worker`). It raises the supply; it never shrinks `sow_cost`
    // (`docs/plan_standing_upkeep.md` §4.8).
    gear_per_worker: f32,
    ladder: &LadderConfig,
    equipment: Option<&mut BandEquipment>,
    equipment_cfg: &crate::equipment_config::EquipmentConfig,
    builders_kit: &crate::equipment_config::KitChoice,
    // Where the arm records this rung's four countdown terms; the band's chain pass evaluates them
    // in queue order after the loop.
    quotes: &mut Vec<(BuildSource, BuildQuote)>,
    // **Does this patch carry a rung entry in the band's queue?** A running quote describes an
    // ENTRY (see `entry_declares_a_rung` at the call site): a Field eroded below its cost derives
    // `Sow` with nobody queued and nobody building, and quoting it would publish a confident
    // countdown for a build that is not happening.
    entry_declares_a_rung: bool,
    // **What this patch's at-risk meter is bleeding this turn** (`forage::patch_meter_rot`),
    // resolved once by the caller off the keeping just stamped — see the Cultivate arm.
    meter_rot: f32,
    // **What the `plant:field` rung costs on THIS ground**, resolved pre-accrual by the caller — see
    // this function's doc.
    field_cost_multiplier: f32,
    // **THIS ENTRY'S share of the band's material pile** (`docs/plan_standing_upkeep.md` §2.7) —
    // settled across every claim on that store before the assignment loop, so a short store stalls
    // this build *proportionally* rather than refusing it and the unbanked remainder of the crew's
    // output is wasted. `FULLY_SERVED` for a rung declaring no material, which is both plant rungs
    // on the shipped ladder.
    //
    // ⛔ **THE ENTRY'S, NEVER THE BAND'S** (`entry_material_coverage` at the call site). The
    // settlement struck its want against the **head** alone, so handing a waiting `sow` the band-wide
    // figure published `BuildTurns::Blocked` with cause `BuildGate::Materials` for a rung that
    // declares no material at all — because the *head* was a pen with an empty hurdle store — and
    // `publish_build_chain` then carried that block down the rest of the queue.
    material_coverage: f32,
) -> bool {
    let eligible = gate.holds();
    // The Sow crew's whole output — the keeping pool owes the rate whatever the builders do.
    let accrual = field_rung.build_accrual(improvement, eligible, workers, gear_per_worker)
        * material_coverage;
    // **The signed twin** — the meter takes the accrual, the countdown takes it net of the rot, so
    // *holding against the bleed* and *losing to it* stay two answers. At the **full pool**, like
    // every entry's quote.
    // **AND THE COUNTDOWN IS SCALED BY THE SAME COVERAGE THE ACCRUAL IS** — a forecast and a take
    // must not disagree (`docs/plan_standing_upkeep.md` §2.7).
    let balance = field_rung.build_balance(
        improvement,
        eligible,
        pool,
        gear_per_worker,
        meter_rot,
        material_coverage,
    );
    // **THE JOB'S PRICE, ON THIS GROUND** — the rung's declared `work_cost` at this patch's own
    // multiplier, which is what a Sow costs by how much of the tile it has to replace (§4.15).
    // **A kit still never moves it** (§4.8): gear is a term of `balance` beside it.
    let sow_cost = field_rung
        .build_cost(field_cost_multiplier)
        .expect("a rung a verb builds has a build meter");
    if accrual <= 0.0 {
        // **A Sow in flight claims the patch's estimate even when it is STALLED** — the shape the
        // other three build arms have. A stall's answer is *"no estimate"*, and it is still the
        // **queued build's** answer, so a band merely gathering this ground must not quote the next
        // rung over it.
        if entry_declares_a_rung {
            quotes.push((
                BuildSource::Patch(tile),
                BuildQuote {
                    cost: sow_cost,
                    material_coverage,
                    // **THE WHOLE CLIMB'S POSITION, not this rung's share of it.** `banked` is what
                    // `build_turns_estimate` reads to tell *"nobody has promised anything yet"* from
                    // *"a meter the player has paid into"*, and a `sow` ordered on untended ground
                    // banks its first leg **below** the Field's base — where the per-rung reading is
                    // still `RUNG_UNSTARTED` and the entry would publish "no estimate" for a build
                    // the player is watching climb. The turn count is untouched: `BuildQuote::turns`
                    // spends `banked` only as `banked + Σ legs − banked`.
                    banked: patch.ladder_position(),
                    legs: crate::forage::patch_build_legs(
                        patch,
                        RungKey::PlantField,
                        ladder,
                        field_cost_multiplier,
                    ),
                    balance,
                    gate,
                },
            ));
        }
        return false;
    }
    // **THE LEG'S PRICE IS FIXED HERE, ON THE TURN IT FIRST TAKES WORK** — idempotent, so it is
    // measured once and held for the whole leg (`ForagePatch::price_field_rung`). Below the stalled
    // return above, deliberately: a Sow that banked nothing has not started its leg, and freezing a
    // price against a basket a later Cultivate leg will still weed would quote the player one number
    // and charge another.
    // The TRANSITION, not the state — `ForagePatch::accrue_field` answers "did this call finish it",
    // so a second band cannot re-announce a Field the first one sowed.
    //
    // **The gear is charged off the METER'S DELTA, after the accrual.** `accrue_field` owns the
    // owner-lock, so a crew sowing ground another faction has claimed banks nothing and therefore
    // spends nothing — a property of the arithmetic rather than of this site re-checking the gate.
    // The three equipment arguments ride here rather than the caller charging afterwards because
    // the meter this bills against is the one this function advances.
    // **The WHOLE ladder position, not the Field rung's clamped share of it.** A `sow` on untended
    // ground climbs `plant:tended` through this same arm, and a per-rung reading is pinned at
    // `RUNG_UNSTARTED` for the whole of that leg — so a delta measured against it charges the kit
    // nothing for work it did (`.claude/rules/core_sim/equipment.md` — *wear follows the work
    // actually done*), and under-charges the boundary turn by the part of the accrual below the
    // Field's base.
    patch.price_field_rung(field_cost_multiplier, ladder);
    let position_before = patch.ladder_position();
    // **EVERY rung this turn's work crossed**, not just the Field. A `sow` on untended ground lays
    // two legs, and the tended rung completing on the way is news the player who ordered *"take it to
    // Field"* wants to see — announced on **Cultivate's** channel, because a rung's whole life reads
    // on its own verb's line (`docs/plan_standing_upkeep.md` §2.8).
    let crossed = patch.accrue_to(faction, accrual, RungKey::PlantField, ladder);
    let sown = crossed.contains(&RungKey::PlantField);
    // Recorded against the meter the accrual just moved — the quote above was struck before it, and
    // a running build's own countdown must be the post-accrual one.
    if entry_declares_a_rung {
        quotes.push((
            BuildSource::Patch(tile),
            BuildQuote {
                cost: sow_cost,
                material_coverage,
                // The whole climb's position — see the stalled quote above.
                banked: patch.ladder_position(),
                legs: crate::forage::patch_build_legs(
                    patch,
                    RungKey::PlantField,
                    ladder,
                    field_cost_multiplier,
                ),
                balance,
                gate,
            },
        ));
    }
    charge_build_wear(
        equipment,
        equipment_cfg,
        builders_kit,
        patch.ladder_position() - position_before,
    );
    for rung in &crossed {
        announce_plant_rung_built(event_log, tick, faction, *rung, tile);
    }
    sown
}

/// **A PLANT RUNG COMPLETING, ON ITS OWN VERB'S CHANNEL** — one line per rung raised, whichever
/// destination the entry that raised it named.
///
/// It exists because a two-leg `sow` completes **two** rungs, and each is a thing the player paid for
/// and wants told: *"Cultivated patch at (x, y)"* on the Cultivate channel, then *"Field sown"* on
/// Sow's. The **queue** line is separate and fires once, at the destination — see
/// `arrived_at_destination`.
fn announce_plant_rung_built(
    event_log: &mut CommandEventLog,
    tick: u64,
    faction: FactionId,
    rung: RungKey,
    tile: UVec2,
) {
    let (kind, label, action) = match rung {
        RungKey::PlantField => (
            CommandEventKind::Sow,
            format!("Field sown at ({}, {})", tile.x, tile.y),
            Improvement::Sow.as_str(),
        ),
        RungKey::PlantTended => (
            CommandEventKind::Cultivate,
            format!("Cultivated patch at ({}, {})", tile.x, tile.y),
            Improvement::Cultivate.as_str(),
        ),
        // A rung no plant verb raises — the wild floor, or a rung the animal web owns — completes
        // nothing a feed line could name.
        _ => return,
    };
    event_log.push(CommandEventEntry::new(
        tick,
        kind,
        faction,
        label,
        Some(format!(
            "status=complete action={action} x={} y={}",
            tile.x, tile.y
        )),
    ));
}

/// Layer 3b (wellbeing) — tech-gated migration: relocate-or-stay, population conserved within the
/// faction (`docs/plan_civ_wellbeing.md`). Runs in the Population stage **after** demographics so
/// morale is current. **Decoupled from `discontent_fraction`** (productivity-only): migration has its
/// own morale-scaled onset at `migration.morale_threshold` (0.25). Each band below the threshold
/// sheds `total × migration_move_fraction(morale)` people, composed mostly of working-age (the total
/// is split across brackets ∝ `bracket_size × weight`, working = 1.0, dependents =
/// `migration.dependent_weight`), who seek the highest-morale eligible same-faction band within
/// reach; found → they **relocate** (source shrinks, destination grows), none reachable → they
/// **stay** (grievance accrues faster via the trapped bonus). Morale NEVER causes faction population
/// loss.
///
/// Destinations are chosen from a single **pre-migration snapshot** of this turn's post-demographics
/// morale/brackets, and every move is computed before any is applied — so relocation is
/// order-independent (a band that receives immigrants this turn isn't re-evaluated as a fuller
/// source, and a source's outflow is unaffected by another source feeding it).
pub fn advance_population_migration(
    sim_config: Res<SimulationConfig>,
    wellbeing_config: Res<WellbeingConfigHandle>,
    tile_registry: Res<TileRegistry>,
    tiles: Query<&Tile>,
    tick: Res<SimulationTick>,
    mut event_log: ResMut<CommandEventLog>,
    // `With<ResidentBand>`: migration relocates people between real bands only — an expedition is
    // never a migration source or destination. `Option<&BandId>` for the same reason
    // `simulate_population` takes one: a band with no durable id has nothing to name a feed event
    // after (worldgen always gives one).
    mut cohorts: Query<(Entity, &mut PopulationCohort, Option<&BandId>), With<ResidentBand>>,
) {
    let wellbeing = wellbeing_config.get();
    let disc_cfg = &wellbeing.discontent;
    let mig_cfg = &wellbeing.migration;
    let width = tile_registry.width;
    let wrap = sim_config.map_topology.wrap_horizontal;

    // Movement-tech reach factor. No concrete movement/transport tech signal exists in the sim yet
    // (capability flags cover construction/industry/power/naval/air/espionage/megaprojects, none of
    // which is a mobility tier), so Phase 1 keeps this at 1.0.
    // TODO(phase2): scale by the civilization's movement/transport tech tier (design doc defers
    // concrete tiers) so advanced factions send emigrants farther.
    let movement_tech_factor = 1.0_f32;
    let reach = mig_cfg.base_reach * movement_tech_factor;
    let reach_sq = (reach * reach) as i32;
    let attractive_morale = scalar_from_f32(mig_cfg.attractive_morale);
    let min_gap = scalar_from_f32(mig_cfg.min_morale_gap);
    let dependent_weight = scalar_from_f32(mig_cfg.dependent_weight);
    let morale_threshold = scalar_from_f32(mig_cfg.morale_threshold);

    // Pre-migration snapshot: everything the destination search + would-move sizing reads. The total
    // leaving is `total × move_fraction`, split across brackets ∝ `bracket_size × weight` so the
    // headline fraction is exact while working-age dominates the composition.
    struct Band {
        entity: Entity,
        faction: FactionId,
        pos: Option<UVec2>,
        morale: Scalar,
        wants_to_move: bool,
        move_working: Scalar,
        move_children: Scalar,
        move_elders: Scalar,
    }
    let mut bands: Vec<Band> = cohorts
        .iter()
        .map(|(entity, cohort, _)| {
            let move_fraction = migration_move_fraction(cohort.morale, mig_cfg);
            // Weighted bracket masses; the total is apportioned in proportion to these.
            let w_working = cohort.working;
            let w_children = cohort.children * dependent_weight;
            let w_elders = cohort.elders * dependent_weight;
            let denom = w_working + w_children + w_elders;
            // Clamp the headline leaving amount to the weighted denominator so no bracket can be
            // over-drafted (`move_x ≤ w_x ≤ bracket_x`), preserving faction population conservation.
            // A no-op under shipped tuning (`total × max_rate ≤ denom` always), but a safety net for
            // extreme-but-valid config (e.g. a very low `dependent_weight` on a dependent-heavy band).
            let total_leaving = (cohort.total() * move_fraction).min(denom);
            let (move_working, move_children, move_elders) = if denom > scalar_zero() {
                (
                    total_leaving * w_working / denom,
                    total_leaving * w_children / denom,
                    total_leaving * w_elders / denom,
                )
            } else {
                (scalar_zero(), scalar_zero(), scalar_zero())
            };
            Band {
                entity,
                faction: cohort.faction,
                pos: tiles.get(cohort.home).ok().map(|tile| tile.position),
                morale: cohort.morale,
                wants_to_move: total_leaving > scalar_zero(),
                move_working,
                move_children,
                move_elders,
            }
        })
        .collect();
    // Bevy query iteration order is not guaranteed stable across runs/rollback, but turn
    // resolution must be deterministic. Sort by entity id so the destination tie-break
    // (first-encountered wins on a morale tie) is reproducible.
    bands.sort_by_key(|b| b.entity.to_bits());

    // For each band that wants to move (morale below the migration threshold), find the
    // highest-morale eligible same-faction band within reach.
    let mut destination_of: Vec<Option<usize>> = vec![None; bands.len()];
    for i in 0..bands.len() {
        if !bands[i].wants_to_move {
            continue;
        }
        let Some(src_pos) = bands[i].pos else {
            continue;
        };
        let mut best: Option<(usize, Scalar)> = None;
        for (j, dest) in bands.iter().enumerate() {
            if j == i || dest.faction != bands[i].faction {
                continue;
            }
            let Some(dest_pos) = dest.pos else {
                continue;
            };
            // Eligible = meaningfully happier than a bare threshold AND than the source.
            if dest.morale < attractive_morale || dest.morale <= bands[i].morale + min_gap {
                continue;
            }
            if crate::grid_utils::wrapped_distance_sq(src_pos, dest_pos, width, wrap) > reach_sq {
                continue;
            }
            if best.is_none_or(|(_, m)| dest.morale > m) {
                best = Some((j, dest.morale));
            }
        }
        destination_of[i] = best.map(|(j, _)| j);
    }

    // Accumulate per-band bracket deltas + head-count tallies from all moves (computed against the
    // snapshot), then apply in one mutating pass so relocation is order-independent.
    let mut deltas: HashMap<Entity, (Scalar, Scalar, Scalar)> = HashMap::new();
    let mut emigrated: HashMap<Entity, u32> = HashMap::new();
    let mut immigrated: HashMap<Entity, u32> = HashMap::new();
    for (i, dest) in destination_of.iter().enumerate() {
        let Some(j) = *dest else { continue };
        let src_entity = bands[i].entity;
        let dest_entity = bands[j].entity;
        let (mw, mc, me) = (
            bands[i].move_working,
            bands[i].move_children,
            bands[i].move_elders,
        );
        let moved_head = (mw + mc + me).round().to_u32();
        if moved_head == 0 {
            continue;
        }
        let src = deltas.entry(src_entity).or_default();
        src.0 -= mw;
        src.1 -= mc;
        src.2 -= me;
        let dst = deltas.entry(dest_entity).or_default();
        dst.0 += mw;
        dst.1 += mc;
        dst.2 += me;
        *emigrated.entry(src_entity).or_default() += moved_head;
        *immigrated.entry(dest_entity).or_default() += moved_head;
    }

    // Apply relocation + refresh the derived per-turn emigrant/immigrant readouts + accrue/decay
    // the grievance accumulator. Base accrual is `grievance_gain × discontent_fraction` (the 0.6
    // discontent onset, unchanged); the trapped bonus applies specifically when the band is below
    // the migration threshold (people *want* to leave) AND has no reachable destination.
    let trapped_multiplier = scalar_from_f32(disc_cfg.trapped_multiplier);
    let grievance_gain = scalar_from_f32(disc_cfg.grievance_gain);
    let grievance_decay = scalar_from_f32(disc_cfg.grievance_decay);
    let index_of: HashMap<Entity, usize> = bands
        .iter()
        .enumerate()
        .map(|(i, b)| (b.entity, i))
        .collect();
    for (entity, mut cohort, band_id) in cohorts.iter_mut() {
        cohort.last_emigrated = emigrated.get(&entity).copied().unwrap_or(0);
        cohort.last_immigrated = immigrated.get(&entity).copied().unwrap_or(0);
        if let Some(band_id) = band_id {
            // Whole people already, so this is reported the turn it happens — no accumulator.
            crate::systems::population::push_migration_events(
                &mut event_log,
                tick.0,
                cohort.faction,
                *band_id,
                cohort.last_emigrated,
                cohort.last_immigrated,
            );
        }
        if let Some((dw, dc, de)) = deltas.get(&entity) {
            cohort.working = (cohort.working + *dw).max(scalar_zero());
            cohort.children = (cohort.children + *dc).max(scalar_zero());
            cohort.elders = (cohort.elders + *de).max(scalar_zero());
            cohort.sync_size();
        }
        if cohort.discontent_fraction <= scalar_zero() {
            cohort.grievance = (cohort.grievance - grievance_decay).max(scalar_zero());
        } else {
            // Trapped = wants to migrate (morale < threshold) but nowhere reachable to go.
            let trapped = cohort.morale < morale_threshold
                && index_of
                    .get(&entity)
                    .map(|&i| destination_of[i].is_none())
                    .unwrap_or(true);
            let mult = if trapped {
                trapped_multiplier
            } else {
                scalar_one()
            };
            let gain = grievance_gain * cohort.discontent_fraction * mult;
            cohort.grievance += gain;
        }
    }
}

/// The config handles [`advance_predator_raids`] reads, bundled into one `SystemParam` (the
/// [`LaborConfigs`] idiom) so the system stays within Bevy's argument budget without silencing clippy.
/// Each is resolved to its `Arc` once at the top of the system.
#[derive(bevy::ecs::system::SystemParam)]
pub struct RaidConfigs<'w> {
    pub fauna: Res<'w, FaunaConfigHandle>,
    pub combat: Res<'w, CombatConfigHandle>,
    pub creatures: Res<'w, CreaturesConfigHandle>,
    pub equipment: Res<'w, EquipmentConfigHandle>,
}

/// **Predators Phase 1b — the raid trigger, and the Warrior role's first live consumer**
/// (`docs/plan_predators.md`). A carnivore with `aggression > 0` within `predators.raid_radius` of a
/// resident band turns on its camp; the band is defended by its **Warriors** (the head-count assigned
/// to [`LaborTarget::Warrior`]). Like the hunt-danger adapter, this builds a [`FightPayload`], resolves
/// it through the neutral combat subsystem, and applies **only the band/defender side's** casualties —
/// working-age only this phase (`wounded` is surfaced in the feed but mechanically inert, as in
/// Phase 0). Runs in the Population stage right after [`advance_labor_allocation`], so warrior counts
/// and band positions are current.
///
/// **Why the band side is TWO contingents, and why that is load-bearing** (do not "simplify" it into a
/// warriors-only side): the placeholder resolver clamps a side's losses to *its own* headcount, so a
/// side with `count 0` takes ZERO losses. A warriors-only band side would therefore give a
/// **0-warrior band zero casualties** — the exact inverse of "an under-guarded band costs it people".
/// So the band's *exposed populace* is present as its own contingent (the thing that can die, at
/// **zero attack** — it dilutes the blow and adds no offense), and the Warriors are the *additional
/// armed defenders* that add power (cutting the enemy-relative loss ratio) and shift the kill/wound
/// split toward wounded. The aggressor's engaged count is a **single** representative of the pack, a
/// deliberate Phase-1b simplification that keeps `power_enemy` modest (≈ `attack × aggression`) so a
/// handful of warriors at attack 1 can meaningfully reduce `(power_enemy / power_self)` — with the
/// whole pack engaged, warriors could never keep up and every raid would be a massacre. Scaling the
/// engaged count with pack size is a Phase-2+ refinement.
/// **One warrior crew's contingent key stem**, index-suffixed per crew: a band that could arm only
/// some of its warriors fields several runs, and two contingents sharing a key would read as one.
const WARRIOR_CONTINGENT: &str = "warrior";
/// The unarmed populace's contingent key — the people the raid can kill who are holding nothing.
const EXPOSED_CONTINGENT: &str = "person";

/// **One warrior line's contingent key**, index-suffixed — spelled once so the payload that names it
/// and the wear charge that reads its strikes back cannot disagree about the spelling.
fn warrior_contingent_key(index: usize) -> String {
    format!("{WARRIOR_CONTINGENT}#{index}")
}

/// **The raid's aggressor is a single representative of the pack** (Predators Phase 1b) — named
/// because the absorbed-damage clamp divides by it, and a bare `1.0` beside a `CombatStats` reads
/// like a multiplier rather than a head count.
const ONE_PACK_REPRESENTATIVE: f32 = 1.0;

pub fn advance_predator_raids(
    herds: Res<HerdRegistry>,
    configs: RaidConfigs,
    sim_config: Res<SimulationConfig>,
    tick: Res<SimulationTick>,
    mut event_log: ResMut<CommandEventLog>,
    tiles: Query<&Tile>,
    mut bands: Query<
        (
            Entity,
            &mut PopulationCohort,
            &mut LaborAllocation,
            Option<&mut BandEquipment>,
        ),
        With<ResidentBand>,
    >,
) {
    // Resolved once — none of these change within a turn (the hunt-danger adapter's discipline).
    let fauna = configs.fauna.get();
    let tuning = configs.combat.get().tuning();
    let person = configs.creatures.get().person();
    let equipment_cfg = configs.equipment.get();
    let raid_radius = fauna.predators.raid_radius;
    let raid_exposure = fauna.predators.raid_exposure;
    let raid_yield_forfeit_fraction = fauna.predators.raid_yield_forfeit_fraction;
    let width = sim_config.grid_size.x;
    let wrap = sim_config.map_topology.wrap_horizontal;
    let map_seed = sim_config.map_seed;
    let tick = tick.0;

    for (entity, mut cohort, mut alloc, mut band_equipment) in bands.iter_mut() {
        // Reset the per-turn raid forfeit up front — this system is its only writer, so a band that
        // is NOT raided this turn must read `0.0` rather than keep last turn's debit.
        alloc.last_raid_forfeit = 0.0;
        let Ok(band_pos) = tiles.get(cohort.current_tile).map(|t| t.position) else {
            continue;
        };
        // Working-age adults are both the defenders and the only bracket Phase-1b casualties come from,
        // so a band with none of them neither defends nor dies.
        let working_age = cohort.working.to_f32();
        if working_age <= 0.0 {
            continue;
        }
        let faction = cohort.faction;
        // Warriors can't exceed the working-age adults present; the rest of the exposed bracket is the
        // populace that stands in the pack's path (bounded by the `raid_exposure` dial).
        let warriors = alloc.workers_on(&LaborTarget::Warrior) as f32;
        let warrior_count = warriors.min(working_age);
        let exposed = raid_exposure.min((working_age - warrior_count).max(0.0));
        // **THE WARRIOR ROLE'S TOE.** The kit staffed on the Warrior row swaps the defenders' whole
        // `attack` tier the same way a spear swaps a hunter's — clubs at `6` over the bare hand's
        // `1` from the `person` roster row. It is resolved through `warrior_profile`, the *same*
        // seam and the same `attack` stat a hunt resolves through, so a weapon is a weapon whichever
        // role carries it; what keeps a spear out of a raid is the kit's `jobs` list.
        //
        // **Only the warrior contingent is armed.** The exposed populace below stays at `attack 0`
        // whatever the band's kit — they are the people who are *not* holding anything, which is the
        // whole reason they are a separate contingent.
        //
        // **AND THE CLUBS ONLY GO ROUND SO FAR** (`equipment.md` → "the partly-equipped party"). A
        // band holding three clubs and standing eight warriors up arms three of them; the other
        // five defend at the bare hand's `1`. Resolved through the same `coverage` seam the hunt
        // uses, so the two roles cannot disagree about what "the band owns three of these" means.
        let warrior_kit = alloc.kit_on(&LaborTarget::Warrior, &equipment_cfg);
        let warrior_coverage = band_equipment
            .as_deref()
            .map(|wear| equipment_cfg.coverage(&warrior_kit, warrior_count, wear));
        // One contingent per crew, best-armed first — the resolver gates each attacker/target pair
        // on that attacker's own `attack`, so an unarmed warrior run contributes exactly what it
        // would have contributed on its own rather than borrowing the clubs' tier.
        let warrior_contingents: Vec<Contingent> =
            match (&warrior_coverage, band_equipment.as_deref()) {
                (Some(coverage), Some(wear)) => coverage
                    .crews()
                    .iter()
                    .enumerate()
                    .map(|(index, crew)| Contingent {
                        kind: ContingentId(warrior_contingent_key(index)),
                        count: crew.workers,
                        profile: equipment_cfg.warrior_profile(person, &crew.kit, wear),
                    })
                    .collect(),
                // **No ledger at all is a hand-rolled fixture, not an unarmed band** — the same
                // fallback every other reader of an absent `BandEquipment` takes, and it keeps the
                // warriors one contingent at the intrinsic tier.
                _ => vec![Contingent {
                    kind: ContingentId(warrior_contingent_key(0)),
                    count: warrior_count,
                    profile: person,
                }],
            };
        // **Each line's own narrowed kit**, parallel to the contingents above — what the wear charge
        // bills, so the clubbed line pays for its blows and the bare one holds nothing to pay with.
        // Empty when the band has no ledger: a fixture band's fight wears nothing out.
        let warrior_crew_kits: Vec<crate::equipment_config::KitChoice> = warrior_coverage
            .as_ref()
            .map(|coverage| {
                coverage
                    .crews()
                    .iter()
                    .map(|crew| crew.kit.clone())
                    .collect()
            })
            .unwrap_or_default();
        // The blows each line landed this turn, scaled by what the packs could absorb. Summed across
        // every predator that raided, because a band three packs turned on swung three times.
        let mut warrior_strikes_by_crew = vec![0.0_f32; warrior_crew_kits.len()];
        let mut warrior_strikes = 0.0_f32;

        // Casualties from every raiding predator this turn are additive and order-independent, so they
        // accumulate into one cohort mutation at the end.
        let mut total_killed = 0.0f32;
        // Feed lines are DEFERRED: a casualty-causing raid also forfeits food (a band-level debit
        // computed after the loop), which is folded into the line's detail before it is pushed.
        let mut raid_lines: Vec<CommandEventEntry> = Vec::new();
        for herd in &herds.herds {
            // Only a **carnivore** raids — the diet gate.
            let Some(def) = fauna.species_by_display(&herd.species) else {
                continue;
            };
            if def.diet != Diet::Carnivore {
                continue;
            }
            // **The raid trigger** (`docs/plan_predators.md`): a carnivore raids to the extent it is
            // aggressive, so its effective attack is `attack × aggression`. A carnivore with
            // `aggression 0` does not raid at all — the gate.
            let effective_attack = def.combat.attack * def.aggression;
            if effective_attack <= 0.0 {
                continue;
            }
            // The pack must have reached the camp — a tighter reach than the prey-sensing disk.
            if crate::grid_utils::hex_distance_wrapped(herd.current_pos, band_pos, width, wrap)
                > raid_radius
            {
                continue;
            }

            // Rollback-stable seed distinct per (predator, band) pair — hash BOTH the herd id and the
            // band entity, so two predators on one band and one predator on two bands all differ. The
            // placeholder resolver ignores `seed`, but it is supplied as a real value (the hunt-danger
            // adapter's discipline).
            let mut hasher = crate::hashing::FnvHasher::new();
            std::hash::Hash::hash(&herd.id, &mut hasher);
            std::hash::Hash::hash(&entity, &mut hasher);
            let seed = map_seed ^ tick ^ std::hash::Hasher::finish(&hasher);

            let payload = FightPayload {
                sides: vec![
                    // Aggressor: a single fighting representative of the pack, at its
                    // aggression-scaled attack (defense/range unchanged).
                    Force {
                        id: ForceId(0),
                        posture: Posture::Aggressor,
                        contingents: vec![Contingent {
                            kind: ContingentId(herd.species.clone()),
                            count: 1.0,
                            profile: CombatStats {
                                attack: effective_attack,
                                ..def.combat
                            },
                        }],
                    },
                    // Defender: the band. TWO contingents (see the fn doc-comment) — the armed Warriors
                    // that add power, and the unarmed exposed folk that can die but add no offense.
                    Force {
                        id: ForceId(1),
                        posture: Posture::Defender,
                        contingents: warrior_contingents
                            .iter()
                            .cloned()
                            .chain(std::iter::once(Contingent {
                                kind: ContingentId::from(EXPOSED_CONTINGENT),
                                count: exposed,
                                profile: CombatStats {
                                    attack: 0.0,
                                    defense: person.defense,
                                    // The exposed folk are the same bodies as the warriors — they
                                    // simply have nothing to fight back with.
                                    durability: person.durability,
                                    range: person.range,
                                    // Hunters do not break off — the party chose this fight, and
                                    // whether it holds is the resolver's business, not a per-hunter
                                    // flight roll. Dynamic troop morale is a later arc (§3).
                                    wariness: person.wariness,
                                },
                            }))
                            .collect(),
                    },
                ],
                terrain: vec![TerrainContext {
                    hex: (band_pos.x, band_pos.y),
                }],
                seed,
            };
            let outcome = resolve_fight(&payload, &tuning);
            // **The clubs are charged for the BLOWS THEY LANDED, scaled by what the pack could
            // absorb** — the same rule the hunt bills its spears on
            // (`.claude/rules/core_sim/equipment.md` → "Wear follows the work actually done"), and
            // the reason `WearQuantum::Fight` is gone: a defence charged per *engagement* billed
            // the whole warrior line for a raid most of it may never have swung in.
            //
            // The pack has no [`crate::combat::DamageLedger`] of its own, so its absorbed share is
            // read from the shared clamp rather than from a bank.
            let pack_dealt: f32 = outcome
                .results
                .iter()
                .filter(|result| result.force == ForceId(0))
                .map(|result| result.damage_dealt)
                .sum();
            let pack_absorbed = crate::combat::damage_absorbed(
                pack_dealt,
                &CombatStats {
                    attack: effective_attack,
                    ..def.combat
                },
                ONE_PACK_REPRESENTATIVE,
            );
            let absorbed_share = if pack_dealt > 0.0 {
                (pack_absorbed / pack_dealt).clamp(0.0, 1.0)
            } else {
                0.0
            };
            for (index, charged) in warrior_strikes_by_crew.iter_mut().enumerate() {
                let landed: f32 = outcome
                    .results
                    .iter()
                    .filter(|result| {
                        result.force == ForceId(1)
                            && result.kind.as_str() == warrior_contingent_key(index)
                    })
                    .map(|result| result.strikes_landed)
                    .sum();
                warrior_strikes += landed * absorbed_share;
                *charged += landed * absorbed_share;
            }
            // Apply ONLY the defender side (`ForceId(1)`); the predator side is discarded (no biomass
            // take here to reconcile, but band casualties are all this phase cares about).
            let (killed_f, wounded_f) =
                outcome.results.iter().fold((0.0f32, 0.0f32), |(k, w), r| {
                    if r.force == ForceId(1) {
                        (k + r.killed, w + r.wounded)
                    } else {
                        (k, w)
                    }
                });
            if killed_f + wounded_f > 0.0 {
                total_killed += killed_f;
                // One feed line per raiding predator, pushed now. Human text names the SPECIES, never
                // the internal herd id; the detail carries the fractional truth (`wounded` is inert this
                // phase — recovery is a later slice, as in Phase 0).
                let killed_r = killed_f.round() as u32;
                raid_lines.push(CommandEventEntry::new(
                    tick,
                    CommandEventKind::PredatorRaid,
                    faction,
                    format!("A {} raid cost {} lives", def.display_name, killed_r),
                    Some(format!(
                        "killed={:.3} wounded={:.3} warriors={} species={}",
                        killed_f, wounded_f, warrior_count as u32, def.display_name
                    )),
                ));
            }
        }
        // **Raids forfeit food** (Predators Phase 3): the band's people were defending or fleeing, not
        // gathering, so a **casualty-causing** raid also costs a fraction of THIS turn's food income.
        // `advance_labor_allocation` ran earlier this Population stage and already credited that income
        // to the larder, so the forfeit is a real `LocalStore::take` debit, capped at what remains. An
        // idle raided band (no income) loses only people. Recorded as the ACTUALLY-taken amount.
        if total_killed > 0.0 {
            let income: f32 = alloc.last_yields.iter().map(|y| y.actual).sum();
            let forfeit = raid_yield_forfeit_fraction * income;
            let taken = cohort.stores.take(FOOD, scalar_from_f32(forfeit)).to_f32();
            alloc.last_raid_forfeit = taken;
            // Fold the forfeit into the raid's feed detail (the wire's `raidForfeit` is the client's
            // authoritative number; this is the human/debug line).
            for line in &mut raid_lines {
                if let Some(detail) = line.detail.as_mut() {
                    detail.push_str(&format!(" forfeit={taken:.3}"));
                }
            }
        }
        // **Charge the warrior kit AFTER the fights**, the accrue-after-take ordering every other
        // wear site uses: this turn's raids are defended at the tier they were priced with and the
        // cliff lands on the next one.
        //
        // **Gated on there having been a warrior to hold the thing.** A band with nobody on the
        // Warrior row was raided with its populace standing in the open; nothing was swung, so
        // nothing wore out — the same pairing that makes the bare-handed comparison free to run
        // everywhere else.
        if warrior_strikes > 0.0 {
            if let Some(kit) = band_equipment.as_mut() {
                for (crew_kit, strikes) in warrior_crew_kits.iter().zip(&warrior_strikes_by_crew) {
                    kit.wear_kit(
                        &equipment_cfg,
                        crew_kit,
                        crate::equipment_config::WearQuantum::Strike,
                        *strikes,
                    );
                }
            }
        }
        for line in raid_lines {
            event_log.push(line);
        }
        // One mutation per band — working-age only this phase.
        cohort.apply_combat_casualties(scalar_from_f32(total_killed));
    }
}

#[cfg(test)]
mod keeping_split_tests {
    //! The per-site keeping split, at the level the labor loop cannot reach: how many keepers a
    //! web's bill needs when its sites are worked with different tools
    //! (`docs/plan_standing_upkeep.md` §2.7).

    use super::*;

    /// A claim on `demand`, worked with the roster kit `kit_id` — the shape
    /// [`keeping_claims`] builds and the split consumes.
    fn claim(index: usize, demand: f32, kit_id: &str) -> KeepingClaim {
        KeepingClaim {
            index,
            demand,
            kit: crate::equipment_config::EquipmentConfig::builtin()
                .kit(kit_id)
                .unwrap_or_else(|| panic!("the shipped roster carries '{kit_id}'")),
            // These cases are about the SPLIT, which groups on the pair; naming no rung keeps every
            // claim in one group per kit, exactly as they were before the rung axis existed.
            rung: None,
            invested: demand,
            tiebreak: format!("{index:010}"),
        }
    }

    /// **THE CREW A WEB'S BILL NEEDS IS SUMMED OVER ITS SITES, NOT DIVIDED OUT OF ITS TOTAL.**
    ///
    /// The shedding order spends *spare* keepers before anything that costs output, so *"how many
    /// hands must stay"* has to be struck at the tools the sites are actually worked with. With one
    /// rate per web the answer was `bill ÷ that rate`; with a tool per site there is no single rate
    /// to divide by, and a hoed site and a bare one owing the same work need **different numbers of
    /// hands**.
    ///
    /// **Both halves.** The mixed pair lands strictly between the two uniform answers — which is
    /// what a per-site sum means and what any single-rate reading gets wrong in one direction or the
    /// other — and the uniform pair still lands exactly on `bill ÷ the one rate`, so the
    /// generalisation did not move the case that ships.
    #[test]
    fn a_webs_keeper_need_is_the_sum_of_each_sites_own_and_not_one_division_of_the_bill() {
        /// Two equal bills, so the only thing that can move the answer is the tool.
        const A_BILL: f32 = 3.0;
        /// Enough hands that the coverage read arms every keeper — the band's scarcity is the
        /// grouping test's subject, not this one.
        const KEEPERS: u32 = 8;

        let equipment = crate::equipment_config::EquipmentConfig::builtin();
        let stocked = BandEquipment::start_stocked_for(&equipment, KEEPERS as f32);
        let need = |kits: [&str; 2]| -> f32 {
            keeping_worker_need(
                &equipment,
                &stocked,
                crate::intensification::RungBranch::Plant,
                KEEPERS,
                &[claim(0, A_BILL, kits[0]), claim(1, A_BILL, kits[1])],
            )
        };

        let bare = need(["none", "none"]);
        let hoed = need(["tillage", "tillage"]);
        let mixed = need(["tillage", "none"]);

        assert_eq!(
            bare,
            2.0 * A_BILL / crate::intensification::PER_WORKER_OUTPUT,
            "two bare sites need their whole bill in hands — {bare}"
        );
        assert!(
            hoed < bare,
            "fixture: the hoes must actually save hands, or every comparison below is vacuous — \
             {hoed} against {bare}"
        );
        assert!(
            mixed > hoed && mixed < bare,
            "one hoed site and one bare needs strictly between the two uniform answers: {mixed} \
             against {hoed} and {bare}"
        );
        assert!(
            (mixed - (hoed + bare) / 2.0).abs() < 1e-5,
            "…and exactly each site's own need summed, which for two equal bills is the mean of \
             the uniform pair: {mixed} against {}",
            (hoed + bare) / 2.0
        );
    }

    /// **SPARE KEEPERS ARE THE HANDS THE BILL DOES NOT NEED, AND A FRACTION OF A HAND IS A HAND.**
    ///
    /// A pool does not divide into whole people, so the crew that must stay is `ceil(need)`. Rounding
    /// the other way hands the shedding order a keeper the bill is still relying on, and the source
    /// it was holding starts to rot on the turn the band merely got smaller.
    #[test]
    fn a_fractional_keeper_need_still_holds_a_whole_keeper_back() {
        const A_CREW: u32 = 4;
        assert_eq!(
            spare_keepers(A_CREW, 2.1),
            1,
            "a need of 2.1 keeps THREE hands: the tenth of a keeper is a whole person"
        );
        assert_eq!(
            spare_keepers(A_CREW, 2.0),
            2,
            "…and a need that lands on a whole number keeps exactly that many"
        );
        assert_eq!(
            spare_keepers(A_CREW, NO_UPKEEP_DEMAND),
            A_CREW,
            "a web with nothing to hold needs nobody, so every hand on the role is spare"
        );
        assert_eq!(
            spare_keepers(A_CREW, A_CREW as f32 * 2.0),
            0,
            "…and a bill bigger than the role can cover leaves nothing spare, never a wrap-around"
        );
    }
}

#[cfg(test)]
mod labor_yield_tests {

    //! Retained per-source food-yield telemetry (`LaborAllocation.last_yields`): a depletable
    //! forage patch's `sustainable = sustainable_yield(pre-take biomass) ×
    //! provisions_per_biomass × output_multiplier` (MSY-based — regrowth at the most-productive
    //! biomass K/2, so a resource at carrying capacity still reads a positive sustainable harvest;
    //! a Sustain gather skims exactly that, so `actual ≈ sustainable`); a hunt's `sustainable` uses
    //! the same formula.
    //!
    //! **Slice 8 split the two webs here, deliberately.** A *gather* is still continuous, so the plant
    //! rows keep `actual ≈ sustainable` under Sustain. A *hunt* takes **whole animals**, so its
    //! `actual` pays in lumps around that rate instead of tracking it, and comparing the two per turn
    //! is no longer the overdraw question — `SourceYield::overdraws` answers it from the policy's own
    //! escapement floor. See `SourceYield`.
    /// **The shipped EQUIPPED haul rate** — what a kitted band drags, off the sled's own tier.
    /// `labor_config`'s `hunt.per_worker_biomass_capacity` is the *bare-handed* baseline since
    /// quality tiers landed, so a fixture that wants "an ordinary band" asks the item table.
    fn equipped_haul_rate() -> f32 {
        crate::equipment_config::EquipmentConfig::builtin().equipped_reference(
            crate::equipment_config::EquipmentStat::HuntCarry,
            crate::labor_config::LaborConfig::builtin()
                .hunt
                .per_worker_biomass_capacity,
        )
    }

    /// The gather twin of [`equipped_haul_rate`] — the baskets' own tier.
    fn equipped_gather_rate() -> f32 {
        crate::equipment_config::EquipmentConfig::builtin().equipped_reference(
            crate::equipment_config::EquipmentStat::ForageCarry,
            crate::labor_config::LaborConfig::builtin()
                .forage
                .per_worker_biomass_capacity,
        )
    }
    use super::advance_labor_allocation;
    use crate::fauna;
    use crate::intensification::NO_CREW_ON_THIS_ACTIVITY;
    use crate::{FoodSiteEntry, FoodSiteRegistry};

    /// **The floor at which `intensification::learn_multiplier` is exactly ×1.0** — the food peak.
    /// Every accrual assertion below that is *not about the floor* passes it, so the call reads the
    /// crew's own output rather than a floor's fraction of it.
    const FOOD_PEAK_FLOOR: f32 = crate::fauna::MSY_BIOMASS_FRACTION;

    use crate::components::{
        BuildJob, BuildSource, Improvement, LaborAllocation, LaborAssignment, LaborTarget,
        LocalStore, MoraleCause, PopulationCohort, SourcePriority, SourceYield, TakeSelection,
        Tile,
    };
    use crate::fauna::{
        forecast_expected_take, hunt_forecast, sustainable_yield, EcologyPhase, Herd, HerdRegistry,
        SourceYieldForecast, HERDING_DISCOVERY_ID, PENNING_DISCOVERY_ID,
    };
    use crate::fauna_config::{FaunaConfigHandle, SizeClass};
    use crate::flora_config::FloraConfig;
    use crate::food::{FoodModule, FoodModuleTag, FoodSiteKind};
    use crate::forage::patch_ecology;
    use crate::forage::{
        advance_forage_regrowth, forage_forecast, CULTIVATION_DISCOVERY_ID,
        SEED_SELECTION_DISCOVERY_ID,
    };
    use crate::forage::{ForagePatch, ForageRegistry};
    use crate::intensification::{
        LadderConfig, LadderConfigHandle, RungKey, NO_UPKEEP_DEMAND, RUNG_COST_UNSCALED,
    };
    use crate::labor_config::LaborConfigHandle;
    use crate::orders::FactionId;
    use crate::resources::{
        CommandEventLog, DiscoveryProgressLedger, FactionInventory, SimulationConfig,
        SimulationTick, TileRegistry,
    };
    use crate::scalar::{scalar_from_f32, scalar_one, scalar_zero};
    use crate::wellbeing_config::WellbeingConfigHandle;
    use crate::NO_IMPROVEMENT_UNDERWAY;
    use bevy::math::UVec2;
    use bevy::prelude::{Entity, World};
    use bevy_ecs::system::RunSystemOnce;
    use sim_runtime::TerrainType;

    const HERD_ID: &str = "game_test";
    const CAP: f32 = 100.0;
    /// One test animal (slice 8). Deliberately **big enough to bind**: at `CAP = 100` the Sustain
    /// escapement at full capacity is 50, so a 5-unit body quantises the take to at most 10 animals
    /// and a lightly-staffed crew genuinely rounds down. A `1.0` here would have made every take
    /// effectively continuous again and quietly stopped these forecast==actual sweeps from covering
    /// the quantiser at all.
    const TEST_GAME_BODY_MASS: f32 = 5.0;
    /// The faction every `spawn_band` band belongs to in this harness.
    const BAND_FACTION: FactionId = FactionId(0);
    /// Whole workers on each assignment: large enough that forage yields clearly and the hunt's
    /// per-worker biomass cap never binds (so a Sustain take is set by the regrowth ceiling).
    const WORKERS: u32 = 10;

    /// **How far a build walk is allowed to run per turn of accrual it needs.** A build's crew is
    /// its whole throughput, but it accrues only on turns its floor leaves something standing, so a
    /// walk bounded at the accrual's own turn count would race the harness's take crew. Generous by
    /// design: it exists to stop a broken fixture looping forever, not to time anything.
    const WALK_TURNS_PER_BUILD_TURN: u32 = 16;

    /// **What THIS harness's crew produces on `rung` in one turn**, in work units — every build
    /// assignment below staffs [`WORKERS`], and since the crew *is* the throughput
    /// (`docs/plan_unit_costed_work.md` §1.2) a build-length assertion computed at any other head
    /// count would describe a build nobody here is running. The retired `full_crew` helper existed
    /// because the accrual was capped at the rung's own crew; it is not any more.
    fn build_work_per_turn(
        rung: &crate::intensification::RungDef,
        _floor: f32,
        source_measure: f32,
    ) -> f32 {
        rung.build_accrual(
            rung.verb_improvement(),
            true,
            the_harness_build_crew(rung, source_measure),
            // **The harness carries no gear**, so every figure it records is a BARE pool's — it
            // measures the ladder's own pacing, not a kit's.
            crate::intensification::NO_BUILD_GEAR,
        )
    }

    /// **THE BUILD CREW A PLANT FIXTURE STAFFS** — [`WORKERS`] hands plus `plant:tended`'s own keeper
    /// count, the padding [`builders_above_the_rate`] explains.
    fn plant_builders(world: &World, key: RungKey) -> u32 {
        let ladder = world.resource::<LadderConfigHandle>().get();
        the_harness_build_crew(ladder.rung(key), harness_patch_load(world))
    }

    /// **The keepers that exactly cover a PLANT rung's demand** at the harness patch's own
    /// tender-load — what a fixture puts on the band's `agriculture` role so the meter it is
    /// building is **held** while it is raised (§4.6a). The animal twin is
    /// [`the_harness_keeping_crew`]; it is a second function rather than a `load` argument on that
    /// one because the two loads are two different measures (a flock's head count against the
    /// ground's own `K`), and a fixture picking the wrong one would staff a plausible number that
    /// covers nothing.
    fn the_harness_plant_keeping_crew(world: &World, key: RungKey) -> u32 {
        let load = harness_patch_load(world);
        let ladder = world.resource::<LadderConfigHandle>().get();
        ladder.rung(key).upkeep_crew_needed(load)
    }

    /// **The harness patch's own tender-load** — the measure the plant rungs' maintenance rate
    /// scales by (`forage::patch_tender_loads`), the plant twin of [`harness_herd_load`]. Resolved
    /// off [`SOURCE_BIOME`]'s own `K` rather than assumed to be one, because this harness stands on
    /// `PrairieSteppe` and not on the reference tile: thin ground presents well under one load, and
    /// that is the whole point of the measure.
    fn harness_patch_load(world: &World) -> f32 {
        let labor = world.resource::<LaborConfigHandle>().get();
        crate::forage::patch_tender_loads(labor.forage.capacity_for(SOURCE_BIOME), &labor.forage)
    }

    /// **THE BUILD CREW AN ANIMAL FIXTURE STAFFS** — the same [`WORKERS`] the plant fixtures state,
    /// with nothing added for the rung's rate. See [`the_harness_build_crew`] for why the padding
    /// went: a build crew supplies none of the rate (§4.6a), so a head count is a head count on both
    /// webs. The keeping a fixture staffs beside it is [`the_harness_keeping_crew`].
    fn animal_builders(_world: &World, _key: RungKey) -> u32 {
        WORKERS
    }

    /// **THE KEEPERS THAT EXACTLY COVER THIS RUNG'S DEMAND** at the harness herd's own load — what a
    /// fixture puts on the band's `husbandry` role so the meter it is building is **held** while it
    /// is raised (§4.6a). It padded a build crew until that slice; it staffs the keeping now.
    fn the_harness_keeping_crew(world: &World, key: RungKey) -> u32 {
        let load = harness_herd_load(world);
        let ladder = world.resource::<LadderConfigHandle>().get();
        ladder.rung(key).upkeep_crew_needed(load)
    }

    /// **The harness herd's own keeper load** — the measure the animal rungs' maintenance rate
    /// scales by (`fauna::herd_keeper_load`). The plant fixtures' twin is [`harness_patch_load`].
    fn harness_herd_load(world: &World) -> f32 {
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let registry = world.resource::<HerdRegistry>();
        registry
            .herds
            .first()
            .map_or(crate::fauna::ONE_KEEPER_LOAD, |herd| {
                fauna::herd_keeper_load(herd, &fauna)
            })
    }

    /// **THE HARNESS'S BUILD CREW: [`WORKERS`] hands, and nothing added for the rung's rate.**
    ///
    /// It carried `+ upkeep_crew_needed` while the rate was a tax on building — a fixed crew fell
    /// below the threshold on any herd of size and the build never ran. §4.6a deleted that threshold:
    /// a build crew supplies none of the rate, so the padding was correcting for nothing and only
    /// made the crew larger than the head count these fixtures state. Every pace assertion divides by
    /// [`build_work_per_turn`], which reads this same crew.
    fn the_harness_build_crew(
        _rung: &crate::intensification::RungDef,
        _source_measure: f32,
    ) -> u32 {
        WORKERS
    }

    /// **Turns this harness's crew needs to finish `rung`'s whole job**, `ceil(cost / work)`. Turns
    /// are an output now, so a test that wants "run it to completion" has to *divide*, and a bare
    /// `1.0 / rate` no longer means anything.
    fn turns_to_finish(
        rung: &crate::intensification::RungDef,
        floor: f32,
        cost_multiplier: f32,
        source_measure: f32,
    ) -> u32 {
        crate::intensification::build_turns_remaining(
            rung.build_cost(cost_multiplier)
                .expect("a rung this harness builds has a build meter"),
            crate::intensification::RUNG_UNSTARTED,
            build_work_per_turn(rung, floor, source_measure),
        )
        .expect("a staffed build finishes")
    }
    /// **ONE WHOLE TURN OF THE HARNESS: the Logistics passes, in stage order, then the labour pass.**
    ///
    /// `advance_labor_allocation` writes two accounts of the keeping and writes them differently —
    /// `upkeep_supplied` **accumulates** across the bands working a source, `upkeep_demanded` is
    /// stamped **first-write-wins** — and both are wiped a whole stage earlier by the two decay
    /// passes. A harness that runs the labour pass twice without them therefore measures a **doubled
    /// supply against one turn's bill**, which the pass itself now refuses to do quietly.
    ///
    /// **The regrowth belongs here too, and leaving it out stalls a plant build**: the rung-2 gate
    /// reads the escapement room, so gatherers on a patch nobody regrows pull it to their floor and
    /// the Cultivate goes ineligible after a single turn.
    ///
    /// **Both webs' passes run, whichever web the caller is exercising.** A real turn runs both, and
    /// each clears its own scratch ahead of any of its own `continue`s — so a plant harness pays
    /// nothing for the animal pass and vice versa, while a harness that grows a second source later
    /// cannot silently fall out of the clear.
    ///
    /// It is deliberately **not** a `clear_*` helper that wipes the two fields: a second producer of
    /// *"what a turn does to the keeping"* is free to disagree with the pass that really does it, and
    /// the decay these passes apply is exactly the cost a harness ought to be paying for leaving a
    /// keeping unstaffed.
    fn advance_one_turn(world: &mut World) {
        world.run_system_once(advance_forage_regrowth);
        world.run_system_once(crate::forage::advance_cultivation);
        world.run_system_once(crate::fauna::advance_husbandry);
        world.run_system_once(advance_labor_allocation);
    }

    /// The biome under the harness's food-module tile — grassland, matching the
    /// `FoodModule::SavannaGrassland` tag it carries. A forage patch's carrying capacity is the
    /// **tile's** (`forage.capacity_by_biome`, the human food web's per-biome table), so the harness
    /// must name a biome rather than read a global constant.
    const SOURCE_BIOME: TerrainType = TerrainType::PrairieSteppe;

    /// A 3×1 world with a food-module tile + a stationary game herd (given `biomass`, cap `CAP`)
    /// both anchored on tile (0,0). Returns the world and that source tile's entity.
    fn world_with_source(biomass: f32) -> (World, Entity) {
        let mut world = World::default();
        let mut config = SimulationConfig::builtin();
        config.map_topology.wrap_horizontal = false;
        world.insert_resource(config);
        world.insert_resource(FaunaConfigHandle::default());
        world.insert_resource(LaborConfigHandle::default());
        world.insert_resource(crate::flora_config::FloraConfigHandle::default());
        world.insert_resource(LadderConfigHandle::default());
        world.insert_resource(WellbeingConfigHandle::default());
        world.insert_resource(crate::combat_config::CombatConfigHandle::default());
        world.insert_resource(crate::creatures_config::CreaturesConfigHandle::default());
        world.insert_resource(crate::equipment_config::EquipmentConfigHandle::default());
        world.insert_resource(crate::materials_config::MaterialsConfigHandle::default());
        world.insert_resource(crate::recipes_config::RecipesConfigHandle::default());
        world.insert_resource(FactionInventory::default());
        world.insert_resource(DiscoveryProgressLedger::default());
        world.insert_resource(CommandEventLog::default());
        world.insert_resource(SimulationTick::default());
        // **An empty road ledger is the shipped turn-1 state** — no traffic has run anywhere yet —
        // so this harness's keeping numbers are the roadless reading they have always been.
        world.insert_resource(crate::routes::RoadRegistry::default());

        let tiles: Vec<Entity> = (0..3)
            .map(|x| {
                world
                    .spawn(Tile {
                        position: UVec2::new(x, 0),
                        terrain: SOURCE_BIOME,
                        ..Default::default()
                    })
                    .id()
            })
            .collect();
        let source_tile = tiles[0];
        world.entity_mut(source_tile).insert(FoodModuleTag {
            module: FoodModule::SavannaGrassland,
            seasonal_weight: 1.0,
            kind: FoodSiteKind::SavannaTrack,
        });
        world.insert_resource(TileRegistry {
            tiles,
            width: 3,
            height: 1,
        });
        // **The source tile is a GATHERING SITE.** Every plant rung carries
        // `requires_gathering_site`, so a fixture that omits this makes the one worked tile
        // unworkable and quietly zeroes every yield these tests measure. It is stated rather than
        // defaulted for exactly that reason — an empty registry is a valid map (all barren), so no
        // fallback can tell "no sites here" from "the fixture forgot".
        world.insert_resource(FoodSiteRegistry::new(vec![FoodSiteEntry {
            position: UVec2::new(0, 0),
            module: FoodModule::SavannaGrassland,
            kind: FoodSiteKind::SavannaTrack,
            seasonal_weight: 1.0,
        }]));

        let fauna = world.resource::<FaunaConfigHandle>().get();
        let mut herd = Herd::new(
            HERD_ID.to_string(),
            "Test Game".to_string(),
            SizeClass::Small,
            vec![UVec2::new(0, 0)],
            biomass,
            CAP,
            0.0,
            fauna.ecology.regrowth_rate,
            TEST_GAME_BODY_MASS,
        );
        herd.refresh_ecology_phase(&fauna);
        drop(fauna);
        let mut registry = HerdRegistry::default();
        registry.herds.push(herd);
        world.insert_resource(registry);

        // Depletable forage patch on the source tile, seeded at the **post-regrowth steady state a
        // Sustain gather holds it at**: `K/2` (Sustain's escapement floor) plus the one turn of
        // regrowth Logistics adds before Population takes. These unit tests run
        // `advance_labor_allocation` alone, so the regrowth has to be in the fixture — seating the
        // patch *at* `K/2` would leave a Sustain gather nothing standing above its floor and every
        // row would read `0`.
        let forage_cfg = world.resource::<LaborConfigHandle>().get();
        let patch_cap = forage_cfg.forage.capacity_for(SOURCE_BIOME);
        let mut patch = ForagePatch::new(UVec2::new(0, 0), patch_cap);
        patch.biomass = patch_cap * crate::fauna::MSY_BIOMASS_FRACTION
            + sustainable_yield(
                patch_cap * crate::fauna::MSY_BIOMASS_FRACTION,
                patch_cap,
                &forage_cfg.forage.ecology,
            );
        patch.refresh_ecology_phase(&forage_cfg.forage.ecology);
        drop(forage_cfg);
        let mut forage_registry = ForageRegistry::default();
        forage_registry.patches.insert(UVec2::new(0, 0), patch);
        world.insert_resource(forage_registry);

        (world, source_tile)
    }

    /// A content band (morale 1 → output multiplier 1.0) on `tile` with the given assignments.
    /// **STAND A BAND'S BUILDERS ON ONE SOURCE** — the 6b shape of what a fixture used to say with
    /// `improvement: Some(verb)` and a build crew on the source row.
    ///
    /// It is two facts now and both are needed, which is the point: the **`builders` role row** is
    /// where the hands are, and the **queue entry** is what they are raising. A fixture that staffed
    /// only the first would find the pool idle, and one that queued only the second would find it
    /// unfunded — neither is a bug in the sim.
    ///
    /// Appends the role row (so it lands at the tail, after the source rows the caller staffed) and
    /// declares the build on the source those rows name.
    fn declare_build(
        world: &mut World,
        band: Entity,
        source: BuildSource,
        declared: BuildJob,
        builders: u32,
    ) {
        let mut allocation = world
            .get_mut::<LaborAllocation>(band)
            .expect("the fixture band has an allocation");
        allocation.assignments.push(LaborAssignment {
            target: LaborTarget::Builders,
            // ⛔ **A `builders` ROW CARRIES NO KIT AT ALL** since §4.7a ②: the builders' kit is a
            // property of the queue ENTRY, and `assign_labor` refuses a token here. The isolation
            // below rides the entry instead.
            kit: None,
            workers: builders,
            priority: SourcePriority::default(),
            upkeep_kit: None,
        });
        assert!(
            allocation.enqueue_build(source.clone(), declared),
            "fixture: a build is declared on a source the band already works"
        );
        // ⛔ **THE HARNESS'S BUILDERS GO OUT BARE, AND THAT IS AN ISOLATION RATHER THAN A DEFAULT.**
        // An absent kit means *derive per entry*, and the roster's answer — `tillage` on a plant
        // build, `hurdling` on an animal one — adds `+0.5` work **per covered worker per turn** on
        // top of their own hands. A start-stocked band holds
        // `ceil(workers × start_stock_fraction)` of each tool, so at [`WORKERS`] hands the whole
        // pool is armed and delivers `10 × 1.5 = 15` a turn against `10`: every pace fixture below
        // would run half again as fast as the number it asserts.
        //
        // Naming `none` **on the entry** holds the gear axis at its identity so these fixtures
        // measure the *meter*, exactly as `FaunaConfig::without_retreat` holds the retreat at its
        // identity across the hunt suites. **The geared default has its own tests** —
        // `the_builders_pool_derives_its_kit_from_the_head_entry`, and
        // `equipment_config::tests::a_build_tool_serves_its_own_web_and_two_of_them_do_not_compound`.
        assert!(
            allocation.set_build_entry_kit(&source, Some(bare_builders())),
            "fixture: the entry just declared takes the bare kit"
        );
    }

    /// **The roster's empty kit** — every predicate reads false, so a party carrying it runs at the
    /// unequipped tiers throughout and spends no durability on anything.
    fn bare_builders() -> crate::equipment_config::KitChoice {
        crate::equipment_config::EquipmentConfig::builtin()
            .kit("none")
            .expect("the shipped roster carries the empty kit")
    }

    /// [`declare_build`]'s plant half, by tile.
    fn declare_patch_build(
        world: &mut World,
        band: Entity,
        tile: bevy::math::UVec2,
        declared: Improvement,
        builders: u32,
    ) {
        declare_build(
            world,
            band,
            BuildSource::Patch(tile),
            BuildJob::Rung(declared),
            builders,
        );
    }

    /// [`declare_build`]'s animal half, by herd id.
    fn declare_herd_build(
        world: &mut World,
        band: Entity,
        herd_id: &str,
        declared: Improvement,
        builders: u32,
    ) {
        declare_build(
            world,
            band,
            BuildSource::Herd(herd_id.to_string()),
            BuildJob::Rung(declared),
            builders,
        );
    }

    /// **A PILE OF HURDLES ON A FIXTURE BAND** — enough that a pen build and a pen's own keeping are
    /// never store-bound (`docs/plan_standing_upkeep.md` §4.9 item 12).
    ///
    /// **A fixture measuring the LADDER has to state this**, because the `animal:pen` rung eats
    /// `hurdles` on both terms now: a bare `LocalStore` covers `0` of the pile, the build's coverage
    /// is `0`, and a harness written to measure pacing measures a stall it staged itself. It is the
    /// same discipline `seed_gathering_site` imposes on the plant web — a fixture must describe a
    /// world the sim can produce.
    ///
    /// The reading is the recipe's own output, so the batch a fixture holds is the batch a bench
    /// would have made.
    fn stock_pen_materials(world: &mut World, band: Entity) {
        const AMPLE_HURDLES: f32 = 1_000.0;
        let materials = crate::materials_config::MaterialsConfig::builtin();
        let recipes = crate::recipes_config::RecipesConfig::builtin();
        let characteristics = recipes
            .recipes()
            .find_map(|(_, recipe)| {
                recipe
                    .outputs
                    .iter()
                    .find(|output| output.material_id() == Some(PEN_MATERIAL))
                    .map(|output| output.characteristics.clone())
            })
            .expect("the shipped book makes the pen's material");
        let band_key = materials
            .band_key(PEN_MATERIAL, &characteristics)
            .expect("the shipped roster rates the pen's material");
        world
            .get_mut::<PopulationCohort>(band)
            .expect("the fixture band exists")
            .stores
            .deposit_material(
                PEN_MATERIAL,
                band_key,
                scalar_from_f32(AMPLE_HURDLES),
                &characteristics,
            );
    }

    /// The material the `animal:pen` rung eats, on both its build pile and its upkeep rate.
    const PEN_MATERIAL: &str = "hurdles";

    fn spawn_band(world: &mut World, tile: Entity, assignments: Vec<LaborAssignment>) -> Entity {
        world
            .spawn((
                PopulationCohort {
                    home: tile,
                    current_tile: tile,
                    size: 30,
                    children: scalar_zero(),
                    // **Sized to whatever the fixture staffs.** Every row draws on one band, so a
                    // fixed pool would let `normalize` trim the tail — the very row under
                    // measurement — and the fixture would report a stall it staged itself
                    // (`docs/plan_standing_upkeep.md` §2.5).
                    working: scalar_from_f32(
                        assignments
                            .iter()
                            .map(|assignment| assignment.staffed_total())
                            .sum::<u32>()
                            .max(100) as f32,
                    ),
                    elders: scalar_zero(),
                    stores: LocalStore::new(),
                    morale: scalar_one(),
                    last_food_consumption: 0.0,
                    last_turn_food_transfers: Default::default(),
                    last_turn_fodder_transfers: Default::default(),
                    last_morale_delta: scalar_zero(),
                    last_morale_cause: MoraleCause::None,
                    last_morale_contributions: Default::default(),
                    last_fertility_factors: Default::default(),
                    discontent_fraction: scalar_zero(),
                    grievance: scalar_zero(),
                    last_emigrated: 0,
                    last_immigrated: 0,
                    age_turns: 0,
                    generation: 0,
                    faction: FactionId(0),
                    knowledge: Vec::new(),
                    migration: None,
                },
                LaborAllocation {
                    assignments,
                    ..Default::default()
                },
            ))
            .id()
    }

    /// (a) both a Forage and a Hunt source capture `actual > 0`; (b) the hunt's `sustainable` equals
    /// the MSY-based `sustainable_yield` value at the pre-take biomass; (c) forage
    /// `sustainable ≡ actual`.
    ///
    /// **RETARGETED IN SLICE 8 on both the start state and the hunt assertion.** It used to start the
    /// herd at *exactly* `CAP * 0.5` ("half cap → clear positive regrowth") and assert the Sustain
    /// take skimmed exactly that regrowth. Both halves were flow-model artifacts:
    /// - `K/2` **is** the Sustain escapement point, so a herd standing there spares **nothing** — the
    ///   fixture was seeding the one biomass at which the hunt correctly takes `0` and then asserting
    ///   it took something. Started above the point, so the herd genuinely has animals to spare.
    /// - `actual ≈ sustainable` is no longer what Sustain means. The take is whole animals off the
    ///   escapement, so it pays in **lumps** around the long-run MSY rate rather than tracking it turn
    ///   by turn. `sustainable` is still asserted to be that honest rate — it is just no longer the
    ///   same question as "did this overdraw", which `overdraws` now answers directly.
    #[test]
    fn forage_and_sustain_hunt_capture_yields() {
        // Above the escapement point, so the herd has whole animals to spare this turn.
        let start = CAP * 0.9;
        let (mut world, tile) = world_with_source(start);
        let band = spawn_band(
            &mut world,
            tile,
            vec![
                LaborAssignment {
                    target: LaborTarget::Forage {
                        tile: UVec2::new(0, 0),
                        floor: 0.5,
                        species: None,
                        take_species: TakeSelection::EVERYTHING,
                    },
                    workers: WORKERS,
                    kit: None,
                    priority: SourcePriority::default(),
                    upkeep_kit: None,
                },
                LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: HERD_ID.to_string(),
                        floor: 0.5,
                    },
                    workers: WORKERS,
                    kit: None,
                    priority: SourcePriority::default(),
                    upkeep_kit: None,
                },
            ],
        );

        // Expected hunt sustainable = one turn's net regrowth at the PRE-take biomass, in provisions
        // (output multiplier is 1.0 at morale 1).
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let expected_sustainable =
            sustainable_yield(start, CAP, &fauna.ecology) * fauna.hunt.provisions_per_biomass;
        drop(fauna);

        // **No bank to seed — the fixture's own STOCK is what lands the animal.** This used to prime
        // `Herd::hunt_credit` to one body so turn one paid, back when the resident take was a banked
        // rate; the take is a stock now (`docs/plan_harvest_floor.md` §1), so the seeding was inert and
        // the comment described a mechanism the path no longer reads. The herd stands at `CAP * 0.9`,
        // which leaves 40 biomass above the food peak — eight whole bodies — so the take is a kill
        // turn by construction rather than by priming.
        world.run_system_once(advance_labor_allocation);

        let alloc = world.get::<LaborAllocation>(band).unwrap();
        assert_eq!(alloc.last_yields.len(), 2, "one yield row per assignment");
        let forage = alloc.last_yields[0].clone();
        let hunt = alloc.last_yields[1].clone();
        assert!(forage.actual > 0.0, "forage produced food: {forage:?}");
        assert!(hunt.actual > 0.0, "hunt produced food: {hunt:?}");
        // Depletable forage (§0-ii): a Sustain gather under the binding regrowth ceiling skims
        // exactly one turn's net regrowth, so `actual ≈ sustainable` (no over-forage flag).
        assert!(
            (forage.actual - forage.sustainable).abs() < 1e-4,
            "sustain forage skims the regrowth → actual ≈ sustainable: {} vs {}",
            forage.actual,
            forage.sustainable
        );
        assert!(
            forage.actual <= forage.sustainable + 1e-4,
            "a Sustain forage draw must not over-forage: {forage:?}"
        );
        assert!(
            (hunt.sustainable - expected_sustainable).abs() < 1e-6,
            "hunt sustainable = net regrowth × provisions_per_biomass: {} vs {}",
            hunt.sustainable,
            expected_sustainable
        );
        // A Sustain hunt is escapement to K/2: it is sustainable **by construction** (it cannot land
        // the herd below its most-productive biomass), whatever this turn's lump happens to be.
        assert!(
            !hunt.overdraws,
            "a Sustain hunt never overdraws — it stops at the MSY point: {hunt:?}"
        );
        assert!(
            !forage.overdraws,
            "a Sustain gather never overdraws: {forage:?}"
        );
    }

    /// An Eradicate hunt near carrying capacity overdraws the herd's meagre regrowth, so the captured
    /// telemetry reads `actual > sustainable` — the leading overhunting signal.
    #[test]
    fn overdraw_reads_actual_above_sustainable() {
        let start = CAP * 0.9; // near cap → small regrowth, so any real take overdraws.
        let (mut world, tile) = world_with_source(start);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.0,
                },
                workers: WORKERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let expected_sustainable =
            sustainable_yield(start, CAP, &fauna.ecology) * fauna.hunt.provisions_per_biomass;
        drop(fauna);

        world.run_system_once(advance_labor_allocation);

        let hunt = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();
        assert!(
            (hunt.sustainable - expected_sustainable).abs() < 1e-6,
            "sustainable pinned to the pre-take net regrowth"
        );
        assert!(
            hunt.actual > hunt.sustainable,
            "an Eradicate overdraw reads actual > sustainable: {} vs {}",
            hunt.actual,
            hunt.sustainable
        );
    }

    /// Regression (Phase 0 bug): a herd AT carrying capacity used to yield 0 under a Sustain hunt
    /// (logistic regrowth is 0 at K), leaving a full herd stuck. Constant escapement answers that
    /// case directly — a full herd is **all** surplus above `K/2` — so it stays huntable, and the
    /// harvest lands it exactly on its most productive biomass and never below.
    #[test]
    fn sustain_hunt_at_capacity_yields_its_surplus_and_stops_at_the_floor() {
        let start = CAP; // full herd — the old net_biomass_delta(K) == 0 bug.
        let (mut world, tile) = world_with_source(start);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.5,
                },
                workers: WORKERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let expected_sustainable =
            sustainable_yield(start, CAP, &fauna.ecology) * fauna.hunt.provisions_per_biomass;
        drop(fauna);

        world.run_system_once(advance_labor_allocation);

        let hunt = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();
        assert!(
            hunt.sustainable > 0.0,
            "a herd at carrying capacity must stay sustainably huntable: {hunt:?}"
        );
        assert!(
            (hunt.sustainable - expected_sustainable).abs() < 1e-6,
            "sustainable = MSY × provisions_per_biomass: {} vs {}",
            hunt.sustainable,
            expected_sustainable
        );
        // **The first harvest is the accumulated stock, and it is honestly larger than one turn's
        // regrowth** (`docs/plan_harvest_floor.md` §1). `sustainable` still reports the long-run MSY
        // line, so `actual > sustainable` here is not an overdraw and must not be read as one — the ⚠
        // is `overdraws`, a fact about the stance's FLOOR.
        assert!(
            hunt.actual > hunt.sustainable,
            "a full herd hands over its standing surplus, not a rate: {hunt:?}"
        );
        assert!(!hunt.overdraws, "Sustain never overdraws: {hunt:?}");

        // **And it stops dead on the floor.** No `advance_herds` here, so the herd never regrows:
        // every later turn takes exactly nothing, because nothing stands above `K/2`. That is the
        // whole of "Sustain cannot draw a herd below its most productive biomass".
        let floor = CAP * crate::fauna::MSY_BIOMASS_FRACTION;
        for _ in 0..8 {
            world.run_system_once(advance_labor_allocation);
        }
        let biomass = world
            .resource::<HerdRegistry>()
            .find(HERD_ID)
            .unwrap()
            .biomass;
        assert!(
            biomass >= floor - TEST_GAME_BODY_MASS && biomass < floor + TEST_GAME_BODY_MASS,
            "a Sustain-hunted herd settles ON its escapement floor ({floor}), within one body: \
             {biomass}"
        );
        let last = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();
        assert_eq!(
            last.actual, 0.0,
            "at the floor there is nothing standing above it to take: {last:?}"
        );
        assert!(!last.overdraws, "Sustain never overdraws: {last:?}");
    }

    use crate::components::FOOD;

    /// The shipped `combat_config.forecast_range_sigmas` — the reported band's width. These seeds
    /// assert on `workers_needed` and the scalar take, never on the band, so the value is inert
    /// here; it is named rather than a bare literal because it is a config lever.
    const SHIPPED_FORECAST_RANGE_SIGMAS: f32 = 2.0;

    /// Set the source-tile forage patch cultivated (owned by faction 0) at the given biomass.
    fn cultivate_source_patch(world: &mut World, biomass: f32) {
        let forage = world.resource::<LaborConfigHandle>().get().forage.clone();
        let mut registry = world.resource_mut::<ForageRegistry>();
        let patch = registry.patches.get_mut(&UVec2::new(0, 0)).unwrap();
        patch.complete_cultivation(
            BAND_FACTION,
            &crate::intensification::LadderConfig::builtin(),
        );
        patch.owner = Some(FactionId(0));
        patch.biomass = biomass;
        // The patch's OWN curve — a tended patch's phase bands ride `patch_ecology`, exactly as the
        // live regrowth pass resolves them.
        patch.refresh_ecology_phase(&patch_ecology(patch, &forage));
    }

    /// Switch a band's (single) Forage assignment to `policy` — what the client's picker does. (The
    /// *finishing* case needs no picker since issue #420: completion retires the build verb itself.)
    fn set_forage_floor(world: &mut World, band: Entity, floor: f32) {
        let mut allocation = world
            .get_mut::<LaborAllocation>(band)
            .expect("band forages");
        let assignment = allocation
            .assignments
            .iter_mut()
            .find(|assignment| matches!(assignment.target, LaborTarget::Forage { .. }))
            .expect("a Forage assignment");
        let LaborTarget::Forage { floor: current, .. } = &mut assignment.target else {
            unreachable!("filtered to Forage above");
        };
        *current = floor;
    }

    /// Stand the source patch up as a completed **Field** (rung 3) at `biomass` — the plant twin of
    /// `Herd::corral_at`, for the tests that need a sown fixture without paying the 25-turn build.
    fn sow_source_patch(world: &mut World, biomass: f32) {
        cultivate_source_patch(world, biomass);
        let forage = world.resource::<LaborConfigHandle>().get().forage.clone();
        let mut registry = world.resource_mut::<ForageRegistry>();
        let patch = registry.patches.get_mut(&UVec2::new(0, 0)).unwrap();
        patch.complete_field(
            BAND_FACTION,
            &crate::intensification::LadderConfig::builtin(),
        );
        patch.refresh_ecology_phase(&patch_ecology(patch, &forage));
    }

    /// Set the (wild, un-cultivated) source patch's biomass and refresh its ecology phase — for the
    /// `workers_needed` overstaffing tests, which need a full patch so the per-policy biomass-fraction
    /// ceiling binds rather than the seeded half-cap stock.
    fn set_wild_patch_biomass(world: &mut World, biomass: f32) {
        let forage = world.resource::<LaborConfigHandle>().get().forage.clone();
        let mut registry = world.resource_mut::<ForageRegistry>();
        let patch = registry.patches.get_mut(&UVec2::new(0, 0)).unwrap();
        patch.biomass = biomass;
        patch.refresh_ecology_phase(&patch_ecology(patch, &forage));
    }

    /// Run a single Forage assignment (given policy) with `WORKERS` on a full patch and return the
    /// captured `workers_needed` — the throughput to invert the per-policy take into a worker count.
    fn forage_workers_needed(floor: f32) -> u32 {
        let (mut world, tile) = world_with_source(CAP);
        let patch_cap = world
            .resource::<LaborConfigHandle>()
            .get()
            .forage
            .capacity_for(SOURCE_BIOME);
        set_wild_patch_biomass(&mut world, patch_cap);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: UVec2::new(0, 0),
                    floor,
                    species: None,
                    take_species: TakeSelection::EVERYTHING,
                },
                workers: WORKERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        world.run_system_once(advance_labor_allocation);
        world.get::<LaborAllocation>(band).unwrap().last_yields[0].workers_needed
    }

    /// Overstaffing: a Sustain hunt whose take is set by the **escapement ceiling** — not labor —
    /// reports the crew that ceiling needs and no more, so `workers_needed < assigned` and the idle
    /// hands are visible.
    ///
    /// **The count is the crew that would clear the herd to its floor in one turn**
    /// (`docs/plan_harvest_floor.md` §7.6), which is bigger than the old MSY-rate count and is
    /// deliberately not clamped: it is what makes *"this crew cannot draw the herd that low"* a thing
    /// the readout can say.
    #[test]
    fn sustain_source_overstaffed_reports_fewer_workers_than_assigned() {
        // **Above the escapement point**: `K/2` is exactly where a Sustain hunt spares nothing, so the
        // old `CAP * 0.5` seeds the one biomass at which this test's premise cannot hold.
        let (mut world, tile) = world_with_source(CAP * 0.9);
        let assigned = 5;
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.5,
                },
                workers: assigned,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );

        // The crew the escapement ceiling asks for, off the same helper the sim uses.
        let expected_crew = {
            let fauna = world.resource::<FaunaConfigHandle>().get();
            let herd = world.resource::<HerdRegistry>().find(HERD_ID).unwrap();
            crate::fauna::hunt_haul_workers(
                crate::fauna::hunt_escapement_ceiling(
                    0.5,
                    herd.biomass,
                    crate::fauna::herd_capacity(herd, &fauna),
                ),
                herd.body_mass,
                equipped_haul_rate(),
            )
        };

        world.run_system_once(advance_labor_allocation);

        let hunt = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();
        assert!(
            hunt.actual > 0.0,
            "the sustain hunt produced food: {hunt:?}"
        );
        assert_eq!(
            hunt.workers_needed, expected_crew,
            "the crew is the one the escapement ceiling needs: {hunt:?}"
        );
        assert!(
            hunt.workers_needed < assigned,
            "the source is overstaffed (extra workers idle): {hunt:?}"
        );
    }

    /// The other extreme: when worker throughput is the binding constraint (few workers, a high
    /// biomass-fraction Eradicate ceiling), every assigned worker was productive → `workers_needed ==
    /// assigned` (no overstaffing).
    #[test]
    fn labor_bound_take_reports_all_assigned_workers_needed() {
        let (mut world, tile) = world_with_source(CAP);
        let cfg = world.resource::<LaborConfigHandle>().get();
        let patch_cap = cfg.forage.capacity_for(SOURCE_BIOME);
        let capacity = equipped_gather_rate();
        drop(cfg);
        set_wild_patch_biomass(&mut world, patch_cap); // full patch.
        let assigned = 2;
        // The scenario is labor-bound iff worker throughput is below the stance's escapement ceiling.
        // Eradicate's floor is `0`, so on a full patch that ceiling is the whole standing crop.
        assert!(
            assigned as f32 * capacity < patch_cap,
            "test precondition: the take must be labor-bound, not ceiling-bound"
        );
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: UVec2::new(0, 0),
                    floor: 0.0,
                    species: None,
                    take_species: TakeSelection::EVERYTHING,
                },
                workers: assigned,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );

        world.run_system_once(advance_labor_allocation);

        let forage = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();
        assert_eq!(
            forage.workers_needed, assigned,
            "a labor-bound take needs every assigned worker: {forage:?}"
        );
    }

    /// A deeper floor needs more workers on the **same** resource: Deplete/Eradicate leave less
    /// standing, so more of the crop is takeable and their inverted worker count exceeds Sustain's on
    /// identical full patches.
    #[test]
    fn deplete_and_eradicate_need_more_workers_than_sustain() {
        let sustain = forage_workers_needed(0.5);
        let deplete = forage_workers_needed(0.15);
        let eradicate = forage_workers_needed(0.0);
        assert!(
            deplete > sustain,
            "deplete's larger take needs more workers: {deplete} vs {sustain}"
        );
        assert!(
            eradicate > sustain,
            "eradicate's larger take needs more workers: {eradicate} vs {sustain}"
        );
        assert!(
            eradicate >= deplete,
            "eradicate's ceiling is ≥ deplete's: {eradicate} vs {deplete}"
        );
    }

    /// A tended (cultivated) patch and a corralled herd both pay out, and each reports an honest
    /// staffing need — **but they no longer report the same KIND of need**, and that is the point.
    ///
    /// The name's original claim (`workers_needed == 1` for both, "maintenance labor, not scaling
    /// gather") is dead twice over: slice 7 retired `TENDED_SOURCE_WORKERS_NEEDED = 1` for the payout,
    /// and slice 8 gave the pen a **standing, herd-sized herder demand**. What the pen reports now is
    /// [`source_crew_needed`] — **one crew sized by whichever of its two jobs binds**: enough hands to
    /// *mind* the heads (`ceil(animals / animals_per_herder)`) **and** to *haul* the meat
    /// (`ceil(take / per_worker_throughput)`). Herding is per head, hauling is per biomass, so neither
    /// term dominates across the roster — this fixture's pen happens to be **haul**-bound.
    #[test]
    fn tended_patch_and_corral_report_their_staffing_need() {
        let (mut world, tile) = world_with_source(CAP);
        let world_labor = world.resource::<LaborConfigHandle>().get();
        let patch_cap = world
            .resource::<LaborConfigHandle>()
            .get()
            .forage
            .capacity_for(SOURCE_BIOME);
        cultivate_source_patch(&mut world, patch_cap);
        // Pen the herd in place (Rung 1c) so a Hunt assignment tends rather than hunts it.
        {
            let mut registry = world.resource_mut::<HerdRegistry>();
            assert!(
                registry.herds[0].corral_at(
                    UVec2::new(0, 0),
                    &crate::intensification::LadderConfig::builtin()
                ),
                "the fixture species must be pennable"
            );
        }

        let forager = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: UVec2::new(0, 0),
                    floor: 0.5,
                    species: None,
                    take_species: TakeSelection::EVERYTHING,
                },
                workers: WORKERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        let keeper = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.5,
                },
                workers: WORKERS,
                // The keeper carries the hunt job's own kit, which is what a pen is collected on
                // (issue #543: carry is carry), so the crew inversion below can be quoted at the
                // shipped haul tier the pen has always collected at.
                kit: Some(
                    crate::equipment_config::EquipmentConfig::builtin()
                        .kit("big_game")
                        .expect("the shipped roster carries the big-game kit"),
                ),
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );

        world.run_system_once(advance_labor_allocation);

        let tended = world.get::<LaborAllocation>(forager).unwrap().last_yields[0].clone();
        let corral = world.get::<LaborAllocation>(keeper).unwrap().last_yields[0].clone();
        assert!(
            tended.actual > 0.0 && corral.actual > 0.0,
            "both tended sources pay out: tended={tended:?} corral={corral:?}"
        );
        // A tended patch's staffing need is **derived** like every other rung's (slice 7): the
        // boosted curve (`tended_regrowth_gain`) can now pay out more biomass than a single forager
        // carries, so the honest count is `ceil(take / per-worker throughput)`, not a fixed `1`.
        // Asserted against the shared helper rather than a magic number, so it tracks a gain retune.
        let expected_foragers = {
            let flora = world
                .resource::<crate::flora_config::FloraConfigHandle>()
                .get();
            let composition = source_tile_composition(&world);
            let patch = world.resource::<ForageRegistry>().patch(SOURCE).unwrap();
            // **The patch's OWN basket rate**, not the flat global one: a tended patch converts at
            // `patch_provisions_per_biomass`, so inverting `actual` through anything else measures a
            // different take than the sim staffed.
            let rate = crate::forage::patch_provisions_per_biomass(
                patch,
                &composition,
                &flora,
                &world_labor.forage,
            );
            let take_biomass = tended.actual / rate;
            let per_worker = crate::forage::forage_per_worker_biomass(equipped_gather_rate(), 1.0);
            (take_biomass / per_worker).ceil() as u32
        };
        assert!(
            expected_foragers >= 1,
            "the tended patch must pay out, or this asserts nothing"
        );
        assert_eq!(
            tended.workers_needed, expected_foragers,
            "a tended patch reports the crew its boosted take needs: {tended:?}"
        );
        // **The pen's staffing need is its whole CREW** (slice 8): `max(herders, haulers)`. Asserted
        // against the shared helpers rather than magic numbers, so it tracks a roster retune.
        let (herders, haulers) = {
            let world_fauna = world.resource::<FaunaConfigHandle>().get();
            let world_ladder = world.resource::<LadderConfigHandle>().get();
            let registry = world.resource::<HerdRegistry>();
            let herders =
                crate::fauna::herd_herders_needed(&registry.herds[0], &world_fauna, &world_ladder);
            let per_worker = crate::fauna::herd_hunt_yield(&registry.herds[0], &world_fauna)
                .apply(equipped_haul_rate(), 1.0)
                .provisions;
            (herders, (corral.actual / per_worker).ceil() as u32)
        };
        assert!(
            herders >= 1,
            "the fixture pen must demand at least one keeper, or this asserts nothing"
        );
        assert_eq!(
            corral.workers_needed,
            herders.max(haulers),
            "the pen reports ONE crew sized by whichever job binds — minding {herders} head vs hauling \
             the take ({haulers}): {corral:?}"
        );
    }

    // **RETIRED: `a_wild_herd_being_tamed_reports_its_full_crew_without_the_ownership_lag`** — the
    // taming-startup-lag fix, which made a Tame source's `workers_needed` report
    // `would_be_herders_needed` rather than the ownership-gated `0` a still-wild herd carried.
    //
    // Both halves of that are gone. `workers_needed` is the **take activity's own** count now
    // (`docs/plan_standing_upkeep.md` §2.2), so no herder term folds into it and there is no
    // ownership to be lagged about; `herdersNeeded` / `herdersNeededIfManaged` keep their own wire
    // fields, where the ownership-independent reading still lives. The keeping's own crew count is
    // `upkeepWorkersNeeded`, and slice 4 is where `herders_needed` becomes that.

    // **RETIRED: `a_patch_being_cultivated_seeds_the_same_build_crew_the_turn_resolves` and
    // `a_cultivating_crew_reports_the_builds_crew_because_it_carries_nothing`** — the two guards on
    // the rung's `crew_needed` floor under `workers_needed`.
    //
    // Both existed because a source published **one** blended worker count while a build was paid a
    // dipped take out of the same crew, so the count had to be floored at the build's staffing or it
    // asked for fewer hands than gathering the same ground. **There is no blended count and no dip**
    // (`docs/plan_standing_upkeep.md` §2.2): the take crew answers for the take, and the build's
    // crew is whatever the player typed. What survives of the claim is the test below.

    /// **COMPLETION FREES THE BUILD'S CREW, AND NEVER STAFFS THE KEEPING**
    /// (`docs/plan_standing_upkeep.md` §2.3). The turn a meter fills, the hands that raised the rung
    /// have finished the thing they were staffed for and go back to the idle pool.
    ///
    /// **The carry-over onto the web's keeping role is RETIRED.** It existed so that a brand-new
    /// improvement did not start decaying on turn one because nobody noticed it had begun costing
    /// something — and §4.6a makes that unreachable: the keeping bill starts at the **first work
    /// banked**, not at completion, so a player who built the thing at all was already paying to
    /// hold it. What would be left is the sim moving hands between two rows the player staffs by
    /// hand.
    ///
    /// **What completion does now is retire the QUEUE ENTRY** (`docs/plan_standing_upkeep.md` §2.4:
    /// *"at its cost, the entry leaves the queue"*), which hands the whole pool to whatever the
    /// player put next. It frees nobody, because the builders never stood on the source.
    ///
    /// **Asserted on BOTH `declares_upkeep` branches**, each against a ladder built for it, because
    /// the retired hand-off forked on exactly that predicate: a rung that costs something to hold
    /// must behave the same way one that costs nothing does.
    #[test]
    fn a_completed_build_retires_its_queue_entry_and_never_staffs_the_keeping() {
        const BUILDERS: u32 = 4;

        /// `plant:tended` with its `upkeep` block replaced by `value` — `Some(..)` for a rung that
        /// costs something to hold, `None` for one that costs nothing. The two branches of
        /// [`RungDef::declares_upkeep`], run through the same completion seam and off the same
        /// shipped record, so neither arm can drift into testing a different rung.
        fn tended_upkeep(value: serde_json::Value) -> LadderConfig {
            let mut json: serde_json::Value =
                serde_json::from_str(crate::intensification::BUILTIN_INTENSIFICATION_LADDER)
                    .expect("the builtin parses");
            let rungs = json["rungs"]
                .as_array_mut()
                .expect("the ladder lists rungs");
            let idx = rungs
                .iter()
                .position(|rung| rung["branch"] == "plant" && rung["id"] == "tended")
                .expect("the shipped ladder defines plant:tended");
            rungs[idx]["upkeep"] = value;
            LadderConfig::from_json_str(&json.to_string()).expect("the fixture ladder is valid")
        }

        let with_upkeep = tended_upkeep(serde_json::json!({
            "work_per_turn": 1.0,
            "scaled_by": "source_load",
            "grace_turns": 0,
        }));
        let without_upkeep = tended_upkeep(serde_json::Value::Null);

        let run = |ladder: std::sync::Arc<LadderConfig>| -> (u32, u32) {
            let (mut world, tile) = world_with_source(CAP);
            world.insert_resource(LadderConfigHandle::new(ladder));
            world.resource_mut::<SimulationConfig>().map_seed = WORTH_TENDING_SEED;
            grant_knowledge(&mut world, CULTIVATION_DISCOVERY_ID);
            let band = spawn_band(
                &mut world,
                tile,
                vec![LaborAssignment {
                    target: LaborTarget::Forage {
                        tile: SOURCE,
                        floor: BUILDER_FLOOR,
                        species: None,
                        take_species: TakeSelection::EVERYTHING,
                    },
                    workers: WORKERS,
                    kit: None,
                    priority: SourcePriority::default(),
                    upkeep_kit: None,
                }],
            );
            declare_patch_build(&mut world, band, SOURCE, Improvement::Cultivate, BUILDERS);
            // Long enough for the meter to fill however the fixture's ground behaves.
            for _ in 0..64 {
                world.run_system_once(advance_forage_regrowth);
                world.run_system_once(advance_labor_allocation);
                if world
                    .resource::<ForageRegistry>()
                    .patch(SOURCE)
                    .expect("the fixture seeded a patch")
                    .is_cultivated()
                {
                    break;
                }
            }
            assert!(
                world
                    .resource::<ForageRegistry>()
                    .patch(SOURCE)
                    .expect("the fixture seeded a patch")
                    .is_cultivated(),
                "fixture: the Cultivate must complete, or the hand-off never runs"
            );
            let allocation = world
                .get::<LaborAllocation>(band)
                .expect("the band keeps its allocation")
                .clone();
            let row = allocation
                .assignments
                .iter()
                .find(|a| matches!(a.target, LaborTarget::Forage { .. }))
                .expect("the forage row survives")
                .clone();
            assert!(
                allocation.build_queue.is_empty(),
                "the finished build's entry leaves the queue either way: {:?}",
                allocation.build_queue
            );
            assert!(
                row.workers > NO_CREW_ON_THIS_ACTIVITY,
                "fixture: the row survives with its gatherers, so there were hands on this source \
                 for a hand-off to have moved: {row:?}"
            );
            // **The builders are still standing where the player put them**, and nobody was moved
            // onto the band's agriculture role — the retired hand-off's one destination
            // (`docs/plan_standing_upkeep.md` §2.5).
            (
                allocation.workers_on(&LaborTarget::Builders),
                allocation.workers_on(&LaborTarget::Agriculture),
            )
        };

        let (still_building, on_keeping) = run(std::sync::Arc::new(with_upkeep));
        assert_eq!(
            still_building, BUILDERS,
            "liveness: the pool the fixture staffed is still there to have been moved"
        );
        assert_eq!(
            on_keeping, NO_CREW_ON_THIS_ACTIVITY,
            "a finished rung that costs something to HOLD moves nobody onto the keeping — the \
             player staffs it, and the bill started at the first work banked"
        );

        let (_, freed) = run(std::sync::Arc::new(without_upkeep));
        assert_eq!(
            freed, NO_CREW_ON_THIS_ACTIVITY,
            "…and so does one that costs nothing: there is one branch now, not two"
        );
    }

    /// **A BUILD RUNNING BESIDE A TAKE CHANGES NEITHER THE TAKE NOR ITS CREW COUNT** — the whole of
    /// what separating the allocations bought (`docs/plan_standing_upkeep.md` §2.2), and the property
    /// that replaced the retired build-crew floor.
    ///
    /// **Asserted across the seam AND the resolved turn**, because the defect class the floor guarded
    /// was a *disagreement between them*: the compose sheet said one thing while the tile card beside
    /// it said another, in the same frame, and it self-healed the next turn — which is exactly why it
    /// survived.
    #[test]
    fn a_build_in_flight_leaves_the_take_row_alone() {
        let (mut world, tile) = world_with_source(CAP);
        let labor = world.resource::<LaborConfigHandle>().get();
        // The same committed-crop ground the other rung-2 payoff tests stand on (#433).
        world.resource_mut::<SimulationConfig>().map_seed = WORTH_TENDING_SEED;
        grant_knowledge(&mut world, CULTIVATION_DISCOVERY_ID);

        const GATHERERS: u32 = 2;
        let composition = source_tile_composition(&world);
        let seed = |world: &World| {
            let flora = world
                .resource::<crate::flora_config::FloraConfigHandle>()
                .get();
            let registry = world.resource::<ForageRegistry>();
            crate::forage::forage_source_yield_preview(
                registry.patch(SOURCE).expect("the fixture seeded a patch"),
                &composition,
                &labor.forage,
                &flora,
                equipped_gather_rate(),
                SEASONAL_WEIGHT,
                NEUTRAL_OUTPUT_MULT,
                GATHERERS,
                SHALLOW_DRAW_FLOOR,
                &TakeSelection::EVERYTHING,
                labor.yield_average_horizon_turns,
                labor.arrivals_horizon_turns,
                SHIPPED_FORECAST_RANGE_SIGMAS,
            )
        };
        let quoted = seed(&world);
        assert!(
            quoted.actual > 0.0,
            "liveness: the gatherers must actually take something"
        );

        // **The turn the quote is about** — the forecast prices the stand one Logistics regrowth
        // from here, so the harness advances it before resolving ([`advance_logistics_regrowth`]).
        advance_logistics_regrowth(&mut world);

        // The resolved turn, with a **Cultivate staffed beside them** out of the same band.
        const BUILDERS: u32 = 3;
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    floor: SHALLOW_DRAW_FLOOR,
                    species: None,
                    take_species: TakeSelection::EVERYTHING,
                },
                workers: GATHERERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        declare_patch_build(&mut world, band, SOURCE, Improvement::Cultivate, BUILDERS);
        world.run_system_once(advance_labor_allocation);
        let resolved = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();

        assert!(
            (resolved.actual - quoted.actual).abs() < FORECAST_EPSILON,
            "the gatherers take exactly what they were quoted, build or no build: \
             {resolved:?} vs {quoted:?}"
        );
        assert_eq!(
            resolved.workers_needed, quoted.workers_needed,
            "seed == resolved — the compose sheet and the tile card cannot disagree in one frame"
        );
        // …and the builders banked their own work, so the build was genuinely running.
        assert!(
            world
                .resource::<ForageRegistry>()
                .patch(SOURCE)
                .unwrap()
                .ladder_position()
                > 0.0,
            "liveness: the build crew must have banked something"
        );
    }

    /// **A TAME IN FLIGHT LEAVES THE HUNT ROW ALONE** — the animal twin of
    /// `a_build_in_flight_leaves_the_take_row_alone` (`docs/plan_standing_upkeep.md` §2.2).
    ///
    /// `hunt_take_workers` answers *"how many hands carry home the peak drop this ceiling allows"*,
    /// and it exists so `workers_needed` and `wasted` can never contradict each other. Since the
    /// gentling crew is its own allocation, the hunters beside it are unaffected: same take, same
    /// count, with and without the verb.
    ///
    /// **It replaced `a_herd_being_tamed_sizes_its_haul_crew_on_the_dipped_carry`**, which pinned the
    /// build dip's half of that agreement — a gentling party hauled `dip ×` what a hunting one did,
    /// so the count had to be sized on the dipped carry or it named a crew that provably could not
    /// lift the drop. There is no dip and no shared crew, so the invariant is now the stronger one:
    /// the improvement axis moves nothing on this row at all.
    #[test]
    fn a_tame_in_flight_leaves_the_hunt_row_alone() {
        /// The builders the fixture stands on the `Tame`, so the arm under test really is running a
        /// build beside the hunters rather than sweeping a queue nobody funds.
        const GENTLING_POOL: u32 = 3;

        // One hunt turn on the slow breeder at a KILL biomass, with and without a Tame in flight.
        // Same herd, same floor, same hunting crew — the improvement is the only axis.
        let row = |improvement: Option<Improvement>| {
            let (mut world, tile) = world_with_source(CAP);
            reseat_slow_breeder(&mut world, SLOW_BREEDER_KILL_BIOMASS);
            let band = spawn_band(
                &mut world,
                tile,
                vec![LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: HERD_ID.to_string(),
                        floor: crate::fauna::MSY_BIOMASS_FRACTION,
                    },
                    workers: WORKERS,
                    kit: None,
                    priority: SourcePriority::default(),
                    upkeep_kit: None,
                }],
            );
            if let Some(declared) = improvement {
                // A real gentling pool beside the hunters, so this is not a no-op sweep.
                declare_herd_build(&mut world, band, HERD_ID, declared, GENTLING_POOL);
            }
            world.run_system_once(advance_labor_allocation);
            world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone()
        };
        let taming = row(Some(Improvement::Tame));
        let hunting = row(NO_IMPROVEMENT_UNDERWAY);

        assert!(
            hunting.actual > 0.0,
            "liveness: the hunters must actually take something"
        );
        assert_eq!(
            (taming.actual, taming.workers_needed),
            (hunting.actual, hunting.workers_needed),
            "a Tame staffed beside the hunters changes neither their take nor their count: \
             {taming:?} vs {hunting:?}"
        );

        // **The ASSIGN-TIME seed says the same number**, which is the half a seed==resolved test
        // exists for: the compose sheet and the band panel cannot disagree in one frame.
        let seed = {
            let (mut world, _) = world_with_source(CAP);
            let labor = world.resource::<LaborConfigHandle>().get();
            reseat_slow_breeder(&mut world, SLOW_BREEDER_KILL_BIOMASS);
            let fauna = world.resource::<FaunaConfigHandle>().get();
            let registry = world.resource::<HerdRegistry>();
            crate::fauna::hunt_source_yield_preview(
                registry.find(HERD_ID).expect("the fixture seeded a herd"),
                &fauna,
                equipped_haul_rate(),
                &crate::fauna::HuntingParty::builtin_equipped(),
                NEUTRAL_OUTPUT_MULT,
                WORKERS,
                crate::fauna::MSY_BIOMASS_FRACTION,
                labor.yield_average_horizon_turns,
                labor.arrivals_horizon_turns,
                SHIPPED_FORECAST_RANGE_SIGMAS,
            )
        };
        assert_eq!(
            seed.workers_needed, taming.workers_needed,
            "seed == resolved: {seed:?} vs {taming:?}"
        );

        // The property the count exists to guarantee: at `workers_needed` the crew can actually lift
        // the biggest drop the ceiling allows (`floor(ceiling/body) + 1` whole bodies).
        let ceiling = crate::fauna::escapement_ceiling(
            crate::fauna::MSY_BIOMASS_FRACTION,
            SLOW_BREEDER_KILL_BIOMASS,
            SLOW_BREEDER_CAP,
        );
        let peak_biomass = ((ceiling / SLOW_BREEDER_BODY).floor() + 1.0) * SLOW_BREEDER_BODY;
        assert!(
            hunting.workers_needed as f32 * equipped_haul_rate() >= peak_biomass,
            "the reported crew must be able to haul the peak drop it was sized on: {} hands carry \
             {} of {peak_biomass}",
            hunting.workers_needed,
            hunting.workers_needed as f32 * equipped_haul_rate()
        );
    }

    /// Reseat the harness herd as a **Wild-Aurochs-shaped slow breeder**: a `body_mass` heavier than one
    /// turn's regrowth at the operating point (`r·K/4 = 0.05·400/4 = 5 ≪ 80`), so it **pulses** — it
    /// spares zero animals on most turns while the stock above its floor rebuilds, then a whole one
    /// when that room clears a body. `biomass` is what picks the turn a test measures: below
    /// `K/2 + body` is a **wait**, at or above it a **kill**.
    fn reseat_slow_breeder(world: &mut World, biomass: f32) {
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let mut registry = world.resource_mut::<HerdRegistry>();
        let herd = &mut registry.herds[0];
        herd.body_mass = SLOW_BREEDER_BODY;
        herd.carrying_capacity = SLOW_BREEDER_CAP;
        herd.biomass = biomass;
        // These fixtures set biomass directly (no `regrow_biomass`); the rung payoff projections read
        // `biomass_before_regrowth` — keep it in sync.
        herd.biomass_before_regrowth = biomass;
        herd.refresh_ecology_phase(&fauna);
    }

    /// One aurochs-shaped body — heavier than one turn's regrowth, and heavier than one hauler carries.
    const SLOW_BREEDER_BODY: f32 = 80.0;
    /// The slow breeder's capacity: `MSY = r·K/4 = 5`, far below `SLOW_BREEDER_BODY`, and big enough
    /// that `K/2 + body` is a reachable biomass (so a **kill** turn is expressible at all).
    const SLOW_BREEDER_CAP: f32 = 400.0;
    /// Above the escapement point (`K/2 = 200`), but by **less than one body** — the WAIT turn: there
    /// is standing surplus, just not a whole animal of it.
    const SLOW_BREEDER_BIOMASS: f32 = 240.0;
    /// `K/2` plus more than one body — the KILL turn.
    const SLOW_BREEDER_KILL_BIOMASS: f32 = 300.0;

    /// A single Sustain-hunt turn on the slow breeder at `biomass` with `workers` assigned; returns
    /// the captured yield row.
    fn slow_breeder_hunt(biomass: f32, workers: u32) -> SourceYield {
        let (mut world, tile) = world_with_source(CAP);
        reseat_slow_breeder(&mut world, biomass);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.5,
                },
                workers,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        world.run_system_once(advance_labor_allocation);
        world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone()
    }

    /// One hunt turn under `policy` on the slow breeder (biomass above `K/2`, empty bank), staffed so
    /// the worker cap never binds; returns the captured yield row.
    fn slow_breeder_hunt_at(floor: f32) -> SourceYield {
        let (mut world, tile) = world_with_source(CAP);
        reseat_slow_breeder(&mut world, SLOW_BREEDER_BIOMASS);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor,
                },
                workers: WORKERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        world.run_system_once(advance_labor_allocation);
        world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone()
    }

    /// **The forward-projected `realized` reads the HONEST OVERHUNTING RATE — and sees the decline.**
    /// `sustainable` is the herd's MSY (the overhunting reference), policy-independent. The lumpy
    /// `actual` cannot be compared to it turn by turn (a kill lands a whole animal and spikes above
    /// MSY even under Sustain), which is why `overdraws` exists. The forward-projected `realized` IS
    /// comparable, and it is ordered by how deep the stance's floor is.
    ///
    /// **A Sustain projection TRACKS MSY to within one animal per window — it does not sit above it,
    /// and the difference is the quantiser.** The window opens on a herd standing above `K/2`, so the
    /// projection draws that accumulated surplus down to the floor and then lives on the regrowth;
    /// what makes Sustain sustainable is its floor, not its being under a line. But on a **slow
    /// breeder** the payout is a *pulse train* — the room above the floor is lighter than one body for
    /// several turns, then clears it — so the window's average lands wherever the last pulse fell
    /// relative to the window's edge, which is a **half-open interval around MSY**, not a floor under
    /// it. (Here: one 80-unit body per window against 40 turns × MSY 5 = 1.25 bodies' worth.)
    ///
    /// It read as a floor while `project_realized_hunt` let the smooth escapement rate flow on turns
    /// the herd could not spare a whole animal — the party's engagement was bounded by its own reach
    /// alone, so the projection quoted a trickle the take never pays. Bounding the engagement by what
    /// the herd can spare (`fauna::animals_affordable`, the clamp the live path always applied) is
    /// what put the pulse into the average, and it is what makes this row agree with the **arrivals
    /// schedule beside it** — two exported projections of the same herd that previously disagreed by
    /// the whole quantisation. So the tolerance below is *one animal spread over the window*, read off
    /// the arrivals pulse rather than restated here, and it is a **tighter** claim than the `>=` it
    /// replaced.
    ///
    /// **The decline is visible as `realized < actual` on Surplus**: the opening turn draws the stock
    /// down to `0.30·K` and the horizon that follows pays only the trickle back, so the steady
    /// headline lands well below the turn the player just watched.
    ///
    /// **Deplete does NOT terminate on a slow breeder, and that is the quantiser rather than a
    /// dilution bug.** Whole animals cannot strip a herd to *exactly* `0.15·K` — the take stops on the
    /// last whole body standing above the brink — so the herd survives a little above it and goes on
    /// offering a sub-body trickle, which the loop correctly counts as wait turns. Its average is
    /// therefore the honest long-run rate under that floor: ordered above Surplus and above Sustain,
    /// but nowhere near a one-turn strip. **The divide-by-turns-simulated rule this used to pin lives
    /// on the one policy that really does spend its source in a turn** —
    /// `eradicate_realized_reads_the_strip_rate_not_a_diluted_average`, whose floor of `0` leaves
    /// nothing standing to round against. (It read as a strip here only while the smooth projection
    /// took `2.25` animals where the take pays `2`.)
    #[test]
    fn realized_reads_the_honest_overhunting_rate() {
        let sustain = slow_breeder_hunt_at(0.5);
        let surplus = slow_breeder_hunt_at(0.3);
        let deplete = slow_breeder_hunt_at(0.15);

        // `sustainable` is MSY, the same under every policy (it is the reference, not the take).
        assert!(
            (sustain.sustainable - surplus.sustainable).abs() < 1e-6
                && (sustain.sustainable - deplete.sustainable).abs() < 1e-6,
            "sustainable is the policy-independent MSY reference: {sustain:?} {surplus:?} {deplete:?}"
        );
        // **One whole animal's provisions**, read off the projection's own arrivals schedule — whose
        // non-zero slots on this slow breeder ARE single-animal pulses — so the tolerance is the
        // sim's own quantum rather than one restated here and left to drift from the fixture.
        let one_animal = sustain.arrivals.iter().cloned().fold(0.0_f32, f32::max);
        assert!(
            one_animal > 0.0,
            "liveness: the projection must land at least one animal in the window, or the quantum \
             below is zero and the assertion is vacuous: {sustain:?}"
        );
        let window_quantum = one_animal
            / LaborConfigHandle::default()
                .get()
                .yield_average_horizon_turns as f32;
        // Sustain tracks its sustainable MSY to within that one animal — above it when the window
        // catches an extra pulse, a shade under when it catches one fewer. Either way it is the MSY
        // the herd can pay, delivered in whole bodies.
        assert!(
            (sustain.realized - sustain.sustainable).abs() <= window_quantum,
            "a Sustain hunt projects its sustainable MSY to within one animal per window \
             ({window_quantum}): {sustain:?}"
        );
        assert!(
            sustain.realized > 0.0,
            "a Sustain hunt on a healthy herd projects a LIVE rate, not zero: {sustain:?}"
        );
        // Overhunting projects the honest rate ABOVE the sustainable reference, ordered by policy.
        assert!(
            surplus.realized > surplus.sustainable,
            "Surplus projects above the sustainable MSY (the honest overhunt rate): {surplus:?}"
        );
        assert!(
            deplete.realized > surplus.realized,
            "Deplete projects deeper than Surplus: {deplete:?} {surplus:?}"
        );
        // The projection SEES THE DECLINE on the stance that survives its own draw: Surplus takes the
        // standing surplus on turn one and then lives on the regrowth above `0.30·K`, so its horizon
        // average is far below the take the player just watched land. The instantaneous reading could
        // not produce that.
        assert!(
            surplus.realized > 0.0 && surplus.realized < surplus.actual,
            "Surplus projects well below its opening draw (sees the decline): {surplus:?}"
        );
        // Deplete leaves the herd just above the Allee brink — the last whole body it could not take
        // without crossing — so the projection runs on and reports the long-run rate that floor
        // sustains. Ordered above Sustain's, which leaves twice as much standing.
        assert!(
            deplete.realized > sustain.realized,
            "a deeper floor must project a higher steady rate than Sustain's: {deplete:?} vs \
             {sustain:?}"
        );
    }

    /// **Eradicate reads the STRIP RATE it delivers, NOT a diluted average.** Eradicate strips the herd
    /// in ~1 turn; the projection breaks the moment the source is spent and divides by the turns it
    /// actually delivered, so `realized` reads the high one-shot strip rate — far above Sustain's MSY —
    /// rather than that rate smeared thin across ~40 mostly-empty horizon turns (which would read
    /// *below* Sustain, the exact dilution the divide-by-turns-simulated rule prevents).
    #[test]
    fn eradicate_realized_reads_the_strip_rate_not_a_diluted_average() {
        let sustain = slow_breeder_hunt_at(0.5);
        let eradicate = slow_breeder_hunt_at(0.0);

        assert!(
            eradicate.realized > sustain.realized,
            "Eradicate strips faster than Sustain sustains: {eradicate:?} vs {sustain:?}"
        );
        // Not diluted toward zero: the one-turn strip of the whole standing stock dwarfs the
        // sustainable MSY. Diluting it over the full horizon would drop it to ~MSY/horizon, *below*
        // Sustain — so this margin is what proves the loop divided by the turns actually simulated.
        assert!(
            eradicate.realized > 10.0 * sustain.sustainable,
            "Eradicate reads its strip rate, not a horizon-diluted average: {eradicate:?} \
             (sustainable {})",
            sustain.sustainable
        );
    }

    /// **A hunt's `workers_needed` is its CEILING's carry crew — never the lumpy `0` of a wait turn.**
    /// The bug: sizing the crew off *this turn's* `take.carried` reads `0` on a slow breeder's wait turn
    /// (the room above the floor is lighter than one body, so nothing drops), collapsing
    /// `workers_needed` beside a `wasted_yield` that says the crew is understaffed — *drop workers* and
    /// *add workers* on one row. The ceiling-derived crew cannot flicker with the pulse, because it is
    /// taken on the same number `wasted_yield` is.
    #[test]
    fn a_slow_breeder_hunt_reports_its_carry_crew_on_a_wait_turn_never_zero() {
        let per_worker = equipped_haul_rate();
        // The crew each turn's ceiling asks for, off the same helper the sim uses.
        let crew_for = |biomass: f32| {
            crate::fauna::hunt_haul_workers(
                crate::fauna::escapement_ceiling(
                    crate::fauna::MSY_BIOMASS_FRACTION,
                    biomass,
                    SLOW_BREEDER_CAP,
                ),
                SLOW_BREEDER_BODY,
                per_worker,
            )
        };
        let wait_crew = crew_for(SLOW_BREEDER_BIOMASS);
        assert!(
            wait_crew >= 2,
            "the fixture must need more than one hauler, or the wait-turn collapse is invisible"
        );

        // Wait turn: the room above the floor is under one body, so nothing drops — but the crew is
        // still the one the ceiling asks for, NOT the old `0`.
        let wait = slow_breeder_hunt(SLOW_BREEDER_BIOMASS, wait_crew);
        assert_eq!(
            wait.actual, 0.0,
            "a slow breeder waits while its room rebuilds: {wait:?}"
        );
        assert_eq!(
            wait.workers_needed, wait_crew,
            "the wait-turn crew is the ceiling's carry crew, not the lumpy 0: {wait:?}"
        );

        // Kill turn: the room clears a body, an animal lands, and the crew is still the ceiling's.
        let kill_crew = crew_for(SLOW_BREEDER_KILL_BIOMASS);
        let kill = slow_breeder_hunt(SLOW_BREEDER_KILL_BIOMASS, kill_crew);
        assert!(kill.actual > 0.0, "the whole animal lands: {kill:?}");
        assert_eq!(
            kill.workers_needed, kill_crew,
            "the kill-turn crew is the ceiling's carry crew too: {kill:?}"
        );
        assert_eq!(
            kill.wasted, 0.0,
            "a crew sized to the ceiling wastes nothing — the pairing `workers_needed`/`wasted` \
             must never disagree: {kill:?}"
        );

        // Overstaffed beyond that crew: the count is ceiling-derived (not clamped up to assigned), so
        // an extra hand is still flagged.
        let over = slow_breeder_hunt(SLOW_BREEDER_KILL_BIOMASS, kill_crew + 1);
        assert_eq!(
            over.workers_needed, kill_crew,
            "the crew is the ceiling's need, independent of overstaffing: {over:?}"
        );
        assert!(
            kill_crew + 1 > over.workers_needed,
            "a herd overstaffed beyond its crew still flags the idle hand: {over:?}"
        );
    }

    /// **A domesticated slow breeder reports `max(herders_needed, steady_haul)`, and it equals the
    /// client's `_max_useful_workers`.** The managed rung staffs one crew big enough for both jobs; the
    /// haul side is the steady carry crew (stable across the pulse), so the band panel's overstaff note
    /// and the compose panel's stepper cap read the same number — which is the whole point of the fix.
    #[test]
    fn a_domesticated_slow_breeder_reports_max_of_herders_and_steady_crew_matching_the_client() {
        let (mut world, tile) = world_with_source(CAP);
        reseat_slow_breeder(&mut world, SLOW_BREEDER_BIOMASS);
        // Tame it outright so it owes a standing herder cost (owner = the band's faction).
        {
            let mut registry = world.resource_mut::<HerdRegistry>();
            let herd = &mut registry.herds[0];
            herd.tame_outright(
                FactionId(0),
                &crate::intensification::LadderConfig::builtin(),
            );
            assert!(herd.is_domesticated(), "the fixture herd must be tamed");
        }
        let assigned = 3;
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.5,
                },
                workers: assigned,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        // The sim's expectation: one crew, `max(herders, steady_haul)` — taken on the **pre-take**
        // herd, which is the state the labor arm sizes the crew against (an escapement ceiling falls
        // with the take that just drew it, so reading it afterwards would measure a different turn).
        let (herders, steady_haul, client_max_useful) = {
            let fauna = world.resource::<FaunaConfigHandle>().get();
            let ladder = world.resource::<LadderConfigHandle>().get();
            let herd = world.resource::<HerdRegistry>().find(HERD_ID).unwrap();
            let herders = crate::fauna::herd_herders_needed(herd, &fauna, &ladder);
            let ceiling_biomass = crate::fauna::hunt_escapement_ceiling(
                0.5,
                herd.biomass,
                crate::fauna::herd_capacity(herd, &fauna),
            );
            let steady_haul = crate::fauna::hunt_haul_workers(
                ceiling_biomass,
                herd.body_mass,
                equipped_haul_rate(),
            );
            // The client's `_max_useful_workers`, in food-space off the same forecast the compose panel
            // reads: ceil((floor(ceiling / foodPerAnimal) + 1) × foodPerAnimal / perWorkerYield).
            let forecast = crate::fauna::hunt_forecast(
                herd,
                &fauna,
                equipped_haul_rate(),
                &crate::fauna::HuntingParty::builtin_equipped(),
                1.0,
            );
            let ceiling = forecast
                .ceiling_at(crate::fauna::MSY_BIOMASS_FRACTION)
                .provisions;
            let food_per_animal = forecast.body_mass_yield.provisions;
            let per_worker_yield = forecast.per_worker_yield.provisions;
            let client = ((((ceiling / food_per_animal).floor() + 1.0) * food_per_animal
                / per_worker_yield)
                .ceil()) as u32;
            (herders, steady_haul, client)
        };
        world.run_system_once(advance_labor_allocation);
        let yielded = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();

        assert!(
            herders >= 1,
            "a tamed herd owes at least one keeper, or this asserts nothing"
        );
        assert_eq!(
            yielded.workers_needed,
            herders.max(steady_haul),
            "a managed herd reports one crew = max(herders, steady haul): {yielded:?}"
        );
        assert_eq!(
            steady_haul, client_max_useful,
            "the sim's steady haul crew equals the client's max-useful count by construction"
        );
        assert!(
            assigned > yielded.workers_needed,
            "the 3-worker fixture is overstaffed past the steady crew: {yielded:?}"
        );
    }

    // --- Pre-commit yield forecast: forecast == actual (the client's "Expected yield") -------------
    //
    // The snapshot exposes a per-source forecast (`per_worker_yield` + the four policy ceilings) so
    // the client can show "Expected yield: +X.XX /turn" and cap its worker stepper BEFORE the player
    // commits. It only works if the forecast agrees with what the sim actually pays — these tests are
    // the guard: they run the REAL `advance_labor_allocation` and compare its payout against the
    // client's composition `min(workers × per_worker_yield, ceiling[policy])`.

    /// The tile coord `world_with_source` anchors its forage patch + herd on.
    const SOURCE: UVec2 = UVec2::new(0, 0);
    /// The `FoodModuleTag::seasonal_weight` `world_with_source` stamps on the source tile — the same
    /// weight the client reads for the tile and folds into its forecast.
    const SEASONAL_WEIGHT: f32 = 1.0;
    /// `spawn_band` bands sit at morale 1.0 → a neutral productivity multiplier, which is also the
    /// multiplier the snapshot captures forecasts at (`FORECAST_OUTPUT_MULTIPLIER`).
    const NEUTRAL_OUTPUT_MULT: f32 = 1.0;
    /// **A floor just ABOVE the food peak**, so the harness patch offers only a sliver of standing
    /// stock — less than one gatherer's throughput. It keeps the take crew, rather than the patch,
    /// the binding term in `a_build_in_flight_leaves_the_take_row_alone`.
    const SHALLOW_DRAW_FLOOR: f32 = 0.55;
    /// f32 slack between the forecast (`workers × per_worker_yield`, provisions) and the sim's take
    /// (biomass → fixed-point provisions): different multiplication order + a 1e-6 fixed-point grid.
    /// Orders of magnitude below one provision.
    const FORECAST_EPSILON: f32 = 1e-4;
    /// Every improvement a **Forage** assignment may carry, plus the pure harvest. Swept against
    /// every stance so `forecast == actual` is checked over the whole (stance × improvement) grid —
    /// which, since the build left the tile for the band's own pool, is the grid on which the
    /// improvement axis must make **no difference at all** to the take.
    const FORAGE_IMPROVEMENTS: [Option<Improvement>; 3] =
        [None, Some(Improvement::Cultivate), Some(Improvement::Sow)];
    /// The animal twin of [`FORAGE_IMPROVEMENTS`].
    const HUNT_IMPROVEMENTS: [Option<Improvement>; 3] =
        [None, Some(Improvement::Tame), Some(Improvement::Corral)];

    /// The client's composition: what it would display as the expected yield for this staffing. The
    /// shared helper — the *same* one the assign-time telemetry seed uses — so these tests pin the
    /// number the client shows, not a re-derivation of it.
    fn expected_yield(forecast: &SourceYieldForecast, workers: u32, floor: f32) -> f32 {
        forecast_expected_take(forecast, workers, floor).provisions
    }

    /// The client's worker-stepper cap.
    fn max_useful_workers(forecast: &SourceYieldForecast, floor: f32) -> u32 {
        (forecast.ceiling_at(floor).provisions / forecast.per_worker_yield.provisions).ceil() as u32
    }

    /// **THE LOGISTICS REGROWTH THESE HARNESSES OWE THEIR SOURCES.**
    ///
    /// A pre-commit forecast prices the source **as next turn's take will find it** — one Logistics
    /// regrowth on (`forage::next_turns_stand` / `fauna::next_turns_quarry`), because every
    /// production caller reads it *after* the Population take. A harness that quotes a forecast and
    /// then runs `advance_labor_allocation` on its own has skipped the stage in between, so
    /// *"forecast == actual"* would be comparing two different turns and the difference would be
    /// exactly the growth.
    ///
    /// It regrows the registries and nothing else — no movement, no shed, no capacity rewrite from
    /// the tile — which is the same simplification the projections make, so what the fixture
    /// advances is precisely what the forecast projected.
    fn advance_logistics_regrowth(world: &mut World) {
        let labor = world.resource::<LaborConfigHandle>().get();
        let fauna = world.resource::<FaunaConfigHandle>().get();
        for patch in world.resource_mut::<ForageRegistry>().patches.values_mut() {
            *patch = crate::forage::next_turns_stand(patch, &labor.forage);
        }
        for herd in world.resource_mut::<HerdRegistry>().herds.iter_mut() {
            *herd = crate::fauna::next_turns_quarry(herd, &fauna);
        }
    }

    /// Re-seat the test herd at `biomass`/`cap` (the harness's default 100-cap herd saturates every
    /// hunt policy ceiling with a single 40-biomass hunter, so a labor-bound hunt needs a bigger one).
    fn reseat_herd(world: &mut World, biomass: f32, cap: f32) {
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let mut registry = world.resource_mut::<HerdRegistry>();
        let herd = &mut registry.herds[0];
        herd.carrying_capacity = cap;
        herd.biomass = biomass;
        // Keep the pre-regrowth reading in sync (slice 8b): these tests set the biomass directly
        // without running `regrow_biomass`, and Sustain's rate reads `biomass_before_regrowth`.
        herd.biomass_before_regrowth = biomass;
        herd.refresh_ecology_phase(&fauna);
    }

    /// **The floors both forecast==actual sweeps walk.** The four the retired stance axis named
    /// (`0.50 / 0.30 / 0.15 / 0`), plus `0.80` and `1.0` — values the assignment can carry now and
    /// the stance axis could not express, `1.0` being the degenerate *"take nothing"* end where the
    /// room is exactly zero.
    const SWEPT_FLOORS: [f32; 6] = [0.0, 0.15, 0.3, 0.5, 0.8, 1.0];

    /// **The builders the forecast sweeps stand on their declared build.** A pool rather than a
    /// per-source crew since `docs/plan_standing_upkeep.md` §2.5 — the number is arbitrary, and
    /// what it has to be is **non-zero**, so the swept improvement really is a build in flight
    /// beside the take rather than an entry nobody is funding.
    const SWEPT_BUILDERS: u32 = 2;

    /// **Forage forecast == actual, at every FLOOR.** For every floor × staffing (labor-bound,
    /// ceiling-bound), the client's `min(workers × per_worker_yield, ceiling_at(floor))` equals the
    /// provisions `advance_labor_allocation` actually pays. Both binding regimes are asserted to
    /// have been exercised, so this can't silently degenerate into testing one branch.
    ///
    /// **Swept over floors rather than over four stances** (`docs/plan_harvest_floor.md` §5): the
    /// assignment carries a continuous number now, so a sweep of four fixed values would only pin
    /// the four the retired axis happened to name.
    #[test]
    fn forage_forecast_equals_actual_take_for_every_floor_and_staffing() {
        let mut saw_labor_bound = false;
        let mut saw_ceiling_bound = false;
        for policy in SWEPT_FLOORS {
            for improvement in FORAGE_IMPROVEMENTS {
                for workers in [1u32, 2, 20] {
                    let (mut world, tile) = world_with_source(CAP);
                    let labor = world.resource::<LaborConfigHandle>().get();
                    // Forecast off the PRE-turn patch state, exactly as the client reads it from the
                    // snapshot captured at the end of last turn.
                    let patch = world
                        .resource::<ForageRegistry>()
                        .patch(SOURCE)
                        .cloned()
                        .expect("seeded patch");
                    let composition = source_tile_composition(&world);
                    let forecast = forage_forecast(
                        &patch,
                        &composition,
                        &labor.forage,
                        &FloraConfig::builtin(),
                        crate::forage::forage_per_worker_biomass(
                            equipped_gather_rate(),
                            SEASONAL_WEIGHT,
                        ),
                        NEUTRAL_OUTPUT_MULT,
                        &TakeSelection::EVERYTHING,
                    );
                    drop(labor);

                    let band = spawn_band(
                        &mut world,
                        tile,
                        vec![LaborAssignment {
                            target: LaborTarget::Forage {
                                tile: SOURCE,
                                floor: policy,
                                species: None,
                                take_species: TakeSelection::EVERYTHING,
                            },
                            workers,
                            kit: None,
                            priority: SourcePriority::default(),
                            upkeep_kit: None,
                        }],
                    );
                    if let Some(declared) = improvement {
                        declare_patch_build(&mut world, band, SOURCE, declared, SWEPT_BUILDERS);
                    }
                    // The stage the forecast was quoted across ([`advance_logistics_regrowth`]).
                    advance_logistics_regrowth(&mut world);
                    world.run_system_once(advance_labor_allocation);
                    let actual = world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual;

                    let labor_term = workers as f32 * forecast.per_worker_yield.provisions;
                    let ceiling = forecast.ceiling_at(policy).provisions;
                    if labor_term < ceiling {
                        saw_labor_bound = true;
                    } else {
                        saw_ceiling_bound = true;
                    }
                    let expected = expected_yield(&forecast, workers, policy);
                    assert!(
                    (actual - expected).abs() < FORECAST_EPSILON,
                    "forage forecast must equal the actual take (floor {policy} + {improvement:?}, \
                     {workers} workers): forecast={expected} actual={actual} ({forecast:?})"
                );
                }
            }
        }
        assert!(
            saw_labor_bound && saw_ceiling_bound,
            "both regimes must be covered: labor-bound={saw_labor_bound} ceiling-bound={saw_ceiling_bound}"
        );
    }

    /// **Hunt forecast == actual, on a fresh (empty-bank) herd.** The fauna twin of the forage test.
    /// The herd is re-seated at a large capacity so the Eradicate ceiling exceeds a single hunter's
    /// throughput (a labor-bound case); 20 hunters overstaff every policy (the ceiling binds).
    ///
    /// **The forecast IS the take**, helper for helper: both are
    /// `min(crew throughput, hunt_escapement_ceiling(...))` quantised to whole animals, so the
    /// invariant holds turn by turn rather than in the long run. The old caveat — that it held only on
    /// an empty kill-credit bank, because the readout was a steady rate while the take cashed a
    /// banked burst — died with the bank (`Herd::hunt_credit`).
    ///
    /// **It sweeps TWO stock levels**: a full herd and [`DRAWN_DOWN_BIOMASS`], a remnant standing
    /// barely above the deepest floors, where a whole-animal take is a large fraction of what is left.
    /// The `stock_cap` clamp is asserted **inert** throughout — an escapement ceiling is `B − floor·K`
    /// and so cannot exceed `B` — which is the property that retired the dip-versus-clamp ordering
    /// question rather than an assumption made about it.
    #[test]
    fn hunt_forecast_equals_actual_take_for_every_floor_and_staffing() {
        let mut saw_labor_bound = false;
        let mut saw_ceiling_bound = false;
        for biomass in [BIG_HERD_CAP, DRAWN_DOWN_BIOMASS] {
            for policy in SWEPT_FLOORS {
                for improvement in HUNT_IMPROVEMENTS {
                    for workers in [1u32, 2, 20] {
                        let (mut world, tile) = world_with_source(CAP);
                        reseat_herd(&mut world, biomass, BIG_HERD_CAP);
                        let herd = world
                            .resource::<HerdRegistry>()
                            .find(HERD_ID)
                            .cloned()
                            .expect("seeded herd");
                        assert_eq!(
                            herd.hunt_credit, 0.0,
                            "the resident take path must not read or write the expedition's bank"
                        );
                        let fauna = world.resource::<FaunaConfigHandle>().get();
                        let per_worker = equipped_haul_rate();
                        let forecast = hunt_forecast(
                            &herd,
                            &fauna,
                            per_worker,
                            &crate::fauna::HuntingParty::builtin_equipped(),
                            NEUTRAL_OUTPUT_MULT,
                        );
                        drop(fauna);

                        let band = spawn_band(
                            &mut world,
                            tile,
                            vec![LaborAssignment {
                                target: LaborTarget::Hunt {
                                    fauna_id: HERD_ID.to_string(),
                                    floor: policy,
                                },
                                workers,
                                kit: None,
                                priority: SourcePriority::default(),
                                upkeep_kit: None,
                            }],
                        );
                        if let Some(declared) = improvement {
                            declare_herd_build(&mut world, band, HERD_ID, declared, SWEPT_BUILDERS);
                        }
                        // The stage the forecast was quoted across
                        // ([`advance_logistics_regrowth`]).
                        advance_logistics_regrowth(&mut world);
                        world.run_system_once(advance_labor_allocation);
                        let actual =
                            world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual;

                        // **The take is the hunters' own** (`docs/plan_standing_upkeep.md` §2.2),
                        // so which side binds does not depend on the improvement at all — which is
                        // itself what the sweep is asserting.
                        let labor_term = workers as f32 * forecast.per_worker_yield.provisions;
                        let ceiling = forecast.ceiling_at(policy).provisions;
                        if labor_term < ceiling {
                            saw_labor_bound = true;
                        } else {
                            saw_ceiling_bound = true;
                        }
                        let expected = expected_yield(&forecast, workers, policy);
                        assert!(
                            (actual - expected).abs() < FORECAST_EPSILON,
                            "hunt forecast must equal the actual take (B={biomass}, floor {policy} + \
                             {improvement:?}, {workers} workers): forecast={expected} \
                             actual={actual} ({forecast:?})"
                        );
                    }
                }
            }
        }
        assert!(
            saw_labor_bound && saw_ceiling_bound,
            "both regimes must be covered: labor-bound={saw_labor_bound} ceiling-bound={saw_ceiling_bound}"
        );
    }

    /// Carrying capacity the hunt forecast sweep re-seats its herd at: large enough that the
    /// Eradicate ceiling exceeds a single hunter's throughput (a labor-bound case), while 20 hunters
    /// overstaff every policy (the ceiling binds).
    const BIG_HERD_CAP: f32 = 1_000.0;

    /// **A remnant herd, standing barely above the deepest escapement floors.** With `K = 1000` it is
    /// under Sustain's `K/2` and under Surplus's `0.30·K` (so those rows honestly offer nothing),
    /// a hair above Deplete's `0.15·K`, and — with `TEST_GAME_BODY_MASS = 5.0` — its Eradicate room
    /// is a handful of whole animals rather than a smooth fraction. That is the regime where a
    /// forecast is easiest to get wrong: near-empty rows, quantisation biting hard, and the standing
    /// stock within a rounding error of the ceiling.
    const DRAWN_DOWN_BIOMASS: f32 = 155.0;

    /// **The rung-3 shape: the POLICY axis collapses, the WORKER cap does not** (slice 7). A **Field**
    /// and a **pen** are yours — you control their reproduction, so no policy takes more or less than
    /// the managed yield. But you still have to carry the harvest home, so `per_worker_yield` is the
    /// crew's real throughput and `max_useful_workers` is the honest `ceil(production / per_worker)`.
    ///
    /// **Retargeted, not weakened.** This test used to be
    /// `tended_patch_and_corral_forecast_full_yield_with_one_worker` and asserted
    /// `max_useful_workers == 1` for every policy — pinning the two defects this slice fixes: the
    /// forecast encoded "one worker collects everything the land offers", and it covered *tended*
    /// patches, which are rung **2** and never belonged in the managed shape at all. Both claims are
    /// now inverted deliberately: the worker count must exceed 1 on a source this rich, and the
    /// fixture is a **Field**. The rung-2 half moved to
    /// `a_tended_patch_is_policy_live_worker_capped_and_can_be_over_farmed`.
    #[test]
    fn a_field_and_a_pen_collapse_the_policy_axis_but_still_need_carrying_home() {
        let (mut world, tile) = world_with_source(CAP);
        let labor = world.resource::<LaborConfigHandle>().get();
        let patch_cap = world
            .resource::<LaborConfigHandle>()
            .get()
            .forage
            .capacity_for(SOURCE_BIOME);
        sow_source_patch(&mut world, patch_cap);
        {
            let mut registry = world.resource_mut::<HerdRegistry>();
            assert!(
                registry.herds[0]
                    .corral_at(SOURCE, &crate::intensification::LadderConfig::builtin()),
                "the fixture species must be pennable"
            );
        }

        let patch = world
            .resource::<ForageRegistry>()
            .patch(SOURCE)
            .cloned()
            .expect("seeded patch");
        let composition = source_tile_composition(&world);
        let patch_forecast = forage_forecast(
            &patch,
            &composition,
            &labor.forage,
            &FloraConfig::builtin(),
            crate::forage::forage_per_worker_biomass(equipped_gather_rate(), SEASONAL_WEIGHT),
            NEUTRAL_OUTPUT_MULT,
            &TakeSelection::EVERYTHING,
        );
        let hunt_per_worker = equipped_haul_rate();
        drop(labor);
        let herd = world
            .resource::<HerdRegistry>()
            .find(HERD_ID)
            .cloned()
            .expect("seeded herd");
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let herd_forecast = hunt_forecast(
            &herd,
            &fauna,
            hunt_per_worker,
            &crate::fauna::HuntingParty::builtin_equipped(),
            NEUTRAL_OUTPUT_MULT,
        );
        drop(fauna);

        // **⛔ THE FLOOR AXIS IS LIVE ON A FIELD, AND THAT IS THE POINT.** It used to collapse: rung
        // 3 paid a flat managed rate on a crop that was never drawn down, so the harvest floor — the
        // one pressure lever the player holds — did **nothing** on the rung the whole ladder climbs
        // toward. A rung may change production; **no rung changes the draw**, so a Field is
        // floor-live, worker-capped and drawn down like every other plant rung, and it can be
        // over-farmed.
        //
        // Asserted as **monotonicity in the floor**: a higher escapement floor holds more stock back,
        // so every step up the sweep must take no more than the one below it — and the sweep as a
        // whole must **strictly** fall, which is the liveness half. A collapsed axis, the model this
        // arc retired, makes every step equal and fails that.
        let mut previous: Option<f32> = None;
        for policy in SWEPT_FLOORS {
            let ceiling = patch_forecast.ceiling_at(policy).provisions;
            if let Some(previous) = previous {
                assert!(
                    ceiling <= previous,
                    "a Field's take must fall as the harvest floor rises: floor {policy} took \
                     {ceiling} against the floor below it, which took {previous}"
                );
            }
            previous = Some(ceiling);
        }
        let first = patch_forecast.ceiling_at(SWEPT_FLOORS[0]).provisions;
        assert!(
            previous.is_some_and(|last| last < first),
            "…and it must actually move across the sweep, or the axis is collapsed after all: \
             {first} to {previous:?}"
        );
        // **⛔ THE FLOOR AXIS IS LIVE ON A PEN TOO, and for the Field's reason.** It used to
        // collapse: a penned herd paid a flat managed production at every stance, so the harvest
        // floor did nothing on the top rung of the animal ladder and a pen could not be over-hunted.
        // A rung may change production; **no rung changes the draw**.
        //
        // Asserted as monotonicity in the floor, with the strict fall as the liveness half — a
        // collapsed axis makes every step equal and fails that.
        let mut previous: Option<f32> = None;
        for policy in SWEPT_FLOORS {
            let ceiling = herd_forecast.ceiling_at(policy).provisions;
            if let Some(previous) = previous {
                assert!(
                    ceiling <= previous,
                    "a pen's take must fall as the harvest floor rises: floor {policy} took \
                     {ceiling} against the floor below it, which took {previous}"
                );
            }
            previous = Some(ceiling);
        }
        let first = herd_forecast.ceiling_at(SWEPT_FLOORS[0]).provisions;
        assert!(
            previous.is_some_and(|last| last < first),
            "…and it must actually move across the sweep, or the axis is collapsed after all: \
             {first} to {previous:?}"
        );

        // **The worker cap is NOT collapsed.** `per_worker_yield` is the crew's real throughput, so
        // this Field genuinely needs more than one pair of hands — the readout the old hardcoded `1`
        // made permanently false.
        let field_workers_needed = max_useful_workers(&patch_forecast, 0.5);
        assert!(
            field_workers_needed > 1,
            "a Field at capacity offers more than one worker can carry: {field_workers_needed}"
        );
        // **AND IT MOVES WITH THE FLOOR**, because the take does: a higher escapement floor offers
        // less, so fewer hands are useful. It used to be flat across the sweep, which was the
        // collapsed axis showing up in the crew readout.
        let mut previous: Option<u32> = None;
        for policy in SWEPT_FLOORS {
            let needed = max_useful_workers(&patch_forecast, policy);
            if let Some(previous) = previous {
                assert!(
                    needed <= previous,
                    "the hands a Field can use must fall as its floor rises: floor {policy} wanted \
                     {needed} against the floor below it, which wanted {previous}"
                );
            }
            previous = Some(needed);
        }

        // Staffed to exactly that count, the crew collects the whole production — and that IS what
        // the sim pays. Understaffed by one, it collects strictly less: the cap really binds.
        let field_band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    floor: 0.5,
                    species: None,
                    take_species: TakeSelection::EVERYTHING,
                },
                workers: field_workers_needed,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        let short_handed = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.5,
                },
                workers: 1,
                // **The keeper is NAMED onto a sledded kit deliberately.** A pen is collected on
                // `EquipmentStat::HuntCarry` — carry is a fact about the people and their gear and
                // never about the ground (issue #543) — and `herd_forecast` above is quoted at
                // `equipped_haul_rate()`, so naming a kit that carries a sled is what makes the
                // forecast and the payout the same number. Leaving it `None` would still resolve
                // `big_game`, which carries one; naming it keeps the fixture's claim independent of
                // what `default_kits.hunt` happens to be. (This comment used to say the row existed
                // because *"only the husbandry kit supplies"* the pen's rate — that kit and that stat
                // are both gone.)
                kit: Some(
                    crate::equipment_config::EquipmentConfig::builtin()
                        .kit("big_game")
                        .expect("the shipped roster carries the big-game kit"),
                ),
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        world.run_system_once(advance_labor_allocation);

        let field_row = world
            .get::<LaborAllocation>(field_band)
            .unwrap()
            .last_yields[0]
            .clone();
        let field_forecast = expected_yield(&patch_forecast, field_workers_needed, 0.5);
        assert!(field_forecast > 0.0);
        assert!(
            (field_row.actual - field_forecast).abs() < FORECAST_EPSILON,
            "Field forecast must equal the actual payout: {field_forecast} vs {}",
            field_row.actual
        );
        assert!(
            (field_row.actual - patch_forecast.ceiling_at(0.5).provisions).abs() < FORECAST_EPSILON,
            "a fully-staffed Field collects everything its FLOOR offers — which is a floor-live \
             ceiling now, not the retired managed rate: {} against {}",
            field_row.actual,
            patch_forecast.ceiling_at(0.5).provisions
        );
        assert!(
            field_row.wasted < FORECAST_EPSILON,
            "a fully-staffed Field wastes nothing: {}",
            field_row.wasted
        );

        let pen_row = world
            .get::<LaborAllocation>(short_handed)
            .unwrap()
            .last_yields[0]
            .clone();
        let pen_forecast = expected_yield(&herd_forecast, 1, 0.5);
        assert!(pen_forecast > 0.0);
        assert!(
            (pen_row.actual - pen_forecast).abs() < FORECAST_EPSILON,
            "pen forecast must equal the actual payout: {pen_forecast} vs {}",
            pen_row.actual
        );
    }

    // **RETIRED: `a_pen_harvest_that_wastes_charges_the_handling_gear_for_more_than_the_sled`**, and
    // its three fixture constants (`UNSEATABLE_BODY_MASS`, `UNSEATABLE_PEN_CAP`,
    // `PEN_STOCK_ABOVE_ESCAPEMENT`) with it — it asserted `butchered > hauled` across **two** items,
    // and the material half of the standing upkeep put both quanta on the **sled**
    // (`docs/plan_standing_upkeep.md` §4.9 item 12): the hurdles became a material, so the pen's
    // collection rate and `biomass_collected` moved to the sled beside `biomass_hauled` — and the
    // rate was then deleted outright (issue #543). The comparison would now be
    // the sled against itself and would pass vacuously. What survives untested here is the two
    // BASES, which `equipment.md` still states: a pen charges `killed_biomass` on one quantum and
    // `carried` on the other.

    /// **Rung 2 is a WILD stand, and since Flora Roster S2 it is a NEUTRAL one** — the plant twin of a
    /// *pastoral* herd, but no longer on a boosted curve. A *bare* (uncommitted) tended patch is
    /// Sustain-gathered at **exactly wild MSY** (`wild MSY × tended_regrowth_gain`, and the gain is now
    /// `1.0`): it regrows and yields exactly as fast as the same patch wild. It still **draws down**
    /// like any wild stand and is marked tended-this-turn — this test pins that neutrality plus those
    /// rung mechanics (it draws down, marks the patch worked, and its Sustain take is honestly
    /// sustainable).
    ///
    /// **The intensification incentive moved to the committed crop.** It was once a flat managed rate (no
    /// draw-down), then a boosted MSY curve; S2 retired the boost because, with S1 making
    /// competitor-removal explicit as a *composition* term, a growth boost double-counted it. So
    /// "tended beats wild" now lives entirely in a committed crop — **weeding + conversion** (§4.3) — and
    /// is pinned by the roster's own bar (`core_sim/tests/flora_roster.rs`) and `flora_commitment.rs`,
    /// which see the crop this scale-free rung mechanic cannot.
    #[test]
    fn a_bare_tended_patch_is_neutral_versus_wild_and_draws_down() {
        let (mut world, tile) = world_with_source(CAP);
        let cfg = world.resource::<LaborConfigHandle>().get();
        let forage = cfg.forage.clone();
        let patch_cap = forage.capacity_for(SOURCE_BIOME);
        let biomass = patch_cap;
        // **The wild rate is this tile's own basket average** (#433), not the flat
        // `provisions_per_biomass` — the point of "bare tended is neutral" is that a patch with no
        // crop committed reads exactly what the same ground reads wild, whatever that ground grows.
        let composition = source_tile_composition(&world);
        let wild_rate = {
            let flora = world
                .resource::<crate::flora_config::FloraConfigHandle>()
                .get();
            let wild = ForagePatch::new(SOURCE, patch_cap);
            crate::forage::patch_provisions_per_biomass(&wild, &composition, &flora, &forage)
        };
        // The **wild counterfactual take**: the stock standing above Sustain's escapement floor,
        // capped by the crew. It is deliberately computed off the wild patch's numbers — the whole
        // claim is that a bare tended patch pays exactly this.
        let wild_take = {
            let crew = WORKERS as f32
                * crate::forage::forage_per_worker_biomass(equipped_gather_rate(), 1.0);
            crew.min(biomass - patch_cap * crate::fauna::MSY_BIOMASS_FRACTION) * wild_rate
        };
        cultivate_source_patch(&mut world, biomass);

        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: UVec2::new(0, 0),
                    floor: 0.5,
                    species: None,
                    take_species: TakeSelection::EVERYTHING,
                },
                workers: WORKERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );

        world.run_system_once(advance_labor_allocation);

        // A bare tended patch reads exactly what the same ground reads wild. Under constant
        // escapement that is now true for **two independent reasons** and the test pins both: the
        // ceiling is `r`-free (so the rung's boosted curve cannot enter it at all), and with no crop
        // committed the conversion rate is the wild basket's.
        let expected = wild_take;
        let paid = world
            .get::<PopulationCohort>(band)
            .unwrap()
            .stores
            .get(FOOD)
            .to_f32();
        assert!(
            (paid - expected).abs() < 1e-3,
            "bare tended band gathers the wild escapement surplus: {paid} vs {expected}"
        );
        // **It draws down** — the correction. A tended patch is a wild stand, so gathering it takes
        // biomass out of it, which is what makes over-farming it possible at all.
        let patch = world
            .resource::<ForageRegistry>()
            .patch(UVec2::new(0, 0))
            .unwrap();
        assert!(
            patch.biomass < biomass,
            "a tended patch is still gathered from a real stock: {} vs {biomass}",
            patch.biomass
        );
        // **And gathering it does NOT hold it.** The band above staffed no keepers, so the patch's
        // whole upkeep went unmet — the behavioural headline of `docs/plan_standing_upkeep.md` §2.4,
        // and the exact case the retired `tended_this_turn` flag spared for free.
        assert_eq!(
            patch.upkeep_supplied, NO_UPKEEP_DEMAND,
            "a gathering crew supplies nothing toward the keeping"
        );
        let ladder = world.resource::<LadderConfigHandle>().get();
        let labor_cfg = world.resource::<LaborConfigHandle>().get();
        // The bill is quoted per tender-load of this ground's own `K` — resolved through the
        // config rather than assumed, since the harness stands on thin steppe, not the reference
        // tile.
        let tile_capacity = labor_cfg.forage.capacity_for(SOURCE_BIOME);
        assert!(
            crate::forage::patch_upkeep_shortfall(patch, &ladder, tile_capacity, &labor_cfg.forage,)
                > NO_UPKEEP_DEMAND,
            "so a gathered-but-unkept tended patch is running a shortfall"
        );
        // Telemetry: `sustainable` is a *measured* MSY line, and a Sustain take is sustainable by
        // its FLOOR (`overdraws`), not by being under that line — the first harvest of a full patch
        // is its accumulated stock and legitimately exceeds one turn's regrowth.
        let row = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();
        assert!((row.actual - expected).abs() < 1e-3);
        assert!(!row.overdraws, "a Sustain gather never overdraws: {row:?}");
    }

    /// **The playtest bug, pinned: every policy on a completed Tended Patch forecast the identical
    /// number.** Rung 2 reads the policy axis again — four policies, four different takes, ordered as
    /// their design intends — and Surplus really does over-farm the patch, so the overdraw ⚠ can
    /// finally fire on the plant web's rung 2. Before slice 7 the managed branch recorded
    /// `sustainable == actual` by construction, so `actual > sustainable` was unreachable here.
    ///
    /// Measured on a **drawn-down** patch (a patch being farmed is below capacity), deliberately.
    /// **Since Flora Roster S2 the gain is neutral (`1.0`)**, so a tended patch reads the same curve as
    /// a wild one and the policies fall in their natural order: Sustain (MSY) < Surplus (`1.6 × MSY`) <
    /// Deplete (20% of biomass) < Eradicate (30%). (At the retired gain 2.0 the boosted Surplus rode
    /// past the flat Deplete skim; that swap is gone with the boost.)
    #[test]
    fn a_tended_patch_is_policy_live_worker_capped_and_can_be_over_farmed() {
        let extractive = [0.5, 0.3, 0.15, 0.0];
        // A real operating point: a patch under active harvest sits below its cap (still above K/2, so
        // Sustain reads the MSY plateau). Full-cap would land Surplus exactly on Deplete (see docstring).
        const OPERATING_FRACTION: f32 = 0.8;
        let mut takes: Vec<(f32, f32)> = Vec::new();
        for policy in extractive {
            let (mut world, tile) = world_with_source(CAP);
            let patch_cap = world
                .resource::<LaborConfigHandle>()
                .get()
                .forage
                .capacity_for(SOURCE_BIOME);
            cultivate_source_patch(&mut world, patch_cap * OPERATING_FRACTION);
            let band = spawn_band(
                &mut world,
                tile,
                vec![LaborAssignment {
                    target: LaborTarget::Forage {
                        tile: SOURCE,
                        floor: policy,
                        species: None,
                        take_species: TakeSelection::EVERYTHING,
                    },
                    workers: WORKERS,
                    kit: None,
                    priority: SourcePriority::default(),
                    upkeep_kit: None,
                }],
            );
            world.run_system_once(advance_labor_allocation);
            let row = world.get::<LaborAllocation>(band).unwrap().last_yields[0].clone();
            let patch = world.resource::<ForageRegistry>().patch(SOURCE).unwrap();
            assert!(
                patch.biomass < patch_cap,
                "{policy:?} must draw the tended patch down"
            );
            if policy >= crate::fauna::MSY_BIOMASS_FRACTION {
                // Sustainable **by its floor**, not by sitting under the MSY line: a first harvest of
                // a patch standing above `K/2` legitimately takes the accumulated surplus, and lands
                // the patch exactly on its most productive biomass.
                assert!(
                    !row.overdraws,
                    "Sustain stops at the MSY point — no ⚠: {row:?}"
                );
                assert!(
                    patch.biomass >= patch_cap * crate::fauna::MSY_BIOMASS_FRACTION - 1e-3,
                    "Sustain never draws a tended patch below `K/2`: {row:?}"
                );
            } else {
                assert!(
                    row.actual > row.sustainable,
                    "{policy:?} over-farms a tended patch — the ⚠ that could never fire before: \
                     {row:?}"
                );
            }
            takes.push((policy, row.actual));
        }
        // Four policies, four DIFFERENT takes — the playtest's "+0.66 whatever I pick", inverted.
        for (i, (policy, take)) in takes.iter().enumerate() {
            for (other_policy, other) in takes.iter().skip(i + 1) {
                assert!(
                    (take - other).abs() > 1e-3,
                    "the policy axis must be live on a tended patch: {policy:?} and \
                     {other_policy:?} both pay {take}"
                );
            }
        }
        // ...and ordered as the axis means: restraint takes least, denial takes most. At the S2 neutral
        // gain the tended patch reads the wild curve, so the natural order holds end to end — Sustain
        // the leanest, then the boosted Surplus, then the flat Deplete skim, Eradicate the deepest.
        let take_of = |wanted: f32| {
            takes
                .iter()
                .find(|(policy, _)| *policy == wanted)
                .expect("every policy ran")
                .1
        };
        assert!(take_of(0.5) < take_of(0.3));
        assert!(take_of(0.3) < take_of(0.15));
        assert!(take_of(0.15) < take_of(0.0));
    }

    /// Place-locality: only the band that tends the cultivated patch is paid. A second same-faction
    /// band that does not tend it (forages an empty neighbor tile) receives nothing — the retired
    /// even-split would have paid it a share.
    #[test]
    fn tended_yield_is_place_local_not_split() {
        let (mut world, tile) = world_with_source(CAP);
        let patch_cap = world
            .resource::<LaborConfigHandle>()
            .get()
            .forage
            .capacity_for(SOURCE_BIOME);
        cultivate_source_patch(&mut world, patch_cap);

        // Band A tends the cultivated patch on (0,0).
        let tending = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: UVec2::new(0, 0),
                    floor: 0.5,
                    species: None,
                    take_species: TakeSelection::EVERYTHING,
                },
                workers: WORKERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        // Band B (same faction) forages the neighbor tile (1,0), which has no food module/patch →
        // it earns nothing from the cultivated patch.
        let idle_tile = world.resource::<TileRegistry>().tiles[1];
        let non_tending = spawn_band(
            &mut world,
            idle_tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: UVec2::new(1, 0),
                    floor: 0.5,
                    species: None,
                    take_species: TakeSelection::EVERYTHING,
                },
                workers: WORKERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );

        world.run_system_once(advance_labor_allocation);

        let tending_food = world
            .get::<PopulationCohort>(tending)
            .unwrap()
            .stores
            .get(FOOD)
            .to_f32();
        let other_food = world
            .get::<PopulationCohort>(non_tending)
            .unwrap()
            .stores
            .get(FOOD)
            .to_f32();
        assert!(
            tending_food > 0.0,
            "the tending band is paid: {tending_food}"
        );
        assert!(
            other_food.abs() < 1e-9,
            "a non-tending same-faction band gets no tended yield (no even-split): {other_food}"
        );
    }

    /// **The free path is gone.** Sustain-foraging a Thriving patch still *teaches the faction
    /// Cultivation* (Rung 1b knowledge, earned by doing), but it **never** accrues
    /// `cultivation_progress` any more — not even once the faction knows Cultivation. Cultivating is
    /// an explicit policy with an investment cost, not a free by-product of gathering.
    #[test]
    fn sustain_forage_teaches_cultivation_but_never_accrues_patch_progress() {
        let (mut world, tile) = world_with_source(CAP * 0.5);
        spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    floor: 0.5,
                    species: None,
                    take_species: TakeSelection::EVERYTHING,
                },
                workers: WORKERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );

        world.run_system_once(advance_labor_allocation);
        let learned = world
            .resource::<DiscoveryProgressLedger>()
            .get_progress(FactionId(0), CULTIVATION_DISCOVERY_ID)
            .to_f32();
        assert!(
            learned > 0.0,
            "Sustain-forage still earns Cultivation knowledge: {learned}"
        );
        assert_eq!(
            patch_progress(&world),
            0.0,
            "Sustain must not silently tame the patch"
        );

        // Even with Cultivation fully known, Sustain still accrues nothing — the old free path.
        world
            .resource_mut::<DiscoveryProgressLedger>()
            .add_progress(FactionId(0), CULTIVATION_DISCOVERY_ID, scalar_one());
        world.run_system_once(advance_labor_allocation);
        assert_eq!(
            patch_progress(&world),
            0.0,
            "knowing Cultivation must not make Sustain tame the patch — Cultivate is the only path"
        );
    }

    /// The source patch's live position on the plant ladder.
    fn patch_progress(world: &World) -> f32 {
        world
            .resource::<ForageRegistry>()
            .patch(SOURCE)
            .expect("seeded patch")
            .ladder_position()
    }

    /// **A map seed whose realization of the source tile is worth tending.** Per-tile realization
    /// (§10) draws a different basket per `(map_seed, tile)`, and under the default seed 0 the
    /// harness tile realizes its staple at a diluted ~0.40 share — correct behaviour, but not the
    /// "a crop is at home here" ground a *payoff* test needs. Seed 3 realizes `seed_grasses` at
    /// ~0.77, so weeding saturates and the tended payoff clears wild by a visible margin.
    const WORTH_TENDING_SEED: u64 = 3;

    /// Grant the harness faction full knowledge of a discovery (the Rung 1b/1c ledger gate that the
    /// Cultivate / Corral improvements check).
    fn grant_knowledge(world: &mut World, discovery: u32) {
        world
            .resource_mut::<DiscoveryProgressLedger>()
            .add_progress(BAND_FACTION, discovery, scalar_one());
    }

    /// **Cultivate is an investment, and the WHOLE turn is the price.** With Cultivation known,
    /// a crew working a patch under `Cultivate` takes **nothing** — its work budget went into the
    /// meter — while progress accrues each turn; once the meter reaches the job's cost the patch is
    /// cultivated and pays the full tended yield, strictly more than the wild Sustain skim.
    ///
    /// **It used to pay `yield_fraction_while_building × the Sustain yield`** and the dip only
    /// showed where hands were the scarce thing, so a crew big enough to saturate the ceiling built
    /// for free. Under one budget (`docs/plan_standing_upkeep.md` §2.2) the cost is the same for
    /// every crew size and it is total, which is what makes *"assign more hands if you want both"*
    /// the actual decision.
    #[test]
    fn cultivate_policy_takes_nothing_then_pays_the_tended_yield() {
        let (mut world, tile) = world_with_source(CAP);
        // **Both halves of this test must stand on the SAME ground.** The payoff is read against
        // the Sustain yield, and since #433 that yield is the tile's own basket average — so the
        // Sustain baseline and the Cultivate run have to share the seed that decides the tile's
        // realization (see the note on the Cultivate world below), or the comparison is between two
        // baskets.
        world.resource_mut::<SimulationConfig>().map_seed = WORTH_TENDING_SEED;
        grant_knowledge(&mut world, CULTIVATION_DISCOVERY_ID);
        let (work_per_turn, turns_to_prepare) = {
            let ladder = world.resource::<LadderConfigHandle>().get();
            let tended = ladder.rung(RungKey::PlantTended);
            (
                build_work_per_turn(tended, FOOD_PEAK_FLOOR, harness_patch_load(&world)),
                turns_to_finish(
                    tended,
                    FOOD_PEAK_FLOOR,
                    RUNG_COST_UNSCALED,
                    harness_patch_load(&world),
                ),
            )
        };

        // Baseline: what the same patch pays under Sustain (the MSY skim) with ample workers.
        let sustain_world_band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    floor: 0.5,
                    species: None,
                    take_species: TakeSelection::EVERYTHING,
                },
                workers: WORKERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        world.run_system_once(advance_labor_allocation);
        let sustain_yield = world
            .get::<LaborAllocation>(sustain_world_band)
            .unwrap()
            .last_yields[0]
            .actual;

        // Cultivate on a fresh patch: the take is the dip, and progress accrues.
        let (mut world, tile) = world_with_source(CAP);
        // Seat this world on a map seed where the source tile's per-tile realization (§10) puts its
        // dominant staple high — with F5's fuller PrairieSteppe basket, tile (0,0) realizes a diluted
        // slice under the default seed 0 (seed_grasses at share ~0.40, not worth tending), which is
        // *correct* realization behaviour but not the "worth-tending tile" this yield test needs.
        world.resource_mut::<SimulationConfig>().map_seed = WORTH_TENDING_SEED;
        grant_knowledge(&mut world, CULTIVATION_DISCOVERY_ID);
        let builders = plant_builders(&world, RungKey::PlantTended);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    floor: 0.5,
                    species: None,
                    take_species: TakeSelection::EVERYTHING,
                },
                workers: WORKERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        declare_patch_build(&mut world, band, SOURCE, Improvement::Cultivate, builders);
        world.run_system_once(advance_labor_allocation);
        let preparing = world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual;
        // **THE WHOLE BUDGET went into the meter, so the take is zero** — and it is zero at *every*
        // crew size, which is the change. Under the retired dip this crew, big enough to saturate
        // the ceiling several times over, paid nothing at all for the build (the ceiling bound it
        // either way); the sparse-crew case below is now the same statement rather than the
        // contrasting one.
        assert!(
            sustain_yield > 0.0,
            "the baseline must be a real take, or the comparison proves nothing: {sustain_yield}"
        );
        assert!(
            (preparing - sustain_yield).abs() < FORECAST_EPSILON,
            "**the gatherers are untouched by the build beside them** — a Cultivate is staffed in \
             its own right, so what it costs is the hands on it, not a share of theirs: \
             {preparing} vs {sustain_yield}"
        );
        assert!(
            (patch_progress(&world) - work_per_turn).abs() < 1e-6,
            "one Cultivate turn banks this crew's whole output: {}",
            patch_progress(&world)
        );

        // Run it to completion. The regrowth system runs alongside (as it does in the real Logistics
        // stage) — a preparing crew takes nothing at all, so the patch is untouched while the ground
        // is prepared and stays healthy by construction.
        for _ in 0..turns_to_prepare {
            world.run_system_once(advance_forage_regrowth);
            world.run_system_once(advance_labor_allocation);
        }
        assert_eq!(
            world
                .resource::<ForageRegistry>()
                .patch(SOURCE)
                .unwrap()
                .ecology_phase,
            EcologyPhase::Thriving,
            "a preparing crew draws nothing — the patch never leaves Thriving"
        );
        assert!(
            world
                .resource::<ForageRegistry>()
                .patch(SOURCE)
                .unwrap()
                .is_cultivated(),
            "sustained Cultivate work completes the patch"
        );
        // **Harvest the finished patch to read the payoff.** The loop above already ran past the
        // completing turn, so the sim has retired `Cultivate` onto the harvest rung itself (issue
        // #420) and this call is a no-op re-assert — kept because what this test measures is the
        // *payoff*, and it must read that number off the harvest rung whatever put the band there.
        // The retire itself is pinned by
        // `a_completed_cultivation_retires_the_build_verb_onto_the_harvest_rung`.
        set_forage_floor(&mut world, band, 0.5);
        // One Logistics turn first: under constant escapement a patch that was just gathered is
        // sitting **on** its floor with nothing above it, so a payoff read without the regrowth would
        // measure an empty turn rather than the rung.
        world.run_system_once(advance_forage_regrowth);
        world.run_system_once(advance_labor_allocation);
        let tended = world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual;
        assert!(
            tended > sustain_yield,
            "a tended patch out-pays the wild Sustain skim — the whole point of the 25 turns: \
             {tended} vs {sustain_yield}"
        );
        assert!(
            tended > preparing,
            "the payoff exceeds what the build turn paid: {tended} vs {preparing}"
        );

        // **…and a SPARSE crew is charged exactly the same way.** The budget is a share of the
        // crew's own turn, so the cost does not depend on whether hands or the ceiling was the
        // binding term — which is precisely what the retired dip could not say.
        let sparse_take = |improvement: Option<Improvement>| {
            let (mut world, tile) = world_with_source(CAP);
            world.resource_mut::<SimulationConfig>().map_seed = WORTH_TENDING_SEED;
            grant_knowledge(&mut world, CULTIVATION_DISCOVERY_ID);
            let band = spawn_band(
                &mut world,
                tile,
                vec![LaborAssignment {
                    target: LaborTarget::Forage {
                        tile: SOURCE,
                        floor: 0.5,
                        species: None,
                        take_species: TakeSelection::EVERYTHING,
                    },
                    workers: SOLE_FORAGER,
                    kit: None,
                    priority: SourcePriority::default(),
                    upkeep_kit: None,
                }],
            );
            if let Some(declared) = improvement {
                // **A pool of the same size stands on the build** — what this fixture meant when
                // one crew did every job (`docs/plan_standing_upkeep.md` §2.5).
                declare_patch_build(&mut world, band, SOURCE, declared, SOLE_FORAGER);
            }
            world.run_system_once(advance_labor_allocation);
            world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual
        };
        let sparse_building = sparse_take(Some(Improvement::Cultivate));
        let sparse_gathering = sparse_take(None);
        assert!(
            sparse_gathering > 0.0,
            "the lone forager must gather something: {sparse_gathering}"
        );
        assert!(
            (sparse_building - sparse_gathering).abs() < FORECAST_EPSILON,
            "…and exactly as much with a Cultivate staffed beside them: {sparse_building} vs \
             {sparse_gathering}"
        );
        // The exact composition `min(effective_workers × per_worker, ceiling)` is pinned per
        // component and at both binding regimes by
        // `forage_forecast_equals_actual_take_for_every_floor_and_staffing`, against a real
        // `advance_labor_allocation` run — not restated here.
    }

    /// **One forager**, so the crew's throughput is the binding term rather than the patch's
    /// standing stock. It used to be the only regime in which the build dip was visible at all;
    /// under one work budget the build's cost is visible at every staffing, and this fixture now
    /// shows that the *sparse* end is charged the same way as the saturating one.
    const SOLE_FORAGER: u32 = 1;

    /// **Corral mirrors Cultivate.** With Penning known and a domesticated herd it owns, a band
    /// working it under `Corral` takes **nothing** while the pen accrues — its whole work budget is
    /// on the fence; at `corral_progress == 1.0` the herd is penned and pays the corral yield.
    ///
    /// It used to take `corralling_yield_fraction × the Sustain (MSY) yield`; see the plant twin for
    /// why the dip dissolved into the budget.
    #[test]
    fn corral_policy_takes_nothing_then_pens_and_pays_the_corral_yield() {
        const BIG_HERD_CAP: f32 = 1_000.0;
        /// Seat the herd a little **above** its `K/2` escapement point: enough spare biomass that a
        /// Sustain take is a real, ceiling-bound number, few enough animals that 10 hunters can carry
        /// all of them.
        const DIP_TEST_ESCAPEMENT_FRACTION: f32 = 0.55;
        let turns_to_build = {
            let (world, _) = world_with_source(CAP);
            let ladder = world.resource::<LadderConfigHandle>().get();
            turns_to_finish(
                ladder.rung(RungKey::AnimalPen),
                FOOD_PEAK_FLOOR,
                RUNG_COST_UNSCALED,
                harness_herd_load(&world),
            )
        };

        // Baseline Sustain hunt yield on the same herd (ample hunters → **ceiling**-bound).
        // **It must be DOMESTICATED too**: Corral can only be worked on a domesticated herd, and the
        // husbandry ladder means a tamed herd lives on the *pastoral* ecology (`r` = 0.15, 3× wild).
        // Comparing the dip against a *wild* herd's MSY would compare two different rungs.
        //
        // **RETARGETED IN SLICE 8 — the herd is seated JUST ABOVE its escapement point, not at
        // capacity.** "The dip pays `fraction ×` the Sustain yield" is only true when **both** takes
        // are ceiling-bound; the moment Sustain becomes *collection*-bound the dip is a fraction of a
        // ceiling the baseline never reached, and the identity is arithmetically false rather than
        // broken. At capacity that is now exactly what happens: escapement is `K/2` = 500 biomass, so
        // 10 hunters (400) are no longer "ample" — Sustain reads 8, Corral reads its full ceiling 5,
        // and `0.5 × 8 = 4 ≠ 5`. Seating the herd at `0.55 × K` restores the fixture's own stated
        // premise (a small escapement the crew can comfortably carry), so the test measures the dip
        // instead of measuring the carry cap.
        let (mut world, tile) = world_with_source(CAP);
        reseat_herd(
            &mut world,
            BIG_HERD_CAP * DIP_TEST_ESCAPEMENT_FRACTION,
            BIG_HERD_CAP,
        );
        {
            let mut registry = world.resource_mut::<HerdRegistry>();
            registry.herds[0].tame_outright(
                BAND_FACTION,
                &crate::intensification::LadderConfig::builtin(),
            );
        }
        let sustain_band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.5,
                },
                workers: WORKERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        world.run_system_once(advance_labor_allocation);
        let sustain_yield = world
            .get::<LaborAllocation>(sustain_band)
            .unwrap()
            .last_yields[0]
            .actual;

        // Corral on a domesticated herd the faction owns + knows **Penning** for (the §4.3
        // reshuffle: rung 3's gate moved off Herding, which now gates `tame` alone).
        let (mut world, tile) = world_with_source(CAP);
        reseat_herd(
            &mut world,
            BIG_HERD_CAP * DIP_TEST_ESCAPEMENT_FRACTION,
            BIG_HERD_CAP,
        );
        grant_knowledge(&mut world, PENNING_DISCOVERY_ID);
        {
            let mut registry = world.resource_mut::<HerdRegistry>();
            registry.herds[0].tame_outright(
                BAND_FACTION,
                &crate::intensification::LadderConfig::builtin(),
            );
        }
        let builders = animal_builders(&world, RungKey::AnimalPen);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.5,
                },
                workers: WORKERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        declare_herd_build(&mut world, band, HERD_ID, Improvement::Corral, builders);
        // **The pen eats hurdles now** — see [`stock_pen_materials`].
        stock_pen_materials(&mut world, band);
        world.run_system_once(advance_labor_allocation);
        let preparing = world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual;
        // **The whole budget is on the fence, so the keeper carries nothing home** — at every crew
        // size, unlike the retired dip, which this crew (ample enough that the herd's own escapement
        // bound it either way) escaped entirely.
        assert!(
            sustain_yield > 0.0,
            "the baseline must be a real take, or the comparison proves nothing: {sustain_yield}"
        );
        assert!(
            (preparing - sustain_yield).abs() < FORECAST_EPSILON,
            "**the hunters are untouched by the fence going up beside them** — a Corral is staffed \
             in its own right: {preparing} vs {sustain_yield}"
        );

        for _ in 0..turns_to_build {
            world.run_system_once(advance_labor_allocation);
        }
        assert!(
            world
                .resource::<HerdRegistry>()
                .find(HERD_ID)
                .unwrap()
                .is_corralled(),
            "sustained Corral work finishes the pen"
        );
        // This harness runs the Population stage ONLY — no Logistics, so the herd never regrows
        // while the pen is built. Re-seat it at capacity so this test measures what it is about: the
        // penned rung out-paying the build turn.
        reseat_herd(&mut world, BIG_HERD_CAP, BIG_HERD_CAP);
        world.run_system_once(advance_labor_allocation);
        let corral_yield = world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual;
        assert!(
            corral_yield > preparing,
            "a penned herd out-pays the build turn: {corral_yield} vs {preparing}"
        );

        // **…and a SPARSE crew is charged the same way**, which is what the budget bought: the cost
        // no longer depends on whether hands or the escapement was the binding term.
        let sparse_take = |improvement: Option<Improvement>| {
            let (mut world, tile) = world_with_source(CAP);
            reseat_herd(
                &mut world,
                BIG_HERD_CAP * DIP_TEST_ESCAPEMENT_FRACTION,
                BIG_HERD_CAP,
            );
            grant_knowledge(&mut world, PENNING_DISCOVERY_ID);
            {
                let mut registry = world.resource_mut::<HerdRegistry>();
                registry.herds[0].tame_outright(
                    BAND_FACTION,
                    &crate::intensification::LadderConfig::builtin(),
                );
            }
            let band = spawn_band(
                &mut world,
                tile,
                vec![LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: HERD_ID.to_string(),
                        floor: 0.5,
                    },
                    workers: SOLE_HUNTER,
                    kit: None,
                    priority: SourcePriority::default(),
                    upkeep_kit: None,
                }],
            );
            if let Some(declared) = improvement {
                // **A pool of the same size stands on the build** — what this fixture meant when
                // one crew did every job (`docs/plan_standing_upkeep.md` §2.5).
                declare_herd_build(&mut world, band, HERD_ID, declared, SOLE_HUNTER);
            }
            world.run_system_once(advance_labor_allocation);
            world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual
        };
        let sparse_building = sparse_take(Some(Improvement::Corral));
        let sparse_hunting = sparse_take(None);
        assert!(
            sparse_hunting > 0.0,
            "the lone hunter must take something: {sparse_hunting}"
        );
        assert!(
            (sparse_building - sparse_hunting).abs() < FORECAST_EPSILON,
            "…and exactly as much with a Corral staffed beside them: {sparse_building} vs \
             {sparse_hunting}"
        );
    }

    /// **One hunter**, so the crew's carry is the binding term rather than the herd's escapement. It
    /// used to be the only regime in which the build dip was visible; the budget charges the build
    /// at every staffing, and this fixture now shows the sparse end matching the saturating one.
    const SOLE_HUNTER: u32 = 1;

    // ---------------------------------------------------------------------------------------------
    // **Completion CLEARS the improvement and leaves the stance alone** (issues #420 + #442). All four
    // rungs share one seam: the turn a build meter fills, the assignment's `improvement` returns to
    // `None`, preserving the source, the commitment, the crew — and, since #442, the player's stated
    // stance. Left on the build verb the band paid `yield_fraction_while_building` forever on a rung
    // that could never accomplish anything more (#420); rewritten onto a hardcoded harvest stance, the
    // sim silently replaced a policy the player chose (#442).
    // ---------------------------------------------------------------------------------------------

    /// **A deliberately NON-default stance for the completion tests.** The handoff used to rewrite
    /// `policy` to `Sustain`, so a completion test run under Sustain could not tell "the stance was
    /// left alone" from "the stance was rewritten to the value it already had". Surplus is a real
    /// player choice and is *not* what the retired constant would have written.
    ///
    /// **Every completion test computes its build length AT this floor**, not at the food peak:
    /// since `docs/plan_harvest_floor.md` §3 the accrual is `crew output ×
    /// learn_multiplier(floor)`, so a builder holding `0.3` takes `0.5/0.3` times as many turns. A
    /// fixture that counted peak-rate turns would stop one short of the completion it is asserting.
    /// There is no health gate left for the floor to trip — pulling harder now *slows* the meter
    /// rather than stopping it.
    const BUILDER_FLOOR: f32 = 0.3;

    /// The client's pre-turn expected take on the source patch at `floor`, off the patch's
    /// **current** state — the same `forage_forecast` composition the forecast==actual sweep uses. Lets
    /// a test name the exact number a turn should pay without re-deriving the MSY/dip arithmetic.
    fn forage_expected_take(
        world: &World,
        workers: u32,
        floor: f32,
        _improvement: Option<Improvement>,
    ) -> f32 {
        let labor = world.resource::<LaborConfigHandle>().get();
        let patch = world
            .resource::<ForageRegistry>()
            .patch(SOURCE)
            .cloned()
            .expect("seeded patch");
        let composition = source_tile_composition(world);
        let forecast = forage_forecast(
            &patch,
            &composition,
            &labor.forage,
            &FloraConfig::builtin(),
            crate::forage::forage_per_worker_biomass(equipped_gather_rate(), SEASONAL_WEIGHT),
            NEUTRAL_OUTPUT_MULT,
            &TakeSelection::EVERYTHING,
        );
        expected_yield(&forecast, workers, floor)
    }

    /// **What the source tile grows** — the realized basket, through the one `tile_flora_composition`
    /// seam the labor arm reads, so a test forecast is priced off exactly the composition the turn
    /// will pay from (#433).
    fn source_tile_composition(world: &World) -> Vec<crate::flora_config::FloraShare> {
        let labor = world.resource::<LaborConfigHandle>().get();
        let flora = world
            .resource::<crate::flora_config::FloraConfigHandle>()
            .get();
        let map_seed = world.resource::<SimulationConfig>().map_seed;
        let tile_entity = world.resource::<TileRegistry>().tiles[0];
        let ground = world.get::<Tile>(tile_entity).expect("the source tile");
        crate::forage::tile_flora_composition(&flora, &labor.forage, ground, map_seed).into_owned()
    }

    /// The plant the source tile's realized basket auto-picks for `rung` — the same
    /// `default_species_for_rung` answer the labor arm reaches. Named **explicitly** on the test
    /// assignment so the retire pass can be asserted to carry the *commitment* across, not merely the
    /// tile coordinate.
    fn source_tile_default_crop(world: &World, rung: RungKey) -> String {
        let labor = world.resource::<LaborConfigHandle>().get();
        let flora = world
            .resource::<crate::flora_config::FloraConfigHandle>()
            .get();
        let map_seed = world.resource::<SimulationConfig>().map_seed;
        let tile_entity = world.resource::<TileRegistry>().tiles[0];
        let ground = world.get::<Tile>(tile_entity).expect("the source tile");
        let composition =
            crate::forage::tile_flora_composition(&flora, &labor.forage, ground, map_seed);
        crate::forage::default_species_for_rung(&composition, &flora, rung)
            .expect("the source tile grows something the tended rung can commit to")
    }

    /// The band's single **worked-source** assignment — completion edits the band's *queue*, so
    /// every field of the row is evidence that nothing on the row moved.
    ///
    /// **A band holds standing-role rows beside its sources** (`builders`, `agriculture`,
    /// `husbandry`), so the assertion is that it works exactly one **source**, not that it holds
    /// exactly one row.
    fn only_source_assignment(world: &World, band: Entity) -> LaborAssignment {
        let allocation = world.get::<LaborAllocation>(band).expect("the band works");
        let sources: Vec<&LaborAssignment> = allocation
            .assignments
            .iter()
            .filter(|assignment| {
                matches!(
                    assignment.target,
                    LaborTarget::Forage { .. } | LaborTarget::Hunt { .. }
                )
            })
            .collect();
        assert_eq!(
            sources.len(),
            1,
            "completion edits the band's queue, it never adds or drops a worked source"
        );
        sources[0].clone()
    }

    /// **What one band has queued on a source** — the 6b reading of what a fixture used to get from
    /// `only_source_assignment(..).improvement`.
    ///
    /// It answers the *declaration*, not the derived rung: the tests that call it are asserting that
    /// completion **retires the entry** and that nothing else touches it.
    fn queued_job(world: &World, band: Entity, source: BuildSource) -> Option<BuildJob> {
        world
            .get::<LaborAllocation>(band)
            .expect("the band works")
            .build_queue_entry(&source)
            .map(|entry| entry.declared)
    }

    /// **THE issue-#420 + #442 fix, plant rung 2.** A band whose patch finishes cultivating this
    /// turn:
    ///
    /// 1. still pays the **dipped** take on the completing turn (the accrue-after-take ordering — the
    ///    pre-commit forecast promised the dip, and completing must not retroactively pay more);
    /// 2. has its **improvement cleared** afterwards, with its worker count, its tile, its committed
    ///    species **and its stance** intact — the last of those being what #442 fixed: the sim used to
    ///    rewrite `policy` to a hardcoded Sustain, replacing a choice the player made;
    /// 3. **pays the undipped take the NEXT turn** — the actual #420 bug: left on the build verb the
    ///    band went on paying the dip forever on ground that was already prepared.
    #[test]
    fn a_completed_cultivation_clears_the_improvement_and_leaves_the_stance_alone() {
        let (mut world, tile) = world_with_source(CAP);
        // The same worth-tending seed `cultivate_policy_pays_the_dip_then_the_tended_yield` pins: the
        // source tile's realization must concentrate its staple hard enough that the tended payoff
        // clears wild, or step 3 would be measuring a marginal crop rather than the retire.
        world.resource_mut::<SimulationConfig>().map_seed = 3;
        grant_knowledge(&mut world, CULTIVATION_DISCOVERY_ID);
        let crop = source_tile_default_crop(&world, RungKey::PlantTended);
        let turns_to_prepare = {
            let ladder = world.resource::<LadderConfigHandle>().get();
            turns_to_finish(
                ladder.rung(RungKey::PlantTended),
                BUILDER_FLOOR,
                RUNG_COST_UNSCALED,
                harness_patch_load(&world),
            )
        };
        let builders = plant_builders(&world, RungKey::PlantTended);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    floor: BUILDER_FLOOR,
                    species: Some(crop.clone()),
                    take_species: TakeSelection::EVERYTHING,
                },
                workers: WORKERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        declare_patch_build(&mut world, band, SOURCE, Improvement::Cultivate, builders);

        // Every turn but the last: the meter fills and the entry stays put.
        for _ in 0..turns_to_prepare - 1 {
            world.run_system_once(advance_forage_regrowth);
            world.run_system_once(advance_labor_allocation);
        }
        assert!(
            !world
                .resource::<ForageRegistry>()
                .patch(SOURCE)
                .unwrap()
                .is_cultivated(),
            "fixture: the patch must still be under construction here"
        );
        assert_eq!(
            queued_job(&world, band, BuildSource::Patch(SOURCE)),
            Some(BuildJob::Rung(Improvement::Cultivate)),
            "an unfinished build keeps its entry — only completion retires it"
        );

        // (1) The completing turn still pays the dip, to the number.
        //
        // **Quoted BEFORE the regrowth, because that is where a client reads it**: the pre-commit
        // forecast prices the stand one Logistics regrowth ahead of the state it is taken from
        // (`forage::next_turns_stand`), so quoting it *after* the regrowth would project a second
        // one and promise a turn that never happens.
        let promised_dip =
            forage_expected_take(&world, WORKERS, BUILDER_FLOOR, Some(Improvement::Cultivate));
        world.run_system_once(advance_forage_regrowth);
        world.run_system_once(advance_labor_allocation);
        let completing = world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual;
        assert!(
            world
                .resource::<ForageRegistry>()
                .patch(SOURCE)
                .unwrap()
                .is_cultivated(),
            "fixture: this is the completing turn"
        );
        assert!(
            (completing - promised_dip).abs() < FORECAST_EPSILON,
            "the turn progress reaches 1.0 is the LAST preparing take — it pays the dip the \
             pre-commit forecast promised: {completing} vs {promised_dip}"
        );

        // (2) The handoff: the queue entry retired, and NOTHING else moved — least of all the
        // stance. Rewriting `policy` here is exactly what issue #442 deletes.
        let completed = only_source_assignment(&world, band);
        assert_eq!(completed.workers, WORKERS, "the crew stays on the source");
        assert_eq!(
            queued_job(&world, band, BuildSource::Patch(SOURCE)),
            None,
            "completion retires the entry — there is nothing left to build here"
        );
        let LaborTarget::Forage {
            tile: completed_tile,
            floor,
            take_species: _,
            species,
        } = &completed.target
        else {
            panic!("completion must not change the target's KIND: {completed:?}");
        };
        assert_eq!(
            *floor, BUILDER_FLOOR,
            "THE #442 fix: the sim never rewrites the player's floor — it was never vacated, so \
             there is nothing to hand back"
        );
        assert_eq!(*completed_tile, SOURCE, "the same ground");
        assert_eq!(
            species.as_deref(),
            Some(crop.as_str()),
            "the crop the crew committed 25 turns to survives the handoff"
        );

        // (3) The bug: the next turn pays the tended harvest, not the dip. Quoted before the
        // regrowth for (1)'s reason — the forecast projects that regrowth itself.
        let promised_harvest = forage_expected_take(&world, WORKERS, BUILDER_FLOOR, None);
        world.run_system_once(advance_forage_regrowth);
        world.run_system_once(advance_labor_allocation);
        let after = world.get::<LaborAllocation>(band).unwrap().last_yields[0].actual;
        assert!(
            (after - promised_harvest).abs() < FORECAST_EPSILON,
            "the band collects the undipped take under its OWN stance: {after} vs \
             {promised_harvest}"
        );
        assert!(
            after > completing,
            "the payoff the 25 turns bought arrives WITHOUT the player touching the picker — the \
             whole of issue #420: {after} vs the dip {completing}"
        );
    }

    /// One Logistics-stage regrowth for the source herd, through the shipped `fauna::regrow_biomass`
    /// — the **exact twin of the `advance_forage_regrowth` call the plant completion test above
    /// already makes**, and the asymmetry it removes.
    ///
    /// The completion harnesses otherwise drive the Population stage only, so a herd never regrows.
    /// A 42-turn build at [`BUILDER_FLOOR`] would then be asserted against a herd its own crew
    /// emptied to that floor in four turns — after which nothing stands above the floor and
    /// `crew_is_working_the_source` is correctly false. That is the sim behaving properly, not a
    /// gate to route around: a Population-only loop is half a turn, and a *completion* test needs the
    /// order the sim runs. (It was invisible before the harvest floor only because the retired
    /// `EcologyPhase::Thriving` gate read a phase `refresh_ecology_phase` never updated here, so it
    /// stayed frozen at the value `reseat_herd` set.)
    fn regrow_source_herd(world: &mut World) {
        let fauna = world.resource::<FaunaConfigHandle>().get();
        let mut registry = world.resource_mut::<HerdRegistry>();
        crate::fauna::regrow_biomass(&mut registry.herds[0], &fauna);
    }

    /// **The animal twin, rung 2.** A herd that finishes taming this turn hands its crew to the harvest
    /// rung with the herd id and the crew intact — so the band starts collecting the pastoral payoff
    /// instead of paying the taming dip on an already-tame herd forever.
    /// **Whole turns run to put real work on the harness patch's meter before anything is
    /// measured.** The plant keeping demand **interpolates on the position**, so a patch at
    /// [`RUNG_UNSTARTED`] honestly owes `0` and supplies `0` — a fixture that measured there would
    /// be comparing two zeroes and would pass with the guard ripped out. Two turns is enough for the
    /// demand to be positive and still climbing, which the second arm below needs.
    const WARM_UP_TURNS: usize = 2;

    /// **A patch part-way into a Cultivate with its `agriculture` role staffed**, its meter carrying
    /// real work and its keeping fully met — the one fixture shape in which the keeping figure
    /// actually moves, and therefore the only one in which a second labour pass can double it.
    ///
    /// The warm-up is driven as **whole turns**, so the world it hands back is one the sim can
    /// really be in: supply banked, bill stamped, and the next legal thing to do a Logistics clear.
    fn a_world_keeping_a_patch_it_is_building() -> World {
        let (mut world, tile) = world_with_source(CAP);
        world.resource_mut::<SimulationConfig>().map_seed = WORTH_TENDING_SEED;
        grant_knowledge(&mut world, CULTIVATION_DISCOVERY_ID);
        let crop = source_tile_default_crop(&world, RungKey::PlantTended);
        let builders = plant_builders(&world, RungKey::PlantTended);
        let keepers = the_harness_plant_keeping_crew(&world, RungKey::PlantTended);
        assert!(
            keepers > NO_CREW_ON_THIS_ACTIVITY,
            "fixture: the rung must cost something to hold, or nothing is being guarded: {keepers}"
        );
        let band = spawn_band(
            &mut world,
            tile,
            vec![
                LaborAssignment {
                    target: LaborTarget::Forage {
                        tile: SOURCE,
                        floor: BUILDER_FLOOR,
                        species: Some(crop),
                        take_species: TakeSelection::EVERYTHING,
                    },
                    workers: WORKERS,
                    kit: None,
                    priority: SourcePriority::default(),
                    upkeep_kit: None,
                },
                LaborAssignment {
                    target: LaborTarget::Agriculture,
                    workers: keepers,
                    kit: None,
                    priority: SourcePriority::default(),
                    upkeep_kit: None,
                },
            ],
        );
        declare_patch_build(&mut world, band, SOURCE, Improvement::Cultivate, builders);
        for _ in 0..WARM_UP_TURNS {
            advance_one_turn(&mut world);
        }
        world
    }

    /// The harness patch's keeping accounts as they stand right now.
    fn patch_keeping(world: &World) -> (f32, f32) {
        let patch = world
            .resource::<ForageRegistry>()
            .patch(SOURCE)
            .expect("the fixture seeded a patch");
        (
            patch.upkeep_supplied,
            patch.upkeep_demanded.expect("a worked patch is billed"),
        )
    }

    /// ⛔ **RUNNING THE LABOUR PASS TWICE WITH NO LOGISTICS BETWEEN IS LOUD, NOT SILENT.**
    ///
    /// `upkeep_supplied` accumulates across the bands working a source — deliberately, because the
    /// upkeep is per-source and two bands each put a fraction of it on the ground — while
    /// `upkeep_demanded` is stamped first-write-wins and never re-struck. Both are cleared a whole
    /// stage earlier. So a driver that skips the clear measures a **doubled supply against one
    /// turn's bill** and reports the keeping as better met than it is, in the flattering direction,
    /// with nothing anywhere saying so.
    ///
    /// The production ordering is not the defect and is not what this pins. What it pins is that the
    /// **misuse announces itself** — and its rescue arm below is what stops "always panic" from
    /// satisfying it.
    #[test]
    #[should_panic(expected = "ran twice with no Logistics pass")]
    fn a_second_labour_pass_with_no_logistics_between_is_refused() {
        let world = a_world_keeping_a_patch_it_is_building();
        let (supplied, _) = patch_keeping(&world);
        assert!(
            supplied > NO_UPKEEP_DEMAND,
            "fixture: the warm-up must leave real keeping banked, or a second pass has nothing to \
             double: {supplied}"
        );
        // **No clear between this and the warm-up's last pass** — the one thing a driver may not do.
        let mut world = world;
        world.run_system_once(advance_labor_allocation);
    }

    /// **THE RESCUE ARM — the same two passes, driven as two turns, bank ONE turn's keeping each.**
    ///
    /// Without this the guard above is satisfied by a pass that panics unconditionally. It also
    /// states the quantity the guard exists to protect: a fully-staffed keeping supplies **exactly**
    /// the bill it was handed, on turn two as on turn one. Under the defect the second turn's supply
    /// is the sum of both turns' shares against a bill stamped once — comfortably past the bill,
    /// which is precisely the over-statement.
    #[test]
    fn two_labour_passes_driven_as_two_turns_each_bank_one_turns_keeping() {
        let mut world = a_world_keeping_a_patch_it_is_building();

        let (first_supplied, first_billed) = patch_keeping(&world);
        assert!(
            first_billed > NO_UPKEEP_DEMAND,
            "fixture: the rung must cost something to hold: {first_billed}"
        );
        assert!(
            (first_supplied - first_billed).abs() < FORECAST_EPSILON,
            "a fully-staffed keeping supplies exactly its bill: {first_supplied} vs {first_billed}"
        );

        advance_one_turn(&mut world);
        let (second_supplied, second_billed) = patch_keeping(&world);
        assert!(
            (second_supplied - second_billed).abs() < FORECAST_EPSILON,
            "and it still does on the next turn — the supply is this turn's, not both turns': \
             {second_supplied} vs {second_billed}"
        );
        // **The bill MOVED between the two turns**, because the plant demand interpolates on the
        // position and the builders banked a turn's work in between. Without that the two arms
        // would be the same reading twice and a stale-basis defect could hide inside the epsilon.
        assert!(
            second_billed > first_billed,
            "fixture: the build must raise the demand between the turns, or the second arm is the \
             first one restated: {first_billed} -> {second_billed}"
        );
    }

    #[test]
    fn a_completed_taming_clears_the_improvement_and_leaves_the_stance_alone() {
        const BIG_HERD_CAP: f32 = 1_000.0;
        let (mut world, tile) = world_with_source(CAP);
        reseat_herd(&mut world, BIG_HERD_CAP, BIG_HERD_CAP);
        grant_knowledge(&mut world, HERDING_DISCOVERY_ID);
        let (turns_to_tame, species) = {
            let ladder = world.resource::<LadderConfigHandle>().get();
            let fauna = world.resource::<FaunaConfigHandle>().get();
            let species = world.resource::<HerdRegistry>().herds[0].species.clone();
            (
                turns_to_finish(
                    ladder.rung(RungKey::AnimalPastoral),
                    BUILDER_FLOOR,
                    fauna.taming_cost_multiplier_for(&species),
                    harness_herd_load(&world),
                ),
                species,
            )
        };
        assert!(
            turns_to_tame > 1,
            "fixture: the {species} herd must take more than one turn to gentle"
        );
        let builders = animal_builders(&world, RungKey::AnimalPastoral);
        // **THE BAND HAS TO STAFF ITS HUSBANDRY ROLE, and that is §4.6a showing through.** The
        // keeping pool owes a half-tamed herd's rate from the first work banked, and a herd whose
        // keeping is wholly unmet does not regrow (`fauna::regrow_biomass` reads
        // `upkeep_supplied <= 0`) — so an unkept herd pinned at its hunters' floor offers no
        // escapement room, the build's own gate goes false, and the Tame stalls forever. The
        // builders used to cover the rate themselves, which is exactly the coupling this slice
        // deleted; a fixture that wants a build to finish now has to state its keepers.
        let keepers = the_harness_keeping_crew(&world, RungKey::AnimalPastoral);
        let band = spawn_band(
            &mut world,
            tile,
            vec![
                LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: HERD_ID.to_string(),
                        floor: BUILDER_FLOOR,
                    },
                    workers: WORKERS,
                    kit: None,
                    priority: SourcePriority::default(),
                    upkeep_kit: None,
                },
                LaborAssignment {
                    target: LaborTarget::Husbandry,
                    workers: keepers,
                    kit: None,
                    priority: SourcePriority::default(),
                    upkeep_kit: None,
                },
            ],
        );
        declare_herd_build(&mut world, band, HERD_ID, Improvement::Tame, builders);

        // **Walk until it completes rather than predicting the turn.** A build banks only on the
        // turns its crew has something standing above its floor, and this harness's take crew draws
        // the herd down, so the *elapsed* turns run to several times the `turns_to_tame` the accrual
        // alone implies. What the test needs is the **transition**, which is found by walking to it;
        // the bound below is a bound, not a prediction.
        let mut turns_taken = 0;
        for _ in 0..turns_to_tame.saturating_mul(WALK_TURNS_PER_BUILD_TURN) {
            if world
                .resource::<HerdRegistry>()
                .find(HERD_ID)
                .unwrap()
                .is_domesticated()
            {
                break;
            }
            // **The verb is still held while the meter is short** — asserted every turn, so the
            // "an unfinished build keeps its verb" guarantee is checked across the whole build
            // rather than at one sampled point.
            assert_eq!(
                queued_job(&world, band, BuildSource::Herd(HERD_ID.to_string())),
                Some(BuildJob::Rung(Improvement::Tame)),
                "an unfinished build keeps its entry"
            );
            regrow_source_herd(&mut world);
            // **A WHOLE TURN, because this fixture staffs its keeping.** The band's `husbandry` row
            // banks `upkeep_supplied` every pass and the Logistics decay passes are what clear it,
            // so a bare labour pass in this loop would add a second turn's keeping on top of the
            // first against a bill stamped once — and the pass now says so rather than quietly
            // reporting the herd better kept than it is.
            advance_one_turn(&mut world);
            turns_taken += 1;
        }
        assert!(
            world
                .resource::<HerdRegistry>()
                .find(HERD_ID)
                .unwrap()
                .is_domesticated(),
            "fixture: the herd must finish being gentled"
        );
        assert!(
            turns_taken > 1,
            "fixture: the build must take real turns, or 'completion is a transition' is untested"
        );
        let completed = only_source_assignment(&world, band);
        assert_eq!(completed.workers, WORKERS, "the crew stays on the herd");
        assert_eq!(
            queued_job(&world, band, BuildSource::Herd(HERD_ID.to_string())),
            None,
            "completion retires the entry"
        );
        let LaborTarget::Hunt { fauna_id, floor } = &completed.target else {
            panic!("completion must not change the target's KIND: {completed:?}");
        };
        assert_eq!(
            *floor, BUILDER_FLOOR,
            "the player's stance is never rewritten (issue #442)"
        );
        assert_eq!(fauna_id, HERD_ID, "the same herd");
    }

    /// **Stand up a band taming the shipped herd on one named kit**, and hand back the taming
    /// progress and the handling gear's condition after one turn.
    ///
    /// A helper rather than two copies, because the whole claim of the tests below is that the
    /// **kit** is the only thing that differs: written twice, a stray difference in the floor, the
    /// crew or the herd's seating would be indistinguishable from the effect under test.
    fn tame_one_turn_on(kit_id: &str) -> (f32, f32) {
        let turn = tame_one_turn_on_herd_owned_by(kit_id, None);
        (turn.progress, turn.gear_wear)
    }

    // **RETIRED: `turns_to_tame_on`** — the end-to-end turns comparison the gear claim used to be
    // measured on.
    //
    // The two kits move the pace through **a second channel**: `big_game` carries spears, so its
    // hunter shrinks the flock, which moves the herd's escapement room and therefore which turns the
    // build is eligible on at all. A turn count measured the kits' attack tiers as much as their
    // handling gear. The claim is about the JOB, so it is measured on the job — see part (3) of
    // `the_handling_kit_takes_work_off_the_job_rather_than_speeding_the_crew`.

    /// **The animal web's builders kit** — the `crook` and nothing else. Named because the fixtures
    /// below put it on the **builders** row, where a build's gear offset is read from. It carried
    /// `hurdles` until the material half of the standing upkeep made those a **material** the
    /// `animal:pen` rung eats (`docs/plan_standing_upkeep.md` §4.9 item 12); every gear dial came
    /// across unchanged, so the fixtures below measure the same pacing they always did.
    const HURDLING_KIT: &str = "hurdling";

    /// The item that kit carries — what a build's wear is charged against on the animal web.
    const CROOK: &str = "crook";

    /// **The PLANT web's builders kit and its tool.** Named so an animal-build fixture can assert
    /// that neither is touched: a hoe brought to a `Tame` adds nothing to what its builders
    /// deliver, so it must be charged nothing.
    const TILLAGE_KIT: &str = "tillage";
    const HOES: &str = "hoes";

    /// What one turn of a `Tame` assignment left behind — the build meter, the gear it spent, and the
    /// two liveness handles that tell a *refused* build apart from a fixture that never ran at all:
    /// `tame_arm_ran` is the herd's `tamed_this_turn`, which the `Tame` arm sets on entry, and
    /// `still_queued` is whether the band's build queue still names the herd afterwards.
    struct TameTurn {
        progress: f32,
        // **RETIRED: `rate`** — the maintenance rate this herd owed on the turn measured. It was
        // carried out of the fixture so a caller could add it back before comparing two arms, the
        // rate riding each herd's own keeper load. Since §4.6a it is netted off nothing: a build
        // crew supplies none of it, so `progress` **is** the crew's output and the two arms are
        // comparable outright.
        gear_wear: f32,
        /// **The HOES' condition after the turn** — the *other* web's build tool, which an animal
        /// build must never charge. `wear` is a per-item ledger, so this is a different number from
        /// [`Self::gear_wear`] and not a re-reading of it.
        hoe_wear: f32,
        /// **What the crew's tools ADD to the pool's output this turn** —
        /// `Herd::build_work_from_gear`. `0` for a crew carrying nothing that helps.
        gear_work: f32,
        tame_arm_ran: bool,
        still_queued: bool,
    }

    /// [`tame_one_turn_on`] with the herd's **owner** as a second dial: `None` leaves it unowned (the
    /// ordinary case — the first accrual claims it), `Some(faction)` seats an owner before the turn
    /// so the ownership rule inside `Herd::accrue_domestication` can be exercised from the labor
    /// system. Parameterised rather than copied so the two arms differ in the owner and nothing else.
    fn tame_one_turn_on_herd_owned_by(kit_id: &str, owner: Option<FactionId>) -> TameTurn {
        const BIG_HERD_CAP: f32 = 1_000.0;
        let (mut world, tile) = world_with_source(CAP);
        reseat_herd(&mut world, BIG_HERD_CAP, BIG_HERD_CAP);
        grant_knowledge(&mut world, HERDING_DISCOVERY_ID);
        if let Some(owner) = owner {
            world.resource_mut::<HerdRegistry>().herds[0].owner = Some(owner);
        }
        let equipment = crate::equipment_config::EquipmentConfig::builtin();
        let builders = animal_builders(&world, RungKey::AnimalPastoral);
        let kit = equipment
            .kit(kit_id)
            .unwrap_or_else(|| panic!("the shipped roster carries the '{kit_id}' kit"));
        let band = spawn_band(
            &mut world,
            tile,
            vec![
                LaborAssignment {
                    target: LaborTarget::Hunt {
                        fauna_id: HERD_ID.to_string(),
                        floor: BUILDER_FLOOR,
                    },
                    workers: WORKERS,
                    kit: Some(kit.clone()),
                    priority: SourcePriority::default(),
                    upkeep_kit: None,
                },
                // **THE `builders` ROW CARRIES NO KIT** — one is refused there since §4.7a ②,
                // because a build's gear is a property of the queue ENTRY and not of the band.
                LaborAssignment {
                    target: LaborTarget::Builders,
                    workers: builders,
                    kit: None,
                    priority: SourcePriority::default(),
                    upkeep_kit: None,
                },
            ],
        );
        {
            let mut allocation = world
                .get_mut::<LaborAllocation>(band)
                .expect("the fixture band has an allocation");
            assert!(allocation.enqueue_build(
                BuildSource::Herd(HERD_ID.to_string()),
                BuildJob::Rung(Improvement::Tame),
            ));
            // **THE KIT UNDER TEST RIDES THE QUEUE ENTRY**, which is where a build's gear offset is
            // read from: naming it on the row is the per-BAND answer §4.7a deleted, and asserting
            // the offset off that row now would be a guard over a dead term.
            assert!(
                allocation.set_build_entry_kit(&BuildSource::Herd(HERD_ID.to_string()), Some(kit),)
            );
        }
        // `spawn_band` builds no ledger, and wear is only charged on an item the band owns — an
        // absent entry is NOT OWNED since the count slice.
        world
            .entity_mut(band)
            .insert(crate::components::BandEquipment::start_stocked(&equipment));

        regrow_source_herd(&mut world);
        world.run_system_once(advance_labor_allocation);

        let progress = world
            .resource::<HerdRegistry>()
            .find(HERD_ID)
            .expect("the fixture herd survives the turn")
            .ladder_position();
        let gear_wear = world
            .get::<crate::components::BandEquipment>(band)
            .expect("the band's ledger survives the turn")
            .wear_of(CROOK);
        let hoe_wear = world
            .get::<crate::components::BandEquipment>(band)
            .expect("the band's ledger survives the turn")
            .wear_of(HOES);
        let tame_arm_ran = world
            .resource::<HerdRegistry>()
            .find(HERD_ID)
            .expect("the fixture herd survives the turn")
            .tamed_this_turn;
        let gear_work = world
            .resource::<HerdRegistry>()
            .find(HERD_ID)
            .expect("the fixture herd survives the turn")
            .build_work_from_gear;
        let still_queued = world
            .get::<LaborAllocation>(band)
            .expect("the band's allocation survives the turn")
            .build_queue_position(&BuildSource::Herd(HERD_ID.to_string()))
            .is_some();
        TameTurn {
            progress,
            gear_wear,
            hoe_wear,
            gear_work,
            tame_arm_ran,
            still_queued,
        }
    }

    /// **THE HANDLING GEAR SHORTENS THE CLIMB, and the kit is the only thing that decides it**
    /// (issue #515). Hurdles, halters and a butchering stone are animal-handling tools, and `Tame`
    /// is exactly the turns a band spends handling animals — so a crew that brought them gentles a
    /// herd sooner than one that left them at camp.
    ///
    /// **THE TOOL RAISES THE CREW; THE JOB'S WORK REQUIREMENT NEVER CHANGES**
    /// (`docs/plan_standing_upkeep.md` §4.8), so the two arms are compared on their **per-turn
    /// accrual** and the rung's own `work_cost` is asserted to be the one bar both are struck
    /// from. That is the inversion of what this test asserted while gear was subtracted from the
    /// job.
    ///
    /// **Asserted as the contribution the config declares, not as two literals**, so retuning the
    /// hurdles' own `build_work` moves the test with the game; and the bare arm's liveness is asserted too, or
    /// *"the geared arm banked more"* would also pass for a build that only runs with gear.
    #[test]
    fn the_handling_kit_speeds_the_crew_rather_than_shrinking_the_job() {
        let equipment = crate::equipment_config::EquipmentConfig::builtin();
        let declared = equipment.build_work_per_worker(
            &equipment
                .kit(HURDLING_KIT)
                .expect("the shipped roster carries the hurdling kit"),
            &crate::components::BandEquipment::start_stocked(&equipment),
            crate::intensification::RungBranch::Animal,
            None,
        );
        assert!(
            declared > crate::intensification::NO_BUILD_GEAR,
            "fixture: the hurdling kit must declare a build contribution above neutral, got \
             {declared}"
        );

        let geared = tame_one_turn_on_herd_owned_by(HURDLING_KIT, None);
        let bare = tame_one_turn_on_herd_owned_by("big_game", None);

        // (1) The ACCRUAL is what the gear moves — the geared pool banks strictly more per turn.
        //
        // **Compared OUTRIGHT**, with nothing added back. It used to need the maintenance rate
        // restored to each arm before the two were comparable, because the rate was netted out of
        // the accrual and rides the herd's own keeper load — and the two kits carry home different
        // amounts, so the two arms' herds owe different rates. §4.6a took the rate out of the
        // accrual entirely, so what is left is the crew's output and nothing else.
        assert!(
            bare.progress > 0.0,
            "fixture: the un-geared crew must actually be taming, or the comparison is two zeroes"
        );
        assert!(
            geared.progress > bare.progress,
            "the gear must raise the crew's own output: geared={} bare={}",
            geared.progress,
            bare.progress
        );
        // **And by exactly what the kit delivers over the pool** — the crew is the same size in
        // both arms, so the whole difference is the hurdles.
        assert!(
            (geared.progress - bare.progress - geared.gear_work).abs() < 1e-4,
            "the difference must be the kit's own delivery: {} - {} against {}",
            geared.progress,
            bare.progress,
            geared.gear_work
        );

        // (2) What it moves is the CREW'S OUTPUT — and **the partly-equipped-party rule decides how
        // much**. `start_stocked` gives the band exactly ONE set of hurdles, so one of the ten
        // keepers is equipped and nine are bare: the pool delivers **one worker's worth** of gear
        // work, not ten. That is the whole reason the per-worker form can be coverage-weighted at
        // all — it is a SUM, so the nine bare hands add zero, where averaging the retired multiplier
        // over them diluted it to ×1.05 and made every extra keeper *slow the build down*.
        const HURDLE_SETS_A_SPAWN_STOCKS: f32 = 1.0;
        assert!(
            (geared.gear_work - HURDLE_SETS_A_SPAWN_STOCKS * declared).abs() < 1e-4,
            "one set of hurdles among {WORKERS} keepers delivers one worker's worth: {} \
             against {declared}",
            geared.gear_work
        );
        assert_eq!(
            bare.gear_work,
            crate::intensification::NO_BUILD_GEAR,
            "a crew carrying nothing that helps delivers nothing extra"
        );

        // (3) **THE JOB IS THE SAME SIZE IN BOTH ARMS** — a `Tame` costs this species' whole
        // `work_cost x taming_cost_multiplier` with handling gear and without (SS4.8). The bar is
        // the rung's own, so there is nothing per-arm to compare: it is asserted as the *one*
        // number both arms' countdowns are struck from.
        //
        // # WHY THE TURN COUNT IS NOT THE MEASURE HERE
        //
        // **The two kits move the pace through a second channel**: `big_game` carries spears and its
        // lone hunter kills more, which shrinks the flock and moves the escapement room the build's
        // own gate reads. An end-to-end turn comparison therefore measures the two kits' ATTACK tiers
        // as much as their handling gear. The finishes-sooner half is (1) above — a strictly larger
        // per-turn accrual against an identical bar — and the end-to-end pair is
        // `intensification::tests::gear_shortens_the_build_and_never_the_job`.
        let ladder = LadderConfig::builtin();
        let pastoral = ladder.rung(RungKey::AnimalPastoral);
        let cost = pastoral
            .build_cost(RUNG_COST_UNSCALED)
            .expect("the pastoral rung builds");
        assert!(
            cost > geared.gear_work,
            "the job is the rung's own price, not a bar the pool's kit shrank: {cost} against a \
             kit delivering {}",
            geared.gear_work
        );

        // **The other half of "the kit decides it"**: a crew that never carried the gear onto the
        // range spends none of it, which is what keeps the two arms a fair comparison.
        assert_eq!(
            bare.gear_wear, 0.0,
            "a crew on a kit without handling gear must not wear handling gear"
        );
    }

    /// **THE BUILD PAYS FOR THE GEAR IT USED, and a stalled one pays nothing.**
    ///
    /// The charge is `WearQuantum::BuildProgress`, over the progress accrued — so it is a per-USE
    /// cost and not a turn clock (`docs/plan_denial_raid.md` §1.2). The two arms are the whole
    /// claim: a crew that builds spends gear, and a crew holding the same kit on the same herd with
    /// **no build verb** spends none of it over the same turn.
    #[test]
    fn a_build_wears_the_handling_gear_and_a_turn_without_one_does_not() {
        let (progress, charged) = tame_one_turn_on(HURDLING_KIT);
        assert!(
            progress > 0.0,
            "fixture: the crew must actually have built something to be charged for"
        );
        let rate = crate::equipment_config::EquipmentConfig::builtin()
            .item(CROOK)
            .expect("the shipped roster carries the crook")
            .wear_for(crate::equipment_config::WearQuantum::BuildProgress)
            .expect("the handling gear wears on build progress")
            .amount;
        // **Divided by its own rate**, so retuning the amount cannot move the claim: what is under
        // test is that the charge was taken over the PROGRESS, not over a turn or a take.
        assert!(
            (charged / rate - progress).abs() < 1e-4,
            "the gear must be charged over the progress it bought: \
             charged={charged} rate={rate} progress={progress}"
        );

        // The same kit, the same herd, the same turn — with nothing being built.
        const BIG_HERD_CAP: f32 = 1_000.0;
        let (mut world, tile) = world_with_source(CAP);
        reseat_herd(&mut world, BIG_HERD_CAP, BIG_HERD_CAP);
        grant_knowledge(&mut world, HERDING_DISCOVERY_ID);
        let equipment = crate::equipment_config::EquipmentConfig::builtin();
        let idler = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: BUILDER_FLOOR,
                },
                workers: WORKERS,
                kit: Some(
                    equipment
                        .kit(HURDLING_KIT)
                        .expect("the shipped roster carries the hurdling kit"),
                ),
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        world
            .entity_mut(idler)
            .insert(crate::components::BandEquipment::start_stocked(&equipment));
        regrow_source_herd(&mut world);
        world.run_system_once(advance_labor_allocation);

        let ledger = world
            .get::<crate::components::BandEquipment>(idler)
            .expect("the idler's ledger survives the turn")
            .clone();
        // **It is the BUILD charge that must be absent, not every charge** — this crew is hunting a
        // wild herd, so its sled is being dragged and that is real work. The handling gear is not
        // charged at a wild hunt on any quantum, which is what makes a bare `wear_of` honest here.
        assert_eq!(
            ledger.wear_of(CROOK),
            0.0,
            "a crew holding the gear with no build in flight must spend none of it"
        );
    }

    /// ⛔ **THE OTHER WEB'S TOOL IS NEITHER CREDITED NOR CHARGED** — `wear` follows the work
    /// actually done, and a hoe does none of a `Tame`.
    ///
    /// The two halves are one rule seen from both ends. `EquipmentEffect::branch` zeroes the hoes'
    /// contribution to an animal build, so charging them would run a tool down against a job it did
    /// not move — which is exactly the phantom charge
    /// [`a_build_the_source_refuses_spends_no_gear`] closes one axis over. The **liveness** arm is
    /// the hurdling pool beside it: without it, *"the hoes spent nothing"* would also be what a
    /// fixture that never reached the build seam reported.
    #[test]
    fn a_pool_carrying_the_other_webs_tool_neither_speeds_nor_spends_it() {
        let hoed = tame_one_turn_on_herd_owned_by(TILLAGE_KIT, None);
        assert!(
            hoed.progress > 0.0,
            "fixture: the crew must actually be taming, or both assertions are vacuous"
        );
        assert_eq!(
            hoed.gear_work,
            crate::intensification::NO_BUILD_GEAR,
            "a hoe takes nothing off a Tame — the branch qualifier's whole job"
        );
        assert_eq!(
            hoed.hoe_wear, 0.0,
            "…and so it is charged nothing: wear follows the work actually done"
        );

        // **Liveness — the animal web's own kit on the same fixture does both.**
        let hurdled = tame_one_turn_on_herd_owned_by(HURDLING_KIT, None);
        assert!(
            hurdled.gear_work > crate::intensification::NO_BUILD_GEAR && hurdled.gear_wear > 0.0,
            "fixture: hurdles on the same Tame must take work off it AND be spent for it, or the \
             two zeroes above prove nothing (took {} spent {})",
            hurdled.gear_work,
            hurdled.gear_wear
        );
        assert_eq!(
            hurdled.hoe_wear, 0.0,
            "and the hurdling pool holds no hoes at all, so nothing charges them either"
        );
    }

    /// **A BUILD THE HERD REFUSES SPENDS NOTHING** — the phantom-charge defect.
    ///
    /// Ownership is deliberately absent from the `Tame` arm's `eligible`: `accrue_domestication`
    /// owns the `owner is None || owner == faction` rule. So a band whose `Tame` outlived another
    /// faction claiming the herd passes every gate the arm checks and computes a positive accrual
    /// each turn, while the herd banks none of it — and because `hunt_rung_already_built(Tame)` is
    /// `is_domesticated()`, its verb is never cleared either. Charged for the accrual it *offered*,
    /// that band bled its handling gear dry against a meter that never moved.
    ///
    /// The two arms are the whole claim, and the second is not optional: *"it spent nothing"* is
    /// also what a fixture that never reached the build seam would report. **The owner is the only
    /// value that differs between them**, and nothing the accrual is computed from — the rung, the
    /// knowledge, the husbandry ceiling, the work predicate, the floor, the crew, the kit's build
    /// rate — reads it, so the offer the owned arm banks is exactly the offer the refused arm was
    /// made and declined.
    #[test]
    fn a_build_the_source_refuses_spends_no_gear() {
        /// The rival that claims the herd out from under the taming band — anyone but
        /// [`BAND_FACTION`], whose claim the arm would honour.
        const RIVAL_FACTION: FactionId = FactionId(7);

        let refused = tame_one_turn_on_herd_owned_by(HURDLING_KIT, Some(RIVAL_FACTION));
        assert_eq!(
            refused.progress, 0.0,
            "a herd another faction owns banks none of the offered accrual"
        );
        assert_eq!(
            refused.gear_wear, 0.0,
            "and so the crew spends none of its handling gear: charged for the meter's delta, \
             a refused build is free"
        );
        // **The fixture really did reach the seam** — the arm sets `tamed_this_turn` on entry — and
        // its verb is still standing afterwards, which is exactly why the old offered-amount charge
        // bled every turn forever rather than stopping at a completion.
        assert!(
            refused.tame_arm_ran,
            "fixture: the Tame arm must actually have run, or 'spent nothing' is vacuous"
        );
        assert!(
            refused.still_queued,
            "fixture: the stalled entry is never retired, so the bleed had no end"
        );

        // **Liveness — the same fixture on the band's OWN herd both accrues and spends.**
        let owned = tame_one_turn_on_herd_owned_by(HURDLING_KIT, Some(BAND_FACTION));
        assert!(
            owned.progress > 0.0,
            "fixture: the same crew on its own herd must actually tame it"
        );
        assert!(
            owned.gear_wear > 0.0,
            "fixture: and must actually spend the gear, or 'spent nothing' proves nothing"
        );
    }

    // ==========================================================================================
    // THE MATERIAL HALF OF BUILD AND UPKEEP (`docs/plan_standing_upkeep.md` §2.7 / §4.9 item 12)
    // ==========================================================================================

    /// **A store holding exactly `units` of the pen's material, and nothing else** — the fixture that
    /// makes the build's coverage a measurable *fraction* rather than an all-or-nothing.
    fn stock_pen_materials_units(world: &mut World, band: Entity, units: f32) {
        let materials = crate::materials_config::MaterialsConfig::builtin();
        let recipes = crate::recipes_config::RecipesConfig::builtin();
        let characteristics = recipes
            .recipes()
            .find_map(|(_, recipe)| {
                recipe
                    .outputs
                    .iter()
                    .find(|output| output.material_id() == Some(PEN_MATERIAL))
                    .map(|output| output.characteristics.clone())
            })
            .expect("the shipped book makes the pen's material");
        let key = materials
            .band_key(PEN_MATERIAL, &characteristics)
            .expect("the shipped roster rates the pen's material");
        let mut cohort = world
            .get_mut::<PopulationCohort>(band)
            .expect("the fixture band exists");
        let held = cohort.stores.material_total(PEN_MATERIAL);
        cohort.stores.take_material_batches(PEN_MATERIAL, held);
        cohort
            .stores
            .deposit_material(PEN_MATERIAL, key, scalar_from_f32(units), &characteristics);
    }

    /// **A TAMED HERD WITH A `Corral` QUEUED AND BUILDERS ON IT** — the one shipped rung that
    /// declares a material on either term, so every test below drives it.
    fn world_with_a_pen_build() -> (World, Entity) {
        const BIG_HERD_CAP: f32 = 1_000.0;
        let (mut world, tile) = world_with_source(CAP);
        reseat_herd(&mut world, BIG_HERD_CAP, BIG_HERD_CAP);
        grant_knowledge(&mut world, PENNING_DISCOVERY_ID);
        {
            let mut registry = world.resource_mut::<HerdRegistry>();
            registry.herds[0].tame_outright(
                BAND_FACTION,
                &crate::intensification::LadderConfig::builtin(),
            );
        }
        let builders = animal_builders(&world, RungKey::AnimalPen);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: BUILDER_FLOOR,
                },
                workers: WORKERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        declare_herd_build(&mut world, band, HERD_ID, Improvement::Corral, builders);
        (world, band)
    }

    /// The work banked on the pen rung — the meter the pile is drawn against.
    fn pen_position(world: &World) -> f32 {
        world
            .resource::<HerdRegistry>()
            .find(HERD_ID)
            .expect("the fixture herd")
            .rung_work_done(
                RungKey::AnimalPen,
                &crate::intensification::LadderConfig::builtin(),
            )
    }

    fn held_material(world: &World, band: Entity) -> f32 {
        world
            .get::<PopulationCohort>(band)
            .expect("the fixture band")
            .stores
            .material_total(PEN_MATERIAL)
            .to_f32()
    }

    /// The pen rung's declared pile and its own width — read off the ladder so a retune moves the
    /// fixture with the game.
    fn pen_pile_and_width() -> (f32, f32) {
        let ladder = crate::intensification::LadderConfig::builtin();
        let pile = ladder
            .rung(RungKey::AnimalPen)
            .build_materials()
            .find(|(id, _)| *id == PEN_MATERIAL)
            .map(|(_, amount)| amount)
            .expect("the shipped `animal:pen` rung declares a material pile");
        let width = ladder
            .rung(RungKey::AnimalPen)
            .build_cost(RUNG_COST_UNSCALED)
            .expect("a rung a verb builds has a build meter");
        (pile, width)
    }

    /// ⛔ **THE PILE IS DRAWN AS THE METER CLIMBS, NOT ON COMPLETION**
    /// (`docs/plan_standing_upkeep.md` §2.7).
    ///
    /// The claim is a **proportion**, so it is asserted as one: after one turn the store has lost the
    /// same share of the pile the meter gained of the rung, against the rung's *own* declared numbers
    /// rather than remembered ones. **The liveness half is that the rung is only PART raised** —
    /// on a finished rung *"drawn in proportion"* and *"drawn on completion"* are the same number.
    #[test]
    fn the_build_pile_is_drawn_in_proportion_to_the_work_banked() {
        let (mut world, band) = world_with_a_pen_build();
        stock_pen_materials(&mut world, band);
        let (pile, width) = pen_pile_and_width();

        let before = held_material(&world, band);
        world.run_system_once(advance_labor_allocation);
        let banked = pen_position(&world);
        let drawn = before - held_material(&world, band);

        assert!(
            banked > 0.0 && banked < width,
            "fixture: the pen must be PART raised, or 'in proportion' and 'on completion' cannot be \
             told apart (banked {banked} of {width})"
        );
        assert!(
            (drawn - pile * banked / width).abs() < FORECAST_EPSILON,
            "the pile is drawn at the share of the rung this turn banked: drew {drawn} against \
             {pile} × {banked}/{width}"
        );
        assert!(
            drawn < pile,
            "**LIVENESS**: drawing the WHOLE pile on a part-raised rung is the on-completion reading \
             this test exists to exclude (drew {drawn} of {pile})"
        );
    }

    /// ⛔ **A SHORT STORE STALLS THE BUILD PROPORTIONALLY — IT NEVER REFUSES IT** (§2.5's stated rule
    /// for an indivisible supplier).
    ///
    /// A store holding a **quarter** of what the turn's pile wants banks a quarter of the turn's work
    /// and spends the quarter; the rest of the crew's output is **wasted**. Both halves are asserted:
    /// the build did move (so this is a stall, not a refusal), and it moved by exactly the coverage.
    #[test]
    fn a_short_store_stalls_the_build_proportionally_rather_than_refusing_it() {
        /// The share of the turn's pile the short fixture's store can pay for.
        const COVERAGE: f32 = 0.25;

        let (mut full, band) = world_with_a_pen_build();
        stock_pen_materials(&mut full, band);
        let stocked = held_material(&full, band);
        full.run_system_once(advance_labor_allocation);
        let fully_banked = pen_position(&full);
        let wanted = stocked - held_material(&full, band);

        let (mut short, band) = world_with_a_pen_build();
        stock_pen_materials_units(&mut short, band, wanted * COVERAGE);
        short.run_system_once(advance_labor_allocation);
        let short_banked = pen_position(&short);

        assert!(
            fully_banked > 0.0 && wanted > 0.0,
            "fixture: the fully-stocked arm must build and draw, or every ratio below is 0/0 \
             (banked {fully_banked}, drew {wanted})"
        );
        assert!(
            short_banked > 0.0,
            "**A SHORT STORE STALLS, IT DOES NOT REFUSE** — the build must still bank something \
             (banked {short_banked})"
        );
        assert!(
            (short_banked - fully_banked * COVERAGE).abs() < FORECAST_EPSILON,
            "…and it banks exactly the coverage: {short_banked} against {fully_banked} × {COVERAGE}"
        );
        assert!(
            held_material(&short, band) < FORECAST_EPSILON,
            "the short store is spent to the last unit — the coverage IS what the store could pay"
        );
    }

    /// **A TAMED HERD WITH A PEN QUEUED, AND A `sow` QUEUED BEHIND IT** — the two-entry queue the
    /// waiting entry's coverage rule needs: the head is the shipped ladder's **only** material
    /// declarer and the entry behind it declares none at all.
    /// A species in [`SOURCE_BIOME`]'s realized basket whose `cultivation_ceiling` reaches
    /// `plant:field` — what a `sow` on this fixture's ground can actually commit to.
    const SOWABLE_SPECIES: &str = "wild_emmer";

    fn world_with_a_sow_queued_behind_a_pen() -> (World, Entity) {
        let (mut world, band) = world_with_a_pen_build();
        grant_knowledge(&mut world, SEED_SELECTION_DISCOVERY_ID);
        // **`plant:field` DECLARES `requires_fresh_water`**, so the harness's dry steppe refuses the
        // rung on its site term and the gate never reaches the coverage test. Tagged on the source
        // tile itself, which is `tile_is_fresh_watered`'s first arm.
        {
            let tile_entity = world
                .resource::<TileRegistry>()
                .index(SOURCE.x, SOURCE.y)
                .expect("the fixture source tile");
            world
                .get_mut::<Tile>(tile_entity)
                .expect("the fixture source tile")
                .terrain_tags |= sim_runtime::TerrainTags::FRESHWATER;
        }
        let source = BuildSource::Patch(SOURCE);
        {
            let mut allocation = world
                .get_mut::<LaborAllocation>(band)
                .expect("the fixture band has an allocation");
            allocation.assignments.push(LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    floor: BUILDER_FLOOR,
                    // **A NAMED CROP, because a Field commits to one.** The tile's realized basket
                    // leads with `seed_grasses`, whose `cultivation_ceiling` stops at *tended*, so a
                    // `sow` that named nothing would be refused `BuildGate::NoCrop` — and a refused
                    // gate never reaches the coverage test this fixture exists to drive.
                    species: Some(SOWABLE_SPECIES.to_string()),
                    take_species: TakeSelection::EVERYTHING,
                },
                workers: WORKERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            });
            assert!(
                allocation.enqueue_build(source.clone(), BuildJob::Rung(Improvement::Sow)),
                "fixture: the sow is queued on a source the band works"
            );
            assert_eq!(
                allocation.build_queue_position(&source),
                Some(1),
                "fixture: the sow must sit BEHIND the pen — a head is funded and a waiting entry is \
                 only dated, which is the whole distinction under test"
            );
            assert!(allocation.set_build_entry_kit(&source, Some(bare_builders())));
        }
        (world, band)
    }

    /// The countdown the patch entry publishes.
    fn published_sow_countdown(world: &World) -> Option<crate::intensification::BuildTurns> {
        world
            .resource::<ForageRegistry>()
            .patch(SOURCE)
            .expect("the fixture patch")
            .build_turns_remaining
    }

    /// The cause that countdown carries.
    fn published_sow_block_reason(world: &World) -> crate::intensification::BuildGate {
        world
            .resource::<ForageRegistry>()
            .patch(SOURCE)
            .expect("the fixture patch")
            .build_blocked_reason
    }

    /// The waiting entry's **own** span — what the chain added on top of the head's date. Both
    /// numbers come off the wire, so this is what a client renders as *"and then N more"*.
    fn sow_span_behind_the_pen(world: &World) -> u32 {
        let head = match published_pen_countdown(world) {
            Some(crate::intensification::BuildTurns::Turns(turns)) => turns,
            other => {
                panic!("fixture: the head must name a date for the tail to be measured: {other:?}")
            }
        };
        let tail = match published_sow_countdown(world) {
            Some(crate::intensification::BuildTurns::Turns(turns)) => turns,
            other => panic!("fixture: the waiting sow must name a date: {other:?}"),
        };
        tail - head
    }

    /// ⛔ **A WAITING ENTRY IS QUOTED AT ITS OWN COVERAGE, NEVER THE BAND'S**
    /// (`docs/plan_standing_upkeep.md` §2.7).
    ///
    /// The settlement strikes its material want against the **head** alone, so `build_coverage`
    /// describes the head's store and nothing else. Handing it to an entry *behind* the head made
    /// another source's shelf into this one's problem: the `sow` — a rung that declares no material
    /// at all — was quoted at the pen's coverage, so a **half-stocked** pen doubled the sow's own
    /// span and `publish_build_chain` carried the inflated date down the rest of the queue.
    ///
    /// The stretch is measured against a **fully-stocked** control run of the same fixture, so the
    /// claim is *"the head's shelf does not move the tail"* rather than a remembered turn count.
    #[test]
    fn a_head_short_of_material_neither_blocks_nor_stretches_the_entry_behind_it() {
        /// The share of the head's pile its store can pay for on the stretched arm. A half doubles
        /// the head's own span, which no rounding can turn back into the covered one.
        const COVERAGE: f32 = 0.5;

        // --- the control: the head wants for nothing ---------------------------------------------
        let (mut full, band) = world_with_a_sow_queued_behind_a_pen();
        stock_pen_materials(&mut full, band);
        let stocked = held_material(&full, band);
        full.run_system_once(advance_labor_allocation);
        let wanted = stocked - held_material(&full, band);
        assert!(
            wanted > 0.0,
            "fixture: the head must actually draw material, or there is no shortage to inflict \
             (drew {wanted})"
        );
        assert_eq!(
            published_sow_block_reason(&full),
            crate::intensification::BuildGate::Open,
            "fixture: the queued sow's own gate must HOLD, or `turns()` never reaches the coverage \
             test and this whole fixture is vacuous"
        );
        let covered_head = match published_pen_countdown(&full) {
            Some(crate::intensification::BuildTurns::Turns(turns)) => turns,
            other => panic!("fixture: a fully-stocked pen build names a date, got {other:?}"),
        };
        let covered_span = sow_span_behind_the_pen(&full);
        assert!(
            covered_span > 1,
            "fixture: the waiting sow must span more than a turn, or a doubling is unmeasurable \
             (span {covered_span})"
        );

        // --- the head's shelf is EMPTY: the tail inherits the head's block, and only that --------
        // **This arm cannot see the defect and is here to say so.** A blocked head sets
        // `publish_build_chain`'s `carried`, and every entry below it publishes that answer without
        // its own quote ever being consulted — which is right: an entry behind a build that cannot
        // start cannot be dated either. So the leak is only visible where the head is *stalled*
        // rather than *stopped*, which is the third arm.
        let (mut dry, band) = world_with_a_sow_queued_behind_a_pen();
        stock_pen_materials_units(&mut dry, band, 0.0);
        dry.run_system_once(advance_labor_allocation);
        assert_eq!(
            published_pen_countdown(&dry),
            Some(crate::intensification::BuildTurns::Blocked),
            "a pen build that cannot draw one hurdle publishes the blocked sentinel"
        );
        assert_eq!(
            published_sow_countdown(&dry),
            Some(crate::intensification::BuildTurns::Blocked),
            "…and the queue behind it carries that, because a date behind an unstartable build \
             would be a promise"
        );

        // --- the head's shelf is HALF: the tail's own span is untouched ---------------------------
        let (mut partial, band) = world_with_a_sow_queued_behind_a_pen();
        stock_pen_materials_units(&mut partial, band, wanted * COVERAGE);
        partial.run_system_once(advance_labor_allocation);
        let stretched_head = match published_pen_countdown(&partial) {
            Some(crate::intensification::BuildTurns::Turns(turns)) => turns,
            other => panic!("a PARTLY covered build still finishes, so it names a date: {other:?}"),
        };
        assert!(
            stretched_head > covered_head,
            "fixture: the head must actually be stalled by its short store ({stretched_head} \
             against the covered {covered_head}), or there is no stretch to leak"
        );
        assert_eq!(
            sow_span_behind_the_pen(&partial),
            covered_span,
            "**THE WAITING ENTRY'S OWN SPAN IS UNTOUCHED BY THE HEAD'S SHORTAGE** — it is dated at \
             the full pool and at FULL coverage, because it has bid on nothing yet"
        );
    }

    /// **A STORE WITH NOTHING IN IT BANKS NOTHING, AND THE ENTRY STAYS QUEUED.** The other end of the
    /// stall: there is no affordability gate anywhere (§2.5 retired the five verbs' own), so the
    /// build **queues** rather than being refused — which is what a build whose builders walked away
    /// already does.
    #[test]
    fn a_build_with_no_material_at_all_stalls_and_stays_in_the_queue() {
        let (mut world, band) = world_with_a_pen_build();
        stock_pen_materials_units(&mut world, band, 0.0);
        world.run_system_once(advance_labor_allocation);

        assert_eq!(
            pen_position(&world),
            0.0,
            "with no hurdles at all the coverage is zero, so the crew's whole output is wasted"
        );
        assert!(
            world
                .get::<LaborAllocation>(band)
                .expect("the fixture band")
                .build_queue_position(&BuildSource::Herd(HERD_ID.to_string()))
                .is_some(),
            "**AND IT IS A STALL, NOT A REFUSAL** — the entry stays in the queue, exactly as a build \
             whose builders left does"
        );
    }

    /// ⛔ **DECAY REFUNDS NOTHING.** Material goes in as the meter climbs and does not come back when
    /// it falls — which is what makes neglect **self-limiting** rather than a store bleeding for
    /// ever: the position falls, the rate falls with it, and an abandoned thing decays toward costing
    /// nothing.
    #[test]
    fn decay_refunds_no_material() {
        let (mut world, band) = world_with_a_pen_build();
        stock_pen_materials(&mut world, band);
        let stocked = held_material(&world, band);
        world.run_system_once(advance_labor_allocation);
        let after_build = held_material(&world, band);
        let raised = pen_position(&world);
        assert!(
            raised > 0.0 && after_build < stocked,
            "fixture: the build must have banked work AND spent material, or there is nothing for a \
             refund to give back (banked {raised}, held {after_build} of {stocked})"
        );

        {
            let ladder = crate::intensification::LadderConfig::builtin();
            let mut registry = world.resource_mut::<HerdRegistry>();
            // **The one mutator of a position** (`Herd::set_ladder_position`), taken all the way back
            // to where the rung started — the animal web sheds animals rather than bleeding a meter,
            // so this is how a position falls at all.
            let base = registry.herds[0].ladder_position() - raised;
            registry.herds[0].set_ladder_position(base, &ladder);
        }
        assert_eq!(
            pen_position(&world),
            0.0,
            "fixture: the decay must actually take the rung back, or the claim is vacuous"
        );
        assert_eq!(
            held_material(&world, band),
            after_build,
            "**THE STORE IS UNTOUCHED** — the road washes away and the stone is spent"
        );
    }

    /// **THE UPKEEP RATE READS THE SAME `scaled_by` THE WORK TERM READS** — one rule, two currencies
    /// (§2.7). A pen holding twice the herd mends twice the fence.
    #[test]
    fn the_upkeep_material_rate_scales_with_the_source_load() {
        let ladder = crate::intensification::LadderConfig::builtin();
        let pen = ladder.rung(RungKey::AnimalPen);
        let rate = pen
            .upkeep_materials()
            .find(|(id, _)| *id == PEN_MATERIAL)
            .map(|(_, rate)| rate)
            .expect("the shipped `animal:pen` rung declares a material rate");

        assert!(
            (pen.upkeep_material_demand(PEN_MATERIAL, 1.0) - rate).abs() < FORECAST_EPSILON,
            "one load's worth is the declared rate itself"
        );
        assert!(
            (pen.upkeep_material_demand(PEN_MATERIAL, 2.0)
                - 2.0 * pen.upkeep_material_demand(PEN_MATERIAL, 1.0))
            .abs()
                < FORECAST_EPSILON,
            "…and it is linear in the source's own load, exactly as the work rate is"
        );
        assert_eq!(
            pen.upkeep_material_demand("a_material_no_rung_names", 1.0),
            crate::intensification::NO_UPKEEP_DEMAND,
            "a material the rung does not name is owed NOTHING — never a defaulted rate"
        );
    }

    /// **THE RATE INTERPOLATES ON THE POSITION** — the second axis §2.7 gives the material term, and
    /// on the shipped ladder `animal:pen`'s `partial_credit: on_completion` is what shapes it: half a
    /// fence is no fence, so a part-raised pen owes the rung **below**'s rate (which names no
    /// material) and the whole of it only once the fence closes.
    #[test]
    fn a_part_raised_pen_owes_no_hurdles_and_a_closed_one_owes_them_all() {
        let ladder = crate::intensification::LadderConfig::builtin();
        let fauna = crate::fauna_config::FaunaConfig::builtin();
        let (_, width) = pen_pile_and_width();

        let (mut world, band) = world_with_a_pen_build();
        stock_pen_materials(&mut world, band);
        world.run_system_once(advance_labor_allocation);
        let part_raised = {
            let herd = world
                .resource::<HerdRegistry>()
                .find(HERD_ID)
                .expect("the fixture herd")
                .clone();
            assert!(
                herd.rung_work_done(RungKey::AnimalPen, &ladder) > 0.0
                    && herd.rung_work_done(RungKey::AnimalPen, &ladder) < width,
                "fixture: the fence must be PART up, or the two readings coincide"
            );
            fauna::herd_upkeep_material_demand(&herd, &fauna, &ladder, PEN_MATERIAL)
        };
        assert_eq!(
            part_raised,
            crate::intensification::NO_UPKEEP_DEMAND,
            "**HALF A FENCE IS NO FENCE** — an `on_completion` rung is worth the rung below it, and \
             `animal:pastoral` names no material"
        );

        // …and the same herd with the fence closed owes the whole rate. Asserted against the rung's
        // own arithmetic at this herd's own load, so a retune moves both sides together.
        {
            let anchor = world.resource::<HerdRegistry>().herds[0].current_pos;
            let mut registry = world.resource_mut::<HerdRegistry>();
            assert!(
                registry.herds[0].corral_at(anchor, &ladder),
                "fixture: the species must be pennable"
            );
        }
        let herd = world
            .resource::<HerdRegistry>()
            .find(HERD_ID)
            .expect("the fixture herd")
            .clone();
        let closed = fauna::herd_upkeep_material_demand(&herd, &fauna, &ladder, PEN_MATERIAL);
        assert!(
            (closed
                - ladder
                    .rung(RungKey::AnimalPen)
                    .upkeep_material_demand(PEN_MATERIAL, fauna::herd_keeper_load(&herd, &fauna)))
            .abs()
                < FORECAST_EPSILON,
            "a closed fence owes the pen rung's whole rate at its own keeper load: {closed}"
        );
        assert!(
            closed > part_raised,
            "**LIVENESS**: the two readings must differ, or the interpolation is not being read"
        );
    }

    /// **THE COUNTDOWN THIS HERD'S PEN ENTRY PUBLISHES**, off the band's own chain pass — the
    /// number a client renders, not the quote it was struck from.
    fn published_pen_countdown(world: &World) -> Option<crate::intensification::BuildTurns> {
        world
            .resource::<HerdRegistry>()
            .find(HERD_ID)
            .expect("the fixture herd")
            .build_turns_remaining
    }

    /// **THE CAUSE THAT COUNTDOWN CARRIES** — `""` for anything that is not blocked.
    fn published_pen_block_reason(world: &World) -> crate::intensification::BuildGate {
        world
            .resource::<HerdRegistry>()
            .find(HERD_ID)
            .expect("the fixture herd")
            .build_blocked_reason
    }

    /// ⛔ **A MATERIALLY STALLED BUILD MUST NOT PUBLISH A NORMAL COUNTDOWN** — a forecast and a take
    /// disagreeing is the shape this arc keeps repairing (`docs/plan_standing_upkeep.md` §2.7).
    ///
    /// The `⌃` track promises *"you have 12 hurdles; it will stall at about a third"*; a queue that
    /// then counted down as though it would not defeats the readout the whole slice exists to add.
    /// So the **same coverage that scales the accrual scales the pace**, and all three states are
    /// swept **against each other** rather than against remembered numbers:
    ///
    /// | the store | the countdown |
    /// |---|---|
    /// | covers the pile | a real date |
    /// | covers a **fraction** | that date **stretched by the same fraction** |
    /// | covers **nothing** | the existing blocked sentinel, **naming the good** |
    #[test]
    fn a_materially_stalled_build_stretches_its_countdown_and_a_dry_store_blocks_it() {
        /// The share of the turn's pile the partly-stocked arm's store can pay for. A half is chosen
        /// so the stretched date is a *doubling*, which no rounding can turn into the unstretched
        /// one on a job this long.
        const COVERAGE: f32 = 0.5;

        // --- fully covered: the date the other two arms are measured against ----------------------
        let (mut full, band) = world_with_a_pen_build();
        stock_pen_materials(&mut full, band);
        let stocked = held_material(&full, band);
        full.run_system_once(advance_labor_allocation);
        let wanted = stocked - held_material(&full, band);
        let covered = match published_pen_countdown(&full) {
            Some(crate::intensification::BuildTurns::Turns(turns)) => turns,
            other => {
                panic!("fixture: a fully-stocked pen build must publish a real date, got {other:?}")
            }
        };
        assert!(
            covered > 1 && wanted > 0.0,
            "fixture: the job must take more than one turn AND draw material, or a stretch cannot \
             be told from a rounding (turns {covered}, drew {wanted})"
        );
        assert_eq!(
            published_pen_block_reason(&full),
            crate::intensification::BuildGate::Open,
            "a build the store covers is not blocked, and publishes no cause"
        );

        // --- partly covered: the SAME date, stretched by the SAME fraction ------------------------
        let (mut partial, band) = world_with_a_pen_build();
        stock_pen_materials_units(&mut partial, band, wanted * COVERAGE);
        partial.run_system_once(advance_labor_allocation);
        let stretched = match published_pen_countdown(&partial) {
            Some(crate::intensification::BuildTurns::Turns(turns)) => turns,
            other => {
                panic!("a PARTLY covered build still finishes, so it still names a date: {other:?}")
            }
        };
        assert!(
            stretched > covered,
            "**A HALF-COVERED BUILD TAKES LONGER, AND THE QUEUE MUST SAY SO** — {stretched} against \
             the covered {covered}"
        );
        // The pace is scaled, so the span is stretched by the inverse — asserted as a ratio against
        // the covered arm rather than as a number, so a retune of the rung moves both together. One
        // turn of slack for the `ceil` at each end.
        let expected = (covered as f32 / COVERAGE).ceil() as u32;
        assert!(
            stretched.abs_diff(expected) <= 1,
            "…and it is stretched by exactly the coverage: {stretched} against {covered}/{COVERAGE} \
             = {expected}"
        );
        assert_eq!(
            published_pen_block_reason(&partial),
            crate::intensification::BuildGate::Open,
            "**A STALL IS NOT A BLOCK** — a build that still banks something is not stuck, and must \
             not publish a cause it would have to explain away"
        );

        // --- covered by nothing: the blocked sentinel, naming the good ----------------------------
        let (mut dry, band) = world_with_a_pen_build();
        stock_pen_materials_units(&mut dry, band, 0.0);
        dry.run_system_once(advance_labor_allocation);
        assert_eq!(
            published_pen_countdown(&dry),
            Some(crate::intensification::BuildTurns::Blocked),
            "**A BUILD THAT CANNOT DRAW ONE UNIT OF WHAT IT EATS PUBLISHES THE BLOCKED SENTINEL** — \
             a number would be a promise, and `Holding`/`Rotting` would name the wrong fact"
        );
        assert_eq!(
            published_pen_block_reason(&dry),
            crate::intensification::BuildGate::Materials,
            "**AND IT SAYS WHY** — the rung's own gate HOLDS here, so a chain pass reading that \
             would publish a block with no cause; the remedy is the bench, not the Builders role"
        );
    }

    /// ⛔ **THE MATERIAL BILL IS THE STAMP, NEVER A LIVE RE-DERIVATION** — and the turn a fence
    /// **closes** is the one turn where the two differ (`docs/plan_standing_upkeep.md` §2.7).
    ///
    /// The material demand interpolates on the same position the work demand does and is carried
    /// across the same Population→Logistics boundary. `animal:pen` is `on_completion`, so a herd owes
    /// **no** hurdles while its fence is going up and the pen rung's whole rate the instant it
    /// closes — and the settlement struck its bill *before* that turn's accrual, so the store paid
    /// nothing. **A decay pass that re-derived the demand live would read a full bill against a `0`
    /// payment on the very turn the pen went up**, tripping the neglect counter on a band that had
    /// done everything right. That is the *"a fully-staffed band bleeds ~0.03 work/turn for ever
    /// while re-arming its grace every turn"* defect this arc already fixed once, restated in a
    /// second currency.
    ///
    /// **The closing turn is the whole fixture**, and the assertion that the pen really did close on
    /// it is what stops this passing on a build that never finished.
    #[test]
    fn the_turn_a_fence_closes_is_judged_against_the_bill_the_keepers_were_handed() {
        let (mut world, band) = world_with_a_pen_build();
        stock_pen_materials(&mut world, band);
        let (_, width) = pen_pile_and_width();

        // Walk to the turn the fence closes — the one turn on which the stamped bill and a live
        // reading part company.
        let mut closed = false;
        for _ in 0..width.ceil() as u32 * 4 {
            world.run_system_once(advance_labor_allocation);
            if world
                .resource::<HerdRegistry>()
                .find(HERD_ID)
                .expect("the fixture herd")
                .is_corralled()
            {
                closed = true;
                break;
            }
        }
        assert!(
            closed,
            "fixture: the fence must close inside the walk, or there is no closing turn to judge"
        );

        let ladder = crate::intensification::LadderConfig::builtin();
        let fauna_cfg = crate::fauna_config::FaunaConfig::builtin();
        let herd = world
            .resource::<HerdRegistry>()
            .find(HERD_ID)
            .expect("the fixture herd")
            .clone();
        // **LIVENESS**: with the fence up, a live reading is a real, positive bill — so the two
        // readings genuinely differ on this turn and the equality below is not a truism.
        let live = fauna::herd_upkeep_material_demand(&herd, &fauna_cfg, &ladder, PEN_MATERIAL);
        assert!(
            live > 0.0,
            "fixture: a CLOSED fence owes hurdles, or the stamp and the live reading cannot differ \
             (live {live})"
        );
        assert_eq!(
            herd.upkeep_materials_demanded
                .get(PEN_MATERIAL)
                .copied()
                .unwrap_or(crate::intensification::NO_UPKEEP_DEMAND),
            crate::intensification::NO_UPKEEP_DEMAND,
            "**THE BILL WAS STRUCK BEFORE THE ACCRUAL** — half a fence is no fence, so the keepers \
             were handed nothing to pay for this turn"
        );
        assert_eq!(
            fauna::herd_material_keeping_basis(&herd, &fauna_cfg, &ladder)
                .get(PEN_MATERIAL)
                .copied()
                .unwrap_or(crate::intensification::NO_UPKEEP_DEMAND),
            crate::intensification::NO_UPKEEP_DEMAND,
            "…and the basis the decay pass judges against is that STAMP, not the live {live} the \
             fence now owes — a live read makes a correctly-kept pen permanently short"
        );
        // **AND SO THE GOOD IS NOT WHAT MAKES THIS TURN UNMET** — which is what stops the neglect
        // counter re-arming for ever on a band that had bought every hurdle its fence asked for.
        //
        // The **work** half is asked with the bill met, because this fixture stands up no `husbandry`
        // keepers: the claim under test is the material stamp, and folding the work shortfall in
        // would make the assertion fail for a reason it is not about.
        let met_work = crate::fauna::herd_keeping_basis(&herd, &fauna_cfg, &ladder);
        assert!(
            !crate::intensification::keeping_is_short(
                met_work,
                met_work,
                &fauna::herd_material_keeping_basis(&herd, &fauna_cfg, &ladder),
                &herd.upkeep_materials_supplied,
            ),
            "a keeping that met its WORK bill is not short on the turn the fence closed — the good \
             it was billed for was nothing, and the store paid nothing"
        );
    }

    /// ⛔ **THE DECAY RIDES THE WORST OF THE TWO SHORTFALL FRACTIONS, AND THERE IS ONE COUNTER**
    /// (§4.9 item 12). Three cases, asserted **against each other** rather than against remembered
    /// numbers: hands short, goods short, and short of both.
    #[test]
    fn the_decay_fraction_is_the_worst_of_the_work_and_the_material_shortfalls() {
        use crate::intensification::{keeping_is_short, keeping_shortfall_fraction};

        const WORK_DEMAND: f32 = 4.0;
        const MATERIAL_DEMAND: f32 = 1.0;
        let goods =
            || std::collections::BTreeMap::from([(PEN_MATERIAL.to_string(), MATERIAL_DEMAND)]);
        let paid =
            |amount: f32| std::collections::BTreeMap::from([(PEN_MATERIAL.to_string(), amount)]);

        // Hands short (a quarter unmet), goods in hand.
        let hands_short = keeping_shortfall_fraction(
            WORK_DEMAND,
            WORK_DEMAND * 0.75,
            &goods(),
            &paid(MATERIAL_DEMAND),
        );
        // Goods short (half unmet), fully staffed.
        let goods_short = keeping_shortfall_fraction(
            WORK_DEMAND,
            WORK_DEMAND,
            &goods(),
            &paid(MATERIAL_DEMAND * 0.5),
        );
        // Short of both — the worse of the two, never their sum.
        let both_short = keeping_shortfall_fraction(
            WORK_DEMAND,
            WORK_DEMAND * 0.75,
            &goods(),
            &paid(MATERIAL_DEMAND * 0.5),
        );

        assert!(
            (hands_short - 0.25).abs() < FORECAST_EPSILON,
            "fully supplied with goods, the fraction is the HANDS' own: {hands_short}"
        );
        assert!(
            (goods_short - 0.5).abs() < FORECAST_EPSILON,
            "**FULLY STAFFED WITH NO HURDLES ROTS AT THE HURDLES' RATE**: {goods_short}"
        );
        assert!(
            (both_short - goods_short).abs() < FORECAST_EPSILON,
            "short of both, it is the WORSE of the two and never their sum: {both_short} against \
             {goods_short} (a sum would read {})",
            hands_short + goods_short
        );

        // **ONE COUNTER, ONE GRACE.** The counter increments if ANY of them is short and resets only
        // when all are met — the same `neglect_turns` and the same `upkeep.grace_turns`.
        assert!(
            keeping_is_short(
                WORK_DEMAND,
                WORK_DEMAND,
                &goods(),
                &paid(MATERIAL_DEMAND * 0.5)
            ),
            "a GOOD short is an unmet turn, on the rung's existing grace"
        );
        assert!(
            keeping_is_short(
                WORK_DEMAND,
                WORK_DEMAND * 0.75,
                &goods(),
                &paid(MATERIAL_DEMAND)
            ),
            "…and so is a HAND short, exactly as it always was"
        );
        assert!(
            !keeping_is_short(WORK_DEMAND, WORK_DEMAND, &goods(), &paid(MATERIAL_DEMAND)),
            "**AND IT RESETS ONLY WHEN ALL OF THEM ARE MET** — the pairing that stops this passing \
             on a predicate that always answers `true`"
        );
    }

    /// ⛔ **AN UNKEPT ROAD AND A RIVAL'S ROAD ARE TWO DIFFERENT REFUSALS.**
    ///
    /// [`super::route_head_gate`] used to answer [`BuildGate::OwnedByOther`] for both, and for a road
    /// **nobody keeps** that is a false sentence rather than a terse one: it sends the player looking
    /// for a rival that does not exist, past the one road on the map they could simply have claimed
    /// by re-issuing the verb. The two states have opposite remedies - negotiate or give up, against
    /// *take it on* - which is the whole reason [`BuildGate::NoKeeper`] exists.
    ///
    /// **A road loses its keeper without anybody deciding to drop it**: `Road::set_position` releases
    /// it the moment decay or disuse takes the tile back below `traffic_ceiling`. So the unkept case
    /// is the ordinary end of a road nobody walked, not an edge case worth collapsing.
    ///
    /// ⛔ **IT IS ASSERTED AT THE GATE, WHICH IS THE ONLY PLACE IT CAN BE.** The verdict is consumed
    /// by `source_banking_its_first_work` - struck *before* the turn's `prune_build_queue` - and that
    /// prune then drops the entry of any road this band does not keep, so neither cause survives to
    /// the published row. A wire-level fixture would assert on an entry that no longer exists.
    ///
    /// **All three arms, because any two pass on a collapsed vocabulary**: the unkept road, the
    /// rival's road, and a road this band really does keep, which must clear the gate outright.
    #[test]
    fn an_unkept_road_and_a_rivals_road_refuse_a_grade_for_different_reasons() {
        let ladder = crate::intensification::LadderConfig::builtin();
        let tile = UVec2::new(5, 6);
        let ours = crate::components::BandId(1);
        let theirs = crate::components::BandId(2);
        let faction = crate::orders::FactionId(0);

        // The faction knows `roadbuilding`, so the knowledge term can never be the answer below and
        // the keeper terms are the only ones under test.
        let mut discovery = crate::resources::DiscoveryProgressLedger::default();
        discovery.add_progress(
            faction,
            crate::routes::ROADBUILDING_DISCOVERY_ID,
            crate::scalar::scalar_one(),
        );
        let threshold = ladder.knowledge.completion_threshold;

        // A trail worn in to the top of the free floor, which is where a `grade` that has banked
        // nothing stands.
        let seat = |keeper: Option<crate::components::BandId>| {
            let mut roads = crate::routes::RoadRegistry::default();
            let road = roads.road_or_trail(tile, &ladder);
            road.set_position(crate::routes::traffic_ceiling(&ladder), &ladder);
            if let Some(band) = keeper {
                road.take_keeper(
                    crate::routes::RoadKeeper { faction, band },
                    crate::routes::NEAR_ENOUGH_TO_KEEP,
                    &ladder,
                );
            }
            roads
        };
        let gate = |roads: &crate::routes::RoadRegistry| {
            super::route_head_gate(
                roads,
                Some(ours),
                tile,
                Improvement::Grade,
                faction,
                &discovery,
                threshold,
                &ladder,
            )
        };

        let unkept = gate(&seat(None));
        assert_eq!(
            unkept,
            crate::intensification::BuildGate::NoKeeper,
            "a road NOBODY keeps refuses because it has no keeper - reporting `{}` here tells the \
             player to go and find a band that does not exist",
            crate::intensification::BuildGate::OwnedByOther.key()
        );

        let rivals = gate(&seat(Some(theirs)));
        assert_eq!(
            rivals,
            crate::intensification::BuildGate::OwnedByOther,
            "and a road another band really does keep is the one case that IS somebody else's"
        );

        assert_ne!(
            unkept, rivals,
            "**LIVENESS**: the two must be different causes, or the distinction this variant exists \
             for has been collapsed again"
        );

        assert!(
            gate(&seat(Some(ours))).holds(),
            "**LIVENESS**: and a road this band keeps must clear the gate outright, or the two \
             refusals above are simply a gate that never opens"
        );
    }

    /// ⛔ **A ROAD'S STONE IS FLAT AND ITS WORK IS NOT** — the one asymmetry on the route branch,
    /// and the assertion that keeps it.
    ///
    /// `routes::road_rung_span` prices a route rung's span at the keeper's own `keeper_remoteness`,
    /// so a road far from the band that keeps it costs proportionally more **worker-turns**. The
    /// **pile does not scale**: a tile of road needs the same twenty stone wherever it lies, and
    /// remoteness already taxes the getting there — taxing it twice would price a distant road out
    /// on an axis the player cannot see.
    ///
    /// **The flatness is arithmetic, not a special case**, which is exactly why it needs pinning: the
    /// draw is `pile × (accrual / width)` and a whole climb banks exactly `width`, so the remoteness
    /// in the denominator is cancelled by the remoteness in the work that fills it.
    ///
    /// ⛔ **AND IT IS DRIVEN THROUGH [`super::head_build_legs`], NOT THROUGH THE SPAN DIRECTLY.**
    /// The identity holds only if the leg is quoted at the width the arm will actually charge
    /// against; quote it at the *unscaled* width — the one-line mistake — and a remote road silently
    /// eats `pile × remoteness` while every arithmetic-only assertion still passes. So the fixture
    /// builds a real road at a real remoteness and asks the seam the turn asks.
    #[test]
    fn a_remote_road_costs_more_work_and_exactly_the_same_stone() {
        /// A keeper far enough out that the rung's span is a different number — the liveness term.
        const REMOTE: f32 = 2.5;
        let ladder = crate::intensification::LadderConfig::builtin();
        let pile = ladder
            .rung(RungKey::RoutePavedRoad)
            .build_materials()
            .map(|(_, amount)| amount)
            .sum::<f32>();
        assert!(
            pile > super::NOTHING_DEMANDED,
            "fixture: `route:paved_road` must declare a pile, or this asserts nothing"
        );

        // A road standing at the TOP of the dirt rung, so the only leg left is the paved one.
        let seat = |remoteness: f32| {
            let mut roads = crate::routes::RoadRegistry::default();
            let tile = UVec2::new(3, 4);
            let (base, width) =
                crate::routes::road_rung_span(RungKey::RouteDirtRoad, &ladder, remoteness);
            let road = roads.road_or_trail(tile, &ladder);
            road.set_position(base + width, &ladder);
            road.take_keeper(
                crate::routes::RoadKeeper {
                    faction: crate::orders::FactionId(0),
                    band: crate::components::BandId(1),
                },
                remoteness,
                &ladder,
            );
            (roads, tile)
        };

        let drawn_over_the_whole_rung = |remoteness: f32| {
            let (roads, tile) = seat(remoteness);
            let legs = super::head_build_legs(
                &BuildSource::Road(tile),
                RungKey::RoutePavedRoad,
                &ForageRegistry::default(),
                &HerdRegistry::default(),
                &roads,
                &ladder,
            );
            // ⛔ **THE MECHANISM, STATED DIRECTLY.** The pile is spread over the leg's third term,
            // and the flatness is that term being the road's OWN priced width — the same number the
            // arm banks work against. Quote the leg at the unscaled span and the stone inflates by
            // the remoteness; this is the assertion that names that, rather than leaving it to be
            // inferred from a total further down.
            let (_, priced) =
                crate::routes::road_rung_span(RungKey::RoutePavedRoad, &ladder, remoteness);
            for (rung, _, width) in &legs {
                assert!(
                    (*width - priced).abs() < FORECAST_EPSILON,
                    "{rung:?} at remoteness {remoteness}: the pile is spread over the leg's width, \
                     so that width MUST be the road's own priced span ({width} against {priced}) - \
                     an unscaled one here multiplies the stone by the remoteness"
                );
            }
            let owed: f32 = legs.iter().map(|(_, owed, _)| *owed).sum();
            // The whole climb in one draw: the accrual covers everything the legs still owe, which
            // is what *"over the whole rung"* means in this arithmetic.
            let drawn = super::build_material_wants(&legs, owed, &ladder)
                .values()
                .sum::<f32>();
            (owed, drawn)
        };

        let (near_work, near_stone) = drawn_over_the_whole_rung(crate::routes::NEAR_ENOUGH_TO_KEEP);
        let (remote_work, remote_stone) = drawn_over_the_whole_rung(REMOTE);

        assert!(
            remote_work > near_work + FORECAST_EPSILON,
            "**LIVENESS**: the remote road must cost MORE WORK ({remote_work} against {near_work}), \
             or remoteness is not in the span and the stone below has nothing to fail to inherit"
        );
        assert!(
            (near_stone - pile).abs() < FORECAST_EPSILON,
            "a whole climb draws exactly the declared pile: {near_stone} against {pile}"
        );
        assert!(
            (remote_stone - near_stone).abs() < FORECAST_EPSILON,
            "⛔ THE PILE IS FLAT: a road at remoteness {REMOTE} must swallow the same stone as one \
             next door — {remote_stone} against {near_stone}. A remote road draws it MORE SLOWLY, \
             over more turns; it does not draw more of it."
        );
    }

    /// **A TWO-LEG QUEUE ENTRY DRAWS EACH LEG'S OWN PILE**, at that leg's own rung's rate over that
    /// leg's own width — the property a single-leg fixture cannot see.
    ///
    /// Asserted at [`super::build_material_wants`], because the shipped ladder has no two-leg climb whose
    /// legs *both* declare a pile: the plant branch's two declare none, and a `corral` requires a
    /// herd already tamed. Stating the legs is what makes the arithmetic under test the
    /// **apportionment** rather than the roster.
    #[test]
    fn a_two_leg_entry_draws_each_legs_own_pile() {
        let ladder = crate::intensification::LadderConfig::builtin();
        let (pile, width) = pen_pile_and_width();

        /// What the first leg has left — small, so a turn's accrual genuinely spills into the second.
        const FIRST_LEG_OWED: f32 = 1.0;
        let legs = [
            (RungKey::AnimalPen, FIRST_LEG_OWED, width),
            (RungKey::AnimalPen, width, width),
        ];
        let accrual = FIRST_LEG_OWED + 3.0;
        let drawn = super::build_material_wants(&legs, accrual, &ladder)
            .get(PEN_MATERIAL)
            .copied()
            .expect("both legs declare the pen's material");
        assert!(
            (drawn - pile * accrual / width).abs() < FORECAST_EPSILON,
            "each leg draws its own rung's pile over its own width, so the two sum to the whole \
             turn's share: {drawn} against {pile} × {accrual}/{width}"
        );

        // **THE OWED CAP IS WHAT MAKES A COMPLETING TURN HONEST** — an accrual that overruns every
        // leg draws exactly the legs' own remainders and no more.
        let capped = super::build_material_wants(&legs, width * 10.0, &ladder)
            .get(PEN_MATERIAL)
            .copied()
            .expect("both legs declare the pen's material");
        assert!(
            (capped - pile * (FIRST_LEG_OWED + width) / width).abs() < FORECAST_EPSILON,
            "an overrunning accrual draws exactly the legs' own remainders: {capped}"
        );
        assert!(
            capped > drawn,
            "**LIVENESS**: the two arms must differ, or the cap is not being read"
        );
    }

    /// **The animal twin, rung 3.** A pen that finishes this turn clears `Corral` the same way — the
    /// keeper crew stays on the herd, under the stance it chose, and starts drawing the pen's
    /// harvest.
    #[test]
    fn a_completed_pen_clears_the_improvement_and_leaves_the_stance_alone() {
        const BIG_HERD_CAP: f32 = 1_000.0;
        let (mut world, tile) = world_with_source(CAP);
        reseat_herd(&mut world, BIG_HERD_CAP, BIG_HERD_CAP);
        grant_knowledge(&mut world, PENNING_DISCOVERY_ID);
        {
            let mut registry = world.resource_mut::<HerdRegistry>();
            registry.herds[0].tame_outright(
                BAND_FACTION,
                &crate::intensification::LadderConfig::builtin(),
            );
        }
        let turns_to_build = {
            let ladder = world.resource::<LadderConfigHandle>().get();
            turns_to_finish(
                ladder.rung(RungKey::AnimalPen),
                BUILDER_FLOOR,
                RUNG_COST_UNSCALED,
                harness_herd_load(&world),
            )
        };
        let builders = animal_builders(&world, RungKey::AnimalPen);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: BUILDER_FLOOR,
                },
                workers: WORKERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        declare_herd_build(&mut world, band, HERD_ID, Improvement::Corral, builders);
        // **The pen eats hurdles now**, so a fixture measuring its pacing has to hold some or it
        // measures a stall it staged itself — see [`stock_pen_materials`].
        stock_pen_materials(&mut world, band);

        // Walked to rather than forecast — see the Tame fixture above.
        let mut turns_taken = 0;
        for _ in 0..turns_to_build.saturating_mul(4) {
            // **The METER is the build's own completion test** — `is_corralled()` is the fence flag
            // the completion sets, and the verb is cleared on the same turn the meter fills, so
            // looping on the flag would sample a turn where the verb is already gone.
            if world
                .resource::<HerdRegistry>()
                .find(HERD_ID)
                .unwrap()
                .corral_meter_full()
            {
                break;
            }
            assert_eq!(
                queued_job(&world, band, BuildSource::Herd(HERD_ID.to_string())),
                Some(BuildJob::Rung(Improvement::Corral)),
                "an unfinished build keeps its entry"
            );
            world.run_system_once(advance_labor_allocation);
            turns_taken += 1;
        }
        assert!(
            turns_taken > 1,
            "fixture: the build must take real turns, or 'completion is a transition' is untested"
        );
        assert!(
            world
                .resource::<HerdRegistry>()
                .find(HERD_ID)
                .unwrap()
                .is_corralled(),
            "fixture: the pen must go up"
        );
        // The loop above exited ON the completing turn, so the state below is the post-completion
        // one — no extra turn is needed, and running one would be measuring the turn *after* the
        // transition rather than the transition.
        let completed = only_source_assignment(&world, band);
        assert_eq!(
            completed.workers, WORKERS,
            "the keeper crew stays on the pen"
        );
        assert_eq!(
            queued_job(&world, band, BuildSource::Herd(HERD_ID.to_string())),
            None,
            "completion retires the entry"
        );
        let LaborTarget::Hunt { fauna_id, floor } = &completed.target else {
            panic!("completion must not change the target's KIND: {completed:?}");
        };
        assert_eq!(
            *floor, BUILDER_FLOOR,
            "the player's stance is never rewritten (issue #442)"
        );
        assert_eq!(fauna_id, HERD_ID, "the same herd");
    }

    /// Without the earned knowledge, the improvements accrue **nothing** — the take is still the
    /// reduced preparing dip (the crew tries, and gets nowhere), but no progress is made. The command
    /// layer rejects the assignment outright; this guards the sim-side gate underneath it.
    #[test]
    fn investment_policies_accrue_nothing_without_the_knowledge() {
        let (mut world, tile) = world_with_source(CAP);
        let builders = plant_builders(&world, RungKey::PlantTended);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    floor: 0.5,
                    species: None,
                    take_species: TakeSelection::EVERYTHING,
                },
                workers: WORKERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        declare_patch_build(&mut world, band, SOURCE, Improvement::Cultivate, builders);
        world.run_system_once(advance_labor_allocation);
        assert_eq!(
            patch_progress(&world),
            0.0,
            "Cultivate without Cultivation knowledge accrues nothing"
        );

        let (mut world, tile) = world_with_source(CAP);
        {
            let mut registry = world.resource_mut::<HerdRegistry>();
            registry.herds[0].tame_outright(
                BAND_FACTION,
                &crate::intensification::LadderConfig::builtin(),
            );
        }
        let builders = animal_builders(&world, RungKey::AnimalPen);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.5,
                },
                workers: WORKERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        declare_herd_build(&mut world, band, HERD_ID, Improvement::Corral, builders);
        world.run_system_once(advance_labor_allocation);
        let herd = world.resource::<HerdRegistry>().find(HERD_ID).unwrap();
        assert_eq!(
            herd.rung_work_done(RungKey::AnimalPen, &LadderConfig::builtin()),
            0.0,
            "Corral without PENNING knowledge builds nothing (the §4.3 gate reshuffle — Herding \
             is no longer enough)"
        );
        assert!(!herd.is_corralled());
    }

    /// A Corral assignment on a herd that is **not domesticated** builds nothing (the second gate).
    #[test]
    fn corral_accrues_nothing_on_a_wild_herd() {
        let (mut world, tile) = world_with_source(CAP);
        grant_knowledge(&mut world, PENNING_DISCOVERY_ID);
        let builders = animal_builders(&world, RungKey::AnimalPen);
        let band = spawn_band(
            &mut world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: 0.5,
                },
                workers: WORKERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        declare_herd_build(&mut world, band, HERD_ID, Improvement::Corral, builders);
        world.run_system_once(advance_labor_allocation);
        let herd = world.resource::<HerdRegistry>().find(HERD_ID).unwrap();
        assert_eq!(
            herd.rung_work_done(RungKey::AnimalPen, &LadderConfig::builtin()),
            0.0,
            "a wild herd cannot be penned — tame it first"
        );
    }

    // ---------------------------------------------------------------------------------------------
    // The knowledge pattern (slice 4, `docs/plan_intensification_ladder.md` §4): **practising a rung
    // teaches the knowledge that unlocks the next rung's verb** — where "practising rung N" means
    // *working a source that currently STANDS ON rung N*, not "using rung N's verb".
    // ---------------------------------------------------------------------------------------------

    /// A herd big enough that a Sustain/Tame take never scrapes it out of the `Thriving` band
    /// mid-test — the earn gate reads the phase, so a starved fixture would pass for the wrong
    /// reason. (Mirrors the local const the corral/tame yield tests use.)
    const TEACHING_HERD_CAP: f32 = 1_000.0;

    /// Faction 0's ledger progress on `discovery`.
    fn knowledge(world: &World, discovery: u32) -> f32 {
        world
            .resource::<DiscoveryProgressLedger>()
            .get_progress(BAND_FACTION, discovery)
            .to_f32()
    }

    /// Staff a band on the source herd under `policy` and resolve one turn.
    fn hunt_one_turn(world: &mut World, tile: Entity, policy: f32) {
        spawn_band(
            world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Hunt {
                    fauna_id: HERD_ID.to_string(),
                    floor: policy,
                },
                workers: WORKERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        world.run_system_once(advance_labor_allocation);
    }

    /// Staff a band on the source patch under `policy` and resolve one turn.
    fn forage_one_turn(world: &mut World, tile: Entity, policy: f32) {
        spawn_band(
            world,
            tile,
            vec![LaborAssignment {
                target: LaborTarget::Forage {
                    tile: SOURCE,
                    floor: policy,
                    species: None,
                    take_species: TakeSelection::EVERYTHING,
                },
                workers: WORKERS,
                kit: None,
                priority: SourcePriority::default(),
                upkeep_kit: None,
            }],
        );
        world.run_system_once(advance_labor_allocation);
    }

    /// **Rung 1 is unchanged by the refactor.** A Sustain hunt on a Thriving *wild* herd still earns
    /// Herding — the shipped §0 behaviour — now driven by the `animal:wild` rung's `earns_knowledge`
    /// rather than a hard-coded branch. It teaches **Herding and nothing else**: Penning is the rung
    /// above, and rung 1 must not skip it.
    #[test]
    fn sustain_hunting_a_wild_herd_still_earns_herding_only() {
        let (mut world, tile) = world_with_source(CAP);
        hunt_one_turn(&mut world, tile, 0.5);

        assert!(
            knowledge(&world, HERDING_DISCOVERY_ID) > 0.0,
            "a Sustain hunt on a Thriving wild herd still earns Herding"
        );
        assert_eq!(
            knowledge(&world, PENNING_DISCOVERY_ID),
            0.0,
            "a WILD herd teaches Herding — Penning comes from keeping TAMED ones"
        );
    }

    /// **The heart of the arc.** The *same* Sustain hunt on a herd that has climbed to **pastoral**
    /// earns **Penning** instead — "you learn herding by managing wild herds; penning by managing
    /// tamed ones". Same verb, different rung, different lesson.
    #[test]
    fn sustain_hunting_a_pastoral_herd_earns_penning() {
        let (mut world, tile) = world_with_source(CAP);
        reseat_herd(&mut world, TEACHING_HERD_CAP, TEACHING_HERD_CAP);
        {
            let mut registry = world.resource_mut::<HerdRegistry>();
            registry.herds[0].tame_outright(
                BAND_FACTION,
                &crate::intensification::LadderConfig::builtin(),
            );
            assert!(
                registry.herds[0].is_domesticated(),
                "the herd stands on rung 2"
            );
        }
        hunt_one_turn(&mut world, tile, 0.5);

        assert!(
            knowledge(&world, PENNING_DISCOVERY_ID) > 0.0,
            "working a PASTORAL herd earns Penning — the rung it stands on decides the lesson"
        );
    }

    /// The plant twin: working a **tended** patch earns **Seed Selection**. The rung decides, not the
    /// verb — a tended patch pays its managed harvest under Sustain, and tending it *is* the practice.
    #[test]
    fn working_a_tended_patch_earns_seed_selection() {
        let (mut world, _tile) = world_with_source(CAP);
        let tile = world.resource::<TileRegistry>().tiles[0];
        {
            let mut registry = world.resource_mut::<ForageRegistry>();
            let patch = registry.patch_mut(SOURCE).expect("seeded patch");
            patch.complete_cultivation(
                BAND_FACTION,
                &crate::intensification::LadderConfig::builtin(),
            );
            assert!(patch.is_cultivated(), "the patch stands on rung 2");
        }
        forage_one_turn(&mut world, tile, 0.5);

        assert!(
            knowledge(&world, SEED_SELECTION_DISCOVERY_ID) > 0.0,
            "working a TENDED patch earns Seed Selection"
        );
    }

    /// **§4.2, RESTATED AS A RATE — a deeper floor learns SLOWER, and stripping learns nothing.**
    ///
    /// It replaced `the_overdrawing_policies_teach_nothing_at_any_rung`, whose subject was a **step**
    /// at the food peak (teach at or above it, nothing below). The harvest floor made restraint a
    /// rate (`intensification::learn_multiplier`, §3), so "these floors teach nothing" is no longer
    /// true of anything but `floor = 0` — and asserting the old inequality would now be asserting
    /// the model the arc removed. Swept across both webs and both of the rungs that teach, so a
    /// future rung cannot quietly opt out.
    #[test]
    fn a_deeper_floor_learns_slower_and_stripping_learns_nothing() {
        // Descending, so each entry must teach strictly less than the one before it. It reaches
        // **above** the food peak — the range the retired four-stance axis could not express.
        const DESCENDING_FLOORS: [f32; 4] = [0.9, 0.5, 0.3, 0.15];

        /// The floor at which nothing is left standing — the one that must teach exactly nothing
        /// because the *rate* is zero.
        const STRIP_IT_BARE: f32 = 0.0;

        /// **Leave it all standing.** The other degenerate end: the rate is its highest (×2), but
        /// nothing stands above the floor, so `crew_is_working_the_source` is false. Watching teaches
        /// nothing — the trade the dial offers, taken past its limit.
        const TOUCH_NOTHING: f32 = 1.0;

        // Animal rung 1 (wild) and rung 2 (pastoral), then plant rung 1 (wild) and rung 2 (tended).
        let hunt_lesson = |floor: f32, tamed: bool| {
            let (mut world, tile) = world_with_source(CAP);
            reseat_herd(&mut world, TEACHING_HERD_CAP, TEACHING_HERD_CAP);
            if tamed {
                world.resource_mut::<HerdRegistry>().herds[0].tame_outright(
                    BAND_FACTION,
                    &crate::intensification::LadderConfig::builtin(),
                );
            }
            hunt_one_turn(&mut world, tile, floor);
            let lesson = if tamed {
                PENNING_DISCOVERY_ID
            } else {
                HERDING_DISCOVERY_ID
            };
            knowledge(&world, lesson)
        };
        let forage_lesson = |floor: f32, cultivated: bool| {
            let (mut world, _) = world_with_source(CAP);
            let tile = world.resource::<TileRegistry>().tiles[0];
            // Seated at capacity, so a floor **above** the food peak still leaves stock standing and
            // the sweep can reach the over-restraint half of the dial at all. The default fixture
            // sits on `K/2`, where every such floor honestly takes nothing.
            {
                let mut registry = world.resource_mut::<ForageRegistry>();
                let patch = registry.patch_mut(SOURCE).expect("seeded patch");
                patch.biomass = patch.carrying_capacity;
            }
            if cultivated {
                world
                    .resource_mut::<ForageRegistry>()
                    .patch_mut(SOURCE)
                    .expect("seeded patch")
                    .complete_cultivation(
                        BAND_FACTION,
                        &crate::intensification::LadderConfig::builtin(),
                    );
            }
            forage_one_turn(&mut world, tile, floor);
            let lesson = if cultivated {
                SEED_SELECTION_DISCOVERY_ID
            } else {
                CULTIVATION_DISCOVERY_ID
            };
            knowledge(&world, lesson)
        };

        // **Both webs assert the SAME shape**, which is the point of the predicate the earn path
        // reads: it is the escapement room, in biomass, before the whole-animal quantiser — so the
        // *lesson* is `learn_rate × learn_multiplier(floor) / lesson_cost` on both webs and orders strictly
        // in the floor. It does not, and must not, depend on `body_mass`.
        for rung_two in [false, true] {
            for (web, lesson_at) in [
                ("plant", &forage_lesson as &dyn Fn(f32, bool) -> f32),
                ("animal", &hunt_lesson as &dyn Fn(f32, bool) -> f32),
            ] {
                // **Liveness first**: a diff-based property improves when the feature breaks, so an
                // ordering sweep alone would pass on an earn path that credited zero everywhere.
                assert!(
                    lesson_at(DESCENDING_FLOORS[0], rung_two) > 0.0,
                    "{web}: the rung must actually teach at the top floor (rung 2 = {rung_two})"
                );
                let mut previous = f32::INFINITY;
                for floor in DESCENDING_FLOORS {
                    let learned = lesson_at(floor, rung_two);
                    assert!(
                        learned < previous,
                        "{web} floor {floor} must learn strictly less than the floor above it \
                         (rung 2 = {rung_two}): {learned} vs {previous}"
                    );
                    previous = learned;
                }
                assert_eq!(
                    lesson_at(STRIP_IT_BARE, rung_two),
                    0.0,
                    "{web}: stripping the source bare teaches nothing (rung 2 = {rung_two})"
                );
                assert_eq!(
                    lesson_at(TOUCH_NOTHING, rung_two),
                    0.0,
                    "{web}: …and watching it teaches nothing either (rung 2 = {rung_two})"
                );
            }
        }
    }

    // `a_source_that_is_not_thriving_teaches_nothing` was deleted with its subject: the
    // `EcologyPhase::Thriving` gate both earn sites carried is gone (`docs/plan_harvest_floor.md`
    // §3.2), replaced by `crew_is_working_the_source` and a floor-paced rate. A collapsing source
    // that still stands above the crew's floor is still being practised on.

    /// **§4.2 — the two food webs learn separately.** Hunting only ever advances the animal track and
    /// foraging the plant track: a master rancher isn't automatically a farmer. This falls out of the
    /// rung's branch, but it is the claim the design makes, so it is asserted directly.
    #[test]
    fn the_two_food_webs_do_not_cross_teach() {
        // Hunting a wild herd teaches Herding and touches NEITHER plant knowledge.
        let (mut world, tile) = world_with_source(CAP);
        hunt_one_turn(&mut world, tile, 0.5);
        assert!(knowledge(&world, HERDING_DISCOVERY_ID) > 0.0);
        assert_eq!(
            knowledge(&world, CULTIVATION_DISCOVERY_ID),
            0.0,
            "hunting must not teach Cultivation"
        );
        assert_eq!(
            knowledge(&world, SEED_SELECTION_DISCOVERY_ID),
            0.0,
            "hunting must not teach Seed Selection"
        );

        // Foraging a wild patch teaches Cultivation and touches NEITHER animal knowledge.
        let (mut world, _) = world_with_source(CAP);
        let tile = world.resource::<TileRegistry>().tiles[0];
        forage_one_turn(&mut world, tile, 0.5);
        assert!(knowledge(&world, CULTIVATION_DISCOVERY_ID) > 0.0);
        assert_eq!(
            knowledge(&world, HERDING_DISCOVERY_ID),
            0.0,
            "foraging must not teach Herding"
        );
        assert_eq!(
            knowledge(&world, PENNING_DISCOVERY_ID),
            0.0,
            "foraging must not teach Penning"
        );
    }
}
