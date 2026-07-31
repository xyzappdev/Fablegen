// Red lines enforced by the compiler rather than by review. `unsafe` would open
// the door to undefined behaviour and irreproducible results; the two extra
// denials keep the simulation in integer arithmetic. See DETERMINISM.md and
// clippy.toml.
#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]
#![deny(clippy::disallowed_types)]

//! Deterministic simulation core of Fablegen.
//!
//! One seed produces one bit-identical result on any machine, on any operating
//! system, under any build profile. Everything else in this crate is
//! subordinate to that: fixed-point instead of floating point, an explicit
//! xoshiro256++ instead of system randomness, iteration over creatures by
//! ascending id instead of hash maps.
//!
//! The crate knows nothing about the chain. It produces a stream of events;
//! who writes that stream and where is not its concern.
//!
//! For now it carries the prohibitions and fixed-point arithmetic. The angle
//! table, an integer `atan2` and the generator follow.

// Fixed-point Q10: the only arithmetic the simulation is allowed to perform.
// Public because a private module would make every item in it dead code until
// the first consumer arrives, and a warning is an error that has not fired yet.
pub mod fixed;

#[cfg(test)]
mod tests {
    use std::hint::black_box;
    use std::panic;

    /// Overflow must panic under every build profile.
    ///
    /// Drop `overflow-checks` from `[profile.release]` and release starts
    /// wrapping `i32` silently while debug keeps panicking, so one seed yields
    /// two different histories. CI runs the tests under both profiles for
    /// exactly this reason.
    #[test]
    fn overflow_panics_in_every_profile() {
        // `black_box` hides the operands from const propagation. Without it the
        // overflow would be caught by the compiler rather than at run time, and
        // the test would prove nothing about the running binary.
        let previous_hook = panic::take_hook();
        panic::set_hook(Box::new(|_| {}));
        let outcome = panic::catch_unwind(|| black_box(i32::MAX) + black_box(1));
        panic::set_hook(previous_hook);

        assert!(
            outcome.is_err(),
            "i32::MAX + 1 did not panic: overflow checks are off in this \
             profile, and determinism between debug and release is no longer \
             guaranteed"
        );
    }
}
