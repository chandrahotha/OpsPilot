//! End-to-end tests for the `pilot` CLI.
//!
//! The tests run the real binary against throwaway fixture directories, so they
//! do not depend on the developer's machine configuration.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A throwaway directory that removes itself when dropped
struct Fixture {
    path: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("pilot-cli-{name}-{}-{unique}", std::process::id()));

        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("fixture directory must be created");

        Fixture { path }
    }

    fn file(&self, relative: &str, content: &str) -> &Self {
        fs::write(self.path.join(relative), content).expect("fixture file must be written");

        self
    }

    fn path_str(&self) -> String {
        self.path.to_string_lossy().to_string()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_pilot"))
        .args(args)
        .output()
        .expect("pilot binary must run")
}

#[test]
fn prints_the_detected_model_as_json() {
    let fixture = Fixture::new("json");
    fixture.file(
        "package.json",
        r#"{"scripts":{"dev":"next dev"},"dependencies":{"next":"15.0.0"}}"#,
    );

    let output = run(&[&fixture.path_str(), "--json"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must be json");

    assert_eq!(json["detected"], true);
    assert_eq!(json["model"]["frontend"]["framework"], "nextjs");
    assert_eq!(json["model"]["frontend"]["port"], 3000);
}

#[test]
fn reports_an_empty_directory_without_failing() {
    let fixture = Fixture::new("empty");

    let output = run(&[&fixture.path_str()]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    assert!(stdout.contains("No project detected"));
}

#[test]
fn prints_a_human_readable_summary() {
    let fixture = Fixture::new("summary");
    fixture
        .file("manage.py", "import django\n")
        .file("requirements.txt", "Django==5.0\n");

    let output = run(&[&fixture.path_str()]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    assert!(stdout.contains("Project:"));
    assert!(stdout.contains("django"));
    assert!(stdout.contains("Evidence"));
}

#[test]
fn rejects_a_missing_directory_with_a_usage_error() {
    let output = run(&["this/path/does/not/exist"]);

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("stderr must be utf-8");
    assert!(stderr.contains("is not a directory"));
}

#[test]
fn prints_help_for_the_help_flag() {
    let output = run(&["--help"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    assert!(stdout.contains("USAGE"));
}

#[test]
fn prints_the_version_for_the_version_flag() {
    let output = run(&["--version"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout must be utf-8");
    assert!(stdout.starts_with("pilot "));
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn rejects_unknown_options() {
    let output = run(&["--nope"]);

    assert_eq!(output.status.code(), Some(2));
}
