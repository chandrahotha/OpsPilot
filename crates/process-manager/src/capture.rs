//! Streaming process output into a service log buffer.

use crate::log_buffer::{LogBuffer, LogEntry, LogStream};
use std::io::{BufRead, BufReader, Read};
use std::process::{ChildStderr, ChildStdout};
use std::sync::Arc;
use std::thread;

/// The pipes of a freshly spawned process
pub type ProcessPipes = (Option<ChildStdout>, Option<ChildStderr>);

/// Start one reader thread per pipe that forwards lines into `logs`
pub fn spawn_output_readers(label: &str, logs: Arc<LogBuffer>, pipes: ProcessPipes) {
    let (stdout, stderr) = pipes;

    if let Some(pipe) = stdout {
        forward_lines(label, LogStream::Stdout, pipe, Arc::clone(&logs));
    }

    if let Some(pipe) = stderr {
        forward_lines(label, LogStream::Stderr, pipe, logs);
    }
}

/// Read lines from any byte source into a service log buffer.
///
/// `Read` is generic instead of taking `ChildStdout`/`ChildStderr` directly, so
/// readers can be tested without spawning processes.
pub fn forward_lines(
    label: &str,
    stream: LogStream,
    pipe: impl Read + Send + 'static,
    logs: Arc<LogBuffer>,
) {
    let label = label.to_string();

    thread::spawn(move || {
        let reader = BufReader::new(pipe);

        for line in reader.lines().map_while(Result::ok) {
            logs.push(LogEntry::output(&label, stream, line));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readers_forward_lines_into_the_buffer() {
        let logs = Arc::new(LogBuffer::new(8));

        forward_lines(
            "frontend",
            LogStream::Stderr,
            std::io::Cursor::new("line one\nline two\n"),
            Arc::clone(&logs),
        );

        // The reader runs on its own thread; give it a moment to drain the pipe.
        for _ in 0..50 {
            if logs.len() == 2 {
                break;
            }

            thread::sleep(std::time::Duration::from_millis(10));
        }

        let messages: Vec<String> = logs
            .snapshot(None)
            .into_iter()
            .map(|entry| entry.message)
            .collect();

        assert_eq!(messages, vec!["line one", "line two"]);
    }
}