//! Bounded map pack decoding: numeric INI chunks, Base64, block headers, LZO/LCW.
use crate::ini::Ini;
use base64::{Engine, engine::general_purpose::STANDARD};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
pub enum PackCodec {
    Lzo,
    Lcw,
}

/// Numbered pack lines are concatenated numerically, independent of physical
/// INI order. Duplicate indexes are rejected rather than silently losing data.
pub fn decode_section(
    ini: &Ini,
    section: &str,
    codec: PackCodec,
    max_output: usize,
) -> Result<Option<Vec<u8>>, &'static str> {
    let mut lines = BTreeMap::new();
    let mut size = 0_usize;
    for entry in ini.section_entries(section) {
        let index: u32 = entry
            .key
            .parse()
            .map_err(|_| "pack line index is not numeric")?;
        if index == 0 || lines.insert(index, entry.value.as_str()).is_some() {
            return Err("invalid or duplicate pack line index");
        }
        size = size
            .checked_add(entry.value.len())
            .ok_or("pack text size overflow")?;
        if size > 16 * 1024 * 1024 {
            return Err("pack text exceeds limit");
        }
    }
    if lines.is_empty() {
        return Ok(None);
    }
    let mut text = String::with_capacity(size);
    for (expected, (&index, value)) in (1_u32..).zip(&lines) {
        if expected != index {
            return Err("missing pack line");
        }
        text.push_str(value);
    }
    let packed = STANDARD.decode(text).map_err(|_| "invalid pack Base64")?;
    decode_blocks(&packed, codec, max_output).map(Some)
}

pub fn decode_blocks(
    mut bytes: &[u8],
    codec: PackCodec,
    max_output: usize,
) -> Result<Vec<u8>, &'static str> {
    let mut out = Vec::new();
    while !bytes.is_empty() {
        let header = bytes.get(..4).ok_or("truncated pack block header")?;
        let compressed = u16::from_le_bytes(header[..2].try_into().unwrap()) as usize;
        let decompressed = u16::from_le_bytes(header[2..].try_into().unwrap()) as usize;
        if compressed == 0 || decompressed == 0 {
            return Err("zero-length pack block");
        }
        let block = bytes.get(4..4 + compressed).ok_or("truncated pack block")?;
        let end = out
            .len()
            .checked_add(decompressed)
            .ok_or("pack output size overflow")?;
        if end > max_output {
            return Err("pack output exceeds limit");
        }
        match codec {
            PackCodec::Lzo => {
                let start = out.len();
                out.resize(end, 0);
                let count = lzokay::decompress::decompress(block, &mut out[start..])
                    .map_err(|_| "invalid LZO block")?;
                if count != decompressed {
                    return Err("LZO output size mismatch");
                }
            }
            PackCodec::Lcw => {
                let decoded = decode_lcw(block, decompressed)?;
                if decoded.len() != decompressed {
                    return Err("LCW output size mismatch");
                }
                out.extend(decoded);
            }
        }
        bytes = &bytes[4 + compressed..];
    }
    Ok(out)
}

/// LCW / Format80: literals, relative short copies, absolute long copies and
/// fill runs. Back references may overlap, but cannot read unwritten bytes.
pub fn decode_lcw(bytes: &[u8], max_output: usize) -> Result<Vec<u8>, &'static str> {
    struct Cursor<'a>(&'a [u8]);
    impl Cursor<'_> {
        fn byte(&mut self) -> Result<u8, &'static str> {
            let (&v, tail) = self.0.split_first().ok_or("truncated LCW command")?;
            self.0 = tail;
            Ok(v)
        }
        fn word(&mut self) -> Result<usize, &'static str> {
            Ok(usize::from(self.byte()?) | (usize::from(self.byte()?) << 8))
        }
    }
    let mut cursor = Cursor(bytes);
    let mut out = Vec::new();
    loop {
        let code = cursor.byte()?;
        if code == 0x80 {
            if !cursor.0.is_empty() {
                return Err("trailing LCW bytes");
            }
            return Ok(out);
        }
        let (count, source) = match code {
            0x00..=0x7f => {
                let count = usize::from(code >> 4) + 3;
                let distance = (usize::from(code & 15) << 8) | usize::from(cursor.byte()?);
                let source = out
                    .len()
                    .checked_sub(distance)
                    .ok_or("LCW back reference before start")?;
                (count, Some(source))
            }
            0x81..=0xbf => (usize::from(code & 63), None),
            0xc0..=0xfd => (usize::from(code & 63) + 3, Some(cursor.word()?)),
            0xfe => {
                let count = cursor.word()?;
                let value = cursor.byte()?;
                let end = out.len().checked_add(count).ok_or("LCW size overflow")?;
                if end > max_output {
                    return Err("LCW output exceeds limit");
                }
                out.resize(end, value);
                continue;
            }
            0xff => {
                let count = cursor.word()?;
                (count, Some(cursor.word()?))
            }
            _ => unreachable!(),
        };
        if count > max_output.saturating_sub(out.len()) {
            return Err("LCW output exceeds limit");
        }
        if let Some(source) = source {
            if count > 0 && source >= out.len() {
                return Err("LCW back reference reads unwritten output");
            }
            for offset in 0..count {
                out.push(out[source + offset]);
            }
        } else {
            for _ in 0..count {
                out.push(cursor.byte()?);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lcw_handles_literal_overlapping_copies_absolute_copies_and_fill() {
        let packed = [
            0x83, b'a', b'b', b'c', 0x00, 3, 0xc0, 0, 0, 0xfe, 2, 0, b'x', 0xff, 3, 0, 0, 0, 0x80,
        ];
        assert_eq!(decode_lcw(&packed, 14).unwrap(), b"abcabcabcxxabc");
        assert_eq!(
            decode_lcw(&[0x81, b'a', 0x10, 1, 0x80], 5).unwrap(),
            b"aaaaa"
        );
        for bad in [
            &[0x00, 1, 0x80][..],
            &[0x81, b'a', 0x00, 0, 0x80],
            &[0x80, 0],
            &[0x81],
            &[0xff, 1, 0, 9, 0, 0x80],
        ] {
            assert!(decode_lcw(bad, 100).is_err());
        }
        assert!(decode_lcw(&packed, 13).is_err());
    }
    #[test]
    fn numeric_pack_order_and_lzo_blocks_preserve_payload() {
        let raw = b"independent block data";
        let compressed = lzokay::compress::compress(raw).unwrap();
        let mut packed = Vec::new();
        packed.extend((compressed.len() as u16).to_le_bytes());
        packed.extend((raw.len() as u16).to_le_bytes());
        packed.extend(compressed);
        let text = STANDARD.encode(&packed);
        let at = text.len() / 2;
        let ini = Ini::parse(&format!(
            "[IsoMapPack5]\n2={}\n1={}\n",
            &text[at..],
            &text[..at]
        ))
        .unwrap();
        assert_eq!(
            decode_section(&ini, "IsoMapPack5", PackCodec::Lzo, 100)
                .unwrap()
                .unwrap(),
            raw
        );
        assert!(decode_blocks(&packed, PackCodec::Lzo, 1).is_err());
        for n in 1..packed.len() {
            assert!(decode_blocks(&packed[..n], PackCodec::Lzo, 100).is_err());
        }
        assert!(
            decode_section(&Ini::parse("[X]\n2=a\n").unwrap(), "X", PackCodec::Lzo, 100).is_err()
        );
        assert!(
            decode_section(
                &Ini::parse("[X]\n1=a\n1=b\n").unwrap(),
                "X",
                PackCodec::Lzo,
                100
            )
            .is_err()
        );
    }
    #[test]
    fn independently_encoded_lzo_literal_vector_decodes() {
        // LZO initial literal opcode 17+length, followed by end marker 17,0,0.
        let raw = b"hello";
        let block = [22, b'h', b'e', b'l', b'l', b'o', 17, 0, 0];
        let mut packed = vec![block.len() as u8, 0, 5, 0];
        packed.extend(block);
        assert_eq!(decode_blocks(&packed, PackCodec::Lzo, 5).unwrap(), raw);
    }
}
