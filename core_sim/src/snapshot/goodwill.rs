use super::*;

use crate::{
    diplomacy_config::DiplomacyConfig,
    goodwill::{Goodwill, KnownGoodwill},
};

/// **What other peoples are known to think of the viewer's** (`docs/plan_early_diplomacy.md` §2):
/// the viewer's own rows of the carried-news ledger, one per other people, in that people's id
/// order. Each row is the opinion as its reasons plus their sum, and the turn it was observed —
/// never the live opinion, which the other people's own frame alone would know.
pub(crate) fn goodwill_states(known: &KnownGoodwill, viewer: FactionId) -> Vec<GoodwillState> {
    known
        .for_viewer(viewer)
        .map(|(other, news)| GoodwillState {
            other_faction: other.0,
            gifts: news.components.gifts,
            exchanges: news.components.exchanges,
            stationed: news.components.stationed,
            link: news.components.link,
            treaty: news.components.treaty,
            breach: news.components.breach,
            total: news.components.total(),
            reported_turn: news.turn,
        })
        .collect()
}

/// **What the viewer's own people think of each other people, right now**: every opinion the
/// viewer's people hold, one row per other people in that people's id order, with `reported_turn` the
/// current turn (it is live, not carried). The viewer's opinion is its own to know; no other people's
/// opinion is ever published here.
pub(crate) fn own_goodwill_states(
    goodwill: &Goodwill,
    viewer: FactionId,
    tick: u64,
) -> Vec<GoodwillState> {
    goodwill
        .iter()
        .filter(|((holder, _), _)| *holder == viewer)
        .map(|((_, about), entry)| GoodwillState {
            other_faction: about.0,
            gifts: entry.gifts,
            exchanges: entry.exchanges,
            stationed: entry.stationed,
            link: entry.link,
            treaty: entry.treaty,
            breach: entry.breach,
            total: entry.total(),
            reported_turn: tick,
        })
        .collect()
}

/// **The word cuts**, off the config, so the client never restates them.
pub(crate) fn goodwill_cuts(config: &DiplomacyConfig) -> GoodwillCutsState {
    GoodwillCutsState {
        wary_below: config.goodwill.wary_below,
        friendly_above: config.goodwill.friendly_above,
    }
}
