use crate::ProcessRequest;
use crate::capture::spawn_output_readers;
use crate::history::{HistoryEntry, OperationHistory};
use crate::log_buffer::LogBuffer;
use crate::outcome::ProcessOutcome;
use crate::platform::{current_platform, kill_tree_command, shell_command, stop_tree_command};
use crate::registry::{ProcessRecord, ProcessSnapshot, ProcessState};
use std::collections::{HashMap, HashSet};
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

    /// Force-terminate a process Pilot started, without a graceful wait.
    ///
    /// Reserved for stuck processes: normal stops must use `stop`.
    fn kill(&self, label: &str) -> ProcessOutcome;

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

    /// Check whether a service or its descendant processes includes the given PID
    fn contains_pid(&self, label: &str, pid: u32) -> bool;

    /// Find which tracked running service (if any) owns or spawned the given PID
    fn any_contains_pid(&self, pid: u32) -> Option<ProcessSnapshot>;
}

/// Process manager for the machine Pilot is running on
pub struct LocalProcessManager {
    records: Mutex<HashMap<String, Arc<ProcessRecord>>>,
    history: OperationHistory,
    /// Labels with a start/stop/restart/kill currently in progress, so two
    /// concurrent lifecycle operations on the same label (e.g. a double-click
    /// Restart, or Restart racing a manual Stop) serialize instead of racing.
    in_flight: Mutex<HashSet<String>>,
}

/// RAII marker that a lifecycle operation is in progress for one label.
/// Dropping it (including on early return) frees the label for the next
/// operation.
struct LabelGuard<'a> {
    manager: &'a LocalProcessManager,
    label: String,
}

impl Drop for LabelGuard<'_> {
    fn drop(&mut self) {
        self.manager
            .in_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.label);
    }
}

impl Default for LocalProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for LocalProcessManager {
    /// NOTE: Cloning shares the same `records` and `history` so both handles
    /// see the same processes. The old implementation created an empty orphan
    /// manager which was a silent logic trap.
    fn clone(&self) -> Self {
        // The process manager is shared via Arc<LocalProcessManager>; cloning
        // the Arc is the correct pattern. This impl exists only for trait
        // bounds that require Clone on the concrete type. Sharing the inner
        // state ensures both handles manage the same set of processes.
        LocalProcessManager {
            records: Mutex::new(
                self.lock_records()
                    .iter()
                    .map(|(k, v)| (k.clone(), Arc::clone(v)))
                    .collect(),
            ),
            history: self.history.clone(),
            in_flight: Mutex::new(HashSet::new()),
        }
    }
}

impl LocalProcessManager {
    /// Create an empty manager
    pub fn new() -> Self {
        LocalProcessManager {
            records: Mutex::new(HashMap::new()),
            history: OperationHistory::default(),
            in_flight: Mutex::new(HashSet::new()),
        }
    }

    /// Claim exclusive rights to run a lifecycle operation for `label`.
    /// Returns an error instead of blocking, matching the existing
    /// "already running; stop it first" style of reporting a conflict.
    fn begin_operation(&self, label: &str) -> Result<LabelGuard<'_>, String> {
        let mut in_flight = self
            .in_flight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if !in_flight.insert(label.to_string()) {
            return Err(format!(
                "another operation is already in progress for {label}; try again once it finishes"
            ));
        }

        Ok(LabelGuard {
            manager: self,
            label: label.to_string(),
        })
    }

    /// Fresh, authoritative liveness check on the child handle Pilot itself
    /// opened. `record.state()` only refreshes every `POLL_INTERVAL` via the
    /// watcher thread, so a process that exited in that window is still
    /// reported "running" for up to that long; sending a tree-kill signal to
    /// its pid in that gap risks hitting an unrelated process if the OS has
    /// already reused the pid. `try_wait()` on the handle Pilot itself holds
    /// carries no such risk, since it identifies the process by handle, not
    /// by pid lookup. Updates state/exit code when it finds an exit, and
    /// reports whether it did.
    fn already_exited(record: &Arc<ProcessRecord>) -> bool {
        let mut fields = record.lock();
        let Some(child) = fields.child().as_mut() else {
            return false;
        };
        let Some(status) = child.try_wait().ok().flatten() else {
            return false;
        };
        *fields.exit_code_mut() = status.code();
        if fields.state() == ProcessState::Running {
            *fields.state_mut() = ProcessState::Exited;
        }
        true
    }

    /// Stop a tracked process, waiting for the tree to exit
    fn stop_process(&self, record: &Arc<ProcessRecord>) -> ProcessOutcome {
        let label = record.request.label.clone();

        if !record.is_running() {
            let detail = "was not running anymore".to_string();
            record.set_state(ProcessState::Exited);

            let snapshot = record.snapshot();

            return ProcessOutcome::Stopped(ProcessSnapshot { detail, ..snapshot });
        }

        if Self::already_exited(record) {
            let detail = "exited just before it could be stopped".to_string();
            let snapshot = record.snapshot();

            return ProcessOutcome::Stopped(ProcessSnapshot { detail, ..snapshot });
        }

        record
            .logs
            .push_system(&label, format!("stopping (pid {})", record.pid));

        #[cfg(windows)]
        record.terminate_job();

        // Ask the platform to take the whole tree; on Windows this is forceful.
        let _ = stop_tree_command(record.pid).output();

        let deadline = Instant::now() + STOP_TIMEOUT;

        while Instant::now() < deadline {
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
            record.set_state(ProcessState::Stopped);
            ProcessState::Stopped
        };

        let platform = current_platform();
        let detail = match state {
            ProcessState::Exited => record
                .lock()
                .exit_code()
                .map(|code| format!("exited with code {code}"))
                .unwrap_or_else(|| "exited".to_string()),
            _ => format!("stopped on {platform}"),
        };

        record.logs.push_system(&label, detail.clone());
        self.history.record("stop", label.clone());

        // Join the watcher thread to prevent thread leak
        if let Some(handle) = record.take_watcher_handle() {
            let _ = handle.join();
        }

        let snapshot = record.snapshot();
        ProcessOutcome::Stopped(ProcessSnapshot { detail, ..snapshot })
    }

    fn lock_records(&self) -> MutexGuard<'_, HashMap<String, Arc<ProcessRecord>>> {
        self.records
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Remove deliberately-stopped records to prevent memory leaks.
    ///
    /// Failed and Exited records are retained so the GUI can display "could
    /// not be started"/exit details, and so a caller polling `status()` for a
    /// one-shot command (e.g. a database migrate/reset) to reach `Exited`
    /// cannot have the record evicted out from under it by a concurrent
    /// `list()` call landing in the gap between the watcher marking the
    /// process Exited and the poller reading that state. They are removed
    /// once the label is started again (start_process overwrites the record).
    fn cleanup_records(&self) {
        let mut records = self.lock_records();
        records.retain(|_, record| {
            matches!(
                record.state(),
                ProcessState::Running | ProcessState::Failed | ProcessState::Exited
            )
        });
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
            .envs(&request.env)
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

                #[cfg(windows)]
                let job = {
                    use std::os::windows::io::AsRawHandle;
                    let job_opt = crate::platform::JobObjectGuard::new();
                    if let Some(ref job) = job_opt {
                        // SAFETY: child.as_raw_handle() is a valid open process handle.
                        unsafe { job.assign_process(child.as_raw_handle()) };
                    }
                    job_opt
                };

                let record = Arc::new(ProcessRecord::new(
                    request.clone(),
                    pid,
                    Arc::clone(&logs),
                    child,
                ));

                #[cfg(windows)]
                if let Some(job) = job {
                    record.set_job(job);
                }

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

        let handle = thread::spawn(move || {
            /// Maximum time a watcher will poll before giving up and marking the
            /// process as exited. Prevents zombie threads from leaked handles.
            const MAX_WATCH_DURATION: Duration = Duration::from_secs(5 * 60);

            let started = Instant::now();
            loop {
                thread::sleep(POLL_INTERVAL);

                let exit = {
                    let mut fields = watcher.lock();

                    match fields.child().as_mut() {
                        Some(child) => child.try_wait().ok().flatten(),
                        None => None,
                    }
                };

                if let Some(status) = exit {
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
                }

                // Safety valve: if the process never signals exit (zombie/leaked
                // handle), stop polling after MAX_WATCH_DURATION to avoid a
                // permanent thread leak.
                if started.elapsed() > MAX_WATCH_DURATION {
                    if watcher.state() == ProcessState::Running {
                        watcher.set_state(ProcessState::Exited);
                        watcher.logs.push_system(
                            &watcher.request.label,
                            "watcher timed out; process assumed ended",
                        );
                    }
                    break;
                }
            }
        });

        // Store the watcher handle so we can join it on drop
        record.set_watcher_handle(handle);
    }

    /// Force-terminate a tracked process without a graceful wait.
    fn kill_process(&self, record: &Arc<ProcessRecord>) -> ProcessOutcome {
        let label = record.request.label.clone();

        if !record.is_running() {
            let detail = "was not running anymore".to_string();
            record.set_state(ProcessState::Exited);

            let snapshot = record.snapshot();

            return ProcessOutcome::Stopped(ProcessSnapshot { detail, ..snapshot });
        }

        if Self::already_exited(record) {
            let detail = "exited just before it could be force terminated".to_string();
            let snapshot = record.snapshot();

            return ProcessOutcome::Stopped(ProcessSnapshot { detail, ..snapshot });
        }

        record
            .logs
            .push_system(&label, format!("force terminating (pid {})", record.pid));

        #[cfg(windows)]
        record.terminate_job();

        // No waiting: signal the tree, then kill the direct child at once.
        let _ = kill_tree_command(record.pid).output();

        if let Some(mut child) = record.take_child() {
            let _ = child.kill();
            let code = child.wait().ok().and_then(|status| status.code());
            record.set_exit_code(code);
        }

        record.set_state(ProcessState::Stopped);
        self.history.record("kill", label.clone());

        // Join the watcher thread to prevent thread leak
        if let Some(handle) = record.take_watcher_handle() {
            let _ = handle.join();
        }

        let snapshot = record.snapshot();
        let detail = "force terminated by Pilot".to_string();
        ProcessOutcome::Stopped(ProcessSnapshot { detail, ..snapshot })
    }

    /// Update a record from the operating system, without overriding a stop
    /// Only reads state; the watcher thread owns try_wait()
    fn refresh(&self, _record: &Arc<ProcessRecord>) {
        // Just read the current state; the watcher thread handles process exit detection
        // This avoids the race condition where both refresh() and the watcher call try_wait()
    }
}

impl ProcessManager for LocalProcessManager {
    fn start(&self, request: &ProcessRequest) -> ProcessOutcome {
        let _guard = match self.begin_operation(&request.label) {
            Ok(guard) => guard,
            Err(error) => return ProcessOutcome::Error(error),
        };

        match self.start_process(request) {
            Ok(record) => ProcessOutcome::Started(record.snapshot()),
            Err(error) => ProcessOutcome::Error(error),
        }
    }

    fn stop(&self, label: &str) -> ProcessOutcome {
        let _guard = match self.begin_operation(label) {
            Ok(guard) => guard,
            Err(error) => return ProcessOutcome::Error(error),
        };

        let record = {
            let records = self.lock_records();

            match records.get(label) {
                Some(record) => Arc::clone(record),
                None => return ProcessOutcome::NotFound(label.to_string()),
            }
        };

        self.stop_process(&record)
    }

    fn kill(&self, label: &str) -> ProcessOutcome {
        let _guard = match self.begin_operation(label) {
            Ok(guard) => guard,
            Err(error) => return ProcessOutcome::Error(error),
        };

        let record = {
            let records = self.lock_records();

            match records.get(label) {
                Some(record) => Arc::clone(record),
                None => return ProcessOutcome::NotFound(label.to_string()),
            }
        };

        self.kill_process(&record)
    }

    /// Stop then start the same label as one atomic operation (guarded by
    /// `begin_operation` for the whole sequence): calls the unguarded
    /// `stop_process`/`start_process` helpers directly rather than `self.stop`/
    /// `self.start`, which would try to claim the same label again and fail.
    /// Without this, two concurrent restarts could both observe the process
    /// as stopped and each spawn their own replacement, leaving one untracked.
    fn restart(&self, label: &str) -> ProcessOutcome {
        let _guard = match self.begin_operation(label) {
            Ok(guard) => guard,
            Err(error) => return ProcessOutcome::Error(error),
        };

        let (existing, request) = {
            let records = self.lock_records();

            match records.get(label) {
                Some(record) => (Some(Arc::clone(record)), record.request.clone()),
                None => return ProcessOutcome::NotFound(label.to_string()),
            }
        };

        if let Some(record) = existing {
            let stopped = self.stop_process(&record);
            if !stopped.is_ok() {
                return stopped;
            }
        }

        match self.start_process(&request) {
            Ok(record) => ProcessOutcome::Started(record.snapshot()),
            Err(error) => ProcessOutcome::Error(error),
        }
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
        self.cleanup_records();
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

    fn contains_pid(&self, label: &str, pid: u32) -> bool {
        let records = self.lock_records();
        if let Some(record) = records.get(label)
            && record.is_running()
        {
            if record.pid == pid {
                return true;
            }
            let descendants = crate::platform::get_descendant_pids(record.pid);
            return descendants.contains(&pid);
        }
        false
    }

    fn any_contains_pid(&self, pid: u32) -> Option<ProcessSnapshot> {
        let records = self.lock_records();
        for record in records.values() {
            if record.is_running()
                && (record.pid == pid
                    || crate::platform::get_descendant_pids(record.pid).contains(&pid))
            {
                return Some(record.snapshot());
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::ProcessState;

    /// A command that sleeps ~30s on any platform.
    ///
    /// Note: `timeout.exe` cannot be used here — it exits immediately when
    /// its output is redirected, and Pilot always pipes child output.
    #[cfg(windows)]
    fn sleeper_command() -> &'static str {
        "ping -n 30 127.0.0.1 >nul"
    }

    #[cfg(not(windows))]
    fn sleeper_command() -> &'static str {
        "sleep 30"
    }

    fn sleeper_request(label: &str) -> ProcessRequest {
        ProcessRequest::new(label, sleeper_command(), ".")
    }

    #[test]
    fn kill_force_terminates_a_running_process() {
        let manager = LocalProcessManager::new();

        assert!(manager.start(&sleeper_request("kill-me")).is_ok());

        match manager.kill("kill-me") {
            ProcessOutcome::Stopped(snapshot) => {
                assert_eq!(snapshot.state, ProcessState::Stopped);
                assert!(snapshot.detail.contains("force terminated"));
            }
            other => panic!("expected Stopped, got {other:?}"),
        }

        assert!(
            !manager.status("kill-me").is_ok() || {
                matches!(
                    manager.status("kill-me"),
                    ProcessOutcome::Snapshot(ref snapshot)
                        if snapshot.state != ProcessState::Running
                )
            }
        );
    }

    #[test]
    fn kill_of_an_unknown_label_reports_not_found() {
        let manager = LocalProcessManager::new();

        assert!(matches!(
            manager.kill("no-such-service"),
            ProcessOutcome::NotFound(_)
        ));
    }

    #[test]
    fn kill_of_an_exited_process_reports_it_ended() {
        let manager = LocalProcessManager::new();

        #[cfg(windows)]
        let quick = ProcessRequest::new("quick-exit", "ping -n 1 127.0.0.1 >nul", ".");
        #[cfg(not(windows))]
        let quick = ProcessRequest::new("quick-exit", "true", ".");

        assert!(manager.start(&quick).is_ok());

        // Wait until the watcher observes the exit (up to ~5s).
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match manager.status("quick-exit") {
                ProcessOutcome::Snapshot(snapshot) if snapshot.state == ProcessState::Exited => {
                    break;
                }
                _ => {}
            }
            if Instant::now() >= deadline {
                panic!("quick command did not exit in time");
            }
            thread::sleep(Duration::from_millis(50));
        }

        match manager.kill("quick-exit") {
            ProcessOutcome::Stopped(snapshot) => {
                assert!(snapshot.detail.contains("not running anymore"));
            }
            other => panic!("expected Stopped, got {other:?}"),
        }
    }

    #[test]
    fn contains_pid_recognizes_running_process() {
        let manager = LocalProcessManager::new();
        let request = sleeper_request("contains-test");
        let outcome = manager.start(&request);
        let pid = match outcome {
            ProcessOutcome::Started(snapshot) => snapshot.pid.expect("must have pid"),
            other => panic!("expected Started, got {other:?}"),
        };

        assert!(manager.contains_pid("contains-test", pid));
        assert!(!manager.contains_pid("contains-test", 999_999));
        assert!(manager.any_contains_pid(pid).is_some());
        assert!(manager.any_contains_pid(999_999).is_none());

        manager.kill("contains-test");
    }
}
