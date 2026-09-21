# Command Center

The desktop window is a command center over the engine described in
ARCHITECTURE.md. Every panel below is driven by a real scan result or a real
engine call. Nothing is shown for capabilities the project does not have.

## Header

Brand, current project name and path, one overall status chip, the Open
Frontend button, project selection, and scan logs. The status chip is derived
from live state:

- NO PROJECT: nothing detected.
- READY: detected, nothing running, no issues.
- STARTING / STOPPING: an action is in flight.
- RUNNING: at least one service running.
- DEGRADED: running, but diagnostics report issues.
- FAILED: not running, and diagnostics report issues.

## Services

One card per detected service (frontend, backend, app, database, docker).
State is observed, not assumed: a port accepting connections, or a process
Pilot started for this project. Start/Stop/Restart buttons appear only for
services the startup plan can actually start; database and docker cards point
at their panels instead of offering dead buttons. While an action runs, the
card shows an orange in-flight state and locks against double clicks.

## Startup plan

The ordered steps Pilot would run, with the exact command and working
directory, plus honest notes for what is not automatic (migrations, compose).
Each step has its own Start button.

## Actions

- Start all: compose stack first, then every plan step. Already-running items
  are skipped, blocked items report why. Returns a per-item summary.
- Stop all: gracefully stops everything Pilot started for this project.
- Kill All Nodes: force-terminates stuck project processes (confirmed first).
- Diagnostics, Rescan.

## Open Frontend

Enabled only while the frontend service is running. Opens
`http://localhost:<detected port>` in the default browser after verifying
something actually listens there. The frontend card shows its own Open button
under the same condition.

## Panels

Docker, Database, Scripts, Capabilities/Evidence, Diagnostics, System
Diagnostics, and Live Logs. Each panel calls its engine commands directly and
reports real outcomes, including daemon-down, command-missing, and
confirmation-required cases.
