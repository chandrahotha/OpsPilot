# Logging

## Collection

Every process Pilot starts gets a bounded ring buffer (1000 lines) fed by
dedicated stdout/stderr reader threads, plus system lines for lifecycle
events (starting, started with pid, stopping, exited with code, force
terminated). Database operations capture a 50-line tail into their outcome.
A broadcast stream also exists for real-time subscribers.

Logs live per service label and are readable through `get_service_logs`
(newest N lines) and `list_processes` (what is tracked, with pid and state).

## Display

The Live Logs panel (right side of the command center) shows:

- The tracked-process selector, with live state and pid per entry.
- Output auto-refreshed every 2 seconds while unpaused, auto-scrolled to the
  newest line, with Pause/Resume, text search, stdout/stderr/system filter,
  and Copy.
- The session Activity feed: detection results, starts, stops, failures,
  diagnostics outcomes, docker/database operations. Every entry is timestamped
  and severity-typed (info/success/warning/error).

Scan logs (project selection and detection evidence) stay under Show Logs in
the header. No log line is fabricated: empty states say no output was
captured, and errors show the real command, exit code, and captured tail.
