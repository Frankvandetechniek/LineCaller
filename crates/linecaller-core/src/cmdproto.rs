//! The small, read-only part of the AVT THipPro **Command Protocol** that
//! LineCaller uses for its R/T level meters. Nothing else from that protocol
//! is implemented: LineCaller only sends Poll telegrams (connect and keep
//! alive) and listens for "Audio Level information".
//!
//! Packet (Command Protocol §2), no byte stuffing, length-framed:
//!
//! ```text
//! FE | dst port | dst addr | src port | src addr | len lo | len hi | seq | type | data… | crc lo | crc hi
//! ```
//!
//! `len` counts everything except the start flag and the CRC. The CRC is
//! CRC-16/X.25 over the same bytes.

use crate::levels::LineLevels;

pub const START: u8 = 0xFE;
pub const TYPE_POLL: u8 = 0x34;
pub const TYPE_PACKET_RESPONSE: u8 = 0x38;
pub const TYPE_PACKET_REJECT: u8 = 0x3C;
pub const TYPE_CONNECTION_REJECT: u8 = 0x40;
pub const TYPE_MISC: u8 = 0x6C;
pub const MISC_AUDIO_LEVELS: u8 = 0x0E;

/// Header bytes counted in `len` before the data field.
const HEADER: usize = 8;
/// Refuse absurd lengths so a corrupt length byte cannot stall the decoder.
const MAX_LEN: usize = 4096;

/// Poll telegram connection types (§3.4).
pub const CONN_MAINTENANCE: u8 = 0;
pub const CONN_MAGIC_TOUCH: u8 = 1;
pub const CONN_SCREENER: u8 = 2;

/// CRC-16/X.25 (reflected 0x1021, init and final XOR 0xFFFF).
pub fn crc16_x25(bytes: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in bytes {
        crc ^= b as u16;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0x8408
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// Build a telegram with all port and address fields 0, as §2.2 asks for
/// "standard applications".
pub fn encode(seq: u8, telegram_type: u8, data: &[u8]) -> Vec<u8> {
    let len = HEADER + data.len();
    let mut body = vec![
        0,
        0,
        0,
        0,
        (len & 0xFF) as u8,
        (len >> 8) as u8,
        seq,
        telegram_type,
    ];
    body.extend_from_slice(data);
    let crc = crc16_x25(&body);
    let mut out = Vec::with_capacity(body.len() + 3);
    out.push(START);
    out.extend_from_slice(&body);
    out.push((crc & 0xFF) as u8);
    out.push((crc >> 8) as u8);
    out
}

/// Poll telegram (§3.4): connection type plus an optional password. It does
/// not use the sequence counter, which LineCaller therefore never advances.
pub fn poll(connection_type: u8, password: &str) -> Vec<u8> {
    let mut data = vec![connection_type];
    if !password.is_empty() {
        data.extend(password.bytes().take(20));
        data.push(0);
    }
    encode(0, TYPE_POLL, &data)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Telegram {
    pub telegram_type: u8,
    pub seq: u8,
    pub data: Vec<u8>,
}

/// Stream decoder: feed bytes as they arrive, take complete telegrams out.
#[derive(Default)]
pub struct Decoder {
    buf: Vec<u8>,
}

impl Decoder {
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
        if self.buf.len() > 4 * MAX_LEN {
            self.buf.clear();
        }
    }

    /// Next valid telegram, skipping garbage and telegrams with a bad CRC.
    pub fn next_telegram(&mut self) -> Option<Telegram> {
        loop {
            match self.buf.iter().position(|&b| b == START) {
                Some(0) => {}
                Some(i) => {
                    self.buf.drain(..i);
                }
                None => {
                    self.buf.clear();
                    return None;
                }
            }
            if self.buf.len() < 1 + 7 {
                return None;
            }
            let len = self.buf[5] as usize | (self.buf[6] as usize) << 8;
            if !(HEADER..=MAX_LEN).contains(&len) {
                self.buf.drain(..1);
                continue;
            }
            let total = 1 + len + 2;
            if self.buf.len() < total {
                return None;
            }
            let body = &self.buf[1..1 + len];
            let crc = self.buf[1 + len] as u16 | (self.buf[2 + len] as u16) << 8;
            if crc16_x25(body) != crc {
                self.buf.drain(..1);
                continue;
            }
            let t = Telegram {
                seq: body[6],
                telegram_type: body[7],
                data: body[HEADER..].to_vec(),
            };
            self.buf.drain(..total);
            return Some(t);
        }
    }
}

/// Parse "Audio Level information" (§20.2): `0E`, version, then all input
/// levels followed by the same number of output levels. Every version in
/// the spec (03h–09h) lists inputs and outputs symmetrically, starting with
/// Line In 1…16 and Line Out 1…16, so Line Out 1 sits at half the level
/// bytes. Line In = what the THipPro receives from the caller (R), Line Out
/// = what it sends to the caller (T). Values are signed dB in 1 dB steps.
pub fn parse_levels(t: &Telegram) -> Option<LineLevels> {
    if t.telegram_type != TYPE_MISC || t.data.first() != Some(&MISC_AUDIO_LEVELS) {
        return None;
    }
    let levels = t.data.get(2..)?;
    if levels.len() < 32 || levels.len() % 2 != 0 {
        return None;
    }
    let half = levels.len() / 2;
    let db = |b: u8| b as i8;
    Some(LineLevels {
        rx: levels[..16].iter().map(|&b| db(b)).collect(),
        tx: levels[half..half + 16].iter().map(|&b| db(b)).collect(),
        source: "thippro",
    })
}

/// Human-readable reason of a Connection Reject telegram (§3.3).
pub fn reject_reason(data: &[u8]) -> &'static str {
    match data.first() {
        Some(1) => "all licensed PC connections are in use",
        Some(2) => "wrong password",
        Some(3) => "too many PC connections (max. 20)",
        Some(4) => "this connection type is not allowed on the THipPro",
        _ => "unknown reason",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_x25_check_value() {
        assert_eq!(crc16_x25(b"123456789"), 0x906E);
    }

    #[test]
    fn poll_layout() {
        let p = poll(CONN_SCREENER, "");
        assert_eq!(&p[..10], &[0xFE, 0, 0, 0, 0, 9, 0, 0, 0x34, 2]);
        assert_eq!(p.len(), 1 + 9 + 2);
        let mut d = Decoder::default();
        d.push(&p);
        let t = d.next_telegram().unwrap();
        assert_eq!(
            (t.telegram_type, t.data.as_slice()),
            (TYPE_POLL, &[2u8][..])
        );
    }

    fn level_telegram(version: u8, inputs: usize) -> Vec<u8> {
        let mut data = vec![MISC_AUDIO_LEVELS, version];
        for i in 0..inputs {
            data.push(if i < 16 {
                (-(i as i8) - 10) as u8
            } else {
                0x80
            });
        }
        for i in 0..inputs {
            data.push(if i < 16 {
                (-(i as i8) - 30) as u8
            } else {
                0x80
            });
        }
        encode(5, TYPE_MISC, &data)
    }

    #[test]
    fn parses_every_documented_level_version() {
        // Input counts per version from §20.2.
        for (version, inputs) in [(3, 28), (4, 34), (6, 38), (7, 40), (8, 42), (9, 74)] {
            let mut d = Decoder::default();
            d.push(&[0x00, 0x13]); // leading garbage
            d.push(&level_telegram(version, inputs));
            let l = parse_levels(&d.next_telegram().unwrap()).unwrap();
            assert_eq!(l.rx[0], -10, "version {version}");
            assert_eq!(l.rx[15], -25);
            assert_eq!(l.tx[0], -30);
            assert_eq!(l.tx[15], -45);
        }
    }

    #[test]
    fn bad_crc_is_skipped_and_stream_resyncs() {
        let mut bad = level_telegram(3, 28);
        let n = bad.len();
        bad[n - 1] ^= 0xFF;
        let good = poll(CONN_SCREENER, "pw");
        let mut d = Decoder::default();
        d.push(&bad);
        d.push(&good[..5]);
        assert!(d.next_telegram().is_none());
        d.push(&good[5..]);
        assert_eq!(d.next_telegram().unwrap().telegram_type, TYPE_POLL);
    }
}
