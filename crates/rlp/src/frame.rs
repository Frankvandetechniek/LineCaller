//! Packet framing for the Remote Light Protocol (spec §3).
//!
//! ```text
//! +------+---------+--------+-------------------+------+
//! | FE   | 01      | len    | payload (len B)   | FD   |
//! +------+---------+--------+-------------------+------+
//! ```
//!
//! The spec says a packet is valid only if the start flag, version and end
//! flag hold their expected values, and only valid packets are processed.
//! It does not say how a receiver should recover from garbage, so
//! [`FrameDecoder`] uses the conservative strategy documented on the type.

use thiserror::Error;

/// Start flag of a packet.
pub const START_FLAG: u8 = 0xFE;
/// End flag of a packet.
pub const END_FLAG: u8 = 0xFD;
/// The only packet version defined by the spec (all others are reserved).
pub const VERSION: u8 = 0x01;
/// Header bytes before the payload: start flag, version, length.
pub const HEADER_LEN: usize = 3;
/// The length field is one byte, so a payload is at most 255 bytes.
pub const MAX_PAYLOAD_LEN: usize = u8::MAX as usize;
/// Largest possible packet on the wire.
pub const MAX_PACKET_LEN: usize = HEADER_LEN + MAX_PAYLOAD_LEN + 1;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EncodeError {
    #[error("payload is {0} bytes; the length field allows at most 255")]
    PayloadTooLong(usize),
    #[error("payload must not be empty")]
    EmptyPayload,
}

/// Wrap a payload in a v1 packet.
pub fn encode_packet(payload: &[u8]) -> Result<Vec<u8>, EncodeError> {
    if payload.is_empty() {
        return Err(EncodeError::EmptyPayload);
    }
    if payload.len() > MAX_PAYLOAD_LEN {
        return Err(EncodeError::PayloadTooLong(payload.len()));
    }
    let mut out = Vec::with_capacity(payload.len() + HEADER_LEN + 1);
    out.push(START_FLAG);
    out.push(VERSION);
    out.push(payload.len() as u8);
    out.extend_from_slice(payload);
    out.push(END_FLAG);
    Ok(out)
}

/// Why bytes were discarded by the decoder. Reported so the connection layer
/// can surface protocol problems instead of silently swallowing them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameIssue {
    /// Bytes before a start flag that could not belong to any packet.
    Garbage { discarded: usize },
    /// A start flag followed by a version other than 0x01.
    UnsupportedVersion { version: u8 },
    /// A complete-length packet whose last byte was not 0xFD.
    BadEndFlag { found: u8 },
}

/// One item produced by the decoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decoded {
    /// The payload of a valid packet.
    Packet(Vec<u8>),
    /// Something invalid was skipped; decoding continues afterwards.
    Issue(FrameIssue),
}

/// Incremental, allocation-bounded packet decoder.
///
/// Feed it whatever a TCP read returned (any number of bytes, including
/// partial or multiple packets) and drain items with [`FrameDecoder::next`].
///
/// Recovery strategy (an implementation choice, the spec defines none):
/// * Bytes before a `FE` start flag are dropped and reported as garbage.
/// * If the version byte is not `01`, or the byte at the position the length
///   field points to is not `FD`, only the leading `FE` is dropped and the
///   search for the next `FE` resumes from the following byte. This lets the
///   decoder re-synchronise when a stray `FE` appears in noise, and never
///   lets one corrupt length byte swallow a following valid packet.
/// * The buffer never holds more than one maximum-size packet of pending data,
///   so a peer cannot make it grow without bound.
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buf: Vec<u8>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(MAX_PACKET_LEN * 2),
        }
    }

    /// Append freshly received bytes.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Bytes buffered but not yet consumed (e.g. a partial packet).
    pub fn pending(&self) -> usize {
        self.buf.len()
    }

    /// Drop any partially received data (call after a reconnect).
    pub fn reset(&mut self) {
        self.buf.clear();
    }

    /// Produce the next packet or issue, or `None` if more bytes are needed.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<Decoded> {
        // 1. Align on a start flag.
        match self.buf.iter().position(|&b| b == START_FLAG) {
            None => {
                if self.buf.is_empty() {
                    return None;
                }
                let n = self.buf.len();
                self.buf.clear();
                return Some(Decoded::Issue(FrameIssue::Garbage { discarded: n }));
            }
            Some(0) => {}
            Some(pos) => {
                self.buf.drain(..pos);
                return Some(Decoded::Issue(FrameIssue::Garbage { discarded: pos }));
            }
        }

        // 2. Version.
        if self.buf.len() < 2 {
            return None;
        }
        let version = self.buf[1];
        if version != VERSION {
            self.buf.drain(..1);
            return Some(Decoded::Issue(FrameIssue::UnsupportedVersion { version }));
        }

        // 3. Length + payload + end flag.
        if self.buf.len() < HEADER_LEN {
            return None;
        }
        let len = self.buf[2] as usize;
        let total = HEADER_LEN + len + 1;
        if self.buf.len() < total {
            return None;
        }
        let end = self.buf[total - 1];
        if end != END_FLAG {
            self.buf.drain(..1);
            return Some(Decoded::Issue(FrameIssue::BadEndFlag { found: end }));
        }
        let payload = self.buf[HEADER_LEN..total - 1].to_vec();
        self.buf.drain(..total);
        Some(Decoded::Packet(payload))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(d: &mut FrameDecoder) -> Vec<Decoded> {
        std::iter::from_fn(|| d.next()).collect()
    }

    #[test]
    fn encodes_header_and_trailer() {
        assert_eq!(
            encode_packet(&[0x06, 0x00]).unwrap(),
            vec![0xFE, 0x01, 0x02, 0x06, 0x00, 0xFD]
        );
    }

    #[test]
    fn rejects_bad_payload_sizes() {
        assert_eq!(encode_packet(&[]), Err(EncodeError::EmptyPayload));
        assert_eq!(
            encode_packet(&[0u8; 256]),
            Err(EncodeError::PayloadTooLong(256))
        );
        assert!(encode_packet(&[0u8; 255]).is_ok());
    }

    #[test]
    fn decodes_byte_by_byte() {
        let pkt = encode_packet(&[0x40, 1, 2, 3]).unwrap();
        let mut d = FrameDecoder::new();
        for (i, b) in pkt.iter().enumerate() {
            d.push(&[*b]);
            let out = drain(&mut d);
            if i + 1 < pkt.len() {
                assert!(out.is_empty(), "emitted early at byte {i}");
            } else {
                assert_eq!(out, vec![Decoded::Packet(vec![0x40, 1, 2, 3])]);
            }
        }
        assert_eq!(d.pending(), 0);
    }

    #[test]
    fn decodes_multiple_packets_in_one_read() {
        let mut bytes = encode_packet(&[1]).unwrap();
        bytes.extend(encode_packet(&[2, 2]).unwrap());
        bytes.extend(&encode_packet(&[3, 3, 3]).unwrap()[..4]); // partial third
        let mut d = FrameDecoder::new();
        d.push(&bytes);
        assert_eq!(
            drain(&mut d),
            vec![Decoded::Packet(vec![1]), Decoded::Packet(vec![2, 2])]
        );
        d.push(&encode_packet(&[3, 3, 3]).unwrap()[4..]);
        assert_eq!(drain(&mut d), vec![Decoded::Packet(vec![3, 3, 3])]);
    }

    #[test]
    fn payload_may_contain_flag_bytes() {
        let payload = vec![0xFE, 0xFD, 0x01, 0xFE];
        let mut d = FrameDecoder::new();
        d.push(&encode_packet(&payload).unwrap());
        assert_eq!(drain(&mut d), vec![Decoded::Packet(payload)]);
    }

    #[test]
    fn skips_leading_garbage() {
        let mut d = FrameDecoder::new();
        d.push(&[0x00, 0x13, 0x37]);
        d.push(&encode_packet(&[9]).unwrap());
        assert_eq!(
            drain(&mut d),
            vec![
                Decoded::Issue(FrameIssue::Garbage { discarded: 3 }),
                Decoded::Packet(vec![9])
            ]
        );
    }

    #[test]
    fn garbage_without_start_flag_is_discarded() {
        let mut d = FrameDecoder::new();
        d.push(&[1, 2, 3, 4]);
        assert_eq!(
            drain(&mut d),
            vec![Decoded::Issue(FrameIssue::Garbage { discarded: 4 })]
        );
        assert_eq!(d.pending(), 0);
    }

    #[test]
    fn unsupported_version_resyncs() {
        let mut d = FrameDecoder::new();
        d.push(&[0xFE, 0x02, 0x01, 0xAA, 0xFD]);
        d.push(&encode_packet(&[7]).unwrap());
        let out = drain(&mut d);
        assert_eq!(
            out[0],
            Decoded::Issue(FrameIssue::UnsupportedVersion { version: 2 })
        );
        assert_eq!(out.last(), Some(&Decoded::Packet(vec![7])));
    }

    #[test]
    fn bad_end_flag_does_not_swallow_next_packet() {
        // Corrupt length (claims 2 bytes but only 1 follows before a real packet).
        let mut bytes = vec![0xFE, 0x01, 0x02, 0xAA, 0xFD];
        bytes.extend(encode_packet(&[5]).unwrap());
        let mut d = FrameDecoder::new();
        d.push(&bytes);
        let out = drain(&mut d);
        assert!(matches!(
            out[0],
            Decoded::Issue(FrameIssue::BadEndFlag { .. })
        ));
        assert_eq!(out.last(), Some(&Decoded::Packet(vec![5])));
    }

    #[test]
    fn buffer_stays_bounded_against_endless_garbage() {
        let mut d = FrameDecoder::new();
        for _ in 0..1000 {
            d.push(&[0x55; 512]);
            while d.next().is_some() {}
            assert!(d.pending() <= MAX_PACKET_LEN);
        }
    }

    #[test]
    fn reset_drops_partial_packet() {
        let mut d = FrameDecoder::new();
        d.push(&[0xFE, 0x01, 0x05, 1, 2]);
        assert!(d.next().is_none());
        d.reset();
        d.push(&encode_packet(&[4]).unwrap());
        assert_eq!(drain(&mut d), vec![Decoded::Packet(vec![4])]);
    }
}
