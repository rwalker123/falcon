//! Noun resolvers + template rendering — **where emergent dressing enters** the narration.
//!
//! The freshness pillar only works if a line can say "ash-elk" *because this world generated an
//! ash-elk*, so every expression is a template and `nouns` binds each slot to live sim state
//! before rendering (`docs/plan_the_telling.md` §2b).
//!
//! Resolvers ship as a registry alongside signals: content composes them, and a `from`/`fallback`
//! naming an unregistered resolver fails at **load**, not at render. Rendering itself is
//! infallible — every placeholder and field is validated when the catalog loads.

use std::collections::BTreeMap;

use sim_schema::TerrainType;

use crate::{
    components::{LaborTarget, PopulationCohort},
    fauna::{EcologyPhase, HerdRegistry},
    fauna_config::FaunaConfig,
    orders::FactionId,
    sites::DiscoveredSiteRecord,
    sites_config::SitesConfig,
};

use super::signals::BandView;

/// A resolved noun slot. `Named` carries the three forms copy needs so lines read naturally;
/// `Scalar` is a bare number the template renders as a rounded integer.
#[derive(Debug, Clone, PartialEq)]
pub enum Noun {
    Named {
        name: String,
        plural: String,
        adjective: String,
    },
    Scalar(f64),
}

impl Noun {
    pub(crate) fn named(
        name: impl Into<String>,
        plural: impl Into<String>,
        adjective: impl Into<String>,
    ) -> Self {
        Noun::Named {
            name: name.into(),
            plural: plural.into(),
            adjective: adjective.into(),
        }
    }

    /// Bare `{slot}` rendering: a number rounds to an integer, a named thing gives its name.
    fn render_bare(&self) -> String {
        match self {
            Noun::Named { name, .. } => name.clone(),
            Noun::Scalar(value) => format!("{}", value.round() as i64),
        }
    }

    fn render_field(&self, field: NounField) -> String {
        match self {
            Noun::Named {
                name,
                plural,
                adjective,
            } => match field {
                NounField::Name => name.clone(),
                NounField::Plural => plural.clone(),
                NounField::Adjective => adjective.clone(),
            },
            // A scalar has no word forms; every field reads as the number itself rather than
            // rendering an empty hole into player-facing copy.
            Noun::Scalar(_) => self.render_bare(),
        }
    }
}

/// The addressable fields of a `{slot.field}` placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NounField {
    Name,
    Plural,
    Adjective,
}

impl NounField {
    pub fn as_str(self) -> &'static str {
        match self {
            NounField::Name => "name",
            NounField::Plural => "plural",
            NounField::Adjective => "adjective",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        match key {
            "name" => Some(NounField::Name),
            "plural" => Some(NounField::Plural),
            "adjective" => Some(NounField::Adjective),
            _ => None,
        }
    }
}

/// Every noun resolver content may bind a slot to.
const NOUN_RESOLVERS: [&str; 6] = [
    "band.count",
    "biome.current_dominant",
    "site.last_discovered",
    "fauna.most_hunted",
    "fauna.most_domesticated",
    "fauna.most_collapsed",
];

/// Is `resolver` a registered noun resolver? The load-time validation hook for `from`/`fallback`.
pub fn is_registered_resolver(resolver: &str) -> bool {
    NOUN_RESOLVERS.contains(&resolver)
}

pub fn registered_resolvers() -> &'static [&'static str] {
    &NOUN_RESOLVERS
}

// --- biome fit tags ------------------------------------------------------------------------
//
// A wardrobe entry's `fit.biome` is a **hard** gate against the ground the band is standing on.
// The vocabulary is narrative, not mechanical (one biome reads as several words a writer would
// reach for), so it lives here rather than on `TerrainType`.

/// Narrative biome tags for a terrain, as a wardrobe entry's `fit.biome` may name them.
fn biome_tags(terrain: TerrainType) -> &'static [&'static str] {
    match terrain {
        TerrainType::AlluvialPlain => &["alluvial", "lowland", "fertile"],
        TerrainType::Floodplain => &["floodplain", "lowland", "fertile", "river"],
        TerrainType::RiverDelta => &["delta", "lowland", "fertile", "river"],
        TerrainType::NavigableRiver => &["river", "water"],
        TerrainType::PrairieSteppe => &["grassland", "savanna", "steppe", "open"],
        TerrainType::HighPlateau => &["grassland", "highland", "open"],
        TerrainType::PeriglacialSteppe => &["steppe", "cold", "open"],
        TerrainType::MixedWoodland => &["woodland", "forest"],
        TerrainType::BorealTaiga => &["forest", "cold"],
        TerrainType::PeatHeath => &["heath", "wetland"],
        TerrainType::FreshwaterMarsh | TerrainType::MangroveSwamp => &["wetland", "marsh"],
        TerrainType::TidalFlat => &["coast", "wetland"],
        TerrainType::SemiAridScrub => &["scrub", "arid", "open"],
        TerrainType::HotDesertErg | TerrainType::RockyReg | TerrainType::SaltFlat => {
            &["desert", "arid"]
        }
        TerrainType::OasisBasin => &["oasis", "arid", "fertile"],
        TerrainType::Tundra => &["tundra", "cold", "open"],
        TerrainType::Glacier | TerrainType::SeasonalSnowfield => &["ice", "cold"],
        TerrainType::RollingHills => &["hills", "upland"],
        TerrainType::AlpineMountain | TerrainType::KarstHighland => &["mountain", "highland"],
        TerrainType::CanyonBadlands => &["badlands", "arid", "upland"],
        TerrainType::ActiveVolcanoSlope
        | TerrainType::BasalticLavaField
        | TerrainType::AshPlain
        | TerrainType::FumaroleBasin => &["volcanic"],
        TerrainType::ImpactCraterField
        | TerrainType::KarstCavernMouth
        | TerrainType::SinkholeField
        | TerrainType::AquiferCeiling => &["strange"],
        TerrainType::DeepOcean
        | TerrainType::ContinentalShelf
        | TerrainType::InlandSea
        | TerrainType::CoralShelf
        | TerrainType::HydrothermalVentField => &["water", "coast"],
    }
}

/// Every biome tag any terrain can carry, for load-time validation of `fit.biome`.
pub fn is_known_biome_tag(tag: &str) -> bool {
    TerrainType::VALUES
        .into_iter()
        .any(|terrain| biome_tags(terrain).contains(&tag))
}

/// Does the ground the band stands on carry `tag`? An unknown current terrain matches nothing,
/// so a biome-gated entry is simply excluded rather than rendering against the wrong ground.
pub fn terrain_has_biome_tag(terrain: Option<TerrainType>, tag: &str) -> bool {
    terrain
        .map(|t| biome_tags(t).contains(&tag))
        .unwrap_or(false)
}

// --- resolution ----------------------------------------------------------------------------

/// Everything the noun resolvers read. Assembled once per turn beside the signal sample, so each
/// source is walked once and every slot in the turn sees consistent state.
pub struct NounContext<'a> {
    pub faction: FactionId,
    /// Total people across the faction's resident bands (the `band.count` scalar).
    pub band_people: f64,
    /// The terrain the player's band is standing on, if resolvable.
    pub current_terrain: Option<TerrainType>,
    /// The faction's most recently discovered site.
    pub last_discovered_site: Option<&'a DiscoveredSiteRecord>,
    pub sites: &'a SitesConfig,
    pub bands: &'a [BandView<'a>],
    pub herds: &'a HerdRegistry,
    pub fauna: &'a FaunaConfig,
}

/// Resolve one noun slot. `None` is normal early-game (nothing hunted yet, no site found); a
/// wardrobe entry that *requires* an unresolved slot is excluded from selection.
pub fn resolve(resolver: &str, ctx: &NounContext<'_>) -> Option<Noun> {
    match resolver {
        "band.count" => Some(Noun::Scalar(ctx.band_people)),
        "biome.current_dominant" => ctx.current_terrain.map(|terrain| {
            let adjective = terrain.as_adjective();
            Noun::named(adjective, adjective, adjective)
        }),
        "site.last_discovered" => ctx.last_discovered_site.map(|record| {
            let name = ctx
                .sites
                .site(&record.site_id)
                .map(|def| def.display_name.clone())
                .unwrap_or_else(|| record.site_id.clone());
            Noun::named(name.clone(), name.clone(), name)
        }),
        "fauna.most_hunted" => most_hunted_species(ctx).map(|species| species_noun(&species, ctx)),
        "fauna.most_domesticated" => {
            most_domesticated_species(ctx).map(|species| species_noun(&species, ctx))
        }
        "fauna.most_collapsed" => {
            most_collapsed_species(ctx).map(|species| species_noun(&species, ctx))
        }
        // Unreachable through validated content; an unknown resolver simply resolves to nothing.
        _ => None,
    }
}

/// Build a `Named` noun from a species display name, using the data-driven `plural`/`adjective`
/// forms where the species table supplies them (both default to the name — see the note on
/// `SpeciesDef::plural`: naive English pluralisation would produce "deers").
fn species_noun(species: &str, ctx: &NounContext<'_>) -> Noun {
    match ctx.fauna.species_by_display(species) {
        Some(def) => Noun::named(species, def.plural_or_name(), def.adjective_or_name()),
        None => Noun::named(species, species, species),
    }
}

/// The species most assigned as a `LaborTarget::Hunt` target, ties broken by species name
/// ascending so the result is independent of assignment order.
fn most_hunted_species(ctx: &NounContext<'_>) -> Option<String> {
    let mut tally: BTreeMap<String, u32> = BTreeMap::new();
    for band in ctx.bands {
        let Some(labor) = band.labor else { continue };
        for assignment in &labor.assignments {
            let LaborTarget::Hunt { fauna_id, .. } = &assignment.target else {
                continue;
            };
            if assignment.workers == 0 {
                continue;
            }
            if let Some(herd) = ctx.herds.find(fauna_id) {
                *tally.entry(herd.species.clone()).or_insert(0) += assignment.workers;
            }
        }
    }
    // `BTreeMap` iterates by species name ascending, so `max_by_key` on the count alone would
    // keep the *last* of a tie; compare the negated name ordering explicitly instead.
    tally
        .into_iter()
        .max_by(|(a_name, a_count), (b_name, b_count)| {
            a_count.cmp(b_count).then(b_name.cmp(a_name))
        })
        .map(|(species, _)| species)
}

/// The species with the highest `domestication_progress` among herds this faction owns.
fn most_domesticated_species(ctx: &NounContext<'_>) -> Option<String> {
    ctx.herds
        .entries()
        .iter()
        .filter(|herd| herd.owner == Some(ctx.faction))
        .max_by(|a, b| {
            a.domestication_progress
                .total_cmp(&b.domestication_progress)
                .then(b.species.cmp(&a.species))
        })
        .map(|herd| herd.species.clone())
}

/// The species of the largest herd currently in `EcologyPhase::Collapsing`.
fn most_collapsed_species(ctx: &NounContext<'_>) -> Option<String> {
    ctx.herds
        .entries()
        .iter()
        .filter(|herd| herd.ecology_phase == EcologyPhase::Collapsing)
        .max_by(|a, b| {
            a.biomass
                .total_cmp(&b.biomass)
                .then(b.species.cmp(&a.species))
        })
        .map(|herd| herd.species.clone())
}

/// The band whose tile answers `biome.current_dominant` — the faction's first resident band in a
/// stable order, so the "ground we are standing on" is the same one every turn.
pub fn primary_band<'a>(bands: &'a [BandView<'a>]) -> Option<&'a PopulationCohort> {
    bands.first().map(|band| band.cohort)
}

// --- templates -----------------------------------------------------------------------------

/// One `{slot}` / `{slot.field}` reference inside a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placeholder {
    pub slot: String,
    pub field: Option<NounField>,
}

/// Parse every placeholder in `template`. An unknown *field* is an error here — at **load** —
/// which is what makes rendering infallible.
pub fn template_placeholders(template: &str) -> Result<Vec<Placeholder>, String> {
    let mut out = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let close = after
            .find('}')
            .ok_or_else(|| format!("unterminated `{{` in template {template:?}"))?;
        let body = &after[..close];
        let (slot, field) = match body.split_once('.') {
            Some((slot, field)) => {
                let parsed = NounField::from_key(field).ok_or_else(|| {
                    format!(
                        "template {template:?} references unknown noun field {field:?} \
                         (expected one of name/plural/adjective)"
                    )
                })?;
                (slot, Some(parsed))
            }
            None => (body, None),
        };
        if slot.is_empty() {
            return Err(format!(
                "template {template:?} has an empty `{{}}` placeholder"
            ));
        }
        out.push(Placeholder {
            slot: slot.to_string(),
            field,
        });
        rest = &after[close + 1..];
    }
    Ok(out)
}

/// Render a template against resolved nouns. Infallible by construction: the catalog's
/// `validate()` proved every placeholder names a declared slot with a legal field, and the
/// selection stage proved every *required* slot resolved. A slot that is declared, optional and
/// unresolved renders as an empty string rather than leaving braces in player-facing copy.
pub fn render(template: &str, nouns: &BTreeMap<String, Noun>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            // Unterminated brace: validated away at load, so just emit the remainder verbatim.
            out.push('{');
            rest = after;
            continue;
        };
        let body = &after[..close];
        let (slot, field) = match body.split_once('.') {
            Some((slot, field)) => (slot, NounField::from_key(field)),
            None => (body, None),
        };
        if let Some(noun) = nouns.get(slot) {
            out.push_str(&match field {
                Some(field) => noun.render_field(field),
                None => noun.render_bare(),
            });
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nouns() -> BTreeMap<String, Noun> {
        BTreeMap::from([
            ("beast".to_string(), Noun::named("Red Deer", "deer", "deer")),
            ("count".to_string(), Noun::Scalar(31.4)),
        ])
    }

    #[test]
    fn renders_bare_named_plural_and_scalar_slots() {
        assert_eq!(render("The {beast.plural} ran", &nouns()), "The deer ran");
        assert_eq!(render("{beast} watched", &nouns()), "Red Deer watched");
        assert_eq!(render("{beast.adjective} bones", &nouns()), "deer bones");
        // A scalar renders as a rounded integer.
        assert_eq!(render("We are {count}.", &nouns()), "We are 31.");
    }

    #[test]
    fn renders_repeated_and_adjacent_placeholders() {
        assert_eq!(
            render("{beast.plural}, {beast.plural}!", &nouns()),
            "deer, deer!"
        );
        assert_eq!(render("no placeholders", &nouns()), "no placeholders");
    }

    #[test]
    fn placeholders_parse_slots_and_fields() {
        let parsed = template_placeholders("We are {count}. The {beast.plural} stayed.").unwrap();
        assert_eq!(
            parsed,
            vec![
                Placeholder {
                    slot: "count".to_string(),
                    field: None
                },
                Placeholder {
                    slot: "beast".to_string(),
                    field: Some(NounField::Plural)
                },
            ]
        );
    }

    #[test]
    fn unknown_placeholder_field_is_a_load_time_error() {
        let err = template_placeholders("{beast.colour}").unwrap_err();
        assert!(err.contains("unknown noun field"), "{err}");
    }

    #[test]
    fn unterminated_placeholder_is_a_load_time_error() {
        assert!(template_placeholders("the {beast").is_err());
    }

    #[test]
    fn resolvers_and_biome_tags_are_registered() {
        for resolver in registered_resolvers() {
            assert!(is_registered_resolver(resolver));
        }
        assert!(!is_registered_resolver("fauna.dominant_local"));
        // The tags the shipped catalog uses must be in the vocabulary.
        for tag in ["alluvial", "grassland", "savanna"] {
            assert!(is_known_biome_tag(tag), "{tag} should be a known biome tag");
        }
        assert!(!is_known_biome_tag("moonscape"));
    }

    #[test]
    fn biome_tag_matching_needs_a_resolved_terrain() {
        assert!(terrain_has_biome_tag(
            Some(TerrainType::PrairieSteppe),
            "savanna"
        ));
        assert!(!terrain_has_biome_tag(
            Some(TerrainType::AlpineMountain),
            "savanna"
        ));
        assert!(!terrain_has_biome_tag(None, "savanna"));
    }
}
