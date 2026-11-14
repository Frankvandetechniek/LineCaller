//! AVT **Remote Light Protocol** v1 codec for LineCaller.
//!
//! This crate is pure: no sockets, no async, no Tauri. It turns commands into
//! bytes and bytes into line status, so it can be tested exhaustively.
//!
//! * [`frame`] handles the `FE 01 <len> <payload> FD` envelope and stream
//!   re-assembly across arbitrary TCP read boundaries.
//! * [`message`] handles the payload layouts of spec §4.
//!
//! Line numbers are zero-based on the wire (`0` = Line 1). The [`LineIndex`]
//! type keeps that explicit; convert with [`LineIndex::display_number`] only
//! when showing a number to a person.

pub mod frame;
pub mod message;

pub use frame::{encode_packet, Decoded, FrameDecoder, FrameIssue};
pub use message::{
    Audio, Command, Incoming, LineIndex, LineState, LineStatus, Location, ParseError, Screening,
    ValidationError, MAX_LINES, THIPPRO_MAX_LOCATIONS,
};
