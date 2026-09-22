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

        // Check for explicit packageManager field in package.json (standard field)
        let manifest_for_pm = read_text(project_path, "package.json");
        if let Some(manifest) = &manifest_for_pm {
            if let Ok(json) = serde_json::from_str::<Value>(manifest) {
                if let Some(pm) = json.get("packageManager").and_then(|v| v.as_str()) {
                    if pm.starts_with("pnpm@") {
                        package_manager = "pnpm";
                    } else if pm.starts_with("yarn@") {
                        package_manager = "yarn";
                    } else if pm.starts_with("bun@") {
                        package_manager = "bun";
                    } else if pm.starts_with("npm@") {
                        package_manager = "npm";
                    }
                    evidence.push(format!("packageManager field: {pm}"));
                }
            }
        }

        // Fall back to lockfile detection if no packageManager field
        if package_manager == "npm" {
            if let Some((file, manager)) = LOCKFILES
                .iter()
                .find(|(file, _)| exists(project_path, file))
            {
                package_manager = manager;
                evidence.push(format!("{file} ({manager})"));
            }
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
            let port = script_port
                .or_else(|| port_from_vite_config(project_path))
                .unwrap_or(*default_port);
            evidence.push(format!("frontend {framework} on port {port}"));
            model.frontend = Some(FrontendInfo::new(*framework, port));
        }

        if let Some((_, framework, default_port)) = resolve(BACKEND_FRAMEWORKS, &dependencies) {
            let port = script_port
                .or_else(|| port_from_env(project_path))
                .unwrap_or(*default_port);
            evidence.push(format!("backend {framework} on port {port}"));
            model.backend = Some(BackendInfo::new(*framework, port));
        }

        // Query builders also count as the project's data layer: Kysely with an
        // embedded database (e.g. `node:sqlite`) has no migration CLI, so the
        // operation layer reports file-based guidance instead of guessing.
        if model.orm.is_none() && dependencies.iter().any(|name| name == "kysely") {
            evidence.push("kysely (query builder)".to_string());
            model.orm = Some(pilot_core::OrmInfo::new("kysely"));
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

/// Read `PORT=<n>` from `.env` (then `.env.example`).
///
/// Only the `PORT` key is read — values of any other key (secrets included)
/// are never inspected. This catches frameworks like NestJS whose port comes
/// from the environment rather than the command line.
///
/// Monorepo services (e.g. `backend/`) typically read the env file from the
/// repository root, so the parent directory is checked as a fallback.
fn port_from_env(project_path: &str) -> Option<u16> {
    let mut roots = vec![project_path.to_string()];
    if let Some(parent) = std::path::Path::new(project_path).parent() {
        let parent = parent.to_string_lossy().to_string();
        if parent != project_path {
            roots.push(parent);
        }
    }

    for root in &roots {
        for file in [".env", ".env.example"] {
            if let Some(content) = read_text(root, file) {
                for line in content.lines() {
                    let line = line.trim();
                    let Some(rest) = line.strip_prefix("PORT=") else {
                        continue;
                    };
                    let digits: String = rest
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .chars()
                        .take_while(|c| c.is_ascii_digit())
                        .collect();
                    if let Ok(port) = digits.parse::<u16>() {
                        if port != 0 {
                            return Some(port);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Read `port: <n>` from `vite.config.ts` / `vite.config.js`.
///
/// Vite projects declare their dev port in config, not in scripts, so the
/// framework default (5173) is wrong whenever the project overrides it.
fn port_from_vite_config(project_path: &str) -> Option<u16> {
    for file in ["vite.config.ts", "vite.config.js"] {
        if let Some(content) = read_text(project_path, file) {
            for line in content.lines() {
                let Some((_, rest)) = line.split_once("port") else {
                    continue;
                };
                let Some((_, rest)) = rest.split_once(':') else {
                    continue;
                };
                let digits: String = rest
                    .trim()
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                if let Ok(port) = digits.parse::<u16>() {
                    if port != 0 {
                        return Some(port);
                    }
                }
            }
        }
    }
    None
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
    fn reads_backend_port_from_env_files() {
        let dir = std::env::temp_dir().join(format!("pilot-node-env-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("fixture dir must be created");
        std::fs::write(dir.join(".env.example"), "PORT=6041\nJWT_SECRET=irrelevant\n").expect("env must be written");

        let path = dir.to_string_lossy().to_string();
        assert_eq!(port_from_env(&path), Some(6041));

        // `.env` wins over `.env.example`.
        std::fs::write(dir.join(".env"), "PORT=7001\n").expect("env must be written");
        assert_eq!(port_from_env(&path), Some(7001));

        // Comments and secrets never leak into port detection: the commented
        // PORT in `.env` is ignored, so `.env.example` still provides 6041.
        std::fs::write(dir.join(".env"), "# PORT=9999\nDATABASE_URL=postgres://x:5432/db\n").expect("env must be written");
        assert_eq!(port_from_env(&path), Some(6041));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detects_kysely_as_the_data_layer() {
        let dir = std::env::temp_dir().join(format!("pilot-node-kysely-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("fixture dir must be created");
        std::fs::write(
            dir.join("package.json"),
            r#"{"dependencies":{"@nestjs/core":"11.0.0","kysely":"0.29.4"}}"#,
        )
        .expect("manifest must be written");

        let path = dir.to_string_lossy().to_string();
        let mut model = ProjectModel::new("demo", &path);
        let evidence = NodeDetector.apply(&path, &mut model);

        assert_eq!(
            model.orm.as_ref().map(|orm| orm.r#type.as_str()),
            Some("kysely")
        );
        assert!(evidence.iter().any(|line| line.contains("kysely")));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_frontend_port_from_vite_config() {
        let dir = std::env::temp_dir().join(format!("pilot-node-vite-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("fixture dir must be created");
        std::fs::write(dir.join("vite.config.ts"), "export default defineConfig({\n  server: {\n    port: 6040,\n  },\n});\n").expect("config must be written");

        let path = dir.to_string_lossy().to_string();
        assert_eq!(port_from_vite_config(&path), Some(6040));

        let _ = std::fs::remove_dir_all(&dir);
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
