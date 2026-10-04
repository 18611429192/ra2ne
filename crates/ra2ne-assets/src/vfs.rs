//! In-memory layered virtual files. Later mounts override earlier mounts.
//! Archive decoding and disk traversal are intentionally separate adapters.
use crate::mix::{FilenameHash, MixArchive};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone, Debug)]
pub struct VirtualFile {
    pub source: String,
    pub bytes: Arc<[u8]>,
}
#[derive(Debug)]
enum Mount {
    Files(BTreeMap<String, VirtualFile>),
    Mix {
        source: String,
        archive: MixArchive,
        hash: FilenameHash,
    },
}
#[derive(Debug)]
pub struct FileView<'a> {
    pub source: &'a str,
    pub bytes: &'a [u8],
}
#[derive(Debug, Default)]
pub struct Vfs {
    mounts: Vec<Mount>,
}

/// Normalize logical asset names, never host paths. Reject ambiguous/traversal
/// names instead of silently mapping them to another file.
pub fn canonical_name(name: &str) -> Result<String, &'static str> {
    let normalized = name.replace('\\', "/");
    if normalized.is_empty()
        || normalized.starts_with('/')
        || normalized.chars().any(|c| c.is_control() || c == ':')
        || normalized
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == ".." || p.trim() != p)
    {
        return Err("invalid virtual asset name");
    }
    Ok(normalized.to_ascii_lowercase())
}
impl Vfs {
    /// Fully validate a mount before replacing any files. Case collisions within
    /// one mount are errors, so host directory enumeration cannot choose winners.
    pub fn mount(
        &mut self,
        source: &str,
        files: Vec<(String, Vec<u8>)>,
    ) -> Result<(), &'static str> {
        if source.is_empty() {
            return Err("empty mount source");
        }
        let mut staged = BTreeMap::new();
        for (name, bytes) in files {
            let name = canonical_name(&name)?;
            if staged
                .insert(
                    name,
                    VirtualFile {
                        source: source.to_owned(),
                        bytes: Arc::from(bytes),
                    },
                )
                .is_some()
            {
                return Err("case collision within mount");
            }
        }
        self.mounts.push(Mount::Files(staged));
        Ok(())
    }
    /// Import a chosen loose-file directory with a total byte budget. Symlinks
    /// and non-UTF-8 names are rejected rather than traversed implicitly. The
    /// entire mount remains atomic on errors, including case collisions.
    pub fn mount_directory(
        &mut self,
        source: &str,
        root: &std::path::Path,
        max_bytes: usize,
    ) -> Result<(), String> {
        fn visit(
            root: &std::path::Path,
            at: &std::path::Path,
            depth: usize,
            remaining: &mut usize,
            files: &mut Vec<(String, Vec<u8>)>,
        ) -> Result<(), String> {
            if depth > 32 || files.len() >= 100_000 {
                return Err("loose mount exceeds directory limits".into());
            }
            let metadata = std::fs::symlink_metadata(at).map_err(|e| e.to_string())?;
            if metadata.file_type().is_symlink() {
                return Err("symlinks are not supported in loose mounts".into());
            }
            if metadata.is_dir() {
                let mut paths = std::fs::read_dir(at)
                    .map_err(|e| e.to_string())?
                    .map(|entry| entry.map(|e| e.path()))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())?;
                paths.sort();
                for path in paths {
                    visit(root, &path, depth + 1, remaining, files)?;
                }
            } else if metadata.is_file() {
                use std::io::Read;
                let name = at
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_str()
                    .ok_or("non-UTF-8 file name")?;
                let mut bytes = Vec::new();
                std::fs::File::open(at)
                    .map_err(|e| e.to_string())?
                    .take(*remaining as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                if bytes.len() > *remaining {
                    return Err("loose mount exceeds byte budget".into());
                }
                *remaining -= bytes.len();
                files.push((name.to_owned(), bytes));
            } else {
                return Err("unsupported loose file type".into());
            }
            Ok(())
        }
        if !root.is_dir() {
            return Err("loose mount root must be a directory".into());
        }
        let mut files = Vec::new();
        let mut remaining = max_bytes;
        visit(root, root, 0, &mut remaining, &mut files)?;
        self.mount(source, files).map_err(str::to_owned)
    }
    pub fn mount_mix(
        &mut self,
        source: &str,
        archive: MixArchive,
        hash: FilenameHash,
    ) -> Result<(), &'static str> {
        if source.is_empty() {
            return Err("empty mount source");
        }
        self.mounts.push(Mount::Mix {
            source: source.to_owned(),
            archive,
            hash,
        });
        Ok(())
    }
    pub fn get(&self, name: &str) -> Result<Option<FileView<'_>>, &'static str> {
        let name = canonical_name(name)?;
        for mount in self.mounts.iter().rev() {
            match mount {
                Mount::Files(files) => {
                    if let Some(file) = files.get(&name) {
                        return Ok(Some(FileView {
                            source: &file.source,
                            bytes: &file.bytes,
                        }));
                    }
                }
                Mount::Mix {
                    source,
                    archive,
                    hash,
                } if name.is_ascii() => {
                    if let Some(bytes) = archive.get(&name, *hash)? {
                        return Ok(Some(FileView { source, bytes }));
                    }
                }
                Mount::Mix { .. } => {}
            }
        }
        Ok(None)
    }
    /// MIX indexes contain hashes, not recoverable names. This enumerates named
    /// loose files only; callers can still resolve known names inside archives.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.mounts
            .iter()
            .filter_map(|mount| match mount {
                Mount::Files(files) => Some(files.keys().map(String::as_str)),
                Mount::Mix { .. } => None,
            })
            .flatten()
            .collect::<BTreeSet<_>>()
            .into_iter()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mod_layers_override_and_retain_source() {
        let mut vfs = Vfs::default();
        vfs.mount(
            "base",
            vec![
                ("RulesMD.INI".into(), vec![1]),
                ("Maps\\Test.map".into(), vec![2]),
            ],
        )
        .unwrap();
        vfs.mount("mod", vec![("rulesmd.ini".into(), vec![3])])
            .unwrap();
        let file = vfs.get("RULESMD.INI").unwrap().unwrap();
        assert_eq!(file.source, "mod");
        assert_eq!(file.bytes, &[3]);
        assert_eq!(vfs.get("maps/test.MAP").unwrap().unwrap().bytes, &[2]);
        assert_eq!(
            vfs.names().collect::<Vec<_>>(),
            vec!["maps/test.map", "rulesmd.ini"]
        );
    }
    #[test]
    fn collisions_and_bad_names_do_not_partially_mount() {
        let mut vfs = Vfs::default();
        vfs.mount("base", vec![("a.ini".into(), vec![1])]).unwrap();
        assert!(
            vfs.mount(
                "bad",
                vec![("a.ini".into(), vec![2]), ("A.INI".into(), vec![3])]
            )
            .is_err()
        );
        assert_eq!(vfs.get("a.ini").unwrap().unwrap().bytes, &[1]);
        assert!(
            vfs.mount(
                "bad",
                vec![("b.ini".into(), vec![]), ("../x".into(), vec![])]
            )
            .is_err()
        );
        assert!(vfs.get("b.ini").unwrap().is_none());
        for name in [
            "", "../a", "/a", "C:\\a", "a//b", "a/./b", "a/../b", " a", "a\0", "a/",
        ] {
            assert!(canonical_name(name).is_err(), "{name:?}");
        }
    }
    #[test]
    fn archive_and_loose_mounts_obey_one_explicit_priority_order() {
        fn archive() -> MixArchive {
            let body = b"archive rules";
            let mut bytes = Vec::new();
            bytes.extend(0_u32.to_le_bytes());
            bytes.extend(1_u16.to_le_bytes());
            bytes.extend((body.len() as u32).to_le_bytes());
            bytes.extend(
                crate::mix::filename_id("rulesmd.ini", FilenameHash::Ra2)
                    .unwrap()
                    .to_le_bytes(),
            );
            bytes.extend(0_u32.to_le_bytes());
            bytes.extend((body.len() as u32).to_le_bytes());
            bytes.extend(body);
            MixArchive::parse(Arc::from(bytes)).unwrap()
        }
        let mut vfs = Vfs::default();
        vfs.mount(
            "base",
            vec![
                ("rulesmd.ini".into(), b"base rules".to_vec()),
                ("中文.map".into(), vec![9]),
            ],
        )
        .unwrap();
        vfs.mount_mix("expand.mix", archive(), FilenameHash::Ra2)
            .unwrap();
        assert_eq!(
            vfs.get("RulesMD.INI").unwrap().unwrap().bytes,
            b"archive rules"
        );
        assert_eq!(vfs.get("中文.map").unwrap().unwrap().bytes, &[9]);
        vfs.mount(
            "loose mod",
            vec![("RULESMD.INI".into(), b"mod rules".to_vec())],
        )
        .unwrap();
        assert_eq!(vfs.get("rulesmd.ini").unwrap().unwrap().bytes, b"mod rules");
        assert_eq!(vfs.get("rulesmd.ini").unwrap().unwrap().source, "loose mod");
    }
    #[test]
    fn loose_directory_mount_is_atomic_when_byte_budget_is_exceeded() {
        let path = std::env::temp_dir().join(format!(
            "ra2ne-vfs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(path.join("Maps")).unwrap();
        std::fs::write(path.join("RulesMD.INI"), b"rules").unwrap();
        std::fs::write(path.join("Maps/test.map"), b"map").unwrap();
        let mut vfs = Vfs::default();
        assert!(vfs.mount_directory("loose", &path, 7).is_err());
        assert!(vfs.get("rulesmd.ini").unwrap().is_none());
        vfs.mount_directory("loose", &path, 8).unwrap();
        assert_eq!(vfs.get("maps/TEST.MAP").unwrap().unwrap().bytes, b"map");
        std::fs::remove_dir_all(path).unwrap();
    }
}
