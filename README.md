# OpsPilot - Cross-Platform Project Operations Launcher

<img width="1895" height="725" alt="image" src="https://github.com/user-attachments/assets/8de1986f-cc08-4386-a510-40df82dd852b" />

> Install once, then: `cd any-project` → `ops-pilot`

OpsPilot detects an unfamiliar project directory and opens a control panel for
operating it: services, ports, environment and diagnostics.

The engine is written in Rust and is platform-independent. The GUI is Tauri +
React and renders only what the engine actually detected.

---

## Install

### Global CLI (npm)

```bash
npm install -g ops-pilot
cd my-project
pilot
```

`pilot [path] [--json]` scans a directory and reports the detected stack, service
ports, database, ORM, Docker and environment files, each with its evidence.

The npm package is a thin Node launcher (`pilot/packages/cli`) that runs the
native `pilot` binary. It resolves that binary in this order:

1. `OPS_PILOT_BINARY` - explicit override
2. `@ops-pilot/cli-<platform>-<arch>` - optional platform package carrying the binary
3. `bin/pilot[.exe]` inside the package - populated by the release pipeline (phase 11)
4. `target/{release,debug}/pilot[.exe]` in this repository - development builds

If none exists, the launcher prints exactly what it searched and how to build the
binary, instead of failing silently.

### From source

```bash
git clone https://github.com/chandrahotha/OpsPilot
cd OpsPilot
npm install
cargo build --release --bin pilot     # native CLI
npm install -g ./pilot/packages/cli   # expose it as `pilot`
npm run dev                           # Tauri window
```

Shortcuts: `npm run install:global` builds the CLI and installs it globally;
`npm run uninstall:global` removes it.

### CLI shortcuts (repository)

```bash
npm run pilot -- .            # human-readable summary via the npm launcher
npm run pilot -- . --json     # normalized project model as JSON
npm run pilot:dev -- .        # same, straight through cargo
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
| `npm run test` | All tests: Rust workspace + npm launcher |
| `npm run test:rust` | `cargo test --workspace` |
| `npm run test:cli` | Launcher resolution tests (`node --test`) |
| `npm run typecheck` | TypeScript check for the shared package and the desktop app |
| `npm run check` | Typecheck + all tests |
| `npm run build` | Rust workspace build + web bundle |
| `npm run build:cli` | Release build of the native `pilot` binary |
| `npm run install:global` | Build the CLI and install it globally as `pilot` |
| `npm run pilot -- <path>` | Run the CLI through the npm launcher |
| `npm run dev` | Tauri development window |

## Security

| Check | Result |
| --- | --- |
| `npm audit` | 0 vulnerabilities (vite 7.3.6 / plugin-react 5.2.0 cleared the esbuild dev-server advisory GHSA-67mh-4wv8-2f99) |
| `cargo audit --file Cargo.lock` | 0 vulnerabilities, 7 unmaintained/unsound warnings, all transitive in Tauri's dependency tree |

The Rust warnings, none of which are vulnerable versions:

| Crate | Version | Advisory | Kind | Reachable from |
| --- | --- | --- | --- | --- |
| `glib` | 0.18.5 | RUSTSEC-2024-0429 | unsound | Tauri's Linux GTK stack (not built on Windows) |
| `proc-macro-error` | 1.0.4 | RUSTSEC-2024-0370 | unmaintained | non-Windows targets |
| `unic-char-property`, `unic-char-range`, `unic-common`, `unic-ucd-ident`, `unic-ucd-version` | 0.9.0 | RUSTSEC-2025-0081 / -0075 / -0080 / -0100 / -0098 | unmaintained | `urlpattern` → `tauri-utils` |

They can only be cleared upstream by Tauri (its Linux backend still uses GTK3);
there is nothing to fix in OpsPilot's own code. Re-check with
`npm run audit`, which needs `cargo install cargo-audit` once per machine.

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
