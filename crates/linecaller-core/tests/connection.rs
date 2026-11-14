//! End-to-end tests of the connection manager against scripted TCP peers.

use linecaller_core::{
    ClientConfig, ClientHandle, ConnectionPhase, ConnectionStatus, EventSink, LineView, SendError,
    Simulator,
};
use rlp::{
    Audio, Command, Decoded, FrameDecoder, LineIndex, LineState, LineStatus, Location, Screening,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
enum Ev {
    Conn(ConnectionStatus),
    Line(LineView),
    Warn(String),
}

struct Sink(mpsc::UnboundedSender<Ev>);
impl EventSink for Sink {
    fn connection(&self, s: &ConnectionStatus) {
        let _ = self.0.send(Ev::Conn(s.clone()));
    }
    fn line(&self, v: &LineView) {
        let _ = self.0.send(Ev::Line(v.clone()));
    }
    fn warning(&self, m: &str) {
        let _ = self.0.send(Ev::Warn(m.to_string()));
    }
}

fn cfg(port: u16, lines: u8) -> ClientConfig {
    ClientConfig {
        host: "127.0.0.1".into(),
        port,
        location: Location(1),
        line_count: lines,
        liveness_interval: None,
        conference: false,
    }
}

fn status(line: u8, state: LineState) -> LineStatus {
    LineStatus {
        line: LineIndex(line),
        state,
        audio: Audio::Hold,
        location: 0,
        audio_locked: false,
        line_locked: false,
        screening: Screening {
            phone_number: "0612".into(),
            ..Default::default()
        },
    }
}

fn pkt(s: &LineStatus) -> Vec<u8> {
    rlp::encode_packet(&s.to_payload()).unwrap()
}

async fn next_matching<F: Fn(&Ev) -> bool>(rx: &mut mpsc::UnboundedReceiver<Ev>, f: F) -> Ev {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let e = rx.recv().await.expect("sink closed");
            if f(&e) {
                return e;
            }
        }
    })
    .await
    .expect("timed out waiting for event")
}

async fn read_commands(sock: &mut TcpStream, n: usize) -> Vec<Command> {
    let mut dec = FrameDecoder::new();
    let mut out = vec![];
    let mut buf = [0u8; 256];
    while out.len() < n {
        let r = tokio::time::timeout(Duration::from_secs(5), sock.read(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert!(r > 0, "client closed");
        dec.push(&buf[..r]);
        while let Some(Decoded::Packet(p)) = dec.next() {
            out.push(Command::from_payload(&p).unwrap());
        }
    }
    out
}

#[tokio::test]
async fn syncs_lines_and_handles_fragmented_and_invalid_input() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let client = ClientHandle::spawn(Arc::new(Sink(tx)));
    client.configure(Some(cfg(port, 3))).await;

    let (mut sock, _) = listener.accept().await.unwrap();
    // Initial sync: one Status Request per displayed line, zero-based, then
    // the location's conference mode (§4.2.7).
    let cmds = read_commands(&mut sock, 4).await;
    assert_eq!(
        cmds,
        (0..3)
            .map(|i| Command::StatusRequest { line: LineIndex(i) })
            .chain([Command::SetConference {
                location: Location(1),
                on: false,
            }])
            .collect::<Vec<_>>()
    );

    // Garbage, then one status split over three writes, then two in one write.
    let mut stream = vec![0x00, 0x42];
    stream.extend(pkt(&status(0, LineState::Ringing)));
    let (a, rest) = stream.split_at(4);
    let (b, c) = rest.split_at(5);
    for part in [a, b, c] {
        sock.write_all(part).await.unwrap();
        sock.flush().await.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut two = pkt(&status(1, LineState::Connected));
    two.extend([0xFE, 0x01, 0x01, 0x99, 0x00]); // bad end flag
    two.extend(pkt(&status(2, LineState::Calling)));
    sock.write_all(&two).await.unwrap();

    next_matching(
        &mut rx,
        |e| matches!(e, Ev::Warn(m) if m.contains("Discarded")),
    )
    .await;
    let Ev::Line(v) = next_matching(&mut rx, |e| matches!(e, Ev::Line(_))).await else {
        unreachable!()
    };
    assert_eq!(v.status.state, LineState::Ringing);
    next_matching(
        &mut rx,
        |e| matches!(e, Ev::Warn(m) if m.contains("end flag")),
    )
    .await;
    next_matching(
        &mut rx,
        |e| matches!(e, Ev::Line(v) if v.status.line == LineIndex(2)),
    )
    .await;
    assert_eq!(client.line(1).unwrap().status.state, LineState::Connected);

    // Commands reach the socket with the configured location.
    client
        .send(Command::SetAudio {
            line: LineIndex(1),
            audio: Audio::OnAir2,
            location: Location(1),
        })
        .await
        .unwrap();
    assert_eq!(
        read_commands(&mut sock, 1).await,
        vec![Command::SetAudio {
            line: LineIndex(1),
            audio: Audio::OnAir2,
            location: Location(1)
        }]
    );
}

#[tokio::test]
async fn reconnects_after_drop_and_marks_lines_stale() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let client = ClientHandle::spawn(Arc::new(Sink(tx)));
    client.configure(Some(cfg(port, 1))).await;

    let (mut sock, _) = listener.accept().await.unwrap();
    read_commands(&mut sock, 1).await;
    sock.write_all(&pkt(&status(0, LineState::Connected)))
        .await
        .unwrap();
    next_matching(&mut rx, |e| matches!(e, Ev::Line(_))).await;
    drop(sock);

    next_matching(&mut rx, |e| matches!(e, Ev::Line(v) if v.stale)).await;
    let Ev::Conn(c) = next_matching(
        &mut rx,
        |e| matches!(e, Ev::Conn(c) if matches!(c.phase, ConnectionPhase::Retrying { .. })),
    )
    .await
    else {
        unreachable!()
    };
    assert!(c.last_error.is_some());
    assert_eq!(
        client
            .send(Command::StatusRequest { line: LineIndex(0) })
            .await,
        Err(SendError::NotConnected)
    );

    let (mut sock2, _) = listener.accept().await.unwrap();
    assert_eq!(
        read_commands(&mut sock2, 2).await,
        vec![
            Command::StatusRequest { line: LineIndex(0) },
            Command::SetConference {
                location: Location(1),
                on: false
            }
        ]
    );
    next_matching(&mut rx, |e| matches!(e, Ev::Conn(c) if c.is_connected())).await;
}

#[tokio::test]
async fn liveness_check_reconnects_silent_peer() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let client = ClientHandle::spawn(Arc::new(Sink(tx)));
    let mut c = cfg(port, 1);
    c.liveness_interval = Some(Duration::from_millis(300));
    client.configure(Some(c)).await;
    let (mut sock, _) = listener.accept().await.unwrap();
    // Initial sync request plus the liveness probe, never answered.
    read_commands(&mut sock, 2).await;
    let Ev::Conn(st) = next_matching(
        &mut rx,
        |e| matches!(e, Ev::Conn(c) if c.last_error.is_some()),
    )
    .await
    else {
        unreachable!()
    };
    assert!(st.last_error.unwrap().contains("liveness"));
}

#[tokio::test]
async fn refused_connection_retries_and_offline_rejects_commands() {
    // Bind then drop to get a port nobody listens on.
    let port = {
        TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    };
    let (tx, mut rx) = mpsc::unbounded_channel();
    let client = ClientHandle::spawn(Arc::new(Sink(tx)));
    client.configure(Some(cfg(port, 1))).await;
    next_matching(
        &mut rx,
        |e| matches!(e, Ev::Conn(c) if matches!(c.phase, ConnectionPhase::Retrying { .. })),
    )
    .await;
    client.configure(None).await;
    next_matching(
        &mut rx,
        |e| matches!(e, Ev::Conn(c) if c.phase == ConnectionPhase::Offline),
    )
    .await;
    assert_eq!(
        client
            .send(Command::StatusRequest { line: LineIndex(0) })
            .await,
        Err(SendError::NotConnected)
    );
}

#[tokio::test]
async fn invalid_commands_are_rejected_before_sending() {
    let (tx, _rx) = mpsc::unbounded_channel();
    let client = ClientHandle::spawn(Arc::new(Sink(tx)));
    let r = client
        .send(Command::Dial {
            line: LineIndex(0),
            audio: Audio::Pretalk,
            location: Location(1),
            number: "12ab".into(),
        })
        .await;
    assert!(matches!(r, Err(SendError::Invalid(_))));
}

#[tokio::test]
async fn full_call_flow_against_simulator() {
    let sim = Simulator::start("127.0.0.1:0".parse().unwrap(), 1)
        .await
        .unwrap();
    sim.set_auto_incoming(false);
    let (tx, mut rx) = mpsc::unbounded_channel();
    let client = ClientHandle::spawn(Arc::new(Sink(tx)));
    client.configure(Some(cfg(sim.addr().port(), 8))).await;
    next_matching(
        &mut rx,
        |e| matches!(e, Ev::Line(v) if v.status.line == LineIndex(7)),
    )
    .await;

    // Line 6 (index 5) is in use by another location in the simulator.
    assert_eq!(client.line(5).unwrap().status.location, 6);

    // Incoming call, accept to ON-AIR A.
    assert!(sim.incoming_call(LineIndex(0)));
    next_matching(
        &mut rx,
        |e| matches!(e, Ev::Line(v) if v.status.state == LineState::Ringing),
    )
    .await;
    client
        .send(Command::Dial {
            line: LineIndex(0),
            audio: Audio::OnAir1,
            location: Location(1),
            number: String::new(),
        })
        .await
        .unwrap();
    next_matching(&mut rx, |e| matches!(e, Ev::Line(v) if v.status.state == LineState::Connected && v.status.audio == Audio::OnAir1)).await;

    // Lock audio, then drop.
    client
        .send(Command::SetLockAudio {
            line: LineIndex(0),
            locked: true,
            location: Location(1),
        })
        .await
        .unwrap();
    next_matching(
        &mut rx,
        |e| matches!(e, Ev::Line(v) if v.status.audio_locked),
    )
    .await;
    client
        .send(Command::Drop {
            line: LineIndex(0),
            location: Location(1),
        })
        .await
        .unwrap();
    next_matching(&mut rx, |e| matches!(e, Ev::Line(v) if v.status.line == LineIndex(0) && v.status.state == LineState::Disconnected)).await;
}
