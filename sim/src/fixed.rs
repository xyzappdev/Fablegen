//! Fixed-point arithmetic in Q10: the only numbers the simulation computes
//! with.
//!
//! Two properties matter here more than convenience, and both are frozen once
//! the first reference log exists:
//!
//! - **Rounding is floor everywhere**, toward negative infinity. The shift
//!   operator already rounds that way; integer division in Rust does not, so
//!   floor division is written out by hand.
//! - **Overflow panics.** It neither saturates nor wraps. Saturation would
//!   distort the history quietly and stay invisible to the golden test: the log
//!   would agree with itself while being wrong. A panic stops the run while the
//!   mistake can still be seen.
//!
//! See DETERMINISM.md.

/// The one scale of the simulation: 1024 internal units to 1, that is Q10.
///
/// Frozen. Changing it moves every value of every history ever produced.
pub const SCALE: i32 = 1024;

/// `SCALE` written as a shift distance, so scaling costs a shift and not a
/// division.
pub const SHIFT: u32 = 10;

/// A fixed-point number: an `i32` holding `SCALE` units per whole one.
///
/// A newtype rather than a bare `i32` on purpose: a bare integer would let a
/// scaled value be added to an unscaled one without a single warning, while
/// this turns the same mistake into a compile error.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash, Default)]
pub struct Fx(i32);

// The arithmetic is offered as inherent methods rather than through the
// `std::ops` traits. Operators arrive the day something consumes them; until
// then the type keeps the smaller surface, and every call site names the module
// whose rounding and overflow rules it is asking for.
#[allow(clippy::should_implement_trait)]
impl Fx {
    /// Wraps a raw Q10 value, in internal units, without scaling it.
    pub const fn from_raw(raw: i32) -> Self {
        Self(raw)
    }

    /// Returns the raw Q10 value, in internal units.
    pub const fn raw(self) -> i32 {
        self.0
    }

    /// Scales a whole number into Q10; panics if the scaled value leaves `i32`.
    pub fn from_int(value: i32) -> Self {
        match value.checked_mul(SCALE) {
            Some(raw) => Self(raw),
            None => panic!("Fx::from_int overflow: {value} scaled by {SCALE} leaves i32"),
        }
    }

    /// Returns the whole part, rounded toward negative infinity.
    pub const fn to_int_floor(self) -> i32 {
        // An arithmetic shift right on a signed integer already rounds toward
        // negative infinity, which is the rounding this project uses.
        self.0 >> SHIFT
    }

    // The checked_* calls below are deliberate and are not a duplicate of the
    // `overflow-checks` setting in the release profile. Panicking on overflow
    // is the semantics of this type; a build profile is a setting somebody can
    // drop without noticing, and the type must not depend on it.

    /// Adds two values; panics on overflow rather than wrapping or saturating.
    pub fn add(self, other: Self) -> Self {
        let (a, b) = (self.0, other.0);
        match a.checked_add(b) {
            Some(raw) => Self(raw),
            None => panic!("Fx::add overflow: {a} + {b} leaves i32"),
        }
    }

    /// Subtracts one value from another; panics on overflow.
    pub fn sub(self, other: Self) -> Self {
        let (a, b) = (self.0, other.0);
        match a.checked_sub(b) {
            Some(raw) => Self(raw),
            None => panic!("Fx::sub overflow: {a} - {b} leaves i32"),
        }
    }

    /// Negates the value; panics on `i32::MIN`, which has no positive twin.
    pub fn neg(self) -> Self {
        let a = self.0;
        match a.checked_neg() {
            Some(raw) => Self(raw),
            None => panic!("Fx::neg overflow: -({a}) leaves i32"),
        }
    }

    /// Multiplies in Q10, rounding toward negative infinity; panics if the
    /// result leaves `i32`.
    pub fn mul(self, other: Self) -> Self {
        let (a, b) = (self.0, other.0);
        // The product of two i32 values needs 63 bits, so i64 always holds it.
        // What can leave i32 is the value after the shift, and that is checked.
        let product = i64::from(a) * i64::from(b);
        // An arithmetic shift right rounds toward negative infinity by itself,
        // so unlike div this needs no correction.
        let shifted = product >> SHIFT;
        match i32::try_from(shifted) {
            Ok(raw) => Self(raw),
            Err(_) => panic!("Fx::mul overflow: ({a} * {b}) >> {SHIFT} leaves i32"),
        }
    }

    /// Divides in Q10, rounding toward negative infinity; panics on a zero
    /// divisor and when the result leaves `i32`.
    // Rust's `/` truncates toward zero; this project rounds toward negative
    // infinity everywhere (frozen after S1). The adjustment below is not a
    // style choice: mixing two rounding modes in one module is a guaranteed
    // divergence, so floor division is written out explicitly.
    pub fn div(self, other: Self) -> Self {
        let (a, b) = (self.0, other.0);
        if b == 0 {
            panic!("Fx::div by zero: {a} / 0");
        }

        // The numerator widens before it is shifted, so nothing is lost on the
        // way in: an i32 shifted left by SHIFT needs 41 bits.
        let numerator = i64::from(a) << SHIFT;
        let divisor = i64::from(b);
        let mut quotient = numerator / divisor;
        let remainder = numerator % divisor;
        // A remainder in Rust carries the sign of the numerator, so operands of
        // unlike sign are exactly the case where truncation landed one step
        // above the floor.
        if remainder != 0 && (remainder < 0) != (divisor < 0) {
            quotient -= 1;
        }

        match i32::try_from(quotient) {
            Ok(raw) => Self(raw),
            Err(_) => panic!("Fx::div overflow: ({a} << {SHIFT}) / {b} leaves i32"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Fx, SCALE, SHIFT};

    #[test]
    fn from_int_round_trips_through_to_int_floor() {
        assert_eq!(Fx::from_int(0).raw(), 0);
        assert_eq!(Fx::from_int(3).raw(), 3 * SCALE);
        assert_eq!(Fx::from_int(-3).raw(), -3 * SCALE);
        assert_eq!(Fx::from_int(3).to_int_floor(), 3);
        assert_eq!(Fx::from_int(-3).to_int_floor(), -3);
    }

    #[test]
    fn mul_and_div_are_exact_on_positives() {
        assert_eq!(Fx::from_int(3).mul(Fx::from_int(4)), Fx::from_int(12));
        assert_eq!(Fx::from_int(12).div(Fx::from_int(4)), Fx::from_int(3));
        // 1.5 is held exactly in Q10, and 1.5 * 2 is 3.
        let one_and_a_half = Fx::from_raw(SCALE + SCALE / 2);
        assert_eq!(one_and_a_half.mul(Fx::from_int(2)), Fx::from_int(3));
        assert_eq!(Fx::from_int(3).div(Fx::from_int(2)), one_and_a_half);
    }

    #[test]
    fn div_rounds_toward_negative_infinity() {
        // -3 / 2 is -1.5, which Q10 holds exactly: -1536 raw units, and -2 once
        // the whole part is taken.
        let half_of_minus_three = Fx::from_int(-3).div(Fx::from_int(2));
        assert_eq!(half_of_minus_three.raw(), -1536);
        assert_eq!(half_of_minus_three.to_int_floor(), -2);

        // -1 / 3 is -0.333..., which Q10 does not hold exactly. Truncation
        // toward zero would give -341 raw units; the floor is -342.
        assert_eq!(Fx::from_int(-1).div(Fx::from_int(3)).raw(), -342);
        // Its positive twin is untouched: there floor and truncation agree.
        assert_eq!(Fx::from_int(1).div(Fx::from_int(3)).raw(), 341);
    }

    #[test]
    fn div_leaves_an_exact_quotient_alone() {
        // The correction must fire on a non-zero remainder and nowhere else,
        // or every exact division of unlike signs would drift by one unit.
        assert_eq!(Fx::from_int(-12).div(Fx::from_int(4)), Fx::from_int(-3));
        assert_eq!(Fx::from_int(12).div(Fx::from_int(-4)), Fx::from_int(-3));
        assert_eq!(Fx::from_int(-12).div(Fx::from_int(-4)), Fx::from_int(3));
        assert_eq!(Fx::from_int(-12).div(Fx::from_int(4)).raw(), -3 * SCALE);
    }

    #[test]
    fn mul_rounds_toward_negative_infinity() {
        // -3 raw units times 0.5 is -1.5 raw units. Truncation toward zero
        // would give -1; the shift floors it to -2.
        assert_eq!(Fx::from_raw(-3).mul(Fx::from_raw(SCALE / 2)).raw(), -2);
        // The positive twin of the same magnitude rounds the other way.
        assert_eq!(Fx::from_raw(3).mul(Fx::from_raw(SCALE / 2)).raw(), 1);
    }

    #[test]
    fn mul_agrees_with_the_shift_on_negatives() {
        for (a, b) in [(-1, 1), (-1234, 5678), (-999_999, -1_000), (7, -3)] {
            let expected = (i64::from(a) * i64::from(b)) >> SHIFT;
            let expected = i32::try_from(expected).expect("the pair fits i32 by construction");
            assert_eq!(
                Fx::from_raw(a).mul(Fx::from_raw(b)).raw(),
                expected,
                "mul disagreed with the shift on {a} * {b}"
            );
        }
    }

    #[test]
    fn to_int_floor_rounds_negatives_down() {
        assert_eq!(Fx::from_raw(-1).to_int_floor(), -1);
        assert_eq!(Fx::from_raw(-SCALE).to_int_floor(), -1);
        assert_eq!(Fx::from_raw(-SCALE - 1).to_int_floor(), -2);
        assert_eq!(Fx::from_raw(SCALE - 1).to_int_floor(), 0);
    }

    #[test]
    fn the_i32_bounds_survive_from_raw() {
        assert_eq!(Fx::from_raw(i32::MAX).raw(), i32::MAX);
        assert_eq!(Fx::from_raw(i32::MIN).raw(), i32::MIN);
        assert!(Fx::from_raw(i32::MIN) < Fx::from_raw(i32::MAX));
        assert_eq!(Fx::default().raw(), 0);
    }

    #[test]
    fn operations_at_the_bounds_that_fit_do_not_panic() {
        assert_eq!(
            Fx::from_raw(i32::MAX).sub(Fx::from_raw(1)).raw(),
            i32::MAX - 1
        );
        assert_eq!(
            Fx::from_raw(i32::MIN).add(Fx::from_raw(1)).raw(),
            i32::MIN + 1
        );
        assert_eq!(Fx::from_raw(i32::MAX).neg().raw(), -i32::MAX);
        assert_eq!(Fx::from_raw(i32::MIN).add(Fx::from_raw(i32::MAX)).raw(), -1);

        // The largest whole number Q10 can carry, scaled and taken back.
        let largest_whole = i32::MAX / SCALE;
        assert_eq!(Fx::from_int(largest_whole).to_int_floor(), largest_whole);
    }

    #[test]
    fn the_i64_intermediate_saves_a_product_that_leaves_i32() {
        // 100_000 * 20 in Q10. The raw product is 102_400_000 * 20_480, that is
        // 2_097_152_000_000: about a thousand times what i32 holds. After the
        // shift it is 2_048_000_000 and fits with room left over, and the answer
        // is exact. Without the widening to i64 the multiplication could not be
        // performed at all.
        let a = Fx::from_int(100_000);
        let b = Fx::from_int(20);
        let raw_product = i64::from(a.raw()) * i64::from(b.raw());

        assert_eq!(raw_product, 2_097_152_000_000);
        assert!(i32::try_from(raw_product).is_err());
        assert_eq!(a.mul(b), Fx::from_int(2_000_000));
        assert_eq!(a.mul(b).raw(), 2_048_000_000);
    }

    #[test]
    #[should_panic(expected = "Fx::add overflow")]
    fn add_overflow_panics() {
        let _ = Fx::from_raw(i32::MAX).add(Fx::from_raw(1));
    }

    #[test]
    #[should_panic(expected = "Fx::sub overflow")]
    fn sub_overflow_panics() {
        let _ = Fx::from_raw(i32::MIN).sub(Fx::from_raw(1));
    }

    #[test]
    #[should_panic(expected = "Fx::neg overflow")]
    fn neg_of_the_minimum_panics() {
        let _ = Fx::from_raw(i32::MIN).neg();
    }

    #[test]
    #[should_panic(expected = "Fx::mul overflow")]
    fn mul_overflow_panics_although_the_i64_product_fits() {
        // i32::MAX raw units times 2. The product is 4_398_046_509_056, which
        // i64 carries without trouble; after the shift it is 4_294_967_294,
        // exactly twice i32::MAX, so the result is what does not fit.
        let _ = Fx::from_raw(i32::MAX).mul(Fx::from_int(2));
    }

    #[test]
    #[should_panic(expected = "Fx::div overflow")]
    fn div_overflow_panics() {
        // The fixed-point twin of i32::MIN / -1: the quotient is 2_147_483_648,
        // one past i32::MAX.
        let _ = Fx::from_raw(i32::MIN).div(Fx::from_int(-1));
    }

    #[test]
    #[should_panic(expected = "Fx::div by zero")]
    fn div_by_zero_panics() {
        let _ = Fx::from_int(1).div(Fx::from_raw(0));
    }

    #[test]
    #[should_panic(expected = "Fx::from_int overflow")]
    fn from_int_overflow_panics() {
        let _ = Fx::from_int(i32::MAX / SCALE + 1);
    }
}
