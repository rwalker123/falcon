//! **The worth of a gift — one formula, shared by the sim and every player process**
//! (`docs/plan_early_diplomacy.md` §2).
//!
//! `worth = Σ over goods ( load_g × amount_g / (amount_g + held_g) )`: a good the receiver holds none
//! of counts at its full load, a good it already holds ten times over counts for about a tenth. It
//! lives here, in the crate both `core_sim` and `sim_ai` depend on, so the sim's goodwill credit and
//! an AI people's judgement of an offer are the same arithmetic and cannot drift. It is the receiver's
//! judgement, never a price table: the number becomes goodwill (in the sim) or an accept/refuse (in
//! an AI) and is never published as a price.

/// One good of a gift, as the receiver's judgement sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GoodAmount {
    /// The load of the gift's whole amount of this good, in the one load currency (`carry.rs`):
    /// what it would weigh in a pack. A good they hold none of is worth exactly this.
    pub load: f32,
    /// How much of it crosses.
    pub amount: f32,
    /// How much the receiving band already holds, in the same unit, immediately before.
    pub held: f32,
}

/// `load × amount / (amount + held)` — a good they have none of counts at its full load, a good
/// they already hold ten times over counts for about a tenth. Zero when nothing crosses.
pub fn good_worth(good: GoodAmount) -> f32 {
    let denominator = good.amount + good.held;
    if good.amount <= 0.0 || denominator <= 0.0 {
        return 0.0;
    }
    good.load * good.amount / denominator
}

/// **The worth of a gift: the sum of its goods' worth.**
pub fn worth(goods: &[GoodAmount]) -> f32 {
    goods.iter().copied().map(good_worth).sum()
}
