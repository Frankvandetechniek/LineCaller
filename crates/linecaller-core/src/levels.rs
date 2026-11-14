//! Per-line audio levels for the R (receive) and T (transmit) meters.
//!
//! The Remote Light Protocol carries no audio levels. A real THipPro only
//! reports them over its Command Protocol ("Audio Level information",
//! telegram 6CH/0EH, 20 Hz, 1 dB steps). Until that link exists, levels come
//! from the simulator only; with a real THipPro the meters show "no data".

use serde::Serialize;

/// Lowest level shown; also used for "no audio on this line".
pub const SILENCE_DB: i8 = -128;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineLevels {
    /// Receive level per line in dB (index 0 = Line 1), `SILENCE_DB` for none.
    pub rx: Vec<i8>,
    /// Transmit level per line in dB.
    pub tx: Vec<i8>,
    /// Where the levels come from: "simulator" (or "thippro" later).
    pub source: &'static str,
}
