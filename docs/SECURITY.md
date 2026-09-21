# Security

OpsPilot executes project commands on your machine. The model is: you chose
the project, you pressed the button, Pilot runs exactly what the project
declares and reports what happened.

## What Pilot runs

- Only commands declared in the scanned project (startup plan steps, declared
  scripts) or explicit Docker/database CLIs for detected integrations.
- `run_script` rejects anything not declared in the project model.
- Destructive database operations (reset, restore) and Kill All Nodes require
  explicit confirmation in the GUI every time.

## Process scope

- Pilot tracks only processes it started. Stop/Stop All/Kill All Nodes
  address tracked processes of the current project (matched by working
  directory). Unrelated system processes are never enumerated for termination.
- Force termination is confined to tracked labels; there is no "kill by port
  owner" path.

## What Pilot never does

- Never reads `.env` contents (presence only).
- Never sends project data anywhere; there is no network code besides
  localhost port probes and opening the local frontend URL in your browser.
- The `open_frontend` path only opens `http://localhost:<detected port>`
  after verifying something listens there.

## Before publishing or sharing

- Keep secrets out of the repository: no `.env` files, tokens, or private
  paths. See `.gitignore`.
- The Tauri Content Security Policy is currently disabled (`csp: null`);
  re-enable a strict CSP before distributing beyond local use.
