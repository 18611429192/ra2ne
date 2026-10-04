//! Explicit experimental bridge from layered INI data to engine definitions.
//! Conversion requires caller supplied timing/movement calibration. It does not
//! establish original-game semantics; every omitted behavior is reported.
use crate::{Armor, ProductionCategory, ProductionRules, Rules, UnitDef, Verses, Weapon};
use ra2ne_assets::rules::{RuleSet, TypeKind};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct ImportPolicy {
    /// Engine movement speed for each original Speed value. No implicit clamp.
    pub speed_table: BTreeMap<u32, i32>,
    /// Explicit engine build duration until original production is implemented.
    pub build_ticks: u32,
    /// ceil(original ROF * numerator / denominator), minimum one engine Tick.
    pub rof_numerator: u32,
    pub rof_denominator: u32,
    pub max_entities: usize,
}

#[derive(Debug)]
pub struct ImportedRules {
    pub rules: Rules,
    pub type_indices: BTreeMap<String, usize>,
    /// Nonempty warnings must be presented before using these experimental rules.
    pub diagnostics: Vec<String>,
}

pub fn import(rules: &RuleSet, policy: &ImportPolicy) -> Result<ImportedRules, String> {
    if policy.rof_numerator == 0 || policy.rof_denominator == 0 || policy.build_ticks == 0 {
        return Err("rule import requires positive explicit timing calibration".into());
    }
    let catalog = rules.load()?;
    let mut warheads = BTreeMap::new();
    for weapon in catalog.weapons.values() {
        if let Some(id) = &weapon.warhead {
            let value = rules
                .get(id, "Verses")
                .ok_or_else(|| format!("[{id}]: referenced Warhead requires explicit Verses"))?;
            let verses = Verses::parse(&value.entry.value).map_err(|e| {
                format!("{}:{}: [{id}] Verses: {e}", value.source, value.entry.line)
            })?;
            warheads.insert(id.to_ascii_lowercase(), verses);
        }
    }
    let mut diagnostics: Vec<_> = catalog
        .diagnostics
        .iter()
        .filter(|d| {
            !(warheads.contains_key(&d.section.to_ascii_lowercase())
                && d.key.eq_ignore_ascii_case("Verses")
                || catalog.type_by_id(&d.section).is_some()
                    && ["Power", "Harvester", "Factory", "Naval"]
                        .iter()
                        .any(|k| k.eq_ignore_ascii_case(&d.key)))
        })
        .map(|d| {
            format!(
                "{}:{}: [{}] {}: {}",
                d.source, d.line, d.section, d.key, d.message
            )
        })
        .collect();
    diagnostics.push("experimental rule import: production timing and movement use caller calibration; original gameplay compatibility is unverified".into());
    let mut units = Vec::with_capacity(catalog.types.len());
    let type_indices: BTreeMap<_, _> = catalog
        .types
        .iter()
        .enumerate()
        .map(|(i, t)| (t.id.to_ascii_lowercase(), i))
        .collect();
    for unit in &catalog.types {
        if unit.kind == TypeKind::Aircraft {
            return Err(format!("[{}]: aircraft movement is unsupported", unit.id));
        }
        let speed = if unit.kind == TypeKind::Building {
            0
        } else {
            *policy.speed_table.get(&unit.speed).ok_or_else(|| {
                format!(
                    "[{}]: Speed={} has no engine calibration",
                    unit.id, unit.speed
                )
            })?
        };
        let cost = u32::try_from(unit.cost)
            .map_err(|_| format!("[{}]: negative Cost cannot be imported", unit.id))?;
        let mut compile_weapon = |id: &String| -> Result<Weapon, String> {
            let source = catalog
                .weapons
                .get(&id.to_ascii_lowercase())
                .ok_or_else(|| format!("[{id}]: missing weapon"))?;
            let damage = u32::try_from(source.damage)
                .map_err(|_| format!("[{id}]: healing weapons are unsupported"))?;
            // Range is in thousandths of a cell; engine range is whole cells.
            // Reject fractional values instead of silently changing combat reach.
            if source.range.0 <= 0 || source.range.0 % 1000 != 0 {
                return Err(format!(
                    "[{id}]: Range must be positive whole cells for this engine"
                ));
            }
            let range = u32::try_from(source.range.0 / 1000)
                .map_err(|_| format!("[{id}]: Range overflow"))?;
            let reload_ticks = u64::from(source.rof) * u64::from(policy.rof_numerator);
            let reload_ticks = reload_ticks
                .div_ceil(u64::from(policy.rof_denominator))
                .max(1);
            let reload_ticks = u32::try_from(reload_ticks)
                .map_err(|_| format!("[{id}]: calibrated ROF overflow"))?;
            if source.projectile.is_some() {
                diagnostics.push(format!(
                    "[{id}]: Projectile behavior is not applied; immediate direct damage only"
                ));
            }
            let verses = source
                .warhead
                .as_ref()
                .map_or(Verses::default(), |id| warheads[&id.to_ascii_lowercase()]);
            Ok(Weapon {
                verses,
                damage,
                range,
                reload_ticks,
            })
        };
        let weapon = unit.primary.as_ref().map(&mut compile_weapon).transpose()?;
        let secondary = unit
            .secondary
            .as_ref()
            .map(&mut compile_weapon)
            .transpose()?;
        for (key, present) in [
            ("Owner", !unit.owners.is_empty()),
            ("Sight", unit.sight.0 != 0),
            ("Image", unit.image.is_some()),
            ("Name", unit.name.is_some()),
        ] {
            if present {
                diagnostics.push(format!(
                    "[{}]: {key} is preserved but not applied by the game",
                    unit.id
                ));
            }
        }
        let power = rules.get(&unit.id, "Power").map_or(Ok(0), |v| {
            v.entry
                .integer()
                .map_err(|e| format!("{}:{}: [{}] Power: {e}", v.source, v.entry.line, unit.id))
        })?;
        let harvester = rules.get(&unit.id, "Harvester").map_or(Ok(false), |v| {
            v.entry.boolean().map_err(|e| {
                format!(
                    "{}:{}: [{}] Harvester: {e}",
                    v.source, v.entry.line, unit.id
                )
            })
        })?;
        if harvester {
            diagnostics.push(format!(
                "[{}]: harvesting uses synthetic capacity/deposit timings",
                unit.id
            ));
        }
        if unit.kind == TypeKind::Building {
            diagnostics.push(format!(
                "[{}]: building footprint/construction behavior is not implemented",
                unit.id
            ));
        }
        let naval = rules.get(&unit.id, "Naval").map_or(Ok(false), |v| {
            v.entry
                .boolean()
                .map_err(|e| format!("{}:{}: [{}] Naval: {e}", v.source, v.entry.line, unit.id))
        })?;
        if naval {
            return Err(format!(
                "[{}]: naval movement/production is unsupported",
                unit.id
            ));
        }
        let category = match unit.kind {
            TypeKind::Vehicle => ProductionCategory::Vehicle,
            TypeKind::Infantry => ProductionCategory::Infantry,
            TypeKind::Aircraft => ProductionCategory::Aircraft,
            TypeKind::Building => ProductionCategory::Building,
        };
        let factory_category = rules
            .get(&unit.id, "Factory")
            .map(
                |v| match v.entry.value.trim().to_ascii_lowercase().as_str() {
                    "unittype" => Ok(Some(ProductionCategory::Vehicle)),
                    "infantrytype" => Ok(Some(ProductionCategory::Infantry)),
                    "aircrafttype" => Ok(Some(ProductionCategory::Aircraft)),
                    "buildingtype" => Ok(Some(ProductionCategory::Building)),
                    "none" => Ok(None),
                    _ => Err(format!(
                        "{}:{}: [{}] unknown Factory category",
                        v.source, v.entry.line, unit.id
                    )),
                },
            )
            .transpose()?
            .flatten();
        if factory_category.is_some() && unit.kind != TypeKind::Building {
            return Err(format!("[{}]: only buildings can be factories", unit.id));
        }
        let mut prerequisites = Vec::new();
        if unit.prerequisites.len() > 1024 {
            return Err(format!("[{}]: prerequisite limit exceeded", unit.id));
        }
        for id in &unit.prerequisites {
            let key = match id.to_ascii_lowercase().as_str() {
                "power" => Some("PrerequisitePower"),
                "factory" => Some("PrerequisiteFactory"),
                "barracks" => Some("PrerequisiteBarracks"),
                "radar" => Some("PrerequisiteRadar"),
                "tech" => Some("PrerequisiteTech"),
                "proc" => Some("PrerequisiteProc"),
                _ => None,
            };
            let generic = rules.get("GenericPrerequisites", id);
            let value =
                if let Some(v) = generic {
                    Some(v)
                } else if let Some(key) = key {
                    Some(rules.get("General", key).ok_or_else(|| {
                        format!("[{}]: alias {id} requires [General] {key}", unit.id)
                    })?)
                } else {
                    None
                };
            if id.eq_ignore_ascii_case("proc")
                && rules
                    .get("General", "PrerequisiteProcAlternate")
                    .is_some_and(|v| {
                        !v.entry.value.trim().is_empty()
                            && !v.entry.value.eq_ignore_ascii_case("none")
                    })
            {
                return Err(format!(
                    "[{}]: PROC non-building alternate prerequisite is unsupported",
                    unit.id
                ));
            }
            let members: Vec<_> = if let Some(v) = value {
                let prefix = format!(
                    "{}:{}: [{}] {}:",
                    v.source, v.entry.line, v.entry.section, v.entry.key
                );
                diagnostics.retain(|d| !d.starts_with(&prefix));
                v.entry.list().collect()
            } else {
                vec![id.as_str()]
            };
            if members.is_empty() || members.len() > 1024 {
                return Err(format!(
                    "[{}]: empty or oversized prerequisite group {id}",
                    unit.id
                ));
            }
            let mut group = Vec::with_capacity(members.len());
            for member in members {
                let index = *type_indices
                    .get(&member.to_ascii_lowercase())
                    .ok_or_else(|| {
                        format!("[{}]: unresolved prerequisite member {member}", unit.id)
                    })?;
                if catalog.types[index].kind != TypeKind::Building {
                    return Err(format!(
                        "[{}]: prerequisite {member} is not a building",
                        unit.id
                    ));
                }
                group.push(index);
            }
            group.sort_unstable();
            group.dedup();
            prerequisites.push(group);
        }
        prerequisites.sort_unstable();
        prerequisites.dedup();
        let armor = unit
            .armor
            .as_ref()
            .map_or(Ok(Armor::None), |s| Armor::parse(s))
            .map_err(|e| format!("[{}]: Armor: {e}", unit.id))?;
        units.push(UnitDef {
            production: Some(ProductionRules {
                category,
                factory_category,
                prerequisites,
            }),
            armor,
            name: unit.id.clone(),
            health: unit.strength,
            speed,
            cost,
            weapon,
            secondary,
            build_ticks: policy.build_ticks,
            power,
            factory: factory_category.is_some(),
            harvester,
        });
    }
    let rules = Rules {
        units,
        max_entities: policy.max_entities,
    };
    rules.validate().map_err(str::to_owned)?;
    Ok(ImportedRules {
        rules,
        type_indices,
        diagnostics,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> ImportPolicy {
        ImportPolicy {
            speed_table: BTreeMap::from([(5, 1)]),
            build_ticks: 30,
            rof_numerator: 2,
            rof_denominator: 3,
            max_entities: 100,
        }
    }
    fn fixture() -> RuleSet {
        let mut r = RuleSet::default();
        r.add_layer("rules.ini", "[VehicleTypes]\n0=TANK\n[TANK]\nStrength=100\nSpeed=5\nCost=500\nPrimary=GUN\nArmor=heavy\n[GUN]\nDamage=20\nROF=10\nRange=5\nWarhead=AP\n[AP]\nVerses=100%,100%,100%,100%,100%,50%,100%,100%,100%,100%,100%\n").unwrap();
        r
    }
    #[test]
    fn effective_overlays_compile_with_explicit_omissions() {
        let mut r = fixture();
        r.add_layer("map.ini", "[TANK]\nStrength=250\nCost=700\n")
            .unwrap();
        let result = import(&r, &policy()).unwrap();
        let tank = &result.rules.units[result.type_indices["tank"]];
        assert_eq!((tank.health, tank.cost, tank.speed), (250, 700, 1));
        let gun = tank.weapon.as_ref().unwrap();
        assert_eq!((gun.damage, gun.range, gun.reload_ticks), (20, 5, 7));
        assert_eq!(tank.armor, Armor::Heavy);
        assert_eq!(gun.verses.damage(20, tank.armor), 10);
        assert!(
            !result
                .diagnostics
                .iter()
                .any(|s| s.contains("Verses: property preserved"))
        );
        r.add_layer(
            "warhead-mod.ini",
            "[AP]\nVerses=100%,100%,100%,100%,100%,150%,100%,100%,100%,100%,100%\n",
        )
        .unwrap();
        let modified = import(&r, &policy()).unwrap();
        assert_eq!(
            modified.rules.units[0]
                .weapon
                .as_ref()
                .unwrap()
                .verses
                .damage(20, Armor::Heavy),
            30
        );
    }
    #[test]
    fn alias_groups_are_or_within_and_between_and_respect_overlays() {
        let mut source = RuleSet::default();
        source
            .add_layer(
                "base.ini",
                include_str!("../../../fixtures/production-experiment.ini"),
            )
            .unwrap();
        source
            .add_layer(
                "groups.ini",
                include_str!("../../../fixtures/prerequisite-groups.ini"),
            )
            .unwrap();
        let imported = import(&source, &policy()).unwrap();
        let indices = imported.type_indices;
        assert_eq!(
            imported.rules.units[indices["testtank"]]
                .production
                .as_ref()
                .unwrap()
                .prerequisites,
            vec![
                vec![indices["testfactory"], indices["testbarracks"]],
                vec![indices["testlab"]]
            ]
        );
        assert_eq!(
            imported.rules.units[indices["testinfantry"]]
                .production
                .as_ref()
                .unwrap()
                .prerequisites,
            vec![vec![indices["testbarracks"], indices["testlab"]]]
        );
        source
            .add_layer("mod.ini", "[GenericPrerequisites]\nFACTORY=TESTLAB\n")
            .unwrap();
        let modified = import(&source, &policy()).unwrap();
        // Both required groups resolve to TESTLAB and canonicalize to one group.
        assert_eq!(
            modified.rules.units[indices["testtank"]]
                .production
                .as_ref()
                .unwrap()
                .prerequisites,
            vec![vec![indices["testlab"]]]
        );
        for text in [
            "[General]\nPrerequisiteTech=\n",
            "[General]\nPrerequisiteTech=MISSING\n",
            "[GenericPrerequisites]\nTRAINING=TESTTANK\n",
            "[GenericPrerequisites]\nTRAINING=TRAINING\n",
            "[TESTTANK]\nPrerequisite=PROC\n[General]\nPrerequisiteProc=TESTLAB\nPrerequisiteProcAlternate=TESTTANK\n",
        ] {
            let mut rules = RuleSet::default();
            rules
                .add_layer(
                    "base.ini",
                    include_str!("../../../fixtures/production-experiment.ini"),
                )
                .unwrap();
            rules
                .add_layer(
                    "groups.ini",
                    include_str!("../../../fixtures/prerequisite-groups.ini"),
                )
                .unwrap();
            rules.add_layer("bad.ini", text).unwrap();
            assert!(import(&rules, &policy()).is_err(), "{text}");
        }
    }
    #[test]
    fn imported_factories_enforce_categories_prerequisites_and_replay() {
        use crate::{Player, Skirmish};
        use ra2ne_core::{Vec2, navigation::NavigationMap};
        use std::sync::Arc;
        let mut source = RuleSet::default();
        source
            .add_layer(
                "production.ini",
                include_str!("../../../fixtures/production-experiment.ini"),
            )
            .unwrap();
        let imported = import(&source, &policy()).unwrap();
        let indices = imported.type_indices;
        let tank = indices["testtank"];
        let infantry = indices["testinfantry"];
        let mut game = Skirmish::new(
            Arc::new(imported.rules),
            NavigationMap::new(20, 20),
            BTreeMap::from([
                (
                    0,
                    Player {
                        credits: 5000,
                        defeated: false,
                    },
                ),
                (
                    1,
                    Player {
                        credits: 5000,
                        defeated: false,
                    },
                ),
            ]),
        )
        .unwrap();
        let factory = game
            .spawn(0, indices["testfactory"], Vec2::new(1, 1))
            .unwrap();
        let barracks = game
            .spawn(0, indices["testbarracks"], Vec2::new(1, 5))
            .unwrap();
        game.spawn(1, indices["testlab"], Vec2::new(15, 15))
            .unwrap();
        let before = game.state_hash();
        assert_eq!(
            game.queue_production(0, factory, tank),
            Err("missing production prerequisite")
        );
        assert_eq!(
            game.queue_production(0, factory, infantry),
            Err("factory cannot produce this category")
        );
        assert_eq!(game.state_hash(), before);
        game.spawn(0, indices["testlab"], Vec2::new(2, 6)).unwrap();
        game.queue_production(0, factory, tank).unwrap();
        assert_eq!(game.players()[&0].credits, 4300);
        game.cancel_production(0, factory, 0).unwrap();
        assert_eq!(game.players()[&0].credits, 5000);
        game.queue_production(0, factory, tank).unwrap();
        game.queue_production(0, barracks, infantry).unwrap();
        let initial = game.save().unwrap();
        let mut restored = Skirmish::load(&initial).unwrap();
        let replay = crate::commands::GameReplay {
            initial,
            ticks: 50,
            commands: vec![],
        };
        for _ in 0..50 {
            game.tick();
            restored.tick();
            assert_eq!(game.state_hash(), restored.state_hash());
            assert_eq!(game.events(), restored.events());
        }
        assert!(game.entities().any(|(_, a)| a.owner == 0 && a.kind == tank));
        assert!(
            game.entities()
                .any(|(_, a)| a.owner == 0 && a.kind == infantry)
        );
        assert!(game.production(factory).is_none());
        assert_eq!(
            crate::commands::GameReplay::decode(&replay.encode().unwrap())
                .unwrap()
                .play(5)
                .unwrap()
                .game
                .state_hash(),
            game.state_hash()
        );
        for overlay in [
            "[TESTTANK]\nPrerequisite=FACTORY\n",
            "[TESTFACTORY]\nFactory=unknown\n",
            "[TESTFACTORY]\nNaval=yes\n",
            "[TESTTANK]\nPrerequisite=TESTINFANTRY\n",
        ] {
            let mut r = RuleSet::default();
            r.add_layer(
                "base.ini",
                include_str!("../../../fixtures/production-experiment.ini"),
            )
            .unwrap();
            r.add_layer("bad.ini", overlay).unwrap();
            assert!(import(&r, &policy()).is_err());
        }
    }
    #[test]
    fn secondary_definitions_and_their_overlays_are_compiled() {
        let mut rules = fixture();
        rules
            .add_layer(
                "secondary.ini",
                "[TANK]\nSecondary=BACKUP\n[BACKUP]\nDamage=7\nROF=12\nRange=3\nWarhead=AP\n",
            )
            .unwrap();
        rules.add_layer("map.ini", "[BACKUP]\nDamage=11\n").unwrap();
        let imported = import(&rules, &policy()).unwrap();
        let secondary = imported.rules.units[0].secondary.as_ref().unwrap();
        assert_eq!(
            (secondary.damage, secondary.range, secondary.reload_ticks),
            (11, 3, 8)
        );
        assert!(
            !imported
                .diagnostics
                .iter()
                .any(|d| d.contains("Secondary is preserved"))
        );
        rules.add_layer("bad.ini", "[BACKUP]\nRange=3.5\n").unwrap();
        assert!(import(&rules, &policy()).is_err());
    }
    #[test]
    fn unsupported_values_fail_without_silent_rounding_or_clamping() {
        for text in [
            "[GUN]\nRange=5.5\n",
            "[GUN]\nDamage=-1\n",
            "[TANK]\nSpeed=20\n",
            "[TANK]\nCost=-1\n",
            "[GUN]\nRange=1025\n",
            "[GUN]\nWarhead=MISSING\n",
            "[AP]\nVerses=100%\n",
            "[TANK]\nArmor=custom\n",
        ] {
            let mut r = fixture();
            r.add_layer("mod.ini", text).unwrap();
            assert!(import(&r, &policy()).is_err(), "{text}");
        }
        let mut p = policy();
        p.rof_denominator = 0;
        assert!(import(&fixture(), &p).is_err());
    }
    #[test]
    fn imported_damage_drives_combat_and_survives_save_restore() {
        use crate::{Player, Skirmish};
        use ra2ne_core::{Vec2, navigation::NavigationMap};
        use std::sync::Arc;
        let imported = import(&fixture(), &policy()).unwrap();
        let mut game = Skirmish::new(
            Arc::new(imported.rules),
            NavigationMap::new(10, 10),
            BTreeMap::from([
                (
                    0,
                    Player {
                        credits: 0,
                        defeated: false,
                    },
                ),
                (
                    1,
                    Player {
                        credits: 0,
                        defeated: false,
                    },
                ),
            ]),
        )
        .unwrap();
        let ids = game
            .populate(&[(0, 0, Vec2::new(1, 1)), (1, 0, Vec2::new(3, 1))])
            .unwrap();
        game.attack(0, &[ids[0]], ids[1]).unwrap();
        let replay = crate::commands::GameReplay {
            initial: game.save().unwrap(),
            ticks: 51,
            commands: vec![],
        };
        game.tick();
        assert_eq!(game.actor(ids[1]).unwrap().health, 90);
        let mut restored = Skirmish::load(&game.save().unwrap()).unwrap();
        for _ in 0..50 {
            game.tick();
            restored.tick();
            assert_eq!(game.state_hash(), restored.state_hash());
            assert_eq!(game.events(), restored.events());
        }
        let decoded = crate::commands::GameReplay::decode(&replay.encode().unwrap()).unwrap();
        assert_eq!(
            decoded.play(5).unwrap().game.state_hash(),
            game.state_hash()
        );
    }
}
