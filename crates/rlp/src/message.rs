//! Payload layouts for commands and status messages (spec §4).
//!
//! Every byte layout here was checked against "Remote Light Protocol for
//! MAGIC THipPro / TH6 / TH2plus", version 1.0, dated 10.10.17.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Command codes (spec §4.2) and the status code (§4.3).
pub mod code {
    pub const DIAL: u8 = 0x01;
    pub const DROP: u8 = 0x02;
    pub const SET_AUDIO: u8 = 0x03;
    pub const SET_LOCK_AUDIO: u8 = 0x04;
    pub const SET_LOCK_LINE: u8 = 0x05;
    pub const STATUS_REQUEST: u8 = 0x06;
    pub const SET_CONFERENCE: u8 = 0x07;
    pub const SET_NEXT_CALLER: u8 = 0x08;
    pub const LINE_STATUS: u8 = 0x40;
}

/// Maximum number of lines LineCaller supports. Protocol line bytes are
/// zero-based, so valid indices are `0..MAX_LINES`.
pub const MAX_LINES: u8 = 16;

/// THipPro accepts up to 6 parallel third-party connections (spec §2), so
/// valid locations for this app are 1..=6. Location 0 means "none".
pub const THIPPRO_MAX_LOCATIONS: u8 = 6;

/// Zero-based line number exactly as it appears on the wire.
/// `LineIndex(0)` is "Line 1" in the user interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LineIndex(pub u8);

impl LineIndex {
    pub fn new(index: u8) -> Result<Self, ValidationError> {
        if index < MAX_LINES {
            Ok(Self(index))
        } else {
            Err(ValidationError::LineOutOfRange(index))
        }
    }
    /// One-based number shown to people.
    pub fn display_number(self) -> u8 {
        self.0 + 1
    }
}

/// A third-party location (console) number, 1..=6 for a THipPro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Location(pub u8);

impl Location {
    pub fn new(n: u8) -> Result<Self, ValidationError> {
        if (1..=THIPPRO_MAX_LOCATIONS).contains(&n) {
            Ok(Self(n))
        } else {
            Err(ValidationError::LocationOutOfRange(n))
        }
    }
}

/// Audio interface. `Unknown` (4) only ever appears in Line Status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Audio {
    Hold,
    Pretalk,
    /// "ON AIR 1" in the spec, "ON-AIR A" in the LineCaller UI.
    OnAir1,
    /// "ON AIR 2" in the spec (THipPro only), "ON-AIR B" in the UI.
    OnAir2,
    Unknown,
}

impl Audio {
    pub fn to_wire(self) -> Result<u8, ValidationError> {
        match self {
            Audio::Hold => Ok(0),
            Audio::Pretalk => Ok(1),
            Audio::OnAir1 => Ok(2),
            Audio::OnAir2 => Ok(3),
            Audio::Unknown => Err(ValidationError::AudioNotSendable),
        }
    }
    /// Decode a status byte. Values outside 0..=4 are reported as `Unknown`
    /// as well, since the spec defines no other meaning for them.
    pub fn from_wire(b: u8) -> Self {
        match b {
            0 => Audio::Hold,
            1 => Audio::Pretalk,
            2 => Audio::OnAir1,
            3 => Audio::OnAir2,
            _ => Audio::Unknown,
        }
    }
}

/// Connection state of a line (spec §4.3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", content = "value")]
pub enum LineState {
    Disconnected,
    Reserved,
    /// Incoming call.
    Ringing,
    /// Outgoing call.
    Calling,
    Connected,
    /// A value the spec does not define; kept so the UI can show it.
    Undefined(u8),
}

impl LineState {
    pub fn from_wire(b: u8) -> Self {
        match b {
            0 => LineState::Disconnected,
            1 => LineState::Reserved,
            2 => LineState::Ringing,
            3 => LineState::Calling,
            4 => LineState::Connected,
            other => LineState::Undefined(other),
        }
    }
    pub fn to_wire(self) -> u8 {
        match self {
            LineState::Disconnected => 0,
            LineState::Reserved => 1,
            LineState::Ringing => 2,
            LineState::Calling => 3,
            LineState::Connected => 4,
            LineState::Undefined(b) => b,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("line index {0} is outside 0..=15")]
    LineOutOfRange(u8),
    #[error("location {0} is outside 1..=6 (MAGIC THipPro allows six third-party locations)")]
    LocationOutOfRange(u8),
    #[error("UNKNOWN is a status value and cannot be sent")]
    AudioNotSendable,
    #[error("HOLD is not allowed for Set Next Caller")]
    NextCallerHold,
    #[error("phone number contains a character that is not allowed: {0:?}")]
    BadPhoneChar(char),
    #[error("phone number is longer than {max} characters")]
    PhoneTooLong { max: usize },
}

/// Longest phone number LineCaller will send. The packet format would allow
/// up to 250 bytes; 64 is an application limit, far above real numbers.
pub const MAX_PHONE_LEN: usize = 64;

/// Characters LineCaller allows in a dialled number. The spec only says
/// "ASCII string"; restricting to dial-pad characters (plus `+` for
/// international format) is an application choice.
pub fn validate_phone_number(number: &str) -> Result<(), ValidationError> {
    if number.len() > MAX_PHONE_LEN {
        return Err(ValidationError::PhoneTooLong { max: MAX_PHONE_LEN });
    }
    if let Some(c) = number
        .chars()
        .find(|c| !(c.is_ascii_digit() || matches!(c, '*' | '#' | '+')))
    {
        return Err(ValidationError::BadPhoneChar(c));
    }
    Ok(())
}

/// Commands from the third-party system to the hybrid (spec §4.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// §4.2.1. If a call is pending (ringing) on the line, the number is
    /// ignored by the hybrid and the call is accepted.
    Dial {
        line: LineIndex,
        audio: Audio,
        location: Location,
        number: String,
    },
    /// §4.2.2
    Drop { line: LineIndex, location: Location },
    /// §4.2.3
    SetAudio {
        line: LineIndex,
        audio: Audio,
        location: Location,
    },
    /// §4.2.4 Lock the line to its current audio (the LOCK button).
    SetLockAudio {
        line: LineIndex,
        locked: bool,
        location: Location,
    },
    /// §4.2.5 Deny incoming calls on the line with busy.
    SetLockLine {
        line: LineIndex,
        locked: bool,
        location: Location,
    },
    /// §4.2.6
    StatusRequest { line: LineIndex },
    /// §4.2.7 Encoded for completeness; not exposed in the LineCaller UI.
    SetConference { location: Location, on: bool },
    /// §4.2.8 Encoded for completeness; not exposed in the LineCaller UI.
    SetNextCaller { audio: Audio, location: Location },
}

impl Command {
    /// Build the payload bytes (without framing).
    pub fn to_payload(&self) -> Result<Vec<u8>, ValidationError> {
        Ok(match self {
            Command::Dial {
                line,
                audio,
                location,
                number,
            } => {
                validate_phone_number(number)?;
                let mut p = vec![code::DIAL, line.0, audio.to_wire()?, location.0];
                p.extend_from_slice(number.as_bytes());
                p.push(0); // zero terminated
                p
            }
            Command::Drop { line, location } => vec![code::DROP, line.0, location.0],
            Command::SetAudio {
                line,
                audio,
                location,
            } => {
                vec![code::SET_AUDIO, line.0, audio.to_wire()?, location.0]
            }
            // Note the field order differs from Set Audio: Line, Lock, Location.
            Command::SetLockAudio {
                line,
                locked,
                location,
            } => {
                vec![code::SET_LOCK_AUDIO, line.0, *locked as u8, location.0]
            }
            Command::SetLockLine {
                line,
                locked,
                location,
            } => {
                vec![code::SET_LOCK_LINE, line.0, *locked as u8, location.0]
            }
            Command::StatusRequest { line } => vec![code::STATUS_REQUEST, line.0],
            Command::SetConference { location, on } => {
                vec![code::SET_CONFERENCE, location.0, *on as u8]
            }
            Command::SetNextCaller { audio, location } => {
                if *audio == Audio::Hold {
                    return Err(ValidationError::NextCallerHold);
                }
                vec![code::SET_NEXT_CALLER, audio.to_wire()?, location.0]
            }
        })
    }

    /// Build the complete framed packet.
    pub fn to_packet(&self) -> Result<Vec<u8>, ValidationError> {
        let payload = self.to_payload()?;
        // All command payloads are far below 255 bytes (phone numbers are capped).
        Ok(crate::frame::encode_packet(&payload).expect("command payload within frame limits"))
    }

    /// Parse a command payload. Used by the simulator; the app never receives commands.
    pub fn from_payload(p: &[u8]) -> Result<Self, ParseError> {
        let need = |n: usize| {
            if p.len() < n {
                Err(ParseError::TooShort {
                    code: p[0],
                    len: p.len(),
                })
            } else {
                Ok(())
            }
        };
        let first = *p.first().ok_or(ParseError::Empty)?;
        let line = |b: u8| LineIndex::new(b).map_err(ParseError::Invalid);
        let loc = |b: u8| Location::new(b).map_err(ParseError::Invalid);
        let audio = |b: u8| {
            if b <= 3 {
                Ok(Audio::from_wire(b))
            } else {
                Err(ParseError::BadValue("audio", b))
            }
        };
        let flag = |b: u8| match b {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ParseError::BadValue("lock", b)),
        };
        Ok(match first {
            code::DIAL => {
                need(5)?;
                let (number, _) = read_cstr(&p[4..]);
                Command::Dial {
                    line: line(p[1])?,
                    audio: audio(p[2])?,
                    location: loc(p[3])?,
                    number,
                }
            }
            code::DROP => {
                need(3)?;
                Command::Drop {
                    line: line(p[1])?,
                    location: loc(p[2])?,
                }
            }
            code::SET_AUDIO => {
                need(4)?;
                Command::SetAudio {
                    line: line(p[1])?,
                    audio: audio(p[2])?,
                    location: loc(p[3])?,
                }
            }
            code::SET_LOCK_AUDIO => {
                need(4)?;
                Command::SetLockAudio {
                    line: line(p[1])?,
                    locked: flag(p[2])?,
                    location: loc(p[3])?,
                }
            }
            code::SET_LOCK_LINE => {
                need(4)?;
                Command::SetLockLine {
                    line: line(p[1])?,
                    locked: flag(p[2])?,
                    location: loc(p[3])?,
                }
            }
            code::STATUS_REQUEST => {
                need(2)?;
                Command::StatusRequest { line: line(p[1])? }
            }
            code::SET_CONFERENCE => {
                need(3)?;
                Command::SetConference {
                    location: loc(p[1])?,
                    on: flag(p[2])?,
                }
            }
            code::SET_NEXT_CALLER => {
                need(3)?;
                Command::SetNextCaller {
                    audio: audio(p[1])?,
                    location: loc(p[2])?,
                }
            }
            other => return Err(ParseError::UnknownCode(other)),
        })
    }
}

/// Screening information carried by Line Status. Empty strings mean
/// "not available" (the spec sends a zero-length string in that case).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Screening {
    pub phone_number: String,
    pub name: String,
    pub first_name: String,
    pub city: String,
}

/// Line Status message (spec §4.3.1, code 0x40).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineStatus {
    pub line: LineIndex,
    pub state: LineState,
    pub audio: Audio,
    /// 0 = not linked to any location.
    pub location: u8,
    pub audio_locked: bool,
    pub line_locked: bool,
    pub screening: Screening,
}

impl LineStatus {
    pub fn to_payload(&self) -> Vec<u8> {
        let mut p = vec![
            code::LINE_STATUS,
            self.line.0,
            self.state.to_wire(),
            match self.audio {
                Audio::Unknown => 4,
                a => a.to_wire().unwrap(),
            },
            self.location,
            self.audio_locked as u8,
            self.line_locked as u8,
        ];
        for s in [
            &self.screening.phone_number,
            &self.screening.name,
            &self.screening.first_name,
            &self.screening.city,
        ] {
            p.extend(s.bytes().filter(|b| *b != 0 && b.is_ascii()));
            p.push(0);
        }
        p
    }

    /// Parse a Line Status payload.
    ///
    /// Leniency (documented assumptions, the spec does not cover these):
    /// * Missing trailing screening strings are treated as empty, and a final
    ///   string without its zero terminator is accepted up to the end of the
    ///   payload. The seven fixed bytes are always required.
    /// * Non-ASCII bytes are mapped as Latin-1 rather than rejected, so a
    ///   caller name with an accent never discards the whole status.
    /// * A line index of 16 or more is rejected (LineCaller supports 16 lines).
    pub fn from_payload(p: &[u8]) -> Result<Self, ParseError> {
        let first = *p.first().ok_or(ParseError::Empty)?;
        if first != code::LINE_STATUS {
            return Err(ParseError::UnknownCode(first));
        }
        if p.len() < 7 {
            return Err(ParseError::TooShort {
                code: first,
                len: p.len(),
            });
        }
        let line = LineIndex::new(p[1]).map_err(ParseError::Invalid)?;
        let mut rest = &p[7..];
        let mut next = || {
            let (s, used) = read_cstr(rest);
            rest = &rest[used..];
            s
        };
        let screening = Screening {
            phone_number: next(),
            name: next(),
            first_name: next(),
            city: next(),
        };
        Ok(LineStatus {
            line,
            state: LineState::from_wire(p[2]),
            audio: Audio::from_wire(p[3]),
            location: p[4],
            audio_locked: p[5] != 0,
            line_locked: p[6] != 0,
            screening,
        })
    }

    pub fn is_occupied_by_other(&self, me: Location) -> bool {
        self.location != 0 && self.location != me.0
    }
}

/// Read a zero-terminated string; returns (string, bytes consumed).
fn read_cstr(b: &[u8]) -> (String, usize) {
    match b.iter().position(|&c| c == 0) {
        Some(end) => (latin1(&b[..end]), end + 1),
        None => (latin1(b), b.len()),
    }
}

fn latin1(b: &[u8]) -> String {
    b.iter()
        .map(|&c| c as char)
        .collect::<String>()
        .trim()
        .to_string()
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("empty payload")]
    Empty,
    #[error("unknown message code 0x{0:02X}")]
    UnknownCode(u8),
    #[error("payload for code 0x{code:02X} is too short ({len} bytes)")]
    TooShort { code: u8, len: usize },
    #[error("invalid {0} value {1}")]
    BadValue(&'static str, u8),
    #[error(transparent)]
    Invalid(ValidationError),
}

/// Messages the hybrid can send to LineCaller. Only Line Status is defined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    LineStatus(LineStatus),
}

impl Incoming {
    pub fn from_payload(p: &[u8]) -> Result<Self, ParseError> {
        match p.first() {
            None => Err(ParseError::Empty),
            Some(&code::LINE_STATUS) => LineStatus::from_payload(p).map(Incoming::LineStatus),
            Some(&other) => Err(ParseError::UnknownCode(other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const L1: Location = Location(1);

    fn li(n: u8) -> LineIndex {
        LineIndex::new(n).unwrap()
    }

    #[test]
    fn dial_layout_matches_spec_4_2_1() {
        let c = Command::Dial {
            line: li(0),
            audio: Audio::Pretalk,
            location: Location(2),
            number: "0301234#".into(),
        };
        let mut expect = vec![0x01, 0x00, 0x01, 0x02];
        expect.extend(b"0301234#");
        expect.push(0);
        assert_eq!(c.to_payload().unwrap(), expect);
    }

    #[test]
    fn accept_is_dial_with_empty_number() {
        let c = Command::Dial {
            line: li(3),
            audio: Audio::OnAir2,
            location: L1,
            number: String::new(),
        };
        assert_eq!(c.to_payload().unwrap(), vec![0x01, 3, 3, 1, 0]);
    }

    #[test]
    fn command_layouts_match_spec() {
        assert_eq!(
            Command::Drop {
                line: li(15),
                location: L1
            }
            .to_payload()
            .unwrap(),
            vec![0x02, 15, 1]
        );
        assert_eq!(
            Command::SetAudio {
                line: li(1),
                audio: Audio::Hold,
                location: L1
            }
            .to_payload()
            .unwrap(),
            vec![0x03, 1, 0, 1]
        );
        assert_eq!(
            Command::SetAudio {
                line: li(1),
                audio: Audio::OnAir1,
                location: L1
            }
            .to_payload()
            .unwrap(),
            vec![0x03, 1, 2, 1]
        );
        assert_eq!(
            Command::SetLockAudio {
                line: li(2),
                locked: true,
                location: Location(6)
            }
            .to_payload()
            .unwrap(),
            vec![0x04, 2, 1, 6]
        );
        assert_eq!(
            Command::SetLockLine {
                line: li(2),
                locked: false,
                location: L1
            }
            .to_payload()
            .unwrap(),
            vec![0x05, 2, 0, 1]
        );
        assert_eq!(
            Command::StatusRequest { line: li(7) }.to_payload().unwrap(),
            vec![0x06, 7]
        );
        assert_eq!(
            Command::SetConference {
                location: L1,
                on: true
            }
            .to_payload()
            .unwrap(),
            vec![0x07, 1, 1]
        );
        assert_eq!(
            Command::SetNextCaller {
                audio: Audio::OnAir1,
                location: L1
            }
            .to_payload()
            .unwrap(),
            vec![0x08, 2, 1]
        );
    }

    #[test]
    fn full_packet_is_framed() {
        let pkt = Command::StatusRequest { line: li(0) }.to_packet().unwrap();
        assert_eq!(pkt, vec![0xFE, 0x01, 0x02, 0x06, 0x00, 0xFD]);
    }

    #[test]
    fn validation() {
        assert!(LineIndex::new(16).is_err());
        assert!(Location::new(0).is_err());
        assert!(Location::new(7).is_err());
        assert!(Location::new(6).is_ok());
        assert_eq!(
            validate_phone_number("12a"),
            Err(ValidationError::BadPhoneChar('a'))
        );
        assert!(validate_phone_number("+31*20#").is_ok());
        assert!(validate_phone_number(&"1".repeat(65)).is_err());
        assert!(Command::SetAudio {
            line: li(0),
            audio: Audio::Unknown,
            location: L1
        }
        .to_payload()
        .is_err());
        assert!(Command::SetNextCaller {
            audio: Audio::Hold,
            location: L1
        }
        .to_payload()
        .is_err());
    }

    #[test]
    fn parses_line_status() {
        let mut p = vec![0x40, 4, 4, 2, 1, 1, 0];
        p.extend(b"0612345678\0Jansen\0Piet\0Hilversum\0");
        let s = LineStatus::from_payload(&p).unwrap();
        assert_eq!(s.line, li(4));
        assert_eq!(s.line.display_number(), 5);
        assert_eq!(s.state, LineState::Connected);
        assert_eq!(s.audio, Audio::OnAir1);
        assert_eq!(s.location, 1);
        assert!(s.audio_locked);
        assert!(!s.line_locked);
        assert_eq!(s.screening.phone_number, "0612345678");
        assert_eq!(s.screening.name, "Jansen");
        assert_eq!(s.screening.first_name, "Piet");
        assert_eq!(s.screening.city, "Hilversum");
    }

    #[test]
    fn line_status_with_empty_screening() {
        let p = vec![0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let s = LineStatus::from_payload(&p).unwrap();
        assert_eq!(s.state, LineState::Disconnected);
        assert_eq!(s.screening, Screening::default());
    }

    #[test]
    fn line_status_tolerates_missing_strings_and_odd_values() {
        let s = LineStatus::from_payload(&[0x40, 1, 9, 7, 0, 0, 1, b'1', b'2']).unwrap();
        assert_eq!(s.state, LineState::Undefined(9));
        assert_eq!(s.audio, Audio::Unknown);
        assert!(s.line_locked);
        assert_eq!(s.screening.phone_number, "12");
        assert_eq!(s.screening.city, "");
    }

    #[test]
    fn line_status_rejections() {
        assert_eq!(
            LineStatus::from_payload(&[0x40, 1, 0]),
            Err(ParseError::TooShort { code: 0x40, len: 3 })
        );
        assert!(matches!(
            LineStatus::from_payload(&[0x40, 16, 0, 0, 0, 0, 0]),
            Err(ParseError::Invalid(_))
        ));
        assert_eq!(
            Incoming::from_payload(&[0x41, 0]),
            Err(ParseError::UnknownCode(0x41))
        );
    }

    #[test]
    fn line_status_round_trip() {
        let s = LineStatus {
            line: li(9),
            state: LineState::Ringing,
            audio: Audio::Hold,
            location: 0,
            audio_locked: false,
            line_locked: false,
            screening: Screening {
                phone_number: "+3135".into(),
                name: "De Vries".into(),
                first_name: "".into(),
                city: "Utrecht".into(),
            },
        };
        assert_eq!(LineStatus::from_payload(&s.to_payload()).unwrap(), s);
    }

    #[test]
    fn command_round_trip() {
        let cmds = [
            Command::Dial {
                line: li(2),
                audio: Audio::OnAir2,
                location: Location(3),
                number: "123*#".into(),
            },
            Command::Drop {
                line: li(2),
                location: Location(3),
            },
            Command::SetAudio {
                line: li(0),
                audio: Audio::Pretalk,
                location: L1,
            },
            Command::SetLockAudio {
                line: li(0),
                locked: true,
                location: L1,
            },
            Command::SetLockLine {
                line: li(0),
                locked: true,
                location: L1,
            },
            Command::StatusRequest { line: li(15) },
        ];
        for c in cmds {
            assert_eq!(Command::from_payload(&c.to_payload().unwrap()).unwrap(), c);
        }
    }
}
