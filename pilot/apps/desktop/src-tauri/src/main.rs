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
use pilot_docker::DockerOperation;
use pilot_docker::{DockerOutcome, compose_down, compose_up, execute as execute_docker};
use pilot_process_manager::{
    LocalProcessManager, LogEntry, ProcessManager, ProcessOutcome, ProcessRequest, ProcessState,
    StartupStep, build_startup_plan, validate_plan_ports, validate_step,
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
    let requested = path.filter(|value| !value.trim().is_empty()).or_else(|| {
        env::var(PROJECT_PATH_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty())
    });

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

/// Normalize paths for case-insensitive, slash-insensitive, canonical comparisons.
pub(crate) fn normalize_path(path: &str) -> String {
    let p = dunce::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
    p.to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_lowercase()
}

/// Check if a service's working directory matches or is a subfolder of the project directory.
pub(crate) fn path_belongs_to_project(service_dir: &str, project_dir: &str) -> bool {
    let s = normalize_path(service_dir);
    let p = normalize_path(project_dir);
    s == p || s.starts_with(&format!("{p}/"))
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
    let model = scan
        .model()
        .cloned()
        .unwrap_or_else(|| ProjectModel::new(pilot_scanner::project_name_from_path(&path), &path));
    Ok(run_engine_diagnostics(&path, &model))
}

/// Get the startup plan for a project (what can be started)
#[command]
fn get_startup_plan(path: Option<String>) -> Result<StartupPlanResponse, String> {
    let path = resolve_project_path(path)?;
    let scan = scan_project(&path);
    let model = scan
        .model()
        .cloned()
        .unwrap_or_else(|| ProjectModel::new(pilot_scanner::project_name_from_path(&path), &path));
    let plan = build_startup_plan(&model, &path);
    let steps: Vec<StartupStepResponse> = plan
        .steps
        .iter()
        .map(|s| StartupStepResponse {
            service: s.service.clone(),
            description: s.description.clone(),
            command: s.command.clone(),
            working_directory: s.working_directory.clone(),
        })
        .collect();
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
    let model = scan
        .model()
        .cloned()
        .unwrap_or_else(|| ProjectModel::new(pilot_scanner::project_name_from_path(&path), &path));
    let plan = build_startup_plan(&model, &path);
    let step_storage;
    let step = if let Some(s) = plan.steps.iter().find(|s| s.service == service) {
        s
    } else if let Some(cmd) = model.commands.iter().find(|c| c.name == service) {
        step_storage = StartupStep::new(&service, &cmd.command, &path);
        &step_storage
    } else {
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
        return Err(message);
    };
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
    if let ProcessOutcome::Snapshot(existing) = manager.status(&step.service)
        && existing.state == ProcessState::Running
        && !path_belongs_to_project(&existing.working_directory, &path)
    {
        return Err(format!(
            "{} is already running for another project ({}) with pid {:?}; stop it there first",
            service, existing.working_directory, existing.pid
        ));
    }
    // Pre-flight: if the service port is held by a process Pilot did not
    // start (a terminal, an IDE, a previous session), refuse now with the
    // holder's name and the way out — instead of failing cryptically later.
    if let Some(port) = declared_port(&model, &service) {
        let probe = pilot_port_manager::inspect_port(port);
        let ours = probe.pid.is_some_and(|pid| {
            manager.list().into_iter().any(|snapshot| {
                snapshot.state == ProcessState::Running
                    && path_belongs_to_project(&snapshot.working_directory, &path)
                    && (snapshot.pid == Some(pid) || manager.contains_pid(&snapshot.label, pid))
            }) || manager.any_contains_pid(pid).is_some()
        });
        if !probe.available && !ours {
            return Err(port_conflict_message(
                &service,
                port,
                probe.pid,
                probe.process.as_deref(),
            ));
        }
    }
    match manager.start(&request) {
        ProcessOutcome::Started(snapshot) => Ok(format!(
            "Started {} (pid {:?})",
            snapshot.label, snapshot.pid
        )),
        ProcessOutcome::Error(e) => Err(format!(
            "Failed to start {} (command `{}` in `{}`): {e}",
            service, step.command, step.working_directory
        )),
        _ => Err("Unexpected outcome".to_string()),
    }
}

/// Port a service key is expected to listen on, when the project declares one.
///
/// Used by the start pre-flight so a blocked start can name the real cause
/// instead of failing later with a cryptic spawn error.
fn declared_port(model: &ProjectModel, service: &str) -> Option<u16> {
    match service {
        "frontend" => model.frontend.as_ref().map(|info| info.port),
        "backend" => model.backend.as_ref().map(|info| info.port),
        "database" => model.database.as_ref().map(|info| info.port),
        _ => None,
    }
}

/// Pre-flight blocker for starting a service whose port is held by a
/// process Pilot did not start.
///
/// The message names the holder and the two ways out, so the pilot never
/// needs an emergency procedure to re-board.
fn port_conflict_message(
    service: &str,
    port: u16,
    pid: Option<u32>,
    process: Option<&str>,
) -> String {
    match (pid, process) {
        (Some(pid), Some(process)) => format!(
            "Cannot start {service}: port {port} is already used by {process} (pid {pid}), \
             which Pilot did not start. Use the Stop External button on the {service} card, \
             or stop it in the terminal you started it from, then start again."
        ),
        _ => format!(
            "Cannot start {service}: port {port} is already in use by a process Pilot \
             could not identify. Free the port (or change it), then start again."
        ),
    }
}

/// Force-terminate a single PID (for processes Pilot did not start).
fn kill_pid(pid: u32) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let output = std::process::Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .creation_flags(0x0800_0000)
            .output()
            .map_err(|error| format!("could not run taskkill for pid {pid}: {error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "taskkill failed for pid {pid}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }

    #[cfg(not(windows))]
    {
        let output = std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .output()
            .map_err(|error| format!("could not run kill for pid {pid}: {error}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "kill failed for pid {pid}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ))
        }
    }
}

/// Stop a project service (process lifecycle - phase 4)
#[command]
fn stop_project(service: String) -> Result<String, String> {
    let manager = get_process_manager();
    match manager.stop(&service) {
        ProcessOutcome::Stopped(snapshot) => {
            Ok(format!("Stopped {} ({})", snapshot.label, snapshot.detail))
        }
        ProcessOutcome::NotFound(label) => Err(format!(
            "Service {} not found or not started by Pilot",
            label
        )),
        ProcessOutcome::Error(e) => Err(e),
        _ => Err("Unexpected outcome".to_string()),
    }
}

/// Restart a project service (process lifecycle - phase 4)
#[command]
fn restart_project(service: String) -> Result<String, String> {
    let manager = get_process_manager();
    match manager.restart(&service) {
        ProcessOutcome::Started(snapshot) => Ok(format!(
            "Restarted {} (pid {:?})",
            snapshot.label, snapshot.pid
        )),
        ProcessOutcome::NotFound(label) => Err(format!(
            "Service {} not found or not started by Pilot",
            label
        )),
        ProcessOutcome::Error(e) => Err(e),
        _ => Err("Unexpected outcome".to_string()),
    }
}

/// Force-stop the external process holding a service's port.
///
/// This is the engine-stop switch for engines Pilot did not start: the
/// service card shows the state, the GUI confirms, then this command
/// re-identifies the holder right before killing it (a recycled PID can
/// never make Pilot terminate the wrong process) and verifies the port is
/// actually free afterwards.
#[command]
fn stop_external_service(path: Option<String>, service: String) -> Result<String, String> {
    let path = resolve_project_path(path)?;
    let scan = scan_project(&path);
    let Some(model) = scan.model() else {
        return Err(
            "no project detected in this directory, so there is no service to stop".to_string(),
        );
    };

    let statuses = build_status(model);
    let status = statuses
        .into_iter()
        .find(|status| status.key == service)
        .ok_or_else(|| format!("no service '{service}' was detected for this project"))?;
    let port = status.port.ok_or_else(|| {
        format!("service '{service}' declares no port, so Pilot cannot identify what holds it")
    })?;

    let probe = pilot_port_manager::inspect_port(port);
    if probe.available {
        return Ok(format!("port {port} is already free; nothing to stop"));
    }
    let pid = probe.pid.ok_or_else(|| {
        format!(
            "port {port} is occupied, but the process holding it could not be identified. \
             Stop it from the terminal you started it in."
        )
    })?;
    if pid == 0 || (cfg!(windows) && pid == 4) {
        return Err(format!(
            "refusing to stop the system process holding port {port}"
        ));
    }

    // If Pilot actually tracks this PID (state said external, but the user
    // started a same-named service through Pilot since), stop it gracefully
    // through the process manager instead of a force-kill.
    // Also check descendant PIDs: cmd.exe → npm → node.exe means the root PID
    // differs from the port-holder PID, so we must check the whole tree.
    let manager = get_process_manager();
    let pilot_owned = manager
        .list()
        .into_iter()
        .find(|snapshot| snapshot.state == ProcessState::Running && snapshot.pid == Some(pid))
        .or_else(|| manager.any_contains_pid(pid));

    if let Some(tracked) = pilot_owned {
        return match manager.stop(&tracked.label) {
            ProcessOutcome::Stopped(stopped) => Ok(format!(
                "stopped {} (Pilot-tracked, {})",
                stopped.label, stopped.detail
            )),
            _ => Err(format!(
                "could not stop Pilot-tracked process {}",
                tracked.label
            )),
        };
    }

    // Re-identify the holder immediately before the kill: PIDs get recycled,
    // and a stale reading must never make Pilot terminate the wrong process.
    let fresh = pilot_port_manager::inspect_port(port);
    if fresh.available {
        return Ok(format!(
            "port {port} became free on its own; nothing to stop"
        ));
    }
    if fresh.pid != Some(pid) {
        let holder = fresh
            .process
            .as_deref()
            .map(|name| {
                format!(
                    "{name} (pid {})",
                    fresh
                        .pid
                        .map(|p| p.to_string())
                        .unwrap_or_else(|| "?".to_string())
                )
            })
            .unwrap_or_else(|| "an unidentified process".to_string());
        return Err(format!(
            "port {port} changed hands (now held by {holder}); re-check the card and try again"
        ));
    }

    kill_pid(pid)?;

    // Give the OS a moment, then verify the engine really stopped.
    std::thread::sleep(std::time::Duration::from_millis(400));
    let holder = probe.process.as_deref().unwrap_or("the external process");
    if pilot_port_manager::is_listening(port) {
        Ok(format!(
            "stopped {holder} (pid {pid}), but port {port} is still occupied — \
             a child process may hold it; check the card again in a moment"
        ))
    } else {
        Ok(format!("stopped {holder} (pid {pid}); port {port} is free"))
    }
}

/// List all tracked processes
///
/// The lifecycle state is serialized as a lowercase enum (`running`,
/// `stopped`, `exited`, `failed`) — never a Rust Debug string — so the GUI
/// matches on it with type safety instead of stringly-typed luck.
#[command]
fn list_processes() -> Result<Vec<ProcessSnapshotResponse>, String> {
    let manager = get_process_manager();
    Ok(manager
        .list()
        .into_iter()
        .map(|s| ProcessSnapshotResponse {
            label: s.label,
            command: s.command,
            working_directory: s.working_directory,
            pid: s.pid,
            state: s.state,
            detail: s.detail,
            exit_code: s.exit_code,
            started_at_ms: s.started_at_ms,
        })
        .collect())
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
/// The ORM selects the integration (Prisma/Django/Alembic/Kysely); without an
/// ORM, a PostgreSQL database falls back to raw `psql`. Anything else reports
/// `NotImplemented` with the reason instead of guessing commands.
fn database_integration_for(
    model: &ProjectModel,
    path: &str,
) -> Result<DatabaseIntegration, DatabaseOutcome> {
    let orm = model.orm.as_ref().map(|orm| orm.r#type.to_lowercase());
    let db_type = model.database.as_ref().map(|db| db.r#type.to_lowercase());
    let integration_type = match orm.as_deref() {
        Some("prisma") => IntegrationType::Prisma,
        Some("django") => IntegrationType::Django,
        Some("alembic") => IntegrationType::Alembic,
        // Kysely against PostgreSQL uses the raw-psql integration; otherwise
        // the database is an embedded SQLite file (no server, no CLI).
        Some("kysely") => match db_type.as_deref() {
            Some("postgresql") | Some("postgres") => IntegrationType::Postgres,
            _ => IntegrationType::Sqlite,
        },
        _ => {
            match db_type.as_deref() {
                Some("postgresql") | Some("postgres") => IntegrationType::Postgres,
                Some(other) => {
                    return Err(DatabaseOutcome::NotImplemented {
                        reason: format!(
                            "no migration integration for database type '{other}' (supported: prisma, django, alembic, kysely/sqlite, postgresql)"
                        ),
                    });
                }
                None => {
                    let detected = orm.as_deref().unwrap_or("none");
                    return Err(DatabaseOutcome::NotImplemented {
                        reason: format!(
                            "no migration integration for ORM '{detected}' (supported: prisma, django, alembic, kysely/sqlite)"
                        ),
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

    // Read env vars from .env and .env.local so migration tools (Prisma, Alembic, etc.)
    // can pick up DATABASE_URL and similar credentials without requiring the user to
    // manually set them. Only the project's own directory is consulted; global env is
    // not touched. Lines that cannot be parsed are silently skipped.
    let mut env = HashMap::new();
    for env_file in [".env", ".env.local", ".env.development"] {
        let env_path = std::path::Path::new(path).join(env_file);
        if let Ok(content) = std::fs::read_to_string(&env_path) {
            for line in content.lines() {
                let line = line.trim();
                // Skip comments and blank lines
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = line.split_once('=') {
                    let key = key.trim().to_string();
                    // Only pass database-related and generic connection env vars;
                    // never forward secrets unrelated to the operation.
                    if key == "DATABASE_URL"
                        || key == "DB_URL"
                        || key == "DB_HOST"
                        || key == "DB_PORT"
                        || key == "DB_NAME"
                        || key == "DB_USER"
                        || key == "DB_PASSWORD"
                        || key == "POSTGRES_URL"
                        || key == "MYSQL_URL"
                        || key == "MONGO_URL"
                        || key.starts_with("DJANGO_")
                        || key == "FLASK_ENV"
                    {
                        let value = value
                            .trim()
                            .trim_matches('"')
                            .trim_matches('\'')
                            .to_string();
                        env.entry(key).or_insert(value);
                    }
                }
            }
        }
    }

    Ok(DatabaseIntegration {
        integration_type,
        project_dir: path.to_string(),
        database,
        env,
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
    let model = scan
        .model()
        .cloned()
        .unwrap_or_else(|| ProjectModel::new(pilot_scanner::project_name_from_path(&path), &path));
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
/// Default number of log lines returned when the caller does not supply a limit.
///
/// Kept in sync with LOG_CAPACITY from the process-manager crate. If that
/// capacity changes, update this value to maintain a sensible default fraction.
const DEFAULT_LOG_LIMIT: usize = 200;

/// Return the captured log lines for a running or recently stopped service.
///
/// Returns an empty list when the service was never started by Pilot.
/// `limit` caps the number of trailing lines (defaults to [`DEFAULT_LOG_LIMIT`]).
#[command]
fn get_service_logs(service: String, limit: Option<usize>) -> Result<Vec<LogEntry>, String> {
    let manager = get_process_manager();
    Ok(manager
        .log_buffer(&service)
        .map(|buffer| buffer.snapshot(limit.or(Some(DEFAULT_LOG_LIMIT))))
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
    let model = scan
        .model()
        .cloned()
        .unwrap_or_else(|| ProjectModel::new(pilot_scanner::project_name_from_path(&path), &path));

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
                    format!(
                        "{} (working directory `{}`)",
                        step.description, step.working_directory
                    )
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
                format!(
                    "daemon running ({})",
                    version.as_deref().unwrap_or("unknown version")
                )
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
    let model = scan
        .model()
        .cloned()
        .unwrap_or_else(|| ProjectModel::new(pilot_scanner::project_name_from_path(&path), &path));
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
        if let ProcessOutcome::Snapshot(existing) = manager.status(&step.service)
            && existing.state == ProcessState::Running
            && path_belongs_to_project(&existing.working_directory, &path)
        {
            lines.push(format!(
                "{} already running (pid {:?}); skipped",
                step.service, existing.pid
            ));
            continue;
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
    let model = scan
        .model()
        .cloned()
        .unwrap_or_else(|| ProjectModel::new(pilot_scanner::project_name_from_path(&path), &path));
    let manager = get_process_manager();
    let mut lines: Vec<String> = Vec::new();

    let mut tracked: Vec<_> = manager
        .list()
        .into_iter()
        .filter(|snapshot| path_belongs_to_project(&snapshot.working_directory, &path))
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
            ProcessOutcome::NotFound(_) => lines.push(format!("{} already gone", snapshot.label)),
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
    let model = scan
        .model()
        .cloned()
        .unwrap_or_else(|| ProjectModel::new(pilot_scanner::project_name_from_path(&path), &path));

    let declared = model
        .commands
        .iter()
        .find(|command| command.name == name)
        .ok_or_else(|| {
            let available: Vec<&str> = model.commands.iter().map(|c| c.name.as_str()).collect();
            if available.is_empty() {
                format!("no scripts are declared in this project, so '{name}' cannot run")
            } else {
                format!(
                    "no script named '{name}' is declared; available: {}",
                    available.join(", ")
                )
            }
        })?;

    let label = format!("script-{name}");
    let manager = get_process_manager();
    if let ProcessOutcome::Snapshot(existing) = manager.status(&label)
        && existing.state == ProcessState::Running
    {
        return Err(format!(
            "script '{name}' is already running (pid {:?}); stop it first",
            existing.pid
        ));
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
        .filter(|snapshot| path_belongs_to_project(&snapshot.working_directory, &path))
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
            ProcessOutcome::Error(error) => failures.push(format!("{}: {error}", snapshot.label)),
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
    let model = scan
        .model()
        .cloned()
        .unwrap_or_else(|| ProjectModel::new(pilot_scanner::project_name_from_path(&path), &path));

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
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Select Project Directory")
        .pick_folder(move |path| {
            let _ = tx.send(path.map(|p| p.to_string()));
        });
    rx.await.map_err(|e| format!("Dialog error: {e}"))
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
    state: ProcessState,
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
            stop_external_service,
            restart_project,
            start_all_project,
            stop_all_project,
            kill_all_project,
            run_script,
            open_frontend,
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

#[cfg(test)]
mod tests {
    use super::*;
    use pilot_core::{BackendInfo, DatabaseInfo, FrontendInfo};

    #[test]
    fn declared_port_maps_service_keys_to_model_ports() {
        let mut model = ProjectModel::new("demo", ".");
        model.frontend = Some(FrontendInfo::new("vite", 3000));
        model.backend = Some(BackendInfo::new("express", 4000));
        model.database = Some(DatabaseInfo::new("postgresql", 5432));

        assert_eq!(declared_port(&model, "frontend"), Some(3000));
        assert_eq!(declared_port(&model, "backend"), Some(4000));
        assert_eq!(declared_port(&model, "database"), Some(5432));
        assert_eq!(declared_port(&model, "app"), None);
        assert_eq!(declared_port(&model, "docker"), None);
    }

    #[test]
    fn port_conflict_message_names_the_external_holder() {
        let message = port_conflict_message("frontend", 3000, Some(4242), Some("node.exe"));

        assert!(message.contains("Cannot start frontend"));
        assert!(message.contains("port 3000"));
        assert!(message.contains("node.exe (pid 4242)"));
        assert!(message.contains("Stop External"));
    }

    #[test]
    fn port_conflict_message_for_an_unidentified_holder_keeps_the_way_out() {
        let message = port_conflict_message("backend", 4000, None, None);

        assert!(message.contains("Cannot start backend"));
        assert!(message.contains("could not identify"));
        assert!(message.contains("then start again"));
    }
}
