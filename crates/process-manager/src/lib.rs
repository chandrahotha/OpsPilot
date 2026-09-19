//! Pilot Process Manager - Process lifecycle operations.
//!
//! PHASE STATUS: the lifecycle itself arrives in phase 4. The contract below is
//! already in place so the operation layer, the GUI and the integrations share
//! one definition of "what a managed process is".
//!
//! Safety rule that the implementation must honour: Pilot tracks only the
//! processes it started itself. It never stops an unrelated system process
//! ("Pilot Prerequisite.md" section 11).

use serde::{Deserialize, Serialize};

/// Result of a process operation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProcessResult {
    /// Operation succeeded
    Success,
    /// Operation failed with error
    Error(String),
    /// Process not found
    NotFound,
    /// The operation is not implemented yet, with the reason
    NotImplemented(&'static str),
}

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

/// Planned startup step, in execution order
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupStep {
    /// Human-readable step description shown in the progress list
    pub description: String,
    /// Command executed by this step
    pub command: String,
}

/// Trait for platform-specific process management
///
/// Implementations exist per platform (Windows, macOS, Linux); the higher-level
/// Pilot engine does not depend on which one is active.
pub trait ProcessManager: Send + Sync {
    /// Start a process
    fn start(&self, request: &ProcessRequest) -> ProcessResult;

    /// Stop a process Pilot started
    fn stop(&self, process_id: u32) -> ProcessResult;

    /// Restart a process Pilot started
    fn restart(&self, process_id: u32) -> ProcessResult;

    /// Get status of a process Pilot started
    fn status(&self, process_id: u32) -> ProcessResult;
}

/// Platform tag used in diagnostics and operation history
pub fn current_platform() -> &'static str {
    std::env::consts::OS
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Unimplemented;

    impl ProcessManager for Unimplemented {
        fn start(&self, _request: &ProcessRequest) -> ProcessResult {
            ProcessResult::NotImplemented("start arrives in phase 4")
        }

        fn stop(&self, _process_id: u32) -> ProcessResult {
            ProcessResult::NotImplemented("stop arrives in phase 4")
        }

        fn restart(&self, _process_id: u32) -> ProcessResult {
            ProcessResult::NotImplemented("restart arrives in phase 4")
        }

        fn status(&self, _process_id: u32) -> ProcessResult {
            ProcessResult::NotImplemented("status arrives in phase 4")
        }
    }

    #[test]
    fn the_trait_is_object_safe_for_platform_implementations() {
        let manager: Box<dyn ProcessManager> = Box::new(Unimplemented);
        let request = ProcessRequest {
            label: "frontend".to_string(),
            command: "npm run dev".to_string(),
            working_directory: ".".to_string(),
        };

        assert!(matches!(
            manager.start(&request),
            ProcessResult::NotImplemented(_)
        ));
    }

    #[test]
    fn the_current_platform_is_reported() {
        assert!(!current_platform().is_empty());
    }
}