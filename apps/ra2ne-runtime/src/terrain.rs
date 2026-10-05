//! Explicit resource mounts for the map viewer; no automatic game search order.
use macroquad::prelude::*;
use ra2ne_assets::{
    map::Ra2Map,
    mix::{FilenameHash, MAX_ARCHIVE_BYTES, MixArchive},
    sprite::Palette,
    theater::Theater,
    vfs::Vfs,
};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Options {
    pub ini: Option<String>,
    pub palette: Option<String>,
    pub mixes: Vec<String>,
}
impl Options {
    pub fn enabled(&self) -> bool {
        self.ini.is_some() || self.palette.is_some() || !self.mixes.is_empty()
    }
    pub fn validate(&self, map: bool) -> Result<(), String> {
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
pub struct Terrain {
    cells: BTreeMap<(i32, i32), (i16, u8)>,
    images: BTreeMap<(i16, u8), (u16, u16, Vec<u8>)>,
    pub report: String,
}
impl Terrain {
    pub fn load(
        options: &Options,
        map: &Ra2Map,
        encoding: ra2ne_assets::text::TextEncoding,
    ) -> Result<Self, String> {
        options.validate(true)?;
        let extension = match map.theater.to_ascii_uppercase().as_str() {
            "TEMPERATE" => "tem",
            "SNOW" => "sno",
            "URBAN" => "urb",
            "DESERT" => "des",
            "NEWURBAN" => "ubn",
            "LUNAR" => "lun",
            _ => return Err("unsupported map theater".into()),
        };
        let ini_bytes = super::read(options.ini.as_ref().unwrap(), 16 * 1024 * 1024)?;
        let ini = encoding.decode(&ini_bytes)?;
        let theater = Theater::parse(&ini, extension)?;
        let palette = Palette::parse(&super::read(options.palette.as_ref().unwrap(), 768)?)?;
        let mut files = Vfs::default();
        let mut mounted = 0;
        for path in &options.mixes {
            let bytes = super::read(path, MAX_ARCHIVE_BYTES)?;
            mounted += bytes.len();
            if mounted > MAX_ARCHIVE_BYTES {
                return Err("terrain mount total exceeds archive budget".into());
            }
            files.mount_mix(path, MixArchive::parse(bytes.into())?, FilenameHash::Ra2)?;
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
            let tmp = match theater.load(id, &files) {
                Ok(tmp) => tmp,
                Err(error) => {
                    missing += subtiles.values().sum::<usize>();
                    eprintln!("terrain tile {id}: {error}");
                    continue;
                }
            };
            if tmp.cell_width != 60 || tmp.cell_height != 30 {
                return Err("viewer requires 60x30 TMP cells".into());
            }
            for (sub, cell_count) in subtiles {
                let image = match tmp.diamond(usize::from(sub)) {
                    Ok(image) => image,
                    Err(error) => {
                        missing += cell_count;
                        eprintln!("terrain tile {id}/{sub}: {error}");
                        continue;
                    }
                };
                if tmp.extra(usize::from(sub)).is_some() {
                    extras += 1;
                }
                decoded += image.pixels.len() * 4;
                if decoded > 128 * 1024 * 1024 {
                    return Err("terrain image cache exceeds 128 MiB".into());
                }
                images.insert(
                    (id, sub),
                    (image.width, image.height, palette.rgba(&image, true)),
                );
            }
        }
        if images.is_empty() {
            return Err("no map terrain images resolved from the supplied mounts".into());
        }
        let report = format!(
            "Terrain base images: {}; fallback cells: {missing}; extra-image variants pending: {extras}",
            images.len()
        );
        eprintln!("{report}");
        Ok(Self {
            cells,
            images,
            report,
        })
    }
    pub fn textures(&self) -> BTreeMap<(i16, u8), Texture2D> {
        self.images
            .iter()
            .map(|(&key, (w, h, rgba))| {
                let texture = Texture2D::from_rgba8(*w, *h, rgba);
                texture.set_filter(FilterMode::Nearest);
                (key, texture)
            })
            .collect()
    }
    pub fn texture<'a>(
        &self,
        cell: (i32, i32),
        textures: &'a BTreeMap<(i16, u8), Texture2D>,
    ) -> Option<&'a Texture2D> {
        textures.get(self.cells.get(&cell)?)
    }
}
