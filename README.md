# RA2NE — Red Alert 2 New Engine

A modern, high-performance RTS runtime that reads users' own Red Alert 2 / Yuri's Revenge assets, maps and rules while preserving the classic ecosystem.

## Current stage: Phase 0 / 1

This first version deliberately has no EA assets, renderer, or MOD dependency. It establishes the new engine chassis first:

- fixed 30 TPS simulation
- integer coordinates and deterministic RNG
- reproducible world-state hash
- clamped, integer-only movement primitive (no goal overshoot)
- spatial broad phase with deterministic neighbourhood queries
- pure-simulation benchmark for 10,000 units
- shared reverse-BFS paths for batch movement on bounded walkability grids
- deterministic bridge entry limits and waiting/arrival/unreachable states
- tick-stamped command playback and checkpoint-based replay verification
- bounded transport-independent multiplayer input barrier
- versioned self-contained replay files with corruption checks and bounded decoding
- ordered INI syntax frontend with source diagnostics and layered in-memory virtual files
- MIX directory reading, encrypted headers, checksums, archive/loose overlays
- explicit UTF-8, Windows-1252 and GBK resource text decoding
- no rendering or complete original resource codecs yet

## Run

```bash
cargo test --workspace
cargo run -p ra2ne-bench --release -- --units=10000 --ticks=900
cargo run -p ra2ne-bench --release -- --units=10000 --ticks=900 --scenario=bridge
```

Replay determinism smoke check (reference log versus lockstep input and file round trip):

```bash
cargo run -p ra2ne-bench --release -- --units=10000 --ticks=900 --replay-check
```

Save and replay a recording (output refuses to overwrite an existing file):

```bash
cargo run -p ra2ne-bench --release -- --replay-check --replay-output=session.rpl
cargo run -p ra2ne-bench --release -- --replay-input=session.rpl
```

Replay format v1 is for the current abstract simulation. Its checksum detects
corruption; it does not authenticate recordings. Changing simulation semantics
requires a format-version change. Network transport and playable multiplayer
are still pending.

Synthetic asset frontend smoke check (no original assets required):

```bash
cargo run -p ra2ne-bench --release -- --asset-check
```

The INI frontend currently accepts UTF-8/ASCII, semicolon comments, repeated
sections and last-value duplicate lookup. Registry entries retain source order.
These are provisional frontend semantics pending original-game fixtures; parsed
keys are not yet applied to unit/gameplay rules. VFS mounts replace whole files;
INI rule merging across files is a separate future rules-loader operation.

Inspect your own resource files without modifying them:

```bash
cargo run -p ra2ne-inspect --release -- ini /path/to/rulesmd.ini --encoding=gbk
cargo run -p ra2ne-inspect --release -- mix /path/to/ra2md.mix
cargo run -p ra2ne-inspect --release -- mix /path/to/ra2md.mix --nested=localmd.mix --file=rulesmd.ini
cargo run -p ra2ne-bench --release -- --mix-check
```

Nested names depend on the actual archive. No archive extraction is needed for
inspection. [Format notes](docs/FORMAT_NOTES.md) record supported variants and
remaining compatibility work.

## Project principles

1. Preserve external content compatibility; do not preserve old internals.
2. Keep the compatibility frontend strictly separate from the modern runtime.
3. Implement Ares/Phobos INI semantics over time; never load legacy DLL hooks.
4. Parallel work must commit deterministically.
5. Core hot paths must not default to O(N²).

See [development status](docs/STATUS.md) for verified progress and remaining 1.0 scope.
