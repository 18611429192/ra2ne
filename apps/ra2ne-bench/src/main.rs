use ra2ne_core::lockstep::{InputFrame, Lockstep};
use ra2ne_core::replay::{CommandLog, MoveCommand};
use ra2ne_core::replay_file::ReplayFile;
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
    if env::args().any(|arg| arg == "--asset-check") {
        asset_check();
        return;
    }
    if let Some(path) =
        env::args().find_map(|arg| arg.strip_prefix("--replay-input=").map(str::to_owned))
    {
        let file = std::fs::File::open(path).expect("open replay");
        let mut bytes = Vec::new();
        use std::io::Read;
        file.take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .expect("read replay");
        let replay = ReplayFile::decode(&bytes).expect("decode replay");
        let playback = replay.play().expect("play replay");
        println!(
            "file_replay_verified=true; ticks={}; checkpoints={}; state_hash={:016x}",
            playback.world.tick_number(),
            playback.checkpoints.len(),
            playback.world.state_hash()
        );
        return;
    }
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
    let mut barrier = Lockstep::new(&[0, 1], ticks, 4, count).unwrap();
    if ticks == 1 {
        barrier
            .submit(InputFrame {
                tick: 0,
                player: 0,
                commands: vec![second, first],
            })
            .unwrap();
    } else {
        for command in [second, first] {
            barrier
                .submit(InputFrame {
                    tick: command.tick,
                    player: command.player,
                    commands: vec![command],
                })
                .unwrap();
        }
    }
    for tick in 0..ticks {
        if tick != 0 && tick != ticks / 2 {
            barrier
                .submit(InputFrame {
                    tick,
                    player: 0,
                    commands: vec![],
                })
                .unwrap();
        }
        assert!(barrier.take_ready().unwrap().is_none());
        barrier
            .submit(InputFrame {
                tick,
                player: 1,
                commands: vec![],
            })
            .unwrap();
        for command in barrier.take_ready().unwrap().unwrap() {
            b.insert(command).unwrap();
        }
    }
    let started = Instant::now();
    let a = a.play(initial.clone(), &map, ticks, 30).unwrap();
    let recording = ReplayFile {
        units: initial,
        map,
        commands: b,
        ticks,
        checkpoint_interval: 30,
    };
    let encoded = recording.encode().unwrap();
    if let Some(path) =
        env::args().find_map(|arg| arg.strip_prefix("--replay-output=").map(str::to_owned))
    {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .expect("create replay (existing files are preserved)");
        file.write_all(&encoded).expect("write replay");
    }
    let b = ReplayFile::decode(&encoded).unwrap().play().unwrap();
    assert_eq!(a.checkpoints, b.checkpoints, "replay desynchronization");
    println!(
        "replay_verified=true; lockstep_verified=true; file_roundtrip_verified=true; units={count}; ticks={ticks}; checkpoints={}; state_hash={:016x}; verification_ms={:.3}",
        a.checkpoints.len(),
        a.world.state_hash(),
        started.elapsed().as_secs_f64() * 1000.0
    );
}

fn asset_check() {
    use ra2ne_assets::{ini::Ini, vfs::Vfs};
    let mut source = String::from("[VehicleTypes]\n");
    for id in 0..10_000 {
        source.push_str(&format!("{id}=UNIT{id}\n"));
    }
    for id in 0..10_000 {
        source.push_str(&format!("[UNIT{id}]\nStrength=100\nSpeed=5\nTracked=yes\n"));
    }
    let started = Instant::now();
    let ini = Ini::parse(&source).unwrap();
    assert!(ini.diagnostics.is_empty());
    assert_eq!(ini.entries().len(), 40_000);
    assert_eq!(ini.section_entries("VehicleTypes").count(), 10_000);
    assert_eq!(ini.get("unit9999", "strength").unwrap().integer(), Ok(100));
    let parse_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut vfs = Vfs::default();
    vfs.mount(
        "synthetic-base",
        vec![("RulesMD.INI".into(), source.into_bytes())],
    )
    .unwrap();
    vfs.mount(
        "synthetic-mod",
        vec![("rulesmd.ini".into(), b"[UNIT0]\nStrength=200\n".to_vec())],
    )
    .unwrap();
    let file = vfs.get("RULESMD.INI").unwrap().unwrap();
    assert_eq!(file.source, "synthetic-mod");
    let overlay = Ini::parse(std::str::from_utf8(&file.bytes).unwrap()).unwrap();
    assert_eq!(overlay.get("unit0", "strength").unwrap().integer(), Ok(200));
    println!(
        "asset_frontend_verified=true; synthetic_unit_types=10000; entries=40000; parse_ms={parse_ms:.3}"
    );
}
