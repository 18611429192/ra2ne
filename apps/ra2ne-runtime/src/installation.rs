//! Narrow stock-installation profile for terrain viewing. No executable is read.
use ra2ne_assets::{
    mix::{FilenameHash, MixArchive},
    sprite::Palette,
    text::TextEncoding,
    theater::Theater,
    vfs::Vfs,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
const INPUT_BUDGET: usize = 512 * 1024 * 1024;
const MOUNT_BUDGET: usize = 256 * 1024 * 1024;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Edition {
    Ra2,
    Yr,
}
impl Edition {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "ra2" => Ok(Self::Ra2),
            "yr" => Ok(Self::Yr),
            _ => Err("edition must be ra2 or yr".into()),
        }
    }
}
struct Profile {
    extension: &'static str,
    base_ini: Option<&'static str>,
    yr_ini: &'static str,
    palette: &'static str,
    base_mix: &'static str,
    yr_mix: &'static str,
}
impl Profile {
    fn auxiliary(&self) -> [&'static str; 4] {
        match self.extension {
            "tem" => ["temperat.mix", "tem.mix", "temperatmd.mix", "temmd.mix"],
            "sno" => ["snow.mix", "sno.mix", "snowmd.mix", "snomd.mix"],
            "urb" => ["urban.mix", "urb.mix", "urbanmd.mix", "urbmd.mix"],
            "des" => ["desert.mix", "des.mix", "desertmd.mix", "desmd.mix"],
            "ubn" => ["urbann.mix", "ubn.mix", "urbannmd.mix", "ubnmd.mix"],
            "lun" => ["lunar.mix", "lun.mix", "lunarmd.mix", "lunmd.mix"],
            _ => unreachable!("validated theater extension"),
        }
    }
    fn new(theater: &str, edition: Edition) -> Result<Self, String> {
        let profile = match theater {
            "TEMPERATE" => Self {
                extension: "tem",
                base_ini: Some("temperat.ini"),
                yr_ini: "temperatmd.ini",
                palette: "isotem.pal",
                base_mix: "isotemp.mix",
                yr_mix: "isotemmd.mix",
            },
            "SNOW" => Self {
                extension: "sno",
                base_ini: Some("snow.ini"),
                yr_ini: "snowmd.ini",
                palette: "isosno.pal",
                base_mix: "isosnow.mix",
                yr_mix: "isosnomd.mix",
            },
            "URBAN" => Self {
                extension: "urb",
                base_ini: Some("urban.ini"),
                yr_ini: "urbanmd.ini",
                palette: "isourb.pal",
                base_mix: "isourb.mix",
                yr_mix: "isourbmd.mix",
            },
            "DESERT" => Self {
                extension: "des",
                base_ini: None,
                yr_ini: "desertmd.ini",
                palette: "isodes.pal",
                base_mix: "isodes.mix",
                yr_mix: "isodesmd.mix",
            },
            "NEWURBAN" => Self {
                extension: "ubn",
                base_ini: None,
                yr_ini: "urbannmd.ini",
                palette: "isoubn.pal",
                base_mix: "isoubn.mix",
                yr_mix: "isoubnmd.mix",
            },
            "LUNAR" => Self {
                extension: "lun",
                base_ini: None,
                yr_ini: "lunarmd.ini",
                palette: "isolun.pal",
                base_mix: "isolun.mix",
                yr_mix: "isolunmd.mix",
            },
            _ => return Err("unsupported installation theater".into()),
        };
        if edition == Edition::Ra2 && profile.base_ini.is_none() {
            return Err("this theater requires the yr edition".into());
        }
        Ok(profile)
    }
}
fn directory(root: &Path) -> Result<BTreeMap<String, PathBuf>, String> {
    let mut files = BTreeMap::new();
    for entry in std::fs::read_dir(root).map_err(|e| format!("{}: {e}", root.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_ascii_lowercase) else {
            continue;
        };
        if files.insert(name.clone(), entry.path()).is_some() {
            return Err(format!("ambiguous installation filenames: {name}"));
        }
    }
    Ok(files)
}
pub fn load(
    root: &str,
    edition: Edition,
    theater: &str,
    encoding: TextEncoding,
) -> Result<(Theater, Palette, Vfs), String> {
    let profile = Profile::new(theater, edition)?;
    let paths = directory(Path::new(root))?;
    let mut files = Vfs::default();
    let mut input_bytes = 0;
    let mut mounted_bytes = 0;
    for name in if edition == Edition::Yr {
        vec!["ra2.mix", "ra2md.mix"]
    } else {
        vec!["ra2.mix"]
    } {
        let path = paths
            .get(name)
            .ok_or_else(|| format!("installation requires {name}"))?;
        let bytes = super::read(
            path.to_str().ok_or("installation path is not UTF-8")?,
            INPUT_BUDGET - input_bytes,
        )?;
        input_bytes += bytes.len();
        let archive = MixArchive::parse(bytes.into())?;
        let auxiliary = profile.auxiliary();
        let mut seen = std::collections::BTreeSet::new();
        for nested in [
            "local.mix",
            "cache.mix",
            "conquer.mix",
            "generic.mix",
            "temperat.mix",
            "tem.mix",
            if theater == "NEWURBAN" {
                "urban.mix"
            } else {
                auxiliary[0]
            },
            if theater == "NEWURBAN" {
                "urb.mix"
            } else {
                auxiliary[1]
            },
            auxiliary[0],
            auxiliary[1],
            "isogen.mix",
            "isotemp.mix",
            if theater == "NEWURBAN" {
                "isourb.mix"
            } else {
                profile.base_mix
            },
            profile.base_mix,
            "localmd.mix",
            "cachemd.mix",
            "conqmd.mix",
            "genermd.mix",
            "temperatmd.mix",
            "temmd.mix",
            if theater == "NEWURBAN" {
                "urbanmd.mix"
            } else {
                auxiliary[2]
            },
            if theater == "NEWURBAN" {
                "urbmd.mix"
            } else {
                auxiliary[3]
            },
            auxiliary[2],
            auxiliary[3],
            "isogenmd.mix",
            "isotemmd.mix",
            if theater == "NEWURBAN" {
                "isourbmd.mix"
            } else {
                profile.yr_mix
            },
            profile.yr_mix,
        ] {
            if !seen.insert(nested) {
                continue;
            }
            if edition == Edition::Ra2 && (nested.ends_with("md.mix") || nested == "localmd.mix") {
                continue;
            }
            let Some(bytes) = archive.get(nested, FilenameHash::Ra2)? else {
                continue;
            };
            mounted_bytes += bytes.len();
            if mounted_bytes > MOUNT_BUDGET {
                return Err("installation terrain mount budget exceeded".into());
            }
            let source = format!("{root}/{name}/{nested}");
            files.mount_mix(&source, MixArchive::parse(bytes.into())?, FilenameHash::Ra2)?;
            eprintln!("terrain mount: {source}");
        }
    }
    let mut ini = String::new();
    let mut ini_names = Vec::new();
    if let Some(base) = profile.base_ini {
        ini_names.push(base);
    }
    if edition == Edition::Yr {
        ini_names.push(profile.yr_ini);
    }
    for name in ini_names {
        let file = files
            .get(name)?
            .ok_or_else(|| format!("installation theater INI missing: {name}"))?;
        ini.push_str(&encoding.decode(file.bytes)?);
        ini.push('\n');
    }
    let palette = files
        .get(profile.palette)?
        .ok_or_else(|| format!("installation terrain palette missing: {}", profile.palette))?;
    Ok((
        Theater::parse(&ini, profile.extension)?,
        Palette::parse(palette.bytes)?,
        files,
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn mix(entries: Vec<(&str, Vec<u8>)>) -> Vec<u8> {
        let mut bytes = (entries.len() as u16).to_le_bytes().to_vec();
        bytes.extend(
            (entries.iter().map(|(_, bytes)| bytes.len()).sum::<usize>() as u32).to_le_bytes(),
        );
        let mut offset = 0_u32;
        for (name, data) in &entries {
            bytes.extend(
                ra2ne_assets::mix::filename_id(name, FilenameHash::Ra2)
                    .unwrap()
                    .to_le_bytes(),
            );
            bytes.extend(offset.to_le_bytes());
            bytes.extend((data.len() as u32).to_le_bytes());
            offset += data.len() as u32;
        }
        for (_, data) in entries {
            bytes.extend(data);
        }
        bytes
    }
    fn tile(value: u8) -> Vec<u8> {
        let mut bytes = Vec::new();
        for number in [1_u32, 1, 8, 4, 20] {
            bytes.extend(number.to_le_bytes());
        }
        bytes.extend([0_u8; 52]);
        bytes.extend([value; 16]);
        bytes
    }
    #[test]
    fn stock_directory_merges_yr_ini_and_overrides_base_resources() {
        struct Directory(PathBuf);
        impl Drop for Directory {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let root = Directory(std::env::temp_dir().join(format!(
                "ra2ne-install-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )));
        std::fs::create_dir(&root.0).unwrap();
        let base = mix(vec![
            (
                "local.mix",
                mix(vec![(
                    "temperat.ini",
                    b"[TileSet0000]\nFileName=clear\nTilesInSet=1".to_vec(),
                )]),
            ),
            ("cache.mix", mix(vec![("isotem.pal", vec![1; 768])])),
            ("isotemp.mix", mix(vec![("clear01.tem", tile(7))])),
        ]);
        let yr = mix(vec![
            (
                "localmd.mix",
                mix(vec![(
                    "temperatmd.ini",
                    b"[TileSet0000]\nTilesInSet=2".to_vec(),
                )]),
            ),
            ("cachemd.mix", mix(vec![("isotem.pal", vec![3; 768])])),
            (
                "isotemmd.mix",
                mix(vec![("clear01.tem", tile(9)), ("clear02.tem", tile(10))]),
            ),
        ]);
        std::fs::write(root.0.join("RA2.MIX"), &base).unwrap();
        std::fs::write(root.0.join("ra2md.mix"), yr).unwrap();
        std::fs::write(root.0.join("RA2.exe"), b"ignored executable").unwrap();
        let path = root.0.to_str().unwrap();
        let (catalogue, palette, files) =
            load(path, Edition::Ra2, "TEMPERATE", TextEncoding::Utf8).unwrap();
        assert_eq!(catalogue.tiles.len(), 1);
        assert_eq!(palette.colors[0], [4; 3]);
        assert_eq!(
            catalogue
                .load(0, &files)
                .unwrap()
                .diamond(0)
                .unwrap()
                .pixels[2],
            7
        );
        let (catalogue, palette, files) =
            load(path, Edition::Yr, "TEMPERATE", TextEncoding::Utf8).unwrap();
        assert_eq!(catalogue.tiles.len(), 2);
        assert_eq!(catalogue.tiles[1].filename, "clear02.tem");
        assert_eq!(palette.colors[0], [12; 3]);
        assert_eq!(
            catalogue
                .load(0, &files)
                .unwrap()
                .diamond(0)
                .unwrap()
                .pixels[2],
            9
        );
        assert_eq!(
            catalogue
                .load(1, &files)
                .unwrap()
                .diamond(0)
                .unwrap()
                .pixels[2],
            10
        );
        std::fs::write(root.0.join("ra2md.mix"), b"broken").unwrap();
        assert!(load(path, Edition::Yr, "TEMPERATE", TextEncoding::Utf8).is_err());
        assert!(load(path, Edition::Ra2, "TEMPERATE", TextEncoding::Utf8).is_ok());
        std::fs::write(root.0.join("ra2.mix"), base).unwrap();
        assert!(load(path, Edition::Ra2, "TEMPERATE", TextEncoding::Utf8).is_err());
    }
    #[test]
    fn theater_profiles_use_iso_palettes_and_gate_yr_only_theaters() {
        for (theater, extension, palette) in [
            ("TEMPERATE", "tem", "isotem.pal"),
            ("SNOW", "sno", "isosno.pal"),
            ("URBAN", "urb", "isourb.pal"),
            ("DESERT", "des", "isodes.pal"),
            ("NEWURBAN", "ubn", "isoubn.pal"),
            ("LUNAR", "lun", "isolun.pal"),
        ] {
            let profile = Profile::new(theater, Edition::Yr).unwrap();
            assert_eq!((profile.extension, profile.palette), (extension, palette));
            assert_eq!(
                Profile::new(theater, Edition::Ra2).is_ok(),
                profile.base_ini.is_some()
            );
        }
        assert!(Profile::new("UNKNOWN", Edition::Yr).is_err());
        assert!(Edition::parse("auto").is_err());
    }
}
