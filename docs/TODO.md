# RA2NE TODO — 待办清单

Consolidated 2026-10-16 from PROGRESS.md, STATUS.md, REAL_RESOURCE_VALIDATION.md
and RULE_EXPERIMENT.md. Companion docs: **REA_SETUP.md** (reverse-engineering
environment, analysis workflow and analysis log) and the living records in
STATUS.md (append new verified batches there).

Baseline: approximately **25%** of 1.0 feature coverage; 125 workspace tests
green (documented on Rust 1.99.0; re-verified locally on Rust 1.99.1).
The target remains: play with user-owned RA2/YR resources, original maps and
ordinary INI mods. Synthetic tests alone cannot satisfy this target.

---

## P0 — Close documented acceptance gaps

- [ ] **11 whole-map graphics failures** (strict private check, 360/371 pass).
  Investigate original runtime behavior for each; do not clamp or substitute.
  - [ ] Missing `Cliff12.sno` subtile 2 — maps01/9103f015; maps02/9103f015
  - [ ] Missing `Cliff13.sno` subtile 1 — maps02/439fea47
  - [ ] Missing `Cliff26.urb` subtile 1 — maps02/b7ce3f39
  - [ ] Missing `hyte01.tem` resource — multi/1d555268; multimd/b00a1790
  - [ ] `LOBRDB01.sno` frame references 6, 9, 10, 12 (30 cells) — maps02/1d3dc118
  - [ ] `WCRATE.tem` frame reference 10 — multi/10d1801a; multimd/59b25b1a
  - [ ] `CRATE.tem` frame reference 255 — multi/c1180b29; multimd/2b5e99be
- [ ] Map parsing still reports 3,050 diagnostics (pending triggers etc.).
  Classify and either implement or keep explicitly documented.

## P1 — Finish original-resource rendering

- [ ] **Voxel normal lighting** (currently unlit CPU rasterizer; no normal
  tables copied — derive from game behavior/documentation).
- [ ] Stock gradient/lighting calibration for remap colors; original saturation
  curves.
- [ ] Numeric multiplayer color assignments for house color previews.
- [ ] Shadows: voxel + SHP sprite shadows.
- [ ] Voxel turrets/barrels facing calibration, art attachment offsets,
  aircraft height/pitch.
- [ ] Infantry mission/action animation (currently static Ready poses only).
- [ ] Vehicle/aircraft SHP support (currently VXL only).
- [ ] Animated/damaged/random terrain variants; mining/growth overlays;
  per-pixel depth order and lighting.
- [ ] Audio (none implemented).
- [ ] Loose-file overrides and complete stock/mod mount search order
  (current stock terrain profile is deliberately narrow; XCC name databases
  optional; automatic game-directory mount policy pending).

## P2 — Original gameplay semantics

- [ ] Original terrain passability and movement semantics derived from tile
  identifiers (today: data viewer; passability not inferred).
- [ ] Physical occupancy, local avoidance, fair bottleneck queues, formations,
  dynamic terrain invalidation.
- [ ] Damage rounding semantics verification (currently floors fixed-point
  products; original behavior unverified) and minimum-damage rules.
- [ ] Splash/projectile/immunity semantics; retaliation.
- [ ] Elite/ammo/AA/deploy/transport/wall weapon selection.
- [ ] Construction: building placement, footprints, original build timing,
  base sidebar, sale/repair.
- [ ] Factions, TechLevel, stolen tech, Ares enhanced prerequisite logic.
- [ ] Harvesting/refinery original rules (today synthetic).
- [ ] Triggers/teams/scripts/lighting sections and mission semantics.
- [ ] Skirmish AI.
- [ ] ShortGame rules, alliances, elimination conditions beyond current.
- [ ] Engine saves vs original RA2 saves compatibility (engine saves only).

## P3 — Multiplayer

- [ ] Playable multiplayer lobby UI (transport and lockstep already exist).
- [ ] Authenticated roster/ownership, disconnect policy, desync hash exchange.
- [ ] Reconnect, encryption, internet sessions.

## P4 — Acceptance & verification

- [ ] End-to-end original-content gameplay acceptance with real maps/mods.
- [ ] Cross-platform determinism verification (Windows/macOS builds).
- [ ] Review pending remote CI results (Linux/Windows/macOS configured).
- [ ] Ordinary INI mod acceptance fixtures beyond the experimental bridge.
- [ ] Investigate the 1,586 unidentified + 572 named-unchecked audit entries
  (audio, PCX, UI strings etc. not yet validated).

## REA-assisted investigation backlog

Use REA + Ghidra on `Ra2Game412/gamemd.exe` (and `Ares.dll` where relevant) to
extract the original behavior these features need. See REA_SETUP.md for the
workflow. Prioritized list:

- [ ] **Movement timing**: original Speed → cells/tick conversion (needed to
  replace `--rule-speed` manual calibration).
- [ ] **ROF/build timing**: original ROF seconds ↔ engine ticks mapping
  (`--rule-rof`), building construction times.
- [ ] **Damage math**: rounding and minimum-damage semantics for Verses scaled
  damage; verify against P2 work.
- [ ] **Voxel lighting**: how the original computes voxel normals/lighting
  (`[Lighting]`, `UseNormal`, ambient values).
- [ ] **House color curves**: original remap/saturation for the 8+ multiplayer
  color numbers.
- [ ] **Terrain passability**: which tile/overlay flags make cells walkable;
  clear-cell and bridge entry rules.
- [ ] **The 11 P0 resource failures**: how the original resolves subtile/frame
  references and what it does for missing resources (explains acceptance gaps).
- [ ] **Infantry animation timing**: Walk/Action frame rates from art/sequence
  data usage.
- [ ] **Attachment calibration**: turret/barrel offsets and facing tables.
- [ ] **Aircraft behavior**: flight height, pitch/roll during turns.
- [ ] (Later) trigger/script engine shape, skirmish AI decision points,
  audio (`sounds.ini`) usage.

---

Notes:
- No EA assets are committed; all analysis stays read-only, local, and
  gitignored (`Ra2Game412/`).
- Append verified work to STATUS.md with tests/hashes as usual; update
  PROGRESS.md percentages only at substantial end-to-end milestones.
