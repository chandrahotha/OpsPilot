//! Pilot Process Manager - Process lifecycle operations.
//!
//! The lifecycle is real as of phase 4: Pilot starts, tracks, captures and stops
//! project processes, and only ever the ones it started itself
//! ("Pilot Prerequisite.md" section 11).
//!
//! Module map:
//!
//! * [`platform`] - the only place with platform-specific commands
//! * [`log_buffer`] - bounded per-service log capture
//! * [`capture`] - pipes-to-log reader threads
//! * [`registry`] - process records and their serializable snapshots
//! * [`history`] - what Pilot executed, for the operation history
//! * [`outcome`] - results of lifecycle operations
//! * [`manager`] - the [`ProcessManager`] contract and its local implementation
//! * [`plan`] - determined startup sequences and their validation

pub mod capture;
pub mod history;
pub mod log_buffer;
pub mod manager;
pub mod outcome;
pub mod plan;
pub mod platform;
pub mod registry;
pub mod unified_log;

pub use history::{HistoryEntry, OperationHistory};
pub use log_buffer::{LogBuffer, LogEntry, LogStream};
pub use manager::{LocalProcessManager, ProcessManager};
pub use outcome::ProcessOutcome;
pub use plan::{
    StartupPlan, StartupStep, build_startup_plan, command_is_declared, command_tool,
    validate_plan_ports, validate_step,
};
pub use platform::{current_platform, kill_tree_command, shell_command, stop_tree_command};
pub use registry::{ProcessRecord, ProcessSnapshot, ProcessState};
pub use unified_log::UnifiedLogStream;

use serde::{Deserialize, Serialize};

/// A process the user asked Pilot to run
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessRequest {
    /// Stable key used to correlate log output with a project service
    pub label: String,
    /// Shell command to execute
    pub command: String,
    /// Working directory for the process
    pub working_directory: String,
}

impl ProcessRequest {
    /// Create a request for a service label and command, run in `directory`
    pub fn new(
        label: impl Into<String>,
        command: impl Into<String>,
        directory: impl Into<String>,
    ) -> Self {
        ProcessRequest {
            label: label.into(),
            command: command.into(),
            working_directory: directory.into(),
        }
    }
}
