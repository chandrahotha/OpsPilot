//! Shared test fixtures.
//!
//! Every fixture is created inside a unique temporary directory, so the tests
//! never depend on the developer's machine configuration
//! (see "Pilot Prerequisite.md" section 25).
#![allow(dead_code)]

use pilot_core::ScanResult;
use pilot_scanner::scan_project;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A throwaway project directory that removes itself when dropped
pub struct Fixture {
    path: PathBuf,
}

impl Fixture {
    /// Create an empty project fixture
    pub fn new(name: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("pilot-scan-{name}-{}-{unique}", std::process::id()));

        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("fixture directory must be created");

        Fixture { path }
    }

    /// Write a file relative to the fixture root
    pub fn file(&self, relative: &str, content: &str) -> &Self {
        let target = self.path.join(relative);

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("fixture parent directory must be created");
        }

        fs::write(target, content).expect("fixture file must be written");

        self
    }

    /// Scan the fixture directory
    pub fn scan(&self) -> ScanResult {
        scan_project(self.path.to_string_lossy().to_string())
    }

    /// Fixture root directory
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Fixture directory name, which is the expected project name
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .expect("fixture must have a name")
            .to_string_lossy()
            .to_string()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
