AGENTS.md src tree is wrong — doesn't match reality. Fix it:```markdown

# AGENTS.md

**Project:** Read-only personal media gallery. Linux/GNOME/Wayland. Tags from folder structure only. Single-user, single-instance.
**Stack:** Rust · GTK4 · libadwaita
**Spec:** `docs/PRODUCT_CONTRACT.md` — read before implementing any feature.

## Source Layout

```
src/
  config.rs         # app-wide constants and defaults
  events.rs         # typed cross-boundary channel events
  state.rs          # session/app state
  scan.rs           # orchestrates indexing pipeline
  thumbnail.rs      # thumbnail generation — async, never blocks UI
  main.rs           # thin app entrypoint
  backend/          # async backend loop and file watching
    mod.rs, app_loop.rs, watcher.rs, live_update.rs
  db/               # SQLite — zero GTK imports
    mod.rs, models.rs, schema.rs, error.rs
    roots.rs, media.rs, tags.rs, search.rs
  index/            # filesystem logic — zero GTK imports
    mod.rs, walker.rs, media.rs, ignore_rules.rs, error.rs
  ui/               # GTK only — zero fs/db imports
    window.rs, sidebar.rs, header.rs, grid_cell.rs
    viewer.rs, settings.rs, filter_sort.rs, model.rs
    style.css, mod.rs
docs/
  PRODUCT_CONTRACT.md   # locked spec
  UI_UX.md              # locked UI spec
```

## Architecture Rules

- `ui/` ↔ `index/`+`db/`: typed channel events only (`events.rs`). No shared mutable state.
- All I/O, DB queries, thumbnail gen: async or offloaded. UI thread never blocks.
- Grid virtualized. Only visible cells render. Target: 50k files, no stutter.
- Filesystem read-only. Never write, move, rename, delete.
- Respect `.galleryignore` (gitignore syntax). Matched dirs not descended into.

## Code Rules

- No `unwrap()`/`expect()` outside tests. `thiserror` at module boundaries; `anyhow` for app-level.
- Comments explain WHY, not what.
- No redundant abstractions. Unify duplicate paths.
- All error variants handled explicitly. No silent discard.
- App-wide constants → `src/config.rs`.

## GTK Gotchas

- **`adw::ToolbarView` scope:** grid column only. Wrapping top-level box → header spans full width including sidebar.
- **Sidebar:** fixed width, always visible. No `GtkPaned`. No `Ctrl+B` toggle. No `adw::OverlaySplitView`.
- **CSS/`.card`:** always add margin. Without it, `border-radius` and `box-shadow` clip at container bounds.

## Build Check (run after every change)

```bash
cargo fmt && cargo clippy -- -D warnings && cargo test
```

Fix errors from current change before responding. Don't chase pre-existing warnings unless task is cleanup. Don't run `cargo clippy --fix`. Remove temp/scratchpad files before finishing.
