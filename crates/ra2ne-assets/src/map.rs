//! RA2/YR map frontend. Coordinates and all unconsumed INI fields are preserved.
//! Loaded records are not a claim that triggers or gameplay are implemented.
use crate::{
    ini::{Entry, Ini},
    pack::{PackCodec, decode_section},
};
use std::collections::{BTreeMap, BTreeSet};

pub const OVERLAY_SIDE: usize = 512;
const MAX_CELLS: usize = OVERLAY_SIDE * OVERLAY_SIDE;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MapRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cell {
    pub x: u16,
    pub y: u16,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MapTile {
    pub cell: Cell,
    pub tile_index: i16,
    pub reserved: [u8; 2],
    pub sub_tile: u8,
    pub height: u8,
    pub reserved_tail: u8,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectKind {
    Vehicle,
    Infantry,
    Aircraft,
    Building,
}
#[derive(Clone, Debug)]
pub struct MapObject {
    pub kind: ObjectKind,
    pub house: String,
    pub type_id: String,
    /// Original health fraction, in 0..=256 units.
    pub health: u16,
    pub cell: Cell,
    pub facing: u8,
    pub mission: Option<String>,
    pub sub_cell: Option<u8>,
    pub fields: Vec<String>,
    pub line: usize,
}
#[derive(Clone, Debug)]
pub struct MapDiagnostic {
    pub section: String,
    pub line: usize,
    pub message: String,
}
#[derive(Debug)]
pub struct Ra2Map {
    pub ini: Ini,
    pub name: String,
    pub theater: String,
    pub size: MapRect,
    pub local_size: MapRect,
    pub tiles: Vec<MapTile>,
    pub overlays: Vec<u8>,
    pub overlay_data: Vec<u8>,
    pub waypoints: BTreeMap<u32, Cell>,
    pub objects: Vec<MapObject>,
    pub diagnostics: Vec<MapDiagnostic>,
}
fn rect(entry: &Entry) -> Result<MapRect, String> {
    let values = entry
        .value
        .split(',')
        .map(|v| v.trim().parse::<u16>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| format!("line {}: invalid map rectangle", entry.line))?;
    if values.len() != 4
        || values[2] == 0
        || values[3] == 0
        || values.iter().any(|&v| usize::from(v) > OVERLAY_SIDE)
    {
        return Err(format!(
            "line {}: map rectangle exceeds coordinate limits",
            entry.line
        ));
    }
    Ok(MapRect {
        x: values[0],
        y: values[1],
        width: values[2],
        height: values[3],
    })
}
impl Ra2Map {
    pub fn parse(text: &str) -> Result<Self, String> {
        let ini = Ini::parse(text)?;
        let size = rect(ini.get("Map", "Size").ok_or("missing [Map] Size")?)?;
        let local_size = match ini.get("Map", "LocalSize") {
            Some(e) => rect(e)?,
            None => size,
        };
        if u32::from(local_size.x) + u32::from(local_size.width)
            > u32::from(size.x) + u32::from(size.width)
            || u32::from(local_size.y) + u32::from(local_size.height)
                > u32::from(size.y) + u32::from(size.height)
            || local_size.x < size.x
            || local_size.y < size.y
        {
            return Err("LocalSize is outside Size".into());
        }
        let theater = ini
            .get("Map", "Theater")
            .ok_or("missing [Map] Theater")?
            .value
            .to_ascii_uppercase();
        let name = ini
            .get("Basic", "Name")
            .map_or("Untitled", |e| e.value.as_str())
            .to_owned();
        let mut diagnostics: Vec<_> = ini
            .diagnostics
            .iter()
            .map(|d| MapDiagnostic {
                section: "INI".into(),
                line: d.line,
                message: d.message.into(),
            })
            .collect();
        if ini.get("Map", "LocalSize").is_none() {
            diagnostics.push(MapDiagnostic {
                section: "Map".into(),
                line: 0,
                message: "missing LocalSize; using Size".into(),
            });
        }
        if !["TEMPERATE", "SNOW", "URBAN", "DESERT", "NEWURBAN", "LUNAR"]
            .contains(&theater.as_str())
        {
            diagnostics.push(MapDiagnostic {
                section: "Map".into(),
                line: ini.get("Map", "Theater").unwrap().line,
                message: format!("unsupported theater: {theater}"),
            });
        }
        let raw = decode_section(&ini, "IsoMapPack5", PackCodec::Lzo, MAX_CELLS * 11)?
            .ok_or("missing IsoMapPack5 terrain data")?;
        if raw.is_empty() || raw.len() % 11 != 0 {
            return Err("IsoMapPack5 is not a nonempty array of 11-byte cells".into());
        }
        let mut tiles = Vec::with_capacity(raw.len() / 11);
        let mut seen = BTreeSet::new();
        for record in raw.as_chunks::<11>().0 {
            let x = u16::from_le_bytes(record[..2].try_into().unwrap());
            let y = u16::from_le_bytes(record[2..4].try_into().unwrap());
            if usize::from(x) >= OVERLAY_SIDE || usize::from(y) >= OVERLAY_SIDE {
                return Err("terrain cell outside 512x512 coordinate space".into());
            }
            if !seen.insert((x, y)) {
                diagnostics.push(MapDiagnostic {
                    section: "IsoMapPack5".into(),
                    line: 0,
                    message: format!("duplicate terrain cell {x},{y}; all records preserved"),
                });
            }
            tiles.push(MapTile {
                cell: Cell { x, y },
                tile_index: i16::from_le_bytes(record[4..6].try_into().unwrap()),
                reserved: record[6..8].try_into().unwrap(),
                sub_tile: record[8],
                height: record[9],
                reserved_tail: record[10],
            });
        }
        let mut overlays =
            decode_section(&ini, "OverlayPack", PackCodec::Lcw, MAX_CELLS)?.unwrap_or_default();
        overlays.resize(MAX_CELLS, 0xff);
        let mut overlay_data =
            decode_section(&ini, "OverlayDataPack", PackCodec::Lcw, MAX_CELLS)?.unwrap_or_default();
        overlay_data.resize(MAX_CELLS, 0);
        let mut waypoints = BTreeMap::new();
        for entry in ini.section_entries("Waypoints") {
            let id: u32 = entry
                .key
                .parse()
                .map_err(|_| format!("line {}: invalid waypoint ID", entry.line))?;
            let encoded: u32 = entry
                .value
                .parse()
                .map_err(|_| format!("line {}: invalid waypoint cell", entry.line))?;
            let x = encoded % 1000;
            let y = encoded / 1000;
            if x >= OVERLAY_SIDE as u32 || y >= OVERLAY_SIDE as u32 {
                return Err(format!(
                    "line {}: waypoint outside map coordinate space",
                    entry.line
                ));
            }
            if waypoints
                .insert(
                    id,
                    Cell {
                        x: x as u16,
                        y: y as u16,
                    },
                )
                .is_some()
            {
                return Err(format!("line {}: duplicate waypoint ID", entry.line));
            }
        }
        let mut objects = Vec::new();
        for (section, kind) in [
            ("Units", ObjectKind::Vehicle),
            ("Infantry", ObjectKind::Infantry),
            ("Aircraft", ObjectKind::Aircraft),
            ("Structures", ObjectKind::Building),
        ] {
            let mut keys = BTreeSet::new();
            for entry in ini.section_entries(section) {
                if !keys.insert(entry.key.to_ascii_lowercase()) {
                    return Err(format!("line {}: duplicate placed object ID", entry.line));
                }
                objects.push(parse_object(entry, kind)?);
            }
        }
        // Preserve complete source sections and make incomplete runtime features
        // visible before the future game loader decides whether to reject them.
        for section in [
            "Triggers",
            "Events",
            "Actions",
            "TeamTypes",
            "TaskForces",
            "ScriptTypes",
            "Tags",
            "CellTags",
            "AITriggerTypes",
            "Terrain",
            "Smudge",
            "Tubes",
            "Lighting",
            "Countries",
            "Houses",
            "PreviewPack",
        ] {
            let count = ini.section_entries(section).count();
            if count > 0 {
                diagnostics.push(MapDiagnostic {
                    section: section.into(),
                    line: ini.section_entries(section).next().unwrap().line,
                    message: format!("{count} entries preserved; runtime semantics pending"),
                });
            }
        }
        Ok(Self {
            ini,
            name,
            theater,
            size,
            local_size,
            tiles,
            overlays,
            overlay_data,
            waypoints,
            objects,
            diagnostics,
        })
    }
    pub fn overlay_at(&self, cell: Cell) -> Option<(u8, u8)> {
        if usize::from(cell.x) >= OVERLAY_SIDE || usize::from(cell.y) >= OVERLAY_SIDE {
            return None;
        }
        let index = usize::from(cell.y) * OVERLAY_SIDE + usize::from(cell.x);
        Some((self.overlays[index], self.overlay_data[index]))
    }
}
fn parse_object(entry: &Entry, kind: ObjectKind) -> Result<MapObject, String> {
    let fields: Vec<_> = entry
        .value
        .split(',')
        .map(|s| s.trim().to_owned())
        .collect();
    let infantry = kind == ObjectKind::Infantry;
    let min_fields = if infantry {
        9
    } else if kind == ObjectKind::Building {
        7
    } else {
        8
    };
    if fields.len() < min_fields || fields[0].is_empty() || fields[1].is_empty() {
        return Err(format!("line {}: incomplete placed object", entry.line));
    }
    let number = |index: usize| {
        fields[index]
            .parse::<u16>()
            .map_err(|_| format!("line {}: invalid object field {index}", entry.line))
    };
    let health = number(2)?;
    let x = number(3)?;
    let y = number(4)?;
    let facing = number(if infantry { 7 } else { 5 })?;
    if health > 256
        || usize::from(x) >= OVERLAY_SIDE
        || usize::from(y) >= OVERLAY_SIDE
        || facing > 255
    {
        return Err(format!(
            "line {}: placed object value outside range",
            entry.line
        ));
    }
    let sub_cell = if infantry {
        let n = number(5)?;
        if n > 5 {
            return Err(format!("line {}: invalid infantry subcell", entry.line));
        }
        Some(n as u8)
    } else {
        None
    };
    let mission = if kind == ObjectKind::Building {
        None
    } else {
        Some(fields[6].clone())
    };
    Ok(MapObject {
        kind,
        house: fields[0].clone(),
        type_id: fields[1].clone(),
        health,
        cell: Cell { x, y },
        facing: facing as u8,
        mission,
        sub_cell,
        fields,
        line: entry.line,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::STANDARD};
    fn sample() -> String {
        let raw = [2, 0, 3, 0, 42, 0, 0, 0, 1, 4, 0];
        let compressed = lzokay::compress::compress(&raw).unwrap();
        let mut packed = Vec::new();
        packed.extend((compressed.len() as u16).to_le_bytes());
        packed.extend((raw.len() as u16).to_le_bytes());
        packed.extend(compressed);
        format!(
            "[Basic]\nName=Synthetic\n[Map]\nSize=0,0,10,10\nLocalSize=1,1,8,8\nTheater=TEMPERATE\n[IsoMapPack5]\n1={}\n[Waypoints]\n0=3002\n[Units]\n0=Americans,TANK,256,2,3,64,Guard,None,0,0\n[Infantry]\n0=Americans,SOLDIER,128,2,3,1,Guard,32,None\n[Structures]\n0=Americans,FACTORY,256,2,3,0,None\n[Triggers]\nT=opaque trigger\n",
            STANDARD.encode(packed)
        )
    }
    #[test]
    fn map_preserves_terrain_objects_waypoints_and_unsupported_sections() {
        let map = Ra2Map::parse(&sample()).unwrap();
        assert_eq!(map.name, "Synthetic");
        assert_eq!(
            map.tiles[0],
            MapTile {
                cell: Cell { x: 2, y: 3 },
                tile_index: 42,
                sub_tile: 1,
                height: 4,
                reserved_tail: 0,
                reserved: [0, 0]
            }
        );
        assert_eq!(map.waypoints[&0], Cell { x: 2, y: 3 });
        assert_eq!(map.objects.len(), 3);
        assert_eq!(map.objects[0].facing, 64);
        assert_eq!(map.objects[1].sub_cell, Some(1));
        assert_eq!(map.objects[1].facing, 32);
        assert_eq!(map.overlay_at(Cell { x: 2, y: 3 }), Some((255, 0)));
        assert_eq!(map.diagnostics[0].section, "Triggers");
        assert!(map.ini.get("Triggers", "T").is_some());
    }
    #[test]
    fn malformed_metadata_objects_and_waypoints_fail_explicitly() {
        let text = sample();
        for (from, to) in [
            ("0,0,10,10", "0,0,0,10"),
            ("1,1,8,8", "9,9,8,8"),
            ("0=3002", "0=999999"),
            ("TANK,256", "TANK,257"),
            ("SOLDIER,128,2,3,1", "SOLDIER,128,2,3,6"),
        ] {
            assert!(Ra2Map::parse(&text.replace(from, to)).is_err(), "{to}");
        }
        assert!(Ra2Map::parse("[Map]\nSize=0,0,10,10\nTheater=SNOW\n").is_err());
    }
}
