# Development rules

- New branches must use `feature/<module-name>`, for example `feature/gateway`. Keep `main` and `dev` under their existing names. Never use the `codex/` prefix.
- Active implementation: Rust in `console/`. The embedded console uses HTML/CSS and browser JavaScript modules, including a pinned React runtime. No Node/TypeScript/WASM build pipeline.
- Scope: one server and one model instance; host preparation, model artifacts, explicit GPU UUID allocation, jobs/logs, enterprise gateway, acceptance and release records. A bounded llama.cpp CPU backend supports local tests. Fuse responsibilities inspired by GPUStack/Xinference/Semaphore inside this product; do not deploy those platforms. No multi-tenant, Kubernetes or remote scheduling.
- `legacy/` contains migration reference code, not the new control plane. Do not revive Notebook/Jupyter.
- Implement improvements in the shared product and cover the affected configuration with regression tests. Keep server-specific paths, accounts, and device selections in deployment configuration.
- One runtime owner per model instance. Never stop foreign containers or workloads. Spawn argument vectors, never interpolate shell commands.
- Every commit MUST include a new development note in `docs/development/` describing intent, changes, verification and limitations. Use `Dev-Note: docs/development/<file>.md` in the commit message. Run `python3 scripts/check-dev-note.py --staged` before committing.
- Validate changes with `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `python3 scripts/smoke-console.py` after building. Also run `python3 scripts/test-api-contract.py`. Real small-model tests use `scripts/test-small-model.py` with explicit binaries and a <200 MiB GGUF. GPU claims require target hardware evidence.
