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
│   visible=true when ≥2 tags active
│
├── Separator horizontal            ← MUST be appended, styled in CSS:
│   .sidebar-panel separator {      background: rgba(255,255,255,0.12); min-height:1px }
│
├── Label "SOURCES"                 [margin: top=16, start=12, bottom=4]
├── Frame (.sources-card)           ← roots_frame
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
- `match_mode_box` toggled visible only when two or more tags are active.
- The sidebar tag-list search entry filters tags only. It does not filter the media grid.
- The tag-list search searches all tags, including tags currently hidden behind the "Show more" collapsed limit.
- "Show more" expands for the current session and changes to "Show less"; this expansion state is not persisted.
- Tag rows use the short display name as primary text and use secondary text or tooltip for breadcrumb disambiguation when names collide.

---

## 2. HEADER BAR LAYOUT

```
adw::HeaderBar
├── START: [nothing — title centered]
├── CENTER/TITLE: adw::WindowTitle "Vesper"
├── PACK END widgets, added in this order with `pack_end()`:
│   └── gtk::Box [horizontal, spacing=8]               ← controls_group
│       ├── gtk::SearchEntry "Search media..." [width-request=260, search-icon]
│       ├── gtk::Button filter summary                  ← active_filter_pill
│       ├── gtk::Box [horizontal, spacing=0] (.linked)   ← view_options_group
│       │   ├── [zoom slider widget]                    [width-request=100, 5 tick marks with labels (XS, S, M, L, XL)]
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
│   │               ○ Date added ↓
│   │               ○ Date added ↑
│   │               ○ Filename A→Z
│   │               ○ Filename Z→A
│   │               ○ File size ↓
│   │               ○ File size ↑
│   └── gtk::Button [⚙ settings]
```

**Rules:**

- **Visual Hierarchy & Title Alignment:** Group related header controls to establish a clean and logical visual hierarchy. To prevent controls from squishing the centered window title, prioritize packing structure and keep search-bar expansion bounded.
  - The search box must remain visible and not collapse to an icon. It should be constrained to a reference width-request of 260px.
  - View configuration controls (the zoom slider and sorting popover button) must be grouped together inside a `.linked` container to represent a single "view options" visual unit. GtkScale does not visually join like GtkButton/GtkEntry — add custom CSS to flatten scale trough/slider borders so it reads as one linked unit with the MenuButton.
- **Title:** Use an explicit `adw::WindowTitle` with title `Vesper`. Do not rely on implicit application-name rendering.
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
  border-radius: 8px;
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

/* Grid cell selected state */
gridview > child.selected > .card {
  border: 2px solid @accent_bg_color;
}
gridview > child.selected > .card .selected-tint {
  background-color: rgba(0, 0, 0, 0.22);
  opacity: 1;
}
```

Grid cell templates must include a `.selected-tint` overlay above the image and below badges/text so selected state reads as a tint instead of only lowering image opacity.

---

## 4. TOP-LEVEL OVERLAY PLACEMENT

The Architecture widget tree is the source of truth for overlay scope:

```text
ApplicationWindow
└── gtk::Overlay app_overlay
    ├── child: main_box
    └── overlay: viewer_overlay
```

Implementation rules:

- Mount `viewer_overlay` on `app_overlay`, not inside the grid-only overlay.
- `viewer_overlay` covers header, sidebar, and grid while open.
- Opening the viewer clears selection and hides the selection action bar.
- Keep `action_bar_revealer` inside the grid overlay so selection controls remain grid-scoped.
- Keep `scan_error_button` inside the grid overlay; it appears at the bottom-left of the grid area, not the entire application window.
- Use `status_banner_stack` below the header for offline-root and indexing status. Do not create separate competing banners for those states.

---

## 5. SETTINGS DIALOG LAYOUT

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

**Rules:**

- Global ignore rules use a multi-line text field, one pattern per line.
- Clicking "Restore Default Ignore Rules" appends missing default rules to `global_ignore_text_view` only. It does not persist or rescan immediately.
- Saving global ignore rules triggers the architecture-defined rescan flow.
- Toggling root-as-tag immediately re-derives tags.
- Maintenance buttons schedule background work and never show modal progress dialogs.

---

## 6. MEDIA BACKEND ASSUMPTIONS

- UI playback uses GTK's media stack: `gtk::MediaFile` / `gtk::MediaStream` rendered through GTK widgets.
- Runtime video playback depends on the platform GTK/GStreamer media backend and installed codec plugins.
- Grid video thumbnail extraction uses external `ffmpeg`.
- Video duration probing uses external `ffprobe`.
- Image thumbnail extraction uses the Rust `image` crate where supported by the current pipeline.
- Missing `ffmpeg` or `ffprobe` must not prevent the app from launching. Affected videos show placeholders and omit duration badges.
- Missing GStreamer codecs/plugins must surface as an in-viewer playback error while preserving next/previous navigation.
- Video duration probing and thumbnail extraction must tolerate unsupported/corrupt files.
- If duration probing fails, show no duration badge.
- If thumbnail extraction fails, show the stable media-type placeholder.
- Playback failure is shown inside the viewer and must not close the viewer or block next/previous navigation.

---

## 7. PACKAGING AND PLATFORM ASSUMPTIONS

v1 targets native Linux packaging first.

**Native package baseline:**

- Debian packaging metadata is the current reference package target.
- Runtime package dependencies must include GTK4/libadwaita and `ffmpeg`.
- The app runs as a normal local desktop process with direct filesystem access to user-selected source roots.
- GNOME/Wayland is the primary desktop target. X11 may work but is not the product baseline.
- Window position restore is not implemented on Wayland.

**Flatpak status:**

- Flatpak is not a v1 support target unless a dedicated packaging pass adds portal-aware source-directory access.
- If Flatpak support is added later, source-root selection must use portals and persist granted folder permissions across launches.
- Flatpak builds must not assume unrestricted host filesystem access.

---

## 8. ACCESSIBILITY IMPLEMENTATION CHECKLIST

Run this checklist before considering a UI change complete:

- Header controls are reachable with `Tab` / `Shift+Tab`.
- Sidebar tag filter, tag rows, match-mode controls, and source-root rows are reachable with keyboard navigation.
- Grid cells expose accessible labels including filename and media type.
- Focused grid cells show the same filename/type overlay as hover.
- Viewer controls expose accessible labels: previous, next, play/pause, mute, fullscreen, info, close.
- Settings rows and icon-only buttons expose accessible labels.
- Closing viewer, Settings, and Keyboard Shortcuts returns focus to the invoking cell/control where practical.
- Selection action bar controls are keyboard reachable and do not trap focus.
- Indexing, offline-root, and scan-error states expose text, not only icons or color.
- High-contrast mode keeps focus rings, selected state, and status text visible.

---

## 9. WHAT NOT TO DO (agent guard rails)

- Do NOT use `adw::OverlaySplitView` — wrong widget, implies toggleable sidebar.
- Do NOT use `GtkPaned` — sidebar is fixed, not resizable.
- Do NOT use `adw::ToolbarView` as sidebar root — breaks vexpand chain.
- Do NOT add `vexpand=true` to anything except `ScrolledWindow` in sidebar.
- Do NOT fake layout with CSS `margin` hacks — use proper widget hierarchy.
- Do NOT restore `Ctrl+B` keybinding.
- Do NOT add `sidebar_width` back to state — width managed by CSS only.
- Do NOT add sidebar collapsed state back to session state.
- Do NOT set `set_visible(false)` via opacity — use `set_visible()` so layout reflows.
- Do NOT hide the sidebar in the first-run empty state; the sidebar must remain visible at its fixed width.
- Do NOT mount the viewer overlay inside the grid overlay; it must cover the full application content area.
- Do NOT show offline media as dimmed grid cells in v1; offline-root media is hidden from grid/search/selection/viewer/tag counts.
- Do NOT attach hover reveal only to `.cell-hover-overlay:hover`; reveal from `gridview > child:hover` so the overlay appears when hovering any part of the card.
- Do NOT hide the filter pill when search is active; search is a filter and must be visible in the filter summary.
- Do NOT show modal progress dialogs for indexing/scanning. Scanning feedback must be non-blocking.
- Do NOT add recent/folders sidebar sections or otherwise restructure the v1 folder-derived tag navigation model.
- Do NOT implement rubber-band drag selection for v1 unless Product Spec is updated to include it.
- Do NOT claim Flatpak support until portal-based source-root persistence is implemented and tested.
- Do NOT make `ffmpeg`/`ffprobe` failures fatal to app startup.

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
