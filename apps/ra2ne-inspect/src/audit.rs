//! Read-only resource acceptance scan. Parsing coverage is not gameplay coverage.
use ra2ne_assets::{
    map::Ra2Map,
    mix::{FilenameHash, MAX_ARCHIVE_BYTES, MixArchive, filename_id},
    sprite::{Palette, Shp},
    text::TextEncoding,
    tmp::Tmp,
};
use std::{collections::BTreeMap, fs, path::Path, sync::Arc};

#[derive(Default)]
struct Audit {
    archives: usize,
    encrypted: usize,
    checksums: usize,
    empty_archives: usize,
    maps: usize,
    shp: usize,
    frames: usize,
    tmp: usize,
    tiles: usize,
    palettes: usize,
    map_diagnostics: usize,
    off_grid_waypoints: usize,
    trailers: BTreeMap<String, usize>,
    unidentified: usize,
    known_other: usize,
    failures: Vec<String>,
    failure_kinds: BTreeMap<String, usize>,
}
impl Audit {
    fn fail(&mut self, path: &str, message: impl std::fmt::Display) {
        *self.failure_kinds.entry(message.to_string()).or_default() += 1;
        self.failures.push(format!("{path}: {message}"));
    }
    fn archive(&mut self, bytes: Arc<[u8]>, path: &str, depth: usize) {
        if depth > 8 || self.archives >= 256 {
            self.fail(path, "archive depth/count limit exceeded");
            return;
        }
        let archive = match MixArchive::parse(bytes) {
            Ok(a) => a,
            Err(e) => {
                self.fail(path, e);
                return;
            }
        };
        self.archives += 1;
        self.encrypted += usize::from(archive.encrypted);
        self.checksums += usize::from(archive.checksum_verified);
        let mut names = BTreeMap::new();
        // Well-known containers work even when an official archive has no XCC
        // database. All other unnamed candidates are identified by structure.
        for name in [
            "local.mix",
            "localmd.mix",
            "cache.mix",
            "cachemd.mix",
            "conquer.mix",
            "conqmd.mix",
            "isotemp.mix",
            "isotemmd.mix",
            "rules.ini",
            "rulesmd.ini",
            "art.ini",
            "artmd.ini",
        ] {
            names.insert(
                filename_id(name, FilenameHash::Ra2).unwrap(),
                name.to_owned(),
            );
        }
        match archive.local_names() {
            Ok(Some(db)) if matches!(db.game, 2 | 5 | 6) => {
                for name in db.names {
                    let id = filename_id(&name, FilenameHash::Ra2).unwrap();
                    // A collision cannot supply an authoritative filename.
                    if let Some(old) = names.get(&id)
                        && !old.eq_ignore_ascii_case(&name)
                    {
                        self.fail(
                            path,
                            format!("ambiguous filename ID {id:08x}: {old} / {name}"),
                        );
                        continue;
                    }
                    names.insert(id, name);
                }
            }
            Ok(Some(_)) => self.fail(path, "unsupported XCC game hash metadata"),
            Ok(None) => {}
            Err(e) => self.fail(path, e),
        }
        for (id, _) in archive.entries() {
            let bytes = archive.get_id(id).unwrap();
            let name = names.get(&id).map(String::as_str);
            let child = format!(
                "{path}/{}",
                name.map_or_else(|| format!("{id:08x}"), str::to_owned)
            );
            self.content(bytes, &child, name, depth);
        }
    }
    fn content(&mut self, bytes: &[u8], path: &str, name: Option<&str>, depth: usize) {
        let extension = name
            .and_then(|n| n.rsplit_once('.'))
            .map(|(_, e)| e.to_ascii_lowercase());
        let ext = extension.as_deref().unwrap_or("");
        if ext == "mix" || looks_like_mix(bytes) {
            self.archive(Arc::from(bytes), path, depth + 1);
        } else if ["map", "mpr", "yrm"].contains(&ext) || has_section(bytes, b"[IsoMapPack5]") {
            match TextEncoding::Windows1252
                .decode(bytes)
                .map_err(str::to_owned)
                .and_then(|text| Ra2Map::parse(&text))
            {
                Ok(map) => {
                    self.maps += 1;
                    self.map_diagnostics += map.diagnostics.len();
                    self.off_grid_waypoints += map.unresolved_waypoints.len();
                    if let Some(tail) = map.terrain_trailer {
                        let key = tail.iter().map(|b| format!("{b:02x}")).collect();
                        *self.trailers.entry(key).or_default() += 1;
                    }
                }
                Err(e) => self.fail(path, e),
            }
        } else if ext == "shp"
            || (name.is_none() || ["tem", "sno", "urb", "ubn", "des", "lun"].contains(&ext))
                && looks_like_shp(bytes)
        {
            match Shp::parse(Arc::from(bytes)) {
                Ok(shp) => {
                    for index in 0..shp.frame_count() {
                        if let Err(e) = shp.frame(index) {
                            self.fail(path, format!("frame {index}: {e}"));
                            return;
                        }
                        self.frames += 1;
                    }
                    self.shp += 1;
                }
                Err(e) => self.fail(path, e),
            }
        } else if ["tem", "sno", "urb", "ubn", "des", "lun"].contains(&ext)
            || name.is_none() && looks_like_tmp(bytes)
        {
            match Tmp::parse(Arc::from(bytes)) {
                Ok(tmp) => {
                    for index in 0..tmp.tile_count() {
                        if tmp.tile(index).is_none() {
                            continue;
                        }
                        if let Err(e) = tmp.diamond(index).and_then(|_| tmp.z_plane(index)) {
                            self.fail(path, format!("tile {index}: {e}"));
                            return;
                        }
                        self.tiles += 1;
                    }
                    self.tmp += 1;
                }
                Err(e) => self.fail(path, e),
            }
        } else if ext == "pal" {
            match Palette::parse(bytes) {
                Ok(_) => self.palettes += 1,
                Err(e) => self.fail(path, e),
            }
        } else if name.is_some() {
            self.known_other += 1;
        } else {
            self.unidentified += 1;
        }
    }
}
fn has_section(bytes: &[u8], section: &[u8]) -> bool {
    bytes.len() <= 16 * 1024 * 1024
        && bytes
            .windows(section.len())
            .any(|w| w.eq_ignore_ascii_case(section))
}
fn looks_like_shp(b: &[u8]) -> bool {
    if b.len() < 32 || b[..2] != [0, 0] {
        return false;
    }
    let word = |at| u16::from_le_bytes([b[at], b[at + 1]]) as usize;
    let (w, h, n) = (word(2), word(4), word(6));
    w > 0 && h > 0 && w * h <= 4_194_304 && n > 0 && n <= 10_000 && 8 + n * 24 <= b.len()
}
fn looks_like_tmp(b: &[u8]) -> bool {
    if b.len() < 20 {
        return false;
    }
    let n = |at| u32::from_le_bytes(b[at..at + 4].try_into().unwrap());
    (1..=256).contains(&n(0))
        && (1..=256).contains(&n(4))
        && matches!((n(8), n(12)), (60, 30) | (48, 24))
}
fn looks_like_mix(b: &[u8]) -> bool {
    if b.len() < 10 {
        return false;
    }
    let first = u16::from_le_bytes(b[..2].try_into().unwrap()) as usize;
    let flags = u32::from_le_bytes(b[..4].try_into().unwrap());
    if first == 0 && flags & !0x30000 != 0 {
        return false;
    }
    if first == 0 && flags & 0x20000 != 0 {
        return b.len() >= 92;
    }
    let start = if first == 0 { 4 } else { 0 };
    let count = u16::from_le_bytes(b[start..start + 2].try_into().unwrap()) as usize;
    let size = u32::from_le_bytes(b[start + 2..start + 6].try_into().unwrap()) as usize;
    let checksum = if first == 0 && flags & 0x10000 != 0 {
        20
    } else {
        0
    };
    count > 0 && start + 6 + count * 12 + size + checksum == b.len()
}
pub fn run(path: &str) -> Result<(), String> {
    let mut audit = Audit::default();
    let input = Path::new(path);
    let mut paths = if input.is_dir() {
        fs::read_dir(input)
            .map_err(|e| e.to_string())?
            .map(|e| e.map(|e| e.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    } else {
        vec![input.to_owned()]
    };
    paths.sort();
    let mut inspected = 0;
    for p in paths {
        let name = p
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("non-UTF8 resource filename")?;
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !p.is_file()
            || ![
                "mix", "map", "mpr", "yrm", "shp", "pal", "tem", "sno", "urb", "ubn", "des", "lun",
            ]
            .contains(&ext.as_str())
        {
            continue;
        }
        inspected += 1;
        let path = p.to_str().ok_or("non-UTF8 resource path")?;
        let bytes = super::read_bounded(path, MAX_ARCHIVE_BYTES)?;
        if ext == "mix" && bytes.is_empty() {
            audit.empty_archives += 1;
            continue;
        }
        if ext == "mix" {
            audit.archive(Arc::from(bytes), name, 0);
        } else {
            audit.content(&bytes, name, Some(name), 0);
        }
    }
    if inspected == 0 {
        return Err("no supported resource files found".into());
    }
    println!(
        "archives={}; encrypted={}; verified_checksums={}; empty_mix_placeholders={}; maps={}; shp={}; decoded_frames={}; tmp={}; decoded_tiles={}; palettes={}; failures={}",
        audit.archives,
        audit.encrypted,
        audit.checksums,
        audit.empty_archives,
        audit.maps,
        audit.shp,
        audit.frames,
        audit.tmp,
        audit.tiles,
        audit.palettes,
        audit.failures.len()
    );
    println!(
        "map_diagnostics={}; off_grid_waypoints={}; terrain_trailers={:?}; unidentified_entries={}; known_unchecked_entries={}",
        audit.map_diagnostics,
        audit.off_grid_waypoints,
        audit.trailers,
        audit.unidentified,
        audit.known_other
    );
    for failure in audit.failures.iter().take(100) {
        println!("failure={failure}");
    }
    if audit.failures.len() > 100 {
        println!("additional_failures={}", audit.failures.len() - 100);
    }
    println!(
        "coverage=resource parsing only; unidentified/other entries and gameplay remain unverified"
    );
    println!("failure_kinds={:?}", audit.failure_kinds);
    if !audit.failures.is_empty() {
        return Err("resource audit found failures".into());
    }
    Ok(())
}
