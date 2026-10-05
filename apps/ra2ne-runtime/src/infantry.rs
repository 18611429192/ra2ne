//! Static original infantry, indexed by the preserved map actor order.
use macroquad::prelude::*;
use ra2ne_assets::{
    infantry::{StandingPose, subcell_offset},
    map::{ObjectKind, Ra2Map},
    overlay::Environment,
    remap,
    rules::RuleSet,
    sprite::{Palette, Shp},
    text::TextEncoding,
    vfs::Vfs,
};
use std::{collections::BTreeMap, sync::Arc};
struct Image {
    width: u16,
    height: u16,
    rgba: Vec<u8>,
}
type ImageKey = (String, usize);
struct Placement {
    key: ImageKey,
    offset: (i32, i32),
}
pub struct Scene {
    actors: BTreeMap<usize, Placement>,
    images: BTreeMap<ImageKey, Image>,
    pub cells: usize,
    pub unresolved: usize,
}
impl Scene {
    pub fn report(&self) -> String {
        format!(
            "Infantry actors: {}; images: {}; unresolved: {}",
            self.cells,
            self.images.len(),
            self.unresolved
        )
    }
    pub fn load(
        files: &Vfs,
        map: &Ra2Map,
        map_text: &str,
        edition: super::installation::Edition,
        encoding: TextEncoding,
    ) -> Result<Self, String> {
        let md = edition == super::installation::Edition::Yr;
        let mut rules = RuleSet::default();
        let mut art = RuleSet::default();
        for (set, name) in [
            (&mut rules, if md { "rulesmd.ini" } else { "rules.ini" }),
            (&mut art, if md { "artmd.ini" } else { "art.ini" }),
        ] {
            let file = files
                .get(name)?
                .ok_or_else(|| format!("infantry catalogue missing: {name}"))?;
            set.add_layer(file.source, &encoding.decode(file.bytes)?)?;
        }
        rules.add_layer("map rules overrides", map_text)?;
        let environment = Environment::parse(&map.theater)?;
        let palette_name = match environment {
            Environment::Temperate => "unittem.pal",
            Environment::Snow => "unitsno.pal",
            Environment::Urban => "uniturb.pal",
            Environment::NewUrban => "unitubn.pal",
            Environment::Desert => "unitdes.pal",
            Environment::Lunar => "unitlun.pal",
        };
        let palette = Palette::parse(
            files
                .get(palette_name)?
                .ok_or("infantry unit palette missing")?
                .bytes,
        )?;
        let heights = map
            .tiles
            .iter()
            .map(|t| ((t.cell.x, t.cell.y), t.height))
            .collect::<BTreeMap<_, _>>();
        let mut scene = Self {
            actors: BTreeMap::new(),
            images: BTreeMap::new(),
            cells: 0,
            unresolved: 0,
        };
        let mut sprites = BTreeMap::<String, Shp>::new();
        let mut bytes = 0;
        for (id, object) in map
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.kind == ObjectKind::Infantry)
        {
            scene.cells += 1;
            let result = (|| -> Result<(), String> {
                let pose = StandingPose::from_rules(&object.type_id, &rules, &art)?;
                let height = *heights
                    .get(&(object.cell.x, object.cell.y))
                    .ok_or("infantry cell outside decoded terrain")?;
                let frame = pose.frame(object.facing);
                let name = pose.image.to_ascii_uppercase();
                if !sprites.contains_key(&name) {
                    let file = files
                        .get(&format!("{name}.shp"))?
                        .ok_or("infantry SHP missing")?;
                    sprites.insert(name.clone(), Shp::parse(Arc::from(file.bytes))?);
                }
                let shp = &sprites[&name];
                let remap_enabled = remap::enabled(&rules, &art, &object.type_id);
                let owner_color = match if remap_enabled {
                    remap::house_color(&rules, &object.house)
                } else {
                    Ok(None)
                } {
                    Ok(value) => value,
                    Err(error) => {
                        eprintln!("owner {}: {error}; preserving source palette", object.house);
                        None
                    }
                };
                let cache_name = owner_color.map_or_else(
                    || name.clone(),
                    |rgb| format!("{name}@{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]),
                );
                let key = (cache_name, frame);
                if !scene.images.contains_key(&key) {
                    let image = shp.frame(frame)?;
                    bytes += image.pixels.len() * 4;
                    if bytes > 128 * 1024 * 1024 {
                        return Err("infantry RGBA cache exceeds 128 MiB".into());
                    }
                    scene.images.insert(
                        key.clone(),
                        Image {
                            width: image.width,
                            height: image.height,
                            rgba: owner_color
                                .map(|rgb| remap::palette(&palette, [16, 31], rgb))
                                .transpose()?
                                .as_ref()
                                .unwrap_or(&palette)
                                .rgba(&image, true),
                        },
                    );
                }
                let sub = subcell_offset(object.sub_cell.ok_or("infantry subcell missing")?)?;
                scene.actors.insert(
                    id,
                    Placement {
                        key,
                        offset: (
                            sub.0 - i32::from(shp.width) / 2,
                            sub.1 - i32::from(shp.height) / 2 - i32::from(height) * 15,
                        ),
                    },
                );
                Ok(())
            })();
            if let Err(error) = result {
                scene.unresolved += 1;
                eprintln!(
                    "infantry {} at ({},{}), line {}: {error}",
                    object.type_id, object.cell.x, object.cell.y, object.line
                );
            }
        }
        eprintln!("{}", scene.report());
        Ok(scene)
    }
    pub fn textures(&self) -> BTreeMap<(String, usize), Texture2D> {
        self.images
            .iter()
            .map(|(key, image)| {
                let t = Texture2D::from_rgba8(image.width, image.height, &image.rgba);
                t.set_filter(FilterMode::Nearest);
                (key.clone(), t)
            })
            .collect()
    }
    pub fn texture<'a>(
        &self,
        id: usize,
        textures: &'a BTreeMap<(String, usize), Texture2D>,
    ) -> Option<(&'a Texture2D, (i32, i32))> {
        let placement = self.actors.get(&id)?;
        Some((textures.get(&placement.key)?, placement.offset))
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_ids_palette_subcell_height_and_bad_frames_are_preserved() {
        let map=Ra2Map::parse("[Map]\nTheater=TEMPERATE\nSize=0,0,10,10\nLocalSize=0,0,10,10\n[IsoMapPack5]\n1=DwALABwCAAMAAAAAAAAEABEAAA==\n[Units]\n0=Owner,TANK,256,2,3,0,Guard,0\n[Infantry]\n0=Owner,E1,256,2,3,1,Guard,224,0\n1=Owner,E1,256,2,3,3,Guard,0,0").unwrap();
        let mut shp = Vec::new();
        for n in [0_u16, 4, 3, 1, 1, 1, 2, 1] {
            shp.extend(n.to_le_bytes());
        }
        for n in [1_u32, 0, 0, 32] {
            shp.extend(n.to_le_bytes());
        }
        shp.extend([7, 0]);
        let mut palette = vec![0; 768];
        palette[21..24].copy_from_slice(&[1, 2, 3]);
        let mut files = Vfs::default();
        files
            .mount(
                "synthetic",
                vec![
                    (
                        "rulesmd.ini".into(),
                        b"[InfantryTypes]\n1=E1\n[E1]\nImage=GI".to_vec(),
                    ),
                    (
                        "artmd.ini".into(),
                        b"[GI]\nImage=ALIAS\nSequence=Stand\n[Stand]\nReady=0,1,1".to_vec(),
                    ),
                    ("ALIAS.shp".into(), shp),
                    ("unittem.pal".into(), palette),
                ],
            )
            .unwrap();
        let scene = Scene::load(
            &files,
            &map,
            "",
            super::super::installation::Edition::Yr,
            TextEncoding::Utf8,
        )
        .unwrap();
        assert_eq!(
            (
                scene.cells,
                scene.actors.len(),
                scene.images.len(),
                scene.unresolved
            ),
            (2, 1, 1, 1)
        );
        assert_eq!(scene.actors[&1].offset, (13, -61));
        assert_eq!(
            &scene.images[&("ALIAS".into(), 0)].rgba[20..24],
            &[4, 8, 12, 255]
        );
        assert_eq!(map.objects[2].facing, 0);
    }
}
