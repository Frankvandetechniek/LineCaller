//! Optional, read-only connection to the THipPro's Command Protocol port that
//! feeds the R/T level meters. It only ever sends Poll telegrams (to connect
//! and to keep the connection alive, §3 / §3.4) and listens for "Audio Level
//! information" (§20.2). It never sends a command that changes the unit.

use crate::cmdproto::{self, Decoder, TYPE_CONNECTION_REJECT};
use crate::connection::EventSink;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::watch;
use tokio::task::JoinHandle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelConfig {
    pub host: String,
    pub port: u16,
    /// Poll connection type (§3.4): 0 Maintenance, 1 MAGIC Touch, 2 Screener.
    pub connection_type: u8,
    pub password: String,
}

/// The unit drops a PC after 5 s without a valid telegram (§3).
const POLL_EVERY: Duration = Duration::from_secs(2);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const RETRY_AFTER: Duration = Duration::from_secs(5);
const RETRY_AFTER_REJECT: Duration = Duration::from_secs(30);

pub struct LevelLink {
    cfg: watch::Sender<Option<LevelConfig>>,
    task: JoinHandle<()>,
}

impl LevelLink {
    pub fn spawn(sink: Arc<dyn EventSink>) -> Self {
        let (tx, rx) = watch::channel(None);
        let task = tokio::spawn(run(rx, sink));
        Self { cfg: tx, task }
    }

    /// `None` closes the link.
    pub fn configure(&self, cfg: Option<LevelConfig>) {
        self.cfg.send_if_modified(|c| {
            let changed = *c != cfg;
            *c = cfg;
            changed
        });
    }
}

impl Drop for LevelLink {
    fn drop(&mut self) {
        self.task.abort();
    }
}

enum End {
    Reconfigured,
    Retry(Duration, Option<String>),
}

async fn run(mut rx: watch::Receiver<Option<LevelConfig>>, sink: Arc<dyn EventSink>) {
    let mut last_problem: Option<String> = None;
    loop {
        let cfg = rx.borrow_and_update().clone();
        let Some(cfg) = cfg else {
            last_problem = None;
            if rx.changed().await.is_err() {
                return;
            }
            continue;
        };
        match session(&cfg, &mut rx, &*sink).await {
            End::Reconfigured => last_problem = None,
            End::Retry(wait, problem) => {
                if let Some(p) = problem {
                    if last_problem.as_deref() != Some(p.as_str()) {
                        sink.warning(&format!("Level meters: {p}"));
                    }
                    last_problem = Some(p);
                }
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {}
                    r = rx.changed() => if r.is_err() { return },
                }
            }
        }
    }
}

async fn session(
    cfg: &LevelConfig,
    rx: &mut watch::Receiver<Option<LevelConfig>>,
    sink: &dyn EventSink,
) -> End {
    let addr = format!("{}:{}", cfg.host, cfg.port);
    let mut stream = match tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(&addr)).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return End::Retry(RETRY_AFTER, Some(format!("cannot reach {addr} ({e})"))),
        Err(_) => return End::Retry(RETRY_AFTER, Some(format!("no answer from {addr}"))),
    };
    let _ = stream.set_nodelay(true);
    let poll = cmdproto::poll(cfg.connection_type, &cfg.password);
    let mut decoder = Decoder::default();
    let mut buf = [0u8; 4096];
    let mut tick = tokio::time::interval(POLL_EVERY);
    loop {
        tokio::select! {
            _ = tick.tick() => {
                if let Err(e) = stream.write_all(&poll).await {
                    return End::Retry(RETRY_AFTER, Some(format!("connection lost ({e})")));
                }
            }
            r = stream.read(&mut buf) => match r {
                Ok(0) => return End::Retry(RETRY_AFTER, Some("the THipPro closed the connection".into())),
                Err(e) => return End::Retry(RETRY_AFTER, Some(format!("connection lost ({e})"))),
                Ok(n) => {
                    decoder.push(&buf[..n]);
                    while let Some(t) = decoder.next_telegram() {
                        if t.telegram_type == TYPE_CONNECTION_REJECT {
                            return End::Retry(
                                RETRY_AFTER_REJECT,
                                Some(format!("the THipPro refused the connection: {}", cmdproto::reject_reason(&t.data))),
                            );
                        }
                        if let Some(levels) = cmdproto::parse_levels(&t) {
                            sink.levels(&levels);
                        }
                    }
                }
            },
            r = rx.changed() => {
                let _ = r;
                return End::Reconfigured;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConnectionStatus, LineLevels, LineView};
    use std::sync::Mutex;
    use tokio::net::TcpListener;

    #[derive(Default)]
    struct Sink(Mutex<Vec<LineLevels>>, Mutex<Vec<String>>);
    impl EventSink for Sink {
        fn connection(&self, _: &ConnectionStatus) {}
        fn line(&self, _: &LineView) {}
        fn warning(&self, m: &str) {
            self.1.lock().unwrap().push(m.into());
        }
        fn levels(&self, l: &LineLevels) {
            self.0.lock().unwrap().push(l.clone());
        }
    }

    #[tokio::test]
    async fn polls_and_receives_levels_only() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let sink = Arc::new(Sink::default());
        let link = LevelLink::spawn(sink.clone());
        link.configure(Some(LevelConfig {
            host: "127.0.0.1".into(),
            port,
            connection_type: cmdproto::CONN_SCREENER,
            password: String::new(),
        }));
        let (mut sock, _) = listener.accept().await.unwrap();
        // First thing the link sends is a Poll telegram, nothing else.
        let mut got = vec![0u8; 12];
        sock.read_exact(&mut got).await.unwrap();
        let mut d = Decoder::default();
        d.push(&got);
        assert_eq!(
            d.next_telegram().unwrap().telegram_type,
            cmdproto::TYPE_POLL
        );
        // The unit pushes a version 03h level telegram.
        let mut data = vec![cmdproto::MISC_AUDIO_LEVELS, 3];
        data.extend(std::iter::repeat_n((-20i8) as u8, 28));
        data.extend(std::iter::repeat_n((-40i8) as u8, 28));
        sock.write_all(&cmdproto::encode(1, cmdproto::TYPE_MISC, &data))
            .await
            .unwrap();
        for _ in 0..50 {
            if !sink.0.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let l = sink.0.lock().unwrap()[0].clone();
        assert_eq!((l.rx[3], l.tx[3], l.source), (-20, -40, "thippro"));
        // A Connection Reject becomes one readable warning.
        sock.write_all(&cmdproto::encode(2, TYPE_CONNECTION_REJECT, &[1]))
            .await
            .unwrap();
        for _ in 0..50 {
            if !sink.1.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(sink.1.lock().unwrap()[0].contains("licensed PC connections"));
    }
}
