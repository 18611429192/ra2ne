use ra2ne_assets::{rules::RuleSet, text::TextEncoding};
use ra2ne_core::{Vec2, navigation::NavigationMap};
use ra2ne_game::{
    Player, ProductionCategory, Skirmish,
    rule_import::{self, ImportPolicy},
};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Default)]
pub struct Options {
    path: Option<String>,
    overlays: Vec<String>,
    unit: Option<String>,
    buildings: Vec<String>,
    queues: Vec<String>,
    speeds: BTreeMap<u32, i32>,
    rof: Option<(u32, u32)>,
    build_ticks: Option<u32>,
}
impl Options {
    pub fn enabled(&self) -> bool {
        self.path.is_some()
    }
    pub fn parse_arg(&mut self, arg: &str) -> Result<bool, String> {
        if let Some(v) = arg.strip_prefix("--rules-experiment=") {
            self.path = Some(v.into());
        } else if let Some(v) = arg.strip_prefix("--rules-overlay=") {
            self.overlays.push(v.into());
        } else if let Some(v) = arg.strip_prefix("--rule-unit=") {
            self.unit = Some(v.into());
        } else if let Some(v) = arg.strip_prefix("--rule-building=") {
            self.buildings.push(v.into());
        } else if let Some(v) = arg.strip_prefix("--rule-queue=") {
            self.queues.push(v.into());
        } else if let Some(v) = arg.strip_prefix("--rule-speed=") {
            let (original, engine) = v
                .split_once(':')
                .ok_or("rule-speed requires original:engine")?;
            let original = original.parse().map_err(|_| "invalid original speed")?;
            let engine = engine.parse().map_err(|_| "invalid engine speed")?;
            if self.speeds.insert(original, engine).is_some() {
                return Err("duplicate speed calibration".into());
            }
        } else if let Some(v) = arg.strip_prefix("--rule-rof=") {
            let (n, d) = v
                .split_once(':')
                .ok_or("rule-rof requires numerator:denominator")?;
            self.rof = Some((
                n.parse().map_err(|_| "invalid ROF numerator")?,
                d.parse().map_err(|_| "invalid ROF denominator")?,
            ));
        } else if let Some(v) = arg.strip_prefix("--rule-build-ticks=") {
            self.build_ticks = Some(v.parse().map_err(|_| "invalid build duration")?);
        } else {
            return Ok(false);
        }
        Ok(true)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.enabled() {
            if self.unit.as_ref().is_none_or(|s| s.is_empty())
                || self.rof.is_none()
                || self.build_ticks.is_none()
            {
                return Err("rule experiment requires rule-unit, rule-rof and rule-build-ticks; every mobile Speed needs rule-speed calibration".into());
            }
            if self.rof.is_some_and(|(n, d)| n == 0 || d == 0) || self.build_ticks == Some(0) {
                return Err("rule timing calibration must be positive".into());
            }
        } else if self.unit.is_some()
            || self.rof.is_some()
            || self.build_ticks.is_some()
            || !self.speeds.is_empty()
            || !self.overlays.is_empty()
            || !self.buildings.is_empty()
            || !self.queues.is_empty()
        {
            return Err("rule options require rules-experiment".into());
        }
        if self.buildings.len() > 32 || self.queues.len() > 32 {
            return Err("at most 32 rule buildings and 32 initial queue orders allowed".into());
        }
        if self.overlays.len() > 64 {
            return Err("at most 64 rule overlays allowed".into());
        }
        Ok(())
    }
    pub fn create(
        &self,
        map: NavigationMap,
        count: usize,
        encoding: TextEncoding,
    ) -> Result<(Skirmish, Vec<String>), String> {
        self.validate()?;
        if count < 2 {
            return Err("rule scenario requires at least two units".into());
        }
        let mut source = RuleSet::default();
        for path in self.path.iter().chain(&self.overlays) {
            let bytes = super::read(path, 16 * 1024 * 1024)?;
            source.add_layer(path, &encoding.decode(&bytes)?)?;
        }
        let (rof_numerator, rof_denominator) = self.rof.ok_or("missing ROF calibration")?;
        let imported = rule_import::import(
            &source,
            &ImportPolicy {
                speed_table: self.speeds.clone(),
                build_ticks: self.build_ticks.ok_or("missing build duration")?,
                rof_numerator,
                rof_denominator,
                max_entities: 20_128,
            },
        )?;
        let id = self.unit.as_ref().ok_or("missing rule unit")?;
        let kind = *imported
            .type_indices
            .get(&id.to_ascii_lowercase())
            .ok_or_else(|| format!("unknown rule unit: {id}"))?;
        if imported.rules.units[kind].speed == 0 {
            return Err("rule scenario requires a mobile unit".into());
        }
        let mut game = Skirmish::new(
            Arc::new(imported.rules),
            map,
            BTreeMap::from([
                (
                    0,
                    Player {
                        credits: 2000,
                        defeated: false,
                    },
                ),
                (
                    1,
                    Player {
                        credits: 2000,
                        defeated: false,
                    },
                ),
            ]),
        )?;
        let entries: Vec<_> = (0..count)
            .map(|i| {
                let side = (i % 2) as u32;
                let n = i / 2;
                (
                    side,
                    kind,
                    Vec2::new(
                        if side == 0 { 4 } else { 40 } + (n % 20) as i32,
                        4 + ((n / 20) % 52) as i32,
                    ),
                )
            })
            .collect();
        game.populate(&entries)?;
        for id in &self.buildings {
            let kind = *imported
                .type_indices
                .get(&id.to_ascii_lowercase())
                .ok_or_else(|| format!("unknown rule building: {id}"))?;
            if game.rules().units[kind]
                .production
                .as_ref()
                .is_none_or(|p| p.category != ProductionCategory::Building)
            {
                return Err(format!("rule-building requires a building: {id}"));
            }
            for owner in 0..=1 {
                let mut position = None;
                for y in [0, 2, 58, 61] {
                    for x in (if owner == 0 { 2..26 } else { 40..64 }).step_by(3) {
                        let candidate = Vec2::new(x, y);
                        if game.map().is_traversable(candidate)
                            && !game.entities().any(|(entity, _)| {
                                game.movement()
                                    .unit(entity.index as usize)
                                    .unwrap()
                                    .position
                                    == candidate
                            })
                        {
                            position = Some(candidate);
                            break;
                        }
                    }
                    if position.is_some() {
                        break;
                    }
                }
                game.spawn(owner, kind, position.ok_or("no free rule base position")?)?;
            }
        }
        for id in &self.queues {
            let kind = *imported
                .type_indices
                .get(&id.to_ascii_lowercase())
                .ok_or_else(|| format!("unknown queued rule unit: {id}"))?;
            for owner in 0..=1 {
                let factory = game
                    .entities()
                    .find(|(entity, _)| game.production_available(owner, *entity, kind).is_ok())
                    .map(|(entity, _)| entity)
                    .ok_or_else(|| format!("no eligible factory for player {owner}: {id}"))?;
                game.queue_production(owner, factory, kind)?;
            }
        }
        Ok((game, imported.diagnostics))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn production_options() -> Options {
        let mut o = Options::default();
        let path = format!(
            "--rules-experiment={}/../../fixtures/production-experiment.ini",
            env!("CARGO_MANIFEST_DIR")
        );
        for arg in [
            &path,
            "--rule-unit=TESTTANK",
            "--rule-speed=5:1",
            "--rule-rof=1:1",
            "--rule-build-ticks=5",
            "--rule-building=TESTFACTORY",
            "--rule-building=TESTBARRACKS",
            "--rule-building=TESTLAB",
        ] {
            o.parse_arg(arg).unwrap();
        }
        o
    }
    #[test]
    fn bases_queue_cancel_and_save_resume_both_categories() {
        let mut o = production_options();
        o.parse_arg("--rule-queue=TESTTANK").unwrap();
        o.parse_arg("--rule-queue=TESTINFANTRY").unwrap();
        let (mut game, _) = o
            .create(NavigationMap::new(64, 64), 8, TextEncoding::Utf8)
            .unwrap();
        assert_eq!(game.entities().count(), 14);
        for owner in 0..=1 {
            assert_eq!(game.players()[&owner].credits, 1200);
            assert_eq!(
                game.entities()
                    .filter(|(id, a)| a.owner == owner && game.production(*id).is_some())
                    .count(),
                2
            );
        }
        let factory = game
            .entities()
            .find(|(id, a)| a.owner == 0 && game.production(*id).is_some())
            .unwrap()
            .0;
        let hash = game.state_hash();
        assert!(game.production_available(1, factory, 0).is_err());
        assert_eq!(game.state_hash(), hash);
        game.cancel_production(0, factory, 0).unwrap();
        assert_eq!(game.players()[&0].credits, 1900);
        let mut restored = Skirmish::load(&game.save().unwrap()).unwrap();
        for _ in 0..8 {
            game.tick();
            restored.tick();
            assert_eq!(game.state_hash(), restored.state_hash());
            assert_eq!(game.events(), restored.events());
        }
        assert_eq!(game.entities().count(), 17);
        assert!(game.entities().all(|(id, _)| game.production(id).is_none()));
    }
    #[test]
    fn scenario_rejects_invalid_buildings_and_unmet_queue_requirements() {
        let mut o = production_options();
        o.parse_arg("--rule-building=TESTTANK").unwrap();
        assert!(
            o.create(NavigationMap::new(64, 64), 8, TextEncoding::Utf8)
                .is_err()
        );
        let mut o = production_options();
        o.buildings.retain(|id| id != "TESTLAB");
        o.parse_arg("--rule-queue=TESTTANK").unwrap();
        assert!(
            o.create(NavigationMap::new(64, 64), 8, TextEncoding::Utf8)
                .is_err()
        );
        let mut o = production_options();
        o.parse_arg("--rule-building=MISSING").unwrap();
        assert!(
            o.create(NavigationMap::new(64, 64), 8, TextEncoding::Utf8)
                .is_err()
        );
    }
    #[test]
    fn explicit_calibration_is_required_and_conflicts_rejected() {
        let mut o = Options::default();
        o.parse_arg("--rule-unit=TANK").unwrap();
        assert!(o.validate().is_err());
        o.parse_arg("--rules-experiment=fixture.ini").unwrap();
        assert!(o.validate().is_err());
        o.parse_arg("--rule-rof=1:1").unwrap();
        o.parse_arg("--rule-build-ticks=30").unwrap();
        o.parse_arg("--rule-speed=5:1").unwrap();
        assert!(o.validate().is_ok());
        assert!(o.parse_arg("--rule-speed=5:2").is_err());
        o.parse_arg("--rule-rof=1:0").unwrap();
        assert!(o.validate().is_err());
    }
}
