//! Project service status derived from what Pilot can actually observe.
//!
//! Pilot reports a service as running only when something is really listening on
//! the port the project declares (see "Pilot Prerequisite.md" section 17: do not
//! claim certainty when evidence is insufficient). Process-level state arrives
//! with the process lifecycle phase.

use pilot_core::ProjectModel;
use pilot_port_manager::inspect_port;
use serde::Serialize;

/// Observed state of a service
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceState {
    /// Something is listening on the service port
    Running,
    /// Nothing is listening on the service port
    Stopped,
    /// The state cannot be observed by Pilot yet
    Unknown,
}

/// Status of a single detected service
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceStatus {
    /// Stable key used by the GUI (`frontend`, `backend`, `database`, `docker`)
    pub key: String,
    /// Human-readable service label
    pub label: String,
    /// Port the service is expected to use, when known
    pub port: Option<u16>,
    /// Observed state
    pub state: ServiceState,
    /// Evidence explaining the state
    pub detail: String,
}

impl ServiceStatus {
    /// Build a status by probing the TCP port the project declares
    pub fn from_port(key: &str, label: &str, port: u16) -> Self {
        let status = inspect_port(port);
        let running = !status.available;

        ServiceStatus {
            key: key.to_string(),
            label: label.to_string(),
            port: Some(port),
            state: if running {
                ServiceState::Running
            } else {
                ServiceState::Stopped
            },
            detail: if running {
                format!("port {port} is accepting connections")
            } else {
                format!("nothing is listening on port {port}")
            },
        }
    }

    /// Build a status for something Pilot cannot observe yet
    pub fn unknown(key: &str, label: &str, detail: &str) -> Self {
        ServiceStatus {
            key: key.to_string(),
            label: label.to_string(),
            port: None,
            state: ServiceState::Unknown,
            detail: detail.to_string(),
        }
    }
}

/// Build the service list for a detected project
///
/// Only services that were actually detected are reported, which is what makes
/// the GUI menu dynamic.
pub fn build_status(model: &ProjectModel) -> Vec<ServiceStatus> {
    let mut services = Vec::new();

    if let Some(frontend) = &model.frontend {
        services.push(ServiceStatus::from_port(
            "frontend",
            "Frontend",
            frontend.port,
        ));
    }

    if let Some(backend) = &model.backend {
        services.push(ServiceStatus::from_port("backend", "Backend", backend.port));
    }

    if let Some(database) = &model.database {
        services.push(ServiceStatus::from_port(
            "database",
            "Database",
            database.port,
        ));
    }

    if model.docker.is_some_and(|docker| docker.detected) {
        services.push(ServiceStatus::unknown(
            "docker",
            "Docker",
            "container state requires the Docker integration (phase 5)",
        ));
    }

    services
}

#[cfg(test)]
mod tests {
    use super::*;
    use pilot_core::{DatabaseInfo, DockerInfo, FrontendInfo};
    use std::net::{Ipv4Addr, TcpListener};

    /// A port that is currently occupied by a listener held by the test
    fn occupied_port() -> (TcpListener, u16) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("must bind");
        let port = listener.local_addr().expect("must have an address").port();

        (listener, port)
    }

    #[test]
    fn a_service_is_running_when_its_port_is_listening() {
        let (_listener, port) = occupied_port();

        let status = ServiceStatus::from_port("frontend", "Frontend", port);

        assert_eq!(status.state, ServiceState::Running);
        assert_eq!(status.port, Some(port));
        assert!(status.detail.contains("accepting connections"));
    }

    #[test]
    fn a_service_is_stopped_when_its_port_is_released() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("must bind");
        let port = listener.local_addr().expect("must have an address").port();
        drop(listener);

        let status = ServiceStatus::from_port("backend", "Backend", port);

        assert_eq!(status.state, ServiceState::Stopped);
        assert!(status.detail.contains("nothing is listening"));
    }

    #[test]
    fn only_detected_services_are_reported() {
        let mut model = ProjectModel::new("demo", ".");
        model.frontend = Some(FrontendInfo::new("nextjs", 3000));

        let services = build_status(&model);

        assert_eq!(services.len(), 1);
        assert_eq!(services[0].key, "frontend");
    }

    #[test]
    fn docker_is_reported_as_unknown_with_an_explanation() {
        let mut model = ProjectModel::new("demo", ".");
        model.database = Some(DatabaseInfo::new("postgresql", 5432));
        model.docker = Some(DockerInfo::new(true, true));

        let services = build_status(&model);

        let docker = services
            .iter()
            .find(|service| service.key == "docker")
            .expect("docker must be reported");

        assert_eq!(docker.state, ServiceState::Unknown);
        assert!(docker.port.is_none());
        assert!(docker.detail.contains("phase 5"));
    }

    #[test]
    fn status_serializes_with_camel_case_state_names() {
        let status = ServiceStatus::unknown("docker", "Docker", "not observable yet");
        let json = serde_json::to_value(&status).expect("status must serialize");

        assert_eq!(json["key"], "docker");
        assert_eq!(json["state"], "unknown");
    }
}