# RA2NE — Red Alert 2 New Engine

A modern, high-performance RTS runtime that reads users' own Red Alert 2 / Yuri's Revenge assets, maps and rules while preserving the classic ecosystem.

## Current stage: Phase 0 / 1

This first version deliberately has no EA assets, renderer, or MOD dependency. It establishes the new engine chassis first:

- fixed 30 TPS simulation
- integer coordinates and deterministic RNG
- reproducible world-state hash
- pure-simulation benchmark for 10,000 units
- no rendering, assets, or compatibility code yet

## Run

```bash
cargo test --workspace
cargo run -p ra2ne-bench --release -- --units=10000 --ticks=900
```

## Project principles

1. Preserve external content compatibility; do not preserve old internals.
2. Keep the compatibility frontend strictly separate from the modern runtime.
3. Implement Ares/Phobos INI semantics over time; never load legacy DLL hooks.
4. Parallel work must commit deterministically.
5. Core hot paths must not default to O(N²).
