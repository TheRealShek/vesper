# Implementation

---

## 0. Scope and Authority

This document defines the GTK4/libadwaita widget structure, CSS class naming, and construction guard rails for Vesper's UI. It implements — and must not deviate from — the widget tree in [Architecture §9](02_Architecture.md#9-widget-tree-source-of-truth) and the state mapping in [Architecture §10](02_Architecture.md#10-state--ui-mapping). Behavior and copy come from [04_Product_Spec.md](04_Product_Spec.md); appearance from [05_Visual_Design.md](05_Visual_Design.md).

`Must`/`must not` are requirements. The UI layer owns widgets, input, and render state only; it must not import `index/`, `db/`, `thumbnail/`, or filesystem modules directly — all cross-boundary data arrives as typed events (Architecture §5).

---

## 1. Widget Structure (GTK4 / libadwaita)

This mirrors Architecture §9 one-to-one; the node names below are the canonical widget identifiers. No node may be added, removed, or re-parented without first amending Architecture §9.

| Node                  | Widget                                   | Notes                                                                 |
| --------------------- | ---------------------------------------- | --------------------------------------------------------------------- |
| root window           | `adw::ApplicationWindow`                 | Single instance; library lock acquired before DB write open (Arch §5) |
| `app_overlay`         | `gtk::Overlay`                           | Hosts `main_box` as child; `viewer_overlay` as an overlay             |
| `main_box`            | `gtk::Box` (horizontal)                  | `hexpand`, `vexpand`                                                   |
| `sidebar_revealer`    | `gtk::Revealer`                          | `transition-type = slide-right`; `reveal-child = !sidebar_collapsed`  |
| `sidebar_root`        | `gtk::Box` (vertical)                    | `.sidebar-panel`; width fixed 220px in CSS (no `width_request`)        |
| `sidebar_header`      | `gtk::Box`                               | Brand + `.sidebar-collapse` (`«`) button                              |
| `tag_list_box`        | `gtk::ListBox` in `gtk::ScrolledWindow`  | Flat, count-sorted `.tag-row` rows; first-run single placeholder row  |
| `sidebar_footer`      | `gtk::Box`                               | `add_source_root_button` (`.suggested-action`) + `open_settings_button` (gear) |
| `grid_toolbar_view`   | `adw::ToolbarView`                       | `.grid-area`                                                           |
| `header_bar`          | `adw::HeaderBar`                         | Start: `sidebar_toggle` (`☰`, visible when collapsed). End: `sort_button`, `thumbnail_size_button`, `select_button`, `primary_menu_button`, window controls |
| `status_banner_stack` | `gtk::Stack`                             | At most one banner by priority (Arch §9): critical → offline → scan/indexing |
| `root_stack`          | `gtk::Stack`                             | Pages `empty`, `no-results`, `grid`; `crossfade` transition           |
| `grid_overlay`        | `gtk::Overlay`                           | Child `grid_view`; overlays `action_bar_revealer`, `scan_error_button`|
| `grid_view`           | `gtk::GridView` in `gtk::ScrolledWindow` | `GtkSignalListItemFactory`; recycled cells; virtualized               |
| `action_bar_revealer` | `gtk::Revealer`                          | Selection action bar; `slide-up`; grid-scoped                         |
| `scan_error_button`   | `gtk::Button` + `gtk::Popover`           | Bottom-left of grid; passive scan-issue indicator                     |
| `viewer_overlay`      | `ViewerView` (composite)                 | Visible only while viewer open; covers sidebar + header               |

**Viewer subtree** (inside `viewer_overlay`):

| Node                | Widget                                    | Notes                                                              |
| ------------------- | ----------------------------------------- | ------------------------------------------------------------------ |
| `viewer_main`       | `gtk::Box` (vertical)                     | Left region                                                        |
| `viewer_topbar`     | `gtk::Box`                                | Brand + breadcrumb; `panel_toggle`, `fullscreen_button`, `viewer_menu_button`, `close_button` |
| `viewer_stage`      | `gtk::Overlay`                            | Child `gtk::Picture` (image) or video widget                       |
| `nav_prev`/`nav_next`| `gtk::Button`                            | `‹` / `›` overlays                                                  |
| `filename_pill`     | `gtk::Box` overlay                        | Filename + "N / M" position                                        |
| `zoom_controls`     | `gtk::Box` overlay                        | fit, `−`, level ("1:1"), `+`, fullscreen                           |
| `info_tags_panel`   | `adw::ViewStack` + `adw::ViewSwitcher`    | Pages `info`, `tags`; both **read-only**; toggled by `panel_toggle`|

**Construction rules:**

- The sidebar is a `gtk::Revealer` wrapping a fixed-width box. There is **no `GtkPaned`, no drag handle, no partial rail** — collapsed means fully hidden (Arch §9; Product §10).
- `grid_view` uses a virtualized `GtkGridView` with a signal factory that binds compact media summaries only; cells hold no SQLite rows, file handles, or GTK objects from the backend (Arch §5).
- The five thumbnail sizes map to five cell measurements; the 12px gutter is constant across sizes (Visual §3).
- `status_banner_stack` shows one child; priority is resolved in the view-model, not by stacking multiple banners.
- The viewer is mounted at the `app_overlay` level so it covers sidebar and header and disables their input while open; opening it clears selection (Arch §9).
- Settings is a separate `adw::PreferencesWindow`/dialog (allowed exception), not a `root_stack` page.

---

## 2. CSS Class Naming Conventions

- Classes are **kebab-case**, component-scoped, and semantic. Never encode a color, pixel size, or token value in a class name (`.tag-row`, not `.indigo-row`).
- One component class per element; **state is a separate standalone adjective class**: `.active`, `.selected`, `.disabled`, `.offline`, `.critical`, `.loading`. Do not fold state into the component name.
- Reuse libadwaita/GTK style classes where they exist: `.suggested-action` (primary buttons), `.flat`, `.dim-label`, `.title-1`/`.title-2`, `.osd` (viewer chrome), `.card`. Introduce a custom class only when no standard class fits.
- Color comes exclusively from Visual §1 tokens defined once in the stylesheet; component classes reference tokens via CSS variables, never literal hex.

**Canonical class list:**

| Area          | Classes                                                                                  |
| ------------- | ---------------------------------------------------------------------------------------- |
| Layout        | `.sidebar-panel`, `.grid-area`                                                            |
| Sidebar       | `.sidebar-header`, `.sidebar-brand`, `.sidebar-collapse`, `.sidebar-footer`, `.tag-row`, `.tag-count` (state: `.active`) |
| Grid cell     | `.media-cell`, `.cell-check`, `.cell-duration`, `.cell-hover-actions` (state: `.selected`, `.loading`) |
| States pages  | `.empty-state`, `.no-results`, `.placeholder-illustration`                                |
| Status        | `.status-banner` (state: `.offline`, `.critical`), `.scan-error-button`, `.scan-error-popover` |
| Selection     | `.selection-bar`                                                                          |
| Viewer        | `.viewer`, `.viewer-topbar`, `.viewer-stage`, `.filename-pill`, `.viewer-nav`, `.zoom-controls`, `.info-panel`, `.info-row`, `.tag-chip` |
| Settings      | `.settings-nav`, `.settings-section`, `.source-root-row`, `.ignore-rule-row`             |

---

## 3. State → UI Wiring

Implement exactly the mapping in [Architecture §10](02_Architecture.md#10-state--ui-mapping). Summary of the bindings the UI must honor:

- `selected_tags` → `.active` on `tag_list_box` rows; `search_query`/tag count → clear-filters affordance; `selected_tags.len >= 2` → match-mode control visible; `tag_filter_mode` → AND/OR radios.
- `sort_order` → sort popover; `search_query` is **not** persisted (clears on launch); `scroll_anchor` restored after window/zoom/sort/filter; `zoom_level` restored on launch.
- `offline_roots` and suspended offline-tag filters → `status_banner_stack` (offline priority); `scan_active` → indexing status when no higher banner; `scan_errors` → `scan_error_button` popover.
- `sidebar_collapsed` → `sidebar_revealer.reveal-child` (inverted) and `sidebar_toggle` visibility; persisted across launches.
- Viewer open, selection, and info-panel-open states are **not** persisted (Arch §8); derived UI (clear-filters label, `no-results` page selection, action-bar visibility, status visibility, match-mode visibility) is recomputed from in-memory state, never stored.

All backend results carry a generation/request id; the UI applies a result only when its generation is current and ignores superseded ones (Arch §5). Model mutations for large result sets are scheduled in idle/frame-sized batches and never block input handlers.

---

## 4. Allowed Dialogs

The UI must not introduce any modal beyond these (Vision §2):

1. Settings panel/window.
2. System folder chooser (add source root).
3. Keyboard-shortcuts help window.
4. Unrecoverable application-error / closing dialog.
5. Database corruption / migration-failure recovery dialog (with "Rebuild Library Index", Arch §4).

Everything else — indexing progress, per-file scan/read errors, offline notices, thumbnail failures — is surfaced through passive banners, the scan-issue indicator, or placeholders. No modal progress, ever.

---

## 5. Implementation Guard Rails — What Not To Do

- **No widget-tree deviation.** Do not add, remove, or re-parent any §1 / Architecture §9 node without amending Architecture §9 first. In particular: no `GtkPaned`, no sidebar resize handle, no extra `root_stack` pages, no second status banner shown simultaneously.
- **No new dialogs** beyond the five in §4. No custom "add folder", "confirm", or "error" modals.
- **Do not build rejected features** even though mockups depict them (Vision §5, Product §0): star ratings; EXIF fields (Date taken, Camera, Lens, ISO, Focal length, Aperture, Exposure); GPS/Location; content hashes (MD5/SHA256); manual/editable tags (`+ Add tag`, tag removal); collections (`Add to Collection`); Date/Type/Rating filter chips; list view; grouped/smart sidebar sections; a Settings "Metadata" page.
- **Viewer Info/Tags are read-only.** The Info panel exposes only filesystem/application metadata (File name, Type, Added, Modified, Dimensions, Duration, Folder, Source). The Tags tab shows folder-derived chips with no add/remove.
- **Selection actions are exactly four** (Open, Reveal in Folder, Copy Path, Clear Selection) plus the count. No delete/move/rename/tag/rate/collect.
- **Ignore rules** are a pattern list (Arch §2); the settings table is presentation only and must not change matching semantics. Defaults are never auto-restored; ship no `.*`/hidden-files default.
- **Never touch the GTK thread with I/O.** Filesystem walking, liveness checks, DB queries, decoding/probing, cache maintenance, and clipboard prep run off-thread (Vision §4, Arch §5). Cells request thumbnails only when visible/near-visible.
- **Never crash on bad media.** Unsupported/ignored files are skipped silently; unreadable supported files aggregate into the scan-issue indicator; thumbnail failures show a stable placeholder (Vision §2).
- **Respect generation guards.** Discard stale off-thread results (scan deltas, queries, decodes) whose generation is superseded; never render the wrong image for the current viewer position or the wrong page for the current query.
- **Respect reduced motion.** When animations are disabled, every transition collapses to an instant swap (Visual §6).

---

## Cross-References

- [Architecture §5 — Runtime & Background Work](02_Architecture.md#5-runtime-and-background-work-model)
- [Architecture §9 — Widget Tree](02_Architecture.md#9-widget-tree-source-of-truth)
- [Architecture §10 — State → UI Mapping](02_Architecture.md#10-state--ui-mapping)
- [Product Spec](04_Product_Spec.md)
- [Visual Design](05_Visual_Design.md)
