//! Narrow stock-installation profile for terrain viewing. No executable is read.
//!
//! Archive priority mirrors gamemd.exe. WinMain (`FUN_006bb9a0`, sites 0x6bd7ef
//! and 0x6bd81f) registers LANGMD.MIX then LANGUAGE.MIX before the game init
//! `FUN_0052ba60` runs `FUN_005301a0` (0x52bb64: EXPANDMD99..00, RA2MD, RA2,
//! CACHEMD, CACHE, LOCALMD, LOCAL), `FUN_00406b10` (0x52bbc7: AUDIOMD, AUDIO),
//! `FUN_00530000` (0x52c307: ECACHE*/ELOCAL* wildcards) and `FUN_00530460`
//! (0x52c59c: CONQMD, GENERMD, GENERIC, ISOGENMD, ISOGEN, CONQUER, CAMEOMD,
//! CAMEO, MAPS*, MULTIMD, MULTI, THEMEMD, THEME, movies). Scenario setup then
//! registers the LOAD and NEUTRAL packs and finally the selected theater
//! (`FUN_005349c0`: `<base>MD.MIX` for snow, `<base>.MIX`, the 3-letter
//! `<code>.MIX`, `<iso>MD.MIX`, `<iso>.MIX`; switching theaters releases the
//! previous set, so the theater group sits below the fixed list).
//!
//! The first registered archive wins: `FUN_005b4430` walks the list from its
//! head, the shipped expandmd01.mix rulesmd.ini has to beat localmd.mix, and
//! every md variant is listed before its base. A name resolves in registered
//! archives before the disk (`CCFileClass::Open`, `FUN_00473c50`: archive
//! search, then `CreateFileA`), so a loose LOCALMD.MIX never shadows the nested
//! copy and a loose RULESMD.INI is ignored while an archive holds one
//! (community-confirmed engine behavior; the disk `RULEMD*.INI` enumeration in
//! `FUN_0052cd70` only adds merge layers). RA2NE therefore mounts loose
//! top-level files first and then the resolved archives in reverse registration
//! order, because the VFS prefers the most recent mount.
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
/// Resume-independent cap on bytes read from disk during one installation load
/// (the shipped YR package needs about 725 MB per load).
const INPUT_BUDGET: usize = 1024 * 1024 * 1024;
/// Cap on bytes kept mounted in the returned VFS. The shipped package mounts
/// about 570 MB for one theater (language, audio, load, neutral, map, multi,
/// video-less packs plus the theater tiles).
const MOUNT_BUDGET: usize = 1024 * 1024 * 1024;
/// Cap on the loose game-directory fallback mount.
const LOOSE_BUDGET: usize = 64 * 1024 * 1024;
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
    /// Theater mix (`<base>.MIX`).
    base_mix: &'static str,
    /// Three-letter theater mix (`<code>.MIX`).
    code_mix: &'static str,
    /// Isometric theater tiles (`<iso>.MIX`).
    iso_mix: &'static str,
    /// Year-specific isometric tiles (`<iso>MD.MIX`).
    iso_mix_md: &'static str,
    /// Year-specific theater mix; gamemd registers it only for snow, which is
    /// the only theater that shipped one (SNOWMD.MIX).
    base_mix_md: Option<&'static str>,
}
impl Profile {
    /// Theater archives in gamemd registration order, highest priority first:
    /// `<base>MD.MIX` (snow only), `<base>.MIX`, `<code>.MIX`, `<iso>MD.MIX`,
    /// `<iso>.MIX` (`FUN_005349c0`, 0x5349c0).
    fn theater_archives(&self, edition: Edition) -> Vec<&'static str> {
        let mut names = Vec::new();
        if edition == Edition::Yr {
            names.extend(self.base_mix_md);
        }
        names.push(self.base_mix);
        names.push(self.code_mix);
        if edition == Edition::Yr {
            names.push(self.iso_mix_md);
        }
        names.push(self.iso_mix);
        names
    }
    fn new(theater: &str, edition: Edition) -> Result<Self, String> {
        let profile = match theater {
            "TEMPERATE" => Self {
                extension: "tem",
                base_ini: Some("temperat.ini"),
                yr_ini: "temperatmd.ini",
                palette: "isotem.pal",
                base_mix: "temperat.mix",
                code_mix: "tem.mix",
                iso_mix: "isotemp.mix",
                iso_mix_md: "isotemmd.mix",
                base_mix_md: None,
            },
            "SNOW" => Self {
                extension: "sno",
                base_ini: Some("snow.ini"),
                yr_ini: "snowmd.ini",
                palette: "isosno.pal",
                base_mix: "snow.mix",
                code_mix: "sno.mix",
                iso_mix: "isosnow.mix",
                iso_mix_md: "isosnomd.mix",
                base_mix_md: Some("snowmd.mix"),
            },
            "URBAN" => Self {
                extension: "urb",
                base_ini: Some("urban.ini"),
                yr_ini: "urbanmd.ini",
                palette: "isourb.pal",
                base_mix: "urban.mix",
                code_mix: "urb.mix",
                iso_mix: "isourb.mix",
                iso_mix_md: "isourbmd.mix",
                base_mix_md: None,
            },
            "DESERT" => Self {
                extension: "des",
                base_ini: None,
                yr_ini: "desertmd.ini",
                palette: "isodes.pal",
                base_mix: "desert.mix",
                code_mix: "des.mix",
                iso_mix: "isodes.mix",
                iso_mix_md: "isodesmd.mix",
                base_mix_md: None,
            },
            "NEWURBAN" => Self {
                extension: "ubn",
                base_ini: None,
                yr_ini: "urbannmd.ini",
                palette: "isoubn.pal",
                base_mix: "urbann.mix",
                code_mix: "ubn.mix",
                iso_mix: "isoubn.mix",
                iso_mix_md: "isoubnmd.mix",
                base_mix_md: None,
            },
            "LUNAR" => Self {
                extension: "lun",
                base_ini: None,
                yr_ini: "lunarmd.ini",
                palette: "isolun.pal",
                base_mix: "lunar.mix",
                code_mix: "lun.mix",
                iso_mix: "isolun.mix",
                iso_mix_md: "isolunmd.mix",
                base_mix_md: None,
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
/// Wildcard enumeration the engine performs with FindFirstFileA; the host
/// filesystem order is arbitrary, so names are sorted for deterministic runs.
fn wildcard(paths: &BTreeMap<String, PathBuf>, prefix: &str, suffix: &str) -> Vec<String> {
    let mut names = paths
        .keys()
        .filter(|name| name.len() > prefix.len() + suffix.len())
        .filter(|name| name.starts_with(prefix) && name.ends_with(suffix))
        .cloned()
        .collect::<Vec<_>>();
    names.sort();
    names
}
/// Engine registration order of the fixed archive list plus the selected
/// theater, highest priority first. Missing names are skipped during opening.
///
/// The RA2 expansion pack name is inferred by analogy: only gamemd.exe was
/// available for analysis, and RA2 patches shipped MIX packs the same way.
fn registration_order(
    edition: Edition,
    profile: &Profile,
    paths: &BTreeMap<String, PathBuf>,
) -> Vec<String> {
    let yr = edition == Edition::Yr;
    let mut names = Vec::new();
    // WinMain (`FUN_006bb9a0`) registers the language packs before everything.
    if yr {
        names.push("langmd.mix".to_owned());
    }
    names.push("language.mix".to_owned());
    // `FUN_005301a0`: EXPANDMD%02d.MIX counted down from 99.
    let expansion = if yr { "expandmd" } else { "expand" };
    names.extend(
        (0..=99)
            .rev()
            .map(|index| format!("{expansion}{index:02}.mix")),
    );
    if yr {
        names.push("ra2md.mix".to_owned());
    }
    names.push("ra2.mix".to_owned());
    if yr {
        names.push("cachemd.mix".to_owned());
    }
    names.push("cache.mix".to_owned());
    if yr {
        names.push("localmd.mix".to_owned());
    }
    names.push("local.mix".to_owned());
    // `FUN_00406b10` audio packs.
    if yr {
        names.push("audiomd.mix".to_owned());
    }
    names.push("audio.mix".to_owned());
    // `FUN_00530000`: ECACHE*.MIX then ELOCAL*.MIX.
    names.extend(wildcard(paths, "ecache", ".mix"));
    names.extend(wildcard(paths, "elocal", ".mix"));
    // `FUN_00530460`.
    if yr {
        names.push("conqmd.mix".to_owned());
    }
    if yr {
        names.push("genermd.mix".to_owned());
    }
    names.push("generic.mix".to_owned());
    if yr {
        names.push("isogenmd.mix".to_owned());
    }
    names.push("isogen.mix".to_owned());
    names.push("conquer.mix".to_owned());
    if yr {
        names.push("cameomd.mix".to_owned());
    }
    names.push("cameo.mix".to_owned());
    names.extend(wildcard(paths, "maps", ".mix"));
    if yr {
        names.push("multimd.mix".to_owned());
    }
    names.push("multi.mix".to_owned());
    if yr {
        names.push("thememd.mix".to_owned());
    }
    names.push("theme.mix".to_owned());
    // Movie packs (MOVMD%02d/MOVIES%02d, MIXFILES\MOV*.MIX) are not terrain
    // resources and are skipped. Scenario setup packs register later:
    // `FUN_00552cc0` LOAD, `FUN_00534e50` NEUTRAL, `FUN_00534fa0` SIDE*%02d
    // (per-side packs, skipped until a side is selected), `FUN_007afb90` WDT.
    if yr {
        names.push("loadmd.mix".to_owned());
    }
    names.push("load.mix".to_owned());
    if yr {
        names.push("ntrlmd.mix".to_owned());
    }
    names.push("neutral.mix".to_owned());
    // `FUN_005349c0` registers the selected theater last of all.
    names.extend(
        profile
            .theater_archives(edition)
            .into_iter()
            .map(str::to_owned),
    );
    names
}
#[derive(Default)]
struct Budget {
    input: usize,
    mounted: usize,
}
impl Budget {
    fn mount(&mut self, bytes: usize) -> Result<(), String> {
        self.mounted += bytes;
        if self.mounted > MOUNT_BUDGET {
            return Err("installation mount budget exceeded".into());
        }
        Ok(())
    }
    fn read(&mut self, path: &Path) -> Result<Vec<u8>, String> {
        let bytes = super::read(
            path.to_str().ok_or("installation path is not UTF-8")?,
            INPUT_BUDGET - self.input,
        )?;
        self.input += bytes.len();
        Ok(bytes)
    }
}
struct Opened {
    /// Path shown in mount diagnostics: the file name, or the container chain
    /// for a nested archive.
    name: String,
    /// Container archives are dropped after opening; see `load`.
    archive: Option<MixArchive>,
    mounted: bool,
}
/// Resolve one registration entry and open it, mirroring the engine: the name
/// is looked up in the already registered archives first (`FUN_005b4430` via
/// `CCFileClass::Open`, `FUN_00473c50`) and only then on disk.
fn open_archive(
    paths: &BTreeMap<String, PathBuf>,
    opened: &mut Vec<Opened>,
    budget: &mut Budget,
    name: &str,
) -> Result<bool, String> {
    for outer in opened.iter() {
        let Some(archive) = outer.archive.as_ref() else {
            continue;
        };
        let Some(bytes) = archive.get(name, FilenameHash::Ra2)? else {
            continue;
        };
        budget.mount(bytes.len())?;
        opened.push(Opened {
            name: format!("{}/{}", outer.name, name),
            archive: Some(MixArchive::parse(bytes.to_vec().into())?),
            mounted: true,
        });
        return Ok(true);
    }
    let Some(path) = paths.get(name) else {
        return Ok(false);
    };
    let bytes = budget.read(path)?;
    // RA2.MIX and RA2MD.MIX only carry the nested archives that are opened
    // individually; keeping their ~486 MB resident would duplicate them.
    let container = matches!(name, "ra2.mix" | "ra2md.mix");
    if !container {
        budget.mount(bytes.len())?;
    }
    opened.push(Opened {
        name: name.to_owned(),
        archive: Some(MixArchive::parse(bytes.into())?),
        mounted: !container,
    });
    Ok(true)
}
/// Loose top-level game-directory files, excluding archives: a MIX file only
/// loads when the registration list above names it, so dropping one into the
/// game directory never registers it. Files beyond the byte budget are skipped
/// with a diagnostic instead of aborting the load.
fn loose_files(paths: &BTreeMap<String, PathBuf>) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut files = Vec::new();
    let mut remaining = LOOSE_BUDGET;
    for (name, path) in paths {
        if name.ends_with(".mix") {
            continue;
        }
        let bytes = match super::read(
            path.to_str().ok_or("installation path is not UTF-8")?,
            remaining,
        ) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!("loose file not mounted: {name} ({error})");
                continue;
            }
        };
        remaining -= bytes.len();
        files.push((name.clone(), bytes));
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
    let mut budget = Budget::default();
    let mut opened: Vec<Opened> = Vec::new();
    for name in registration_order(edition, &profile, &paths) {
        let required = name == "ra2.mix" || (edition == Edition::Yr && name == "ra2md.mix");
        if !open_archive(&paths, &mut opened, &mut budget, &name)? && required {
            return Err(format!("installation requires {name}"));
        }
    }
    for entry in opened.iter_mut() {
        if !entry.mounted {
            entry.archive = None;
        }
    }
    let mut files = Vfs::default();
    let loose = loose_files(&paths)?;
    budget.mount(loose.iter().map(|(_, bytes)| bytes.len()).sum())?;
    if !loose.is_empty() {
        files.mount("loose game directory", loose)?;
    }
    // Reverse registration order: the VFS prefers the most recent mount.
    for entry in opened.iter_mut().rev() {
        if !entry.mounted {
            continue;
        }
        let Some(archive) = entry.archive.take() else {
            continue;
        };
        let source = format!("{root}/{}", entry.name);
        files.mount_mix(&source, archive, FilenameHash::Ra2)?;
        eprintln!("terrain mount: {source}");
    }
    let mut ini = String::new();
    let mut ini_names = Vec::new();
    if edition == Edition::Ra2 {
        // ra2.exe reads the un-suffixed theater INI.
        if let Some(base) = profile.base_ini {
            ini_names.push(base);
        }
    } else {
        // gamemd.exe builds "%sMD.INI" only (FUN_00545150); the md files
        // already supersede the base lists, so the base is not merged.
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
    fn scratch_directory() -> Directory {
        Directory(std::env::temp_dir().join(format!(
            "ra2ne-install-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )))
    }
    struct Directory(PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn stock_directory_uses_md_theater_ini_and_overrides_base_resources() {
        let root = scratch_directory();
        std::fs::create_dir(&root.0).unwrap();
        let base = mix(vec![
            (
                "local.mix",
                mix(vec![(
                    "temperat.ini",
                    b"[TileSet0000]\nFileName=clear\nTilesInSet=1\n[TileSet0001]\nFileName=slope\nTilesInSet=1".to_vec(),
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
                    b"[TileSet0000]\nFileName=clear\nTilesInSet=2".to_vec(),
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
        assert_eq!(catalogue.tiles.len(), 2);
        assert_eq!(catalogue.tiles[1].filename, "slope01.tem");
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
        // The md INI alone builds the table: the base-only slope set is absent.
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
        std::fs::write(
            root.0.join("EXPANDMD01.MIX"),
            mix(vec![("clear01.tem", tile(11))]),
        )
        .unwrap();
        let (catalogue, _, files) =
            load(path, Edition::Yr, "TEMPERATE", TextEncoding::Utf8).unwrap();
        assert_eq!(
            catalogue
                .load(0, &files)
                .unwrap()
                .diamond(0)
                .unwrap()
                .pixels[2],
            11
        );
        std::fs::write(
            root.0.join("expandmd02.mix"),
            mix(vec![("clear01.tem", tile(12))]),
        )
        .unwrap();
        let (catalogue, _, files) =
            load(path, Edition::Yr, "TEMPERATE", TextEncoding::Utf8).unwrap();
        assert_eq!(
            catalogue
                .load(0, &files)
                .unwrap()
                .diamond(0)
                .unwrap()
                .pixels[2],
            12
        );
        let (catalogue, _, files) =
            load(path, Edition::Ra2, "TEMPERATE", TextEncoding::Utf8).unwrap();
        assert_eq!(
            catalogue
                .load(0, &files)
                .unwrap()
                .diamond(0)
                .unwrap()
                .pixels[2],
            7
        );
        std::fs::write(root.0.join("ra2md.mix"), b"broken").unwrap();
        assert!(load(path, Edition::Yr, "TEMPERATE", TextEncoding::Utf8).is_err());
        assert!(load(path, Edition::Ra2, "TEMPERATE", TextEncoding::Utf8).is_ok());
        std::fs::write(root.0.join("ra2.mix"), base).unwrap();
        assert!(load(path, Edition::Ra2, "TEMPERATE", TextEncoding::Utf8).is_err());
    }
    #[test]
    fn registration_order_resolves_archives_before_disk() {
        let root = scratch_directory();
        std::fs::create_dir(&root.0).unwrap();
        std::fs::write(
            root.0.join("ra2.mix"),
            mix(vec![
                (
                    "local.mix",
                    mix(vec![
                        (
                            "temperat.ini",
                            b"[TileSet0000]\nFileName=clear\nTilesInSet=1".to_vec(),
                        ),
                        ("rulesmd.ini", b"archived rules".to_vec()),
                        ("archived.txt", b"from archive".to_vec()),
                    ]),
                ),
                ("cache.mix", mix(vec![("isotem.pal", vec![1; 768])])),
                (
                    "temperat.mix",
                    mix(vec![("theater_probe.txt", b"nested theater".to_vec())]),
                ),
                ("isotemp.mix", mix(vec![("clear01.tem", tile(7))])),
            ]),
        )
        .unwrap();
        std::fs::write(
            root.0.join("ra2md.mix"),
            mix(vec![
                (
                    "localmd.mix",
                    mix(vec![(
                        "temperatmd.ini",
                        b"[TileSet0000]\nFileName=clear\nTilesInSet=1".to_vec(),
                    )]),
                ),
                ("isotemmd.mix", mix(vec![("clear01.tem", tile(9))])),
            ]),
        )
        .unwrap();
        // LANGMD.MIX registers before RA2MD.MIX, so CAMEOMD.MIX resolves inside
        // it and the loose copy is never registered.
        std::fs::write(
            root.0.join("langmd.mix"),
            mix(vec![(
                "cameomd.mix",
                mix(vec![("cameo_probe.txt", b"from langmd".to_vec())]),
            )]),
        )
        .unwrap();
        std::fs::write(
            root.0.join("cameomd.mix"),
            mix(vec![("cameo_probe.txt", b"from top level".to_vec())]),
        )
        .unwrap();
        std::fs::write(
            root.0.join("isotemmd.mix"),
            mix(vec![("clear01.tem", tile(3))]),
        )
        .unwrap();
        std::fs::write(
            root.0.join("Temperat.mix"),
            mix(vec![("theater_probe.txt", b"loose theater".to_vec())]),
        )
        .unwrap();
        std::fs::write(root.0.join("archived.txt"), b"from disk").unwrap();
        std::fs::write(root.0.join("loose_only.txt"), b"loose only").unwrap();
        std::fs::write(root.0.join("RULESMD.INI"), b"loose rules").unwrap();
        std::fs::write(root.0.join("rulemd01.ini"), b"add-on rules").unwrap();
        let path = root.0.to_str().unwrap();
        let (catalogue, _, files) =
            load(path, Edition::Yr, "TEMPERATE", TextEncoding::Utf8).unwrap();
        let probe = |name: &str| {
            String::from_utf8(files.get(name).unwrap().unwrap().bytes.to_vec()).unwrap()
        };
        // CAMEOMD.MIX resolves inside LANGMD.MIX before the loose CAMEO.MIX.
        assert_eq!(probe("cameo_probe.txt"), "from langmd");
        // ISOTEMMD.MIX resolves inside RA2MD.MIX before the loose copy.
        assert_eq!(
            catalogue
                .load(0, &files)
                .unwrap()
                .diamond(0)
                .unwrap()
                .pixels[2],
            9
        );
        // TEMPERAT.MIX resolves inside RA2.MIX before the loose copy.
        assert_eq!(probe("theater_probe.txt"), "nested theater");
        // Registered archives shadow loose files of the same name.
        assert_eq!(probe("archived.txt"), "from archive");
        // Names no archive holds fall through to the game directory.
        assert_eq!(probe("loose_only.txt"), "loose only");
        // A loose rulesmd.ini is ignored while an archive carries one: the name
        // resolves through CCFileClass::Open (archives first), and gamemd's
        // disk RULEMD*.INI enumeration (FUN_0052cd70) only adds merge layers.
        assert_eq!(probe("rulesmd.ini"), "archived rules");
        assert_eq!(probe("rulemd01.ini"), "add-on rules");
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
        // gamemd registers `<base>MD.MIX` only for snow (`FUN_005349c0`).
        assert_eq!(
            Profile::new("SNOW", Edition::Yr)
                .unwrap()
                .theater_archives(Edition::Yr),
            [
                "snowmd.mix",
                "snow.mix",
                "sno.mix",
                "isosnomd.mix",
                "isosnow.mix"
            ]
        );
        assert_eq!(
            Profile::new("SNOW", Edition::Ra2)
                .unwrap()
                .theater_archives(Edition::Ra2),
            ["snow.mix", "sno.mix", "isosnow.mix"]
        );
    }
}
