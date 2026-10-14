//! **The ladder's identities are CONFIG STRINGS, interned** (`docs/plan_intensification_ladder.md`
//! §5: *"the ladder is data over a bounded set of coded primitives"*).
//!
//! A branch, a rung and a verb are named by `intensification_ladder.json`, and a new branch must be
//! addable by a JSON edit alone, so none of them can be a Rust enum variant. They are instead three
//! thin newtypes over one **global, append-only, thread-safe interner**:
//!
//! | type | is | wire spelling |
//! |---|---|---|
//! | [`Sym`] | one interned string, `Copy`, a `u32` | the string |
//! | [`BranchId`] | a branch (`plant`, `animal`, `route`, …) | `plant` |
//! | [`VerbId`] | a build verb (`cultivate`, `grade`, …) | `cultivate` |
//! | [`RungId`] | a rung: a branch and an id within it | `plant:tended` |
//!
//! # ⛔ NEVER ITERATE IN `Sym` ORDER
//!
//! A `Sym`'s number is the order its string was first interned, which depends on which test, thread
//! or load path touched it first. Nothing deterministic may depend on that, so **every `Ord` here
//! compares the strings** — a `BTreeMap<BranchId, _>` iterates alphabetically, and the ladder's own
//! ordering is the config's `order` field. The `Hash` impls are on the number (a `HashMap` iterates
//! in a randomised order regardless), so a `HashMap` keyed on these is for lookup only.
//!
//! # ⛔ SAVES CARRY NAMES, NEVER NUMBERS
//!
//! A `Sym`'s number means nothing outside this process. [`Serialize`]/[`Deserialize`] therefore
//! write and re-intern the **string**, which is what lets a checkpoint or a save round-trip across
//! processes whose interners were filled in different orders.
//!
//! # The `shipped` module is test support
//!
//! [`shipped`] names the rungs the shipped `intensification_ladder.json` defines. It exists so the
//! tests can say `shipped::plant_tended()` rather than carry a string, and **no non-test source file
//! may use it** — production code reads the ladder's properties, never an identity
//! (`docs/plan_intensification_ladder.md` §5).

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// **The separator between a branch and a rung id in a rung's wire spelling** — `plant:tended`.
pub const RUNG_KEY_SEPARATOR: char = ':';

/// **The global string table**: names in first-seen order, and the reverse lookup. Append-only, so
/// a `&'static str` handed out of it is valid for the rest of the process (each name is leaked once,
/// and the table holds a few dozen).
#[derive(Default)]
struct Interner {
    by_name: HashMap<&'static str, u32>,
    names: Vec<&'static str>,
}

fn interner() -> &'static RwLock<Interner> {
    static INTERNER: OnceLock<RwLock<Interner>> = OnceLock::new();
    INTERNER.get_or_init(|| RwLock::new(Interner::default()))
}

/// **One interned string.** `Copy`, four bytes, compared by number for equality and by
/// **string** for order — see the module doc.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Sym(u32);

impl Sym {
    /// Intern `name`, returning the existing symbol if it was seen before.
    pub fn intern(name: &str) -> Sym {
        {
            let table = interner().read().expect("ladder interner poisoned");
            if let Some(&index) = table.by_name.get(name) {
                return Sym(index);
            }
        }
        let mut table = interner().write().expect("ladder interner poisoned");
        // Another thread may have interned it between the read lock dropping and this write lock.
        if let Some(&index) = table.by_name.get(name) {
            return Sym(index);
        }
        let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
        let index = u32::try_from(table.names.len()).expect("fewer than 2^32 ladder names");
        table.names.push(leaked);
        table.by_name.insert(leaked, index);
        Sym(index)
    }

    /// The string this symbol names.
    pub fn as_str(self) -> &'static str {
        let table = interner().read().expect("ladder interner poisoned");
        table.names[self.0 as usize]
    }
}

impl PartialOrd for Sym {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Sym {
    /// **By string, never by number** — the interning order is not deterministic.
    fn cmp(&self, other: &Self) -> Ordering {
        if self.0 == other.0 {
            return Ordering::Equal;
        }
        self.as_str().cmp(other.as_str())
    }
}

impl fmt::Debug for Sym {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.as_str())
    }
}

impl fmt::Display for Sym {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for Sym {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Sym {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Ok(Sym::intern(&name))
    }
}

/// Newtype boilerplate shared by [`BranchId`] and [`VerbId`]: both are one interned string.
macro_rules! symbol_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Sym);

        impl $name {
            /// Intern `name` as this kind of id.
            pub fn new(name: &str) -> Self {
                Self(Sym::intern(name))
            }

            /// The string this id names.
            pub fn as_str(self) -> &'static str {
                self.0.as_str()
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({:?})", stringify!($name), self.as_str())
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl From<&str> for $name {
            fn from(name: &str) -> Self {
                Self::new(name)
            }
        }
    };
}

symbol_newtype!(
    /// **A BRANCH OF THE LADDER** — `plant`, `animal`, `route`, `forestry`, `extraction`, and
    /// whatever the config adds next. Its [`crate::intensification::SourceKind`] says what kind of
    /// thing stands on it.
    BranchId
);

symbol_newtype!(
    /// **A BUILD VERB** — `cultivate`, `sow`, `grade`, …. Verbs are globally unique across the
    /// ladder: one verb builds exactly one rung.
    VerbId
);

/// **ONE RUNG OF ONE BRANCH** — `plant:tended`. The wire and every validation message have always
/// spelled a rung `branch:id`, and this serialises as exactly that string.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RungId {
    /// The branch the rung sits on.
    pub branch: BranchId,
    /// The rung's id within its branch (`tended`).
    pub id: Sym,
}

impl RungId {
    /// A rung of `branch` named `id`.
    pub fn new(branch: BranchId, id: &str) -> Self {
        Self {
            branch,
            id: Sym::intern(id),
        }
    }

    /// **The rung's name on the wire** — `"<branch>:<id>"`.
    pub fn wire_key(self) -> String {
        format!(
            "{}{}{}",
            self.branch.as_str(),
            RUNG_KEY_SEPARATOR,
            self.id.as_str()
        )
    }

    /// The rung's id within its branch, as a string.
    pub fn id_str(self) -> &'static str {
        self.id.as_str()
    }
}

impl fmt::Debug for RungId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RungId({})", self.wire_key())
    }
}

impl fmt::Display for RungId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.wire_key())
    }
}

/// A rung token that is not `branch:id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RungIdParseError(pub String);

impl fmt::Display for RungIdParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}` is not a rung id — expected `<branch>{}<id>`",
            self.0, RUNG_KEY_SEPARATOR
        )
    }
}

impl std::error::Error for RungIdParseError {}

impl FromStr for RungId {
    type Err = RungIdParseError;

    /// Parse a `branch:id` token. Splits on the **first** separator and refuses an empty half, so a
    /// stray `:` never mints a rung named the empty string.
    fn from_str(token: &str) -> Result<Self, Self::Err> {
        match token.split_once(RUNG_KEY_SEPARATOR) {
            Some((branch, id)) if !branch.is_empty() && !id.is_empty() => Ok(RungId {
                branch: BranchId::new(branch),
                id: Sym::intern(id),
            }),
            _ => Err(RungIdParseError(token.to_string())),
        }
    }
}

impl Serialize for RungId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.wire_key())
    }
}

impl<'de> Deserialize<'de> for RungId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let token = String::deserialize(deserializer)?;
        token.parse().map_err(serde::de::Error::custom)
    }
}

/// **THE RUNGS AND BRANCHES THE SHIPPED `intensification_ladder.json` DEFINES** — test support.
///
/// ⛔ **NO NON-TEST SOURCE MAY USE THIS MODULE.** Production code that names a shipped rung is code
/// that tests a specific identity, which is exactly what the ladder being data forbids; it reads a
/// property of the rung instead. A guard test pins that `shipped::` appears in no non-test `src`
/// file.
pub mod shipped {
    use super::{BranchId, RungId, VerbId};

    /// The shipped branch ids.
    pub fn plant() -> BranchId {
        BranchId::new("plant")
    }
    pub fn animal() -> BranchId {
        BranchId::new("animal")
    }
    pub fn route() -> BranchId {
        BranchId::new("route")
    }
    pub fn forestry() -> BranchId {
        BranchId::new("forestry")
    }
    pub fn extraction() -> BranchId {
        BranchId::new("extraction")
    }

    /// The shipped rung ids.
    pub fn plant_wild() -> RungId {
        RungId::new(plant(), "wild")
    }
    pub fn plant_tended() -> RungId {
        RungId::new(plant(), "tended")
    }
    pub fn plant_field() -> RungId {
        RungId::new(plant(), "field")
    }
    pub fn animal_wild() -> RungId {
        RungId::new(animal(), "wild")
    }
    pub fn animal_pastoral() -> RungId {
        RungId::new(animal(), "pastoral")
    }
    pub fn animal_pen() -> RungId {
        RungId::new(animal(), "pen")
    }
    pub fn route_path() -> RungId {
        RungId::new(route(), "path")
    }
    pub fn route_trail() -> RungId {
        RungId::new(route(), "trail")
    }
    pub fn route_dirt_road() -> RungId {
        RungId::new(route(), "dirt_road")
    }
    pub fn route_paved_road() -> RungId {
        RungId::new(route(), "paved_road")
    }
    pub fn forestry_deadfall() -> RungId {
        RungId::new(forestry(), "deadfall")
    }
    pub fn forestry_felling() -> RungId {
        RungId::new(forestry(), "felling")
    }
    pub fn forestry_coppice() -> RungId {
        RungId::new(forestry(), "coppice")
    }
    pub fn extraction_gathering() -> RungId {
        RungId::new(extraction(), "gathering")
    }
    pub fn extraction_quarry() -> RungId {
        RungId::new(extraction(), "quarry")
    }

    /// The shipped verb ids.
    pub fn cultivate() -> VerbId {
        VerbId::new("cultivate")
    }
    pub fn sow() -> VerbId {
        VerbId::new("sow")
    }
    pub fn tame() -> VerbId {
        VerbId::new("tame")
    }
    pub fn corral() -> VerbId {
        VerbId::new("corral")
    }
    pub fn grade() -> VerbId {
        VerbId::new("grade")
    }
    pub fn pave() -> VerbId {
        VerbId::new("pave")
    }
    pub fn fell() -> VerbId {
        VerbId::new("fell")
    }
    pub fn coppice() -> VerbId {
        VerbId::new("coppice")
    }
    pub fn quarry() -> VerbId {
        VerbId::new("quarry")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interning_the_same_name_twice_is_the_same_symbol() {
        assert_eq!(Sym::intern("plant"), Sym::intern("plant"));
        assert_ne!(Sym::intern("plant"), Sym::intern("animal"));
    }

    #[test]
    fn order_is_by_string_not_by_the_order_names_were_interned() {
        // Intern the later name first, so number order and string order disagree.
        let zebra = BranchId::new("zz_ids_test_zebra");
        let aardvark = BranchId::new("aa_ids_test_aardvark");
        assert!(aardvark < zebra);
        let mut sorted = vec![zebra, aardvark];
        sorted.sort();
        assert_eq!(sorted, vec![aardvark, zebra]);
    }

    #[test]
    fn a_rung_id_round_trips_as_its_wire_key() {
        let rung = RungId::new(BranchId::new("plant"), "tended");
        assert_eq!(rung.wire_key(), "plant:tended");
        assert_eq!("plant:tended".parse::<RungId>().unwrap(), rung);
        let json = serde_json::to_string(&rung).unwrap();
        assert_eq!(json, "\"plant:tended\"");
        assert_eq!(serde_json::from_str::<RungId>(&json).unwrap(), rung);
    }

    #[test]
    fn a_malformed_rung_token_is_refused() {
        for bad in ["", "plant", ":tended", "plant:"] {
            assert!(bad.parse::<RungId>().is_err(), "`{bad}` must not parse");
        }
    }

    #[test]
    fn a_branch_and_a_verb_serialise_as_bare_strings() {
        let branch = BranchId::new("route");
        assert_eq!(serde_json::to_string(&branch).unwrap(), "\"route\"");
        let verb: VerbId = serde_json::from_str("\"grade\"").unwrap();
        assert_eq!(verb, VerbId::new("grade"));
    }

    #[test]
    fn interning_from_many_threads_agrees_on_one_symbol_per_name() {
        let handles: Vec<_> = (0..8)
            .map(|_| std::thread::spawn(|| Sym::intern("ids_test_contended")))
            .collect();
        let symbols: Vec<Sym> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert!(symbols.windows(2).all(|pair| pair[0] == pair[1]));
    }
}
