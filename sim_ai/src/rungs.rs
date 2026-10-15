//! **The shipped rungs the seat plays, by the wire keys `ladderRungs` publishes.** A seat is a
//! player process: it reaches the world through the snapshot alone, so it names a rung the way it
//! names a verb or a knowledge (`specialists/food/rules.rs`) - by the id the shipped
//! `intensification_ladder.json` declares. Every per-rung figure it reads off a patch or a herd row
//! is a [`sim_runtime::RungQuoteState`] looked up under one of these keys.

/// The plant branch's tended rung (verb `cultivate`).
pub const PLANT_TENDED: &str = "plant:tended";
/// The plant branch's sown rung (verb `sow`).
pub const PLANT_FIELD: &str = "plant:field";
/// The animal branch's domesticated rung (verb `tame`).
pub const ANIMAL_PASTORAL: &str = "animal:pastoral";
/// The animal branch's enclosed rung (verb `corral`).
pub const ANIMAL_PEN: &str = "animal:pen";
