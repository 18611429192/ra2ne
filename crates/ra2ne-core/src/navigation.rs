//! Shared, deterministic four-way paths on a bounded walkability grid.
use crate::Vec2;
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct NavigationMap {
    width: usize,
    height: usize,
    walkable: Vec<bool>,
    capacity: Vec<usize>,
}

impl NavigationMap {
    pub fn new(width: usize, height: usize) -> Self {
        assert!(width > 0 && height > 0);
        assert!(width <= i32::MAX as usize && height <= i32::MAX as usize);
        let size = width.checked_mul(height).expect("map dimensions overflow");
        Self {
            width,
            height,
            walkable: vec![true; size],
            capacity: vec![usize::MAX; size],
        }
    }

    pub fn state_hash(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for value in [self.width as u64, self.height as u64].into_iter().chain(
            self.walkable
                .iter()
                .zip(&self.capacity)
                .flat_map(|(&w, &c)| {
                    [
                        u64::from(w),
                        if c == usize::MAX { u64::MAX } else { c as u64 },
                    ]
                }),
        ) {
            hash ^= value;
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
        hash
    }
    pub(crate) fn snapshot(&self) -> (usize, usize, impl Iterator<Item = (bool, usize)> + '_) {
        (
            self.width,
            self.height,
            self.walkable
                .iter()
                .copied()
                .zip(self.capacity.iter().copied()),
        )
    }

    fn index(&self, p: Vec2) -> Option<usize> {
        (p.x >= 0 && p.y >= 0 && (p.x as usize) < self.width && (p.y as usize) < self.height)
            .then(|| p.y as usize * self.width + p.x as usize)
    }

    pub fn is_traversable(&self, p: Vec2) -> bool {
        self.index(p)
            .is_some_and(|i| self.walkable[i] && self.capacity[i] > 0)
    }

    pub fn set_walkable(&mut self, p: Vec2, walkable: bool) -> bool {
        let Some(index) = self.index(p) else {
            return false;
        };
        self.walkable[index] = walkable;
        true
    }
    /// Maximum entries into this cell per simulation tick; zero closes it.
    /// This models bottleneck throughput, not physical occupancy or avoidance.
    pub fn set_capacity(&mut self, p: Vec2, capacity: usize) -> bool {
        let Some(index) = self.index(p) else {
            return false;
        };
        self.capacity[index] = capacity;
        true
    }

    /// One reverse BFS serves every member of a movement group. Stable neighbor
    /// ordering resolves equal-length routes identically on every client.
    pub fn route_to(&self, goal: Vec2) -> Option<SharedRoute> {
        let target = self.index(goal)?;
        if !self.walkable[target] || self.capacity[target] == 0 {
            return None;
        }
        let mut distance = vec![usize::MAX; self.walkable.len()];
        let mut queue = VecDeque::new();
        distance[target] = 0;
        queue.push_back(target);
        while let Some(index) = queue.pop_front() {
            for next in neighbors(index, self.width, self.height)
                .into_iter()
                .flatten()
            {
                if self.walkable[next] && self.capacity[next] > 0 && distance[next] == usize::MAX {
                    distance[next] = distance[index] + 1;
                    queue.push_back(next);
                }
            }
        }
        Some(SharedRoute {
            width: self.width,
            height: self.height,
            goal,
            distance,
            capacity: self.capacity.clone(),
        })
    }
}

#[derive(Debug)]
pub struct SharedRoute {
    width: usize,
    height: usize,
    pub goal: Vec2,
    distance: Vec<usize>,
    capacity: Vec<usize>,
}

impl SharedRoute {
    pub(crate) fn capacity_at(&self, p: Vec2) -> usize {
        self.capacity[p.y as usize * self.width + p.x as usize]
    }
    pub(crate) fn state_hash(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for value in [self.width, self.height]
            .into_iter()
            .chain(self.distance.iter().copied())
            .chain(self.capacity.iter().copied())
        {
            // Normalize the unreachable sentinel across 32/64-bit hosts.
            hash ^= if value == usize::MAX {
                u64::MAX
            } else {
                value as u64
            };
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }
    pub fn next_step(&self, p: Vec2) -> Option<Vec2> {
        if p.x < 0 || p.y < 0 || p.x as usize >= self.width || p.y as usize >= self.height {
            return None;
        }
        let index = p.y as usize * self.width + p.x as usize;
        let current = self.distance[index];
        if current == usize::MAX {
            return None;
        }
        if current == 0 {
            return Some(p);
        }
        neighbors(index, self.width, self.height)
            .into_iter()
            .flatten()
            .find(|&next| self.distance[next] < current)
            .map(|next| Vec2::new((next % self.width) as i32, (next / self.width) as i32))
    }
}

fn neighbors(index: usize, width: usize, height: usize) -> [Option<usize>; 4] {
    let x = index % width;
    let y = index / width;
    [
        (y > 0).then(|| index - width),
        (x > 0).then(|| index - 1),
        (x + 1 < width).then(|| index + 1),
        (y + 1 < height).then(|| index + width),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_crosses_only_bridge_opening() {
        let mut map = NavigationMap::new(7, 5);
        for y in 0..5 {
            map.set_walkable(Vec2::new(3, y), y == 2);
        }
        let route = map.route_to(Vec2::new(6, 0)).unwrap();
        let mut p = Vec2::new(0, 0);
        let mut crossed = false;
        for _ in 0..20 {
            let next = route.next_step(p).unwrap();
            assert_eq!((next.x - p.x).abs() + (next.y - p.y).abs(), 1);
            if next.x == 3 {
                assert_eq!(next.y, 2);
                crossed = true;
            }
            p = next;
            if p == route.goal {
                break;
            }
        }
        assert!(crossed);
        assert_eq!(p, route.goal);
    }

    #[test]
    fn blocked_and_unreachable_cells_are_rejected() {
        let mut map = NavigationMap::new(3, 3);
        for y in 0..3 {
            map.set_walkable(Vec2::new(1, y), false);
        }
        assert!(map.route_to(Vec2::new(1, 1)).is_none());
        assert!(map.route_to(Vec2::new(-1, 0)).is_none());
        let route = map.route_to(Vec2::new(2, 2)).unwrap();
        assert_eq!(route.next_step(Vec2::new(0, 0)), None);
        assert_eq!(route.next_step(Vec2::new(2, 2)), Some(Vec2::new(2, 2)));
    }
}
