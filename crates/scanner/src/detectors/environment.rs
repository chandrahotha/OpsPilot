//! Environment configuration detection.
//!
//! Only the presence of environment files is recorded; values are never read or
//! exposed (see "Pilot Prerequisite.md" section 15: never display secret values).

use crate::{Detector, exists};
use pilot_core::{EnvironmentInfo, ProjectModel};

/// Detect .env / .env.example / .env.local presence
pub struct EnvironmentDetector;

const ENV_FILES: &[(&str, bool, bool, bool)] = &[
    // (file, env_file, env_example, env_local)
    (".env", true, false, false),
    (".env.example", false, true, false),
    (".env.sample", false, true, false),
    (".env.local", false, false, true),
];

impl Detector for EnvironmentDetector {
    fn name(&self) -> &'static str {
        "environment"
    }

    fn detect(&self, project_path: &str) -> bool {
        ENV_FILES
            .iter()
            .any(|(file, ..)| exists(project_path, file))
    }

    fn apply(&self, project_path: &str, model: &mut ProjectModel) -> Vec<String> {
        let mut evidence = Vec::new();
        let mut info = EnvironmentInfo::default();

        for (file, env_file, env_example, env_local) in ENV_FILES {
            if exists(project_path, file) {
                info.env_file |= env_file;
                info.env_example |= env_example;
                info.env_local |= env_local;
                evidence.push(format!("{file} found"));
            }
        }

        if !evidence.is_empty() {
            model.environment = Some(info);
        }

        evidence
    }
}
