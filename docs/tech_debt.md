# Technical Debt & Architecture Backlog

This document tracks known architecture debt, non-critical issues, future refactors, and design concerns discovered during audits.

## Rules

* Do not add speculative issues.
* Every entry must reference a completed investigation or audit.
* Include severity and rationale.
* Remove items once implemented.
* Keep this document concise.

---

# Open Items

## ARCH-001: Event Generations for UI Synchronization
Status: Open
Severity: Medium
Category: Architecture Debt
Discovered By: Live Update Audit (2026-06)
Background:
The UI currently processes:
* DataFetched
* QueryResult
* MediaAdded
* MediaRemoved
* TagsUpdated

These events can all mutate overlapping UI state. There is currently no generation ID, version number, or query token to prevent stale results from overwriting newer state.
Risk:
* Late hydration results may overwrite newer updates.
* Future synchronization bugs may be difficult to diagnose.
Decision:
Do not work on this until a reproducible user-visible bug or measurable performance issue exists.
Potential Future Direction:
Introduce generation IDs or query tokens and ignore stale UI updates.

## ARCH-002: Query Results vs Live Updates
Status: Open
Severity: Medium
Category: Architecture Debt
Discovered By: Live Update Audit (2026-06)
Background:
Live MediaAdded and MediaRemoved events operate independently from backend query/filter results. When searches or filters are active, live updates may not be coordinated with the current query state.
Risk:
* Potential ordering inconsistencies.
* Potential divergence between query results and live updates.
Decision:
Do not work on this until a reproducible user-visible bug or measurable performance issue exists.
Potential Future Direction:
Refresh active query state after live updates or introduce query-aware delta handling.

## ARCH-003: Subtree Scan Error Reporting
Status: Open
Severity: Low
Category: Robustness
Discovered By: Live Update Audit (2026-06)
Background:
Subtree scan failures are not surfaced clearly to the UI.
Risk:
* Failures may be harder to diagnose.
Reason Not Fixed Yet:
Does not affect correctness.
Potential Future Direction:
Emit user-visible warning or structured backend error event.

---

# Future Architecture Refactors

## ARCH-004: Overloaded FetchData Event
Status: Open
Severity: High
Category: Architecture Debt
Discovered By: FetchData Architecture Audit (2026-06)
Background:
Currently, `AppEvent::FetchData` handles UI hydration, but it is heavily overloaded with side-effects: synchronous filesystem liveness probing, `notify` watcher configuration, database mutation, and startup scan sequencing. Furthermore, it drops concurrent fetches and forces a complete, unversioned reload of the GTK `ListStore`.
Risk:
* **Scalability:** Synchronous `fs::read_dir` inside the async loop blocks backend processing (potential deadlock if a drive hangs). Reloading the entire 50k media library on every UI event causes unacceptable memory and rendering churn.
* **Correctness:** Dropping concurrent requests leads to missed updates. Unversioned full-store reloads can overwrite newer incremental live updates with stale data.
* **Architecture:** Tightly coupling UI read operations to filesystem reconciliation prevents safe caching and optimization.
Reason Not Fixed Yet:
Requires fundamental decoupling of the Liveness/Watcher subsystems from the UI hydration path.
Potential Future Direction:
1. Extract liveness probing and watcher management to background workers that independently update the DB.
2. Refactor `FetchData` into a pure, read-only UI hydration query without side effects.
3. Replace coarse-grained list reloads with targeted invalidations.

---

# Recently Resolved

## RESOLVED: Full Library Lookup During Single File Updates
Resolved In: <commit>
Summary:
Replaced full get_all_media_with_tags() lookup with targeted get_media_with_tags_by_path() lookup.
Result:
Single-file live updates no longer require scanning the entire media library.

## RESOLVED: Scan/Delete Race Reintroducing Deleted Files
Resolved In: <commit>
Summary:
Pending scan batches now validate file existence immediately before upsert.
Result:
Deleted files can no longer be resurrected by stale queued scan events.

## RESOLVED: Viewer Sorted/Unsorted Index Mismatch
Resolved In: <commit>

## RESOLVED: Selection Sorted/Unsorted Index Mismatch
Resolved In: <commit>

## RESOLVED: Settings State Clobbering
Resolved In: <commit>

## RESOLVED: Search Contract Violations
Resolved In: <commit>

## RESOLVED: Offline Media Hidden From UI
Resolved In: <commit>

## RESOLVED: Broken Subtree Tag Cleanup
Resolved In: <commit>

## RESOLVED: Scroll Restoration Calculation
Resolved In: <commit>
