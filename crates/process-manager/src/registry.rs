//! Process records and the snapshots the GUI shows.
//!
//! A [`ProcessRecord`] describes one process Pilot started; a [`ProcessSnapshot`]
//! is its serializable view. Pilot only ever tracks processes it started itself
//! (spec section 11).

use crate::ProcessRequest;
use crate::log_buffer::{LogBuffer, now_ms};
use serde::Serialize;
use std::process::{Child, ChildStderr, ChildStdout};
use std::sync::{Arc, Mutex, MutexGuard};

/// Lifecycle state of a process Pilot started
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProcessState {
    /// The process is alive
    Running,
    /// Pilot stopped the process
    Stopped,
    /// The process exited on its own
    Exited,
    /// Pilot could not start the process
    Failed,
}

/// Serializable view of a started process
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessSnapshot {
    /// Service label (frontend, backend, ...)
    pub label: String,
    /// Command that was executed
    pub command: String,
    /// Directory the command runs in (scopes the label to one project)
    pub working_directory: String,
    /// Operating system process id, while it is alive
    pub pid: Option<u32>,
    /// Lifecycle state
    pub state: ProcessState,
    /// Start time in milliseconds since the Unix epoch
    pub started_at_ms: Option<u64>,
    /// Exit code, once the process ended
    pub exit_code: Option<i32>,
    /// Human-readable detail (platform, exit reason)
    pub detail: String,
}

/// A process Pilot started, with its captured output
pub struct ProcessRecord {
    /// What was started, kept so a restart can repeat the same command
    pub request: ProcessRequest,
    /// Operating system process id
    pub pid: u32,
    /// Start time in milliseconds since the Unix epoch
    pub started_at_ms: u64,
    /// Captured output, shared with the reader threads
    pub logs: Arc<LogBuffer>,
    inner: Mutex<RecordInner>,
}

/// Mutable fields behind a single mutex, locked together so a snapshot or a
/// transition can never mix a new state with an old exit code.
pub struct RecordInner {
    state: ProcessState,
    exit_code: Option<i32>,
    child: Option<Child>,
    watcher_handle: Option<std::thread::JoinHandle<()>>,
    #[cfg(windows)]
    job: Option<crate::platform::JobObjectGuard>,
}

/// Locked view of a record's mutable fields, held for one short critical section
pub struct RecordLock<'a> {
    inner: MutexGuard<'a, RecordInner>,
}

impl<'a> RecordLock<'a> {
    /// Current lifecycle state
    pub fn state(&self) -> ProcessState {
        self.inner.state
    }

    /// Mutable lifecycle state
    pub fn state_mut(&mut self) -> &mut ProcessState {
        &mut self.inner.state
    }

    /// Recorded exit code, if the process ended
    pub fn exit_code(&self) -> Option<i32> {
        self.inner.exit_code
    }

    /// Mutable exit code
    pub fn exit_code_mut(&mut self) -> &mut Option<i32> {
        &mut self.inner.exit_code
    }

    /// The child handle, if it is still tracked
    pub fn child(&mut self) -> &mut Option<Child> {
        &mut self.inner.child
    }

    /// The watcher thread handle, if running
    pub fn watcher_handle(&mut self) -> &mut Option<std::thread::JoinHandle<()>> {
        &mut self.inner.watcher_handle
    }
}

impl ProcessRecord {
    /// Create a record for a freshly started child process
    pub fn new(request: ProcessRequest, pid: u32, logs: Arc<LogBuffer>, child: Child) -> Self {
        ProcessRecord {
            request,
            pid,
            started_at_ms: now_ms(),
            logs,
            inner: Mutex::new(RecordInner {
                state: ProcessState::Running,
                exit_code: None,
                child: Some(child),
                watcher_handle: None,
                #[cfg(windows)]
                job: None,
            }),
        }
    }

    /// Assign a Windows Job Object to this record
    #[cfg(windows)]
    pub fn set_job(&self, job: crate::platform::JobObjectGuard) {
        let mut inner = self.lock();
        inner.inner.job = Some(job);
    }

    /// Terminate the Windows Job Object if present
    #[cfg(windows)]
    pub fn terminate_job(&self) {
        let inner = self.lock();
        if let Some(ref job) = inner.inner.job {
            job.terminate();
        }
    }

    /// Create a record for a process that could not be started
    pub fn failed(request: ProcessRequest, logs: Arc<LogBuffer>) -> Self {
        ProcessRecord {
            request,
            pid: 0,
            started_at_ms: now_ms(),
            logs,
            inner: Mutex::new(RecordInner {
                state: ProcessState::Failed,
                exit_code: None,
                child: None,
                watcher_handle: None,
                #[cfg(windows)]
                job: None,
            }),
        }
    }

    /// Locked view of a record's mutable fields
    pub fn lock(&self) -> RecordLock<'_> {
        RecordLock {
            inner: self
                .inner
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        }
    }

    /// Current lifecycle state
    pub fn state(&self) -> ProcessState {
        self.lock().state()
    }

    /// Update the lifecycle state
    pub fn set_state(&self, state: ProcessState) {
        *self.lock().state_mut() = state;
    }

    /// The exit code, if the process already ended
    pub fn exit_code(&self) -> Option<i32> {
        self.lock().exit_code()
    }

    /// Record an exit code
    pub fn set_exit_code(&self, code: Option<i32>) {
        *self.lock().exit_code_mut() = code;
    }

    /// Take the child handle, leaving nothing behind
    pub fn take_child(&self) -> Option<Child> {
        self.lock().child().take()
    }

    /// Take the output pipes of the child, leaving the handle itself in place
    pub fn take_pipes(&self) -> (Option<ChildStdout>, Option<ChildStderr>) {
        let mut fields = self.lock();

        match fields.child().as_mut() {
            Some(child) => (child.stdout.take(), child.stderr.take()),
            None => (None, None),
        }
    }

    /// Whether the process is still alive
    pub fn is_running(&self) -> bool {
        matches!(self.state(), ProcessState::Running)
    }

    /// Serializable snapshot of the record
    pub fn snapshot(&self) -> ProcessSnapshot {
        let state = self.state();
        let running = self.is_running();
        let detail = match state {
            ProcessState::Running => {
                format!(
                    "started by Pilot on {}",
                    crate::platform::current_platform()
                )
            }
            ProcessState::Stopped => "stopped by Pilot".to_string(),
            ProcessState::Exited => match self.exit_code() {
                Some(code) => format!("exited with code {code}"),
                None => "exited".to_string(),
            },
            ProcessState::Failed => "could not be started".to_string(),
        };

        ProcessSnapshot {
            label: self.request.label.clone(),
            command: self.request.command.clone(),
            working_directory: self.request.working_directory.clone(),
            pid: running.then_some(self.pid),
            state,
            started_at_ms: Some(self.started_at_ms),
            exit_code: self.exit_code(),
            detail,
        }
    }

    /// Mutably lock the child handle, for the manager's stop and watcher threads
    pub fn lock_child(&self) -> RecordLock<'_> {
        self.lock()
    }

    /// Set the watcher thread handle
    pub fn set_watcher_handle(&self, handle: std::thread::JoinHandle<()>) {
        let mut fields = self.lock();
        fields.watcher_handle().replace(handle);
    }

    /// Take the watcher thread handle, leaving nothing behind
    pub fn take_watcher_handle(&self) -> Option<std::thread::JoinHandle<()>> {
        let mut fields = self.lock();
        fields.watcher_handle().take()
    }
}

impl Drop for ProcessRecord {
    fn drop(&mut self) {
        // Ensure the watcher thread is joined to prevent thread leaks
        if let Some(handle) = self.take_watcher_handle() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_request() -> ProcessRequest {
        ProcessRequest::new("frontend", "npm run dev", ".")
    }

    #[test]
    fn snapshots_serialize_with_camel_case_keys() {
        let record = ProcessRecord::failed(sample_request(), Arc::new(LogBuffer::new(4)));
        let json = serde_json::to_value(record.snapshot()).expect("snapshot must serialize");

        assert_eq!(json["label"], "frontend");
        assert_eq!(json["command"], "npm run dev");
        assert_eq!(json["state"], "failed");
        assert!(json["pid"].is_null());
        assert!(json["startedAtMs"].is_u64());
        assert!(json["detail"].is_string());
    }

    #[test]
    fn a_failed_record_reports_that_it_could_not_start() {
        let record = ProcessRecord::failed(sample_request(), Arc::new(LogBuffer::new(4)));

        assert_eq!(record.state(), ProcessState::Failed);
        assert!(!record.is_running());
        assert!(record.snapshot().detail.contains("could not be started"));
    }

    #[test]
    fn state_and_exit_code_can_be_updated() {
        let record = ProcessRecord::failed(sample_request(), Arc::new(LogBuffer::new(4)));

        record.set_state(ProcessState::Running);
        assert!(record.is_running());

        record.set_state(ProcessState::Exited);
        record.set_exit_code(Some(3));

        let snapshot = record.snapshot();
        assert_eq!(snapshot.state, ProcessState::Exited);
        assert_eq!(snapshot.exit_code, Some(3));
        assert!(snapshot.detail.contains("exited with code 3"));
        assert!(snapshot.pid.is_none());
    }
}
