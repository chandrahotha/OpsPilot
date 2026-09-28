//! Shell and process-tree handling for the current platform.
//!
//! Every platform difference lives here: the rest of the engine only asks for a
//! shell command or a stop command and does not care which OS is active.

use std::process::Command;

/// Hide the child process window on Windows.
///
/// Without `CREATE_NO_WINDOW`, every spawned helper (`cmd`, `taskkill`, dev
/// servers) pops up its own console window, which looks like the app is
/// opening terminals by itself.
#[cfg(windows)]
fn hide_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0800_0000);
}

/// Build a command that runs `command` through the platform shell.
///
/// Using the shell keeps behaviour identical to the user's own terminal, which is
/// what makes Windows shims such as `npm.cmd` work.
pub fn shell_command(command: &str) -> Command {
    #[cfg(windows)]
    {
        let mut shell = Command::new("cmd");
        shell.arg("/C").arg(command);
        hide_window(&mut shell);
        shell
    }

    #[cfg(not(windows))]
    {
        let mut shell = Command::new("sh");
        shell.arg("-c").arg(command);
        shell
    }
}

/// Build a command that stops a whole process tree.
///
/// On Windows the tree is killed with `taskkill /T /F`; on Unix the process group
/// started for the shell receives the signal, so grandchildren (the actual dev
/// server) die together with their shell.
pub fn stop_tree_command(pid: u32) -> Command {
    #[cfg(windows)]
    {
        let mut stop = Command::new("taskkill");
        stop.arg("/PID").arg(pid.to_string()).arg("/T").arg("/F");
        hide_window(&mut stop);
        stop
    }

    #[cfg(not(windows))]
    {
        let mut stop = Command::new("kill");
        stop.arg("-TERM").arg(format!("-{pid}"));
        stop
    }
}

/// Build a command that force-terminates a whole process tree.
///
/// Unlike [`stop_tree_command`] (which asks Unix process groups to exit via
/// SIGTERM and waits), this sends SIGKILL on Unix and `taskkill /T /F` on
/// Windows, then the caller kills the direct child without waiting. Reserved
/// for stuck processes ("Kill All Nodes"): normal stops must use `stop`.
pub fn kill_tree_command(pid: u32) -> Command {
    #[cfg(windows)]
    {
        let mut kill = Command::new("taskkill");
        kill.arg("/PID").arg(pid.to_string()).arg("/T").arg("/F");
        hide_window(&mut kill);
        kill
    }

    #[cfg(not(windows))]
    {
        let mut kill = Command::new("kill");
        kill.arg("-KILL").arg(format!("-{pid}"));
        kill
    }
}

/// Platform identifier reported in snapshots and operation history
pub fn current_platform() -> &'static str {
    std::env::consts::OS
}

/// Whether stop is forceful on this platform
pub const STOP_IS_FORCEFUL: bool = cfg!(windows);

#[cfg(windows)]
mod win_api {
    #[repr(C)]
    pub struct IO_COUNTERS {
        pub read_operation_count: u64,
        pub write_operation_count: u64,
        pub other_operation_count: u64,
        pub read_transfer_count: u64,
        pub write_transfer_count: u64,
        pub other_transfer_count: u64,
    }

    #[repr(C)]
    pub struct JOBOBJECT_BASIC_LIMIT_INFORMATION {
        pub per_process_user_time_limit: i64,
        pub per_job_user_time_limit: i64,
        pub limit_flags: u32,
        pub minimum_working_set_size: usize,
        pub maximum_working_set_size: usize,
        pub active_process_limit: u32,
        pub affinity: usize,
        pub priority_class: u32,
        pub scheduling_class: u32,
    }

    #[repr(C)]
    pub struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
        pub basic_limit_information: JOBOBJECT_BASIC_LIMIT_INFORMATION,
        pub io_info: IO_COUNTERS,
        pub process_memory_limit: usize,
        pub job_memory_limit: usize,
        pub peak_process_memory_limit: usize,
        pub peak_job_memory_limit: usize,
    }

    #[repr(C)]
    pub struct PROCESSENTRY32W {
        pub dw_size: u32,
        pub cnt_usage: u32,
        pub th32_process_id: u32,
        pub th32_default_heap_id: usize,
        pub th32_module_id: u32,
        pub cnt_threads: u32,
        pub th32_parent_process_id: u32,
        pub pc_pri_class_base: i32,
        pub dw_flags: u32,
        pub sz_exe_file: [u16; 260],
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn CreateJobObjectW(
            lp_job_attributes: *mut std::ffi::c_void,
            lp_name: *const u16,
        ) -> *mut std::ffi::c_void;
        pub fn SetInformationJobObject(
            h_job: *mut std::ffi::c_void,
            job_object_info_class: u32,
            lp_job_object_info: *mut std::ffi::c_void,
            cb_job_object_info_length: u32,
        ) -> i32;
        pub fn AssignProcessToJobObject(
            h_job: *mut std::ffi::c_void,
            h_process: *mut std::ffi::c_void,
        ) -> i32;
        pub fn TerminateJobObject(h_job: *mut std::ffi::c_void, u_exit_code: u32) -> i32;
        pub fn CreateToolhelp32Snapshot(
            dw_flags: u32,
            th32_process_id: u32,
        ) -> *mut std::ffi::c_void;
        pub fn Process32FirstW(
            h_snapshot: *mut std::ffi::c_void,
            lppe: *mut PROCESSENTRY32W,
        ) -> i32;
        pub fn Process32NextW(h_snapshot: *mut std::ffi::c_void, lppe: *mut PROCESSENTRY32W)
        -> i32;
        pub fn CloseHandle(h_object: *mut std::ffi::c_void) -> i32;
    }
}

/// Windows Job Object wrapper ensuring clean termination of spawned process trees
#[cfg(windows)]
pub struct JobObjectGuard {
    handle: *mut std::ffi::c_void,
}

#[cfg(windows)]
unsafe impl Send for JobObjectGuard {}
#[cfg(windows)]
unsafe impl Sync for JobObjectGuard {}

#[cfg(windows)]
impl JobObjectGuard {
    pub fn new() -> Option<Self> {
        use win_api::*;
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
            if handle.is_null() {
                return None;
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.basic_limit_information.limit_flags = 0x2000; // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            let ok = SetInformationJobObject(
                handle,
                9, // JobObjectExtendedLimitInformation
                &mut info as *mut _ as *mut std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if ok == 0 {
                CloseHandle(handle);
                return None;
            }
            Some(JobObjectGuard { handle })
        }
    }

    /// # Safety
    ///
    /// `process_handle` must be a valid, open process handle for the lifetime
    /// of this call.
    pub unsafe fn assign_process(&self, process_handle: std::os::windows::io::RawHandle) -> bool {
        use win_api::*;
        // SAFETY: caller guarantees process_handle is a valid open process handle.
        unsafe { AssignProcessToJobObject(self.handle, process_handle) != 0 }
    }

    pub fn terminate(&self) {
        use win_api::*;
        unsafe {
            TerminateJobObject(self.handle, 1);
        }
    }
}

#[cfg(windows)]
impl Drop for JobObjectGuard {
    fn drop(&mut self) {
        use win_api::*;
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

/// Get all descendant process IDs for a given root PID
#[cfg(windows)]
pub fn get_descendant_pids(root_pid: u32) -> Vec<u32> {
    use std::collections::{HashMap, HashSet, VecDeque};
    use win_api::*;

    const TH32CS_SNAPPROCESS: u32 = 0x00000002;
    const INVALID_HANDLE_VALUE: *mut std::ffi::c_void = -1isize as *mut std::ffi::c_void;

    let mut parent_to_children: HashMap<u32, Vec<u32>> = HashMap::new();

    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE || snapshot.is_null() {
            return Vec::new();
        }

        let mut entry = PROCESSENTRY32W {
            dw_size: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            cnt_usage: 0,
            th32_process_id: 0,
            th32_default_heap_id: 0,
            th32_module_id: 0,
            cnt_threads: 0,
            th32_parent_process_id: 0,
            pc_pri_class_base: 0,
            dw_flags: 0,
            sz_exe_file: [0; 260],
        };

        if Process32FirstW(snapshot, &mut entry) != 0 {
            loop {
                parent_to_children
                    .entry(entry.th32_parent_process_id)
                    .or_default()
                    .push(entry.th32_process_id);

                if Process32NextW(snapshot, &mut entry) == 0 {
                    break;
                }
            }
        }

        CloseHandle(snapshot);
    }

    let mut descendants = Vec::new();
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();

    queue.push_back(root_pid);
    visited.insert(root_pid);

    while let Some(current) = queue.pop_front() {
        if let Some(children) = parent_to_children.get(&current) {
            for &child in children {
                if visited.insert(child) {
                    descendants.push(child);
                    queue.push_back(child);
                }
            }
        }
    }

    descendants
}

#[cfg(not(windows))]
pub fn get_descendant_pids(root_pid: u32) -> Vec<u32> {
    #[cfg(target_os = "linux")]
    {
        use std::collections::{HashMap, HashSet, VecDeque};
        let mut parent_to_children: HashMap<u32, Vec<u32>> = HashMap::new();
        if let Ok(entries) = std::fs::read_dir("/proc") {
            for entry in entries.flatten() {
                if let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() {
                    let stat_path = format!("/proc/{pid}/stat");
                    if let Ok(content) = std::fs::read_to_string(stat_path)
                        && let Some(idx) = content.rfind(')')
                    {
                        let rest = &content[idx + 1..];
                        let parts: Vec<&str> = rest.split_whitespace().collect();
                        if parts.len() >= 2
                            && let Ok(ppid) = parts[1].parse::<u32>()
                        {
                            parent_to_children.entry(ppid).or_default().push(pid);
                        }
                    }
                }
            }
        }

        let mut descendants = Vec::new();
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        queue.push_back(root_pid);
        visited.insert(root_pid);
        while let Some(current) = queue.pop_front() {
            if let Some(children) = parent_to_children.get(&current) {
                for &child in children {
                    if visited.insert(child) {
                        descendants.push(child);
                        queue.push_back(child);
                    }
                }
            }
        }
        descendants
    }

    #[cfg(target_os = "macos")]
    {
        use std::collections::{HashMap, HashSet, VecDeque};

        // macOS has no /proc; `ps -Ao pid=,ppid=` (BSD ps, no GNU-only flags)
        // lists every process on the system with its parent, which is enough
        // to walk the same parent-to-children tree the Linux branch builds.
        let mut parent_to_children: HashMap<u32, Vec<u32>> = HashMap::new();
        if let Ok(output) = Command::new("ps").args(["-Ao", "pid=,ppid="]).output() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let mut parts = line.split_whitespace();
                let (Some(pid_str), Some(ppid_str)) = (parts.next(), parts.next()) else {
                    continue;
                };
                if let (Ok(pid), Ok(ppid)) = (pid_str.parse::<u32>(), ppid_str.parse::<u32>()) {
                    parent_to_children.entry(ppid).or_default().push(pid);
                }
            }
        }

        let mut descendants = Vec::new();
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        queue.push_back(root_pid);
        visited.insert(root_pid);
        while let Some(current) = queue.pop_front() {
            if let Some(children) = parent_to_children.get(&current) {
                for &child in children {
                    if visited.insert(child) {
                        descendants.push(child);
                        queue.push_back(child);
                    }
                }
            }
        }
        descendants
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = root_pid;
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_commands_use_the_platform_shell() {
        let shell = shell_command("echo hi");
        let program = format!("{:?}", shell);

        if cfg!(windows) {
            assert!(program.contains("cmd"), "got {program}");
        } else {
            assert!(program.contains("sh"), "got {program}");
        }
    }

    #[test]
    fn stop_commands_target_the_process_tree() {
        let stop = stop_tree_command(4242);
        let rendered = format!("{:?}", stop);

        if cfg!(windows) {
            assert!(rendered.contains("taskkill"), "got {rendered}");
            assert!(rendered.contains("4242"), "got {rendered}");
        } else {
            assert!(rendered.contains("-4242"), "got {rendered}");
        }
    }

    #[test]
    fn the_platform_is_reported() {
        assert!(!current_platform().is_empty());
    }
}
