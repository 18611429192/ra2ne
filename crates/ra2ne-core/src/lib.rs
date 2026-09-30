//! Deterministic simulation primitives for RA2NE.
//!
//! Phase 0/1 intentionally has no renderer or Red Alert 2 asset dependency.

use std::collections::BTreeMap;

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

impl Unit {
    /// Advances each axis by at most `speed`, and never overshoots the goal.
    /// This stays integer-only so lockstep clients get identical results.
    fn move_towards_goal(&mut self) {
        self.position.x = advance_axis(self.position.x, self.goal.x, self.speed);
        self.position.y = advance_axis(self.position.y, self.goal.y, self.speed);
    }
}

fn advance_axis(current: i32, target: i32, speed: i32) -> i32 {
    debug_assert!(speed >= 0);
    let delta = target.saturating_sub(current);
    current.saturating_add(delta.clamp(-speed, speed))
}

/// Deterministic broad-phase spatial index. BTreeMap gives stable traversal.
#[derive(Debug)]
pub struct SpatialGrid { cell_size: i32, cells: BTreeMap<(i32, i32), Vec<usize>> }
impl SpatialGrid {
    pub fn new(cell_size: i32) -> Self {
        assert!(cell_size > 0);
        Self { cell_size, cells: BTreeMap::new() }
    }
    pub fn rebuild(&mut self, units: &[Unit]) {
        self.cells.clear();
        for (id, unit) in units.iter().enumerate() {
            let key = (unit.position.x.div_euclid(self.cell_size), unit.position.y.div_euclid(self.cell_size));
            self.cells.entry(key).or_default().push(id);
        }
    }
    pub fn candidates_near(&self, position: Vec2, radius: i32) -> Vec<usize> {
        let min_x = (position.x - radius).div_euclid(self.cell_size);
        let max_x = (position.x + radius).div_euclid(self.cell_size);
        let min_y = (position.y - radius).div_euclid(self.cell_size);
        let max_y = (position.y + radius).div_euclid(self.cell_size);
        let mut ids = Vec::new();
        for x in min_x..=max_x { for y in min_y..=max_y {
            if let Some(cell) = self.cells.get(&(x, y)) { ids.extend(cell); }
        }}
        ids.sort_unstable();
        ids
    }
}

#[derive(Debug)]
pub struct World { tick: u64, units: Vec<Unit>, spatial: SpatialGrid }
impl World {
    pub fn seeded(unit_count: usize, seed: u64) -> Self {
        let mut rng = DeterministicRng::new(seed);
        let mut units = Vec::with_capacity(unit_count);
        for _ in 0..unit_count {
            let p = Vec2::new((rng.next_u32() % 4096) as i32, (rng.next_u32() % 4096) as i32);
            let g = Vec2::new((rng.next_u32() % 4096) as i32, (rng.next_u32() % 4096) as i32);
            units.push(Unit { position: p, goal: g, speed: 2 });
        }
        let mut spatial = SpatialGrid::new(128);
        spatial.rebuild(&units);
        Self { tick: 0, units, spatial }
    }
    pub fn tick(&mut self) {
        for unit in &mut self.units {
            unit.move_towards_goal();
        }
        self.spatial.rebuild(&self.units);
        self.tick += 1;
    }
    pub fn tick_number(&self) -> u64 { self.tick }
    pub fn unit_count(&self) -> usize { self.units.len() }
    pub fn nearby_candidate_count(&self, position: Vec2, radius: i32) -> usize { self.spatial.candidates_near(position, radius).len() }
    pub fn state_hash(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for u in &self.units { for v in [u.position.x as u32, u.position.y as u32, u.goal.x as u32, u.goal.y as u32] {
            hash ^= v as u64; hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }}
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
    #[test]
    fn grid_query_is_deterministic_and_local() {
        let world = World::seeded(10_000, 7);
        let first = world.nearby_candidate_count(Vec2::new(2_048, 2_048), 128);
        assert_eq!(first, world.nearby_candidate_count(Vec2::new(2_048, 2_048), 128));
        assert!(first < world.unit_count());
    }
    #[test]
    fn movement_reaches_goal_without_overshooting() {
        let mut unit = Unit { position: Vec2::new(0, 0), goal: Vec2::new(3, -3), speed: 2 };
        unit.move_towards_goal();
        assert_eq!(unit.position, Vec2::new(2, -2));
        unit.move_towards_goal();
        assert_eq!(unit.position, unit.goal);
        unit.move_towards_goal();
        assert_eq!(unit.position, unit.goal);
    }
}
