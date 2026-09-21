# Project Detection

Detection answers one question: "what is in this directory, and what can
Pilot control?" It is read-only. Nothing is executed during a scan.

## How it works

`scan_project(path)` runs every registered detector in a fixed order and
merges the results into one `ProjectModel`:

1. Each detector checks for its marker files (`package.json`, `manage.py`,
   `prisma/schema.prisma`, `alembic.ini`, `Dockerfile`, compose files,
   `.env*`, `Cargo.toml`, Python manifests).
2. Detectors that match append evidence strings and fill in their section of
   the model (framework, port, commands, database, ORM, docker, environment).
3. Evidence is sorted and deduplicated. With no evidence, the result is
   "no project detected" rather than a guess.

## What is detected

- Frontend framework and port (Next, Nuxt, Angular, React, SvelteKit, Vite),
  with explicit `--port` flags in scripts taking priority over defaults.
- Backend framework and port (NestJS, Express, Fastify, Koa, Django,
  FastAPI, Flask), including Django via `manage.py`.
- Database type and port from Prisma datasource providers (only inside the
  `datasource` block), compose service images, and Python database drivers.
- ORM: Prisma, Django, or Alembic.
- Docker: Dockerfile presence and compose files.
- Environment files: `.env`, `.env.example`/`.env.sample`, `.env.local`.
  Contents are never read.
- Declared commands from `package.json` scripts (dev/start/serve plus
  build/test/lint), with the correct runner syntax per lockfile
  (npm/yarn/pnpm/bun).

## Limits

- Ports are defaults plus script hints, not guarantees. A project can listen
  on a different port than detected; service status then relies on Pilot's
  tracked processes instead of the port.
- A `datasource provider` the scanner does not map is reported as evidence
  ("not mapped") rather than silently dropped.
- If two ORM markers exist, both are reported as evidence; the model keeps
  one ORM value.
