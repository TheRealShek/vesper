# AGENTS.md

**Project:** Keyboard-first local media gallery for Linux. Indexes images/videos; tags from folder structure only. Read-only. Single-user, single-instance.
**Stack:** Rust · GTK4 · libadwaita
**Spec:** `docs/PRODUCT_CONTRACT.md` — read before implementing any feature.

## Gotchas

_(Populated during implementation. Record any pattern the AI repeatedly gets wrong.)_

## Rules

**Code quality**

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
