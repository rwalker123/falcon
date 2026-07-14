//! Data-driven tuning for scouting expeditions (traveling parties).
//!
//! Loaded from `data/expedition_config.json`. An **expedition** is a detached `StartingUnit` band
//! (a `PopulationCohort` tagged `Expedition`, deliberately lacking `ResidentBand`) that a faction
//! outfits with workers + provisions and drives out to explore. This config holds the levers that
//! shape how big a party can be, how far it can report from (communication range), what it observes
//! per turn, and how its carried provisions are drawn and consumed. Mirrors the `sites_config.rs` /
//! `fauna_config.rs` loader pattern (baked-in builtin + optional file/env override), so tuning an
//! expedition is a JSON edit, no code — **plus** the `crisis_config.rs` validation convention:
//! [`ExpeditionConfig::validate`] runs inside `from_json_str`, so no load path can hand the sim a
//! config that would silently disable expeditions (see that method for what is bounded and why).

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use bevy::prelude::Resource;
use serde::Deserialize;
use thiserror::Error;

pub const BUILTIN_EXPEDITION_CONFIG: &str = include_str!("data/expedition_config.json");

/// Root expedition configuration. Every lever here is a tuning knob read through the handle — no
/// bare literal drives expedition behavior. The builtin JSON carries all fields (it is the single
/// source of the shipped defaults), so no `#[serde(default)]` merge is needed; a malformed, partial,
/// **or invariant-breaking** override file is rejected and falls back to the builtin via
/// [`load_expedition_config_from_env`]. See [`ExpeditionConfig::validate`] for what is enforced and
/// what is deliberately left free.
#[derive(Debug, Clone, Deserialize)]
pub struct ExpeditionConfig {
    /// Hard cap on the workers a single expedition party can carry (also clamped to the home band's
    /// available workers at launch).
    pub max_party_size: u32,
    /// Base communication range (tiles): the expedition only flushes its observed tiles to the
    /// faction map while within this hex distance of its home band. Early-game default is short so
    /// distant exploration reports back "as a lump on return".
    pub comm_range_tiles: u32,
    /// Multiplier on `comm_range_tiles` reserved for a future movement/comm-tech signal. Stubbed at
    /// 1.0 today — mirrors migration's stubbed movement-tech factor.
    // TODO(phase2): scale by movement/comm tech; mirrors migration's stubbed factor.
    pub comm_range_tech_factor: f32,
    /// The expedition's per-turn line-of-sight observation radius. Default matches the band base
    /// sight range (`visibility_config.json` BandScout `base_range` 6) — an expedition sees as far
    /// as a normal band, it just reports on a comm-range delay.
    pub observe_sight_range: u32,
    /// Provisions drawn from the home band's larder at launch = `party × hex-distance-to-target ×
    /// this`.
    pub provision_draw_per_worker_per_tile: f32,
    /// Provisions the party consumes per turn = `party × this`. Non-fatal at zero in v1
    /// (deterministic success). Scouts only — a hunting party lives off its own kills.
    pub provision_upkeep_per_worker: f32,
    /// Hunting-expedition (PR 2) tuning — how a party follows a herd, harvests, and delivers.
    pub hunt: HuntExpeditionConfig,
    /// Scout opportunistic-replenish (PR 2) tuning — when/where a scout tops up off passing game.
    pub replenish: ReplenishConfig,
}

/// Hunting-expedition levers (`docs/plan_exploration_and_sites.md` §2b). A hunt party follows a
/// migratory herd, takes a **productive** hunt's worth of biomass each turn (`workers ×
/// per_worker_biomass_capacity`, capped per policy — **Sustain** by the shared MSY *flow* ceiling
/// (`fauna::hunt_policy_ceiling`, the same take a resident band's Hunt arm makes), the depleting
/// policies by their *stock* headroom down to their floor; see `hunt_expedition_ceiling`),
/// accumulates food up to a carry cap, and delivers it. The take **policy** is chosen
/// per-expedition at launch (on the mission), not here.
#[derive(Debug, Clone, Deserialize)]
pub struct HuntExpeditionConfig {
    /// Carry cap = `party_workers × this` (provisions). Tuned so a party fills a cap in ~4–6 active
    /// turns at the productive take rate (`party × per_worker_biomass_capacity × provisions_per_biomass`).
    pub per_worker_carry: f32,
    /// How close (hex distance) the party must be to the herd to take food this turn.
    pub reach_tiles: u32,
    /// When the herd's circuit brings it within this hex distance of the home band, the party may
    /// flip to deliver early — but only with a worthwhile load (see `min_deliver_fraction`).
    pub drop_off_within_tiles: u32,
    /// Early-delivery gate: with the herd near the band (`drop_off_within_tiles`), only flip to
    /// deliver once `carried ≥ this × cap` (default 0.5) — fixes the empty-larder flip-flop.
    pub min_deliver_fraction: f32,
    /// Viability threshold for the **launch forecast** (`hunt_trip_forecast`): a trip whose
    /// estimated turns-to-fill exceeds this is flagged NOT VIABLE in the `ExpeditionSent` feed line
    /// (it still launches — the player's call). Default **20** = 4× the throughput-implied trip
    /// length, where that length is `per_worker_carry / (per_worker_biomass_capacity ×
    /// provisions_per_biomass)` = `4.0 / (40 × 0.02)` = 5 turns — the turns any policy needs to fill
    /// a pack at *full* hunter throughput. Beyond 4× that, the herd's sustainable yield (not the
    /// hunters) is the binding constraint by a wide margin, and the trip is a trap.
    pub viability_warn_turns: u32,
    /// How far forward the launch forecast (`hunt_trip_forecast`) simulates the trip before giving
    /// up and reporting "won't fill". Default **60**. Two reasons for a bound:
    /// - *Information*: `viability_warn_turns` is 20, so a trip past ~3× that is emphatically not
    ///   viable and the exact turn count carries no information a player can act on — "won't fill"
    ///   says everything.
    /// - *Cost*: the forecast is exported per herd × policy × party size every snapshot, so the
    ///   horizon bounds the per-snapshot work (`policies × max_party_size × this` turn-steps/herd).
    pub forecast_horizon_turns: u32,
}

/// Scout opportunistic-replenish levers: the scout's own use of the shared `hunt_take` primitive.
#[derive(Debug, Clone, Deserialize)]
pub struct ReplenishConfig {
    /// Top up when remaining provisions are below `party_workers × provision_upkeep_per_worker ×
    /// this` (i.e. fewer than this many turns of upkeep remain).
    pub low_turns: u32,
    /// The scout must be within this hex distance of a huntable herd to top up.
    pub reach_tiles: u32,
}

/// The smallest meaningful value for a **counted** lever (turns or tiles). At `0` the behaviour the
/// lever gates does not run *at all* rather than running weakly: a `0` forecast horizon simulates
/// **zero** turns (so every trip reports "won't fill" and the client disables every send button), a
/// `0` reach can never be satisfied against a roaming herd, a `0` `low_turns` never triggers a
/// replenish. That is a silently disabled feature, not a tuning — so these levers are bounded here.
const MIN_COUNTED_LEVER: u32 = 1;

/// Upper bound for a lever expressed as a **fraction** of something (today only
/// `hunt.min_deliver_fraction`, a fraction of the carry cap): a gate above a full pack could never
/// open.
const MAX_FRACTION: f32 = 1.0;

impl ExpeditionConfig {
    pub fn builtin() -> Arc<Self> {
        Arc::new(
            Self::from_json_str(BUILTIN_EXPEDITION_CONFIG)
                .expect("builtin expedition config should parse and validate"),
        )
    }

    pub fn from_json_str(json: &str) -> Result<Self, ExpeditionConfigError> {
        let config: ExpeditionConfig = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }

    pub fn from_file(path: &Path) -> Result<Self, ExpeditionConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| ExpeditionConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        ExpeditionConfig::from_json_str(&contents)
    }

    /// Enforce the invariants that, if broken, would make an expedition **silently do nothing**
    /// rather than behave differently — the class of hole a `0` is most likely to open. Mirrors the
    /// `crisis_config.rs` loader convention (validate inside `from_json_str`, so **every** load path
    /// — builtin, default file, and the `EXPEDITION_CONFIG_PATH` override — is covered).
    ///
    /// Deliberately **unbounded** (they have coherent meanings at their extremes, so bounding them
    /// would be inventing policy): `comm_range_tiles` (`0` = "the party must physically walk back
    /// into camp to report"), `hunt.drop_off_within_tiles` (`0` = no early drop-off; a full pack
    /// still delivers), and the upper end of `max_party_size` / `hunt.forecast_horizon_turns` (both
    /// only cost snapshot time — the estimate table is `O(policies × max_party_size × horizon)` per
    /// herd — which is an operator's call, not an invariant).
    pub fn validate(&self) -> Result<(), ExpeditionConfigError> {
        // A party of zero workers can never be outfitted: `send_expedition` requires
        // `1 <= party_workers <= max_party_size`, so `0` refuses every launch.
        require_at_least("max_party_size", self.max_party_size, MIN_COUNTED_LEVER)?;
        // Negative/NaN would saturate to `0` in `effective_comm_range`'s `as u32` cast, silently
        // zeroing the comm range whatever `comm_range_tiles` says.
        require_positive_finite("comm_range_tech_factor", self.comm_range_tech_factor)?;
        // A scout that observes nothing is not a scout.
        require_at_least(
            "observe_sight_range",
            self.observe_sight_range,
            MIN_COUNTED_LEVER,
        )?;
        // Both are legitimately `0` (free launches / no upkeep — v1 ships deterministic success), but
        // a *negative* draw would pay the band provisions for launching a party.
        require_non_negative_finite(
            "provision_draw_per_worker_per_tile",
            self.provision_draw_per_worker_per_tile,
        )?;
        require_non_negative_finite(
            "provision_upkeep_per_worker",
            self.provision_upkeep_per_worker,
        )?;

        // Carry cap = `party × per_worker_carry`. At `0` the pack is full the instant it is empty:
        // every trip "completes" immediately with nothing aboard.
        require_positive_finite("hunt.per_worker_carry", self.hunt.per_worker_carry)?;
        // The take *and* the trip-completion decision both live inside the reach guard; `0` demands
        // the party stand on the herd's exact tile, which a roaming herd may never allow.
        require_at_least("hunt.reach_tiles", self.hunt.reach_tiles, MIN_COUNTED_LEVER)?;
        // The early-delivery gate. At `0` the party delivers an empty pack (the flip-flop bug this
        // lever exists to fix); above a full pack it could never open.
        require_fraction("hunt.min_deliver_fraction", self.hunt.min_deliver_fraction)?;
        // A `0` threshold flags every trip NOT VIABLE, making the signal meaningless.
        require_at_least(
            "hunt.viability_warn_turns",
            self.hunt.viability_warn_turns,
            MIN_COUNTED_LEVER,
        )?;
        // **The bug this validator was written for.** `simulate_hunt_trip` loops `1..=horizon`, so a
        // `0` horizon simulates zero turns: every herd × policy × party size reports `turns_to_fill =
        // None` + `first_turn_provisions = 0`, the launch feed says "the party will return empty" for
        // every trip, and the client's `_hunt_trip_impossible` gate disables every send button.
        // Hunting expeditions cease to exist, silently.
        require_at_least(
            "hunt.forecast_horizon_turns",
            self.hunt.forecast_horizon_turns,
            MIN_COUNTED_LEVER,
        )?;
        // Cross-field: a horizon shorter than the viability threshold is incoherent — a trip the
        // player would be told is viable (`turns_to_fill <= viability_warn_turns`) could not even be
        // *discovered* before the simulation gives up and reports "won't fill".
        require_at_least(
            "hunt.forecast_horizon_turns",
            self.hunt.forecast_horizon_turns,
            self.hunt.viability_warn_turns,
        )?;

        // Scouts top up below `party × upkeep × low_turns`; `0` never triggers, and a `0` reach
        // demands the herd's exact tile.
        require_at_least(
            "replenish.low_turns",
            self.replenish.low_turns,
            MIN_COUNTED_LEVER,
        )?;
        require_at_least(
            "replenish.reach_tiles",
            self.replenish.reach_tiles,
            MIN_COUNTED_LEVER,
        )?;

        Ok(())
    }

    /// Effective communication range in tiles (`comm_range_tiles × comm_range_tech_factor`, rounded).
    pub fn effective_comm_range(&self) -> u32 {
        (self.comm_range_tiles as f32 * self.comm_range_tech_factor).round() as u32
    }
}

fn require_at_least(
    field: &'static str,
    value: u32,
    min: u32,
) -> Result<(), ExpeditionConfigError> {
    if value < min {
        return Err(ExpeditionConfigError::Invalid {
            field,
            constraint: format!("be at least {min}"),
            value: value.to_string(),
        });
    }
    Ok(())
}

fn require_positive_finite(field: &'static str, value: f32) -> Result<(), ExpeditionConfigError> {
    if !value.is_finite() || value <= 0.0 {
        return Err(ExpeditionConfigError::Invalid {
            field,
            constraint: "be finite and greater than 0".to_string(),
            value: value.to_string(),
        });
    }
    Ok(())
}

fn require_non_negative_finite(
    field: &'static str,
    value: f32,
) -> Result<(), ExpeditionConfigError> {
    if !value.is_finite() || value < 0.0 {
        return Err(ExpeditionConfigError::Invalid {
            field,
            constraint: "be finite and at least 0".to_string(),
            value: value.to_string(),
        });
    }
    Ok(())
}

fn require_fraction(field: &'static str, value: f32) -> Result<(), ExpeditionConfigError> {
    if !value.is_finite() || value <= 0.0 || value > MAX_FRACTION {
        return Err(ExpeditionConfigError::Invalid {
            field,
            constraint: format!("be finite and in (0, {MAX_FRACTION}]"),
            value: value.to_string(),
        });
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum ExpeditionConfigError {
    #[error("failed to read expedition config from {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse expedition config: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("invalid expedition config: `{field}` must {constraint}, got {value}")]
    Invalid {
        field: &'static str,
        constraint: String,
        value: String,
    },
}

/// Handle for accessing the expedition configuration.
#[derive(Resource, Debug, Clone)]
pub struct ExpeditionConfigHandle(pub Arc<ExpeditionConfig>);

impl ExpeditionConfigHandle {
    pub fn new(config: Arc<ExpeditionConfig>) -> Self {
        Self(config)
    }

    pub fn get(&self) -> Arc<ExpeditionConfig> {
        Arc::clone(&self.0)
    }

    pub fn replace(&mut self, config: Arc<ExpeditionConfig>) {
        self.0 = config;
    }
}

impl Default for ExpeditionConfigHandle {
    fn default() -> Self {
        Self(ExpeditionConfig::builtin())
    }
}

/// Metadata about the expedition configuration source.
#[derive(Resource, Debug, Clone, Default)]
pub struct ExpeditionConfigMetadata {
    path: Option<PathBuf>,
}

impl ExpeditionConfigMetadata {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    pub fn set_path(&mut self, path: Option<PathBuf>) {
        self.path = path;
    }
}

/// Load expedition configuration from environment (`EXPEDITION_CONFIG_PATH`) or the default data
/// path, falling back to the baked-in builtin. Not wired into the `reload_config` hot-reload path
/// (mirrors `sites_config.json` / `fauna_config.json`).
///
/// Every candidate goes through [`ExpeditionConfig::from_json_str`], so it is **validated** before it
/// can reach the sim: an override that would silently disable a feature (a `0` forecast horizon, a
/// `0` carry cap, …) is rejected and logged at **error** level naming the broken invariant, and the
/// known-good builtin is used instead. A bad override therefore fails loudly and can never take
/// effect — it cannot quietly do something plausible.
pub fn load_expedition_config_from_env() -> (Arc<ExpeditionConfig>, ExpeditionConfigMetadata) {
    let override_path = env::var("EXPEDITION_CONFIG_PATH").ok().map(PathBuf::from);
    let default_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/data/expedition_config.json");

    let candidates: Vec<PathBuf> = match override_path {
        Some(ref path) => vec![path.clone()],
        None => vec![default_path.clone()],
    };

    for path in candidates {
        match ExpeditionConfig::from_file(&path) {
            Ok(config) => {
                tracing::info!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    "expedition_config.loaded=file"
                );
                return (Arc::new(config), ExpeditionConfigMetadata::new(Some(path)));
            }
            // A broken invariant is an operator error, not a missing file: it means the config that
            // *was* found says something incoherent. Shout about it.
            Err(err @ ExpeditionConfigError::Invalid { .. }) => {
                tracing::error!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "expedition_config.invalid_rejected"
                );
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::config",
                    path = %path.display(),
                    error = %err,
                    "expedition_config.load_failed"
                );
            }
        }
    }

    let config = ExpeditionConfig::builtin();
    tracing::info!(
        target: "shadow_scale::config",
        "expedition_config.loaded=builtin"
    );
    (config, ExpeditionConfigMetadata::new(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped builtin, mutated per-test to break exactly one invariant.
    fn valid_config() -> ExpeditionConfig {
        ExpeditionConfig::from_json_str(BUILTIN_EXPEDITION_CONFIG).expect("builtin is valid")
    }

    /// Assert `config` is rejected by `validate()` and that the error names `field`.
    fn assert_rejects(config: ExpeditionConfig, field: &str) {
        match config.validate() {
            Err(ExpeditionConfigError::Invalid { field: got, .. }) => assert_eq!(
                got, field,
                "validate() rejected the config, but blamed the wrong field"
            ),
            other => panic!("expected `{field}` to be rejected, got {other:?}"),
        }
    }

    #[test]
    fn builtin_config_parses() {
        let config = ExpeditionConfig::builtin();
        assert_eq!(config.max_party_size, 8);
        assert_eq!(config.comm_range_tiles, 2);
        assert_eq!(config.observe_sight_range, 6);
        assert!(config.provision_draw_per_worker_per_tile > 0.0);
        assert!(config.provision_upkeep_per_worker > 0.0);
        assert!(config.hunt.per_worker_carry > 0.0);
        assert!(config.hunt.reach_tiles >= 1);
        // Min-deliver gate in (0, 1]; the viability warning needs at least one turn to warn about.
        assert!(config.hunt.min_deliver_fraction > 0.0 && config.hunt.min_deliver_fraction <= 1.0);
        assert!(config.hunt.viability_warn_turns >= 1);
        // The forecast horizon must simulate at least one turn — at `0`, `simulate_hunt_trip`'s
        // `1..=horizon` loop runs zero times and EVERY trip reports "won't fill" (the sibling
        // assertion this file was missing).
        assert!(config.hunt.forecast_horizon_turns >= 1);
        // ...and it must reach at least as far as the viability threshold, or a "viable" trip could
        // never be discovered before the simulation gives up.
        assert!(config.hunt.forecast_horizon_turns >= config.hunt.viability_warn_turns);
        assert!(config.replenish.low_turns >= 1);
        assert!(config.replenish.reach_tiles >= 1);
    }

    /// **The regression this validator exists for.** A `0` forecast horizon used to be accepted
    /// silently and killed every hunting expedition on the map (zero simulated turns → `turns_to_fill
    /// = None` for every herd × policy × party size → the client disables every send button). It must
    /// now be *rejected*, not merely "not shipped".
    #[test]
    fn zero_forecast_horizon_is_rejected() {
        let mut config = valid_config();
        config.hunt.forecast_horizon_turns = 0;
        assert_rejects(config, "hunt.forecast_horizon_turns");
    }

    /// The guard must hold through the real load path too — an `EXPEDITION_CONFIG_PATH`-style JSON
    /// override with a `0` horizon is refused by `from_json_str`, so it can never reach the sim.
    #[test]
    fn zero_forecast_horizon_is_rejected_when_loaded_from_json() {
        let json = BUILTIN_EXPEDITION_CONFIG.replace(
            "\"forecast_horizon_turns\": 60",
            "\"forecast_horizon_turns\": 0",
        );
        assert!(
            json != BUILTIN_EXPEDITION_CONFIG,
            "the builtin's forecast_horizon_turns default moved — update this test's patch"
        );
        match ExpeditionConfig::from_json_str(&json) {
            Err(ExpeditionConfigError::Invalid { field, .. }) => {
                assert_eq!(field, "hunt.forecast_horizon_turns");
            }
            other => panic!("a 0-horizon override must be rejected at load, got {other:?}"),
        }
    }

    /// A horizon shorter than the viability threshold is incoherent: every trip the player would be
    /// told is viable lies beyond the point the simulation stops looking.
    #[test]
    fn forecast_horizon_shorter_than_the_viability_threshold_is_rejected() {
        let mut config = valid_config();
        config.hunt.viability_warn_turns = 20;
        config.hunt.forecast_horizon_turns = 19;
        assert_rejects(config, "hunt.forecast_horizon_turns");
    }

    /// One rejection case: the field the error must blame, and the mutation that breaks it.
    type RejectionCase = (&'static str, fn(&mut ExpeditionConfig));

    /// Every other lever whose `0` (or negative/NaN) would silently disable behaviour rather than
    /// tune it. One case per bounded lever — the guard is proven, not assumed.
    #[test]
    fn levers_that_would_silently_disable_a_feature_are_rejected() {
        let cases: Vec<RejectionCase> = vec![
            // No party can be outfitted at all.
            ("max_party_size", |c| c.max_party_size = 0),
            // Saturates to a 0 comm range in the `as u32` cast, whatever comm_range_tiles says.
            ("comm_range_tech_factor", |c| c.comm_range_tech_factor = 0.0),
            ("comm_range_tech_factor", |c| {
                c.comm_range_tech_factor = -1.0
            }),
            ("comm_range_tech_factor", |c| {
                c.comm_range_tech_factor = f32::NAN
            }),
            // A scout that observes nothing.
            ("observe_sight_range", |c| c.observe_sight_range = 0),
            // A negative draw would *pay* the band for launching a party.
            ("provision_draw_per_worker_per_tile", |c| {
                c.provision_draw_per_worker_per_tile = -1.0
            }),
            ("provision_upkeep_per_worker", |c| {
                c.provision_upkeep_per_worker = -1.0
            }),
            // Carry cap 0 → the pack is "full" the instant it is empty; every trip returns nothing.
            ("hunt.per_worker_carry", |c| c.hunt.per_worker_carry = 0.0),
            // Reach 0 → the party must stand on a roaming herd's exact tile to ever take or finish.
            ("hunt.reach_tiles", |c| c.hunt.reach_tiles = 0),
            // 0 → deliver an empty pack (the flip-flop bug); > 1 → the gate can never open.
            ("hunt.min_deliver_fraction", |c| {
                c.hunt.min_deliver_fraction = 0.0
            }),
            ("hunt.min_deliver_fraction", |c| {
                c.hunt.min_deliver_fraction = 1.5
            }),
            // 0 → every trip is flagged NOT VIABLE, so the signal carries nothing.
            ("hunt.viability_warn_turns", |c| {
                c.hunt.viability_warn_turns = 0
            }),
            // 0 → a scout never tops up / must stand on the herd's exact tile.
            ("replenish.low_turns", |c| c.replenish.low_turns = 0),
            ("replenish.reach_tiles", |c| c.replenish.reach_tiles = 0),
        ];

        for (field, break_it) in cases {
            let mut config = valid_config();
            break_it(&mut config);
            assert_rejects(config, field);
        }
    }

    /// The invariants are a floor, not a straitjacket: the levers with coherent meanings at `0` stay
    /// loadable, so validation never invents policy the design didn't ask for.
    #[test]
    fn deliberately_unbounded_levers_still_validate() {
        let mut config = valid_config();
        // "The party must physically walk back into camp to report."
        config.comm_range_tiles = 0;
        // No early drop-off; a full pack still delivers.
        config.hunt.drop_off_within_tiles = 0;
        // Free launches, no upkeep (v1 ships deterministic success).
        config.provision_draw_per_worker_per_tile = 0.0;
        config.provision_upkeep_per_worker = 0.0;
        assert!(config.validate().is_ok());
    }

    #[test]
    fn effective_comm_range_applies_factor() {
        let config = ExpeditionConfig::builtin();
        assert_eq!(config.effective_comm_range(), config.comm_range_tiles);
    }
}
