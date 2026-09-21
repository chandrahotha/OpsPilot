//! Tests for the scan result contract consumed by the GUI.

mod common;

use common::Fixture;
use pilot_scanner::scan_with_detectors;

#[test]
fn scan_with_detectors_allows_an_empty_detector_list() {
    let fixture = Fixture::new("no-detectors");
    fixture.file("package.json", r#"{"dependencies":{"next":"15.0.0"}}"#);

    let result = scan_with_detectors(fixture.path().to_string_lossy().to_string(), &[]);

    assert!(!result.detected);
}

#[test]
fn scan_result_serializes_for_the_gui() {
    let fixture = Fixture::new("serialize");
    fixture.file(
        "package.json",
        r#"{"dependencies":{"next":"15.0.0"},"devDependencies":{"typescript":"5.6.0"}}"#,
    );

    let json = serde_json::to_value(fixture.scan()).expect("scan result must serialize");

    assert_eq!(json["detected"], true);
    assert_eq!(json["model"]["frontend"]["framework"], "nextjs");
    assert_eq!(json["model"]["frontend"]["port"], 3000);
    assert_eq!(json["model"]["project"]["name"], fixture.name());
    assert!(json["model"].get("backend").is_none());
    assert!(json["evidence"].is_array());
}

#[test]
fn evidence_is_deterministic_and_not_repeated() {
    let fixture = Fixture::new("evidence");
    fixture
        .file("package.json", r#"{"dependencies":{"next":"15.0.0"}}"#)
        .file("Dockerfile", "FROM node:22-alpine\n")
        .file(
            "docker-compose.yml",
            "services:\n  db:\n    image: postgres:16\n",
        );

    let first = fixture.scan();
    let second = fixture.scan();

    assert_eq!(first.evidence, second.evidence);
    let mut sorted = first.evidence.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted, first.evidence);
}
