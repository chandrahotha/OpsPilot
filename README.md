# OpsPilot — Project Command Center

<div align="center">
  <img width="1895" height="725" alt="OpsPilot Dashboard" src="https://github.com/user-attachments/assets/8de1986f-cc08-4386-a510-40df82dd852b" />
  <br/><br/>
  <strong>Install once. Open any project. Pilot tells you what it is, starts what it needs, and shows you everything that happens.</strong>
</div>

---

## What it is

Developers keep generating full-stack projects they then struggle to operate:
which service starts first, which port it uses, how the database migrates,
where the logs go. OpsPilot turns an unfamiliar project directory into an
operations console. It scans the project, reports what it found with evidence,
starts services with one click, streams their logs, runs diagnostics, and
stops everything cleanly when you are done.

Two interfaces share one Rust engine:

- **Desktop command center** (Tauri + React): service cards, startup plan,
  one-click Start all / Stop all, Docker and database panels, runnable
  scripts, live logs with activity feed, diagnostics, and system health.
- **CLI** (`pilot [path] [--json]`): scan any directory and get a
  human-readable or machine-readable report.

## How it works

1. **Scan.** Every detector (Node.js, Python/Django, Prisma, Alembic, Docker,
   Cargo, environment files) checks its marker files and contributes evidence
   to one normalized `ProjectModel`. Nothing is executed during a scan.
2. **Plan.** Detected capabilities plus declared commands become an ordered
   startup plan with validation and honest warnings for what is not automatic.
3. **Run.** Each step spawns through the platform shell as a tracked process
   with piped output, a full process-tree stop, and a force-kill path for
   stuck processes. On Windows everything runs windowless.
4. **Observe.** Service state comes from real TCP probes merged with tracked
   process state. Logs stream into per-service buffers. Diagnostics report
   problem, evidence, cause, and recommended action for every check.

Design rules: no fake success, every detection cites evidence, Pilot tracks
only processes it started, and the engine runs unchanged on Windows, macOS,
and Linux.

---

## Install

### Windows installer

Download `OpsPilot_0.1.0_x64-setup.exe` from
[GitHub Releases](https://github.com/chandrahotha/OpsPilot/releases) and run
it. Published by Digi Tracks.

### Global CLI (npm)

```bash
npm install -g ops-pilot
cd my-project
pilot
```

### From source

```bash
git clone https://github.com/chandrahotha/OpsPilot
cd OpsPilot
npm install
cargo build --release --bin pilot     # native CLI
npm install -g ./pilot/packages/cli   # expose it as `pilot`
npm run dev                           # Tauri development window
```

### Build the installer from source (Windows)

Requires [NSIS 3.x](https://nsis.sourceforge.io/)
(`winget install NSIS.NSIS`) so `makensis` is on PATH:

```bash
cd pilot/apps/desktop
npm run build   # vite bundle + tauri build → target/release/bundle/nsis/OpsPilot_0.1.0_x64-setup.exe
```

---

## Use

### Desktop

Open the app, choose **Select Project** (or start Pilot inside a project
folder; the last project reopens automatically). The header shows the project
name and one overall status: READY, STARTING, RUNNING, STOPPING, DEGRADED,
FAILED, or NO PROJECT.

- **Services**: one card per detected service with live state, port, and the
  process behind it. Start, Stop, and Restart appear only where the startup
  plan can actually run them.
- **Start all / Stop all**: compose stack first, then every plan step;
  already-running items are skipped and blocked items explain why. Kill All
  Nodes force-terminates stuck project processes after confirmation.
- **Open Frontend**: enabled while the frontend listens; opens the detected
  URL in the default browser.
- **Docker panel**: daemon status, container list with start/stop/restart/
  logs, compose up/down. **Database panel**: status, migrate, seed, backup,
  plus confirmed reset/restore for Prisma, Django, Alembic, and PostgreSQL.
- **Scripts panel**: every declared project command, runnable as a tracked
  process with stop support.
- **Live logs**: auto-refreshing process output with pause, search, stream
  filter, and copy, plus a session activity feed of detections, starts,
  stops, failures, and diagnostics.
- **Diagnostics / System Diagnostics**: deterministic checks and subsystem
  health with per-service execution readiness and reasons.

### CLI

```bash
pilot                  # scan current directory
pilot /path/to/project # scan specific directory
pilot --json           # machine-readable ProjectModel
pilot --help           # usage
```

```json
{
  "detected": true,
  "model": {
    "project": { "name": "my-app", "path": "/home/user/my-app" },
    "frontend": { "framework": "nextjs", "port": 3000 },
    "backend": { "framework": "fastapi", "port": 8000 },
    "database": { "type": "postgresql", "port": 5432 },
    "orm": { "type": "prisma" },
    "docker": { "detected": true, "compose": true },
    "environment": { "envFile": true, "envExample": true, "envLocal": false },
    "commands": [
      { "name": "dev", "command": "npm run dev", "source": "package.json scripts" },
      { "name": "start", "command": "npm run start", "source": "package.json scripts" }
    ]
  },
  "evidence": [
    "frontend nextjs on port 3000",
    "datasource provider postgresql on port 5432",
    "docker-compose.yml (docker compose)",
    "package.json (node)",
    "prisma/schema.prisma (prisma)"
  ]
}
```

---

## Project structure

```text
OpsPilot/
├── crates/
│   ├── core/            → Normalized project model + scan result (shared by all front ends)
│   ├── scanner/         → Modular detectors (Node.js, Python, Django, Prisma, Alembic, Docker, Cargo, env)
│   ├── port-manager/    → TCP port inspection, process ownership, free-port search
│   ├── diagnostics/     → Deterministic checks with problem/evidence/cause/action
│   ├── process-manager/ → Lifecycle: start/stop/restart/kill, log capture, tree kill, history, plans
│   ├── docker/          → Docker operations (containers, compose up/down, logs)
│   └── database/        → Database operations (Prisma/Django/Alembic/Postgres)
├── pilot/
│   ├── apps/desktop/    → Tauri + React command center
│   └── packages/shared/ → Shared TypeScript contracts mirroring the engine
├── src/main.rs          → `pilot` CLI binary
├── tests/cli.rs         → End-to-end CLI tests against real fixtures
├── docs/                → Architecture, detection, processes, logging, troubleshooting, security
└── Cargo.toml           → Rust workspace
```

## Scripts

| Command | Purpose |
|---------|---------|
| `npm run test` | All tests: Rust workspace + npm launcher |
| `npm run typecheck` | TypeScript check for shared package + desktop app |
| `npm run check` | Typecheck + all tests |
| `npm run build` | Rust workspace build + web bundle |
| `npm run build:cli` | Release build of the native `pilot` binary |
| `npm run install:global` | Build CLI and install globally as `pilot` |
| `npm run pilot -- <path>` | Run CLI through npm launcher |
| `npm run dev` | Tauri development window |
| `npm run audit` | Security audit (npm + cargo) |
| `cargo fmt --all` | Format the Rust workspace |
| `cargo clippy --workspace --all-targets -- -D warnings` | Lint, warnings deny |

---

## What works today

| Area | Details |
|------|---------|
| **Modular scanner** | Node.js, Python, Django, Prisma, Alembic, Docker, Cargo, environment files. Every finding carries its evidence. |
| **Normalized project model** | One canonical `ProjectModel` shared by CLI, GUI, and operations (`pilot-core`). |
| **CLI** | `pilot [path] [--json]`. Human-readable or JSON output; declares every detected command. |
| **Dynamic GUI** | Dashboard renders only detected capabilities. No Docker project shows no Docker panel. |
| **Service status** | Real TCP probes merged with tracked-process state scoped to the current project. |
| **Process lifecycle** | Start, Stop, Restart, Start all, Stop all, confirmed Kill All Nodes. Tree kill, log capture, history. Windowless spawning on Windows. |
| **Startup plans** | Ordered steps with exact commands plus honest warnings, shown before anything runs. |
| **Docker operations** | Daemon status, container list, start/stop/restart/logs, compose up/down. |
| **Database operations** | Status, migrate, seed, backup directly; reset/restore behind two-step confirmation. |
| **Scripts** | Declared project commands run as tracked processes with stop support. |
| **Live logs** | Auto-refreshing output with pause, search, stream filter, copy, plus a session activity feed. |
| **Diagnostics** | Runtimes, dependencies, environment, database and service reachability. Each check states problem, evidence, cause, and recommended action. |
| **System diagnostics** | Subsystem health plus per-service execution readiness with reasons. |
| **Open Frontend** | Detected URL opened in the default browser, only while actually listening. |

---

## Testing

- **Rust**: `cargo test --workspace` — 142 tests across core, scanner, port-manager, diagnostics, process-manager, docker, database, desktop shell, CLI.
- **TypeScript**: `npm run typecheck` — shared contracts + desktop app.
- **Launcher**: `node --test` — 9 resolution-order tests.
- **Total**: **151 tests**, all passing, zero compiler warnings, zero clippy warnings (`-D warnings`), `cargo fmt --check` clean.
- Fixtures are created in temp directories; nothing is written outside them.
- `npm run check` (typecheck + all tests) must pass before any PR; the `Check` workflow enforces format, clippy, typecheck, tests, and the web build on every push and PR.

---

## Security

| Check | Result |
|-------|--------|
| `npm audit` | **0 vulnerabilities** |
| `cargo audit --file Cargo.lock` | **0 vulnerabilities** (unmaintained/unsound warnings are transitive in Tauri's tree; only Tauri can clear them) |

Re-check anytime: `npm run audit` (requires `cargo install cargo-audit` once).

Safety model: Pilot runs only declared project commands, tracks only processes
it started, scopes stops to the current project, confirms destructive database
operations and force-kills every time, never reads `.env` contents, and sends
no data anywhere. See `docs/SECURITY.md`.

---

## Roadmap

| Phase | Scope | Status |
|-------|-------|--------|
| 1–9 | Architecture, scanner, dynamic UI, process lifecycle, Docker, database, ports, diagnostics, logs | ✅ Done |
| 10 | AI layer (optional) | 📋 Planned |
| 11 | Packaging: installers, platform binary packages | 🔨 Done for Windows NSIS locally; CI builds all platforms on tag |
| 12 | Release: npm publish, GitHub Releases | 🔨 CI workflow ready (`.github/workflows/release.yml`); npm publish pending registry access |

---

## Documentation

| Document | Covers |
| -------- | ------ |
| `docs/ARCHITECTURE.md` | Engine crates, Tauri shell, React frontend, CLI, data flow |
| `docs/PROJECT-DETECTION.md` | How the scanner determines what a project contains |
| `docs/COMMAND-CENTER.md` | The UI and how each control maps to real engine state |
| `docs/PROCESS-MANAGEMENT.md` | Start, stop, restart, force-kill, state, and safety rules |
| `docs/LOGGING.md` | Log collection, live logs, and the activity feed |
| `docs/TROUBLESHOOTING.md` | Common failures and how to diagnose them |
| `docs/CONTRIBUTING.md` | How to contribute |
| `docs/SECURITY.md` | Process execution scope, secrets, and safety notes |
| `Pilot Prerequisite.md` | The original product spec the implementation follows |

## Contributing

See `docs/CONTRIBUTING.md`. In short: fork, branch, keep `npm run check`
green, update docs with behavior changes, and describe what you verified
against a real project.

---

## License

MIT
