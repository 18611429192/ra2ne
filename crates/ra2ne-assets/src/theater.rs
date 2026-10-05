//! Theater INI tile-number catalogue. This does not infer movement or lighting.
use crate::{ini::Ini, tmp::Tmp, vfs::Vfs};
use std::{collections::BTreeSet, sync::Arc};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TileFile {
    pub set: usize,
    pub number: usize,
    pub filename: String,
}
#[derive(Debug)]
pub struct ResolvedTile {
    pub tmp: Tmp,
    pub filename: String,
    pub source: String,
}
#[derive(Debug)]
pub struct Theater {
    pub tiles: Vec<TileFile>,
    pub diagnostics: Vec<String>,
}
impl Theater {
    /// Global map tile IDs concatenate TilesInSet in numeric section order.
    /// Zero-sized sets reserve no IDs. File numbers start at one.
    pub fn parse(text: &str, extension: &str) -> Result<Self, &'static str> {
        if !matches!(
            extension.to_ascii_lowercase().as_str(),
            "tem" | "sno" | "urb" | "des" | "ubn" | "lun"
        ) {
            return Err("unsupported theater extension");
        }
        let ini = Ini::parse(text)?;
        let mut sets = BTreeSet::new();
        for entry in ini.entries() {
            let section = entry.section.to_ascii_lowercase();
            if let Some(number) = section.strip_prefix("tileset") {
                if number.len() != 4 || !number.bytes().all(|b| b.is_ascii_digit()) {
                    return Err("invalid tile set section number");
                }
                sets.insert(
                    number
                        .parse::<usize>()
                        .map_err(|_| "invalid tile set number")?,
                );
            }
        }
        if sets.is_empty() {
            return Err("theater has no tile sets");
        }
        let mut tiles = Vec::new();
        let mut diagnostics = Vec::new();
        for (expected, set) in sets.into_iter().enumerate() {
            if set != expected {
                return Err("missing tile set would shift map tile IDs");
            }
            let section = format!("TileSet{set:04}");
            let value = &ini
                .get(&section, "TilesInSet")
                .ok_or("missing TilesInSet")?
                .value;
            // The original editor uses atoi here. Preserve its decimal-prefix
            // behavior locally, without relaxing the general INI integer API.
            let sign = usize::from(value.starts_with(['+', '-']));
            let digits = value.as_bytes()[sign..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
            let count = if digits == 0 {
                0
            } else {
                value[..sign + digits]
                    .parse::<i32>()
                    .map_err(|_| "tile count integer overflow")?
            };
            if value.parse::<i32>().is_err() {
                diagnostics.push(format!(
                    "{section}.TilesInSet={value:?}: legacy decimal-prefix count {count}"
                ));
            }
            if !(0..=4096).contains(&count) || tiles.len() + count as usize > 32768 {
                return Err("theater tile count exceeds map index limits");
            }
            if count == 0 {
                continue;
            }
            let name = &ini
                .get(&section, "FileName")
                .ok_or("missing tile FileName")?
                .value;
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            {
                return Err("invalid tile filename prefix");
            }
            for number in 1..=count as usize {
                tiles.push(TileFile {
                    set,
                    number,
                    filename: format!("{name}{number:02}.{}", extension.to_ascii_lowercase()),
                });
            }
        }
        Ok(Self { tiles, diagnostics })
    }
    /// The on-disk 0xffff map marker (-1) resolves to clear tile zero.
    /// Other negative indices remain invalid.
    pub fn load(&self, index: i16, files: &Vfs) -> Result<Tmp, &'static str> {
        Ok(self.resolve(index, files)?.tmp)
    }
    /// Original editor lookup uses .urb fallback for NewUrban, then .tem.
    /// Report the resolved filename so callers also select its palette.
    pub fn resolve(&self, index: i16, files: &Vfs) -> Result<ResolvedTile, &'static str> {
        let index = if index == -1 { 0 } else { index };
        let tile = usize::try_from(index)
            .ok()
            .and_then(|i| self.tiles.get(i))
            .ok_or("map tile index outside theater catalogue")?;
        let (stem, extension) = tile
            .filename
            .rsplit_once('.')
            .ok_or("invalid theater filename")?;
        let mut extensions = vec![extension];
        if extension == "ubn" {
            extensions.push("urb");
        }
        if extension != "tem" {
            extensions.push("tem");
        }
        for extension in extensions {
            let filename = format!("{stem}.{extension}");
            if let Some(file) = files.get(&filename)? {
                return Ok(ResolvedTile {
                    tmp: Tmp::parse(Arc::from(file.bytes))?,
                    filename,
                    source: file.source.to_owned(),
                });
            }
        }
        Err("theater TMP resource missing")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_order_and_empty_sets_preserve_ids() {
        let theater = Theater::parse("[TileSet0002]\nFileName=slope\nTilesInSet=2\n[TileSet0000]\nFileName=clear\nTilesInSet=1\n[TileSet0001]\nTilesInSet=0", "TEM").unwrap();
        assert_eq!(
            theater
                .tiles
                .iter()
                .map(|t| t.filename.as_str())
                .collect::<Vec<_>>(),
            ["clear01.tem", "slope01.tem", "slope02.tem"]
        );
        assert_eq!(theater.tiles[1].set, 2);
        assert!(theater.load(-1, &Vfs::default()).is_err());
        assert!(theater.load(3, &Vfs::default()).is_err());
        assert!(theater.load(0, &Vfs::default()).is_err());
    }
    #[test]
    fn legacy_tile_counts_are_reported_without_shifting_later_ids() {
        let theater = Theater::parse("[TileSet0000]\nFileName=clear\nTilesInSet=1\n[TileSet0001]\nTilesInSet=o\n[TileSet0002]\nFileName=slope\nTilesInSet=2ignored", "tem").unwrap();
        assert_eq!(theater.tiles.len(), 3);
        assert_eq!(theater.tiles[1].filename, "slope01.tem");
        assert_eq!(theater.diagnostics.len(), 2);
        assert!(
            Theater::parse(
                "[TileSet0000]\nFileName=clear\nTilesInSet=999999999999999999999ignored",
                "tem"
            )
            .is_err()
        );
    }
    #[test]
    fn theater_fallback_retains_palette_identity_and_rejects_corrupt_preferred_file() {
        let theater = Theater::parse("[TileSet0000]\nFileName=clear\nTilesInSet=1", "ubn").unwrap();
        let mut bytes = Vec::new();
        for n in [1_u32, 1, 8, 4, 20] {
            bytes.extend(n.to_le_bytes());
        }
        bytes.extend([0_u8; 52]);
        bytes.extend([7_u8; 16]);
        let mut files = Vfs::default();
        files
            .mount("temperate", vec![("clear01.tem".into(), bytes.clone())])
            .unwrap();
        assert_eq!(theater.resolve(0, &files).unwrap().filename, "clear01.tem");
        files
            .mount("urban", vec![("clear01.urb".into(), bytes)])
            .unwrap();
        let resolved = theater.resolve(0, &files).unwrap();
        assert_eq!(
            (resolved.filename.as_str(), resolved.source.as_str()),
            ("clear01.urb", "urban")
        );
        files
            .mount(
                "newurban",
                vec![("clear01.ubn".into(), b"corrupt".to_vec())],
            )
            .unwrap();
        assert!(theater.resolve(0, &files).is_err());
    }
    #[test]
    fn clear_marker_loads_zero_but_other_negative_ids_do_not() {
        let theater = Theater::parse("[TileSet0000]\nFileName=clear\nTilesInSet=1", "tem").unwrap();
        let mut bytes = Vec::new();
        for n in [1_u32, 1, 8, 4, 20] {
            bytes.extend(n.to_le_bytes());
        }
        bytes.extend([0_u8; 52]);
        bytes.extend([7_u8; 16]);
        let mut files = Vfs::default();
        files
            .mount("synthetic", vec![("clear01.tem".into(), bytes)])
            .unwrap();
        assert_eq!(
            theater.load(-1, &files).unwrap().diamond(0).unwrap(),
            theater.load(0, &files).unwrap().diamond(0).unwrap()
        );
        assert!(theater.load(-2, &files).is_err());
    }
    #[test]
    fn rejects_shifted_ids_and_unsafe_names() {
        for text in [
            "[TileSet0001]\nTilesInSet=0",
            "[TileSet0000]\nTilesInSet=-1",
            "[TileSet0000]\nTilesInSet=1\nFileName=../clear",
            "[TileSet0000]\nTilesInSet=32769\nFileName=clear",
        ] {
            assert!(Theater::parse(text, "tem").is_err());
        }
        assert!(Theater::parse("[TileSet0000]\nTilesInSet=0", "../tem").is_err());
    }
}
