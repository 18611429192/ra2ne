//! Deterministic simulation primitives for RA2NE.
//!
//! Phase 0/1 intentionally has no renderer or Red Alert 2 asset dependency.

pub const TICKS_PER_SECOND: u32 = 30;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Vec2 { pub x: i32, pub y: i32 }
impl Vec2 { pub const fn new(x: i32, y: i32) -> Self { Self { x, y } } }

/// Integer-only RNG: simulation code must never use the operating-system RNG.
#[derive(Clone, Copy, Debug)]
pub struct DeterministicRng { state: u64 }
impl DeterministicRng {
    pub const fn new(seed: u64) -> Self { Self { state: seed } }
    pub fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        (z ^ (z >> 31)) as u32
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Unit { pub position: Vec2, pub goal: Vec2, pub speed: i32 }

#[derive(Debug)]
pub struct World { tick: u64, units: Vec<Unit> }
impl World {
    pub fn seeded(unit_count: usize, seed: u64) -> Self {
        let mut rng = DeterministicRng::new(seed);
        let mut units = Vec::with_capacity(unit_count);
        for _ in 0..unit_count {
            let p = Vec2::new((rng.next_u32() % 4096) as i32, (rng.next_u32() % 4096) as i32);
            let g = Vec2::new((rng.next_u32() % 4096) as i32, (rng.next_u32() % 4096) as i32);
            units.push(Unit { position: p, goal: g, speed: 2 });
        }
        Self { tick: 0, units }
    }
    pub fn tick(&mut self) {
        // Stable order is deterministic. Future jobs calculate in parallel, then
        // commit sorted by EntityId rather than mutating the World concurrently.
        for unit in &mut self.units {
            unit.position.x += (unit.goal.x - unit.position.x).signum() * unit.speed;
            unit.position.y += (unit.goal.y - unit.position.y).signum() * unit.speed;
        }
        self.tick += 1;
    }
    pub fn tick_number(&self) -> u64 { self.tick }
    pub fn unit_count(&self) -> usize { self.units.len() }
    pub fn state_hash(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for u in &self.units {
            for v in [u.position.x as u32, u.position.y as u32, u.goal.x as u32, u.goal.y as u32] {
                hash ^= v as u64;
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        hash ^ self.tick
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn same_seed_produces_same_state() {
        let mut a = World::seeded(10_000, 0x5241_324e_45);
        let mut b = World::seeded(10_000, 0x5241_324e_45);
        for _ in 0..300 { a.tick(); b.tick(); }
        assert_eq!(a.state_hash(), b.state_hash());
    }
}
