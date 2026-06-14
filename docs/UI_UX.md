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
├── Label "SOURCES"                 [margin-start=12]
├── Frame (.card)                   ← roots_frame
│   └── Box vertical                ← roots_list_box  (populated by window.rs)
│
└── [nothing else — no empty boxes, no second separator, no roots_box]
```

**Rules:**

- Only `ScrolledWindow` gets `vexpand=true`. Nothing else.
- `roots_list_box` populated externally from `window.rs` via `SidebarWidgets.roots_list_box`.
- `match_mode_box` toggled visible/invisible based on active tag count.

---

## 3. HEADER BAR LAYOUT

```
adw::HeaderBar
├── START: [nothing — title centered]
├── CENTER/TITLE: none (title-widget not set, app name shows)
├── PACK END widgets, added in this order with `pack_end()`:
│   ├── gtk::Button [⚙ settings]
│   ├── gtk::MenuButton "⋮"            ← view options popover
│   ├── [zoom slider widget]
│   ├── gtk::Button "● N tags"         ← active_filter_pill
│   └── gtk::SearchEntry "Search media..." [hexpand=true, search-icon]
│
├── VISUAL ORDER (left→right inside trailing header area):
│   ├── gtk::SearchEntry "Search media..." [hexpand=true, search-icon]
│   ├── gtk::Button filter summary      ← active_filter_pill
│   │   visible=false when no tags and no search are active
│   │   visible=true when tags and/or search are active
│   │   labels:
│   │     - "● Search" when only search is active
│   │     - "● N tags" when only tags are active
│   │     - "● N tags + search" when both are active
│   │   click → clear active tags and search query
│   ├── [zoom slider widget]
│   ├── gtk::MenuButton "⋮"            ← view options popover
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

- Search always visible whenever the header is visible. Never hidden. Never icon-only.
- Filter pill is a filter summary, not tag-only. It must reveal whenever any filtering is active, including search-only filtering.
- Filter pill `set_visible()` — not opacity. Must not leave gap when hidden.
- Clicking the filter pill clears all active filters represented by the pill: selected tags and search query.
- Sort dropdown removed from header. Lives only in `⋮` popover.
- No sidebar toggle button anywhere in header.
- Header controls must have accessible labels/tooltips: search, clear filters, zoom level, sort by, settings.

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

/* Grid cell hover overlay */
.card .cell-hover-overlay {
  background: linear-gradient(to top, rgba(0, 0, 0, 0.75), transparent);
  transition: opacity 150ms ease;
  opacity: 0;
}
gridview > child:hover > .card .cell-hover-overlay {
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

SELECTED (Ctrl+Click):
┌─────────────┐  ← accent border around perimeter
│✓            │  ← checkmark, top-left, accent circle bg
│  thumbnail  │  ← subtle dark tint
│  (tinted)   │
└─────────────┘
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
- Shortcut discoverability, such as a shortcuts window/overlay.

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
