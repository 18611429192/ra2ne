//! Self-contained, bounded engine saves. Original RA2 saves are not this format.
use super::*;
use ra2ne_core::snapshot::{Reader, Writer};
const LIMIT: usize = 128 * 1024 * 1024;
fn write_id(w: &mut Writer, id: EntityId) {
    w.u32(id.index);
    w.u32(id.generation);
}
fn read_id(r: &mut Reader<'_>) -> Result<EntityId, &'static str> {
    Ok(EntityId {
        index: r.u32()?,
        generation: r.u32()?,
    })
}
impl Skirmish {
    pub fn save(&self) -> Result<Vec<u8>, &'static str> {
        let mut w = Writer::new();
        w.0.extend_from_slice(b"RA2NEGS5");
        w.u64(self.tick);
        w.boolean(self.started_with_opponents);
        w.boolean(self.finished);
        w.boolean(self.winner.is_some());
        if let Some(owner) = self.winner {
            w.u32(owner);
        }
        w.u64(self.rules.max_entities as u64);
        w.u64(self.rules.units.len() as u64);
        for def in &self.rules.units {
            w.boolean(def.production.is_some());
            if let Some(p) = &def.production {
                w.u32(p.category as u32);
                w.boolean(p.factory_category.is_some());
                if let Some(c) = p.factory_category {
                    w.u32(c as u32);
                }
                w.u64(p.prerequisites.len() as u64);
                for group in &p.prerequisites {
                    w.u64(group.len() as u64);
                    for &i in group {
                        w.u64(i as u64);
                    }
                }
            }
            w.u32(def.armor as u32);
            w.bytes(def.name.as_bytes());
            w.u32(def.health);
            w.i32(def.speed);
            w.u32(def.cost);
            w.u32(def.build_ticks);
            w.i32(def.power);
            w.boolean(def.factory);
            w.boolean(def.harvester);
            for weapon in [&def.weapon, &def.secondary] {
                w.boolean(weapon.is_some());
                if let Some(weapon) = weapon {
                    for value in weapon.verses.0 {
                        w.u32(value);
                    }
                    w.u32(weapon.damage);
                    w.u32(weapon.range);
                    w.u32(weapon.reload_ticks);
                }
            }
        }
        w.bytes(&self.map.checkpoint()?);
        w.bytes(&self.movement.checkpoint()?);
        w.u64(self.players.len() as u64);
        for (&owner, player) in &self.players {
            w.u32(owner);
            w.u64(player.credits);
            w.boolean(player.defeated);
        }
        w.u64(self.slots.len() as u64);
        for slot in &self.slots {
            w.u32(slot.generation);
            w.boolean(slot.actor.is_some());
            if let Some(actor) = &slot.actor {
                w.u32(actor.owner);
                w.u64(actor.kind as u64);
                w.u32(actor.health);
                w.u32(actor.cooldown);
                w.u32(actor.cargo);
                w.boolean(actor.target.is_some());
                if let Some(target) = actor.target {
                    write_id(&mut w, target);
                }
            }
        }
        w.u64(self.resources.len() as u64);
        for (&(x, y), &amount) in &self.resources {
            w.i32(x);
            w.i32(y);
            w.u32(amount);
        }
        w.u64(self.harvesting.len() as u64);
        for (&id, order) in &self.harvesting {
            write_id(&mut w, id);
            w.i32(order.resource.x);
            w.i32(order.resource.y);
            write_id(&mut w, order.refinery);
            w.boolean(order.returning);
        }
        w.u64(self.production.len() as u64);
        for (&id, queue) in &self.production {
            write_id(&mut w, id);
            w.u64(queue.len() as u64);
            for job in queue {
                w.u64(job.kind as u64);
                w.u32(job.remaining);
                w.u32(job.paid);
            }
        }
        w.u64(self.state_hash());
        if w.0.len() > LIMIT {
            return Err("game save exceeds byte limit");
        }
        Ok(w.0)
    }
    pub fn load(bytes: &[u8]) -> Result<Self, &'static str> {
        let mut r = Reader::new(bytes)?;
        if r.take(8)? != b"RA2NEGS5" {
            return Err("unsupported game save");
        }
        let tick = r.u64()?;
        let started_with_opponents = r.boolean()?;
        let finished = r.boolean()?;
        let winner = if r.boolean()? { Some(r.u32()?) } else { None };
        let max_entities = r.count(100_000)?;
        let definitions = r.count(100_000)?;
        let mut units = Vec::with_capacity(definitions);
        let mut prerequisite_count = 0usize;
        for _ in 0..definitions {
            let production = if r.boolean()? {
                let category = ProductionCategory::from_index(r.u32()?)?;
                let factory_category = if r.boolean()? {
                    Some(ProductionCategory::from_index(r.u32()?)?)
                } else {
                    None
                };
                let n = r.count(1024)?;
                let mut prerequisites = Vec::with_capacity(n);
                for _ in 0..n {
                    let count = r.count(1024)?;
                    if count == 0 {
                        return Err("empty prerequisite group");
                    }
                    prerequisite_count = prerequisite_count
                        .checked_add(count)
                        .ok_or("prerequisite size overflow")?;
                    if prerequisite_count > 1_000_000 {
                        return Err("prerequisite total exceeds limit");
                    }
                    let mut group = Vec::with_capacity(count);
                    for _ in 0..count {
                        group.push(r.count(definitions)?);
                    }
                    prerequisites.push(group);
                }
                Some(ProductionRules {
                    category,
                    factory_category,
                    prerequisites,
                })
            } else {
                None
            };
            let armor = Armor::from_index(r.u32()?)?;
            let name = std::str::from_utf8(r.bytes(4096)?)
                .map_err(|_| "invalid save rule name")?
                .to_owned();
            let health = r.u32()?;
            let speed = r.i32()?;
            let cost = r.u32()?;
            let build_ticks = r.u32()?;
            let power = r.i32()?;
            let factory = r.boolean()?;
            let harvester = r.boolean()?;
            let mut read_weapon = || -> Result<Option<Weapon>, &'static str> {
                Ok(if r.boolean()? {
                    let mut values = [0; 11];
                    for value in &mut values {
                        *value = r.u32()?;
                    }
                    Some(Weapon {
                        verses: Verses(values),
                        damage: r.u32()?,
                        range: r.u32()?,
                        reload_ticks: r.u32()?,
                    })
                } else {
                    None
                })
            };
            let weapon = read_weapon()?;
            let secondary = read_weapon()?;
            units.push(UnitDef {
                production,
                armor,
                name,
                health,
                speed,
                cost,
                build_ticks,
                power,
                factory,
                harvester,
                weapon,
                secondary,
            });
        }
        let rules = Arc::new(Rules {
            units,
            max_entities,
        });
        rules.validate()?;
        let map = NavigationMap::restore_checkpoint(r.bytes(LIMIT)?)?;
        let mut movement = World::restore_checkpoint(r.bytes(LIMIT)?)?;
        movement.set_spatial_cell_size(8)?;
        if movement.tick_number() != tick {
            return Err("save tick mismatch");
        }
        let count = r.count(64)?;
        let mut players = BTreeMap::new();
        for _ in 0..count {
            let owner = r.u32()?;
            let player = Player {
                credits: r.u64()?,
                defeated: r.boolean()?,
            };
            if players.insert(owner, player).is_some() {
                return Err("duplicate save player");
            }
        }
        if players.is_empty()
            || winner.is_some_and(|owner| !players.contains_key(&owner))
            || winner.is_some() && !finished
        {
            return Err("invalid save outcome");
        }
        let count = r.count(max_entities)?;
        if count != movement.unit_count() {
            return Err("save entity count mismatch");
        }
        let mut slots = Vec::with_capacity(count);
        let mut free = BTreeSet::new();
        for index in 0..count {
            let generation = r.u32()?;
            let actor = if r.boolean()? {
                let owner = r.u32()?;
                let kind = r.count(definitions)?;
                let health = r.u32()?;
                let cooldown = r.u32()?;
                let cargo = r.u32()?;
                let target = if r.boolean()? {
                    Some(read_id(&mut r)?)
                } else {
                    None
                };
                let def = rules.units.get(kind).ok_or("invalid save unit kind")?;
                let unit = movement.unit(index).unwrap();
                if !players.contains_key(&owner)
                    || health == 0
                    || health > def.health
                    || cargo > 30
                    || unit.speed != def.speed
                    || !map.is_traversable(unit.position)
                {
                    return Err("invalid save actor");
                }
                if cooldown
                    > def
                        .weapon
                        .iter()
                        .chain(&def.secondary)
                        .map(|w| w.reload_ticks)
                        .max()
                        .unwrap_or(0)
                {
                    return Err("invalid save cooldown");
                }
                Some(Actor {
                    owner,
                    kind,
                    health,
                    cooldown,
                    cargo,
                    target,
                })
            } else {
                let unit = movement.unit(index).unwrap();
                if unit.speed != 0 || unit.goal != unit.position {
                    return Err("invalid dead save slot");
                }
                if generation < u32::MAX {
                    free.insert(index as u32);
                }
                None
            };
            slots.push(Slot { generation, actor });
        }
        let mut game = Self {
            movement,
            map,
            rules,
            slots,
            free,
            players,
            events: Vec::new(),
            production: BTreeMap::new(),
            harvesting: BTreeMap::new(),
            resources: BTreeMap::new(),
            tick,
            started_with_opponents,
            winner,
            finished,
        };
        let count = r.count(4 * 1024 * 1024)?;
        for _ in 0..count {
            let position = Vec2::new(r.i32()?, r.i32()?);
            let amount = r.u32()?;
            if !game.map.is_traversable(position)
                || game
                    .resources
                    .insert((position.x, position.y), amount)
                    .is_some()
            {
                return Err("invalid save resource");
            }
        }
        let count = r.count(max_entities)?;
        for _ in 0..count {
            let id = read_id(&mut r)?;
            let resource = Vec2::new(r.i32()?, r.i32()?);
            let refinery = read_id(&mut r)?;
            let returning = r.boolean()?;
            let actor = game.actor(id).ok_or("invalid saved harvester")?;
            if !game.rules.units[actor.kind].harvester
                || !game.resources.contains_key(&(resource.x, resource.y))
            {
                return Err("invalid saved harvest order");
            }
            if game
                .harvesting
                .insert(
                    id,
                    HarvestOrder {
                        resource,
                        refinery,
                        returning,
                    },
                )
                .is_some()
            {
                return Err("duplicate harvest order");
            }
        }
        let count = r.count(max_entities)?;
        for _ in 0..count {
            let id = read_id(&mut r)?;
            let length = r.count(32)?;
            let actor = game.actor(id).ok_or("invalid saved factory")?;
            if !game.rules.units[actor.kind].factory || length == 0 {
                return Err("invalid saved production queue");
            }
            let mut queue = VecDeque::new();
            for _ in 0..length {
                let kind = r.count(definitions)?;
                let remaining = r.u32()?;
                let paid = r.u32()?;
                let def = game
                    .rules
                    .units
                    .get(kind)
                    .ok_or("invalid production kind")?;
                game.check_factory_category(actor.kind, kind)?;
                if remaining > def.build_ticks || paid != def.cost {
                    return Err("invalid saved production job");
                }
                queue.push_back(Production {
                    kind,
                    remaining,
                    paid,
                });
            }
            if game.production.insert(id, queue).is_some() {
                return Err("duplicate production queue");
            }
        }
        let expected = r.u64()?;
        r.end()?;
        if game.state_hash() != expected {
            return Err("game save hash mismatch");
        }
        Ok(game)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn save_preserves_reused_generations_and_combat_cooldowns() {
        let mut game = crate::tests::game();
        let a = game.spawn(0, 0, Vec2::new(1, 1)).unwrap();
        game.spawn(1, 0, Vec2::new(2, 1)).unwrap();
        game.spawn(0, 0, Vec2::new(15, 1)).unwrap();
        game.spawn(1, 0, Vec2::new(15, 15)).unwrap();
        for _ in 0..3 {
            game.tick();
        }
        assert!(game.actor(a).is_none());
        assert!(!game.finished());
        let replacement = game.spawn(0, 0, Vec2::new(3, 3)).unwrap();
        assert_eq!(replacement.index, a.index);
        assert_ne!(replacement.generation, a.generation);
        let mut restored = Skirmish::load(&game.save().unwrap()).unwrap();
        assert!(restored.actor(a).is_none());
        assert!(restored.actor(replacement).is_some());
        for _ in 0..20 {
            game.tick();
            restored.tick();
            assert_eq!(game.state_hash(), restored.state_hash());
        }
    }
    #[test]
    fn save_restores_economy_orders_and_exact_future_simulation() {
        let mut game = super::super::tests::game();
        let def = &mut Arc::make_mut(&mut game.rules).units[0];
        def.factory = true;
        def.harvester = true;
        def.weapon = None;
        let depot = game.spawn(0, 0, Vec2::new(1, 1)).unwrap();
        let miner = game.spawn(0, 0, Vec2::new(2, 1)).unwrap();
        game.set_resource(Vec2::new(4, 1), 35).unwrap();
        game.harvest(0, &[miner], Vec2::new(4, 1), depot).unwrap();
        game.queue_production(0, depot, 0).unwrap();
        for _ in 0..5 {
            game.tick();
        }
        let bytes = game.save().unwrap();
        let mut restored = Skirmish::load(&bytes).unwrap();
        assert_eq!(game.state_hash(), restored.state_hash());
        for _ in 0..100 {
            game.tick();
            restored.tick();
            assert_eq!(game.state_hash(), restored.state_hash());
            assert_eq!(game.events(), restored.events());
        }
        for end in 0..bytes.len() {
            assert!(Skirmish::load(&bytes[..end]).is_err());
        }
        for index in 0..bytes.len() {
            let mut changed = bytes.clone();
            changed[index] ^= 0x80;
            assert!(
                Skirmish::load(&changed).is_err(),
                "accepted corruption at {index}"
            );
        }
    }
}
