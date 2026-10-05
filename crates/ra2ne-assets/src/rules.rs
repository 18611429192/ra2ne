//! Typed rule discovery and explicit key-level overlays. This frontend does not
//! silently discard unknown MOD properties or equate parsing with gameplay.
use crate::ini::{Entry, Ini};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixedDecimal(pub i64);
impl FixedDecimal {
    pub const SCALE: i64 = 1000;
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        let value = value.trim();
        let (negative, value) = if let Some(v) = value.strip_prefix('-') {
            (true, v)
        } else {
            (false, value.strip_prefix('+').unwrap_or(value))
        };
        let mut parts = value.split('.');
        let whole = parts.next().unwrap();
        let fraction = parts.next().unwrap_or("");
        if parts.next().is_some()
            || (whole.is_empty() && fraction.is_empty())
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || fraction.len() > 3
            || !fraction.bytes().all(|b| b.is_ascii_digit())
        {
            return Err("decimal requires at most three fractional digits");
        }
        let whole: i64 = if whole.is_empty() {
            0
        } else {
            whole.parse().map_err(|_| "decimal overflow")?
        };
        let fraction: i64 = if fraction.is_empty() {
            0
        } else {
            fraction.parse().map_err(|_| "invalid decimal fraction")?
        };
        let magnitude = whole
            .checked_mul(Self::SCALE)
            .and_then(|v| v.checked_add(fraction * 10_i64.pow(3 - fraction_digits(value))))
            .ok_or("decimal overflow")?;
        Ok(Self(if negative { -magnitude } else { magnitude }))
    }
}
fn fraction_digits(value: &str) -> u32 {
    value.split_once('.').map_or(0, |(_, v)| v.len() as u32)
}

#[derive(Debug)]
pub struct RuleLayer {
    pub source: String,
    pub ini: Ini,
}
#[derive(Clone, Copy, Debug)]
pub struct RuleValue<'a> {
    pub source: &'a str,
    pub entry: &'a Entry,
}
#[derive(Debug, Default)]
pub struct RuleSet {
    layers: Vec<RuleLayer>,
    sections: BTreeMap<String, EffectiveSection>,
}
#[derive(Debug, Default)]
struct EffectiveSection {
    order: Vec<String>,
    values: BTreeMap<String, (usize, usize)>,
}
impl RuleSet {
    pub fn add_layer(&mut self, source: &str, text: &str) -> Result<(), &'static str> {
        if source.is_empty() {
            return Err("empty rule source");
        }
        let ini = Ini::parse(text)?;
        let layer_id = self.layers.len();
        for (index, entry) in ini.entries().iter().enumerate() {
            let section = self
                .sections
                .entry(entry.section.to_ascii_lowercase())
                .or_default();
            let key = entry.key.to_ascii_lowercase();
            if section
                .values
                .insert(key.clone(), (layer_id, index))
                .is_none()
            {
                section.order.push(key);
            }
        }
        self.layers.push(RuleLayer {
            source: source.to_owned(),
            ini,
        });
        Ok(())
    }
    fn value_at(&self, (layer, index): (usize, usize)) -> RuleValue<'_> {
        RuleValue {
            source: &self.layers[layer].source,
            entry: &self.layers[layer].ini.entries()[index],
        }
    }
    pub fn get(&self, section: &str, key: &str) -> Option<RuleValue<'_>> {
        self.sections
            .get(&section.to_ascii_lowercase())?
            .values
            .get(&key.to_ascii_lowercase())
            .map(|&at| self.value_at(at))
    }
    /// Existing key positions stay stable across overrides; new keys append in
    /// layer/source order. All original layers remain available for diagnostics.
    pub fn section(&self, section: &str) -> Vec<RuleValue<'_>> {
        self.sections
            .get(&section.to_ascii_lowercase())
            .map_or_else(Vec::new, |section| {
                section
                    .order
                    .iter()
                    .map(|key| self.value_at(section.values[key]))
                    .collect()
            })
    }
    pub fn layers(&self) -> &[RuleLayer] {
        &self.layers
    }
    /// Strict compilation remains available for the experimental game importer.
    pub fn load(&self) -> Result<RuleCatalog, String> {
        self.load_mode(false)
    }
    /// Inspect the whole source even when some registered placeholders cannot
    /// compile. No health/weapon defaults are invented; failures remain explicit.
    pub fn discover(&self) -> Result<RuleCatalog, String> {
        self.load_mode(true)
    }
    fn load_mode(&self, discovery: bool) -> Result<RuleCatalog, String> {
        let mut types = Vec::new();
        let mut ids = BTreeSet::new();
        let mut diagnostics = Vec::new();
        let mut incomplete_types = Vec::new();
        let mut incomplete_weapons = Vec::new();
        for layer in &self.layers {
            for d in &layer.ini.diagnostics {
                diagnostics.push(RuleDiagnostic {
                    source: layer.source.clone(),
                    line: d.line,
                    section: "INI".into(),
                    key: String::new(),
                    message: d.message.into(),
                });
            }
        }
        for (registry, kind) in [
            ("VehicleTypes", TypeKind::Vehicle),
            ("InfantryTypes", TypeKind::Infantry),
            ("AircraftTypes", TypeKind::Aircraft),
            ("BuildingTypes", TypeKind::Building),
        ] {
            for value in self.section(registry) {
                let id = value.entry.value.trim();
                if id.is_empty() || !id.is_ascii() || id.contains([',', '[', ']']) {
                    return Err(format!(
                        "{}:{}: invalid registered type ID",
                        value.source, value.entry.line
                    ));
                }
                if !ids.insert(id.to_ascii_lowercase()) {
                    if discovery {
                        diagnostics.push(RuleDiagnostic {
                            source: value.source.to_owned(), line: value.entry.line,
                            section: registry.to_owned(), key: value.entry.key.clone(),
                            message: format!("duplicate registered type {id}; first registry position retained for discovery"),
                        });
                        continue;
                    }
                    return Err(format!(
                        "{}:{}: duplicate registered type {id}",
                        value.source, value.entry.line
                    ));
                }
                let parsed = (|| -> Result<TypeRule, String> {
                    let strength = required_integer(self, id, "Strength")?;
                    if strength <= 0 {
                        return Err(format!("{id}: Strength must be positive"));
                    }
                    let speed = optional_integer(self, id, "Speed", 0)?;
                    if speed < 0 {
                        return Err(format!("{id}: negative Speed"));
                    }
                    let sight = optional_decimal(self, id, "Sight", FixedDecimal(0))?;
                    if sight.0 < 0 {
                        return Err(format!("{id}: negative Sight"));
                    }
                    let primary = optional_string(self, id, "Primary");
                    let secondary = optional_string(self, id, "Secondary");
                    let prerequisites = self
                        .get(id, "Prerequisite")
                        .map_or_else(Vec::new, |v| v.entry.list().map(str::to_owned).collect());
                    let owners = self
                        .get(id, "Owner")
                        .map_or_else(Vec::new, |v| v.entry.list().map(str::to_owned).collect());
                    let properties = self
                        .section(id)
                        .into_iter()
                        .map(|v| (v.entry.key.to_ascii_lowercase(), v.entry.value.clone()))
                        .collect();
                    Ok(TypeRule {
                        id: id.to_owned(),
                        kind,
                        strength: strength as u32,
                        speed: speed as u32,
                        cost: optional_integer(self, id, "Cost", 0)?,
                        sight,
                        primary,
                        secondary,
                        prerequisites,
                        owners,
                        image: optional_string(self, id, "Image"),
                        name: optional_string(self, id, "Name"),
                        armor: optional_string(self, id, "Armor"),
                        properties,
                    })
                })();
                match parsed {
                    Ok(unit) => types.push(unit),
                    Err(message) if discovery => incomplete_types.push(RuleDiagnostic {
                        source: value.source.to_owned(),
                        line: value.entry.line,
                        section: id.to_owned(),
                        key: String::new(),
                        message,
                    }),
                    Err(message) => return Err(message),
                }
                collect_unknown(
                    self,
                    id,
                    &[
                        "Strength",
                        "Speed",
                        "Cost",
                        "Sight",
                        "Primary",
                        "Secondary",
                        "Prerequisite",
                        "Owner",
                        "Image",
                        "Name",
                        "Armor",
                    ],
                    &mut diagnostics,
                );
            }
        }
        let mut weapons = BTreeMap::new();
        for unit in &types {
            for id in unit.primary.iter().chain(&unit.secondary) {
                let key = id.to_ascii_lowercase();
                if weapons.contains_key(&key) {
                    continue;
                }
                if incomplete_weapons
                    .iter()
                    .any(|d: &RuleDiagnostic| d.section.eq_ignore_ascii_case(id))
                {
                    continue;
                }
                let parsed = (|| -> Result<WeaponRule, String> {
                    let damage = required_integer(self, id, "Damage")?;
                    let rof = required_integer(self, id, "ROF")?;
                    let range = optional_decimal(self, id, "Range", FixedDecimal(0))?;
                    if rof < 0 || range.0 < 0 {
                        return Err(format!("{id}: negative ROF or Range"));
                    }
                    Ok(WeaponRule {
                        id: id.clone(),
                        damage,
                        rof: rof as u32,
                        range,
                        warhead: optional_string(self, id, "Warhead"),
                        projectile: optional_string(self, id, "Projectile"),
                    })
                })();
                match parsed {
                    Ok(weapon) => {
                        weapons.insert(key, weapon);
                    }
                    Err(message) if discovery => {
                        let key = if unit
                            .primary
                            .as_ref()
                            .is_some_and(|primary| primary.eq_ignore_ascii_case(id))
                        {
                            "Primary"
                        } else {
                            "Secondary"
                        };
                        let reference = self.get(&unit.id, key).unwrap();
                        incomplete_weapons.push(RuleDiagnostic {
                            source: reference.source.to_owned(),
                            line: reference.entry.line,
                            section: id.to_owned(),
                            key: String::new(),
                            message,
                        });
                    }
                    Err(message) => return Err(message),
                }
                collect_unknown(
                    self,
                    id,
                    &["Damage", "ROF", "Range", "Warhead", "Projectile"],
                    &mut diagnostics,
                );
            }
        }
        let parsed_sections: BTreeSet<_> = types
            .iter()
            .map(|t| t.id.to_ascii_lowercase())
            .chain(weapons.keys().cloned())
            .chain(
                [
                    "vehicletypes",
                    "infantrytypes",
                    "aircrafttypes",
                    "buildingtypes",
                ]
                .map(str::to_owned),
            )
            .collect();
        let sections: BTreeSet<_> = self
            .layers
            .iter()
            .flat_map(|l| {
                l.ini
                    .entries()
                    .iter()
                    .map(|e| e.section.to_ascii_lowercase())
            })
            .collect();
        for section in sections.difference(&parsed_sections) {
            for v in self.section(section) {
                diagnostics.push(RuleDiagnostic {
                    source: v.source.to_owned(),
                    line: v.entry.line,
                    section: v.entry.section.clone(),
                    key: v.entry.key.clone(),
                    message: "property preserved; typed/runtime semantics pending".into(),
                });
            }
        }
        let type_index = types
            .iter()
            .enumerate()
            .map(|(index, t)| (t.id.to_ascii_lowercase(), index))
            .collect();
        Ok(RuleCatalog {
            types,
            weapons,
            diagnostics,
            type_index,
            registered_type_count: ids.len(),
            incomplete_types,
            incomplete_weapons,
        })
    }
}
fn optional_string(rules: &RuleSet, section: &str, key: &str) -> Option<String> {
    rules.get(section, key).and_then(|v| {
        (!v.entry.value.is_empty() && !v.entry.value.eq_ignore_ascii_case("none"))
            .then(|| v.entry.value.clone())
    })
}
fn required_integer(rules: &RuleSet, section: &str, key: &str) -> Result<i32, String> {
    let v = rules
        .get(section, key)
        .ok_or_else(|| format!("[{section}] missing {key}"))?;
    v.entry
        .integer()
        .map_err(|e| format!("{}:{}: [{section}] {key}: {e}", v.source, v.entry.line))
}
fn optional_integer(
    rules: &RuleSet,
    section: &str,
    key: &str,
    default: i32,
) -> Result<i32, String> {
    if rules.get(section, key).is_some() {
        required_integer(rules, section, key)
    } else {
        Ok(default)
    }
}
fn optional_decimal(
    rules: &RuleSet,
    section: &str,
    key: &str,
    default: FixedDecimal,
) -> Result<FixedDecimal, String> {
    rules.get(section, key).map_or(Ok(default), |v| {
        FixedDecimal::parse(&v.entry.value)
            .map_err(|e| format!("{}:{}: [{section}] {key}: {e}", v.source, v.entry.line))
    })
}
fn collect_unknown(rules: &RuleSet, section: &str, known: &[&str], out: &mut Vec<RuleDiagnostic>) {
    for v in rules.section(section) {
        if !known
            .iter()
            .any(|key| key.eq_ignore_ascii_case(&v.entry.key))
        {
            out.push(RuleDiagnostic {
                source: v.source.to_owned(),
                line: v.entry.line,
                section: v.entry.section.clone(),
                key: v.entry.key.clone(),
                message: "property preserved; typed/runtime semantics pending".into(),
            });
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TypeKind {
    Vehicle,
    Infantry,
    Aircraft,
    Building,
}
#[derive(Clone, Debug)]
pub struct TypeRule {
    pub id: String,
    pub kind: TypeKind,
    pub strength: u32,
    pub speed: u32,
    pub cost: i32,
    pub sight: FixedDecimal,
    pub primary: Option<String>,
    pub secondary: Option<String>,
    pub prerequisites: Vec<String>,
    pub owners: Vec<String>,
    pub image: Option<String>,
    pub name: Option<String>,
    pub armor: Option<String>,
    pub properties: BTreeMap<String, String>,
}
#[derive(Clone, Debug)]
pub struct WeaponRule {
    pub id: String,
    pub damage: i32,
    pub rof: u32,
    pub range: FixedDecimal,
    pub warhead: Option<String>,
    pub projectile: Option<String>,
}
#[derive(Clone, Debug)]
pub struct RuleDiagnostic {
    pub source: String,
    pub line: usize,
    pub section: String,
    pub key: String,
    pub message: String,
}
#[derive(Debug)]
pub struct RuleCatalog {
    pub registered_type_count: usize,
    pub incomplete_types: Vec<RuleDiagnostic>,
    pub incomplete_weapons: Vec<RuleDiagnostic>,
    pub types: Vec<TypeRule>,
    pub weapons: BTreeMap<String, WeaponRule>,
    pub diagnostics: Vec<RuleDiagnostic>,
    type_index: BTreeMap<String, usize>,
}
impl RuleCatalog {
    pub fn type_by_id(&self, id: &str) -> Option<&TypeRule> {
        self.type_index
            .get(&id.to_ascii_lowercase())
            .map(|&index| &self.types[index])
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlays_merge_keys_preserve_registry_order_and_source() {
        let mut rules = RuleSet::default();
        rules.add_layer("rules.ini","[VehicleTypes]\n2=TANK\n10=SCOUT\n[TANK]\nStrength=100\nSpeed=5\nCost=500\nPrimary=CANNON\nUnknown.Mod.Flag=yes\n[SCOUT]\nStrength=80\n[CANNON]\nDamage=20\nROF=10\nRange=5.5\nWarhead=AP\n[AP]\nVerses=100%,100%\n").unwrap();
        rules
            .add_layer("map.ini", "[TANK]\nStrength=200\nCost=700\n")
            .unwrap();
        let catalog = rules.load().unwrap();
        assert_eq!(
            catalog
                .types
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            vec!["TANK", "SCOUT"]
        );
        let tank = catalog.type_by_id("tank").unwrap();
        assert_eq!(tank.strength, 200);
        assert_eq!(tank.speed, 5);
        assert_eq!(tank.cost, 700);
        assert_eq!(rules.get("TANK", "Strength").unwrap().source, "map.ini");
        assert_eq!(catalog.weapons["cannon"].range, FixedDecimal(5500));
        assert!(
            catalog
                .diagnostics
                .iter()
                .any(|d| d.key == "Unknown.Mod.Flag")
        );
        assert!(catalog.diagnostics.iter().any(|d| d.key == "Verses"));
        assert_eq!(tank.properties["unknown.mod.flag"], "yes");
    }
    #[test]
    fn fixed_decimals_do_not_use_float_or_accept_ambiguous_precision() {
        for (text, n) in [
            ("1", 1000),
            ("0.125", 125),
            (".5", 500),
            ("-1.5", -1500),
            ("+3.00", 3000),
            ("0", 0),
        ] {
            assert_eq!(FixedDecimal::parse(text), Ok(FixedDecimal(n)));
        }
        for text in [
            ".",
            "1.0001",
            "1e3",
            "NaN",
            "1.2.3",
            "-",
            "9223372036854775807",
        ] {
            assert!(FixedDecimal::parse(text).is_err());
        }
    }
    #[test]
    fn invalid_types_and_missing_weapons_are_explicit_errors() {
        for text in [
            "[VehicleTypes]\n0=TANK\n",
            "[VehicleTypes]\n0=TANK\n[TANK]\nStrength=-1\n",
            "[VehicleTypes]\n0=TANK\n[TANK]\nStrength=100\nPrimary=MISSING\n",
            "[VehicleTypes]\n0=TANK\n1=tank\n[TANK]\nStrength=100\n",
        ] {
            let mut rules = RuleSet::default();
            rules.add_layer("bad.ini", text).unwrap();
            assert!(rules.load().is_err());
        }
    }
    #[test]
    fn discovery_keeps_valid_types_while_reporting_placeholders_and_bad_weapons() {
        let mut rules = RuleSet::default();
        rules.add_layer("fixture.ini", "[VehicleTypes]\n0=TANK\n1=DUMMY\n2=tank\n[TANK]\nStrength=100\nPrimary=GUN\nSecondary=BAD\n[DUMMY]\nPrimary=MISSING\n[GUN]\nDamage=20\nROF=10\nRange=5\n[BAD]\nDamage=5\n").unwrap();
        assert!(rules.load().is_err());
        let discovered = rules.discover().unwrap();
        assert_eq!(discovered.registered_type_count, 2);
        assert_eq!(discovered.types.len(), 1);
        assert_eq!(discovered.type_by_id("tank").unwrap().strength, 100);
        assert!(discovered.type_by_id("dummy").is_none());
        assert_eq!(discovered.incomplete_types.len(), 1);
        assert_eq!(discovered.incomplete_types[0].section, "DUMMY");
        assert_eq!(discovered.incomplete_weapons.len(), 1);
        assert_eq!(discovered.incomplete_weapons[0].section, "BAD");
        assert_eq!(
            discovered.incomplete_weapons[0].line,
            rules.get("TANK", "Secondary").unwrap().entry.line
        );
        assert!(
            discovered
                .diagnostics
                .iter()
                .any(|d| d.message.contains("duplicate registered"))
        );
        assert!(rules.get("DUMMY", "Primary").is_some());
        assert_eq!(discovered.weapons.len(), 1);
    }
}
