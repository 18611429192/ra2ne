//! In-memory command playback foundation. No network transport or file format yet.
use crate::{Unit, Vec2, World, navigation::NavigationMap};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoveCommand {
    /// Command executes before movement at this tick (first tick is zero).
    pub tick: u64,
    pub player: u32,
    pub sequence: u64,
    pub units: Vec<usize>,
    pub goal: Vec2,
}

/// Arrival order cannot alter playback: sort by tick, player, sequence.
/// Player ownership validation will be supplied by the game rules layer.
#[derive(Debug, Default)]
pub struct CommandLog {
    commands: BTreeMap<(u64, u32, u64), MoveCommand>,
}

impl CommandLog {
    pub fn commands(&self) -> impl Iterator<Item = &MoveCommand> {
        self.commands.values()
    }

    pub fn insert(&mut self, mut command: MoveCommand) -> Result<(), &'static str> {
        let key = (command.tick, command.player, command.sequence);
        if self.commands.contains_key(&key) {
            return Err("duplicate command key");
        }
        command.units.sort_unstable();
        command.units.dedup();
        self.commands.insert(key, command);
        Ok(())
    }

    /// Execute a fresh world for `ticks`. Commands outside the playback window
    /// are rejected so truncated input cannot silently look like a full replay.
    pub fn play(
        &self,
        units: Vec<Unit>,
        map: &NavigationMap,
        ticks: u64,
        checkpoint_interval: u64,
    ) -> Result<Playback, &'static str> {
        if checkpoint_interval == 0 {
            return Err("checkpoint interval must be positive");
        }
        if units.iter().any(|u| u.speed < 0) {
            return Err("negative unit speed");
        }
        if self.commands.values().any(|c| c.tick >= ticks) {
            return Err("command outside playback window");
        }
        let mut world = World::from_units(units);
        let mut commands = self.commands.values().peekable();
        let mut checkpoints = vec![(0, world.state_hash())];
        for tick in 0..ticks {
            while commands.peek().is_some_and(|c| c.tick == tick) {
                let command = commands.next().unwrap();
                world.move_group(&command.units, map, command.goal)?;
            }
            world.tick();
            if world.tick_number().is_multiple_of(checkpoint_interval) || tick + 1 == ticks {
                checkpoints.push((world.tick_number(), world.state_hash()));
            }
        }
        Ok(Playback { world, checkpoints })
    }
}

#[derive(Debug)]
pub struct Playback {
    pub world: World,
    /// Tick count and deterministic world hash, including the initial world.
    pub checkpoints: Vec<(u64, u64)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn units() -> Vec<Unit> {
        vec![
            Unit {
                position: Vec2::new(0, 0),
                goal: Vec2::new(0, 0),
                speed: 1
            };
            2
        ]
    }
    fn command(tick: u64, player: u32, sequence: u64, goal: Vec2) -> MoveCommand {
        MoveCommand {
            tick,
            player,
            sequence,
            goal,
            units: vec![1, 0, 1],
        }
    }
    #[test]
    fn shuffled_arrival_produces_identical_checkpoints() {
        let commands = [
            command(0, 1, 0, Vec2::new(4, 4)),
            command(2, 1, 1, Vec2::new(0, 4)),
            command(2, 0, 0, Vec2::new(4, 0)),
        ];
        let mut a = CommandLog::default();
        let mut b = CommandLog::default();
        for c in &commands {
            a.insert(c.clone()).unwrap();
        }
        for c in commands.iter().rev() {
            b.insert(c.clone()).unwrap();
        }
        let map = NavigationMap::new(5, 5);
        let a = a.play(units(), &map, 20, 1).unwrap();
        let b = b.play(units(), &map, 20, 1).unwrap();
        assert_eq!(a.checkpoints, b.checkpoints);
        assert_eq!(a.world.unit(0).unwrap().position, Vec2::new(0, 4));
    }
    #[test]
    fn duplicates_and_truncation_are_errors() {
        let mut log = CommandLog::default();
        let c = command(3, 0, 0, Vec2::new(2, 2));
        log.insert(c.clone()).unwrap();
        assert_eq!(log.insert(c), Err("duplicate command key"));
        assert!(log.play(units(), &NavigationMap::new(5, 5), 3, 1).is_err());
        assert!(log.play(units(), &NavigationMap::new(5, 5), 4, 0).is_err());
    }
    #[test]
    fn invalid_command_fails_playback() {
        let mut log = CommandLog::default();
        let mut c = command(0, 0, 0, Vec2::new(2, 2));
        c.units = vec![2];
        log.insert(c).unwrap();
        assert!(log.play(units(), &NavigationMap::new(5, 5), 4, 1).is_err());
    }
    #[test]
    fn commands_execute_before_the_named_tick() {
        let mut log = CommandLog::default();
        log.insert(command(1, 0, 0, Vec2::new(2, 0))).unwrap();
        let result = log.play(units(), &NavigationMap::new(5, 5), 2, 1).unwrap();
        assert_eq!(result.world.unit(0).unwrap().position, Vec2::new(1, 0));
        assert_eq!(result.checkpoints.len(), 3);
    }
}
