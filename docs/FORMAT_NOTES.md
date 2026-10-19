# Compatibility format notes

The frontend implements binary-format facts using new Rust APIs and bounded
parsers. Reference implementations are read for layout/algorithm confirmation;
their source is not copied into this repository. Dependencies use their own
permissive licenses and remain recorded in Cargo.lock.

## Westwood MIX

References:
- [EA mission editor XCC MIX parser](https://github.com/electronicarts/CNC_TS_and_RA2_Mission_Editor/blob/main/3rdParty/xcc/misc/mix_file.cpp)
- [EA mission editor MIX structures](https://github.com/electronicarts/CNC_TS_and_RA2_Mission_Editor/blob/main/3rdParty/xcc/misc/cc_structures.h)
- [EA mission editor key derivation](https://github.com/electronicarts/CNC_TS_and_RA2_Mission_Editor/blob/main/3rdParty/xcc/misc/mix_decode.cpp)
- [OpenRA MIX loader](https://github.com/OpenRA/OpenRA/blob/bleed/OpenRA.Mods.Cnc/FileSystem/MixFile.cs)

Legacy directories start with a little-endian u16 count and u32 body size.
Extended archives begin with a u32 flags field: bit 16 denotes a SHA-1 body
checksum, bit 17 an encrypted directory. Each entry is (u32 ID, u32 relative
body offset, u32 length). Extended encrypted directories follow an 80-byte key
source; the six-byte header and entries are rounded to eight-byte Blowfish ECB
blocks. File bodies remain unencrypted. Two little-endian 40-byte RSA blocks,
using the public Westwood 319-bit modulus and exponent 65537, supply 39 bytes
each; the first 56 bytes initialize Blowfish. These legacy primitives are used
only to read game resources.

For TS/RA2/YR, names are ASCII-uppercase with backslash separators. If length
modulo four is r != 0, append byte r followed by 3-r copies of the first byte
of the incomplete trailing group. The identifier is standard IEEE CRC32 of
that padded string. Early C&C/RA uses a distinct rotate-add hash; callers
select the algorithm explicitly, since the header alone is insufficient.

An archive does not inherently store recoverable filenames. Known names can be
looked up directly; raw inspection lists IDs and sizes. Optional XCC local name
databases are bounded and decoded. The audit discovers nested containers by
structure; explicit-name nested inspection is also available, maximum depth
eight. Original gameplay mount-priority policy remains pending. Within-archive duplicate IDs are rejected as ambiguous. Overlapping byte
ranges are allowed because entries may alias shared data. Entire declared body
and optional checksum must match the supplied archive slice exactly.

Tests include independently computed Python pow/zlib vectors and a full
Python cryptography/hashlib encrypted fixture, in addition to header variants,
truncation, duplicate IDs, bad offsets, checksums and archive/loose precedence.
These generated fixtures are not original game assets and do not establish
whole-game compatibility.

## Text and mount policy

UTF-8 (including BOM), Windows-1252 and GBK decoding are explicit. Invalid byte
sequences are rejected instead of replacing characters. In-memory and directory
mounts share caller-selected priority with MIX mounts; later layers replace
whole files. Named loose-file enumeration excludes unnamed MIX entries. Disk
imports reject symlinks and unsupported file types, have a caller byte budget,
and publish only after successful validation of the whole mount. This adapter
expects stable user-provided directories, not concurrently mutated hostile paths.

## Stock installation mount order

`apps/ra2ne-runtime/src/installation.rs` reproduces gamemd.exe's archive
registration order and name lookup (analysis record and function addresses in
REA_SETUP.md):

- Registration order, highest priority first: `LANGMD`/`LANGUAGE`;
  `EXPANDMD99..00`, `RA2MD`, `RA2`, `CACHEMD`, `CACHE`, `LOCALMD`, `LOCAL`;
  `AUDIOMD`, `AUDIO`; `ECACHE*`/`ELOCAL*` wildcards; `CONQMD`, `GENERMD`,
  `GENERIC`, `ISOGENMD`, `ISOGEN`, `CONQUER`, `CAMEOMD`, `CAMEO`, `MAPS*`,
  `MULTIMD`, `MULTI`, `THEMEMD`, `THEME`, movie packs (skipped); then the
  scenario packs `LOAD`, `NEUTRAL`; and last the selected theater
  (`<base>MD.MIX` for snow only, `<base>.MIX`, `<code>.MIX`, `<iso>MD.MIX`,
  `<iso>.MIX`). The first registered archive wins, so RA2NE mounts in reverse:
  the VFS prefers the most recent mount.
- A registration name resolves in the already registered archives before the
  disk (`CCFileClass::Open`), so a loose `.mix` file is only picked up when the
  registration list names it and no archive already holds that name.
- Loose non-archive game-directory files mount as the lowest layer, because the
  engine only reaches the disk when no archive holds the name. The budget is
  64 MiB; larger files are skipped with a diagnostic. Subdirectories are not
  part of this fallback mount.
- Budgets: 1 GiB total disk reads and 1 GiB resident mounts (the shipped YR
  package mounts about 570 MB per theater). `RA2.MIX`/`RA2MD.MIX` are opened
  for name resolution but not kept mounted: their roots only carry the nested
  archives that are mounted individually, so a mod that adds root-level entries
  to them would not be seen.
- Documented gaps: movie packs; the per-side `SIDEC%02d[Md].MIX` packs (no side
  selection yet); the WDT packs; and the disk `RULEMD*.INI` merge layers, so
  only `rules(md).ini` itself is read.

## RA2/YR map packs and initial rules schema

References:
- [EA map loading](https://github.com/electronicarts/CNC_TS_and_RA2_Mission_Editor/blob/main/MissionEditor/MapData.cpp)
- [EA map pack wrapper](https://github.com/electronicarts/CNC_TS_and_RA2_Mission_Editor/blob/main/MissionEditorPackLib/MissionEditorPackLib.cpp)
- [EA binary structures](https://github.com/electronicarts/CNC_TS_and_RA2_Mission_Editor/blob/main/3rdParty/xcc/misc/cc_structures.h)
- [EA compression implementations](https://github.com/electronicarts/CNC_TS_and_RA2_Mission_Editor/blob/main/3rdParty/xcc/misc/shp_decode.cpp)

Numbered Base64 sections are concatenated by numeric index. Pack streams contain
(u16 compressed length, u16 decompressed length, compressed bytes) blocks.
IsoMapPack5 uses LZO1X. OverlayPack and OverlayDataPack use LCW/Format80;
missing/short overlay arrays default-pad to 512x512 with 255 and zero,
respectively, matching the map editor's array initialization. The frontend
rejects malformed lengths, missing/duplicate pack indexes, bad back references,
size overruns and decompressed-size mismatches. LCW here uses ordinary absolute
long-copy offsets; other resource variants need their own explicit codec mode.

Terrain records are eleven bytes: u16 X, u16 Y, signed i16 tile index, two
reserved bytes, sub-tile byte, height byte, trailing reserved byte. Reserved
bytes and duplicate cell records remain available rather than being silently
rewritten. Waypoints encode Y*1000+X in file-coordinate space. Placed objects
retain all comma-separated fields, with typed house, kind, type ID, health,
file-coordinate cell, facing, mission and infantry subcell. Editor internal
variable naming swaps these axes; the frontend retains file order.

Size, LocalSize and Theater are checked. Parsing does not determine tile
passability: that requires theater templates/terrain rules and movement classes.
Trigger/team/script/terrain/lighting and related sections are retained with
runtime-pending diagnostics. Missing terrain or malformed placed records fail
explicitly. Authentic game and MOD fixture verification remains required.

Map parse diagnostics over the 371 supplied maps (2026-10-19; reproduce with
`cargo test -p ra2ne-runtime survey_map_parse_diagnostics -- --ignored --nocapture`)
total 3,050 and classify as:

| Category | Count | Handling |
| --- | ---: | --- |
| "N entries preserved; runtime semantics pending" | 3,001 | Informational: 15 map sections (Lighting, Terrain, PreviewPack, Actions, Events, Tags, Triggers, Houses, ScriptTypes, TaskForces, TeamTypes, Smudge, AITriggerTypes, CellTags, Countries) stay available through the map INI; trigger/AI/house/lighting simulation is not implemented. |
| Off-grid `Waypoints` values | 44 | Preserved but excluded from usable coordinates; the engine has no cell outside the grid. |
| Duplicate key inside one section | 3 | Last value wins, matching the original INI merge (`mapsmd03/310709fc` repeats `[YAGRND]`, `multimd/d508117d` repeats `BaseNormal`/`Sight`). |
| Entry outside any section | 2 | Ignored for lookups: `multi/tn01t2.map` writes `CampaignMoneyDeltaEasy/Hard`, which belong to RULES.INI `[General]` (stock rules.ini line 242) and are read section-scoped, so a bare map entry cannot override them. |

None of these categories is a syntax error in the original parser; all are
kept, and only the last two are annotated as text oddities rather than errors.

Rules support explicit source layers merged per section/key, keeping registry
positions stable when existing keys are overridden. An index avoids rescanning
the whole INI for every unit. Initial typed discovery handles four object
registries, Strength, Speed, Cost, Sight, primary/secondary weapon references,
owners, prerequisites, image/name/armor and basic weapon records. Decimals are
parsed as exact thousandths, never host floating point. Unknown properties and
unconsumed sections remain in the source with diagnostics. Defaults describe
this incomplete discovery schema, not undocumented original-engine defaults;
weapon/warhead/projectile/build/AI semantics are not implemented by this loader.


## Engine state and game transport formats

The little-endian engine formats are independent of original game
save/replay protocols: `RA2NEMV1` (movement), `RA2NEMP1` (navigation map),
`RA2NEGS5` (full game save), `RA2NEGR5` (full replay) and `RA2NEGF1` (input frame).
TCP handshake is `RA2NETP5`. Full-game v1/v2/v3/v4 files and v1/v2/v3/v4 peers are rejected after
adding armor/Verses, secondary weapons and typed production constraints to combat semantics, hashes and definitions; no migration is
provided. Movement/map/frame formats retain their v1 layouts.
Saves preserve shared route identities, and reconstruct/revalidate BFS topology
before accepting a movement state. Hashes detect corruption, not malicious
cryptographic forgery. Event buffers and spatial caches are transient.

Decoders limit whole state/replay files to 128 MiB, actors to 100,000, map/total
route cells to 4,194,304, players to 64, replay commands to 100,000 and replay
steps to 1,000,000. A frame is at most 64 KiB with 64 commands; each selection is
at most 1,024 handles and an accepted frame contains at most 4,096 selected handles.
Lockstep accepts at most 120 future Ticks. TCP adds a four-byte message length,
a versioned session handshake, at most 128 queued messages and bounded I/O per
poll. Session membership does not provide encrypted transport or matchmaking.

## Standard armor and direct warhead damage

The [Ares developer documentation](https://ares-developers.github.io/Ares-docs/new/additionalarmortypesandverses.html)
documents the eleven standard armor names/order and original Verses special
values. The experimental adapter implements their direct damage multipliers,
0% target rejection and 1%/2% passive-acquisition exclusions. This reference
does not establish exact original damage rounding: the engine currently uses
integer floor without a minimum-damage adjustment. Retaliation, splash, armor
extensions and immunity flags remain pending. Values require exactly eleven
percentage entries with at most three fractional digits and nonnegative u32
fixed-point magnitude. Unknown/custom armor and missing referenced Verses fail.


The experimental dual-weapon adapter uses primary-first armor eligibility,
with passive-acquisition restrictions evaluated per slot. Range is checked after
selection, and both slots share an actor cooldown. This is a bounded subset;
[Phobos developer documentation](https://phobos.readthedocs.io/en/latest/New-or-Enhanced-Logics.html)
describes secondary fallback and numerous special selection cases not implemented
here, including transport/deploy/ammo/AA behavior. NoSecondaryWeaponFallback is
retained with diagnostics. v3 also mixes the movement hash before hashing the
game Tick, preventing the two identical Tick XORs from cancelling in idle games.


Typed production v5 stores each definition's product category, optional factory
category and resolved prerequisite groups. Each group is a set of alternatives; all
groups are required. Limits: 1024 groups per type, 1024 members per group,
1,000,000 total; indices must refer to typed buildings. The
[Phobos AI mapping reference](https://phobos.readthedocs.io/en/build-47/AI-Scripting-and-Mapping.html)
identifies InfantryType/UnitType factory tags. Exact prerequisite ownership and
pause/resume rules here are an explicit experimental subset, not verified
original queue semantics; non-building alternates/faction/tech/building placement remain
pending. Restrictions participate in hashes and are restored before queues.


[Ares prerequisite documentation](https://ares-developers.github.io/Ares-docs/new/prerequisites.html)
identifies the six standard generic groups and custom `[GenericPrerequisites]`
overrides. The adapter implements their building-member subset, canonicalized
into OR-within/AND-between groups. Base lists must be explicit in source data;
original defaults are not fabricated. PROC vehicle alternates and other enhanced
prerequisite behavior remain pending. v5 persists group boundaries and bounds
total member allocations during decoding, before accepting game state.


## Real-package parsing refinements

Actual maps can end their terrain arrays with four extra bytes. The frontend
preserves this trailer separately; incomplete records of other lengths still
fail. Off-grid numeric waypoints are kept with diagnostics outside the usable
coordinate map. This enables inspection without inventing usable paths.

TMP flag words interpret only their three documented low bitfields. Unused high
bits and inactive metadata occur as uninitialized debug-fill values in actual
resources. They are retained, while active offsets/dimensions remain checked.

SHP uses the low format byte: 0/1 raw pixels, 2 stored-length raw rows, 3
length-prefixed transparent-run rows. Empty frame flags are ignored; nonempty
frames remain bounded. Final transparent runs clamp to row width, while literal
overruns still fail. See the independently consulted
[OpenRA SHP reader](https://github.com/OpenRA/OpenRA/blob/bleed/OpenRA.Mods.Common/SpriteLoaders/ShpTSLoader.cs)
and the [EA/XCC row decoder](https://github.com/electronicarts/CNC_TS_and_RA2_Mission_Editor/blob/main/3rdParty/xcc/misc/shp_decode.cpp).
No source code from these references is copied.

XCC local filename metadata contains a 32-byte signature, size/type/version,
game/count and NUL-terminated names. Size/count/name bounds and canonical names
are validated. It is optional archive metadata, not a requirement for named MIX
lookup and not authorization to extract paths. The audit preserves unknown entry
coverage instead of inferring complete resource/gameplay support.

See REAL_RESOURCE_VALIDATION.md for real-package checks and remaining limits.

## Theater lookup and basic TMP composition

The published [EA map editor](https://github.com/electronicarts/CNC_TS_and_RA2_Mission_Editor/blob/main/MissionEditor/MapData.cpp)
treats map tile ID `0xffff` as tile zero. The frontend preserves the raw signed
map ID, while theater lookup resolves only `-1` to zero; other negative IDs fail.
The [EA loading code](https://github.com/electronicarts/CNC_TS_and_RA2_Mission_Editor/blob/main/MissionEditor/Loading.cpp)
concatenates numeric TileSet counts, formats one-based two-digit filenames, reads
counts with decimal-prefix `atoi` semantics, merges the corresponding MD theater
INI and uses ISO palettes. NewUrban lookup can fall back to Urban, then all
non-temperate theaters can fall back to Temperate. The resolved extension must
also select its corresponding palette. Corrupt preferred files are errors rather
than a reason to silently choose another resource.

RA2NE applies decimal-prefix count handling only to this theater catalogue and
reports each non-strict value. General INI integer parsing remains strict. The
provided stock UrbanNMD INI contains `TilesInSet=o`, which resolves to zero in
the original editor and must not shift subsequent map IDs.

Engine checks (`gamemd.exe`): the theater loader `FUN_00545150` builds only the
`"%sMD.INI"` name (format string `0x8295e8`), so a YR install reads e.g.
`temperatmd.ini` alone — the base list is not merged. The supplied stock md
lists are supersets of the base lists for all three theaters, so the tile IDs
match the editor's merge. Tile lookup appends `.TEM` (string `0x829140`) and
`.URB` (string `0x829138`) after the environment extension and the
NewUrban→Urban fallback. A missing tile file, a map ID beyond the catalogue, a
subtile index past the TMP's tile list and a NULL tile block all draw nothing
in the original: subtile indices wrap modulo the tile count, and missing inputs
are neither errors nor substitution candidates.

[EA/XCC TMP geometry](https://github.com/electronicarts/CNC_TS_and_RA2_Mission_Editor/blob/main/3rdParty/xcc/misc/tmp_ts_file.cpp)
places base and extra graphics in the same coordinate system. RA2NE subtracts
the subtile's base X/Y from extra X/Y and composites nonzero extra indices over
the diamond; transparent extra pixels preserve base pixels. Bounding arithmetic
uses i64 before dimension conversion, limits each composite to 4,194,304 pixels,
and leaves Z planes separate. This basic image composition is not Z-buffer
rendering or an implementation of the original game's complete rendering order.

## Static overlay lookup

The [EA loading code](https://github.com/electronicarts/CNC_TS_and_RA2_Mission_Editor/blob/main/MissionEditor/Loading.cpp)
and [drawing code](https://github.com/electronicarts/CNC_TS_and_RA2_Mission_Editor/blob/main/MissionEditor/IsoView.cpp)
describe overlay enumeration, image aliases, palette selection and bridge/track
positioning. RA2NE uses ordered OverlayTypes values as byte IDs; numeric keys
are labels rather than sparse array indices. Invalid definitions retain their
slots. Byte 255 denotes no overlay. OverlayData is a direct SHP frame reference;
invalid references are reported without rewriting or clamping the source map.

YR uses rulesmd/artmd as its base catalogue, with map rules applied afterwards.
Theater-suffixed SHPs use the corresponding ISO palette; generic/NewTheater
SHPs use unit palettes. Resource overlays use the ordinary temperat.pal palette.
NewTheater lookup includes the generic G variant before other theater variants;
this is required for Yuri's GGFWLL wall in the supplied Desert/Lunar samples.
The installation profile also mounts conquer/conqmd and generic/genermd archives.
Corrupt preferred assets are errors. Unsupported custom art palettes are diagnosed.

The viewer retains the full SHP canvas and frame offsets, applies static
bridge/track offsets and culls by image bounds. Fully transparent frames and
references outside decoded terrain cells are counted separately. Source arrays
are preserved. RGBA caching is bounded to 128 MiB. Animation, damage selection,
per-pixel depth, remap/shadows and gameplay remain pending.

Engine checks (`gamemd.exe`): the map loader `FUN_005fd2e0` reads the overlay
pack byte by byte, skips `0xff` (no overlay) and otherwise indexes its type
array directly with that byte — `DAT_00a83d84[byte]`, no bounds check.
`FUN_005fe250` appends each new type and stores its own array index as the ID,
and `FUN_005fec70` returns an existing type when a name repeats: IDs are
first-seen order of distinct names, numeric INI keys are labels, and duplicates
or invalid definitions never shift later IDs. The draw entry `FUN_0047f6a0`
returns before drawing IDs 167 and 178 — the stock placeholder types `USELESS`
and `VEINHOLEDUMMY` — and draws nothing when the stored data byte is not below
the SHP's frame count. RA2NE implements those outcomes, including the modulo
wrap and NULL-block rules for terrain, and freezes the tolerated no-image
references in `apps/ra2ne-runtime/tests/data/map_no_image_expected.tsv` instead
of clamping frames or substituting resources. Ore/vein and rubble overlay
frame selection still uses the stored byte; the original computes ore frames
from cell coordinates.

## Placed scenery records and drawing anchor

`[Terrain]` records encode the cell as `1000*y+x` in the INI key and the named
TerrainTypes ID in the value. RA2NE exposes typed scenery records separately from
playable actors and retains the original INI. Coordinates must fit the 512x512
storage grid, image/type names are bounded, and duplicate cells are diagnosed
including differently formatted decimal keys. The graphics loader also checks
that the cell exists in decoded terrain before drawing it.

Static scenery shares stock image aliases, theater resource/palette lookup and
bounded decoding with overlays, but uses its own TerrainTypes registry and frame
zero. Both catalogues currently support at most 255 registered entries. The
[EA drawing code](https://github.com/electronicarts/CNC_TS_and_RA2_Mission_Editor/blob/main/MissionEditor/IsoView.cpp)
centers terrain-object canvases with a three-pixel vertical adjustment; RA2NE's
offset from the projected cell center is `(-width/2, -height/2-3)`.
The renderer tests the full image bounds independently of the cell's TMP bounds,
so tall trees remain drawable while their cell anchor is outside the viewport.
This is static drawing, not original per-pixel depth or terrain-object gameplay.

## Static original infantry Ready poses

Infantry art resolves the rules Image alias and optional art Image alias, reads
Sequence from the original art definition, and selects the first Ready frame
for each direction. Ready currently accepts three nonnegative fields:
`start,count,stride`; start/stride/count are checked against the SHP format's
10,000-frame limit. Zero count, negative numbers, extended/fixed-direction
fields and unsupported art flags/custom palettes are diagnosed.

The [EA drawing code](https://github.com/electronicarts/CNC_TS_and_RA2_Mission_Editor/blob/main/MissionEditor/IsoView.cpp)
uses infantry direction `(7-facing/32)%8`. The resulting frame is
`start + direction*stride`. Subcells 0 through 4 have projected offsets
`(0,-7)`, `(15,0)`, `(-15,0)`, `(0,7)` and `(0,0)` for 60x30 cells.
The complete SHP canvas is centered there, with map height shifting Y upward
by 15 pixels per level. Actors preserve the original map record order, and image
bounds determine culling. Palette index zero is transparent; unit palettes are
selected by current theater. Owner remap and shadows are not applied.

The narrow renderer currently accepts ordinary image.shp files. Voxel,
NewTheater, TerrainPalette and custom Palette infantry art are unsupported.
A type with an existing rules definition remains drawable when map overrides
replace its InfantryTypes registry label. This graphics behavior does not
establish the game's complete registration/INI-loading semantics.

## VXL geometry and HVA poses

The published [EA/XCC VXL structures](https://github.com/electronicarts/CNC_TS_and_RA2_Mission_Editor/blob/main/3rdParty/xcc/misc/cc_structures.h)
use an 802-byte file header, 28-byte limb headers and 92-byte limb footers. Body
size locates the footers; footer offsets locate start/end tables and run data.
Each x/y column is empty when both signed offsets are -1. Otherwise its inclusive
end bounds the byte stream. Runs contain Z skip, voxel count, color/normal pairs
and a repeated count byte; zero-color entries remain decoded voxels. RA2NE checks
offset ranges, table overlap, duplicate count bytes, height progress and exact
column consumption. Metadata includes raw palette/remap bytes, header flags,
limb scale/bounds/local transform, dimensions and normal mode/indices.

Reads are limited to 64 MiB, 256 limbs and 4,194,304 decoded voxels. Transforms,
scale and bounds must be finite; scale must be positive and bounds ordered.
Normal indices select an entry of the four engine normal tables (16, 36, 64 or
245 entries for the limb's normal generation byte); the lighting section below
covers how the engine turns that into a colour. The decoder itself performs no
rasterization.

HVA has a 24-byte header, 16-byte section names and 48-byte 3x4 matrices.
The [OpenRA HVA reader](https://github.com/OpenRA/OpenRA/blob/bleed/OpenRA.Mods.Cnc/FileFormats/HvaReader.cs)
reads matrices in frame-major, section-minor order. RA2NE follows this layout,
with a multi-frame/multi-section regression distinguishing it from section-major
indexing present in the older XCC helper. Frames are bounded to 4,096 and sections
to 256; byte size and finite matrix elements are checked before use. The generic
matrix application helper checks finite results, but does not yet combine VXL
local transforms, HVA translations and engine-specific model scaling.

Stock readers associate limb and HVA sections by index, not names. Some shipped
HTK/HTKtur files have differing names despite matching section counts. `bind`
checks equal counts and preserves index association; `bind_by_name` provides
optional strict tooling with explicit ambiguity/missing-name errors. The shipped
probe.hva is a 24-byte, one-frame, zero-section placeholder. It parses and is
counted separately, but cannot bind to a nonempty model or supply a matrix.
Anonymous HVA detection requires a nonempty section table and an exact size;
unknown resources are retained in audit coverage rather than assumed to be HVA.

## Voxel normal lighting and VPL sections

`gamemd.exe` keeps four normal tables for the VXL normal generations
(16/36/64/245 float triples at `0x846a08`, `0x846ac8`, `0x846c78`, `0x846f78`,
pointers at `0x8469e4`, counts at `0x8469f8`; the RA2 tables are generations 2
and 4). The light direction is a fixed world vector built once at startup
(`FUN_00754c00`: `(-1/sqrt(2), -1/sqrt(2), 0)` rotated about index-Y by
`0.785375`), so RA2NE folds it into
`crates/ra2ne-assets/src/normals.rs` together with the 16x section scale and no
longer needs an INI or map `[Lighting]` input for voxel shading.

For each drawn model the engine builds a shade byte per normal
(`FUN_007586f0`): with `L` and `U` the world-to-model transforms of the light
and of `(0, 0, 1)`, `H = normalize(L + U)`,
`diffuse = max(0, n·L)`, `specular = max(0, n·H / (3 - 2·n·H))` and
`shade = round(16 · (diffuse + specular))`. The draw step (`0x7576b9`–`0x7576e5`)
then uses `shade` as a section and the voxel's colour byte as the row index into
the section buffer at `0xb41178` (`sections[shade · 256 + colour]`).

`voxels.vpl` supplies those sections: a 16-byte header (first/last remap colour,
section count, reserved), a 768-byte 8-bit palette the engine ignores, and
`section count` tables of 256 bytes. The stock file is 8976 bytes with 32
sections and the remap range 16..=31. Sections equal to the section count (a
full `diffuse + specular` of 2.0) would read one table past the buffer in the
engine; RA2NE clamps those to the last section. The engine additionally feeds
the world-to-model matrix translation into the light transform; RA2NE applies
only the rotation (`Tᵀ · flip_y · R_yaw⁻¹`, mirroring the engine's transposed
world-to-model matrix).
