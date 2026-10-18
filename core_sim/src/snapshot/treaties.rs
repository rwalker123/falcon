use super::*;

use crate::{
    connections::ConnectionLedger,
    routes::LinkFloor,
    routes::{standing_link, RoadRegistry},
    treaties::{Treaties, TreatyTerms},
};
use sim_runtime::{StandingLinkState, TreatyBundleState, TreatyState, TreatyTermsState};

/// **A treaty's terms on the wire**: the two flags and each bundle's two legs as unrated lines
/// (nothing has been drawn for a promise).
pub(crate) fn terms_state(terms: &TreatyTerms) -> TreatyTermsState {
    TreatyTermsState {
        road_rights: terms.road_rights,
        open_borders: terms.open_borders,
        bundles: terms
            .agreed_trade
            .iter()
            .map(|bundle| TreatyBundleState {
                sent: crate::snapshot::dealings::lines_state(&bundle.sent),
                returned: crate::snapshot::dealings::lines_state(&bundle.returned),
            })
            .collect(),
    }
}

/// **The treaties one viewer's frame carries** (`docs/plan_early_diplomacy.md` §3): the ones the
/// viewer's people is party to, in the other people's id order. Another pair's treaties are theirs
/// to know.
pub(crate) fn treaty_states(treaties: &Treaties, viewer: FactionId) -> Vec<TreatyState> {
    treaties
        .of_viewer(viewer)
        .into_iter()
        .map(|(other, treaty)| TreatyState {
            other_faction: other.0,
            terms: terms_state(&treaty.terms()),
            formed_turn: treaty.formed_turn,
        })
        .collect()
}

/// **The standing links one viewer's frame carries**: every pair of a resident band of the viewer's
/// people and a resident band of ANOTHER people it holds a live tie with, joined end to end by a
/// kept road ([`standing_link`], the one predicate `propose_treaty` and `propose_trade` ask). The
/// live-tie filter is the proposal's own gate, and it keeps a row from naming a band the viewer has
/// never met. Ordered by (viewer band, other band).
pub(crate) fn standing_link_states(
    bands: &[(BandId, FactionId, UVec2)],
    viewer: FactionId,
    roads: &RoadRegistry,
    connections: &ConnectionLedger,
    geometry: (u32, u32, bool),
    floor: LinkFloor,
) -> Vec<StandingLinkState> {
    let (width, height, wrap) = geometry;
    let mut rows = Vec::new();
    for (own_band, own_faction, own_pos) in bands {
        if *own_faction != viewer {
            continue;
        }
        for (other_band, other_faction, other_pos) in bands {
            if *other_faction == viewer
                || !connections.tie_is_live(*own_band, *other_band)
                || !standing_link(roads, *own_pos, *other_pos, width, height, wrap, floor)
            {
                continue;
            }
            rows.push(StandingLinkState {
                viewer_band: own_band.0,
                other_band: other_band.0,
                other_faction: other_faction.0,
            });
        }
    }
    rows.sort_by_key(|row| (row.viewer_band, row.other_band));
    rows
}
