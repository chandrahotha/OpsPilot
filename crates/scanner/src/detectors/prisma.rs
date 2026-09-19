//! Prisma detection: ORM presence and datasource provider.

use crate::{exists, read_text, Detector};
use pilot_core::{DatabaseInfo, OrmInfo, ProjectModel};

/// Prisma datasource providers mapped to the normalized database model.
/// (provider, database, port) - SQLite has no network port and is reported as evidence only.
const PROVIDERS: &[(&str, &str, u16)] = &[
    ("postgresql", "postgresql", 5432),
    ("mysql", "mysql", 3306),
    ("mongodb", "mongodb", 27017),
    ("sqlserver", "sqlserver", 1433),
];

/// Detect Prisma by presence of the Prisma schema
pub struct PrismaDetector;

impl Detector for PrismaDetector {
    fn name(&self) -> &'static str {
        "prisma"
    }

    fn detect(&self, project_path: &str) -> bool {
        exists(project_path, "prisma/schema.prisma")
    }

    fn apply(&self, project_path: &str, model: &mut ProjectModel) -> Vec<String> {
        let mut evidence = vec!["prisma/schema.prisma (prisma)".to_string()];
        model.orm = Some(OrmInfo::new("prisma"));

        let Some(schema) = read_text(project_path, "prisma/schema.prisma") else {
            evidence.push("prisma schema is not readable; provider detection skipped".to_string());
            return evidence;
        };

        let provider = schema
            .lines()
            .map(|line| line.trim_start().to_lowercase())
            .find_map(|line| {
                if !line.starts_with("provider") {
                    return None;
                }
                let (_, value) = line.split_once('=')?;
                Some(value.trim().trim_matches('"').to_string())
            });

        match provider.as_deref() {
            Some("sqlite") => {
                evidence.push("datasource provider sqlite (no network port)".to_string());
            }
            Some(provider) => {
                if let Some((_, database, port)) =
                    PROVIDERS.iter().find(|(name, ..)| *name == provider)
                {
                    evidence.push(format!("datasource provider {database} on port {port}"));
                    model.database = Some(DatabaseInfo::new(*database, *port));
                } else {
                    evidence.push(format!("datasource provider {provider} (not mapped)"));
                }
            }
            None => evidence.push("datasource provider not found in schema".to_string()),
        }

        evidence
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_schema_presence() {
        assert!(!PrismaDetector.detect("this/path/does/not/exist"));
        assert_eq!(PrismaDetector.name(), "prisma");
    }
}