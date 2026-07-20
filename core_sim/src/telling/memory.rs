//! **Memory threads** — the concept doc's memory ledger (`docs/Emergent Narrative.md` §5).
//!
//! A small set of durable threads (a valley you refused, a herd you finished) that later beats can
//! call back to. *Callbacks are what make a 200-turn emergent game feel authored.*
//!
//! Two properties carry the whole idea, and neither is an optimisation detail:
//!
//! 1. **A thread snapshots its noun at first sight and never re-resolves it.** The point is that
//!    the story remembers a thing that may no longer exist — the herd went extinct, the site is
//!    four hundred turns behind you. Re-resolving would make a callback silently vanish exactly
//!    when it would land hardest.
//! 2. **Eviction is by least-recently-*referenced*, not oldest-first-seen.** A thread the story
//!    keeps returning to is the one worth keeping; the one nothing has called back to in two
//!    hundred turns is the one to drop.
//!
//! The `kind` is **free-form and content-defined** (`remembers[].kind` in the catalog). Resolver
//! registration is generic over the kinds the catalog declares, so a modder adding a thread kind
//! needs no engine change — the boundary `docs/plan_the_telling.md` §1b draws.

use std::collections::BTreeMap;

use super::nouns::Noun;

/// One remembered noun, snapshotted at first sight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thread {
    /// Content-defined family (`"place"`, `"beast"`, …). Never an engine enum.
    pub kind: String,
    /// Dedupe identity: the resolved noun's `name`. Rediscovering the same site must not create a
    /// second thread.
    pub key: String,
    /// **The noun record as it read when the thread was first written**, never re-resolved.
    pub name: String,
    pub plural: String,
    pub adjective: String,
    pub first_seen_tick: u64,
    /// The last tick a resolver drew this thread into a beat that landed. Backs eviction.
    pub last_referenced_tick: u64,
}

impl Thread {
    /// The remembered noun, rendered from the snapshot rather than from the live world.
    pub fn noun(&self) -> Noun {
        Noun::named(
            self.name.clone(),
            self.plural.clone(),
            self.adjective.clone(),
        )
    }
}

/// Which thread of a kind a `thread.<kind>.<selector>` resolver picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadSelector {
    /// Earliest `first_seen_tick` — the "we were different then" callback.
    Oldest,
    /// Latest `first_seen_tick`.
    Recent,
}

impl ThreadSelector {
    pub fn as_str(self) -> &'static str {
        match self {
            ThreadSelector::Oldest => "oldest",
            ThreadSelector::Recent => "recent",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        match key {
            "oldest" => Some(ThreadSelector::Oldest),
            "recent" => Some(ThreadSelector::Recent),
            _ => None,
        }
    }
}

/// Every selector a thread resolver may name.
pub const THREAD_SELECTORS: [ThreadSelector; 2] = [ThreadSelector::Oldest, ThreadSelector::Recent];

/// Namespace prefix for the thread noun-resolver family.
pub const THREAD_RESOLVER_PREFIX: &str = "thread.";

/// Parse `thread.<kind>.<selector>` into its parts. `None` for anything else — including a known
/// prefix with an unknown selector, which then fails the catalog's resolver check like any typo.
pub fn parse_thread_resolver(resolver: &str) -> Option<(&str, ThreadSelector)> {
    let rest = resolver.strip_prefix(THREAD_RESOLVER_PREFIX)?;
    // `rsplit_once`, not `split_once`: the selector is the last segment, so a kind is free to
    // contain a dot if content wants one.
    let (kind, selector) = rest.rsplit_once('.')?;
    if kind.is_empty() {
        return None;
    }
    ThreadSelector::from_key(selector).map(|selector| (kind, selector))
}

/// The thread a `thread.<kind>.<selector>` resolver picks, or `None` when the kind is empty
/// (normal early-game — the existing machinery already handles an unresolved slot: a wardrobe
/// entry requiring it is excluded, and a `fallback` chain moves on).
///
/// Ties break by `key` ascending, so the pick is independent of insertion order.
pub fn select_thread<'a>(
    threads: &'a BTreeMap<String, Vec<Thread>>,
    kind: &str,
    selector: ThreadSelector,
) -> Option<&'a Thread> {
    let of_kind = threads.get(kind)?;
    match selector {
        ThreadSelector::Oldest => of_kind.iter().min_by(|a, b| {
            a.first_seen_tick
                .cmp(&b.first_seen_tick)
                .then_with(|| a.key.cmp(&b.key))
        }),
        ThreadSelector::Recent => of_kind.iter().max_by(|a, b| {
            a.first_seen_tick
                .cmp(&b.first_seen_tick)
                // `max_by` keeps the *last* maximum, so invert the key ordering to land on the
                // smallest key among ties.
                .then_with(|| b.key.cmp(&a.key))
        }),
    }
}

/// Resolve a thread resolver straight to its remembered noun.
pub fn resolve_thread_noun(
    threads: &BTreeMap<String, Vec<Thread>>,
    resolver: &str,
) -> Option<Noun> {
    let (kind, selector) = parse_thread_resolver(resolver)?;
    select_thread(threads, kind, selector).map(Thread::noun)
}

/// Write a thread: **upsert by key**, never push. Rediscovering the same site updates the existing
/// thread's `last_referenced_tick` and leaves its snapshotted noun and `first_seen_tick` alone.
///
/// When the kind is full, evict the **least recently referenced** thread (ties by `key` ascending,
/// for determinism) before inserting.
pub fn remember(
    threads: &mut BTreeMap<String, Vec<Thread>>,
    kind: &str,
    noun: &Noun,
    tick: u64,
    max_per_kind: usize,
) {
    // Only a named noun has the word forms a callback needs; a bare scalar has no identity to
    // remember, so it is silently not a thread.
    let Noun::Named {
        name,
        plural,
        adjective,
    } = noun
    else {
        return;
    };

    let of_kind = threads.entry(kind.to_string()).or_default();
    if let Some(existing) = of_kind.iter_mut().find(|thread| &thread.key == name) {
        existing.last_referenced_tick = tick;
        return;
    }

    // Evict down to `max_per_kind - 1` so the new thread fits. A cap of 0 disables the kind
    // entirely rather than looping.
    if max_per_kind == 0 {
        return;
    }
    while of_kind.len() >= max_per_kind {
        let Some(victim) = of_kind
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                a.last_referenced_tick
                    .cmp(&b.last_referenced_tick)
                    .then_with(|| a.key.cmp(&b.key))
            })
            .map(|(index, _)| index)
        else {
            break;
        };
        of_kind.remove(victim);
    }

    of_kind.push(Thread {
        kind: kind.to_string(),
        key: name.clone(),
        name: name.clone(),
        plural: plural.clone(),
        adjective: adjective.clone(),
        first_seen_tick: tick,
        last_referenced_tick: tick,
    });
}

/// Mark a thread as referenced this turn (the eviction clock). No-op for a thread that has since
/// been evicted, which is why this takes the key rather than a borrow.
pub fn touch(threads: &mut BTreeMap<String, Vec<Thread>>, kind: &str, key: &str, tick: u64) {
    if let Some(thread) = threads
        .get_mut(kind)
        .and_then(|of_kind| of_kind.iter_mut().find(|thread| thread.key == key))
    {
        thread.last_referenced_tick = tick;
    }
}

/// Does the kind hold at least `min_count` threads first seen at least `older_than_turns` ago?
/// The `{ "thread": K, … }` predicate's whole implementation.
pub fn count_matching(
    threads: &BTreeMap<String, Vec<Thread>>,
    kind: &str,
    older_than_turns: u32,
    tick: u64,
) -> usize {
    threads
        .get(kind)
        .map(|of_kind| {
            of_kind
                .iter()
                .filter(|thread| {
                    tick.saturating_sub(thread.first_seen_tick) >= older_than_turns as u64
                })
                .count()
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noun(name: &str) -> Noun {
        Noun::named(name, format!("{name}s"), name)
    }

    fn store() -> BTreeMap<String, Vec<Thread>> {
        BTreeMap::new()
    }

    #[test]
    fn thread_resolvers_parse_kind_and_selector() {
        assert_eq!(
            parse_thread_resolver("thread.place.oldest"),
            Some(("place", ThreadSelector::Oldest))
        );
        assert_eq!(
            parse_thread_resolver("thread.beast.recent"),
            Some(("beast", ThreadSelector::Recent))
        );
        assert_eq!(parse_thread_resolver("thread.place.middling"), None);
        assert_eq!(parse_thread_resolver("thread.place"), None);
        assert_eq!(parse_thread_resolver("site.last_discovered"), None);
    }

    /// Upsert, not push: the same site found twice is one thread.
    #[test]
    fn remember_dedupes_by_key_and_keeps_the_first_sighting() {
        let mut threads = store();
        remember(&mut threads, "place", &noun("Great Peak"), 3, 8);
        remember(&mut threads, "place", &noun("Great Peak"), 40, 8);
        let of_kind = &threads["place"];
        assert_eq!(
            of_kind.len(),
            1,
            "rediscovery must not create a second thread"
        );
        assert_eq!(of_kind[0].first_seen_tick, 3);
        assert_eq!(of_kind[0].last_referenced_tick, 40);
    }

    /// Eviction drops the least recently **referenced**, not the oldest first-seen: a thread the
    /// story keeps returning to survives even though it was written first.
    #[test]
    fn eviction_drops_the_least_recently_referenced_not_the_oldest() {
        const CAP: usize = 2;
        let mut threads = store();
        remember(&mut threads, "place", &noun("Old Favourite"), 1, CAP);
        remember(&mut threads, "place", &noun("Forgotten"), 2, CAP);
        // The story keeps returning to the *older* thread.
        touch(&mut threads, "place", "Old Favourite", 50);

        remember(&mut threads, "place", &noun("Newcomer"), 60, CAP);
        let keys: Vec<&str> = threads["place"]
            .iter()
            .map(|thread| thread.key.as_str())
            .collect();
        assert_eq!(keys, vec!["Old Favourite", "Newcomer"]);
    }

    #[test]
    fn selectors_pick_oldest_and_recent_with_a_key_tie_break() {
        let mut threads = store();
        remember(&mut threads, "place", &noun("Bravo"), 5, 8);
        remember(&mut threads, "place", &noun("Alpha"), 5, 8);
        remember(&mut threads, "place", &noun("Zulu"), 9, 8);

        // Same `first_seen_tick` => the key decides, both ways.
        assert_eq!(
            select_thread(&threads, "place", ThreadSelector::Oldest)
                .unwrap()
                .key,
            "Alpha"
        );
        assert_eq!(
            select_thread(&threads, "place", ThreadSelector::Recent)
                .unwrap()
                .key,
            "Zulu"
        );
        assert!(select_thread(&threads, "beast", ThreadSelector::Oldest).is_none());
    }

    #[test]
    fn count_matching_applies_the_age_gate() {
        let mut threads = store();
        remember(&mut threads, "place", &noun("Early"), 0, 8);
        remember(&mut threads, "place", &noun("Late"), 90, 8);
        assert_eq!(count_matching(&threads, "place", 0, 100), 2);
        assert_eq!(count_matching(&threads, "place", 25, 100), 1);
        assert_eq!(count_matching(&threads, "place", 200, 100), 0);
        assert_eq!(count_matching(&threads, "beast", 0, 100), 0);
    }

    /// **The whole point of a thread.** A thread snapshots its noun at first sight and is never
    /// re-resolved, so a callback still lands after the underlying source has changed or gone —
    /// the herd went extinct, the site is four hundred turns behind you. Re-resolving would make
    /// callbacks silently vanish exactly when they would land hardest.
    #[test]
    fn a_threads_noun_is_snapshotted_at_first_sight_and_never_re_resolved() {
        let mut threads = store();
        // The "live world" the noun came from.
        let mut source = Noun::named("Ash Elk", "ash elk", "elk");
        remember(&mut threads, "beast", &source, 5, 8);

        // Mutate the source out from under the thread: the species is renamed, and then the herd
        // goes extinct entirely.
        source = Noun::named("Something Else", "others", "other");
        drop(source);

        let remembered = select_thread(&threads, "beast", ThreadSelector::Oldest).unwrap();
        assert_eq!(remembered.name, "Ash Elk");
        assert_eq!(remembered.plural, "ash elk");
        assert_eq!(remembered.adjective, "elk");
        assert_eq!(
            resolve_thread_noun(&threads, "thread.beast.oldest"),
            Some(Noun::named("Ash Elk", "ash elk", "elk")),
            "the resolver must read the snapshot, not the world"
        );

        // Re-remembering the same key with *different* word forms must not rewrite the snapshot
        // either — the thread is the memory of the first sighting.
        remember(
            &mut threads,
            "beast",
            &Noun::named("Ash Elk", "REWRITTEN", "REWRITTEN"),
            80,
            8,
        );
        let remembered = select_thread(&threads, "beast", ThreadSelector::Oldest).unwrap();
        assert_eq!(remembered.plural, "ash elk");
        assert_eq!(remembered.first_seen_tick, 5);
    }

    #[test]
    fn a_scalar_noun_is_not_a_thread() {
        let mut threads = store();
        remember(&mut threads, "count", &Noun::Scalar(31.0), 1, 8);
        assert!(threads.get("count").is_none_or(Vec::is_empty));
    }
}
