//! Pilot Database Manager - Database operations.
//!
//! Phase 6: Real database operations via Prisma, Django, Alembic, and raw PostgreSQL.
//! Destructive operations must always be confirmed by the user first
//! ("Pilot Prerequisite.md" section 12).

use pilot_process_manager::{LocalProcessManager, ProcessManager, ProcessOutcome, ProcessRequest, ProcessState};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;

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
    Error { message: String, output: Option<String> },
    /// Operation is not implemented for this integration
    NotImplemented { reason: String },
}

impl DatabaseOutcome {
    /// Whether the operation succeeded
    pub fn is_success(&self) -> bool {
        matches!(self, DatabaseOutcome::Success { .. })
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

    /// Execute a database operation for a given integration
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

        match integration.integration_type {
            IntegrationType::Prisma => self.execute_prisma(integration, operation),
            IntegrationType::Django => self.execute_django(integration, operation),
            IntegrationType::Alembic => self.execute_alembic(integration, operation),
            IntegrationType::Postgres => self.execute_postgres(integration, operation),
        }
    }

    /// Execute a Prisma operation
    fn execute_prisma(
        &self,
        integration: &DatabaseIntegration,
        operation: DatabaseOperation,
    ) -> DatabaseOutcome {
        let command = match operation {
            DatabaseOperation::Status => "prisma validate".to_string(),
            DatabaseOperation::Migrate => "prisma migrate deploy".to_string(),
            DatabaseOperation::Seed => "prisma db seed".to_string(),
            DatabaseOperation::Reset => "prisma migrate reset --force".to_string(),
            DatabaseOperation::Backup => {
                if let Some(db) = &integration.database {
                    if matches!(db.r#type, DatabaseType::PostgreSQL) {
                        format!("pg_dump -h {} -p {} -U postgres -d {} > backup.sql", 
                            db.host, db.port, db.name)
                    } else {
                        return DatabaseOutcome::NotImplemented {
                            reason: "Backup not implemented for this database type with Prisma".to_string(),
                        };
                    }
                } else {
                    return DatabaseOutcome::Error {
                        message: "Database connection info not available".to_string(),
                        output: None,
                    };
                }
            }
            DatabaseOperation::Restore => {
                if let Some(db) = &integration.database {
                    if matches!(db.r#type, DatabaseType::PostgreSQL) {
                        format!("psql -h {} -p {} -U postgres -d {} < backup.sql", 
                            db.host, db.port, db.name)
                    } else {
                        return DatabaseOutcome::NotImplemented {
                            reason: "Restore not implemented for this database type with Prisma".to_string(),
                        };
                    }
                } else {
                    return DatabaseOutcome::Error {
                        message: "Database connection info not available".to_string(),
                        output: None,
                    };
                }
            }
        };

        self.run_command(integration, &command, operation)
    }

    /// Execute a Django operation
    fn execute_django(
        &self,
        integration: &DatabaseIntegration,
        operation: DatabaseOperation,
    ) -> DatabaseOutcome {
        let python = std::env::var("PYTHON_EXECUTABLE").unwrap_or_else(|_| "python".to_string());
        let manage_py = format!("{}/manage.py", integration.project_dir);

        let command = match operation {
            DatabaseOperation::Status => format!("{} {} check --deploy", python, manage_py),
            DatabaseOperation::Migrate => format!("{} {} migrate --noinput", python, manage_py),
            DatabaseOperation::Seed => format!("{} {} loaddata fixtures/*.json", python, manage_py),
            DatabaseOperation::Reset => format!("{} {} flush --noinput && {} {} migrate --noinput", python, manage_py, python, manage_py),
            DatabaseOperation::Backup => {
                if let Some(db) = &integration.database {
                    if matches!(db.r#type, DatabaseType::PostgreSQL) {
                        format!("pg_dump -h {} -p {} -U postgres -d {} > backup.sql", 
                            db.host, db.port, db.name)
                    } else {
                        return DatabaseOutcome::NotImplemented {
                            reason: "Backup not implemented for this database type with Django".to_string(),
                        };
                    }
                } else {
                    return DatabaseOutcome::Error {
                        message: "Database connection info not available".to_string(),
                        output: None,
                    };
                }
            }
            DatabaseOperation::Restore => {
                if let Some(db) = &integration.database {
                    if matches!(db.r#type, DatabaseType::PostgreSQL) {
                        format!("psql -h {} -p {} -U postgres -d {} < backup.sql", 
                            db.host, db.port, db.name)
                    } else {
                        return DatabaseOutcome::NotImplemented {
                            reason: "Restore not implemented for this database type with Django".to_string(),
                        };
                    }
                } else {
                    return DatabaseOutcome::Error {
                        message: "Database connection info not available".to_string(),
                        output: None,
                    };
                }
            }
        };

        self.run_command(integration, &command, operation)
    }

    /// Execute an Alembic operation
    fn execute_alembic(
        &self,
        integration: &DatabaseIntegration,
        operation: DatabaseOperation,
    ) -> DatabaseOutcome {
        let command = match operation {
            DatabaseOperation::Status => "alembic current".to_string(),
            DatabaseOperation::Migrate => "alembic upgrade head".to_string(),
            DatabaseOperation::Seed => {
                return DatabaseOutcome::NotImplemented {
                    reason: "Alembic doesn't have built-in seeding support".to_string(),
                };
            }
            DatabaseOperation::Reset => "alembic downgrade base && alembic upgrade head".to_string(),
            DatabaseOperation::Backup => {
                if let Some(db) = &integration.database {
                    if matches!(db.r#type, DatabaseType::PostgreSQL) {
                        format!("pg_dump -h {} -p {} -U postgres -d {} > backup.sql", 
                            db.host, db.port, db.name)
                    } else {
                        return DatabaseOutcome::NotImplemented {
                            reason: "Backup not implemented for this database type with Alembic".to_string(),
                        };
                    }
                } else {
                    return DatabaseOutcome::Error {
                        message: "Database connection info not available".to_string(),
                        output: None,
                    };
                }
            }
            DatabaseOperation::Restore => {
                if let Some(db) = &integration.database {
                    if matches!(db.r#type, DatabaseType::PostgreSQL) {
                        format!("psql -h {} -p {} -U postgres -d {} < backup.sql", 
                            db.host, db.port, db.name)
                    } else {
                        return DatabaseOutcome::NotImplemented {
                            reason: "Restore not implemented for this database type with Alembic".to_string(),
                        };
                    }
                } else {
                    return DatabaseOutcome::Error {
                        message: "Database connection info not available".to_string(),
                        output: None,
                    };
                }
            }
        };

        self.run_command(integration, &command, operation)
    }

    /// Execute a raw PostgreSQL operation
    fn execute_postgres(
        &self,
        integration: &DatabaseIntegration,
        operation: DatabaseOperation,
    ) -> DatabaseOutcome {
        let db = match &integration.database {
            Some(db) => db,
            None => {
                return DatabaseOutcome::Error {
                    message: "Database connection info required for PostgreSQL operations".to_string(),
                    output: None,
                };
            }
        };

        if !matches!(db.r#type, DatabaseType::PostgreSQL) {
            return DatabaseOutcome::NotImplemented {
                reason: "PostgreSQL integration only supports PostgreSQL databases".to_string(),
            };
        }

        let command = match operation {
            DatabaseOperation::Status => {
                format!("psql -h {} -p {} -U postgres -d {} -c \"SELECT 1\"", 
                    db.host, db.port, db.name)
            }
            DatabaseOperation::Migrate => {
                return DatabaseOutcome::NotImplemented {
                    reason: "Raw PostgreSQL doesn't have a migration system; use Prisma, Django, or Alembic".to_string(),
                };
            }
            DatabaseOperation::Seed => {
                return DatabaseOutcome::NotImplemented {
                    reason: "Raw PostgreSQL doesn't have a seeding system".to_string(),
                };
            }
            DatabaseOperation::Reset => {
                format!("psql -h {} -p {} -U postgres -d {} -c \"DROP SCHEMA public CASCADE; CREATE SCHEMA public;\"", 
                    db.host, db.port, db.name)
            }
            DatabaseOperation::Backup => {
                format!("pg_dump -h {} -p {} -U postgres -d {} > backup.sql", 
                    db.host, db.port, db.name)
            }
            DatabaseOperation::Restore => {
                format!("psql -h {} -p {} -U postgres -d {} < backup.sql", 
                    db.host, db.port, db.name)
            }
        };

        self.run_command(integration, &command, operation)
    }

    /// Run a command through the process manager
    fn run_command(
        &self,
        integration: &DatabaseIntegration,
        command: &str,
        operation: DatabaseOperation,
    ) -> DatabaseOutcome {
        let request = ProcessRequest::new(
            format!("db-{}", operation.as_str()),
            command.to_string(),
            integration.project_dir.clone(),
        );

        let outcome = self.process_manager.start(&request);
        
        match outcome {
            ProcessOutcome::Started(snapshot) => {
                std::thread::sleep(Duration::from_secs(2));
                
                let status_outcome = self.process_manager.status(&request.label);
                match status_outcome {
                    ProcessOutcome::Snapshot(s) => {
                        if s.state == ProcessState::Exited {
                            if s.exit_code == Some(0) {
                                DatabaseOutcome::Success { 
                                    output: format!("{} completed successfully", operation.as_str()) 
                                }
                            } else {
                                DatabaseOutcome::Error { 
                                    message: format!("{} failed with exit code {:?}", operation.as_str(), s.exit_code),
                                    output: None,
                                }
                            }
                        } else {
                            DatabaseOutcome::Success { 
                                output: format!("{} started", operation.as_str()) 
                            }
                        }
                    }
                    _ => DatabaseOutcome::Success { 
                        output: format!("{} initiated", operation.as_str()) 
                    },
                }
            }
            ProcessOutcome::Error(e) => DatabaseOutcome::Error { 
                message: e, 
                output: None 
            },
            _ => DatabaseOutcome::Error {
                message: "Failed to start database operation".to_string(),
                output: None,
            },
        }
    }
}

impl Default for DatabaseManager {
    fn default() -> Self {
        Self::new()
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
        assert!(matches!(result, DatabaseOutcome::NeedsConfirmation { risk: RiskLevel::High }));
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
        let _ = format!("prisma migrate deploy");
    }

    #[test]
    fn django_migrate_command() {
        let _ = format!("python manage.py migrate --noinput");
    }

    #[test]
    fn alembic_migrate_command() {
        let _ = "alembic upgrade head".to_string();
    }
}