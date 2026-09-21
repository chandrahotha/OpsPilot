# Process Management

Implemented in `crates/process-manager`, exposed through Tauri commands in
`pilot/apps/desktop/src-tauri/src/main.rs`.

## Starting

`start_project` rebuilds the startup plan for the current project, finds the
step for the requested service key, validates it (working directory exists,
runtime available), and spawns it through the platform shell with piped
stdout/stderr. On Windows children are created with `CREATE_NO_WINDOW`, so no
console windows appear. Starting an already-running label is rejected with the
pid and a pointer to stop it first; if that label belongs to another project,
the error names that project.

`start_all_project` starts the compose stack first (databases before servers),
then every plan step, skipping what already runs and reporting what is
blocked and why.

## Stopping

`stop_project` signals the whole process tree (`taskkill /PID /T /F` on
Windows, SIGTERM to the process group on Unix), waits up to 5 seconds, then
kills the direct child if the tree did not exit. `stop_all_project` stops
every process Pilot tracks for the current project (matched by working
directory, so other projects are untouched), then brings the compose stack
down.

## Force termination

`kill` skips the graceful wait: signal the tree, kill the child immediately,
record "force terminated by Pilot". `kill_all_project` applies it to every
tracked process of the current project and reports killed, already-gone, and
failed counts. The GUI confirms before invoking it. Only Pilot-tracked
processes are ever targeted; unrelated system processes are never touched.

## Restart

Stop, then start with the same recorded request. Restarting a label Pilot
never started reports "not found or not started by Pilot".

## State and safety

- Pilot tracks only processes it started, keyed by label.
- `status` refreshes liveness from the OS before answering.
- Labels are global to the Pilot session; the GUI scopes operations to the
  current project by working directory.
- No silent failures: every outcome is Started, Snapshot, Stopped, NotFound,
  or Error with a readable message.
