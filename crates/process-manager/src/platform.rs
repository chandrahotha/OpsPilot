//! Shell and process-tree handling for the current platform.
//!
//! Every platform difference lives here: the rest of the engine only asks for a
//! shell command or a stop command and does not care which OS is active
//! ("Pilot Prerequisite.md" section 20).

use std::process::Command;

/// Build a command that runs `command` through the platform shell.
///
/// Using the shell keeps behaviour identical to the user's own terminal, which is
/// what makes Windows shims such as `npm.cmd` work.
pub fn shell_command(command: &str) -> Command {
    #[cfg(windows)]
    {
        let mut shell = Command::new("cmd");
        shell.arg("/C").arg(command);
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
        stop
    }

    #[cfg(not(windows))]
    {
        let mut stop = Command::new("kill");
        stop.arg("-TERM").arg(format!("-{pid}"));
        stop
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