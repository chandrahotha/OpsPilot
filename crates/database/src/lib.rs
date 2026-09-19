//! Pilot Database Manager - Database operations.
//!
//! PHASE STATUS: database operations arrive in phase 6 and run through explicit
//! integrations (Prisma, Django, Alembic). Destructive operations must always be
//! confirmed by the user first ("Pilot Prerequisite.md" section 12).

/// Database operation type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseOperation {
    /// Check database status
    Status,
    /// Run migrations
    Migrate,
    /// Seed database
    Seed,
    /// Reset database
    Reset,
    /// Backup database
    Backup,
    /// Restore database
    Restore,
}

impl DatabaseOperation {
    /// Whether the operation can destroy project data
    ///
    /// Destructive operations require an explicit confirmation before execution.
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
}

/// Risk level used by the confirmation dialog
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    /// Read-only operation
    Low,
    /// Operation that changes data
    Medium,
    /// Operation that can destroy data
    High,
}

/// Database type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Outcome of a database operation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatabaseOutcome {
    /// The operation completed
    Done,
    /// The operation requires user confirmation first
    NeedsConfirmation(RiskLevel),
    /// The operation is not implemented yet, with the reason
    NotImplemented(&'static str),
}

/// Execute a database operation for an ORM integration
///
/// PHASE STATUS: unimplemented. Execution needs the process layer (phase 4) and
/// the integration commands (phase 6).
pub fn execute(operation: DatabaseOperation) -> DatabaseOutcome {
    if operation.is_destructive() {
        return DatabaseOutcome::NeedsConfirmation(RiskLevel::High);
    }

    DatabaseOutcome::NotImplemented("database operations arrive in phase 6")
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
        assert_eq!(
            execute(DatabaseOperation::Reset),
            DatabaseOutcome::NeedsConfirmation(RiskLevel::High)
        );
    }

    #[test]
    fn non_destructive_operations_report_their_phase() {
        assert_eq!(
            execute(DatabaseOperation::Migrate),
            DatabaseOutcome::NotImplemented("database operations arrive in phase 6")
        );
    }

    #[test]
    fn status_is_the_lowest_risk_operation() {
        assert_eq!(DatabaseOperation::Status.risk(), RiskLevel::Low);
        assert_eq!(DatabaseOperation::Reset.risk(), RiskLevel::High);
    }
}