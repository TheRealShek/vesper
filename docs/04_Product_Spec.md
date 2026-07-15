# Product

---

## 0. Product Contract and Terms

This document is the source of truth for observable v1 behavior and acceptance criteria. [02_Architecture.md](02_Architecture.md) defines how the behavior remains correct and responsive; [03_Implementation.md](03_Implementation.md) defines the required GTK structure; [05_Visual_Design.md](05_Visual_Design.md) defines the visual language.

- **Library:** every indexed record from all configured source roots.
- **Visible library:** records whose roots are online and whose paths are not ignored.
- **Current view:** the visible library after active tag filters and search, in the active sort/ranking order.
- **Immediately:** the input handler returns without waiting for filesystem, database, decode, probe, external-process, or full-model work. The visual result must still meet Section 15's measured latency.
- **Non-blocking:** existing content and navigation remain usable; it does not merely mean work was moved behind a modal spinner.

## 1. Search Behavior

**One search box. No syntax. No prefixes.**

The search box is located in the top bar, always visible. Typing into it filters the current grid in real time.

**What is searched simultaneously:**

- Filename basename without extension
- Full file path, including extension
- Tag display names and tag display paths. Internal tag ids such as `source_root_id` are not searched.

**Ranking:**

Results are ranked deterministically:

1. Exact filename basename match.
2. Filename basename prefix match.
3. Filename basename substring match.
4. Exact tag match.
5. Tag substring match.
6. Path substring match.
7. Current sort order.
8. Full path ascending as the final tie-breaker.

**Ranking example:**

If the query is `japan` and the current sort is Date modified newest first:

- `Japan.jpg` ranks before `Japan_Trip.jpg` because exact basename beats prefix.
- `Japan_Trip.jpg` ranks before `My_Japan_Photo.jpg` because prefix beats substring.
- A file tagged `Japan` ranks before a file whose path contains `/Travel/Japan/` only as a path segment.
- Two files in the same rank tier use Date modified newest first.
- If two files still tie, the full path ascending order is used.

**Behavior:**

- Search activates on keystroke. No need to press Enter.
- Search operates on the currently filtered set. If a tag filter is active, search further narrows within that filtered result.
- Search is case-insensitive, Unicode-normalized, and substring-based.
- The displayed query is the user's raw text. Matching uses its Unicode-normalized, case-folded form after trimming leading/trailing whitespace; an empty effective query disables search.
- A media item is emitted once at its best matching rank even if several tags or path segments match.
- Search does not use fuzzy matching, prefixes, operators, or query syntax.
- Clearing the search box restores the tag-filtered view within the Section 15 search budget.
- Search and tag filters are independent dimensions. Both can be active simultaneously.
- The search box displays the current query at all times. It is never hidden.

---

## 2. Main Application Layout

The application has one persistent window divided into a fixed sidebar column and a grid column whose header belongs only to the grid column:

```
┌───────────┬──────────────────────────────────┐
│           │             TOP BAR              │
│           ├──────────────────────────────────┤
│  SIDEBAR  │                                  │
│           │             GRID                 │
│           │                                  │
└───────────┴──────────────────────────────────┘
```

**Top bar:** Contains `Vesper` at the start, the centered search box, then Clear filters when applicable, thumbnail-size slider, labeled Sort menu, and Settings at the end. It is always visible above the grid column and never spans or overlays the sidebar.

**Sidebar:** Contains the tag list and source root indicators. It has a fixed width of 220px and is always visible.

**Grid:** The main content area. Fills all remaining space. Scrolls vertically. Never paginates — it is one continuous scrollable surface.

There is no navigation history, no back button, no breadcrumb. The application is always showing one unified library with filters applied.

---

## 3. Sidebar Behavior

The sidebar contains the tag list.

**Tag list:**

- Tags are displayed as a flat list — not a tree, not a hierarchy.
- Tags are sorted by file count descending, then case-insensitive display name, then exact path identity.
- Each tag entry shows: tag name and file count.
- If multiple tags share the same displayed name, the entry provides folder lineage as secondary text or tooltip.
- If the folder lineage also collides across source roots, the entry includes source-root display name or path in secondary text or tooltip.
- The list is scrollable.
- After the 30th tag, a "Show more" control appears. Activating it expands the full list for the current session and changes the control to "Show less."
- Tag-list expansion state is not persisted.
- A "Filter tags" input at the top of the sidebar filters the tag list itself (not the media grid).
- Tag-list filtering searches all tags, not only the first 30 visible tags.
- While tag-list filtering is active, all matching tags are shown and the "Show more/less" control is hidden. Clearing the tag query restores its prior session-only expanded/collapsed state.

**Tag selection:**

- Clicking a tag activates it as a filter. The grid updates immediately.
- Multiple tags can be active simultaneously.
- By default, multiple active tags use OR logic — the grid shows files matching any active tag.
- A toggle in the sidebar switches to AND logic — the grid shows only files matching all active tags simultaneously. The toggle appears only when two or more tags are active.
- Active tags use the Visual Design leading accent indicator and subtle row background; tags are flat rows, not chips.
- Clicking an active tag deactivates it.
- Global clearing is handled by the neutral `Clear filters (N)` header button and the no-results clear button. There is no separate sidebar "Clear all" control in v1.

---

## 4. Grid View Behavior

The grid displays all media matching the current filter and search state.

**Layout:**

- All cells are square.
- The number of columns adjusts to fill available width based on the current zoom level.
- Thumbnail size is controlled by a five-detent slider in the top bar. The accessible/tooltip values are XS, S, M, L, and XL; printed labels and zoom icons are not shown around the slider.
- Default zoom level: M.
- The grid scrolls vertically. It does not paginate.
- The grid is virtualized — only visible and a small near-visible buffer of cells are bound/rendered. The v1 acceptance target is 50,000 indexed files; larger libraries are best effort and must not cause unbounded memory growth.

**Empty state:**

- If no source roots are configured: a centered prompt asking the user to add a source directory. A single button opens the native file chooser directly, same as first-launch (Section 13).
- If filters or search produce no results: a centered message stating no media matches the current filters. A button clears active tag filters and search query only. It does not reset sort, zoom, source roots, or match mode.

**Loading state:**

- On first launch or after adding a new source root, thumbnails appear progressively as they are generated. Cells show a placeholder until their thumbnail is ready. The grid is usable immediately — the user does not wait for indexing to complete.

---

## 5. Grid Cell Specification

Every cell is square. The thumbnail fills the entire cell, center-cropped. Non-square media is never letterboxed.

**Four visual states:**

**Default (no interaction):**

- Thumbnail fills cell entirely.
- No text overlay. No badges except video duration.
- Video cells display a duration badge in the bottom-right corner. Format: `M:SS` or `H:MM:SS`. It uses the Visual Design high-contrast dark surface and white text.
- If video duration is unavailable, no duration badge is shown.

**Hover:**

- A gradient overlay rises from the bottom of the cell.
- The filename appears in the overlay, single line, truncated with ellipsis if needed.
- No file-type icon appears in the hover overlay; it is redundant with the media and video-duration treatment.
- Video cells continue to show the duration badge.
- Hover state activates/fades within 120ms of pointer entry/exit.

**Focused:**

- A clear focus outline appears around the card boundary.
- The filename overlay appears, matching hover behavior for keyboard users.
- Focused and selected states can coexist.

**Selected:**

- An accent-colored border appears around the cell perimeter.
- A checkmark appears in the top-left corner on a small circular accent background.
- The thumbnail receives a black tint of no more than 12%; its picture opacity is not reduced.
- Selected state is additive — multiple cells can be selected simultaneously.

**Thumbnail placeholder:**

- Cells without a generated thumbnail show a neutral theme surface with a centered media-type icon.
- Placeholder is replaced by the actual thumbnail as soon as it is ready. No refresh required.
- Placeholders do not shimmer. A native spinner may appear only when a visible decode takes longer than 400ms.

---

## 6. Image Behavior

**In the grid:**

- Static thumbnail, center-cropped to square.
- GIF files show the first frame only. They do not animate in the grid.

**In the viewer:**

- The viewer opens immediately with a stable loading surface while the full-resolution image is read and decoded in the background. Loading a large or slow file must not freeze viewer controls or navigation.
- GIF files display first frame only in the viewer; no animation.
- Default display: fit to viewer area, maintaining aspect ratio.
- Scroll wheel: zoom in and out, centered on cursor position.
- Click and drag: pan when zoomed beyond fit.
- Double-click: toggle between fit-to-viewer and 100% (1:1 pixel) zoom.
- Minimum zoom is fit-to-viewer.
- Maximum zoom is 800%.
- Zoom changes in 12.5% relative steps.
- Pan is clamped to image bounds.
- At 100% zoom, the image can be panned freely.
- Zoom resets to fit when navigating to a different file.
- If decode fails, the viewer shows "This image could not be displayed." while close and next/previous navigation remain functional.

---

## 7. Video Behavior

**In the grid:**

- Thumbnail extracted from the video by the thumbnail pipeline.
- Duration badge appears in the bottom-right corner when duration metadata is available.
- Videos do not autoplay in the grid. No hover preview.

**In the viewer:**

- Video begins playing automatically when the viewer opens.
- Playback controls are always visible at the bottom of the viewer: play/pause, seek bar, current time, total duration, volume.
- Clicking anywhere on the video (outside controls) toggles play/pause.
- Seek bar is draggable. Clicking any point on the seek bar seeks to that position.
- Volume is adjustable. Mute toggle available.
- `F` key toggles viewer fullscreen for both images and videos. For videos, playback controls remain available.
- Video does not loop by default. Looping can be toggled via a visible control in the viewer playback bar and is remembered only for the current viewer session.
- Navigating to the next or previous file while a video is playing stops playback of the current video before loading the next item.

---

## 8. Viewer Overlay Behavior

The viewer is a full-application overlay that appears above the grid, sidebar, and header. It does not navigate to a new screen.

**Opening:**

- Single click on any grid cell opens the viewer for that file.
- Opening the viewer clears any active selection and hides the selection action bar.
- A black scrim with 92% opacity covers the underlying application; fullscreen uses solid black. The content itself is not assigned opacity.
- The viewer opens with a fade-in animation (under 120ms).

**Layout:**

- Media is centered in the viewer area.
- Left and right navigation chevrons appear on pointer proximity to the corresponding edge, keyboard focus, or recent keyboard navigation.
- Close and Info form one compact top-right OSD toolbar; they are not separate oversized circular buttons.
- Viewer fullscreen expands media within the viewer overlay and hides nonessential viewer chrome. The underlying header and sidebar are already covered by the viewer overlay.

**Navigation:**

- Left arrow key or left chevron: previous file in the current filtered set.
- Right arrow key or right chevron: next file in the current filtered set.
- When the viewer opens, it captures a snapshot of the current filtered, sorted media list. Navigation uses that snapshot until the viewer closes.
- Navigation respects the active filters and search query from the moment the viewer opened. It does not navigate through the entire library if a filter is active.
- If the current file becomes unavailable while the viewer is open, the viewer shows a file-unavailable message and keeps next/previous navigation functional.
- The viewer snapshot stores stable media identities, not GTK row indices. Items removed or made offline after opening are skipped during next/previous navigation; if no navigable items remain, the viewer shows an empty/unavailable state with only close available.
- Navigation wraps: going past the last file returns to the first, and going before the first wraps to the last. Wrapping uses a brief 120ms opacity-only edge cue in the navigation direction; never flash or scale the whole viewer.

**Info panel:**

- Toggled by pressing `I` or clicking the info button.
- Slides in from the right side of the viewer.
- Displays: filename, full path, file size, dimensions (images) or duration (videos), date added, modified date, assigned tags.
- The full path is selectable, uses middle ellipsis when needed, and exposes a copy affordance or context action.
- The info panel pushes the media layout. The media area shrinks to accommodate the panel, preventing any overlap.
- Info panel state (open/closed) is not persisted across sessions.

**Closing:**

- `Escape` key closes the viewer.
- Clicking outside the media area (on the dimmed background) closes the viewer.
- Clicks on viewer controls, media, or the info panel do not close the viewer.
- Clicking the close button closes the viewer.
- On close, the grid returns to the item and within-cell offset captured in memory when the viewer opened, then highlights that cell for 900ms. This viewer-origin anchor is separate from the session-persistence anchor. If the item no longer belongs to the current view, retain the nearest valid scroll position and do not fabricate a highlight.

---

## 9. Selection and Multi-Selection Behavior

**Single selection:**

- `Ctrl+Click` on a cell selects it. If no other cells are selected, selection mode activates.

**Additive selection:**

- Additional `Ctrl+Click` adds or removes individual cells from the selection.

**Range selection:**

- `Shift+Click` selects all cells between the last selected cell and the clicked cell, in grid order.
- If no range anchor exists, `Shift+Click` selects only the clicked cell and makes it the range anchor.

**Select all:**

- `Ctrl+A` selects all media in the current filtered view.
- `Ctrl+Space` toggles the focused cell without opening it. `Shift+Space` extends the range from the selection anchor to the focused cell. These are the keyboard equivalents of modifier-click selection.

**Selection mode:**

- Selection mode is active whenever one or more cells are selected.
- A contextual action bar slides up from the bottom of the grid area when selection mode is active; it never covers the sidebar.
- The action bar shows: count of selected items, "Open file location" button, "Copy path(s)" button, "Deselect all" button.
- The "Open file location" button is disabled if the selection spans more than one physical folder, displaying a tooltip that explains they must be in the same folder to open their location.
- The "Copy path(s)" button copies the full paths of all selected items to the clipboard as a newline-separated list, without quotes.
- No destructive actions (rename, delete, move) exist in the action bar.
- Any filter, search, source-root availability, or sort change clears selection and exits selection mode.
- Selection is stored by stable media identity, never by mutable grid index. Actions use the current selected identities even when virtualized cells are unbound.
- Pressing `Enter` on a focused cell while selection mode is active clears selection and opens the viewer for the focused cell.
- Rubber-band drag selection is not part of v1.

**Exiting selection mode:**

- Pressing `Escape` clears all selections and exits selection mode.
- Clicking "Deselect all" in the action bar exits selection mode.
- Clicking any cell without `Ctrl` held while in selection mode clears selection and opens the viewer for that cell.

---

## 10. Filtering Behavior

Filtering is the combination of active tag selections and the active search query.

**Tag filter:**

- Activating a tag filters the grid to files carrying that tag.
- Multiple tags default to OR logic.
- AND/OR toggle in the sidebar switches the filter logic for all active tags simultaneously.
- Tag filters and search filters are additive — both apply simultaneously.

**Filter indicator:**

- A neutral header button displays `Clear filters (N)`, where `N` is the number of active tags plus one when search is active.
- The control is not a pill or suggested/primary action. Its accessible description names the active dimensions, for example `Clear two tag filters and search`.
- Clicking it immediately clears all search and tag filter criteria simultaneously.

**Filter persistence:**

- Active tag filters are persisted across sessions as part of session state.
- The active search query is NOT persisted and is always cleared on launch.
- The user reopens the application to the same tag-filtered state they left.
- Filters belonging to an offline root are suspended and omitted from the active-filter count until that root is successfully rescanned. Filters whose root was removed are discarded.

---

## 11. Sorting Behavior

Sorting is controlled by the labeled `Sort` menu button in the top bar.

**Available sort options:**

| Option                       | Description             |
| ---------------------------- | ----------------------- |
| Date modified (newest first) | Default on first launch |
| Date modified (oldest first) |                         |
| Date added (newest first)    |                         |
| Date added (oldest first)    |                         |
| Filename (A → Z)             |                         |
| Filename (Z → A)             |                         |
| File size (largest first)    |                         |
| File size (smallest first)   |                         |

**Behavior:**

- Sort applies to the entire filtered set, not just the visible cells.
- A sort change enqueues asynchronous work immediately and meets Section 15's filter/sort budget. The previous valid grid remains interactive until the new ordered result replaces it; do not blank the grid or show a modal loading state.
- Sort preference is persisted across sessions.
- Videos and images are not sorted into separate groups. They are sorted together by the active sort criterion.
- Filename sort is case-insensitive natural sort, with full path as the final tie-breaker.

---

## 12. Error Handling From a User Perspective

The application never displays a blocking error dialog for file-level failures.

**Diagnostics and privacy:**

- Vesper has no telemetry in v1.
- Diagnostics are local-only.
- The application does not prompt users to upload logs.
- User media files are never uploaded, synced, or sent anywhere by diagnostics.

**Unreadable files:**

- Files that cannot be read during indexing are skipped.
- A passive indicator in the bottom-left of the grid area shows a count of files that could not be indexed: "N files could not be indexed." Clicking this indicator opens a non-blocking popover (not a dialog or new page) containing a scrollable list of all the affected paths.
- Scan errors are tied to scan generation. A later successful scan of the same path clears the previous error.

**Thumbnail generation failures:**

- If a thumbnail cannot be generated for a file, the cell shows a permanent placeholder with a media-type icon.
- No error message is shown for individual thumbnail failures.

**Offline source roots:**

- If a source root directory is unavailable at launch, a passive indicator appears: "1 source root is offline." The remaining available media is shown normally.
- The media files belonging to the offline source root are hidden from the grid, search, selection, viewer navigation, and tag counts. Offline media is not shown as disabled or dimmed grid cells in v1.

**Corrupt video files:**

- Attempting to open a corrupt video in the viewer shows a centered message within the viewer: "This file could not be played." Navigation controls remain functional. The user can navigate away.

**Application-level errors:**

- The application does not crash on any file-related error.
- If an unrecoverable application error occurs, the application displays a single dialog: "An unexpected error occurred. The application will close." with a button to close. No stack trace is shown to the user.
- A recognized database migration or index-corruption failure is recoverable when source roots and settings can be read safely. Its startup dialog explains that user media is unaffected and offers "Rebuild Library Index" and "Close". Rebuild progress is non-modal once the main window can open. Unknown failures use the generic closing dialog above.
- Unrecoverable application errors use this dialog only. Recoverable critical states may use banners or passive status surfaces.

---

## 13. First-Launch Experience

**Condition:** No source roots are currently configured.

**What the user sees:**

- The application opens to an empty state screen.
- Centered on screen: application name ("Vesper"), a one-line description ("Browse your media by your folder structure."), and a single prominent button: "Add Source Directory."
- The sidebar is shown normally with a "No tags available" placeholder. Settings and shortcut help remain active; search, sort, and zoom remain visible but are disabled until at least one visible media record exists. Disabled controls retain accessible explanations.

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

## 14. Canonical Browsing Flow

**On every launch after the first:**

1. Application opens. Session state is restored (filters, sort, scroll anchor, zoom level).
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

## 15. Performance Acceptance Budgets

These are release acceptance requirements. Measure a release build using the reference environment and report median and 95th-percentile (p95) results across at least 20 repetitions after one untimed warm-up. A result passes when its stated p95 budget is met. Do not count native file-chooser time or time spent waiting for unavailable/network storage.

**Reference environment for acceptance testing:**

- Linux desktop on GNOME/Wayland.
- 4 physical CPU cores or better.
- 8 GB RAM or better.
- SSD storage for the library and app cache.
- Warm start means the SQLite database and visible initial thumbnails exist and OS cache may be warm.
- Cold start means the database exists and visible thumbnails exist, but OS disk cache is dropped/not assumed warm. First-ever indexing is measured separately.
- HDD and network-mounted folders are supported only as best effort; they are not the baseline for timing acceptance.
- Test libraries should include a realistic mix of nested folders, duplicate folder names, JPEG/PNG/GIF/WEBP images, and common MP4/MOV/WebM videos.
- The 10,000-file add-root test includes mixed image/video discovery and thumbnail scheduling, but does not wait for every video thumbnail.

| Interaction | p95 budget | Completion point |
| --- | ---: | --- |
| Warm launch, existing 50k library | 2s | Window accepts input and cached initial cells are visible |
| Cold launch, existing 50k library | 3s | Same as warm launch; background availability checks may continue |
| Add/remove tag or change AND/OR mode | 200ms | Correct first viewport is bound |
| Search keystroke | 150ms | Correct first viewport for the latest query is bound; superseded queries never flash |
| Change sort | 200ms | Correctly ordered first viewport is bound |
| Open cached image viewer | 300ms | Viewer chrome and cached/decoding surface are responsive |
| Start supported local video | 1s | First frame/playback or a codec error is visible |
| Close viewer | 200ms | Origin/nearest grid cell is visible and keyboard focus restored |
| Open Settings or shortcut help | 200ms | Dialog is visible and accepts input |
| Add a 10k-file source root | 1s / 5s | Indexing status within 1s; first discovered rows/available thumbnails within 5s |

- During grid scrolling on a 60Hz display, p95 frame time is at most 16.7ms and no GTK-thread task caused by indexing/query/thumbnail publication may run longer than 8ms. Occasional compositor or codec startup frames outside Vesper's control are excluded and must be noted.
- GTK-thread input callbacks return within 8ms. Filesystem liveness, SQLite queries, media probing/decoding, thumbnail work, cache eviction, and large clipboard preparation are never performed synchronously in them.
- Indexing, regeneration, and watcher bursts use bounded memory and queues. Repeated input supersedes obsolete query work instead of growing a backlog.
- Thumbnail generation does not block the UI. The grid remains scrollable and interactive during background indexing.
- Full indexing and all thumbnail completion have no fixed wall-clock budget because media complexity and storage speed vary; progress/status must continue updating and already-discovered media must remain usable.

---

## 16. Accessibility and Keyboard Behavior

**Full keyboard navigation is supported.**

| Key                        | Action                                            |
| -------------------------- | ------------------------------------------------- |
| `Tab` / `Shift+Tab`        | Move focus between UI regions                     |
| `Arrow keys` (in grid)     | Move cell focus                                   |
| `Enter` (on focused cell)  | Open viewer                                       |
| `Escape`                   | Close viewer / exit selection mode / clear search |
| `Ctrl+A`                   | Select all in current view                        |
| `Ctrl+Space`               | Toggle selection of focused grid cell             |
| `Shift+Space`              | Range-select to focused grid cell                 |
| `Ctrl+Click`               | Add cell to selection                             |
| `Shift+Click`              | Range select                                      |
| `F` (in viewer)            | Toggle viewer fullscreen                          |
| `I` (in viewer)            | Toggle info panel                                 |
| `←` / `→` (in viewer)      | Navigate to previous/next file                    |
| `Space` (in viewer, video) | Toggle play/pause                                 |

**Accessibility:**

- The application uses libadwaita's built-in accessibility support.
- All interactive elements are reachable via keyboard.
- All interactive elements have accessible labels.
- The application respects the system's high-contrast preference automatically via libadwaita.
- The application respects the system's dark/light mode preference. If no system preference is available, default to dark.

**Accessibility acceptance criteria:**

- A user can add a source directory, focus the grid, open a media item, navigate to next/previous, and close the viewer using keyboard navigation.
- A user can select one or more focused grid cells, copy selected paths, and deselect all using keyboard navigation.
- A user can open and close Settings and Keyboard Shortcuts. Focus returns to the invoking control/cell, or to the grid/first enabled header control if the invoker no longer exists.
- Icon-only controls expose accessible names matching their action.
- Indexing, offline-root, and scan-error states are perceivable as text, not only icon or color changes.

---

## 17. GRID CELL STATES

```
DEFAULT (no interaction):
┌─────────────┐
│             │
│  thumbnail  │  ← center-cropped, fills cell
│  (square)   │
│        1:23 │  ← duration badge, video only, bottom-right when known
└─────────────┘

HOVER (cursor over cell, activates within 120ms):
┌─────────────┐
│             │
│  thumbnail  │
│▓▓▓▓▓▓▓▓▓▓▓▓│  ← gradient rises from bottom
│ filename…   │  ← name only, truncated
└─────────────┘

FOCUSED (keyboard focus / Tab navigation):
┌─────────────┐  ← solid accent outline around card boundary (offset by 2px)
│             │
│  thumbnail  │
│▓▓▓▓▓▓▓▓▓▓▓▓│  ← filename overlay revealed for keyboard-first parity
│ filename…   │
└─────────────┘

SELECTED (Ctrl+Click):
┌─────────────┐  ← accent border around perimeter
│✓            │  ← checkmark, top-left, accent circle bg
│  thumbnail  │  ← subtle dark tint
│  (tinted)   │
└─────────────┘
```

**Focus and overlay parity:**

- The filename hover overlay must also be shown when a card has keyboard focus.
- The focus ring uses a solid clear outline offset by 2px from the card edge, matches the card radius, and remains distinct from the selection border.

---

## 18. VIEWER OVERLAY STATES

```
VIEWER OPEN (single-click on cell):
┌─────────────────────────────────────────────────┐
│  [black scrim — 92% opacity]        [info][close]│
│                                                 │
│  ‹                  [media]                  ›  │
│              (chevrons on hover)                │
│                                                 │
│         [▶ 0:12 / 1:30 ══════●══ 🔊 ↺]         │  ← video only
└─────────────────────────────────────────────────┘

VIEWER + INFO PANEL (press I):
┌──────────────────────────────┬──────────────────┐
│                              │ filename.jpg     │
│         [media]              │ /full/path       │
│                              │ 4.2 MB           │
│                              │ 1920×1080        │
│                              │ 2024-03-12       │
│                              │ Tags: A, B       │
└──────────────────────────────┴──────────────────┘
Info panel slides in from right and pushes the media (shrinking the media area).
```

---

## 19. SELECTION ACTION BAR

Appears when ≥1 cell selected. Slides up from and attaches to the bottom edge of the grid. Sidebar unaffected. It is an opaque toolbar with a top border, not a floating capsule.

```
┌─────────────────────────────────────────────────┐
│  ✓ 3 selected  [Open Location] [Copy Path(s)] [Deselect all]  │
└─────────────────────────────────────────────────┘
```

No destructive actions (no delete, rename, move).

`Deselect all` uses neutral styling; it is not destructive/red. Open and Copy retain visible labels even if standard symbolic icons accompany them.

- **Open Location**: Opens the one containing folder in the system file manager. It is disabled if the selection contains items from more than one physical folder, showing a tooltip explaining: "Selected files must reside in the same folder." A launch failure is reported passively and does not clear selection.
- **Copy Path(s)**: Copies the full path of each selected item to the clipboard, formatted as a newline-separated list with no quotation marks.

**Accessibility:** buttons must expose labels such as `Open containing folder`, `Copy selected paths`, and `Deselect all`. Keyboard focus must be reachable without trapping focus in the bar.

---

## 20. EMPTY STATES

**No source roots configured:**

```
┌────────────────────────────────────────────────────────┐
│ Vesper   [Search disabled]   [Size] [Sort] [Settings] │
│────────────────────────────────────────────────────────│
│                    [folder icon]                       │
│          Browse your media by your folder structure.  │
│                                                        │
│                 [ Add Source Directory ]               │
│                                                        │
│          Press F1 or Ctrl+? for keyboard shortcuts     │
└────────────────────────────────────────────────────────┘
Sidebar: Rendered always, even in the first-launch empty state. The tag list shows a "No tags available" placeholder, and the sources list shows as empty.
```

**Filters/search produce no results:**

```
Centered in grid area:
"No media matches the current filters."
[ Clear filters ]  ← clears active tag filters and search query only
```

---

## 21. INDEXING / SCANNING STATE

When scanning/indexing is active, the app must provide visible, non-blocking feedback. Adding a large source directory must never leave the user wondering whether anything happened.

**Required behavior:**

- Show an indexing/scanning status indicator while background scan work is active.
- The indicator must be non-modal. Do not use progress dialogs or blocking overlays.
- The grid, sidebar, header, and viewer remain usable while scanning continues.
- Prefer stable status text over precise progress if total work is unknown.
- If available, status may show discovered/indexed item counts, for example `Indexing media… 438 files found`.
- Status updates are stable and screen-reader friendly: update displayed counts at most ten times per second, while completion and failure appear immediately.
- Scan errors remain represented by `scan_error_button` / error surfaces and must not be silently swallowed.

**Acceptable placements:**

- below the header as a banner/status row

Indexing status and offline-root status share the status banner/row stack below the header, with offline-root status taking priority. Scan errors remain in the grid-area passive indicator.

**Do not:**

- block the UI thread
- show a modal progress dialog
- prevent browsing already indexed items
- fake progress percentages when total work is unknown

---

## 22. KEYBOARD SHORTCUTS

| Key             | Context      | Action                         |
| --------------- | ------------ | ------------------------------ |
| `Escape`        | Viewer open  | Close viewer                   |
| `Escape`        | Selection    | Deselect all, exit mode        |
| `Escape`        | Search focus | Clear search                   |
| `←` `→`         | Viewer       | Previous / next file           |
| `I`             | Viewer       | Toggle info panel              |
| `F`             | Viewer       | Toggle viewer fullscreen       |
| `Space`         | Viewer+video | Toggle play/pause              |
| `Enter`         | Grid focus   | Open viewer                    |
| `Ctrl+A`        | Grid         | Select all in filtered view    |
| `Ctrl+Space`    | Grid focus   | Toggle focused cell selection  |
| `Shift+Space`   | Grid focus   | Range-select to focused cell   |
| `Ctrl+Click`    | Grid         | Add cell to selection          |
| `Shift+Click`   | Grid         | Range select                   |
| `F1` / `Ctrl+?` | Global       | Open Keyboard Shortcuts window |

**Keyboard Shortcuts Window:**

The Keyboard Shortcuts window is a modal dialog (`gtk::ShortcutsWindow`) displaying a static two-column layout mapping keys to their actions. It is populated directly from the keyboard shortcut table above.

**Shortcut precedence:**

- `Escape` precedence: shortcut/help window closes first; viewer fullscreen exits second; viewer closes third; selection clears fourth; focused search entry clears fifth; otherwise no-op.
- Text entries consume normal text-editing keys unless the viewer is open.
- Grid keyboard navigation remains active when no text entry is focused.
- Viewer shortcuts take precedence only while the viewer is open.
- When the Keyboard Shortcuts window closes, focus returns to the widget or grid cell that opened it.

---

## 23. ACCESSIBILITY AND FOCUS

Vesper is keyboard-first. Focus state and selection state are separate concepts and must not be conflated.

**Definitions:**

- **Focused**: the widget or grid cell that receives keyboard input.
- **Selected**: one or more media items included in the current selection set for batch actions.
- A grid cell can be focused but not selected, selected but not focused, both, or neither.

**Focus ring requirements:**

- The focused grid cell must have a visible focus indicator distinct from the selected state.
- The selected state uses accent border/checkmark/tint as defined in Grid Cell States.
- The focused state must remain visible during keyboard navigation, even when no item is selected.
- Native GTK/libadwaita focus styling is preferred for buttons, entries, sliders, radio buttons, and popover controls.
- Do not remove focus outlines globally in CSS.
- `Tab` / `Shift+Tab` must move predictably through header controls, sidebar controls, grid, viewer controls, and action bar controls.
- Text entries may consume text-editing keys while focused; grid/viewer shortcuts apply when focus is not inside editable text.

**Accessibility requirements:**

- Icon-only buttons must expose accessible labels and tooltips.
- State-changing controls must expose their current state through native GTK widgets where possible.
- Error and indexing states must be perceivable as text, not only icons/color.

---

## 24. SETTINGS DIALOG

The Settings panel is a modal dialog (`adw::PreferencesWindow` or modal `gtk::Window`) and not an inline overlay.

**Widget Tree:**

```
adw::PreferencesWindow [modal=true]
└── adw::PreferencesPage
    ├── adw::PreferencesGroup [title="Source Directories"]
    │   ├── gtk::ListBox [roots_list_box]
    │   │   └── Row with path, offline state if applicable, and remove button
    │   └── gtk::Button "Add Source Directory"
    ├── adw::PreferencesGroup [title="Tag Behavior"]
    │   └── adw::ActionRow [title="Include source root name as tag"]
    │       └── gtk::Switch [root_as_tag_switch]
    ├── adw::PreferencesGroup [title="Ignore Rules"]
    │   ├── gtk::ScrolledWindow
    │   │   └── gtk::TextView [global_ignore_text_view]
    │   ├── gtk::Button "Restore Default Ignore Rules"
    │   └── gtk::Button "Apply Ignore Rules"
    └── adw::PreferencesGroup [title="Library Maintenance"]
        ├── gtk::Button "Rescan Library"
        ├── gtk::Button "Regenerate Thumbnails"
        └── gtk::Button "Rebuild Library Index"
```

**Settings Fields:**

- **Source Roots List**: List showing currently configured directories, with a button to remove each, and an "Add Source Directory" button.
- **Ignore Rules List**: Multi-line text field containing global ignore patterns, one pattern per line.
- **Root-as-Tag Toggle**: Switch to control whether the source root directory name itself is included as a tag.
- **Restore Default Ignore Rules**: Appends any missing default ignore rules to the ignore-rules text field without removing user-defined rules.
- **Apply Ignore Rules**: Validates and saves the complete field, then rescans online roots. It is enabled only while the field differs from the saved rules.
- **Rescan Library**: Refreshes source-root availability, ignore-rule results, media metadata, tag derivation, and deleted/new file records.
- **Regenerate Thumbnails**: Recreates thumbnails for modified or failed media in the background.
- **Rebuild Library Index**: Recreates database-derived records from configured source roots while preserving settings. It never modifies user media files.

**Settings behavior:**

- Adding a root begins background indexing immediately if the root is accepted.
- Adding an overlapping, duplicate, or nested root is rejected with a non-blocking message: "This folder is already covered by an existing source directory."
- Removing a root asks for confirmation showing the root path and the explicit text "Files on disk will not be changed." Confirming cancels active work for that root and removes its records from the library; canceling makes no change.
- Clicking "Restore Default Ignore Rules" updates the ignore-rules text field only.
- "Apply Ignore Rules" validates the entire field. Valid rules are saved atomically and trigger a rescan of all online roots. Invalid rules leave the previous saved rules active and show the first invalid line inline.
- Closing Settings discards unapplied ignore-rule edits. Root additions/removals and root-as-tag changes apply immediately and are not rolled back.
- Toggling root-as-tag enqueues re-derivation immediately. The current grid remains usable, selection clears, and tags/counts are replaced as one completed generation rather than row by row.
- Maintenance actions are non-blocking and report progress through indexing/scanning status surfaces.

---

## 25. Cross-Feature Acceptance Scenarios

A v1 implementation is not complete until these end-to-end scenarios pass in addition to the Section 15 budgets:

1. **First run and roots:** start with no state, add a valid root, browse records while thumbnails are pending, reject duplicate/nested/containing roots, then remove the root after confirmation without changing any media file.
2. **Path and tag identity:** index duplicate folder basenames under different lineages/roots, show distinct disambiguated tags and correct online counts, and prevent a supported file symlink plus its target from appearing twice.
3. **Latest-query wins:** type several search characters quickly while changing tags/sort; only the newest query appears, ordering is deterministic, input remains responsive, and no obsolete full result flashes.
4. **Watcher correctness:** create, partially copy, modify, rename, and delete supported files. Unstable copies do not appear early; rename is remove-plus-add; modified media keeps its old thumbnail until successful explicit regeneration; a failed/canceled scan never deletes unseen records.
5. **Offline recovery:** make one root unavailable while another stays online. Offline media, navigation entries, and counts disappear; affected filters suspend; records remain stored; media returns only after a successful rescan.
6. **Viewer and selection:** select by mouse and keyboard, run copy/open-location rules, open the viewer (which clears selection), navigate a stable snapshot across media errors, then close to the captured origin/nearest valid cell with focus restored.
7. **Restart:** persist roots, tag filters/mode, sort, zoom, window size, and a stable scroll anchor; clear search, selection, viewer, and info-panel state; safely discard identities that no longer exist.
8. **Failure isolation:** test unreadable/corrupt media and missing `ffmpeg`, `ffprobe`, and playback codecs. Startup and browsing continue, aggregate/path errors and placeholders appear in the specified surfaces, and no file-level modal dialog is shown.
9. **Visual quality:** pass every checklist item in [05_Visual_Design.md](05_Visual_Design.md#12-visual-acceptance-checklist) in light, dark, and high-contrast appearances without regressing Section 15.

---

## Cross-References

- [Target User and Usage Model](01_Vision.md#3-target-user-and-usage-model)
- [Source Directory Model](02_Architecture.md#1-source-directory-model)
- [Ignore Rules](02_Architecture.md#2-ignore-rules)
- [Tag Model and Tag Behavior](02_Architecture.md#3-tag-model-and-tag-behavior)
- [Session Persistence Behavior](02_Architecture.md#8-session-persistence-behavior)
- [Widget Tree](02_Architecture.md#9-widget-tree-source-of-truth)
- [Sidebar Internal Layout](03_Implementation.md#1-sidebar-internal-layout)
- [Header Bar Layout](03_Implementation.md#2-header-bar-layout)
- [State → UI Mapping](02_Architecture.md#10-state--ui-mapping)
- [Optional Future / Taste Tradeoffs](01_Vision.md#6-optional-future--taste-tradeoffs)
- [What Not To Do](03_Implementation.md#10-what-not-to-do-agent-guard-rails)
- [Visual Design](05_Visual_Design.md)
