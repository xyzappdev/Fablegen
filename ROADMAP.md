# Roadmap

What will be built, in roughly the order it will be built. No dates: this is a
list of things that have to be true, not a schedule, and a date here would be a
promise the repository cannot keep.

Each item is done when it is demonstrable, not when it is written.

---

## Foundations

- [x] Repository, workspace and CI that fails on any compiler or linter warning
- [x] Gates that reject floating point and system randomness in the simulation
      sources, and that prove on every run that they still reject them
- [ ] Fixed-point arithmetic, Q10, with tests on the boundaries and on overflow
- [ ] An angle table of 1024 divisions, generated once and committed as data
- [ ] An integer `atan2` returning an angle index, tested for monotonicity
- [ ] xoshiro256++ implemented explicitly, tested against known vectors
- [ ] Seeding the generator from 32 bytes of sha256

## The world

- [ ] Creatures, food, world state and the configuration that shapes them
- [ ] Decoding a genome from its sixteen bytes
- [ ] A toroidal world with neighbour lookup
- [ ] Sensing: nearest food, prey and threat, compared by squared distance
- [ ] Steering, movement paid for in energy, with quadratic costs
- [ ] Eating, attacking, and the three ways to die
- [ ] Reproduction: threshold, crossover, mutation
- [ ] An offline runner producing an event log
- [ ] A golden test: one seed, many epochs, one hash, compared to a reference

## Balance

- [ ] A batch runner across many seeds
- [ ] Per-epoch metrics: population, energy, age, births, causes of death
- [ ] A measure of genetic diversity over time
- [ ] Constants tuned until an ecosystem neither collapses nor saturates
- [ ] A long run that stays alive without pinning against the population cap

An ecosystem that dies in fifty ticks, or one that flatlines at its cap with no
selection pressure, is the expected first result. Getting past that is the
longest part of this list.

## On chain

- [ ] Memo encoding and chunking
- [ ] Per-epoch and chained hashes
- [ ] Seed commitment: announcing a future slot, then reading its block hash
- [ ] Handling the case where the announced slot is skipped
- [ ] A daemon that writes epochs continuously and recovers from a restart
- [ ] A sustained run on devnet

## Reading it back

- [ ] An indexer that reads the memo log and rebuilds world state from it
- [ ] Verification that the hash chain is unbroken across the whole range
- [ ] An independent verifier: replay the simulation, compare against the chain
- [ ] An HTTP API, so the front end never talks to an RPC node directly

## Seeing it

- [ ] A deterministic renderer drawing each creature from its sixteen bytes
- [ ] A live view of the world
- [ ] A card per creature: genome, decoded traits, a plain-language label
- [ ] Lineage: ancestry reconstructed from the log rather than stored
- [ ] A record of extinct lines: genome, cause of death, descendants left
- [ ] A verification page with a copy-pasteable command

## Before anything is called finished

- [ ] Public code at a tagged commit, with the constants it ran under
- [ ] Every claim on the site checked against what the chain can actually show
- [ ] Monitoring: liveness, wallet balance, an alert if the chain breaks
- [ ] A written procedure for what to do when it breaks
