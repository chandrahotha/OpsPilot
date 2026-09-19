//! Alembic migration detection.

use crate::{exists, Detector};
use pilot_core::{OrmInfo, ProjectModel};

/// Detect Alembic by presence of alembic.ini
pub struct AlembicDetector;

impl Detector for AlembicDetector {
    fn name(&self) -> &'static str {
        "alembic"
    }

    fn detect(&self, project_path: &str) -> bool {
        exists(project_path, "alembic.ini")
    }

    fn apply(&self, _project_path: &str, model: &mut ProjectModel) -> Vec<String> {
        model.orm = Some(OrmInfo::new("alembic"));
        vec!["alembic.ini (alembic)".to_string()]
    }
}