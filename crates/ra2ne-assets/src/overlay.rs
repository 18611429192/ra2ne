//! Original map overlay catalogue and SHP lookup. This is graphics metadata,
//! not wall connectivity, ore growth, bridge navigation or gameplay.
use crate::{rules::RuleSet, sprite::Shp, vfs::Vfs};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Environment {
    Temperate,
    Snow,
    Urban,
    NewUrban,
    Desert,
    Lunar,
}
impl Environment {
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "TEMPERATE" => Ok(Self::Temperate),
            "SNOW" => Ok(Self::Snow),
            "URBAN" => Ok(Self::Urban),
            "NEWURBAN" => Ok(Self::NewUrban),
            "DESERT" => Ok(Self::Desert),
            "LUNAR" => Ok(Self::Lunar),
            _ => Err("unsupported overlay theater"),
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::Temperate => "tem",
            Self::Snow => "sno",
            Self::Urban => "urb",
            Self::NewUrban => "ubn",
            Self::Desert => "des",
            Self::Lunar => "lun",
        }
    }
    pub fn letter(self) -> char {
        match self {
            Self::Temperate => 'T',
            Self::Snow => 'A',
            Self::Urban => 'U',
            Self::NewUrban => 'N',
            Self::Desert => 'D',
            Self::Lunar => 'L',
        }
    }
    fn iso_palette(self) -> &'static str {
        match self {
            Self::Temperate => "isotem.pal",
            Self::Snow => "isosno.pal",
            Self::Urban => "isourb.pal",
            Self::NewUrban => "isoubn.pal",
            Self::Desert => "isodes.pal",
            Self::Lunar => "isolun.pal",
        }
    }
    fn unit_palette(self) -> &'static str {
        match self {
            Self::Temperate => "unittem.pal",
            Self::Snow => "unitsno.pal",
            Self::Urban => "uniturb.pal",
            Self::NewUrban => "unitubn.pal",
            Self::Desert => "unitdes.pal",
            Self::Lunar => "unitlun.pal",
        }
    }
}
const ENVIRONMENTS: [Environment; 6] = [
    Environment::Temperate,
    Environment::Snow,
    Environment::Urban,
    Environment::NewUrban,
    Environment::Lunar,
    Environment::Desert,
];
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Geometry {
    Normal,
    HighBridge,
    GroundAligned,
    Veinhole,
}
impl Geometry {
    /// Pixel offset from the cell's projected center. SHP frame() retains the
    /// complete canvas, including the stored frame X/Y offsets.
    pub fn offset(self, data: u8, width: u16, height: u16) -> (i32, i32) {
        let x = -i32::from(width) / 2;
        let y = -i32::from(height) / 2 - 15;
        match self {
            Self::Normal => (x, y),
            Self::HighBridge => (x - 1, y - if (9..=17).contains(&data) { 15 } else { 0 }),
            Self::GroundAligned => (x, y + 15),
            Self::Veinhole => (x, y - 45),
        }
    }
}
#[derive(Debug)]
pub struct OverlayType {
    pub type_id: String,
    pub image: String,
    pub new_theater: bool,
    pub resource: bool,
    pub geometry: Geometry,
}
#[derive(Debug)]
pub struct OverlayCatalog {
    /// Source order determines the byte ID; numeric INI keys are labels.
    /// Invalid definitions keep an empty slot so subsequent IDs never shift.
    pub types: Vec<Option<OverlayType>>,
    pub diagnostics: Vec<String>,
}
#[derive(Debug)]
pub struct ResolvedOverlay {
    pub shp: Shp,
    pub filename: String,
    pub source: String,
    pub palette: &'static str,
    pub geometry: Geometry,
}
fn safe_prefix(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn flag(rules: &RuleSet, section: &str, key: &str) -> Result<bool, &'static str> {
    rules
        .get(section, key)
        .map_or(Ok(false), |v| v.entry.boolean())
}
impl OverlayCatalog {
    pub fn from_rules(rules: &RuleSet, art: &RuleSet) -> Result<Self, &'static str> {
        let entries = rules.section("OverlayTypes");
        if entries.is_empty() || entries.len() > 255 {
            return Err("overlay registry must contain 1..255 entries (255 means no overlay)");
        }
        let mut types = Vec::with_capacity(entries.len());
        let mut diagnostics = Vec::new();
        for (id, entry) in entries.iter().enumerate() {
            let definition = (|| {
                let type_id = &entry.entry.value;
                if !safe_prefix(type_id) {
                    return Err("invalid overlay type ID");
                }
                let art_id = rules
                    .get(type_id, "Image")
                    .map_or(type_id.as_str(), |v| v.entry.value.as_str());
                let image = art
                    .get(art_id, "Image")
                    .map_or(art_id, |v| v.entry.value.as_str());
                if !safe_prefix(art_id) || !safe_prefix(image) {
                    return Err("invalid overlay image prefix");
                }
                if art.get(art_id, "Palette").is_some() {
                    return Err("custom overlay Palette is not implemented");
                }
                let resource = flag(rules, type_id, "Tiberium")?
                    || flag(rules, type_id, "IsVeins")?
                    || flag(rules, type_id, "IsVeinholeMonster")?;
                let upper = type_id.to_ascii_uppercase();
                let geometry = if flag(rules, type_id, "IsVeinholeMonster")? {
                    Geometry::Veinhole
                } else if ["BRIDGE1", "BRIDGE2", "BRIDGEB1", "BRIDGEB2"].contains(&upper.as_str()) {
                    Geometry::HighBridge
                } else if ["TRACKS", "TRACKTUNNEL", "RAILBRDG", "LOBRDG", "LOBRDB"]
                    .iter()
                    .any(|p| {
                        upper.strip_prefix(p).is_some_and(|tail| {
                            !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit())
                        })
                    })
                {
                    Geometry::GroundAligned
                } else {
                    Geometry::Normal
                };
                Ok(OverlayType {
                    type_id: type_id.to_owned(),
                    image: image.to_owned(),
                    new_theater: flag(art, art_id, "NewTheater")?,
                    resource,
                    geometry,
                })
            })();
            match definition {
                Ok(definition) => types.push(Some(definition)),
                Err(error) => {
                    types.push(None);
                    diagnostics.push(format!(
                        "overlay {id} ({}:{}): {error}",
                        entry.source, entry.entry.line
                    ));
                }
            }
        }
        Ok(Self { types, diagnostics })
    }
    pub fn resolve(
        &self,
        id: u8,
        environment: Environment,
        files: &Vfs,
    ) -> Result<ResolvedOverlay, &'static str> {
        let definition = self
            .types
            .get(usize::from(id))
            .and_then(Option::as_ref)
            .ok_or("overlay ID missing or unsupported")?;
        let mut candidates = vec![(
            format!("{}.{}", definition.image, environment.extension()),
            environment.iso_palette(),
        )];
        if definition.new_theater {
            if definition.image.len() < 2 {
                return Err("NewTheater image prefix is too short");
            }
            // G is the generic NewTheater variant, colored with the current
            // environment's unit palette rather than a separate generic PAL.
            for (letter, palette) in
                std::iter::once((environment.letter(), environment.unit_palette()))
                    .chain(std::iter::once(('G', environment.unit_palette())))
                    .chain(ENVIRONMENTS.map(|theater| (theater.letter(), theater.unit_palette())))
            {
                let mut prefix = definition.image.clone();
                prefix.replace_range(1..2, &letter.to_string());
                candidates.push((format!("{prefix}.shp"), palette));
            }
        }
        candidates.push((
            format!("{}.shp", definition.image),
            environment.unit_palette(),
        ));
        for theater in ENVIRONMENTS {
            candidates.push((
                format!("{}.{}", definition.image, theater.extension()),
                theater.iso_palette(),
            ));
        }
        for (filename, palette) in candidates {
            if let Some(file) = files.get(&filename)? {
                return Ok(ResolvedOverlay {
                    shp: Shp::parse(Arc::from(file.bytes))?,
                    filename,
                    source: file.source.to_owned(),
                    palette: if definition.resource {
                        "temperat.pal"
                    } else {
                        palette
                    },
                    geometry: definition.geometry,
                });
            }
        }
        Err("overlay SHP resource missing")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn layer(text: &str) -> RuleSet {
        let mut rules = RuleSet::default();
        rules.add_layer("synthetic", text).unwrap();
        rules
    }
    fn shp() -> Vec<u8> {
        let mut bytes = Vec::new();
        for value in [0_u16, 4, 3, 1] {
            bytes.extend(value.to_le_bytes());
        }
        for value in [1_u16, 1, 2, 1] {
            bytes.extend(value.to_le_bytes());
        }
        for value in [1_u32, 0, 0, 32] {
            bytes.extend(value.to_le_bytes());
        }
        bytes.extend([7, 0]);
        bytes
    }
    #[test]
    fn registry_labels_duplicates_and_layer_overrides_do_not_shift_ids() {
        let mut rules = layer("[OverlayTypes]\n10=WALL\n2=BAD\n7=WALL\n[BAD]\nImage=../escape");
        rules
            .add_layer(
                "map",
                "[OverlayTypes]\n10=ORE\n99=BRIDGE1\n[ORE]\nTiberium=yes\n[BRIDGE1]\nImage=BRIDGE",
            )
            .unwrap();
        let catalogue = OverlayCatalog::from_rules(&rules, &RuleSet::default()).unwrap();
        assert_eq!(catalogue.types.len(), 4);
        assert_eq!(catalogue.types[0].as_ref().unwrap().type_id, "ORE");
        assert!(catalogue.types[0].as_ref().unwrap().resource);
        assert!(catalogue.types[1].is_none());
        assert_eq!(catalogue.types[2].as_ref().unwrap().type_id, "WALL");
        assert_eq!(
            catalogue.types[3].as_ref().unwrap().geometry,
            Geometry::HighBridge
        );
        assert_eq!(catalogue.diagnostics.len(), 1);
        assert!(OverlayCatalog::from_rules(&RuleSet::default(), &RuleSet::default()).is_err());
    }
    #[test]
    fn aliases_and_theater_fallback_preserve_palette_and_reject_corrupt_native_file() {
        let rules =
            layer("[OverlayTypes]\n1=ORE\n2=WALL\n[ORE]\nTiberium=yes\n[WALL]\nImage=ALIAS");
        let art = layer("[ALIAS]\nImage=GASAND\nNewTheater=yes");
        let catalogue = OverlayCatalog::from_rules(&rules, &art).unwrap();
        let mut files = Vfs::default();
        files
            .mount(
                "stock",
                vec![("ORE.tem".into(), shp()), ("GTSAND.shp".into(), shp())],
            )
            .unwrap();
        let ore = catalogue.resolve(0, Environment::Snow, &files).unwrap();
        assert_eq!(
            (ore.filename.as_str(), ore.palette),
            ("ORE.tem", "temperat.pal")
        );
        let wall = catalogue.resolve(1, Environment::Snow, &files).unwrap();
        assert_eq!(
            (wall.filename.as_str(), wall.palette),
            ("GTSAND.shp", "unittem.pal")
        );
        assert_eq!(wall.shp.frame(0).unwrap().pixels[5], 7);
        files
            .mount(
                "bad override",
                vec![("GASAND.sno".into(), b"corrupt".to_vec())],
            )
            .unwrap();
        assert!(catalogue.resolve(1, Environment::Snow, &files).is_err());
        assert!(catalogue.resolve(255, Environment::Snow, &files).is_err());
    }
    #[test]
    fn generic_new_theater_image_uses_current_environment_palette() {
        let catalogue = OverlayCatalog::from_rules(
            &layer("[OverlayTypes]\n1=GAFWLL"),
            &layer("[GAFWLL]\nNewTheater=yes"),
        )
        .unwrap();
        let mut files = Vfs::default();
        files
            .mount(
                "generic",
                vec![("GGFWLL.shp".into(), shp()), ("GTFWLL.shp".into(), shp())],
            )
            .unwrap();
        let resolved = catalogue.resolve(0, Environment::Lunar, &files).unwrap();
        assert_eq!(
            (resolved.filename.as_str(), resolved.palette),
            ("GGFWLL.shp", "unitlun.pal")
        );
    }
    #[test]
    fn bridge_track_and_canvas_offsets_are_distinct() {
        let catalogue = OverlayCatalog::from_rules(
            &layer("[OverlayTypes]\n1=LOBRDG01\n2=LOBRDB01"),
            &RuleSet::default(),
        )
        .unwrap();
        for definition in catalogue.types.iter().flatten() {
            assert_eq!(definition.geometry, Geometry::GroundAligned);
        }
        assert_eq!(Geometry::Normal.offset(0, 60, 60), (-30, -45));
        assert_eq!(Geometry::HighBridge.offset(9, 60, 60), (-31, -60));
        assert_eq!(Geometry::HighBridge.offset(18, 60, 60), (-31, -45));
        assert_eq!(Geometry::GroundAligned.offset(0, 60, 60), (-30, -30));
        assert_eq!(Geometry::Veinhole.offset(0, 60, 60), (-30, -90));
    }
}
