# Product

---

## 1. Search Behavior

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

**Sidebar:** Contains the tag list and source root indicators. Collapsible. Width is user-adjustable and persisted across sessions.

**Grid:** The main content area. Fills all remaining space. Scrolls vertically. Never paginates — it is one continuous scrollable surface.

There is no navigation history, no back button, no breadcrumb. The application is always showing one unified library with filters applied.

---

## 3. Sidebar Behavior

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
- If filters or search produce no results: a centered message stating no media matches the current filters. A button clears all active filters.

**Loading state:**

- On first launch or after adding a new source root, thumbnails appear progressively as they are generated. Cells show a placeholder until their thumbnail is ready. The grid is usable immediately — the user does not wait for indexing to complete.

---

## 5. Grid Cell Specification

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
- At 100% zoom, the image can be panned freely.
- Zoom resets to fit when navigating to a different file.

---

## 7. Video Behavior

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

## 8. Viewer Overlay Behavior

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

## 9. Selection and Multi-Selection Behavior

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

## 10. Filtering Behavior

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

## 11. Sorting Behavior

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

## 12. Error Handling From a User Perspective

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

## 13. First-Launch Experience

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

## 14. Canonical Browsing Flow

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

## 15. Performance Expectations From a User Perspective

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

## 16. Accessibility and Keyboard Behavior

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

## 17. GRID CELL STATES

```
DEFAULT (no interaction):
┌─────────────┐
│             │
│  thumbnail  │  ← center-cropped, fills cell
│  (square)   │
│         🕐  │  ← duration badge, video only, bottom-right
└─────────────┘

HOVER (cursor over cell, activates after 150ms):
┌─────────────┐
│             │
│  thumbnail  │
│▓▓▓▓▓▓▓▓▓▓▓▓│  ← gradient rises from bottom
│🖼 filename… │  ← icon + name, truncated
└─────────────┘

FOCUSED (keyboard focus / Tab navigation):
┌ ─ ─ ─ ─ ─ ─ ┐  ← dashed accent outline around card boundary (offset by 2px)
│             │
│  thumbnail  │
│▓▓▓▓▓▓▓▓▓▓▓▓│  ← overlay revealed (filename + icon) for keyboard-first parity
│🖼 filename… │
└ ─ ─ ─ ─ ─ ─ ┘

SELECTED (Ctrl+Click):
┌─────────────┐  ← accent border around perimeter
│✓            │  ← checkmark, top-left, accent circle bg
│  thumbnail  │  ← subtle dark tint
│  (tinted)   │
└─────────────┘

**Focus & Overlay parity:**
- **Overlay behavior:** The hover overlay (filename and icon) must be shown when a card has keyboard focus, matching hover behavior and ensuring accessibility for keyboard users.
- **Focus ring:** The focus ring must use a clear outline offset by 2px from the card edge, matching the border-radius of the card and avoiding collision/clipping with the selection border.
```

---

## 18. VIEWER OVERLAY STATES

```
VIEWER OPEN (single-click on cell):
┌─────────────────────────────────────────────────┐
│  [dimmed grid behind — 85% opacity]    [ℹ][✕]  │
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
Info panel slides in from right. Does not push media.
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
│     Browse your media           │
│   by your folder structure.     │
│                                 │
│   [ Add Source Directory ]      │
│                                 │
│ Press F1 or Ctrl+? for shortcuts│  ← small, subtle footer label
└─────────────────────────────────┘
Sidebar: NOT rendered until first source added. This is the only exception to the main widget tree above; after at least one source exists, the sidebar is always present and fixed-width.
```

**Filters/search produce no results:**

```
Centered in grid area:
"No media matches the current filters."
[ Clear all filters ]
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
- in the grid empty/content area
- in the sidebar footer/source area

**Do not:**

- block the UI thread
- show a modal progress dialog
- prevent browsing already indexed items
- fake progress percentages when total work is unknown

---

## 22. KEYBOARD SHORTCUTS

| Key           | Context      | Action                      |
| ------------- | ------------ | --------------------------- |
| `Escape`      | Viewer open  | Close viewer                |
| `Escape`      | Selection    | Deselect all, exit mode     |
| `Escape`      | Search focus | Clear search                |
| `←` `→`       | Viewer       | Previous / next file        |
| `I`           | Viewer       | Toggle info panel           |
| `F`           | Viewer+video | Toggle fullscreen           |
| `Space`       | Viewer+video | Toggle play/pause           |
| `Enter`       | Grid focus   | Open viewer                 |
| `Ctrl+A`      | Grid         | Select all in filtered view |
| `Ctrl+Click`  | Grid         | Add cell to selection       |
| `Shift+Click` | Grid         | Range select                |
| `F1` / `Ctrl+?` | Global       | Open Keyboard Shortcuts window |

No `Ctrl+B` — sidebar toggle removed.

**Shortcut precedence:**

- Grid keyboard navigation must remain active when no text entry is focused.
- Viewer shortcuts must take precedence only while the viewer is open.

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
