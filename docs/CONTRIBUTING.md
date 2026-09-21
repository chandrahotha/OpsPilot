# Contributing

1. Fork and branch from `main`.
2. Keep changes small and scoped; do not rewrite working engine code to
   support a UI change unless there is no other way.
3. Follow the product rules: no fake success, no invented state, no silent
   failures. Every new control must call a real engine path.
4. Update or add tests with the change:
   - Rust: `cargo test --workspace`
   - TypeScript: `npm run typecheck`
   - Launcher: `npm --workspace ops-pilot test`
5. Keep the tree warning-free and formatted: `cargo fmt --all`,
   `cargo clippy --workspace --all-targets -- -D warnings`, `npm run check`
   (typecheck plus all tests) must pass before opening a PR. The `Check`
   workflow (`.github/workflows/check.yml`) enforces all of this on every
   push and pull request.
6. Update the relevant file in `docs/` when behavior changes.
7. Describe what you verified against a real project in the PR text.
