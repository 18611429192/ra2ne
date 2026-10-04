//! In-memory layered virtual files. Later mounts override earlier mounts.
//! Archive decoding and disk traversal are intentionally separate adapters.
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug)]
pub struct VirtualFile {
    pub source: String,
    pub bytes: Arc<[u8]>,
}
#[derive(Debug, Default)]
pub struct Vfs {
    files: BTreeMap<String, VirtualFile>,
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
        self.files.extend(staged);
        Ok(())
    }
    pub fn get(&self, name: &str) -> Result<Option<&VirtualFile>, &'static str> {
        Ok(self.files.get(&canonical_name(name)?))
    }
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
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
        assert_eq!(file.bytes.as_ref(), &[3]);
        assert_eq!(
            vfs.get("maps/test.MAP").unwrap().unwrap().bytes.as_ref(),
            &[2]
        );
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
        assert_eq!(vfs.get("a.ini").unwrap().unwrap().bytes.as_ref(), &[1]);
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
}
