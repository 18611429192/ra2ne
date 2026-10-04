//! Explicit synthetic economy policy; original resource/refinery rules pending.
use super::*;
const CAPACITY: u32 = 30;
const CREDIT_PER_UNIT: u64 = 25;
#[derive(Clone, Debug)]
pub(crate) struct HarvestOrder {
    pub resource: Vec2,
    pub refinery: EntityId,
    pub returning: bool,
}
impl Skirmish {
    pub fn set_resource(&mut self, position: Vec2, amount: u32) -> Result<(), &'static str> {
        if self.tick != 0 || !self.map.is_traversable(position) {
            return Err("invalid initial resource");
        }
        self.resources.insert((position.x, position.y), amount);
        Ok(())
    }
    pub fn resource(&self, position: Vec2) -> u32 {
        self.resources
            .get(&(position.x, position.y))
            .copied()
            .unwrap_or(0)
    }
    pub fn harvest(
        &mut self,
        player: u32,
        ids: &[EntityId],
        resource: Vec2,
        refinery: EntityId,
    ) -> Result<(), &'static str> {
        let indexes = self.selected(player, ids)?;
        let depot = self.actor(refinery).ok_or("stale refinery")?;
        if depot.owner != player || !self.rules.units[depot.kind].factory {
            return Err("invalid refinery");
        }
        if self.resource(resource) == 0 {
            return Err("empty resource");
        }
        if indexes
            .iter()
            .any(|&i| !self.rules.units[self.slots[i].actor.as_ref().unwrap().kind].harvester)
        {
            return Err("selected unit is not a harvester");
        }
        self.movement.move_group(&indexes, &self.map, resource)?;
        for index in indexes {
            let id = self.entity_at(index).unwrap();
            self.slots[index].actor.as_mut().unwrap().target = None;
            self.harvesting.insert(
                id,
                HarvestOrder {
                    resource,
                    refinery,
                    returning: false,
                },
            );
        }
        Ok(())
    }
    pub(crate) fn advance_harvesting(&mut self) {
        let ids: Vec<_> = self.harvesting.keys().copied().collect();
        for id in ids {
            let order = self.harvesting[&id].clone();
            let Some(actor) = self.actor(id) else {
                self.harvesting.remove(&id);
                continue;
            };
            let owner = actor.owner;
            let Some(depot) = self.actor(order.refinery) else {
                self.harvesting.remove(&id);
                continue;
            };
            if depot.owner != owner {
                self.harvesting.remove(&id);
                continue;
            }
            let depot_position = self
                .movement
                .unit(order.refinery.index as usize)
                .unwrap()
                .position;
            let position = self.movement.unit(id.index as usize).unwrap().position;
            if order.returning {
                if position == depot_position {
                    let actor = self.slots[id.index as usize].actor.as_mut().unwrap();
                    let credit = u64::from(actor.cargo) * CREDIT_PER_UNIT;
                    self.players.get_mut(&owner).unwrap().credits =
                        self.players[&owner].credits.saturating_add(credit);
                    actor.cargo = 0;
                    if self.resource(order.resource) > 0
                        && self
                            .movement
                            .move_group(&[id.index as usize], &self.map, order.resource)
                            .is_ok()
                    {
                        self.harvesting.get_mut(&id).unwrap().returning = false;
                    } else {
                        self.harvesting.remove(&id);
                    }
                } else if self.movement.unit(id.index as usize).unwrap().goal != depot_position {
                    let _ =
                        self.movement
                            .move_group(&[id.index as usize], &self.map, depot_position);
                }
            } else if position == order.resource {
                let amount = self.resources.get_mut(&(position.x, position.y)).unwrap();
                let actor = self.slots[id.index as usize].actor.as_mut().unwrap();
                if *amount > 0 && actor.cargo < CAPACITY {
                    *amount -= 1;
                    actor.cargo += 1;
                }
                if actor.cargo == CAPACITY || *amount == 0 {
                    if actor.cargo == 0 {
                        self.harvesting.remove(&id);
                    } else if self
                        .movement
                        .move_group(&[id.index as usize], &self.map, depot_position)
                        .is_ok()
                    {
                        self.harvesting.get_mut(&id).unwrap().returning = true;
                    }
                }
            }
        }
    }
}
