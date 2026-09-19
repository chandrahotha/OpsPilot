use crate::capture::spawn_output_readers;
use crate::history::{HistoryEntry, OperationHistory};
use crate::log_buffer::LogBuffer;
use crate::outcome::ProcessOutcome;
use crate::platform::{current_platform, shell_command, stop_tree_command};
use crate::registry::{ProcessRecord, ProcessSnapshot, ProcessState};
use crate::ProcessRequest;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

/// Log lines kept per service
pub const LOG_CAPACITY: usize = 1000;

/// How long a stop waits for the process tree to exit before killing it directly
pub const STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// How often the manager re-checks whether a process is still alive
pub const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Contract for platform-specific process management.
///
/// Implementations exist per platform (Windows, macOS, Linux); the higher-level
/// Pilot engine does not depend on which one is active.
pub trait ProcessManager: Send + Sync {
    /// Start a process and begin capturing its output
    fn start(&self, request: &ProcessRequest) -> ProcessOutcome;

    /// Stop a process Pilot started, together with its child processes
    fn stop(&self, label: &str) -> ProcessOutcome;

    /// Stop a process and start it again with the same command
    fn restart(&self, label: &str) -> ProcessOutcome;

    /// Read the state of a process Pilot started
    fn status(&self, label: &str) -> ProcessOutcome;

    /// Every tracked process, ordered by label
    fn list(&self) -> Vec<ProcessSnapshot>;

    /// The log buffer of a service, for live output
    fn log_buffer(&self, label: &str) -> Option<Arc<LogBuffer>>;

    /// The operation history, newest first
    fn history(&self) -> Vec<HistoryEntry>;
}

/// Process manager for the machine Pilot is running on
pub struct LocalProcessManager {
    records: Mutex<HashMap<String, Arc<ProcessRecord>>>,
    history: OperationHistory,
}

impl Default for LocalProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalProcessManager {
    /// Create an empty manager
    pub fn new() -> Self {
        LocalProcessManager {
            records: Mutex::new(HashMap::new()),
            history: OperationHistory::default(),
        }
    }

    /// Stop a tracked process, waiting for the tree to exit
    fn stop_process(&self, record: &Arc<ProcessRecord>) -> ProcessOutcome {
        let label = record.request.label.clone();

        self.refresh(record);

        if !record.is_running() {
            let detail = "was not running anymore".to_string();
            record.set_state(ProcessState::Exited);

            let snapshot = record.snapshot();

            return ProcessOutcome::Stopped(ProcessSnapshot { detail, ..snapshot });
        }

        record
            .logs
            .push_system(&label, format!("stopping (pid {})", record.pid));

        // Ask the platform to take the whole tree; on Windows this is forceful.
        let _ = stop_tree_command(record.pid).output();

        let deadline = Instant::now() + STOP_TIMEOUT;

        while Instant::now() < deadline {
            self.refresh(record);

            if !record.is_running() {
                break;
            }

            thread::sleep(POLL_INTERVAL);
        }

        if record.is_running() {
            // The tree signal did not reach the process: kill the direct child.
            if let Some(mut child) = record.take_child() {
                let _ = child.kill();
                let code = child.wait().ok().and_then(|status| status.code());
                record.set_exit_code(code);
            }
        }

        let state = if record.state() == ProcessState::Exited {
            ProcessState::Exited
        } else {
            ProcessState::Stopped
        };

        record.set_state(state);

        let detail = match record.exit_code() {
            Some(code) => format!("stopped by Pilot (exit code {code})"),
            None => "stopped by Pilot".to_string(),
        };

        record.logs.push_system(&label, detail.clone());
        self.history.record("stop", label.clone());

        let snapshot = record.snapshot();
        ProcessOutcome::Stopped(ProcessSnapshot { detail, ..snapshot })
    }

    fn lock_records(&self) -> MutexGuard<'_, HashMap<String, Arc<ProcessRecord>>> {
        self.records
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn start_process(&self, request: &ProcessRequest) -> Result<Arc<ProcessRecord>, String> {
        let mut records = self.lock_records();

        if let Some(existing) = records.get(&request.label)
            && existing.is_running()
        {
            return Err(format!(
                "{} is already running (pid {}); stop it first",
                request.label, existing.pid
            ));
        }

        let logs = Arc::new(LogBuffer::new(LOG_CAPACITY));
        logs.push_system(
            &request.label,
            format!(
                "starting `{}` in {}",
                request.command, request.working_directory
            ),
        );

        let mut shell = shell_command(&request.command);
        shell
            .current_dir(&request.working_directory)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // A dedicated process group lets a Unix stop take the whole tree, so the
        // real dev server does not survive its shell.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            shell.process_group(0);
        }

        let record = match shell.spawn() {
            Ok(child) => {
                let pid = child.id();
                logs.push_system(&request.label, format!("started with pid {pid}"));

                let record = Arc::new(ProcessRecord::new(
                    request.clone(),
                    pid,
                    Arc::clone(&logs),
                    child,
                ));

                records.insert(request.label.clone(), Arc::clone(&record));
                record
            }
            Err(error) => {
                let record = Arc::new(ProcessRecord::failed(request.clone(), Arc::clone(&logs)));
                logs.push_system(&request.label, format!("could not start: {error}"));
                records.insert(request.label.clone(), record);

                return Err(format!("could not start `{}`: {error}", request.command));
            }
        };

        drop(records);

        self.watch(&record);

        self.history
            .record("start", format!("{}: {}", request.label, request.command));

        Ok(record)
    }

    /// Start the output readers and the exit watcher for a fresh record
    fn watch(&self, record: &Arc<ProcessRecord>) {
        spawn_output_readers(
            &record.request.label,
            Arc::clone(&record.logs),
            record.take_pipes(),
        );

        let watcher = Arc::clone(record);
        let platform = current_platform();

        thread::spawn(move || loop {
            thread::sleep(POLL_INTERVAL);

            let exit = {
                let mut fields = watcher.lock();

                match fields.child().as_mut() {
                    Some(child) => child.try_wait().ok().flatten(),
                    None => None,
                }
            };

            let Some(status) = exit else {
                continue;
            };

            watcher.set_exit_code(status.code());

            if watcher.state() == ProcessState::Running {
                watcher.set_state(ProcessState::Exited);
            }

            let reason = status
                .code()
                .map(|code| format!("process exited with code {code}"))
                .unwrap_or_else(|| format!("process ended on {platform}"));

            watcher.logs.push_system(&watcher.request.label, reason);
            break;
        });
    }

    /// Update a record from the operating system, without overriding a stop
    fn refresh(&self, record: &Arc<ProcessRecord>) {
        let exit = {
            let mut fields = record.lock();

            match fields.child().as_mut() {
                Some(child) => child.try_wait().ok().flatten(),
                None => None,
            }
        };

        if let Some(status) = exit {
            record.set_exit_code(status.code());

            if record.state() == ProcessState::Running {
                record.set_state(ProcessState::Exited);
            }
        }
    }
}

impl ProcessManager for LocalProcessManager {
    fn start(&self, request: &ProcessRequest) -> ProcessOutcome {
        match self.start_process(request) {
            Ok(record) => ProcessOutcome::Started(record.snapshot()),
            Err(error) => ProcessOutcome::Error(error),
        }
    }

    fn stop(&self, label: &str) -> ProcessOutcome {
        let record = {
            let records = self.lock_records();

            match records.get(label) {
                Some(record) => Arc::clone(record),
                None => return ProcessOutcome::NotFound(label.to_string()),
            }
        };

        self.stop_process(&record)
    }

    fn restart(&self, label: &str) -> ProcessOutcome {
        let request = {
            let records = self.lock_records();

            match records.get(label) {
                Some(record) => record.request.clone(),
                None => return ProcessOutcome::NotFound(label.to_string()),
            }
        };

        let stopped = self.stop(label);

        if !stopped.is_ok() {
            return stopped;
        }

        self.start(&request)
    }

    fn status(&self, label: &str) -> ProcessOutcome {
        let record = {
            let records = self.lock_records();

            match records.get(label) {
                Some(record) => Arc::clone(record),
                None => return ProcessOutcome::NotFound(label.to_string()),
            }
        };

        self.refresh(&record);
        ProcessOutcome::Snapshot(record.snapshot())
    }

    fn list(&self) -> Vec<ProcessSnapshot> {
        let records = self.lock_records();
        let mut snapshots: Vec<ProcessSnapshot> =
            records.values().map(|record| record.snapshot()).collect();

        snapshots.sort_by(|left, right| left.label.cmp(&right.label));
        snapshots
    }

    fn log_buffer(&self, label: &str) -> Option<Arc<LogBuffer>> {
        self.lock_records()
            .get(label)
            .map(|record| Arc::clone(&record.logs))
    }

    fn history(&self) -> Vec<HistoryEntry> {
        self.history.snapshot(None)
    }
}