//! Bounded Westwood MIX archives, including encrypted RA/TS/RA2 directories.
//! Format facts: EA mission editor's XCC mix_file.cpp / mix_decode.cpp.
//! This module is a new Rust implementation; no original assets are included.
use blowfish::{
    Blowfish,
    cipher::{BlockDecrypt, KeyInit, generic_array::GenericArray},
};
use num_bigint::BigUint;
use sha1::{Digest, Sha1};
use std::{collections::BTreeMap, ops::Range, sync::Arc};

const CHECKSUM: u32 = 0x0001_0000;
const ENCRYPTED: u32 = 0x0002_0000;
pub const MAX_ARCHIVE_BYTES: usize = 2_147_483_647;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilenameHash {
    /// Early C&C / Red Alert rotate-add filename hash.
    Classic,
    /// Tiberian Sun / RA2 / Yuri's Revenge CRC32 with Westwood padding.
    Ra2,
}

pub fn filename_id(name: &str, kind: FilenameHash) -> Result<u32, &'static str> {
    let canonical = crate::vfs::canonical_name(name)?;
    if !canonical.is_ascii() {
        return Err("MIX lookup requires ASCII asset names");
    }
    let mut bytes = canonical
        .replace('/', "\\")
        .to_ascii_uppercase()
        .into_bytes();
    match kind {
        FilenameHash::Classic => {
            let mut id = 0_u32;
            for chunk in bytes.chunks(4) {
                let mut word = [0_u8; 4];
                word[..chunk.len()].copy_from_slice(chunk);
                id = id.rotate_left(1).wrapping_add(u32::from_le_bytes(word));
            }
            Ok(id)
        }
        FilenameHash::Ra2 => {
            let remainder = bytes.len() % 4;
            if remainder != 0 {
                let first = bytes[bytes.len() - remainder];
                bytes.push(remainder as u8);
                bytes.extend(std::iter::repeat_n(first, 3 - remainder));
            }
            Ok(crc32(&bytes))
        }
    }
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

#[derive(Clone, Debug)]
pub struct MixArchive {
    bytes: Arc<[u8]>,
    entries: BTreeMap<u32, Range<usize>>,
    pub encrypted: bool,
    pub checksum_verified: bool,
}
impl MixArchive {
    /// XCC's optional filename metadata. MIX lookup itself never depends on it.
    pub fn local_names(&self) -> Result<Option<LocalNameDatabase>, &'static str> {
        self.get("local mix database.dat", FilenameHash::Ra2)?
            .map(LocalNameDatabase::parse)
            .transpose()
    }
    pub fn parse(bytes: Arc<[u8]>) -> Result<Self, &'static str> {
        if bytes.len() < 6 || bytes.len() > MAX_ARCHIVE_BYTES {
            return Err("invalid MIX length");
        }
        let legacy = u16_at(&bytes, 0)? != 0;
        let flags = if legacy { 0 } else { u32_at(&bytes, 0)? };
        if flags & !(CHECKSUM | ENCRYPTED) != 0 {
            return Err("unknown MIX flags");
        }
        let encrypted = flags & ENCRYPTED != 0;
        let (header, data_start): (Vec<u8>, usize) = if encrypted {
            let source = bytes.get(4..84).ok_or("truncated MIX key source")?;
            let cipher: Blowfish = Blowfish::new_from_slice(&derive_key(source)?)
                .map_err(|_| "invalid MIX Blowfish key")?;
            let first = bytes.get(84..92).ok_or("truncated encrypted MIX header")?;
            let mut block = GenericArray::clone_from_slice(first);
            cipher.decrypt_block(&mut block);
            let count = u16_at(&block, 0)? as usize;
            let header_size = 6 + count * 12;
            let padded = (header_size + 7) & !7;
            let end = 84 + padded;
            let mut decrypted = bytes
                .get(84..end)
                .ok_or("truncated encrypted MIX index")?
                .to_vec();
            for chunk in decrypted.as_chunks_mut::<8>().0 {
                cipher.decrypt_block(GenericArray::from_mut_slice(chunk));
            }
            decrypted.truncate(header_size);
            (decrypted, end)
        } else {
            let start = if legacy { 0 } else { 4 };
            let count = u16_at(&bytes, start)? as usize;
            let end = start + 6 + count * 12;
            (
                bytes.get(start..end).ok_or("truncated MIX index")?.to_vec(),
                end,
            )
        };
        let count = u16_at(&header, 0)? as usize;
        let data_size = u32_at(&header, 2)? as usize;
        let data_end = data_start
            .checked_add(data_size)
            .ok_or("MIX body size overflow")?;
        let checksum_size = if flags & CHECKSUM != 0 { 20 } else { 0 };
        if data_end.checked_add(checksum_size) != Some(bytes.len()) {
            return Err("MIX body size mismatch");
        }
        let mut entries = BTreeMap::new();
        for index in 0..count {
            let at = 6 + index * 12;
            let id = u32_at(&header, at)?;
            let offset = u32_at(&header, at + 4)? as usize;
            let length = u32_at(&header, at + 8)? as usize;
            let end = offset
                .checked_add(length)
                .ok_or("MIX entry size overflow")?;
            if end > data_size {
                return Err("MIX entry outside body");
            }
            if entries
                .insert(id, data_start + offset..data_start + end)
                .is_some()
            {
                return Err("duplicate MIX file ID");
            }
        }
        let checksum_verified = flags & CHECKSUM != 0;
        if checksum_verified {
            let digest = Sha1::digest(&bytes[data_start..data_end]);
            if digest.as_slice() != &bytes[data_end..] {
                return Err("MIX checksum mismatch");
            }
        }
        Ok(Self {
            bytes,
            entries,
            encrypted,
            checksum_verified,
        })
    }
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
    pub fn entries(&self) -> impl Iterator<Item = (u32, usize)> + '_ {
        self.entries.iter().map(|(&id, range)| (id, range.len()))
    }
    pub fn get_id(&self, id: u32) -> Option<&[u8]> {
        self.entries
            .get(&id)
            .map(|range| &self.bytes[range.clone()])
    }
    pub fn get(&self, name: &str, kind: FilenameHash) -> Result<Option<&[u8]>, &'static str> {
        Ok(self.get_id(filename_id(name, kind)?))
    }
}

#[derive(Clone, Debug)]
pub struct LocalNameDatabase {
    pub game: u32,
    pub names: Vec<String>,
}
impl LocalNameDatabase {
    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        const MAGIC: &[u8] = b"XCC by Olaf van der Spek\x1a\x04\x17\x27\x10\x19\x80\x00";
        if bytes.len() < 52 || bytes.len() > 16 * 1024 * 1024 || bytes.get(..32) != Some(MAGIC) {
            return Err("invalid XCC local name database header");
        }
        if u32_at(bytes, 32)? as usize != bytes.len()
            || u32_at(bytes, 36)? != 0
            || u32_at(bytes, 40)? != 0
        {
            return Err("unsupported XCC database size/type/version");
        }
        let game = u32_at(bytes, 44)?;
        let count = u32_at(bytes, 48)? as usize;
        if count > 65_535 {
            return Err("XCC filename count exceeds MIX limit");
        }
        let mut tail = &bytes[52..];
        let mut names = Vec::with_capacity(count);
        for _ in 0..count {
            let end = tail
                .iter()
                .position(|&b| b == 0)
                .ok_or("unterminated XCC filename")?;
            if end == 0 || end > 1024 || !tail[..end].is_ascii() {
                return Err("invalid XCC filename");
            }
            let name = std::str::from_utf8(&tail[..end]).unwrap();
            crate::vfs::canonical_name(name)?;
            names.push(name.to_owned());
            tail = &tail[end + 1..];
        }
        if !tail.is_empty() {
            return Err("trailing XCC filename data");
        }
        Ok(Self { game, names })
    }
}
fn u16_at(bytes: &[u8], at: usize) -> Result<u16, &'static str> {
    Ok(u16::from_le_bytes(
        bytes
            .get(at..at + 2)
            .ok_or("truncated MIX integer")?
            .try_into()
            .unwrap(),
    ))
}
fn u32_at(bytes: &[u8], at: usize) -> Result<u32, &'static str> {
    Ok(u32::from_le_bytes(
        bytes
            .get(at..at + 4)
            .ok_or("truncated MIX integer")?
            .try_into()
            .unwrap(),
    ))
}

/// Public Westwood modulus (DER integer payload) and exponent 65537. Two
/// little-endian 40-byte RSA blocks yield 39 bytes each; first 56 are the key.
/// Resource decoding only: this legacy construction is not application crypto.
fn derive_key(source: &[u8]) -> Result<[u8; 56], &'static str> {
    if source.len() != 80 {
        return Err("invalid MIX key source length");
    }
    let modulus = BigUint::from_bytes_be(&[
        0x51, 0xbc, 0xda, 0x08, 0x6d, 0x39, 0xfc, 0xe4, 0x56, 0x51, 0x60, 0xd6, 0x51, 0x71, 0x3f,
        0xa2, 0xe8, 0xaa, 0x54, 0xfa, 0x66, 0x82, 0xb0, 0x4a, 0xab, 0xdd, 0x0e, 0x6a, 0xf8, 0xb0,
        0xc1, 0xe6, 0xd1, 0xfb, 0x4f, 0x3d, 0xaa, 0x43, 0x7f, 0x15,
    ]);
    let exponent = BigUint::from(65_537_u32);
    let mut decoded = [0_u8; 78];
    for (index, chunk) in source.as_chunks::<40>().0.iter().enumerate() {
        let bytes = BigUint::from_bytes_le(chunk)
            .modpow(&exponent, &modulus)
            .to_bytes_le();
        let count = bytes.len().min(39);
        decoded[index * 39..index * 39 + count].copy_from_slice(&bytes[..count]);
    }
    Ok(decoded[..56].try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xcc_filename_metadata_is_bounded_and_never_becomes_a_disk_path() {
        let mut bytes = b"XCC by Olaf van der Spek\x1a\x04\x17\x27\x10\x19\x80\x00".to_vec();
        for value in [0_u32, 0, 0, 5, 2] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend(b"Tank.SHP\x00local.mix\x00");
        let length = bytes.len() as u32;
        bytes[32..36].copy_from_slice(&length.to_le_bytes());
        let names = LocalNameDatabase::parse(&bytes).unwrap();
        assert_eq!(names.game, 5);
        assert_eq!(names.names, ["Tank.SHP", "local.mix"]);
        for n in 0..bytes.len() {
            assert!(LocalNameDatabase::parse(&bytes[..n]).is_err());
        }
        let mut bad = bytes.clone();
        bad[48..52].copy_from_slice(&65_536_u32.to_le_bytes());
        assert!(LocalNameDatabase::parse(&bad).is_err());
        let mut bad = bytes.clone();
        bad[40] = 1;
        assert!(LocalNameDatabase::parse(&bad).is_err());
        let mut bad = bytes.clone();
        bad[52..60].copy_from_slice(b"../x.shp");
        assert!(LocalNameDatabase::parse(&bad).is_err());
        let mut bad = bytes;
        *bad.last_mut().unwrap() = b'x';
        assert!(LocalNameDatabase::parse(&bad).is_err());
    }
    use blowfish::cipher::BlockEncrypt;
    fn fixture(legacy: bool, flags: u32) -> Vec<u8> {
        let payload = b"[General]\nName=Synthetic\n";
        let mut header = Vec::new();
        header.extend(1_u16.to_le_bytes());
        header.extend((payload.len() as u32).to_le_bytes());
        header.extend(
            filename_id("rulesmd.ini", FilenameHash::Ra2)
                .unwrap()
                .to_le_bytes(),
        );
        header.extend(0_u32.to_le_bytes());
        header.extend((payload.len() as u32).to_le_bytes());
        let mut result = Vec::new();
        if !legacy {
            result.extend(flags.to_le_bytes());
        }
        if flags & ENCRYPTED != 0 {
            let mut source = [0_u8; 80];
            source[0] = 1;
            source[40] = 1;
            result.extend(source);
            let cipher: Blowfish = Blowfish::new_from_slice(&derive_key(&source).unwrap()).unwrap();
            header.resize((header.len() + 7) & !7, 0);
            for chunk in header.as_chunks_mut::<8>().0 {
                cipher.encrypt_block(GenericArray::from_mut_slice(chunk));
            }
        }
        result.extend(header);
        result.extend(payload);
        if flags & CHECKSUM != 0 {
            result.extend(Sha1::digest(payload));
        }
        result
    }
    #[test]
    fn all_header_variants_resolve_same_payload() {
        for (legacy, flags) in [
            (true, 0),
            (false, 0),
            (false, CHECKSUM),
            (false, ENCRYPTED),
            (false, ENCRYPTED | CHECKSUM),
        ] {
            let mix = MixArchive::parse(Arc::from(fixture(legacy, flags))).unwrap();
            assert_eq!(mix.entry_count(), 1);
            assert_eq!(mix.encrypted, flags & ENCRYPTED != 0);
            assert_eq!(mix.checksum_verified, flags & CHECKSUM != 0);
            assert_eq!(
                mix.get("RulesMD.INI", FilenameHash::Ra2).unwrap(),
                Some(b"[General]\nName=Synthetic\n".as_slice())
            );
            assert!(mix.get("missing.ini", FilenameHash::Ra2).unwrap().is_none());
        }
    }
    #[test]
    fn hash_matches_independent_python_zlib_vectors() {
        // Generated independently from the XCC padding facts with Python zlib.
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(
            filename_id("local mix database.dat", FilenameHash::Ra2).unwrap(),
            0x366e_051f
        );
        assert_eq!(
            filename_id("rulesmd.ini", FilenameHash::Ra2).unwrap(),
            0x8218f9f4
        );
        assert_eq!(
            filename_id("a/b.ini", FilenameHash::Ra2),
            filename_id("A\\B.INI", FilenameHash::Ra2)
        );
        assert!(filename_id("../rules.ini", FilenameHash::Ra2).is_err());
    }
    #[test]
    fn truncated_and_invalid_archives_never_return_partial_index() {
        for flags in [0, CHECKSUM, ENCRYPTED, ENCRYPTED | CHECKSUM] {
            let bytes = fixture(false, flags);
            for n in 0..bytes.len() {
                assert!(MixArchive::parse(Arc::from(&bytes[..n])).is_err());
            }
            let mut extra = bytes.clone();
            extra.push(0);
            assert!(MixArchive::parse(Arc::from(extra)).is_err());
        }
        let mut bytes = fixture(false, 0);
        bytes[18..22].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(MixArchive::parse(Arc::from(bytes)).is_err());
        let mut bytes = fixture(false, CHECKSUM);
        *bytes.last_mut().unwrap() ^= 1;
        assert!(MixArchive::parse(Arc::from(bytes)).is_err());
        assert!(MixArchive::parse(Arc::from([0, 0, 4, 0, 0, 0])).is_err());
    }
    #[test]
    fn rsa_key_derivation_matches_independent_modular_exponentiation() {
        let source = std::array::from_fn::<_, 80, _>(|i| i as u8);
        let actual = derive_key(&source).unwrap();
        let expected = [
            0xfd, 0x0b, 0x07, 0x7d, 0x57, 0x80, 0xa7, 0xd0, 0x8d, 0x96, 0x68, 0x1d, 0xcb, 0xe1,
            0xbe, 0xa9, 0x45, 0x0e, 0x77, 0xfc, 0xba, 0x66, 0x32, 0x45, 0x1c, 0x94, 0x20, 0x8a,
            0x07, 0x11, 0x27, 0x4b, 0x90, 0x5a, 0x20, 0x5f, 0x5a, 0xff, 0x6b, 0xc0, 0x4d, 0x92,
            0x0b, 0x8f, 0x3d, 0x2f, 0x29, 0x86, 0x8b, 0xe2, 0x03, 0x0a, 0x36, 0xd4, 0x0c, 0x8e,
        ];
        assert_eq!(actual, expected);
    }
    #[test]
    fn independently_encrypted_python_fixture_decodes() {
        // Fixture built with Python pow, zlib, hashlib and cryptography Blowfish.
        let bytes = [
            0x00, 0x00, 0x03, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09,
            0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
            0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25,
            0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f, 0x30, 0x31, 0x32, 0x33,
            0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f, 0x40, 0x41,
            0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x4f,
            0xbb, 0xc0, 0x2a, 0x91, 0xf4, 0xbc, 0xb7, 0x81, 0x29, 0x25, 0xb0, 0xce, 0x87, 0x46,
            0x4a, 0x12, 0x14, 0xa9, 0x52, 0xcb, 0x59, 0x16, 0x87, 0xbc, 0x5b, 0x47, 0x65, 0x6e,
            0x65, 0x72, 0x61, 0x6c, 0x5d, 0x0a, 0x4e, 0x61, 0x6d, 0x65, 0x3d, 0x49, 0x6e, 0x64,
            0x65, 0x70, 0x65, 0x6e, 0x64, 0x65, 0x6e, 0x74, 0x46, 0x69, 0x78, 0x74, 0x75, 0x72,
            0x65, 0x0a, 0x08, 0xc6, 0x35, 0x3c, 0xb3, 0x57, 0xfc, 0xc0, 0xb7, 0x99, 0xd6, 0x75,
            0x90, 0xc5, 0x55, 0x86, 0x26, 0x26, 0x37, 0xa3,
        ];
        let mix = MixArchive::parse(Arc::from(bytes)).unwrap();
        assert!(mix.encrypted && mix.checksum_verified);
        assert_eq!(
            mix.get("rulesmd.ini", FilenameHash::Ra2).unwrap(),
            Some(b"[General]\nName=IndependentFixture\n".as_slice())
        );
    }
    #[test]
    fn duplicate_archive_ids_are_rejected_instead_of_choosing_a_winner() {
        let mut bytes = fixture(false, 0);
        bytes[4..6].copy_from_slice(&2_u16.to_le_bytes());
        let index = bytes[10..22].to_vec();
        bytes.splice(22..22, index);
        assert_eq!(
            MixArchive::parse(Arc::from(bytes)).unwrap_err(),
            "duplicate MIX file ID"
        );
    }
}
