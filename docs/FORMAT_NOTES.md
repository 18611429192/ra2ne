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
