//! Ordered INI syntax frontend with source locations and ASCII-insensitive lookup.
//! Unknown sections/keys remain available for a future rules schema. Parsing
//! successfully does not mean a game feature has been implemented.
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    pub section: String,
    pub key: String,
    pub value: String,
    pub line: usize,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub line: usize,
    pub message: &'static str,
}
#[derive(Debug, Default)]
pub struct Ini {
    entries: Vec<Entry>,
    lookup: BTreeMap<(String, String), usize>,
    pub diagnostics: Vec<Diagnostic>,
}
impl Ini {
    /// UTF-8/ASCII input only for now. Legacy codepage decoding belongs before
    /// this stage. Semicolon starts a comment; standalone and section-header //
    /// annotations are accepted without stripping // from values.
    /// Repeated sections merge, duplicate keys use the last value for lookup,
    /// while every original entry remains in source order for list processing.
    pub fn parse(text: &str) -> Result<Self, &'static str> {
        if text.len() > 16 * 1024 * 1024 || text.contains('\0') {
            return Err("INI input exceeds limits or contains NUL");
        }
        let mut result = Self::default();
        let mut section = None::<String>;
        for (index, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
            let line = index + 1;
            let text = raw.split(';').next().unwrap().trim();
            if text.is_empty() || text.starts_with("//") {
                continue;
            }
            if text.starts_with('[') {
                // Original rules include [Section] // trailing annotations.
                // Do not strip // from values such as HTTP URLs or file names.
                let text = text.split_once(']').map_or(text, |(name, tail)| {
                    if tail.trim_start().starts_with("//") {
                        &text[..name.len() + 1]
                    } else {
                        text
                    }
                });
                if !text.ends_with(']')
                    || text.len() <= 2
                    || text[1..text.len() - 1].contains(['[', ']'])
                {
                    result.diagnostics.push(Diagnostic {
                        line,
                        message: "malformed section header",
                    });
                    section = None;
                    continue;
                }
                let name = text[1..text.len() - 1].trim();
                if name.is_empty() {
                    result.diagnostics.push(Diagnostic {
                        line,
                        message: "empty section name",
                    });
                    section = None;
                } else {
                    section = Some(name.to_owned());
                }
                continue;
            }
            let Some((key, value)) = text.split_once('=') else {
                result.diagnostics.push(Diagnostic {
                    line,
                    message: "expected key=value",
                });
                continue;
            };
            let key = key.trim();
            if key.is_empty() {
                result.diagnostics.push(Diagnostic {
                    line,
                    message: "empty key",
                });
                continue;
            }
            let Some(section) = &section else {
                result.diagnostics.push(Diagnostic {
                    line,
                    message: "entry outside a valid section",
                });
                continue;
            };
            let id = (section.to_ascii_lowercase(), key.to_ascii_lowercase());
            if result.lookup.insert(id, result.entries.len()).is_some() {
                result.diagnostics.push(Diagnostic {
                    line,
                    message: "duplicate key overrides earlier value",
                });
            }
            result.entries.push(Entry {
                section: section.clone(),
                key: key.to_owned(),
                value: value.trim().to_owned(),
                line,
            });
        }
        Ok(result)
    }
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    pub fn get(&self, section: &str, key: &str) -> Option<&Entry> {
        self.lookup
            .get(&(section.to_ascii_lowercase(), key.to_ascii_lowercase()))
            .map(|&id| &self.entries[id])
    }
    /// Source order matters for indexed registries; do not sort keys lexically.
    pub fn section_entries<'a>(&'a self, section: &'a str) -> impl Iterator<Item = &'a Entry> {
        self.entries
            .iter()
            .filter(move |e| e.section.eq_ignore_ascii_case(section))
    }
}
impl Entry {
    pub fn integer(&self) -> Result<i32, &'static str> {
        self.value.parse().map_err(|_| "invalid decimal integer")
    }
    pub fn boolean(&self) -> Result<bool, &'static str> {
        match self.value.to_ascii_lowercase().as_str() {
            "yes" | "true" | "1" => Ok(true),
            "no" | "false" | "0" => Ok(false),
            _ => Err("invalid boolean"),
        }
    }
    pub fn list(&self) -> impl Iterator<Item = &str> {
        self.value
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_rules_style_syntax_without_reordering_registries() {
        let ini = Ini::parse("\u{feff}; synthetic fixture\r\n[VehicleTypes]\r\n2=TANK\r\n10=SCOUT ; comment\r\n[TANK]\r\nStrength=100\r\nTracked=yes\r\nPrerequisite=FACTORY, RADAR\r\nName=Tank=a\r\nEmpty=\r\n").unwrap();
        assert!(ini.diagnostics.is_empty());
        assert_eq!(
            ini.section_entries("vehicletypes")
                .map(|e| e.key.as_str())
                .collect::<Vec<_>>(),
            vec!["2", "10"]
        );
        assert_eq!(ini.get("tank", "strength").unwrap().integer(), Ok(100));
        assert_eq!(ini.get("TANK", "Tracked").unwrap().boolean(), Ok(true));
        assert_eq!(
            ini.get("tank", "prerequisite")
                .unwrap()
                .list()
                .collect::<Vec<_>>(),
            vec!["FACTORY", "RADAR"]
        );
        assert_eq!(ini.get("tank", "name").unwrap().value, "Tank=a");
        assert_eq!(ini.get("tank", "empty").unwrap().value, "");
    }
    #[test]
    fn repeated_sections_merge_and_duplicate_values_keep_provenance() {
        let ini =
            Ini::parse("[TANK]\nStrength=100\n[tank]\nstrength=200\nUnknown.Mod.Key=abc").unwrap();
        assert_eq!(ini.get("Tank", "Strength").unwrap().integer(), Ok(200));
        assert_eq!(ini.get("Tank", "Strength").unwrap().line, 4);
        assert_eq!(ini.entries().len(), 3);
        assert_eq!(ini.diagnostics[0].line, 4);
        assert!(ini.get("tank", "Unknown.Mod.Key").is_some());
    }
    #[test]
    fn invalid_sections_do_not_leak_keys_to_prior_section() {
        let ini = Ini::parse("a=b\n[Good]\nx=1\n[Broken\ny=2\n[]\nz=3\n[Next]\n=empty\nmissing_equals\nn=999999999999\nb=maybe").unwrap();
        assert_eq!(ini.diagnostics.len(), 7);
        assert!(ini.get("Good", "y").is_none());
        assert!(ini.get("Next", "n").unwrap().integer().is_err());
        assert!(ini.get("Next", "b").unwrap().boolean().is_err());
        assert!(Ini::parse("\0").is_err());
    }
    #[test]
    fn section_slash_comments_do_not_discard_values_or_relax_broken_headers() {
        let ini = Ini::parse("// note\n[Heat] // annotation\nDamage=30\nURL=https://example.invalid/path\n[Broken] garbage\nx=9\n[Next]\ny=7").unwrap();
        assert_eq!(ini.get("Heat", "Damage").unwrap().integer(), Ok(30));
        assert_eq!(
            ini.get("Heat", "URL").unwrap().value,
            "https://example.invalid/path"
        );
        assert!(ini.get("Heat", "x").is_none());
        assert_eq!(ini.diagnostics.len(), 2);
    }
}
