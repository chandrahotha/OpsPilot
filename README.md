# OpsPilot - Cross-Platform Project Operations Launcher

<img src="README-banner.png" alt="OpsPilot Banner" style="width:100%; max-width:800px; margin: 2rem 0;">

> Install once, then: `cd any-project` → `ops-pilot`

OpsPilot detects an unfamiliar project directory and opens a control panel for
operating it: services, ports, environment and diagnostics.

The engine is written in Rust and is platform-independent. The GUI is Tauri +
React and renders only what the engine actually detected.

---

## Quick Start (from source)

```bash
git clone https://github.com/chandrahotha/OpsPilot
cd OpsPilot
npm install
npm run dev          # opens the Tauri window
```

Scan a directory from the CLI:

```bash
npm run pilot -- .            # human-readable summary
npm run pilot -- . --json     # normalized project model as JSON
```

## What works today

| Area | Status |
| --- | --- |
| Modular scanner (Node.js, Python, Django, Prisma, Alembic, Docker, environment) | Implemented, with evidence per finding |
| Normalized project model shared by CLI and GUI | Implemented (`pilot-core`) |
| `pilot` CLI: scan a directory, print the model as text or JSON | Implemented |
| Dynamic GUI: sections appear only for detected capabilities | Implemented |
| Service status from real TCP port observation | Implemented (`pilot-port-manager`) |
| Port inspect and find-free-port | Implemented (per-port process ownership is not) |
| Deterministic diagnostics (runtimes, dependencies, environment) | Implemented (`pilot-diagnostics`) |
| Process lifecycle (start / stop / restart) | Phase 4 - commands return an explicit "not implemented" error |
| Docker operations | Phase 5 - returns `DockerOutcome::NotImplemented` |
| Database operations (migrate / seed / reset) | Phase 6 - destructive ones require confirmation first |
---

## Features

- **Project Detection** - identifies the stack from manifests, lockfiles, compose files and schema files
- **Project Model** - one normalized model (`pilot-core`) consumed by both the CLI and the GUI
- **Dynamic GUI** - the dashboard is built from the detected capabilities
- **Evidence** - every detection lists the file or declaration it is based on
- **Service Status** - a service counts as running only when its port accepts connections
- **Port Management** - inspect ports, find a free port near a preferred one
- **Diagnostics** - problem, evidence, possible cause and recommended action per check
- **Environment Awareness** - presence of `.env` / `.env.example` / `.env.local`; values are never read
- **Safety First** - the scanner only reads files; it never executes project commands

## Technology Stack

- **GUI**: Tauri 2 + React 19 + TypeScript
- **Core**: Rust (edition 2024), modular crates
- **Contracts**: `serde` JSON shared between Rust and TypeScript (`pilot/packages/shared`)

## Repository Layout

```
OpsPilot/
├── crates/
│   ├── core/            normalized project model + scan result
│   ├── scanner/         detection engine (one module per stack)
│   ├── port-manager/    port inspection and conflict handling
│   ├── diagnostics/     deterministic health checks
│   ├── process-manager/ lifecycle contract (phase 4)
│   ├── docker/          Docker operations (phase 5)
│   ├── database/        database operations (phase 6)
│   └── integrations/    Prisma/Django/Alembic/Node/Python operations
├── pilot/
│   ├── apps/desktop/    Tauri + React application
│   └── packages/shared/ shared TypeScript contracts
├── src/main.rs          the `pilot` CLI
├── tests/cli.rs         end-to-end CLI tests
└── Cargo.toml           Rust workspace
```

## Development Commands

| Command | Purpose |
| --- | --- |
| `npm run test` | `cargo test --workspace` (unit + integration tests) |
| `npm run typecheck` | TypeScript check for the shared package and the desktop app |
| `npm run check` | Typecheck + Rust tests |
| `npm run build` | Rust workspace build + web bundle |
| `npm run pilot -- <path>` | Run the CLI against a directory |
| `npm run dev` | Tauri development window |

## Testing

- Rust: `cargo test --workspace` - unit tests per crate plus integration tests in
  `crates/scanner/tests` that build throwaway fixture projects in the system temp directory.
- CLI: `tests/cli.rs` executes the real binary against fixtures.
- TypeScript: `npm run typecheck`.
- Tests never depend on the developer's machine configuration, and nothing is
  written outside temp directories.

## Roadmap

| Phase | Scope | Status |
| --- | --- | --- |
| 1 | Architecture: repository, Tauri shell, Rust core, project model, basic GUI | Done |
| 2 | Scanner: project detection | Done |
| 3 | Dynamic UI: menus from detected capabilities | Done |
| 4 | Process lifecycle: start, stop, restart, status | Next |
| 5 | Docker | Planned |
| 6 | Database (Prisma/PostgreSQL first, then Django/Alembic) | Planned |
| 7 | Ports: per-port process ownership, safe port changes | Planned |
| 8 | Diagnostics: database and service reachability | Planned |
| 9 | Logs | Planned |
| 10 | AI layer (optional) | Planned |
| 11 | Packaging (Windows, macOS, Linux) | Planned |
| 12 | Release (npm, GitHub Releases) | Planned |

## License

MIT
| Logs, AI layer, packaging, release | Phases 9-12 |

Nothing is reported as done unless it happened: work that belongs to a later
phase returns an explicit `NotImplemented` result instead of a fake success.