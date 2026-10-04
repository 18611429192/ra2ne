//! Explicit experimental bridge from layered INI data to engine definitions.
//! Conversion requires caller supplied timing/movement calibration. It does not
//! establish original-game semantics; every omitted behavior is reported.
use crate::{Armor, Rules, UnitDef, Verses, Weapon};
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
                    && ["Power", "Harvester"]
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
    let mut type_indices = BTreeMap::new();
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
            ("Prerequisite", !unit.prerequisites.is_empty()),
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
        // A Factory tag is a category, not an unrestricted engine factory.
        if rules.get(&unit.id, "Factory").is_some() {
            diagnostics.push(format!(
                "[{}]: Factory category is not implemented; production disabled",
                unit.id
            ));
        }
        type_indices.insert(unit.id.to_ascii_lowercase(), units.len());
        let armor = unit
            .armor
            .as_ref()
            .map_or(Ok(Armor::None), |s| Armor::parse(s))
            .map_err(|e| format!("[{}]: Armor: {e}", unit.id))?;
        units.push(UnitDef {
            armor,
            name: unit.id.clone(),
            health: unit.strength,
            speed,
            cost,
            weapon,
            secondary,
            build_ticks: policy.build_ticks,
            power,
            factory: false,
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
