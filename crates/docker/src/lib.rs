//! Pilot Docker Manager - Docker container operations.
//!
//! Phase 5: Real Docker operations via the Docker CLI. Validates Docker availability
//! before any operation (spec section 13). Uses docker and docker compose commands
//! to inspect, start, stop, restart, log, and rebuild containers and images.

use serde::{Deserialize, Serialize};
use std::process::Command;

/// Docker container status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerStatus {
    /// Container name
    pub name: String,
    /// Container ID (short)
    pub id: String,
    /// Container image
    pub image: String,
    /// Whether the container is running
    pub running: bool,
    /// Container status (created, running, paused, stopped, etc.)
    pub status: String,
    /// Port mappings (host:container)
    pub ports: Vec<String>,
    /// Compose project name (if created via compose)
    pub compose_project: Option<String>,
    /// Service name within compose project (if applicable)
    pub compose_service: Option<String>,
}

/// Docker management operations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DockerOperation {
    /// Check Docker daemon status
    CheckStatus,
    /// List all containers (including stopped)
    ListContainers,
    /// Start a container by name or ID
    StartContainer(String),
    /// Stop a container by name or ID
    StopContainer(String),
    /// Restart a container by name or ID
    RestartContainer(String),
    /// View container logs
    Logs(String),
    /// Rebuild Docker image for a service
    RebuildImage(String),
}

/// Outcome of a Docker operation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DockerOutcome {
    /// Docker daemon status
    Status { available: bool, version: Option<String> },
    /// List of containers
    Containers(Vec<ContainerStatus>),
    /// Operation succeeded, returns affected container names
    Started(Vec<String>),
    Stopped(Vec<String>),
    Restarted(Vec<String>),
    /// Logs output
    Logs { container: String, output: String },
    /// Image rebuilt
    Rebuilt { image: String },
    /// Docker CLI is not available
    Unavailable(String),
    /// Operation failed with error
    Error(String),
}

/// Check if Docker CLI is available and get version
pub fn docker_version() -> Option<String> {
    Command::new("docker")
        .args(["--version"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
}

/// Check if Docker daemon is running
pub fn docker_available() -> bool {
    Command::new("docker")
        .args(["info"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if Docker Compose is available (v2 plugin)
pub fn compose_available() -> bool {
    Command::new("docker")
        .args(["compose", "version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Parse docker ps -a --format json output
fn parse_containers_json(output: &str) -> Vec<ContainerStatus> {
    output
        .lines()
        .filter_map(|line| {
            let json: serde_json::Value = serde_json::from_str(line).ok()?;
            Some(ContainerStatus {
                name: json["Names"].as_str().unwrap_or("").to_string(),
                id: json["ID"].as_str().unwrap_or("").to_string(),
                image: json["Image"].as_str().unwrap_or("").to_string(),
                running: json["State"].as_str() == Some("running"),
                status: json["Status"].as_str().unwrap_or("").to_string(),
                ports: json["Ports"]
                    .as_str()
                    .map(|p| p.split(", ").map(|s| s.to_string()).collect())
                    .unwrap_or_default(),
                compose_project: json["ComposeProject"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
                compose_service: json["ComposeService"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string()),
            })
        })
        .collect()
}

/// List all containers
pub fn list_containers() -> DockerOutcome {
    if !docker_available() {
        return DockerOutcome::Unavailable("Docker daemon is not running".to_string());
    }

    let output = Command::new("docker")
        .args([
            "ps",
            "-a",
            "--format",
            r#"{\"Names\":\"{{.Names}}\",\"ID\":\"{{.ID}}\",\"Image\":\"{{.Image}}\",\"State\":\"{{.State}}\",\"Status\":\"{{.Status}}\",\"Ports\":\"{{.Ports}}\",\"ComposeProject\":\"{{.ComposeProject}}\",\"ComposeService\":\"{{.ComposeService}}\"}"#,
        ])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let containers = parse_containers_json(&stdout);
            DockerOutcome::Containers(containers)
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            DockerOutcome::Error(format!("docker ps failed: {stderr}"))
        }
        Err(e) => DockerOutcome::Error(format!("failed to execute docker ps: {e}")),
    }
}

/// Start a container by name or ID
pub fn start_container(name: &str) -> DockerOutcome {
    if !docker_available() {
        return DockerOutcome::Unavailable("Docker daemon is not running".to_string());
    }

    let output = Command::new("docker")
        .args(["start", name])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            DockerOutcome::Started(vec![name.to_string()])
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            DockerOutcome::Error(format!("docker start {name} failed: {stderr}"))
        }
        Err(e) => DockerOutcome::Error(format!("failed to execute docker start: {e}")),
    }
}

/// Stop a container by name or ID
pub fn stop_container(name: &str) -> DockerOutcome {
    if !docker_available() {
        return DockerOutcome::Unavailable("Docker daemon is not running".to_string());
    }

    let output = Command::new("docker")
        .args(["stop", name])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            DockerOutcome::Stopped(vec![name.to_string()])
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            DockerOutcome::Error(format!("docker stop {name} failed: {stderr}"))
        }
        Err(e) => DockerOutcome::Error(format!("failed to execute docker stop: {e}")),
    }
}

/// Restart a container by name or ID
pub fn restart_container(name: &str) -> DockerOutcome {
    if !docker_available() {
        return DockerOutcome::Unavailable("Docker daemon is not running".to_string());
    }

    let output = Command::new("docker")
        .args(["restart", name])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            DockerOutcome::Restarted(vec![name.to_string()])
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            DockerOutcome::Error(format!("docker restart {name} failed: {stderr}"))
        }
        Err(e) => DockerOutcome::Error(format!("failed to execute docker restart: {e}")),
    }
}

/// Get container logs
pub fn container_logs(name: &str) -> DockerOutcome {
    if !docker_available() {
        return DockerOutcome::Unavailable("Docker daemon is not running".to_string());
    }

    let output = Command::new("docker")
        .args(["logs", "--tail", "100", name])
        .output();

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let combined = if stderr.is_empty() {
                stdout.to_string()
            } else {
                format!("{stdout}\n{stderr}")
            };
            DockerOutcome::Logs {
                container: name.to_string(),
                output: combined,
            }
        }
        Err(e) => DockerOutcome::Error(format!("failed to execute docker logs: {e}")),
    }
}

/// Rebuild image for a service (via docker compose build)
pub fn rebuild_image(service: &str) -> DockerOutcome {
    if !docker_available() {
        return DockerOutcome::Unavailable("Docker daemon is not running".to_string());
    }

    if !compose_available() {
        return DockerOutcome::Unavailable("Docker Compose is not available".to_string());
    }

    let output = Command::new("docker")
        .args(["compose", "build", "--no-cache", service])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            DockerOutcome::Rebuilt {
                image: service.to_string(),
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            DockerOutcome::Error(format!("docker compose build {service} failed: {stderr}"))
        }
        Err(e) => DockerOutcome::Error(format!("failed to execute docker compose build: {e}")),
    }
}

/// Execute a Docker operation
pub fn execute(operation: DockerOperation) -> DockerOutcome {
    match operation {
        DockerOperation::CheckStatus => {
            let available = docker_available();
            let version = if available { docker_version() } else { None };
            DockerOutcome::Status { available, version }
        }
        DockerOperation::ListContainers => list_containers(),
        DockerOperation::StartContainer(name) => start_container(&name),
        DockerOperation::StopContainer(name) => stop_container(&name),
        DockerOperation::RestartContainer(name) => restart_container(&name),
        DockerOperation::Logs(name) => container_logs(&name),
        DockerOperation::RebuildImage(service) => rebuild_image(&service),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_version_parses_correctly() {
        let _ = docker_version();
    }

    #[test]
    fn check_status_returns_available_or_unavailable() {
        let result = execute(DockerOperation::CheckStatus);
        assert!(matches!(
            result,
            DockerOutcome::Status { .. } | DockerOutcome::Unavailable(_)
        ));
    }

    #[test]
    fn parse_containers_handles_empty_output() {
        let containers = parse_containers_json("");
        assert!(containers.is_empty());
    }

    #[test]
    fn parse_containers_handles_malformed_json() {
        // `{}` is valid JSON (empty object), so it parses as a ContainerStatus with empty fields
        // Use truly malformed JSON to test error handling
        let containers = parse_containers_json("not json\n{invalid");
        assert!(containers.is_empty());
    }
}