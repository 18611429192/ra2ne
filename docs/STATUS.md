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

## 2026-10-04: standard armor and direct warhead Verses

Implemented eleven standard armor slots and exact percentage parsing. Layered
warhead Verses now scales direct shot damage by target armor. Explicit attacks
on zero-multiplier armor fail atomically; automatic selection skips 0%, 1% and
2% armor matches. Other percentages, including fractional values, use their
actual multiplier. No retaliation logic or original splash/projectile/immunity
semantics are claimed. Damage currently floors fixed-point products, with u32
saturation and no minimum-damage adjustment; original rounding is unverified.
Missing referenced Verses, malformed counts/precision and custom armor fail.

Definitions, state hashes and saves include armor and all eleven multipliers.
Game save/replay and TCP handshake versions advance to v2; v1 full-game artifacts
and peers are rejected. Movement/map checkpoints and frame codecs remain v1.

Verification on Rust 1.99.0 / Linux:
- 73 workspace tests pass, strict Clippy and formatting pass; Release builds pass.
- Tests cover slots/percentages/overflow, immune-nearest-target skipping,
  atomic immune attack rejection, 1%/2% passive acquisition exclusions, actual
  scaled damage, warhead overlays, hash sensitivity, v1 save/peer rejection,
  and nonuniform-armor save/replay round trips.
- Two 512-unit experimental runs finish 900 active Ticks with hash
  `b1b622812c3d5cd7`; saving at 450 and restoring for 450 matches exactly.
- TCP/replay: two independent 2,000-unit peers, 900 Ticks, 148,041-byte replay,
  hash `5645243b40d0dbe8`.
- Sustained 10,000-unit combat: 900 Ticks, 31 matching checkpoints,
  hash `fe3ce38bf6d1d62b`; 2.328 ms/Tick on this machine while other regression
  processes ran. This is a synthetic measurement, not an original-game comparison.

Hash changes reflect the expanded rule definitions. No new graphical or
original-resource acceptance was performed. See RULE_EXPERIMENT.md and
FORMAT_NOTES.md for supported semantics and sources. 1.0 remains incomplete.

## 2026-10-04: secondary weapons and Tick-sensitive game hashes

Both weapon slots now compile from layered INI rules. Primary-first armor
eligibility chooses the weapon for explicit commands, automatic acquisition and
pursuit; passive flags are evaluated separately for automatic selection. Chosen
range, damage and reload are used in combat. Secondary-only definitions work.
Both slots share an actor cooldown. Longer secondary range alone does not bypass
an eligible primary. Elite/ammo/AA/deploy/transport/wall selection remains pending.

Regression found that movement hash's final Tick XOR was immediately cancelled
by the game's identical Tick XOR in stationary scenes. Game hashing now mixes
the movement hash first. A dedicated idle-world test verifies different Tick
hashes and save restore. Both weapon definitions and multipliers are hashed and
saved. Full-game saves/replays and TCP handshakes are v3; v1/v2 are rejected.

Verified on Rust 1.99.0 / Linux:
- 77 workspace tests, strict Clippy, formatting and Release builds pass.
- New tests cover primary priority, secondary-only actors, passive fallback,
  no range-only fallback, secondary-range pursuit, damage/cooldown, invalid
  secondary rules, overlays, save restore and full-game replay.
- Two 512-unit overlay runs requested 900 calls, finished at Tick 539 and froze
  with hash `37d77b58c0cac090`. Save at Tick 100 plus 800 calls matches exactly;
  this is 539 active Ticks, not 900 active combat Ticks.
- Network fixture deliberately gives its primary zero Verses; actual shot
  events must have secondary damage and match across peers. Two independent
  2,000-unit peers / 900 Ticks and full replay agree, 148,098 bytes,
  hash `7611521db24237ec`.
- Sustained 10,000-unit primary combat: 900 Ticks / 31 matching checkpoints,
  hash `1bb42b61f7f74d43`, 2.679 ms/Tick with concurrent regression processes.

Added a redistributable secondary-overlay fixture and documented selection
boundaries. No new graphical/original-resource acceptance is claimed; 1.0
remains incomplete.

## 2026-10-04: typed factories and exact building prerequisites

Imported definitions now carry product category, optional factory category and
resolved exact prerequisite building indices. Vehicle/infantry factory categories
are enforced before payment; the owning player must have every prerequisite.
Missing prerequisites pause existing paid jobs, and restoration resumes them.
Production snapshots power and owned types once per Tick instead of scanning
all entities for every factory's power check. Initial population/spawn remains
independent of prerequisite gates.

Unknown factory categories, non-building factories/prerequisites, unresolved
aliases, naval definitions and unsupported aircraft/building production fail.
Generic aliases, faction overlap, TechLevel, stolen tech and original build
timing remain pending. Building placement and an original build sidebar are
not added by this batch. Synthetic untyped factories retain fixture semantics.

Restrictions are hashed and saved; queued categories are checked on restore.
Full-game saves/replays and TCP handshakes advance to v4. Limits are 1024
prerequisites per type and 1,000,000 total, including bounded decoding.

Verification on Rust 1.99.0 / Linux:
- 79 workspace tests, strict Clippy, formatting and Release builds pass.
- Independent synthetic INI fixture tests category rejection, same-player exact
  prerequisites, enemy prerequisite exclusion, atomic rejection, credit
  reservation/refund, real vehicle/infantry production, save and full replay.
- A combat test destroys a prerequisite; the paid job pauses, survives save/load,
  and resumes when the owned building is restored, with matching future events.
- TCP secondary fixture: 2,000 units / two peers / 900 Ticks, 148,099-byte replay,
  hash `61f44cf2f816d1d4`.
- Sustained combat: 10,000 units / 900 Ticks / 31 matching checkpoints,
  hash `81fcc0038225b6df`, 2.600 ms/Tick with concurrent regression processes.
- Synthetic runtime: 516 initial actors / 900 Ticks, hash `2cb632011ad0e9e5`.

Overall 1.0 feature coverage is estimated at approximately 25%, with 75% still
remaining. This is not an acceptance score or time estimate; see PROGRESS.md.
Original resource/map/MOD acceptance remains pending, and 1.0 is incomplete.

## 2026-10-04: generic prerequisite groups

Added the six explicit General aliases POWER/FACTORY/BARRACKS/RADAR/TECH/PROC
and custom GenericPrerequisites building groups. Custom entries override standard
base lists. Each group requires any one owned building, and every named group
must be satisfied. Exact IDs become singleton groups. Groups and members are
canonicalized by stable type index, including duplicate removal. Consumed source
properties no longer emit misleading pending-semantics diagnostics.

Missing base lists, empty groups, unknown/non-building members and nested groups
fail explicitly. Referenced PROC non-building alternates remain unsupported and
fail rather than silently narrowing the requirement. Other Ares enhanced
prerequisite logic is pending. This is not complete MOD compatibility.

Full-game save/replay and TCP handshake formats are v5; v1–v4 are rejected.
Decoding bounds 1024 groups/type, 1024 members/group and 1,000,000 members total.
Group boundaries and members participate in hashes and persisted rules.

Verification on Rust 1.99.0 / Linux:
- 81 workspace tests, strict Clippy, formatting and Release builds pass.
- New tests cover group OR/AND semantics, own/enemy buildings, standard/custom
  overrides, malformed groups, canonical duplicate groups, production and
  save/full-game replay round trips.
- Runtime loads a multi-group fixture: 512 actors / 300 Ticks,
  hash `0209355b371f7336`. Saving at 100 and restoring for 200 matches exactly.
  This is a movement-only preview with grouped definitions, not original base
  production acceptance; factory production is exercised by engine tests.
- TCP secondary fixture: two independent 2,000-unit peers / 900 Ticks and
  replay agree, 148,099 bytes, hash `61f44cf2f816d1d4`.

Added a redistributable group-overlay fixture and source references. Overall
1.0 feature coverage remains approximately 25%; original-content acceptance
still needs major work. No new graphical/original-resource acceptance is claimed.


## Experimental production controls (2026-10-04)

- Added read-only production availability checks shared by queue validation and
  the runtime sidebar, avoiding frontend copies of factory/prerequisite rules.
- Experimental INI scenes accept explicit buildings and initial product orders
  for both players, with bounded placement and explicit failures.
- Sidebar supports factory selection, paged product costs/availability, queue
  remaining ticks, low-power feedback, and first-job cancellation/refund.
- Runtime integration tests cover both product categories, ownership rejection
  without mutation, refund, completed production and deterministic save resume;
  malformed base/product requests are rejected.
- Original construction, footprints, factions and multiplayer production UI
  remain pending. This milestone does not establish original skirmish acceptance.
- Validation: 83 workspace tests passed; strict Clippy, format and release build
  passed. Actual release CLI: 14 entities at tick 45, then 18 at tick 120.
  Loading the tick-45 save and running 75 more ticks matched the uninterrupted
  tick-120 hash `b0f9f90499dcd98c`.
- Window/screenshot verification is pending in this environment: Xvfb is absent
  and package installation could not obtain it. Sidebar layout is compiled but
  has not been visually verified in this batch.


## Runtime input recording and complete queue controls (2026-10-04)

- Runtime battle sessions route movement, stop, attack, production and cancellation
  through shared engine actions. Optional v5 replay recording includes rejected
  input and begins from a post-setup snapshot.
- Added standalone headless replay verification and final-step handling for
  input entered after the last tick. Save and replay outputs share that final state.
- Recording bounds are checked before command application, including selection
  canonicalization, known-player validation and a conservative 128 MiB budget.
- Queue pages show individual product names and remaining ticks; any displayed
  job can be cancelled/refunded. Active-job progress and factory-change page reset
  are included.
- New real loopback TCP test covers typed production, refund and ownership/category
  rejections, matching two peers and encoded replay at every tick. Runtime tests
  cover input/refund/rejections, final commands, nonzero initial ticks, duplicate
  selections and byte/selection/player bounds.
- No wire/save format change; original-content acceptance, multiplayer UI and
  graphical verification remain pending. Overall 1.0 estimate remains 25%.
- Validation: 88 workspace tests passed, including finished-match replay timeline
  and rejected final input; strict Clippy, format and release build passed.
- Release CLI recorded/replayed tick 45 at hash `510ad2683fe3deae`. Recording
  75 steps from the tick-45 save reached tick 120 at `b0f9f90499dcd98c`, matching
  standalone replay and uninterrupted execution. Corrupt replay, output overwrite
  and incompatible CLI flags were rejected. Window verification remains pending.


## 2026-10-05: real-package parsing repairs and directory audit

Recovered the user-provided Ra2Game412.zip and tested its actual resources. Added
read-only `ra2ne-inspect audit`, bounded XCC filename metadata, optional terrain
trailers, preserved/off-grid waypoint diagnostics, TMP padding handling, correct
SHP compression byte/format-two/empty-frame behavior and row-edge transparency.
Rule discovery now continues through incomplete placeholders without weakening
strict gameplay compilation; section-header slash comments are accepted.

97 workspace tests, strict Clippy, formatting and Release passed. Actual audit:
60 MIX containers (26 encrypted, 44 verified checksums), 371 maps, 5,616 SHPs /
110,160 frames, 9,929 TMPs / 46,392 tile occurrences and 4 named palettes;
zero failures among identified formats. Two empty MIXs, 1,586 unidentified and
572 known-unchecked entries are explicitly separate. No original resources are
committed. Headless map-view loading, sustained combat and TCP/replay regression
passed; game hashes remain unchanged. Detailed reproduction, input fingerprint,
rule-discovery results and limitations are in REAL_RESOURCE_VALIDATION.md.

1.0 remains incomplete. Overall estimated feature coverage stays approximately
25%, because parsing authentic resources does not establish original gameplay.

### Theater catalogue and base terrain viewer

The assets frontend now resolves numeric TileSet sections into global map tile
IDs, including zero-sized sets. It rejects missing sections, excessive counts and
unsafe filename prefixes. The runtime accepts explicit theater INI/PAL/MIX
resources, caches decoded tile/subtile images within 128 MiB, creates nearest
filtered textures and draws them at the existing 60x30 isometric projection.
Terrain MIX mounts share an archive byte budget and later mounts take precedence.
Unresolved cells are reported and retain the synthetic fallback.

Validation: 99 workspace tests and strict Clippy passed. The actual temperate
catalogue resolves 838 filenames. Loading the previously tested real map
`86b74276.map` with isotemp/isotemmd decoded 598 distinct base images, reported
920 cells with negative tile IDs and five variants with pending extra graphics.
At 120 headless ticks its 26 actors retain hash `6209414cb5672eb7`.
This validates resource resolution/decoding; the interactive texture placement
has not been visually verified in this environment. No original files are shipped.
Automatic game-directory mounting, clear-cell handling, extras, lighting and
occlusion remain incomplete; the overall 1.0 estimate remains unchanged.

## 2026-10-05: stock directory terrain, clear markers and extra graphics

The original-map viewer now accepts `--game-dir=PATH --edition=ra2|yr` and reads
selected stock nested archives, including common/theater resources outside the
ISO-only archives. It merges base/MD theater INIs, automatically selects ISO
palettes and defaults directory text to Windows-1252. RA2 mode excludes YR
archives; YR mode requires both root archives. Root input is capped at 512 MiB,
mounted terrain archives at 256 MiB and decoded RGBA images at 128 MiB.
Ambiguous filename case collisions fail. This is a narrow stock terrain profile;
expansion/mod archives, loose overrides and complete game search order are pending.

Clear marker `0xffff` now resolves to tile zero. Base/extra TMP pixels composite
at their relative offsets with transparent extra pixels preserving the diamond.
Image bounds drive culling, preventing tall extras from disappearing when their
cell anchor leaves the viewport. The camera starts at the map center. NewUrban
and Temperate resource fallbacks retain the correct palette identity; damaged
preferred resources do not silently fall back. Theater count compatibility
handles and reports the stock `TilesInSet=o` entry without relaxing general INI
integer parsing.

Verified: 107 workspace tests, strict Clippy, formatting and release builds.
Synthetic tests cover clear markers, offset/transparent composition, extreme
bounds, palette identity, corrupt preferred assets, stock directory case lookup,
base/MD INI merging, resource overrides and option conflicts. Actual maps from
all six theaters load with zero fallback cells; detailed counts are in
REAL_RESOURCE_VALIDATION.md. Temperate directory loading also runs 120 actual
window frames/120 ticks with hash `6209414cb5672eb7`; Temperate, NewUrban and Urban
screenshots were visually inspected. The previous manual example's ordinary
`temperat.pal` was corrected to the terrain `isotem.pal` after window validation.

Original actors are still placeholders. Overlays/bridge spans, animated tiles,
random/damaged variants, lighting and pixel depth order, terrain passability and
original gameplay remain incomplete. This advances authentic terrain viewing,
not full-game acceptance; the overall 1.0 coverage estimate remains about 25%.

## 2026-10-05: original static overlays and full-map graphics checks

Added ordered OverlayTypes metadata, map Image overrides, SHP frame lookup,
NewTheater generic G variants and resource/ISO/unit palette selection. The stock
installation profile mounts conquer/conqmd and generic/genermd. Runtime draws
ore/gems, walls, bridges and other static overlays alongside TMP terrain, with
bounded caches and image-bound culling. Missing references, transparent frames
and off-terrain references have distinct diagnostics; source map arrays survive.

Six real theater samples have zero unresolved overlay cells. NewUrban/Lunar
screenshots were inspected; the 120-frame Temperate viewer retains its previous
state hash. 113 regular workspace tests pass, strict Clippy and release builds
pass. Synthetic regressions cover ID ordering, aliases, palette identity, generic
wall lookup, geometry, exact frame references and unsupported data diagnostics.

The new opt-in strict acceptance test checks all 371 private maps: 360 pass and
11 fail on remaining terrain resources/subtiles or overlay frame references.
See REAL_RESOURCE_VALIDATION.md for exact map IDs and reproduction. These failures
remain visible; static overlay support does not establish full-map acceptance,
original gameplay or a completed 1.0. Overall feature estimate remains about 25%.

## 2026-10-05: placed original scenery

Added typed `[Terrain]` records without adding them to playable actors. Original
trees, lamps, signs and other registered terrain objects now render through the
existing bounded SHP path, with their own registry, frame-zero images, drawing
anchor and diagnostics. Source INI records survive invalid keys, duplicate cells
and missing type definitions. Drawing considers scenery bounds independently of
TMP/overlay bounds.

Extended real-map graphics checks resolve all 165,818 scenery records in all 371
maps; there are no scenery failures or off-terrain references. NewUrban release
screenshots show trees and lamps; a 120-frame Temperate window run retains the
previous state hash. 114 regular workspace tests and strict Clippy pass. The
private whole-map acceptance test still reports the same 11 terrain/overlay
failures. Scenery collision, animation, shadows, emitted lighting and destruction
are pending. Full original gameplay and the overall 1.0 estimate remain unchanged.

## 2026-10-05: original static infantry

Added bounded Ready-sequence metadata and original infantry SHP images in the map
viewer. Eight facing buckets, grid subcells and map elevation select and position
standing frames. Actor ordering and source metadata remain intact; unsupported
actors keep their placeholders and bad frames are diagnosed. Image caching is
bounded and culling uses rendered bounds.

The full-map graphics check resolves all 4,011 infantry in 371 maps, including
nine existing E1 definitions under a map registry override. NewUrban window
screenshots show original civilian figures. 116 regular tests, strict Clippy and
release builds pass. The same 11 whole-map terrain/overlay gaps remain visible.
Owner remap, shadows, action animation, VXL actors and original gameplay are
pending. The overall 1.0 estimate remains unchanged.

## 2026-10-05: VXL/HVA readers and real-model audit

Added bounded sparse VXL geometry decoding and HVA frame/section transforms.
Model metadata, voxel palette/normal indices and file matrices remain available
for rendering work. The inspector reports model/pose details and validates paired
section counts. Strict named binding is optional; stock index association handles
real HTK name differences. Empty probe.hva is explicitly counted as a placeholder.

The full resource audit now reads 221 VXLs / 1,212,644 voxels and 221 HVAs / 468
matrices with zero parsing failures. Selected stock rules resolve 104 related
model stems with no missing HVA. 120 regular workspace tests and strict Clippy
pass, including bounded malformed geometry and multi-frame matrix fixtures.
Original vehicle rendering and gameplay remain pending; the overall 1.0 estimate
remains unchanged. The 11 whole-map terrain/overlay acceptance gaps are separate
from this successful file-format audit.
