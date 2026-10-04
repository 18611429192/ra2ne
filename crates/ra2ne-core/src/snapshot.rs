//! Bounded little-endian movement checkpoints. This is an engine format.
use super::*;
const MAX_BYTES: usize = 128 * 1024 * 1024;
const MAX_CELLS: usize = 4 * 1024 * 1024;
pub struct Writer(pub Vec<u8>);
impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}
impl Writer {
    pub fn new() -> Self {
        Self(Vec::new())
    }
    pub fn u64(&mut self, n: u64) {
        self.0.extend_from_slice(&n.to_le_bytes());
    }
    pub fn u32(&mut self, n: u32) {
        self.0.extend_from_slice(&n.to_le_bytes());
    }
    pub fn i32(&mut self, n: i32) {
        self.u32(n as u32);
    }
    pub fn boolean(&mut self, n: bool) {
        self.0.push(u8::from(n));
    }
    pub fn bytes(&mut self, value: &[u8]) {
        self.u64(value.len() as u64);
        self.0.extend_from_slice(value);
    }
}
pub struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, &'static str> {
        if bytes.len() > MAX_BYTES {
            return Err("checkpoint exceeds byte limit");
        }
        Ok(Self { bytes, offset: 0 })
    }
    pub fn take(&mut self, count: usize) -> Result<&'a [u8], &'static str> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or("checkpoint offset overflow")?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or("truncated checkpoint")?;
        self.offset = end;
        Ok(bytes)
    }
    pub fn u64(&mut self) -> Result<u64, &'static str> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    pub fn u32(&mut self) -> Result<u32, &'static str> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub fn i32(&mut self) -> Result<i32, &'static str> {
        Ok(self.u32()? as i32)
    }
    pub fn boolean(&mut self) -> Result<bool, &'static str> {
        match self.take(1)?[0] {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err("invalid checkpoint boolean"),
        }
    }
    pub fn count(&mut self, limit: usize) -> Result<usize, &'static str> {
        let value = usize::try_from(self.u64()?).map_err(|_| "checkpoint count overflow")?;
        if value > limit {
            return Err("checkpoint count exceeds limit");
        }
        Ok(value)
    }
    pub fn bytes(&mut self, limit: usize) -> Result<&'a [u8], &'static str> {
        let count = self.count(limit)?;
        self.take(count)
    }
    pub fn end(&self) -> Result<(), &'static str> {
        if self.offset != self.bytes.len() {
            Err("trailing checkpoint bytes")
        } else {
            Ok(())
        }
    }
}
fn normalized(n: usize) -> u64 {
    if n == usize::MAX { u64::MAX } else { n as u64 }
}
fn host(n: u64) -> Result<usize, &'static str> {
    if n == u64::MAX {
        Ok(usize::MAX)
    } else {
        usize::try_from(n).map_err(|_| "checkpoint host overflow")
    }
}
impl NavigationMap {
    pub fn checkpoint(&self) -> Result<Vec<u8>, &'static str> {
        let mut w = Writer::new();
        w.0.extend_from_slice(b"RA2NEMP1");
        let (width, height, cells) = self.snapshot();
        if width.checked_mul(height).is_none_or(|n| n > MAX_CELLS) {
            return Err("map checkpoint exceeds limits");
        }
        w.u64(width as u64);
        w.u64(height as u64);
        for (walkable, capacity) in cells {
            w.boolean(walkable);
            w.u64(normalized(capacity));
        }
        w.u64(self.state_hash());
        Ok(w.0)
    }
    pub fn restore_checkpoint(bytes: &[u8]) -> Result<Self, &'static str> {
        let mut r = Reader::new(bytes)?;
        if r.take(8)? != b"RA2NEMP1" {
            return Err("unsupported map checkpoint");
        }
        let width = r.count(MAX_CELLS)?;
        let height = r.count(MAX_CELLS)?;
        if width == 0 || height == 0 || width.checked_mul(height).is_none_or(|n| n > MAX_CELLS) {
            return Err("map checkpoint exceeds limits");
        }
        let mut map = Self::new(width, height);
        for y in 0..height {
            for x in 0..width {
                map.set_walkable(Vec2::new(x as i32, y as i32), r.boolean()?);
                map.set_capacity(Vec2::new(x as i32, y as i32), host(r.u64()?)?);
            }
        }
        if map.state_hash() != r.u64()? {
            return Err("map checkpoint hash mismatch");
        }
        r.end()?;
        Ok(map)
    }
}
impl World {
    pub fn checkpoint(&self) -> Result<Vec<u8>, &'static str> {
        if self.units.len() > 100_000 {
            return Err("checkpoint unit limit");
        }
        let mut writer = Writer::new();
        writer.0.extend_from_slice(b"RA2NEMV1");
        writer.u64(self.tick);
        writer.u64(self.units.len() as u64);
        for unit in &self.units {
            for value in [
                unit.position.x,
                unit.position.y,
                unit.goal.x,
                unit.goal.y,
                unit.speed,
            ] {
                writer.i32(value);
            }
        }
        let mut routes = Vec::new();
        let mut ids = BTreeMap::new();
        for order in self.orders.iter().flatten() {
            let pointer = Arc::as_ptr(&order.route);
            ids.entry(pointer).or_insert_with(|| {
                routes.push(&order.route);
                routes.len() - 1
            });
        }
        writer.u64(routes.len() as u64);
        let mut total_cells = 0usize;
        for route in routes {
            let (width, height, goal, distance, capacity) = route.checkpoint_parts();
            total_cells = total_cells
                .checked_add(
                    width
                        .checked_mul(height)
                        .ok_or("checkpoint dimensions overflow")?,
                )
                .ok_or("checkpoint cells overflow")?;
            if total_cells > MAX_CELLS {
                return Err("checkpoint route exceeds limits");
            }
            writer.u64(width as u64);
            writer.u64(height as u64);
            writer.i32(goal.x);
            writer.i32(goal.y);
            for &value in distance {
                writer.u64(normalized(value));
            }
            for &value in capacity {
                writer.u64(normalized(value));
            }
        }
        for order in &self.orders {
            writer.boolean(order.is_some());
            if let Some(order) = order {
                writer.u64(ids[&Arc::as_ptr(&order.route)] as u64);
                writer.0.push(order.state as u8);
            }
        }
        writer.u64(self.state_hash());
        if writer.0.len() > MAX_BYTES {
            return Err("checkpoint exceeds byte limit");
        }
        Ok(writer.0)
    }
    pub fn restore_checkpoint(bytes: &[u8]) -> Result<Self, &'static str> {
        let mut reader = Reader::new(bytes)?;
        if reader.take(8)? != b"RA2NEMV1" {
            return Err("unsupported movement checkpoint");
        }
        let tick = reader.u64()?;
        if tick > 1_000_000_000_000 {
            return Err("checkpoint tick exceeds limit");
        }
        let count = reader.count(100_000)?;
        let mut units = Vec::with_capacity(count);
        for _ in 0..count {
            let unit = Unit {
                position: Vec2::new(reader.i32()?, reader.i32()?),
                goal: Vec2::new(reader.i32()?, reader.i32()?),
                speed: reader.i32()?,
            };
            if unit.speed < 0 {
                return Err("invalid checkpoint speed");
            }
            units.push(unit);
        }
        let route_count = reader.count(count)?;
        let mut routes = Vec::with_capacity(route_count);
        let mut total_cells = 0usize;
        for _ in 0..route_count {
            let width = reader.count(MAX_CELLS)?;
            let height = reader.count(MAX_CELLS)?;
            let cells = width
                .checked_mul(height)
                .ok_or("checkpoint dimensions overflow")?;
            total_cells = total_cells
                .checked_add(cells)
                .ok_or("checkpoint cells overflow")?;
            if width == 0 || height == 0 || total_cells > MAX_CELLS {
                return Err("checkpoint route exceeds limits");
            }
            let goal = Vec2::new(reader.i32()?, reader.i32()?);
            let mut distance = Vec::with_capacity(cells);
            let mut capacity = Vec::with_capacity(cells);
            for _ in 0..cells {
                distance.push(host(reader.u64()?)?);
            }
            for _ in 0..cells {
                capacity.push(host(reader.u64()?)?);
            }
            routes.push(Arc::new(SharedRoute::restore_parts(
                width, height, goal, distance, capacity,
            )?));
        }
        let mut orders = Vec::with_capacity(count);
        for unit in &units {
            orders.push(if reader.boolean()? {
                let index = reader.count(route_count)?;
                let route = routes
                    .get(index)
                    .ok_or("invalid checkpoint route id")?
                    .clone();
                let state = match reader.take(1)?[0] {
                    0 => MovementState::Idle,
                    1 => MovementState::Moving,
                    2 => MovementState::Arrived,
                    3 => MovementState::Unreachable,
                    4 => MovementState::Waiting,
                    _ => return Err("invalid movement state"),
                };
                if route.goal != unit.goal {
                    return Err("checkpoint order goal mismatch");
                }
                Some(MovementOrder { route, state })
            } else {
                None
            });
        }
        let expected = reader.u64()?;
        reader.end()?;
        let mut world = Self::from_units(units);
        world.tick = tick;
        world.orders = orders;
        if world.state_hash() != expected {
            return Err("movement checkpoint hash mismatch");
        }
        Ok(world)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checkpoint_preserves_shared_routes_waiting_and_future_ticks() {
        let mut map = NavigationMap::new(8, 1);
        map.set_capacity(Vec2::new(3, 0), 1);
        let mut world = World::from_units(vec![
            Unit {
                position: Vec2::new(0, 0),
                goal: Vec2::new(0, 0),
                speed: 1
            };
            10
        ]);
        world
            .move_group(&(0..10).collect::<Vec<_>>(), &map, Vec2::new(7, 0))
            .unwrap();
        for _ in 0..4 {
            world.tick();
        }
        let bytes = world.checkpoint().unwrap();
        let mut restored = World::restore_checkpoint(&bytes).unwrap();
        assert_eq!(restored.state_hash(), world.state_hash());
        for _ in 0..20 {
            world.tick();
            restored.tick();
            assert_eq!(restored.state_hash(), world.state_hash());
        }
        for end in 0..bytes.len() {
            assert!(World::restore_checkpoint(&bytes[..end]).is_err());
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(World::restore_checkpoint(&trailing).is_err());
    }
}
