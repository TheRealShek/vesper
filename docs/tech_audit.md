# Vesper Codebase Audit & Technical Debt

**Originally created:** 2026-07-15
**Last verified:** 2026-07-16 (all previously open items fixed)

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

- Every previously STILL OPEN, REGRESSED, and NEW issue from the 2026-07-15
  audit has been fixed; the active backlog below is empty.
- Confirmed fixes are retained in a compressed inventory so stable IDs remain
  traceable.

---

# Open, regressed, and newly identified issues

_None. All items from the 2026-07-15 verification were fixed on 2026-07-16;
see the inventory below._

---

# Confirmed fixed inventory

### A-1 — CONFIRMED FIXED (2026-07-16)
The migration-failure path previously routed to a one-button stub. It now shows
the 04 §12 two-button recovery dialog (Rebuild Library Index / Close); Rebuild
preserves the source-root configuration, moves the failed index aside, and
recreates a fresh index at the current schema. Normal startup stays blocked
until the user chooses.

### A-2 — CONFIRMED FIXED
Tags previously used global basename identity. They now use
`source_root_id + relative_folder_path` throughout storage, events, and filtering.

### A-3 — CONFIRMED FIXED
The media schema previously lacked required identity, thumbnail-state, access-time,
constraint, and index fields. Those columns, constraints, and indexes now exist.

### A-4 — CONFIRMED FIXED (2026-07-16)
Partial scans previously cleared every prior error for the root. Errors are now
cleared per successfully-upserted batch (path-scoped, across generations), and
the root/subtree-wide replacement runs only when the scan completed; partial or
cancelled scans retain errors for unvisited paths.

### A-5 — CONFIRMED FIXED
Settings and session state previously lived in `state.json`. They now persist in
SQLite, with a one-time legacy import.

### A-6 — CONFIRMED FIXED (2026-07-16)
Anchor restoration previously raced pre-query hydration. The startup anchor is
now held pending and resolved only against the authoritative query result
(`QueryResult`/`QueryChunk` of the current generation), never against
hydration; it is cleared once its item is found.

### A-7 — CONFIRMED FIXED (2026-07-16)
Filter reconciliation was previously guarded by `is_first_fetch`. Persisted,
active, and suspended filters are now reconciled on every authoritative
roots+tags snapshot, so a filter suspended by an offline root reactivates when
the root returns during the same session, and the selection/row highlighting
follows the reconciled set.

### B-1 — CONFIRMED FIXED
The database previously had no process-level library lock. Vesper now acquires an
exclusive lock before opening SQLite and handles second-instance activation.

### B-2 — CONFIRMED FIXED (2026-07-16)
Hydration is now a pure database read: liveness probing is self-scheduled by the
liveness worker (periodic tick plus explicit probes on root add/remove), not a
hydration side effect. Query results are published in bounded, generation-tagged
chunks (`QueryResult` + `QueryChunk`) and applied per event-loop iteration.
Fetch coalescing keeps a pending-refresh bit so a trailing hydration always
follows an in-progress one.

### B-3 — CONFIRMED FIXED (2026-07-16)
`TEMP_FILE_SUFFIXES` now matches 02 §6 exactly: `.crdownload`, `.download`,
`.partial`, `.swp`, and `~`, shared by full scans and live-update classification.

### B-4 — CONFIRMED FIXED
Delete events previously removed records without root-liveness protection. Removal
now requires an online owning root and confirmation that the file is gone.

### B-5 — CONFIRMED FIXED
Filesystem debounce previously used 500ms. The app-wide debounce constant is now
the specified 300ms.

### B-6 — CONFIRMED FIXED
Only Rescan existed previously. Rescan, Regenerate Thumbnails, and Rebuild Library
Index now share one mutually exclusive maintenance coordinator.

### B-7 — CONFIRMED FIXED (2026-07-16)
Every root-owned full scan — initial, rescan, and rebuild — now carries its
root's generation-based cancellation token and stops consuming, publishing,
sweeping, and error-set mutation once the root is invalidated. Pending subtree
work is bounded (128 queued+running slots) before any task is spawned. Settings
changes remain explicitly outside generation invalidation.

### B-8 — CONFIRMED FIXED (2026-07-16)
The last `eprintln!` diagnostics (thumbnail subprocess failures) now go through
the structured `tracing` logger with `redact_path`, so no full media path leaks
outside debug/trace level and all operational output lands in the rotated log.

### I-1 — CONFIRMED FIXED
Directory symlinks were previously followed one level. Directory symlink entries
are now skipped in v1.

### I-2 — CONFIRMED FIXED (2026-07-16)
Canonical-identity conflicts across scans/roots previously failed the whole
upsert batch. The upsert now reconciles deterministically at the database
boundary: the record with the lexicographically smaller path wins; the losing
duplicate is skipped (no row, no tags) or replaced, never a batch-wide error.

### I-3 — CONFIRMED FIXED
Duplicate and nested source roots were previously accepted. Canonical roots are now
compared in both directions and covered paths are rejected before insertion.

### I-4 — CONFIRMED FIXED
Roots were previously inserted before complete path validation. Existence,
canonicalization, directory type, and readability are now checked first.

### I-5 — CONFIRMED FIXED (2026-07-16)
`IgnoreValidationError` and `validate_global_patterns` are production-consumed
(Settings Apply) and no longer carry `#[allow(dead_code)]` or stale "not wired"
documentation. The redundant directory validator was removed; the walker's
`load_directory_rules` is the canonical `.galleryignore` handler with whole-file
(no partial apply) rejection.

### I-6 — CONFIRMED FIXED (2026-07-16)
Per-entry `ReadDir` iterator failures and unreadable entry file types now mark
the walk partial (`had_read_error`), so no deletion sweep can run for a scan
whose directory enumeration was incomplete.

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

### U-4 — CONFIRMED FIXED (2026-07-16)
Tag ordering is now exactly file count descending → case-insensitive display
name → exact path identity (`source_root_id`, `relative_folder_path`), in both
the database query and UI presentation; `display_path` no longer participates
in the ordering key.

### U-5 — CONFIRMED FIXED (2026-07-16)
Settings previously received a `BackendState` clone captured at main-window
construction. Every dialog opening now reads the current saved state from the
shared `AppState`, and each control merges its field-scoped change against the
current backend state at apply time, so one control cannot clobber another.

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

### U-11 — CONFIRMED FIXED (2026-07-16)
The viewer now captures its opening origin (snapshot position) separately from
navigation state. Close resolves that origin against the current grid, falls
back to the nearest valid snapshot neighbour when it disappeared, and the
existing scroll/focus/900ms-highlight flow applies to that cell.

### U-12 — CONFIRMED FIXED (2026-07-16)
The conflicting GTK `CustomFilter`/`CustomSorter` were removed; the list models
are pass-throughs, and the Unicode-normalized database search/ranking (exact
basename → prefix → substring → exact tag → tag substring → path substring →
current sort → full path ascending) is the single authoritative order. Every
hydration and filter/sort/search change dispatches one superseding,
generation-stamped query.

### U-13 — CONFIRMED FIXED (2026-07-16)
Scan progress publication is now rate-limited by elapsed time (≤10/s), and the
scan-error indicator's visibility/count derives from the authoritative persisted
`scan_errors` set (fetched via a typed event after each completion and at
startup), not from one completion event's local failure vector.

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

### V-6 — CONFIRMED FIXED (2026-07-16)
The selection bar is now grid-width and edge-attached with an opaque toolbar
surface and top border — no floating margin, capsule radius, or drop shadow —
and "Deselect all" uses neutral (non-destructive) styling.

### V-7 — CONFIRMED FIXED
The header previously used zoom-side icons and an ellipsis Sort icon. Those icons
are removed and Sort is presented as a labeled menu control.

### NEW-1 — CONFIRMED FIXED (2026-07-16)
Offline-root media is excluded by one `sr.is_available = 1` predicate applied to
hydration, search/filter queries, and tag counts (selection and viewer consume
those results). Rows stay in SQLite and return when the root is authoritatively
online; the grid's offline dimming/badging was removed.

### NEW-2 — CONFIRMED FIXED (2026-07-16)
A thumbnail completion whose guarded database write is rejected (source changed
during generation) is now treated as stale: the obsolete cache file is removed,
no `ThumbnailReady` is published, and the row stays stale for regeneration.

### NEW-3 — CONFIRMED FIXED (2026-07-16)
Every persisted timestamp (`date_added`, `created_at`, `modified_at`,
`added_at`, `applied_at`, `last_seen`) is now UTC Unix milliseconds; migration 6
converts legacy second-resolution values deterministically (×1000), and the UI
converts milliseconds to local time for display.

### NEW-4 — CONFIRMED FIXED (2026-07-16)
The scan-error surface no longer holds an `Arc<Database>` or queries SQLite on
the GTK thread. It requests the persisted error paths via the typed
`FetchScanErrors` event; the read runs on the database worker and returns as
`ScanErrorPaths`, cached UI-side for the popover.

### NEW-5 — CONFIRMED FIXED (2026-07-16)
`derive_tags` stores the relative folder lineage as `display_path` (empty for
root-as-tag) without the source-root name; root context appears only in the
UI's collision-driven secondary text. The tag upsert refreshes display fields
on conflict so legacy prefixed rows converge on rescan.

### NEW-6 — CONFIRMED FIXED (2026-07-16)
The viewer captures an ordered vector of stable media identities at open time
and navigates only that snapshot, resolving each identity against the live
model, skipping removed/offline items, wrapping with the edge cue, and showing
the unavailable state when none remain. Live query replacements no longer
mutate the navigation set.

### NEW-7 — CONFIRMED FIXED (2026-07-16)
A non-empty tag query now shows every matching tag (the 30-row collapse is
bypassed) and hides Show more/less without changing the session expansion flag,
which is reapplied when the query clears.

### NEW-8 — CONFIRMED FIXED (2026-07-16)
The 05 §10 removal list is applied literally: full-opacity viewer controls,
opacity-only viewer motion, no stacked viewer/video gradients (only the grid
cell's filename hover gradient remains), 6px shadowless grid cells, explicit
"Offline" text without whole-row dimming, and filename-only hover content.

### NEW-9 — CONFIRMED FIXED (2026-07-16)
Selection actions snapshot only stable data inside the click callback: the
clipboard payload is joined and set in deferred idle work, and Open file
location uses the asynchronous GIO launch with a logged, recoverable error.
