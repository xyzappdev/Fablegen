# Determinism

The whole project rests on one property: **one seed produces one bit-identical
result, on any machine, on any operating system, under any build profile.**

Everything below exists to protect that property. If any of it is broken, the
log stops being reproducible and the project stops being checkable.

Changing these invariants means changing the simulation's output. Once a
reference log exists, any such change must break the golden test.

---

## 1. One seed, one history

A run is fully described by its seed and the code that produced it. Nothing is
read from the clock, the environment, the file system layout, the thread
scheduler, or anything else outside the seed.

## 2. No floating point

`f32` and `f64` are banned inside the simulation. Operation order changes after
optimisation, fused multiply-add behaves differently across targets, and libm
implementations disagree in the last bits.

Arithmetic is fixed-point, `SCALE = 1024`, that is Q10:

```
world coordinate 1.5  ->  1536 internal units
multiply:  (a * b) >> 10
divide:    floor((a << 10) / b)
```

Values are `i32`. Intermediate products widen to `i64` and narrow back, so a
multiplication cannot overflow silently on the way.

Rounding is **floor everywhere**, toward negative infinity. The shift operator
already rounds that way while integer division in Rust truncates toward zero,
so floor division is written out explicitly rather than inherited from the
operator. Two rounding modes in one module would be a guaranteed divergence.
The rule is frozen once the first reference log exists: changing it would move
every value of every history.

Overflow **panics**. It neither saturates nor wraps. Saturation would distort
the history quietly and stay invisible to the golden test, because the log
would go on agreeing with itself while being wrong; a panic stops the run while
the mistake can still be seen. The fixed-point type checks every operation
itself instead of relying on the `overflow-checks` setting of a build profile,
which is a setting somebody can drop without noticing. See section 7.

Floating point is permitted only where it cannot reach the log: rendering in
the browser, tuning charts, test utilities.

## 3. No system randomness

`rand()`, `Math.random()`, `std::random` and every operating-system entropy
source are banned.

The generator is **xoshiro256++**, implemented explicitly inside the project,
`wrapping` arithmetic on `u64`. One state per run. Calls happen strictly in the
order the tick loop defines.

It is seeded from 32 bytes of sha256, read as four little-endian `u64`.

One extra generator call in an unexpected place shifts every subsequent event.
Change code that draws from the generator and the golden test must fail; if it
does not, the test is a bad test.

## 4. No standard-library trigonometry

`sin`, `cos` and `atan2` from the standard library are banned: their last bits
differ between implementations.

Instead:

- a table of **1024 divisions** of the circle, `i32` values, precomputed and
  committed to the repository as data rather than computed at startup;
- an integer `atan2` returning an angle index in `0..1023`, by octant reduction
  plus a binary search over that same committed sine table;
- distances are compared **squared**. No square root is needed anywhere.

**`atan2` has no table of its own**, and that is not about saving space. One set
of numbers is one thing to keep correct; two would drift apart silently. The
agreement between `atan2` and `sin`/`cos` holds **by construction** rather than
by coincidence: the search compares the cross products `SIN[m] * |x|` against
`COS[m] * |y|` in `i64` -- the same ordering as `tan(m) <= |y| / |x|`, without
the division -- and the cosine comes from the same table, a quarter turn ahead.
So `atan2(sin(i), cos(i))` is `i` **exactly**, for all 1024 indices, and a test
says so.

`atan2(0, 0)` returns 0. That is a **convention, not a result**: the origin has
no direction and every other answer is equally arbitrary. It is written down
because an undocumented arbitrary value is the kind of thing somebody later
"fixes", and that would move every history ever produced.

**A direction lying between two tabulated ones resolves to the tabulated one on
the side of the nearer axis. The rounding is mirrored about the axes rather
than uniform around the circle.**

The reason is the same oddness the table itself is built for. Mirrored rounding
makes `atan2(-y, x) == -atan2(y, x)` hold **exactly**, for every direction and
not only for the tabulated ones: reflecting about an axis and rounding commute.
A uniform floor counterclockwise breaks that oddness and buys nothing for it --
it drags **every** direction the same way round, a constant error with a
non-zero mean, where mirrored rounding errs symmetrically and drifts nowhere on
average. Ties away from zero was chosen for the table itself by exactly the
same argument.

This is **frozen with the first reference log**, like the floor rule and the
table's rounding.

The table is rounded to the **nearest** integer, with ties **away from zero**.
That is not an exception to the floor rule of section 2 but a different layer:
floor governs the fixed-point arithmetic of the hot path, while this table is
computed offline, once, and frozen as data -- nothing recomputes it at run time.
Ties away from zero is the choice because it negates cleanly: rounding `-x`
gives exactly minus the rounding of `x`, and that is what makes the table
**exactly** antisymmetric, `SIN[(1024 - i) % 1024] == -SIN[i]`. Floor would sag
the whole table toward negative infinity and lose that symmetry.

The generator lives in `tools/gen-trig-table/`, outside the simulation crate,
because computing a sine needs the floating point that `sim/` bans outright.
Regenerate the committed file with:

```
cargo run -p gen-trig-table > sim/src/trig_table.rs
```

The table is checked by **two independent mechanisms**: integer invariants
inside the simulation (quarter points, antisymmetry, monotonicity per quarter,
range, `sin^2 + cos^2`), and an exact comparison against a freshly computed
table in the generator's own tests. Both sides carry negative probes that hand
the check a corrupted copy and require it to fail, for the same reason the CI
gates self-test on every run: a check nobody has seen fail is not known to check
anything.

## 5. Fixed iteration order

Iterating over hash maps or sets is banned wherever order can affect the
result. Arrays and explicit sorting by id only.

Creatures are always walked in ascending id order. No exceptions.

## 6. No parallelism inside a run

Tuning batches may run in parallel **across runs**, each in its own thread with
its own state. Within a single run, execution is strictly sequential.

## 7. Overflow is checked in every build profile

`overflow-checks` is switched on for the release profile in the root manifest.
Without it, debug panics on overflow while release wraps silently, and one seed
would produce two different histories depending on how the binary was built.

Wrapping is legal only where it is intended, and only through explicit
`wrapping_*` calls.

CI runs the test suite under both profiles, and a unit test asserts that
overflow still panics in whichever profile it was compiled under.

## 8. The golden test

Mandatory, and run on every commit:

```
fixed seed -> N epochs -> sha256 of the whole event log
           -> compared against the reference committed to this repository
```

A failure means either determinism was broken or the mechanics were changed
deliberately. In the second case the reference is updated in a separate commit
that says so.

---

## How this is enforced

Not by discipline. By the toolchain:

- `#![forbid(unsafe_code)]` and two `deny` attributes on the crate;
- a clippy configuration that rejects the floating-point primitives outright;
- CI gates that grep the sources for floating point, system randomness and the
  wall clock, because a linter only sees compiled code while a grep also sees
  string literals, macros and commented-out drafts;
- a self-test that plants deliberate violations before every real gate run and
  fails the build if the gate does not catch them. A green gate over a clean
  tree is otherwise indistinguishable from a gate that checks nothing.
