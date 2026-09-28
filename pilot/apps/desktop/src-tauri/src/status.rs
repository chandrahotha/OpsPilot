//! Project service status derived from what Pilot can actually observe.
//!
//! Pilot reports a service as running only when something is really listening on
//! the port the project declares — never claiming certainty when evidence is
//! insufficient. Process-level state arrives with the process lifecycle phase.

use pilot_core::ProjectModel;
use pilot_port_manager::inspect_port;
use pilot_process_manager::{ProcessManager, ProcessOutcome, ProcessState, build_startup_plan};
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
    /// PID of the process holding the service port, when the port is occupied
    /// and the owner could be identified
    pub owner_pid: Option<u32>,
    /// Name of the process holding the service port, when identified
    pub owner_name: Option<String>,
    /// True when Pilot's own process manager started the running process.
    /// False means the port is held by something Pilot did not start (a
    /// terminal, an IDE, or a previous session).
    pub pilot_started: bool,
}

/// Human-readable PID for messages: `1234`, never `Some(1234)`.
fn fmt_pid(pid: Option<u32>) -> String {
    pid.map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Detail line for a port that is accepting connections, naming the owner
/// so the pilot knows who is flying.
fn running_detail(port: u16, pid: Option<u32>, name: Option<&str>) -> String {
    match (pid, name) {
        (Some(pid), Some(name)) => format!(
            "port {port} is accepting connections; held by {name} (pid {pid}), which Pilot did not start"
        ),
        _ => format!("port {port} is accepting connections"),
    }
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
                running_detail(port, status.pid, status.process.as_deref())
            } else {
                format!("nothing is listening on port {port}")
            },
            owner_pid: if running { status.pid } else { None },
            owner_name: if running { status.process } else { None },
            pilot_started: false,
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
            owner_pid: None,
            owner_name: None,
            pilot_started: false,
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
    } else if model.docker.is_some_and(|docker| docker.compose) {
        // Fallback: Docker Compose detected but no explicit database config found.
        // This can happen when the database runs in Docker Compose but the project
        // lacks traditional config files (Prisma, Django, Alembic).
        // We report it as Unknown since we can't probe the port without knowing it.
        services.push(ServiceStatus::unknown(
            "database",
            "Database",
            "detected via Docker Compose (port unknown, so the state is not observable)",
        ));
    }

    if model.docker.is_some_and(|docker| docker.detected) {
        services.push(ServiceStatus::unknown(
            "docker",
            "Docker",
            "Docker detected; container state is not observable via ports",
        ));
    }

    // Startup steps without a port-observable service (e.g. the generic
    // "app" step for a declared run script with no detected framework)
    // still need a card in the GUI. Without this, get_status returns an
    // empty list while a startup step exists, so the user has no Start
    // button to click even though the project is startable.
    let plan = build_startup_plan(model, &model.project.path);
    for step in &plan.steps {
        if !services.iter().any(|service| service.key == step.service) {
            services.push(ServiceStatus::unknown(
                &step.service,
                &display_label(&step.service),
                &format!(
                    "{}; no port declared so the state is not observable yet",
                    step.description
                ),
            ));
        }
    }

    // A service Pilot itself started counts as running even when its
    // declared port is not accepting connections yet (still booting, or the
    // project listens on a different port than the scanner detected).
    // Without this, Start stays enabled after a successful start and invites
    // duplicate-start errors.
    overlay_tracked_state(&mut services, &model.project.path);

    services
}

/// Overlay Pilot's tracked-process state onto port-observed statuses.
///
/// Scoped by working directory so a same-named service started for another
/// project is never misattributed to this one.
fn overlay_tracked_state(services: &mut [ServiceStatus], project_path: &str) {
    let manager = crate::get_process_manager();

    for service in services.iter_mut() {
        if !matches!(service.key.as_str(), "frontend" | "backend" | "app") {
            continue;
        }

        let ProcessOutcome::Snapshot(snapshot) = manager.status(&service.key) else {
            continue;
        };

        if !crate::path_belongs_to_project(&snapshot.working_directory, project_path) {
            continue;
        }

        match snapshot.state {
            ProcessState::Running => {
                service.pilot_started = true;
                let pid = fmt_pid(snapshot.pid);
                match service.port {
                    // The port is up and it is Pilot's own process: the
                    // normal, healthy state after Start.
                    Some(port) if service.state == ServiceState::Running => {
                        let actual_pid = service
                            .owner_pid
                            .map(|p| p.to_string())
                            .unwrap_or_else(|| pid.clone());
                        service.detail = format!(
                            "started by Pilot ({actual_pid}); port {port} is accepting connections"
                        );
                    }
                    // Tracked process is alive but the port is not accepting
                    // yet (still booting, or a different port was detected).
                    Some(port) => {
                        service.state = ServiceState::Running;
                        service.detail = format!(
                            "started by Pilot ({pid}); port {port} is not accepting connections yet"
                        );
                    }
                    None => {
                        service.state = ServiceState::Running;
                        service.detail = format!(
                            "started by Pilot ({pid}); no port declared so the state comes from the process"
                        );
                    }
                }
            }
            ProcessState::Exited | ProcessState::Failed => {
                if service.state == ServiceState::Stopped {
                    service.detail = format!(
                        "{}; Pilot-started process ended ({})",
                        service.detail, snapshot.detail
                    );
                }
            }
            ProcessState::Stopped => {}
        }
    }

    // Also check if any port-observed service is owned by Pilot through child processes
    for service in services.iter_mut() {
        if let Some(port_pid) = service.owner_pid {
            if manager.contains_pid(&service.key, port_pid) {
                service.pilot_started = true;
                if let Some(port) = service.port {
                    service.detail = format!(
                        "started by Pilot ({port_pid}); port {port} is accepting connections"
                    );
                }
            } else if let Some(owner_snapshot) = manager.any_contains_pid(port_pid)
                && crate::path_belongs_to_project(&owner_snapshot.working_directory, project_path)
            {
                service.pilot_started = true;
                if let Some(port) = service.port {
                    service.detail = format!(
                        "started by Pilot ({port_pid}); port {port} is accepting connections"
                    );
                }
            }
        }
    }
}

/// Human-readable label for a startup-step service key ("app" -> "App").
fn display_label(service: &str) -> String {
    let mut chars = service.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => service.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pilot_core::{DatabaseInfo, DockerInfo, FrontendInfo};
    use std::net::{Ipv4Addr, TcpListener};
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use std::time::Duration;

    /// Port tests bind and release real ports. Running them in parallel would let
    /// one test take a port another test just released, so they are serialized.
    fn port_lock() -> MutexGuard<'static, ()> {
        static PORT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

        PORT_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Wait until the operating system has actually released a port
    fn wait_until_free(port: u16) -> bool {
        for _ in 0..50 {
            if pilot_port_manager_free(port) {
                return true;
            }

            std::thread::sleep(Duration::from_millis(10));
        }

        false
    }

    /// Reuse the port manager's own read-only inspection for the wait loop
    fn pilot_port_manager_free(port: u16) -> bool {
        ServiceStatus::from_port("test", "Test", port).state == ServiceState::Stopped
    }

    /// A port that is currently occupied by a listener held by the test
    fn occupied_port() -> (TcpListener, u16) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("must bind");
        let port = listener.local_addr().expect("must have an address").port();

        (listener, port)
    }

    #[test]
    fn a_service_is_running_when_its_port_is_listening() {
        let _lock = port_lock();
        let (_listener, port) = occupied_port();

        let status = ServiceStatus::from_port("frontend", "Frontend", port);

        assert_eq!(status.state, ServiceState::Running);
        assert_eq!(status.port, Some(port));
        assert!(status.detail.contains("accepting connections"));
    }

    #[test]
    fn a_service_is_stopped_when_its_port_is_released() {
        let _lock = port_lock();

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("must bind");
        let port = listener.local_addr().expect("must have an address").port();
        drop(listener);

        assert!(
            wait_until_free(port),
            "port {port} should be free again after the listener is dropped"
        );

        let status = ServiceStatus::from_port("backend", "Backend", port);

        assert_eq!(status.state, ServiceState::Stopped);
        assert!(status.detail.contains("nothing is listening"));
        assert!(!status.pilot_started);
        assert!(status.owner_pid.is_none());
        assert!(status.owner_name.is_none());
    }

    #[test]
    fn running_detail_names_the_process_holding_the_port() {
        let with_owner = running_detail(3000, Some(4242), Some("node.exe"));
        assert!(with_owner.contains("held by node.exe (pid 4242)"));
        assert!(with_owner.contains("which Pilot did not start"));
        assert!(with_owner.contains("accepting connections"));

        let without_owner = running_detail(5432, None, None);
        assert!(without_owner.contains("accepting connections"));
        assert!(!without_owner.contains("held by"));
    }

    #[test]
    fn an_external_listener_is_running_but_not_pilot_started() {
        let _lock = port_lock();
        let (_listener, port) = occupied_port();

        let mut model = ProjectModel::new("demo", ".");
        model.frontend = Some(FrontendInfo::new("vite", port));

        let services = build_status(&model);
        let frontend = services
            .iter()
            .find(|service| service.key == "frontend")
            .expect("frontend must be reported");

        // Something is listening, but Pilot did not start it: the pilot must
        // be told that the flight is being flown by someone else.
        assert_eq!(frontend.state, ServiceState::Running);
        assert!(!frontend.pilot_started);
        assert!(frontend.detail.contains("accepting connections"));
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
        assert!(docker.detail.contains("not observable"));
    }

    #[test]
    fn status_serializes_with_camel_case_state_names() {
        let status = ServiceStatus::unknown("docker", "Docker", "not observable yet");
        let json = serde_json::to_value(&status).expect("status must serialize");

        assert_eq!(json["key"], "docker");
        assert_eq!(json["state"], "unknown");
        assert_eq!(json["pilotStarted"], false);
        assert_eq!(json["ownerPid"], serde_json::Value::Null);
        assert_eq!(json["ownerName"], serde_json::Value::Null);
    }

    /// A command that sleeps ~30s on any platform, so the test can observe
    /// the Running state before stopping it again.
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

    #[test]
    fn a_pilot_started_process_overrides_a_closed_port() {
        use pilot_process_manager::{ProcessManager, ProcessRequest};

        let _lock = port_lock();

        // A port that is currently free.
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("must bind");
        let port = listener.local_addr().expect("must have an address").port();
        drop(listener);
        assert!(
            wait_until_free(port),
            "port {port} should be free again after the listener is dropped"
        );

        let mut model = ProjectModel::new("demo", ".");
        model.frontend = Some(FrontendInfo::new("vite", port));

        let manager = crate::get_process_manager();
        // Clean up a leftover from an earlier aborted run, if any.
        let _ = manager.stop("frontend");
        let request = ProcessRequest::new("frontend", sleeper_command(), ".");
        assert!(
            matches!(
                manager.start(&request),
                pilot_process_manager::ProcessOutcome::Started(_)
            ),
            "the sleeper process must start"
        );

        let services = build_status(&model);
        let frontend = services
            .iter()
            .find(|service| service.key == "frontend")
            .expect("frontend must be reported");

        assert_eq!(frontend.state, ServiceState::Running);
        assert!(frontend.pilot_started);
        assert!(frontend.detail.contains("started by Pilot"));
        assert!(
            !frontend.detail.contains("pid Some("),
            "no Rust debug syntax in the UI: {}",
            frontend.detail
        );

        let _ = manager.stop("frontend");
    }

    #[test]
    fn tracked_state_from_another_project_is_not_attributed() {
        use pilot_process_manager::{ProcessManager, ProcessRequest};

        let _lock = port_lock();

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("must bind");
        let port = listener.local_addr().expect("must have an address").port();
        drop(listener);
        assert!(
            wait_until_free(port),
            "port {port} should be free again after the listener is dropped"
        );

        let manager = crate::get_process_manager();
        let _ = manager.stop("backend");
        let request = ProcessRequest::new("backend", sleeper_command(), ".");
        assert!(
            matches!(
                manager.start(&request),
                pilot_process_manager::ProcessOutcome::Started(_)
            ),
            "the sleeper process must start"
        );

        // Same label, different project directory: the overlay must not apply.
        let mut other = ProjectModel::new("other", "some/other/project");
        other.backend = Some(pilot_core::BackendInfo::new("express", port));
        let services = build_status(&other);
        let backend = services
            .iter()
            .find(|service| service.key == "backend")
            .expect("backend must be reported");

        assert_eq!(backend.state, ServiceState::Stopped);
        assert!(!backend.pilot_started);
        assert!(!backend.detail.contains("started by Pilot"));

        let _ = manager.stop("backend");
    }

    #[test]
    fn a_run_script_without_a_framework_still_gets_a_service_card() {
        use pilot_core::CommandInfo;

        let mut model = ProjectModel::new("demo", ".");
        model.commands.push(CommandInfo::new(
            "start",
            "node server.js",
            "package.json scripts",
        ));

        let services = build_status(&model);

        let app = services
            .iter()
            .find(|service| service.key == "app")
            .expect("app step must produce a service card");
        assert_eq!(app.label, "App");
        assert_eq!(app.state, ServiceState::Unknown);
    }
}
