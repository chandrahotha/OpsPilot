//! Scan orchestration: run every detector and merge their findings into the
//! normalized project model.

use crate::{Detector, detectors, project_name_from_path};
use pilot_core::{ProjectModel, ScanResult};

/// Scan a project directory and build the normalized project model.
///
/// Detection is pure inspection: no command is executed and no file is written.
pub fn scan_project(project_path: impl Into<String>) -> ScanResult {
    scan_with_detectors(project_path, &detectors::default_detectors())
}

/// Scan using an explicit detector list.
///
/// This is the extension point for tests and future pluggable detectors.
pub fn scan_with_detectors(
    project_path: impl Into<String>,
    detectors: &[Box<dyn Detector>],
) -> ScanResult {
    let project_path = project_path.into();
    let mut model = ProjectModel::new(project_name_from_path(&project_path), &project_path);
    let mut evidence = Vec::new();

    for detector in detectors {
        if !detector.detect(&project_path) {
            continue;
        }

        evidence.extend(detector.apply(&project_path, &mut model));
    }

    evidence.sort();
    evidence.dedup();

    if evidence.is_empty() {
        return ScanResult::none();
    }

    ScanResult::with_model(model).with_evidence(evidence)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_directory_is_not_a_project() {
        let result = scan_with_detectors("this/path/does/not/exist", &[]);

        assert!(!result.detected);
        assert!(result.model().is_none());
        assert!(result.evidence.is_empty());
    }

    #[test]
    fn scanning_the_scanner_crate_reports_rust_evidence() {
        let result = scan_project(".");

        assert!(result.detected);
        assert!(
            result
                .evidence
                .iter()
                .any(|line| line.contains("Cargo.toml")),
            "expected Cargo.toml evidence, got {:?}",
            result.evidence
        );
        assert_eq!(
            result.model().map(|model| model.project.path.as_str()),
            Some(".")
        );
    }
}