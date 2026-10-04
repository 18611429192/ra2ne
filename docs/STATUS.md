# RA2NE development status

## 2026-10-04: batch navigation restored on remote baseline

Remote main was 6e59dac. Prior session-local commits could not be recovered from this workspace; this implementation is new work on the actual remote baseline.

Implemented: bounded four-way walkability grid; one reverse BFS shared per movement command; atomic command validation; idle/moving/waiting/arrived/unreachable status; deterministic unit-ID priority for limited cell entries; route topology and speed included in state hashes.

Verified with Rust 1.99.0, x86_64 Linux:
- cargo test --workspace: 11 passed.
- cargo clippy --workspace --all-targets -- -D warnings: passed.
- Release builds passed.
- 10,000 units, 900 ticks, bridge: 0.231 / 0.225 ms per tick; hash c0c6b5571b31487c both runs; planning 1.402 / 1.133 ms.
- Random straight movement: 1.109 ms per tick; hash e859424054a3c7c8.

These timings measure this machine and simulation only. They are not comparisons with the original game. Bridge capacity limits entries per tick, not physical occupancy; units can overlap. Routes snapshot the map and require reissuing after terrain changes. Finite groups drain in stable ID order; continuous new traffic needs a fair queue. Navigation uses abstract grid cells, not original RA2 map coordinates.

## 2026-10-04: deterministic command playback foundation

Commands execute before their named zero-based tick, ordered by (tick, player, sequence). Unit selections are sorted and deduplicated. Duplicate keys, invalid commands, invalid checkpoint intervals and commands outside the playback window are rejected. Playback starts from explicit units and a navigation map and emits periodic world-hash checkpoints.

Verified after a clean build:
- cargo test --workspace: 15 passed.
- cargo clippy --workspace --all-targets -- -D warnings: passed.
- Release replay check: 10,000 units, 900 ticks, 31 checkpoints matched with reversed command arrival; final hash 4c5d69f610b38d8a; both playbacks including planning/checkpoint hashing took 376.934 ms total.

This is an in-memory replay foundation. Persistence, network transport, player ownership checks, initial-state/map identity validation and cross-platform verification are still pending. The old local-only commits remain unrecovered.

## Remaining 1.0 acceptance scope

- [x] In-memory tick-stamped movement commands and replay checkpoint verification.
- [ ] Replay file format, metadata validation and network lockstep.
- [ ] Local avoidance, physical occupancy, fair bottleneck queues, formation movement, dynamic terrain invalidation.
- [ ] Resource frontend: user-owned RA2/YR data, VFS, MIX archives and required codecs.
- [ ] Map and INI rules loading with explicit unsupported-feature diagnostics.
- [ ] Rendering, input, camera, audio and user interface.
- [ ] Units, combat, economy, buildings, triggers and skirmish AI.
- [ ] Save/load, multiplayer and replay.
- [ ] Compatibility fixtures for original maps and ordinary INI mods.
- [ ] Full-game performance and deterministic cross-platform verification.

1.0 is not complete. Ares/Phobos complete compatibility remains a later goal. No original game assets are distributed.
