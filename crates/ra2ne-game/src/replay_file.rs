//! Versioned, bounded full-game replay and input-frame wire codecs.
use crate::{EntityId, commands::*};
use ra2ne_core::{
    Vec2,
    snapshot::{Reader, Writer},
};
const MAX_BYTES: usize = 128 * 1024 * 1024;
fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    })
}
fn finish(mut w: Writer, limit: usize) -> Result<Vec<u8>, &'static str> {
    if w.0.len() > limit - 8 {
        return Err("encoded input exceeds limit");
    }
    w.u64(checksum(&w.0));
    Ok(w.0)
}
fn open<'a>(bytes: &'a [u8], magic: &[u8; 8], limit: usize) -> Result<Reader<'a>, &'static str> {
    if bytes.len() < 16 || bytes.len() > limit {
        return Err("input file outside limits");
    }
    let end = bytes.len() - 8;
    if checksum(&bytes[..end]) != u64::from_le_bytes(bytes[end..].try_into().unwrap()) {
        return Err("input checksum mismatch");
    }
    let mut r = Reader::new(&bytes[..end])?;
    if r.take(8)? != magic {
        return Err("unsupported input format");
    }
    Ok(r)
}
fn id(w: &mut Writer, value: EntityId) {
    w.u32(value.index);
    w.u32(value.generation);
}
fn read_id(r: &mut Reader<'_>) -> Result<EntityId, &'static str> {
    Ok(EntityId {
        index: r.u32()?,
        generation: r.u32()?,
    })
}
fn selection(w: &mut Writer, ids: &[EntityId]) -> Result<(), &'static str> {
    if ids.len() > 1024 {
        return Err("selection exceeds codec limit");
    }
    w.u64(ids.len() as u64);
    for &value in ids {
        id(w, value);
    }
    Ok(())
}
fn read_selection(r: &mut Reader<'_>) -> Result<Vec<EntityId>, &'static str> {
    (0..r.count(1024)?).map(|_| read_id(r)).collect()
}
fn write_action(w: &mut Writer, action: &Action) -> Result<(), &'static str> {
    match action {
        Action::Move { units, goal } => {
            w.u32(0);
            selection(w, units)?;
            w.i32(goal.x);
            w.i32(goal.y);
        }
        Action::Stop { units } => {
            w.u32(1);
            selection(w, units)?;
        }
        Action::Attack { units, target } => {
            w.u32(2);
            selection(w, units)?;
            id(w, *target);
        }
        Action::Produce { factory, kind } => {
            w.u32(3);
            id(w, *factory);
            w.u64(*kind as u64);
        }
        Action::Cancel { factory, index } => {
            w.u32(4);
            id(w, *factory);
            w.u64(*index as u64);
        }
        Action::Harvest {
            units,
            resource,
            refinery,
        } => {
            w.u32(5);
            selection(w, units)?;
            w.i32(resource.x);
            w.i32(resource.y);
            id(w, *refinery);
        }
    }
    Ok(())
}
fn read_action(r: &mut Reader<'_>) -> Result<Action, &'static str> {
    Ok(match r.u32()? {
        0 => Action::Move {
            units: read_selection(r)?,
            goal: Vec2::new(r.i32()?, r.i32()?),
        },
        1 => Action::Stop {
            units: read_selection(r)?,
        },
        2 => Action::Attack {
            units: read_selection(r)?,
            target: read_id(r)?,
        },
        3 => Action::Produce {
            factory: read_id(r)?,
            kind: r.count(100_000)?,
        },
        4 => Action::Cancel {
            factory: read_id(r)?,
            index: r.count(31)?,
        },
        5 => Action::Harvest {
            units: read_selection(r)?,
            resource: Vec2::new(r.i32()?, r.i32()?),
            refinery: read_id(r)?,
        },
        _ => return Err("unknown action tag"),
    })
}
impl Frame {
    pub fn encode(&self) -> Result<Vec<u8>, &'static str> {
        if self.commands.len() > 64 {
            return Err("frame command limit");
        }
        let mut w = Writer::new();
        w.0.extend_from_slice(b"RA2NEGF1");
        w.u64(self.tick);
        w.u32(self.player);
        w.u64(self.commands.len() as u64);
        for command in &self.commands {
            if command.tick != self.tick || command.player != self.player {
                return Err("frame metadata mismatch");
            }
            w.u64(command.sequence);
            write_action(&mut w, &command.action)?;
        }
        finish(w, 64 * 1024)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, &'static str> {
        let mut r = open(bytes, b"RA2NEGF1", 64 * 1024)?;
        let tick = r.u64()?;
        let player = r.u32()?;
        let count = r.count(64)?;
        let mut commands = Vec::with_capacity(count);
        for _ in 0..count {
            commands.push(Command {
                tick,
                player,
                sequence: r.u64()?,
                action: read_action(&mut r)?,
            });
        }
        r.end()?;
        Ok(Self {
            tick,
            player,
            commands,
        })
    }
}
impl GameReplay {
    pub fn encode(&self) -> Result<Vec<u8>, &'static str> {
        if self.ticks > 1_000_000 || self.commands.len() > 100_000 {
            return Err("replay exceeds codec limit");
        }
        let mut w = Writer::new();
        w.0.extend_from_slice(b"RA2NEGR5");
        w.bytes(&self.initial);
        w.u64(self.ticks);
        w.u64(self.commands.len() as u64);
        for command in &self.commands {
            w.u64(command.tick);
            w.u32(command.player);
            w.u64(command.sequence);
            write_action(&mut w, &command.action)?;
        }
        finish(w, MAX_BYTES)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, &'static str> {
        let mut r = open(bytes, b"RA2NEGR5", MAX_BYTES)?;
        let initial = r.bytes(MAX_BYTES)?.to_vec();
        let ticks = r.u64()?;
        if ticks > 1_000_000 {
            return Err("replay duration exceeds limit");
        }
        let count = r.count(100_000)?;
        let mut commands = Vec::with_capacity(count);
        for _ in 0..count {
            commands.push(Command {
                tick: r.u64()?,
                player: r.u32()?,
                sequence: r.u64()?,
                action: read_action(&mut r)?,
            });
        }
        r.end()?;
        Ok(Self {
            initial,
            ticks,
            commands,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn game_replay_roundtrip_and_all_frame_corruptions_rejected() {
        let mut game = crate::tests::game();
        let entity = game.spawn(0, 0, Vec2::new(1, 1)).unwrap();
        let command = Command {
            tick: 0,
            player: 0,
            sequence: 3,
            action: Action::Move {
                units: vec![entity],
                goal: Vec2::new(5, 5),
            },
        };
        let frame = Frame {
            tick: 0,
            player: 0,
            commands: vec![command.clone()],
        };
        let bytes = frame.encode().unwrap();
        assert_eq!(Frame::decode(&bytes).unwrap(), frame);
        for end in 0..bytes.len() {
            assert!(Frame::decode(&bytes[..end]).is_err());
        }
        for i in 0..bytes.len() {
            let mut bad = bytes.clone();
            bad[i] ^= 1;
            assert!(Frame::decode(&bad).is_err());
        }
        let replay = GameReplay {
            initial: game.save().unwrap(),
            ticks: 20,
            commands: vec![command],
        };
        let encoded = replay.encode().unwrap();
        let restored = GameReplay::decode(&encoded).unwrap();
        assert_eq!(
            replay.play(1).unwrap().checkpoints,
            restored.play(1).unwrap().checkpoints
        );
    }
}
