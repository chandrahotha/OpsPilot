//! Unified log stream for real-time log aggregation across all services.
//!
//! Provides a broadcast-based streaming layer on top of the per-service
//! ring buffers, enabling real-time log tailing in the GUI.

use crate::log_buffer::{LogBuffer, LogEntry, LogStream};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// Unified log stream that aggregates logs from all services and supports
/// real-time streaming via broadcast channels.
#[derive(Clone)]
pub struct UnifiedLogStream {
    /// Broadcast sender for real-time log streaming
    sender: broadcast::Sender<LogEntry>,
    /// Per-service ring buffers for historical log access (shared via Arc<Mutex>)
    buffers: Arc<RwLock<HashMap<String, Arc<tokio::sync::Mutex<LogBuffer>>>>>,
    /// Default capacity for new service buffers
    default_capacity: usize,
}

impl UnifiedLogStream {
    /// Create a new unified log stream with the given default buffer capacity
    pub fn new(default_capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(1024);
        UnifiedLogStream {
            sender,
            buffers: Arc::new(RwLock::new(HashMap::new())),
            default_capacity,
        }
    }

    /// Get or create a buffer for a service
    async fn get_or_create_buffer(&self, service: &str) -> Arc<tokio::sync::Mutex<LogBuffer>> {
        let mut buffers = self.buffers.write().await;
        buffers
            .entry(service.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(LogBuffer::new(self.default_capacity))))
            .clone()
    }

/// Push a log entry to the stream (broadcasts to all subscribers and stores in buffer)
    pub async fn push(&self, entry: LogEntry) {
        let service = entry.service.clone();
        let buffer = self.get_or_create_buffer(&service).await;
        buffer.lock().await.push(entry.clone());
        let _ = self.sender.send(entry);
    }

    /// Push a system event to the stream
    pub async fn push_system(&self, service: &str, message: impl Into<String>) {
        self.push(LogEntry::system(service, message)).await;
    }

    /// Push a process output line to the stream
    pub async fn push_output(&self, service: &str, stream: LogStream, message: impl Into<String>) {
        self.push(LogEntry::output(service, stream, message)).await;
    }

    /// Subscribe to the unified log stream for real-time updates
    pub fn subscribe(&self) -> broadcast::Receiver<LogEntry> {
        self.sender.subscribe()
    }

    /// Get the current number of subscribers
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }

    /// Get a snapshot of recent logs for a specific service
    pub async fn snapshot(&self, service: &str, limit: Option<usize>) -> Vec<LogEntry> {
        let buffers = self.buffers.read().await;
        if let Some(buffer) = buffers.get(service) {
            buffer.lock().await.snapshot(limit)
        } else {
            Vec::new()
        }
    }

    /// Get a snapshot of recent logs for all services
    pub async fn snapshot_all(&self, limit: Option<usize>) -> Vec<LogEntry> {
        let buffers = self.buffers.read().await;
        let mut all_entries = Vec::new();
        for buffer in buffers.values() {
            all_entries.extend(buffer.lock().await.snapshot(limit));
        }
        // Sort by timestamp
        all_entries.sort_by_key(|e| e.timestamp_ms);
        all_entries
    }

    /// Clear logs for a specific service
    pub async fn clear_service(&self, service: &str) {
        let mut buffers = self.buffers.write().await;
        if let Some(buffer) = buffers.get(service) {
            buffer.lock().await.clear();
        }
    }

    /// Clear all logs
    pub async fn clear_all(&self) {
        let mut buffers = self.buffers.write().await;
        for buffer in buffers.values() {
            buffer.lock().await.clear();
        }
    }
}

impl Default for UnifiedLogStream {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log_buffer::{LogEntry, LogStream};

    #[tokio::test]
    async fn unified_stream_broadcasts_entries() {
        let stream = UnifiedLogStream::new(100);
        let mut rx = stream.subscribe();

        stream.push_output("frontend", LogStream::Stdout, "hello").await;
        stream.push_output("backend", LogStream::Stderr, "world").await;

        let first = rx.recv().await.expect("should receive first entry");
        assert_eq!(first.service, "frontend");
        assert_eq!(first.message, "hello");

        let second = rx.recv().await.expect("should receive second entry");
        assert_eq!(second.service, "backend");
        assert_eq!(second.message, "world");
    }

    #[tokio::test]
    async fn multiple_subscribers_receive_same_entries() {
        let stream = UnifiedLogStream::new(100);
        let mut rx1 = stream.subscribe();
        let mut rx2 = stream.subscribe();

        stream.push_output("test", LogStream::Stdout, "broadcast").await;

        let entry1 = rx1.recv().await.expect("rx1 should receive");
        let entry2 = rx2.recv().await.expect("rx2 should receive");

        assert_eq!(entry1.message, "broadcast");
        assert_eq!(entry2.message, "broadcast");
    }

    #[tokio::test]
    async fn system_events_are_broadcast() {
        let stream = UnifiedLogStream::new(100);
        let mut rx = stream.subscribe();

        stream.push_system("frontend", "started by Pilot").await;

        let entry = rx.recv().await.expect("should receive system entry");
        assert_eq!(entry.stream, LogStream::System);
        assert_eq!(entry.message, "started by Pilot");
    }

    #[tokio::test]
    async fn snapshot_returns_recent_logs() {
        let stream = UnifiedLogStream::new(100);

        stream.push_output("frontend", LogStream::Stdout, "first").await;
        stream.push_output("frontend", LogStream::Stdout, "second").await;
        stream.push_output("backend", LogStream::Stdout, "third").await;

        let frontend_logs = stream.snapshot("frontend", None).await;
        assert_eq!(frontend_logs.len(), 2);
        assert_eq!(frontend_logs[0].message, "first");
        assert_eq!(frontend_logs[1].message, "second");

        let all_logs = stream.snapshot_all(None).await;
        assert_eq!(all_logs.len(), 3);
    }

    #[tokio::test]
    async fn snapshot_with_limit_keeps_newest() {
        let stream = UnifiedLogStream::new(100);

        for i in 0..10 {
            stream.push_output("test", LogStream::Stdout, i.to_string()).await;
        }

        let limited = stream.snapshot("test", Some(3)).await;
        assert_eq!(limited.len(), 3);
        assert_eq!(limited[0].message, "7");
        assert_eq!(limited[2].message, "9");
    }

    #[tokio::test]
    async fn clear_service_removes_logs() {
        let stream = UnifiedLogStream::new(100);

        stream.push_output("frontend", LogStream::Stdout, "keep").await;
        stream.push_output("backend", LogStream::Stdout, "remove").await;

        stream.clear_service("backend").await;

        let frontend_logs = stream.snapshot("frontend", None).await;
        let backend_logs = stream.snapshot("backend", None).await;

        assert_eq!(frontend_logs.len(), 1);
        assert_eq!(backend_logs.len(), 0);
    }
}