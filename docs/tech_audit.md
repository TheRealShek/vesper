# Vesper Codebase Audit & Technical Debt

**Originally created:** 2026-07-15
**Last verified:** 2026-07-16

This document records the current result of a from-scratch verification audit of
Vesper against the five numbered specification documents:
`01_Vision.md`, `02_Architecture.md`, `03_Implementation.md`,
`04_Product_Spec.md`, and `05_Visual_Design.md`.

The verification included the complete A/B/I/T/U/V issue set, cross-feature
regression checks, and the following validation gate:

```text
cargo fmt --check && cargo clippy -- -D warnings && cargo test
```

All three stages passed. The test result was **109 passed; 0 failed; 0 ignored;
0 measured; 0 filtered out**.

## How to read this document

- **STILL OPEN** means the original violation remains wholly or partly present.
- **REGRESSED** means the original work exists, but its current integration breaks
  the required behavior.
- **NEW ISSUE** means the verification found a specification violation not covered
  by the original A/B/I/T/U/V item set.
- Every open entry states the specification **Law**, the current-code
  **Violation**, and the required **Fix** scope.
- Confirmed fixes are retained in a compressed inventory at the bottom so stable
  IDs remain traceable without mixing closed work into the active backlog.

---

# Open, regressed, and newly identified issues

## A — Data identity, schema, persistence

### A-1 — STILL OPEN: migration failure has no Rebuild/Close recovery — **Critical**

- **Law:** 02 §4 requires explicit transactional migrations and
  `schema_migrations`. When a migration prevents normal startup, 04 §12 requires a
  recoverable dialog explaining that user media is unaffected and offering exactly
  **Rebuild Library Index** and **Close**.
- **Violation:** Transactional migrations now exist, and migration errors are
  distinguished at `src/main.rs:112-115`, but
  `run_migration_recovery_dialog` is explicitly still a stub. It routes directly
  to the generic one-button closing dialog at `src/main.rs:216-225`; there is no
  Rebuild response or recovery flow.
- **Fix:** Replace the stub with the Product Spec's two-button recovery dialog and
  connect Rebuild to a recovery path that can recreate the index while preserving
  user media and source-root configuration. Normal startup must remain blocked
  until the user chooses Rebuild or Close.

### A-4 — REGRESSED: partial scans can clear unresolved scan errors — **High**

- **Law:** 02 §4 requires persistent `scan_errors` keyed by source root,
  generation, and path. 04 §12 requires an error to clear only after that path is
  successfully scanned or is authoritatively known to have disappeared.
- **Violation:** The required tables and persistence exist, but a partial full-root
  scan first skips its deletion sweep at `src/scan.rs:191-213` and then
  unconditionally clears every prior error for the root at `src/scan.rs:215-225`
  via `clear_scan_errors_for_root` (`src/db/scan_errors.rs:48-60`). Previous errors
  below an unreadable or otherwise unvisited subtree can therefore disappear even
  though those paths did not succeed in the current scan.
- **Fix:** Reconcile scan errors with the same completeness guarantees as media
  reconciliation. Clear only paths proven successful within the completed scan
  scope; retain errors for unreadable, cancelled, failed, or offline portions of a
  partial scan.

### A-6 — STILL OPEN: stable scroll-anchor restoration is racy — **Medium**

- **Law:** 02 §8 requires a stable anchor containing media identity, offset within
  the cell, and sort/filter context. Window size, zoom, sort, and filters must be
  restored before resolving that anchor against the final visible result set.
- **Violation:** `ScrollAnchor` is persisted, but startup filter restoration can
  dispatch an asynchronous query at `src/ui/window.rs:616-623` while anchor
  resolution is independently queued with `idle_add_local_once` at
  `src/ui/window.rs:625-681`. A later `QueryResult` clears and rebuilds the entire
  list at `src/ui/window.rs:476-500` without restoring the anchor again. Depending
  on scheduling, the anchor can be resolved against pre-query hydration and then
  invalidated by the actual restored-filter result.
- **Fix:** Tie anchor restoration to completion of the newest restored query
  generation. Resolve and scroll only after the final sort/filter/search model has
  been published and settled; discard anchor work associated with superseded
  generations.

### A-7 — REGRESSED: suspended filters do not reactivate during the session — **Medium**

- **Law:** 02 §8 requires filters belonging to removed roots to be discarded and
  filters belonging to offline roots to be suspended and hidden, then restored
  automatically after that root returns and is rescanned. The status surface must
  explain the suspension.
- **Violation:** Identity-aware reconciliation exists, but it is guarded by
  `if is_first_fetch` at `src/ui/window.rs:574-683`. Hydration caused by a later
  root-availability transition does not rerun reconciliation, so a filter suspended
  at startup remains suspended after the root returns during the same process.
  The banner text is consequently based on a stale suspended-filter set.
- **Fix:** Reconcile persisted/active/suspended filters on every authoritative root
  availability and tag snapshot, not only the first fetch. Preserve identity-based
  discard semantics and trigger one superseding query when the active set changes.

## B — Runtime, backend, concurrency

### B-2 — STILL OPEN: hydration and query publication are not fully decoupled/bounded — **High**

- **Law:** 02 §5 requires UI hydration to be a pure database read, independent of
  liveness probing, watcher configuration, scans, and database mutation. Search,
  filter, sort, and hydration results carry generations, and large results are
  published/applied in bounded chunks. 03 §9–§10 forbid unbounded GTK-thread list
  replacement and filesystem work as a hydration side effect.
- **Violation:** Query generations, stale-result rejection, structured scan errors,
  and chunked initial hydration exist, but three gaps remain:
  1. `AppEvent::FetchData` still sends `LivenessCommand::Probe` as a side effect at
     `src/backend/app_loop.rs:300-312`, so hydration is not pure.
  2. `QueryMedia` returns one complete `Vec` in one `QueryResult` at
     `src/backend/app_loop.rs:278-298`. The GTK handler synchronously calls
     `remove_all` and appends every item at `src/ui/window.rs:476-487`, so large
     filtered/search results are not bounded or frame-batched.
  3. Fetch coalescing drops any fetch arriving while one is active at
     `src/backend/app_loop.rs:300-304` and records no trailing refresh, allowing a
     newer requested state refresh to be lost.
- **Fix:** Make hydration a database-only request; schedule liveness independently.
  Publish all potentially large query results in generation-tagged chunks and
  apply them in idle/frame-sized batches. Coalescing must retain a pending refresh
  bit or newest requested generation so a final hydration always follows an
  in-progress one.

### B-3 — STILL OPEN: scanner-level temporary suffix list is incomplete — **High**

- **Law:** 02 §6 requires two metadata reads 250ms apart, bounded retries at
  1s/5s/30s, and scanner-level rejection of transient files including
  `.crdownload`, `.download`, `.partial`, `.swp`, and names ending in `~`. These
  files must never produce media rows or scan errors.
- **Violation:** The double-read and bounded retry flow exists, but
  `TEMP_FILE_SUFFIXES` at `src/index/media.rs:20-24` omits `.download`. Such a file
  can pass the scanner-level filter and be treated as ordinary supported media if
  its remaining filename has a supported extension.
- **Fix:** Make the hard-coded scanner-level suffix set match 02 §6 exactly and keep
  it shared by full scans and live-update classification before stability probing
  or error publication.

### B-7 — STILL OPEN: cancellation and queue bounds cover only part of the job system — **Medium**

- **Law:** 02 §5 requires one active full-root scan, bounded probe/thumbnail/subtree
  work, UI-query priority over thumbnails, and root-scoped generations that cancel
  stale work when a root is removed. Per the verified scope decision, settings
  changes do **not** bump the job generation; only root removal does.
- **Violation:** The settings-change scope is correct: `UpdateSettings` at
  `src/backend/app_loop.rs:166-171` does not invalidate a generation, while
  `RemoveSourceRoot` does at `src/backend/app_loop.rs:112-116`. However:
  1. Only subtree scans receive and check a cancellation token
     (`src/backend/app_loop.rs:221-248`, `src/scan.rs:304-317`). Initial and
     maintenance full-root scans call `run_scan` without root-removal cancellation
     (`src/backend/maintenance.rs:119-149`, `src/backend/maintenance.rs:228-250`).
  2. Each distinct subtree request creates a Tokio task before it awaits the scan
     semaphore at `src/backend/app_loop.rs:202-265`. Active scans are bounded, but
     the number of queued tasks/jobs is not.
- **Fix:** Carry root id and generation through every root-owned scan/job, including
  initial and maintenance scans, and stop publishing or sweeping after root
  invalidation. Put pending subtree work in a bounded queue before spawning worker
  tasks. Keep settings changes explicitly outside generation invalidation.

### B-8 — STILL OPEN: diagnostics still bypass rotation and path privacy — **Low**

- **Law:** 02 §7 requires local rotated logs (10 MB ×3), lifecycle/scan/root-
  availability/migration events, and no full filesystem paths at info level.
- **Violation:** The `tracing` subscriber, rotation, and redacted structured events
  exist, but thumbnail subprocess failures still call `eprintln!` with the complete
  `media_path` at `src/thumbnail.rs:415` and `src/thumbnail.rs:472`. These messages
  bypass the rotated log and expose full media paths at normal diagnostic level.
- **Fix:** Route all remaining operational diagnostics through the structured logger
  and apply the same redaction policy. Full paths may appear only at explicit
  debug/trace level.

## I — Indexing policy

### I-2 — STILL OPEN: canonical duplicate reconciliation is only per walk — **Critical**

- **Law:** 02 §1 allows file symlinks only when their resolved target is a supported
  media file inside an allowed source-root boundary. A symlink and its target, or
  multiple symlinks to one target, must yield one media record. 02 §4–§5 require a
  unique canonical identity and deterministic canonical-conflict reconciliation.
- **Violation:** `src/index/walker.rs:256-281` correctly enforces root boundaries and
  deduplicates canonical targets within one walker invocation. The seen set is not
  shared across separate root scans, however. A symlink under root A targeting a
  regular file under non-overlapping root B can reach two scans. Database upsert
  handles only `ON CONFLICT(path)` at `src/db/media.rs:47-67`; the unique
  `canonical_identity` collision can fail the whole batch instead of reconciling or
  skipping the duplicate.
- **Fix:** Add canonical-identity conflict handling at the database/index boundary,
  where it applies across all scans and roots. Reconcile to one deterministic
  path-based record without turning a duplicate into a batch-wide indexing error.

### I-5 — STILL OPEN: production validation remains marked and documented as dead code — **Medium**

- **Law:** 02 §2 requires unified global-then-local, last-match-wins ignore
  evaluation. Invalid patterns must not partially apply, and Settings must identify
  invalid lines. Production validation code must reflect and enforce that contract
  without suppressing genuine liveness checks.
- **Violation:** Unified matching is implemented at
  `src/index/ignore_rules.rs:80-115`, and Settings calls
  `validate_global_patterns` at `src/ui/settings.rs:42-47`. Despite that production
  use, `IgnoreValidationError` remains `#[allow(dead_code)]` at
  `src/index/ignore_rules.rs:24`, `validate_global_patterns` remains suppressed at
  line 124, and their documentation says they are not wired to a production caller
  (`src/index/ignore_rules.rs:17-23`, `src/index/ignore_rules.rs:117-123`). The
  directory validator has the same suppression and stale claim at lines 151-159.
- **Fix:** Remove obsolete dead-code suppressions from production-consumed types and
  functions and make their documentation match actual callers. Either wire the
  directory validator into the indexing validation path or remove/consolidate it
  if the established walker validator is the canonical implementation; retain
  per-line, no-partial-apply behavior.

### I-6 — STILL OPEN: some directory-read errors still allow a deletion sweep — **Medium**

- **Law:** 02 §5 states that cancelled, failed, partial, or offline scans must never
  perform the stale-generation deletion sweep. Any unreadable portion makes the
  scan partial unless its absence is authoritatively established.
- **Violation:** Failure to open a directory sets `had_read_error` at
  `src/index/walker.rs:128-151`, and `src/scan.rs:191-213` skips the sweep for a
  partial summary. But an error returned while iterating an already-open `ReadDir`
  only emits `ScanEvent::Error` and continues at `src/index/walker.rs:155-167`; it
  never sets `had_read_error`. The summary can therefore be marked complete and
  delete records for entries that were not discoverable.
- **Fix:** Mark the walk partial for every directory enumeration error, including
  per-entry iterator failures, channel cancellation, and an offline root detected
  before reconciliation. Gate all full and subtree sweeps on that complete status.

## U — UI structure and behavior

### U-4 — STILL OPEN: sidebar tag ordering does not end with exact identity — **High**

- **Law:** 02 §3 and 04 §2 require tag order by file count descending, then
  case-insensitive display name, then exact path identity. Equal display names use
  lineage only for disambiguation. A-2 identity must remain
  `source_root_id + relative_folder_path` through display and activation.
- **Violation:** Tag rows now retain the full A-2 identity and render flat rows with
  lineage, but both UI and database ordering insert `display_path` before the
  identity fields: `src/ui/sidebar.rs:32-44` and `src/db/tags.rs:63-75`.
  `display_path` is presentation data, not the specified final exact identity, and
  can change the relative order of tags before the identity tie-break is reached.
  U-12 Unicode normalization does not repair this because the sidebar sorts the
  unnormalized display values independently.
- **Fix:** Use exactly count descending → case-insensitive display name → canonical
  path identity, consistently in database batch publication and UI presentation.
  Keep lineage generation separate from the ordering key and preserve the full
  identity in activation/filter payloads.

### U-5 — REGRESSED: reopening Settings can overwrite newer settings — **High**

- **Law:** 03 §5 and 04 §24 require Settings controls to reflect the current saved
  state. Apply validates and saves only the edited setting, while closing discards
  unapplied edits. Independent setting changes must not overwrite each other.
- **Violation:** `src/ui/window.rs:245-262` captures one `BackendState` clone when
  the main window is constructed. Every later Settings opening receives that stale
  startup clone. Ignore Apply and the root-as-tag switch then send the complete
  cloned state back at `src/ui/settings.rs:326-331` and
  `src/ui/settings.rs:354-364`. A later dialog opening can display old values and
  applying one field can revert a field saved during an earlier opening.
- **Fix:** Read the current backend settings whenever the dialog opens, or maintain
  a UI-side current snapshot updated by authoritative backend events. Persist
  field-scoped changes or merge against the current backend state so one control
  cannot clobber unrelated settings.

### U-11 — STILL OPEN: viewer close does not restore the captured origin — **Low**

- **Law:** 04 §8 and §25 require viewer close to return to the captured origin, or
  the nearest valid cell if it disappeared, restore focus, and highlight that cell
  for 900ms.
- **Violation:** `Viewer` stores only `current_index` at `src/ui/viewer.rs:6-13`.
  Navigation mutates that index at `src/ui/viewer.rs:639-668`, and close publishes
  the current navigated index at `src/ui/viewer.rs:633-636`. No opening-origin media
  identity or origin-relative fallback is retained, so navigating before close
  changes which cell is scrolled to and highlighted.
- **Fix:** Capture the opening media identity separately from navigation state.
  On close resolve that identity against the current grid, fall back to the nearest
  valid cell if necessary, then scroll, focus, and apply the 900ms highlight.

### U-12 — REGRESSED: GTK filtering/sorting overrides compliant database search — **Medium**

- **Law:** 04 §1 requires Unicode-normalized search and ranking by exact basename,
  basename prefix, basename substring, exact tag, tag substring, path substring,
  current sort, and full path ascending. 02 §4 makes full path the final stable
  tie-breaker.
- **Violation:** `src/db/search.rs:34-53` and `src/db/search.rs:99-122` implement the
  normalized query and required database ranking. Those results are then wrapped
  in a GTK `FilterListModel` and `SortListModel` at
  `src/ui/filter_controller.rs:45-61`. The local filter uses only
  `to_lowercase().contains(...)` at `src/ui/filter_sort.rs:19-35`, which can reject
  canonically equivalent Unicode matches, and the local sorter implements only
  four coarse ranks at `src/ui/filter_sort.rs:54-93`, which can reorder the
  database's correct ranking and tie-break.
- **Fix:** Establish one authoritative search filter/order. Remove the conflicting
  local search transformations or make them consume the exact same normalized keys,
  rank values, current-sort key, and full-path tie-break without reinterpreting the
  database result.

### U-13 — STILL OPEN: status counts are unthrottled and scan-error visibility can be stale — **Low**

- **Law:** 02 §9–§10 and 04 §12 require recoverable-critical > offline > indexing
  banner priority, an independent scan-error indicator/popover listing affected
  paths, and progress/count updates no more than ten times per second.
- **Violation:** Banner priority and path listing exist, but scan progress is emitted
  every 50 discoveries with no time throttle at `src/scan.rs:100-106` and applied
  immediately at `src/ui/window.rs:380-383`; a fast scan can exceed ten updates per
  second. In addition, any `ScanCompleted(0, ...)` hides the scan-error button at
  `src/ui/window.rs:384-403` without checking errors persisted for other roots or
  subtrees. The database is queried only after the hidden button is clicked at
  `src/ui/window.rs:1132-1139`.
- **Fix:** Rate-limit progress publication by elapsed time and merge counts at the
  receiver. Derive scan-error button visibility/count from the authoritative
  persisted error set after each scan completion rather than from only that
  completion event's local failure vector.

## V — Visual design

### V-6 — STILL OPEN: the selection bar remains a floating destructive capsule — **Medium**

- **Law:** 03 §3/§10 and 05 §5/§7/§10 require ordinary controls and the selection
  bar to avoid pill/floating-capsule treatment. The selection bar is edge-attached,
  uses an opaque toolbar/header surface with a top border, has no drop shadow, and
  “Deselect all” is not destructive styling.
- **Violation:** The original tag/filter/scan pill cases were removed, but the
  selection bar is still centered with a bottom margin and `.action-bar` class at
  `src/ui/selection_bar.rs:24-31`. Its CSS uses a 12px radius and drop shadow at
  `src/ui/style.css:69-75`, and “Deselect all” still uses
  `destructive-action` at `src/ui/selection_bar.rs:42-45`.
- **Fix:** Make the selection bar grid-width and edge-attached with the specified
  toolbar surface and top border. Remove floating margins, capsule radius/shadow,
  and destructive styling from the neutral deselection action.

## NEW — Issues newly identified by verification

### NEW-1 — NEW ISSUE: offline media is published, rendered, and counted — **High**

- **Law:** 01 §4, 02 §1/§3, 03 §10, and 04 §12 require offline-root media to be
  excluded from the grid, search, selection, viewer navigation, and tag counts.
  Records remain in SQLite, but v1 must not show them as disabled or dimmed cells.
- **Violation:** Search joins `source_roots` but never filters
  `sr.is_available = 1` (`src/db/search.rs:10-60`). Hydration likewise returns rows
  from offline roots (`src/db/media.rs:442-475`). Grid binding explicitly dims and
  badges those rows at `src/ui/grid_cell.rs:517-524` and
  `src/ui/grid_cell.rs:633-644`. Tag counts count all `media_tags` rows without an
  availability join at `src/db/tags.rs:63-75`.
- **Fix:** Define one online-visible media predicate and apply it consistently to
  hydration, search/filter queries, selection/viewer snapshots, and tag counts.
  Preserve offline database rows and restore their visibility only after the root
  is authoritatively online.

### NEW-2 — NEW ISSUE: rejected stale thumbnail completions are still published — **High**

- **Law:** 01 §4 and 02 §4 require a modified file to keep its old thumbnail and
  stale state until regeneration for the current source version succeeds. Results
  produced for a previous file version must not replace the UI or clear stale
  state.
- **Violation:** `Database::set_thumbnail_success` correctly guards its update with
  `modified_at` and returns whether a row changed at `src/db/media.rs:120-145`.
  `generate_and_record` ignores that boolean at `src/thumbnail.rs:348-352`, returns
  success anyway, and the worker emits `UiEvent::ThumbnailReady` at
  `src/thumbnail.rs:123-129`. If the media changes during generation, the database
  rejects the stale write but the UI still installs the obsolete cache path.
- **Fix:** Treat a false guarded update as a stale/cancelled result, remove or leave
  the unreferenced generated file for bounded cleanup, and do not publish
  `ThumbnailReady`. Leave the row stale for a generation based on the current
  source metadata.

### NEW-3 — NEW ISSUE: schema timestamps use seconds instead of Unix milliseconds — **Medium**

- **Law:** 02 §4 states that all timestamps are stored as UTC Unix milliseconds;
  the UI converts them to local time only for display.
- **Violation:** The general conversion used for media metadata returns Unix seconds
  at `src/db/models.rs:172-180`. `date_added` uses `strftime('%s', 'now')` at
  `src/db/media.rs:47-50`; source-root `added_at` uses the seconds helper at
  `src/db/roots.rs:8-13`; migration `applied_at` uses `as_secs()` at
  `src/db/migrations.rs:97-103`; and scan-error `last_seen` also uses
  `strftime('%s', 'now')` at `src/db/scan_errors.rs:28-34`. Only the thumbnail
  access-time path uses the millisecond helper.
- **Fix:** Standardize every persisted timestamp on UTC Unix milliseconds, migrate
  existing second-resolution values deterministically, and update UI conversion to
  interpret the stored unit consistently.

### NEW-4 — NEW ISSUE: GTK UI directly owns and queries SQLite — **High**

- **Law:** 02 §9 and 03 §9–§10 require UI/backend/database communication through
  typed events. `src/ui/` is GTK-only; filesystem and database work must be
  asynchronous or offloaded and must never block an input callback.
- **Violation:** The window construction path receives an `Arc<Database>` at
  `src/ui/window.rs:147`, captures it for the scan-error button, and synchronously
  calls `get_scan_error_paths()` inside `connect_clicked` at
  `src/ui/window.rs:1130-1139`. This crosses the UI/database module boundary and can
  block the GTK thread on a SQLite lock/read.
- **Fix:** Request scan-error data through a typed backend event, perform the query
  on the database worker/read connection, and return a generation/current-state
  result for the GTK layer to render.

### NEW-5 — NEW ISSUE: tag display paths incorrectly include the source-root name — **Medium**

- **Law:** 02 §3 defines `display_path` as `relative_folder_path` with separators
  rendered as breadcrumbs. The source-root name/path is added only as secondary
  disambiguation when otherwise equal display paths collide across roots.
- **Violation:** `derive_tags` prepends `root_name` to every ordinary tag
  `display_path` at `src/scan.rs:615-645`. For a source root `media` and relative
  folder `Travel/Japan`, the stored value becomes `media/Travel/Japan` instead of
  `Travel/Japan`. This also contaminates U-4 presentation ordering and search data.
- **Fix:** Store the relative folder lineage as `display_path`, using breadcrumb
  presentation at the UI boundary. Add source-root context only in collision-driven
  secondary text/tooltip generation, never to the canonical display-path field.

### NEW-6 — NEW ISSUE: viewer navigation has no stable identity snapshot — **High**

- **Law:** 04 §8 requires the viewer to capture the current filtered, sorted media
  list at open time and navigate that snapshot until close. The snapshot stores
  stable media identities, skips items removed or made offline, and shows an
  unavailable state if none remain.
- **Violation:** `Viewer` retains the live mutable `gtk::SortListModel` and a GTK row
  index at `src/ui/viewer.rs:6-13`. `open`, `next`, and `prev` read the model's
  current item count and positions at `src/ui/viewer.rs:592-600` and
  `src/ui/viewer.rs:639-668`. A live query replacement can change the navigation
  set/order while the viewer is open, and there is no stable-id list with which to
  skip removed/offline items.
- **Fix:** Capture an ordered vector of stable media identities when opening the
  viewer. Resolve each identity against authoritative current availability during
  navigation, skip invalid entries, and keep the captured order independent of live
  GTK model mutations.

### NEW-7 — NEW ISSUE: tag-list search still enforces the 30-tag collapse — **Low**

- **Law:** 03 §1 and 04 §2 require tag-list search to inspect all tags, show every
  matching tag while the query is non-empty, and hide the Show more/less control.
  Clearing the query restores the previous session-only expanded state.
- **Violation:** `src/ui/sidebar.rs:245-287` filters all rows, but matching rows after
  the 30th are still hidden unless `show_all` is already true
  (`total_matches <= 30 || show_all`). When more than 30 matches exist, the code
  shows the Show more/less button rather than hiding it.
- **Fix:** When the trimmed tag query is non-empty, bypass the collapsed limit and
  hide Show more/less without changing the saved session expansion flag. Reapply
  that flag after the query is cleared.

### NEW-8 — NEW ISSUE: explicit visual-removal-list violations remain — **Medium**

- **Law:** 03 §3/§10 and 05 §10 explicitly remove low-opacity primary controls,
  viewer scale transforms, stacked viewer/video gradients, grid-cell shadows,
  redundant media-type hover icons, whole-row offline dimming, and nonstandard
  media-card radii.
- **Violation:** Current code still contains:
  - 40% resting viewer controls at `src/ui/style.css:96-103`;
  - a viewer scale transform at `src/ui/style.css:116-125`;
  - stacked video/header gradients at `src/ui/style.css:135-143`;
  - 12px grid-cell radius and shadow at `src/ui/style.css:148-155`;
  - whole-row offline opacity at `src/ui/style.css:186-188`; and
  - a redundant media-type icon in the hover overlay at
    `src/ui/grid_cell.rs:241-250`.
- **Fix:** Apply the 05 §10 removal list literally: readable full-opacity controls,
  opacity-only viewer motion, one specified filename gradient, 6px shadowless grid
  cells, explicit Offline text without row dimming, and filename-only hover content.

### NEW-9 — NEW ISSUE: selection actions perform external work in GTK callbacks — **Medium**

- **Law:** 03 §9 requires external application launch and potentially large
  clipboard payload work to be offloaded or deferred outside the immediate GTK
  input callback. Input handlers must remain bounded and non-blocking.
- **Violation:** `Copy path(s)` collects every selected path and joins the complete
  payload directly inside `connect_clicked` at `src/ui/selection_bar.rs:67-75`.
  `Open file location` constructs the URI and invokes
  `AppInfo::launch_default_for_uri` synchronously in its click callback at
  `src/ui/selection_bar.rs:78-92`.
- **Fix:** Snapshot only the required stable selection data in the input callback,
  prepare large clipboard content in bounded/deferred work, and dispatch external
  launch through the appropriate asynchronous/deferred GLib path with recoverable
  error reporting.

---

# Confirmed fixed inventory

### A-2 — CONFIRMED FIXED
Tags previously used global basename identity. They now use
`source_root_id + relative_folder_path` throughout storage, events, and filtering.

### A-3 — CONFIRMED FIXED
The media schema previously lacked required identity, thumbnail-state, access-time,
constraint, and index fields. Those columns, constraints, and indexes now exist.

### A-5 — CONFIRMED FIXED
Settings and session state previously lived in `state.json`. They now persist in
SQLite, with a one-time legacy import.

### B-1 — CONFIRMED FIXED
The database previously had no process-level library lock. Vesper now acquires an
exclusive lock before opening SQLite and handles second-instance activation.

### B-4 — CONFIRMED FIXED
Delete events previously removed records without root-liveness protection. Removal
now requires an online owning root and confirmation that the file is gone.

### B-5 — CONFIRMED FIXED
Filesystem debounce previously used 500ms. The app-wide debounce constant is now
the specified 300ms.

### B-6 — CONFIRMED FIXED
Only Rescan existed previously. Rescan, Regenerate Thumbnails, and Rebuild Library
Index now share one mutually exclusive maintenance coordinator.

### I-1 — CONFIRMED FIXED
Directory symlinks were previously followed one level. Directory symlink entries
are now skipped in v1.

### I-3 — CONFIRMED FIXED
Duplicate and nested source roots were previously accepted. Canonical roots are now
compared in both directions and covered paths are rejected before insertion.

### I-4 — CONFIRMED FIXED
Roots were previously inserted before complete path validation. Existence,
canonicalization, directory type, and readability are now checked first.

### T-1 — CONFIRMED FIXED
Thumbnail cache keys, stale/failure state, and old-thumbnail preservation were
missing. They now exist with an explicit regeneration flow.

### T-2 — CONFIRMED FIXED
Thumbnail caches previously had no budgets or LRU policy. Disk and memory limits,
non-visible LRU eviction, and batched access timestamps are now implemented.

### T-3 — CONFIRMED FIXED
Deleted media and removed roots previously left thumbnail files behind. Cache files
are now cleaned during media deletion, stale sweeps, and root removal.

### U-1 — CONFIRMED FIXED
The viewer was mounted inside the grid overlay. It is now mounted on the top-level
application overlay while grid-scoped controls remain in the grid.

### U-2 — CONFIRMED FIXED
The header previously violated the required title/search/filter/sort/zoom layout.
It now uses the specified composition and five-detent zoom control.

### U-3 — CONFIRMED FIXED
The UI previously exposed and sorted by Date created. User-facing sort and metadata
now use Date added semantics.

### U-6 — CONFIRMED FIXED
Open Location was previously enabled across multiple physical folders. It now
disables with the specified explanation when selected parents differ.

### U-7 — CONFIRMED FIXED
Viewer zoom previously used the wrong step and lacked the required maximum. It now
uses 12.5% relative steps and clamps from fit through 800%.

### U-8 — CONFIRMED FIXED
The information panel previously overlaid the media. It is now a sibling panel that
pushes and shrinks the media area.

### U-9 — CONFIRMED FIXED
The information panel previously exposed Created and lacked the required path/date
details. It now shows Date added and the required path affordance.

### U-10 — CONFIRMED FIXED
Viewer navigation previously wrapped silently. It now shows the specified 120ms
opacity-only directional edge cue.

### U-14 — CONFIRMED FIXED
First-launch controls and empty states previously lacked required accessible
explanations and hints. The specified disabled states, shortcut hint, and tag state exist.

### V-1 — CONFIRMED FIXED
The stylesheet previously redefined the application accent. Custom accent overrides
have been removed in favor of the system theme.

### V-2 — CONFIRMED FIXED
The stylesheet previously used the audited hard-coded dark application surfaces.
Those cited surfaces now use theme-provided colors.

### V-3 — CONFIRMED FIXED
Loading previously used a continuous shimmer. It now uses a stable placeholder and
delayed native spinner.

### V-4 — CONFIRMED FIXED
The stylesheet previously used `transition: all`. Transitions are now restricted to
explicit properties.

### V-5 — CONFIRMED FIXED
Grid selection previously lacked the bounded tint layer. Cells now use the required
selection tint without lowering picture opacity.

### V-7 — CONFIRMED FIXED
The header previously used zoom-side icons and an ellipsis Sort icon. Those icons
are removed and Sort is presented as a labeled menu control.
