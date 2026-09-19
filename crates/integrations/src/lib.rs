//! Pilot Integrations - Node.js, Python, Docker, Prisma, Django and Alembic.
//!
//! PHASE STATUS: this crate is the API surface for the operation phases (4-6).
//! Detection is not duplicated here - it is delegated to `pilot-scanner` so the
//! integrations and the GUI can never disagree. Operation execution needs the
//! process lifecycle layer (phase 4) and therefore returns
//! [`IntegrationResult::NotImplemented`] until then: Pilot never reports an
//! action as completed when it did not happen.

use std::process::Command;

/// Result of an integration operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrationResult {
    /// The integration is available or the operation completed
    Success,
    /// The operation failed, with a readable message
    Error(String),
    /// The operation is not implemented yet, with the reason
    NotImplemented(&'static str),
}

/// Node.js integration
pub mod node {
    use crate::IntegrationResult;
    use pilot_scanner::Detector;
    use pilot_scanner::detectors::node::NodeDetector;

    /// Whether a Node.js project was detected at `path`
    pub fn detect_project(path: &str) -> IntegrationResult {
        if NodeDetector.detect(path) {
            IntegrationResult::Success
        } else {
            IntegrationResult::Error(format!("no package.json found in {path}"))
        }
    }

    /// Commands the project exposes
    pub fn get_commands() -> IntegrationResult {
        IntegrationResult::NotImplemented(
            "listing project commands requires command validation (phase 4)",
        )
    }
}

/// Python integration
pub mod python {
    use crate::IntegrationResult;
    use pilot_scanner::Detector;
    use pilot_scanner::detectors::python::PythonDetector;

    /// Whether a Python project was detected at `path`
    pub fn detect_project(path: &str) -> IntegrationResult {
        if PythonDetector.detect(path) {
            IntegrationResult::Success
        } else {
            IntegrationResult::Error(format!("no Python manifest found in {path}"))
        }
    }

    /// Commands the project exposes
    pub fn get_commands() -> IntegrationResult {
        IntegrationResult::NotImplemented(
            "listing project commands requires command validation (phase 4)",
        )
    }
}

/// Docker integration
pub mod docker {
    use crate::docker_command;

    /// Whether the Docker CLI is available, probed read-only
    pub fn is_available() -> bool {
        docker_command("--version")
            .output()
            .is_ok_and(|output| output.status.success())
    }
}

/// Prisma integration
pub mod prisma {
    use crate::IntegrationResult;

    /// Run a Prisma migration
    pub fn migrate() -> IntegrationResult {
        IntegrationResult::NotImplemented("running migrations requires the process layer (phase 4)")
    }
}

/// Django integration
pub mod django {
    use crate::IntegrationResult;

    /// Run Django makemigrations
    pub fn makemigrations() -> IntegrationResult {
        IntegrationResult::NotImplemented(
            "running makemigrations requires the process layer (phase 4)",
        )
    }

    /// Run Django migrate
    pub fn migrate() -> IntegrationResult {
        IntegrationResult::NotImplemented("running migrations requires the process layer (phase 4)")
    }
}

/// Alembic integration
pub mod alembic {
    use crate::IntegrationResult;

    /// Run an Alembic migration
    pub fn migrate() -> IntegrationResult {
        IntegrationResult::NotImplemented("running migrations requires the process layer (phase 4)")
    }
}

/// Build a Docker CLI probe, handling Windows shims
fn docker_command(flag: &str) -> Command {
    #[cfg(windows)]
    {
        let mut command = Command::new("cmd");
        command.args(["/C", "docker", flag]);
        command
    }

    #[cfg(not(windows))]
    {
        let mut command = Command::new("docker");
        command.arg(flag);
        command
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_detection_reports_a_missing_manifest() {
        let result = node::detect_project("this/path/does/not/exist");

        assert!(matches!(result, IntegrationResult::Error(_)));
    }

    #[test]
    fn unimplemented_operations_say_so() {
        assert!(matches!(
            prisma::migrate(),
            IntegrationResult::NotImplemented(_)
        ));
        assert!(matches!(
            django::makemigrations(),
            IntegrationResult::NotImplemented(_)
        ));
        assert!(matches!(
            alembic::migrate(),
            IntegrationResult::NotImplemented(_)
        ));
    }

    #[test]
    fn docker_availability_is_probed_not_assumed() {
        // The result depends on the machine, so only stability is asserted here.
        assert_eq!(docker::is_available(), docker::is_available());
    }
}