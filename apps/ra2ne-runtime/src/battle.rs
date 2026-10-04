use ra2ne_core::{Vec2, World, navigation::NavigationMap};
use ra2ne_game::{EntityId, Player, Rules, Skirmish, UnitDef, Weapon};
use std::{collections::BTreeMap, ops::Deref, sync::Arc};
pub enum Simulation {
    Movement(World),
    Battle(Box<Skirmish>),
}
impl Deref for Simulation {
    type Target = World;
    fn deref(&self) -> &World {
        match self {
            Self::Movement(world) => world,
            Self::Battle(game) => game.movement(),
        }
    }
}
impl Simulation {
    pub fn tick(&mut self) {
        match self {
            Self::Movement(world) => world.tick(),
            Self::Battle(game) => game.tick(),
        }
    }
    pub fn state_hash(&self) -> u64 {
        match self {
            Self::Movement(world) => world.state_hash(),
            Self::Battle(game) => game.state_hash(),
        }
    }
    pub fn live_count(&self) -> usize {
        match self {
            Self::Movement(world) => world.unit_count(),
            Self::Battle(game) => game.entities().count(),
        }
    }
    pub fn alive(&self, index: usize) -> bool {
        match self {
            Self::Movement(_) => true,
            Self::Battle(game) => game.entity_at(index).is_some(),
        }
    }
    pub fn move_group(
        &mut self,
        ids: &[usize],
        map: &NavigationMap,
        goal: Vec2,
    ) -> Result<(), &'static str> {
        match self {
            Self::Movement(world) => world.move_group(ids, map, goal),
            Self::Battle(game) => {
                let handles = handles(game, ids)?;
                game.move_units(0, &handles, goal)
            }
        }
    }
    pub fn stop_group(&mut self, ids: &[usize]) -> Result<(), &'static str> {
        match self {
            Self::Movement(world) => world.stop_group(ids),
            Self::Battle(game) => {
                let handles = handles(game, ids)?;
                game.stop_units(0, &handles)
            }
        }
    }
    pub fn attack_at(&mut self, ids: &[usize], goal: Vec2) -> Option<Result<(), &'static str>> {
        let Self::Battle(game) = self else {
            return None;
        };
        let target = game
            .entities()
            .filter(|(_, a)| a.owner != 0)
            .map(|(id, _)| {
                (
                    id,
                    game.movement().unit(id.index as usize).unwrap().position,
                )
            })
            .find(|(_, p)| *p == goal)?
            .0;
        Some(handles(game, ids).and_then(|ids| game.attack(0, &ids, target)))
    }
}
fn handles(game: &Skirmish, ids: &[usize]) -> Result<Vec<EntityId>, &'static str> {
    ids.iter()
        .map(|&index| game.entity_at(index).ok_or("stale selection"))
        .collect()
}
pub fn synthetic(map: NavigationMap, count: usize) -> Result<Skirmish, &'static str> {
    let tank = UnitDef {
        production: None,
        secondary: None,
        armor: ra2ne_game::Armor::None,
        name: "Tank".into(),
        health: 100,
        speed: 1,
        cost: 100,
        weapon: Some(Weapon {
            verses: ra2ne_game::Verses::default(),
            damage: 10,
            range: 4,
            reload_ticks: 15,
        }),
        build_ticks: 90,
        power: 0,
        factory: false,
        harvester: false,
    };
    let mut factory = tank.clone();
    factory.name = "Factory / synthetic depot".into();
    factory.health = 1000;
    factory.speed = 0;
    factory.weapon = None;
    factory.factory = true;
    factory.power = 100;
    let mut miner = tank.clone();
    miner.name = "Harvester".into();
    miner.health = 300;
    miner.weapon = None;
    miner.harvester = true;
    let mut game = Skirmish::new(
        Arc::new(Rules {
            units: vec![tank, factory, miner],
            max_entities: 20_004,
        }),
        map,
        BTreeMap::from([
            (
                0,
                Player {
                    credits: 2000,
                    defeated: false,
                },
            ),
            (
                1,
                Player {
                    credits: 2000,
                    defeated: false,
                },
            ),
        ]),
    )?;
    let mut entries: Vec<_> = (0..count)
        .map(|i| {
            let side = (i % 2) as u32;
            let n = i / 2;
            (
                side,
                0,
                Vec2::new(
                    if side == 0 {
                        4 + (n % 20) as i32
                    } else {
                        40 + (n % 20) as i32
                    },
                    4 + ((n / 20) % 52) as i32,
                ),
            )
        })
        .collect();
    entries.extend([
        (0, 1, Vec2::new(10, 30)),
        (1, 1, Vec2::new(54, 30)),
        (0, 2, Vec2::new(12, 30)),
        (1, 2, Vec2::new(52, 30)),
    ]);
    let ids = game.populate(&entries)?;
    for (owner, position) in [(0, Vec2::new(15, 30)), (1, Vec2::new(49, 30))] {
        game.set_resource(position, 500)?;
        let base = count + owner as usize;
        game.harvest(owner, &[ids[base + 2]], position, ids[base])?;
        game.queue_production(owner, ids[base], 0)?;
    }
    Ok(game)
}
