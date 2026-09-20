# OpsPilot — Cross-Platform Project Operations Launcher

<div align="center">
  <img width="1895" height="725" alt="OpsPilot Dashboard" src="https://github.com/user-attachments/assets/8de1986f-cc08-4386-a510-40df82dd852b" />
  <br/><br/>
  <strong>Install once. Open any project. Pilot tells you what it is and how to run it.</strong>
</div>

---

## Quick Start

### Global CLI (npm)

```bash
npm install -g ops-pilot
cd my-project
pilot
```

`pilot [path] [--json]` scans a directory and reports the detected stack, service ports, database, ORM, Docker and environment files — each with its evidence.

### From Source

```bash
git clone https://github.com/chandrahotha/OpsPilot
cd OpsPilot
npm install
cargo build --release --bin pilot     # native CLI
npm install -g ./pilot/packages/cli   # expose it as `pilot`
npm run dev                           # Tauri window
```

| Command | Purpose |
```
OpsPilot/
├── crates/
│   ├── core/            → Normalized project model + scan result (shared by all front ends)
│   ├── scanner/         → Modular detectors (Node.js, Python, Django, Prisma, Alembic, Docker, Cargo, env)
│   ├── port-manager/    → TCP port inspection + free-port search
│   ├── diagnostics/     → Deterministic checks with problem/evidence/cause/action
│   ├── process-manager/ → Lifecycle: start/stop/restart/status, log capture, tree kill, history
│   ├── docker/          → Docker operations (Phase 5)
│   ├── database/        → Database operations (Phase 6)
│   └── integrations/    → Prisma/Django/Alembic/Node/Python operation layer
├── pilot/
│   ├── apps/desktop/    → Tauri + React application
│   └── packages/shared/ → Shared TypeScript contracts (model, commands, diagnostics)
├── src/main.rs          → `pilot` CLI binary
├── tests/cli.rs         → End-to-end CLI tests against real fixtures
└── Cargo.toml           → Rust workspace
```

### Design Principles

1. **No fake success** — Every operation either happens or returns an explicit `NotImplemented` with the phase it belongs to
2. **Evidence-based** — Every detection, status, and diagnostic cites its source (file, port, command output)
3. **Safety first** — Pilot tracks only processes it started; never kills unrelated system processes
4. **Platform-independent engine** — Rust core runs on Windows, macOS, Linux; only the shell layer differs
5. **Single source of truth** — The normalized `ProjectModel` is the contract between scanner, CLI, GUI, and integrations

---

## CLI Usage

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

## Security

| Check | Result |
|-------|--------|
| `npm audit` | **0 vulnerabilities** (vite 7.3.6 / plugin-react 5.2.0 cleared GHSA-67mh-4wv8-2f99) |
| `cargo audit --file Cargo.lock` | **0 vulnerabilities**, 7 unmaintained/unsound warnings (all transitive in Tauri's tree) |

The Rust warnings are transitive only: `glib` (Linux GTK), `proc-macro-error` (non-Windows), five `unic-*` crates via `urlpattern` → `tauri-utils`. Only Tauri can clear them.

Re-check anytime: `npm run audit` (requires `cargo install cargo-audit` once).

---

## Testing

- **Rust**: `cargo test --workspace` — 92 tests across core, scanner, port-manager, diagnostics, process-manager, docker, database
- **CLI**: `tests/cli.rs` — executes real binary against throwaway fixture projects
- **TypeScript**: `npm run typecheck` — shared contracts + desktop app
- **Launcher**: `node --test` — 9 resolution-order tests
- **Total**: **101 tests**, all passing
- **Zero machine-dependent tests** — fixtures created in temp directories, nothing written outside

---

## Roadmap

| Phase | Scope | Status |
|-------|-------|--------|
| 1 | Architecture: repository, Tauri shell, Rust core, project model, basic GUI | ✅ Done |
| 2 | Scanner: project detection | ✅ Done |
| 3 | Dynamic UI: menus from detected capabilities | ✅ Done |
| 4 | Process lifecycle: start, stop, restart, status, logs, history, startup plans | ✅ **Done** |
| 5 | Docker: containers, compose, start/stop/restart/logs | ✅ **Done** |
| 6 | Database: Prisma/PostgreSQL, Django, Alembic (migrate/seed/reset + confirmations) | ✅ **Done** |
| 7 | Ports: per-port process ownership, safe port changes | ✅ **Done** |
| 8 | Diagnostics: database/service reachability | 📋 Planned |
| 9 | Logs: unified streaming UI | 📋 Planned |
| 10 | AI layer (optional) | 📋 Planned |
| 11 | Packaging: Windows/macOS/Linux installers, platform binary packages | 📋 Planned |
| 12 | Release: npm publish, GitHub Releases | 📋 Planned |

---

## Contributing

1. Fork the repo
2. Create a feature branch
3. `npm run check` must pass (typecheck + all tests)
4. Open a PR with a clear description

---

## License

MIT
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

---

## What Works Today

| Area | Status | Details |
|------|--------|---------|
| **Modular Scanner** | ✅ **Done** | Node.js, Python, Django, Prisma, Alembic, Docker, Cargo, environment files — each finding includes its evidence |
| **Normalized Project Model** | ✅ **Done** | One canonical `ProjectModel` shared by CLI, GUI, and integrations (`pilot-core`) |
| **CLI** | ✅ **Done** | `pilot [path] [--json]` — human-readable or JSON output; declares every detected command |
| **Dynamic GUI** | ✅ **Done** | Tauri + React; dashboard renders *only* detected capabilities |
| **Service Status** | ✅ **Done** | Real TCP port observation; a service is "running" only when its port accepts connections |
| **Port Management** | ✅ **Done** | Inspect, find-free-port, **process ownership (PID + name + command)**, safe port changes in project files |
| **Deterministic Diagnostics** | ✅ **Done** | Runtimes, dependencies, environment — problem, evidence, cause, recommended action |
| **Process Lifecycle** | ✅ **Done** | Start / Stop / Restart / Status / Logs / History — real processes, tree kill, log capture, operation history |
| **Startup Plans** | ✅ **Done** | Detected capabilities + declared commands → ordered steps + validation + warnings |
| **Docker Operations** | ✅ **Done** | Container list, start/stop/restart/logs, compose awareness |
| **Database Operations** | ✅ **Done** | Migrate / seed / reset with destructive-operation confirmations (Prisma, Django, Alembic, PostgreSQL) |

---

## Architecture