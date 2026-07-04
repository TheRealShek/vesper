# Tech Implementation

---

## 1. SIDEBAR INTERNAL LAYOUT

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

## 2. HEADER BAR LAYOUT

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

## 3. CSS RULES (critical)

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

## 4. WHAT NOT TO DO (agent guard rails)

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

---

## Cross-References

> See [Source Directory Model] in [02_Architecture.md] for full spec.

> See [Ignore Rules] in [02_Architecture.md] for full spec.

> See [Tag Model and Tag Behavior] in [02_Architecture.md] for full spec.

> See [Search Behavior] in [04_Product_Spec.md] for full spec.

> See [Grid View Behavior] in [04_Product_Spec.md] for full spec.

> See [Session Persistence Behavior] in [02_Architecture.md] for full spec.

> See [Performance Expectations] in [04_Product_Spec.md] for full spec.

> See [Explicitly Accepted Constraints] in [01_Vision.md] for full spec.

> See [Widget Tree] in [02_Architecture.md] for full spec.

> See [State → UI Mapping] in [02_Architecture.md] for full spec.

> See [Grid Cell States] in [04_Product_Spec.md] for full spec.

> See [Indexing / Scanning State] in [04_Product_Spec.md] for full spec.

> See [Accessibility and Focus] in [04_Product_Spec.md] for full spec.
