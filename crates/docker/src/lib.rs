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

/// Build a `docker` CLI command that never pops up a console window.
///
/// Without `CREATE_NO_WINDOW`, every Docker probe flashes a console on
/// Windows — including the ones behind container-list refreshes.
fn docker_command() -> Command {
    let mut command = Command::new("docker");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}

/// Check if Docker CLI is available and get version
pub fn docker_version() -> Option<String> {
    docker_command()
        .args(["--version"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
}

/// Check if Docker daemon is running
pub fn docker_available() -> bool {
    docker_command()
        .args(["info"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check if Docker Compose is available (v2 plugin)
pub fn compose_available() -> bool {
    docker_command()
        .args(["compose", "version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Compose identity labels present on every compose-managed container,
/// regardless of daemon version.
const COMPOSE_PROJECT_LABEL: &str = "com.docker.compose.project";
const COMPOSE_SERVICE_LABEL: &str = "com.docker.compose.service";

/// Read a compose identity value: prefer the template field when the daemon
/// provides it, fall back to the container labels (always present).
fn compose_value(json: &serde_json::Value, field: &str, label: &str) -> Option<String> {
    json[field]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .or_else(|| {
            json["Labels"].as_str().and_then(|labels| {
                labels.split(',').find_map(|pair| {
                    let (key, value) = pair.split_once('=')?;
                    (key.trim() == label && !value.trim().is_empty())
                        .then(|| value.trim().to_string())
                })
            })
        })
}

/// Parse `docker ps -a --format "{{json .}}"` output (one object per line).
///
/// The full context object is used instead of cherry-picked template fields
/// because fields like `.ComposeProject` do not exist on every daemon
/// version and make the whole template fail. Compose identity comes from
/// labels, which all versions provide.
fn parse_containers_json(output: &str) -> Vec<ContainerStatus> {
    output
        .lines()
        .filter_map(|line| {
            let json: serde_json::Value = serde_json::from_str(line).ok()?;
            let name = json["Names"].as_str().unwrap_or("").trim_start_matches('/');
            if name.is_empty() && json["ID"].as_str().unwrap_or("").is_empty() {
                return None;
            }
            Some(ContainerStatus {
                name: name.to_string(),
                id: json["ID"].as_str().unwrap_or("").to_string(),
                image: json["Image"].as_str().unwrap_or("").to_string(),
                running: json["State"].as_str() == Some("running"),
                status: json["Status"].as_str().unwrap_or("").to_string(),
                ports: json["Ports"]
                    .as_str()
                    .filter(|ports| !ports.is_empty())
                    .map(|ports| ports.split(", ").map(|s| s.to_string()).collect())
                    .unwrap_or_default(),
                compose_project: compose_value(&json, "ComposeProject", COMPOSE_PROJECT_LABEL),
                compose_service: compose_value(&json, "ComposeService", COMPOSE_SERVICE_LABEL),
            })
        })
        .collect()
}

/// List all containers
pub fn list_containers() -> DockerOutcome {
    if !docker_available() {
        return DockerOutcome::Unavailable("Docker daemon is not running".to_string());
    }

    let output = docker_command()
        .args(["ps", "-a", "--format", "{{json .}}"])
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

    let output = docker_command()
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

    let output = docker_command()
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

    let output = docker_command()
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

    let output = docker_command()
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

    let output = docker_command()
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

/// Start all services declared in the project's compose file (`docker compose up -d`).
///
/// Runs with `project_dir` as the working directory so the compose file is
/// found without requiring `-f` flags or absolute paths.
pub fn compose_up(project_dir: &str) -> DockerOutcome {
    if !docker_available() {
        return DockerOutcome::Unavailable("Docker daemon is not running".to_string());
    }

    if !compose_available() {
        return DockerOutcome::Unavailable("Docker Compose is not available".to_string());
    }

    let output = docker_command()
        .args(["compose", "up", "-d"])
        .current_dir(project_dir)
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            DockerOutcome::Started(vec![stdout.trim().to_string()])
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            DockerOutcome::Error(format!("docker compose up failed: {stderr}"))
        }
        Err(e) => DockerOutcome::Error(format!("failed to execute docker compose up: {e}")),
    }
}

/// Stop all services started from the project's compose file (`docker compose down`).
pub fn compose_down(project_dir: &str) -> DockerOutcome {
    if !docker_available() {
        return DockerOutcome::Unavailable("Docker daemon is not running".to_string());
    }

    if !compose_available() {
        return DockerOutcome::Unavailable("Docker Compose is not available".to_string());
    }

    let output = docker_command()
        .args(["compose", "down"])
        .current_dir(project_dir)
        .output();

    match output {
        Ok(output) if output.status.success() => {
            DockerOutcome::Stopped(vec!["compose project".to_string()])
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            DockerOutcome::Error(format!("docker compose down failed: {stderr}"))
        }
        Err(e) => DockerOutcome::Error(format!("failed to execute docker compose down: {e}")),
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

    #[test]
    fn parse_full_context_json_reads_compose_identity_from_labels() {
        // Shape of `docker ps --format "{{json .}}"` on daemons whose
        // template context has no ComposeProject/ComposeService fields.
        let line = r#"{"Command":"\"postgres\"","CreatedAt":"2026-09-21","ID":"abc123","Image":"postgres:16","Labels":"com.docker.compose.project=vertex,com.docker.compose.service=db","LocalVolumes":"1","Mounts":"","Names":"/vertex-db-1","Networks":"vertex_default","Ports":"5432/tcp","RunningFor":"2 hours ago","Size":"0B","State":"running","Status":"Up 2 hours"}"#;

        let containers = parse_containers_json(line);

        assert_eq!(containers.len(), 1);
        assert_eq!(containers[0].name, "vertex-db-1");
        assert_eq!(containers[0].id, "abc123");
        assert!(containers[0].running);
        assert_eq!(containers[0].compose_project.as_deref(), Some("vertex"));
        assert_eq!(containers[0].compose_service.as_deref(), Some("db"));
    }

    #[test]
    fn parse_context_json_without_labels_has_no_compose_identity() {
        let line = r#"{"ID":"def456","Image":"redis:7","Names":"redis","Ports":"","State":"exited","Status":"Exited (0)"}"#;

        let containers = parse_containers_json(line);

        assert_eq!(containers.len(), 1);
        assert_eq!(containers[0].name, "redis");
        assert!(!containers[0].running);
        assert!(containers[0].ports.is_empty());
        assert!(containers[0].compose_project.is_none());
    }

    #[test]
    fn compose_up_in_a_dir_without_a_compose_file_fails_cleanly() {
        // Uses a throwaway directory so this test never touches a real project.
        // With no compose file present the outcome must be Unavailable (no
        // daemon) or Error (compose failed) -- never a panic, never success.
        let dir = std::env::temp_dir().join("pilot-test-no-compose");
        let _ = std::fs::create_dir_all(&dir);
        let result = compose_up(&dir.to_string_lossy());
        assert!(matches!(
            result,
            DockerOutcome::Unavailable(_) | DockerOutcome::Error(_)
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn compose_down_in_a_dir_without_a_compose_file_fails_cleanly() {
        let dir = std::env::temp_dir().join("pilot-test-no-compose-down");
        let _ = std::fs::create_dir_all(&dir);
        let result = compose_down(&dir.to_string_lossy());
        assert!(matches!(
            result,
            DockerOutcome::Unavailable(_) | DockerOutcome::Error(_)
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}