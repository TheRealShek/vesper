# Architecture

---

## 1. Source Directory Model

The user designates one or more directories on their filesystem as **source roots**. The application indexes all media files found recursively within those roots.

**Behavior:**

- Source roots are added and removed via the Settings panel.
- Any number of source roots can be active simultaneously.
- All media from all source roots appears in a single unified grid.
- Removing a source root removes its media from the library immediately. Files on disk are untouched.
- The application watches all source roots for changes while running. New files appear in the grid automatically. Deleted files disappear automatically. File system events are debounced before processing.
- Symbolic links within source roots are followed one level deep. Circular symlinks are ignored silently.
- If a source root directory is unavailable at launch (unmounted drive, deleted path), the application launches normally, shows available media, and displays a passive indicator that one or more source roots are offline. No blocking dialog.

**Supported media types:**

- Images: JPEG, PNG, GIF (static, first frame only), WEBP, TIFF, BMP, HEIC.
- Videos: MP4, MKV, AVI, MOV, WEBM, FLV, M4V.
- All other file types are silently ignored during indexing.

---

## 2. Ignore Rules

The application supports a pattern-based ignore system that prevents matching files and directories from being indexed. It works at two levels: global rules that apply across all source roots, and per-directory `.galleryignore` files that apply locally.

**Global ignore rules:**

- Managed in the Settings panel under "Ignore Rules."
- Displayed as an editable list of patterns, one per line.
- Apply to every source root without exception.
- Evaluated before any file or directory is indexed.

**Per-directory `.galleryignore` files:**

- A plain text file named `.galleryignore` placed inside any directory within a source root.
- Rules in a `.galleryignore` file apply to that directory and all of its descendants.
- Rules do not propagate upward.
- `.galleryignore` files are watched for changes while the application is running. Editing a `.galleryignore` file triggers a rescan of the affected directory automatically.
- `.galleryignore` files are never shown in the media grid.

**Pattern syntax:**

Patterns follow the same rules as `.gitignore`:

- `*.ext` — matches any file with that extension anywhere within scope.
- `foldername/` — matches a directory of that name (trailing slash denotes directory).
- `foldername` — matches any file or directory of that name.
- `**/pattern` — matches pattern at any depth within scope.
- `pattern/**` — matches everything inside a directory named pattern.
- A leading `!` negates a pattern — explicitly includes files that would otherwise be ignored.
- Lines beginning with `#` are comments and are ignored.
- Blank lines are ignored.

**Rule precedence:**

1. Per-directory `.galleryignore` rules are evaluated first, innermost directory first.
2. Global rules are evaluated after per-directory rules.
3. A negation rule (`!pattern`) at any level can un-ignore a file that a broader rule would have excluded.
4. The most specific matching rule wins.

**Behavior:**

- A directory matched by an ignore rule is not descended into. Its entire subtree is excluded.
- Ignored files and directories produce no entries in the library and no tags.
- Ignored files are not counted in tag file counts.
- Ignore rules take effect on the next rescan or filesystem watch event. Already-indexed files that become ignored are removed from the library on the next rescan.
- No indication is shown in the UI for ignored files. They simply do not exist from the application's perspective.

**Default global ignore patterns (pre-populated on first launch):**

```
.git/
node_modules/
.Trash/
.cache/
*.tmp
*.part
.DS_Store
Thumbs.db
```

The user can edit or remove any default pattern. Defaults are never restored automatically.

---

## 3. Tag Model and Tag Behavior

Tags are derived exclusively from the folder hierarchy of each source root. No manual tags exist in v1.

**Derivation rule:**

Every file receives one tag per ancestor folder between the source root and the file itself (inclusive, based on user preference). The tag name is the folder name exactly as it appears on disk.

**Example:**

```
Source root: /home/user/media

File: /home/user/media/Travel/Japan/2023/photo.jpg

Tags assigned: Travel, Japan, 2023
```

**Root inclusion toggle:**

A setting controls whether the source root directory name itself is included as a tag. Default: OFF. When OFF, only subdirectories below the root are used as tags.

**Tag properties:**

- Tags are case-sensitive and match the folder name exactly.
- Tags are re-derived on every rescan. They cannot be edited manually.
- A file with no subdirectory between it and the source root has no tags.
- Tags have a file count — the number of media files that carry that tag.
- Tags are sorted by file count, descending. The tag with the most files appears first.

**Tag inheritance:**

Selecting a parent tag includes all files that have that tag at any depth. Selecting "Travel" shows all files in `Travel/` and all subdirectories recursively.

---

## 4. Session Persistence Behavior

The application restores the following state on every launch after the first:

| State item               | Persisted                    |
| ------------------------ | ---------------------------- |
| Active tag filters       | Yes                          |
| AND/OR tag filter mode   | Yes                          |
| Active search query      | No — always clears on launch |
| Sort order               | Yes                          |
| Grid zoom level          | Yes                          |
| Sidebar width            | Yes                          |
| Sidebar collapsed state  | Yes                          |
| Scroll position in grid  | Yes                          |
| Window size and position | Yes                          |
| Source root list         | Yes                          |
| Root-as-tag toggle       | Yes                          |

Session state that is explicitly NOT persisted:

- Viewer open state. The app never re-opens the viewer on launch.
- Selection state. No cells are pre-selected on launch.
- Info panel open state within viewer.

---

## 5. WIDGET TREE (source of truth)

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

## 6. STATE → UI MAPPING

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

## Cross-References

> See [Explicitly Accepted Constraints] in [01_Vision.md] for full spec.

> See [Indexing / Scanning State] in [04_Product_Spec.md] for full spec.

> See [What Not To Do] in [03_Implementation.md] for full spec.
