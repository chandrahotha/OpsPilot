//! Pilot Scanner - Project detection engine
//!
//! Scans a project directory and identifies the project stack (Node.js, Python,
//! Django, Prisma, Alembic, Docker, ...). Detection is modular: every stack is
//! described by an independent [`Detector`] implementation, and all detectors
//! together produce the normalized [`ProjectModel`] defined by `pilot-core`.
//!
//! The scanner only inspects files. It never executes project commands, which is
//! what makes it safe to run against an unfamiliar directory
//! (see "Pilot Prerequisite.md" section 19).

use std::fs;
use std::path::PathBuf;

pub mod detectors;
pub mod scan;

pub use pilot_core::ScanResult;
pub use scan::{scan_project, scan_with_detectors};

/// A single stack detector.
///
/// Implementations must stay independent from each other: a detector only
/// enriches the project model with the capability it owns, so a new stack can be
/// added without touching existing detectors.
pub trait Detector {
    /// Stable detector name, reported as evidence and used in diagnostics
    fn name(&self) -> &'static str;

    /// Whether this detector recognizes the project at `project_path`
    fn detect(&self, project_path: &str) -> bool;

    /// Enrich the project model and return the evidence that justified it.
    ///
    /// Evidence must describe facts found in the project (file names, declared
    /// dependencies, documented defaults), never assumptions presented as facts.
    fn apply(&self, project_path: &str, model: &mut ProjectModel) -> Vec<String>;
}

use pilot_core::ProjectModel;

/// Extract the project name from the last path segment of a directory path
pub fn project_name_from_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_end_matches('/')
        .rsplit('/')
        .find(|segment| !segment.is_empty() && *segment != ".")
        .unwrap_or("unknown-project")
        .to_string()
}

/// Whether a path relative to the project root exists
pub fn exists(project_path: &str, relative: &str) -> bool {
    fs::metadata(join(project_path, relative)).is_ok()
}

/// Read a text file relative to the project root, if it exists and is readable
pub fn read_text(project_path: &str, relative: &str) -> Option<String> {
    fs::read_to_string(join(project_path, relative)).ok()
}

/// Join a project root with a path relative to it, using the host separator
fn join(project_path: &str, relative: &str) -> PathBuf {
    PathBuf::from(project_path)
        .join(relative.trim_start_matches(['/', '\\']))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_name_is_the_last_path_segment() {
        assert_eq!(project_name_from_path("C:/projects/my-ai-app"), "my-ai-app");
        assert_eq!(project_name_from_path("C:\\projects\\my-ai-app"), "my-ai-app");
        assert_eq!(project_name_from_path("/home/dev/my-ai-app/"), "my-ai-app");
        assert_eq!(project_name_from_path("."), "unknown-project");
    }

    #[test]
    fn existence_checks_do_not_panic_on_missing_paths() {
        assert!(!exists("this/path/does/not/exist", "package.json"));
        assert!(read_text("this/path/does/not/exist", "package.json").is_none());
    }

    #[test]
    fn reads_files_beside_the_manifest() {
        // The scanner's own crate root always contains a Cargo.toml.
        assert!(exists(".", "Cargo.toml"));
        assert!(read_text(".", "Cargo.toml").is_some_and(|text| text.contains("pilot-scanner")));
    }
}