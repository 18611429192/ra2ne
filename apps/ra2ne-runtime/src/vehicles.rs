//! Experimental unlit VXL vehicles, preserving map actor IDs.
use macroquad::prelude::*;
use ra2ne_assets::{
    map::{ObjectKind, Ra2Map},
    overlay::Environment,
    remap,
    rules::RuleSet,
    sprite::Palette,
    text::TextEncoding,
    vfs::Vfs,
    voxel::{Hva, Vxl},
    voxel_render::{self, Component},
};
use std::collections::BTreeMap;
struct Image {
    width: u16,
    height: u16,
    rgba: Vec<u8>,
    offset: (i32, i32),
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
            "Voxel actors: {}; images: {}; unresolved: {}",
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
                .ok_or_else(|| format!("vehicle catalogue missing: {name}"))?;
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
                .ok_or("vehicle unit palette missing")?
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
        let mut models = BTreeMap::<String, (Vxl, Hva)>::new();
        let mut bytes = 0;
        for (id, object) in map
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| matches!(o.kind, ObjectKind::Vehicle | ObjectKind::Aircraft))
        {
            scene.cells += 1;
            let result = (|| -> Result<(), String> {
                let image = rules
                    .get(&object.type_id, "Image")
                    .map_or(object.type_id.as_str(), |v| v.entry.value.as_str());
                let name = art
                    .get(image, "Image")
                    .map_or(image, |v| v.entry.value.as_str())
                    .to_ascii_uppercase();
                let height = *heights
                    .get(&(object.cell.x, object.cell.y))
                    .ok_or("vehicle outside decoded terrain")?;
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
                let key = (cache_name, usize::from(object.facing));
                let mut parts = Vec::new();
                for suffix in ["", "tur", "barl"] {
                    let part = format!("{name}{suffix}");
                    if !models.contains_key(&part) {
                        if let Some(file) = files.get(&format!("{part}.vxl"))? {
                            let model = Vxl::parse(file.bytes)?;
                            let pose = Hva::parse(
                                files
                                    .get(&format!("{part}.hva"))?
                                    .ok_or("vehicle HVA missing")?
                                    .bytes,
                            )?;
                            pose.bind(&model)?;
                            models.insert(part.clone(), (model, pose));
                        } else if suffix.is_empty() {
                            return Err("vehicle VXL missing (SHP vehicles pending)".into());
                        }
                    }
                    if models.contains_key(&part) {
                        parts.push(part);
                    }
                }
                if !scene.images.contains_key(&key) {
                    let components = parts
                        .iter()
                        .map(|p| {
                            let (model, pose) = &models[p];
                            Component {
                                model,
                                pose,
                                frame: 0,
                                yaw: f32::from(object.facing) * std::f32::consts::TAU / 256.0,
                            }
                        })
                        .collect::<Vec<_>>();
                    let raster = voxel_render::render(&components)?;
                    bytes += raster.image.pixels.len() * 4;
                    if bytes > 128 * 1024 * 1024 {
                        return Err("vehicle RGBA budget exceeded".into());
                    }
                    scene.images.insert(
                        key.clone(),
                        Image {
                            width: raster.image.width,
                            height: raster.image.height,
                            rgba: owner_color
                                .map(|rgb| remap::palette(&palette, models[&name].0.remap, rgb))
                                .transpose()?
                                .as_ref()
                                .unwrap_or(&palette)
                                .rgba(&raster.image, true),
                            offset: raster.offset,
                        },
                    );
                }
                let offset = scene.images[&key].offset;
                scene.actors.insert(
                    id,
                    Placement {
                        key,
                        offset: (offset.0, offset.1 - i32::from(height) * 15),
                    },
                );
                Ok(())
            })();
            if let Err(error) = result {
                scene.unresolved += 1;
                eprintln!(
                    "vehicle {} at ({},{}), line {}: {error}",
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
