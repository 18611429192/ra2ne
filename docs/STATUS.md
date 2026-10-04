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

## 2026-10-04: MIX resources and inspection entry point

Implemented legacy and extended MIX directories, RA2/YR padded CRC32 and classic
filename IDs, RSA-derived Blowfish encrypted headers, SHA-1 body verification,
zero-copy archive entry views, nested archive inspection and atomic loose-file
imports. MIX/loose mounts share one explicit overlay order. Added explicit
UTF-8, Windows-1252 and GBK decoding, with invalid-input diagnostics. Added
ra2ne-inspect ini/mix CLI. No original resources are bundled or used in these
checks; XCC name databases and automatic original-game mount policy remain
pending. See FORMAT_NOTES.md for source references and format boundaries.

Verification: 36 workspace tests, Clippy and Release passed. CLI read an
independently generated nested encrypted/checksummed MIX and its rulesmd.ini,
and decoded Chinese GBK INI. --mix-check: 10,000 entries parse 1.379 ms, all
10,000 named lookups 3.543 ms. Bridge: 0.216 ms/Tick, unchanged hash
c0c6b5571b31487c. Replay/file/lockstep: 31 checkpoints, unchanged hash
4c5d69f610b38d8a, verification 361.427 ms. Original assets, maps, rendering
and gameplay remain required for 1.0 acceptance.

## 2026-10-04: bounded maps and indexed rule overlays

Added bounded numeric Base64 packs, LZO1X terrain and LCW overlays; map Size,
LocalSize/Theater metadata, eleven-byte terrain cells, waypoints and placed
vehicle/infantry/aircraft/building records. Complete INI and unconsumed object
fields remain available. Pending trigger/team/script/lighting sections report
diagnostics. Added per-key rule layers with source provenance, registry order,
indexed property/type lookup, exact decimals and initial type/weapon discovery.
Unknown MOD fields are preserved and reported. Inspector supports map and rules
modes, map files inside MIX, and ordered --overlay= paths. This loads data;
terrain passability and original gameplay semantics still need implementation.

Verification: 44 workspace tests, Clippy and Release passed. Independent literal
LZO map fixture was read by CLI. A map-style rule overlay modified type Strength
and produced a source/line diagnostic for an unknown MOD flag. --rules-check:
10,000 types with a shared weapon loaded in 89.069 ms, retaining registry order
and the final type's overridden Strength. Source facts and limitations are
recorded in FORMAT_NOTES.md. No original asset fixture was available here.

## Interactive and game-system foundation (not 1.0 acceptance)

- `ra2ne-runtime`: fixed 30-Tick simulation with an isometric interactive renderer,
  selection, group movement, pause, camera pan/zoom, minimap and diagnostics.
  Map inputs remain a data viewer: original terrain passability and gameplay are
  not inferred from tile identifiers. Headless mode runs without a window.
- SHP TS raw/RLE indexed frame and six-bit PAL decoding, plus TMP TS diamond
  tile/extra/Z-plane decoding. Malformed offsets and output sizes are bounded.
  This does not yet implement original animation, voxel rendering or shadows.
- `ra2ne-game`: generation-checked entity handles, atomic owner-validated commands,
  spatial combat queries, simultaneous damage, production queues, credit
  reservations/refunds, power pause, blocked exits and winner detection.
  Initial population uses one spatial rebuild. The later update below adds
  harvesting, attack pursuit, draw handling, saves and runtime integration.
  Definitions still use explicit engine timings; original RA2 rules remain pending.
- Workspace tests and strict Clippy pass. Two headless runs with 10,000 units and
  900 Ticks produced the same hash `7497f64dc6800336`.
- A native window could not be visually validated in this environment because
  AF_UNIX sockets needed by the virtual display are denied. Compilation and
  headless tests do not count as graphical acceptance.

Original assets/maps/mods, complete skirmish, actual network transport and full
save/replay integration remain required before a 1.0 release.


## Full synthetic skirmish, saves and transport

- `--battle` integrates combat, production and the synthetic harvesting loop into
  the interactive runtime. Right click attacks an enemy cell or moves; `B` queues
  a tank. Health bars, resources, credits and power are visible. Physical unit
  occupancy/avoidance and original movement timing are still pending.
- Harvesters reserve no resources ahead of time: stable entity order gathers one
  unit per Tick, holds 30 units, returns and deposits at 25 credits/unit. Factories
  double as depots only in this synthetic fixture, not as original RA2 semantics.
- Attack pursuit groups paths by target; victory/draw freeze further simulation.
  The current elimination condition counts all actors. Original ShortGame rules,
  alliances, AI, base construction and sale/repair remain unimplemented.
- Engine saves embed definitions, map, unit generations, shared path fields,
  combat state, resources, harvesting and production. Loads are bounded and reject
  malformed topology, unknown versions, truncation, trailing bytes and hash
  mismatches. They do not read original RA2 saves.
- Full game commands have canonical ordering, bounded input barriers, explicit
  empty frames and deterministic rejected-command records. Full replay files
  embed an initial save. Input frames have a bounded binary wire format.
- Nonblocking TCP peers validate a session token, player identity, initial state
  hash and Tick. Queues and poll work are bounded. Actual loopback tests exercise
  fragmented messages, mismatched state, invalid lengths and input barriers.
  Lobby/UI integration, encryption, reconnect, internet sessions and multiplayer
  acceptance are pending.
- Linux software-rendered window verification completed: 120 frames, screenshot
  visually inspected, same state hash as headless at 120 Ticks. An initial black
  screenshot exposed a capture-after-swap bug, which was fixed and rechecked.
  Input interaction, hardware GPU and Windows rendering have not been validated.
- Continuous 900-Tick execution matches saving at Tick 450 and restoring for 450
  Ticks. Save tests also compare all events and hashes for 100 future Ticks and
  reject every truncation and single-byte corruption of a fixture.
- A sustained 10,000-unit/900-Tick combat fixture uses enough health to keep actors
  alive throughout: two runs have 31 identical checkpoints, approximately
  2.50 ms/Tick on this machine. The earlier 0.96 ms fixture included idle calls
  after combat ended and is not a sustained combat measurement.
- CI is configured for Linux, Windows and macOS tests, strict Clippy, headless
  runtime and deterministic combat benchmarks. Remote CI results are pending.

This remains a foundation rather than RA2NE 1.0. Original resource/theater/voxel/
map/rule behavior, ordinary mods, audio, AI, construction and complete multiplayer
still require implementation and original-game acceptance fixtures.

End-to-end TCP verification additionally ran two independent 2,000-unit games
through 900 Ticks over a real localhost connection. Each Tick's state matched;
encoding, decoding and replaying the recorded full-game commands produced the
same final hash `5a835421a4a3e97a` (147,993-byte replay fixture).

## 2026-10-04: layered rules now drive an experimental skirmish

Added `ra2ne-game::rule_import` and runtime `--rules-experiment` mode. Effective
INI overrides now supply health, cost, primary damage/range/reload, signed power
and checked harvester flags to actual engine definitions. Type registry order
and case-insensitive lookup are preserved. Movement and timing require explicit
caller calibration; fractional combat range, unsupported air movement, healing
damage, missing calibration and overflow fail instead of silently approximating.
Unapplied armor, warhead/projectile, secondary, prerequisites, ownership, sight,
images and factory categories report diagnostics. Buildings remain stationary
engine definitions without construction/footprint semantics. Factories are not
enabled by the importer until category restrictions exist. Full limits and CLI
examples are in RULE_EXPERIMENT.md; a synthetic INI fixture is redistributable.

Verified on Rust 1.99.0 / Linux:
- 67 workspace tests pass; strict Clippy and formatting pass.
- Release workspace build passes.
- Imported fixture's damage is observed in combat; engine save/restore matches
  events and hashes for 50 subsequent Tick calls in a dedicated test.
- Two independent 512-unit runs requested 900 Tick calls, reached elimination
  at Tick 628, and froze with the same hash `7dc34aa136498086`.
- Saving at Tick 450 and restoring for 450 further calls reaches the identical
  Tick 628 / `7dc34aa136498086` result. This is not 900 active combat Ticks.
- TCP/replay regression: two independent peers, 2,000 units, 900 Ticks,
  147,993-byte replay, unchanged final hash `5a835421a4a3e97a`.

No graphical acceptance or original-asset compatibility claim is added by this
batch. 1.0 remains incomplete: original movement/terrain semantics, armor and
projectiles, full production/construction, audio, AI, multiplayer UI and real
map/MOD acceptance fixtures still need work.
