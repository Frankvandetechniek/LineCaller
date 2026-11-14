//! TCP connection manager for the Remote Light Protocol.
//!
//! One background task owns the socket. Everything else talks to it through
//! [`ClientHandle`], so network work never blocks the UI thread and there is
//! exactly one writer on the socket.
//!
//! Responsibilities:
//! * connect with a timeout, reconnect with back-off after any failure;
//! * re-assemble packets across arbitrary TCP reads ([`rlp::FrameDecoder`]);
//! * request the status of every displayed line after (re)connecting;
//! * optional liveness check (an app-side assumption, see PROTOCOL_NOTES.md);
//! * report connection state, line status and protocol warnings to an
//!   [`EventSink`].

use crate::store::{now_ms, LineStore, LineView};
use rlp::{Command, Decoded, FrameDecoder, FrameIssue, Incoming, LineIndex, Location};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep, timeout};

pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(3);
/// How long to wait for a reply to a liveness probe (capped at the interval).
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const BACKOFF_MS: [u64; 6] = [500, 1_000, 2_000, 5_000, 10_000, 15_000];

/// What the connection task needs to know. Derived from [`crate::Settings`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientConfig {
    pub host: String,
    pub port: u16,
    pub location: Location,
    pub line_count: u8,
    /// `None` disables the liveness check.
    pub liveness_interval: Option<Duration>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "phase"
)]
pub enum ConnectionPhase {
    /// Not trying to connect (no configuration, or the user disconnected).
    Offline,
    Connecting {
        attempt: u32,
    },
    Connected {
        since_ms: u64,
    },
    /// Waiting before the next reconnect attempt.
    Retrying {
        attempt: u32,
        retry_at_ms: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    #[serde(flatten)]
    pub phase: ConnectionPhase,
    /// "host:port" being used.
    pub endpoint: String,
    pub last_error: Option<String>,
}

impl ConnectionStatus {
    fn offline() -> Self {
        Self {
            phase: ConnectionPhase::Offline,
            endpoint: String::new(),
            last_error: None,
        }
    }
    pub fn is_connected(&self) -> bool {
        matches!(self.phase, ConnectionPhase::Connected { .. })
    }
}

/// Where the connection task reports to. Implemented by the Tauri layer
/// (emits events) and by tests (records them).
pub trait EventSink: Send + Sync + 'static {
    fn connection(&self, status: &ConnectionStatus);
    fn line(&self, view: &LineView);
    fn warning(&self, message: &str);
}

#[derive(Debug, Error, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "message", rename_all = "camelCase")]
pub enum SendError {
    #[error("Not connected to the THipPro.")]
    NotConnected,
    #[error("Sending to the THipPro failed: {0}")]
    Io(String),
    #[error("{0}")]
    Invalid(String),
    #[error("The connection task has stopped.")]
    Stopped,
}

enum Ctl {
    Send(Command, oneshot::Sender<Result<(), SendError>>),
    Configure(Option<ClientConfig>),
    /// Skip the back-off wait and try again now.
    RetryNow,
    Shutdown,
}

/// Cheap, cloneable handle to the connection task.
#[derive(Clone)]
pub struct ClientHandle {
    tx: mpsc::Sender<Ctl>,
    shared: Arc<Mutex<Shared>>,
}

struct Shared {
    status: ConnectionStatus,
    lines: LineStore,
}

impl ClientHandle {
    /// Spawn the connection task on the current Tokio runtime.
    pub fn spawn(sink: Arc<dyn EventSink>) -> Self {
        let (tx, rx) = mpsc::channel(64);
        let shared = Arc::new(Mutex::new(Shared {
            status: ConnectionStatus::offline(),
            lines: LineStore::new(),
        }));
        let task = Task {
            rx,
            sink,
            shared: shared.clone(),
            cfg: None,
        };
        tokio::spawn(task.run());
        Self { tx, shared }
    }

    /// Apply new settings. `None` disconnects and stays offline.
    pub async fn configure(&self, cfg: Option<ClientConfig>) {
        let _ = self.tx.send(Ctl::Configure(cfg)).await;
    }

    pub async fn retry_now(&self) {
        let _ = self.tx.send(Ctl::RetryNow).await;
    }

    pub async fn shutdown(&self) {
        let _ = self.tx.send(Ctl::Shutdown).await;
    }

    /// Validate, encode and write one command. Resolves once the bytes are
    /// written to the socket. The protocol has no acknowledgement, so the
    /// effect is only visible when the THipPro sends the next Line Status.
    pub async fn send(&self, cmd: Command) -> Result<(), SendError> {
        cmd.to_payload()
            .map_err(|e| SendError::Invalid(e.to_string()))?;
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Ctl::Send(cmd, reply_tx))
            .await
            .map_err(|_| SendError::Stopped)?;
        reply_rx.await.map_err(|_| SendError::Stopped)?
    }

    pub fn status(&self) -> ConnectionStatus {
        self.shared.lock().unwrap().status.clone()
    }

    pub fn lines(&self) -> Vec<Option<LineView>> {
        self.shared.lock().unwrap().lines.snapshot()
    }

    pub fn line(&self, index: u8) -> Option<LineView> {
        self.shared.lock().unwrap().lines.get(index).cloned()
    }
}

/// Why a live session ended.
enum SessionEnd {
    /// Link problem: back off and reconnect to the same endpoint.
    Lost(String),
    /// Settings changed or user disconnected: start over immediately.
    Reconfigured,
    Shutdown,
}

/// Outcome of handling a control message while not connected.
enum Offline {
    Keep,
    Restart,
    Stop,
}

struct Task {
    rx: mpsc::Receiver<Ctl>,
    sink: Arc<dyn EventSink>,
    shared: Arc<Mutex<Shared>>,
    cfg: Option<ClientConfig>,
}

impl Task {
    fn publish(&self, phase: ConnectionPhase, last_error: Option<String>) {
        let endpoint = self
            .cfg
            .as_ref()
            .map(|c| format!("{}:{}", c.host, c.port))
            .unwrap_or_default();
        let status = ConnectionStatus {
            phase,
            endpoint,
            last_error,
        };
        self.shared.lock().unwrap().status = status.clone();
        self.sink.connection(&status);
    }

    fn mark_stale(&self) {
        let stale = self.shared.lock().unwrap().lines.mark_all_stale();
        for v in &stale {
            self.sink.line(v);
        }
    }

    /// Handle a control message while there is no socket.
    fn offline_ctl(&mut self, ctl: Option<Ctl>) -> Offline {
        match ctl {
            None | Some(Ctl::Shutdown) => Offline::Stop,
            Some(Ctl::Send(_, reply)) => {
                let _ = reply.send(Err(SendError::NotConnected));
                Offline::Keep
            }
            Some(Ctl::Configure(cfg)) => self.reconfigure(cfg),
            Some(Ctl::RetryNow) => Offline::Restart,
        }
    }

    fn reconfigure(&mut self, cfg: Option<ClientConfig>) -> Offline {
        let endpoint_changed = match (&self.cfg, &cfg) {
            (Some(a), Some(b)) => a.host != b.host || a.port != b.port,
            _ => true,
        };
        if endpoint_changed {
            self.shared.lock().unwrap().lines.clear();
        }
        self.cfg = cfg;
        Offline::Restart
    }

    async fn run(mut self) {
        let mut attempt: u32 = 0;
        let mut last_error: Option<String> = None;
        loop {
            let Some(cfg) = self.cfg.clone() else {
                attempt = 0;
                self.publish(ConnectionPhase::Offline, None);
                loop {
                    let ctl = self.rx.recv().await;
                    match self.offline_ctl(ctl) {
                        Offline::Keep => continue,
                        Offline::Restart => break,
                        Offline::Stop => return,
                    }
                }
                continue;
            };

            attempt += 1;
            self.publish(ConnectionPhase::Connecting { attempt }, last_error.clone());
            let connect = timeout(
                CONNECT_TIMEOUT,
                TcpStream::connect((cfg.host.as_str(), cfg.port)),
            );
            tokio::pin!(connect);
            let result = loop {
                tokio::select! {
                    r = &mut connect => break Some(r),
                    ctl = self.rx.recv() => match self.offline_ctl(ctl) {
                        Offline::Keep => continue,
                        Offline::Restart => break None,
                        Offline::Stop => return,
                    },
                }
            };
            let end = match result {
                None => {
                    attempt = 0;
                    last_error = None;
                    continue;
                }
                Some(Err(_)) => SessionEnd::Lost(format!(
                    "No answer from {}:{} within {}s",
                    cfg.host,
                    cfg.port,
                    CONNECT_TIMEOUT.as_secs()
                )),
                Some(Ok(Err(e))) => SessionEnd::Lost(describe_io(&e)),
                Some(Ok(Ok(stream))) => {
                    attempt = 0;
                    let end = self.session(stream, &cfg).await;
                    self.mark_stale();
                    end
                }
            };

            match end {
                SessionEnd::Shutdown => return,
                SessionEnd::Reconfigured => {
                    attempt = 0;
                    last_error = None;
                    continue;
                }
                SessionEnd::Lost(err) => {
                    tracing::warn!("RLP link: {err}");
                    last_error = Some(err);
                    let delay = Duration::from_millis(
                        BACKOFF_MS[(attempt as usize).min(BACKOFF_MS.len() - 1)],
                    );
                    self.publish(
                        ConnectionPhase::Retrying {
                            attempt: attempt.max(1),
                            retry_at_ms: now_ms() + delay.as_millis() as u64,
                        },
                        last_error.clone(),
                    );
                    let wait = sleep(delay);
                    tokio::pin!(wait);
                    loop {
                        tokio::select! {
                            _ = &mut wait => break,
                            ctl = self.rx.recv() => match self.offline_ctl(ctl) {
                                Offline::Keep => continue,
                                Offline::Restart => { if self.cfg.as_ref() != Some(&cfg) { attempt = 0; last_error = None; } break }
                                Offline::Stop => return,
                            },
                        }
                    }
                }
            }
        }
    }

    async fn session(&mut self, stream: TcpStream, cfg: &ClientConfig) -> SessionEnd {
        let _ = stream.set_nodelay(true);
        let sock = socket2::SockRef::from(&stream);
        let ka = socket2::TcpKeepalive::new()
            .with_time(Duration::from_secs(10))
            .with_interval(Duration::from_secs(5));
        let _ = sock.set_tcp_keepalive(&ka);

        let (mut rd, mut wr) = stream.into_split();
        let mut decoder = FrameDecoder::new();
        let mut buf = [0u8; 2048];
        let mut cfg = cfg.clone();

        self.publish(ConnectionPhase::Connected { since_ms: now_ms() }, None);

        // Initial sync: ask for the status of every displayed line.
        for i in 0..cfg.line_count {
            if let Err(e) = write_cmd(&mut wr, &Command::StatusRequest { line: LineIndex(i) }).await
            {
                return SessionEnd::Lost(e);
            }
        }

        let mut last_rx = Instant::now();
        let mut probe_sent: Option<Instant> = None;
        let mut tick = tokio::time::interval(Duration::from_millis(500));
        let mut warn_budget = WarnBudget::default();

        loop {
            tokio::select! {
                read = rd.read(&mut buf) => {
                    let n = match read {
                        Ok(0) => return SessionEnd::Lost("The THipPro closed the connection".into()),
                        Ok(n) => n,
                        Err(e) => return SessionEnd::Lost(describe_io(&e)),
                    };
                    last_rx = Instant::now();
                    probe_sent = None;
                    decoder.push(&buf[..n]);
                    while let Some(item) = decoder.next() {
                        match item {
                            Decoded::Packet(p) => match Incoming::from_payload(&p) {
                                Ok(Incoming::LineStatus(s)) => {
                                    let changed = self.shared.lock().unwrap().lines.apply(s);
                                    if let Some(v) = changed { self.sink.line(&v); }
                                }
                                Err(e) => warn_budget.warn(&*self.sink, format!("Ignored message: {e}")),
                            },
                            Decoded::Issue(issue) => warn_budget.warn(&*self.sink, describe_issue(&issue)),
                        }
                    }
                }
                ctl = self.rx.recv() => match ctl {
                    None | Some(Ctl::Shutdown) => return SessionEnd::Shutdown,
                    Some(Ctl::RetryNow) => {}
                    Some(Ctl::Send(cmd, reply)) => {
                        let r = write_cmd(&mut wr, &cmd).await;
                        let failed = r.is_err();
                        let _ = reply.send(r.clone().map_err(SendError::Io));
                        if failed { return SessionEnd::Lost(r.unwrap_err()); }
                    }
                    Some(Ctl::Configure(new)) => {
                        match new {
                            Some(n) if n.host == cfg.host && n.port == cfg.port => {
                                // Same endpoint: keep the socket, sync newly shown lines.
                                for i in cfg.line_count..n.line_count {
                                    if let Err(e) = write_cmd(&mut wr, &Command::StatusRequest { line: LineIndex(i) }).await {
                                        self.cfg = Some(n);
                                        return SessionEnd::Lost(e);
                                    }
                                }
                                self.cfg = Some(n.clone());
                                cfg = n;
                            }
                            other => { self.reconfigure(other); return SessionEnd::Reconfigured; }
                        }
                    }
                },
                _ = tick.tick() => {
                    if let Some(interval) = cfg.liveness_interval {
                        match probe_sent {
                            None if last_rx.elapsed() >= interval => {
                                if let Err(e) = write_cmd(&mut wr, &Command::StatusRequest { line: LineIndex(0) }).await {
                                    return SessionEnd::Lost(e);
                                }
                                probe_sent = Some(Instant::now());
                            }
                            Some(t) if t.elapsed() >= interval.min(PROBE_TIMEOUT) => {
                                return SessionEnd::Lost(format!(
                                    "No reply to a status request within {}s (liveness check)",
                                    interval.min(PROBE_TIMEOUT).as_secs_f32()));
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}

async fn write_cmd(wr: &mut tokio::net::tcp::OwnedWriteHalf, cmd: &Command) -> Result<(), String> {
    let bytes = cmd.to_packet().map_err(|e| e.to_string())?;
    match timeout(WRITE_TIMEOUT, wr.write_all(&bytes)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(describe_io(&e)),
        Err(_) => Err(format!(
            "Write did not complete within {}s",
            WRITE_TIMEOUT.as_secs()
        )),
    }
}

fn describe_io(e: &std::io::Error) -> String {
    use std::io::ErrorKind::*;
    match e.kind() {
        ConnectionRefused => "Connection refused (is Remote Light enabled on this port?)".into(),
        ConnectionReset | ConnectionAborted | BrokenPipe => {
            "Connection dropped by the network or THipPro".into()
        }
        TimedOut => "Network timeout".into(),
        _ => e.to_string(),
    }
}

fn describe_issue(issue: &FrameIssue) -> String {
    match issue {
        FrameIssue::Garbage { discarded } => {
            format!("Discarded {discarded} byte(s) outside any packet")
        }
        FrameIssue::UnsupportedVersion { version } => {
            format!("Ignored packet with unsupported version 0x{version:02X}")
        }
        FrameIssue::BadEndFlag { found } => {
            format!("Ignored packet with bad end flag 0x{found:02X}")
        }
    }
}

/// Avoid flooding the UI if a peer sends a stream of junk: at most 5
/// warnings per 10 seconds, then one summary.
#[derive(Default)]
struct WarnBudget {
    window_start: Option<Instant>,
    count: u32,
}

impl WarnBudget {
    fn warn(&mut self, sink: &dyn EventSink, msg: String) {
        tracing::warn!("RLP: {msg}");
        let now = Instant::now();
        if self
            .window_start
            .is_none_or(|s| now.duration_since(s) > Duration::from_secs(10))
        {
            self.window_start = Some(now);
            self.count = 0;
        }
        self.count += 1;
        match self.count {
            1..=5 => sink.warning(&msg),
            6 => sink.warning("Further protocol warnings suppressed for 10 seconds"),
            _ => {}
        }
    }
}
