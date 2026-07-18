# AGENTS.md

## Project

Vesper is a read-only, single-user, single-instance personal media gallery for
Linux/GNOME/Wayland. Tags come only from folder structure.

**Stack:** Rust, GTK4, libadwaita, SQLite, GTK/GStreamer, `ffmpeg`, and `ffprobe`.

## Required Workflow

1. Before implementing any feature, read the task-specific canonical documents
   in the map below.
2. Inspect the relevant existing modules before editing. Preserve their boundaries
   and reuse established patterns.
3. After changing any `.rs` file or Rust build configuration, run this exact
   command — all three stages, in this order:

   ```bash
   cargo fmt && cargo clippy -- -D warnings && cargo test
   ```

   This is mandatory and non-negotiable. It is NOT satisfied by any substitute:
   `cargo build`, `cargo check`, or `cargo test` alone do NOT count — `fmt` and
   `clippy -- -D warnings` must both run. A per-task instruction that names a
   lesser check (e.g. "run cargo build && cargo test") is a floor, never a
   replacement: run this full command regardless. Do not report work as done,
   validated, or passing — and do not paste any pass/fail summary — until this
   exact command has actually run and every stage has succeeded. If you skipped
   it, say so plainly rather than implying it ran.

4. Fix every issue introduced by your changes. Do not use `cargo clippy --fix` or
   broaden the task to unrelated pre-existing issues; report any such issue that
   blocks validation. Remove temporary files before finishing.

Documentation-only changes do not require Rust validation.

## Where to Look

| Task | Canonical documentation | Primary code |
| --- | --- | --- |
| Product scope or constraints | `docs/01_Vision.md` | — |
| Storage, source roots, indexing, tags, ignore rules, filesystem behavior | `docs/02_Architecture.md` | `src/db/`, `src/index/`, `src/scan.rs` |
| Backend loop, watching, live updates | `docs/02_Architecture.md` | `src/backend/`, `src/events.rs` |
| Thumbnails or media metadata | `docs/02_Architecture.md` | `src/thumbnail.rs`, `src/index/media.rs` |
| App-wide defaults or constants | Relevant canonical document | `src/config.rs` |

Document ownership: Vision defines scope; Architecture defines system and data
models. If canonical documents conflict, report the conflict instead of inventing
behavior.

## Architectural Boundaries

- `src/db/` and `src/index/` must not import GTK.
- Frontend, backend, index, and database boundaries communicate through typed events in
  `src/events.rs`; do not add shared mutable state across them.
- All filesystem I/O, database work, and thumbnail generation must be asynchronous
  or offloaded. Never block the UI thread.
- Keep the grid virtualized so only visible cells render; target 50,000 files without
  stutter.
- Keep `src/main.rs` thin. Put app-wide constants in `src/config.rs`.

## Product Invariants

- The media filesystem is read-only: never write, move, rename, or delete user media.
- `.galleryignore` uses the architecture-defined gitignore-like, last-match-wins
  behavior. Do not descend into ignored directories.
- Source roots may not duplicate, contain, or nest within one another after path
  canonicalization.
- Do not follow directory symlinks in v1. File symlinks must remain within the source
  policy and must not produce duplicate records.
- Offline roots remain listed, but their media is excluded from the grid, search,
  selection, viewer navigation, and tag counts.
- Media identity is path-based. Canonical physical identity prevents indexing the
  same path target twice; it is not content-duplicate detection.
- Tags are keyed by `source_root_id + relative_folder_path`; disambiguate duplicate
  display labels.
- Use `Date added` when birth time is unreliable; do not promise `Date created` on
  Linux.
- Preserve an old thumbnail for a modified file until asynchronous regeneration
  succeeds; it may be marked stale.

## Rust Quality Rules

- Write idiomatic, maintainable Rust—not code that merely compiles. Prefer standard
  library and established project patterns over cleverness or speculative helpers.
- Do not use `unsafe` unless no safe design can meet the requirement. Keep unavoidable
  `unsafe` blocks minimal, document each with a `// SAFETY:` invariant, encapsulate
  them behind a safe API, and add focused tests. Never use `unsafe` to bypass borrow,
  lifetime, thread-safety, or FFI correctness problems.
- No `unwrap()` or `expect()` outside tests. Handle every error explicitly; never
  silently discard one. Use `thiserror` at module boundaries and `anyhow` at the app
  boundary. A production `unwrap()` is permitted only when it is genuinely required
  by a proven invariant that cannot be represented through normal error handling;
  document that invariant immediately at the call site and add a focused test. Treat
  this as an exceptional, review-required case—not a convenience shortcut.
- Avoid needless cloning, allocation, collection, locking, and `pub` visibility.
  Do not introduce blocking work into async or UI paths.
- Avoid duplicate paths, premature abstraction, unnecessary traits/generics, wrapper
  types with no invariant, and broad refactors unrelated to the task.
- Comments explain why and document invariants; do not narrate obvious code.

## Platform Guard Rails

- Native Debian-style Linux packaging is the v1 baseline; do not claim Flatpak support
  before portal-based root persistence is implemented and tested.
- Video thumbnails use `ffmpeg`; duration uses `ffprobe`. Missing tools/codecs are non-fatal.

## Optional Visual GTK App Inspection (GNOME/Wayland Dev Host)

Only use this workflow when the user explicitly requests visual/UI verification, or
after first asking and receiving their permission. It starts and drives the local app
and may read its local SQLite state; it is not part of routine Rust validation.

- Run the debug app as an Xwayland client so it can be found and captured:
  ```bash
  GDK_BACKEND=x11 ./target/debug/vesper >/tmp/vesper_run.log 2>&1 &
  ```
- Locate, resize, or maximize its X11 window with `wmctrl`; capture it with ImageMagick:
  ```bash
  WID=$(wmctrl -l | grep -i vesper | head -1 | awk '{print $1}')
  wmctrl -i -r "$WID" -b add,maximized_vert,maximized_horz
  import -window "$WID" /tmp/vesper-shot.png
  ```
  On multi-monitor setups, move the window onto the primary monitor before attempting
  mouse interaction: `wmctrl -i -r "$WID" -e 0,60,60,1500,900`.
- Mouse injection may be unreliable under this setup. Keyboard injection works through
  `ydotool` once `ydotoold` is running with an explicit socket:
  ```bash
  ydotoold -p /tmp/ydsock & export YDOTOOL_SOCKET=/tmp/ydsock
  ydotool key 53:1 53:0
  ydotool type "query"
  ```
- Do not attempt to drive **Add Source Root** through the UI: `GtkFileDialog` uses a
  Wayland portal window that the X11 capture/injection tools cannot access.
- `~/.local/share/vesper/vesper.db` is a live SQLite database. It may be inspected to
  understand UI state; alter it only with separate explicit permission. Never move or
  replace the database while the app may be running, as this can lose WAL-backed state.
- Temporary `VESPER_TEST_VIEWER`, `VESPER_TEST_INFO`, and `VESPER_TEST_SELECT` hooks,
  if present in `src/ui/window.rs`, are test scaffolding only. Do not add them for
  ordinary checks, and remove any such scaffolding before delivery.
