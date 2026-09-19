//! Rust project detection.
//!
//! Rust projects are reported as evidence only: the normalized project model has
//! no Rust section, and a Cargo project does not by itself define an application
//! port. This detector exists so the toolchain is visible in diagnostics.

use crate::{exists, Detector};
use pilot_core::ProjectModel;

/// Detect a Cargo (Rust) project
pub struct RustDetector;

impl Detector for RustDetector {
    fn name(&self) -> &'static str {
        "rust"
    }

    fn detect(&self, project_path: &str) -> bool {
        exists(project_path, "Cargo.toml")
    }

    fn apply(&self, _project_path: &str, _model: &mut ProjectModel) -> Vec<String> {
        vec!["Cargo.toml (cargo)".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_cargo_manifest() {
        // The scanner's own crate root always contains a Cargo.toml.
        assert!(RustDetector.detect("."));
        assert_eq!(RustDetector.name(), "rust");
    }
}