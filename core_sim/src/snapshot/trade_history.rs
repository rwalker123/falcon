use super::*;

use crate::{
    snapshot::dealings::lines_state,
    trade_history::{TradeHistory, TradeRecord},
};
use sim_runtime::TradeRecordState;

/// **The trade history one viewer's frame carries** (`docs/plan_early_diplomacy.md` §1): the viewer's
/// own people's records, oldest first. Another people's trades are theirs to know. Material rows are
/// unrated lines, the proposal-leg form.
pub(crate) fn trade_history_states(
    history: &TradeHistory,
    viewer: FactionId,
) -> Vec<TradeRecordState> {
    history.of(viewer).iter().map(record_state).collect()
}

fn record_state(record: &TradeRecord) -> TradeRecordState {
    TradeRecordState {
        turn: record.turn,
        own_band: record.own_band.0,
        other_faction: record.other_faction.0,
        other_band: record.other_band.0,
        dealing_id: record.dealing.map_or(0, |id| id.0),
        outcome: record.outcome.as_str().to_string(),
        goods_out: lines_state(&record.goods_out),
        goods_in: lines_state(&record.goods_in),
    }
}
