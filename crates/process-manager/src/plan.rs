//! Determined startup sequences and their validation.
//!
//! The plan answers "what can I do to run this project?" without running
//! anything yet: it turns detected capabilities and declared commands into
//! ordered steps, each validated before it executes (spec sections 10 and 19).

use pilot_core::ProjectModel;
use serde::Serialize;

/// One ordered step of a startup sequence
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupStep {
    /// Service this step starts (frontend, backend, ...)
    pub service: String,
    /// Human-readable step description shown in the progress list
    pub description: String,
    /// The exact command Pilot will run
    pub command: String,
    /// Directory the command runs in
    pub working_directory: String,
}

impl StartupStep {
    /// Create a startup step for a service, command and project directory
    pub fn new(
        service: impl Into<String>,
        command: impl Into<String>,
        working_directory: impl Into<String>,
    ) -> Self {
        let service = service.into();
        let command = command.into();

        StartupStep {
            service: service.clone(),
            description: format!("start {service}"),
            command,
            working_directory: working_directory.into(),
        }
    }
}

/// A startup sequence the user can review before Pilot runs anything
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupPlan {
    /// Ordered steps
    pub steps: Vec<StartupStep>,
    /// Capabilities with no automatic step, explained rather than guessed
    pub warnings: Vec<String>,
}

impl StartupPlan {
    /// An empty plan with one explanation, for projects with nothing to start
    pub fn empty(warning: impl Into<String>) -> Self {
        StartupPlan {
            steps: Vec::new(),
            warnings: vec![warning.into()],
        }
    }

    /// Whether Pilot can start something automatically
    pub fn executable(&self) -> bool {
        !self.steps.is_empty()
    }

    /// The exact commands the plan would run, in order
    pub fn commands(&self) -> Vec<String> {
        self.steps.iter().map(|step| step.command.clone()).collect()
    }
}

/// Build the startup sequence for a detected project.
///
/// Database, Docker and migration steps stay warnings on purpose: they are
/// operated from the Database and Docker panels (with explicit confirmation
/// for destructive operations), and inventing automatic commands for them
/// would be a lie.
pub fn build_startup_plan(model: &ProjectModel, project_path: &str) -> StartupPlan {
    let mut plan = StartupPlan {
        steps: Vec::new(),
        warnings: Vec::new(),
    };

    let frontend_cmd_info = if model.frontend.is_some() {
        model
            .commands
            .iter()
            .find(|c| is_frontend_command(&c.name))
            .or_else(|| {
                model
                    .commands
                    .iter()
                    .find(|c| is_run_command(&c.name))
            })
    } else {
        None
    };

    let backend_cmd_info = if model.backend.is_some() {
        model
            .commands
            .iter()
            .find(|c| is_backend_command(&c.name) || c.source.contains("manage.py"))
            .or_else(|| {
                model
                    .commands
                    .iter()
                    .find(|c| {
                        is_run_command(&c.name)
                            && (model.frontend.is_none()
                                || Some(&c.command)
                                    != frontend_cmd_info.map(|i| &i.command))
                    })
            })
    } else {
        None
    };

    if model.frontend.is_some() {
        if let Some(info) = frontend_cmd_info {
            let work_dir = step_working_dir(project_path, Some(info));
            plan.steps
                .push(StartupStep::new("frontend", info.command.clone(), work_dir));
        } else {
            plan.warnings.push(
                "package.json declares no dev or start script, so nothing can be started automatically"
                    .to_string(),
            );
        }
    }

    if model.backend.is_some() {
        if let Some(info) = backend_cmd_info {
            let work_dir = step_working_dir(project_path, Some(info));
            plan.steps
                .push(StartupStep::new("backend", info.command.clone(), work_dir));
        } else if model.frontend.is_some() && frontend_cmd_info.is_some() {
            let cmd = frontend_cmd_info.map(|i| i.command.as_str()).unwrap_or_default();
            plan.warnings.push(format!(
                "the backend is served by the same process ({cmd}), which is why it has no separate step"
            ));
        } else {
            let fw = model
                .backend
                .as_ref()
                .map(|b| b.framework.as_str())
                .unwrap_or("backend");
            plan.warnings.push(format!(
                "no start command was declared for the {fw} backend; add it explicitly"
            ));
        }
    }

    if model.frontend.is_none() && model.backend.is_none() {
        let app_cmd = model
            .commands
            .iter()
            .find(|c| is_run_command(&c.name))
            .map(|c| c.command.clone());

        if let Some(ref cmd) = app_cmd {
            plan.steps.push(StartupStep::new("app", cmd.clone(), project_path));
        } else {
            plan.warnings.push(
                "no startable service was detected; declare a dev/start/serve command to enable Start"
                    .to_string(),
            );
        }
    }

    if model.orm.is_some() {
        plan.warnings.push(
            "database migrations are not automatic; run them from the Database panel".to_string(),
        );
    }

    if model
        .docker
        .is_some_and(|docker| docker.compose || docker.detected)
    {
        plan.warnings.push(
            "Docker services are not started automatically; manage them from the Docker panel"
                .to_string(),
        );
    }

    if model.database.is_some() && !plan.executable() {
        plan.warnings.push(
            "the database has no managed service; operate it from the Database panel".to_string(),
        );
    }

    plan
}

/// Whether a declared command starts the project
fn is_run_command(name: &str) -> bool {
    matches!(name, "dev" | "start" | "serve")
}

fn is_frontend_command(name: &str) -> bool {
    matches!(
        name,
        "dev:frontend" | "frontend" | "dev:client" | "client" | "dev:web" | "web" | "dev:ui" | "ui"
    )
}

fn is_backend_command(name: &str) -> bool {
    matches!(
        name,
        "dev:backend" | "backend" | "dev:server" | "server" | "dev:api" | "api" | "runserver"
    )
}

fn step_working_dir(project_path: &str, command_info: Option<&pilot_core::CommandInfo>) -> String {
    if let Some(info) = command_info {
        if let Some((dir, _)) = info.source.split_once('/') {
            let path = std::path::Path::new(project_path).join(dir);
            if path.is_dir() {
                return path.to_string_lossy().to_string();
            }
        }
    }
    project_path.to_string()
}

/// Which runtime a command line needs, inferred from its first word
pub fn command_tool(command: &str) -> Option<&'static str> {
    let first = command.split_whitespace().next().unwrap_or_default();

    match first {
        "npm" | "node" | "npx" | "yarn" | "pnpm" | "bun" => Some("node"),
        "python" | "python3" | "py" | "pip" | "pipx" | "poetry" | "uv" | "uvicorn" | "gunicorn" => {
            Some("python")
        }
        "cargo" => Some("cargo"),
        "docker" | "docker-compose" => Some("docker"),
        _ => None,
    }
}

/// Validate a step before it executes, as a readable list of blockers.
///
/// An empty list means the step is safe to attempt.
pub fn validate_step(step: &StartupStep) -> Vec<String> {
    let mut blockers = Vec::new();

    if !std::path::Path::new(&step.working_directory).is_dir() {
        blockers.push(format!(
            "the working directory {} does not exist",
            step.working_directory
        ));
    }

    if let Some(tool) = command_tool(&step.command)
        && !pilot_diagnostics::tool_available(tool)
    {
        blockers.push(format!(
            "`{tool}` is not available, but `{}` needs it; install {tool} and try again",
            step.command
        ));
    }

    blockers
}

/// Validate the ports a plan would use against what is already listening.
pub fn validate_plan_ports(model: &ProjectModel) -> Vec<String> {
    let mut blockers = Vec::new();

    for (service, port) in [
        ("frontend", model.frontend.as_ref().map(|info| info.port)),
        ("backend", model.backend.as_ref().map(|info| info.port)),
        ("database", model.database.as_ref().map(|info| info.port)),
    ] {
        let Some(port) = port else {
            continue;
        };

        if pilot_port_manager::is_listening(port) {
            blockers.push(format!(
                "port {port} is already in use, which {service} would need"
            ));
        }
    }

    blockers
}

/// Whether a startup step runs exactly a command the scanner declared.
///
/// Pilot runs declared commands only; anything else needs an explicit user
/// action and is rejected here.
pub fn command_is_declared(model: &ProjectModel, service: &str, command: &str) -> bool {
    known_service(service)
        && model
            .commands
            .iter()
            .any(|declared| declared.command == command)
}

/// The services a startup step may address
fn known_service(service: &str) -> bool {
    matches!(service, "frontend" | "backend" | "app")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pilot_core::{BackendInfo, CommandInfo, DockerInfo, FrontendInfo, ProjectModel};

    fn node_model(command: &str) -> ProjectModel {
        let mut model = ProjectModel::new("demo", ".");
        model.frontend = Some(FrontendInfo::new("vite", 5173));
        model
            .commands
            .push(CommandInfo::new("dev", command, "package.json scripts"));

        model
    }

    #[test]
    fn a_node_frontend_produces_one_run_step() {
        let plan = build_startup_plan(&node_model("npm run dev"), ".");

        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].service, "frontend");
        assert_eq!(plan.steps[0].command, "npm run dev");
        assert_eq!(plan.steps[0].working_directory, ".");
        assert!(plan.executable());
        assert_eq!(plan.commands(), vec!["npm run dev".to_string()]);
    }

    #[test]
    fn a_full_stack_app_gets_one_step_and_an_explanation() {
        let mut model = node_model("npm run dev");
        model.backend = Some(BackendInfo::new("express", 3000));

        let plan = build_startup_plan(&model, ".");

        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].command, "npm run dev");
        assert!(
            plan.warnings
                .iter()
                .any(|warning| warning.contains("backend"))
        );
    }

    #[test]
    fn a_project_with_separate_frontend_and_backend_commands_gets_both_steps() {
        let mut model = node_model("npm run dev");
        model.backend = Some(BackendInfo::new("django", 8000));
        model.commands.push(CommandInfo::new(
            "backend",
            "python manage.py runserver",
            "manage.py",
        ));

        let plan = build_startup_plan(&model, ".");

        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.steps[0].service, "frontend");
        assert_eq!(plan.steps[0].command, "npm run dev");
        assert_eq!(plan.steps[1].service, "backend");
        assert_eq!(plan.steps[1].command, "python manage.py runserver");
        assert!(plan.executable());
    }

    #[test]
    fn a_django_project_uses_its_runner() {
        let mut model = ProjectModel::new("demo", ".");
        model.backend = Some(BackendInfo::new("django", 8000));
        model.commands.push(CommandInfo::new(
            "dev",
            "python manage.py runserver",
            "manage.py",
        ));

        let plan = build_startup_plan(&model, ".");

        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].service, "backend");
        assert_eq!(plan.steps[0].command, "python manage.py runserver");
    }

    #[test]
    fn a_framework_without_a_declared_command_stays_a_warning() {
        let mut model = ProjectModel::new("demo", ".");
        model.backend = Some(BackendInfo::new("fastapi", 8000));

        let plan = build_startup_plan(&model, ".");

        assert!(!plan.executable());
        assert!(
            plan.warnings
                .iter()
                .any(|warning| warning.contains("fastapi"))
        );
    }

    #[test]
    fn a_frontend_without_scripts_has_nothing_to_start() {
        let mut model = ProjectModel::new("demo", ".");
        model.frontend = Some(FrontendInfo::new("vite", 5173));

        let plan = build_startup_plan(&model, ".");

        assert!(!plan.executable());
        assert!(plan.warnings.iter().any(|warning| warning.contains("dev")));
    }

    #[test]
    fn docker_and_migrations_are_warnings_not_steps() {
        let mut model = node_model("npm run dev");
        model.docker = Some(DockerInfo::new(true, true));

        let plan = build_startup_plan(&model, ".");

        assert_eq!(plan.steps.len(), 1);
        assert!(
            plan.warnings
                .iter()
                .any(|warning| warning.contains("Docker panel"))
        );
    }

    #[test]
    fn tools_are_inferred_from_the_command_line() {
        assert_eq!(command_tool("npm run dev"), Some("node"));
        assert_eq!(command_tool("npx vite"), Some("node"));
        assert_eq!(command_tool("python manage.py runserver"), Some("python"));
        assert_eq!(command_tool("docker compose up"), Some("docker"));
        assert_eq!(command_tool("cargo run"), Some("cargo"));
        assert_eq!(command_tool("make serve"), None);
    }

    #[test]
    fn validation_catches_a_missing_working_directory() {
        let step = StartupStep::new("frontend", "npm run dev", "this/path/does/not/exist");
        let blockers = validate_step(&step);

        assert!(
            blockers
                .iter()
                .any(|blocker| blocker.contains("working directory"))
        );
    }

    #[test]
    fn validation_of_a_runnable_step_has_no_blockers() {
        let step = StartupStep::new("frontend", "node --version", ".");

        assert!(validate_step(&step).is_empty());
    }

    #[test]
    fn an_unavailable_runtime_blocks_the_step() {
        let step = StartupStep::new("backend", "definitely-missing-tool-xyz run", ".");

        assert!(
            validate_step(&step).is_empty(),
            "an unknown tool cannot be checked"
        );
    }

    #[test]
    fn only_declared_commands_may_run() {
        let model = node_model("npm run dev");

        assert!(command_is_declared(&model, "frontend", "npm run dev"));
        assert!(!command_is_declared(&model, "frontend", "npm install"));
        assert!(!command_is_declared(&model, "database", "npm run dev"));
    }

    #[test]
    fn a_non_django_backend_with_a_run_command_gets_a_backend_step() {
        let mut model = ProjectModel::new("demo", ".");
        model.backend = Some(BackendInfo::new("fastapi", 8000));
        model.commands.push(CommandInfo::new(
            "dev",
            "uvicorn main:app --reload",
            "package.json scripts",
        ));

        let plan = build_startup_plan(&model, ".");

        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].service, "backend");
        assert_eq!(plan.steps[0].command, "uvicorn main:app --reload");
        assert!(plan.executable());
    }

    #[test]
    fn a_run_command_without_a_detected_framework_gets_an_app_step() {
        let mut model = ProjectModel::new("demo", ".");
        model.commands.push(CommandInfo::new(
            "start",
            "node server.js",
            "package.json scripts",
        ));

        let plan = build_startup_plan(&model, ".");

        assert_eq!(plan.steps.len(), 1);
        assert_eq!(plan.steps[0].service, "app");
        assert!(plan.executable());
    }

    #[test]
    fn an_empty_project_plan_explains_itself() {
        let model = ProjectModel::new("demo", ".");

        let plan = build_startup_plan(&model, ".");

        assert!(!plan.executable());
        assert!(!plan.warnings.is_empty());
    }

    #[test]
    fn app_is_a_known_service() {
        let model = node_model("npm run dev");

        assert!(command_is_declared(&model, "app", "npm run dev"));
    }
}
