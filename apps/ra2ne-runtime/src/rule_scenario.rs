use ra2ne_assets::{rules::RuleSet, text::TextEncoding};
use ra2ne_core::{Vec2, navigation::NavigationMap};
use ra2ne_game::{
    Player, Skirmish,
    rule_import::{self, ImportPolicy},
};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Default)]
pub struct Options {
    path: Option<String>,
    overlays: Vec<String>,
    unit: Option<String>,
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
        {
            return Err("rule options require rules-experiment".into());
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
                max_entities: 20_004,
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
        Ok((game, imported.diagnostics))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
