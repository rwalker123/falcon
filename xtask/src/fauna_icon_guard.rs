//! `cargo xtask fauna-icon-guard` — the gate that every fauna species the SIM ships has bundled
//! map art on the CLIENT.
//!
//! ## The gap this closes (issue #439)
//!
//! `Steppe Runners` and `Marsh Grazers` are two of the roster's twenty species and neither had an
//! entry in `FoodIcons.HERD_SPECIES` **at all**, so `species_key_for` answered `""` and both drew
//! the `HERD_DEFAULT` OS emoji on a live map. Three separate doc comments asserted coverage was
//! complete and all three were false, because the check behind them was *"does every key in the
//! client's table have a PNG?"* — a question that **cannot see a species the client's table has
//! never heard of**. The client's own preview harness had the same blind spot by construction: its
//! roster is a hand-written list on the client side of the wire.
//!
//! So the only assertion that catches this class is one that starts from the **other side's**
//! roster. This guard reads `fauna_config.json`, and for every species' `display_name` walks the
//! exact path a herd marker walks at runtime:
//!
//! 1. resolve the label through a faithful copy of `FoodIcons.species_key_for` (longest keyword
//!    wins) — an empty answer means the species has no keyword and would draw an emoji;
//! 2. look that key up in `FaunaSprites.SPRITE_PATHS` — a miss means the emoji again;
//! 3. stat the PNG it names **and its `<name>.png.import` sidecar** — either missing means
//!    `IconSprites` loads nothing and the caller falls back.
//!
//! The sidecar is part of the runtime path, not metadata beside it: Godot never loads
//! `res://…/elk.png` off disk, it reads `elk.png.import` and loads the `.ctex` its `path=` key names
//! under `.godot/imported/`. A PNG committed without its sidecar therefore fails to load exactly as
//! if the art were absent — and it is an easy commit to make, because `.import` files are tracked,
//! nothing gitignores them, and this repo forbids broad `git add`, so every one is staged by hand.
//! No Godot harness runs in CI, so a PNG-only stat is the difference between a green gate and a
//! roster that draws emoji in every checkout but the author's.
//!
//! Every `SPRITE_PATHS` entry is stat'd the same way, so a typo'd filename or an unstaged sidecar is
//! caught even under a key no species currently reaches.
//!
//! ## Why it parses GDScript as data
//!
//! The two tables are the client's runtime source of truth; a copy of them here would be the very
//! duplication whose drift caused the defect. Reading the `.gd` files means the guard is checking
//! what actually ships. The parse is deliberately strict — a missing dict opener, a missing closing
//! `}`, or an empty table is an ERROR rather than a vacuous pass, because a reformat that silently
//! defeated the parse would leave a green gate over an unchecked table.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// The sim's fauna roster — the authority this guard checks the client against.
const FAUNA_CONFIG: &str = "core_sim/src/data/fauna_config.json";
/// Species entries live under this key; a config without it is read as the roster itself.
const SPECIES_SECTION: &str = "species";
/// The player-facing name embedded in a herd label, and therefore the string the client matches on.
const DISPLAY_NAME_FIELD: &str = "display_name";

/// Client root, which `res://` resolves to on disk.
const CLIENT_DIR: &str = "clients/godot_thin_client";
const RES_PREFIX: &str = "res://";

/// The keyword table the runtime matcher searches, and the key→PNG table the marker draws from.
const FOOD_ICONS_GD: &str = "clients/godot_thin_client/src/scripts/ui/FoodIcons.gd";
const HERD_SPECIES_DICT: &str = "HERD_SPECIES";
const FAUNA_SPRITES_GD: &str = "clients/godot_thin_client/src/scripts/ui/FaunaSprites.gd";
const SPRITE_PATHS_DICT: &str = "SPRITE_PATHS";
/// The `const SPRITE_DIR := "…"` the `SPRITE_PATHS` values are built from. Read from the file
/// rather than hardcoded, so moving the art folder does not need an edit here.
const SPRITE_DIR_CONST: &str = "SPRITE_DIR";

/// Godot's import sidecar suffix. It is APPENDED to the whole file name (`elk.png` →
/// `elk.png.import`), never substituted for the extension.
const IMPORT_SIDECAR_SUFFIX: &str = ".import";

pub fn run(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    if let Some(arg) = args.first() {
        return Err(format!(
            "fauna-icon-guard: unknown argument '{arg}' (this command takes none)"
        )
        .into());
    }

    let species = load_species(Path::new(FAUNA_CONFIG))?;
    let keywords = load_herd_keywords(Path::new(FOOD_ICONS_GD))?;
    let sprites = load_sprite_paths(Path::new(FAUNA_SPRITES_GD))?;

    // Species failures and table failures are kept apart so the headline can count SPECIES — the
    // thing a player sees go wrong — while a broken table entry no species reaches is still listed.
    let mut species_failures: Vec<String> = Vec::new();
    let mut table_failures: Vec<String> = Vec::new();
    let mut reached_files: BTreeSet<PathBuf> = BTreeSet::new();
    let mut flagged_files: BTreeSet<PathBuf> = BTreeSet::new();

    for (species_id, display_name) in &species {
        let key = species_key_for(display_name, &keywords);
        if key.is_empty() {
            species_failures.push(format!(
                "{species_id} (\"{display_name}\"): NO KEYWORD — `FoodIcons.species_key_for` returns \
                 \"\", so this species draws the `HERD_DEFAULT` OS emoji. Add a keyword to \
                 {HERD_SPECIES_DICT} that is a substring of the display name."
            ));
            continue;
        }
        let Some(res_path) = sprites.get(&key) else {
            species_failures.push(format!(
                "{species_id} (\"{display_name}\"): resolves to keyword `{key}`, which is NOT in \
                 {SPRITE_PATHS_DICT} — `FaunaSprites.for_herd` returns null and the marker falls back \
                 to the emoji."
            ));
            continue;
        };
        let disk_path = disk_path_for(res_path);
        match art_state(&disk_path) {
            ArtState::Bundled => {
                reached_files.insert(disk_path);
            }
            ArtState::MissingPng => {
                species_failures.push(format!(
                    "{species_id} (\"{display_name}\"): resolves to keyword `{key}` → `{res_path}`, \
                     but that file DOES NOT EXIST ({}). The texture load fails and the marker falls \
                     back to the emoji.",
                    disk_path.display()
                ));
                flagged_files.insert(disk_path);
            }
            ArtState::MissingSidecar(sidecar) => {
                species_failures.push(format!(
                    "{species_id} (\"{display_name}\"): resolves to keyword `{key}` → `{res_path}`, \
                     whose PNG is on disk but whose Godot IMPORT SIDECAR IS MISSING ({}). Godot does \
                     not load the PNG — it reads the sidecar's `path=` and loads the imported texture \
                     under `.godot/imported/` — so the load fails and the marker falls back to the \
                     emoji. Stage the `{IMPORT_SIDECAR_SUFFIX}` file beside the PNG (open the Godot \
                     project once to regenerate it if it was never created).",
                    sidecar.display()
                ));
                flagged_files.insert(disk_path);
            }
        }
    }

    // Stat'd independently of the roster: a typo'd filename or an unstaged sidecar under a key no
    // species currently reaches is still art that will never load the day a species does reach it.
    // Files already named by a species line are skipped — one broken file is one defect, however
    // many ways it is arrived at.
    for (key, res_path) in &sprites {
        let disk_path = disk_path_for(res_path);
        if flagged_files.contains(&disk_path) {
            continue;
        }
        match art_state(&disk_path) {
            ArtState::Bundled => {}
            ArtState::MissingPng => table_failures.push(format!(
                "{SPRITE_PATHS_DICT}[\"{key}\"] → `{res_path}`: file DOES NOT EXIST ({}). No species \
                 currently resolves to this key, so nothing renders wrong yet — but the day one does, \
                 it will.",
                disk_path.display()
            )),
            ArtState::MissingSidecar(sidecar) => table_failures.push(format!(
                "{SPRITE_PATHS_DICT}[\"{key}\"] → `{res_path}`: the PNG exists but its Godot IMPORT \
                 SIDECAR IS MISSING ({}). Godot resolves the PNG through that sidecar, so this art \
                 cannot load. No species currently resolves to this key, so nothing renders wrong yet \
                 — but the day one does, it will draw the emoji.",
                sidecar.display()
            )),
        }
    }

    if !species_failures.is_empty() || !table_failures.is_empty() {
        // Printed rather than carried in the error, because `main` renders a returned error with
        // `Debug` — which would escape every newline and hand back one unreadable line. The error is
        // then the one-line verdict, and this is the audit.
        eprintln!(
            "fauna-icon-guard FAILED — {} of {} species in {FAUNA_CONFIG} do not reach bundled art:",
            species_failures.len(),
            species.len()
        );
        for failure in species_failures.iter().chain(table_failures.iter()) {
            eprintln!("  - {failure}");
        }
        return Err(format!(
            "fauna-icon-guard: {} species and {} extra {SPRITE_PATHS_DICT} entr(ies) failed — see above",
            species_failures.len(),
            table_failures.len()
        )
        .into());
    }

    println!(
        "fauna-icon-guard: {} species -> {} sprites, all present with their `{IMPORT_SIDECAR_SUFFIX}` \
         sidecars ({} keys in {SPRITE_PATHS_DICT}, {} in {HERD_SPECIES_DICT})",
        species.len(),
        reached_files.len(),
        sprites.len(),
        keywords.len()
    );
    Ok(())
}

/// `(species id, display name)` for every species entry that has one.
///
/// Entries without a `display_name` are skipped rather than failing: the config's top level carries
/// `_comment_*` STRING keys beside its objects, and a species the sim does not name is not one the
/// client could match on anyway.
fn load_species(path: &Path) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let text = fs::read_to_string(path)
        .map_err(|err| format!("fauna-icon-guard: cannot read {}: {err}", path.display()))?;
    let root: Value = serde_json::from_str(&text)
        .map_err(|err| format!("fauna-icon-guard: cannot parse {}: {err}", path.display()))?;
    let roster = root
        .get(SPECIES_SECTION)
        .unwrap_or(&root)
        .as_object()
        .ok_or_else(|| {
            format!(
                "fauna-icon-guard: {} has no `{SPECIES_SECTION}` object",
                path.display()
            )
        })?;

    let species: Vec<(String, String)> = roster
        .iter()
        .filter_map(|(id, entry)| {
            let display_name = entry.get(DISPLAY_NAME_FIELD)?.as_str()?;
            Some((id.clone(), display_name.to_string()))
        })
        .collect();

    if species.is_empty() {
        return Err(format!(
            "fauna-icon-guard: no species with a `{DISPLAY_NAME_FIELD}` found in {} — the guard \
             would pass vacuously, so this is an error.",
            path.display()
        )
        .into());
    }
    Ok(species)
}

/// The keys of `FoodIcons.HERD_SPECIES` — the keywords the runtime matcher searches for.
fn load_herd_keywords(path: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let text = read_gd(path)?;
    let keys: Vec<String> = dict_lines(&text, HERD_SPECIES_DICT, path)?
        .iter()
        .filter_map(|line| quoted(line).first().map(|key| key.to_string()))
        .collect();
    if keys.is_empty() {
        return Err(empty_dict_error(HERD_SPECIES_DICT, path));
    }
    Ok(keys)
}

/// `FaunaSprites.SPRITE_PATHS` as key → `res://` path, with `SPRITE_DIR` expanded.
fn load_sprite_paths(path: &Path) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let text = read_gd(path)?;
    let sprite_dir = const_string(&text, SPRITE_DIR_CONST).ok_or_else(|| {
        format!(
            "fauna-icon-guard: no `const {SPRITE_DIR_CONST} := \"…\"` in {}",
            path.display()
        )
    })?;

    let mut sprites = BTreeMap::new();
    for line in dict_lines(&text, SPRITE_PATHS_DICT, path)? {
        // `"deer": SPRITE_DIR + "deer.png",` — the first quoted string is the key, the last is the
        // file. A value that spelled the whole `res://` path out is honoured as-is.
        let parts = quoted(&line);
        let (Some(key), Some(file)) = (parts.first(), parts.last()) else {
            continue;
        };
        if key == file {
            return Err(format!(
                "fauna-icon-guard: {SPRITE_PATHS_DICT} entry `{line}` in {} names no file",
                path.display()
            )
            .into());
        }
        let res_path = if file.starts_with(RES_PREFIX) {
            (*file).to_string()
        } else {
            format!("{sprite_dir}{file}")
        };
        sprites.insert((*key).to_string(), res_path);
    }
    if sprites.is_empty() {
        return Err(empty_dict_error(SPRITE_PATHS_DICT, path));
    }
    Ok(sprites)
}

fn read_gd(path: &Path) -> Result<String, Box<dyn Error>> {
    fs::read_to_string(path)
        .map_err(|err| format!("fauna-icon-guard: cannot read {}: {err}", path.display()).into())
}

fn empty_dict_error(dict: &str, path: &Path) -> Box<dyn Error> {
    format!(
        "fauna-icon-guard: parsed ZERO entries from `{dict}` in {} — the table cannot really be \
         empty, so the parse is broken and every check below it would pass vacuously.",
        path.display()
    )
    .into()
}

/// The body lines of a GDScript dict literal, comments and blanks dropped.
///
/// Bounded by the `NAME := {` opener and the next line that is exactly `}`, which is how the two
/// tables are formatted. Both bounds are required: a dict that never closes means the file was
/// reformatted into something this parser no longer understands, and guessing would be worse than
/// failing.
fn dict_lines(text: &str, dict: &str, path: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let opener = format!("{dict} := {{");
    let mut lines = text.lines().skip_while(|line| !line.contains(&opener));
    if lines.next().is_none() {
        return Err(format!(
            "fauna-icon-guard: no `{opener}` in {} — the table was renamed or reformatted.",
            path.display()
        )
        .into());
    }

    let mut body = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "}" {
            return Ok(body);
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        body.push(trimmed.to_string());
    }
    Err(format!(
        "fauna-icon-guard: `{dict}` in {} is never closed by a line that is exactly `}}`.",
        path.display()
    )
    .into())
}

/// The value of a `const NAME := "…"` declaration.
fn const_string(text: &str, name: &str) -> Option<String> {
    let opener = format!("const {name} := ");
    text.lines()
        .find(|line| line.trim_start().starts_with(&opener))
        .and_then(|line| quoted(line).first().map(|value| (*value).to_string()))
}

/// The double-quoted substrings of a line, in order.
///
/// Splitting on `"` puts every quoted run at an odd index. Neither table contains an escaped quote
/// (the keys are species keywords and the values are file names), so this needs no escape handling.
fn quoted(line: &str) -> Vec<&str> {
    line.split('"').skip(1).step_by(2).collect()
}

/// A faithful copy of `FoodIcons.species_key_for`: lowercase the label, then return the FIRST
/// `HERD_SPECIES` key found as a substring, walking the keys longest-first.
///
/// Longest-first is the whole behaviour — it is what makes "Wild Reindeer" resolve to `reindeer`
/// rather than to `deer`, and what lets a two-word key like `steppe runner` exist at all. Length is
/// counted in CHARACTERS to match GDScript's `String.length()`; ties break on the key itself, which
/// GDScript's `sort_custom` leaves unspecified but no pair of same-length keys currently matches the
/// same label.
fn species_key_for(label: &str, keywords: &[String]) -> String {
    let mut by_length: Vec<&String> = keywords.iter().collect();
    by_length.sort_by(|a, b| {
        b.chars()
            .count()
            .cmp(&a.chars().count())
            .then_with(|| a.cmp(b))
    });
    let lower = label.to_lowercase();
    by_length
        .into_iter()
        .find(|keyword| lower.contains(keyword.as_str()))
        .cloned()
        .unwrap_or_default()
}

/// The on-disk path a `res://` path names.
fn disk_path_for(res_path: &str) -> PathBuf {
    Path::new(CLIENT_DIR).join(res_path.trim_start_matches(RES_PREFIX))
}

/// The `.import` sidecar that belongs beside a bundled file.
///
/// The suffix is appended to the FULL file name, so this cannot be `Path::with_extension`, which
/// would turn `elk.png` into `elk.import` — a path that is always absent and would make the check
/// below fire on every species at once.
fn import_sidecar_for(disk_path: &Path) -> PathBuf {
    let mut name = disk_path.as_os_str().to_os_string();
    name.push(IMPORT_SIDECAR_SUFFIX);
    PathBuf::from(name)
}

/// What Godot would find for one bundled sprite: both halves of it, or which half is missing.
///
/// The two failures are kept apart because they have different causes and different fixes —
/// "generate the art" versus "stage the sidecar / re-import the project".
#[derive(Debug, PartialEq, Eq)]
enum ArtState {
    /// PNG and sidecar both present — the marker draws its sprite.
    Bundled,
    /// No PNG on disk.
    MissingPng,
    /// PNG on disk, sidecar absent from the path it should occupy.
    MissingSidecar(PathBuf),
}

fn art_state(disk_path: &Path) -> ArtState {
    if !disk_path.exists() {
        return ArtState::MissingPng;
    }
    let sidecar = import_sidecar_for(disk_path);
    if !sidecar.exists() {
        return ArtState::MissingSidecar(sidecar);
    }
    ArtState::Bundled
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keywords() -> Vec<String> {
        ["deer", "reindeer", "elk", "steppe runner", "hare"]
            .iter()
            .map(|key| key.to_string())
            .collect()
    }

    #[test]
    fn longest_keyword_wins() {
        assert_eq!(species_key_for("Wild Reindeer", &keywords()), "reindeer");
        assert_eq!(species_key_for("Red Deer", &keywords()), "deer");
        assert_eq!(
            species_key_for("Steppe Runners", &keywords()),
            "steppe runner"
        );
    }

    #[test]
    fn unknown_species_resolves_to_nothing() {
        assert!(species_key_for("Thunder Mammoths", &keywords()).is_empty());
    }

    /// The sidecar is a SIBLING of the whole file name, not a re-extension of it — `with_extension`
    /// here would look for `elk.import` and report every species broken.
    #[test]
    fn sidecar_suffix_is_appended_not_substituted() {
        assert_eq!(
            import_sidecar_for(Path::new("assets/icons/fauna/elk.png")),
            PathBuf::from("assets/icons/fauna/elk.png.import")
        );
    }

    /// A PNG staged without its sidecar is the failure this guard was extended to catch: Godot loads
    /// the imported texture the sidecar names, so the art is unreachable and the marker draws an
    /// emoji, while a PNG-only stat calls it present.
    #[test]
    fn png_without_its_sidecar_is_its_own_failure() {
        let dir = std::env::temp_dir().join("fauna_icon_guard_sidecar_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        let png = dir.join("elk.png");

        assert_eq!(art_state(&png), ArtState::MissingPng);

        fs::write(&png, []).expect("write png");
        assert_eq!(
            art_state(&png),
            ArtState::MissingSidecar(import_sidecar_for(&png))
        );

        fs::write(import_sidecar_for(&png), []).expect("write sidecar");
        assert_eq!(art_state(&png), ArtState::Bundled);

        fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn dict_parse_skips_comments_and_requires_a_close() {
        let text = "const D := {\n\t# a comment\n\t\"a\": 1,\n\n\t\"b\": 2,\n}\n";
        let body = dict_lines(text, "D", Path::new("test.gd")).expect("closed dict parses");
        assert_eq!(body, vec!["\"a\": 1,", "\"b\": 2,"]);
        assert!(dict_lines("const D := {\n\t\"a\": 1,\n", "D", Path::new("test.gd")).is_err());
    }
}
