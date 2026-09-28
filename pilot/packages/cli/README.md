# ops_pilot

Install once, then open any project:

```bash
npm install -g ops_pilot
cd my-project
pilot
```

`pilot [path] [--json]` scans a directory and reports what the project is:
detected stack, service ports, database, ORM, Docker and environment files, each
with the evidence it is based on.

```bash
pilot                # scan the current directory
pilot ../other-app   # scan another directory
pilot --json         # print the normalized project model as JSON
pilot --help
```

## How this package works

The npm package is a thin Node launcher; the engine is the native `pilot` binary
written in Rust. The launcher resolves that binary in this order:

1. `OPS_PILOT_BINARY` - explicit override
2. `@ops_pilot/cli-<platform>-<arch>` - optional platform package carrying the binary
3. `bin/pilot-<platform>-<arch>[.exe]` inside this package - populated by the release pipeline
4. `target/{release,debug}/pilot[.exe]` in the repository - development builds

If none of them exists, the launcher prints exactly what it searched and how to
build the binary (`cargo build --release --bin pilot`) instead of failing silently.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Scan completed (including "no project detected") |
| 1 | The native binary is missing or could not be started |
| 2 | Invalid usage (unknown option, or the path is not a directory) |

## Status

Scanning, service status and diagnostics are implemented. The desktop
application (see the repository root) adds process lifecycle (start, stop,
restart, start-all, stop-all, force-kill), Docker and database operations,
script running, live logs, and system diagnostics on top of the same engine.

Repository: https://github.com/chandrahotha/OpsPilot