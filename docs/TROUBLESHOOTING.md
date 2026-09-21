# Troubleshooting

## Start does nothing useful

Read the message banner and the activity feed first; they carry the engine's
exact reason. Common cases:

- "No startup step found": the project has no dev/start/serve command. The
  Startup Plan panel shows what was detected and what is missing.
- "already running (pid N)": the service is up, possibly under another
  project. The Services card shows which project owns it when it is tracked.
- Port already in use: System Diagnostics lists the conflict under warnings.

## A service shows Stopped but the app is up

The scanner records framework default ports plus `--port` hints. If the app
listens elsewhere, the port probe stays red but a Pilot-started process still
shows Running via process tracking. External processes on unexpected ports
cannot be attributed; start the service through Pilot instead.

## Database operation fails

The Database panel prints the exit code plus the captured output tail. Check
that the tool exists on PATH (`prisma`, `python`/`manage.py`, `alembic`,
`psql`), that the database server runs, and that connection settings match.
Reset/restore always ask for confirmation first.

## Docker panel says the daemon is not running

The Docker CLI exists but the daemon does not. Start Docker Desktop (or the
daemon), then Refresh. Compose up/down need both daemon and compose plugin;
System Diagnostics shows each independently.

## Frontend button disabled

"Frontend Not Running" means no frontend service is in Running state. Start
the frontend (or Start all), wait for the port check, then open it.

## Kill All Nodes vs Stop All

Stop All asks processes to exit and waits. If something hangs (common with
crashed dev servers holding a port), Kill All Nodes force-terminates only
Pilot-tracked processes of the current project, after confirmation.

## Still stuck

Run Diagnostics and System Diagnostics, read the evidence lines, and check
the Live Logs for the service. Every check states its evidence and a
recommended action; paste those (not screenshots of the whole window) when
asking for help.
