use std::{env, time::Instant};
use ra2ne_core::{Vec2, World, TICKS_PER_SECOND};

fn argument(name: &str, default: usize) -> usize {
    env::args().skip(1)
        .find_map(|arg| arg.strip_prefix(&format!("--{name}=")).and_then(|v| v.parse().ok()))
        .unwrap_or(default)
}

fn main() {
    let units = argument("units", 10_000);
    let ticks = argument("ticks", 900);
    let mut world = World::seeded(units, 0x5241_324e_45);
    let started = Instant::now();
    for _ in 0..ticks { world.tick(); }
    let elapsed = started.elapsed();
    let per_tick_ms = elapsed.as_secs_f64() * 1000.0 / ticks as f64;
    println!("RA2NE Phase 1 benchmark");
    println!("units={}; ticks={}; tps_target={}", world.unit_count(), world.tick_number(), TICKS_PER_SECOND);
    println!("simulation_ms_per_tick={per_tick_ms:.3}; state_hash={:016x}", world.state_hash());
    println!("nearby_candidates@center={}", world.nearby_candidate_count(Vec2::new(2048, 2048), 128));
}
use std::{env, time::Instant};
use ra2ne_core::{World, TICKS_PER_SECOND};

fn argument(name: &str, default: usize) -> usize {
    env::args().skip(1)
        .find_map(|arg| arg.strip_prefix(&format!("--{name}=")).and_then(|v| v.parse().ok()))
        .unwrap_or(default)
}

fn main() {
    let units = argument("units", 10_000);
    let ticks = argument("ticks", 900);
    let mut world = World::seeded(units, 0x5241_324e_45);
    let started = Instant::now();
    for _ in 0..ticks { world.tick(); }
    let elapsed = started.elapsed();
    let per_tick_ms = elapsed.as_secs_f64() * 1000.0 / ticks as f64;
    println!("RA2NE Phase 1 benchmark");
    println!("units={}; ticks={}; tps_target={}", world.unit_count(), world.tick_number(), TICKS_PER_SECOND);
    println!("simulation_ms_per_tick={per_tick_ms:.3}; state_hash={:016x}", world.state_hash());
}
