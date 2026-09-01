//! **Save slots on disk**: where they live, what a slot may be called, and how a listing stays
//! cheap.
//!
//! The format itself is [`crate::save`]; this module only decides files. The split matters for one
//! reason in particular — [`list_slots`] reads each file's **header** and stops. That is what the
//! format's uncompressed-header-first layout is *for*: measured on a 160x104 world, reading a header
//! is 0.005 ms against 63 ms to decode the payload, so a listing that decoded worlds would be four
//! orders of magnitude more expensive and would scale with the number of slots.

use std::fs;
use std::io;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use sim_runtime::commands::SaveSlotInfo;
use thiserror::Error;

use crate::save::{read_save_header, SaveError, SaveHeader};

/// Where saves live. Overrides [`DEFAULT_SAVE_DIR`]; the same environment-override idiom as
/// `SIM_PORT_BASE` and the `*_CONFIG_PATH` family (`core_sim/CLAUDE.md`).
pub const SAVE_DIR_ENV: &str = "SIM_SAVE_DIR";

/// Relative to the server's working directory, so a dev checkout and a shipped install both get a
/// sensible place without configuration.
pub const DEFAULT_SAVE_DIR: &str = "saves";

/// Extension of a save file. Distinct enough that a directory listing says what these are.
pub const SAVE_FILE_EXTENSION: &str = "shdw";

/// How much of a save file [`list_slots`] reads to get at its header.
///
/// The whole point of the uncompressed-header-first layout is that a listing costs a header rather
/// than a world, and a listing that read the file whole threw that away: at 160x104 a slot is
/// ~1.2 MB, so ten slots cost ~12 MB of I/O on every pane open, every save and every delete.
///
/// **8 KiB, against a measured 1,313-byte header** on the shipped config catalog
/// (`save_round_trip.rs` -> `a_large_map_save_is_measured`, which asserts the fit). The header does
/// not grow with the map -- the only part of it that grows at all is the [`crate::save::SaveHeader`]
/// `config_fingerprint`, one `(file name, u64)` entry per boot config at roughly 30 bytes -- so the
/// headroom is worth about 200 more config files.
///
/// A header that outgrew this is **not** treated as a corrupt file: see [`read_header_only`].
pub const HEADER_PREFIX_BYTES: u64 = 8 * 1024;

/// The longest slot name accepted.
///
/// A bound rather than a taste: the name becomes a path component, and filesystems have their own
/// limits that differ by platform. Refusing early gives one predictable answer everywhere instead of
/// an `io::Error` that varies.
pub const MAX_SLOT_NAME_LEN: usize = 64;

/// Why a slot name was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SlotNameError {
    #[error("a slot name cannot be empty")]
    Empty,
    #[error("a slot name may be at most {MAX_SLOT_NAME_LEN} characters")]
    TooLong,
    #[error(
        "a slot name may contain only letters, digits, spaces, '-' and '_' — it becomes a filename"
    )]
    IllegalCharacter,
}

/// **Is this a name we are willing to turn into a filename?**
///
/// A whitelist, not a blacklist of the traversal sequences anyone happened to think of. `..`, `/`,
/// `\`, a leading `~`, a drive letter, a NUL and every control character are all refused by the same
/// rule — *this character is not a letter, a digit, a space, `-` or `_`* — rather than by a list that
/// has to stay ahead of the ways a path can be spelled on three platforms. A slot name arrives over
/// the wire from a text field, so the question is not whether *this* client is careful.
///
/// It is still free-form enough for a player-typed name: "Second Age 12", "before_the_flood".
pub fn validate_slot_name(slot: &str) -> Result<(), SlotNameError> {
    if slot.is_empty() {
        return Err(SlotNameError::Empty);
    }
    if slot.chars().count() > MAX_SLOT_NAME_LEN {
        return Err(SlotNameError::TooLong);
    }
    if !slot
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == '_')
    {
        return Err(SlotNameError::IllegalCharacter);
    }
    Ok(())
}

/// The directory saves live in, honouring [`SAVE_DIR_ENV`].
pub fn save_dir() -> PathBuf {
    std::env::var_os(SAVE_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SAVE_DIR))
}

/// The file a slot name maps to, once the name has been vetted.
pub fn slot_path(dir: &Path, slot: &str) -> Result<PathBuf, SlotNameError> {
    validate_slot_name(slot)?;
    Ok(dir.join(format!("{slot}.{SAVE_FILE_EXTENSION}")))
}

/// Write a blob to a slot, creating the directory if it is not there.
///
/// **Written to a temporary file and renamed**, so a crash or a full disk mid-write leaves the
/// previous save intact rather than a truncated one. The autosave hook rewrites the same slot on a
/// cadence, which makes "the file you are overwriting is the only copy" the normal case rather than
/// the unlucky one.
pub fn write_slot(dir: &Path, slot: &str, bytes: &[u8]) -> Result<PathBuf, SlotStoreError> {
    let path = slot_path(dir, slot)?;
    fs::create_dir_all(dir).map_err(SlotStoreError::Io)?;
    let temp = path.with_extension(format!("{SAVE_FILE_EXTENSION}.partial"));
    fs::write(&temp, bytes).map_err(SlotStoreError::Io)?;
    fs::rename(&temp, &path).map_err(SlotStoreError::Io)?;
    Ok(path)
}

/// Read a slot's bytes.
pub fn read_slot(dir: &Path, slot: &str) -> Result<Vec<u8>, SlotStoreError> {
    let path = slot_path(dir, slot)?;
    match fs::read(&path) {
        Ok(bytes) => Ok(bytes),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Err(SlotStoreError::NotFound),
        Err(err) => Err(SlotStoreError::Io(err)),
    }
}

/// Delete a slot.
pub fn delete_slot(dir: &Path, slot: &str) -> Result<(), SlotStoreError> {
    let path = slot_path(dir, slot)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Err(SlotStoreError::NotFound),
        Err(err) => Err(SlotStoreError::Io(err)),
    }
}

/// Whatever went wrong reaching a slot.
#[derive(Debug, Error)]
pub enum SlotStoreError {
    #[error(transparent)]
    Name(#[from] SlotNameError),
    #[error("no save is stored under that slot")]
    NotFound,
    #[error("the save could not be read or written: {0}")]
    Io(#[source] io::Error),
    #[error(transparent)]
    Format(#[from] SaveError),
}

/// **Every readable slot, newest first — from headers alone.**
///
/// A file that is not a save, or is a save this build cannot read, is **skipped with a warning**
/// rather than failing the listing: one corrupt file in the directory must not make the load menu
/// unopenable, which is precisely when a player needs it.
///
/// Each file costs one open, one stat and a [`HEADER_PREFIX_BYTES`] read -- never the payload.
pub fn list_slots(dir: &Path) -> Vec<SaveSlotInfo> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        // No directory yet simply means no saves yet — the first save creates it.
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Vec::new(),
        Err(err) => {
            tracing::warn!(
                target: "shadow_scale::save",
                dir = %dir.display(),
                error = %err,
                "save.list.unreadable_dir"
            );
            return Vec::new();
        }
    };

    let mut slots: Vec<SaveSlotInfo> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some(SAVE_FILE_EXTENSION) {
            continue;
        }
        let Some(slot) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        // A file whose name we would refuse to write is a file we do not offer to load.
        if validate_slot_name(slot).is_err() {
            continue;
        }

        // ONLY the header, and only a bounded prefix of the file to get it.
        let (header, metadata) = match read_header_only(&path) {
            Ok(read) => read,
            Err(SlotStoreError::Io(err)) => {
                tracing::warn!(
                    target: "shadow_scale::save",
                    path = %path.display(),
                    error = %err,
                    "save.list.unreadable_file"
                );
                continue;
            }
            Err(err) => {
                tracing::warn!(
                    target: "shadow_scale::save",
                    path = %path.display(),
                    error = %err,
                    "save.list.skipped"
                );
                continue;
            }
        };

        let modified_unix_seconds = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|since| since.as_secs())
            .unwrap_or_default();

        slots.push(SaveSlotInfo {
            slot: slot.to_string(),
            turn: header.turn,
            campaign_title: header.campaign_title,
            map_preset_id: header.world.map_preset_id,
            width: header.world.width,
            height: header.world.height,
            world_seed: header.world.world_seed,
            start_profile_id: header.world.start_profile_id,
            size_bytes: metadata.len(),
            modified_unix_seconds,
        });
    }

    // Newest first, then by name so the order is total — two saves written in the same second must
    // not swap places between two listings of the same directory.
    slots.sort_by(|a, b| {
        b.modified_unix_seconds
            .cmp(&a.modified_unix_seconds)
            .then_with(|| a.slot.cmp(&b.slot))
    });
    slots
}

/// **Read one slot's header without reading its world.**
///
/// The bounded read is the whole point of the layout ([`HEADER_PREFIX_BYTES`]); the metadata comes
/// off the same open handle, so `size_bytes` is the file's true length rather than the length of
/// what was read.
///
/// **A header bigger than the bound is a bound that is wrong, not a save that is broken.** Truncating
/// a CBOR document mid-way makes `ciborium` fail exactly as a corrupt file does, and a listing that
/// believed it would drop a perfectly good slot off the load menu -- silently, and every slot at
/// once, since every save's header is about the same size. So a decode failure on a prefix that was
/// *cut short* re-reads that one file whole and warns naming the bound: one full read in a case that
/// does not currently arise, and no way to lose a save to a constant.
fn read_header_only(path: &Path) -> Result<(SaveHeader, fs::Metadata), SlotStoreError> {
    let file = fs::File::open(path).map_err(SlotStoreError::Io)?;
    let metadata = file.metadata().map_err(SlotStoreError::Io)?;
    let mut prefix = Vec::new();
    file.take(HEADER_PREFIX_BYTES)
        .read_to_end(&mut prefix)
        .map_err(SlotStoreError::Io)?;

    match read_save_header(&prefix) {
        Ok(header) => Ok((header, metadata)),
        // Only a decode error can be truncation: a bad magic or a version mismatch is answered by
        // the leading bytes and would read the same from the whole file.
        Err(err @ SaveError::Header(_)) if metadata.len() > HEADER_PREFIX_BYTES => {
            tracing::warn!(
                target: "shadow_scale::save",
                path = %path.display(),
                bound = HEADER_PREFIX_BYTES,
                error = %err,
                "save.list.header_over_prefix -- re-reading the file whole; raise HEADER_PREFIX_BYTES"
            );
            let bytes = fs::read(path).map_err(SlotStoreError::Io)?;
            let header = read_save_header(&bytes)?;
            Ok((header, metadata))
        }
        Err(err) => Err(SlotStoreError::Format(err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(case: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "shadow_scale_save_store_{}_{case}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn a_player_typed_name_is_accepted() {
        for name in ["autosave", "Second Age 12", "before_the_flood", "quick-1"] {
            assert!(validate_slot_name(name).is_ok(), "{name} should be legal");
        }
    }

    /// **Traversal is refused by the whitelist**, not by a list of spellings. Each of these would
    /// otherwise reach outside the save directory, and the point is that they all fail for the same
    /// one reason rather than each needing to have been anticipated.
    #[test]
    fn a_name_that_could_escape_the_directory_is_refused() {
        for name in [
            "..",
            "../etc/passwd",
            "..\\windows",
            "/etc/passwd",
            "a/b",
            "a\\b",
            "~root",
            "C:file",
            ".hidden",
            "nul\0byte",
            "new\nline",
        ] {
            assert_eq!(
                validate_slot_name(name),
                Err(SlotNameError::IllegalCharacter),
                "{name:?} must be refused"
            );
        }
        assert_eq!(validate_slot_name(""), Err(SlotNameError::Empty));
        assert_eq!(
            validate_slot_name(&"x".repeat(MAX_SLOT_NAME_LEN + 1)),
            Err(SlotNameError::TooLong)
        );
    }

    /// And the refusal happens **before** any path is built, so there is no window in which a
    /// traversing name exists as a `PathBuf` that something else might use.
    #[test]
    fn a_refused_name_never_becomes_a_path() {
        let dir = scratch("no_path");
        assert!(slot_path(&dir, "../escape").is_err());
        assert!(write_slot(&dir, "../escape", b"bytes").is_err());
        assert!(read_slot(&dir, "../escape").is_err());
        assert!(delete_slot(&dir, "../escape").is_err());
    }

    #[test]
    fn a_missing_slot_reads_as_not_found_rather_than_an_io_error() {
        let dir = scratch("missing");
        assert!(matches!(
            read_slot(&dir, "nothing_here"),
            Err(SlotStoreError::NotFound)
        ));
        assert!(matches!(
            delete_slot(&dir, "nothing_here"),
            Err(SlotStoreError::NotFound)
        ));
    }

    #[test]
    fn writing_then_deleting_a_slot_round_trips() {
        let dir = scratch("round_trip");
        write_slot(&dir, "slot one", b"some bytes").expect("write");
        assert_eq!(read_slot(&dir, "slot one").expect("read"), b"some bytes");
        delete_slot(&dir, "slot one").expect("delete");
        assert!(matches!(
            read_slot(&dir, "slot one"),
            Err(SlotStoreError::NotFound)
        ));
    }

    /// A directory holding junk still lists — one unreadable file must not close the load menu.
    #[test]
    fn a_file_that_is_not_a_save_is_skipped_rather_than_fatal() {
        let dir = scratch("junk");
        write_slot(&dir, "broken", b"not a save at all").expect("write");
        fs::write(dir.join("notes.txt"), "ignore me").expect("write");
        assert!(list_slots(&dir).is_empty());
    }

    /// **A listing costs a prefix, and still reports the whole file.**
    ///
    /// A real save, because the two things worth asserting only exist on one: that a file far larger
    /// than [`HEADER_PREFIX_BYTES`] lists from that prefix alone, and that `size_bytes` is the
    /// **file's** length. The listing used to read the file whole and could therefore fall back on
    /// the buffer's length for the size; a bounded read makes that fallback a lie, so the size now
    /// comes off the same open handle the prefix was read from.
    #[test]
    fn a_listing_reads_a_prefix_and_still_reports_the_whole_file() {
        let dir = scratch("prefix");
        let mut app = crate::build_test_app();
        app.update();
        let blob = crate::save::encode_save(&app.world).expect("the world encodes");
        let path = write_slot(&dir, "one world", &blob).expect("write");

        let on_disk = fs::metadata(&path).expect("stat").len();
        assert!(
            on_disk > HEADER_PREFIX_BYTES,
            "the fixture must be bigger than the prefix or it proves nothing: {on_disk} bytes"
        );

        let slots = list_slots(&dir);
        assert_eq!(slots.len(), 1, "the slot must list from its header alone");
        assert_eq!(slots[0].slot, "one world");
        assert_eq!(
            slots[0].size_bytes, on_disk,
            "a row reports the size of the FILE, not of the prefix that was read"
        );
    }

    #[test]
    fn listing_a_directory_that_does_not_exist_is_empty_not_an_error() {
        let dir = scratch("absent").join("nested_and_absent");
        assert!(list_slots(&dir).is_empty());
    }
}
