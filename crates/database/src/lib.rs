//! Pilot Database Manager - Database operations.
//!
//! Phase 6: Real database operations via Prisma, Django, Alembic, and raw PostgreSQL.
//! Destructive operations must always be confirmed by the user first
//! ("Pilot Prerequisite.md" section 12).

use pilot_process_manager::{
    LocalProcessManager, ProcessManager, ProcessOutcome, ProcessRequest, ProcessState,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How long a database operation may run before Pilot stops waiting for it.
/// Migrations on large schemas can take minutes; the loop polls the process
/// state instead of sleeping a fixed amount of time.
const OPERATION_TIMEOUT: Duration = Duration::from_secs(300);
/// How often the process state is polled while waiting for completion.
const POLL_INTERVAL: Duration = Duration::from_millis(200);
/// How many trailing log lines are returned with an operation outcome.
const LOG_TAIL_LINES: usize = 50;

/// Database operation type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DatabaseOperation {
    /// Check database status / connectivity
    Status,
    /// Run migrations
    Migrate,
    /// Seed database
    Seed,
    /// Reset database (destructive)
    Reset,
    /// Backup database
    Backup,
    /// Restore database (destructive)
    Restore,
}

impl DatabaseOperation {
    /// Whether the operation can destroy project data
    pub fn is_destructive(self) -> bool {
        matches!(self, DatabaseOperation::Reset | DatabaseOperation::Restore)
    }

    /// Risk level shown in the confirmation dialog
    pub fn risk(self) -> RiskLevel {
        match self {
            DatabaseOperation::Reset | DatabaseOperation::Restore => RiskLevel::High,
            DatabaseOperation::Status | DatabaseOperation::Backup => RiskLevel::Low,
            DatabaseOperation::Migrate | DatabaseOperation::Seed => RiskLevel::Medium,
        }
    }

    /// Get a string representation for logging
    pub fn as_str(&self) -> &'static str {
        match self {
            DatabaseOperation::Status => "status",
            DatabaseOperation::Migrate => "migrate",
            DatabaseOperation::Seed => "seed",
            DatabaseOperation::Reset => "reset",
            DatabaseOperation::Backup => "backup",
            DatabaseOperation::Restore => "restore",
        }
    }
}

/// Risk level used by the confirmation dialog
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RiskLevel {
    /// Read-only operation
    Low,
    /// Operation that changes data
    Medium,
    /// Operation that can destroy data
    High,
}

/// Database type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DatabaseType {
    /// PostgreSQL
    PostgreSQL,
    /// MySQL
    MySQL,
    /// MongoDB
    MongoDB,
    /// SQLite
    SQLite,
}

/// Database connection information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseInfo {
    /// Database type
    pub r#type: DatabaseType,
    /// Connection host
    pub host: String,
    /// Connection port
    pub port: u16,
    /// Database name
    pub name: String,
}

/// Database integration type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IntegrationType {
    /// Prisma ORM
    Prisma,
    /// Django ORM
    Django,
    /// Alembic migrations
    Alembic,
    /// Raw PostgreSQL (psql)
    Postgres,
    /// Embedded SQLite file database (e.g. Kysely + `node:sqlite`).
    ///
    /// There is no server to start and no migration CLI to run: the schema
    /// migrates inside the backend process on boot. Operations are file
    /// operations or honest guidance, never invented commands.
    Sqlite,
}

/// Database integration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseIntegration {
    /// Type of integration
    pub integration_type: IntegrationType,
    /// Project directory
    pub project_dir: String,
    /// Database connection info (if available)
    pub database: Option<DatabaseInfo>,
    /// Environment variables
    pub env: HashMap<String, String>,
}

/// Outcome of a database operation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DatabaseOutcome {
    /// Operation completed successfully
    Success { output: String },
    /// Operation requires user confirmation first
    NeedsConfirmation { risk: RiskLevel },
    /// Operation failed with error
    Error {
        message: String,
        output: Option<String>,
    },
    /// Operation is not implemented for this integration
    NotImplemented { reason: String },
}

impl DatabaseOutcome {
    /// Whether the operation succeeded
    pub fn is_success(&self) -> bool {
        matches!(self, DatabaseOutcome::Success { .. })
    }
}

use std::sync::mpsc;

/// Handle for a running database operation
pub struct DatabaseOperationHandle {
    receiver: mpsc::Receiver<DatabaseOutcome>,
    label: String,
    process_manager: LocalProcessManager,
}

impl DatabaseOperationHandle {
    /// Wait for the operation to complete, blocking until done
    pub fn wait(self) -> DatabaseOutcome {
        self.receiver.recv().unwrap_or_else(|_| DatabaseOutcome::Error {
            message: "operation channel disconnected".to_string(),
            output: None,
        })
    }

    /// Try to get the result without blocking
    pub fn try_wait(&self) -> Option<DatabaseOutcome> {
        self.receiver.try_recv().ok()
    }

    /// Get the process label for log access
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Get access to the process manager for log retrieval
    pub fn process_manager(&self) -> &LocalProcessManager {
        &self.process_manager
    }
}

/// Database manager that executes operations via the process manager
pub struct DatabaseManager {
    process_manager: LocalProcessManager,
}

impl DatabaseManager {
    /// Create a new database manager
    pub fn new() -> Self {
        Self {
            process_manager: LocalProcessManager::new(),
        }
    }

    /// Execute a database operation for a given integration (blocking for backward compatibility)
    pub fn execute(
        &self,
        integration: &DatabaseIntegration,
        operation: DatabaseOperation,
    ) -> DatabaseOutcome {
        if operation.is_destructive() {
            return DatabaseOutcome::NeedsConfirmation {
                risk: operation.risk(),
            };
        }

        self.execute_async(integration, operation).wait()
    }

    /// Execute a database operation asynchronously, returning a handle that can be polled
    pub fn execute_async(
        &self,
        integration: &DatabaseIntegration,
        operation: DatabaseOperation,
    ) -> DatabaseOperationHandle {
        let (tx, rx) = mpsc::channel();
        let integration = integration.clone();
        let process_manager = self.process_manager.clone();
        let label = format!("db-{}", operation.as_str());

        std::thread::spawn(move || {
            let outcome = Self::run_command_static(&process_manager, &integration, operation);
            let _ = tx.send(outcome);
        });

        DatabaseOperationHandle {
            receiver: rx,
            label,
            process_manager: self.process_manager.clone(),
        }
    }

    /// Execute a database operation, bypassing the destructive-operation gate.
    ///
    /// The caller must have obtained explicit user confirmation already
    /// (the GUI confirms destructive operations before invoking with
    /// `confirmed=true`). Non-destructive operations behave exactly like
    /// [`DatabaseManager::execute`].
    pub fn execute_confirmed(
        &self,
        integration: &DatabaseIntegration,
        operation: DatabaseOperation,
    ) -> DatabaseOutcome {
        self.execute_confirmed_async(integration, operation).wait()
    }

    /// Execute a database operation asynchronously, bypassing the destructive-operation gate
    pub fn execute_confirmed_async(
        &self,
        integration: &DatabaseIntegration,
        operation: DatabaseOperation,
    ) -> DatabaseOperationHandle {
        let (tx, rx) = mpsc::channel();
        let integration = integration.clone();
        let process_manager = self.process_manager.clone();
        let label = format!("db-{}", operation.as_str());

        std::thread::spawn(move || {
            let outcome = Self::run_command_static(&process_manager, &integration, operation);
            let _ = tx.send(outcome);
        });

        DatabaseOperationHandle {
            receiver: rx,
            label,
            process_manager: self.process_manager.clone(),
        }
    }

    /// Internal static method to run a command (used by async handlers)
    fn run_command_static(
        process_manager: &LocalProcessManager,
        integration: &DatabaseIntegration,
        operation: DatabaseOperation,
    ) -> DatabaseOutcome {
        if matches!(integration.integration_type, IntegrationType::Sqlite) {
            return Self::execute_sqlite(integration, operation);
        }

        let command = match integration.integration_type {
            IntegrationType::Prisma => Self::prisma_command(integration, operation),
            IntegrationType::Django => Self::django_command(integration, operation),
            IntegrationType::Alembic => Self::alembic_command(integration, operation),
            IntegrationType::Postgres => Self::postgres_command(integration, operation),
            IntegrationType::Sqlite => unreachable!("handled above"),
        };

        match command {
            Ok(cmd) => Self::run_command_impl(process_manager, integration, &cmd, operation),
            Err(outcome) => outcome,
        }
    }

    fn prisma_command(
        integration: &DatabaseIntegration,
        operation: DatabaseOperation,
    ) -> Result<String, DatabaseOutcome> {
        match operation {
            DatabaseOperation::Status => Ok("prisma validate".to_string()),
            DatabaseOperation::Migrate => Ok("prisma migrate deploy".to_string()),
            DatabaseOperation::Seed => Ok("prisma db seed".to_string()),
            DatabaseOperation::Reset => Ok("prisma migrate reset --force".to_string()),
            DatabaseOperation::Backup => {
                if let Some(db) = &integration.database {
                    if matches!(db.r#type, DatabaseType::PostgreSQL) {
                        Ok(format!(
                            "pg_dump -h {} -p {} -U postgres -d {} > backup.sql",
                            db.host, db.port, db.name
                        ))
                    } else {
                        Err(DatabaseOutcome::NotImplemented {
                            reason: "Backup not implemented for this database type with Prisma".to_string(),
                        })
                    }
                } else {
                    Err(DatabaseOutcome::Error {
                        message: "Database connection info not available".to_string(),
                        output: None,
                    })
                }
            }
            DatabaseOperation::Restore => {
                if let Some(db) = &integration.database {
                    if matches!(db.r#type, DatabaseType::PostgreSQL) {
                        Ok(format!(
                            "psql -h {} -p {} -U postgres -d {} < backup.sql",
                            db.host, db.port, db.name
                        ))
                    } else {
                        Err(DatabaseOutcome::NotImplemented {
                            reason: "Restore not implemented for this database type with Prisma".to_string(),
                        })
                    }
                } else {
                    Err(DatabaseOutcome::Error {
                        message: "Database connection info not available".to_string(),
                        output: None,
                    })
                }
            }
        }
    }

    fn django_command(
        integration: &DatabaseIntegration,
        operation: DatabaseOperation,
    ) -> Result<String, DatabaseOutcome> {
        let python = std::env::var("PYTHON_EXECUTABLE").unwrap_or_else(|_| "python".to_string());
        let manage_py = format!("{}/manage.py", integration.project_dir);

        match operation {
            DatabaseOperation::Status => Ok(format!("{} {} check --deploy", python, manage_py)),
            DatabaseOperation::Migrate => Ok(format!("{} {} migrate --noinput", python, manage_py)),
            DatabaseOperation::Seed => Ok(format!("{} {} loaddata fixtures/*.json", python, manage_py)),
            DatabaseOperation::Reset => Ok(format!(
                "{} {} flush --noinput && {} {} migrate --noinput",
                python, manage_py, python, manage_py
            )),
            DatabaseOperation::Backup => {
                if let Some(db) = &integration.database {
                    if matches!(db.r#type, DatabaseType::PostgreSQL) {
                        Ok(format!(
                            "pg_dump -h {} -p {} -U postgres -d {} > backup.sql",
                            db.host, db.port, db.name
                        ))
                    } else {
                        Err(DatabaseOutcome::NotImplemented {
                            reason: "Backup not implemented for this database type with Django".to_string(),
                        })
                    }
                } else {
                    Err(DatabaseOutcome::Error {
                        message: "Database connection info not available".to_string(),
                        output: None,
                    })
                }
            }
            DatabaseOperation::Restore => {
                if let Some(db) = &integration.database {
                    if matches!(db.r#type, DatabaseType::PostgreSQL) {
                        Ok(format!(
                            "psql -h {} -p {} -U postgres -d {} < backup.sql",
                            db.host, db.port, db.name
                        ))
                    } else {
                        Err(DatabaseOutcome::NotImplemented {
                            reason: "Restore not implemented for this database type with Django".to_string(),
                        })
                    }
                } else {
                    Err(DatabaseOutcome::Error {
                        message: "Database connection info not available".to_string(),
                        output: None,
                    })
                }
            }
        }
    }

    fn alembic_command(
        _integration: &DatabaseIntegration,
        operation: DatabaseOperation,
    ) -> Result<String, DatabaseOutcome> {
        match operation {
            DatabaseOperation::Status => Ok("alembic current".to_string()),
            DatabaseOperation::Migrate => Ok("alembic upgrade head".to_string()),
            DatabaseOperation::Seed => Err(DatabaseOutcome::NotImplemented {
                reason: "Alembic doesn't have built-in seeding support".to_string(),
            }),
            DatabaseOperation::Reset => Ok("alembic downgrade base && alembic upgrade head".to_string()),
            DatabaseOperation::Backup => {
                if let Some(db) = &_integration.database {
                    if matches!(db.r#type, DatabaseType::PostgreSQL) {
                        Ok(format!(
                            "pg_dump -h {} -p {} -U postgres -d {} > backup.sql",
                            db.host, db.port, db.name
                        ))
                    } else {
                        Err(DatabaseOutcome::NotImplemented {
                            reason: "Backup not implemented for this database type with Alembic".to_string(),
                        })
                    }
                } else {
                    Err(DatabaseOutcome::Error {
                        message: "Database connection info not available".to_string(),
                        output: None,
                    })
                }
            }
            DatabaseOperation::Restore => {
                if let Some(db) = &_integration.database {
                    if matches!(db.r#type, DatabaseType::PostgreSQL) {
                        Ok(format!(
                            "psql -h {} -p {} -U postgres -d {} < backup.sql",
                            db.host, db.port, db.name
                        ))
                    } else {
                        Err(DatabaseOutcome::NotImplemented {
                            reason: "Restore not implemented for this database type with Alembic".to_string(),
                        })
                    }
                } else {
                    Err(DatabaseOutcome::Error {
                        message: "Database connection info not available".to_string(),
                        output: None,
                    })
                }
            }
        }
    }

    fn postgres_command(
        integration: &DatabaseIntegration,
        operation: DatabaseOperation,
    ) -> Result<String, DatabaseOutcome> {
        let db = match &integration.database {
            Some(db) => db,
            None => {
                return Err(DatabaseOutcome::Error {
                    message: "Database connection info required for PostgreSQL operations".to_string(),
                    output: None,
                });
            }
        };

        if !matches!(db.r#type, DatabaseType::PostgreSQL) {
            return Err(DatabaseOutcome::NotImplemented {
                reason: "PostgreSQL integration only supports PostgreSQL databases".to_string(),
            });
        }

        match operation {
            DatabaseOperation::Status => Ok(format!(
                "psql -h {} -p {} -U postgres -d {} -c \"SELECT 1\"",
                db.host, db.port, db.name
            )),
            DatabaseOperation::Migrate => Err(DatabaseOutcome::NotImplemented {
                reason: "Raw PostgreSQL doesn't have a migration system; use Prisma, Django, or Alembic".to_string(),
            }),
            DatabaseOperation::Seed => Err(DatabaseOutcome::NotImplemented {
                reason: "Raw PostgreSQL doesn't have a seeding system".to_string(),
            }),
            DatabaseOperation::Reset => Ok(format!(
                "psql -h {} -p {} -U postgres -d {} -c \"DROP SCHEMA public CASCADE; CREATE SCHEMA public;\"",
                db.host, db.port, db.name
            )),
            DatabaseOperation::Backup => Ok(format!(
                "pg_dump -h {} -p {} -U postgres -d {} > backup.sql",
                db.host, db.port, db.name
            )),
            DatabaseOperation::Restore => Ok(format!(
                "psql -h {} -p {} -U postgres -d {} < backup.sql",
                db.host, db.port, db.name
            )),
        }
    }

    /// File extensions that mark an embedded SQLite database.
    const SQLITE_EXTENSIONS: &[&str] = &["db", "sqlite", "sqlite3", "db3"];

    /// Directories never descended into while looking for database files.
    const SQLITE_SKIPPED_DIRS: &[&str] = &[
        "node_modules",
        "target",
        "dist",
        "build",
        ".git",
        ".next",
        "coverage",
        ".turbo",
    ];

    /// Find SQLite database files under a project directory.
    ///
    /// The walk is depth-limited and skips dependency/build output so a
    /// status check stays fast even in large monorepos.
    fn find_sqlite_files(project_dir: &str) -> Vec<std::path::PathBuf> {
        fn visit(dir: &std::path::Path, depth: u8, out: &mut Vec<std::path::PathBuf>) {
            if depth > 4 {
                return;
            }
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let skip = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| {
                            crate::DatabaseManager::SQLITE_SKIPPED_DIRS.contains(&name)
                        });
                    if !skip {
                        visit(&path, depth + 1, out);
                    }
                } else if path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| {
                        crate::DatabaseManager::SQLITE_EXTENSIONS
                            .contains(&ext.to_lowercase().as_str())
                    })
                {
                    out.push(path);
                }
            }
        }

        let mut files = Vec::new();
        visit(std::path::Path::new(project_dir), 0, &mut files);
        files.sort();
        files
    }

    /// Execute a file-based SQLite database operation.
    ///
    /// There is no server and no migration CLI: the backend process creates,
    /// migrates and seeds the file on boot. Status and backup inspect the
    /// files; migrate/seed point at the backend instead of pretending.
    fn execute_sqlite(
        integration: &DatabaseIntegration,
        operation: DatabaseOperation,
    ) -> DatabaseOutcome {
        const BOOT_GUIDANCE: &str = "file-based databases migrate and seed automatically when the backend boots; start the backend and look for 'Migrations completed' in its logs";

        match operation {
            DatabaseOperation::Status => {
                let files = Self::find_sqlite_files(&integration.project_dir);
                if files.is_empty() {
                    DatabaseOutcome::Error {
                        message: format!(
                            "no SQLite database file found under {}; {BOOT_GUIDANCE}",
                            integration.project_dir
                        ),
                        output: None,
                    }
                } else {
                    let list = files
                        .iter()
                        .map(|path| {
                            let size = std::fs::metadata(path)
                                .map(|meta| meta.len())
                                .unwrap_or(0);
                            format!(
                                "{} ({} bytes)",
                                path.strip_prefix(&integration.project_dir)
                                    .unwrap_or(path)
                                    .to_string_lossy(),
                                size
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    DatabaseOutcome::Success {
                        output: format!("SQLite database file(s):\n{list}"),
                    }
                }
            }
            DatabaseOperation::Migrate | DatabaseOperation::Seed => {
                DatabaseOutcome::NotImplemented {
                    reason: format!(
                        "{} has no CLI for SQLite; {BOOT_GUIDANCE}",
                        operation.as_str()
                    ),
                }
            }
            DatabaseOperation::Backup => {
                let files = Self::find_sqlite_files(&integration.project_dir);
                let Some(source) = files.first() else {
                    return DatabaseOutcome::Error {
                        message: "no SQLite database file found to back up".to_string(),
                        output: None,
                    };
                };
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_secs())
                    .unwrap_or(0);
                let backup = source.with_extension(format!("backup-{timestamp}.db"));
                match std::fs::copy(source, &backup) {
                    Ok(_) => DatabaseOutcome::Success {
                        output: format!("backed up to {}", backup.to_string_lossy()),
                    },
                    Err(error) => DatabaseOutcome::Error {
                        message: format!("backup failed: {error}"),
                        output: None,
                    },
                }
            }
            DatabaseOperation::Reset => {
                let files = Self::find_sqlite_files(&integration.project_dir);
                if files.is_empty() {
                    return DatabaseOutcome::Error {
                        message: "no SQLite database file found to reset".to_string(),
                        output: None,
                    };
                }
                let mut removed = Vec::new();
                for file in &files {
                    // Remove journal artifacts alongside the main file.
                    for extra in [
                        file.with_extension("db-wal"),
                        file.with_extension("db-shm"),
                        file.with_extension("db-journal"),
                    ] {
                        let _ = std::fs::remove_file(&extra);
                    }
                    match std::fs::remove_file(file) {
                        Ok(()) => removed.push(file.to_string_lossy().to_string()),
                        Err(error) => {
                            return DatabaseOutcome::Error {
                                message: format!(
                                    "could not remove {}: {error}",
                                    file.to_string_lossy()
                                ),
                                output: None,
                            };
                        }
                    }
                }
                DatabaseOutcome::Success {
                    output: format!(
                        "removed {};\nrestart the backend to recreate it (migrations + seed run on boot)",
                        removed.join(", ")
                    ),
                }
            }
            DatabaseOperation::Restore => DatabaseOutcome::NotImplemented {
                reason: "automatic restore is not supported for SQLite; stop the backend, copy a *.backup-*.db file back over the live database file, then start the backend".to_string(),
            },
        }
    }

    /// Run a command through the process manager and wait for it to finish.
    ///
    /// The process state is polled until it exits (or the timeout elapses)
    /// instead of sleeping a fixed amount of time, and the trailing log
    /// output is captured so the caller sees what actually happened.
    fn run_command(
        &self,
        integration: &DatabaseIntegration,
        command: &str,
        operation: DatabaseOperation,
    ) -> DatabaseOutcome {
        Self::run_command_impl(&self.process_manager, integration, command, operation)
    }

    fn run_command_impl(
        process_manager: &LocalProcessManager,
        integration: &DatabaseIntegration,
        command: &str,
        operation: DatabaseOperation,
    ) -> DatabaseOutcome {
        let request = ProcessRequest::new(
            format!("db-{}", operation.as_str()),
            command.to_string(),
            integration.project_dir.clone(),
        );

        let outcome = process_manager.start(&request);

        match outcome {
            ProcessOutcome::Started(_snapshot) => {
                let deadline = Instant::now() + OPERATION_TIMEOUT;
                loop {
                    match process_manager.status(&request.label) {
                        ProcessOutcome::Snapshot(s) => {
                            if s.state == ProcessState::Exited {
                                let logs = Self::tail_logs_static(process_manager, &request.label);
                                if s.exit_code == Some(0) {
                                    return DatabaseOutcome::Success {
                                        output: if logs.is_empty() {
                                            format!("{} completed successfully", operation.as_str())
                                        } else {
                                            logs
                                        },
                                    };
                                }
                                return DatabaseOutcome::Error {
                                    message: {
                                        let base = format!(
                                            "{} failed with exit code {}",
                                            operation.as_str(),
                                            fmt_exit(s.exit_code)
                                        );
                                        match missing_tool_hint(command, &logs) {
                                            Some(hint) => format!("{base}. {hint}"),
                                            None => base,
                                        }
                                    },
                                    output: Some(logs),
                                };
                            }
                        }
                        ProcessOutcome::NotFound(label) => {
                            return DatabaseOutcome::Error {
                                message: format!(
                                    "lost track of the {} process ({label})",
                                    operation.as_str()
                                ),
                                output: Some(Self::tail_logs_static(process_manager, &request.label)),
                            };
                        }
                        ProcessOutcome::Error(e) => {
                            return DatabaseOutcome::Error {
                                message: e,
                                output: Some(Self::tail_logs_static(process_manager, &request.label)),
                            };
                        }
                        _ => {}
                    }

                    if Instant::now() >= deadline {
                        let _ = process_manager.stop(&request.label);
                        return DatabaseOutcome::Error {
                            message: format!(
                                "{} timed out after {}s and was stopped",
                                operation.as_str(),
                                OPERATION_TIMEOUT.as_secs()
                            ),
                            output: Some(Self::tail_logs_static(process_manager, &request.label)),
                        };
                    }

                    std::thread::sleep(POLL_INTERVAL);
                }
            }
            ProcessOutcome::Error(e) => DatabaseOutcome::Error {
                message: e,
                output: None,
            },
            other => DatabaseOutcome::Error {
                message: format!(
                    "could not start {} (unexpected process outcome: {})",
                    operation.as_str(),
                    outcome_name(&other),
                ),
                output: None,
            },
        }
    }

    /// Last lines of captured stdout/stderr for a database operation.
    fn tail_logs(&self, label: &str) -> String {
        Self::tail_logs_static(&self.process_manager, label)
    }

    fn tail_logs_static(process_manager: &LocalProcessManager, label: &str) -> String {
        process_manager
            .log_buffer(label)
            .map(|buffer| {
                buffer
                    .snapshot(Some(LOG_TAIL_LINES))
                    .iter()
                    .map(|entry| entry.message.clone())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
    }
}

impl Default for DatabaseManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Human-readable exit code for messages: `1`, never `Some(1)`.
fn fmt_exit(code: Option<i32>) -> String {
    code.map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Actionable hint when the command's own tool is not installed.
///
/// Detected from the captured output (not the exit code alone, which could
/// mean anything the tool itself reported). Returns `None` when the tool ran
/// and failed on its own terms.
fn missing_tool_hint(command: &str, logs: &str) -> Option<String> {
    let tool = command
        .split_whitespace()
        .next()
        .unwrap_or("the required tool");

    // Check for missing tool errors more precisely:
    // - Look for the tool name in the error message
    // - Check for specific error patterns that indicate "command not found"
    let tool_lower = tool.to_lowercase();
    let logs_lower = logs.to_lowercase();

    // Patterns that strongly indicate "command not found"
    let missing_patterns = [
        format!("{tool_lower}: command not found"),
        format!("{tool_lower} : command not found"),
        format!("'{tool_lower}' is not recognized"),
        format!("{tool_lower} is not recognized"),
        format!("command not found: {tool_lower}"),
        format!("executable file not found: {tool_lower}"),
        format!("{tool_lower}: not found"),
    ];

    let has_missing_pattern = missing_patterns.iter().any(|p| logs_lower.contains(p));

    // Also check generic patterns but only if the tool name appears nearby
    let generic_missing = (logs_lower.contains("command not found") || logs_lower.contains("not recognized as an internal or external command"))
        && logs_lower.contains(&tool_lower);

    if !has_missing_pattern && !generic_missing {
        return None;
    }

    Some(format!(
        "`{tool}` is not installed or not on PATH; install the client, or run the database with `docker compose up`"
    ))
}

/// Short name of a process outcome for error messages.
fn outcome_name(outcome: &ProcessOutcome) -> &'static str {
    match outcome {
        ProcessOutcome::Started(_) => "started",
        ProcessOutcome::Snapshot(_) => "snapshot",
        ProcessOutcome::Stopped(_) => "stopped",
        ProcessOutcome::NotFound(_) => "not-found",
        ProcessOutcome::Error(_) => "error",
    }
}

/// High-level function to execute a database operation
pub fn execute(integration: &DatabaseIntegration, operation: DatabaseOperation) -> DatabaseOutcome {
    let manager = DatabaseManager::new();
    manager.execute(integration, operation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destructive_operations_are_flagged() {
        assert!(DatabaseOperation::Reset.is_destructive());
        assert!(DatabaseOperation::Restore.is_destructive());
        assert!(!DatabaseOperation::Migrate.is_destructive());
        assert!(!DatabaseOperation::Status.is_destructive());
    }

    #[test]
    fn destructive_operations_require_confirmation() {
        let integration = DatabaseIntegration {
            integration_type: IntegrationType::Prisma,
            project_dir: ".".to_string(),
            database: None,
            env: HashMap::new(),
        };

        let manager = DatabaseManager::new();
        let result = manager.execute(&integration, DatabaseOperation::Reset);
        assert!(matches!(
            result,
            DatabaseOutcome::NeedsConfirmation {
                risk: RiskLevel::High
            }
        ));
    }

    fn sqlite_integration(project_dir: &str) -> DatabaseIntegration {
        DatabaseIntegration {
            integration_type: IntegrationType::Sqlite,
            project_dir: project_dir.to_string(),
            database: None,
            env: HashMap::new(),
        }
    }

    fn sqlite_fixture(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pilot-sqlite-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("fixture dir must be created");
        dir
    }

    #[test]
    fn sqlite_status_reports_found_database_files() {
        let dir = sqlite_fixture("status");
        std::fs::write(dir.join("app.db"), b"fake-sqlite").expect("db file must be written");
        std::fs::create_dir_all(dir.join("node_modules")).expect("dir must be created");
        std::fs::write(dir.join("node_modules").join("ignored.db"), b"nope")
            .expect("db file must be written");

        let outcome = DatabaseManager::execute_sqlite(
            &sqlite_integration(&dir.to_string_lossy()),
            DatabaseOperation::Status,
        );

        match outcome {
            DatabaseOutcome::Success { output } => {
                assert!(output.contains("app.db"), "got: {output}");
                assert!(!output.contains("ignored.db"), "got: {output}");
            }
            other => panic!("expected Success, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sqlite_status_without_a_file_explains_boot_behavior() {
        let dir = sqlite_fixture("missing");

        let outcome = DatabaseManager::execute_sqlite(
            &sqlite_integration(&dir.to_string_lossy()),
            DatabaseOperation::Status,
        );

        match outcome {
            DatabaseOutcome::Error { message, .. } => {
                assert!(message.contains("Migrations completed"), "got: {message}");
            }
            other => panic!("expected Error, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sqlite_migrate_and_seed_point_at_the_backend() {
        let integration = sqlite_integration(".");

        for operation in [DatabaseOperation::Migrate, DatabaseOperation::Seed] {
            match DatabaseManager::execute_sqlite(&integration, operation) {
                DatabaseOutcome::NotImplemented { reason } => {
                    assert!(reason.contains("backend"), "got: {reason}");
                }
                other => panic!("expected NotImplemented, got {other:?}"),
            }
        }
    }

    #[test]
    fn sqlite_backup_copies_the_database_file() {
        let dir = sqlite_fixture("backup");
        std::fs::write(dir.join("app.db"), b"fake-sqlite").expect("db file must be written");

        let outcome = DatabaseManager::execute_sqlite(
            &sqlite_integration(&dir.to_string_lossy()),
            DatabaseOperation::Backup,
        );

        match outcome {
            DatabaseOutcome::Success { output } => {
                assert!(output.contains("backed up to"), "got: {output}");
                let backups: Vec<_> = std::fs::read_dir(&dir)
                    .expect("dir must list")
                    .flatten()
                    .filter(|entry| {
                        entry
                            .file_name()
                            .to_string_lossy()
                            .starts_with("app.backup-")
                    })
                    .collect();
                assert_eq!(backups.len(), 1);
            }
            other => panic!("expected Success, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sqlite_reset_removes_the_database_file() {
        let dir = sqlite_fixture("reset");
        std::fs::write(dir.join("app.db"), b"fake-sqlite").expect("db file must be written");

        let outcome = DatabaseManager::execute_sqlite(
            &sqlite_integration(&dir.to_string_lossy()),
            DatabaseOperation::Reset,
        );

        match outcome {
            DatabaseOutcome::Success { output } => {
                assert!(output.contains("restart the backend"), "got: {output}");
                assert!(!dir.join("app.db").exists());
            }
            other => panic!("expected Success, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_destructive_operations_dont_require_confirmation() {
        let integration = DatabaseIntegration {
            integration_type: IntegrationType::Prisma,
            project_dir: ".".to_string(),
            database: None,
            env: HashMap::new(),
        };

        let manager = DatabaseManager::new();
        let result = manager.execute(&integration, DatabaseOperation::Status);
        assert!(!matches!(result, DatabaseOutcome::NeedsConfirmation { .. }));
    }

    #[test]
    fn status_is_low_risk() {
        assert_eq!(DatabaseOperation::Status.risk(), RiskLevel::Low);
        assert_eq!(DatabaseOperation::Reset.risk(), RiskLevel::High);
    }

    #[test]
    fn prisma_migrate_command() {
        let _ = "prisma migrate deploy";
    }

    #[test]
    fn django_migrate_command() {
        let _ = "python manage.py migrate --noinput";
    }

    #[test]
    fn alembic_migrate_command() {
        let _ = "alembic upgrade head".to_string();
    }

    #[test]
    fn exit_codes_format_without_rust_debug_syntax() {
        assert_eq!(fmt_exit(Some(1)), "1");
        assert_eq!(fmt_exit(Some(0)), "0");
        assert_eq!(fmt_exit(None), "unknown");
    }

    #[test]
    fn missing_windows_tool_is_detected_with_guidance() {
        let logs = "'psql' is not recognized as an internal or external command,\noperable program or batch file.";

        let hint = missing_tool_hint("psql -h localhost -c \"SELECT 1\"", logs)
            .expect("must detect the missing tool");

        assert!(hint.contains("`psql` is not installed"));
        assert!(hint.contains("docker compose up"));
    }

    #[test]
    fn missing_unix_tool_is_detected_with_guidance() {
        let hint = missing_tool_hint("pg_dump -h localhost", "pg_dump: command not found")
            .expect("must detect the missing tool");

        assert!(hint.contains("`pg_dump` is not installed"));
    }

    #[test]
    fn tool_failures_are_not_mistaken_for_missing_tools() {
        assert_eq!(
            missing_tool_hint(
                "psql -h localhost",
                "psql: FATAL: database \"x\" does not exist"
            ),
            None
        );
        assert_eq!(missing_tool_hint("prisma migrate deploy", ""), None);
    }
}
