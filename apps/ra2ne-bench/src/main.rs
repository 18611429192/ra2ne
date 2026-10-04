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
    if env::args().any(|arg| arg == "--network-check") {
        network_check();
        return;
    }
    if env::args().any(|arg| arg == "--game-check") {
        game_check();
        return;
    }
    if env::args().any(|arg| arg == "--rules-check") {
        rules_check();
        return;
    }
    if env::args().any(|arg| arg == "--mix-check") {
        mix_check();
        return;
    }
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
    let overlay = Ini::parse(std::str::from_utf8(file.bytes).unwrap()).unwrap();
    assert_eq!(overlay.get("unit0", "strength").unwrap().integer(), Ok(200));
    println!(
        "asset_frontend_verified=true; synthetic_unit_types=10000; entries=40000; parse_ms={parse_ms:.3}"
    );
}

fn mix_check() {
    use ra2ne_assets::mix::{FilenameHash, MixArchive, filename_id};
    use std::sync::Arc;
    let count = 10_000_u16;
    let mut bytes = Vec::new();
    bytes.extend(0_u32.to_le_bytes());
    bytes.extend(count.to_le_bytes());
    bytes.extend((u32::from(count) * 4).to_le_bytes());
    for id in 0..u32::from(count) {
        bytes.extend(
            filename_id(&format!("asset{id}.bin"), FilenameHash::Ra2)
                .unwrap()
                .to_le_bytes(),
        );
        bytes.extend((id * 4).to_le_bytes());
        bytes.extend(4_u32.to_le_bytes());
    }
    for id in 0..u32::from(count) {
        bytes.extend(id.to_le_bytes());
    }
    let started = Instant::now();
    let mix = MixArchive::parse(Arc::from(bytes)).unwrap();
    let parse_ms = started.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(mix.entry_count(), usize::from(count));
    let started = Instant::now();
    for id in 0..u32::from(count) {
        assert_eq!(
            mix.get(&format!("ASSET{id}.BIN"), FilenameHash::Ra2)
                .unwrap()
                .unwrap(),
            &id.to_le_bytes()
        );
    }
    println!(
        "mix_verified=true; entries={count}; parse_ms={parse_ms:.3}; lookup_ms={:.3}",
        started.elapsed().as_secs_f64() * 1000.0
    );
}

fn rules_check() {
    use ra2ne_assets::rules::RuleSet;
    let mut text = String::from("[VehicleTypes]\n");
    for id in 0..10_000 {
        text.push_str(&format!("{id}=UNIT{id}\n"));
    }
    for id in 0..10_000 {
        text.push_str(&format!(
            "[UNIT{id}]\nStrength=100\nSpeed=5\nCost=500\nPrimary=CANNON\n"
        ));
    }
    text.push_str("[CANNON]\nDamage=20\nROF=10\nRange=5.5\n");
    let started = Instant::now();
    let mut rules = RuleSet::default();
    rules.add_layer("synthetic rules.ini", &text).unwrap();
    rules
        .add_layer("synthetic map override", "[UNIT9999]\nStrength=200\n")
        .unwrap();
    let catalog = rules.load().unwrap();
    let load_ms = started.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(catalog.types.len(), 10_000);
    assert_eq!(catalog.weapons.len(), 1);
    assert!(catalog.diagnostics.is_empty());
    assert_eq!(catalog.type_by_id("unit9999").unwrap().strength, 200);
    println!("rules_verified=true; types=10000; weapons=1; load_ms={load_ms:.3}");
}

fn game_check() {
    use ra2ne_game::{Player, Rules, Skirmish, UnitDef, Weapon};
    use std::{collections::BTreeMap, sync::Arc};
    let count = argument("units", 10_000);
    let ticks = argument("ticks", 900);
    assert!((2..=100_000).contains(&count) && ticks > 0);
    let rules = Arc::new(Rules {
        units: vec![UnitDef {
            production: None,
            secondary: None,
            armor: ra2ne_game::Armor::None,
            name: "test-tank".into(),
            health: 1_000_000,
            speed: 1,
            cost: 100,
            weapon: Some(Weapon {
                verses: ra2ne_game::Verses::default(),
                damage: 10,
                range: 4,
                reload_ticks: 15,
            }),
            build_ticks: 30,
            power: 0,
            factory: false,
            harvester: false,
        }],
        max_entities: count,
    });
    let entries: Vec<_> = (0..count)
        .map(|i| {
            (
                (i % 2) as u32,
                0,
                Vec2::new((i % 256) as i32, (i / 256) as i32),
            )
        })
        .collect();
    let mut hashes = Vec::new();
    let start = Instant::now();
    let mut simulated = 0;
    for _ in 0..2 {
        let mut game = Skirmish::new(
            rules.clone(),
            NavigationMap::new(256, count.div_ceil(256)),
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
        .unwrap();
        game.populate(&entries).unwrap();
        for tick in 0..ticks {
            game.tick();
            simulated += 1;
            if tick % 30 == 0 || tick + 1 == ticks {
                hashes.push(game.state_hash());
            }
        }
    }
    let midpoint = hashes.len() / 2;
    assert_eq!(&hashes[..midpoint], &hashes[midpoint..]);
    println!(
        "game_determinism_verified=true; units={count}; ticks={ticks}; checkpoints={midpoint}; ms_per_tick={:.3}; state_hash={:016x}",
        start.elapsed().as_secs_f64() * 1000.0 / simulated as f64,
        hashes.last().unwrap()
    );
}

fn network_check() {
    use ra2ne_game::{
        Player, Rules, Skirmish, UnitDef, Weapon,
        commands::{Action, Command, Frame, GameLockstep, GameReplay},
        network::{Peer, Session},
    };
    use std::{
        collections::BTreeMap,
        net::{TcpListener, TcpStream},
        sync::Arc,
        time::Duration,
    };
    let count = argument("units", 2000);
    let ticks = argument("ticks", 900);
    assert!((2..=20_000).contains(&count) && (1..=1_000_000).contains(&ticks));
    let rules = Arc::new(Rules {
        units: vec![UnitDef {
            production: None,
            secondary: Some(Weapon {
                verses: ra2ne_game::Verses::default(),
                damage: 10,
                range: 4,
                reload_ticks: 15,
            }),
            armor: ra2ne_game::Armor::None,
            name: "network-test-tank".into(),
            health: 1_000_000,
            speed: 1,
            cost: 100,
            weapon: Some(Weapon {
                verses: ra2ne_game::Verses([0; 11]),
                damage: 100,
                range: 4,
                reload_ticks: 15,
            }),
            build_ticks: 30,
            power: 0,
            factory: false,
            harvester: false,
        }],
        max_entities: count,
    });
    let mut a = Skirmish::new(
        rules,
        NavigationMap::new(64, 64),
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
    .unwrap();
    let entries: Vec<_> = (0..count)
        .map(|i| {
            (
                (i % 2) as u32,
                0,
                Vec2::new((i % 64) as i32, ((i / 64) % 64) as i32),
            )
        })
        .collect();
    a.populate(&entries).unwrap();
    let initial = a.save().unwrap();
    let mut b = Skirmish::load(&initial).unwrap();
    let session = Session {
        token: [97; 16],
        initial_hash: a.state_hash(),
        tick: 0,
    };
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let outgoing = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (incoming, _) = listener.accept().unwrap();
    let mut left = Peer::new(outgoing, session, 0, 1).unwrap();
    let mut right = Peer::new(incoming, session, 1, 0).unwrap();
    let mut first = GameLockstep::new([0, 1], 0, 2).unwrap();
    let mut second = GameLockstep::new([0, 1], 0, 2).unwrap();
    let mut recorded = Vec::new();
    let start = Instant::now();
    for tick in 0..ticks as u64 {
        let frame = |player| Frame {
            tick,
            player,
            commands: if tick % 120 == 0 {
                vec![Command {
                    tick,
                    player,
                    sequence: tick,
                    action: Action::Move {
                        units: a
                            .entities()
                            .filter(|(_, actor)| actor.owner == player)
                            .take(64)
                            .map(|(id, _)| id)
                            .collect(),
                        goal: Vec2::new(if player == 0 { 29 } else { 33 }, 32),
                    },
                }]
            } else {
                vec![]
            },
        };
        let blue = frame(0);
        let red = frame(1);
        recorded.extend(blue.commands.clone());
        recorded.extend(red.commands.clone());
        first.submit(0, blue.clone()).unwrap();
        second.submit(1, red.clone()).unwrap();
        left.send(&blue).unwrap();
        right.send(&red).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            for frame in left.poll().unwrap() {
                first.submit(1, frame).unwrap();
            }
            for frame in right.poll().unwrap() {
                second.submit(0, frame).unwrap();
            }
            if first.missing_players().is_empty() && second.missing_players().is_empty() {
                break;
            }
            assert!(Instant::now() < deadline, "network input timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
        let left_commands = first.take_ready().unwrap();
        let right_commands = second.take_ready().unwrap();
        assert_eq!(left_commands, right_commands);
        for (left_command, right_command) in left_commands.iter().zip(&right_commands) {
            assert_eq!(
                left_command.action.apply(&mut a, left_command.player),
                right_command.action.apply(&mut b, right_command.player)
            );
        }
        a.tick();
        b.tick();
        assert_eq!(a.events(), b.events());
        for event in a.events() {
            if let ra2ne_game::Event::Shot { damage, .. } = event {
                assert_eq!(
                    *damage, 10,
                    "network fixture must fire its secondary weapon"
                );
            }
        }
        assert_eq!(
            a.state_hash(),
            b.state_hash(),
            "network divergence at {tick}"
        );
    }
    let replay = GameReplay {
        initial,
        ticks: ticks as u64,
        commands: recorded,
    };
    let bytes = replay.encode().unwrap();
    let restored = GameReplay::decode(&bytes).unwrap().play(30).unwrap();
    assert_eq!(restored.game.state_hash(), a.state_hash());
    println!(
        "tcp_game_replay_verified=true; secondary_fixture=true; peers=2; units={count}; ticks={ticks}; replay_bytes={}; elapsed_ms={:.3}; state_hash={:016x}",
        bytes.len(),
        start.elapsed().as_secs_f64() * 1000.0,
        a.state_hash()
    );
}
