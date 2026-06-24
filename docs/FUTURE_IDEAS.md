# FUTURE_IDEAS.md

# Vesper: Future Ideas and Research

This document outlines future enhancements for Vesper, structured by priority and category. These post-v1 considerations are designed to extend the application's capabilities without violating the core product philosophy of a read-only, folder-derived media gallery.

---

## 1. UI/UX Polish (v1 gaps)

- **Result counts in header** — displays the active count of filtered items relative to the total library size (e.g., "24 of 10,412 files") in the header bar, fitting Vesper's philosophy of keeping the user informed of search specificity without screen clutter.
- **Tag counts in sidebar** — shows a file count next to each tag in the sidebar list, fitting Vesper's philosophy of treating folder structures as the definitive guide to collection density.
- **Video play indicator on grid cells** — overlays a play icon on video cells in the grid, fitting Vesper's philosophy of immediate visual clarity and media-first hierarchy.
- **Viewer filename and progress overlay** — displays the current file name and index position (e.g., "3 of 42") in the viewer, fitting Vesper's philosophy of providing spatial awareness during navigation.
- **Viewer loading and error states** — renders loading indicators for large files and clean error messages for corrupted media, fitting Vesper's philosophy of graceful, silent degradation under bad file conditions.
- **Grouped info panel metadata layout** — groups properties in the information sidebar visually, fitting Vesper's philosophy of presenting file metadata cleanly without competing with the media itself.

---

## 2. Performance & Scale

- **Smarter thumbnail caching** — uses file modification times (mtime) in the database to detect modified media and regenerate thumbnails only as needed, fitting Vesper's philosophy of maintaining a high-performance local footprint.
- **Scan parallelism** — walks the directory tree and processes metadata using concurrent threads, fitting Vesper's philosophy of instant-read capability on startup for large libraries.
- **Virtual scroll buffer tuning** — dynamically adjusts off-screen thumbnail preloading based on scroll velocity, fitting Vesper's philosophy of maintaining a stutter-free grid at scale.
- **SQLite FTS5 trigram indexing** — implements a trigram-based search index for substring matching, fitting Vesper's philosophy of maintaining sub-150ms search latency across 50k+ files.

---

## 3. Viewer Enhancements

- **Zoom memory per file** — retains the zoom level and panning coordinates when navigating between media files in the viewer, fitting Vesper's philosophy of seamless comparison of high-resolution details.
- **Keyboard scrubbing for video** — enables navigation of videos using configurable keyboard skip steps or percentage keys, fitting Vesper's philosophy of keyboard-first navigation and mouse-free operation.
- **Color profile awareness** — reads and respects embedded ICC color profiles during rendering, fitting Vesper's philosophy of displaying media in its true, native fidelity.

---

## 4. Filtering & Discovery

- **Saved filter presets** — lets users save combinations of active tags and searches as persistent shortcuts, fitting Vesper's philosophy of finding specific groups of files quickly.
- **Negation filters** — allows users to exclude matching folders/tags from the view, fitting Vesper's philosophy of powerful discovery tools strictly within folder-derived tags.
- **Untagged media filter** — provides a filter to show only files residing directly in the source roots, fitting Vesper's philosophy of finding files based on filesystem location.

---

## 5. Sidebar Depth

- **Tag grouping by source root** — organizes the tag list under headers corresponding to each added source directory, fitting Vesper's philosophy of mapping the UI directly to disk structure.
- **Collapsible tag groups** — allows sections of tags to be collapsed or expanded, fitting Vesper's philosophy of keeping the fixed-width sidebar compact and efficient.
- **Sidebar tag search highlighting** — highlights matching characters as the user filters the sidebar tags, fitting Vesper's philosophy of rapid tag navigation in large collections.

---

## 6. Keyboard & Accessibility

- **Keyboard-only multi-selection** — allows selecting multiple cells and performing batch copy actions using only keyboard modifiers, fitting Vesper's philosophy of full keyboard-first power-user capabilities.
- **Screen reader labels for grid cells** — exposes filenames and derived tag hierarchies to assistive screen readers, fitting Vesper's philosophy of native integration with GNOME accessibility standards.
- **Interactive shortcut cheat sheet** — provides a searchable, categorized overlay of all shortcut keys, fitting Vesper's philosophy of self-documenting, learnable controls.
- **High-contrast focus outlines** — renders a highly visible outline around the focused cell, fitting Vesper's philosophy of accessible, trackable keyboard navigation.

---

## 7. Explicitly Deferred (Post-v2 Territory)

- **Manual tag entry** — Deferred because it violates the philosophy that folder structure is the only organizational system, and requires complex write/sync schemas.
- **EXIF metadata browsing and filtering** — Deferred because reading and parsing EXIF headers for all media adds significant I/O latency, conflicting with the goal of fast, lightweight scanning.
- **Multiple library support** — Deferred because Vesper is designed as a single-library, single-user desktop gallery, and library switching adds unnecessary interface complexity.
- **Cloud synchronization and remote storage** — Deferred because Vesper is strictly a local-first application, and syncing introduces account management, privacy concerns, and network dependencies.
- **Destructive file operations (delete, rename, move)** — Deferred because Vesper is designed as a read-only viewer to guarantee files on disk are never corrupted or accidentally deleted.
- **AI-based content tagging and facial recognition** — Deferred because local AI inference requires heavy external dependencies and conflicts with the simple, deterministic folder-based tag model.
- **Map view and GPS-based browsing** — Deferred because it requires EXIF coordinate parsing, internet access for map tiles, and complex geographical grouping widgets.
- **Calendar or timeline views** — Deferred because it depends on parsing EXIF creation timestamps rather than fast filesystem metadata, and requires layout models that deviate from the unified grid.
- **Plugin or extension system** — Deferred because keeping the codebase unified and minimal prevents security vulnerabilities and ensures long-term maintenance stability.
- **Slideshow mode** — Deferred because it is explicitly rejected in PRODUCT_CONTRACT.md section 25.
- **Recent tags** — Deferred because UI_UX.md section 14 explicitly forbids adding new sidebar sections.
