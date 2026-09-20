//! OpsPilot desktop shell.
//!
//! The window is a thin front end over the Pilot engine: every command delegates
//! to the scanner, port manager or diagnostics engine, and none of them runs a
//! project command. The GUI renders whatever the engine detected, which is what
//! makes the menu dynamic per project.

#![windows_subsystem = "windows"]

mod status;

use pilot_core::ProjectModel;
use pilot_diagnostics::{DiagnosticsReport, run_diagnostics as run_engine_diagnostics};
use pilot_process_manager::{LocalProcessManager, ProcessManager, ProcessOutcome, ProcessRequest, build_startup_plan, validate_step};
use pilot_scanner::scan_project;
use status::{ServiceStatus, build_status};
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
#[command]
fn start_project(path: Option<String>, service: String) -> Result<String, String> {
    let path = resolve_project_path(path)?;
    let scan = scan_project(&path);
    let model = scan.model().cloned().unwrap_or_else(|| {
        ProjectModel::new(pilot_scanner::project_name_from_path(&path), &path)
    });
    let plan = build_startup_plan(&model, &path);
    let step = plan.steps.iter().find(|s| s.service == service)
        .ok_or_else(|| format!("No startup step found for service {}", service))?;
    let blockers = validate_step(step);
    if !blockers.is_empty() {
        return Err(format!("Cannot start {}: {}", service, blockers.join(", ")));
    }
    let request = ProcessRequest::new(&step.service, &step.command, &step.working_directory);
    let manager = get_process_manager();
    match manager.start(&request) {
        ProcessOutcome::Started(snapshot) => Ok(format!("Started {} (pid {:?})", snapshot.label, snapshot.pid)),
        ProcessOutcome::Error(e) => Err(e),
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
        pid: s.pid,
        state: format!("{:?}", s.state),
        detail: s.detail,
        exit_code: s.exit_code,
        started_at_ms: s.started_at_ms,
    }).collect())
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

// Response types for Tauri serialization
#[derive(serde::Serialize)]
struct StartupPlanResponse {
    executable: bool,
    steps: Vec<StartupStepResponse>,
    warnings: Vec<String>,
}

#[derive(serde::Serialize)]
struct StartupStepResponse {
    service: String,
    description: String,
    command: String,
    working_directory: String,
}

#[derive(serde::Serialize)]
struct ProcessSnapshotResponse {
    label: String,
    command: String,
    pid: Option<u32>,
    state: String,
    detail: String,
    exit_code: Option<i32>,
    started_at_ms: Option<u64>,
}

// Global process manager instance
use std::sync::OnceLock;
static PROCESS_MANAGER: OnceLock<Arc<LocalProcessManager>> = OnceLock::new();

fn get_process_manager() -> &'static Arc<LocalProcessManager> {
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
            get_process_status,
            list_processes,
            select_directory
        ])
        .run(tauri::generate_context!())
        .expect("error while running the OpsPilot application");
}
