//! Pilot Diagnostics Engine - Deterministic health checks
//!
//! Every check returns structured, evidence-based results: problem, evidence,
//! possible cause and recommended action, as required by
//! "Pilot Prerequisite.md" section 17.
//!
//! Checks are deterministic and read-only. They combine what the scanner already
//! collected with toolchain probes; no project command is ever executed.

use pilot_core::ProjectModel;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

/// A single diagnostic check result
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticResult {
    /// Stable check identifier (for example `node-runtime`)
    pub id: String,
    /// Human-readable check name
    pub name: String,
    /// Whether the check passed
    pub passed: bool,
    /// Human-readable problem description
    pub problem: Option<String>,
    /// Evidence supporting the result
    pub evidence: Option<String>,
    /// Possible cause of any issue
    pub cause: Option<String>,
    /// Recommended action to fix the issue
    pub recommended_action: Option<String>,
}

impl DiagnosticResult {
    /// A check that passed, with the evidence that confirmed it
    pub fn passed(
        id: impl Into<String>,
        name: impl Into<String>,
        evidence: impl Into<String>,
    ) -> Self {
        DiagnosticResult {
            id: id.into(),
            name: name.into(),
            passed: true,
            problem: None,
            evidence: Some(evidence.into()),
            cause: None,
            recommended_action: None,
        }
    }

    /// A check that failed, with problem, evidence and recommended action
    pub fn failed(
        id: impl Into<String>,
        name: impl Into<String>,
        problem: impl Into<String>,
        evidence: impl Into<String>,
        recommended_action: impl Into<String>,
    ) -> Self {
        DiagnosticResult {
            id: id.into(),
            name: name.into(),
            passed: false,
            problem: Some(problem.into()),
            evidence: Some(evidence.into()),
            cause: None,
            recommended_action: Some(recommended_action.into()),
        }
    }

    /// Attach a suspected cause to this result
    pub fn with_cause(mut self, cause: impl Into<String>) -> Self {
        self.cause = Some(cause.into());
        self
    }
}

/// Result of a full diagnostics run
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsReport {
    /// Every check that was executed
    pub checks: Vec<DiagnosticResult>,
    /// Number of failed checks
    pub issues: usize,
}

impl DiagnosticsReport {
    /// Build a report from check results
    pub fn new(checks: Vec<DiagnosticResult>) -> Self {
        let issues = checks.iter().filter(|check| !check.passed).count();

        DiagnosticsReport { checks, issues }
    }

    /// All failed checks
    pub fn problems(&self) -> impl Iterator<Item = &DiagnosticResult> {
        self.checks.iter().filter(|check| !check.passed)
    }

    /// Whether every check passed
    pub fn is_healthy(&self) -> bool {
        self.issues == 0
    }
}

/// Run all deterministic diagnostics for a project
///
/// `project_path` is the directory the scanner inspected; the detected model
/// decides which runtime checks are relevant.
pub fn run_diagnostics(project_path: &str, model: &ProjectModel) -> DiagnosticsReport {
    let mut checks = Vec::new();

    if model.frontend.is_some() || model.backend.is_some() {
        checks.push(runtime_check(
            "node-runtime",
            "Node.js",
            "node",
            "Install Node.js from https://nodejs.org and reopen the project.",
        ));
    }

    if model
        .backend
        .as_ref()
        .is_some_and(|backend| matches!(backend.framework.as_str(), "django" | "fastapi" | "flask"))
    {
        checks.push(runtime_check(
            "python-runtime",
            "Python",
            "python",
            "Install Python from https://python.org and reopen the project.",
        ));
    }

    if model.docker.is_some_and(|docker| docker.detected) {
        checks.push(runtime_check(
            "docker-runtime",
            "Docker",
            "docker",
            "Install Docker Desktop, or remove Docker usage from the project.",
        ));
    }

    checks.push(dependencies_check(project_path, model));
    checks.push(environment_check(project_path, model));

    DiagnosticsReport::new(checks)
}

/// Check whether a runtime is installed by asking it for its version (read-only)
fn runtime_check(id: &str, name: &str, tool: &str, recommended_action: &str) -> DiagnosticResult {
    match tool_version(tool) {
        Some(version) => {
            DiagnosticResult::passed(id, name, format!("{tool} --version -> {version}"))
        }
        None => DiagnosticResult::failed(
            id,
            name,
            format!("{name} executable not found on PATH"),
            format!("`{tool} --version` could not be executed"),
            recommended_action,
        )
        .with_cause(format!(
            "{name} is not installed, or is not on the PATH Pilot sees"
        )),
    }
}

/// Check that Node.js dependencies are installed when the project needs them
fn dependencies_check(project_path: &str, model: &ProjectModel) -> DiagnosticResult {
    let node_project = model.frontend.is_some() || model.backend.is_some();

    if !node_project || !Path::new(project_path).join("package.json").exists() {
        return DiagnosticResult::passed(
            "dependencies",
            "Dependencies",
            "no Node.js manifest in the scanned directory; dependency check not required",
        );
    }

    if Path::new(project_path).join("node_modules").is_dir() {
        DiagnosticResult::passed("dependencies", "Dependencies", "node_modules is present")
    } else {
        DiagnosticResult::failed(
            "dependencies",
            "Dependencies",
            "node_modules is missing",
            "package.json was detected but node_modules does not exist",
            "Run the project's install command (for example `npm install`).",
        )
        .with_cause("Dependencies were never installed, or node_modules was removed")
    }
}

/// Check environment configuration completeness
fn environment_check(project_path: &str, model: &ProjectModel) -> DiagnosticResult {
    let Some(environment) = model.environment else {
        return DiagnosticResult::passed(
            "environment",
            "Environment",
            "this project does not use environment files",
        );
    };

    if environment.env_file {
        DiagnosticResult::passed(
            "environment",
            "Environment",
            "a .env file is present (values are never read by Pilot)",
        )
    } else if environment.env_example {
        DiagnosticResult::failed(
            "environment",
            "Environment",
            ".env.example exists but .env is missing",
            format!("checked in {project_path}"),
            "Create .env from .env.example and fill in the required values.",
        )
        .with_cause("The project expects a local environment file that was never created")
    } else {
        DiagnosticResult::passed("environment", "Environment", "no .env file expected")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pilot_core::{EnvironmentInfo, FrontendInfo};

    #[test]
    fn a_report_counts_only_failed_checks() {
        let report = DiagnosticsReport::new(vec![
            DiagnosticResult::passed("a", "A", "ok"),
            DiagnosticResult::failed("b", "B", "broken", "evidence", "fix it"),
        ]);

        assert_eq!(report.issues, 1);
        assert_eq!(report.problems().count(), 1);
        assert!(!report.is_healthy());
    }

    #[test]
    fn environment_is_flagged_when_only_the_example_exists() {
        let mut model = ProjectModel::new("demo", ".");
        model.environment = Some(EnvironmentInfo::new(false, true, false));

        let check = environment_check(".", &model);

        assert!(!check.passed);
        assert!(check.problem.is_some());
        assert!(check.cause.is_some());
        assert!(check.recommended_action.is_some());
    }

    #[test]
    fn environment_passes_without_env_files() {
        let model = ProjectModel::new("demo", ".");

        assert!(environment_check(".", &model).passed);
    }

    #[test]
    fn dependencies_are_skipped_without_a_node_manifest() {
        let mut model = ProjectModel::new("demo", ".");
        model.frontend = Some(FrontendInfo::new("nextjs", 3000));

        // This crate's own directory has no package.json, so the check is skipped.
        let check = dependencies_check(".", &model);

        assert!(check.passed);
        assert!(check.evidence.is_some_and(|text| text.contains("not required")));
    }

    #[test]
    fn an_empty_model_still_reports_dependencies_and_environment() {
        let model = ProjectModel::new("demo", ".");

        let report = run_diagnostics(".", &model);

        assert_eq!(report.checks.len(), 2);
        assert!(report.is_healthy());
    }

    #[test]
    fn a_node_project_triggers_the_node_runtime_check() {
        let mut model = ProjectModel::new("demo", ".");
        model.frontend = Some(FrontendInfo::new("nextjs", 3000));

        let report = run_diagnostics(".", &model);

        assert!(
            report.checks.iter().any(|check| check.id == "node-runtime"),
            "checks: {:?}",
            report.checks
        );
    }

    #[test]
    fn results_serialize_for_the_gui() {
        let report = DiagnosticsReport::new(vec![DiagnosticResult::failed(
            "docker-runtime",
            "Docker",
            "Docker executable not found on PATH",
            "`docker --version` could not be executed",
            "Install Docker Desktop, or remove Docker usage from the project.",
        )]);

        let json = serde_json::to_value(&report).expect("report must serialize");

        assert_eq!(json["issues"], 1);
        assert_eq!(json["checks"][0]["id"], "docker-runtime");
        assert_eq!(json["checks"][0]["passed"], false);
        assert!(json["checks"][0]["recommendedAction"].is_string());
    }
}
/// Ask a tool for its version, returning the first line of output on success
pub fn tool_version(tool: &str) -> Option<String> {
    let output = command_for(tool).output().ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = stdout.lines().next().unwrap_or_default().trim().to_string();

    (!version.is_empty()).then_some(version)
}

/// Whether a tool can be executed at all (read-only probe)
///
/// Used by the operation layer before it plans a startup step.
pub fn tool_available(tool: &str) -> bool {
    tool_version(tool).is_some()
}

/// Build a `--version` probe for a tool, handling Windows shims such as `npm.cmd`
fn command_for(tool: &str) -> Command {
    #[cfg(windows)]
    {
        let mut command = Command::new("cmd");
        command.args(["/C", tool, "--version"]);
        command
    }

    #[cfg(not(windows))]
    {
        let mut command = Command::new(tool);
        command.arg("--version");
        command
    }
}