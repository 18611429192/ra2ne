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
pub struct Theater {
    pub tiles: Vec<TileFile>,
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
        for (expected, set) in sets.into_iter().enumerate() {
            if set != expected {
                return Err("missing tile set would shift map tile IDs");
            }
            let section = format!("TileSet{set:04}");
            let count = ini
                .get(&section, "TilesInSet")
                .ok_or("missing TilesInSet")?
                .integer()?;
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
        Ok(Self { tiles })
    }
    pub fn load(&self, index: i16, files: &Vfs) -> Result<Tmp, &'static str> {
        let tile = usize::try_from(index)
            .ok()
            .and_then(|i| self.tiles.get(i))
            .ok_or("map tile index outside theater catalogue")?;
        let file = files
            .get(&tile.filename)?
            .ok_or("theater TMP resource missing")?;
        Tmp::parse(Arc::from(file.bytes))
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
