//! World-state types, partitioned along the nine domain sections of `snapshot.fbs`.
//!
//! Each module owns one section's structs and enums; append a new snapshot field to the module
//! that owns its section (and to the matching `crate::codec` module).

pub mod campaign;
pub mod culture;
pub mod economy;
pub mod governance;
pub mod knowledge;
pub mod map;
pub mod population;
pub mod subsistence;

pub use campaign::*;
pub use culture::*;
pub use economy::*;
pub use governance::*;
pub use knowledge::*;
pub use map::*;
pub use population::*;
pub use subsistence::*;

/// Hundredths, the precision the client stream is diffed at.
///
/// **Wire precision is a performance decision, not a fidelity one.** A per-tile field compared at
/// full precision puts its tile in *every* delta if it drifts at all, and per-tile cost is paid on
/// the whole map every turn forever — so a field diffed at 1e-6 to be rendered at 1e-1 generates
/// four digits of pure delta traffic. Two decimals is finer than anything the client renders.
///
/// This is deliberately the **comparison** precision, not a mutation: the value on the wire keeps
/// its full precision, so nothing downstream loses resolution — a tile simply stops being called
/// "changed" for a movement no one could see. See `docs/plan_delta_streaming.md` §3.5.
pub(crate) const WIRE_COMPARE_SCALE: f32 = 100.0;

/// Fixed-point (1e6) raw units per hundredth — the [`WIRE_COMPARE_SCALE`] twin for `Scalar` fields.
pub(crate) const FIXED_RAW_PER_HUNDREDTH: i64 = 10_000;

/// Do two `f32`s agree to [`WIRE_COMPARE_SCALE`]? Non-finite values compare bitwise so a `NaN`
/// appearing or clearing is never mistaken for "unchanged".
pub(crate) fn same_to_hundredths_f32(a: f32, b: f32) -> bool {
    if !a.is_finite() || !b.is_finite() {
        return a.to_bits() == b.to_bits();
    }
    (a * WIRE_COMPARE_SCALE).round() == (b * WIRE_COMPARE_SCALE).round()
}

/// Do two fixed-point (1e6) values agree to hundredths?
pub(crate) fn same_to_hundredths_fixed(a: i64, b: i64) -> bool {
    a.div_euclid(FIXED_RAW_PER_HUNDREDTH) == b.div_euclid(FIXED_RAW_PER_HUNDREDTH)
}
