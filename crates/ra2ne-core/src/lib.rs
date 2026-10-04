//! Deterministic simulation primitives for RA2NE.
//!
//! Phase 0/1 intentionally has no renderer or Red Alert 2 asset dependency.

use std::collections::BTreeMap;
use std::sync::Arc;
pub mod navigation;
use navigation::{NavigationMap, SharedRoute};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MovementState {
    Idle,
    Moving,
    Arrived,
    Unreachable,
    Waiting,
}

#[derive(Debug)]
struct MovementOrder {
    route: Arc<SharedRoute>,
    state: MovementState,
}

pub const TICKS_PER_SECOND: u32 = 30;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Vec2 {
    pub x: i32,
    pub y: i32,
}
impl Vec2 {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// Integer-only RNG: simulation code must never use the operating-system RNG.
#[derive(Clone, Copy, Debug)]
pub struct DeterministicRng {
    state: u64,
}
impl DeterministicRng {
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }
    pub fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        (z ^ (z >> 31)) as u32
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Unit {
    pub position: Vec2,
    pub goal: Vec2,
    pub speed: i32,
}

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
pub struct SpatialGrid {
    cell_size: i32,
    cells: BTreeMap<(i32, i32), Vec<usize>>,
}
impl SpatialGrid {
    pub fn new(cell_size: i32) -> Self {
        assert!(cell_size > 0);
        Self {
            cell_size,
            cells: BTreeMap::new(),
        }
    }
    pub fn rebuild(&mut self, units: &[Unit]) {
        self.cells.clear();
        for (id, unit) in units.iter().enumerate() {
            let key = (
                unit.position.x.div_euclid(self.cell_size),
                unit.position.y.div_euclid(self.cell_size),
            );
            self.cells.entry(key).or_default().push(id);
        }
    }
    pub fn candidates_near(&self, position: Vec2, radius: i32) -> Vec<usize> {
        let min_x = (position.x - radius).div_euclid(self.cell_size);
        let max_x = (position.x + radius).div_euclid(self.cell_size);
        let min_y = (position.y - radius).div_euclid(self.cell_size);
        let max_y = (position.y + radius).div_euclid(self.cell_size);
        let mut ids = Vec::new();
        for x in min_x..=max_x {
            for y in min_y..=max_y {
                if let Some(cell) = self.cells.get(&(x, y)) {
                    ids.extend(cell);
                }
            }
        }
        ids.sort_unstable();
        ids
    }
}

#[derive(Debug)]
pub struct World {
    tick: u64,
    units: Vec<Unit>,
    spatial: SpatialGrid,
    orders: Vec<Option<MovementOrder>>,
}
impl World {
    pub fn seeded(unit_count: usize, seed: u64) -> Self {
        let mut rng = DeterministicRng::new(seed);
        let mut units = Vec::with_capacity(unit_count);
        for _ in 0..unit_count {
            let p = Vec2::new(
                (rng.next_u32() % 4096) as i32,
                (rng.next_u32() % 4096) as i32,
            );
            let g = Vec2::new(
                (rng.next_u32() % 4096) as i32,
                (rng.next_u32() % 4096) as i32,
            );
            units.push(Unit {
                position: p,
                goal: g,
                speed: 2,
            });
        }
        let mut spatial = SpatialGrid::new(128);
        spatial.rebuild(&units);
        Self {
            tick: 0,
            orders: (0..units.len()).map(|_| None).collect(),
            units,
            spatial,
        }
    }
    pub fn from_units(units: Vec<Unit>) -> Self {
        assert!(units.iter().all(|u| u.speed >= 0));
        let mut spatial = SpatialGrid::new(128);
        spatial.rebuild(&units);
        Self {
            tick: 0,
            orders: (0..units.len()).map(|_| None).collect(),
            units,
            spatial,
        }
    }
    /// Validate the complete command before changing any units. All selected
    /// units share one path field, independent of selection order or duplicates.
    pub fn move_group(
        &mut self,
        ids: &[usize],
        map: &NavigationMap,
        goal: Vec2,
    ) -> Result<(), &'static str> {
        if ids.iter().any(|&id| id >= self.units.len()) {
            return Err("invalid unit id");
        }
        if ids.is_empty() {
            return Ok(());
        }
        let route = Arc::new(map.route_to(goal).ok_or("invalid or blocked goal")?);
        for &id in ids {
            let unit = &mut self.units[id];
            unit.goal = goal;
            let state = if route.next_step(unit.position).is_none() {
                MovementState::Unreachable
            } else if unit.position == goal {
                MovementState::Arrived
            } else {
                MovementState::Moving
            };
            self.orders[id] = Some(MovementOrder {
                route: Arc::clone(&route),
                state,
            });
        }
        Ok(())
    }
    pub fn unit(&self, id: usize) -> Option<&Unit> {
        self.units.get(id)
    }
    pub fn movement_state(&self, id: usize) -> Option<MovementState> {
        self.units.get(id).map(|_| {
            self.orders[id]
                .as_ref()
                .map_or(MovementState::Idle, |o| o.state)
        })
    }
    pub fn tick(&mut self) {
        let mut entries = BTreeMap::<(i32, i32), usize>::new();
        for (unit, order) in self.units.iter_mut().zip(&mut self.orders) {
            if let Some(order) = order {
                if !matches!(order.state, MovementState::Moving | MovementState::Waiting) {
                    continue;
                }
                order.state = MovementState::Moving;
                for _ in 0..unit.speed {
                    let Some(next) = order.route.next_step(unit.position) else {
                        order.state = MovementState::Unreachable;
                        break;
                    };
                    let capacity = order.route.capacity_at(next);
                    if capacity != usize::MAX {
                        let entered = entries.entry((next.x, next.y)).or_default();
                        if *entered >= capacity {
                            order.state = MovementState::Waiting;
                            break;
                        }
                        *entered += 1;
                    }
                    unit.position = next;
                    if next == unit.goal {
                        order.state = MovementState::Arrived;
                        break;
                    }
                }
            } else {
                unit.move_towards_goal();
            }
        }
        self.spatial.rebuild(&self.units);
        self.tick += 1;
    }
    pub fn tick_number(&self) -> u64 {
        self.tick
    }
    pub fn unit_count(&self) -> usize {
        self.units.len()
    }
    pub fn nearby_candidate_count(&self, position: Vec2, radius: i32) -> usize {
        self.spatial.candidates_near(position, radius).len()
    }
    pub fn state_hash(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for u in &self.units {
            for v in [
                u.position.x as u32,
                u.position.y as u32,
                u.goal.x as u32,
                u.goal.y as u32,
                u.speed as u32,
            ] {
                hash ^= v as u64;
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        // Route topology changes future simulation, even before units move.
        let mut routes = Vec::<&SharedRoute>::new();
        let mut route_ids = BTreeMap::new();
        for order in &self.orders {
            let value = if let Some(order) = order {
                let route = order.route.as_ref();
                // Addresses only identify shared allocations locally; IDs are
                // assigned in unit order, never from address traversal order.
                let id = *route_ids
                    .entry(std::ptr::from_ref(route))
                    .or_insert_with(|| {
                        routes.push(route);
                        routes.len() - 1
                    });
                ((id as u64 + 1) << 3) | order.state as u64
            } else {
                0
            };
            hash ^= value;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        for route in routes {
            hash ^= route.state_hash();
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^ self.tick
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bridge_capacity_queues_across_groups_and_drains() {
        let mut map = NavigationMap::new(3, 1);
        map.set_capacity(Vec2::new(1, 0), 1);
        let units = vec![
            Unit {
                position: Vec2::new(0, 0),
                goal: Vec2::new(0, 0),
                speed: 1
            };
            4
        ];
        let mut world = World::from_units(units);
        world.move_group(&[0, 2], &map, Vec2::new(2, 0)).unwrap();
        world.move_group(&[1, 3], &map, Vec2::new(2, 0)).unwrap();
        world.tick();
        assert_eq!(world.unit(0).unwrap().position, Vec2::new(1, 0));
        for id in 1..4 {
            assert_eq!(world.movement_state(id), Some(MovementState::Waiting));
        }
        for _ in 0..4 {
            world.tick();
        }
        for id in 0..4 {
            assert_eq!(world.movement_state(id), Some(MovementState::Arrived));
        }
    }
    #[test]
    fn zero_capacity_closes_route_and_reissue_resumes() {
        let mut map = NavigationMap::new(3, 1);
        map.set_capacity(Vec2::new(1, 0), 0);
        let mut world = World::from_units(vec![Unit {
            position: Vec2::new(0, 0),
            goal: Vec2::new(0, 0),
            speed: 1,
        }]);
        world.move_group(&[0], &map, Vec2::new(2, 0)).unwrap();
        assert_eq!(world.movement_state(0), Some(MovementState::Unreachable));
        map.set_capacity(Vec2::new(1, 0), 1);
        world.move_group(&[0], &map, Vec2::new(2, 0)).unwrap();
        world.tick();
        world.tick();
        assert_eq!(world.movement_state(0), Some(MovementState::Arrived));
    }
    #[test]
    fn batch_routes_around_wall_and_arrives() {
        let mut map = NavigationMap::new(7, 5);
        for y in 0..5 {
            map.set_walkable(Vec2::new(3, y), y == 2);
        }
        let units = (0..100)
            .map(|_| Unit {
                position: Vec2::new(0, 0),
                goal: Vec2::new(0, 0),
                speed: 2,
            })
            .collect();
        let mut world = World::from_units(units);
        world
            .move_group(&(0..100).collect::<Vec<_>>(), &map, Vec2::new(6, 0))
            .unwrap();
        for _ in 0..10 {
            world.tick();
        }
        for id in 0..100 {
            assert_eq!(world.unit(id).unwrap().position, Vec2::new(6, 0));
            assert_eq!(world.movement_state(id), Some(MovementState::Arrived));
        }
    }
    #[test]
    fn invalid_batch_is_atomic_and_unreachable_stays_put() {
        let mut map = NavigationMap::new(3, 3);
        for y in 0..3 {
            map.set_walkable(Vec2::new(1, y), false);
        }
        let mut world = World::from_units(vec![Unit {
            position: Vec2::new(0, 0),
            goal: Vec2::new(0, 0),
            speed: 1,
        }]);
        let before = world.state_hash();
        assert!(world.move_group(&[0, 1], &map, Vec2::new(2, 2)).is_err());
        assert_eq!(world.state_hash(), before);
        world.move_group(&[0], &map, Vec2::new(2, 2)).unwrap();
        world.tick();
        assert_eq!(world.movement_state(0), Some(MovementState::Unreachable));
        assert_eq!(world.unit(0).unwrap().position, Vec2::new(0, 0));
    }
    #[test]
    fn batch_selection_order_does_not_change_simulation() {
        let map = NavigationMap::new(8, 8);
        let units = vec![
            Unit {
                position: Vec2::new(0, 0),
                goal: Vec2::new(0, 0),
                speed: 1
            };
            3
        ];
        let mut a = World::from_units(units.clone());
        let mut b = World::from_units(units);
        a.move_group(&[0, 1, 2], &map, Vec2::new(7, 7)).unwrap();
        b.move_group(&[2, 0, 1, 0], &map, Vec2::new(7, 7)).unwrap();
        for _ in 0..20 {
            assert_eq!(a.state_hash(), b.state_hash());
            a.tick();
            b.tick();
        }
    }
    #[test]
    fn hash_includes_speed_and_route_topology() {
        let units = vec![Unit {
            position: Vec2::new(0, 0),
            goal: Vec2::new(0, 0),
            speed: 1,
        }];
        let mut faster = units.clone();
        faster[0].speed = 2;
        assert_ne!(
            World::from_units(units.clone()).state_hash(),
            World::from_units(faster).state_hash()
        );
        let map = NavigationMap::new(3, 3);
        let mut blocked = map.clone();
        blocked.set_walkable(Vec2::new(1, 0), false);
        let mut a = World::from_units(units.clone());
        let mut b = World::from_units(units);
        a.move_group(&[0], &map, Vec2::new(2, 2)).unwrap();
        b.move_group(&[0], &blocked, Vec2::new(2, 2)).unwrap();
        assert_ne!(a.state_hash(), b.state_hash());
    }
    #[test]
    fn same_seed_produces_same_state() {
        let mut a = World::seeded(10_000, 0x0052_4132_4e45);
        let mut b = World::seeded(10_000, 0x0052_4132_4e45);
        for _ in 0..300 {
            a.tick();
            b.tick();
        }
        assert_eq!(a.state_hash(), b.state_hash());
    }
    #[test]
    fn grid_query_is_deterministic_and_local() {
        let world = World::seeded(10_000, 7);
        let first = world.nearby_candidate_count(Vec2::new(2_048, 2_048), 128);
        assert_eq!(
            first,
            world.nearby_candidate_count(Vec2::new(2_048, 2_048), 128)
        );
        assert!(first < world.unit_count());
    }
    #[test]
    fn movement_reaches_goal_without_overshooting() {
        let mut unit = Unit {
            position: Vec2::new(0, 0),
            goal: Vec2::new(3, -3),
            speed: 2,
        };
        unit.move_towards_goal();
        assert_eq!(unit.position, Vec2::new(2, -2));
        unit.move_towards_goal();
        assert_eq!(unit.position, unit.goal);
        unit.move_towards_goal();
        assert_eq!(unit.position, unit.goal);
    }
}
