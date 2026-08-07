//! Trigonometry for the simulation: a committed table, never a library call.
//!
//! `sin`, `cos` and `atan2` from the standard library are banned
//! (DETERMINISM.md #4): their last bits differ between implementations, so
//! one seed would produce two histories on two machines. Instead the circle is
//! cut into 1024 steps and the sine of every step is committed as data in
//! `trig_table.rs`, generated offline by `tools/gen-trig-table`.
//!
//! There is no cosine table. Cosine is the same table read a quarter turn
//! ahead, so there is exactly one set of numbers to keep correct, and no way
//! for two tables to drift apart. `atan2` uses the same table again, by binary
//! search, so it agrees with `sin` and `cos` by construction rather than by
//! two sets of numbers happening to match.

use crate::fixed::Fx;
use crate::trig_table::SIN;

/// Steps the full circle is cut into: angle indices run over `0..1024`.
pub const ANGLE_STEPS: i32 = 1024;

// A power of two, so folding an index onto the circle is one AND instead of a
// remainder. The mask also does the right thing with negative indices, which
// `%` would not: -1 folds to 1023, a step backwards along the circle, rather
// than to -1.
const ANGLE_MASK: i32 = ANGLE_STEPS - 1;

// A quarter turn in steps. Reading the sine table this far ahead gives cosine,
// because cos(x) == sin(x + a quarter turn).
const QUARTER_TURN: i32 = ANGLE_STEPS / 4;

// A half turn in steps, used to reflect a folded angle back across the axes.
const HALF_TURN: i32 = ANGLE_STEPS / 2;

// An eighth of the circle: the range `atan2` searches after folding.
const OCTANT_STEPS: i32 = ANGLE_STEPS / 8;

// Reads the sine table at any index, folding it onto the circle first. The one
// place that touches the array, so the fold cannot be forgotten somewhere.
fn sin_raw(index: i32) -> i32 {
    // After the mask the index is in 0..1024 with the sign bit cleared, so the
    // lookup is always in range and the conversion loses nothing.
    SIN[(index & ANGLE_MASK) as usize]
}

// Reads the cosine at any index: the same table, a quarter turn ahead. Folding
// before the addition keeps the sum away from the upper bound of i32, where
// adding a quarter turn to i32::MAX would overflow and panic.
fn cos_raw(index: i32) -> i32 {
    sin_raw((index & ANGLE_MASK) + QUARTER_TURN)
}

/// Returns the sine of an angle index in Q10; any index folds onto the circle.
pub fn sin(index: i32) -> Fx {
    Fx::from_raw(sin_raw(index))
}

/// Returns the cosine of an angle index in Q10; any index folds onto the circle.
pub fn cos(index: i32) -> Fx {
    Fx::from_raw(cos_raw(index))
}

/// A direction folded into the first octant, with the way back remembered.
struct Fold {
    /// The larger of the two magnitudes.
    u: i64,
    /// The smaller of the two magnitudes.
    v: i64,
    /// Whether the magnitudes were exchanged to get there.
    swapped: bool,
    x_negative: bool,
    y_negative: bool,
}

/// Folds a direction into the first octant, keeping what is needed to unfold.
fn fold(y: Fx, x: Fx) -> Fold {
    // Widening before taking the magnitude is not fussiness: i32::MIN has no
    // positive twin inside i32, so `abs` on the raw value would overflow and
    // panic on exactly one input.
    let raw_x = i64::from(x.raw());
    let raw_y = i64::from(y.raw());
    let (ax, ay) = (raw_x.abs(), raw_y.abs());

    // One comparison puts the direction below the diagonal, which together
    // with the two signs is the whole octant reduction.
    let swapped = ay > ax;
    let (u, v) = if swapped { (ay, ax) } else { (ax, ay) };

    Fold {
        u,
        v,
        swapped,
        x_negative: raw_x < 0,
        y_negative: raw_y < 0,
    }
}

/// Finds the angle of a first-octant direction: the largest tabulated index
/// whose tangent does not exceed `v / u`.
///
/// No division and no second table. `SIN[m] * u <= COS[m] * v` orders exactly
/// as `tan(m) <= v / u` does, because the cosine is positive across the octant,
/// and the products are taken in i64 so nothing overflows on the way.
fn search_octant(u: i64, v: i64) -> i32 {
    // Both magnitudes are zero, which happens only at the origin. The angle of
    // the origin is a convention, not a result; see `atan2`.
    if u == 0 {
        return 0;
    }

    // Index 0 always qualifies -- SIN[0] is zero and both magnitudes are
    // non-negative -- so the invariant "low qualifies" holds before the first
    // step and is what makes the search total.
    let mut low = 0;
    let mut high = OCTANT_STEPS;

    while low < high {
        // Rounded upwards, or a midpoint equal to `low` would never advance.
        let mid = low + (high - low + 1) / 2;
        if i64::from(sin_raw(mid)) * u <= i64::from(cos_raw(mid)) * v {
            low = mid;
        } else {
            high = mid - 1;
        }
    }

    low
}

/// Turns a first-octant angle back into a full-circle index.
fn unfold(folded: &Fold, local: i32) -> i32 {
    // Undo the exchange of magnitudes: a reflection about the diagonal of the
    // quadrant, which is the quarter turn minus the angle.
    let quadrant = if folded.swapped {
        QUARTER_TURN - local
    } else {
        local
    };

    // Then the signs. Masking at the end is what folds a full turn back to
    // zero in the fourth quadrant, where the reflection produces a negative.
    let index = match (folded.x_negative, folded.y_negative) {
        (false, false) => quadrant,
        (true, false) => HALF_TURN - quadrant,
        (true, true) => HALF_TURN + quadrant,
        (false, true) => -quadrant,
    };

    index & ANGLE_MASK
}

/// Returns the angle index of the direction `(x, y)`, in `0..1024`.
///
/// The same units `sin` and `cos` take, so `atan2(sin(i), cos(i))` is `i`
/// exactly, for every index.
///
/// `atan2(0, 0)` returns 0. That is a **convention, not a result**: the origin
/// has no direction, and every other answer is equally arbitrary. It is written
/// down here because a panic on the hot path would be worse, and because an
/// undocumented arbitrary value is the kind of thing that gets "fixed" later
/// and moves every history ever produced.
///
/// Directions between two tabulated ones are resolved to the tabulated
/// direction on the side of the nearer axis -- a consequence of folding into
/// the first octant and reflecting back, which floors the folded angle and
/// therefore rounds toward whichever axis the fold measured from. This is
/// frozen with the first reference log, like every other rounding rule.
pub fn atan2(y: Fx, x: Fx) -> i32 {
    let folded = fold(y, x);
    unfold(&folded, search_octant(folded.u, folded.v))
}

#[cfg(test)]
mod tests {
    use super::{
        atan2, cos, fold, search_octant, sin, sin_raw, unfold, Fold, ANGLE_MASK, ANGLE_STEPS,
        HALF_TURN, OCTANT_STEPS, QUARTER_TURN, SIN,
    };
    use crate::fixed::{Fx, SCALE};

    // The table is checked by two mechanisms that share nothing. Here are the
    // invariants: properties a table of sines has to hold whatever produced it,
    // in integers only. The exact comparison against a fresh computation lives
    // in tools/gen-trig-table, where floating point is allowed.
    //
    // Every check below takes the table as an argument rather than reading the
    // constant, and that is the whole point: it lets the probes at the bottom
    // hand it a corrupted copy and demand a failure. A check that has never
    // been seen to fail is not known to check anything -- the same reason
    // ci/gates.sh self-tests on every run.

    /// Reads a table at any index, folding onto the circle exactly as `sin` does.
    fn at(table: &[i32; 1024], index: i32) -> i32 {
        table[(index & ANGLE_MASK) as usize]
    }

    /// The four quarter points, which the table must hit exactly.
    fn check_reference_points(table: &[i32; 1024]) -> Result<(), String> {
        let expected = [
            (0, 0),
            (QUARTER_TURN, SCALE),
            (2 * QUARTER_TURN, 0),
            (3 * QUARTER_TURN, -SCALE),
        ];

        for (index, want) in expected {
            let got = at(table, index);
            if got != want {
                return Err(format!("table[{index}] is {got}, expected {want}"));
            }
        }
        Ok(())
    }

    /// The table negates under reflection: `table[-i] == -table[i]`.
    ///
    /// This is what the offline rounding mode buys. Ties away from zero negates
    /// cleanly; floor does not, and a floored table would sag toward negative
    /// infinity and lose this symmetry.
    fn check_antisymmetry(table: &[i32; 1024]) -> Result<(), String> {
        for index in 0..ANGLE_STEPS {
            let value = at(table, index);
            let mirrored = at(table, ANGLE_STEPS - index);
            if mirrored != -value {
                return Err(format!(
                    "table[{}] is {mirrored}, expected {} to mirror table[{index}]",
                    ANGLE_STEPS - index,
                    -value
                ));
            }
        }
        Ok(())
    }

    /// Each quarter of the circle is monotone in the direction a sine goes.
    ///
    /// Weakly monotone, not strictly. Near the peak the sine changes by less
    /// than half a unit of Q10 per step, so neighbours round to the same
    /// integer -- table[255] and table[256] are both SCALE. Demanding a strict
    /// step there would be demanding a resolution Q10 does not have.
    fn check_monotone_quarters(table: &[i32; 1024]) -> Result<(), String> {
        for quarter in 0..4 {
            // Rising from zero to the peak, falling across the peak and through
            // zero to the trough, rising again from the trough back to zero.
            let going_up = quarter == 0 || quarter == 3;
            let start = quarter * QUARTER_TURN;

            for index in start..start + QUARTER_TURN {
                let current = at(table, index);
                let next = at(table, index + 1);
                let ordered = if going_up {
                    next >= current
                } else {
                    next <= current
                };
                if !ordered {
                    let direction = if going_up { "rise" } else { "fall" };
                    return Err(format!(
                        "quarter {quarter} must {direction}: table[{index}] is {current}, \
                         table[{}] is {next}",
                        index + 1
                    ));
                }
            }
        }
        Ok(())
    }

    /// No value leaves the range a sine can reach: `-SCALE..=SCALE`.
    fn check_range(table: &[i32; 1024]) -> Result<(), String> {
        for (index, &value) in table.iter().enumerate() {
            if !(-SCALE..=SCALE).contains(&value) {
                return Err(format!(
                    "table[{index}] is {value}, outside -{SCALE}..={SCALE}"
                ));
            }
        }
        Ok(())
    }

    /// How far `sin^2 + cos^2` may sit from `SCALE^2`.
    ///
    /// Derived, not measured. Every stored value is the true one off by at most
    /// half a unit: `s = S + e`, `|e| <= 1/2`. Then `s^2 = S^2 + 2*S*e + e^2`,
    /// and since `|S| <= SCALE`, the linear term drifts by at most
    /// `2 * SCALE * 1/2 = SCALE`. Two such terms give `2 * SCALE`, and the two
    /// `e^2` add at most `1/2` on top, which cannot carry an integer past the
    /// bound. Tightening this to what the table actually achieves would be
    /// fitting the check to the answer, so it stays at the derived value.
    const IDENTITY_TOLERANCE: i64 = 2 * SCALE as i64;

    /// `sin^2 + cos^2` stays within rounding distance of one, everywhere.
    fn check_pythagorean_identity(table: &[i32; 1024]) -> Result<(), String> {
        let unit = i64::from(SCALE) * i64::from(SCALE);

        for index in 0..ANGLE_STEPS {
            let sine = i64::from(at(table, index));
            let cosine = i64::from(at(table, index + QUARTER_TURN));
            let sum = sine * sine + cosine * cosine;
            let drift = (sum - unit).abs();

            if drift > IDENTITY_TOLERANCE {
                return Err(format!(
                    "at index {index}: sin^2 + cos^2 is {sum}, off from {unit} by {drift}"
                ));
            }
        }
        Ok(())
    }

    // -- The committed table against every invariant ------------------------

    #[test]
    fn the_table_hits_the_quarter_points_exactly() {
        check_reference_points(&SIN).expect("committed table");
    }

    #[test]
    fn the_table_is_antisymmetric() {
        check_antisymmetry(&SIN).expect("committed table");
    }

    #[test]
    fn the_table_is_monotone_on_every_quarter() {
        check_monotone_quarters(&SIN).expect("committed table");
    }

    #[test]
    fn the_table_stays_inside_the_unit_circle() {
        check_range(&SIN).expect("committed table");
    }

    #[test]
    fn the_table_satisfies_the_pythagorean_identity() {
        check_pythagorean_identity(&SIN).expect("committed table");
    }

    // -- Negative probes: each check has to be seen failing ------------------

    /// A copy of the committed table, for a probe to damage.
    fn corrupted() -> [i32; 1024] {
        SIN
    }

    #[test]
    fn probe_a_rotation_by_one_step_fails_the_reference_points() {
        let table = corrupted();
        let mut rotated = [0_i32; 1024];
        for (index, slot) in rotated.iter_mut().enumerate() {
            *slot = table[(index + 1) % table.len()];
        }
        let error = check_reference_points(&rotated)
            .expect_err("a table rotated by one step passed the reference points");
        // Not just "some error": the message has to name the damage. A probe
        // satisfied by any failure would stay green on a check that broke for
        // an unrelated reason.
        assert!(
            error.contains("table[0] is 6"),
            "the reference points failed for the wrong reason: {error}"
        );
    }

    #[test]
    fn probe_a_flipped_sign_in_the_second_half_fails_the_antisymmetry() {
        let mut table = corrupted();
        // Index 700 sits in the second half, where the sine is negative and far
        // from zero, so flipping the sign genuinely changes the value.
        assert_ne!(table[700], 0);
        table[700] = -table[700];
        let error = check_antisymmetry(&table)
            .expect_err("a table with one sign flipped passed the antisymmetry check");
        assert!(
            error.contains("table[700]"),
            "the antisymmetry failed for the wrong reason: {error}"
        );
    }

    #[test]
    fn probe_two_swapped_neighbours_fail_the_monotonicity() {
        let mut table = corrupted();
        // Mid-quarter, where consecutive entries differ by several units, so
        // the swap really does reverse the order.
        assert!(table[100] < table[101]);
        table.swap(100, 101);
        let error = check_monotone_quarters(&table)
            .expect_err("a table with two neighbours swapped passed the monotonicity check");
        assert!(
            error.contains("quarter 0 must rise") && error.contains("table[100]"),
            "the monotonicity failed for the wrong reason: {error}"
        );
    }

    #[test]
    fn probe_a_value_past_the_peak_fails_the_range() {
        let mut table = corrupted();
        table[100] = SCALE + 1;
        let error =
            check_range(&table).expect_err("a table reaching past SCALE passed the range check");
        assert!(
            error.contains("table[100] is 1025"),
            "the range check failed for the wrong reason: {error}"
        );
    }

    #[test]
    fn probe_a_dented_value_fails_the_identity() {
        let mut table = corrupted();
        // Far beyond half a unit of rounding, which is all the tolerance
        // allows for.
        table[100] += 100;
        let error = check_pythagorean_identity(&table)
            .expect_err("a table dented by 100 units passed the identity check");
        assert!(
            error.contains("at index 100"),
            "the identity failed for the wrong reason: {error}"
        );
    }

    // -- The lookup functions ------------------------------------------------

    #[test]
    fn sin_and_cos_return_the_quarter_points() {
        assert_eq!(sin(0), Fx::from_raw(0));
        assert_eq!(sin(QUARTER_TURN), Fx::from_raw(SCALE));
        assert_eq!(sin(2 * QUARTER_TURN), Fx::from_raw(0));
        assert_eq!(sin(3 * QUARTER_TURN), Fx::from_raw(-SCALE));

        assert_eq!(cos(0), Fx::from_raw(SCALE));
        assert_eq!(cos(QUARTER_TURN), Fx::from_raw(0));
        assert_eq!(cos(2 * QUARTER_TURN), Fx::from_raw(-SCALE));
        assert_eq!(cos(3 * QUARTER_TURN), Fx::from_raw(0));
    }

    #[test]
    fn cos_is_the_sine_table_read_a_quarter_turn_ahead() {
        for index in 0..ANGLE_STEPS {
            assert_eq!(cos(index), sin(index + QUARTER_TURN), "at index {index}");
        }
    }

    #[test]
    fn any_index_folds_onto_the_circle() {
        for index in 0..ANGLE_STEPS {
            assert_eq!(
                sin(index + ANGLE_STEPS),
                sin(index),
                "one turn on at {index}"
            );
            assert_eq!(
                sin(index - ANGLE_STEPS),
                sin(index),
                "one turn back at {index}"
            );
            assert_eq!(
                cos(index + ANGLE_STEPS),
                cos(index),
                "one turn on at {index}"
            );
        }

        // Negative indices walk backwards along the circle rather than off it.
        assert_eq!(sin(-1), sin(ANGLE_STEPS - 1));
        assert_eq!(cos(-1), cos(ANGLE_STEPS - 1));
    }

    #[test]
    fn the_bounds_of_i32_fold_without_overflowing() {
        // A quarter turn is added inside `cos`, so an unfolded i32::MAX would
        // overflow there and panic under both build profiles.
        assert_eq!(sin(i32::MAX), sin(i32::MAX & ANGLE_MASK));
        assert_eq!(cos(i32::MAX), cos(i32::MAX & ANGLE_MASK));
        assert_eq!(sin(i32::MIN), sin(0));
        assert_eq!(cos(i32::MIN), cos(0));
    }

    // -- atan2 ---------------------------------------------------------------
    //
    // Same discipline as the table above: every property is a function taking
    // the implementation as an argument, so the probes at the bottom can hand
    // it a deliberately broken one and require a failure.

    /// An angle-from-direction implementation, so a probe can supply a broken one.
    type Atan2Impl = fn(Fx, Fx) -> i32;

    /// A direction inside `octant`, at parameter `t` in `0..=SCALE`.
    ///
    /// The sweeps run in increasing-angle order and meet exactly on the octant
    /// boundaries: the last direction of one octant is the first of the next.
    /// Every coordinate is an exact integer, so nothing here depends on the
    /// property being measured.
    fn sweep_direction(octant: i32, t: i32) -> (Fx, Fx) {
        let s = SCALE;
        let (x, y) = match octant {
            0 => (s, t),
            1 => (s - t, s),
            2 => (-t, s),
            3 => (-s, s - t),
            4 => (-s, -t),
            5 => (-(s - t), -s),
            6 => (t, -s),
            7 => (s, -(s - t)),
            _ => panic!("octant {octant} is outside 0..8"),
        };
        (Fx::from_raw(x), Fx::from_raw(y))
    }

    /// Every tabulated direction must come back as the index it was built from.
    fn check_round_trip(atan2_impl: Atan2Impl) -> Result<(), String> {
        let mut diverging = Vec::new();
        let mut total = 0;

        for index in 0..ANGLE_STEPS {
            let recovered = atan2_impl(sin(index), cos(index));
            if recovered != index {
                total += 1;
                if diverging.len() < 8 {
                    diverging.push(format!("{index} -> {recovered}"));
                }
            }
        }

        if total == 0 {
            return Ok(());
        }
        Err(format!(
            "the round trip diverges at {total} of {ANGLE_STEPS} indices, first: {}",
            diverging.join(", ")
        ))
    }

    /// The four axes, which have to land on the quarter points exactly.
    fn check_reference_directions(atan2_impl: Atan2Impl) -> Result<(), String> {
        let axes = [
            ("positive x", 0, SCALE, 0),
            ("positive y", SCALE, 0, QUARTER_TURN),
            ("negative x", 0, -SCALE, 2 * QUARTER_TURN),
            ("negative y", -SCALE, 0, 3 * QUARTER_TURN),
        ];

        for (name, y, x, want) in axes {
            let got = atan2_impl(Fx::from_raw(y), Fx::from_raw(x));
            if got != want {
                return Err(format!("the {name} axis gave {got}, expected {want}"));
            }
        }
        Ok(())
    }

    /// Lengthening a direction must not turn it.
    ///
    /// This is what catches an implementation that reached for a magnitude
    /// where it needed a ratio: the two agree at length SCALE and part company
    /// everywhere else.
    fn check_scale_invariance(atan2_impl: Atan2Impl) -> Result<(), String> {
        for octant in 0..8 {
            for t in (0..=SCALE).step_by(37) {
                let (x, y) = sweep_direction(octant, t);
                let want = atan2_impl(y, x);

                for factor in [2, 3, 17, 1000] {
                    let scaled_x = Fx::from_raw(x.raw() * factor);
                    let scaled_y = Fx::from_raw(y.raw() * factor);
                    let got = atan2_impl(scaled_y, scaled_x);
                    if got != want {
                        return Err(format!(
                            "octant {octant} at t={t} scaled by {factor}: \
                             angle {got}, expected {want}"
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Sweeping an octant must walk its indices in order and stay inside it.
    fn check_monotone_octants(atan2_impl: Atan2Impl) -> Result<(), String> {
        for octant in 0..8 {
            let low = octant * OCTANT_STEPS;
            let high = low + OCTANT_STEPS;
            let mut previous: Option<i32> = None;

            for t in 0..=SCALE {
                let (x, y) = sweep_direction(octant, t);
                let angle = atan2_impl(y, x);

                // The eighth octant runs into the closure of the circle, where
                // a full turn is written 0 because the index space is modular.
                // Reading it back as ANGLE_STEPS is not papering over a
                // discontinuity: monotonicity is a claim about the angle, and
                // the wrap belongs to the representative, not to the angle.
                let angle = if octant == 7 && angle == 0 {
                    ANGLE_STEPS
                } else {
                    angle
                };

                if !(low..=high).contains(&angle) {
                    return Err(format!(
                        "octant {octant} at t={t}: angle {angle} is outside {low}..={high}"
                    ));
                }
                if let Some(previous) = previous {
                    if angle < previous {
                        return Err(format!(
                            "octant {octant} at t={t}: angle {angle} went back from {previous}"
                        ));
                    }
                }
                previous = Some(angle);
            }
        }
        Ok(())
    }

    #[test]
    fn atan2_recovers_every_tabulated_index() {
        check_round_trip(atan2).expect("the shipped atan2");
    }

    #[test]
    fn atan2_hits_the_four_axes() {
        check_reference_directions(atan2).expect("the shipped atan2");
    }

    #[test]
    fn atan2_ignores_the_length_of_a_direction() {
        check_scale_invariance(atan2).expect("the shipped atan2");
    }

    #[test]
    fn atan2_walks_each_octant_in_order() {
        check_monotone_octants(atan2).expect("the shipped atan2");
    }

    #[test]
    fn atan2_of_the_origin_is_the_documented_convention() {
        // Not a natural result, a written-down choice. The test exists so that
        // changing it has to be deliberate.
        assert_eq!(atan2(Fx::from_raw(0), Fx::from_raw(0)), 0);
    }

    #[test]
    fn atan2_survives_the_bounds_of_the_fixed_point_type() {
        // i32::MIN is the input that would overflow a magnitude taken before
        // widening, and the products would overflow i32 long before that.
        // Nothing here may panic under either build profile.
        let extremes = [i32::MIN, i32::MIN + 1, -SCALE, -1, 0, 1, SCALE, i32::MAX];

        for &x in &extremes {
            for &y in &extremes {
                let angle = atan2(Fx::from_raw(y), Fx::from_raw(x));
                assert!(
                    (0..ANGLE_STEPS).contains(&angle),
                    "atan2({y}, {x}) gave {angle}, outside the index space"
                );
            }
        }

        // The axes still answer correctly at the far end of the range.
        assert_eq!(atan2(Fx::from_raw(0), Fx::from_raw(i32::MAX)), 0);
        assert_eq!(atan2(Fx::from_raw(i32::MAX), Fx::from_raw(0)), QUARTER_TURN);
        assert_eq!(atan2(Fx::from_raw(0), Fx::from_raw(i32::MIN)), HALF_TURN);
        assert_eq!(
            atan2(Fx::from_raw(i32::MIN), Fx::from_raw(0)),
            3 * QUARTER_TURN
        );
    }

    // -- Negative probes on atan2 --------------------------------------------

    /// The pipeline with the octant mapping shifted one step.
    fn atan2_with_a_shifted_octant(y: Fx, x: Fx) -> i32 {
        let folded = fold(y, x);
        // The real one unfolds the index the search found.
        unfold(&folded, search_octant(folded.u, folded.v) + 1)
    }

    /// The pipeline with the magnitude comparison that picks the octant inverted.
    fn atan2_with_an_inverted_swap(y: Fx, x: Fx) -> i32 {
        let raw_x = i64::from(x.raw());
        let raw_y = i64::from(y.raw());
        let (ax, ay) = (raw_x.abs(), raw_y.abs());
        // The real fold swaps when `ay > ax`.
        let swapped = ay <= ax;
        let (u, v) = if swapped { (ay, ax) } else { (ax, ay) };
        let folded = Fold {
            u,
            v,
            swapped,
            x_negative: raw_x < 0,
            y_negative: raw_y < 0,
        };
        unfold(&folded, search_octant(folded.u, folded.v))
    }

    /// The pipeline with the magnitudes never taken.
    fn atan2_without_the_magnitude(y: Fx, x: Fx) -> i32 {
        let raw_x = i64::from(x.raw());
        let raw_y = i64::from(y.raw());
        // The real fold takes `abs` of both here.
        let (ax, ay) = (raw_x, raw_y);
        let swapped = ay > ax;
        let (u, v) = if swapped { (ay, ax) } else { (ax, ay) };
        let folded = Fold {
            u,
            v,
            swapped,
            x_negative: raw_x < 0,
            y_negative: raw_y < 0,
        };
        unfold(&folded, search_octant(folded.u, folded.v))
    }

    /// The pipeline comparing the smaller magnitude against the sine directly,
    /// instead of comparing the two cross products.
    fn atan2_from_the_magnitude_alone(y: Fx, x: Fx) -> i32 {
        let folded = fold(y, x);
        // Right only for a direction of length SCALE, which is exactly why the
        // real search multiplies by the other magnitude.
        let mut local = 0;
        while local < OCTANT_STEPS && i64::from(sin_raw(local + 1)) <= folded.v {
            local += 1;
        }
        unfold(&folded, local)
    }

    #[test]
    fn probe_a_shifted_octant_fails_the_round_trip() {
        let error = check_round_trip(atan2_with_a_shifted_octant)
            .expect_err("an octant mapping off by one passed the round trip");
        // The message has to name the damage, or the probe would stay green on
        // a check that broke for an unrelated reason.
        assert!(
            error.contains("0 -> 1"),
            "the round trip failed for the wrong reason: {error}"
        );
    }

    #[test]
    fn probe_an_inverted_swap_fails_the_reference_directions() {
        let error = check_reference_directions(atan2_with_an_inverted_swap)
            .expect_err("an inverted swap condition passed the reference directions");
        assert!(
            error.contains("the positive x axis gave 256, expected 0"),
            "the reference directions failed for the wrong reason: {error}"
        );
    }

    #[test]
    fn probe_a_lost_magnitude_fails_the_monotonicity() {
        let error = check_monotone_octants(atan2_without_the_magnitude)
            .expect_err("dropping the magnitudes passed the monotonicity check");
        assert!(
            error.contains("octant 3"),
            "the monotonicity failed for the wrong reason: {error}"
        );
    }

    #[test]
    fn probe_a_magnitude_used_as_a_ratio_fails_the_scale_invariance() {
        let error = check_scale_invariance(atan2_from_the_magnitude_alone)
            .expect_err("comparing a bare magnitude passed the scale invariance");
        assert!(
            error.contains("scaled by"),
            "the scale invariance failed for the wrong reason: {error}"
        );
    }
}
