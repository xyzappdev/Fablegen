//! The one source of randomness in the simulation: xoshiro256++, written out.
//!
//! Every operating-system entropy source is banned (DETERMINISM.md #3).
//! A run is described entirely by its seed, so the
//! generator has to be part of the code that is published and re-run, not a
//! library that might be a different version on someone else's machine.
//!
//! The algorithm is the reference one by David Blackman and Sebastiano Vigna,
//! <https://prng.di.unimi.it/xoshiro256plusplus.c> (public domain). It is
//! transcribed rather than depended upon, and pinned by the known-answer
//! vectors those authors published; see the tests.
//!
//! One state per run. Draws happen strictly in the order the tick loop
//! defines: one extra draw in an unexpected place shifts every later event.

/// Words of state the generator carries.
const STATE_WORDS: usize = 4;

/// Bytes of seed material a generator is built from: four words of eight.
///
/// This is the width of a sha256 digest, which is where the bytes are meant to
/// come from. Computing that digest is **not** this module's job: the seed
/// arrives already made, and where it comes from is decided elsewhere. This
/// crate knows nothing about the chain: it consumes a seed and produces a
/// stream of events, and who supplies the one or writes the other is not its
/// concern.
pub const SEED_BYTES: usize = STATE_WORDS * 8;

// The three constants of the algorithm. They are named rather than left in the
// body because they are the whole algorithm: change one and the generator is a
// different one that still looks right. A constant that decides behaviour
// belongs somewhere it can be named and checked, not buried in an expression.
// The negative probe in the tests changes one on purpose and requires the
// reference vectors to fail.
//
// Rotation applied to the sum that forms the output word.
const OUTPUT_ROTATION: u32 = 23;
// Shift that feeds the state mixing.
const MIX_SHIFT: u32 = 17;
// Rotation that closes the state update.
const STATE_ROTATION: u32 = 45;

/// xoshiro256++: the only generator the simulation is allowed to draw from.
#[derive(Clone, Debug)]
pub struct Rng {
    state: [u64; STATE_WORDS],
}

impl Rng {
    /// Creates a generator from a raw state; panics if the state is all zeroes.
    ///
    /// The all-zero state is **absorbing**: xoshiro maps it to itself, so the
    /// generator would return zeroes forever without any error. A run seeded
    /// that way would look finished rather than broken, and its log would be
    /// self-consistent nonsense. A panic is the cheaper failure.
    pub fn from_state(state: [u64; STATE_WORDS]) -> Self {
        if state == [0; STATE_WORDS] {
            panic!(
                "Rng::from_state: the all-zero state is absorbing, xoshiro256++ \
                 would emit zeroes forever"
            );
        }
        Self { state }
    }

    /// Creates a generator from 32 bytes of seed; panics if they are all zero.
    ///
    /// The bytes become the state through `state_from_seed`, so 32 zero bytes
    /// give the absorbing state and are refused exactly like it.
    pub fn from_seed(seed: &[u8; SEED_BYTES]) -> Self {
        Self::from_state(state_from_seed(seed))
    }

    /// Returns the next 64 bits and advances the state.
    ///
    /// Every addition is `wrapping_*` on purpose: wrapping here is the
    /// algorithm, not an accident, and writing it out keeps the result
    /// identical under every build profile.
    pub fn next_u64(&mut self) -> u64 {
        let s = &mut self.state;

        let result = s[0]
            .wrapping_add(s[3])
            .rotate_left(OUTPUT_ROTATION)
            .wrapping_add(s[0]);

        // A left shift discards the high bits by definition, which is what the
        // algorithm asks for; unlike an addition it cannot overflow, so no
        // build profile can make this line behave differently.
        let t = s[1] << MIX_SHIFT;

        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];

        s[2] ^= t;

        s[3] = s[3].rotate_left(STATE_ROTATION);

        result
    }
}

/// Reads 32 seed bytes as four `u64` words, little-endian, eight bytes each in
/// order.
///
/// Written with `from_le_bytes` rather than a cast, so the result is the same
/// on a big-endian machine as on a little-endian one. A seed that decoded
/// differently per architecture would give one seed two histories, which is
/// the one thing this project cannot have.
pub fn state_from_seed(seed: &[u8; SEED_BYTES]) -> [u64; STATE_WORDS] {
    let mut state = [0_u64; STATE_WORDS];

    for (word, chunk) in state.iter_mut().zip(seed.chunks_exact(8)) {
        let mut bytes = [0_u8; 8];
        bytes.copy_from_slice(chunk);
        *word = u64::from_le_bytes(bytes);
    }

    state
}

#[cfg(test)]
mod tests {
    use super::{
        state_from_seed, Rng, MIX_SHIFT, OUTPUT_ROTATION, SEED_BYTES, STATE_ROTATION, STATE_WORDS,
    };
    use std::hint::black_box;
    use std::panic;

    // Known-answer vectors. These are NOT produced by the code they test, and
    // that is the entire point: a vector computed by our own implementation and
    // then called a reference proves only that the code agrees with itself --
    // the same emptiness as a CI gate that checks nothing.
    //
    // Source: the `reference` test of the `rand_xoshiro` crate
    // (rust-random/rngs, src/xoshiro256plusplus.rs), whose own comment records
    // where the numbers came from: "These values were produced with the
    // reference implementation: http://xoshiro.di.unimi.it/xoshiro256plusplus.c"
    // -- that is, Blackman and Vigna's published C, not a re-derivation.
    //
    // Cross-checked three ways before being written down here:
    //   1. two independent mirrors of that crate's source list the identical
    //      ten values for the identical seed;
    //   2. the first two draws were recomputed by hand from the authors' own C
    //      at <https://prng.di.unimi.it/xoshiro256plusplus.c>, which this
    //      module transcribes: from the state [1, 2, 3, 4] the first output is
    //      rotl(1 + 4, 23) + 1 = 5 * 2^23 + 1 = 41943041, and the second works
    //      out to 58720359, both matching below;
    //   3. the state itself is pinned by those values rather than assumed.

    /// The reference state: the seed bytes of the source test, read little-endian.
    const REFERENCE_STATE: [u64; STATE_WORDS] = [1, 2, 3, 4];

    /// The first ten words the reference implementation emits from that state.
    const REFERENCE_OUTPUT: [u64; 10] = [
        41_943_041,
        58_720_359,
        3_588_806_011_781_223,
        3_591_011_842_654_386,
        9_228_616_714_210_784_205,
        9_973_669_472_204_895_162,
        14_011_001_112_246_962_877,
        12_406_186_145_184_390_807,
        15_849_039_046_786_891_736,
        10_450_023_813_501_588_000,
    ];

    /// Compares a produced sequence against the published reference vectors.
    ///
    /// Takes the sequence as an argument rather than generating it, so the
    /// probes below can hand it the output of a deliberately altered generator
    /// and require a failure.
    fn check_known_vectors(produced: &[u64]) -> Result<(), String> {
        if produced.len() < REFERENCE_OUTPUT.len() {
            return Err(format!(
                "only {} values produced, the reference lists {}",
                produced.len(),
                REFERENCE_OUTPUT.len()
            ));
        }

        for (draw, (&got, &want)) in produced.iter().zip(REFERENCE_OUTPUT.iter()).enumerate() {
            if got != want {
                return Err(format!(
                    "draw {draw}: produced {got}, reference says {want}"
                ));
            }
        }
        Ok(())
    }

    /// Requires a constructor to refuse the all-zero state by panicking.
    fn check_zero_state_is_refused(construct: fn([u64; STATE_WORDS])) -> Result<(), String> {
        let previous_hook = panic::take_hook();
        panic::set_hook(Box::new(|_| {}));
        let outcome = panic::catch_unwind(|| construct([0; STATE_WORDS]));
        panic::set_hook(previous_hook);

        if outcome.is_err() {
            return Ok(());
        }
        Err(
            "the all-zero state was accepted: xoshiro absorbs it, so the generator \
             would emit zeroes forever and the run would look finished rather than \
             broken"
                .to_string(),
        )
    }

    /// Draws `count` words from a generator.
    fn draws_from(mut rng: Rng, count: usize) -> Vec<u64> {
        (0..count).map(|_| rng.next_u64()).collect()
    }

    #[test]
    fn the_generator_matches_the_published_reference_vectors() {
        let produced = draws_from(Rng::from_state(REFERENCE_STATE), REFERENCE_OUTPUT.len());
        check_known_vectors(&produced).expect("the shipped generator");
    }

    #[test]
    fn the_first_reference_value_is_reproducible_by_hand() {
        // Spelled out so the reference vectors are not one opaque block that
        // has to be trusted whole: from [1, 2, 3, 4] the algorithm's first
        // output is rotl(s0 + s3, 23) + s0, and every term here is small
        // enough to check without a computer.
        let by_hand = 5_u64.rotate_left(OUTPUT_ROTATION).wrapping_add(1);
        assert_eq!(by_hand, 5 * (1 << 23) + 1);
        assert_eq!(by_hand, REFERENCE_OUTPUT[0]);
    }

    #[test]
    fn the_same_state_gives_the_same_sequence() {
        let left = draws_from(Rng::from_state([7, 11, 13, 17]), 64);
        let right = draws_from(Rng::from_state([7, 11, 13, 17]), 64);
        assert_eq!(left, right);
    }

    #[test]
    fn a_different_state_diverges_immediately() {
        let left = draws_from(Rng::from_state([7, 11, 13, 17]), 64);
        // One bit apart in one word.
        let right = draws_from(Rng::from_state([7, 11, 13, 16]), 64);

        assert_ne!(left[0], right[0], "the first draw already has to differ");
        let shared = left
            .iter()
            .zip(right.iter())
            .filter(|(a, b)| a == b)
            .count();
        assert!(
            shared < 4,
            "{shared} of 64 draws coincided, which is not two diverging streams"
        );
    }

    #[test]
    fn the_arithmetic_wraps_instead_of_panicking() {
        // The profile guard. If any wrapping_add above were a plain +, this
        // would panic in the dev profile and, worse, wrap silently in release
        // with overflow-checks off -- one seed, two histories. CI runs the
        // suite under both profiles, so the reference vectors above are checked
        // twice over; this test makes the overflow itself certain to happen.
        let mut rng = Rng::from_state([u64::MAX; STATE_WORDS]);
        for _ in 0..10_000 {
            let _ = black_box(rng.next_u64());
        }
    }

    #[test]
    fn the_all_zero_state_is_refused() {
        check_zero_state_is_refused(|state| {
            let _ = Rng::from_state(state);
        })
        .expect("the shipped constructor");
    }

    #[test]
    #[should_panic(expected = "the all-zero state is absorbing")]
    fn the_all_zero_state_panics_with_a_message_that_says_why() {
        let _ = Rng::from_state([0; STATE_WORDS]);
    }

    // -- Seeding from 32 bytes -----------------------------------------------

    /// The 32 seed bytes of the source test, which decode to `REFERENCE_STATE`.
    ///
    /// Written out as the source lists them, so the layout the vectors were
    /// produced under is pinned here and not merely assumed.
    const REFERENCE_SEED: [u8; SEED_BYTES] = [
        1, 0, 0, 0, 0, 0, 0, 0, // word 0, little-endian
        2, 0, 0, 0, 0, 0, 0, 0, // word 1
        3, 0, 0, 0, 0, 0, 0, 0, // word 2
        4, 0, 0, 0, 0, 0, 0, 0, // word 3
    ];

    /// A byte-to-state decoder, so a probe can supply one with the wrong order.
    type Decoder = fn(&[u8; SEED_BYTES]) -> [u64; STATE_WORDS];

    /// Checks that a decoder reads the bytes as four little-endian words.
    ///
    /// The expected words are written out by hand from the byte order, not
    /// produced by calling the function under test: 0x00 through 0x1F laid down
    /// in ascending order become words whose *last* byte is the lowest-numbered
    /// one. A decoder checked against its own output would agree with itself in
    /// any byte order at all.
    fn check_seed_layout(decode: Decoder) -> Result<(), String> {
        let mut ascending = [0_u8; SEED_BYTES];
        for (index, byte) in ascending.iter_mut().enumerate() {
            *byte = u8::try_from(index).expect("32 fits in a byte");
        }

        let cases = [
            (
                "ascending bytes",
                ascending,
                [
                    0x0706_0504_0302_0100_u64,
                    0x0F0E_0D0C_0B0A_0908,
                    0x1716_1514_1312_1110,
                    0x1F1E_1D1C_1B1A_1918,
                ],
            ),
            ("the reference seed", REFERENCE_SEED, REFERENCE_STATE),
        ];

        for (name, seed, expected) in cases {
            let decoded = decode(&seed);
            for (word, (&got, &want)) in decoded.iter().zip(expected.iter()).enumerate() {
                if got != want {
                    return Err(format!(
                        "{name}, word {word}: decoded {got:#018x}, expected {want:#018x}"
                    ));
                }
            }
        }
        Ok(())
    }

    #[test]
    fn the_seed_decodes_little_endian() {
        check_seed_layout(state_from_seed).expect("the shipped decoder");
    }

    #[test]
    fn the_reference_seed_drives_the_reference_vectors() {
        // The end-to-end check: the published 32 bytes, through the decoder and
        // the generator, must give the published words. This is what ties the
        // byte layout to vectors produced outside this repository -- a wrong
        // layout would still decode "some" state and produce plausible noise.
        let produced = draws_from(Rng::from_seed(&REFERENCE_SEED), REFERENCE_OUTPUT.len());
        check_known_vectors(&produced).expect("the shipped seeding");
    }

    #[test]
    #[should_panic(expected = "the all-zero state is absorbing")]
    fn thirty_two_zero_bytes_reach_the_absorbing_state_and_panic() {
        // Zero bytes decode to the zero state, so seeding inherits the guard
        // rather than needing a second one.
        assert_eq!(state_from_seed(&[0; SEED_BYTES]), [0; STATE_WORDS]);
        let _ = Rng::from_seed(&[0; SEED_BYTES]);
    }

    // -- Negative probes -----------------------------------------------------

    /// The generator with the output rotation moved by one step.
    ///
    /// A transcription slip of exactly this size is the realistic way to get
    /// this algorithm wrong: it still produces plausible-looking noise.
    fn sequence_with_a_changed_rotation(state: [u64; STATE_WORDS], count: usize) -> Vec<u64> {
        let mut s = state;
        let mut produced = Vec::with_capacity(count);

        for _ in 0..count {
            // The reference rotates by OUTPUT_ROTATION here.
            let result = s[0]
                .wrapping_add(s[3])
                .rotate_left(OUTPUT_ROTATION + 1)
                .wrapping_add(s[0]);
            let t = s[1] << MIX_SHIFT;
            s[2] ^= s[0];
            s[3] ^= s[1];
            s[1] ^= s[2];
            s[0] ^= s[3];
            s[2] ^= t;
            s[3] = s[3].rotate_left(STATE_ROTATION);
            produced.push(result);
        }

        produced
    }

    #[test]
    fn probe_a_changed_rotation_fails_the_known_vectors() {
        let produced = sequence_with_a_changed_rotation(REFERENCE_STATE, REFERENCE_OUTPUT.len());
        let error = check_known_vectors(&produced)
            .expect_err("a generator with the wrong rotation matched the reference vectors");
        // The message has to name the damage, or the probe would stay green on
        // a check that broke for an unrelated reason.
        assert!(
            error.starts_with("draw 0: produced 83886081, reference says 41943041"),
            "the known vectors failed for the wrong reason: {error}"
        );
    }

    /// The decoder reading the bytes the other way round.
    ///
    /// Big-endian is the realistic slip here: both orders decode 32 bytes into
    /// four words without complaint, and both look right until the numbers are
    /// compared with something produced elsewhere.
    fn state_from_seed_big_endian(seed: &[u8; SEED_BYTES]) -> [u64; STATE_WORDS] {
        let mut state = [0_u64; STATE_WORDS];
        for (word, chunk) in state.iter_mut().zip(seed.chunks_exact(8)) {
            let mut bytes = [0_u8; 8];
            bytes.copy_from_slice(chunk);
            // The real decoder reads little-endian here.
            *word = u64::from_be_bytes(bytes);
        }
        state
    }

    #[test]
    fn probe_a_big_endian_decoder_fails_the_seed_layout() {
        let error = check_seed_layout(state_from_seed_big_endian)
            .expect_err("a big-endian decoder passed the little-endian layout check");
        assert!(
            error.starts_with("ascending bytes, word 0: decoded 0x0001020304050607"),
            "the seed layout failed for the wrong reason: {error}"
        );
    }

    #[test]
    fn probe_a_dropped_zero_check_fails_the_zero_state_check() {
        let error = check_zero_state_is_refused(|state| {
            // Straight past the guard in `from_state`, which is what a
            // constructor written in a hurry would look like.
            let _ = Rng { state };
        })
        .expect_err("a constructor without the guard passed the zero-state check");
        assert!(
            error.contains("the all-zero state was accepted"),
            "the zero-state check failed for the wrong reason: {error}"
        );
    }
}
