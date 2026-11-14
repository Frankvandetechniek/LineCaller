//! Latest known status per line, plus timing metadata for the UI.

use rlp::{LineStatus, MAX_LINES};
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// What the frontend receives for one line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineView {
    pub status: LineStatus,
    /// When LineCaller received the latest status for this line (Unix ms).
    pub updated_at_ms: u64,
    /// When the connection state last changed, as observed by LineCaller.
    /// Used for the call timer. This is the time the status *arrived*, not a
    /// timestamp from the THipPro (the protocol carries none).
    pub state_since_ms: u64,
    /// True while the link to the THipPro is down: the status is the last
    /// one seen and may be out of date.
    pub stale: bool,
}

#[derive(Debug, Default)]
pub struct LineStore {
    lines: Vec<Option<LineView>>,
}

impl LineStore {
    pub fn new() -> Self {
        Self {
            lines: vec![None; MAX_LINES as usize],
        }
    }

    /// Apply a status update. Returns the view if anything visible changed.
    pub fn apply(&mut self, status: LineStatus) -> Option<LineView> {
        self.apply_at(status, now_ms())
    }

    pub fn apply_at(&mut self, status: LineStatus, now: u64) -> Option<LineView> {
        let slot = &mut self.lines[status.line.0 as usize];
        let state_since_ms = match slot {
            Some(prev) if prev.status.state == status.state => prev.state_since_ms,
            _ => now,
        };
        let changed = slot
            .as_ref()
            .map(|p| p.status != status || p.stale)
            .unwrap_or(true);
        let view = LineView {
            status,
            updated_at_ms: now,
            state_since_ms,
            stale: false,
        };
        *slot = Some(view.clone());
        changed.then_some(view)
    }

    /// Mark everything as possibly out of date (link lost).
    pub fn mark_all_stale(&mut self) -> Vec<LineView> {
        self.lines
            .iter_mut()
            .flatten()
            .filter(|v| !v.stale)
            .map(|v| {
                v.stale = true;
                v.clone()
            })
            .collect()
    }

    pub fn clear(&mut self) {
        self.lines = vec![None; MAX_LINES as usize];
    }

    pub fn snapshot(&self) -> Vec<Option<LineView>> {
        self.lines.clone()
    }

    pub fn get(&self, index: u8) -> Option<&LineView> {
        self.lines.get(index as usize).and_then(|l| l.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rlp::{Audio, LineIndex, LineState, Screening};

    fn st(state: LineState, audio: Audio) -> LineStatus {
        LineStatus {
            line: LineIndex(2),
            state,
            audio,
            location: 1,
            audio_locked: false,
            line_locked: false,
            screening: Screening::default(),
        }
    }

    #[test]
    fn timer_starts_on_state_change_only() {
        let mut s = LineStore::new();
        let v = s
            .apply_at(st(LineState::Connected, Audio::Pretalk), 1000)
            .unwrap();
        assert_eq!(v.state_since_ms, 1000);
        // audio change keeps the call timer running
        let v = s
            .apply_at(st(LineState::Connected, Audio::OnAir1), 5000)
            .unwrap();
        assert_eq!(v.state_since_ms, 1000);
        // identical status is not re-emitted
        assert!(s
            .apply_at(st(LineState::Connected, Audio::OnAir1), 6000)
            .is_none());
        let v = s
            .apply_at(st(LineState::Disconnected, Audio::Hold), 9000)
            .unwrap();
        assert_eq!(v.state_since_ms, 9000);
    }

    #[test]
    fn stale_marks_and_clears() {
        let mut s = LineStore::new();
        s.apply_at(st(LineState::Connected, Audio::Hold), 1);
        assert_eq!(s.mark_all_stale().len(), 1);
        assert!(s.mark_all_stale().is_empty());
        // the same status after reconnect is re-emitted (no longer stale) and keeps its timer
        let v = s
            .apply_at(st(LineState::Connected, Audio::Hold), 50)
            .unwrap();
        assert!(!v.stale);
        assert_eq!(v.state_since_ms, 1);
    }
}
