//! Bounded nonblocking TCP peer transport. Session tokens establish membership;
//! this prototype transport has no encryption or public matchmaking service.
use crate::commands::Frame;
use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    net::TcpStream,
};
const MAX_MESSAGE: usize = 64 * 1024;
const IO_BUDGET: usize = 256 * 1024;
#[derive(Clone, Copy, Debug)]
pub struct Session {
    pub token: [u8; 16],
    pub initial_hash: u64,
    pub tick: u64,
}
#[derive(Debug)]
pub struct Peer {
    stream: TcpStream,
    session: Session,
    local_player: u32,
    remote_player: u32,
    ready: bool,
    incoming: Vec<u8>,
    outgoing: VecDeque<Vec<u8>>,
    written: usize,
}
fn invalid(reason: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason)
}
impl Peer {
    pub fn new(
        stream: TcpStream,
        session: Session,
        local_player: u32,
        remote_player: u32,
    ) -> io::Result<Self> {
        if local_player == remote_player {
            return Err(invalid("duplicate peer identity"));
        }
        stream.set_nonblocking(true)?;
        stream.set_nodelay(true)?;
        let mut peer = Self {
            stream,
            session,
            local_player,
            remote_player,
            ready: false,
            incoming: Vec::new(),
            outgoing: VecDeque::new(),
            written: 0,
        };
        let mut hello = b"RA2NETP5".to_vec();
        hello.extend_from_slice(&session.token);
        hello.extend_from_slice(&local_player.to_le_bytes());
        hello.extend_from_slice(&session.tick.to_le_bytes());
        hello.extend_from_slice(&session.initial_hash.to_le_bytes());
        peer.enqueue(hello)?;
        Ok(peer)
    }
    pub fn ready(&self) -> bool {
        self.ready
    }
    pub fn pending_messages(&self) -> usize {
        self.outgoing.len()
    }
    fn enqueue(&mut self, message: Vec<u8>) -> io::Result<()> {
        if message.len() > MAX_MESSAGE || self.outgoing.len() >= 128 {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "network queue budget exceeded",
            ));
        }
        let mut packet = Vec::with_capacity(message.len() + 4);
        packet.extend_from_slice(&(message.len() as u32).to_le_bytes());
        packet.extend(message);
        self.outgoing.push_back(packet);
        Ok(())
    }
    pub fn send(&mut self, frame: &Frame) -> io::Result<()> {
        if frame.player != self.local_player {
            return Err(invalid("outbound sender mismatch"));
        }
        self.enqueue(frame.encode().map_err(invalid)?)
    }
    /// Run on the connection's owner thread. A disconnect is an error; callers
    /// must stop submitting empty frames for that peer so lockstep stalls.
    pub fn poll(&mut self) -> io::Result<Vec<Frame>> {
        let mut budget = IO_BUDGET;
        while let Some(packet) = self.outgoing.front() {
            if budget == 0 {
                break;
            }
            let end = (self.written + budget).min(packet.len());
            match self.stream.write(&packet[self.written..end]) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "peer write closed",
                    ));
                }
                Ok(count) => {
                    self.written += count;
                    budget -= count;
                    if self.written == packet.len() {
                        self.outgoing.pop_front();
                        self.written = 0;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        let mut frames = Vec::new();
        let mut buffer = [0u8; 8192];
        let mut budget = IO_BUDGET;
        while budget > 0 {
            let limit = buffer.len().min(budget);
            match self.stream.read(&mut buffer[..limit]) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "peer disconnected",
                    ));
                }
                Ok(count) => {
                    budget -= count;
                    self.incoming.extend_from_slice(&buffer[..count]);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
            loop {
                if self.incoming.len() < 4 {
                    break;
                }
                let size = u32::from_le_bytes(self.incoming[..4].try_into().unwrap()) as usize;
                if size == 0 || size > MAX_MESSAGE {
                    return Err(invalid("network message length outside bounds"));
                }
                if self.incoming.len() < size + 4 {
                    break;
                }
                let message = &self.incoming[4..size + 4];
                if !self.ready {
                    if message.len() != 44
                        || &message[..8] != b"RA2NETP5"
                        || message[8..24] != self.session.token
                        || u32::from_le_bytes(message[24..28].try_into().unwrap())
                            != self.remote_player
                        || u64::from_le_bytes(message[28..36].try_into().unwrap())
                            != self.session.tick
                        || u64::from_le_bytes(message[36..44].try_into().unwrap())
                            != self.session.initial_hash
                    {
                        return Err(invalid("peer session or initial state mismatch"));
                    }
                    self.ready = true;
                } else {
                    let frame = Frame::decode(message).map_err(invalid)?;
                    if frame.player != self.remote_player {
                        return Err(invalid("inbound sender mismatch"));
                    }
                    frames.push(frame);
                }
                self.incoming.drain(..size + 4);
            }
        }
        Ok(frames)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::GameLockstep;
    use std::{
        net::TcpListener,
        time::{Duration, Instant},
    };
    fn peers(a: Session, b: Session) -> (Peer, Peer) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let left = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (right, _) = listener.accept().unwrap();
        (
            Peer::new(left, a, 0, 1).unwrap(),
            Peer::new(right, b, 1, 0).unwrap(),
        )
    }
    #[test]
    fn tcp_production_cancel_and_rejections_match_both_peers_and_replay() {
        use crate::{
            Player, Skirmish,
            commands::{Action, Command, GameReplay},
            rule_import::{self, ImportPolicy},
        };
        use ra2ne_assets::rules::RuleSet;
        use ra2ne_core::{Vec2, navigation::NavigationMap};
        use std::{collections::BTreeMap, sync::Arc};
        let mut source = RuleSet::default();
        source
            .add_layer(
                "fixture",
                include_str!("../../../fixtures/production-experiment.ini"),
            )
            .unwrap();
        let imported = rule_import::import(
            &source,
            &ImportPolicy {
                speed_table: BTreeMap::from([(5, 1)]),
                build_ticks: 5,
                rof_numerator: 1,
                rof_denominator: 1,
                max_entities: 100,
            },
        )
        .unwrap();
        let tank = imported.type_indices["testtank"];
        let infantry = imported.type_indices["testinfantry"];
        let mut a = Skirmish::new(
            Arc::new(imported.rules),
            NavigationMap::new(64, 64),
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
        )
        .unwrap();
        let mut factories = Vec::new();
        for owner in 0..=1 {
            let x = 4 + owner as i32 * 40;
            factories.push(
                a.spawn(owner, imported.type_indices["testfactory"], Vec2::new(x, 4))
                    .unwrap(),
            );
            a.spawn(
                owner,
                imported.type_indices["testbarracks"],
                Vec2::new(x, 10),
            )
            .unwrap();
            a.spawn(owner, imported.type_indices["testlab"], Vec2::new(x, 16))
                .unwrap();
        }
        let initial = a.save().unwrap();
        let mut b = Skirmish::load(&initial).unwrap();
        let commands = vec![
            Command {
                tick: 0,
                player: 0,
                sequence: 0,
                action: Action::Produce {
                    factory: factories[0],
                    kind: tank,
                },
            },
            Command {
                tick: 0,
                player: 0,
                sequence: 1,
                action: Action::Produce {
                    factory: factories[0],
                    kind: tank,
                },
            },
            Command {
                tick: 0,
                player: 1,
                sequence: 0,
                action: Action::Produce {
                    factory: factories[1],
                    kind: tank,
                },
            },
            Command {
                tick: 1,
                player: 0,
                sequence: 2,
                action: Action::Cancel {
                    factory: factories[0],
                    index: 1,
                },
            },
            Command {
                tick: 2,
                player: 1,
                sequence: 1,
                action: Action::Produce {
                    factory: factories[1],
                    kind: infantry,
                },
            },
            Command {
                tick: 3,
                player: 1,
                sequence: 2,
                action: Action::Cancel {
                    factory: factories[0],
                    index: 0,
                },
            },
        ];
        let replay = GameReplay {
            initial,
            ticks: 12,
            commands: commands.clone(),
        };
        let expected = GameReplay::decode(&replay.encode().unwrap())
            .unwrap()
            .play(1)
            .unwrap();
        assert_eq!(expected.rejected.len(), 2);
        let session = Session {
            token: [53; 16],
            initial_hash: a.state_hash(),
            tick: 0,
        };
        let (mut left, mut right) = peers(session, session);
        let mut left_barrier = GameLockstep::new([0, 1], 0, 2).unwrap();
        let mut right_barrier = GameLockstep::new([0, 1], 0, 2).unwrap();
        for tick in 0..12 {
            for player in 0..=1 {
                let frame = Frame {
                    tick,
                    player,
                    commands: commands
                        .iter()
                        .filter(|c| c.tick == tick && c.player == player)
                        .cloned()
                        .collect(),
                };
                if player == 0 {
                    left.send(&frame).unwrap();
                    left_barrier.submit(0, frame).unwrap();
                } else {
                    right.send(&frame).unwrap();
                    right_barrier.submit(1, frame).unwrap();
                }
            }
            let deadline = Instant::now() + Duration::from_secs(2);
            while !left_barrier.missing_players().is_empty()
                || !right_barrier.missing_players().is_empty()
            {
                for frame in left.poll().unwrap() {
                    left_barrier.submit(1, frame).unwrap();
                }
                for frame in right.poll().unwrap() {
                    right_barrier.submit(0, frame).unwrap();
                }
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
            let mut rejected_a = Vec::new();
            for c in left_barrier.take_ready().unwrap() {
                if let Err(reason) = c.action.apply(&mut a, c.player) {
                    rejected_a.push((c.sequence, reason));
                }
            }
            let mut rejected_b = Vec::new();
            for c in right_barrier.take_ready().unwrap() {
                if let Err(reason) = c.action.apply(&mut b, c.player) {
                    rejected_b.push((c.sequence, reason));
                }
            }
            assert_eq!(rejected_a, rejected_b);
            a.tick();
            b.tick();
            assert_eq!(a.state_hash(), b.state_hash());
            assert_eq!(a.state_hash(), expected.checkpoints[tick as usize].1);
        }
        assert_eq!(a.players()[&0].credits, 1300);
        assert_eq!(a.players()[&1].credits, 1300);
        assert_eq!(a.entities().count(), 8);
    }
    #[test]
    fn real_loopback_tcp_handshake_frames_and_input_barrier() {
        let session = Session {
            token: [71; 16],
            initial_hash: 1234,
            tick: 0,
        };
        let (mut left, mut right) = peers(session, session);
        let mut barrier = GameLockstep::new([0, 1], 0, 2).unwrap();
        left.send(&Frame {
            tick: 0,
            player: 0,
            commands: vec![],
        })
        .unwrap();
        right
            .send(&Frame {
                tick: 0,
                player: 1,
                commands: vec![],
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !barrier.missing_players().is_empty() {
            for frame in left.poll().unwrap() {
                barrier.submit(1, frame).unwrap();
            }
            for frame in right.poll().unwrap() {
                barrier.submit(0, frame).unwrap();
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(left.ready() && right.ready());
        assert_eq!(barrier.take_ready().unwrap().len(), 0);
        assert_eq!(barrier.tick_number(), 1);
        assert!(
            left.send(&Frame {
                tick: 1,
                player: 1,
                commands: vec![]
            })
            .is_err()
        );
    }
    #[test]
    fn fragmented_headers_and_payloads_wait_without_losing_bytes() {
        let session = Session {
            token: [13; 16],
            initial_hash: 55,
            tick: 0,
        };
        let (mut left, mut right) = peers(session, session);
        let hello = right.outgoing.front().unwrap().clone();
        for byte in hello {
            right.stream.write_all(&[byte]).unwrap();
            assert!(left.poll().unwrap().is_empty());
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while !left.ready() {
            left.poll().unwrap();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        let frame = Frame {
            tick: 0,
            player: 1,
            commands: vec![],
        };
        right.send(&frame).unwrap();
        let packet = right.outgoing.back().unwrap().clone();
        let mut received = Vec::new();
        for byte in packet {
            right.stream.write_all(&[byte]).unwrap();
            received.extend(left.poll().unwrap());
        }
        while received.is_empty() {
            received.extend(left.poll().unwrap());
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(received, vec![frame]);
        right.stream.write_all(&u32::MAX.to_le_bytes()).unwrap();
        loop {
            match left.poll() {
                Err(error) => {
                    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
                    break;
                }
                Ok(_) => {
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }
    #[test]
    fn old_protocol_handshake_is_rejected() {
        let session = Session {
            token: [11; 16],
            initial_hash: 1,
            tick: 0,
        };
        let (mut left, mut right) = peers(session, session);
        let mut packet = right.outgoing.front().unwrap().clone();
        packet[4..12].copy_from_slice(b"RA2NETP1");
        right.stream.write_all(&packet).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match left.poll() {
                Err(error) => {
                    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
                    break;
                }
                Ok(_) => {
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
        assert!(!left.ready());
    }
    #[test]
    fn mismatched_initial_state_is_rejected_before_inputs() {
        let a = Session {
            token: [11; 16],
            initial_hash: 1,
            tick: 0,
        };
        let mut b = a;
        b.initial_hash = 2;
        let (mut left, mut right) = peers(a, b);
        right.poll().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match left.poll() {
                Err(error) => {
                    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
                    break;
                }
                Ok(_) => {
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
        assert!(!left.ready());
    }
}
