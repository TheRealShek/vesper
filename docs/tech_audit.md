# Vesper Codebase Audit & Technical Debt (merged)

**Created:** 2026-07-15.
**Merges:** the former `docs/tech_debt.md` (architecture backlog) and `docs/tech_issues_docs.md` (doc re-audit file, now empty), plus a fresh full-codebase audit against the five numbered specification docs. The gitignored root-level `tech_issues_docs.md` (2026-07-04 audit) is fully absorbed here and is superseded.

**How to read this document:**

- Each issue states the **Law** — the exact requirement in a numbered doc (`01`–`05`) — the **Violation** with `file:line` evidence, and the **Fix**.
- IDs are stable. `A-*` = architecture/data, `B-*` = backend/runtime, `I-*` = indexing policy, `T-*` = thumbnails, `U-*` = UI structure/behavior, `V-*` = visual design.
- Severity: **Critical** = breaks a `must` that affects data identity or correctness; **High** = breaks a `must` visible to the user; **Medium** = breaks a `must` with a workaround or a strong `should`; **Low** = polish/robustness.
- Section "Rivers" at the end gives the required fix order: fix upstream issues before downstream ones.
- Rules for maintaining this file: no speculative issues; every entry cites an audit; remove entries once fixed; keep concise.

---

## Carried-over open items (from former tech_debt.md)

These predate this audit and remain open. They are folded into the rivers below.

### ARCH-001: No generations on UI-mutating events — Medium
DataFetched / QueryResult / MediaAdded / MediaRemoved / TagsUpdated can all mutate overlapping UI state with no generation ID or query token; late results can overwrite newer state. Law: 02_Architecture §5 "Each search/filter/sort request carries a monotonically increasing query generation… the UI applies only the newest complete result."
Fix: add `query_generation` to query events and generation checks before applying results (part of B-2).

### ARCH-002: Live updates uncoordinated with active query — Medium
`MediaAdded`/`MediaRemoved` are applied independently of the active search/filter/sort. Law: 02 §5 "Live filesystem deltas are evaluated against the active query before publication, or trigger a superseding query refresh."
Fix: on live delta, either evaluate against active query or issue a superseding query refresh (part of B-2).

### ARCH-003: Subtree scan errors not surfaced — Low
Subtree scan failures are dropped (`if let Ok(res)` in `src/backend/app_loop.rs:207`, error branch silently ignored). Law: 02 §5 "Terminal completion, cancellation, offline, and error events may not be silently discarded."
Fix: emit a structured backend error event on subtree scan failure (part of B-2).

### ARCH-004: Overloaded FetchData event — High
`AppEvent::FetchData` (`src/backend/app_loop.rs:250-367`) performs synchronous `fs::read_dir` liveness probing in the async loop, configures watchers, mutates the DB (`set_source_root_available`), triggers startup scans, drops concurrent fetches via an `AtomicBool`, and reloads the **entire** library (`get_all_media_with_tags`) into one unversioned `DataFetched` event. Law: 02 §5 "UI hydration is a pure database read. It must not probe filesystem liveness, reconfigure watchers, start scans, or mutate the database as a side effect"; 03 §10 "Do NOT perform root-liveness checks, watcher setup, scans, or database writes as side effects of UI hydration"; "Do NOT … reload the entire library for a single-file watcher event."
Fix: extract liveness probing + watcher management into an independent background worker; make FetchData a read-only query; replace full-store reloads with generation-tagged bounded chunks (B-2, depends on A-river).

---

## A — Data identity, schema, persistence

### A-1: No migration system; best-effort ALTER TABLE at startup — **Critical**
- **Law:** 02 §4 "All schema changes go through explicit migrations; startup must not rely on best-effort `ALTER TABLE` statements that silently ignore failure." Required table `schema_migrations`.
- **Violation:** `src/db/schema.rs:13-17` runs `let _ = conn.execute("ALTER TABLE …")` swallowing errors; no `schema_migrations` table exists (`src/db/schema.rs:34-79`).
- **Fix:** add a `schema_migrations` table and a transactional migration runner; a failed migration must prevent normal startup and route to the Product recovery dialog (04 §12).

### A-2: Tags are global unique strings, not path-qualified — **Critical**
- **Law:** 01 §2 / 02 §3: `tag_id = source_root_id + relative_folder_path`; two folders with the same basename are different tags. Required unique constraint `tags(source_root_id, relative_folder_path)`.
- **Violation:** `src/db/schema.rs:61-64` — `tags(name TEXT NOT NULL UNIQUE)`. All folders named `2023` merge into one tag across all roots.
- **Fix:** migrate `tags` to `(id, source_root_id, relative_folder_path, display_name, display_path)` with the spec's unique index; update tag derivation (`src/scan.rs:380-408` returns bare names) to emit path-qualified identities; update all tag queries and `events.rs` payloads to carry identity + display name + display path.

### A-3: `media` table missing required columns/constraints — **Critical**
- **Law:** 02 §4 required columns: relative path, canonical path identity, `date_added` semantics, `thumbnail_cache_key`, thumbnail stale/failure status, `last_accessed_at`; unique `media(source_root_id, relative_path)`; unique `media.canonical_identity`; indexes on `date_added`, `size_bytes`, `media_type`, `last_accessed_at`, `(source_root_id, scan_generation)`.
- **Violation:** `src/db/schema.rs:43-58` — media stores absolute `path` (UNIQUE) only; no relative_path, no canonical_identity, no thumbnail_cache_key/stale/failure, no last_accessed_at; `indexed_at` exists but is not exposed as "Date added" anywhere.
- **Fix:** migration adding the required columns/uniques/indexes; store relative paths per root; store canonical target path for regular files and resolved target for file symlinks (enables I-2).

### A-4: Missing `scan_errors`, `settings`, `session_state` tables — **High**
- **Law:** 02 §4 required tables list.
- **Violation:** none of the three exist (`src/db/schema.rs:34-79`). Scan errors live only in a transient UI `Vec` (`src/ui/header.rs:35`); settings/session live in `state.json`.
- **Fix:** create the tables via migrations; persist scan errors keyed by `(source_root_id, scan_generation, path)` with category/message/last-seen; a later successful scan of the path clears the error (04 §12).

### A-5: Session/settings persisted in `state.json`, not SQLite — **High**
- **Law:** 02 §4 "global settings, including ignore rules and root-as-tag; session state, including filters, sort, zoom, scroll anchor, and window size" are SQLite responsibilities.
- **Violation:** `src/state.rs:71-98` reads/writes `~/.config/vesper/state.json`.
- **Fix:** after A-4, move `AppState` persistence into `settings`/`session_state` tables; keep a one-time import from `state.json`.

### A-6: Scroll position stored as raw item index, not stable anchor — **Medium**
- **Law:** 02 §8 "Persist scroll position as a stable anchor: `anchor_media_id/path`, `anchor_offset_within_cell`, sort/filter context hash."
- **Violation:** `src/state.rs:11` — `scroll_position: u32` item index.
- **Fix:** persist the anchor triple; restore window size/zoom/sort/filters first, then resolve the anchor (depends on A-5).

### A-7: Suspended/discarded offline tag filters not implemented — **Medium**
- **Law:** 02 §8: persisted filter whose root was removed is discarded; whose root is offline is suspended + hidden and restored after rescan; status surface explains it.
- **Violation:** `active_tags: Vec<String>` (`src/state.rs:8`) are plain names with no root linkage; no suspension logic exists.
- **Fix:** after A-2 filters reference tag identities; add suspend/discard reconciliation at hydration and the status text.

---

## B — Runtime, backend, concurrency

### B-1: No single-instance library lock — **Critical**
- **Law:** 01 §4 / 02 §5 "The app acquires a library lock before opening the database for write access… Two write-capable instances must never use the same library state simultaneously."
- **Violation:** `src/main.rs:58-65` opens the DB directly. (GTK application-id gives some activation dedup, but nothing guards the DB/state files.)
- **Fix:** acquire an exclusive lock file next to the DB before opening it for write; second instance activates the existing window or exits with a clear non-blocking message.

### B-2: FetchData decoupling + generations (ARCH-001/002/004, ARCH-003) — **High**
See carried-over items above. One combined workstream:
1. Background liveness/watcher worker that updates the DB independently (also fixes B-4).
2. Pure read-only hydration query, generation-tagged, delivered in bounded chunks (02 §5 "publication order", 03 §9).
3. Query generations on search/filter/sort; UI ignores stale generations.
4. Live deltas evaluated against the active query or superseding refresh.
5. Structured error events for failed scans (no silent `if let Ok`: `src/backend/app_loop.rs:171,207`).

### B-3: No stability check / bounded retry for changed files — **High**
- **Law:** 02 §6 "Read metadata twice 250ms apart… if size or modified time changes, defer probing"; retries at 1s/5s/30s; do not publish unstable records; scanner-level temp extensions (`.crdownload`, `.partial`, `~`, `.swp`) never produce records or errors.
- **Violation:** `src/backend/live_update.rs:107-126` reads metadata once and publishes immediately; no temp-extension filter beyond the user-visible default ignore list.
- **Fix:** in the live-update path, double-read metadata 250ms apart with bounded backoff before upsert; add the scanner-level transient-extension list to `index/media.rs` classification or a pre-filter.

### B-4: Deletions processed regardless of root online state — **High**
- **Law:** 02 §5 change-event table: "Deleted file → Remove only if the source root is online and the file is confirmed gone"; 01 §4 "Source-root disappearance is treated as offline, not deletion."
- **Violation:** `src/backend/watcher.rs:21-25` classifies any non-existing path as Deleted; `src/backend/live_update.rs:25-45` removes records unconditionally. An unmounted root emitting remove events mass-deletes library records.
- **Fix:** before removal, verify the owning source root is still online (probe root dir); if the root is gone, mark it offline and preserve records.

### B-5: Watcher debounce is 500ms, spec says 300ms — **Low**
- **Law:** 02 §1 "app-wide debounce constant in `src/config.rs` (v1 default: 300ms)."
- **Violation:** `src/config.rs:5` — `FS_DEBOUNCE_MS: u64 = 500`.
- **Fix:** set to 300.

### B-6: No maintenance operations (Rescan is the only one; no Regenerate/Rebuild, no mutual exclusion) — **High**
- **Law:** 02 §5 maintenance ops: Rescan Library, Regenerate Thumbnails, Rebuild Library Index; only one index-mutating maintenance op at a time with a passive "already running" status. 04 §24 buttons.
- **Violation:** only `AppEvent::RescanRoots` exists (`src/events.rs`, `src/backend/app_loop.rs:153`); no regenerate/rebuild events, no exclusion guard.
- **Fix:** add typed events + backend jobs for the three ops, guarded by a single maintenance mutex; surface status via the banner stack. (Regenerate depends on T-river; Rebuild depends on A-1/A-4.)

### B-7: Concurrency/bounding rules not implemented — **Medium**
- **Law:** 02 §5: one active full-root scan at a time; probe/thumbnail concurrency `min(4, parallelism)`; UI query priority over thumbnails; jobs carry root id + generation and are cancelled on root removal / settings change.
- **Violation:** `RescanRoots` scans roots serially inline (OK) but `RescanSubtree` spawns unbounded parallel tasks (`src/backend/app_loop.rs:206`); nothing cancels in-flight scans when a root is removed (`RemoveSourceRoot` just deletes rows); thumbnail worker has no priority interaction with queries.
- **Fix:** bounded job queues keyed by root + generation; on root removal bump generation and drop stale results (generation exists per scan but removal doesn't invalidate running walkers).

### B-8: No logging/diagnostics subsystem — **Low**
- **Law:** 02 §7: local log files, rotation (10 MB ×3), lifecycle/scan/availability/migration events, no full paths at info level.
- **Violation:** everything is `eprintln!` (throughout backend).
- **Fix:** add `tracing` + rolling file appender in the user state dir honoring the spec's rotation and path-privacy rules.

---

## I — Indexing policy (walker, symlinks, roots, ignore rules)

### I-1: Directory symlinks are followed (one level) — **Critical**
- **Law:** 01 §4 / 02 §1 "Directory symlinks are not followed in v1."
- **Violation:** `src/index/walker.rs:22` `MAX_SYMLINK_DEPTH: u8 = 1` and traversal at `walker.rs:161-214`; the file even cites an outdated spec ("followed one level deep per spec section 4").
- **Fix:** skip any directory entry whose `file_type().is_symlink()`; delete the depth machinery; fix the stale doc comments.

### I-2: File symlink boundary + canonical duplicate prevention missing — **Critical**
- **Law:** 02 §1 "File symlinks may be indexed only if they resolve to a supported media file inside an allowed source-root boundary"; symlinks resolving outside all roots or duplicating an indexed canonical file are skipped. 02 §4 unique `canonical_identity`. 02 §5 canonical conflict reconciliation.
- **Violation:** `src/index/walker.rs:170-233` indexes file symlinks by their link path with no boundary check and no canonical dedup — a symlink and its target both get rows.
- **Fix:** after A-3 adds `canonical_identity`: resolve file symlinks, reject targets outside all source roots, and upsert with canonical-identity conflict handling per the reconciliation rules.

### I-3: Overlapping/duplicate/nested source roots are not rejected — **Critical**
- **Law:** 01 §4 / 02 §1 "A root is rejected if its canonical path duplicates an existing root, is inside an existing root, or contains an existing root." 04 §24 message: "This folder is already covered by an existing source directory."
- **Violation:** `src/backend/app_loop.rs:38-77` canonicalizes then inserts; only exact-path UNIQUE collision fails, nested/containing roots are accepted → duplicate indexing.
- **Fix:** in `AddSourceRoot`, compare the canonical path against all existing canonical roots (prefix in both directions) and reject with the specified non-blocking message.

### I-4: Root path rejected-at-add validation incomplete — **Medium**
- **Law:** 02 §1 "A newly selected path that does not exist, is not a directory, cannot be read, or cannot be canonicalized is rejected with a recoverable Settings error; it is not stored as an offline root."
- **Violation:** `src/backend/app_loop.rs:85-118` inserts the root, then scans, then rolls back on scan failure — a transient scan error deletes the root, and the row is briefly visible.
- **Fix:** validate exists/is_dir/read_dir before insert (fast, root dir only); keep the scan-failure path but stop treating scan failure as "root invalid."

### I-5: Ignore-rule semantics diverge from spec — **Medium**
- **Law:** 02 §2: global rules evaluated first, then `.galleryignore` from root down, **last matching rule wins** across the combined list; invalid patterns do not partially apply and Settings identifies the invalid line.
- **Violation:** `src/index/ignore_rules.rs` + `walker.rs:194,216` evaluate local stack and global rules as separate `Gitignore` objects (`is_ignored(path, …, ignore_stack, global_rules)`), so precedence between a global rule and a local negation cannot follow one last-match-wins list; there is no validation feedback path to Settings (see U-5).
- **Fix:** build one effective matcher list per directory (global first, then stacked locals) with unified last-match evaluation; return per-line validation errors to the Settings dialog.

### I-6: Scan generation is not root-scoped protection against cancelled sweeps — **Medium**
- **Law:** 02 §5 "A canceled, failed, or offline scan must never perform that deletion sweep"; per-root generation, stale results ignored.
- **Violation:** `src/scan.rs:149-157` runs `remove_stale_media` whenever the walker returns Ok, but a root that goes offline mid-scan yields partial discovery… `walk_directory` returns Ok after per-directory read errors (`walker.rs:100-121`), so a partially-readable root triggers a sweep that deletes records for unreadable subtrees.
- **Fix:** track fatal-vs-partial: if any directory read error occurred (or the root went offline), skip the reconciliation sweep for that generation.

---

## T — Thumbnails and cache

### T-1: No cache-key addressing, stale flag, or explicit-regeneration flow — **High**
- **Law:** 01 §4 / 02 §4: cache files addressed by `thumbnail_cache_key` + variant; modified files set `thumbnail_stale=true` and keep the old thumbnail until explicit regeneration succeeds; failures store a status for a stable placeholder.
- **Violation:** schema has only `thumbnail_path` (`src/db/schema.rs:52`); `src/thumbnail.rs` writes files and updates `thumbnail_path` directly; no stale/failure status anywhere.
- **Fix:** after A-3: generate stable cache keys, add stale/failure columns, keep old key on modify, wire "Regenerate Thumbnails" (B-6) to stale/failed rows.

### T-2: No cache size limit / LRU eviction / access-time batching — **Medium**
- **Law:** 02 §4: 5 GB disk limit with LRU eviction of non-visible entries; `last_accessed_at` from thumbnail reads, batched, ≤1 write per item per 10 minutes; 256 MB / 512-entry memory cache.
- **Violation:** no eviction or accounting exists in `src/thumbnail.rs` or `db/`.
- **Fix:** track `last_accessed_at` (A-3), add a cache-maintenance job with the specified budgets.

### T-3: Cache cleanup for deleted media/removed roots — **Medium**
- **Law:** 02 §4 "cache cleanup removes entries for deleted media and removed roots."
- **Violation:** DB rows cascade-delete (`schema.rs:57`) but cache files on disk are never removed.
- **Fix:** collect cache keys before deletion and remove files in the same background job.

---

## U — UI structure and behavior

### U-1: Viewer overlay mounted inside the grid overlay, not the app overlay — **High**
- **Law:** 02 §9 widget tree: `viewer_overlay` is an overlay of the top-level `app_overlay`, covering header + sidebar; 03 §4 / §10 "Do NOT mount the viewer overlay inside the grid overlay."
- **Violation:** `src/ui/window.rs:774-786` adds `viewer.dim_bg`/`viewer.overlay` to `main_overlay` inside the grid column (`grid_toolbar_view` at `window.rs:799-807` holds the stack).
- **Fix:** create `app_overlay` wrapping `main_box` under the `ApplicationWindow`; move viewer mounting there; keep `action_bar_revealer` and `scan_error_button` grid-scoped.

### U-2: Header bar violates the required composition — **High**
- **Law:** 03 §2 / 05 §5: `adw::WindowTitle "Vesper"` at start; search centered in an `adw::Clamp` title widget (280→360px); neutral labeled `Clear filters (N)` (never pill/suggested-action); 96px five-detent zoom slider with no flanking zoom icons, accessible XS–XL value text; Sort as labeled menu button with disclosure arrow (never ellipsis icon); size+Sort not `.linked`; pack `controls_group` once.
- **Violation:** `src/ui/header.rs:38-145` — pill `suggested-action` filter button (:38-43), search packed with `pack_end` not centered (:145), sort uses `view-more-symbolic` ellipsis icon (:94-100), zoom slider 120px continuous 0.0–4.0 with zoom-out/zoom-in icons (:104-124), zoom+sort wrapped in `.linked` (:134-140), multiple `pack_end` calls (:142-145), no `WindowTitle`.
- **Fix:** rebuild header per 03 §2; five-detent slider (`set_round_digits(0)`, marks without labels, accessible value text); label `Clear filters (N)` where N counts tags + 1 for search.

### U-3: Sort model uses "Date created" instead of "Date added" — **High**
- **Law:** 04 §11 sort options include Date added; 01 §4 "must not expose filesystem birth time as a guaranteed `Date created` field on Linux."
- **Violation:** `src/ui/header.rs:54-63` lists "Date created (…)"; `src/db/search.rs:105-106` sorts by `m.created_at` (birth time); `src/events.rs` SortOrder has DateCreated variants.
- **Fix:** rename options/variants to Date added and sort by `date_added` (= `indexed_at` semantics per A-3); drop `created_at` from user-facing surfaces (also viewer info panel, U-9).

### U-4: Sidebar tag rows: pills, no lineage disambiguation, wrong sort tie-break — **High**
- **Law:** 02 §3 tags sorted by count desc → case-insensitive name → path identity; duplicate display names must be disambiguated with lineage (secondary text/tooltip); 03 §1 / 05 §5 flat rows with 3px accent indicator, trailing count, never chips/pills; batch reorder only at batch boundaries.
- **Violation:** `src/ui/window.rs:405-430` sorts by count only, renders `"name (count)"` single labels with `tag-chip` CSS class, no lineage, and rebuilds the whole list per TagsUpdated event.
- **Fix:** after A-2: rows with display name + trailing count label, secondary lineage text only on collision, `.tag-row` styling per 03 §3, full tie-break sort, batch updates.

### U-5: Settings dialog missing required groups and required apply-flow — **High**
- **Law:** 03 §5 / 04 §24: Ignore Rules group has "Restore Default Ignore Rules" and "Apply Ignore Rules" (validate → save → rescan; dirty-gated; closing discards unapplied edits); Library Maintenance group with Rescan/Regenerate/Rebuild; root removal shows confirmation with "Files on disk will not be changed."
- **Violation:** `src/ui/settings.rs` — ignore rules are saved on **every keystroke** with no validation (`:174-189`); no Restore Defaults, no Apply button, no Maintenance group; remove button deletes immediately with no confirmation (`:86-89`); closing settings always fires a full `RescanRoots` (`:28-36`).
- **Fix:** implement the 03 §5 layout: buffer edits locally, validate on Apply (with first-invalid-line inline error, needs I-5), rescan only on successful apply; add confirmation dialog for removal; add maintenance buttons wired to B-6.

### U-6: Selection bar "Open Location" not disabled for multi-folder selections — **Medium**
- **Law:** 04 §9/§19: disabled when selection spans >1 physical folder, with tooltip "Selected files must reside in the same folder."
- **Violation:** `src/ui/selection_bar.rs:74-89` always enabled; opens the first item's parent.
- **Fix:** on selection change compute distinct parent count; `set_sensitive(false)` + tooltip when >1.

### U-7: Viewer zoom: no 800% max, wrong step (15% vs 12.5%) — **Medium**
- **Law:** 04 §6: max zoom 800%; zoom changes in 12.5% relative steps.
- **Violation:** `src/ui/viewer.rs:913` `zoom_step = 1.15`, no upper clamp in `zoom_to_internal`.
- **Fix:** `zoom_step = 1.125`; clamp `final_zoom` to `8.0` (of 1:1) and floor at fit.

### U-8: Info panel floats over media instead of pushing it — **Medium**
- **Law:** 04 §8 / 05 §8: "The info panel pushes the media layout. The media area shrinks… preventing any overlap"; opaque side panel with leading border.
- **Violation:** `src/ui/viewer.rs:297` adds the panel as an overlay on the media area.
- **Fix:** restructure viewer content as horizontal Box (media area + `gtk::Revealer` SlideLeft panel).

### U-9: Info panel shows "Created", no "Date added" — **Medium**
- **Law:** 04 §8 info fields: filename, path (selectable, middle-ellipsis, copy affordance), size, dimensions/duration, **date added**, modified date, tags.
- **Violation:** `src/ui/viewer.rs:288-289` shows Created + Modified.
- **Fix:** with U-3/A-3: replace Created with Date added; add path copy affordance.

### U-10: No wrap-around edge cue in viewer navigation — **Low**
- **Law:** 04 §8 "Wrapping uses a brief 120ms opacity-only edge cue in the navigation direction; never flash or scale the whole viewer."
- **Violation:** `src/ui/viewer.rs:587-609` wraps silently.
- **Fix:** 120ms opacity pulse on the edge chevron region when the index wraps.

### U-11: Closing viewer doesn't highlight origin cell — **Low**
- **Law:** 04 §8 "On close, the grid… highlights that cell for 900ms."
- **Violation:** `src/ui/window.rs:556-564` scrolls + focuses only.
- **Fix:** add a temporary CSS class to the origin cell, remove after 900ms.

### U-12: Search contract gaps: ranking tiers, tie-breaker, normalization — **Medium**
- **Law:** 04 §1 eight-tier ranking (exact basename → basename prefix → basename substring → exact tag → tag substring → path substring → current sort → full path asc); search is Unicode-normalized; 02 §4 "full path as the final tie-breaker."
- **Violation:** `src/db/search.rs:102-129` implements 3 tiers (exact-filename / tag-LIKE / else) and uses `m.id` as tie-breaker; SQLite `LIKE` is ASCII-case-insensitive only, no Unicode normalization/case-folding.
- **Fix:** rank with the full 8-tier CASE (basename-without-extension column or computed), tie-break on `m.path`; store a normalized (NFC + casefold) filename/path/tag column at index time and normalize the query in Rust before binding.

### U-13: Status surfaces incomplete (banner priority, scan error popover paths) — **Low**
- **Law:** 02 §9/§10 status priority (recoverable critical > offline > indexing); 04 §12 scan-error indicator opens a popover listing affected paths; counts update ≤10×/s.
- **Violation:** `src/ui/window.rs:107,799-807` has offline banner + scan indicator banner but no priority stack (both can show); verify popover content against 04 §12 wording ("N files could not be indexed.").
- **Fix:** single `gtk::Stack` for banners with the priority rule; keep `scan_error_button` independent.

### U-14: First-launch/empty-state contract not fully met — **Low**
- **Law:** 04 §13/§20: disabled search/sort/zoom retain accessible explanations; "Press F1 or Ctrl+? for keyboard shortcuts" hint; sidebar shows "No tags available" + empty sources.
- **Violation:** header controls use `visible(false)` (`src/ui/header.rs:99,119`) instead of disabled-with-explanation.
- **Fix:** use `set_sensitive(false)` + tooltip/accessible description instead of hiding; add the shortcut hint line.

---

## V — Visual design (05 §10 removal list is the law; all verified present 2026-07-15)

### V-1: Custom accent override — **High**
`src/ui/style.css:1-2` defines `@define-color accent_color #5a6b8c`. Law: 05 §2 "Do not redefine accent_color or accent_bg_color." Fix: delete; inherit system accent.

### V-2: Hard-coded dark surfaces — **High**
`style.css:236,241` use `#242424`/`#181818`. Law: 05 §2. Fix: replace with `@view_bg_color`/named colors; viewer scrim per 03 §3 (`rgba(0,0,0,0.92)` is allowed — it's the specified scrim, not a surface).

### V-3: Shimmer animation — **Medium**
`style.css:112-120` `@keyframes shimmer`. Law: 05 §6 / 03 §3 "no shimmer." Fix: stable neutral placeholder; native spinner only after 400ms decode wait.

### V-4: `transition: all` — **Medium**
`style.css:158,216`. Law: 05 §9 / 03 §3. Fix: transition `opacity` (and specific properties) only, 120ms standard.

### V-5: Missing `.selected-tint` overlay; selection styling — **Medium**
Law: 03 §3 grid cell templates include `.selected-tint` above image, below badges; selection never lowers picture opacity; tint ≤12% black. Violation: no `.selected-tint` in `src/ui/grid_cell.rs:24-91` or `style.css`; active tag/selected styles use solid `@accent_bg_color` fills (`style.css:22,175,223`). Fix: add the tint child widget + CSS; replace solid accent fills with 05 §5 treatments (3px indicator + `alpha(@accent_color,0.14)` row background).

### V-6: Pill/chip components where flat rows/neutral buttons are required — **Medium**
Tag rows use `tag-chip` (`src/ui/window.rs:426`), filter button is `pill suggested-action` (`src/ui/header.rs:39`), scan-error button is `osd pill` (header.rs:28). Law: 05 §10 removal list; 03 §3 `.filter-pill` must not exist as pill styling — spec's `.tag-row` model applies. Fix: covered by U-2/U-4; sweep CSS for remaining pill classes.

### V-7: Zoom icons around slider / ellipsis Sort icon — **Medium**
`src/ui/header.rs:122-124` zoom icons; `:96` ellipsis. Law: 05 §10. Fix: covered by U-2.

---

## Rivers — required fix order

Fix each river top-to-bottom; a downstream issue must not be attempted before its upstream is merged. Rivers are largely independent of each other except where noted.

### River 1 — Schema foundation (everything flows from here)
```
A-1 migrations runner + schema_migrations
 └─→ A-2 path-qualified tags
 └─→ A-3 media columns (relative_path, canonical_identity, date_added,
      thumbnail_cache_key/stale/failure, last_accessed_at, indexes)
 └─→ A-4 scan_errors / settings / session_state tables
       └─→ A-5 move state.json → SQLite
             └─→ A-6 stable scroll anchor
             └─→ A-7 suspended/discarded offline filters (also needs A-2)
```

### River 2 — Backend correctness (needs A-1; chunked hydration benefits from A-3)
```
B-1 single-instance lock            (independent, do first — tiny)
B-5 debounce 300ms                  (independent, one-liner)
B-2 FetchData decoupling + query generations (ARCH-001/002/004)
 └─→ B-4 delete-only-when-root-online (needs the liveness worker from B-2)
 └─→ B-3 stability checks / temp extensions in live updates
 └─→ B-7 bounded job queues + cancellation on root removal
 └─→ ARCH-003 / structured scan-error events (store in scan_errors, needs A-4)
B-8 logging (independent, anytime)
```

### River 3 — Indexing policy (I-1/I-3/I-4 independent; I-2 needs A-3)
```
I-1 stop following directory symlinks
I-3 reject overlapping/nested roots
I-4 validate root before insert
I-5 unified last-match ignore evaluation ──→ feeds U-5 (Apply validation)
A-3 ──→ I-2 file-symlink boundary + canonical dedup
B-2 ──→ I-6 no deletion sweep after partial/failed scan
```

### River 4 — Thumbnails (needs A-3)
```
A-3 ──→ T-1 cache keys + stale/failure flags
          └─→ T-2 LRU eviction + access batching
          └─→ T-3 cache cleanup for deleted media/roots
          └─→ B-6 maintenance ops (Regenerate; Rebuild also needs A-1/A-4)
```

### River 5 — UI structure (mostly independent of data rivers, except noted)
```
U-1 viewer overlay → app_overlay        (do first: other UI work builds on the tree)
U-2 header rebuild ──→ U-3 Date added sort (needs A-3 date_added)
A-2 ──→ U-4 tag rows with lineage + tie-break sort
I-5 ──→ U-5 settings dialog (apply flow, restore defaults, confirmation, maintenance group w/ B-6)
U-6 open-location disable               (independent)
U-7 zoom clamp/step                     (independent)
U-8 info panel push ──→ U-9 date added field (needs U-3)
U-10 wrap cue, U-11 close highlight     (independent, low)
A-3 ──→ U-12 search ranking/normalization (needs basename/normalized columns)
U-13 banner priority stack, U-14 empty-state disabled controls (independent)
```

### River 6 — Visual cleanup (independent; do after U-1/U-2/U-4 to avoid restyling widgets twice)
```
V-1 accent → V-2 surfaces → V-3 shimmer → V-4 transition:all → V-5 selected-tint → V-6 pills → V-7 icons
(order within river is by review safety, not hard dependency; follow 05 §11's step plan)
```

**Suggested overall sequence:** B-1, B-5, I-1, I-3 (small, critical) → River 1 → River 2 → River 3 remainder → River 4 → River 5 → River 6.

---

# Recently Resolved (carried over)

- Full library lookup during single-file updates → targeted `get_media_with_tags_by_path`.
- Scan/delete race reintroducing deleted files → batches validate existence before upsert.
- Viewer sorted/unsorted index mismatch; Selection sorted/unsorted index mismatch; Settings state clobbering; Search contract violations (partial — see U-12 for remaining gaps); Offline media hidden from UI; Broken subtree tag cleanup; Scroll restoration calculation.
