//! OpsPilot desktop shell.
//!
//! The window is a thin front end over the Pilot engine: every command delegates
//! to the scanner, port manager or diagnostics engine, and none of them runs a
//! project command. The GUI renders whatever the engine detected, which is what
//! makes the menu dynamic per project.

mod status;

use pilot_core::ProjectModel;
use pilot_diagnostics::{DiagnosticsReport, run_diagnostics as run_engine_diagnostics};
use pilot_scanner::scan_project;
use status::{ServiceStatus, build_status};
use std::env;
use std::path::PathBuf;
use tauri::command;

/// Environment variable used by the launcher to open a specific project
const PROJECT_PATH_ENV: &str = "PILOT_PROJECT_PATH";

/// Message returned by operations that belong to a later phase
const NOT_IMPLEMENTED_START: &str = "Starting projects is not implemented yet: the process lifecycle arrives in phase 4, \
and Pilot only runs project commands it can track, log and stop.";

/// Resolve the directory Pilot should operate on.
///
/// Order: explicit path (from the launcher) > `PILOT_PROJECT_PATH` > current
/// directory, so `cd my-project && pilot` opens that project.
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
        return Err(format!("`{}` is not a directory", candidate.display()));
    }

    // Normalize so the dashboard shows a real project name and path instead of `.`.
    pilot_scanner::resolve_project_path(&candidate.to_string_lossy())
        .map_err(|error| format!("could not resolve `{}`: {error}", candidate.display()))
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

    // Diagnostics must still work for a directory without a detected stack: the
    // environment and dependency checks are useful on their own.
    let model = scan.model().cloned().unwrap_or_else(|| {
        ProjectModel::new(pilot_scanner::project_name_from_path(&path), &path)
    });

    Ok(run_engine_diagnostics(&path, &model))
}

/// Start a project (process lifecycle - phase 4)
#[command]
fn start_project() -> Result<String, String> {
    Err(NOT_IMPLEMENTED_START.to_string())
}

/// Stop a project (process lifecycle - phase 4)
#[command]
fn stop_project() -> Result<String, String> {
    Err("Stopping projects is not implemented yet: Pilot only stops processes it started itself (phase 4)."
        .to_string())
}

/// Restart a project (process lifecycle - phase 4)
#[command]
fn restart_project() -> Result<String, String> {
    Err("Restarting projects is not implemented yet (phase 4).".to_string())
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
    
    // Wait for the callback to be called
    let result = rx.recv().map_err(|e| format!("Dialog error: {}", e))?;
    
    Ok(result)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            detect_project,
            get_status,
            run_diagnostics,
            start_project,
            stop_project,
            restart_project,
            select_directory
        ])
        .run(tauri::generate_context!())
        .expect("error while running the OpsPilot application");
}