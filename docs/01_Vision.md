# Vision

---

## 1. Product Overview and Goals

Vesper is a personal media gallery application for Linux. It allows a user to point the application at one or more directories on their filesystem, and immediately browse all images and videos within those directories as a unified, visually rich grid.

The application surfaces media. It does not manage, edit, transcode, upload, sync, or organize files. It is a viewer and a browser.

**Goals:**

- Make large personal media collections browsable with minimal friction.
- Make finding a specific file or group of files fast.
- Make consuming media (viewing images, watching videos) feel native and seamless.
- Feel like a premium, polished desktop application — not a file manager with thumbnails.

---

## 2. Core Philosophy and Non-Goals

**Philosophy:**

- Media is the product. The UI is the frame. The frame must never compete with the media.
- Folder structure is the only organizational system. The user's existing directory layout IS the tagging system.
- Simplicity over features. Every interaction must be learnable without documentation.
- The application never crashes on bad data. It degrades gracefully and silently for file-level errors.
- State is preserved across sessions. The application picks up exactly where the user left it.

**Non-Goals (explicitly out of scope for v1):**

- File editing, cropping, rotating, or any destructive operations.
- User-defined manual tags (tags come only from folder structure).
- Cloud sync or remote storage.
- Facial recognition or AI-based content tagging.
- Duplicate detection.
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

There is one user. There is one library. There are no accounts, no login, no sharing.

The application runs on a single machine. All data is local.

---

## 4. Explicitly Accepted Constraints

These are known limitations that are accepted as part of the v1 product definition.

- **No EXIF data displayed or indexed.** File dates come from filesystem metadata only (created, modified timestamps).
- **GIF files show first frame only.** No animation in grid or viewer.
- **No playback of audio-only files.** Audio files are ignored entirely.
- **File identity is path-based.** Moving or renaming a file outside the application causes it to be re-indexed as a new file. Tag associations derived from folder structure are re-derived correctly on rescan.
- **Thumbnails are not regenerated automatically if source files change.** A manual rescan (triggered from Settings) regenerates thumbnails for modified files.
- **No HEIC support guaranteed.** HEIC indexing is attempted; if the system decoder is unavailable, HEIC files are skipped silently.
- **No RAW format support.** RAW image files (CR2, NEF, ARW, etc.) are ignored.
- **Tag names reflect folder names exactly.** Unicode folder names produce Unicode tags. Folder names with special characters are displayed as-is.
- **The application is single-user and single-instance.** Running two instances simultaneously against the same library produces undefined behavior.
- **Window position is not restored on Wayland.** The compositor controls window placement.

---

## 5. Explicitly Rejected Features

The following features will not be built, debated, or reconsidered for v1.

- Manual (user-defined) tags
- File deletion, renaming, or moving from within the app
- Image editing of any kind
- Rating or starring
- Duplicate detection
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

---

## 6. OPTIONAL FUTURE / TASTE TRADEOFFS

These are valid improvements, but not mandatory for v1 correctness. Implement only when the effort/benefit tradeoff is worth it and without changing the v1 navigation model.

- Result count in header, e.g. `47 of 1,284 items`.
- Tag counts in sidebar.
- Empty state copy refinements.
- Thumbnail loading/failure/offline state refinements.
- Video play indicator on grid cells.
- More explicit grid zoom behavior documentation.
- Sort label wording refinements.
- Viewer filename and position overlay.
- Viewer loading/error states.
- Grouped info panel metadata layout.

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

The application is dark by default, media-first in its visual design, and persistent in its session state. It opens where you left it. It never blocks you with dialogs. It never crashes on bad files. It never fights your folder structure.

The grid is the product. The viewer is the payoff. The tags are the map.

---

_This document describes the complete v1 product. Any feature not mentioned here is not part of v1._
