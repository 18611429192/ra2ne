//! Explicit resource mounts for the map viewer; no automatic game search order.
use macroquad::prelude::*;
use ra2ne_assets::{
    map::Ra2Map,
    mix::{FilenameHash, MixArchive},
    sprite::Palette,
    theater::Theater,
    vfs::Vfs,
};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Options {
    pub game_dir: Option<String>,
    pub edition: Option<super::installation::Edition>,
    pub ini: Option<String>,
    pub palette: Option<String>,
    pub mixes: Vec<String>,
}
impl Options {
    pub fn enabled(&self) -> bool {
        self.game_dir.is_some()
            || self.edition.is_some()
            || self.ini.is_some()
            || self.palette.is_some()
            || !self.mixes.is_empty()
    }
    pub fn validate(&self, map: bool) -> Result<(), String> {
        if self.game_dir.is_some() {
            if !map
                || self.edition.is_none()
                || self.ini.is_some()
                || self.palette.is_some()
                || !self.mixes.is_empty()
            {
                return Err("game-dir terrain requires --map and --edition=ra2|yr and cannot combine explicit terrain files".into());
            }
            return Ok(());
        }
        if self.edition.is_some() {
            return Err("edition requires game-dir".into());
        }
        if self.enabled()
            && (!map
                || self.ini.is_none()
                || self.palette.is_none()
                || self.mixes.is_empty()
                || self.mixes.len() > 16)
        {
            return Err("terrain requires --map, --terrain-ini, --terrain-palette and 1..16 --terrain-mix mounts".into());
        }
        Ok(())
    }
}
struct Image {
    width: u16,
    height: u16,
    rgba: Vec<u8>,
    offset: (i32, i32),
}
pub struct TerrainTexture {
    pub texture: Texture2D,
    pub offset: (i32, i32),
}
pub struct Terrain {
    cells: BTreeMap<(i32, i32), (i16, u8)>,
    images: BTreeMap<(i16, u8), Image>,
    pub report: String,
    pub overlays: Option<super::overlays::Scene>,
}
impl Terrain {
    pub fn load(
        options: &Options,
        map: &Ra2Map,
        encoding: ra2ne_assets::text::TextEncoding,
        map_text: &str,
    ) -> Result<Self, String> {
        options.validate(true)?;
        if !options.enabled() {
            return Err("terrain options are absent".into());
        }
        let extension = match map.theater.to_ascii_uppercase().as_str() {
            "TEMPERATE" => "tem",
            "SNOW" => "sno",
            "URBAN" => "urb",
            "DESERT" => "des",
            "NEWURBAN" => "ubn",
            "LUNAR" => "lun",
            _ => return Err("unsupported map theater".into()),
        };
        let (theater, palette, files) = if let Some(root) = &options.game_dir {
            super::installation::load(root, options.edition.unwrap(), &map.theater, encoding)?
        } else {
            let ini_bytes = super::read(options.ini.as_ref().unwrap(), 16 * 1024 * 1024)?;
            let ini = encoding.decode(&ini_bytes)?;
            let theater = Theater::parse(&ini, extension)?;
            let palette = Palette::parse(&super::read(options.palette.as_ref().unwrap(), 768)?)?;
            let mut files = Vfs::default();
            let mut mounted = 0;
            for path in &options.mixes {
                let bytes = super::read(path, 256 * 1024 * 1024 - mounted)?;
                mounted += bytes.len();
                if mounted > 256 * 1024 * 1024 {
                    return Err("terrain mount total exceeds archive budget".into());
                }
                files.mount_mix(path, MixArchive::parse(bytes.into())?, FilenameHash::Ra2)?;
            }
            (theater, palette, files)
        };
        let overlays = if options.game_dir.is_some() {
            Some(super::overlays::Scene::load(
                &files,
                map,
                map_text,
                options.edition.unwrap(),
                encoding,
            )?)
        } else {
            None
        };
        for diagnostic in &theater.diagnostics {
            eprintln!("theater catalogue: {diagnostic}");
        }
        let mut palettes = BTreeMap::new();
        palettes.insert(extension.to_owned(), palette);
        for (ext, name) in [("tem", "isotem.pal"), ("urb", "isourb.pal")] {
            if !palettes.contains_key(ext)
                && let Some(file) = files.get(name)?
            {
                palettes.insert(ext.to_owned(), Palette::parse(file.bytes)?);
            }
        }
        let cells = map
            .tiles
            .iter()
            .map(|t| {
                (
                    (i32::from(t.cell.x), i32::from(t.cell.y)),
                    (t.tile_index, t.sub_tile),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut images = BTreeMap::new();
        let mut decoded = 0;
        let mut missing = 0;
        let mut extras = 0;
        let mut references = BTreeMap::<i16, BTreeMap<u8, usize>>::new();
        for &(id, sub) in cells.values() {
            *references.entry(id).or_default().entry(sub).or_default() += 1;
        }
        for (id, subtiles) in references {
            let resolved = match theater.resolve(id, &files) {
                Ok(resolved) => resolved,
                Err(error) => {
                    missing += subtiles.values().sum::<usize>();
                    eprintln!("terrain tile {id}: {error}");
                    continue;
                }
            };
            let source_extension = resolved.filename.rsplit_once('.').unwrap().1;
            let Some(palette) = palettes.get(source_extension) else {
                missing += subtiles.values().sum::<usize>();
                eprintln!(
                    "terrain tile {id}: palette missing for {}",
                    resolved.filename
                );
                continue;
            };
            let tmp = resolved.tmp;
            if tmp.cell_width != 60 || tmp.cell_height != 30 {
                return Err("viewer requires 60x30 TMP cells".into());
            }
            for (sub, cell_count) in subtiles {
                let composite = match tmp.composite(usize::from(sub)) {
                    Ok(image) => image,
                    Err(error) => {
                        missing += cell_count;
                        eprintln!("terrain tile {id}/{sub}: {error}");
                        continue;
                    }
                };
                let image = composite.image;
                if tmp.extra(usize::from(sub)).is_some() {
                    extras += 1;
                }
                decoded += image.pixels.len() * 4;
                if decoded > 128 * 1024 * 1024 {
                    return Err("terrain image cache exceeds 128 MiB".into());
                }
                images.insert(
                    (id, sub),
                    Image {
                        width: image.width,
                        height: image.height,
                        rgba: palette.rgba(&image, true),
                        offset: (composite.offset_x, composite.offset_y),
                    },
                );
            }
        }
        if images.is_empty() {
            return Err("no map terrain images resolved from the supplied mounts".into());
        }
        let report = format!(
            "Terrain images: {}; fallback cells: {missing}; extra-image variants: {extras}",
            images.len()
        );
        eprintln!("{report}");
        Ok(Self {
            overlays,
            cells,
            images,
            report,
        })
    }
    pub fn textures(&self) -> BTreeMap<(i16, u8), TerrainTexture> {
        self.images
            .iter()
            .map(|(&key, image)| {
                let texture = Texture2D::from_rgba8(image.width, image.height, &image.rgba);
                texture.set_filter(FilterMode::Nearest);
                (
                    key,
                    TerrainTexture {
                        texture,
                        offset: image.offset,
                    },
                )
            })
            .collect()
    }
    pub fn texture<'a>(
        &self,
        cell: (i32, i32),
        textures: &'a BTreeMap<(i16, u8), TerrainTexture>,
    ) -> Option<&'a TerrainTexture> {
        textures.get(self.cells.get(&cell)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directory_and_explicit_mount_modes_cannot_be_ambiguous() {
        let mut options = Options::default();
        assert!(options.validate(false).is_ok());
        options.game_dir = Some("game".into());
        assert!(options.validate(true).is_err());
        options.edition = Some(super::super::installation::Edition::Yr);
        assert!(options.validate(true).is_ok());
        assert!(options.validate(false).is_err());
        options.mixes.push("isotemp.mix".into());
        assert!(options.validate(true).is_err());
        options.game_dir = None;
        assert!(options.validate(true).is_err());
        options.edition = None;
        options.ini = Some("temperat.ini".into());
        options.palette = Some("isotem.pal".into());
        assert!(options.validate(true).is_ok());
        options.mixes = vec!["archive.mix".into(); 17];
        assert!(options.validate(true).is_err());
    }
}
