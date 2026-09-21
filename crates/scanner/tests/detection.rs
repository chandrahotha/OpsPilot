//! Detection tests for the supported project stacks.

mod common;

use common::Fixture;

#[test]
fn detects_a_nextjs_prisma_docker_project() {
    let fixture = Fixture::new("nextjs");
    fixture
        .file(
            "package.json",
            r#"{
                "name": "my-ai-app",
                "scripts": { "dev": "next dev" },
                "dependencies": { "next": "15.0.0", "react": "19.0.0" }
            }"#,
        )
        .file("package-lock.json", "{}")
        .file(
            "prisma/schema.prisma",
            "datasource db {\n  provider = \"postgresql\"\n}\n",
        )
        .file(
            "docker-compose.yml",
            "services:\n  db:\n    image: postgres:16\n",
        )
        .file(".env", "DATABASE_URL=postgres://localhost:5432/db\n")
        .file(".env.example", "DATABASE_URL=\n");

    let result = fixture.scan();

    assert!(result.detected, "evidence: {:?}", result.evidence);
    let model = result.model().expect("model must be present");

    assert_eq!(model.project.name, fixture.name());
    assert_eq!(
        model.frontend.as_ref().map(|f| f.framework.as_str()),
        Some("nextjs")
    );
    assert_eq!(model.frontend.as_ref().map(|f| f.port), Some(3000));
    assert_eq!(
        model.orm.as_ref().map(|orm| orm.r#type.as_str()),
        Some("prisma")
    );
    assert_eq!(
        model.database.as_ref().map(|db| db.r#type.as_str()),
        Some("postgresql")
    );

    let docker = model.docker.expect("docker must be detected");
    assert!(docker.detected && docker.compose);

    let environment = model.environment.expect("environment must be detected");
    assert!(environment.env_file && environment.env_example);

    assert!(result.evidence.iter().any(|line| line.contains("nextjs")));
    assert!(
        result
            .evidence
            .iter()
            .any(|line| line.contains("package-lock.json"))
    );
}

#[test]
fn reads_an_explicit_port_from_the_dev_script() {
    let fixture = Fixture::new("explicit-port");
    fixture.file(
        "package.json",
        r#"{"scripts":{"dev":"vite --port 4100"},"devDependencies":{"vite":"5.0.0"}}"#,
    );

    let result = fixture.scan();
    let model = result.model().expect("model must be present");

    assert_eq!(model.frontend.as_ref().map(|f| f.port), Some(4100));
}

#[test]
fn detects_a_django_project_from_manage_py() {
    let fixture = Fixture::new("django");
    fixture
        .file("manage.py", "#!/usr/bin/env python\n")
        .file(
            "requirements.txt",
            "Django==5.0\npsycopg[binary]==3.1\nalembic==1.13\n",
        )
        .file("alembic.ini", "[alembic]\n");

    let result = fixture.scan();
    let model = result.model().expect("model must be present");

    assert_eq!(
        model.backend.as_ref().map(|b| b.framework.as_str()),
        Some("django")
    );
    assert_eq!(model.backend.as_ref().map(|b| b.port), Some(8000));
    assert_eq!(
        model.database.as_ref().map(|db| db.r#type.as_str()),
        Some("postgresql")
    );
    assert!(
        result
            .evidence
            .iter()
            .any(|line| line.contains("manage.py"))
    );
    assert!(result.evidence.iter().any(|line| line.contains("alembic")));
}

#[test]
fn detects_a_fastapi_project_from_pyproject() {
    let fixture = Fixture::new("fastapi");
    fixture.file(
        "pyproject.toml",
        "[project]\nname = \"api\"\ndependencies = [\"fastapi\"]\n",
    );

    let result = fixture.scan();
    let model = result.model().expect("model must be present");

    assert_eq!(
        model.backend.as_ref().map(|b| b.framework.as_str()),
        Some("fastapi")
    );
    assert!(model.frontend.is_none());
}

#[test]
fn an_empty_directory_reports_no_project() {
    let fixture = Fixture::new("empty");

    let result = fixture.scan();

    assert!(!result.detected);
    assert!(result.model().is_none());
    assert!(result.evidence.is_empty());
}

#[test]
fn a_docker_only_project_is_detected_without_frameworks() {
    let fixture = Fixture::new("docker-only");
    fixture
        .file("Dockerfile", "FROM node:22-alpine\n")
        .file("compose.yaml", "services:\n  cache:\n    image: redis:7\n");

    let result = fixture.scan();
    let model = result.model().expect("model must be present");

    assert!(model.frontend.is_none());
    assert!(model.backend.is_none());
    assert!(model.database.is_none());
    assert!(model.docker.is_some_and(|d| d.detected && d.compose));
}
