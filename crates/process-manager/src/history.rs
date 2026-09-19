//! Operation history (spec section 19): what Pilot executed, when, and how it ended.

use crate::log_buffer::now_ms;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard};

/// One entry of the operation history
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    /// Wall-clock time in milliseconds since the Unix epoch
    pub timestamp_ms: u64,
    /// Operation name (start, stop, restart)
    pub operation: String,
    /// What exactly was done, including the command for start operations
    pub detail: String,
}

/// Bounded history of the operations Pilot executed
#[derive(Debug)]
pub struct OperationHistory {
    capacity: usize,
    entries: Mutex<VecDeque<HistoryEntry>>,
}

impl OperationHistory {
    /// Create a history that keeps at most `capacity` entries
    pub fn new(capacity: usize) -> Self {
        OperationHistory {
            capacity: capacity.max(1),
            entries: Mutex::new(VecDeque::new()),
        }
    }

    /// Record an operation, dropping the oldest entry when full
    pub fn record(&self, operation: &str, detail: impl Into<String>) {
        let mut entries = self.lock();

        if entries.len() == self.capacity {
            entries.pop_front();
        }

        entries.push_back(HistoryEntry {
            timestamp_ms: now_ms(),
            operation: operation.to_string(),
            detail: detail.into(),
        });
    }

    /// Snapshot ordered newest first, limited to `limit` entries
    pub fn snapshot(&self, limit: Option<usize>) -> Vec<HistoryEntry> {
        let entries = self.lock();
        let start = limit
            .map(|limit| entries.len().saturating_sub(limit))
            .unwrap_or_default();

        entries.iter().skip(start).rev().cloned().collect()
    }

    fn lock(&self) -> MutexGuard<'_, VecDeque<HistoryEntry>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Default for OperationHistory {
    /// Keeps the last 100 operations
    fn default() -> Self {
        OperationHistory::new(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_newest_entries_and_returns_them_newest_first() {
        let history = OperationHistory::new(2);

        history.record("start", "frontend: npm run dev");
        history.record("stop", "frontend");
        history.record("start", "backend: python manage.py runserver");

        let entries = history.snapshot(None);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].operation, "start");
        assert!(entries[0].detail.contains("backend"));
        assert_eq!(entries[1].operation, "stop");
        assert!(entries[0].timestamp_ms >= entries[1].timestamp_ms);
    }

    #[test]
    fn can_be_limited() {
        let history = OperationHistory::default();

        for index in 0..4 {
            history.record("start", index.to_string());
        }

        let entries = history.snapshot(Some(2));

        assert_eq!(entries.len(), 2);
        assert!(entries[0].detail.contains("3"));
        assert!(entries[1].detail.contains("2"));
    }

    #[test]
    fn entries_serialize_with_camel_case_keys() {
        let history = OperationHistory::new(4);

        history.record("stop", "frontend");

        let json = serde_json::to_value(history.snapshot(None)).expect("history must serialize");

        assert_eq!(json[0]["operation"], "stop");
        assert_eq!(json[0]["detail"], "frontend");
        assert!(json[0]["timestampMs"].is_u64());
    }
}