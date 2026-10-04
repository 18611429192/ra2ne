//! Deterministic game systems foundation. Timings and rule definitions are
//! explicit engine values; the complete original RA2 rules adapter is pending.
pub mod commands;
mod economy;
pub mod network;
mod replay_file;
pub mod rule_import;
mod save;
use economy::HarvestOrder;
pub use ra2ne_assets::armor::{Armor, Verses};
use ra2ne_core::{Unit, Vec2, World, navigation::NavigationMap};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct EntityId {
    pub index: u32,
    pub generation: u32,
}
#[derive(Clone, Debug)]
pub struct Weapon {
    pub verses: Verses,
    pub damage: u32,
    pub range: u32,
    pub reload_ticks: u32,
}
#[derive(Clone, Debug)]
pub struct UnitDef {
    pub armor: Armor,
    pub name: String,
    pub health: u32,
    pub speed: i32,
    pub cost: u32,
    pub weapon: Option<Weapon>,
    pub build_ticks: u32,
    pub power: i32,
    pub factory: bool,
    pub harvester: bool,
}
#[derive(Clone, Debug)]
pub struct Rules {
    pub units: Vec<UnitDef>,
    pub max_entities: usize,
}
impl Rules {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.units.is_empty()
            || self.units.len() > 100_000
            || self.max_entities == 0
            || self.max_entities > 100_000
        {
            return Err("invalid game rule limits");
        }
        let mut names = BTreeSet::new();
        for unit in &self.units {
            if unit.name.is_empty()
                || unit.name.len() > 4096
                || !names.insert(unit.name.to_ascii_lowercase())
                || unit.health == 0
                || unit.speed < 0
                || unit.speed > 16
                || unit.build_ticks == 0
                || unit
                    .weapon
                    .as_ref()
                    .is_some_and(|w| w.range == 0 || w.range > 1024 || w.reload_ticks == 0)
            {
                return Err("invalid unit rule");
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Player {
    pub credits: u64,
    pub defeated: bool,
}
#[derive(Clone, Debug)]
pub struct Actor {
    pub owner: u32,
    pub kind: usize,
    pub health: u32,
    pub target: Option<EntityId>,
    pub cooldown: u32,
    pub cargo: u32,
}
#[derive(Debug)]
struct Slot {
    generation: u32,
    actor: Option<Actor>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Event {
    Shot {
        source: EntityId,
        target: EntityId,
        damage: u32,
    },
    Destroyed {
        entity: EntityId,
    },
    Produced {
        factory: EntityId,
        entity: EntityId,
    },
    Draw,
    Victory {
        player: u32,
    },
}
#[derive(Clone, Debug)]
pub struct Production {
    pub kind: usize,
    pub remaining: u32,
    pub paid: u32,
}
#[derive(Debug)]
pub struct Skirmish {
    movement: World,
    map: NavigationMap,
    rules: Arc<Rules>,
    slots: Vec<Slot>,
    free: BTreeSet<u32>,
    players: BTreeMap<u32, Player>,
    events: Vec<Event>,
    production: BTreeMap<EntityId, VecDeque<Production>>,
    harvesting: BTreeMap<EntityId, HarvestOrder>,
    resources: BTreeMap<(i32, i32), u32>,
    tick: u64,
    started_with_opponents: bool,
    winner: Option<u32>,
    finished: bool,
}
impl Skirmish {
    pub fn new(
        rules: Arc<Rules>,
        map: NavigationMap,
        players: BTreeMap<u32, Player>,
    ) -> Result<Self, &'static str> {
        rules.validate()?;
        if players.is_empty() || players.len() > 64 {
            return Err("empty player roster");
        }
        let mut movement = World::from_units(Vec::new());
        movement.set_spatial_cell_size(8)?;
        Ok(Self {
            movement,
            map,
            rules,
            slots: Vec::new(),
            free: BTreeSet::new(),
            players,
            events: Vec::new(),
            production: BTreeMap::new(),
            harvesting: BTreeMap::new(),
            resources: BTreeMap::new(),
            tick: 0,
            started_with_opponents: false,
            winner: None,
            finished: false,
        })
    }
    pub fn movement(&self) -> &World {
        &self.movement
    }
    pub fn rules(&self) -> &Rules {
        &self.rules
    }
    pub fn map(&self) -> &NavigationMap {
        &self.map
    }
    pub fn players(&self) -> &BTreeMap<u32, Player> {
        &self.players
    }
    pub fn events(&self) -> &[Event] {
        &self.events
    }
    pub fn tick_number(&self) -> u64 {
        self.tick
    }
    pub fn finished(&self) -> bool {
        self.finished
    }
    pub fn winner(&self) -> Option<u32> {
        self.winner
    }
    pub fn actor(&self, id: EntityId) -> Option<&Actor> {
        let slot = self.slots.get(id.index as usize)?;
        (slot.generation == id.generation)
            .then_some(slot.actor.as_ref())
            .flatten()
    }
    pub fn entity_at(&self, index: usize) -> Option<EntityId> {
        let slot = self.slots.get(index)?;
        slot.actor.as_ref()?;
        Some(EntityId {
            index: index as u32,
            generation: slot.generation,
        })
    }
    pub fn entities(&self) -> impl Iterator<Item = (EntityId, &Actor)> + '_ {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            slot.actor.as_ref().map(|actor| {
                (
                    EntityId {
                        index: index as u32,
                        generation: slot.generation,
                    },
                    actor,
                )
            })
        })
    }
    /// Atomic initial population: one spatial rebuild for the entire roster.
    pub fn populate(
        &mut self,
        entries: &[(u32, usize, Vec2)],
    ) -> Result<Vec<EntityId>, &'static str> {
        if !self.slots.is_empty() || self.tick != 0 {
            return Err("population requires an empty initial game");
        }
        if entries.len() > self.rules.max_entities {
            return Err("entity capacity reached");
        }
        let mut units = Vec::with_capacity(entries.len());
        let mut slots = Vec::with_capacity(entries.len());
        let mut owners = BTreeSet::new();
        for &(owner, kind, position) in entries {
            if self.finished {
                return Err("game finished");
            }
            if !self.players.contains_key(&owner) || self.players[&owner].defeated {
                return Err("unknown or defeated player");
            }
            let definition = self.rules.units.get(kind).ok_or("unknown unit type")?;
            if !self.map.is_traversable(position) {
                return Err("spawn outside walkable map");
            }
            units.push(Unit {
                position,
                goal: position,
                speed: definition.speed,
            });
            slots.push(Slot {
                generation: 0,
                actor: Some(Actor {
                    owner,
                    kind,
                    health: definition.health,
                    target: None,
                    cooldown: 0,
                    cargo: 0,
                }),
            });
            owners.insert(owner);
        }
        self.movement.spawn_units(&units)?;
        self.slots = slots;
        self.started_with_opponents = owners.len() > 1;
        Ok((0..entries.len())
            .map(|index| EntityId {
                index: index as u32,
                generation: 0,
            })
            .collect())
    }
    pub fn spawn(
        &mut self,
        owner: u32,
        kind: usize,
        position: Vec2,
    ) -> Result<EntityId, &'static str> {
        if self.finished {
            return Err("game finished");
        }
        if !self.players.contains_key(&owner) || self.players[&owner].defeated {
            return Err("unknown or defeated player");
        }
        let definition = self.rules.units.get(kind).ok_or("unknown unit type")?;
        if !self.map.is_traversable(position) {
            return Err("spawn outside walkable map");
        }
        if self.free.is_empty() && self.slots.len() >= self.rules.max_entities {
            return Err("entity capacity reached");
        }
        let actor = Actor {
            owner,
            kind,
            health: definition.health,
            target: None,
            cooldown: 0,
            cargo: 0,
        };
        let unit = Unit {
            position,
            goal: position,
            speed: definition.speed,
        };
        let id = if let Some(index) = self.free.pop_first() {
            self.movement.replace_unit(index as usize, unit)?;
            let slot = &mut self.slots[index as usize];
            slot.actor = Some(actor);
            EntityId {
                index,
                generation: slot.generation,
            }
        } else {
            let index = self.slots.len() as u32;
            self.movement.spawn_units(&[unit])?;
            self.slots.push(Slot {
                generation: 0,
                actor: Some(actor),
            });
            EntityId {
                index,
                generation: 0,
            }
        };
        self.started_with_opponents |= self
            .entities()
            .map(|(_, a)| a.owner)
            .collect::<BTreeSet<_>>()
            .len()
            > 1;
        Ok(id)
    }
    fn selected(&self, player: u32, ids: &[EntityId]) -> Result<Vec<usize>, &'static str> {
        if self.finished {
            return Err("game finished");
        }
        if !self.players.contains_key(&player) || self.players[&player].defeated {
            return Err("unknown or defeated player");
        }
        let mut result = BTreeSet::new();
        for &id in ids {
            let actor = self.actor(id).ok_or("stale or missing entity")?;
            if actor.owner != player {
                return Err("unit belongs to another player");
            }
            result.insert(id.index as usize);
        }
        Ok(result.into_iter().collect())
    }
    pub fn move_units(
        &mut self,
        player: u32,
        ids: &[EntityId],
        goal: Vec2,
    ) -> Result<(), &'static str> {
        let indexes = self.selected(player, ids)?;
        self.movement.move_group(&indexes, &self.map, goal)?;
        for &index in &indexes {
            self.harvesting.remove(&self.entity_at(index).unwrap());
        }
        for index in indexes {
            self.slots[index].actor.as_mut().unwrap().target = None;
        }
        Ok(())
    }
    pub fn stop_units(&mut self, player: u32, ids: &[EntityId]) -> Result<(), &'static str> {
        let indexes = self.selected(player, ids)?;
        self.movement.stop_group(&indexes)?;
        for &index in &indexes {
            self.harvesting.remove(&self.entity_at(index).unwrap());
        }
        for index in indexes {
            self.slots[index].actor.as_mut().unwrap().target = None;
        }
        Ok(())
    }
    pub fn attack(
        &mut self,
        player: u32,
        ids: &[EntityId],
        target: EntityId,
    ) -> Result<(), &'static str> {
        let indexes = self.selected(player, ids)?;
        let victim = self.actor(target).ok_or("stale or missing target")?;
        if victim.owner == player {
            return Err("friendly target");
        }
        if indexes.iter().any(|&index| {
            self.rules.units[self.slots[index].actor.as_ref().unwrap().kind]
                .weapon
                .is_none()
        }) {
            return Err("selected unit has no weapon");
        }
        let armor = self.rules.units[victim.kind].armor;
        if indexes.iter().any(|&index| {
            !self.rules.units[self.slots[index].actor.as_ref().unwrap().kind]
                .weapon
                .as_ref()
                .unwrap()
                .verses
                .can_target(armor)
        }) {
            return Err("selected weapon cannot target this armor");
        }
        self.movement.stop_group(&indexes)?;
        for &index in &indexes {
            self.harvesting.remove(&self.entity_at(index).unwrap());
        }
        for index in indexes {
            self.slots[index].actor.as_mut().unwrap().target = Some(target);
        }
        Ok(())
    }
    pub fn production(&self, factory: EntityId) -> Option<&VecDeque<Production>> {
        self.production.get(&factory)
    }
    /// Payment is reserved when queued. A cancelled job refunds its reservation.
    pub fn queue_production(
        &mut self,
        player: u32,
        factory: EntityId,
        kind: usize,
    ) -> Result<(), &'static str> {
        self.selected(player, &[factory])?;
        if self.finished {
            return Err("game finished");
        }
        let actor = self.actor(factory).unwrap();
        if !self.rules.units[actor.kind].factory {
            return Err("entity is not a factory");
        }
        let definition = self.rules.units.get(kind).ok_or("unknown unit type")?;
        if self.production.get(&factory).is_some_and(|q| q.len() >= 32) {
            return Err("production queue full");
        }
        let account = self.players.get_mut(&player).unwrap();
        if account.credits < u64::from(definition.cost) {
            return Err("insufficient credits");
        }
        account.credits -= u64::from(definition.cost);
        self.production
            .entry(factory)
            .or_default()
            .push_back(Production {
                kind,
                remaining: definition.build_ticks,
                paid: definition.cost,
            });
        Ok(())
    }
    pub fn cancel_production(
        &mut self,
        player: u32,
        factory: EntityId,
        index: usize,
    ) -> Result<(), &'static str> {
        self.selected(player, &[factory])?;
        let queue = self
            .production
            .get_mut(&factory)
            .ok_or("empty production queue")?;
        let job = queue.remove(index).ok_or("unknown production job")?;
        let account = self.players.get_mut(&player).unwrap();
        account.credits = account.credits.saturating_add(u64::from(job.paid));
        if queue.is_empty() {
            self.production.remove(&factory);
        }
        Ok(())
    }
    pub fn power_balance(&self, player: u32) -> i64 {
        self.entities()
            .filter(|(_, a)| a.owner == player)
            .map(|(_, a)| i64::from(self.rules.units[a.kind].power))
            .sum()
    }
    fn advance_production(&mut self) {
        let factories: Vec<_> = self.production.keys().copied().collect();
        for factory in factories {
            let Some(actor) = self.actor(factory) else {
                continue;
            };
            let owner = actor.owner;
            if self.power_balance(owner) < 0 {
                continue;
            }
            let job = self
                .production
                .get_mut(&factory)
                .unwrap()
                .front_mut()
                .unwrap();
            job.remaining = job.remaining.saturating_sub(1);
            if job.remaining != 0 {
                continue;
            }
            let kind = job.kind;
            let position = self.movement.unit(factory.index as usize).unwrap().position;
            // Deterministic adjacent exit; a blocked exit retains the paid job.
            let exit = [(1, 0), (0, 1), (-1, 0), (0, -1)]
                .into_iter()
                .map(|(x, y)| Vec2::new(position.x + x, position.y + y))
                .find(|&p| {
                    self.map.is_traversable(p)
                        && !self.movement.nearby_candidates(p, 0).into_iter().any(|i| {
                            self.entity_at(i).is_some()
                                && self.movement.unit(i).unwrap().position == p
                        })
                });
            if let Some(exit) = exit
                && let Ok(entity) = self.spawn(owner, kind, exit)
            {
                let queue = self.production.get_mut(&factory).unwrap();
                queue.pop_front();
                if queue.is_empty() {
                    self.production.remove(&factory);
                }
                self.events.push(Event::Produced { factory, entity });
            }
        }
    }
    /// Intent generation uses a snapshot. All shots in a tick resolve together;
    /// mutually lethal combat is independent of actor iteration order.
    pub fn tick(&mut self) {
        self.events.clear();
        if self.finished {
            return;
        }
        self.advance_pursuit();
        self.movement.tick();
        let mut shots = Vec::new();
        for (id, actor) in self.entities() {
            if actor.cooldown > 1 {
                continue;
            }
            let Some(weapon) = &self.rules.units[actor.kind].weapon else {
                continue;
            };
            let position = self.movement.unit(id.index as usize).unwrap().position;
            let target = actor.target.filter(|&target| {
                self.in_range(actor.owner, position, target, weapon.range)
                    && weapon
                        .verses
                        .can_target(self.rules.units[self.actor(target).unwrap().kind].armor)
            });
            let target = target.or_else(|| {
                self.movement
                    .nearby_candidates(position, weapon.range as i32)
                    .into_iter()
                    .filter_map(|index| self.entity_at(index))
                    .filter(|&target| self.in_range(actor.owner, position, target, weapon.range))
                    .filter(|&target| {
                        weapon.verses.passive_acquire(
                            self.rules.units[self.actor(target).unwrap().kind].armor,
                        )
                    })
                    .min_by_key(|target| {
                        (
                            distance_squared(
                                position,
                                self.movement.unit(target.index as usize).unwrap().position,
                            ),
                            *target,
                        )
                    })
            });
            if let Some(target) = target {
                let armor = self.rules.units[self.actor(target).unwrap().kind].armor;
                shots.push((
                    id,
                    target,
                    weapon.verses.damage(weapon.damage, armor),
                    weapon.reload_ticks,
                ));
            }
        }
        for slot in &mut self.slots {
            if let Some(actor) = &mut slot.actor {
                actor.cooldown = actor.cooldown.saturating_sub(1);
            }
        }
        let mut damage = BTreeMap::<EntityId, u64>::new();
        for (source, target, amount, reload) in shots {
            self.slots[source.index as usize]
                .actor
                .as_mut()
                .unwrap()
                .cooldown = reload;
            *damage.entry(target).or_default() += u64::from(amount);
            self.events.push(Event::Shot {
                source,
                target,
                damage: amount,
            });
        }
        let mut dead = Vec::new();
        for (target, amount) in damage {
            let actor = self.slots[target.index as usize].actor.as_mut().unwrap();
            actor.health = actor
                .health
                .saturating_sub(amount.min(u64::from(u32::MAX)) as u32);
            if actor.health == 0 {
                dead.push(target);
            }
        }
        if !dead.is_empty() {
            let replacements: Vec<_> = dead
                .iter()
                .map(|id| {
                    let position = self.movement.unit(id.index as usize).unwrap().position;
                    (
                        id.index as usize,
                        Unit {
                            position,
                            goal: position,
                            speed: 0,
                        },
                    )
                })
                .collect();
            self.movement.replace_units(&replacements).unwrap();
            for id in dead {
                self.destroy(id);
            }
        }
        self.advance_harvesting();
        self.advance_production();
        self.tick += 1;
        if self.started_with_opponents {
            let alive: BTreeSet<_> = self.entities().map(|(_, a)| a.owner).collect();
            for (&owner, player) in &mut self.players {
                if !alive.contains(&owner) {
                    player.defeated = true;
                }
            }
            if alive.len() == 1 {
                let player = *alive.first().unwrap();
                self.winner = Some(player);
                self.finished = true;
                self.events.push(Event::Victory { player });
            } else if alive.is_empty() {
                self.finished = true;
                self.events.push(Event::Draw);
            }
        }
    }
    fn advance_pursuit(&mut self) {
        let mut groups = BTreeMap::<EntityId, Vec<usize>>::new();
        let mut stops = Vec::new();
        let mut clear = Vec::new();
        for (id, actor) in self.entities() {
            let Some(target) = actor.target else {
                continue;
            };
            let Some(victim) = self.actor(target) else {
                clear.push(id.index as usize);
                continue;
            };
            let definition = &self.rules.units[actor.kind];
            let Some(weapon) = &definition.weapon else {
                continue;
            };
            let unit = self.movement.unit(id.index as usize).unwrap();
            if self.in_range(actor.owner, unit.position, target, weapon.range) {
                if unit.goal != unit.position {
                    stops.push(id.index as usize);
                }
            } else if definition.speed > 0 && victim.owner != actor.owner {
                let goal = self.movement.unit(target.index as usize).unwrap().position;
                if unit.goal != goal {
                    groups.entry(target).or_default().push(id.index as usize);
                }
            }
        }
        self.movement.stop_group(&stops).unwrap();
        for index in clear {
            self.slots[index].actor.as_mut().unwrap().target = None;
        }
        for (target, indexes) in groups {
            let goal = self.movement.unit(target.index as usize).unwrap().position;
            let _ = self.movement.move_group(&indexes, &self.map, goal);
        }
    }
    fn in_range(&self, owner: u32, position: Vec2, target: EntityId, range: u32) -> bool {
        self.actor(target).is_some_and(|actor| {
            actor.owner != owner
                && distance_squared(
                    position,
                    self.movement.unit(target.index as usize).unwrap().position,
                ) <= u64::from(range) * u64::from(range)
        })
    }
    fn destroy(&mut self, id: EntityId) {
        self.production.remove(&id);
        self.harvesting.remove(&id);
        let slot = &mut self.slots[id.index as usize];
        slot.actor = None;
        if slot.generation < u32::MAX {
            slot.generation += 1;
            if slot.generation < u32::MAX {
                self.free.insert(id.index);
            }
        }
        self.events.push(Event::Destroyed { entity: id });
    }
    pub fn state_hash(&self) -> u64 {
        let mut hash = self.movement.state_hash();
        let mut put = |v: u64| {
            hash ^= v;
            hash = hash.wrapping_mul(0x100_0000_01b3);
        };
        put(self.tick);
        put(u64::from(self.finished));
        put(self.resources.len() as u64);
        for (&(x, y), &amount) in &self.resources {
            put(x as u32 as u64);
            put(y as u32 as u64);
            put(u64::from(amount));
        }
        put(self.harvesting.len() as u64);
        for (id, order) in &self.harvesting {
            put(u64::from(id.index));
            put(u64::from(id.generation));
            put(order.resource.x as u32 as u64);
            put(order.resource.y as u32 as u64);
            put(u64::from(order.refinery.index));
            put(u64::from(order.refinery.generation));
            put(u64::from(order.returning));
        }
        put(self.map.state_hash());
        put(self.rules.max_entities as u64);
        for (factory, queue) in &self.production {
            put(u64::from(factory.index));
            put(u64::from(factory.generation));
            put(queue.len() as u64);
            for job in queue {
                put(job.kind as u64);
                put(u64::from(job.remaining));
                put(u64::from(job.paid));
            }
        }
        put(u64::from(self.started_with_opponents));
        put(self.winner.map_or(u64::MAX, u64::from));
        put(self.players.len() as u64);
        put(self.slots.len() as u64);
        put(self.rules.units.len() as u64);
        put(self.production.len() as u64);
        for (&owner, p) in &self.players {
            put(u64::from(owner));
            put(p.credits);
            put(u64::from(p.defeated));
        }
        for slot in &self.slots {
            put(u64::from(slot.generation));
            put(u64::from(slot.actor.is_some()));
            if let Some(a) = &slot.actor {
                put(u64::from(a.owner));
                put(a.kind as u64);
                put(u64::from(a.health));
                put(u64::from(a.cooldown));
                put(u64::from(a.cargo));
                put(a.target.map_or(u64::MAX, |t| {
                    (u64::from(t.generation) << 32) | u64::from(t.index)
                }));
            }
        }
        for d in &self.rules.units {
            put(d.armor as u64);
            for &b in d.name.as_bytes() {
                put(u64::from(b));
            }
            put(d.name.len() as u64);
            put(u64::from(d.health));
            put(d.speed as u32 as u64);
            put(u64::from(d.cost));
            put(u64::from(d.build_ticks));
            put(d.power as u32 as u64);
            put(u64::from(d.factory));
            put(u64::from(d.harvester));
            put(u64::from(d.weapon.is_some()));
            if let Some(w) = &d.weapon {
                for value in w.verses.0 {
                    put(u64::from(value));
                }
                put(u64::from(w.damage));
                put(u64::from(w.range));
                put(u64::from(w.reload_ticks));
            }
        }
        hash
    }
}
fn distance_squared(a: Vec2, b: Vec2) -> u64 {
    let x = i64::from(a.x) - i64::from(b.x);
    let y = i64::from(a.y) - i64::from(b.y);
    x.unsigned_abs()
        .saturating_mul(x.unsigned_abs())
        .saturating_add(y.unsigned_abs().saturating_mul(y.unsigned_abs()))
}
#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn game() -> Skirmish {
        let rules = Arc::new(Rules {
            units: vec![UnitDef {
                armor: Armor::None,
                name: "tank".into(),
                health: 100,
                speed: 1,
                cost: 100,
                weapon: Some(Weapon {
                    verses: Verses::default(),
                    damage: 60,
                    range: 4,
                    reload_ticks: 2,
                }),
                build_ticks: 10,
                power: 0,
                factory: false,
                harvester: false,
            }],
            max_entities: 100,
        });
        Skirmish::new(
            rules,
            NavigationMap::new(20, 20),
            BTreeMap::from([
                (
                    0,
                    Player {
                        credits: 1000,
                        defeated: false,
                    },
                ),
                (
                    1,
                    Player {
                        credits: 1000,
                        defeated: false,
                    },
                ),
            ]),
        )
        .unwrap()
    }
    #[test]
    fn armor_immunity_skips_nearest_target_and_attack_rejection_is_atomic() {
        let mut game = game();
        let rules = Arc::make_mut(&mut game.rules);
        let mut immune = rules.units[0].clone();
        immune.name = "immune".into();
        immune.armor = Armor::Heavy;
        immune.weapon = None;
        rules.units.push(immune);
        rules.units[0].weapon.as_mut().unwrap().verses.0[Armor::Heavy as usize] = 0;
        let a = game.spawn(0, 0, Vec2::new(1, 1)).unwrap();
        let b = game.spawn(1, 1, Vec2::new(2, 1)).unwrap();
        let c = game.spawn(1, 0, Vec2::new(3, 1)).unwrap();
        let before = game.state_hash();
        assert!(game.attack(0, &[a], b).is_err());
        assert_eq!(game.state_hash(), before);
        game.tick();
        assert_eq!(game.actor(b).unwrap().health, 100);
        assert_eq!(game.actor(c).unwrap().health, 40);
        assert!(game.events().iter().any(|e| matches!(e, Event::Shot { source, target, damage: 60 } if *source == a && *target == c)));
    }
    #[test]
    fn special_verses_allow_explicit_attack_but_block_passive_acquisition() {
        for percent in [1000, 2000] {
            let mut game = game();
            let rules = Arc::make_mut(&mut game.rules);
            rules.units[0].weapon.as_mut().unwrap().damage = 100;
            rules.units[0].weapon.as_mut().unwrap().verses.0[0] = percent;
            let a = game.spawn(0, 0, Vec2::new(1, 1)).unwrap();
            let b = game.spawn(1, 0, Vec2::new(2, 1)).unwrap();
            game.tick();
            assert!(game.events().is_empty());
            game.attack(0, &[a], b).unwrap();
            game.tick();
            assert_eq!(game.actor(b).unwrap().health, 100 - percent / 1000);
            assert_eq!(game.actor(a).unwrap().health, 100);
        }
    }
    #[test]
    fn armor_and_verses_are_part_of_handshake_hash_and_versioned_saves() {
        let game = game();
        let mut bytes = game.save().unwrap();
        bytes[..8].copy_from_slice(b"RA2NEGS1");
        assert!(Skirmish::load(&bytes).is_err());
        let mut other = Skirmish::load(&game.save().unwrap()).unwrap();
        Arc::make_mut(&mut other.rules).units[0].armor = Armor::Heavy;
        assert_ne!(game.state_hash(), other.state_hash());
        let restored = Skirmish::load(&other.save().unwrap()).unwrap();
        assert_eq!(other.state_hash(), restored.state_hash());
        Arc::make_mut(&mut other.rules).units[0]
            .weapon
            .as_mut()
            .unwrap()
            .verses
            .0[5] = 50_000;
        assert_ne!(restored.state_hash(), other.state_hash());
    }
    #[test]
    fn simultaneous_fire_is_mutually_lethal_and_slots_reject_stale_handles() {
        let mut game = game();
        let a = game.spawn(0, 0, Vec2::new(1, 1)).unwrap();
        let b = game.spawn(1, 0, Vec2::new(2, 1)).unwrap();
        game.tick();
        assert_eq!(game.actor(a).unwrap().health, 40);
        assert_eq!(game.actor(b).unwrap().health, 40);
        game.tick();
        assert_eq!(game.actor(a).unwrap().health, 40);
        game.tick();
        assert!(game.actor(a).is_none() && game.actor(b).is_none());
        assert!(game.move_units(0, &[a], Vec2::new(3, 3)).is_err());
        assert_eq!(
            game.events()
                .iter()
                .filter(|e| matches!(e, Event::Destroyed { .. }))
                .count(),
            2
        );
    }
    #[test]
    fn commands_validate_whole_selection_and_ownership_before_mutation() {
        let mut game = game();
        let a = game.spawn(0, 0, Vec2::new(1, 1)).unwrap();
        let b = game.spawn(1, 0, Vec2::new(15, 15)).unwrap();
        let hash = game.state_hash();
        assert!(game.move_units(0, &[a, b], Vec2::new(3, 3)).is_err());
        assert_eq!(game.state_hash(), hash);
        assert!(game.attack(0, &[a], a).is_err());
        assert_eq!(game.state_hash(), hash);
        game.move_units(0, &[a, a], Vec2::new(3, 3)).unwrap();
        game.tick();
        assert_ne!(
            game.movement.unit(a.index as usize).unwrap().position,
            Vec2::new(1, 1)
        );
    }
    #[test]
    fn attack_pursues_until_range_then_stops_and_draw_freezes_tick() {
        let mut game = game();
        let a = game.spawn(0, 0, Vec2::new(1, 1)).unwrap();
        let b = game.spawn(1, 0, Vec2::new(10, 1)).unwrap();
        game.attack(0, &[a], b).unwrap();
        for _ in 0..30 {
            game.tick();
        }
        assert!(game.finished());
        assert!(game.winner().is_none());
        let tick = game.tick_number();
        game.tick();
        assert_eq!(game.tick_number(), tick);
        assert!(game.spawn(0, 0, Vec2::new(1, 1)).is_err());
    }
    #[test]
    fn harvesting_conserves_resource_and_credits_and_rejects_foreign_depot() {
        let mut game = game();
        Arc::make_mut(&mut game.rules).units[0].factory = true;
        Arc::make_mut(&mut game.rules).units[0].harvester = true;
        Arc::make_mut(&mut game.rules).units[0].weapon = None;
        let depot = game.spawn(0, 0, Vec2::new(1, 1)).unwrap();
        let miner = game.spawn(0, 0, Vec2::new(2, 1)).unwrap();
        game.set_resource(Vec2::new(3, 1), 35).unwrap();
        game.harvest(0, &[miner], Vec2::new(3, 1), depot).unwrap();
        for _ in 0..100 {
            game.tick();
        }
        assert_eq!(game.resource(Vec2::new(3, 1)), 0);
        assert_eq!(game.actor(miner).unwrap().cargo, 0);
        assert_eq!(game.players()[&0].credits, 1000 + 35 * 25);
        assert!(game.harvesting.is_empty());
        assert!(game.harvest(1, &[miner], Vec2::new(3, 1), depot).is_err());
    }
    #[test]
    fn production_reserves_refunds_and_waits_for_power_and_exit() {
        let mut game = game();
        let rules = Arc::make_mut(&mut game.rules);
        rules.units[0].factory = true;
        rules.units[0].power = -1;
        rules.units[0].weapon = None;
        rules.units[0].build_ticks = 2;
        let factory = game.spawn(0, 0, Vec2::new(1, 1)).unwrap();
        game.queue_production(0, factory, 0).unwrap();
        assert_eq!(game.players()[&0].credits, 900);
        game.tick();
        assert_eq!(game.production(factory).unwrap()[0].remaining, 2);
        game.cancel_production(0, factory, 0).unwrap();
        assert_eq!(game.players()[&0].credits, 1000);
        Arc::make_mut(&mut game.rules).units[0].power = 0;
        game.queue_production(0, factory, 0).unwrap();
        game.tick();
        assert_eq!(game.entities().count(), 1);
        for p in [
            Vec2::new(2, 1),
            Vec2::new(1, 2),
            Vec2::new(0, 1),
            Vec2::new(1, 0),
        ] {
            game.map.set_walkable(p, false);
        }
        game.tick();
        assert_eq!(game.production(factory).unwrap()[0].remaining, 0);
        game.map.set_walkable(Vec2::new(2, 1), true);
        game.tick();
        assert_eq!(game.entities().count(), 2);
        assert!(game.production(factory).is_none());
        assert!(
            game.events()
                .iter()
                .any(|e| matches!(e, Event::Produced { .. }))
        );
        let before = game.state_hash();
        assert!(game.queue_production(1, factory, 0).is_err());
        assert_eq!(game.state_hash(), before);
    }
    #[test]
    fn spatial_bucket_tuning_does_not_change_combat_outcome() {
        let mut a = game();
        let mut b = game();
        b.movement.set_spatial_cell_size(1).unwrap();
        for game in [&mut a, &mut b] {
            game.spawn(0, 0, Vec2::new(1, 1)).unwrap();
            game.spawn(1, 0, Vec2::new(4, 1)).unwrap();
        }
        for _ in 0..10 {
            a.tick();
            b.tick();
            assert_eq!(a.state_hash(), b.state_hash());
        }
    }
}
