use super::*;

use crate::{diplomacy_config::DiplomacyConfig, goodwill::KnownGoodwill};

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

/// **The word cuts**, off the config, so the client never restates them.
pub(crate) fn goodwill_cuts(config: &DiplomacyConfig) -> GoodwillCutsState {
    GoodwillCutsState {
        wary_below: config.goodwill.wary_below,
        friendly_above: config.goodwill.friendly_above,
    }
}
