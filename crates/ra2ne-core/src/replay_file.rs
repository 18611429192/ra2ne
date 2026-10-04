//! Versioned, self-contained little-endian replay files for the current abstract
//! simulation. Not an original RA2 replay format. No assets or paths are stored.
use crate::{
    TICKS_PER_SECOND, Unit, Vec2,
    navigation::NavigationMap,
    replay::{CommandLog, Playback},
};

const MAGIC: &[u8; 8] = b"RA2NERP\0";
const VERSION: u32 = 1;
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_CELLS: usize = 1_048_576;
const MAX_UNITS: usize = 100_000;
const MAX_COMMANDS: usize = 100_000;
const MAX_TICKS: u64 = 1_000_000;

#[derive(Debug)]
pub struct ReplayFile {
    pub units: Vec<Unit>,
    pub map: NavigationMap,
    pub commands: CommandLog,
    pub ticks: u64,
    pub checkpoint_interval: u64,
}

impl ReplayFile {
    pub fn play(&self) -> Result<Playback, &'static str> {
        self.validate()?;
        self.commands.play(
            self.units.clone(),
            &self.map,
            self.ticks,
            self.checkpoint_interval,
        )
    }

    fn validate(&self) -> Result<(), &'static str> {
        let (width, height, _) = self.map.snapshot();
        if width * height > MAX_CELLS
            || self.units.len() > MAX_UNITS
            || self.commands.commands().count() > MAX_COMMANDS
            || self.ticks > MAX_TICKS
            || self.checkpoint_interval == 0
        {
            return Err("replay exceeds format limits");
        }
        if self.units.iter().any(|u| u.speed < 0) {
            return Err("negative unit speed");
        }
        for c in self.commands.commands() {
            if c.tick >= self.ticks
                || c.units.len() > MAX_UNITS
                || c.units.iter().any(|&id| id >= self.units.len())
                || !self.map.is_traversable(c.goal)
            {
                return Err("invalid replay command");
            }
        }
        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>, &'static str> {
        self.validate()?;
        let mut out = MAGIC.to_vec();
        put32(&mut out, VERSION);
        put32(&mut out, TICKS_PER_SECOND);
        put64(&mut out, self.ticks);
        put64(&mut out, self.checkpoint_interval);
        let (width, height, cells) = self.map.snapshot();
        put32(&mut out, width as u32);
        put32(&mut out, height as u32);
        for (walkable, capacity) in cells {
            out.push(u8::from(walkable));
            put64(
                &mut out,
                if capacity == usize::MAX {
                    u64::MAX
                } else {
                    capacity as u64
                },
            );
        }
        put32(&mut out, self.units.len() as u32);
        for u in &self.units {
            for v in [u.position.x, u.position.y, u.goal.x, u.goal.y, u.speed] {
                put32(&mut out, v as u32);
            }
        }
        put32(&mut out, self.commands.commands().count() as u32);
        for c in self.commands.commands() {
            put64(&mut out, c.tick);
            put32(&mut out, c.player);
            put64(&mut out, c.sequence);
            put32(&mut out, c.goal.x as u32);
            put32(&mut out, c.goal.y as u32);
            put32(&mut out, c.units.len() as u32);
            for &id in &c.units {
                put32(&mut out, id as u32);
            }
            if out.len() > MAX_BYTES - 8 {
                return Err("replay exceeds byte limit");
            }
        }
        let checksum = checksum(&out);
        put64(&mut out, checksum);
        Ok(out)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() < 48 || bytes.len() > MAX_BYTES {
            return Err("invalid replay length");
        }
        let payload = &bytes[..bytes.len() - 8];
        let expected = u64::from_le_bytes(bytes[bytes.len() - 8..].try_into().unwrap());
        if checksum(payload) != expected {
            return Err("replay checksum mismatch");
        }
        let mut r = Reader(payload);
        if r.take(8)? != MAGIC || r.u32()? != VERSION {
            return Err("unsupported replay format");
        }
        if r.u32()? != TICKS_PER_SECOND {
            return Err("unsupported simulation rate");
        }
        let ticks = r.u64()?;
        let checkpoint_interval = r.u64()?;
        if ticks > MAX_TICKS || checkpoint_interval == 0 {
            return Err("invalid timing metadata");
        }
        let width = r.u32()? as usize;
        let height = r.u32()? as usize;
        let count = width.checked_mul(height).ok_or("map size overflow")?;
        if width == 0 || height == 0 || count > MAX_CELLS || count > r.0.len() / 9 {
            return Err("invalid map dimensions");
        }
        let mut map = NavigationMap::new(width, height);
        for index in 0..count {
            let walkable = match r.take(1)?[0] {
                0 => false,
                1 => true,
                _ => return Err("invalid walkability"),
            };
            let raw = r.u64()?;
            let capacity = if raw == u64::MAX {
                usize::MAX
            } else {
                let value = usize::try_from(raw).map_err(|_| "capacity overflow")?;
                if value == usize::MAX {
                    return Err("capacity collides with unlimited sentinel");
                }
                value
            };
            let p = Vec2::new((index % width) as i32, (index / width) as i32);
            map.set_walkable(p, walkable);
            map.set_capacity(p, capacity);
        }
        let count = r.u32()? as usize;
        if count > MAX_UNITS || count > r.0.len() / 20 {
            return Err("invalid unit count");
        }
        let mut units = Vec::with_capacity(count);
        for _ in 0..count {
            units.push(Unit {
                position: Vec2::new(r.u32()? as i32, r.u32()? as i32),
                goal: Vec2::new(r.u32()? as i32, r.u32()? as i32),
                speed: r.u32()? as i32,
            });
        }
        let count = r.u32()? as usize;
        if count > MAX_COMMANDS || count > r.0.len() / 32 {
            return Err("invalid command count");
        }
        let mut commands = CommandLog::default();
        for _ in 0..count {
            let tick = r.u64()?;
            let player = r.u32()?;
            let sequence = r.u64()?;
            let goal = Vec2::new(r.u32()? as i32, r.u32()? as i32);
            let count = r.u32()? as usize;
            if count > MAX_UNITS || count > r.0.len() / 4 {
                return Err("invalid selection length");
            }
            let mut selection = Vec::with_capacity(count);
            for _ in 0..count {
                selection.push(r.u32()? as usize);
            }
            commands.insert(crate::replay::MoveCommand {
                tick,
                player,
                sequence,
                units: selection,
                goal,
            })?;
        }
        if !r.0.is_empty() {
            return Err("trailing replay data");
        }
        let replay = Self {
            units,
            map,
            commands,
            ticks,
            checkpoint_interval,
        };
        replay.validate()?;
        Ok(replay)
    }
}
fn put32(out: &mut Vec<u8>, value: u32) {
    out.extend(value.to_le_bytes());
}
fn put64(out: &mut Vec<u8>, value: u64) {
    out.extend(value.to_le_bytes());
}
// Corruption detection only; this is not an authentication mechanism.
fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    })
}
struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], &'static str> {
        if self.0.len() < count {
            return Err("truncated replay");
        }
        let (head, tail) = self.0.split_at(count);
        self.0 = tail;
        Ok(head)
    }
    fn u32(&mut self) -> Result<u32, &'static str> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, &'static str> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> ReplayFile {
        let mut map = NavigationMap::new(5, 5);
        map.set_walkable(Vec2::new(2, 1), false);
        map.set_capacity(Vec2::new(2, 2), 1);
        let mut commands = CommandLog::default();
        commands
            .insert(crate::replay::MoveCommand {
                tick: 0,
                player: 0,
                sequence: 0,
                units: vec![0],
                goal: Vec2::new(4, 4),
            })
            .unwrap();
        ReplayFile {
            units: vec![Unit {
                position: Vec2::new(0, 0),
                goal: Vec2::new(0, 0),
                speed: 1,
            }],
            map,
            commands,
            ticks: 20,
            checkpoint_interval: 2,
        }
    }
    fn recheck(bytes: &mut [u8]) {
        let n = bytes.len() - 8;
        let c = checksum(&bytes[..n]);
        bytes[n..].copy_from_slice(&c.to_le_bytes());
    }
    #[test]
    fn round_trip_preserves_map_units_commands_and_checkpoints() {
        let original = sample();
        let bytes = original.encode().unwrap();
        let decoded = ReplayFile::decode(&bytes).unwrap();
        assert_eq!(bytes, decoded.encode().unwrap());
        assert_eq!(
            original.play().unwrap().checkpoints,
            decoded.play().unwrap().checkpoints
        );
    }
    #[test]
    fn every_truncation_and_corruption_is_rejected() {
        let bytes = sample().encode().unwrap();
        for n in 0..bytes.len() {
            assert!(ReplayFile::decode(&bytes[..n]).is_err());
        }
        for n in 0..bytes.len() {
            let mut corrupt = bytes.clone();
            corrupt[n] ^= 1;
            assert!(ReplayFile::decode(&corrupt).is_err());
        }
    }
    #[test]
    fn metadata_limits_and_trailing_bytes_are_rejected() {
        for (offset, value) in [(8, 2_u32), (12, 60), (32, u32::MAX)] {
            let mut bytes = sample().encode().unwrap();
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            recheck(&mut bytes);
            assert!(ReplayFile::decode(&bytes).is_err());
        }
        let mut bytes = sample().encode().unwrap();
        bytes.insert(bytes.len() - 8, 0);
        recheck(&mut bytes);
        assert!(ReplayFile::decode(&bytes).is_err());
        let mut file = sample();
        file.units[0].speed = -1;
        assert!(file.encode().is_err());
        file.units[0].speed = 1;
        file.ticks = 0;
        assert!(file.encode().is_err());
    }
}
