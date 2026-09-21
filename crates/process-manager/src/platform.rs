//! Shell and process-tree handling for the current platform.
//!
//! Every platform difference lives here: the rest of the engine only asks for a
//! shell command or a stop command and does not care which OS is active
//! ("Pilot Prerequisite.md" section 20).

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
        stop.arg("/PID")
            .arg(pid.to_string())
            .arg("/T")
            .arg("/F");
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
        kill.arg("/PID")
            .arg(pid.to_string())
            .arg("/T")
            .arg("/F");
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