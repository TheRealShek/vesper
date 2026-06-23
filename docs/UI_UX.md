# UI_UX.md

# UI/UX Implementation Reference — v1 Locked

---

## 1. WIDGET TREE (source of truth)

```
adw::ApplicationWindow
└── gtk::Box [horizontal, hexpand=true, vexpand=true]  ← main_box
    ├── gtk::Box [vertical, vexpand=true]               ← sidebar_root
    │   CSS: .sidebar-panel
    │   width: fixed 220px (min-width=max-width in CSS, no width_request in Rust)
    │   NO GtkPaned. NO toggle. NO collapse.
    │   NOTE: omitted entirely in the first-run empty state until a source exists.
    │
    └── adw::ToolbarView [hexpand=true, vexpand=true]   ← grid_toolbar_view
        CSS: .grid-area
        ├── TOP: adw::HeaderBar                         ← header_bar
        ├── TOP: adw::Banner                            ← offline_banner
        └── CONTENT: gtk::Stack                         ← root_stack
            ├── "empty"   → EmptyState widget
            ├── "no-results" → NoResults widget
            └── "grid"    → gtk::Overlay               ← main_overlay
                            ├── child: gtk::GridView   ← grid_view
                            ├── overlay: dim_bg
                            ├── overlay: viewer overlay
                            ├── overlay: action_bar_revealer
                            └── overlay: scan_error_button
```

---

## 2. SIDEBAR INTERNAL LAYOUT

```
sidebar_root (gtk::Box vertical, vexpand=true)
│   CSS class: sidebar-panel
│   background: #242424
│   border-right: 1px solid rgba(255,255,255,0.07)
│
├── Label "TAGS"                    [margin: top=16, start=12, bottom=4]
├── SearchEntry "Filter tags..."    [margin: start=12, end=12, bottom=6]
├── ScrolledWindow                  [vexpand=TRUE ← ONLY widget with vexpand]
│   └── Overlay
│       ├── child: gtk::Box vertical
│       │   ├── ListBox             ← tag_list_box (.navigation-sidebar)
│       │   └── Button "Show more"  ← hidden until >30 tags
│       └── overlay: Label "No tags available"  ← shown when 0 tags
│
├── Box horizontal "Match: ○Any ●All"  ← match_mode_box
│   visible=false by default
│   visible=true when ≥1 tag active
│
├── Separator horizontal            ← MUST be appended, styled in CSS:
│   .sidebar-panel separator {      background: rgba(255,255,255,0.12); min-height:1px }
│
├── Label "SOURCES"                 [margin: top=16, start=12, bottom=4]
├── Frame (.card)                   ← roots_frame
│   └── ListBox                     ← roots_list_box  (.navigation-sidebar, populated by window.rs)
│       └── Row (custom visual layout per root):
│           └── Box horizontal [spacing=8]
│               ├── Icon "folder-symbolic" (dimmed in offline state)
│               ├── Label [root name] (ellipsized, dimmed in offline state)
│               └── [Optional: Icon "network-offline-symbolic" or Label "(Offline)"]
│
└── [nothing else — no empty boxes, no second separator, no roots_box]
```

**Rules:**

- Only `ScrolledWindow` gets `vexpand=true`. Nothing else.
- `roots_list_box` populated externally from `window.rs` via `SidebarWidgets.roots_list_box` with custom horizontal rows representing folders and offline states.
- `match_mode_box` toggled visible/invisible based on active tag count.

---

## 3. HEADER BAR LAYOUT

```
adw::HeaderBar
├── START: [nothing — title centered]
├── CENTER/TITLE: none (title-widget not set, app name shows)
├── PACK END widgets, added in this order with `pack_end()`:
│   └── gtk::Box [horizontal, spacing=8]               ← controls_group
│       ├── gtk::SearchEntry "Search media..." [width-request=260, search-icon]
│       ├── gtk::Button filter summary                  ← active_filter_pill
│       ├── gtk::Box [horizontal, spacing=0] (.linked)   ← view_options_group
│       │   ├── [zoom slider widget]                    [width-request=100]
│       │   └── gtk::MenuButton "⋮"                     ← view options popover
│       └── gtk::Button [⚙ settings]
│
├── VISUAL ORDER (left→right inside trailing header area):
│   ├── gtk::SearchEntry "Search media..." [width-request=260, search-icon]
│   ├── gtk::Button filter summary      ← active_filter_pill
│   │   visible=false when no tags and no search are active
│   │   visible=true when tags and/or search are active
│   │   labels:
│   │     - "● Search" when only search is active
│   │     - "● N tags" when only tags are active
│   │     - "● N tags + search" when both are active
│   │   click → clear active tags and search query
│   ├── [zoom slider widget]            
│   ├── gtk::MenuButton "⋮"             
│   │   tooltip="Sort by"               
│   │   └── GtkPopover
│   │       └── Box vertical "Sort by"
│   │           └── CheckButton group (radio):
│   │               ● Date modified ↓  (default)
│   │               ○ Date modified ↑
│   │               ○ Date created ↓
│   │               ○ Date created ↑
│   │               ○ Filename A→Z
│   │               ○ Filename Z→A
│   │               ○ File size ↓
│   │               ○ File size ↑
│   └── gtk::Button [⚙ settings]
```

**Rules:**

- **Visual Hierarchy & Title Alignment:** Group related header controls to establish a clean and logical visual hierarchy. To prevent controls from squishing the centered window title, prioritize packing structure and keep search-bar expansion bounded.
  - The search box must remain visible and not collapse to an icon. It should be constrained to a reference width-request of 260px.
  - View configuration controls (the zoom slider and sorting popover button) must be grouped together inside a `.linked` container to represent a single "view options" visual unit.
- **Filter Pill Summary:** The filter pill acts as a global filter status indicator. It must become visible whenever tag filters or search filters are active. It must use `set_visible(true/false)` rather than opacity to prevent layout gaps, and clicking it must clear all search and tag filter criteria.
- **Control Placement & Hygiene:**
  - The sort dropdown exists only within the view options popover, not as a separate header button.
  - The header must not include a sidebar toggle button or collapse controls.
  - All header widgets must expose standard accessibility labels and tooltips.

---

## 4. CSS RULES (critical)

```css
/* Sidebar surface — elevated above grid */
.sidebar-panel {
  min-width: 220px;
  max-width: 220px;
  background-color: #242424;
  border-right: 1px solid rgba(255, 255, 255, 0.07);
}

/* Grid surface — deeper */
.grid-area {
  background-color: #181818;
}

/* 
 * Spacing Goal: Ensure the grid has a comfortable visual density with breathing room
 * between media thumbnails, facilitating easier horizontal scanning and visual grouping.
 * Card Margin Goal: Prevent clipping of card border-radius, drop-shadows, and focus states
 * by enforcing margins inside cell bounds.
 * 
 * Reference values:
 */
gridview {
  border-spacing: 16px;
}
gridview > child > .card {
  margin: 4px;
  border-radius: 12px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
}

/* Separator inside sidebar — visible on dark bg */
.sidebar-panel separator {
  background-color: rgba(255, 255, 255, 0.12);
  min-height: 1px;
  margin-top: 4px;
  margin-bottom: 4px;
}

/* Filter pill in header */
.filter-pill {
  background-color: @accent_bg_color;
  border-radius: 999px;
  padding: 0 10px;
}

/* Interactive Tag Chips in Sidebar */
row.tag-chip {
  background-color: rgba(255, 255, 255, 0.03);
  border: 1px solid rgba(255, 255, 255, 0.15);
  border-radius: 99px;
  padding: 4px 12px;
  margin-bottom: 6px;
  transition: all 150ms ease-in-out;
}
row.tag-chip:hover:not(.active) {
  background-color: rgba(255, 255, 255, 0.08);
  border-color: rgba(255, 255, 255, 0.3);
}
row.tag-chip.active {
  background-color: @accent_bg_color;
  color: @accent_fg_color;
  border-color: transparent;
}
row.tag-chip.active:hover {
  background-color: alpha(@accent_bg_color, 0.95);
}

/* Source Root Items in Sidebar */
.sources-card row {
  padding: 8px 12px;
  min-height: 36px;
  border-radius: 6px;
  transition: background-color 150ms ease;
}
.sources-card row:hover {
  background-color: rgba(255, 255, 255, 0.04);
}
.sources-card image {
  margin-right: 8px;
  opacity: 0.7;
}
.sources-card row.offline {
  opacity: 0.55;
}

/* Grid cell focus outline (keyboard focus parity) */
gridview > child:focus-within > .card {
  outline: 2px solid alpha(@accent_color, 0.65);
  outline-offset: 2px;
}

/* Grid cell hover overlay - visible on hover or keyboard focus */
.card .cell-hover-overlay {
  background: linear-gradient(to top, rgba(0, 0, 0, 0.75), transparent);
  transition: opacity 150ms ease;
  opacity: 0;
}
gridview > child:hover > .card .cell-hover-overlay,
gridview > child:focus-within > .card .cell-hover-overlay {
  opacity: 1;
}
```

---

## 5. STATE → UI MAPPING

| State field                          | Widget affected             | Behavior                                                               |
| ------------------------------------ | --------------------------- | ---------------------------------------------------------------------- |
| `selected_tags`                      | `tag_list_box` rows         | Row gets `.active` CSS class                                           |
| `selected_tags.len` + `search_query` | `active_filter_pill`        | `set_visible(has_tags or has_search)`; label summarizes active filters |
| `selected_tags.len`                  | `match_mode_box`            | `set_visible(count > 0)`                                               |
| `tag_filter_mode`                    | `match_any_radio/all_radio` | Radio active state                                                     |
| `sort_order`                         | Sort popover radio group    | Active radio reflects current sort                                     |
| `search_query`                       | Search entry                | NOT persisted — clears on launch                                       |
| `scroll_position`                    | `grid_view`                 | Restored on launch                                                     |
| `zoom_level`                         | Zoom slider                 | Restored on launch                                                     |

**Not persisted:** viewer open state, selection state, info panel state, search query.

**Derived UI only:** active filter pill label, no-results stack page, action bar visibility, scan/indexing status visibility, and match mode visibility are recalculated from current in-memory state and are not stored independently.

---

## 6. GRID CELL STATES

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

## 7. VIEWER OVERLAY STATES

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

## 8. SELECTION ACTION BAR

Appears when ≥1 cell selected. Slides up from bottom over grid. Sidebar unaffected.

```
┌─────────────────────────────────────────────────┐
│  ✓ 3 selected  [Open Location] [Copy Path(s)] [Deselect all]  │
└─────────────────────────────────────────────────┘
```

No destructive actions (no delete, rename, move).

**Accessibility:** buttons must expose labels such as `Open containing folder`, `Copy selected paths`, and `Deselect all`. Keyboard focus must be reachable without trapping focus in the bar.

---

## 9. EMPTY STATES

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

## 10. INDEXING / SCANNING STATE

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

## 11. KEYBOARD SHORTCUTS

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

## 12. ACCESSIBILITY AND FOCUS

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

## 13. OPTIONAL FUTURE / TASTE TRADEOFFS

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

## 14. WHAT NOT TO DO (agent guard rails)

- Do NOT use `adw::OverlaySplitView` — wrong widget, implies toggleable sidebar.
- Do NOT use `GtkPaned` — sidebar is fixed, not resizable.
- Do NOT use `adw::ToolbarView` as sidebar root — breaks vexpand chain.
- Do NOT add `vexpand=true` to anything except `ScrolledWindow` in sidebar.
- Do NOT fake layout with CSS `margin` hacks — use proper widget hierarchy.
- Do NOT restore `Ctrl+B` keybinding.
- Do NOT add `sidebar_width` back to state — width managed by CSS only.
- Do NOT set `set_visible(false)` via opacity — use `set_visible()` so layout reflows.
- Do NOT make the first-run empty state keep an invisible/sidebar placeholder; omit the sidebar entirely until a source exists.
- Do NOT attach hover reveal only to `.cell-hover-overlay:hover`; reveal from `gridview > child:hover` so the overlay appears when hovering any part of the card.
- Do NOT hide the filter pill when search is active; search is a filter and must be visible in the filter summary.
- Do NOT show modal progress dialogs for indexing/scanning. Scanning feedback must be non-blocking.
- Do NOT add recent/folders sidebar sections or otherwise restructure the v1 folder-derived tag navigation model.
