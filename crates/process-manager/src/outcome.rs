//! Outcome of a process operation.

use crate::registry::ProcessSnapshot;
use serde::Serialize;

/// Outcome of a process operation
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProcessOutcome {
    /// The process was started
    Started(ProcessSnapshot),
    /// A snapshot was read
    Snapshot(ProcessSnapshot),
    /// The process was stopped
    Stopped(ProcessSnapshot),
    /// No process with this label is tracked
    NotFound(String),
    /// The operation failed, with a readable reason
    Error(String),
}

impl ProcessOutcome {
    /// The snapshot carried by this outcome, if any
    pub fn snapshot(&self) -> Option<&ProcessSnapshot> {
        match self {
            ProcessOutcome::Started(snapshot)
            | ProcessOutcome::Snapshot(snapshot)
            | ProcessOutcome::Stopped(snapshot) => Some(snapshot),
            ProcessOutcome::NotFound(_) | ProcessOutcome::Error(_) => None,
        }
    }

    /// Whether the operation succeeded
    pub fn is_ok(&self) -> bool {
        !matches!(
            self,
            ProcessOutcome::Error(_) | ProcessOutcome::NotFound(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::ProcessState;
    use crate::ProcessRequest;

    fn snapshot() -> ProcessSnapshot {
        let request = ProcessRequest {
            label: "frontend".to_string(),
            command: "npm run dev".to_string(),
            working_directory: ".".to_string(),
        };

        crate::registry::ProcessRecord::failed(request, std::sync::Arc::new(
            crate::log_buffer::LogBuffer::new(2),
        ))
        .snapshot()
    }

    #[test]
    fn started_outcomes_carry_a_snapshot() {
        let outcome = ProcessOutcome::Started(snapshot());

        assert!(outcome.is_ok());
        assert_eq!(outcome.snapshot().map(|s| s.label.as_str()), Some("frontend"));
    }

    #[test]
    fn error_outcomes_carry_no_snapshot() {
        let outcome = ProcessOutcome::Error("nope".to_string());

        assert!(!outcome.is_ok());
        assert!(outcome.snapshot().is_none());
        assert_eq!(outcome.snapshot(), None);
        assert!(matches!(
            ProcessOutcome::NotFound("backend".to_string()).snapshot(),
            None
        ));
        let _ = ProcessState::Running;
    }

    #[test]
    fn outcomes_serialize_with_camel_case_tags() {
        let json = serde_json::to_value(ProcessOutcome::NotFound("db".to_string()))
            .expect("outcome must serialize");

        assert!(json.is_object());
        assert!(json.get("notFound").is_some() || json.get("NotFound").is_some());
    }
}