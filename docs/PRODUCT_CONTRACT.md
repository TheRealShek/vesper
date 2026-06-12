# PRODUCT_CONTRACT.md

# Vesper: Personal Linux Media Gallery — Definitive Product Specification

**Status:** Locked
**Version:** 1.1
**Scope:** v1 product only. No stretch goals. No future ideas.

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

## 4. Source Directory Model

The user designates one or more directories on their filesystem as **source roots**. The application indexes all media files found recursively within those roots.

**Behavior:**

- Source roots are added and removed via the Settings panel.
- Any number of source roots can be active simultaneously.
- All media from all source roots appears in a single unified grid.
- Removing a source root removes its media from the library immediately. Files on disk are untouched.
- The application watches all source roots for changes while running. New files appear in the grid automatically. Deleted files disappear automatically. File system events are debounced before processing.
- Symbolic links within source roots are followed one level deep. Circular symlinks are ignored silently.
- If a source root directory is unavailable at launch (unmounted drive, deleted path), the application launches normally, shows available media, and displays a passive indicator that one or more source roots are offline. No blocking dialog.

**Supported media types:**

- Images: JPEG, PNG, GIF (static, first frame only), WEBP, TIFF, BMP, HEIC.
- Videos: MP4, MKV, AVI, MOV, WEBM, FLV, M4V.
- All other file types are silently ignored during indexing.

---

## 5. Ignore Rules

The application supports a pattern-based ignore system that prevents matching files and directories from being indexed. It works at two levels: global rules that apply across all source roots, and per-directory `.galleryignore` files that apply locally.

**Global ignore rules:**

- Managed in the Settings panel under "Ignore Rules."
- Displayed as an editable list of patterns, one per line.
- Apply to every source root without exception.
- Evaluated before any file or directory is indexed.

**Per-directory `.galleryignore` files:**

- A plain text file named `.galleryignore` placed inside any directory within a source root.
- Rules in a `.galleryignore` file apply to that directory and all of its descendants.
- Rules do not propagate upward.
- `.galleryignore` files are watched for changes while the application is running. Editing a `.galleryignore` file triggers a rescan of the affected directory automatically.
- `.galleryignore` files are never shown in the media grid.

**Pattern syntax:**

Patterns follow the same rules as `.gitignore`:

- `*.ext` — matches any file with that extension anywhere within scope.
- `foldername/` — matches a directory of that name (trailing slash denotes directory).
- `foldername` — matches any file or directory of that name.
- `**/pattern` — matches pattern at any depth within scope.
- `pattern/**` — matches everything inside a directory named pattern.
- A leading `!` negates a pattern — explicitly includes files that would otherwise be ignored.
- Lines beginning with `#` are comments and are ignored.
- Blank lines are ignored.

**Rule precedence:**

1. Per-directory `.galleryignore` rules are evaluated first, innermost directory first.
2. Global rules are evaluated after per-directory rules.
3. A negation rule (`!pattern`) at any level can un-ignore a file that a broader rule would have excluded.
4. The most specific matching rule wins.

**Behavior:**

- A directory matched by an ignore rule is not descended into. Its entire subtree is excluded.
- Ignored files and directories produce no entries in the library and no tags.
- Ignored files are not counted in tag file counts.
- Ignore rules take effect on the next rescan or filesystem watch event. Already-indexed files that become ignored are removed from the library on the next rescan.
- No indication is shown in the UI for ignored files. They simply do not exist from the application's perspective.

**Default global ignore patterns (pre-populated on first launch):**

```
.git/
node_modules/
.Trash/
.cache/
*.tmp
*.part
.DS_Store
Thumbs.db
```

The user can edit or remove any default pattern. Defaults are never restored automatically.

---

## 6. Tag Model and Tag Behavior

Tags are derived exclusively from the folder hierarchy of each source root. No manual tags exist in v1.

**Derivation rule:**

Every file receives one tag per ancestor folder between the source root and the file itself (inclusive, based on user preference). The tag name is the folder name exactly as it appears on disk.

**Example:**

```
Source root: /home/user/media

File: /home/user/media/Travel/Japan/2023/photo.jpg

Tags assigned: Travel, Japan, 2023
```

**Root inclusion toggle:**

A setting controls whether the source root directory name itself is included as a tag. Default: OFF. When OFF, only subdirectories below the root are used as tags.

**Tag properties:**

- Tags are case-sensitive and match the folder name exactly.
- Tags are re-derived on every rescan. They cannot be edited manually.
- A file with no subdirectory between it and the source root has no tags.
- Tags have a file count — the number of media files that carry that tag.
- Tags are sorted by file count, descending. The tag with the most files appears first.

**Tag inheritance:**

Selecting a parent tag includes all files that have that tag at any depth. Selecting "Travel" shows all files in `Travel/` and all subdirectories recursively.

---

## 6. Search Behavior

**One search box. No syntax. No prefixes.**

The search box is located in the top bar, always visible. Typing into it filters the current grid in real time.

**What is searched simultaneously:**

- Filename (without extension)
- Full file path
- All tags assigned to the file

**Ranking:**

Results are ranked by relevance. Exact filename matches appear first. Tag matches appear second. Path matches appear third. Within each rank tier, results maintain the current sort order.

**Behavior:**

- Search activates on keystroke. No need to press Enter.
- Search operates on the currently filtered set. If a tag filter is active, search further narrows within that filtered result.
- Clearing the search box returns the grid to the pre-search state instantly.
- Search and tag filters are independent dimensions. Both can be active simultaneously.
- The search box displays the current query at all times. It is never hidden.

---

## 8. Main Application Layout

The application has one persistent window divided into three zones:

```
┌──────────────────────────────────────────────┐
│                   TOP BAR                    │
├───────────┬──────────────────────────────────┤
│           │                                  │
│  SIDEBAR  │           GRID                   │
│           │                                  │
│           │                                  │
└───────────┴──────────────────────────────────┘
```

**Top bar:** Contains the application title, search box, sort controls, grid size slider, and settings button. Always visible. Never hidden.

**Sidebar:** Contains the tag list and source root indicators. Collapsible. Width is user-adjustable and persisted across sessions.

**Grid:** The main content area. Fills all remaining space. Scrolls vertically. Never paginates — it is one continuous scrollable surface.

There is no navigation history, no back button, no breadcrumb. The application is always showing one unified library with filters applied.

---

## 9. Sidebar Behavior

The sidebar contains the tag list.

**Tag list:**

- Tags are displayed as a flat list — not a tree, not a hierarchy.
- Tags are sorted by file count, descending. Most-populated tag appears first.
- Each tag entry shows: tag name, file count.
- The list is scrollable.
- After the 30th tag, a "Show more" control appears. Activating it expands the full list.
- A search/filter input at the top of the sidebar filters the tag list itself (not the media grid).

**Tag selection:**

- Clicking a tag activates it as a filter. The grid updates immediately.
- Multiple tags can be active simultaneously.
- By default, multiple active tags use OR logic — the grid shows files matching any active tag.
- A toggle in the sidebar switches to AND logic — the grid shows only files matching all active tags simultaneously.
- Active tags are visually distinguished (filled chip vs outlined chip).
- Clicking an active tag deactivates it.
- A "Clear all" control appears when any tag filter is active. Activating it deactivates all tags at once.

**Sidebar collapse:**

- The sidebar can be collapsed to an icon-only strip.
- Keyboard shortcut: `Ctrl+B` toggles sidebar visibility.
- Collapsed state is persisted across sessions.

---

## 10. Grid View Behavior

The grid displays all media matching the current filter and search state.

**Layout:**

- All cells are square.
- The number of columns adjusts to fill available width based on the current zoom level.
- The zoom level is controlled by a slider in the top bar. It has five steps: XS, S, M, L, XL.
- Default zoom level: M.
- The grid scrolls vertically. It does not paginate.
- The grid is virtualized — only visible cells are rendered. Scrolling through 100,000 files does not degrade performance.

**Empty state:**

- If no source roots are configured: a centered prompt asking the user to add a source directory. A single button opens the source root configuration.
- If filters or search produce no results: a centered message stating no media matches the current filters. A button clears all active filters.

**Loading state:**

- On first launch or after adding a new source root, thumbnails appear progressively as they are generated. Cells show a placeholder until their thumbnail is ready. The grid is usable immediately — the user does not wait for indexing to complete.

---

## 11. Grid Cell Specification

Every cell is square. The thumbnail fills the entire cell, center-cropped. Non-square media is never letterboxed.

**Three visual states:**

**Default (no interaction):**

- Thumbnail fills cell entirely.
- No text overlay. No badges except video duration.
- Video cells display a duration badge in the bottom-right corner. Format: `M:SS` or `H:MM:SS`. The badge is semi-transparent dark with white text.

**Hover:**

- A gradient overlay rises from the bottom of the cell.
- The filename appears in the overlay, single line, truncated with ellipsis if needed.
- A file type indicator (image or video icon) appears in the overlay.
- Video cells continue to show the duration badge.
- Hover state activates within 150ms of cursor entry. It disappears immediately on cursor exit.

**Selected:**

- An accent-colored border appears around the cell perimeter.
- A checkmark appears in the top-left corner on a small circular accent background.
- The thumbnail receives a subtle dark tint.
- Selected state is additive — multiple cells can be selected simultaneously.

**Thumbnail placeholder:**

- Cells without a generated thumbnail show a neutral dark background with a centered media-type icon.
- Placeholder is replaced by the actual thumbnail as soon as it is ready. No refresh required.

---

## 12. Image Behavior

**In the grid:**

- Static thumbnail, center-cropped to square.
- GIF files show the first frame only. They do not animate in the grid.

**In the viewer:**

- Full resolution image is loaded.
- Default display: fit to viewer area, maintaining aspect ratio.
- Scroll wheel: zoom in and out, centered on cursor position.
- Click and drag: pan when zoomed beyond fit.
- Double-click: toggle between fit-to-viewer and 100% (1:1 pixel) zoom.
- At 100% zoom, the image can be panned freely.
- Zoom resets to fit when navigating to a different file.

---

## 13. Video Behavior

**In the grid:**

- Thumbnail extracted from the video by the thumbnail pipeline.
- Duration badge always visible in the bottom-right corner.
- Videos do not autoplay in the grid. No hover preview.

**In the viewer:**

- Video begins playing automatically when the viewer opens.
- Playback controls are always visible at the bottom of the viewer: play/pause, seek bar, current time, total duration, volume.
- Clicking anywhere on the video (outside controls) toggles play/pause.
- Seek bar is draggable. Clicking any point on the seek bar seeks to that position.
- Volume is adjustable. Mute toggle available.
- `F` key toggles fullscreen within the viewer overlay (media fills the entire screen).
- Video loops when it reaches the end. Looping can be toggled via a control in the viewer.
- Navigating to the next or previous file while a video is playing stops playback of the current video before loading the next item.

---

## 14. Viewer Overlay Behavior

The viewer is an overlay that appears on top of the grid. It does not navigate to a new screen.

**Opening:**

- Single click on any grid cell opens the viewer for that file.
- The grid behind the viewer dims to approximately 85% opacity.
- The viewer opens with a fade-in animation (under 120ms).

**Layout:**

- Media is centered in the viewer area.
- Left and right navigation chevrons appear on hover over the left and right edges.
- A close button appears in the top-right corner.
- An info toggle button appears in the top-right area.

**Navigation:**

- Left arrow key or left chevron: previous file in the current filtered set.
- Right arrow key or right chevron: next file in the current filtered set.
- Navigation respects the current active filters and search query. It does not navigate through the entire library if a filter is active.
- Navigation wraps: going past the last file returns to the first.

**Info panel:**

- Toggled by pressing `I` or clicking the info button.
- Slides in from the right side of the viewer.
- Displays: filename, full path, file size, dimensions (images) or duration (videos), created date, modified date, assigned tags.
- Does not push the media. Overlays on top of the viewer background.
- Info panel state (open/closed) is not persisted across sessions.

**Closing:**

- `Escape` key closes the viewer.
- Clicking outside the media area (on the dimmed background) closes the viewer.
- Clicking the close button closes the viewer.
- On close, the grid scrolls back to and highlights the cell that was open in the viewer. Scroll position is exactly restored.

---

## 15. Selection and Multi-Selection Behavior

**Single selection:**

- `Ctrl+Click` on a cell selects it. If no other cells are selected, selection mode activates.

**Additive selection:**

- Additional `Ctrl+Click` adds or removes individual cells from the selection.

**Range selection:**

- `Shift+Click` selects all cells between the last selected cell and the clicked cell, in grid order.

**Select all:**

- `Ctrl+A` selects all media in the current filtered view.

**Selection mode:**

- Selection mode is active whenever one or more cells are selected.
- A contextual action bar slides up from the bottom of the screen when selection mode is active.
- The action bar shows: count of selected items, "Open file location" button, "Copy path(s)" button, "Deselect all" button.
- No destructive actions (rename, delete, move) exist in the action bar.

**Exiting selection mode:**

- Pressing `Escape` clears all selections and exits selection mode.
- Clicking "Deselect all" in the action bar exits selection mode.
- Clicking any cell without `Ctrl` held while in selection mode clears selection and opens the viewer for that cell.

---

## 16. Filtering Behavior

Filtering is the combination of active tag selections and the active search query.

**Tag filter:**

- Activating a tag filters the grid to files carrying that tag.
- Multiple tags default to OR logic.
- AND/OR toggle in the sidebar switches the filter logic for all active tags simultaneously.
- Tag filters and search filters are additive — both apply simultaneously.

**Filter indicator:**

- The top bar displays active filter state: how many tags are active, current search query if any.
- A "Clear all filters" button appears in the top bar whenever any filter is active. Activating it clears all tag selections and the search query simultaneously.

**Filter persistence:**

- Active filters are persisted across sessions as part of session state.
- The user reopens the application to the same filtered state they left.

---

## 17. Sorting Behavior

Sorting is controlled by a dropdown in the top bar.

**Available sort options:**

| Option                       | Description             |
| ---------------------------- | ----------------------- |
| Date modified (newest first) | Default on first launch |
| Date modified (oldest first) |                         |
| Date created (newest first)  |                         |
| Date created (oldest first)  |                         |
| Filename (A → Z)             |                         |
| Filename (Z → A)             |                         |
| File size (largest first)    |                         |
| File size (smallest first)   |                         |

**Behavior:**

- Sort applies to the entire filtered set, not just the visible cells.
- Sort order changes are immediate — the grid reflows without a loading state.
- Sort preference is persisted across sessions.
- Videos and images are not sorted into separate groups. They are sorted together by the active sort criterion.

---

## 18. Session Persistence Behavior

The application restores the following state on every launch after the first:

| State item               | Persisted                    |
| ------------------------ | ---------------------------- |
| Active tag filters       | Yes                          |
| AND/OR tag filter mode   | Yes                          |
| Active search query      | No — always clears on launch |
| Sort order               | Yes                          |
| Grid zoom level          | Yes                          |
| Sidebar width            | Yes                          |
| Sidebar collapsed state  | Yes                          |
| Scroll position in grid  | Yes                          |
| Window size and position | Yes                          |
| Source root list         | Yes                          |
| Root-as-tag toggle       | Yes                          |

Session state that is explicitly NOT persisted:

- Viewer open state. The app never re-opens the viewer on launch.
- Selection state. No cells are pre-selected on launch.
- Info panel open state within viewer.

---

## 19. Error Handling From a User Perspective

The application never displays a blocking error dialog for file-level failures.

**Unreadable files:**

- Files that cannot be read during indexing are silently skipped.
- A passive indicator in the bottom-left of the application shows a count of files that could not be indexed: "N files could not be indexed." Clicking this indicator opens a scrollable list of affected paths.

**Thumbnail generation failures:**

- If a thumbnail cannot be generated for a file, the cell shows a permanent placeholder with a media-type icon.
- No error message is shown for individual thumbnail failures.

**Offline source roots:**

- If a source root directory is unavailable at launch, a passive indicator appears: "1 source root is offline." The remaining available media is shown normally.
- The offline root's media remains visible in the grid with a subtle overlay indicating the files are currently inaccessible.

**Corrupt video files:**

- Attempting to open a corrupt video in the viewer shows a centered message within the viewer: "This file could not be played." Navigation controls remain functional. The user can navigate away.

**Application-level errors:**

- The application does not crash on any file-related error.
- If an unrecoverable application error occurs, the application displays a single dialog: "An unexpected error occurred. The application will close." with a button to close. No stack trace is shown to the user.

---

## 20. First-Launch Experience

**Condition:** No source roots have ever been configured.

**What the user sees:**

- The application opens to an empty state screen.
- Centered on screen: application name ("Vesper"), a one-line description ("Browse your media by folder"), and a single prominent button: "Add Source Directory."
- No sidebar is shown. No top bar controls are active except the settings button.

**What happens when the user clicks "Add Source Directory":**

- The native GNOME file chooser dialog opens (folder selection mode).
- The user selects a directory.
- The dialog closes.
- The application immediately begins indexing the selected directory in the background.
- The grid appears with placeholders populating in real time as thumbnails are generated.
- The sidebar populates with tags as they are derived during indexing.
- The user can browse immediately. Indexing completes in the background.

**After first source root is added:**

- The first-launch empty state is never shown again unless all source roots are removed.

---

## 21. Canonical Browsing Flow

**On every launch after the first:**

1. Application opens. Session state is restored (filters, sort, scroll position, zoom level).
2. The grid shows the last-viewed filtered state.
3. Thumbnails that were already generated are shown immediately. New files discovered since last launch appear as placeholders and gain thumbnails progressively.

**On first launch:**

- All media, no filters, sorted by date modified descending.

**Typical browsing session:**

1. User opens app. Sees last context.
2. User clicks a tag in the sidebar to narrow to a trip or event.
3. User scrolls the grid. Finds a file.
4. User single-clicks to open the viewer.
5. User navigates with arrow keys through the filtered set.
6. User presses `Escape` to return to the grid at the same scroll position.
7. User types in the search box to further narrow. Grid updates in real time.
8. User clears filters. Returns to full library.
9. User closes application.

---

## 22. Performance Expectations From a User Perspective

These are expected behaviors, not implementation targets.

- The application opens and is interactive within 2 seconds on a standard Linux desktop with an existing library.
- The grid is scrollable without visible stutter at all zoom levels for libraries up to 50,000 files.
- Applying or removing a tag filter updates the grid within 200ms for libraries up to 50,000 files.
- Search results update within 150ms of each keystroke for libraries up to 50,000 files.
- Opening the viewer for an already-thumbnailed image takes under 300ms.
- Video playback begins within 1 second of opening the viewer for local files.
- Thumbnail generation does not block the UI. The grid remains scrollable and interactive during background indexing.
- Adding a new source root with 10,000 files begins showing thumbnails within 5 seconds of confirmation. Full indexing completes in the background.

---

## 23. Accessibility and Keyboard Behavior

**Full keyboard navigation is supported.**

| Key                        | Action                                            |
| -------------------------- | ------------------------------------------------- |
| `Tab` / `Shift+Tab`        | Move focus between UI regions                     |
| `Arrow keys` (in grid)     | Move cell focus                                   |
| `Enter` (on focused cell)  | Open viewer                                       |
| `Escape`                   | Close viewer / exit selection mode / clear search |
| `Ctrl+B`                   | Toggle sidebar                                    |
| `Ctrl+A`                   | Select all in current view                        |
| `Ctrl+Click`               | Add cell to selection                             |
| `Shift+Click`              | Range select                                      |
| `F` (in viewer)            | Toggle fullscreen video                           |
| `I` (in viewer)            | Toggle info panel                                 |
| `←` / `→` (in viewer)      | Navigate to previous/next file                    |
| `Space` (in viewer, video) | Toggle play/pause                                 |

**Accessibility:**

- The application uses libadwaita's built-in accessibility support.
- All interactive elements are reachable via keyboard.
- All interactive elements have accessible labels.
- The application respects the system's high-contrast preference automatically via libadwaita.
- The application respects the system's dark/light mode preference. Dark mode is the recommended default.

---

## 24. Explicitly Accepted Constraints

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

## 25. Explicitly Rejected Features

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

## 26. Final Product Summary

Vesper is a fast, beautiful, keyboard-friendly media gallery for Linux that treats your existing folder structure as its organizational system.

You add directories. It indexes them. Your folder names become tags. You filter by those tags. You find your media. You view it.

It does not try to replace your filesystem. It does not try to be Lightroom. It does not ask you to import, organize, rate, or manage anything.

It does one thing: it makes browsing a large personal media collection on Linux feel as good as it should.

The application is dark by default, media-first in its visual design, and persistent in its session state. It opens where you left it. It never blocks you with dialogs. It never crashes on bad files. It never fights your folder structure.

The grid is the product. The viewer is the payoff. The tags are the map.

---

_This document describes the complete v1 product. Any feature not mentioned here is not part of v1._
