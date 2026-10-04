//! Runtime commands share the engine action path and can be recorded verbatim.
use ra2ne_game::{
    Skirmish,
    commands::{Action, Command, GameReplay},
};
use std::ops::Deref;

pub struct Session {
    game: Skirmish,
    replay: Option<GameReplay>,
    start: u64,
    recorded_bytes: usize,
}
impl Deref for Session {
    type Target = Skirmish;
    fn deref(&self) -> &Skirmish {
        &self.game
    }
}
impl Session {
    pub fn new(game: Skirmish) -> Self {
        let start = game.tick_number();
        Self {
            game,
            replay: None,
            start,
            recorded_bytes: 0,
        }
    }
    pub fn record(&mut self) -> Result<(), &'static str> {
        self.game
            .tick_number()
            .checked_add(1_000_000)
            .ok_or("recording tick overflow")?;
        self.replay = Some(GameReplay {
            initial: self.game.save()?,
            ticks: 0,
            commands: Vec::new(),
        });
        self.recorded_bytes = self.replay.as_ref().unwrap().initial.len() + 40;
        if self.recorded_bytes > 128 * 1024 * 1024 {
            self.replay = None;
            return Err("recording initial state exceeds byte limit");
        }
        self.start = self.game.tick_number();
        Ok(())
    }
    pub fn apply(&mut self, player: u32, mut action: Action) -> Result<(), &'static str> {
        if let Some(replay) = &mut self.replay {
            if replay.commands.len() >= 100_000 || replay.ticks >= 1_000_000 {
                return Err("runtime recording limit reached");
            }
            if !self.game.players().contains_key(&player) {
                return Err("unknown recording player");
            }
            action.canonicalize()?;
            let command = Command {
                tick: self
                    .start
                    .checked_add(replay.ticks)
                    .ok_or("recording tick overflow")?,
                player,
                sequence: replay.commands.len() as u64,
                action: action.clone(),
            };
            let bytes = ra2ne_game::commands::Frame {
                tick: command.tick,
                player,
                commands: vec![command.clone()],
            }
            .encode()?
            .len();
            // Conservatively include the frame header in the recording budget.
            if self.recorded_bytes + bytes > 128 * 1024 * 1024 {
                return Err("runtime recording byte limit reached");
            }
            self.recorded_bytes += bytes;
            // Rejections are input too: replay will reject them at the same tick.
            replay.commands.push(command);
        }
        action.apply(&mut self.game, player)
    }
    pub fn tick(&mut self) {
        self.game.tick();
        if let Some(replay) = &mut self.replay {
            replay.ticks += 1;
        }
    }
    /// Include commands entered after the last tick in a final simulation step.
    pub fn finish(&mut self) {
        if self.replay.as_ref().is_some_and(|r| {
            r.commands
                .last()
                .is_some_and(|c| c.tick == self.start + r.ticks)
        }) {
            self.tick();
        }
    }
    pub fn replay_bytes(&self) -> Result<Vec<u8>, &'static str> {
        self.replay
            .as_ref()
            .ok_or("recording was not enabled")?
            .encode()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra2ne_core::{Vec2, navigation::NavigationMap};
    #[test]
    fn finished_game_keeps_replay_timeline_and_rejected_tail_input() {
        let template = crate::battle::synthetic(NavigationMap::new(64, 64), 8).unwrap();
        let mut game = Skirmish::new(
            std::sync::Arc::new(template.rules().clone()),
            NavigationMap::new(64, 64),
            template.players().clone(),
        )
        .unwrap();
        game.spawn(0, 0, Vec2::new(1, 1)).unwrap();
        game.spawn(1, 0, Vec2::new(2, 1)).unwrap();
        let mut session = Session::new(game);
        session.record().unwrap();
        for _ in 0..200 {
            session.tick();
        }
        assert!(session.finished());
        assert!(session.tick_number() < 200);
        assert_eq!(
            session.apply(0, Action::Stop { units: vec![] }),
            Err("game finished")
        );
        session.finish();
        let replay = GameReplay::decode(&session.replay_bytes().unwrap()).unwrap();
        assert_eq!(replay.ticks, 201);
        let playback = replay.play(1).unwrap();
        assert_eq!(playback.rejected.len(), 1);
        assert_eq!(playback.game.state_hash(), session.state_hash());
    }
    #[test]
    fn recording_rejects_unencodable_inputs_before_mutation() {
        let game = crate::battle::synthetic(NavigationMap::new(64, 64), 8).unwrap();
        let mut session = Session::new(game);
        session.record().unwrap();
        let id = session.entities().next().unwrap().0;
        let hash = session.state_hash();
        assert!(
            session
                .apply(
                    0,
                    Action::Stop {
                        units: vec![id; 1025]
                    }
                )
                .is_err()
        );
        assert!(session.apply(99, Action::Stop { units: vec![id] }).is_err());
        assert_eq!(session.state_hash(), hash);
        session.recorded_bytes = 128 * 1024 * 1024;
        assert!(session.apply(0, Action::Stop { units: vec![id] }).is_err());
        assert_eq!(session.state_hash(), hash);
        let replay = GameReplay::decode(&session.replay_bytes().unwrap()).unwrap();
        assert!(replay.commands.is_empty());
        assert_eq!(replay.play(1).unwrap().game.state_hash(), hash);
    }
    #[test]
    fn resumed_recording_uses_snapshot_tick_and_canonical_selection() {
        let game = crate::battle::synthetic(NavigationMap::new(64, 64), 8).unwrap();
        let mut session = Session::new(game);
        for _ in 0..7 {
            session.tick();
        }
        session.record().unwrap();
        let id = session.entities().next().unwrap().0;
        session
            .apply(
                0,
                Action::Stop {
                    units: vec![id, id],
                },
            )
            .unwrap();
        session.finish();
        let replay = GameReplay::decode(&session.replay_bytes().unwrap()).unwrap();
        assert_eq!(replay.commands[0].tick, 7);
        assert!(matches!(&replay.commands[0].action, Action::Stop {units} if units.len() == 1));
        assert_eq!(
            replay.play(1).unwrap().game.state_hash(),
            session.state_hash()
        );
        session.finish();
        assert_eq!(session.tick_number(), 8);
    }
    #[test]
    fn runtime_input_refund_rejections_and_tail_match_encoded_replay() {
        let game = crate::battle::synthetic(NavigationMap::new(64, 64), 8).unwrap();
        let mut session = Session::new(game);
        session.record().unwrap();
        let factory = session
            .entities()
            .find(|(_, a)| a.owner == 0 && session.rules().units[a.kind].factory)
            .unwrap()
            .0;
        let mobile = session
            .entities()
            .find(|(_, a)| a.owner == 0 && session.rules().units[a.kind].speed > 0)
            .unwrap()
            .0;
        session
            .apply(0, Action::Produce { factory, kind: 2 })
            .unwrap();
        session
            .apply(
                0,
                Action::Move {
                    units: vec![mobile],
                    goal: Vec2::new(20, 20),
                },
            )
            .unwrap();
        session.tick();
        session
            .apply(
                0,
                Action::Stop {
                    units: vec![mobile],
                },
            )
            .unwrap();
        assert!(
            session
                .apply(1, Action::Cancel { factory, index: 0 })
                .is_err()
        );
        for _ in 0..10 {
            session.tick();
        }
        session
            .apply(0, Action::Cancel { factory, index: 1 })
            .unwrap();
        session.finish();
        let replay = GameReplay::decode(&session.replay_bytes().unwrap()).unwrap();
        assert_eq!(replay.ticks, 12);
        let playback = replay.play(1).unwrap();
        assert_eq!(playback.rejected.len(), 1);
        assert_eq!(playback.game.state_hash(), session.state_hash());
        assert_eq!(playback.game.players(), session.players());
    }
}
