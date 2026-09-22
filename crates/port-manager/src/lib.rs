//! Pilot Port Manager - Port detection, conflict handling, and process ownership
//!
//! Phase 7: Detects configured application ports, ports currently in use,
//! identifies the owning process (PID + name), and safely changes port
//! configurations in project files.
//!
//! Process ownership detection is cross-platform (Windows, Linux, macOS)
//! and only reports verified ownership ("Pilot Prerequisite.md" section 17).

#[cfg(target_os = "linux")]
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Timeout used when probing a port. Kept short so inspection never blocks the UI
pub const PROBE_TIMEOUT: Duration = Duration::from_millis(250);

/// How far above a preferred port Pilot searches for a free port
pub const SEARCH_RANGE: u16 = 20;

/// Build a helper command that never pops up a console window.
///
/// On Windows every `Command` without `CREATE_NO_WINDOW` flashes a console.
/// These helpers run on every status poll, so a visible window would flicker
/// constantly while the GUI is open.
#[cfg(windows)]
#[allow(dead_code)]
fn silent_command(program: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut command = Command::new(program);
    command.creation_flags(0x0800_0000);
    command
}

#[cfg(windows)]
mod win_net {
    pub const AF_INET: u32 = 2;
    pub const AF_INET6: u32 = 23;
    pub const TCP_TABLE_OWNER_PID_ALL: u32 = 5;
    pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct MIB_TCPROW_OWNER_PID {
        pub dw_state: u32,
        pub dw_local_addr: u32,
        pub dw_local_port: u32,
        pub dw_remote_addr: u32,
        pub dw_remote_port: u32,
        pub dw_owning_pid: u32,
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct MIB_TCP6ROW_OWNER_PID {
        pub uc_local_addr: [u8; 16],
        pub dw_local_scope_id: u32,
        pub dw_local_port: u32,
        pub uc_remote_addr: [u8; 16],
        pub dw_remote_scope_id: u32,
        pub dw_remote_port: u32,
        pub dw_state: u32,
        pub dw_owning_pid: u32,
    }

    #[link(name = "iphlpapi")]
    unsafe extern "system" {
        pub fn GetExtendedTcpTable(
            p_tcp_table: *mut std::ffi::c_void,
            pdw_size: *mut u32,
            b_order: i32,
            ul_af: u32,
            table_class: u32,
            reserved: u32,
        ) -> u32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn OpenProcess(
            dw_desired_access: u32,
            b_inherit_handle: i32,
            dw_process_id: u32,
        ) -> *mut std::ffi::c_void;
        pub fn CloseHandle(h_object: *mut std::ffi::c_void) -> i32;
        pub fn QueryFullProcessImageNameW(
            h_process: *mut std::ffi::c_void,
            dw_flags: u32,
            lp_exe_name: *mut u16,
            lpdw_size: *mut u32,
        ) -> i32;
    }

    pub fn find_tcp_owner(port: u16) -> Option<u32> {
        if let Some(pid) = find_tcp4_owner(port) {
            return Some(pid);
        }
        find_tcp6_owner(port)
    }

    fn find_tcp4_owner(port: u16) -> Option<u32> {
        unsafe {
            let mut size: u32 = 0;
            let _ = GetExtendedTcpTable(
                std::ptr::null_mut(),
                &mut size,
                0,
                AF_INET,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            );
            if size == 0 {
                return None;
            }

            let mut buffer = vec![0u8; size as usize];
            let ret = GetExtendedTcpTable(
                buffer.as_mut_ptr() as *mut std::ffi::c_void,
                &mut size,
                0,
                AF_INET,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            );
            if ret != 0 {
                return None;
            }

            let num_entries = *(buffer.as_ptr() as *const u32);
            let rows = buffer.as_ptr().add(4) as *const MIB_TCPROW_OWNER_PID;
            for i in 0..num_entries as usize {
                let row = *rows.add(i);
                let row_port = u16::from_be((row.dw_local_port & 0xFFFF) as u16);
                if row_port == port && row.dw_owning_pid != 0 {
                    return Some(row.dw_owning_pid);
                }
            }
            None
        }
    }

    fn find_tcp6_owner(port: u16) -> Option<u32> {
        unsafe {
            let mut size: u32 = 0;
            let _ = GetExtendedTcpTable(
                std::ptr::null_mut(),
                &mut size,
                0,
                AF_INET6,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            );
            if size == 0 {
                return None;
            }

            let mut buffer = vec![0u8; size as usize];
            let ret = GetExtendedTcpTable(
                buffer.as_mut_ptr() as *mut std::ffi::c_void,
                &mut size,
                0,
                AF_INET6,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            );
            if ret != 0 {
                return None;
            }

            let num_entries = *(buffer.as_ptr() as *const u32);
            let rows = buffer.as_ptr().add(4) as *const MIB_TCP6ROW_OWNER_PID;
            for i in 0..num_entries as usize {
                let row = *rows.add(i);
                let row_port = u16::from_be((row.dw_local_port & 0xFFFF) as u16);
                if row_port == port && row.dw_owning_pid != 0 {
                    return Some(row.dw_owning_pid);
                }
            }
            None
        }
    }
}

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
    ChangePort {
        from: u16,
        to: u16,
        #[serde(rename = "projectDir")]
        project_dir: String,
    },
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
    Changed {
        from: u16,
        to: u16,
        #[serde(rename = "modifiedFiles")]
        modified_files: Vec<String>,
    },
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

    /// Check if something is accepting TCP connections on the loopback interface.
    ///
    /// Both IPv4 (`127.0.0.1`) and IPv6 (`::1`) loopback are probed: dev
    /// servers such as Vite may listen on IPv6 only, and an IPv4-only probe
    /// would wrongly report the port as free.
    pub fn is_listening(&self, port: u16) -> bool {
        let v4 = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
        if TcpStream::connect_timeout(&v4.into(), PROBE_TIMEOUT).is_ok() {
            return true;
        }
        let v6 = SocketAddrV6::new(Ipv6Addr::LOCALHOST, port, 0, 0);
        TcpStream::connect_timeout(&v6.into(), PROBE_TIMEOUT).is_ok()
    }

    /// Check if a port can be bound (is free for a new service).
    ///
    /// A port counts as free only when it is bindable on both IPv4 and IPv6
    /// loopback: a server holding one family must block reuse of the port.
    pub fn is_bindable(&self, port: u16) -> bool {
        TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok()
            && TcpListener::bind((Ipv6Addr::LOCALHOST, port)).is_ok()
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

    /// Windows: Native TCP table query via iphlpapi and process query via kernel32
    #[cfg(windows)]
    fn get_port_owner_windows(&self, port: u16) -> Option<ProcessOwner> {
        let pid = win_net::find_tcp_owner(port)?;
        self.get_process_info_windows(pid)
    }

    #[cfg(windows)]
    fn get_process_info_windows(&self, pid: u32) -> Option<ProcessOwner> {
        if pid == 0 {
            return None;
        }
        if pid == 4 {
            return Some(ProcessOwner {
                pid,
                name: "System".to_string(),
                command: "System".to_string(),
                start_time: None,
            });
        }

        unsafe {
            let handle = win_net::OpenProcess(win_net::PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return Some(ProcessOwner {
                    pid,
                    name: format!("process-{pid}"),
                    command: String::new(),
                    start_time: None,
                });
            }

            let mut buffer = [0u16; 1024];
            let mut size = buffer.len() as u32;
            let success = win_net::QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size);
            win_net::CloseHandle(handle);

            if success != 0 && size > 0 {
                let full_path = String::from_utf16_lossy(&buffer[..size as usize]);
                let name = std::path::Path::new(&full_path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| full_path.clone());
                Some(ProcessOwner {
                    pid,
                    name,
                    command: full_path,
                    start_time: None,
                })
            } else {
                Some(ProcessOwner {
                    pid,
                    name: format!("pid-{pid}"),
                    command: String::new(),
                    start_time: None,
                })
            }
        }
    }

    /// Linux: Use ss and /proc
    #[cfg(target_os = "linux")]
    fn get_port_owner_linux(&self, port: u16) -> Option<ProcessOwner> {
        use std::sync::OnceLock;

        static PID_REGEX: OnceLock<Regex> = OnceLock::new();
        static COMM_REGEX: OnceLock<Regex> = OnceLock::new();

        let pid_regex = PID_REGEX.get_or_init(|| Regex::new(r"pid=(\d+),?").expect("pid regex"));
        let comm_regex = COMM_REGEX.get_or_init(|| Regex::new(r#"comm=\"([^\"]+)\""#).expect("comm regex"));

        let output = Command::new("ss").args(["-tlnp"]).output().ok()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let port_pattern = format!(":{}", port);

        for line in stdout.lines() {
            if line.contains(&port_pattern) {
                let pid = pid_regex
                    .captures(line)
                    .and_then(|c| c.get(1))
                    .and_then(|m| m.as_str().parse::<u32>().ok())?;

                let name = comm_regex
                    .captures(line)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_else(|| {
                        fs::read_to_string(format!("/proc/{}/comm", pid))
                            .ok()
                            .map(|s| s.trim().to_string())
                            .unwrap_or_else(|| "unknown".to_string())
                    });

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

                let command = String::from_utf8_lossy(&cmd_output.stdout)
                    .trim()
                    .to_string();

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

    /// Replace port references in a file using proper parsing for structured formats
    fn replace_port_in_file(
        file_path: &Path,
        from_port: u16,
        to_port: u16,
    ) -> Result<bool, std::io::Error> {
        let content = fs::read_to_string(file_path)?;
        let from_str = from_port.to_string();
        let to_str = to_port.to_string();

        let extension = file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let new_content = match extension.as_str() {
            "json" => Self::replace_port_in_json(&content, &from_str, &to_str)?,
            "yaml" | "yml" => Self::replace_port_in_yaml(&content, &from_str, &to_str)?,
            "toml" => Self::replace_port_in_toml(&content, &from_str, &to_str)?,
            _ => Self::replace_port_in_text(&content, &from_str, &to_str),
        };

        if new_content != content {
            fs::write(file_path, new_content.as_bytes())?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Replace port in JSON content using serde_json
    fn replace_port_in_json(content: &str, from: &str, to: &str) -> Result<String, std::io::Error> {
        let mut value: serde_json::Value = serde_json::from_str(content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Self::replace_port_in_json_value(&mut value, from, to, None);
        Ok(serde_json::to_string_pretty(&value)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?)
    }

    fn replace_port_in_json_value(value: &mut serde_json::Value, from: &str, to: &str, key: Option<&str>) {
        match value {
            serde_json::Value::String(s) => {
                // Only replace if the key suggests it's a port, or if the value is a pure number
                if Self::is_port_context(key, s) && s == from {
                    *s = to.to_string();
                }
            }
            serde_json::Value::Number(n) => {
                // Replace numeric port values
                if n.as_u64() == Some(from.parse().unwrap_or(0)) {
                    *value = serde_json::Value::Number(to.parse().unwrap());
                }
            }
            serde_json::Value::Array(arr) => {
                for item in arr {
                    Self::replace_port_in_json_value(item, from, to, None);
                }
            }
            serde_json::Value::Object(obj) => {
                for (k, v) in obj.iter_mut() {
                    Self::replace_port_in_json_value(v, from, to, Some(k));
                }
            }
            _ => {}
        }
    }

    /// Replace port in YAML content using serde_yaml
    fn replace_port_in_yaml(content: &str, from: &str, to: &str) -> Result<String, std::io::Error> {
        let mut value: serde_yaml::Value = serde_yaml::from_str(content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Self::replace_port_in_yaml_value(&mut value, from, to, None);
        Ok(serde_yaml::to_string(&value)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?)
    }

    fn replace_port_in_yaml_value(value: &mut serde_yaml::Value, from: &str, to: &str, key: Option<&str>) {
        match value {
            serde_yaml::Value::String(s) => {
                if Self::is_port_context(key, s) && s == from {
                    *s = to.to_string();
                }
            }
            serde_yaml::Value::Number(n) => {
                if n.as_u64() == Some(from.parse().unwrap_or(0)) {
                    *value = serde_yaml::Value::Number(to.parse().unwrap());
                }
            }
            serde_yaml::Value::Sequence(seq) => {
                for item in seq {
                    Self::replace_port_in_yaml_value(item, from, to, None);
                }
            }
            serde_yaml::Value::Mapping(map) => {
                for (k, v) in map.iter_mut() {
                    let key_str = k.as_str().unwrap_or("");
                    Self::replace_port_in_yaml_value(v, from, to, Some(key_str));
                }
            }
            _ => {}
        }
    }

    /// Replace port in TOML content using toml
    fn replace_port_in_toml(content: &str, from: &str, to: &str) -> Result<String, std::io::Error> {
        let mut value: toml::Value = content.parse()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Self::replace_port_in_toml_value(&mut value, from, to, None);
        Ok(value.to_string())
    }

    fn replace_port_in_toml_value(value: &mut toml::Value, from: &str, to: &str, key: Option<&str>) {
        match value {
            toml::Value::String(s) => {
                if Self::is_port_context(key, s) && s == from {
                    *s = to.to_string();
                }
            }
            toml::Value::Integer(n) => {
                if *n as u64 == from.parse().unwrap_or(0) {
                    *value = toml::Value::Integer(to.parse::<i64>().unwrap());
                }
            }
            toml::Value::Array(arr) => {
                for item in arr {
                    Self::replace_port_in_toml_value(item, from, to, None);
                }
            }
            toml::Value::Table(table) => {
                for (k, v) in table.iter_mut() {
                    Self::replace_port_in_toml_value(v, from, to, Some(k));
                }
            }
            _ => {}
        }
    }

    /// Check if a key/value pair is likely a port configuration
    fn is_port_context(key: Option<&str>, value: &str) -> bool {
        if let Some(key) = key {
            let key_lower = key.to_lowercase();
            // Common port-related keys
            if key_lower.contains("port") {
                return true;
            }
        }
        // Also replace if the value is a pure number (likely a port number)
        value.parse::<u16>().is_ok()
    }

    /// Replace port in plain text files (fallback for .env, config files, etc.)
    fn replace_port_in_text(content: &str, from: &str, to: &str) -> String {
        let mut result = String::with_capacity(content.len());
        let mut i = 0;
        let bytes = content.as_bytes();
        let from_bytes = from.as_bytes();
        let from_len = from_bytes.len();

        while i < bytes.len() {
            if i + from_len <= bytes.len() && &bytes[i..i + from_len] == from_bytes {
                let before_ok = i == 0 || !bytes[i - 1].is_ascii_digit();
                let after_ok = i + from_len >= bytes.len() || !bytes[i + from_len].is_ascii_digit();

                if before_ok && after_ok {
                    result.push_str(to);
                    i += from_len;
                    continue;
                }
            }
            result.push(bytes[i] as char);
            i += 1;
        }

        result
    }

    /// Change a port across all relevant files in a project
    pub fn change_port(
        project_dir: &str,
        from_port: u16,
        to_port: u16,
    ) -> Result<Vec<String>, std::io::Error> {
        if from_port == to_port {
            return Ok(Vec::new());
        }

        let files = Self::find_port_files(project_dir);
        let mut modified = Vec::new();

        for file in files {
            if Self::replace_port_in_file(&file, from_port, to_port)? {
                modified.push(
                    file.strip_prefix(project_dir)
                        .unwrap_or(&file)
                        .to_string_lossy()
                        .to_string(),
                );
            }
        }

        Ok(modified)
    }
}

/// Port manager with both inspection and changing capabilities
pub struct PortManagerWithChange {
    inspector: PortManager,
}

impl PortManagerWithChange {
    pub fn new() -> Self {
        Self {
            inspector: PortManager::new(),
        }
    }

    pub fn inspect_port(&self, port: u16) -> PortStatus {
        self.inspector.inspect_port(port)
    }

    pub fn find_free_port(&self, preferred: u16) -> Option<u16> {
        self.inspector.find_free_port(preferred)
    }

    pub fn change_port(
        &self,
        project_dir: &str,
        from_port: u16,
        to_port: u16,
    ) -> Result<Vec<String>, std::io::Error> {
        PortChanger::change_port(project_dir, from_port, to_port)
    }

    pub fn execute(&self, operation: PortOperation) -> PortOutcome {
        match operation {
            PortOperation::InspectPort(port) => {
                PortOutcome::Status(self.inspector.inspect_port(port))
            }
            PortOperation::FindFreePort(port) => match self.inspector.find_free_port(port) {
                Some(free) => PortOutcome::FreePort(free),
                None => PortOutcome::NotImplemented("no free port found in search range"),
            },
            PortOperation::ChangePort {
                from,
                to,
                project_dir,
            } => match PortChanger::change_port(&project_dir, from, to) {
                Ok(modified) => PortOutcome::Changed {
                    from,
                    to,
                    modified_files: modified,
                },
                Err(e) => PortOutcome::Error(e.to_string()),
            },
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

/// Whether something is accepting TCP connections on the loopback interface.
///
/// Probes both IPv4 (`127.0.0.1`) and IPv6 (`::1`): dev servers such as Vite
/// may listen on IPv6 only, and an IPv4-only probe would miss them.
pub fn is_listening(port: u16) -> bool {
    PortManager::new().is_listening(port)
}

/// Whether a port can be bound, i.e. it is free for a new service.
///
/// A port counts as free only when bindable on both IPv4 and IPv6 loopback.
pub fn is_bindable(port: u16) -> bool {
    PortManager::new().is_bindable(port)
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
        assert_eq!(status.pid, Some(std::process::id()));
        assert!(status.process.is_some());
    }

    #[test]
    fn an_ipv6_only_listener_is_detected() {
        let _lock = port_lock();

        let listener = match TcpListener::bind((Ipv6Addr::LOCALHOST, 0)) {
            Ok(listener) => listener,
            Err(_) => return, // No IPv6 loopback on this machine; nothing to verify.
        };
        let port = listener.local_addr().expect("must have an address").port();

        // An IPv4-only probe would miss this; the dual-stack probe must not.
        assert!(is_listening(port));
        assert!(!is_bindable(port));
        assert!(!inspect_port(port).available);
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
        let temp_dir = std::env::temp_dir().join(format!("port-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).expect("temp dir must be created");
        // Create a package.json with port 3000
        fs::write(temp_dir.join("package.json"), r#"{"scripts": {"dev": "vite --port 3000"}}"#).expect("file must be written");

        let result = execute(PortOperation::ChangePort {
            from: 3000,
            to: 3100,
            project_dir: temp_dir.to_string_lossy().to_string(),
        });
        match result {
            PortOutcome::Changed {
                from,
                to,
                modified_files,
            } => {
                assert_eq!(from, 3000);
                assert_eq!(to, 3100);
                assert_eq!(modified_files, vec!["package.json".to_string()]);
            }
            other => panic!("expected Changed, got {:?}", other),
        }

        let _ = fs::remove_dir_all(&temp_dir);
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
        let op = PortOperation::ChangePort {
            from: 3000,
            to: 3100,
            project_dir: ".".to_string(),
        };
        let json = serde_json::to_value(&op).unwrap();
        // Handle both externally tagged ({"ChangePort": {...}}) and internally tagged formats
        let from = json
            .get("from")
            .or_else(|| json.get("changePort").and_then(|v| v.get("from")))
            .expect("from field");
        assert_eq!(from, 3000);

        let to = json
            .get("to")
            .or_else(|| json.get("changePort").and_then(|v| v.get("to")))
            .expect("to field");
        assert_eq!(to, 3100);

        let project_dir = json
            .get("projectDir")
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
        let from = json
            .get("from")
            .or_else(|| json.get("changed").and_then(|v| v.get("from")))
            .expect("from field");
        assert_eq!(from, 3000);

        let to = json
            .get("to")
            .or_else(|| json.get("changed").and_then(|v| v.get("to")))
            .expect("to field");
        assert_eq!(to, 3100);

        let modified_files = json
            .get("modifiedFiles")
            .or_else(|| json.get("changed").and_then(|v| v.get("modifiedFiles")))
            .expect("modifiedFiles field");
        assert_eq!(modified_files[0], "package.json");
    }
}
