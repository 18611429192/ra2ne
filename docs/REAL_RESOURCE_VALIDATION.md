# Actual game resource validation — 2026-10-05

User-provided input: `Ra2Game412.zip`, 386,668,027 bytes.
SHA-256: `a9888197e6a77d75474307c57e372d9eef663e620aecfcc8c6d81f3cf6f84687`.
No resource bytes, original executables or private fixture copies are included in this repository. The supplied package includes RA2/YR resources, an expansion resource layer and Ares/spawn components. This is a test of this package, not proof of every original release or ordinary MOD.

## Reproduce

Unpack your copy and pass its game directory to:

```sh
cargo run --release -p ra2ne-inspect -- audit /path/to/Ra2Game412
```

The audit scans supported loose files and top-level MIX archives, follows structurally detected nested MIX containers up to depth eight, and uses optional XCC filename databases or explicit known names. Anonymous files use structural recognition. Child directories such as HT are not separately scanned. Counts are resource occurrences, including copies in different packages; they are not unique asset counts. Empty top-level MIX placeholders are reported separately. Audio, PCX, voxels, UI string files and other unidentified/uninspected entries are not validated. Map text uses Windows-1252 for this audit; localized text fidelity needs its own explicit encoding check.

## Observed failures and fixes

- All 371 detected maps initially failed because IsoMapPack5 had four trailing bytes. These packages all use a zero trailer. Complete eleven-byte cells and the optional four-byte trailer are now preserved separately; other incomplete lengths still fail.
- Twelve map occurrences had stale negative/off-grid waypoint values. Forty-four such entries are preserved with diagnostics and excluded from usable cell coordinates. Nonnumeric values and duplicate IDs still fail. This does not invent paths for invalid waypoints or implement mission triggers.
- TMP flag words contain unused high bits, including debug-fill values. Only the three defined bits are interpreted; the raw word stays available. Active pixel ranges still receive bounds checks.
- Theater-specific extensions can contain SHP sprites, so audit dispatch uses structure before assuming TMP.
- SHP compression is a byte rather than a four-byte word. Empty frames can carry unused flags and metadata. A terminal transparent run may exceed the row width; it now clamps within the row. Literal overruns/truncations still fail.
- SHP format two is raw data with a stored row length, not format-three transparent-run encoding. Synthetic exact-pixel fixtures cover both and row padding.
- Actual rules contain section-header `//` comments and incomplete registered placeholders such as DeathDummy, plus duplicate registry entries. Discovery reports incomplete types/weapons while continuing through valid definitions. Strict gameplay compilation retains its prior rejection behavior; no undocumented Strength or weapon defaults are invented.

## Current results

| Resource | Successfully inspected |
| --- | ---: |
| MIX containers | 60 |
| Encrypted directories | 26 |
| Verified SHA-1 archive bodies | 44 |
| Empty MIX placeholders | 2 |
| Maps | 371 |
| SHP files | 5,616 |
| Decoded SHP frames | 110,160 |
| TMP files | 9,929 |
| Decoded TMP tile occurrences | 46,392 |
| Named PAL files | 4 |
| Detected-resource parsing failures | 0 |
| Unidentified entries | 1,586 |
| Named but unchecked entries | 572 |

Map parsing still reports 3,050 diagnostics, including pending triggers and preserved source entries. A successful audit does not certify their runtime behavior.

Rule discovery:
- rules.ini: 409 unique registered types, 402 typed definitions, 78 weapons, 7 incomplete types, 4 incomplete weapons.
- rulesmd.ini: 559 unique registered types, 552 typed definitions, 109 weapons, 7 incomplete types, 5 incomplete weapons.

A formerly rejected map (multimd MIX ID 86b74276) now loads through the existing data-viewer runtime: 26 placed actors, 120 headless ticks, hash `6209414cb5672eb7`. No original map gameplay or graphical acceptance is claimed by this headless check.

## Regression verification

Rust 1.99.0, x86_64 Linux: 97 workspace tests pass; strict Clippy, formatting and release workspace build pass. New redistributable synthetic fixtures cover trailer preservation, off-grid waypoints, slash comments, bounded XCC metadata, incomplete discovery, TMP padding, SHP format-two pixels, empty frames and edge transparency.

The existing two-peer TCP/game replay fixture (2,000 units / 900 ticks) still matches hash `61f44cf2f816d1d4`, 148,099-byte replay. The sustained synthetic combat fixture (10,000 units / 900 ticks / 31 checkpoints) still matches `81fcc0038225b6df`, 2.789 ms/Tick on this host while the TCP check also ran. These are synthetic engine measurements, not original-game comparisons.

Original terrain rendering/passability, full INI semantics, units/animations/voxels/audio, construction, AI, triggers, playable multiplayer and end-to-end real-map/MOD acceptance remain unfinished. Overall estimated 1.0 feature coverage remains approximately 25%; this batch establishes authentic parsing coverage, not full-game completion.

## Stock installation terrain viewing (2026-10-05)

`--game-dir=... --edition=yr` now resolves the following real map samples from the
uploaded installation. Counts refer to unique tile/subtile images; extras are
included in those images, not additional map cells. All six use the stock game's
matching INI and ISO palette. Original source files remain outside the repository.

| Theater | Archive / map ID | Images | Images with extras | Fallback cells |
| --- | --- | ---: | ---: | ---: |
| Temperate | expandmd01 / b6bdb430 | 129 | 80 | 0 |
| Snow | expandmd01 / 0629413d | 936 | 214 | 0 |
| Urban | maps01 / 179782bb | 1022 | 132 | 0 |
| Desert | expandmd01 / 03a45e74 | 869 | 112 | 0 |
| NewUrban | expandmd01 / 19a211e3 | 893 | 104 | 0 |
| Lunar | mapsmd03 / 8ba1de85 | 216 | 25 | 0 |

The earlier multimd / 86b74276 map resolves 599 images, five with extras, and zero
fallback cells after clear-marker handling. Running it for 120 actual window
frames gives the same `6209414cb5672eb7` hash as 120 headless ticks. Temperate,
NewUrban and Urban window screenshots have been inspected for palette selection,
terrain image alignment and visible extra graphics. An initial incorrect ordinary
scene palette was detected by that inspection and replaced by the ISO palette.

The Urban sample's rubble resources reside in the ordinary `urban.mix`, not just
`isourb.mix`; mounting only ISO archives missed 416 cells. NewUrban's INI also
contains a literal letter `o` as a tile-set count. Its original decimal-prefix
behavior is supported with an explicit diagnostic. These were real-resource
compatibility failures, not changes to synthetic gameplay rules.

This validates these sample maps and the basic rendering path. It does not
certify every original map, the complete stock/mod mount order, overlays and
animation, per-pixel depth/lighting, navigation or gameplay. 107 current workspace
tests pass; the 1.0 feature-coverage estimate remains about 25%.
