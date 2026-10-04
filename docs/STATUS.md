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
- [x] Versioned replay files with bounded decoding and timing/command validation.
- [x] Transport-independent lockstep input barrier.
- [ ] Network transport, authenticated roster/ownership, disconnect policy and desync exchange.
- [ ] Local avoidance, physical occupancy, fair bottleneck queues, formation movement, dynamic terrain invalidation.
- [ ] Resource frontend: user-owned RA2/YR data, VFS, MIX archives and required codecs.
- [ ] Map and INI rules loading with explicit unsupported-feature diagnostics.
- [ ] Rendering, input, camera, audio and user interface.
- [ ] Units, combat, economy, buildings, triggers and skirmish AI.
- [ ] Save/load, multiplayer and replay.
- [ ] Compatibility fixtures for original maps and ordinary INI mods.
- [ ] Full-game performance and deterministic cross-platform verification.

1.0 is not complete. Ares/Phobos complete compatibility remains a later goal. No original game assets are distributed.

## 2026-10-04: transport-independent lockstep input barrier

Added fixed player rosters, explicit empty input frames, missing-player stalls,
canonical command ordering, idempotent retransmissions and atomic conflict
rejection. Input lead, commands per frame and selections are bounded by caller
limits. Authentication, unit ownership, timeout/disconnect policy, desync hash
exchange and network transport remain pending; this is not playable multiplayer.

Verification: 19 tests, Clippy and Release passed. The replay smoke check now
collects commands through a two-player barrier with reversed future arrival and
missing-frame assertions: 10,000 units / 900 ticks / 31 checkpoints match the
reference log, final hash 4c5d69f610b38d8a; two playbacks 351.768 ms. One-tick
boundary also passed. Bridge simulation 0.223 ms/Tick, hash c0c6b5571b31487c.

## 2026-10-04: self-contained replay persistence

Format v1 stores explicit initial units, complete abstract navigation map,
capacity limits, ordered commands, simulation rate and playback timing. Fixed
little-endian integer encoding and an FNV checksum detect accidental corruption.
Decoding bounds bytes (64 MiB), cells (1,048,576), units/commands (100,000 each)
and ticks (1,000,000), checks lengths before allocation, rejects unsupported
versions/rates, invalid IDs/goals/speeds and trailing data. Capacity sentinels
are normalized; actual cross-platform verification is still pending. No original
RA2 format or asset compatibility is claimed. The file version must change when
simulation semantics change. A checksum is not authentication, and CPU execution
budgets for untrusted recordings remain the embedding application's responsibility.

CLI supports --replay-output= and --replay-input=; output uses create-new to
preserve existing files. Verification: 22 tests, Clippy, Release passed; all
single-byte corruptions and all truncations of a fixture are rejected. 10,000
units / 900 ticks: reference and decoded lockstep recording agree at 31
checkpoints, hash 4c5d69f610b38d8a, total verification 358.935 ms. A separate
process read the persisted file with the same hash. One-tick boundary passed.

## 2026-10-04: compatibility frontend primitives

Added separate ra2ne-assets crate. INI parsing retains ordered source entries,
line numbers, unknown fields and duplicate-key diagnostics; lookup is ASCII
case-insensitive and uses the last duplicate value. Syntax errors explicitly
report invalid sections and entries. Values have optional checked integer,
boolean and comma-list conversion. UTF-8/ASCII only; legacy codepages, quoted
semicolon semantics, original-game duplicate behavior and schema validation
need future compatibility fixtures. No gameplay keys are implemented yet.

In-memory VFS layers use explicit caller-defined mount order and source labels.
Names normalize ASCII case and separators; invalid/traversal names and collisions
within a mount are rejected atomically. Later mounts replace entire files, not
individual INI keys. Disk adapters, MIX lookup/decryption and codecs are pending.

Verification: 27 workspace tests, Clippy and Release passed. --asset-check
parses 10,000 synthetic unit types / 40,000 entries in 19.623 ms and checks mod
file override and case-insensitive lookup. Bridge regression: 0.228 ms/Tick,
hash c0c6b5571b31487c. Lockstep/file round-trip regression: 31 checkpoints,
hash 4c5d69f610b38d8a, 359.082 ms total. Synthetic tests do not establish
original RA2/YR map or ordinary MOD compatibility. Next: loose-file inspection,
MIX archive frontend and authentic compatibility fixtures, alongside remaining
navigation/transport/gameplay milestones. 1.0 remains incomplete.
