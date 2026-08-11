//! World-state types, partitioned along the domain sections of `snapshot.fbs`.
//!
//! Each module owns one section's structs and enums; append a new snapshot field to the module
//! that owns its section (and to the matching `crate::codec` module).

pub mod campaign;
pub mod connections;
pub mod culture;
pub mod economy;
pub mod governance;
pub mod knowledge;
pub mod map;
pub mod population;
pub mod subsistence;

pub use campaign::*;
pub use connections::*;
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

/// Which hundredths bucket a fixed-point (1e6) value falls in, on the same **round-to-nearest**
/// grid [`same_to_hundredths_f32`] uses.
///
/// The grid has to be round-to-nearest, not floor: `docs/plan_delta_streaming.md` §3.6 states the
/// comparison rule as `(v * 100).round()`, and the whole safety argument for the deadband is that
/// the client is never more than **half** a hundredth behind the sim. Bucketing by floor would
/// silently double that bound, and would put the two live `Scalar` fields
/// (`TileState::temperature`, `TileState::habitability` — both signed) on a different grid from
/// their `f32` neighbours in the same comparison.
///
/// Exact ties land on +∞ here, while `f32::round` sends them away from zero. That is deliberate,
/// not an oversight: this is a "did this change?" predicate over a uniform grid, so which side of a
/// bucket a single boundary value joins cannot change the half-a-hundredth bound — and doing it in
/// integers avoids a float round trip for a value that is already fixed-point.
fn hundredths_bucket_fixed(value: i64) -> i64 {
    (value + FIXED_RAW_PER_HUNDREDTH / 2).div_euclid(FIXED_RAW_PER_HUNDREDTH)
}

/// Do two fixed-point (1e6) values agree to hundredths?
pub(crate) fn same_to_hundredths_fixed(a: i64, b: i64) -> bool {
    hundredths_bucket_fixed(a) == hundredths_bucket_fixed(b)
}

#[cfg(test)]
mod wire_compare_tests {
    use super::*;

    /// One hundredth, in fixed-point raw units — the grid spacing itself.
    const ONE_HUNDREDTH: i64 = FIXED_RAW_PER_HUNDREDTH;
    /// The first raw value above a bucket boundary (boundaries sit at `HALF + k * ONE_HUNDREDTH`).
    const HALF_HUNDREDTH: i64 = FIXED_RAW_PER_HUNDREDTH / 2;

    /// **The fixed-point comparison must be the same round-to-nearest grid as the `f32` one.**
    ///
    /// A floor grid also separates values a hundredth apart, so only the *boundary* placement can
    /// tell the two apart — hence straddling a boundary rather than merely stepping by 0.01.
    #[test]
    fn values_straddling_a_hundredth_boundary_differ_in_both_signs() {
        for k in [-2i64, -1, 0, 1, 2] {
            let boundary = HALF_HUNDREDTH + k * ONE_HUNDREDTH;
            assert!(
                !same_to_hundredths_fixed(boundary - 1, boundary),
                "raw {} and {} sit either side of a round-to-nearest boundary",
                boundary - 1,
                boundary
            );
        }
    }

    /// …and everything strictly inside one bucket compares equal, symmetrically about zero.
    #[test]
    fn values_inside_one_hundredth_bucket_are_equal_in_both_signs() {
        for k in [-2i64, -1, 0, 1, 2] {
            let low = HALF_HUNDREDTH + k * ONE_HUNDREDTH;
            let high = low + ONE_HUNDREDTH - 1;
            assert!(
                same_to_hundredths_fixed(low, high),
                "raw {low} and {high} are the same round-to-nearest bucket"
            );
        }
    }

    /// The two helpers agree away from ties, which is the property the deadband's safety argument
    /// depends on (ties differ by design — see [`hundredths_bucket_fixed`]).
    #[test]
    fn the_fixed_and_float_helpers_share_one_grid() {
        const RAW_PER_UNIT: f32 = 1_000_000.0;
        let probes = [-30_001i64, -20_000, -4_999, -1, 0, 1, 4_999, 20_000, 30_001];
        for a in probes {
            for b in probes {
                assert_eq!(
                    same_to_hundredths_fixed(a, b),
                    same_to_hundredths_f32(a as f32 / RAW_PER_UNIT, b as f32 / RAW_PER_UNIT),
                    "raw {a} vs {b} must bucket the same way on both sides"
                );
            }
        }
    }
}
