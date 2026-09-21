//! Bounded, thread-safe log capture per service.
//!
//! Every process Pilot starts writes into its own buffer, so the GUI can show
//! frontend, backend and database output separately and together (spec section 16)
//! without unbounded memory growth.

use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Which stream a log line came from
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LogStream {
    /// The process wrote to stdout
    Stdout,
    /// The process wrote to stderr
    Stderr,
    /// Pilot itself recorded an event (started, stopped, failed)
    System,
}

/// One captured log line
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    /// Wall-clock time in milliseconds since the Unix epoch
    pub timestamp_ms: u64,
    /// Service label the line belongs to (frontend, backend, ...)
    pub service: String,
    /// Stream the line came from
    pub stream: LogStream,
    /// The line itself, without the trailing newline
    pub message: String,
}

impl LogEntry {
    /// Create a Pilot-generated system entry with the current timestamp
    pub fn system(service: &str, message: impl Into<String>) -> Self {
        LogEntry {
            timestamp_ms: now_ms(),
            service: service.to_string(),
            stream: LogStream::System,
            message: message.into(),
        }
    }

    /// Create a process output entry with the current timestamp
    pub fn output(service: &str, stream: LogStream, message: impl Into<String>) -> Self {
        LogEntry {
            timestamp_ms: now_ms(),
            service: service.to_string(),
            stream,
            message: message.into(),
        }
    }
}

/// Current wall-clock time in milliseconds since the Unix epoch
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or_default()
}

/// Ring buffer that keeps the newest `capacity` lines per service
pub struct LogBuffer {
    capacity: usize,
    entries: Mutex<VecDeque<LogEntry>>,
}

impl LogBuffer {
    /// Create a buffer that keeps at most `capacity` lines
    pub fn new(capacity: usize) -> Self {
        LogBuffer {
            capacity: capacity.max(1),
            entries: Mutex::new(VecDeque::new()),
        }
    }

    /// Append an entry, dropping the oldest one when the buffer is full
    pub fn push(&self, entry: LogEntry) {
        let mut entries = self.lock();

        if entries.len() == self.capacity {
            entries.pop_front();
        }

        entries.push_back(entry);
    }

    /// Append a Pilot-generated system entry
    pub fn push_system(&self, service: &str, message: impl Into<String>) {
        self.push(LogEntry::system(service, message));
    }

    /// Snapshot of the buffered lines, oldest first.
    ///
    /// `limit` keeps only the newest `limit` lines.
    pub fn snapshot(&self, limit: Option<usize>) -> Vec<LogEntry> {
        let entries = self.lock();
        let start = limit
            .map(|limit| entries.len().saturating_sub(limit))
            .unwrap_or_default();

        entries.iter().skip(start).cloned().collect()
    }

    /// Drop every buffered line
    pub fn clear(&self) {
        self.lock().clear();
    }

    /// Number of buffered lines
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Whether nothing is buffered
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<LogEntry>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_returned_oldest_first() {
        let buffer = LogBuffer::new(4);

        buffer.push(LogEntry::output("frontend", LogStream::Stdout, "first"));
        buffer.push(LogEntry::output("frontend", LogStream::Stderr, "second"));

        let lines = buffer.snapshot(None);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].message, "first");
        assert_eq!(lines[1].message, "second");
        assert_eq!(lines[1].stream, LogStream::Stderr);
        assert_eq!(lines[0].service, "frontend");
    }

    #[test]
    fn capacity_drops_the_oldest_lines() {
        let buffer = LogBuffer::new(2);

        for index in 0..5 {
            buffer.push(LogEntry::output(
                "backend",
                LogStream::Stdout,
                index.to_string(),
            ));
        }

        let messages: Vec<String> = buffer
            .snapshot(None)
            .into_iter()
            .map(|entry| entry.message)
            .collect();

        assert_eq!(messages, vec!["3", "4"]);
        assert_eq!(buffer.len(), 2);
    }

    #[test]
    fn a_limit_keeps_the_newest_lines() {
        let buffer = LogBuffer::new(8);

        for index in 0..6 {
            buffer.push(LogEntry::output("db", LogStream::Stdout, index.to_string()));
        }

        let messages: Vec<String> = buffer
            .snapshot(Some(2))
            .into_iter()
            .map(|entry| entry.message)
            .collect();

        assert_eq!(messages, vec!["4", "5"]);
    }

    #[test]
    fn system_entries_record_pilot_events() {
        let buffer = LogBuffer::new(4);

        buffer.push_system("frontend", "started by Pilot");

        let entry = &buffer.snapshot(None)[0];

        assert_eq!(entry.stream, LogStream::System);
        assert_eq!(entry.message, "started by Pilot");
        assert!(entry.timestamp_ms > 0);
    }

    #[test]
    fn clear_empties_the_buffer() {
        let buffer = LogBuffer::new(4);

        buffer.push_system("frontend", "started");
        buffer.clear();

        assert!(buffer.is_empty());
    }

    #[test]
    fn entries_serialize_with_camel_case_keys() {
        let entry = LogEntry::output("frontend", LogStream::Stdout, "ready");
        let json = serde_json::to_value(&entry).expect("entry must serialize");

        assert_eq!(json["service"], "frontend");
        assert_eq!(json["stream"], "stdout");
        assert_eq!(json["message"], "ready");
        assert!(json["timestampMs"].is_u64());
    }
}
