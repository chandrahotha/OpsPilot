// Pilot Project Model - Normalized project structure detected by the scanner
//
// All detectors must produce a normalized internal project model.
// The GUI should consume this model instead of directly scanning the filesystem.
//
// The serialized form is the normalized project model defined in
// "Pilot Prerequisite.md" section 7: camelCase JSON keys, e.g.
//
// {
//   "project":     { "name": "my-ai-app", "path": "..." },
//   "frontend":    { "framework": "nextjs", "port": 3000 },
//   "backend":     { "framework": "fastapi", "port": 8000 },
//   "database":    { "type": "postgresql", "port": 5432 },
//   "orm":         { "type": "prisma" },
//   "docker":      { "detected": true, "compose": true },
//   "environment": { "envFile": true, "envExample": true }
// }

use serde::{Deserialize, Serialize};

/// Project information including name and path
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectInfo {
    /// Project name
    pub name: String,
    /// Project path
    pub path: String,
}

impl ProjectInfo {
    /// Create project information from a name and a path
    pub fn new(name: impl Into<String>, path: impl Into<String>) -> Self {
        ProjectInfo {
            name: name.into(),
            path: path.into(),
        }
    }
}

/// Frontend framework and port information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrontendInfo {
    /// Frontend framework (nextjs, vite, react, etc.)
    pub framework: String,
    /// Server port
    pub port: u16,
}

impl FrontendInfo {
    /// Create frontend information for a framework and port
    pub fn new(framework: impl Into<String>, port: u16) -> Self {
        FrontendInfo {
            framework: framework.into(),
            port,
        }
    }
}

/// Backend framework and port information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackendInfo {
    /// Backend framework (fastapi, express, django, etc.)
    pub framework: String,
    /// Server port
    pub port: u16,
}

impl BackendInfo {
    /// Create backend information for a framework and port
    pub fn new(framework: impl Into<String>, port: u16) -> Self {
        BackendInfo {
            framework: framework.into(),
            port,
        }
    }
}

/// Database type and port information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseInfo {
    /// Database type (postgresql, mysql, sqlite, etc.)
    pub r#type: String,
    /// Database port
    pub port: u16,
}

impl DatabaseInfo {
    /// Create database information for a type and port
    pub fn new(r#type: impl Into<String>, port: u16) -> Self {
        DatabaseInfo {
            r#type: r#type.into(),
            port,
        }
    }
}

/// ORM type information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrmInfo {
    /// ORM type (prisma, django, alembic, etc.)
    pub r#type: String,
}

impl OrmInfo {
    /// Create ORM information for an ORM type
    pub fn new(r#type: impl Into<String>) -> Self {
        OrmInfo {
            r#type: r#type.into(),
        }
    }
}

/// A command the project declares, ready to be executed by Pilot.
///
/// Pilot only runs commands it can show the user first
/// ("Pilot Prerequisite.md" sections 8 and 19): every entry records where it was
/// found, and no command is invented from a detected framework alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandInfo {
    /// Stable command name (dev, start, build, test, lint)
    pub name: String,
    /// The exact command line Pilot would execute
    pub command: String,
    /// Where the command was found (package.json scripts, manage.py, ...)
    pub source: String,
}

impl CommandInfo {
    /// Create command information
    pub fn new(name: impl Into<String>, command: impl Into<String>, source: impl Into<String>) -> Self {
        CommandInfo {
            name: name.into(),
            command: command.into(),
            source: source.into(),
        }
    }
}

/// Docker detection status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DockerInfo {
    /// Whether Docker was detected
    pub detected: bool,
    /// Whether Docker Compose is detected
    pub compose: bool,
}

impl DockerInfo {
    /// Create Docker information from detection flags
    pub fn new(detected: bool, compose: bool) -> Self {
        DockerInfo { detected, compose }
    }
}

/// Environment configuration information
///
/// Field names are snake_case in Rust and serialized as camelCase
/// (`envFile`, `envExample`, `envLocal`) to match the normalized project model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentInfo {
    /// Whether .env file is present
    pub env_file: bool,
    /// Whether .env.example file is present
    pub env_example: bool,
    /// Whether .env.local file is present
    pub env_local: bool,
}

impl EnvironmentInfo {
    /// Create environment information from presence flags
    pub fn new(env_file: bool, env_example: bool, env_local: bool) -> Self {
        EnvironmentInfo {
            env_file,
            env_example,
            env_local,
        }
    }
}

/// Complete normalized project model produced by detectors
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectModel {
    /// Project name and path
    pub project: ProjectInfo,
    /// Frontend framework and port
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frontend: Option<FrontendInfo>,
    /// Backend framework and port
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<BackendInfo>,
    /// Database type and configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<DatabaseInfo>,
    /// ORM type
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orm: Option<OrmInfo>,
    /// Docker detection status
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docker: Option<DockerInfo>,
    /// Environment configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<EnvironmentInfo>,
    /// Commands the project declares
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<CommandInfo>,
}

impl ProjectModel {
    /// Create an empty project model for a project name and path
    pub fn new(name: impl Into<String>, path: impl Into<String>) -> Self {
        ProjectModel {
            project: ProjectInfo::new(name, path),
            frontend: None,
            backend: None,
            database: None,
            orm: None,
            docker: None,
            environment: None,
            commands: Vec::new(),
        }
    }

    /// Whether any capability besides the project identity was detected
    pub fn has_capabilities(&self) -> bool {
        self.frontend.is_some()
            || self.backend.is_some()
            || self.database.is_some()
            || self.orm.is_some()
            || self.docker.is_some()
            || self.environment.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_model() -> ProjectModel {
        ProjectModel {
            project: ProjectInfo::new("my-ai-app", "C:/projects/my-ai-app"),
            frontend: Some(FrontendInfo::new("nextjs", 3000)),
            backend: Some(BackendInfo::new("fastapi", 8000)),
            database: Some(DatabaseInfo::new("postgresql", 5432)),
            orm: Some(OrmInfo::new("prisma")),
            docker: Some(DockerInfo::new(true, true)),
            environment: Some(EnvironmentInfo::new(true, true, false)),
            commands: vec![CommandInfo::new("dev", "npm run dev", "package.json scripts")],
        }
    }

    #[test]
    fn serializes_with_normalized_camel_case_keys() {
        let json = serde_json::to_value(sample_model()).expect("model must serialize");

        assert_eq!(json["project"]["name"], "my-ai-app");
        assert_eq!(json["frontend"]["framework"], "nextjs");
        assert_eq!(json["frontend"]["port"], 3000);
        assert_eq!(json["backend"]["framework"], "fastapi");
        assert_eq!(json["backend"]["port"], 8000);
        assert_eq!(json["database"]["type"], "postgresql");
        assert_eq!(json["database"]["port"], 5432);
        assert_eq!(json["orm"]["type"], "prisma");
        assert_eq!(json["docker"]["detected"], true);
        assert_eq!(json["docker"]["compose"], true);
        assert_eq!(json["environment"]["envFile"], true);
        assert_eq!(json["environment"]["envExample"], true);
        assert_eq!(json["environment"]["envLocal"], false);
    }

    #[test]
    fn omits_undetected_sections() {
        let model = ProjectModel::new("plain", ".");
        let json = serde_json::to_value(&model).expect("model must serialize");

        assert!(json.get("frontend").is_none());
        assert!(json.get("backend").is_none());
        assert!(json.get("database").is_none());
        assert!(json.get("orm").is_none());
        assert!(json.get("docker").is_none());
        assert!(json.get("environment").is_none());
        assert!(!model.has_capabilities());
    }

    #[test]
    fn round_trips_through_json() {
        let model = sample_model();
        let json = serde_json::to_string(&model).expect("model must serialize");
        let parsed: ProjectModel = serde_json::from_str(&json).expect("model must deserialize");

        assert_eq!(model, parsed);
        assert!(parsed.has_capabilities());
    }

    #[test]
    fn tolerates_missing_optional_keys() {
        let parsed: ProjectModel =
            serde_json::from_str(r#"{"project":{"name":"x","path":"."}}"#).expect("must parse");

        assert_eq!(parsed.project.name, "x");
        assert!(parsed.frontend.is_none());
        assert!(parsed.commands.is_empty());
    }

    #[test]
    fn declared_commands_serialize_with_camel_case_keys() {
        let mut model = ProjectModel::new("demo", ".");
        model.commands.push(CommandInfo::new(
            "dev",
            "npm run dev",
            "package.json scripts",
        ));

        let json = serde_json::to_value(&model).expect("model must serialize");
        let command = &json["commands"][0];

        assert_eq!(command["name"], "dev");
        assert_eq!(command["command"], "npm run dev");
        assert_eq!(command["source"], "package.json scripts");
    }

    #[test]
    fn empty_command_list_is_omitted() {
        let json = serde_json::to_value(ProjectModel::new("demo", ".")).expect("must serialize");

        assert!(json.get("commands").is_none());
    }
}