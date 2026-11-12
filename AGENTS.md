# Development rules

- Active implementation: Rust in `console/`. Embedded HTML/CSS and small vanilla JavaScript are allowed; no Node/TypeScript/WASM build pipeline.
- Scope: one local server, one vLLM instance, explicit GPU UUID allocation, configuration, jobs and logs. Do not add multi-tenant, Kubernetes, remote scheduling, or platform services without a concrete request.
- `legacy/` contains migration reference code, not the new control plane. Do not revive Notebook/Jupyter.
- One runtime owner per model instance. Never stop foreign containers or workloads. Spawn argument vectors, never interpolate shell commands.
- Every commit MUST include a new development note in `docs/development/` describing intent, changes, verification and limitations. Use `Dev-Note: docs/development/<file>.md` in the commit message. Run `python3 scripts/check-dev-note.py --staged` before committing.
- Validate changes with `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `python3 scripts/smoke-console.py` after building. GPU claims require target hardware evidence.
