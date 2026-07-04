# AGENTS.md

**Project:** Read-only personal media gallery. Linux/GNOME/Wayland. Tags from folder structure only. Single-user, single-instance.
**Stack:** Rust · GTK4 · libadwaita
**Spec:** Read `docs/04_Product_Spec.md` before implementing any feature. For behavior touching storage, indexing, thumbnails, source roots, tags, or UI structure, also read the matching canonical doc listed below.

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
    mod.rs, window.rs, sidebar.rs, header.rs, grid_cell.rs
    viewer.rs, settings.rs, filter_sort.rs, filter_controller.rs
    selection_bar.rs, shortcuts.rs, model.rs, style.css
docs/
  01_Vision.md          # product vision, philosophy, and constraints
  02_Architecture.md    # system architecture, widget tree, and logic models
  03_Implementation.md  # sidebar/header layout, CSS rules, developer guard rails
  04_Product_Spec.md    # interactive features, layout, keyboard shortcuts
```

## Documentation Ownership

- `01_Vision.md`: product goal, user model, scope, non-goals, accepted constraints.
- `02_Architecture.md`: source-root model, tag identity, storage/cache/index model, workers, filesystem rules, widget tree.
- `03_Implementation.md`: GTK layout details, CSS rules, packaging/backend assumptions, implementation guard rails.
- `04_Product_Spec.md`: user-visible behavior, flows, shortcuts, settings, error states, acceptance criteria.

## Architecture Rules

- `ui/` ↔ `index/`+`db/`: typed channel events only (`events.rs`). No shared mutable state.
- All I/O, DB queries, thumbnail gen: async or offloaded. UI thread never blocks.
- Grid virtualized. Only visible cells render. Target: 50k files, no stutter.
- Filesystem read-only. Never write, move, rename, delete.
- Respect `.galleryignore` using the architecture-defined gitignore-like last-match-wins rule. Matched dirs are not descended into.
- One library may contain multiple non-overlapping source roots. Reject duplicate, nested, or containing roots by canonical path.
- Directory symlinks are not followed in v1. File symlinks may be indexed only inside allowed source-root policy and without duplicate records.
- Offline roots remain visible in the source list, but their media is hidden from grid/search/selection/viewer navigation/tag counts.
- Tags are path-qualified internally: `source_root_id + relative_folder_path`. Display names may be short, but duplicate labels must be disambiguated.
- Product-level media identity is path-based. Canonical physical identity is only for preventing duplicate indexing paths from overlapping roots/supported file symlinks, not content duplicate detection.
- Modified existing files keep their old thumbnail visible and may be marked thumbnail-stale until explicit regeneration succeeds.
- Date sorting/info uses `Date added` where filesystem birth time is unreliable. Do not reintroduce `Date created` as a guaranteed Linux feature.

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
- **Viewer overlay:** mount at the top-level app overlay so it covers header, sidebar, and grid. Do not mount it inside the grid-only overlay.
- **Selection action bar:** remains grid-scoped. Opening the viewer clears selection; viewer mode and selection mode do not coexist in v1.
- **Status surfaces:** offline/indexing status use the status banner/row stack below the header. Fatal unrecoverable app errors use the Product-specified closing dialog, not a banner.
- **Settings restore defaults:** "Restore Default Ignore Rules" only updates the ignore-rules text field. Saving global ignore rules is what applies changes and triggers rescan.

## Platform and Media Backend

- Native Linux/Debian-style packaging is the v1 baseline. Do not claim Flatpak support until portal-based source-root persistence is implemented and tested.
- GTK media playback uses `gtk::MediaFile` / `gtk::MediaStream` through the platform GTK/GStreamer backend.
- Video thumbnails use external `ffmpeg`; duration probing uses external `ffprobe`.
- Missing `ffmpeg`, `ffprobe`, or codecs must not be fatal to app startup. Use placeholders/no duration badge or in-viewer playback errors as specified.

## Build Check (run after every change)

```bash
cargo fmt && cargo clippy -- -D warnings && cargo test
```

Fix errors from current change before responding. Don't chase pre-existing warnings unless task is cleanup. Don't run `cargo clippy --fix`. Remove temp/scratchpad files before finishing.
