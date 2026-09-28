<div align="center">
  <img width="280" alt="OpsPilot" src="https://raw.githubusercontent.com/chandrahotha/OpsPilot/main/docs/assets/logo.png" />
</div>

# @ops_pilot/cli

<div align="center">

[![npm](https://img.shields.io/npm/v/@ops_pilot/cli.svg)](https://www.npmjs.com/package/@ops_pilot/cli)
[![License: Apache 2.0](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](https://github.com/chandrahotha/OpsPilot/blob/main/LICENSE)
[![Node](https://img.shields.io/badge/node-%3E%3D18-339933.svg)](package.json)
[![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey.svg)](#how-this-package-works)

</div>

Install once, then open any project:

```bash
npm install -g @ops_pilot/cli
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