//! `voxels.vpl`: the section tables the original engine uses to shade voxels.
//!
//! The stock file lives in `RA2.MIX`/`LOCAL.MIX` (`voxels.vpl`, 8976 bytes) and the
//! engine loads it during startup (`FUN_0052ba60` sets the name at 0x52bd68 through
//! `CCFileClass::Set_Name` and stores the section buffer at 0xb41178). The voxel
//! draw code reads it as `sections[shade * 256 + voxel_color]`
//! (`MOV CL,[EDX+EBX+0xb41178]` at 0x7576e5), where `shade` is the rounded
//! diffuse+specular term the engine builds per model (`FUN_007586f0`).
//!
//! Layout:
//!
//! ```text
//! u32 first remap colour | u32 last remap colour | u32 section count | u32 reserved
//! 256 x RGB palette (768 bytes)
//! section count x 256 byte tables: table[section][voxel colour] = palette index
//! ```
//!
//! The shipped file declares 32 sections and the remap range 16..=31, and the
//! embedded 8-bit palette is ignored by the engine.
const HEADER: usize = 16;
const PALETTE: usize = 768;
const MAX_SECTIONS: usize = 32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Vpl {
    /// House remap range from the header (the stock file uses 16..=31).
    pub remap: [u8; 2],
    /// Embedded palette; the engine replaces it with the unit palette.
    pub palette: [[u8; 3]; 256],
    sections: Vec<[u8; 256]>,
}

impl Vpl {
    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() < HEADER + PALETTE {
            return Err("truncated VPL header");
        }
        let u32_at = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let count = u32_at(8) as usize;
        if count == 0 || count > MAX_SECTIONS {
            return Err("VPL section count outside bounds");
        }
        if bytes.len() != HEADER + PALETTE + count * 256 {
            return Err("VPL size does not match its section count");
        }
        let remap = [u32_at(0).min(255) as u8, u32_at(4).min(255) as u8];
        let palette = std::array::from_fn(|i| {
            [
                bytes[HEADER + i * 3],
                bytes[HEADER + i * 3 + 1],
                bytes[HEADER + i * 3 + 2],
            ]
        });
        let sections = (0..count)
            .map(|s| {
                let at = HEADER + PALETTE + s * 256;
                std::array::from_fn(|i| bytes[at + i])
            })
            .collect();
        Ok(Self {
            remap,
            palette,
            sections,
        })
    }
    pub fn section_count(&self) -> usize {
        self.sections.len()
    }
    /// Palette index the engine draws for one voxel.
    ///
    /// Sections equal to the section count happen when `diffuse + specular`
    /// rounds to the full 16 scale; the original reads one table past the
    /// buffer there, so out-of-range sections clamp to the last table.
    pub fn shade(&self, section: i32, color: u8) -> u8 {
        let section = section.clamp(0, self.sections.len() as i32 - 1) as usize;
        self.sections[section][usize::from(color)]
    }
    pub fn section(&self, section: usize) -> Option<&[u8; 256]> {
        self.sections.get(section)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(sections: usize) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&31u32.to_le_bytes());
        bytes.extend_from_slice(&(sections as u32).to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        for i in 0..256u32 {
            bytes.extend_from_slice(&[i as u8, (i / 2) as u8, (i / 3) as u8]);
        }
        for s in 0..sections {
            for c in 0..256 {
                bytes.push((s * 256 + c) as u8);
            }
        }
        bytes
    }
    #[test]
    fn parses_engine_layout_and_shades() {
        let vpl = Vpl::parse(&fixture(32)).unwrap();
        assert_eq!(vpl.remap, [16, 31]);
        assert_eq!(vpl.section_count(), 32);
        assert_eq!(vpl.palette[3], [3, 1, 1]);
        assert_eq!(vpl.shade(0, 7), 7);
        assert_eq!(vpl.shade(31, 7), (31 * 256 + 7) as u8);
        // The engine reads one table past the buffer for full-brightness normals.
        assert_eq!(vpl.shade(32, 7), (31 * 256 + 7) as u8);
        assert_eq!(vpl.shade(-4, 7), 7);
        assert_eq!(vpl.section(3).unwrap()[9], (3 * 256 + 9) as u8);
        assert!(vpl.section(32).is_none());
    }
    #[test]
    fn rejects_wrong_sizes_and_counts() {
        assert!(Vpl::parse(&[]).is_err());
        assert!(Vpl::parse(&fixture(32)[..100]).is_err());
        let mut zero = fixture(1);
        zero[8..12].copy_from_slice(&0u32.to_le_bytes());
        assert!(Vpl::parse(&zero).is_err());
        let mut too_many = fixture(32);
        too_many[8..12].copy_from_slice(&33u32.to_le_bytes());
        assert!(Vpl::parse(&too_many).is_err());
        assert!(Vpl::parse(&fixture(32)[..fixture(32).len() - 1]).is_err());
    }
}
