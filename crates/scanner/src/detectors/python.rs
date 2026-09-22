//! Python detection: manifests, framework and database driver resolution.

use crate::{Detector, exists, read_text};
use pilot_core::{BackendInfo, CommandInfo, DatabaseInfo, OrmInfo, ProjectModel};

/// Python manifests that mark the directory as a Python project
const MANIFESTS: &[&str] = &[
    "pyproject.toml",
    "requirements.txt",
    "Pipfile",
    "poetry.lock",
    "setup.py",
];

/// Backend frameworks resolved from Python manifests. (needle, framework, port)
const BACKEND_FRAMEWORKS: &[(&str, &str, u16)] = &[
    ("django", "django", 8000),
    ("fastapi", "fastapi", 8000),
    ("flask", "flask", 5000),
];

/// Database drivers resolved from Python manifests. (needle, database, port)
const DATABASE_DRIVERS: &[(&str, &str, u16)] = &[
    ("psycopg", "postgresql", 5432),
    ("asyncpg", "postgresql", 5432),
    ("pymysql", "mysql", 3306),
    ("mysqlclient", "mysql", 3306),
    ("pymongo", "mongodb", 27017),
];

/// Detect Python projects and the framework they use
pub struct PythonDetector;

impl Detector for PythonDetector {
    fn name(&self) -> &'static str {
        "python"
    }

    fn detect(&self, project_path: &str) -> bool {
        MANIFESTS.iter().any(|file| exists(project_path, file)) || exists(project_path, "manage.py")
    }

    fn apply(&self, project_path: &str, model: &mut ProjectModel) -> Vec<String> {
        let mut evidence = Vec::new();

        for file in MANIFESTS {
            if exists(project_path, file) {
                evidence.push(format!("{file} (python)"));
            }
        }

        // A Django project is identified by manage.py even without a manifest.
        if exists(project_path, "manage.py") {
            evidence.push("manage.py (python)".to_string());
            model.backend = Some(BackendInfo::new("django", 8000));
            model.orm = Some(OrmInfo::new("django"));

            // manage.py is the canonical Django runner, so this is an explicit
            // command rather than an assumption about the framework.
            let command = "python manage.py runserver";

            if !model.commands.iter().any(|known| known.name == "dev") {
                model
                    .commands
                    .push(CommandInfo::new("dev", command, "manage.py"));
                evidence.push(format!("dev command `{command}` (manage.py)"));
            }
        }

        let manifests = MANIFESTS
            .iter()
            .filter_map(|file| read_text(project_path, file))
            .map(|content| strip_comments(&content).to_lowercase())
            .collect::<Vec<_>>();

        if manifests.is_empty() {
            return evidence;
        }

        if model.backend.is_none()
            && let Some((needle, framework, port)) = resolve(BACKEND_FRAMEWORKS, &manifests)
        {
            evidence.push(format!("backend {framework} on port {port} ({needle})"));
            model.backend = Some(BackendInfo::new(*framework, *port));
            if *framework == "django" {
                model.orm = Some(OrmInfo::new("django"));
            }
        }

        if model.database.is_none()
            && let Some((needle, database, port)) = resolve(DATABASE_DRIVERS, &manifests)
        {
            evidence.push(format!("database {database} on port {port} ({needle})"));
            model.database = Some(DatabaseInfo::new(*database, *port));
        }

        evidence
    }
}

/// Resolve the first candidate whose needle appears in any manifest
fn resolve<'a>(
    candidates: &'a [(&'a str, &'a str, u16)],
    manifests: &[String],
) -> Option<&'a (&'a str, &'a str, u16)> {
    candidates
        .iter()
        .find(|(needle, ..)| manifests.iter().any(|content| content.contains(needle)))
}

/// Strip Python-style comments (lines starting with #) from manifest content
fn strip_comments(content: &str) -> String {
    content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') {
                None
            } else {
                // Remove inline comments
                if let Some(idx) = line.find(" #") {
                    Some(line[..idx].to_string())
                } else {
                    Some(line.to_string())
                }
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_django_from_requirements() {
        let manifests = vec!["django==5.0\npsycopg[binary]==3.1\n".to_string()];

        assert_eq!(
            resolve(BACKEND_FRAMEWORKS, &manifests).map(|f| f.1),
            Some("django")
        );
        assert_eq!(
            resolve(DATABASE_DRIVERS, &manifests).map(|d| d.1),
            Some("postgresql")
        );
    }

    #[test]
    fn does_not_detect_missing_projects() {
        assert!(!PythonDetector.detect("this/path/does/not/exist"));
    }
}
