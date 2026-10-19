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

## Static overlays and complete map graphics audit (2026-10-05)

Directory mode now resolves and renders static original overlay images. The six
previous samples have these results; each reports zero unresolved overlay cells.
Empty means a referenced frame is entirely transparent, not a missing image.

| Theater | Overlay cells | Cached images | Empty cells |
| --- | ---: | ---: | ---: |
| Temperate | 712 | 16 | 0 |
| Snow | 301 | 23 | 161 |
| Urban | 230 | 31 | 0 |
| Desert | 929 | 71 | 84 |
| NewUrban | 743 | 43 | 0 |
| Lunar | 259 | 19 | 0 |

NewUrban and Lunar window screenshots were inspected for visible resources,
bridge alignment and Yuri walls. multimd/86b74276 resolves 730 overlay cells and
26 images, with 105 transparent cells and no unresolved cells. Its actual
120-frame window run still produces hash `6209414cb5672eb7`.

A new strict opt-in graphics acceptance check processes every map against the
selected stock installation, checking terrain composites, SHP frame references
and palettes. With YR selected, **360 of 371 maps pass; 11 maps fail**. All 371
still parse structurally. This is broader coverage than the six successful
samples and exposes remaining acceptance gaps rather than certifying every map.

| Failure | Map archive / ID |
| --- | --- |
| Missing Cliff12.sno subtile 2 | maps01/9103f015; maps02/9103f015 |
| Missing Cliff13.sno subtile 1 | maps02/439fea47 |
| Missing Cliff26.urb subtile 1 | maps02/b7ce3f39 |
| Missing hyte01.tem resource | multi/1d555268; multimd/b00a1790 |
| LOBRDB01.sno frame references 6, 9, 10, 12 (30 cells) | maps02/1d3dc118 |
| WCRATE.tem frame reference 10 | multi/10d1801a; multimd/59b25b1a |
| CRATE.tem frame reference 255 | multi/c1180b29; multimd/2b5e99be |

The check stops terrain validation at the first terrain error in each map;
reported failures are not an exhaustive count of bad cells. No frame clamping,
resource substitution or source-map repair is used to make this check pass.
Original runtime behavior for these references still needs investigation.
The strict private test currently fails on this installation as expected and is
ignored by default; CI does not require or redistribute game resources.

```sh
RA2NE_GAME_DIR=/absolute/path/to/game RA2NE_MAP_DIR=/absolute/path/to/maps \
RA2NE_EDITION=yr RA2NE_MAP_ENCODING=windows1252 \
cargo test -p ra2ne-runtime all_private_map_graphics_resolve_and_decode -- --ignored --nocapture
```

113 regular workspace tests pass, plus this separately run private acceptance
check. Overlay rendering remains static: mining/growth, connectivity, bridge
passability, animation, shadows, lighting and original gameplay are unfinished.

## Placed trees, lamps and other scenery (2026-10-05)

The strict full-map check now also resolves `[Terrain]` objects for **all 371
maps**, including the six maps where terrain validation stops early. With the
same YR stock profile, 165,818 scenery records resolve, 7,261 per-map image-cache
entries decode, zero records lie outside decoded terrain, and zero maps have
unresolved scenery. Cache-entry counts include repeated assets across maps.
The existing overall result is unchanged: 360 complete maps pass and 11 fail on
previously documented terrain/overlay references. The strict private test still
fails as a whole; the scenery-specific checks do not erase those failures.

NewUrban/19a211e3 resolves 1,250 scenery objects and 28 distinct images. Its release
window screenshot was inspected for tree, street-lamp and sign placement.
Temperate multimd/86b74276 resolves 1,040 scenery objects and 13 distinct images;
its 120-frame release window run retains hash `6209414cb5672eb7`.
114 regular workspace tests pass. A new synthetic regression checks named,
case-insensitive types, rules/art aliases, native palette identity, frame-zero
pixels, scenery anchoring and preservation/diagnosis of invalid records.
Animation, shadows, emitted light, occupation/collision and destruction remain
pending, as does complete original gameplay.

## Static infantry graphics (2026-10-05)

All **4,011 placed infantry records across 371 maps** now resolve standing SHP
frames with zero unresolved infantry records. The original 11 whole-map
terrain/overlay failures remain unchanged; the strict private acceptance test
still fails overall and remains ignored in regular CI.

NewUrban/19a211e3 has 45 infantry actors and 16 distinct selected images, including
visible original civilian figures in the inspected window screenshot. The first
full check exposed nine E1 actors in mapsmd03/73220e81 whose map overrides replace
a base InfantryTypes registry label. The graphics loader now permits existing
rules definitions, and the repeated full check resolves all nine.

116 regular workspace tests pass, with strict Clippy, formatting and release
builds. New synthetic regressions verify Ready start/count/stride bounds, all
256 facing values, existing definitions under registry overrides, stable actor
IDs, image aliases, unit palette pixels, subcells, map height and out-of-range
frames without clamping. Only static Ready poses are implemented: owner remap,
shadows, missions/action animation, VXL actors and original gameplay are pending.

The NewUrban release viewer was also run for 120 window frames. Its 441 actor
records produce hash `4ded6965e81531ec`, matching a separate 120-tick headless run.

## VXL/HVA geometry audit (2026-10-05)

The expanded complete-package resource audit reports **zero parsing failures**:

| Additional resource | Count |
| --- | ---: |
| VXL files | 221 |
| Decoded voxels | 1,212,644 |
| HVA files | 221 |
| Pose matrices | 468 |
| Empty HVA placeholders (included above) | 1 |

Previous coverage remains 60 archives, 371 maps, 5,616 SHPs / 110,160 frames and
9,929 TMPs / 46,392 decoded tile occurrences. 1,512 entries remain unidentified
and 204 named entries unchecked. Successful format parsing remains distinct
from the 11 documented whole-map graphics acceptance failures.

A separate selected-stock-profile check resolves 104 unique VehicleTypes and
AircraftTypes image/body/turret/barrel resource stems with matching VXL/HVA counts
from local/localmd and conquer/conqmd, and no missing HVA among those models.
This is not a complete actor renderer or complete mod resource search order.
HTK and HTKtur initially failed strict name association; index association used
by the stock readers resolves both. HTNK has one limb, 4,816 voxels, one HVA frame,
normal mode 2 and matching DUMMY01 names; both new inspector commands were run
against these real resources.

120 regular workspace tests, strict Clippy, formatting and release inspector
builds pass. Synthetic tests exercise sparse/empty columns, skip/run colors and
normals, all truncation points, bad offsets/run trailers, non-finite floats,
frame-major matrices, index/name association, empty HVA placeholders and audit
classification failures. Vehicle/aircraft rasterization, normal lighting,
turret/barrel composition, remap/shadows and original gameplay remain pending.

### Experimental vehicle rendering / native resource video

The NewUrban sample `expandmd01/19a211e3.map` resolves 38 vehicle/aircraft records
into 29 experimental unlit voxel images with zero unresolved. The same map has
45 infantry and 441 total actors. Headless and GPU 120-tick checks retain state
hash `4ded6965e81531ec`. This is one map sample, not a complete vehicle census.
A 1280×720 native Macroquad showcase cycles original GI/CONS/DOG Walk frames and
rotates composed HTNK/MTNK VXL/HVA models. Its 360 exported frames form a 12-second
30-fps MP4. Scripted showcase motion does not implement map actor movement or
original combat. The 11 full-map graphics failures listed above were later
resolved as original engine behavior (superseded: see the 2026-10-19 section).

## Strict map graphics acceptance resolved — engine no-image semantics (2026-10-19)

The previously open whole-map graphics failures are resolved as original engine
behavior, not as accepted exceptions, clamping or substitution. REA analysis of
`gamemd.exe` (SHA-256 `7b8a0685…`) established each outcome; the earlier
by-key numbering reading of the overlay registry was corrected by a cross-check
of the draw-time skip IDs (below).

| Original behavior | Evidence |
| --- | --- |
| Overlay IDs are first-seen order of distinct type names; numeric INI keys are labels | `FUN_005fe250`: types append (`array[count++] = this`) and store their own array index as the ID; `FUN_005fec70` returns the existing type for a repeated name |
| Overlay map byte `0xff` means empty; every other byte indexes the type array directly, no bounds check | `FUN_005fd2e0` (`0x5fd41a`–`0x5fd433`; `FUN_005fc380` creates the overlay object) |
| Draw returns before IDs 167 and 178 | `FUN_0047f6a0`; under first-seen ordering those IDs are the stock placeholders `USELESS` (rule key 170) and `VEINHOLEDUMMY` (key 181) |
| Frame index at or past the SHP frame count draws nothing | `FUN_0047f6a0`, frame table lookup |
| Missing tile file, ID beyond the theater catalogue and NULL TMP block draw nothing; subtile index wraps modulo the tile count | theater/TMP lookup paths |
| YR reads only `"%sMD.INI"` theater lists and registers `EXPANDMD##.MIX` above the main archives | `FUN_00545150`, `FUN_005301a0` |

Implementation consequences: scene statistics separate out-of-range frames and
engine-skipped IDs; terrain resolution returns loaded / missing / outside instead
of failing the map; TMP composition wraps subtile indexes and reports empty
blocks; the strict check freezes the tolerated no-image references in
`apps/ra2ne-runtime/tests/data/map_no_image_expected.tsv` and fails when a
scanned map produces anything outside that table (exact bidirectional match per
map; unknown rows are errors too).

Frozen table (663 rows, all on the 11 previously failing maps):

| Kind | Rows | Meaning |
| --- | ---: | --- |
| `terrain-missing` | 421 | tile resource file absent from every mounted archive (e.g. `Mclif06-19.sno`, `hyte01-09.sno`, `mram10.sno`, `mwater01.sno`) |
| `terrain-noblock` | 140 | subtile index wraps to an empty/NULL tile block |
| `terrain-outside` | 94 | tile ID beyond the theater catalogue (e.g. 989–1060) |
| `overlay-frame` | 8 | stored data byte at/past the frame count (`205`/{6,9,10,12}; `179`/255 `CRATE`; `242`/10 `WCRATE`) |

Full run with the supplied package (`RA2NE_GAME_DIR=Ra2Game412`,
`RA2NE_MAP_DIR=…/maps-extracted`, `RA2NE_EDITION=yr`,
`RA2NE_MAP_ENCODING=windows1252`), 371 maps, 314.4 s:

```
map_graphics_maps=371; terrain_images=240327; overlay_cells=271966;
overlay_images=15687; empty_overlay_cells=10204; outside_overlay_cells=1;
tolerated_no_image_refs=663
map_scenery_cells=165818; images=7261; outside=0; out_of_range=0; failing_maps=0
map_infantry_actors=4011; unresolved=0; failing_maps=0
```

Reproduce:

```sh
cargo test -p ra2ne-runtime all_private_map_graphics_resolve_and_decode -- --ignored --nocapture
cargo test -p ra2ne-runtime freeze_map_no_image_expectations -- --ignored --nocapture  # regenerate the reviewed table
```

Known remaining fidelity gaps (documented, not acceptance blockers): ore/vein
overlay frame selection still uses the stored data byte where the original
computes frames from cell coordinates; the rubble overlay (`IsRubble`) object
image path is not modeled; overlay/scenery animation, damage states and
per-pixel depth remain pending.

## Engine mount search order aligned and re-verified (2026-10-19)

`apps/ra2ne-runtime/src/installation.rs` now resolves and mounts the archives in
gamemd.exe's own registration order (REA evidence in REA_SETUP.md): language
packs, `EXPANDMD99..00`, `RA2MD`/`RA2` with their nested `CACHE`/`LOCAL`
packs, audio, `ECACHE*`/`ELOCAL*`, the graphics packs (`CONQ*`, `GENER*`,
`ISOGEN*`, `CONQUER`, `CAMEO*`, map packs, `MULTI*`, `THEME*`), the scenario
`LOAD`/`NEUTRAL` packs, and finally the selected theater
(`<base>MD.MIX` for snow only, `<base>.MIX`, `<code>.MIX`, `<iso>MD.MIX`,
`<iso>.MIX`). Names resolve in registered archives before the disk, so loose
`.mix` files stay invisible unless the registration list names them and no
archive holds that name, and a loose `rulesmd.ini` is ignored while an archived
copy exists. Loose non-archive game-directory files mount as the lowest layer.

Re-run with the supplied package (`RA2NE_GAME_DIR=Ra2Game412`,
`RA2NE_MAP_DIR=…/maps-extracted`, `RA2NE_EDITION=yr`,
`RA2NE_MAP_ENCODING=windows1252`), 371 maps, 374.1 s. Per-theater map counts:
DESERT 17, LUNAR 1, NEWURBAN 26, SNOW 102, TEMPERATE 169, URBAN 56 — each
theater loaded its own theater group (snow mounts five archives including
`SNOWMD.MIX`, the others four):

```
map_graphics_maps=371; terrain_images=240327; overlay_cells=271966;
overlay_images=15687; empty_overlay_cells=10204; outside_overlay_cells=1;
tolerated_no_image_refs=663
map_scenery_cells=165818; images=7261; outside=0; out_of_range=0; failing_maps=0
map_infantry_actors=4011; unresolved=0; failing_maps=0
```

The totals are identical to the pre-change frozen table, so the broader mount
set (audio, load, neutral, language, map and multi packs — about 590 MB per
theater) changed no resolution outcome. Budgets are 1 GiB reads, 1 GiB resident
mounts and 64 MiB for the loose fallback layer.
