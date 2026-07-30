# Fablegen

A deterministic artificial-life simulation whose entire history is designed to
be written to Solana, linked together by a hash chain, and recomputed
independently by anyone who wants to check it.

**Status: early work in progress. Nothing is running on chain yet.** What
exists today is the determinism scaffolding: the crate skeleton, the rules the
simulation must obey, and the CI that enforces them. See
[ROADMAP.md](ROADMAP.md) for what comes next.

## Who builds it

The code in this repository is written by **Claude Opus 5**. Architecture and
review are handled by **Claude Fable 5**. Product decisions and acceptance are
made by a human. The name of the project is a nod to Fable.

Every commit here carries a `Model:` trailer naming the model it was produced
under. That records which model did the work; it is bookkeeping, not proof.
A commit hash attests that a tree has not changed since it was written, and
nothing about who wrote it.

This project is not affiliated with Anthropic and is not an Anthropic product.

---

## What it is

A population of small creatures lives in a closed world. Each carries a genome
of sixteen bytes, and those same bytes both drive its behaviour and draw its
body. Creatures spend energy to move and to sense, eat, reproduce, and die.

The simulation is deterministic to the bit. Given the same seed, it produces
the same history everywhere: on any machine, on any operating system, under any
build profile. That is not a nice property to have, it is the entire point.
Details and the invariants that protect it are in
[DETERMINISM.md](DETERMINISM.md).

The intended design is that every epoch is written to Solana as a memo, each
entry carrying the hash of the one before it. Take the published code, read the
log back from the chain, replay it, and compare. If the results diverge, the
project is caught.

Being a public log on a public chain means tampering is **detectable**, not
impossible: the key that writes those entries is held by the project. That
distinction is deliberate and is not going to be smoothed over in the copy.

## Three things that make it worth building

**Nothing defines what it means to succeed.** There is no fitness function.
The only thing imposed is the cost of existing: energy drains every tick and
food is finite. Metabolism, predation, caution, how fast a lineage mutates —
none of that is programmed. It either appears or it does not. Fitness is not
computed, it is observed afterwards, as the number of offspring left behind.

**The mutation rate evolves along with everything else.** One byte of the
genome sets how often that genome mutates, and that byte mutates at its own
rate. The population tunes how fast it changes, without anyone reaching in.

**The randomness cannot be ground for a good outcome.** Each epoch draws its
randomness from the block hash of a slot announced before that slot exists.
There is nothing to search: a future block hash is unknown to everyone,
including us. The announcement and the slot both sit in the log, so the claim
is checkable rather than promised.

## How to check it yourself

Not yet possible, and saying otherwise would be the first broken claim in a
project whose whole pitch is that its claims hold.

When there is a log to check, verification will consist of: taking this
repository at a tagged commit, reading the memo log back from the chain,
replaying the simulation from the announced seed, and comparing the per-epoch
hashes with the ones recorded on chain. The command to do it will be published
here and will be copy-pasteable.

Until then, what can be checked is the code: it is all here, including the CI
gates and the self-test that proves those gates actually reject violations
instead of merely looking green.

## What that does not mean

Naming the models is a statement about how the work is produced, not a claim
that anything here is autonomous. The agent does not decide what the project
is, does not approve its own work, and does not act without a human asking it
to. Every change lands because a person asked for it and accepted the result.

## Licence

MIT. See [LICENSE](LICENSE).
