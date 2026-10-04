use ra2ne_core::replay::{CommandLog, MoveCommand};
use ra2ne_core::{TICKS_PER_SECOND, Unit, Vec2, World, navigation::NavigationMap};
use std::{env, time::Instant};

fn argument(name: &str, default: usize) -> usize {
    env::args()
        .skip(1)
        .find_map(|arg| {
            arg.strip_prefix(&format!("--{name}="))
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or(default)
}

fn main() {
    let units = argument("units", 10_000);
    let ticks = argument("ticks", 900);
    assert!(ticks > 0, "ticks must be positive");
    if env::args().any(|arg| arg == "--replay-check") {
        replay_check(units, ticks as u64);
        return;
    }
    let bridge = env::args().any(|arg| arg == "--scenario=bridge");
    let mut world = if bridge {
        World::from_units(
            (0..units)
                .map(|id| Unit {
                    position: Vec2::new((id % 120) as i32, ((id / 120) % 256) as i32),
                    goal: Vec2::new(250, 128),
                    speed: 1,
                })
                .collect(),
        )
    } else {
        World::seeded(units, 0x0052_4132_4e45)
    };
    if bridge {
        let mut map = NavigationMap::new(256, 256);
        for y in 0..256 {
            map.set_walkable(Vec2::new(128, y), y == 128);
        }
        map.set_capacity(Vec2::new(128, 128), 4);
        let planning = Instant::now();
        world
            .move_group(&(0..units).collect::<Vec<_>>(), &map, Vec2::new(250, 128))
            .unwrap();
        println!(
            "shared_path_planning_ms={:.3}",
            planning.elapsed().as_secs_f64() * 1000.0
        );
    }
    let started = Instant::now();
    for _ in 0..ticks {
        world.tick();
    }
    let elapsed = started.elapsed();
    let per_tick_ms = elapsed.as_secs_f64() * 1000.0 / ticks as f64;
    println!("RA2NE Phase 1 benchmark");
    println!(
        "units={}; ticks={}; tps_target={}",
        world.unit_count(),
        world.tick_number(),
        TICKS_PER_SECOND
    );
    println!(
        "simulation_ms_per_tick={per_tick_ms:.3}; state_hash={:016x}",
        world.state_hash()
    );
    println!(
        "nearby_candidates@center={}",
        world.nearby_candidate_count(Vec2::new(2048, 2048), 128)
    );
}

fn replay_check(count: usize, ticks: u64) {
    let initial: Vec<_> = (0..count)
        .map(|id| Unit {
            position: Vec2::new((id % 120) as i32, ((id / 120) % 256) as i32),
            goal: Vec2::new((id % 120) as i32, ((id / 120) % 256) as i32),
            speed: 1,
        })
        .collect();
    let mut map = NavigationMap::new(256, 256);
    for y in 0..256 {
        map.set_walkable(Vec2::new(128, y), y == 128);
    }
    map.set_capacity(Vec2::new(128, 128), 4);
    let first = MoveCommand {
        tick: 0,
        player: 0,
        sequence: 0,
        units: (0..count).collect(),
        goal: Vec2::new(250, 128),
    };
    let second = MoveCommand {
        tick: ticks / 2,
        player: 0,
        sequence: 1,
        units: (0..count).rev().collect(),
        goal: Vec2::new(0, 128),
    };
    let mut a = CommandLog::default();
    let mut b = CommandLog::default();
    a.insert(first.clone()).unwrap();
    a.insert(second.clone()).unwrap();
    b.insert(second).unwrap();
    b.insert(first).unwrap();
    let started = Instant::now();
    let a = a.play(initial.clone(), &map, ticks, 30).unwrap();
    let b = b.play(initial, &map, ticks, 30).unwrap();
    assert_eq!(a.checkpoints, b.checkpoints, "replay desynchronization");
    println!(
        "replay_verified=true; units={count}; ticks={ticks}; checkpoints={}; state_hash={:016x}; two_playbacks_ms={:.3}",
        a.checkpoints.len(),
        a.world.state_hash(),
        started.elapsed().as_secs_f64() * 1000.0
    );
}
