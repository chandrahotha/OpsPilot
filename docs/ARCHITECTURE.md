# OpsPilot Architecture

OpsPilot is a project operations launcher: point it at an unfamiliar project
directory and it tells you what the project is, starts what is required,
and shows you what is happening. The codebase has two layers.

## Rust engine (`crates/`)

The engine does all detection and all operations. It has no UI code.

| Crate | Job |
| ----- | --- |
| `core` | The normalized `ProjectModel` and `ScanResult`. This is the single contract between scanner, CLI, GUI, and operations. |
| `scanner` | Read-only detectors (Node.js, Python/Django, Prisma, Alembic, Docker, Cargo, environment files). Each finding carries its evidence. |
| `port-manager` | TCP port probing, process ownership lookup, free-port search. |
| `diagnostics` | Deterministic checks (runtimes, dependencies, environment, reachability). Every check reports problem, evidence, cause, and recommended action. |
| `process-manager` | Process lifecycle: start, stop, restart, force-kill, status, log capture, operation history, startup plans. |
| `docker` | Docker CLI operations: daemon status, containers, compose up/down, logs. |
| `database` | Database operations for Prisma, Django, Alembic, and raw PostgreSQL: status, migrate, seed, backup, confirmed reset/restore. |

The engine never invents state. Detection is file evidence, "running" means a
port accepts connections or Pilot itself tracks a live process, and every
operation reports its real outcome.

## Desktop shell (`pilot/apps/desktop/src-tauri`)

A thin Tauri layer. Each command resolves the project path, scans, delegates
to exactly one engine function, and returns the result. There is no business
logic here beyond wiring (command list in `src/main.rs`, port-observed plus
process-tracked status in `src/status.rs`).

## React frontend (`pilot/apps/desktop/src`)

Plain React state, no store library. All engine access goes through `api.ts`
(one typed function per Tauri command). The dashboard renders only detected
capabilities: panels appear when the scan found something for them to operate.

## CLI (`src/main.rs` + `pilot/packages/cli`)

The `pilot` binary prints a human or JSON scan report for a directory. The npm
package resolves to a platform binary and exposes it as the `pilot` command.

## Data flow

```text
Select directory -> detect_project -> ScanResult -> dashboard state
                                          |
              get_status (5s poll) -> ServiceStatus[] -> service cards
              get_startup_plan     -> StartupPlan     -> plan panel
              start/stop/restart   -> message string  -> banner + activity feed
              logs/panels          -> on demand       -> side panels
```

There is no push channel from backend to frontend. Service state refreshes on
a 5 second poll plus an immediate refresh after every lifecycle action.
