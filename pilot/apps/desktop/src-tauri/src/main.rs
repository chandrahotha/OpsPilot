//! OpsPilot desktop shell.
//!
//! The window is a thin front end over the Pilot engine: every command delegates
//! to the scanner, port manager or diagnostics engine, and none of them runs a
//! project command. The GUI renders whatever the engine detected, which is what
//! makes the menu dynamic per project.

#![windows_subsystem = "windows"]

mod status;

use pilot_core::ProjectModel;
use pilot_database::{
    DatabaseIntegration, DatabaseManager, DatabaseOperation, DatabaseOutcome, DatabaseType,
    IntegrationType,
};
use pilot_diagnostics::{DiagnosticsReport, run_diagnostics as run_engine_diagnostics};
use pilot_docker::{DockerOutcome, compose_down, compose_up, execute as execute_docker};
use pilot_docker::DockerOperation;
use pilot_process_manager::{
    LocalProcessManager, LogEntry, ProcessManager, ProcessOutcome, ProcessRequest, ProcessState,
    build_startup_plan, validate_plan_ports, validate_step,
};
use pilot_scanner::scan_project;
use status::{ServiceStatus, build_status};
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::command;

/// Environment variable used by the launcher to open a specific project
const PROJECT_PATH_ENV: &str = "PILOT_PROJECT_PATH";

/// Resolve the directory Pilot should operate on.
///
/// Order: explicit path (from the launcher) > PILOT_PROJECT_PATH > current
/// directory, so cd my-project && pilot opens that project.
fn resolve_project_path(path: Option<String>) -> Result<String, String> {
    let requested = path
        .filter(|value| !value.trim().is_empty())
        .or_else(|| env::var(PROJECT_PATH_ENV).ok().filter(|value| !value.trim().is_empty()));

    let candidate = match requested {
        Some(value) => PathBuf::from(value),
        None => env::current_dir()
            .map_err(|error| format!("could not read the current directory: {error}"))?,
    };

    if !candidate.is_dir() {
        return Err(format!("{} is not a directory", candidate.display()));
    }

    // Normalize so the dashboard shows a real project name and path instead of ..
    pilot_scanner::resolve_project_path(&candidate.to_string_lossy())
        .map_err(|error| format!("could not resolve {}: {error}", candidate.display()))
}

/// Scan a project directory and return the normalized project model
#[command]
fn detect_project(path: Option<String>) -> Result<pilot_core::ScanResult, String> {
    let path = resolve_project_path(path)?;
    Ok(scan_project(path))
}

/// Report the observed state of every detected service
#[command]
fn get_status(path: Option<String>) -> Result<Vec<ServiceStatus>, String> {
    let path = resolve_project_path(path)?;
    let scan = scan_project(&path);
    Ok(match scan.model() {
        Some(model) => build_status(model),
        None => Vec::new(),
    })
}

/// Run the deterministic diagnostics for a project
#[command]
fn run_diagnostics(path: Option<String>) -> Result<DiagnosticsReport, String> {
    let path = resolve_project_path(path)?;
    let scan = scan_project(&path);
    let model = scan.model().cloned().unwrap_or_else(|| {
        ProjectModel::new(pilot_scanner::project_name_from_path(&path), &path)
    });
    Ok(run_engine_diagnostics(&path, &model))
}

/// Get the startup plan for a project (what can be started)
#[command]
fn get_startup_plan(path: Option<String>) -> Result<StartupPlanResponse, String> {
    let path = resolve_project_path(path)?;
    let scan = scan_project(&path);
    let model = scan.model().cloned().unwrap_or_else(|| {
        ProjectModel::new(pilot_scanner::project_name_from_path(&path), &path)
    });
    let plan = build_startup_plan(&model, &path);
    let steps: Vec<StartupStepResponse> = plan.steps.iter().map(|s| StartupStepResponse {
        service: s.service.clone(),
        description: s.description.clone(),
        command: s.command.clone(),
        working_directory: s.working_directory.clone(),
    }).collect();
    Ok(StartupPlanResponse {
        executable: plan.executable(),
        steps,
        warnings: plan.warnings,
    })
}

/// Start a project service (process lifecycle - phase 4)
///
/// The `service` argument must be a service *key* (`frontend`, `backend`,
/// `app`) as reported by `get_status`, not the human-readable label.
/// On failure the error names the service, the attempted command, the
/// working directory and the plan warnings, so the GUI can show the user
/// exactly why nothing started instead of failing silently.
#[command]
fn start_project(path: Option<String>, service: String) -> Result<String, String> {
    let path = resolve_project_path(path)?;
    let scan = scan_project(&path);
    let model = scan.model().cloned().unwrap_or_else(|| {
        ProjectModel::new(pilot_scanner::project_name_from_path(&path), &path)
    });
    let plan = build_startup_plan(&model, &path);
    let step = plan.steps.iter().find(|s| s.service == service)
        .ok_or_else(|| {
            let mut message = format!("No startup step found for service '{service}'");
            if plan.steps.is_empty() {
                message.push_str("; the startup plan has no executable steps");
            } else {
                let available: Vec<&str> = plan.steps.iter().map(|s| s.service.as_str()).collect();
                message.push_str(&format!("; available services: {}", available.join(", ")));
            }
            if !plan.warnings.is_empty() {
                message.push_str(&format!("; plan notes: {}", plan.warnings.join(" | ")));
            }
            message
        })?;
    let blockers = validate_step(step);
    if !blockers.is_empty() {
        return Err(format!(
            "Cannot start {} (command `{}` in `{}`): {}",
            service,
            step.command,
            step.working_directory,
            blockers.join(", ")
        ));
    }
    let request = ProcessRequest::new(&step.service, &step.command, &step.working_directory);
    let manager = get_process_manager();
    // Scope the duplicate guard to this project: the same label running for
    // a *different* project must name that project instead of confusingly
    // claiming "already running".
    if let ProcessOutcome::Snapshot(existing) = manager.status(&step.service) {
        if existing.state == ProcessState::Running
            && !existing.working_directory.eq_ignore_ascii_case(&path)
        {
            return Err(format!(
                "{} is already running for another project ({}) with pid {:?}; stop it there first",
                service, existing.working_directory, existing.pid
            ));
        }
    }
    match manager.start(&request) {
        ProcessOutcome::Started(snapshot) => Ok(format!("Started {} (pid {:?})", snapshot.label, snapshot.pid)),
        ProcessOutcome::Error(e) => Err(format!(
            "Failed to start {} (command `{}` in `{}`): {e}",
            service, step.command, step.working_directory
        )),
        _ => Err("Unexpected outcome".to_string()),
    }
}

/// Stop a project service (process lifecycle - phase 4)
#[command]
fn stop_project(service: String) -> Result<String, String> {
    let manager = get_process_manager();
    match manager.stop(&service) {
        ProcessOutcome::Stopped(snapshot) => Ok(format!("Stopped {} ({})", snapshot.label, snapshot.detail)),
        ProcessOutcome::NotFound(label) => Err(format!("Service {} not found or not started by Pilot", label)),
        ProcessOutcome::Error(e) => Err(e),
        _ => Err("Unexpected outcome".to_string()),
    }
}

/// Restart a project service (process lifecycle - phase 4)
#[command]
fn restart_project(service: String) -> Result<String, String> {
    let manager = get_process_manager();
    match manager.restart(&service) {
        ProcessOutcome::Started(snapshot) => Ok(format!("Restarted {} (pid {:?})", snapshot.label, snapshot.pid)),
        ProcessOutcome::NotFound(label) => Err(format!("Service {} not found or not started by Pilot", label)),
        ProcessOutcome::Error(e) => Err(e),
        _ => Err("Unexpected outcome".to_string()),
    }
}

/// Get process status for a service
#[command]
fn get_process_status(service: String) -> Result<ProcessSnapshotResponse, String> {
    let manager = get_process_manager();
    match manager.status(&service) {
        ProcessOutcome::Snapshot(snapshot) => Ok(ProcessSnapshotResponse {
            label: snapshot.label,
            command: snapshot.command,
            working_directory: snapshot.working_directory,
            pid: snapshot.pid,
            state: format!("{:?}", snapshot.state),
            detail: snapshot.detail,
            exit_code: snapshot.exit_code,
            started_at_ms: snapshot.started_at_ms,
        }),
        ProcessOutcome::NotFound(label) => Err(format!("Service {} not found", label)),
        ProcessOutcome::Error(e) => Err(e),
        _ => Err("Unexpected outcome".to_string()),
    }
}

/// List all tracked processes
#[command]
fn list_processes() -> Result<Vec<ProcessSnapshotResponse>, String> {
    let manager = get_process_manager();
    Ok(manager.list().into_iter().map(|s| ProcessSnapshotResponse {
        label: s.label,
        command: s.command,
        working_directory: s.working_directory,
        pid: s.pid,
        state: format!("{:?}", s.state),
        detail: s.detail,
        exit_code: s.exit_code,
        started_at_ms: s.started_at_ms,
    }).collect())
}

/// Docker daemon status and CLI version (read-only probe).
#[command]
fn docker_status() -> Result<DockerOutcome, String> {
    Ok(execute_docker(DockerOperation::CheckStatus))
}

/// List all Docker containers visible to the daemon.
#[command]
fn list_docker_containers() -> Result<DockerOutcome, String> {
    Ok(execute_docker(DockerOperation::ListContainers))
}

/// Run a Docker container action: `start`, `stop`, `restart`, `logs` or `rebuild`.
#[command]
fn docker_container_action(name: String, action: String) -> Result<DockerOutcome, String> {
    let operation = match action.as_str() {
        "start" => DockerOperation::StartContainer(name),
        "stop" => DockerOperation::StopContainer(name),
        "restart" => DockerOperation::RestartContainer(name),
        "logs" => DockerOperation::Logs(name),
        "rebuild" => DockerOperation::RebuildImage(name),
        other => {
            return Err(format!(
                "unknown docker action '{other}' (expected start, stop, restart, logs or rebuild)"
            ));
        }
    };
    Ok(execute_docker(operation))
}

/// Bring the project's compose stack up (`up`) or down (`down`).
#[command]
fn docker_compose(path: Option<String>, action: String) -> Result<DockerOutcome, String> {
    let path = resolve_project_path(path)?;
    match action.as_str() {
        "up" => Ok(compose_up(&path)),
        "down" => Ok(compose_down(&path)),
        other => Err(format!(
            "unknown compose action '{other}' (expected up or down)"
        )),
    }
}

/// Parse a database operation name from the GUI.
fn parse_database_operation(operation: &str) -> Result<DatabaseOperation, String> {
    match operation {
        "status" => Ok(DatabaseOperation::Status),
        "migrate" => Ok(DatabaseOperation::Migrate),
        "seed" => Ok(DatabaseOperation::Seed),
        "reset" => Ok(DatabaseOperation::Reset),
        "backup" => Ok(DatabaseOperation::Backup),
        "restore" => Ok(DatabaseOperation::Restore),
        other => Err(format!(
            "unknown database operation '{other}' (expected status, migrate, seed, reset, backup or restore)"
        )),
    }
}

/// Build the database integration for a scanned project.
///
/// The ORM selects the integration (Prisma/Django/Alembic); without an ORM,
/// a PostgreSQL database falls back to raw `psql`. Anything else reports
/// `NotImplemented` with the reason instead of guessing commands.
fn database_integration_for(model: &ProjectModel, path: &str) -> Result<DatabaseIntegration, DatabaseOutcome> {
    let orm = model.orm.as_ref().map(|orm| orm.r#type.to_lowercase());
    let integration_type = match orm.as_deref() {
        Some("prisma") => IntegrationType::Prisma,
        Some("django") => IntegrationType::Django,
        Some("alembic") => IntegrationType::Alembic,
        _ => {
            let db_type = model.database.as_ref().map(|db| db.r#type.to_lowercase());
            match db_type.as_deref() {
                Some("postgresql") | Some("postgres") => IntegrationType::Postgres,
                Some(other) => {
                    return Err(DatabaseOutcome::NotImplemented {
                        reason: format!(
                            "no migration integration for database type '{other}' (supported: prisma, django, alembic, postgresql)"
                        ),
                    });
                }
                None => {
                    return Err(DatabaseOutcome::NotImplemented {
                        reason: "no ORM or database was detected in this project".to_string(),
                    });
                }
            }
        }
    };

    let database = model.database.as_ref().and_then(|db| {
        let db_type = match db.r#type.to_lowercase().as_str() {
            "postgresql" | "postgres" => DatabaseType::PostgreSQL,
            "mysql" | "mariadb" => DatabaseType::MySQL,
            "mongodb" | "mongo" => DatabaseType::MongoDB,
            "sqlite" => DatabaseType::SQLite,
            _ => return None,
        };
        Some(pilot_database::DatabaseInfo {
            r#type: db_type,
            host: "localhost".to_string(),
            port: db.port,
            name: model.project.name.clone(),
        })
    });

    Ok(DatabaseIntegration {
        integration_type,
        project_dir: path.to_string(),
        database,
        env: HashMap::new(),
    })
}

/// Run a database operation (migrate/seed/status/backup, or confirmed reset/restore).
///
/// Destructive operations (`reset`, `restore`) return `NeedsConfirmation`
/// unless `confirmed` is true, in which case the caller has already obtained
/// explicit user confirmation in the GUI.
#[command]
fn database_operation(
    path: Option<String>,
    operation: String,
    confirmed: bool,
) -> Result<DatabaseOutcome, String> {
    let path = resolve_project_path(path)?;
    let scan = scan_project(&path);
    let model = scan.model().cloned().unwrap_or_else(|| {
        ProjectModel::new(pilot_scanner::project_name_from_path(&path), &path)
    });
    let integration = database_integration_for(&model, &path).map_err(|outcome| {
        // Surface NotImplemented reasons as errors the GUI can display.
        match outcome {
            DatabaseOutcome::NotImplemented { reason } => reason,
            _ => "could not determine the database integration".to_string(),
        }
    })?;
    let operation = parse_database_operation(&operation)?;
    let manager = DatabaseManager::new();
    if confirmed {
        Ok(manager.execute_confirmed(&integration, operation))
    } else {
        Ok(manager.execute(&integration, operation))
    }
}

/// Captured stdout/stderr lines for a service started by Pilot.
///
/// Returns an empty list when the service was never started by Pilot.
/// `limit` caps the number of trailing lines (defaults to 200).
#[command]
fn get_service_logs(service: String, limit: Option<usize>) -> Result<Vec<LogEntry>, String> {
    let manager = get_process_manager();
    Ok(manager
        .log_buffer(&service)
        .map(|buffer| buffer.snapshot(limit.or(Some(200))))
        .unwrap_or_default())
}

/// One execution-readiness row in the system health report.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ReadinessEntry {
    service: String,
    command: String,
    ready: bool,
    detail: String,
}

/// OpsPilot's own health plus the current project's execution readiness.
///
/// This is the machine-readable backing for the System Diagnostics panel:
/// every subsystem reports its state, and every startup step reports
/// whether it can run right now and why not.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemHealth {
    frontend: bool,
    tauri_bridge: bool,
    rust_backend: bool,
    project_scanner: bool,
    project_detected: bool,
    project_name: Option<String>,
    project_path: Option<String>,
    process_manager: bool,
    tracked_processes: usize,
    docker_available: bool,
    docker_version: Option<String>,
    compose_available: bool,
    readiness: Vec<ReadinessEntry>,
    warnings: Vec<String>,
}

/// Report OpsPilot subsystem health and execution readiness for a project.
#[command]
fn system_health(path: Option<String>) -> Result<SystemHealth, String> {
    let path = resolve_project_path(path)?;
    let scan = scan_project(&path);
    let detected = scan.detected;
    let model = scan.model().cloned().unwrap_or_else(|| {
        ProjectModel::new(pilot_scanner::project_name_from_path(&path), &path)
    });

    let manager = get_process_manager();
    let tracked_processes = manager.list().len();

    let plan = build_startup_plan(&model, &path);
    let mut readiness: Vec<ReadinessEntry> = plan
        .steps
        .iter()
        .map(|step| {
            let blockers = validate_step(step);
            ReadinessEntry {
                service: step.service.clone(),
                command: step.command.clone(),
                ready: blockers.is_empty(),
                detail: if blockers.is_empty() {
                    format!("{} (working directory `{}`)", step.description, step.working_directory)
                } else {
                    blockers.join("; ")
                },
            }
        })
        .collect();
    if model.docker.is_some() {
        readiness.push(ReadinessEntry {
            service: "docker".to_string(),
            command: "docker compose up -d".to_string(),
            ready: pilot_docker::docker_available() && pilot_docker::compose_available(),
            detail: if pilot_docker::docker_available() {
                if pilot_docker::compose_available() {
                    "compose stack can be started".to_string()
                } else {
                    "docker daemon is running but compose is not available".to_string()
                }
            } else {
                "docker daemon is not running".to_string()
            },
        });
    }

    Ok(SystemHealth {
        frontend: true,
        tauri_bridge: true,
        rust_backend: true,
        project_scanner: true,
        project_detected: detected,
        project_name: Some(model.project.name.clone()),
        project_path: Some(model.project.path.clone()),
        process_manager: true,
        tracked_processes,
        docker_available: pilot_docker::docker_available(),
        docker_version: pilot_docker::docker_version(),
        compose_available: pilot_docker::compose_available(),
        readiness,
        warnings: {
            let mut warnings = plan.warnings;
            // Surface port conflicts so the user knows before pressing Start
            // that a port is already occupied (possibly by their own service
            // running outside Pilot).
            warnings.extend(validate_plan_ports(&model));
            warnings
        },
    })
}

/// One-line summary of a Docker outcome for start-all/stop-all reports.
fn docker_summary(outcome: &DockerOutcome) -> String {
    match outcome {
        DockerOutcome::Status { available, version } => {
            if *available {
                format!("daemon running ({})", version.as_deref().unwrap_or("unknown version"))
            } else {
                "daemon not running".to_string()
            }
        }
        DockerOutcome::Containers(containers) => format!("{} container(s)", containers.len()),
        DockerOutcome::Started(names) => format!("started {}", names.join(", ")),
        DockerOutcome::Stopped(names) => format!("stopped {}", names.join(", ")),
        DockerOutcome::Restarted(names) => format!("restarted {}", names.join(", ")),
        DockerOutcome::Logs { container, .. } => format!("logs for {container}"),
        DockerOutcome::Rebuilt { image } => format!("rebuilt {image}"),
        DockerOutcome::Unavailable(reason) => format!("unavailable: {reason}"),
        DockerOutcome::Error(message) => format!("error: {message}"),
    }
}

/// Whether the compose stack for a project can be operated right now.
fn compose_ready() -> bool {
    pilot_docker::docker_available() && pilot_docker::compose_available()
}

/// Start everything the project requires, in order, with one call.
///
/// 1. Compose stack first (databases before servers) when the project has a
///    compose file and the daemon is available.
/// 2. Then every startup-plan step in order.
///
/// Already-running services are skipped, blocked steps are reported with
/// their reason — the result is a per-item summary, never a silent no-op.
#[command]
fn start_all_project(path: Option<String>) -> Result<String, String> {
    let path = resolve_project_path(path)?;
    let scan = scan_project(&path);
    let model = scan.model().cloned().unwrap_or_else(|| {
        ProjectModel::new(pilot_scanner::project_name_from_path(&path), &path)
    });
    let manager = get_process_manager();
    let mut lines: Vec<String> = Vec::new();

    if model.docker.is_some_and(|docker| docker.compose) {
        if compose_ready() {
            lines.push(format!("compose: {}", docker_summary(&compose_up(&path))));
        } else {
            lines.push("compose skipped: Docker daemon or compose is not available".to_string());
        }
    }

    let plan = build_startup_plan(&model, &path);
    if plan.steps.is_empty() {
        lines.push("no startable services; declare a dev/start/serve command".to_string());
    }

    for step in &plan.steps {
        if let ProcessOutcome::Snapshot(existing) = manager.status(&step.service) {
            if existing.state == ProcessState::Running
                && existing.working_directory.eq_ignore_ascii_case(&path)
            {
                lines.push(format!(
                    "{} already running (pid {:?}); skipped",
                    step.service, existing.pid
                ));
                continue;
            }
        }

        let blockers = validate_step(step);
        if !blockers.is_empty() {
            lines.push(format!("{} skipped: {}", step.service, blockers.join("; ")));
            continue;
        }

        let request = ProcessRequest::new(&step.service, &step.command, &step.working_directory);
        match manager.start(&request) {
            ProcessOutcome::Started(snapshot) => lines.push(format!(
                "started {} (pid {:?})",
                snapshot.label, snapshot.pid
            )),
            ProcessOutcome::Error(error) => {
                if error.contains("already running") {
                    lines.push(format!("{} skipped: {error}", step.service));
                } else {
                    lines.push(format!("{} failed: {error}", step.service));
                }
            }
            _ => lines.push(format!("{} failed: unexpected outcome", step.service)),
        }
    }

    Ok(lines.join("\n"))
}

/// Stop everything Pilot started for this project.
///
/// Only processes whose working directory is this project are touched, in
/// reverse label order; then the compose stack comes down when the project
/// has one and the daemon is available.
#[command]
fn stop_all_project(path: Option<String>) -> Result<String, String> {
    let path = resolve_project_path(path)?;
    let scan = scan_project(&path);
    let model = scan.model().cloned().unwrap_or_else(|| {
        ProjectModel::new(pilot_scanner::project_name_from_path(&path), &path)
    });
    let manager = get_process_manager();
    let mut lines: Vec<String> = Vec::new();

    let mut tracked: Vec<_> = manager
        .list()
        .into_iter()
        .filter(|snapshot| snapshot.working_directory.eq_ignore_ascii_case(&path))
        .collect();
    tracked.sort_by(|a, b| b.label.cmp(&a.label));

    if tracked.is_empty() {
        lines.push("nothing tracked for this project".to_string());
    }

    for snapshot in &tracked {
        match manager.stop(&snapshot.label) {
            ProcessOutcome::Stopped(stopped) => {
                lines.push(format!("stopped {} ({})", stopped.label, stopped.detail))
            }
            ProcessOutcome::NotFound(_) => {
                lines.push(format!("{} already gone", snapshot.label))
            }
            ProcessOutcome::Error(error) => {
                lines.push(format!("{} failed to stop: {error}", snapshot.label))
            }
            _ => lines.push(format!("{}: unexpected outcome", snapshot.label)),
        }
    }

    if model.docker.is_some_and(|docker| docker.compose) {
        if compose_ready() {
            lines.push(format!("compose: {}", docker_summary(&compose_down(&path))));
        } else {
            lines.push("compose skipped: Docker daemon or compose is not available".to_string());
        }
    }

    Ok(lines.join("\n"))
}

/// Run a declared project script (e.g. `build`, `lint`, `test`) as a tracked process.
///
/// Only commands the scanner declared in the project model may run, under
/// the label `script-<name>`; anything else is rejected. Stopping works
/// through the regular `stop_project` command with the same label.
#[command]
fn run_script(path: Option<String>, name: String) -> Result<String, String> {
    let path = resolve_project_path(path)?;
    let scan = scan_project(&path);
    let model = scan.model().cloned().unwrap_or_else(|| {
        ProjectModel::new(pilot_scanner::project_name_from_path(&path), &path)
    });

    let declared = model.commands.iter().find(|command| command.name == name)
        .ok_or_else(|| {
            let available: Vec<&str> = model.commands.iter().map(|c| c.name.as_str()).collect();
            if available.is_empty() {
                format!("no scripts are declared in this project, so '{name}' cannot run")
            } else {
                format!("no script named '{name}' is declared; available: {}", available.join(", "))
            }
        })?;

    let label = format!("script-{name}");
    let manager = get_process_manager();
    if let ProcessOutcome::Snapshot(existing) = manager.status(&label) {
        if existing.state == ProcessState::Running {
            return Err(format!(
                "script '{name}' is already running (pid {:?}); stop it first",
                existing.pid
            ));
        }
    }

    let request = ProcessRequest::new(&label, &declared.command, &path);
    match manager.start(&request) {
        ProcessOutcome::Started(snapshot) => Ok(format!(
            "Started script '{name}' (`{}`) with pid {:?}",
            snapshot.command, snapshot.pid
        )),
        ProcessOutcome::Error(error) => Err(format!("script '{name}' failed to start: {error}")),
        _ => Err(format!("script '{name}' failed: unexpected outcome")),
    }
}

/// Force-terminate everything Pilot started for this project.
///
/// Unlike `stop_all_project` (graceful), this sends an immediate kill to
/// stuck processes. Only processes tracked by Pilot for this project are
/// touched — never unrelated system processes.
#[command]
fn kill_all_project(path: Option<String>) -> Result<String, String> {
    let path = resolve_project_path(path)?;
    let manager = get_process_manager();

    let mut tracked: Vec<_> = manager
        .list()
        .into_iter()
        .filter(|snapshot| snapshot.working_directory.eq_ignore_ascii_case(&path))
        .collect();
    tracked.sort_by(|a, b| b.label.cmp(&a.label));

    if tracked.is_empty() {
        return Ok("nothing tracked for this project; nothing to kill".to_string());
    }

    let mut killed = 0usize;
    let mut gone = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for snapshot in &tracked {
        match manager.kill(&snapshot.label) {
            ProcessOutcome::Stopped(_) => killed += 1,
            ProcessOutcome::NotFound(_) => gone += 1,
            ProcessOutcome::Error(error) => {
                failures.push(format!("{}: {error}", snapshot.label))
            }
            _ => failures.push(format!("{}: unexpected outcome", snapshot.label)),
        }
    }

    let mut summary = format!("force terminated {killed} process(es)");
    if gone > 0 {
        summary.push_str(&format!(", {gone} already gone"));
    }
    if !failures.is_empty() {
        summary.push_str(&format!("; could not kill: {}", failures.join("; ")));
    }
    Ok(summary)
}

/// Open a URL in the default browser (no console window, detached).
fn open_url_detached(url: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let output = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .creation_flags(0x0800_0000)
            .output()
            .map_err(|error| format!("could not open {url}: {error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "could not open {url}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .output()
            .map_err(|error| format!("could not open {url}: {error}"))
            .and_then(|output| {
                if output.status.success() {
                    Ok(())
                } else {
                    Err(format!("could not open {url}"))
                }
            })
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .output()
            .map_err(|error| format!("could not open {url}: {error}"))
            .and_then(|output| {
                if output.status.success() {
                    Ok(())
                } else {
                    Err(format!("could not open {url}"))
                }
            })
    }
}

/// Open the running frontend in the default browser.
///
/// The URL is derived from the detected frontend port and is only opened
/// when something is actually listening there — never a guessed address.
#[command]
fn open_frontend(path: Option<String>) -> Result<String, String> {
    let path = resolve_project_path(path)?;
    let scan = scan_project(&path);
    let model = scan.model().cloned().unwrap_or_else(|| {
        ProjectModel::new(pilot_scanner::project_name_from_path(&path), &path)
    });

    let frontend = model.frontend.as_ref().ok_or_else(|| {
        "no frontend was detected in this project, so there is nothing to open".to_string()
    })?;
    let url = format!("http://localhost:{}", frontend.port);

    if pilot_port_manager::is_listening(frontend.port) {
        open_url_detached(&url)?;
        Ok(format!("opened {url} in the default browser"))
    } else {
        Err(format!(
            "{} ({} on port {}) is not running; start it first",
            frontend.framework, "frontend", frontend.port
        ))
    }
}

/// Open a directory picker and return the selected path
#[command]
async fn select_directory(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    use std::sync::mpsc;
    let (tx, rx) = mpsc::channel();
    app.dialog()
        .file()
        .set_title("Select Project Directory")
        .pick_folder(move |path| {
            let _ = tx.send(path.map(|p| p.to_string()));
        });
    let result = rx.recv().map_err(|e| format!("Dialog error: {}", e))?;
    Ok(result)
}

// Response types for Tauri serialization (camelCase to match the TS contract)
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StartupPlanResponse {
    executable: bool,
    steps: Vec<StartupStepResponse>,
    warnings: Vec<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StartupStepResponse {
    service: String,
    description: String,
    command: String,
    working_directory: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ProcessSnapshotResponse {
    label: String,
    command: String,
    working_directory: String,
    pid: Option<u32>,
    state: String,
    detail: String,
    exit_code: Option<i32>,
    started_at_ms: Option<u64>,
}

// Global process manager instance
use std::sync::OnceLock;
static PROCESS_MANAGER: OnceLock<Arc<LocalProcessManager>> = OnceLock::new();

pub(crate) fn get_process_manager() -> &'static Arc<LocalProcessManager> {
    PROCESS_MANAGER.get_or_init(|| Arc::new(LocalProcessManager::new()))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            detect_project,
            get_status,
            run_diagnostics,
            get_startup_plan,
            start_project,
            stop_project,
            restart_project,
            start_all_project,
            stop_all_project,
            kill_all_project,
            run_script,
            open_frontend,
            get_process_status,
            list_processes,
            get_service_logs,
            docker_status,
            list_docker_containers,
            docker_container_action,
            docker_compose,
            database_operation,
            system_health,
            select_directory
        ])
        .run(tauri::generate_context!())
        .expect("error while running the OpsPilot application");
}
