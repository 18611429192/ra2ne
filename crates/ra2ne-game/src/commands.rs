//! Stable commands for full game replay and transport-independent lockstep.
use super::*;
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    Move {
        units: Vec<EntityId>,
        goal: Vec2,
    },
    Stop {
        units: Vec<EntityId>,
    },
    Attack {
        units: Vec<EntityId>,
        target: EntityId,
    },
    Produce {
        factory: EntityId,
        kind: usize,
    },
    Cancel {
        factory: EntityId,
        index: usize,
    },
    Harvest {
        units: Vec<EntityId>,
        resource: Vec2,
        refinery: EntityId,
    },
}
impl Action {
    /// Validates selection limits and sorts/deduplicates handles for stable input.
    pub fn canonicalize(&mut self) -> Result<(), &'static str> {
        let ids = match self {
            Self::Move { units, .. }
            | Self::Stop { units }
            | Self::Attack { units, .. }
            | Self::Harvest { units, .. } => Some(units),
            _ => None,
        };
        if let Some(ids) = ids {
            if ids.len() > 1024 {
                return Err("command selection exceeds limit");
            }
            ids.sort_unstable();
            ids.dedup();
        }
        Ok(())
    }
    pub fn apply(&self, game: &mut Skirmish, player: u32) -> Result<(), &'static str> {
        match self {
            Self::Move { units, goal } => game.move_units(player, units, *goal),
            Self::Stop { units } => game.stop_units(player, units),
            Self::Attack { units, target } => game.attack(player, units, *target),
            Self::Produce { factory, kind } => game.queue_production(player, *factory, *kind),
            Self::Cancel { factory, index } => game.cancel_production(player, *factory, *index),
            Self::Harvest {
                units,
                resource,
                refinery,
            } => game.harvest(player, units, *resource, *refinery),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Command {
    pub tick: u64,
    pub player: u32,
    pub sequence: u64,
    pub action: Action,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    pub tick: u64,
    pub player: u32,
    pub commands: Vec<Command>,
}
#[derive(Debug)]
pub struct GameLockstep {
    players: BTreeSet<u32>,
    tick: u64,
    max_lead: u64,
    frames: BTreeMap<(u64, u32), Frame>,
}
impl GameLockstep {
    pub fn new(
        players: impl IntoIterator<Item = u32>,
        tick: u64,
        max_lead: u64,
    ) -> Result<Self, &'static str> {
        let players = players.into_iter().collect::<BTreeSet<_>>();
        if players.is_empty() || players.len() > 64 || max_lead > 120 {
            return Err("invalid game lockstep limits");
        }
        Ok(Self {
            players,
            tick,
            max_lead,
            frames: BTreeMap::new(),
        })
    }
    pub fn tick_number(&self) -> u64 {
        self.tick
    }
    pub fn missing_players(&self) -> Vec<u32> {
        self.players
            .iter()
            .copied()
            .filter(|p| !self.frames.contains_key(&(self.tick, *p)))
            .collect()
    }
    /// The transport must authenticate the sender before calling this method.
    pub fn submit(
        &mut self,
        authenticated_player: u32,
        mut frame: Frame,
    ) -> Result<(), &'static str> {
        if frame.player != authenticated_player || !self.players.contains(&frame.player) {
            return Err("frame sender mismatch");
        }
        if frame.tick < self.tick
            || frame.tick.saturating_sub(self.tick) > self.max_lead
            || frame.commands.len() > 64
        {
            return Err("frame outside limits");
        }
        let mut sequences = BTreeSet::new();
        let mut total_selection = 0;
        for command in &mut frame.commands {
            if command.tick != frame.tick
                || command.player != frame.player
                || !sequences.insert(command.sequence)
            {
                return Err("invalid frame command metadata");
            }
            command.action.canonicalize()?;
            total_selection += match &command.action {
                Action::Move { units, .. }
                | Action::Stop { units }
                | Action::Attack { units, .. }
                | Action::Harvest { units, .. } => units.len(),
                _ => 0,
            };
        }
        if total_selection > 4096 {
            return Err("frame selection budget exceeded");
        }
        frame.commands.sort_by_key(|c| c.sequence);
        let key = (frame.tick, frame.player);
        if let Some(previous) = self.frames.get(&key) {
            if previous != &frame {
                return Err("conflicting input retransmission");
            }
            return Ok(());
        }
        self.frames.insert(key, frame);
        Ok(())
    }
    pub fn take_ready(&mut self) -> Option<Vec<Command>> {
        if self.tick == u64::MAX || !self.missing_players().is_empty() {
            return None;
        }
        let mut commands = Vec::new();
        for &player in &self.players {
            commands.extend(self.frames.remove(&(self.tick, player)).unwrap().commands);
        }
        self.tick = self.tick.checked_add(1)?;
        Some(commands)
    }
}
#[derive(Debug)]
pub struct GameReplay {
    pub initial: Vec<u8>,
    pub ticks: u64,
    pub commands: Vec<Command>,
}
#[derive(Debug)]
pub struct GamePlayback {
    pub game: Skirmish,
    pub checkpoints: Vec<(u64, u64)>,
    pub rejected: Vec<(u64, u32, u64, &'static str)>,
}
impl GameReplay {
    pub fn play(&self, interval: u64) -> Result<GamePlayback, &'static str> {
        if self.ticks > 1_000_000 || self.commands.len() > 100_000 || interval == 0 {
            return Err("game replay exceeds limits");
        }
        let mut game = Skirmish::load(&self.initial)?;
        let start = game.tick_number();
        let end = start
            .checked_add(self.ticks)
            .ok_or("replay tick overflow")?;
        let mut commands = self.commands.clone();
        for c in &mut commands {
            if c.tick < start || c.tick >= end || !game.players().contains_key(&c.player) {
                return Err("replay command outside timeline");
            }
            c.action.canonicalize()?;
        }
        commands.sort_by_key(|c| (c.tick, c.player, c.sequence));
        if commands.windows(2).any(|pair| {
            (pair[0].tick, pair[0].player, pair[0].sequence)
                == (pair[1].tick, pair[1].player, pair[1].sequence)
        }) {
            return Err("duplicate replay command");
        }
        let mut commands = commands.into_iter().peekable();
        let mut checkpoints = Vec::new();
        let mut rejected = Vec::new();
        for tick in start..end {
            while commands.peek().is_some_and(|c| c.tick == tick) {
                let command = commands.next().unwrap();
                if let Err(reason) = command.action.apply(&mut game, command.player) {
                    rejected.push((tick, command.player, command.sequence, reason));
                }
            }
            game.tick();
            if (tick - start + 1) % interval == 0 || tick + 1 == end {
                checkpoints.push((tick + 1, game.state_hash()));
            }
        }
        Ok(GamePlayback {
            game,
            checkpoints,
            rejected,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_game_lockstep_shuffled_frames_and_replay_match() {
        let mut game = super::super::tests::game();
        let a = game.spawn(0, 0, Vec2::new(1, 1)).unwrap();
        let b = game.spawn(1, 0, Vec2::new(15, 15)).unwrap();
        let initial = game.save().unwrap();
        let commands = vec![
            Command {
                tick: 0,
                player: 1,
                sequence: 0,
                action: Action::Move {
                    units: vec![b],
                    goal: Vec2::new(7, 7),
                },
            },
            Command {
                tick: 0,
                player: 0,
                sequence: 0,
                action: Action::Move {
                    units: vec![a, a],
                    goal: Vec2::new(7, 7),
                },
            },
        ];
        let replay = GameReplay {
            initial,
            ticks: 30,
            commands: commands.clone(),
        };
        let playback = replay.play(1).unwrap();
        assert!(playback.rejected.is_empty());
        let mut barrier = GameLockstep::new([0, 1], 0, 2).unwrap();
        assert!(
            barrier
                .submit(
                    0,
                    Frame {
                        tick: 0,
                        player: 1,
                        commands: vec![commands[0].clone()]
                    }
                )
                .is_err()
        );
        for tick in 0..30 {
            for player in [1, 0] {
                let frame = Frame {
                    tick,
                    player,
                    commands: commands
                        .iter()
                        .filter(|c| c.tick == tick && c.player == player)
                        .cloned()
                        .collect(),
                };
                barrier.submit(player, frame.clone()).unwrap();
                barrier.submit(player, frame).unwrap();
                if player == 1 {
                    assert!(barrier.take_ready().is_none());
                }
            }
            for command in barrier.take_ready().unwrap() {
                command.action.apply(&mut game, command.player).unwrap();
            }
            game.tick();
            assert_eq!(game.state_hash(), playback.checkpoints[tick as usize].1);
        }
        assert_eq!(game.state_hash(), playback.game.state_hash());
    }
}
