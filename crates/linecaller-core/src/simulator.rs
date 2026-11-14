//! A small, in-process stand-in for a MAGIC THipPro that speaks the Remote
//! Light Protocol over a real TCP socket.
//!
//! LineCaller's simulator mode connects to this exactly as it would to a real
//! system, so framing, decoding, reconnects and the UI are all exercised
//! without placing real calls. The hybrid behaviour modelled here is a
//! simplified reading of the spec (§4.2.3 audio replacement, §4.2.5 line
//! lock, location ownership) and is **not** a reference for how a real
//! THipPro behaves in every detail.

use crate::store::now_ms;
use rlp::{
    Audio, Command, Decoded, FrameDecoder, LineIndex, LineState, LineStatus, Screening, MAX_LINES,
};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

/// A line that is "in use by another studio" so the UI can show that case.
const FOREIGN_LINE: u8 = 5;

#[derive(Clone, Copy)]
struct Timers {
    /// Outgoing call answered after this many ms.
    answer_ms: u64,
    /// Unanswered incoming call given up after this many ms.
    ring_timeout_ms: u64,
}

struct SimLine {
    status: LineStatus,
    /// When the current state should advance (Calling → Connected, Ringing → gone).
    due_ms: Option<u64>,
    busy_number: bool,
}

struct SimState {
    lines: Vec<SimLine>,
    rng: u64,
    timers: Timers,
    auto_incoming: bool,
    next_auto_call_ms: u64,
}

const CALLERS: &[(&str, &str, &str, &str)] = &[
    ("+31612345678", "de Vries", "Anouk", "Hilversum"),
    ("+31201234567", "Jansen", "Mark", "Amsterdam"),
    ("+3243211234", "Peeters", "Lotte", "Antwerpen"),
    ("+31107654321", "Bakker", "Sven", "Rotterdam"),
    ("0302345678", "", "", "Utrecht"),
    ("", "", "", ""),
    ("+4930123456", "Schneider", "Jonas", "Berlin"),
    ("+31505550123", "Visser", "Femke", "Groningen"),
];

impl SimState {
    fn new(other_location: u8) -> Self {
        let mut lines: Vec<SimLine> = (0..MAX_LINES)
            .map(|i| SimLine {
                status: LineStatus {
                    line: LineIndex(i),
                    state: LineState::Disconnected,
                    audio: Audio::Hold,
                    location: 0,
                    audio_locked: false,
                    line_locked: false,
                    screening: Screening::default(),
                },
                due_ms: None,
                busy_number: false,
            })
            .collect();
        let foreign = &mut lines[FOREIGN_LINE as usize].status;
        foreign.state = LineState::Connected;
        foreign.audio = Audio::Pretalk;
        foreign.location = other_location;
        foreign.screening = Screening {
            phone_number: "+31356771234".into(),
            name: "Smit".into(),
            first_name: "Eva".into(),
            city: "Bussum".into(),
        };
        Self {
            lines,
            rng: now_ms() | 1,
            timers: Timers {
                answer_ms: 2_500,
                ring_timeout_ms: 45_000,
            },
            auto_incoming: true,
            next_auto_call_ms: now_ms() + 6_000,
        }
    }

    fn rand(&mut self) -> u64 {
        // xorshift64
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }

    fn reset_line(l: &mut SimLine) {
        // Line lock (incoming-call block) survives a drop; everything else resets.
        l.status = LineStatus {
            line: l.status.line,
            state: LineState::Disconnected,
            audio: Audio::Hold,
            location: 0,
            audio_locked: false,
            line_locked: l.status.line_locked,
            screening: Screening::default(),
        };
        l.due_ms = None;
        l.busy_number = false;
    }

    /// §4.2.3: other lines of the same location at the target interface are
    /// dropped (ON AIR) or put on HOLD (PRETALK) unless audio-locked.
    /// Conference mode is not modelled (always off).
    fn make_room(&mut self, target: u8, audio: Audio, location: u8, changed: &mut Vec<u8>) {
        if !matches!(audio, Audio::Pretalk | Audio::OnAir1 | Audio::OnAir2) {
            return;
        }
        for l in self.lines.iter_mut() {
            let s = &l.status;
            if s.line.0 != target
                && s.location == location
                && s.audio == audio
                && !s.audio_locked
                && matches!(s.state, LineState::Connected | LineState::Calling)
            {
                if audio == Audio::Pretalk {
                    l.status.audio = Audio::Hold;
                } else {
                    Self::reset_line(l);
                }
                changed.push(l.status.line.0);
            }
        }
    }

    /// Apply a command; returns the lines whose status should be sent.
    fn handle(&mut self, cmd: Command) -> Vec<u8> {
        let now = now_ms();
        let mut changed = Vec::new();
        let owner_ok = |s: &LineStatus, loc: u8| s.location == 0 || s.location == loc;
        match cmd {
            Command::StatusRequest { line } => changed.push(line.0),
            Command::Dial {
                line,
                audio,
                location,
                number,
            } => {
                let i = line.0 as usize;
                let st = self.lines[i].status.clone();
                if owner_ok(&st, location.0) {
                    match st.state {
                        LineState::Ringing => {
                            self.make_room(line.0, audio, location.0, &mut changed);
                            let l = &mut self.lines[i];
                            l.status.state = LineState::Connected;
                            l.status.audio = audio;
                            l.status.location = location.0;
                            l.due_ms = None;
                        }
                        LineState::Disconnected if !number.is_empty() => {
                            self.make_room(line.0, audio, location.0, &mut changed);
                            let answer = self.timers.answer_ms;
                            let l = &mut self.lines[i];
                            l.status.state = LineState::Calling;
                            l.status.audio = audio;
                            l.status.location = location.0;
                            l.status.screening = Screening {
                                phone_number: number.clone(),
                                ..Default::default()
                            };
                            l.busy_number = number.starts_with("999");
                            l.due_ms = Some(now + answer);
                        }
                        _ => {}
                    }
                }
                changed.push(line.0);
            }
            Command::Drop { line, location } => {
                let l = &mut self.lines[line.0 as usize];
                if owner_ok(&l.status, location.0) {
                    Self::reset_line(l);
                }
                changed.push(line.0);
            }
            Command::SetAudio {
                line,
                audio,
                location,
            } => {
                let i = line.0 as usize;
                let st = self.lines[i].status.clone();
                if owner_ok(&st, location.0)
                    && !matches!(st.state, LineState::Disconnected | LineState::Ringing)
                {
                    self.make_room(line.0, audio, location.0, &mut changed);
                    let l = &mut self.lines[i];
                    l.status.audio = audio;
                    l.status.location = location.0;
                }
                changed.push(line.0);
            }
            Command::SetLockAudio {
                line,
                locked,
                location,
            } => {
                let l = &mut self.lines[line.0 as usize];
                if owner_ok(&l.status, location.0) {
                    l.status.audio_locked = locked;
                }
                changed.push(line.0);
            }
            Command::SetLockLine {
                line,
                locked,
                location,
            } => {
                let l = &mut self.lines[line.0 as usize];
                if owner_ok(&l.status, location.0) {
                    l.status.line_locked = locked;
                }
                changed.push(line.0);
            }
            Command::SetConference { .. } | Command::SetNextCaller { .. } => {}
        }
        changed.sort_unstable();
        changed.dedup();
        changed
    }

    fn ring(&mut self, line: u8) -> bool {
        let pick = (self.rand() % CALLERS.len() as u64) as usize;
        let timeout = self.timers.ring_timeout_ms;
        let l = &mut self.lines[line as usize];
        if l.status.state != LineState::Disconnected || l.status.line_locked {
            return false;
        }
        let (p, n, f, c) = CALLERS[pick];
        l.status.state = LineState::Ringing;
        l.status.audio = Audio::Hold;
        l.status.location = 0;
        l.status.screening = Screening {
            phone_number: p.into(),
            name: n.into(),
            first_name: f.into(),
            city: c.into(),
        };
        l.due_ms = Some(now_ms() + timeout);
        true
    }

    /// Advance timers. Returns lines that changed.
    fn tick(&mut self, visible_lines: u8) -> Vec<u8> {
        let now = now_ms();
        let mut changed = Vec::new();
        for l in self.lines.iter_mut() {
            if l.due_ms.is_some_and(|d| d <= now) {
                match l.status.state {
                    LineState::Calling if l.busy_number => Self::reset_line(l),
                    LineState::Calling => {
                        l.status.state = LineState::Connected;
                        l.due_ms = None;
                    }
                    LineState::Ringing => Self::reset_line(l),
                    _ => l.due_ms = None,
                }
                changed.push(l.status.line.0);
            }
        }
        if self.auto_incoming && now >= self.next_auto_call_ms {
            self.next_auto_call_ms = now + 15_000 + self.rand() % 25_000;
            let ringing = self
                .lines
                .iter()
                .filter(|l| l.status.state == LineState::Ringing)
                .count();
            if ringing < 2 && visible_lines > 0 {
                let start = (self.rand() % visible_lines as u64) as u8;
                for k in 0..visible_lines {
                    let i = (start + k) % visible_lines;
                    if self.ring(i) {
                        changed.push(i);
                        break;
                    }
                }
            }
        }
        changed
    }
}

/// Handle to a running simulator.
pub struct Simulator {
    addr: SocketAddr,
    state: Arc<Mutex<SimState>>,
    updates: broadcast::Sender<LineStatus>,
    tasks: Vec<JoinHandle<()>>,
    visible: Arc<std::sync::atomic::AtomicU8>,
}

impl Simulator {
    /// Start on `bind` (use port 0 for an ephemeral port).
    /// `my_location` is used to pick a different location for the demo line
    /// that is "in use by another studio".
    pub async fn start(bind: SocketAddr, my_location: u8) -> std::io::Result<Self> {
        let listener = TcpListener::bind(bind).await?;
        let addr = listener.local_addr()?;
        let other = if my_location == 6 { 5 } else { 6 };
        let state = Arc::new(Mutex::new(SimState::new(other)));
        let (updates, _) = broadcast::channel(256);
        let visible = Arc::new(std::sync::atomic::AtomicU8::new(MAX_LINES));

        let accept = {
            let state = state.clone();
            let updates = updates.clone();
            tokio::spawn(async move {
                while let Ok((sock, _)) = listener.accept().await {
                    tokio::spawn(serve(sock, state.clone(), updates.clone()));
                }
            })
        };
        let ticker = {
            let state = state.clone();
            let updates = updates.clone();
            let visible = visible.clone();
            tokio::spawn(async move {
                let mut iv = tokio::time::interval(Duration::from_millis(250));
                loop {
                    iv.tick().await;
                    let statuses: Vec<LineStatus> = {
                        let mut s = state.lock().unwrap();
                        let changed = s.tick(visible.load(std::sync::atomic::Ordering::Relaxed));
                        changed
                            .iter()
                            .map(|&i| s.lines[i as usize].status.clone())
                            .collect()
                    };
                    for st in statuses {
                        let _ = updates.send(st);
                    }
                }
            })
        };
        Ok(Self {
            addr,
            state,
            updates,
            tasks: vec![accept, ticker],
            visible,
        })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Limit random incoming calls to the lines LineCaller shows.
    pub fn set_visible_lines(&self, n: u8) {
        self.visible
            .store(n.min(MAX_LINES), std::sync::atomic::Ordering::Relaxed);
    }

    pub fn set_auto_incoming(&self, on: bool) {
        let mut s = self.state.lock().unwrap();
        s.auto_incoming = on;
        s.next_auto_call_ms = now_ms() + 6_000;
    }

    /// Make a line ring with a fake caller. Returns false if the line is busy or locked.
    pub fn incoming_call(&self, line: LineIndex) -> bool {
        let st = {
            let mut s = self.state.lock().unwrap();
            if !s.ring(line.0) {
                return false;
            }
            s.lines[line.0 as usize].status.clone()
        };
        let _ = self.updates.send(st);
        true
    }

    #[cfg(test)]
    fn fast_timers(&self) {
        let mut s = self.state.lock().unwrap();
        s.timers = Timers {
            answer_ms: 50,
            ring_timeout_ms: 60_000,
        };
        s.auto_incoming = false;
    }
}

impl Drop for Simulator {
    fn drop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
    }
}

async fn serve(
    sock: TcpStream,
    state: Arc<Mutex<SimState>>,
    updates: broadcast::Sender<LineStatus>,
) {
    let mut rx = updates.subscribe();
    let (mut rd, mut wr) = sock.into_split();
    let mut dec = FrameDecoder::new();
    let mut buf = [0u8; 1024];
    loop {
        tokio::select! {
            r = rd.read(&mut buf) => {
                let n = match r { Ok(0) | Err(_) => return, Ok(n) => n };
                dec.push(&buf[..n]);
                while let Some(item) = dec.next() {
                    let Decoded::Packet(p) = item else { continue };
                    let Ok(cmd) = Command::from_payload(&p) else { continue };
                    let is_status_req = matches!(cmd, Command::StatusRequest { .. });
                    let out: Vec<LineStatus> = {
                        let mut s = state.lock().unwrap();
                        let changed = s.handle(cmd);
                        changed.iter().map(|&i| s.lines[i as usize].status.clone()).collect()
                    };
                    for st in out {
                        if is_status_req {
                            // §4.2.6: the answer goes only to the requester.
                            let pkt = rlp::encode_packet(&st.to_payload()).unwrap();
                            if wr.write_all(&pkt).await.is_err() { return; }
                        } else {
                            let _ = updates.send(st);
                        }
                    }
                }
            }
            u = rx.recv() => match u {
                Ok(st) => {
                    let pkt = rlp::encode_packet(&st.to_payload()).unwrap();
                    if wr.write_all(&pkt).await.is_err() { return; }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rlp::Location;

    #[test]
    fn set_audio_on_air_drops_unlocked_line_at_same_interface() {
        let mut s = SimState::new(6);
        let loc = Location(1);
        s.ring(0);
        s.ring(1);
        s.handle(Command::Dial {
            line: LineIndex(0),
            audio: Audio::OnAir1,
            location: loc,
            number: String::new(),
        });
        s.handle(Command::Dial {
            line: LineIndex(1),
            audio: Audio::Hold,
            location: loc,
            number: String::new(),
        });
        let changed = s.handle(Command::SetAudio {
            line: LineIndex(1),
            audio: Audio::OnAir1,
            location: loc,
        });
        assert_eq!(changed, vec![0, 1]);
        assert_eq!(s.lines[0].status.state, LineState::Disconnected);
        assert_eq!(s.lines[1].status.audio, Audio::OnAir1);
    }

    #[test]
    fn audio_lock_prevents_drop_and_pretalk_goes_to_hold() {
        let mut s = SimState::new(6);
        let loc = Location(1);
        for i in 0..3 {
            s.ring(i);
        }
        s.handle(Command::Dial {
            line: LineIndex(0),
            audio: Audio::OnAir1,
            location: loc,
            number: String::new(),
        });
        s.handle(Command::SetLockAudio {
            line: LineIndex(0),
            locked: true,
            location: loc,
        });
        s.handle(Command::Dial {
            line: LineIndex(1),
            audio: Audio::Pretalk,
            location: loc,
            number: String::new(),
        });
        s.handle(Command::Dial {
            line: LineIndex(2),
            audio: Audio::Hold,
            location: loc,
            number: String::new(),
        });
        s.handle(Command::SetAudio {
            line: LineIndex(2),
            audio: Audio::OnAir1,
            location: loc,
        });
        assert_eq!(
            s.lines[0].status.state,
            LineState::Connected,
            "locked line survives"
        );
        s.handle(Command::SetAudio {
            line: LineIndex(2),
            audio: Audio::Pretalk,
            location: loc,
        });
        assert_eq!(
            s.lines[1].status.audio,
            Audio::Hold,
            "pretalk line moved to hold"
        );
    }

    #[test]
    fn foreign_line_rejects_other_location() {
        let mut s = SimState::new(6);
        s.handle(Command::Drop {
            line: LineIndex(FOREIGN_LINE),
            location: Location(1),
        });
        assert_eq!(
            s.lines[FOREIGN_LINE as usize].status.state,
            LineState::Connected
        );
    }

    #[test]
    fn line_lock_blocks_incoming() {
        let mut s = SimState::new(6);
        s.handle(Command::SetLockLine {
            line: LineIndex(3),
            locked: true,
            location: Location(1),
        });
        assert!(!s.ring(3));
    }

    #[tokio::test]
    async fn speaks_rlp_over_tcp() {
        let sim = Simulator::start("127.0.0.1:0".parse().unwrap(), 1)
            .await
            .unwrap();
        sim.fast_timers();
        let mut c = TcpStream::connect(sim.addr()).await.unwrap();
        c.write_all(
            &Command::Dial {
                line: LineIndex(2),
                audio: Audio::Pretalk,
                location: Location(1),
                number: "0201234".into(),
            }
            .to_packet()
            .unwrap(),
        )
        .await
        .unwrap();
        let mut dec = FrameDecoder::new();
        let mut buf = [0u8; 512];
        let mut states = vec![];
        while states.len() < 2 {
            let n = tokio::time::timeout(Duration::from_secs(3), c.read(&mut buf))
                .await
                .unwrap()
                .unwrap();
            dec.push(&buf[..n]);
            while let Some(Decoded::Packet(p)) = dec.next() {
                let s = LineStatus::from_payload(&p).unwrap();
                assert_eq!(s.line, LineIndex(2));
                states.push(s.state);
            }
        }
        assert_eq!(states, vec![LineState::Calling, LineState::Connected]);
    }
}
