# Product

---

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
- Search does not use fuzzy matching, prefixes, operators, or query syntax.
- Clearing the search box returns the grid to the pre-search state instantly.
- Search and tag filters are independent dimensions. Both can be active simultaneously.
- The search box displays the current query at all times. It is never hidden.

---

## 2. Main Application Layout

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

**Tag selection:**

- Clicking a tag activates it as a filter. The grid updates immediately.
- Multiple tags can be active simultaneously.
- By default, multiple active tags use OR logic — the grid shows files matching any active tag.
- A toggle in the sidebar switches to AND logic — the grid shows only files matching all active tags simultaneously. The toggle appears only when two or more tags are active.
- Active tags are visually distinguished (filled chip vs outlined chip).
- Clicking an active tag deactivates it.
- Global clearing is handled by the active filter pill in the header and the no-results clear button. There is no separate sidebar "Clear all" control in v1.

---

## 4. Grid View Behavior

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
- Video cells display a duration badge in the bottom-right corner. Format: `M:SS` or `H:MM:SS`. The badge is semi-transparent dark with white text.
- If video duration is unavailable, no duration badge is shown.

**Hover:**

- A gradient overlay rises from the bottom of the cell.
- The filename appears in the overlay, single line, truncated with ellipsis if needed.
- A file type indicator (image or video icon) appears in the overlay.
- Video cells continue to show the duration badge.
- Hover state activates within 150ms of cursor entry. It fades out within 150ms of cursor exit.

**Focused:**

- A clear focus outline appears around the card boundary.
- The filename and file type overlay appears, matching hover behavior for keyboard users.
- Focused and selected states can coexist.

**Selected:**

- An accent-colored border appears around the cell perimeter.
- A checkmark appears in the top-left corner on a small circular accent background.
- The thumbnail receives a subtle dark tint.
- Selected state is additive — multiple cells can be selected simultaneously.

**Thumbnail placeholder:**

- Cells without a generated thumbnail show a neutral dark background with a centered media-type icon.
- Placeholder is replaced by the actual thumbnail as soon as it is ready. No refresh required.

---

## 6. Image Behavior

**In the grid:**

- Static thumbnail, center-cropped to square.
- GIF files show the first frame only. They do not animate in the grid.

**In the viewer:**

- Full resolution image is loaded.
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
- The application content behind the viewer dims to approximately 85% opacity.
- The viewer opens with a fade-in animation (under 120ms).

**Layout:**

- Media is centered in the viewer area.
- Left and right navigation chevrons appear on hover over the left and right edges.
- A close button appears in the top-right corner.
- An info toggle button appears in the top-right area.
- Viewer fullscreen expands media within the viewer overlay and hides nonessential viewer chrome. The underlying header and sidebar are already covered by the viewer overlay.

**Navigation:**

- Left arrow key or left chevron: previous file in the current filtered set.
- Right arrow key or right chevron: next file in the current filtered set.
- When the viewer opens, it captures a snapshot of the current filtered, sorted media list. Navigation uses that snapshot until the viewer closes.
- Navigation respects the active filters and search query from the moment the viewer opened. It does not navigate through the entire library if a filter is active.
- If the current file becomes unavailable while the viewer is open, the viewer shows a file-unavailable message and keeps next/previous navigation functional.
- Navigation wraps: going past the last file returns to the first, and going before the first wraps to the last. Wrapping triggers a brief visual pulse/flash transition on the viewer in both directions to indicate that a wrap-around occurred.

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
- On close, the grid scrolls back to and highlights the cell that was open in the viewer. Scroll position is restored using the persisted scroll anchor. The highlight fades after 900ms.

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

**Selection mode:**

- Selection mode is active whenever one or more cells are selected.
- A contextual action bar slides up from the bottom of the screen when selection mode is active.
- The action bar shows: count of selected items, "Open file location" button, "Copy path(s)" button, "Deselect all" button.
- The "Open file location" button is disabled if the selection spans more than one physical folder, displaying a tooltip that explains they must be in the same folder to open their location.
- The "Copy path(s)" button copies the full paths of all selected items to the clipboard as a newline-separated list, without quotes.
- No destructive actions (rename, delete, move) exist in the action bar.
- Any filter, search, source-root availability, or sort change clears selection and exits selection mode.
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

- The active filter pill (acting as a "Clear all filters" button) displays a generic label summarizing the active filter status:
  - "● Search" when only search is active.
  - "● N tags" when only tags are active.
  - "● N tags + search" when both tags and search are active.
- Clicking the active filter pill instantly clears all search and tag filter criteria simultaneously.

**Filter persistence:**

- Active tag filters are persisted across sessions as part of session state.
- The active search query is NOT persisted and is always cleared on launch.
- The user reopens the application to the same tag-filtered state they left.

---

## 11. Sorting Behavior

Sorting is controlled by a dropdown in the top bar.

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
- Sort order changes are immediate — the grid reflows without a loading state.
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
- Unrecoverable application errors use this dialog only. Recoverable critical states may use banners or passive status surfaces.

---

## 13. First-Launch Experience

**Condition:** No source roots are currently configured.

**What the user sees:**

- The application opens to an empty state screen.
- Centered on screen: application name ("Vesper"), a one-line description ("Browse your media by your folder structure."), and a single prominent button: "Add Source Directory."
- The sidebar is shown normally with a "No tags available" placeholder. Settings and shortcut help remain active; search, sort, and zoom are disabled or inert until media exists.

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

## 15. Performance Expectations From a User Perspective

These are expected behaviors, not implementation targets.

**Reference environment for acceptance testing:**

- Linux desktop on GNOME/Wayland.
- 4 physical CPU cores or better.
- 8 GB RAM or better.
- SSD storage for the library and app cache.
- Warm start means the SQLite database already exists and thumbnails for visible initial items are cached.
- Cold start means the database exists but OS disk cache should not be assumed warm.
- HDD and network-mounted folders are supported only as best effort; they are not the baseline for timing acceptance.
- Test libraries should include a realistic mix of nested folders, duplicate folder names, JPEG/PNG/GIF/WEBP images, and common MP4/MOV/WebM videos.
- The 10,000-file add-root expectation includes mixed image/video discovery and thumbnail job scheduling, but not completion of every video thumbnail.

- The application opens and is interactive within 2 seconds on a standard Linux desktop with an existing library.
- The grid is scrollable without visible stutter at all zoom levels for libraries up to 50,000 files.
- Applying or removing a tag filter updates the grid within 200ms for libraries up to 50,000 files.
- Search results update within 150ms of each keystroke for libraries up to 50,000 files.
- Opening the viewer for an already-thumbnailed image takes under 300ms.
- Video playback begins within 1 second of opening the viewer for local files.
- Thumbnail generation does not block the UI. The grid remains scrollable and interactive during background indexing.
- Adding a new source root with 10,000 files begins showing thumbnails within 5 seconds of confirmation. Full indexing completes in the background.

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
- A user can open and close Settings and Keyboard Shortcuts, with focus returning to the invoking control or grid cell where practical.
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

HOVER (cursor over cell, activates after 150ms):
┌─────────────┐
│             │
│  thumbnail  │
│▓▓▓▓▓▓▓▓▓▓▓▓│  ← gradient rises from bottom
│🖼 filename… │  ← icon + name, truncated
└─────────────┘

FOCUSED (keyboard focus / Tab navigation):
┌─────────────┐  ← solid accent outline around card boundary (offset by 2px)
│             │
│  thumbnail  │
│▓▓▓▓▓▓▓▓▓▓▓▓│  ← overlay revealed (filename + icon) for keyboard-first parity
│🖼 filename… │
└─────────────┘

SELECTED (Ctrl+Click):
┌─────────────┐  ← accent border around perimeter
│✓            │  ← checkmark, top-left, accent circle bg
│  thumbnail  │  ← subtle dark tint
│  (tinted)   │
└─────────────┘

**Focus & Overlay parity:**
- **Overlay behavior:** The hover overlay (filename and icon) must be shown when a card has keyboard focus, matching hover behavior and ensuring accessibility for keyboard users.
- **Focus ring:** The focus ring must use a solid clear outline offset by 2px from the card edge, matching the border-radius of the card and avoiding collision/clipping with the selection border.
```

---

## 18. VIEWER OVERLAY STATES

```
VIEWER OPEN (single-click on cell):
┌─────────────────────────────────────────────────┐
│  [dimmed app content — 85% opacity]    [ℹ][✕]  │
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

Appears when ≥1 cell selected. Slides up from bottom over grid. Sidebar unaffected.

```
┌─────────────────────────────────────────────────┐
│  ✓ 3 selected  [Open Location] [Copy Path(s)] [Deselect all]  │
└─────────────────────────────────────────────────┘
```

No destructive actions (no delete, rename, move).

- **Open Location**: Opens the containing folder in the system file manager. Disabled if the selection contains items from more than one physical folder, showing a tooltip explaining: "Selected files must reside in the same folder."
- **Copy Path(s)**: Copies the full path of each selected item to the clipboard, formatted as a newline-separated list with no quotation marks.

**Accessibility:** buttons must expose labels such as `Open containing folder`, `Copy selected paths`, and `Deselect all`. Keyboard focus must be reachable without trapping focus in the bar.

---

## 20. EMPTY STATES

**No source roots configured:**

```
┌─────────────────────────────────┐
│  Vesper                    [⚙]  │  ← header, settings only
│─────────────────────────────────│
│                                 │
│           📁                    │
│ Browse your media by your       │
│ folder structure.               │
│                                 │
│   [ Add Source Directory ]      │
│                                 │
│ Press F1 or Ctrl+? for shortcuts│  ← small, subtle footer label
└─────────────────────────────────┘
Sidebar: Rendered always, even in the first-launch empty state. The tag list shows a "No tags available" placeholder, and the sources list shows as empty.
```

**Filters/search produce no results:**

```
Centered in grid area:
"No media matches the current filters."
[ Clear all filters ]  ← clears active tag filters and search query only
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
| `Ctrl+Click`    | Grid         | Add cell to selection          |
| `Shift+Click`   | Grid         | Range select                   |
| `F1` / `Ctrl+?` | Global       | Open Keyboard Shortcuts window |

**Keyboard Shortcuts Window:**

The Keyboard Shortcuts window is a modal dialog (`gtk::ShortcutWindow`) displaying a static two-column layout mapping keys to their actions. It is populated directly from the keyboard shortcut table above.

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
    │   └── gtk::Button "Restore Default Ignore Rules"
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
- **Rescan Library**: Refreshes source-root availability, ignore-rule results, media metadata, tag derivation, and deleted/new file records.
- **Regenerate Thumbnails**: Recreates thumbnails for modified or failed media in the background.
- **Rebuild Library Index**: Recreates database-derived records from configured source roots while preserving settings. It never modifies user media files.

**Settings behavior:**

- Adding a root begins background indexing immediately if the root is accepted.
- Adding an overlapping, duplicate, or nested root is rejected with a non-blocking message: "This folder is already covered by an existing source directory."
- Removing a root cancels active work for that root and removes its records from the library. Files on disk are untouched.
- Clicking "Restore Default Ignore Rules" updates the ignore-rules text field only. Changes apply and trigger rescan when settings are saved.
- Saving global ignore rules triggers a rescan of all online source roots.
- Toggling root-as-tag immediately re-derives all tags.
- Maintenance actions are non-blocking and report progress through indexing/scanning status surfaces.

---

## Cross-References

> See [Target User and Usage Model] in [01_Vision.md] for full spec.

> See [Source Directory Model] in [02_Architecture.md] for full spec.

> See [Ignore Rules] in [02_Architecture.md] for full spec.

> See [Tag Model and Tag Behavior] in [02_Architecture.md] for full spec.

> See [Session Persistence Behavior] in [02_Architecture.md] for full spec.

> See [Widget Tree] in [02_Architecture.md] for full spec.

> See [Sidebar Internal Layout] in [03_Implementation.md] for full spec.

> See [Header Bar Layout] in [03_Implementation.md] for full spec.

> See [State → UI Mapping] in [02_Architecture.md] for full spec.

> See [Optional Future / Taste Tradeoffs] in [01_Vision.md] for full spec.

> See [What Not To Do] in [03_Implementation.md] for full spec.
