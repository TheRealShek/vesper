# Vesper: Future Ideas and Research

This is a post-v1 research backlog, not a second product specification. An item becomes committed only after it is promoted into the numbered canonical documents. Future work must preserve Vesper's read-only filesystem contract, folder-derived tag model, single-library model, and media-first visual language in [05_Visual_Design.md](05_Visual_Design.md).

---

## 1. How to Evaluate an Idea

Before promotion, record:

1. **User problem:** a concrete recurring task, not “more features.”
2. **UI cost:** what permanent chrome, modes, settings, and shortcuts it adds.
3. **Backend cost:** schema, indexing, cache, worker, and migration impact.
4. **Scale behavior:** expected cost at 50,000 files and while roots are offline.
5. **Accessibility:** keyboard, screen reader, large text, reduced motion, and high contrast.
6. **Exit condition:** a prototype result or measurable threshold that justifies keeping it.

Prefer changes that improve retrieval, viewing, resilience, or performance without creating another navigation system.

### Already required in v1

The following old “future” ideas are now canonical requirements and must not be deferred: tag counts, viewer loading/error states, grouped essential metadata, keyboard multi-selection, grid-cell accessibility labels, high-contrast focus, bounded virtualized loading, and stale-thumbnail behavior.

---

## 2. Near-Term Polish Candidates

These fit the current product model and can be considered after v1 acceptance passes.

### 2.1 Result context in the header

- **Problem:** after several filters, the user cannot tell whether the result is narrow or broad without scrolling.
- **Direction:** show quiet text such as `47 of 12,480` near Clear filters. Hide it when no media exists; do not turn it into a badge.
- **Backend:** return filtered and online-total counts with the active query generation, not as a second unversioned query.
- **Success:** adds no measurable query latency and remains readable at 200% text scale.

### 2.2 Viewer filename and position

- **Problem:** arrow-key navigation lacks location context.
- **Direction:** a single bottom-left OSD line containing truncated filename and `3 of 42`; reveal on viewer open/navigation and on pointer movement, then hide after a calm delay.
- **Constraints:** no permanent top gradient, no duplicated metadata panel, and screen readers announce navigation only once.
- **Success:** users can identify position without opening Info and the overlay never covers video controls.

### 2.3 Density presets

- **Problem:** five thumbnail sizes may still feel too spacious or tight on unusual displays.
- **Direction:** add a Settings choice for Comfortable/Compact grid spacing while preserving the five size detents.
- **Risk:** additional combinations complicate performance and visual testing.
- **Success:** both presets pass scrolling budgets at all five sizes and preserve focus-ring clearance.

### 2.4 Sidebar match clarity

- **Problem:** `Any` versus `All` may be unclear to occasional users.
- **Direction:** test concise explanatory text (`Match any selected tag` / `Match all selected tags`) in a tooltip or status line rather than adding permanent help icons.
- **Success:** usability testing shows fewer mistaken empty-result states without increasing sidebar height materially.

---

## 3. Filtering and Discovery

### 3.1 Excluded tag filters

- **Value:** find media in one folder lineage but not another.
- **Interaction:** each selected tag has Include/Exclude state; exclude is visually distinct without relying on red alone.
- **Data/query:** `included_tags` retain Any/All semantics, then `excluded_tags` remove matches. Persist stable path-qualified tag identities.
- **Risk:** a three-state tag row is harder to learn and use by keyboard.
- **Promotion gate:** prototype a discoverable interaction with no context menu dependency.

### 3.2 Untagged media filter

- **Value:** find files directly under a source root when root-as-tag is off.
- **Interaction:** one synthetic `Untagged` filter row, clearly separated from folder-derived tags so it is not mistaken for a real folder.
- **Data/query:** query media with no `media_tags` rows among online roots.
- **Promotion gate:** counts and Any/All behavior are specified for combinations with real tags.

### 3.3 Saved views

- **Value:** return to a frequently used combination of tags, match mode, search, and sort.
- **Direction:** user-named saved views in a compact popover, not a new permanent sidebar section.
- **Risks:** stale tags after folder moves, search persistence, naming/management UI, and conflict with v1 simplicity.
- **Promotion gate:** define missing/offline-tag behavior and cap management complexity before schema work.

### 3.4 Recently added view

- **Value:** inspect what a recent scan introduced without EXIF or filesystem birth time.
- **Direction:** predefined Date-added ranges (Today, 7 days, 30 days) inside a filter popover.
- **Constraints:** use reliable `date_added`; do not add a timeline/calendar or persistent `Recent` sidebar section.
- **Promotion gate:** prove the view is useful beyond sorting Date added newest-first.

### 3.5 Source-scoped results

- **Value:** temporarily focus one configured source root when several contain similar folder structures.
- **Direction:** source rows may expose a `Show only this source` action while remaining status rows by default.
- **Risk:** source filtering becomes a second tag system and can make offline/filter state confusing.
- **Promotion gate:** define one obvious clearing mechanism and interaction with saved/tag filters.

---

## 4. Viewer and Media Experience

### 4.1 Adjacent-media prefetch

- **Value:** faster arrow navigation through large images and short clips.
- **Direction:** prefetch metadata and a bounded preview for previous/next snapshot items after the current item becomes ready.
- **Constraints:** current media and UI queries always outrank prefetch; cancel on rapid navigation or viewer close.
- **Success:** p95 next-image display improves without exceeding cache/memory limits.

### 4.2 Per-item zoom memory

- **Value:** compare details across a short navigation sequence.
- **Direction:** retain zoom/pan in viewer memory for recently visited items only; clear when viewer closes.
- **Risk:** surprising orientation when users expect every item to fit.
- **Promotion gate:** test an explicit `Remember zoom while viewer is open` behavior versus always-fit.

### 4.3 Video keyboard controls

- **Value:** efficient playback without pointer travel.
- **Direction:** `J/L` or `Shift+Left/Right` seek backward/forward, `M` mute, and optional frame-step while paused where the backend supports it.
- **Constraints:** shortcuts must not collide with global/viewer navigation and must appear in shortcut help.
- **Success:** seek feedback is visible and corrupt/unsupported streams remain recoverable.

### 4.4 Subtitles and audio tracks

- **Value:** make local videos with multiple tracks usable.
- **Direction:** native-looking track popover in video controls, populated from the playback backend.
- **Risks:** GStreamer backend capability varies; track switching may not be exposed consistently by GTK's high-level media API.
- **Promotion gate:** technical spike confirms reliable enumeration/switching on supported package baseline.

### 4.5 Color-managed images

- **Value:** accurate photography display.
- **Direction:** honor embedded ICC profiles and convert to the display color space without indexing EXIF metadata.
- **Risks:** decode cost, display-profile availability on Wayland, memory use, and inconsistent format support.
- **Success:** reference-image tests show correct conversion with no viewer responsiveness regression.

### 4.6 HDR and wide-gamut video research

- **Value:** correct playback on modern displays.
- **Direction:** investigate GTK/GStreamer/Wayland color-pipeline capabilities before promising UI.
- **Constraint:** no fake HDR toggle and no claim of support until end-to-end metadata and output are verified.

### 4.7 Animated image support

- **Value:** optionally play GIF/WebP animation in the viewer while the grid remains static.
- **Risks:** memory, CPU, seek behavior, and conflict with v1's explicit static-GIF constraint.
- **Promotion gate:** requires a Product/Vision scope revision and a pause control that respects reduced motion.

### 4.8 Compare mode

- **Value:** inspect two images side by side without editing them.
- **Direction:** enter from exactly two selected images; synchronized zoom is optional and off by default.
- **Risks:** creates a new viewer mode, complicates keyboard/focus behavior, and may be poor on small windows.
- **Promotion gate:** demonstrate a simple exit path and no persistence/new organizational model.

---

## 5. Performance and Scale

### 5.1 SQLite FTS5 trigram search

- **Goal:** preserve substring ranking under 150ms as filenames/tags grow.
- **Approach:** benchmark FTS5 trigram against indexed `LIKE`/normalized helper columns using realistic Unicode/path data.
- **Risks:** SQLite build options, index size, migration/rebuild cost, and exact ranking parity.
- **Promotion gate:** adopt only if p95 improves materially at 50k/200k fixtures without ranking regressions.

### 5.2 Multiple thumbnail variants

- **Goal:** avoid decoding 256px thumbnails for tiny cells and improve sharpness at XL/high scale factors.
- **Approach:** cache small/standard/large variants selected by cell pixel size and monitor scale.
- **Risks:** cache growth and regeneration cost.
- **Promotion gate:** define per-variant keying/LRU and keep the total default disk budget understandable.

### 5.3 Velocity-aware prefetch

- **Goal:** reduce placeholders during fast scroll without evicting useful visible work.
- **Approach:** expand the near-visible window in the direction of travel, then shrink it when scrolling stops.
- **Constraints:** bounded request count; cancel stale direction; never starve UI queries.
- **Success:** fewer placeholder frames in a repeatable scroll trace with no input-frame regression.

### 5.4 Storage-aware scheduling

- **Goal:** avoid disk thrash on HDDs/removable drives while using SSD capacity effectively.
- **Approach:** infer behavior from measured latency/queue depth rather than asking users to classify drives.
- **Risk:** unstable heuristics and false assumptions about network mounts.
- **Promotion gate:** show improvement across representative SSD/HDD/removable fixtures.

### 5.5 Automated performance regression suite

- **Goal:** enforce the Product budgets continuously.
- **Approach:** deterministic 10k/50k synthetic libraries, recorded interaction traces, GTK-frame timing, query-generation assertions, and reportable p50/p95.
- **Value:** prevents visual or backend polish from silently reintroducing whole-model reloads and UI stalls.

### 5.6 Cache health tooling

- **Goal:** diagnose space use and corrupt/missing thumbnail records.
- **Direction:** Settings shows cache size and last cleanup; offer non-blocking `Clear thumbnail cache` followed by demand regeneration.
- **Constraint:** clearing cache never removes media/index records and must explain the temporary placeholder impact.

---

## 6. Accessibility and Input

### 6.1 Rich screen-reader position announcements

- Announce filename, media type, selection state, and `item X of Y` without reading full paths by default.
- Coalesce announcements during rapid arrow navigation.
- Validate with Orca against virtualized/recycled grid cells.

### 6.2 Large-text adaptive header

- Reflow or move infrequent controls into one labeled menu at very large text scales while keeping Search and Settings reachable.
- This is accessibility adaptation, not a general responsive sidebar collapse.
- Test at 200% and 300% text scaling before promotion.

### 6.3 User-configurable seek and navigation increments

- Allow a small set of video seek-step choices only if default keyboard seeking ships.
- Keep grid navigation deterministic; do not add a general shortcut editor in the first iteration.

### 6.4 Touchpad gestures

- Research pinch-to-zoom and two-finger viewer pan with mouse/keyboard parity.
- Avoid global swipe gestures that conflict with desktop/workspace navigation.
- Respect reduced motion and make gestures additive, never required.

---

## 7. Desktop Integration and Diagnostics

### 7.1 Codec/dependency health page

- Show availability of `ffmpeg`, `ffprobe`, GTK playback backend, and common codec support in Settings/About.
- Provide actionable package-oriented text without making startup fatal.
- Do not expose raw command output or pretend a probe guarantees every file will play.

### 7.2 MPRIS video integration

- Expose current video play/pause/position to GNOME media controls while the viewer is open.
- Clear state immediately when navigating to an image or closing the viewer.
- Evaluate privacy implications of exposing filenames as track titles; default to application name unless explicitly justified.

### 7.3 Long-operation completion notification

- Optionally notify when a user-initiated initial scan, rebuild, or regeneration completes while Vesper is unfocused.
- Never notify for routine watcher updates.
- Provide a single opt-out setting only if notifications prove useful.

### 7.4 Redacted diagnostic export

- Produce a local archive containing versions, aggregate timings, error categories, and logs with media paths removed or hashed.
- Preview exactly what will be exported; Vesper still performs no upload.
- Keep this separate from normal user flows and telemetry.

### 7.5 File-manager integration

- Research a context action that opens an already-configured folder/path in Vesper's current library.
- Reject any design that silently adds arbitrary roots, creates temporary libraries, or implies file management.

---

## 8. Ideas Requiring Explicit Scope Revision

These are not normal backlog items. They contradict or materially expand the current Vision and require an explicit product decision before design or implementation:

- manual tags, ratings, stars, or albums;
- file delete, rename, move, edit, export, or sharing;
- content duplicate detection;
- face/object recognition or AI-generated tags;
- EXIF browsing, GPS maps, or calendar/timeline navigation;
- cloud/remote storage and synchronization;
- multiple libraries/profiles;
- slideshow, print, plugins, or mobile/cross-platform clients;
- permanent Recent/Folders sidebar sections or a collapsible v1 sidebar;
- directory-symlink traversal;
- automatic regeneration that discards the last successful thumbnail for a modified file.

Do not implement these under the label of “polish.”
