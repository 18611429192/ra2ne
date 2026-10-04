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
looked up directly; raw inspection lists IDs and sizes. XCC name databases,
automatic game identification and automatic nested-mount discovery remain
pending. Nested MIX inspection currently follows explicit names, maximum depth
eight. Within-archive duplicate IDs are rejected as ambiguous. Overlapping byte
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
