# Tech Implementation

---

## 0. Implementation Contract

This document fixes the GTK/libadwaita structure and the performance guard rails needed to realize [04_Product_Spec.md](04_Product_Spec.md). [05_Visual_Design.md](05_Visual_Design.md) owns visual styling and overrides older CSS examples. Widget names are stable integration points between UI modules and tests. Reference sizes and durations become app-wide constants in `src/config.rs` when used from Rust; do not duplicate numeric literals across modules.

## 1. SIDEBAR INTERNAL LAYOUT

```
sidebar_root (gtk::Box vertical, vexpand=true, hexpand=false)
│   CSS class: sidebar-panel
│   width: 220px via CSS minimum + no horizontal expansion
│
├── Label "Tags"                    [margin: top=16, start=12, bottom=4]
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
│   .sidebar-panel separator { background: alpha(@window_fg_color,0.12); min-height:1px }
│
├── Label "Sources"                 [margin: top=16, start=12, bottom=4]
├── ListBox                         ← roots_list_box (.navigation-sidebar, populated by window.rs)
│   └── Row (flat visual layout per root):
│       └── Box horizontal [spacing=8]
│           ├── Icon "folder-symbolic"
│           ├── Label [root name] (ellipsized)
│           └── Label "Offline" [only while offline]
│
└── [nothing else — no empty boxes, no second separator, no roots_box]
```

**Rules:**

- `sidebar_root` expands vertically as a top-level column. Among its children, only the tag `ScrolledWindow` gets `vexpand=true`; no other sidebar child may consume surplus height.
- `roots_list_box` populated externally from `window.rs` via `SidebarWidgets.roots_list_box` with custom horizontal rows representing folders and offline states.
- `match_mode_box` toggled visible only when two or more tags are active.
- The sidebar tag-list search entry filters tags only. It does not filter the media grid.
- The tag-list search searches all tags, including tags currently hidden behind the "Show more" collapsed limit.
- While the tag-list search contains non-whitespace text, show every matching tag and hide the "Show more/less" control. Clearing the query restores the prior session-only expanded/collapsed state.
- "Show more" expands for the current session and changes to "Show less"; this expansion state is not persisted.
- Tag rows use a flat navigation-row layout: a 3px active indicator, short display name, optional lineage secondary text only for collisions, and a trailing count. Do not implement tags as chips/pills.
- Source rows are status/navigation information, not cards. Offline state uses readable `Offline` text rather than reducing opacity for the entire row.

---

## 2. HEADER BAR LAYOUT

```
adw::HeaderBar
├── START: adw::WindowTitle "Vesper"
├── CENTER/TITLE: adw::Clamp [maximum-size=360, tightening-threshold=280]
│   └── gtk::SearchEntry "Search media..." [hexpand=true]
└── END: gtk::Box [horizontal, spacing=8]              ← controls_group (pack once)
    ├── gtk::Button "Clear filters (N)"               ← clear_filters_button
    │   visible=false when no tags and no search are active
    │   visible=true when tags and/or search are active
    │   N = active tag count + 1 when search is active
    │   click → clear active tags and search query
    ├── gtk::Scale [width-request=96, 5 detents]       ← zoom_slider
    ├── gtk::MenuButton "Sort ▾"                       ← sort_menu_btn
    │   tooltip="Sort media"
    │   └── GtkPopover
    │       └── Box vertical "Sort by"
    │           └── CheckButton group (radio):
    │               ● Date modified ↓  (default)
    │               ○ Date modified ↑
    │               ○ Date added ↓
    │               ○ Date added ↑
    │               ○ Filename A→Z
    │               ○ Filename Z→A
    │               ○ File size ↓
    │               ○ File size ↑
    └── gtk::Button [settings symbolic icon]           ← settings_btn
```

**Rules:**

- **Visual hierarchy:** Use the Visual Design header composition. The title anchors the start, search owns the center, and infrequent view controls stay at the end.
- **Search:** The search box must remain visible and never collapse to an icon. Give it a 280px natural width and allow growth to 360px; the 960px minimum window width prevents destructive compression.
- **Title:** Use an explicit `adw::WindowTitle` with title `Vesper` at the start. Do not rely on implicit application-name rendering.
- **GTK placement:** `pack_start()` the `adw::WindowTitle`, set the Clamp as `header_bar.title_widget`, and `pack_end()` `controls_group` once.
- **Packing order:** Pack `controls_group` into the header once, then append its children in the documented visual order. Do not call `pack_end()` separately for every control because GTK end-packing reversals make the result order-dependent.
- **Clear-filters button:** `clear_filters_button` is a neutral labeled button, never a pill or `suggested-action`. It becomes visible whenever tags or search are active, uses `set_visible()`, and clears both dimensions.
- **Thumbnail size:** Use a five-detent 96px scale with no zoom icons and no printed `XS–XL` labels. Expose the current name through accessible value text and tooltip.
- **Control Placement & Hygiene:**
  - Sort uses a visible text label and disclosure arrow, not a vertical-ellipsis icon.
  - Do not wrap size and Sort in `.linked`; proximity is enough and the actions are not one compound control.
  - The header must not include a sidebar toggle button or collapse controls.
  - All header widgets must expose standard accessibility labels and tooltips.

---

## 3. CSS RULES (critical)

The complete visual contract is [05_Visual_Design.md](05_Visual_Design.md). Keep `style.css` small: it should bridge GTK widgets to that contract, not invent a second theme.

```css
.sidebar-panel {
  min-width: 220px;
  background-color: @headerbar_bg_color;
  border-right: 1px solid alpha(@window_fg_color, 0.12);
}

gridview {
  border-spacing: 4px; /* with 4px cell margins, visible media-to-media gap is 12px */
  background-color: @view_bg_color;
}

gridview > child > .media-cell {
  margin: 4px;
  border-radius: 6px;
  border: 2px solid transparent;
}

.sidebar-panel row.tag-row {
  border-left: 3px solid transparent; /* reserves alignment in inactive state */
}

.sidebar-panel row.tag-row.active {
  background-color: alpha(@accent_color, 0.14);
  border-left: 3px solid @accent_color;
}

gridview > child:focus-within > .media-cell {
  outline: 2px solid @accent_color;
  outline-offset: 2px;
}

.media-cell .cell-hover-overlay {
  background: linear-gradient(to top, rgba(0, 0, 0, 0.82), transparent 60%);
  transition: opacity 120ms ease;
  opacity: 0;
}

gridview > child:hover > .media-cell .cell-hover-overlay,
gridview > child:focus-within > .media-cell .cell-hover-overlay {
  opacity: 1;
}

gridview > child:selected > .media-cell {
  border: 2px solid @accent_bg_color;
}

gridview > child:selected > .media-cell .selected-tint {
  background-color: rgba(0, 0, 0, 0.12);
  opacity: 1;
}

.viewer-bg { background-color: rgba(0, 0, 0, 0.92); }
.viewer-bg.fullscreen { background-color: black; }
```

Grid cell templates include `.selected-tint` above the image and below badges/text. Never implement selection by lowering `GtkPicture` opacity. Do not redefine accent colors, hard-code application surfaces, use `transition: all`, add shimmer, or add grid-cell shadows.

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
    │   ├── gtk::Button "Restore Default Ignore Rules"
    │   └── gtk::Button "Apply Ignore Rules" [suggested-action, sensitive when dirty]
    └── adw::PreferencesGroup [title="Library Maintenance"]
        ├── gtk::Button "Rescan Library"
        ├── gtk::Button "Regenerate Thumbnails"
        └── gtk::Button "Rebuild Library Index"
```

**Rules:**

- Global ignore rules use a multi-line text field, one pattern per line.
- Clicking "Restore Default Ignore Rules" appends missing default rules to `global_ignore_text_view` only. It does not persist or rescan immediately.
- Editing the field marks it dirty. "Apply Ignore Rules" validates the entire field; on success it persists the rules, clears dirty state, and triggers the architecture-defined rescan. On failure it keeps the previous saved rules active and identifies the first invalid line inline.
- Closing Settings with unapplied ignore-rule edits discards those edits. Source-root and root-as-tag changes are immediate and are not rolled back.
- Toggling root-as-tag immediately enqueues one generation-based re-derivation; publish the completed tags/counts as a batch.
- A source-root remove button opens the Product-defined confirmation and passes the stable root id, not a list-row index, to the backend after confirmation.
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
- Full-size image reads and decode run off the GTK thread. The viewer shows a loading surface immediately, then installs the decoded texture on the GTK thread only if its viewer generation is still current.

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
- Closing viewer, Settings, and Keyboard Shortcuts returns focus to the invoking cell/control. If that widget was removed, focus the grid; if the grid is empty, focus the first enabled header control.
- Selection action bar controls are keyboard reachable and do not trap focus.
- Indexing, offline-root, and scan-error states expose text, not only icons or color.
- High-contrast mode keeps focus rings, selected state, and status text visible.

---

## 9. Responsiveness Implementation Checklist

- `gtk::ListItemFactory::setup` creates reusable widgets. `bind` only assigns already-available summary data and cached/placeholder textures; it performs no filesystem, database, probing, or decode work. `unbind` disconnects handlers and cancels cell-owned requests.
- Keep only visible and approximately one viewport of near-visible thumbnail requests at high priority. Recalculate priority after scroll/zoom changes and deduplicate by cache key.
- Decode/scale image bytes in bounded workers. Restrict GTK object creation and widget mutation to the GTK thread, and apply those mutations in idle/frame-sized batches.
- Do not replace the complete `gio::ListStore` for a single watcher delta. Apply query-aware insert/remove/update deltas, or issue one superseding query when ordering/filter membership may have changed.
- A hydration/fetch event is read-only. Filesystem liveness, watcher configuration, scans, and database writes are independent backend jobs.
- Coalesce scan counters, tag counts, and status text before crossing into UI code. Publish at most one progress refresh every 100ms; final completion/error events bypass this throttle.
- Search/filter/sort handlers enqueue a generation-tagged request and return immediately. They never wait for SQLite or rebuild 50,000 rows synchronously.
- Clipboard path construction and external file-manager launching happen outside input callbacks. Completion/failure returns through typed events and produces user-visible feedback.
- CSS transitions follow Visual Design: use opacity for hover/viewer state and native revealers for panels; never use `transition: all`, animate layout during grid scrolling, or animate thousands of cells at once.
- Verify the Product performance budgets with release builds and a 50,000-item fixture; debug-build feel is not an acceptance measurement.

---

## 10. WHAT NOT TO DO (agent guard rails)

- Do NOT use `adw::OverlaySplitView` — wrong widget, implies toggleable sidebar.
- Do NOT use `GtkPaned` — sidebar is fixed, not resizable.
- Do NOT use `adw::ToolbarView` as sidebar root — breaks vexpand chain.
- Do NOT add `vexpand=true` to sidebar children other than the tag `ScrolledWindow`; `sidebar_root` itself remains vertically expanding as a top-level column.
- Do NOT fake layout with CSS `margin` hacks — use proper widget hierarchy.
- Do NOT restore `Ctrl+B` keybinding.
- Do NOT add `sidebar_width` back to state — width managed by CSS only.
- Do NOT add sidebar collapsed state back to session state.
- Do NOT set `set_visible(false)` via opacity — use `set_visible()` so layout reflows.
- Do NOT hide the sidebar in the first-run empty state; the sidebar must remain visible at its fixed width.
- Do NOT mount the viewer overlay inside the grid overlay; it must cover the full application content area.
- Do NOT show offline media as dimmed grid cells in v1; offline-root media is hidden from grid/search/selection/viewer/tag counts.
- Do NOT attach hover reveal only to `.cell-hover-overlay:hover`; reveal from `gridview > child:hover` so the overlay appears when hovering any part of the card.
- Do NOT hide `clear_filters_button` when search is active; search counts as one active filter dimension.
- Do NOT show modal progress dialogs for indexing/scanning. Scanning feedback must be non-blocking.
- Do NOT add recent/folders sidebar sections or otherwise restructure the v1 folder-derived tag navigation model.
- Do NOT implement rubber-band drag selection for v1 unless Product Spec is updated to include it.
- Do NOT claim Flatpak support until portal-based source-root persistence is implemented and tested.
- Do NOT make `ffmpeg`/`ffprobe` failures fatal to app startup.
- Do NOT perform root-liveness checks, watcher setup, scans, or database writes as side effects of UI hydration.
- Do NOT publish one GTK model mutation per scanned file or reload the entire library for a single-file watcher event.
- Do NOT decode thumbnails/full-size images, format a large multi-path clipboard payload, or launch external programs synchronously in an input callback.
- Do NOT redefine the system accent, hard-code light/dark application surfaces, or dim primary controls to create visual hierarchy.
- Do NOT implement ordinary tags, source rows, filters, or the selection bar as pills/floating capsules.
- Do NOT use vertical ellipsis for Sort, decorative zoom icons, redundant media-type hover icons, shimmer loading, or a viewer scale transform.

---

## Cross-References

- [Source Directory Model](02_Architecture.md#1-source-directory-model)
- [Ignore Rules](02_Architecture.md#2-ignore-rules)
- [Tag Model and Tag Behavior](02_Architecture.md#3-tag-model-and-tag-behavior)
- [Search Behavior](04_Product_Spec.md#1-search-behavior)
- [Grid View Behavior](04_Product_Spec.md#4-grid-view-behavior)
- [Session Persistence Behavior](02_Architecture.md#8-session-persistence-behavior)
- [Performance Acceptance Budgets](04_Product_Spec.md#15-performance-acceptance-budgets)
- [Explicitly Accepted Constraints](01_Vision.md#4-explicitly-accepted-constraints)
- [Widget Tree](02_Architecture.md#9-widget-tree-source-of-truth)
- [State → UI Mapping](02_Architecture.md#10-state--ui-mapping)
- [Grid Cell States](04_Product_Spec.md#17-grid-cell-states)
- [Indexing / Scanning State](04_Product_Spec.md#21-indexing--scanning-state)
- [Accessibility and Focus](04_Product_Spec.md#23-accessibility-and-focus)
- [Visual Design](05_Visual_Design.md)
