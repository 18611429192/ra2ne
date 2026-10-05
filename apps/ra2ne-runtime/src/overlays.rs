//! Static original overlay SHP frames for the data viewer. No gameplay changes.
use macroquad::prelude::*;
use ra2ne_assets::{
    map::{OVERLAY_SIDE, Ra2Map},
    overlay::{Environment, OverlayCatalog},
    rules::RuleSet,
    sprite::Palette,
    text::TextEncoding,
    vfs::Vfs,
};
use std::collections::{BTreeMap, BTreeSet};
const IMAGE_BUDGET: usize = 128 * 1024 * 1024;
#[derive(Debug)]
struct Image {
    width: u16,
    height: u16,
    offset: (i32, i32),
    rgba: Vec<u8>,
}
pub struct Texture {
    pub texture: Texture2D,
    pub offset: (i32, i32),
}
#[derive(Debug, Default)]
pub struct Statistics {
    pub cells: usize,
    pub images: usize,
    pub unresolved: usize,
    pub empty: usize,
    pub outside: usize,
}
impl Statistics {
    pub fn report(&self) -> String {
        format!(
            "Overlay cells: {}; images: {}; unresolved: {}; empty: {}; outside terrain: {}",
            self.cells, self.images, self.unresolved, self.empty, self.outside
        )
    }
}
pub struct Scene {
    cells: BTreeMap<(i32, i32), (u8, u8)>,
    images: BTreeMap<(u8, u8), Image>,
    pub statistics: Statistics,
}
fn load_rules(files: &Vfs, name: &str, encoding: TextEncoding) -> Result<RuleSet, String> {
    let file = files
        .get(name)?
        .ok_or_else(|| format!("overlay catalogue file missing: {name}"))?;
    let mut rules = RuleSet::default();
    rules.add_layer(file.source, &encoding.decode(file.bytes)?)?;
    Ok(rules)
}
impl Scene {
    pub fn load(
        files: &Vfs,
        map: &Ra2Map,
        map_text: &str,
        edition: super::installation::Edition,
        encoding: TextEncoding,
    ) -> Result<Self, String> {
        let md = edition == super::installation::Edition::Yr;
        let mut rules = load_rules(
            files,
            if md { "rulesmd.ini" } else { "rules.ini" },
            encoding,
        )?;
        rules.add_layer("map rules overrides", map_text)?;
        let art = load_rules(files, if md { "artmd.ini" } else { "art.ini" }, encoding)?;
        let catalogue = OverlayCatalog::from_rules(&rules, &art)?;
        for diagnostic in &catalogue.diagnostics {
            eprintln!("{diagnostic}");
        }
        let environment = Environment::parse(&map.theater)?;
        let terrain_cells = map
            .tiles
            .iter()
            .map(|tile| (i32::from(tile.cell.x), i32::from(tile.cell.y)))
            .collect::<BTreeSet<_>>();
        let mut cells = BTreeMap::new();
        let mut outside = 0;
        let mut references = BTreeMap::<u8, BTreeMap<u8, usize>>::new();
        for (index, &id) in map.overlays.iter().enumerate() {
            if id == 255 {
                continue;
            }
            let cell = ((index % OVERLAY_SIDE) as i32, (index / OVERLAY_SIDE) as i32);
            if !terrain_cells.contains(&cell) {
                outside += 1;
                continue;
            }
            let data = map.overlay_data[index];
            cells.insert(cell, (id, data));
            *references.entry(id).or_default().entry(data).or_default() += 1;
        }
        let mut images = BTreeMap::new();
        let mut palettes = BTreeMap::<&'static str, Palette>::new();
        let mut decoded = 0;
        let mut missing = 0;
        let mut empty = 0;
        for (id, frames) in references {
            let resolved = match catalogue.resolve(id, environment, files) {
                Ok(resolved) => resolved,
                Err(error) => {
                    missing += frames.values().sum::<usize>();
                    eprintln!("overlay {id}: {error}");
                    continue;
                }
            };
            if !palettes.contains_key(resolved.palette) {
                let Some(file) = files.get(resolved.palette)? else {
                    missing += frames.values().sum::<usize>();
                    eprintln!("overlay {id}: palette missing: {}", resolved.palette);
                    continue;
                };
                palettes.insert(resolved.palette, Palette::parse(file.bytes)?);
            }
            let palette = &palettes[resolved.palette];
            for (frame, count) in frames {
                let image = match resolved.shp.frame(usize::from(frame)) {
                    Ok(image) => image,
                    Err(error) => {
                        missing += count;
                        eprintln!("overlay {id}/{frame} ({}): {error}", resolved.filename);
                        continue;
                    }
                };
                if image.pixels.iter().all(|&p| p == 0) {
                    empty += count;
                    continue;
                }
                decoded += image.pixels.len() * 4;
                if decoded > IMAGE_BUDGET {
                    return Err("overlay RGBA cache exceeds 128 MiB".into());
                }
                let offset = resolved.geometry.offset(frame, image.width, image.height);
                images.insert(
                    (id, frame),
                    Image {
                        width: image.width,
                        height: image.height,
                        offset,
                        rgba: palette.rgba(&image, true),
                    },
                );
            }
        }
        let statistics = Statistics {
            cells: cells.len(),
            images: images.len(),
            unresolved: missing,
            empty,
            outside,
        };
        eprintln!("{}", statistics.report());
        Ok(Self {
            cells,
            images,
            statistics,
        })
    }
    pub fn textures(&self) -> BTreeMap<(u8, u8), Texture> {
        self.images
            .iter()
            .map(|(&id, image)| {
                let texture = Texture2D::from_rgba8(image.width, image.height, &image.rgba);
                texture.set_filter(FilterMode::Nearest);
                (
                    id,
                    Texture {
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
        textures: &'a BTreeMap<(u8, u8), Texture>,
    ) -> Option<&'a Texture> {
        textures.get(self.cells.get(&cell)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra2ne_assets::{
        ini::Ini,
        map::{Cell, MapRect, MapTile},
    };
    fn shp() -> Vec<u8> {
        let mut bytes = Vec::new();
        for value in [0_u16, 4, 3, 3] {
            bytes.extend(value.to_le_bytes());
        }
        for offset in [80_u32, 82] {
            for value in [1_u16, 1, 2, 1] {
                bytes.extend(value.to_le_bytes());
            }
            for value in [1_u32, 0, 0, offset] {
                bytes.extend(value.to_le_bytes());
            }
        }
        bytes.extend([0_u8; 24]);
        bytes.extend([7, 0, 8, 0]);
        bytes
    }
    fn map() -> Ra2Map {
        let mut map = Ra2Map {
            ini: Ini::parse("[Map]\nTheater=TEMPERATE").unwrap(),
            name: "Synthetic overlays".into(),
            theater: "TEMPERATE".into(),
            size: MapRect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
            local_size: MapRect {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            },
            tiles: Vec::new(),
            terrain_trailer: None,
            overlays: vec![255; OVERLAY_SIDE * OVERLAY_SIDE],
            overlay_data: vec![0; OVERLAY_SIDE * OVERLAY_SIDE],
            waypoints: BTreeMap::new(),
            unresolved_waypoints: BTreeMap::new(),
            objects: Vec::new(),
            diagnostics: Vec::new(),
        };
        for (x, id, frame) in [(2, 0, 1), (3, 0, 9), (4, 0, 2), (5, 250, 0)] {
            map.tiles.push(MapTile {
                cell: Cell { x, y: 3 },
                tile_index: 0,
                sub_tile: 0,
                height: 4,
                reserved: [0; 2],
                reserved_tail: 0,
            });
            let index = 3 * OVERLAY_SIDE + usize::from(x);
            map.overlays[index] = id;
            map.overlay_data[index] = frame;
        }
        map.overlays[511 * OVERLAY_SIDE + 511] = 0;
        map
    }
    #[test]
    fn map_overrides_frame_identity_palette_and_off_map_cells_are_preserved() {
        let mut palette = vec![0; 768];
        palette[8 * 3..8 * 3 + 3].copy_from_slice(&[2, 3, 4]);
        let mut files = Vfs::default();
        files
            .mount(
                "synthetic installation",
                vec![
                    ("rules.ini".into(), b"[OverlayTypes]\n1=UNUSED".to_vec()),
                    (
                        "rulesmd.ini".into(),
                        b"[OverlayTypes]\n10=ORE\n[ORE]\nImage=MISSING\nTiberium=yes".to_vec(),
                    ),
                    ("artmd.ini".into(), Vec::new()),
                    ("ALIAS.tem".into(), shp()),
                    ("temperat.pal".into(), palette),
                    ("isotem.pal".into(), vec![63; 768]),
                ],
            )
            .unwrap();
        let map = map();
        let scene = Scene::load(
            &files,
            &map,
            "[ORE]\nImage=ALIAS",
            super::super::installation::Edition::Yr,
            TextEncoding::Utf8,
        )
        .unwrap();
        assert_eq!(scene.images.len(), 1);
        let image = &scene.images[&(0, 1)];
        assert_eq!(&image.rgba[5 * 4..6 * 4], &[8, 12, 16, 255]);
        assert_eq!(image.rgba[6 * 4 + 3], 0);
        assert_eq!(image.offset, (-2, -16));
        assert_eq!(
            (
                scene.statistics.unresolved,
                scene.statistics.empty,
                scene.statistics.outside
            ),
            (2, 1, 1)
        );
        assert_eq!(scene.cells.len(), 4);
        assert_eq!(map.overlay_data[3 * OVERLAY_SIDE + 3], 9); // no clamping/rewrite
        assert_eq!(map.overlays[511 * OVERLAY_SIDE + 511], 0);
        assert!(
            Scene::load(
                &files,
                &map,
                "",
                super::super::installation::Edition::Ra2,
                TextEncoding::Utf8
            )
            .is_err()
        ); // no art.ini
    }
}
