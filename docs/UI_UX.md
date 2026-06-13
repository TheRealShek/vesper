# VESPER_UI_SPEC.md
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
├── PACK END (left→right as rendered right→left):
│   ├── gtk::Entry "Search media..."   [hexpand=true, search-icon]
│   ├── gtk::Button "● N tags"         ← active_filter_pill
│   │   visible=false when 0 tags active
│   │   visible=true, label="● N tags" when ≥1 tag active
│   │   click → clear all active tags
│   ├── [zoom slider widget]
│   ├── gtk::MenuButton "⋮"            ← view options popover
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
- Search always visible. Never hidden. Never icon-only.
- Filter pill `set_visible()` — not opacity. Must not leave gap when hidden.
- Sort dropdown removed from header. Lives only in `⋮` popover.
- No sidebar toggle button anywhere in header.

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
.cell-hover-overlay {
    background: linear-gradient(to top, rgba(0,0,0,0.75), transparent);
    transition: opacity 150ms ease;
    opacity: 0;
}
.cell-hover-overlay:hover {
    opacity: 1;
}
```

---

## 5. STATE → UI MAPPING

| State field         | Widget affected              | Behavior                              |
|---------------------|------------------------------|---------------------------------------|
| `selected_tags`     | `tag_list_box` rows          | Row gets `.active` CSS class          |
| `selected_tags.len` | `active_filter_pill`         | `set_visible(count > 0)`              |
| `selected_tags.len` | `match_mode_box`             | `set_visible(count > 0)`              |
| `tag_filter_mode`   | `match_any_radio/all_radio`  | Radio active state                    |
| `sort_order`        | Sort popover radio group     | Active radio reflects current sort    |
| `search_query`      | Search entry                 | NOT persisted — clears on launch      |
| `scroll_position`   | `grid_view`                  | Restored on launch                    |
| `zoom_level`        | Zoom slider                  | Restored on launch                    |

**Not persisted:** viewer open state, selection state, info panel state, search query.

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
Sidebar: NOT rendered until first source added.
```

**Filters/search produce no results:**
```
Centered in grid area:
"No media matches the current filters."
[ Clear all filters ]
```

---

## 10. KEYBOARD SHORTCUTS

| Key           | Context      | Action                        |
|---------------|--------------|-------------------------------|
| `Escape`      | Viewer open  | Close viewer                  |
| `Escape`      | Selection    | Deselect all, exit mode       |
| `Escape`      | Search focus | Clear search                  |
| `←` `→`       | Viewer       | Previous / next file          |
| `I`           | Viewer       | Toggle info panel             |
| `F`           | Viewer+video | Toggle fullscreen             |
| `Space`       | Viewer+video | Toggle play/pause             |
| `Enter`       | Grid focus   | Open viewer                   |
| `Ctrl+A`      | Grid         | Select all in filtered view   |
| `Ctrl+Click`  | Grid         | Add cell to selection         |
| `Shift+Click` | Grid         | Range select                  |

No `Ctrl+B` — sidebar toggle removed.

---

## 11. WHAT NOT TO DO (agent guard rails)

- Do NOT use `adw::OverlaySplitView` — wrong widget, implies toggleable sidebar.
- Do NOT use `GtkPaned` — sidebar is fixed, not resizable.
- Do NOT use `adw::ToolbarView` as sidebar root — breaks vexpand chain.
- Do NOT add `vexpand=true` to anything except `ScrolledWindow` in sidebar.
- Do NOT fake layout with CSS `margin` hacks — use proper widget hierarchy.
- Do NOT restore `Ctrl+B` keybinding.
- Do NOT add `sidebar_width` back to state — width managed by CSS only.
- Do NOT set `set_visible(false)` via opacity — use `set_visible()` so layout reflows.
