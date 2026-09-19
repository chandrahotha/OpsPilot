//! Pilot Docker Manager - Docker container operations.
//!
//! PHASE STATUS: Docker operations arrive in phase 5. The operation list is kept
//! here so the API surface is explicit; execution must first validate that Docker
//! is available ("Pilot Prerequisite.md" section 13).

/// Docker container status
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerStatus {
    /// Container name
    pub name: String,
    /// Whether the container is running
    pub running: bool,
    /// Container status (created, running, paused, stopped, etc.)
    pub status: String,
}

/// Docker management operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockerOperation {
    /// Check Docker status
    CheckStatus,
    /// Start the Docker daemon
    StartDaemon,
    /// Stop the Docker daemon
    StopDaemon,
    /// List all containers
    ListContainers,
    /// Start a container
    StartContainer(String),
    /// Stop a container
    StopContainer(String),
    /// Restart a container
    RestartContainer(String),
    /// View container logs
    Logs(String),
    /// Rebuild Docker image
    RebuildImage(String),
}

/// Outcome of a Docker operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DockerOutcome {
    /// The operation completed, with affected containers
    Containers(Vec<ContainerStatus>),
    /// The Docker CLI is not available on this machine
    Unavailable(String),
    /// The operation is not implemented yet, with the reason
    NotImplemented(&'static str),
}

/// Execute a Docker operation
///
/// PHASE STATUS: unimplemented. Returning success here would be a lie: the GUI
/// shows the outcome to the user.
pub fn execute(_operation: DockerOperation) -> DockerOutcome {
    DockerOutcome::NotImplemented("Docker operations arrive in phase 5")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_operations_report_that_they_are_not_implemented() {
        assert_eq!(
            execute(DockerOperation::CheckStatus),
            DockerOutcome::NotImplemented("Docker operations arrive in phase 5")
        );
    }
}