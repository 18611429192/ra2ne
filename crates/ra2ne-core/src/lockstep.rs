//! Transport-independent input barrier. An empty frame explicitly means no input.
//! Callers authenticate player identities before submission. Ownership and game
//! rules validation are separate from this deterministic ordering layer.
use crate::replay::MoveCommand;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputFrame {
    pub tick: u64,
    pub player: u32,
    pub commands: Vec<MoveCommand>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Submission {
    Accepted,
    Duplicate,
}

#[derive(Debug)]
pub struct Lockstep {
    players: BTreeSet<u32>,
    tick: u64,
    max_lead: u64,
    max_commands: usize,
    max_selection: usize,
    pending: BTreeMap<(u64, u32), InputFrame>,
}

impl Lockstep {
    pub fn new(
        players: &[u32],
        max_lead: u64,
        max_commands: usize,
        max_selection: usize,
    ) -> Result<Self, &'static str> {
        let unique: BTreeSet<_> = players.iter().copied().collect();
        if unique.is_empty() || unique.len() != players.len() {
            return Err("player roster must be nonempty and unique");
        }
        Ok(Self {
            players: unique,
            tick: 0,
            max_lead,
            max_commands,
            max_selection,
            pending: BTreeMap::new(),
        })
    }

    pub fn tick_number(&self) -> u64 {
        self.tick
    }

    /// Validation completes before mutation. Canonical retransmissions are
    /// idempotent; conflicting frames never replace an accepted frame.
    pub fn submit(&mut self, mut frame: InputFrame) -> Result<Submission, &'static str> {
        if !self.players.contains(&frame.player) {
            return Err("unknown player");
        }
        if frame.tick < self.tick {
            return Err("stale frame");
        }
        if frame.tick - self.tick > self.max_lead {
            return Err("frame beyond input window");
        }
        if frame.commands.len() > self.max_commands {
            return Err("too many commands");
        }
        for command in &mut frame.commands {
            if command.tick != frame.tick || command.player != frame.player {
                return Err("command does not match frame");
            }
            if command.units.len() > self.max_selection {
                return Err("selection exceeds limit");
            }
            command.units.sort_unstable();
            command.units.dedup();
        }
        frame.commands.sort_by_key(|c| c.sequence);
        if frame
            .commands
            .windows(2)
            .any(|c| c[0].sequence == c[1].sequence)
        {
            return Err("duplicate command sequence");
        }
        let key = (frame.tick, frame.player);
        if let Some(existing) = self.pending.get(&key) {
            return if existing == &frame {
                Ok(Submission::Duplicate)
            } else {
                Err("conflicting input frame")
            };
        }
        self.pending.insert(key, frame);
        Ok(Submission::Accepted)
    }

    pub fn missing_players(&self) -> Vec<u32> {
        self.players
            .iter()
            .copied()
            .filter(|player| !self.pending.contains_key(&(self.tick, *player)))
            .collect()
    }

    /// None stalls without consuming any frame. Some(empty) is a completed
    /// no-input tick. The caller executes commands then advances its world once.
    pub fn take_ready(&mut self) -> Result<Option<Vec<MoveCommand>>, &'static str> {
        if !self.missing_players().is_empty() {
            return Ok(None);
        }
        let next = self.tick.checked_add(1).ok_or("tick overflow")?;
        let mut commands = Vec::new();
        for player in &self.players {
            commands.extend(self.pending.remove(&(self.tick, *player)).unwrap().commands);
        }
        self.tick = next;
        Ok(Some(commands))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vec2;
    fn frame(tick: u64, player: u32) -> InputFrame {
        InputFrame {
            tick,
            player,
            commands: vec![MoveCommand {
                tick,
                player,
                sequence: 0,
                units: vec![1, 0, 1],
                goal: Vec2::new(2, 2),
            }],
        }
    }
    fn session() -> Lockstep {
        Lockstep::new(&[1, 0], 2, 4, 8).unwrap()
    }

    #[test]
    fn missing_input_stalls_and_empty_input_advances() {
        let mut s = session();
        s.submit(frame(0, 1)).unwrap();
        assert_eq!(s.take_ready().unwrap(), None);
        assert_eq!(s.tick_number(), 0);
        assert_eq!(s.missing_players(), vec![0]);
        s.submit(InputFrame {
            tick: 0,
            player: 0,
            commands: vec![],
        })
        .unwrap();
        assert_eq!(s.take_ready().unwrap().unwrap().len(), 1);
        assert_eq!(s.tick_number(), 1);
    }
    #[test]
    fn shuffled_frames_produce_the_same_log() {
        let inputs = [frame(0, 0), frame(0, 1), frame(1, 0), frame(1, 1)];
        let mut a = session();
        let mut b = session();
        for f in &inputs {
            a.submit(f.clone()).unwrap();
        }
        for f in inputs.iter().rev() {
            b.submit(f.clone()).unwrap();
        }
        for _ in 0..2 {
            let commands = a.take_ready().unwrap().unwrap();
            assert_eq!(commands, b.take_ready().unwrap().unwrap());
            assert_eq!(commands[0].player, 0);
            assert_eq!(commands[0].units, vec![0, 1]);
        }
    }
    #[test]
    fn retransmission_is_idempotent_and_conflicts_are_atomic() {
        let mut s = session();
        s.submit(frame(0, 0)).unwrap();
        let mut f = frame(0, 0);
        f.commands[0].units = vec![0, 1];
        assert_eq!(s.submit(f.clone()), Ok(Submission::Duplicate));
        f.commands[0].goal.x = 3;
        assert_eq!(s.submit(f), Err("conflicting input frame"));
        s.submit(frame(0, 1)).unwrap();
        assert_eq!(s.take_ready().unwrap().unwrap()[0].goal.x, 2);
        assert_eq!(s.submit(frame(0, 0)), Err("stale frame"));
    }
    #[test]
    fn malformed_and_unbounded_input_is_rejected() {
        assert!(Lockstep::new(&[], 2, 4, 8).is_err());
        assert!(Lockstep::new(&[0, 0], 2, 4, 8).is_err());
        let mut s = session();
        assert!(s.submit(frame(0, 9)).is_err());
        assert!(s.submit(frame(3, 0)).is_err());
        let mut f = frame(0, 0);
        f.commands[0].player = 1;
        assert!(s.submit(f).is_err());
        let mut f = frame(0, 0);
        f.commands.push(f.commands[0].clone());
        assert!(s.submit(f).is_err());
        let mut f = frame(0, 0);
        f.commands[0].units = vec![0; 9];
        assert!(s.submit(f).is_err());
        let mut f = frame(0, 0);
        f.commands = vec![f.commands[0].clone(); 5];
        assert!(s.submit(f).is_err());
        assert_eq!(s.missing_players(), vec![0, 1]);
    }
}
