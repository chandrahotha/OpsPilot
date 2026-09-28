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

    // 1. Scan root directory
    for detector in detectors {
        if !detector.detect(&project_path) {
            continue;
        }

        evidence.extend(detector.apply(&project_path, &mut model));
    }

    // 2. Scan standard monorepo / subfolder paths if directories exist
    const SUBDIRS: &[&str] = &[
        "frontend",
        "client",
        "web",
        "ui",
        "backend",
        "server",
        "api",
        "srv",
        "apps/web",
        "apps/client",
        "apps/frontend",
        "apps/ui",
        "apps/api",
        "apps/backend",
        "apps/server",
    ];

    for rel_sub in SUBDIRS {
        let sub_path = std::path::Path::new(&project_path).join(rel_sub);
        if !sub_path.is_dir() {
            continue;
        }
        let sub_str = sub_path.to_string_lossy();
        for detector in detectors {
            if !detector.detect(&sub_str) {
                continue;
            }

            let mut sub_model = ProjectModel::new(project_name_from_path(&sub_str), &*sub_str);
            let sub_evidence = detector.apply(&sub_str, &mut sub_model);

            if model.frontend.is_none() && sub_model.frontend.is_some() {
                model.frontend = sub_model.frontend;
            }
            if model.backend.is_none() && sub_model.backend.is_some() {
                model.backend = sub_model.backend;
            }
            if model.database.is_none() && sub_model.database.is_some() {
                model.database = sub_model.database;
            }
            if model.orm.is_none() && sub_model.orm.is_some() {
                model.orm = sub_model.orm;
            }
            if model.docker.is_none() && sub_model.docker.is_some() {
                model.docker = sub_model.docker;
            }

            let is_be = matches!(
                *rel_sub,
                "backend" | "server" | "api" | "srv" | "apps/api" | "apps/backend" | "apps/server"
            );
            let is_fe = matches!(
                *rel_sub,
                "frontend"
                    | "client"
                    | "web"
                    | "ui"
                    | "apps/web"
                    | "apps/client"
                    | "apps/frontend"
                    | "apps/ui"
            );

            for mut cmd in sub_model.commands {
                cmd.source = format!("{rel_sub}/{}", cmd.source);
                if is_be && (cmd.name == "dev" || cmd.name == "start" || cmd.name == "serve") {
                    cmd.name = format!("{}:backend", cmd.name);
                } else if is_fe && (cmd.name == "dev" || cmd.name == "start" || cmd.name == "serve")
                {
                    cmd.name = format!("{}:frontend", cmd.name);
                }
                if !model.commands.iter().any(|c| c.name == cmd.name) {
                    model.commands.push(cmd);
                }
            }

            for ev in sub_evidence {
                // Don't prefix with subdirectory since evidence already contains file paths
                evidence.push(ev);
            }
        }
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
