//! The last 20 numbers dialled from this LineCaller installation.

use serde::{Deserialize, Serialize};

pub const MAX_RECENTS: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentCall {
    pub number: String,
    /// One-based line number the call was placed on (display only).
    pub line_number: u8,
    pub at_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recents {
    pub entries: Vec<RecentCall>,
}

impl Recents {
    /// Add a number to the top. A repeat of an existing number moves it up
    /// instead of creating a duplicate. Keeps at most 20 entries.
    pub fn record(&mut self, call: RecentCall) {
        self.entries.retain(|e| e.number != call.number);
        self.entries.insert(0, call);
        self.entries.truncate(MAX_RECENTS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_twenty_most_recent_unique() {
        let mut r = Recents::default();
        for i in 0..25 {
            r.record(RecentCall {
                number: format!("{i}"),
                line_number: 1,
                at_ms: i,
            });
        }
        assert_eq!(r.entries.len(), 20);
        assert_eq!(r.entries[0].number, "24");
        r.record(RecentCall {
            number: "10".into(),
            line_number: 2,
            at_ms: 99,
        });
        assert_eq!(r.entries.len(), 20);
        assert_eq!(r.entries[0].number, "10");
        assert_eq!(r.entries.iter().filter(|e| e.number == "10").count(), 1);
    }
}
