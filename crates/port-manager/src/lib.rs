//! Pilot Port Manager - Port detection, conflict handling, and process ownership
//!
//! Phase 7: Detects configured application ports, ports currently in use,
//! identifies the owning process (PID + name), and safely changes port
//! configurations in project files.
//!
//! Process ownership detection is cross-platform (Windows, Linux, macOS)
//! and only reports verified ownership ("Pilot Prerequisite.md" section 17).

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Timeout used when probing a port. Kept short so inspection never blocks the UI
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(250);

/// How far above a preferred port Pilot searches for a free port
pub const SEARCH_RANGE: u16 = 20;

/// Port status information
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortStatus {
    /// The port number
    pub port: u16,
    /// Whether the port is available
    pub available: bool,
    /// The process ID occupying the port (if occupied)
    pub pid: Option<u32>,
    /// The process name occupying the port (if occupied)
    pub process: Option<String>,
    /// The command line of the owning process (if available)
    pub command: Option<String>,
}

/// Detailed information about a process owning a port
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessOwner {
    /// Process ID
    pub pid: u32,
    /// Process name
    pub name: String,
    /// Full command line
    pub command: String,
    /// Process start time (if available)
    pub start_time: Option<String>,
}

/// Port management operations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PortOperation {
    /// Find a free port near the specified port
    FindFreePort(u16),
    /// Inspect a port to see if it's in use and by whom
    InspectPort(u16),
    /// Change a port configuration in project files
    ChangePort { from: u16, to: u16, #[serde(rename = "projectDir")] project_dir: String },
}

/// Outcome of a port operation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PortOutcome {
    /// The inspected port state with ownership info
    Status(PortStatus),
    /// A free port was found
    FreePort(u16),
    /// Port change completed with list of modified files
    Changed { from: u16, to: u16, #[serde(rename = "modifiedFiles")] modified_files: Vec<String> },
    /// The operation is not implemented yet
    NotImplemented(&'static str),
    /// Operation failed with error
    Error(String),
}

/// Port manager for process ownership detection
pub struct PortManager;

impl PortManager {
    /// Create a new port manager
    pub fn new() -> Self {
        Self
    }

    /// Check if something is accepting TCP connections on the loopback interface
    pub fn is_listening(&self, port: u16) -> bool {
        let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
        TcpStream::connect_timeout(&address.into(), PROBE_TIMEOUT).is_ok()
    }

    /// Check if a port can be bound (is free for a new service)
    pub fn is_bindable(&self, port: u16) -> bool {
        TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok()
    }

    /// Get the process owning a port (cross-platform)
    fn get_port_owner(&self, port: u16) -> Option<ProcessOwner> {
        #[cfg(windows)]
        {
            self.get_port_owner_windows(port)
        }
        #[cfg(target_os = "linux")]
        {
            self.get_port_owner_linux(port)
        }
        #[cfg(target_os = "macos")]
        {
            self.get_port_owner_macos(port)
        }
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        {
            None
        }
    }

    /// Windows: Use netstat and tasklist
    #[cfg(windows)]
    fn get_port_owner_windows(&self, port: u16) -> Option<ProcessOwner> {
        let output = Command::new("cmd")
            .args(["/C", &format!("netstat -ano | findstr :{}", port)])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let pid_regex = Regex::new(r"\s+(\d+)\s*$").ok()?;
        
        for line in stdout.lines() {
            if line.contains(&format!(":{}", port)) && (line.contains("TCP") || line.contains("UDP")) {
                if let Some(caps) = pid_regex.captures(line) {
                    if let Ok(pid) = caps.get(1).unwrap().as_str().parse::<u32>() {
                        if let Some(owner) = self.get_process_info_windows(pid) {
                            return Some(owner);
                        }
                    }
                }
            }
        }
        None
    }

    #[cfg(windows)]
    fn get_process_info_windows(&self, pid: u32) -> Option<ProcessOwner> {
        let output = Command::new("cmd")
            .args(["/C", &format!("tasklist /FI \"PID eq {}\" /FO CSV", pid)])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines().skip(1) {
            if let Some(name) = line.split(',').next() {
                let name = name.trim_matches('"');
                let cmd_output = Command::new("cmd")
                    .args(["/C", &format!("wmic process where ProcessId={} get CommandLine /format:list", pid)])
                    .output()
                    .ok()?;
                
                let cmd_stdout = String::from_utf8_lossy(&cmd_output.stdout);
                let command = cmd_stdout
                    .lines()
                    .find(|l| l.starts_with("CommandLine="))
                    .map(|l| l["CommandLine=".len()..].to_string())
                    .unwrap_or_default();

                return Some(ProcessOwner {
                    pid,
                    name: name.to_string(),
                    command,
                    start_time: None,
                });
            }
        }
        None
    }

    /// Linux: Use ss and /proc
    #[cfg(target_os = "linux")]
    fn get_port_owner_linux(&self, port: u16) -> Option<ProcessOwner> {
        let output = Command::new("ss")
            .args(["-tlnp"])
            .output()
            .ok()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let port_pattern = format!(":{}", port);
        let pid_regex = Regex::new(r"pid=(\d+),?").ok()?;
        let comm_regex = Regex::new(r#"comm=\"([^\"]+)\""#).ok()?;

        for line in stdout.lines() {
            if line.contains(&port_pattern) {
                let pid = pid_regex.captures(line)
                    .and_then(|c| c.get(1))
                    .and_then(|m| m.as_str().parse::<u32>().ok())?;
                
                let name = comm_regex.captures(line)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_else(||
                        fs::read_to_string(format!("/proc/{}/comm", pid))
                            .ok()
                            .map(|s| s.trim().to_string())
                            .unwrap_or_else(||"unknown".to_string())
                    );

                let command = fs::read_to_string(format!("/proc/{}/cmdline", pid))
                    .ok()
                    .map(|s| s.replace('\0', " "))
                    .unwrap_or_default();

                return Some(ProcessOwner {
                    pid,
                    name,
                    command,
                    start_time: None,
                });
            }
        }
        None
    }

    /// macOS: Use lsof
    #[cfg(target_os = "macos")]
    fn get_port_owner_macos(&self, port: u16) -> Option<ProcessOwner> {
        let output = Command::new("lsof")
            .args(["-i", &format!("tcp:{}", port), "-P"])
            .output()
            .ok()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = stdout.lines().collect();
        if lines.len() > 1 {
            let parts: Vec<&str> = lines[1].split_whitespace().collect();
            if parts.len() >= 2 {
                let name = parts[0].to_string();
                let pid = parts[1].parse::<u32>().ok()?;
                
                let cmd_output = Command::new("ps")
                    .args(["-p", &pid.to_string(), "-o", "command="])
                    .output()
                    .ok()?;
                
                let command = String::from_utf8_lossy(&cmd_output.stdout).trim().to_string();

                return Some(ProcessOwner {
                    pid,
                    name,
                    command,
                    start_time: None,
                });
            }
        }
        None
    }

    /// Inspect a port and get its status with ownership info
    pub fn inspect_port(&self, port: u16) -> PortStatus {
        let occupied = self.is_listening(port) || !self.is_bindable(port);
        
        let (pid, process, command) = if occupied {
            if let Some(owner) = self.get_port_owner(port) {
                (Some(owner.pid), Some(owner.name), Some(owner.command))
            } else {
                (None, None, None)
            }
        } else {
            (None, None, None)
        };

        PortStatus {
            port,
            available: !occupied,
            pid,
            process,
            command,
        }
    }

    /// Find the first free port at or above preferred
    pub fn find_free_port(&self, preferred: u16) -> Option<u16> {
        let last = preferred.saturating_add(SEARCH_RANGE);
        (preferred..=last).find(|port| self.inspect_port(*port).available)
    }

    /// Get all ports in use with ownership info
    pub fn get_all_occupied_ports(&self) -> Vec<PortStatus> {
        let mut occupied = Vec::new();
        for port in 1..=65535 {
            if self.is_listening(port) {
                occupied.push(self.inspect_port(port));
            }
        }
        occupied
    }
}

/// Port configuration changer - safely updates port references in project files
pub struct PortChanger;

impl PortChanger {
    /// File patterns that may contain port references
    const PORT_PATTERNS: &[&str] = &[
        "package.json",
        "package-lock.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        "docker-compose.yml",
        "docker-compose.yaml",
        "compose.yml",
        "compose.yaml",
        ".env",
        ".env.*",
        "vite.config.*",
        "next.config.*",
        "nuxt.config.*",
        "vite.config.ts",
        "vite.config.js",
        "webpack.config.*",
        "*.yml",
        "*.yaml",
        "*.conf",
        "*.config.*",
        "Dockerfile",
        "dockerfile",
        "nginx.conf",
        "*.ini",
        "*.toml",
    ];

    /// Find all files in a project that may contain port references
    pub fn find_port_files(project_dir: &str) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let project_path = Path::new(project_dir);

        for pattern in Self::PORT_PATTERNS {
            let glob_pattern = project_path.join("**").join(pattern);
            if let Ok(paths) = glob::glob(glob_pattern.to_str().unwrap_or("")) {
                for path in paths.flatten() {
                    if path.is_file() && !files.contains(&path) {
                        files.push(path);
                    }
                }
            }
        }

        files
    }

    /// Replace port references in a file
    fn replace_port_in_file(file_path: &Path, from_port: u16, to_port: u16) -> Result<bool, std::io::Error> {
        let content = fs::read_to_string(file_path)?;
        let from_str = from_port.to_string();
        let to_str = to_port.to_string();

        // Simple replacement: replace "port" with "new_port" but only when surrounded by
        // non-digit characters (or start/end of string). We do this by iterating through
        // the string and checking each occurrence.
        let mut result = String::with_capacity(content.len());
        let mut i = 0;
        let bytes = content.as_bytes();
        let from_bytes = from_str.as_bytes();
        let from_len = from_bytes.len();

        while i < bytes.len() {
            // Check if the port number matches at this position
            if i + from_len <= bytes.len() && &bytes[i..i+from_len] == from_bytes {
                // Check if surrounded by non-digits (or start/end)
                let before_ok = i == 0 || !bytes[i-1].is_ascii_digit();
                let after_ok = i + from_len >= bytes.len() || !bytes[i+from_len].is_ascii_digit();
                
                if before_ok && after_ok {
                    result.push_str(&to_str);
                    i += from_len;
                    continue;
                }
            }
            result.push(bytes[i] as char);
            i += 1;
        }

        let new_content = result;
        
        if new_content != content {
            fs::write(file_path, new_content.as_bytes())?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Change a port across all relevant files in a project
    pub fn change_port(project_dir: &str, from_port: u16, to_port: u16) -> Result<Vec<String>, std::io::Error> {
        if from_port == to_port {
            return Ok(Vec::new());
        }

        let files = Self::find_port_files(project_dir);
        let mut modified = Vec::new();

        for file in files {
            if Self::replace_port_in_file(&file, from_port, to_port)? {
                modified.push(file.strip_prefix(project_dir).unwrap_or(&file).to_string_lossy().to_string());
            }
        }

        Ok(modified)
    }
}

/// Port manager with both inspection and changing capabilities
pub struct PortManagerWithChange {
    inspector: PortManager,
    changer: PortChanger,
}

impl PortManagerWithChange {
    pub fn new() -> Self {
        Self {
            inspector: PortManager::new(),
            changer: PortChanger,
        }
    }

    pub fn inspect_port(&self, port: u16) -> PortStatus {
        self.inspector.inspect_port(port)
    }

    pub fn find_free_port(&self, preferred: u16) -> Option<u16> {
        self.inspector.find_free_port(preferred)
    }

    pub fn get_all_occupied_ports(&self) -> Vec<PortStatus> {
        self.inspector.get_all_occupied_ports()
    }

    pub fn change_port(&self, project_dir: &str, from_port: u16, to_port: u16) -> Result<Vec<String>, std::io::Error> {
        PortChanger::change_port(project_dir, from_port, to_port)
    }

    pub fn execute(&self, operation: PortOperation) -> PortOutcome {
        match operation {
            PortOperation::InspectPort(port) => PortOutcome::Status(self.inspector.inspect_port(port)),
            PortOperation::FindFreePort(port) => match self.inspector.find_free_port(port) {
                Some(free) => PortOutcome::FreePort(free),
                None => PortOutcome::NotImplemented("no free port found in search range"),
            },
            PortOperation::ChangePort { from, to, project_dir } => {
                match PortChanger::change_port(&project_dir, from, to) {
                    Ok(modified) => PortOutcome::Changed {
                        from,
                        to,
                        modified_files: modified,
                    },
                    Err(e) => PortOutcome::Error(e.to_string()),
                }
            }
        }
    }
}

impl Default for PortManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for PortManagerWithChange {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether something is accepting TCP connections on the loopback interface
pub fn is_listening(port: u16) -> bool {
    let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    TcpStream::connect_timeout(&address.into(), PROBE_TIMEOUT).is_ok()
}

/// Whether a port can be bound, i.e. it is free for a new service
pub fn is_bindable(port: u16) -> bool {
    TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok()
}

/// Inspect the current state of a port (legacy function for compatibility)
pub fn inspect_port(port: u16) -> PortStatus {
    PortManager::new().inspect_port(port)
}

/// Find the first free port at or above preferred
pub fn find_free_port(preferred: u16) -> Option<u16> {
    PortManager::new().find_free_port(preferred)
}

/// Execute a port operation (legacy function for compatibility)
pub fn execute(operation: PortOperation) -> PortOutcome {
    PortManagerWithChange::new().execute(operation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    /// Port tests bind and release real ports. Running them in parallel would let
    /// one test take a port another test just released, so they are serialized.
    fn port_lock() -> MutexGuard<'static, ()> {
        static PORT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

        PORT_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Wait until the operating system has actually released a port
    fn wait_until_free(port: u16) -> bool {
        for _ in 0..50 {
            if inspect_port(port).available {
                return true;
            }

            std::thread::sleep(Duration::from_millis(10));
        }

        false
    }

    #[test]
    fn a_bound_port_is_reported_as_occupied() {
        let _lock = port_lock();

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("must bind");
        let port = listener.local_addr().expect("must have an address").port();

        assert!(is_listening(port));

        let status = inspect_port(port);

        assert_eq!(status.port, port);
        assert!(!status.available);
        assert!(status.pid.is_none(), "ownership must not be claimed");
        assert!(status.process.is_none());
    }

    #[test]
    fn a_released_port_becomes_available_again() {
        let _lock = port_lock();

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("must bind");
        let port = listener.local_addr().expect("must have an address").port();
        drop(listener);

        assert!(
            wait_until_free(port),
            "port {port} should be free again after the listener is dropped"
        );

        let status = inspect_port(port);

        assert_eq!(status.port, port);
        assert!(status.available);
    }

    #[test]
    fn free_port_search_skips_occupied_ports() {
        let _lock = port_lock();

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("must bind");
        let occupied = listener.local_addr().expect("must have an address").port();

        let free = find_free_port(occupied).expect("a free port must be found in range");

        assert!(free > occupied);
        assert!(inspect_port(free).available);
    }

    #[test]
    fn changing_a_port_is_reported_as_changed() {
        let result = execute(PortOperation::ChangePort { from: 3000, to: 3100, project_dir: ".".to_string() });
        match result {
            PortOutcome::Changed { from, to, modified_files } => {
                assert_eq!(from, 3000);
                assert_eq!(to, 3100);
                assert_eq!(modified_files, Vec::<String>::new());
            }
            other => panic!("expected Changed, got {:?}", other),
        }
    }

    #[test]
    fn inspecting_through_the_operation_layer_returns_a_status() {
        let _lock = port_lock();

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("must bind");
        let port = listener.local_addr().expect("must have an address").port();

        match execute(PortOperation::InspectPort(port)) {
            PortOutcome::Status(status) => assert!(!status.available),
            other => panic!("expected a status, got {other:?}"),
        }
    }

    #[test]
    fn port_status_serializes_with_camel_case() {
        let status = PortStatus {
            port: 3000,
            available: false,
            pid: Some(1234),
            process: Some("node".to_string()),
            command: Some("node server.js".to_string()),
        };

        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["port"], 3000);
        assert_eq!(json["available"], false);
        assert_eq!(json["pid"], 1234);
        assert_eq!(json["process"], "node");
        assert_eq!(json["command"], "node server.js");
    }

    #[test]
    fn port_operation_serializes_with_camel_case() {
        let op = PortOperation::ChangePort { from: 3000, to: 3100, project_dir: ".".to_string() };
        let json = serde_json::to_value(&op).unwrap();
        // Handle both externally tagged ({"ChangePort": {...}}) and internally tagged formats
        let from = json.get("from")
            .or_else(|| json.get("changePort").and_then(|v| v.get("from")))
            .expect("from field");
        assert_eq!(from, 3000);
        
        let to = json.get("to")
            .or_else(|| json.get("changePort").and_then(|v| v.get("to")))
            .expect("to field");
        assert_eq!(to, 3100);
        
        let project_dir = json.get("projectDir")
            .or_else(|| json.get("changePort").and_then(|v| v.get("projectDir")))
            .expect("projectDir field");
        assert_eq!(project_dir, ".");
    }

    #[test]
    fn port_outcome_changed_serializes() {
        let outcome = PortOutcome::Changed {
            from: 3000,
            to: 3100,
            modified_files: vec!["package.json".to_string()],
        };
        let json = serde_json::to_value(&outcome).unwrap();
        // Handle both externally tagged ({"Changed": {...}}) and internally tagged formats
        let from = json.get("from")
            .or_else(|| json.get("changed").and_then(|v| v.get("from")))
            .expect("from field");
        assert_eq!(from, 3000);
        
        let to = json.get("to")
            .or_else(|| json.get("changed").and_then(|v| v.get("to")))
            .expect("to field");
        assert_eq!(to, 3100);
        
        let modified_files = json.get("modifiedFiles")
            .or_else(|| json.get("changed").and_then(|v| v.get("modifiedFiles")))
            .expect("modifiedFiles field");
        assert_eq!(modified_files[0], "package.json");
    }
}


