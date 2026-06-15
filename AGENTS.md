# AGENTS.md

**Project:** Keyboard-first local media gallery for Linux. Indexes images/videos; tags from folder structure only. Read-only. Single-user, single-instance.
**Stack:** Rust · GTK4 · libadwaita
**Spec:** `docs/PRODUCT_CONTRACT.md` — read before implementing any feature.

## Gotchas

- After code changes, always perform the following:
  - Run `cargo fmt && cargo clippy -- -D warnings && cargo test --offline 2>&1`.
  - Fix any errors introduced by the current change before responding.
  - If the repository is already warning-free, also run `cargo clippy -- -D warnings` before finishing.
  - Do not run `cargo clippy --fix` automatically.
  - Do not chase unrelated pre-existing warnings unless the task specifically asks for cleanup.
- Always clean up any temporary scratchpad or test files (e.g., `check_gtk.rs`) from the codebase when you are done with them.
- GTK CSS/`.card`: Always provide margins. Touching container bounds will clip `border-radius` and `box-shadow`.

## Rules

**Code quality**

- Always write WHY-focused inline comments for any non-obvious architecture or design choices (do not explain WHAT the code does).

- No `unwrap()`/`expect()` outside tests. Use `thiserror` at module boundaries (`index/`, `db/`); `anyhow` for application-level propagation.
- No redundant abstractions. If two paths do the same thing, unify them.
- Handle all error variants explicitly. No silent discard.

**Architecture**

- `src/ui/` has zero knowledge of filesystem or DB internals.
- `src/index/` and `src/db/` have zero GTK imports.
- Cross-boundary communication via typed events/channels only — no shared mutable state.
- Application-wide constants and default settings go in `src/config.rs`.

**GTK / performance**

- UI thread must never block. All I/O, DB queries, thumbnail generation are async or offloaded.
- Grid is virtualized. Only visible cells render. Target: 50,000 files without stutter.

**Filesystem**

- Read-only. Never write, rename, move, or delete files.
- Respect `.galleryignore` (gitignore syntax). Matched directories not descended into.
