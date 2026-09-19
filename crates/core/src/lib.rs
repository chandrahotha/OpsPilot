//! Pilot Core - Cross-Platform Project Operations Core
//!
//! Holds the platform-independent project intelligence shared by every Pilot
//! front end (CLI, GUI): the normalized [`ProjectModel`] and the [`ScanResult`]
//! produced by the scanner.

use serde::{Deserialize, Serialize};

pub mod model;

pub use model::{
    BackendInfo, CommandInfo, DatabaseInfo, DockerInfo, EnvironmentInfo, FrontendInfo, OrmInfo,
    ProjectInfo, ProjectModel,
};

/// Result of scanning a project directory
///
/// `detected` is true only when at least one capability was recognized. A plain
/// directory (or a directory that only contains unrelated files) yields
/// [`ScanResult::none`], which makes the GUI show its empty state.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ScanResult {
    /// Whether a project capability was detected
    pub detected: bool,
    /// Normalized project model if a capability was detected
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ProjectModel>,
    /// Human-readable evidence supporting the detection (file names, assumptions)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
}

impl ScanResult {
    /// Create a new ScanResult indicating no project detected
    pub fn none() -> Self {
        ScanResult {
            detected: false,
            model: None,
            evidence: Vec::new(),
        }
    }

    /// Create a new ScanResult with a project model
    pub fn with_model(model: ProjectModel) -> Self {
        ScanResult {
            detected: true,
            model: Some(model),
            evidence: Vec::new(),
        }
    }

    /// Attach the evidence that justified this scan result
    pub fn with_evidence(mut self, evidence: Vec<String>) -> Self {
        self.evidence = evidence;
        self
    }

    /// The normalized project model, if a project was detected
    pub fn model(&self) -> Option<&ProjectModel> {
        self.model.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_is_not_detected_and_has_no_model() {
        let result = ScanResult::none();

        assert!(!result.detected);
        assert!(result.model().is_none());
        assert!(result.evidence.is_empty());
    }

    #[test]
    fn with_model_marks_the_scan_as_detected() {
        let result = ScanResult::with_model(ProjectModel::new("demo", "."))
            .with_evidence(vec!["package.json".to_string()]);

        assert!(result.detected);
        assert_eq!(result.model().map(|model| model.project.name.as_str()), Some("demo"));
        assert_eq!(result.evidence, vec!["package.json".to_string()]);
    }

    #[test]
    fn empty_sections_are_not_serialized() {
        let json = serde_json::to_value(ScanResult::none()).expect("result must serialize");

        assert_eq!(json["detected"], false);
        assert!(json.get("evidence").is_none());
        assert!(json.get("model").is_none());
    }
}