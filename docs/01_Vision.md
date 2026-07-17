# Vision

---

## 0. How to Read the v1 Specification

The remaining numbered documents form the normative specification:

- this document defines product scope, principles, and non-goals;
- [02_Architecture.md](02_Architecture.md) owns data identity, persistence, indexing, concurrency, and failure isolation;

<!-- 03_Implementation.md, 04_Product_Spec.md, and 05_Visual_Design.md are being rewritten. -->

`Must` and `must not` are requirements. `Should` is a preferred choice that may be changed only with a documented reason. `May` is optional. A v1 feature is complete only when its relevant Product acceptance criteria pass and its Architecture constraints are preserved. Existing code is not evidence that a contradictory behavior is acceptable; update the code or update the owning specification deliberately.

## 1. Product Overview and Goals

Vesper is a personal media gallery application for Linux. It allows a user to point the application at one or more local directories on their filesystem, and immediately browse all images and videos within those directories as one unified, visually rich library.

The application surfaces media. It does not manage, edit, transcode, upload, sync, or organize files. It is a viewer and a browser.

**Goals:**

- Make large personal media collections browsable with minimal friction.
- Make finding a specific file or group of files fast.
- Make consuming media (viewing images, watching videos) feel native and seamless.
- Keep input, scrolling, and already-indexed media responsive while discovery, probing, and thumbnail work runs in the background.
- Feel like a premium, polished desktop application — not a file manager with thumbnails.
- Keep the library read-only. Vesper observes the filesystem; it never becomes the source of truth for the user's files.

---

## 2. Core Philosophy and Non-Goals

**Philosophy:**

- Media is the product. The UI is the frame. The frame must never compete with the media.
- Folder structure is the only organizational system. The user's existing directory layout IS the tagging system.
- Tags represent folder lineage, not just folder names. Internally, tag identity is path-qualified so common names like `2023`, `Photos`, or `Misc` do not merge across unrelated folders.
- Simplicity over features. Every interaction must be learnable without documentation.
- The application never crashes on bad media. Unsupported and ignored files are skipped silently; unreadable supported files are reported through an aggregate passive indicator; thumbnail failures use placeholders.
- The application never blocks browsing with indexing, progress, or file-level error dialogs. Settings, folder chooser, shortcut help, and unrecoverable application errors are allowed dialog exceptions.
- The durable browsing context defined in Architecture is preserved across sessions. Transient state such as search text, selection, and an open viewer intentionally resets.

**Non-Goals (explicitly out of scope for v1):**

- File editing, cropping, rotating, or any destructive operations.
- User-defined manual tags (tags come only from folder structure).
- Cloud sync or remote storage.
- Facial recognition or AI-based content tagging.
- Content duplicate detection.
- Exporting, sharing, or uploading media.
- Multiple library support.
- Plugin or extension system.
- Mobile or cross-platform support.
- Printing.
- Rating or starring system.
- EXIF-based smart albums.

---

## 3. Target User and Usage Model

**Target user:** A single person on a Linux desktop (GNOME, Wayland) with a personal media collection stored across one or more local directories. They have organized their media into folders, and those folders reflect meaningful categories (trips, years, projects, people, events).

**Usage model:**

- The user opens the application.
- The application restores the last session context.
- The user browses, filters, searches, and views media.
- The user closes the application.

There is one user. There is one library. That library may contain multiple source roots. There is no library switching, no separate library profile, no accounts, no login, and no sharing.

The application runs on a single machine. All data is local.

---

## 4. Explicitly Accepted Constraints

These are known limitations that are accepted as part of the v1 product definition.

- **No EXIF data displayed or indexed.** EXIF is never the source for visible dates, filtering, sorting, or smart albums.
- **GIF files show first frame only.** No animation in grid or viewer.
- **No playback of audio-only files.** Audio files are ignored entirely.
- **File identity is path-based at the product level.** Moving or renaming a file outside the application is treated as removal plus addition. The implementation must use canonical physical identity only to prevent duplicate indexing paths caused by overlapping roots, duplicate canonical roots, or supported file symlink paths.
- **Content duplicate detection is out of scope.** Vesper does not scan for visually identical, byte-identical, or semantically duplicate media. Required physical duplicate prevention is limited to filesystem path/root/symlink safety.
- **Overlapping source roots are rejected.** A root cannot be added if it is already covered by an existing source root, contains an existing source root, or resolves to the same canonical location as an existing root.
- **Directory symlinks are not followed in v1.** File symlinks may be indexed only when they resolve to supported media inside an allowed source-root boundary and do not create duplicate library entries.
- **Source-root disappearance is treated as offline, not deletion.** If an entire root becomes unavailable, its media is hidden from the grid, search, selection, viewer navigation, and tag counts, but its records are preserved for when the root returns.
- **Thumbnails are not regenerated automatically for modified existing files.** New files receive thumbnails automatically. Deleted files are removed when the source root is online. Modified files update metadata automatically and may be marked thumbnail-stale, but the old thumbnail remains visible until explicit regeneration succeeds.
- **No HEIC support guaranteed.** HEIC indexing is attempted; if the system decoder is unavailable, HEIC files are skipped silently.
- **No RAW format support.** RAW image files (CR2, NEF, ARW, etc.) are ignored.
- **Displayed tag names reflect folder names exactly.** Unicode folder names produce Unicode tags. Folder names with special characters are displayed as-is. When two tags share the same display name, the UI must disambiguate them with folder lineage.
- **Tag counts are required.** Sidebar ordering depends on file counts, so counts are part of v1 rather than a future enhancement.
- **Dates come from reliable filesystem/application metadata.** Modified time comes from the filesystem. v1 uses `Date added to library` for the second date sort/display field; it must not expose filesystem birth time as a guaranteed `Date created` field on Linux.
- **The application is single-user and single-instance.** Vesper must prevent two write-capable instances from using the same library state at the same time. A second launch should focus the existing window when possible or exit with a clear non-blocking message.
- **Window position is not restored on Wayland.** The compositor controls window placement.
- **Theme follows the system preference.** If no system dark/light preference is available, Vesper defaults to dark.
- **Native Linux packaging is the v1 baseline.** Flatpak support is optional future work unless explicitly added with portal-aware source-directory access.
- **The responsiveness budgets are product requirements.** Filesystem walking, liveness checks, database work, media decoding/probing, cache maintenance, and clipboard preparation must not run on the GTK thread. Background work must be bounded and yield to interactive queries.

---

## 5. Explicitly Rejected Features

The following features will not be built, debated, or reconsidered for v1.

- Manual (user-defined) tags
- File deletion, renaming, or moving from within the app
- Image editing of any kind
- Rating or starring
- Content duplicate detection
- Face or object recognition
- Cloud sync or backup
- Sharing or exporting
- Slideshow mode
- Print support
- Multiple libraries or library switching
- Password protection or encryption
- Plugin system
- Batch operations beyond copy-path and open-location
- Import workflows (the filesystem is the import)
- EXIF browsing or filtering
- Map view or GPS-based browsing
- Calendar or timeline view
- Undo/redo
- Per-directory ignore files with syntax more complex than gitignore patterns
- Visible offline media cells in the grid
- Directory symlink traversal

---

## 6. Optional Future / Taste Tradeoffs

These are valid improvements, but not mandatory for v1 correctness. Implement only when the effort/benefit tradeoff is worth it and without changing the v1 navigation model.

- Result count in header, e.g. `47 of 1,284 items`.
- Empty state copy refinements.
- Placeholder illustration and loading-copy refinements that do not change the required loading/failure/offline states.
- Sort label wording refinements.
- Viewer filename and position overlay.
- Additional viewer transition polish beyond the required loading and error states.
- Grouped info panel metadata layout.
- Comfortable/compact grid-density presets that preserve the five thumbnail sizes.

**Still out of scope for v1:**

- Recent/folders sidebar sections that restructure the navigation model.
- Saved filter presets.
- Any destructive file operation.
- Any modal progress flow.

---

## 7. Final Product Summary

Vesper is a fast, beautiful, keyboard-friendly media gallery for Linux that treats your existing folder structure as its organizational system.

You add directories. It indexes them. Your folder names become tags. You filter by those tags. You find your media. You view it.

It does not try to replace your filesystem. It does not try to be Lightroom. It does not ask you to import, organize, rate, or manage anything.

It does one thing: it makes browsing a large personal media collection on Linux feel as good as it should.

The application follows the system theme with a dark fallback, stays media-first in its visual design, and preserves session state. It opens where you left it. It never blocks browsing with progress or file-level error dialogs. It never crashes on bad files. It never fights your folder structure.

The grid is the product. The viewer is the payoff. The tags are the map.

---

_This document fixes v1 scope. The other numbered documents provide the normative design and acceptance details. A feature absent from all five numbered documents is not part of v1._
