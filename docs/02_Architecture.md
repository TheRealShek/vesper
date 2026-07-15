# Architecture

---

## 0. Architecture Contract

This document is the source of truth for data identity, persistence, indexing, background work, and cross-thread communication. User-visible behavior belongs to [04_Product_Spec.md](04_Product_Spec.md); GTK widget construction belongs to [03_Implementation.md](03_Implementation.md). All filesystem and database paths described here are read-only with respect to user media: Vesper may write only its own database, cache, logs, and lock files.

## 1. Source Directory Model

The user designates one or more local directories on their filesystem as **source roots**. Vesper has exactly one library; that library may contain multiple source roots. All indexed media from online roots appears in one unified grid.

**Behavior:**

- Source roots are added and removed via the Settings panel.
- Any number of non-overlapping source roots can be active simultaneously.
- All media from all source roots appears in a single unified grid.
- Adding a source root resolves its canonical path before storing it. A newly selected path that does not exist, is not a directory, cannot be read, or cannot be canonicalized is rejected with a recoverable Settings error; it is not stored as an offline root.
- A root is rejected if its canonical path duplicates an existing root, is inside an existing root, or contains an existing root.
- Removing a source root cancels active scan/thumbnail jobs for that root, removes its records transactionally, and leaves files on disk untouched.
- Each scan has a per-root generation id. Late results from stale generations are ignored.
- The application watches all online source roots for changes while running. New files appear in the grid automatically. Deleted files disappear automatically only when the source root itself is online.
- File system events use the app-wide debounce constant in `src/config.rs` (v1 default: 300ms) before processing.
- If a source root directory is unavailable at launch or disappears while running, the root is marked offline. Its media is hidden from the grid, search, selection, viewer navigation, and tag counts, but database records are preserved.
- Offline source roots remain visible in the sidebar source-root list with a passive offline indicator. No blocking dialog is shown.
- When an offline root becomes available again, it is rescanned before its media re-enters visible results.
- Root availability probing is scheduled independently of UI hydration and database read queries. A slow or disconnected mount must never stall application startup, search, filtering, or the GTK thread.

**Symlink policy:**

- Directory symlinks are not followed in v1.
- File symlinks may be indexed only if they resolve to a supported media file inside an allowed source-root boundary.
- File symlinks resolving outside all source roots, resolving to unsupported media, or duplicating an already indexed canonical file are skipped.

**Recognized/attempted media types:**

- Images: JPEG, PNG, GIF (static, first frame only), WEBP, TIFF, BMP, HEIC.
- Videos: MP4, MKV, AVI, MOV, WEBM, FLV, M4V.
- All other file types are silently ignored during indexing.
- Extension matching is ASCII case-insensitive. A supported extension makes a path a media candidate; successful decode is not required before publishing its basic record, except that HEIC without an available system decoder is skipped as stated below.
- HEIC is best-effort and decoder-dependent. If no system decoder is available, HEIC files are skipped.
- Hidden files and folders are indexed unless excluded by ignore rules.

---

## 2. Ignore Rules

The application supports a gitignore-like pattern system that prevents matching files and directories from being indexed. It works at two levels: global rules that apply across all source roots, and per-directory `.galleryignore` files that apply locally.

**Global ignore rules:**

- Managed in the Settings panel under "Ignore Rules."
- Displayed as a multi-line text field, one pattern per line.
- Apply to every source root without exception.
- Evaluated as the first entries in the effective rule list for every path.

**Per-directory `.galleryignore` files:**

- A plain text file named `.galleryignore` placed inside any directory within a source root.
- Rules in a `.galleryignore` file apply to that directory and all of its descendants.
- Rules do not propagate upward.
- `.galleryignore` files are watched for changes while the application is running. Editing a `.galleryignore` file triggers a rescan of the affected directory automatically.
- `.galleryignore` files are never shown in the media grid.

**Pattern syntax:**

Patterns follow gitignore-like rules:

- `*.ext` — matches any file with that extension anywhere within scope.
- `foldername/` — matches a directory of that name (trailing slash denotes directory).
- `foldername` — matches any file or directory of that name.
- `**/pattern` — matches pattern at any depth within scope.
- `pattern/**` — matches everything inside a directory named pattern.
- A leading `!` negates a pattern — explicitly includes files that would otherwise be ignored.
- Lines beginning with `#` are comments and are ignored.
- Blank lines are ignored.

Paths are matched relative to the directory that owns the rule set, using `/` as the logical separator even on paths received from the OS. A leading `/` anchors a pattern to that rule set's directory. A pattern containing `/` matches the relative path; a pattern without `/` matches a basename at any depth in scope. A trailing `/` matches directories only. Backslash escapes a leading `#` or `!` and otherwise represents a literal next character. Invalid patterns do not partially apply: Settings keeps the previous saved rules and identifies the invalid line.

**Rule precedence:**

1. Build the effective rule list from global rules first.
2. Append `.galleryignore` rules from the source root down to the candidate file's parent directory.
3. The last matching rule wins.
4. A negation rule (`!pattern`) is just another last-matching rule.
5. If a directory is excluded and not descended into, descendants cannot be re-included by rules inside that excluded directory.

**Behavior:**

- A directory matched by an ignore rule is not descended into. Its entire subtree is excluded.
- Ignored files and directories produce no entries in the library and no tags.
- Ignored files are not counted in tag file counts.
- Applying valid global ignore rules triggers a rescan of all online source roots.
- Editing a `.galleryignore` file triggers a rescan of the affected subtree.
- Already-indexed files that become ignored are removed from visible library records during the relevant rescan.
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

**Identity model:**

```text
tag_id: source_root_id + relative_folder_path
display_name: basename(relative_folder_path)
display_path: relative_folder_path with path separators rendered as breadcrumbs
```

Tag identity is path-qualified. Two folders with the same basename are different tags when their source root or relative folder path differs.

**Derivation rule:**

Every file receives one tag per ancestor folder between the source root and the file itself (inclusive, based on user preference). The displayed tag name is the folder name exactly as it appears on disk.

**Example:**

```
Source root: /home/user/media

File: /home/user/media/Travel/Japan/2023/photo.jpg

Tags assigned:
- Travel        (display_path: Travel)
- Japan         (display_path: Travel / Japan)
- 2023          (display_path: Travel / Japan / 2023)
```

**Root inclusion toggle:**

A setting controls whether the source root directory name itself is included as a tag. Default: OFF. When OFF, only subdirectories below the root are used as tags. Toggling this setting re-derives all tags.

**Tag properties:**

- Tags are case-sensitive and match the folder name exactly.
- Tags are re-derived on every rescan. They cannot be edited manually.
- When root-as-tag is OFF, a file directly under the source root has no tags. When root-as-tag is ON, that file receives the source-root tag.
- Tags have a file count — the number of online, visible media files that carry that tag.
- Tags are sorted by file count descending, then case-insensitive display name, then exact path identity.
- If multiple tags share the same display name, the sidebar row must disambiguate them with breadcrumb context in secondary text or tooltip.
- If display paths also collide across source roots, the sidebar row must include source-root display name or path in secondary text or tooltip.
- Sidebar count and sort updates must be published as batches. During a scan, existing rows keep their order until a batch boundary; the UI must not reorder the list once per discovered file.

**Tag inheritance:**

Selecting a tag includes all files that carry that exact path-qualified tag. Because files inherit ancestor-folder tags, selecting `Travel` shows files in `Travel/` and its descendants.

---

## 4. Storage, Index, and Cache Model

Vesper stores application state in a local SQLite database plus an on-disk thumbnail cache. There is no external API in v1.

**SQLite responsibilities:**

- schema version and migrations;
- source roots, including original path, canonical path, online/offline state, and last scan generation;
- media records, including source root, relative path, filename, canonical file identity when available, media type, size_bytes, modified_at, date added, dimensions/duration when known, thumbnail cache key, and thumbnail status;
- tag records using path-qualified identity;
- media-tag join records;
- scan errors tied to scan generation and path;
- global settings, including ignore rules and root-as-tag;
- session state, including filters, sort, zoom, scroll anchor, and window size;
- search indexes or indexed columns required for responsive filtering.

**Database schema contract:**

SQLite uses WAL mode, foreign keys, and a busy timeout. All schema changes go through explicit migrations; startup must not rely on best-effort `ALTER TABLE` statements that silently ignore failure.

All timestamps are stored as UTC Unix milliseconds. The UI converts them to local time for display. `date_added` is assigned when a path identity is first committed and is preserved across rescans and metadata-only updates; a move/rename creates a new path identity and therefore a new `date_added`.

Required tables:

| Table                | Purpose                                                                                                  |
| -------------------- | -------------------------------------------------------------------------------------------------------- |
| `schema_migrations`  | Stores applied migration ids and timestamps.                                                             |
| `source_roots`       | Stores original path, canonical path, display path, added time, online/offline state, and scan generation. |
| `media`              | Stores one row per indexed path identity, including source root, relative path, filename, canonical path identity, type, size_bytes, modified_at, date added, dimensions/duration, thumbnail cache key, thumbnail stale/failure status, scan generation, and last_accessed_at for thumbnail LRU eviction. Offline rows remain stored but are excluded by visible-library queries. |
| `tags`               | Stores path-qualified tag identity, display name, display path, and source root.                          |
| `media_tags`         | Stores media-to-tag join rows.                                                                           |
| `scan_errors`        | Stores path, source root, scan generation, error category, message, and last-seen time.                  |
| `settings`           | Stores global settings such as root-as-tag and serialized ignore rules.                                  |
| `session_state`      | Stores filters, sort, zoom, scroll anchor, window size, and other restart state.                         |

Required constraints and indexes:

- `source_roots.canonical_path` is unique.
- `media(source_root_id, relative_path)` is unique.
- `media.canonical_identity` stores the canonical target **path string**, not an inode/device or content hash, and has a unique index. A regular file stores its own canonical path; a supported file symlink stores its resolved target path. This prevents a symlink and its target from appearing twice while deliberately leaving distinct hard-link paths as distinct product identities.
- `tags(source_root_id, relative_folder_path)` is unique.
- `media_tags(media_id, tag_id)` is the primary key.
- Index media by `source_root_id`, `modified_at`, `date_added`, `filename`, `size_bytes`, `media_type`, `last_accessed_at`, and `(source_root_id, scan_generation)`.
- Index tags by `display_name`, `display_path`, and file-count query inputs.
- Index scan errors by `(source_root_id, scan_generation)` and path.

Migration behavior:

- Migrations run inside transactions.
- A failed migration leaves the previous schema intact and prevents normal app startup.
- A known database corruption/migration failure uses the Product-defined recovery dialog. Choosing "Rebuild Library Index" first preserves a recoverable copy of source roots and settings, recreates derived database records, and never modifies media files. If those settings cannot be read safely, rebuilding is unavailable and the generic closing dialog is used.
- Schema downgrade is not supported in v1.

**Thumbnail cache responsibilities:**

- cache files are addressed by a generated `thumbnail_cache_key` plus thumbnail variant;
- new files generate thumbnails automatically;
- modified existing files update metadata automatically, set `thumbnail_stale=true`, and keep pointing at the previous `thumbnail_cache_key` until explicit regeneration succeeds;
- successful explicit regeneration writes a new cache file, updates `thumbnail_cache_key`, clears `thumbnail_stale`, and then makes the new thumbnail visible;
- failed thumbnail generation stores a failure status so the grid can show a stable placeholder;
- cache cleanup removes entries for deleted media and removed roots;
- cache/database corruption recovery must provide a safe rebuild path that never modifies user media files.

**Thumbnail cache limits:**

- Cache directory is owned by Vesper under the user cache directory, separate from the SQLite database.
- v1 stores one square grid variant at 256px. Additional variants require a Product/Implementation update.
- Cache files are addressed by stable `thumbnail_cache_key`, not by raw filename, to avoid path-length and special-character issues.
- Default disk limit is 5 GB. When exceeded, evict least-recently-used thumbnail files that are not referenced by currently visible media.
- `last_accessed_at` is based on thumbnail reads, not filesystem mtime/atime — Linux noatime/relatime mounts make fs atime unreliable. Access timestamps are buffered and written in batches; a thumbnail is updated at most once per ten minutes so scrolling does not create a database write per cell bind.
- Memory cache limit is 256 MB or 512 decoded thumbnails, whichever is reached first.
- Visible and near-visible thumbnails have priority and are not evicted from memory during the current frame/update.
- Regenerate Thumbnails may overwrite cache entries for modified or failed media; it does not rewrite original media.
- Rebuild Library Index may discard and recreate the thumbnail cache manifest, but should preserve reusable cache files when their key still matches.

**Canonical identity scope:**

- Product-level media identity is path-based: moving or renaming a file is modeled as delete plus create.
- Canonical physical identity is not content duplicate detection.
- Canonical physical identity is used only to prevent duplicate indexing paths caused by overlapping roots, duplicate canonical roots, or supported file symlink paths.
- Hard links and bind mounts are not collapsed as duplicate content in v1 unless they are also caught by source-root overlap or symlink policy.

**Search and sort indexes:**

- Search must be case-insensitive and Unicode-normalized.
- Filename sorting uses case-insensitive natural ordering with full path as the final tie-breaker.
- All filter/sort/search queries must have deterministic ordering, with full path ascending as the final tie-breaker when no stronger ordering applies.

---

## 5. Runtime and Background Work Model

All I/O, database queries, media probing, thumbnail generation, and filesystem watching happen outside the GTK UI thread.

**Module boundaries:**

- `ui/` owns GTK widgets, input handling, and rendering state only.
- `backend/` owns long-running tasks, filesystem events, and job coordination.
- `index/` owns filesystem walking, ignore-rule evaluation, media discovery, and path normalization.
- `db/` owns SQLite schema, migrations, queries, and persistence.
- `thumbnail/` owns thumbnail extraction and cache writes.
- Cross-boundary communication uses typed events from `events.rs`. UI code must not import filesystem/index/database modules directly.

**Typed event contract:**

- Every user command that can succeed or fail carries a `request_id`; the backend returns exactly one terminal success/error event, even when the error is also logged.
- Scan/delta events carry `source_root_id` and `scan_generation`. Query events carry `query_generation`. Viewer/thumbnail results carry the media id plus a request generation.
- UI payloads contain immutable display summaries and cache/placeholder state, never SQLite rows, open file handles, GTK objects, or shared mutable collections.
- Progress events may be coalesced. Terminal completion, cancellation, offline, and error events may not be silently discarded.
- Cancellation is an explicit outcome, not an error dialog. The UI ignores a terminal result only when its generation/request is provably superseded.

**Worker behavior:**

- Source-root scans, metadata probing, and thumbnail jobs run through bounded background queues.
- Every cross-boundary channel is bounded. Producers pause or coalesce low-priority work when a queue is full; they must not create an unbounded task or event per file.
- Scanner concurrency defaults to one active full-root scan at a time to avoid disk thrash.
- Subtree rescans from watcher events may coalesce into the active root scan; otherwise they run after the current scan batch.
- Metadata/media-probe concurrency defaults to `min(4, available_parallelism)`.
- Thumbnail generation concurrency defaults to `min(4, available_parallelism)` because ffmpeg/decoders are CPU and I/O heavy.
- UI query jobs have priority over thumbnail generation.
- Watcher events are debounced for 300ms and then coalesced by source root plus nearest affected directory.
- Search/filter query updates supersede older in-flight queries; only the latest result generation is applied to UI state.
- Jobs carry source-root id and scan generation.
- Removing a root, changing root-as-tag, or changing ignore rules cancels or supersedes affected jobs.
- Stale job results are ignored if their generation no longer matches the current source-root generation.
- UI-facing queries are cancelable or superseded so rapid search/filter changes do not queue obsolete work.
- Thumbnail loading uses bounded memory caching; the grid requests thumbnails only for visible or near-visible cells.
- Scan errors are tied to path plus scan generation. A later successful scan of the same path clears the previous error.

**Indexing pipeline and publication order:**

1. **Discover:** walk paths and apply root, symlink, temporary-file, and ignore policies. Do not decode media here.
2. **Commit basic records:** read stable basic metadata, classify by supported extension, and upsert path identities in transactions of at most 500 records or 50ms of accumulated work, whichever comes first.
3. **Publish summaries:** emit one generation-tagged batch event containing added/changed/removed media summaries and tag-count deltas. The UI must not receive one full-library reload or one GTK mutation event per discovered file.
4. **Enrich:** probe dimensions/duration outside the scanner. Enrichment updates the existing row and is independently retryable.
5. **Thumbnail:** generate/cache thumbnails last. Visible and near-visible requests outrank scan-order jobs.

The first usable grid therefore depends only on discovery and basic-record commits, not on `ffprobe`, image decoding, or thumbnail completion. At the end of a successful full-root walk, records not marked by the current generation are removed in one reconciliation transaction. A canceled, failed, or offline scan must never perform that deletion sweep.

**Database and UI query behavior:**

- Database writes are serialized through one writer; UI reads use separate read-only connections or snapshots compatible with WAL.
- UI hydration is a pure database read. It must not probe filesystem liveness, reconfigure watchers, start scans, or mutate the database as a side effect.
- Each search/filter/sort request carries a monotonically increasing query generation. A newer request supersedes the older one; the UI applies only the newest complete result.
- Large result sets are delivered as compact media summaries in bounded chunks. GTK model mutations are scheduled in idle/frame-sized batches, preserving the old valid result until the replacement begins; input handlers never wait synchronously for the complete result.
- Live filesystem deltas are evaluated against the active query before publication, or trigger a superseding query refresh. They must not be appended blindly to a filtered/sorted model.
- Thumbnail request keys are deduplicated across visible cells and scan work. Requests that move outside the near-visible window may be canceled before decode starts.

**Maintenance operations:**

- Rescan library refreshes source-root availability, ignore-rule results, media metadata, tag derivation, and deleted/new file records.
- Regenerate thumbnails schedules thumbnail jobs for modified or failed media without blocking the UI.
- Rebuild library index recreates database-derived records from source roots and preserves user settings. It never modifies user media files.
- Only one maintenance operation that mutates index state may run at a time. Starting another presents a passive "Library maintenance is already running" status instead of overlapping destructive database work.

**Single-instance behavior:**

- The app acquires a library lock before opening the database for write access.
- If a second instance starts, it attempts to activate/focus the existing window through the application id. If the platform cannot do so, it exits with a clear message.
- If focusing is unavailable, the second instance exits with a clear non-blocking message.
- Two write-capable instances must never use the same library state simultaneously.

**Change-event behavior:**

| Event              | Behavior                                                                 |
| ------------------ | ------------------------------------------------------------------------ |
| New file           | Auto-index and generate thumbnail.                                       |
| Deleted file       | Remove only if the source root is online and the file is confirmed gone. |
| Moved/renamed file | Treat as delete plus create.                                             |
| Modified file      | Update metadata; thumbnail regeneration is explicit.                     |
| Root unavailable   | Mark root offline; preserve records; hide its media.                     |
| Root available     | Rescan root before showing media again.                                  |

**Canonical conflict reconciliation:**

- If a newly discovered path conflicts with an existing row on canonical identity, first check whether the old path still exists.
- If the old path is missing in the same source-root generation, reconcile the event as a rename/move: remove the old row and insert the new path as a fresh path identity.
- If the old path still exists and the conflict is caused by duplicate root coverage or a supported file symlink path, skip the duplicate path.
- If the conflict cannot be classified, keep the existing row and record a scan warning rather than publishing two records for the same physical file.

---

## 6. Transient Files and Partial Copies

Filesystem watchers may report files before copy/write operations are complete. Vesper must avoid publishing unstable media records.

**Temporary-file rules:**

- Default ignore patterns include common temporary extensions such as `*.tmp`, `*.part`, and known cache/trash directories.
- Additional transient extensions such as `.crdownload`, `.download`, `.partial`, `.swp`, and files ending in `~` are treated as scanner-level temporary files even if not present in user-visible ignore rules.
- Scanner-level temporary files do not produce scan errors.

**Stability rules:**

- The double-metadata stability check applies to watcher-created files and files modified within the last two seconds. Read metadata twice 250ms apart using an async timer; if size or modified time changes, defer probing. Do not impose this delay on every old file in an initial/full scan.
- Retry unstable or temporarily unreadable supported files with bounded backoff: 1s, 5s, 30s, then once on the next rescan/watch event.
- Do not create a visible media row until the file has stable metadata and either media probing succeeds or a stable placeholder state can be recorded.
- If the source root goes offline during retries, stop retrying and mark the root offline rather than recording per-file failures.

---

## 7. Logging and Diagnostics

Vesper has no telemetry in v1. Diagnostics are local-only.

**Logging policy:**

- Logs are written to the user's local state/cache area only.
- Logs are never uploaded or sent to a remote service by the application.
- Default logging records app lifecycle, scan start/finish, source-root availability changes, migration failures, and aggregate error counts.
- Avoid logging full media paths at info level. Full paths may appear at debug level only when explicitly enabled.
- Log rotation keeps at most 10 MB per file and 3 rotated files.
- Debug logging is enabled by environment variable or explicit developer build setting, not by default user interaction.
- Fatal startup errors may show a user-facing message, but stack traces remain in local logs.

---

## 8. Session Persistence Behavior

The application restores the following state on every launch after the first:

| State item              | Persisted                    |
| ----------------------- | ---------------------------- |
| Active tag filters      | Yes                          |
| AND/OR tag filter mode  | Yes                          |
| Active search query     | No — always clears on launch |
| Sort order              | Yes                          |
| Grid zoom level         | Yes                          |
| Scroll anchor in grid   | Yes                          |
| Window size             | Yes                          |
| Window position         | No on Wayland                |
| Source root list        | Yes                          |
| Root-as-tag toggle      | Yes                          |

Session state that is explicitly NOT persisted:

- Viewer open state. The app never re-opens the viewer on launch.
- Selection state. No cells are pre-selected on launch.
- Info panel open state within viewer.

**Scroll restoration:**

Persist scroll position as a stable anchor, not a raw pixel offset:

```text
anchor_media_id/path
anchor_offset_within_cell
sort/filter context hash
```

Restore window size, zoom, sort, and filters before resolving the scroll anchor.

If a persisted tag identity no longer exists because its root was removed, its filter is discarded. If its root is offline, the filter is suspended and hidden rather than making the remaining online library appear empty; it is restored when that root returns and its rescan succeeds. The status surface explains that offline-source filters are temporarily unavailable.

---

## 9. WIDGET TREE (source of truth)

```
adw::ApplicationWindow
└── gtk::Overlay                                           ← app_overlay
    ├── child: gtk::Box [horizontal, hexpand=true, vexpand=true] ← main_box
    │   ├── gtk::Box [vertical, vexpand=true]               ← sidebar_root
    │   │   CSS: .sidebar-panel
    │   │   width: fixed 220px (min-width=max-width in CSS, no width_request in Rust)
    │   │   NO GtkPaned. NO toggle. NO collapse.
    │   │   rendered in first-run empty state with "No tags available" and empty sources list
    │   │
    │   └── adw::ToolbarView [hexpand=true, vexpand=true]   ← grid_toolbar_view
    │       CSS: .grid-area
    │       ├── TOP: adw::HeaderBar                         ← header_bar
    │       ├── TOP: status banner/row stack                 ← status_banner_stack
    │       └── CONTENT: gtk::Stack                         ← root_stack
    │           ├── "empty"      → EmptyState widget
    │           ├── "no-results" → NoResults widget
    │           └── "grid"       → gtk::Overlay              ← grid_overlay
    │                               ├── child: gtk::GridView ← grid_view
    │                               ├── overlay: action_bar_revealer
    │                               └── overlay: scan_error_button
    └── overlay: viewer_overlay [visible only while viewer open]
```

Viewer overlay is mounted at `app_overlay` level so it covers the full application content area, including sidebar and header, and temporarily disables sidebar/header interaction. The selection action bar remains grid-scoped. Opening the viewer clears selection; viewer mode and selection mode cannot be active simultaneously in v1.

`scan_error_button` is bottom-left of the grid area. Offline-root and indexing status use the status banner/row stack below the header.

**Status priority:**

1. recoverable critical state;
2. offline roots;
3. scan/indexing active;

Scan warnings/errors remain independently accessible through `scan_error_button`, even when a higher-priority banner is present. Unrecoverable application errors are not shown in the status banner stack; they use the Product-specified closing dialog.

---

## 10. STATE → UI MAPPING

| State field                          | Widget affected             | Behavior                                                               |
| ------------------------------------ | --------------------------- | ---------------------------------------------------------------------- |
| `selected_tags`                      | `tag_list_box` rows         | Row gets `.active` CSS class                                           |
| `selected_tags.len` + `search_query` | `clear_filters_button`      | `set_visible(has_tags or has_search)`; label is `Clear filters (N)`     |
| `selected_tags.len`                  | `match_mode_box`            | `set_visible(count >= 2)`                                              |
| `tag_filter_mode`                    | `match_any_radio/all_radio` | Radio active state                                                     |
| `sort_order`                         | Sort popover radio group    | Active radio reflects current sort                                     |
| `search_query`                       | Search entry                | NOT persisted — clears on launch                                       |
| `scroll_anchor`                      | `grid_view`                 | Restored after zoom/sort/filter restore                                |
| `zoom_level`                         | Zoom slider                 | Restored on launch                                                     |
| `offline_roots`                      | `status_banner_stack`       | Offline status visible while any root is offline                       |
| suspended offline tag filters        | `status_banner_stack`       | Offline text explains that affected filters are temporarily unavailable |
| `scan_active`                        | `status_banner_stack`       | Indexing status visible when no higher-priority status is active       |
| `scan_errors`                        | `scan_error_button`         | Passive grid-area indicator with popover                               |

**Not persisted:** viewer open state, selection state, info panel state, search query.

**Derived UI only:** clear-filters label, no-results stack page, action bar visibility, scan/indexing status visibility, and match mode visibility are recalculated from current in-memory state and are not stored independently.

---

## Cross-References

- [Explicitly Accepted Constraints](01_Vision.md#4-explicitly-accepted-constraints)
- [Indexing / Scanning State](04_Product_Spec.md#21-indexing--scanning-state)
- [What Not To Do](03_Implementation.md#10-what-not-to-do-agent-guard-rails)
