# FUTURE_IDEAS.md

# Vesper: Future Ideas and Research

This document outlines researched future enhancements for Vesper, ranked by priority. These are strictly post-v1 considerations and must not interfere with the v1 locked product contract defined in `PRODUCT_CONTRACT.md` and `UI_UX.md`.

---

## 1. SQLite + FTS5
**Category:** Performance / Infrastructure
**Effort:** High
**Dependency:** None (replaces in-memory indexing)
**What it adds:** Low-latency search, robust tag querying, and sorting without keeping 50k+ file metadata entries in RAM. Allows the app to scale gracefully.
**How it integrates:** `rusqlite` crate with the `bundled` feature to ensure FTS5 availability across distributions. Use trigram tokenizer (`tokenize="trigram"`) for fast sub-string and filename matching.
**Risks or unknowns:** Managing database concurrency between the background indexing thread (writer) and the UI thread (reader). SQLite's WAL (Write-Ahead Logging) mode will be required to prevent UI blocking.
**v1 contract conflict:** None. Supports the Section 22 latency target of 150ms search latency at 50k files.

## 2. GStreamer via gstreamer-rs
**Category:** UI / Performance
**Effort:** High
**Dependency:** None
**What it adds:** Reliable video playback, hardware decoding via DMABuf on GTK 4.14+, and seek-to-frame extraction for accurate video thumbnails.
**How it integrates:** `gstreamer`, `gstreamer-video`, and `gstreamer-play` crates. Research into `gst-plugin-gtk4` indicates the use of `gtk4paintablesink` which directly provides a `gdk::Paintable` sink for the GTK widget tree, bypassing old `gtk::Video` limitations.
**Risks or unknowns:** Packaging GStreamer in Flatpak requires the GStreamer SDK extension. Managing GStreamer pipeline states async can be complex compared to the simple native GTK video player.
**v1 contract conflict:** None. Greatly improves Section 13 (Video Behavior).

## 3. TOML config persistence (Infrastructure Needed Now)
**Category:** Infrastructure
**Effort:** Low
**Dependency:** None
**What it adds:** Persists user session state (filters, sort order, zoom level, window size) across launches seamlessly.
**How it integrates:** `serde`, `serde_derive`, and the `toml` crate. Use `glib::user_config_dir()` to store the file at `~/.config/vesper/config.toml`.
**Risks or unknowns:** Schema evolution. Must gracefully handle missing fields or corrupted TOML files by falling back to defaults silently.
**v1 contract conflict:** None. This is explicitly required by Section 18 (Session Persistence Behavior) and is active v1 work rather than a future idea.

## 4. ASHPD (XDG Desktop Portal)
**Category:** Distribution / Extensibility
**Effort:** Medium
**Dependency:** Flatpak sandbox
**What it adds:** Allows opening directories safely from within a Flatpak sandbox. Provides the "Add Source Directory" picker and "Open containing folder" action.
**How it integrates:** The `ashpd` crate. Uses `org.freedesktop.portal.FileChooser` for folder selection and `org.freedesktop.portal.OpenURI` to show items in the native file manager. Requires passing the Wayland window handle via `ashpd::WindowIdentifier::from_native`.
**Risks or unknowns:** Requires an async runtime (like `tokio`) bridging with the GTK UI thread. GTK4 Wayland window handles are strict and require correct mapping.
**v1 contract conflict:** Fulfills the "Open file location" requirement in Section 15 safely for sandboxes.

## 5. Flatpak manifest
**Category:** Distribution
**Effort:** Medium
**Dependency:** ASHPD (for portal access)
**What it adds:** Standardized, sandboxed distribution for Linux desktops. Easy installation via Flathub.
**How it integrates:** A `json` or `yaml` Flatpak manifest using the `org.gnome.Sdk` runtime (Rust extension required for `flatpak-builder`). Will need `--filesystem=home:ro` for raw file access, or full portal integration.
**Risks or unknowns:** Flathub submission requirements are strict. Managing the `cargo` offline build sources using `flatpak-cargo-generator.py` can be tedious to update.
**v1 contract conflict:** None.

## 6. inotify / filesystem watch
**Category:** Performance / UI
**Effort:** Medium
**Dependency:** Async indexing architecture
**What it adds:** Live updates to the media grid when files are added, removed, or when `.galleryignore` changes, without requiring a manual rescan.
**How it integrates:** The `notify` crate, specifically `notify::RecommendedWatcher`. File system events must be debounced (e.g., waiting 500ms after the last event) before triggering a DB update and UI refresh.
**Risks or unknowns:** Exhausting the system's `fs.inotify.max_user_watches` limit (default often 65k) if the user adds a massive source root. Must degrade gracefully to manual rescans if limit is hit.
**v1 contract conflict:** Fulfills Section 4 requirement for watching source roots.

## 7. HEIC decode
**Category:** Extensibility
**Effort:** High
**Dependency:** None
**What it adds:** Support for viewing modern iPhone photos natively.
**How it integrates:** `libheif-sys` or the `heif` crate via FFI bindings to system `libheif`.
**Risks or unknowns:** Patent encumbrances mean `libheif` in many Linux distributions (or Flatpak runtimes) is compiled *without* HEVC support. Fallback behavior is highly likely to trigger.
**v1 contract conflict:** Touches Section 24 (Accepted Constraints) which notes HEIC support is attempted but not guaranteed.

## 8. Thumbnail cache invalidation
**Category:** Performance
**Effort:** Medium
**Dependency:** SQLite DB
**What it adds:** Automatically regenerates thumbnails if the underlying source image is modified by an external editor.
**How it integrates:** Store the file `mtime` (modified time) in the SQLite database alongside the thumbnail path. On rescan, compare current `mtime` to the database; if newer, regenerate.
**Risks or unknowns:** Hashing files is too slow for 50k images, making `mtime` the only viable trigger. However, `mtime` relies on accurate filesystem timestamps.
**v1 contract conflict:** Lifts the constraint in Section 24 which explicitly states thumbnails are *not* auto-regenerated in v1.

## 9. DBus single-instance enforcement (Already Implemented)
**Category:** Infrastructure
**Effort:** None
**Dependency:** None
**What it adds:** Prevents multiple Vesper instances from running simultaneously against the same library, preventing DB corruption and race conditions.
**How it integrates:** Vesper already implements this natively in `src/main.rs` by calling `.application_id("io.github.TheRealShek.vesper")` on the `adw::Application` builder. GTK/GIO handles the single-instance DBus name acquisition automatically.
**Risks or unknowns:** None.
**v1 contract conflict:** Resolves the undefined behavior originally noted in Section 24.

## 10. AppStream / GNOME Software metadata
**Category:** Distribution
**Effort:** Low
**Dependency:** Flatpak manifest
**What it adds:** Visibility, screenshots, and descriptions in GNOME Software and Flathub.
**How it integrates:** Write an AppStream XML file (`*.metainfo.xml`) including `<id>`, `<name>`, `<summary>`, `<screenshots>`, and `<releases>`. Validated via the `appstreamcli` tool.
**Risks or unknowns:** Flathub reviewers will reject submissions if the screenshot dimensions, padding, or content rating tags (`OARS`) are incorrect.
**v1 contract conflict:** None.

## 11. Hardware-accelerated image decode
**Category:** Performance
**Effort:** Medium
**Dependency:** None
**What it adds:** Secure, sandboxed, and fast image decoding (JPEG, PNG, WEBP, TIFF, BMP) without blocking the main UI thread.
**How it integrates:** The `glycin` crate, which is the GNOME-blessed path (used by Loupe). It spawns isolated background processes for decoding and returns GTK-compatible textures natively.
**Risks or unknowns:** `glycin` is heavily tied to the modern GNOME stack. Process spawning overhead might be slow for rendering thousands of tiny grid thumbnails compared to `image-rs`, but is perfect for the full-resolution Viewer overlay.
**v1 contract conflict:** None.

## 12. EXIF metadata read (post-v1)
**Category:** Extensibility
**Effort:** Medium
**Dependency:** SQLite DB
**What it adds:** Extracts accurate Date Taken, Camera Model, and Resolution directly from the media file rather than relying on filesystem timestamps.
**How it integrates:** `kamadak-exif` (a pure Rust EXIF parser). Data will be displayed in the Info panel (`I` key).
**Risks or unknowns:** EXIF parsing adds significant I/O latency during the initial directory index. 
**v1 contract conflict:** Directly conflicts with Section 24, which states file dates come from filesystem metadata *only* in v1. This is strictly a post-v1 feature.

## 13. Tantivy full-text search (post-v1 alternative)
**Category:** Performance
**Effort:** High
**Dependency:** FTS5 failure to scale
**What it adds:** Typo-tolerance, BM25 relevance ranking, and highly advanced full-text indexing.
**How it integrates:** The `tantivy` crate (Rust's Lucene alternative).
**Risks or unknowns:** Creates separate index files distinct from the SQLite database, leading to potential eventual consistency issues. Almost certainly overkill for simple filename/tag prefix matching.
**v1 contract conflict:** None, but should only be explored if SQLite FTS5 fails to meet the 150ms latency target.

## 14. GNOME Shortcut Window
**Category:** UI
**Effort:** Low
**Dependency:** None
**What it adds:** In-app discoverability for keyboard shortcuts (`Escape`, `Ctrl+A`, `F`, `I`, etc.).
**How it integrates:** `gtk::ShortcutsWindow`, `gtk::ShortcutsSection`, and `gtk::ShortcutsGroup` accessible via a `Ctrl+?` shortcut or the `⋮` menu.
**Risks or unknowns:** Tedious to maintain in code as shortcuts evolve.
**v1 contract conflict:** Resolves the optional future idea mentioned in `UI_UX.md` Section 13.

## 15. Result count in header (post-v1 UI)
**Category:** UI
**Effort:** Low
**Dependency:** SQLite `COUNT(*)` query optimization
**What it adds:** Gives the user immediate context on the size of their filtered result set (e.g., `"47 of 1,284 items"`).
**How it integrates:** A `gtk::Label` placed in the `adw::HeaderBar` (likely centered or next to the filter pill).
**Risks or unknowns:** Running a `COUNT(*)` query on 50,000 rows on every keystroke during search could stutter the UI thread. Needs to be async or debounced.
**v1 contract conflict:** Resolves the optional future idea mentioned in `UI_UX.md` Section 13.

---
*End of Future Ideas Document.*
