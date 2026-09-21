//! Prisma detection: ORM presence and datasource provider.

use crate::{Detector, exists, read_text};
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

        let provider = provider_from_schema(&schema);

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

/// Extract the `provider = "..."` value from a Prisma schema.
///
/// Only the `provider` inside a `datasource` block counts. Schemas also
/// contain a `generator` block with e.g. `provider = "prisma-client-js"`,
/// which must never be mistaken for the database provider.
fn provider_from_schema(schema: &str) -> Option<String> {
    let mut in_datasource = false;
    let mut depth: i32 = 0;

    for line in schema.lines() {
        if !in_datasource {
            if line.trim_start().to_lowercase().starts_with("datasource") {
                let opens = line.matches('{').count() as i32;
                let closes = line.matches('}').count() as i32;
                if opens == 0 {
                    // `datasource db` with the brace on a following line.
                    in_datasource = true;
                    depth = 0;
                } else {
                    // The block opens (and maybe closes) on this line.
                    if let Some(provider) = provider_from_line(line) {
                        return Some(provider);
                    }
                    if closes >= opens {
                        continue;
                    }
                    in_datasource = true;
                    depth = opens - closes;
                }
            }
            continue;
        }

        if let Some(provider) = provider_from_line(line) {
            return Some(provider);
        }

        depth += line.matches('{').count() as i32;
        depth -= line.matches('}').count() as i32;
        if depth <= 0 {
            in_datasource = false;
            depth = 0;
        }
    }

    None
}

/// Extract a `provider = "..."` assignment from a single line.
fn provider_from_line(line: &str) -> Option<String> {
    let (left, right) = line.split_once('=')?;

    let is_provider = left
        .trim_end()
        .to_lowercase()
        .rsplit([' ', '\t', '{'])
        .next()
        .is_some_and(|token| token == "provider");

    if !is_provider {
        return None;
    }

    // Stop at the closing quote, brace or whitespace so that both
    // `"postgresql"` and `"mysql" }` yield the bare provider name.
    let value = right
        .trim()
        .trim_start_matches('"')
        .split(|character: char| character == '"' || character == '}' || character.is_whitespace())
        .next()
        .unwrap_or_default()
        .to_lowercase();

    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_schema_presence() {
        assert!(!PrismaDetector.detect("this/path/does/not/exist"));
        assert_eq!(PrismaDetector.name(), "prisma");
    }

    #[test]
    fn reads_the_provider_from_a_multiline_schema() {
        let schema =
            "datasource db {\n  provider = \"postgresql\"\n  url      = env(\"DATABASE_URL\")\n}\n";

        assert_eq!(provider_from_schema(schema).as_deref(), Some("postgresql"));
    }

    #[test]
    fn reads_the_provider_from_a_single_line_schema() {
        let schema = "datasource db { provider = \"mysql\" }";

        assert_eq!(provider_from_schema(schema).as_deref(), Some("mysql"));
    }

    #[test]
    fn ignores_schemas_without_a_provider() {
        assert_eq!(
            provider_from_schema("model User {\n  id Int @id\n}\n"),
            None
        );
    }

    #[test]
    fn ignores_the_generator_provider() {
        let schema = "generator client {\n  provider = \"prisma-client-js\"\n}\n\ndatasource db {\n  provider = \"sqlite\"\n  url = \"file:./dev.db\"\n}\n";

        assert_eq!(provider_from_schema(schema).as_deref(), Some("sqlite"));
    }

    #[test]
    fn generator_without_datasource_yields_no_provider() {
        let schema = "generator client {\n  provider = \"prisma-client-js\"\n}\n";

        assert_eq!(provider_from_schema(schema), None);
    }

    #[test]
    fn reads_the_provider_from_a_single_line_datasource() {
        let schema = "generator client { provider = \"prisma-client-js\" }\ndatasource db { provider = \"postgresql\" }";

        assert_eq!(provider_from_schema(schema).as_deref(), Some("postgresql"));
    }
}
