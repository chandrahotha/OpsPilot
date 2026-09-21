//! Node.js detection: manifest reading, package manager and framework resolution.

use crate::{Detector, exists, read_text};
use pilot_core::{BackendInfo, CommandInfo, FrontendInfo, ProjectModel};
use serde_json::Value;

/// Frontend frameworks resolved from package.json dependencies.
///
/// Order matters: it is the precedence used when several frameworks are present
/// (a Vite + React project is reported as `vite`).
/// (dependency, framework, default port)
const FRONTEND_FRAMEWORKS: &[(&str, &str, u16)] = &[
    ("next", "nextjs", 3000),
    ("nuxt", "nuxt", 3000),
    ("@angular/core", "angular", 4200),
    ("react-scripts", "react", 3000),
    ("@sveltejs/kit", "sveltekit", 5173),
    ("vite", "vite", 5173),
    ("react", "react", 3000),
];

/// Backend frameworks resolved from package.json dependencies.
/// (dependency, framework, default port)
const BACKEND_FRAMEWORKS: &[(&str, &str, u16)] = &[
    ("@nestjs/core", "nestjs", 3000),
    ("express", "express", 3000),
    ("fastify", "fastify", 3000),
    ("koa", "koa", 3000),
];

/// Package manager lockfiles. (file, manager)
const LOCKFILES: &[(&str, &str)] = &[
    ("pnpm-lock.yaml", "pnpm"),
    ("yarn.lock", "yarn"),
    ("bun.lockb", "bun"),
    ("bun.lock", "bun"),
    ("package-lock.json", "npm"),
];

/// Scripts that start the project, in preference order (spec section 8)
const RUN_SCRIPTS: &[&str] = &["dev", "start", "serve"];

/// Scripts that are reported but never started automatically
const SUPPORT_SCRIPTS: &[&str] = &["build", "test", "lint"];

/// Detect Node.js projects and the framework they use
pub struct NodeDetector;

impl Detector for NodeDetector {
    fn name(&self) -> &'static str {
        "node"
    }

    fn detect(&self, project_path: &str) -> bool {
        exists(project_path, "package.json")
    }

    fn apply(&self, project_path: &str, model: &mut ProjectModel) -> Vec<String> {
        let mut evidence = vec!["package.json (node)".to_string()];
        let mut package_manager = "npm";

        if let Some((file, manager)) = LOCKFILES
            .iter()
            .find(|(file, _)| exists(project_path, file))
        {
            package_manager = manager;
            evidence.push(format!("{file} ({manager})"));
        }

        let Some(manifest) = read_text(project_path, "package.json") else {
            evidence.push("package.json is not readable; framework detection skipped".to_string());
            return evidence;
        };

        let Ok(json) = serde_json::from_str::<Value>(&manifest) else {
            evidence
                .push("package.json is not valid JSON; framework detection skipped".to_string());
            return evidence;
        };

        let dependencies = collect_dependencies(&json);
        let script_port = port_from_scripts(&json);

        if let Some((_, framework, default_port)) = resolve(FRONTEND_FRAMEWORKS, &dependencies) {
            let port = script_port.unwrap_or(*default_port);
            evidence.push(format!("frontend {framework} on port {port}"));
            model.frontend = Some(FrontendInfo::new(*framework, port));
        }

        if let Some((_, framework, default_port)) = resolve(BACKEND_FRAMEWORKS, &dependencies) {
            let port = script_port.unwrap_or(*default_port);
            evidence.push(format!("backend {framework} on port {port}"));
            model.backend = Some(BackendInfo::new(*framework, port));
        }

        collect_commands(&json, package_manager, model, &mut evidence);

        evidence
    }
}

/// Collect the commands the project declares in its scripts.
///
/// Only scripts that exist are reported, and each command records its source, so
/// the operation layer can show the user exactly what would run (spec section 19).
fn collect_commands(
    json: &Value,
    package_manager: &str,
    model: &mut ProjectModel,
    evidence: &mut Vec<String>,
) {
    let Some(scripts) = json.get("scripts").and_then(Value::as_object) else {
        return;
    };

    for script in RUN_SCRIPTS.iter().chain(SUPPORT_SCRIPTS.iter()) {
        let Some(body) = scripts.get(*script).and_then(Value::as_str) else {
            continue;
        };

        if body.trim().is_empty() {
            continue;
        }

        let command = run_command(package_manager, script);

        if !model.commands.iter().any(|known| known.name == *script) {
            model.commands.push(CommandInfo::new(
                *script,
                command.clone(),
                "package.json scripts",
            ));
            evidence.push(format!(
                "{script} command `{command}` (package.json scripts)"
            ));
        }
    }
}

/// Build the run command for a package manager and script name
fn run_command(package_manager: &str, script: &str) -> String {
    match package_manager {
        "yarn" => format!("yarn {script}"),
        "pnpm" => format!("pnpm {script}"),
        "bun" => format!("bun run {script}"),
        _ => format!("npm run {script}"),
    }
}

/// Collect dependency names from `dependencies` and `devDependencies`
fn collect_dependencies(json: &Value) -> Vec<String> {
    let mut names = Vec::new();

    for section in ["dependencies", "devDependencies"] {
        if let Some(map) = json.get(section).and_then(Value::as_object) {
            names.extend(map.keys().cloned());
        }
    }

    names
}

/// Resolve the first framework whose dependency is declared
fn resolve<'a>(
    candidates: &'a [(&'a str, &'a str, u16)],
    dependencies: &[String],
) -> Option<&'a (&'a str, &'a str, u16)> {
    candidates
        .iter()
        .find(|(dependency, ..)| dependencies.iter().any(|name| name == dependency))
}

/// Read an explicit `--port <n>` from any package.json script.
///
/// This is the only port source that is treated as certain; framework defaults
/// are documented in the evidence instead of being presented as configuration.
fn port_from_scripts(json: &Value) -> Option<u16> {
    let scripts = json.get("scripts")?.as_object()?;

    scripts.values().find_map(|script| {
        let script = script.as_str()?;
        let (_, rest) = script.split_once("--port")?;
        let digits: String = rest
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();

        digits.parse().ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(json: &str) -> Value {
        serde_json::from_str(json).expect("test manifest must be valid JSON")
    }

    #[test]
    fn resolves_next_before_react() {
        let deps = collect_dependencies(&manifest(
            r#"{"dependencies":{"react":"19.0.0","next":"15.0.0"}}"#,
        ));

        let resolved = resolve(FRONTEND_FRAMEWORKS, &deps).expect("next must resolve");

        assert_eq!(resolved.1, "nextjs");
        assert_eq!(resolved.2, 3000);
    }

    #[test]
    fn resolves_vite_and_express() {
        let deps = collect_dependencies(&manifest(
            r#"{"dependencies":{"express":"4.0.0"},"devDependencies":{"vite":"5.0.0"}}"#,
        ));

        assert_eq!(
            resolve(FRONTEND_FRAMEWORKS, &deps).map(|f| f.1),
            Some("vite")
        );
        assert_eq!(
            resolve(BACKEND_FRAMEWORKS, &deps).map(|b| b.1),
            Some("express")
        );
    }

    #[test]
    fn reads_explicit_script_port() {
        let json = manifest(r#"{"scripts":{"dev":"next dev --port 4321"}}"#);

        assert_eq!(port_from_scripts(&json), Some(4321));
        assert_eq!(
            port_from_scripts(&manifest(r#"{"scripts":{"dev":"vite"}}"#)),
            None
        );
    }

    #[test]
    fn ignores_invalid_manifests() {
        let mut model = ProjectModel::new("demo", ".");

        let evidence = NodeDetector.apply("this/path/does/not/exist", &mut model);

        assert!(model.frontend.is_none());
        assert!(evidence.iter().any(|line| line.contains("not readable")));
    }

    #[test]
    fn declares_only_scripts_that_exist() {
        let json = manifest(
            r#"{
                "scripts": {
                    "dev": "vite",
                    "build": "vite build",
                    "deploy": "custom-thing"
                }
            }"#,
        );
        let mut model = ProjectModel::new("demo", ".");
        let mut evidence = Vec::new();

        collect_commands(&json, "npm", &mut model, &mut evidence);

        let names: Vec<&str> = model
            .commands
            .iter()
            .map(|command| command.name.as_str())
            .collect();

        assert_eq!(names, vec!["dev", "build"]);
        assert_eq!(model.commands[0].command, "npm run dev");
        assert_eq!(model.commands[0].source, "package.json scripts");
        assert!(evidence.iter().any(|line| line.contains("npm run dev")));
    }

    #[test]
    fn uses_the_detected_package_manager_for_run_commands() {
        assert_eq!(run_command("npm", "dev"), "npm run dev");
        assert_eq!(run_command("yarn", "dev"), "yarn dev");
        assert_eq!(run_command("pnpm", "dev"), "pnpm dev");
        assert_eq!(run_command("bun", "dev"), "bun run dev");
    }

    #[test]
    fn does_not_declare_the_same_command_twice() {
        let json = manifest(r#"{"scripts":{"dev":"vite"}}"#);
        let mut model = ProjectModel::new("demo", ".");
        let mut evidence = Vec::new();

        collect_commands(&json, "npm", &mut model, &mut evidence);
        collect_commands(&json, "npm", &mut model, &mut evidence);

        assert_eq!(model.commands.len(), 1);
        assert_eq!(evidence.len(), 1);
    }

    #[test]
    fn a_manifest_without_scripts_declares_nothing() {
        let json = manifest(r#"{"dependencies":{"next":"15.0.0"}}"#);
        let mut model = ProjectModel::new("demo", ".");
        let mut evidence = Vec::new();

        collect_commands(&json, "npm", &mut model, &mut evidence);

        assert!(model.commands.is_empty());
        assert!(evidence.is_empty());
    }
}
