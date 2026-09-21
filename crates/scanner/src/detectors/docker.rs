//! Docker detection: Dockerfile, Compose files and the services they declare.

use crate::{Detector, exists, read_text};
use pilot_core::{DatabaseInfo, ProjectModel};

/// Compose files recognized by Pilot. A single `compose.yml` is a Docker Compose v2 file.
const COMPOSE_FILES: &[&str] = &[
    "docker-compose.yml",
    "docker-compose.yaml",
    "compose.yml",
    "compose.yaml",
];

/// Database images commonly declared in Compose files. (image, database, port)
const DATABASE_IMAGES: &[(&str, &str, u16)] = &[
    ("postgres", "postgresql", 5432),
    ("mysql", "mysql", 3306),
    ("mariadb", "mysql", 3306),
    ("mongo", "mongodb", 27017),
];

/// Detect a Docker project from a Dockerfile
pub struct DockerfileDetector;

impl Detector for DockerfileDetector {
    fn name(&self) -> &'static str {
        "dockerfile"
    }

    fn detect(&self, project_path: &str) -> bool {
        exists(project_path, "Dockerfile")
    }

    fn apply(&self, _project_path: &str, model: &mut ProjectModel) -> Vec<String> {
        model.docker.get_or_insert_default().detected = true;

        vec!["Dockerfile (docker)".to_string()]
    }
}

/// Detect Docker Compose and the database service it declares
pub struct DockerComposeDetector;

impl Detector for DockerComposeDetector {
    fn name(&self) -> &'static str {
        "docker-compose"
    }

    fn detect(&self, project_path: &str) -> bool {
        COMPOSE_FILES.iter().any(|file| exists(project_path, file))
    }

    fn apply(&self, project_path: &str, model: &mut ProjectModel) -> Vec<String> {
        let Some(file) = COMPOSE_FILES.iter().find(|file| exists(project_path, file)) else {
            return Vec::new();
        };

        let mut evidence = vec![format!("{file} (docker compose)")];
        let docker = model.docker.get_or_insert_default();
        docker.detected = true;
        docker.compose = true;

        if model.database.is_none()
            && let Some(compose) = read_text(project_path, file)
        {
            let lowered = compose.to_lowercase();
            if let Some((image, database, port)) = DATABASE_IMAGES
                .iter()
                .find(|(image, ..)| lowered.contains(image))
            {
                evidence.push(format!("compose service image {image} on port {port}"));
                model.database = Some(DatabaseInfo::new(*database, *port));
            }
        }

        evidence
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dockerfile_detector_marks_docker_only() {
        let mut model = ProjectModel::new("demo", ".");

        let evidence = DockerfileDetector.apply(".", &mut model);

        let docker = model.docker.expect("docker section must be present");
        assert!(docker.detected);
        assert!(!docker.compose);
        assert_eq!(evidence, vec!["Dockerfile (docker)".to_string()]);
    }

    #[test]
    fn compose_detector_without_files_changes_nothing() {
        let mut model = ProjectModel::new("demo", ".");

        let evidence = DockerComposeDetector.apply("this/path/does/not/exist", &mut model);

        assert!(evidence.is_empty());
        assert!(model.docker.is_none());
    }
}
